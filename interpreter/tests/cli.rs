//! The `sg` command from outside (choice 1322): each command and flag on a path that works, what
//! it writes and where, and its exit code; and the ways it fails, each with its code. What the
//! programs themselves do is tested in `golden.rs`; here only the command around them, and the codes and the
//! streams of a program that reads its command line with `parseOrExit` (choice 1429).
//!
//! The exit codes: 0 when all went well; what `exit` gives; 1 for an error the checker or the C
//! compiler finds, a panic, or a failed test; 2 for a command line that is not one of the usages, a
//! folder of `-L` that is not there, a program that cannot be read, an output into a folder that
//! is not there, and an output that is a folder. An output closed before the end (`sg x.sg | head`,
//! `sg emit-c x.sg | head`) ends the program or the tool by `SIGPIPE` on Linux and macOS, with 0 on
//! Windows; an output that cannot be written (a full disk) ends a program with a message and 1. A
//! standard error closed before the end ends the tool's message the same way (1604), and a
//! program's as compiled code ends it: a panic with 1, `main`'s error by `SIGPIPE`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A folder of its own for each test, made new: the tests run at once.
fn work(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("cli").join(name);
    removed(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A folder taken away with what is in it, the folders of packages the cache keeps read only too
/// (choice 1591) made writable first.
fn removed(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut folders = vec![dir.to_path_buf()];
        while let Some(folder) = folders.pop() {
            let Ok(meta) = std::fs::symlink_metadata(&folder) else { continue };
            if !meta.is_dir() {
                continue;
            }
            let _ = std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755));
            if let Ok(read) = std::fs::read_dir(&folder) {
                folders.extend(read.flatten().map(|entry| entry.path()));
            }
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

/// `sg` with these words, in `dir`.
fn sg(dir: &Path, words: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sg")).args(words).current_dir(dir).stdin(Stdio::null()).output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The exit code, and what was written to the output and to standard error.
fn ran(out: &Output) -> (i32, String, String) {
    (out.status.code().unwrap_or(-1), text(&out.stdout), text(&out.stderr))
}

/// Whether there is a C compiler: the commands that compile are tried only then.
fn has_compiler() -> bool {
    let found = sg::cc::compiler().is_ok();
    if !found {
        eprintln!("no C compiler found: the commands that compile are not tested");
    }
    found
}

/// A program built in `dir`: on Windows with `.exe`.
fn program(dir: &Path, stem: &str) -> PathBuf {
    PathBuf::from(sg::cc::program_file(&dir.join(stem).to_string_lossy()))
}

const HELLO: &str = "void main() {\n    print(\"hello\")\n}\n";
const WORDS: &str = "void main(List<string> args) {\n    for word in args {\n        print(word)\n    }\n}\n";
const EXIT: &str = "void main() {\n    exit(7)\n}\n";
const PANICS: &str = "void main() {\n    var xs = int[2](0)\n    var i = 5\n    print(xs[i])\n}\n";
const WRONG: &str = "void main() {\n    print(nope)\n}\n";
const TESTS: &str = "test \"adds\" {\n    assert(1 + 1 == 2)\n}\n\ntest \"multiplies\" {\n    assert(2 * 3 == 6)\n}\n";
const FAILS: &str = "test \"adds\" {\n    assert(1 + 1 == 2)\n}\n\ntest \"is wrong\" {\n    assert(1 == 2)\n}\n";
const LIBRARY: &str = "export int add(int a, int b) {\n    return a + b\n}\n";
const MANY: &str = "void main() {\n    for i in 0..1000000 {\n        print(i)\n    }\n}\n";

#[test]
fn runs_a_program() {
    let dir = work("runs_a_program");
    write(&dir, "hello.sg", HELLO);
    write(&dir, "words.sg", WORDS);
    assert_eq!(ran(&sg(&dir, &["hello.sg"])), (0, "hello\n".into(), String::new()));
    // The words after the program are its own, flags of `sg` among them.
    assert_eq!(ran(&sg(&dir, &["words.sg", "one", "--release", "-L", "two words"])), (0, "one\n--release\n-L\ntwo words\n".into(), String::new()));
    // `-L` before the program, as two words or one; the folder must be there.
    std::fs::create_dir_all(dir.join("libs")).unwrap();
    assert_eq!(ran(&sg(&dir, &["-L", "libs", "words.sg", "a"])), (0, "a\n".into(), String::new()));
    assert_eq!(ran(&sg(&dir, &["-Llibs", "words.sg", "a"])), (0, "a\n".into(), String::new()));
    // A folder is a program of every file in it.
    let folder = dir.join("app");
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "main.sg", HELLO);
    assert_eq!(ran(&sg(&dir, &["app"])), (0, "hello\n".into(), String::new()));
}

#[test]
fn exit_codes_of_a_run() {
    let dir = work("exit_codes_of_a_run");
    write(&dir, "exit.sg", EXIT);
    write(&dir, "panics.sg", PANICS);
    write(&dir, "wrong.sg", WRONG);
    std::fs::write(dir.join("bytes.sg"), b"\xff\xfe").unwrap();
    std::fs::create_dir_all(dir.join("empty")).unwrap();
    assert_eq!(ran(&sg(&dir, &["exit.sg"])).0, 7);
    let (code, out, err) = ran(&sg(&dir, &["panics.sg"]));
    assert_eq!((code, out.as_str()), (1, ""));
    assert!(err.starts_with("panic: index 5 is out of range: the length is 2\n  --> panics.sg:4:11"), "{err}");
    let (code, _, err) = ran(&sg(&dir, &["wrong.sg"]));
    assert_eq!(code, 1);
    assert!(err.starts_with("error: there is no variable named `nope`\n  --> wrong.sg:2:11"), "{err}");
    // A program that cannot be read, and a command line that is not one of the usages: 2.
    let (code, _, err) = ran(&sg(&dir, &["missing.sg"]));
    assert_eq!(code, 2);
    assert!(err.starts_with("error: cannot read missing.sg"), "{err}");
    let (code, _, err) = ran(&sg(&dir, &["bytes.sg"]));
    assert_eq!(code, 2);
    assert!(err.starts_with("error: cannot read bytes.sg") && err.contains("UTF-8"), "{err}");
    assert_eq!(ran(&sg(&dir, &["empty"])), (2, String::new(), "error: there is no `.sg` file in empty\n".into()));
    let (code, _, err) = ran(&sg(&dir, &[]));
    assert_eq!(code, 2);
    assert!(err.starts_with("usage: sg "), "{err}");
    assert_eq!(ran(&sg(&dir, &["-L"])), (2, String::new(), "error: -L names a folder: `-L folder`\n".into()));
    let (code, _, err) = ran(&sg(&dir, &["-L", "nowhere", "exit.sg"]));
    assert_eq!(code, 2);
    assert!(err.starts_with("error: -L nowhere"), "{err}");
}

/// An output closed before the program ends, as `head` closes it, ends it the same way interpreted
/// and compiled (1330): by `SIGPIPE` on Linux and macOS, as C's and Go's programs end; with 0 on
/// Windows, which has no such signal.
#[test]
fn an_output_closed_early() {
    use std::io::BufRead;
    let dir = work("an_output_closed_early");
    write(&dir, "many.sg", MANY);
    let mut ways = vec![(Command::new(env!("CARGO_BIN_EXE_sg")), "interpreted")];
    ways[0].0.arg("many.sg");
    if has_compiler() {
        assert_eq!(sg(&dir, &["build", "many.sg"]).status.code(), Some(0));
        ways.push((Command::new(program(&dir, "many")), "compiled"));
    }
    for (mut command, way) in ways {
        let mut child = command.current_dir(&dir).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let mut first = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut first).unwrap();
        assert_eq!(first, "0\n", "{way}");
        let out = child.wait_with_output().unwrap();
        assert_eq!((text(&out.stdout), text(&out.stderr)), (String::new(), String::new()), "{way}");
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!((out.status.code(), out.status.signal()), (None, Some(13)), "{way}");
        }
        #[cfg(windows)]
        assert_eq!(out.status.code(), Some(0), "{way}");
    }
}

/// The programs of the tests below, each run interpreted and, with a C compiler, compiled: the
/// two must end the same way.
#[cfg(unix)]
fn both_ways(dir: &Path, programs: &[(&str, &str)]) -> Vec<(Command, String)> {
    let mut ways = Vec::new();
    for (stem, source) in programs {
        write(dir, &format!("{stem}.sg"), source);
        let mut interpreted = Command::new(env!("CARGO_BIN_EXE_sg"));
        interpreted.arg(format!("{stem}.sg")).current_dir(dir);
        ways.push((interpreted, format!("{stem} interpreted")));
    }
    if has_compiler() {
        for (stem, _) in programs {
            assert_eq!(sg(dir, &["build", &format!("{stem}.sg")]).status.code(), Some(0), "{stem}");
            let mut compiled = Command::new(program(dir, stem));
            compiled.current_dir(dir);
            ways.push((compiled, format!("{stem} compiled")));
        }
    }
    ways
}

/// An output that cannot be written, as a full disk (`/dev/full`), ends the program with a
/// message and 1, interpreted and compiled (1547): at `print`, before `printError`, and at the
/// end, where what is shorter than the block of the output is written first, after `exit` too.
/// `Stdout` gives its error in its `Result`, and the program goes on. A `main` that ends with an
/// error has it told first, then the output's (1603).
#[cfg(target_os = "linux")]
#[test]
fn an_output_that_cannot_be_written() {
    let dir = work("an_output_that_cannot_be_written");
    let full = "error: cannot write output: No space left on device (os error 28)\n";
    let programs = [
        ("short", HELLO),
        ("long", "void main() {\n    for i in 0..20000 {\n        print($\"line {i} of many\")\n    }\n    printError(\"never\")\n}\n"),
        ("exits", "void main() {\n    print(\"short\")\n    exit(3)\n}\n"),
        ("error", "void main() {\n    print(\"before\")\n    printError(\"never\")\n}\n"),
        ("fails", "Result<void, string> main() {\n    print(\"before\")\n    return Error(\"it failed\")\n}\n"),
        ("result", "void main() {\n    if Stdout.writeLine(\"x\".repeat(70000)) is Error(cause) {\n        printError($\"write: {cause}\")\n    }\n    if Stdout.flush() is Error(cause) {\n        printError($\"flush: {cause}\")\n    }\n    printError(\"goes on\")\n}\n"),
    ];
    for (mut command, way) in both_ways(&dir, &programs) {
        let out = command.stdout(std::fs::File::options().write(true).open("/dev/full").unwrap()).output().unwrap();
        let (code, _, err) = ran(&out);
        match way.split(' ').next() {
            Some("result") => assert_eq!((code, err.as_str()), (0, "write: no space left\nflush: no space left\ngoes on\n"), "{way}"),
            Some("fails") => assert_eq!((code, err), (1, format!("error: it failed\n{full}")), "{way}"),
            _ => assert_eq!((code, err.as_str()), (1, full), "{way}"),
        }
    }
}

/// An output that was never open (`>&-`) takes what is written and drops it, interpreted and
/// compiled (1547), as Rust's own output does: the program goes on to its end.
#[cfg(unix)]
#[test]
fn an_output_never_open() {
    let dir = work("an_output_never_open");
    let programs = [("many", "void main() {\n    for i in 0..20000 {\n        print(i)\n    }\n    printError(\"the end\")\n}\n")];
    for (command, way) in both_ways(&dir, &programs) {
        let out = Command::new("sh").args(["-c", "exec \"$0\" \"$@\" >&-"]).arg(command.get_program()).args(command.get_args()).current_dir(&dir).output().unwrap();
        assert_eq!(ran(&out), (0, String::new(), "the end\n".into()), "{way}");
    }
}

/// A program started with `SIGPIPE` ignored, as some shells and runners start theirs, still ends
/// by it when its output is closed early, interpreted and compiled (1330, 1547): the error of
/// the closed pipe, which it gets instead, ends it as the signal would.
#[cfg(unix)]
#[test]
fn an_output_closed_early_with_the_signal_ignored() {
    use std::io::BufRead;
    use std::os::unix::process::ExitStatusExt;
    let dir = work("an_output_closed_early_with_the_signal_ignored");
    for (command, way) in both_ways(&dir, &[("many", MANY)]) {
        let mut child = Command::new("sh").args(["-c", "trap '' PIPE; exec \"$0\" \"$@\""]).arg(command.get_program()).args(command.get_args()).current_dir(&dir).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let mut first = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut first).unwrap();
        assert_eq!(first, "0\n", "{way}");
        let out = child.wait_with_output().unwrap();
        assert_eq!(text(&out.stderr), "", "{way}");
        assert_eq!((out.status.code(), out.status.signal()), (None, Some(13)), "{way}");
    }
}

/// A program started with its output or its standard error closed (`program >&-`) does not write
/// what it prints into the first file it opens, which would take the number of the closed stream:
/// compiled code opens `/dev/null` there first, as Rust's runtime does for the interpreter (1595).
#[cfg(unix)]
#[test]
fn closed_standard_streams() {
    let dir = work("closed_standard_streams");
    let source = "Result<void, IoError> main() {\n    var writer = try File.createText(\"data.txt\")\n    try writer.writeLine(\"the data\")\n    print(\"a line for the output\")\n    printError(\"a line for the standard error\")\n    try writer.close()\n    return Ok()\n}\n";
    for (command, way) in both_ways(&dir, &[("streams", source)]) {
        for closed in [">&-", "2>&-", "<&- >&- 2>&-"] {
            let _ = std::fs::remove_file(dir.join("data.txt"));
            let out = Command::new("sh").args(["-c", &format!("exec \"$0\" \"$@\" {closed}")]).arg(command.get_program()).args(command.get_args()).current_dir(&dir).output().unwrap();
            assert_eq!(out.status.code(), Some(0), "{way} {closed}");
            assert_eq!(std::fs::read_to_string(dir.join("data.txt")).unwrap(), "the data\n", "{way} {closed}");
        }
    }
}

/// A line of `printError` (and so of `Log`) is one write, however long it is: four programs that
/// write their lines into one file opened for appending never mix them (1595). Before, compiled
/// code wrote a line longer than 512 bytes in pieces, and the interpreter wrote the end of the
/// line apart.
#[cfg(unix)]
#[test]
fn error_lines_are_whole() {
    let dir = work("error_lines_are_whole");
    let source = "void main(List<string> args) {\n    var payload = \"x\".repeat(2000)\n    for _ in 0..500 {\n        printError($\"{args[0]} {payload} END\")\n    }\n}\n";
    for (command, way) in both_ways(&dir, &[("lines", source)]) {
        let _ = std::fs::remove_file(dir.join("lines.txt"));
        let script = "for tag in A B C D; do \"$0\" \"$@\" $tag 2>>lines.txt & done; wait";
        let out = Command::new("sh").args(["-c", script]).arg(command.get_program()).args(command.get_args()).current_dir(&dir).output().unwrap();
        assert!(out.status.success(), "{way}");
        let written = std::fs::read_to_string(dir.join("lines.txt")).unwrap();
        let payload = "x".repeat(2000);
        let whole = written.lines().filter(|line| ["A", "B", "C", "D"].iter().any(|tag| *line == format!("{tag} {payload} END"))).count();
        assert_eq!((written.lines().count(), whole), (2000, 2000), "{way}");
    }
}

/// A standard error whose reader is gone ends the program by `SIGPIPE`, as the output does
/// (1601), interpreted and compiled, also when the program was started with the signal ignored:
/// a line of `Log` or `printError` that no one can read does not let it go on as if it were read.
#[cfg(unix)]
#[test]
fn an_error_output_closed_early() {
    use std::os::unix::process::ExitStatusExt;
    let dir = work("an_error_output_closed_early");
    let source = "void main(List<string> args) {\n    var line = \"\"\n    _ = Stdin.readLine(ref line)\n    if args[0] == \"log\" {\n        Log.error(\"no one reads this\")\n    } else {\n        printError(\"no one reads this\")\n    }\n    print(\"went on\")\n}\n";
    for (command, way) in both_ways(&dir, &[("closed", source)]) {
        for (kind, ignored) in [("log", false), ("printError", false), ("printError", true)] {
            let trap = if ignored { "trap '' PIPE; " } else { "" };
            let mut child = Command::new("sh").args(["-c", &format!("{trap}exec \"$0\" \"$@\"")]).arg(command.get_program()).args(command.get_args()).arg(kind).current_dir(&dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
            // The reader of the standard error is gone before the program writes to it.
            drop(child.stderr.take());
            std::io::Write::write_all(&mut child.stdin.take().unwrap(), b"go\n").unwrap();
            let out = child.wait_with_output().unwrap();
            assert_eq!((text(&out.stdout), out.status.code(), out.status.signal()), (String::new(), None, Some(13)), "{way} {kind} {ignored}");
        }
    }
}

/// A panic in a program whose output was closed early is told, and ends the program with 1,
/// interpreted and compiled (1602): compiled code ended by `SIGPIPE` while it gave out what was
/// printed, before it wrote the panic.
#[cfg(unix)]
#[test]
fn a_panic_into_a_closed_output() {
    let dir = work("a_panic_into_a_closed_output");
    let source = "void main() {\n    var line = \"\"\n    _ = Stdin.readLine(ref line)\n    print(\"one short line\")\n    var xs = List<int>()\n    var k = 5\n    print(xs[k])\n}\n";
    for (mut command, way) in both_ways(&dir, &[("panics", source)]) {
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        drop(child.stdout.take());
        std::io::Write::write_all(&mut child.stdin.take().unwrap(), b"go\n").unwrap();
        let out = child.wait_with_output().unwrap();
        let err = text(&out.stderr);
        assert!(err.starts_with("panic: index 5 is out of range"), "{way}: {err}");
        assert_eq!(out.status.code(), Some(1), "{way}");
    }
}

/// A standard error no one reads (a pipe whose reader is gone) ends the tool by `SIGPIPE`, as
/// its output does (1604): a check's error and a command line that is not a usage ended with
/// 101, a Rust panic. A program ends as compiled code ends: a panic with 1, the message lost
/// (1602), and `main`'s error by `SIGPIPE`, as a line of `printError` (1601).
#[cfg(unix)]
#[test]
fn a_standard_error_no_one_reads() {
    use std::os::unix::process::ExitStatusExt;
    let dir = work("a_standard_error_no_one_reads");
    write(&dir, "wrong.sg", WRONG);
    let ended = |command: &mut Command| {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        let status = command.stdout(Stdio::null()).stderr(writer).status().unwrap();
        (status.code(), status.signal())
    };
    let tool = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sg"));
        command.current_dir(&dir);
        command
    };
    assert_eq!(ended(tool().arg("wrong.sg")), (None, Some(13)), "a check's error");
    assert_eq!(ended(tool().args(["nonsense", "x", "y"])), (None, Some(13)), "not a usage");
    assert_eq!(ended(tool().arg("run")), (None, Some(13)), "no program");
    let failing = "Result<void, string> main() {\n    return Error(\"no one reads this\")\n}\n";
    for (mut command, way) in both_ways(&dir, &[("panics", PANICS), ("fails", failing)]) {
        let expected = if way.starts_with("panics") { (Some(1), None) } else { (None, Some(13)) };
        assert_eq!(ended(&mut command), expected, "{way}");
    }
}

/// Folders whose links lead back into the folder of the program are walked once (1596): the
/// tool walked them for ever, when it looked for what a missing name could be and when it ran
/// the tests of every module.
#[cfg(unix)]
#[test]
fn walks_a_folder_whose_links_go_round() {
    let dir = work("walks_a_folder_whose_links_go_round");
    for name in ["a", "b", "c"] {
        std::os::unix::fs::symlink(".", dir.join(name)).unwrap();
    }
    write(&dir, "main.sg", "void main() {\n    var x = Missing()\n}\n");
    for words in [&["main.sg"][..], &["test", "."][..]] {
        let (code, _, err) = ran(&sg(&dir, words));
        assert_eq!(code, 1, "{words:?}");
        assert!(err.contains("there is no function named `Missing`"), "{words:?}: {err}");
    }
}

/// Texts nested deeper than the parser takes are an error (1596): the tool looked for the names
/// in each before it parsed, nested as deep as they were, and its stack ran out.
#[test]
fn refuses_texts_nested_too_deep() {
    let dir = work("refuses_texts_nested_too_deep");
    let mut body = "1".to_string();
    for _ in 0..32_000 {
        body = format!("$\"{{{body}}}\"");
    }
    write(&dir, "deep.sg", &format!("void main() {{\n    print({body})\n}}\n"));
    let (code, _, err) = ran(&sg(&dir, &["deep.sg"]));
    assert_eq!(code, 1);
    assert!(err.starts_with("error: this is nested more than 1000 deep"), "{}", &err[..err.len().min(300)]);
}

#[test]
fn runs_tests() {
    let dir = work("runs_tests");
    write(&dir, "tests.sg", TESTS);
    write(&dir, "fails.sg", FAILS);
    let (code, out, err) = ran(&sg(&dir, &["test", "tests.sg"]));
    assert_eq!((code, err.as_str()), (0, ""));
    assert!(out.ends_with("2 test(s) passed\n"), "{out}");
    // A failed test is told in the output, as the others are; the code is 1.
    let (code, out, err) = ran(&sg(&dir, &["test", "fails.sg"]));
    assert_eq!((code, err.as_str()), (1, ""));
    assert!(out.contains("FAIL  is wrong") && out.ends_with("1 of 2 test(s) failed\n"), "{out}");
    let (code, _, err) = ran(&sg(&dir, &["test", "tests.sg", "more"]));
    assert_eq!(code, 2);
    assert!(err.starts_with("usage: sg "), "{err}");
    assert_eq!(ran(&sg(&dir, &["test"])).0, 2);
    if !has_compiler() {
        return;
    }
    // Compiled, `--compiled` before or after the path; the program it builds does not stay.
    for words in [&["test", "--compiled", "tests.sg"][..], &["test", "tests.sg", "--compiled"][..]] {
        let (code, out, err) = ran(&sg(&dir, words));
        assert_eq!((code, err.as_str()), (0, ""), "{words:?}");
        assert!(out.ends_with("2 test(s) passed\n"), "{out}");
    }
    let (code, out, _) = ran(&sg(&dir, &["test", "--compiled", "fails.sg"]));
    assert_eq!(code, 1);
    assert!(out.ends_with("1 of 2 test(s) failed\n"), "{out}");
    let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, ["fails.sg", "tests.sg"]);
}

#[test]
fn builds_a_program() {
    if !has_compiler() {
        return;
    }
    let dir = work("builds_a_program");
    write(&dir, "hello.sg", HELLO);
    write(&dir, "panics.sg", PANICS);
    write(&dir, "wrong.sg", WRONG);
    write(&dir, "tests.sg", TESTS);
    // Named after the file without `-o`, says nothing when it is built.
    assert_eq!(ran(&sg(&dir, &["build", "hello.sg"])), (0, String::new(), String::new()));
    let built = Command::new(program(&dir, "hello")).output().unwrap();
    assert_eq!(ran(&built), (0, "hello\n".into(), String::new()));
    // `-o` and `--release`, in any order.
    assert_eq!(ran(&sg(&dir, &["build", "--release", "hello.sg", "-o", "fast"])).0, 0);
    assert_eq!(ran(&Command::new(program(&dir, "fast")).output().unwrap()).1, "hello\n");
    // A panic of compiled code is the interpreter's, with the same code.
    assert_eq!(ran(&sg(&dir, &["build", "panics.sg"])).0, 0);
    let (code, _, err) = ran(&Command::new(program(&dir, "panics")).output().unwrap());
    assert_eq!(code, 1);
    assert!(err.starts_with("panic: index 5 is out of range: the length is 2"), "{err}");
    // An error of the program: 1, and nothing is written.
    let (code, _, err) = ran(&sg(&dir, &["build", "wrong.sg"]));
    assert_eq!(code, 1);
    assert!(err.starts_with("error: there is no variable named `nope`"), "{err}");
    assert!(!program(&dir, "wrong").exists());
    // `--test`: the program that runs the tests.
    assert_eq!(ran(&sg(&dir, &["build", "tests.sg", "--test", "-o", "check"])).0, 0);
    let (code, out, _) = ran(&Command::new(program(&dir, "check")).output().unwrap());
    assert_eq!(code, 0);
    assert!(out.ends_with("2 test(s) passed\n"), "{out}");
    let (code, _, err) = ran(&sg(&dir, &["build", "missing.sg"]));
    assert_eq!(code, 2);
    assert!(err.starts_with("error: cannot read missing.sg"), "{err}");
}

#[test]
fn builds_a_library() {
    if !has_compiler() {
        return;
    }
    let dir = work("builds_a_library");
    write(&dir, "calc.sg", LIBRARY);
    write(&dir, "hello.sg", HELLO);
    let (static_name, shared_name) = if cfg!(windows) {
        ("calc.lib", "calc.dll")
    } else if cfg!(target_os = "macos") {
        ("libcalc.a", "libcalc.dylib")
    } else {
        ("libcalc.a", "libcalc.so")
    };
    // Each says what it built and how a program links it, and writes the header beside it.
    let (code, out, err) = ran(&sg(&dir, &["build", "calc.sg", "--library"]));
    assert_eq!((code, err.as_str()), (0, ""));
    assert!(out.starts_with(&format!("built {static_name} and calc.h; a program links it with `")), "{out}");
    assert!(dir.join(static_name).exists() && dir.join("calc.h").exists());
    let header = std::fs::read_to_string(dir.join("calc.h")).unwrap();
    assert!(header.contains("add("), "{header}");
    std::fs::remove_file(dir.join("calc.h")).unwrap();
    let (code, out, err) = ran(&sg(&dir, &["build", "calc.sg", "--shared"]));
    assert_eq!((code, err.as_str()), (0, ""));
    assert!(out.starts_with(&format!("built {shared_name}")), "{out}");
    assert!(dir.join(shared_name).exists() && dir.join("calc.h").exists());
    // A library has no `main`.
    let (code, _, err) = ran(&sg(&dir, &["build", "hello.sg", "--library"]));
    assert_eq!(code, 1);
    assert!(err.starts_with("error: a library has no `main`"), "{err}");
}

#[test]
fn builds_for_a_machine_without_a_system() {
    if !has_compiler() {
        return;
    }
    let dir = work("builds_for_a_machine_without_a_system");
    write(&dir, "board.sg", "export long start() {\n    return 42\n}\n\nvoid onPanic(read string message) {\n}\n");
    write(&dir, "hello.sg", HELLO);
    let (code, out, err) = ran(&sg(&dir, &["build", "board.sg", "--freestanding"]));
    assert_eq!((code, err.as_str()), (0, ""));
    assert!(out.starts_with("built board.o for a machine without an operating system"), "{out}");
    assert!(dir.join("board.o").exists());
    // Flags after `--` go to the C compiler as they are.
    assert_eq!(ran(&sg(&dir, &["build", "board.sg", "--freestanding", "-o", "small.o", "--", "-Os"])).0, 0);
    assert!(dir.join("small.o").exists());
    let (code, _, err) = ran(&sg(&dir, &["build", "hello.sg", "--freestanding"]));
    assert_eq!(code, 1);
    assert!(err.starts_with("error: a program without an operating system has no `main`"), "{err}");
}

/// Command lines that are not one of the usages: 2, with the usage, and nothing built.
#[test]
fn refuses_what_is_not_a_usage() {
    let dir = work("refuses_what_is_not_a_usage");
    write(&dir, "hello.sg", HELLO);
    let refused: &[&[&str]] = &[
        &["build"],
        &["build", "hello.sg", "--test", "--library"],
        &["build", "hello.sg", "--shared", "--freestanding"],
        &["build", "hello.sg", "--target", "riscv32-unknown-none-elf"],
        &["build", "hello.sg", "--", "-Os"],
        &["build", "hello.sg", "--relase"],
        &["build", "hello.sg", "other.sg"],
        // A third program, and a flag twice (1326).
        &["build", "hello.sg", "other.sg", "third.sg"],
        &["build", "hello.sg", "-o", "a", "-o", "b"],
        &["build", "hello.sg", "--release", "--release"],
        &["build", "hello.sg", "--test", "--test"],
        &["build", "hello.sg", "--freestanding", "--target", "a", "--target", "b"],
        &["test", "--compiled", "--compiled"],
        &["test", "--compiled", "hello.sg", "--compiled"],
        // A flag with nothing after it (1327).
        &["build", "hello.sg", "-o"],
        &["build", "hello.sg", "--target"],
        &["build", "hello.sg", "--freestanding", "--target"],
        // `-o` is not `emit-c`'s (1328).
        &["emit-c", "hello.sg", "-o", "hello.c"],
        &["emit-c"],
        &["test", "--compiled"],
    ];
    for words in refused {
        let (code, out, err) = ran(&sg(&dir, words));
        assert_eq!((code, out.as_str()), (2, ""), "{words:?}: {err}");
        assert!(err.starts_with("usage: sg "), "{words:?}: {err}");
    }
    let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, ["hello.sg"]);
}

/// An output into a folder that is not there is told before anything is compiled, as a program
/// that cannot be read is: 2 (1329).
#[test]
fn refuses_an_output_folder_that_is_not_there() {
    let dir = work("refuses_an_output_folder_that_is_not_there");
    write(&dir, "hello.sg", HELLO);
    write(&dir, "calc.sg", LIBRARY);
    write(&dir, "file", "");
    let nowhere = Path::new("nowhere").join("hello").display().to_string();
    let in_a_file = Path::new("file").join("hello").display().to_string();
    for (words, folder) in [
        (vec!["build", "hello.sg", "-o", &nowhere], "nowhere"),
        (vec!["build", "hello.sg", "-o", &in_a_file], "file"),
        (vec!["build", "calc.sg", "--library", "-o", &nowhere], "nowhere"),
        (vec!["build", "hello.sg", "--test", "-o", &nowhere], "nowhere"),
    ] {
        let target = words.last().unwrap().to_string();
        assert_eq!(ran(&sg(&dir, &words)), (2, String::new(), format!("error: cannot write {target}: there is no folder {folder}\n")), "{words:?}");
    }
    let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, ["calc.sg", "file", "hello.sg"]);
}

/// An output that is a folder is told before anything is compiled too: the program of a folder
/// built from the folder above it is named as the folder by default (1398, as Go tells it).
#[test]
fn refuses_an_output_that_is_a_folder() {
    let dir = work("refuses_an_output_that_is_a_folder");
    write(&dir, "hello.sg", HELLO);
    let shop = dir.join("shop");
    std::fs::create_dir_all(&shop).unwrap();
    write(&shop, "main.sg", HELLO);
    // On Windows the program is `shop.exe`, which is not the folder.
    if !cfg!(windows) {
        assert_eq!(ran(&sg(&dir, &["build", "shop"])), (2, String::new(), "error: `shop` is a folder; name the program with -o\n".into()));
        assert_eq!(ran(&sg(&dir, &["build", "shop", "--test"])), (2, String::new(), "error: `shop` is a folder; name the program with -o\n".into()));
    }
    let folder = sg::cc::program_file("out");
    std::fs::create_dir_all(dir.join(&folder)).unwrap();
    assert_eq!(ran(&sg(&dir, &["build", "hello.sg", "-o", "out"])), (2, String::new(), format!("error: cannot write {folder}: it is a folder\n")));
    assert_eq!(std::fs::read_dir(dir.join(&folder)).unwrap().count(), 0);
}

/// A program given as a file in the current folder names the files of its modules as it is
/// named, `stock/product.sg` and not `./stock/product.sg` (1401).
#[test]
fn names_module_files_as_the_main_file_is() {
    let dir = work("names_module_files_as_the_main_file_is");
    write(&dir, "main.sg", "use stock\n\nvoid main() {\n    print(stock.count())\n}\n");
    let stock = dir.join("stock");
    std::fs::create_dir_all(&stock).unwrap();
    write(&stock, "product.sg", "public int count() {\n    return nope\n}\n");
    let place = Path::new("stock").join("product.sg").display().to_string();
    for main in ["main.sg", "./main.sg"] {
        let (code, _, err) = ran(&sg(&dir, &[main]));
        let shown = if main.starts_with("./") { Path::new(".").join(&place).display().to_string() } else { place.clone() };
        assert_eq!(code, 1);
        assert!(err.contains(&format!("--> {shown}:2:12\n")), "{main}: {err}");
    }
}

/// The C of `emit-c` into an output closed before the end ends the tool as it ends a program
/// (1399, 1330): no message.
#[test]
fn emits_c_into_an_output_closed_early() {
    use std::io::BufRead;
    let dir = work("emits_c_into_an_output_closed_early");
    write(&dir, "hello.sg", HELLO);
    // The C is larger than what a pipe holds, so that its writing meets the closed end.
    let mut child = Command::new(env!("CARGO_BIN_EXE_sg")).args(["emit-c", "hello.sg"]).current_dir(&dir).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let mut first = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut first).unwrap();
    assert!(first.starts_with('#'), "{first}");
    let out = child.wait_with_output().unwrap();
    assert_eq!(text(&out.stderr), "");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!((out.status.code(), out.status.signal()), (None, Some(13)));
    }
    #[cfg(windows)]
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn emits_c() {
    let dir = work("emits_c");
    write(&dir, "hello.sg", HELLO);
    write(&dir, "wrong.sg", WRONG);
    write(&dir, "tests.sg", TESTS);
    write(&dir, "calc.sg", LIBRARY);
    // The C goes to the output, and nothing is written.
    for words in [&["emit-c", "hello.sg"][..], &["emit-c", "hello.sg", "--release"][..], &["emit-c", "tests.sg", "--test"][..], &["emit-c", "calc.sg", "--library"][..]] {
        let (code, out, err) = ran(&sg(&dir, words));
        assert_eq!((code, err.as_str()), (0, ""), "{words:?}");
        assert!(out.starts_with('#') && out.contains("#include"), "{words:?}");
    }
    let (code, out, err) = ran(&sg(&dir, &["emit-c", "wrong.sg"]));
    assert_eq!((code, out.as_str()), (1, ""));
    assert!(err.starts_with("error: there is no variable named `nope`"), "{err}");
    let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, ["calc.sg", "hello.sg", "tests.sg", "wrong.sg"]);
}

/// `sg` with these words, in `dir`, with `tmp` as the temporary folder.
fn sg_in(dir: &Path, tmp: &Path, words: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sg")).args(words).current_dir(dir).env("TMPDIR", tmp).env("TMP", tmp).env("TEMP", tmp).stdin(Stdio::null()).output().unwrap()
}

/// What is in a folder, by name.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

/// The files a build makes on the way (the C, the object of a library, the program of compiled
/// tests) are in a folder of the temporary folder that only this user opens, made new each time,
/// and gone at the end (choice 1333): nothing another user put there at a name that could be
/// guessed is written through or run.
#[test]
fn keeps_its_temporary_files_to_itself() {
    if !has_compiler() {
        return;
    }
    let dir = work("keeps_its_temporary_files_to_itself");
    let tmp = dir.join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    write(&dir, "hello.sg", HELLO);
    write(&dir, "tests.sg", TESTS);
    write(&dir, "calc.sg", LIBRARY);
    for words in [&["build", "hello.sg"][..], &["build", "calc.sg", "--library"][..], &["build", "calc.sg", "--shared"][..], &["test", "tests.sg", "--compiled"][..]] {
        let (code, _, err) = ran(&sg_in(&dir, &tmp, words));
        assert_eq!((code, err.as_str()), (0, ""), "{words:?}");
        assert_eq!(names_in(&tmp), Vec::<String>::new(), "{words:?}");
    }
    // What another user plants at the names of the process (`exec` keeps the number of the
    // shell): a link to a file of the user, and a file where a folder would be made.
    #[cfg(unix)]
    {
        let precious = write(&dir, "precious.txt", "precious\n");
        let planted = |words: &str| {
            let script = format!("ln -s \"$0\" \"$TMPDIR/sg-$$.c\"; ln -s \"$0\" \"$TMPDIR/sg-test-$$\"; : > \"$TMPDIR/sg-$$\"; exec \"$1\" {words}");
            Command::new("sh").arg("-c").arg(script).arg(&precious).arg(env!("CARGO_BIN_EXE_sg")).current_dir(&dir).env("TMPDIR", &tmp).stdin(Stdio::null()).output().unwrap()
        };
        for words in ["build hello.sg -o planted", "build calc.sg --library", "test tests.sg --compiled"] {
            let (code, _, err) = ran(&planted(words));
            assert_eq!((code, err.as_str()), (0, ""), "{words}");
            assert_eq!(std::fs::read_to_string(&precious).unwrap(), "precious\n", "{words}");
            for name in names_in(&tmp) {
                std::fs::remove_file(tmp.join(name)).unwrap();
            }
        }
    }
}

/// A name the linker is given whole (choice 1339): a comma in the name of a library is part of
/// its name, not the start of another option of the linker (`-map` would write a file).
#[test]
fn a_comma_in_a_name_goes_to_the_linker_whole() {
    if !has_compiler() {
        return;
    }
    let dir = work("a_comma_in_a_name_goes_to_the_linker_whole");
    write(&dir, "v,-map,PLANTED.sg", LIBRARY);
    let (code, out, err) = ran(&sg(&dir, &["build", "v,-map,PLANTED.sg", "--shared"]));
    assert_eq!((code, err.as_str()), (0, ""), "{out}");
    let shared = if cfg!(windows) {
        "v,-map,PLANTED.dll"
    } else if cfg!(target_os = "macos") {
        "libv,-map,PLANTED.dylib"
    } else {
        "libv,-map,PLANTED.so"
    };
    assert!(dir.join(shared).exists(), "{:?}", names_in(&dir));
    assert!(!dir.join("PLANTED").exists() && !dir.join("PLANTED.dylib").exists() && !dir.join("PLANTED.dll.lib").exists(), "{:?}", names_in(&dir));
    if cfg!(windows) {
        assert!(dir.join("v,-map,PLANTED.dll.lib").exists(), "{:?}", names_in(&dir));
    }
    // The name a program that links it looks for it by.
    if cfg!(target_os = "macos") {
        if let Ok(told) = Command::new("otool").arg("-D").arg(dir.join(shared)).output() {
            assert!(text(&told.stdout).contains("@rpath/libv,-map,PLANTED.dylib"), "{}", text(&told.stdout));
        }
    }
}

/// A file a program takes in, by `embed` or as a C file, lies in the folder of the file that
/// names it or below it, also where a link leads (choice 1339): a module of someone else's does
/// not read a file of the machine it is built on into the program.
#[cfg(unix)]
#[test]
fn takes_in_no_file_from_outside_its_folder() {
    let dir = work("takes_in_no_file_from_outside_its_folder");
    write(&dir, "secret.txt", "SECRET\n");
    write(&dir, "secret.c", "static int f(void) { return 1; }\n");
    let project = dir.join("project");
    std::fs::create_dir_all(project.join("data")).unwrap();
    std::os::unix::fs::symlink("../secret.txt", project.join("token.txt")).unwrap();
    std::os::unix::fs::symlink("../../secret.txt", project.join("data/token.txt")).unwrap();
    std::os::unix::fs::symlink("../secret.c", project.join("native.c")).unwrap();
    write(&project, "inside.txt", "inside\n");
    std::os::unix::fs::symlink("inside.txt", project.join("also.txt")).unwrap();
    for (name, program) in [
        ("token.sg", "const byte[] Token = embed(\"token.txt\")\n\nvoid main() {\n    print(Token.length())\n}\n"),
        ("deeper.sg", "const byte[] Token = embed(\"data/token.txt\")\n\nvoid main() {\n    print(Token.length())\n}\n"),
        ("native.sg", "extern \"./native.c\" {\n    int f()\n}\n\nvoid main() {\n    print(unsafe f())\n}\n"),
    ] {
        write(&project, name, program);
        let (code, out, err) = ran(&sg(&project, &[name]));
        assert_eq!((code, out.as_str()), (1, ""), "{name}: {err}");
        assert!(err.contains("lies outside the folder of this file") && err.contains("secret."), "{name}: {err}");
    }
    // A link that stays inside is taken.
    write(&project, "also.sg", "const byte[] Also = embed(\"also.txt\")\n\nvoid main() {\n    print(Also.length())\n}\n");
    let (code, out, err) = ran(&sg(&project, &["also.sg"]));
    assert_eq!((code, out.as_str(), err.as_str()), (0, "7\n", ""));
}

/// A function whose values no thread's stack holds is refused by `sg build` on every machine
/// (choice 1346): it could not run, and the C compiler of Windows on Arm fails on a frame above
/// 256 MB. Two variables that each fit count together; the interpreter keeps them on its heap.
#[test]
fn refuses_a_frame_no_stack_holds() {
    if !has_compiler() {
        return;
    }
    let dir = work("refuses_a_frame_no_stack_holds");
    write(&dir, "big.sg", "void main() {\n    var first = byte[150000000]\n    var second = byte[150000000]\n    print(first[0] + second[0])\n}\n");
    let (code, out, err) = ran(&sg(&dir, &["build", "big.sg"]));
    assert_eq!((code, out.as_str()), (1, ""), "{err}");
    assert!(err.starts_with("error: the values of `main` may take more than 256 MB of its stack"), "{err}");
    assert!(!program(&dir, "big").exists());
    // Under the bound it is built.
    write(&dir, "fits.sg", "void main() {\n    var data = byte[100000000]\n    print(data[0])\n}\n");
    let (code, _, err) = ran(&sg(&dir, &["build", "fits.sg"]));
    assert_eq!((code, err.as_str()), (0, ""));
}

/// A program that reads its command line with `parseOrExit` (choice 1429), interpreted and compiled:
/// the help and the version go to the output with 0, an error to the standard error with 2 and a line
/// that names `--help`, and a command line that reads runs the program.
#[test]
fn a_command_line_read_or_exit() {
    const GIFCROP: &str = "void main(List<string> args) {\n    var cl = CommandLine(\"gifcrop\", \"Crops GIF images.\")\n    cl.flag(\"verbose\", 'v', \"Print each step\")\n    cl.option(\"output\", 'o', \"FILE\", \"Where to write\")\n    cl.positional(\"input\", \"The GIF to read\")\n    cl.version(\"1.2.0\")\n    var a = cl.parseOrExit(read args)\n    print($\"{a.positional(\"input\")} -> {a.option(\"output\") ?? \"out.gif\"}, verbose {a.flag(\"verbose\")}\")\n}\n";
    const HELP: &str = "Crops GIF images.\n\nUsage: gifcrop [options] <input>\n\nArguments:\n  <input>              The GIF to read\n\nOptions:\n  -v, --verbose        Print each step\n  -o, --output FILE    Where to write\n  -h, --help           Print this help\n      --version        Print the version\n";
    let dir = work("a_command_line_read_or_exit");
    write(&dir, "gifcrop.sg", GIFCROP);
    let mut ways: Vec<(Vec<String>, &str)> = vec![(vec![env!("CARGO_BIN_EXE_sg").into(), "gifcrop.sg".into()], "interpreted")];
    if has_compiler() {
        assert_eq!(ran(&sg(&dir, &["build", "gifcrop.sg"])), (0, String::new(), String::new()));
        ways.push((vec![program(&dir, "gifcrop").to_string_lossy().into_owned()], "compiled"));
    }
    for (command, way) in ways {
        let run = |words: &[&str]| {
            ran(&Command::new(&command[0]).args(&command[1..]).args(words).current_dir(&dir).stdin(Stdio::null()).output().unwrap())
        };
        assert_eq!(run(&["--help"]), (0, HELP.into(), String::new()), "{way}");
        assert_eq!(run(&["in.gif", "-h"]), (0, HELP.into(), String::new()), "{way}");
        assert_eq!(run(&["--version"]), (0, "gifcrop 1.2.0\n".into(), String::new()), "{way}");
        assert_eq!(run(&["-vo", "small.gif", "in.gif"]), (0, "in.gif -> small.gif, verbose true\n".into(), String::new()), "{way}");
        assert_eq!(run(&["in.gif"]), (0, "in.gif -> out.gif, verbose false\n".into(), String::new()), "{way}");
        let tried = "Try 'gifcrop --help'.\n";
        assert_eq!(run(&[]), (2, String::new(), format!("gifcrop: missing <input>\n{tried}")), "{way}");
        assert_eq!(run(&["--verbos", "in.gif"]), (2, String::new(), format!("gifcrop: unknown option '--verbos' (did you mean '--verbose'?)\n{tried}")), "{way}");
        assert_eq!(run(&["in.gif", "-o"]), (2, String::new(), format!("gifcrop: option '-o' needs a value\n{tried}")), "{way}");
        assert_eq!(run(&["in.gif", "more.gif"]), (2, String::new(), format!("gifcrop: unexpected argument 'more.gif'\n{tried}")), "{way}");
    }
}

/// Every parsing case of JSONTestSuite (`tests/json-suite`, choice 1442), read by `Json.parse`
/// interpreted and compiled: each `y_` read, each `n_` refused, each `i_` as SG's rules decide.
#[test]
fn a_json_suite() {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("json-suite");
    let cases = suite.join("test_parsing").to_string_lossy().into_owned();
    let check = suite.join("check.sg").to_string_lossy().into_owned();
    let dir = work("a_json_suite");
    let expected = (0, "318 of 318 files as expected\n".to_string(), String::new());
    assert_eq!(ran(&sg(&dir, &[&check, &cases])), expected, "interpreted");
    if has_compiler() {
        let out = program(&dir, "check").to_string_lossy().into_owned();
        assert_eq!(ran(&sg(&dir, &["build", &check, "-o", &out])), (0, String::new(), String::new()));
        let run = Command::new(&out).arg(&cases).current_dir(&dir).stdin(Stdio::null()).output().unwrap();
        assert_eq!(ran(&run), expected, "compiled");
    }
}

/// RE2's search tests (`tests/regex-suite`, choice 1468), run by `Regex` interpreted and
/// compiled: the leftmost match and the match of the whole text, with the groups.
#[test]
fn a_regex_suite() {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("regex-suite");
    let cases = suite.join("re2-search.txt").to_string_lossy().into_owned();
    let check = suite.join("check.sg").to_string_lossy().into_owned();
    let dir = work("a_regex_suite");
    let expected = (0, "1772 of 1888 cases tried, 116 left out; 0 wrong\n".to_string(), String::new());
    assert_eq!(ran(&sg(&dir, &[&check, &cases])), expected, "interpreted");
    if has_compiler() {
        let out = program(&dir, "check").to_string_lossy().into_owned();
        assert_eq!(ran(&sg(&dir, &["build", &check, "-o", &out])), (0, String::new(), String::new()));
        let run = Command::new(&out).arg(&cases).current_dir(&dir).stdin(Stdio::null()).output().unwrap();
        assert_eq!(ran(&run), expected, "compiled");
    }
}

/// The archives of `tests/zip-suite` (choice 1512): Go's `archive/zip/testdata`, archives made wrong
/// on purpose by `make.py`, and archives of Python and Info-ZIP, read by `ZipReader` interpreted
/// and compiled; what it prints is `expected.txt`.
#[test]
fn a_zip_suite() {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("zip-suite");
    let check = suite.join("check.sg").to_string_lossy().into_owned();
    let folder = suite.to_string_lossy().into_owned();
    let expected = std::fs::read_to_string(suite.join("expected.txt")).unwrap();
    let dir = work("a_zip_suite");
    assert_eq!(ran(&sg(&dir, &[&check, &folder])), (0, expected.clone(), String::new()), "interpreted");
    if has_compiler() {
        let out = program(&dir, "check").to_string_lossy().into_owned();
        assert_eq!(ran(&sg(&dir, &["build", &check, "-o", &out])), (0, String::new(), String::new()));
        let run = Command::new(&out).arg(&folder).current_dir(&dir).stdin(Stdio::null()).output().unwrap();
        assert_eq!(ran(&run), (0, expected, String::new()), "compiled");
    }
}

const LOGS: &str = "void main() {\n    print(Log.level())\n    Log.debug(\"debug\")\n    Log.warn(\"warn\")\n}\n";

/// The lines of standard error with the time of each left out: what follows `Z `.
fn log_lines(err: &str) -> Vec<String> {
    err.lines().map(|line| line.split_once("Z ").map_or(line, |(_, rest)| rest).to_string()).collect()
}

/// `SG_LOG` sets the level a program starts at, in any case; empty is not set; a value that names
/// no level is told in a line of `Log`'s own, escaped, and `Info` is used (choice 1449). The same
/// interpreted and compiled.
#[test]
fn log_level_from_the_environment() {
    let dir = work("log_level_from_the_environment");
    write(&dir, "logs.sg", LOGS);
    let mut ways = vec![vec![env!("CARGO_BIN_EXE_sg").to_string(), "logs.sg".to_string()]];
    if has_compiler() {
        assert_eq!(ran(&sg(&dir, &["build", "logs.sg"])).0, 0);
        ways.push(vec![program(&dir, "logs").to_string_lossy().into_owned()]);
    }
    let bad = "SG_LOG is \"verbose\", which is not a level (trace, debug, info, warn, error, off); Info is used";
    let cases: [(Option<&str>, &str, Vec<String>); 7] = [
        (None, "Info", vec!["WARN  warn".into()]),
        (Some("debug"), "Debug", vec!["DEBUG debug".into(), "WARN  warn".into()]),
        (Some("DeBuG"), "Debug", vec!["DEBUG debug".into(), "WARN  warn".into()]),
        (Some("off"), "Off", vec![]),
        (Some(""), "Info", vec!["WARN  warn".into()]),
        (Some("verbose"), "Info", vec![format!("WARN  {bad}"), "WARN  warn".into()]),
        (
            Some("x\n2026-11-19T20:15:03.123Z INFO  forged"),
            "Info",
            vec![
                "WARN  SG_LOG is \"x\\n2026-11-19T20:15:03.123Z INFO  forged\", which is not a level (trace, debug, info, warn, error, off); Info is used".into(),
                "WARN  warn".into(),
            ],
        ),
    ];
    for way in &ways {
        for (value, level, lines) in &cases {
            let mut command = Command::new(&way[0]);
            command.args(&way[1..]).current_dir(&dir).stdin(Stdio::null());
            match value {
                Some(value) => command.env("SG_LOG", value),
                None => command.env_remove("SG_LOG"),
            };
            let (code, out, err) = ran(&command.output().unwrap());
            assert_eq!((code, out, log_lines(&err)), (0, format!("{level}\n"), lines.clone()), "{way:?} with SG_LOG={value:?}");
        }
    }
}

/// `sg build` compiles with the C compiler `CC` names, on every system (1533): one that is not
/// there is told by its name, and an empty `CC` is no compiler named.
#[test]
fn builds_with_the_compiler_cc_names() {
    if !has_compiler() {
        return;
    }
    let dir = work("builds_with_the_compiler_cc_names");
    write(&dir, "hello.sg", HELLO);
    let build = |cc: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_sg"))
            .args(["build", "hello.sg", "-o", "hello"])
            .env("CC", cc)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        ran(&out)
    };
    let (code, out, err) = build("no-such-compiler-1533");
    assert_eq!((code, out.as_str()), (1, ""));
    assert!(err.starts_with("error: cannot run the C compiler `no-such-compiler-1533`: "), "{err}");
    let (code, _, err) = build("");
    assert_eq!(code, 0, "{err}");
    assert!(program(&dir, "hello").exists());
}

/// A pattern with so many states that the DFAs of `Regex` empty their cache more often than they
/// may in one search, and leave it to Pike's machine (1536, 1539): compiled only, as the
/// interpreter takes minutes over it. What it prints is Go's; the groups and the replacement,
/// found where the match starts after the text's start by Pike's machine with two slots and then
/// by the one of every group (1598), are Rust's.
#[test]
fn a_regex_dfa_that_gives_up() {
    if !has_compiler() {
        return;
    }
    const PROGRAM: &str = r#"void main() {
    var text = string.withCapacity(60000)
    uint x = 1
    for _ in 0..60000 {
        x = x.wrappingMultiply(1103515245).wrappingAdd(12345)
        if (x >> 16) & 1 == 0 {
            text.write('a')
        } else {
            text.write('b')
        }
    }
    var re = Regex.of("(a|b)*a(a|b){16}")
    print($"{re.find(read text) ?? panic("a match")} {re.isMatch(read text)} {Regex.of("a(a|b){16}b").findAll(read text).length()}")
    var later = $"xy{text}".toString()
    var groups = Regex.of("((a|b)*)a((a|b){16})")
    var caps = groups.captures(read later) ?? panic("a match")
    print($"{caps.get(0) ?? panic("0")} {caps.get(1) ?? panic("1")} {caps.get(3) ?? panic("3")} {caps.get(4) ?? panic("4")}")
    print(groups.replaceAll(read later, "[$3]"))
}
"#;
    let dir = work("a_regex_dfa_that_gives_up");
    write(&dir, "many.sg", PROGRAM);
    let out = program(&dir, "many").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", "many.sg", "--release", "-o", &out])), (0, String::new(), String::new()));
    let run = Command::new(&out).current_dir(&dir).stdin(Stdio::null()).output().unwrap();
    let expected = "0..60000 true 2869\n2..60002 2..59985 59986..60002 60001..60002\nxy[aabaabaabaaaaabb]\n";
    assert_eq!(ran(&run), (0, expected.to_string(), String::new()));
}

/// The trials of `trials/` (1418, 1529): `grep` and `wc` written in SG, interpreted and compiled, on
/// the megabyte `trials/inputs/text.sg` makes from seed 7, give what their versions in Rust and Zig
/// give; the counts below were taken from those (`python3 trials/measure.py` holds the three to one
/// another on more cases and on 100 MB).
#[test]
fn the_trials() {
    if !has_compiler() {
        return;
    }
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let source = |trial: &str| trials.join(trial).join("sg").join(format!("{trial}.sg")).to_string_lossy().into_owned();
    let dir = work("the_trials");
    let text = program(&dir, "text").to_string_lossy().into_owned();
    let maker = trials.join("inputs").join("text.sg").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &maker, "--release", "-o", &text])), (0, String::new(), String::new()));
    let made = Command::new(&text).args(["1", "7"]).current_dir(&dir).stdin(Stdio::null()).output().unwrap();
    assert!(made.status.success());
    std::fs::write(dir.join("t1.log"), &made.stdout).unwrap();
    let mut built = Vec::new();
    for trial in ["grep", "wc"] {
        let out = program(&dir, trial).to_string_lossy().into_owned();
        assert_eq!(ran(&sg(&dir, &["build", &source(trial), "--release", "-o", &out])), (0, String::new(), String::new()));
        built.push(out);
    }
    let cases: [(&str, &[&str], &str); 5] = [
        ("wc", &["t1.log"], "9198 115234 1000058 t1.log\n"),
        ("grep", &["-c", "ERROR", "t1.log"], "1293\n"),
        ("grep", &["-c", "-i", "TIMEOUT|ş[a-z]+r", "t1.log"], "2565\n"),
        ("grep", &["-v", "-c", r"@example\.com (GET|POST)", "t1.log"], "3010\n"),
        ("grep", &["-n", "in [0-9]{4} ms: .*日本", "t1.log"], ""),
    ];
    let mut printed = Vec::new();
    for (trial, args, expected) in cases {
        let script = source(trial);
        let mut words = vec![script.as_str()];
        words.extend_from_slice(args);
        let interpreted = ran(&sg(&dir, &words));
        let program = &built[if trial == "grep" { 0 } else { 1 }];
        let compiled = ran(&Command::new(program).args(args).current_dir(&dir).stdin(Stdio::null()).output().unwrap());
        assert_eq!(interpreted, compiled, "{trial} {args:?}: interpreted and compiled");
        if !expected.is_empty() {
            assert_eq!(compiled, (0, expected.to_string(), String::new()), "{trial} {args:?}");
        }
        printed.push(compiled.1);
    }
    // The lines of the last case: 660, each with its number.
    assert_eq!(printed[4].lines().count(), 660);
    assert!(printed[4].lines().all(|line| line.split_once(':').is_some_and(|(n, _)| n.parse::<u32>().is_ok())));
}

const ORDER: &str = r#"extern {
    int puts(read CString text)
}

struct Point : Formattable {
    int x
    int y

    void format(read this, ref Formatter f) {
        f.write($"({this.x}, {this.y})")
    }
}

Result<void, IoError> main() {
    print("one")
    printError("two")
    var c = CString.from("three") ?? panic("zero byte")
    _ = unsafe puts(read c)
    print($"four {4} {-4} {4.5} {true} {'4'} {Point { x = 4, y = 4 }}")
    try Stdout.write("five ")
    try Stdout.writeLine(5)
    try Stderr.writeLine("six")
    print("x".repeat(70000))
    print($"{"seven",6}|{7,-3}|{7:X4}")
    string line = ""
    for i in 0..3 {
        line.write($"{i}:{line.length()};")
    }
    print(line)
    exit(3)
}
"#;

/// What the output and the standard error get, sent into one file, comes in the order the
/// program wrote it (1544-1546): `print` gathers the output in a block of its own, which is
/// given on before `printError`, `Stderr`, a call of C (whose `puts` writes into C's `stdout`)
/// and the end of the program by `exit`. A line far longer than the block, and lines with
/// widths, which `sg_vlines` writes, go into it too. A value written into a text that it reads
/// sees the text as it was before the line.
#[test]
fn the_output_keeps_its_order() {
    let dir = work("the_output_keeps_its_order");
    write(&dir, "order.sg", ORDER);
    let expected = format!(
        "one\ntwo\nthree\nfour 4 -4 4.5 true 4 (4, 4)\nfive 5\nsix\n{}\n seven|7  |0007\n0:0;1:4;2:8;\n",
        "x".repeat(70000)
    );
    let mut ways = vec![(Command::new(env!("CARGO_BIN_EXE_sg")), "interpreted")];
    ways[0].0.arg("order.sg");
    if has_compiler() {
        assert_eq!(ran(&sg(&dir, &["build", "order.sg"])), (0, String::new(), String::new()));
        ways.push((Command::new(program(&dir, "order")), "compiled"));
    }
    for (mut command, way) in ways {
        let file = std::fs::File::create(dir.join("both.txt")).unwrap();
        let status = command.current_dir(&dir).stdin(Stdio::null()).stdout(file.try_clone().unwrap()).stderr(file).status().unwrap();
        assert_eq!(status.code(), Some(3), "{way}");
        let both = std::fs::read_to_string(dir.join("both.txt")).unwrap();
        assert!(both == expected, "{way}: {}", both.replace(&"x".repeat(70000), "x * 70000"));
    }
}

/// A program whose `sg.project` takes packages from folders (choices 1552, 1555): compiled, it
/// prints what the interpreter prints (`tests/packages/shop/main.out`).
#[test]
fn builds_a_program_with_packages() {
    if !has_compiler() {
        return;
    }
    let dir = work("builds_a_program_with_packages");
    let shop = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/packages/shop");
    let out = dir.join("shop");
    assert_eq!(ran(&sg(&dir, &["build", &shop.to_string_lossy(), "-o", &out.to_string_lossy()])), (0, String::new(), String::new()));
    let expected = std::fs::read_to_string(shop.join("main.out")).unwrap().replace("\r\n", "\n");
    assert_eq!(ran(&Command::new(program(&dir, "shop")).output().unwrap()), (0, expected, String::new()));
}

/// An `sg.project` that cannot be followed stops the command before anything is read, with 2,
/// as a program that cannot be read does; the message names the line.
#[test]
fn refuses_a_project_that_cannot_be_followed() {
    let dir = work("refuses_a_project_that_cannot_be_followed");
    let lib = dir.join("lib");
    for (name, address, requires) in [("utf", "example.com/bob/utf", ""), ("xml", "example.com/ada/xml", "require text example.com/bob/utf 1.0.0\n")] {
        std::fs::create_dir_all(lib.join(name)).unwrap();
        write(&lib.join(name), "sg.project", &format!("package {address}\n{requires}"));
        write(&lib.join(name), &format!("{name}.sg"), "public int one() {\n    return 1\n}\n");
    }
    std::fs::create_dir_all(lib.join("bare")).unwrap();
    write(&lib.join("bare"), "bare.sg", "public int one() {\n    return 1\n}\n");
    let cases: &[(&str, &str)] = &[
        ("requires xml example.com/ada/xml 1.4.0\n", "sg.project:1: `requires` is not a line of `sg.project`"),
        ("require xml example.com/ada/xml 1.4.0\n", "sg.lock has no example.com/ada/xml 1.4.0: `sg lock` writes it again"),
        ("require xml example.com/ada/xml 1.4.0\nlocal xml lib/nothing\n", "sg.project:2: there is no folder"),
        ("require bare example.com/ada/bare 1.0.0\nlocal bare lib/bare\n", "has no `sg.project`, which every package has"),
        ("require xml example.com/ada/other 1.4.0\nlocal xml lib/xml\n", "sg.project:2: the package in"),
        ("require xml example.com/ada/xml 1.4.0\nlocal xml lib/xml\n", "sg.lock does not have: `sg lock` writes it again"),
        ("require lib example.com/ada/xml 1.4.0\nlocal lib lib/xml\n", "sg.project:1: the folder `lib` beside the program has the name of the package `lib`"),
    ];
    write(&dir, "main.sg", "void main() {\n    print(1)\n}\n");
    std::fs::create_dir_all(dir.join("lib/sub")).unwrap();
    write(&dir.join("lib/sub"), "sub.sg", "public int two() {\n    return 2\n}\n");
    for (project, said) in cases {
        write(&dir, "sg.project", project);
        let (code, out, err) = ran(&sg(&dir, &["main.sg"]));
        assert_eq!((code, out.as_str()), (2, ""), "{project}: {err}");
        assert!(err.starts_with("error: ") && err.contains(said), "{project}: {err}");
    }
    // Given there with a `local` line, the package the other one uses is found, by its own name.
    write(&dir, "sg.project", "require xml example.com/ada/xml 1.4.0\nrequire letters example.com/bob/utf 1.2.0\nlocal xml lib/xml\nlocal letters lib/utf\n");
    write(&dir, "main.sg", "use xml\nuse letters\n\nvoid main() {\n    print(xml.one() + letters.one())\n}\n");
    std::fs::remove_dir_all(dir.join("lib/sub")).unwrap();
    assert_eq!(ran(&sg(&dir, &["main.sg"])), (0, "2\n".into(), String::new()));
}

/// A package kept inside the program's folder is read as a package and not as a module of the
/// program, as a folder with a `go.mod` is in Go: its tests are not the program's.
#[test]
fn a_package_inside_the_program_is_not_its_module() {
    let dir = work("a_package_inside_the_program_is_not_its_module");
    let xml = dir.join("vendor").join("xml");
    std::fs::create_dir_all(&xml).unwrap();
    write(&xml, "sg.project", "package example.com/ada/xml\n");
    write(&xml, "xml.sg", "public int one() {\n    return 1\n}\n\ntest \"the package's own\" {\n    assert(false)\n}\n");
    write(&dir, "sg.project", "require xml example.com/ada/xml 1.0.0\nlocal xml vendor/xml\n");
    write(&dir, "main.sg", "use xml\n\nvoid main() {\n    print(one())\n}\n\ntest \"the program's own\" {\n    assert(one() == 1)\n}\n");
    assert_eq!(ran(&sg(&dir, &["main.sg"])), (0, "1\n".into(), String::new()));
    let (code, out, _) = ran(&sg(&dir, &["test", "."]));
    assert_eq!((code, out.as_str()), (0, "ok    the program's own\n1 test(s) passed\n"));
    write(&dir, "main.sg", "use vendor.xml\n\nvoid main() {\n    print(one())\n}\n");
    let (code, _, err) = ran(&sg(&dir, &["main.sg"]));
    assert_eq!(code, 1);
    assert!(err.starts_with("error: there is no module named `vendor.xml`"), "{err}");
}

/// `git` for the repositories of the tests, without the settings of the user or of the system.
fn git(dir: &Path, words: &[&str], index: Option<&Path>, input: &[u8]) -> String {
    use std::io::Write;
    let empty = dir.parent().unwrap().join("empty-gitconfig");
    if !empty.exists() {
        std::fs::write(&empty, "").unwrap();
    }
    let mut command = Command::new("git");
    // Git for Windows keeps names such as `con.sg` out of its index; the tests make them on purpose.
    command.arg("-C").arg(dir).args(["-c", "user.name=SG", "-c", "user.email=sg@example.com", "-c", "commit.gpgsign=false", "-c", "core.protectNTFS=false"]).args(words);
    command.env("GIT_CONFIG_NOSYSTEM", "1").env("GIT_CONFIG_GLOBAL", &empty).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let mut child = command.spawn().expect("git is installed where the tests run");
    child.stdin.take().unwrap().write_all(input).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "git {words:?}: {}", text(&out.stderr));
    text(&out.stdout).trim().to_string()
}

/// A version of a package in the mirror (choice 1582): a bare repository at `mirror/<address>`
/// whose tag `v<version>` is a commit of these files, each with the mode git writes. The files
/// are put in by git's plumbing, with no folder of files, so a tree no system could check out
/// can be made too; a tag given again is moved.
fn publish(mirror: &Path, address: &str, version: &str, files: &[(&str, &str, &[u8])]) {
    let repository = mirror.join(address);
    if !repository.exists() {
        std::fs::create_dir_all(&repository).unwrap();
        git(&repository, &["init", "-q", "--bare"], None, b"");
    }
    let index = mirror.join("index");
    let _ = std::fs::remove_file(&index);
    for (mode, path, bytes) in files {
        let object = git(&repository, &["hash-object", "-w", "--stdin", "--no-filters"], None, bytes);
        git(&repository, &["update-index", "--add", "--cacheinfo", &format!("{mode},{object},{path}")], Some(&index), b"");
    }
    let tree = git(&repository, &["write-tree"], Some(&index), b"");
    let commit = git(&repository, &["commit-tree", &tree, "-m", version], None, b"");
    git(&repository, &["update-ref", &format!("refs/tags/v{version}"), &commit], None, b"");
}

/// A package of one file of SG beside its `sg.project`.
fn publish_sg(mirror: &Path, address: &str, version: &str, requires: &str, name: &str, code: &str) {
    let project = format!("package {address}\n{requires}");
    publish(mirror, address, version, &[("100644", "sg.project", project.as_bytes()), ("100644", &format!("{name}.sg"), code.as_bytes())]);
}

/// `sg` in `dir`, taking packages from `mirror` into `cache`.
fn sg_with(dir: &Path, mirror: &Path, cache: &Path, words: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sg")).args(words).current_dir(dir).env("SG_MIRROR", mirror).env("SG_CACHE", cache).stdin(Stdio::null()).output().unwrap()
}

/// The folders of packages in the cache, `<address>@<version>/sha256-<hex>` (choice 1591), and
/// anything else left in it but the folders above them.
fn cached(cache: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut folders = vec![cache.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).map(|read| read.flatten().collect::<Vec<_>>()).unwrap_or_default() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            match entry.file_type().unwrap().is_dir() && !name.starts_with("sha256-") {
                true => folders.push(path),
                false => found.push(path),
            }
        }
    }
    found.sort();
    found
}

/// A program that names one package, which uses another the program does not name (choice
/// 1576), fetched from a mirror (1582) at the versions `sg add` and `sg update` choose (1577),
/// kept in the cache (1579) and pinned by the lock (1571); fetched again when the cache has it
/// not, refused when its tag was moved (1558), and taken away by `sg remove`.
#[test]
fn fetches_packages_from_a_mirror() {
    let dir = work("fetches_packages_from_a_mirror");
    let (mirror, cache, program_dir) = (dir.join("mirror"), dir.join("cache"), dir.join("shop"));
    std::fs::create_dir_all(&program_dir).unwrap();
    let utf = |said: &str| format!("public string name() {{\n    return \"{said}\"\n}}\n");
    publish_sg(&mirror, "example.com/bob/utf", "1.0.0", "", "utf", &utf("utf 1.0"));
    publish_sg(&mirror, "example.com/bob/utf", "1.1.0", "", "utf", &utf("utf 1.1"));
    let xml = |said: &str| format!("use text\n\npublic string describe() {{\n    return $\"{said} on {{text.name()}}\".toString()\n}}\n");
    let xml_files = |requires: &str, code: &str| -> Vec<(String, String)> {
        vec![
            ("sg.project".into(), format!("package example.com/ada/xml\nrequire text example.com/bob/utf {requires}\n")),
            ("xml.sg".into(), code.to_string()),
            ("README.md".into(), "# xml\n".into()),
            // Another package kept in the repository is not this one's (choice 1581).
            ("examples/demo/sg.project".into(), "package example.com/ada/demo\n".into()),
            ("examples/demo/demo.sg".into(), "void main() {\n}\n".into()),
        ]
    };
    let put = |version: &str, files: Vec<(String, String)>| {
        let files: Vec<(&str, &str, &[u8])> = files.iter().map(|(path, code)| ("100644", path.as_str(), code.as_bytes())).collect();
        publish(&mirror, "example.com/ada/xml", version, &files);
    };
    put("1.4.0", xml_files("1.0.0", &xml("xml 1.4")));
    put("1.5.0", xml_files("1.1.0", &xml("xml 1.5")));
    write(&program_dir, "main.sg", "use xml\n\nvoid main() {\n    print(xml.describe())\n}\n");
    let run = |words: &[&str]| ran(&sg_with(&program_dir, &mirror, &cache, words));

    let (code, out, err) = run(&["add", "example.com/ada/xml@1.4"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "example.com/ada/xml 1.4.0 (added)\nexample.com/bob/utf 1.0.0 (added)\n");
    assert_eq!(std::fs::read_to_string(program_dir.join("sg.project")).unwrap(), "require xml example.com/ada/xml 1.4.0\n");
    let lock = std::fs::read_to_string(program_dir.join("sg.lock")).unwrap();
    let lines: Vec<&str> = lock.lines().collect();
    assert!(lines.len() == 3 && lines[0].starts_with("//") && lines[1].starts_with("example.com/ada/xml 1.4.0 sha256-") && lines[2].starts_with("example.com/bob/utf 1.0.0 sha256-"), "{lock}");
    assert_eq!(run(&["main.sg"]), (0, "xml 1.4 on utf 1.0\n".into(), String::new()));
    if has_compiler() {
        let out = dir.join("shop-built");
        assert_eq!(run(&["build", "main.sg", "-o", &out.to_string_lossy()]), (0, String::new(), String::new()));
        assert_eq!(ran(&Command::new(program(&dir, "shop-built")).output().unwrap()), (0, "xml 1.4 on utf 1.0\n".into(), String::new()));
    }

    // The files of the tag, read only, in a folder named by their SHA-256; the other package
    // kept in the repository is left out.
    let folders = cached(&cache);
    let xml_folder = folders.iter().find(|folder| folder.join("xml.sg").exists()).expect("xml is in the cache");
    assert!(lines[1].ends_with(&*xml_folder.file_name().unwrap().to_string_lossy()));
    assert!(xml_folder.join("README.md").exists() && !xml_folder.join("examples").exists());
    assert!(std::fs::metadata(xml_folder.join("xml.sg")).unwrap().permissions().readonly());
    #[cfg(unix)]
    assert!(std::fs::metadata(xml_folder).unwrap().permissions().readonly(), "the folder is read only too");
    assert!(xml_folder.parent().unwrap().ends_with("example.com/ada/xml@1.4.0"), "{xml_folder:?}");
    assert_eq!(folders.len(), 2, "{folders:?}");

    // The package the program does not name cannot be reached from it, by any name.
    write(&program_dir, "other.sg", "use utf_1\n\nvoid main() {\n    print(utf_1.name())\n}\n");
    assert_eq!(run(&["other.sg"]).0, 1);
    std::fs::remove_file(program_dir.join("other.sg")).unwrap();

    // Without the cache, what the lock holds is fetched again.
    removed(&cache);
    assert_eq!(run(&["main.sg"]), (0, "xml 1.4 on utf 1.0\n".into(), String::new()));

    let (code, out, err) = run(&["update", "xml"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "example.com/ada/xml 1.5.0 (1.4.0 before)\nexample.com/bob/utf 1.1.0 (1.0.0 before)\n");
    assert_eq!(std::fs::read_to_string(program_dir.join("sg.project")).unwrap(), "require xml example.com/ada/xml 1.5.0\n");
    assert_eq!(run(&["main.sg"]), (0, "xml 1.5 on utf 1.1\n".into(), String::new()));
    assert_eq!(run(&["update"]), (0, "every package is at its newest version\n".into(), String::new()));

    // A tag moved to other files no longer has the SHA-256 the lock holds.
    put("1.5.0", xml_files("1.1.0", &xml("xml 1.5, moved")));
    removed(&cache);
    let (code, _, err) = run(&["main.sg"]);
    assert_eq!(code, 2);
    assert!(err.contains("the tag v1.5.0 of example.com/ada/xml has other files than sg.lock holds") && err.contains("the tag was moved"), "{err}");

    let (code, out, err) = run(&["remove", "xml"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "example.com/ada/xml 1.5.0 (taken away)\nexample.com/bob/utf 1.1.0 (taken away)\n");
    assert_eq!(std::fs::read_to_string(program_dir.join("sg.project")).unwrap(), "");
    assert!(!program_dir.join("sg.lock").exists());
}

/// The lock is only read when a program runs, and one that does not fit `sg.project` stops it
/// with 2 and the line; `sg lock` makes it fit without raising a version (choice 1580).
#[test]
fn the_lock_fits_the_project() {
    let dir = work("the_lock_fits_the_project");
    let (mirror, cache, program_dir) = (dir.join("mirror"), dir.join("cache"), dir.join("program"));
    std::fs::create_dir_all(&program_dir).unwrap();
    for version in ["1.0.0", "1.1.0"] {
        publish_sg(&mirror, "example.com/bob/utf", version, "", "utf", &format!("public string name() {{\n    return \"utf {version}\"\n}}\n"));
    }
    write(&program_dir, "main.sg", "use utf\n\nvoid main() {\n    print(utf.name())\n}\n");
    let run = |words: &[&str]| ran(&sg_with(&program_dir, &mirror, &cache, words));
    let stops = |said: &str| {
        let (code, out, err) = run(&["main.sg"]);
        assert!(code == 2 && out.is_empty() && err.contains(said) && err.contains("`sg lock` writes it again from `sg.project`"), "{said}: {err}");
    };
    write(&program_dir, "sg.project", "require utf example.com/bob/utf 1.0.0\n");
    stops("sg.lock has no example.com/bob/utf 1.0.0");
    assert_eq!(run(&["lock"]), (0, "example.com/bob/utf 1.0.0 (added)\n".into(), String::new()));
    assert_eq!(run(&["main.sg"]), (0, "utf 1.0.0\n".into(), String::new()));
    write(&program_dir, "sg.project", "require utf example.com/bob/utf 1.1.0\n");
    stops("sg.lock has example.com/bob/utf 1.0.0, older than the 1.1.0 the line asks for");
    assert_eq!(run(&["lock"]), (0, "example.com/bob/utf 1.1.0 (1.0.0 before)\n".into(), String::new()));
    assert_eq!(run(&["main.sg"]), (0, "utf 1.1.0\n".into(), String::new()));
    write(&program_dir, "sg.project", "// nothing\n");
    write(&program_dir, "main.sg", "void main() {\n    print(1)\n}\n");
    stops("example.com/bob/utf 1.1.0 is used by nothing");
    write(&program_dir, "sg.lock", "example.com/bob/utf 1.1.0 sha256-12\n");
    stops("sg.lock:1: `sha256-12` is not a SHA-256");
    // The commands that are not usages, and a version no tag has.
    assert_eq!(run(&["lock", "x"]).0, 2);
    assert_eq!(run(&["add"]).0, 2);
    let (code, _, err) = run(&["add", "example.com/bob/utf@2"]);
    assert!(code == 1 && err.contains("example.com/bob/utf has no version that fits `example.com/bob/utf@2`: its versions are 1.0.0, 1.1.0"), "{err}");
    let (code, _, err) = run(&["add", "example.com/bob/nothing"]);
    assert!(code == 1 && err.contains("example.com/bob/nothing is not in the mirror SG_MIRROR names"), "{err}");
}

/// What git lists is checked before a file is written (choices 1573, 1574): a link, a
/// submodule, two names that differ only in case, a name Windows keeps and a file too large are
/// refused, and nothing is written.
#[test]
fn refuses_a_package_not_every_system_can_hold() {
    let dir = work("refuses_a_package_not_every_system_can_hold");
    let (mirror, cache, program_dir) = (dir.join("mirror"), dir.join("cache"), dir.join("program"));
    std::fs::create_dir_all(&program_dir).unwrap();
    let project = b"package example.com/eve/bad\n".as_slice();
    let code = b"public int one() {\n    return 1\n}\n".as_slice();
    let large = vec![b'x'; (16 << 20) + 1];
    // The version, its files (mode, path, bytes) and what the message says.
    type Case<'a> = (&'a str, Vec<(&'a str, &'a str, &'a [u8])>, &'a str);
    let cases: &[Case] = &[
        ("1.0.0", vec![("120000", "bad.sg", b"/etc/passwd".as_slice())], "bad.sg is a link, and a package has none"),
        ("1.1.0", vec![("160000", "inner", b"".as_slice())], "inner is a submodule, and a package has none"),
        ("1.2.0", vec![("100644", "Bad.sg", code), ("100644", "bad.sg", code)], "Bad.sg and bad.sg differ only in case"),
        ("1.3.0", vec![("100644", "lib/con.sg", code)], "has a name Windows keeps for a device"),
        ("1.4.0", vec![("100644", "data.bin", large.as_slice())], "data.bin has 16777217 bytes, more than the 16 MB a file of a package may have"),
    ];
    for (version, files, said) in cases {
        let mut files = files.clone();
        files.push(("100644", "sg.project", project));
        // A submodule's line names a commit, which need not be in the repository.
        let files: Vec<(&str, &str, Vec<u8>)> =
            files.iter().map(|(mode, path, bytes)| (*mode, *path, if *mode == "160000" { Vec::new() } else { bytes.to_vec() })).collect();
        let repository = mirror.join("example.com/eve/bad");
        if !repository.exists() {
            std::fs::create_dir_all(&repository).unwrap();
            git(&repository, &["init", "-q", "--bare"], None, b"");
        }
        let index = mirror.join("index");
        let _ = std::fs::remove_file(&index);
        for (mode, path, bytes) in &files {
            let object = match *mode {
                "160000" => "1111111111111111111111111111111111111111".to_string(),
                _ => git(&repository, &["hash-object", "-w", "--stdin", "--no-filters"], None, bytes),
            };
            git(&repository, &["update-index", "--add", "--cacheinfo", &format!("{mode},{object},{path}")], Some(&index), b"");
        }
        let tree = git(&repository, &["write-tree"], Some(&index), b"");
        let commit = git(&repository, &["commit-tree", &tree, "-m", version], None, b"");
        git(&repository, &["update-ref", &format!("refs/tags/v{version}"), &commit], None, b"");
        let (code, out, err) = ran(&sg_with(&program_dir, &mirror, &cache, &["add", &format!("example.com/eve/bad@{version}")]));
        assert!(code == 1 && out.is_empty() && err.contains(&format!("example.com/eve/bad {version} is refused: ")) && err.contains(said), "{version}: {err}");
        assert!(!program_dir.join("sg.project").exists() && !program_dir.join("sg.lock").exists());
        assert_eq!(cached(&cache), Vec::<PathBuf>::new(), "{version}");
    }
}

/// `unsafe` in a package is allowed only by the program (choices 1562, 1578, 1583): on the
/// package's own `require` line, also for one the program uses through another; a new version
/// that starts to use it stops `sg update`.
#[test]
fn unsafe_in_a_package_is_the_programs_to_allow() {
    let dir = work("unsafe_in_a_package_is_the_programs_to_allow");
    let (mirror, cache, program_dir) = (dir.join("mirror"), dir.join("cache"), dir.join("program"));
    std::fs::create_dir_all(&program_dir).unwrap();
    publish_sg(&mirror, "example.com/bo/zlib", "1.0.0", "", "zlib", "public int seven() {\n    return 7\n}\n");
    let raw = "public int seven() {\n    var cells = Memory.allocate<int>(1)\n    var seen = 0\n    unsafe {\n        cells[0] = 7\n        seen = cells[0]\n        Memory.free(cells)\n    }\n    return seen\n}\n";
    publish_sg(&mirror, "example.com/bo/zlib", "1.1.0", "", "zlib", raw);
    publish_sg(&mirror, "example.com/ada/wrap", "1.0.0", "require zlib example.com/bo/zlib 1.1.0\n", "wrap", "use zlib\n\npublic int eight() {\n    return zlib.seven() + 1\n}\n");
    let run = |words: &[&str]| ran(&sg_with(&program_dir, &mirror, &cache, words));
    write(&program_dir, "main.sg", "use zlib\n\nvoid main() {\n    print(zlib.seven())\n}\n");
    assert_eq!(run(&["add", "example.com/bo/zlib@1.0"]).0, 0);
    assert_eq!(run(&["main.sg"]), (0, "7\n".into(), String::new()));
    let (code, _, err) = run(&["update", "zlib"]);
    assert!(code == 1 && err.contains("example.com/bo/zlib 1.1.0 uses `unsafe`, which the version before did not, and nothing is written"), "{err}");
    assert_eq!(std::fs::read_to_string(program_dir.join("sg.project")).unwrap(), "require zlib example.com/bo/zlib 1.0.0\n");

    write(&program_dir, "sg.project", "require zlib example.com/bo/zlib 1.1.0\n");
    let (code, out, _) = run(&["lock"]);
    assert_eq!((code, out.as_str()), (0, "example.com/bo/zlib 1.1.0 (1.0.0 before, uses `unsafe`)\n    the program builds with it once a `require` line of it ends in `unsafe`: `require zlib example.com/bo/zlib 1.1.0 unsafe`\n"));
    assert!(std::fs::read_to_string(program_dir.join("sg.lock")).unwrap().contains(" unsafe\n"));
    let (code, _, err) = run(&["main.sg"]);
    assert!(code == 1 && err.contains("`unsafe` is in the package example.com/bo/zlib, and the program's `sg.project` does not allow it"), "{err}");
    assert!(err.contains("by ending the package's `require` line in `unsafe`: `require zlib example.com/bo/zlib 1.1.0 unsafe`"), "{err}");
    write(&program_dir, "sg.project", "require zlib example.com/bo/zlib 1.1.0 unsafe\n");
    assert_eq!(run(&["main.sg"]), (0, "7\n".into(), String::new()));

    // Through another package: the program names it on a line of its own.
    write(&program_dir, "sg.project", "require wrap example.com/ada/wrap 1.0.0\n");
    write(&program_dir, "main.sg", "use wrap\n\nvoid main() {\n    print(wrap.eight())\n}\n");
    assert_eq!(run(&["lock"]).0, 0);
    let (code, _, err) = run(&["main.sg"]);
    assert!(code == 1 && err.contains("through another package, and allows it on a `require` line of its own that ends in `unsafe`: `require zlib example.com/bo/zlib 1.1.0 unsafe`"), "{err}");
    write(&program_dir, "sg.project", "require wrap example.com/ada/wrap 1.0.0\nrequire zlib example.com/bo/zlib 1.1.0 unsafe\n");
    assert_eq!(run(&["main.sg"]), (0, "8\n".into(), String::new()));
}

/// Git runs in a folder of `sg`'s own and finds no repository there (choice 1591): a project
/// folder that holds the files of a bare repository, its `config` among them, and the `GIT_`
/// variables of the user's are not read. Both here would send git to a folder that is not there.
#[test]
fn git_reads_no_settings_of_the_project_folder() {
    let dir = work("git_reads_no_settings_of_the_project_folder");
    let (mirror, cache, program_dir) = (dir.join("mirror"), dir.join("cache"), dir.join("program"));
    std::fs::create_dir_all(program_dir.join("objects")).unwrap();
    std::fs::create_dir_all(program_dir.join("refs")).unwrap();
    publish_sg(&mirror, "example.com/bob/utf", "1.0.0", "", "utf", "public int one() {\n    return 1\n}\n");
    let absolute = std::path::absolute(&mirror).unwrap();
    let elsewhere = format!("[url \"{}/\"]\n\tinsteadOf = {}/\n", dir.join("nowhere").display(), absolute.display()).replace('\\', "/");
    write(&program_dir, "HEAD", "ref: refs/heads/main\n");
    write(&program_dir, "config", &format!("[core]\n\trepositoryformatversion = 0\n\tbare = true\n{elsewhere}"));
    write(&program_dir, "main.sg", "use utf\n\nvoid main() {\n    print(utf.one())\n}\n");
    let output = Command::new(env!("CARGO_BIN_EXE_sg"))
        .args(["add", "example.com/bob/utf"])
        .current_dir(&program_dir)
        .env("SG_MIRROR", &mirror)
        .env("SG_CACHE", &cache)
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", format!("url.{}/.insteadOf", dir.join("nowhere").display()))
        .env("GIT_CONFIG_VALUE_0", format!("{}/", absolute.display()))
        .env("GIT_DIR", dir.join("nowhere"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let (code, out, err) = ran(&output);
    assert_eq!((code, out.as_str()), (0, "example.com/bob/utf 1.0.0 (added)\n"), "{err}");
    assert_eq!(ran(&sg_with(&program_dir, &mirror, &cache, &["main.sg"])), (0, "1\n".into(), String::new()));
}

/// `sg.project` and `sg.lock` are written into a new file put in their place, so a link there is
/// replaced and not followed (choice 1591); the name of a library a package gives is written in
/// the lock only when the checker takes it.
#[cfg(unix)]
#[test]
fn writes_the_lock_in_place_of_a_link() {
    let dir = work("writes_the_lock_in_place_of_a_link");
    let (mirror, cache, program_dir, outside) = (dir.join("mirror"), dir.join("cache"), dir.join("program"), dir.join("outside"));
    std::fs::create_dir_all(&program_dir).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let code = "extern \"z x\" {\n    int crc(int a)\n}\n\nextern \"z\" {\n    int adler(int a)\n}\n\npublic int one() {\n    return 1\n}\n";
    publish_sg(&mirror, "example.com/bob/utf", "1.0.0", "", "utf", code);
    std::os::unix::fs::symlink(outside.join("lock"), program_dir.join("sg.lock")).unwrap();
    std::os::unix::fs::symlink(outside.join("project"), program_dir.join("sg.project")).unwrap();
    let (code, out, err) = ran(&sg_with(&program_dir, &mirror, &cache, &["add", "example.com/bob/utf"]));
    assert_eq!(code, 0, "{err}");
    assert!(out.starts_with("example.com/bob/utf 1.0.0 (added, uses `unsafe`, links the C libraries z)\n"), "{out}");
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0, "nothing is written through the links");
    for name in ["sg.lock", "sg.project"] {
        assert!(std::fs::symlink_metadata(program_dir.join(name)).unwrap().is_file(), "{name}");
    }
    assert!(std::fs::read_to_string(program_dir.join("sg.lock")).unwrap().ends_with(" unsafe c=z\n"));
    assert_eq!(std::fs::read_dir(&program_dir).unwrap().count(), 2, "no file is left beside them");
}

/// A package goes into the cache only once its SHA-256 is the lock's and its `package` line its
/// address, and under its address and version (choice 1591): files fetched from another address
/// are never taken for it.
#[test]
fn keeps_a_package_only_under_its_own_address() {
    let dir = work("keeps_a_package_only_under_its_own_address");
    let (mirror, cache, program_dir) = (dir.join("mirror"), dir.join("cache"), dir.join("program"));
    std::fs::create_dir_all(&program_dir).unwrap();
    let files: &[(&str, &str, &[u8])] = &[("100644", "sg.project", b"package example.com/ada/lib\n"), ("100644", "lib.sg", b"public int value() {\n    return 666\n}\n")];
    publish(&mirror, "example.com/eve/seed", "1.0.0", files);
    let (code, _, err) = ran(&sg_with(&program_dir, &mirror, &cache, &["add", "example.com/eve/seed"]));
    assert!(code == 1 && err.contains("example.com/eve/seed 1.0.0: sg.project: the package is example.com/ada/lib, and it was asked for as example.com/eve/seed"), "{err}");
    assert_eq!(cached(&cache), Vec::<PathBuf>::new());

    // The same files under the address they say, and a lock that holds their SHA-256 for it.
    publish(&mirror, "example.com/ada/lib", "1.0.0", &[("100644", "sg.project", b"package example.com/ada/lib\n"), ("100644", "lib.sg", b"public int value() {\n    return 7\n}\n")]);
    write(&program_dir, "main.sg", "use lib\n\nvoid main() {\n    print(lib.value())\n}\n");
    assert_eq!(ran(&sg_with(&program_dir, &mirror, &cache, &["add", "example.com/ada/lib"])).0, 0);
    let lock = std::fs::read_to_string(program_dir.join("sg.lock")).unwrap();
    let mut seed = Vec::new();
    for (_, path, bytes) in files {
        seed.push((*path, *bytes));
    }
    let seeded = sg::fetch::tree_sum(&mut seed);
    let good = lock.lines().nth(1).unwrap().split(' ').nth(2).unwrap().to_string();
    write(&program_dir, "sg.lock", &lock.replace(&good, &seeded));
    let (code, _, err) = ran(&sg_with(&program_dir, &mirror, &cache, &["main.sg"]));
    assert!(code == 2 && err.contains("the tag v1.0.0 of example.com/ada/lib has other files than sg.lock holds"), "{err}");
    assert_eq!(cached(&cache).len(), 1);
}

/// What git lists of a tag is read as it comes and stopped at the first limit passed (choice
/// 1591): a tree of a few kilobytes that names a million files is refused at once.
#[test]
fn stops_reading_a_tree_that_names_too_many_files() {
    let dir = work("stops_reading_a_tree_that_names_too_many_files");
    let (mirror, cache, program_dir) = (dir.join("mirror"), dir.join("cache"), dir.join("program"));
    std::fs::create_dir_all(&program_dir).unwrap();
    let repository = mirror.join("example.com/eve/bomb");
    std::fs::create_dir_all(&repository).unwrap();
    git(&repository, &["init", "-q", "--bare"], None, b"");
    let blob = git(&repository, &["hash-object", "-w", "--stdin", "--no-filters"], None, b"x");
    // Sixteen names for one tree, five times over: 16^5 paths.
    let mut tree = git(&repository, &["mktree"], None, (0..16).map(|at| format!("100644 blob {blob}\tf{at:x}.sg\n")).collect::<String>().as_bytes());
    for _ in 0..4 {
        tree = git(&repository, &["mktree"], None, (0..16).map(|at| format!("040000 tree {tree}\td{at:x}\n")).collect::<String>().as_bytes());
    }
    let project = git(&repository, &["hash-object", "-w", "--stdin", "--no-filters"], None, b"package example.com/eve/bomb\n");
    tree = git(&repository, &["mktree"], None, format!("040000 tree {tree}\tbomb\n100644 blob {project}\tsg.project\n").as_bytes());
    let commit = git(&repository, &["commit-tree", &tree, "-m", "bomb"], None, b"");
    git(&repository, &["update-ref", "refs/tags/v1.0.0", &commit], None, b"");
    let started = std::time::Instant::now();
    let (code, _, err) = ran(&sg_with(&program_dir, &mirror, &cache, &["add", "example.com/eve/bomb"]));
    assert!(code == 1 && err.contains("git lists more than the 20,000 files `sg` reads of a tag"), "{err}");
    assert!(started.elapsed() < std::time::Duration::from_secs(60), "{:?}", started.elapsed());
    assert_eq!(cached(&cache), Vec::<PathBuf>::new());
}

/// Two names of a package the system takes for one folder are refused when it is kept, whatever
/// the system folds (choice 1591): `ſtock` is `stock` on macOS, and not on Linux.
#[test]
fn refuses_two_names_the_system_takes_for_one() {
    let dir = work("refuses_two_names_the_system_takes_for_one");
    let (mirror, cache, program_dir) = (dir.join("mirror"), dir.join("cache"), dir.join("program"));
    std::fs::create_dir_all(&program_dir).unwrap();
    let probe = dir.join("probe");
    std::fs::create_dir_all(probe.join("stock")).unwrap();
    let folds = probe.join("\u{17f}tock").exists();
    let code = b"public int one() {\n    return 1\n}\n".as_slice();
    publish(&mirror, "example.com/eve/fold", "1.0.0", &[("100644", "sg.project", b"package example.com/eve/fold\n"), ("100644", "stock/a.sg", code), ("100644", "\u{17f}tock/b.sg", code)]);
    let (code, _, err) = ran(&sg_with(&program_dir, &mirror, &cache, &["add", "example.com/eve/fold"]));
    match folds {
        true => {
            assert!(code == 1 && err.contains("example.com/eve/fold 1.0.0 cannot be kept on this system: \u{17f}tock is the name of another of its files or folders here"), "{err}");
            assert_eq!(cached(&cache), Vec::<PathBuf>::new());
        }
        false => assert_eq!(code, 0, "{err}"),
    }
}

/// A package cannot have the name of a type of the language, nor a folder of modules
/// (choice 1591): `Hex.encode` is the type's, in the program and inside the language's own module.
#[test]
fn a_package_is_not_named_as_a_type() {
    let dir = work("a_package_is_not_named_as_a_type");
    let (mirror, cache, program_dir) = (dir.join("mirror"), dir.join("cache"), dir.join("program"));
    std::fs::create_dir_all(&program_dir).unwrap();
    publish_sg(&mirror, "example.com/eve/Hex", "1.0.0", "", "hex", "public string encode(read byte[] data) {\n    return \"taken\"\n}\n");
    write(&program_dir, "main.sg", "void main() {\n    print(Hex.encode(\"Hi\".asBytes()))\n}\n");
    let run = |words: &[&str]| ran(&sg_with(&program_dir, &mirror, &cache, words));
    assert_eq!(run(&["add", "example.com/eve/Hex"]).0, 0);
    assert_eq!(std::fs::read_to_string(program_dir.join("sg.project")).unwrap(), "require hex example.com/eve/Hex 1.0.0\n");
    assert_eq!(run(&["main.sg"]), (0, "4869\n".into(), String::new()));
    write(&program_dir, "sg.project", "require Hex example.com/eve/Hex 1.0.0\n");
    let (code, _, err) = run(&["main.sg"]);
    assert!(code == 2 && err.contains("`Hex` is a type of the language, and a package cannot have its name; give the package a name in front of its address: `require hex example.com/eve/Hex 1.0.0`"), "{err}");
    std::fs::remove_file(program_dir.join("sg.project")).unwrap();
    std::fs::remove_file(program_dir.join("sg.lock")).unwrap();
    std::fs::create_dir_all(program_dir.join("Hex")).unwrap();
    write(&program_dir.join("Hex"), "hex.sg", "public string encode(read byte[] data) {\n    return \"taken\"\n}\n");
    let (code, _, err) = run(&["main.sg"]);
    assert!(code == 2 && err.contains("the folder Hex has the name of a type of the language, and a folder of modules cannot have it"), "{err}");
}
