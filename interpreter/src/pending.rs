//! What Windows does not run yet. Windows is reached slice by slice (the order is in the
//! decisions, "Windows, the direction"): the base first, then tasks (both in), the network, other
//! programs and C. Until its slice has come, what a program writes of the rest is an error where it is
//! written when the program is built or run on Windows, and a call of a function of the
//! language's own module that needs it is an error at the call, as a program built without an
//! operating system is told (`freestanding`). Each slice takes its part out of here; after the
//! last nothing is left.

use std::collections::BTreeSet;

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::freestanding::{hold, Rule};
use crate::types::Ty;

type Res<T> = Result<T, Diag>;

/// The part of the language a slice brings to Windows, as the message names it.
const NETWORK: &str = "the network and async input and output come to Windows in the 99th slice";
const PROGRAMS: &str = "other programs and signals come to Windows in the 100th slice";
const C: &str = "calls of C come to Windows in the 101st slice";

const NOTE: &str = "Windows is reached slice by slice: it has its base (memory, printing, files and paths, the clock, random numbers, the environment) and tasks, threads and locks";

/// What a type holds that Windows does not run yet.
fn type_needs(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Listener | Ty::Socket => Some(format!("the network is not on Windows yet: {NETWORK}")),
        Ty::CFunc(..) => Some(format!("a `CFunc` is not on Windows yet: {C}")),
        Ty::Nullable(inner) | Ty::List(inner) | Ty::Ptr(inner) | Ty::Array(inner, _) | Ty::Slice(inner) | Ty::Read(inner) | Ty::Ref(inner) | Ty::Shared(inner, _) | Ty::Weak(inner, _) => {
            type_needs(inner)
        }
        Ty::Map(key, value) => type_needs(key).or_else(|| type_needs(value)),
        Ty::Set(inner) => type_needs(inner),
        Ty::Result(ok, error) => type_needs(ok).or_else(|| type_needs(error)),
        Ty::Func(params, ret) => params.iter().chain([&**ret]).find_map(type_needs),
        _ => None,
    }
}

/// What an expression does by itself that Windows does not run yet, with where.
fn needs(expr: &Expr) -> Option<(Span, String)> {
    let network = |what: &str| Some(format!("{what} is not on Windows yet: {NETWORK}"));
    let programs = |what: &str| Some(format!("{what} is not on Windows yet: {PROGRAMS}"));
    let c = |what: &str| Some(format!("{what} is not on Windows yet: {C}"));
    let (span, what) = match &expr.kind {
        ExprKind::Call { name_span, callee, .. } | ExprKind::Method { name_span, callee, .. } => {
            let what = match callee {
                Callee::Net(_) | Callee::NetAsync(_) => network("the network"),
                Callee::IoAsync(call) => network(&format!("`File.{}Async`", call.name())),
                Callee::StreamAsync(_) => network("async input and output"),
                Callee::System(call) => match call {
                    SystemCall::Executable
                    | SystemCall::Run
                    | SystemCall::Start
                    | SystemCall::Wait
                    | SystemCall::WaitAsync
                    | SystemCall::Signal
                    | SystemCall::PipeReader
                    | SystemCall::PipeWriter => programs("running another program"),
                    SystemCall::SignalListen
                    | SystemCall::SignalNext
                    | SystemCall::SignalNextAsync
                    | SystemCall::SignalStop
                    | SystemCall::SignalRaise
                    | SystemCall::SignalNumber => programs("a signal"),
                    SystemCall::UdpBind
                    | SystemCall::UdpConnect
                    | SystemCall::UdpSend
                    | SystemCall::UdpSendAsync
                    | SystemCall::UdpReceive
                    | SystemCall::UdpReceiveAsync
                    | SystemCall::SocketName
                    | SystemCall::StreamName
                    | SystemCall::CloseSocket
                    | SystemCall::Lookup
                    | SystemCall::LookupAsync => network("the network"),
                    _ => None,
                },
                Callee::Extern | Callee::Variadic(_) | Callee::CValue(_) | Callee::CFieldValue(..) => c("a function or a variable of C"),
                Callee::Memory(MemoryCall::AddressOf) => c("`Memory.addressOf`"),
                _ => None,
            };
            (*name_span, what)
        }
        _ => return None,
    };
    what.map(|what| (span, what))
}

const WINDOWS: Rule = Rule {
    needs,
    type_needs,
    lock: None,
    async_fn: None,
    direct: |what| what.to_string(),
    call: |callee, what| format!("`{callee}` is not on Windows yet: {}", what.split_once(": ").map_or(what, |(_, why)| why)),
    note: NOTE,
    tests: true,
};

/// Holds the program to what Windows runs today: `declared` are the types it writes in its
/// signatures and its fields, with where. Gives the functions of the language's own module that
/// Windows does not run yet, which nothing the program writes calls, to be left out of the C.
pub fn check(prog: &Program, declared: &[(Span, Ty)]) -> Res<BTreeSet<String>> {
    if let Some((_, block)) = prog.blocks.first() {
        return Err(Diag::error(block.span, format!("an `extern` block is not on Windows yet: {C}")).note(NOTE));
    }
    if let Some(func) = prog.funcs.iter().find(|func| func.export) {
        return Err(Diag::error(func.name_span, format!("an `export` function is not on Windows yet: {C}")).note(NOTE));
    }
    hold(prog, declared, &WINDOWS)
}
