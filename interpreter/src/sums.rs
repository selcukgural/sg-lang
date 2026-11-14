//! Sums in a loop that cannot overflow for a number of rounds, in a checked build.
//!
//! `sum += b` inside a loop is tested for overflow at every step, and the test keeps the C
//! compiler from adding several numbers at once. When the body only adds to a variable
//! declared before the loop, each statement at most once a round, and every value added
//! comes from a type narrower than the sum (or is a constant), then before the loop starts
//! it is known how many rounds can run without the sum overflowing: a `long` that starts at
//! 0 and adds a `byte` a round has room for 2^63 / 255 rounds. The back end writes the loop
//! twice: one copy runs that many rounds without the test, the other takes over with it, so
//! a sum that does overflow stops the program at the same step and with the same message.
//! Java's and Swift's compilers split loops the same way for their index checks.
//!
//! This module only finds the sums; `emit_c` writes the two copies.

use std::collections::HashSet;

use crate::ast::{BinOp, Block, Expr, ExprKind, IndexKind, InterpPart, Stmt, StmtKind, UnOp};
use crate::types::{IntTy, Ty};

/// A variable the body of a loop adds to (or takes from), with what one round can change it
/// by at most.
pub struct Sum<'a> {
    pub name: &'a str,
    /// The least and the most one round adds: `lo <= 0 <= hi`, since a statement may not run.
    pub lo: i128,
    pub hi: i128,
    /// The statements that change it: in the copy of the loop that runs without the test,
    /// these are written without one.
    pub steps: Vec<&'a Stmt>,
}

/// The sums of a loop body. `var` is the loop's own variable, never a sum: a copy made in each
/// round, which the body may change without changing the count of rounds. `type_of` gives the type of a variable declared before the loop, when it is an integer the
/// loop can reach only by its name (not a borrow). An empty answer means the loop is written
/// once.
pub fn sums<'a>(body: &'a Block, var: &str, type_of: impl Fn(&str) -> Option<IntTy>) -> Vec<Sum<'a>> {
    let mut scan = Scan::default();
    scan.block(body);
    // A loop inside the body would run a step many times in one round. A call keeps the C
    // compiler from doing several rounds at once whatever the test does, so the test costs
    // little next to it, while a second copy of the loop calls a function from two places,
    // which the C compiler may then stop putting where it is called (`classes` went from
    // 0.30 s to 0.40 s that way).
    if scan.loops || scan.calls {
        return Vec::new();
    }
    let mut found: Vec<Sum<'a>> = Vec::new();
    let mut unbounded = HashSet::new();
    for &(name, stmt, op, value) in &scan.steps {
        if scan.bound.contains(name) || scan.spoiled.contains(name) || name == var {
            continue;
        }
        let Some(sum_ty) = type_of(name) else { continue };
        let at = match found.iter().position(|sum| sum.name == name) {
            Some(at) => at,
            None => {
                found.push(Sum { name, lo: 0, hi: 0, steps: Vec::new() });
                found.len() - 1
            }
        };
        let sum = &mut found[at];
        match bounds(value, sum_ty) {
            Some((least, most)) => {
                let (least, most) = if op == BinOp::Sub { (-most, -least) } else { (least, most) };
                sum.lo += least.min(0);
                sum.hi += most.max(0);
                sum.steps.push(stmt);
            }
            // A step with no small bound: this sum keeps its test.
            None => {
                unbounded.insert(name);
            }
        }
    }
    // A sum with a step of no small bound is left out, and so is one whose bounds would not
    // fit in a C `long long`, which no sum of a few narrow values comes near.
    let far = i128::from(i64::MAX);
    found.retain(|sum| !unbounded.contains(sum.name) && sum.hi <= far && -sum.lo <= far);
    found
}

/// The least and the most a value added to a sum of type `sum` can be, when that is small:
/// a constant, or a value of a narrower integer type (widened to the sum's, or cast to it).
fn bounds(value: &Expr, sum: IntTy) -> Option<(i128, i128)> {
    let mut value = value;
    loop {
        match &value.kind {
            ExprKind::Widen(_, inner) | ExprKind::Load(inner) => value = inner,
            ExprKind::Cast { ty: Ty::Int(to), expr } if matches!(expr.ty.strip_borrow(), Ty::Int(from) if to.min() <= from.min() && from.max() <= to.max()) => value = expr,
            _ => break,
        }
    }
    match &value.kind {
        ExprKind::Int(number, _) => return Some((*number, *number)),
        ExprKind::Unary(UnOp::Neg, inner) => {
            if let ExprKind::Int(number, _) = inner.kind {
                return Some((-number, -number));
            }
        }
        _ => {}
    }
    match value.ty.strip_borrow() {
        Ty::Int(ty) if ty.bits() < sum.bits() && sum.min() <= ty.min() && ty.max() <= sum.max() => Some((ty.min(), ty.max())),
        _ => None,
    }
}

/// What a walk over the body finds.
#[derive(Default)]
struct Scan<'a> {
    /// A loop is written inside the body.
    loops: bool,
    /// Something in the body is a call in C: a function or a method, a lookup in a map, a
    /// collection, a `new`, a formatted text.
    calls: bool,
    /// Names the body declares: such a name may be another variable than the one before the loop.
    bound: HashSet<&'a str>,
    /// Names the body changes other than by adding to them: assigned, or lent as `ref`.
    spoiled: HashSet<&'a str>,
    /// `name += value` and `name -= value`.
    steps: Vec<(&'a str, &'a Stmt, BinOp, &'a Expr)>,
}

impl<'a> Scan<'a> {
    fn block(&mut self, block: &'a Block) {
        for stmt in block {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, init, .. } => {
                self.bound.insert(name);
                self.expr(init);
            }
            StmtKind::Assign { target, op, value, through } => {
                match &target.kind {
                    ExprKind::Var(name) => match op {
                        Some(op @ (BinOp::Add | BinOp::Sub)) if !through => self.steps.push((name, stmt, *op, value)),
                        _ => {
                            self.spoiled.insert(name);
                        }
                    },
                    _ => self.expr(target),
                }
                self.expr(value);
            }
            StmtKind::If { cond, then, els, .. } => {
                self.expr(cond);
                self.block(then);
                if let Some(els) = els {
                    self.block(els);
                }
            }
            StmtKind::While { .. } | StmtKind::For { .. } | StmtKind::ForEach { .. } => self.loops = true,
            // What it opens is `shared`, which compiled code does not take yet.
            StmtKind::As { value, body, .. } => {
                self.expr(value);
                self.block(body);
            }
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(value) | StmtKind::Discard(value) => self.expr(value),
        }
    }

    /// What is lent as `ref` may be changed: every name inside it is taken as changed (a name
    /// only read there, as in `ref xs[sum]`, too: being careful costs no more than a loop
    /// written once).
    fn lent(&mut self, expr: &'a Expr) {
        self.expr(expr);
        let mut names = Vec::new();
        names_in(expr, &mut names);
        self.spoiled.extend(names);
    }

    fn expr(&mut self, expr: &'a Expr) {
        match &expr.kind {
            ExprKind::Borrow { mutable: true, expr: inner } => self.lent(inner),
            ExprKind::IsVariant { expr: inner, pattern, by_ref, .. } => {
                self.bound.extend(pattern.names());
                // `ref` names change what they point into.
                if *by_ref {
                    self.lent(inner);
                } else {
                    self.expr(inner);
                }
            }
            ExprKind::Match { subject, arms, .. } => {
                self.expr(subject);
                for arm in arms {
                    self.bound.extend(arm.pattern.names());
                    if let Some(guard) = &arm.guard {
                        self.expr(guard);
                    }
                    self.block(&arm.body);
                    if let Some(value) = &arm.value {
                        self.expr(value);
                    }
                }
            }
            _ => {
                if matches!(
                    expr.kind,
                    ExprKind::Call { .. } | ExprKind::Method { .. } | ExprKind::Collection { .. } | ExprKind::New(_) | ExprKind::Interp(_) | ExprKind::Index { kind: IndexKind::Map, .. }
                ) {
                    self.calls = true;
                }
                children(expr, &mut |child| self.expr(child));
            }
        }
    }
}

/// The names read anywhere inside an expression.
fn names_in<'a>(expr: &'a Expr, found: &mut Vec<&'a str>) {
    if let ExprKind::Var(name) = &expr.kind {
        found.push(name);
    }
    children(expr, &mut |child| names_in(child, found));
}

/// Calls `each` on the expressions directly inside `expr`; the blocks of a `match` are not
/// expressions and are left to the caller.
fn children<'a>(expr: &'a Expr, each: &mut dyn FnMut(&'a Expr)) {
    match &expr.kind {
        ExprKind::Int(..)
        | ExprKind::Float(..)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Char(_)
        | ExprKind::Null
        | ExprKind::Var(_)
        | ExprKind::Constant { .. }
        | ExprKind::FuncRef(_) => {}
        ExprKind::Interp(parts) => {
            for part in parts {
                if let InterpPart::Expr(value, _) = part {
                    each(value);
                }
            }
        }
        ExprKind::Field { recv: inner, .. }
        | ExprKind::Unary(_, inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::New(inner)
        | ExprKind::NewShared { value: inner, .. }
        | ExprKind::Open { inner, .. }
        | ExprKind::Within(inner)
        | ExprKind::Share(inner)
        | ExprKind::WeakOf(inner)
        | ExprKind::IsNull { expr: inner, .. }
        | ExprKind::Try { expr: inner, .. }
        | ExprKind::Unbox(inner)
        | ExprKind::AsSlice(inner)
        | ExprKind::Widen(_, inner)
        | ExprKind::Load(inner)
        | ExprKind::Borrow { expr: inner, .. }
        | ExprKind::IsVariant { expr: inner, .. } => each(inner),
        ExprKind::Binary(_, left, right) | ExprKind::Coalesce { left, right, .. } | ExprKind::Index { recv: left, index: right, .. } => {
            each(left);
            each(right);
        }
        ExprKind::Call { args, .. } | ExprKind::Variant { args, .. } => args.iter().for_each(each),
        ExprKind::Method { recv, args, .. } => {
            each(recv);
            args.iter().for_each(each);
        }
        ExprKind::Collection { args, items, .. } => {
            args.iter().for_each(&mut *each);
            for item in items.iter().flatten() {
                item.values.iter().for_each(&mut *each);
            }
        }
        ExprKind::Array { args, items, .. } => {
            args.iter().for_each(&mut *each);
            items.iter().flatten().for_each(&mut *each);
        }
        ExprKind::Slice { recv, from, to } => {
            each(recv);
            from.iter().chain(to.iter()).for_each(|end| each(end));
        }
        ExprKind::StructLit { fields, .. } => fields.iter().for_each(|field| each(&field.value)),
        ExprKind::Return(value) => value.iter().for_each(|value| each(value)),
        ExprKind::Match { subject, arms, .. } => {
            each(subject);
            for arm in arms {
                arm.guard.iter().for_each(&mut *each);
                arm.value.iter().for_each(&mut *each);
            }
        }
    }
}
