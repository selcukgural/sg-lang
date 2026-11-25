//! Interpreter for the language: parses, checks and runs source text.
//!
//! Written as a library; the language server will later reuse the same parsing and checking code.

pub mod ast;
pub mod cc;
pub mod check;
pub mod static_reads;
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
pub mod project;
pub mod fetch;
pub mod sha256;
pub mod opening;
pub mod json;
pub mod lsp;
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
pub use source::{load, load_over, Sources};

/// The tree is walked recursively, so deep calls need stack: each thread of the interpreter has
/// this much, taken by the system a page at a time, and a call or a walk of a value that finds
/// too little of it left is a panic of the program (choices 1343 and 1823).
pub(crate) const STACK_BYTES: usize = 1 << 30;

/// Parses the files of the program into one tree. With `tests`, the test command: only the
/// `test` blocks of the files it was pointed at are kept.
fn parse(sources: &Sources, tests: bool) -> Result<ast::Program, Diag> {
    let files: Vec<parser::FileIn> = sources.files.iter().map(|file| (file.start, file.end, file.module.clone(), file.map.clone())).collect();
    let mut prog = parser::parse_files(&sources.text, &files)?;
    prog.tests.retain(|test| !tests || sources.file_at(test.name_span.start).tested);
    c_files(&mut prog, sources)?;
    unsafe_in_packages(sources)?;
    embeds(&mut prog, sources)?;
    prog.lib_dirs = sources.lib_dirs.clone();
    prog.home = sources.files.first().and_then(|file| std::path::Path::new(&file.path).parent()).and_then(|folder| std::path::absolute(if folder.as_os_str().is_empty() { std::path::Path::new(".") } else { folder }).ok());
    Ok(prog)
}

/// `unsafe` and `extern` in a package the program does not allow them (choice 1562): found in
/// the source of each file of a package the program reads, from a folder or from the cache, so
/// what the lock says is not trusted for it (choice 1578).
fn unsafe_in_packages(sources: &Sources) -> Result<(), Diag> {
    for file in &sources.files {
        let Some(package) = sources.packages.iter().find(|package| !file.map.prefix.is_empty() && package.name == file.map.prefix) else { continue };
        if package.allows_unsafe {
            continue;
        }
        let Some((span, word, _)) = fetch::unsafe_words(&sources.text, crate::diag::Span::new(file.start, file.end)).into_iter().next() else { continue };
        let last = package.address.rsplit('/').next().unwrap_or(&package.address);
        let name = if package.named { package.name.clone() } else if project::is_name(last) { last.to_string() } else { project::suggested(last) };
        let line = format!("require {name} {} {} unsafe", package.address, package.version);
        let hint = match package.named {
            true => format!("the program allows it by ending the package's `require` line in `unsafe`: `{line}`"),
            false => format!("the program uses it through another package, and allows it on a `require` line of its own that ends in `unsafe`: `{line}`"),
        };
        return Err(Diag::error(span, format!("`{word}` is in the package {}, and the program's `sg.project` does not allow it", package.address)).hint(hint));
    }
    Ok(())
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
        let file = sources.file_at(foreign.span.start);
        // A package brings SG and no C: `sg` compiles no C a package gives (choice 1560).
        if !file.map.prefix.is_empty() {
            return Err(Diag::error(foreign.span, format!("a package does not bring C of its own: `{written}` is in the package `{}`", file.map.prefix))
                .note("a package calls a C library the system has, as `extern \"z\"`; it is not given C to compile"));
        }
        let source = &file.path;
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
        inside(&folder, &place, written, foreign.span)?;
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

/// A file a program takes in (`embed`, `extern "./native.c"`) lies in the folder of the file
/// that names it or below it, also where a link leads: a module of someone else's does not read
/// a file of the machine that builds it (choice 1339).
fn inside(folder: &std::path::Path, place: &std::path::Path, written: &str, span: crate::diag::Span) -> Result<(), Diag> {
    let folder = if folder.as_os_str().is_empty() { std::path::Path::new(".") } else { folder };
    match (std::fs::canonicalize(folder), std::fs::canonicalize(place)) {
        (Ok(folder), Ok(found)) if found.starts_with(&folder) => Ok(()),
        (_, found) => {
            let leads = found.map(|found| format!("it leads to {}", sys_path(&found))).unwrap_or_else(|error| format!("where it leads cannot be found: {error}"));
            Err(Diag::error(span, format!("`{written}` lies outside the folder of this file")).note(leads).note("a file a program takes in lies in the folder of the file that names it or below it, also where a link leads"))
        }
    }
}

/// A path as the system writes it, without Windows' `\\?\`.
fn sys_path(path: &std::path::Path) -> String {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(text)
}

/// The most bytes a file of `embed` may have: a program carries them, and its C writes each.
pub const EMBED_LIMIT: u64 = 64 << 20;

/// Reads each file `embed("...")` names, beside the file of SG it is written in.
fn embeds(prog: &mut ast::Program, sources: &Sources) -> Result<(), Diag> {
    for embed in &mut prog.embeds {
        let source = &sources.file_at(embed.span.start).path;
        let folder = std::path::Path::new(source).parent().map(std::path::Path::to_path_buf).unwrap_or_default();
        // The path is written from the folder of the file it is in.
        let place = folder.join(&embed.written);
        let size = match std::fs::metadata(&place) {
            Ok(found) if found.is_file() => found.len(),
            _ => return Err(Diag::error(embed.span, format!("there is no file `{}` beside this file", embed.written)).note(format!("it is looked for at {}", place.display()))),
        };
        inside(&folder, &place, &embed.written, embed.span)?;
        if size > EMBED_LIMIT {
            return Err(Diag::error(embed.span, format!("`{}` has {size} bytes, and `embed` takes a file of at most 64 MB", embed.written))
                .note("the program carries the bytes, and so does the C it is compiled from"));
        }
        let bytes = std::fs::read(&place).map_err(|error| Diag::error(embed.span, format!("`{}` cannot be read: {error}", embed.written)))?;
        embed.bytes = Some(bytes.into());
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
            Some(module) => with_use(sources, diag, &module),
            None => diag,
        }),
    }
}

/// The hint of the missing `use` line, and its fix: the line put before the first line of the
/// file that is not a comment or blank (choice 1371).
fn with_use(sources: &Sources, diag: Diag, module: &str) -> Diag {
    let file = sources.file_at(diag.span.start);
    let text = &sources.text[file.start..file.end];
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("//") {
            break;
        }
        at += line.len();
    }
    let place = diag::Span::new(file.start + at, file.start + at);
    diag.hint(format!("`use {module}` at the top of the file")).fix(place, format!("use {module}\n"))
}

fn missing_use(sources: &Sources, diag: &Diag) -> Option<String> {
    if !diag.msg.starts_with("there is no ") || diag.msg.contains("module") {
        return None;
    }
    let name = diag.msg.split('`').nth(1)?;
    // The folders of the program are not modules a package can name.
    if !sources.file_at(diag.span.start).map.prefix.is_empty() {
        return None;
    }
    let all = sources.with_every_module()?;
    let unread = &all.files[sources.files.len()..];
    let files: Vec<parser::FileIn> = unread.iter().map(|file| (file.start, file.end, file.module.clone(), file.map.clone())).collect();
    let prog = parser::parse_files(&all.text, &files).ok()?;
    let types = prog.structs.iter().map(|decl| (&decl.name, decl.public)).chain(prog.enums.iter().map(|decl| (&decl.name, decl.public)));
    let funcs = prog.funcs.iter().map(|func| (&func.name, func.public));
    let mut modules: Vec<&str> = types.chain(funcs).filter(|(full, public)| *public && ast::short_name(full) == name).map(|(full, _)| ast::module_of(full)).collect();
    modules.sort();
    modules.first().map(|module| module.to_string())
}

/// Whether the standard error, a terminal, shows the colours `Log` gives its levels (choice
/// 1455): on Windows the console is asked to take the escape sequences.
pub fn colour_console() -> bool {
    sys::colour_console()
}

/// What the tool or a program tells on the standard error as it ends: a check's error, a panic,
/// `main`'s error, as compiled code tells them (1596). Nothing can be told when the standard
/// error cannot be written, so the code is the same; but a pipe whose reader is gone ends the
/// tool by `SIGPIPE` on Linux and macOS for a check's error (1604), and the program for `main`'s
/// error, as a line of `printError` does (1601), where a panic has left the signal out (1602).
pub fn tell_at_end(stop: &Stop, sources: &Sources) {
    let mut err = std::io::stderr().lock();
    let broken = |written: std::io::Result<()>| matches!(written, Err(error) if cfg!(unix) && error.kind() == std::io::ErrorKind::BrokenPipe);
    let into_a_pipe_no_one_reads = match stop {
        Stop::Diag(diag) => broken(err.write_all(diag.render_in(sources).as_bytes())) && diag.kind == diag::Kind::Error,
        Stop::Failed(text) => broken(writeln!(err, "error: {text}")),
        Stop::OutputFailed(error) => {
            let _ = writeln!(err, "error: cannot write output: {error}");
            false
        }
        Stop::OutputClosed | Stop::TestsFailed | Stop::Exit(_) => false,
    };
    drop(err);
    if into_a_pipe_no_one_reads {
        signals::end_by_closed_output();
    }
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
    run_sources_in(sources, out, World { args: Vec::new(), input: Box::new(std::io::empty()), input_is_terminal: false, input_waits: false, err_colour: false, output_is_terminal: false, err_is_terminal: false, err: &mut err })
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
        let world = World { args: Vec::new(), input: Box::new(std::io::empty()), input_is_terminal: false, input_waits: false, err_colour: false, output_is_terminal: false, err_is_terminal: false, err: &mut err };
        let (_, failed) = interp::run_tests(&prog, sources, out, world)?;
        Ok(failed == 0)
    })
}

/// Parses and checks the program, `test` blocks too, as an editor shows it (`sg lsp`): the first
/// error, or nothing.
pub fn check_sources(sources: &Sources) -> Result<(), Diag> {
    on_large_stack(|| check_all(sources, true).map(|_| ()))
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
                    Some(module) => with_use(sources, diag, &module),
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
    emit_c_freestanding_for(sources, checked, false)
}

/// The same, for the machine Clang's `--target` names when `other_machine` is set.
pub fn emit_c_freestanding_for(sources: &Sources, checked: bool, other_machine: bool) -> Result<String, Diag> {
    on_large_stack(|| {
        let mut prog = parse(sources, false)?;
        prog.freestanding = true;
        prog.other_machine = other_machine;
        if let Err(diag) = check::check(&mut prog, &sources.text, false) {
            return Err(match missing_use(sources, &diag) {
                Some(module) => with_use(sources, diag, &module),
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
    on_stack(STACK_BYTES, work)
}

/// Runs a program as `run` does, on a stack of `bytes`: what a program does where the stack runs
/// low (choice 1832) is tested without making it a billion bytes deep.
#[cfg(test)]
pub(crate) fn run_on_stack(src: &str, out: &mut (dyn Write + Send), bytes: usize) -> Result<(), Stop> {
    let sources = Sources::single(src, "");
    let mut err = std::io::stderr();
    let world = World { args: Vec::new(), input: Box::new(std::io::empty()), input_is_terminal: false, input_waits: false, err_colour: false, output_is_terminal: false, err_is_terminal: false, err: &mut err };
    on_stack(bytes, || {
        let prog = check_all(&sources, false)?;
        interp::run_program(&prog, &sources, out, world)
    })
}

fn on_stack<T: Send>(bytes: usize, work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(bytes)
            .spawn_scoped(scope, || {
                crate::interp::mark_stack(bytes);
                work()
            })
            .expect("could not start the interpreter thread")
            .join()
            .expect("the interpreter stopped unexpectedly")
    })
}
