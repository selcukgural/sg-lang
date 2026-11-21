//! `log`: `std`'s log measured against `log` and `env_logger` (choices 1553, 1629-1637, 1667,
//! 1670). Three ways of using it:
//!
//!     log lines COUNT    COUNT lines at the level Info, `request 12 took 84 ms`
//!     log off COUNT      COUNT lines at the level Debug while the log writes from Info: none is
//!                        written, and the text of none is made
//!     log tasks COUNT    four threads at once, each writing a quarter of COUNT lines at the level
//!                        Info, `task 2 request 12 took 84 ms`
//!
//! The lines go to the standard error, each `2026-11-21T20:15:03.123Z INFO  ` and the message, one
//! whole line from one call: `env_logger` is given the format of SG's lines, its time to the
//! millisecond, and no colour. Each prints on the standard output how many lines it wrote (`off`,
//! how many it did not, and the sum of the numbers it would have written). A wrong use is told on
//! the standard error, with exit code 2.

use log::{LevelFilter, debug, info};
use std::io::Write;
use std::process::exit;
use std::thread;

const USAGE: &str = "usage: log lines COUNT | off COUNT | tasks COUNT";

fn lines(count: i64) {
    for i in 0..count {
        info!("request {i} took {} ms", i * 7 % 1000);
    }
    println!("lines {count}");
}

fn off(count: i64) {
    let mut sum = 0i64;
    for i in 0..count {
        debug!("request {i} took {} ms", i * 7 % 1000);
        sum += i * 7 % 1000;
    }
    println!("lines 0 of {count}\nsum {sum}");
}

fn work(task: i64, count: i64) -> i64 {
    for i in 0..count {
        info!("task {task} request {i} took {} ms", i * 7 % 1000);
    }
    count
}

fn tasks(count: i64) {
    let shares = [count / 4, count / 4, count / 4, count - count / 4 * 3];
    let handles: Vec<_> = shares.iter().enumerate().map(|(task, &share)| thread::spawn(move || work(task as i64, share))).collect();
    let written: i64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
    println!("lines {written}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let known = args.len() == 2 && matches!(args[0].as_str(), "lines" | "off" | "tasks");
    let count = if known { args[1].parse::<i64>().unwrap_or(-1) } else { -1 };
    if count < 0 {
        eprintln!("{USAGE}");
        exit(2);
    }
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .write_style(env_logger::WriteStyle::Never)
        .format(|buf, record| writeln!(buf, "{} {:<5} {}", buf.timestamp_millis(), record.level(), record.args()))
        .init();
    match args[0].as_str() {
        "lines" => lines(count),
        "off" => off(count),
        _ => tasks(count),
    }
}
