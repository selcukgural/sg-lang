//! A `lock` block does not wait: inside it there is no other `lock`, no `wait`, and no call of
//! a function that does one of these, followed through the calls. A task that holds a lock
//! then never waits, so it always lets the lock go, and locks and waits cannot deadlock.
//!
//! Runs after the checker, on the whole program: whether a function waits depends on the
//! functions it calls, wherever they are written. A call through a `Func` is taken to wait
//! when any function whose address the program takes, with as many parameters and the same
//! return type, waits.

use std::collections::HashMap;

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::sums::children;
use crate::types::Ty;

type Res<T> = Result<T, Diag>;

/// Why a function waits.
#[derive(Clone)]
enum Why {
    /// It takes a lock (`true`) or waits for a task (`false`) here.
    Itself(Span, bool),
    /// It calls this function, which waits.
    Calls(String),
    /// It calls a function value that could be this function, which waits.
    CallsValue(String),
}

/// What one body does that matters here, in the order it is written.
#[derive(Default)]
struct Body {
    /// The first `lock` or `wait` written in it (not counting what calls do).
    itself: Option<(Span, bool)>,
    /// The functions it calls by name, with where.
    calls: Vec<(String, Span)>,
    /// The calls through a function value: how many arguments, the return type, where.
    values: Vec<(usize, Ty, Span)>,
}

pub fn check(prog: &Program) -> Res<()> {
    // Every body, under the name a call reaches it by.
    let mut bodies: Vec<(String, &Block)> = Vec::new();
    for func in &prog.funcs {
        bodies.push((func.name.clone(), &func.body));
    }
    for decl in &prog.structs {
        for method in &decl.methods {
            bodies.push((format!("{}.{}", decl.name, method.name), &method.body));
        }
    }
    for decl in &prog.enums {
        for method in &decl.methods {
            bodies.push((format!("{}.{}", decl.name, method.name), &method.body));
        }
    }
    let mut scans: HashMap<&str, Body> = HashMap::new();
    let mut taken: Vec<(String, usize, Ty)> = Vec::new();
    for (name, body) in &bodies {
        let mut scan = Body::default();
        let mut lock = None;
        walk_block(body, &mut |expr| look(expr, &mut scan, &mut taken), &mut |span| {
            lock.get_or_insert(span);
        });
        // The one written first.
        if let Some(span) = lock {
            if scan.itself.is_none_or(|(wait, _)| span.start < wait.start) {
                scan.itself = Some((span, true));
            }
        }
        scans.insert(name, scan);
    }
    for test in &prog.tests {
        walk_block(&test.body, &mut |expr| look(expr, &mut Body::default(), &mut taken), &mut |_| {});
    }

    // Which functions wait: those that do it themselves, then those that call them, until
    // nothing more is found.
    let mut waits: HashMap<String, Why> = HashMap::new();
    for (name, scan) in &scans {
        if let Some((span, lock)) = scan.itself {
            waits.insert(name.to_string(), Why::Itself(span, lock));
        }
    }
    loop {
        let mut found = Vec::new();
        for (name, scan) in &scans {
            if waits.contains_key(*name) {
                continue;
            }
            if let Some((callee, _)) = scan.calls.iter().find(|(callee, _)| waits.contains_key(callee)) {
                found.push((name.to_string(), Why::Calls(callee.clone())));
            } else if let Some(target) = scan.values.iter().find_map(|(count, ret, _)| value_target(&taken, &waits, *count, ret)) {
                found.push((name.to_string(), Why::CallsValue(target)));
            }
        }
        if found.is_empty() {
            break;
        }
        waits.extend(found);
    }

    for (_, body) in &bodies {
        locks_in(body, &waits, &taken)?;
    }
    for test in &prog.tests {
        locks_in(&test.body, &waits, &taken)?;
    }
    Ok(())
}

/// A function whose address the program takes, that a call through a value of `count`
/// parameters returning `ret` could reach, and that waits.
fn value_target(taken: &[(String, usize, Ty)], waits: &HashMap<String, Why>, count: usize, ret: &Ty) -> Option<String> {
    taken.iter().find(|(name, params, returns)| *params == count && returns == ret && waits.contains_key(name)).map(|(name, ..)| name.clone())
}

/// Records what one expression does: a `wait`, a call, a call through a value, a function
/// whose address is taken.
fn look(expr: &Expr, scan: &mut Body, taken: &mut Vec<(String, usize, Ty)>) {
    match &expr.kind {
        ExprKind::Method { callee: Callee::Wait, name_span, .. } => {
            scan.itself.get_or_insert((*name_span, false));
        }
        ExprKind::Method { name, name_span, callee: Callee::Method { owner, .. }, .. } => scan.calls.push((format!("{owner}.{name}"), *name_span)),
        ExprKind::Call { callee: Callee::Static { owner, name }, name_span, .. } => scan.calls.push((format!("{owner}.{name}"), *name_span)),
        ExprKind::Call { name, name_span, callee: Callee::Plain, .. } => scan.calls.push((name.clone(), *name_span)),
        ExprKind::Call { args, name_span, callee: Callee::Value | Callee::FieldValue(_), .. }
        | ExprKind::Method { args, name_span, callee: Callee::FieldValue(_), .. } => scan.values.push((args.len(), expr.ty.clone(), *name_span)),
        ExprKind::FuncRef(name) => {
            if let Ty::Func(params, ret) = &expr.ty {
                taken.push((name.clone(), params.len(), (**ret).clone()));
            }
        }
        _ => {}
    }
}

/// Walks a block: `each` sees every expression, `lock` every `lock` statement.
fn walk_block<'a>(block: &'a Block, each: &mut dyn FnMut(&'a Expr), lock: &mut dyn FnMut(Span)) {
    for stmt in block {
        walk_stmt(stmt, each, lock);
    }
}

fn walk_stmt<'a>(stmt: &'a Stmt, each: &mut dyn FnMut(&'a Expr), lock: &mut dyn FnMut(Span)) {
    match &stmt.kind {
        StmtKind::Let { init, .. } => walk_expr(init, each, lock),
        StmtKind::Assign { target, value, .. } => {
            walk_expr(target, each, lock);
            walk_expr(value, each, lock);
        }
        StmtKind::If { cond, then, els, .. } => {
            walk_expr(cond, each, lock);
            walk_block(then, each, lock);
            if let Some(els) = els {
                walk_block(els, each, lock);
            }
        }
        StmtKind::While { cond, body, .. } => {
            walk_expr(cond, each, lock);
            walk_block(body, each, lock);
        }
        StmtKind::For { from, to, body, .. } => {
            walk_expr(from, each, lock);
            walk_expr(to, each, lock);
            walk_block(body, each, lock);
        }
        StmtKind::ForEach { iter, body, .. } => {
            walk_expr(iter, each, lock);
            walk_block(body, each, lock);
        }
        StmtKind::As { value, body, .. } => {
            walk_expr(value, each, lock);
            walk_block(body, each, lock);
        }
        StmtKind::Lock { items, body } => {
            lock(stmt.span);
            for item in items {
                walk_expr(&item.value, each, lock);
            }
            walk_block(body, each, lock);
        }
        StmtKind::Return(value) => {
            if let Some(value) = value {
                walk_expr(value, each, lock);
            }
        }
        StmtKind::Break | StmtKind::Continue => {}
        StmtKind::Expr(value) | StmtKind::Discard(value) => walk_expr(value, each, lock),
    }
}

fn walk_expr<'a>(expr: &'a Expr, each: &mut dyn FnMut(&'a Expr), lock: &mut dyn FnMut(Span)) {
    each(expr);
    if let ExprKind::Match { arms, .. } = &expr.kind {
        for arm in arms {
            walk_block(&arm.body, each, lock);
        }
    }
    children(expr, &mut |child| walk_expr(child, each, lock));
}

/// Finds every `lock` block in a body and refuses what waits inside it.
fn locks_in(block: &Block, waits: &HashMap<String, Why>, taken: &[(String, usize, Ty)]) -> Res<()> {
    let mut error = None;
    each_lock(block, &mut |body| {
        if error.is_none() {
            error = inside(body, waits, taken).err();
        }
    });
    error.map_or(Ok(()), Err)
}

/// Calls `each` with the body of every outermost `lock` block in `block`.
fn each_lock<'a>(block: &'a Block, each: &mut dyn FnMut(&'a Block)) {
    for stmt in block {
        match &stmt.kind {
            StmtKind::Lock { body, .. } => each(body),
            StmtKind::If { then, els, .. } => {
                each_lock(then, each);
                if let Some(els) = els {
                    each_lock(els, each);
                }
            }
            StmtKind::While { body, .. } | StmtKind::For { body, .. } | StmtKind::ForEach { body, .. } | StmtKind::As { body, .. } => each_lock(body, each),
            _ => {}
        }
        // A `match` arm is a block too.
        let mut arms: Vec<&'a Block> = Vec::new();
        stmt_arms(stmt, &mut arms);
        for arm in arms {
            each_lock(arm, each);
        }
    }
}

/// The blocks of the `match` arms anywhere in the expressions of a statement.
fn stmt_arms<'a>(stmt: &'a Stmt, arms: &mut Vec<&'a Block>) {
    let mut exprs: Vec<&'a Expr> = Vec::new();
    match &stmt.kind {
        StmtKind::Let { init, .. } => exprs.push(init),
        StmtKind::Assign { target, value, .. } => exprs.extend([target, value]),
        StmtKind::If { cond, .. } | StmtKind::While { cond, .. } => exprs.push(cond),
        StmtKind::For { from, to, .. } => exprs.extend([from, to]),
        StmtKind::ForEach { iter, .. } => exprs.push(iter),
        StmtKind::As { value, .. } => exprs.push(value),
        StmtKind::Return(Some(value)) | StmtKind::Expr(value) | StmtKind::Discard(value) => exprs.push(value),
        _ => {}
    }
    for expr in exprs {
        arms_in(expr, arms);
    }
}

fn arms_in<'a>(expr: &'a Expr, arms: &mut Vec<&'a Block>) {
    if let ExprKind::Match { arms: found, .. } = &expr.kind {
        arms.extend(found.iter().map(|arm| &arm.body));
    }
    children(expr, &mut |child| arms_in(child, arms));
}

/// What waits inside the body of one `lock` block, as an error.
fn inside(body: &Block, waits: &HashMap<String, Why>, taken: &[(String, usize, Ty)]) -> Res<()> {
    let mut error: Option<Diag> = None;
    let mut nested = None;
    walk_block(
        body,
        &mut |expr| {
            if error.is_some() {
                return;
            }
            error = match &expr.kind {
                ExprKind::Method { callee: Callee::Wait, name_span, .. } => Some(
                    Diag::error(*name_span, "a `lock` block does not wait for a task")
                        .note(NO_WAIT)
                        .hint("wait for the task before the block or after it"),
                ),
                ExprKind::Method { name, name_span, callee: Callee::Method { owner, .. }, .. } => called(&format!("{owner}.{name}"), *name_span, waits),
                ExprKind::Call { callee: Callee::Static { owner, name }, name_span, .. } => called(&format!("{owner}.{name}"), *name_span, waits),
                ExprKind::Call { name, name_span, callee: Callee::Plain, .. } => called(name, *name_span, waits),
                ExprKind::Call { args, name_span, callee: Callee::Value | Callee::FieldValue(_), .. }
                | ExprKind::Method { args, name_span, callee: Callee::FieldValue(_), .. } => value_target(taken, waits, args.len(), &expr.ty).map(|target| {
                    let diag = Diag::error(*name_span, format!("this function value could be `{}`, which waits, and a `lock` block does not wait", short_name(&target)));
                    explained(diag, &target, waits).note(NO_WAIT)
                }),
                _ => None,
            };
        },
        &mut |span| {
            nested.get_or_insert(span);
        },
    );
    if let Some(diag) = error {
        return Err(diag);
    }
    if let Some(span) = nested {
        return Err(Diag::error(span, "a `lock` block does not take another lock")
            .note(NO_WAIT)
            .hint("lock both in one block, which takes them in a fixed order: `lock a as x, b as y { ... }`"));
    }
    Ok(())
}

const NO_WAIT: &str = "a task that holds a lock never waits, so it always lets the lock go: locks and waits cannot deadlock";

/// The error for a call, inside a `lock` block, of a function that waits.
fn called(name: &str, span: Span, waits: &HashMap<String, Why>) -> Option<Diag> {
    waits.get(name)?;
    let diag = Diag::error(span, format!("`{}` waits, and a `lock` block does not wait", short_name(name)));
    Some(explained(diag, name, waits).note(NO_WAIT))
}

/// Shows where the function named `name` waits, following its calls to the `lock` or the
/// `wait` that makes it wait.
fn explained(diag: Diag, name: &str, waits: &HashMap<String, Why>) -> Diag {
    let mut chain = vec![short_name(name).to_string()];
    let mut at = name.to_string();
    for _ in 0..waits.len() {
        match &waits[&at] {
            Why::Itself(span, lock) => {
                let what = if *lock { "takes a lock" } else { "waits for a task" };
                let path = match chain.len() {
                    1 => format!("`{}` {what} here", chain[0]),
                    _ => format!("{} {what} here", chain.iter().map(|name| format!("`{name}`")).collect::<Vec<_>>().join(" calls ")),
                };
                return diag.related(*span, path);
            }
            Why::Calls(callee) | Why::CallsValue(callee) => {
                chain.push(short_name(callee).to_string());
                at = callee.clone();
            }
        }
    }
    diag
}
