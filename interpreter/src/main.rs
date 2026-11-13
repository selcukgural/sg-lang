use std::io::Write;
use std::process::ExitCode;

use sg::Stop;

/// Writes the program as C. Without `out` the C goes to the output; with it, the C compiler
/// turns it into a program of that name. `release`: integer overflow wraps and the C compiler
/// optimises; otherwise a debug build, which checks.
fn build(path: &str, out: Option<String>, release: bool) -> ExitCode {
    let sources = match sg::load(std::path::Path::new(path), false) {
        Ok(sources) => sources,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    let code = match sg::emit_c_sources(&sources, !release) {
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
    if let Err(error) = std::fs::write(&c_file, code) {
        eprintln!("error: cannot write {}: {error}", c_file.display());
        return ExitCode::FAILURE;
    }
    let level = if release { "-O2" } else { "-O0" };
    let status = std::process::Command::new("cc").args(["-std=c11", level, "-o", &out]).arg(&c_file).status();
    let _ = std::fs::remove_file(&c_file);
    match status {
        Ok(status) if status.success() => ExitCode::SUCCESS,
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    const USAGE: &str = "usage: sg <file.sg | folder>                             runs the program\n       sg test <file.sg | folder>                        runs its tests; with a folder, those of every module in it\n       sg build <file.sg | folder> [--release] [-o out]  compiles it through C\n       sg emit-c <file.sg | folder> [--release]          prints the C";
    let (path, tests) = match args.as_slice() {
        [path] => (path, false),
        [command, path] if command == "test" => (path, true),
        [command, rest @ ..] if command == "build" || command == "emit-c" => {
            // A debug build checks integer overflow; `--release` wraps and optimises.
            let (mut path, mut out, mut release) = (None, None, false);
            let mut rest = rest.iter();
            while let Some(arg) = rest.next() {
                match arg.as_str() {
                    "--release" => release = true,
                    "-o" => out = rest.next().cloned(),
                    _ if path.is_none() => path = Some(arg),
                    _ => path = None,
                }
            }
            let Some(path) = path else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            let stem = std::path::Path::new(path).file_stem().map(|stem| stem.to_string_lossy().into_owned());
            let out = (command == "build").then(|| out.or(stem).unwrap_or_else(|| "a.out".to_string()));
            return build(path, out, release);
        }
        _ => {
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
        false => sg::run_sources(&sources, &mut out),
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
        Err(Stop::OutputClosed) => ExitCode::SUCCESS,
        Err(Stop::OutputFailed(error)) => {
            eprintln!("error: cannot write output: {error}");
            ExitCode::FAILURE
        }
    }
}
