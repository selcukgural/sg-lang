use std::io::Write;
use std::process::ExitCode;

use std::path::PathBuf;

use sg::Stop;

/// Takes the folders `-L` names out of the words (`-L folder` or `-Lfolder`), each checked and
/// made absolute. `None` after the error is written.
fn take_folders(words_left: &mut Vec<String>) -> Option<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    let mut rest = Vec::new();
    let mut words = std::mem::take(words_left).into_iter();
    while let Some(word) = words.next() {
        let written = match word.strip_prefix("-L") {
            Some("") => match words.next() {
                Some(folder) => folder,
                None => {
                    eprintln!("error: -L names a folder: `-L folder`");
                    return None;
                }
            },
            Some(folder) => folder.to_string(),
            None => {
                rest.push(word);
                continue;
            }
        };
        match sg::link::folder(&written) {
            Ok(dir) => dirs.push(dir),
            Err(error) => {
                eprintln!("error: {error}");
                return None;
            }
        }
    }
    *words_left = rest;
    Some(dirs)
}


/// Writes the program as C. Without `out` the C goes to the output; with it, the C compiler
/// turns it into a program of that name. `release`: integer overflow wraps and the C compiler
/// optimises; otherwise a debug build, which checks. `tests`: the program that runs the `test`
/// blocks instead of `main`; with a folder, those of every module in it.
/// What `sg build` makes: a program, the program that runs the tests, a library C calls,
/// static (`.a`, `.lib` on Windows) or shared (`.so`, `.dylib` on macOS, `.dll` with its
/// `.dll.lib` on Windows), or the object of a program that runs
/// without an operating system (`.o`), which the code that starts the machine is linked with.
#[derive(Clone, Copy, PartialEq)]
enum Made {
    Program,
    Tests,
    Static,
    Shared,
    Freestanding,
}

/// How a program without an operating system is compiled: for the machine `target` names
/// (Clang's `--target`), with `flags` given on to the C compiler as they are.
#[derive(Default)]
struct Machine {
    target: Option<String>,
    flags: Vec<String>,
}

fn build(path: &str, out: Option<String>, release: bool, made: Made, machine: &Machine, dirs: &[PathBuf]) -> ExitCode {
    let mut sources = match sg::load(std::path::Path::new(path), made == Made::Tests) {
        Ok(sources) => sources,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    sources.lib_dirs = dirs.to_vec();
    let name = library_name(path);
    let emitted = match made {
        Made::Tests => sg::emit_c_tests(&sources, !release).map(|code| (code, None)),
        Made::Program => sg::emit_c_sources(&sources, !release).map(|code| (code, None)),
        Made::Freestanding => sg::emit_c_freestanding_for(&sources, !release, machine.target.is_some()).map(|code| (code, None)),
        Made::Static | Made::Shared => sg::emit_c_library(&sources, !release, &name).map(|(code, header)| (code, Some(header))),
    };
    let (code, header) = match emitted {
        Ok(code) => code,
        Err(diag) => {
            eprint!("{}", diag.render_in(&sources));
            return ExitCode::FAILURE;
        }
    };
    let Some(out) = out else { return say(&code) };
    let work = match sg::cc::Work::new() {
        Ok(work) => work,
        Err(error) => {
            eprintln!("error: cannot make a folder for the build in the temporary folder: {error}");
            return ExitCode::FAILURE;
        }
    };
    let c_file = work.path().join(format!("{name}.c"));
    // The libraries the `extern` blocks of the program name.
    let libs = sg::emit_c::link_flags(&code);
    if let Err(error) = std::fs::write(&c_file, code) {
        eprintln!("error: cannot write {}: {error}", c_file.display());
        return ExitCode::FAILURE;
    }
    let level = if release { "-O2" } else { "-O0" };
    // No fused multiply-add: a fractional result is the same as in the interpreter. `-lm`
    // for the math functions (`sqrt`, `fmod`), `-pthread` for the threads of tasks.
    // A machine named by its target is compiled for by Clang, which knows them all: the one `CC`
    // names, as C's builds have it, or the first `clang` of the path.
    // On Windows the compiler is Clang anyway.
    let compiler = match machine.target {
        Some(_) if !cfg!(windows) => std::env::var("CC").ok().filter(|cc| !cc.is_empty()).unwrap_or_else(|| "clang".to_string()),
        _ => match sg::cc::compiler() {
            Ok(compiler) => compiler,
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::FAILURE;
            }
        },
    };
    let mut cc = std::process::Command::new(&compiler);
    cc.args(["-std=c11", level, "-ffp-contract=off"]);
    if made != Made::Freestanding {
        cc.args(sg::cc::compile_flags(!release));
    }
    let system = match made {
        Made::Program | Made::Tests | Made::Shared => match sg::cc::link_flags(work.path()) {
            Ok(flags) => flags,
            Err(error) => {
                eprintln!("error: cannot write the manifest of the program: {error}");
                return ExitCode::FAILURE;
            }
        },
        Made::Static | Made::Freestanding => Vec::new(),
    };
    let out = match made {
        Made::Program | Made::Tests => sg::cc::program_file(&out),
        _ => out,
    };
    // A library is code that may be placed anywhere, so that the static one goes into a
    // program that is (PIE) and into a shared library too.
    let object = work.path().join(format!("{name}.o"));
    match made {
        Made::Program | Made::Tests => {
            cc.arg("-o").arg(&out).arg(&c_file).args(sg::link::folder_flags(dirs, true)).args(&libs).args(&system);
        }
        // Code that may be placed anywhere is all Windows has.
        Made::Static => {
            if !cfg!(windows) {
                cc.arg("-fPIC");
            }
            cc.args(["-c", "-o"]).arg(&object).arg(&c_file);
        }
        // Without an operating system: only the headers C promises there, nothing of C's library
        // assumed, and no guard of the stack, which would call into it. Each function and each
        // variable in a section of its own, as Rust has them, so that a link with
        // `--gc-sections` leaves out what nothing calls.
        Made::Freestanding => {
            cc.args(["-ffreestanding", "-fno-stack-protector", "-ffunction-sections", "-fdata-sections", "-c"]);
            if let Some(target) = &machine.target {
                cc.arg(format!("--target={target}"));
            }
            cc.args(&machine.flags).arg("-o").arg(&out).arg(&c_file);
        }
        // What goes to the linker with a name in it is given whole with `-Xlinker`: after
        // `-Wl,` a comma in a name would be read as the start of another option (choice 1339).
        // Only the `export` functions are seen from outside. A shared library is never
        // unloaded: the threads of its tasks would run on in code that is gone (Linux keeps it
        // with `nodelete`; macOS does not unload a library with variables of its threads).
        // On Windows only what is exported from the DLL is seen anyway, and its import library is
        // `x.dll.lib`, beside it (Rust's name: `x.lib` is the static one).
        Made::Shared if cfg!(windows) => {
            cc.args(["-shared", "-o"]).arg(&out).arg(&c_file).args(sg::link::folder_flags(dirs, true)).args(&libs).args(&system);
            cc.args(["-Xlinker".to_string(), format!("/IMPLIB:{out}.lib")]);
        }
        Made::Shared => {
            cc.args(["-fPIC", "-shared", "-fvisibility=hidden", "-o"]).arg(&out).arg(&c_file).args(sg::link::folder_flags(dirs, true)).args(&libs).args(&system);
            if cfg!(target_os = "macos") {
                let file = std::path::Path::new(&out).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                cc.args(["-Xlinker", "-install_name", "-Xlinker"]).arg(format!("@rpath/{file}"));
            } else {
                cc.arg("-Wl,-z,nodelete");
            }
        }
    }
    let status = cc.status();
    let status = match (made, status) {
        // The static library is the object alone, in an archive made new.
        (Made::Static, Ok(status)) if status.success() => {
            let _ = std::fs::remove_file(&out);
            let archiver = if cfg!(windows) { "llvm-lib" } else { "ar" };
            let archived = sg::cc::archive(std::path::Path::new(&out), &object).status();
            match archived {
                Ok(archived) if archived.success() => Ok(archived),
                Ok(_) => {
                    eprintln!("error: `{archiver}` could not make {out}");
                    return ExitCode::FAILURE;
                }
                Err(error) => {
                    eprintln!("error: cannot run `{archiver}`: {error}");
                    return ExitCode::FAILURE;
                }
            }
        }
        (_, status) => status,
    };
    // The libraries as a program that links this one gives them: the folders of `-L` first.
    let with_folders: Vec<String> = dirs.iter().map(|dir| format!("-L{}", dir.display())).chain(libs.iter().cloned()).collect();
    match status {
        Ok(status) if status.success() && made == Made::Freestanding => {
            let links = match libs.is_empty() {
                true => String::new(),
                false => format!(", with {}", with_folders.join(" ")),
            };
            say(&format!("built {out} for a machine without an operating system; link it with the code that starts the machine{links}; it takes nothing of C's library, and of the C compiler's own only what that compiler calls on its own (GCC's atomics on 64-bit Arm, `-lgcc`)\n"))
        }
        Ok(status) if status.success() => {
            // Windows has no `rpath`: the DLLs of the libraries of `-L` go beside the program.
            if cfg!(windows) && matches!(made, Made::Program | Made::Tests | Made::Shared) {
                for line in sg::link::copy_dlls(&libs, dirs, std::path::Path::new(&out)) {
                    eprintln!("{line}");
                }
            }
            let Some(header) = header else { return ExitCode::SUCCESS };
            let header_path = std::path::Path::new(&out).with_file_name(format!("{name}.h"));
            if let Err(error) = std::fs::write(&header_path, header) {
                eprintln!("error: cannot write {}: {error}", header_path.display());
                return ExitCode::FAILURE;
            }
            let linked = match made {
                // On Windows the libraries of the system are named inside the library itself.
                Made::Static if cfg!(windows) => with_folders.iter().fold(out.clone(), |line, flag| format!("{line} {flag}")),
                Made::Static => with_folders.iter().cloned().chain(["-lm".to_string(), "-pthread".to_string()]).fold(out.clone(), |line, flag| format!("{line} {flag}")),
                Made::Shared if cfg!(windows) => format!("{out}.lib"),
                _ => {
                    let dir = std::path::Path::new(&out).parent().map(|dir| dir.display().to_string()).filter(|dir| !dir.is_empty()).unwrap_or_else(|| ".".to_string());
                    format!("-L{dir} -l{name}")
                }
            };
            match made {
                // A program finds a DLL in its own folder or in the system's.
                Made::Shared if cfg!(windows) => say(&format!("built {out}, {out}.lib and {}; a program links it with `{linked}`, and finds {out} beside itself\n", header_path.display())),
                _ => say(&format!("built {out} and {}; a program links it with `{linked}`\n", header_path.display())),
            }
        }
        Ok(_) if !libs.is_empty() => {
            if let Some(diag) = sg::missing_library(&sources, made == Made::Tests, matches!(made, Made::Static | Made::Shared)) {
                eprint!("{}", diag.render_in(&sources));
                return ExitCode::FAILURE;
            }
            eprintln!("error: the C compiler could not build the program; is every library it links ({}) installed where the C compiler looks?", libs.join(" "));
            ExitCode::FAILURE
        }
        Ok(_) if machine.target.is_some() => {
            let target = machine.target.as_deref().unwrap_or_default();
            eprintln!("error: `{compiler}` did not build the program for `{target}`; if it does not know that machine (Apple's Clang knows only its own), `CC` names a Clang that does");
            ExitCode::FAILURE
        }
        Ok(_) => {
            eprintln!("error: the C compiler did not accept the program; this is a mistake in sg");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("error: cannot run the C compiler `{compiler}`: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The name of a library: that of its folder, or of its file without `.sg`.
/// What the tool itself writes to the output: the C of `emit-c`, the line `built ...`. An output
/// closed before the end ends the tool quietly, as it ends a program (choice 1399).
fn say(text: &str) -> ExitCode {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {
            sg::signals::end_by_closed_output();
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: cannot write output: {error}");
            ExitCode::FAILURE
        }
    }
}

fn library_name(path: &str) -> String {
    let path = std::path::Path::new(path);
    let path = if path.is_dir() { std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()) } else { path.to_path_buf() };
    path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_else(|| "library".to_string())
}

/// The test command on compiled code: the tests are built into a program in a folder of its own
/// (`sg::cc::Work`), where on Windows the DLLs of `-L` are copied beside it; that program is run,
/// and its exit code is given on.
fn test_compiled(path: &str, dirs: &[PathBuf]) -> ExitCode {
    let work = match sg::cc::Work::new() {
        Ok(work) => work,
        Err(error) => {
            eprintln!("error: cannot make a folder for the tests in the temporary folder: {error}");
            return ExitCode::FAILURE;
        }
    };
    let binary = sg::cc::program_file(&work.path().join("tests").to_string_lossy());
    let built = build(path, Some(binary.clone()), false, Made::Tests, &Machine::default(), dirs);
    let status = match built == ExitCode::SUCCESS {
        true => Some(std::process::Command::new(&binary).status()),
        false => None,
    };
    let Some(status) = status else { return built };
    match status {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: cannot run the compiled tests: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The output or the standard error of the interpreter. On Windows a console is written in
/// UTF-16, which Rust's own stream does, and it refuses bytes that are not UTF-8: here such a byte
/// is shown as U+FFFD, as compiled code and a terminal of Linux or macOS show it, and a character
/// cut between two writes is kept until it is whole. Elsewhere, and into a file or a pipe, the
/// bytes go as they are.
struct Shown<W: std::io::Write + std::io::IsTerminal> {
    inner: W,
    console: bool,
    held: Vec<u8>,
}

impl<W: std::io::Write + std::io::IsTerminal> Shown<W> {
    fn new(inner: W) -> Self {
        let console = cfg!(windows) && inner.is_terminal();
        Shown { inner, console, held: Vec::new() }
    }
}

impl<W: std::io::Write + std::io::IsTerminal> std::io::Write for Shown<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if !self.console {
            return self.inner.write(bytes);
        }
        self.held.extend_from_slice(bytes);
        let mut text = String::new();
        let mut rest: &[u8] = &self.held;
        loop {
            match std::str::from_utf8(rest) {
                Ok(valid) => {
                    text.push_str(valid);
                    rest = &[];
                    break;
                }
                Err(error) => {
                    let (valid, after) = rest.split_at(error.valid_up_to());
                    text.push_str(std::str::from_utf8(valid).expect("checked"));
                    match error.error_len() {
                        Some(bad) => {
                            text.push('\u{FFFD}');
                            rest = &after[bad..];
                        }
                        // A character cut at the end waits for the next write.
                        None => {
                            rest = after;
                            break;
                        }
                    }
                }
            }
        }
        let kept = rest.to_vec();
        self.inner.write_all(text.as_bytes())?;
        self.held = kept;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// A word of the command line as the system gave it: its bytes on Linux and macOS; on Windows
/// its UTF-16 turned into UTF-8, where a half of a pair that stands alone becomes bytes that are
/// not UTF-8, so that it is the same error as elsewhere, at the same place.
fn bytes(word: std::ffi::OsString) -> Vec<u8> {
    word.into_encoded_bytes()
}

fn main() -> ExitCode {
    // A word that is not UTF-8 is left as it is for the program, which checks it.
    let words: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let args: Vec<String> = words.iter().map(|word| word.to_string_lossy().into_owned()).collect();
    const USAGE: &str = "usage: sg [-L folder]... <file.sg | folder> [args...]              runs the program; the words after it are its command line\n       sg test <file.sg | folder> [--compiled] [-L folder]...     runs its tests; with a folder, those of every module in it\n       sg build <file.sg | folder> [--release] [--test] [-o out]  compiles it through C; with --test, the program that runs its tests\n       sg build ... -L folder                                     looks for the libraries of `extern` blocks there too\n       sg build <file.sg | folder> --library|--shared [--release] [-o out]\n                                                                  a library C calls, static (.a, .lib) or shared (.so, .dylib, .dll), and its header\n       sg build <file.sg | folder> --freestanding [--target triple] [--release] [-o out.o] [-- C flags...]\n                                                                  an object for a machine without an operating system\n       sg emit-c <file.sg | folder> [--release] [--test | --library | --freestanding] [-L folder]...\n                                                                  prints the C\n       sg lsp                                                     the language server an editor starts\n       sg add <address>[@1.4] | sg update [name] | sg remove <name>\n                                                                  the packages of the `sg.project` here, and its `sg.lock`\n       sg lock                                                    writes `sg.lock` for what `sg.project` says, raising no version";
    let usage = || {
        eprintln!("{USAGE}");
        ExitCode::from(2)
    };
    // The folders `-L` names before the program, whose own words come after it.
    let mut lead = 0;
    while let Some(word) = args.get(lead) {
        match word.as_str() {
            "-L" => lead += 2,
            _ if word.starts_with("-L") => lead += 1,
            _ => break,
        }
    }
    // The commands of packages, on the `sg.project` of the current folder (choices 1564, 1580).
    if args.first().is_some_and(|word| matches!(word.as_str(), "add" | "update" | "remove" | "lock")) {
        return match sg::fetch::command(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(sg::fetch::Failure::Usage) => usage(),
            Err(sg::fetch::Failure::Error(message)) => {
                eprintln!("error: {message}");
                ExitCode::from(1)
            }
        };
    }
    // The language server an editor starts: it talks on the input and the output (choice 1370).
    if args.first().is_some_and(|word| word == "lsp") {
        if args.len() != 1 {
            return usage();
        }
        let code = sg::lsp::serve(std::io::stdin().lock(), std::io::stdout());
        return ExitCode::from(code as u8);
    }
    let (path, tests, dirs, skip) = match args.as_slice() {
        [command, rest @ ..] if command == "test" => {
            let mut rest = rest.to_vec();
            let Some(dirs) = take_folders(&mut rest) else { return ExitCode::from(2) };
            match rest.as_slice() {
                [path] if path != "--compiled" => (path.clone(), true, dirs, 0),
                [first, second] if (first == "--compiled") != (second == "--compiled") => {
                    return test_compiled(if first == "--compiled" { second } else { first }, &dirs);
                }
                _ => return usage(),
            }
        }
        [command, rest @ ..] if command == "build" || command == "emit-c" => {
            // What follows `--` is C's, `-L` too.
            let split = rest.iter().position(|word| word == "--").unwrap_or(rest.len());
            let mut ours = rest[..split].to_vec();
            // `emit-c` takes the folders too: which name of an `extern` block's library is found
            // there decides what the first line of the C says to link.
            let Some(dirs) = take_folders(&mut ours) else { return ExitCode::from(2) };
            ours.extend(rest[split..].iter().cloned());
            // A debug build checks integer overflow; `--release` wraps and optimises.
            // Not a usage: a second program, a flag given twice, `-o` or `--target` with nothing
            // after it (choices 1326 and 1327).
            let (mut path, mut out, mut release, mut made, mut wrong) = (None, None, false, Made::Program, false);
            let mut machine = Machine::default();
            let mut rest = ours.iter();
            while let Some(arg) = rest.next() {
                let kind = match arg.as_str() {
                    "--test" => Some(Made::Tests),
                    "--library" => Some(Made::Static),
                    "--shared" => Some(Made::Shared),
                    "--freestanding" => Some(Made::Freestanding),
                    _ => None,
                };
                match arg.as_str() {
                    "--release" => {
                        wrong |= release;
                        release = true;
                    }
                    "-o" => {
                        wrong |= out.is_some();
                        out = rest.next().cloned();
                        wrong |= out.is_none();
                    }
                    "--target" => {
                        wrong |= machine.target.is_some();
                        machine.target = rest.next().cloned();
                        wrong |= machine.target.is_none();
                    }
                    "--" => {
                        machine.flags = rest.by_ref().cloned().collect();
                    }
                    _ if kind.is_some() => {
                        wrong |= made != Made::Program;
                        made = kind.expect("matched");
                    }
                    _ => {
                        wrong |= path.is_some();
                        path = Some(arg);
                    }
                }
            }
            // A target and flags of C are for a machine without an operating system; `emit-c`
            // writes to the output, so `-o` is not its (1328).
            let stray = made != Made::Freestanding && (machine.target.is_some() || !machine.flags.is_empty());
            let Some(path) = path.filter(|_| !wrong && !stray && !(command == "emit-c" && out.is_some())) else {
                return usage();
            };
            let name = library_name(path);
            let stem = match made {
                Made::Static if cfg!(windows) => Some(format!("{name}.lib")),
                Made::Static => Some(format!("lib{name}.a")),
                Made::Shared if cfg!(windows) => Some(format!("{name}.dll")),
                Made::Shared if cfg!(target_os = "macos") => Some(format!("lib{name}.dylib")),
                Made::Shared => Some(format!("lib{name}.so")),
                Made::Freestanding => Some(format!("{name}.o")),
                _ => std::path::Path::new(path).file_stem().map(|stem| stem.to_string_lossy().into_owned()),
            };
            let named = out.is_some();
            let out = (command == "build").then(|| out.or(stem).unwrap_or_else(|| "a.out".to_string()));
            // An output into a folder that is not there is told before anything is compiled, as
            // a program that cannot be read is (1329); so is an output that is a folder, as the
            // program of a folder is named by default when it is built from the folder above it
            // (1398, as Go tells it).
            if let Some(out) = &out {
                let folder = std::path::Path::new(out).parent().filter(|folder| !folder.as_os_str().is_empty());
                if let Some(folder) = folder.filter(|folder| !folder.is_dir()) {
                    eprintln!("error: cannot write {out}: there is no folder {}", folder.display());
                    return ExitCode::from(2);
                }
                let file = match made {
                    Made::Program | Made::Tests => sg::cc::program_file(out),
                    _ => out.clone(),
                };
                if std::path::Path::new(&file).is_dir() {
                    match named {
                        true => eprintln!("error: cannot write {file}: it is a folder"),
                        false => eprintln!("error: `{file}` is a folder; name the program with -o"),
                    }
                    return ExitCode::from(2);
                }
            }
            return build(path, out, release, made, &machine, &dirs);
        }
        [..] if lead < args.len() => {
            let mut before = args[..lead].to_vec();
            let Some(dirs) = take_folders(&mut before) else { return ExitCode::from(2) };
            (args[lead].clone(), false, dirs, lead)
        }
        [..] if lead > 0 => {
            let mut before = args.to_vec();
            if take_folders(&mut before).is_none() {
                return ExitCode::from(2);
            }
            return usage();
        }
        _ => return usage(),
    };
    // A folder is a program whose root module is every file in it; the test command then runs
    // the tests of every module under it.
    let mut sources = match sg::load(std::path::Path::new(&path), tests) {
        Ok(sources) => sources,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    sources.lib_dirs = dirs;

    let mut out = std::io::BufWriter::new(Shown::new(std::io::stdout()));
    // A failed test is reported like an error of the program: exit code 1.
    let result = match tests {
        false => {
            use std::io::IsTerminal;
            let mut err = Shown::new(std::io::stderr());
            let input_is_terminal = std::io::stdin().is_terminal();
            let input_waits = input_waits();
            // `Log` colours its level only on a terminal (choice 1455); on Windows the console is
            // asked to take the escape sequences.
            let err_colour = std::io::stderr().is_terminal() && sg::colour_console();
            let program_args = words.into_iter().skip(skip + 1).map(bytes).collect();
            let world = sg::World { args: program_args, input: Box::new(std::io::stdin()), input_is_terminal, input_waits, err_colour, err: &mut err };
            sg::run_sources_in(&sources, &mut out, world)
        }
        true => sg::test_sources(&sources, &mut out).and_then(|passed| match passed {
            true => Ok(()),
            false => Err(Stop::TestsFailed),
        }),
    };
    // Output written so far goes out before any error message.
    let flushed = out.flush();

    // An output closed before the end ends the program as compiled code ends (1330).
    let closed = || {
        sg::signals::end_by_closed_output();
        ExitCode::SUCCESS
    };
    match result {
        Ok(()) => match flushed {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => closed(),
            Err(error) => {
                eprintln!("error: cannot write output: {error}");
                ExitCode::FAILURE
            }
        },
        Err(Stop::Diag(diag)) => {
            eprint!("{}", diag.render_in(&sources));
            ExitCode::FAILURE
        }
        // The program's error, and then the output's, if it could not be written (1603); a
        // pipe whose reader is gone is not told.
        Err(Stop::Failed(text)) => {
            eprintln!("error: {text}");
            if let Err(error) = &flushed {
                if error.kind() != std::io::ErrorKind::BrokenPipe {
                    eprintln!("error: cannot write output: {error}");
                }
            }
            ExitCode::FAILURE
        }
        Err(Stop::TestsFailed) => ExitCode::FAILURE,
        // What was printed is out, or the output was closed meanwhile, which ends the program
        // as it ends compiled code, whose writes go out at `exit`.
        Err(Stop::Exit(code)) => match flushed {
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => closed(),
            Err(error) => {
                eprintln!("error: cannot write output: {error}");
                ExitCode::FAILURE
            }
            Ok(()) => ExitCode::from(code),
        },
        Err(Stop::OutputClosed) => closed(),
        Err(Stop::OutputFailed(error)) => {
            eprintln!("error: cannot write output: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Whether the input of this process can make it wait: anything but a file (a pipe, a
/// terminal, a socket). A file is always ready.
fn input_waits() -> bool {
    #[cfg(unix)]
    let Ok(own) = std::os::fd::AsFd::as_fd(&std::io::stdin()).try_clone_to_owned() else { return true };
    #[cfg(windows)]
    let Ok(own) = std::os::windows::io::AsHandle::as_handle(&std::io::stdin()).try_clone_to_owned() else { return true };
    !std::fs::File::from(own).metadata().is_ok_and(|meta| meta.is_file())
}
