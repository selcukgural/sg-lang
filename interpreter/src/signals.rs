//! The signals of the system, for the interpreter: what `Signals.listen` and a `SignalListener`
//! do, the way compiled code does them (see `SIGNALS_RUNTIME` in `emit_c.rs`). The handler only
//! writes the kind of the signal into a pipe, which is safe in a handler; a thread started by the
//! first listener waits for the pipe and hands each signal to the listeners of its kind, waking the
//! fiber that waits for one through the scheduler's `Done`. The pipe is read only under the lock of
//! the table, and `raise` reads it itself, so a signal it sends has been handed on when it returns.
//!
//! The table is the process's, as the signals are: listeners of several runs in one process see
//! the same signals.

use std::collections::{BTreeMap, VecDeque};
use std::ffi::c_int;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::net::Done;

/// The kinds, in the order of the variants of `Signal`: the numbers of the system.
#[cfg(target_os = "linux")]
const NUMBERS: [c_int; KINDS] = [2, 15, 1, 3, 10, 12, 28];
#[cfg(not(target_os = "linux"))]
const NUMBERS: [c_int; KINDS] = [2, 15, 1, 3, 30, 31, 28];
const KINDS: usize = 7;

/// What `signal` gives for a refused handler.
const SIG_ERR: usize = usize::MAX;

extern "C" {
    fn signal(number: c_int, handler: usize) -> usize;
    fn raise(number: c_int) -> c_int;
    fn write(fd: c_int, bytes: *const std::ffi::c_void, count: usize) -> isize;
    #[cfg(target_os = "linux")]
    fn __errno_location() -> *mut c_int;
    #[cfg(not(target_os = "linux"))]
    fn __error() -> *mut c_int;
}

fn errno_place() -> *mut c_int {
    // SAFETY: both give the place of this thread's `errno`, which lives as long as the thread.
    #[cfg(target_os = "linux")]
    return unsafe { __errno_location() };
    #[cfg(not(target_os = "linux"))]
    return unsafe { __error() };
}

/// The end of the pipe the handler writes into; -1 before the first listener.
static WRITE_END: AtomicI32 = AtomicI32::new(-1);

/// The handler of every kind listened for: the kind goes into the pipe. A full pipe drops it,
/// which loses nothing that counts, since the kinds that wait are kept once each.
extern "C" fn on_signal(number: c_int) {
    let place = errno_place();
    // SAFETY: the place of this thread's `errno`; it is put back as it was.
    let saved = unsafe { *place };
    if let Some(kind) = NUMBERS.iter().position(|n| *n == number) {
        let byte = kind as u8;
        // SAFETY: `write` may be called in a handler; the end does not block.
        unsafe { write(WRITE_END.load(Ordering::Relaxed), (&byte as *const u8).cast(), 1) };
    }
    // SAFETY: as above.
    unsafe { *place = saved };
}

struct Listener {
    /// The kinds it listens for, a bit each.
    mask: u32,
    /// The kinds that came and were not taken, in the order they came, each once.
    pending: VecDeque<u8>,
    /// The fiber that waits for one: the work it waits for, and whom to tell.
    waiter: Option<(u64, Arc<Done>)>,
}

struct Table {
    next: u64,
    listeners: BTreeMap<u64, Listener>,
    /// How many listeners each kind has.
    counts: [u32; KINDS],
    /// What each kind did before its first listener: given back after its last.
    before: [usize; KINDS],
    /// The end of the pipe the kinds are read from, which does not block; `None` before the first
    /// listener.
    read: Option<std::os::unix::net::UnixStream>,
}

static TABLE: Mutex<Table> = Mutex::new(Table { next: 0, listeners: BTreeMap::new(), counts: [0; KINDS], before: [0; KINDS], read: None });

fn table() -> MutexGuard<'static, Table> {
    TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A listener of the kinds whose bits `mask` has; gives its number. The first one makes the pipe
/// and starts the thread that reads it.
pub fn listen(mask: u32) -> std::io::Result<u64> {
    let mut table = table();
    if table.read.is_none() {
        let (read, write) = std::os::unix::net::UnixStream::pair()?;
        read.set_nonblocking(true)?;
        write.set_nonblocking(true)?;
        WRITE_END.store(std::os::unix::io::IntoRawFd::into_raw_fd(write), Ordering::Relaxed);
        let fd = std::os::unix::io::AsRawFd::as_raw_fd(&read);
        table.read = Some(read);
        std::thread::Builder::new().name("signals".to_string()).spawn(move || relay(fd))?;
    }
    for (kind, &number) in NUMBERS.iter().enumerate() {
        if mask & (1 << kind) != 0 {
            if table.counts[kind] == 0 {
                // SAFETY: the handler only writes into the pipe, which is safe in a handler.
                let before = unsafe { signal(number, on_signal as extern "C" fn(c_int) as usize) };
                if before == SIG_ERR {
                    return Err(std::io::Error::last_os_error());
                }
                table.before[kind] = before;
            }
            table.counts[kind] += 1;
        }
    }
    table.next += 1;
    let id = table.next;
    table.listeners.insert(id, Listener { mask, pending: VecDeque::new(), waiter: None });
    Ok(id)
}

/// The listener ends; a kind it was the last listener of does what it did before the first.
pub fn stop(id: u64) {
    let mut table = table();
    let Some(listener) = table.listeners.remove(&id) else { return };
    for (kind, &number) in NUMBERS.iter().enumerate() {
        if listener.mask & (1 << kind) != 0 {
            table.counts[kind] -= 1;
            if table.counts[kind] == 0 {
                // SAFETY: what the kind did before, as `signal` gave it.
                unsafe { signal(number, table.before[kind]) };
            }
        }
    }
}

/// The next kind that came to the listener, taken; or, when none has, `None`, and the work `work`
/// is told through `done` once one comes.
pub fn take_or_wait(id: u64, work: u64, done: Arc<Done>) -> Option<u8> {
    let mut table = table();
    let listener = table.listeners.get_mut(&id).expect("a listener lives while it is used");
    match listener.pending.pop_front() {
        Some(kind) => Some(kind),
        None => {
            listener.waiter = Some((work, done));
            None
        }
    }
}

/// A wait cut short by a cancel: nobody is to be told.
pub fn forget_waiter(id: u64) {
    if let Some(listener) = table().listeners.get_mut(&id) {
        listener.waiter = None;
    }
}

/// The signal of the kind to this program, as C's `raise`: to this thread, so the handler has written
/// it when the call returns; then the pipe is handed on here, so the listeners have it.
pub fn raise_kind(kind: usize) {
    // SAFETY: sends a signal; what it does is what the program set up.
    unsafe { raise(NUMBERS[kind]) };
    hand_on(&mut table());
}

/// The end of a program whose output was closed before it was through, as `head` closes it: by
/// `SIGPIPE`, as compiled code ends, and C's and Go's programs (choice 1330). Rust starts the
/// interpreter with that signal ignored, so its default is set back first. Returns only if the
/// signal did not end the program.
pub fn end_by_closed_output() {
    const SIGPIPE: c_int = 13;
    const SIG_DFL: usize = 0;
    // SAFETY: the default of a signal nothing here listens for, and that signal to this thread.
    unsafe {
        signal(SIGPIPE, SIG_DFL);
        raise(SIGPIPE);
    }
}

pub fn number(kind: usize) -> i32 {
    NUMBERS[kind]
}

/// What the pipe holds goes to the listeners of each kind, under the lock of the table.
fn hand_on(table: &mut Table) {
    let mut bytes = [0u8; 64];
    loop {
        let Some(read) = table.read.as_mut() else { return };
        let got = match std::io::Read::read(read, &mut bytes) {
            Ok(0) => return,
            Ok(got) => got,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        };
        for &kind in &bytes[..got] {
            for listener in table.listeners.values_mut() {
                if listener.mask & (1 << kind) == 0 {
                    continue;
                }
                if !listener.pending.contains(&kind) {
                    listener.pending.push_back(kind);
                }
                if let Some((work, done)) = listener.waiter.take() {
                    done.tell(work);
                }
            }
        }
        // Fewer than asked for: the pipe was empty after them.
        if got < bytes.len() {
            return;
        }
    }
}

/// The thread that waits for the pipe and hands on what it holds, which `raise_kind` may have done
/// first.
fn relay(fd: std::os::unix::io::RawFd) {
    loop {
        crate::net::wait(&[(fd, false)], None);
        hand_on(&mut table());
    }
}
