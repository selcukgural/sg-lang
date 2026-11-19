//! Fuzzing for differences (choices 1324 and 1361-1368). Each seed gives a program (`gen.rs`);
//! the interpreter runs it (the `sg` program, in a process of its own, choice 1368), and so does
//! its C, compiled as a debug build under the address and undefined-behaviour sanitizers. The
//! two must print the same, a panic's message and the code included. When the program does not
//! panic, its release build (`-O2`, under the undefined-behaviour sanitizer) must print what the
//! debug build printed (choice 1364).
//!
//!     sg-difference gen <seed>                 the program of a seed
//!     sg-difference once <seed>                runs it and says what was found
//!     sg-difference run <out> <seconds> [jobs] [first seed]
//!                                              runs seeds one after another, `jobs` at once
//!     sg-difference reduce <seed> <out.sg>     the smallest program that shows the same
//!
//! `SG` names the `sg` program (`sg` of the path when not set) and `CC` the C compiler (`clang`).

mod gen;
mod rng;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gen::{render, Gen, Node};

/// How long one run of a program may take: the programs are made to end in far less.
const LIMIT: Duration = Duration::from_secs(10);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seed = |at: usize| -> u64 { args.get(at).and_then(|text| text.parse().ok()).unwrap_or_else(|| usage()) };
    match args.first().map(String::as_str) {
        Some("gen") => print!("{}", render(&Gen::new(seed(1)).program())),
        Some("once") => {
            let work = std::env::temp_dir().join(format!("sg-difference-{}", std::process::id()));
            let outcome = try_program(&render(&Gen::new(seed(1)).program()), &work);
            println!("{}", outcome.describe());
            let _ = std::fs::remove_dir_all(&work);
        }
        Some("run") => {
            let out = PathBuf::from(args.get(1).unwrap_or_else(|| usage()));
            let seconds = seed(2);
            let jobs = args.get(3).and_then(|text| text.parse().ok()).unwrap_or(4);
            let first = args.get(4).and_then(|text| text.parse().ok()).unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() << 20);
            run(&out, Duration::from_secs(seconds), jobs, first);
        }
        Some("reduce") => {
            let out = PathBuf::from(args.get(2).unwrap_or_else(|| usage()));
            reduce(seed(1), &out);
        }
        _ => usage(),
    }
}

fn usage() -> ! {
    eprintln!("usage: sg-difference gen <seed> | once <seed> | run <out> <seconds> [jobs] [first seed] | reduce <seed> <out.sg>");
    std::process::exit(2)
}

/// What one program came to.
enum Outcome {
    /// The checker refused it: the generator wrote what it should not have, or the checker is
    /// stricter than it should be. Read, not counted as a difference (choice 1362).
    Rejected(String),
    /// Both ran and printed the same; whether the program ended in a panic.
    Same { panicked: bool },
    /// A finding: its kind, and what each side printed.
    Found { kind: &'static str, sides: Vec<(&'static str, String)> },
}

impl Outcome {
    fn describe(&self) -> String {
        match self {
            Outcome::Rejected(message) => format!("rejected\n{message}"),
            Outcome::Same { panicked } => format!("same{}", if *panicked { " (a panic)" } else { "" }),
            Outcome::Found { kind, sides } => {
                let mut text = format!("FOUND {kind}\n");
                for (side, output) in sides {
                    text.push_str(&format!("--- {side}\n{output}\n"));
                }
                text
            }
        }
    }

    fn kind(&self) -> Option<&'static str> {
        match self {
            Outcome::Found { kind, .. } => Some(kind),
            _ => None,
        }
    }
}

/// What a process printed, its standard error after its output, and how it ended.
struct Ran {
    text: String,
    code: Option<i32>,
    timed_out: bool,
}

fn run_limited(command: &mut Command) -> Ran {
    let mut child = match command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(error) => return Ran { text: format!("cannot start: {error}"), code: None, timed_out: false },
    };
    // The pipes are read on threads, so that a program that prints much does not wait on them.
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut bytes);
        bytes
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stderr, &mut bytes);
        bytes
    });
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break Some(status);
        }
        if start.elapsed() > LIMIT {
            let _ = child.kill();
            timed_out = true;
            break child.wait().ok();
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    let text = format!("{}{}", String::from_utf8_lossy(&out), String::from_utf8_lossy(&err));
    let code = status.and_then(|status| status.code());
    #[cfg(unix)]
    let text = match status.and_then(|status| std::os::unix::process::ExitStatusExt::signal(&status)) {
        Some(signal) => format!("{text}(signal {signal})\n"),
        None => text,
    };
    Ran { text, code, timed_out }
}

fn sg() -> String {
    std::env::var("SG").unwrap_or_else(|_| "sg".to_string())
}

fn cc() -> String {
    std::env::var("CC").ok().filter(|cc| !cc.is_empty()).unwrap_or_else(|| "clang".to_string())
}

/// The output as the golden tests show it: the text, then the code when it is not 0.
fn shown(ran: &Ran) -> String {
    match (ran.timed_out, ran.code) {
        (true, _) => format!("{}(did not end in {} seconds)\n", ran.text, LIMIT.as_secs()),
        (false, Some(0)) => ran.text.clone(),
        (false, Some(code)) => format!("{}exit code {code}\n", ran.text),
        (false, None) => ran.text.clone(),
    }
}

/// A fault of `sg` itself rather than of the program.
fn tool_fault(ran: &Ran) -> bool {
    ran.timed_out || ran.code == Some(101) || ran.code.is_none() || ran.text.contains("mistake in sg") || ran.text.contains("panicked at")
}

fn sanitizer_report(text: &str) -> bool {
    text.contains("runtime error:") || text.contains("AddressSanitizer") || text.contains("UndefinedBehaviorSanitizer") || text.contains("LeakSanitizer")
}

/// The C of the program, debug or release, built under the sanitizers in `work`.
fn build(work: &Path, release: bool) -> Result<PathBuf, (&'static str, String)> {
    let mut emit = Command::new(sg());
    emit.current_dir(work).arg("emit-c").arg("prog.sg");
    if release {
        emit.arg("--release");
    }
    let emitted = run_limited(&mut emit);
    if emitted.code != Some(0) {
        return Err(if tool_fault(&emitted) { ("tool", shown(&emitted)) } else { ("rejected", emitted.text) });
    }
    let name = if release { "release" } else { "debug" };
    let c_file = work.join(format!("{name}.c"));
    std::fs::write(&c_file, &emitted.text).map_err(|error| ("tool", format!("cannot write the C: {error}")))?;
    let binary = work.join(name);
    let mut compile = Command::new(cc());
    compile.args(["-std=c11", "-ffp-contract=off", "-g"]);
    match release {
        true => compile.args(["-O2", "-fsanitize=undefined", "-fno-sanitize-recover=all"]),
        false => compile.args(["-O1", "-fsanitize=address,undefined", "-fno-sanitize-recover=all"]),
    };
    if cfg!(target_os = "linux") {
        compile.arg("-fstack-clash-protection");
    }
    compile.arg("-o").arg(&binary).arg(&c_file).args(["-lm", "-pthread"]);
    let compiled = run_limited(&mut compile);
    if compiled.code != Some(0) {
        return Err(("cc", shown(&compiled)));
    }
    Ok(binary)
}

fn run_binary(binary: &Path, work: &Path) -> Ran {
    run_limited(Command::new(binary).current_dir(work).env("ASAN_OPTIONS", "detect_leaks=0"))
}

/// Runs one program in `work`, both ways, and compares.
fn try_program(program: &str, work: &Path) -> Outcome {
    let _ = std::fs::remove_dir_all(work);
    if let Err(error) = std::fs::create_dir_all(work).and_then(|_| std::fs::write(work.join("prog.sg"), program)) {
        return Outcome::Found { kind: "tool", sides: vec![("harness", format!("cannot write the program: {error}"))] };
    }
    let debug = match build(work, false) {
        Ok(binary) => binary,
        Err(("rejected", message)) => return Outcome::Rejected(message),
        Err((kind, message)) => return Outcome::Found { kind, sides: vec![("build", message)] },
    };
    let interpreted = run_limited(Command::new(sg()).current_dir(work).arg("prog.sg"));
    if tool_fault(&interpreted) {
        return Outcome::Found { kind: if interpreted.timed_out { "timeout" } else { "tool" }, sides: vec![("interpreter", shown(&interpreted))] };
    }
    let compiled = run_binary(&debug, work);
    let (interpreted_text, compiled_text) = (shown(&interpreted), shown(&compiled));
    if sanitizer_report(&compiled.text) {
        return Outcome::Found { kind: "sanitizer", sides: vec![("interpreter", interpreted_text), ("debug", compiled_text)] };
    }
    if compiled.timed_out {
        return Outcome::Found { kind: "timeout", sides: vec![("interpreter", interpreted_text), ("debug", compiled_text)] };
    }
    if interpreted_text != compiled_text {
        return Outcome::Found { kind: "differs", sides: vec![("interpreter", interpreted_text), ("debug", compiled_text)] };
    }
    if compiled.code != Some(0) {
        return Outcome::Same { panicked: true };
    }
    let release = match build(work, true) {
        Ok(binary) => binary,
        Err((kind, message)) => return Outcome::Found { kind: if kind == "rejected" { "release-rejected" } else { kind }, sides: vec![("release build", message)] },
    };
    let fast = run_binary(&release, work);
    let fast_text = shown(&fast);
    if sanitizer_report(&fast.text) {
        return Outcome::Found { kind: "sanitizer-release", sides: vec![("debug", compiled_text), ("release", fast_text)] };
    }
    if fast_text != compiled_text {
        return Outcome::Found { kind: "release-differs", sides: vec![("debug", compiled_text), ("release", fast_text)] };
    }
    Outcome::Same { panicked: false }
}

#[derive(Default)]
struct Counts {
    programs: u64,
    rejected: u64,
    panicked: u64,
    found: Vec<(String, u64)>,
}

/// Runs seeds from `first` on, `jobs` at once, for `time`. What is found goes to
/// `out/found/<kind>-<seed>/`, the first programs the checker refused to `out/rejected/`.
fn run(out: &Path, time: Duration, jobs: usize, first: u64) {
    // The C compiler and its sanitizers are tried first: without them every program would be a
    // finding of kind `cc`.
    if let Err(message) = preflight(&out.join("preflight")) {
        eprintln!("the C compiler cannot build with the sanitizers here:\n{message}");
        std::process::exit(1);
    }
    let next = Arc::new(AtomicU64::new(first));
    let counts = Arc::new(Mutex::new(Counts::default()));
    let start = Instant::now();
    println!("seeds from {first}, {jobs} at once, {} seconds", time.as_secs());
    let mut workers = Vec::new();
    for job in 0..jobs {
        let (next, counts, out) = (next.clone(), counts.clone(), out.to_path_buf());
        workers.push(std::thread::spawn(move || {
            let work = out.join("work").join(job.to_string());
            while start.elapsed() < time {
                let seed = next.fetch_add(1, Ordering::Relaxed);
                let program = render(&Gen::new(seed).program());
                let outcome = try_program(&program, &work);
                let mut counts = counts.lock().unwrap();
                counts.programs += 1;
                match &outcome {
                    Outcome::Same { panicked } => counts.panicked += *panicked as u64,
                    Outcome::Rejected(message) => {
                        counts.rejected += 1;
                        if counts.rejected <= 200 {
                            let dir = out.join("rejected");
                            let _ = std::fs::create_dir_all(&dir);
                            let _ = std::fs::write(dir.join(format!("{seed}.sg")), &program);
                            let _ = std::fs::write(dir.join(format!("{seed}.txt")), message);
                        }
                    }
                    Outcome::Found { kind, .. } => {
                        match counts.found.iter_mut().find(|(seen, _)| seen == kind) {
                            Some((_, count)) => *count += 1,
                            None => counts.found.push((kind.to_string(), 1)),
                        }
                        let dir = out.join("found").join(format!("{kind}-{seed}"));
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::fs::write(dir.join("prog.sg"), &program);
                        let _ = std::fs::write(dir.join("outcome.txt"), outcome.describe());
                        println!("found {kind} at seed {seed}");
                    }
                }
            }
        }));
    }
    let mut last = Instant::now();
    while workers.iter().any(|worker| !worker.is_finished()) {
        std::thread::sleep(Duration::from_millis(200));
        if last.elapsed() >= Duration::from_secs(60) {
            last = Instant::now();
            report(&counts.lock().unwrap(), start.elapsed());
        }
    }
    for worker in workers {
        let _ = worker.join();
    }
    report(&counts.lock().unwrap(), start.elapsed());
    let _ = std::fs::remove_dir_all(out.join("work"));
}

fn preflight(work: &Path) -> Result<(), String> {
    std::fs::create_dir_all(work).map_err(|error| error.to_string())?;
    std::fs::write(work.join("t.c"), "int main(void) { return 0; }\n").map_err(|error| error.to_string())?;
    for flags in [&["-O1", "-fsanitize=address,undefined"][..], &["-O2", "-fsanitize=undefined"][..]] {
        let ran = run_limited(Command::new(cc()).args(flags).arg("-o").arg(work.join("t")).arg(work.join("t.c")));
        if ran.code != Some(0) {
            return Err(shown(&ran));
        }
    }
    let _ = std::fs::remove_dir_all(work);
    Ok(())
}

fn report(counts: &Counts, elapsed: Duration) {
    let found: Vec<String> = counts.found.iter().map(|(kind, count)| format!("{kind} {count}")).collect();
    println!(
        "{}s: {} programs ({:.1}/s), {} refused by the checker, {} ended in a panic; found: {}",
        elapsed.as_secs(),
        counts.programs,
        counts.programs as f64 / elapsed.as_secs_f64().max(1.0),
        counts.rejected,
        counts.panicked,
        if found.is_empty() { "nothing".to_string() } else { found.join(", ") }
    );
}

/// Every list of statements in the tree, each as the way to it from the top: the node and the
/// part at each level. The empty way is the top.
fn lists(nodes: &[Node]) -> Vec<Vec<(usize, usize)>> {
    fn walk(nodes: &[Node], way: &mut Vec<(usize, usize)>, all: &mut Vec<Vec<(usize, usize)>>) {
        all.push(way.clone());
        for (at, node) in nodes.iter().enumerate() {
            for (part, (_, body)) in node.parts.iter().enumerate() {
                way.push((at, part));
                walk(body, way, all);
                way.pop();
            }
        }
    }
    let mut all = Vec::new();
    walk(nodes, &mut Vec::new(), &mut all);
    all
}

fn list_at<'a>(nodes: &'a mut Vec<Node>, way: &[(usize, usize)]) -> Option<&'a mut Vec<Node>> {
    let mut list = nodes;
    for &(at, part) in way {
        list = &mut list.get_mut(at)?.parts.get_mut(part)?.1;
    }
    Some(list)
}

/// The smallest program the generator's tree gives that the checker takes and that shows the
/// same kind of finding (choice 1366): runs of statements are taken away, halves first, then
/// smaller runs, as delta debugging does, and then each block is replaced by what it holds,
/// until nothing more can go.
fn reduce(seed: u64, out: &Path) {
    let work = std::env::temp_dir().join(format!("sg-difference-reduce-{}", std::process::id()));
    let mut nodes = Gen::new(seed).program();
    let Some(kind) = try_program(&render(&nodes), &work).kind() else {
        eprintln!("seed {seed} shows nothing to reduce");
        std::process::exit(1);
    };
    println!("reducing a finding of kind {kind}, {} lines", render(&nodes).lines().count());
    let mut tries = 0;
    let mut still = |candidate: &[Node]| {
        tries += 1;
        try_program(&render(candidate), &work).kind() == Some(kind)
    };
    loop {
        let mut smaller = false;
        let mut at = 0;
        while let Some(way) = lists(&nodes).get(at).cloned() {
            let len = list_at(&mut nodes, &way).map_or(0, |list| list.len());
            let mut size = len.div_ceil(2).max(1);
            while size >= 1 && len > 0 {
                let mut start = 0;
                while start < list_at(&mut nodes, &way).map_or(0, |list| list.len()) {
                    let mut candidate = nodes.clone();
                    let list = list_at(&mut candidate, &way).unwrap();
                    let end = (start + size).min(list.len());
                    list.drain(start..end);
                    if still(&candidate) {
                        nodes = candidate;
                        smaller = true;
                    } else {
                        start += size;
                    }
                }
                if size == 1 {
                    break;
                }
                size = size.div_ceil(2);
            }
            at += 1;
        }
        // A block replaced by the statements inside it.
        let mut at = 0;
        while let Some(way) = lists(&nodes).get(at).cloned() {
            let mut item = 0;
            while item < list_at(&mut nodes, &way).map_or(0, |list| list.len()) {
                let mut candidate = nodes.clone();
                let list = list_at(&mut candidate, &way).unwrap();
                let node = list.remove(item);
                let inner: Vec<Node> = node.parts.into_iter().flat_map(|(_, body)| body).collect();
                let count = inner.len();
                if count > 0 {
                    list.splice(item..item, inner);
                    if still(&candidate) {
                        nodes = candidate;
                        smaller = true;
                        continue;
                    }
                }
                item += 1;
            }
            at += 1;
        }
        if !smaller {
            break;
        }
    }
    let text = render(&nodes);
    println!("{} lines after {tries} tries", text.lines().count());
    std::fs::write(out, &text).unwrap();
    let _ = std::fs::remove_dir_all(&work);
}
