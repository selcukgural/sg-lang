//! Interpreter for the language: parses, checks and runs source text.
//!
//! Written as a library; the language server will later reuse the same parsing and checking code.

pub mod ast;
pub mod check;
pub mod diag;
pub mod interp;
pub mod lexer;
pub mod parser;
pub mod types;

use std::io::Write;

pub use diag::Diag;
pub use interp::Stop;

/// The tree is walked recursively, so deep calls need stack. `interp::MAX_DEPTH` sets the limit;
/// this stack is large enough to reach it.
const STACK_BYTES: usize = 1 << 30;

/// Parses, checks and runs the program. `print` output goes to `out`. The `test` blocks are
/// left out, as in a normal build.
pub fn run(src: &str, out: &mut (dyn Write + Send)) -> Result<(), Stop> {
    on_large_stack(|| {
        let mut prog = parser::parse(src)?;
        check::check(&mut prog, src, false)?;
        interp::run_program(&prog, src, out)
    })
}

/// The test command: parses and checks the program with its `test` blocks and runs each of
/// them. The report goes to `out`; `path` names the file in the messages. Gives `true` if no
/// test failed.
pub fn run_tests(src: &str, path: &str, out: &mut (dyn Write + Send)) -> Result<bool, Stop> {
    on_large_stack(|| {
        let mut prog = parser::parse(src)?;
        check::check(&mut prog, src, true)?;
        let (_, failed) = interp::run_tests(&prog, src, path, out)?;
        Ok(failed == 0)
    })
}

fn on_large_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(STACK_BYTES)
            .spawn_scoped(scope, work)
            .expect("could not start the interpreter thread")
            .join()
            .expect("the interpreter stopped unexpectedly")
    })
}
