//! Interpreter for the language: parses, checks and runs source text.
//!
//! Written as a library; the language server will later reuse the same parsing and checking code.

pub mod ast;
pub mod check;
pub mod diag;
pub mod emit_c;
pub mod exhaust;
pub mod interp;
pub mod input;
pub mod lexer;
pub mod net;
pub mod parser;
pub mod raw;
pub mod shortest;
pub mod source;
pub mod opening;
pub mod sums;
pub mod types;
pub mod waits;

use std::io::Write;

pub use diag::Diag;
pub use interp::{Stop, World};
pub use source::{load, Sources};

/// The tree is walked recursively, so deep calls need stack. `interp::MAX_DEPTH` sets the limit;
/// this stack is large enough to reach it.
pub(crate) const STACK_BYTES: usize = 1 << 30;

/// Parses the files of the program into one tree. With `tests`, the test command: only the
/// `test` blocks of the files it was pointed at are kept.
fn parse(sources: &Sources, tests: bool) -> Result<ast::Program, Diag> {
    let files: Vec<(usize, usize, String)> = sources.files.iter().map(|file| (file.start, file.end, file.module.clone())).collect();
    let mut prog = parser::parse_files(&sources.text, &files)?;
    prog.tests.retain(|test| !tests || sources.file_at(test.name_span.start).tested);
    Ok(prog)
}

/// Parses and checks the program. For a name that was not found, the modules no file names
/// are looked at too: if one of them has the name and it is `public`, the hint is the missing
/// `use` line.
fn check_all(sources: &Sources, tests: bool) -> Result<ast::Program, Diag> {
    let mut prog = parse(sources, tests)?;
    match check::check(&mut prog, &sources.text, tests) {
        Ok(()) => Ok(prog),
        Err(diag) => Err(match missing_use(sources, &diag) {
            Some(module) => diag.hint(format!("`use {module}` at the top of the file")),
            None => diag,
        }),
    }
}

fn missing_use(sources: &Sources, diag: &Diag) -> Option<String> {
    if !diag.msg.starts_with("there is no ") || diag.msg.contains("module") {
        return None;
    }
    let name = diag.msg.split('`').nth(1)?;
    let all = sources.with_every_module()?;
    let unread = &all.files[sources.files.len()..];
    let files: Vec<(usize, usize, String)> = unread.iter().map(|file| (file.start, file.end, file.module.clone())).collect();
    let prog = parser::parse_files(&all.text, &files).ok()?;
    let types = prog.structs.iter().map(|decl| (&decl.name, decl.public)).chain(prog.enums.iter().map(|decl| (&decl.name, decl.public)));
    let funcs = prog.funcs.iter().map(|func| (&func.name, func.public));
    let mut modules: Vec<&str> = types.chain(funcs).filter(|(full, public)| *public && ast::short_name(full) == name).map(|(full, _)| ast::module_of(full)).collect();
    modules.sort();
    modules.first().map(|module| module.to_string())
}

/// Parses, checks and runs a program that is one text. `print` output goes to `out`. The
/// `test` blocks are left out, as in a normal build.
pub fn run(src: &str, out: &mut (dyn Write + Send)) -> Result<(), Stop> {
    run_sources(&Sources::single(src, ""), out)
}

/// Parses, checks and runs the program, with no words on its command line, an empty input,
/// and the standard error of this process.
pub fn run_sources(sources: &Sources, out: &mut (dyn Write + Send)) -> Result<(), Stop> {
    let mut err = std::io::stderr();
    run_sources_in(sources, out, World { args: Vec::new(), input: Box::new(std::io::empty()), input_is_terminal: false, input_waits: false, err: &mut err })
}

/// Parses, checks and runs the program in the world it is given: its command line, its input
/// and its standard error.
pub fn run_sources_in(sources: &Sources, out: &mut (dyn Write + Send), world: World) -> Result<(), Stop> {
    on_large_stack(|| {
        let prog = check_all(sources, false)?;
        interp::run_program(&prog, &sources.text, out, world)
    })
}

/// The test command for a program that is one text; `path` names it in the messages.
pub fn run_tests(src: &str, path: &str, out: &mut (dyn Write + Send)) -> Result<bool, Stop> {
    test_sources(&Sources::single(src, path), out)
}

/// The test command: parses and checks the program with its `test` blocks and runs each of
/// them. The report goes to `out`. Gives `true` if no test failed.
pub fn test_sources(sources: &Sources, out: &mut (dyn Write + Send)) -> Result<bool, Stop> {
    on_large_stack(|| {
        let prog = check_all(sources, true)?;
        let mut err = std::io::stderr();
        let world = World { args: Vec::new(), input: Box::new(std::io::empty()), input_is_terminal: false, input_waits: false, err: &mut err };
        let (_, failed) = interp::run_tests(&prog, sources, out, world)?;
        Ok(failed == 0)
    })
}

/// The back end for a program that is one text. `path` names the source file in the panic
/// messages of the compiled program.
pub fn emit_c(src: &str, path: &str, checked: bool) -> Result<String, Diag> {
    emit_c_sources(&Sources::single(src, path), checked)
}

/// The back end: parses and checks the program and writes it as C. `checked`: a debug build,
/// where integer overflow panics; a release build wraps.
pub fn emit_c_sources(sources: &Sources, checked: bool) -> Result<String, Diag> {
    on_large_stack(|| {
        let prog = check_all(sources, false)?;
        emit_c::emit(&prog, sources, checked, false)
    })
}

/// The back end for the test command: the program that runs the `test` blocks, each in a
/// process of its own, and reports them as the interpreter does.
pub fn emit_c_tests(sources: &Sources, checked: bool) -> Result<String, Diag> {
    on_large_stack(|| {
        let prog = check_all(sources, true)?;
        emit_c::emit(&prog, sources, checked, true)
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
