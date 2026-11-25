//! The rule of values that are never read: a local variable whose value nothing reads is a
//! compile error, as Go makes an unused variable one.
//!
//! The checker has already found which variable each name stands for (`Seen`); this pass goes
//! over the checked body of one function or test and tells a read from a write. Writing a
//! variable, one of its fields or one of its items is not reading it, unless the write goes
//! through a borrow, a `shared` value or raw memory to a place that lives on after the
//! variable. Every other use reads it: a method called on it, `ref x` given to a call, `try
//! x.close()`. A parameter is not looked at, nor a variable of a `Disposable` type, whose use
//! is its end.

use std::collections::HashMap;

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::types::Ty;

type Res<T> = Result<T, Diag>;

/// What the checker found in one body.
#[derive(Default)]
pub struct Seen {
    /// Each name it looked up, by where the use starts, with where the variable it found is
    /// declared.
    pub uses: HashMap<(usize, String), usize>,
    /// Each local variable, by where its name is declared: name, type, where.
    pub locals: HashMap<usize, (String, Ty, Span)>,
}

/// How a local variable came to be, for the words of the error.
#[derive(Clone, Copy)]
enum Kind<'p> {
    /// `var x = init`
    Let(&'p Expr),
    /// `for x in ...`: the text of what the loop goes over.
    Loop(Span),
    /// A name in a pattern of `match` or `is`, with the variant it is written in, if any.
    Pattern(Option<Span>),
    /// The name of an `as` or a `lock` block.
    Block,
}

#[derive(Default)]
struct Usage {
    read: bool,
    /// The first write, if the variable is written after it is declared.
    written: Option<Span>,
    /// That write is to a field or an item, not to the whole variable.
    in_part: bool,
}

struct Pass<'p> {
    seen: &'p Seen,
    kinds: HashMap<usize, Kind<'p>>,
    usage: HashMap<usize, Usage>,
}

/// Checks one body: the first local variable in it whose value is never read is the error.
pub fn check(body: &Block, seen: &Seen, disposes: &dyn Fn(&Ty) -> bool, src: &str) -> Res<()> {
    let mut pass = Pass { seen, kinds: HashMap::new(), usage: HashMap::new() };
    pass.block(body);
    let mut locals: Vec<&(String, Ty, Span)> = seen.locals.values().collect();
    locals.sort_by_key(|(_, _, span)| span.start);
    for (name, ty, span) in locals {
        if name == "_" {
            continue;
        }
        let usage = pass.usage.remove(&span.start).unwrap_or_default();
        if usage.read {
            continue;
        }
        let kind = pass.kinds.get(&span.start).copied();
        if matches!(kind, Some(Kind::Let(_))) && disposes(ty) {
            continue;
        }
        if let Some(written) = usage.written {
            let mut diag = Diag::error(*span, format!("`{name}` is given values that are never read")).related(written, "it is written here");
            if let (Some(Kind::Let(init)), true) = (kind, usage.in_part) {
                if let Some(copied) = copy_of(init, src) {
                    diag = diag
                        .note(format!("`{name}` is a copy of `{copied}`: what is written to it does not reach `{copied}`"))
                        .hint(format!("to change `{copied}` itself, borrow it: `var {name} = ref {copied}`"));
                    return Err(diag);
                }
            }
            return Err(diag.hint(format!("read `{name}` where it is needed, or take it away with the lines that write it")));
        }
        let diag = Diag::error(*span, format!("`{name}` is never used"));
        return Err(match kind {
            Some(Kind::Let(_)) | None => diag.hint("take the line away; to run what it is given and drop the value on purpose, write `_ = ...`"),
            Some(Kind::Loop(over)) => diag.hint(format!("when the loop needs no name: `for _ in {}`", &src[over.start..over.end])),
            Some(Kind::Pattern(Some(variant))) => {
                let written = format!("{}_{}", &src[variant.start..span.start], &src[span.end..variant.end]);
                diag.hint(format!("write `_` in its place: `{written}`"))
            }
            Some(Kind::Pattern(None)) => diag.hint("write `_` in its place"),
            Some(Kind::Block) => diag.note("the block is lent the value under this name and does nothing with it"),
        });
    }
    Ok(())
}

/// The place a `var` copies its value from, as written: `rect.corner` in
/// `var p = rect.corner`, when the value is a copy of a struct and not moved out of it.
fn copy_of<'s>(init: &Expr, src: &'s str) -> Option<&'s str> {
    let mut value = init;
    while let ExprKind::Load(inner) = &value.kind {
        value = inner;
    }
    if value.moved || !matches!(value.kind, ExprKind::Field { .. } | ExprKind::Index { .. }) || !matches!(init.ty, Ty::Struct(_)) {
        return None;
    }
    Some(&src[value.span.start..value.span.end])
}

/// Whether a write that goes through a value of this type lands in a place that lives on
/// after the variable it starts from.
fn reaches_out(ty: &Ty) -> bool {
    matches!(ty, Ty::Read(_) | Ty::Ref(_) | Ty::Shared(..) | Ty::Weak(..) | Ty::Raw(_) | Ty::Slice(_) | Ty::Locked(_))
}

impl<'p> Pass<'p> {
    /// The variable a use of `name` at `start` stands for. A use the checker did not note
    /// (it found the name another way) counts for every variable of that name declared before
    /// it, so that the rule never calls a variable unread that is read.
    fn found(&self, start: usize, name: &str) -> Vec<usize> {
        match self.seen.uses.get(&(start, name.to_string())) {
            Some(at) => vec![*at],
            None => self.seen.locals.iter().filter(|(at, (local, _, _))| local == name && **at < start).map(|(at, _)| *at).collect(),
        }
    }

    fn read(&mut self, start: usize, name: &str) {
        for at in self.found(start, name) {
            self.usage.entry(at).or_default().read = true;
        }
    }

    fn write(&mut self, start: usize, name: &str, span: Span, in_part: bool) {
        for at in self.found(start, name) {
            let usage = self.usage.entry(at).or_default();
            if usage.written.is_none() {
                usage.written = Some(span);
                usage.in_part = in_part;
            }
        }
    }

    fn declared(&mut self, span: Span, kind: Kind<'p>) {
        self.kinds.insert(span.start, kind);
    }

    fn block(&mut self, block: &'p Block) {
        for stmt in block {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &'p Stmt) {
        match &stmt.kind {
            StmtKind::Let { name_span, init, .. } => {
                self.expr(init);
                self.declared(*name_span, Kind::Let(init));
            }
            StmtKind::Assign { target, value, through, .. } => {
                self.expr(value);
                self.target(target, *through);
            }
            StmtKind::If { cond, then, els, .. } => {
                self.expr(cond);
                self.block(then);
                if let Some(els) = els {
                    self.block(els);
                }
            }
            StmtKind::While { cond, body, .. } => {
                self.expr(cond);
                self.block(body);
            }
            StmtKind::For { var_span, from, to, body, .. } => {
                self.expr(from);
                self.expr(to);
                self.declared(*var_span, Kind::Loop(Span { start: from.span.start, end: to.span.end }));
                self.block(body);
            }
            StmtKind::ForEach { var_span, iter, body, .. } => {
                self.expr(iter);
                self.declared(*var_span, Kind::Loop(iter.span));
                self.block(body);
            }
            StmtKind::As { value, name_span, body, .. } => {
                self.expr(value);
                self.declared(*name_span, Kind::Block);
                self.block(body);
            }
            StmtKind::Lock { items, when, body, .. } => {
                for item in items {
                    self.expr(&item.value);
                    self.declared(item.name_span, Kind::Block);
                }
                if let Some(when) = when {
                    self.expr(when);
                }
                self.block(body);
            }
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            StmtKind::Expr(value) | StmtKind::Discard(value) => self.expr(value),
            StmtKind::Unsafe(body) => self.block(body),
            StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Const { .. } | StmtKind::Static { .. } => {}
        }
    }

    /// The target of an assignment. `through`: the target is a `ref` variable, and the write
    /// goes to what it borrows.
    fn target(&mut self, target: &'p Expr, through: bool) {
        let mut place = target;
        let mut reaches = false;
        loop {
            match &place.kind {
                ExprKind::Var(name) => {
                    let declared = self.found(place.span.start, name).iter().any(|at| self.seen.locals.get(at).is_some_and(|(_, ty, _)| reaches_out(ty)));
                    if through || reaches || declared || reaches_out(&place.ty) {
                        self.read(place.span.start, name);
                    } else {
                        self.write(place.span.start, name, target.span, !std::ptr::eq(place, target));
                    }
                    return;
                }
                ExprKind::Field { recv, .. } => {
                    reaches |= reaches_out(&recv.ty);
                    place = recv;
                }
                ExprKind::Index { recv, index, kind, .. } => {
                    self.expr(index);
                    reaches |= *kind == IndexKind::Raw || reaches_out(&recv.ty);
                    place = recv;
                }
                ExprKind::Unbox(inner) => {
                    reaches |= reaches_out(&inner.ty);
                    place = inner;
                }
                ExprKind::Open { inner, .. } => {
                    reaches = true;
                    place = inner;
                }
                _ => {
                    self.expr(place);
                    return;
                }
            }
        }
    }

    /// The names a pattern declares. `variant`: the variant pattern it is written in.
    fn pattern(&mut self, pattern: &Pattern, variant: Option<Span>) {
        match &pattern.kind {
            PatternKind::Bind(_) => self.declared(pattern.span, Kind::Pattern(variant)),
            PatternKind::Variant { args: Some(args), .. } => args.iter().for_each(|arg| self.pattern(arg, Some(pattern.span))),
            PatternKind::Or(alternatives) => alternatives.iter().for_each(|alternative| self.pattern(alternative, None)),
            _ => {}
        }
    }

    fn exprs(&mut self, exprs: &'p [Expr]) {
        for expr in exprs {
            self.expr(expr);
        }
    }

    /// An expression whose value is used: every variable in it is read.
    fn expr(&mut self, expr: &'p Expr) {
        match &expr.kind {
            ExprKind::Var(name) => self.read(expr.span.start, name),
            ExprKind::Int(..) | ExprKind::Float(..) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Char(_) | ExprKind::Null | ExprKind::SizeOf { .. } | ExprKind::Constant { .. } | ExprKind::Const(_) | ExprKind::Static(_) | ExprKind::Embed(_) | ExprKind::FuncRef(_) | ExprKind::CVar(_) | ExprKind::End => {}
            ExprKind::Interp(parts) => {
                for part in parts {
                    if let InterpPart::Expr(value, _) = part {
                        self.expr(value);
                    }
                }
            }
            ExprKind::Field { recv: inner, .. }
            | ExprKind::Unary(_, inner)
            | ExprKind::New(inner)
            | ExprKind::Atomic(inner)
            | ExprKind::Share(inner)
            | ExprKind::WeakOf(inner)
            | ExprKind::Borrow { expr: inner, .. }
            | ExprKind::IsNull { expr: inner, .. }
            | ExprKind::Try { expr: inner, .. }
            | ExprKind::Unsafe(inner)
            | ExprKind::RawCast { expr: inner, .. }
            | ExprKind::Await(inner)
            | ExprKind::Unbox(inner)
            | ExprKind::AsSlice(inner)
            | ExprKind::Widen(_, inner)
            | ExprKind::Load(inner)
            | ExprKind::Open { inner, .. }
            | ExprKind::Within(inner)
            | ExprKind::Cast { expr: inner, .. }
            | ExprKind::NewShared { value: inner, .. } => self.expr(inner),
            ExprKind::Binary(_, left, right) | ExprKind::Coalesce { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            ExprKind::Cond { cond, then, els } => {
                self.expr(cond);
                self.expr(then);
                self.expr(els);
            }
            // A call of a variable that holds a function reads it; the name of a function is
            // not looked for among the variables.
            ExprKind::Call { name, name_span, args, .. } => {
                if let Some(at) = self.seen.uses.get(&(name_span.start, name.clone())) {
                    self.usage.entry(*at).or_default().read = true;
                }
                self.exprs(args);
            }
            ExprKind::Allocate { args, .. } | ExprKind::Variant { args, .. } => self.exprs(args),
            ExprKind::Method { recv, args, .. } => {
                self.expr(recv);
                self.exprs(args);
            }
            ExprKind::Index { recv, index, .. } => {
                self.expr(recv);
                self.expr(index);
            }
            ExprKind::Collection { args, items, .. } => {
                self.exprs(args);
                for item in items.iter().flatten() {
                    self.exprs(&item.values);
                }
            }
            ExprKind::Array { args, items, .. } => {
                self.exprs(args);
                if let Some(items) = items {
                    self.exprs(items);
                }
            }
            ExprKind::Slice { recv, from, to } => {
                self.expr(recv);
                for end in [from, to].into_iter().flatten() {
                    self.expr(end);
                }
            }
            ExprKind::StructLit { fields, .. } => {
                for field in fields {
                    self.expr(&field.value);
                }
            }
            ExprKind::With { base, fields } => {
                self.expr(base);
                for field in fields {
                    self.expr(&field.value);
                }
            }
            ExprKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            ExprKind::IsVariant { expr: inner, pattern, .. } => {
                self.expr(inner);
                self.pattern(pattern, None);
            }
            ExprKind::Match { subject, arms, .. } => {
                self.expr(subject);
                for arm in arms {
                    self.pattern(&arm.pattern, None);
                    if let Some(guard) = &arm.guard {
                        self.expr(guard);
                    }
                    self.block(&arm.body);
                    if let Some(value) = &arm.value {
                        self.expr(value);
                    }
                }
            }
        }
    }
}
