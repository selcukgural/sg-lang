//! `random`: `std`'s random numbers measured against `rand` (choices 1553, 1629-1637, 1667, 1669).
//! The generator is `rand_xoshiro`'s `Xoshiro256StarStar`, from `seed_from_u64`, which is SG's:
//! xoshiro256** with its state set by SplitMix64, so the numbers of 64 bits are the same. Four ways
//! of using them:
//!
//!     random numbers COUNT    COUNT numbers of 64 bits from the seed 1
//!     random range COUNT      COUNT numbers from 0 up to 1000, without 1000, counted in a histogram
//!     random fill MEGABYTES   MEGABYTES megabytes of random bytes, 64 KB at a time into one buffer
//!     random tokens COUNT     COUNT tokens of 64 hex digits (32 bytes) from the random bytes of the
//!                             system (`SysRng`, `getrandom`)
//!
//! The generators are not the same in each language, so nothing printed depends on the numbers
//! drawn, only on whether they look random: how many were drawn, and whether the bits set
//! (`numbers`), each bar of the histogram (`range`) and the mean of every 64th byte (`fill`) lie
//! within six standard deviations of what they should be; `tokens` prints how many characters the
//! tokens had and whether all were hex digits. A wrong use is told on the standard error, with exit
//! code 2.

use rand::{Rng, RngExt, SeedableRng, TryRng};
use rand::rngs::SysRng;
use rand_xoshiro::Xoshiro256StarStar;
use std::process::exit;

const USAGE: &str = "usage: random numbers COUNT | range COUNT | fill MEGABYTES | tokens COUNT";

fn numbers(count: i64) {
    let mut random = Xoshiro256StarStar::seed_from_u64(1);
    let mut ones = 0i64;
    for _ in 0..count {
        ones += random.next_u64().count_ones() as i64;
    }
    let off = ones - 32 * count;
    println!("numbers {count}\nbits near half {}", off * off <= 36 * 16 * count);
}

fn range(count: i64) {
    let mut random = Xoshiro256StarStar::seed_from_u64(1);
    let mut bars = [0i64; 1000];
    for _ in 0..count {
        bars[random.random_range(0..1000u32) as usize] += 1;
    }
    let expected = count / 1000;
    let near = bars.iter().all(|&bar| (bar - expected) * (bar - expected) <= 36 * expected + 36);
    println!("numbers {count}\nbars 1000\nevery bar near {near}");
}

fn fill(megabytes: i64) {
    let mut random = Xoshiro256StarStar::seed_from_u64(1);
    let mut buffer = vec![0u8; 65536];
    let rounds = megabytes * 16;
    let (mut sum, mut seen) = (0i64, 0i64);
    for _ in 0..rounds {
        random.fill_bytes(&mut buffer);
        sum += buffer.iter().step_by(64).map(|&b| b as i64).sum::<i64>();
        seen += 1024;
    }
    let off = 2 * sum - 255 * seen;
    println!("bytes {}\nmean near {}", rounds * 65536, off * off <= 786420 * seen);
}

fn tokens(count: i64) {
    let mut bytes = [0u8; 32];
    let mut text = [0u8; 64];
    let (mut characters, mut hex) = (0i64, true);
    for _ in 0..count {
        SysRng.try_fill_bytes(&mut bytes).expect("the random bytes of the system");
        hex::encode_to_slice(bytes, &mut text).unwrap();
        characters += text.len() as i64;
        hex &= text.iter().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c));
    }
    println!("tokens {count}\ncharacters {characters}\nhex {hex}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let known = args.len() == 2 && matches!(args[0].as_str(), "numbers" | "range" | "fill" | "tokens");
    let count = if known { args[1].parse::<i64>().unwrap_or(-1) } else { -1 };
    if count < 0 {
        eprintln!("{USAGE}");
        exit(2);
    }
    match args[0].as_str() {
        "numbers" => numbers(count),
        "range" => range(count),
        "fill" => fill(count),
        _ => tokens(count),
    }
}
