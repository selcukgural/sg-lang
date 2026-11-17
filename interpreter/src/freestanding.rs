//! A program built without an operating system (`sg build --freestanding`) does nothing that
//! needs one: no output of a process, no files, no clock, no network, no threads and no tasks,
//! no random bytes of the system, nothing of C's library that is not written into the program
//! itself. What does is an error where the program writes it; a call of a function of the
//! language's own module that needs a system is an error at the call, with what it needs.
//!
//! Runs after the checker, on the whole program, as `waits` does: whether a function of the
//! language's own module needs a system depends on the functions it calls. Those of them that
//! need one and that nothing calls are left out of the C (`Program::left_out`); a function the
//! program writes is held to the rule whether it is called or not.

use std::collections::{BTreeSet, HashMap};

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::types::Ty;
use crate::waits::walk_block;

type Res<T> = Result<T, Diag>;

/// The full names of the functions of the language's own module start with it.
const STD_PREFIX: &str = "std.";

/// What every error says of a program built this way.
const NOTE: &str = "a program built with `--freestanding` runs where there is no operating system: what needs one is done through the machine itself, with raw memory and `Memory.writeVolatile`, or through a C file beside the program";

/// Why a function of the language's own module needs a system.
#[derive(Clone)]
enum Why {
    /// It does this itself.
    Itself(String),
    /// It calls this function, which needs one.
    Calls(String),
}

/// The functions of `<math.h>` that are methods of `float` and `double`. `abs`, `min`, `max`,
/// `isNaN` and `isInfinite` are written without it.
const MATH: [&str; 18] = ["sqrt", "floor", "ceil", "round", "truncate", "exp", "log", "log2", "log10", "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "pow", "fusedMultiplyAdd"];

/// What a type holds that needs a system, as the message says it.
pub fn system_type(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Reader(_) => Some("a `StreamReader` reads a file, the input or a connection of the system".to_string()),
        Ty::Writer => Some("a `StreamWriter` writes a file or an output of the system".to_string()),
        Ty::Task(_) => Some("a `Task` runs on the threads of the system".to_string()),
        Ty::Locked(_) => Some("a `locked` value is taken with a lock of the system".to_string()),
        Ty::Source | Ty::Token => Some("cancellation belongs to the tasks of the system".to_string()),
        Ty::Listener | Ty::Socket => Some("a connection of the network is the system's".to_string()),
        Ty::Map(..) => Some("a `HashMap` draws the key of its hash from the random bytes of the system".to_string()),
        Ty::Set(_) => Some("a `HashSet` draws the key of its hash from the random bytes of the system".to_string()),
        Ty::Nullable(inner) | Ty::List(inner) | Ty::Ptr(inner) | Ty::Array(inner, _) | Ty::Slice(inner) | Ty::Read(inner) | Ty::Ref(inner) | Ty::Shared(inner, _) | Ty::Weak(inner, _) => {
            system_type(inner)
        }
        Ty::Result(ok, error) => system_type(ok).or_else(|| system_type(error)),
        Ty::Func(params, ret) => params.iter().chain([&**ret]).find_map(system_type),
        _ => None,
    }
}

/// What an expression does by itself that needs a system, with where, as the message says it.
fn needs(expr: &Expr) -> Option<(Span, String)> {
    match &expr.kind {
        ExprKind::Call { name, name_span, callee: Callee::Plain, .. } if matches!(name.as_str(), "print" | "printError") => {
            Some((*name_span, format!("`{name}` writes to an output of the process")))
        }
        ExprKind::Call { name, name_span, callee: Callee::Plain, .. } if name == "exit" => Some((*name_span, "`exit` ends the process".to_string())),
        ExprKind::Await(_) => Some((expr.span, "`await` runs on the tasks of the system".to_string())),
        ExprKind::Call { name_span, callee, .. } | ExprKind::Method { name_span, callee, .. } => {
            let what = match callee {
                Callee::Io(call) | Callee::IoAsync(call) => format!("`File.{}` reads or writes the file system", call.name()),
                Callee::Stream(_) | Callee::StreamAsync(_) => "a stream is a file, the input or an output of the system".to_string(),
                Callee::Net(_) | Callee::NetAsync(_) => "the network is the system's".to_string(),
                Callee::TaskRun => "`Task.run` starts a thread of the system".to_string(),
                Callee::Wait => "`wait` waits for a thread of the system".to_string(),
                Callee::WhenAll | Callee::WhenAny | Callee::Delay(_) => "a task runs on the threads of the system".to_string(),
                Callee::Cancel(_) => "cancellation belongs to the tasks of the system".to_string(),
                Callee::Clock(_) => "it reads the clock of the system".to_string(),
                Callee::Entropy => "it asks the system for random bytes".to_string(),
                Callee::System(_) => "it calls the system".to_string(),
                Callee::Memory(MemoryCall::Errno | MemoryCall::SetErrno | MemoryCall::IoErrorOf) => "`errno` belongs to C's library".to_string(),
                Callee::Parse(_) if matches!(&expr.ty, Ty::Result(ok, _) if matches!(**ok, Ty::Float(_))) => {
                    "a fractional number is read with C's library (`strtod`)".to_string()
                }
                _ => match &expr.kind {
                    ExprKind::Method { recv, name, callee: Callee::Plain, .. } if matches!(recv.ty, Ty::Float(_)) && MATH.contains(&name.as_str()) => {
                        format!("`{name}` is a function of C's library (`<math.h>`)")
                    }
                    _ => return None,
                },
            };
            Some((*name_span, what))
        }
        _ => None,
    }
}

/// The functions a body calls by name, with where: also a function whose address it takes,
/// which may be called through it.
fn calls(expr: &Expr) -> Option<(String, Span)> {
    match &expr.kind {
        ExprKind::Method { name, name_span, callee: Callee::Method { owner, .. }, .. } => Some((format!("{owner}.{name}"), *name_span)),
        ExprKind::Call { callee: Callee::Static { owner, name }, name_span, .. } | ExprKind::Method { callee: Callee::Static { owner, name }, name_span, .. } => {
            Some((format!("{owner}.{name}"), *name_span))
        }
        ExprKind::Call { name, name_span, callee: Callee::Plain, .. } => Some((name.clone(), *name_span)),
        ExprKind::FuncRef(name) => Some((name.clone(), expr.span)),
        _ => None,
    }
}

/// What one body does that matters here.
#[derive(Default)]
struct Body {
    /// The first thing written in it that needs a system.
    itself: Option<(Span, String)>,
    /// The functions it calls, with where.
    calls: Vec<(String, Span)>,
}

fn scan(func: &Func) -> Body {
    let mut body = Body::default();
    let mut lock = None;
    walk_block(
        &func.body,
        &mut |expr| {
            if body.itself.is_none() {
                body.itself = needs(expr).or_else(|| system_type(&expr.ty).map(|what| (expr.span, what)));
            }
            if let Some(call) = calls(expr) {
                body.calls.push(call);
            }
        },
        &mut |span| {
            lock.get_or_insert(span);
        },
    );
    if let Some(span) = lock {
        if body.itself.as_ref().is_none_or(|(first, _)| span.start < first.start) {
            body.itself = Some((span, "a `lock` block takes a lock of the system".to_string()));
        }
    }
    if func.is_async {
        body.itself = Some((func.name_span, "an async function runs on the tasks of the system".to_string()));
    }
    body
}

/// Holds the program to the rule, and gives the functions of the language's own module that
/// need a system, which are left out of the C. `declared`: the types the program writes in the
/// signatures of its functions and in its fields, with where.
pub fn check(prog: &Program, declared: &[(Span, Ty)]) -> Res<BTreeSet<String>> {
    for (span, ty) in declared {
        if let Some(what) = system_type(ty) {
            return Err(Diag::error(*span, format!("{what}, and this program is built without an operating system")).note(NOTE));
        }
    }
    // Every body, under the name a call reaches it by, in the order it is written.
    let mut bodies: Vec<(String, &Func)> = prog.funcs.iter().filter(|func| func.foreign.is_none()).map(|func| (func.name.clone(), func)).collect();
    for (owner, methods) in prog.structs.iter().map(|decl| (&decl.name, &decl.methods)).chain(prog.enums.iter().map(|decl| (&decl.name, &decl.methods))) {
        bodies.extend(methods.iter().map(|method| (format!("{owner}.{}", method.name), method)));
    }
    let scans: Vec<(String, Body, bool)> = bodies.iter().map(|(name, func)| (name.clone(), scan(func), name.starts_with(STD_PREFIX))).collect();

    // Which functions of the language's own module need a system: those that do it
    // themselves, then those that call them, until nothing more is found.
    let mut need: HashMap<String, Why> = HashMap::new();
    for (name, body, std) in &scans {
        if let (true, Some((_, what))) = (std, &body.itself) {
            need.insert(name.clone(), Why::Itself(what.clone()));
        }
    }
    loop {
        let mut found = Vec::new();
        for (name, body, std) in &scans {
            if !*std || need.contains_key(name) {
                continue;
            }
            if let Some((callee, _)) = body.calls.iter().find(|(callee, _)| need.contains_key(callee)) {
                found.push((name.clone(), Why::Calls(callee.clone())));
            }
        }
        if found.is_empty() {
            break;
        }
        need.extend(found);
    }

    // What the program writes: what needs a system there, or a call of a function of the
    // language's own module that does.
    for (_, body, std) in &scans {
        if *std {
            continue;
        }
        let call = body.calls.iter().find(|(callee, _)| need.contains_key(callee));
        let first = match (&body.itself, call) {
            (Some((span, what)), Some((_, at))) if span.start < at.start => Err((*span, what.clone())),
            (Some((span, what)), None) => Err((*span, what.clone())),
            (_, Some((callee, at))) => Ok((callee, *at)),
            (None, None) => continue,
        };
        return Err(match first {
            Err((span, what)) => Diag::error(span, format!("{what}, and this program is built without an operating system")).note(NOTE),
            Ok((callee, at)) => {
                let what = inner(callee, &need);
                Diag::error(at, format!("`{}` needs an operating system: {what}, and this program is built without one", shown(callee))).note(NOTE)
            }
        });
    }
    Ok(need.into_keys().collect())
}

/// What a function of the language's own module does at the end of its calls that needs a
/// system.
fn inner(name: &str, need: &HashMap<String, Why>) -> String {
    let mut at = name;
    for _ in 0..=need.len() {
        match need.get(at) {
            Some(Why::Itself(what)) => return what.clone(),
            Some(Why::Calls(callee)) => at = callee,
            None => break,
        }
    }
    "it calls the system".to_string()
}

/// A function of the language's own module as the program names it: `Instant.now`.
fn shown(name: &str) -> String {
    name.strip_prefix(STD_PREFIX).unwrap_or(name).to_string()
}
