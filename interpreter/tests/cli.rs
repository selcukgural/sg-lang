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
//! Windows.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A folder of its own for each test, made new: the tests run at once.
fn work(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("cli").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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
