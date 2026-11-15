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
    Struct(Rc<str>, Rc<Vec<Cell>>),
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
    /// `Task<R>`: a task started with `Task.run`, by the call of an async function, or by
    /// `Task.delay`.
    Task(Rc<RefCell<Task>>),
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
    /// The `Task.delay` tasks, by when they end on the clock of the run, then in the order
    /// they were made.
    timers: BTreeMap<(u64, u64), std::rc::Weak<RefCell<Task>>>,
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

    /// The fiber that gets the turn: the first that is ready. When none is, the time of the
    /// first timer comes. When there is no timer either, nothing could end a wait, and the
    /// run stops with a panic.
    fn next(state: &mut State, at: Span) -> usize {
        loop {
            if let Some(fiber) = state.ready.pop_front() {
                return fiber;
            }
            let Some((&key, _)) = state.timers.iter().next() else {
                state.stop = Some(Stop::Diag(Diag::panic(at, "every task waits, and nothing is left that could end a wait")));
                return 0;
            };
            let task = state.timers.remove(&key).expect("just seen");
            let due = state.clock + Duration::from_millis(key.0);
            let now = Instant::now();
            if due > now {
                std::thread::sleep(due - now);
            }
            state.now = key.0;
            if let Some(task) = task.upgrade() {
                let mut task = task.borrow_mut();
                task.state = TaskState::Done(Some(Value::Void));
                let waiters = std::mem::take(&mut task.waiters);
                state.ready.extend(waiters);
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
    test_err: RefCell<String>,
    sched: Sched,
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

thread_local! {
    /// `Stdin`, `Stdout` and `Stderr` of the program running on this thread.
    static STREAMS: RefCell<Option<[Cell; 3]>> = const { RefCell::new(None) };
    /// The program is ending with a panic or `exit`: a writer that ends now does not write
    /// its buffer, as in compiled code, where nothing runs after them.
    static ENDING: Flag<bool> = const { Flag::new(false) };
}

/// Makes `Stdin`, `Stdout` and `Stderr` for a program that starts.
fn open_streams() {
    let writer = |sink| cell(Value::Writer(Rc::new(RefCell::new(Writer { sink, buffer: Vec::new(), failed: None }))));
    let stdin = cell(Value::Reader(Rc::new(RefCell::new(Reader { file: None, done: 0 }))));
    STREAMS.with(|streams| *streams.borrow_mut() = Some([stdin, writer(Sink::Stdout), writer(Sink::Stderr)]));
    ENDING.with(|ending| ending.set(false));
}

/// What `Stdin`, `Stdout` or `Stderr` names: a `ref` to the stream.
fn stream_slot(name: &str) -> Option<Cell> {
    let at = ["Stdin", "Stdout", "Stderr"].iter().position(|known| *known == name)?;
    STREAMS.with(|streams| streams.borrow().as_ref().map(|streams| cell(Value::Borrow(streams[at].clone()))))
}

fn ending() {
    ENDING.with(|ending| ending.set(true));
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
        Value::Struct(_, cells) | Value::Enum(_, _, cells) | Value::Array(cells) => cells.iter().any(|item| owns_any(&item.borrow(), ids)),
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

/// A `StreamReader`: a file read through a buffer, or, with no file, the input of the program.
#[derive(Debug)]
pub struct Reader {
    file: Option<std::io::BufReader<std::fs::File>>,
    /// The bytes given out so far: where an `InvalidText` lies.
    done: u64,
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
/// number, laid out as JavaScript does it (`0.1`, `100`, `1.5e+21`, `1e-7`). `float` reads
/// back as a `float`.
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
    let most = match ty {
        FloatTy::Float => 9,
        FloatTy::Double => 17,
    };
    let written = (1..=most)
        .map(|digits| format!("{:.*e}", digits - 1, value))
        .find(|text| match ty {
            FloatTy::Float => text.parse::<f32>().ok() == Some(value as f32),
            FloatTy::Double => text.parse::<f64>().ok() == Some(value),
        })
        .expect("17 digits always read back");
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

fn ok(value: Value) -> Value {
    Value::Enum("Result".into(), 0, Rc::new(vec![cell(value)]))
}

fn failed(cause: Value) -> Value {
    Value::Enum("Result".into(), 1, Rc::new(vec![cell(cause)]))
}

/// The `IoError` for what the system said. The codes are the same on Linux and macOS, except
/// for a full quota.
fn io_cause(error: Option<&std::io::Error>, code: Option<i32>) -> Value {
    const QUOTA: i32 = if cfg!(target_os = "linux") { 122 } else { 69 };
    let variant = |at: usize, fields: Vec<Value>| Value::Enum("IoError".into(), at, Rc::new(fields.into_iter().map(cell).collect()));
    match code {
        // ENOENT, ENOTDIR
        Some(2 | 20) => variant(0, Vec::new()),
        // EPERM, EACCES, EROFS
        Some(1 | 13 | 30) => variant(1, Vec::new()),
        // EISDIR
        Some(21) => variant(2, Vec::new()),
        // ENOSPC, EDQUOT
        Some(28) => variant(3, Vec::new()),
        Some(code) if code == QUOTA => variant(3, Vec::new()),
        Some(code) => variant(5, vec![Value::Int(IntTy::Int, code as i128)]),
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

/// Writes the bytes into the file, which is made if it is not there; `append` keeps what
/// it holds and puts them after it.
/// Bytes read as text: `Ok`, or `InvalidText` with the place of the first bad byte, counted
/// from `at`.
fn text_read(bytes: Vec<u8>, at: u64) -> Value {
    match String::from_utf8(bytes) {
        Ok(text) => ok(Value::Str(Rc::new(text))),
        Err(error) => failed(io_cause(None, None).invalid_text(at + error.utf8_error().valid_up_to() as u64)),
    }
}

fn write_file(path: &str, bytes: &[u8], append: bool) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    match append {
        true => options.append(true).create(true),
        false => options.write(true).create(true).truncate(true),
    };
    options.open(path)?.write_all(bytes)
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
        Value::Struct(name, fields) => Value::Struct(name.clone(), cells(fields)),
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
    pub input: &'w mut (dyn BufRead + Send + 'static),
    /// The input is a terminal: what was printed goes out before the program waits for a line.
    pub input_is_terminal: bool,
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
fn keep_names(after: &[String], mut names: HashMap<String, Cell>, frame: &mut Frame) {
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
    scopes: Vec<HashMap<String, Cell>>,
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

pub fn run_program<'a>(prog: &'a Program, src: &'a str, out: &'a mut dyn Write, world: World<'a>) -> Result<(), Stop> {
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
    let io = Shared { out: RefCell::new(out), world: RefCell::new(world), test_err: RefCell::new(String::new()), sched: Sched::new() };
    std::thread::scope(|scope| {
        let mut interp = Interp { funcs, methods, structs, enums, src, io: &io, scope, me: 0, depth: 0, in_test: false, guards: Vec::new(), aliases, started: Vec::new() };
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
    let io = Shared { out: RefCell::new(out), world: RefCell::new(world), test_err: RefCell::new(String::new()), sched: Sched::new() };
    std::thread::scope(|scope| {
        let mut interp = Interp { funcs, methods, structs, enums, src, io: &io, scope, me: 0, depth: 0, in_test: true, guards: Vec::new(), aliases, started: Vec::new() };
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
        let mut frame = Frame { scopes: vec![HashMap::new()] };
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
}

impl<'a, 'e> Interp<'a, 'e> {
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
                    self.start_fiber(task, func, args, false)?;
                }
                TaskState::Running => {
                    task.borrow_mut().waiters.push(self.me);
                    self.io.sched.block(self.me, at)?;
                }
            }
        }
    }

    /// `f(x)` of an async function `f`, without `await`: the task starts now and runs until it
    /// first waits, then this fiber goes on, as in C#.
    fn start_async(&mut self, func: &'a Func, args: Vec<Value>, at: Span) -> Res<Value> {
        let task = Rc::new(RefCell::new(Task { state: TaskState::Running, started: at, waiters: Vec::new() }));
        self.started.push(task.clone());
        self.start_fiber(&task, func, args, true)?;
        Ok(Value::Task(task))
    }

    /// `Task.delay(ms)`: a task that ends when the clock of the run has gone that far.
    fn delay(&mut self, ms: i128, at: Span) -> Res<Value> {
        if ms < 0 {
            return Err(Diag::panic(at, format!("`Task.delay({ms})`: a delay is not negative")).into());
        }
        let task = Rc::new(RefCell::new(Task { state: TaskState::Running, started: at, waiters: Vec::new() }));
        let mut state = self.io.sched.lock();
        let key = (state.now + ms as u64, state.made);
        state.made += 1;
        state.timers.insert(key, Rc::downgrade(&task));
        drop(state);
        self.started.push(task.clone());
        Ok(Value::Task(task))
    }

    /// Runs `func(args)` for `task` on a fiber of its own, which gets the turn now. With
    /// `caller_ready` this fiber is next once the new one waits; otherwise this one waits for
    /// the task and goes on when it ends.
    fn start_fiber(&mut self, task: &Rc<RefCell<Task>>, func: &'a Func, args: Vec<Value>, caller_ready: bool) -> Res<()> {
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
                let result = begun.and_then(|()| inner.call(func, None, args, started));
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
        let mut frame = Frame { scopes: vec![HashMap::new()] };
        if let Some(this) = this {
            frame.declare("this", this);
        }
        for (param, value) in func.params.iter().zip(args) {
            frame.declare(&param.name, value);
        }
        self.depth += 1;
        let result = self.block(&func.body, &mut frame);
        drop(frame);
        self.depth -= 1;
        // A stop of the run waits for nothing: the program ends.
        if !self.started.is_empty() && !matches!(result, Err(Unwind::Stop(_) | Unwind::Abandon)) {
            self.unwaited()?;
        }
        match result {
            Ok(Flow::Return(value)) | Err(Unwind::Return(value)) => Ok(value),
            Ok(_) => Ok(Value::Void),
            Err(other) => Err(other),
        }
    }

    fn block(&mut self, block: &'a Block, frame: &mut Frame) -> Res<Flow> {
        frame.scopes.push(HashMap::new());
        let mut flow = Flow::Next;
        for stmt in block {
            flow = self.stmt(stmt, frame)?;
            if !matches!(flow, Flow::Next) {
                break;
            }
        }
        frame.scopes.pop();
        Ok(flow)
    }

    /// A statement. What it lent of `shared checked` values ends with it.
    fn stmt(&mut self, stmt: &'a Stmt, frame: &mut Frame) -> Res<Flow> {
        let mark = self.guards.len();
        let flow = self.stmt_in(stmt, frame);
        self.guards.truncate(mark);
        flow
    }

    fn stmt_in(&mut self, stmt: &'a Stmt, frame: &mut Frame) -> Res<Flow> {
        match &stmt.kind {
            StmtKind::Let { name, init, .. } => {
                let value = self.expr(init, frame)?;
                frame.declare(name, value);
            }
            StmtKind::Assign { target, op, value, through } => {
                let mut new = self.expr(value, frame)?;
                let mut target = self.place(target, frame)?;
                if *through {
                    target = behind_borrows(target);
                }
                if let Some(op) = op {
                    let old = strip(target.borrow().clone());
                    new = binary(*op, old, new, stmt.span)?;
                }
                *target.borrow_mut() = new;
            }
            StmtKind::If { cond, then, els, after } => {
                // The scope holds the names of an `is` pattern in the condition.
                frame.scopes.push(HashMap::new());
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
                    frame.scopes.push(HashMap::new());
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
                    frame.scopes.push(HashMap::new());
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
                // A map or a set is gone over as it was when the loop started.
                let entry = |(key, value): &(Value, Cell)| {
                    cell(Value::Struct("entry".into(), Rc::new(vec![cell(key.clone()), value.clone()])))
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
                        (Value::Lines(lines), _) => self.next_line(lines)?.map(cell),
                        (Value::List(items), _) => items.borrow().get(next).cloned(),
                        (Value::Array(items) | Value::Slice(items), _) => items.get(next).cloned(),
                        _ => unreachable!("the checker only lets `for` go over a collection"),
                    };
                    let Some(item) = item else { break };
                    next += 1;
                    let bound = if by.is_some() { Value::Borrow(item) } else { item.borrow().clone() };
                    frame.scopes.push(HashMap::new());
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
                frame.scopes.push(HashMap::new());
                frame.declare(name, Value::Borrow(held.value.clone()));
                let flow = self.block(body, frame);
                frame.scopes.pop();
                return flow;
            }
            // `lock a as x, b as y { ... }`: what each `shared locked` value holds, lent to
            // change for the block. Tasks change hands only where one waits, and nothing in
            // the block waits, so no other task holds the lock here.
            StmtKind::Lock { items, body } => {
                let mut taken: Vec<Rc<Held>> = Vec::new();
                frame.scopes.push(HashMap::new());
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
                let flow = self.block(body, frame);
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
        match &expr.kind {
            ExprKind::Var(name) => Ok(frame.slot(name)),
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
            ExprKind::Var(_) | ExprKind::Field { .. } if expr.moved => Ok(self.place(expr, frame)?.replace(Value::Null)),
            ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. } => {
                let value = fresh(&self.place(expr, frame)?.borrow());
                Ok(value)
            }
            ExprKind::FuncRef(name) => Ok(Value::Func(name.as_str().into())),
            ExprKind::Constant { ty, name } => Ok(constant(ty, name)),
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
            ExprKind::Binary(op, left, right) => {
                let left = strip(self.expr(left, frame)?);
                let right = strip(self.expr(right, frame)?);
                self.binary(*op, left, right, expr.span)
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
            ExprKind::StructLit { name, fields, .. } => {
                let mut cells: Vec<Option<Cell>> = vec![None; fields.len()];
                for init in fields {
                    cells[init.index] = Some(cell(self.expr(&init.value, frame)?));
                }
                let cells = cells.into_iter().map(|field| field.expect("the checker requires every field")).collect();
                Ok(Value::Struct(name.as_str().into(), Rc::new(cells)))
            }
            ExprKind::New(inner) => Ok(Value::Ptr(cell(self.expr(inner, frame)?))),
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
                    frame.scopes.push(HashMap::new());
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
                let Value::Task(task) = strip(self.expr(inner, frame)?) else { unreachable!("the checker lets only a task be awaited") };
                self.await_task(&task, expr.span)
            }
            ExprKind::Call { name, args, callee, .. } => match callee {
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
                _ => self.call_expr(expr, name, args, frame),
            },
            ExprKind::Method { recv, name, args, callee, .. } => match callee {
                Callee::Static { owner, name } => {
                    let func = self.methods[owner.as_str()][name.as_str()];
                    let values = self.values(args, frame)?;
                    self.call(func, None, values, expr.span)
                }
                Callee::Method { owner, this, whole } => {
                    let func = self.methods[owner.as_str()][name.as_str()];
                    // The receiver is lent or handed over without `read` / `ref` at the call.
                    let mark = self.guards.len();
                    let mut target = self.place(recv, frame)?;
                    // `read FolderRef this`: the method takes the value that holds the struct.
                    loop {
                        let next = match &*target.borrow() {
                            Value::Shared(_) if *whole => None,
                            value => behind(value),
                        };
                        let Some(inner) = next else { break };
                        target = inner;
                    }
                    let lent = self.guards.len();
                    let this = match this {
                        This::Value => fresh(&target.borrow()),
                        This::Read | This::Ref => Value::Borrow(target),
                    };
                    let values = self.values(args, frame)?;
                    self.change_reserved(mark..lent)?;
                    self.call(func, Some(this), values, expr.span)
                }
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
                Callee::Value => unreachable!("a variable is called without a receiver"),
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
                            let cause = Value::Struct("TextError".into(), Rc::new(vec![position]));
                            Value::Enum("Result".into(), 1, Rc::new(vec![cell(cause)]))
                        }
                    })
                }
                // `ulong.fromLittleEndian(bytes)`: exactly as many bytes as the type takes.
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
                Callee::Stream(call) => self.stream(*call, recv, args, frame),
                // `Task.run(work, x)`: the task runs when it is waited for.
                Callee::TaskRun => {
                    let Value::Func(name) = strip(self.expr(&args[0], frame)?) else { unreachable!("the checker lets `Task.run` start only a function") };
                    let values = self.values(&args[1..], frame)?;
                    let task = Rc::new(RefCell::new(Task { state: TaskState::Lazy(name, values), started: expr.span, waiters: Vec::new() }));
                    self.started.push(task.clone());
                    Ok(Value::Task(task))
                }
                Callee::Delay => {
                    let Value::Int(_, ms) = strip(self.expr(&args[0], frame)?) else { unreachable!("the checker lets `Task.delay` take an `int`") };
                    self.delay(ms, expr.span)
                }
                Callee::Wait => {
                    let Value::Task(task) = strip(self.expr(recv, frame)?) else { unreachable!("the checker lets only a task be waited for") };
                    self.await_task(&task, expr.span)
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
                        return Ok(self.next_line(lines)?.unwrap_or(Value::Null));
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
                    true => self.start_async(func, values, expr.span),
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
                Ok(format!("{} {{ {} }}", short_name(name), named.join(", ")))
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
            (Value::Struct(_, a), Value::Struct(_, b)) => (a, b),
            (Value::Enum(_, a_index, a), Value::Enum(_, b_index, b)) if a_index == b_index => (a, b),
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
        let mut values = self.values(args, frame)?.into_iter().map(strip);
        let Some(Value::Str(path)) = values.next() else { unreachable!("the checker makes a path text") };
        let path = path.to_string();
        // No file has a name with a zero byte in it; the system would see the name end there.
        if path.contains('\0') {
            return Ok(failed(io_cause(None, Some(2))));
        }
        let done = |result: std::io::Result<Value>| match result {
            Ok(value) => ok(value),
            Err(error) => failed(io_cause(Some(&error), error.raw_os_error())),
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
                true => Err(std::io::Error::from_raw_os_error(21)),
                false => Ok(Value::Reader(Rc::new(RefCell::new(Reader { file: Some(std::io::BufReader::with_capacity(STREAM_BUFFER, file)), done: 0 })))),
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

    /// The methods of a `StreamReader` and a `StreamWriter`.
    fn stream(&mut self, call: StreamCall, recv: &'a Expr, args: &'a [Expr], frame: &mut Frame) -> Res<Value> {
        let stream = owned(self.expr(recv, frame)?);
        match (call, stream) {
            (StreamCall::ReadLine, Value::Reader(reader)) => {
                let Value::Borrow(slot) = self.expr(&args[0], frame)? else { unreachable!("the checker makes the line a `ref string`") };
                self.read_line(&reader, &behind_borrows(slot))
            }
            (StreamCall::ReadText, Value::Reader(reader)) => {
                let mut bytes = Vec::new();
                let start = reader.borrow().done;
                let read = match reader.borrow_mut().file.as_mut() {
                    Some(file) => file.read_to_end(&mut bytes),
                    None => {
                        if self.io.world.borrow().input_is_terminal {
                            self.flush_out()?;
                        }
                        self.io.world.borrow_mut().input.read_to_end(&mut bytes)
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
                            self.io.world.borrow_mut().input.read(&mut bytes[got..])
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
            (StreamCall::WriteBytes, Value::Writer(writer)) => {
                let bytes: Vec<u8> = cells_of(&owned(self.expr(&args[0], frame)?)).iter().map(|item| match &*item.borrow() {
                    Value::Int(_, byte) => *byte as u8,
                    _ => unreachable!("the checker makes the items bytes"),
                }).collect();
                self.write_stream(&writer, &bytes, false)
            }
            (StreamCall::Write | StreamCall::WriteLine, Value::Writer(writer)) => {
                let value = self.expr(&args[0], frame)?;
                let mut text = self.show(&value, false, args[0].span)?;
                if call == StreamCall::WriteLine {
                    text.push('\n');
                }
                self.write_stream(&writer, text.as_bytes(), false)
            }
            (StreamCall::Flush, Value::Writer(writer)) => self.write_stream(&writer, &[], true),
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

    /// The next line into `slot`, without its `\n` or `\r\n`: `Ok(true)`, `Ok(false)` at the
    /// end, or `Error`. A line that is not text is used up, and `slot` is left empty.
    fn read_line(&mut self, reader: &Rc<RefCell<Reader>>, slot: &Cell) -> Res<Value> {
        let mut bytes = Vec::new();
        let read = match reader.borrow_mut().file.as_mut() {
            Some(file) => file.read_until(b'\n', &mut bytes),
            None => {
                if self.io.world.borrow().input_is_terminal {
                    self.flush_out()?;
                }
                self.io.world.borrow_mut().input.read_until(b'\n', &mut bytes)
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
    fn next_line(&mut self, lines: &Rc<RefCell<Lines>>) -> Res<Option<Value>> {
        if lines.borrow().done {
            return Ok(None);
        }
        let reader = lines.borrow().reader.clone();
        let line = cell(Value::Str(Rc::new(String::new())));
        let result = self.read_line(&reader, &line)?;
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
    IntTy::from_name(&rest.to_lowercase()).expect("the checker knows the type")
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
        _ => unreachable!("the checker gives constants to number types only"),
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
