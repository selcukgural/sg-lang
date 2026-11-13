//! Runs every `.sg` file and compares the output with the `.out` file next to it.
//! `.out`: what the program printed, followed by the error or panic message if any.
//! To regenerate the expected outputs: `UPDATE_GOLDEN=1 cargo test`

use std::path::{Path, PathBuf};

fn run_file(path: &Path) -> String {
    let src = std::fs::read_to_string(path).unwrap();
    let name = path.file_name().unwrap().to_string_lossy();
    let mut out = Vec::new();
    // A file named `tests_...` is run by the test command.
    let result = match name.starts_with("tests_") {
        true => sg::run_tests(&src, &name, &mut out).map(|_| ()),
        false => sg::run(&src, &mut out),
    };
    let mut text = String::from_utf8(out).unwrap();
    match result {
        Ok(()) => {}
        Err(sg::Stop::Diag(diag)) => text.push_str(&diag.render(&src, &name)),
        Err(sg::Stop::Failed(error)) => text.push_str(&format!("error: {error}\n")),
        Err(other) => text.push_str(&format!("{other:?}\n")),
    }
    text
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
        let cc = std::process::Command::new("cc").args(["-std=c11", "-O1", "-o"]).arg(&binary).arg(&c_file).output().unwrap();
        if !cc.status.success() {
            failures.push(format!("--- {name}: the C compiler rejected it\n{}", String::from_utf8_lossy(&cc.stderr)));
            continue;
        }
        let run = std::process::Command::new(&binary).output().unwrap();
        let shown = |run: &std::process::Output| {
            let text = format!("{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
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
                .args(["-std=c11", "-O1", "-g", "-fsanitize=address,undefined", "-fno-sanitize-recover=all", "-o"])
                .arg(&checked_binary)
                .arg(&c_file)
                .output()
                .unwrap();
            if cc.status.success() {
                let run = std::process::Command::new(&checked_binary).output().unwrap();
                let actual = shown(&run);
                if actual != expected {
                    failures.push(format!("--- {name}, under the sanitizers\nexpected:\n{expected}\nactual:\n{actual}"));
                }
                sanitized += 1;
            }
        }
    }
    assert!(failures.is_empty(), "{} compiled program(s) differ:\n{}", failures.len(), failures.join("\n"));
    assert!(compiled >= 10, "only {compiled} programs were compiled: the back end skips too much");
    eprintln!("{compiled} of {} programs compiled through C and gave the same output; {sanitized} also ran clean under the sanitizers", total);
}
