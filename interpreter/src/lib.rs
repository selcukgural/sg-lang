//! Interpreter for the language: parses, checks and runs source text.
//!
//! Written as a library; the language server will later reuse the same parsing and checking code.

pub mod ast;
pub mod cc;
pub mod check;
pub mod diag;
pub mod emit_c;
pub mod exhaust;
pub mod header;
pub mod ffi;
pub mod freestanding;
pub mod interp;
pub mod input;
pub mod lexer;
pub mod link;
pub mod net;
pub mod parser;
pub mod raw;
pub mod shortest;
#[cfg(unix)]
pub mod signals;
#[cfg(windows)]
#[path = "sys/signals_windows.rs"]
pub mod signals;
pub mod source;
pub mod opening;
pub mod sums;
pub(crate) mod sys;
pub mod types;
pub mod unicode;
pub mod unicode_tables;
pub mod unread;
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
    c_files(&mut prog, sources)?;
    prog.lib_dirs = sources.lib_dirs.clone();
    prog.home = sources.files.first().and_then(|file| std::path::Path::new(&file.path).parent()).and_then(|folder| std::path::absolute(if folder.as_os_str().is_empty() { std::path::Path::new(".") } else { folder }).ok());
    Ok(prog)
}

/// Finds the C file each `extern "./native.c"` block names, beside the file of SG it is
/// written in, and keeps where it lies in every declaration of the block.
fn c_files(prog: &mut ast::Program, sources: &Sources) -> Result<(), Diag> {
    let mut found: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    let mut find = |foreign: &mut ast::Foreign| -> Result<(), Diag> {
        let Some(written) = &foreign.file else { return Ok(()) };
        if let Some(path) = found.get(&foreign.span.start) {
            foreign.path = Some(path.clone());
            return Ok(());
        }
        let source = &sources.file_at(foreign.span.start).path;
        let folder = std::path::Path::new(source).parent().map(std::path::Path::to_path_buf).unwrap_or_default();
        let place = folder.join(&written[2..]);
        // On Windows without `\\?\`, which the C compiler does not take in an `#include`.
        let whole = match cfg!(windows) {
            true => std::path::absolute(&place),
            false => std::fs::canonicalize(&place),
        };
        let path = match whole {
            Ok(path) if path.is_file() => path,
            _ => {
                return Err(Diag::error(foreign.span, format!("there is no C file `{written}` beside this file"))
                    .note(format!("it is looked for at {}", place.display())))
            }
        };
        let path = path.to_string_lossy().into_owned();
        found.insert(foreign.span.start, path.clone());
        foreign.path = Some(path);
        Ok(())
    };
    for (_, foreign) in &mut prog.blocks {
        find(foreign)?;
    }
    for func in &mut prog.funcs {
        if let Some(foreign) = &mut func.foreign {
            find(foreign)?;
        }
    }
    for var in &mut prog.cvars {
        find(&mut var.foreign)?;
    }
    Ok(())
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
        interp::run_program(&prog, sources, out, world)
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

/// The back end for a library C calls (`sg build --library` or `--shared`): the C of the
/// library, which has no `main`, and its header, named `name`; `links` are the libraries a
/// program adds when it links the static one.
pub fn emit_c_library(sources: &Sources, checked: bool, name: &str) -> Result<(String, String), Diag> {
    on_large_stack(|| {
        let mut prog = parse(sources, false)?;
        prog.library = true;
        match check::check(&mut prog, &sources.text, false) {
            Ok(()) => {}
            Err(diag) => {
                return Err(match missing_use(sources, &diag) {
                    Some(module) => diag.hint(format!("`use {module}` at the top of the file")),
                    None => diag,
                })
            }
        }
        let code = emit_c::emit(&prog, sources, checked, false)?;
        let links = emit_c::link_flags(&code);
        Ok((code, header::header(&prog, name, &links)))
    })
}

/// The back end for a program built without an operating system (`sg build --freestanding`):
/// its C has no `main` and includes only the headers C promises there; the code that starts
/// the machine calls its `export` functions.
pub fn emit_c_freestanding(sources: &Sources, checked: bool) -> Result<String, Diag> {
    on_large_stack(|| {
        let mut prog = parse(sources, false)?;
        prog.freestanding = true;
        if let Err(diag) = check::check(&mut prog, &sources.text, false) {
            return Err(match missing_use(sources, &diag) {
                Some(module) => diag.hint(format!("`use {module}` at the top of the file")),
                None => diag,
            });
        }
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

/// Once the C compiler could not link the program: the library it did not find, said at the
/// `extern` block that names it. `None` if each one is there.
pub fn missing_library(sources: &Sources, tests: bool, library: bool) -> Option<Diag> {
    on_large_stack(|| {
        let mut prog = parse(sources, tests).ok()?;
        prog.library = library;
        check::check(&mut prog, &sources.text, tests).ok()?;
        let used = ffi::used(&prog, tests);
        let (blocks, _) = ffi::links(&prog, &used);
        link::missing(&prog, &blocks)
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
