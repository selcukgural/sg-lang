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

/// `DirectoryRoot` on Windows (see `root.rs`).
#[path = "root_windows.rs"]
pub(crate) mod root;

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
    fn ReOpenFile(original: Handle, access: u32, share: u32, flags: u32) -> Handle;
    fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> Bool;
    fn CopyFileExW(from: *const u16, to: *const u16, progress: *const c_void, data: *const c_void, cancel: *mut Bool, flags: u32) -> Bool;
    fn CreateDirectoryW(name: *const u16, security: *const SecurityAttributes) -> Bool;
    fn GetFullPathNameW(name: *const u16, length: u32, buffer: *mut u16, file: *mut *mut u16) -> u32;
    fn GetTempPathW(length: u32, buffer: *mut u16) -> u32;
    fn LocalFree(memory: Handle) -> Handle;
    fn OpenProcess(access: u32, inherit: Bool, pid: u32) -> Handle;
    fn GetExitCodeProcess(process: Handle, code: *mut u32) -> Bool;
    fn TerminateProcess(process: Handle, code: u32) -> Bool;
    fn GenerateConsoleCtrlEvent(event: u32, group: u32) -> Bool;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn GetStdHandle(which: u32) -> Handle;
    fn GetConsoleMode(console: Handle, mode: *mut u32) -> Bool;
    fn SetConsoleMode(console: Handle, mode: u32) -> Bool;
    fn GetCurrentProcess() -> Handle;
    fn GetProcessAffinityMask(process: Handle, mine: *mut usize, system: *mut usize) -> Bool;
    fn GetActiveProcessorCount(group: u16) -> u32;
    fn GetFileType(file: Handle) -> u32;
    fn GetFileInformationByHandleEx(handle: Handle, class: i32, info: *mut c_void, size: u32) -> Bool;
    fn DeviceIoControl(handle: Handle, code: u32, input: *const c_void, input_size: u32, output: *mut c_void, output_size: u32, returned: *mut u32, overlapped: *mut c_void) -> Bool;
    fn GetLastError() -> u32;
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> Handle;
    fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry) -> Bool;
    fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry) -> Bool;
}

/// `PROCESSENTRY32W`: a program in a snapshot of the system's.
#[repr(C)]
struct ProcessEntry {
    size: u32,
    usage: u32,
    pid: u32,
    heap: usize,
    module: u32,
    threads: u32,
    parent: u32,
    priority: i32,
    flags: u32,
    exe: [u16; 260],
}

#[link(name = "advapi32")]
extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(text: *const u16, revision: u32, descriptor: *mut *mut c_void, length: *mut u32) -> Bool;
}

/// The text as Windows takes it: UTF-16 ended by a zero.
fn wide(text: &str) -> Vec<u16> {
    std::ffi::OsStr::new(text).encode_wide().chain([0]).collect()
}

/// A name written as a text, as Windows takes it (choice 1950): UTF-16 ended by a zero.
fn wide_name(text: &str) -> Vec<u16> {
    crate::names::os_of(text).encode_wide().chain([0]).collect()
}

/// A path as Windows takes it. One too long for the old limit of 260 characters is made whole and
/// given with `\\?\` in front, as Rust's own functions of files give it, so that it opens on every
/// Windows whether long paths are turned on or not. The whole path stays in UTF-16, so a half of a
/// pair in the folder it is made from is kept (1956).
fn wide_path(path: &str) -> Vec<u16> {
    let plain = wide_name(path);
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
    let slash = u16::from(b'\\');
    let mut verbatim: Vec<u16> = r"\\?\".encode_utf16().collect();
    match full.starts_with(&[slash, slash]) {
        true => {
            verbatim.extend("UNC".encode_utf16());
            verbatim.extend_from_slice(&full[1..]);
        }
        false => verbatim.extend_from_slice(&full),
    }
    verbatim.push(0);
    verbatim
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
/// `Task.processorCount()` (1856), as the C runtime counts it: the processors of the process's
/// affinity, or every active one of every group where it has the whole system's. Rust's
/// `available_parallelism` counts the system's only.
pub(crate) fn processor_count() -> usize {
    let (mut mine, mut system) = (0usize, 0usize);
    // SAFETY: the handle of the process itself, and two places to write to.
    if unsafe { GetProcessAffinityMask(GetCurrentProcess(), &mut mine, &mut system) } != 0 && mine != 0 && mine != system {
        return mine.count_ones() as usize;
    }
    // SAFETY: no pointer; `ALL_PROCESSOR_GROUPS` is 0xFFFF.
    let all = unsafe { GetActiveProcessorCount(0xFFFF) };
    if all > 0 { all as usize } else { 1 }
}

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
    match std::fs::metadata(crate::names::os_of(path)) {
        Ok(info) if info.is_file() => 1,
        _ => 0,
    }
}

/// The command of `System.run` and `System.start`: the program at the path, the words after its
/// name (std writes the path as the name, quoted), the folder and the environment, whose names
/// have no case; where its streams go is set by the caller. A cleared
/// environment gets `SystemRoot` back when no change gave it, as compiled code gives it. Only the
/// three streams reach the program: std makes every handle it opens one no program inherits.
pub(crate) fn command_of(values: &[Value]) -> std::process::Command {
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
    // The names as Windows writes them (choice 1950).
    let os = crate::names::os_of;
    let words = texts(1);
    let mut command = std::process::Command::new(os(&text(0)));
    command.args(words[1..].iter().map(|word| os(word)));
    let folder = text(2);
    if !folder.is_empty() {
        command.current_dir(os(&folder));
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
            Value::Bool(true) => command.env(os(name), os(&value)),
            _ => command.env_remove(os(name)),
        };
    }
    command
}

const SYNCHRONIZE: u32 = 0x0010_0000;
const PROCESS_TERMINATE: u32 = 0x0001;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

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
    std::fs::metadata(crate::names::os_of(&folder)).err().unwrap_or(error)
}

pub(crate) fn ended_as(status: std::process::ExitStatus) -> (i64, i64) {
    (0, i64::from(status.code().unwrap_or(0)))
}

/// Whether a signal (by its number on Linux) leaves a running program as it is on Windows: those
/// whose default on Linux and macOS does nothing to one (`WindowChange` 28, `Child` 17, `Continue`
/// 18) and those that pause it (`SIGSTOP` 19 and the three of a terminal, 20 to 22), since
/// Windows pauses no program.
pub(crate) fn inert(signal: i64) -> bool {
    matches!(signal, 17..=22 | 28)
}

/// A group of its own (2023, `CREATE_NEW_PROCESS_GROUP`) or no console (`newSession`, 2024,
/// `DETACHED_PROCESS`); Windows has no terminal to put a group in front at, so `ways[7]` is only
/// the group it makes.
pub(crate) fn grouped(command: &mut std::process::Command, ways: &[i128]) -> std::io::Result<Option<std::fs::File>> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x200;
    const DETACHED_PROCESS: u32 = 0x8;
    let way = |at: usize| ways.get(at).copied().unwrap_or(0);
    let mut flags = 0;
    if way(5) != 0 {
        flags |= CREATE_NEW_PROCESS_GROUP;
    }
    if way(6) != 0 {
        flags |= DETACHED_PROCESS;
    }
    if flags != 0 {
        command.creation_flags(flags);
    }
    Ok(None)
}

/// `System.terminalFront`: a console of Windows has no group in front; nothing.
pub(crate) fn terminal_front(_group: i64) -> std::io::Result<()> {
    Ok(())
}

/// `System.signalGroup`: `Interrupt` (2) is Ctrl-Break to the group, the one event Windows sends
/// a group; another signal goes to the first program, as `signal` sends it.
pub(crate) fn signal_group(pid: i64, group: i64, number: i64) {
    const CTRL_BREAK_EVENT: u32 = 1;
    if number == 2 {
        // SAFETY: sends an event to the programs of a group of this console.
        unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, group as u32) };
    } else {
        signal(pid, number);
    }
}

/// `System.signal`: every signal but those `inert` names ends the program with the code 1, since
/// Windows has no polite stop to send another program.
pub(crate) fn signal(pid: i64, signal: i64) {
    if inert(signal) {
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

/// `Process.parentId()`: the number of the program that started this one, from a snapshot of
/// the programs of the system; 0 when it cannot be found.
pub(crate) fn parent_id() -> i64 {
    const TH32CS_SNAPPROCESS: u32 = 2;
    let me = std::process::id();
    // SAFETY: the entry is as large as it says, and the snapshot is closed after the walk.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot as isize == -1 {
            return 0;
        }
        let mut entry: ProcessEntry = std::mem::zeroed();
        entry.size = std::mem::size_of::<ProcessEntry>() as u32;
        let mut parent = 0;
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            if entry.pid == me {
                parent = i64::from(entry.parent);
                break;
            }
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
        parent
    }
}

/// `Process.signalId`: as `signal`, to a program this one need not have started. No program of
/// that number is `NotFound`; one that may not be stopped `PermissionDenied`; one that has ended
/// but is still known (its handle held somewhere) takes it as done, as Linux and macOS do.
pub(crate) fn signal_id(pid: i64, signal: i64) -> std::io::Result<()> {
    const ERROR_INVALID_PARAMETER: u32 = 87;
    const STILL_ACTIVE: u32 = 259;
    let Ok(pid) = u32::try_from(pid) else { return Err(std::io::Error::from_raw_os_error(2)) };
    let access = if inert(signal) { SYNCHRONIZE } else { PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION };
    // SAFETY: the handle is closed after the call.
    unsafe {
        let process = OpenProcess(access, 0, pid);
        if process.is_null() {
            return Err(match GetLastError() {
                ERROR_INVALID_PARAMETER => std::io::Error::from_raw_os_error(2),
                code => std::io::Error::from_raw_os_error(code as i32),
            });
        }
        let mut done = Ok(());
        if !inert(signal) && TerminateProcess(process, 1) == 0 {
            let error = std::io::Error::last_os_error();
            let mut code = 0;
            if GetExitCodeProcess(process, &mut code) == 0 || code == STILL_ACTIVE {
                done = Err(error);
            }
        }
        CloseHandle(process);
        done
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
pub(crate) use super::net::keep_alive;

/// Whether `path` is a point of Windows that is not followed (`ERROR_CANT_ACCESS_FILE`), as the
/// file of a socket of the machine is: std gives what the folder says of it, a file.
pub(crate) fn unfollowed(path: &std::ffi::OsStr, info: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    /// `FILE_ATTRIBUTE_REPARSE_POINT`.
    const REPARSE: u32 = 0x400;
    if info.file_attributes() & REPARSE == 0 || info.is_dir() {
        return false;
    }
    matches!(std::fs::File::open(path), Err(error) if error.raw_os_error() == Some(1920))
}

/// `IO_REPARSE_TAG_SYMLINK`: the tag of a symbolic link; a junction (`IO_REPARSE_TAG_MOUNT_POINT`)
/// and the file of a socket of the machine have others.
const TAG_LINK: u32 = 0xA000_000C;

/// The handle of the path itself, a link not followed, to look at it.
fn itself(path: &std::ffi::OsStr) -> std::io::Result<Handle> {
    /// `FILE_READ_ATTRIBUTES`; `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`;
    /// `OPEN_EXISTING`; `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`.
    const READ_ATTRIBUTES: u32 = 0x80;
    let name: Vec<u16> = path.encode_wide().chain([0]).collect();
    // SAFETY: the name ends with a zero; the handle is closed by the caller.
    let handle = unsafe { CreateFileW(name.as_ptr(), READ_ATTRIBUTES, 7, std::ptr::null(), 3, 0x0200_0000 | 0x0020_0000, std::ptr::null_mut()) };
    match handle == INVALID_HANDLE {
        true => Err(std::io::Error::last_os_error()),
        false => Ok(handle),
    }
}

/// The tag of the point the path itself is, or 0 when it is none.
fn reparse_tag(path: &std::ffi::OsStr) -> u32 {
    let Ok(handle) = itself(path) else { return 0 };
    // `FILE_ATTRIBUTE_TAG_INFO` (`FileAttributeTagInfo`, 9): the attributes and the tag.
    let mut info = [0u32; 2];
    // SAFETY: the handle is open and `info` has the size given.
    let got = unsafe { GetFileInformationByHandleEx(handle, 9, info.as_mut_ptr().cast(), 8) };
    // SAFETY: opened above, closed once.
    unsafe { CloseHandle(handle) };
    match got != 0 && info[0] & 0x400 != 0 {
        true => info[1],
        false => 0,
    }
}

/// `File.createLink(target, path)`: a symbolic link, made without the right of an administrator
/// where Windows lets a user do so (the mode of a developer), and of a folder when the target is a
/// folder, looked for from the folder of the link as Windows will look for it (Go's way). The `/`
/// of a target is written `\`, which Windows follows (choice 1973).
pub(crate) fn create_link(target: &str, path: &str) -> std::io::Result<()> {
    let target = crate::names::os_of(&target.replace('/', "\\"));
    let path = crate::names::os_of(path);
    let from = std::path::Path::new(&path).parent().map(|folder| folder.join(&target)).unwrap_or_else(|| std::path::PathBuf::from(&target));
    let folder = match std::path::Path::new(&target).is_absolute() {
        true => std::fs::metadata(&target),
        false => std::fs::metadata(&from),
    }
    .is_ok_and(|info| info.is_dir());
    match folder {
        true => std::os::windows::fs::symlink_dir(&target, &path),
        false => std::os::windows::fs::symlink_file(&target, &path),
    }
}

/// `File.readLink(path)`: what a symbolic link holds, as Go reads it: the name it substitutes, with
/// `\??\C:\` given as `C:\` and `\??\UNC\` as `\\`. A path that is no link (a junction too)
/// is `InvalidInput` (choice 1975).
pub(crate) fn read_link(path: &str) -> std::io::Result<Vec<u8>> {
    use std::os::windows::ffi::OsStringExt;
    let handle = itself(&crate::names::os_of(path))?;
    // `MAXIMUM_REPARSE_DATA_BUFFER_SIZE`; `FSCTL_GET_REPARSE_POINT`.
    let mut buffer = vec![0u8; 16 * 1024];
    let mut got = 0u32;
    // SAFETY: the handle is open and the buffer has the size given.
    let done = unsafe { DeviceIoControl(handle, 0x0009_00A8, std::ptr::null(), 0, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut got, std::ptr::null_mut()) };
    let error = std::io::Error::last_os_error();
    // SAFETY: opened above, closed once.
    unsafe { CloseHandle(handle) };
    let invalid = || std::io::Error::from(std::io::ErrorKind::InvalidInput);
    if done == 0 {
        // `ERROR_NOT_A_REPARSE_POINT`
        return Err(if error.raw_os_error() == Some(4390) { invalid() } else { error });
    }
    let word = |at: usize| u16::from_le_bytes([buffer[at], buffer[at + 1]]) as usize;
    let tag = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
    if tag != TAG_LINK || got < 20 {
        return Err(invalid());
    }
    let (offset, length) = (word(8), word(10));
    let flags = u32::from_le_bytes([buffer[16], buffer[17], buffer[18], buffer[19]]);
    let start = 20 + offset;
    if start + length > got as usize {
        return Err(invalid());
    }
    let mut name: Vec<u16> = buffer[start..start + length].as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    // `SYMLINK_FLAG_RELATIVE`: a relative target is given as it is.
    if flags & 1 == 0 {
        let prefix: Vec<u16> = r"\??\".encode_utf16().collect();
        if name.starts_with(&prefix) {
            let rest = name[4..].to_vec();
            let unc: Vec<u16> = r"UNC\".encode_utf16().collect();
            if rest.len() >= 2 && rest[1] == u16::from(b':') {
                name = rest;
            } else if rest.starts_with(&unc) {
                name = r"\\".encode_utf16().chain(rest[4..].iter().copied()).collect();
            }
        }
    }
    let text = crate::names::text_of_os(&std::ffi::OsString::from_wide(&name));
    Ok(text.into_bytes())
}

/// The bits of the mode of Unix Windows is given as (choice 1978): only the attribute of a file
/// that may only be read, which takes the writing away from everyone.
pub(crate) fn mode_of(info: &std::fs::Metadata) -> i128 {
    match (info.is_dir(), info.permissions().readonly()) {
        (true, true) => 0o555,
        (true, false) => 0o777,
        (false, true) => 0o444,
        (false, false) => 0o666,
    }
}

/// The kind `File.linkInfo` gives what `symlink_metadata` said of a path: a symbolic link is
/// `Link`; a junction and the file of a socket of the machine are `Other`.
pub(crate) fn link_kind(path: &std::ffi::OsStr, info: &std::fs::Metadata) -> u8 {
    use std::os::windows::fs::MetadataExt;
    if info.file_attributes() & 0x400 != 0 {
        let tag = reparse_tag(path);
        if tag == TAG_LINK {
            return 2;
        }
        // A point that stands for another name (the bit `0x2000_0000`: a junction) and the file
        // of a socket of the machine (`IO_REPARSE_TAG_AF_UNIX`) are `Other`; a point that only
        // marks a file kept by a service (the files of OneDrive) is the file it marks.
        if tag & 0x2000_0000 != 0 || tag == 0x8000_0023 {
            return 3;
        }
    }
    match info.is_dir() {
        true => 1,
        false => 0,
    }
}

/// `File.setPermissions(path, mode)`: the bit of the owner's writing kept as the attribute of a
/// file that may only be read, Go's way; the others are not kept by Windows (choice 1978).
pub(crate) fn set_permissions(path: &str, mode: u32) -> std::io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    // Through a handle of what the path leads to, so that a link is followed as `chmod` follows it:
    // `FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES`, `FILE_FLAG_BACKUP_SEMANTICS` for a folder.
    let file = std::fs::OpenOptions::new().access_mode(0x80 | 0x100).custom_flags(0x0200_0000).open(crate::names::os_of(path))?;
    let mut permissions = file.metadata()?.permissions();
    permissions.set_readonly(mode & 0o200 == 0);
    file.set_permissions(permissions)
}

/// Whether the open file is a file of the disk (`GetFileType`), which alone has a lock (1979).
pub(crate) fn disk_file(file: &std::fs::File) -> bool {
    use std::os::windows::io::AsRawHandle;
    // SAFETY: the handle is the open file's; `FILE_TYPE_DISK` is 1.
    unsafe { GetFileType(file.as_raw_handle()) == 1 }
}

/// The lock of a file (1979): `kind` 0 alone and 1 shared, waited for; 2 alone and 3 shared,
/// tried, `false` where another holds it; 4 let go. `LockFileEx`, through std, as compiled code
/// calls it. A second lock changes the kind, as `flock` does: the lock held is let go first.
pub(crate) fn lock_file(file: &std::fs::File, kind: u8) -> std::io::Result<bool> {
    /// `ERROR_NOT_LOCKED`
    const NOT_LOCKED: i32 = 158;
    let tried = |result: Result<(), std::fs::TryLockError>| match result {
        Ok(()) => Ok(true),
        Err(std::fs::TryLockError::WouldBlock) => Ok(false),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    };
    let unlocked = file.unlock();
    match kind {
        0 => file.lock().map(|()| true),
        1 => file.lock_shared().map(|()| true),
        2 => tried(file.try_lock()),
        3 => tried(file.try_lock_shared()),
        _ => match unlocked {
            Err(error) if error.raw_os_error() != Some(NOT_LOCKED) => Err(error),
            _ => Ok(true),
        },
    }
}

/// Opens the library of wrappers at `path`. It and the DLLs it needs are looked for in the
/// `folders` (the program's own and those of `-L`) and in `System32`, never in the folder the
/// interpreter works in or on `PATH`: a DLL put there would run inside it. The folders stay
/// added for the process, as the library stays loaded.
pub(crate) fn open_library(path: &std::path::Path, folders: &[std::path::PathBuf]) -> Result<*mut c_void, String> {
    const SEARCH_USER_DIRS: u32 = 0x400;
    const SEARCH_SYSTEM32: u32 = 0x800;
    for folder in folders {
        // `AddDllDirectory` takes a path without `\\?\`, worked out in UTF-16 so that no unit is lost.
        let units: Vec<u16> = folder.as_os_str().encode_wide().collect();
        let unc: Vec<u16> = r"\\?\UNC\".encode_utf16().collect();
        let verbatim: Vec<u16> = r"\\?\".encode_utf16().collect();
        let mut name: Vec<u16> = match () {
            () if units.starts_with(&unc) => r"\\".encode_utf16().chain(units[unc.len()..].iter().copied()).collect(),
            () if units.starts_with(&verbatim) => units[verbatim.len()..].to_vec(),
            () => units,
        };
        name.push(0);
        // SAFETY: the name ends with a zero.
        if unsafe { AddDllDirectory(name.as_ptr()) }.is_null() {
            return Err(format!("the folder {} cannot be looked in for DLLs: {}", folder.display(), last_error()));
        }
    }
    let name: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
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
        // WSAECONNRESET, WSAECONNABORTED, WSAENOTCONN, WSAESHUTDOWN (a send after `closeSending`,
        // EPIPE elsewhere), ERROR_BROKEN_PIPE, ERROR_NO_DATA
        10054 | 10053 | 10057 | 10058 | 109 | 232 => 8,
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
        NOT_A_TERMINAL => 21,
        // Codes of the runtime's own: a path that leads out of a root, what is not a regular
        // file, and what cannot be handed to the system.
        crate::root::OUTSIDE_ROOT => 22,
        crate::root::NOT_A_FILE => 20,
        crate::root::INVALID_INPUT => 15,
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
    let handle = open_itself(path, DELETE)?;
    let result = (|| {
        let info = info_of(handle)?;
        if info.attributes & DIRECTORY != 0 && info.attributes & REPARSE_POINT == 0 {
            return Err(std::io::Error::from_raw_os_error(IS_DIR));
        }
        delete_handle(handle)
    })();
    // SAFETY: the handle is this function's.
    unsafe { CloseHandle(handle) };
    result
}

/// Deletes what the handle, opened with the right to delete, is of.
fn delete_handle(handle: Handle) -> std::io::Result<()> {
    const DISPOSITION_EX: i32 = 21;
    const DISPOSITION: i32 = 4;
    // FILE_DISPOSITION_FLAG_DELETE, _POSIX_SEMANTICS, _IGNORE_READONLY_ATTRIBUTE
    const FLAGS: u32 = 0x1 | 0x2 | 0x10;
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
    if std::fs::metadata(crate::names::os_of(from))?.is_dir() {
        return Err(std::io::Error::from_raw_os_error(IS_DIR));
    }
    let (from, to) = (wide_path(from), wide_path(to));
    // SAFETY: both names end with a zero; no progress function is given.
    match unsafe { CopyFileExW(from.as_ptr(), to.as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), FAIL_IF_EXISTS) } {
        0 => Err(last_error()),
        _ => Ok(()),
    }
}

/// `writer.syncData()` (`data`) and `writer.sync()`: Rust's `sync_data` and `sync_all`, which
/// are what compiled code calls: `fdatasync` and `fsync` on Linux, `F_FULLFSYNC` for both on
/// macOS, `FlushFileBuffers` for both on Windows.
pub(crate) fn sync_file(file: &std::fs::File, data: bool) -> std::io::Result<()> {
    match data {
        true => file.sync_data(),
        false => file.sync_all(),
    }
}

/// `File.openRegular(path)` (1873): a file of the disk opened to read; a folder is `IS_DIR`, and
/// anything else `None`. A folder is told before it is opened; a device (`NUL`, `COM1`) or a pipe
/// is known only by its handle (`GetFileType`), so it is opened and closed again, as compiled code
/// does.
pub(crate) fn open_regular(path: &str) -> std::io::Result<Option<std::fs::File>> {
    use std::os::windows::io::AsRawHandle;
    const FILE_TYPE_DISK: u32 = 1;
    let path = crate::names::os_of(path);
    if std::fs::metadata(&path).is_ok_and(|info| info.is_dir()) {
        return Err(std::io::Error::from_raw_os_error(IS_DIR));
    }
    let file = std::fs::File::open(&path)?;
    // SAFETY: the handle is the open file's; `GetFileType` only asks what it is.
    if unsafe { GetFileType(file.as_raw_handle() as Handle) } != FILE_TYPE_DISK {
        return Ok(None);
    }
    if file.metadata()?.is_dir() {
        return Err(std::io::Error::from_raw_os_error(IS_DIR));
    }
    Ok(Some(file))
}

/// `writer.setLength(n)`: Rust's `set_len`, which needs the right to write the file's data. A writer
/// of `openAppend` holds only the right to append, as compiled code's does, so the file is opened
/// once more through its handle (`ReOpenFile`) with the right to write, and its end set there; the
/// writer still appends.
pub(crate) fn set_length(file: &std::fs::File, length: u64) -> std::io::Result<()> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    const ACCESS_DENIED: i32 = 5;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const SHARE_ALL: u32 = 7;
    match file.set_len(length) {
        Err(error) if error.raw_os_error() == Some(ACCESS_DENIED) => {
            // SAFETY: the handle is the file's, open while `file` is.
            let again = unsafe { ReOpenFile(file.as_raw_handle() as Handle, GENERIC_WRITE, SHARE_ALL, 0) };
            if again as isize == -1 {
                return Err(std::io::Error::last_os_error());
            }
            // SAFETY: a new handle that nothing else owns; the `File` closes it.
            unsafe { std::fs::File::from_raw_handle(again as _) }.set_len(length)
        }
        other => other,
    }
}

/// `Directory.sync(path)`: Windows keeps the names of a folder with the files themselves, so
/// there is nothing to do but to tell a path that is not a folder.
pub(crate) fn sync_folder(path: &str) -> std::io::Result<()> {
    match std::fs::metadata(crate::names::os_of(path)) {
        Ok(info) if info.is_dir() => Ok(()),
        Ok(_) => Err(std::io::Error::from_raw_os_error(NOT_DIR)),
        Err(error) => Err(refine(path, error)),
    }
}

/// A folder and all it holds: Rust's `remove_dir_all`, which works from the handle of each folder
/// and follows no link or junction. A path that is a link, or a file, is `NotDirectory`.
pub(crate) fn remove_all(path: &str) -> std::io::Result<()> {
    let path = crate::names::os_of(path);
    if !std::fs::symlink_metadata(&path)?.is_dir() {
        return Err(std::io::Error::from_raw_os_error(NOT_DIR));
    }
    std::fs::remove_dir_all(&path)
}

/// A new folder, or with `file` a new empty file, named from the template with its `XXXXXX` made
/// random, that only its owner may open: its list of access has one entry, full access for the
/// owner, and inherits none.
pub(crate) fn make_temp(template: &str, file: bool) -> std::io::Result<String> {
    make_temp_kept(template, file, None)
}

/// `File.openTemp()` (2076): a file made from the template as `make_temp` makes one, kept open to
/// be read and written, which Windows deletes when its last handle is closed
/// (`FILE_FLAG_DELETE_ON_CLOSE`). Its name is seen in the folder until then.
pub(crate) fn open_temp(template: &str) -> std::io::Result<std::fs::File> {
    let mut kept = INVALID_HANDLE;
    make_temp_kept(template, true, Some(&mut kept))?;
    // SAFETY: the handle was just opened and is nobody else's.
    Ok(unsafe { std::os::windows::io::FromRawHandle::from_raw_handle(kept) })
}

/// `writer.openReader()` (2074): the file of the writer opened once more through its handle, so
/// that a read does not move where the writer writes.
pub(crate) fn reader_of(file: &std::fs::File) -> std::io::Result<std::fs::File> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    const GENERIC_READ: u32 = 0x8000_0000;
    // SAFETY: the handle is open; the new one is this value's own.
    let handle = unsafe { ReOpenFile(file.as_raw_handle(), GENERIC_READ, 7, 0) };
    match handle == INVALID_HANDLE {
        true => Err(last_error()),
        false => Ok(unsafe { std::fs::File::from_raw_handle(handle) }),
    }
}

/// A read at a place of the file: the read of a reader `writer.openReader()` gave, which keeps
/// its place itself.
pub(crate) fn read_at(file: &std::fs::File, into: &mut [u8], at: u64) -> std::io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, into, at)
}

/// `make_temp`; with `keep`, the file stays open in it, to be read and written, and goes when it
/// is closed.
fn make_temp_kept(template: &str, file: bool, mut keep: Option<&mut Handle>) -> std::io::Result<String> {
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
                // GENERIC_READ | GENERIC_WRITE | DELETE; FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_DELETE_ON_CLOSE
                let (access, flags) = match keep.is_some() {
                    true => (0x8000_0000 | GENERIC_WRITE | 0x0001_0000, 0x100 | 0x0400_0000),
                    false => (GENERIC_WRITE, NORMAL),
                };
                // SAFETY: the name ends with a zero; `security` lives across the call.
                let handle = unsafe { CreateFileW(wide_name.as_ptr(), access, SHARE_ALL, (&security as *const SecurityAttributes).cast(), CREATE_NEW, flags, std::ptr::null_mut()) };
                if handle != INVALID_HANDLE {
                    match keep.as_mut() {
                        Some(kept) => **kept = handle,
                        // SAFETY: the handle was just opened and is closed once.
                        None => unsafe {
                            CloseHandle(handle);
                        },
                    }
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
    // Without the `\` at its end, as compiled code gives it; a half of a pair in it is kept (1956).
    if buffer.len() > 3 && buffer.last() == Some(&u16::from(b'\\')) {
        buffer.pop();
    }
    use std::os::windows::ffi::OsStringExt;
    crate::names::text_of_os(&std::ffi::OsString::from_wide(&buffer))
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
        Some(5) if std::fs::metadata(crate::names::os_of(path)).is_ok_and(|info| info.is_dir()) => std::io::Error::from_raw_os_error(IS_DIR),
        Some(3) if file_on_the_way(path) => std::io::Error::from_raw_os_error(NOT_DIR),
        _ => error,
    }
}

/// Whether the nearest folder on the way that is there is a file.
fn file_on_the_way(path: &str) -> bool {
    let path = std::path::PathBuf::from(crate::names::os_of(path));
    let mut on = path.parent();
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

// The pseudo console of Windows 10 1809 and later (ConPTY) for a program started at a terminal
// of its own (2043). std's `Command` cannot give a program one, so the program is started here
// with `CreateProcessW`, from what `command_of` made of its words, folder and environment.

#[repr(C)]
struct StartupInfo {
    cb: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: u32,
    y: u32,
    x_size: u32,
    y_size: u32,
    x_chars: u32,
    y_chars: u32,
    fill: u32,
    flags: u32,
    show: u16,
    reserved2_size: u16,
    reserved2: *mut u8,
    input: Handle,
    output: Handle,
    error: Handle,
}

#[repr(C)]
struct StartupInfoEx {
    info: StartupInfo,
    attributes: *mut c_void,
}

#[repr(C)]
struct ProcessInformation {
    process: Handle,
    thread: Handle,
    pid: u32,
    tid: u32,
}

/// `COORD`: columns, then rows.
#[repr(C)]
#[derive(Clone, Copy)]
struct Coord {
    x: i16,
    y: i16,
}

#[repr(C)]
#[derive(Default)]
struct SmallRect {
    left: i16,
    top: i16,
    right: i16,
    bottom: i16,
}

#[repr(C)]
#[derive(Default)]
struct ScreenBufferInfo {
    size: [i16; 2],
    cursor: [i16; 2],
    attributes: u16,
    window: SmallRect,
    largest: [i16; 2],
}

#[link(name = "kernel32")]
extern "system" {
    fn InitializeProcThreadAttributeList(list: *mut c_void, count: u32, flags: u32, size: *mut usize) -> Bool;
    fn UpdateProcThreadAttribute(list: *mut c_void, flags: u32, attribute: usize, value: *const c_void, size: usize, previous: *mut c_void, returned: *mut usize) -> Bool;
    fn DeleteProcThreadAttributeList(list: *mut c_void);
    fn CreateProcessW(
        application: *const u16,
        line: *mut u16,
        process_security: *const c_void,
        thread_security: *const c_void,
        inherit: Bool,
        flags: u32,
        environment: *const c_void,
        folder: *const u16,
        startup: *const StartupInfo,
        information: *mut ProcessInformation,
    ) -> Bool;
    fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
    fn GetConsoleScreenBufferInfo(console: Handle, info: *mut ScreenBufferInfo) -> Bool;
}

type PtyCreate = unsafe extern "system" fn(Coord, Handle, Handle, u32, *mut isize) -> i32;
type PtyResize = unsafe extern "system" fn(isize, Coord) -> i32;
type PtyClose = unsafe extern "system" fn(isize);

/// The three calls of a pseudo console, found in `kernel32.dll` when first asked for; `None` on
/// a Windows without them.
fn pty_calls() -> Option<(PtyCreate, PtyResize, PtyClose)> {
    static CALLS: OnceLock<Option<(usize, usize, usize)>> = OnceLock::new();
    const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x800;
    let found = CALLS.get_or_init(|| {
        let name = wide("kernel32.dll");
        // SAFETY: the name ends with a zero; the names looked for end with one too.
        unsafe {
            let kernel = LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
            if kernel.is_null() {
                return None;
            }
            let create = GetProcAddress(kernel, c"CreatePseudoConsole".as_ptr());
            let resize = GetProcAddress(kernel, c"ResizePseudoConsole".as_ptr());
            let close = GetProcAddress(kernel, c"ClosePseudoConsole".as_ptr());
            if create.is_null() || resize.is_null() || close.is_null() {
                return None;
            }
            Some((create as usize, resize as usize, close as usize))
        }
    });
    // SAFETY: the addresses are of these functions, whose types are as Windows declares them.
    found.map(|(create, resize, close)| unsafe {
        (std::mem::transmute::<usize, PtyCreate>(create), std::mem::transmute::<usize, PtyResize>(resize), std::mem::transmute::<usize, PtyClose>(close))
    })
}

/// A failed call of a pseudo console: its `HRESULT` as the code of Windows inside it.
fn hresult_error(result: i32) -> std::io::Error {
    match (result as u32) & 0xFFFF_0000 {
        0x8007_0000 => std::io::Error::from_raw_os_error((result as u32 & 0xFFFF) as i32),
        _ => std::io::Error::from_raw_os_error(31),
    }
}

/// `ERROR_NOT_SUPPORTED`: a Windows with no pseudo console.
const NOT_SUPPORTED: i32 = 50;

/// A program started at a pseudo console, which std's `Child` cannot hold: its handle, closed
/// where this ends.
pub(crate) struct ConsoleChild {
    process: isize,
    pid: u32,
}

impl ConsoleChild {
    pub(crate) fn id(&self) -> u32 {
        self.pid
    }

    /// How it ended, once it has, waiting when `block`.
    fn ended(&mut self, block: bool) -> std::io::Result<Option<std::process::ExitStatus>> {
        use std::os::windows::process::ExitStatusExt;
        const WAIT_OBJECT_0: u32 = 0;
        // SAFETY: the handle is this program's until it ends.
        unsafe {
            if WaitForSingleObject(self.process as Handle, if block { u32::MAX } else { 0 }) != WAIT_OBJECT_0 {
                return Ok(None);
            }
            let mut code = 0u32;
            if GetExitCodeProcess(self.process as Handle, &mut code) == 0 {
                return Err(last_error());
            }
            Ok(Some(std::process::ExitStatus::from_raw(code)))
        }
    }

    pub(crate) fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.ended(true).map(|status| status.unwrap_or_else(|| std::os::windows::process::ExitStatusExt::from_raw(1)))
    }

    pub(crate) fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.ended(false)
    }
}

impl Drop for ConsoleChild {
    fn drop(&mut self) {
        // SAFETY: the handle is closed once.
        unsafe { CloseHandle(self.process as Handle) };
    }
}

/// An argument as C's library reads it back, as Rust writes it (and compiled code,
/// `sg_proc_arg`): quoted when it is empty or holds a space or a tab; a quote after `n`
/// backslashes is written after `2n+1`, and the backslashes before the closing quote doubled.
fn push_arg(line: &mut Vec<u16>, arg: &[u16]) {
    let backslash = u16::from(b'\\');
    let quote_mark = u16::from(b'"');
    let quote = arg.is_empty() || arg.iter().any(|c| *c == u16::from(b' ') || *c == u16::from(b'\t'));
    if quote {
        line.push(quote_mark);
    }
    let mut slashes = 0;
    for &c in arg {
        if c == backslash {
            slashes += 1;
        } else {
            if c == quote_mark {
                line.extend(std::iter::repeat_n(backslash, slashes + 1));
            }
            slashes = 0;
        }
        line.push(c);
    }
    if quote {
        line.extend(std::iter::repeat_n(backslash, slashes));
        line.push(quote_mark);
    }
}

/// An argument of a batch file, escaped for `cmd.exe` as Rust escapes it (and `sg_proc_bat_arg`);
/// a `\r` or a `\n` cannot be passed.
fn push_batch_arg(line: &mut Vec<u16>, arg: &[u16]) -> std::io::Result<()> {
    let backslash = u16::from(b'\\');
    let quote_mark = u16::from(b'"');
    let mut quote = arg.is_empty() || arg.last() == Some(&backslash);
    for &c in arg {
        if c == u16::from(b'\r') || c == u16::from(b'\n') {
            return Err(std::io::Error::from_raw_os_error(87));
        }
        let plain = c >= 0x80 || (c as u8).is_ascii_alphanumeric() || b"$#*+-./:?@\\_".contains(&(c as u8));
        if !plain || (0x80..=0x9F).contains(&c) {
            quote = true;
        }
    }
    if quote {
        line.push(quote_mark);
    }
    let mut slashes = 0;
    for &c in arg {
        if c == backslash {
            slashes += 1;
        } else {
            if c == quote_mark {
                line.extend(std::iter::repeat_n(backslash, slashes));
                line.push(quote_mark);
            } else if c == u16::from(b'%') {
                line.extend("%%cd:~,".encode_utf16());
            }
            slashes = 0;
        }
        line.push(c);
    }
    if quote {
        line.extend(std::iter::repeat_n(backslash, slashes));
        line.push(quote_mark);
    }
    Ok(())
}

/// The block of the environment of a program: this program's, or none when cleared, with the
/// changes of the command made, sorted as Windows wants it, without case.
fn environment_block(command: &std::process::Command, clear: bool) -> Vec<u16> {
    let mut entries: Vec<(std::ffi::OsString, std::ffi::OsString)> = match clear {
        true => Vec::new(),
        false => std::env::vars_os().collect(),
    };
    let same = |a: &std::ffi::OsStr, b: &std::ffi::OsStr| a.to_string_lossy().to_uppercase() == b.to_string_lossy().to_uppercase();
    for (name, value) in command.get_envs() {
        entries.retain(|(have, _)| !same(have, name));
        if let Some(value) = value {
            entries.push((name.to_os_string(), value.to_os_string()));
        }
    }
    entries.sort_by_key(|(name, _)| name.to_string_lossy().to_uppercase());
    let mut block = Vec::new();
    for (name, value) in entries {
        block.extend(name.encode_wide());
        block.push(u16::from(b'='));
        block.extend(value.encode_wide());
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

/// Starts the program of `values` (as `System.start` takes them) at a pseudo console of `rows`
/// and `columns`: the program, the end its screen is read from, the end its keys are written to,
/// and the pseudo console, which `close_terminal` closes.
pub(crate) fn start_at_console(values: &[Value], rows: i128, columns: i128) -> std::io::Result<(ConsoleChild, std::fs::File, std::fs::File, i64)> {
    use std::os::windows::io::IntoRawHandle;
    const EXTENDED_STARTUPINFO_PRESENT: u32 = 0x0008_0000;
    const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;
    const PSEUDOCONSOLE: usize = 0x0002_0016;
    let (create, _, close) = pty_calls().ok_or_else(|| std::io::Error::from_raw_os_error(NOT_SUPPORTED))?;
    let command = command_of(values);
    let program = command.get_program().to_os_string();
    let mut app: Vec<u16> = program.encode_wide().collect();
    let mut line = Vec::new();
    let lower = program.to_string_lossy().to_lowercase();
    let batch = lower.trim_end_matches(['.', ' ']).ends_with(".bat") || lower.trim_end_matches(['.', ' ']).ends_with(".cmd");
    if batch {
        // `cmd.exe` from the folder of the system, never from `PATH`.
        let mut system = vec![0u16; 300];
        // SAFETY: the buffer holds 300 characters, as told.
        let n = unsafe { GetSystemDirectoryW(system.as_mut_ptr(), system.len() as u32) } as usize;
        system.truncate(n);
        system.extend("\\cmd.exe".encode_utf16());
        line.extend("cmd.exe /e:ON /v:OFF /d /c \"\"".encode_utf16());
        line.extend(app.iter().copied());
        line.push(u16::from(b'"'));
        for arg in command.get_args() {
            line.push(u16::from(b' '));
            push_batch_arg(&mut line, &arg.encode_wide().collect::<Vec<u16>>())?;
        }
        line.push(u16::from(b'"'));
        app = system;
    } else {
        // The name is quoted and not escaped, as Rust writes it: a quote cannot be in it.
        if app.contains(&u16::from(b'"')) {
            return Err(std::io::Error::from_raw_os_error(87));
        }
        line.push(u16::from(b'"'));
        line.extend(app.iter().copied());
        line.push(u16::from(b'"'));
        for arg in command.get_args() {
            line.push(u16::from(b' '));
            push_arg(&mut line, &arg.encode_wide().collect::<Vec<u16>>());
        }
    }
    app.push(0);
    line.push(0);
    let folder: Option<Vec<u16>> = command.get_current_dir().map(|dir| dir.as_os_str().encode_wide().chain([0]).collect());
    let environment = environment_block(&command, matches!(values[3], Value::Bool(true)));
    let (their_keys, keys) = std::io::pipe()?;
    let (screen, their_screen) = std::io::pipe()?;
    let their_keys = their_keys.into_raw_handle();
    let their_screen = their_screen.into_raw_handle();
    let mut console: isize = 0;
    // SAFETY: the handles are open; the pseudo console keeps its own of them, so they are closed
    // after the call.
    let made = unsafe { create(Coord { x: columns as i16, y: rows as i16 }, their_keys, their_screen, 0, &mut console) };
    // SAFETY: as above.
    unsafe {
        CloseHandle(their_keys);
        CloseHandle(their_screen);
    }
    if made < 0 {
        return Err(hresult_error(made));
    }
    let mut size = 0usize;
    // SAFETY: the list is as large as the first call says; it is deleted after the start, and the
    // pseudo console is closed when the program does not start.
    unsafe {
        InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
        let mut list = vec![0u8; size];
        let attributes = list.as_mut_ptr().cast::<c_void>();
        let fail = |error: std::io::Error| {
            close(console);
            Err(error)
        };
        if InitializeProcThreadAttributeList(attributes, 1, 0, &mut size) == 0 {
            return fail(last_error());
        }
        if UpdateProcThreadAttribute(attributes, 0, PSEUDOCONSOLE, console as *const c_void, std::mem::size_of::<isize>(), std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
            let error = last_error();
            DeleteProcThreadAttributeList(attributes);
            return fail(error);
        }
        let mut start: StartupInfoEx = std::mem::zeroed();
        start.info.cb = std::mem::size_of::<StartupInfoEx>() as u32;
        start.attributes = attributes;
        // Handles that name none, or the program would get those of this one where they were sent
        // elsewhere, and write there rather than to its console.
        const STARTF_USESTDHANDLES: u32 = 0x100;
        start.info.flags = STARTF_USESTDHANDLES;
        start.info.input = INVALID_HANDLE;
        start.info.output = INVALID_HANDLE;
        start.info.error = INVALID_HANDLE;
        let mut information: ProcessInformation = std::mem::zeroed();
        let started = CreateProcessW(
            app.as_ptr(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            environment.as_ptr().cast(),
            folder.as_ref().map_or(std::ptr::null(), |folder| folder.as_ptr()),
            &start.info,
            &mut information,
        );
        let error = last_error();
        DeleteProcThreadAttributeList(attributes);
        if started == 0 {
            return fail(error);
        }
        CloseHandle(information.thread);
        let child = ConsoleChild { process: information.process as isize, pid: information.pid };
        keep_console(console, information.pid);
        let screen = std::fs::File::from(std::os::windows::io::OwnedHandle::from(screen));
        let keys = std::fs::File::from(std::os::windows::io::OwnedHandle::from(keys));
        Ok((child, screen, keys, console as i64))
    }
}

/// A pseudo terminal of Linux and macOS is never made here: `start_at_console` starts the program.
pub(crate) fn open_terminal(_rows: i128, _columns: i128) -> std::io::Result<(std::fs::File, std::fs::File)> {
    Err(std::io::Error::from_raw_os_error(NOT_SUPPORTED))
}

pub(crate) fn controlled(_command: &mut std::process::Command, _at: std::ffi::c_int) {}

/// The pseudo consoles still open, by what names them: a thread waits for the end of the program
/// of each and closes it then, so that the pipe of what it shows comes to its end as a terminal of
/// Linux and macOS does, without waiting for `Process.wait`; `resizeTerminal` resizes only one
/// still here.
static CONSOLES: std::sync::Mutex<Vec<i64>> = std::sync::Mutex::new(Vec::new());

fn consoles() -> std::sync::MutexGuard<'static, Vec<i64>> {
    CONSOLES.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The pseudo console just made for program `pid`, closed once it ends (or by `close_terminal`).
fn keep_console(console: isize, pid: u32) {
    consoles().push(console as i64);
    // SAFETY: the handle only waits, and is closed after.
    let process = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) } as isize;
    if process == 0 {
        return;
    }
    let waiter = std::thread::Builder::new().name("pseudo console".to_string()).spawn(move || {
        // SAFETY: as above.
        unsafe {
            WaitForSingleObject(process as Handle, u32::MAX);
            CloseHandle(process as Handle);
        }
        close_terminal(console as i64);
    });
    if waiter.is_err() {
        // SAFETY: closed once; the console is then closed only by `close_terminal`.
        unsafe { CloseHandle(process as Handle) };
    }
}

/// `System.resizeTerminal` (2048): `ResizePseudoConsole`; nothing once the program has ended.
pub(crate) fn resize_terminal(terminal: i64, rows: i64, columns: i64) -> std::io::Result<()> {
    let (_, resize, _) = pty_calls().ok_or_else(|| std::io::Error::from_raw_os_error(NOT_SUPPORTED))?;
    let open = consoles();
    if !open.contains(&terminal) {
        return Ok(());
    }
    // SAFETY: the pseudo console is open while it is in the list, which is held.
    match unsafe { resize(terminal as isize, Coord { x: columns as i16, y: rows as i16 }) } {
        made if made < 0 => Err(hresult_error(made)),
        _ => Ok(()),
    }
}

/// `System.closeTerminal` (2042): the pseudo console ends, unless the end of its program closed it
/// already, and with it the pipe of what it shows, once that has been read.
pub(crate) fn close_terminal(terminal: i64) {
    let mut open = consoles();
    let Some(at) = open.iter().position(|kept| *kept == terminal) else { return };
    open.swap_remove(at);
    drop(open);
    if let Some((_, _, close)) = pty_calls() {
        // SAFETY: out of the list, so closed once.
        unsafe { close(terminal as isize) };
    }
}

/// No terminal where one was asked for: `IoError.NotATerminal`, a code of the runtime's own.
pub(crate) const NOT_A_TERMINAL: i32 = -22;

/// `System.terminalSize` (2051): the window of the console the output of this program goes to.
pub(crate) fn terminal_size() -> std::io::Result<(i64, i64)> {
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    let mut info = ScreenBufferInfo::default();
    // SAFETY: the information is written into `info`.
    if unsafe { GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &mut info) } == 0 {
        return Err(std::io::Error::from_raw_os_error(NOT_A_TERMINAL));
    }
    let window = &info.window;
    Ok((i64::from(window.bottom - window.top + 1), i64::from(window.right - window.left + 1)))
}

/// The modes of the console before the first raw mode: input, output, and 1 when the output is a
/// console; put back at the end of the run (2050).
static FIRST_MODES: OnceLock<(u32, u32, bool)> = OnceLock::new();

const STD_INPUT_HANDLE: u32 = -10i32 as u32;
const STD_OUTPUT: u32 = -11i32 as u32;

/// `System.terminalRaw` (2049): the console of the input without line input, echo and Ctrl-C as a
/// signal, giving keys as a terminal sends them; its output reads the sequences of a terminal.
/// `saved` gets the mode of the input, of the output, and 1 when the output is a console, as
/// numbers of 8 bytes, as compiled code keeps them.
pub(crate) fn terminal_raw(saved: &mut [u8; 128]) -> std::io::Result<()> {
    const ENABLE_PROCESSED_INPUT: u32 = 0x1;
    const ENABLE_LINE_INPUT: u32 = 0x2;
    const ENABLE_ECHO_INPUT: u32 = 0x4;
    const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x200;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x4;
    const DISABLE_NEWLINE_AUTO_RETURN: u32 = 0x8;
    // SAFETY: the modes are written into the numbers given.
    unsafe {
        let input = GetStdHandle(STD_INPUT_HANDLE);
        let output = GetStdHandle(STD_OUTPUT);
        let mut in_mode = 0u32;
        let mut out_mode = 0u32;
        if GetConsoleMode(input, &mut in_mode) == 0 {
            return Err(std::io::Error::from_raw_os_error(NOT_A_TERMINAL));
        }
        let console_out = GetConsoleMode(output, &mut out_mode) != 0;
        saved[..8].copy_from_slice(&u64::from(in_mode).to_le_bytes());
        saved[8..16].copy_from_slice(&u64::from(out_mode).to_le_bytes());
        saved[16..24].copy_from_slice(&u64::from(console_out).to_le_bytes());
        FIRST_MODES.get_or_init(|| (in_mode, out_mode, console_out));
        let raw = (in_mode & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT)) | ENABLE_VIRTUAL_TERMINAL_INPUT;
        if SetConsoleMode(input, raw) == 0 {
            return Err(last_error());
        }
        if console_out {
            SetConsoleMode(output, out_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN);
        }
    }
    Ok(())
}

/// `System.terminalRestore`: the modes `terminal_raw` saved.
pub(crate) fn terminal_restore(saved: &[u8; 128]) -> std::io::Result<()> {
    let number = |at: usize| u64::from_le_bytes(saved[at..at + 8].try_into().unwrap_or([0; 8]));
    // SAFETY: modes of the console, as `GetConsoleMode` gave them.
    unsafe {
        if SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), number(0) as u32) == 0 {
            return Err(std::io::Error::from_raw_os_error(NOT_A_TERMINAL));
        }
        if number(16) != 0 {
            SetConsoleMode(GetStdHandle(STD_OUTPUT), number(8) as u32);
        }
    }
    Ok(())
}

/// The console as it was before the first raw mode, put back at the end of the run (2050).
pub(crate) fn terminal_back() {
    if let Some(&(in_mode, out_mode, console_out)) = FIRST_MODES.get() {
        // SAFETY: modes of the console, as `GetConsoleMode` gave them.
        unsafe {
            SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), in_mode);
            if console_out {
                SetConsoleMode(GetStdHandle(STD_OUTPUT), out_mode);
            }
        }
    }
}
