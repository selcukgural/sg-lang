//! The interpreter on Windows carries the manifest compiled programs carry (`src/sg.manifest`,
//! see `cc::MANIFEST`): the functions of C it calls through `extern` take and give text as UTF-8,
//! as they do in compiled code, and a path may be longer than 260 characters.
//!
//! `sg help` (choice 2082) carries the pages in `src/help/`, which `website/plain/build.mjs` writes:
//! this lists them for `src/help.rs`, so that a new page needs no line of Rust.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=src/sg.manifest");
    println!("cargo:rerun-if-changed=src/help");
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && msvc {
        let manifest = Path::new(&root).join("src").join("sg.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFESTUAC:NO");
    }
    let help = Path::new(&root).join("src").join("help");
    let mut pages: Vec<(String, String)> = std::fs::read_dir(&help)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "txt"))
        .map(|path| (path.file_stem().unwrap().to_str().unwrap().to_string(), path.to_str().unwrap().to_string()))
        .collect();
    pages.sort();
    let mut code = String::from("/// The text of each word's page, by the name of its file.\npub(crate) static PAGES: &[(&str, &str)] = &[\n");
    for (name, path) in &pages {
        code.push_str(&format!("    ({name:?}, include_str!({path:?})),\n"));
    }
    code.push_str("];\n");
    let names = help.join("names.tsv");
    code.push_str(&format!("/// A line for every name of the index.\npub(crate) static NAMES: &str = include_str!({:?});\n", names.to_str().unwrap()));
    std::fs::write(Path::new(&std::env::var("OUT_DIR").unwrap()).join("help.rs"), code).unwrap();
}
