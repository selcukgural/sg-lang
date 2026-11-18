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
    // On Windows without `\\?\`, which the C compiler and the messages would carry.
    let path = match cfg!(windows) {
        true => std::path::absolute(path),
        false => std::fs::canonicalize(path),
    }
    .map_err(|error| format!("-L {written}: {error}"))?;
    // A folder that is written into the program, where it looks when it starts, must not be one
    // others may write into. On Windows nothing is written into the program: it looks in its
    // own folder and the system's (see `copy_dlls`).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return Err(format!(
                "-L {written}: others may write into this folder, and a library put there would run inside the program\n  hint: `chmod go-w {}`, or a folder of your own",
                path.display()
            ));
        }
        // The loader reads `$ORIGIN` in a path it looks in, and `:` between two paths.
        let text = path.to_string_lossy();
        if text.contains(['$', ':']) {
            return Err(format!("-L {written}: the path of the folder holds `$` or `:`, which the loader of libraries reads as words of its own"));
        }
    }
    Ok(path)
}

/// What the C compiler is given for the folders: each is looked in when the program is linked
/// (`-L`) and, on Linux and macOS, when it starts (`rpath`), so that it runs without
/// `LD_LIBRARY_PATH`. Windows has no `rpath`.
pub fn folder_flags(dirs: &[PathBuf], rpath: bool) -> Vec<OsString> {
    let mut flags = Vec::new();
    for dir in dirs {
        flags.push(OsString::from("-L"));
        flags.push(dir.as_os_str().to_owned());
        if rpath && !cfg!(windows) {
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
            // C's own library, and its mathematics, is in every program on Windows.
            [one] if c_itself(one) => continue,
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
        if !c_itself(&chosen) {
            libs.insert(chosen);
        }
    }
    Ok(libs)
}

/// Whether `name` is C's own library, `c`, or its mathematics, `m`, on Windows: those are C's
/// library there, which every program is linked with, and nothing is linked for them.
fn c_itself(name: &str) -> bool {
    cfg!(windows) && matches!(name, "c" | "m")
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
    let cc = crate::cc::shown_here();
    match places.is_empty() {
        true => format!("looked for with {flags} where the C compiler `{cc}` looks"),
        false => format!("looked for with {flags} in {}, and where the C compiler `{cc}` looks", places.join(", ")),
    }
}

/// Whether the C compiler links an empty program with library `name`. Each question is asked
/// once in a process.
pub fn found(name: &str, dirs: &[PathBuf]) -> bool {
    if c_itself(name) {
        return true;
    }
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
    let Ok(compiler) = crate::cc::compiler() else { return false };
    let Ok(dir) = crate::ffi::private_dir() else { return false };
    let source = dir.join("probe.c");
    let linked = std::fs::write(&source, "int main(void) { return 0; }\n").is_ok()
        && std::process::Command::new(compiler)
            .args(crate::cc::compile_flags(false))
            .arg("-o")
            .arg(crate::cc::program_file(&dir.join("probe").to_string_lossy()))
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

/// On Windows, where a program has no `rpath`: the DLLs of the libraries linked from the folders
/// of `-L` are copied beside the program, `out`, where it looks for them when it starts (with the
/// folder of the system), and so are the DLLs they need in turn that lie in those folders (curl's
/// needs zlib's). The name of each DLL is read from its import library (`zlib.lib` names
/// `zlib1.dll`) and from the table of imports of each DLL copied, and the DLL is looked for in the
/// same folders, as vcpkg puts the one in `lib` and the other in `bin`. What is said is for the
/// output: each DLL copied, or one an import library names that is in none of the folders.
pub fn copy_dlls(libs: &[String], dirs: &[PathBuf], out: &Path) -> Vec<String> {
    let mut said = Vec::new();
    let beside = out.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(Path::new("."));
    // Each DLL with whether an import library named it (rather than another DLL).
    let mut wanted: Vec<(String, bool)> = Vec::new();
    for lib in libs {
        let name = lib.strip_prefix("-l").unwrap_or(lib);
        let Some(import) = dirs.iter().map(|dir| dir.join(format!("{name}.lib"))).find(|path| path.is_file()) else { continue };
        for dll in std::fs::read(&import).map(|bytes| dll_names(&bytes)).unwrap_or_default() {
            if !wanted.iter().any(|(known, _)| known.eq_ignore_ascii_case(&dll)) {
                wanted.push((dll, true));
            }
        }
    }
    // A name read from a file is only a name: a path in it would read or write outside the
    // folders (choice 1339).
    wanted.retain(|(dll, _)| plain_file_name(dll));
    let mut at = 0;
    while at < wanted.len() {
        let (dll, named) = wanted[at].clone();
        at += 1;
        let Some(found) = dirs.iter().map(|dir| dir.join(&dll)).find(|path| path.is_file()) else {
            if named {
                said.push(format!("note: {dll} is in no folder of `-L`; the program looks for it in its own folder and in the system's"));
            }
            continue;
        };
        let Ok(bytes) = std::fs::read(&found) else { continue };
        for needed in dll_imports(&bytes).into_iter().filter(|needed| plain_file_name(needed)) {
            if !wanted.iter().any(|(known, _)| known.eq_ignore_ascii_case(&needed)) {
                wanted.push((needed, false));
            }
        }
        let target = beside.join(&dll);
        if std::fs::read(&target).ok().as_ref() == Some(&bytes) {
            continue;
        }
        match std::fs::write(&target, &bytes) {
            Ok(()) => said.push(format!("copied {} beside the program", found.display())),
            Err(error) => said.push(format!("warning: {dll} could not be copied beside the program: {error}")),
        }
    }
    said
}

/// A name of a file and nothing more: no folder (`/`, `\\`), no drive or stream (`:`), not `.`
/// or `..`, no byte below 32.
fn plain_file_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.chars().any(|c| matches!(c, '/' | '\\' | ':') || c.is_control())
}

/// The names of the DLLs a DLL (or a program) imports from: its table of imports, in the
/// format Windows loads (PE32+). Nothing for what is not one.
fn dll_imports(bytes: &[u8]) -> Vec<String> {
    let u16_at = |at: usize| bytes.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let mut names = Vec::new();
    let Some(pe) = u32_at(0x3c) else { return names };
    if bytes.get(pe..pe + 4) != Some(b"PE\0\0") {
        return names;
    }
    let coff = pe + 4;
    let (Some(sections), Some(optional_size)) = (u16_at(coff + 2), u16_at(coff + 16)) else { return names };
    let optional = coff + 20;
    // PE32+; the second directory is the table of imports.
    if u16_at(optional) != Some(0x20b) {
        return names;
    }
    let Some(imports) = u32_at(optional + 112 + 8) else { return names };
    let table = optional + optional_size;
    let offset_of = |rva: usize| {
        (0..sections).find_map(|n| {
            let section = table + n * 40;
            let (size, address, raw_size, raw) = (u32_at(section + 8)?, u32_at(section + 12)?, u32_at(section + 16)?, u32_at(section + 20)?);
            (rva >= address && rva < address + size.max(raw_size)).then(|| rva - address + raw)
        })
    };
    let Some(mut entry) = offset_of(imports).filter(|_| imports != 0) else { return names };
    // Each entry is 20 bytes, the RVA of the name of its DLL at 12; an empty one ends them.
    while let Some(name_rva) = u32_at(entry + 12).filter(|rva| *rva != 0) {
        if let Some(start) = offset_of(name_rva) {
            if let Some(rest) = bytes.get(start..) {
                let end = rest.iter().position(|byte| *byte == 0).unwrap_or(rest.len());
                if end > 0 {
                    names.push(String::from_utf8_lossy(&rest[..end]).into_owned());
                }
            }
        }
        entry += 20;
    }
    names
}

/// The names of the DLLs an import library imports from: each member of the archive that is a
/// short import (Microsoft's `IMPORT_OBJECT_HEADER`) names its DLL after the name of its symbol.
/// A static library has none.
fn dll_names(bytes: &[u8]) -> Vec<String> {
    const MAGIC: &[u8] = b"!<arch>\n";
    const HEADER: usize = 60;
    let mut names: Vec<String> = Vec::new();
    if !bytes.starts_with(MAGIC) {
        return names;
    }
    let mut at = MAGIC.len();
    while at + HEADER <= bytes.len() {
        let size: usize = match std::str::from_utf8(&bytes[at + 48..at + 58]).ok().and_then(|size| size.trim().parse().ok()) {
            Some(size) => size,
            None => break,
        };
        let start = at + HEADER;
        let Some(data) = bytes.get(start..start.saturating_add(size)) else { break };
        // Sig1 0, Sig2 0xFFFF, then 16 bytes more; the name of the symbol and of the DLL follow.
        if data.len() > 20 && data[..4] == [0, 0, 0xFF, 0xFF] {
            let mut parts = data[20..].split(|byte| *byte == 0);
            if let (Some(_symbol), Some(dll)) = (parts.next(), parts.next()) {
                let dll = String::from_utf8_lossy(dll).into_owned();
                if !dll.is_empty() && !names.contains(&dll) {
                    names.push(dll);
                }
            }
        }
        at = start + size + (size & 1);
    }
    names
}

#[cfg(test)]
mod tests {
    use super::{copy_dlls, dll_imports, dll_names};

    /// A member of an archive, as `ar` writes it.
    fn member(name: &str, data: &[u8]) -> Vec<u8> {
        let mut bytes = format!("{name:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n", 0, 0, 0, 644, data.len()).into_bytes();
        bytes.extend_from_slice(data);
        if data.len() % 2 == 1 {
            bytes.push(b'\n');
        }
        bytes
    }

    /// A short import of `deflate` from `dll`.
    fn import_of(dll: &str) -> Vec<u8> {
        let mut import = vec![0u8, 0, 0xFF, 0xFF, 0, 0, 0x64, 0x86, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        import.extend_from_slice(format!("deflate\0{dll}\0").as_bytes());
        import
    }

    /// A PE32+ image of one section whose table of imports names `zlib1.dll` and `KERNEL32.dll`.
    #[test]
    fn imports_of_a_dll() {
        let mut image = vec![0u8; 0x400];
        image[0x3c] = 0x80;
        image[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        image[coff + 2] = 1; // one section
        image[coff + 16] = 240; // the size of the optional header
        let optional = coff + 20;
        image[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        image[optional + 120..optional + 124].copy_from_slice(&0x1000u32.to_le_bytes()); // imports at RVA 0x1000
        let section = optional + 240;
        image[section + 8..section + 12].copy_from_slice(&0x200u32.to_le_bytes());
        image[section + 12..section + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        image[section + 16..section + 20].copy_from_slice(&0x200u32.to_le_bytes());
        image[section + 20..section + 24].copy_from_slice(&0x200u32.to_le_bytes()); // at 0x200 in the file
        // Two entries, then an empty one; the names at RVA 0x1100 and 0x1110.
        image[0x200 + 12..0x200 + 16].copy_from_slice(&0x1100u32.to_le_bytes());
        image[0x214 + 12..0x214 + 16].copy_from_slice(&0x1110u32.to_le_bytes());
        image[0x300..0x309].copy_from_slice(b"zlib1.dll");
        image[0x310..0x31c].copy_from_slice(b"KERNEL32.dll");
        assert_eq!(dll_imports(&image), vec!["zlib1.dll".to_string(), "KERNEL32.dll".to_string()]);
        assert!(dll_imports(b"MZ not an image").is_empty());
        // A name past the end of the file is no name, and no panic.
        let mut cut = image.clone();
        cut.truncate(0x300);
        assert!(dll_imports(&cut).is_empty());
        let mut far = image;
        far[0x200 + 12..0x200 + 16].copy_from_slice(&0x11f0u32.to_le_bytes());
        far.truncate(0x3e0);
        assert_eq!(dll_imports(&far), vec!["KERNEL32.dll".to_string()]);
    }

    /// The name of a DLL read from an import library is a name, not a path: one that would lead
    /// out of the folders is not copied, the others are.
    #[test]
    fn copies_only_dlls_by_name() {
        let root = crate::sys::private_dir().unwrap();
        let (libs, out) = (root.join("a/libs"), root.join("b/out"));
        std::fs::create_dir_all(&libs).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        let mut archive = b"!<arch>\n".to_vec();
        for dll in ["../outside.dll", "..\\outside.dll", "zlib1.dll"] {
            archive.extend(member("x.dll/", &import_of(dll)));
        }
        std::fs::write(libs.join("z.lib"), archive).unwrap();
        std::fs::write(root.join("a/outside.dll"), b"not to be copied").unwrap();
        std::fs::write(libs.join("zlib1.dll"), b"a DLL").unwrap();
        let said = copy_dlls(&["-lz".to_string()], std::slice::from_ref(&libs), &out.join("program.exe"));
        assert!(!root.join("b/outside.dll").exists(), "{said:?}");
        assert_eq!(std::fs::read(out.join("zlib1.dll")).ok().as_deref(), Some(&b"a DLL"[..]), "{said:?}");
        assert_eq!(said.len(), 1, "{said:?}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// An archive of one short import of `deflate` from `zlib1.dll`, and a member that is not one.
    #[test]
    fn names_of_dlls() {
        let import = import_of("zlib1.dll");
        let mut archive = b"!<arch>\n".to_vec();
        archive.extend(member("other.obj/", b"not an import"));
        archive.extend(member("zlib1.dll/", &import));
        archive.extend(member("zlib1.dll/", &import));
        assert_eq!(dll_names(&archive), vec!["zlib1.dll".to_string()]);
        assert!(dll_names(b"not an archive").is_empty());
    }
}
