//! The libraries a program links: the folders `-L` names, and which of the names an `extern`
//! block gives a library (`extern "z", "zlib"`) is the one linked.
//!
//! Whether a library is there is asked of the C compiler, which links an empty program with it:
//! only the compiler knows every place it looks, and on macOS the libraries of the system are
//! not files on the disk but stubs in its SDK. The question is asked only for a block with more
//! than one name, and for every library once a link has failed, to say which one is missing.

use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::ast::{Foreign, Program};
use crate::diag::Diag;

/// The folder a `-L` names, as it is given to the C compiler: absolute. An error, in the words
/// of the command line, for a folder that is not there, and for one others may write into,
/// since the program looks for its libraries there when it starts (`rpath`).
pub fn folder(written: &str) -> Result<PathBuf, String> {
    let path = Path::new(written);
    let meta = std::fs::metadata(path).map_err(|_| format!("-L {written}: no such folder"))?;
    if !meta.is_dir() {
        return Err(format!("-L {written}: not a folder"));
    }
    let path = std::fs::canonicalize(path).map_err(|error| format!("-L {written}: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return Err(format!(
                "-L {written}: others may write into this folder, and a library put there would run inside the program\n  hint: `chmod go-w {}`, or a folder of your own",
                path.display()
            ));
        }
    }
    // The loader reads `$ORIGIN` in a path it looks in, and `:` between two paths.
    let text = path.to_string_lossy();
    if text.contains(['$', ':']) {
        return Err(format!("-L {written}: the path of the folder holds `$` or `:`, which the loader of libraries reads as words of its own"));
    }
    Ok(path)
}

/// What the C compiler is given for the folders: each is looked in when the program is linked
/// (`-L`) and when it starts (`rpath`), so that it runs without `LD_LIBRARY_PATH`.
pub fn folder_flags(dirs: &[PathBuf], rpath: bool) -> Vec<OsString> {
    let mut flags = Vec::new();
    for dir in dirs {
        flags.push(OsString::from("-L"));
        flags.push(dir.as_os_str().to_owned());
        if rpath {
            flags.extend(["-Xlinker", "-rpath", "-Xlinker"].map(OsString::from));
            flags.push(dir.as_os_str().to_owned());
        }
    }
    flags
}

/// The library each block links: the name it gives, or of several the first one the C
/// compiler finds. A program without an operating system links nothing itself; for it the first
/// name is the one said.
pub fn choose(prog: &Program, blocks: &[&Foreign]) -> Result<BTreeSet<String>, Diag> {
    let mut libs = BTreeSet::new();
    for foreign in blocks {
        let chosen = match foreign.libs.as_slice() {
            [] => continue,
            [one] => one.clone(),
            _ if prog.freestanding => foreign.libs[0].clone(),
            names => match names.iter().find(|name| found(name, &prog.lib_dirs)) {
                Some(name) => name.clone(),
                None => {
                    let listed: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
                    return Err(Diag::error(foreign.span, format!("none of the libraries this `extern` block names was found: {}", listed.join(", ")))
                        .note(looked(names, &prog.lib_dirs))
                        .hint("`-L folder` names a folder where the library lies"));
                }
            },
        };
        libs.insert(chosen);
    }
    Ok(libs)
}

/// Once a link has failed: the first library the C compiler does not find, said at the
/// `extern` block that names it. `None` if each one is there, and something else is missing.
pub fn missing(prog: &Program, blocks: &[&Foreign]) -> Option<Diag> {
    let libs = choose(prog, blocks).ok()?;
    let lib = libs.iter().find(|lib| !found(lib, &prog.lib_dirs))?;
    let foreign = blocks.iter().find(|foreign| foreign.libs.contains(lib))?;
    Some(
        Diag::error(foreign.span, format!("the library `{lib}` was not found"))
            .note(looked(std::slice::from_ref(lib), &prog.lib_dirs))
            .hint("`-L folder` names a folder where the library lies"),
    )
}

/// Where the names were looked for, in a note.
fn looked(names: &[String], dirs: &[PathBuf]) -> String {
    let flags: Vec<String> = names.iter().map(|name| format!("`-l{name}`")).collect();
    let flags = match flags.as_slice() {
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        [] => String::new(),
    };
    let places: Vec<String> = dirs.iter().map(|dir| dir.display().to_string()).collect();
    match places.is_empty() {
        true => format!("looked for with {flags} where the C compiler `cc` looks"),
        false => format!("looked for with {flags} in {}, and where the C compiler `cc` looks", places.join(", ")),
    }
}

/// Whether the C compiler links an empty program with library `name`. Each question is asked
/// once in a process.
pub fn found(name: &str, dirs: &[PathBuf]) -> bool {
    /// The answers so far, by the name and the folders.
    type Asked = HashMap<(String, Vec<PathBuf>), bool>;
    static ASKED: Mutex<Option<Asked>> = Mutex::new(None);
    let key = (name.to_string(), dirs.to_vec());
    if let Some(&known) = ASKED.lock().ok().as_ref().and_then(|asked| asked.as_ref()?.get(&key)) {
        return known;
    }
    let answer = ask(name, dirs);
    if let Ok(mut asked) = ASKED.lock() {
        asked.get_or_insert_with(HashMap::new).insert(key, answer);
    }
    answer
}

fn ask(name: &str, dirs: &[PathBuf]) -> bool {
    let Ok(dir) = crate::ffi::private_dir() else { return false };
    let source = dir.join("probe.c");
    let linked = std::fs::write(&source, "int main(void) { return 0; }\n").is_ok()
        && std::process::Command::new("cc")
            .arg("-o")
            .arg(dir.join("probe"))
            .arg(&source)
            .args(folder_flags(dirs, false))
            .arg(format!("-l{name}"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
    let _ = std::fs::remove_dir_all(&dir);
    linked
}
