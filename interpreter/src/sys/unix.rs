//! What the interpreter asks Linux and macOS: random bytes, the clocks, the calls of the file
//! system that the standard library of Rust does not make as the language needs them, and other
//! programs. Windows has the same functions in `windows.rs`.


use crate::interp::Value;

/// `Entropy.fill(ref bytes)`: the random bytes of the system, as compiled code reads them
/// (`arc4random_buf` on macOS and the BSDs, `getrandom` on Linux). `false` if the system gave none.
pub(crate) fn entropy(bytes: &mut [u8]) -> bool {
    #[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd"))]
    {
        extern "C" {
            fn arc4random_buf(buf: *mut std::ffi::c_void, len: usize);
        }
        // SAFETY: the buffer holds `len` bytes.
        unsafe { arc4random_buf(bytes.as_mut_ptr().cast(), bytes.len()) };
        true
    }
    #[cfg(not(any(target_os = "macos", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd")))]
    {
        extern "C" {
            fn getrandom(buf: *mut std::ffi::c_void, len: usize, flags: std::ffi::c_uint) -> isize;
        }
        let mut got = 0;
        while got < bytes.len() {
            // SAFETY: the rest of the buffer holds `len - got` bytes.
            let n = unsafe { getrandom(bytes[got..].as_mut_ptr().cast(), bytes.len() - got, 0) };
            if n > 0 {
                got += n as usize;
            } else if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            } else {
                return false;
            }
        }
        true
    }
}

/// `Clock.monotonic()` and `Clock.wall()`: the real clocks, not the clock of the run, so that
/// what a program measures is the time it took. Read with `clock_gettime`, as compiled code reads
/// them, so that the monotonic clock is the same one: on macOS `CLOCK_UPTIME_RAW`, which stops
/// while the machine sleeps, as Linux's `CLOCK_MONOTONIC` and Rust's `Instant` (choice 1674).
/// `Task.processorCount()` (1856): Rust's `available_parallelism`, which the C runtime counts the
/// same way (the affinity and the cgroup's quota on Linux, the processors on line on macOS).
pub(crate) fn processor_count() -> usize {
    std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
}

pub(crate) fn clock(wall: bool) -> i128 {
    #[repr(C)]
    struct Timespec {
        seconds: i64,
        nanoseconds: std::ffi::c_long,
    }
    extern "C" {
        fn clock_gettime(clock: std::ffi::c_int, time: *mut Timespec) -> std::ffi::c_int;
    }
    const REALTIME: std::ffi::c_int = 0;
    #[cfg(target_os = "macos")]
    const MONOTONIC: std::ffi::c_int = 8;
    #[cfg(not(target_os = "macos"))]
    const MONOTONIC: std::ffi::c_int = 1;
    let mut time = Timespec { seconds: 0, nanoseconds: 0 };
    // SAFETY: `time` is a `struct timespec` that lives across the call.
    unsafe { clock_gettime(if wall { REALTIME } else { MONOTONIC }, &mut time) };
    i128::from(time.seconds) * 1_000_000_000 + i128::from(time.nanoseconds)
}

/// `Clock.offsetAt(seconds)`: how many seconds the time zone of the system (`TZ`,
/// `/etc/localtime`) is ahead of UTC at that moment, as `localtime_r` gives the local date and
/// time, which compiled code reads the same way; 0 when the C library does not say.
pub(crate) fn local_offset(seconds: i64) -> i32 {
    use std::ffi::{c_char, c_int, c_long};
    // `struct tm` of glibc and of macOS, which are the same.
    #[repr(C)]
    struct Tm {
        second: c_int,
        minute: c_int,
        hour: c_int,
        day: c_int,
        month: c_int,
        year: c_int,
        weekday: c_int,
        year_day: c_int,
        summer: c_int,
        offset: c_long,
        zone: *const c_char,
    }
    extern "C" {
        fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let mut local = Tm { second: 0, minute: 0, hour: 0, day: 0, month: 0, year: 0, weekday: 0, year_day: 0, summer: 0, offset: 0, zone: std::ptr::null() };
    // SAFETY: both live across the call; `time_t` is 64 bits on the systems the interpreter runs on.
    if unsafe { localtime_r(&seconds, &mut local) }.is_null() {
        return 0;
    }
    let days = days_from_civil(i64::from(local.year) + 1900, i64::from(local.month) + 1, i64::from(local.day));
    let at = days * 86400 + i64::from(local.hour) * 3600 + i64::from(local.minute) * 60 + i64::from(local.second);
    (at - seconds) as i32
}

/// The days from 1970-01-01 to a date of the Gregorian calendar, as `std/time.sg` counts them.
pub(super) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * if month > 2 { month - 3 } else { month + 9 } + 2) / 5 + day - 1;
    era * 146097 + of_era * 365 + of_era / 4 - of_era / 100 + of_year - 719468
}

/// `System.executable(path)`: 1 for a file this program may run, 2 for a file that is there
/// and may not be run, 0 for no file (a folder is none). A link is followed.
pub(crate) fn executable(path: &str) -> i128 {
    use std::os::unix::ffi::OsStrExt;
    if path.contains('\0') {
        return 0;
    }
    match std::fs::metadata(path) {
        Ok(info) if info.is_file() => {
            let Ok(name) = std::ffi::CString::new(std::ffi::OsStr::new(path).as_bytes()) else { return 0 };
            // SAFETY: the name is a text ended by a zero byte; 1 is X_OK.
            match unsafe { access(name.as_ptr(), 1) } {
                0 => 1,
                _ => 2,
            }
        }
        _ => 0,
    }
}

extern "C" {
    fn access(path: *const std::ffi::c_char, mode: std::ffi::c_int) -> std::ffi::c_int;
    fn kill(pid: std::ffi::c_int, signal: std::ffi::c_int) -> std::ffi::c_int;
    fn waitid(idtype: std::ffi::c_int, id: std::ffi::c_uint, info: *mut u8, options: std::ffi::c_int) -> std::ffi::c_int;
    fn fcntl(fd: std::ffi::c_int, cmd: std::ffi::c_int, ...) -> std::ffi::c_int;
    fn sysconf(name: std::ffi::c_int) -> std::ffi::c_long;
    #[cfg(target_os = "linux")]
    fn syscall(number: std::ffi::c_long, ...) -> std::ffi::c_long;
}

/// The highest descriptor the process may have, plus one: `sysconf(_SC_OPEN_MAX)`, at most a
/// million.
fn open_max() -> std::ffi::c_int {
    const SC_OPEN_MAX: std::ffi::c_int = if cfg!(target_os = "linux") { 4 } else { 5 };
    // SAFETY: `sysconf` only reads a limit.
    let max = unsafe { sysconf(SC_OPEN_MAX) };
    std::ffi::c_int::try_from(max.clamp(256, 1 << 20)).unwrap_or(1 << 20)
}

/// Marks every descriptor from 3 up close-on-exec, in the child between `fork` and `exec`: one
/// call of `close_range` on Linux 5.11 and later, otherwise one `fcntl` for each.
fn cloexec_from_3(last: std::ffi::c_int) {
    const F_SETFD: std::ffi::c_int = 2;
    const FD_CLOEXEC: std::ffi::c_int = 1;
    #[cfg(target_os = "linux")]
    {
        const CLOSE_RANGE: std::ffi::c_long = 436;
        const CLOSE_RANGE_CLOEXEC: std::ffi::c_uint = 4;
        // SAFETY: `close_range(3, ~0, CLOSE_RANGE_CLOEXEC)` only changes flags of descriptors.
        if unsafe { syscall(CLOSE_RANGE, 3 as std::ffi::c_uint, std::ffi::c_uint::MAX, CLOSE_RANGE_CLOEXEC) } == 0 {
            return;
        }
    }
    for fd in 3..last {
        // SAFETY: setting the flag of a descriptor that is not open fails and does nothing.
        unsafe { fcntl(fd, F_SETFD, FD_CLOEXEC) };
    }
}

/// The command of `System.run` and `System.start`: the program at the path, the words, the
/// folder and the environment; and where its input comes from (the number the call was given).
/// Only the input, the output and the standard error reach the program: a descriptor the process
/// was handed open (a test runner's pipe) is marked close-on-exec in the child, before the
/// program runs. Marked, not closed, so that Rust's own pipe, which tells the parent why the
/// start failed, still works.
pub(crate) fn command_of(values: &[Value]) -> (std::process::Command, i128) {
    use std::os::unix::process::CommandExt;
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
    command.arg0(&words[0]).args(&words[1..]);
    let folder = text(2);
    if !folder.is_empty() {
        command.current_dir(folder);
    }
    if matches!(values[3], Value::Bool(true)) {
        command.env_clear();
    }
    let kept = items(6);
    for ((name, value), kept) in texts(4).iter().zip(texts(5)).zip(kept) {
        match kept {
            Value::Bool(true) => command.env(name, value),
            _ => command.env_remove(name),
        };
    }
    let input = match &values[7] {
        Value::Int(_, input) => *input,
        _ => unreachable!("the checker makes it a number"),
    };
    let last = open_max();
    // SAFETY: between `fork` and `exec` the closure only calls `fcntl` and the system call of
    // `close_range`, which are safe there, and allocates nothing.
    unsafe {
        command.pre_exec(move || {
            cloexec_from_3(last);
            Ok(())
        });
    }
    (command, input)
}

/// Waits until program `pid` has ended, without taking it away (`waitid` with `WNOWAIT`): its
/// number names no other program until it is waited for.
pub(crate) fn wait_unreaped(pid: i64) {
    const P_PID: std::ffi::c_int = 1;
    const WEXITED: std::ffi::c_int = 4;
    const WNOWAIT: std::ffi::c_int = if cfg!(target_os = "linux") { 0x0100_0000 } else { 0x20 };
    const EINTR: i32 = 4;
    let mut info = [0u8; 256];
    loop {
        // SAFETY: `info` has room for a `siginfo_t` on both systems (128 and 104 bytes).
        let done = unsafe { waitid(P_PID, pid as std::ffi::c_uint, info.as_mut_ptr(), WEXITED | WNOWAIT) };
        if done == 0 || std::io::Error::last_os_error().raw_os_error() != Some(EINTR) {
            return;
        }
    }
}

/// How a program ended, as `System.run` and `System.wait` give it: 0 and the code, or 1 and the
/// signal that stopped it.
/// Why a program did not start, as the system said it.
pub(crate) fn not_started(error: std::io::Error, _folder: &Value) -> std::io::Error {
    error
}

pub(crate) fn ended_as(status: std::process::ExitStatus) -> (i64, i64) {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => (0, i64::from(code)),
        (None, Some(signal)) => (1, i64::from(signal)),
        (None, None) => (0, 0),
    }
}

/// The C string of a path that holds no zero byte.
fn c_path(path: &str) -> std::ffi::CString {
    std::ffi::CString::new(path).expect("a path without a zero byte")
}

/// `unlink`; a folder is `IsDirectory` on every system (macOS says `EPERM`).
pub(crate) fn remove_file(path: &str) -> std::io::Result<()> {
    std::fs::remove_file(path).map_err(|error| match std::fs::symlink_metadata(path) {
        Ok(info) if info.is_dir() => std::io::Error::from_raw_os_error(IS_DIR),
        _ => error,
    })
}

/// A rename that fails with `EEXIST` when the target is there, in one step of the system.
/// A file onto itself (or onto another name of it) is no move, and some systems would say it was
/// done: it is `EEXIST` too.
pub(crate) fn rename_new(from: &str, to: &str) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    if let (Ok(a), Ok(b)) = (std::fs::symlink_metadata(from), std::fs::symlink_metadata(to)) {
        if a.dev() == b.dev() && a.ino() == b.ino() {
            return Err(std::io::Error::from_raw_os_error(EXISTS));
        }
    }
    let (from, to) = (c_path(from), c_path(to));
    #[cfg(target_os = "macos")]
    let failed = {
        extern "C" {
            fn renamex_np(from: *const std::ffi::c_char, to: *const std::ffi::c_char, flags: std::ffi::c_uint) -> std::ffi::c_int;
        }
        // RENAME_EXCL
        unsafe { renamex_np(from.as_ptr(), to.as_ptr(), 4) != 0 }
    };
    #[cfg(not(target_os = "macos"))]
    let failed = {
        extern "C" {
            fn renameat2(from_dir: std::ffi::c_int, from: *const std::ffi::c_char, to_dir: std::ffi::c_int, to: *const std::ffi::c_char, flags: std::ffi::c_uint) -> std::ffi::c_int;
        }
        // AT_FDCWD, RENAME_NOREPLACE
        unsafe { renameat2(-100, from.as_ptr(), -100, to.as_ptr(), 1) != 0 }
    };
    match failed {
        true => Err(std::io::Error::last_os_error()),
        false => Ok(()),
    }
}

/// What the file holds into a new file, which gets its permissions (without the bits of
/// `setuid`, `setgid` and `sticky`). A target that is there is `EEXIST`; a target that could not
/// be written whole is removed.
pub(crate) fn copy_new(from: &str, to: &str) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut source = std::fs::File::open(from)?;
    let info = source.metadata()?;
    if info.is_dir() {
        return Err(std::io::Error::from_raw_os_error(IS_DIR));
    }
    let mut target = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(to)?;
    let written = std::io::copy(&mut source, &mut target)
        .and_then(|_| target.set_permissions(std::fs::Permissions::from_mode(info.permissions().mode() & 0o777)));
    if written.is_err() {
        drop(target);
        let _ = std::fs::remove_file(to);
    }
    written
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

/// `writer.setLength(n)`: `ftruncate`, which a file opened to append may do too.
pub(crate) fn set_length(file: &std::fs::File, length: u64) -> std::io::Result<()> {
    file.set_len(length)
}

/// `Directory.sync(path)`: the folder opened and put on the disk as a file is (`fsync`, and
/// `F_FULLFSYNC` on macOS), so that a name made, renamed or removed in it stays. A path that is a
/// file is `NotDirectory`, as compiled code's `O_DIRECTORY` gives it.
pub(crate) fn sync_folder(path: &str) -> std::io::Result<()> {
    let folder = std::fs::File::open(path)?;
    if !folder.metadata()?.is_dir() {
        return Err(std::io::Error::from_raw_os_error(NOT_DIR));
    }
    folder.sync_all()
}

/// A folder and all it holds, links not followed: a path that is a link, or a file, is `ENOTDIR`.
pub(crate) fn remove_all(path: &str) -> std::io::Result<()> {
    if !std::fs::symlink_metadata(path)?.is_dir() {
        return Err(std::io::Error::from_raw_os_error(NOT_DIR));
    }
    std::fs::remove_dir_all(path)
}

/// `mkdtemp`: a new folder that only the user can go into, its name made from the template; with
/// `file`, `mkstemp`: a new empty file only the user may read and write, closed again.
pub(crate) fn make_temp(template: &str, file: bool) -> std::io::Result<String> {
    extern "C" {
        fn mkdtemp(template: *mut std::ffi::c_char) -> *mut std::ffi::c_char;
        fn mkstemp(template: *mut std::ffi::c_char) -> std::ffi::c_int;
        fn close(fd: std::ffi::c_int) -> std::ffi::c_int;
    }
    let mut name = c_path(template).into_bytes_with_nul();
    if file {
        // SAFETY: the name ends with a zero, and the system writes only over its `XXXXXX`.
        let fd = unsafe { mkstemp(name.as_mut_ptr().cast()) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: the descriptor was just opened and is closed once.
        unsafe { close(fd) };
    } else if unsafe { mkdtemp(name.as_mut_ptr().cast()) }.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    name.pop();
    Ok(String::from_utf8(name).expect("the template is text and the system puts letters in it"))
}

/// Codes of the system the interpreter gives itself where the system gave none: no such file,
/// a file where one is to be made, a file where a folder was wanted, a folder where a file was.
pub(crate) const NOT_FOUND: i32 = 2;
pub(crate) const EXISTS: i32 = 17;
pub(crate) const NOT_DIR: i32 = 20;
pub(crate) const IS_DIR: i32 = 21;
/// EMSGSIZE: a datagram too long.
pub(crate) const MESSAGE_TOO_LONG: i32 = if cfg!(target_os = "linux") { 90 } else { 40 };

/// The variant of `IoError` for a code of the system, as compiled code tells them apart
/// (`sg_io_kind`): its number in the enum, and 5, `Other`, for a code with no variant of its own.
pub(crate) fn io_kind(code: i32) -> usize {
    const QUOTA: i32 = if cfg!(target_os = "linux") { 122 } else { 69 };
    // ECONNREFUSED; ECONNRESET, ECONNABORTED, EPIPE; EADDRINUSE, EADDRNOTAVAIL: their numbers differ.
    const REFUSED: i32 = if cfg!(target_os = "linux") { 111 } else { 61 };
    const RESET: [i32; 2] = if cfg!(target_os = "linux") { [104, 103] } else { [54, 53] };
    const IN_USE: i32 = if cfg!(target_os = "linux") { 98 } else { 48 };
    // EADDRNOTAVAIL: an address not of the machine, or no local port left.
    const NOT_AVAILABLE: i32 = if cfg!(target_os = "linux") { 99 } else { 49 };
    const NOT_EMPTY: i32 = if cfg!(target_os = "linux") { 39 } else { 66 };
    // ENOTCONN: the other side of a connection has broken it off.
    const NOT_CONNECTED: i32 = if cfg!(target_os = "linux") { 107 } else { 57 };
    match code {
        _ if code == REFUSED => 7,
        _ if RESET.contains(&code) || code == 32 || code == NOT_CONNECTED => 8,
        MESSAGE_TOO_LONG => 16,
        // ESPIPE: a pipe cannot seek.
        29 => 17,
        _ if code == IN_USE => 9,
        _ if code == NOT_AVAILABLE => 19,
        _ if code == NOT_EMPTY => 14,
        // ENOENT
        2 => 0,
        // EEXIST
        17 => 12,
        // ENOTDIR
        20 => 13,
        // EPERM, EACCES, EROFS
        1 | 13 | 30 => 1,
        // EISDIR
        21 => 2,
        // ENOSPC, EDQUOT
        28 => 3,
        _ if code == QUOTA => 3,
        _ => 5,
    }
}

/// `kill`: sends the signal to the program.
pub(crate) fn signal(pid: i64, signal: i64) {
    // SAFETY: `kill` only sends a signal.
    unsafe { kill(pid as std::ffi::c_int, signal as std::ffi::c_int) };
}

/// A new folder in the temporary folder that only this user can open.
pub(crate) fn private_dir() -> std::io::Result<std::path::PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let base = std::env::temp_dir();
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.as_nanos());
    for attempt in 0..100u32 {
        let dir = base.join(format!("sg-ffi-{}-{seed:x}-{attempt}", std::process::id()));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other("every name tried was taken"))
}

/// The descriptor of a pipe or a socket, and the file or the socket of one.
pub(crate) type RawFd = std::os::unix::io::RawFd;
pub(crate) fn fd_of<T: std::os::unix::io::IntoRawFd>(owned: T) -> RawFd {
    owned.into_raw_fd()
}
/// # Safety
/// `fd` is open and owned by nothing else.
pub(crate) unsafe fn file_of_fd(fd: RawFd) -> std::fs::File {
    std::os::unix::io::FromRawFd::from_raw_fd(fd)
}
/// # Safety
/// The number of a datagram socket the interpreter keeps, which it owns from here.
pub(crate) fn udp_fd(socket: std::net::UdpSocket) -> RawFd {
    fd_of(socket)
}

/// As `file_of_fd`.
pub(crate) unsafe fn udp_of_fd(fd: RawFd) -> std::net::UdpSocket {
    std::os::unix::io::FromRawFd::from_raw_fd(fd)
}

extern "C" {
    fn dlopen(path: *const std::ffi::c_char, flags: std::ffi::c_int) -> *mut std::ffi::c_void;
    fn dlsym(handle: *mut std::ffi::c_void, name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    fn dlerror() -> *const std::ffi::c_char;
}

/// What `dlerror` says, as text.
fn dl_error() -> String {
    // SAFETY: `dlerror` gives a text that ends in a zero byte, or `null`.
    let text = unsafe { dlerror() };
    if text.is_null() {
        return "nothing".to_string();
    }
    // SAFETY: as above.
    unsafe { std::ffi::CStr::from_ptr(text) }.to_string_lossy().into_owned()
}

/// Opens the library of wrappers at `path`, every symbol found now (`RTLD_NOW`), so that a
/// missing one is an error here and not in the middle of the run. The libraries it needs are
/// found where it was linked to look (`rpath`), so `_folders` is not needed here.
pub(crate) fn open_library(path: &std::path::Path, _folders: &[std::path::PathBuf]) -> Result<*mut std::ffi::c_void, String> {
    /// The same value on Linux and macOS.
    const RTLD_NOW: std::ffi::c_int = 2;
    let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| "the path of the library holds a zero byte".to_string())?;
    // SAFETY: `path` ends in a zero byte; the library was built by the interpreter.
    let handle = unsafe { dlopen(path.as_ptr(), RTLD_NOW) };
    match handle.is_null() {
        true => Err(format!("`dlopen` says: {}", dl_error())),
        false => Ok(handle),
    }
}

/// The address of `name` in a library `open_library` opened.
pub(crate) fn library_symbol(handle: *mut std::ffi::c_void, name: &str) -> Result<*mut std::ffi::c_void, String> {
    let symbol = std::ffi::CString::new(name).expect("no zero byte");
    // SAFETY: `handle` is open and `symbol` ends in a zero byte.
    let found = unsafe { dlsym(handle, symbol.as_ptr()) };
    match found.is_null() {
        true => Err(format!("`dlsym` says: {}", dl_error())),
        false => Ok(found),
    }
}

/// The folder for files that do not have to stay, as the system gives it. Only Windows asks:
/// on Linux and macOS the language's own module reads `TMPDIR` itself.
/// A terminal of Linux or macOS takes the escape sequences of colours as they are (choice 1455).
pub(crate) fn colour_console() -> bool {
    true
}

pub(crate) fn temp_folder() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

/// The error of a call on the path, as Linux and macOS give it already (see `windows.rs`).
pub(crate) fn refine(_path: &str, error: std::io::Error) -> std::io::Error {
    error
}
