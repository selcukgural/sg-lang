use std::io::Write;
use std::process::ExitCode;

use sg::Stop;

/// Writes the program as C. Without `out` the C goes to the output; with it, the C compiler
/// turns it into a program of that name. `release`: integer overflow wraps and the C compiler
/// optimises; otherwise a debug build, which checks. `tests`: the program that runs the `test`
/// blocks instead of `main`; with a folder, those of every module in it.
/// What `sg build` makes: a program, the program that runs the tests, or a library C calls,
/// static (`.a`) or shared (`.so`, `.dylib` on macOS).
#[derive(Clone, Copy, PartialEq)]
enum Made {
    Program,
    Tests,
    Static,
    Shared,
}

fn build(path: &str, out: Option<String>, release: bool, made: Made) -> ExitCode {
    let sources = match sg::load(std::path::Path::new(path), made == Made::Tests) {
        Ok(sources) => sources,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    let name = library_name(path);
    let emitted = match made {
        Made::Tests => sg::emit_c_tests(&sources, !release).map(|code| (code, None)),
        Made::Program => sg::emit_c_sources(&sources, !release).map(|code| (code, None)),
        Made::Static | Made::Shared => sg::emit_c_library(&sources, !release, &name).map(|(code, header)| (code, Some(header))),
    };
    let (code, header) = match emitted {
        Ok(code) => code,
        Err(diag) => {
            eprint!("{}", diag.render_in(&sources));
            return ExitCode::FAILURE;
        }
    };
    let Some(out) = out else {
        print!("{code}");
        return ExitCode::SUCCESS;
    };
    let c_file = std::env::temp_dir().join(format!("sg-{}.c", std::process::id()));
    // The libraries the `extern` blocks of the program name.
    let libs = sg::emit_c::link_flags(&code);
    if let Err(error) = std::fs::write(&c_file, code) {
        eprintln!("error: cannot write {}: {error}", c_file.display());
        return ExitCode::FAILURE;
    }
    let level = if release { "-O2" } else { "-O0" };
    // No fused multiply-add: a fractional result is the same as in the interpreter. `-lm`
    // for the math functions (`sqrt`, `fmod`), `-pthread` for the threads of tasks.
    let mut cc = std::process::Command::new("cc");
    cc.args(["-std=c11", level, "-ffp-contract=off"]);
    // A library is code that may be placed anywhere, so that the static one goes into a
    // program that is (PIE) and into a shared library too.
    let objects = std::env::temp_dir().join(format!("sg-{}", std::process::id()));
    let object = objects.join(format!("{name}.o"));
    if made == Made::Static {
        if let Err(error) = std::fs::create_dir_all(&objects) {
            eprintln!("error: cannot make {}: {error}", objects.display());
            return ExitCode::FAILURE;
        }
    }
    match made {
        Made::Program | Made::Tests => {
            cc.arg("-o").arg(&out).arg(&c_file).args(&libs).arg("-lm").arg("-pthread");
        }
        Made::Static => {
            cc.args(["-fPIC", "-c", "-o"]).arg(&object).arg(&c_file);
        }
        // Only the `export` functions are seen from outside. A shared library is never
        // unloaded: the threads of its tasks would run on in code that is gone (Linux keeps it
        // with `nodelete`; macOS does not unload a library with variables of its threads).
        Made::Shared => {
            cc.args(["-fPIC", "-shared", "-fvisibility=hidden", "-o"]).arg(&out).arg(&c_file).args(&libs).arg("-lm").arg("-pthread");
            if cfg!(target_os = "macos") {
                let file = std::path::Path::new(&out).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                cc.arg(format!("-Wl,-install_name,@rpath/{file}"));
            } else {
                cc.arg("-Wl,-z,nodelete");
            }
        }
    }
    let status = cc.status();
    let _ = std::fs::remove_file(&c_file);
    let status = match (made, status) {
        // The static library is the object alone, in an archive made new.
        (Made::Static, Ok(status)) if status.success() => {
            let _ = std::fs::remove_file(&out);
            let archived = std::process::Command::new("ar").arg("rcs").arg(&out).arg(&object).status();
            let _ = std::fs::remove_dir_all(&objects);
            match archived {
                Ok(archived) if archived.success() => Ok(archived),
                Ok(_) => {
                    eprintln!("error: `ar` could not make {out}");
                    return ExitCode::FAILURE;
                }
                Err(error) => {
                    eprintln!("error: cannot run `ar`: {error}");
                    return ExitCode::FAILURE;
                }
            }
        }
        (_, status) => status,
    };
    match status {
        Ok(status) if status.success() => {
            let Some(header) = header else { return ExitCode::SUCCESS };
            let header_path = std::path::Path::new(&out).with_file_name(format!("{name}.h"));
            if let Err(error) = std::fs::write(&header_path, header) {
                eprintln!("error: cannot write {}: {error}", header_path.display());
                return ExitCode::FAILURE;
            }
            let linked = match made {
                Made::Static => libs.iter().cloned().chain(["-lm".to_string(), "-pthread".to_string()]).fold(out.clone(), |line, flag| format!("{line} {flag}")),
                _ => {
                    let dir = std::path::Path::new(&out).parent().map(|dir| dir.display().to_string()).filter(|dir| !dir.is_empty()).unwrap_or_else(|| ".".to_string());
                    format!("-L{dir} -l{name}")
                }
            };
            println!("built {out} and {}; a program links it with `{linked}`", header_path.display());
            ExitCode::SUCCESS
        }
        Ok(_) if !libs.is_empty() => {
            eprintln!("error: the C compiler could not build the program; is every library it links ({}) installed where the C compiler looks?", libs.join(" "));
            ExitCode::FAILURE
        }
        Ok(_) => {
            eprintln!("error: the C compiler did not accept the program; this is a mistake in sg");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("error: cannot run the C compiler `cc`: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The name of a library: that of its folder, or of its file without `.sg`.
fn library_name(path: &str) -> String {
    let path = std::path::Path::new(path);
    let path = if path.is_dir() { std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()) } else { path.to_path_buf() };
    path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_else(|| "library".to_string())
}

/// The test command on compiled code: the tests are built into a program next to the other
/// temporary files, that program is run, and its exit code is given on.
fn test_compiled(path: &str) -> ExitCode {
    let binary = std::env::temp_dir().join(format!("sg-test-{}", std::process::id()));
    let built = build(path, Some(binary.to_string_lossy().into_owned()), false, Made::Tests);
    if built != ExitCode::SUCCESS {
        return built;
    }
    let status = std::process::Command::new(&binary).status();
    let _ = std::fs::remove_file(&binary);
    match status {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: cannot run the compiled tests: {error}");
            ExitCode::FAILURE
        }
    }
}

/// A word of the command line as the system gave it.
fn bytes(word: std::ffi::OsString) -> Vec<u8> {
    #[cfg(unix)]
    {
        std::os::unix::ffi::OsStringExt::into_vec(word)
    }
    #[cfg(not(unix))]
    {
        word.to_string_lossy().into_owned().into_bytes()
    }
}

fn main() -> ExitCode {
    // A word that is not UTF-8 is left as it is for the program, which checks it.
    let words: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let args: Vec<String> = words.iter().map(|word| word.to_string_lossy().into_owned()).collect();
    const USAGE: &str = "usage: sg <file.sg | folder> [args...]                            runs the program; the words after it are its command line\n       sg test <file.sg | folder> [--compiled]                    runs its tests; with a folder, those of every module in it\n       sg build <file.sg | folder> [--release] [--test] [-o out]  compiles it through C; with --test, the program that runs its tests\n       sg build <file.sg | folder> --library|--shared [--release] [-o out]\n                                                                  a library C calls, static (.a) or shared, and its header\n       sg emit-c <file.sg | folder> [--release] [--test | --library]\n                                                                  prints the C";
    let (path, tests) = match args.as_slice() {
        [command, path] if command == "test" => (path, true),
        [command, first, second] if command == "test" && (first == "--compiled" || second == "--compiled") => {
            return test_compiled(if first == "--compiled" { second } else { first });
        }
        [command, rest @ ..] if command == "build" || command == "emit-c" => {
            // A debug build checks integer overflow; `--release` wraps and optimises.
            let (mut path, mut out, mut release, mut made, mut twice) = (None, None, false, Made::Program, false);
            let mut rest = rest.iter();
            while let Some(arg) = rest.next() {
                let kind = match arg.as_str() {
                    "--test" => Some(Made::Tests),
                    "--library" => Some(Made::Static),
                    "--shared" => Some(Made::Shared),
                    _ => None,
                };
                match arg.as_str() {
                    "--release" => release = true,
                    "-o" => out = rest.next().cloned(),
                    _ if kind.is_some() => {
                        twice |= made != Made::Program;
                        made = kind.expect("matched");
                    }
                    _ if path.is_none() => path = Some(arg),
                    _ => path = None,
                }
            }
            let Some(path) = path.filter(|_| !twice) else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            let name = library_name(path);
            let stem = match made {
                Made::Static => Some(format!("lib{name}.a")),
                Made::Shared if cfg!(target_os = "macos") => Some(format!("lib{name}.dylib")),
                Made::Shared => Some(format!("lib{name}.so")),
                _ => std::path::Path::new(path).file_stem().map(|stem| stem.to_string_lossy().into_owned()),
            };
            let out = (command == "build").then(|| out.or(stem).unwrap_or_else(|| "a.out".to_string()));
            return build(path, out, release, made);
        }
        [command, ..] if command == "test" => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
        [path, ..] => (path, false),
        [] => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    // A folder is a program whose root module is every file in it; the test command then runs
    // the tests of every module under it.
    let sources = match sg::load(std::path::Path::new(path), tests) {
        Ok(sources) => sources,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };

    let mut out = std::io::BufWriter::new(std::io::stdout());
    // A failed test is reported like an error of the program: exit code 1.
    let result = match tests {
        false => {
            use std::io::IsTerminal;
            let mut err = std::io::stderr();
            let input_is_terminal = std::io::stdin().is_terminal();
            let input_waits = input_waits();
            let program_args = words.into_iter().skip(1).map(bytes).collect();
            let world = sg::World { args: program_args, input: Box::new(std::io::stdin()), input_is_terminal, input_waits, err: &mut err };
            sg::run_sources_in(&sources, &mut out, world)
        }
        true => sg::test_sources(&sources, &mut out).and_then(|passed| match passed {
            true => Ok(()),
            false => Err(Stop::TestsFailed),
        }),
    };
    // Output written so far goes out before any error message.
    let flushed = out.flush();

    match result {
        Ok(()) => match flushed {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: cannot write output: {error}");
                ExitCode::FAILURE
            }
        },
        Err(Stop::Diag(diag)) => {
            eprint!("{}", diag.render_in(&sources));
            ExitCode::FAILURE
        }
        Err(Stop::Failed(text)) => {
            eprintln!("error: {text}");
            ExitCode::FAILURE
        }
        Err(Stop::TestsFailed) => ExitCode::FAILURE,
        // What was printed is out; an output that was closed meanwhile does not change the code.
        Err(Stop::Exit(code)) => match flushed {
            Err(error) if error.kind() != std::io::ErrorKind::BrokenPipe => {
                eprintln!("error: cannot write output: {error}");
                ExitCode::FAILURE
            }
            _ => ExitCode::from(code),
        },
        Err(Stop::OutputClosed) => ExitCode::SUCCESS,
        Err(Stop::OutputFailed(error)) => {
            eprintln!("error: cannot write output: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Whether the input of this process can make it wait: anything but a file (a pipe, a
/// terminal, a socket). A file is always ready.
fn input_waits() -> bool {
    use std::os::fd::AsFd;
    let Ok(fd) = std::io::stdin().as_fd().try_clone_to_owned() else { return true };
    !std::fs::File::from(fd).metadata().is_ok_and(|meta| meta.is_file())
}
