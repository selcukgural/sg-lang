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
| [grep](grep) | The lines a regular expression matches, with `-c`, `-i`, `-n` and `-v` | 133 |

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

Still to come, in this order: a `jq` of JSON, a key-value store with a write-ahead log, a pool of workers,
an HTTP/1.1 file server under load, a GIF tool, and an interpreter of Lox.

## How they are written (1525-1532)

- Each trial has a folder with `sg/`, `rust/` and `zig/`. Rust uses its standard library and the crates
  people use for the job (`regex`), pinned in `Cargo.lock`, and threads rather than an async runtime. Zig
  uses its standard library, and a library of C where Zig has none, as Zig programs do: `grep` links
  PCRE2 (with its JIT) through `build.zig` (1541), and `std/unicode` ICU4C, Zig having no categories or
  normal forms (1659). SG uses only what comes with the language.
- The three give the same output, byte for byte, on the same input; their errors are worded each in its
  own language's way. `tests/cli.rs` (`the_trials`) runs the SG versions interpreted and compiled on a
  megabyte and holds them to what Rust and Zig print.
- Nothing large is kept here: [`inputs/text.sg`](inputs/text.sg) makes lines of a log from a seed, the
  same bytes on every machine, [`inputs/json.sg`](inputs/json.sg) the documents of `std/json`, and
  [`inputs/bytes.sg`](inputs/bytes.sg) the bytes that do not compress of `std/compression` and
  `std/encoding`, and [`inputs/unicode.sg`](inputs/unicode.sg) the text in many writings of `std/unicode`.
  The gzip, the archives, the Base64 and the NFD that the programs read are made by Python's `gzip`,
  `zipfile`, `base64` and `unicodedata`, which none of the programs measured is.
- Where each language may lawfully write other bytes (compressed data, an archive), the program writes
  into a file, and `measure.py` opens each one with Python and holds it to the input before anything is
  timed; the size each wrote is printed under its table. What is too large for the output and must be
  the same everywhere (Base64, a normal form) goes into a file too, held to what Python makes of the input.

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
