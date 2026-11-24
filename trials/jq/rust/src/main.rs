//! `jq`: a part of jq 1.8.2's language run on a stream of JSON, giving the same bytes on the standard
//! output and the same exit code as jq 1.8.2 (choices 1804-1812; the cases and what is in the part
//! are in `trials/jq/README.md`).
//!
//!     jq [-c] [-r] [-n] [-s] [-e] [-S] [--tab] [--indent n] [--arg name text] [--argjson name json]
//!        FILTER [FILE...]
//!
//! The filter is read into a tree and walked by a machine with a stack of its own (`machine.rs`); the
//! input is read in pieces of 64 KB and each value by `serde_json` (`input.rs`); the output goes
//! through a block of 64 KB (`output.rs`).

mod builtins;
mod compile;
mod input;
mod json;
mod machine;
mod number;
mod output;
mod parser;
mod single;
mod value;

use compile::{DefIr, Ir, compile};
use input::{Next, Reader, parse_text};
use machine::{Inputs, Machine};
use output::{Format, Output, Style, to_json};
use std::io::IsTerminal;
use value::Value;

/// jq's results of one input (`process` in its `main.c`), and its exit codes.
const OK: i32 = 0;
const OK_NULL_KIND: i32 = -1;
const OK_NO_OUTPUT: i32 = -4;
const ERROR_SYSTEM: i32 = 2;
const ERROR_COMPILE: i32 = 3;
const ERROR_UNKNOWN: i32 = 5;

struct Options {
    format: Format,
    raw: bool,
    null_input: bool,
    slurp: bool,
    exit_status: bool,
}

fn usage(message: &str) -> ! {
    eprintln!("jq: {message}");
    eprintln!(
        "usage: jq [-c] [-r] [-n] [-s] [-e] [-S] [--tab] [--indent n] [--arg name text] [--argjson name json] FILTER [FILE...]"
    );
    std::process::exit(ERROR_SYSTEM);
}

/// What `input` and `inputs` read: the stream, or once the whole of it as an array (`-n -s`).
struct Source<'a> {
    reader: &'a mut Reader,
    slurp: bool,
}

impl Inputs for Source<'_> {
    fn next_input(&mut self) -> Option<Result<Value, String>> {
        if self.slurp {
            self.slurp = false;
            return Some(slurp(self.reader));
        }
        match self.reader.next() {
            Next::Value(v) => Some(Ok(v)),
            Next::End => None,
            Next::Error(message) => Some(Err(message)),
        }
    }
}

/// Every value of the stream in one array.
fn slurp(reader: &mut Reader) -> Result<Value, String> {
    let mut items = Vec::new();
    loop {
        match reader.next() {
            Next::Value(v) => items.push(v),
            Next::End => return Ok(Value::array(items)),
            Next::Error(message) => return Err(message),
        }
    }
}

fn report(error: &Value) {
    match error {
        Value::String(message) => eprintln!("jq: error (at <stdin>): {message}"),
        other => eprintln!("jq: error (at <stdin>) (not a string): {}", to_json(other)),
    }
}

/// Runs the filter on one input; jq's result of it.
fn process(
    machine: &mut Machine,
    program: &'static Ir,
    env: &machine::Env,
    input: Value,
    source: &mut Source,
    out: &mut Output,
    options: &Options,
) -> i32 {
    let mut status = OK_NO_OUTPUT;
    let mut emit = |v: Value| {
        status = if options.exit_status && matches!(v, Value::Null | Value::False) { OK_NULL_KIND } else { OK };
        out.value(&v, options.format, options.raw);
    };
    match machine.run(program, input, env, source, &mut emit) {
        Ok(()) => status,
        Err(e) => {
            report(&e);
            ERROR_UNKNOWN
        }
    }
}

/// The stack of the thread the program runs on. Reading, writing and comparing a value recurse at
/// most 10,000 levels (jq's limits), which the main thread's 8 MB hold; but a value the filter builds
/// may be nested far deeper (`reduce range(1000000) as $i (0; [.])`, which jq runs), and dropping it
/// recurses as deep as it is. Only the pages used are taken.
const STACK: usize = 512 * 1024 * 1024;

fn main() {
    let program = std::thread::Builder::new().stack_size(STACK).spawn(run).expect("a thread to run on");
    let code = program.join().unwrap_or(ERROR_UNKNOWN);
    std::process::exit(code);
}

/// Whether an argument is an option, as jq tells: `-` and a letter, or `--`.
fn is_option(arg: &str) -> bool {
    let bytes = arg.as_bytes();
    bytes.len() > 1 && bytes[0] == b'-' && (bytes[1] == b'-' || bytes[1].is_ascii_alphabetic() || bytes[1] >= 0x80)
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut options = Options {
        format: Format { style: Style::Spaces(2), sort_keys: false },
        raw: false,
        null_input: false,
        slurp: false,
        exit_status: false,
    };
    let mut filter: Option<String> = None;
    let mut files = Vec::new();
    let mut names = Vec::new();
    let mut values = Vec::new();
    let mut at = 0;
    let mut options_done = false;
    while at < args.len() {
        let arg = &args[at];
        at += 1;
        if options_done || !is_option(arg) {
            if filter.is_none() {
                filter = Some(arg.clone());
            } else {
                files.push(arg.clone());
            }
            continue;
        }
        let value_of = |at: &mut usize| -> String {
            let v = args.get(*at).cloned().unwrap_or_else(|| usage(&format!("{arg} takes a parameter")));
            *at += 1;
            v
        };
        match arg.as_str() {
            "--" => options_done = true,
            "--compact-output" => options.format.style = Style::Compact,
            "--raw-output" => options.raw = true,
            "--null-input" => options.null_input = true,
            "--slurp" => options.slurp = true,
            "--exit-status" => options.exit_status = true,
            "--sort-keys" => options.format.sort_keys = true,
            "--tab" => options.format.style = Style::Tab,
            "--indent" => {
                let n = value_of(&mut at);
                options.format.style = match n.parse::<i32>() {
                    Ok(-1) => Style::Tab,
                    Ok(n @ 0..=7) => Style::Spaces(n as usize),
                    _ => usage("--indent takes a number between -1 and 7"),
                };
            }
            "--arg" | "--argjson" => {
                let name = value_of(&mut at);
                let text = value_of(&mut at);
                let value = if arg == "--arg" {
                    Value::string(text)
                } else {
                    parse_text(&text).unwrap_or_else(|_| usage(&format!("Invalid JSON text passed to --argjson: {text}")))
                };
                names.push(name);
                values.push(value);
            }
            a if !a.starts_with("--") => {
                for c in a[1..].chars() {
                    match c {
                        'c' => options.format.style = Style::Compact,
                        'r' => options.raw = true,
                        'n' => options.null_input = true,
                        's' => options.slurp = true,
                        'e' => options.exit_status = true,
                        'S' => options.format.sort_keys = true,
                        _ => usage(&format!("Unknown option: {a}")),
                    }
                }
            }
            a => usage(&format!("Unknown option: {a}")),
        }
    }
    // With no filter, jq runs `.` unless both its input and its output are a terminal.
    let filter = filter.unwrap_or_else(|| {
        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            usage("no filter given");
        }
        ".".to_string()
    });
    let (program, defs) = match compile(&filter, &names) {
        Ok(compiled) => compiled,
        Err(message) => {
            eprintln!("jq: error: {message}\njq: 1 compile error");
            std::process::exit(ERROR_COMPILE);
        }
    };
    // The tree lives as long as the program: its nodes are referred to by `&'static`.
    let program: &'static Ir = Box::leak(Box::new(program));
    let defs: &'static [DefIr] = Box::leak(defs.into_boxed_slice());

    let mut reader = Reader::new(files);
    let mut out = Output::new();
    let env = Machine::env(values);
    let mut machine = Machine::new(defs);
    let mut ret = OK_NO_OUTPUT;
    let mut last_result = -1;
    let record = |ret: i32, last_result: &mut i32| {
        if ret <= 0 && ret != OK_NO_OUTPUT {
            *last_result = (ret != OK_NULL_KIND) as i32;
        }
    };
    if options.null_input {
        let mut source = Source { reader: &mut reader, slurp: options.slurp };
        ret = process(&mut machine, program, &env, Value::Null, &mut source, &mut out, &options);
        record(ret, &mut last_result);
    } else if options.slurp {
        match slurp(&mut reader) {
            Ok(all) => {
                let mut source = Source { reader: &mut reader, slurp: false };
                ret = process(&mut machine, program, &env, all, &mut source, &mut out, &options);
                record(ret, &mut last_result);
            }
            Err(message) => {
                eprintln!("jq: error (at <stdin>): {message}");
                ret = ERROR_UNKNOWN;
            }
        }
    } else {
        loop {
            match reader.next() {
                Next::Value(value) => {
                    let mut source = Source { reader: &mut reader, slurp: false };
                    ret = process(&mut machine, program, &env, value, &mut source, &mut out, &options);
                    record(ret, &mut last_result);
                    // Once a file could not be opened or read, jq's loop stops after the value it was reading.
                    if reader.file_error {
                        break;
                    }
                }
                Next::End => break,
                Next::Error(message) => {
                    eprintln!("jq: error (at <stdin>): {message}");
                    ret = ERROR_UNKNOWN;
                    break;
                }
            }
        }
    }
    if reader.file_error {
        ret = ERROR_SYSTEM;
    }
    out.flush();
    if options.exit_status {
        if ret != OK_NO_OUTPUT {
            ret.abs()
        } else {
            match last_result {
                -1 => OK_NO_OUTPUT.abs(),
                0 => OK_NULL_KIND.abs(),
                _ => OK,
            }
        }
    } else {
        ret.max(0)
    }
}
