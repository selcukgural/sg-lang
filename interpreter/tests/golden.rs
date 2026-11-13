//! Runs every `.sg` file and compares the output with the `.out` file next to it.
//! `.out`: what the program printed, followed by the error or panic message if any.
//! To regenerate the expected outputs: `UPDATE_GOLDEN=1 cargo test`

use std::path::{Path, PathBuf};

/// What a program is given besides its source: the words of its command line, one on each
/// line of the `.args` file next to it, and its input, the `.in` file. `{dir}` in a word is a
/// folder of its own that the program can write into; the folder is written back as `{dir}`
/// in what the program prints.
struct Given {
    args: Vec<String>,
    input: Vec<u8>,
    dir: PathBuf,
}

fn given(path: &Path, work: &Path) -> Given {
    let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
    let dir = work.join(&stem);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let args = match std::fs::read_to_string(path.with_extension("args")) {
        Ok(text) => text.lines().map(|line| line.replace("{dir}", &dir.display().to_string())).collect(),
        Err(_) => Vec::new(),
    };
    let input = std::fs::read(path.with_extension("in")).unwrap_or_default();
    Given { args, input, dir }
}

/// The exit code is part of what a program did when it is neither 0 nor 1 (1 is also what a
/// panic and an error of `main` end with).
fn exit_line(code: Option<i32>) -> String {
    match code {
        Some(0 | 1) => String::new(),
        Some(code) => format!("exit code {code}\n"),
        None => "ended by a signal\n".to_string(),
    }
}

fn run_file(path: &Path) -> String {
    let src = std::fs::read_to_string(path).unwrap();
    let name = path.file_name().unwrap().to_string_lossy();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let given = given(path, &root.join("target").join("io").join("interpreted"));
    let mut out = Vec::new();
    let (mut input, mut err) = (std::io::Cursor::new(given.input), Vec::new());
    // A file named `tests_...` is run by the test command.
    let result = match name.starts_with("tests_") {
        true => sg::run_tests(&src, &name, &mut out).map(|_| ()),
        false => {
            let args = given.args.iter().map(|arg| arg.clone().into_bytes()).collect();
            let world = sg::World { args, input: &mut input, input_is_terminal: false, err: &mut err };
            sg::run_sources_in(&sg::Sources::single(&src, ""), &mut out, world)
        }
    };
    let mut text = String::from_utf8(out).unwrap();
    text.push_str(&String::from_utf8(err).unwrap());
    match result {
        Ok(()) => {}
        Err(sg::Stop::Diag(diag)) => text.push_str(&diag.render(&src, &name)),
        Err(sg::Stop::Failed(error)) => text.push_str(&format!("error: {error}\n")),
        Err(sg::Stop::Exit(code)) => text.push_str(&exit_line(Some(code as i32))),
        Err(other) => text.push_str(&format!("{other:?}\n")),
    }
    text.replace(&given.dir.display().to_string(), "{dir}")
}

/// A program of several files: `path` is its main file, or its folder. With `tests` the test
/// command is run on it. The folder the program is in is left out of the paths in the output.
fn run_program(path: &Path, tests: bool, within: &Path) -> String {
    let sources = sg::load(path, tests).unwrap();
    let mut out = Vec::new();
    let result = match tests {
        true => sg::test_sources(&sources, &mut out).map(|_| ()),
        false => sg::run_sources(&sources, &mut out),
    };
    let mut text = String::from_utf8(out).unwrap();
    match result {
        Ok(()) => {}
        Err(sg::Stop::Diag(diag)) => text.push_str(&diag.render_in(&sources)),
        Err(sg::Stop::Failed(error)) => text.push_str(&format!("error: {error}\n")),
        Err(other) => text.push_str(&format!("{other:?}\n")),
    }
    text.replace(&format!("{}/", within.display()), "")
}

/// What a compiled program left behind when it ended: `None` if it freed everything it took,
/// otherwise the report. `plain` is the program as it is built, `sanitized` the same program
/// built with the address sanitizer.
///
/// On macOS the sanitizer does not look for leaks, so the system's `leaks` tool runs the plain
/// program. Elsewhere the sanitizer does, when the program ends. A tool that cannot be run is
/// an error, not a pass.
fn left_behind(plain: &Path, sanitized: &Path, args: &[&str], input: &Path) -> Option<String> {
    let stdin = || std::fs::File::open(input).map_or_else(|_| std::process::Stdio::null(), std::process::Stdio::from);
    if cfg!(target_os = "macos") {
        let _ = sanitized;
        let run = match std::process::Command::new("leaks").arg("--atExit").arg("--").arg(plain).args(args).stdin(stdin()).output() {
            Ok(run) => run,
            Err(error) => return Some(format!("the `leaks` tool could not be run: {error}")),
        };
        let report = String::from_utf8_lossy(&run.stdout).into_owned();
        // "Process 123: 0 leaks for 0 total leaked bytes."
        match report.lines().find(|line| line.contains(" leaks for ") || line.contains(" leak for ")) {
            Some(line) if line.contains(": 0 leaks for ") => None,
            Some(_) => Some(report.lines().filter(|line| line.contains("leak")).take(12).collect::<Vec<_>>().join("\n")),
            None => Some(format!("`leaks` gave no count:\n{report}{}", String::from_utf8_lossy(&run.stderr))),
        }
    } else {
        let run = match std::process::Command::new(sanitized).args(args).stdin(stdin()).env("ASAN_OPTIONS", "detect_leaks=1").output() {
            Ok(run) => run,
            Err(error) => return Some(format!("the sanitized program could not be run: {error}")),
        };
        let report = String::from_utf8_lossy(&run.stderr);
        report.contains("LeakSanitizer").then(|| report.lines().skip_while(|line| !line.contains("LeakSanitizer")).take(12).collect::<Vec<_>>().join("\n"))
    }
}

/// The leak check tried on a C program that leaks on purpose: a check that does not see that
/// leak, or cannot be run, must not let anything pass.
fn leak_check_sees_a_leak(work: &Path) {
    // Each caller builds its own copy, so two tests running at once do not write one file.
    let stem = format!("leaks-on-purpose-{:?}", std::thread::current().id()).replace(['(', ')'], "");
    let (leaky_c, leaky, leaky_san) = (work.join(format!("{stem}.c")), work.join(&stem), work.join(format!("{stem}-san")));
    std::fs::write(&leaky_c, "#include <stdlib.h>\n#include <string.h>\nint main(void) { char* volatile block = malloc(64); memset(block, 1, 64); block = NULL; return 0; }\n").unwrap();
    for (out, flags) in [(&leaky, &["-O1"][..]), (&leaky_san, &["-O1", "-g", "-fsanitize=address"][..])] {
        let cc = std::process::Command::new("cc").arg("-std=c11").args(flags).arg("-o").arg(out).arg(&leaky_c).output().unwrap();
        assert!(cc.status.success(), "the C compiler rejected the program that leaks on purpose:\n{}", String::from_utf8_lossy(&cc.stderr));
    }
    let seen = left_behind(&leaky, &leaky_san, &[], Path::new(""));
    assert!(
        seen.as_deref().is_some_and(|report| (report.contains("leak") || report.contains("Leak")) && !report.contains("could not be run")),
        "the leak check does not see a leak that is there, so it cannot be trusted: {seen:?}"
    );
}

/// Programs with modules. `tests/modules` holds a few modules (folders) and, next to them,
/// programs of one file that use them. A folder with a `main.sg` is a program of its own: it
/// is run as a folder, and so are its tests.
#[test]
fn modules() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    // (what is run, with the test command, the expected output, the folder left out of paths)
    let mut runs: Vec<(PathBuf, bool, PathBuf, PathBuf)> = Vec::new();
    let dir = root.join("tests/modules");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|entry| entry.unwrap().path()).collect();
    entries.push(root.join("examples/shop"));
    entries.sort();
    for path in entries {
        if path.extension().is_some_and(|ext| ext == "sg") {
            runs.push((path.clone(), false, path.with_extension("out"), dir.clone()));
        } else if path.join("main.sg").exists() {
            let within = path.parent().unwrap().to_path_buf();
            runs.push((path.clone(), false, path.join("main.out"), within.clone()));
            runs.push((path.clone(), true, path.join("tests.out"), within));
        }
    }
    assert!(runs.len() > 10);

    let mut failures = Vec::new();
    for (path, tests, golden, within) in &runs {
        let actual = run_program(path, *tests, within);
        if update {
            std::fs::write(golden, &actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(golden).unwrap_or_default();
        if actual != expected {
            failures.push(format!("--- {}\nexpected:\n{expected}\nactual:\n{actual}", golden.strip_prefix(root).unwrap().display()));
        }
    }
    assert!(failures.is_empty(), "{} program(s) differ:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn golden() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in ["examples", "tests/cases"] {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "sg") {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(!files.is_empty());

    let mut failures = Vec::new();
    for path in &files {
        let actual = run_file(path);
        let golden = path.with_extension("out");
        if update {
            std::fs::write(&golden, &actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&golden).unwrap_or_default();
        if actual != expected {
            failures.push(format!(
                "--- {}\nexpected:\n{expected}\nactual:\n{actual}",
                path.strip_prefix(root).unwrap().display()
            ));
        }
    }
    assert!(failures.is_empty(), "{} file(s) differ:\n{}", failures.len(), failures.join("\n"));
}

/// The back end: every program it can compile must print what the interpreter prints, and
/// panic with the same message (a debug build). The programs in `tests/release` are built as
/// release builds and compared with their own `.out` files: there overflow wraps. Programs that use what it does not compile yet are skipped.
///
/// Every compiled program is then run once more to see that it freed everything it took: a
/// value left behind on an early way out is read by nothing and freed by nothing, so neither
/// the output nor the sanitizers show it. Before that the leak check is tried on a C program
/// that leaks on purpose; if it does not see that leak, the test fails instead of passing
/// with a check that checks nothing.
#[test]
fn compiled() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    if std::process::Command::new("cc").arg("--version").output().is_err() {
        eprintln!("no C compiler (`cc`) found: the back end is not tested");
        return;
    }
    let work = root.join("target").join("compiled");
    std::fs::create_dir_all(&work).unwrap();
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in ["examples", "tests/cases", "tests/release"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
        for entry in entries {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name.ends_with(".sg") && !name.starts_with("tests_") {
                files.push(path);
            }
        }
    }
    files.sort();
    // Programs with modules: the programs of one file next to the modules, and the folders
    // that are programs.
    let modules = root.join("tests/modules");
    let mut programs: Vec<PathBuf> = std::fs::read_dir(&modules).unwrap().map(|entry| entry.unwrap().path()).collect();
    programs.push(root.join("examples/shop"));
    programs.sort();
    programs.retain(|path| path.extension().is_some_and(|ext| ext == "sg") || path.join("main.sg").exists());
    let total = files.len() + programs.len();
    files.extend(programs);

    // `SG_NO_SANITIZERS=1` skips the second, slower build of every program.
    let sanitizers = std::env::var_os("SG_NO_SANITIZERS").is_none();
    let mut sanitized = 0;
    let mut compiled = 0;
    let mut failures = Vec::new();
    // The programs to look at for leaks: the name, the program, the program under the sanitizers.
    // The programs to look at for leaks: the name, the program, the program under the
    // sanitizers, its command line and the file of its input.
    let mut built: Vec<(String, PathBuf, PathBuf, Vec<String>, PathBuf)> = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // Deep recursion is caught by the interpreter's own count of calls; compiled code has
        // no such count yet (the stack depth question of the design is open).
        if name == "panic_stack_overflow.sg" {
            continue;
        }
        // A program the checker rejects, or one that uses what is not compiled yet.
        // In `tests/release` the build is a release build: overflow wraps.
        let release = path.parent().is_some_and(|dir| dir.ends_with("release"));
        // A program with modules is read from its folder; the folder it is in is left out of
        // the paths in its panic messages, as in the `modules` test.
        let with_modules = path.is_dir() || path.starts_with(&modules);
        let (code, golden, within) = match with_modules {
            true => {
                let sources = sg::load(path, false).unwrap();
                let golden = if path.is_dir() { path.join("main.out") } else { path.with_extension("out") };
                (sg::emit_c_sources(&sources, true), golden, format!("{}/", path.parent().unwrap().display()))
            }
            false => (sg::emit_c(&std::fs::read_to_string(path).unwrap(), &name, !release), path.with_extension("out"), String::new()),
        };
        let Ok(code) = code else { continue };
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let stem = if with_modules { format!("modules-{stem}") } else { stem };
        let (c_file, binary) = (work.join(format!("{stem}.c")), work.join(&stem));
        std::fs::write(&c_file, code).unwrap();
        let cc = std::process::Command::new("cc").args(["-std=c11", "-O1", "-ffp-contract=off", "-o"]).arg(&binary).arg(&c_file).arg("-lm").output().unwrap();
        if !cc.status.success() {
            failures.push(format!("--- {name}: the C compiler rejected it\n{}", String::from_utf8_lossy(&cc.stderr)));
            continue;
        }
        let given = given(path, &work.join("io"));
        // Each run starts with an empty folder, so a file one run made is not there for the next.
        let run_given = |binary: &Path| {
            let _ = std::fs::remove_dir_all(&given.dir);
            std::fs::create_dir_all(&given.dir).unwrap();
            let mut child = std::process::Command::new(binary)
                .args(&given.args)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let mut stdin = child.stdin.take().unwrap();
            let input = given.input.clone();
            let feeding = std::thread::spawn(move || {
                use std::io::Write;
                let _ = stdin.write_all(&input);
            });
            let run = child.wait_with_output().unwrap();
            feeding.join().unwrap();
            run
        };
        let run = run_given(&binary);
        let shown = |run: &std::process::Output| {
            let text = format!("{}{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr), exit_line(run.status.code()));
            let text = text.replace(&given.dir.display().to_string(), "{dir}");
            if within.is_empty() { text } else { text.replace(&within, "") }
        };
        let actual = shown(&run);
        if release && std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::write(&golden, &actual).unwrap();
        }
        let expected = std::fs::read_to_string(&golden).unwrap_or_default();
        if actual != expected {
            failures.push(format!("--- {name}\nexpected:\n{expected}\nactual:\n{actual}"));
        }
        compiled += 1;

        // The same C again under the address and undefined-behaviour sanitizers: a use after
        // free, a double free or a read outside a buffer would stop the program here.
        if sanitizers {
            let checked_binary = work.join(format!("{stem}-san"));
            let cc = std::process::Command::new("cc")
                .args(["-std=c11", "-O1", "-g", "-ffp-contract=off", "-fsanitize=address,undefined", "-fno-sanitize-recover=all", "-o"])
                .arg(&checked_binary)
                .arg(&c_file)
                .arg("-lm")
                .output()
                .unwrap();
            if cc.status.success() {
                let run = run_given(&checked_binary);
                let actual = shown(&run);
                if actual != expected {
                    failures.push(format!("--- {name}, under the sanitizers\nexpected:\n{expected}\nactual:\n{actual}"));
                }
                sanitized += 1;
                built.push((name.clone(), binary.clone(), checked_binary, given.args.clone(), path.with_extension("in")));
            } else {
                failures.push(format!("--- {name}: the C compiler rejected it under the sanitizers\n{}", String::from_utf8_lossy(&cc.stderr)));
            }
        }
    }

    // Leaks. First the check itself: it has to see a leak that is there.
    let mut leak_free = 0;
    if sanitizers {
        leak_check_sees_a_leak(&work);

        // Then every program, several at a time.
        let next = std::sync::atomic::AtomicUsize::new(0);
        let found = std::sync::Mutex::new(Vec::new());
        let workers = std::thread::available_parallelism().map_or(4, |count| count.get()).min(8);
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((name, plain, checked, args, input)) = built.get(index) else { break };
                    let args: Vec<&str> = args.iter().map(String::as_str).collect();
                    if let Some(report) = left_behind(plain, checked, &args, input) {
                        found.lock().unwrap().push(format!("--- {name}: memory was left behind\n{report}"));
                    }
                });
            }
        });
        let mut found = found.into_inner().unwrap();
        found.sort();
        leak_free = built.len() - found.len();
        failures.extend(found);
    } else {
        eprintln!("SG_NO_SANITIZERS is set: the compiled programs were NOT run under the sanitizers and NOT looked at for leaks");
    }
    assert!(failures.is_empty(), "{} compiled program(s) differ:\n{}", failures.len(), failures.join("\n"));
    assert!(compiled >= 10, "only {compiled} programs were compiled: the back end skips too much");
    assert!(!sanitizers || leak_free == compiled, "only {leak_free} of {compiled} compiled programs were looked at for leaks");
    eprintln!("{compiled} of {total} programs compiled through C and gave the same output; {sanitized} also ran clean under the sanitizers; {leak_free} left no memory behind");
}

/// The test command on compiled code. Every program of tests is built into the program that
/// runs its `test` blocks, each in a process of its own, and what that program writes is
/// compared with what the test command of the interpreter writes: the same `.out` files. It
/// is run again under the sanitizers, and every test that passes is run once more on its own
/// (`--only N`) to see that it left no memory behind; a test that fails stops where it is and
/// frees nothing.
#[test]
fn compiled_tests() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    if std::process::Command::new("cc").arg("--version").output().is_err() {
        eprintln!("no C compiler (`cc`) found: the back end is not tested");
        return;
    }
    let work = root.join("target").join("compiled-tests");
    std::fs::create_dir_all(&work).unwrap();
    // (what is tested, the expected output, the folder left out of the paths)
    let mut programs: Vec<(PathBuf, PathBuf, String)> = Vec::new();
    for dir in ["examples", "tests/cases"] {
        let mut files: Vec<PathBuf> = std::fs::read_dir(root.join(dir)).unwrap().map(|entry| entry.unwrap().path()).collect();
        files.sort();
        for path in files {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name.starts_with("tests_") && name.ends_with(".sg") {
                programs.push((path.clone(), path.with_extension("out"), String::new()));
            }
        }
    }
    let mut folders: Vec<PathBuf> = std::fs::read_dir(root.join("tests/modules")).unwrap().map(|entry| entry.unwrap().path()).collect();
    folders.push(root.join("examples/shop"));
    folders.sort();
    for path in folders {
        if path.join("tests.out").exists() {
            programs.push((path.clone(), path.join("tests.out"), format!("{}/", path.parent().unwrap().display())));
        }
    }

    let sanitizers = std::env::var_os("SG_NO_SANITIZERS").is_none();
    if sanitizers {
        leak_check_sees_a_leak(&work);
    } else {
        eprintln!("SG_NO_SANITIZERS is set: the compiled tests were NOT run under the sanitizers and NOT looked at for leaks");
    }
    let (mut compiled, mut alone) = (0, 0);
    let mut failures = Vec::new();
    for (path, golden, within) in &programs {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let sources = match path.is_dir() {
            true => sg::load(path, true).unwrap(),
            false => sg::Sources::single(&std::fs::read_to_string(path).unwrap(), &name),
        };
        // A program the checker rejects (its `.out` is that error), or one that uses what is
        // not compiled yet.
        let Ok(code) = sg::emit_c_tests(&sources, true) else { continue };
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let (c_file, plain, checked) = (work.join(format!("{stem}.c")), work.join(&stem), work.join(format!("{stem}-san")));
        std::fs::write(&c_file, code).unwrap();
        let mut builds = vec![(&plain, &["-O1", "-g"][..])];
        if sanitizers {
            builds.push((&checked, &["-O1", "-g", "-fsanitize=address,undefined", "-fno-sanitize-recover=all"][..]));
        }
        let mut rejected = false;
        for (binary, flags) in &builds {
            let cc = std::process::Command::new("cc").args(["-std=c11", "-ffp-contract=off"]).args(*flags).arg("-o").arg(binary).arg(&c_file).arg("-lm").output().unwrap();
            if !cc.status.success() {
                failures.push(format!("--- {name}: the C compiler rejected it ({})\n{}", flags.join(" "), String::from_utf8_lossy(&cc.stderr)));
                rejected = true;
            }
        }
        if rejected {
            continue;
        }
        let expected = std::fs::read_to_string(golden).unwrap_or_default();
        for (binary, _) in &builds {
            let run = std::process::Command::new(binary).output().unwrap();
            let actual = format!("{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
            let actual = if within.is_empty() { actual } else { actual.replace(within.as_str(), "") };
            if actual != expected {
                let which = if *binary == &checked { ", under the sanitizers" } else { "" };
                failures.push(format!("--- {name}{which}\nexpected:\n{expected}\nactual:\n{actual}"));
            }
            // The exit code says whether every test passed, as the last line does.
            if run.status.success() != expected.trim_end().ends_with("passed") {
                failures.push(format!("--- {name}: the exit code is {:?}, and the report ends with: {}", run.status.code(), expected.lines().last().unwrap_or_default()));
            }
        }
        compiled += 1;

        // Each test on its own: the ones that pass must have freed everything.
        if sanitizers {
            for number in 0.. {
                let only = number.to_string();
                let run = std::process::Command::new(&plain).args(["--only", &only]).output().unwrap();
                match run.status.code() {
                    // There is no test of this number: all of them were looked at.
                    Some(3) => break,
                    Some(0) => {
                        if let Some(report) = left_behind(&plain, &checked, &["--only", &only], Path::new("")) {
                            failures.push(format!("--- {name}, test {number}: memory was left behind\n{report}"));
                        }
                        alone += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} compiled test program(s) differ:\n{}", failures.len(), failures.join("\n"));
    assert!(compiled >= 4, "only {compiled} programs of tests were compiled: the back end skips too much");
    assert!(!sanitizers || alone >= 8, "only {alone} tests were looked at for leaks");
    eprintln!("{compiled} of {} programs of tests compiled through C and reported the same; {alone} passing tests left no memory behind", programs.len());
}
