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
    /// Whether it calls C, which may call any `export` function back.
    calls_c: bool,
}

pub fn check(prog: &Program) -> Res<()> {
    // What a call of C may call back.
    let exports: Vec<String> = prog.funcs.iter().filter(|func| func.export).map(|func| func.name.clone()).collect();
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
            } else if let Some(target) = exports.iter().find(|export| scan.calls_c && waits.contains_key(*export)) {
                found.push((name.to_string(), Why::CallsValue(target.clone())));
            }
        }
        if found.is_empty() {
            break;
        }
        waits.extend(found);
    }

    for (_, body) in &bodies {
        locks_in(body, &waits, &taken, &exports)?;
    }
    // What gives the value of a static does not wait either: a task that finds it holds up
    // every task that reads it meanwhile.
    for decl in &prog.statics {
        static_waits(decl, &scans[decl.init.as_str()], &waits, &taken, &exports)?;
    }
    for test in &prog.tests {
        locks_in(&test.body, &waits, &taken, &exports)?;
    }

    // Which functions hold their thread, found the same way. An async function never does
    // (that is refused below), and its call only starts a task.
    // The async functions and methods, under the name a call reaches them by.
    let mut async_bodies: Vec<(String, &Block)> = prog.funcs.iter().filter(|func| func.is_async).map(|func| (func.name.clone(), &func.body)).collect();
    for (owner, methods) in prog.structs.iter().map(|decl| (&decl.name, &decl.methods)).chain(prog.enums.iter().map(|decl| (&decl.name, &decl.methods))) {
        async_bodies.extend(methods.iter().filter(|method| method.is_async).map(|method| (format!("{owner}.{}", method.name), &method.body)));
    }
    let asyncs: std::collections::HashSet<&str> = async_bodies.iter().map(|(name, _)| name.as_str()).collect();
    let mut holds: HashMap<String, Why> = HashMap::new();
    for (name, scan) in &scans {
        if let Some((span, what)) = &scan.holds {
            holds.insert(name.to_string(), Why::Holds(*span, what.clone()));
        }
    }
    loop {
        let mut found = Vec::new();
        for (name, scan) in &scans {
            if holds.contains_key(*name) || asyncs.contains(name) {
                continue;
            }
            if let Some((callee, _)) = scan.calls.iter().find(|(callee, _)| holds.contains_key(callee)) {
                found.push((name.to_string(), Why::Calls(callee.clone())));
            } else if let Some(target) = scan.values.iter().find_map(|(count, ret, _)| value_target(&taken, &holds, *count, ret)) {
                found.push((name.to_string(), Why::CallsValue(target)));
            } else if let Some(target) = exports.iter().find(|export| scan.calls_c && holds.contains_key(*export)) {
                found.push((name.to_string(), Why::CallsValue(target.clone())));
            }
        }
        if found.is_empty() {
            break;
        }
        holds.extend(found);
    }
    for (_, body) in &async_bodies {
        held_in(body, &holds, &taken, &exports)?;
    }
    for (name, _) in &async_bodies {
        awaits_itself(name, &scans, &asyncs)?;
    }
    Ok(())
}

/// Refuses what holds the thread in the body of an async function.
fn held_in(body: &Block, holds: &HashMap<String, Why>, taken: &[(String, usize, Ty)], exports: &[String]) -> Res<()> {
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
                    let diag = Diag::error(span, format!("{what} holds the thread until it is done, and an async function does not hold its thread")).note(NO_HOLD);
                    Some(match async_version(expr) {
                        Some(version) => diag.hint(format!("`await {version}` does the same work without holding the thread")),
                        None => diag.hint(RUN_APART),
                    })
                }
                // `cmd.run()` and `process.wait()` have async versions of the same name.
                ExprKind::Method { recv, name, name_span, callee: Callee::Method { owner, .. }, args } => {
                    let full = format!("{owner}.{name}");
                    call(&full, *name_span).map(|diag| match (ASYNC_METHODS.contains(&full.as_str()), short_reader(recv)) {
                        (true, Some(recv)) => {
                            let shown: Vec<String> = args.iter().map(written_arg).collect();
                            diag.hint(format!("`await {recv}.{name}Async({})` does the same work without holding the thread", shown.join(", ")))
                        }
                        _ => diag,
                    })
                }
                ExprKind::Call { callee: Callee::Static { owner, name }, name_span, args, .. } | ExprKind::Method { callee: Callee::Static { owner, name }, name_span, args, .. } => {
                    let full = format!("{owner}.{name}");
                    call(&full, *name_span).map(|diag| match ASYNC_FUNCTIONS.contains(&full.as_str()) {
                        true => {
                            let shown: Vec<String> = args.iter().map(written_arg).collect();
                            diag.hint(format!("`await {}.{name}Async({})` does the same work without holding the thread", short_name(owner), shown.join(", ")))
                        }
                        false => diag,
                    })
                }
                ExprKind::Call { name, name_span, callee: Callee::Plain, .. } => call(name, *name_span),
                ExprKind::Call { args, name_span, callee: Callee::Value | Callee::FieldValue(_), .. }
                | ExprKind::Method { args, name_span, callee: Callee::FieldValue(_), .. } => value_target(taken, holds, args.len(), &expr.ty).map(|target| {
                    let diag = Diag::error(*name_span, format!("this function value could be `{}`, which holds the thread until it is done, and an async function does not hold its thread", short_name(&target)));
                    explained(diag, &target, holds).note(NO_HOLD).hint(RUN_APART)
                }),
                _ if calls_c(expr).is_some() => exports.iter().find(|export| holds.contains_key(*export)).map(|target| {
                    let diag = Diag::error(calls_c(expr).expect("tested"), format!("this call of C may call `{}` back, which holds the thread until it is done, and an async function does not hold its thread", short_name(target)));
                    explained(diag, target, holds).note(NO_HOLD).hint(RUN_APART)
                }),
                _ => None,
            };
        },
        &mut |_| {},
    );
    error.map_or(Ok(()), Err)
}

/// The full names of the functions of the language's own module start with it.
const STD_PREFIX: &str = "std.";

/// What a function of the language's own module does at the end of its calls that holds the
/// thread, as `holding` names it.
fn inner_hold(name: &str, waits: &HashMap<String, Why>) -> Option<String> {
    let mut at = name.to_string();
    for _ in 0..waits.len() {
        match waits.get(&at)? {
            Why::Holds(_, what) => return Some(what.clone()),
            Why::Itself(..) => return None,
            Why::Calls(callee) | Why::CallsValue(callee) => at = callee.clone(),
        }
    }
    None
}

/// The methods of the language's own module that hold the thread and have an async version,
/// named with `Async` after them.
const ASYNC_METHODS: [&str; 9] = [
    "std.Command.run",
    "std.Command.output",
    "std.Command.status",
    "std.Process.wait",
    "std.UdpSocket.sendTo",
    "std.UdpSocket.receiveFrom",
    "std.UdpSocket.send",
    "std.UdpSocket.receive",
    "std.SignalListener.next",
];

/// The functions of the language's own module that hold the thread and have an async version.
const ASYNC_FUNCTIONS: [&str; 1] = ["std.SocketAddress.lookup"];

const NO_HOLD: &str = "the tasks of async functions share a few threads: one that holds its thread keeps the others from running";
const RUN_APART: &str = "a function that does it runs on a thread of its own with `await Task.run(work, x)`";

/// What an expression does by itself that holds the thread until it is done, as the
/// message names it: input or output that is not async, or a `wait`. Writing to the output
/// and to the standard error is not counted, as `print` is not.
fn holding(expr: &Expr) -> Option<(Span, String)> {
    match &expr.kind {
        ExprKind::Method { callee: Callee::Wait, name_span, .. } => Some((*name_span, "`wait`".to_string())),
        // Inside the language's own module: `Command.run` and the others wait for the program.
        ExprKind::Call { callee: Callee::System(SystemCall::Run), name_span, .. } | ExprKind::Method { callee: Callee::System(SystemCall::Run), name_span, .. } => {
            Some((*name_span, "`System.run`".to_string()))
        }
        ExprKind::Call { callee: Callee::System(SystemCall::Wait), name_span, .. } | ExprKind::Method { callee: Callee::System(SystemCall::Wait), name_span, .. } => {
            Some((*name_span, "`System.wait`".to_string()))
        }
        // `SignalListener.next` waits for a signal.
        ExprKind::Call { callee: Callee::System(SystemCall::SignalNext), name_span, .. } | ExprKind::Method { callee: Callee::System(SystemCall::SignalNext), name_span, .. } => {
            Some((*name_span, "`System.signalNext`".to_string()))
        }
        // `UdpSocket` and `SocketAddress.lookup` wait for the network.
        ExprKind::Call { callee: Callee::System(call @ (SystemCall::UdpSend | SystemCall::UdpReceive | SystemCall::Lookup)), name_span, .. }
        | ExprKind::Method { callee: Callee::System(call @ (SystemCall::UdpSend | SystemCall::UdpReceive | SystemCall::Lookup)), name_span, .. } => {
            let name = match call {
                SystemCall::UdpSend => "udpSend",
                SystemCall::UdpReceive => "udpReceive",
                _ => "lookup",
            };
            Some((*name_span, format!("`System.{name}`")))
        }
        ExprKind::Call { callee: Callee::Io(call), name_span, .. } | ExprKind::Method { callee: Callee::Io(call), name_span, .. } => {
            Some((*name_span, format!("`File.{}`", call.name())))
        }
        ExprKind::Method { recv, name, name_span, callee: Callee::Stream(_), .. } => match stream_name(recv) {
            Some("Stdout" | "Stderr") => None,
            Some("Stdin") => Some((*name_span, format!("`Stdin.{name}`"))),
            _ if matches!(recv.ty.strip_borrow(), crate::types::Ty::Reader(true)) => Some((*name_span, format!("`{name}` on the reader of a connection"))),
            _ => Some((*name_span, format!("`{name}` on a file"))),
        },
        // Binding and asking for the port do not wait for the network.
        ExprKind::Call { callee: Callee::Net(NetCall::Connect), name_span, .. } | ExprKind::Method { callee: Callee::Net(NetCall::Connect), name_span, .. } => {
            Some((*name_span, "`TcpStream.connect`".to_string()))
        }
        ExprKind::Method { name, name_span, callee: Callee::Net(NetCall::Accept), .. } => Some((*name_span, format!("`{name}` on a listener"))),
        ExprKind::Method { name, name_span, callee: Callee::Net(NetCall::Receive | NetCall::Send), .. } => Some((*name_span, format!("`{name}` on a connection"))),
        _ => None,
    }
}

/// The async call that does what a call of input or output that holds the thread does:
/// `File.readTextAsync(read path)` for `File.readText(read path)`.
fn async_version(expr: &Expr) -> Option<String> {
    let (written, args) = match &expr.kind {
        ExprKind::Call { callee: Callee::Io(call), args, .. } | ExprKind::Method { callee: Callee::Io(call), args, .. } => (format!("File.{}Async", call.name()), args),
        ExprKind::Method { callee: Callee::Stream(StreamCall::Lines), .. } => return None,
        ExprKind::Call { callee: Callee::Net(NetCall::Connect), args, .. } | ExprKind::Method { callee: Callee::Net(NetCall::Connect), args, .. } => ("TcpStream.connectAsync".to_string(), args),
        ExprKind::Method { recv, name, callee: Callee::Net(NetCall::Accept | NetCall::Receive | NetCall::Send), args, .. } => (format!("{}.{name}Async", short_reader(recv)?), args),
        ExprKind::Method { recv, name, callee: Callee::Stream(_), args, .. } => {
            let reader = match stream_name(recv) {
                Some(name) => name.to_string(),
                None => short_reader(recv)?,
            };
            (format!("{reader}.{name}Async"), args)
        }
        _ => return None,
    };
    let shown: Vec<String> = args.iter().map(written_arg).collect();
    Some(format!("{written}({})", shown.join(", ")))
}

/// The stream as the program names it, when it is a variable: `reader` in `reader.readLine(ref line)`.
fn short_reader(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Var(name) => Some(name.clone()),
        ExprKind::Borrow { expr: inner, .. } | ExprKind::Load(inner) | ExprKind::Unbox(inner) => short_reader(inner),
        _ => None,
    }
}

/// An argument as the hint writes it: a variable with the borrow in front of it, anything
/// else as `...`.
fn written_arg(arg: &Expr) -> String {
    match &arg.kind {
        ExprKind::Borrow { expr: inner, mutable } => match &inner.kind {
            ExprKind::Var(name) => format!("{} {name}", if *mutable { "ref" } else { "read" }),
            ExprKind::AsSlice(array) if matches!(&array.kind, ExprKind::Var(_)) => format!("{} {}", if *mutable { "ref" } else { "read" }, written_arg(array)),
            // A text written where a `read string` goes is lent without `read`.
            ExprKind::Str(_) => written_arg(inner),
            _ => "...".to_string(),
        },
        ExprKind::AsSlice(array) => written_arg(array),
        ExprKind::Var(name) => name.clone(),
        ExprKind::Str(text) => format!("{text:?}"),
        _ => "...".to_string(),
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
fn awaits_itself(start: &str, scans: &HashMap<&str, Body>, asyncs: &std::collections::HashSet<&str>) -> Res<()> {
    // Breadth first, with the way back to `start` for the message.
    let mut came_from: HashMap<&str, (&str, Span)> = HashMap::new();
    let mut queue: Vec<&str> = vec![start];
    let mut at = 0;
    while at < queue.len() {
        let name = queue[at];
        at += 1;
        for (callee, span) in &scans[name].awaits {
            let Some(callee_name) = asyncs.get(callee.as_str()) else {
                continue;
            };
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
            let callee = *callee_name;
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
    scan.calls_c |= calls_c(expr).is_some();
    match &expr.kind {
        ExprKind::Method { callee: Callee::Wait, name_span, .. } => {
            scan.itself.get_or_insert((*name_span, How::Wait));
        }
        ExprKind::Await(inner) => {
            scan.itself.get_or_insert((expr.span, How::Await));
            match &inner.kind {
                ExprKind::Call { name, name_span, callee: Callee::Plain, .. } => scan.awaits.push((name.clone(), *name_span)),
                ExprKind::Method { name, name_span, callee: Callee::Method { owner, .. }, .. } | ExprKind::Call { name_span, callee: Callee::Static { owner, name }, .. } => {
                    scan.awaits.push((format!("{owner}.{name}"), *name_span))
                }
                _ => {}
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

/// Where the expression calls C, if it does: a function of C, or one through a `CFunc`.
fn calls_c(expr: &Expr) -> Option<Span> {
    match &expr.kind {
        ExprKind::Call { name_span, callee: Callee::Extern | Callee::Variadic(_) | Callee::CValue(_), .. } | ExprKind::Method { name_span, callee: Callee::CFieldValue(..), .. } => Some(*name_span),
        _ => None,
    }
}

thread_local! {
    /// Whether the walk sees only the branch of this system of an `if` on the operating system
    /// (see `walk_here`).
    static HERE_ONLY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Walks a statement as `walk_stmt` does, but of an `if` on the operating system
/// (`OperatingSystem.isWindows()`) only the branch of the system the program is built for: what
/// is built from it, as the C of compiled code is, may call only what this system has. The
/// checker walks both.
pub(crate) fn walk_here<'a>(stmt: &'a Stmt, each: &mut dyn FnMut(&'a Expr)) {
    let before = HERE_ONLY.with(|here| here.replace(true));
    walk_stmt(stmt, each, &mut |_| {});
    HERE_ONLY.with(|here| here.set(before));
}

/// Walks a block: `each` sees every expression, `lock` every `lock` statement.
pub(crate) fn walk_block<'a>(block: &'a Block, each: &mut dyn FnMut(&'a Expr), lock: &mut dyn FnMut(Span)) {
    for stmt in block {
        walk_stmt(stmt, each, lock);
    }
}

pub(crate) fn walk_stmt<'a>(stmt: &'a Stmt, each: &mut dyn FnMut(&'a Expr), lock: &mut dyn FnMut(Span)) {
    match &stmt.kind {
        StmtKind::Let { init, .. } => walk_expr(init, each, lock),
        StmtKind::Assign { target, value, .. } => {
            walk_expr(target, each, lock);
            walk_expr(value, each, lock);
        }
        StmtKind::If { cond: Expr { kind: ExprKind::Call { callee: Callee::Os(os), .. } | ExprKind::Method { callee: Callee::Os(os), .. }, .. }, then, els, .. }
            if HERE_ONLY.with(|here| here.get()) =>
        {
            let taken = match os.here() {
                true => Some(then),
                false => els.as_ref(),
            };
            if let Some(taken) = taken {
                walk_block(taken, each, lock);
            }
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
        StmtKind::Unsafe(body) => walk_block(body, each, lock),
        StmtKind::Return(value) => {
            if let Some(value) = value {
                walk_expr(value, each, lock);
            }
        }
        StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Const { .. } | StmtKind::Static { .. } => {}
        StmtKind::Expr(value) | StmtKind::Discard(value) => walk_expr(value, each, lock),
    }
}

pub(crate) fn walk_expr<'a>(expr: &'a Expr, each: &mut dyn FnMut(&'a Expr), lock: &mut dyn FnMut(Span)) {
    each(expr);
    if let ExprKind::Match { arms, .. } = &expr.kind {
        for arm in arms {
            walk_block(&arm.body, each, lock);
        }
    }
    children(expr, &mut |child| walk_expr(child, each, lock));
}

/// Finds every `lock` block in a body and refuses what waits inside it.
fn locks_in(block: &Block, waits: &HashMap<String, Why>, taken: &[(String, usize, Ty)], exports: &[String]) -> Res<()> {
    let mut error = None;
    each_lock(block, &mut |body| {
        if error.is_none() {
            error = inside(body, waits, taken, exports).err();
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
            StmtKind::While { body, .. } | StmtKind::For { body, .. } | StmtKind::ForEach { body, .. } | StmtKind::As { body, .. } | StmtKind::Unsafe(body) => each_lock(body, each),
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
fn inside(body: &Block, waits: &HashMap<String, Why>, taken: &[(String, usize, Ty)], exports: &[String]) -> Res<()> {
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
                _ if calls_c(expr).is_some() => exports.iter().find(|export| waits.contains_key(*export)).map(|target| {
                    let diag = Diag::error(calls_c(expr).expect("tested"), format!("this call of C may call `{}` back, which waits, and a `lock` block does not wait", short_name(target)));
                    explained(diag, target, waits).note(NO_WAIT)
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

/// The error for the function that gives the value of a static, when it waits.
fn static_waits(decl: &StaticDecl, scan: &Body, waits: &HashMap<String, Why>, taken: &[(String, usize, Ty)], exports: &[String]) -> Res<()> {
    let Some(why) = waits.get(&decl.init) else { return Ok(()) };
    let name = decl.local.clone().unwrap_or_else(|| short_name(&decl.name).to_string());
    let diag = match why {
        Why::Itself(span, how) => {
            let what = match how {
                How::Lock => "takes a lock",
                How::Wait => "waits for a task",
                How::Await => "awaits a task",
            };
            Diag::error(*span, format!("the value of the static `{name}` is found where it is first used, and this {what}"))
        }
        Why::Calls(callee) => {
            let span = scan.calls.iter().find(|(called, _)| called == callee).map_or(decl.name_span, |(_, span)| *span);
            explained(Diag::error(span, format!("the value of the static `{name}` is found where it is first used, and `{}` waits", short_name(callee))), callee, waits)
        }
        Why::CallsValue(target) => {
            let span = scan.values.iter().find(|(count, ret, _)| value_target(taken, waits, *count, ret).is_some()).map(|(_, _, span)| *span);
            let span = span.unwrap_or(decl.name_span);
            let what = match scan.calls_c && exports.contains(target) {
                true => "this call of C may call",
                false => "this function value could be",
            };
            explained(Diag::error(span, format!("the value of the static `{name}` is found where it is first used, and {what} `{}`, which waits", short_name(target))), target, waits)
        }
        Why::Holds(..) => unreachable!("a function waits by itself or through a call"),
    };
    Err(diag.note("a task that finds the value of a static never waits, so every task that reads the static meanwhile goes on"))
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
        // What a function of the language's own module does inside is not shown: the message
        // says what it does, at the call the program wrote.
        if at.starts_with(STD_PREFIX) {
            let what = match inner_hold(&at, waits).as_deref() {
                Some("`System.run`" | "`System.wait`") => "waits for another program to end".to_string(),
                Some("`System.udpSend`" | "`System.udpReceive`") => "waits for the network".to_string(),
                Some("`System.lookup`") => "waits for a name to be looked up".to_string(),
                Some(what) => format!("does {what}"),
                None => "waits".to_string(),
            };
            let path = match chain.len() {
                1 => format!("`{}` {what}", chain[0]),
                _ => format!("{}, which {what}", chain.iter().map(|name| format!("`{name}`")).collect::<Vec<_>>().join(" calls ")),
            };
            return diag.note(path);
        }
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
