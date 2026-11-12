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
use crate::types::IntTy;

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
    /// Fields in declaration order.
    Struct(Rc<Vec<Cell>>),
    /// `T*`
    Ptr(Cell),
    /// `read T` / `ref T`
    Borrow(Cell),
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
    /// How the value is written by `print` and inside `$"..."`.
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
}

impl From<Diag> for Stop {
    fn from(diag: Diag) -> Stop {
        Stop::Diag(diag)
    }
}

type Res<T> = Result<T, Stop>;

enum Flow {
    Next,
    Break,
    Continue,
    Return(Value),
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

pub fn run_program(prog: &Program, src: &str, out: &mut dyn Write) -> Res<()> {
    let funcs = prog.funcs.iter().map(|func| (func.name.as_str(), func)).collect();
    let mut methods: HashMap<&str, HashMap<&str, &Func>> = HashMap::new();
    for decl in &prog.structs {
        methods.insert(&decl.name, decl.methods.iter().map(|method| (method.name.as_str(), method)).collect());
    }
    let mut interp = Interp { funcs, methods, src, out, depth: 0 };
    let main = interp.funcs["main"];
    interp.call(main, None, Vec::new(), main.name_span)?;
    Ok(())
}

struct Interp<'a> {
    funcs: HashMap<&'a str, &'a Func>,
    /// Functions written in struct bodies: struct name, then function name.
    methods: HashMap<&'a str, HashMap<&'a str, &'a Func>>,
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
        let flow = self.block(&func.body, &mut frame)?;
        self.depth -= 1;
        match flow {
            Flow::Return(value) => Ok(value),
            _ => Ok(Value::Void),
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
                if self.condition(cond, frame)? {
                    return self.block(then, frame);
                } else if let Some(els) = els {
                    return self.block(els, frame);
                }
            }
            StmtKind::While { cond, body } => {
                while self.condition(cond, frame)? {
                    match self.block(body, frame)? {
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
                for i in start..end {
                    frame.scopes.push(HashMap::new());
                    frame.declare(var, Value::Int(ty, i));
                    let flow = self.block(body, frame)?;
                    frame.scopes.pop();
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
            StmtKind::Expr(expr) => {
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
                        Value::Struct(fields) => return Ok(fields[*index].clone()),
                        _ => unreachable!("the checker rules out field access on `null` and non-structs"),
                    }
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
                let mut text = String::new();
                for part in parts {
                    match part {
                        InterpPart::Text(piece) => text.push_str(piece),
                        InterpPart::Expr(inner) => text.push_str(&strip(self.expr(inner, frame)?).format()),
                    }
                }
                Ok(Value::Fmt(text.into()))
            }
            ExprKind::Var(_) | ExprKind::Field { .. } => {
                let value = self.place(expr, frame)?.borrow().clone();
                Ok(value)
            }
            ExprKind::Widen(ty, inner) => match strip(self.expr(inner, frame)?) {
                Value::Int(_, value) => Ok(Value::Int(*ty, value)),
                _ => unreachable!("the checker only widens integers"),
            },
            ExprKind::Load(inner) => Ok(strip(self.expr(inner, frame)?)),
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
                Ok(binary(*op, left, right, expr.span)?)
            }
            ExprKind::Cast { ty, expr: inner } => match strip(self.expr(inner, frame)?) {
                Value::Int(_, value) if ty.fits(value as i128) => Ok(Value::Int(*ty, value)),
                Value::Int(_, value) => {
                    Err(Diag::panic(expr.span, format!("value {value} does not fit in {}", ty.name())).into())
                }
                _ => unreachable!("the checker only casts integers"),
            },
            ExprKind::StructLit { fields, .. } => {
                let mut cells: Vec<Option<Cell>> = vec![None; fields.len()];
                for init in fields {
                    cells[init.index] = Some(cell(self.expr(&init.value, frame)?));
                }
                let cells = cells.into_iter().map(|field| field.expect("the checker requires every field")).collect();
                Ok(Value::Struct(Rc::new(cells)))
            }
            ExprKind::New(inner) => Ok(Value::Ptr(cell(self.expr(inner, frame)?))),
            ExprKind::Borrow { expr: inner, .. } => Ok(Value::Borrow(self.place(inner, frame)?)),
            ExprKind::IsNull { expr: inner, negated } => {
                let is_null = matches!(strip(self.expr(inner, frame)?), Value::Null);
                Ok(Value::Bool(is_null != *negated))
            }
            ExprKind::Coalesce(left, right) => {
                let value = self.expr(left, frame)?;
                if matches!(strip(value.clone()), Value::Null) {
                    self.expr(right, frame)
                } else {
                    Ok(value)
                }
            }
            ExprKind::Call { name, args, callee, .. } => match callee {
                Callee::Static { owner, name } => {
                    let func = self.methods[owner.as_str()][name.as_str()];
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
                        This::Value => target.borrow().clone(),
                        This::Read | This::Ref => Value::Borrow(target),
                    };
                    let values = self.values(args, frame)?;
                    self.call(func, Some(this), values, expr.span)
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
                    let recv = strip(self.expr(recv, frame)?);
                    let mut values = Vec::new();
                    for arg in args {
                        values.push(strip(self.expr(arg, frame)?));
                    }
                    Ok(method(recv, name, values))
                }
            },
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
                let text = strip(self.expr(&args[0], frame)?).format();
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
        let failed = |sides: Option<(Value, Value)>| {
            let mut diag = Diag::panic(call.span, format!("assert failed: {text}"));
            if let Some((left, right)) = sides {
                diag = diag.note(format!("left: {}", left.debug())).note(format!("right: {}", right.debug()));
            }
            Stop::Diag(diag)
        };
        match &cond.kind {
            ExprKind::Binary(op, left, right) if op.is_comparison() => {
                let left = strip(self.expr(left, frame)?);
                let right = strip(self.expr(right, frame)?);
                match binary(*op, left.clone(), right.clone(), cond.span)? {
                    Value::Bool(true) => Ok(Value::Void),
                    _ => Err(failed(Some((left, right)))),
                }
            }
            _ if self.condition(cond, frame)? => Ok(Value::Void),
            _ => Err(failed(None)),
        }
    }

    fn write_line(&mut self, text: &str) -> Res<()> {
        writeln!(self.out, "{text}").map_err(|error| match error.kind() {
            std::io::ErrorKind::BrokenPipe => Stop::OutputClosed,
            _ => Stop::OutputFailed(error.to_string()),
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

/// Overflow panics in every build; `wrappingAdd` is written to wrap on purpose.
fn arithmetic(op: BinOp, ty: IntTy, a: i64, b: i64, span: Span) -> Result<Value, Diag> {
    let (wide_a, wide_b) = (a as i128, b as i128);
    let result = match op {
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

fn method(recv: Value, name: &str, args: Vec<Value>) -> Value {
    match (&recv, name, args.as_slice()) {
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
        _ => unreachable!("the checker only lets known methods through"),
    }
}
