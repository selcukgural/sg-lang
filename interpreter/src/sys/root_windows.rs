//! `DirectoryRoot` on Windows (choices 2066 to 2071), as `root.rs` walks a path on Linux and
//! macOS and as compiled code does it (`ROOT_RUNTIME`): every part is opened by its name in the
//! folder before it (`NtCreateFile` with the handle of that folder), without following a point
//! that stands for another name. A symbolic link is read: one that says where it leads from its
//! own folder is walked on; one that names a place from a drive or from the root of the system
//! leads out of the root, and so does a junction, which always names such a place. A point that
//! only marks a file kept by a service is opened as the system opens it.

use super::{delete_handle, info_of, last_error, CloseHandle, CreateFileW, DeviceIoControl, GetFileInformationByHandleEx, GetFileType, Handle, DIRECTORY, INVALID_HANDLE, IS_DIR, NOT_DIR, REPARSE_POINT, TAG_LINK};
use crate::root::{code, INVALID_INPUT, LINKS, NOT_A_FILE, OUTSIDE_ROOT};
use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum: u16,
    buffer: *const u16,
}

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root: Handle,
    name: *const UnicodeString,
    attributes: u32,
    security: *const c_void,
    quality: *const c_void,
}

#[repr(C)]
struct IoStatus {
    status: usize,
    information: usize,
}

#[link(name = "ntdll")]
extern "system" {
    fn NtCreateFile(handle: *mut Handle, access: u32, attributes: *const ObjectAttributes, status: *mut IoStatus, size: *const i64, file_attributes: u32, share: u32, disposition: u32, options: u32, extended: *const c_void, extended_size: u32) -> i32;
    fn RtlNtStatusToDosError(status: i32) -> u32;
}

const READ_ATTRIBUTES: u32 = 0x80;
const SYNCHRONIZE: u32 = 0x0010_0000;
const DELETE: u32 = 0x0001_0000;
const LIST_DIRECTORY: u32 = 0x1;
const APPEND_DATA: u32 = 0x4;
/// `FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE`.
const FOLDER: u32 = LIST_DIRECTORY | 0x20 | READ_ATTRIBUTES | SYNCHRONIZE;
/// `FILE_GENERIC_READ` and `FILE_GENERIC_WRITE`.
const GENERIC_READ: u32 = 0x0012_0089;
const GENERIC_WRITE: u32 = 0x0012_0116;
const OPEN: u32 = 1;
const CREATE: u32 = 2;
const OPEN_IF: u32 = 3;
const OVERWRITE_IF: u32 = 5;
const DIRECTORY_FILE: u32 = 0x1;
const NON_DIRECTORY_FILE: u32 = 0x40;
const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
/// `FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_FOR_BACKUP_INTENT`.
const SYNC: u32 = 0x20 | 0x4000;
/// `ERROR_FILE_NOT_FOUND`, `ERROR_PATH_NOT_FOUND`, `ERROR_ALREADY_EXISTS`.
const NOT_FOUND: i32 = 2;
const PATH_NOT_FOUND: i32 = 3;
const EXISTS: i32 = 183;
/// `ERROR_CANT_RESOLVE_FILENAME`: links that never end, as Windows says it.
const LOOP: i32 = 1921;

/// A handle of a folder on the way: closed where it ends, unless it is the root's.
struct Folder {
    handle: Handle,
    own: bool,
}

impl Drop for Folder {
    fn drop(&mut self) {
        if self.own {
            // SAFETY: the handle is this value's own.
            unsafe { CloseHandle(self.handle) };
        }
    }
}

/// A handle closed where it ends.
struct Held(Handle);

impl Drop for Held {
    fn drop(&mut self) {
        // SAFETY: the handle is this value's own.
        unsafe { CloseHandle(self.0) };
    }
}

/// The name in the folder `dir` opened; an empty name opens the folder itself once more.
fn open_at(dir: Handle, name: &[u16], access: u32, disposition: u32, options: u32) -> io::Result<Handle> {
    if name.len() > 32000 {
        return Err(code(206));
    }
    let text = UnicodeString { length: (name.len() * 2) as u16, maximum: (name.len() * 2) as u16, buffer: name.as_ptr() };
    // OBJ_CASE_INSENSITIVE
    let attributes = ObjectAttributes { length: std::mem::size_of::<ObjectAttributes>() as u32, root: dir, name: &text, attributes: 0x40, security: std::ptr::null(), quality: std::ptr::null() };
    let mut status = IoStatus { status: 0, information: 0 };
    let mut handle: Handle = std::ptr::null_mut();
    // SAFETY: the structures live across the call; the name has the length given.
    let done = unsafe { NtCreateFile(&mut handle, access, &attributes, &mut status, std::ptr::null(), 0x80, 7, disposition, options, std::ptr::null(), 0) };
    if done < 0 {
        // STATUS_FILE_IS_A_DIRECTORY, which Windows would tell as a right not held.
        if done as u32 == 0xC000_00BA {
            return Err(code(IS_DIR));
        }
        // SAFETY: a number in, a number out.
        let error = unsafe { RtlNtStatusToDosError(done) };
        return Err(code(if error == 0 { 31 } else { error as i32 }));
    }
    Ok(handle)
}

/// What a handle opened without following is: 0 no point, 1 a symbolic link, 2 a point that
/// stands for another name some other way (a junction), 3 a point that marks a file.
fn point_of(handle: Handle) -> u8 {
    let mut tag = [0u32; 2];
    // `FileAttributeTagInfo`. SAFETY: the buffer has the size given.
    if unsafe { GetFileInformationByHandleEx(handle, 9, tag.as_mut_ptr().cast(), 8) } == 0 || tag[0] & REPARSE_POINT == 0 {
        return 0;
    }
    match tag[1] {
        TAG_LINK => 1,
        // IO_REPARSE_TAG_MOUNT_POINT, or the bit of a point that stands for another name.
        0xA000_0003 => 2,
        other if other & 0x2000_0000 != 0 => 2,
        _ => 3,
    }
}

/// What the symbolic link holds, when it leads from its own folder.
fn target_of(handle: Handle) -> io::Result<Vec<u16>> {
    // `MAXIMUM_REPARSE_DATA_BUFFER_SIZE`; `FSCTL_GET_REPARSE_POINT`.
    let mut buffer = vec![0u8; 16 * 1024];
    let mut got = 0u32;
    // SAFETY: the handle is open and the buffer has the size given.
    if unsafe { DeviceIoControl(handle, 0x0009_00A8, std::ptr::null(), 0, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut got, std::ptr::null_mut()) } == 0 {
        return Err(last_error());
    }
    let word = |at: usize| u16::from_le_bytes([buffer[at], buffer[at + 1]]) as usize;
    let (offset, length) = (word(8), word(10));
    let flags = u32::from_le_bytes([buffer[16], buffer[17], buffer[18], buffer[19]]);
    let start = 20 + offset;
    if got < 20 || start + length > got as usize {
        return Err(code(LOOP));
    }
    let name: Vec<u16> = buffer[start..start + length].as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    if name.is_empty() {
        return Err(code(NOT_FOUND));
    }
    // `SYMLINK_FLAG_RELATIVE` is not set for a place named from a drive; one named from the root
    // of the drive (`\x`) leads out as well.
    let separator = |unit: u16| unit == u16::from(b'\\') || unit == u16::from(b'/');
    if flags & 1 == 0 || separator(name[0]) || name.get(1) == Some(&u16::from(b':')) {
        return Err(code(OUTSIDE_ROOT));
    }
    Ok(name)
}

fn parts(path: &[u16]) -> Vec<Vec<u16>> {
    path.split(|unit| *unit == u16::from(b'/') || *unit == u16::from(b'\\')).filter(|part| !part.is_empty()).map(<[u16]>::to_vec).collect()
}

/// The folder that holds the last part of the path, the name of that part, and whether it is a
/// point that marks a file, which is opened as the system opens it.
fn reach(root: Handle, path: &[u16], follow: bool) -> io::Result<(Folder, Vec<u16>, bool)> {
    let mut rest: std::collections::VecDeque<Vec<u16>> = parts(path).into();
    let mut at = Folder { handle: root, own: false };
    // The folders gone into stay open until the walk ends, each above the next: `..` goes back
    // to the one before, and the system is never asked what is above a folder.
    let mut above: Vec<Folder> = Vec::new();
    let mut links = 0;
    let dot = u16::from(b'.');
    while let Some(part) = rest.pop_front() {
        if part == [dot] {
            continue;
        }
        if part == [dot, dot] {
            match above.pop() {
                Some(before) => at = before,
                None => return Err(code(OUTSIDE_ROOT)),
            }
            continue;
        }
        let last = rest.is_empty();
        if last && !follow {
            return Ok((at, part, false));
        }
        let opened = match last {
            true => open_at(at.handle, &part, READ_ATTRIBUTES | SYNCHRONIZE, OPEN, OPEN_REPARSE_POINT | SYNC),
            false => open_at(at.handle, &part, FOLDER, OPEN, DIRECTORY_FILE | OPEN_REPARSE_POINT | SYNC),
        };
        let handle = match opened {
            Ok(handle) => handle,
            // A last part that is not there: the call says so, or makes it.
            Err(error) if last && matches!(error.raw_os_error(), Some(NOT_FOUND | PATH_NOT_FOUND)) => return Ok((at, part, false)),
            Err(error) => return Err(error),
        };
        match point_of(handle) {
            2 => {
                drop(Held(handle));
                return Err(code(OUTSIDE_ROOT));
            }
            1 => {
                let target = target_of(handle);
                drop(Held(handle));
                links += 1;
                if links > LINKS {
                    return Err(code(LOOP));
                }
                for piece in parts(&target?).into_iter().rev() {
                    rest.push_front(piece);
                }
            }
            point if last => {
                drop(Held(handle));
                return Ok((at, part, point == 3));
            }
            _ => above.push(std::mem::replace(&mut at, Folder { handle, own: true })),
        }
    }
    Ok((at, Vec::new(), false))
}

/// The path under the root opened as the access, the disposition and the options say. A path
/// that is the folder of the walk itself opens it when `folder` is set, and is `IsDirectory`
/// otherwise. With `follow`, a name found changed to a link after the walk is walked again.
fn open_final(root: i64, path: &str, follow: bool, access: u32, disposition: u32, options: u32, folder: bool) -> io::Result<Handle> {
    let path: Vec<u16> = crate::names::os_of(path).encode_wide().collect();
    if path.contains(&0) {
        return Err(code(NOT_FOUND));
    }
    let mut tries = 0;
    loop {
        let (at, leaf, marked) = reach(root as isize as Handle, &path, follow)?;
        if leaf.is_empty() && !folder {
            return Err(code(IS_DIR));
        }
        let handle = open_at(at.handle, &leaf, access, disposition, options | if marked { 0 } else { OPEN_REPARSE_POINT })?;
        if follow && !marked && matches!(point_of(handle), 1 | 2) {
            drop(Held(handle));
            tries += 1;
            if tries == 4 {
                return Err(code(LOOP));
            }
            continue;
        }
        return Ok(handle);
    }
}

/// `System.rootOpen`: the folder as a root. With no root (`-1`) the path is found as any path
/// is, a link followed.
pub(crate) fn open(root: i64, path: &str) -> io::Result<i64> {
    if root < 0 {
        if path.contains('\0') {
            return Err(code(NOT_FOUND));
        }
        let name = super::wide_path(path);
        // OPEN_EXISTING; FILE_FLAG_BACKUP_SEMANTICS. SAFETY: the name ends with a zero.
        let handle = unsafe { CreateFileW(name.as_ptr(), FOLDER, 7, std::ptr::null(), 3, 0x0200_0000, std::ptr::null_mut()) };
        if handle == INVALID_HANDLE {
            return Err(super::refine(path, last_error()));
        }
        if info_of(handle).map(|info| info.attributes & DIRECTORY == 0).unwrap_or(true) {
            drop(Held(handle));
            return Err(code(NOT_DIR));
        }
        return Ok(handle as isize as i64);
    }
    open_final(root, path, true, FOLDER, OPEN, DIRECTORY_FILE | SYNC, true).map(|handle| handle as isize as i64)
}

/// `System.rootReader`: with `regular`, only a file of the disk.
pub(crate) fn reader(root: i64, path: &str, regular: bool) -> io::Result<std::fs::File> {
    // FILE_SYNCHRONOUS_IO_NONALERT
    let handle = open_final(root, path, true, GENERIC_READ, OPEN, NON_DIRECTORY_FILE | 0x20, false)?;
    // FILE_TYPE_DISK. SAFETY: the handle is open.
    if regular && unsafe { GetFileType(handle) } != 1 {
        drop(Held(handle));
        return Err(code(NOT_A_FILE));
    }
    // SAFETY: the handle was just opened and is nobody else's.
    Ok(unsafe { std::fs::File::from_raw_handle(handle) })
}

/// `System.rootWriter`: `how` 1 makes or empties the file, 2 writes after what it holds, 3 from
/// its start over what it holds; a missing file is made.
pub(crate) fn writer(root: i64, path: &str, how: i64) -> io::Result<std::fs::File> {
    let access = if how == 2 { APPEND_DATA | SYNCHRONIZE } else { GENERIC_WRITE };
    let disposition = if how == 1 { OVERWRITE_IF } else { OPEN_IF };
    let handle = open_final(root, path, true, access, disposition, NON_DIRECTORY_FILE | 0x20, false)?;
    // SAFETY: the handle was just opened and is nobody else's.
    Ok(unsafe { std::fs::File::from_raw_handle(handle) })
}

/// `System.rootStatus`: the fields of `System.status`, or of `System.linkStatus` without `follow`.
pub(crate) fn status(root: i64, path: &str, follow: bool) -> io::Result<[i128; 4]> {
    let handle = Held(open_final(root, path, follow, READ_ATTRIBUTES | SYNCHRONIZE, OPEN, SYNC, true)?);
    let info = info_of(handle.0)?;
    let folder = info.attributes & DIRECTORY != 0;
    // FILE_TYPE_DISK. SAFETY: the handle is open.
    let mut kind = if folder { 1 } else if unsafe { GetFileType(handle.0) } == 1 { 0 } else { 3 };
    if !follow {
        match point_of(handle.0) {
            1 => kind = 2,
            2 => kind = 3,
            _ => {}
        }
    }
    let size = if folder { 0 } else { (i128::from(info.size_high) << 32) | i128::from(info.size_low) };
    let units = (i128::from(info.written.high) << 32) | i128::from(info.written.low);
    let modified = ((units - 116_444_736_000_000_000) * 100).clamp(i128::from(i64::MIN), i128::from(i64::MAX));
    // FILE_ATTRIBUTE_READONLY
    let fixed = info.attributes & 0x1 != 0;
    let mode = match (folder, fixed) {
        (true, true) => 0o555,
        (true, false) => 0o777,
        (false, true) => 0o444,
        (false, false) => 0o666,
    };
    Ok([kind, size, modified, mode])
}

/// `System.rootList`: the names in the folder with their kinds, not sorted.
pub(crate) fn list(root: i64, path: &str) -> io::Result<Vec<(Vec<u8>, u8)>> {
    use std::os::windows::ffi::OsStringExt;
    let handle = Held(open_final(root, path, true, LIST_DIRECTORY | READ_ATTRIBUTES | SYNCHRONIZE, OPEN, DIRECTORY_FILE | SYNC, true)?);
    let mut buffer = vec![0u64; 8192];
    let mut found = Vec::new();
    loop {
        // `FileIdBothDirectoryInfo`. SAFETY: the buffer has the size given.
        if unsafe { GetFileInformationByHandleEx(handle.0, 10, buffer.as_mut_ptr().cast(), (buffer.len() * 8) as u32) } == 0 {
            let error = last_error();
            // ERROR_NO_MORE_FILES
            if error.raw_os_error() == Some(18) {
                break;
            }
            return Err(error);
        }
        // SAFETY: the system filled the buffer with entries, each saying where the next starts.
        let bytes: &[u8] = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast(), buffer.len() * 8) };
        let mut at = 0usize;
        loop {
            let number = |from: usize| u32::from_le_bytes([bytes[at + from], bytes[at + from + 1], bytes[at + from + 2], bytes[at + from + 3]]);
            let (next, attributes, length, tag) = (number(0) as usize, number(56), number(60) as usize, number(64));
            let name: Vec<u16> = bytes[at + 104..at + 104 + length].as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
            let dot = u16::from(b'.');
            if name != [dot] && name != [dot, dot] {
                // The tag of a point lies where the size of the extended attributes does.
                let link = attributes & REPARSE_POINT != 0 && (tag == TAG_LINK || tag == 0xA000_0003);
                // FILE_ATTRIBUTE_DEVICE
                let kind = if link { 2 } else if attributes & DIRECTORY != 0 { 1 } else if attributes & 0x40 != 0 { 3 } else { 0 };
                found.push((std::ffi::OsString::from_wide(&name).as_encoded_bytes().to_vec(), kind));
            }
            if next == 0 {
                break;
            }
            at += next;
        }
    }
    Ok(found)
}

/// `System.rootMake`.
pub(crate) fn make(root: i64, path: &str) -> io::Result<()> {
    let wide: Vec<u16> = crate::names::os_of(path).encode_wide().collect();
    if wide.contains(&0) {
        return Err(code(NOT_FOUND));
    }
    let (at, leaf, _) = reach(root as isize as Handle, &wide, false)?;
    if leaf.is_empty() {
        return Err(code(EXISTS));
    }
    let handle = open_at(at.handle, &leaf, LIST_DIRECTORY | SYNCHRONIZE, CREATE, DIRECTORY_FILE | OPEN_REPARSE_POINT | SYNC)?;
    drop(Held(handle));
    Ok(())
}

/// `System.rootRemove`: the file, or the link itself; with `folder`, the empty folder, and a link
/// to a folder is no folder, as on Linux and macOS.
pub(crate) fn remove(root: i64, path: &str, folder: bool) -> io::Result<()> {
    let wide: Vec<u16> = crate::names::os_of(path).encode_wide().collect();
    if wide.contains(&0) {
        return Err(code(NOT_FOUND));
    }
    let (at, leaf, _) = reach(root as isize as Handle, &wide, false)?;
    if leaf.is_empty() {
        return Err(code(if folder { INVALID_INPUT } else { IS_DIR }));
    }
    let handle = Held(open_at(at.handle, &leaf, DELETE | READ_ATTRIBUTES | SYNCHRONIZE, OPEN, OPEN_REPARSE_POINT | SYNC)?);
    let info = info_of(handle.0)?;
    let plain_folder = info.attributes & DIRECTORY != 0 && info.attributes & REPARSE_POINT == 0;
    match (folder, plain_folder) {
        (true, false) => Err(code(NOT_DIR)),
        (false, true) => Err(code(IS_DIR)),
        _ => delete_handle(handle.0),
    }
}

/// `System.rootClose`.
pub(crate) fn close(root: i64) {
    if root >= 0 {
        // SAFETY: the handle is the root's own.
        unsafe { CloseHandle(root as isize as Handle) };
    }
}
