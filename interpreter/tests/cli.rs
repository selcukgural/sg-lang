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
    Command::new(env!("CARGO_BIN_EXE_sg")).args(words).current_dir(dir).stdin(Stdio::null()).finished()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The exit code, and what was written to the output and to standard error.
fn ran(out: &Output) -> (i32, String, String) {
    (out.status.code().unwrap_or(-1), text(&out.stdout), text(&out.stderr))
}

/// The longest a program of these tests may take: far more than any does, so one that is still
/// going waits for something that never comes, and the test fails saying which, rather than
/// holding the run for ever (a compiled trial of the network once waited 46 minutes).
const PROGRAM_LIMIT: std::time::Duration = std::time::Duration::from_secs(300);

/// `output()` with `PROGRAM_LIMIT`: what it wrote is captured; one past the limit is killed, and
/// the test panics with its command line.
trait Finished {
    fn finished(&mut self) -> Output;
}

impl Finished for Command {
    fn finished(&mut self) -> Output {
        use std::io::Read;
        let mut child = self.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let take = |pipe: Option<Box<dyn Read + Send>>| {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                if let Some(mut pipe) = pipe {
                    let _ = pipe.read_to_end(&mut bytes);
                }
                bytes
            })
        };
        let out = take(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
        let err = take(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
        let start = std::time::Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if start.elapsed() > PROGRAM_LIMIT {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{:?} {:?} did not end within {} s", self.get_program(), self.get_args().collect::<Vec<_>>(), PROGRAM_LIMIT.as_secs());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        Output { status, stdout: out.join().unwrap(), stderr: err.join().unwrap() }
    }
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

/// `isTerminal()` on a real terminal (choice 1901), interpreted and compiled: `script` runs the
/// program on a terminal of its own (a pty), where the input, the output, the standard error and
/// `/dev/tty` opened as a file are terminals; sent into files they are not, `/dev/tty` still is.
/// Skipped where there is no `script`.
#[cfg(unix)]
#[test]
fn streams_on_a_terminal() {
    if Command::new("script").arg("-V").stdin(Stdio::null()).output().is_err() {
        eprintln!("no `script`: streams on a terminal are not tested");
        return;
    }
    let dir = work("streams_on_a_terminal");
    let source = "Result<void, IoError> main() {\n    var tty = try File.openText(\"/dev/tty\")\n    var back = try File.openAppend(\"/dev/tty\")\n    printError($\"err {Stderr.isTerminal()}\")\n    print($\"in {Stdin.isTerminal()} out {Stdout.isTerminal()} tty {tty.isTerminal()} {back.isTerminal()}\")\n    try back.close()\n    return Ok()\n}\n";
    for (command, way) in both_ways(&dir, &[("terminal", source)]) {
        let program = std::iter::once(command.get_program()).chain(command.get_args()).map(|word| format!("'{}'", word.to_string_lossy())).collect::<Vec<_>>().join(" ");
        for (line, redirected) in [("", false), (" < /dev/null > out.txt 2> err.txt", true)] {
            let line = format!("{program}{line}");
            // macOS's `script` takes the command as its words, util-linux's as `-c`.
            let mut script = Command::new("script");
            match cfg!(target_os = "macos") {
                true => script.args(["-q", "/dev/null", "sh", "-c", &line]),
                false => script.args(["-qec", &line, "/dev/null"]),
            };
            let out = script.current_dir(&dir).stdin(Stdio::null()).finished();
            let shown = text(&out.stdout);
            match redirected {
                false => {
                    assert!(shown.contains("in true out true tty true true"), "{way}: {shown:?}");
                    assert!(shown.contains("err true"), "{way}: {shown:?}");
                }
                true => {
                    assert_eq!(std::fs::read_to_string(dir.join("out.txt")).unwrap(), "in false out false tty true true\n", "{way}: {shown:?}");
                    assert_eq!(std::fs::read_to_string(dir.join("err.txt")).unwrap(), "err false\n", "{way}: {shown:?}");
                }
            }
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

/// Calls nested past the stack, in `main` and in a task: compiled code has a stack of 1 GB
/// (choice 1821) and says so when it is full, where it ended with a SIGSEGV (choice 1815). The
/// interpreter's bound follows its own stack of 1 GB (choice 1823), where it was 5,000 calls,
/// and its panic says the same.
#[test]
fn a_stack_that_overflows_is_told() {
    if !has_compiler() {
        return;
    }
    let dir = work("a_stack_that_overflows_is_told");
    // `+ 1` after the call, so that it is no tail call a C compiler turns into a loop.
    let down = "int down(int n) {\n    return down(n + 1) + 1\n}\n\n";
    write(&dir, "deep.sg", &format!("{down}void main() {{\n    print(down(0))\n}}\n"));
    write(&dir, "task.sg", &format!("{down}int work() {{\n    return down(0)\n}}\n\nvoid main() {{\n    var task = Task.run(work)\n    print(task.wait())\n}}\n"));
    // A million calls deep fit, where 8 MB held about a hundred thousand.
    write(&dir, "fits.sg", "int sum(int n) {\n    if n == 0 {\n        return 0\n    }\n    return sum(n - 1) + 1\n}\n\nvoid main() {\n    print(sum(1000000))\n}\n");
    for stem in ["deep", "task"] {
        assert_eq!(ran(&sg(&dir, &["build", &format!("{stem}.sg")])).0, 0);
        let (code, out, err) = ran(&Command::new(program(&dir, stem)).output().unwrap());
        assert_eq!((code, out.as_str()), (1, ""), "{stem}: {err}");
        assert!(err.starts_with("panic: stack overflow: calls are nested deeper than the stack of this thread holds"), "{stem}: {err}");
    }
    assert_eq!(ran(&sg(&dir, &["build", "fits.sg"])).0, 0);
    assert_eq!(ran(&Command::new(program(&dir, "fits")).output().unwrap()), (0, "1000000\n".into(), String::new()));
    for stem in ["deep", "task"] {
        let (code, out, err) = ran(&sg(&dir, &[&format!("{stem}.sg")]));
        assert_eq!((code, out.as_str()), (1, ""), "{stem}: {err}");
        assert!(err.starts_with("panic: stack overflow: calls are nested deeper than the stack of this thread holds\n  last called: `down`"), "{stem}: {err}");
    }
    // A hundred thousand calls fit in the interpreter: a call takes it about 2.5 KB of its stack.
    write(&dir, "fits_interpreted.sg", "int sum(int n) {\n    if n == 0 {\n        return 0\n    }\n    return sum(n - 1) + 1\n}\n\nvoid main() {\n    print(sum(100000))\n}\n");
    assert_eq!(ran(&sg(&dir, &["fits_interpreted.sg"])), (0, "100000\n".into(), String::new()));
}

/// Where the system gives no thread a stack of 1 GB, compiled code runs `main` on the stack it
/// has; `ok_json_stack_low` on 1 MB of it reads and writes the deep part of its texts with the list
/// of `Json.parse` and `toJson` (choices 1832, 1833), and prints what the recursion prints on 1 GB.
/// That the stack is that small is seen from a recursion a million calls deep, which fits 1 GB.
#[cfg(target_os = "linux")]
#[test]
fn json_compiled_goes_on_with_a_list_where_the_stack_runs_low() {
    if !has_compiler() {
        return;
    }
    let dir = work("json_compiled_goes_on_with_a_list_where_the_stack_runs_low");
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let source = std::fs::read_to_string(cases.join("ok_json_stack_low.sg")).unwrap();
    let expected = std::fs::read_to_string(cases.join("ok_json_stack_low.out")).unwrap();
    write(&dir, "json.sg", &source);
    write(&dir, "fits.sg", "int sum(int n) {\n    if n == 0 {\n        return 0\n    }\n    return sum(n - 1) + 1\n}\n\nvoid main() {\n    print(sum(1000000))\n}\n");
    // 800 MB of address space leaves no room for a stack of 1 GB.
    let small = |stem: &str| {
        let line = format!("ulimit -s 1024 && ulimit -v 800000 && exec '{}'", program(&dir, stem).display());
        ran(&Command::new("sh").arg("-c").arg(line).output().unwrap())
    };
    for release in [false, true] {
        let mut words = vec!["build", "json.sg"];
        if release {
            words.push("--release");
        }
        assert_eq!(ran(&sg(&dir, &words)).0, 0);
        assert_eq!(small("json"), (0, expected.clone(), String::new()), "release: {release}");
    }
    assert_eq!(ran(&sg(&dir, &["build", "fits.sg"])).0, 0);
    let (code, out, err) = small("fits");
    assert_eq!((code, out.as_str()), (1, ""));
    assert!(err.starts_with("panic: stack overflow"), "{err}");
}

/// A `try` inside a statement is computed first, under a name, only where all that comes before
/// it reads (choice 1817); elsewhere compiled code asks for a name, as before.
#[test]
fn a_try_is_computed_first_only_after_reads() {
    let dir = work("a_try_is_computed_first_only_after_reads");
    let head = "Result<string, string> word(int x) {\n    return Ok(\"w\")\n}\n\nint mark() {\n    return 1\n}\n\nResult<int, string> fill(ref List<string> given) {\n    var xs = List<string>()\n    var lists = List<List<string>>()\n    var n = 1\n    var s = \"owned\"\n    var m = HashMap<int, string>()\n    print($\"{xs.length()} {lists.length()} {n} {s} {given.length()} {m.length()}\")\n";
    let tail = "    return Ok(0)\n}\n\nvoid main() {\n    var g = List<string>()\n    _ = fill(ref g)\n}\n";
    let cases = [
        ("xs.add(try word(n))", true),
        ("given.add(try word(n))", true),
        ("m.add(7, try word(n))", true),
        // An index may panic, a call does something, a value is moved, `n` is named by the `try`.
        ("lists[0].add(try word(n))", false),
        ("m.add(mark(), try word(n))", false),
        ("lists.add(List<string> { s, try word(n) })", false),
        ("xs.add(try word(n + 0 * (int)xs.length()))", false),
    ];
    for (i, (line, compiled)) in cases.iter().enumerate() {
        let file = format!("case{i}.sg");
        write(&dir, &file, &format!("{head}    {line}\n{tail}"));
        let (code, _, err) = ran(&sg(&dir, &["emit-c", &file]));
        if *compiled {
            assert_eq!((code, err.as_str()), (0, ""), "{line}");
        } else {
            assert_eq!(code, 1, "{line}");
            assert!(err.contains("is not in the C back end yet"), "{line}: {err}");
        }
    }
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

/// A program started by a program has only 0, 1 and 2 open, whatever was handed to the one that
/// starts it: with glibc 2.34 the spawn closes the others; with a C library that cannot (musl,
/// glibc before 2.34) they are marked close-on-exec first (choice 1932). Built by `sg build`, and
/// with `musl-gcc` where it is installed, which asks the second way.
#[cfg(target_os = "linux")]
#[test]
fn a_started_program_gets_no_other_descriptor() {
    if !has_compiler() {
        return;
    }
    let dir = work("a_started_program_gets_no_other_descriptor");
    write(&dir, "fds.sg", "Result<void, ProcessError> main() {\n    var ls = Command(\"ls\")\n    ls.args(read List<string> { \"/proc/self/fd\" })\n    print(try ls.output())\n    return Ok()\n}\n");
    assert_eq!(ran(&sg(&dir, &["build", "fds.sg", "-o", "glibc"])), (0, String::new(), String::new()));
    let mut programs = vec!["glibc"];
    // The C of the program linked with musl, statically: Debian's `musl-gcc` links a program
    // that loads musl only where libgcc asks nothing of glibc (not on 64-bit Arm).
    let musl = std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join("musl-gcc").is_file()));
    if musl {
        let c = sg(&dir, &["emit-c", "fds.sg"]);
        std::fs::write(dir.join("fds.c"), &c.stdout).unwrap();
        let built = Command::new("musl-gcc").args(["-std=c11", "-O1", "-o", "musl", "fds.c", "-lm", "-pthread", "-static"]).current_dir(&dir).stdin(Stdio::null()).finished();
        assert!(built.status.success(), "{}", text(&built.stderr));
        programs.push("musl");
    } else {
        eprintln!("no musl-gcc: a C library without closefrom is not asked");
    }
    for program in programs {
        // Descriptor 7 is open, not close-on-exec, when the program starts.
        let run = format!("exec 7</dev/null; exec ./{program}");
        let (code, out, err) = ran(&Command::new("sh").args(["-c", &run]).current_dir(&dir).stdin(Stdio::null()).finished());
        assert_eq!((code, err.as_str()), (0, ""), "{program}");
        // `ls` reads the folder through one of its own (3).
        let open: Vec<&str> = out.lines().collect();
        assert!(open.starts_with(&["0", "1", "2"]) && !open.contains(&"7"), "{program}: {open:?}");
    }
}

/// The flags of C after `--` (choices 1935-1939): given to every build after sg's own; some said
/// on every build with a warning; what sg owns refused, 2, before anything is compiled; one the C
/// compiler does not take named. They change nothing of what SG checks: a debug build with `-O3`
/// still stops on an overflow, and a release build with `-O0` still wraps.
#[test]
fn builds_with_flags_of_c() {
    if !has_compiler() {
        return;
    }
    let dir = work("builds_with_flags_of_c");
    write(&dir, "hello.sg", HELLO);
    write(&dir, "calc.sg", LIBRARY);
    write(&dir, "checked.sg", "void main() {\n    print(\"main\")\n}\n\ntest \"adds\" {\n    assert(1 + 1 == 2)\n}\n");
    write(&dir, "wraps.sg", "void main() {\n    byte level = 250\n    for _ in 0..10 {\n        level += 1\n    }\n    print(level)\n}\n");
    for words in [
        &["build", "hello.sg", "-o", "fast", "--", "-O3", "-g"][..],
        &["build", "checked.sg", "--test", "-o", "tests", "--", "-O1"],
        &["build", "calc.sg", "--library", "--", "-O3"],
        &["build", "calc.sg", "--shared", "--", "-O1"],
    ] {
        let (code, _, err) = ran(&sg(&dir, words));
        assert_eq!((code, err.as_str()), (0, ""), "{words:?}");
    }
    assert_eq!(ran(&Command::new(program(&dir, "fast")).stdin(Stdio::null()).finished()).1, "hello\n");
    assert_eq!(ran(&Command::new(program(&dir, "tests")).stdin(Stdio::null()).finished()).0, 0);
    // Said on every build, one line each, and built.
    for _ in 0..2 {
        let (code, _, err) = ran(&sg(&dir, &["build", "hello.sg", "-o", "warned", "--", "-ffast-math", "-DSG_THREAD_STACK=((size_t)8<<20)"]));
        assert_eq!(code, 0, "{err}");
        let said: Vec<&str> = err.lines().filter(|line| line.starts_with("warning: -")).collect();
        assert_eq!(said.len(), 2, "{err}");
        assert!(said[0].starts_with("warning: -ffast-math: fractional results may differ from the interpreter's and between machines"), "{err}");
        assert!(said[1].starts_with("warning: -DSG_THREAD_STACK=((size_t)8<<20): it changes a setting of SG's runtime"), "{err}");
    }
    // Refused before anything is compiled.
    for flag in ["-o", "-c", "-std=c99", "-shared", "-m32", "-funsigned-char", "--target=x86_64-linux-gnu"] {
        let (code, out, err) = ran(&sg(&dir, &["build", "hello.sg", "-o", "refused", "--", flag, "x"]));
        assert_eq!((code, out.as_str()), (2, ""), "{flag}: {err}");
        assert!(err.starts_with(&format!("error: {flag} in the C flags: ")), "{flag}: {err}");
        assert!(!program(&dir, "refused").exists(), "{flag}");
    }
    let (code, _, err) = ran(&sg(&dir, &["build", "hello.sg", "-o", "unknown", "--", "-fno-such-flag-of-c"]));
    assert_eq!(code, 1, "{err}");
    assert!(err.ends_with("error: the C compiler did not build the program with the C flags given after `--`: -fno-such-flag-of-c\n"), "{err}");
    // SG's checks are the build's, not the C compiler's (1939).
    assert_eq!(ran(&sg(&dir, &["build", "wraps.sg", "-o", "checked", "--", "-O3"])).0, 0);
    let (code, _, err) = ran(&Command::new(program(&dir, "checked")).stdin(Stdio::null()).finished());
    assert!(code != 0 && err.starts_with("panic: overflow: 255 + 1 does not fit in byte"), "{code} {err}");
    assert_eq!(ran(&sg(&dir, &["build", "wraps.sg", "--release", "-o", "wrapped", "--", "-O0"])).0, 0);
    assert_eq!(ran(&Command::new(program(&dir, "wrapped")).stdin(Stdio::null()).finished()), (0, "4\n".to_string(), String::new()));
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
        // `emit-c` compiles nothing (1935).
        &["emit-c", "hello.sg", "--", "-O3"],
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
    Command::new(env!("CARGO_BIN_EXE_sg")).args(words).current_dir(dir).env("TMPDIR", tmp).env("TMP", tmp).env("TEMP", tmp).stdin(Stdio::null()).finished()
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
            Command::new("sh").arg("-c").arg(script).arg(&precious).arg(env!("CARGO_BIN_EXE_sg")).current_dir(&dir).env("TMPDIR", &tmp).stdin(Stdio::null()).finished()
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
            ran(&Command::new(&command[0]).args(&command[1..]).args(words).current_dir(&dir).stdin(Stdio::null()).finished())
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
        let run = Command::new(&out).arg(&cases).current_dir(&dir).stdin(Stdio::null()).finished();
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
        let run = Command::new(&out).arg(&cases).current_dir(&dir).stdin(Stdio::null()).finished();
        assert_eq!(ran(&run), expected, "compiled");
    }
}

/// Unicode's normalization tests (`tests/normalization-suite`, choice 1614), run by
/// `text.normalize(form)` and `text.isNormalized(form)` interpreted and compiled.
#[test]
fn a_normalization_suite() {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("normalization-suite");
    let cases = suite.join("NormalizationTest.txt").to_string_lossy().into_owned();
    let check = suite.join("check.sg").to_string_lossy().into_owned();
    let dir = work("a_normalization_suite");
    let expected = (0, "20034 lines and 1094978 other code points tried; 0 wrong\n".to_string(), String::new());
    assert_eq!(ran(&sg(&dir, &[&check, &cases])), expected, "interpreted");
    if has_compiler() {
        let out = program(&dir, "check").to_string_lossy().into_owned();
        assert_eq!(ran(&sg(&dir, &["build", &check, "-o", &out])), (0, String::new(), String::new()));
        let run = Command::new(&out).arg(&cases).current_dir(&dir).stdin(Stdio::null()).finished();
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
        let run = Command::new(&out).arg(&folder).current_dir(&dir).stdin(Stdio::null()).finished();
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
    let run = Command::new(&out).current_dir(&dir).stdin(Stdio::null()).finished();
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
    let made = Command::new(&text).args(["1", "7"]).current_dir(&dir).stdin(Stdio::null()).finished();
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
        let compiled = ran(&Command::new(program).args(args).current_dir(&dir).stdin(Stdio::null()).finished());
        assert_eq!(interpreted, compiled, "{trial} {args:?}: interpreted and compiled");
        if !expected.is_empty() {
            assert_eq!(compiled, (0, expected.to_string(), String::new()), "{trial} {args:?}");
        }
        printed.push(compiled.1);
    }
    // The lines of the last case: 660, each with its number.
    assert_eq!(printed[4].lines().count(), 660);
    assert!(printed[4].lines().all(|line| line.split_once(':').is_some_and(|(n, _)| n.parse::<u32>().is_ok())));
    // Bytes that are not UTF-8, of each kind, do not stop `grep`: each line is searched with
    // `isMatchBytes`, where they match nothing, `.` neither, and written as it is (choice 1840).
    let bad: &[u8] = b"first ok\nbad \xff\xfe ok\r\n\xe6\x97 cut ok\n\xc0\x80 long\n\xed\xa0\x80 surrogate ok\nok.\xf4\x90\x80\x80\nlast ok";
    std::fs::write(dir.join("bad.log"), bad).unwrap();
    let cases: [(&[&str], &[u8]); 3] = [
        (&["-n", "ok", "bad.log"], b"1:first ok\n2:bad \xff\xfe ok\n3:\xe6\x97 cut ok\n5:\xed\xa0\x80 surrogate ok\n6:ok.\xf4\x90\x80\x80\n7:last ok\n"),
        // `bad` is one, and the three bad bytes before ` surrogate` are no three characters.
        (&["-c", "^.{3} ", "bad.log"], b"1\n"),
        (&["-v", "ok", "bad.log"], b"\xc0\x80 long\n"),
    ];
    let script = source("grep");
    for (args, expected) in cases {
        let mut words = vec![script.as_str()];
        words.extend_from_slice(args);
        let interpreted = sg(&dir, &words);
        let compiled = Command::new(&built[0]).args(args).current_dir(&dir).stdin(Stdio::null()).finished();
        assert_eq!(interpreted.stdout, expected, "grep {args:?} interpreted");
        assert_eq!(compiled.stdout, expected, "grep {args:?} compiled");
        assert_eq!((interpreted.status.code(), compiled.status.code()), (Some(0), Some(0)), "grep {args:?}");
    }
}

/// The trial of `kv` (choices 1844-1853): the store of keys and values with a log, built and run
/// interpreted and compiled, with each `--sync`, on commands at their edges (bytes that are not
/// UTF-8, an empty key and value, a CR, lines that are not commands, a last line without its end);
/// on its log read back; on sets enough to compact it on its own; on its log cut in its last record,
/// as a crash leaves it; on a bit turned in the middle, where it does not start; and on what its format
/// cannot tell (zeros after the last record, a length turned so the record reaches past the end). What it prints,
/// its exit code and the length of its log are those the trial's model gives
/// (`trials/kv/tests/check.py`); interpreted, it leaves the log compiled code leaves, byte for byte.
#[test]
fn the_trial_of_kv() {
    if !has_compiler() {
        return;
    }
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let dir = work("the_trial_of_kv");
    let source = trials.join("kv").join("sg").join("kv.sg").to_string_lossy().into_owned();
    let built = program(&dir, "kv").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &built])), (0, String::new(), String::new()));
    // Runs the store on `input` in `folder`, interpreted or compiled: the exit code, what it
    // printed and the log it left.
    let run = |interpreted: bool, folder: &str, sync: &str, input: &[u8]| {
        let path = dir.join(format!("{folder}.in"));
        std::fs::write(&path, input).unwrap();
        let mut command = match interpreted {
            true => {
                let mut command = Command::new(env!("CARGO_BIN_EXE_sg"));
                command.arg(&source);
                command
            }
            false => Command::new(&built),
        };
        let out = command.args(["--sync", sync, folder]).current_dir(&dir).stdin(std::fs::File::open(&path).unwrap()).finished();
        let log = std::fs::read(dir.join(folder).join("log")).unwrap_or_default();
        (out.status.code().unwrap_or(-1), out.stdout, log)
    };
    let edge: &[u8] = b"SET a 1\nSET b two words\nGET a\nGET b\nGET c\nDEL a\nDEL a\nGET a\nSET  empty key\nGET \nSET k \nGET k\n\
        SET \xff\xfe bytes \xc3\x28\nGET \xff\xfe\nSET cr value\r\nGET cr\nFOO\nSET x\nset a 1\n\nGET\nDEL\nCOMPACT\nGET b\nCOMPACT x\n SET a 1";
    let again: &[u8] = b"GET b\nSET c 3\nDEL b\nGET b\nGET c\n";
    let mut hot = Vec::new();
    for n in 0..3000 {
        hot.extend_from_slice(format!("SET hot{} {}\n", n % 3, "v".repeat(390 + n % 20)).as_bytes());
    }
    hot.extend_from_slice(b"GET hot0\nGET hot1\nGET hot2\nGET hot3\n");
    let gets: &[u8] = b"GET hot0\nGET hot1\nGET hot2\n";
    let lengths = |out: &[u8]| out.split(|b| *b == b'\n').map(<[u8]>::len).collect::<Vec<_>>();
    for interpreted in [false, true] {
        let how = if interpreted { "interpreted" } else { "compiled" };
        let edges = format!("edges-{how}");
        let (code, out, log) = run(interpreted, &edges, "never", edge);
        assert_eq!((code, out.as_slice(), log.len()), (1, &b"OK\nOK\n1\ntwo words\n(nil)\n1\n0\n(nil)\nOK\nempty key\nOK\n\nOK\nbytes \xc3(\nOK\nvalue\r\nOK\ntwo words\n"[..], 103), "edges {how}");
        let (code, out, log) = run(interpreted, &edges, "always", again);
        assert_eq!((code, out.as_slice(), log.len()), (0, &b"two words\nOK\n1\n(nil)\n3\n"[..], 132), "read back {how}");
        // 3,000 sets of three keys: the log is compacted once it is past 1 MB.
        let hots = format!("hot-{how}");
        let (code, out, log) = run(interpreted, &hots, "batch", &hot);
        assert_eq!((code, lengths(&out)[3000..].to_vec(), log.len()), (0, vec![407, 408, 409, 5, 0], 202_040), "compacted {how}");
        // The last record cut off, as a crash in its write leaves it: cut away, and the value
        // before it is there.
        let cut = format!("cut-{how}");
        std::fs::create_dir_all(dir.join(&cut)).unwrap();
        std::fs::write(dir.join(&cut).join("log"), &log[..log.len() - 5]).unwrap();
        let (code, out, cut_log) = run(interpreted, &cut, "batch", gets);
        assert_eq!((code, lengths(&out), cut_log.len()), (0, vec![407, 408, 406, 0], 201_614), "cut {how}");
        // A bit turned in the first record, with more after it: the store does not start.
        let turned = format!("turned-{how}");
        std::fs::create_dir_all(dir.join(&turned)).unwrap();
        let mut bad = log.clone();
        bad[20] ^= 4;
        std::fs::write(dir.join(&turned).join("log"), &bad).unwrap();
        let (code, out, turned_log) = run(interpreted, &turned, "batch", gets);
        assert_eq!((code, out.len(), turned_log == bad), (1, 0, true), "turned {how}");
        // What the trial cannot tell, as its README says. Thirteen zeros after the last record are
        // a record cut off, cut away; fourteen are damage, and the log is left as it is.
        for (zeros, want) in [(13, (0, vec![407, 408, 409, 0], log.len())), (14, (1, vec![0], log.len() + 14))] {
            let name = format!("zeros-{zeros}-{how}");
            std::fs::create_dir_all(dir.join(&name)).unwrap();
            let mut padded = log.clone();
            padded.resize(log.len() + zeros, 0);
            std::fs::write(dir.join(&name).join("log"), &padded).unwrap();
            let (code, out, left) = run(interpreted, &name, "batch", gets);
            assert_eq!((code, lengths(&out), left.len()), want, "{zeros} zeros {how}");
        }
        // A bit turned in the length of the first key makes it reach past the end: the log is cut
        // away from there, with every record after it, as a crash would leave it.
        let long = format!("long-{how}");
        std::fs::create_dir_all(dir.join(&long)).unwrap();
        let mut bad = log.clone();
        bad[8] ^= 1;
        std::fs::write(dir.join(&long).join("log"), &bad).unwrap();
        let (code, out, long_log) = run(interpreted, &long, "batch", gets);
        assert_eq!((code, out.as_slice(), long_log.len()), (0, &b"(nil)\n(nil)\n(nil)\n"[..], 0), "length turned {how}");
    }
    // Interpreted and compiled leave the same logs.
    for name in ["edges", "hot", "cut"] {
        let log = |how: &str| std::fs::read(dir.join(format!("{name}-{how}")).join("log")).unwrap();
        assert!(log("interpreted") == log("compiled"), "the log of {name}");
    }
}

/// The trial of `jq` (choices 1804-1822): `trials/jq/sg`, a folder of files, built and run on the
/// megabyte of records, lines and numbers `trials/inputs/json.sg` makes from seed 7, and on inputs
/// that break things. What it prints and its exit code are jq 1.8.2's, taken from jq itself (the
/// standard error is each one's own, as the trial's README says); interpreted, it gives what it
/// gives compiled, the standard error too.
#[test]
fn the_trial_of_jq() {
    if !has_compiler() {
        return;
    }
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let dir = work("the_trial_of_jq");
    let maker = program(&dir, "json-input").to_string_lossy().into_owned();
    let maker_source = trials.join("inputs").join("json.sg").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &maker_source, "--release", "-o", &maker])), (0, String::new(), String::new()));
    for kind in ["records", "lines", "numbers"] {
        let made = Command::new(&maker).args([kind, "1", "7"]).current_dir(&dir).stdin(Stdio::null()).finished();
        assert!(made.status.success());
        std::fs::write(dir.join(format!("{kind}.json")), &made.stdout).unwrap();
    }
    let mut deep = b"[".repeat(10001);
    deep.push(b'1');
    deep.extend(b"]".repeat(10001));
    std::fs::write(dir.join("deep.json"), deep).unwrap();
    std::fs::write(dir.join("bad.json"), b"\"a\xc3b\" \"\xed\xa0\x80f\" \"ok\xe4\" \"\\ud800\"\n").unwrap();
    std::fs::write(dir.join("nums.json"), b"[1e1000, -0, 1.10, 100000000000000000001, 0.10000000000000000001, 15e-1147483647, 0.1]\n").unwrap();
    std::fs::write(dir.join("mid.json"), b"{\"a\":1}\n{\"a\":\"x\"}\n{\"a\":3}\n").unwrap();
    // At the bounds of how a number read is held and compared (choice 1837).
    std::fs::write(dir.join("bounds.json"), b"[123456789012345, 1234567890123456, 123456789012345.5, 0.0000000, 0.000001, 1E-307, 1.0000000000000001E-307, 1E+307, 2E+307, 5, 5.00000000000000000001, 99999999999999.99, 1e16]\n").unwrap();
    let source = trials.join("jq").join("sg").to_string_lossy().into_owned();
    let built = program(&dir, "jq").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &built])), (0, String::new(), String::new()));
    // The arguments, whether the interpreter runs it too, the exit code and what is printed.
    let cases: [(&[&str], bool, i32, &str); 17] = [
        (&["-c", "[.[] | select(.active and .score > 5000)] | length", "records.json"], true, 0, "667\n"),
        (&["-c", "group_by(.address.city) | map({c: .[0].address.city, n: length}) | .[0:3]", "records.json"], false, 0, "[{\"c\":\"Ankara\",\"n\":346},{\"c\":\"Kraków\",\"n\":319},{\"c\":\"Lyon\",\"n\":368}]\n"),
        (&["-r", ".[0:2][] | [.id, .name, .score] | @csv", "records.json"], false, 0, "7919,\"dennis278\",8417.856\n15838,\"linus688\",9524.804\n"),
        (&["-n", "-c", "reduce inputs as $r (0; . + $r.id)", "lines.json"], true, 0, "29132638932\n"),
        (&["-c", "[.. | numbers] | add", "numbers.json"], false, 0, "-33436.19407786794\n"),
        (&["-c", "sort_by(.balance) | .[0].id, .[-1].id", "records.json"], false, 0, "467221\n14911477\n"),
        (&["-c", "reduce .[] as $r ({}; .[$r.address.city] += 1) | to_entries | max_by(.value)", "records.json"], false, 0, "{\"key\":\"Lyon\",\"value\":368}\n"),
        // 10,001 levels are one too many; bad UTF-8 is repaired, a lone surrogate escaped stops it.
        (&["-c", "length", "deep.json"], true, 5, ""),
        (&["-c", "., length", "bad.json"], true, 5, "\"a\u{FFFD}b\"\n3\n\"\u{FFFD}f\"\n2\n\"ok\u{FFFD}\"\n3\n"),
        // Literals kept as written, numbers computed as doubles, the order of literals exact.
        (&["-c", "., map(. + 0), (.[4] > .[6]), (map(-.) | .[1])", "nums.json"], true, 0,
         "[1E+1000,-0,1.10,100000000000000000001,0.10000000000000000001,2E-1147483646,0.1]\n[1.7976931348623157e+308,0,1.1,1e+20,0.1,0,0.1]\ntrue\n0\n"),
        // A whole number of 15 digits is its double, of 16 a literal; a zero with seven zeros after
        // its point is `0E-7`; two of 15 digits are ordered by their doubles, two with the same
        // double and more digits exactly (1837); a whole double is written as an integer (1825).
        (&["-c", "., map(. + 0), (sort | .[0:3]), (.[9] == .[10]), (.[9] < .[10]), (.[5] < .[6]), (unique | length)", "bounds.json"], true, 0,
         "[123456789012345,1234567890123456,123456789012345.5,0E-7,0.000001,1E-307,1.0000000000000001E-307,1E+307,2E+307,5,5.00000000000000000001,99999999999999.99,1E+16]\n[123456789012345,1234567890123456,123456789012345.5,0,1e-06,1e-307,1.0000000000000001e-307,1e+307,2e+307,5,5,99999999999999.98,1e+16]\n[0E-7,1E-307,1.0000000000000001E-307]\nfalse\ntrue\ntrue\n13\n"),
        (&["-n", "-c", "[range(4) | . * 1e15, . / 2]"], true, 0, "[0,0,1000000000000000,0.5,2000000000000000,1,3000000000000000,1.5]\n"),
        // An error in the middle: the others are worked, and the last input decides the exit code.
        (&["-c", ".a + 1", "mid.json"], true, 0, "2\n4\n"),
        (&["-n", "-c", "reduce range(10001) as $i (0; [.]) | (. == .), ([.] | length)"], true, 0, "true\n1\n"),
        // Compared 10,001 deep, past the 5,000 nested calls the interpreter once took (1823).
        (&["-n", "-c", "reduce range(10001) as $i (0; [.]) | [., [.]] | sort | length"], true, 5, ""),
        (&[".", "missing.json"], true, 2, ""),
        (&[".["], true, 3, ""),
    ];
    for (args, interpreted, code, out) in cases {
        let compiled = ran(&Command::new(&built).args(args).current_dir(&dir).stdin(Stdio::null()).finished());
        assert_eq!((compiled.0, compiled.1.as_str()), (code, out), "{args:?}: {}", compiled.2);
        if interpreted {
            let mut words = vec![source.as_str()];
            words.extend_from_slice(args);
            assert_eq!(ran(&sg(&dir, &words)), compiled, "{args:?}: interpreted and compiled");
        }
    }
}

/// The trial of `std`'s JSON (1553, 1634): `trials/std/json` in SG, interpreted and compiled, on
/// the megabyte of each kind `trials/inputs/json.sg` makes from seed 7, gives what `serde_json` and
/// Zig's `std.json` give: the counts and sums below were taken from those, and a copy is the
/// document with its lines joined, as theirs is.
#[test]
fn the_trial_of_json() {
    if !has_compiler() {
        return;
    }
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let dir = work("the_trial_of_json");
    let maker = program(&dir, "json-input").to_string_lossy().into_owned();
    let maker_source = trials.join("inputs").join("json.sg").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &maker_source, "--release", "-o", &maker])), (0, String::new(), String::new()));
    for kind in ["records", "lines", "numbers"] {
        let made = Command::new(&maker).args([kind, "1", "7"]).current_dir(&dir).stdin(Stdio::null()).finished();
        assert!(made.status.success());
        std::fs::write(dir.join(format!("{kind}.json")), &made.stdout).unwrap();
    }
    let source = trials.join("std").join("json").join("sg").join("json.sg").to_string_lossy().into_owned();
    let built = program(&dir, "json").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &built])), (0, String::new(), String::new()));
    let counts = |values, null, bool_, integer, number, string, array, object, bytes: (u64, u64), sums: (&str, &str)| {
        format!(
            "values {values}\nnull {null}\nbool {bool_}\ninteger {integer}\nnumber {number}\nstring {string}\n\
             array {array}\nobject {object}\nstring bytes {}\nname bytes {}\ninteger sum {}\nnumber sum {}\n",
            bytes.0, bytes.1, sums.0, sums.1
        )
    };
    let records = counts(67540, 660, 6724, 6724, 10848, 25011, 8137, 9436, (298340, 217812), ("7048255495089", "31052479.53212802"));
    let lines = format!(
        "documents 2712\n{}",
        counts(67539, 660, 6724, 6724, 10848, 25011, 8136, 9436, (298340, 217812), ("7048255495089", "31052479.53212802"))
    );
    let numbers = counts(76002, 0, 0, 85, 50156, 256, 25249, 256, (1793, 3667), ("3655", "-37091.1940778679"));
    let joined = |kind: &str| {
        let text = std::fs::read_to_string(dir.join(format!("{kind}.json"))).unwrap();
        format!("{}\n", text.replace('\n', ""))
    };
    let cases = [
        (["sum", "records.json"], records),
        (["lines", "lines.json"], lines),
        (["sum", "numbers.json"], numbers),
        (["copy", "records.json"], joined("records")),
        (["copy", "numbers.json"], joined("numbers")),
    ];
    for (args, expected) in cases {
        let interpreted = ran(&sg(&dir, &[source.as_str(), args[0], args[1]]));
        let compiled = ran(&Command::new(&built).args(args).current_dir(&dir).stdin(Stdio::null()).finished());
        assert_eq!(interpreted, compiled, "json {args:?}: interpreted and compiled");
        assert!(compiled == (0, expected, String::new()), "json {args:?}");
    }
}

/// The inputs of the trials of compression and zip: 200,000 bytes of the log of `inputs/text.sg`
/// and the lines of `inputs/json.sg` up to about 100,000 bytes, each made from the seed 7.
fn trial_inputs(dir: &Path) -> (Vec<u8>, Vec<u8>) {
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let mut made = Vec::new();
    for (input, args) in [("text", ["1", "7"].as_slice()), ("json", ["lines", "1", "7"].as_slice())] {
        let maker = program(dir, &format!("{input}-input")).to_string_lossy().into_owned();
        let source = trials.join("inputs").join(format!("{input}.sg")).to_string_lossy().into_owned();
        assert_eq!(ran(&sg(dir, &["build", &source, "--release", "-o", &maker])), (0, String::new(), String::new()));
        let out = Command::new(&maker).args(args).current_dir(dir).stdin(Stdio::null()).finished();
        assert!(out.status.success());
        made.push(out.stdout);
    }
    let text = made[0][..200_000].to_vec();
    let lines = &made[1];
    let mut end = 0;
    while end < 100_000 {
        end += lines[end..].iter().position(|&b| b == b'\n').unwrap() + 1;
    }
    (text, lines[..end].to_vec())
}

/// The trial of `std`'s compression (choices 1642, 1646-1649), run small: the SG program,
/// interpreted and compiled, writes the same gzip of the text, opens it back into the text, and
/// counts the records of the lines as they are, each compressed in zlib and opened again.
#[test]
fn the_trial_of_compression() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_trial_of_compression");
    let (text, lines) = trial_inputs(&dir);
    std::fs::write(dir.join("text.log"), &text).unwrap();
    std::fs::write(dir.join("lines.json"), &lines).unwrap();
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let source = trials.join("std").join("compression").join("sg").join("compression.sg").to_string_lossy().into_owned();
    let built = program(&dir, "compression").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &built])), (0, String::new(), String::new()));
    let compiled = |args: &[&str]| Command::new(&built).args(args).current_dir(&dir).stdin(Stdio::null()).finished();
    let said = (0, "bytes 200000\n".to_string(), String::new());
    assert_eq!(ran(&sg(&dir, &[source.as_str(), "compress", "text.log", "interpreted.gz"])), said);
    assert_eq!(ran(&compiled(&["compress", "text.log", "compiled.gz"])), said);
    let packed = std::fs::read(dir.join("compiled.gz")).unwrap();
    assert_eq!(std::fs::read(dir.join("interpreted.gz")).unwrap(), packed, "the same gzip, interpreted and compiled");
    assert!(packed.len() < text.len() / 3, "the log compresses");
    let opened = sg(&dir, &[source.as_str(), "decompress", "compiled.gz"]);
    assert_eq!((opened.status.code(), &opened.stdout), (Some(0), &text), "opened, interpreted");
    let opened = compiled(&["decompress", "interpreted.gz"]);
    assert_eq!((opened.status.code(), &opened.stdout), (Some(0), &text), "opened, compiled");
    let count = lines.iter().filter(|&&b| b == b'\n').count();
    let bytes = lines.len() - count;
    let interpreted = ran(&sg(&dir, &[source.as_str(), "records", "lines.json"]));
    assert_eq!(interpreted, ran(&compiled(&["records", "lines.json"])), "records: interpreted and compiled");
    assert_eq!(interpreted.1, format!("records {count}\nbytes {bytes}\nsame {count}\n"));
    assert_eq!(ran(&compiled(&["decompress", "text.log"])).0, 1, "not gzip");
    assert_eq!(ran(&compiled(&["compress", "text.log"])).0, 2, "a wrong use");
}

/// The trial of `std`'s zip archives (choices 1643, 1650, 1652), run small: the SG program,
/// interpreted and compiled, writes the same archive of the text in parts of 10,000 bytes, reads
/// every entry back and lists them.
#[test]
fn the_trial_of_zip() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_trial_of_zip");
    let (text, _) = trial_inputs(&dir);
    std::fs::write(dir.join("text.log"), &text).unwrap();
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let source = trials.join("std").join("zip").join("sg").join("zip.sg").to_string_lossy().into_owned();
    let built = program(&dir, "zip").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &built])), (0, String::new(), String::new()));
    let compiled = |args: &[&str]| ran(&Command::new(&built).args(args).current_dir(&dir).stdin(Stdio::null()).finished());
    let said = (0, "entries 20\nbytes 200000\n".to_string(), String::new());
    assert_eq!(ran(&sg(&dir, &[source.as_str(), "write", "text.log", "interpreted.zip"])), said);
    assert_eq!(compiled(&["write", "text.log", "compiled.zip"]), said);
    assert_eq!(std::fs::read(dir.join("interpreted.zip")).unwrap(), std::fs::read(dir.join("compiled.zip")).unwrap(), "the same archive");
    assert_eq!(ran(&sg(&dir, &[source.as_str(), "read", "compiled.zip"])), said);
    assert_eq!(compiled(&["read", "interpreted.zip"]), said);
    let listed: String = (0..20).map(|i| format!("logs/part-{i:05}.txt 10000\n")).collect();
    assert_eq!(ran(&sg(&dir, &[source.as_str(), "list", "compiled.zip"])), (0, listed.clone(), String::new()));
    assert_eq!(compiled(&["list", "compiled.zip"]), (0, listed, String::new()));
    assert_eq!(compiled(&["read", "text.log"]).0, 1, "not an archive");
    assert_eq!(compiled(&["read"]).0, 2, "a wrong use");
}

/// The trials of `std`'s SHA-256, HMAC, CRC-32 and Adler-32 (choices 1653-1655), run small: the SG
/// programs, interpreted and compiled, give for the text what Python's `hashlib`, `hmac` and `zlib`
/// give, the values below: its SHA-256, 1,000 hashes chained from 32 zero bytes, the HMAC-SHA-256 of
/// each of its 1,832 lines (the last without its `\n`) put together with XOR, its CRC-32 and
/// Adler-32, and the CRC-32 of each line put together the same way.
#[test]
fn the_trials_of_hash_and_checksum() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_trials_of_hash_and_checksum");
    let (text, _) = trial_inputs(&dir);
    std::fs::write(dir.join("text.log"), &text).unwrap();
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let cases: [(&str, &[&str], &str); 6] = [
        ("hash", &["sha256", "text.log"], "6beed989b9e321c83cbe0cb876354ce7f018c2d293b501b68d7f7ee53c918418\n"),
        ("hash", &["chain", "1000"], "36c1cb4f826ae42ceba848227e0c5f786178ca9dceca6772e5d728d09c30a2f6\n"),
        ("hash", &["hmac", "text.log"], "lines 1832\nxor cdb16de1d525ec6bd601dff87334eabd8c06de26aea839ea27e0fb4c5cd7df0f\n"),
        ("checksum", &["crc32", "text.log"], "2119eaab\n"),
        ("checksum", &["adler32", "text.log"], "60e7f412\n"),
        ("checksum", &["lines", "text.log"], "lines 1832\nxor 914841d1\n"),
    ];
    for trial in ["hash", "checksum"] {
        let source = trials.join("std").join(trial).join("sg").join(format!("{trial}.sg")).to_string_lossy().into_owned();
        let built = program(&dir, trial).to_string_lossy().into_owned();
        assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &built])), (0, String::new(), String::new()));
        let compiled = |args: &[&str]| ran(&Command::new(&built).args(args).current_dir(&dir).stdin(Stdio::null()).finished());
        for (_, args, expected) in cases.iter().filter(|case| case.0 == trial) {
            let mut words = vec![source.as_str()];
            words.extend_from_slice(args);
            let interpreted = ran(&sg(&dir, &words));
            assert_eq!(interpreted, compiled(args), "{trial} {args:?}: interpreted and compiled");
            assert_eq!(interpreted, (0, expected.to_string(), String::new()), "{trial} {args:?}");
        }
        assert_eq!(compiled(&["crc32", "missing.txt"]).0, if trial == "hash" { 2 } else { 1 }, "{trial}: a missing file");
        assert_eq!(compiled(&["sum"]).0, 2, "{trial}: a wrong use");
    }
}

/// The trial of `std`'s hex and Base64 (choices 1660, 1665), run small: the SG program, interpreted
/// and compiled, writes the text in Base64 and reads it back into the same bytes, and gives for
/// 1,000 values and tokens, made from the seed 1, what Python's `bytes.hex` and `base64` give (the
/// values below).
#[test]
fn the_trial_of_encoding() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_trial_of_encoding");
    let (text, _) = trial_inputs(&dir);
    std::fs::write(dir.join("text.log"), &text).unwrap();
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let source = trials.join("std").join("encoding").join("sg").join("encoding.sg").to_string_lossy().into_owned();
    let built = program(&dir, "encoding").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &built])), (0, String::new(), String::new()));
    let compiled = |args: &[&str]| ran(&Command::new(&built).args(args).current_dir(&dir).stdin(Stdio::null()).finished());
    let interpreted = |args: &[&str]| {
        let mut words = vec![source.as_str()];
        words.extend_from_slice(args);
        ran(&sg(&dir, &words))
    };
    let written = (0, "bytes 266668\n".to_string(), String::new());
    assert_eq!(interpreted(&["encode", "text.log", "interpreted.b64"]), written);
    assert_eq!(compiled(&["encode", "text.log", "compiled.b64"]), written);
    let encoded = std::fs::read(dir.join("compiled.b64")).unwrap();
    assert_eq!(std::fs::read(dir.join("interpreted.b64")).unwrap(), encoded, "the same Base64, interpreted and compiled");
    assert!(encoded.starts_with(b"MjAyNi0xMS0wMVQwMDowMDowMlogREVCVUcgbWFy"), "what Python's `base64` writes");
    let read = (0, "bytes 200000\n".to_string(), String::new());
    assert_eq!(interpreted(&["decode", "compiled.b64", "interpreted.bin"]), read);
    assert_eq!(compiled(&["decode", "interpreted.b64", "compiled.bin"]), read);
    assert_eq!(std::fs::read(dir.join("interpreted.bin")).unwrap(), text, "read back, interpreted");
    assert_eq!(std::fs::read(dir.join("compiled.bin")).unwrap(), text, "read back, compiled");
    let cases: [(&[&str], &str); 2] = [
        (&["hex", "1000"], "values 1000\ndigits 64000\nsum 70274\nxor 3d9a57b94c0389b7\n"),
        (&["tokens", "1000"], "tokens 1000\ncharacters 43427\nsum 86011\nxor b502de436bbdaa56\n"),
    ];
    for (args, expected) in cases {
        assert_eq!(interpreted(args), compiled(args), "{args:?}: interpreted and compiled");
        assert_eq!(compiled(args), (0, expected.to_string(), String::new()), "{args:?}");
    }
    assert_eq!(compiled(&["decode", "text.log", "x.bin"]).0, 1, "not Base64");
    assert_eq!(compiled(&["hex", "many"]).0, 2, "a wrong use");
}

/// The trial of `std`'s categories and normal forms of Unicode (choices 1661, 1662), run small, on
/// the first 1,907 lines of `trials/inputs/unicode.sg` with the seed 7: the SG program, interpreted
/// and compiled, counts the categories as Python's `unicodedata` does, writes each line in NFD as
/// many bytes as Python does, finds no line of that in NFC, and brings it back to the text, which is.
#[test]
fn the_trial_of_unicode() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_trial_of_unicode");
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let maker = program(&dir, "unicode-input").to_string_lossy().into_owned();
    let input = trials.join("inputs").join("unicode.sg").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &input, "--release", "-o", &maker])), (0, String::new(), String::new()));
    let made = Command::new(&maker).args(["1", "7"]).current_dir(&dir).stdin(Stdio::null()).finished();
    assert!(made.status.success());
    let mut end = 0;
    while end < 200_000 {
        end += made.stdout[end..].iter().position(|&b| b == b'\n').unwrap() + 1;
    }
    let text = &made.stdout[..end];
    std::fs::write(dir.join("text.txt"), text).unwrap();
    let source = trials.join("std").join("unicode").join("sg").join("unicode.sg").to_string_lossy().into_owned();
    let built = program(&dir, "unicode").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &built])), (0, String::new(), String::new()));
    let compiled = |args: &[&str]| ran(&Command::new(&built).args(args).current_dir(&dir).stdin(Stdio::null()).finished());
    let interpreted = |args: &[&str]| {
        let mut words = vec![source.as_str()];
        words.extend_from_slice(args);
        ran(&sg(&dir, &words))
    };
    let counts = "Lu 2816\nLl 64445\nLt 0\nLm 0\nLo 19144\nMn 1097\nMc 1113\nMe 0\nNd 7577\nNl 0\nNo 0\nPc 0\nPd 184\nPs 164\nPe 164\n\
                  Pi 165\nPf 165\nPo 907\nSm 0\nSc 141\nSk 367\nSo 1606\nZs 21530\nZl 0\nZp 0\nCc 0\nCf 0\nCo 0\nCn 0\n";
    assert_eq!(interpreted(&["category", "text.txt"]), (0, counts.to_string(), String::new()));
    assert_eq!(compiled(&["category", "text.txt"]), (0, counts.to_string(), String::new()));
    let lines = (0, "lines 1907\n".to_string(), String::new());
    assert_eq!(interpreted(&["nfd", "text.txt", "interpreted.nfd"]), lines);
    assert_eq!(compiled(&["nfd", "text.txt", "compiled.nfd"]), lines);
    let decomposed = std::fs::read(dir.join("compiled.nfd")).unwrap();
    assert_eq!(std::fs::read(dir.join("interpreted.nfd")).unwrap(), decomposed, "the same NFD, interpreted and compiled");
    assert_eq!(decomposed.len(), 241_608, "as many bytes as Python's NFD");
    assert_eq!(compiled(&["check", "text.txt"]), (0, "lines 1907\nnormalized 1907\n".to_string(), String::new()));
    assert_eq!(interpreted(&["check", "compiled.nfd"]), (0, "lines 1907\nnormalized 0\n".to_string(), String::new()));
    assert_eq!(interpreted(&["nfc", "compiled.nfd", "interpreted.nfc"]), lines);
    assert_eq!(compiled(&["nfc", "interpreted.nfd", "compiled.nfc"]), lines);
    assert_eq!(std::fs::read(dir.join("interpreted.nfc")).unwrap(), text, "back in NFC, interpreted");
    assert_eq!(std::fs::read(dir.join("compiled.nfc")).unwrap(), text, "back in NFC, compiled");
    assert_eq!(compiled(&["check", "missing.txt"]).0, 1, "a missing file");
    assert_eq!(compiled(&["nfkc", "text.txt", "x"]).0, 2, "a wrong use");
}

/// A trial of `trials/std/<module>` built in `dir`: its source, and the program compiled from it
/// with `--release`.
fn std_trial(dir: &Path, module: &str) -> (String, String) {
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let source = trials.join("std").join(module).join("sg").join(format!("{module}.sg")).to_string_lossy().into_owned();
    let built = program(dir, module).to_string_lossy().into_owned();
    assert_eq!(ran(&sg(dir, &["build", &source, "--release", "-o", &built])), (0, String::new(), String::new()));
    (source, built)
}

/// The trial of `std`'s calendar and clocks (choices 1667, 1668), run small: the SG program,
/// interpreted and compiled, writes 1,000 moments as `chrono` does, reads seven moments of RFC 3339
/// (before 1970, with offsets of either sign and fractions of one to nine digits) to the seconds
/// `chrono` reads, and, where the system has the time zone, sees 1,000 moments in Istanbul as
/// `chrono` sees them.
#[test]
fn the_trial_of_time() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_trial_of_time");
    let (source, built) = std_trial(&dir, "time");
    std::fs::write(
        dir.join("moments.txt"),
        "1970-01-01T00:00:00Z\n1969-12-31T23:59:59.999999999Z\n2026-11-21T23:30:05.5+03:00\n1900-01-01T00:00:00-14:00\n\
         2199-12-31T23:59:59.123456789+14:00\n2000-02-29T12:00:00.01-05:45\n1987-03-14T22:05:41.0274+05:45\n",
    )
    .unwrap();
    let compiled = |args: &[&str]| {
        ran(&Command::new(&built).args(args).env("TZ", "Europe/Istanbul").current_dir(&dir).stdin(Stdio::null()).finished())
    };
    let interpreted = |args: &[&str]| {
        let mut words = vec![source.as_str()];
        words.extend_from_slice(args);
        ran(&Command::new(env!("CARGO_BIN_EXE_sg")).args(words).env("TZ", "Europe/Istanbul").current_dir(&dir).stdin(Stdio::null()).finished())
    };
    let cases: [(&[&str], &str); 3] = [
        (&["format", "1000"], "moments 1000\ncharacters 25273\nsum 53997\n"),
        (&["parse", "moments.txt"], "lines 7\nseconds 8339006144\nnanoseconds 1660856788\noffsets 10800\n"),
        (&["clock", "1000"], "reads 1000\nforward true\n"),
    ];
    for (args, expected) in cases {
        assert_eq!(interpreted(args), (0, expected.to_string(), String::new()), "{args:?}, interpreted");
        assert_eq!(compiled(args), (0, expected.to_string(), String::new()), "{args:?}, compiled");
    }
    // `TZ` names a zone of the tz database only where the system has one (not on Windows).
    let local = compiled(&["local", "1000"]);
    assert_eq!(interpreted(&["local", "1000"]), local, "local, interpreted and compiled");
    if Path::new("/usr/share/zoneinfo/Europe/Istanbul").exists() {
        assert_eq!(local, (0, "moments 1000\noffsets 9839376\nparts 2111618\n".to_string(), String::new()));
    }
    assert_eq!(compiled(&["parse", "missing.txt"]).0, 1, "a missing file");
    std::fs::write(dir.join("bad.txt"), "2026-11-21T23:30:05Z\n2026-02-30T00:00:00Z\n").unwrap();
    let bad = compiled(&["parse", "bad.txt"]);
    assert_eq!(bad.0, 1, "not a moment");
    assert!(bad.2.contains("line 2"), "{bad:?}");
    assert_eq!(compiled(&["format", "many"]).0, 2, "a wrong use");
}

/// The trials of `std`'s command line, environment and paths (choices 1795-1802), run small: the SG
/// programs, interpreted and compiled, print what `clap`, `std::env` and `std::path` print for the
/// same words.
#[test]
fn the_trials_of_command_line_environment_and_path() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_trials_of_command_line_environment_and_path");
    for module in ["command_line", "environment", "path"] {
        let (source, built) = std_trial(&dir, module);
        let compiled = |args: &[&str]| {
            let mut command = Command::new(&built);
            command.args(args).env("SG_TRIAL_TEXT", "/usr/bin:/home/ayşe/bin").env_remove("SG_TRIAL_MISSING");
            #[cfg(unix)]
            command.env("SG_TRIAL_BYTES", <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(b"caf\xe9"));
            ran(&command.current_dir(&dir).stdin(Stdio::null()).finished())
        };
        let interpreted = |args: &[&str]| {
            let mut command = Command::new(env!("CARGO_BIN_EXE_sg"));
            command.arg(&source).args(args).env("SG_TRIAL_TEXT", "/usr/bin:/home/ayşe/bin").env_remove("SG_TRIAL_MISSING");
            #[cfg(unix)]
            command.env("SG_TRIAL_BYTES", <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(b"caf\xe9"));
            ran(&command.current_dir(&dir).stdin(Stdio::null()).finished())
        };
        let mut cases: Vec<(Vec<&str>, &str)> = match module {
            "command_line" => vec![
                (
                    vec!["-vfn", "--level=9", "-o", "out.zip", "zip", "src", "-xa*.tmp", "--exclude", "*.log", "notes.txt", "--", "-odd.txt", "-"],
                    "verbose true\nquiet false\nforce true\ndry-run true\noutput out.zip\nlevel 9\nexclude 2 a*.tmp *.log\n\
                     format zip\nfolder src\nfiles 3 18 notes.txt -\n",
                ),
                (
                    vec!["tar", "docs", "--", "--not-an-option", "-"],
                    "verbose false\nquiet false\nforce false\ndry-run false\noutput none\nlevel none\nexclude 0\n\
                     format tar\nfolder docs\nfiles 2 16 --not-an-option -\n",
                ),
            ],
            "environment" => vec![
                (vec!["get", "SG_TRIAL_TEXT", "SG_TRIAL_MISSING"], "SG_TRIAL_TEXT /usr/bin:/home/ayşe/bin\nSG_TRIAL_MISSING missing\n"),
                (vec!["read", "SG_TRIAL_TEXT", "1000"], "reads 1000\nbytes 24000\n"),
            ],
            _ => vec![
                (vec!["many", "1000"], "paths 1000\nbytes 23559\nnames 9045\nextensions 3271 in 849\nfolders 13514\n"),
                (vec!["many", "1000", "join"], "paths 1000\nbytes 23559\nnames 9045\nextensions 3271 in 849\nfolders 13514\n"),
            ],
        };
        // A path is joined with `\` on Windows, and a variable there holds UTF-16.
        if cfg!(unix) && module == "path" {
            cases.push((
                vec!["show", "logs/report.txt", "/report.txt", "café/日本.tar.gz", "./build.sg"],
                "logs/report.txt: report.txt .txt logs logs/report.txt/more.txt\n/report.txt: report.txt .txt / /report.txt/more.txt\n\
                 café/日本.tar.gz: 日本.tar.gz .gz café café/日本.tar.gz/more.txt\n./build.sg: build.sg .sg . ./build.sg/more.txt\n",
            ));
        }
        if cfg!(unix) && module == "environment" {
            // Not UTF-8: the byte 0xE9 comes as U+10FFE9 (choice 1950).
            cases.push((vec!["get", "SG_TRIAL_BYTES"], "SG_TRIAL_BYTES caf\u{10FFE9}\n"));
        }
        for (args, expected) in &cases {
            assert_eq!(interpreted(args), (0, expected.to_string(), String::new()), "{module} {args:?}, interpreted");
            assert_eq!(compiled(args), (0, expected.to_string(), String::new()), "{module} {args:?}, compiled");
        }
        let wrong: &[&str] = match module {
            "command_line" => &["--verbos", "zip", "src"],
            "environment" => &["read", "SG_TRIAL_TEXT", "many"],
            _ => &["many", "1000", "glue"],
        };
        assert_eq!(compiled(wrong).0, 2, "{module}: a wrong use");
        assert_eq!(interpreted(wrong).0, 2, "{module}: a wrong use, interpreted");
    }
}

/// The trial of `std`'s random numbers (choices 1667, 1669), run small, interpreted and compiled.
/// What it prints does not depend on the numbers drawn; that they are those of the reference code
/// of xoshiro256** seeded with SplitMix64, as `rand_xoshiro`'s, is held by `ok_random_seeded`.
#[test]
fn the_trial_of_random() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_trial_of_random");
    let (source, built) = std_trial(&dir, "random");
    let compiled = |args: &[&str]| ran(&Command::new(&built).args(args).current_dir(&dir).stdin(Stdio::null()).finished());
    let interpreted = |args: &[&str]| {
        let mut words = vec![source.as_str()];
        words.extend_from_slice(args);
        ran(&sg(&dir, &words))
    };
    let cases: [(&[&str], &str); 4] = [
        (&["numbers", "1000"], "numbers 1000\nbits near half true\n"),
        (&["range", "1000"], "numbers 1000\nbars 1000\nevery bar near true\n"),
        (&["fill", "1"], "bytes 1048576\nmean near true\n"),
        (&["tokens", "10"], "tokens 10\ncharacters 640\nhex true\n"),
    ];
    for (args, expected) in cases {
        assert_eq!(interpreted(args), (0, expected.to_string(), String::new()), "{args:?}, interpreted");
        assert_eq!(compiled(args), (0, expected.to_string(), String::new()), "{args:?}, compiled");
    }
    assert_eq!(compiled(&["numbers", "many"]).0, 2, "a wrong use");
}

/// A way of running a program: the exit code, the output and the standard error.
type Run<'a> = dyn Fn(&[&str]) -> (i32, String, String) + 'a;

/// The trial of `std`'s log (choices 1667, 1670), run small, interpreted and compiled: each line
/// on the standard error is a time to the millisecond, `INFO ` and the message; the lines of four
/// tasks are whole lines, every message there once; a line that is off writes nothing.
#[test]
fn the_trial_of_log() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_trial_of_log");
    let (source, built) = std_trial(&dir, "log");
    let compiled = |args: &[&str]| {
        ran(&Command::new(&built).args(args).env_remove("SG_LOG").current_dir(&dir).stdin(Stdio::null()).finished())
    };
    let interpreted = |args: &[&str]| {
        let mut words = vec![source.as_str()];
        words.extend_from_slice(args);
        ran(&Command::new(env!("CARGO_BIN_EXE_sg")).args(words).env_remove("SG_LOG").current_dir(&dir).stdin(Stdio::null()).finished())
    };
    // The messages of the lines, each line held to its form, in order of their text.
    let messages = |error: &str| {
        let mut seen: Vec<String> = error
            .lines()
            .map(|line| {
                let (time, rest) = line.split_at(24);
                let digits = time.bytes().enumerate().all(|(i, b)| match i {
                    4 | 7 => b == b'-',
                    10 => b == b'T',
                    13 | 16 => b == b':',
                    19 => b == b'.',
                    23 => b == b'Z',
                    _ => b.is_ascii_digit(),
                });
                assert!(digits, "{line:?}");
                rest.strip_prefix(" INFO  ").unwrap_or_else(|| panic!("{line:?}")).to_string()
            })
            .collect();
        seen.sort();
        seen
    };
    let runs: [&Run<'_>; 2] = [&interpreted, &compiled];
    for run in runs {
        let (code, output, error) = run(&["lines", "3"]);
        assert_eq!((code, output.as_str()), (0, "lines 3\n"));
        assert_eq!(messages(&error), ["request 0 took 0 ms", "request 1 took 7 ms", "request 2 took 14 ms"]);
        assert_eq!(run(&["off", "1000"]), (0, "lines 0 of 1000\nsum 499500\n".to_string(), String::new()));
        let (code, output, error) = run(&["tasks", "100"]);
        assert_eq!((code, output.as_str()), (0, "lines 100\n"));
        let mut expected: Vec<String> =
            (0..4).flat_map(|task| (0..25).map(move |i| format!("task {task} request {i} took {} ms", i * 7 % 1000))).collect();
        expected.sort();
        assert_eq!(messages(&error), expected);
    }
    assert_eq!(compiled(&["lines", "many"]).0, 2, "a wrong use");
}

/// The trial of the network (choices 1678-1682), run small, interpreted and compiled, async and
/// blocking: what came back over the loopback is what `std::net`, `tokio` and Zig's `std.Io.net`
/// count, and the addresses are written as they and Python's `ipaddress` write them.
#[test]
fn the_trial_of_net() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_trial_of_net");
    let (source, built) = std_trial(&dir, "net");
    std::fs::write(dir.join("lines.txt"), "one\r\ntwo two\n\nlast").unwrap();
    std::fs::write(
        dir.join("addresses.txt"),
        "1.2.3.4:80\n[2001:0DB8:0000:0000:0000:0000:0000:0001]:443\n[fe80::1:0:0:0]:0\n[0:0:0:0:0:0:0:1]:65535\n\
         255.255.255.255:8080\nnot an address\n01.2.3.4:80\n1.2.3.4:65536\n",
    )
    .unwrap();
    let compiled = |args: &[&str]| ran(&Command::new(&built).args(args).current_dir(&dir).stdin(Stdio::null()).finished());
    let interpreted = |args: &[&str]| {
        let mut words = vec![source.as_str()];
        words.extend_from_slice(args);
        ran(&Command::new(env!("CARGO_BIN_EXE_sg")).args(words).current_dir(&dir).stdin(Stdio::null()).finished())
    };
    let cases: [(&[&str], &str); 7] = [
        (&["echo", "MODE", "64", "8", "100"], "connections 8\nmessages 800\nbytes 51200\nsum 39600\n"),
        (&["echo", "MODE", "4096", "2", "300"], "connections 2\nmessages 600\nbytes 2457600\nsum 67172\n"),
        (&["connect", "MODE", "100"], "connections 100\nbytes 100\n"),
        (&["stream", "MODE", "2", "4096"], "bytes 2097152\nsum 3375104\n"),
        (&["lines", "MODE", "lines.txt"], "lines 4\nbytes 14\nlongest 7\n"),
        (&["udp", "MODE", "1024", "4", "50"], "sockets 4\nmessages 200\nbytes 204800\nsum 4900\n"),
        (&["address", "addresses.txt"], ADDRESSES),
    ];
    for (args, expected) in cases {
        for mode in ["async", "blocking"] {
            let args: Vec<&str> = args.iter().map(|&a| if a == "MODE" { mode } else { a }).collect();
            assert_eq!(interpreted(&args), (0, expected.to_string(), String::new()), "{args:?}, interpreted");
            assert_eq!(compiled(&args), (0, expected.to_string(), String::new()), "{args:?}, compiled");
        }
    }
    let written = "1.2.3.4:80\n[2001:db8::1]:443\n[fe80::1:0:0:0]:0\n[::1]:65535\n255.255.255.255:8080\ninvalid\ninvalid\ninvalid\n";
    assert_eq!(interpreted(&["print", "addresses.txt", "interpreted.txt"]), (0, String::new(), String::new()));
    assert_eq!(compiled(&["print", "addresses.txt", "compiled.txt"]), (0, String::new(), String::new()));
    assert_eq!(std::fs::read_to_string(dir.join("interpreted.txt")).unwrap(), written, "interpreted");
    assert_eq!(std::fs::read_to_string(dir.join("compiled.txt")).unwrap(), written, "compiled");
    assert_eq!(compiled(&["address", "missing.txt"]).0, 1, "a missing file");
    assert_eq!(compiled(&["echo", "now", "64", "8", "100"]).0, 2, "a wrong use");
    assert_eq!(compiled(&["udp", "async", "65536", "1", "1"]).0, 2, "a datagram too large");
}

/// The servers of the server lane in SG (choices 1701, 1704, 1706), held to what they must answer by
/// the load of `trials/server/load/load.c`, quickly: the store filled and every answer checked byte
/// for byte, with commands in flight, with values of 64 KB and with idle connections open; the HTTP
/// server with connections kept, closed after each request, and at a fixed rate. Nothing is timed.
/// The load waits on epoll or kqueue, so not on Windows.
#[cfg(unix)]
#[test]
fn the_servers_of_the_server_lane() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_servers_of_the_server_lane");
    let server = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials").join("server");
    let load = dir.join("load");
    let cc = sg::cc::compiler().unwrap();
    let built = Command::new(&cc)
        .args(["-O2", "-pthread", "-o"])
        .arg(&load)
        .arg(server.join("load").join("load.c"))
        .arg("-lm")
        .finished();
    assert_eq!(ran(&built).0, 0, "the load: {}", ran(&built).2);
    let resp = [
        "--fill --conns 4 --pipeline 16 --keys 2000",
        "--check --keys 2000 --conns 8 --duration 0.5 --warmup 0",
        "--check --keys 2000 --conns 8 --pipeline 16 --duration 0.5 --warmup 0",
        "--fill --conns 4 --pipeline 4 --keys 200 --value 65536",
        "--check --keys 200 --value 65536 --conns 4 --duration 0.5 --warmup 0",
        "--idle 200 --conns 4 --duration 0.5 --warmup 0",
    ];
    let http = [
        "--check --conns 8 --duration 0.5 --warmup 0",
        "--check --close --conns 4 --duration 0.5 --warmup 0",
        "--check --conns 8 --rate 2000 --duration 0.5 --warmup 0",
    ];
    for (name, runs) in [("resp", &resp[..]), ("http", &http[..])] {
        let source = server.join(name).join("sg").join(format!("{name}.sg")).to_string_lossy().into_owned();
        let program = program(&dir, name).to_string_lossy().into_owned();
        assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &program])), (0, String::new(), String::new()));
        // A port the system has just given and taken back.
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let mut child = Command::new(&program).arg(port.to_string()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
        let started = std::time::Instant::now();
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(started.elapsed().as_secs() < 10, "{name} does not listen on {port}");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        for args in runs {
            let out = Command::new(&load)
                .arg(name)
                .arg(format!("127.0.0.1:{port}"))
                .args(args.split(' '))
                .finished();
            let (code, stdout, stderr) = ran(&out);
            let answered = stdout.contains("\"answers\":") && !stdout.contains("\"answers\":0,");
            assert!(code == 0 && answered, "{name} {args}: exit {code}\n{stdout}\n{stderr}");
        }
        child.kill().unwrap();
        child.wait().unwrap();
    }
}

/// The file server of the server lane in SG (choices 1861 to 1871), built for release: the load's own check
/// of every byte of whole files, of ranges at random places, of 304 and of 404, on files written as the load
/// expects them (byte O of file F is (7F + O) mod 251); then requests at the edges, each sent as it is
/// written: a path out of the folder, `/` written `%2F`, a range past the end, a folder without its `/`, a
/// FIFO, which the server must not open. `trials/server/files/check.py` holds the four versions to many more.
#[cfg(unix)]
#[test]
fn the_file_server_of_the_server_lane() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_file_server_of_the_server_lane");
    let server = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials").join("server");
    let load = dir.join("load");
    let cc = sg::cc::compiler().unwrap();
    let built = Command::new(&cc).args(["-O2", "-pthread", "-o"]).arg(&load).arg(server.join("load").join("load.c")).arg("-lm").finished();
    assert_eq!(ran(&built).0, 0, "the load: {}", ran(&built).2);
    let source = server.join("files").join("sg").join("files.sg").to_string_lossy().into_owned();
    let program = program(&dir, "files").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &program])), (0, String::new(), String::new()));
    let root = dir.join("root");
    std::fs::create_dir_all(root.join("f")).unwrap();
    std::fs::create_dir_all(root.join("sub")).unwrap();
    for k in 0..20usize {
        let size = 200_000usize;
        let bytes: Vec<u8> = (0..size).map(|o| ((7 * k + o) % 251) as u8).collect();
        std::fs::write(root.join("f").join(format!("{k:06}.bin")), bytes).unwrap();
    }
    std::fs::write(root.join("a.txt"), "hello world\n").unwrap();
    std::fs::write(root.join("sub").join("index.html"), "<p>").unwrap();
    assert!(Command::new("mkfifo").arg(root.join("fifo")).status().unwrap().success());
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut child = Command::new(&program).arg(port.to_string()).arg(&root).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
    let started = std::time::Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(started.elapsed().as_secs() < 10, "files does not listen on {port}");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    for args in [
        "--check --files 20 --size 200000 --conns 8 --duration 0.5 --warmup 0",
        "--check --files 20 --size 200000 --range 70000 --conns 8 --duration 0.5 --warmup 0",
        "--check --files 20 --size 200000 --since --conns 8 --duration 0.5 --warmup 0",
        "--check --files 20 --size 200000 --missing --conns 8 --duration 0.5 --warmup 0",
        "--check --files 20 --size 200000 --close --conns 4 --duration 0.5 --warmup 0",
    ] {
        let out = Command::new(&load).arg("files").arg(format!("127.0.0.1:{port}")).args(args.split(' ')).finished();
        let (code, stdout, stderr) = ran(&out);
        let answered = stdout.contains("\"answers\":") && !stdout.contains("\"answers\":0,");
        assert!(code == 0 && answered, "files {args}: exit {code}\n{stdout}\n{stderr}");
    }
    let ask = |head: &str| -> String {
        use std::io::{Read, Write};
        let mut conn = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        conn.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        conn.write_all(format!("{head}\r\nHost: here\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
        let mut got = Vec::new();
        conn.read_to_end(&mut got).unwrap();
        String::from_utf8_lossy(&got).lines().filter(|l| !l.starts_with("date:") && !l.starts_with("last-modified:")).collect::<Vec<_>>().join("\n")
    };
    for (head, want) in [
        ("GET /a.txt HTTP/1.1", "HTTP/1.1 200 OK\ncontent-type: text/plain; charset=utf-8\naccept-ranges: bytes\nconnection: close\ncontent-length: 12\n\nhello world"),
        ("GET /../a.txt HTTP/1.1", "HTTP/1.1 400 Bad Request\ncontent-type: text/plain\nconnection: close\ncontent-length: 11\n\nbad request"),
        ("GET /sub%2Findex.html HTTP/1.1", "HTTP/1.1 400 Bad Request\ncontent-type: text/plain\nconnection: close\ncontent-length: 11\n\nbad request"),
        ("GET /sub HTTP/1.1", "HTTP/1.1 301 Moved Permanently\ncontent-type: text/plain\nlocation: /sub/\nconnection: close\ncontent-length: 17\n\nmoved permanently"),
        ("GET /sub/ HTTP/1.1", "HTTP/1.1 200 OK\ncontent-type: text/html; charset=utf-8\naccept-ranges: bytes\nconnection: close\ncontent-length: 3\n\n<p>"),
        ("GET /fifo HTTP/1.1", "HTTP/1.1 404 Not Found\ncontent-type: text/plain\nconnection: close\ncontent-length: 9\n\nnot found"),
        ("GET /a.txt HTTP/1.1\r\nRange: bytes=-3", "HTTP/1.1 206 Partial Content\ncontent-type: text/plain; charset=utf-8\naccept-ranges: bytes\ncontent-range: bytes 9-11/12\nconnection: close\ncontent-length: 3\n\nld"),
        ("GET /a.txt HTTP/1.1\r\nRange: bytes=12-", "HTTP/1.1 416 Range Not Satisfiable\ncontent-type: text/plain\ncontent-range: bytes */12\nconnection: close\ncontent-length: 21\n\nrange not satisfiable"),
    ] {
        assert_eq!(ask(head), want, "{head}");
    }
    assert!(child.try_wait().unwrap().is_none(), "the server ended");
    child.kill().unwrap();
    child.wait().unwrap();
}

/// The servers of the server lane stand what clients do to them (choices 1750 to 1753, 1759, 1761,
/// 1762), as `trials/server/load/stand.c` does it, quickly: resets, halves closed, a byte at a time,
/// clients that do not read, requests that are not the protocol, more clients than descriptors, and
/// SIGTERM; after each round the server answers and holds no more descriptors than before, and clients
/// that do not read make it hold no more than its bound. The rule that its memory does not grow round by
/// round is held at the heavy level, on a quiet machine (1759): beside the other tests the allocator's
/// arenas warm up for ten rounds and more. The release builds, and the builds under the address and
/// undefined-behaviour sanitizers (with the leak check at the end, on Linux) for the requests that
/// are not the protocol, the resets and the stop. On Linux 10,000 connections and the two servers
/// at once; on macOS, which keeps the port of a connection the stand closed 30 s and has 16,384,
/// a tenth of them and one server after the other. The stand waits on epoll or kqueue, so not on Windows.
#[cfg(unix)]
#[test]
fn the_servers_stand() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_servers_stand");
    let server = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials").join("server");
    let stand = dir.join("stand");
    let cc = sg::cc::compiler().unwrap();
    let built = Command::new(&cc).args(["-std=c11", "-O2", "-o"]).arg(&stand).arg(server.join("load").join("stand.c")).finished();
    assert_eq!(ran(&built).0, 0, "the stand: {}", ran(&built).2);
    let linux = cfg!(target_os = "linux");
    // macOS keeps the port of a connection the stand closed 30 s and has 16,384: a tenth there.
    let n = |count: u32| if linux { count } else { count / 10 };
    let runs: Vec<(String, &str)> = vec![
        (format!("answers --conns {} --at-once 200 --rounds 2", n(10000)), ""),
        // The server's 256 descriptors are outnumbered on macOS too.
        (format!("fds --conns {} --rounds 2", if linux { 1000 } else { 400 }), ""),
        (format!("reset --conns {} --at-once 500 --rounds 2", n(10000)), ""),
        (format!("half --conns {} --at-once 500 --rounds 2", n(10000)), ""),
        (format!("split --conns {} --at-once 200 --rounds 2", n(1000)), ""),
        (format!("trickle --conns {} --at-once 500 --rounds 2", n(1000)), ""),
        ("noread --conns 100 --rounds 2".to_string(), ""),
        ("noread --conns 100 --rounds 2 --idle 1000".to_string(), "--idle-limit 1000"),
        (format!("term --conns {} --grace 1000", n(10000)), "--grace 1000"),
        (format!("fuzz --conns {} --at-once 50 --rounds 1", n(10000)), ""),
    ];
    let sanitized: Vec<(String, &str)> = vec![
        (format!("fuzz --conns {} --at-once 50 --rounds 1", n(10000)), ""),
        (format!("reset --conns {} --at-once 200 --rounds 1", n(2000)), ""),
        (format!("term --conns {} --grace 1000 --margin 10000", n(2000)), "--grace 1000"),
    ];
    // The two servers at once, each on its own thread.
    let jobs: Vec<_> = ["resp", "http"]
        .into_iter()
        .map(|name| {
            let (dir, server, stand, cc, runs, sanitized) = (dir.clone(), server.clone(), stand.clone(), cc.clone(), runs.clone(), sanitized.clone());
            let job = move || {
                let source = server.join(name).join("sg").join(format!("{name}.sg")).to_string_lossy().into_owned();
                let release = program(&dir, name).to_string_lossy().into_owned();
                assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &release])), (0, String::new(), String::new()));
                let c = sg(&dir, &["emit-c", &source, "--release"]);
                assert_eq!(c.status.code(), Some(0), "{name}: {}", text(&c.stderr));
                let c_file = dir.join(format!("{name}-sanitized.c"));
                std::fs::write(&c_file, &c.stdout).unwrap();
                let checked = program(&dir, &format!("{name}-sanitized"));
                let out = Command::new(&cc)
                    .args(["-std=c11", "-O1", "-g", "-fsanitize=address,undefined", "-fno-sanitize-recover=all", "-o"])
                    .arg(&checked)
                    .arg(&c_file)
                    .args(["-lm", "-pthread"])
                    .finished();
                assert_eq!(ran(&out).0, 0, "{name} under the sanitizers: {}", ran(&out).2);
                let checked = checked.to_string_lossy().into_owned();
                // The bound on what clients that do not read make a server hold is kept: two rounds
                // are too few for the rule of rounds.
                let memory = |args: &String| if args.starts_with("noread") { args.clone() } else { format!("{args} --no-memory") };
                let plain = runs.iter().map(|(args, server_args)| (memory(args), *server_args, release.clone(), false));
                let under = sanitized.iter().map(|(args, server_args)| (format!("{args} --no-memory --sanity 5000"), *server_args, checked.clone(), true));
                for (args, server_args, program, is_sanitized) in plain.chain(under) {
                    let out = Command::new(&stand)
                        .arg(name)
                        .args(args.split(' '))
                        .arg("--timeout")
                        .arg("240")
                        .arg("--")
                        .arg(&program)
                        .arg("PORT")
                        .args(server_args.split(' ').filter(|w| !w.is_empty()))
                        .finished();
                    let (code, stdout, stderr) = ran(&out);
                    let under = if is_sanitized { " (under the sanitizers)" } else { "" };
                    assert!(code == 0 && stdout.contains("\"stood\":true"), "{name} {args}{under}: exit {code}\n{stdout}\n{stderr}");
                }
            };
            if linux {
                Some(std::thread::spawn(job))
            } else {
                job();
                None
            }
        })
        .collect();
    for job in jobs.into_iter().flatten() {
        if let Err(cause) = job.join() {
            std::panic::resume_unwind(cause);
        }
    }
}

/// A client on a machine given 100 local ports (the test of `a_client_out_of_local_ports`).
#[cfg(target_os = "linux")]
const PORTS: &str = r#"void letGo(List<TcpStream> conns) {
}

async Result<bool, IoError> late(string address) {
    match await TcpStream.connectAsync(read address) {
        Ok(_) => {
            print("async: connected")
            return Ok(true)
        }
        Error(cause) => {
            print($"async: refused: {cause}")
            return Ok(false)
        }
    }
}

Result<void, IoError> main() {
    var listener = try TcpListener.bind("127.0.0.1:0")
    var address = $"127.0.0.1:{listener.port()}".toString()
    var clients = List<TcpStream>()
    var served = List<TcpStream>()
    int refused = 0
    for _ in 0..300 {
        match TcpStream.connect(read address) {
            Ok(conn) => {
                clients.add(conn)
                var other = try listener.accept()
                served.add(other)
            }
            Error(cause) => {
                if refused == 0 {
                    print($"refused: {cause}")
                }
                refused += 1
            }
        }
    }
    print($"connected: {clients.length()}, refused: {refused}")
    var task = late(address.clone())
    if try task.wait() {
        _ = try listener.accept()
    }
    letGo(served)
    letGo(clients)
    var again = try TcpStream.connect(read address)
    var other = try listener.accept()
    try again.send("ping".asBytes())
    var buffer = byte[4](0)
    nint got = 0
    while got < 4 {
        var n = try other.receive(ref buffer[got..])
        if n == 0 {
            break
        }
        got += n
    }
    var word = string.fromUtf8(buffer[0..got]) ?? "not text"
    print($"connected again: {word}")
    return Ok()
}
"#;

/// A client whose local ports run out (choice 1751 i): past them `connect` and `connectAsync` give
/// `IoError.AddressNotAvailable` (1764) and the program goes on, interpreted and compiled; once its connections are let go,
/// a new one connects. It needs a machine of 100 ports or so, and runs only on one: a step of
/// `.github/workflows/linux.yml` narrows the range with `sysctl` after the other tests and runs this
/// one alone with `SG_PORTS_NARROWED` set, and on our own machine `podman run --sysctl
/// net.ipv4.ip_local_port_range="40000 40099"` does (`benchmarks/robustness.md`). Narrowed beside
/// the other tests, it would starve them.
#[cfg(target_os = "linux")]
#[test]
fn a_client_out_of_local_ports() {
    let range = std::fs::read_to_string("/proc/sys/net/ipv4/ip_local_port_range").unwrap_or_default();
    let ends: Vec<u32> = range.split_whitespace().filter_map(|word| word.parse().ok()).collect();
    if ends.len() != 2 || ends[1] - ends[0] >= 200 {
        // The job that narrows the range says so, and the test is not left out there unseen.
        assert!(std::env::var_os("SG_PORTS_NARROWED").is_none(), "SG_PORTS_NARROWED, but the local ports are {}", range.trim());
        eprintln!("the local ports are not narrowed ({}): a_client_out_of_local_ports not run", range.trim());
        return;
    }
    let dir = work("a_client_out_of_local_ports");
    for (mut way, name) in both_ways(&dir, &[("ports", PORTS)]) {
        let (code, stdout, stderr) = ran(&way.finished());
        let lines: Vec<&str> = stdout.lines().collect();
        let counts = lines.iter().find_map(|line| line.strip_prefix("connected: ")).unwrap_or("");
        let connected: u32 = counts.split(',').next().unwrap_or("").parse().unwrap_or(0);
        let all = format!("{name}: exit {code}\n{stdout}\n{stderr}");
        assert_eq!(code, 0, "{all}");
        assert_eq!(lines.first(), Some(&"refused: address not available"), "{all}");
        assert!(connected > 0 && connected <= ends[1] - ends[0] + 1, "{all}");
        assert!(lines.contains(&"async: refused: address not available"), "{all}");
        assert_eq!(lines.last(), Some(&"connected again: ping"), "{all}");
    }
}

/// What `net address` prints of the addresses of `the_trial_of_net`, as Rust and Zig print it.
const ADDRESSES: &str = "addresses 5\nv6 3\nports 74138\ncharacters 75\nsum 299\ninvalid 3\n";

/// Every module of `std` is measured against Rust and Zig before it is done (1553, 1636): it has a
/// part of its own (`## json`) in `benchmarks/library.md`, which holds the measurements, or is named
/// in its list of those not measured yet, which the slices empty.
#[test]
fn every_module_of_std_is_measured() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let section = std::fs::read_to_string(root.join("..").join("benchmarks").join("library.md")).unwrap();
    let waiting = section
        .lines()
        .find_map(|line| line.strip_prefix("Not measured yet: "))
        .expect("the section names the modules not measured yet");
    let waiting: Vec<&str> = waiting.trim_end_matches('.').split(", ").map(|name| name.trim_matches('`')).collect();
    let mut modules: Vec<String> = std::fs::read_dir(root.join("src").join("std"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "sg"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    modules.sort();
    for module in &modules {
        let measured = section.contains(&format!("\n## {module}\n"));
        let listed = waiting.contains(&module.as_str());
        assert!(
            measured != listed,
            "std/{module}.sg: {}",
            if measured { "measured, and still in the list of those not measured yet" } else { "neither measured nor in the list of those not measured yet" }
        );
    }
    for name in &waiting {
        assert!(modules.iter().any(|m| m == name), "`{name}` in the list is not a module of std");
    }
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
    Command::new(env!("CARGO_BIN_EXE_sg")).args(words).current_dir(dir).env("SG_MIRROR", mirror).env("SG_CACHE", cache).stdin(Stdio::null()).finished()
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

/// `Task.processorCount()` (1856) gives the same number interpreted and compiled, and on Linux and
/// macOS the number Rust's `available_parallelism` gives, which counts the same way.
#[test]
fn the_processor_count_is_one_number() {
    if !has_compiler() {
        return;
    }
    let dir = work("the_processor_count_is_one_number");
    let source = write(&dir, "count.sg", "void main() {\n    print(Task.processorCount())\n}\n").to_string_lossy().into_owned();
    let built = program(&dir, "count").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "-o", &built])), (0, String::new(), String::new()));
    let interpreted = ran(&sg(&dir, &[&source]));
    let compiled = ran(&Command::new(&built).current_dir(&dir).stdin(Stdio::null()).finished());
    assert_eq!(interpreted, compiled);
    let n: usize = compiled.1.trim().parse().unwrap();
    assert!(n >= 1);
    if cfg!(unix) {
        assert_eq!(n, std::thread::available_parallelism().unwrap().get());
    }
}

/// `File.openRegular` (1873) opens only a regular file, interpreted and compiled alike, in a plain
/// call and as a task: a folder is `IsDirectory`, a device (`/dev/null`, `NUL` on Windows) and a FIFO
/// are `NotAFile`, and a FIFO is never opened, so a writer waiting for a reader of it is not let go.
/// `reader.info()` asks the open file, so it sees the file grow.
#[test]
fn opens_only_a_regular_file() {
    if !has_compiler() {
        return;
    }
    let dir = work("opens_only_a_regular_file");
    let files = dir.join("files");
    std::fs::create_dir_all(files.join("folder")).unwrap();
    let device = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let source = write(
        &dir,
        "open.sg",
        &format!(
            r#"void show(string path, string shown) {{
    match File.openRegular(read path) {{
        Ok(reader) => {{
            match reader.info() {{
                Ok(info) => print($"{{shown}}: {{info.kind}} {{info.size}}")
                Error(cause) => print($"{{shown}}: info {{cause}}")
            }}
        }}
        Error(cause) => print($"{{shown}}: {{cause}}")
    }}
}}
async void showAsync(string path, string shown) {{
    match await File.openRegularAsync(read path) {{
        Ok(reader) => {{
            match reader.info() {{
                Ok(info) => print($"async {{shown}}: {{info.kind}} {{info.size}}")
                Error(cause) => print($"async {{shown}}: info {{cause}}")
            }}
        }}
        Error(cause) => print($"async {{shown}}: {{cause}}")
    }}
}}
Result<void, IoError> main(List<string> args) {{
    var dir = args[0]
    var path = Path.join(read dir, "a.txt")
    try File.writeText(read path, "hello")
    show(path.clone(), "a.txt")
    show(Path.join(read dir, "folder"), "folder")
    show(Path.join(read dir, "missing"), "missing")
    show("{device}", "device")
    if args.length() > 1 {{
        show(Path.join(read dir, "fifo"), "fifo")
        showAsync(Path.join(read dir, "fifo"), "fifo").wait()
    }}
    showAsync(path.clone(), "a.txt").wait()
    showAsync(Path.join(read dir, "folder"), "folder").wait()
    showAsync("{device}", "device").wait()
    var reader = try File.openText(read path)
    var before = try reader.info()
    try File.appendText(read path, " world")
    var after = try reader.info()
    print($"{{before.size}} -> {{after.size}}")
    print(IoError.NotAFile)
    return Ok()
}}
"#
        ),
    )
    .to_string_lossy()
    .into_owned();
    let built = program(&dir, "open").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "-o", &built])), (0, String::new(), String::new()));
    let folder = files.to_string_lossy().into_owned();
    let mut args = vec![folder.clone()];
    let mut writer = None;
    let released = dir.join("released");
    if cfg!(unix) {
        let fifo = files.join("fifo");
        let _ = std::fs::remove_file(&fifo);
        assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
        let script = format!("echo x > '{}' && touch '{}'", fifo.display(), released.display());
        writer = Some(Command::new("sh").args(["-c", &script]).spawn().unwrap());
        args.push("fifo".to_string());
    }
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let interpreted = ran(&sg(&dir, &[&[source.as_str()][..], &words].concat()));
    let compiled = ran(&Command::new(&built).args(&words).current_dir(&dir).stdin(Stdio::null()).finished());
    assert_eq!(interpreted, compiled);
    let mut want = vec!["a.txt: File 5", "folder: is a folder", "missing: not found", "device: not a file"];
    if cfg!(unix) {
        want.extend(["fifo: not a file", "async fifo: not a file"]);
    }
    want.extend(["async a.txt: File 5", "async folder: is a folder", "async device: not a file", "5 -> 11", "not a file"]);
    assert_eq!(compiled, (0, want.join("\n") + "\n", String::new()));
    if let Some(mut writer) = writer {
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!released.exists(), "a writer waiting on the FIFO was let go");
        assert!(writer.try_wait().unwrap().is_none(), "the writer of the FIFO ended");
        writer.kill().unwrap();
        writer.wait().unwrap();
    }
}

/// The trial of `hashsum` (choices 1854-1859): the SHA-256 of files from a pool of workers, built and
/// run interpreted and compiled, with one worker and with several: the lines of GNU's `sha256sum` in
/// the order of the files, a name with a `\` escaped (not on Windows, where it is a separator), a
/// missing file told and the others going on with the exit code 1, the input once and then at its end,
/// and an option not known (2).
#[test]
fn the_trial_of_hashsum() {
    if !has_compiler() {
        return;
    }
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let dir = work("the_trial_of_hashsum");
    let source = trials.join("hashsum").join("sg").join("hashsum.sg").to_string_lossy().into_owned();
    let built = program(&dir, "hashsum").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &built])), (0, String::new(), String::new()));
    write(&dir, "a", "a");
    write(&dir, "empty", "");
    let input = write(&dir, "input", "a");
    let (a, b, empty) = (
        "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb",
        "3e23e8160039594a33894f6564e1b1348bbd7a0088d42c4acb73eeaed59c009d",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
    let mut words = vec!["a", "missing", "empty", "-", "a", "-"];
    let mut want = format!("{a}  a\n{empty}  empty\n{a}  -\n{a}  a\n{empty}  -\n");
    if cfg!(unix) {
        write(&dir, "back\\slash", "b");
        words.insert(1, "back\\slash");
        want = format!("{a}  a\n\\{b}  back\\\\slash\n{empty}  empty\n{a}  -\n{a}  a\n{empty}  -\n");
    }
    for interpreted in [false, true] {
        for workers in ["1", "3"] {
            let mut command = match interpreted {
                true => {
                    let mut command = Command::new(env!("CARGO_BIN_EXE_sg"));
                    command.arg(&source);
                    command
                }
                false => Command::new(&built),
            };
            let out = command.args(["-j", workers]).args(&words).current_dir(&dir).stdin(std::fs::File::open(&input).unwrap()).finished();
            let (code, stdout, stderr) = ran(&out);
            assert_eq!((code, stdout.as_str()), (1, want.as_str()), "interpreted {interpreted}, -j {workers}");
            assert!(stderr.contains("missing"), "{stderr}");
        }
    }
    let out = Command::new(&built).args(["-x", "a"]).current_dir(&dir).stdin(Stdio::null()).finished();
    assert_eq!((out.status.code(), out.stdout.len()), (Some(2), 0));
}

/// The trial of `giftool` (choices 1874-1881), interpreted and compiled, on a GIF of eight frames with
/// every disposal, a transparent index in each, a loop count and a comment with NULs: `--info` is what
/// `gifsicle -I` writes of it; each option writes the same bytes interpreted and compiled; what it
/// writes, read and written again, is the same bytes; a GIF cut short ends with 1 and writes nothing;
/// an option not known ends with 2. `trials/giftool/tests/check.py` holds it to a model, picture by picture.
/// The interpreter of Lox (choices 1883-1894), compiled and interpreted, held to what clox writes for
/// the same program: a class with a superclass and `super`, a closure, numbers as `%g` writes them,
/// three thousand objects, and an error at run time with its trace; the REPL; a missing file and a
/// wrong use. The loop of choice 1894 (`--ip-local`) and the collector at every allocation
/// (`--stress-gc`) write the same. The book's tests and the comparison with clox itself are in
/// `trials/lox/tests` (`check.py`, `fuzz.py`).
#[test]
fn the_trial_of_lox() {
    if !has_compiler() {
        return;
    }
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let dir = work("the_trial_of_lox");
    let source = trials.join("lox").join("sg").to_string_lossy().into_owned();
    let built = program(&dir, "lox").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &built])), (0, String::new(), String::new()));
    let script = r#"class Shape {
  init(name) { this.name = name; }
  area() { return 0; }
  describe() { return this.name + " of area"; }
}
class Square < Shape {
  init(side) { super.init("square"); this.side = side; }
  area() { return this.side * this.side; }
}
fun counter() {
  var n = 0;
  fun next() { n = n + 1; return n; }
  return next;
}
var c = counter();
c();
var s = Square(1.5);
print s.describe();
print s.area();
print c() + 10;
print 1 / 3;
print 123456789;
print -0;
print "a" + "b" == "ab";
print Square;
print s;
print c;
var list = nil;
class Node { init(v, next) { this.v = v; this.next = next; } }
for (var i = 0; i < 3000; i = i + 1) { list = Node(i, list); }
var total = 0;
while (list != nil) { total = total + list.v; list = list.next; }
print total;
print nil + 1;
"#;
    std::fs::write(dir.join("script.lox"), script).unwrap();
    let output = "square of area\n2.25\n12\n0.333333\n1.23457e+08\n-0\ntrue\nSquare\nSquare instance\n<fn next>\n4.4985e+06\n";
    let error = "Operands must be two numbers or two strings.\n[line 34] in script\n";
    let run = |interpreted: bool, words: &[&str], input: Option<&str>| {
        let mut command = match interpreted {
            true => {
                let mut command = Command::new(env!("CARGO_BIN_EXE_sg"));
                command.arg(&source);
                command
            }
            false => Command::new(&built),
        };
        command.args(words).current_dir(&dir);
        match input {
            Some(input) => command.stdin(std::fs::File::open(dir.join(input)).unwrap()).finished(),
            None => command.stdin(Stdio::null()).finished(),
        }
    };
    std::fs::write(dir.join("repl.txt"), "var a = 1;\nprint a + 1;\nprint b;\nprint a;\nfun\n").unwrap();
    let repl = "repl.txt";
    for interpreted in [false, true] {
        let flags: &[&[&str]] = if interpreted { &[&[]] } else { &[&[], &["--ip-local"], &["--stress-gc"], &["--ip-local", "--stress-gc"]] };
        for flags in flags {
            let out = run(interpreted, &[flags, &["script.lox"][..]].concat(), None);
            assert_eq!(ran(&out), (70, output.to_string(), error.to_string()), "interpreted {interpreted}, {flags:?}");
        }
        let out = run(interpreted, &[], Some(repl));
        let said = "Undefined variable 'b'.\n[line 1] in script\n[line 2] Error at end: Expect function name.\n";
        assert_eq!(ran(&out), (0, "> > 2\n> > 1\n> > \n".to_string(), said.to_string()), "interpreted {interpreted}");
        let out = run(interpreted, &["missing.lox"], None);
        assert_eq!(ran(&out), (74, String::new(), "Could not open file \"missing.lox\".\n".to_string()), "interpreted {interpreted}");
        let out = run(interpreted, &["a.lox", "b.lox"], None);
        assert_eq!(ran(&out), (64, String::new(), "Usage: clox [path]\n".to_string()), "interpreted {interpreted}");
    }
}

#[test]
fn the_trial_of_giftool() {
    if !has_compiler() {
        return;
    }
    let trials = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("trials");
    let dir = work("the_trial_of_giftool");
    let source = trials.join("giftool").join("sg").join("giftool.sg").to_string_lossy().into_owned();
    let built = program(&dir, "giftool").to_string_lossy().into_owned();
    assert_eq!(ran(&sg(&dir, &["build", &source, "--release", "-o", &built])), (0, String::new(), String::new()));
    let sample = std::fs::read(trials.join("giftool").join("tests").join("sample.gif")).unwrap();
    std::fs::write(dir.join("sample.gif"), &sample).unwrap();
    std::fs::write(dir.join("cut.gif"), &sample[..sample.len() / 2]).unwrap();
    let info = "* sample.gif 8 images\n  logical screen 70x50\n  global color table [4]\n  background 0\n  loop count 3\n  \
                end comment \\000a\n  + image #0 30x20 transparent 0\n  + image #1 30x20 at 5,3 transparent 1\n    \
                disposal asis delay 0.07s\n  + image #2 30x20 at 10,6 transparent 2\n    disposal background delay 0.14s\n  \
                + image #3 30x20 at 15,9 transparent 3\n    disposal previous delay 0.21s\n  + image #4 30x20 at 20,12 \
                transparent 0\n    disposal previous delay 0.28s\n  + image #5 30x20 at 25,15 transparent 1\n    \
                disposal background delay 0.35s\n  + image #6 30x20 at 30,18 transparent 2\n    disposal asis delay \
                0.42s\n  + image #7 30x20 at 35,21 transparent 3\n    delay 0.49s\n";
    let run = |interpreted: bool, words: &[&str]| {
        let mut command = match interpreted {
            true => {
                let mut command = Command::new(env!("CARGO_BIN_EXE_sg"));
                command.arg(&source);
                command
            }
            false => Command::new(&built),
        };
        command.args(words).current_dir(&dir).stdin(Stdio::null()).finished()
    };
    let options: [&[&str]; 7] = [
        &[],
        &["--unoptimize"],
        &["-O2"],
        &["-O1", "--rotate-90"],
        &["--crop", "10,5+40x30", "--flip-horizontal"],
        &["--rotate-270"],
        &["--unoptimize", "--flip-vertical", "--rotate-180"],
    ];
    for interpreted in [false, true] {
        let out = run(interpreted, &["--info", "sample.gif"]);
        assert_eq!(ran(&out), (0, info.to_string(), String::new()), "interpreted {interpreted}");
        let out = run(interpreted, &["cut.gif"]);
        assert_eq!((out.status.code(), out.stdout.len()), (Some(1), 0), "interpreted {interpreted}");
        assert!(text(&out.stderr).contains("cut.gif"), "{}", text(&out.stderr));
        let out = run(interpreted, &["--sideways", "sample.gif"]);
        assert_eq!((out.status.code(), out.stdout.len()), (Some(2), 0), "interpreted {interpreted}");
    }
    for words in options {
        let compiled = run(false, &[words, &["sample.gif"]].concat());
        assert_eq!(compiled.status.code(), Some(0), "{words:?}: {}", text(&compiled.stderr));
        let interpreted = run(true, &[words, &["sample.gif"]].concat());
        assert!(interpreted.stdout == compiled.stdout, "{words:?}: interpreted and compiled differ");
        std::fs::write(dir.join("again.gif"), &compiled.stdout).unwrap();
        let again = run(false, &["again.gif"]);
        assert!(again.stdout == compiled.stdout, "{words:?}: written again, it changed");
    }
}
