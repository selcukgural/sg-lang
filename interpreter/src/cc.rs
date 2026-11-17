//! The C compiler a program is built with, and what it is given on each system besides the C:
//! on Linux and macOS the system's `cc` with C's math library and threads; on Windows Clang with
//! LLVM's linker, C's library linked into the program (`/MT`), a stack of 8 MB for the main thread
//! and a manifest (see `MANIFEST`). The back end's tests build with the same.

use std::path::Path;

/// Where LLVM's installer puts Clang on Windows.
#[cfg(windows)]
const LLVM_CLANG: &str = r"C:\Program Files\LLVM\bin\clang.exe";

/// The C compiler: `cc` on Linux and macOS. On Windows the one `CC` names, else `clang` on the
/// `PATH`, else the one LLVM's installer puts in its folder; none of them is an error that says
/// what to install.
pub fn compiler() -> Result<String, String> {
    #[cfg(windows)]
    {
        if let Some(cc) = std::env::var("CC").ok().filter(|cc| !cc.is_empty()) {
            return Ok(cc);
        }
        if std::process::Command::new("clang").arg("--version").output().is_ok_and(|out| out.status.success()) {
            return Ok("clang".to_string());
        }
        if Path::new(LLVM_CLANG).exists() {
            return Ok(LLVM_CLANG.to_string());
        }
        Err("no C compiler: sg builds through Clang\n  hint: install LLVM (`winget install LLVM.LLVM`) and the Build Tools of Visual Studio with the workload of C++ for the desktop (`winget install Microsoft.VisualStudio.2022.BuildTools`), or name a Clang in `CC`".to_string())
    }
    #[cfg(not(windows))]
    Ok("cc".to_string())
}

/// What the C compiler is given before the C file: on Windows LLVM's linker, and C's library
/// linked into the program, so that it needs nothing installed to run. A debug build on Windows
/// also writes what a debugger reads beside the program (`x.pdb`).
pub fn compile_flags(debug: bool) -> Vec<String> {
    let mut flags = Vec::new();
    if cfg!(windows) {
        flags.extend(["-fuse-ld=lld", "-fms-runtime-lib=static"].map(String::from));
        if debug {
            flags.push("-g".to_string());
        }
    }
    flags
}

/// The manifest every program carries on Windows: C's functions take and give text as UTF-8
/// (from Windows 10 1903), a path may be longer than 260 characters, and the program is one of
/// Windows 10 and 11.
pub const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </windowsSettings>
  </application>
</assembly>
"#;

/// What the C compiler is given after the C file to link a program: C's math library and threads
/// on Linux and macOS. On Windows the manifest, written into `work`; 8 MB of stack for the main
/// thread, as Linux and macOS give it; and the libraries a program loads are looked for only in
/// its own folder and in the system's, never in the folder it is started in.
pub fn link_flags(work: &Path) -> std::io::Result<Vec<String>> {
    if !cfg!(windows) {
        return Ok(vec!["-lm".to_string(), "-pthread".to_string()]);
    }
    let manifest = work.join("sg.manifest");
    if std::fs::read_to_string(&manifest).ok().as_deref() != Some(MANIFEST) {
        std::fs::write(&manifest, MANIFEST)?;
    }
    Ok(vec![
        "-Wl,/STACK:8388608".to_string(),
        "-Wl,/MANIFEST:EMBED".to_string(),
        format!("-Wl,/MANIFESTINPUT:{}", manifest.display()),
        "-Wl,/MANIFESTUAC:NO".to_string(),
        "-Wl,/DEPENDENTLOADFLAG:0x1000".to_string(),
    ])
}

/// The file of a program: on Windows with `.exe` when the name has no extension.
pub fn program_file(out: &str) -> String {
    if cfg!(windows) && Path::new(out).extension().is_none() {
        return format!("{out}.exe");
    }
    out.to_string()
}
