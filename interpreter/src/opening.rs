//! Where a `shared checked` value is reached, and for how long.
//!
//! Runs on a function after the checker. Each place where what a `shared checked T` holds is
//! reached gets an `Open` node that says how (see `Opening`): read or changed once, or lent for
//! longer. A borrow that is lent lasts until the call it was lent to returns (unless the result
//! borrows from it), or for a `for`, a `match` or an `as` block, until they end; otherwise
//! until its statement ends. `Within` nodes mark the calls and conditions where they end, and
//! the program checks each access against the borrows in progress while it runs.
//!
//! The same walk finds the conflicts the program would meet every time it ran: an access
//! through the same variable while a borrow of it that does not allow it is in progress
//! (`for child in read folder.children { folder.bytes = 0 }`). A conflict through another
//! variable is left to run time. It also refuses a borrow that would outlive its statement:
//! one kept in a variable, returned, or given to the code after an `if`.

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::types::Ty;

type Res<T> = Result<T, Diag>;

/// How a value is used where it is reached.
#[derive(Clone, Copy, PartialEq)]
enum Use {
    /// Copied out, compared, or only passed through.
    Value,
    /// Assigned.
    Change,
    /// Lent to read.
    Lend,
    /// Lent to change.
    LendChange,
    /// The receiver of a method that changes it and has arguments.
    Reserve,
}

impl Use {
    fn lends(self) -> bool {
        matches!(self, Use::Lend | Use::LendChange | Use::Reserve)
    }

    /// How a `shared checked` value is opened for this use.
    fn opening(self) -> Opening {
        match self {
            Use::Value => Opening::Look,
            Use::Change => Opening::Change,
            Use::Lend => Opening::Lend,
            Use::LendChange => Opening::LendChange,
            Use::Reserve => Opening::Reserve,
        }
    }

    /// The use of the value a field or an item is reached through: lent if the field is.
    fn of_holder(self) -> Use {
        if self.lends() {
            Use::Lend
        } else {
            Use::Value
        }
    }
}

/// What a borrow in progress was lent to, for the messages.
#[derive(Clone, Copy, PartialEq)]
enum Why {
    Call,
    Loop,
    Match,
    Condition,
    Block { mutable: bool },
    Line,
}

/// A borrow of a `shared checked` value in progress, as far as the walk can see it.
struct Borrowed {
    /// The variable and the fields it was reached through (`folder`, `a.next`); `None` when
    /// that cannot be told before the program runs (an item of a list).
    key: Option<String>,
    write: bool,
    reserved: bool,
    span: Span,
    why: Why,
}

struct Opener<'a> {
    src: &'a str,
    /// Whether a value of the type is a borrow or carries one.
    lends: &'a dyn Fn(&Ty) -> bool,
    borrowed: Vec<Borrowed>,
    /// What a borrow made now is lent to.
    why: Why,
}

/// Opens the `shared checked` values reached in a function body.
pub fn open(body: &mut Block, lends: &dyn Fn(&Ty) -> bool, src: &str) -> Res<()> {
    let mut opener = Opener { src, lends, borrowed: Vec::new(), why: Why::Line };
    opener.block(body)
}

/// What a `shared checked` value of this type holds, if it is one (through borrows).
fn checked_behind(ty: &Ty) -> Option<&Ty> {
    let mut ty = ty;
    while let Ty::Read(inner) | Ty::Ref(inner) = ty {
        ty = inner;
    }
    match ty {
        Ty::Shared(inner, true) => Some(inner),
        _ => None,
    }
}

/// The variable and the fields a value is reached through: `a.next`.
fn key_of(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Var(name) => Some(name.clone()),
        ExprKind::Field { recv, name, .. } => Some(format!("{}.{name}", key_of(recv)?)),
        ExprKind::Open { inner, .. } | ExprKind::Load(inner) | ExprKind::Borrow { expr: inner, .. } | ExprKind::Unbox(inner) => key_of(inner),
        _ => None,
    }
}

/// The methods of the built-in types that change their receiver.
fn changes(name: &str) -> bool {
    matches!(name, "add" | "addAll" | "clear" | "removeLast" | "reverse" | "remove" | "getOrAdd" | "sort" | "write" | "next")
}

/// Whether a pattern gives names.
fn names_in(pattern: &Pattern) -> bool {
    match &pattern.kind {
        PatternKind::Bind(_) => true,
        PatternKind::Variant { args: Some(args), .. } => args.iter().any(names_in),
        PatternKind::Or(alternatives) => alternatives.iter().any(names_in),
        _ => false,
    }
}

/// Whether a condition gives names that borrow: `x is Circle(r)`.
fn gives_names(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::IsVariant { pattern, .. } => names_in(pattern),
        ExprKind::Binary(_, left, right) => gives_names(left) || gives_names(right),
        ExprKind::Unary(_, inner) => gives_names(inner),
        _ => false,
    }
}

/// Puts `build(expr)` where `expr` was.
fn wrap(expr: &mut Expr, build: impl FnOnce(Box<Expr>) -> ExprKind) {
    let span = expr.span;
    let ty = expr.ty.clone();
    let inner = std::mem::replace(expr, Expr { kind: ExprKind::Null, span, ty: Ty::Void, moved: false });
    expr.kind = build(Box::new(inner));
    expr.ty = ty;
}

impl Opener<'_> {
    fn block(&mut self, block: &mut Block) -> Res<()> {
        for stmt in block.iter_mut() {
            self.stmt(stmt)?;
        }
        Ok(())
    }

    fn text(&self, span: Span) -> &str {
        &self.src[span.start..span.end]
    }

    /// A borrow made inside `mark` that is still in progress, if any.
    fn kept(&self, mark: usize) -> Option<&Borrowed> {
        self.borrowed[mark..].first()
    }

    fn stmt(&mut self, stmt: &mut Stmt) -> Res<()> {
        let mark = self.borrowed.len();
        self.why = Why::Line;
        let result = self.stmt_in(stmt, mark);
        self.borrowed.truncate(mark);
        result
    }

    fn stmt_in(&mut self, stmt: &mut Stmt, mark: usize) -> Res<()> {
        match &mut stmt.kind {
            StmtKind::Let { name, init, .. } => {
                self.expr(init, Use::Value)?;
                if (self.lends)(&init.ty) {
                    if let Some(kept) = self.kept(mark) {
                        let holder = kept.key.clone().unwrap_or_else(|| self.text(kept.span).to_string());
                        return Err(Diag::error(init.span, format!("this borrows what a `shared checked` value holds, and such a borrow ends with its line; `{name}` would keep it"))
                            .related(kept.span, "it is borrowed here")
                            .hint(format!("lend it for a block instead: `read {holder} as value {{ ... }}`")));
                    }
                }
            }
            StmtKind::Assign { target, value, .. } => {
                // The right side first, then the place, as the program does.
                self.expr(value, Use::Value)?;
                if (self.lends)(&value.ty) {
                    if let Some(kept) = self.kept(mark) {
                        return Err(Diag::error(value.span, "this borrows what a `shared checked` value holds, and such a borrow ends with its line; the place on the left would keep it")
                            .related(kept.span, "it is borrowed here"));
                    }
                }
                self.expr(target, Use::Change)?;
            }
            StmtKind::If { cond, then, els, after } => {
                self.condition(cond, mark)?;
                if !after.is_empty() {
                    if let Some(kept) = self.kept(mark) {
                        return Err(Diag::error(cond.span, "the names of this condition borrow what a `shared checked` value holds, and such a borrow ends with the `if`; the code after it cannot see them")
                            .related(kept.span, "it is borrowed here")
                            .hint("use the names inside the `if`"));
                    }
                }
                self.block(then)?;
                if let Some(els) = els {
                    self.block(els)?;
                }
            }
            StmtKind::While { cond, body, after } => {
                self.condition(cond, mark)?;
                if !after.is_empty() {
                    if let Some(kept) = self.kept(mark) {
                        return Err(Diag::error(cond.span, "the names of this condition borrow what a `shared checked` value holds, and such a borrow ends with the loop; the code after it cannot see them")
                            .related(kept.span, "it is borrowed here"));
                    }
                }
                self.block(body)?;
            }
            StmtKind::For { from, to, body, .. } => {
                self.expr(from, Use::Value)?;
                self.expr(to, Use::Value)?;
                self.borrowed.truncate(mark);
                self.block(body)?;
            }
            StmtKind::ForEach { iter, body, by, .. } => {
                self.why = Why::Loop;
                let used = match by {
                    Some(Mode::Read) => Use::Lend,
                    Some(Mode::Ref) => Use::LendChange,
                    None => Use::Value,
                };
                // `for word in words` over a `shared checked List`: the loop reaches into it.
                match checked_behind(&iter.ty).is_some() {
                    true => self.open_here(iter, used.max_lend())?,
                    false => self.expr(iter, used)?,
                }
                self.block(body)?;
            }
            StmtKind::As { mutable, value, body, .. } => {
                self.why = Why::Block { mutable: *mutable };
                let used = if *mutable { Use::LendChange } else { Use::Lend };
                self.open_here(value, used)?;
                self.block(body)?;
            }
            // A `shared locked` value is never `checked`: nothing here is opened.
            StmtKind::Lock { items, body } => {
                for item in items {
                    self.expr(&mut item.value, Use::Value)?;
                }
                self.block(body)?;
            }
            StmtKind::Return(Some(value)) => {
                self.expr(value, Use::Value)?;
                if (self.lends)(&value.ty) {
                    if let Some(kept) = self.kept(mark) {
                        return Err(Diag::error(value.span, "this borrows what a `shared checked` value holds, and such a borrow ends with its line; it cannot be returned")
                            .related(kept.span, "it is borrowed here")
                            .hint("return a copy, or take what it holds as a `read` parameter, so that the caller lends it"));
                    }
                }
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(expr) | StmtKind::Discard(expr) => self.expr(expr, Use::Value)?,
        }
        Ok(())
    }

    /// The condition of an `if` or a `while`: what it lends ends with it, unless it gives
    /// names that borrow, which the block then uses.
    fn condition(&mut self, cond: &mut Expr, mark: usize) -> Res<()> {
        self.why = Why::Condition;
        self.expr(cond, Use::Value)?;
        if self.borrowed.len() > mark && !gives_names(cond) {
            wrap(cond, ExprKind::Within);
            self.borrowed.truncate(mark);
        }
        self.why = Why::Line;
        Ok(())
    }

    /// Opens the `shared checked` value `expr` is, where it stands, for `used`.
    fn open_here(&mut self, expr: &mut Expr, used: Use) -> Res<()> {
        let holder = used.of_holder();
        self.expr(expr, holder)?;
        let span = expr.span;
        let key = key_of(expr);
        self.reach(key, used, span)?;
        wrap(expr, |inner| ExprKind::Open { inner, how: used.opening() });
        Ok(())
    }

    /// An access to a `shared checked` value reached through `key`: refused if a borrow of it
    /// in progress through the same variable does not allow it, and recorded if it is lent.
    fn reach(&mut self, key: Option<String>, used: Use, span: Span) -> Res<()> {
        let write = matches!(used, Use::Change | Use::LendChange);
        if let Some(key) = &key {
            if let Some(other) = self.borrowed.iter().rev().find(|other| other.key.as_ref() == Some(key) && (other.write || write)) {
                return Err(self.conflict(key, other, write, span));
            }
        }
        if used.lends() {
            self.borrowed.push(Borrowed { key, write, reserved: used == Use::Reserve, span, why: self.why });
        }
        Ok(())
    }

    /// A conflict the program would meet every time it ran this line.
    fn conflict(&self, key: &str, other: &Borrowed, write: bool, span: Span) -> Diag {
        let (doing, there) = match other.write {
            true => ("changed", "it is being changed from here"),
            false => ("read", "it is being read from here"),
        };
        let verb = if write { "changed" } else { "read" };
        let lasting = match other.why {
            Why::Call => " for the call",
            Why::Loop => " for the whole loop",
            Why::Match => " for the whole `match`",
            Why::Condition => " for the whole `if`",
            Why::Block { .. } => " for the whole block",
            Why::Line => "",
        };
        let diag = Diag::error(span, format!("`{key}` is being {doing}{lasting}, so it cannot be {verb} here: this line would stop the program every time it ran"))
            .related(other.span, there)
            .note("a `shared checked` value is lent to one that changes it or to any number that read it, not both at once");
        match other.why {
            Why::Loop if write => diag.hint("collect what to change in a list, and change it after the loop"),
            Why::Loop => diag.hint(format!("copy what the loop needs from `{key}` into a variable before it")),
            Why::Block { mutable: false } if write => diag.hint(format!("open it with `ref {key} as ...` and change it through that name")),
            Why::Block { .. } => diag.hint("use the name the block gives"),
            Why::Match | Why::Condition => diag.hint("copy what the test needs into a variable first, and test that"),
            Why::Call => diag.hint("work the value out before the call: `var value = ...`"),
            Why::Line => diag,
        }
    }

    /// Walks an expression in the order the program computes it.
    fn expr(&mut self, expr: &mut Expr, used: Use) -> Res<()> {
        // A field or an item used where a borrow goes is lent.
        let used = match (used, &expr.ty, expr.is_place()) {
            (Use::Value, Ty::Read(_), true) => Use::Lend,
            (Use::Value, Ty::Ref(_), true) => Use::LendChange,
            _ => used,
        };
        // Where what was lent to a call (or a `match`) began.
        let mut call = None;
        match &mut expr.kind {
            ExprKind::Field { recv, .. } => self.reached(recv, used)?,
            ExprKind::Index { recv, index, .. } => {
                self.reached(recv, used)?;
                self.expr(index, Use::Value)?;
            }
            ExprKind::Slice { recv, from, to } => {
                let lent = if matches!(expr.ty, Ty::Ref(_)) { Use::LendChange } else { Use::Lend };
                self.reached(recv, lent)?;
                for bound in [from, to].into_iter().flatten() {
                    self.expr(bound, Use::Value)?;
                }
            }
            ExprKind::Borrow { mutable, expr: inner } => {
                let lent = if *mutable { Use::LendChange } else { Use::Lend };
                self.expr(inner, lent)?;
            }
            // `width(read node)`: what the `shared checked` value holds, lent.
            ExprKind::Unbox(inner) => {
                let lent = if matches!(expr.ty, Ty::Ref(_)) { Use::LendChange } else { Use::Lend };
                self.reached(inner, lent)?;
            }
            ExprKind::AsSlice(inner) => self.expr(inner, used)?,
            ExprKind::Load(inner) | ExprKind::Widen(_, inner) | ExprKind::Cast { expr: inner, .. } | ExprKind::Unary(_, inner) => self.expr(inner, Use::Value)?,
            ExprKind::New(inner) | ExprKind::NewShared { value: inner, .. } | ExprKind::Share(inner) | ExprKind::WeakOf(inner) => self.expr(inner, Use::Value)?,
            ExprKind::IsNull { expr: inner, .. } | ExprKind::Try { expr: inner, .. } => self.expr(inner, Use::Value)?,
            ExprKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(value, Use::Value)?;
                }
            }
            ExprKind::Binary(op, left, right) => {
                // `a == b` on two `shared checked` values compares what they hold.
                let compares = matches!(op, BinOp::Eq | BinOp::Ne) && checked_behind(&left.ty).is_some() && checked_behind(&right.ty).is_some();
                for side in [left, right] {
                    match compares {
                        true => self.open_here(side, Use::Lend)?,
                        false => self.expr(side, Use::Value)?,
                    }
                }
            }
            ExprKind::Coalesce { left, right, .. } => {
                self.expr(left, Use::Value)?;
                self.expr(right, Use::Value)?;
            }
            ExprKind::Interp(parts) => {
                for part in parts {
                    if let InterpPart::Expr(inner, _) = part {
                        match checked_behind(&inner.ty).is_some() {
                            true => self.open_here(inner, Use::Lend)?,
                            false => self.expr(inner, Use::Value)?,
                        }
                    }
                }
            }
            ExprKind::StructLit { fields, .. } => {
                for field in fields {
                    self.expr(&mut field.value, Use::Value)?;
                }
            }
            ExprKind::Variant { args, .. } => {
                for arg in args {
                    self.expr(arg, Use::Value)?;
                }
            }
            ExprKind::Collection { args, items, .. } => {
                for arg in args.iter_mut() {
                    self.expr(arg, Use::Value)?;
                }
                for item in items.iter_mut().flatten() {
                    for value in &mut item.values {
                        self.expr(value, Use::Value)?;
                    }
                }
            }
            ExprKind::Array { args, items, .. } => {
                for value in args.iter_mut().chain(items.iter_mut().flatten()) {
                    self.expr(value, Use::Value)?;
                }
            }
            ExprKind::IsVariant { expr: inner, pattern, by_ref, .. } => {
                let used = match (names_in(pattern), *by_ref) {
                    (false, _) => Use::Value,
                    (true, false) => Use::Lend,
                    (true, true) => Use::LendChange,
                };
                self.subject(inner, used, Why::Condition)?;
            }
            ExprKind::Match { subject, arms, borrowed } => {
                call = Some(self.borrowed.len());
                let used = if *borrowed { Use::Lend } else { Use::Value };
                self.subject(subject, used, Why::Match)?;
                let why = self.why;
                for arm in arms {
                    if let Some(guard) = &mut arm.guard {
                        self.expr(guard, Use::Value)?;
                    }
                    self.block(&mut arm.body)?;
                    self.why = why;
                    if let Some(value) = &mut arm.value {
                        self.expr(value, Use::Value)?;
                    }
                }
            }
            ExprKind::Call { args, callee, name, .. } => {
                call = Some(self.borrowed.len());
                let why = std::mem::replace(&mut self.why, Why::Call);
                // `print(node)` writes what a `shared checked` value holds.
                let shows = matches!(callee, Callee::Plain) && matches!(name.as_str(), "print" | "printError");
                for arg in args {
                    match shows && checked_behind(&arg.ty).is_some() {
                        true => self.open_here(arg, Use::Lend)?,
                        false => self.expr(arg, Use::Value)?,
                    }
                }
                self.why = why;
            }
            ExprKind::Method { recv, args, callee, name, .. } => {
                call = Some(self.borrowed.len());
                let why = std::mem::replace(&mut self.why, Why::Call);
                let (used, reaches) = match callee {
                    Callee::Method { this, whole: true, .. } => (if *this == This::Read { Use::Lend } else { Use::Value }, false),
                    Callee::Method { this: This::Read, .. } => (Use::Lend, true),
                    Callee::Method { this: This::Ref, .. } => (if args.is_empty() { Use::LendChange } else { Use::Reserve }, true),
                    Callee::Method { this: This::Value, .. } => (Use::Value, true),
                    // `.clone()` of a `shared` value is one more owner: nothing is reached.
                    Callee::Plain if name == "clone" && matches!(recv.ty.strip_borrow(), Ty::Shared(..)) => (Use::Value, false),
                    Callee::Plain if changes(name) => {
                        let simple = args.is_empty() || matches!(name.as_str(), "write" | "sort" | "reverse");
                        (if simple { Use::LendChange } else { Use::Reserve }, true)
                    }
                    Callee::Plain => (Use::Lend, true),
                    Callee::Take | Callee::Replace | Callee::Stream(_) => (Use::LendChange, true),
                    _ => (Use::Value, false),
                };
                let first = self.borrowed.len();
                match reaches {
                    true => self.reached(recv, used)?,
                    false => self.expr(recv, used)?,
                }
                let lent = self.borrowed.len();
                for arg in args {
                    self.expr(arg, Use::Value)?;
                }
                // The receiver that was lent to read while the arguments were computed is lent
                // to change now.
                for at in first..lent {
                    if !self.borrowed[at].reserved {
                        continue;
                    }
                    let (key, span) = (self.borrowed[at].key.clone(), self.borrowed[at].span);
                    if let Some(key) = &key {
                        let mut others = self.borrowed.iter().enumerate().rev();
                        if let Some((_, other)) = others.find(|(other_at, other)| *other_at != at && other.key.as_ref() == Some(key)) {
                            return Err(self.conflict(key, other, true, span));
                        }
                    }
                    self.borrowed[at].write = true;
                    self.borrowed[at].reserved = false;
                }
                self.why = why;
            }
            ExprKind::Open { .. } | ExprKind::Within(_) => unreachable!("a function is opened once"),
            ExprKind::Int(..) | ExprKind::Float(..) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Char(_) | ExprKind::Null | ExprKind::Var(_) | ExprKind::FuncRef(_) | ExprKind::Constant { .. } => {}
        }
        if let Some(mark) = call {
            self.end_call(expr, mark);
        }
        Ok(())
    }

    /// A value whose field, item or contents are reached: opened here if it is a `shared
    /// checked` one, otherwise walked on with the same use.
    fn reached(&mut self, holder: &mut Expr, used: Use) -> Res<()> {
        match checked_behind(&holder.ty).is_some() {
            true => self.open_here(holder, used),
            false => self.expr(holder, used),
        }
    }

    /// The value a `match` or an `is` looks into: opened for as long as their names are used.
    fn subject(&mut self, subject: &mut Expr, used: Use, why: Why) -> Res<()> {
        let outer = std::mem::replace(&mut self.why, why);
        let result = self.reached(subject, used);
        self.why = outer;
        result
    }

    /// A call (or a `match`) has ended: what was lent to it ends with it, unless its result
    /// borrows, and then it goes on as long as the result is used.
    fn end_call(&mut self, expr: &mut Expr, mark: usize) {
        if self.borrowed.len() == mark || (self.lends)(&expr.ty) {
            return;
        }
        self.borrowed.truncate(mark);
        wrap(expr, ExprKind::Within);
    }
}

impl Use {
    /// The use a loop over a `shared checked` collection makes of it: always lent.
    fn max_lend(self) -> Use {
        match self {
            Use::Value => Use::Lend,
            other => other,
        }
    }
}
