//! What the interpreter asks Windows: random bytes, the clocks and the calls of the file system,
//! as compiled code asks them, through the functions of Windows declared here (no crate). Other
//! programs, signals and the network come in later slices; until then nothing reaches them,
//! since the checker refuses a program that uses them (`pending`).

use std::ffi::{c_char, c_int, c_void};
use std::os::windows::ffi::OsStrExt;
use std::sync::OnceLock;

use crate::interp::Value;

type Handle = *mut c_void;
type Bool = i32;

const INVALID_HANDLE: Handle = -1isize as Handle;

#[repr(C)]
#[derive(Default)]
struct FileTime {
    low: u32,
    high: u32,
}

#[repr(C)]
#[derive(Default)]
struct ByHandleFileInformation {
    attributes: u32,
    created: FileTime,
    accessed: FileTime,
    written: FileTime,
    volume: u32,
    size_high: u32,
    size_low: u32,
    links: u32,
    index_high: u32,
    index_low: u32,
}

#[repr(C)]
struct SecurityAttributes {
    length: u32,
    descriptor: *mut c_void,
    inherit: Bool,
}

#[link(name = "kernel32")]
extern "system" {
    fn QueryPerformanceCounter(count: *mut i64) -> Bool;
    fn QueryPerformanceFrequency(frequency: *mut i64) -> Bool;
    fn GetSystemTimePreciseAsFileTime(time: *mut u64);
    fn LoadLibraryExW(name: *const u16, file: Handle, flags: u32) -> Handle;
    fn GetProcAddress(module: Handle, name: *const c_char) -> *mut c_void;
    fn CreateFileW(name: *const u16, access: u32, share: u32, security: *const c_void, disposition: u32, flags: u32, template: Handle) -> Handle;
    fn CloseHandle(handle: Handle) -> Bool;
    fn GetFileInformationByHandle(handle: Handle, info: *mut ByHandleFileInformation) -> Bool;
    fn SetFileInformationByHandle(handle: Handle, class: i32, info: *const c_void, size: u32) -> Bool;
    fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> Bool;
    fn CopyFileExW(from: *const u16, to: *const u16, progress: *const c_void, data: *const c_void, cancel: *mut Bool, flags: u32) -> Bool;
    fn CreateDirectoryW(name: *const u16, security: *const SecurityAttributes) -> Bool;
    fn GetFullPathNameW(name: *const u16, length: u32, buffer: *mut u16, file: *mut *mut u16) -> u32;
    fn GetTempPathW(length: u32, buffer: *mut u16) -> u32;
    fn LocalFree(memory: Handle) -> Handle;
}

#[link(name = "advapi32")]
extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(text: *const u16, revision: u32, descriptor: *mut *mut c_void, length: *mut u32) -> Bool;
}

/// The text as Windows takes it: UTF-16 ended by a zero.
fn wide(text: &str) -> Vec<u16> {
    std::ffi::OsStr::new(text).encode_wide().chain([0]).collect()
}

/// A path as Windows takes it. One too long for the old limit of 260 characters is made whole and
/// given with `\\?\` in front, as Rust's own functions of files give it, so that it opens on every
/// Windows whether long paths are turned on or not.
fn wide_path(path: &str) -> Vec<u16> {
    let plain = wide(path);
    if plain.len() < 248 || path.starts_with(r"\\?\") {
        return plain;
    }
    let mut full = vec![0u16; 32768];
    // SAFETY: `full` has room for the length given; the name ends with a zero.
    let n = unsafe { GetFullPathNameW(plain.as_ptr(), full.len() as u32, full.as_mut_ptr(), std::ptr::null_mut()) } as usize;
    if n == 0 || n >= full.len() {
        return plain;
    }
    full.truncate(n);
    let text = String::from_utf16_lossy(&full);
    let verbatim = match text.strip_prefix(r"\\") {
        Some(share) => format!(r"\\?\UNC\{share}"),
        None => format!(r"\\?\{text}"),
    };
    wide(&verbatim)
}

/// `Entropy.fill(ref bytes)`: the random bytes of the system, from `ProcessPrng` as compiled code
/// reads them (what Rust reads too). Its library is loaded from the folder of the system, once.
/// `false` if the system gave none.
pub(crate) fn entropy(bytes: &mut [u8]) -> bool {
    type ProcessPrng = unsafe extern "system" fn(*mut u8, usize) -> Bool;
    static FOUND: OnceLock<Option<ProcessPrng>> = OnceLock::new();
    let found = FOUND.get_or_init(|| {
        const SEARCH_SYSTEM32: u32 = 0x800;
        let name = wide("bcryptprimitives.dll");
        // SAFETY: the name ends with a zero; the library stays loaded for the process.
        let module = unsafe { LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), SEARCH_SYSTEM32) };
        if module.is_null() {
            return None;
        }
        // SAFETY: as above; the function has the type Windows gives it.
        let address = unsafe { GetProcAddress(module, c"ProcessPrng".as_ptr()) };
        (!address.is_null()).then(|| unsafe { std::mem::transmute::<*mut c_void, ProcessPrng>(address) })
    });
    match found {
        // SAFETY: the buffer holds `len` bytes; `ProcessPrng` never fails.
        Some(fill) => unsafe { fill(bytes.as_mut_ptr(), bytes.len()) != 0 },
        None => false,
    }
}

/// `Clock.monotonic()` and `Clock.wall()` in nanoseconds: `QueryPerformanceCounter`, and the time
/// since 1970 from `GetSystemTimePreciseAsFileTime`, as compiled code reads them.
pub(crate) fn clock(wall: bool) -> i128 {
    if wall {
        let mut time = 0u64;
        // SAFETY: `time` lives across the call.
        unsafe { GetSystemTimePreciseAsFileTime(&mut time) };
        // In units of 100 ns since 1601.
        return (i128::from(time) - 116_444_736_000_000_000) * 100;
    }
    let (mut count, mut frequency) = (0i64, 0i64);
    // SAFETY: both live across the calls, which never fail from Windows XP on.
    unsafe {
        QueryPerformanceCounter(&mut count);
        QueryPerformanceFrequency(&mut frequency);
    }
    i128::from(count) * 1_000_000_000 / i128::from(frequency.max(1))
}

/// What a later slice brings: nothing reaches it, since the checker refuses the program first.
#[cold]
fn pending(what: &str) -> ! {
    panic!("{what} is not on Windows yet, and the checker should have refused the program")
}

pub(crate) fn executable(_path: &str) -> i128 {
    pending("running another program")
}

pub(crate) fn command_of(_values: &[Value]) -> (std::process::Command, i128) {
    pending("running another program")
}

pub(crate) fn wait_unreaped(_pid: i64) {
    pending("running another program")
}

pub(crate) fn ended_as(_status: std::process::ExitStatus) -> (i64, i64) {
    pending("running another program")
}

pub(crate) fn signal(_pid: i64, _signal: i64) {
    pending("a signal")
}

/// The descriptor of a pipe or a socket, and the file or the socket of one.
pub(crate) type RawFd = i32;
pub(crate) fn fd_of<T>(_: T) -> RawFd {
    pending("a pipe or a socket")
}
pub(crate) unsafe fn file_of_fd(_fd: RawFd) -> std::fs::File {
    pending("a pipe")
}
pub(crate) unsafe fn udp_of_fd(_fd: RawFd) -> std::net::UdpSocket {
    pending("the network")
}

/// Libraries of C, which come in a later slice: none opens yet.
pub(crate) unsafe fn dlopen(_path: *const c_char, _flags: c_int) -> *mut c_void {
    std::ptr::null_mut()
}
pub(crate) unsafe fn dlsym(_handle: *mut c_void, _name: *const c_char) -> *mut c_void {
    std::ptr::null_mut()
}
pub(crate) unsafe fn dlerror() -> *const c_char {
    c"calls of C come to Windows in the 101st slice".as_ptr()
}

/// Codes of the system the interpreter gives itself where the system gave none: no such file
/// (`ERROR_FILE_NOT_FOUND`), a file where one is to be made (`ERROR_ALREADY_EXISTS`), a file
/// where a folder was wanted (`ERROR_DIRECTORY`), a folder where a file was
/// (`ERROR_DIRECTORY_NOT_SUPPORTED`): the codes Rust tells these apart by.
pub(crate) const NOT_FOUND: i32 = 2;
pub(crate) const EXISTS: i32 = 183;
pub(crate) const NOT_DIR: i32 = 267;
pub(crate) const IS_DIR: i32 = 336;
/// `WSAEMSGSIZE`: a datagram too long.
pub(crate) const MESSAGE_TOO_LONG: i32 = 10040;

/// The variant of `IoError` for a code of Windows, as compiled code tells them apart
/// (`sg_io_kind`) and as Rust does: its number in the enum, and 5, `Other`, for the rest.
pub(crate) fn io_kind(code: i32) -> usize {
    match code {
        // ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_INVALID_DRIVE, ERROR_BAD_NETPATH,
        // ERROR_BAD_NET_NAME
        2 | 3 | 15 | 53 | 67 => 0,
        // ERROR_ACCESS_DENIED, ERROR_WRITE_PROTECT, ERROR_PRIVILEGE_NOT_HELD
        5 | 19 | 1314 => 1,
        336 => 2,
        // ERROR_HANDLE_DISK_FULL, ERROR_DISK_FULL, ERROR_DISK_QUOTA_EXCEEDED
        39 | 112 | 1295 => 3,
        // WSAECONNREFUSED
        10061 => 7,
        // WSAECONNRESET, WSAECONNABORTED, WSAENOTCONN, ERROR_BROKEN_PIPE, ERROR_NO_DATA
        10054 | 10053 | 10057 | 109 | 232 => 8,
        // WSAEADDRINUSE
        10048 => 9,
        // ERROR_FILE_EXISTS, ERROR_ALREADY_EXISTS
        80 | 183 => 12,
        267 => 13,
        // ERROR_DIR_NOT_EMPTY
        145 => 14,
        // ERROR_INVALID_PARAMETER, ERROR_INVALID_NAME
        87 | 123 => 15,
        MESSAGE_TOO_LONG => 16,
        _ => 5,
    }
}

/// The last error of Windows as Rust's error.
fn last_error() -> std::io::Error {
    std::io::Error::last_os_error()
}

/// A handle of the file or the folder itself, a link not followed, to ask about it or to delete it.
fn open_itself(path: &str, access: u32) -> std::io::Result<Handle> {
    const SHARE_ALL: u32 = 0x7;
    const OPEN_EXISTING: u32 = 3;
    const BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let name = wide_path(path);
    // SAFETY: the name ends with a zero.
    let handle = unsafe { CreateFileW(name.as_ptr(), access, SHARE_ALL, std::ptr::null(), OPEN_EXISTING, BACKUP_SEMANTICS | OPEN_REPARSE_POINT, std::ptr::null_mut()) };
    match handle == INVALID_HANDLE {
        true => Err(last_error()),
        false => Ok(handle),
    }
}

/// What Windows says of the file the handle is of.
fn info_of(handle: Handle) -> std::io::Result<ByHandleFileInformation> {
    let mut info = ByHandleFileInformation::default();
    // SAFETY: `info` lives across the call.
    match unsafe { GetFileInformationByHandle(handle, &mut info) } {
        0 => Err(last_error()),
        _ => Ok(info),
    }
}

const DIRECTORY: u32 = 0x10;
const REPARSE_POINT: u32 = 0x400;

/// Deletes the file, a link itself and not what it points to. The name goes at once, even while
/// another program has the file open (POSIX semantics, from Windows 10 1809), and a file that may
/// only be read is deleted as on Linux and macOS. A folder is `IsDirectory`. A file system without
/// POSIX semantics (FAT) deletes it the old way.
pub(crate) fn remove_file(path: &str) -> std::io::Result<()> {
    const DELETE: u32 = 0x0001_0000;
    const DISPOSITION_EX: i32 = 21;
    const DISPOSITION: i32 = 4;
    // FILE_DISPOSITION_FLAG_DELETE, _POSIX_SEMANTICS, _IGNORE_READONLY_ATTRIBUTE
    const FLAGS: u32 = 0x1 | 0x2 | 0x10;
    let handle = open_itself(path, DELETE)?;
    let result = (|| {
        let info = info_of(handle)?;
        if info.attributes & DIRECTORY != 0 && info.attributes & REPARSE_POINT == 0 {
            return Err(std::io::Error::from_raw_os_error(IS_DIR));
        }
        // SAFETY: the flags live across the call, with the size given.
        if unsafe { SetFileInformationByHandle(handle, DISPOSITION_EX, (&FLAGS as *const u32).cast(), 4) } != 0 {
            return Ok(());
        }
        let delete: u8 = 1;
        // SAFETY: as above.
        match unsafe { SetFileInformationByHandle(handle, DISPOSITION, (&delete as *const u8).cast(), 1) } {
            0 => Err(last_error()),
            _ => Ok(()),
        }
    })();
    // SAFETY: the handle is this function's.
    unsafe { CloseHandle(handle) };
    result
}

/// The volume and the number of the file the path names, a link not followed.
fn file_id(path: &str) -> Option<(u32, u32, u32)> {
    let handle = open_itself(path, 0).ok()?;
    let info = info_of(handle);
    // SAFETY: the handle is this function's.
    unsafe { CloseHandle(handle) };
    info.ok().map(|info| (info.volume, info.index_high, info.index_low))
}

/// A rename that fails with `ERROR_ALREADY_EXISTS` when the target is there, in one step of the
/// system. A file onto itself (or onto another name of it) is no move: it is `AlreadyExists` too.
pub(crate) fn rename_new(from: &str, to: &str) -> std::io::Result<()> {
    if let (Some(a), Some(b)) = (file_id(from), file_id(to)) {
        if a == b {
            return Err(std::io::Error::from_raw_os_error(EXISTS));
        }
    }
    let (from, to) = (wide_path(from), wide_path(to));
    // SAFETY: both names end with a zero; without flags the target is never replaced.
    match unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) } {
        0 => Err(last_error()),
        _ => Ok(()),
    }
}

/// What the file holds into a new file, with its attributes: `CopyFileExW`, which removes a target
/// it could not write whole. A target that is there is `AlreadyExists`, a folder `IsDirectory`.
pub(crate) fn copy_new(from: &str, to: &str) -> std::io::Result<()> {
    const FAIL_IF_EXISTS: u32 = 0x1;
    if std::fs::metadata(from)?.is_dir() {
        return Err(std::io::Error::from_raw_os_error(IS_DIR));
    }
    let (from, to) = (wide_path(from), wide_path(to));
    // SAFETY: both names end with a zero; no progress function is given.
    match unsafe { CopyFileExW(from.as_ptr(), to.as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), FAIL_IF_EXISTS) } {
        0 => Err(last_error()),
        _ => Ok(()),
    }
}

/// A folder and all it holds: Rust's `remove_dir_all`, which works from the handle of each folder
/// and follows no link or junction. A path that is a link, or a file, is `NotDirectory`.
pub(crate) fn remove_all(path: &str) -> std::io::Result<()> {
    if !std::fs::symlink_metadata(path)?.is_dir() {
        return Err(std::io::Error::from_raw_os_error(NOT_DIR));
    }
    std::fs::remove_dir_all(path)
}

/// A new folder, named from the template with its `XXXXXX` made random, that only its owner may
/// open: its list of access has one entry, full access for the owner, and inherits none.
pub(crate) fn make_temp(template: &str) -> std::io::Result<String> {
    const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let Some(stem) = template.strip_suffix("XXXXXX") else {
        return Err(std::io::Error::from_raw_os_error(87));
    };
    let sddl = wide("D:P(A;OICI;FA;;;OW)");
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: the text ends with a zero; the descriptor is freed below.
    if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), 1, &mut descriptor, std::ptr::null_mut()) } == 0 {
        return Err(last_error());
    }
    let security = SecurityAttributes { length: std::mem::size_of::<SecurityAttributes>() as u32, descriptor, inherit: 0 };
    let made = (|| {
        for _ in 0..100 {
            let mut random = [0u8; 6];
            if !entropy(&mut random) {
                return Err(std::io::Error::other("the system gave no random bytes"));
            }
            let name: String = random.iter().map(|byte| LETTERS[*byte as usize % LETTERS.len()] as char).collect();
            let path = format!("{stem}{name}");
            let wide_name = wide_path(&path);
            // SAFETY: the name ends with a zero; `security` lives across the call.
            if unsafe { CreateDirectoryW(wide_name.as_ptr(), &security) } != 0 {
                return Ok(path);
            }
            let error = last_error();
            if error.raw_os_error() != Some(EXISTS) {
                return Err(error);
            }
        }
        Err(std::io::Error::from_raw_os_error(EXISTS))
    })();
    // SAFETY: the descriptor was given by the call above.
    unsafe { LocalFree(descriptor) };
    made
}

/// The folder for files that do not have to stay, as `GetTempPath2W` gives it where Windows has
/// it (a folder of the system's own for a service), and `GetTempPathW` before; without the `\` at
/// its end.
pub(crate) fn temp_folder() -> String {
    type GetTempPath2W = unsafe extern "system" fn(u32, *mut u16) -> u32;
    static FOUND: OnceLock<Option<GetTempPath2W>> = OnceLock::new();
    let found = FOUND.get_or_init(|| {
        let name = wide("kernel32.dll");
        const SEARCH_SYSTEM32: u32 = 0x800;
        // SAFETY: the name ends with a zero; kernel32 is loaded in every process.
        let module = unsafe { LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), SEARCH_SYSTEM32) };
        // SAFETY: as above; the function has the type Windows gives it.
        let address = unsafe { GetProcAddress(module, c"GetTempPath2W".as_ptr()) };
        (!address.is_null()).then(|| unsafe { std::mem::transmute::<*mut c_void, GetTempPath2W>(address) })
    });
    let mut buffer = vec![0u16; 32768];
    // SAFETY: the buffer has room for the length given.
    let n = unsafe {
        match found {
            Some(get) => get(buffer.len() as u32, buffer.as_mut_ptr()),
            None => GetTempPathW(buffer.len() as u32, buffer.as_mut_ptr()),
        }
    } as usize;
    buffer.truncate(n.min(buffer.len()));
    let mut text = String::from_utf16_lossy(&buffer);
    if text.ends_with('\\') && text.len() > 3 {
        text.pop();
    }
    text
}

/// A new folder in the temporary folder that only this user can open.
pub(crate) fn private_dir() -> std::io::Result<std::path::PathBuf> {
    let template = std::env::temp_dir().join("sg-ffi-XXXXXX");
    make_temp(&template.to_string_lossy()).map(std::path::PathBuf::from)
}

/// The error of a call on the path, as Linux and macOS give it: a folder opened as a file, which
/// Windows refuses as `ERROR_ACCESS_DENIED`, is `IsDirectory`; a path with a file on the way,
/// which Windows does not find (`ERROR_PATH_NOT_FOUND`), is `NotDirectory`.
pub(crate) fn refine(path: &str, error: std::io::Error) -> std::io::Error {
    match error.raw_os_error() {
        Some(5) if std::fs::metadata(path).is_ok_and(|info| info.is_dir()) => std::io::Error::from_raw_os_error(IS_DIR),
        Some(3) if file_on_the_way(path) => std::io::Error::from_raw_os_error(NOT_DIR),
        _ => error,
    }
}

/// Whether the nearest folder on the way that is there is a file.
fn file_on_the_way(path: &str) -> bool {
    let mut on = std::path::Path::new(path).parent();
    while let Some(folder) = on {
        if folder.as_os_str().is_empty() {
            return false;
        }
        if let Ok(info) = std::fs::metadata(folder) {
            return !info.is_dir();
        }
        on = folder.parent();
    }
    false
}
