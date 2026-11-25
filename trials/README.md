# Trials

Programs that people write, written in SG, in Rust and in Zig, the same algorithm each time and written
plainly, then measured against each other (choices 1418, 1540). Rust and Zig are the languages in SG's
lane: compiled ahead of time, no collector, memory held and freed by the program; Go, measured in the first
trial, was left for them. Each difference found is a numbered choice in
the [choices of each slice](../design/decisions/README.md), fixed and measured again.

**The numbers are in [`benchmarks/`](../benchmarks/README.md)**: the trials in
[Programs against Rust and Zig](../benchmarks/programs.md), the modules of `std` in
[The library against Rust and Zig](../benchmarks/library.md), and how they are measured, with the rules
that keep it fair, in [How it is measured](../benchmarks/method.md). This folder holds the code.

| Trial | What it is | Slice |
|---|---|---|
| [wc](wc) | The lines, words and bytes of files, read in pieces of 64 KB | 133 |
| [grep](grep) | The lines a regular expression matches, with `-c`, `-i`, `-n` and `-v`; a line is read and searched as bytes, so one that is not UTF-8 is written as it is (choice 1840) | 133, 173 |
| [jq](jq) | The jq people use, held byte for byte to jq 1.8.2, on records, lines and numbers of JSON and on inputs that break things (choices 1804-1822); jq and jaq measured beside it | 166 |
| [kv](kv) | A store of keys and values with a log written before each change, synced each block, each record or never; read back, cut by a crash, damaged, compacted; held to a model in Python and killed in the middle (choices 1844-1853) | 175 |
| [hashsum](hashsum) | The SHA-256 of each file from a pool of workers, written in the order given, byte for byte as GNU's `sha256sum` writes it; small files, large ones, one of 1 GB, a load that is not even, and the scaling with the workers (choices 1854-1860) | 176 |
| [giftool](giftool) | A GIF read, changed and written again: LZW by hand, cropped, flipped, turned, drawn whole (`--unoptimize`) and cut to what changed (`-O1`, `-O2`); `--info` byte for byte as `gifsicle -I`; held to a model in Python and to gifsicle (choices 1874-1881) | 179 |
| [lox](lox) | An interpreter of Lox after clox, the bytecode interpreter of *Crafting Interpreters*: compiler, machine, closures, classes and a collector over a heap of handles, NaN boxing; held byte for byte to clox on the book's 246 tests and 5,000 programs from a seed, and measured against it (choices 1883-1894); SG and clox only | 181 |

The modules of `std` are measured the same way, each in `trials/std/<module>` (choices 1553, 1629-1637),
and their tables are in [The library against Rust and Zig](../benchmarks/library.md):

| Module | What it is | Slice |
|---|---|---|
| [std/json](std/json) | A document read and walked, read and written back, and a document a line read | 149 |
| [std/compression](std/compression) | 100 MB of a log compressed in gzip and opened, 99,640 records each compressed and opened in zlib, 100 MB that do not compress | 150 |
| [std/zip](std/zip) | 100 MB written as 10,001 entries, the entries of an archive read, 100,000 entries listed | 150 |
| [std/hash](std/hash) | The SHA-256 of 100 MB, ten million hashes chained, an HMAC-SHA-256 of each of 921,952 lines | 151 |
| [std/checksum](std/checksum) | The CRC-32 and the Adler-32 of 100 MB, the CRC-32 of each of 921,952 lines | 151 |
| [std/encoding](std/encoding) | 100 MB written in Base64 and read back, ten million values of 32 bytes in hex and back, ten million tokens in the Base64 of URLs and back | 152 |
| [std/unicode](std/unicode) | The category of each character of 100 MB in many writings, each line into NFC and into NFD, each line checked to be NFC | 152 |
| [std/time](std/time) | Ten million moments written in ISO 8601, 100 MB of RFC 3339 read, a million moments in a time zone, the clock read ten million times | 153 |
| [std/random](std/random) | A hundred million numbers, a hundred million in a range, 100 MB of bytes, a million tokens of the system's bytes | 153 |
| [std/log](std/log) | A million lines, a hundred million that are off, a million from four tasks | 153 |
| [std/net](std/net) | Echoes of 64 bytes to 64 KB, 1,000 connections at once, 50,000 one after another, 1 GB one way, 100 MB in lines, datagrams, ten million addresses read and written; async and blocking | 154 |

Servers are measured in [server](server) (choices 1698-1707): a store that speaks a part of Redis's protocol,
an HTTP server, and an HTTP/1.1 file server with nginx beside it (choices 1861-1871), each in SG, Rust, Zig and
C, under one load written in C, on Linux, with their tables in
[Servers against Rust, Zig and C](../benchmarks/server.md).

From slice 180 on a new trial is written in SG alone and measured against the C program it mirrors (`lox`
against clox), not written again in Rust and Zig: three versions of every program took more time than the
language could spare. The trials before it keep theirs.

## How they are written (1525-1532)

- Each trial has a folder with `sg/`, `rust/` and `zig/`. Rust uses its standard library and the crates
  people use for the job (`regex`), pinned in `Cargo.lock`, and threads rather than an async runtime,
  except where SG's async functions are measured: `std/net` uses `tokio` for its async cases (1678). Zig
  uses its standard library, and a library of C where Zig has none, as Zig programs do: `grep` links
  PCRE2 (with its JIT) through `build.zig` (1541), `std/unicode` ICU4C, Zig having no categories or
  normal forms (1659), and `std/time` the C library's `localtime_r`, Zig having no time zones; its
  calendar and its ISO 8601 are written by hand (1667). SG uses only what comes with the language.
- The three give the same output, byte for byte, on the same input; their errors are worded each in its
  own language's way. `tests/cli.rs` (`the_trials`) runs the SG versions interpreted and compiled on a
  megabyte and holds them to what Rust and Zig print, and `grep` to the bytes of lines that are not
  UTF-8; `the_trial_of_jq` holds `jq` to what jq 1.8.2 prints. `Regex`, which `grep` leans on, is held
  to Rust's `regex` and PCRE2 by `interpreter/tests/regex-suite/differ` (1841). A trial written in
  several files has its SG version as a folder, `sg/`, built whole.
- The trials are ruthless: each pushes the language as far as the world will (a text of 100 MB, ten
  million members, 10,000 levels and one more, bad UTF-8 of each kind, numbers past a `double`, a million
  calls deep, a file cut off), and no case is left out because SG is weak there (choice 1809).
- Each language is written its own way, as its programmers write it; what is a way of working, an
  algorithm, goes into all three or into none (choice 1819).
- Nothing large is kept here: [`inputs/text.sg`](inputs/text.sg) makes lines of a log from a seed, the
  same bytes on every machine, [`inputs/json.sg`](inputs/json.sg) the documents of `std/json`, and
  [`inputs/bytes.sg`](inputs/bytes.sg) the bytes that do not compress of `std/compression` and
  `std/encoding`, [`inputs/unicode.sg`](inputs/unicode.sg) the text in many writings of `std/unicode`,
  [`inputs/time.sg`](inputs/time.sg) the moments in RFC 3339 of `std/time`, and
  [`inputs/address.sg`](inputs/address.sg) the v4 and v6 addresses with their ports of `std/net`; the
  inputs of `jq` that break things are written by `measure.py` itself.
  The gzip, the archives, the Base64 and the NFD that the programs read are made by Python's `gzip`,
  `zipfile`, `base64` and `unicodedata`, which none of the programs measured is.
- Where each language may lawfully write other bytes (compressed data, an archive), the program writes
  into a file, and `measure.py` opens each one with Python and holds it to the input before anything is
  timed; the size each wrote is printed under its table. What is too large for the output and must be
  the same everywhere (Base64, a normal form) goes into a file too, held to what Python makes of the input.
- What a log writes on the standard error goes into a file; each line is held to its form and the
  messages, in the order of their text, must be the same everywhere. Where the numbers drawn differ
  (Zig's generator is another), the programs print only whether they look random (1669, 1670). Every
  program runs with `TZ=Europe/Istanbul` and without `SG_LOG` or `RUST_LOG`.
- The network runs over the loopback, the server and its clients tasks of one program, each on a port the
  system chooses. The side that closes a connection first waits a while before its port is free again, and
  macOS has 16,384 ports for connections, so the server closes first (1684). A run that has not ended
  after five minutes stops the measuring, and what its threads were doing is written into
  `trials/target/hung-PID.txt` (1683); a run that exits with a code other than 0 stops it too, so that no
  program that gave up early is counted fast (1685).

## Fairness (1554)

No measurement is made, chosen or reported so that it favours SG. The rules are in
[How it is measured](../benchmarks/method.md#fairness).

## Measuring

From the repository's root, with Rust, Zig 0.17, PCRE2, ICU4C and a C compiler installed:

```
python3 trials/measure.py               # every trial: checked, then measured
python3 trials/measure.py grep          # one trial
python3 trials/measure.py std/json      # one module of std
python3 trials/measure.py --check       # only that the outputs are the same
```

How the time, the memory, the size and the lines are taken, and how each language is built, is in
[How it is measured](../benchmarks/method.md#against-rust-and-zig).

## Results

In [Programs against Rust and Zig](../benchmarks/programs.md) and
[The library against Rust and Zig](../benchmarks/library.md).
