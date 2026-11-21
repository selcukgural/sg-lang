//! `time`: `std`'s calendar and clocks measured against `chrono` and the standard library's
//! `Instant` (choices 1553, 1629-1637, 1667, 1668). Four ways of using them:
//!
//!     time format COUNT    COUNT moments between 1900 and 2200, each written in ISO 8601 into a
//!                          text kept from one to the next; a quarter of them whole seconds, the
//!                          others with milliseconds, microseconds or nanoseconds
//!     time parse FILE      each line of FILE, a moment in RFC 3339 with an offset, read and taken
//!                          to UTC
//!     time local COUNT     COUNT moments between 1900 and 2200, each seen in the time zone of the
//!                          system (`TZ`)
//!     time clock COUNT     the monotonic clock read COUNT times
//!
//! The moments are made by SplitMix64 from the seed 1. `format` prints how many it wrote, how many
//! characters they had and a sum of one character of each; `parse` how many lines, the sum of their
//! seconds since 1970, of their nanoseconds and of their offsets in seconds; `local` the sum of the
//! offsets and of the year, month, day, hour and minute seen; `clock` how many times it read the
//! clock and whether it never went back. A file that cannot be read is told on the standard error,
//! with exit code 1, as is a line that is not a moment; a wrong use, with 2.
//!
//! `format` writes with `to_rfc3339_opts(SecondsFormat::AutoSi, true)`, the way `chrono` offers
//! for this form, whose fraction has 3, 6 or 9 digits as SG's; it is four times as fast as
//! `write!` with `format("%Y-%m-%dT%H:%M:%S%.fZ")` into the kept text.

use chrono::{DateTime, Datelike, Local, SecondsFormat, Timelike, Utc};
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::process::exit;
use std::time::Instant;

const USAGE: &str = "usage: time format COUNT | parse FILE | local COUNT | clock COUNT";

struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
}

/// 1900-01-01 and 2200-01-01, in seconds since 1970.
const LOW: i64 = -2208988800;
const SPAN: u64 = 9467107200;

/// A moment between 1900 and 2200: whole seconds, or a fraction of milliseconds, microseconds or
/// nanoseconds, each as likely.
fn moment(random: &mut SplitMix) -> DateTime<Utc> {
    let seconds = LOW + (random.next() % SPAN) as i64;
    let nanoseconds = match random.next() % 4 {
        1 => (random.next() % 1000) as u32 * 1_000_000,
        2 => (random.next() % 1_000_000) as u32 * 1000,
        3 => (random.next() % 1_000_000_000) as u32,
        _ => 0,
    };
    DateTime::from_timestamp(seconds, nanoseconds).unwrap()
}

fn format(count: u64) {
    let mut random = SplitMix(1);
    let mut text = String::with_capacity(64);
    let (mut characters, mut sum) = (0u64, 0u64);
    for i in 0..count {
        let t = moment(&mut random);
        text.clear();
        text.push_str(&t.to_rfc3339_opts(SecondsFormat::AutoSi, true));
        characters += text.len() as u64;
        sum += text.as_bytes()[(i % text.len() as u64) as usize] as u64;
    }
    println!("moments {count}\ncharacters {characters}\nsum {sum}");
}

fn parse(path: &str) -> io::Result<()> {
    let mut reader = BufReader::with_capacity(65536, File::open(path)?);
    let mut line = String::new();
    let (mut count, mut seconds, mut nanoseconds, mut offsets) = (0i64, 0i64, 0i64, 0i64);
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        count += 1;
        let text = line.strip_suffix('\n').unwrap_or(&line);
        let text = text.strip_suffix('\r').unwrap_or(text);
        match DateTime::parse_from_rfc3339(text) {
            Ok(seen) => {
                seconds += seen.timestamp();
                nanoseconds += seen.timestamp_subsec_nanos() as i64;
                offsets += seen.offset().local_minus_utc() as i64;
            }
            Err(e) => {
                eprintln!("time: {path}: line {count}: not a moment: {e}");
                exit(1);
            }
        }
    }
    println!("lines {count}\nseconds {seconds}\nnanoseconds {nanoseconds}\noffsets {offsets}");
    Ok(())
}

fn local(count: u64) {
    let mut random = SplitMix(1);
    let (mut offsets, mut parts) = (0i64, 0i64);
    for _ in 0..count {
        let seen = moment(&mut random).with_timezone(&Local);
        offsets += seen.offset().local_minus_utc() as i64;
        parts += (seen.year() + seen.month() as i32 + seen.day() as i32 + seen.hour() as i32 + seen.minute() as i32) as i64;
    }
    println!("moments {count}\noffsets {offsets}\nparts {parts}");
}

fn clock(count: u64) {
    let mut last = Instant::now();
    let mut forward = true;
    for _ in 0..count {
        let now = Instant::now();
        if now < last {
            forward = false;
        }
        last = now;
    }
    println!("reads {count}\nforward {forward}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let file = args.len() == 2 && args[0] == "parse";
    let counts = args.len() == 2 && matches!(args[0].as_str(), "format" | "local" | "clock");
    if !file && !counts {
        eprintln!("{USAGE}");
        exit(2);
    }
    if file {
        if let Err(e) = parse(&args[1]) {
            eprintln!("time: {}: {e}", args[1]);
            exit(1);
        }
        return;
    }
    let Ok(count) = args[1].parse::<u64>() else {
        eprintln!("{USAGE}");
        exit(2);
    };
    match args[0].as_str() {
        "format" => format(count),
        "local" => local(count),
        _ => clock(count),
    }
}
