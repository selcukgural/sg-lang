//! A root under attack, a file with no name, and one file read and written through one opening
//! (choices 2066 to 2077), each interpreted and compiled. `tests/roots/roots.sg` is the program.
//!
//! - While a program opens, looks at and lists names under a root a hundred thousand times, this
//!   test swaps those names between what lies inside the root and links that lead out of it: not
//!   one byte from outside is read, nothing outside is listed, and nothing is made there. The
//!   compiled program runs twice: as it is built, and built to walk every path part by part
//!   (`-DSG_ROOT_WALK_ONLY`), which is what it does where the system has no call that keeps a
//!   path under a folder.
//! - A file of `File.openTemp()` leaves nothing in the folder of temporary files, also when its
//!   program is ended from outside while it holds the file.
//! - A gigabyte goes through such a file and comes back as it was written.
//! - A file written and read at places a seed gives, through `File.openUpdate` and
//!   `writer.openReader()`, ends with the bytes that a program of C leaves with a stream opened
//!   with "r+" (`tests/roots/update.c`), and the two read the same.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const LIMIT: Duration = Duration::from_secs(300);

fn source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("roots").join("roots.sg")
}

/// A folder of its own in the temporary folder (not under the user's documents, where macOS
/// may hold a program that opens a file).
fn work(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("sg-roots-tests").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// The program compiled into `dir` under `name`, with the words given to `sg build` after the
/// file.
fn built(dir: &Path, name: &str, words: &[&str]) -> Vec<OsString> {
    let sg = OsString::from(env!("CARGO_BIN_EXE_sg"));
    let program = PathBuf::from(sg::cc::program_file(&dir.join(name).to_string_lossy()));
    let mut command = Command::new(&sg);
    command.arg("build").arg(source()).arg("-o").arg(&program);
    // `--release` comes before the `--` that starts the flags of C.
    let (ours, c_flags): (Vec<&str>, Vec<&str>) = words.iter().partition(|word| word.starts_with("--"));
    command.args(ours);
    if !c_flags.is_empty() {
        command.arg("--").args(c_flags);
    }
    let out = command.output().unwrap();
    assert!(out.status.success(), "sg build failed:\n{}", text(&out));
    vec![program.into_os_string()]
}

fn interpreted() -> Vec<OsString> {
    vec![OsString::from(env!("CARGO_BIN_EXE_sg")), source().into_os_string()]
}

/// Runs the program to its end, within the limit, and gives what it printed.
fn run(launcher: &[OsString], words: &[&str], vars: &[(&str, &Path)]) -> String {
    let mut command = Command::new(&launcher[0]);
    command.args(&launcher[1..]).args(words).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (name, value) in vars {
        command.env(name, value);
    }
    let mut child = command.spawn().unwrap();
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

/// A link to a folder or to a file; `false` where this user may make none (Windows without the
/// mode of a developer).
fn link(target: &str, at: &Path, folder: bool) -> bool {
    #[cfg(unix)]
    {
        let _ = folder;
        std::os::unix::fs::symlink(target, at).is_ok()
    }
    #[cfg(windows)]
    {
        let target = target.replace('/', "\\");
        match folder {
            true => std::os::windows::fs::symlink_dir(target, at).is_ok(),
            false => std::os::windows::fs::symlink_file(target, at).is_ok(),
        }
    }
}

/// The folder of the race: `site` is the root, with a folder and a file that lie inside it, and
/// links that lead to the folder and the file outside.
fn lay_out(dir: &Path) -> bool {
    let (site, out) = (dir.join("site"), dir.join("out"));
    std::fs::create_dir_all(site.join("dir_in")).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(site.join("dir_in").join("data.txt"), "inside").unwrap();
    std::fs::write(site.join("file_in"), "inside").unwrap();
    std::fs::write(out.join("data.txt"), "the secret").unwrap();
    std::fs::write(out.join("marker-outside"), "").unwrap();
    link("../out", &site.join("lnk"), true) && link("../out/data.txt", &site.join("lnk_file"), false)
}

#[test]
fn a_root_holds_while_its_names_are_swapped() {
    let base = work("race");
    let build = base.join("build");
    std::fs::create_dir_all(&build).unwrap();
    // A round is four calls and now and then a fifth: 25000 rounds are a hundred thousand.
    let runs = [("interpreted", interpreted(), "4000"), ("compiled", built(&build, "roots", &[]), "25000"), ("compiled to walk", built(&build, "roots-walk", &["-DSG_ROOT_WALK_ONLY"]), "25000")];
    for (name, launcher, rounds) in &runs {
        let dir = base.join(name.replace(' ', "-"));
        if !lay_out(&dir) {
            eprintln!("no link can be made here: the race is not run");
            return;
        }
        let site = dir.join("site");
        let mut child = Command::new(&launcher[0])
            .args(&launcher[1..])
            .arg("race")
            .arg(&dir)
            .arg(rounds)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // The names the program asks for are now what lies inside, now a link that leads out,
        // and missing only for the moment between two renames. A rename the system refuses (on
        // Windows, of a folder with an open file in it) is tried again the next time round.
        let pairs = [("dir_in", "lnk", "flip"), ("file_in", "lnk_file", "leaf.txt")];
        // Which of the two the asked name is now: 0 none, 1 what lies inside, 2 the link.
        let mut now = [0u8; 2];
        let start = Instant::now();
        let mut swaps = 0u64;
        while child.try_wait().unwrap().is_none() {
            assert!(start.elapsed() < LIMIT, "{name}: the program did not end within {} s", LIMIT.as_secs());
            for (which, (inside, outside, asked)) in pairs.iter().enumerate() {
                let held = [*inside, *inside, *outside][now[which] as usize];
                if now[which] != 0 && std::fs::rename(site.join(asked), site.join(held)).is_err() {
                    continue;
                }
                let next = if now[which] == 1 { 2 } else { 1 };
                let from = if next == 1 { *inside } else { *outside };
                match std::fs::rename(site.join(from), site.join(asked)) {
                    Ok(()) => {
                        now[which] = next;
                        swaps += 1;
                    }
                    Err(_) => now[which] = 0,
                }
            }
            std::thread::yield_now();
        }
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "{name}: the program failed:\n{}", text(&out));
        let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let last = said.lines().last().unwrap_or("");
        let numbers: Vec<u64> = last.split_whitespace().filter_map(|word| word.parse().ok()).collect();
        assert_eq!(numbers.len(), 3, "{name}: {said}");
        let (inside, outside, errors) = (numbers[0], numbers[1], numbers[2]);
        assert_eq!(outside, 0, "{name}: something outside the root was reached:\n{said}");
        assert!(inside > 0, "{name}: nothing inside was ever read ({swaps} swaps):\n{said}");
        assert!(swaps > 0 && errors > 0, "{name}: the names were never found swapped ({swaps} swaps):\n{said}");
        assert!(!dir.join("out").join("made.txt").exists(), "{name}: a file was made outside the root");
        assert_eq!(std::fs::read_to_string(dir.join("out").join("data.txt")).unwrap(), "the secret", "{name}: the file outside was written");
        eprintln!("{name}: inside {inside}, refused or missing {errors}, {swaps} swaps");
    }
}

/// The names in a folder.
fn names_in(folder: &Path) -> Vec<OsString> {
    std::fs::read_dir(folder).unwrap().map(|entry| entry.unwrap().file_name()).collect()
}

#[test]
fn a_file_with_no_name_goes_with_its_program() {
    let base = work("temp");
    let build = base.join("build");
    std::fs::create_dir_all(&build).unwrap();
    for (name, launcher) in [("interpreted", interpreted()), ("compiled", built(&build, "roots", &[]))] {
        for killed in [false, true] {
            let folder = base.join(format!("{name}-{killed}"));
            std::fs::create_dir_all(&folder).unwrap();
            let mut child = Command::new(&launcher[0])
                .args(&launcher[1..])
                .arg("temp")
                .env("TMPDIR", &folder)
                .env("TMP", &folder)
                .env("TEMP", &folder)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
            assert_eq!(lines.next().unwrap().unwrap(), "ready", "{name}");
            // While it is open the file has no name on Linux and macOS; Windows shows one.
            let held = names_in(&folder);
            assert_eq!(held.len(), usize::from(cfg!(windows)), "{name}: {held:?}");
            match killed {
                true => child.kill().unwrap(),
                false => {
                    child.stdin.take().unwrap().write_all(b"go\n").unwrap();
                    assert_eq!(lines.next().unwrap().unwrap(), "closed", "{name}");
                }
            }
            child.wait().unwrap();
            // Windows lets a file go a moment after its last handle is closed.
            let start = Instant::now();
            while !names_in(&folder).is_empty() && start.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(names_in(&folder).is_empty(), "{name}, killed {killed}: left behind {:?}", names_in(&folder));
        }
    }
}

#[test]
fn a_gigabyte_through_a_file_with_no_name() {
    let base = work("big");
    let folder = base.join("tmp");
    std::fs::create_dir_all(&folder).unwrap();
    let vars: [(&str, &Path); 3] = [("TMPDIR", &folder), ("TMP", &folder), ("TEMP", &folder)];
    let compiled = built(&base, "roots", &["--release"]);
    assert_eq!(run(&compiled, &["big", "1024"], &vars), "1073741824 bytes, 0 wrong, 0 beyond the end");
    assert_eq!(run(&interpreted(), &["big", "16"], &vars), "16777216 bytes, 0 wrong, 0 beyond the end");
    assert!(names_in(&folder).is_empty(), "left behind {:?}", names_in(&folder));
}

#[cfg(unix)]
#[test]
fn one_opening_reads_and_writes_as_c_does() {
    let base = work("update");
    let c_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("roots").join("update.c");
    let in_c = base.join("update_c");
    let compiler = std::env::var_os("CC").unwrap_or_else(|| OsString::from("cc"));
    let made = Command::new(&compiler).arg("-O1").arg("-o").arg(&in_c).arg(&c_source).output().unwrap();
    assert!(made.status.success(), "the C compiler failed:\n{}", text(&made));
    let launchers = [("C", vec![in_c.into_os_string()]), ("interpreted", interpreted()), ("compiled", built(&base, "roots", &[]))];
    let start: Vec<u8> = (0..10000u32).map(|at| (at * 7 % 256) as u8).collect();
    for (seed, ops) in [("1", "400"), ("12345", "5000"), ("987654321", "20000")] {
        let mut results = Vec::new();
        for (name, launcher) in &launchers {
            let file = base.join(format!("{name}-{seed}.bin"));
            std::fs::write(&file, &start).unwrap();
            let file_text = file.to_string_lossy().to_string();
            // The program of C takes the path first; the program of SG its command before it.
            let said = match *name {
                "C" => run(launcher, &[&file_text, seed, ops], &[]),
                _ => run(launcher, &["update", &file_text, seed, ops], &[]),
            };
            results.push((name, said, std::fs::read(&file).unwrap()));
        }
        let (_, c_said, c_bytes) = &results[0];
        for (name, said, bytes) in &results[1..] {
            assert_eq!(said, c_said, "seed {seed}: {name} read something else than C");
            assert!(bytes == c_bytes, "seed {seed}: {name} left a file of {} bytes that differs from C's of {}", bytes.len(), c_bytes.len());
        }
    }
}
