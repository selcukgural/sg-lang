//! The C compiler a program is built with, and what it is given on each system besides the C:
//! on Linux and macOS the one `CC` names, else the system's `cc`, with C's math library and threads; on Windows Clang with
//! LLVM's linker, C's library linked into the program (`/MT`), a stack of 8 MB for the main thread
//! and a manifest (see `MANIFEST`). The back end's tests build with the same.

use std::path::Path;

/// Where LLVM's installer puts Clang on Windows.
#[cfg(windows)]
const LLVM_CLANG: &str = r"C:\Program Files\LLVM\bin\clang.exe";

/// The C compiler: the one `CC` names, as C's builds have it (1533); else `cc` on Linux and macOS,
/// and on Windows `clang` on the `PATH`, else the one LLVM's installer puts in its folder; none of
/// them is an error that says what to install.
pub fn compiler() -> Result<String, String> {
    if let Some(cc) = std::env::var("CC").ok().filter(|cc| !cc.is_empty()) {
        return Ok(cc);
    }
    #[cfg(windows)]
    {
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

/// What `sg build` makes of a flag of C given after `--` (choice 1937): most go on as they are,
/// unknown ones too; some go on with a warning, said on every build; some are refused, because sg
/// owns what they change.
#[derive(Debug, PartialEq)]
pub enum Judged {
    /// The warnings, one line each, without `warning: `.
    Given(Vec<String>),
    /// The error, without `error: `.
    Refused(String),
}

/// The flags of C after `--` judged together, so that a flag and the word after it (`-D SG_X`,
/// `-z execstack`, `-Xlinker execstack`) are seen as one.
pub fn judge_flags(flags: &[String]) -> Judged {
    const FRACTIONS: &[&str] = &[
        "-ffast-math",
        "-Ofast",
        "-ffp-contract=fast",
        "-ffp-contract=on",
        "-ffp-contract=fast-honor-pragmas",
        "-funsafe-math-optimizations",
        "-ffinite-math-only",
        "-fno-signed-zeros",
        "-freciprocal-math",
        "-fassociative-math",
        "-mfpmath=387",
    ];
    const LAYOUT: &[&str] = &["-m16", "-m32", "-mx32", "-fshort-enums", "-fshort-wchar", "-funsigned-char", "-fsigned-char"];
    let mut warnings = Vec::new();
    let mut words = flags.iter().map(String::as_str).peekable();
    while let Some(flag) = words.next() {
        let refused = match flag {
            "-o" => Some("the output is sg's to name: write -o before `--`".to_string()),
            _ if flag.starts_with("-o") => Some("the output is sg's to name: write -o before `--`".to_string()),
            "-c" => Some("what is made is sg's to say: a program, --test, --library, --shared or --freestanding".to_string()),
            "-S" | "-E" => Some("`sg emit-c` prints the C".to_string()),
            _ if flag == "-x" || flag.starts_with("-x") => Some("the input is the C that sg writes".to_string()),
            _ if flag.starts_with("-std=") => Some("the C that sg writes is C11".to_string()),
            "-shared" => Some("a shared library is built with --shared".to_string()),
            "-static" | "-static-pie" => Some("a program links the system's C library when it starts; a static program is not built yet".to_string()),
            "--target" | "-target" => Some("the machine is named with --target before `--`, with --freestanding".to_string()),
            _ if flag.starts_with("--target=") => Some("the machine is named with --target before `--`, with --freestanding".to_string()),
            _ if LAYOUT.contains(&flag) || flag == "-fpack-struct" || flag.starts_with("-fpack-struct=") => {
                Some("it changes the size or the layout of C's types, which the C that sg writes, and the libraries it calls, are written for".to_string())
            }
            _ => None,
        };
        if let Some(why) = refused {
            return Judged::Refused(format!("{flag} in the C flags: {why}"));
        }
        if FRACTIONS.contains(&flag) {
            warnings.push(format!("{flag}: fractional results may differ from the interpreter's and between machines"));
        } else if flag == "-fno-stack-clash-protection" {
            warnings.push(format!("{flag}: a function with a large frame may start below its stack, in the memory of another thread"));
        }
        // What the linker is told: after `-Wl,` split at the commas, or the word after `-z` or
        // `-Xlinker`.
        let linker: Vec<&str> = match flag {
            "-z" | "-Xlinker" => words.peek().map(|word| vec![*word]).unwrap_or_default(),
            _ if flag.starts_with("-Wl,") => flag["-Wl,".len()..].split(',').collect(),
            _ if flag.starts_with("-z") => vec![&flag[2..]],
            _ => Vec::new(),
        };
        let shown = match flag {
            "-z" | "-Xlinker" => format!("{flag} {}", words.peek().copied().unwrap_or_default()),
            _ => flag.to_string(),
        };
        for word in linker {
            match word.trim_start_matches("-z") {
                "execstack" => warnings.push(format!("{shown}: the stack becomes executable, which takes away a guard against code written into it")),
                "norelro" => warnings.push(format!("{shown}: the tables of the linker stay writable, which takes away a guard against their being overwritten")),
                _ => {}
            }
        }
        // A setting of SG's runtime, as `-DSG_THREAD_STACK=...`.
        let name = match flag {
            "-D" | "-U" => words.peek().copied(),
            _ if flag.starts_with("-D") || flag.starts_with("-U") => Some(&flag[2..]),
            _ => None,
        };
        if name.is_some_and(|name| name.starts_with("SG_")) {
            let shown = if flag.len() == 2 { format!("{flag} {}", name.unwrap_or_default()) } else { flag.to_string() };
            warnings.push(format!("{shown}: it changes a setting of SG's runtime, which is not a promise and may change from one version to the next"));
        }
    }
    Judged::Given(warnings)
}

/// The C compiler as a message names it: `cc`, or on Windows `clang` whatever folder it is in.
pub fn shown(compiler: &str) -> String {
    Path::new(compiler).file_stem().map_or_else(|| compiler.to_string(), |stem| stem.to_string_lossy().into_owned())
}

/// The C compiler as a message names it, when it is not known which one was found.
pub fn shown_here() -> String {
    compiler().map_or_else(|_| if cfg!(windows) { "clang".to_string() } else { "cc".to_string() }, |compiler| shown(&compiler))
}

/// What the C compiler is given before the C file: on Windows LLVM's linker, and C's library
/// linked into the program, so that it needs nothing installed to run. A debug build carries
/// what a debugger reads, with the lines of SG the C names (choice 1943): inside the program on
/// Linux, beside it on macOS (`x.dSYM`, which the C compiler makes) and on Windows (`x.pdb`).
pub fn compile_flags(debug: bool) -> Vec<String> {
    let mut flags = Vec::new();
    // A frame larger than the page the system leaves free below a stack is touched page by
    // page as it is made: without that a function with a large local value starts below its
    // stack, in the memory of another thread, and writes there. GCC and Clang do it on Linux;
    // on Windows the compiler does it for every frame larger than a page (`__chkstk`).
    if cfg!(target_os = "linux") {
        flags.push("-fstack-clash-protection".to_string());
    }
    if cfg!(windows) {
        flags.extend(["-fuse-ld=lld", "-fms-runtime-lib=static"].map(String::from));
    }
    // The columns would be those of the C, which the line of SG does not have.
    if debug {
        flags.extend(["-g", "-gno-column-info"].map(String::from));
    }
    flags
}

/// The manifest every program carries on Windows: C's functions take and give text as UTF-8
/// (from Windows 10 1903), a path may be longer than 260 characters, and the program is one of
/// Windows 10 and 11. The interpreter carries it too (`build.rs`), so that C it calls reads a
/// path as compiled code does.
pub const MANIFEST: &str = include_str!("sg.manifest");

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
        // Given whole: a comma in the temporary folder would end it after `-Wl,`.
        "-Xlinker".to_string(),
        format!("/MANIFESTINPUT:{}", manifest.display()),
        "-Wl,/MANIFESTUAC:NO".to_string(),
        "-Wl,/DEPENDENTLOADFLAG:0x1000".to_string(),
    ])
}

/// A folder of its own for what a build makes on the way (the C, the object of a library, the
/// manifest, the program of compiled tests): new, in the temporary folder, opened only by this
/// user, and at a name no one could guess (choice 1333). Removed when the build is done.
pub struct Work(std::path::PathBuf);

impl Work {
    pub fn new() -> std::io::Result<Self> {
        crate::sys::private_dir().map(Work)
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What makes a static library of an object: `ar` on Linux and macOS; on Windows LLVM's
/// `llvm-lib`, from the folder of the C compiler when it is named with one, else from `PATH`.
pub fn archive(out: &Path, object: &Path) -> std::process::Command {
    if !cfg!(windows) {
        let mut ar = std::process::Command::new("ar");
        ar.arg("rcs").arg(out).arg(object);
        return ar;
    }
    let lib = compiler()
        .ok()
        .and_then(|cc| Path::new(&cc).parent().filter(|dir| !dir.as_os_str().is_empty()).map(|dir| dir.join("llvm-lib.exe")))
        .filter(|lib| lib.exists())
        .map_or_else(|| "llvm-lib".into(), |lib| lib.into_os_string());
    let mut made = std::process::Command::new(lib);
    made.arg("/nologo").arg(format!("/out:{}", out.display())).arg(object);
    made
}

/// The file of a program: on Windows with `.exe` when the name has no extension.
pub fn program_file(out: &str) -> String {
    if cfg!(windows) && Path::new(out).extension().is_none() {
        return format!("{out}.exe");
    }
    out.to_string()
}

#[cfg(test)]
mod tests {
    use super::{judge_flags, Judged};

    fn judged(flags: &[&str]) -> Judged {
        judge_flags(&flags.iter().map(|flag| flag.to_string()).collect::<Vec<_>>())
    }

    /// Choice 1937: the three kinds of flags of C after `--`.
    #[test]
    fn flags_of_c_are_given_warned_or_refused() {
        // Given quietly, the unknown ones too.
        for flags in [&["-O3", "-march=native", "-flto", "-g", "-s", "-fsanitize=address", "-fno-omit-frame-pointer", "-DFAST", "-Iinclude", "-lz", "-Wl,--as-needed", "-mabi=lp64d", "-fno-such-flag"][..], &["-D", "NAME"], &["-z", "now"]] {
            assert_eq!(judged(flags), Judged::Given(Vec::new()), "{flags:?}");
        }
        // Given, with a warning each.
        for (flags, said) in [
            (&["-ffast-math"][..], "-ffast-math: fractional results"),
            (&["-Ofast"], "-Ofast: fractional"),
            (&["-ffp-contract=fast"], "-ffp-contract=fast: fractional"),
            (&["-mfpmath=387"], "-mfpmath=387: fractional"),
            (&["-fno-stack-clash-protection"], "-fno-stack-clash-protection: a function with a large frame"),
            (&["-Wl,-z,execstack"], "-Wl,-z,execstack: the stack becomes executable"),
            (&["-Wl,-znorelro"], "-Wl,-znorelro: the tables of the linker stay writable"),
            (&["-z", "execstack"], "-z execstack: the stack becomes executable"),
            (&["-Xlinker", "norelro"], "-Xlinker norelro: the tables"),
            (&["-DSG_THREAD_STACK=1"], "-DSG_THREAD_STACK=1: it changes a setting of SG's runtime"),
            (&["-D", "SG_COUNT=0"], "-D SG_COUNT=0: it changes a setting"),
            (&["-USG_SYNC"], "-USG_SYNC: it changes a setting"),
        ] {
            match judged(flags) {
                Judged::Given(warnings) => assert!(warnings.len() == 1 && warnings[0].starts_with(said), "{flags:?}: {warnings:?}"),
                refused => panic!("{flags:?}: {refused:?}"),
            }
        }
        // Refused: what sg owns, and the layout of C's types.
        for flag in ["-o", "-ofile", "-c", "-S", "-E", "-x", "-xc", "-std=c99", "-shared", "-static", "-static-pie", "--target", "-target", "--target=x86_64-linux-gnu", "-m32", "-mx32", "-fshort-enums", "-fpack-struct", "-fpack-struct=1", "-fshort-wchar", "-funsigned-char", "-fsigned-char"] {
            match judged(&["-O2", flag]) {
                Judged::Refused(why) => assert!(why.starts_with(&format!("{flag} in the C flags: ")), "{why}"),
                given => panic!("{flag}: {given:?}"),
            }
        }
    }
}
