//! Runs every `.sg` file and compares the output with the `.out` file next to it.
//! `.out`: what the program printed, followed by the error or panic message if any.
//! To regenerate the expected outputs: `UPDATE_GOLDEN=1 cargo test`

use std::path::{Path, PathBuf};


/// `PATH` as the tests of other programs see it: the folder of `sg-test-tool` (`tests/tool`) first,
/// the program they start in place of `sh`, `cat` and their kin, which Windows does not have. It
/// is built once, with `sg build`; a test that needs it fails when it cannot be built.
fn tool_path() -> std::ffi::OsString {
    static PATH: std::sync::OnceLock<std::ffi::OsString> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let folder = root.join("target").join("tool");
        std::fs::create_dir_all(&folder).unwrap();
        let built = std::process::Command::new(env!("CARGO_BIN_EXE_sg"))
            .arg("build")
            .arg(root.join("tests").join("tool").join("main.sg"))
            .arg("-o")
            .arg(folder.join("sg-test-tool"))
            .output()
            .unwrap();
        assert!(built.status.success(), "sg-test-tool, which the tests of other programs start, could not be built:\n{}{}", String::from_utf8_lossy(&built.stdout), String::from_utf8_lossy(&built.stderr));
        let mut folders = vec![folder];
        folders.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        std::env::join_paths(folders).unwrap()
    })
    .clone()
}

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

/// The output a program is held to: `name.out`, or on Windows `name.windows.out` where the output
/// is not the same there (a path written with `\`).
fn golden_of(out: &Path) -> PathBuf {
    if cfg!(windows) {
        let stem = out.file_stem().unwrap().to_string_lossy();
        let windows = out.with_file_name(format!("{stem}.windows.out"));
        if windows.exists() {
            return windows;
        }
    }
    out.to_path_buf()
}

/// Where `UPDATE_GOLDEN=1` writes the output of a program: on Windows into `name.windows.out`
/// when it is not what `name.out` holds (and that file goes when it is).
fn write_golden(out: &Path, actual: &str) {
    if cfg!(windows) {
        let stem = out.file_stem().unwrap().to_string_lossy();
        let windows = out.with_file_name(format!("{stem}.windows.out"));
        match std::fs::read_to_string(out).is_ok_and(|common| common == actual) {
            true => {
                let _ = std::fs::remove_file(&windows);
            }
            false => std::fs::write(&windows, actual).unwrap(),
        }
        return;
    }
    std::fs::write(out, actual).unwrap();
}

/// A program the tests start gets the default action of `Interrupt` and `Quit`, whatever this
/// process was given: a shell that starts `cargo test` in the background (`&`, without job
/// control) has both ignored, an ignored signal stays ignored in every program started after it,
/// and a test that sends one (`ok_signals_process`) would then depend on how the tests were
/// started.
fn default_signals(command: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        extern "C" {
            fn signal(number: i32, handler: usize) -> usize;
        }
        // SAFETY: between `fork` and `exec` the closure only calls `signal`, which may be called
        // there, and allocates nothing. 0 is SIG_DFL.
        unsafe {
            command.pre_exec(|| {
                signal(2, 0);
                signal(3, 0);
                Ok(())
            });
        }
    }
    command
}

/// The exit code is part of what a program did when it is neither 0 nor 1 (1 is also what a
/// panic and an error of `main` end with).
/// The warnings of the C compiler that fail the build of a compiled program: every one of
/// `-Wall -Wextra`, so that C the back end writes badly is seen here and not by a user. One is
/// left on as a warning only: a function nobody calls (`-Wunused-function`). The runtime is a
/// library every program carries, the language's own module comes whole, and SG lets a program
/// have a function it does not call; the C compiler leaves out what is not called.
///
/// gcc's analyses that follow values through the optimiser also see paths that cannot happen.
/// Each one found is listed in `GCC_SEES_WHAT_IS_NOT_THERE` with the program it is in, and is a
/// warning only there: anywhere else it fails the test, as does any other warning there.
fn warnings(stem: &str) -> Vec<String> {
    let mut flags: Vec<String> = ["-Wall", "-Wextra", "-Werror", "-Wno-unused-function"].iter().map(|flag| flag.to_string()).collect();
    if c_compiler_is_gcc() {
        for (program, warning) in GCC_SEES_WHAT_IS_NOT_THERE {
            if stem == program {
                flags.push(format!("-Wno-error={warning}"));
            }
        }
    }
    flags
}

/// gcc's warnings that are not true of the program they are listed with, and why.
const GCC_SEES_WHAT_IS_NOT_THERE: [(&str, &str); 4] = [
    // `free` on the bytes of a constant text (`"y"`): a text is freed only when its capacity is
    // above 0, and a constant's is 0 (`SG_LIT`); gcc warns on the `free` behind the test.
    ("ok_disposable", "free-nonheap-object"),
    // A borrow of a `shared checked` value puts its guard, on the stack, on the value's list,
    // and takes it off again (`sg_unlend`) before the block ends; gcc does not follow it off.
    ("ok_folders", "dangling-pointer"),
    // A read of more than 64 KB straight into the buffer, never taken for the 3-byte array
    // the program reads into: the length of the read is the buffer's.
    ("ok_text_blocks", "stringop-overflow"),
    // The same read, in the server of the tests of `examples/curl`, into `buffer[0..length]` of a
    // 256-byte array: a longer window is a panic before the read, which gcc does not follow.
    ("curl", "stringop-overflow"),
];

/// Whether `cc` is gcc (clang says so in its version).
fn c_compiler_is_gcc() -> bool {
    static IS_GCC: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *IS_GCC.get_or_init(|| {
        if cfg!(windows) {
            return false;
        }
        let version = std::process::Command::new("cc").arg("--version").output().map(|out| String::from_utf8_lossy(&out.stdout).into_owned()).unwrap_or_default();
        !version.contains("clang")
    })
}

/// The C compiler of the tests: the one `sg build` uses (`cc` on Linux and macOS, Clang on
/// Windows), with what it is given on this system before the C file.
fn c_command() -> std::process::Command {
    static COMPILER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let compiler = COMPILER.get_or_init(|| sg::cc::compiler().unwrap_or_else(|error| panic!("{error}")));
    let mut command = std::process::Command::new(compiler);
    command.args(sg::cc::compile_flags(false));
    command
}

/// The folders where the libraries of the examples lie on this machine, from `SG_TEST_LIB_DIRS`,
/// as `PATH` lists folders: on the CI of Windows the `lib` and `bin` of vcpkg. Linux and macOS
/// find them where the C compiler looks.
fn test_lib_dirs() -> Vec<PathBuf> {
    std::env::var_os("SG_TEST_LIB_DIRS").map(|dirs| std::env::split_paths(&dirs).filter(|dir| !dir.as_os_str().is_empty()).collect()).unwrap_or_default()
}

/// What links the libraries of a program: the folders of `test_lib_dirs`, then the `-l` of its
/// `extern` blocks. On Windows, which has no `rpath`, their DLLs are copied beside `binary`, as
/// `sg build` copies them.
fn libraries_of(code: &str, binary: &Path) -> Vec<std::ffi::OsString> {
    let dirs = test_lib_dirs();
    let libs = sg::emit_c::link_flags(code);
    if cfg!(windows) {
        for line in sg::link::copy_dlls(&libs, &dirs, binary) {
            assert!(!line.starts_with("warning"), "{line}");
        }
    }
    let mut flags = sg::link::folder_flags(&dirs, true);
    flags.extend(libs.into_iter().map(Into::into));
    flags
}

/// What the C compiler is given after the C file to link a program on this system.
fn system_libs(work: &Path) -> Vec<String> {
    sg::cc::link_flags(work).unwrap()
}

/// A program built into `work`: on Windows with `.exe`.
fn program_at(work: &Path, stem: &str) -> PathBuf {
    PathBuf::from(sg::cc::program_file(&work.join(stem).to_string_lossy()))
}

/// The address and the undefined-behaviour sanitizers as this machine has them: both, with
/// their runtime, on Linux and macOS. On Windows each is tried once on a program of a few lines;
/// what it lacks is said in the output, not passed over: the undefined-behaviour check without
/// its runtime stops the program at the fault (`-fsanitize-trap`).
fn sanitizer_flags(work: &Path) -> Vec<String> {
    static FLAGS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    FLAGS
        .get_or_init(|| {
            let full = ["-fsanitize=address,undefined", "-fno-sanitize-recover=all"].map(String::from).to_vec();
            if !cfg!(windows) {
                return full;
            }
            let probe = work.join("sanitizer-probe.c");
            std::fs::write(&probe, "#include <stdio.h>\nint main(void) { int x[2] = { 1, 2 }; printf(\"%d\\n\", x[1]); return 0; }\n").unwrap();
            let address_trap = ["-fsanitize=address", "-fsanitize=undefined", "-fsanitize-trap=undefined"].map(String::from).to_vec();
            let trap = ["-fsanitize=undefined", "-fsanitize-trap=undefined"].map(String::from).to_vec();
            for (flags, said) in [(full, None), (address_trap, Some("undefined behaviour: checked without its runtime (a fault stops the program)")), (trap, Some("address sanitizer: not on this machine; undefined behaviour: checked without its runtime"))] {
                let out = program_at(work, "sanitizer-probe");
                let built = c_command().args(["-std=c11", "-O1", "-g"]).args(&flags).arg("-o").arg(&out).arg(&probe).output();
                let ran = built.is_ok_and(|built| built.status.success()) && std::process::Command::new(&out).output().is_ok_and(|run| run.status.success());
                if ran {
                    if let Some(said) = said {
                        eprintln!("{said}");
                    }
                    return flags;
                }
            }
            panic!("the C compiler builds no program under the undefined-behaviour sanitizer on this machine");
        })
        .clone()
}

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
    // A signal reaches the whole process, and one no listener is left for ends it: a program
    // that sends signals runs in a process of its own, as compiled code does. So does one that
    // starts other programs, which finds `sg-test-tool` on the `PATH` of its own process.
    if name.contains("signals") || name.contains("process") {
        let mut child = default_signals(&mut std::process::Command::new(env!("CARGO_BIN_EXE_sg")))
            .arg(&*name)
            .args(&given.args)
            .current_dir(path.parent().unwrap())
            .env("PATH", tool_path())
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
        let text = format!("{}{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr), exit_line(run.status.code()));
        return with_slashes(&text.replace(&given.dir.display().to_string(), "{dir}"));
    }
    let mut out = Vec::new();
    let mut err = Vec::new();
    // A file named `tests_...` is run by the test command.
    let result = match name.starts_with("tests_") {
        true => sg::run_tests(&src, &name, &mut out).map(|_| ()),
        false => {
            let args = given.args.iter().map(|arg| arg.clone().into_bytes()).collect();
            let world = sg::World { args, input: Box::new(std::io::Cursor::new(given.input)), input_is_terminal: false, input_waits: false, err: &mut err };
            let mut sources = sg::Sources::single(&src, "");
            // A program that tells which folders were looked in is not given those of this
            // machine.
            if !name.starts_with("error_") {
                sources.lib_dirs = test_lib_dirs();
            }
            sg::run_sources_in(&sources, &mut out, world)
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
    let mut sources = sg::load(path, tests).unwrap();
    sources.lib_dirs = test_lib_dirs();
    let mut out = Vec::new();
    let mut err = Vec::new();
    // What a program writes to its standard error, and the exit code it gives, are kept as
    // for a program of one file, so that compiled code is held to the same.
    let result = match tests {
        true => sg::test_sources(&sources, &mut out).map(|_| ()),
        false => {
            let world = sg::World { args: Vec::new(), input: Box::new(std::io::empty()), input_is_terminal: false, input_waits: false, err: &mut err };
            sg::run_sources_in(&sources, &mut out, world)
        }
    };
    let mut text = String::from_utf8(out).unwrap();
    text.push_str(&String::from_utf8(err).unwrap());
    match result {
        Ok(()) => {}
        Err(sg::Stop::Diag(diag)) => text.push_str(&diag.render_in(&sources)),
        Err(sg::Stop::Failed(error)) => text.push_str(&format!("error: {error}\n")),
        Err(sg::Stop::Exit(code)) => text.push_str(&exit_line(Some(code as i32))),
        Err(other) => text.push_str(&format!("{other:?}\n")),
    }
    let text = with_slashes(&text);
    text.replace(&format!("{}/", within.display()), "").replace(&format!("{}/", within.display()).replace('\\', "/"), "")
}

/// On Windows the place of an error is written with `\` (`--> stock\items.sg:3:5`); the
/// output of the program is compared with these written as on Linux and macOS.
fn with_slashes(text: &str) -> String {
    if !cfg!(windows) {
        return text.to_string();
    }
    let lines = text.split_inclusive('\n').map(|line| match line.trim_start().starts_with("--> ") {
        true => line.replace('\\', "/"),
        false => line.to_string(),
    });
    lines.collect()
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
        let run = match std::process::Command::new("leaks").arg("--atExit").arg("--").arg(plain).args(args).stdin(stdin()).env("PATH", tool_path()).output() {
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
        let run = match std::process::Command::new(sanitized).args(args).stdin(stdin()).env("ASAN_OPTIONS", "detect_leaks=1").env("PATH", tool_path()).output() {
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

/// The thread sanitizer tried on a C program that races on purpose: a check that does not see
/// that race, or cannot be built, must not let the programs on several threads pass.
fn race_check_sees_a_race(work: &Path) {
    let stem = format!("races-on-purpose-{:?}", std::thread::current().id()).replace(['(', ')'], "");
    let (racy_c, racy) = (work.join(format!("{stem}.c")), work.join(&stem));
    std::fs::write(
        &racy_c,
        "#include <pthread.h>\n#include <stdio.h>\nstatic long count = 0;\nstatic void* add(void* p) { (void)p; for (int i = 0; i < 1000; i++) count++; return NULL; }\n\
         int main(void) { pthread_t a, b; pthread_create(&a, NULL, add, NULL); pthread_create(&b, NULL, add, NULL); pthread_join(a, NULL); pthread_join(b, NULL); printf(\"%ld\\n\", count); return 0; }\n",
    )
    .unwrap();
    let cc = std::process::Command::new("cc").args(["-std=c11", "-O1", "-g", "-fsanitize=thread", "-o"]).arg(&racy).arg(&racy_c).arg("-pthread").output().unwrap();
    assert!(cc.status.success(), "the C compiler cannot build a program under the thread sanitizer:\n{}", String::from_utf8_lossy(&cc.stderr));
    let run = std::process::Command::new(&racy).output().unwrap();
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("ThreadSanitizer: data race"),
        "the thread sanitizer does not see a race that is there, so it cannot be trusted:\n{}",
        String::from_utf8_lossy(&run.stderr)
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
    entries.push(root.join("examples/zlib"));
    entries.push(root.join("examples/sqlite"));
    entries.push(root.join("examples/curl"));
    entries.push(root.join("examples/headers"));
    entries.push(root.join("examples/shutdown"));
    entries.push(root.join("examples/queue"));
    entries.push(root.join("examples/calclib"));
    // The module of `examples/bare` that needs no machine: its tests.
    entries.push(root.join("examples/bare/log"));
    entries.sort();
    for path in entries {
        if path.extension().is_some_and(|ext| ext == "sg") {
            runs.push((path.clone(), false, path.with_extension("out"), dir.clone()));
        } else if path.join("main.sg").exists() {
            let within = path.parent().unwrap().to_path_buf();
            runs.push((path.clone(), false, path.join("main.out"), within.clone()));
            runs.push((path.clone(), true, path.join("tests.out"), within));
        } else if path.join("tests.out").exists() {
            // A library has no `main`, only its tests.
            runs.push((path.clone(), true, path.join("tests.out"), path.parent().unwrap().to_path_buf()));
        }
    }
    assert!(runs.len() > 10);

    let mut failures = Vec::new();
    for (path, tests, golden, within) in &runs {
        let actual = run_program(path, *tests, within);
        if update {
            write_golden(golden, &actual);
            continue;
        }
        let expected = std::fs::read_to_string(golden_of(golden)).unwrap_or_default();
        if actual != expected {
            failures.push(format!("--- {}\nexpected:\n{expected}\nactual:\n{actual}", golden.strip_prefix(root).unwrap().display()));
        }
    }
    assert!(failures.is_empty(), "{} program(s) differ:\n{}", failures.len(), failures.join("\n"));
}

/// What a program asks of the C compiler's own library when it is not optimised, as `sg build`
/// compiles a debug build: nothing of 128 bits (`__udivti3` and its kin), which Windows does not
/// link (the C compiler's library is not given to the linker there). Each program of one file is
/// compiled into an object, its undefined names read; on every system, so that what would break
/// Windows is seen where the code is written.
#[test]
fn needs_nothing_of_128_bits() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target").join("unoptimised");
    std::fs::create_dir_all(&work).unwrap();
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in ["examples", "tests/cases", "tests/release", "bench"] {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "sg") {
                files.push(path);
            }
        }
    }
    files.sort();
    let nm = match cfg!(windows) {
        true => sg::cc::compiler().ok().and_then(|cc| Path::new(&cc).parent().map(|dir| dir.join("llvm-nm.exe"))).filter(|path| path.exists()).map_or_else(|| "llvm-nm".into(), PathBuf::into_os_string),
        false => std::ffi::OsString::from("nm"),
    };
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let failures = std::sync::Mutex::new(Vec::new());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut built = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let at = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(path) = files.get(at) else { break };
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                let release = path.parent().is_some_and(|dir| dir.ends_with("release"));
                let Ok(code) = sg::emit_c(&std::fs::read_to_string(path).unwrap(), &name, !release) else { continue };
                let stem = format!("{}-{}", path.parent().unwrap().file_name().unwrap().to_string_lossy(), path.file_stem().unwrap().to_string_lossy());
                let (c_file, object) = (work.join(format!("{stem}.c")), work.join(format!("{stem}.o")));
                std::fs::write(&c_file, code).unwrap();
                let made = c_command().args(["-std=c11", "-O0", "-w", "-c", "-o"]).arg(&object).arg(&c_file).output().unwrap();
                if !made.status.success() {
                    failures.lock().unwrap().push(format!("{name}: {}", String::from_utf8_lossy(&made.stderr)));
                    continue;
                }
                built.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let listed = std::process::Command::new(&nm).arg("-u").arg(&object).output().unwrap();
                assert!(listed.status.success(), "nm: {}", String::from_utf8_lossy(&listed.stderr));
                for line in String::from_utf8_lossy(&listed.stdout).lines() {
                    let wanted = line.split_whitespace().last().unwrap_or_default();
                    if ["ti3", "ti4", "mulodi4"].iter().any(|end| wanted.ends_with(end)) {
                        failures.lock().unwrap().push(format!("{name} asks the C compiler's library for `{wanted}`"));
                    }
                }
            });
        }
    });
    assert!(*built.get_mut() > 250, "only {} programs were compiled", built.get_mut());
    let failures = failures.into_inner().unwrap();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
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
            write_golden(&golden, &actual);
            continue;
        }
        let expected = std::fs::read_to_string(golden_of(&golden)).unwrap_or_default();
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
    if let Err(error) = sg::cc::compiler() {
        panic!("{error}: the back end cannot be tested");
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
    programs.push(root.join("examples/zlib"));
    programs.push(root.join("examples/sqlite"));
    programs.push(root.join("examples/curl"));
    programs.push(root.join("examples/headers"));
    programs.push(root.join("examples/shutdown"));
    programs.push(root.join("examples/queue"));
    programs.sort();
    programs.retain(|path| path.extension().is_some_and(|ext| ext == "sg") || path.join("main.sg").exists());
    let total = files.len() + programs.len();
    files.extend(programs);

    // `SG_NO_SANITIZERS=1` skips the second, slower build of every program.
    let sanitizers = std::env::var_os("SG_NO_SANITIZERS").is_none();
    let mut sanitized = 0;
    let mut raced = 0;
    // Programs on several threads that ran without the thread sanitizer, which Windows has not.
    let mut unraced = 0;
    let mut compiled = 0;
    let mut ended_in_panic = 0;
    let mut ended_by_signal = 0;
    let mut leak_free_counted = 0;
    let mut failures = Vec::new();
    // The programs to look at for leaks: the name, the program, the program under the
    // sanitizers, its command line and the file of its input.
    let mut built: Vec<(String, PathBuf, PathBuf, Vec<String>, PathBuf)> = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // Deep recursion is caught by the interpreter's own count of calls; compiled code has
        // no such count yet (the stack depth question of the design is open).
        // So is a value nested too deep for the interpreter's stack, and an array too big for
        // the memory: compiled code makes it on the stack, which ends at its guard page.
        if matches!(name.as_str(), "panic_stack_overflow.sg" | "panic_deep_value_written.sg" | "panic_array_no_memory.sg") {
            continue;
        }
        // Tasks that wait on channels for each other: the interpreter stops with a panic, and
        // compiled code waits for ever until the count of waiting tasks comes with the hardening.
        if name == "panic_channel_deadlock.sg" {
            continue;
        }
        // The interpreter checks every read and write of raw memory against the blocks of
        // `Memory.allocate`; compiled code checks nothing there (it is written inside `unsafe`),
        // and leaves it to the sanitizers.
        if name.starts_with("panic_unsafe_") {
            continue;
        }
        // A library that is not there: the interpreter says so before the program runs, and the
        // C compiler would refuse to link it.
        if name == "error_extern_no_library.sg" {
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
        let (c_file, binary) = (work.join(format!("{stem}.c")), program_at(&work, &stem));
        // A program with tasks or `locked` values runs on several threads, and so does one that
        // listens for signals (their relay is a thread).
        let threads = code.contains("#define SG_THREADS 1") || code.contains("#define SG_SIGNALS_ASYNC");
        std::fs::write(&c_file, &code).unwrap();
        let cc = c_command().args(["-std=c11", "-O1", "-ffp-contract=off"]).args(warnings(&stem)).arg("-o").arg(&binary).arg(&c_file).args(libraries_of(&code, &binary)).args(system_libs(&work)).output().unwrap();
        if !cc.status.success() {
            failures.push(format!("--- {name}: the C compiler rejected it\n{}", String::from_utf8_lossy(&cc.stderr)));
            continue;
        }
        let given = given(path, &work.join("io"));
        // Each run starts with an empty folder, so a file one run made is not there for the next.
        let run_given = |binary: &Path| {
            let _ = std::fs::remove_dir_all(&given.dir);
            std::fs::create_dir_all(&given.dir).unwrap();
            // Leaks are looked at on their own below (Linux's address sanitizer would look at
            // the end of every run and add its report to the output).
            let mut child = default_signals(&mut std::process::Command::new(binary))
                .args(&given.args)
                .env("ASAN_OPTIONS", "detect_leaks=0")
                .env("PATH", tool_path())
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
            let text = with_slashes(&text);
            if within.is_empty() { text } else { text.replace(&within, "").replace(&within.replace('\\', "/"), "") }
        };
        let actual = shown(&run);
        if release && std::env::var_os("UPDATE_GOLDEN").is_some() {
            write_golden(&golden, &actual);
        }
        let expected = std::fs::read_to_string(golden_of(&golden)).unwrap_or_default();
        // Tasks that print in compiled code do so in an order of their own: where the program
        // says so in its `.unordered` file, the lines are compared in any order.
        let unordered = path.with_extension("unordered").exists();
        let same = |actual: &str| match unordered {
            true => {
                let lines = |text: &str| {
                    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
                    lines.sort();
                    lines
                };
                lines(actual) == lines(&expected)
            }
            false => actual == expected,
        };
        if !same(&actual) {
            failures.push(format!("--- {name}\nexpected:\n{expected}\nactual:\n{actual}"));
        }
        compiled += 1;

        // The same C again under the address and undefined-behaviour sanitizers: a use after
        // free, a double free or a read outside a buffer would stop the program here.
        if sanitizers {
            let checked_binary = program_at(&work, &format!("{stem}-san"));
            let cc = c_command()
                .args(["-std=c11", "-O1", "-g", "-ffp-contract=off"])
                .args(sanitizer_flags(&work))
                .arg("-o")
                .arg(&checked_binary)
                .arg(&c_file)
                .args(libraries_of(&code, &checked_binary))
                .args(system_libs(&work))
                .output()
                .unwrap();
            if cc.status.success() {
                let run = run_given(&checked_binary);
                let actual = shown(&run);
                if !same(&actual) {
                    failures.push(format!("--- {name}, under the sanitizers\nexpected:\n{expected}\nactual:\n{actual}"));
                }
                sanitized += 1;
                // A panic does not unwind (the row "Copy and cleanup"): what the program owned
                // is given back by the system, not freed, so it is not looked at for leaks.
                // Whether a pointer to it is still in a register when the process ends is up to
                // the C compiler, and a leak tool would judge that.
                // One that a signal ended did not end at all, as far as its memory goes.
                if String::from_utf8_lossy(&run.stderr).lines().any(|line| line.starts_with("panic: ")) {
                    ended_in_panic += 1;
                } else if run.status.code().is_none() {
                    ended_by_signal += 1;
                } else if cfg!(windows) {
                    // No LeakSanitizer on Windows: the count of blocks of the debug build looked
                    // (`leak:` in the output, compared above).
                    leak_free_counted += 1;
                } else {
                    built.push((name.clone(), binary.clone(), checked_binary, given.args.clone(), path.with_extension("in")));
                }
            } else {
                failures.push(format!("--- {name}: the C compiler rejected it under the sanitizers\n{}", String::from_utf8_lossy(&cc.stderr)));
            }
        }

        // A program on several threads once more under the thread sanitizer: two threads that
        // reach the same memory without a lock between them would stop it here. Windows has no
        // thread sanitizer: what was not checked there is counted, and said below.
        if sanitizers && threads && cfg!(windows) {
            unraced += 1;
        } else if sanitizers && threads {
            if raced == 0 {
                race_check_sees_a_race(&work);
            }
            let raced_binary = program_at(&work, &format!("{stem}-tsan"));
            let cc = c_command()
                .args(["-std=c11", "-O1", "-g", "-ffp-contract=off", "-fsanitize=thread", "-o"])
                .arg(&raced_binary)
                .arg(&c_file)
                .args(libraries_of(&code, &raced_binary))
                .arg("-lm")
                .arg("-pthread")
                .output()
                .unwrap();
            if cc.status.success() {
                let run = run_given(&raced_binary);
                let actual = shown(&run);
                if !same(&actual) {
                    failures.push(format!("--- {name}, under the thread sanitizer\nexpected:\n{expected}\nactual:\n{actual}"));
                }
                raced += 1;
            } else {
                failures.push(format!("--- {name}: the C compiler rejected it under the thread sanitizer\n{}", String::from_utf8_lossy(&cc.stderr)));
            }
        }
    }

    // Leaks. First the check itself: it has to see a leak that is there.
    let mut leak_free = leak_free_counted;
    if sanitizers && cfg!(windows) {
        eprintln!("leaks: looked at by the count of blocks of each debug build (LeakSanitizer is not on Windows)");
        eprintln!("thread sanitizer: not on Windows; {unraced} program(s) on several threads ran without it");
    }
    if sanitizers && !cfg!(windows) {
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
    } else if !sanitizers {
        eprintln!("SG_NO_SANITIZERS is set: the compiled programs were NOT run under the sanitizers and NOT looked at for leaks");
    }
    assert!(failures.is_empty(), "{} compiled program(s) differ:\n{}", failures.len(), failures.join("\n"));
    assert!(compiled >= 10, "only {compiled} programs were compiled: the back end skips too much");
    assert!(
        !sanitizers || leak_free + ended_in_panic + ended_by_signal == compiled,
        "only {leak_free} of {compiled} compiled programs were looked at for leaks ({ended_in_panic} ended in a panic, {ended_by_signal} by a signal)"
    );
    assert!(!sanitizers || raced > 0 || cfg!(windows), "no program on several threads ran under the thread sanitizer");
    eprintln!(
        "{compiled} of {total} programs compiled through C and gave the same output; {sanitized} also ran clean under the sanitizers, {raced} on several threads under the thread sanitizer; {leak_free} of those that did not end in a panic or by a signal left no memory behind ({ended_in_panic} ended in a panic, {ended_by_signal} by a signal)"
    );
}

/// Compiled code reads the input the async way on a thread of input and output, so a task that
/// waits for a line holds no thread of the pool (the interpreter holds every task meanwhile, so
/// this is a test of compiled code alone). Sixty-four tasks wait for a line each, more than the
/// pool has threads, and a task that only waits for a delay still runs: its line on the standard
/// error is looked for before any input is given.
#[test]
fn compiled_input_waits_alone() {
    const PROGRAM: &str = r#"
async Result<long, IoError> one() {
    var line = ""
    _ = try await Stdin.readLineAsync(ref line)
    return Ok(line.length())
}

async Result<long, IoError> readers(int depth) {
    if depth == 0 {
        return await one()
    }
    var left = readers(depth - 1)
    var right = readers(depth - 1)
    long sum = try await left
    sum += try await right
    return Ok(sum)
}

async void tick() {
    await Task.delay(20)
    printError("tick")
}

async Result<void, IoError> main() {
    var ticking = tick()
    var lines = readers(6)
    await ticking
    print(try await lines)
    return Ok()
}
"#;
    use std::io::{BufRead, Write};
    let binary = build_compiled(PROGRAM, "input-waits-alone");
    let mut child = std::process::Command::new(&binary)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let errors = child.stderr.take().unwrap();
    let (send, lines) = std::sync::mpsc::channel();
    let reading = std::thread::spawn(move || {
        for line in std::io::BufReader::new(errors).lines() {
            let _ = send.send(line.unwrap());
        }
    });
    let first = lines.recv_timeout(std::time::Duration::from_secs(10));
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all("ab\n".repeat(64).as_bytes()).unwrap();
    drop(stdin);
    let run = child.wait_with_output().unwrap();
    reading.join().unwrap();
    assert_eq!(first.as_deref(), Ok("tick"), "the task that waits for a delay did not run while the others waited for the input");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "128\n");
    assert!(run.status.success());
}

/// A token ends a wait for a line of the input at once, and the line that comes later is not
/// lost: the next read gets it. The input is a pipe that is written only once the cancel has
/// been seen on the standard error, in compiled code and in the interpreter alike.
const INPUT_CANCEL: &str = r#"
async Result<void, IoError> main() {
    var line = ""
    var soon = CancellationSource.after(50)
    match await Stdin.readLineAsync(ref line, read soon.token()) {
        Ok(more) => printError($"early: {more} {line}")
        Error(cause) => printError($"first: {cause}")
    }
    var more = try await Stdin.readLineAsync(ref line)
    print($"then: {more} {line}")
    return Ok()
}
"#;

/// Waits for the first line on `errors`, then writes `late\n` into `input` and lets it go.
fn feed_after_cancel(errors: impl std::io::Read + Send + 'static, mut input: impl std::io::Write) -> Result<String, std::sync::mpsc::RecvTimeoutError> {
    use std::io::BufRead;
    let (send, lines) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(errors).lines() {
            let Ok(line) = line else { break };
            let _ = send.send(line);
        }
    });
    let first = lines.recv_timeout(std::time::Duration::from_secs(10));
    let _ = input.write_all(b"late\n");
    first
}

/// A program of one file compiled into `target/compiled` as `stem`, held to the warnings.
fn build_compiled(source: &str, stem: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target").join("compiled");
    std::fs::create_dir_all(&work).unwrap();
    let (c_file, binary) = (work.join(format!("{stem}.c")), program_at(&work, stem));
    std::fs::write(&c_file, sg::emit_c(source, &format!("{stem}.sg"), true).unwrap()).unwrap();
    let cc = c_command().args(["-std=c11", "-O1"]).args(warnings("")).arg("-o").arg(&binary).arg(&c_file).args(system_libs(&work)).output().unwrap();
    assert!(cc.status.success(), "the C compiler rejected {stem}:\n{}", String::from_utf8_lossy(&cc.stderr));
    binary
}

#[test]
fn compiled_input_cancel() {
    let binary = build_compiled(INPUT_CANCEL, "input-cancel");
    let mut child = std::process::Command::new(&binary)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let errors = child.stderr.take().unwrap();
    let stdin = child.stdin.take().unwrap();
    let first = feed_after_cancel(errors, stdin);
    let run = child.wait_with_output().unwrap();
    assert_eq!(first.as_deref(), Ok("first: Cancelled(Cancelled)"));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "then: true late\n");
    assert!(run.status.success());
}

#[test]
fn interpreted_input_cancel() {
    let (input, mut feeder) = std::io::pipe().unwrap();
    let (errors, mut err) = std::io::pipe().unwrap();
    let running = std::thread::spawn(move || {
        let mut out = Vec::new();
        let world = sg::World { args: Vec::new(), input: Box::new(input), input_is_terminal: false, input_waits: true, err: &mut err };
        let result = sg::run_sources_in(&sg::Sources::single(INPUT_CANCEL, ""), &mut out, world);
        (result.is_ok(), String::from_utf8(out).unwrap())
    });
    let first = feed_after_cancel(errors, &mut feeder);
    drop(feeder);
    let (ok, out) = running.join().unwrap();
    assert_eq!(first.as_deref(), Ok("first: Cancelled(Cancelled)"));
    assert_eq!(out, "then: true late\n");
    assert!(ok);
}

/// Async programs on the network, on processes and on threads, compiled with a pool of one
/// thread and of two (`SG_POOL_MOST`, which no program sets): the thread that waits on the loop
/// of the network is then soon the only one, and a task that a thread outside the pool wakes
/// meanwhile has to wake the loop to run; and a thousand tasks woken at once fill the queue of a
/// thread (`ok_pool_queues`). Under the thread sanitizer when it is there. Then once more
/// with the pauses of `-DSG_PAUSE_WAKES` (see `ok_wake_races`), under the thread sanitizer and
/// under the address sanitizer: a task freed while the thread that ended its wait still wakes
/// it is seen there.
#[test]
fn compiled_small_pools() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    if let Err(error) = sg::cc::compiler() {
        panic!("{error}: the back end cannot be tested");
    }
    let work = root.join("target").join("compiled-pools");
    std::fs::create_dir_all(&work).unwrap();
    let sanitizers = std::env::var_os("SG_NO_SANITIZERS").is_none();
    let mut failures = Vec::new();
    for stem in ["ok_net_wakes_from_thread", "ok_pool_queues", "ok_net_echo", "ok_net_cancel", "ok_net_slots", "ok_udp_async", "ok_async_results", "ok_process_async", "ok_process_cancel", "ok_wake_races", "ok_lock_when_async", "ok_channel_async"] {
        let source = std::fs::read_to_string(root.join("tests/cases").join(format!("{stem}.sg"))).unwrap();
        let expected = std::fs::read_to_string(root.join("tests/cases").join(format!("{stem}.out"))).unwrap();
        let c_file = work.join(format!("{stem}.c"));
        std::fs::write(&c_file, sg::emit_c(&source, &format!("{stem}.sg"), true).unwrap()).unwrap();
        // (the most threads of the pool, the pauses, the address sanitizer in place of the thread one)
        for (most, paused, address) in [(1, false, false), (2, false, false), (2, true, false), (2, true, true)] {
            if paused && !sanitizers {
                continue;
            }
            let label = format!("{stem}-{most}{}{}", if paused { "-paused" } else { "" }, if address { "-address" } else { "" });
            let binary = program_at(&work, &label);
            let mut cc = c_command();
            cc.args(["-std=c11", "-O1", "-g"]).arg(format!("-DSG_POOL_MOST={most}"));
            if paused {
                cc.arg("-DSG_PAUSE_WAKES");
            }
            // As in `compiled`, the thread sanitizer's build is not held to the warnings: gcc warns
            // that it does not follow `atomic_thread_fence`, which the counts of `shared` use.
            // Windows has no thread sanitizer: there the others look.
            if sanitizers && (cfg!(windows) || address) {
                cc.args(sanitizer_flags(&work));
            } else if sanitizers {
                cc.arg("-fsanitize=thread");
            } else {
                cc.args(warnings(stem));
            }
            let built = cc.arg("-o").arg(&binary).arg(&c_file).args(system_libs(&work)).output().unwrap();
            assert!(built.status.success(), "the C compiler rejected {stem}:\n{}", String::from_utf8_lossy(&built.stderr));
            let run = std::process::Command::new(&binary).current_dir(&work).env("PATH", tool_path()).stdin(std::process::Stdio::null()).output().unwrap();
            let actual = format!("{}{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr), exit_line(run.status.code()));
            if actual != expected {
                failures.push(format!("--- {label}\nexpected:\n{expected}\nactual:\n{actual}"));
            }
        }
    }
    if sanitizers && cfg!(windows) {
        eprintln!("thread sanitizer: not on Windows; the programs of small pools ran under the others");
    }
    assert!(failures.is_empty(), "{} program(s) differ:\n{}", failures.len(), failures.join("\n"));
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
    if let Err(error) = sg::cc::compiler() {
        panic!("{error}: the back end cannot be tested");
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
    folders.push(root.join("examples/zlib"));
    folders.push(root.join("examples/sqlite"));
    folders.push(root.join("examples/curl"));
    folders.push(root.join("examples/headers"));
    folders.push(root.join("examples/shutdown"));
    folders.push(root.join("examples/queue"));
    folders.push(root.join("examples/calclib"));
    folders.push(root.join("examples/bare/log"));
    folders.sort();
    for path in folders {
        if path.join("tests.out").exists() {
            programs.push((path.clone(), path.join("tests.out"), format!("{}/", path.parent().unwrap().display())));
        }
    }

    let sanitizers = std::env::var_os("SG_NO_SANITIZERS").is_none();
    if sanitizers && !cfg!(windows) {
        leak_check_sees_a_leak(&work);
    } else if sanitizers {
        eprintln!("leaks of the compiled tests: looked at by the count of blocks of each debug build (LeakSanitizer is not on Windows)");
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
        let (c_file, plain, checked) = (work.join(format!("{stem}.c")), program_at(&work, &stem), program_at(&work, &format!("{stem}-san")));
        std::fs::write(&c_file, &code).unwrap();
        // The plain build is held to the warnings (see `warnings`); the one under the
        // sanitizers is the same C.
        let plain_flags: Vec<String> = ["-O1", "-g"].iter().map(|flag| flag.to_string()).chain(warnings(&stem)).collect();
        let mut builds = vec![(&plain, plain_flags)];
        if sanitizers {
            builds.push((&checked, ["-O1", "-g"].iter().map(|flag| flag.to_string()).chain(sanitizer_flags(&work)).collect()));
        }
        let mut rejected = false;
        for (binary, flags) in &builds {
            let cc = c_command().args(["-std=c11", "-ffp-contract=off"]).args(flags).arg("-o").arg(binary).arg(&c_file).args(libraries_of(&code, binary)).args(system_libs(&work)).output().unwrap();
            if !cc.status.success() {
                failures.push(format!("--- {name}: the C compiler rejected it ({})\n{}", flags.join(" "), String::from_utf8_lossy(&cc.stderr)));
                rejected = true;
            }
        }
        if rejected {
            continue;
        }
        let expected = std::fs::read_to_string(golden_of(golden)).unwrap_or_default();
        for (binary, _) in &builds {
            let run = std::process::Command::new(binary).output().unwrap();
            let actual = format!("{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
            let actual = with_slashes(&actual);
            let actual = if within.is_empty() { actual } else { actual.replace(within.as_str(), "").replace(&within.replace('\\', "/"), "") };
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
                    // On Windows the count of blocks of the debug build looked, and would have
                    // said so and ended with 1.
                    Some(0) if cfg!(windows) => alone += 1,
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

/// A program is looked for by its name only in the folders of `PATH` that start with `/`: with
/// `.`, an empty part and a relative folder in `PATH`, a file of the folder the program works in
/// is not run by its name alone, in the interpreter and in compiled code. Our own `PATH` cannot be
/// changed inside the golden tests, so the program runs as a process of its own here.
/// A function C calls cannot be unwound through C: a panic in it ends the process, in the
/// interpreter as in compiled code. And the interpreter runs such a function only on a thread
/// that is in a call of C; compiled code runs it on any thread.
#[test]
fn callbacks_end_the_process() {
    const PANICS: &str = r#"
extern {
    void qsort(raw void base, nuint count, nuint size, CFunc<raw void, raw void, int> compare)
}

export int compareInts(raw void a, raw void b) {
    panic("compared")
}

void main() {
    var items = Memory.allocate<int>(2)
    print("before")
    unsafe qsort((raw void)items, 2, 4, compareInts)
    print("after")
}
"#;
    const THREAD_SG: &str = r#"
extern "./native.c" {
    int onThread()
}

export void calledBack(int n) {
    print($"called back with {n}")
}

void main() {
    print(unsafe onThread())
}
"#;
    const THREAD_C: &str = r#"
#include <stddef.h>
#include <stdint.h>
#ifdef _WIN32
#include <windows.h>

static DWORD WINAPI run(void* arg) {
    (void)arg;
    calledBack(7);
    return 0;
}

static int32_t onThread(void) {
    HANDLE thread = CreateThread(NULL, 0, run, NULL, 0, NULL);
    if (!thread) return -1;
    WaitForSingleObject(thread, INFINITE);
    CloseHandle(thread);
    return 0;
}
#else
#include <pthread.h>

static void* run(void* arg) {
    (void)arg;
    calledBack(7);
    return NULL;
}

static int32_t onThread(void) {
    pthread_t thread;
    if (pthread_create(&thread, NULL, run, NULL) != 0) return -1;
    pthread_join(thread, NULL);
    return 0;
}
#endif
"#;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target").join("callbacks-end");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(work.join("panics")).unwrap();
    std::fs::create_dir_all(work.join("thread")).unwrap();
    std::fs::write(work.join("panics/main.sg"), PANICS).unwrap();
    std::fs::write(work.join("thread/main.sg"), THREAD_SG).unwrap();
    std::fs::write(work.join("thread/native.c"), THREAD_C).unwrap();
    let run = |dir: &str| std::process::Command::new(env!("CARGO_BIN_EXE_sg")).arg("main.sg").current_dir(work.join(dir)).output().unwrap();

    let panicked = run("panics");
    assert_eq!(String::from_utf8_lossy(&panicked.stdout), "before\n");
    let said = String::from_utf8_lossy(&panicked.stderr);
    assert!(said.starts_with("panic: compared\n") && said.contains("main.sg:7:5"), "interpreter: {said}");
    assert_eq!(panicked.status.code(), Some(1));

    let elsewhere = run("thread");
    assert_eq!(String::from_utf8_lossy(&elsewhere.stdout), "");
    let said = String::from_utf8_lossy(&elsewhere.stderr);
    assert!(said.starts_with("error: C called `calledBack` on a thread where the interpreter is in no call of C\n"), "interpreter: {said}");
    assert_eq!(elsewhere.status.code(), Some(1));

    for (dir, stdout, code) in [("panics", "before\n", Some(1)), ("thread", "called back with 7\n0\n", Some(0))] {
        let sources = sg::load(&work.join(dir), false).unwrap();
        let (c_file, binary) = (work.join(dir).join("out.c"), program_at(&work.join(dir), "out"));
        std::fs::write(&c_file, sg::emit_c_sources(&sources, true).unwrap()).unwrap();
        let cc = c_command().args(["-std=c11", "-O1"]).args(warnings("")).arg("-o").arg(&binary).arg(&c_file).args(system_libs(&work)).output().unwrap();
        assert!(cc.status.success(), "the C compiler rejected {dir}:\n{}", String::from_utf8_lossy(&cc.stderr));
        let ran = std::process::Command::new(&binary).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&ran.stdout), stdout, "compiled {dir}: {}", String::from_utf8_lossy(&ran.stderr));
        assert_eq!(ran.status.code(), code, "compiled {dir}: {}", String::from_utf8_lossy(&ran.stderr));
        if dir == "panics" {
            assert!(String::from_utf8_lossy(&ran.stderr).starts_with("panic: compared\n"), "compiled: {}", String::from_utf8_lossy(&ran.stderr));
        }
    }
}

#[test]
fn process_path_lookup() {
    #[cfg(unix)]
    process_path_lookup_here();
    #[cfg(windows)]
    process_path_lookup_windows();
}

/// The same on Windows, with what Windows adds (choices 1032 to 1034): a name without an extension
/// is looked for as `name.exe`; `.`, an empty part, a relative folder and a folder of the drive
/// the program works in (`\bin`) are not looked in; a quoted folder is; a batch file runs only
/// when named with its extension, through `cmd.exe`, its arguments escaped (`%` is not a variable,
/// a quote stays in its argument), and an argument with a line break is `InvalidInput`; a code
/// with its highest bit set is written in hex.
#[cfg(windows)]
fn process_path_lookup_windows() {
    const PROGRAM: &str = r#"
void show(read string what, Result<string, ProcessError> done) {
    match done {
        Ok(text) => print($"{what}: {text.trim().replaceAll("\r\n", " | ")}")
        Error(cause) => print($"{what}: {cause}")
    }
}

Command says(read string program, read string word) {
    var command = Command(program)
    command.arg(read $"out={word}".toString())
    return command
}

void main() {
    show("found", says("found", "found").output())
    show("with its extension", says("found.exe", "found").output())
    show("quoted folder", says("quoted", "quoted").output())
    show("here", says("here", "here").output())
    show("relative", says("relative", "relative").output())
    show("rooted", says("rooted", "rooted").output())
    show("with a slash", says(".\\here", "here").output())
    var batch = Command("hello.bat")
    batch.args(read List<string> { "a b", "50%PATH%", "say \"hi\"", "" })
    show("batch", batch.output())
    show("batch without its extension", Command("hello").output())
    var broken = Command("hello.cmd")
    broken.arg("two\nlines")
    show("a line break", broken.output())
    show("crash code", Command("crash.bat").output())
}
"#;
    const EXPECTED: &str = "found: found\nwith its extension: found\nquoted folder: quoted\nhere: NotFound\nrelative: NotFound\nrooted: NotFound\nwith a slash: here\n\
                            batch: [a b] [50%PATH%] [say \"\"hi\"\"] []\nbatch without its extension: NotFound\na line break: InvalidInput\n\
                            crash code: the program exited with code 0xC0000005\n";
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target").join("process-path");
    let _ = std::fs::remove_dir_all(&work);
    for folder in ["bin", "ok", "with;semicolon", "rooted"] {
        std::fs::create_dir_all(work.join(folder)).unwrap();
    }
    // `sg-test-tool` under each name: it says the word it is given.
    let tool = std::env::split_paths(&tool_path()).next().unwrap().join("sg-test-tool.exe");
    for path in ["here.exe", "bin/relative.exe", "ok/found.exe", "with;semicolon/quoted.exe", "rooted/rooted.exe"] {
        std::fs::copy(&tool, work.join(path)).unwrap();
    }
    std::fs::write(work.join("ok/hello.bat"), "@echo [%~1] [%~2] [%~3] [%~4]\r\n").unwrap();
    std::fs::copy(work.join("ok/hello.bat"), work.join("ok/hello.cmd")).unwrap();
    std::fs::write(work.join("ok/crash.bat"), "@exit /b -1073741819\r\n").unwrap();
    let source = work.join("main.sg");
    std::fs::write(&source, PROGRAM).unwrap();
    let system = std::env::var("SystemRoot").unwrap();
    // `\rooted` is a folder of the drive the program works in: not absolute, not looked in.
    let rooted = work.join("rooted").to_string_lossy()[2..].to_string();
    let path = format!(".;;bin;{rooted};\"{}\";{};{system}\\System32", work.join("with;semicolon").display(), work.join("ok").display());
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_sg")).arg(&source).current_dir(&work).env("PATH", &path).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout), EXPECTED, "interpreter: {}", String::from_utf8_lossy(&run.stderr));
    let binary = work.join("main.exe");
    let built = std::process::Command::new(env!("CARGO_BIN_EXE_sg")).arg("build").arg(&source).arg("-o").arg(&binary).output().unwrap();
    assert!(built.status.success(), "sg build: {}", String::from_utf8_lossy(&built.stderr));
    let run = std::process::Command::new(&binary).current_dir(&work).env("PATH", &path).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout), EXPECTED, "compiled: {}", String::from_utf8_lossy(&run.stderr));
}

#[cfg(unix)]
fn process_path_lookup_here() {
    use std::os::unix::fs::PermissionsExt;
    const PROGRAM: &str = r#"
void show(read string what, Result<string, ProcessError> done) {
    match done {
        Ok(text) => print($"{what}: {text.trim()}")
        Error(cause) => print($"{what}: {cause}")
    }
}

void main() {
    var list = Command("ls")
    list.args(read List<string> { "-d", "/" })
    show("ls", list.output())
    show("here", Command("here").output())
    show("relative", Command("relative").output())
    show("with a slash", Command("./here").output())
}
"#;
    const EXPECTED: &str = "ls: /\nhere: NotFound\nrelative: NotFound\nwith a slash: here\n";
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target").join("process-path");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(work.join("bin")).unwrap();
    for (path, says) in [("ls", "wrong ls"), ("here", "here"), ("bin/relative", "relative")] {
        let file = work.join(path);
        std::fs::write(&file, format!("#!/bin/sh\necho {says}\n")).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let source = work.join("main.sg");
    std::fs::write(&source, PROGRAM).unwrap();
    let path = ".::bin:/bin:/usr/bin";
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_sg")).arg(&source).current_dir(&work).env("PATH", path).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout), EXPECTED, "interpreter: {}", String::from_utf8_lossy(&run.stderr));
    if std::process::Command::new("cc").arg("--version").output().is_err() {
        eprintln!("no C compiler (`cc`) found: the back end is not tested");
        return;
    }
    let (c_file, binary) = (work.join("main.c"), work.join("main"));
    std::fs::write(&c_file, sg::emit_c(PROGRAM, "main.sg", true).unwrap()).unwrap();
    let cc = std::process::Command::new("cc").args(["-std=c11", "-O1"]).args(warnings("")).arg("-o").arg(&binary).arg(&c_file).arg("-lm").arg("-pthread").output().unwrap();
    assert!(cc.status.success(), "the C compiler rejected it:\n{}", String::from_utf8_lossy(&cc.stderr));
    let run = std::process::Command::new(&binary).current_dir(&work).env("PATH", path).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout), EXPECTED, "compiled: {}", String::from_utf8_lossy(&run.stderr));
}

/// The time zone of the system, set by `TZ` to the rules of Central Europe written out (choice
/// 1310), so that no file of the system's zones is needed: an hour ahead of UTC in winter, two in
/// summer, from 02:00 on the last Sunday of March to 03:00 on the last Sunday of October (macOS
/// keeps such rules to the years from 1970 on, glibc does not, so a year before has only winter). The
/// interpreter and compiled code read it with `localtime_r`; Windows does not read `TZ`, so
/// there `ok_time_offset` checks the zone of the machine only against itself.
#[cfg(unix)]
#[test]
fn local_time_zone() {
    const PROGRAM: &str = r#"
void local(int month, int day, int hour, int minute) {
    match DateTimeOffset.local(2026, month, day, hour, minute, 0) {
        Ok(moment) => print($"{month}-{day} {hour}:{minute}: {moment} = {moment.toUtc()}")
        Error(cause) => print($"{month}-{day} {hour}:{minute}: {cause}")
    }
}

void at(read string utc) {
    var moment = DateTime.parse(utc) ?? panic("a moment")
    var seen = moment.toLocal()
    print($"{moment} -> {seen} {seen.offset()} {seen.hour()} {seen.dayOfWeek()}")
}

void main() {
    at("2026-01-15T12:00:00Z")
    at("2026-07-01T12:00:00Z")
    at("1950-01-15T12:00:00Z")
    at("2026-03-29T00:59:59Z")
    at("2026-03-29T01:00:00Z")
    at("2026-10-25T00:59:59Z")
    at("2026-10-25T01:00:00Z")
    at("2026-12-31T23:30:00Z")
    // The hour skipped in spring, and the hour seen twice in autumn: the earlier of the two.
    local(3, 29, 1, 59)
    local(3, 29, 2, 0)
    local(3, 29, 2, 30)
    local(3, 29, 3, 0)
    local(10, 25, 1, 59)
    local(10, 25, 2, 0)
    local(10, 25, 2, 30)
    local(10, 25, 3, 0)
    // `plus` keeps the offset; through UTC the offset of the new moment.
    var winter = DateTimeOffset.local(2026, 1, 15, 12, 0, 0) ?? panic("a local time")
    var half = Duration.fromSeconds(86400 * 182)
    print(winter.plus(half))
    print(winter.toUtc().plus(half).toLocal())
}
"#;
    const EXPECTED: &str = "\
2026-01-15T12:00:00Z -> 2026-01-15T13:00:00+01:00 3600s 13 Thursday
2026-07-01T12:00:00Z -> 2026-07-01T14:00:00+02:00 7200s 14 Wednesday
1950-01-15T12:00:00Z -> 1950-01-15T13:00:00+01:00 3600s 13 Sunday
2026-03-29T00:59:59Z -> 2026-03-29T01:59:59+01:00 3600s 1 Sunday
2026-03-29T01:00:00Z -> 2026-03-29T03:00:00+02:00 7200s 3 Sunday
2026-10-25T00:59:59Z -> 2026-10-25T02:59:59+02:00 7200s 2 Sunday
2026-10-25T01:00:00Z -> 2026-10-25T02:00:00+01:00 3600s 2 Sunday
2026-12-31T23:30:00Z -> 2027-01-01T00:30:00+01:00 3600s 0 Friday
3-29 1:59: 2026-03-29T01:59:00+01:00 = 2026-03-29T00:59:00Z
3-29 2:0: SkippedLocalTime
3-29 2:30: SkippedLocalTime
3-29 3:0: 2026-03-29T03:00:00+02:00 = 2026-03-29T01:00:00Z
10-25 1:59: 2026-10-25T01:59:00+02:00 = 2026-10-24T23:59:00Z
10-25 2:0: 2026-10-25T02:00:00+02:00 = 2026-10-25T00:00:00Z
10-25 2:30: 2026-10-25T02:30:00+02:00 = 2026-10-25T00:30:00Z
10-25 3:0: 2026-10-25T03:00:00+01:00 = 2026-10-25T02:00:00Z
2026-07-16T12:00:00+01:00
2026-07-16T13:00:00+02:00
";
    const ZONE: &str = "CET-1CEST,M3.5.0,M10.5.0/3";
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target").join("local-time-zone");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let source = work.join("main.sg");
    std::fs::write(&source, PROGRAM).unwrap();
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_sg")).arg(&source).env("TZ", ZONE).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout), EXPECTED, "interpreter: {}", String::from_utf8_lossy(&run.stderr));
    if std::process::Command::new("cc").arg("--version").output().is_err() {
        eprintln!("no C compiler (`cc`) found: the back end is not tested");
        return;
    }
    let (c_file, binary) = (work.join("main.c"), work.join("main"));
    std::fs::write(&c_file, sg::emit_c(PROGRAM, "main.sg", true).unwrap()).unwrap();
    let cc = c_command().args(["-std=c11", "-O1"]).args(warnings("")).arg("-o").arg(&binary).arg(&c_file).arg("-lm").output().unwrap();
    assert!(cc.status.success(), "the C compiler rejected it:\n{}", String::from_utf8_lossy(&cc.stderr));
    let run = std::process::Command::new(&binary).env("TZ", ZONE).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout), EXPECTED, "compiled: {}", String::from_utf8_lossy(&run.stderr));
}

/// `examples/shutdown` sent real signals from outside, as Ctrl-C and `kill` send them, interpreted
/// and compiled (also under the thread sanitizer): with no connection, the first signal ends it;
/// with one open, it stops taking connections and serves that one to its end; a second signal cuts
/// it. Each time it says how many it served and ends with 0. Each signal is sent once the line that
/// shows the last one taken is seen, so none is folded into another.
#[test]
fn shutdown_on_signals() {
    #[cfg(unix)]
    shutdown_on_signals_here();
    #[cfg(windows)]
    shutdown_on_signals_windows();
}

/// `examples/shutdown` on Windows, interpreted and compiled, sent Ctrl-C and Ctrl-Break as a
/// console sends them, and its console closed (choice 1046). A small program of C, `tests/sender.c`,
/// attaches to the server's console and raises the event there, so the tests do not lose their
/// own console; for a closed console it runs the server in a pseudo console of its own and closes
/// that, as Windows Terminal closes a tab. A closed console is `Terminate`.
#[cfg(windows)]
fn shutdown_on_signals_windows() {
    use std::io::{BufRead, Read, Write};
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CTRL_C: &str = "0";
    const CTRL_BREAK: &str = "1";
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let example = root.join("examples/shutdown");
    let work = root.join("target").join("shutdown");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let sender = program_at(&work, "sender");
    let cc = c_command().arg("-o").arg(&sender).arg(root.join("tests/sender.c")).args(system_libs(&work)).output().unwrap();
    assert!(cc.status.success(), "the C compiler rejected the sender:\n{}", String::from_utf8_lossy(&cc.stderr));
    let binary = program_at(&work, "shutdown");
    let built = std::process::Command::new(env!("CARGO_BIN_EXE_sg")).arg("build").arg(example.join("main.sg")).arg("-o").arg(&binary).output().unwrap();
    assert!(built.status.success(), "sg build: {}", String::from_utf8_lossy(&built.stderr));
    let ways: Vec<(&str, Vec<std::ffi::OsString>)> = vec![("interpreted", vec![env!("CARGO_BIN_EXE_sg").into(), example.join("main.sg").into()]), ("compiled", vec![binary.into()])];
    for (way, command) in &ways {
        for scenario in ["idle", "one open", "cut", "closed"] {
            // A console closed is a pseudo console the sender makes: the server's first line is the
            // sender's, with the server's number.
            let mut server = if scenario == "closed" {
                let mut line = std::ffi::OsString::new();
                for part in command {
                    line.push("\"");
                    line.push(part);
                    line.push("\" ");
                }
                line.push("127.0.0.1:0");
                let mut pty = std::process::Command::new(&sender);
                pty.arg("pty").arg(line).stdin(std::process::Stdio::piped());
                pty
            } else {
                let mut server = std::process::Command::new(&command[0]);
                server.args(&command[1..]).arg("127.0.0.1:0").stdin(std::process::Stdio::null());
                server
            };
            let mut child = server.creation_flags(CREATE_NO_WINDOW).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
            let mut pid = child.id().to_string();
            let (lines, heard) = std::sync::mpsc::channel();
            let stdout = child.stdout.take().unwrap();
            std::thread::spawn(move || {
                for line in std::io::BufReader::new(stdout).lines() {
                    if lines.send(line.unwrap()).is_err() {
                        break;
                    }
                }
            });
            let next = || heard.recv_timeout(std::time::Duration::from_secs(20)).unwrap_or_else(|_| panic!("{way}, {scenario}: the server said nothing more"));
            let mut first = next();
            if scenario == "closed" {
                pid = first.strip_prefix("server ").unwrap_or_else(|| panic!("{way}, {scenario}: {first}")).to_string();
                first = next();
            }
            let port: u16 = first.strip_prefix("listening on port ").and_then(|port| port.parse().ok()).unwrap_or_else(|| panic!("{way}, {scenario}: {first}"));
            let mut said = vec![first];
            let pid = pid;
            let send = |event: &str| {
                let sent = std::process::Command::new(&sender).args([&pid[..], event]).creation_flags(CREATE_NO_WINDOW).status().unwrap();
                assert!(sent.success(), "{way}, {scenario}: the event could not be raised ({sent})");
            };
            let talk = |conn: &mut std::net::TcpStream, text: &str| {
                conn.write_all(text.as_bytes()).unwrap();
                let mut back = vec![0u8; text.len()];
                conn.read_exact(&mut back).unwrap();
                assert_eq!(back, text.as_bytes(), "{way}, {scenario}: the echo");
            };
            match scenario {
                "idle" => send(CTRL_C),
                "closed" => child.stdin.take().unwrap().write_all(b"close\n").unwrap(),
                "one open" => {
                    let mut conn = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
                    talk(&mut conn, "hello");
                    send(CTRL_C);
                    said.push(next());
                    talk(&mut conn, "still served");
                }
                _ => {
                    let mut conn = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
                    talk(&mut conn, "hello");
                    send(CTRL_BREAK);
                    said.push(next());
                    send(CTRL_BREAK);
                    said.push(next());
                    let mut rest = Vec::new();
                    conn.set_read_timeout(Some(std::time::Duration::from_secs(20))).unwrap();
                    assert_eq!(conn.read_to_end(&mut rest).unwrap_or(0), 0, "{way}, {scenario}: the connection was not cut");
                }
            }
            // A server the event did not reach would hold the test: it is stopped after 30 s. What
            // it has loaded is said with it: a console program with `user32` is told of a closed
            // console otherwise than one without.
            let mut waited = 0;
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if waited == 300 {
                    let loaded = std::process::Command::new("tasklist").args(["/m", "/fi", &format!("PID eq {pid}")]).output().map(|out| String::from_utf8_lossy(&out.stdout).into_owned()).unwrap_or_default();
                    let _ = child.kill();
                    panic!("{way}, {scenario}: the server did not end; it said {said:?}; it had loaded:\n{loaded}");
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
                waited += 1;
            };
            said.extend(heard.iter());
            let mut errors = String::new();
            child.stderr.take().unwrap().read_to_string(&mut errors).unwrap();
            let expected: &[&str] = match scenario {
                "idle" => &["Interrupt: no new connections, waiting for the open ones", "stopped cleanly after 0 connection(s)"],
                "closed" => &["Terminate: no new connections, waiting for the open ones", "stopped cleanly after 0 connection(s)"],
                "one open" => &["Interrupt: no new connections, waiting for the open ones", "stopped cleanly after 1 connection(s)"],
                _ => &[
                    "Interrupt: no new connections, waiting for the open ones",
                    "Interrupt again: cutting the open connections",
                    "a connection was cut",
                    "stopped cleanly after 1 connection(s)",
                ],
            };
            assert_eq!(&said[1..], expected, "{way}, {scenario}; standard error: {errors}");
            assert_eq!(status.code(), Some(0), "{way}, {scenario}: {errors}");
            assert!(errors.is_empty(), "{way}, {scenario}: {errors}");
        }
    }
}

#[cfg(unix)]
fn shutdown_on_signals_here() {
    use std::io::{BufRead, Read, Write};
    extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    const INTERRUPT: i32 = 2;
    const TERMINATE: i32 = 15;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let example = root.join("examples/shutdown");
    let work = root.join("target").join("shutdown");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();

    // How the server is started: interpreted, then compiled if there is a C compiler.
    let mut ways: Vec<(String, Vec<std::ffi::OsString>)> = vec![("interpreted".to_string(), vec![env!("CARGO_BIN_EXE_sg").into(), example.join("main.sg").into()])];
    if std::process::Command::new("cc").arg("--version").output().is_ok() {
        let sources = sg::load(&example, false).unwrap();
        let code = sg::emit_c_sources(&sources, true).unwrap();
        let c_file = work.join("shutdown.c");
        std::fs::write(&c_file, &code).unwrap();
        for (name, flags) in [("compiled", &["-O1"][..]), ("compiled under the thread sanitizer", &["-O1", "-g", "-fsanitize=thread"][..])] {
            let binary = work.join(name.replace(' ', "-"));
            let cc = std::process::Command::new("cc").arg("-std=c11").args(flags).args(warnings("")).arg("-o").arg(&binary).arg(&c_file).args(sg::emit_c::link_flags(&code)).arg("-lm").arg("-pthread").output().unwrap();
            assert!(cc.status.success(), "the C compiler rejected examples/shutdown ({name}):\n{}", String::from_utf8_lossy(&cc.stderr));
            ways.push((name.to_string(), vec![binary.into()]));
        }
    } else {
        eprintln!("no C compiler (`cc`) found: examples/shutdown is not compiled");
    }

    for (way, command) in &ways {
        for scenario in ["idle", "one open", "cut"] {
            let mut child = std::process::Command::new(&command[0])
                .args(&command[1..])
                .arg("127.0.0.1:0")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let pid = child.id() as i32;
            let send = |signal: i32| {
                // SAFETY: the child has not been waited for, so the number is still its own.
                assert_eq!(unsafe { kill(pid, signal) }, 0, "{way}, {scenario}: the signal could not be sent");
            };
            // The lines come through a thread, so a server that says nothing fails the test
            // instead of holding it.
            let (lines, heard) = std::sync::mpsc::channel();
            let stdout = child.stdout.take().unwrap();
            std::thread::spawn(move || {
                for line in std::io::BufReader::new(stdout).lines() {
                    if lines.send(line.unwrap()).is_err() {
                        break;
                    }
                }
            });
            let next = || heard.recv_timeout(std::time::Duration::from_secs(20)).unwrap_or_else(|_| panic!("{way}, {scenario}: the server said nothing more"));
            let first = next();
            let port: u16 = first.strip_prefix("listening on port ").and_then(|port| port.parse().ok()).unwrap_or_else(|| panic!("{way}, {scenario}: {first}"));
            let mut said = vec![first];
            let talk = |conn: &mut std::net::TcpStream, text: &str| {
                conn.write_all(text.as_bytes()).unwrap();
                let mut back = vec![0u8; text.len()];
                conn.read_exact(&mut back).unwrap();
                assert_eq!(back, text.as_bytes(), "{way}, {scenario}: the echo");
            };
            match scenario {
                "idle" => send(INTERRUPT),
                "one open" => {
                    let mut conn = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
                    talk(&mut conn, "hello");
                    send(INTERRUPT);
                    said.push(next());
                    talk(&mut conn, "still served");
                }
                _ => {
                    let mut conn = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
                    talk(&mut conn, "hello");
                    send(TERMINATE);
                    said.push(next());
                    send(TERMINATE);
                    said.push(next());
                    let mut rest = Vec::new();
                    conn.set_read_timeout(Some(std::time::Duration::from_secs(20))).unwrap();
                    assert_eq!(conn.read_to_end(&mut rest).unwrap(), 0, "{way}, {scenario}: the connection was not cut");
                }
            }
            let status = child.wait().unwrap();
            said.extend(heard.iter());
            let mut errors = String::new();
            child.stderr.take().unwrap().read_to_string(&mut errors).unwrap();
            let expected: &[&str] = match scenario {
                "idle" => &["Interrupt: no new connections, waiting for the open ones", "stopped cleanly after 0 connection(s)"],
                "one open" => &["Interrupt: no new connections, waiting for the open ones", "stopped cleanly after 1 connection(s)"],
                _ => &[
                    "Terminate: no new connections, waiting for the open ones",
                    "Terminate again: cutting the open connections",
                    "a connection was cut",
                    "stopped cleanly after 1 connection(s)",
                ],
            };
            assert_eq!(&said[1..], expected, "{way}, {scenario}; standard error: {errors}");
            assert_eq!(status.code(), Some(0), "{way}, {scenario}: {errors}");
            assert!(errors.is_empty(), "{way}, {scenario}: {errors}");
        }
    }
}

/// A library built from SG and called from C (`sg build --library` and `--shared`): the
/// calculator of `examples/calclib`, with its program of C, prints `main.out` as a static
/// library, a shared one and a release build, under the sanitizers too, and leaves nothing
/// behind. Two libraries of SG live in one program, each with its runtime; C calls a library
/// from threads of its own, under the thread sanitizer; plain structs that point at each other
/// are declared in a header C compiles; a panic aborts the program that loaded it; and the
/// checker asks of a library what it gives C.
#[test]
fn libraries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sg_bin = env!("CARGO_BIN_EXE_sg");
    let work = root.join("target").join("libraries");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let example = root.join("examples/calclib");
    let expected = std::fs::read_to_string(example.join("main.out")).unwrap();
    // A program of C is linked as a program of SG is, with what this system needs after it.
    let system = system_libs(&work);
    let cc = |args: &[&std::ffi::OsStr]| {
        let made = c_command().args(["-std=c11"]).args(args).args(&system).output().unwrap();
        assert!(made.status.success(), "the C compiler rejected {args:?}:\n{}", String::from_utf8_lossy(&made.stderr));
    };
    // What a program that links the static library defines first on Windows (see the header).
    let static_define: &std::ffi::OsStr = if cfg!(windows) { "-DCALCLIB_STATIC".as_ref() } else { "-DCALCLIB_NOTHING".as_ref() };
    let sg = |args: &[&str], dir: &Path| {
        let made = std::process::Command::new(sg_bin).arg("build").args(args).current_dir(dir).output().unwrap();
        assert!(made.status.success(), "sg build {args:?}: {}{}", String::from_utf8_lossy(&made.stdout), String::from_utf8_lossy(&made.stderr));
        String::from_utf8_lossy(&made.stdout).into_owned()
    };
    let main_c = example.join("main.c");
    // What a program of C prints with `printf`: on Windows its standard output is text, and
    // C's library ends each line with `\r\n` there.
    let printed = |out: &[u8]| String::from_utf8_lossy(out).replace("\r\n", "\n");

    // The static library, debug and release, and the shared one.
    for (dir, flag, release) in [("static", "--library", false), ("release", "--library", true), ("shared", "--shared", false)] {
        let out = work.join(dir);
        std::fs::create_dir_all(&out).unwrap();
        let file = match flag {
            "--library" if cfg!(windows) => "calclib.lib".to_string(),
            "--library" => "libcalclib.a".to_string(),
            _ if cfg!(windows) => "calclib.dll".to_string(),
            _ if cfg!(target_os = "macos") => "libcalclib.dylib".to_string(),
            _ => "libcalclib.so".to_string(),
        };
        let mut args = vec![example.to_str().unwrap(), flag, "-o", file.as_str()];
        if release {
            args.push("--release");
        }
        let said = sg(&args, &out);
        assert!(said.starts_with(&format!("built {file}")) && said.contains(" and "), "{said}");
        let header = std::fs::read_to_string(out.join("calclib.h")).unwrap();
        assert!(header.contains("typedef struct Calculator Calculator;") && header.contains("CALCLIB_API int32_t calc_eval(Calculator *calc, const char *text, double *out);"), "{header}");
        let program = program_at(&out, "calc");
        let mut flags: Vec<std::ffi::OsString> = ["-Wall", "-Wextra", "-Werror", "-pedantic", "-o"].iter().map(Into::into).collect();
        flags.push(program.clone().into());
        flags.push("-I".into());
        flags.push(out.clone().into());
        flags.push(main_c.clone().into());
        match flag {
            "--library" => flags.extend([static_define.into(), out.join(&file).into()]),
            // The program lies beside the DLL, where Windows looks for it.
            _ if cfg!(windows) => flags.push(out.join(format!("{file}.lib")).into()),
            _ => flags.extend([format!("-L{}", out.display()).into(), "-lcalclib".into(), format!("-Wl,-rpath,{}", out.display()).into()]),
        }
        cc(&flags.iter().map(|flag| flag.as_os_str()).collect::<Vec<_>>());
        let ran = std::process::Command::new(&program).output().unwrap();
        assert_eq!(printed(&ran.stdout), expected, "{dir}: {}", String::from_utf8_lossy(&ran.stderr));
        assert!(ran.status.success(), "{dir}: {}", String::from_utf8_lossy(&ran.stderr));
        // Only the functions of the header are seen from outside.
        let defined = seen_from_outside(&out.join(&file), flag == "--shared");
        assert!(!defined.is_empty() && defined.iter().all(|name| name.starts_with("calc_")), "{dir} shows more than its functions: {defined:?}");
        if dir == "static" && cfg!(windows) {
            eprintln!("a library built from SG is run under the sanitizers on Linux and macOS, not on Windows");
        }
        if dir == "static" && std::env::var_os("SG_NO_SANITIZERS").is_none() && !cfg!(windows) {
            // The same C under the sanitizers, as one program, and looked at for leaks.
            let code = std::process::Command::new(sg_bin).args(["emit-c", example.to_str().unwrap(), "--library"]).output().unwrap();
            assert!(code.status.success());
            std::fs::write(out.join("calclib.c"), &code.stdout).unwrap();
            let sanitized = out.join("calc-san");
            cc(&["-O1".as_ref(), "-g".as_ref(), "-fsanitize=address,undefined".as_ref(), "-fno-sanitize-recover=all".as_ref(), "-I".as_ref(), out.as_os_str(), "-o".as_ref(), sanitized.as_os_str(), main_c.as_os_str(), out.join("calclib.c").as_os_str()]);
            let ran = std::process::Command::new(&sanitized).output().unwrap();
            assert_eq!(String::from_utf8_lossy(&ran.stdout), expected, "under the sanitizers: {}", String::from_utf8_lossy(&ran.stderr));
            leak_check_sees_a_leak(&work);
            if let Some(report) = left_behind(&program, &sanitized, &[], Path::new("")) {
                panic!("the program of C with the library left memory behind:\n{report}");
            }
        }
    }

    // Two libraries of SG in one program: each has its runtime, and none of its names is seen.
    let other = work.join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(
        other.join("words.sg"),
        "// Counts the words of a text with a map and a task, as the calculator uses both.\nint countIn(string text) {\n    var seen = HashSet<string>()\n    for word in text.split(\" \") {\n        if word.length() > 0 {\n            _ = seen.add(word.clone())\n        }\n    }\n    return (int)seen.length()\n}\n\nexport int words_distinct(read CString text) {\n    var copy = string.fromCString(text) ?? \"\"\n    var task = Task.run(countIn, copy)\n    return task.wait()\n}\n\n// C's `long` crosses as it is.\nexport CLong words_next(CLong at) {\n    return (CLong)((long)at + 1)\n}\n",
    )
    .unwrap();
    std::fs::write(
        other.join("both.c"),
        "#include <stdio.h>\n#include \"calclib.h\"\n#include \"words.h\"\nint main(void) {\n    Calculator *calc = calc_new();\n    double value = 0;\n    calc_eval(calc, \"6 * 7\", &value);\n    printf(\"%g %d %ld\\n\", value, (int)words_distinct(\"to be or not to be\"), words_next(41L));\n    calc_free(calc);\n    return 0;\n}\n",
    )
    .unwrap();
    let (calclib, words) = match cfg!(windows) {
        true => ("calclib.lib", "other.lib"),
        false => ("libcalclib.a", "libother.a"),
    };
    sg(&[example.to_str().unwrap(), "--library"], &other);
    sg(&["words.sg", "--library", "-o", words], &other);
    let header = std::fs::read_to_string(other.join("words.h")).unwrap();
    assert!(header.contains("long words_next(long at);"), "{header}");
    let both = program_at(&other, "both");
    cc(&["-Wall".as_ref(), "-Werror".as_ref(), static_define, "-DWORDS_STATIC".as_ref(), "-I".as_ref(), other.as_os_str(), "-o".as_ref(), both.as_os_str(), other.join("both.c").as_os_str(), other.join(calclib).as_os_str(), other.join(words).as_os_str()]);
    let ran = std::process::Command::new(&both).output().unwrap();
    assert_eq!(printed(&ran.stdout), "42 4 42\n", "{}", String::from_utf8_lossy(&ran.stderr));

    // C calls the library from threads of its own, each with a calculator, under the thread
    // sanitizer: the runtime starts what it needs on first use, from any thread.
    let threads_c = work.join("threads.c");
    std::fs::write(
        &threads_c,
        "#include <stdio.h>\n#include \"calclib.h\"\nstatic long sum_of(void) {\n    Calculator *calc = calc_new();\n    double sum = 0, value = 0;\n    for (int i = 0; i < 20; i++) {\n        calc_set(calc, \"i\", i);\n        calc_eval(calc, \"x = i * 2\", &value);\n    }\n    calc_sum(calc, \"k + x\", \"k\", 1, 100, &sum);\n    calc_free(calc);\n    return (long)sum;\n}\n#ifdef _WIN32\n#include <windows.h>\nstatic long sums[4];\nstatic DWORD WINAPI work(void *arg) {\n    *(long *)arg = sum_of();\n    return 0;\n}\nint main(void) {\n    HANDLE threads[4];\n    for (int i = 0; i < 4; i++) threads[i] = CreateThread(NULL, 0, work, &sums[i], 0, NULL);\n    WaitForMultipleObjects(4, threads, TRUE, INFINITE);\n    long total = 0;\n    for (int i = 0; i < 4; i++) { CloseHandle(threads[i]); total += sums[i]; }\n    printf(\"%ld\\n\", total);\n    return 0;\n}\n#else\n#include <pthread.h>\nstatic void *work(void *arg) {\n    (void)arg;\n    return (void *)sum_of();\n}\nint main(void) {\n    pthread_t threads[4];\n    for (int i = 0; i < 4; i++) pthread_create(&threads[i], NULL, work, NULL);\n    long total = 0;\n    for (int i = 0; i < 4; i++) { void *got; pthread_join(threads[i], &got); total += (long)got; }\n    printf(\"%ld\\n\", total);\n    return 0;\n}\n#endif\n",
    )
    .unwrap();
    // Windows has no thread sanitizer: there the threads run the static library as it is.
    if cfg!(windows) {
        let threads = program_at(&work, "threads");
        let static_dir = work.join("static");
        cc(&[static_define, "-I".as_ref(), static_dir.as_os_str(), "-o".as_ref(), threads.as_os_str(), threads_c.as_os_str(), static_dir.join("calclib.lib").as_os_str()]);
        let ran = std::process::Command::new(&threads).output().unwrap();
        assert_eq!(printed(&ran.stdout), "35400\n", "{}", String::from_utf8_lossy(&ran.stderr));
        eprintln!("thread sanitizer: not on Windows; the threads of C ran the library without it");
    } else if std::env::var_os("SG_NO_SANITIZERS").is_none() {
        race_check_sees_a_race(&work);
        let racy = work.join("threads-tsan");
        let static_dir = work.join("static");
        cc(&["-O1".as_ref(), "-g".as_ref(), "-fsanitize=thread".as_ref(), "-I".as_ref(), static_dir.as_os_str(), "-o".as_ref(), racy.as_os_str(), threads_c.as_os_str(), static_dir.join("calclib.c").as_os_str()]);
        let ran = std::process::Command::new(&racy).output().unwrap();
        // Each thread: 5050 + 100 * 38.
        assert_eq!(printed(&ran.stdout), "35400\n", "{}", String::from_utf8_lossy(&ran.stderr));
        assert!(ran.status.success() && !String::from_utf8_lossy(&ran.stderr).contains("ThreadSanitizer"), "{}", String::from_utf8_lossy(&ran.stderr));
    }

    // A library that starts no task of its own is called from threads of C just the same: every
    // program with an `export` function is built for several threads (choice 1334), also one
    // whose C calls back from a thread of its own.
    let plain = work.join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    std::fs::write(plain.join("describe.sg"), "// Writes a list into a text and gives its length; no task, no `unsafe`.\nexport long describe(int n) {\n    var items = List<int>()\n    for i in 0..(n % 200) {\n        items.add(i)\n    }\n    var text = $\"the items {items} of {n}\".toString()\n    return text.length()\n}\n").unwrap();
    std::fs::write(plain.join("callback.sg"), "export void tick(int n) {\n    print(n)\n}\n\nvoid main() {\n    tick(1)\n}\n").unwrap();
    for name in ["describe.sg", "callback.sg"] {
        let words: &[&str] = if name == "describe.sg" { &["emit-c", "describe.sg", "--library"] } else { &["emit-c", "callback.sg"] };
        let code = std::process::Command::new(sg_bin).args(words).current_dir(&plain).output().unwrap();
        assert!(code.status.success(), "{}", String::from_utf8_lossy(&code.stderr));
        let code = String::from_utf8_lossy(&code.stdout).into_owned();
        assert!(code.contains("#define SG_THREADS 1\n"), "{name} is not built for several threads");
        if name == "describe.sg" {
            std::fs::write(plain.join("describe.c"), code).unwrap();
        }
    }
    let said = sg(&["describe.sg", "--library", "-o", if cfg!(windows) { "describe.lib" } else { "libdescribe.a" }], &plain);
    assert!(said.starts_with("built "), "{said}");
    let plain_c = plain.join("threads.c");
    std::fs::write(
        &plain_c,
        "#include <stdio.h>\n#include \"describe.h\"\nstatic long sum_of(void) {\n    long total = 0;\n    for (int i = 0; i < 2000; i++) total += (long)describe(i);\n    return total;\n}\n#ifdef _WIN32\n#include <windows.h>\nstatic long sums[4];\nstatic DWORD WINAPI work(void *arg) {\n    *(long *)arg = sum_of();\n    return 0;\n}\nint main(void) {\n    HANDLE threads[4];\n    for (int i = 0; i < 4; i++) threads[i] = CreateThread(NULL, 0, work, &sums[i], 0, NULL);\n    WaitForMultipleObjects(4, threads, TRUE, INFINITE);\n    long total = 0;\n    for (int i = 0; i < 4; i++) { CloseHandle(threads[i]); total += sums[i]; }\n    printf(\"%ld\\n\", total);\n    return 0;\n}\n#else\n#include <pthread.h>\nstatic void *work(void *arg) {\n    (void)arg;\n    return (void *)sum_of();\n}\nint main(void) {\n    pthread_t threads[4];\n    for (int i = 0; i < 4; i++) pthread_create(&threads[i], NULL, work, NULL);\n    long total = 0;\n    for (int i = 0; i < 4; i++) { void *got; pthread_join(threads[i], &got); total += (long)got; }\n    printf(\"%ld\\n\", total);\n    return 0;\n}\n#endif\n",
    )
    .unwrap();
    if cfg!(windows) {
        let threads = program_at(&plain, "threads");
        cc(&["-DDESCRIBE_STATIC".as_ref(), "-I".as_ref(), plain.as_os_str(), "-o".as_ref(), threads.as_os_str(), plain_c.as_os_str(), plain.join("describe.lib").as_os_str()]);
        let ran = std::process::Command::new(&threads).output().unwrap();
        assert!(ran.status.success(), "{}", String::from_utf8_lossy(&ran.stderr));
    } else if std::env::var_os("SG_NO_SANITIZERS").is_none() {
        let racy = plain.join("threads-tsan");
        cc(&["-O1".as_ref(), "-g".as_ref(), "-fsanitize=thread".as_ref(), "-I".as_ref(), plain.as_os_str(), "-o".as_ref(), racy.as_os_str(), plain_c.as_os_str(), plain.join("describe.c").as_os_str()]);
        let ran = std::process::Command::new(&racy).output().unwrap();
        assert!(ran.status.success() && !String::from_utf8_lossy(&ran.stderr).contains("ThreadSanitizer"), "{}", String::from_utf8_lossy(&ran.stderr));
    }

    // Plain structs that point at themselves and at each other, one held by value in the
    // other: the header names first those a struct before them points at, and C compiles it.
    let linked = work.join("linked");
    std::fs::create_dir_all(&linked).unwrap();
    std::fs::write(
        linked.join("nodes.sg"),
        "struct Node {\n    raw Node next\n    raw Owner owner\n    int value\n}\n\nstruct Owner {\n    raw Node first\n    Node spare\n}\n\nunion Cell {\n    raw Cell next\n    int value\n}\n\n// Adds up the values of a list.\nexport int nodes_sum(raw Node list, Cell cell) {\n    var sum = 0\n    var at = list\n    while at != null {\n        sum += unsafe at[0].value\n        at = unsafe at[0].next\n    }\n    return sum\n}\n",
    )
    .unwrap();
    std::fs::write(
        linked.join("nodes.c"),
        "#include <stdio.h>\n#include \"nodes.h\"\nint main(void) {\n    Owner owner = {0};\n    Node last = { NULL, &owner, 2 };\n    Node first = { &last, &owner, 40 };\n    Cell cell = { NULL };\n    printf(\"%d\\n\", (int)nodes_sum(&first, cell));\n    return 0;\n}\n",
    )
    .unwrap();
    let nodes_lib = if cfg!(windows) { "nodes.lib" } else { "libnodes.a" };
    sg(&["nodes.sg", "--library", "-o", nodes_lib], &linked);
    let nodes = program_at(&linked, "nodes");
    let nodes_define: &std::ffi::OsStr = if cfg!(windows) { "-DNODES_STATIC".as_ref() } else { "-DNODES_NOTHING".as_ref() };
    cc(&["-Wall".as_ref(), "-Wextra".as_ref(), "-Werror".as_ref(), "-pedantic".as_ref(), nodes_define, "-I".as_ref(), linked.as_os_str(), "-o".as_ref(), nodes.as_os_str(), linked.join("nodes.c").as_os_str(), linked.join(nodes_lib).as_os_str()]);
    let ran = std::process::Command::new(&nodes).output().unwrap();
    assert_eq!(printed(&ran.stdout), "42\n", "{}", String::from_utf8_lossy(&ran.stderr));

    // A panic writes its message and aborts: other threads of the program may be running, so
    // C's `exit` and the program's `atexit` functions do not run.
    let panics_c = work.join("panics.c");
    std::fs::write(&panics_c, "#include <stdio.h>\n#include \"calclib.h\"\nint main(void) {\n    double value = 0;\n    printf(\"before\\n\");\n    fflush(stdout);\n    calc_eval(NULL, \"1\", &value);\n    printf(\"after\\n\");\n    return 0;\n}\n").unwrap();
    let panics = program_at(&work, "panics");
    let static_dir = work.join("static");
    cc(&[static_define, "-I".as_ref(), static_dir.as_os_str(), "-o".as_ref(), panics.as_os_str(), panics_c.as_os_str(), static_dir.join(if cfg!(windows) { "calclib.lib" } else { "libcalclib.a" }).as_os_str()]);
    let ran = std::process::Command::new(&panics).output().unwrap();
    assert_eq!(printed(&ran.stdout), "before\n");
    // The message goes through C's `stderr`, which the program of C keeps as text on Windows.
    let said = printed(&ran.stderr);
    assert!(said.starts_with("panic: C gave `null` to `calc_eval`, which takes a `ref Calculator`\n") && said.contains("api.sg:"), "{said}");
    // `abort`: the signal 6 on Linux and macOS; on Windows C's library ends the process at once
    // (`__fastfail`), with 0xC0000409, as a panic of Rust that aborts does.
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(ran.status.signal(), Some(6), "a panic in a library aborts: {:?}", ran.status);
    }
    #[cfg(windows)]
    assert_eq!(ran.status.code(), Some(0xC000_0409_u32 as i32), "a panic in a library aborts: {:?}", ran.status);

    // What the checker asks of a library.
    for (name, text, message) in [
        ("main.sg", "export int one() { return 1 }\nvoid main() { }\n", "error: a library has no `main`: C calls its `export` functions"),
        ("none.sg", "int one() { return 1 }\n", "error: a library gives C its `export` functions, and this one has none"),
        ("word.sg", "struct union {\n    List<int> items\n}\nexport union* make() { return new union { items = List<int>() } }\n", "is a word of C"),
        ("plain.sg", "struct Point {\n    int x\n}\nexport Point* make() { return new Point { x = 1 } }\n", "error: C calls `make`, and cannot take a `Point*` from it"),
        ("borrow.sg", "struct Box {\n    List<int> items\n}\nexport read Box give(read Box b) from b { return b }\n", "give C the value itself: `Box*`"),
    ] {
        let file = work.join(name);
        std::fs::write(&file, text).unwrap();
        let made = std::process::Command::new(sg_bin).args(["build", name, "--library"]).current_dir(&work).output().unwrap();
        let said = String::from_utf8_lossy(&made.stderr);
        assert!(!made.status.success() && said.contains(message), "{name}: {said}");
    }
    let clash = work.join("clash");
    std::fs::create_dir_all(clash.join("a")).unwrap();
    std::fs::create_dir_all(clash.join("b")).unwrap();
    std::fs::write(clash.join("a/a.sg"), "public struct Item {\n    public List<int> items\n}\n").unwrap();
    std::fs::write(clash.join("b/b.sg"), "public enum Item {\n    One\n}\n").unwrap();
    std::fs::write(clash.join("lib.sg"), "use a\nuse b\nexport a.Item* first() { return new a.Item { items = List<int>() } }\nexport b.Item* second() { return new b.Item.One }\n").unwrap();
    let made = std::process::Command::new(sg_bin).args(["build", ".", "--library"]).current_dir(&clash).output().unwrap();
    let said = String::from_utf8_lossy(&made.stderr);
    assert!(!made.status.success() && said.contains("the header of the library would name two types `Item`"), "{said}");
}

/// The names a library built from SG lets others see: the functions it defines, by `nm`, or on
/// Windows by LLVM's `llvm-nm` for a static library (without what Microsoft's headers define
/// inline) and `llvm-readobj` for what a DLL exports.
fn seen_from_outside(file: &Path, shared: bool) -> Vec<String> {
    let tool = |name: &str| -> std::ffi::OsString {
        if !cfg!(windows) {
            return name.into();
        }
        let beside = sg::cc::compiler().ok().and_then(|cc| Path::new(&cc).parent().map(|dir| dir.join(format!("{name}.exe")))).filter(|path| path.exists());
        beside.map_or_else(|| name.into(), PathBuf::into_os_string)
    };
    if cfg!(windows) && shared {
        let read = std::process::Command::new(tool("llvm-readobj")).arg("--coff-exports").arg(file).output().unwrap();
        assert!(read.status.success(), "llvm-readobj: {}", String::from_utf8_lossy(&read.stderr));
        return String::from_utf8_lossy(&read.stdout).lines().filter_map(|line| line.trim().strip_prefix("Name: ")).map(str::to_string).collect();
    }
    let names = std::process::Command::new(tool(if cfg!(windows) { "llvm-nm" } else { "nm" })).arg("-g").arg(file).output().unwrap();
    assert!(names.status.success(), "nm: {}", String::from_utf8_lossy(&names.stderr));
    let listed = String::from_utf8_lossy(&names.stdout).into_owned();
    let defined = listed.lines().filter(|line| line.contains(" T ")).map(|line| line.rsplit(' ').next().unwrap().trim_start_matches('_').to_string());
    // Microsoft's headers of C define `printf` and its kin inline, and every object that calls
    // one carries them, each once in the program however many objects do: not names of SG.
    let microsofts = |name: &String| cfg!(windows) && (name.contains("printf") || name.contains("scanf") || name.starts_with("local_stdio"));
    defined.filter(|name| !microsofts(name)).collect()
}

/// The interpreter keeps the values it gave C by their address, and checks what C gives back:
/// an address it did not give, one of another type, one given back already, one lent in a way
/// the rules of borrowing forbid, and `null` where none is taken are panics. Compiled code
/// checks `null` only.
#[test]
fn handles_misused() {
    const PROGRAM: &str = r#"
extern "./native.c" {
    void misuse(int how)
}

struct Tally {
    List<int> seen
}

struct Other {
    List<int> seen
}

export Tally* tally_new() {
    return new Tally { seen = List<int>() }
}

export Other* other_new() {
    return new Other { seen = List<int>() }
}

export void tally_add(ref Tally tally, int n) {
    tally.seen.add(n)
}

export nint tally_count(read Tally tally) {
    return tally.seen.length()
}

// Calls C back while it changes the tally.
export void tally_each(ref Tally tally, CFunc<raw void, void> visit, raw void data) {
    tally.seen.add(0)
    unsafe visit(data)
}

export void tally_free(Tally* tally) {
    _ = tally
}

export nint text_length(read CString text) {
    var made = string.fromCString(text) ?? ""
    return made.length()
}

void main(List<string> args) {
    unsafe misuse(int.parse(args[0]) ?? 0)
    print("no panic")
}
"#;
    const NATIVE: &str = r#"
#include <stddef.h>
static void *kept = NULL;
static void reads(void *tally) { (void)tally_count(tally); }
static void frees(void *tally) { tally_free(tally); }
static void misuse(int32_t how) {
    void *tally = tally_new();
    int local = 0;
    switch (how) {
    case 0: tally_add(tally, 1); tally_free(tally); break;
    case 1: tally_free(tally); tally_add(tally, 1); break;
    case 2: tally_add(&local, 1); break;
    case 3: tally_add(other_new(), 1); break;
    case 4: tally_each(tally, reads, tally); break;
    case 5: kept = tally; tally_each(tally, frees, tally); break;
    case 6: tally_free(NULL); break;
    case 7: (void)text_length(NULL); break;
    }
}
"#;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target").join("handles-misused");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("main.sg"), PROGRAM).unwrap();
    std::fs::write(work.join("native.c"), NATIVE).unwrap();
    let cases: [(&str, &str); 8] = [
        ("0", "no panic\n"),
        ("1", "panic: C gave `tally_add` an address that holds no `Tally`\n"),
        ("2", "panic: C gave `tally_add` an address that holds no `Tally`\n"),
        ("3", "panic: C gave `tally_add` a `Other` where it takes a `Tally`\n"),
        ("4", "panic: C gave `tally_count` a `Tally` that a call of `tally_each` in progress changes\n"),
        ("5", "panic: C gave `tally_free` a `Tally` that a call of `tally_each` in progress changes\n"),
        ("6", "panic: C gave `null` to `tally_free`, which takes a `Tally*`\n"),
        ("7", "panic: C gave `null` to `text_length`, which takes a `read CString`\n"),
    ];
    for (how, said) in cases {
        let ran = std::process::Command::new(env!("CARGO_BIN_EXE_sg")).args(["main.sg", how]).current_dir(&work).output().unwrap();
        let all = format!("{}{}", String::from_utf8_lossy(&ran.stdout), String::from_utf8_lossy(&ran.stderr));
        assert!(all.starts_with(said), "case {how}: {all}");
        assert_eq!(ran.status.code(), Some(if how == "0" { 0 } else { 1 }), "case {how}: {all}");
    }
    // Compiled, the program checks `null` alone, with the same message and its hint.
    let sources = sg::load(&work.join("main.sg"), false).unwrap();
    let (c_file, binary) = (work.join("out.c"), program_at(&work, "out"));
    std::fs::write(&c_file, sg::emit_c_sources(&sources, true).unwrap()).unwrap();
    let cc = c_command().args(["-std=c11", "-O1"]).args(warnings("")).arg("-o").arg(&binary).arg(&c_file).args(system_libs(&work)).output().unwrap();
    assert!(cc.status.success(), "{}", String::from_utf8_lossy(&cc.stderr));
    for (how, said) in [cases[0], cases[6], cases[7]] {
        let ran = std::process::Command::new(&binary).arg(how).output().unwrap();
        let all = format!("{}{}", String::from_utf8_lossy(&ran.stdout), String::from_utf8_lossy(&ran.stderr));
        assert!(all.starts_with(said), "compiled case {how}: {all}");
        if how == "6" {
            assert!(all.contains("hint: a parameter C may give `null` is written `Tally*?`"), "{all}");
        }
    }
}

/// `-L folder`: the libraries of the `extern` blocks are looked for there when a program is
/// linked and when it starts, by the interpreter, a built program and the program of tests; of
/// the names a block gives, the first found is linked. A folder that is not there, or that
/// others may write into, is an error. The program of tests starts itself again for each test
/// from the path the system gives, also when it was started by a name found on `PATH`.
#[test]
fn library_folders() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sg_bin = env!("CARGO_BIN_EXE_sg");
    let work = root.join("target").join("library-folders");
    let _ = std::fs::remove_dir_all(&work);
    let lib = work.join("lib");
    let bin = work.join("bin");
    let elsewhere = work.join("elsewhere");
    for dir in [&lib, &bin, &elsewhere] {
        std::fs::create_dir_all(dir).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    std::fs::write(lib.join("folder.c"), "#ifdef _WIN32\n__declspec(dllexport)\n#endif\nint sgfolder_twice(int n) { return n * 2; }\n").unwrap();
    // On Windows a DLL, `sgfolder.dll`, and the import library a program links, `sgfolder.lib`.
    let file = if cfg!(windows) {
        "sgfolder.dll"
    } else if cfg!(target_os = "macos") {
        "libsgfolder.dylib"
    } else {
        "libsgfolder.so"
    };
    let mut cc = c_command();
    cc.arg("-shared");
    if !cfg!(windows) {
        cc.arg("-fPIC");
    }
    cc.arg("-o").arg(lib.join(file)).arg(lib.join("folder.c"));
    if cfg!(target_os = "macos") {
        cc.arg("-Wl,-install_name,@rpath/libsgfolder.dylib");
    }
    let made = cc.output().unwrap();
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let block = "extern \"sgfolder_none\", \"sgfolder\" {\n    int sgfolder_twice(int n)\n}\n";
    std::fs::write(work.join("p.sg"), format!("{block}\nvoid main() {{\n    print(unsafe sgfolder_twice(21))\n}}\n")).unwrap();
    std::fs::write(work.join("t.sg"), format!("{block}\ntest \"twice\" {{\n    assert(unsafe sgfolder_twice(21) == 42)\n}}\n")).unwrap();
    let sg = |args: &[&str]| std::process::Command::new(sg_bin).args(args).current_dir(&work).env_remove("LD_LIBRARY_PATH").env_remove("DYLD_LIBRARY_PATH").output().unwrap();
    let said = |run: &std::process::Output| format!("{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));

    // The interpreter, both ways of writing it.
    for args in [&["-L", "lib", "p.sg"][..], &["-Llib", "p.sg"][..]] {
        let run = sg(args);
        assert_eq!(said(&run), "42\n", "sg {args:?}");
    }
    // A built program finds the library where it starts, without `LD_LIBRARY_PATH`; on Windows,
    // which has no `rpath`, the DLL is copied beside it.
    let run = sg(&["build", "p.sg", "-L", "lib", "-o", "bin/p"]);
    assert!(run.status.success(), "{}", said(&run));
    if cfg!(windows) {
        assert!(said(&run).contains("sgfolder.dll beside the program") && bin.join("sgfolder.dll").exists(), "{}", said(&run));
        let again = sg(&["build", "p.sg", "-L", "lib", "-o", "bin/p"]);
        assert!(again.status.success() && !said(&again).contains("copied"), "the same DLL is not copied twice: {}", said(&again));
    }
    let run = std::process::Command::new(program_at(&bin, "p")).current_dir(&elsewhere).env_remove("LD_LIBRARY_PATH").env_remove("DYLD_LIBRARY_PATH").output().unwrap();
    assert_eq!(said(&run), "42\n");
    // The tests, interpreted and compiled.
    for args in [&["test", "-L", "lib", "t.sg"][..], &["test", "t.sg", "--compiled", "-L", "lib"][..]] {
        let run = sg(args);
        assert!(run.status.success() && said(&run).contains("ok    twice"), "sg {args:?}: {}", said(&run));
    }
    // The program of tests started by a name found on `PATH`, from another folder.
    let run = sg(&["build", "t.sg", "--test", "-L", "lib", "-o", "bin/tests-of-folders"]);
    assert!(run.status.success(), "{}", said(&run));
    let run = std::process::Command::new("tests-of-folders").current_dir(&elsewhere).env("PATH", &bin).output().unwrap();
    assert!(run.status.success() && said(&run).contains("ok    twice"), "{}", said(&run));

    // Without the folder neither name is found, and the error says so at the block.
    let run = sg(&["p.sg"]);
    assert!(said(&run).contains("none of the libraries this `extern` block names was found: `sgfolder_none`, `sgfolder`"), "{}", said(&run));
    let run = sg(&["build", "p.sg", "-o", "bin/q"]);
    assert!(!run.status.success() && said(&run).contains("none of the libraries"), "{}", said(&run));
    // A folder that is not there, and on Linux and macOS one others may write into.
    let run = sg(&["-L", "nowhere", "p.sg"]);
    assert_eq!((run.status.code(), said(&run)), (Some(2), "error: -L nowhere: no such folder\n".to_string()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let open = work.join("open");
        std::fs::create_dir_all(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
        let run = sg(&["build", "p.sg", "-L", "open", "-o", "bin/q"]);
        assert!(run.status.code() == Some(2) && said(&run).contains("-L open: others may write into this folder"), "{}", said(&run));
    }
    // `emit-c` links nothing, but says on its first line which of the names it found there.
    let run = sg(&["emit-c", "p.sg", "-L", "lib"]);
    let first = String::from_utf8_lossy(&run.stdout).lines().next().unwrap_or_default().to_string();
    assert!(run.status.success() && first.contains("-lsgfolder") && !first.contains("sgfolder_none"), "{first}{}", said(&run));
    let run = sg(&["emit-c", "p.sg"]);
    assert!(!run.status.success() && said(&run).contains("none of the libraries"), "{}", said(&run));
}

/// The count of blocks a debug build keeps, tried on programs made to leak: a block taken and
/// never given back, one given back that was not taken, and a block a test leaves. A count that
/// does not see them must not let the programs pass. A release build has no count.
#[test]
fn block_count_sees_a_leak() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target").join("block-count");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let program = "void main() {\n    var words = List<string>()\n    words.add(\"one\".repeat(3))\n    print(words.length())\n}\n";
    let tests = "test \"a test\" {\n    var words = List<string>()\n    words.add(\"one\".repeat(3))\n    assert(words.length() == 1)\n}\n";
    let check = "#if SG_COUNT\n    if (sg_blocks_left(\"the program\", 0)) return 1;";
    let cases = [
        ("taken", program, false, check, format!("    (void)malloc(24);\n{check}"), "leak: the program ended with 1 block of memory never given back"),
        ("given", program, false, check, format!("    free((malloc)(24));\n{check}"), "leak: the program gave back 1 block of memory more than it took"),
        ("test", tests, true, "    test->run();\n", "    test->run();\n    (void)malloc(24);\n".to_string(), "leak: the test ended with 1 block of memory never given back"),
    ];
    for (name, source, tests, find, put, report) in cases {
        let sources = sg::Sources::single(source, &format!("{name}.sg"));
        let code = match tests {
            true => sg::emit_c_tests(&sources, true),
            false => sg::emit_c_sources(&sources, true),
        }
        .unwrap();
        assert!(code.contains(find), "{name}: the place to leak at is not in the C");
        let c_file = work.join(format!("{name}.c"));
        std::fs::write(&c_file, code.replacen(find, &put, 1)).unwrap();
        let binary = program_at(&work, name);
        let cc = c_command().args(["-std=c11", "-o"]).arg(&binary).arg(&c_file).args(system_libs(&work)).output().unwrap();
        assert!(cc.status.success(), "{name}: {}", String::from_utf8_lossy(&cc.stderr));
        let run = std::process::Command::new(&binary).output().unwrap();
        let said = format!("{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
        assert!(!run.status.success() && said.contains(report) && said.contains("note: safe SG gives back every block it takes, so this is a mistake in sg"), "{name}: the count of blocks does not see a leak that is there, so it cannot be trusted:\n{said}");
    }
    let release = sg::emit_c_sources(&sg::Sources::single(program, "release.sg"), false).unwrap();
    assert!(release.contains("#define SG_COUNT 0\n"));
}

/// A delay waits about as long as it was asked for, also a short one: fifty delays of one
/// millisecond end well within half a second. A clock that wakes only at the ticks of the
/// system's timer (about 16 ms on Windows, where `Task.delay` takes a timer of high resolution)
/// would take some 800 ms. Each delay waits at least its millisecond.
/// A const found when the program is compiled takes at most 100 000 000 steps, so that every
/// machine stops at the same place. Here, apart from the cases: reaching the end takes seconds,
/// which the cases would spend once for the interpreter and once more for the back end.
#[test]
fn const_steps_run_out() {
    let source = "int spin() {\n    var n = 0\n    while n >= 0 {\n        n = n + 1 - 1\n    }\n    return n\n}\n\nconst int Spun = spin()\n\nvoid main() {\n    print(Spun)\n}\n";
    let error = sg::emit_c(source, "spin.sg", true).expect_err("a const that never ends is refused");
    assert_eq!(error.msg, "finding the value of `Spun` takes more than 100 000 000 steps");
    assert_eq!(&source[error.span.start..error.span.end], "spin()");
}

/// A program nests at most 1000 deep (choice 1342), and one nested far deeper is that error,
/// found quickly and in little memory: without the bound the parser, the checker or the reading
/// of the module names (`a.b.b...`) overflowed the stack, took minutes or gigabytes.
#[test]
fn deep_sources_are_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target").join("deep-sources");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let n = 100_000;
    let sources = [
        ("parens", format!("void main() {{\n    var x = {}1{}\n    print(x)\n}}\n", "(".repeat(n), ")".repeat(n))),
        ("unary", format!("void main() {{\n    var x = {}1\n    print(x)\n}}\n", "-".repeat(n))),
        ("blocks", format!("void main() {{\n{}{}}}\n", "if true {\n".repeat(n), "}\n".repeat(n))),
        ("chain", format!("void main() {{\n    var x = 1{}\n    print(x)\n}}\n", " + 1".repeat(n))),
        ("calls", format!("int f(int x) {{\n    return x\n}}\n\nvoid main() {{\n    var x = {}1{}\n    print(x)\n}}\n", "f(".repeat(n), ")".repeat(n))),
        ("types", format!("void main() {{\n    {}int{} x = null\n}}\n", "List<".repeat(n), ">".repeat(n))),
        ("members", format!("void main() {{\n    var x = a{}\n}}\n", ".b".repeat(n))),
    ];
    for (name, source) in sources {
        let path = work.join(format!("{name}.sg"));
        std::fs::write(&path, &source).unwrap();
        let started = std::time::Instant::now();
        let loaded = sg::load(&path, false).unwrap();
        let error = sg::emit_c_sources(&loaded, true).expect_err("a program nested 100 000 deep is refused");
        assert_eq!(error.msg, "this is nested more than 1000 deep", "{name}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10), "{name}: refusing it took {:?}", started.elapsed());
    }
}

/// A function whose values take more than the stack of its thread (choices 1344 and 1345) ends
/// at the guard page below the stack. Without the touching of `sg_probe_stack` its frame was
/// made all at once below the guard, and `smash` wrote into memory that is not this thread's,
/// and the program went on: on Linux the stacks of threads lie next to each other. The large
/// array is in a branch that never runs; C makes the whole frame when the function starts.
#[test]
fn large_frames_touch_their_stack() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target").join("large-frames");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    // `first` starts on a thread of its own, then `second` on the next, whose stack lies below.
    // `second` keeps an array on its stack while `first` calls `victim`.
    let source = "long smash() {\n    var marks = long[8192](4702111234474983745)\n    long sum = 0\n    for m in read marks {\n        sum += m % 7\n    }\n    return sum\n}\n\nlong victim(bool never, int at) {\n    var sum = smash()\n    if never {\n        var big = byte[SIZE]\n        big[at] = 1\n        sum += (long)big[at / 2]\n    }\n    return sum\n}\n\nlong first(Channel<int> go, Sender<int> done, bool never, int at) {\n    _ = go.receive()\n    var result = victim(never, at)\n    _ = done.send(1)\n    return result\n}\n\nlong second(Sender<int> go, Channel<int> done) {\n    var mine = long[400000](1)\n    _ = go.send(1)\n    _ = done.receive()\n    long sum = 0\n    for m in read mine {\n        sum += m\n    }\n    return sum\n}\n\nvoid main(List<string> args) {\n    var go = Channel<int>(1)\n    var done = Channel<int>(1)\n    var goSender = go.sender()\n    var doneSender = done.sender()\n    var a = Task.run(first, go, doneSender, args.length() > 5, (int)args.length() + 100)\n    var b = Task.run(second, goSender, done)\n    var damaged = b.wait()\n    _ = a.wait()\n    print($\"400000 expected, {damaged} found\")\n}\n";
    for (size, ends) in [("16", true), ("10000000", false)] {
        let code = sg::emit_c_sources(&sg::Sources::single(&source.replace("SIZE", size), "frames.sg"), false).unwrap();
        let c_file = work.join(format!("frames{size}.c"));
        std::fs::write(&c_file, code).unwrap();
        let binary = program_at(&work, &format!("frames{size}"));
        let cc = c_command().args(["-std=c11", "-O2", "-o"]).arg(&binary).arg(&c_file).args(system_libs(&work)).output().unwrap();
        assert!(cc.status.success(), "{}", String::from_utf8_lossy(&cc.stderr));
        let run = std::process::Command::new(&binary).output().unwrap();
        let said = String::from_utf8_lossy(&run.stdout).into_owned();
        match ends {
            true => assert!(run.status.success() && said.trim() == "400000 expected, 400000 found", "a small frame: {said}{}", String::from_utf8_lossy(&run.stderr)),
            false => assert!(!run.status.success() && said.is_empty(), "a frame of 10 MB on a stack of 8 MB went on: {said}"),
        }
    }
}

#[test]
fn short_delays_are_short() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target").join("short-delays");
    std::fs::create_dir_all(&work).unwrap();
    let source = "async void main() {\n    var start = Instant.now()\n    for _ in 0..50 {\n        await Task.delay(1)\n    }\n    var waited = start.elapsed()\n    print(waited)\n    print(waited >= Duration.fromMilliseconds(50))\n    print(waited < Duration.fromMilliseconds(500))\n}\n";
    let code = sg::emit_c_sources(&sg::Sources::single(source, "delays.sg"), false).unwrap();
    let c_file = work.join("delays.c");
    std::fs::write(&c_file, code).unwrap();
    let binary = program_at(&work, "delays");
    let cc = c_command().args(["-std=c11", "-O1", "-o"]).arg(&binary).arg(&c_file).args(system_libs(&work)).output().unwrap();
    assert!(cc.status.success(), "{}", String::from_utf8_lossy(&cc.stderr));
    let run = std::process::Command::new(&binary).output().unwrap();
    let said = String::from_utf8_lossy(&run.stdout).into_owned();
    let lines: Vec<&str> = said.lines().collect();
    assert!(run.status.success() && lines.len() == 3, "{said}{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(lines[1], "true", "fifty delays of 1 ms took {}, less than 50 ms", lines[0]);
    assert_eq!(lines[2], "true", "fifty delays of 1 ms took {}: the clock of delays wakes late", lines[0]);
}

/// On Windows a listener and a datagram socket hold their port alone (`SO_EXCLUSIVEADDRUSE`): one
/// bound to every address of the machine (`":0"`) does not have the port of `127.0.0.1` taken by
/// another socket that asks to share it (`SO_REUSEADDR`), which Windows would otherwise let take it
/// and receive what is sent there, whether the program is interpreted or compiled.
#[cfg(windows)]
#[test]
fn ports_are_held_alone() {
    use std::io::{BufRead, Write};

    #[link(name = "ws2_32")]
    extern "system" {
        fn socket(family: i32, kind: i32, protocol: i32) -> usize;
        fn setsockopt(s: usize, level: i32, name: i32, value: *const u8, len: i32) -> i32;
        fn bind(s: usize, name: *const u8, len: i32) -> i32;
        fn closesocket(s: usize) -> i32;
    }
    // Whether a socket that asks to share the port (`SO_REUSEADDR`) may bind 127.0.0.1:`port`, while
    // another socket has the port on every address.
    let shared_bind = |kind: i32, port: u16| -> bool {
        // Winsock is started by std's first socket.
        drop(std::net::UdpSocket::bind("127.0.0.1:0").unwrap());
        // SAFETY: plain calls of Winsock on a socket made and closed here.
        unsafe {
            let s = socket(2, kind, 0);
            assert_ne!(s, !0, "no socket");
            let one: i32 = 1;
            setsockopt(s, 0xffff, 4, (&one as *const i32).cast(), 4);
            let mut at = [0u8; 16];
            at[0] = 2;
            at[2..4].copy_from_slice(&port.to_be_bytes());
            at[4..8].copy_from_slice(&[127, 0, 0, 1]);
            let bound = bind(s, at.as_ptr(), 16) == 0;
            closesocket(s);
            bound
        }
    };
    // What the check sees: a socket of std, which does not hold its port alone, is taken.
    let plain = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
    assert!(shared_bind(2, plain.local_addr().unwrap().port()), "a socket that asks to share the port should take one not held alone");
    drop(plain);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target").join("ports-alone");
    std::fs::create_dir_all(&work).unwrap();
    let source = "Result<void, IoError> main() {\n    var listener = try TcpListener.bind(\":0\")\n    var datagrams = try UdpSocket.bind(\":0\")\n    print(listener.port())\n    print(datagrams.port())\n    try Stdout.flush()\n    var line = \"\"\n    _ = try Stdin.readLine(ref line)\n    return Ok()\n}\n";
    let file = work.join("ports.sg");
    std::fs::write(&file, source).unwrap();
    let code = sg::emit_c_sources(&sg::Sources::single(source, "ports.sg"), false).unwrap();
    let c_file = work.join("ports.c");
    std::fs::write(&c_file, code).unwrap();
    let binary = program_at(&work, "ports");
    let cc = c_command().args(["-std=c11", "-O1", "-o"]).arg(&binary).arg(&c_file).args(system_libs(&work)).output().unwrap();
    assert!(cc.status.success(), "{}", String::from_utf8_lossy(&cc.stderr));
    let ways = [("interpreted", std::process::Command::new(env!("CARGO_BIN_EXE_sg")).arg(&file).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap()),
        ("compiled", std::process::Command::new(&binary).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap())];
    for (how, mut child) in ways {
        let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut port = |what: &str| -> u16 {
            let mut line = String::new();
            out.read_line(&mut line).unwrap();
            line.trim().parse().unwrap_or_else(|_| panic!("{how}: no port of the {what}: {line:?}"))
        };
        let (listener, datagrams) = (port("listener"), port("datagram socket"));
        assert!(!shared_bind(1, listener), "{how}: another socket took the port of the listener");
        assert!(!shared_bind(2, datagrams), "{how}: another socket took the port of the datagram socket");
        child.stdin.take().unwrap().write_all(b"\n").unwrap();
        assert!(child.wait().unwrap().success(), "{how}: the program failed");
    }
}
