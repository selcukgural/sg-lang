//! A compiled program in the system's debugger (choices 1943 to 1948): a debug build carries what
//! a debugger reads, and the C names the lines of SG, so that a breakpoint is set on a line of a
//! `.sg` file, the frames of the stack name the program's own files and lines, and the source is
//! shown from where it lies however the debugger was started. lldb on macOS, gdb on Linux; on
//! Windows, where the CI has no debugger that runs without a window, the `.pdb` beside the
//! program is read for the files of SG it names.
//!
//! The program lies in a folder whose name has a space and letters that are not ASCII, and the
//! debugger runs in another folder, so that only the absolute path the C gives finds its source.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const MAIN: &str = "use shapes

int sum(read List<int> xs) {
    var total = 0
    for x in xs {
        total += x
    }
    return total
}

void main(List<string> args) {
    var xs = List<int>()
    xs.add(3)
    xs.add(4)
    print(sum(read xs))
    print(shapes.area((int)args.length() + 2, 5))
}
";

const AREA: &str = "public int area(int width, int height) {
    var product = width * height
    return product
}
";

/// The line of `total += x` in `MAIN`, of the call of `sum`, of `var product` in `AREA` and of the
/// `print` of the area. The
/// width comes from the command line, so that an optimised build cannot work `area` out before
/// it runs.
const ADD_LINE: usize = 6;
const CALL_LINE: usize = 15;
const AREA_LINE: usize = 2;
const PRINT_LINE: usize = 16;

/// The longest a build or a debugger may take: far more than either does.
const LIMIT: std::time::Duration = std::time::Duration::from_secs(300);

fn finished(command: &mut Command) -> Output {
    use std::io::Read;
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap_or_else(|error| panic!("cannot start {:?}: {error}", command.get_program()));
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
        if start.elapsed() > LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{:?} {:?} did not end within {} s", command.get_program(), command.get_args().collect::<Vec<_>>(), LIMIT.as_secs());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    Output { status, stdout: out.join().unwrap(), stderr: err.join().unwrap() }
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// The program and its module written into a folder of their own, under a name with a space
/// and letters that are not ASCII. The folder is in the system's temporary folder, not in the
/// repository's `target/`: on macOS a program the debugger starts waits for the user's leave to
/// open a file under `~/Documents` or `~/Desktop`, where a repository often lies, and would wait
/// for ever.
fn written(name: &str) -> PathBuf {
    let top = std::env::temp_dir().join("sg-debugger-tests").join(name);
    let _ = std::fs::remove_dir_all(&top);
    let dir = top.join("çalışma klasörü");
    std::fs::create_dir_all(dir.join("shapes")).unwrap();
    std::fs::write(dir.join("main.sg"), MAIN).unwrap();
    std::fs::write(dir.join("shapes").join("area.sg"), AREA).unwrap();
    dir
}

/// `sg build main.sg` in `dir` with these words; the program it made.
fn built(dir: &Path, stem: &str, words: &[&str]) -> PathBuf {
    let out = finished(Command::new(env!("CARGO_BIN_EXE_sg")).args(["build", "main.sg", "-o", stem]).args(words).current_dir(dir));
    assert!(out.status.success(), "sg build failed:\n{}", text(&out));
    PathBuf::from(sg::cc::program_file(&dir.join(stem).to_string_lossy()))
}

/// The debugger of this system, run in the folder above the program's with these commands, each
/// after a breakpoint is met; what it wrote.
fn debugged(program: &Path, breaks: &[String], commands: &[&str]) -> String {
    let above = program.parent().and_then(Path::parent).unwrap();
    let mut command;
    if cfg!(target_os = "macos") {
        command = Command::new("lldb");
        command.arg("--batch").arg("--no-lldbinit");
        for at in breaks {
            command.arg("-o").arg(format!("breakpoint set --file {at}"));
        }
        command.arg("-o").arg("run");
        for each in commands {
            command.arg("-o").arg(each);
        }
        command.arg("-o").arg("kill").arg("--").arg(program);
    } else {
        command = Command::new("gdb");
        command.args(["-batch", "-nx", "-ex", "set pagination off", "-ex", "set breakpoint pending on"]);
        for at in breaks {
            command.arg("-ex").arg(format!("break {at}"));
        }
        command.arg("-ex").arg("run");
        for each in commands {
            command.arg("-ex").arg(each);
        }
        command.arg("-ex").arg("kill").arg(program);
    }
    let out = finished(command.current_dir(above));
    text(&out)
}

/// lldb names a breakpoint's place by the file and the line; gdb by the same with a colon.
fn place(file: &str, line: usize) -> String {
    if cfg!(target_os = "macos") {
        format!("{file} --line {line}")
    } else {
        format!("{file}:{line}")
    }
}

/// The commands that show the stack, the variables of the frame and the source of the frame.
fn stack() -> &'static str {
    if cfg!(target_os = "macos") { "thread backtrace" } else { "bt" }
}

/// The first breakpoint is let go, so that the loop does not stop on it again.
fn forget_first() -> &'static str {
    if cfg!(target_os = "macos") { "breakpoint delete --force 1" } else { "delete 1" }
}

fn locals() -> &'static str {
    if cfg!(target_os = "macos") { "frame variable" } else { "info locals" }
}

#[test]
#[cfg(not(windows))]
fn a_debug_build_stops_on_lines_of_sg() {
    let dir = written("lines");
    let program = built(&dir, "prog", &[]);
    let said = debugged(&program, &[place("main.sg", ADD_LINE), place("area.sg", AREA_LINE)], &[stack(), locals(), forget_first(), "continue", stack(), locals()]);
    // The first stop: in `sum`, called from `main`, both named by the lines of `main.sg`.
    assert!(said.contains(&format!("main.sg:{ADD_LINE}")), "no stop at main.sg:{ADD_LINE}:\n{said}");
    assert!(said.contains(&format!("main.sg:{CALL_LINE}")), "the frame of main is not at main.sg:{CALL_LINE}:\n{said}");
    assert!(said.contains("f_sum") && said.contains("f_main"), "the frames do not name sum and main:\n{said}");
    assert!(said.contains("v_total"), "the variable total is not shown:\n{said}");
    // The source is shown, though the debugger runs in another folder: found by its absolute path.
    assert!(said.contains("total += x"), "the line of SG is not shown:\n{said}");
    // The second stop: in the module's file, in its folder.
    assert!(said.contains(&format!("area.sg:{AREA_LINE}")), "no stop at area.sg:{AREA_LINE}:\n{said}");
    assert!(said.contains("var product = width * height"), "the line of the module is not shown:\n{said}");
    assert!(said.contains("v_width") && said.contains("v_height"), "the parameters of area are not shown:\n{said}");
    // The C's own lines name the C file, not a line of SG (choice 1945).
    assert!(!said.contains("main.sg:0"), "a line 0 of SG:\n{said}");
}

/// An optimised build given `-g` (choice 1944): what is computed is folded and spread over the
/// helpers it is inlined into, so a line of arithmetic may have no place of its own, as in any
/// optimised C; a line that calls (`print`) keeps one, and the stack names it.
#[test]
#[cfg(not(windows))]
fn a_release_build_with_g_stops_on_lines_of_sg() {
    let dir = written("release");
    let program = built(&dir, "prog", &["--release", "--", "-g"]);
    let said = debugged(&program, &[place("main.sg", PRINT_LINE)], &[stack()]);
    assert!(said.contains(&format!("main.sg:{PRINT_LINE}")), "no stop at main.sg:{PRINT_LINE}:\n{said}");
    assert!(said.contains("print(shapes.area"), "the line of SG is not shown:\n{said}");
}

#[test]
#[cfg(not(windows))]
fn a_release_build_carries_no_lines() {
    let dir = written("plain");
    let program = built(&dir, "prog", &["--release"]);
    let bytes = std::fs::read(&program).unwrap();
    let found = bytes.windows(b"main.sg".len()).any(|window| window == b"main.sg");
    assert!(!found, "a release build names its file of SG");
    assert!(!program.with_extension("dSYM").exists(), "a release build has a .dSYM");
}

/// On Windows the `.pdb` names the files of SG by their paths, the folder that is not ASCII
/// in UTF-8.
#[test]
#[cfg(windows)]
fn a_debug_build_names_its_files_of_sg_in_its_pdb() {
    let dir = written("pdb");
    let program = built(&dir, "prog", &[]);
    let pdb = std::fs::read(program.with_extension("pdb")).expect("a debug build writes a .pdb");
    let has = |what: &str| pdb.windows(what.len()).any(|window| window == what.as_bytes());
    assert!(has("main.sg"), "the .pdb does not name main.sg");
    assert!(has("area.sg"), "the .pdb does not name area.sg");
    assert!(has("çalışma klasörü"), "the .pdb does not name the folder as it is written");
}

/// The C names the language's own module by the name messages give it (choice 1947), and the
/// C after the lines of SG by the C file of the build (1945).
#[test]
fn the_c_names_the_lines_of_its_files() {
    let dir = written("emitted");
    std::fs::write(dir.join("paths.sg"), "void main() {\n    print(Path.join(\"a\", \"b\"))\n}\n").unwrap();
    let out = finished(Command::new(env!("CARGO_BIN_EXE_sg")).args(["emit-c", "paths.sg"]).current_dir(&dir));
    assert!(out.status.success(), "{}", text(&out));
    let code = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(code.contains("\"<built-in>/std/path.sg\""), "the module's own file is not named");
    let own = code.lines().find(|line| line.starts_with("#line 2 ") && line.contains("paths.sg")).expect("a #line of paths.sg");
    let path = own.trim_start_matches("#line 2 ");
    assert!(path.starts_with('"') && Path::new(&decoded(path)).is_absolute(), "the path is not absolute: {own}");
    // The same file, however the folders on the way are written (`/var` is `/private/var` on macOS).
    assert_eq!(std::fs::canonicalize(decoded(path)).unwrap(), std::fs::canonicalize(dir.join("paths.sg")).unwrap());
    // Each `#line` of the C file names the line after it.
    for (at, line) in code.lines().enumerate() {
        if let Some(rest) = line.strip_prefix("#line ").filter(|rest| rest.ends_with(" \"paths.c\"")) {
            let number: usize = rest.split(' ').next().unwrap().parse().unwrap();
            assert_eq!(number, at + 2, "{line}");
        }
    }
    assert!(code.lines().any(|line| line.ends_with(" \"paths.c\"")), "no line gives the C back its own lines");
    assert!(!code.lines().any(|line| line.starts_with("#line") && line.contains(" @")), "a mark was left");
}

/// A C string as `#line` takes it, back to its text: `\\`, `\"`, `\?` and three octal digits.
fn decoded(literal: &str) -> String {
    let inner = &literal[1..literal.len() - 1];
    let bytes = inner.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' {
            let c = bytes[at + 1];
            if c.is_ascii_digit() {
                out.push(u8::from_str_radix(&inner[at + 1..at + 4], 8).unwrap());
                at += 4;
                continue;
            }
            out.push(c);
            at += 2;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8(out).unwrap()
}
