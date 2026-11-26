//! The lock of a file between two programs (choice 1979): one holds it and the other tries it or
//! waits for it, each interpreted and compiled, in every pairing. `tests/locks/locks.sg` is the
//! program. A lock goes when its program lets it go, and when the program ends without doing so.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const LIMIT: Duration = Duration::from_secs(120);

fn source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("locks").join("locks.sg")
}

/// A folder of its own in the temporary folder (not under the user's documents, where macOS
/// may hold a program that opens a file).
fn work(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("sg-locks-tests").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// The words that start the program: the interpreter with the file, and the compiled program.
fn launchers(dir: &Path) -> Vec<(&'static str, Vec<OsString>)> {
    let sg = OsString::from(env!("CARGO_BIN_EXE_sg"));
    let program = PathBuf::from(sg::cc::program_file(&dir.join("locks").to_string_lossy()));
    let built = Command::new(&sg).arg("build").arg(source()).arg("-o").arg(&program).output().unwrap();
    assert!(built.status.success(), "sg build failed:\n{}", text(&built));
    vec![("interpreted", vec![sg, source().into_os_string()]), ("compiled", vec![program.into_os_string()])]
}

fn command(launcher: &[OsString], words: &[&str], path: &Path) -> Command {
    let mut command = Command::new(&launcher[0]);
    command.args(&launcher[1..]).arg(words[0]).arg(path).args(&words[1..]);
    command
}

/// Runs the program to its end, within the limit.
fn run(launcher: &[OsString], words: &[&str], path: &Path) -> String {
    let mut child = command(launcher, words, path).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > LIMIT {
            let _ = child.kill();
            panic!("{words:?} did not end within {} s", LIMIT.as_secs());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{words:?} failed:\n{}", text(&out));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A holder started and waited for until it says it holds the lock; its output is read line by
/// line by a thread, which sends each on.
struct Holder {
    child: Child,
    lines: std::sync::mpsc::Receiver<String>,
}

impl Holder {
    fn start(launcher: &[OsString], words: &[&str], path: &Path) -> Holder {
        let mut child = command(launcher, words, path).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
        let out = child.stdout.take().unwrap();
        let (send, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        let mut holder = Holder { child, lines };
        assert_eq!(holder.next(), "held");
        holder
    }

    fn next(&mut self) -> String {
        match self.lines.recv_timeout(LIMIT) {
            Ok(line) => line,
            Err(_) => {
                let _ = self.child.kill();
                panic!("the holder said nothing within {} s", LIMIT.as_secs());
            }
        }
    }

    /// The line that makes it let go, and what it says then.
    fn release(mut self) {
        let mut input = self.child.stdin.take().unwrap();
        input.write_all(b"go\n").unwrap();
        drop(input);
        assert_eq!(self.next(), "let go");
        assert!(self.child.wait().unwrap().success());
    }
}

#[test]
fn a_lock_is_seen_by_another_program() {
    let dir = work("seen");
    let path = dir.join("locked.txt");
    std::fs::write(&path, "hello").unwrap();
    let launchers = launchers(&dir);
    for (holder_name, holder_launcher) in &launchers {
        for (other_name, other) in &launchers {
            let pair = format!("{holder_name} holds, {other_name} tries");
            // Alone: neither lock is got.
            let holder = Holder::start(holder_launcher, &["hold"], &path);
            assert_eq!(run(other, &["try"], &path), "false false", "{pair}, alone");
            holder.release();
            assert_eq!(run(other, &["try"], &path), "true true", "{pair}, let go");
            // Shared: another shared lock is got, one alone is not.
            let holder = Holder::start(holder_launcher, &["hold", "shared"], &path);
            assert_eq!(run(other, &["try"], &path), "false true", "{pair}, shared");
            holder.release();
        }
    }
}

#[test]
fn a_lock_is_waited_for_and_goes_with_its_program() {
    let dir = work("waited");
    let path = dir.join("locked.txt");
    std::fs::write(&path, "hello").unwrap();
    let launchers = launchers(&dir);
    for (holder_name, holder_launcher) in &launchers {
        for (other_name, other) in &launchers {
            let pair = format!("{holder_name} holds, {other_name} waits");
            // `lockAsync` waits while the lock is held, and gets it once it is let go.
            let holder = Holder::start(holder_launcher, &["hold"], &path);
            let mut waiter = command(other, &["wait"], &path).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
            std::thread::sleep(Duration::from_millis(300));
            assert!(waiter.try_wait().unwrap().is_none(), "{pair}: the waiter did not wait");
            holder.release();
            let start = Instant::now();
            while waiter.try_wait().unwrap().is_none() {
                assert!(start.elapsed() < LIMIT, "{pair}: the waiter did not get the lock");
                std::thread::sleep(Duration::from_millis(5));
            }
            let out = waiter.wait_with_output().unwrap();
            assert_eq!(text(&out).trim(), "got", "{pair}");
            // A program killed with the lock leaves it to the next.
            let mut holder = Holder::start(holder_launcher, &["hold"], &path);
            holder.child.kill().unwrap();
            holder.child.wait().unwrap();
            assert_eq!(run(other, &["try"], &path), "true true", "{pair}, killed");
        }
    }
}
