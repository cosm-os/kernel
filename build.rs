extern crate cc;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{Error, Write};
use std::path::Path;

fn scan_folder(loc: &Path) -> (HashMap<String, Vec<String>>, Vec<String>) {
    let mut folders: HashMap<String, Vec<String>> = HashMap::new();
    let mut files: Vec<String> = Vec::new();
    let mut current = Vec::new();

    if loc.is_dir() {
        for entry in fs::read_dir(loc).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let path_str = String::from(path.to_str().unwrap()).replace("\\", "/");

            current.push(path_str.clone());

            // if folder then scan recursively
            if path.is_dir() {
                let (d, mut f) = scan_folder(&path);
                for (key, value) in d.into_iter() {
                    folders.insert(key, value);
                }

                files.append(&mut f);
            } else {
                files.push(path_str);
            }
        }

        current.sort();
        folders
            .entry(String::from(loc.to_str().unwrap()).replace("\\", "/"))
            .or_insert(current);
    } else {
        panic!("{:?} is not a folder!", loc);
    }

    (folders, files)
}

fn fill_from_location(f: &mut fs::File, loc: &Path) -> Result<(), Error> {
    let (folders, mut files) = scan_folder(loc);
    let mut folder_it: Vec<_> = folders.keys().collect();

    let loc_str = loc.to_str().unwrap();
    let mut idx = loc_str.len();

    if !loc_str.ends_with("/") {
        idx += 1;
    }

    folder_it.sort();
    files.sort();
    for dir in folder_it.iter() {
        let strip: String = dir.chars().skip(idx).collect();
        write!(f, "        files.insert(b\"{}\", (b\"", strip)?;

        // Write child elements separated with \n
        let sub = folders.get(*dir).unwrap();
        let mut first = true;
        for child in sub.iter() {
            let idx = child.rfind('/').unwrap() + 1;
            let (_, c) = child.split_at(idx);
            if first {
                write!(f, "{}", c)?;
                first = false;
            } else {
                write!(f, "\\n{}", c)?;
            }
        }
        write!(f, "\", true));\n")?;
    }

    for name in files.iter() {
        let (_, strip) = name.split_at(idx);
        write!(
            f,
            "        files.insert(b\"{}\", (include_bytes!(\"{}\"), false));\n",
            strip, name
        )?;
    }

    Ok(())
}

#[cfg(not(target_arch = "x86_64"))]
fn asm(_out_dir: &str) -> () {
    ()
}

#[cfg(target_arch = "x86_64")]
fn asm(out_dir: &str) -> () {
    use std::process::Command;

    println!("cargo:rerun-if-changed=src/asm/x86_64/trampoline.asm");

    // Use `yasm` instead of `nasm`
    let status = Command::new("yasm")
        .arg("-f").arg("bin")
        .arg("-o").arg(format!("{}/trampoline", out_dir))
        .arg("src/asm/x86_64/trampoline.asm")
        .status()
        .expect("failed to run `yasm`");
    if ! status.success() {
        panic!("`yasm` failed with exit status {}", status);
    }
}

const HEAD: &[u8; 207] = b"
mod gen {
    use alloc::collections::BTreeMap;
    pub fn gen() -> BTreeMap<&'static [u8], (&'static [u8], bool)> {
        let mut files: BTreeMap<&'static [u8], (&'static [u8], bool)> = BTreeMap::new();
";

const BODY: &[u8; 22] = b"        files.clear();";

const TAIL: &[u8; 23] = b"
        files
    }
}
";

fn main() {
    println!("cargo:rustc-env=TARGET={}", env::var("TARGET").unwrap());
    println!("cargo:rerun-if-env-changed=INITFS_FOLDER");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("gen.rs");
    let mut f = fs::File::create(&dest_path).unwrap();
    let src = env::var("INITFS_FOLDER");

    asm(&out_dir);
    f.write_all(HEAD).unwrap();

    match src {
        Ok(v) => {
            println!("cargo:rerun-if-changed={}", v);
            fill_from_location(&mut f, Path::new(&v)).unwrap();
        }

        Err(e) => {
            f.write_all(BODY).unwrap();
            println!(
                "cargo:warning=location not found: {}, please set proper INITFS_FOLDER.",
                e
            );
        }
    }

    f.write_all(TAIL).unwrap();

    if cfg!(target_arch = "aarch64") {
        println!("cargo:rerun-if-changed=src/arch/aarch64/init/pre_kstart/early-init.c");
        cc::Build::new()
            .file("src/arch/aarch64/init/pre_kstart/early-init.c")
            .target("aarch64-unknown-none")
            .compile("target/early-init");
    }
}
