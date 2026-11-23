//! What the interpreter asks Windows: random bytes, the clocks and the calls of the file system,
//! as compiled code asks them, through the functions of Windows declared here (no crate); the
//! network is in `net_windows.rs`, the signals in `signals_windows.rs`. Other programs are started
//! through std's `Command`, which quotes the command line as compiled code does and escapes the
//! arguments of a batch file the same way.

use std::ffi::{c_char, c_void};
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
    fn FileTimeToSystemTime(time: *const u64, out: *mut u16) -> Bool;
    fn SystemTimeToFileTime(time: *const u16, out: *mut u64) -> Bool;
    fn GetDynamicTimeZoneInformation(zone: *mut u32) -> u32;
    fn SystemTimeToTzSpecificLocalTimeEx(zone: *const u32, utc: *const u16, local: *mut u16) -> Bool;
    fn LoadLibraryExW(name: *const u16, file: Handle, flags: u32) -> Handle;
    fn GetProcAddress(module: Handle, name: *const c_char) -> *mut c_void;
    fn AddDllDirectory(folder: *const u16) -> *mut c_void;
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
    fn OpenProcess(access: u32, inherit: Bool, pid: u32) -> Handle;
    fn GetExitCodeProcess(process: Handle, code: *mut u32) -> Bool;
    fn TerminateProcess(process: Handle, code: u32) -> Bool;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn GetStdHandle(which: u32) -> Handle;
    fn GetConsoleMode(console: Handle, mode: *mut u32) -> Bool;
    fn SetConsoleMode(console: Handle, mode: u32) -> Bool;
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

/// `Clock.offsetAt(seconds)`: how many seconds the time zone of the system is ahead of UTC at that
/// moment, by the rules Windows keeps for that year, as compiled code reads it; 0 when Windows does
/// not say. `TZ` is not read.
pub(crate) fn local_offset(seconds: i64) -> i32 {
    let units = seconds.wrapping_mul(10_000_000).wrapping_add(116_444_736_000_000_000);
    if units < 0 {
        return 0;
    }
    let (mut utc, mut local) = ([0u16; 8], [0u16; 8]);
    // `DYNAMIC_TIME_ZONE_INFORMATION`, 432 bytes, whose fields lie at multiples of four.
    let mut zone = [0u32; 108];
    let mut there = 0u64;
    // SAFETY: each buffer is as large as the structure Windows writes (`SYSTEMTIME` is eight
    // `WORD`s) and lives across the calls.
    unsafe {
        if FileTimeToSystemTime(&(units as u64), utc.as_mut_ptr()) == 0 || GetDynamicTimeZoneInformation(zone.as_mut_ptr()) == u32::MAX {
            return 0;
        }
        if SystemTimeToTzSpecificLocalTimeEx(zone.as_ptr(), utc.as_ptr(), local.as_mut_ptr()) == 0 || SystemTimeToFileTime(local.as_ptr(), &mut there) == 0 {
            return 0;
        }
    }
    ((there as i64 - units) / 10_000_000) as i32
}

/// `System.executable`: 1 for a file, 0 for none (a folder is none). Windows has no right to run a
/// file apart from the right to read it: whether it runs, starting it says.
pub(crate) fn executable(path: &str) -> i128 {
    match std::fs::metadata(path) {
        Ok(info) if info.is_file() => 1,
        _ => 0,
    }
}

/// The command of `System.run` and `System.start`: the program at the path, the words after its
/// name (std writes the path as the name, quoted), the folder and the environment, whose names
/// have no case; and where its input comes from (the number the call was given). A cleared
/// environment gets `SystemRoot` back when no change gave it, as compiled code gives it. Only the
/// three streams reach the program: std makes every handle it opens one no program inherits.
pub(crate) fn command_of(values: &[Value]) -> (std::process::Command, i128) {
    let text = |at: usize| match &values[at] {
        Value::Str(text) => text.to_string(),
        _ => unreachable!("the checker makes it text"),
    };
    let items = |at: usize| match &values[at] {
        Value::List(items) => items.borrow().iter().map(|item| item.borrow().clone()).collect::<Vec<Value>>(),
        _ => unreachable!("the checker makes it a list"),
    };
    let texts = |at: usize| {
        items(at)
            .into_iter()
            .map(|item| match item {
                Value::Str(text) => text.to_string(),
                _ => unreachable!("the checker makes it a list of texts"),
            })
            .collect::<Vec<String>>()
    };
    let words = texts(1);
    let mut command = std::process::Command::new(text(0));
    command.args(&words[1..]);
    let folder = text(2);
    if !folder.is_empty() {
        command.current_dir(folder);
    }
    let names = texts(4);
    if matches!(values[3], Value::Bool(true)) {
        command.env_clear();
        if !names.iter().any(|name| name.eq_ignore_ascii_case("SystemRoot")) {
            if let Some(root) = std::env::var_os("SystemRoot") {
                command.env("SystemRoot", root);
            }
        }
    }
    let kept = items(6);
    for ((name, value), kept) in names.iter().zip(texts(5)).zip(kept) {
        match kept {
            Value::Bool(true) => command.env(name, value),
            _ => command.env_remove(name),
        };
    }
    let input = match &values[7] {
        Value::Int(_, input) => *input,
        _ => unreachable!("the checker makes it a number"),
    };
    (command, input)
}

const SYNCHRONIZE: u32 = 0x0010_0000;
const PROCESS_TERMINATE: u32 = 0x0001;

/// Waits until program `pid` has ended. Its number names no other program meanwhile: the
/// interpreter keeps its handle until it is waited for.
pub(crate) fn wait_unreaped(pid: i64) {
    // SAFETY: the handle is closed once waited for.
    unsafe {
        let process = OpenProcess(SYNCHRONIZE, 0, pid as u32);
        if !process.is_null() {
            WaitForSingleObject(process, u32::MAX);
            CloseHandle(process);
        }
    }
}

/// How a program ended, as `System.run` and `System.wait` give it: 0 and its code, of 32 bits.
/// Why a program did not start: a folder to work in that is not there is `NotFound` and a file
/// `NotDirectory`, as on Linux and macOS, where Windows says `ERROR_DIRECTORY` of both.
pub(crate) fn not_started(error: std::io::Error, folder: &Value) -> std::io::Error {
    let Value::Str(folder) = folder else { return error };
    let folder = folder.to_string();
    if error.raw_os_error() != Some(NOT_DIR) || folder.is_empty() {
        return error;
    }
    std::fs::metadata(&folder).err().unwrap_or(error)
}

pub(crate) fn ended_as(status: std::process::ExitStatus) -> (i64, i64) {
    (0, i64::from(status.code().unwrap_or(0)))
}

/// `System.signal`: every signal but `WindowChange` (28, which does nothing) ends the program with
/// the code 1, since Windows has no polite stop to send another program.
pub(crate) fn signal(pid: i64, signal: i64) {
    if signal == 28 {
        return;
    }
    // SAFETY: the handle is closed after the call.
    unsafe {
        let process = OpenProcess(PROCESS_TERMINATE, 0, pid as u32);
        if !process.is_null() {
            TerminateProcess(process, 1);
            CloseHandle(process);
        }
    }
}

/// The descriptor of a pipe or a socket, and the file or the socket of one: a socket is its
/// handle, cut to 32 bits (see `net::number_of`).
pub(crate) type RawFd = i32;
pub(crate) fn fd_of<T: std::os::windows::io::IntoRawHandle>(owned: T) -> RawFd {
    owned.into_raw_handle() as isize as RawFd
}
/// # Safety
/// `fd` is a handle that is open and owned by nothing else.
pub(crate) unsafe fn file_of_fd(fd: RawFd) -> std::fs::File {
    std::os::windows::io::FromRawHandle::from_raw_handle(fd as isize as Handle)
}
/// The number of a datagram socket the interpreter keeps, which it owns from here.
pub(crate) fn udp_fd(socket: std::net::UdpSocket) -> RawFd {
    let fd = super::net::number_of(&socket);
    std::mem::forget(socket);
    fd
}
/// SAFETY: as `FromRawSocket`.
pub(crate) unsafe fn udp_of_fd(fd: RawFd) -> std::net::UdpSocket {
    super::net::udp_of(fd)
}

/// Opens the library of wrappers at `path`. It and the DLLs it needs are looked for in the
/// `folders` (the program's own and those of `-L`) and in `System32`, never in the folder the
/// interpreter works in or on `PATH`: a DLL put there would run inside it. The folders stay
/// added for the process, as the library stays loaded.
pub(crate) fn open_library(path: &std::path::Path, folders: &[std::path::PathBuf]) -> Result<*mut c_void, String> {
    const SEARCH_USER_DIRS: u32 = 0x400;
    const SEARCH_SYSTEM32: u32 = 0x800;
    for folder in folders {
        // `AddDllDirectory` takes a path without `\\?\`.
        let text = folder.to_string_lossy();
        let plain = text.strip_prefix(r"\\?\UNC\").map(|rest| format!(r"\\{rest}")).or_else(|| text.strip_prefix(r"\\?\").map(str::to_string)).unwrap_or_else(|| text.to_string());
        let name = wide(&plain);
        // SAFETY: the name ends with a zero.
        if unsafe { AddDllDirectory(name.as_ptr()) }.is_null() {
            return Err(format!("the folder {plain} cannot be looked in for DLLs: {}", last_error()));
        }
    }
    let name = wide(&path.to_string_lossy());
    // SAFETY: the name ends with a zero; the library stays loaded for the process.
    let module = unsafe { LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), SEARCH_USER_DIRS | SEARCH_SYSTEM32) };
    match module.is_null() {
        true => Err(format!("`LoadLibraryExW` says: {}", last_error())),
        false => Ok(module),
    }
}

/// The address of `name` in a library `open_library` opened.
pub(crate) fn library_symbol(handle: *mut c_void, name: &str) -> Result<*mut c_void, String> {
    let symbol = std::ffi::CString::new(name).expect("no zero byte");
    // SAFETY: `handle` is a module that is loaded and `symbol` ends in a zero byte.
    let found = unsafe { GetProcAddress(handle, symbol.as_ptr()) };
    match found.is_null() {
        true => Err(format!("`GetProcAddress` says of `{name}`: {}", last_error())),
        false => Ok(found),
    }
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
        // WSAEADDRNOTAVAIL
        10049 => 19,
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

/// A new folder, or with `file` a new empty file, named from the template with its `XXXXXX` made
/// random, that only its owner may open: its list of access has one entry, full access for the
/// owner, and inherits none.
pub(crate) fn make_temp(template: &str, file: bool) -> std::io::Result<String> {
    const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let Some(stem) = template.strip_suffix("XXXXXX") else {
        return Err(std::io::Error::from_raw_os_error(87));
    };
    let sddl = wide(if file { "D:P(A;;FA;;;OW)" } else { "D:P(A;OICI;FA;;;OW)" });
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
            if file {
                const GENERIC_WRITE: u32 = 0x4000_0000;
                const CREATE_NEW: u32 = 1;
                const NORMAL: u32 = 0x80;
                const SHARE_ALL: u32 = 0x7;
                // SAFETY: the name ends with a zero; `security` lives across the call.
                let handle = unsafe { CreateFileW(wide_name.as_ptr(), GENERIC_WRITE, SHARE_ALL, (&security as *const SecurityAttributes).cast(), CREATE_NEW, NORMAL, std::ptr::null_mut()) };
                if handle != INVALID_HANDLE {
                    // SAFETY: the handle was just opened and is closed once.
                    unsafe { CloseHandle(handle) };
                    return Ok(path);
                }
            // SAFETY: the name ends with a zero; `security` lives across the call.
            } else if unsafe { CreateDirectoryW(wide_name.as_ptr(), &security) } != 0 {
                return Ok(path);
            }
            let error = last_error();
            // A file is ERROR_FILE_EXISTS where it was to be made new.
            if error.raw_os_error() != Some(EXISTS) && error.raw_os_error() != Some(80) {
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
/// Whether the console of the standard error takes the escape sequences of colours, turned on
/// here when it does not yet (`ENABLE_VIRTUAL_TERMINAL_PROCESSING`); an old console that refuses
/// them shows no colour, never the sequences (choice 1455).
pub(crate) fn colour_console() -> bool {
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const VIRTUAL_TERMINAL: u32 = 0x0004;
    let mut mode = 0u32;
    // SAFETY: the handle of the standard error, and a mode read into a local.
    unsafe {
        let console = GetStdHandle(STD_ERROR_HANDLE);
        if GetConsoleMode(console, &mut mode) == 0 {
            return false;
        }
        mode & VIRTUAL_TERMINAL != 0 || SetConsoleMode(console, mode | VIRTUAL_TERMINAL) != 0
    }
}

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
/// Removes the folders of the libraries of wrappers that interpreters which have ended left
/// (see `crate::ffi`). Each folder is named after the process that made it (`private_dir`); one
/// of a process that still runs is left alone, since its DLL may be about to be loaded.
pub(crate) fn remove_old_libraries() {
    const QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    const ACCESS_DENIED: i32 = 5;
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(pid) = name.strip_prefix("sg-ffi-").and_then(|rest| rest.split_once('-')).and_then(|(pid, _)| pid.parse::<u32>().ok()) else { continue };
        if pid == std::process::id() || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        // SAFETY: a handle that is given is closed below.
        let process = unsafe { OpenProcess(QUERY_LIMITED_INFORMATION, 0, pid) };
        let running = match process.is_null() {
            // A process of another user is one that runs.
            true => last_error().raw_os_error() == Some(ACCESS_DENIED),
            false => {
                let mut code = 0u32;
                // SAFETY: `process` is open; it is closed once.
                let asked = unsafe { GetExitCodeProcess(process, &mut code) } != 0;
                unsafe { CloseHandle(process) };
                !asked || code == STILL_ACTIVE
            }
        };
        if !running {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// A new folder that only this user may open, named after this process (`sg-ffi-<pid>-...`),
/// so that `remove_old_libraries` tells whose it is.
pub(crate) fn private_dir() -> std::io::Result<std::path::PathBuf> {
    let template = std::env::temp_dir().join(format!("sg-ffi-{}-XXXXXX", std::process::id()));
    make_temp(&template.to_string_lossy(), false).map(std::path::PathBuf::from)
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
