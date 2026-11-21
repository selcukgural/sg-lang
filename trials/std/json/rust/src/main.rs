//! `json`: `std`'s JSON measured against `serde_json` (choices 1553, 1629-1637). Three ways of
//! using it:
//!
//!     json sum FILE      reads the document and walks it: what it holds, counted and added up
//!     json copy FILE     reads the document and writes it back on one line
//!     json lines FILE    reads a document on each line, and walks each, as `sum` does
//!
//! What `sum` and `lines` print: how many values of each kind there are, the bytes of the texts
//! and of the names of members, and the sums of the whole numbers and of the others, added in the
//! order they come. A file that cannot be read or is not JSON is told on the standard error, with
//! exit code 1; a wrong use, with 2.

use serde_json::Value;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::process::exit;

#[derive(Default)]
struct Counts {
    values: i64,
    nulls: i64,
    bools: i64,
    integers: i64,
    numbers: i64,
    strings: i64,
    arrays: i64,
    objects: i64,
    string_bytes: i64,
    name_bytes: i64,
    integer_sum: i64,
    number_sum: f64,
}

fn walk(value: &Value, c: &mut Counts) {
    c.values += 1;
    match value {
        Value::Null => c.nulls += 1,
        Value::Bool(_) => c.bools += 1,
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                c.integers += 1;
                c.integer_sum += i;
            } else {
                c.numbers += 1;
                c.number_sum += n.as_f64().unwrap();
            }
        }
        Value::String(text) => {
            c.strings += 1;
            c.string_bytes += text.len() as i64;
        }
        Value::Array(items) => {
            c.arrays += 1;
            for item in items {
                walk(item, c);
            }
        }
        Value::Object(members) => {
            c.objects += 1;
            for (name, member) in members {
                c.name_bytes += name.len() as i64;
                walk(member, c);
            }
        }
    }
}

fn show(c: &Counts, out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "values {}", c.values)?;
    writeln!(out, "null {}", c.nulls)?;
    writeln!(out, "bool {}", c.bools)?;
    writeln!(out, "integer {}", c.integers)?;
    writeln!(out, "number {}", c.numbers)?;
    writeln!(out, "string {}", c.strings)?;
    writeln!(out, "array {}", c.arrays)?;
    writeln!(out, "object {}", c.objects)?;
    writeln!(out, "string bytes {}", c.string_bytes)?;
    writeln!(out, "name bytes {}", c.name_bytes)?;
    writeln!(out, "integer sum {}", c.integer_sum)?;
    writeln!(out, "number sum {}", c.number_sum)
}

fn fail(message: String) -> ! {
    eprintln!("json: {message}");
    exit(1)
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 || !["sum", "copy", "lines"].contains(&args[0].as_str()) {
        eprintln!("usage: json sum|copy|lines FILE");
        exit(2);
    }
    let name = &args[1];
    let stdout = io::stdout();
    let mut out = io::BufWriter::with_capacity(65536, stdout.lock());
    let mut c = Counts::default();
    if args[0] == "lines" {
        let file = File::open(name).unwrap_or_else(|e| fail(format!("{name}: {e}")));
        let mut reader = BufReader::with_capacity(65536, file);
        let mut line = String::new();
        let mut number = 0u64;
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) => fail(format!("{name}: {e}")),
            }
            number += 1;
            match serde_json::from_str::<Value>(&line) {
                Ok(value) => walk(&value, &mut c),
                Err(e) => fail(format!("{name}, line {number}: {e}")),
            }
        }
        writeln!(out, "documents {number}")?;
        return show(&c, &mut out);
    }
    let text = std::fs::read_to_string(name).unwrap_or_else(|e| fail(format!("{name}: {e}")));
    let value: Value = serde_json::from_str(&text).unwrap_or_else(|e| fail(format!("{name}: {e}")));
    if args[0] == "sum" {
        walk(&value, &mut c);
        show(&c, &mut out)
    } else {
        serde_json::to_writer(&mut out, &value)?;
        writeln!(out)
    }
}
