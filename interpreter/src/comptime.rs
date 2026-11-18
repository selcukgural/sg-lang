//! What a const may run when the program is compiled. Its initializer may call any function:
//! no word marks the functions that may be called there. The checker looks through the body of
//! each function the initializer reaches, and of each it calls, for what would not give the
//! same value on every machine and in every run: input and output, the clock, the random bytes
//! of the system, the environment, other programs, raw memory and C, tasks and locks, a call
//! through a function value, and the functions of a fraction that the C library of another
//! system may round otherwise. What it finds is a compile error, which shows the calls that
//! lead there.

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::types::Ty;
use std::collections::{HashMap, HashSet, VecDeque};

type Res<T> = Result<T, Diag>;

/// The methods a value may be given to without a call written: `$"{x}"` calls `format`, `==`
/// calls `equals`, a `HashMap` calls `hash`, `sort` calls `compare`, and a value that ends calls
/// `dispose`.
const UNWRITTEN: &[&str] = &["format", "equals", "hash", "compare", "dispose"];

/// The functions of a fraction whose last bit the C library of each system finds in its own
/// way: what the program computes when it runs could differ from what the compiler found.
pub const ROUNDED_ELSEWHERE: &[&str] = &[
    "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "exp", "log", "log2", "log10", "pow", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "cbrt",
    "exp2", "expMinusOne", "logOnePlus", "erf", "erfc", "gamma", "logGamma", "hypot",
];

/// What a const's initializer reaches: the functions, each by the name a call reaches it by
/// (`name`, `Owner.name`), in the order they were reached, and the other consts it reads.
pub struct Reach {
    pub funcs: Vec<String>,
    pub consts: Vec<String>,
    /// The statics read on the way, with where each is first read.
    pub statics: Vec<(String, Span)>,
    /// The first function reached that the checker has not checked yet.
    pub unchecked: Option<String>,
    /// For each function reached, the one that called it and where (none for the initializer).
    from: HashMap<String, (Option<String>, Span)>,
}

/// What one body does that matters here.
#[derive(Default)]
struct Scan {
    /// The first thing written in it that cannot run when the program is compiled: where, what
    /// it does, and why that is barred when it is not plain.
    barred: Option<(Span, String, Option<String>)>,
    calls: Vec<(String, Span)>,
    consts: Vec<String>,
    statics: Vec<(String, Span)>,
}

/// Follows the calls of a const's initializer through the bodies `bodies` holds (the checked
/// ones; `unchecked` names the others). Stops at the first unchecked one.
pub fn reach(init: &Expr, bodies: &HashMap<String, &Func>, unchecked: &HashSet<String>) -> Reach {
    let mut found = Reach { funcs: Vec::new(), consts: Vec::new(), statics: Vec::new(), unchecked: None, from: HashMap::new() };
    let first = scan_expr(init, bodies);
    found.consts.extend(first.consts);
    found.statics.extend(first.statics);
    let mut queue: VecDeque<(String, Option<String>, Span)> = first.calls.into_iter().map(|(name, span)| (name, None, span)).collect();
    while let Some((name, caller, span)) = queue.pop_front() {
        if found.from.contains_key(&name) {
            continue;
        }
        if unchecked.contains(&name) {
            found.unchecked = Some(name);
            return found;
        }
        let Some(func) = bodies.get(&name) else { continue };
        found.from.insert(name.clone(), (caller, span));
        found.funcs.push(name.clone());
        let scan = scan_body(func, bodies);
        found.consts.extend(scan.consts);
        found.statics.extend(scan.statics);
        queue.extend(scan.calls.into_iter().map(|(callee, at)| (callee, Some(name.clone()), at)));
    }
    found
}

/// Refuses what the initializer of the const `shown` and the functions it reaches do that cannot
/// run when the program is compiled; the error shows the calls that lead there.
pub fn check(init: &Expr, shown: &str, reach: &Reach, bodies: &HashMap<String, &Func>) -> Res<()> {
    let explained = |diag: Diag, why: Option<String>| match why {
        Some(why) => diag.note(why).note(LATER),
        None => diag.note(LATER),
    };
    if let Some((span, what, why)) = scan_expr(init, bodies).barred {
        return Err(explained(Diag::error(span, format!("the value of `{shown}` is found when the program is compiled, and this {what}")), why));
    }
    for name in &reach.funcs {
        let func = bodies[name];
        let what = match func.is_async {
            true => Some((func.name_span, "starts a task".to_string(), None)),
            false => scan_body(func, bodies).barred,
        };
        let Some((span, what, why)) = what else { continue };
        let barred = format!("the value of `{shown}` is found when the program is compiled, and what it runs {what}");
        // The calls from the initializer to here, the first first.
        let mut chain = vec![name.clone()];
        let mut at = name.clone();
        while let Some((Some(caller), _)) = reach.from.get(&at) {
            chain.push(caller.clone());
            at = caller.clone();
        }
        chain.reverse();
        let call_span = reach.from[&chain[0]].1;
        // What a function of the language's own module does inside is not shown: the message
        // says what it does, at the call the program wrote.
        if let Some(inside) = chain.iter().position(|name| name.starts_with("std.")) {
            let shown_chain = &chain[..=inside];
            let written_at = match inside {
                0 => call_span,
                _ => reach.from[&chain[inside]].1,
            };
            let path = path_of(shown_chain);
            return Err(explained(Diag::error(written_at, barred).note(format!("{path}, which {what}")), why));
        }
        let path = path_of(&chain);
        let path = match chain.len() {
            1 => format!("{path} {what} here"),
            _ => format!("{path}, which {what} here"),
        };
        return Err(explained(Diag::error(call_span, barred).related(span, path), why));
    }
    Ok(())
}

/// Why a value that the compiler cannot find is found when the program runs.
const LATER: &str = "a value that cannot be found when the program is compiled is a `static`, found when the program first uses it: `static T Name = f()`";

/// `a` calls `b` calls `c`.
fn path_of(chain: &[String]) -> String {
    let names: Vec<String> = chain.iter().map(|name| format!("`{}`", shown_name(name))).collect();
    names.join(" calls ")
}

/// A function as the program writes it: `load`, `Random.fromSystem`.
fn shown_name(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((owner, method)) if owner.contains('.') || owner.chars().next().is_some_and(char::is_uppercase) => format!("{}.{method}", short_name(owner)),
        _ => short_name(name).to_string(),
    }
}

fn scan_body(func: &Func, bodies: &HashMap<String, &Func>) -> Scan {
    let mut scan = Scan::default();
    let mut locked = None;
    crate::waits::walk_block(&func.body, &mut |expr| look(expr, &mut scan, bodies), &mut |stmt| {
        locked.get_or_insert(stmt.span);
    });
    if let Some(span) = locked {
        if scan.barred.as_ref().is_none_or(|(first, ..)| span.start < first.start) {
            scan.barred = Some((span, "takes a lock".to_string(), None));
        }
    }
    scan
}

fn scan_expr(init: &Expr, bodies: &HashMap<String, &Func>) -> Scan {
    let mut scan = Scan::default();
    crate::waits::walk_expr(init, &mut |expr| look(expr, &mut scan, bodies), &mut |_| {});
    scan
}

fn look(expr: &Expr, scan: &mut Scan, bodies: &HashMap<String, &Func>) {
    match &expr.kind {
        ExprKind::Call { name, name_span, callee: Callee::Plain, .. } => match name.as_str() {
            "print" | "printError" => bar(scan, *name_span, "writes out"),
            "exit" => bar(scan, *name_span, "ends"),
            _ => scan.calls.push((name.clone(), *name_span)),
        },
        ExprKind::Method { recv, name, name_span, callee: Callee::Plain, .. } if matches!(recv.ty.strip_borrow(), Ty::Float(_)) && ROUNDED_ELSEWHERE.contains(&name.as_str()) => {
            let why = format!("the C library of each system finds the last bit of `{name}` in its own way, and the program could find another value when it runs");
            if scan.barred.is_none() {
                scan.barred = Some((*name_span, format!("calls `{name}`"), Some(why)));
            }
        }
        ExprKind::Call { name_span, callee, .. } | ExprKind::Method { name_span, callee, .. } => match callee {
            Callee::Static { owner, name } => scan.calls.push((format!("{owner}.{name}"), *name_span)),
            Callee::Method { owner, .. } => {
                let ExprKind::Method { name, .. } = &expr.kind else { unreachable!("a method is called on a value") };
                scan.calls.push((format!("{owner}.{name}"), *name_span));
            }
            _ => {
                if let Some(what) = barred_call(callee) {
                    bar(scan, *name_span, what);
                }
            }
        },
        ExprKind::Await(_) => bar(scan, expr.span, "awaits a task"),
        ExprKind::Allocate { .. } => bar(scan, expr.span, "allocates raw memory"),
        // The compiler of C lays a type out for the machine the program is compiled for.
        ExprKind::SizeOf { align, .. } => bar(scan, expr.span, if *align { "measures the alignment of a type, which differs from machine to machine" } else { "measures the size of a type, which differs from machine to machine" }),
        ExprKind::CVar(name) => bar(scan, expr.span, &format!("reads `{}`, a variable of C", short_name(name))),
        ExprKind::Const(full) => scan.consts.push(full.clone()),
        // What a static holds is found when the program runs, and may change while it runs.
        ExprKind::Static(full) => {
            scan.statics.push((full.clone(), expr.span));
            let shown = full.split('#').next().map(|name| short_name(name).to_string()).unwrap_or_default();
            bar(scan, expr.span, &format!("reads the static `{shown}`"));
        }
        _ => {}
    }
    if matches!(expr.ty, Ty::Raw(_)) {
        bar(scan, expr.span, "uses raw memory");
    }
    // A value of a type with a method that runs without a call written.
    let mut named = Vec::new();
    types_in(&expr.ty, &mut named);
    for owner in named {
        for method in UNWRITTEN {
            let full = format!("{owner}.{method}");
            if bodies.contains_key(&full) {
                scan.calls.push((full, expr.span));
            }
        }
    }
}

fn bar(scan: &mut Scan, span: Span, what: &str) {
    if scan.barred.is_none() {
        scan.barred = Some((span, what.to_string(), None));
    }
}

/// What a call of the language that cannot run when the program is compiled does.
fn barred_call(callee: &Callee) -> Option<&'static str> {
    Some(match callee {
        Callee::Io(_) | Callee::IoAsync(_) => "reads or writes a file",
        Callee::Stream(_) | Callee::StreamAsync(_) => "reads or writes a stream",
        Callee::Net(_) | Callee::NetAsync(_) => "uses the network",
        Callee::System(call) => match call {
            SystemCall::Variable => "reads a variable of the environment",
            SystemCall::UdpSend | SystemCall::UdpReceive | SystemCall::Lookup => "uses the network",
            SystemCall::Run | SystemCall::Wait => "runs another program",
            SystemCall::SignalNext => "waits for a signal",
            _ => "uses the file system or the operating system",
        },
        Callee::Clock(_) => "reads the clock",
        Callee::Entropy => "draws random bytes of the system",
        Callee::Memory(_) | Callee::Pointer | Callee::FromCString => "uses raw memory",
        Callee::Extern | Callee::Variadic(_) | Callee::CValue(_) | Callee::CFieldValue(..) => "calls C",
        Callee::Value | Callee::FieldValue(_) => "calls a function value",
        Callee::TaskRun | Callee::Wait | Callee::WhenAll | Callee::WhenAny | Callee::Delay(_) => "starts or waits for a task",
        Callee::Cancel(_) => "cancels tasks",
        _ => return None,
    })
}

/// The structs and enums a type names, those inside it too.
fn types_in(ty: &Ty, found: &mut Vec<String>) {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => {
            if !found.contains(name) {
                found.push(name.clone());
            }
        }
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) | Ty::Array(inner, _) | Ty::Slice(inner) | Ty::Shared(inner, _) | Ty::Weak(inner, _) | Ty::Locked(inner) | Ty::Task(inner) | Ty::Atomic(inner) | Ty::Read(inner) | Ty::Ref(inner) => types_in(inner, found),
        Ty::Result(a, b) | Ty::Map(a, b) | Ty::Entry(a, b) => {
            types_in(a, found);
            types_in(b, found);
        }
        _ => {}
    }
}
