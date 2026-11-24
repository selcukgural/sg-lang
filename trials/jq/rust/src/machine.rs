//! The machine that walks the filter's tree with a stack of its own on the heap, not the machine's:
//! what is left to do with a value is a chain of continuations (`Kont`), and the points that have
//! more outputs to give are on a stack of choices (`choices`). An output goes to its continuation;
//! when a filter has no more outputs the machine goes back to the last choice that has (it
//! backtracks). A call in tail position passes its continuation on unchanged, so it takes no room.
//!
//! In `path(f)` and on the left of an assignment, a value carries the path it was found at
//! (`Track`), as jq tracks it.

use crate::builtins::{self, Native, binary, delete_paths, error, get_path, set_path, slice};
use crate::single;
use crate::compile::{DefIr, Ir};
use crate::parser::Op;
use crate::value::{Key, Map, Value, trim_vec};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Where variables and parameters are found: a chain of frames.
#[derive(Clone, Default)]
pub struct Env(Option<Rc<Frame>>);

pub struct Frame {
    parent: Env,
    slot: Slot,
}

enum Slot {
    Var(Value),
    Params(Vec<Closure>),
}

/// A filter argument: where it was written and the frames it sees there.
#[derive(Clone)]
struct Closure {
    ir: &'static Ir,
    env: Env,
}

impl Env {
    pub fn push_var(&self, value: Value) -> Env {
        self.push(Slot::Var(value))
    }

    pub fn var(&self, up: usize) -> Value {
        Machine::var(self, up)
    }

    fn push(&self, slot: Slot) -> Env {
        Env(Some(Rc::new(Frame { parent: self.clone(), slot })))
    }

    fn up(&self, mut n: usize) -> &Env {
        let mut env = self;
        while n > 0 {
            env = &env.0.as_ref().expect("a frame").parent;
            n -= 1;
        }
        env
    }

    fn slot(&self, up: usize) -> &Slot {
        &self.up(up).0.as_ref().expect("a frame").slot
    }
}

impl Drop for Env {
    /// A long chain is dropped one frame at a time, not by recursion.
    fn drop(&mut self) {
        let mut next = self.0.take();
        while let Some(rc) = next {
            match Rc::try_unwrap(rc) {
                Ok(mut frame) => next = frame.parent.0.take(),
                Err(_) => break,
            }
        }
    }
}

/// A path being followed: a chain from the last key to the first.
type PathList = Option<Rc<PathNode>>;

struct PathNode {
    parent: PathList,
    key: Value,
}

fn path_to_vec(mut path: &PathList) -> Vec<Value> {
    let mut keys = Vec::new();
    while let Some(node) = path {
        keys.push(node.key.clone());
        path = &node.parent;
    }
    keys.reverse();
    keys
}

fn path_push(path: &PathList, key: Value) -> PathList {
    Some(Rc::new(PathNode { parent: path.clone(), key }))
}

/// Whether a value carries a path.
#[derive(Clone)]
enum Track {
    Off,
    Path(PathList),
    /// Made by a filter that is not a path: where the path stood, and the value there (jq lets a
    /// path go on from a value identical to it). Behind an `Rc`, so that a track stays two words.
    Broken(Rc<(PathList, Value)>),
}

/// Whether two values are the same one, as jq's `jv_identical` tells: the same `null`, `false`,
/// `true` or computed number, or the same literal, text, array or object in memory. A whole number
/// read is held by its value here and in jq is a value of its own each time: never the same one.
fn identical(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) | (Value::False, Value::False) | (Value::True, Value::True) => true,
        (Value::Number(x), Value::Number(y)) => x.to_bits() == y.to_bits(),
        (Value::Literal(x), Value::Literal(y)) => Rc::ptr_eq(x, y),
        (Value::String(x), Value::String(y)) => Rc::ptr_eq(x, y),
        (Value::Array(x), Value::Array(y)) => Rc::ptr_eq(x, y),
        (Value::Object(x), Value::Object(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

impl Track {
    /// The track of a value made by a filter that is not a path, from `input`.
    fn broken(&self, input: &Value) -> Track {
        match self {
            Track::Off => Track::Off,
            Track::Path(p) => Track::Broken(Rc::new((p.clone(), input.clone()))),
            Track::Broken(..) => self.clone(),
        }
    }

    /// The path where `value` stands, if it stands on one.
    fn intact(&self, value: &Value) -> Option<PathList> {
        match self {
            Track::Off => None,
            Track::Path(p) => Some(p.clone()),
            Track::Broken(b) => identical(value, &b.1).then(|| b.0.clone()),
        }
    }

    /// The track of `container[key]`.
    fn extend(&self, container: &Value, key: &Value) -> Result<Track, Value> {
        match self {
            Track::Off => Ok(Track::Off),
            _ => match self.intact(container) {
                Some(p) => Ok(Track::Path(path_push(&p, key.clone()))),
                None => Err(error(format!(
                    "Invalid path expression near attempt to access element {} of {}",
                    crate::output::to_json(key),
                    crate::output::to_json(container)
                ))),
            },
        }
    }
}

/// A continuation: what is left to do with a value; `None` is the end (write it out).
#[derive(Clone, Default)]
struct Kont(Option<Rc<K>>);

impl Kont {
    fn new(k: K) -> Kont {
        Kont(Some(Rc::new(k)))
    }

    /// The frame, taken when held only here, else copied.
    fn open(mut self) -> Option<K> {
        let rc = self.0.take()?;
        Some(Rc::try_unwrap(rc).unwrap_or_else(|rc| (*rc).clone()))
    }
}

impl Drop for Kont {
    /// A long chain is dropped one frame at a time, not by recursion.
    fn drop(&mut self) {
        let mut next = self.0.take();
        while let Some(rc) = next {
            match Rc::try_unwrap(rc) {
                Ok(mut frame) => next = frame.next_mut().and_then(|k| k.0.take()),
                Err(_) => break,
            }
        }
    }
}

type State = Rc<RefCell<Option<Value>>>;

/// What an assignment does with the value at each path.
enum How {
    /// `lhs |= f`
    Filter(&'static Ir, Env),
    /// `lhs op= v`
    Arithmetic(Op, Value),
    /// `lhs //= v`
    Alternative(Value),
    /// `lhs = v`
    Set(Value),
}

/// An assignment under way: the paths of its left, then the value they are set on, held here alone.
struct Update {
    value: Value,
    paths: Vec<Vec<Value>>,
    at: usize,
    deletes: Vec<Value>,
    how: How,
    track: Track,
    next: Kont,
}

type UpdateState = Rc<RefCell<Update>>;

#[derive(Clone)]
enum K {
    Pipe { rhs: &'static Ir, env: Env, next: Kont },
    BinaryRight { op: Op, lhs: &'static Ir, input: Value, env: Env, track: Track, next: Kont },
    BinaryLeft { op: Op, right: Value, track: Track, next: Kont },
    Neg { track: Track, next: Kont },
    AndLeft { rhs: &'static Ir, input: Value, env: Env, track: Track, next: Kont },
    OrLeft { rhs: &'static Ir, input: Value, env: Env, track: Track, next: Kont },
    Truth { track: Track, next: Kont },
    If { then: &'static Ir, otherwise: &'static Ir, input: Value, env: Env, track: Track, next: Kont },
    As { body: &'static Ir, input: Value, env: Env, track: Track, next: Kont },
    IndexKey { target: &'static Ir, optional: bool, input: Value, env: Env, track: Track, next: Kont },
    IndexApply { key: Value, optional: bool, next: Kont },
    SliceFrom { target: &'static Ir, to: &'static Ir, optional: bool, input: Value, env: Env, track: Track, next: Kont },
    SliceTo { target: &'static Ir, from: Value, optional: bool, input: Value, env: Env, track: Track, next: Kont },
    SliceApply { from: Value, to: Value, optional: bool, next: Kont },
    IterateApply { optional: bool, next: Kont },
    TryOut { next: Kont },
    AlternativeOut { found: Rc<Cell<bool>>, next: Kont },
    Collect { items: Rc<RefCell<Vec<Value>>> },
    ObjectKey { entries: &'static [(Ir, Ir)], at: usize, map: Rc<Map>, input: Value, env: Env, track: Track, next: Kont },
    ObjectValue { entries: &'static [(Ir, Ir)], at: usize, map: Rc<Map>, key: Key, input: Value, env: Env, track: Track, next: Kont },
    ReduceInit { source: &'static Ir, update: &'static Ir, input: Value, env: Env, track: Track, next: Kont },
    ReduceItem { update: &'static Ir, env: Env, state: State },
    ReduceOut { state: State },
    ForeachInit { source: &'static Ir, update: &'static Ir, extract: Option<&'static Ir>, input: Value, env: Env, track: Track, next: Kont },
    ForeachItem { update: &'static Ir, extract: Option<&'static Ir>, env: Env, state: State, track: Track, next: Kont },
    ForeachOut { extract: Option<&'static Ir>, env: Env, state: State, track: Track, next: Kont },
    Retrack { track: Track, next: Kont },
    Apply { native: Native, track: Track, next: Kont },
    Raise,
    NativeArg { native: Native, args: &'static [Ir], at: usize, values: Vec<Value>, input: Value, env: Env, track: Track, next: Kont },
    PathOut { track: Track, next: Kont },
    GetPath { input: Value, track: Track, next: Kont },
    LimitCount { f: &'static Ir, input: Value, env: Env, track: Track, next: Kont },
    LimitOut { left: Rc<Cell<f64>>, height: usize, next: Kont },
    IsEmptyOut { height: usize, track: Track, next: Kont },
    LastOut { state: State },
    RangeFrom { upto: &'static Ir, input: Value, env: Env, track: Track, next: Kont },
    RangeTo { from: Value, track: Track, next: Kont },
    AssignValue { lhs: &'static Ir, input: Value, env: Env, track: Track, next: Kont },
    UpdateValue { how: Option<Op>, lhs: &'static Ir, input: Value, env: Env, track: Track, next: Kont },
    UpdatePath { state: UpdateState },
    UpdateOut { path: Value, state: UpdateState, height: usize },
}

impl K {
    fn next_mut(&mut self) -> Option<&mut Kont> {
        use K::*;
        match self {
            Pipe { next, .. }
            | BinaryRight { next, .. }
            | BinaryLeft { next, .. }
            | Neg { next, .. }
            | AndLeft { next, .. }
            | OrLeft { next, .. }
            | Truth { next, .. }
            | If { next, .. }
            | As { next, .. }
            | IndexKey { next, .. }
            | IndexApply { next, .. }
            | SliceFrom { next, .. }
            | SliceTo { next, .. }
            | SliceApply { next, .. }
            | IterateApply { next, .. }
            | TryOut { next, .. }
            | AlternativeOut { next, .. }
            | ObjectKey { next, .. }
            | ObjectValue { next, .. }
            | ReduceInit { next, .. }
            | ForeachInit { next, .. }
            | ForeachItem { next, .. }
            | ForeachOut { next, .. }
            | Retrack { next, .. }
            | Apply { next, .. }
            | NativeArg { next, .. }
            | PathOut { next, .. }
            | GetPath { next, .. }
            | LimitCount { next, .. }
            | LimitOut { next, .. }
            | IsEmptyOut { next, .. }
            | RangeFrom { next, .. }
            | RangeTo { next, .. }
            | AssignValue { next, .. }
            | UpdateValue { next, .. } => Some(next),
            Collect { .. } | ReduceItem { .. } | ReduceOut { .. } | Raise | LastOut { .. }
            | UpdatePath { .. } | UpdateOut { .. } => None,
        }
    }
}

/// A point to go back to.
enum Choice {
    /// A filter still to run (the right of `,`).
    Eval(&'static Ir, Value, Track, Env, Kont),
    /// The next items of an array or members of an object.
    Iterate { container: Value, at: usize, track: Track, next: Kont },
    /// The next values of `..`: the containers being walked, each with where it is.
    Recurse { stack: Vec<(Value, usize, Track)>, next: Kont },
    Range { at: f64, end: f64, track: Track, next: Kont },
    Inputs { track: Track, next: Kont },
    /// When the filter inside has no more outputs: the array of them.
    Collect { items: Rc<RefCell<Vec<Value>>>, track: Track, next: Kont },
    ReduceEnd { state: State, track: Track, next: Kont },
    /// The update of one item has no more outputs: `null` if it had none.
    ReduceStep { state: State },
    Last { state: State, track: Track, next: Kont },
    IsEmpty { track: Track, next: Kont },
    Alternative { rhs: &'static Ir, input: Value, track: Track, env: Env, next: Kont, found: Rc<Cell<bool>> },
    /// The body of a `?` began here: an error raised inside it stops here.
    TryBody,
    /// A value left the body of a `?` here: an error raised after it is not the body's.
    TryLeft,
    /// The update of a path had no output: the path is deleted.
    UpdateNone { path: Value, state: UpdateState },
    /// The left of an assignment has given all its paths: they are set now.
    UpdatePaths { state: UpdateState },
    /// The update of one path is done: on to the next.
    UpdateNext { state: UpdateState },
}

enum Step {
    Eval(&'static Ir, Value, Track, Env, Kont),
    Return(Value, Track, Kont),
    Backtrack,
    Raise(Value),
}

fn result(r: Result<Value, Value>, track: Track, next: Kont) -> Step {
    match r {
        Ok(v) => Step::Return(v, track, next),
        Err(e) => Step::Raise(e),
    }
}

/// What the filter reads when it asks for input (`input`, `inputs`).
pub trait Inputs {
    fn next_input(&mut self) -> Option<Result<Value, String>>;
}

pub struct Machine {
    defs: &'static [DefIr],
    choices: Vec<Choice>,
}

impl Machine {
    pub fn new(defs: &'static [DefIr]) -> Machine {
        Machine { defs, choices: Vec::new() }
    }

    /// The variables given, as the outermost frames.
    pub fn env(vars: Vec<Value>) -> Env {
        let mut env = Env::default();
        for v in vars {
            env = env.push(Slot::Var(v));
        }
        env
    }

    /// Runs the filter on one input, giving each output to `emit` as it comes. An error not caught
    /// ends the run.
    pub fn run(
        &mut self,
        ir: &'static Ir,
        input: Value,
        env: &Env,
        inputs: &mut dyn Inputs,
        emit: &mut dyn FnMut(Value),
    ) -> Result<(), Value> {
        self.choices.clear();
        let mut step = Step::Eval(ir, input, Track::Off, env.clone(), Kont::default());
        loop {
            step = match step {
                Step::Eval(ir, input, track, env, next) => self.eval(ir, input, track, env, next, inputs),
                Step::Return(value, track, next) => match next.open() {
                    None => {
                        emit(value);
                        Step::Backtrack
                    }
                    Some(k) => self.resume(k, value, track, inputs),
                },
                Step::Backtrack => match self.choices.pop() {
                    None => return Ok(()),
                    Some(choice) => self.retry(choice, inputs),
                },
                Step::Raise(e) => {
                    if self.unwind() {
                        Step::Backtrack
                    } else {
                        self.choices.clear();
                        return Err(e);
                    }
                }
            };
        }
    }

    /// Goes back to the `?` the error was raised in; false when there is none.
    fn unwind(&mut self) -> bool {
        let mut left = 0;
        while let Some(choice) = self.choices.pop() {
            match choice {
                Choice::TryLeft => left += 1,
                Choice::TryBody if left == 0 => return true,
                Choice::TryBody => left -= 1,
                _ => {}
            }
        }
        false
    }

    fn var(env: &Env, up: usize) -> Value {
        match env.slot(up) {
            Slot::Var(v) => v.clone(),
            Slot::Params(_) => unreachable!("a variable"),
        }
    }

    fn closure(env: &Env, up: usize, index: usize) -> &Closure {
        match env.slot(up) {
            Slot::Params(closures) => &closures[index],
            Slot::Var(_) => unreachable!("a parameter"),
        }
    }

    /// The output of a node of a single output, worked out at once (`single.rs`); `None` for any
    /// other node, which the machine runs.
    fn quick(ir: &Ir, input: &Value, env: &Env) -> Option<Result<Value, Value>> {
        match ir {
            Ir::Literal(v) => Some(Ok(v.clone())),
            Ir::Var(up) => Some(Ok(Self::var(env, *up))),
            Ir::Identity | Ir::Single(_) => Some(single::eval(ir, input.clone(), env)),
            _ => None,
        }
    }

    fn eval(&mut self, ir: &'static Ir, input: Value, track: Track, env: Env, k: Kont, inputs: &mut dyn Inputs) -> Step {
        match ir {
            Ir::Identity => Step::Return(input, track, k),
            // Off a path, a node of a single output is worked out at once; on one (in `path(f)` and
            // left of an assignment) the machine follows it, to keep the path.
            Ir::Single(inner) => match track {
                Track::Off => result(single::eval(inner, input, &env), track, k),
                _ => Step::Eval(inner, input, track, env, k),
            },
            Ir::Format(native, f) => {
                let track = track.broken(&input);
                Step::Eval(f, input, Track::Off, env, Kont::new(K::Apply { native: *native, track, next: k }))
            }
            Ir::Recurse => {
                if has_items(&input) {
                    self.choices.push(Choice::Recurse { stack: vec![(input.clone(), 0, track.clone())], next: k.clone() });
                }
                Step::Return(input, track, k)
            }
            Ir::Literal(v) => Step::Return(v.clone(), track.broken(&input), k),
            Ir::Var(up) => Step::Return(Self::var(&env, *up), track.broken(&input), k),
            Ir::Field(target, key, optional) => {
                let optional = *optional;
                if let Ir::Identity = **target {
                    return self.index(input, track, key.clone(), optional, k);
                }
                Step::Eval(target, input, track, env, Kont::new(K::IndexApply { key: key.clone(), optional, next: k }))
            }
            Ir::Index(target, key, optional) => {
                let optional = *optional;
                match Self::quick(key, &input, &env) {
                    Some(Err(e)) => Step::Raise(e),
                    Some(Ok(key)) => {
                        if let Ir::Identity = **target {
                            return self.index(input, track, key, optional, k);
                        }
                        Step::Eval(target, input, track, env, Kont::new(K::IndexApply { key, optional, next: k }))
                    }
                    None => {
                        let next = Kont::new(K::IndexKey {
                            target,
                            optional,
                            input: input.clone(),
                            env: env.clone(),
                            track,
                            next: k,
                        });
                        Step::Eval(key, input, Track::Off, env, next)
                    }
                }
            }
            Ir::Slice(target, from, to, optional) => {
                let optional = *optional;
                match Self::quick(from, &input, &env) {
                    Some(Err(e)) => Step::Raise(e),
                    Some(Ok(from)) => self.slice_to(target, to, from, optional, input, env, track, k),
                    None => {
                        let next = Kont::new(K::SliceFrom {
                            target,
                            to,
                            optional,
                            input: input.clone(),
                            env: env.clone(),
                            track,
                            next: k,
                        });
                        Step::Eval(from, input, Track::Off, env, next)
                    }
                }
            }
            Ir::Iterate(target, optional) => {
                let optional = *optional;
                if let Ir::Identity = **target {
                    return self.iterate(input, track, optional, k);
                }
                Step::Eval(target, input, track, env, Kont::new(K::IterateApply { optional, next: k }))
            }
            Ir::Try(body) => {
                self.choices.push(Choice::TryBody);
                Step::Eval(body, input, track, env, Kont::new(K::TryOut { next: k }))
            }
            Ir::Collect(body) => {
                let items = Rc::new(RefCell::new(Vec::new()));
                self.choices.push(Choice::Collect { items: items.clone(), track: track.broken(&input), next: k });
                Step::Eval(body, input, Track::Off, env, Kont::new(K::Collect { items }))
            }
            Ir::Object(entries) => {
                let track = track.broken(&input);
                self.object(entries, 0, Rc::new(Map::with_capacity(entries.len())), input, env, track, k)
            }
            Ir::Neg(a) => {
                let track = track.broken(&input);
                match Self::quick(a, &input, &env) {
                    Some(Err(e)) => Step::Raise(e),
                    Some(Ok(v)) => result(negate(v), track, k),
                    None => Step::Eval(a, input, Track::Off, env, Kont::new(K::Neg { track, next: k })),
                }
            }
            Ir::Pipe(a, b) => {
                if let Track::Off = track
                    && let Some(v) = Self::quick(a, &input, &env)
                {
                    return match v {
                        Ok(v) => Step::Eval(b, v, track, env, k),
                        Err(e) => Step::Raise(e),
                    };
                }
                let next = Kont::new(K::Pipe { rhs: b, env: env.clone(), next: k });
                Step::Eval(a, input, track, env, next)
            }
            Ir::Comma(a, b) => {
                self.choices.push(Choice::Eval(b, input.clone(), track.clone(), env.clone(), k.clone()));
                Step::Eval(a, input, track, env, k)
            }
            Ir::Alternative(a, b) => {
                let found = Rc::new(Cell::new(false));
                self.choices.push(Choice::Alternative {
                    rhs: b,
                    input: input.clone(),
                    track: track.clone(),
                    env: env.clone(),
                    next: k.clone(),
                    found: found.clone(),
                });
                Step::Eval(a, input, track, env, Kont::new(K::AlternativeOut { found, next: k }))
            }
            Ir::And(a, b) | Ir::Or(a, b) => {
                let track = track.broken(&input);
                let next = if let Ir::And(..) = ir {
                    K::AndLeft { rhs: b, input: input.clone(), env: env.clone(), track, next: k }
                } else {
                    K::OrLeft { rhs: b, input: input.clone(), env: env.clone(), track, next: k }
                };
                Step::Eval(a, input, Track::Off, env, Kont::new(next))
            }
            Ir::Binary(op, a, b) => {
                let track = track.broken(&input);
                match Self::quick(b, &input, &env) {
                    Some(Err(e)) => Step::Raise(e),
                    Some(Ok(right)) => self.binary_left(*op, a, right, input, env, track, k),
                    None => {
                        let next = Kont::new(K::BinaryRight {
                            op: *op,
                            lhs: a,
                            input: input.clone(),
                            env: env.clone(),
                            track,
                            next: k,
                        });
                        Step::Eval(b, input, Track::Off, env, next)
                    }
                }
            }
            Ir::If(c, then, otherwise) => match Self::quick(c, &input, &env) {
                Some(Err(e)) => Step::Raise(e),
                Some(Ok(v)) => Step::Eval(if v.truthy() { then } else { otherwise }, input, track, env, k),
                None => {
                    let next =
                        Kont::new(K::If { then, otherwise, input: input.clone(), env: env.clone(), track, next: k });
                    Step::Eval(c, input, Track::Off, env, next)
                }
            },
            Ir::Reduce(source, init, update) => {
                let track = track.broken(&input);
                let next = Kont::new(K::ReduceInit { source, update, input: input.clone(), env: env.clone(), track, next: k });
                Step::Eval(init, input, Track::Off, env, next)
            }
            Ir::Foreach(source, init, update, extract) => {
                let track = track.broken(&input);
                let extract = extract.as_deref();
                let next = Kont::new(K::ForeachInit {
                    source,
                    update,
                    extract,
                    input: input.clone(),
                    env: env.clone(),
                    track,
                    next: k,
                });
                Step::Eval(init, input, Track::Off, env, next)
            }
            Ir::As(source, body) => {
                let next = Kont::new(K::As { body, input: input.clone(), env: env.clone(), track, next: k });
                Step::Eval(source, input, Track::Off, env, next)
            }
            Ir::CallDef { def, up, args } => {
                let def = &self.defs[*def];
                let base = env.up(*up).clone();
                let env = if args.is_empty() {
                    base
                } else {
                    let closures = args
                        .iter()
                        .map(|arg| match arg {
                            // A parameter passed on is passed as it is, so that recursion does not
                            // grow the chain of frames.
                            Ir::CallParam { up, index } => Self::closure(&env, *up, *index).clone(),
                            _ => Closure { ir: arg, env: env.clone() },
                        })
                        .collect();
                    base.push(Slot::Params(closures))
                };
                Step::Eval(&def.body, input, track, env, k)
            }
            Ir::CallParam { up, index } => {
                let closure = Self::closure(&env, *up, *index).clone();
                Step::Eval(closure.ir, input, track, closure.env, k)
            }
            Ir::Empty => Step::Backtrack,
            Ir::Error(None) => Step::Raise(input),
            Ir::Error(Some(message)) => Step::Eval(message, input, Track::Off, env, Kont::new(K::Raise)),
            Ir::TypeFilter(mask) => {
                if input.kind_bit() & mask != 0 {
                    Step::Return(input, track, k)
                } else {
                    Step::Backtrack
                }
            }
            Ir::Native(native, args) => {
                let track = track.broken(&input);
                if args.is_empty() {
                    return result(native.call(input, &[]), track, k);
                }
                self.native_arg(*native, args, args.len(), Vec::with_capacity(args.len()), input, env, track, k)
            }
            Ir::Path(f) => {
                let out = track.broken(&input);
                Step::Eval(f, input, Track::Path(None), env, Kont::new(K::PathOut { track: out, next: k }))
            }
            Ir::GetPath(p) => {
                let next = Kont::new(K::GetPath { input: input.clone(), track, next: k });
                Step::Eval(p, input, Track::Off, env, next)
            }
            Ir::Limit(n, f) => {
                let next = Kont::new(K::LimitCount { f, input: input.clone(), env: env.clone(), track, next: k });
                Step::Eval(n, input, Track::Off, env, next)
            }
            Ir::First(f) => {
                let height = self.choices.len();
                let next = Kont::new(K::LimitOut { left: Rc::new(Cell::new(1.0)), height, next: k });
                Step::Eval(f, input, track, env, next)
            }
            Ir::Last(f) => {
                let state: State = Rc::new(RefCell::new(None));
                self.choices.push(Choice::Last { state: state.clone(), track: track.broken(&input), next: k });
                Step::Eval(f, input, Track::Off, env, Kont::new(K::LastOut { state }))
            }
            Ir::IsEmpty(g) => {
                let height = self.choices.len();
                let track = track.broken(&input);
                self.choices.push(Choice::IsEmpty { track: track.clone(), next: k.clone() });
                Step::Eval(g, input, Track::Off, env, Kont::new(K::IsEmptyOut { height, track, next: k }))
            }
            Ir::Range(from, upto) => {
                let next = Kont::new(K::RangeFrom { upto, input: input.clone(), env: env.clone(), track, next: k });
                Step::Eval(from, input, Track::Off, env, next)
            }
            Ir::Input => {
                let track = track.broken(&input);
                match inputs.next_input() {
                    Some(Ok(v)) => Step::Return(v, track, k),
                    Some(Err(message)) => Step::Raise(error(message)),
                    None => Step::Raise(error("No more inputs")),
                }
            }
            Ir::Inputs => self.inputs(track.broken(&input), k, inputs),
            Ir::Assign(lhs, rhs) => {
                let track = track.broken(&input);
                let next = Kont::new(K::AssignValue { lhs, input: input.clone(), env: env.clone(), track, next: k });
                Step::Eval(rhs, input, Track::Off, env, next)
            }
            Ir::Update(lhs, f) => {
                let track = track.broken(&input);
                self.update(lhs, How::Filter(f, env.clone()), input, env, track, k)
            }
            Ir::ArithmeticUpdate(op, lhs, rhs) => {
                let track = track.broken(&input);
                let next =
                    Kont::new(K::UpdateValue { how: Some(*op), lhs, input: input.clone(), env: env.clone(), track, next: k });
                Step::Eval(rhs, input, Track::Off, env, next)
            }
            Ir::AlternativeUpdate(lhs, rhs) => {
                let track = track.broken(&input);
                let next =
                    Kont::new(K::UpdateValue { how: None, lhs, input: input.clone(), env: env.clone(), track, next: k });
                Step::Eval(rhs, input, Track::Off, env, next)
            }
        }
    }

    /// What a continuation does with a value that has come to it.
    fn resume(&mut self, k: K, value: Value, vtrack: Track, _inputs: &mut dyn Inputs) -> Step {
        match k {
            K::Pipe { rhs, env, next } => Step::Eval(rhs, value, vtrack, env, next),
            K::BinaryRight { op, lhs, input, env, track, next } => self.binary_left(op, lhs, value, input, env, track, next),
            K::BinaryLeft { op, right, track, next } => result(binary(op, value, right), track, next),
            K::Neg { track, next } => result(negate(value), track, next),
            K::AndLeft { rhs, input, env, track, next } => {
                if !value.truthy() {
                    return Step::Return(Value::False, track, next);
                }
                Step::Eval(rhs, input, Track::Off, env, Kont::new(K::Truth { track, next }))
            }
            K::OrLeft { rhs, input, env, track, next } => {
                if value.truthy() {
                    return Step::Return(Value::True, track, next);
                }
                Step::Eval(rhs, input, Track::Off, env, Kont::new(K::Truth { track, next }))
            }
            K::Truth { track, next } => Step::Return(Value::from_bool(value.truthy()), track, next),
            K::If { then, otherwise, input, env, track, next } => {
                Step::Eval(if value.truthy() { then } else { otherwise }, input, track, env, next)
            }
            K::As { body, input, env, track, next } => {
                let env = env.push(Slot::Var(value));
                Step::Eval(body, input, track, env, next)
            }
            K::IndexKey { target, optional, input, env, track, next } => {
                if let Ir::Identity = *target {
                    return self.index(input, track, value, optional, next);
                }
                Step::Eval(target, input, track, env, Kont::new(K::IndexApply { key: value, optional, next }))
            }
            K::IndexApply { key, optional, next } => self.index(value, vtrack, key, optional, next),
            K::SliceFrom { target, to, optional, input, env, track, next } => {
                self.slice_to(target, to, value, optional, input, env, track, next)
            }
            K::SliceTo { target, from, optional, input, env, track, next } => {
                Step::Eval(target, input, track, env, Kont::new(K::SliceApply { from, to: value, optional, next }))
            }
            K::SliceApply { from, to, optional, next } => self.slice(value, vtrack, from, to, optional, next),
            K::IterateApply { optional, next } => self.iterate(value, vtrack, optional, next),
            K::TryOut { next } => {
                self.choices.push(Choice::TryLeft);
                Step::Return(value, vtrack, next)
            }
            K::AlternativeOut { found, next } => {
                if value.truthy() {
                    found.set(true);
                    Step::Return(value, vtrack, next)
                } else {
                    Step::Backtrack
                }
            }
            K::Collect { items } => {
                items.borrow_mut().push(value);
                Step::Backtrack
            }
            K::ObjectKey { entries, at, map, input, env, track, next } => match value {
                Value::String(key) => {
                    self.object_value(entries, at, map, Key(key), input, env, track, next)
                }
                other => Step::Raise(error(format!("Object keys must be strings, not {}", builtins::describe(&other)))),
            },
            K::ObjectValue { entries, at, mut map, key, input, env, track, next } => {
                Rc::make_mut(&mut map).insert(key, value);
                self.object(entries, at + 1, map, input, env, track, next)
            }
            K::ReduceInit { source, update, input, env, track, next } => {
                let state: State = Rc::new(RefCell::new(Some(value)));
                self.choices.push(Choice::ReduceEnd { state: state.clone(), track, next });
                let item = Kont::new(K::ReduceItem { update, env: env.clone(), state });
                Step::Eval(source, input, Track::Off, env, item)
            }
            K::ReduceItem { update, env, state } => {
                let env = env.push(Slot::Var(value));
                let acc = state.borrow_mut().take().unwrap_or(Value::Null);
                if matches!(update, Ir::Identity | Ir::Literal(_) | Ir::Var(_) | Ir::Single(_)) {
                    // An update of a single output, worked out at once on the state it is handed.
                    return match single::eval(update, acc, &env) {
                        Ok(v) => {
                            *state.borrow_mut() = Some(v);
                            Step::Backtrack
                        }
                        Err(e) => Step::Raise(e),
                    };
                }
                self.choices.push(Choice::ReduceStep { state: state.clone() });
                Step::Eval(update, acc, Track::Off, env, Kont::new(K::ReduceOut { state }))
            }
            K::ReduceOut { state } => {
                *state.borrow_mut() = Some(value);
                Step::Backtrack
            }
            K::ForeachInit { source, update, extract, input, env, track, next } => {
                let state: State = Rc::new(RefCell::new(Some(value)));
                let item = Kont::new(K::ForeachItem { update, extract, env: env.clone(), state, track, next });
                Step::Eval(source, input, Track::Off, env, item)
            }
            K::ForeachItem { update, extract, env, state, track, next } => {
                let env = env.push(Slot::Var(value));
                let acc = state.borrow_mut().take().unwrap_or(Value::Null);
                let out = Kont::new(K::ForeachOut { extract, env: env.clone(), state, track, next });
                Step::Eval(update, acc, Track::Off, env, out)
            }
            K::ForeachOut { extract, env, state, track, next } => {
                *state.borrow_mut() = Some(value.clone());
                match extract {
                    Some(extract) => Step::Eval(extract, value, Track::Off, env, retrack(track, next)),
                    None => Step::Return(value, track, next),
                }
            }
            K::Retrack { track, next } => Step::Return(value, track, next),
            K::Apply { native, track, next } => result(native.call(value, &[]), track, next),
            K::Raise => Step::Raise(value),
            K::NativeArg { native, args, at, mut values, input, env, track, next } => {
                values.push(value);
                self.native_arg(native, args, at, values, input, env, track, next)
            }
            K::PathOut { track, next } => match vtrack.intact(&value) {
                Some(path) => Step::Return(Value::array(path_to_vec(&path)), track, next),
                None => Step::Raise(error(format!(
                    "Invalid path expression with result {}",
                    crate::output::to_json(&value)
                ))),
            },
            K::GetPath { input, track, next } => {
                let Value::Array(keys) = &value else {
                    return Step::Raise(error("Path must be specified as an array"));
                };
                let found = match get_path(&input, keys) {
                    Ok(v) => v,
                    Err(e) => return Step::Raise(e),
                };
                let track = match (&track, &value) {
                    (Track::Off, _) => Track::Off,
                    (_, Value::Array(keys)) => match track.intact(&input) {
                        Some(mut p) => {
                            for key in keys.iter() {
                                p = path_push(&p, key.clone());
                            }
                            Track::Path(p)
                        }
                        None => {
                            return Step::Raise(error(format!(
                                "Invalid path expression with result {}",
                                crate::output::to_json(&input)
                            )));
                        }
                    },
                    _ => Track::Off,
                };
                Step::Return(found, track, next)
            }
            K::LimitCount { f, input, env, track, next } => {
                let Some(n) = value.number() else {
                    return Step::Raise(error(format!("{} cannot be used as a count", builtins::describe(&value))));
                };
                if n > 0.0 {
                    let height = self.choices.len();
                    let out = Kont::new(K::LimitOut { left: Rc::new(Cell::new(n)), height, next });
                    Step::Eval(f, input, track, env, out)
                } else if n == 0.0 {
                    Step::Backtrack
                } else {
                    Step::Raise(error("limit doesn't support negative count"))
                }
            }
            K::LimitOut { left, height, next } => {
                let n = left.get() - 1.0;
                left.set(n);
                if n <= 0.0 {
                    self.choices.truncate(height);
                }
                Step::Return(value, vtrack, next)
            }
            K::IsEmptyOut { height, track, next } => {
                self.choices.truncate(height);
                Step::Return(Value::False, track, next)
            }
            K::LastOut { state } => {
                *state.borrow_mut() = Some(value);
                Step::Backtrack
            }
            K::RangeFrom { upto, input, env, track, next } => match Self::quick(upto, &input, &env) {
                Some(Err(e)) => Step::Raise(e),
                Some(Ok(to)) => self.range(value, to, track.broken(&input), next),
                None => {
                    let out = Kont::new(K::RangeTo { from: value, track: track.broken(&input), next });
                    Step::Eval(upto, input, Track::Off, env, out)
                }
            },
            K::RangeTo { from, track, next } => self.range(from, value, track, next),
            K::AssignValue { lhs, input, env, track, next } => self.update(lhs, How::Set(value), input, env, track, next),
            K::UpdateValue { how, lhs, input, env, track, next } => {
                let how = match how {
                    Some(op) => How::Arithmetic(op, value),
                    None => How::Alternative(value),
                };
                self.update(lhs, how, input, env, track, next)
            }
            K::UpdatePath { state } => {
                let Some(path) = vtrack.intact(&value) else {
                    return Step::Raise(invalid_path(&value));
                };
                state.borrow_mut().paths.push(path_to_vec(&path));
                Step::Backtrack
            }
            K::UpdateOut { path, state, height } => {
                self.choices.truncate(height);
                let Value::Array(path) = path else { unreachable!() };
                if let Err(e) = set_path(&mut state.borrow_mut().value, &path, value) {
                    return Step::Raise(e);
                }
                Step::Backtrack
            }
        }
    }

    /// Goes back to a choice.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn retry(&mut self, choice: Choice, inputs: &mut dyn Inputs) -> Step {
        match choice {
            Choice::Eval(ir, input, track, env, next) => Step::Eval(ir, input, track, env, next),
            Choice::Iterate { container, at, track, next } => self.iterate_at(container, at, track, next),
            Choice::Recurse { mut stack, next } => {
                while let Some((container, at, track)) = stack.last_mut() {
                    let item = match &*container {
                        Value::Array(items) => items.get(*at).map(|v| (v.clone(), Value::Number(*at as f64))),
                        Value::Object(map) => {
                            map.get_index(*at).map(|(k, v)| (v.clone(), Value::String(k.0.clone())))
                        }
                        _ => None,
                    };
                    let Some((value, key)) = item else {
                        stack.pop();
                        continue;
                    };
                    *at += 1;
                    let track = match track.extend(container, &key) {
                        Ok(t) => t,
                        Err(e) => return Step::Raise(e),
                    };
                    if has_items(&value) {
                        stack.push((value.clone(), 0, track.clone()));
                    }
                    self.choices.push(Choice::Recurse { stack, next: next.clone() });
                    return Step::Return(value, track, next);
                }
                Step::Backtrack
            }
            Choice::Range { at, end, track, next } => {
                let following = at + 1.0;
                if !(following >= end) {
                    self.choices.push(Choice::Range { at: following, end, track: track.clone(), next: next.clone() });
                }
                Step::Return(Value::Number(at), track, next)
            }
            Choice::Inputs { track, next } => self.inputs(track, next, inputs),
            Choice::Collect { items, track, next } => {
                let mut items = std::mem::take(&mut *items.borrow_mut());
                trim_vec(&mut items);
                Step::Return(Value::array(items), track, next)
            }
            Choice::ReduceEnd { state, track, next } => {
                let value = state.borrow_mut().take().unwrap_or(Value::Null);
                Step::Return(value, track, next)
            }
            Choice::ReduceStep { state } => {
                let mut s = state.borrow_mut();
                if s.is_none() {
                    *s = Some(Value::Null);
                }
                Step::Backtrack
            }
            Choice::Last { state, track, next } => match state.borrow_mut().take() {
                Some(value) => Step::Return(value, track, next),
                None => Step::Backtrack,
            },
            Choice::IsEmpty { track, next } => Step::Return(Value::True, track, next),
            Choice::Alternative { rhs, input, track, env, next, found } => {
                if found.get() {
                    Step::Backtrack
                } else {
                    Step::Eval(rhs, input, track, env, next)
                }
            }
            Choice::TryBody | Choice::TryLeft => Step::Backtrack,
            Choice::UpdateNone { path, state } => {
                state.borrow_mut().deletes.push(path);
                Step::Backtrack
            }
            Choice::UpdatePaths { state } | Choice::UpdateNext { state } => self.update_next(state),
        }
    }

    /// `container[key]`; when `optional`, its failure gives no output.
    fn index(&mut self, container: Value, track: Track, key: Value, optional: bool, next: Kont) -> Step {
        let track = match track.extend(&container, &key) {
            Ok(t) => t,
            Err(e) => return Step::Raise(e),
        };
        match builtins::index(&container, &key) {
            Ok(v) => Step::Return(v, track, next),
            Err(_) if optional => Step::Backtrack,
            Err(e) => Step::Raise(e),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn slice_to(
        &mut self,
        target: &'static Ir,
        to: &'static Ir,
        from: Value,
        optional: bool,
        input: Value,
        env: Env,
        track: Track,
        next: Kont,
    ) -> Step {
        match Self::quick(to, &input, &env) {
            Some(Err(e)) => Step::Raise(e),
            Some(Ok(to)) => {
                if let Ir::Identity = target {
                    return self.slice(input, track, from, to, optional, next);
                }
                Step::Eval(target, input, track, env, Kont::new(K::SliceApply { from, to, optional, next }))
            }
            None => {
                let out = Kont::new(K::SliceTo {
                    target,
                    from,
                    optional,
                    input: input.clone(),
                    env: env.clone(),
                    track,
                    next,
                });
                Step::Eval(to, input, Track::Off, env, out)
            }
        }
    }

    fn slice(&mut self, container: Value, track: Track, from: Value, to: Value, optional: bool, next: Kont) -> Step {
        let mut bounds = Map::with_capacity(2);
        bounds.insert(Key(Rc::new("start".into())), from.clone());
        bounds.insert(Key(Rc::new("end".into())), to.clone());
        let track = match track.extend(&container, &Value::Object(Rc::new(bounds))) {
            Ok(t) => t,
            Err(e) => return Step::Raise(e),
        };
        match slice(&container, &from, &to) {
            Ok(v) => Step::Return(v, track, next),
            Err(_) if optional => Step::Backtrack,
            Err(e) => Step::Raise(e),
        }
    }

    fn iterate(&mut self, container: Value, track: Track, optional: bool, next: Kont) -> Step {
        match &container {
            Value::Array(_) | Value::Object(_) => self.iterate_at(container, 0, track, next),
            _ if optional => Step::Backtrack,
            other => Step::Raise(error(format!("Cannot iterate over {}", builtins::describe(other)))),
        }
    }

    fn iterate_at(&mut self, container: Value, at: usize, track: Track, next: Kont) -> Step {
        let (item, key, length) = match &container {
            Value::Array(items) => match items.get(at) {
                Some(v) => (v.clone(), Value::Number(at as f64), items.len()),
                None => return Step::Backtrack,
            },
            Value::Object(map) => match map.get_index(at) {
                Some((k, v)) => (v.clone(), Value::String(k.0.clone()), map.len()),
                None => return Step::Backtrack,
            },
            _ => return Step::Backtrack,
        };
        let item_track = match &track {
            Track::Off => Track::Off,
            _ => match track.extend(&container, &key) {
                Ok(t) => t,
                Err(e) => return Step::Raise(e),
            },
        };
        if at + 1 < length {
            self.choices.push(Choice::Iterate { container, at: at + 1, track, next: next.clone() });
        }
        Step::Return(item, item_track, next)
    }

    /// The left of `a op b`, once the right has given `right`.
    #[allow(clippy::too_many_arguments)]
    fn binary_left(
        &mut self,
        op: Op,
        lhs: &'static Ir,
        right: Value,
        input: Value,
        env: Env,
        track: Track,
        next: Kont,
    ) -> Step {
        if let Ir::Identity = lhs {
            // The input itself, taken: added to in place when held only here.
            return result(binary(op, input, right), track, next);
        }
        if let Some(left) = Self::quick(lhs, &input, &env) {
            drop(input);
            return match left {
                Ok(left) => result(binary(op, left, right), track, next),
                Err(e) => Step::Raise(e),
            };
        }
        Step::Eval(lhs, input, Track::Off, env, Kont::new(K::BinaryLeft { op, right, track, next }))
    }

    /// The members of an object from entry `at` on.
    #[allow(clippy::too_many_arguments)]
    fn object(
        &mut self,
        entries: &'static [(Ir, Ir)],
        at: usize,
        map: Rc<Map>,
        input: Value,
        env: Env,
        track: Track,
        next: Kont,
    ) -> Step {
        let Some((key, _)) = entries.get(at) else {
            return Step::Return(Value::Object(map), track, next);
        };
        match Self::quick(key, &input, &env) {
            Some(Err(e)) => Step::Raise(e),
            Some(Ok(Value::String(key))) => self.object_value(entries, at, map, Key(key), input, env, track, next),
            Some(Ok(other)) => {
                Step::Raise(error(format!("Object keys must be strings, not {}", builtins::describe(&other))))
            }
            None => {
                let out = Kont::new(K::ObjectKey { entries, at, map, input: input.clone(), env: env.clone(), track, next });
                Step::Eval(key, input, Track::Off, env, out)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn object_value(
        &mut self,
        entries: &'static [(Ir, Ir)],
        at: usize,
        mut map: Rc<Map>,
        key: Key,
        input: Value,
        env: Env,
        track: Track,
        next: Kont,
    ) -> Step {
        let value = &entries[at].1;
        match Self::quick(value, &input, &env) {
            Some(Err(e)) => Step::Raise(e),
            Some(Ok(v)) => {
                Rc::make_mut(&mut map).insert(key, v);
                self.object(entries, at + 1, map, input, env, track, next)
            }
            None => {
                let out =
                    Kont::new(K::ObjectValue { entries, at, map, key, input: input.clone(), env: env.clone(), track, next });
                Step::Eval(value, input, Track::Off, env, out)
            }
        }
    }

    /// The arguments of a built-in from the last to the first (the first varies fastest, as jq
    /// runs them); `values` holds those given, last first.
    #[allow(clippy::too_many_arguments)]
    fn native_arg(
        &mut self,
        native: Native,
        args: &'static [Ir],
        mut at: usize,
        mut values: Vec<Value>,
        input: Value,
        env: Env,
        track: Track,
        next: Kont,
    ) -> Step {
        while at > 0 {
            at -= 1;
            match Self::quick(&args[at], &input, &env) {
                Some(Err(e)) => return Step::Raise(e),
                Some(Ok(v)) => values.push(v),
                None => {
                    let out = Kont::new(K::NativeArg {
                        native,
                        args,
                        at,
                        values,
                        input: input.clone(),
                        env: env.clone(),
                        track,
                        next,
                    });
                    return Step::Eval(&args[at], input, Track::Off, env, out);
                }
            }
        }
        values.reverse();
        result(native.call(input, &values), track, next)
    }

    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn range(&mut self, from: Value, to: Value, track: Track, next: Kont) -> Step {
        let (Some(a), Some(b)) = (from.number(), to.number()) else {
            return Step::Raise(error("Range bounds must be numeric"));
        };
        // As jq's RANGE: on while the value is not at or past the end (so NaN never ends).
        if a >= b {
            return Step::Backtrack;
        }
        if !(a + 1.0 >= b) {
            self.choices.push(Choice::Range { at: a + 1.0, end: b, track: track.clone(), next: next.clone() });
        }
        Step::Return(from, track, next)
    }

    fn inputs(&mut self, track: Track, next: Kont, inputs: &mut dyn Inputs) -> Step {
        match inputs.next_input() {
            Some(Ok(v)) => {
                self.choices.push(Choice::Inputs { track: track.clone(), next: next.clone() });
                Step::Return(v, track, next)
            }
            Some(Err(message)) => Step::Raise(error(message)),
            None => Step::Backtrack,
        }
    }

    /// `lhs = v`, `lhs |= f` and the others: the paths of `lhs` on the input first, then each set in
    /// turn on the value held here alone, as jq 1.8's `_assign` and `_modify` set them; the paths `f`
    /// gives nothing for are deleted at the end.
    fn update(&mut self, lhs: &'static Ir, how: How, input: Value, env: Env, track: Track, next: Kont) -> Step {
        let update = Update { value: input.clone(), paths: Vec::new(), at: 0, deletes: Vec::new(), how, track, next };
        let state = Rc::new(RefCell::new(update));
        self.choices.push(Choice::UpdatePaths { state: state.clone() });
        Step::Eval(lhs, input, Track::Path(None), env, Kont::new(K::UpdatePath { state }))
    }

    /// Sets the paths of an assignment from the next one on; a filter that is not worked out at once
    /// is run by the machine, and the next path taken when it is done.
    fn update_next(&mut self, state: UpdateState) -> Step {
        let mut s = state.borrow_mut();
        let s = &mut *s;
        while s.at < s.paths.len() {
            let path = &s.paths[s.at];
            s.at += 1;
            if let How::Set(v) = &s.how {
                if let Err(e) = set_path(&mut s.value, path, v.clone()) {
                    return Step::Raise(e);
                }
                continue;
            }
            let old = match get_path(&s.value, path) {
                Ok(v) => v,
                Err(e) => return Step::Raise(e),
            };
            let new = match &s.how {
                How::Arithmetic(op, right) => binary(*op, old, right.clone()),
                How::Alternative(right) => Ok(if old.truthy() { old } else { right.clone() }),
                How::Filter(f, env) => match Self::quick(f, &old, env) {
                    Some(new) => new,
                    None => {
                        let (f, env) = (*f, env.clone());
                        let path = Value::array(path.clone());
                        self.choices.push(Choice::UpdateNext { state: state.clone() });
                        let height = self.choices.len();
                        self.choices.push(Choice::UpdateNone { path: path.clone(), state: state.clone() });
                        let out = Kont::new(K::UpdateOut { path, state: state.clone(), height });
                        return Step::Eval(f, old, Track::Off, env, out);
                    }
                },
                How::Set(_) => unreachable!(),
            };
            match new {
                Ok(new) => {
                    if let Err(e) = set_path(&mut s.value, path, new) {
                        return Step::Raise(e);
                    }
                }
                Err(e) => return Step::Raise(e),
            }
        }
        let value = std::mem::replace(&mut s.value, Value::Null);
        let deletes = std::mem::take(&mut s.deletes);
        let (track, next) = (s.track.clone(), std::mem::take(&mut s.next));
        if deletes.is_empty() {
            return Step::Return(value, track, next);
        }
        result(delete_paths(value, &Value::array(deletes)), track, next)
    }
}

fn has_items(v: &Value) -> bool {
    match v {
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        _ => false,
    }
}

fn negate(v: Value) -> Result<Value, Value> {
    v.negate().ok_or_else(|| error(format!("{} cannot be negated", builtins::describe(&v))))
}

fn invalid_path(v: &Value) -> Value {
    error(format!("Invalid path expression with result {}", crate::output::to_json(v)))
}

fn retrack(track: Track, next: Kont) -> Kont {
    match track {
        Track::Off => next,
        track => Kont::new(K::Retrack { track, next }),
    }
}
