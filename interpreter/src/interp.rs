//! Runs the tree directly.
//!
//! The checker has already verified names and types, so nothing here reports an `error`.
//! What remains are the data-dependent failures, reported as `panic`: overflow, division by
//! zero, a cast that does not fit, `assert`, `panic`, and running out of stack.
//!
//! Values live in cells. A variable and a struct field are cells; `T*` owns a cell on the
//! heap; `read x` and `ref x` point at the cell of `x`. A value that was moved is shared
//! between the old and the new owner here, which cannot be observed because the checker
//! rejects any use of the old one.

use std::cell::{Cell as Flag, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io::{BufRead, Read, Write};
use std::rc::Rc;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::check::float_fits;
use crate::raw::{self, RawPtr};
use crate::sys::{clock, command_of, copy_new, ended_as, entropy, executable, make_temp, remove_all, remove_file, rename_new, wait_unreaped};
use crate::types::{FloatTy, IntTy, TextIter, Ty};

/// Limit on nested calls. Past it the program gets a readable panic instead of a crash.
pub const MAX_DEPTH: usize = 5000;

type Cell = Rc<RefCell<Value>>;

#[derive(Clone, Debug)]
pub enum Value {
    Int(IntTy, i128),
    /// A `float` is kept as the `double` that holds it exactly, and every operation on it
    /// is done in `f32`.
    Float(FloatTy, f64),
    Bool(bool),
    Char(char),
    Str(Rc<String>),
    Fmt(Rc<String>),
    Void,
    Null,
    /// The struct's name and its fields in declaration order.
    Struct(Rc<str>, Rc<Fields>),
    /// The enum's name (`Result` for the built-in one), the variant's position and what it
    /// carries.
    Enum(Rc<str>, usize, Rc<Vec<Cell>>),
    /// `List<T>`: every item in a cell of its own, so that an item can be lent.
    List(Rc<RefCell<Vec<Cell>>>),
    /// `HashMap<K, V>`: under each key, the key as it was given and the cell of the value.
    /// A hash table; the design promises no order (see `Table`).
    Map(Rc<RefCell<Entries>>),
    /// `T[N]`: every item in a cell of its own. A value: a copy gets cells of its own.
    Array(Rc<Vec<Cell>>),
    /// `read T[]` / `ref T[]`: the cells of the items it looks at.
    Slice(Rc<Vec<Cell>>),
    /// The address of a named function: its name.
    Func(Rc<str>),
    /// `HashSet<T>`: the items, each under itself as the key.
    Set(Rc<RefCell<Table<Value>>>),
    /// What `format` writes into.
    Formatter(Rc<RefCell<String>>),
    /// `T*`
    Ptr(Cell),
    /// `shared T`: one owner of the value. `Rc` counts the owners; a borrow of what it points
    /// to holds the cell inside, not the `Held`, so it does not count as an owner.
    Shared(Rc<Held>),
    /// `weak T`: looks at a `shared` value without owning it.
    Weak(std::rc::Weak<Held>),
    /// `read T` / `ref T`
    Borrow(Cell),
    /// What goes over a text: `text.chars()`, `text.split(',')`. A copy goes on with the
    /// same place in the text, but the checker lets it be used from one name only.
    Iter(Rc<RefCell<Cursor>>),
    /// `StreamReader`. A moved reader is shared with its old place, as every value is here.
    Reader(Rc<RefCell<Reader>>),
    /// `StreamWriter`. A moved writer is taken out of its old place, so that it ends where
    /// compiled code ends it, and writes its buffer there.
    Writer(Rc<RefCell<Writer>>),
    /// `reader.lines()`.
    Lines(Rc<RefCell<Lines>>),
    /// `CancellationSource`
    Source(Rc<RefCell<Cancel>>),
    /// `CancellationToken`; `CancellationToken.None` looks at no source.
    Token(Option<Rc<RefCell<Cancel>>>),
    /// `Task<R>`: a task started with `Task.run`, by the call of an async function, or by
    /// `Task.delay`.
    Task(Rc<RefCell<Task>>),
    /// `TcpListener`, which does not block. Closed when the last place that holds it lets it
    /// go: a moved value leaves its place empty, so that is where compiled code closes it.
    Listener(Rc<std::net::TcpListener>),
    /// `TcpStream`, which does not block.
    Socket(Rc<std::net::TcpStream>),
    /// `CString`: the bytes of a text and a zero byte after them, where C can read them.
    CStr(Rc<std::ffi::CString>),
    /// `raw T`: an address, and the block of `Memory.allocate` it was made from (see `raw`).
    /// `null` is `Value::Null` until it meets an address.
    Raw(RawPtr),
}

/// What a `CancellationSource` and its tokens look at: whether it is cancelled, and the
/// `Task.delay` timers that end early when it is.
#[derive(Debug, Default)]
pub struct Cancel {
    cancelled: bool,
    /// The keys of the timers, in `State::timers`, of the delays given a token of it.
    delays: Vec<(u64, u64)>,
    /// The fibers that wait for the network or for a thread, given a token of it.
    waits: Vec<usize>,
}

/// What a fiber waits for outside the run: a socket to be ready to read (or, `true`, to
/// write), or a piece of work on a thread of its own to end.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Outside {
    Socket(crate::sys::RawFd, bool),
    Work(u64),
}

/// What the clock of a run ends when its time comes.
enum Timer {
    /// `Task.delay(ms)`, and with a token, `Task.delay(ms, read token)`: the task then gives
    /// `Ok()`, and the source no longer has to end it.
    Delay { task: std::rc::Weak<RefCell<Task>>, token: Option<std::rc::Weak<RefCell<Cancel>>> },
    /// `CancellationSource.after(ms)`: the source is cancelled, if it is still there.
    Cancel(std::rc::Weak<RefCell<Cancel>>),
}

/// The `Cancelled { }` that a cancelled wait gives.
fn cancelled() -> Value {
    Value::Struct("Cancelled".into(), Fields::plain(Vec::new()))
}

/// A task. Every task that runs code is a fiber: a thread of its own, of which one runs at a
/// time (see `Sched`).
#[derive(Debug)]
pub struct Task {
    state: TaskState,
    /// Where it was started, for the message of a panic inside it.
    started: Span,
    /// The fibers that wait for it to end.
    waiters: Vec<usize>,
}

#[derive(Debug)]
enum TaskState {
    /// Started with `Task.run` and not waited for yet: such a task runs when it is waited for,
    /// so the order of a program without async functions is the order of its waits. The
    /// function and what it is given.
    Lazy(Rc<str>, Vec<Value>),
    /// Running on its fiber, or waiting there; or a `Task.delay` whose time has not come.
    Running,
    /// Ended, with what its function returned, until the one that waits takes it.
    Done(Option<Value>),
}

/// What a fiber takes to the thread it runs on: the values of one program are only ever used
/// by the fiber that has the turn.
struct Baton<T>(T);

// SAFETY: a fiber touches the values of the program (`Rc`, `RefCell`) only while it has the
// turn, and the turn changes hands under the mutex of `Sched`, which orders every use before
// the hand-over before every use after it. The fiber is given what it reaches through the baton.
unsafe impl<T> Send for Baton<T> {}

/// Who has the turn among the fibers of one run. Fiber 0 is the thread that runs `main` or
/// the tests; every task that runs code gets a fiber of its own when it starts. A fiber gives
/// the turn away only where it waits, so a run goes the same way every time.
struct Sched {
    state: Mutex<State>,
}

struct State {
    running: usize,
    next_fiber: usize,
    /// The fibers whose wait is over, in the order they get the turn.
    ready: VecDeque<usize>,
    /// The `Task.delay` tasks and the `CancellationSource.after` sources, by when they end on
    /// the clock of the run, then in the order they were made; each with the moment of the real
    /// clock before which it does not end. The clock of the run gives the order, so that it is
    /// the same on every run; the real clock holds a timer back until its time has really gone
    /// by since it was made, also after the run computed for longer than the clock moved.
    timers: BTreeMap<(u64, u64), (Instant, Timer)>,
    made: u64,
    /// The clock of the run, in milliseconds. It moves only when every fiber waits and a
    /// timer is next, so which timer ends first never depends on the speed of the machine;
    /// the run still sleeps until the time comes.
    now: u64,
    clock: Instant,
    /// The fibers that have a thread and have not ended.
    fibers: BTreeSet<usize>,
    /// What each fiber waits on for its turn, fiber 0 included: only the one that gets the
    /// turn is woken.
    wakers: HashMap<usize, std::sync::Arc<Condvar>>,
    /// What the thread-locals of the program hold, from the fiber that gives the turn away to
    /// the one that takes it.
    thread: Option<Thread>,
    /// A fiber stopped the run (a panic, `exit`): fiber 0 gets the turn, and this.
    stop: Option<Stop>,
    /// The interpreter itself failed on a fiber: fiber 0 goes on failing with it.
    crashed: Option<Box<dyn std::any::Any + Send>>,
    /// Fiber 0 is ending the other fibers after a stop: one that gets the turn unwinds.
    ending: bool,
    /// The fibers that wait for a socket or for work on a thread, by fiber.
    outside: BTreeMap<usize, Outside>,
    /// The fibers whose wait outside a cancel ended.
    cut: BTreeSet<usize>,
    /// What the threads of work tell the scheduler through, made when the first is started.
    wake: Option<crate::net::Wake>,
    works: u64,
    /// The tasks of `Task.run` started in this run, which run when they are waited for, or
    /// when a fiber is about to wait outside the run.
    lazy: Vec<std::rc::Weak<RefCell<Task>>>,
}

// SAFETY: as for `Baton`: what `State` holds of the program is touched only by the fiber that
// has the turn, and only under the mutex.
unsafe impl Send for State {}

impl Sched {
    fn new() -> Sched {
        let state = State {
            running: 0,
            next_fiber: 1,
            ready: VecDeque::new(),
            timers: BTreeMap::new(),
            made: 0,
            now: 0,
            clock: Instant::now(),
            fibers: BTreeSet::new(),
            wakers: HashMap::from([(0, std::sync::Arc::new(Condvar::new()))]),
            thread: None,
            stop: None,
            crashed: None,
            ending: false,
            outside: BTreeMap::new(),
            cut: BTreeSet::new(),
            wake: None,
            works: 0,
            lazy: Vec::new(),
        };
        Sched { state: Mutex::new(state) }
    }

    /// Gives the turn to fiber `next` and wakes it.
    fn give(state: &mut State, next: usize) {
        state.running = next;
        if let Some(waker) = state.wakers.get(&next) {
            waker.notify_one();
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn wait_turn<'g>(&self, mut state: MutexGuard<'g, State>, me: usize) -> MutexGuard<'g, State> {
        let waker = state.wakers[&me].clone();
        while state.running != me {
            state = waker.wait(state).unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        state
    }

    /// Fiber `me`, which has said where it waits, gives the turn to `next` and waits to get it
    /// back.
    fn hand(&self, mut state: MutexGuard<'_, State>, me: usize, next: usize) -> Res<()> {
        state.thread = Some(Thread::take());
        Self::give(&mut state, next);
        let mut state = self.wait_turn(state, me);
        Self::resume(&mut state, me)
    }

    /// What fiber `me` does when the turn comes back to it.
    fn resume(state: &mut State, me: usize) -> Res<()> {
        if let Some(thread) = state.thread.take() {
            thread.put();
        }
        if me == 0 {
            if let Some(crash) = state.crashed.take() {
                std::panic::resume_unwind(crash);
            }
            if let Some(stop) = state.stop.take() {
                return Err(Unwind::Stop(stop));
            }
        } else if state.ending {
            ending();
            return Err(Unwind::Abandon);
        }
        Ok(())
    }

    /// Fiber `me` waits for something to end: the turn goes to the fiber that is ready first.
    fn block(&self, me: usize, at: Span) -> Res<()> {
        let mut state = self.lock();
        let next = Self::next(&mut state, at);
        self.hand(state, me, next)
    }

    /// When a timer ends on the real clock: at its time on the clock of the run, but not before
    /// `earliest`, its time counted from when it was made.
    fn due(state: &State, at: u64, earliest: Instant) -> Instant {
        (state.clock + Duration::from_millis(at)).max(earliest)
    }

    /// The fiber that gets the turn: the first that is ready. When none is, the time of the
    /// first timer comes. When there is no timer either, nothing could end a wait, and the
    /// run stops with a panic.
    fn next(state: &mut State, at: Span) -> usize {
        loop {
            if let Some(fiber) = state.ready.pop_front() {
                return fiber;
            }
            // Sockets and threads are waited for until the first timer is due.
            if !state.outside.is_empty() {
                let limit = state.timers.iter().next().map(|(key, (earliest, _))| Self::due(state, key.0, *earliest).saturating_duration_since(Instant::now()));
                if limit != Some(Duration::ZERO) {
                    Self::wait_outside(state, limit);
                    continue;
                }
            }
            let Some((&key, _)) = state.timers.iter().next() else {
                state.stop = Some(Stop::Diag(Diag::panic(at, "every task waits, and nothing is left that could end a wait")));
                return 0;
            };
            let (earliest, timer) = state.timers.remove(&key).expect("just seen");
            // A source that is gone cancels nothing, and is not waited for.
            if let Timer::Cancel(source) = &timer {
                if source.strong_count() == 0 {
                    continue;
                }
            }
            let due = Self::due(state, key.0, earliest);
            let now = Instant::now();
            if due > now {
                std::thread::sleep(due - now);
            }
            state.now = key.0;
            match timer {
                Timer::Delay { task, token } => {
                    let value = match &token {
                        Some(source) => {
                            if let Some(source) = source.upgrade() {
                                source.borrow_mut().delays.retain(|other| *other != key);
                            }
                            ok(Value::Void)
                        }
                        None => Value::Void,
                    };
                    if let Some(task) = task.upgrade() {
                        Self::end_timer(state, &task, value);
                    }
                }
                Timer::Cancel(source) => {
                    if let Some(source) = source.upgrade() {
                        Self::cancel(state, &source);
                    }
                }
            }
        }
    }

    /// Waits for the sockets and the threads that fibers wait for, at most `limit`. Those
    /// that are ready are made ready, in the order of their fibers; the clock of the run goes
    /// on to the time that has gone by, since a wait outside is not the clock's to choose.
    fn wait_outside(state: &mut State, limit: Option<Duration>) {
        let mut sockets = Vec::new();
        let mut owners = Vec::new();
        for (&fiber, what) in &state.outside {
            if let Outside::Socket(fd, write) = *what {
                sockets.push((fd, write));
                owners.push(fiber);
            }
        }
        let hear = state.wake.as_ref().map(|wake| crate::net::fd_of(&wake.hear));
        if let Some(fd) = hear {
            sockets.push((fd, false));
        }
        let ready = crate::net::wait(&sockets, limit);
        let mut woken = false;
        for (fiber, ready) in owners.iter().zip(&ready) {
            if *ready {
                state.outside.remove(fiber);
                state.ready.push_back(*fiber);
                woken = true;
            }
        }
        if hear.is_some() && ready[sockets.len() - 1] {
            let ended = state.wake.as_ref().expect("heard").ended();
            for id in ended {
                let fiber = state.outside.iter().find(|(_, what)| **what == Outside::Work(id)).map(|(fiber, _)| *fiber);
                // Work whose fiber a cancel woke has no one waiting for it.
                if let Some(fiber) = fiber {
                    state.outside.remove(&fiber);
                    state.ready.push_back(fiber);
                    woken = true;
                }
            }
        }
        if woken {
            state.now = state.now.max(state.clock.elapsed().as_millis() as u64);
        }
    }

    /// A task that runs no code ends with `value`: those that wait for it are ready.
    fn end_timer(state: &mut State, task: &Rc<RefCell<Task>>, value: Value) {
        let mut task = task.borrow_mut();
        task.state = TaskState::Done(Some(value));
        let waiters = std::mem::take(&mut task.waiters);
        state.ready.extend(waiters);
    }

    /// `source.cancel()`: the delays given a token of it end now, with `Error(Cancelled { })`,
    /// in the order they were started. Cancelling again does nothing.
    fn cancel(state: &mut State, source: &Rc<RefCell<Cancel>>) {
        let (keys, waits) = {
            let mut source = source.borrow_mut();
            if source.cancelled {
                return;
            }
            source.cancelled = true;
            (std::mem::take(&mut source.delays), std::mem::take(&mut source.waits))
        };
        // A wait for a socket or a thread ends at once; what the thread does goes on, and
        // what it gives is not lost where it can be kept (the input).
        for fiber in waits {
            if state.outside.remove(&fiber).is_some() {
                state.cut.insert(fiber);
                state.ready.push_back(fiber);
            }
        }
        for key in keys {
            if let Some((_, Timer::Delay { task, .. })) = state.timers.remove(&key) {
                if let Some(task) = task.upgrade() {
                    Self::end_timer(state, &task, failed(cancelled()));
                }
            }
        }
    }

    /// Fiber `me` has ended with `result`: its task gets what it returned, and the turn goes
    /// on. A stop goes to fiber 0.
    fn finish(&self, me: usize, task: Option<Rc<RefCell<Task>>>, result: Result<Value, Unwind>, at: Span) {
        let mut state = self.lock();
        state.fibers.remove(&me);
        state.wakers.remove(&me);
        let next = match result {
            Ok(value) => {
                if let Some(task) = task {
                    let mut task = task.borrow_mut();
                    task.state = TaskState::Done(Some(value));
                    let waiters = std::mem::take(&mut task.waiters);
                    state.ready.extend(waiters);
                }
                Self::next(&mut state, at)
            }
            Err(Unwind::Abandon) => 0,
            Err(Unwind::Stop(stop)) => {
                state.stop = Some(stop);
                0
            }
            Err(_) => unreachable!("`return` ends at its call, `break` and `continue` at their loop"),
        };
        state.thread = Some(Thread::take());
        Self::give(&mut state, next);
    }
}

/// What every fiber of one run reaches: the output, the world around the program, what a
/// test wrote to the standard error, and the scheduler. Only the fiber that has the turn
/// touches them.
struct Shared<'e> {
    out: RefCell<&'e mut dyn Write>,
    world: RefCell<World<'e>>,
    input: RefCell<crate::input::Input>,
    test_err: RefCell<String>,
    sched: Sched,
    /// The functions of C the program calls (see `ffi`).
    ffi: crate::ffi::Library,
    /// The programs `System.start` started and that have not been waited for, by number.
    children: RefCell<HashMap<i64, Kid>>,
    /// The files of the program, for a panic that must be shown where it happens: in a
    /// function C called (see `Interp::die`).
    sources: &'e crate::source::Sources,
    /// The values of SG an `export` function gave C to keep (`T*`), by the address C holds.
    handles: RefCell<HashMap<usize, Handle>>,
}

/// A value of SG that C holds: an `export` function gave it as a `T*`, and C gives it back,
/// or lends it, by its address.
struct Handle {
    value: Cell,
    ty: Ty,
    /// Lent to the calls in progress: how many `read` borrows, or -1 for a `ref` one; and the
    /// function that took the first.
    lent: i32,
    lent_to: String,
}

/// A program `System.start` started. What it writes to an output it shares with the run is read
/// by a thread of its own and written out to the run's when it is waited for, since the run's
/// output need not be the stream of the process (`copies`, with whether it is the standard error).
struct Kid {
    child: std::process::Child,
    copies: Vec<(std::thread::JoinHandle<Vec<u8>>, bool)>,
}

/// What a thread keeps of the program running on it, taken to the thread a task runs on
/// and brought back.
struct Thread {
    streams: Option<[Cell; 3]>,
    ending: bool,
    live: Live,
}

impl Thread {
    fn take() -> Thread {
        Thread {
            streams: STREAMS.with(|streams| streams.borrow_mut().take()),
            ending: ENDING.with(|ending| ending.get()),
            live: LIVE.with(|live| std::mem::take(&mut *live.borrow_mut())),
        }
    }

    fn put(self) {
        STREAMS.with(|streams| *streams.borrow_mut() = self.streams);
        ENDING.with(|ending| ending.set(self.ending));
        LIVE.with(|live| *live.borrow_mut() = self.live);
    }
}

/// How many bytes a writer keeps before it writes them, in the interpreter and in compiled
/// code alike: a program that reads its own file back sees the same bytes in both.
const STREAM_BUFFER: usize = 65536;

/// The longest line, or text, the reader of a connection gives unless told otherwise.
const READER_LIMIT: usize = 65536;

/// The turn given away for a call of C (`Interp::c_leave`): the work the scheduler waits
/// for, and where its end is told.
struct Leave {
    id: u64,
    done: std::sync::Arc<crate::net::Done>,
}

/// `State::running` while no fiber has the turn and a thread looks for the next.
const NOBODY: usize = usize::MAX;

/// What `Memory.addressOf` gave an address for: the cell of the value, and its type.
type Given = (std::rc::Weak<RefCell<Value>>, Ty);

thread_local! {
    /// The interpreter that is in a call of C on this thread, for C to call an `export`
    /// function through (see `enter`); null when none is.
    static ENTERED: Flag<*mut std::ffi::c_void> = const { Flag::new(std::ptr::null_mut()) };
    /// The turn the call of C on this thread gave away, for a function C calls back to take
    /// it again (see `Interp::entered`).
    static LEFT: RefCell<Option<Leave>> = const { RefCell::new(None) };
    /// The addresses `Memory.addressOf` gave on this thread: the cell of the value, and its
    /// type, for `(ref T)data` to find it again and check it.
    static ADDRESSES: RefCell<HashMap<usize, Given>> = RefCell::new(HashMap::new());
    /// `Stdin`, `Stdout` and `Stderr` of the program running on this thread.
    static STREAMS: RefCell<Option<[Cell; 3]>> = const { RefCell::new(None) };
    /// The program is ending with a panic or `exit`: a writer that ends now does not write
    /// its buffer, as in compiled code, where nothing runs after them.
    static ENDING: Flag<bool> = const { Flag::new(false) };
}

/// Where the system's last error of a fiber is kept (see `Interp::errno`): on Windows beside
/// `errno`, elsewhere `errno` itself.
const LAST_ERROR: usize = if cfg!(windows) { 1 } else { 0 };

/// What C calls for an `export` function of the program, through the stub the library of
/// wrappers has for it: on a thread in a call of C, the interpreter of that call runs the
/// function. C is not unwound through: a panic ends the process (see `Interp::die`).
///
/// # Safety
///
/// Called only by the stubs the library of wrappers was built with: `args` holds the address
/// of each argument, as the C type of the parameter, and `ret` has room for what it gives.
pub(crate) unsafe extern "C" fn enter(number: std::ffi::c_int, name: *const std::ffi::c_char, args: *mut *mut std::ffi::c_void, ret: *mut std::ffi::c_void) {
    let me = ENTERED.with(|entered| entered.get());
    if me.is_null() {
        // SAFETY: the stub gives its name as a text that ends in a zero byte.
        let name = unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy();
        eprintln!("error: C called `{name}` on a thread where the interpreter is in no call of C");
        eprintln!("  = note: the interpreter runs a function C calls only during a call of C on the same thread; compiled code (`sg build`) takes it on any thread");
        std::process::exit(1);
    }
    // SAFETY: `ENTERED` holds the interpreter that is in a call of C on this thread, and it
    // touches nothing of itself until the call returns (see `Interp::call_c`).
    let interp = unsafe { &mut *me.cast::<Interp<'static, 'static>>() };
    interp.entered(number as usize, args, ret);
}

/// Makes `Stdin`, `Stdout` and `Stderr` for a program that starts.
fn open_streams() {
    let writer = |sink| cell(Value::Writer(Rc::new(RefCell::new(Writer { sink, buffer: Vec::new(), failed: None }))));
    let stdin = cell(Value::Reader(Rc::new(RefCell::new(Reader { file: None, net: None, done: 0 }))));
    STREAMS.with(|streams| *streams.borrow_mut() = Some([stdin, writer(Sink::Stdout), writer(Sink::Stderr)]));
    ENDING.with(|ending| ending.set(false));
    PENDING.with(|pending| pending.borrow_mut().clear());
}

/// What `Stdin`, `Stdout` or `Stderr` names: a `ref` to the stream.
fn stream_slot(name: &str) -> Option<Cell> {
    let at = ["Stdin", "Stdout", "Stderr"].iter().position(|known| *known == name)?;
    STREAMS.with(|streams| streams.borrow().as_ref().map(|streams| cell(Value::Borrow(streams[at].clone()))))
}

fn ending() {
    ENDING.with(|ending| ending.set(true));
}

/// The fields of a struct value, in declaration order. Those of a `Disposable` type name it,
/// and are handed to `PENDING` when the last place that holds them lets them go: the
/// interpreter then runs `dispose` on them (see `Interp::disposals`), as compiled code does
/// where it drops the value.
#[derive(Debug)]
pub struct Fields {
    cells: Vec<Cell>,
    /// The `Disposable` type the value is of, until it is disposed of.
    disposed_as: Option<Rc<str>>,
}

impl Fields {
    fn plain(cells: Vec<Cell>) -> Rc<Fields> {
        Rc::new(Fields { cells, disposed_as: None })
    }
}

impl std::ops::Deref for Fields {
    type Target = Vec<Cell>;
    fn deref(&self) -> &Vec<Cell> {
        &self.cells
    }
}

impl Drop for Fields {
    fn drop(&mut self) {
        // A panic and `exit` dispose of nothing, as in compiled code.
        if let Some(name) = self.disposed_as.take() {
            if !ENDING.try_with(Flag::get).unwrap_or(true) {
                let cells = std::mem::take(&mut self.cells);
                let _ = PENDING.try_with(|pending| pending.borrow_mut().push((name, cells)));
            }
        }
    }
}

thread_local! {
    /// The values of `Disposable` types that their last owner let go, in the order it did,
    /// waiting for their `dispose`.
    static PENDING: RefCell<Vec<(Rc<str>, Vec<Cell>)>> = const { RefCell::new(Vec::new()) };
}

/// What a `shared` value points to.
#[derive(Debug)]
pub struct Held {
    value: Cell,
    /// `shared checked`: the borrows of the value in progress.
    lock: Option<RefCell<Lock>>,
    /// Its number in `LIVE`.
    id: usize,
}

impl Held {
    fn new(value: Value, checked: bool, made: Span) -> Rc<Held> {
        let id = LIVE.with(|live| {
            let mut live = live.borrow_mut();
            live.next += 1;
            live.next
        });
        let held = Rc::new(Held { value: cell(value), lock: checked.then(|| RefCell::new(Lock::default())), id });
        LIVE.with(|live| live.borrow_mut().held.insert(id, (Rc::downgrade(&held), made)));
        held
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        // The program may be ending with this thread's storage already gone.
        let _ = LIVE.try_with(|live| live.borrow_mut().held.remove(&self.id));
    }
}

/// The `shared` values alive on this thread, by number, each with where it was made: what is
/// left when a program or a test ends holds itself up, and is reported.
#[derive(Default)]
struct Live {
    held: HashMap<usize, (std::rc::Weak<Held>, Span)>,
    next: usize,
}

thread_local! {
    static LIVE: RefCell<Live> = RefCell::new(Live::default());
}

/// The borrows of a `shared checked` value in progress: where each began.
#[derive(Debug, Default)]
struct Lock {
    reads: Vec<Span>,
    write: Option<Span>,
}

/// A borrow of a `shared checked` value in progress; it ends when this is dropped.
struct Guard {
    held: Rc<Held>,
    write: bool,
    /// Lent to read while the arguments of a method that changes it are computed.
    reserved: bool,
    span: Span,
    /// The type it holds, for the message of a conflict.
    what: String,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let Some(lock) = &self.held.lock else { return };
        let mut lock = lock.borrow_mut();
        if self.write {
            lock.write = None;
        } else if let Some(at) = lock.reads.iter().rposition(|span| *span == self.span) {
            lock.reads.remove(at);
        }
    }
}

/// The name of what a value is, for the leak report.
fn kind_name(value: &Value) -> String {
    match value {
        Value::Struct(name, _) | Value::Enum(name, ..) => short_name(name).to_string(),
        Value::Int(ty, _) => ty.name().to_string(),
        Value::Str(_) => "string".to_string(),
        Value::List(_) => "List".to_string(),
        Value::Map(_) => "HashMap".to_string(),
        Value::Set(_) => "HashSet".to_string(),
        _ => "value".to_string(),
    }
}

/// Whether a value owns, somewhere inside it, one of the `shared` values numbered in `ids`.
fn owns_any(value: &Value, ids: &[usize]) -> bool {
    match value {
        Value::Shared(held) => ids.contains(&held.id),
        Value::Struct(_, fields) => fields.iter().any(|item| owns_any(&item.borrow(), ids)),
        Value::Enum(_, _, cells) | Value::Array(cells) => cells.iter().any(|item| owns_any(&item.borrow(), ids)),
        Value::List(items) => items.borrow().iter().any(|item| owns_any(&item.borrow(), ids)),
        Value::Map(entries) => entries.borrow().values().any(|(_, item)| owns_any(&item.borrow(), ids)),
        Value::Set(items) => items.borrow().values().any(|item| owns_any(item, ids)),
        Value::Ptr(inner) => owns_any(&inner.borrow(), ids),
        _ => false,
    }
}

/// `text` with every whole word `word` replaced by `with`.
pub(crate) fn replace_word(text: &str, word: &str, with: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    let is_name = |c: char| c.is_alphanumeric() || c == '_';
    while let Some(at) = rest.find(word) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + word.len()..].chars().next();
        out.push_str(&rest[..at]);
        match before.is_some_and(is_name) || after.is_some_and(is_name) {
            true => out.push_str(word),
            false => out.push_str(with),
        }
        rest = &rest[at + word.len()..];
    }
    out.push_str(rest);
    out
}

/// The `shared` value a value is, or that a borrow looks at.
fn held_of(value: &Value) -> Option<Rc<Held>> {
    match value {
        Value::Shared(held) => Some(held.clone()),
        Value::Borrow(inner) => held_of(&inner.borrow()),
        _ => None,
    }
}

/// What a pointer, a borrow or a `shared` value points to.
fn behind(value: &Value) -> Option<Cell> {
    match value {
        Value::Ptr(inner) | Value::Borrow(inner) => Some(inner.clone()),
        Value::Shared(held) => Some(held.value.clone()),
        _ => None,
    }
}

/// A `StreamReader`: a file read through a buffer, a connection (`net`), or, with neither,
/// the input of the program.
#[derive(Debug)]
pub struct Reader {
    file: Option<std::io::BufReader<std::fs::File>>,
    net: Option<NetReader>,
    /// The bytes given out so far: where an `InvalidText` lies.
    done: u64,
}

impl Reader {
    fn is_stdin(&self) -> bool {
        self.file.is_none() && self.net.is_none()
    }
}

/// The reader of a connection (`conn.reader(limit)`). What has come and has not been given
/// out stays in `block` until a read can take all it needs, so a cancel loses nothing.
#[derive(Debug)]
pub struct NetReader {
    stream: Rc<std::net::TcpStream>,
    block: Vec<u8>,
    /// Where what has not been given out starts in `block`.
    at: usize,
    /// The other side has let the connection go.
    ended: bool,
    /// The longest line, or text, it gives.
    limit: usize,
    /// It met a longer one: every later read gives `LineTooLong`.
    too_long: bool,
}

impl NetReader {
    /// What a read can do with what the block holds: `None` when it needs more to come,
    /// `Some(None)` when the line or the text is longer than the limit, and `Some(Some(n))`
    /// when it takes the first `n` bytes. `want`: the size of the buffer of `readBytes` and
    /// `readSome`.
    fn ready(&self, call: StreamCall, want: usize) -> Option<Option<usize>> {
        if self.too_long {
            return Some(None);
        }
        let rest = &self.block[self.at..];
        let held = rest.len();
        let fits = |content: usize, taken: usize| Some(if content > self.limit { None } else { Some(taken) });
        match call {
            StreamCall::ReadLine => match rest.iter().position(|byte| *byte == b'\n') {
                Some(end) => fits(end - usize::from(end > 0 && rest[end - 1] == b'\r'), end + 1),
                // A line of `limit` bytes may still end with `\r\n`.
                None if held > self.limit.saturating_add(1) => Some(None),
                None if self.ended => fits(held, held),
                None => None,
            },
            StreamCall::ReadText if held > self.limit => Some(None),
            StreamCall::ReadText => self.ended.then_some(Some(held)),
            StreamCall::ReadBytes if held >= want || self.ended => Some(Some(held.min(want))),
            StreamCall::ReadSome if held > 0 || self.ended || want == 0 => Some(Some(held.min(want))),
            _ => None,
        }
    }
}

#[derive(Debug)]
enum Sink {
    /// `None` once it is closed.
    File(Option<std::fs::File>),
    Stdout,
    Stderr,
}

/// A `StreamWriter`. Into a file, what is written waits in a buffer; `Stdout` writes where
/// `print` writes, and `Stderr` where `printError` does.
#[derive(Debug)]
pub struct Writer {
    sink: Sink,
    buffer: Vec<u8>,
    /// The first error writing met: every later call gives it again.
    failed: Option<Value>,
}

/// A writer that ends without `close` (in a field, a list, after an exit through `try`)
/// writes its buffer and loses the error, if one comes.
impl Drop for Writer {
    fn drop(&mut self) {
        if let Sink::File(Some(file)) = &mut self.sink {
            if self.failed.is_none() && !ENDING.with(Flag::get) {
                let _ = file.write_all(&self.buffer);
            }
        }
    }
}

/// `reader.lines()`: where it reads from, and whether it has met the end or an error.
#[derive(Debug)]
pub struct Lines {
    reader: Rc<RefCell<Reader>>,
    done: bool,
}

/// Where an iterator over a text is.
#[derive(Debug)]
pub struct Cursor {
    kind: TextIter,
    text: Rc<String>,
    /// The byte the next part starts at.
    at: usize,
    /// What `split` splits at.
    separator: Rc<str>,
    /// `split` has given its last part.
    done: bool,
}

impl Cursor {
    /// The next part, as compiled code finds it too.
    fn next(&mut self) -> Option<Value> {
        let text = self.text.clone();
        let rest = &text[self.at.min(text.len())..];
        match self.kind {
            TextIter::Chars => {
                let found = rest.chars().next()?;
                self.at += found.len_utf8();
                Some(Value::Char(found))
            }
            TextIter::Split => {
                if self.done {
                    return None;
                }
                let part = match rest.find(&*self.separator) {
                    Some(end) => {
                        self.at += end + self.separator.len();
                        &rest[..end]
                    }
                    None => {
                        self.done = true;
                        rest
                    }
                };
                Some(Value::Str(part.to_string().into()))
            }
            TextIter::SplitWhitespace => {
                let start = rest.find(|found: char| !found.is_whitespace())?;
                let word = &rest[start..];
                let end = word.find(char::is_whitespace).unwrap_or(word.len());
                self.at += start + end;
                Some(Value::Str(word[..end].to_string().into()))
            }
            // A line ends at `\n`, and a `\r` right before it is not part of the line.
            TextIter::Lines => {
                if rest.is_empty() {
                    return None;
                }
                let line = match rest.find('\n') {
                    Some(end) => {
                        self.at += end + 1;
                        rest[..end].strip_suffix('\r').unwrap_or(&rest[..end])
                    }
                    None => {
                        self.at = text.len();
                        rest
                    }
                };
                Some(Value::Str(line.to_string().into()))
            }
            TextIter::ReadLines => unreachable!("the lines of a reader are `Value::Lines`"),
        }
    }
}

/// What a text is searched for, as text: a character, a text, or one byte.
fn needle(value: &Value) -> Vec<u8> {
    match value {
        Value::Char(found) => found.to_string().into_bytes(),
        Value::Str(text) => text.as_bytes().to_vec(),
        Value::Int(_, byte) => vec![*byte as u8],
        _ => unreachable!("the checker lets only a character, a text or a byte be looked for"),
    }
}

/// Where `part` first (or last) lies in `text`, in bytes.
fn find_bytes(text: &[u8], part: &[u8], last: bool) -> Option<usize> {
    if part.len() > text.len() {
        return None;
    }
    let mut starts = 0..=text.len() - part.len();
    let at = |start: &usize| &text[*start..*start + part.len()] == part;
    if last { starts.rev().find(at) } else { starts.find(at) }
}

type Entries = Table<(Value, Cell)>;

/// The hash table behind `HashMap` and `HashSet`, built the way compiled code builds it, so
/// that a program prints the same in both: the entries lie one after the other in the order
/// they were added, and an entry that is removed is replaced by the last one. The language
/// promises no order; this is only what falls out of the table.
#[derive(Debug)]
pub struct Table<V> {
    keys: Vec<Key>,
    held: Vec<V>,
    index: HashMap<Key, usize>,
}

impl<V> Table<V> {
    fn new() -> Table<V> {
        Table { keys: Vec::new(), held: Vec::new(), index: HashMap::new() }
    }

    fn len(&self) -> usize {
        self.keys.len()
    }

    fn clear(&mut self) {
        self.keys.clear();
        self.held.clear();
        self.index.clear();
    }

    fn get(&self, key: &Key) -> Option<&V> {
        self.index.get(key).map(|at| &self.held[*at])
    }

    fn contains_key(&self, key: &Key) -> bool {
        self.index.contains_key(key)
    }

    /// Puts the value under the key; gives what was there before, if anything.
    fn insert(&mut self, key: Key, value: V) -> Option<V> {
        if let Some(at) = self.index.get(&key) {
            return Some(std::mem::replace(&mut self.held[*at], value));
        }
        self.index.insert(key.clone(), self.keys.len());
        self.keys.push(key);
        self.held.push(value);
        None
    }

    /// Takes the entry out; the last entry moves into its place.
    fn remove(&mut self, key: &Key) -> Option<V> {
        let at = self.index.remove(key)?;
        self.keys.swap_remove(at);
        let value = self.held.swap_remove(at);
        if let Some(moved) = self.keys.get(at) {
            self.index.insert(moved.clone(), at);
        }
        Some(value)
    }

    fn keys(&self) -> impl Iterator<Item = &Key> {
        self.keys.iter()
    }

    fn values(&self) -> impl Iterator<Item = &V> {
        self.held.iter()
    }
}

/// A key of a `HashMap` or an item of a `HashSet`: an integer, a `bool`, text, or a struct
/// or an enum made of such parts (an enum starts with the position of its variant).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    Null,
    Bool(bool),
    Int(i128),
    /// A fractional number in its total order (see `float_key`). Never the key of a map:
    /// it is what `equals` and `compare` look at.
    Float(i64),
    Str(Rc<String>),
    Parts(Vec<Key>),
}

/// The total order of `equals` and `compare`: every `NaN` is one value, below every other;
/// `-0.0` is `0.0`; the rest are in the order of the numbers. The bits of a positive number
/// already sort as the number does; those of a negative one sort the other way round.
fn float_key(value: f64) -> i64 {
    if value.is_nan() {
        return i64::MIN;
    }
    let bits = (value + 0.0).to_bits() as i64;
    if bits < 0 {
        bits ^ i64::MAX
    } else {
        bits
    }
}

/// How a fractional number is written: the fewest digits that read back as the same
/// number, and of those the nearest, a tie going to the even digit (choice 496), laid out as
/// JavaScript does it (`0.1`, `100`, `1.5e+21`, `1e-7`). `float` reads back as a `float`.
/// The compiled code finds the same digits with Dragonbox (`shortest.rs`).
pub fn show_float(value: f64, ty: FloatTy) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.to_string();
    }
    let written = match ty {
        FloatTy::Float => fewest_digits(value as f32),
        FloatTy::Double => fewest_digits(value),
    };
    let (mantissa, exponent) = written.split_once('e').expect("written with an exponent");
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let point = exponent.parse::<i32>().expect("a number") + 1;
    let count = digits.len() as i32;
    let body = if count <= point && point <= 21 {
        format!("{digits}{}", "0".repeat((point - count) as usize))
    } else if 0 < point && point <= 21 {
        format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
    } else if -6 < point && point <= 0 {
        format!("0.{}{digits}", "0".repeat((-point) as usize))
    } else {
        let rest = if count > 1 { format!(".{}", &digits[1..]) } else { String::new() };
        let sign = if point - 1 < 0 { '-' } else { '+' };
        format!("{}{rest}e{sign}{}", &digits[..1], (point - 1).abs())
    };
    if negative {
        format!("-{body}")
    } else {
        body
    }
}

/// The fewest digits of a number, written with an exponent (`1.25e-1`). Rust's `{:e}` finds
/// how many; of the numbers with that many digits it gives one that reads back, but in a tie
/// between two it takes the larger. The number correctly rounded to that many digits
/// (`{:.*e}` rounds a tie to the even digit) is the nearest, and is taken when it reads back;
/// it does not only next to a power of two, where the number below is nearer.
fn fewest_digits<T: std::fmt::LowerExp + std::str::FromStr + PartialEq>(value: T) -> String {
    let shortest = format!("{value:e}");
    let count = shortest.split('e').next().expect("a mantissa").bytes().filter(u8::is_ascii_digit).count();
    let rounded = format!("{value:.*e}", count - 1);
    match rounded.parse::<T>() {
        Ok(back) if back == value => rounded,
        _ => shortest,
    }
}

/// A fractional number written in the program, read in its type.
fn float_literal(text: &str, ty: FloatTy) -> f64 {
    match ty {
        FloatTy::Float => text.parse::<f32>().expect("the lexer read it") as f64,
        FloatTy::Double => text.parse::<f64>().expect("the lexer read it"),
    }
}

fn key_of(value: &Value) -> Key {
    match value {
        Value::Bool(value) => Key::Bool(*value),
        Value::Int(_, value) => Key::Int(*value),
        Value::Char(value) => Key::Int(*value as i128),
        Value::Float(_, value) => Key::Float(float_key(*value)),
        Value::Str(text) => Key::Str(text.clone()),
        Value::Null => Key::Null,
        Value::Struct(_, fields) => Key::Parts(fields.iter().map(|field| key_of(&field.borrow())).collect()),
        Value::Enum(_, index, fields) => {
            let parts = fields.iter().map(|field| key_of(&field.borrow()));
            Key::Parts(std::iter::once(Key::Int(*index as i128)).chain(parts).collect())
        }
        Value::Ptr(inner) | Value::Borrow(inner) => key_of(&inner.borrow()),
        _ => unreachable!("the checker only lets `Hashable` values be keys"),
    }
}

/// `map.add(key, value)`: a key that is already there is a mistake, not a silent overwrite.
fn map_add(entries: &mut Entries, key: Value, value: Value, shown: String, span: Span) -> Result<(), Diag> {
    let under = key_of(&key);
    if entries.contains_key(&under) {
        return Err(Diag::panic(span, format!("key {shown} is already in the map"))
            .note("to change what is kept under a key, write `map[key] = value`"));
    }
    entries.insert(under, (key, cell(value)));
    Ok(())
}

/// `IoError.Cancelled(Cancelled { })`
/// `IoError.HostNotFound`: the host could not be found, or the address is not `host:port`.
fn io_host_not_found() -> Value {
    Value::Enum("IoError".into(), 10, Rc::new(Vec::new()))
}

/// `IoError.LineTooLong`: the reader of a connection met a line, or a text, longer than its
/// limit.
fn io_line_too_long() -> Value {
    Value::Enum("IoError".into(), 11, Rc::new(Vec::new()))
}

fn io_cancelled() -> Value {
    Value::Enum("IoError".into(), 6, Rc::new(vec![cell(cancelled())]))
}

/// Whether `compareExchange` finds the value it expects: the same bits, as C compares them, so
/// `-0.0` is not `0.0` and a NaN may be itself.
fn same_bits(found: &Value, expected: &Value) -> bool {
    match (found, expected) {
        (Value::Float(FloatTy::Float, a), Value::Float(_, b)) => (*a as f32).to_bits() == (*b as f32).to_bits(),
        (Value::Float(_, a), Value::Float(_, b)) => a.to_bits() == b.to_bits(),
        (Value::Int(_, a), Value::Int(_, b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (found, expected) => raw_of(found).addr == raw_of(expected).addr,
    }
}

fn ok(value: Value) -> Value {
    Value::Enum("Result".into(), 0, Rc::new(vec![cell(value)]))
}

fn failed(cause: Value) -> Value {
    Value::Enum("Result".into(), 1, Rc::new(vec![cell(cause)]))
}

/// The `IoError` for what the system said, its variant told by the code (`sys::io_kind`).
fn io_cause(error: Option<&std::io::Error>, code: Option<i32>) -> Value {
    let variant = |at: usize, fields: Vec<Value>| Value::Enum("IoError".into(), at, Rc::new(fields.into_iter().map(cell).collect()));
    match code {
        Some(code) => match crate::sys::io_kind(code) {
            5 => variant(5, vec![Value::Int(IntTy::Int, code as i128)]),
            at => variant(at, Vec::new()),
        },
        // An argument std refuses before the system sees it (a line break for a batch file of
        // Windows): `InvalidInput`, as compiled code gives it.
        None if error.is_some_and(|error| error.kind() == std::io::ErrorKind::InvalidInput) => variant(15, Vec::new()),
        // An error that did not come from the system: EIO.
        None if error.is_some() => variant(5, vec![Value::Int(IntTy::Int, 5)]),
        None => Value::Void,
    }
}

trait InvalidText {
    fn invalid_text(self, at: u64) -> Value;
}

impl InvalidText for Value {
    /// `IoError.InvalidText(at)`
    fn invalid_text(self, at: u64) -> Value {
        Value::Enum("IoError".into(), 4, Rc::new(vec![cell(Value::Int(IntTy::Long, at as i128))]))
    }
}

/// Bytes read as text: `Ok`, or `InvalidText` with the place of the first bad byte, counted
/// from `at`.
fn text_read(bytes: Vec<u8>, at: u64) -> Value {
    match String::from_utf8(bytes) {
        Ok(text) => ok(Value::Str(Rc::new(text))),
        Err(error) => failed(io_cause(None, None).invalid_text(at + error.utf8_error().valid_up_to() as u64)),
    }
}

/// The kind of a file as `System.list` and `System.status` give it: 0 a file, 1 a folder, 2 a
/// link, 3 anything else.
fn kind_code(kind: &std::fs::FileType) -> u8 {
    match () {
        _ if kind.is_file() => 0,
        _ if kind.is_dir() => 1,
        _ if kind.is_symlink() => 2,
        _ => 3,
    }
}

/// Writes the bytes into the file, which is made if it is not there; `append` keeps what
/// it holds and puts them after it.
fn write_file(path: &str, bytes: &[u8], append: bool) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    match append {
        true => options.append(true).create(true),
        false => options.write(true).create(true).truncate(true),
    };
    options.open(path)?.write_all(bytes)
}

/// The type of the pointer `buffer.pointer()` gives, also when a cast is written around it.
fn arg_pointer_type(arg: &Expr) -> Ty {
    match &arg.kind {
        ExprKind::RawCast { expr, .. } => expr.ty.clone(),
        _ => arg.ty.clone(),
    }
}

/// The address a raw pointer holds; `null` is 0.
fn raw_of(value: &Value) -> RawPtr {
    match value {
        Value::Raw(pointer) => *pointer,
        Value::Null => RawPtr::NULL,
        _ => unreachable!("the checker gives a raw pointer here"),
    }
}

/// The value of type `ty` that lies at `addr`, which the caller has checked.
fn load(addr: usize, ty: &Ty, plain: &crate::raw::PlainTypes, span: Span) -> Result<Value, Diag> {
    // SAFETY: the caller checked that the bytes of a value of `ty` lie at `addr`, or the
    // pointer came from outside, where the program says so with `unsafe`.
    unsafe {
        let at = addr as *const u8;
        Ok(match ty {
            Ty::Atomic(inner) => return load(addr, inner, plain, span),
            Ty::Int(int) => Value::Int(*int, raw::read_int(addr, *int)),
            Ty::Float(FloatTy::Float) => Value::Float(FloatTy::Float, at.cast::<f32>().read_unaligned() as f64),
            Ty::Float(FloatTy::Double) => Value::Float(FloatTy::Double, at.cast::<f64>().read_unaligned()),
            Ty::Bool => Value::Bool(at.read() != 0),
            Ty::Char => {
                let code = at.cast::<u32>().read_unaligned();
                let Some(found) = char::from_u32(code) else {
                    return Err(Diag::panic(span, format!("raw memory holds {code} where a `char` is read, and no character has that code")));
                };
                Value::Char(found)
            }
            Ty::Raw(_) => {
                let address = at.cast::<usize>().read_unaligned();
                Value::Raw(RawPtr { addr: address, block: raw::block_at(address) })
            }
            Ty::CFunc(..) => Value::Raw(RawPtr { addr: at.cast::<usize>().read_unaligned(), block: None }),
            Ty::Array(item, length) => {
                let (size, _) = raw::layout(item, plain);
                let items = (0..*length).map(|at| load(addr + at * size, item, plain, span).map(cell)).collect::<Result<Vec<Cell>, Diag>>()?;
                Value::Array(Rc::new(items))
            }
            // A union is kept as its bytes, which each read of a field takes as that field.
            Ty::Struct(name) if plain[name].union => {
                let (size, _) = raw::layout(ty, plain);
                let bytes = (0..size).map(|at| cell(Value::Int(IntTy::Byte, *at_byte(addr + at) as i128))).collect();
                Value::Struct(name.as_str().into(), Fields::plain(vec![cell(Value::Array(Rc::new(bytes)))]))
            }
            Ty::Struct(name) => {
                let decl = &plain[name];
                let fields = decl
                    .fields
                    .iter()
                    .zip(raw::places(name, plain))
                    .map(|(field, place)| match place {
                        raw::Place::Bytes(at) => load(addr + at, field, plain, span).map(cell),
                        raw::Place::Bits { at, width } => Ok(cell(load_bits(addr, at, width, field))),
                    })
                    .collect::<Result<Vec<Cell>, Diag>>()?;
                Value::Struct(name.as_str().into(), Fields::plain(fields))
            }
            other => unreachable!("raw memory holds plain data, not `{other}`"),
        })
    }
}

/// Writes `value`, of type `ty`, at `addr`, which the caller has checked.
fn store(addr: usize, ty: &Ty, value: &Value, plain: &crate::raw::PlainTypes) {
    let value = strip(value.clone());
    // SAFETY: as for `load`.
    unsafe {
        let at = addr as *mut u8;
        match (ty, &value) {
            (Ty::Atomic(inner), _) => store(addr, inner, &value, plain),
            (Ty::Int(int), Value::Int(_, number)) => raw::write_int(addr, *int, *number),
            (Ty::Float(FloatTy::Float), Value::Float(_, number)) => at.cast::<f32>().write_unaligned(*number as f32),
            (Ty::Float(FloatTy::Double), Value::Float(_, number)) => at.cast::<f64>().write_unaligned(*number),
            (Ty::Bool, Value::Bool(truth)) => at.write(u8::from(*truth)),
            (Ty::Char, Value::Char(found)) => at.cast::<u32>().write_unaligned(*found as u32),
            (Ty::Raw(_) | Ty::CFunc(..), pointer) => at.cast::<usize>().write_unaligned(raw_of(pointer).addr),
            (Ty::Array(item, _), Value::Array(items)) => {
                let (size, _) = raw::layout(item, plain);
                for (position, held) in items.iter().enumerate() {
                    store(addr + position * size, item, &held.borrow(), plain);
                }
            }
            (Ty::Struct(name), Value::Struct(_, fields)) if plain[name].union => {
                let Value::Array(bytes) = &*fields[0].borrow() else { unreachable!("a union is kept as its bytes") };
                for (position, byte) in bytes.iter().enumerate() {
                    let Value::Int(_, byte) = *byte.borrow() else { unreachable!("a union is kept as its bytes") };
                    at.add(position).write(byte as u8);
                }
            }
            (Ty::Struct(name), Value::Struct(_, fields)) => {
                for ((field, place), held) in plain[name].fields.iter().zip(raw::places(name, plain)).zip(fields.iter()) {
                    match place {
                        raw::Place::Bytes(at) => store(addr + at, field, &held.borrow(), plain),
                        raw::Place::Bits { at, width } => store_bits(addr, at, width, &held.borrow()),
                    }
                }
            }
            (other, _) => unreachable!("raw memory holds plain data, not `{other}`"),
        }
    }
}

/// A window of `Memory.viewRef`: cells of its own, kept in step with the raw memory under it.
/// What the program wrote into a cell goes to the memory before raw memory is reached again, and
/// what was written into the memory comes back into the cells after; each side writes only the
/// items it changed. `memory` and `cells` are the bytes of each side at the last step.
struct Window {
    items: Rc<Vec<Cell>>,
    at: RawPtr,
    item: Ty,
    memory: Vec<u8>,
    cells: Vec<u8>,
}

thread_local! {
    /// The windows of `Memory.viewRef` of this thread that may still be used.
    static WINDOWS: RefCell<Vec<Window>> = const { RefCell::new(Vec::new()) };
}

/// The bytes of `value`, of type `ty`, as they would lie in raw memory, padding as zero.
fn bytes_of(value: &Value, ty: &Ty, plain: &crate::raw::PlainTypes) -> Vec<u8> {
    let (size, _) = raw::layout(ty, plain);
    let mut room = vec![0u8; size.max(1)];
    store(room.as_mut_ptr() as usize, ty, value, plain);
    room.truncate(size);
    room
}

/// The `count` bytes at `addr`.
fn bytes_at(addr: usize, count: usize) -> Vec<u8> {
    // SAFETY: the caller checked that the bytes are there.
    unsafe { std::slice::from_raw_parts(addr as *const u8, count) }.to_vec()
}

/// Writes what the program changed in a window into the memory under it, and lets go of the
/// windows nothing uses any more.
fn windows_out(plain: &crate::raw::PlainTypes) {
    WINDOWS.with(|windows| {
        let mut windows = windows.borrow_mut();
        for window in windows.iter_mut() {
            let (size, _) = raw::layout(&window.item, plain);
            for (position, held) in window.items.iter().enumerate() {
                let now = bytes_of(&held.borrow(), &window.item, plain);
                let range = position * size..(position + 1) * size;
                if now != window.cells[range.clone()] {
                    let at = window.at.addr + position * size;
                    store(at, &window.item, &held.borrow(), plain);
                    window.cells[range.clone()].copy_from_slice(&now);
                    window.memory[range].copy_from_slice(&bytes_at(at, size));
                }
            }
        }
        windows.retain(|window| Rc::strong_count(&window.items) > 1);
    });
}

/// Reads into the cells of each window what was written into the memory under it since. Only
/// a window over a block of `Memory.allocate` that is still live: memory C gave may be given back
/// without a word.
fn windows_in(plain: &crate::raw::PlainTypes, span: Span) -> Result<(), Diag> {
    WINDOWS.with(|windows| {
        let mut windows = windows.borrow_mut();
        windows.retain(|window| window.at.block.is_none() || raw::check(window.at, window.memory.len()).is_ok());
        for window in windows.iter_mut().filter(|window| window.at.block.is_some()) {
            let (size, _) = raw::layout(&window.item, plain);
            for (position, held) in window.items.iter().enumerate() {
                let at = window.at.addr + position * size;
                let range = position * size..(position + 1) * size;
                let now = bytes_at(at, size);
                if now != window.memory[range.clone()] {
                    let value = load(at, &window.item, plain, span)?;
                    window.cells[range.clone()].copy_from_slice(&bytes_of(&value, &window.item, plain));
                    *held.borrow_mut() = value;
                    window.memory[range].copy_from_slice(&now);
                }
            }
        }
        Ok(())
    })
}

/// A place in raw memory: where it starts, the type of what lies there, and for a bit field
/// its bits from `at`.
struct RawSpot {
    at: RawPtr,
    ty: Ty,
    bits: Option<(usize, u32)>,
}

impl RawSpot {
    fn read(&self, plain: &crate::raw::PlainTypes, span: Span) -> Result<Value, Diag> {
        match self.bits {
            Some((at, width)) => Ok(load_bits(self.at.addr, at, width, &self.ty)),
            None => load(self.at.addr, &self.ty, plain, span),
        }
    }

    fn write(&self, value: &Value, plain: &crate::raw::PlainTypes) {
        match self.bits {
            Some((at, width)) => store_bits(self.at.addr, at, width, value),
            None => store(self.at.addr, &self.ty, value, plain),
        }
    }
}

/// The struct a value of this type is, behind pointers and borrows.
fn struct_name(ty: &Ty) -> Option<&str> {
    match ty {
        Ty::Read(inner) | Ty::Ref(inner) | Ty::Ptr(inner) | Ty::Shared(inner, _) => struct_name(inner),
        Ty::Struct(name) => Some(name),
        _ => None,
    }
}

/// A union with these bytes.
fn union_value(name: &str, bytes: &[u8]) -> Value {
    let cells = bytes.iter().map(|byte| cell(Value::Int(IntTy::Byte, *byte as i128))).collect();
    Value::Struct(name.into(), Fields::plain(vec![cell(Value::Array(Rc::new(cells)))]))
}

/// The bytes of a union.
fn union_bytes(fields: &Fields) -> Vec<u8> {
    let Value::Array(bytes) = &*fields[0].borrow() else { unreachable!("a union is kept as its bytes") };
    bytes
        .iter()
        .map(|byte| match *byte.borrow() {
            Value::Int(_, byte) => byte as u8,
            _ => unreachable!("a union is kept as its bytes"),
        })
        .collect()
}

/// A panic when `value` does not fit a bit field of `width` bits; a `bool` always does.
fn fits_bits(value: &Value, width: u32, span: Span) -> Result<(), Diag> {
    let Value::Int(int, number) = strip(value.clone()) else { return Ok(()) };
    let (low, high) = if int.signed() { (-(1i128 << (width - 1)), (1i128 << (width - 1)) - 1) } else { (0, (1i128 << width) - 1) };
    if number < low || number > high {
        return Err(Diag::panic(span, format!("{number} does not fit in a bit field of {width} bit(s), which holds {low} to {high}")));
    }
    Ok(())
}

/// The byte at `addr`.
///
/// # Safety
/// The byte is there to read.
unsafe fn at_byte<'a>(addr: usize) -> &'a u8 {
    &*(addr as *const u8)
}

/// The bits of a bit field: `width` bits from bit `at` of the value at `addr`, as a value of
/// the field's type (an integer, signed when its type is, or a `bool`).
fn load_bits(addr: usize, at: usize, width: u32, ty: &Ty) -> Value {
    let mut word = 0u128;
    for (position, byte) in (at / 8..(at + width as usize).div_ceil(8)).enumerate() {
        // SAFETY: the caller checked that the bytes of the whole value are there.
        word |= (unsafe { *at_byte(addr + byte) } as u128) << (8 * position);
    }
    let bits = (word >> (at % 8)) & ((1u128 << width) - 1);
    match ty {
        Ty::Bool => Value::Bool(bits != 0),
        Ty::Int(int) if int.signed() && bits >> (width - 1) == 1 => Value::Int(*int, bits as i128 - (1i128 << width)),
        Ty::Int(int) => Value::Int(*int, bits as i128),
        other => unreachable!("a bit field is an integer or a `bool`, not `{other}`"),
    }
}

/// Writes `value` into the bits of a bit field, and leaves the bits around it as they are. The
/// value fits: the interpreter checks it where it is given.
fn store_bits(addr: usize, at: usize, width: u32, value: &Value) {
    let number = match strip(value.clone()) {
        Value::Bool(truth) => u128::from(truth),
        Value::Int(_, number) => number as u128,
        _ => unreachable!("a bit field holds an integer or a `bool`"),
    };
    let mask = ((1u128 << width) - 1) << (at % 8);
    let bytes = at / 8..(at + width as usize).div_ceil(8);
    let mut word = 0u128;
    for (position, byte) in bytes.clone().enumerate() {
        // SAFETY: as for `load_bits`.
        word |= (unsafe { *at_byte(addr + byte) } as u128) << (8 * position);
    }
    word = (word & !mask) | ((number << (at % 8)) & mask);
    for (position, byte) in bytes.enumerate() {
        // SAFETY: as for `load_bits`; the value is written where it lies.
        unsafe { ((addr + byte) as *mut u8).write((word >> (8 * position)) as u8) };
    }
}

fn cell(value: Value) -> Cell {
    Rc::new(RefCell::new(value))
}

/// The value behind any borrows.
fn strip(mut value: Value) -> Value {
    while let Value::Borrow(target) = value {
        value = target.borrow().clone();
    }
    value
}

/// The value behind any borrows, pointers and `shared` values.
fn owned(mut value: Value) -> Value {
    while let Some(target) = behind(&value) {
        value = target.borrow().clone();
    }
    value
}

/// A value read out of a variable or a field gets cells of its own, so that a copy of a
/// `Copyable` struct can be changed without changing the original. What a `T*` owns stays
/// where it is: a pointer is only ever moved.
fn fresh(value: &Value) -> Value {
    let cells = |fields: &Rc<Vec<Cell>>| Rc::new(fields.iter().map(|field| cell(fresh(&field.borrow()))).collect());
    match value {
        // There is one value of a `Disposable` type, however often it is read: it is disposed
        // of once, when the last place that holds it lets it go.
        Value::Struct(_, fields) if fields.disposed_as.is_some() => value.clone(),
        Value::Struct(name, fields) => Value::Struct(name.clone(), Fields::plain(fields.iter().map(|field| cell(fresh(&field.borrow()))).collect())),
        Value::Enum(name, index, fields) => Value::Enum(name.clone(), *index, cells(fields)),
        Value::Array(items) => Value::Array(cells(items)),
        other => other.clone(),
    }
}

/// A list, an array or a slice as the cells of its items; text as its bytes, which a slice
/// only reads.
fn cells_of(value: &Value) -> Rc<Vec<Cell>> {
    match value {
        Value::List(items) => Rc::new(items.borrow().clone()),
        Value::Array(items) | Value::Slice(items) => items.clone(),
        Value::Str(text) => Rc::new(text.bytes().map(|byte| cell(Value::Int(IntTy::Byte, byte as i128))).collect()),
        _ => unreachable!("the checker only lets a slice look at items"),
    }
}

/// What a method of a list finds when it is called on an array or a slice: the same cells.
fn as_list(value: Value) -> Value {
    match value {
        Value::Array(items) | Value::Slice(items) => Value::List(Rc::new(RefCell::new((*items).clone()))),
        other => other,
    }
}

/// The value every item of `byte[16]` starts with.
fn zero(ty: &Ty) -> Value {
    match ty {
        Ty::Int(ty) => Value::Int(*ty, 0),
        Ty::Float(ty) => Value::Float(*ty, 0.0),
        Ty::Bool => Value::Bool(false),
        Ty::Nullable(_) => Value::Null,
        _ => unreachable!("the checker only lets a type with a zero value be filled this way"),
    }
}

/// The cell a `ref` or `read` variable stands for.
/// A `string`, also behind borrows and pointers: `p.write(x)` with `string* p`.
fn is_text_behind(mut ty: &Ty) -> bool {
    while let Ty::Read(inner) | Ty::Ref(inner) | Ty::Ptr(inner) = ty {
        ty = inner;
    }
    *ty == Ty::Str
}

fn behind_borrows(mut target: Cell) -> Cell {
    loop {
        let inner = match &*target.borrow() {
            Value::Borrow(inner) => inner.clone(),
            _ => break,
        };
        target = inner;
    }
    target
}

impl Value {
    /// How a number, a `bool` or text is written. Structs and enums go through `Interp::show`.
    fn format(&self) -> String {
        match self {
            Value::Int(_, value) => value.to_string(),
            Value::Float(ty, value) => show_float(*value, *ty),
            Value::Bool(value) => value.to_string(),
            Value::Char(value) => value.to_string(),
            Value::Str(text) | Value::Fmt(text) => text.to_string(),
            _ => unreachable!("the checker only lets printable values through"),
        }
    }

    /// How the value is written in an `assert` message: strings are quoted.
    fn debug(&self) -> String {
        match self {
            Value::Str(text) | Value::Fmt(text) => format!("{text:?}"),
            Value::Char(value) => quoted_char(*value),
            Value::Null => "null".to_string(),
            Value::Int(..) | Value::Float(..) | Value::Bool(_) => self.format(),
            _ => "(value)".to_string(),
        }
    }
}

/// Why the program stopped.
#[derive(Debug)]
pub enum Stop {
    Diag(Diag),
    /// Output was closed (`program | head`): exit silently.
    OutputClosed,
    /// Writing to output failed: stop with a message.
    OutputFailed(String),
    /// `main` returned an error: the text of the error.
    Failed(String),
    /// The test command ran and at least one test failed; the report is already written.
    TestsFailed,
    /// `exit(code)`: the program ends with this code.
    Exit(u8),
}

/// What a program sees around it besides its output: the words of its command line, its
/// input and the standard error.
pub struct World<'w> {
    /// The words after the program's name, as the system gave them: they are checked to be
    /// UTF-8 when `main` takes them.
    pub args: Vec<Vec<u8>>,
    pub input: Box<dyn Read + Send + 'static>,
    /// The input is a terminal: what was printed goes out before the program waits for a line.
    pub input_is_terminal: bool,
    /// The input can make the program wait (a pipe, a terminal): an async read of it waits on
    /// a thread of its own, and a cancel can end the wait. Input in memory or in a file is
    /// read where it is asked for.
    pub input_waits: bool,
    pub err: &'w mut (dyn Write + Send + 'static),
}

impl From<Diag> for Stop {
    fn from(diag: Diag) -> Stop {
        Stop::Diag(diag)
    }
}

/// Why evaluation is being abandoned. Besides a stop of the whole program, an arm of a
/// `match` can leave from inside an expression with `return`, `break` or `continue`; these
/// travel up to the call or the loop they belong to.
enum Unwind {
    Stop(Stop),
    Return(Value),
    Break,
    Continue,
    /// Another fiber stopped the run, and this one ends without a word.
    Abandon,
}

impl From<Diag> for Unwind {
    fn from(diag: Diag) -> Unwind {
        ending();
        Unwind::Stop(Stop::Diag(diag))
    }
}

impl From<Stop> for Unwind {
    fn from(stop: Stop) -> Unwind {
        Unwind::Stop(stop)
    }
}

type Res<T> = Result<T, Unwind>;

enum Flow {
    Next,
    Break,
    Continue,
    Return(Value),
}

/// Moves the names of an `is` that the checker lets the code after an `if` or `while` see
/// out of the scope of its condition, into the scope around the statement.
fn keep_names(after: &[String], mut names: Scope, frame: &mut Frame) {
    for name in after {
        if let Some(value) = names.remove(name) {
            frame.scopes.last_mut().expect("an open scope").insert(name.clone(), value);
        }
    }
}

/// What a loop body did, when it left from inside an expression: a `break` or `continue`
/// in a `match` arm ends here.
fn loop_flow(result: Res<Flow>) -> Res<Flow> {
    match result {
        Err(Unwind::Break) => Ok(Flow::Break),
        Err(Unwind::Continue) => Ok(Flow::Continue),
        other => other,
    }
}

/// The variant and the cells of an enum value, behind pointers and borrows.
fn enum_parts(mut value: Value) -> (usize, Rc<Vec<Cell>>) {
    loop {
        value = match value {
            Value::Enum(_, index, fields) => return (index, fields),
            Value::Ptr(inner) | Value::Borrow(inner) => inner.borrow().clone(),
            Value::Shared(held) => held.value.borrow().clone(),
            _ => unreachable!("the checker only lets enums be matched"),
        };
    }
}

/// Whether the value in `held` matches the pattern, and what the names in it take: what the
/// value holds there, or with `borrowed` a borrow of it. Through a borrow the names borrow.
fn fits(pattern: &Pattern, held: &Cell, borrowed: bool, binds: &mut Vec<(String, Value)>) -> bool {
    // A pattern other than a name, `_` or `null` takes only a value that is not `null`.
    let null = matches!(owned(held.borrow().clone()), Value::Null);
    match &pattern.kind {
        PatternKind::Wildcard => true,
        PatternKind::Null => null,
        PatternKind::Or(alternatives) => {
            let before = binds.len();
            alternatives.iter().any(|alternative| {
                binds.truncate(before);
                fits(alternative, held, borrowed, binds)
            })
        }
        _ if null && !matches!(pattern.kind, PatternKind::Bind(_)) => false,
        PatternKind::Bool(wanted) => matches!(strip(held.borrow().clone()), Value::Bool(found) if found == *wanted),
        PatternKind::Range(low, high) => {
            let (PatternKind::Int(low), PatternKind::Int(high)) = (&low.kind, &high.kind) else {
                unreachable!("the checker turns the ends of a range into numbers")
            };
            let found = match strip(held.borrow().clone()) {
                Value::Int(_, number) => number,
                Value::Char(value) => value as i128,
                _ => unreachable!("the checker lets a range match only a number or a character"),
            };
            (*low..=*high).contains(&found)
        }
        PatternKind::Bind(name) => {
            let bound = if borrowed { Value::Borrow(held.clone()) } else { held.borrow().clone() };
            binds.push((name.clone(), bound));
            true
        }
        PatternKind::Int(wanted) => match strip(held.borrow().clone()) {
            Value::Int(_, number) => number == *wanted,
            Value::Char(value) => value as i128 == *wanted,
            _ => unreachable!("the checker lets a number match only a number or a character"),
        },
        PatternKind::Char(_) => unreachable!("the checker turns a character into a number"),
        PatternKind::Str(text) => match strip(held.borrow().clone()) {
            Value::Str(found) => *found == *text,
            _ => unreachable!("the checker lets a text match only a text"),
        },
        PatternKind::Variant { index, args, .. } => {
            let value = held.borrow().clone();
            let borrowed = borrowed || matches!(value, Value::Borrow(_));
            let (found, fields) = enum_parts(value);
            found == *index && args.iter().flatten().zip(fields.iter()).all(|(arg, field)| fits(arg, field, borrowed, binds))
        }
    }
}

/// Variables of one function call.
struct Frame {
    scopes: Vec<Scope>,
}

/// The names of one block, in the order they were declared. They end in the reverse order
/// (`dispose` order): what is declared later may have borrowed from what is declared earlier.
#[derive(Default)]
struct Scope(Vec<(String, Cell)>);

impl Scope {
    fn get(&self, name: &str) -> Option<&Cell> {
        self.0.iter().rev().find(|(each, _)| each == name).map(|(_, slot)| slot)
    }

    fn insert(&mut self, name: String, slot: Cell) {
        match self.0.iter_mut().find(|(each, _)| *each == name) {
            Some(found) => found.1 = slot,
            None => self.0.push((name, slot)),
        }
    }

    fn remove(&mut self, name: &str) -> Option<Cell> {
        let at = self.0.iter().position(|(each, _)| each == name)?;
        Some(self.0.remove(at).1)
    }

    /// The newest name, taken out to end.
    fn pop(&mut self) -> Option<Cell> {
        self.0.pop().map(|(_, slot)| slot)
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        while self.0.pop().is_some() {}
    }
}

impl Frame {
    fn slot(&self, name: &str) -> Cell {
        match self.scopes.iter().rev().find_map(|scope| scope.get(name)) {
            Some(slot) => slot.clone(),
            None => stream_slot(name).expect("names are checked before running"),
        }
    }

    fn declare(&mut self, name: &str, value: Value) {
        self.scopes.last_mut().expect("an open scope").insert(name.to_string(), cell(value));
    }
}

/// What the fibers of a run share. The input is taken out of the world into a block of its own.
fn shared<'a>(out: &'a mut dyn Write, mut world: World<'a>, ffi: crate::ffi::Library, sources: &'a crate::source::Sources) -> Shared<'a> {
    let source = std::mem::replace(&mut world.input, Box::new(std::io::empty()));
    let input = crate::input::Input::new(source, world.input_waits);
    Shared { out: RefCell::new(out), world: RefCell::new(world), input: RefCell::new(input), test_err: RefCell::new(String::new()), sched: Sched::new(), ffi, children: RefCell::new(HashMap::new()), sources, handles: RefCell::new(HashMap::new()) }
}

pub fn run_program<'a>(prog: &'a Program, sources: &'a crate::source::Sources, out: &'a mut dyn Write, world: World<'a>) -> Result<(), Stop> {
    let src = sources.text.as_str();
    let funcs = prog.funcs.iter().map(|func| (func.name.as_str(), func)).collect();
    let mut methods: HashMap<&str, HashMap<&str, &Func>> = HashMap::new();
    for decl in &prog.structs {
        methods.insert(&decl.name, decl.methods.iter().map(|method| (method.name.as_str(), method)).collect());
    }
    for decl in &prog.enums {
        methods.insert(&decl.name, decl.methods.iter().map(|method| (method.name.as_str(), method)).collect());
    }
    let structs = prog.structs.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let enums = prog.enums.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let aliases = prog.aliases.iter().map(|alias| (alias.name.as_str(), &alias.ty)).collect();
    let ffi = crate::ffi::Library::open(prog, false).map_err(Stop::Diag)?;
    let io = shared(out, world, ffi, sources);
    let consts = Consts::given(prog);
    std::thread::scope(|scope| {
        let mut interp = Interp { funcs, methods, structs, enums, src, io: &io, scope, me: 0, depth: 0, in_test: false, guards: Vec::new(), aliases, started: Vec::new(), plain: &prog.plain, foreign: &prog.foreign, std_from: std_from(prog), errno: [0; 2], locking: 0, atomics: 0, consts: &consts };
        let result = run_main(&mut interp);
        interp.end_fibers();
        result
    })
}

fn run_main(interp: &mut Interp) -> Result<(), Stop> {
    let first = LIVE.with(|live| live.borrow().next);
    open_streams();
    let main = interp.funcs["main"];
    // `main(List<string> args)`: the words of the command line, each checked to be text.
    let mut args = Vec::new();
    if !main.params.is_empty() {
        let mut words = Vec::new();
        let given = std::mem::take(&mut interp.io.world.borrow_mut().args);
        for (number, word) in given.into_iter().enumerate() {
            match String::from_utf8(word) {
                Ok(word) => words.push(cell(Value::Str(Rc::new(word)))),
                Err(_) => return Err(Stop::Failed(format!("args[{number}] is not valid UTF-8"))),
            }
        }
        args.push(Value::List(Rc::new(RefCell::new(words))));
    }
    let result = match interp.call(main, None, args, main.name_span) {
        // `Result<void, E> main()` ended with `Error(cause)`: the cause is written out.
        Ok(Value::Enum(_, 1, fields)) => {
            let cause = fields[0].borrow().clone();
            interp.show(&cause, false, main.name_span).and_then(|text| Err(Unwind::Stop(Stop::Failed(text))))
        }
        other => other,
    };
    match result {
        // What `main` owned is gone; a `shared` value still alive holds itself up.
        Ok(_) => match interp.leaked(first, "the program") {
            Some(report) => Err(Stop::Diag(report)),
            None => Ok(()),
        },
        Err(Unwind::Stop(stop)) => Err(stop),
        Err(_) => unreachable!("`return` ends at its call, `break` and `continue` at their loop; fiber 0 is not abandoned"),
    }
}

/// Runs every `test` block and writes one line for each: `ok` or `FAIL` with the reason.
/// Gives how many passed and how many failed. A test that fails does not stop the others.
pub fn run_tests<'a>(prog: &'a Program, sources: &'a crate::source::Sources, out: &'a mut dyn Write, world: World<'a>) -> Result<(usize, usize), Stop> {
    let src = sources.text.as_str();
    let funcs = prog.funcs.iter().map(|func| (func.name.as_str(), func)).collect();
    let mut methods: HashMap<&str, HashMap<&str, &Func>> = HashMap::new();
    for decl in &prog.structs {
        methods.insert(&decl.name, decl.methods.iter().map(|method| (method.name.as_str(), method)).collect());
    }
    for decl in &prog.enums {
        methods.insert(&decl.name, decl.methods.iter().map(|method| (method.name.as_str(), method)).collect());
    }
    let structs = prog.structs.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let enums = prog.enums.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let aliases = prog.aliases.iter().map(|alias| (alias.name.as_str(), &alias.ty)).collect();
    let ffi = crate::ffi::Library::open(prog, true).map_err(Stop::Diag)?;
    let io = shared(out, world, ffi, sources);
    let consts = Consts::given(prog);
    std::thread::scope(|scope| {
        let mut interp = Interp { funcs, methods, structs, enums, src, io: &io, scope, me: 0, depth: 0, in_test: true, guards: Vec::new(), aliases, started: Vec::new(), plain: &prog.plain, foreign: &prog.foreign, std_from: std_from(prog), errno: [0; 2], locking: 0, atomics: 0, consts: &consts };
        run_each_test(&mut interp, prog, sources)
    })
}

fn run_each_test<'a>(interp: &mut Interp<'a, '_>, prog: &'a Program, sources: &crate::source::Sources) -> Result<(usize, usize), Stop> {
    let (mut passed, mut failed) = (0, 0);
    for test in &prog.tests {
        // Nothing is shared between tests: each starts with variables of its own.
        interp.depth = 0;
        interp.io.test_err.borrow_mut().clear();
        open_streams();
        let first = LIVE.with(|live| live.borrow().next);
        let mut frame = Frame { scopes: vec![Scope::default()] };
        let ended = match interp.block(&test.body, &mut frame) {
            Ok(Flow::Return(value)) | Err(Unwind::Return(value)) => Ok(value),
            Ok(_) => Ok(Value::Void),
            Err(other) => Err(other),
        };
        drop(frame);
        interp.guards.clear();
        // A test that ended with an error leaves the tasks it did not wait for: they run now.
        // One that stopped leaves the fibers that still wait: they end.
        let ended = match ended {
            Err(Unwind::Stop(stop)) => {
                interp.started.clear();
                interp.end_fibers();
                Err(Unwind::Stop(stop))
            }
            ended => match interp.unwaited() {
                Ok(()) => ended,
                Err(stop) => {
                    interp.started.clear();
                    interp.end_fibers();
                    Err(stop)
                }
            },
        };
        // A test that passes and leaves `shared` values alive fails with the report.
        let ended = match ended {
            Ok(Value::Enum(name, 1, fields)) => Ok(Value::Enum(name, 1, fields)),
            Ok(value) => match interp.leaked(first, "the test") {
                Some(report) => Err(Unwind::Stop(Stop::Diag(report))),
                None => Ok(value),
            },
            other => other,
        };
        let reason = match ended {
            // `try` met an error: the test fails with it.
            Ok(Value::Enum(_, 1, fields)) => {
                let cause = fields[0].borrow().clone();
                match interp.show(&cause, false, test.name_span) {
                    Ok(text) => Some(format!("error: {text}\n")),
                    Err(Unwind::Stop(stop)) => return Err(stop),
                    Err(_) => unreachable!("writing a value does not leave a loop or a function"),
                }
            }
            Ok(_) => None,
            Err(Unwind::Stop(Stop::Diag(diag))) => Some(diag.render_in(sources)),
            Err(Unwind::Stop(stop)) => return Err(stop),
            Err(_) => unreachable!("`break` and `continue` end at their loop"),
        };
        let reason = reason.map(|reason| format!("{}{reason}", interp.io.test_err.borrow()));
        // A test of a module is written with its module in front: `stock: nothing ships ...`.
        let name = match sources.file_at(test.name_span.start).module.as_str() {
            "" => test.name.clone(),
            module => format!("{module}: {}", test.name),
        };
        let line = match &reason {
            None => {
                passed += 1;
                format!("ok    {name}")
            }
            Some(reason) => {
                failed += 1;
                format!("FAIL  {name}\n{}", reason.trim_end())
            }
        };
        match interp.write_line(&line) {
            Ok(()) => {}
            Err(Unwind::Stop(stop)) => return Err(stop),
            Err(_) => unreachable!("writing a line does not leave a loop or a function"),
        }
    }
    let summary = match failed {
        0 => format!("{passed} test(s) passed"),
        _ => format!("{failed} of {} test(s) failed", passed + failed),
    };
    match interp.write_line(&summary) {
        Ok(()) => Ok((passed, failed)),
        Err(Unwind::Stop(stop)) => Err(stop),
        Err(_) => unreachable!("writing a line does not leave a loop or a function"),
    }
}

struct Interp<'a, 'e> {
    funcs: HashMap<&'a str, &'a Func>,
    /// Functions written in struct bodies: struct name, then function name.
    methods: HashMap<&'a str, HashMap<&'a str, &'a Func>>,
    structs: HashMap<&'a str, &'a StructDecl>,
    enums: HashMap<&'a str, &'a EnumDecl>,
    src: &'a str,
    /// The output, the world, and what the running test wrote with `printError` (shown under
    /// the test's `FAIL` line, and dropped when the test passes, as compiled tests do).
    io: &'a Shared<'e>,
    /// Where the threads of fibers are started.
    scope: &'a std::thread::Scope<'a, 'e>,
    /// The fiber this runs on.
    me: usize,
    depth: usize,
    /// Running the test command: `exit` fails the test instead of ending the program.
    in_test: bool,
    /// The borrows of `shared checked` values in progress, the latest last. Each part of the
    /// program that lends one drops what it added when it ends.
    guards: Vec<Guard>,
    aliases: HashMap<&'a str, &'a TypeExpr>,
    /// The tasks this fiber started that it has not waited for yet.
    started: Vec<Rc<RefCell<Task>>>,
    /// The types of the fields of each struct of plain data: how it lies in raw memory.
    plain: &'a crate::raw::PlainTypes,
    /// What each function of C takes and gives (see `Program::foreign`).
    foreign: &'a HashMap<String, (Vec<Ty>, Ty)>,
    /// Where the files of the language's own module start in the text: they come after the
    /// program's. A panic in them is shown at the call that came from outside (see `call`).
    std_from: usize,
    /// The `errno` of this fiber and, on Windows, its last error of the system
    /// (`GetLastError`): what the last call of C left, and what the next one starts with
    /// (`C.errno()`, `C.setErrno(n)`, `C.lastError()`, `C.setLastError(n)`). Kept here, since
    /// the interpreter's own work between two calls of C may change the system's.
    errno: [i32; 2],
    /// How many `lock` blocks this fiber is inside: an atomic operation there does not give the
    /// turn away, since no other task may take a lock this one holds.
    locking: usize,
    /// How many atomic operations this fiber has done: the turn goes to another every
    /// `ATOMICS_PER_TURN` of them.
    atomics: u64,
    /// The values of the consts.
    consts: &'a Consts<'a>,
}

/// The values of the consts, each in a cell of its own, made the first time it is read. When
/// the program runs they come from what the checker found; when the checker finds them, from
/// running each initializer, the consts it names first.
pub struct Consts<'a> {
    inits: HashMap<&'a str, &'a Expr>,
    given: &'a BTreeMap<String, (Ty, ConstVal)>,
    embeds: &'a [Embed],
    cells: RefCell<HashMap<String, Cell>>,
    /// The consts being found, the outer first: one that needs itself is found here.
    finding: RefCell<Vec<String>>,
}

impl<'a> Consts<'a> {
    fn given(prog: &'a Program) -> Consts<'a> {
        Consts { inits: HashMap::new(), given: &prog.const_values, embeds: &prog.embeds, cells: RefCell::new(HashMap::new()), finding: RefCell::new(Vec::new()) }
    }
}

/// A value of a const as the interpreter holds it.
fn value_of(found: &ConstVal) -> Value {
    match found {
        ConstVal::Int(ty, value) => Value::Int(*ty, *value),
        ConstVal::Float(ty, value) => Value::Float(*ty, *value),
        ConstVal::Bool(value) => Value::Bool(*value),
        ConstVal::Char(value) => Value::Char(*value),
        ConstVal::Str(text) => Value::Str(text.as_str().to_string().into()),
        ConstVal::Array(items) => Value::Array(Rc::new(items.iter().map(|item| cell(value_of(item))).collect())),
        ConstVal::Bytes(bytes) => Value::Array(Rc::new(bytes.iter().map(|byte| cell(Value::Int(IntTy::Byte, *byte as i128))).collect())),
        ConstVal::Struct(name, fields) => Value::Struct(name.as_str().into(), Fields::plain(fields.iter().map(|field| cell(value_of(field))).collect())),
        ConstVal::Enum(name, index, fields) => Value::Enum(name.as_str().into(), *index, Rc::new(fields.iter().map(|field| cell(value_of(field))).collect())),
    }
}

/// What a const holds, out of the value the interpreter found for it.
fn const_of(found: &Value) -> ConstVal {
    match found {
        Value::Int(ty, value) => ConstVal::Int(*ty, *value),
        Value::Float(ty, value) => ConstVal::Float(*ty, *value),
        Value::Bool(value) => ConstVal::Bool(*value),
        Value::Char(value) => ConstVal::Char(*value),
        Value::Str(text) => ConstVal::Str(text.to_string()),
        Value::Borrow(target) => const_of(&target.borrow()),
        Value::Array(items) => {
            let items: Vec<ConstVal> = items.iter().map(|item| const_of(&item.borrow())).collect();
            match items.iter().all(|item| matches!(item, ConstVal::Int(IntTy::Byte, _))) {
                true => ConstVal::Bytes(items.iter().map(|item| match item {
                    ConstVal::Int(_, byte) => *byte as u8,
                    _ => unreachable!("tested above"),
                }).collect()),
                false => ConstVal::Array(items),
            }
        }
        Value::Struct(name, fields) => ConstVal::Struct(name.to_string(), fields.iter().map(|field| const_of(&field.borrow())).collect()),
        Value::Enum(name, index, fields) => ConstVal::Enum(name.to_string(), *index, fields.iter().map(|field| const_of(&field.borrow())).collect()),
        _ => unreachable!("the checker gives a const only plain values"),
    }
}

/// Finds the value of every const of the program, as the checker has checked its initializer:
/// the interpreter runs it, and a const it names is found first. A panic on the way (an
/// overflow, an index out of range) is a compile error, which tells the const it was found for.
pub fn find_consts(prog: &Program, src: &str) -> Result<BTreeMap<String, ConstVal>, Diag> {
    let sources = crate::source::Sources::single(src, "");
    let mut sink = std::io::sink();
    let mut err = std::io::sink();
    let world = World { args: Vec::new(), input: Box::new(std::io::empty()), input_is_terminal: false, input_waits: false, err: &mut err };
    let io = shared(&mut sink, world, crate::ffi::Library::default(), &sources);
    let none = BTreeMap::new();
    let mut consts = Consts { inits: HashMap::new(), given: &none, embeds: &prog.embeds, cells: RefCell::new(HashMap::new()), finding: RefCell::new(Vec::new()) };
    consts.inits = prog.consts.iter().map(|decl| (decl.name.as_str(), &decl.init)).collect();
    let funcs = prog.funcs.iter().map(|func| (func.name.as_str(), func)).collect();
    let structs = prog.structs.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let enums = prog.enums.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let aliases = prog.aliases.iter().map(|alias| (alias.name.as_str(), &alias.ty)).collect();
    std::thread::scope(|scope| {
        let mut interp = Interp { funcs, methods: HashMap::new(), structs, enums, src, io: &io, scope, me: 0, depth: 0, in_test: false, guards: Vec::new(), aliases, started: Vec::new(), plain: &prog.plain, foreign: &prog.foreign, std_from: std_from(prog), errno: [0; 2], locking: 0, atomics: 0, consts: &consts };
        let mut found = BTreeMap::new();
        for decl in &prog.consts {
            match interp.const_cell(&decl.name, decl.name_span) {
                Ok(slot) => {
                    found.insert(decl.name.clone(), const_of(&slot.borrow()));
                }
                Err(Unwind::Stop(Stop::Diag(mut diag))) => {
                    ENDING.with(|ending| ending.set(false));
                    if diag.kind == crate::diag::Kind::Panic {
                        let name = decl.local.clone().unwrap_or_else(|| short_name(&decl.name).to_string());
                        diag.kind = crate::diag::Kind::Error;
                        diag.msg = format!("the value of `{name}` is found when the program is compiled, and this stops it: {}", diag.msg);
                    }
                    return Err(diag);
                }
                Err(_) => unreachable!("a const is a value: nothing in it returns, breaks or waits"),
            }
        }
        Ok(found)
    })
}

/// How many atomic operations a fiber does before the turn goes to the next one that is ready;
/// `Atomic.spinLoop()` gives it at once.
const ATOMICS_PER_TURN: u64 = 64;

/// Where the first file of the language's own module starts; the end of the text if none was read in.
fn std_from(prog: &Program) -> usize {
    prog.files.iter().filter(|file| file.module == crate::source::STD_MODULE).map(|file| file.start).min().unwrap_or(usize::MAX)
}

impl<'a, 'e> Interp<'a, 'e> {
    /// The cell of a const, made the first time it is read.
    fn const_cell(&mut self, name: &str, span: Span) -> Res<Cell> {
        if let Some(found) = self.consts.cells.borrow().get(name) {
            return Ok(found.clone());
        }
        let value = match self.consts.given.get(name) {
            Some((_, found)) => value_of(found),
            None => {
                let init: &'a Expr = self.consts.inits[name];
                if self.consts.finding.borrow().iter().any(|each| each == name) {
                    let shown = |full: &str| full.split('#').next().map(|written| short_name(written).to_string()).unwrap_or_default();
                    let path: Vec<String> = self.consts.finding.borrow().iter().skip_while(|each| *each != name).map(|each| format!("`{}`", shown(each))).collect();
                    return Err(Diag::error(span, format!("the value of `{}` needs itself: {} -> `{}`", shown(name), path.join(" -> "), shown(name)))
                        .hint("a const is given a value that does not lead back to it")
                        .into());
                }
                self.consts.finding.borrow_mut().push(name.to_string());
                let found = self.expr(init, &mut Frame { scopes: vec![Scope::default()] });
                self.consts.finding.borrow_mut().pop();
                owned(found?)
            }
        };
        let slot = cell(value);
        self.consts.cells.borrow_mut().insert(name.to_string(), slot.clone());
        Ok(slot)
    }

    /// Runs the `export` function numbered `number` for C, with the arguments whose addresses
    /// C gave, and writes what it gives where C has room for it.
    fn entered(&mut self, number: usize, args: *mut *mut std::ffi::c_void, ret: *mut std::ffi::c_void) {
        let (name, params, gives) = self.io.ffi.export(number).clone();
        let func = self.funcs[name.as_str()];
        // The call of C gave the turn away: the function takes it back while it runs. One
        // that comes while the run ends (another task stopped it) still runs: C cannot be
        // unwound through, and the fiber unwinds when the call of C returns.
        let left = LEFT.with(|cell| cell.borrow_mut().take());
        if let Some(Leave { id, done }) = &left {
            done.tell(*id);
            let state = self.io.sched.lock();
            let mut state = self.io.sched.wait_turn(state, self.me);
            state.outside.remove(&self.me);
            match Sched::resume(&mut state, self.me) {
                Ok(()) | Err(Unwind::Abandon) => {}
                Err(Unwind::Stop(stop)) => {
                    drop(state);
                    self.die(stop);
                }
                Err(_) => unreachable!("the turn comes back with a stop or the end of the run"),
            }
        }
        self.run_entered(func, &params, &gives, args, ret);
        let Some(left) = left else { return };
        {
            let mut state = self.io.sched.lock();
            if state.ending {
                // The fiber that ends the run gets the turn back at once; this one takes it
                // again when the call of C returns, and unwinds.
                state.thread = Some(Thread::take());
                Sched::give(&mut state, 0);
                LEFT.with(|cell| *cell.borrow_mut() = Some(left));
                return;
            }
        }
        match self.c_leave(func.name_span) {
            Ok(left) => LEFT.with(|cell| *cell.borrow_mut() = left),
            Err(Unwind::Stop(stop)) => self.die(stop),
            Err(_) => unreachable!("giving the turn away stops the run or not"),
        }
    }

    /// Runs the `export` function `func` for C: its arguments from `args`, what it gives into
    /// `ret`.
    fn run_entered(&mut self, func: &'a Func, params: &[Ty], gives: &Ty, args: *mut *mut std::ffi::c_void, ret: *mut std::ffi::c_void) {
        let mut values = Vec::new();
        let mut lent = Vec::new();
        for (position, ty) in params.iter().enumerate() {
            // SAFETY: the stub gives the address of each argument, as the C type of its parameter.
            let at = unsafe { *args.add(position) } as usize;
            let value = match ty {
                // C's `const char*`: the text up to its zero byte, copied for the call.
                Ty::Read(inner) if **inner == Ty::CString => {
                    // SAFETY: the stub keeps the pointer C gave at `at`.
                    let address = unsafe { (at as *const usize).read() };
                    match address {
                        0 => Err(Diag::panic(func.name_span, format!("C gave `null` to `{}`, which takes a `read CString`", short_name(&func.name)))),
                        // SAFETY: C promises a text that ends in a zero byte.
                        _ => Ok(Value::Borrow(cell(Value::CStr(Rc::new(unsafe { std::ffi::CStr::from_ptr(address as *const std::ffi::c_char) }.to_owned()))))),
                    }
                }
                Ty::Ptr(_) | Ty::Nullable(_) | Ty::Read(_) | Ty::Ref(_) => {
                    // SAFETY: a value C holds is passed as an address, which the stub keeps at `at`.
                    let address = unsafe { (at as *const usize).read() };
                    self.handle_in(func, ty, address, &mut lent)
                }
                _ => load(at, ty, self.plain, func.name_span),
            };
            match value {
                Ok(value) => values.push(value),
                Err(diag) => self.die(Stop::Diag(diag)),
            }
        }
        let result = self.call(func, None, values, func.name_span);
        {
            let mut handles = self.io.handles.borrow_mut();
            for address in lent {
                if let Some(handle) = handles.get_mut(&address) {
                    handle.lent = if handle.lent < 0 { 0 } else { handle.lent - 1 };
                }
            }
        }
        match result {
            Ok(value) => match gives {
                Ty::Void => {}
                Ty::Ptr(inner) | Ty::Nullable(inner) => {
                    let address = match strip(value) {
                        Value::Ptr(held) => {
                            let address = Rc::as_ptr(&held) as usize;
                            let ty = match inner.as_ref() {
                                Ty::Ptr(target) => (**target).clone(),
                                other => other.clone(),
                            };
                            self.io.handles.borrow_mut().insert(address, Handle { value: held, ty, lent: 0, lent_to: String::new() });
                            address
                        }
                        _ => 0,
                    };
                    // SAFETY: `ret` has room for the address C is given.
                    unsafe { (ret as *mut usize).write(address) };
                }
                _ => store(ret as usize, gives, &value, self.plain),
            },
            Err(Unwind::Stop(stop)) => self.die(stop),
            Err(_) => unreachable!("`return` ends at its call, `break` and `continue` at their loop"),
        }
    }

    /// The value of SG C gives an `export` function by its address: taken back for a `T*`,
    /// lent for a `read T` or a `ref T`. An address the interpreter did not give for a value
    /// of that type, one given back already, and one lent in a way the borrow rules forbid,
    /// are panics. What is lent is put in `lent`, to be given back when the call ends.
    fn handle_in(&self, func: &Func, ty: &Ty, address: usize, lent: &mut Vec<usize>) -> Result<Value, Diag> {
        let name = short_name(&func.name);
        let (target, owned, nullable, writes) = match ty {
            Ty::Nullable(inner) => match inner.as_ref() {
                Ty::Ptr(target) => (target.as_ref(), true, true, false),
                other => unreachable!("the checker gives C a `T*?`, not a `{other}?`"),
            },
            Ty::Ptr(target) => (target.as_ref(), true, false, false),
            Ty::Read(target) => (target.as_ref(), false, false, false),
            Ty::Ref(target) => (target.as_ref(), false, false, true),
            other => unreachable!("C gives a value of SG as `T*`, `read T` or `ref T`, not `{other}`"),
        };
        let span = func.name_span;
        if address == 0 {
            if nullable {
                return Ok(Value::Null);
            }
            let diag = Diag::panic(span, format!("C gave `null` to `{name}`, which takes a `{ty}`"));
            return Err(match owned {
                true => diag.hint(format!("a parameter C may give `null` is written `{target}*?`")),
                false => diag,
            });
        }
        let mut handles = self.io.handles.borrow_mut();
        let Some(handle) = handles.get_mut(&address) else {
            return Err(Diag::panic(span, format!("C gave `{name}` an address that holds no `{target}`"))
                .note("C gives back what an `export` function gave it as a `T*`, until it is given back to one that takes the `T*`"));
        };
        if handle.ty != *target {
            return Err(Diag::panic(span, format!("C gave `{name}` a `{}` where it takes a `{target}`", handle.ty)));
        }
        if handle.lent != 0 && (owned || writes || handle.lent < 0) {
            let how = if handle.lent < 0 { "changes" } else { "reads" };
            return Err(Diag::panic(span, format!("C gave `{name}` a `{target}` that a call of `{}` in progress {how}", handle.lent_to))
                .note("while a call of C borrows a value of SG, the rules of borrowing hold for it as in SG"));
        }
        if owned {
            let handle = handles.remove(&address).expect("found above");
            return Ok(Value::Ptr(handle.value));
        }
        if handle.lent == 0 {
            handle.lent_to = name.to_string();
        }
        handle.lent = if writes { -1 } else { handle.lent + 1 };
        lent.push(address);
        Ok(Value::Borrow(handle.value.clone()))
    }

    /// Ends the process from inside a function C called, where nothing can be unwound: what
    /// was printed goes out, then the panic or the error, shown as at the end of a run.
    fn die(&self, stop: Stop) -> ! {
        ENDING.with(|ending| ending.set(true));
        if let Ok(mut out) = self.io.out.try_borrow_mut() {
            let _ = out.flush();
        }
        let code = match stop {
            Stop::Diag(diag) => {
                eprint!("{}", diag.render_in(self.io.sources));
                1
            }
            Stop::Failed(text) => {
                eprintln!("error: {text}");
                1
            }
            Stop::OutputFailed(error) => {
                eprintln!("error: cannot write output: {error}");
                1
            }
            Stop::Exit(code) => i32::from(code),
            Stop::OutputClosed => 0,
            Stop::TestsFailed => 1,
        };
        std::process::exit(code)
    }

    /// Waits for the tasks that nothing holds any more: a function that ended with an error
    /// left them (the checker lets every other exit go only after they are waited for). What
    /// they give is dropped; the error that ended the function is the one that counts.
    fn unwaited(&mut self) -> Res<()> {
        while let Some(at) = self.started.iter().position(|task| Rc::strong_count(task) == 1) {
            let task = self.started.remove(at);
            let started = task.borrow().started;
            self.await_task(&task, started)?;
        }
        Ok(())
    }

    /// Waits for a task, `wait` and `await` alike, and gives what its function returned. A
    /// task of `Task.run` starts here; the others are running already, or are timers.
    fn await_task(&mut self, task: &Rc<RefCell<Task>>, at: Span) -> Res<Value> {
        self.started.retain(|other| !Rc::ptr_eq(other, task));
        loop {
            let state = std::mem::replace(&mut task.borrow_mut().state, TaskState::Running);
            match state {
                TaskState::Done(value) => return Ok(value.expect("a task is waited for once")),
                TaskState::Lazy(name, args) => {
                    task.borrow_mut().waiters.push(self.me);
                    let func = self.funcs[&*name];
                    self.start_fiber(task, func, None, args, false)?;
                }
                TaskState::Running => {
                    task.borrow_mut().waiters.push(self.me);
                    self.io.sched.block(self.me, at)?;
                }
            }
        }
    }

    /// `Task.whenAll(tasks)`: waits for the tasks one after the other, in the order of the
    /// list, which is left empty; a task of `Task.run` runs when its turn comes. Gives an ended
    /// task of what they gave, as a list unless they give nothing.
    fn when_all(&mut self, list: &Rc<RefCell<Vec<Cell>>>, gives: bool, at: Span) -> Res<Value> {
        let items = std::mem::take(&mut *list.borrow_mut());
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            let Value::Task(task) = item.borrow().clone() else { unreachable!("a list of tasks") };
            values.push(cell(self.await_task(&task, at)?));
        }
        let value = if gives { Value::List(Rc::new(RefCell::new(values))) } else { Value::Void };
        Ok(self.ended_task(value, at))
    }

    /// `Task.whenAny(ref tasks)`: the first task of the list that has ended is taken out of it,
    /// and an ended task of what it gave is given. When none has, this fiber waits for all of
    /// them at once; when one of them is a task of `Task.run` that has not run, the first such
    /// runs now, as if it were waited for. An empty list is a panic: nothing could end.
    fn when_any(&mut self, list: &Rc<RefCell<Vec<Cell>>>, at: Span) -> Res<Value> {
        if list.borrow().is_empty() {
            return Err(Diag::panic(at, "`Task.whenAny` is given an empty list, so no task could end").into());
        }
        loop {
            let tasks: Vec<Rc<RefCell<Task>>> = list
                .borrow()
                .iter()
                .map(|item| match &*item.borrow() {
                    Value::Task(task) => task.clone(),
                    _ => unreachable!("a list of tasks"),
                })
                .collect();
            if let Some(first) = tasks.iter().position(|task| matches!(task.borrow().state, TaskState::Done(_))) {
                list.borrow_mut().remove(first);
                let value = self.await_task(&tasks[first], at)?;
                return Ok(self.ended_task(value, at));
            }
            for task in &tasks {
                let mut task = task.borrow_mut();
                if matches!(task.state, TaskState::Running) {
                    task.waiters.push(self.me);
                }
            }
            let lazy = tasks.iter().find(|task| matches!(task.borrow().state, TaskState::Lazy(..)));
            let waited = match lazy {
                Some(task) => {
                    let TaskState::Lazy(name, args) = std::mem::replace(&mut task.borrow_mut().state, TaskState::Running) else { unreachable!("just seen") };
                    task.borrow_mut().waiters.push(self.me);
                    let func = self.funcs[&*name];
                    self.start_fiber(task, func, None, args, false)
                }
                None => self.io.sched.block(self.me, at),
            };
            // Only the first end counts: this fiber no longer waits for the others, and is not
            // made ready again by one that ended at the same time.
            for task in &tasks {
                task.borrow_mut().waiters.retain(|fiber| *fiber != self.me);
            }
            self.io.sched.lock().ready.retain(|fiber| *fiber != self.me);
            waited?;
        }
    }

    /// `x.m(args)` of a method of the program. An async method called without `await` (`here`
    /// false) starts as a task, as an async function does.
    #[allow(clippy::too_many_arguments)]
    fn method_call(&mut self, expr: &'a Expr, recv: &'a Expr, owner: &str, name: &str, this: This, whole: bool, args: &'a [Expr], frame: &mut Frame, here: bool) -> Res<Value> {
        let func = self.methods[owner][name];
        // The receiver is lent or handed over without `read` / `ref` at the call.
        let mark = self.guards.len();
        let mut target = self.place(recv, frame)?;
        // `read FolderRef this`: the method takes the value that holds the struct.
        loop {
            let next = match &*target.borrow() {
                Value::Shared(_) if whole => None,
                value => behind(value),
            };
            let Some(inner) = next else { break };
            target = inner;
        }
        let lent = self.guards.len();
        let this = match this {
            // A receiver handed over leaves its place empty, as an argument does.
            This::Value if recv.moved && matches!(&*target.borrow(), Value::Struct(..)) => target.replace(Value::Null),
            This::Value => fresh(&target.borrow()),
            This::Read | This::Ref => Value::Borrow(target),
        };
        let values = self.values(args, frame)?;
        self.change_reserved(mark..lent)?;
        match func.is_async && !here {
            true => self.start_async(func, Some(this), values, expr.span),
            false => self.call(func, Some(this), values, expr.span),
        }
    }

    /// `f(x)` of an async function `f`, without `await`: the task starts now and runs until it
    /// first waits, then this fiber goes on, as in C#.
    fn start_async(&mut self, func: &'a Func, this: Option<Value>, args: Vec<Value>, at: Span) -> Res<Value> {
        let task = Rc::new(RefCell::new(Task { state: TaskState::Running, started: at, waiters: Vec::new() }));
        self.started.push(task.clone());
        self.start_fiber(&task, func, this, args, true)?;
        Ok(Value::Task(task))
    }

    /// `Task.delay(ms)`: a task that ends when the clock of the run has gone that far. With
    /// a token (`told`), it ends early when the token is cancelled, at once when it already
    /// is, and says which way it ended.
    fn delay(&mut self, ms: i128, told: Option<Option<Rc<RefCell<Cancel>>>>, at: Span) -> Res<Value> {
        if ms < 0 {
            let token = if told.is_some() { ", token" } else { "" };
            return Err(Diag::panic(at, format!("`Task.delay({ms}{token})`: a delay is not negative")).into());
        }
        let task = Rc::new(RefCell::new(Task { state: TaskState::Running, started: at, waiters: Vec::new() }));
        self.started.push(task.clone());
        let source = told.clone().flatten();
        if source.as_ref().is_some_and(|source| source.borrow().cancelled) {
            task.borrow_mut().state = TaskState::Done(Some(failed(cancelled())));
            return Ok(Value::Task(task));
        }
        let mut state = self.io.sched.lock();
        let key = (state.now + ms as u64, state.made);
        state.made += 1;
        let token = told.map(|_| source.as_ref().map_or_else(std::rc::Weak::new, Rc::downgrade));
        state.timers.insert(key, (Instant::now() + Duration::from_millis(ms as u64), Timer::Delay { task: Rc::downgrade(&task), token }));
        drop(state);
        if let Some(source) = source {
            source.borrow_mut().delays.push(key);
        }
        Ok(Value::Task(task))
    }

    /// `CancellationSource.after(ms)`: a source that the clock of the run cancels.
    fn source_after(&mut self, ms: i128, at: Span) -> Res<Value> {
        if ms < 0 {
            return Err(Diag::panic(at, format!("`CancellationSource.after({ms})`: a time is not negative")).into());
        }
        let source = Rc::new(RefCell::new(Cancel::default()));
        let mut state = self.io.sched.lock();
        let key = (state.now + ms as u64, state.made);
        state.made += 1;
        state.timers.insert(key, (Instant::now() + Duration::from_millis(ms as u64), Timer::Cancel(Rc::downgrade(&source))));
        Ok(Value::Source(source))
    }

    /// Whether the token given to an async method, if one was, is cancelled.
    fn cancelled(&mut self, token: Option<&'a Expr>, frame: &mut Frame) -> Res<bool> {
        let Some(token) = token else { return Ok(false) };
        Ok(self.token(token, frame)?.is_some_and(|source| source.borrow().cancelled))
    }

    /// The source a token argument looks at; `None` for `CancellationToken.None`.
    fn token(&mut self, arg: &'a Expr, frame: &mut Frame) -> Res<Option<Rc<RefCell<Cancel>>>> {
        match owned(self.expr(arg, frame)?) {
            Value::Token(source) => Ok(source),
            _ => unreachable!("the checker makes it a token"),
        }
    }

    /// The tasks of `Task.run` that have not run start now, as they would on threads of their
    /// own in compiled code, and each runs until it waits.
    /// An atomic operation, or a function of `Atomic`. One task runs at a time, so each is
    /// one step whatever its order, and every order acts as `SequentiallyConsistent`. The turn
    /// changes hands at `Atomic.spinLoop()` and every `ATOMICS_PER_TURN` operations, so that a
    /// task that spins waiting for a value lets the one that writes it run.
    fn atomic(&mut self, op: AtomicOp, recv: Option<&'a Expr>, args: &'a [Expr], span: Span, frame: &mut Frame) -> Res<Value> {
        match op {
            AtomicOp::Fence | AtomicOp::CompilerFence => return Ok(Value::Void),
            AtomicOp::SpinLoop => {
                self.pass_turn(span)?;
                return Ok(Value::Void);
            }
            _ => {}
        }
        let recv = recv.expect("an atomic operation has a value it works on");
        // What lies in raw memory is written there; anything else is the cell of the value,
        // through the borrows, pointers and owners on the way.
        let spot = match recv.in_raw_memory() {
            true => Err(self.raw_place(recv, "written", frame)?),
            false => {
                let mut at = self.place(recv, frame)?;
                loop {
                    let next = match &*at.borrow() {
                        Value::Borrow(inner) | Value::Ptr(inner) => inner.clone(),
                        Value::Shared(held) => held.value.clone(),
                        _ => break,
                    };
                    at = next;
                }
                Ok(at)
            }
        };
        let values = self.values(args, frame)?.into_iter().map(strip).collect::<Vec<Value>>();
        // A hand-over costs a wake of a thread, so the turn goes every so many operations: a
        // loop that spins on a value does at least one in each round.
        self.atomics = self.atomics.wrapping_add(1);
        if self.atomics.is_multiple_of(ATOMICS_PER_TURN) {
            self.pass_turn(span)?;
        }
        let old = match &spot {
            Ok(at) => strip(at.borrow().clone()),
            Err(raw) => raw.read(self.plain, span)?,
        };
        let (new, gives) = match op {
            AtomicOp::Load => (None, old.clone()),
            AtomicOp::Store => (Some(values[0].clone()), Value::Void),
            AtomicOp::Exchange => (Some(values[0].clone()), old.clone()),
            AtomicOp::CompareExchange(_) => match same_bits(&old, &values[0]) {
                true => (Some(values[1].clone()), ok(old.clone())),
                false => (None, failed(old.clone())),
            },
            AtomicOp::FetchAdd(wrapping) | AtomicOp::FetchSub(wrapping) => {
                let add = matches!(op, AtomicOp::FetchAdd(_));
                let new = match (&old, &values[0]) {
                    (Value::Int(ty, a), Value::Int(_, b)) => {
                        let sum = if add { a + b } else { a - b };
                        if !wrapping && !ty.fits(sum) {
                            let (sign, other) = if add { ("+", "Add") } else { ("-", "Sub") };
                            return Err(Diag::panic(span, format!("overflow: {a} {sign} {b} does not fit in {}", ty.name()))
                                .hint(format!("`wrappingFetch{other}` wraps around"))
                                .into());
                        }
                        Value::Int(*ty, ty.wrap(sum))
                    }
                    _ => binary(if add { BinOp::Add } else { BinOp::Sub }, old.clone(), values[0].clone(), span)?,
                };
                (Some(new), old.clone())
            }
            AtomicOp::FetchAnd | AtomicOp::FetchOr | AtomicOp::FetchXor => {
                let new = match (&old, &values[0]) {
                    (Value::Bool(a), Value::Bool(b)) => Value::Bool(match op {
                        AtomicOp::FetchAnd => a & b,
                        AtomicOp::FetchOr => a | b,
                        _ => a ^ b,
                    }),
                    (Value::Int(ty, a), Value::Int(_, b)) => Value::Int(*ty, ty.wrap(match op {
                        AtomicOp::FetchAnd => a & b,
                        AtomicOp::FetchOr => a | b,
                        _ => a ^ b,
                    })),
                    _ => unreachable!("the checker takes the bits of integers and of `bool`"),
                };
                (Some(new), old.clone())
            }
            AtomicOp::Fence | AtomicOp::CompilerFence | AtomicOp::SpinLoop => unreachable!("handled above"),
        };
        if let Some(new) = new {
            match &spot {
                Ok(at) => *at.borrow_mut() = new,
                Err(raw) => {
                    raw.write(&new, self.plain);
                    windows_in(self.plain, span)?;
                }
            }
        }
        Ok(gives)
    }

    /// Where an atomic operation lets the other tasks run: the turn goes to the next fiber that
    /// is ready, the tasks of `Task.run` not started yet start, and this fiber is ready again
    /// after them. A fiber alone, or one inside a `lock` block, keeps the turn.
    fn pass_turn(&mut self, at: Span) -> Res<()> {
        if self.locking > 0 {
            return Ok(());
        }
        {
            let state = self.io.sched.lock();
            let lazy = state.lazy.iter().any(|task| task.upgrade().is_some_and(|task| matches!(task.borrow().state, TaskState::Lazy(..))));
            if state.ready.is_empty() && state.outside.is_empty() && !lazy {
                return Ok(());
            }
        }
        self.start_lazy()?;
        let mut state = self.io.sched.lock();
        // Sockets and threads that are done by now make their fibers ready, without a wait.
        if !state.outside.is_empty() {
            Sched::wait_outside(&mut state, Some(Duration::ZERO));
        }
        let Some(next) = state.ready.pop_front() else { return Ok(()) };
        state.ready.push_back(self.me);
        let _ = at;
        self.io.sched.hand(state, self.me, next)
    }

    fn start_lazy(&mut self) -> Res<()> {
        loop {
            let lazy = std::mem::take(&mut self.io.sched.lock().lazy);
            if lazy.is_empty() {
                return Ok(());
            }
            for task in lazy.iter().filter_map(std::rc::Weak::upgrade) {
                if !matches!(task.borrow().state, TaskState::Lazy(..)) {
                    continue;
                }
                let TaskState::Lazy(name, args) = std::mem::replace(&mut task.borrow_mut().state, TaskState::Running) else { unreachable!("just seen") };
                let func = self.funcs[&*name];
                self.start_fiber(&task, func, None, args, true)?;
            }
        }
    }

    /// Before a call of C, which may wait as long as it likes: in compiled code it holds up
    /// only its own thread, so here the turn goes to the other fibers until it returns, as
    /// for a wait outside the run. A fiber alone in the run keeps the turn, and pays nothing.
    /// Gives what `c_back` takes the turn back with.
    fn c_leave(&mut self, at: Span) -> Res<Option<Leave>> {
        let alone = |state: &State, me: usize| me == 0 && state.fibers.is_empty() && state.lazy.iter().all(|task| task.upgrade().is_none_or(|task| !matches!(task.borrow().state, TaskState::Lazy(..))));
        if alone(&self.io.sched.lock(), self.me) {
            return Ok(None);
        }
        self.start_lazy()?;
        let (id, done) = self.work(at)?;
        let mut state = self.io.sched.lock();
        state.outside.insert(self.me, Outside::Work(id));
        state.thread = Some(Thread::take());
        match state.ready.pop_front() {
            Some(next) => Sched::give(&mut state, next),
            None => {
                // Nobody is ready: a thread of its own waits for the first that is, as the
                // fiber that gives the turn away does when it waits outside.
                state.running = NOBODY;
                drop(state);
                let sched = Baton(&self.io.sched as *const Sched);
                let driven = std::thread::Builder::new().spawn_scoped(self.scope, move || {
                    let sched = sched;
                    // SAFETY: the scheduler lives as long as the scope of the run's threads.
                    let sched = unsafe { &*sched.0 };
                    let mut state = sched.lock();
                    if state.running == NOBODY {
                        let next = Sched::next(&mut state, at);
                        Sched::give(&mut state, next);
                    }
                });
                if let Err(error) = driven {
                    return Err(Diag::panic(at, format!("the system refused a thread for the scheduler: {error}")).into());
                }
            }
        }
        Ok(Some(Leave { id, done }))
    }

    /// After a call of C that gave the turn away (`c_leave`): the scheduler is told the call
    /// is over, and the fiber waits for its turn.
    fn c_back(&mut self, left: Option<Leave>) -> Res<()> {
        let Some(Leave { id, done }) = left else { return Ok(()) };
        done.tell(id);
        let state = self.io.sched.lock();
        let mut state = self.io.sched.wait_turn(state, self.me);
        state.outside.remove(&self.me);
        Sched::resume(&mut state, self.me)
    }

    /// Fiber `me` waits outside the run for `what`, until it is ready or the token, if one is
    /// given, is cancelled. Gives whether the wait was cut short by a cancel.
    fn wait_for(&mut self, what: Outside, source: Option<&Rc<RefCell<Cancel>>>, at: Span) -> Res<bool> {
        if let Some(source) = source {
            if source.borrow().cancelled {
                return Ok(true);
            }
        }
        // What the network waits for may be another task of this run.
        self.start_lazy()?;
        if let Some(source) = source {
            if source.borrow().cancelled {
                return Ok(true);
            }
            source.borrow_mut().waits.push(self.me);
        }
        self.io.sched.lock().outside.insert(self.me, what);
        let waited = self.io.sched.block(self.me, at);
        if let Some(source) = source {
            source.borrow_mut().waits.retain(|fiber| *fiber != self.me);
        }
        let cut = self.io.sched.lock().cut.remove(&self.me);
        waited?;
        Ok(cut)
    }

    /// An async read of an input that can make the program wait: until what the read needs
    /// is in the block of the input, a thread reads into it and this fiber waits. A cancel
    /// ends the wait, and gives `Some(Error(Cancelled))`; what the thread reads stays in the
    /// block. `None` when the read can be done now.
    fn input_wait(&mut self, call: StreamCall, args: &'a [Expr], token: Option<&'a Expr>, at: Span, frame: &mut Frame) -> Res<Option<Value>> {
        let need = match call {
            StreamCall::ReadLine => crate::input::Need::Line,
            StreamCall::ReadText => crate::input::Need::End,
            StreamCall::ReadBytes => crate::input::Need::Bytes(cells_of(&owned(self.expr(&args[0], frame)?)).len()),
            StreamCall::ReadSome => crate::input::Need::Some,
            _ => unreachable!("only reads wait for the input"),
        };
        let source = match token {
            Some(token) => self.token(token, frame)?,
            None => None,
        };
        loop {
            if source.as_ref().is_some_and(|source| source.borrow().cancelled) {
                return Ok(Some(failed(io_cancelled())));
            }
            if self.io.input.borrow_mut().ready(need) {
                return Ok(None);
            }
            if self.io.world.borrow().input_is_terminal {
                self.flush_out()?;
            }
            let (id, done) = self.work(at)?;
            let Some((id, work)) = self.io.input.borrow_mut().fill(id) else { continue };
            if let Some(work) = work {
                std::thread::spawn(move || {
                    work();
                    done.tell(id);
                });
            }
            if self.wait_for(Outside::Work(id), source.as_ref(), at)? {
                return Ok(Some(failed(io_cancelled())));
            }
        }
    }

    /// A number for a piece of work on a thread of its own, and what the thread tells the
    /// scheduler through when it has ended.
    fn work(&mut self, at: Span) -> Res<(u64, std::sync::Arc<crate::net::Done>)> {
        let mut state = self.io.sched.lock();
        if state.wake.is_none() {
            let wake = crate::net::Wake::new().map_err(|error| Diag::panic(at, format!("the system refused a socket for the scheduler: {error}")))?;
            state.wake = Some(wake);
        }
        state.works += 1;
        let done = state.wake.as_ref().expect("made above").done.clone();
        Ok((state.works, done))
    }

    /// The functions of the network. A socket that is not ready gives the turn away until it
    /// is; a token is looked at before the work, and a wait it cuts short leaves the socket as
    /// it was, so nothing that came is lost.
    fn net(&mut self, call: NetCall, recv: &'a Expr, args: &'a [Expr], token: Option<&'a Expr>, at: Span, frame: &mut Frame) -> Res<Value> {
        let io_failed = |error: std::io::Error| failed(io_cause(Some(&error), error.raw_os_error()));
        match call {
            NetCall::Bind => {
                let Value::Str(address) = strip(self.expr(&args[0], frame)?) else { unreachable!("the checker makes the address text") };
                Ok(match crate::net::bind_address(&address) {
                    None => failed(io_host_not_found()),
                    Some(address) => match crate::net::bind(address) {
                        Ok(listener) => ok(Value::Listener(Rc::new(listener))),
                        Err(error) => io_failed(error),
                    },
                })
            }
            NetCall::Connect => {
                let Value::Str(address) = strip(self.expr(&args[0], frame)?) else { unreachable!("the checker makes the address text") };
                let address = address.to_string();
                let source = match token {
                    Some(token) => self.token(token, frame)?,
                    None => None,
                };
                if source.as_ref().is_some_and(|source| source.borrow().cancelled) {
                    return Ok(failed(io_cancelled()));
                }
                let (id, done) = self.work(at)?;
                let slot = std::sync::Arc::new(Mutex::new(None));
                let kept = slot.clone();
                std::thread::spawn(move || {
                    let connected = crate::net::connect(&address);
                    *kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(connected);
                    done.tell(id);
                });
                if self.wait_for(Outside::Work(id), source.as_ref(), at)? {
                    return Ok(failed(io_cancelled()));
                }
                let connected = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take().expect("the work has ended");
                Ok(match connected {
                    Ok(stream) => ok(Value::Socket(Rc::new(stream))),
                    Err(None) => failed(io_host_not_found()),
                    Err(Some(error)) => io_failed(error),
                })
            }
            NetCall::Reader => {
                let Value::Socket(stream) = owned(self.expr(recv, frame)?) else { unreachable!("the checker calls `reader` on a stream") };
                let limit = match args.first() {
                    Some(arg) => match strip(self.expr(arg, frame)?) {
                        Value::Int(_, limit) if limit < 1 => {
                            return Err(Diag::panic(arg.span, format!("the longest line of a reader is at least 1 byte, and this is {limit}")).into())
                        }
                        Value::Int(_, limit) => usize::try_from(limit).unwrap_or(usize::MAX),
                        _ => unreachable!("the checker makes the limit a number"),
                    },
                    None => READER_LIMIT,
                };
                let net = NetReader { stream, block: Vec::new(), at: 0, ended: false, limit, too_long: false };
                Ok(Value::Reader(Rc::new(RefCell::new(Reader { file: None, net: Some(net), done: 0 }))))
            }
            NetCall::Port => {
                let Value::Listener(listener) = owned(self.expr(recv, frame)?) else { unreachable!("the checker calls `port` on a listener") };
                Ok(Value::Int(IntTy::Int, listener.local_addr().map_or(0, |address| address.port()) as i128))
            }
            NetCall::Accept => {
                let Value::Listener(listener) = owned(self.expr(recv, frame)?) else { unreachable!("the checker calls `accept` on a listener") };
                let source = match token {
                    Some(token) => self.token(token, frame)?,
                    None => None,
                };
                let fd = crate::net::fd_of(&*listener);
                if source.as_ref().is_some_and(|source| source.borrow().cancelled) {
                    return Ok(failed(io_cancelled()));
                }
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            return Ok(match stream.set_nonblocking(true) {
                                Ok(()) => ok(Value::Socket(Rc::new(stream))),
                                Err(error) => io_failed(error),
                            });
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if self.wait_for(Outside::Socket(fd, false), source.as_ref(), at)? {
                                return Ok(failed(io_cancelled()));
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(error) => return Ok(io_failed(error)),
                    }
                }
            }
            NetCall::Receive => {
                let Value::Socket(stream) = owned(self.expr(recv, frame)?) else { unreachable!("the checker calls `receive` on a stream") };
                let cells = cells_of(&owned(self.expr(&args[0], frame)?));
                let source = match token {
                    Some(token) => self.token(token, frame)?,
                    None => None,
                };
                let fd = crate::net::fd_of(&*stream);
                let mut bytes = vec![0u8; cells.len()];
                if source.as_ref().is_some_and(|source| source.borrow().cancelled) {
                    return Ok(failed(io_cancelled()));
                }
                loop {
                    match (&*stream).read(&mut bytes) {
                        Ok(got) => {
                            for (slot, byte) in cells.iter().zip(&bytes[..got]) {
                                *slot.borrow_mut() = Value::Int(IntTy::Byte, *byte as i128);
                            }
                            return Ok(ok(Value::Int(IntTy::Nint, got as i128)));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if self.wait_for(Outside::Socket(fd, false), source.as_ref(), at)? {
                                return Ok(failed(io_cancelled()));
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(error) => return Ok(io_failed(error)),
                    }
                }
            }
            NetCall::Send => {
                let Value::Socket(stream) = owned(self.expr(recv, frame)?) else { unreachable!("the checker calls `send` on a stream") };
                let bytes: Vec<u8> = cells_of(&owned(self.expr(&args[0], frame)?)).iter().map(|item| match &*item.borrow() {
                    Value::Int(_, byte) => *byte as u8,
                    _ => unreachable!("the checker makes the items bytes"),
                }).collect();
                let source = match token {
                    Some(token) => self.token(token, frame)?,
                    None => None,
                };
                let fd = crate::net::fd_of(&*stream);
                if source.as_ref().is_some_and(|source| source.borrow().cancelled) {
                    return Ok(failed(io_cancelled()));
                }
                // A cancel cuts a wait short; what went out before it has gone out.
                let mut sent = 0;
                while sent < bytes.len() {
                    match (&*stream).write(&bytes[sent..]) {
                        Ok(n) => sent += n,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if self.wait_for(Outside::Socket(fd, true), source.as_ref(), at)? {
                                return Ok(failed(io_cancelled()));
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(error) => return Ok(io_failed(error)),
                    }
                }
                Ok(ok(Value::Void))
            }
        }
    }

    /// Runs `func(args)` for `task` on a fiber of its own, which gets the turn now. With
    /// `caller_ready` this fiber is next once the new one waits; otherwise this one waits for
    /// the task and goes on when it ends.
    fn start_fiber(&mut self, task: &Rc<RefCell<Task>>, func: &'a Func, this: Option<Value>, args: Vec<Value>, caller_ready: bool) -> Res<()> {
        let started = task.borrow().started;
        let id = {
            let mut state = self.io.sched.lock();
            let id = state.next_fiber;
            state.next_fiber += 1;
            state.fibers.insert(id);
            state.wakers.insert(id, std::sync::Arc::new(Condvar::new()));
            id
        };
        let mut inner = Interp {
            funcs: self.funcs.clone(),
            methods: self.methods.clone(),
            structs: self.structs.clone(),
            enums: self.enums.clone(),
            src: self.src,
            io: self.io,
            scope: self.scope,
            me: id,
            depth: 0,
            in_test: self.in_test,
            guards: Vec::new(),
            aliases: self.aliases.clone(),
            started: Vec::new(),
            plain: self.plain,
            foreign: self.foreign,
            std_from: self.std_from,
            errno: [0; 2],
            locking: 0,
            atomics: 0,
            consts: self.consts,
        };
        let weak = Rc::downgrade(task);
        let name: Rc<str> = func.name.as_str().into();
        let io = self.io;
        let work = Baton(move || {
            let sched = &io.sched;
            let state = sched.lock();
            let mut state = sched.wait_turn(state, id);
            let begun = Sched::resume(&mut state, id);
            drop(state);
            let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let result = begun.and_then(|()| inner.call(func, this, args, started));
                drop(inner);
                // A panic that already shows a second place keeps it: the line of a task
                // started inside this one, or the borrow it met.
                result.map_err(|unwind| match unwind {
                    Unwind::Stop(Stop::Diag(diag)) if diag.kind == crate::diag::Kind::Panic && diag.related.is_none() => {
                        Unwind::Stop(Stop::Diag(diag.related(started, format!("in the task `{}` started here", short_name(&name)))))
                    }
                    other => other,
                })
            }));
            match ran {
                Ok(result) => sched.finish(id, weak.upgrade(), result, started),
                Err(crash) => {
                    let mut state = sched.lock();
                    state.fibers.remove(&id);
                    state.wakers.remove(&id);
                    state.crashed = Some(crash);
                    state.thread = Some(Thread::take());
                    Sched::give(&mut state, 0);
                }
            }
        });
        let spawned = std::thread::Builder::new().stack_size(crate::STACK_BYTES).spawn_scoped(self.scope, move || {
            let work = work;
            (work.0)()
        });
        if let Err(error) = spawned {
            let mut state = self.io.sched.lock();
            state.fibers.remove(&id);
            state.wakers.remove(&id);
            drop(state);
            return Err(Diag::panic(started, format!("the system refused a thread for this task: {error}")).into());
        }
        let mut state = self.io.sched.lock();
        if caller_ready {
            state.ready.push_front(self.me);
        }
        self.io.sched.hand(state, self.me, id)
    }

    /// After a stop on fiber 0: every other fiber still waiting gets the turn once, one after
    /// the other, and unwinds.
    fn end_fibers(&mut self) {
        let sched = &self.io.sched;
        let mut state = sched.lock();
        state.ending = true;
        state.ready.clear();
        state.timers.clear();
        state.outside.clear();
        while let Some(&fiber) = state.fibers.iter().next() {
            state.thread = Some(Thread::take());
            Sched::give(&mut state, fiber);
            state = sched.wait_turn(state, 0);
            if let Some(thread) = state.thread.take() {
                thread.put();
            }
        }
        state.ending = false;
        state.stop = None;
    }

    fn call(&mut self, func: &'a Func, this: Option<Value>, args: Vec<Value>, call_span: Span) -> Res<Value> {
        if self.depth == MAX_DEPTH {
            return Err(Diag::panic(call_span, format!("stack overflow: more than {MAX_DEPTH} nested calls"))
                .note(format!("last called: `{}`", func.name))
                .into());
        }
        let mut frame = Frame { scopes: vec![Scope::default()] };
        if let Some(this) = this {
            frame.declare("this", this);
        }
        for (param, value) in func.params.iter().zip(args) {
            frame.declare(&param.name, value);
        }
        self.depth += 1;
        let result = self.block(&func.body, &mut frame);
        // What the function still holds ends in the reverse order of its names. A stop of the
        // run waits for nothing: the program ends. Otherwise a task left behind (an exit with
        // an error) is awaited where its name ends, before what was declared before it.
        let stops = matches!(result, Err(Unwind::Stop(_) | Unwind::Abandon));
        let mut ended = Ok(());
        while let Some(mut scope) = frame.scopes.pop() {
            while let Some(slot) = scope.pop() {
                drop(slot);
                if ended.is_ok() && !stops && !self.started.is_empty() {
                    ended = self.unwaited();
                }
            }
        }
        self.depth -= 1;
        ended?;
        if !self.started.is_empty() && !stops {
            self.unwaited()?;
        }
        if !stops {
            self.disposals()?;
        }
        match result {
            Ok(Flow::Return(value)) | Err(Unwind::Return(value)) => Ok(value),
            Ok(_) => Ok(Value::Void),
            // A panic inside the language's own module is shown at the call of the program that
            // led there, as `#[track_caller]` does in Rust: the line in the module says nothing
            // to the one who called it.
            Err(Unwind::Stop(Stop::Diag(mut diag)))
                if diag.kind == crate::diag::Kind::Panic
                    && diag.span.start >= self.std_from
                    && call_span.start < self.std_from
                    && !(func.this.is_some() && crate::emit_c::RUNTIME_METHODS.contains(&func.name.as_str())) =>
            {
                // A hint there would name code the caller cannot see.
                diag.span = call_span;
                diag.hint = None;
                Err(Unwind::Stop(Stop::Diag(diag)))
            }
            Err(other) => Err(other),
        }
    }

    fn block(&mut self, block: &'a Block, frame: &mut Frame) -> Res<Flow> {
        frame.scopes.push(Scope::default());
        let mut flow = Flow::Next;
        for stmt in block {
            flow = self.stmt(stmt, frame)?;
            if !matches!(flow, Flow::Next) {
                break;
            }
        }
        frame.scopes.pop();
        self.disposals()?;
        Ok(flow)
    }

    /// A statement. What it lent of `shared checked` values ends with it.
    fn stmt(&mut self, stmt: &'a Stmt, frame: &mut Frame) -> Res<Flow> {
        // What the test of an `if` or a `while` made and let go ends before the block runs.
        self.disposals()?;
        let mark = self.guards.len();
        let flow = self.stmt_in(stmt, frame);
        self.guards.truncate(mark);
        if flow.is_ok() {
            self.disposals()?;
        }
        flow
    }

    /// Runs `dispose` on the values of `Disposable` types that their last owner let go, in
    /// the order it did. What one of them holds ends after its `dispose`, before the next
    /// value: the order of compiled code, which drops a value field by field.
    fn disposals(&mut self) -> Res<()> {
        loop {
            let batch = PENDING.with(|pending| std::mem::take(&mut *pending.borrow_mut()));
            if batch.is_empty() {
                return Ok(());
            }
            for (name, cells) in batch {
                let func = self.methods[&*name]["dispose"];
                let value = cell(Value::Struct(name, Fields::plain(cells)));
                self.call(func, Some(Value::Borrow(value.clone())), Vec::new(), func.name_span)?;
                drop(value);
                self.disposals()?;
            }
        }
    }

    fn stmt_in(&mut self, stmt: &'a Stmt, frame: &mut Frame) -> Res<Flow> {
        match &stmt.kind {
            StmtKind::Let { name, init, .. } => {
                let value = self.expr(init, frame)?;
                frame.declare(name, value);
            }
            // A write into raw memory: the right side first, as for any place.
            StmtKind::Assign { target, op, value, .. } if target.in_raw_memory() => {
                let mut new = strip(self.expr(value, frame)?);
                let verb = if op.is_some() { "read" } else { "written" };
                let spot = self.raw_place(target, verb, frame)?;
                if let Some(op) = op {
                    let old = spot.read(self.plain, target.span)?;
                    new = binary(*op, old, new, stmt.span)?;
                }
                if let Some((_, width)) = spot.bits {
                    fits_bits(&new, width, stmt.span)?;
                }
                spot.write(&new, self.plain);
                windows_in(self.plain, stmt.span)?;
            }
            // `u.word = 3`: the bytes of the field are written over those of the union.
            StmtKind::Assign { target: place @ Expr { kind: ExprKind::Field { recv, index, .. }, .. }, value, .. } if self.union_of(recv).is_some() => {
                let name = self.union_of(recv).expect("matched above");
                let new = self.expr(value, frame)?;
                let ty = self.plain[&name].fields[*index].clone();
                let mut holder = self.place(recv, frame)?;
                loop {
                    let found = holder.borrow().clone();
                    match found {
                        Value::Ptr(inner) | Value::Borrow(inner) => holder = inner,
                        Value::Shared(held) => holder = held.value.clone(),
                        Value::Struct(_, fields) => {
                            let mut bytes = union_bytes(&fields);
                            let given = bytes_of(&new, &ty, self.plain);
                            bytes[..given.len()].copy_from_slice(&given);
                            *holder.borrow_mut() = union_value(&name, &bytes);
                            break;
                        }
                        _ => unreachable!("the checker found a union at {}", place.span.start),
                    }
                }
            }
            StmtKind::Assign { target, op, value, through } => {
                let mut new = self.expr(value, frame)?;
                let width = match &target.kind {
                    ExprKind::Field { recv, index, .. } => struct_name(&recv.ty).and_then(|name| self.plain.get(name)).and_then(|plain| plain.bits[*index]),
                    _ => None,
                };
                let mut target = self.place(target, frame)?;
                if *through {
                    target = behind_borrows(target);
                }
                if let Some(op) = op {
                    let old = strip(target.borrow().clone());
                    new = binary(*op, old, new, stmt.span)?;
                }
                if let Some(width) = width {
                    fits_bits(&new, width, stmt.span)?;
                }
                *target.borrow_mut() = new;
            }
            StmtKind::If { cond, then, els, after } => {
                // The scope holds the names of an `is` pattern in the condition.
                frame.scopes.push(Scope::default());
                let flow = if self.condition(cond, frame)? {
                    self.block(then, frame)?
                } else if let Some(els) = els {
                    self.block(els, frame)?
                } else {
                    Flow::Next
                };
                let names = frame.scopes.pop().expect("the scope of the condition");
                // `if r is not Ok(value) { return }`: the code after it sees `value`.
                keep_names(after, names, frame);
                return Ok(flow);
            }
            StmtKind::While { cond, body, after } => {
                let depth = frame.scopes.len();
                let mark = self.guards.len();
                loop {
                    // What the condition lent for the names it gave ends with the round.
                    self.guards.truncate(mark);
                    frame.scopes.push(Scope::default());
                    let flow = match self.condition(cond, frame)? {
                        true => loop_flow(self.block(body, frame))?,
                        false => {
                            // `while r is not Ok(value) { ... }`: the code after it sees `value`.
                            let names = frame.scopes.pop().expect("the scope of the condition");
                            keep_names(after, names, frame);
                            Flow::Break
                        }
                    };
                    frame.scopes.truncate(depth);
                    match flow {
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Next | Flow::Continue => {}
                    }
                }
            }
            StmtKind::For { var, from, to, body, .. } => {
                let (Value::Int(ty, start), Value::Int(_, end)) = (self.expr(from, frame)?, self.expr(to, frame)?) else {
                    unreachable!("the checker makes range bounds integers")
                };
                let depth = frame.scopes.len();
                for i in start..end {
                    frame.scopes.push(Scope::default());
                    frame.declare(var, Value::Int(ty, i));
                    let flow = loop_flow(self.block(body, frame))?;
                    frame.scopes.truncate(depth);
                    match flow {
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Next | Flow::Continue => {}
                    }
                }
            }
            StmtKind::ForEach { var, iter, body, by, .. } => {
                let collection = owned(self.expr(iter, frame)?);
                // A loop handed the collection owns its items one by one: each ends with its
                // turn, as in compiled code, and not with the collection.
                let owns = by.is_none() && (iter.moved || !iter.is_place());
                // A map or a set is gone over as it was when the loop started.
                let entry = |(key, value): &(Value, Cell)| {
                    let value = if owns { cell(value.replace(Value::Null)) } else { value.clone() };
                    cell(Value::Struct("entry".into(), Fields::plain(vec![cell(key.clone()), value])))
                };
                let snapshot: Option<Vec<Cell>> = match &collection {
                    Value::Map(entries) => Some(entries.borrow().values().map(entry).collect()),
                    Value::Set(items) => Some(items.borrow().values().cloned().map(cell).collect()),
                    _ => None,
                };
                let depth = frame.scopes.len();
                let mut next = 0;
                loop {
                    let item = match (&collection, &snapshot) {
                        (_, Some(items)) => items.get(next).cloned(),
                        (Value::Iter(cursor), _) => cursor.borrow_mut().next().map(cell),
                        (Value::Lines(lines), _) => self.next_line(lines, iter.span)?.map(cell),
                        (Value::List(items), _) => items.borrow().get(next).cloned(),
                        (Value::Array(items) | Value::Slice(items), _) => items.get(next).cloned(),
                        _ => unreachable!("the checker only lets `for` go over a collection"),
                    };
                    let Some(item) = item else { break };
                    next += 1;
                    let bound = match (by, owns) {
                        (Some(_), _) => Value::Borrow(item),
                        (None, true) => item.replace(Value::Null),
                        (None, false) => item.borrow().clone(),
                    };
                    frame.scopes.push(Scope::default());
                    frame.declare(var, bound);
                    let flow = loop_flow(self.block(body, frame))?;
                    frame.scopes.truncate(depth);
                    match flow {
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Next | Flow::Continue => {}
                    }
                }
            }
            // `read folder as f { ... }`: what the `shared` value holds, lent for the block.
            StmtKind::As { value, name, body, .. } => {
                let holder = self.expr(value, frame)?;
                let Some(held) = held_of(&holder) else { unreachable!("the checker opens only a `shared` value") };
                frame.scopes.push(Scope::default());
                frame.declare(name, Value::Borrow(held.value.clone()));
                let flow = self.block(body, frame);
                frame.scopes.pop();
                return flow;
            }
            // `lock a as x, b as y { ... }`: what each `shared locked` value holds, lent to
            // change for the block. Tasks change hands only where one waits, and nothing in
            // the block waits, so no other task holds the lock here.
            StmtKind::Unsafe(body) => return self.block(body, frame),
            // A const of a block: its value was found when the program was compiled.
            StmtKind::Const { .. } => {}
            StmtKind::Lock { items, body } => {
                let mut taken: Vec<Rc<Held>> = Vec::new();
                frame.scopes.push(Scope::default());
                for item in items {
                    let holder = self.expr(&item.value, frame)?;
                    let Some(held) = held_of(&holder) else { unreachable!("the checker locks only a `shared locked` value") };
                    if let Some(first) = taken.iter().position(|other| Rc::ptr_eq(other, &held)) {
                        frame.scopes.pop();
                        return Err(Diag::panic(item.value.span, format!("`{}` and `{}` are one value, which a `lock` takes once", items[first].name, item.name)).into());
                    }
                    frame.declare(&item.name, Value::Borrow(held.value.clone()));
                    taken.push(held);
                }
                self.locking += 1;
                let flow = self.block(body, frame);
                self.locking -= 1;
                frame.scopes.pop();
                return flow;
            }
            StmtKind::Return(value) => {
                let value = match value {
                    Some(expr) => self.expr(expr, frame)?,
                    None => Value::Void,
                };
                return Ok(Flow::Return(value));
            }
            StmtKind::Break => return Ok(Flow::Break),
            StmtKind::Continue => return Ok(Flow::Continue),
            StmtKind::Expr(expr) | StmtKind::Discard(expr) => {
                self.expr(expr, frame)?;
            }
        }
        Ok(Flow::Next)
    }

    fn condition(&mut self, cond: &'a Expr, frame: &mut Frame) -> Res<bool> {
        match strip(self.expr(cond, frame)?) {
            Value::Bool(value) => Ok(value),
            _ => unreachable!("the checker makes conditions `bool`"),
        }
    }

    /// The cell an expression names. Field access looks through pointers and borrows.
    /// An expression that is not a place gets a temporary cell.
    fn place(&mut self, expr: &'a Expr, frame: &mut Frame) -> Res<Cell> {
        // What lies in raw memory is not lent: a place there is read as a copy.
        if expr.in_raw_memory() {
            return Ok(cell(self.raw_place(expr, "read", frame)?.read(self.plain, expr.span)?));
        }
        match &expr.kind {
            ExprKind::Var(name) => Ok(frame.slot(name)),
            // Read where it lies: no one changes it.
            ExprKind::Const(name) => self.const_cell(name, expr.span),
            ExprKind::Open { inner, how } => {
                let slot = self.place(inner, frame)?;
                let value = slot.borrow().clone();
                self.open(&value, *how, inner)?;
                Ok(slot)
            }
            ExprKind::Field { recv, index, .. } => {
                let mut target = self.place(recv, frame)?;
                loop {
                    let value = target.borrow().clone();
                    match value {
                        Value::Ptr(inner) | Value::Borrow(inner) => target = inner,
                        Value::Shared(held) => target = held.value.clone(),
                        // A field of a union is read out of its bytes, into a cell of its own.
                        Value::Struct(name, fields) if self.plain.get(&*name).is_some_and(|plain| plain.union) => {
                            let bytes = union_bytes(&fields);
                            let ty = &self.plain[&*name].fields[*index];
                            return Ok(cell(load(bytes.as_ptr() as usize, ty, self.plain, expr.span)?));
                        }
                        Value::Struct(_, fields) => return Ok(fields[*index].clone()),
                        _ => unreachable!("the checker rules out field access on `null` and non-structs"),
                    }
                }
            }
            ExprKind::Index { recv, index, .. } => {
                let holder = self.place(recv, frame)?;
                let holder = owned(holder.borrow().clone());
                let at = strip(self.expr(index, frame)?);
                let out_of_range = |at: i128, length: usize| {
                    Unwind::from(Diag::panic(expr.span, format!("index {at} is out of range: the length is {length}")))
                };
                match (holder, &at) {
                    (Value::Str(text), Value::Int(_, at)) => match usize::try_from(*at).ok().and_then(|at| text.as_bytes().get(at)) {
                        Some(byte) => Ok(cell(Value::Int(IntTy::Byte, *byte as i128))),
                        None => Err(out_of_range(*at, text.len())),
                    },
                    (Value::List(items), Value::Int(_, at)) => {
                        let items = items.borrow();
                        match usize::try_from(*at).ok().and_then(|at| items.get(at)) {
                            Some(item) => Ok(item.clone()),
                            None => Err(out_of_range(*at, items.len())),
                        }
                    }
                    (Value::Array(items) | Value::Slice(items), Value::Int(_, at)) => match usize::try_from(*at).ok().and_then(|at| items.get(at)) {
                        Some(item) => Ok(item.clone()),
                        None => Err(out_of_range(*at, items.len())),
                    },
                    (Value::Map(entries), key) => match entries.borrow().get(&key_of(key)) {
                        Some((_, value)) => Ok(value.clone()),
                        None => {
                            let shown = self.show(key, true, expr.span)?;
                            Err(Diag::panic(expr.span, format!("key {shown} is not in the map"))
                                .note("`map.get(key)` gives `null` for a missing key; `map.add(key, value)` puts one in")
                                .into())
                        }
                    },
                    _ => unreachable!("the checker only lets text, lists and maps be indexed"),
                }
            }
            _ => Ok(cell(self.expr(expr, frame)?)),
        }
    }

    fn expr(&mut self, expr: &'a Expr, frame: &mut Frame) -> Res<Value> {
        match &expr.kind {
            ExprKind::Int(value, ty) => Ok(Value::Int(*ty, *value)),
            ExprKind::Float(text, ty) => Ok(Value::Float(*ty, float_literal(text, *ty))),
            ExprKind::Bool(value) => Ok(Value::Bool(*value)),
            ExprKind::Str(text) => Ok(Value::Str(text.as_str().to_string().into())),
            ExprKind::Null => Ok(Value::Null),
            ExprKind::Interp(parts) => {
                // The values are computed first, from left to right; they are written after
                // that, so a `format` method runs when every value of the line is there.
                let mut values = Vec::new();
                for part in parts {
                    if let InterpPart::Expr(inner, _) = part {
                        values.push(self.expr(inner, frame)?);
                    }
                }
                let mut values = values.iter();
                let mut text = String::new();
                for part in parts {
                    match part {
                        InterpPart::Text(piece) => text.push_str(piece),
                        InterpPart::Expr(inner, spec) => {
                            let value = values.next().expect("one value for each part");
                            let shown = match spec.and_then(|spec| spec.letter) {
                                Some((letter, digits)) => written_as(&owned(value.clone()), letter, digits.unwrap_or(0) as usize),
                                None => self.show(value, false, inner.span)?,
                            };
                            let width = spec.map_or(0, |spec| spec.width);
                            let fill = (width.unsigned_abs() as usize).saturating_sub(shown.chars().count());
                            if width > 0 {
                                text.extend(std::iter::repeat_n(' ', fill));
                            }
                            text.push_str(&shown);
                            if width < 0 {
                                text.extend(std::iter::repeat_n(' ', fill));
                            }
                        }
                    }
                }
                Ok(Value::Fmt(text.into()))
            }
            // A moved value leaves its place empty, as it does in compiled code: what is left
            // behind would otherwise count as one more owner of a `shared` value. A moved
            // writer is the only one that writes its buffer, when it is dropped.
            ExprKind::Field { .. } | ExprKind::Index { .. } if expr.in_raw_memory() => {
                Ok(self.raw_place(expr, "read", frame)?.read(self.plain, expr.span)?)
            }
            ExprKind::Var(_) | ExprKind::Field { .. } if expr.moved => Ok(self.place(expr, frame)?.replace(Value::Null)),
            ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. } => {
                let value = fresh(&self.place(expr, frame)?.borrow());
                Ok(value)
            }
            ExprKind::FuncRef(name) if matches!(expr.ty, Ty::CFunc(..)) => Ok(Value::Raw(RawPtr { addr: self.io.ffi.address(name), block: None })),
            ExprKind::FuncRef(name) => Ok(Value::Func(name.as_str().into())),
            // C owns the memory of its variable, so it is not checked as a block.
            ExprKind::CVar(name) => Ok(Value::Raw(RawPtr { addr: self.io.ffi.variable(name), block: None })),
            ExprKind::Constant { ty, name } => Ok(constant(ty, name)),
            ExprKind::Const(name) => Ok(fresh(&self.const_cell(name, expr.span)?.borrow())),
            ExprKind::Embed(index) => {
                let bytes = self.consts.embeds[*index].bytes.as_ref().expect("read before the checker runs");
                Ok(Value::Array(Rc::new(bytes.iter().map(|byte| cell(Value::Int(IntTy::Byte, *byte as i128))).collect())))
            }
            ExprKind::Collection { ty, items, .. } => {
                let given = items.iter().flatten();
                if ty.name == "List" {
                    let mut cells = Vec::new();
                    for item in given {
                        cells.push(cell(self.expr(&item.values[0], frame)?));
                    }
                    return Ok(Value::List(Rc::new(RefCell::new(cells))));
                }
                if ty.name == "HashSet" {
                    let mut set = Table::new();
                    for item in given {
                        let value = strip(self.expr(&item.values[0], frame)?);
                        set.insert(key_of(&value), value);
                    }
                    return Ok(Value::Set(Rc::new(RefCell::new(set))));
                }
                let mut entries = Entries::new();
                for item in given {
                    let key = strip(self.expr(&item.values[0], frame)?);
                    let value = self.expr(&item.values[1], frame)?;
                    let shown = self.show(&key, true, item.span)?;
                    map_add(&mut entries, key, value, shown, item.span)?;
                }
                Ok(Value::Map(Rc::new(RefCell::new(entries))))
            }
            ExprKind::Widen(ty, inner) => Ok(convert(strip(self.expr(inner, frame)?), ty)),
            ExprKind::Load(inner) => Ok(fresh(&owned(self.expr(inner, frame)?))),
            ExprKind::Unary(UnOp::Neg, operand) => match strip(self.expr(operand, frame)?) {
                Value::Int(ty, value) => {
                    if !ty.fits(-value) {
                        return Err(Diag::panic(expr.span, format!("overflow: -({value}) does not fit in {}", ty.name())).into());
                    }
                    Ok(Value::Int(ty, -value))
                }
                Value::Float(ty, value) => Ok(Value::Float(ty, -value)),
                _ => unreachable!("the checker makes the operand a number"),
            },
            ExprKind::Unary(UnOp::Not, operand) => Ok(Value::Bool(!self.condition(operand, frame)?)),
            ExprKind::Unary(UnOp::BitNot, operand) => match strip(self.expr(operand, frame)?) {
                Value::Int(ty, value) => Ok(Value::Int(ty, ty.wrap(!value))),
                _ => unreachable!("the checker makes the operand an integer"),
            },
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), left, right) => {
                let left_value = self.condition(left, frame)?;
                // Short circuit: when the left side decides the result, the right side does not run.
                if left_value == (*op == BinOp::Or) {
                    return Ok(Value::Bool(left_value));
                }
                Ok(Value::Bool(self.condition(right, frame)?))
            }
            // `p + n`: `n` items further on.
            ExprKind::Binary(op, left, right) if matches!(expr.ty, Ty::Raw(_)) => {
                let Ty::Raw(item) = &expr.ty else { unreachable!("tested above") };
                let base = raw_of(&strip(self.expr(left, frame)?));
                let count = self.int(right, frame)?;
                let (size, _) = raw::layout(item, self.plain);
                let count = if *op == BinOp::Sub { count.wrapping_neg() } else { count };
                Ok(Value::Raw(base.offset(count.wrapping_mul(size as i128))))
            }
            // Two addresses, or one and `null`.
            ExprKind::Binary(op, left, right) if matches!(left.ty.strip_borrow(), Ty::CFunc(..)) || matches!(right.ty.strip_borrow(), Ty::CFunc(..)) => {
                let left = raw_of(&strip(self.expr(left, frame)?));
                let right = raw_of(&strip(self.expr(right, frame)?));
                Ok(Value::Bool((left.addr == right.addr) == (*op == BinOp::Eq)))
            }
            ExprKind::Binary(op, left, right) if matches!(left.ty.strip_borrow(), Ty::Raw(_)) || matches!(right.ty.strip_borrow(), Ty::Raw(_)) => {
                let item = match left.ty.strip_borrow() {
                    Ty::Raw(item) => (**item).clone(),
                    _ => Ty::Void,
                };
                let left = raw_of(&strip(self.expr(left, frame)?));
                let right = raw_of(&strip(self.expr(right, frame)?));
                Ok(match op {
                    BinOp::Lt => Value::Bool(left.addr < right.addr),
                    BinOp::Le => Value::Bool(left.addr <= right.addr),
                    BinOp::Gt => Value::Bool(left.addr > right.addr),
                    BinOp::Ge => Value::Bool(left.addr >= right.addr),
                    // Two pointers into two blocks have nothing between them to count.
                    BinOp::Sub => {
                        if let (Some(a), Some(b)) = (left.block, right.block) {
                            if a != b {
                                return Err(Diag::panic(expr.span, "the items between two pointers are counted, and the two point into two blocks of raw memory").into());
                            }
                        }
                        let (size, _) = raw::layout(&item, self.plain);
                        let bytes = left.addr as i128 - right.addr as i128;
                        Value::Int(IntTy::Nint, if size == 0 { 0 } else { bytes / size as i128 })
                    }
                    _ => Value::Bool((left.addr == right.addr) == (*op == BinOp::Eq)),
                })
            }
            ExprKind::Binary(op, left, right) => {
                let left = strip(self.expr(left, frame)?);
                let right = strip(self.expr(right, frame)?);
                self.binary(*op, left, right, expr.span)
            }
            ExprKind::Unsafe(inner) => self.expr(inner, frame),
            // `(ref State)data`: the value whose address `Memory.addressOf` gave, if it is
            // still there and of that type.
            ExprKind::RawCast { expr: inner, .. } if expr.ty.is_borrow() => {
                let pointer = raw_of(&strip(self.expr(inner, frame)?));
                let wanted = expr.ty.strip_borrow();
                let lent = if matches!(expr.ty, Ty::Ref(_)) { "ref" } else { "read" };
                let found = ADDRESSES.with(|given| given.borrow().get(&pointer.addr).cloned());
                let why = match found {
                    _ if pointer.addr == 0 => format!("`({lent} {wanted})` is given an address that is `null`"),
                    None => format!("`({lent} {wanted})` is given an address that `Memory.addressOf` did not give"),
                    Some((_, ty)) if ty != *wanted => format!("`({lent} {wanted})` is given the address of a `{ty}`"),
                    Some((held, ty)) => match held.upgrade() {
                        None => format!("`({lent} {wanted})` is given the address of a value that has ended"),
                        Some(held) if matches!(*held.borrow(), Value::Null) && !matches!(ty, Ty::Nullable(_)) => {
                            format!("`({lent} {wanted})` is given the address of a value that was moved away")
                        }
                        Some(held) => return Ok(Value::Borrow(held)),
                    },
                };
                Err(Diag::panic(expr.span, why).into())
            }
            ExprKind::RawCast { expr: inner, .. } => Ok(match strip(self.expr(inner, frame)?) {
                Value::Null => Value::Null,
                // A number made into a pointer finds its block again by its address.
                Value::Int(_, number) => {
                    let addr = number as i64 as usize;
                    Value::Raw(RawPtr { addr, block: raw::block_at(addr) })
                }
                pointer => Value::Raw(raw_of(&pointer)),
            }),
            ExprKind::SizeOf { align, of, .. } => {
                let (size, alignment) = raw::layout(of, self.plain);
                Ok(Value::Int(IntTy::Nint, if *align { alignment } else { size } as i128))
            }
            // `Memory.allocate<T>(n)`
            ExprKind::Allocate { args, .. } => {
                let Ty::Raw(item) = &expr.ty else { unreachable!("the checker gives a raw pointer") };
                let count = self.int(&args[0], frame)?;
                if count < 0 {
                    return Err(Diag::panic(expr.span, format!("`Memory.allocate` is given a count below 0: {count}")).into());
                }
                let (size, align) = raw::layout(item, self.plain);
                match raw::allocate(count as usize, size, align) {
                    Ok(pointer) => Ok(Value::Raw(pointer)),
                    Err(raw::AllocError::TooLarge) => {
                        Err(Diag::panic(expr.span, format!("`Memory.allocate` is asked for more memory than an address can count: {count} items of `{item}`")).into())
                    }
                    Err(raw::AllocError::OutOfMemory) => Err(Diag::panic(expr.span, format!("`Memory.allocate` found no memory for {count} items of `{item}`")).into()),
                }
            }
            ExprKind::Char(value) => Ok(Value::Char(*value)),
            ExprKind::Array { args, items, .. } => {
                let Ty::Array(item, length) = &expr.ty else { unreachable!("the checker gives an array its type") };
                let mut cells = Vec::new();
                match (items, args.first()) {
                    (Some(items), _) => {
                        for given in items {
                            cells.push(cell(self.expr(given, frame)?));
                        }
                    }
                    (None, Some(value)) => {
                        let value = self.expr(value, frame)?;
                        cells.extend((0..*length).map(|_| cell(fresh(&value))));
                    }
                    (None, None) => cells.extend((0..*length).map(|_| cell(zero(item)))),
                }
                Ok(Value::Array(Rc::new(cells)))
            }
            ExprKind::AsSlice(inner) => Ok(Value::Slice(cells_of(&owned(self.expr(inner, frame)?)))),
            ExprKind::Slice { recv, from, to } => {
                // The items where they are, not a copy of them: a place is looked into.
                let holder = match recv.is_place() {
                    true => owned(self.place(recv, frame)?.borrow().clone()),
                    false => owned(self.expr(recv, frame)?),
                };
                let length = match &holder {
                    Value::Str(text) => text.len() as i128,
                    other => cells_of(other).len() as i128,
                };
                let from = match from {
                    Some(from) => self.int(from, frame)?,
                    None => 0,
                };
                let to = match to {
                    Some(to) => self.int(to, frame)?,
                    None => length,
                };
                // An end that lies outside is said first: `xs[4..]` on three items is out of
                // range, not a range that ends before it starts.
                if from < 0 || from > length || to < 0 || to > length {
                    return Err(Diag::panic(expr.span, format!("range {from}..{to} is out of range: the length is {length}")).into());
                }
                if from > to {
                    return Err(Diag::panic(expr.span, format!("range {from}..{to} ends before it starts")).into());
                }
                let Value::Str(text) = holder else {
                    return Ok(Value::Slice(Rc::new(cells_of(&holder)[from as usize..to as usize].to_vec())));
                };
                let (start, end) = (from as usize, to as usize);
                if let Some(cut) = [start, end].into_iter().find(|at| !text.is_char_boundary(*at)) {
                    let inside = text.char_indices().take_while(|(at, _)| *at < cut).last().expect("a character starts before the cut");
                    let (first, last) = (inside.0, inside.0 + inside.1.len_utf8());
                    return Err(Diag::panic(expr.span, format!("range {from}..{to} cuts a character in half"))
                        .note(format!("`{}` takes bytes {first}..{last}; a `string` is always valid UTF-8", inside.1))
                        .into());
                }
                // A window in the design; a copy here, which cannot be told apart.
                Ok(Value::Str(text[start..end].to_string().into()))
            }
            ExprKind::Cast { ty, expr: inner } => match (strip(self.expr(inner, frame)?), ty) {
                (Value::Raw(pointer), Ty::Int(IntTy::Nint)) => Ok(Value::Int(IntTy::Nint, pointer.addr as isize as i128)),
                (Value::Raw(pointer), Ty::Int(_)) => Ok(Value::Int(IntTy::Nuint, pointer.addr as i128)),
                (Value::Null, Ty::Int(to)) => Ok(Value::Int(*to, 0)),
                (Value::Int(_, value), Ty::Int(to)) if to.fits(value) => Ok(Value::Int(*to, value)),
                (Value::Int(_, value), Ty::Int(to)) => Err(Diag::panic(expr.span, format!("value {value} does not fit in {}", to.name()))
                    .hint(format!("`tryTo{0}()` gives `null` when the value does not fit, and `wrappingTo{0}()` cuts it to size", to.title()))
                    .into()),
                (Value::Float(_, value), Ty::Int(to)) if float_fits(value, *to) => Ok(Value::Int(*to, value.trunc() as i128)),
                (Value::Float(from, value), Ty::Int(to)) => {
                    Err(Diag::panic(expr.span, format!("value {} does not fit in {}", show_float(value, from), to.name()))
                        .hint(format!("`tryTo{}()` gives `null` when the value does not fit", to.title()))
                        .into())
                }
                // A code that is no character panics in every build: a text holds characters only.
                (Value::Int(_, value), Ty::Char) => match crate::check::char_of(value) {
                    Some(found) => Ok(Value::Char(found)),
                    None => Err(Diag::panic(expr.span, format!("{value} is not the code of a character"))
                        .hint("`tryToChar()` gives `null` when no character has the code")
                        .into()),
                },
                (Value::Char(value), Ty::Char) => Ok(Value::Char(value)),
                (Value::Char(value), Ty::Int(to)) if to.fits(value as i128) => Ok(Value::Int(*to, value as i128)),
                (Value::Char(value), Ty::Int(to)) => Err(Diag::panic(expr.span, format!("value {} does not fit in {}", value as u32, to.name()))
                    .hint("`(int)c` holds the code of every character")
                    .into()),
                (value, to) => Ok(convert(value, to)),
            },
            // A union is its bytes: zero, with the one field given written over them.
            ExprKind::StructLit { name, fields, .. } if self.plain.get(name.as_str()).is_some_and(|plain| plain.union) => {
                let value = self.expr(&fields[0].value, frame)?;
                let ty = Ty::Struct(name.clone());
                let (size, _) = raw::layout(&ty, self.plain);
                let mut bytes = vec![0u8; size];
                let given = bytes_of(&value, &self.plain[name.as_str()].fields[fields[0].index], self.plain);
                bytes[..given.len()].copy_from_slice(&given);
                Ok(union_value(name, &bytes))
            }
            ExprKind::StructLit { name, fields, .. } => {
                let mut cells: Vec<Option<Cell>> = vec![None; fields.len()];
                for init in fields {
                    let value = self.expr(&init.value, frame)?;
                    if let Some(width) = self.plain.get(name.as_str()).and_then(|plain| plain.bits[init.index]) {
                        fits_bits(&value, width, init.value.span)?;
                    }
                    cells[init.index] = Some(cell(value));
                }
                let cells = cells.into_iter().map(|field| field.expect("the checker requires every field")).collect();
                // One of a `Disposable` type is disposed of when its last owner lets it go.
                let disposed_as = self.structs[name.as_str()].disposable().then(|| name.as_str().into());
                Ok(Value::Struct(name.as_str().into(), Rc::new(Fields { cells, disposed_as })))
            }
            ExprKind::New(inner) => Ok(Value::Ptr(cell(self.expr(inner, frame)?))),
            // An atomic value is kept as the value itself: one task runs at a time.
            ExprKind::Atomic(inner) => self.expr(inner, frame),
            ExprKind::NewShared { value, checked, .. } => Ok(Value::Shared(Held::new(self.expr(value, frame)?, *checked, expr.span))),
            ExprKind::Open { inner, how } => {
                let value = self.expr(inner, frame)?;
                self.open(&value, *how, inner)?;
                Ok(value)
            }
            ExprKind::Within(inner) => {
                let mark = self.guards.len();
                let value = self.expr(inner, frame);
                self.guards.truncate(mark);
                value
            }
            ExprKind::Share(inner) => Ok(match strip(self.expr(inner, frame)?) {
                Value::Shared(held) => Value::Shared(held),
                Value::Weak(looked) => looked.upgrade().map_or(Value::Null, Value::Shared),
                Value::Null => Value::Null,
                _ => unreachable!("the checker lets `share` take only `shared` and `weak` values"),
            }),
            ExprKind::WeakOf(inner) => Ok(match strip(self.expr(inner, frame)?) {
                Value::Shared(held) => Value::Weak(Rc::downgrade(&held)),
                Value::Weak(looked) => Value::Weak(looked),
                Value::Null => Value::Null,
                _ => unreachable!("the checker lets `weak` take only `shared` and `weak` values"),
            }),
            ExprKind::Borrow { expr: inner, .. } => Ok(Value::Borrow(self.place(inner, frame)?)),
            ExprKind::IsNull { expr: inner, negated } => {
                let is_null = matches!(strip(self.expr(inner, frame)?), Value::Null);
                Ok(Value::Bool(is_null != *negated))
            }
            ExprKind::Coalesce { left, right, on_result: false } => {
                let value = self.expr(left, frame)?;
                if matches!(strip(value.clone()), Value::Null) {
                    self.expr(right, frame)
                } else {
                    Ok(value)
                }
            }
            ExprKind::Coalesce { left, right, on_result: true } => match self.expr(left, frame)? {
                Value::Enum(_, 0, fields) => Ok(fields[0].borrow().clone()),
                _ => self.expr(right, frame),
            },
            ExprKind::Try { expr: inner, kind } => {
                let value = self.expr(inner, frame)?;
                match (kind, value) {
                    (TryKind::Null, Value::Null) => Err(Unwind::Return(Value::Null)),
                    (TryKind::Null, value) => Ok(value),
                    (TryKind::Result(_), Value::Enum(_, 0, fields)) => Ok(fields.first().map_or(Value::Void, |field| field.borrow().clone())),
                    (TryKind::Result(convert), Value::Enum(result, _, fields)) => {
                        let mut cause = fields[0].borrow().clone();
                        if let Some((owner, variant)) = convert {
                            cause = Value::Enum(owner.as_str().into(), *variant, Rc::new(vec![cell(cause)]));
                        }
                        Err(Unwind::Return(Value::Enum(result, 1, Rc::new(vec![cell(cause)]))))
                    }
                    _ => unreachable!("the checker only lets `try` open a `Result` or a `T?`"),
                }
            }
            ExprKind::Return(value) => {
                let value = match value {
                    Some(expr) => self.expr(expr, frame)?,
                    None => Value::Void,
                };
                Err(Unwind::Return(value))
            }
            ExprKind::Variant { owner, index, args } => {
                let cells = self.values(args, frame)?.into_iter().map(cell).collect();
                Ok(Value::Enum(owner.as_str().into(), *index, Rc::new(cells)))
            }
            ExprKind::Unbox(inner) => match self.expr(inner, frame)? {
                Value::Borrow(target) => match &*behind_borrows(target).borrow() {
                    Value::Ptr(owned) => Ok(Value::Borrow(owned.clone())),
                    Value::Shared(held) => Ok(Value::Borrow(held.value.clone())),
                    _ => unreachable!("the checker only unboxes a borrow of a pointer"),
                },
                _ => unreachable!("the checker only unboxes a borrow"),
            },
            ExprKind::IsVariant { expr: inner, negated, pattern, .. } => {
                let value = cell(self.expr(inner, frame)?);
                // `is` only looks, so the names borrow.
                let mut binds = Vec::new();
                let found = fits(pattern, &value, true, &mut binds);
                if found {
                    for (name, bound) in binds {
                        frame.scopes.last_mut().expect("an open scope").insert(name, cell(bound));
                    }
                }
                Ok(Value::Bool(found != *negated))
            }
            ExprKind::Match { subject, arms, borrowed } => {
                let value = self.expr(subject, frame)?;
                let held = cell(value.clone());
                let depth = frame.scopes.len();
                for arm in arms {
                    frame.scopes.push(Scope::default());
                    // `var other` takes the value as it was given; any other pattern looks inside.
                    let mut binds = Vec::new();
                    let fits = match &arm.pattern.kind {
                        PatternKind::Bind(name) => {
                            binds.push((name.clone(), value.clone()));
                            true
                        }
                        _ => fits(&arm.pattern, &held, *borrowed, &mut binds),
                    };
                    if fits {
                        for (name, bound) in binds {
                            frame.declare(&name, bound);
                        }
                    }
                    let chosen = fits
                        && match &arm.guard {
                            Some(guard) => self.condition(guard, frame)?,
                            None => true,
                        };
                    if chosen {
                        for stmt in &arm.body {
                            match self.stmt(stmt, frame)? {
                                Flow::Next => {}
                                Flow::Break => return Err(Unwind::Break),
                                Flow::Continue => return Err(Unwind::Continue),
                                Flow::Return(value) => return Err(Unwind::Return(value)),
                            }
                        }
                        let result = match &arm.value {
                            Some(value) => self.expr(value, frame)?,
                            None => Value::Void,
                        };
                        frame.scopes.truncate(depth);
                        return Ok(result);
                    }
                    frame.scopes.truncate(depth);
                }
                unreachable!("the checker requires an arm for every variant")
            }
            // `await f(x)` of an async function runs it here, as part of this task.
            ExprKind::Await(inner) => {
                if let ExprKind::Call { name, args, callee: Callee::Plain, .. } = &inner.kind {
                    if let Some(func) = self.funcs.get(name.as_str()).copied().filter(|func| func.is_async) {
                        let values = self.values(args, frame)?;
                        return self.call(func, None, values, inner.span);
                    }
                }
                // `await x.load()` of an async method runs it here too.
                if let ExprKind::Method { recv, name, args, callee: Callee::Method { owner, this, whole }, .. } = &inner.kind {
                    if self.methods[owner.as_str()][name.as_str()].is_async {
                        return self.method_call(inner, recv, owner, name, *this, *whole, args, frame, true);
                    }
                }
                if let ExprKind::Call { args, callee: Callee::Static { owner, name }, .. } = &inner.kind {
                    let func = self.methods[owner.as_str()][name.as_str()];
                    if func.is_async {
                        let values = self.values(args, frame)?;
                        return self.call(func, None, values, inner.span);
                    }
                }
                let Value::Task(task) = strip(self.expr(inner, frame)?) else { unreachable!("the checker lets only a task be awaited") };
                self.await_task(&task, expr.span)
            }
            ExprKind::Call { name, args, callee, .. } => match callee {
                Callee::Cancel(CancelCall::New) => Ok(Value::Source(Rc::new(RefCell::new(Cancel::default())))),
                Callee::Static { owner, name } => {
                    let func = self.methods[owner.as_str()][name.as_str()];
                    let values = self.values(args, frame)?;
                    self.call(func, None, values, expr.span)
                }
                Callee::Value => {
                    let Value::Func(target) = strip(frame.slot(name).borrow().clone()) else {
                        unreachable!("the checker only lets a function value be called")
                    };
                    let func = self.funcs[&*target];
                    let values = self.values(args, frame)?;
                    self.call(func, None, values, expr.span)
                }
                Callee::Extern => self.call_extern(name, args, expr, frame),
                Callee::Variadic(params) => {
                    let ret = self.foreign[name].1.clone();
                    self.call_c(None, &crate::ffi::variadic_key(name, params, &ret), params, &ret, args, expr, frame)
                }
                Callee::CValue(params) => {
                    let target = raw_of(&strip(frame.slot(name).borrow().clone()));
                    if target.addr == 0 {
                        return Err(Diag::panic(expr.span, format!("`{name}` is called, and it is a `CFunc` that is `null`")).into());
                    }
                    self.call_c(Some(target.addr), name, params, &expr.ty, args, expr, frame)
                }
                _ => self.call_expr(expr, name, args, frame),
            },
            ExprKind::Method { recv, name, args, callee, .. } => match callee {
                Callee::System(call) => self.system(*call, args, frame),
                Callee::Atomic { op, .. } => self.atomic(*op, Some(recv), args, expr.span, frame),
                Callee::Extern | Callee::Variadic(_) | Callee::Pointer => unreachable!("a function of C is called by its name, and `pointer()` is read by its call"),
                // `string.fromCString(p)`: the bytes up to the zero byte, copied and checked.
                Callee::FromCString => {
                    let given = strip(self.expr(&args[0], frame)?);
                    if let Value::CStr(text) = &given {
                        return Ok(match String::from_utf8(text.as_bytes().to_vec()) {
                            Ok(text) => Value::Enum("Result".into(), 0, Rc::new(vec![cell(Value::Str(text.into()))])),
                            Err(error) => {
                                let position = cell(Value::Int(IntTy::Nint, error.utf8_error().valid_up_to() as i128));
                                let cause = Value::Struct("TextError".into(), Fields::plain(vec![position]));
                                Value::Enum("Result".into(), 1, Rc::new(vec![cell(cause)]))
                            }
                        });
                    }
                    let pointer = raw_of(&given);
                    if pointer.addr == 0 {
                        return Err(Diag::panic(expr.span, "`string.fromCString` is given a pointer that is `null`").into());
                    }
                    let length = match raw::text_length(pointer) {
                        Ok(length) => length,
                        Err(raw::Fault::Freed) => return Err(Diag::panic(expr.span, "`string.fromCString` reads raw memory after it was given back with `Memory.free`").into()),
                        Err(_) => return Err(Diag::panic(expr.span, "`string.fromCString` finds no zero byte before the end of the block").into()),
                    };
                    // SAFETY: `length` bytes lie at the pointer: in its block, or C said so.
                    let bytes = unsafe { std::slice::from_raw_parts(pointer.addr as *const u8, length) }.to_vec();
                    Ok(match String::from_utf8(bytes) {
                        Ok(text) => Value::Enum("Result".into(), 0, Rc::new(vec![cell(Value::Str(text.into()))])),
                        Err(error) => {
                            let position = cell(Value::Int(IntTy::Nint, error.utf8_error().valid_up_to() as i128));
                            let cause = Value::Struct("TextError".into(), Fields::plain(vec![position]));
                            Value::Enum("Result".into(), 1, Rc::new(vec![cell(cause)]))
                        }
                    })
                }
                // `CString.from(text)`: a zero byte inside is an error at its position.
                Callee::ToCString => {
                    let Value::Str(text) = strip(self.expr(&args[0], frame)?) else { unreachable!("the checker gives `CString.from` a text") };
                    Ok(match std::ffi::CString::new(text.as_bytes()) {
                        Ok(made) => Value::Enum("Result".into(), 0, Rc::new(vec![cell(Value::CStr(Rc::new(made)))])),
                        Err(error) => {
                            let position = cell(Value::Int(IntTy::Nint, error.nul_position() as i128));
                            let cause = Value::Struct("TextError".into(), Fields::plain(vec![position]));
                            Value::Enum("Result".into(), 1, Rc::new(vec![cell(cause)]))
                        }
                    })
                }
                Callee::Static { owner, name } => {
                    let func = self.methods[owner.as_str()][name.as_str()];
                    let values = self.values(args, frame)?;
                    match func.is_async {
                        true => self.start_async(func, None, values, expr.span),
                        false => self.call(func, None, values, expr.span),
                    }
                }
                Callee::Method { owner, this, whole } => self.method_call(expr, recv, owner, name, *this, *whole, args, frame, false),
                Callee::FieldValue(index) => {
                    let Value::Struct(_, fields) = owned(self.place(recv, frame)?.borrow().clone()) else {
                        unreachable!("the checker only lets a field of a struct be called")
                    };
                    let Value::Func(target) = strip(fields[*index].borrow().clone()) else {
                        unreachable!("the checker only lets a function value be called")
                    };
                    let func = self.funcs[&*target];
                    let values = self.values(args, frame)?;
                    self.call(func, None, values, expr.span)
                }
                Callee::CFieldValue(index, params) => {
                    let Value::Struct(_, fields) = owned(self.place(recv, frame)?.borrow().clone()) else {
                        unreachable!("the checker only lets a field of a struct be called")
                    };
                    let target = raw_of(&strip(fields[*index].borrow().clone()));
                    if target.addr == 0 {
                        return Err(Diag::panic(expr.span, format!("`{name}` is called, and it is a `CFunc` that is `null`")).into());
                    }
                    self.call_c(Some(target.addr), name, params, &expr.ty, args, expr, frame)
                }
                Callee::Value | Callee::CValue(_) => unreachable!("a variable is called without a receiver"),
                Callee::Join => {
                    let Value::Str(separator) = strip(self.expr(&args[0], frame)?) else {
                        unreachable!("the checker makes the separator text")
                    };
                    let Value::List(items) = owned(self.expr(&args[1], frame)?) else {
                        unreachable!("the checker makes the second argument a list")
                    };
                    let parts: Vec<String> = items.borrow().iter().map(|item| item.borrow().format()).collect();
                    Ok(Value::Str(parts.join(&separator).into()))
                }
                Callee::WithCapacity => {
                    let room = self.int(&args[0], frame)?;
                    let Ok(room) = usize::try_from(room) else {
                        return Err(Diag::panic(expr.span, format!("the capacity of a text cannot be negative: {room}")).into());
                    };
                    let mut text = String::new();
                    if text.try_reserve(room).is_err() {
                        return Err(Diag::panic(expr.span, format!("there is no memory for a text of {room} bytes")).into());
                    }
                    Ok(Value::Str(Rc::new(text)))
                }
                Callee::FromUtf8 => {
                    let cells = cells_of(&owned(self.expr(&args[0], frame)?));
                    let bytes: Vec<u8> = cells.iter().map(|item| match &*item.borrow() {
                        Value::Int(_, byte) => *byte as u8,
                        _ => unreachable!("the checker makes the items bytes"),
                    }).collect();
                    Ok(match String::from_utf8(bytes) {
                        Ok(text) => Value::Enum("Result".into(), 0, Rc::new(vec![cell(Value::Str(text.into()))])),
                        Err(error) => {
                            let position = cell(Value::Int(IntTy::Nint, error.utf8_error().valid_up_to() as i128));
                            let cause = Value::Struct("TextError".into(), Fields::plain(vec![position]));
                            Value::Enum("Result".into(), 1, Rc::new(vec![cell(cause)]))
                        }
                    })
                }
                // `ulong.fromLittleEndian(bytes)`: exactly as many bytes as the type takes.
                // The bytes, laid where a value of the type lies.
                Callee::StructFromBytes(name) => {
                    let cells = cells_of(&owned(self.expr(&args[0], frame)?));
                    let ty = Ty::Struct(name.clone());
                    let (size, _) = raw::layout(&ty, self.plain);
                    if cells.len() != size {
                        let message = format!("`{}.fromBytes` reads {size} bytes, and it was given {}", crate::ast::short_name(name), cells.len());
                        return Err(Diag::panic(expr.span, message).into());
                    }
                    let bytes: Vec<u8> = cells
                        .iter()
                        .map(|item| match *item.borrow() {
                            Value::Int(_, byte) => byte as u8,
                            _ => unreachable!("the checker makes the items bytes"),
                        })
                        .collect();
                    Ok(load(bytes.as_ptr() as usize, &ty, self.plain, expr.span)?)
                }
                Callee::FromBytes { ty, big } => {
                    let cells = cells_of(&owned(self.expr(&args[0], frame)?));
                    let size = (ty.bits() / 8) as usize;
                    if cells.len() != size {
                        let message = format!("`{}.{name}` reads {size} bytes, and it was given {}", ty.name(), cells.len());
                        return Err(Diag::panic(expr.span, message).into());
                    }
                    let mut bits: u128 = 0;
                    for (at, item) in cells.iter().enumerate() {
                        let Value::Int(_, byte) = &*item.borrow() else { unreachable!("the checker makes the items bytes") };
                        let shift = if *big { (size - 1 - at) * 8 } else { at * 8 };
                        bits |= (*byte as u128) << shift;
                    }
                    Ok(Value::Int(*ty, ty.wrap(bits as i128)))
                }
                Callee::Parse(radix) => {
                    let Value::Str(text) = strip(self.expr(&args[0], frame)?) else {
                        unreachable!("the checker makes the argument text")
                    };
                    let Ty::Result(ty, _) = &expr.ty else { unreachable!("`parse` gives a `Result`") };
                    Ok(match parse_number(text.as_bytes(), ty, *radix) {
                        Ok(value) => Value::Enum("Result".into(), 0, Rc::new(vec![cell(value)])),
                        Err(error) => {
                            let cause = match error {
                                Some(position) => Value::Enum("NumberError".into(), 0, Rc::new(vec![cell(Value::Int(IntTy::Nint, position as i128))])),
                                None => Value::Enum("NumberError".into(), 1, Rc::new(Vec::new())),
                            };
                            Value::Enum("Result".into(), 1, Rc::new(vec![cell(cause)]))
                        }
                    })
                }
                Callee::Io(call) => self.io(*call, args, frame),
                Callee::Clock(wall) => Ok(Value::Int(IntTy::Long, clock(*wall))),
                Callee::Os(os) => Ok(Value::Bool(os.here())),
                Callee::Entropy => {
                    let cells = cells_of(&owned(self.expr(&args[0], frame)?));
                    let mut bytes = vec![0u8; cells.len()];
                    if !entropy(&mut bytes) {
                        return Err(Diag::panic(expr.span, "the system gave no random bytes").into());
                    }
                    for (slot, byte) in cells.iter().zip(&bytes) {
                        *slot.borrow_mut() = Value::Int(IntTy::Byte, i128::from(*byte));
                    }
                    Ok(Value::Void)
                }
                Callee::Memory(call) => self.memory(*call, args, expr.span, frame),
                Callee::Stream(call) => self.stream(*call, recv, args, None, frame),
                Callee::Net(call) => self.net(*call, recv, args, None, expr.span, frame),
                // Awaited where it is written: the wait is done here, and the task given has
                // ended.
                Callee::NetAsync(call) => {
                    let given = match call {
                        NetCall::Connect | NetCall::Receive | NetCall::Send => 1,
                        NetCall::Accept | NetCall::Bind | NetCall::Port | NetCall::Reader => 0,
                    };
                    let value = self.net(*call, recv, &args[..given], args.get(given), expr.span, frame)?;
                    Ok(self.ended_task(value, expr.span))
                }
                // `Task.run(work, x)`: the task runs when it is waited for.
                Callee::TaskRun => {
                    let Value::Func(name) = strip(self.expr(&args[0], frame)?) else { unreachable!("the checker lets `Task.run` start only a function") };
                    let values = self.values(&args[1..], frame)?;
                    let task = Rc::new(RefCell::new(Task { state: TaskState::Lazy(name, values), started: expr.span, waiters: Vec::new() }));
                    self.started.push(task.clone());
                    let mut state = self.io.sched.lock();
                    // Those that have run, or are gone, are let go of now and then.
                    if state.lazy.len() >= 64 {
                        state.lazy.retain(|task| task.upgrade().is_some_and(|task| matches!(task.borrow().state, TaskState::Lazy(..))));
                    }
                    state.lazy.push(Rc::downgrade(&task));
                    drop(state);
                    Ok(Value::Task(task))
                }
                Callee::Delay(told) => {
                    let Value::Int(_, ms) = strip(self.expr(&args[0], frame)?) else { unreachable!("the checker lets `Task.delay` take an `int`") };
                    let token = match told {
                        true => Some(self.token(&args[1], frame)?),
                        false => None,
                    };
                    self.delay(ms, token, expr.span)
                }
                Callee::Cancel(CancelCall::After) => {
                    let Value::Int(_, ms) = strip(self.expr(&args[0], frame)?) else { unreachable!("the checker makes the time an `int`") };
                    self.source_after(ms, expr.span)
                }
                Callee::Cancel(call) => {
                    let source = match owned(self.expr(recv, frame)?) {
                        Value::Source(source) => Some(source),
                        Value::Token(source) => source,
                        _ => unreachable!("the checker calls these on a source or a token"),
                    };
                    let is_cancelled = source.as_ref().is_some_and(|source| source.borrow().cancelled);
                    Ok(match call {
                        CancelCall::Cancel => {
                            let source = source.expect("a source");
                            let mut state = self.io.sched.lock();
                            Sched::cancel(&mut state, &source);
                            Value::Void
                        }
                        CancelCall::Token | CancelCall::Clone => Value::Token(source),
                        CancelCall::IsCancelled => Value::Bool(is_cancelled),
                        CancelCall::Check if is_cancelled => failed(cancelled()),
                        CancelCall::Check => ok(Value::Void),
                        CancelCall::New | CancelCall::After => unreachable!("handled above"),
                    })
                }
                // Input and output in the interpreter is done when it is called, so the task
                // has ended when it is given; a token is looked at before the work.
                Callee::IoAsync(call) => {
                    let mut values = self.values(args, frame)?;
                    let params = match call {
                        IoCall::ReadText | IoCall::ReadBytes | IoCall::OpenText | IoCall::CreateText | IoCall::OpenAppend => 1,
                        IoCall::WriteText | IoCall::WriteBytes | IoCall::AppendText => 2,
                    };
                    let source = match values.len() > params {
                        true => match owned(values.pop().expect("a token")) {
                            Value::Token(source) => source,
                            _ => unreachable!("the checker makes it a token"),
                        },
                        false => None,
                    };
                    let value = match source.is_some_and(|source| source.borrow().cancelled) {
                        true => failed(io_cancelled()),
                        false => self.io_values(*call, values)?,
                    };
                    Ok(self.ended_task(value, expr.span))
                }
                // A writer's async methods do the work of the methods without `Async`; the
                // block that the compiled code writes on a thread of its own is written here.
                Callee::StreamAsync(call) => {
                    let given = if matches!(call, StreamCall::ReadText | StreamCall::Flush | StreamCall::Close) { 0 } else { 1 };
                    let token = args.get(given);
                    // The input that can make the program wait is waited for outside the run.
                    if matches!(call, StreamCall::ReadLine | StreamCall::ReadText | StreamCall::ReadBytes | StreamCall::ReadSome) && self.io.input.borrow().waits {
                        let stdin = matches!(owned(self.expr(recv, frame)?), Value::Reader(reader) if reader.borrow().is_stdin());
                        if stdin {
                            if let Some(cancelled) = self.input_wait(*call, &args[..given], token, expr.span, frame)? {
                                return Ok(self.ended_task(cancelled, expr.span));
                            }
                        }
                    }
                    let value = self.stream(*call, recv, &args[..given], token, frame)?;
                    Ok(self.ended_task(value, expr.span))
                }
                Callee::Wait => {
                    let Value::Task(task) = strip(self.expr(recv, frame)?) else { unreachable!("the checker lets only a task be waited for") };
                    self.await_task(&task, expr.span)
                }
                // `Task.whenAll(tasks)` and `Task.whenAny(ref tasks)` are awaited where they are
                // written: the wait is done here, and the task given has ended.
                Callee::WhenAll => {
                    let Value::List(list) = strip(self.expr(&args[0], frame)?) else { unreachable!("the checker gives `Task.whenAll` a list") };
                    let gives = !matches!(&expr.ty, Ty::Task(result) if **result == Ty::Void);
                    self.when_all(&list, gives, expr.span)
                }
                Callee::WhenAny => {
                    let Value::List(list) = strip(self.expr(&args[0], frame)?) else { unreachable!("the checker gives `Task.whenAny` a list") };
                    self.when_any(&list, expr.span)
                }
                Callee::Take => {
                    let slot = behind_borrows(self.place(recv, frame)?);
                    let old = slot.replace(Value::Null);
                    Ok(old)
                }
                Callee::Replace => {
                    let slot = behind_borrows(self.place(recv, frame)?);
                    let new = self.expr(&args[0], frame)?;
                    let old = slot.replace(new);
                    Ok(old)
                }
                // `text.write(x)` and `text.clear()` change the text where it is. A text that
                // nothing else holds grows in place; one that is shared is copied first.
                Callee::Plain if matches!(name.as_str(), "write" | "clear") && is_text_behind(&recv.ty) => {
                    let mut slot = self.place(recv, frame)?;
                    loop {
                        let inner = match &*slot.borrow() {
                            Value::Borrow(inner) | Value::Ptr(inner) => inner.clone(),
                            _ => break,
                        };
                        slot = inner;
                    }
                    let added = match args.first() {
                        Some(arg) => {
                            let value = self.expr(arg, frame)?;
                            self.show(&value, false, arg.span)?
                        }
                        None => String::new(),
                    };
                    let mut value = slot.borrow_mut();
                    let Value::Str(text) = &mut *value else { unreachable!("the checker makes the receiver a text") };
                    let text = Rc::make_mut(text);
                    if name == "clear" {
                        text.clear();
                    }
                    text.push_str(&added);
                    Ok(Value::Void)
                }
                // `.clone()` of a `shared` value: one more owner.
                Callee::Plain if name == "clone" && matches!(recv.ty.strip_borrow(), Ty::Shared(..)) => Ok(strip(self.expr(recv, frame)?)),
                Callee::Plain => {
                    // An array that is put in order is the one in its place, not a copy of it.
                    let mark = self.guards.len();
                    let recv = match recv.is_place() && matches!(name.as_str(), "sort" | "reverse") {
                        true => owned(self.place(recv, frame)?.borrow().clone()),
                        false => owned(self.expr(recv, frame)?),
                    };
                    let lent = self.guards.len();
                    if let Value::Lines(lines) = &recv {
                        return Ok(self.next_line(lines, expr.span)?.unwrap_or(Value::Null));
                    }
                    // `f.write(x)` inside a `format` method.
                    if let Value::Formatter(written) = &recv {
                        let value = self.expr(&args[0], frame)?;
                        let text = self.show(&value, false, args[0].span)?;
                        written.borrow_mut().push_str(&text);
                        return Ok(Value::Void);
                    }
                    let mut values = Vec::new();
                    for arg in args {
                        values.push(strip(self.expr(arg, frame)?));
                    }
                    self.change_reserved(mark..lent)?;
                    // An array or a slice keeps its cells, which are those of its owner; the
                    // values move between them.
                    if let (Value::Array(cells) | Value::Slice(cells), "sort" | "reverse") = (&recv, name.as_str()) {
                        let order = match name.as_str() {
                            "sort" => self.sorted((**cells).clone(), expr.span)?,
                            _ => cells.iter().rev().cloned().collect(),
                        };
                        let values: Vec<Value> = order.iter().map(|item| item.borrow().clone()).collect();
                        for (item, value) in cells.iter().zip(values) {
                            *item.borrow_mut() = value;
                        }
                        return Ok(Value::Void);
                    }
                    let recv = as_list(recv);
                    if matches!(recv, Value::List(_) | Value::Map(_) | Value::Set(_)) {
                        return self.collection_method(recv, name, values, expr.span);
                    }
                    // A separator that is empty would split nowhere and everywhere.
                    if let (Value::Str(_), "split", [Value::Str(separator)]) = (&recv, name.as_str(), values.as_slice()) {
                        if separator.is_empty() {
                            return Err(Diag::panic(expr.span, "the separator of `split` is empty")
                                .hint("the characters, one after the other: `text.chars()`")
                                .into());
                        }
                    }
                    // Nothing to look for, or a number of times that is no number of times.
                    match (&recv, name.as_str(), values.as_slice()) {
                        (Value::Str(_), "replaceAll", [Value::Str(old), _]) if old.is_empty() => {
                            return Err(Diag::panic(expr.span, "the text that `replaceAll` looks for is empty").into());
                        }
                        (Value::Str(text), "repeat", [Value::Int(_, times)]) => {
                            if *times < 0 {
                                return Err(Diag::panic(expr.span, format!("a text cannot be repeated {times} times")).into());
                            }
                            // As in compiled code, a text too big for the memory stops the program
                            // with a panic instead of aborting it.
                            let size = (text.len() as i128).checked_mul(*times).filter(|size| *size <= isize::MAX as i128);
                            let mut repeated = String::new();
                            if size.is_none_or(|size| repeated.try_reserve_exact(size as usize).is_err()) {
                                return Err(Diag::panic(expr.span, format!("there is no memory for a text of {} bytes repeated {times} times", text.len())).into());
                            }
                            for _ in 0..*times {
                                repeated.push_str(text);
                            }
                            return Ok(Value::Str(repeated.into()));
                        }
                        _ => {}
                    }
                    Ok(method(recv, name, values))
                }
            },
        }
    }

    /// The `shared` values made since `first` that are still alive when a program or a test
    /// (`what`) ends: nothing outside them owns them any more, so they hold each other up.
    fn leaked(&self, first: usize, what: &str) -> Option<Diag> {
        let mut alive: Vec<(usize, Rc<Held>, Span)> = LIVE.with(|live| {
            let live = live.borrow();
            live.held.iter().filter(|(id, _)| **id > first).filter_map(|(id, (held, made))| held.upgrade().map(|held| (*id, held, *made))).collect()
        });
        if alive.is_empty() {
            return None;
        }
        alive.sort_by_key(|(id, _, _)| *id);
        let ids: Vec<usize> = alive.iter().map(|(id, _, _)| *id).collect();
        // The values of one type made at one place are counted together, the most first.
        let mut groups: Vec<(String, Span, usize)> = Vec::new();
        for (_, held, made) in &alive {
            let name = kind_name(&held.value.borrow());
            match groups.iter_mut().find(|(other, at, _)| *other == name && at == made) {
                Some(group) => group.2 += 1,
                None => groups.push((name, *made, 1)),
            }
        }
        groups.sort_by_key(|group| std::cmp::Reverse(group.2));
        // The first field found that owns one of the others.
        let through = alive.iter().find_map(|(_, held, _)| {
            let Value::Struct(name, fields) = &*held.value.borrow() else { return None };
            let decl = self.structs.get(name.as_ref())?;
            let at = fields.iter().position(|field| owns_any(&field.borrow(), &ids))?;
            Some((format!("{}.{}", short_name(name), decl.fields[at].name), &decl.fields[at].ty))
        });
        let count = alive.len();
        let plural = if count == 1 { "value" } else { "values" };
        let msg = match &through {
            Some((field, _)) if count == 1 => format!("{what} ended with 1 `shared` value still alive, which holds itself through `{field}`"),
            Some((field, _)) => format!("{what} ended with {count} `shared` {plural} still alive, which hold each other through `{field}`"),
            None => format!("{what} ended with {count} `shared` {plural} still alive"),
        };
        let (name, made, many) = &groups[0];
        let mut diag = Diag::leak(*made, msg).note(format!("{many} x `{name}`, made here:"));
        if let Some((name, made, many)) = groups.get(1) {
            diag = diag.related(*made, format!("{many} x `{name}`, made here"));
        }
        if groups.len() > 2 {
            let rest: usize = groups[2..].iter().map(|(_, _, many)| many).sum();
            diag = diag.note(format!("and {rest} more, made elsewhere"));
        }
        diag = match through {
            Some((_, written)) => {
                // The field's type with its aliases spelled out and `weak` for `shared`.
                let mut text = self.src[written.span.start..written.span.end].to_string();
                for (alias, target) in &self.aliases {
                    let short = short_name(alias);
                    let spelled = &self.src[target.span.start..target.span.end];
                    text = replace_word(&text, short, spelled);
                }
                match text.contains("shared ") {
                    true => diag.hint(format!("the side that does not own is `weak`: `{}`", text.replace("shared ", "weak "))),
                    false => diag.hint("the side that does not own is `weak`"),
                }
            }
            None => diag.hint("one of the values that hold each other should look at the other with `weak`"),
        };
        Some(diag)
    }

    /// Reaches what a `shared checked` value holds: checks the access against the borrows of
    /// it in progress, and records a borrow that goes on after it.
    fn open(&mut self, value: &Value, how: Opening, holder: &Expr) -> Res<()> {
        let Some(held) = held_of(value) else { return Ok(()) };
        let Some(lock) = &held.lock else { return Ok(()) };
        let changing = matches!(how, Opening::Change | Opening::LendChange);
        let what = {
            let mut ty = &holder.ty;
            while let Ty::Read(inner) | Ty::Ref(inner) | Ty::Shared(inner, _) = ty {
                ty = inner;
            }
            short_name(&ty.to_string()).to_string()
        };
        let conflict = {
            let lock = lock.borrow();
            match (lock.write, lock.reads.last()) {
                (Some(at), _) => Some((at, "changed", "it is being changed from here")),
                (None, Some(at)) if changing => Some((*at, "read", "it is being read from here")),
                _ => None,
            }
        };
        if let Some((at, doing, there)) = conflict {
            let verb = if changing { "changed" } else { "read" };
            return Err(Diag::panic(holder.span, format!("this `{what}` is being {doing}, so it cannot be {verb} here"))
                .related(at, there)
                .note("a `shared checked` value is lent to one that changes it or to any number that read it, not both at once")
                .into());
        }
        let write = how == Opening::LendChange;
        match how {
            Opening::Lend | Opening::Reserve | Opening::LendChange => {
                let mut lock = lock.borrow_mut();
                match write {
                    true => lock.write = Some(holder.span),
                    false => lock.reads.push(holder.span),
                }
            }
            Opening::Look | Opening::Change => return Ok(()),
        }
        self.guards.push(Guard { held, write, reserved: how == Opening::Reserve, span: holder.span, what });
        Ok(())
    }

    /// The receiver of a method that changes it was lent to read while the arguments were
    /// computed; now it is lent to change, if nothing else looks at it.
    fn change_reserved(&mut self, lent: std::ops::Range<usize>) -> Res<()> {
        for at in lent {
            let guard = &mut self.guards[at];
            if !guard.reserved {
                continue;
            }
            let Some(lock) = &guard.held.lock else { continue };
            let mut lock = lock.borrow_mut();
            if let Some(mine) = lock.reads.iter().rposition(|span| *span == guard.span) {
                lock.reads.remove(mine);
            }
            if let Some(other) = lock.write.or(lock.reads.last().copied()) {
                let doing = if lock.write.is_some() { "changed" } else { "read" };
                lock.reads.push(guard.span);
                return Err(Diag::panic(guard.span, format!("this `{}` is being {doing}, so it cannot be changed here", guard.what))
                    .related(other, format!("it is being {doing} from here"))
                    .note("a `shared checked` value is lent to one that changes it or to any number that read it, not both at once")
                    .into());
            }
            lock.write = Some(guard.span);
            guard.write = true;
            guard.reserved = false;
        }
        Ok(())
    }

    /// A call of a function of C. Each argument goes into a slot as the C type it is. A borrow
    /// and the items of `buffer.pointer()` are copied into memory of their own, where C can
    /// reach them, and copied back after the call: C may have written them.
    fn call_extern(&mut self, name: &str, args: &'a [Expr], expr: &'a Expr, frame: &mut Frame) -> Res<Value> {
        let (params, ret) = &self.foreign[name];
        let (params, ret) = (params.clone(), ret.clone());
        self.call_c(None, name, &params, &ret, args, expr, frame)
    }

    /// A call of C: of the function of C named `name`, or, with `through`, of the function at
    /// that address (a `CFunc`).
    #[allow(clippy::too_many_arguments)]
    fn call_c(&mut self, through: Option<usize>, name: &str, params: &[Ty], ret: &Ty, args: &'a [Expr], expr: &'a Expr, frame: &mut Frame) -> Res<Value> {
        let (params, ret) = (params.to_vec(), ret.clone());
        // A struct C gives back is written into room whose address follows the arguments.
        let returned_struct = matches!(ret, Ty::Struct(_)).then(|| raw::Scratch::new(raw::layout(&ret, self.plain).0));
        let mut slots: Vec<crate::ffi::Slot> = vec![0; args.len() + usize::from(returned_struct.is_some())];
        if let Some(room) = &returned_struct {
            slots[args.len()] = room.addr() as crate::ffi::Slot;
        }
        // Memory lent for the call: where it is, what lies there, and the cells to write back.
        let mut lent: Vec<(raw::Scratch, Ty, Vec<Cell>)> = Vec::new();
        for ((arg, param), slot) in args.iter().zip(&params).zip(slots.iter_mut()) {
            let at = (slot as *mut crate::ffi::Slot).cast::<u8>();
            let pointer_of = |inner: &'a Expr| match &inner.kind {
                ExprKind::Method { recv, callee: Callee::Pointer, .. } => Some(&**recv),
                _ => None,
            };
            let buffer = match &arg.kind {
                ExprKind::RawCast { expr: inner, .. } => pointer_of(inner),
                _ => pointer_of(arg),
            };
            if let Some(recv) = buffer {
                let holder = self.place(recv, frame)?;
                let items = cells_of(&owned(holder.borrow().clone()));
                let Ty::Raw(item) = &arg_pointer_type(arg) else { unreachable!("`pointer()` gives a raw pointer") };
                let item = (**item).clone();
                let (size, _) = raw::layout(&item, self.plain);
                let scratch = raw::Scratch::new(items.len() * size);
                for (position, held) in items.iter().enumerate() {
                    store(scratch.addr() + position * size, &item, &held.borrow(), self.plain);
                }
                // SAFETY: a slot holds eight bytes, and an address takes as many.
                unsafe { at.cast::<usize>().write(scratch.addr()) };
                lent.push((scratch, item, items.to_vec()));
                continue;
            }
            let value = self.expr(arg, frame)?;
            // SAFETY: a slot holds eight bytes, and each value below takes at most as many.
            unsafe {
                match param {
                    Ty::Int(int) => raw::write_int(at as usize, *int, match strip(value) {
                        Value::Int(_, number) => number,
                        _ => unreachable!("the checker gives an integer"),
                    }),
                    Ty::Float(_) | Ty::Bool | Ty::Raw(_) | Ty::CFunc(..) => store(at as usize, param, &value, self.plain),
                    // A struct given by value is copied out for C, which may change its copy.
                    Ty::Struct(_) => {
                        let (size, _) = raw::layout(param, self.plain);
                        let scratch = raw::Scratch::new(size);
                        store(scratch.addr(), param, &strip(value), self.plain);
                        at.cast::<usize>().write(scratch.addr());
                        lent.push((scratch, param.clone(), Vec::new()));
                    }
                    Ty::Read(inner) | Ty::Ref(inner) => {
                        let Value::Borrow(target) = value else { unreachable!("the checker gives a borrow") };
                        let target = behind_borrows(target);
                        if **inner == Ty::CString {
                            let Value::CStr(text) = &*target.borrow() else { unreachable!("the checker gives a `CString`") };
                            at.cast::<usize>().write(text.as_ptr() as usize);
                            continue;
                        }
                        let (size, _) = raw::layout(inner, self.plain);
                        let scratch = raw::Scratch::new(size);
                        store(scratch.addr(), inner, &target.borrow(), self.plain);
                        at.cast::<usize>().write(scratch.addr());
                        // What C was lent to read is not written back.
                        let back = if matches!(param, Ty::Ref(_)) { vec![target] } else { Vec::new() };
                        lent.push((scratch, (**inner).clone(), back));
                    }
                    other => unreachable!("the checker lets a C function take no `{other}`"),
                }
            }
        }
        let mut addresses: Vec<*mut std::ffi::c_void> = slots.iter_mut().map(|slot| (slot as *mut crate::ffi::Slot).cast::<std::ffi::c_void>()).collect();
        let mut returned: crate::ffi::Slot = 0;
        let mut errno = self.errno;
        let io = self.io;
        // C may call an `export` function back, which this interpreter runs: from here until
        // the call returns, nothing of it is touched but through `ENTERED`.
        windows_out(self.plain);
        let left = self.c_leave(expr.span)?;
        let left_before = LEFT.with(|cell| cell.replace(left));
        let me: *mut Self = self;
        let before = ENTERED.with(|entered| entered.replace(me.cast()));
        let room = (&mut returned as *mut crate::ffi::Slot).cast::<std::ffi::c_void>();
        // SAFETY: the wrapper reads as many slots as the function takes, each as the C type it
        // was declared with, and writes at most eight bytes into `returned`. What the function
        // of C does is the program's to answer for: it is called inside `unsafe`.
        unsafe {
            match through {
                None => io.ffi.wrapper(name)(addresses.as_mut_ptr(), room, errno.as_mut_ptr()),
                Some(address) => io.ffi.through(&crate::ffi::through_key(&params, &ret))(address as *mut std::ffi::c_void, addresses.as_mut_ptr(), room, errno.as_mut_ptr()),
            }
        }
        ENTERED.with(|entered| entered.set(before));
        let left = LEFT.with(|cell| cell.replace(left_before));
        self.c_back(left)?;
        windows_in(self.plain, expr.span)?;
        self.errno = errno;
        for (scratch, item, cells) in &lent {
            let (size, _) = raw::layout(item, self.plain);
            for (position, held) in cells.iter().enumerate() {
                let value = load(scratch.addr() + position * size, item, self.plain, expr.span)?;
                *held.borrow_mut() = value;
            }
        }
        let at = (&returned as *const crate::ffi::Slot).cast::<u8>() as usize;
        Ok(match &ret {
            Ty::Void => Value::Void,
            Ty::Struct(_) => load(returned_struct.as_ref().expect("room for the struct").addr(), &ret, self.plain, expr.span)?,
            Ty::Int(int) => Value::Int(*int, unsafe { raw::read_int(at, *int) }),
            other => load(at, other, self.plain, expr.span)?,
        })
    }

    /// The address of a place in raw memory and the type of what lies there. The item of raw
    /// memory it is in was checked to lie in a live block (`verb` says whether it is read or
    /// written, for the panic).
    fn raw_place(&mut self, expr: &'a Expr, verb: &str, frame: &mut Frame) -> Res<RawSpot> {
        match &expr.kind {
            ExprKind::Index { recv, index, kind: IndexKind::Raw } => {
                let base = raw_of(&strip(self.expr(recv, frame)?));
                let at = self.int(index, frame)?;
                let (size, _) = raw::layout(&expr.ty, self.plain);
                let item = base.offset(at.wrapping_mul(size as i128));
                self.reach(item, 1, &expr.ty, verb, expr.span)?;
                Ok(RawSpot { at: item, ty: expr.ty.clone(), bits: None })
            }
            ExprKind::Field { recv, index, .. } => {
                let RawSpot { at: base, ty, .. } = self.raw_place(recv, verb, frame)?;
                let Ty::Struct(name) = &ty else { unreachable!("a field in raw memory is a field of a struct") };
                Ok(match raw::places(name, self.plain)[*index] {
                    raw::Place::Bytes(at) => RawSpot { at: base.offset(at as i128), ty: expr.ty.clone(), bits: None },
                    raw::Place::Bits { at, width } => RawSpot { at: base, ty: expr.ty.clone(), bits: Some((at, width)) },
                })
            }
            ExprKind::Index { recv, index, .. } => {
                let RawSpot { at: base, ty, .. } = self.raw_place(recv, verb, frame)?;
                let Ty::Array(item, length) = &ty else { unreachable!("an item in raw memory is an item of an array") };
                let at = self.int(index, frame)?;
                if at < 0 || at >= *length as i128 {
                    return Err(Diag::panic(expr.span, format!("index {at} is out of range: the length is {length}")).into());
                }
                let (size, _) = raw::layout(item, self.plain);
                Ok(RawSpot { at: base.offset(at * size as i128), ty: (**item).clone(), bits: None })
            }
            _ => unreachable!("the checker found a place in raw memory"),
        }
    }

    /// Whether `size` bytes of raw memory, a `ty`, can be read or written at `at`; a panic
    /// that says why not.
    /// The union a field is reached in, when it is one.
    fn union_of(&self, recv: &Expr) -> Option<String> {
        struct_name(&recv.ty).filter(|name| self.plain.get(*name).is_some_and(|plain| plain.union)).map(str::to_string)
    }

    fn reach(&self, at: RawPtr, count: usize, ty: &Ty, verb: &str, span: Span) -> Res<()> {
        self.reach_unaligned(at, count, ty, verb, span)?;
        let (_, align) = raw::layout(ty, self.plain);
        if !at.addr.is_multiple_of(align) {
            let off = at.addr % align;
            return Err(Diag::panic(span, format!("raw memory is reached {off} byte(s) past a multiple of {align}, the alignment of `{ty}`: `Memory.readUnaligned` reads it there")).into());
        }
        Ok(())
    }

    /// As `reach`, without the test of the alignment. What the program wrote into the windows of
    /// `Memory.viewRef` goes to the memory first.
    fn reach_unaligned(&self, at: RawPtr, count: usize, ty: &Ty, verb: &str, span: Span) -> Res<()> {
        windows_out(self.plain);
        let (size, _) = raw::layout(ty, self.plain);
        let why = match raw::check(at, count.saturating_mul(size)) {
            Ok(()) => return Ok(()),
            Err(raw::Fault::Null) => format!("raw memory is {verb} through a pointer that is `null`"),
            Err(raw::Fault::Freed) => format!("raw memory is {verb} after it was given back with `Memory.free`"),
            Err(raw::Fault::Outside { at, block }) if size > 0 => {
                let first = at.div_euclid(size as i128);
                let items = match count {
                    1 => format!("item {first}"),
                    _ => format!("items {first} to {}", first + count as i128 - 1),
                };
                format!("raw memory is {verb} outside its block: {items}, where the block holds {} items of `{ty}`", block / size)
            }
            Err(raw::Fault::Outside { at, block }) => format!("raw memory is {verb} outside its block: byte {at}, where the block holds {block} bytes"),
        };
        Err(Diag::panic(span, why).into())
    }

    /// `Memory.free(p)` and `Memory.copy(source, target, count)`.
    fn memory(&mut self, call: MemoryCall, args: &'a [Expr], span: Span, frame: &mut Frame) -> Res<Value> {
        match call {
            // On Linux and macOS the system's last error is `errno` too; on Windows it is
            // `GetLastError`, kept beside it.
            MemoryCall::Errno => Ok(Value::Int(IntTy::Int, self.errno[0] as i128)),
            MemoryCall::LastError => Ok(Value::Int(IntTy::Int, self.errno[LAST_ERROR] as i128)),
            MemoryCall::SetErrno => {
                self.errno[0] = self.int(&args[0], frame)? as i32;
                Ok(Value::Void)
            }
            MemoryCall::SetLastError => {
                self.errno[LAST_ERROR] = self.int(&args[0], frame)? as i32;
                Ok(Value::Void)
            }
            // The address of the cell of the value, kept with its type for `(ref T)data`.
            MemoryCall::AddressOf => {
                let ExprKind::Borrow { expr: place, .. } = &args[0].kind else { unreachable!("the checker gives `Memory.addressOf` a borrow") };
                let held = behind_borrows(self.place(place, frame)?);
                let addr = Rc::as_ptr(&held) as usize;
                ADDRESSES.with(|given| given.borrow_mut().insert(addr, (Rc::downgrade(&held), place.ty.strip_borrow().clone())));
                Ok(Value::Raw(RawPtr { addr, block: None }))
            }
            // The items are read once, into cells of their own: the window only reads, and
            // what lies there is the program's to keep still while it looks (`unsafe`).
            MemoryCall::View => {
                let at = raw_of(&strip(self.expr(&args[0], frame)?));
                let count = self.int(&args[1], frame)?;
                if count < 0 {
                    return Err(Diag::panic(span, format!("`Memory.view` is given a count below 0: {count}")).into());
                }
                let Ty::Raw(item) = args[0].ty.strip_borrow() else { unreachable!("the checker gives `Memory.view` a raw pointer") };
                let count = usize::try_from(count).unwrap_or(usize::MAX);
                if count == 0 {
                    return Ok(Value::Slice(Rc::new(Vec::new())));
                }
                if at.addr == 0 {
                    return Err(Diag::panic(span, format!("`Memory.view` is given `null` and a count of {count}")).into());
                }
                self.reach(at, count, item, "read", span)?;
                let (size, _) = raw::layout(item, self.plain);
                let mut cells = Vec::with_capacity(count);
                for position in 0..count {
                    let value = load(at.addr + position * size, item, self.plain, span)?;
                    cells.push(cell(value));
                }
                Ok(Value::Slice(Rc::new(cells)))
            }
            // A window that writes: cells of its own, kept in step with the memory (`Window`).
            MemoryCall::ViewRef => {
                let at = raw_of(&strip(self.expr(&args[0], frame)?));
                let count = self.int(&args[1], frame)?;
                if count < 0 {
                    return Err(Diag::panic(span, format!("`Memory.viewRef` is given a count below 0: {count}")).into());
                }
                let Ty::Raw(item) = args[0].ty.strip_borrow() else { unreachable!("the checker gives `Memory.viewRef` a raw pointer") };
                let count = usize::try_from(count).unwrap_or(usize::MAX);
                if count == 0 {
                    return Ok(Value::Slice(Rc::new(Vec::new())));
                }
                if at.addr == 0 {
                    return Err(Diag::panic(span, format!("`Memory.viewRef` is given `null` and a count of {count}")).into());
                }
                self.reach(at, count, item, "written", span)?;
                let (size, _) = raw::layout(item, self.plain);
                let mut cells = Vec::with_capacity(count);
                let mut written = Vec::with_capacity(count * size);
                for position in 0..count {
                    let value = load(at.addr + position * size, item, self.plain, span)?;
                    written.extend(bytes_of(&value, item, self.plain));
                    cells.push(cell(value));
                }
                let items = Rc::new(cells);
                let window = Window { items: items.clone(), at, item: (**item).clone(), memory: bytes_at(at.addr, count * size), cells: written };
                WINDOWS.with(|windows| windows.borrow_mut().push(window));
                Ok(Value::Slice(items))
            }
            // A place in raw memory is read and written where it lies, each time it is
            // written: the interpreter leaves nothing out, so a volatile access is any access.
            MemoryCall::ReadVolatile => self.expr(&args[0], frame),
            MemoryCall::WriteVolatile => {
                let new = strip(self.expr(&args[1], frame)?);
                let spot = self.raw_place(&args[0], "written", frame)?;
                spot.write(&new, self.plain);
                windows_in(self.plain, span)?;
                Ok(Value::Void)
            }
            MemoryCall::ReadUnaligned => {
                let at = raw_of(&strip(self.expr(&args[0], frame)?));
                let Ty::Raw(item) = args[0].ty.strip_borrow() else { unreachable!("the checker gives `Memory.readUnaligned` a raw pointer") };
                self.reach_unaligned(at, 1, item, "read", span)?;
                Ok(load(at.addr, item, self.plain, span)?)
            }
            MemoryCall::WriteUnaligned => {
                let at = raw_of(&strip(self.expr(&args[0], frame)?));
                let value = self.expr(&args[1], frame)?;
                let Ty::Raw(item) = args[0].ty.strip_borrow() else { unreachable!("the checker gives `Memory.writeUnaligned` a raw pointer") };
                self.reach_unaligned(at, 1, item, "written", span)?;
                store(at.addr, item, &value, self.plain);
                windows_in(self.plain, span)?;
                Ok(Value::Void)
            }
            // A number that is no code of the system's, below 0, is kept as it is.
            MemoryCall::IoErrorOf | MemoryCall::IoErrorOfErrno => Ok(io_cause(None, Some(self.int(&args[0], frame)? as i32))),
            MemoryCall::Free => {
                let pointer = raw_of(&strip(self.expr(&args[0], frame)?));
                let Ty::Raw(item) = args[0].ty.strip_borrow() else { unreachable!("the checker gives `Memory.free` a raw pointer") };
                let align = if **item == Ty::Void { 1 } else { raw::layout(item, self.plain).1 };
                windows_out(self.plain);
                let why = match raw::free(pointer, align) {
                    Ok(()) => return Ok(Value::Void),
                    Err(raw::FreeError::Twice) => "`Memory.free` is given memory that was given back already".to_string(),
                    Err(raw::FreeError::Inside) => "`Memory.free` is given an address inside a block, not the one `Memory.allocate` gave".to_string(),
                    Err(raw::FreeError::Aligned { made }) => format!(
                        "`Memory.free` is given a block made for a type aligned to {} through a `{}`: a block is given back through a pointer to the type it was made for",
                        match made {
                            Some(made) => format!("{made} bytes"),
                            None => "16 bytes or less".to_string(),
                        },
                        args[0].ty.strip_borrow()
                    ),
                };
                Err(Diag::panic(span, why).into())
            }
            MemoryCall::Copy => {
                let source = raw_of(&strip(self.expr(&args[0], frame)?));
                let target = raw_of(&strip(self.expr(&args[1], frame)?));
                let count = self.int(&args[2], frame)?;
                if count < 0 {
                    return Err(Diag::panic(span, format!("`Memory.copy` is given a count below 0: {count}")).into());
                }
                if count == 0 {
                    return Ok(Value::Void);
                }
                let Ty::Raw(item) = args[0].ty.strip_borrow() else { unreachable!("the checker gives `Memory.copy` raw pointers") };
                let (size, _) = raw::layout(item, self.plain);
                let count = usize::try_from(count).unwrap_or(usize::MAX);
                let bytes = count.saturating_mul(size);
                self.reach(source, count, item, "read", span)?;
                self.reach(target, count, item, "written", span)?;
                // SAFETY: both ranges lie in live blocks, or came from outside under `unsafe`.
                unsafe { raw::copy(source.addr, target.addr, bytes) };
                windows_in(self.plain, span)?;
                Ok(Value::Void)
            }
        }
    }

    fn int(&mut self, expr: &'a Expr, frame: &mut Frame) -> Res<i128> {
        match strip(self.expr(expr, frame)?) {
            Value::Int(_, value) => Ok(value),
            _ => unreachable!("the checker makes an index an integer"),
        }
    }

    fn values(&mut self, args: &'a [Expr], frame: &mut Frame) -> Res<Vec<Value>> {
        let mut values = Vec::new();
        for arg in args {
            values.push(self.expr(arg, frame)?);
        }
        Ok(values)
    }

    fn call_expr(&mut self, expr: &'a Expr, name: &str, args: &'a [Expr], frame: &mut Frame) -> Res<Value> {
        match name {
            "print" => {
                let value = self.expr(&args[0], frame)?;
                let text = self.show(&value, false, args[0].span)?;
                self.write_line(&text)?;
                Ok(Value::Void)
            }
            "printError" => {
                let value = self.expr(&args[0], frame)?;
                let text = self.show(&value, false, args[0].span)?;
                // What was printed before comes first, as it does on a terminal. Nothing can
                // be told when the standard error cannot be written, so that is not an error.
                if self.in_test {
                    let mut written = self.io.test_err.borrow_mut();
                    written.push_str(&text);
                    written.push('\n');
                    return Ok(Value::Void);
                }
                self.flush_out()?;
                let _ = writeln!(self.io.world.borrow_mut().err, "{text}");
                Ok(Value::Void)
            }
            "panic" => {
                let text = strip(self.expr(&args[0], frame)?).format();
                Err(Diag::panic(expr.span, text).into())
            }
            "exit" => {
                let code = self.int(&args[0], frame)? as u8;
                if self.in_test {
                    return Err(Diag::panic(expr.span, format!("`exit({code})` in a test: a test cannot end the program")).into());
                }
                ending();
                Err(Unwind::Stop(Stop::Exit(code)))
            }
            "assert" => self.assert(expr, &args[0], frame),
            "same" => match (strip(self.expr(&args[0], frame)?), strip(self.expr(&args[1], frame)?)) {
                (Value::Shared(a), Value::Shared(b)) => Ok(Value::Bool(Rc::ptr_eq(&a, &b))),
                _ => unreachable!("the checker lets `same` take only two `shared` values"),
            },
            _ => {
                let func = self.funcs[name];
                let values = self.values(args, frame)?;
                match func.is_async {
                    true => self.start_async(func, None, values, expr.span),
                    false => self.call(func, None, values, expr.span),
                }
            }
        }
    }

    /// On failure writes the expression text and, for a comparison, the value of both sides.
    fn assert(&mut self, call: &'a Expr, cond: &'a Expr, frame: &mut Frame) -> Res<Value> {
        let text = &self.src[cond.span.start..cond.span.end];
        let failed = |sides: Option<(String, String)>| {
            let mut diag = Diag::panic(call.span, format!("assert failed: {text}"));
            if let Some((left, right)) = sides {
                diag = diag.note(format!("left: {left}")).note(format!("right: {right}"));
            }
            Unwind::from(diag)
        };
        match &cond.kind {
            ExprKind::Binary(op, left, right) if op.is_comparison() => {
                let left = strip(self.expr(left, frame)?);
                let right = strip(self.expr(right, frame)?);
                match self.binary(*op, left.clone(), right.clone(), cond.span)? {
                    Value::Bool(true) => Ok(Value::Void),
                    _ => {
                        // A struct or an enum is written the way `print` would write it.
                        let mut shown = Vec::new();
                        for side in [&left, &right] {
                            shown.push(match owned(side.clone()) {
                                Value::Struct(..) | Value::Enum(..) | Value::List(_) | Value::Set(_) | Value::Map(_) | Value::Array(_) | Value::Slice(_) => self.show(side, true, cond.span)?,
                                _ => side.debug(),
                            });
                        }
                        let right = shown.pop().expect("two sides");
                        Err(failed(Some((shown.pop().expect("two sides"), right))))
                    }
                }
            }
            _ if self.condition(cond, frame)? => Ok(Value::Void),
            _ => Err(failed(None)),
        }
    }

    /// How a value is written as text. A struct with a `format` method of its own writes
    /// itself; any other struct or enum is written field by field. `inside`: the value is
    /// part of another one, where text is quoted to keep the parts apart.
    fn show(&mut self, value: &Value, inside: bool, span: Span) -> Res<String> {
        let list = |interp: &mut Self, fields: &[Cell]| -> Res<Vec<String>> {
            let mut parts = Vec::new();
            for field in fields {
                let value = field.borrow().clone();
                parts.push(interp.show(&value, true, span)?);
            }
            Ok(parts)
        };
        match value {
            Value::Ptr(inner) | Value::Borrow(inner) => {
                let value = inner.borrow().clone();
                self.show(&value, inside, span)
            }
            Value::Shared(held) => {
                let value = held.value.borrow().clone();
                self.show(&value, inside, span)
            }
            Value::Array(_) | Value::Slice(_) => self.show(&as_list(value.clone()), inside, span),
            Value::Str(text) if inside => Ok(format!("{text:?}")),
            Value::Char(value) if inside => Ok(quoted_char(*value)),
            Value::Null => Ok("null".to_string()),
            Value::Void => Ok(String::new()),
            Value::Struct(name, fields) => {
                if let Some(text) = self.own_format(name, value, span)? {
                    return Ok(text);
                }
                let decl = self.structs[&**name];
                let parts = list(self, fields)?;
                let named: Vec<String> = decl.fields.iter().zip(parts).map(|(field, text)| format!("{} = {text}", field.name)).collect();
                // A struct without fields is written by its name alone, as in Rust.
                Ok(match named.is_empty() {
                    true => short_name(name).to_string(),
                    false => format!("{} {{ {} }}", short_name(name), named.join(", ")),
                })
            }
            Value::Enum(name, index, fields) => {
                if let Some(text) = self.own_format(name, value, span)? {
                    return Ok(text);
                }
                let word = match &**name {
                    "Result" => ["Ok", "Error"][*index],
                    _ => self.enums[&**name].variants[*index].name.as_str(),
                };
                let parts = list(self, fields)?;
                Ok(match parts.is_empty() {
                    true => word.to_string(),
                    false => format!("{word}({})", parts.join(", ")),
                })
            }
            // `[1, 2]`, `{"x": 10}`, `{1, 2}`
            Value::List(items) => {
                let items: Vec<Cell> = items.borrow().clone();
                Ok(format!("[{}]", list(self, &items)?.join(", ")))
            }
            Value::Set(items) => {
                let items: Vec<Cell> = items.borrow().values().cloned().map(cell).collect();
                Ok(format!("{{{}}}", list(self, &items)?.join(", ")))
            }
            Value::Map(entries) => {
                let entries: Vec<(Value, Cell)> = entries.borrow().values().cloned().collect();
                let mut parts = Vec::new();
                for (key, value) in entries {
                    let value = value.borrow().clone();
                    parts.push(format!("{}: {}", self.show(&key, true, span)?, self.show(&value, true, span)?));
                }
                Ok(format!("{{{}}}", parts.join(", ")))
            }
            Value::Formatter(_) => unreachable!("the checker does not let a `Formatter` be written"),
            other => Ok(other.format()),
        }
    }

    /// An operator on two values. On a struct or an enum `==` is `equals` and `<` is `compare`.
    fn binary(&mut self, op: BinOp, left: Value, right: Value, span: Span) -> Res<Value> {
        use std::cmp::Ordering;
        let (left, right) = (as_list(owned(left)), as_list(owned(right)));
        let by_method = |value: &Value| matches!(value, Value::Struct(..) | Value::Enum(..) | Value::List(_) | Value::Set(_) | Value::Map(_));
        if !by_method(&left) {
            return Ok(binary(op, left, right, span)?);
        }
        Ok(Value::Bool(match op {
            BinOp::Eq => self.equal(&left, &right, span)?,
            BinOp::Ne => !self.equal(&left, &right, span)?,
            _ => {
                let ordering = self.compare(&left, &right, span)?;
                match op {
                    BinOp::Lt => ordering == Ordering::Less,
                    BinOp::Le => ordering != Ordering::Greater,
                    BinOp::Gt => ordering == Ordering::Greater,
                    _ => ordering != Ordering::Less,
                }
            }
        }))
    }

    /// The text a struct or an enum writes with its own `format`, if it wrote one.
    fn own_format(&mut self, name: &str, value: &Value, span: Span) -> Res<Option<String>> {
        let Some(format) = self.methods.get(name).and_then(|methods| methods.get("format")).copied() else { return Ok(None) };
        let written = Rc::new(RefCell::new(String::new()));
        let this = Value::Borrow(cell(value.clone()));
        let formatter = Value::Borrow(cell(Value::Formatter(written.clone())));
        self.call(format, Some(this), vec![formatter], span)?;
        let text = written.borrow().clone();
        Ok(Some(text))
    }

    /// The method of a struct or an enum that `==`, `<`, `indexOf` or `sort` runs, if the
    /// type wrote its own: called with both values lent.
    fn compared(&mut self, method: &str, left: &Value, right: &Value, span: Span) -> Res<Option<Value>> {
        let (Value::Struct(name, _) | Value::Enum(name, ..)) = left else { return Ok(None) };
        let Some(func) = self.methods.get(&**name).and_then(|methods| methods.get(method)).copied() else { return Ok(None) };
        let this = Value::Borrow(cell(left.clone()));
        let other = Value::Borrow(cell(right.clone()));
        self.call(func, Some(this), vec![other], span).map(Some)
    }

    /// `a.equals(b)`: the struct's own `equals`, or field by field.
    fn equal(&mut self, left: &Value, right: &Value, span: Span) -> Res<bool> {
        let (left, right) = (as_list(owned(left.clone())), as_list(owned(right.clone())));
        if let Some(result) = self.compared("equals", &left, &right, span)? {
            return Ok(matches!(result, Value::Bool(true)));
        }
        // Collections: a list item by item in order; a set and a map by what they hold,
        // whatever the order.
        let pairs: Vec<(Cell, Cell)> = match (&left, &right) {
            (Value::List(a), Value::List(b)) => {
                let (a, b) = (a.borrow(), b.borrow());
                if a.len() != b.len() {
                    return Ok(false);
                }
                a.iter().cloned().zip(b.iter().cloned()).collect()
            }
            (Value::Set(a), Value::Set(b)) => {
                let (a, b) = (a.borrow(), b.borrow());
                return Ok(a.len() == b.len() && a.keys().all(|key| b.contains_key(key)));
            }
            (Value::Map(a), Value::Map(b)) => {
                let (a, b) = (a.borrow(), b.borrow());
                if a.len() != b.len() || !a.keys().all(|key| b.contains_key(key)) {
                    return Ok(false);
                }
                a.keys().zip(a.values()).map(|(key, (_, value))| (value.clone(), b.get(key).expect("just tested").1.clone())).collect()
            }
            _ => Vec::new(),
        };
        if matches!(left, Value::List(_) | Value::Map(_)) {
            for (a, b) in pairs {
                let (a, b) = (a.borrow().clone(), b.borrow().clone());
                if !self.equal(&a, &b, span)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        let (left_fields, right_fields) = match (&left, &right) {
            (Value::Struct(_, a), Value::Struct(_, b)) => (&a.cells, &b.cells),
            (Value::Enum(_, a_index, a), Value::Enum(_, b_index, b)) if a_index == b_index => (&**a, &**b),
            (Value::Enum(..), Value::Enum(..)) => return Ok(false),
            // A `T?` that holds a value on one side only: the value may be one that is no key.
            (Value::Null, Value::Null) => return Ok(true),
            (Value::Null, _) | (_, Value::Null) => return Ok(false),
            _ => return Ok(key_of(&left) == key_of(&right)),
        };
        for (a, b) in left_fields.iter().zip(right_fields.iter()) {
            let (a, b) = (a.borrow().clone(), b.borrow().clone());
            if !self.equal(&a, &b, span)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// `a.compare(b)`: the struct's own `compare`; numbers and text have their order.
    fn compare(&mut self, left: &Value, right: &Value, span: Span) -> Res<std::cmp::Ordering> {
        let (left, right) = (owned(left.clone()), owned(right.clone()));
        match self.compared("compare", &left, &right, span)? {
            Some(Value::Int(_, result)) => Ok(result.cmp(&0)),
            _ => Ok(key_of(&left).cmp(&key_of(&right))),
        }
    }

    /// The cells of the items in order; equal items keep the order they had. The items are
    /// taken out while they are compared, so that a `compare` that fails leaves through here.
    fn sorted(&mut self, mut cells: Vec<Cell>, span: Span) -> Res<Vec<Cell>> {
        let mut failed = None;
        cells.sort_by(|a, b| {
            let (a, b) = (a.borrow().clone(), b.borrow().clone());
            match self.compare(&a, &b, span) {
                Ok(ordering) if failed.is_none() => ordering,
                Ok(_) => std::cmp::Ordering::Equal,
                Err(stop) => {
                    failed.get_or_insert(stop);
                    std::cmp::Ordering::Equal
                }
            }
        });
        match failed {
            Some(stop) => Err(stop),
            None => Ok(cells),
        }
    }

    /// The methods of `List<T>`, `HashMap<K, V>` and `HashSet<T>`.
    fn collection_method(&mut self, recv: Value, name: &str, mut args: Vec<Value>, span: Span) -> Res<Value> {
        match (&recv, name) {
            (Value::List(items), "indexOf" | "contains") => {
                let items: Vec<Cell> = items.borrow().clone();
                let mut found = None;
                for (at, item) in items.iter().enumerate() {
                    let item = item.borrow().clone();
                    if self.equal(&item, &args[0], span)? {
                        found = Some(at);
                        break;
                    }
                }
                Ok(match (name, found) {
                    ("contains", _) => Value::Bool(found.is_some()),
                    (_, Some(at)) => Value::Int(IntTy::Nint, at as i128),
                    (_, None) => Value::Null,
                })
            }
            (Value::List(items), "sort") => {
                let sorted = self.sorted(items.borrow().clone(), span)?;
                *items.borrow_mut() = sorted;
                Ok(Value::Void)
            }
            (Value::Map(entries), "add") => {
                let value = args.remove(1);
                let key = args.remove(0);
                let shown = self.show(&key, true, span)?;
                map_add(&mut entries.borrow_mut(), key, value, shown, span)?;
                Ok(Value::Void)
            }
            _ => Ok(collection_method(recv, name, args)),
        }
    }

    fn flush_out(&mut self) -> Res<()> {
        self.io.out.borrow_mut().flush().map_err(output_failed)
    }

    /// `File.readText(path)` and the others: each gives `Ok` with what it read, or `Error`
    /// with an `IoError`.
    fn io(&mut self, call: IoCall, args: &'a [Expr], frame: &mut Frame) -> Res<Value> {
        let values = self.values(args, frame)?;
        self.io_values(call, values)
    }

    /// A task that has ended with `value`, to be awaited.
    fn ended_task(&mut self, value: Value, at: Span) -> Value {
        let task = Rc::new(RefCell::new(Task { state: TaskState::Done(Some(value)), started: at, waiters: Vec::new() }));
        self.started.push(task.clone());
        Value::Task(task)
    }

    fn io_values(&mut self, call: IoCall, values: Vec<Value>) -> Res<Value> {
        let mut values = values.into_iter().map(strip);
        let Some(Value::Str(path)) = values.next() else { unreachable!("the checker makes a path text") };
        let path = path.to_string();
        // No file has a name with a zero byte in it; the system would see the name end there.
        if path.contains('\0') {
            return Ok(failed(io_cause(None, Some(crate::sys::NOT_FOUND))));
        }
        // What Windows says differently is said as Linux and macOS say it (`refine`).
        let done = |result: std::io::Result<Value>| match result {
            Ok(value) => ok(value),
            Err(error) => {
                let error = crate::sys::refine(&path, error);
                failed(io_cause(Some(&error), error.raw_os_error()))
            }
        };
        Ok(match call {
            IoCall::ReadText => match std::fs::read(&path) {
                Ok(bytes) => text_read(bytes, 0),
                Err(error) => done(Err(error)),
            },
            IoCall::ReadBytes => done(std::fs::read(&path).map(|bytes| {
                Value::List(Rc::new(RefCell::new(bytes.into_iter().map(|byte| cell(Value::Int(IntTy::Byte, byte as i128))).collect())))
            })),
            IoCall::WriteText | IoCall::AppendText => {
                let Some(Value::Str(text)) = values.next() else { unreachable!("the checker makes the second argument text") };
                done(write_file(&path, text.as_bytes(), call == IoCall::AppendText).map(|()| Value::Void))
            }
            IoCall::WriteBytes => {
                let bytes: Vec<u8> = cells_of(&values.next().expect("bytes")).iter().map(|item| match &*item.borrow() {
                    Value::Int(_, byte) => *byte as u8,
                    _ => unreachable!("the checker makes the items bytes"),
                }).collect();
                done(write_file(&path, &bytes, false).map(|()| Value::Void))
            }
            // A folder opens, but cannot be read: it is told at once, as compiled code does.
            IoCall::OpenText => done(std::fs::File::open(&path).and_then(|file| match file.metadata()?.is_dir() {
                true => Err(std::io::Error::from_raw_os_error(crate::sys::IS_DIR)),
                false => Ok(Value::Reader(Rc::new(RefCell::new(Reader { file: Some(std::io::BufReader::with_capacity(STREAM_BUFFER, file)), net: None, done: 0 })))),
            })),
            IoCall::CreateText | IoCall::OpenAppend => {
                let opened = match call {
                    IoCall::CreateText => std::fs::File::create(&path),
                    _ => std::fs::OpenOptions::new().append(true).create(true).open(&path),
                };
                done(opened.map(|file| Value::Writer(Rc::new(RefCell::new(Writer { sink: Sink::File(Some(file)), buffer: Vec::new(), failed: None })))))
            }
        })
    }

    /// `File.delete(path)`, `File.move(from, to)` and the other calls of the file system and of
    /// the environment beyond reading and writing a file. Each asks the system what compiled code
    /// asks it, so the two give the same answers.
    fn system(&mut self, call: SystemCall, args: &'a [Expr], frame: &mut Frame) -> Res<Value> {
        if call == SystemCall::WaitAsync {
            return self.wait_async(args, frame);
        }
        if matches!(
            call,
            SystemCall::SignalListen | SystemCall::SignalNext | SystemCall::SignalNextAsync | SystemCall::SignalStop | SystemCall::SignalRaise | SystemCall::SignalNumber
        ) {
            return self.signals(call, args, frame);
        }
        if matches!(
            call,
            SystemCall::UdpBind
                | SystemCall::UdpConnect
                | SystemCall::UdpSend
                | SystemCall::UdpSendAsync
                | SystemCall::UdpReceive
                | SystemCall::UdpReceiveAsync
                | SystemCall::SocketName
                | SystemCall::StreamName
                | SystemCall::CloseSocket
                | SystemCall::Lookup
                | SystemCall::LookupAsync
        ) {
            return self.datagrams(call, args, frame);
        }
        let mut values = Vec::new();
        for (at, arg) in args.iter().enumerate() {
            let value = self.expr(arg, frame)?;
            // The text `System.run` keeps the output in is written where it is.
            values.push(if call == SystemCall::Run && at == 10 { value } else { owned(value) });
        }
        let text = |at: usize| match &values[at] {
            Value::Str(text) => text.to_string(),
            _ => unreachable!("the checker makes it text"),
        };
        let first = match values.first() {
            Some(Value::Str(text)) => text.to_string(),
            _ => String::new(),
        };
        let done = |result: std::io::Result<Value>| match result {
            Ok(value) => ok(value),
            Err(error) => {
                let error = crate::sys::refine(&first, error);
                failed(io_cause(Some(&error), error.raw_os_error()))
            }
        };
        let not_found = || failed(io_cause(None, Some(crate::sys::NOT_FOUND)));
        // No file has a zero byte in its name: such a path is not found.
        let paths = match call {
            SystemCall::Current | SystemCall::TempFolder => 0,
            SystemCall::Move | SystemCall::Replace | SystemCall::Copy => 2,
            _ => 1,
        };
        if call == SystemCall::Executable {
            return Ok(Value::Int(IntTy::Long, executable(&text(0))));
        }
        if call == SystemCall::Run {
            return self.run_child(values);
        }
        let number = |at: usize| match &values[at] {
            Value::Int(_, number) => *number,
            _ => unreachable!("the checker makes it a number"),
        };
        match call {
            SystemCall::Start => return self.start_child(values),
            SystemCall::Wait => {
                let ended = self.reap(number(0) as i64);
                for (slot, value) in cells_of(&values[1]).iter().zip([ended.0, ended.1]) {
                    *slot.borrow_mut() = Value::Int(IntTy::Long, i128::from(value));
                }
                return Ok(Value::Void);
            }
            SystemCall::Signal => {
                // The program is not taken away before it is waited for, so the number is still
                // its own.
                crate::sys::signal(number(0) as i64, number(1) as i64);
                return Ok(Value::Void);
            }
            SystemCall::PipeReader => {
                // SAFETY: the end of the pipe was made by `System.start` and is owned by nothing else.
                let file = unsafe { crate::sys::file_of_fd(number(0) as crate::sys::RawFd) };
                return Ok(Value::Reader(Rc::new(RefCell::new(Reader { file: Some(std::io::BufReader::with_capacity(STREAM_BUFFER, file)), net: None, done: 0 }))));
            }
            SystemCall::PipeWriter => {
                // SAFETY: as above.
                let file = unsafe { crate::sys::file_of_fd(number(0) as crate::sys::RawFd) };
                return Ok(Value::Writer(Rc::new(RefCell::new(Writer { sink: Sink::File(Some(file)), buffer: Vec::new(), failed: None }))));
            }
            _ => {}
        }
        if call != SystemCall::Variable && (0..paths).any(|at| text(at).contains('\0')) {
            return Ok(not_found());
        }
        Ok(match call {
            SystemCall::Delete => done(remove_file(&text(0)).map(|()| Value::Void)),
            SystemCall::Move => done(rename_new(&text(0), &text(1)).map(|()| Value::Void)),
            SystemCall::Replace => done(std::fs::rename(text(0), text(1)).map(|()| Value::Void)),
            SystemCall::Copy => done(copy_new(&text(0), &text(1)).map(|()| Value::Void)),
            SystemCall::CreateDirectory => done(std::fs::create_dir(text(0)).map(|()| Value::Void)),
            SystemCall::RemoveDirectory => done(std::fs::remove_dir(text(0)).map(|()| Value::Void)),
            SystemCall::RemoveAll => done(remove_all(&text(0)).map(|()| Value::Void)),
            SystemCall::List => {
                let mut found = Vec::new();
                let listed = std::fs::read_dir(text(0)).and_then(|entries| {
                    for entry in entries {
                        let entry = entry?;
                        let kind = entry.file_type().map(|kind| kind_code(&kind)).unwrap_or(3);
                        found.push((entry.file_name().as_encoded_bytes().to_vec(), kind));
                    }
                    Ok(())
                });
                if let Err(error) = listed {
                    return Ok(done(Err(error)));
                }
                found.sort();
                let mut names = Vec::new();
                for (name, _) in &found {
                    match std::str::from_utf8(name) {
                        Ok(name) => names.push(cell(Value::Str(Rc::new(name.to_string())))),
                        Err(error) => return Ok(failed(io_cause(None, None).invalid_text(error.valid_up_to() as u64))),
                    }
                }
                let (Value::List(into), Value::List(kinds)) = (&values[1], &values[2]) else { unreachable!("the checker makes them lists") };
                into.borrow_mut().extend(names);
                kinds.borrow_mut().extend(found.iter().map(|(_, kind)| cell(Value::Int(IntTy::Byte, i128::from(*kind)))));
                ok(Value::Void)
            }
            SystemCall::Status => match std::fs::metadata(text(0)) {
                Ok(info) => {
                    let modified = info.modified().map(|time| match time.duration_since(std::time::UNIX_EPOCH) {
                        Ok(after) => i128::try_from(after.as_nanos()).unwrap_or(i128::MAX),
                        Err(before) => -i128::try_from(before.duration().as_nanos()).unwrap_or(i128::MAX),
                    });
                    let modified = modified.unwrap_or(0).clamp(i128::from(i64::MIN), i128::from(i64::MAX));
                    let fields = cells_of(&values[1]);
                    let size = i128::from(info.len()).min(i128::from(i64::MAX));
                    for (slot, value) in fields.iter().zip([i128::from(kind_code(&info.file_type())), size, modified]) {
                        *slot.borrow_mut() = Value::Int(IntTy::Long, value);
                    }
                    ok(Value::Void)
                }
                Err(error) => done(Err(error)),
            },
            SystemCall::Current => match std::env::current_dir() {
                Ok(folder) => text_read(folder.as_os_str().as_encoded_bytes().to_vec(), 0),
                Err(error) => done(Err(error)),
            },
            SystemCall::MakeTemp => done(make_temp(&text(0)).map(|folder| Value::Str(Rc::new(folder)))),
            SystemCall::TempFolder => ok(Value::Str(Rc::new(crate::sys::temp_folder()))),
            SystemCall::Executable
            | SystemCall::Run
            | SystemCall::Start
            | SystemCall::Wait
            | SystemCall::WaitAsync
            | SystemCall::Signal
            | SystemCall::PipeReader
            | SystemCall::PipeWriter
            | SystemCall::UdpBind
            | SystemCall::UdpConnect
            | SystemCall::UdpSend
            | SystemCall::UdpSendAsync
            | SystemCall::UdpReceive
            | SystemCall::UdpReceiveAsync
            | SystemCall::SocketName
            | SystemCall::StreamName
            | SystemCall::CloseSocket
            | SystemCall::Lookup
            | SystemCall::LookupAsync
            | SystemCall::SignalListen
            | SystemCall::SignalNext
            | SystemCall::SignalNextAsync
            | SystemCall::SignalStop
            | SystemCall::SignalRaise
            | SystemCall::SignalNumber => unreachable!("done above"),
            SystemCall::Variable => {
                let name = text(0);
                // A name with `=` or a zero byte in it is no variable's name.
                let found = match name.is_empty() || name.contains(['=', '\0']) {
                    true => None,
                    false => std::env::var_os(&name),
                };
                match found {
                    Some(value) => {
                        let Value::List(into) = &values[1] else { unreachable!("the checker makes it a list") };
                        let bytes = value.as_encoded_bytes();
                        into.borrow_mut().extend(bytes.iter().map(|byte| cell(Value::Int(IntTy::Byte, i128::from(*byte)))));
                        Value::Bool(true)
                    }
                    None => Value::Bool(false),
                }
            }
        })
    }

    /// `System.start`: starts the program and goes on. Its input is the run's, nothing, the text
    /// (written by a thread of its own) or a pipe whose end the program keeps; its output and its
    /// standard error are pipes it keeps when asked for, and otherwise pipes read by threads and
    /// written out to the run's when it is waited for (see `Kid`).
    fn start_child(&mut self, values: Vec<Value>) -> Res<Value> {
        use std::io::Read;
        use std::process::Stdio;
        let (mut command, input) = command_of(&values);
        let flag = |at: usize| matches!(values[at], Value::Bool(true));
        let (out, err) = (flag(9), flag(10));
        command.stdin(match input {
            0 => Stdio::inherit(),
            1 => Stdio::null(),
            _ => Stdio::piped(),
        });
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        self.flush_out()?;
        let _ = self.io.world.borrow_mut().err.flush();
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let error = crate::sys::not_started(error, &values[2]);
                return Ok(failed(io_cause(Some(&error), error.raw_os_error())));
            }
        };
        // The number, the three ends kept here, and what names the program to the calls of the
        // system: the number again, which the interpreter keeps its children by on every system.
        let mut made = [i128::from(child.id()), -1, -1, -1, i128::from(child.id())];
        if let Some(feed) = child.stdin.take() {
            match input {
                2 => {
                    let Value::Str(text) = &values[8] else { unreachable!("the checker makes it text") };
                    let text = text.to_string();
                    let mut feed = feed;
                    // A program that ends before it reads all of it is no error.
                    std::thread::spawn(move || {
                        let _ = feed.write_all(text.as_bytes());
                    });
                }
                _ => made[1] = i128::from(crate::sys::fd_of(feed)),
            }
        }
        let mut copies = Vec::new();
        let output = child.stdout.take().expect("piped");
        match out {
            true => made[2] = i128::from(crate::sys::fd_of(output)),
            false => {
                let mut output = output;
                copies.push((std::thread::spawn(move || {
                    let mut kept = Vec::new();
                    let _ = output.read_to_end(&mut kept);
                    kept
                }), false));
            }
        }
        let error = child.stderr.take().expect("piped");
        match err {
            true => made[3] = i128::from(crate::sys::fd_of(error)),
            false => {
                let mut error = error;
                copies.push((std::thread::spawn(move || {
                    let mut kept = Vec::new();
                    let _ = error.read_to_end(&mut kept);
                    kept
                }), true));
            }
        }
        self.io.children.borrow_mut().insert(made[0] as i64, Kid { child, copies });
        for (slot, value) in cells_of(&values[11]).iter().zip(made) {
            *slot.borrow_mut() = Value::Int(IntTy::Long, value);
        }
        Ok(ok(Value::Void))
    }

    /// `System.wait`: waits for the program and takes it away; what it wrote to an output it
    /// shares with the run goes out there now. 0 and the code, or 1 and the signal.
    fn reap(&mut self, pid: i64) -> (i64, i64) {
        let Some(mut kid) = self.io.children.borrow_mut().remove(&pid) else { return (0, 0) };
        let ended = kid.child.wait().map_or((0, 0), ended_as);
        for (copy, err) in kid.copies {
            let bytes = copy.join().unwrap_or_default();
            if bytes.is_empty() {
                continue;
            }
            match err {
                true => {
                    let mut world = self.io.world.borrow_mut();
                    let _ = world.err.write_all(&bytes).and_then(|()| world.err.flush());
                }
                false => {
                    let mut out = self.io.out.borrow_mut();
                    let _ = out.write_all(&bytes).and_then(|()| out.flush());
                }
            }
        }
        ended
    }

    /// `await System.waitAsync(pid, read token, ref ended)`: a thread waits for the end of the
    /// program without taking it away while this fiber gives the turn away; a cancel ends the
    /// wait, and leaves the program to whoever stops it.
    fn wait_async(&mut self, args: &'a [Expr], frame: &mut Frame) -> Res<Value> {
        let at = args[0].span;
        let Value::Int(_, pid) = strip(self.expr(&args[0], frame)?) else { unreachable!("the checker makes it a number") };
        let source = self.token(&args[1], frame)?;
        let ended = owned(self.expr(&args[2], frame)?);
        let value = 'waited: {
            if source.as_ref().is_some_and(|source| source.borrow().cancelled) {
                break 'waited failed(io_cancelled());
            }
            let (id, done) = self.work(at)?;
            let pid = pid as i64;
            std::thread::spawn(move || {
                wait_unreaped(pid);
                done.tell(id);
            });
            if self.wait_for(Outside::Work(id), source.as_ref(), at)? {
                break 'waited failed(io_cancelled());
            }
            let (kind, number) = self.reap(pid);
            for (slot, value) in cells_of(&ended).iter().zip([kind, number]) {
                *slot.borrow_mut() = Value::Int(IntTy::Long, i128::from(value));
            }
            ok(Value::Void)
        };
        Ok(self.ended_task(value, at))
    }

    /// The calls of the system under `Signals` and `SignalListener` (see `signals.rs`). A wait for
    /// a signal is a piece of work the relay of the signals tells the scheduler of, so the other
    /// tasks run meanwhile, with `next` as with `nextAsync`; a cancel ends the wait, and a signal
    /// that comes later stays for the next call.
    fn signals(&mut self, call: SystemCall, args: &'a [Expr], frame: &mut Frame) -> Res<Value> {
        let at = args[0].span;
        let Value::Int(_, number) = strip(self.expr(&args[0], frame)?) else { unreachable!("the checker makes it a number") };
        match call {
            SystemCall::SignalListen => match crate::signals::listen(number as u32) {
                Ok(id) => Ok(Value::Int(IntTy::Long, i128::from(id))),
                Err(error) => Err(Diag::panic(at, format!("the system refused to listen for signals: {error}")).into()),
            },
            SystemCall::SignalStop => {
                crate::signals::stop(number as u64);
                Ok(Value::Void)
            }
            SystemCall::SignalRaise => {
                // What was printed goes out first: a signal that ends the program does not lose it.
                self.flush_out()?;
                crate::signals::raise_kind(number as usize);
                Ok(Value::Void)
            }
            SystemCall::SignalNumber => Ok(Value::Int(IntTy::Int, i128::from(crate::signals::number(number as usize)))),
            _ => {
                let id = number as u64;
                let source = match call {
                    SystemCall::SignalNextAsync => self.token(&args[1], frame)?,
                    _ => None,
                };
                let kind = loop {
                    if source.as_ref().is_some_and(|source| source.borrow().cancelled) {
                        break None;
                    }
                    let (work, done) = self.work(at)?;
                    if let Some(kind) = crate::signals::take_or_wait(id, work, done) {
                        break Some(kind);
                    }
                    if self.wait_for(Outside::Work(work), source.as_ref(), at)? {
                        crate::signals::forget_waiter(id);
                        break None;
                    }
                };
                if call == SystemCall::SignalNext {
                    return Ok(Value::Int(IntTy::Long, i128::from(kind.expect("only a token cancels"))));
                }
                let value = match kind {
                    Some(kind) => {
                        let into = owned(self.expr(&args[2], frame)?);
                        if let Some(slot) = cells_of(&into).first() {
                            *slot.borrow_mut() = Value::Int(IntTy::Long, i128::from(kind));
                        }
                        ok(Value::Void)
                    }
                    None => failed(cancelled()),
                };
                Ok(self.ended_task(value, at))
            }
        }
    }

    /// The calls of the system under `UdpSocket` and `SocketAddress`. A socket is the number of a
    /// socket of the system, made by `System.udpBind` and closed by `System.closeSocket`; it does not
    /// block, so a call whose socket is not ready gives the turn away until it is, as a `TcpStream`
    /// does, and a token is looked at before the work. A datagram is received into a block one byte
    /// longer than the buffer: one that fills it did not fit. An async call gives a task that has
    /// ended, which the module awaits where it is written.
    fn datagrams(&mut self, call: SystemCall, args: &'a [Expr], frame: &mut Frame) -> Res<Value> {
        let at = args[0].span;
        let is_async = matches!(call, SystemCall::UdpSendAsync | SystemCall::UdpReceiveAsync | SystemCall::LookupAsync);
        let given = if is_async { &args[..args.len() - 1] } else { args };
        let mut values = Vec::new();
        for arg in given {
            values.push(owned(self.expr(arg, frame)?));
        }
        let source = match is_async {
            true => self.token(&args[args.len() - 1], frame)?,
            false => None,
        };
        let number = |at: usize| match &values[at] {
            Value::Int(_, number) => *number,
            _ => unreachable!("the checker makes it a number"),
        };
        let flag = |at: usize| matches!(values[at], Value::Bool(true));
        let address = |at: usize| crate::net::address_of(flag(at), number(at + 1) as u64, number(at + 2) as u64, number(at + 3) as u16);
        let io_failed = |error: std::io::Error| {
            let error = crate::net::udp_error(error);
            failed(io_cause(Some(&error), error.raw_os_error()))
        };
        let put = |into: &Value, numbers: &[u64]| {
            for (slot, value) in cells_of(into).iter().zip(numbers) {
                *slot.borrow_mut() = Value::Int(IntTy::Ulong, i128::from(*value));
            }
        };
        let fd = || number(0) as crate::sys::RawFd;
        // SAFETY: the number is that of a socket `System.udpBind` made, which the `UdpSocket` of the
        // program owns and closes once, with `System.closeSocket`; this one is never closed.
        let socket = || std::mem::ManuallyDrop::new(unsafe { crate::sys::udp_of_fd(fd()) });
        let cancelled = source.as_ref().is_some_and(|source| source.borrow().cancelled);
        let value = match call {
            SystemCall::UdpBind => match crate::net::udp_bind(address(0)) {
                Ok(socket) => ok(Value::Int(IntTy::Long, i128::from(crate::sys::udp_fd(socket)))),
                Err(error) => io_failed(error),
            },
            SystemCall::UdpConnect => match socket().connect(address(1)) {
                Ok(()) => {
                    crate::net::udp_connected(&socket());
                    ok(Value::Void)
                }
                Err(error) => io_failed(error),
            },
            SystemCall::UdpSend | SystemCall::UdpSendAsync if cancelled => failed(io_cancelled()),
            SystemCall::UdpSend | SystemCall::UdpSendAsync => {
                let bytes: Vec<u8> = cells_of(&values[1]).iter().map(|item| match &*item.borrow() {
                    Value::Int(_, byte) => *byte as u8,
                    _ => unreachable!("the checker makes the items bytes"),
                }).collect();
                let to = flag(2).then(|| address(3));
                let socket = socket();
                loop {
                    let sent = match to {
                        Some(to) => socket.send_to(&bytes, to),
                        None => socket.send(&bytes),
                    };
                    match sent {
                        Ok(_) => break ok(Value::Void),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if self.wait_for(Outside::Socket(fd(), true), source.as_ref(), at)? {
                                break failed(io_cancelled());
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(error) => break io_failed(error),
                    }
                }
            }
            SystemCall::UdpReceive | SystemCall::UdpReceiveAsync if cancelled => failed(io_cancelled()),
            SystemCall::UdpReceive | SystemCall::UdpReceiveAsync => {
                let cells = cells_of(&values[1]);
                let mut block = vec![0u8; cells.len() + 1];
                let socket = socket();
                loop {
                    match socket.recv_from(&mut block) {
                        Ok((got, _)) if got > cells.len() => break failed(io_cause(None, Some(crate::sys::MESSAGE_TOO_LONG))),
                        Ok((got, from)) => {
                            for (slot, byte) in cells.iter().zip(&block[..got]) {
                                *slot.borrow_mut() = Value::Int(IntTy::Byte, i128::from(*byte));
                            }
                            let [v6, high, low, port] = crate::net::bits_of(from);
                            put(&values[2], &[got as u64, v6, high, low, port]);
                            break ok(Value::Void);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if self.wait_for(Outside::Socket(fd(), false), source.as_ref(), at)? {
                                break failed(io_cancelled());
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(error) => break io_failed(error),
                    }
                }
            }
            SystemCall::SocketName | SystemCall::StreamName => {
                let remote = flag(1);
                let found = match &values[0] {
                    Value::Socket(stream) if remote => stream.peer_addr(),
                    Value::Socket(stream) => stream.local_addr(),
                    _ if remote => socket().peer_addr(),
                    _ => socket().local_addr(),
                };
                match found {
                    Ok(found) => {
                        put(&values[2], &crate::net::bits_of(found));
                        Value::Bool(true)
                    }
                    Err(_) => Value::Bool(false),
                }
            }
            SystemCall::CloseSocket => {
                // SAFETY: as above; the `UdpSocket` of the program is let go of here, once.
                drop(unsafe { crate::sys::udp_of_fd(fd()) });
                Value::Void
            }
            SystemCall::Lookup | SystemCall::LookupAsync if cancelled => failed(io_cancelled()),
            SystemCall::Lookup | SystemCall::LookupAsync => {
                let Value::Str(host) = &values[0] else { unreachable!("the checker makes the host text") };
                let host = host.to_string();
                let found = match is_async {
                    false => Some(crate::net::lookup(&host)),
                    true => {
                        let (id, done) = self.work(at)?;
                        let slot = std::sync::Arc::new(Mutex::new(None));
                        let kept = slot.clone();
                        std::thread::spawn(move || {
                            let found = crate::net::lookup(&host);
                            *kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(found);
                            done.tell(id);
                        });
                        match self.wait_for(Outside::Work(id), source.as_ref(), at)? {
                            true => None,
                            false => slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take(),
                        }
                    }
                };
                match found {
                    None => failed(io_cancelled()),
                    Some(None) => failed(io_host_not_found()),
                    Some(Some(found)) => {
                        let Value::List(into) = &values[1] else { unreachable!("the checker makes it a list") };
                        for ip in found {
                            let [v6, high, low, _] = crate::net::bits_of(std::net::SocketAddr::new(ip, 0));
                            into.borrow_mut().extend([v6, high, low].iter().map(|value| cell(Value::Int(IntTy::Ulong, i128::from(*value)))));
                        }
                        ok(Value::Void)
                    }
                }
            }
            _ => unreachable!("only the calls of datagrams come here"),
        };
        Ok(match is_async {
            true => self.ended_task(value, at),
            false => value,
        })
    }

    /// `System.run`: starts the program at the path and waits for it to end. What was printed
    /// goes out first. The program writes into pipes whose bytes are copied, as they come, to
    /// the output and the standard error of the run (or into the text, for its output when it
    /// is kept), since those need not be the streams of the process; its input is that of the
    /// process, nothing, or the text, written by a thread of its own while the output is read.
    fn run_child(&mut self, values: Vec<Value>) -> Res<Value> {
        use std::io::Read;
        use std::process::Stdio;
        let text = |at: usize| match &values[at] {
            Value::Str(text) => text.to_string(),
            _ => unreachable!("the checker makes it text"),
        };
        let flag = |at: usize| matches!(values[at], Value::Bool(true));
        let (mut command, input) = command_of(&values);
        command.stdin(match input {
            0 => Stdio::inherit(),
            1 => Stdio::null(),
            _ => Stdio::piped(),
        });
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let fed = text(8);
        let capture = flag(9);
        self.flush_out()?;
        let _ = self.io.world.borrow_mut().err.flush();
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let error = crate::sys::not_started(error, &values[2]);
                return Ok(failed(io_cause(Some(&error), error.raw_os_error())));
            }
        };
        let feed = child.stdin.take();
        let mut out_pipe = child.stdout.take().expect("piped");
        let mut err_pipe = child.stderr.take().expect("piped");
        let mut kept_out = Vec::new();
        {
            let mut world = self.io.world.borrow_mut();
            let err: &mut (dyn Write + Send) = &mut *world.err;
            let mut out = self.io.out.borrow_mut();
            std::thread::scope(|scope| {
                if let Some(mut feed) = feed {
                    // A program that ends before it reads all of it is no error.
                    scope.spawn(move || {
                        let _ = feed.write_all(fed.as_bytes());
                    });
                }
                scope.spawn(move || {
                    let mut block = [0u8; 65536];
                    while let Ok(got) = err_pipe.read(&mut block) {
                        if got == 0 || err.write_all(&block[..got]).and_then(|()| err.flush()).is_err() {
                            break;
                        }
                    }
                });
                match capture {
                    true => {
                        let _ = out_pipe.read_to_end(&mut kept_out);
                    }
                    false => {
                        let mut block = [0u8; 65536];
                        while let Ok(got) = out_pipe.read(&mut block) {
                            if got == 0 || out.write_all(&block[..got]).and_then(|()| out.flush()).is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
        let ended = match child.wait() {
            Ok(status) => status,
            Err(error) => return Ok(failed(io_cause(Some(&error), error.raw_os_error()))),
        };
        // What it wrote, kept where it is UTF-8; otherwise where the first byte that is not.
        let mut bad = -1;
        if capture {
            let Value::Borrow(slot) = &values[10] else { unreachable!("the checker makes it a `ref string`") };
            match String::from_utf8(kept_out) {
                Ok(text) => *behind_borrows(slot.clone()).borrow_mut() = Value::Str(Rc::new(text)),
                Err(error) => bad = i64::try_from(error.utf8_error().valid_up_to()).unwrap_or(i64::MAX),
            }
        }
        let (kind, number) = ended_as(ended);
        let fields = cells_of(&values[11]);
        for (slot, value) in fields.iter().zip([kind, number, bad]) {
            *slot.borrow_mut() = Value::Int(IntTy::Long, i128::from(value));
        }
        Ok(ok(Value::Void))
    }

    /// The methods of a `StreamReader` and a `StreamWriter`.
    /// A method of a reader or a writer. `token`: the async method was given one, which is
    /// looked at after the arguments, before the work.
    fn stream(&mut self, call: StreamCall, recv: &'a Expr, args: &'a [Expr], token: Option<&'a Expr>, frame: &mut Frame) -> Res<Value> {
        let stream = owned(self.expr(recv, frame)?);
        if let Value::Reader(reader) = &stream {
            if reader.borrow().net.is_some() && call != StreamCall::Lines {
                let (slot, cells) = match call {
                    StreamCall::ReadLine => match self.expr(&args[0], frame)? {
                        Value::Borrow(slot) => (Some(behind_borrows(slot)), Rc::new(Vec::new())),
                        _ => unreachable!("the checker makes the line a `ref string`"),
                    },
                    StreamCall::ReadBytes | StreamCall::ReadSome => (None, cells_of(&owned(self.expr(&args[0], frame)?))),
                    _ => (None, Rc::new(Vec::new())),
                };
                let source = match token {
                    Some(token) => self.token(token, frame)?,
                    None => None,
                };
                if source.as_ref().is_some_and(|source| source.borrow().cancelled) {
                    return Ok(failed(io_cancelled()));
                }
                return self.net_read(call, reader, slot.as_ref(), &cells, source.as_ref(), recv.span);
            }
        }
        match (call, stream) {
            (StreamCall::ReadLine, Value::Reader(reader)) => {
                let Value::Borrow(slot) = self.expr(&args[0], frame)? else { unreachable!("the checker makes the line a `ref string`") };
                if self.cancelled(token, frame)? {
                    return Ok(failed(io_cancelled()));
                }
                self.read_line(&reader, &behind_borrows(slot), recv.span)
            }
            (StreamCall::ReadText, Value::Reader(reader)) => {
                if self.cancelled(token, frame)? {
                    return Ok(failed(io_cancelled()));
                }
                let mut bytes = Vec::new();
                let start = reader.borrow().done;
                let read = match reader.borrow_mut().file.as_mut() {
                    Some(file) => file.read_to_end(&mut bytes),
                    None => {
                        if self.io.world.borrow().input_is_terminal {
                            self.flush_out()?;
                        }
                        self.io.input.borrow_mut().read_to_end(&mut bytes)
                    }
                };
                reader.borrow_mut().done += bytes.len() as u64;
                Ok(match read {
                    Ok(_) => text_read(bytes, start),
                    Err(error) => failed(io_cause(Some(&error), error.raw_os_error())),
                })
            }
            (StreamCall::Lines, Value::Reader(reader)) => Ok(Value::Lines(Rc::new(RefCell::new(Lines { reader, done: false })))),
            (StreamCall::ReadBytes, Value::Reader(reader)) => {
                let cells = cells_of(&owned(self.expr(&args[0], frame)?));
                if self.cancelled(token, frame)? {
                    return Ok(failed(io_cancelled()));
                }
                let mut bytes = vec![0u8; cells.len()];
                let mut got = 0;
                // As many as the buffer holds: fewer only at the end.
                let read = loop {
                    if got == bytes.len() {
                        break Ok(());
                    }
                    let step = match reader.borrow_mut().file.as_mut() {
                        Some(file) => file.read(&mut bytes[got..]),
                        None => {
                            if self.io.world.borrow().input_is_terminal {
                                self.flush_out()?;
                            }
                            self.io.input.borrow_mut().read(&mut bytes[got..])
                        }
                    };
                    match step {
                        Ok(0) => break Ok(()),
                        Ok(n) => got += n,
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(error) => break Err(error),
                    }
                };
                reader.borrow_mut().done += got as u64;
                for (slot, byte) in cells.iter().zip(&bytes[..got]) {
                    *slot.borrow_mut() = Value::Int(IntTy::Byte, *byte as i128);
                }
                Ok(match read {
                    Ok(()) => ok(Value::Int(IntTy::Nint, got as i128)),
                    Err(error) => failed(io_cause(Some(&error), error.raw_os_error())),
                })
            }
            // What the block holds, or one read of the system when it is empty.
            (StreamCall::ReadSome, Value::Reader(reader)) => {
                let cells = cells_of(&owned(self.expr(&args[0], frame)?));
                if self.cancelled(token, frame)? {
                    return Ok(failed(io_cancelled()));
                }
                let mut bytes = vec![0u8; cells.len()];
                let read = loop {
                    if bytes.is_empty() {
                        break Ok(0);
                    }
                    let step = match reader.borrow_mut().file.as_mut() {
                        Some(file) => file.read(&mut bytes),
                        None => {
                            if self.io.world.borrow().input_is_terminal {
                                self.flush_out()?;
                            }
                            self.io.input.borrow_mut().read(&mut bytes)
                        }
                    };
                    match step {
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        other => break other,
                    }
                };
                Ok(match read {
                    Ok(got) => {
                        reader.borrow_mut().done += got as u64;
                        for (slot, byte) in cells.iter().zip(&bytes[..got]) {
                            *slot.borrow_mut() = Value::Int(IntTy::Byte, *byte as i128);
                        }
                        ok(Value::Int(IntTy::Nint, got as i128))
                    }
                    Err(error) => failed(io_cause(Some(&error), error.raw_os_error())),
                })
            }
            (StreamCall::WriteBytes, Value::Writer(writer)) => {
                let bytes: Vec<u8> = cells_of(&owned(self.expr(&args[0], frame)?)).iter().map(|item| match &*item.borrow() {
                    Value::Int(_, byte) => *byte as u8,
                    _ => unreachable!("the checker makes the items bytes"),
                }).collect();
                if self.cancelled(token, frame)? {
                    return Ok(failed(io_cancelled()));
                }
                self.write_stream(&writer, &bytes, false)
            }
            (StreamCall::Write | StreamCall::WriteLine, Value::Writer(writer)) => {
                let value = self.expr(&args[0], frame)?;
                if self.cancelled(token, frame)? {
                    return Ok(failed(io_cancelled()));
                }
                let mut text = self.show(&value, false, args[0].span)?;
                if call == StreamCall::WriteLine {
                    text.push('\n');
                }
                self.write_stream(&writer, text.as_bytes(), false)
            }
            (StreamCall::Flush, Value::Writer(writer)) => {
                if self.cancelled(token, frame)? {
                    return Ok(failed(io_cancelled()));
                }
                self.write_stream(&writer, &[], true)
            }
            (StreamCall::Close, Value::Writer(writer)) => {
                let result = self.write_stream(&writer, &[], true)?;
                let mut writer = writer.borrow_mut();
                // The buffer is written, or its error is in `result`: nothing is left to do
                // when it ends.
                writer.buffer.clear();
                writer.failed.get_or_insert(Value::Void);
                writer.sink = Sink::File(None);
                Ok(result)
            }
            _ => unreachable!("the checker gives each method its stream"),
        }
    }

    /// A read of the reader of a connection: what has come is received into its block, waiting
    /// for the connection when nothing has, until the block holds all the read needs (a whole
    /// line, the whole text, the bytes asked for, or anything for `readSome`); it is then taken.
    /// A cancel ends a wait, and what came stays in the block. `slot`: the line of `readLine`;
    /// `cells`: the buffer of `readBytes` and `readSome`.
    fn net_read(&mut self, call: StreamCall, reader: &Rc<RefCell<Reader>>, slot: Option<&Cell>, cells: &[Cell], source: Option<&Rc<RefCell<Cancel>>>, at: Span) -> Res<Value> {
        let put = |line: &str| {
            if let Some(slot) = slot {
                let mut current = slot.borrow_mut();
                let Value::Str(text) = &mut *current else { unreachable!("the checker makes the line text") };
                let text = Rc::make_mut(text);
                text.clear();
                text.push_str(line);
            }
        };
        let taken = loop {
            let ready = reader.borrow().net.as_ref().expect("a reader of a connection").ready(call, cells.len());
            match ready {
                Some(taken) => break taken,
                None => match self.net_fill(reader, source, at)? {
                    Ok(false) => {}
                    Ok(true) => return Ok(failed(io_cancelled())),
                    Err(error) => {
                        put("");
                        return Ok(failed(io_cause(Some(&error), error.raw_os_error())));
                    }
                },
            }
        };
        let mut held = reader.borrow_mut();
        let start = held.done;
        let net = held.net.as_mut().expect("a reader of a connection");
        let Some(count) = taken else {
            net.too_long = true;
            put("");
            return Ok(failed(io_line_too_long()));
        };
        let mut bytes = net.block[net.at..net.at + count].to_vec();
        net.at += count;
        held.done += count as u64;
        Ok(match call {
            StreamCall::ReadLine if bytes.is_empty() => {
                put("");
                ok(Value::Bool(false))
            }
            StreamCall::ReadLine => {
                if bytes.last() == Some(&b'\n') {
                    bytes.pop();
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                }
                match std::str::from_utf8(&bytes) {
                    Ok(line) => {
                        put(line);
                        ok(Value::Bool(true))
                    }
                    Err(error) => {
                        put("");
                        failed(io_cause(None, None).invalid_text(start + error.valid_up_to() as u64))
                    }
                }
            }
            StreamCall::ReadText => text_read(bytes, start),
            _ => {
                for (slot, byte) in cells.iter().zip(&bytes) {
                    *slot.borrow_mut() = Value::Int(IntTy::Byte, *byte as i128);
                }
                ok(Value::Int(IntTy::Nint, count as i128))
            }
        })
    }

    /// Receives what has come on the connection of a reader into its block, waiting for the
    /// socket when nothing has: `Ok(true)` when a cancel cut the wait short.
    fn net_fill(&mut self, reader: &Rc<RefCell<Reader>>, source: Option<&Rc<RefCell<Cancel>>>, at: Span) -> Res<Result<bool, std::io::Error>> {
        let stream = reader.borrow().net.as_ref().expect("a reader of a connection").stream.clone();
        let fd = crate::net::fd_of(&*stream);
        let mut bytes = vec![0u8; STREAM_BUFFER];
        loop {
            match (&*stream).read(&mut bytes) {
                Ok(got) => {
                    let mut held = reader.borrow_mut();
                    let net = held.net.as_mut().expect("a reader of a connection");
                    net.ended |= got == 0;
                    net.block.drain(..net.at);
                    net.at = 0;
                    net.block.extend_from_slice(&bytes[..got]);
                    return Ok(Ok(false));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if self.wait_for(Outside::Socket(fd, false), source, at)? {
                        return Ok(Ok(true));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Ok(Err(error)),
            }
        }
    }

    /// The next line into `slot`, without its `\n` or `\r\n`: `Ok(true)`, `Ok(false)` at the
    /// end, or `Error`. A line that is not text is used up, and `slot` is left empty.
    fn read_line(&mut self, reader: &Rc<RefCell<Reader>>, slot: &Cell, at: Span) -> Res<Value> {
        if reader.borrow().net.is_some() {
            return self.net_read(StreamCall::ReadLine, reader, Some(slot), &[], None, at);
        }
        let mut bytes = Vec::new();
        let read = match reader.borrow_mut().file.as_mut() {
            Some(file) => file.read_until(b'\n', &mut bytes),
            None => {
                if self.io.world.borrow().input_is_terminal {
                    self.flush_out()?;
                }
                self.io.input.borrow_mut().read_until(b'\n', &mut bytes)
            }
        };
        let start = reader.borrow().done;
        reader.borrow_mut().done += bytes.len() as u64;
        // The line goes into the text that is there, which keeps its room.
        let put = |line: &str| {
            let mut current = slot.borrow_mut();
            let Value::Str(text) = &mut *current else { unreachable!("the checker makes the line text") };
            let text = Rc::make_mut(text);
            text.clear();
            text.push_str(line);
        };
        Ok(match read {
            Err(error) => {
                put("");
                failed(io_cause(Some(&error), error.raw_os_error()))
            }
            Ok(0) => {
                put("");
                ok(Value::Bool(false))
            }
            Ok(_) => {
                if bytes.last() == Some(&b'\n') {
                    bytes.pop();
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                }
                match std::str::from_utf8(&bytes) {
                    Ok(line) => {
                        put(line);
                        ok(Value::Bool(true))
                    }
                    Err(error) => {
                        put("");
                        failed(io_cause(None, None).invalid_text(start + error.valid_up_to() as u64))
                    }
                }
            }
        })
    }

    /// What `reader.lines()` gives next: a line of its own, or the error that ends it.
    fn next_line(&mut self, lines: &Rc<RefCell<Lines>>, at: Span) -> Res<Option<Value>> {
        if lines.borrow().done {
            return Ok(None);
        }
        let reader = lines.borrow().reader.clone();
        let line = cell(Value::Str(Rc::new(String::new())));
        let result = self.read_line(&reader, &line, at)?;
        Ok(match result {
            Value::Enum(_, 0, fields) if matches!(*fields[0].borrow(), Value::Bool(true)) => Some(ok(line.borrow().clone())),
            Value::Enum(_, 0, _) => {
                lines.borrow_mut().done = true;
                None
            }
            error => {
                lines.borrow_mut().done = true;
                Some(error)
            }
        })
    }

    /// Writes `bytes` through a writer; `flush`: what its buffer holds goes out too. Gives
    /// `Ok()`, or the first error the writer met.
    fn write_stream(&mut self, writer: &Rc<RefCell<Writer>>, bytes: &[u8], flush: bool) -> Res<Value> {
        let mut writer = writer.borrow_mut();
        if let Some(cause) = &writer.failed {
            return Ok(failed(cause.clone()));
        }
        let writer = &mut *writer;
        let result = match &mut writer.sink {
            Sink::Stdout => {
                let mut out = self.io.out.borrow_mut();
                match out.write_all(bytes).and_then(|()| if flush { out.flush() } else { Ok(()) }) {
                    Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => return Err(output_failed(error)),
                    other => other,
                }
            }
            // As `printError`: in a test it is kept for the `FAIL` line.
            Sink::Stderr if self.in_test => {
                self.io.test_err.borrow_mut().push_str(&String::from_utf8_lossy(bytes));
                Ok(())
            }
            Sink::Stderr => {
                self.flush_out()?;
                let mut world = self.io.world.borrow_mut();
                world.err.write_all(bytes).and_then(|()| if flush { world.err.flush() } else { Ok(()) })
            }
            Sink::File(file) => {
                writer.buffer.extend_from_slice(bytes);
                match flush || writer.buffer.len() >= STREAM_BUFFER {
                    true => {
                        let file = file.as_mut().expect("a closed writer is moved");
                        let result = file.write_all(&writer.buffer);
                        writer.buffer.clear();
                        result
                    }
                    false => Ok(()),
                }
            }
        };
        Ok(match result {
            Ok(()) => ok(Value::Void),
            Err(error) => {
                let cause = io_cause(Some(&error), error.raw_os_error());
                writer.failed = Some(cause.clone());
                failed(cause)
            }
        })
    }

    fn write_line(&mut self, text: &str) -> Res<()> {
        writeln!(self.io.out.borrow_mut(), "{text}").map_err(output_failed)
    }
}

/// Writing where `print` writes failed: the program stops.
fn output_failed(error: std::io::Error) -> Unwind {
    ending();
    match error.kind() {
        std::io::ErrorKind::BrokenPipe => Unwind::Stop(Stop::OutputClosed),
        _ => Unwind::Stop(Stop::OutputFailed(error.to_string())),
    }
}

fn binary(op: BinOp, left: Value, right: Value, span: Span) -> Result<Value, Diag> {
    use std::cmp::Ordering;

    let ordering: Ordering = match (&left, &right) {
        (Value::Int(ty, a), Value::Int(_, b)) if !op.is_comparison() => return arithmetic(op, *ty, *a, *b, span),
        (Value::Int(_, a), Value::Int(_, b)) => a.cmp(b),
        (Value::Float(ty, a), Value::Float(_, b)) if !op.is_comparison() => return Ok(Value::Float(*ty, fractional(op, *ty, *a, *b))),
        // As the hardware compares: nothing is equal to `NaN`, nor before or after it.
        (Value::Float(_, a), Value::Float(_, b)) => match a.partial_cmp(b) {
            Some(ordering) => ordering,
            None => return Ok(Value::Bool(op == BinOp::Ne)),
        },
        (Value::Str(a), Value::Str(b)) => a.cmp(b),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Char(a), Value::Char(b)) => a.cmp(b),
        _ => unreachable!("the checker rules out other operand types"),
    };
    Ok(Value::Bool(match op {
        BinOp::Eq => ordering == Ordering::Equal,
        BinOp::Ne => ordering != Ordering::Equal,
        BinOp::Lt => ordering == Ordering::Less,
        BinOp::Le => ordering != Ordering::Greater,
        BinOp::Gt => ordering == Ordering::Greater,
        BinOp::Ge => ordering != Ordering::Less,
        _ => unreachable!("only comparisons reach here"),
    }))
}

/// `+`, `-`, `*`, `/` and `%` as the hardware does them, a `float` in `f32`: no panic, a
/// division by zero gives an infinity or `NaN`. `%` keeps the sign of the left side, as C's
/// `fmod` does.
fn fractional(op: BinOp, ty: FloatTy, a: f64, b: f64) -> f64 {
    match ty {
        FloatTy::Float => {
            let (a, b) = (a as f32, b as f32);
            (match op {
                BinOp::Add => a + b,
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                BinOp::Div => a / b,
                BinOp::Rem => a % b,
                _ => unreachable!("only arithmetic reaches here"),
            }) as f64
        }
        FloatTy::Double => match op {
            BinOp::Add => a + b,
            BinOp::Sub => a - b,
            BinOp::Mul => a * b,
            BinOp::Div => a / b,
            BinOp::Rem => a % b,
            _ => unreachable!("only arithmetic reaches here"),
        },
    }
}

/// A number made into another numeric type where nothing can fail: an integer into a
/// wider one, a number into a fractional type (rounded to the nearest), a `double` into
/// a `float`.
fn convert(value: Value, to: &Ty) -> Value {
    match (value, to) {
        (Value::Int(_, value), Ty::Int(to)) => Value::Int(*to, value),
        (Value::Int(_, value), Ty::Float(FloatTy::Float)) => Value::Float(FloatTy::Float, value as f32 as f64),
        (Value::Int(_, value), Ty::Float(FloatTy::Double)) => Value::Float(FloatTy::Double, value as f64),
        (Value::Float(_, value), Ty::Float(FloatTy::Float)) => Value::Float(FloatTy::Float, value as f32 as f64),
        (Value::Float(_, value), Ty::Float(FloatTy::Double)) => Value::Float(FloatTy::Double, value),
        _ => unreachable!("the checker only converts numbers"),
    }
}

/// Overflow panics in every build; `wrappingAdd` is written to wrap on purpose. A shift
/// count outside the bits of the type panics the same way; the bits that `<<` pushes out
/// are gone without a word, since that is what a shift is for.
fn arithmetic(op: BinOp, ty: IntTy, a: i128, b: i128, span: Span) -> Result<Value, Diag> {
    if op.is_shift() && !(0..ty.bits() as i128).contains(&b) {
        return Err(Diag::panic(span, format!("shift by {b} is out of range: {} has {} bits", ty.name(), ty.bits())));
    }
    // A number is kept as its value, so the bits above the type agree on both sides: an
    // unsigned value has zeros there, and `>>` brings down zeros; a signed one carries its
    // sign up, and `>>` brings the sign down.
    let result = match op {
        BinOp::BitAnd => Some(a & b),
        BinOp::BitOr => Some(a | b),
        BinOp::BitXor => Some(a ^ b),
        BinOp::Shl => return Ok(Value::Int(ty, ty.wrap(a << b))),
        BinOp::Shr => Some(a >> b),
        BinOp::Add => Some(a + b),
        BinOp::Sub => Some(a - b),
        // Two `ulong`s can multiply past what `i128` holds; that does not fit the type either.
        BinOp::Mul => a.checked_mul(b),
        BinOp::Div | BinOp::Rem if b == 0 => {
            return Err(Diag::panic(span, format!("division by zero: {a} {} 0", op.symbol())));
        }
        BinOp::Div => Some(a / b),
        BinOp::Rem => Some(a % b),
        _ => unreachable!("only arithmetic reaches here"),
    };
    match result {
        Some(result) if ty.fits(result) => Ok(Value::Int(ty, result)),
        _ => {
            let diag = Diag::panic(span, format!("overflow: {a} {} {b} does not fit in {}", op.symbol(), ty.name()));
            Err(match op {
                BinOp::Add => diag.hint("`tryAdd` gives `null` instead, and `wrappingAdd` wraps around"),
                BinOp::Sub => diag.hint("`trySubtract` gives `null` instead, and `wrappingSubtract` wraps around"),
                BinOp::Mul => diag.hint("`tryMultiply` gives `null` instead, and `wrappingMultiply` wraps around"),
                BinOp::Div => diag.hint("`tryDivide` gives `null` instead"),
                _ => diag,
            })
        }
    }
}

/// The methods of the collections that neither compare items nor can fail.
fn collection_method(recv: Value, name: &str, mut args: Vec<Value>) -> Value {
    let lent = |found: Option<Cell>| found.map_or(Value::Null, Value::Borrow);
    match (&recv, name) {
        (Value::List(items), "length") => Value::Int(IntTy::Nint, items.borrow().len() as i128),
        (Value::List(items), "clear") => {
            items.borrow_mut().clear();
            Value::Void
        }
        (Value::List(items), "add") => {
            items.borrow_mut().push(cell(args.remove(0)));
            Value::Void
        }
        // The items are `Copyable`: each gets a cell of its own.
        (Value::List(items), "addAll") => {
            let added: Vec<Cell> = cells_of(&args[0]).iter().map(|item| cell(fresh(&item.borrow()))).collect();
            items.borrow_mut().extend(added);
            Value::Void
        }
        (Value::List(items), "get") => {
            let Value::Int(_, at) = args[0] else { unreachable!("the checker makes an index an integer") };
            lent(usize::try_from(at).ok().and_then(|at| items.borrow().get(at).cloned()))
        }
        (Value::List(items), "removeLast") => {
            let last = items.borrow_mut().pop();
            last.map_or(Value::Null, |item| item.borrow().clone())
        }
        (Value::List(items), "reverse") => {
            items.borrow_mut().reverse();
            Value::Void
        }
        (Value::Set(items), "length") => Value::Int(IntTy::Nint, items.borrow().len() as i128),
        (Value::Set(items), "clear") => {
            items.borrow_mut().clear();
            Value::Void
        }
        (Value::Set(items), "add") => {
            let value = args.remove(0);
            Value::Bool(items.borrow_mut().insert(key_of(&value), value).is_none())
        }
        (Value::Set(items), "contains") => Value::Bool(items.borrow().contains_key(&key_of(&args[0]))),
        (Value::Set(items), "remove") => Value::Bool(items.borrow_mut().remove(&key_of(&args[0])).is_some()),
        (Value::Map(entries), "length") => Value::Int(IntTy::Nint, entries.borrow().len() as i128),
        (Value::Map(entries), "clear") => {
            entries.borrow_mut().clear();
            Value::Void
        }
        (Value::Map(entries), "get") => lent(entries.borrow().get(&key_of(&args[0])).map(|(_, value)| value.clone())),
        (Value::Map(entries), "contains") => Value::Bool(entries.borrow().contains_key(&key_of(&args[0]))),
        (Value::Map(entries), "remove") => {
            let removed = entries.borrow_mut().remove(&key_of(&args[0]));
            removed.map_or(Value::Null, |(_, value)| value.borrow().clone())
        }
        (Value::Map(entries), "getOrAdd") => {
            let initial = args.remove(1);
            let key = args.remove(0);
            let mut entries = entries.borrow_mut();
            let under = key_of(&key);
            if !entries.contains_key(&under) {
                entries.insert(under.clone(), (key, cell(initial)));
            }
            Value::Borrow(entries.get(&under).expect("just put there").1.clone())
        }
        _ => unreachable!("the checker only lets known methods through"),
    }
}

/// The integer type of `tryToInt`, `wrappingToUlong`.
fn converted_to(name: &str) -> IntTy {
    let rest = name.strip_prefix("tryTo").or_else(|| name.strip_prefix("wrappingTo")).expect("a conversion");
    IntTy::from_title(rest).expect("the checker knows the type")
}

/// `int.MaxValue`, `double.NaN`, `float.Pi`: a `float` as the `double` that holds it.
fn constant(ty: &Ty, name: &str) -> Value {
    match ty {
        Ty::Int(ty) => Value::Int(*ty, if name == "MinValue" { ty.min() } else { ty.max() }),
        Ty::Float(FloatTy::Double) => Value::Float(
            FloatTy::Double,
            match name {
                "MinValue" => f64::MIN,
                "MaxValue" => f64::MAX,
                "NaN" => f64::NAN,
                "Infinity" => f64::INFINITY,
                "Pi" => std::f64::consts::PI,
                _ => std::f64::consts::E,
            },
        ),
        Ty::Float(FloatTy::Float) => Value::Float(
            FloatTy::Float,
            f64::from(match name {
                "MinValue" => f32::MIN,
                "MaxValue" => f32::MAX,
                "NaN" => f32::NAN,
                "Infinity" => f32::INFINITY,
                "Pi" => std::f32::consts::PI,
                _ => std::f32::consts::E,
            }),
        ),
        Ty::Token => Value::Token(None),
        _ => unreachable!("the checker gives constants to number types and the token type only"),
    }
}

/// A number written with the letter after `:` in `{x:X2}`: `digits` is the digits it has at
/// least (`x`, `X`, `b`, `D`) or the digits after the point (`F`). A negative integer is
/// written in hex and in binary as the bits of its type hold it, as C# and Rust write it;
/// in decimal with `-` in front of the digits. The same as `sg_written_as` in compiled code.
fn written_as(value: &Value, letter: Letter, digits: usize) -> String {
    match (value, letter) {
        (Value::Float(ty, value), Letter::Fixed) => match value.is_finite() {
            true => format!("{value:.digits$}"),
            false => show_float(*value, *ty),
        },
        (Value::Int(_, value), Letter::Decimal) => {
            let sign = if *value < 0 { "-" } else { "" };
            format!("{sign}{:0digits$}", value.unsigned_abs())
        }
        (Value::Int(ty, value), _) => {
            let bits = (*value as u128) & (u128::MAX >> (128 - ty.bits()));
            match letter {
                Letter::Hex => format!("{bits:0digits$x}"),
                Letter::HexUpper => format!("{bits:0digits$X}"),
                _ => format!("{bits:0digits$b}"),
            }
        }
        _ => unreachable!("the checker lets a letter through only for a number it fits"),
    }
}

/// The number a text writes, in the type: `Err(Some(position))` where the text stops being
/// a number (its length when it ends too early), `Err(None)` when an integer type does not
/// hold it. A fractional number past the type is `Infinity`.
/// `radix` is 10, or 16 for `parseHex` (an integer only). The same rules as
/// `sg_read_whole` and `sg_read_fraction` in compiled code.
fn parse_number(text: &[u8], ty: &Ty, radix: u32) -> Result<Value, Option<usize>> {
    let signed = matches!(text.first(), Some(b'-' | b'+'));
    let start = usize::from(signed);
    let digits_from = |at: usize| text[at..].iter().take_while(|byte| byte.is_ascii_digit()).count();
    match ty {
        Ty::Int(ty) => {
            if text.first() == Some(&b'-') && !ty.signed() {
                return Err(Some(0));
            }
            let digits = text[start..].iter().take_while(|byte| char::from(**byte).is_digit(radix)).count();
            if digits == 0 || start + digits < text.len() {
                return Err(Some(start + digits));
            }
            // Every digit is looked at first: a text that is not a number says so even
            // when it is long.
            let mut value: i128 = 0;
            for byte in &text[start..] {
                value = value * i128::from(radix) + i128::from(char::from(*byte).to_digit(radix).expect("checked to be a digit"));
                if value > 1 << 64 {
                    return Err(None);
                }
            }
            let value = if text[0] == b'-' { -value } else { value };
            if ty.fits(value) { Ok(Value::Int(*ty, value)) } else { Err(None) }
        }
        Ty::Float(ty) => {
            // What `print` writes, it reads back: `NaN`, `Infinity`, `-Infinity`.
            let rest = &text[start..];
            if rest == b"NaN" && !signed {
                return Ok(Value::Float(*ty, f64::NAN));
            }
            if rest == b"Infinity" {
                return Ok(Value::Float(*ty, if text[0] == b'-' { f64::NEG_INFINITY } else { f64::INFINITY }));
            }
            let mut at = start + digits_from(start);
            let mut digits = at - start;
            if text.get(at) == Some(&b'.') {
                let after = digits_from(at + 1);
                (at, digits) = (at + 1 + after, digits + after);
            }
            if digits == 0 {
                return Err(Some(at));
            }
            if matches!(text.get(at), Some(b'e' | b'E')) {
                at += 1;
                if matches!(text.get(at), Some(b'-' | b'+')) {
                    at += 1;
                }
                let exponent = digits_from(at);
                at += exponent;
                if exponent == 0 {
                    return Err(Some(at));
                }
            }
            if at < text.len() {
                return Err(Some(at));
            }
            let written = std::str::from_utf8(text).expect("only ASCII is left");
            let value = match ty {
                FloatTy::Float => f64::from(written.parse::<f32>().expect("checked above")),
                FloatTy::Double => written.parse::<f64>().expect("checked above"),
            };
            // Past the largest number it is `Infinity`, as in Rust, C# and C.
            Ok(Value::Float(*ty, value))
        }
        _ => unreachable!("the checker gives `parse` to number types only"),
    }
}

/// `pow`, `atan2`, `min`, `max`; on a `float` in `f32`. `min` and `max` give `NaN` if either
/// side is one, and take `-0` as less than `0`, as C# does.
fn float_method2(ty: FloatTy, a: f64, name: &str, b: f64) -> Value {
    let pick = |a: f64, b: f64| -> f64 {
        if a.is_nan() || b.is_nan() {
            return f64::NAN;
        }
        let a_first = match a == b {
            true => (name == "min") == a.is_sign_negative(),
            false => (name == "min") == (a < b),
        };
        if a_first { a } else { b }
    };
    let result = match (ty, name) {
        (_, "min" | "max") => pick(a, b),
        (FloatTy::Float, "pow") => f64::from((a as f32).powf(b as f32)),
        (FloatTy::Float, _) => f64::from((a as f32).atan2(b as f32)),
        (FloatTy::Double, "pow") => a.powf(b),
        (FloatTy::Double, _) => a.atan2(b),
    };
    Value::Float(ty, result)
}

/// `abs`, `sqrt`, `floor`, `ceil`, `round` (half away from zero), `truncate`, `isNaN`,
/// `isInfinite`, and the functions of the C library: `exp`, `log`, `sin` and the rest. On a
/// `float` in `f32`.
fn float_method(ty: FloatTy, value: f64, name: &str) -> Value {
    match name {
        "isNaN" => return Value::Bool(value.is_nan()),
        "isInfinite" => return Value::Bool(value.is_infinite()),
        _ => {}
    }
    let result = match ty {
        FloatTy::Float => {
            let value = value as f32;
            (match name {
                "abs" => value.abs(),
                "sqrt" => value.sqrt(),
                "floor" => value.floor(),
                "ceil" => value.ceil(),
                "round" => value.round(),
                "exp" => value.exp(),
                "log" => value.ln(),
                "log2" => value.log2(),
                "log10" => value.log10(),
                "sin" => value.sin(),
                "cos" => value.cos(),
                "tan" => value.tan(),
                "asin" => value.asin(),
                "acos" => value.acos(),
                "atan" => value.atan(),
                _ => value.trunc(),
            }) as f64
        }
        FloatTy::Double => match name {
            "abs" => value.abs(),
            "sqrt" => value.sqrt(),
            "floor" => value.floor(),
            "ceil" => value.ceil(),
            "round" => value.round(),
            "exp" => value.exp(),
            "log" => value.ln(),
            "log2" => value.log2(),
            "log10" => value.log10(),
            "sin" => value.sin(),
            "cos" => value.cos(),
            "tan" => value.tan(),
            "asin" => value.asin(),
            "acos" => value.acos(),
            "atan" => value.atan(),
            _ => value.trunc(),
        },
    };
    Value::Float(ty, result)
}

/// A character inside another value, or in an `assert` message: in single quotes, with the
/// escapes a character constant has. Compiled code writes it the same way.
pub fn quoted_char(value: char) -> String {
    match value {
        '\'' => "'\\''".to_string(),
        '\\' => "'\\\\'".to_string(),
        '\n' => "'\\n'".to_string(),
        '\t' => "'\\t'".to_string(),
        '\r' => "'\\r'".to_string(),
        '\0' => "'\\0'".to_string(),
        _ if (value as u32) < 32 || value as u32 == 127 => format!("'\\u{{{:x}}}'", value as u32),
        _ => format!("'{value}'"),
    }
}

/// An ASCII method of a `char` or a `byte` on the code: a test gives `Ok`, a change of case
/// gives `Err` with the new code.
fn ascii_method(code: u32, name: &str) -> Result<bool, u32> {
    let Ok(byte) = u8::try_from(code) else {
        return if name.starts_with("is") { Ok(false) } else { Err(code) };
    };
    match name {
        "isAscii" => Ok(byte.is_ascii()),
        "isAsciiDigit" => Ok(byte.is_ascii_digit()),
        "isAsciiLetter" => Ok(byte.is_ascii_alphabetic()),
        "isAsciiAlphanumeric" => Ok(byte.is_ascii_alphanumeric()),
        "isAsciiUpper" => Ok(byte.is_ascii_uppercase()),
        "isAsciiLower" => Ok(byte.is_ascii_lowercase()),
        "isAsciiWhitespace" => Ok(byte.is_ascii_whitespace()),
        "toAsciiUpper" => Err(byte.to_ascii_uppercase() as u32),
        _ => Err(byte.to_ascii_lowercase() as u32),
    }
}

fn method(recv: Value, name: &str, args: Vec<Value>) -> Value {
    match (&recv, name, args.as_slice()) {
        // `null` where the plain operator would panic.
        (Value::Int(ty, a), "tryAdd" | "trySubtract" | "tryMultiply" | "tryDivide", [Value::Int(_, b)]) => {
            let (a, b) = (*a, *b);
            let result = match name {
                "tryAdd" => Some(a + b),
                "trySubtract" => Some(a - b),
                "tryMultiply" => a.checked_mul(b),
                _ if b == 0 => return Value::Null,
                _ => Some(a / b),
            };
            match result {
                Some(result) if ty.fits(result) => Value::Int(*ty, result),
                _ => Value::Null,
            }
        }
        // The bytes of the number in an array, the lowest first, or the highest first.
        (Value::Int(ty, value), "toLittleEndian" | "toBigEndian", []) => {
            let bits = *value as u128;
            let mut bytes: Vec<Cell> = (0..ty.bits() / 8).map(|at| cell(Value::Int(IntTy::Byte, ((bits >> (at * 8)) & 0xff) as i128))).collect();
            if name == "toBigEndian" {
                bytes.reverse();
            }
            Value::Array(Rc::new(bytes))
        }
        (Value::Int(_, value), "tryToChar", []) => crate::check::char_of(*value).map_or(Value::Null, Value::Char),
        (Value::Int(_, value), _, []) if name.starts_with("tryTo") || name.starts_with("wrappingTo") => {
            let to = converted_to(name);
            match name.starts_with("tryTo") {
                true if to.fits(*value) => Value::Int(to, *value),
                true => Value::Null,
                false => Value::Int(to, to.wrap(*value)),
            }
        }
        // Toward zero, as a cast; `NaN` and what lies past the type give `null`.
        (Value::Float(_, value), _, []) if name.starts_with("tryTo") => {
            let to = converted_to(name);
            match float_fits(*value, to) {
                true => Value::Int(to, value.trunc() as i128),
                false => Value::Null,
            }
        }
        (Value::Int(ty, a), "min" | "max", [Value::Int(_, b)]) => Value::Int(*ty, if name == "min" { *a.min(b) } else { *a.max(b) }),
        (Value::Float(ty, a), "pow" | "atan2" | "min" | "max", [Value::Float(_, b)]) => float_method2(*ty, *a, name, *b),
        (Value::Float(ty, a), "fusedMultiplyAdd", [Value::Float(_, b), Value::Float(_, c)]) => Value::Float(
            *ty,
            match ty {
                FloatTy::Float => (*a as f32).mul_add(*b as f32, *c as f32) as f64,
                FloatTy::Double => a.mul_add(*b, *c),
            },
        ),
        (Value::Char(value), _, []) if name.starts_with("isAscii") || name.starts_with("toAscii") => match ascii_method(*value as u32, name) {
            Ok(test) => Value::Bool(test),
            Err(changed) => Value::Char(char::from_u32(changed).expect("an ASCII change keeps a character")),
        },
        (Value::Int(ty, value), _, []) if name.starts_with("isAscii") || name.starts_with("toAscii") => match ascii_method(*value as u32, name) {
            Ok(test) => Value::Bool(test),
            Err(changed) => Value::Int(*ty, changed as i128),
        },
        (Value::Char(value), "isWhitespace", []) => Value::Bool(value.is_whitespace()),
        (Value::Char(value), "isLetter" | "isNumeric" | "isAlphanumeric" | "isUpper" | "isLower" | "isControl", []) => {
            use crate::unicode::{CONTROL, LETTER, LOWER, NUMERIC, UPPER};
            let bits = match name {
                "isLetter" => LETTER,
                "isNumeric" => NUMERIC,
                "isAlphanumeric" => LETTER | NUMERIC,
                "isUpper" => UPPER,
                "isLower" => LOWER,
                _ => CONTROL,
            };
            Value::Bool(crate::unicode::is(*value, bits))
        }
        (Value::Char(value), "toUpper", []) => Value::Char(crate::unicode::upper(*value)),
        (Value::Char(value), "toLower", []) => Value::Char(crate::unicode::lower(*value)),
        (Value::Char(value), "utf8Length", []) => Value::Int(IntTy::Nint, value.len_utf8() as i128),
        (Value::Str(_), "clone", []) => recv,
        (Value::Str(_), "asBytes", []) => Value::Slice(cells_of(&recv)),
        (_, "equals", [other]) => Value::Bool(key_of(&recv) == key_of(other)),
        (_, "compare", [other]) => Value::Int(IntTy::Int, key_of(&recv).cmp(&key_of(other)) as i128),
        (Value::Int(ty, a), _, [Value::Int(_, b)]) => {
            let (a, b) = (*a, *b);
            let result = match name {
                "wrappingAdd" => a.wrapping_add(b),
                "wrappingSubtract" => a.wrapping_sub(b),
                _ => a.wrapping_mul(b),
            };
            Value::Int(*ty, ty.wrap(result))
        }
        (_, "toString", []) => Value::Str(recv.format().into()),
        (Value::Float(ty, value), _, []) => float_method(*ty, *value, name),
        (Value::Str(text), "length", []) => Value::Int(IntTy::Nint, text.len() as i128),
        (Value::Str(text), "indexOf" | "lastIndexOf", [part]) => match find_bytes(text.as_bytes(), &needle(part), name == "lastIndexOf") {
            Some(at) => Value::Int(IntTy::Nint, at as i128),
            None => Value::Null,
        },
        (Value::Str(text), "contains", [part]) => Value::Bool(find_bytes(text.as_bytes(), &needle(part), false).is_some()),
        (Value::Str(text), "startsWith", [part]) => Value::Bool(text.as_bytes().starts_with(&needle(part))),
        (Value::Str(text), "endsWith", [part]) => Value::Bool(text.as_bytes().ends_with(&needle(part))),
        (Value::Str(text), "trim", []) => Value::Str(text.trim().to_string().into()),
        (Value::Str(text), "trimStart", []) => Value::Str(text.trim_start().to_string().into()),
        (Value::Str(text), "trimEnd", []) => Value::Str(text.trim_end().to_string().into()),
        (Value::Str(text), "replaceAll", [old, new]) => {
            let (old, new) = (String::from_utf8(needle(old)), String::from_utf8(needle(new)));
            Value::Str(text.replace(&old.expect("a character or a text"), &new.expect("a character or a text")).into())
        }
        (Value::Str(text), "equalsIgnoreAsciiCase", [Value::Str(other)]) => Value::Bool(text.eq_ignore_ascii_case(other)),
        (Value::Str(text), "equalsIgnoreCase", [Value::Str(other)]) => Value::Bool(crate::unicode::equals_ignore_case(text, other)),
        (Value::Str(text), "toUpper", []) => Value::Str(crate::unicode::upper_text(text).into()),
        (Value::Str(text), "toLower", []) => Value::Str(crate::unicode::lower_text(text).into()),
        (Value::Str(text), "foldCase", []) => Value::Str(crate::unicode::fold_text(text).into()),
        (Value::Str(text), "chars" | "split" | "splitWhitespace" | "lines", _) => {
            let kind = match name {
                "chars" => TextIter::Chars,
                "split" => TextIter::Split,
                "splitWhitespace" => TextIter::SplitWhitespace,
                _ => TextIter::Lines,
            };
            let separator = args.first().map_or(Vec::new(), needle);
            let separator = String::from_utf8(separator).expect("a separator is a character or a text");
            Value::Iter(Rc::new(RefCell::new(Cursor { kind, text: text.clone(), at: 0, separator: separator.into(), done: false })))
        }
        (Value::Iter(cursor), "next", []) => cursor.borrow_mut().next().unwrap_or(Value::Null),
        _ => unreachable!("the checker only lets known methods through"),
    }
}
