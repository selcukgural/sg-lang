//! What the value of a static found when the program first uses it reads on the way: the
//! statics its initializer names, and those named in every function it calls, and in every
//! function those call. A static that needs itself, around any number of others, is a compile
//! error (see `check::statics_found`).

use crate::ast::*;
use crate::diag::Span;
use crate::types::Ty;
use std::collections::{HashMap, HashSet, VecDeque};

/// The methods a value may be given to without a call written: `$"{x}"` calls `format`, `==`
/// calls `equals`, a `HashMap` calls `hash`, `sort` calls `compare`, and a value that ends calls
/// `dispose`.
const UNWRITTEN: &[&str] = &["format", "equals", "hash", "compare", "dispose"];

/// The statics the initializer `init` reads, through the calls to the bodies `bodies` holds
/// (each by the name a call reaches it by: `name`, `Owner.name`), each with where it is read.
pub fn statics_read(init: &Expr, bodies: &HashMap<String, &Func>) -> Vec<(String, Span)> {
    let mut statics = Vec::new();
    let mut calls = VecDeque::new();
    crate::waits::walk_expr(init, &mut |expr| look(expr, &mut statics, &mut calls, bodies), &mut |_| {});
    let mut reached = HashSet::new();
    while let Some(name) = calls.pop_front() {
        if !reached.insert(name.clone()) {
            continue;
        }
        let Some(func) = bodies.get(&name) else { continue };
        crate::waits::walk_block(&func.body, &mut |expr| look(expr, &mut statics, &mut calls, bodies), &mut |_| {});
    }
    statics
}

fn look(expr: &Expr, statics: &mut Vec<(String, Span)>, calls: &mut VecDeque<String>, bodies: &HashMap<String, &Func>) {
    match &expr.kind {
        ExprKind::Call { name, callee: Callee::Plain, .. } => calls.push_back(name.clone()),
        ExprKind::Call { callee: Callee::Static { owner, name }, .. } | ExprKind::Method { callee: Callee::Static { owner, name }, .. } => calls.push_back(format!("{owner}.{name}")),
        ExprKind::Method { name, callee: Callee::Method { owner, .. }, .. } => calls.push_back(format!("{owner}.{name}")),
        ExprKind::Static(full) => statics.push((full.clone(), expr.span)),
        _ => {}
    }
    // A value of a type with a method that runs without a call written.
    let mut named = Vec::new();
    types_in(&expr.ty, &mut named);
    for owner in named {
        for method in UNWRITTEN {
            let full = format!("{owner}.{method}");
            if bodies.contains_key(&full) {
                calls.push_back(full);
            }
        }
    }
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
