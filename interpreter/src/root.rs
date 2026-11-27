//! `DirectoryRoot` in the interpreter (choices 2066 to 2071): a folder held open, and paths
//! found under it so that none leads out. Compiled code does the same in its runtime
//! (`ROOT_RUNTIME` in `emit_c.rs`), and the two give the same answers.
//!
//! A path is walked part by part from the handle of the root, never by the system from a text:
//! each folder on the way is opened by its name in the folder before it, without following a
//! link; a link is read and its parts put in front of what is left of the path; `..` goes back to
//! the folder the walk came through: what the system says is above the folder at hand is taken
//! only when it is that very folder (the same device and file number, kept for a path with a `..`
//! in it), and otherwise the names are walked again from the root. A link whose text starts at
//! the root of the system, and a `..` with no folder left to go back to, are `OutsideRoot`. At most `LINKS` links are followed. What the
//! walk gives is the folder that holds the last part and the name of that part, and the call is
//! made on that name without following a link, so a name changed to a link in between is an
//! error and not a way out.

use std::io;

/// A path that leads out of the root: `IoError.OutsideRoot`, a code of the runtime's own.
pub(crate) const OUTSIDE_ROOT: i32 = -23;
/// `IoError.NotAFile` and `IoError.InvalidInput`, as codes of the runtime's own.
pub(crate) const NOT_A_FILE: i32 = -20;
pub(crate) const INVALID_INPUT: i32 = -15;

/// The most links one path follows, as Linux follows.
pub(crate) const LINKS: usize = 40;

pub(crate) fn code(code: i32) -> io::Error {
    io::Error::from_raw_os_error(code)
}

/// The parts of a path, or of what a link holds; an empty one (`a//b`, a `/` at the end) is
/// none.
#[cfg(unix)]
fn parts(path: &[u8], separators: &[u8]) -> Vec<Vec<u8>> {
    path.split(|byte| separators.contains(byte)).filter(|part| !part.is_empty()).map(<[u8]>::to_vec).collect()
}

#[cfg(unix)]
pub(crate) use unix::{close, list, make, open, reader, remove, status, writer};
#[cfg(windows)]
pub(crate) use crate::sys::root::{close, list, make, open, reader, remove, status, writer};

#[cfg(unix)]
mod unix {
    use super::{code, parts, INVALID_INPUT, LINKS, NOT_A_FILE, OUTSIDE_ROOT};
    use std::collections::VecDeque;
    use std::ffi::{c_char, c_int, c_uint, c_void, CString};
    use std::io;
    use std::os::unix::io::FromRawFd;

    extern "C" {
        fn openat(dir: c_int, path: *const c_char, flags: c_int, ...) -> c_int;
        fn readlinkat(dir: c_int, path: *const c_char, into: *mut c_char, room: usize) -> isize;
        fn mkdirat(dir: c_int, path: *const c_char, mode: c_uint) -> c_int;
        fn unlinkat(dir: c_int, path: *const c_char, flags: c_int) -> c_int;
        #[link_name = "close"]
        fn close_fd(fd: c_int) -> c_int;
        #[cfg_attr(all(target_os = "macos", target_arch = "x86_64"), link_name = "fdopendir$INODE64")]
        fn fdopendir(fd: c_int) -> *mut c_void;
        #[cfg_attr(all(target_os = "macos", target_arch = "x86_64"), link_name = "readdir$INODE64")]
        fn readdir(folder: *mut c_void) -> *const u8;
        fn closedir(folder: *mut c_void) -> c_int;
        #[cfg(target_os = "linux")]
        fn syscall(number: std::ffi::c_long, ...) -> std::ffi::c_long;
        #[cfg(not(target_os = "linux"))]
        #[cfg_attr(all(target_os = "macos", target_arch = "x86_64"), link_name = "fstatat$INODE64")]
        fn fstatat(dir: c_int, path: *const c_char, into: *mut u8, flags: c_int) -> c_int;
    }

    const LINUX: bool = cfg!(target_os = "linux");
    /// The flags of `open` whose numbers Linux gives differently on ARM and PowerPC.
    const ARM: bool = cfg!(any(target_arch = "arm", target_arch = "aarch64", target_arch = "powerpc", target_arch = "powerpc64"));
    const O_WRONLY: c_int = 1;
    const O_NONBLOCK: c_int = if LINUX { 0o4000 } else { 0x4 };
    const O_APPEND: c_int = if LINUX { 0o2000 } else { 0x8 };
    const O_CREAT: c_int = if LINUX { 0o100 } else { 0x200 };
    const O_TRUNC: c_int = if LINUX { 0o1000 } else { 0x400 };
    const O_NOCTTY: c_int = if LINUX { 0o400 } else { 0x20000 };
    const O_CLOEXEC: c_int = if LINUX { 0o2000000 } else { 0x1000000 };
    const O_NOFOLLOW: c_int = match (LINUX, ARM) {
        (true, true) => 0o100000,
        (true, false) => 0o400000,
        (false, _) => 0x100,
    };
    const O_DIRECTORY: c_int = match (LINUX, ARM) {
        (true, true) => 0o40000,
        (true, false) => 0o200000,
        (false, _) => 0x100000,
    };
    /// A folder opened only to find names in it, which needs no right to list it: `O_PATH` on
    /// Linux, `O_SEARCH` on macOS (`O_EXEC | O_DIRECTORY`).
    const O_LOOK: c_int = if LINUX { 0o10000000 } else { 0x40000000 };
    const AT_FDCWD: c_int = if LINUX { -100 } else { -2 };
    const AT_SYMLINK_NOFOLLOW: c_int = if LINUX { 0x100 } else { 0x20 };
    const AT_REMOVEDIR: c_int = if LINUX { 0x200 } else { 0x80 };
    const ENOENT: i32 = 2;
    const EEXIST: i32 = 17;
    const ENOTDIR: i32 = 20;
    const EISDIR: i32 = 21;
    const EINVAL: i32 = 22;
    const EMLINK: i32 = 31;
    const ELOOP: i32 = if LINUX { 40 } else { 62 };

    fn last() -> io::Error {
        io::Error::last_os_error()
    }

    /// A descriptor of a folder on the way: closed where it ends, unless it is the root's.
    struct Folder {
        fd: c_int,
        own: bool,
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            if self.own {
                // SAFETY: the descriptor is this value's own.
                unsafe { close_fd(self.fd) };
            }
        }
    }

    /// A name as the system takes it; none has a zero byte in it.
    fn name_of(bytes: &[u8]) -> io::Result<CString> {
        CString::new(bytes).map_err(|_| code(ENOENT))
    }

    fn open_at(dir: c_int, name: &CString, flags: c_int) -> io::Result<c_int> {
        loop {
            // SAFETY: the name ends with a zero byte; the mode is read only with `O_CREAT`.
            let fd = unsafe { openat(dir, name.as_ptr(), flags | O_CLOEXEC, 0o666 as c_uint) };
            if fd >= 0 {
                return Ok(fd);
            }
            let error = last();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }

    /// The folder of that name in `dir`, opened to find names in; a link is not followed. A
    /// macOS without `O_SEARCH` opens it to be read.
    fn open_folder(dir: c_int, name: &CString, follow: bool) -> io::Result<c_int> {
        let flags = O_DIRECTORY | if follow { 0 } else { O_NOFOLLOW };
        match open_at(dir, name, flags | O_LOOK) {
            Err(error) if !LINUX && error.raw_os_error() == Some(EINVAL) => open_at(dir, name, flags),
            other => other,
        }
    }

    /// What the link of that name in `dir` holds; `EINVAL` for a name that is no link.
    fn link_of(dir: c_int, name: &CString) -> io::Result<Vec<u8>> {
        let mut text = vec![0u8; 4096];
        // SAFETY: the buffer holds as many bytes as it is said to.
        let count = unsafe { readlinkat(dir, name.as_ptr(), text.as_mut_ptr().cast(), text.len()) };
        if count < 0 {
            return Err(last());
        }
        text.truncate(count as usize);
        Ok(text)
    }

    /// A folder the walk went into: its name, and, once a `..` may come, which folder of the
    /// system it was (its device and its file number).
    struct Step {
        name: Vec<u8>,
        id: Option<(u64, u64)>,
    }

    /// Which folder of the system the descriptor is of.
    fn identity(fd: c_int) -> Option<(u64, u64)> {
        raw_at(fd, &here()).ok().map(|raw| (raw.device, raw.number))
    }

    /// The folder the names of `trail` lead to from the root, each opened in the one before
    /// it, and each kept as the folder it is: where a `..` goes back to when the system's
    /// answer is not the folder the walk came through.
    fn descend(root: c_int, trail: &mut [Step]) -> io::Result<Folder> {
        let mut at = Folder { fd: root, own: false };
        for step in trail {
            let fd = open_folder(at.fd, &name_of(&step.name)?, false)?;
            at = Folder { fd, own: true };
            step.id = identity(fd);
        }
        Ok(at)
    }

    /// The folder that holds the last part of the path, and the name of that part, which is
    /// no link when `follow` is set and the name is there; an empty name is the folder itself.
    fn reach(root: c_int, path: &[u8], follow: bool) -> io::Result<(Folder, Vec<u8>)> {
        let mut rest: VecDeque<Vec<u8>> = parts(path, b"/").into();
        let mut at = Folder { fd: root, own: false };
        let mut trail: Vec<Step> = Vec::new();
        // A path with a `..` in it keeps which folder each step went into.
        let mut told = rest.iter().any(|part| part == b"..");
        let mut links = 0;
        while let Some(part) = rest.pop_front() {
            if part == b"." {
                continue;
            }
            if part == b".." {
                if trail.pop().is_none() {
                    return Err(code(OUTSIDE_ROOT));
                }
                let Some(before) = trail.last() else {
                    at = Folder { fd: root, own: false };
                    continue;
                };
                // One step: what the system says is above, when it is the folder the walk came
                // through.
                if let Some(expected) = before.id {
                    if let Ok(fd) = open_folder(at.fd, &name_of(b"..")?, false) {
                        let up = Folder { fd, own: true };
                        if identity(up.fd) == Some(expected) {
                            at = up;
                            continue;
                        }
                    }
                }
                // From the root again, by the names; from here on each folder is kept.
                told = true;
                at = descend(root, &mut trail)?;
                continue;
            }
            let last_part = rest.is_empty();
            if last_part && !follow {
                return Ok((at, part));
            }
            let name = name_of(&part)?;
            if !last_part {
                // Most parts on the way are folders: one call finds it out and opens it.
                match open_folder(at.fd, &name, false) {
                    Ok(fd) => {
                        at = Folder { fd, own: true };
                        trail.push(Step { name: part, id: if told { identity(fd) } else { None } });
                        continue;
                    }
                    Err(error) if matches!(error.raw_os_error(), Some(ENOTDIR | EMLINK)) || error.raw_os_error() == Some(ELOOP) => {}
                    Err(error) => return Err(error),
                }
            }
            match link_of(at.fd, &name) {
                Ok(target) => {
                    links += 1;
                    if links > LINKS {
                        return Err(code(ELOOP));
                    }
                    if target.is_empty() {
                        return Err(code(ENOENT));
                    }
                    if target[0] == b'/' {
                        return Err(code(OUTSIDE_ROOT));
                    }
                    for piece in parts(&target, b"/").into_iter().rev() {
                        rest.push_front(piece);
                    }
                }
                // There, and no link: the last part, or a file where a folder was to be.
                Err(error) if error.raw_os_error() == Some(EINVAL) => match last_part {
                    true => return Ok((at, part)),
                    false => return Err(code(ENOTDIR)),
                },
                // A last part that is not there: the call says so, or makes it.
                Err(error) if error.raw_os_error() == Some(ENOENT) && last_part => return Ok((at, part)),
                Err(error) => return Err(error),
            }
        }
        Ok((at, Vec::new()))
    }

    /// A name changed to a link between the walk and the call is walked again, a few times.
    fn settled<T>(mut call: impl FnMut() -> io::Result<T>) -> io::Result<T> {
        let mut tries = 0;
        loop {
            match call() {
                Err(error) if matches!(error.raw_os_error(), Some(EMLINK)) || error.raw_os_error() == Some(ELOOP) => {
                    tries += 1;
                    if tries == 4 {
                        return Err(error);
                    }
                }
                other => return other,
            }
        }
    }

    fn bytes_of(path: &str) -> Vec<u8> {
        crate::names::os_of(path).as_encoded_bytes().to_vec()
    }

    fn here() -> CString {
        CString::new(".").expect("no zero byte")
    }

    /// What the system says of a name in a folder, a link not followed.
    struct Raw {
        mode: u32,
        size: u64,
        seconds: i64,
        nanos: i64,
        device: u64,
        number: u64,
    }

    /// `kind`, size, time of the last write in nanoseconds and permissions of the name in
    /// `dir`, a link not followed: the fields of `System.status`.
    fn fields_at(dir: c_int, name: &CString) -> io::Result<[i128; 4]> {
        let raw = raw_at(dir, name)?;
        Ok(fields(raw.mode, raw.size, raw.seconds, raw.nanos))
    }

    #[cfg(target_os = "linux")]
    fn raw_at(dir: c_int, name: &CString) -> io::Result<Raw> {
        const STATX: std::ffi::c_long = if cfg!(target_arch = "x86_64") {
            332
        } else if cfg!(any(target_arch = "arm", target_arch = "x86")) {
            if cfg!(target_arch = "arm") {
                397
            } else {
                383
            }
        } else if cfg!(any(target_arch = "powerpc", target_arch = "powerpc64")) {
            383
        } else {
            291
        };
        // STATX_TYPE | STATX_MODE | STATX_MTIME | STATX_INO | STATX_SIZE
        const MASK: c_uint = 0x1 | 0x2 | 0x40 | 0x100 | 0x200;
        let mut info = [0u64; 32];
        // SAFETY: `struct statx` is 256 bytes on every machine.
        let done = unsafe { syscall(STATX, dir, name.as_ptr(), AT_SYMLINK_NOFOLLOW, MASK, info.as_mut_ptr()) };
        if done != 0 {
            return Err(last());
        }
        let bytes: &[u8] = unsafe { std::slice::from_raw_parts(info.as_ptr().cast(), 256) };
        let mode = u16::from_ne_bytes([bytes[28], bytes[29]]);
        let size = u64::from_ne_bytes(bytes[40..48].try_into().expect("eight bytes"));
        let seconds = i64::from_ne_bytes(bytes[112..120].try_into().expect("eight bytes"));
        let nanos = u32::from_ne_bytes(bytes[120..124].try_into().expect("four bytes"));
        let number = u64::from_ne_bytes(bytes[32..40].try_into().expect("eight bytes"));
        // The two halves of the number of the device.
        let device = u64::from_ne_bytes(bytes[136..144].try_into().expect("eight bytes"));
        Ok(Raw { mode: u32::from(mode), size, seconds, nanos: i64::from(nanos), device, number })
    }

    #[cfg(not(target_os = "linux"))]
    fn raw_at(dir: c_int, name: &CString) -> io::Result<Raw> {
        // `struct stat` of macOS with 64-bit file numbers: 144 bytes.
        let mut info = [0u64; 18];
        // SAFETY: the buffer is as large as the struct.
        let done = unsafe { fstatat(dir, name.as_ptr(), info.as_mut_ptr().cast(), AT_SYMLINK_NOFOLLOW) };
        if done != 0 {
            return Err(last());
        }
        let bytes: &[u8] = unsafe { std::slice::from_raw_parts(info.as_ptr().cast(), 144) };
        let mode = u16::from_ne_bytes([bytes[4], bytes[5]]);
        let seconds = i64::from_ne_bytes(bytes[48..56].try_into().expect("eight bytes"));
        let nanos = i64::from_ne_bytes(bytes[56..64].try_into().expect("eight bytes"));
        let size = i64::from_ne_bytes(bytes[96..104].try_into().expect("eight bytes"));
        let device = u64::from(u32::from_ne_bytes(bytes[0..4].try_into().expect("four bytes")));
        let number = u64::from_ne_bytes(bytes[8..16].try_into().expect("eight bytes"));
        Ok(Raw { mode: u32::from(mode), size: size.max(0) as u64, seconds, nanos, device, number })
    }

    fn fields(mode: u32, size: u64, seconds: i64, nanos: i64) -> [i128; 4] {
        let kind = match mode & 0o170000 {
            0o100000 => 0,
            0o040000 => 1,
            0o120000 => 2,
            _ => 3,
        };
        let modified = (i128::from(seconds) * 1_000_000_000 + i128::from(nanos)).clamp(i128::from(i64::MIN), i128::from(i64::MAX));
        [kind, i128::from(size).min(i128::from(i64::MAX)), modified, i128::from(mode & 0o7777)]
    }

    /// `System.rootOpen`: the folder as a root. With no root (`-1`) the path is found as any
    /// path is, a link followed.
    pub(crate) fn open(root: i64, path: &str) -> io::Result<i64> {
        let path = bytes_of(path);
        if root < 0 {
            return open_folder(AT_FDCWD, &name_of(&path)?, true).map(i64::from);
        }
        settled(|| {
            let (at, leaf) = reach(root as c_int, &path, true)?;
            let name = if leaf.is_empty() { here() } else { name_of(&leaf)? };
            open_folder(at.fd, &name, false).map(i64::from)
        })
    }

    /// `System.rootReader`: with `regular`, only a regular file is opened.
    pub(crate) fn reader(root: i64, path: &str, regular: bool) -> io::Result<std::fs::File> {
        let path = bytes_of(path);
        settled(|| {
            let (at, leaf) = reach(root as c_int, &path, true)?;
            if leaf.is_empty() {
                return Err(code(EISDIR));
            }
            let name = name_of(&leaf)?;
            // Asked before it is opened, so that a pipe is never opened and no writer that waits
            // on it is let go; opened without waiting, and looked at again.
            let plain = |mode_kind: i128| match mode_kind {
                0 => Ok(()),
                1 => Err(code(EISDIR)),
                // Changed to a link after the walk found it none: walked again.
                2 => Err(code(ELOOP)),
                _ => Err(code(NOT_A_FILE)),
            };
            if regular {
                plain(fields_at(at.fd, &name)?[0])?;
            }
            let flags = O_NOFOLLOW | if regular { O_NONBLOCK | O_NOCTTY } else { 0 };
            let fd = open_at(at.fd, &name, flags)?;
            // SAFETY: the descriptor was just opened and is nobody else's.
            let file = unsafe { std::fs::File::from_raw_fd(fd) };
            let info = file.metadata()?;
            if info.is_dir() {
                return Err(code(EISDIR));
            }
            if regular && !info.is_file() {
                return Err(code(NOT_A_FILE));
            }
            Ok(file)
        })
    }

    /// `System.rootWriter`: `how` 1 makes or empties the file, 2 writes after what it holds, 3
    /// from its start over what it holds; a missing file is made.
    pub(crate) fn writer(root: i64, path: &str, how: i64) -> io::Result<std::fs::File> {
        let path = bytes_of(path);
        settled(|| {
            let (at, leaf) = reach(root as c_int, &path, true)?;
            if leaf.is_empty() {
                return Err(code(EISDIR));
            }
            let flags = O_WRONLY
                | O_CREAT
                | O_NOFOLLOW
                | match how {
                    1 => O_TRUNC,
                    2 => O_APPEND,
                    _ => 0,
                };
            let fd = open_at(at.fd, &name_of(&leaf)?, flags)?;
            // SAFETY: the descriptor was just opened and is nobody else's.
            Ok(unsafe { std::fs::File::from_raw_fd(fd) })
        })
    }

    /// `System.rootStatus`: the fields of `System.status`, or of `System.linkStatus` without
    /// `follow`.
    pub(crate) fn status(root: i64, path: &str, follow: bool) -> io::Result<[i128; 4]> {
        let path = bytes_of(path);
        settled(|| {
            let (at, leaf) = reach(root as c_int, &path, follow)?;
            let name = if leaf.is_empty() { here() } else { name_of(&leaf)? };
            let fields = fields_at(at.fd, &name)?;
            // The name was changed to a link after the walk found it none: walked again, so that
            // a link is never told as what it leads to.
            match follow && fields[0] == 2 {
                true => Err(code(ELOOP)),
                false => Ok(fields),
            }
        })
    }

    /// `System.rootList`: the names in the folder with their kinds, not sorted.
    pub(crate) fn list(root: i64, path: &str) -> io::Result<Vec<(Vec<u8>, u8)>> {
        let path = bytes_of(path);
        let fd = settled(|| {
            let (at, leaf) = reach(root as c_int, &path, true)?;
            let name = if leaf.is_empty() { here() } else { name_of(&leaf)? };
            open_at(at.fd, &name, O_DIRECTORY | O_NOFOLLOW)
        })?;
        // SAFETY: the descriptor is a folder's, and becomes the listing's.
        let folder = unsafe { fdopendir(fd) };
        if folder.is_null() {
            let error = last();
            // SAFETY: the listing did not take the descriptor.
            unsafe { close_fd(fd) };
            return Err(error);
        }
        // Where the name and the kind lie in a `struct dirent`.
        let (kind_at, name_at) = if LINUX { (18, 19) } else { (20, 21) };
        let mut found = Vec::new();
        loop {
            // SAFETY: the listing is open; the entry lives until the next call.
            let entry = unsafe { readdir(folder) };
            if entry.is_null() {
                break;
            }
            // SAFETY: the name ends with a zero byte inside the entry.
            let name = unsafe { std::ffi::CStr::from_ptr(entry.add(name_at).cast()) }.to_bytes().to_vec();
            if name == b"." || name == b".." {
                continue;
            }
            // SAFETY: the byte of the kind lies inside the entry.
            let kind = match unsafe { *entry.add(kind_at) } {
                8 => 0,
                4 => 1,
                10 => 2,
                // A file system that does not say: the name is asked.
                0 => name_of(&name).and_then(|name| fields_at(fd, &name)).map(|fields| fields[0] as u8).unwrap_or(3),
                _ => 3,
            };
            found.push((name, kind));
        }
        // SAFETY: the listing is this call's, and closes its descriptor.
        unsafe { closedir(folder) };
        Ok(found)
    }

    /// `System.rootMake`: `mkdirat`.
    pub(crate) fn make(root: i64, path: &str) -> io::Result<()> {
        let (at, leaf) = reach(root as c_int, &bytes_of(path), false)?;
        if leaf.is_empty() {
            return Err(code(EEXIST));
        }
        let name = name_of(&leaf)?;
        // SAFETY: the name ends with a zero byte.
        match unsafe { mkdirat(at.fd, name.as_ptr(), 0o777) } {
            0 => Ok(()),
            _ => Err(last()),
        }
    }

    /// `System.rootRemove`: the file, or the link itself; with `folder`, the empty folder.
    pub(crate) fn remove(root: i64, path: &str, folder: bool) -> io::Result<()> {
        let (at, leaf) = reach(root as c_int, &bytes_of(path), false)?;
        if leaf.is_empty() {
            return Err(code(if folder { INVALID_INPUT } else { EISDIR }));
        }
        let name = name_of(&leaf)?;
        // SAFETY: the name ends with a zero byte.
        if unsafe { unlinkat(at.fd, name.as_ptr(), if folder { AT_REMOVEDIR } else { 0 }) } == 0 {
            return Ok(());
        }
        let error = last();
        // macOS says of a folder removed as a file that it is not permitted: it is told as Linux
        // tells it.
        match !folder && matches!(error.raw_os_error(), Some(1 | EISDIR)) && fields_at(at.fd, &name).is_ok_and(|fields| fields[0] == 1) {
            true => Err(code(EISDIR)),
            false => Err(error),
        }
    }

    /// `System.rootClose`.
    pub(crate) fn close(root: i64) {
        // SAFETY: the handle is the root's own.
        unsafe { close_fd(root as c_int) };
    }
}
