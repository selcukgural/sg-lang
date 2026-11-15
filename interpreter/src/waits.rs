//! A `lock` block does not wait: inside it there is no other `lock`, no `wait`, and no call of
//! a function that does one of these, followed through the calls. A task that holds a lock
//! then never waits, so it always lets the lock go, and locks and waits cannot deadlock.
//!
//! An async function does not hold its thread: inside it there is no `wait`, no input or
//! output that holds the thread until it is done, and no call of a function that does one of
//! these. And an async function does not await itself, directly or around: the task of a call
//! that is awaited lies inside the task of the caller, which would then have no end.
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

/// How a function waits by itself.
#[derive(Clone, Copy, PartialEq)]
enum How {
    Lock,
    Wait,
    Await,
}

/// Why a function waits, or holds its thread.
#[derive(Clone)]
enum Why {
    /// It takes a lock, waits for a task or awaits one here; or, for holding the thread, it
    /// does what the text says here.
    Itself(Span, How),
    Holds(Span, String),
    /// It calls this function, which waits.
    Calls(String),
    /// It calls a function value that could be this function, which waits.
    CallsValue(String),
}

/// What one body does that matters here, in the order it is written.
#[derive(Default)]
struct Body {
    /// The first `lock`, `wait` or `await` written in it (not counting what calls do).
    itself: Option<(Span, How)>,
    /// The first thing written in it that holds the thread until it is done: a `wait`, input
    /// or output that is not async. What it is, for the message.
    holds: Option<(Span, String)>,
    /// The async functions it awaits by name, with where.
    awaits: Vec<(String, Span)>,
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
                scan.itself = Some((span, How::Lock));
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

    // Which functions hold their thread, found the same way. An async function never does
    // (that is refused below), and its call only starts a task.
    let asyncs: HashMap<&str, &Func> = prog.funcs.iter().filter(|func| func.is_async).map(|func| (func.name.as_str(), func)).collect();
    let mut holds: HashMap<String, Why> = HashMap::new();
    for (name, scan) in &scans {
        if let Some((span, what)) = &scan.holds {
            holds.insert(name.to_string(), Why::Holds(*span, what.clone()));
        }
    }
    loop {
        let mut found = Vec::new();
        for (name, scan) in &scans {
            if holds.contains_key(*name) || asyncs.contains_key(name) {
                continue;
            }
            if let Some((callee, _)) = scan.calls.iter().find(|(callee, _)| holds.contains_key(callee)) {
                found.push((name.to_string(), Why::Calls(callee.clone())));
            } else if let Some(target) = scan.values.iter().find_map(|(count, ret, _)| value_target(&taken, &holds, *count, ret)) {
                found.push((name.to_string(), Why::CallsValue(target)));
            }
        }
        if found.is_empty() {
            break;
        }
        holds.extend(found);
    }
    for func in prog.funcs.iter().filter(|func| func.is_async) {
        held_in(&func.body, &holds, &taken)?;
    }
    for func in prog.funcs.iter().filter(|func| func.is_async) {
        awaits_itself(func, &scans, &asyncs)?;
    }
    Ok(())
}

/// Refuses what holds the thread in the body of an async function.
fn held_in(body: &Block, holds: &HashMap<String, Why>, taken: &[(String, usize, Ty)]) -> Res<()> {
    let mut error: Option<Diag> = None;
    walk_block(
        body,
        &mut |expr| {
            if error.is_some() {
                return;
            }
            let call = |name: &str, span: Span| {
                holds.get(name)?;
                let diag = Diag::error(span, format!("`{}` holds the thread until it is done, and an async function does not hold its thread", short_name(name)));
                Some(explained(diag, name, holds).note(NO_HOLD).hint(RUN_APART))
            };
            error = match &expr.kind {
                // `wait` itself is refused by the checker, with its own hint.
                ExprKind::Method { callee: Callee::Wait, .. } => None,
                _ if holding(expr).is_some() => {
                    let (span, what) = holding(expr).expect("just seen");
                    Some(Diag::error(span, format!("{what} holds the thread until it is done, and an async function does not hold its thread")).note(NO_HOLD).hint(RUN_APART))
                }
                ExprKind::Method { name, name_span, callee: Callee::Method { owner, .. }, .. } => call(&format!("{owner}.{name}"), *name_span),
                ExprKind::Call { callee: Callee::Static { owner, name }, name_span, .. } => call(&format!("{owner}.{name}"), *name_span),
                ExprKind::Call { name, name_span, callee: Callee::Plain, .. } => call(name, *name_span),
                ExprKind::Call { args, name_span, callee: Callee::Value | Callee::FieldValue(_), .. }
                | ExprKind::Method { args, name_span, callee: Callee::FieldValue(_), .. } => value_target(taken, holds, args.len(), &expr.ty).map(|target| {
                    let diag = Diag::error(*name_span, format!("this function value could be `{}`, which holds the thread until it is done, and an async function does not hold its thread", short_name(&target)));
                    explained(diag, &target, holds).note(NO_HOLD).hint(RUN_APART)
                }),
                _ => None,
            };
        },
        &mut |_| {},
    );
    error.map_or(Ok(()), Err)
}

const NO_HOLD: &str = "the tasks of async functions share a few threads: one that holds its thread keeps the others from running";
const RUN_APART: &str = "a function that does it runs on a thread of its own with `await Task.run(work, x)`";

/// What an expression does by itself that holds the thread until it is done, as the
/// message names it: input or output that is not async, or a `wait`. Writing to the output
/// and to the standard error is not counted, as `print` is not.
fn holding(expr: &Expr) -> Option<(Span, String)> {
    match &expr.kind {
        ExprKind::Method { callee: Callee::Wait, name_span, .. } => Some((*name_span, "`wait`".to_string())),
        ExprKind::Call { callee: Callee::Io(call), name_span, .. } | ExprKind::Method { callee: Callee::Io(call), name_span, .. } => {
            Some((*name_span, format!("`File.{}`", call.name())))
        }
        ExprKind::Method { recv, name, name_span, callee: Callee::Stream(_), .. } => match stream_name(recv) {
            Some("Stdout" | "Stderr") => None,
            Some("Stdin") => Some((*name_span, format!("`Stdin.{name}`"))),
            _ => Some((*name_span, format!("`{name}` on a file"))),
        },
        _ => None,
    }
}

/// `Stdin`, `Stdout` or `Stderr`, when the expression is one of them.
fn stream_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Var(name) if matches!(name.as_str(), "Stdin" | "Stdout" | "Stderr") => Some(name),
        ExprKind::Borrow { expr: inner, .. } | ExprKind::Load(inner) | ExprKind::Unbox(inner) => stream_name(inner),
        _ => None,
    }
}

/// Refuses an async function that awaits itself, directly or through other async functions.
fn awaits_itself(func: &Func, scans: &HashMap<&str, Body>, asyncs: &HashMap<&str, &Func>) -> Res<()> {
    let start = func.name.as_str();
    // Breadth first, with the way back to `start` for the message.
    let mut came_from: HashMap<&str, (&str, Span)> = HashMap::new();
    let mut queue: Vec<&str> = vec![start];
    let mut at = 0;
    while at < queue.len() {
        let name = queue[at];
        at += 1;
        for (callee, span) in &scans[name].awaits {
            if !asyncs.contains_key(callee.as_str()) {
                continue;
            }
            if callee == start {
                // The chain from `start` to here.
                let mut chain = vec![(name, *span)];
                let mut back = name;
                while back != start {
                    let (from, span) = came_from[back];
                    chain.push((from, span));
                    back = from;
                }
                chain.reverse();
                let (_, first) = chain[0];
                let short = short_name(start);
                let diag = match chain.len() {
                    1 => Diag::error(first, format!("the async function `{short}` awaits itself")),
                    _ => {
                        let names: Vec<String> = chain.iter().map(|(name, _)| format!("`{}`", short_name(name))).collect();
                        let (last_name, last_span) = chain[chain.len() - 1];
                        Diag::error(first, format!("the async function `{short}` awaits itself around: {} awaits `{short}`", names.join(" awaits ")))
                            .related(last_span, format!("`{}` awaits `{short}` here", short_name(last_name)))
                    }
                };
                return Err(diag
                    .note("an awaited call runs inside the task of its caller, so a task that holds itself would have no end")
                    .hint("keep the call's task in a variable, which gives it a task of its own: `var task = f(x)`, then `await task`"));
            }
            let callee = asyncs[callee.as_str()].name.as_str();
            if !came_from.contains_key(callee) && callee != start {
                came_from.insert(callee, (name, *span));
                queue.push(callee);
            }
        }
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
    if scan.holds.is_none() {
        scan.holds = holding(expr);
    }
    match &expr.kind {
        ExprKind::Method { callee: Callee::Wait, name_span, .. } => {
            scan.itself.get_or_insert((*name_span, How::Wait));
        }
        ExprKind::Await(inner) => {
            scan.itself.get_or_insert((expr.span, How::Await));
            if let ExprKind::Call { name, name_span, callee: Callee::Plain, .. } = &inner.kind {
                scan.awaits.push((name.clone(), *name_span));
            }
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
                ExprKind::Await(_) => Some(
                    Diag::error(expr.span, "a `lock` block does not await a task")
                        .note(NO_WAIT)
                        .hint("await the task before the block or after it"),
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
            Why::Holds(span, what) => {
                let path = match chain.len() {
                    1 => format!("`{}` does {what} here", chain[0]),
                    _ => format!("{}, which does {what} here", chain.iter().map(|name| format!("`{name}`")).collect::<Vec<_>>().join(" calls ")),
                };
                return diag.related(*span, path);
            }
            Why::Itself(span, how) => {
                let what = match how {
                    How::Lock => "takes a lock",
                    How::Wait => "waits for a task",
                    How::Await => "awaits a task",
                };
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
