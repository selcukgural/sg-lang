//! The signals of the system on Windows, for the interpreter, as compiled code has them (choices
//! 1042 to 1044): the handler of the console, which Windows runs on a thread of its own, hands
//! each event on to the listeners of its kind itself, so no pipe and no relay are needed. Ctrl-C
//! and Ctrl-Break are `Interrupt`; the console closed, the user logging off and the system shutting
//! down are `Terminate`, and then the handler does not return, since Windows ends the program
//! once it does: the program ends in its own way, within the few seconds Windows gives it. The
//! other kinds never come on Windows, and `raise` hands them on all the same.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::net::Done;

const KINDS: usize = 14;
/// Windows has no numbers of its own: those of Linux stand for them, for `Process.signal`, where
/// only those whose default leaves a running program as it is are told apart (`sys::inert`). The
/// last, `SIGSTOP`, is no variant of `Signal`: `Signals.stopSelf` raises it.
const NUMBERS: [i32; KINDS] = [2, 15, 1, 3, 10, 12, 28, 20, 18, 21, 22, 17, 14, 19];

const CTRL_C_EVENT: u32 = 0;
const CTRL_BREAK_EVENT: u32 = 1;
const CTRL_CLOSE_EVENT: u32 = 2;
const CTRL_LOGOFF_EVENT: u32 = 5;
const CTRL_SHUTDOWN_EVENT: u32 = 6;
/// How Ctrl-C ends a program that does not listen for it.
const STATUS_CONTROL_C_EXIT: u32 = 0xC000_013A;

#[link(name = "kernel32")]
extern "system" {
    fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32;
    fn ExitProcess(code: u32) -> !;
    fn Sleep(milliseconds: u32);
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
    /// The handler of the console is set.
    installed: bool,
}

static TABLE: Mutex<Table> = Mutex::new(Table { next: 0, listeners: BTreeMap::new(), counts: [0; KINDS], installed: false });

fn table() -> MutexGuard<'static, Table> {
    TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The handler of the console: the event goes to the listeners of its kind; with none, Windows
/// does what it does by default.
unsafe extern "system" fn on_event(event: u32) -> i32 {
    let kind = match event {
        CTRL_C_EVENT | CTRL_BREAK_EVENT => 0,
        CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => 1,
        _ => return 0,
    };
    let heard = {
        let mut table = table();
        let heard = table.counts[kind] > 0;
        if heard {
            hand(&mut table, kind as u8);
        }
        heard
    };
    if !heard {
        return 0;
    }
    if kind == 1 {
        loop {
            // SAFETY: sleeps; the program ends around it.
            unsafe { Sleep(u32::MAX) };
        }
    }
    1
}

/// A listener of the kinds whose bits `mask` has; gives its number. The first one sets the handler
/// of the console.
pub fn listen(mask: u32) -> std::io::Result<u64> {
    let mut table = table();
    if !table.installed {
        // SAFETY: the handler only takes the lock of the table and tells the scheduler.
        if unsafe { SetConsoleCtrlHandler(Some(on_event), 1) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        table.installed = true;
    }
    // A program started with Ctrl-C ignored (Windows hands that on to the programs a program
    // starts) is told of it again while it listens, as a listener on Linux and macOS takes the
    // place of an ignored signal.
    if mask & 1 != 0 && table.counts[0] == 0 {
        // SAFETY: only turns the ignoring of Ctrl-C off for this process.
        unsafe { SetConsoleCtrlHandler(None, 0) };
    }
    for kind in 0..KINDS {
        if mask & (1 << kind) != 0 {
            table.counts[kind] += 1;
        }
    }
    table.next += 1;
    let id = table.next;
    table.listeners.insert(id, Listener { mask, pending: VecDeque::new(), waiter: None });
    Ok(id)
}

/// The listener ends; a kind it was the last listener of does what Windows does by default.
pub fn stop(id: u64) {
    let mut table = table();
    let Some(listener) = table.listeners.remove(&id) else { return };
    for kind in 0..KINDS {
        if listener.mask & (1 << kind) != 0 {
            table.counts[kind] -= 1;
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

/// The end of a program whose output was closed before it was through: Windows has no signal for
/// it, so the program ends with 0, as compiled code does there (choice 1330).
pub fn end_by_closed_output() {}

/// The signal of the kind to this program: handed on here when someone listens; otherwise its
/// default, the end of the program as Ctrl-C ends it, and nothing for a signal whose default on
/// Linux and macOS leaves a running program as it is or pauses it, since Windows pauses no
/// program (`WindowChange`, `Child`, `Continue`, the three of a terminal, and `SIGSTOP`).
pub fn raise_kind(kind: usize) {
    let heard = {
        let mut table = table();
        let heard = table.counts[kind] > 0;
        if heard {
            hand(&mut table, kind as u8);
        }
        heard
    };
    if !heard && !crate::sys::inert(i64::from(NUMBERS[kind])) {
        // SAFETY: ends the process; what was printed went out before the call.
        unsafe { ExitProcess(STATUS_CONTROL_C_EXIT) };
    }
}

pub fn number(kind: usize) -> i32 {
    NUMBERS[kind]
}

/// A signal of the kind goes to its listeners, each of which keeps a kind once.
fn hand(table: &mut Table, kind: u8) {
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
