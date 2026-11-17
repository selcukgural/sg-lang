//! The interpreter on Windows carries the manifest compiled programs carry (`src/sg.manifest`,
//! see `cc::MANIFEST`): the functions of C it calls through `extern` take and give text as UTF-8,
//! as they do in compiled code, and a path may be longer than 260 characters.

fn main() {
    println!("cargo:rerun-if-changed=src/sg.manifest");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && msvc {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("src").join("sg.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFESTUAC:NO");
    }
}
