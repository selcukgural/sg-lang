# Trials

Programs that people write, written in SG, in Rust and in Zig, the same algorithm each time and written
plainly, then measured against each other (choices 1418, 1540). Rust and Zig are the languages in SG's
lane: compiled ahead of time, no collector, memory held and freed by the program; Go, measured in the first
trial, was left for them. Each difference found is a numbered choice in
the [interpreter's README](../interpreter/README.md), fixed and measured again.

| Trial | What it is | Slice |
|---|---|---|
| [wc](wc) | The lines, words and bytes of files, read in pieces of 64 KB | 133 |
| [grep](grep) | The lines a regular expression matches, with `-c`, `-i`, `-n` and `-v` | 133 |

The modules of `std` are measured the same way, each in `trials/std/<module>` (choices 1553, 1629-1637),
and their tables are in the [interpreter's README](../interpreter/README.md), "`std`, measured against Rust
and Zig":

| Module | What it is | Slice |
|---|---|---|
| [std/json](std/json) | A document read and walked, read and written back, and a document a line read | 149 |
| [std/compression](std/compression) | 100 MB of a log compressed in gzip and opened, 99,640 records each compressed and opened in zlib, 100 MB that do not compress | 150 |
| [std/zip](std/zip) | 100 MB written as 10,001 entries, the entries of an archive read, 100,000 entries listed | 150 |
| [std/hash](std/hash) | The SHA-256 of 100 MB, ten million hashes chained, an HMAC-SHA-256 of each of 921,952 lines | 151 |
| [std/checksum](std/checksum) | The CRC-32 and the Adler-32 of 100 MB, the CRC-32 of each of 921,952 lines | 151 |

Still to come, in this order: a `jq` of JSON, a key-value store with a write-ahead log, a pool of workers,
an HTTP/1.1 file server under load, a GIF tool, and an interpreter of Lox.

## How they are written (1525-1532)

- Each trial has a folder with `sg/`, `rust/` and `zig/`. Rust uses its standard library and the crates
  people use for the job (`regex`), pinned in `Cargo.lock`, and threads rather than an async runtime. Zig
  uses its standard library, and a library of C where Zig has none, as Zig programs do: `grep` links
  PCRE2 (with its JIT) through `build.zig` (1541). SG uses only what comes with the language.
- The three give the same output, byte for byte, on the same input; their errors are worded each in its
  own language's way. `tests/cli.rs` (`the_trials`) runs the SG versions interpreted and compiled on a
  megabyte and holds them to what Rust and Zig print.
- Nothing large is kept here: [`inputs/text.sg`](inputs/text.sg) makes lines of a log from a seed, the
  same bytes on every machine, [`inputs/json.sg`](inputs/json.sg) the documents of `std/json`, and
  [`inputs/bytes.sg`](inputs/bytes.sg) the bytes that do not compress of `std/compression`. The gzip and
  the archives that the programs read are made by Python's `gzip` and `zipfile`, which none of the
  programs measured is.
- Where each language may lawfully write other bytes (compressed data, an archive), the program writes
  into a file, and `measure.py` opens each one with Python and holds it to the input before anything is
  timed; the size each wrote is printed under its table.

## Fairness (1554)

No measurement is made, chosen or reported so that it favours SG. In particular:

- **The other languages are written as their own people write them.** Rust and Zig get the idiomatic
  program and the library people use for the job, in their fastest ordinary build (`--release`,
  `ReleaseFast`); nothing is slowed down, left unbuffered or written the long way. When a faster ordinary way
  is shown for Rust or Zig, it is taken and everything is measured again, also when SG loses by it.
- **The same work.** The same algorithm where one is named, the same input, the same output byte for byte,
  checked before anything is timed; a case where the programs do different work (a check one skips, a
  Unicode rule one ignores) is said beside the numbers or made the same.
- **Every case is published.** The cases are chosen before the numbers are seen, and none is dropped,
  changed or put out of sight because SG loses it; a case where SG loses stays in the table, and the
  difference goes into "Slower than C" in the interpreter's README with what would fix it.
- **SG as it ships.** SG is measured in its release build with every check of safety it has by default, and
  once more with its arithmetic checked. Nothing is turned off, and nothing is tuned for a measurement alone
  (1543): a change counts only where it serves every program.
- **Said plainly.** The machine, the compilers and their versions, the build flags, how time, memory and
  size are taken, and what is left out (a shared library Zig loads) are written beside every table, and a
  difference that comes from the tools rather than the language (another C compiler, another LLVM) is
  named as such.

## Measuring

From the repository's root, with Rust, Zig 0.17, PCRE2 and a C compiler installed:

```
python3 trials/measure.py               # every trial: checked, then measured
python3 trials/measure.py grep          # one trial
python3 trials/measure.py std/json      # one module of std
python3 trials/measure.py --check       # only that the outputs are the same
```

The time is the best of five runs after one to warm up, the memory the most the program held (its peak
resident set), the size that of the program stripped, and the lines those that are neither blank nor only
a comment. SG is built with `sg build --release` and once more with its arithmetic checked (the C of
`sg emit-c` at `-O2`); Rust with `cargo build --release`; Zig twice (1542), `ReleaseFast` and
`ReleaseSafe`, which keeps its checks of bounds and overflow. `std/compression` builds Rust once more with
`flate2`'s feature `zlib-rs`, which people turn on for speed (1632). No LTO and no profile. The size of Zig's
`grep` leaves out PCRE2, which it loads as a shared library (about 650 KB).

## Results

The modules of `std` were measured on the same machine on 21 November 2026, with macOS 27.2, Apple clang
21.0, rustc 1.99.1 and Zig 0.17.1; their tables are in the interpreter's README.

Apple M3 Max, macOS, 20 November 2026; 100 MB of text from `inputs/text.sg` (seed 1). SG is compiled by
the system's `cc` (Apple clang 21), Rust by its own LLVM (23.1), Zig 0.17.1 (LLVM too) with PCRE2 10.49.

### wc

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| 100 MB | 0.088 s, 1.9 MB | 0.089 s, 1.9 MB | 0.067 s, 2.0 MB | 0.098 s, 2.0 MB | 0.108 s, 2.2 MB |
| 100 MB from the input | 0.088 s, 1.9 MB | 0.089 s, 1.9 MB | 0.067 s, 2.0 MB | 0.099 s, 2.0 MB | 0.107 s, 2.2 MB |
| Size, stripped | 50 KB | 50 KB | 352 KB | 345 KB | 442 KB |
| Lines | 70 | 70 | 73 | 62 | 62 |

The difference with Rust is the C compiler, not SG: the same C built through Homebrew's clang 23.1
(`CC=... sg build`, which 1533 made possible on Linux and macOS) takes 0.043 s.

Measured again after 1543-1546 (the slice after the first), and after 1549-1550, which changed nothing of
`wc`'s work (it reads blocks, not lines): the numbers above are of those runs, the same within a
millisecond.

### grep

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| `-c ERROR` | 0.046 s, 2.0 MB | 0.046 s, 2.0 MB | 0.066 s, 2.8 MB | 0.060 s, 3.4 MB | 0.061 s, 3.6 MB |
| `-n timeout` | 0.052 s, 2.1 MB | 0.054 s, 2.1 MB | 0.072 s, 2.8 MB | 0.069 s, 3.5 MB | 0.070 s, 3.6 MB |
| `-c -i error` | 0.150 s, 2.0 MB | 0.159 s, 2.0 MB | 0.086 s, 3.0 MB | 0.069 s, 3.5 MB | 0.070 s, 3.6 MB |
| `-c '[a-z]+[0-9]+@example\.com (PUT\|DELETE)'` | 0.228 s, 2.1 MB | 0.236 s, 2.1 MB | 0.146 s, 3.2 MB | 0.129 s, 3.5 MB | 0.130 s, 3.6 MB |
| `'in [0-9]{4} ms: .*日本'` | 0.064 s, 2.2 MB | 0.064 s, 2.1 MB | 0.106 s, 3.2 MB | 0.072 s, 3.5 MB | 0.072 s, 3.6 MB |
| `-v -c INFO` | 0.049 s, 2.0 MB | 0.048 s, 2.0 MB | 0.068 s, 2.8 MB | 0.062 s, 3.4 MB | 0.063 s, 3.6 MB |
| Size, stripped | 164 KB | 164 KB | 1684 KB | 362 KB | 442 KB |
| Lines | 101 | 101 | 145 | 137 | 137 |

`-n timeout` prints 136,194 lines: it took 0.086 s, against Rust's and Zig's 0.072 s, until the slice that
followed this one. A line is now written into a block of 64 KB of the program's own, piece by piece where
it is printed, and no longer through C's `fwrite` and its lock with its format read at run time
(1544-1546): printing costs SG 4 ms over the search, as it costs Rust. The search was what was left
(`-c timeout` 0.072 s, Rust 0.065 s), until the slice after that: a plain text is now looked for by its two
rarest bytes, sixteen places at a time (1549), where `memchr` stopped at every `t`, and UTF-8 is checked by
going straight to the first byte past ASCII (1550). `-c timeout` takes 0.049 s (Rust 0.066 s, Zig 0.066 s,
the best of seven, not a case of the table), `-n timeout` 0.052 s and `-c ERROR` 0.046 s; reading the lines
is 0.033 s of them. `-i` and the address are still behind, for the reasons below.

Before the first slice of the trials, `Regex` ran Pike's machine on every line: `-i error` took 1.16 s, the address 2.06 s and
`日本` 0.83 s. An inner text every match holds (1535), a lazy DFA (1536) with its cache kept in the `Regex`
(1538), and the same for where a match is (1539) brought them to the numbers above. Go's `regexp` took 1.13 s and 1.33 s on the two hardest. What is left behind Rust and Zig:
`-i` looks for `error` in any case a byte at a time where Rust's `regex` uses the processor's vector
instructions, and the DFA's loop checks each read of its table against the table's end; PCRE2's JIT turns the
pattern into machine code, and its search for a pattern that starts with a class or a word in any case
goes as fast as a search for text.

Rust's `\w` and `\d` are Unicode's unless `(?-u)` is written, while SG's are ASCII's as RE2's, and PCRE2
backtracks where SG and Rust never do: the patterns measured mean the same in all three.
