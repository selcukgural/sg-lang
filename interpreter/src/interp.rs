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

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::types::{IntTy, Ty};

/// Limit on nested calls. Past it the program gets a readable panic instead of a crash.
pub const MAX_DEPTH: usize = 5000;

type Cell = Rc<RefCell<Value>>;

#[derive(Clone, Debug)]
pub enum Value {
    Int(IntTy, i64),
    Bool(bool),
    Str(Rc<str>),
    Fmt(Rc<str>),
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
    /// `read T` / `ref T`
    Borrow(Cell),
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
    Int(i64),
    Str(Rc<str>),
    Parts(Vec<Key>),
}

fn key_of(value: &Value) -> Key {
    match value {
        Value::Bool(value) => Key::Bool(*value),
        Value::Int(_, value) => Key::Int(*value),
        Value::Str(text) => Key::Str(text.clone()),
        Value::Null => Key::Null,
        Value::Struct(_, fields) => Key::Parts(fields.iter().map(|field| key_of(&field.borrow())).collect()),
        Value::Enum(_, index, fields) => {
            let parts = fields.iter().map(|field| key_of(&field.borrow()));
            Key::Parts(std::iter::once(Key::Int(*index as i64)).chain(parts).collect())
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

/// The value behind any borrows and pointers.
fn owned(mut value: Value) -> Value {
    while let Value::Borrow(target) | Value::Ptr(target) = value {
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
        Value::Str(text) => Rc::new(text.bytes().map(|byte| cell(Value::Int(IntTy::Byte, byte as i64))).collect()),
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
        Ty::Bool => Value::Bool(false),
        Ty::Nullable(_) => Value::Null,
        _ => unreachable!("the checker only lets a type with a zero value be filled this way"),
    }
}

/// The cell a `ref` or `read` variable stands for.
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
            Value::Bool(value) => value.to_string(),
            Value::Str(text) | Value::Fmt(text) => text.to_string(),
            _ => unreachable!("the checker only lets printable values through"),
        }
    }

    /// How the value is written in an `assert` message: strings are quoted.
    fn debug(&self) -> String {
        match self {
            Value::Str(text) | Value::Fmt(text) => format!("{text:?}"),
            Value::Null => "null".to_string(),
            Value::Int(..) | Value::Bool(_) => self.format(),
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
}

impl From<Diag> for Unwind {
    fn from(diag: Diag) -> Unwind {
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
            _ => unreachable!("the checker only lets enums be matched"),
        };
    }
}

/// Variables of one function call.
struct Frame {
    scopes: Vec<HashMap<String, Cell>>,
}

impl Frame {
    fn slot(&self, name: &str) -> Cell {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .expect("names are checked before running")
            .clone()
    }

    fn declare(&mut self, name: &str, value: Value) {
        self.scopes.last_mut().expect("an open scope").insert(name.to_string(), cell(value));
    }
}

pub fn run_program(prog: &Program, src: &str, out: &mut dyn Write) -> Result<(), Stop> {
    let funcs = prog.funcs.iter().map(|func| (func.name.as_str(), func)).collect();
    let mut methods: HashMap<&str, HashMap<&str, &Func>> = HashMap::new();
    for decl in &prog.structs {
        methods.insert(&decl.name, decl.methods.iter().map(|method| (method.name.as_str(), method)).collect());
    }
    let structs = prog.structs.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let enums = prog.enums.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let mut interp = Interp { funcs, methods, structs, enums, src, out, depth: 0 };
    let main = interp.funcs["main"];
    let result = match interp.call(main, None, Vec::new(), main.name_span) {
        // `Result<void, E> main()` ended with `Error(cause)`: the cause is written out.
        Ok(Value::Enum(_, 1, fields)) => {
            let cause = fields[0].borrow().clone();
            interp.show(&cause, false, main.name_span).and_then(|text| Err(Unwind::Stop(Stop::Failed(text))))
        }
        other => other,
    };
    match result {
        Ok(_) => Ok(()),
        Err(Unwind::Stop(stop)) => Err(stop),
        Err(_) => unreachable!("`return` ends at its call, `break` and `continue` at their loop"),
    }
}

/// Runs every `test` block and writes one line for each: `ok` or `FAIL` with the reason.
/// Gives how many passed and how many failed. A test that fails does not stop the others.
pub fn run_tests(prog: &Program, sources: &crate::source::Sources, out: &mut dyn Write) -> Result<(usize, usize), Stop> {
    let src = sources.text.as_str();
    let funcs = prog.funcs.iter().map(|func| (func.name.as_str(), func)).collect();
    let mut methods: HashMap<&str, HashMap<&str, &Func>> = HashMap::new();
    for decl in &prog.structs {
        methods.insert(&decl.name, decl.methods.iter().map(|method| (method.name.as_str(), method)).collect());
    }
    let structs = prog.structs.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let enums = prog.enums.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let mut interp = Interp { funcs, methods, structs, enums, src, out, depth: 0 };
    let (mut passed, mut failed) = (0, 0);
    for test in &prog.tests {
        // Nothing is shared between tests: each starts with variables of its own.
        interp.depth = 0;
        let mut frame = Frame { scopes: vec![HashMap::new()] };
        let ended = match interp.block(&test.body, &mut frame) {
            Ok(Flow::Return(value)) | Err(Unwind::Return(value)) => Ok(value),
            Ok(_) => Ok(Value::Void),
            Err(other) => Err(other),
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

struct Interp<'a> {
    funcs: HashMap<&'a str, &'a Func>,
    /// Functions written in struct bodies: struct name, then function name.
    methods: HashMap<&'a str, HashMap<&'a str, &'a Func>>,
    structs: HashMap<&'a str, &'a StructDecl>,
    enums: HashMap<&'a str, &'a EnumDecl>,
    src: &'a str,
    out: &'a mut dyn Write,
    depth: usize,
}

impl<'a> Interp<'a> {
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
        self.depth -= 1;
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

    fn stmt(&mut self, stmt: &'a Stmt, frame: &mut Frame) -> Res<Flow> {
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
            StmtKind::If { cond, then, els } => {
                // The scope holds the names of an `is` pattern in the condition.
                frame.scopes.push(HashMap::new());
                let flow = if self.condition(cond, frame)? {
                    self.block(then, frame)?
                } else if let Some(els) = els {
                    self.block(els, frame)?
                } else {
                    Flow::Next
                };
                frame.scopes.pop();
                return Ok(flow);
            }
            StmtKind::While { cond, body } => {
                let depth = frame.scopes.len();
                loop {
                    frame.scopes.push(HashMap::new());
                    let flow = match self.condition(cond, frame)? {
                        true => loop_flow(self.block(body, frame))?,
                        false => Flow::Break,
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
            ExprKind::Field { recv, index, .. } => {
                let mut target = self.place(recv, frame)?;
                loop {
                    let value = target.borrow().clone();
                    match value {
                        Value::Ptr(inner) | Value::Borrow(inner) => target = inner,
                        Value::Struct(_, fields) => return Ok(fields[*index].clone()),
                        _ => unreachable!("the checker rules out field access on `null` and non-structs"),
                    }
                }
            }
            ExprKind::Index { recv, index, .. } => {
                let holder = self.place(recv, frame)?;
                let holder = owned(holder.borrow().clone());
                let at = strip(self.expr(index, frame)?);
                let out_of_range = |at: i64, length: usize| {
                    Unwind::from(Diag::panic(expr.span, format!("index {at} is out of range: the length is {length}")))
                };
                match (holder, &at) {
                    (Value::Str(text), Value::Int(_, at)) => match usize::try_from(*at).ok().and_then(|at| text.as_bytes().get(at)) {
                        Some(byte) => Ok(cell(Value::Int(IntTy::Byte, *byte as i64))),
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
            ExprKind::Bool(value) => Ok(Value::Bool(*value)),
            ExprKind::Str(text) => Ok(Value::Str(text.as_str().into())),
            ExprKind::Null => Ok(Value::Null),
            ExprKind::Interp(parts) => {
                // The values are computed first, from left to right; they are written after
                // that, so a `format` method runs when every value of the line is there.
                let mut values = Vec::new();
                for part in parts {
                    if let InterpPart::Expr(inner) = part {
                        values.push(self.expr(inner, frame)?);
                    }
                }
                let mut values = values.iter();
                let mut text = String::new();
                for part in parts {
                    match part {
                        InterpPart::Text(piece) => text.push_str(piece),
                        InterpPart::Expr(inner) => {
                            let value = values.next().expect("one value for each part");
                            text.push_str(&self.show(value, false, inner.span)?);
                        }
                    }
                }
                Ok(Value::Fmt(text.into()))
            }
            ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. } => {
                let value = fresh(&self.place(expr, frame)?.borrow());
                Ok(value)
            }
            ExprKind::FuncRef(name) => Ok(Value::Func(name.as_str().into())),
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
            ExprKind::Widen(ty, inner) => match strip(self.expr(inner, frame)?) {
                Value::Int(_, value) => Ok(Value::Int(*ty, value)),
                _ => unreachable!("the checker only widens integers"),
            },
            ExprKind::Load(inner) => Ok(fresh(&owned(self.expr(inner, frame)?))),
            ExprKind::Unary(UnOp::Neg, operand) => match strip(self.expr(operand, frame)?) {
                Value::Int(ty, value) => {
                    let result = -(value as i128);
                    if !ty.fits(result) {
                        return Err(Diag::panic(expr.span, format!("overflow: -({value}) does not fit in {}", ty.name())).into());
                    }
                    Ok(Value::Int(ty, result as i64))
                }
                _ => unreachable!("the checker makes the operand an integer"),
            },
            ExprKind::Unary(UnOp::Not, operand) => Ok(Value::Bool(!self.condition(operand, frame)?)),
            ExprKind::Unary(UnOp::BitNot, operand) => match strip(self.expr(operand, frame)?) {
                Value::Int(ty, value) => Ok(Value::Int(ty, ty.wrap(!(value as i128)))),
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
            ExprKind::Char(_) => unreachable!("the checker turns a character into a `byte`"),
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
                    Value::Str(text) => text.len() as i64,
                    other => cells_of(other).len() as i64,
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
                Ok(Value::Str(text[start..end].into()))
            }
            ExprKind::Cast { ty, expr: inner } => match strip(self.expr(inner, frame)?) {
                Value::Int(_, value) if ty.fits(value as i128) => Ok(Value::Int(*ty, value)),
                Value::Int(_, value) => {
                    Err(Diag::panic(expr.span, format!("value {value} does not fit in {}", ty.name())).into())
                }
                _ => unreachable!("the checker only casts integers"),
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
                    _ => unreachable!("the checker only unboxes a borrow of a pointer"),
                },
                _ => unreachable!("the checker only unboxes a borrow"),
            },
            ExprKind::IsVariant { expr: inner, negated, pattern, .. } => {
                let (index, fields) = enum_parts(self.expr(inner, frame)?);
                let PatternKind::Variant { index: wanted, args, .. } = &pattern.kind else {
                    unreachable!("`is` only tests for a variant")
                };
                let found = index == *wanted;
                if found {
                    // `is` only looks, so the names borrow.
                    for ((name, _), field) in args.iter().flatten().zip(fields.iter()) {
                        if let Some(name) = name {
                            frame.scopes.last_mut().expect("an open scope").insert(name.clone(), cell(Value::Borrow(field.clone())));
                        }
                    }
                }
                Ok(Value::Bool(found != *negated))
            }
            ExprKind::Match { subject, arms, borrowed } => {
                let value = self.expr(subject, frame)?;
                // A number is compared with the arms; an enum is taken apart.
                let number = match strip(value.clone()) {
                    Value::Int(_, number) => Some(number),
                    _ => None,
                };
                let (index, fields) = match number {
                    Some(_) => (0, Rc::new(Vec::new())),
                    None => enum_parts(value.clone()),
                };
                let depth = frame.scopes.len();
                for arm in arms {
                    frame.scopes.push(HashMap::new());
                    let fits = match &arm.pattern.kind {
                        PatternKind::Wildcard => true,
                        PatternKind::Bind(name) => {
                            frame.declare(name, value.clone());
                            true
                        }
                        PatternKind::Int(wanted) => number == Some(*wanted),
                        PatternKind::Char(_) => unreachable!("the checker turns a character into a number"),
                        PatternKind::Variant { index: wanted, args, .. } => {
                            if index == *wanted {
                                for ((name, _), field) in args.iter().flatten().zip(fields.iter()) {
                                    let Some(name) = name else { continue };
                                    let bound = if *borrowed { Value::Borrow(field.clone()) } else { field.borrow().clone() };
                                    frame.declare(name, bound);
                                }
                            }
                            index == *wanted
                        }
                    };
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
                Callee::Method { owner, this } => {
                    let func = self.methods[owner.as_str()][name.as_str()];
                    // The receiver is lent or handed over without `read` / `ref` at the call.
                    let mut target = self.place(recv, frame)?;
                    loop {
                        let inner = match &*target.borrow() {
                            Value::Ptr(inner) | Value::Borrow(inner) => inner.clone(),
                            _ => break,
                        };
                        target = inner;
                    }
                    let this = match this {
                        This::Value => fresh(&target.borrow()),
                        This::Read | This::Ref => Value::Borrow(target),
                    };
                    let values = self.values(args, frame)?;
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
                Callee::FromUtf8 => {
                    let cells = cells_of(&owned(self.expr(&args[0], frame)?));
                    let bytes: Vec<u8> = cells.iter().map(|item| match &*item.borrow() {
                        Value::Int(_, byte) => *byte as u8,
                        _ => unreachable!("the checker makes the items bytes"),
                    }).collect();
                    Ok(match String::from_utf8(bytes) {
                        Ok(text) => Value::Enum("Result".into(), 0, Rc::new(vec![cell(Value::Str(text.into()))])),
                        Err(error) => {
                            let position = cell(Value::Int(IntTy::Nint, error.utf8_error().valid_up_to() as i64));
                            let cause = Value::Struct("TextError".into(), Rc::new(vec![position]));
                            Value::Enum("Result".into(), 1, Rc::new(vec![cell(cause)]))
                        }
                    })
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
                Callee::Plain => {
                    let recv = owned(self.expr(recv, frame)?);
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
                    let recv = as_list(recv);
                    if matches!(recv, Value::List(_) | Value::Map(_) | Value::Set(_)) {
                        return self.collection_method(recv, name, values, expr.span);
                    }
                    Ok(method(recv, name, values))
                }
            },
        }
    }

    fn int(&mut self, expr: &'a Expr, frame: &mut Frame) -> Res<i64> {
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
            "panic" => {
                let text = strip(self.expr(&args[0], frame)?).format();
                Err(Diag::panic(expr.span, text).into())
            }
            "assert" => self.assert(expr, &args[0], frame),
            _ => {
                let func = self.funcs[name];
                let values = self.values(args, frame)?;
                self.call(func, None, values, expr.span)
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
            Value::Array(_) | Value::Slice(_) => self.show(&as_list(value.clone()), inside, span),
            Value::Str(text) if inside => Ok(format!("{text:?}")),
            Value::Null => Ok("null".to_string()),
            Value::Void => Ok(String::new()),
            Value::Struct(name, fields) => {
                if let Some(format) = self.methods.get(&**name).and_then(|methods| methods.get("format")).copied() {
                    let written = Rc::new(RefCell::new(String::new()));
                    let this = Value::Borrow(cell(value.clone()));
                    let formatter = Value::Borrow(cell(Value::Formatter(written.clone())));
                    self.call(format, Some(this), vec![formatter], span)?;
                    let text = written.borrow().clone();
                    return Ok(text);
                }
                let decl = self.structs[&**name];
                let parts = list(self, fields)?;
                let named: Vec<String> = decl.fields.iter().zip(parts).map(|(field, text)| format!("{} = {text}", field.name)).collect();
                Ok(format!("{} {{ {} }}", short_name(name), named.join(", ")))
            }
            Value::Enum(name, index, fields) => {
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

    /// The method of a struct that `==`, `<`, `indexOf` or `sort` runs, if the struct wrote
    /// its own: called with both values lent.
    fn compared(&mut self, method: &str, left: &Value, right: &Value, span: Span) -> Res<Option<Value>> {
        let Value::Struct(name, _) = left else { return Ok(None) };
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
                    (_, Some(at)) => Value::Int(IntTy::Nint, at as i64),
                    (_, None) => Value::Null,
                })
            }
            (Value::List(items), "sort") => {
                // The list is taken out while its items are compared, so that a `compare`
                // that fails leaves through here.
                let mut sorted: Vec<Cell> = items.borrow().clone();
                let mut failed = None;
                sorted.sort_by(|a, b| {
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
                if let Some(stop) = failed {
                    return Err(stop);
                }
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

    fn write_line(&mut self, text: &str) -> Res<()> {
        writeln!(self.out, "{text}").map_err(|error| match error.kind() {
            std::io::ErrorKind::BrokenPipe => Unwind::Stop(Stop::OutputClosed),
            _ => Unwind::Stop(Stop::OutputFailed(error.to_string())),
        })
    }
}

fn binary(op: BinOp, left: Value, right: Value, span: Span) -> Result<Value, Diag> {
    use std::cmp::Ordering;

    let ordering: Ordering = match (&left, &right) {
        (Value::Int(ty, a), Value::Int(_, b)) if !op.is_comparison() => return arithmetic(op, *ty, *a, *b, span),
        (Value::Int(_, a), Value::Int(_, b)) => a.cmp(b),
        (Value::Str(a), Value::Str(b)) => a.cmp(b),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
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

/// Overflow panics in every build; `wrappingAdd` is written to wrap on purpose. A shift
/// count outside the bits of the type panics the same way; the bits that `<<` pushes out
/// are gone without a word, since that is what a shift is for.
fn arithmetic(op: BinOp, ty: IntTy, a: i64, b: i64, span: Span) -> Result<Value, Diag> {
    let (wide_a, wide_b) = (a as i128, b as i128);
    if op.is_shift() && !(0..ty.bits() as i64).contains(&b) {
        return Err(Diag::panic(span, format!("shift by {b} is out of range: {} has {} bits", ty.name(), ty.bits())));
    }
    let result = match op {
        // A number is kept with its sign carried up, so the bits above the type agree on
        // both sides and `>>` brings the sign down with it.
        BinOp::BitAnd => wide_a & wide_b,
        BinOp::BitOr => wide_a | wide_b,
        BinOp::BitXor => wide_a ^ wide_b,
        BinOp::Shl => return Ok(Value::Int(ty, ty.wrap(wide_a << b))),
        BinOp::Shr => wide_a >> b,
        BinOp::Add => wide_a + wide_b,
        BinOp::Sub => wide_a - wide_b,
        BinOp::Mul => wide_a * wide_b,
        BinOp::Div | BinOp::Rem if b == 0 => {
            return Err(Diag::panic(span, format!("division by zero: {a} {} 0", op.symbol())));
        }
        BinOp::Div => wide_a / wide_b,
        BinOp::Rem => wide_a % wide_b,
        _ => unreachable!("only arithmetic reaches here"),
    };
    if !ty.fits(result) {
        return Err(Diag::panic(span, format!("overflow: {a} {} {b} does not fit in {}", op.symbol(), ty.name())));
    }
    Ok(Value::Int(ty, result as i64))
}

/// The methods of the collections that neither compare items nor can fail.
fn collection_method(recv: Value, name: &str, mut args: Vec<Value>) -> Value {
    let lent = |found: Option<Cell>| found.map_or(Value::Null, Value::Borrow);
    match (&recv, name) {
        (Value::List(items), "length") => Value::Int(IntTy::Nint, items.borrow().len() as i64),
        (Value::List(items), "clear") => {
            items.borrow_mut().clear();
            Value::Void
        }
        (Value::List(items), "add") => {
            items.borrow_mut().push(cell(args.remove(0)));
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
        (Value::Set(items), "length") => Value::Int(IntTy::Nint, items.borrow().len() as i64),
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
        (Value::Map(entries), "length") => Value::Int(IntTy::Nint, entries.borrow().len() as i64),
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

fn method(recv: Value, name: &str, args: Vec<Value>) -> Value {
    match (&recv, name, args.as_slice()) {
        // `null` where the plain operator would panic.
        (Value::Int(ty, a), "tryAdd" | "trySubtract" | "tryMultiply" | "tryDivide", [Value::Int(_, b)]) => {
            let (a, b) = (*a as i128, *b as i128);
            let result = match name {
                "tryAdd" => a + b,
                "trySubtract" => a - b,
                "tryMultiply" => a * b,
                _ if b == 0 => return Value::Null,
                _ => a / b,
            };
            if ty.fits(result) {
                Value::Int(*ty, result as i64)
            } else {
                Value::Null
            }
        }
        (Value::Str(_), "clone", []) => recv,
        (Value::Str(_), "asBytes", []) => Value::Slice(cells_of(&recv)),
        (_, "equals", [other]) => Value::Bool(key_of(&recv) == key_of(other)),
        (_, "compare", [other]) => Value::Int(IntTy::Int, key_of(&recv).cmp(&key_of(other)) as i64),
        (Value::Int(ty, a), _, [Value::Int(_, b)]) => {
            let (a, b) = (*a as i128, *b as i128);
            let result = match name {
                "wrappingAdd" => a.wrapping_add(b),
                "wrappingSubtract" => a.wrapping_sub(b),
                _ => a.wrapping_mul(b),
            };
            Value::Int(*ty, ty.wrap(result))
        }
        (_, "toString", []) => Value::Str(recv.format().into()),
        (Value::Str(text), "length", []) => Value::Int(IntTy::Nint, text.len() as i64),
        (Value::Str(text), "indexOf", [Value::Int(_, byte)]) => match text.bytes().position(|found| found as i64 == *byte) {
            Some(at) => Value::Int(IntTy::Nint, at as i64),
            None => Value::Null,
        },
        _ => unreachable!("the checker only lets known methods through"),
    }
}
