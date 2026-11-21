# The library against Rust and Zig

Each module of `std` measured against the crates Rust's people use for the job and against Zig's own
library, as the trials are (1553, 1629-1637): the time, the best of five runs after one to warm up; the most
memory held; the size of the program stripped; and its lines. SG is built with `--release` and once more with
its arithmetic checked, Zig with `ReleaseFast` and `ReleaseSafe`. The machine, the compilers and the flags
are in [How it is measured](method.md), and so are the rules of fairness (1554). `python3 trials/measure.py std/json`
measures one module again.

Not measured yet: `atomic`, `command_line`, `environment`, `files`, `log`, `net`, `path`, `process`, `random`, `regex`, `signals`, `time`.

The trial `grep` measures `Regex.isMatch` already ([Programs against Rust and Zig](programs.md#grep)); `regex` gets a table of its own,
with `findAll`, captures and replacing, in a later slice, as do `atomic`, `files`, `process` and `signals`,
which 1637 does not name.

## json

`trials/std/json`: `sum` reads a document and walks it, counting the values of each kind and adding up
the numbers; `copy` reads it and writes it back on one line; `lines` reads a document on each line and walks
each. 100 MB of each kind from `trials/inputs/json.sg`, seed 1: records (numbers whole and not, texts with
escapes and letters past ASCII, nested objects and arrays), one record a line, and polygons of coordinates
of seventeen digits, as `canada.json`. Apple M3 Max, 21 November 2026:

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| Records, read and summed | 0.476 s, 786 MB | 0.494 s, 786 MB | 0.668 s, 1286 MB | 0.552 s, 1074 MB | 0.697 s, 1191 MB |
| Records, read and written back | 0.688 s, 881 MB | 0.722 s, 881 MB | 0.763 s, 1286 MB | 0.719 s, 1074 MB | 0.889 s, 1191 MB |
| A document a line, each read and summed | 0.392 s, 2.4 MB | 0.412 s, 2.4 MB | 0.543 s, 2.8 MB | 0.464 s, 2.2 MB | 0.582 s, 2.3 MB |
| Coordinates, read and summed | 0.355 s, 518 MB | 0.358 s, 518 MB | 0.415 s, 1117 MB | 0.457 s, 942 MB | 0.591 s, 1066 MB |
| Size, stripped | 99 KB | 99 KB | 485 KB | 426 KB | 522 KB |
| Lines | 130 | 130 | 111 | 110 | 110 |

Rust is `serde_json` 1.0.154 into a `Value`, with `preserve_order` and `float_roundtrip` (1638); without
them the coordinates take 0.386 s and come out other numbers. Zig is `std.json` into a `Value`, in an arena
(`parseFromSliceLeaky`), emptied before each line. The outputs are the same bytes; every number with a
fraction in the inputs is written without an exponent and is not whole, since the three write `1e-7`,
`1e+16` and `2.0` each its own way. A struct mapped by `#[derive(Deserialize)]` reads faster in Rust, but
does other work: SG has no mapping (1411). Before 1639 and 1640, SG took 0.590 s and 1,347 MB, 0.802 s and
1,443 MB, 0.438 s, and 0.467 s and 1,335 MB: the coordinates were the case it lost, at 1.13 times Rust.
`copy` holds 95 MB more than `sum`: `toJson` makes the whole text before it is written, where Rust and Zig
write as they go.

## compression

`trials/std/compression`, at level 6: `compress` reads 100 MB of the log of `trials/inputs/text.sg` and writes
it in gzip into a file, a piece at a time; `decompress` opens the same, made by Python's `gzip`, on the
standard output; `records` compresses each of 99,640 lines of `trials/inputs/json.sg` (about 370 bytes) in
zlib and opens it again in memory; and the last case compresses 100 MB of `trials/inputs/bytes.sg`, which do
not compress. Apple M3 Max, 21 November 2026:

| Case | SG | SG, checked | Rust | Rust, `zlib-rs` | Zig | Zig, safe |
|---|---|---|---|---|---|---|
| 100 MB of a log, compressed | 1.616 s, 2.6 MB | 1.616 s, 2.6 MB | 1.675 s, 2.6 MB | 0.949 s, 2.7 MB | 1.556 s, 2.5 MB | 1.687 s, 2.7 MB |
| 100 MB of a log, opened | 0.084 s, 2.7 MB | 0.124 s, 2.7 MB | 0.175 s, 2.3 MB | 0.086 s, 2.3 MB | 0.171 s, 2.3 MB | 0.206 s, 2.4 MB |
| 99,640 records, each compressed and opened | 1.562 s, 2.6 MB | 1.765 s, 2.7 MB | 1.551 s, 2.6 MB | 1.694 s, 2.8 MB | 1.669 s, 2.4 MB | 2.020 s, 2.8 MB |
| 100 MB that do not compress, compressed | 1.483 s, 2.6 MB | 1.592 s, 2.6 MB | 1.578 s, 2.5 MB | 1.267 s, 2.7 MB | 1.622 s, 2.5 MB | 2.020 s, 2.7 MB |
| Size, stripped | 115 KB | 132 KB | 435 KB | 483 KB | 394 KB | 474 KB |
| Lines | 86 | 86 | 89 | 89 | 93 | 93 |

Rust is `flate2` 1.1.11, with its default `miniz_oxide` 0.9.1 and once more with the feature `zlib-rs`
(0.6.8); Zig is `std.compress.flate`. What is compressed is not compared, since each finds its own matches:
`measure.py` opens what each wrote and holds it to the input. The log took SG 18,728,988 bytes (zlib's own,
byte for byte), `miniz_oxide` 18,371,889, `zlib-rs` 19,275,743 and Zig 19,229,859; the records 27,113,059,
27,110,665, 27,101,349 and 27,112,644; the bytes that do not compress 100,030,523, 100,015,773, 100,030,538
and 100,167,750. `zlib-rs` compresses at level 6 by zlib-ng's own way, which SG does not take (1485, 1651).
Before 1646 to 1649, SG took 1.768 s, 0.169 s, 2.253 s and 1.490 s; before the CRC-32 and Adler-32 of the
hundred-and-fifty-first slice (1656, 1658), 1.632 s, 0.108 s, 1.564 s and 1.511 s.

## zip

`trials/std/zip`: `write` cuts the same 100 MB of the log into 10,001 entries of 10,000 bytes and writes them
deflated at level 6; `read` opens every entry of an archive of the same entries made by Python's `zipfile`
into memory, its CRC-32 checked; `list` prints the name and the size of each of 100,000 small entries. Apple
M3 Max, 21 November 2026:

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| 100 MB in 10,001 entries, written | 1.068 s, 100.7 MB | 1.200 s, 100.9 MB | 0.770 s, 101.8 MB | 1.198 s, 98.7 MB | 1.322 s, 98.9 MB |
| 10,001 entries, read | 0.139 s, 4.3 MB | 0.185 s, 4.3 MB | 0.183 s, 4.8 MB | 0.517 s, 2.2 MB | 0.586 s, 2.5 MB |
| 100,000 entries, listed | 0.027 s, 19.6 MB | 0.027 s, 19.6 MB | 0.251 s, 22.7 MB | 0.008 s, 2.2 MB | 0.010 s, 2.5 MB |
| Size, stripped | 261 KB | 277 KB | 1678 KB | 394 KB | 490 KB |
| Lines | 61 | 61 | 66 | 159 | 159 |

Rust is the `zip` crate 9.8.1 with its default features (it deflates with `zlib-rs` 0.6.8, and brings its
other methods and encryption along, hence its size). Zig's `std.zip` reads archives and does not write them,
so its `write` writes the headers of `std.zip` itself with `std.compress.flate` (1644), and its `read`
checks the CRC-32 that `std.zip` does not (1645). The archives written took SG 25,831,364 bytes, Rust
25,934,270 and Zig 25,809,759, each read back by Python before anything was timed. SG checks every name of
the archive against the others when it opens it (1503), which Rust and Zig do not; that is the time and the
memory of the listing, which Zig does without keeping anything. Before 1650 and 1652, SG took 1.191 s,
0.231 s and 7.2 MB, and 0.048 s and 49.8 MB; before the CRC-32 of 1656, 1.086 s and 0.164 s.

## hash

`trials/std/hash`: `sha256` hashes 100 MB of the log, read 64 KB at a time, as `sha256sum` does; `chain`
hashes 32 zero bytes ten million times, each time the hash of the one before, the cost of one call with
nothing read; `hmac` signs each of the 921,952 lines of the log with HMAC-SHA-256 under one key of 32 bytes
and puts the tags together with XOR. Apple M3 Max, 21 November 2026:

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| 100 MB of a log, hashed | 0.049 s, 2.0 MB | 0.048 s, 2.0 MB | 0.045 s, 2.0 MB | 0.041 s, 2.0 MB | 0.045 s, 2.2 MB |
| 10,000,000 hashes, chained | 0.331 s, 1.8 MB | 0.334 s, 1.8 MB | 0.323 s, 2.0 MB | 0.284 s, 2.0 MB | 0.281 s, 2.1 MB |
| 921,952 lines, each signed | 0.138 s, 1.9 MB | 0.138 s, 1.9 MB | 0.144 s, 2.1 MB | 0.113 s, 2.0 MB | 0.122 s, 2.2 MB |
| Size, stripped | 67 KB | 67 KB | 352 KB | 345 KB | 442 KB |
| Lines | 67 | 67 | 80 | 76 | 76 |

Rust is `sha2` 0.11.1 and `hmac` 0.13.0 with their default features, Zig `std.crypto.hash.sha2` and
`std.crypto.auth.hmac`; all three use the SHA instructions of the processor, SG since 1657. A line is signed
without its end, `\n` or `\r\n`, by each. `Bytes.equalInConstantTime` is not measured: alone it takes too
little time to tell. Before 1657, SG took 0.303 s, 2.018 s and 1.001 s, six to nine times the others.

## checksum

`trials/std/checksum`: the CRC-32 and the Adler-32 of 100 MB of the log, read 64 KB at a time, and the CRC-32
of each of its 921,952 lines, put together with XOR. Apple M3 Max, 21 November 2026:

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| 100 MB of a log, CRC-32 | 0.013 s, 2.0 MB | 0.012 s, 2.0 MB | 0.010 s, 2.1 MB | 0.219 s, 2.0 MB | 0.220 s, 2.2 MB |
| 100 MB of a log, Adler-32 | 0.024 s, 2.0 MB | 0.046 s, 2.0 MB | 0.022 s, 2.1 MB | 0.034 s, 2.0 MB | 0.036 s, 2.1 MB |
| 921,952 lines, each a CRC-32 | 0.044 s, 1.9 MB | 0.044 s, 1.9 MB | 0.035 s, 2.1 MB | 0.169 s, 2.0 MB | 0.170 s, 2.2 MB |
| Size, stripped | 83 KB | 83 KB | 352 KB | 345 KB | 442 KB |
| Lines | 55 | 55 | 58 | 61 | 61 |

Rust is `crc32fast` 1.5.5 and `adler2` 2.0.1 (the Adler-32 of `miniz_oxide`) with their default features;
`crc32fast` folds with the processor's carry-less product, as SG does since 1656. Zig's `std.hash.Crc32`
takes a byte at a time through one table. Reading 100 MB alone takes about 9 ms of each case. Before 1656
and 1658, SG took 0.035 s, 0.038 s and 0.060 s.

## encoding

`trials/std/encoding`: `encode` writes 100 MB of bytes that do not repeat (`trials/inputs/bytes.sg`) in
Base64, read 48 KB at a time, and `decode` reads Python's Base64 of them back, 64 KB of text at a time, each
into a file that must be what Python's `base64` makes; `hex` writes ten million values of 32 bytes, the size
of a SHA-256, as hex and reads each back, and `tokens` does the same with ten million tokens of 16 to 48
bytes in the Base64 of URLs, both from SplitMix64 and with no file read. Each program writes into buffers it
keeps from one call to the next: SG through `encodeInto` and `decodeInto` (1665). Apple M3 Max, 21 November
2026:

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| 100 MB, written in Base64 | 0.069 s, 2.2 MB | 0.095 s, 2.2 MB | 0.050 s, 2.2 MB | 0.070 s, 2.1 MB | 0.072 s, 2.3 MB |
| 133 MB of Base64, read back | 0.066 s, 2.2 MB | 0.093 s, 2.2 MB | 0.057 s, 2.2 MB | 0.053 s, 2.1 MB | 0.059 s, 2.3 MB |
| 10,000,000 values of 32 bytes, in hex and back | 0.621 s, 1.9 MB | 0.733 s, 1.9 MB | 1.852 s, 2.0 MB | 0.402 s, 2.0 MB | 1.742 s, 2.1 MB |
| 10,000,000 tokens, in Base64 for URLs and back | 0.605 s, 1.9 MB | 0.800 s, 1.9 MB | 0.443 s, 2.0 MB | 0.448 s, 2.0 MB | 0.510 s, 2.1 MB |
| Size, stripped | 67 KB | 83 KB | 369 KB | 361 KB | 474 KB |
| Lines | 169 | 169 | 137 | 152 | 152 |

Rust is `base64` 0.22.1 and `hex` 0.4.3 with their default features, through `encode_slice`, `decode_slice`,
`encode_to_slice` and `decode_to_slice`; Zig `std.base64` and `std.fmt.bytesToHex` and `hexToBytes`, whose
hex of 32 bytes is an array of a size known when the program is compiled. SG reads its Base64 text checked to
be UTF-8 first, as `decode` takes a `string`. Before 1664 and 1665 SG took 0.437 s, 0.635 s, 3.931 s and
4.531 s: a character written into a text at a time, each digit read through a row of comparisons, the bytes
added to a list that grew, and a new text and list for each value. What is still slower is in
[Slower than C](slower-than-c.md#the-library).

## unicode

`trials/std/unicode`: on each of the 950,206 lines of 100 MB of text in many writings
(`trials/inputs/unicode.sg`: Turkish, French, German, Vietnamese, Greek with its old accents, Cyrillic,
Korean, Japanese, Arabic, Devanagari, emoji and ASCII; 43% of the characters past ASCII), `category` counts
the general category of each character; `nfc` writes each line of the text in NFD (made by Python's
`unicodedata`, 121 MB) in NFC, and `nfd` each line of the text in NFD, each into a file that must be what
Python makes; `check` asks of each line whether it is in NFC. Apple M3 Max, 21 November 2026:

| Case | SG | SG, checked | Rust | Rust, `unicode-normalization` | Zig | Zig, safe |
|---|---|---|---|---|---|---|
| 100 MB, the category of each character | 0.464 s, 2.0 MB | 0.480 s, 2.0 MB | 0.491 s, 2.1 MB | 0.488 s, 2.1 MB | 0.499 s, 2.2 MB | 0.524 s, 2.3 MB |
| 121 MB in NFD, each line into NFC | 0.755 s, 2.3 MB | 0.766 s, 2.3 MB | 0.932 s, 2.2 MB | 1.301 s, 2.3 MB | 1.008 s, 2.5 MB | 1.013 s, 2.6 MB |
| 100 MB in NFC, each line into NFD | 0.650 s, 2.4 MB | 0.660 s, 2.4 MB | 0.657 s, 2.2 MB | 1.087 s, 2.2 MB | 0.753 s, 2.5 MB | 0.766 s, 2.6 MB |
| 950,206 lines, each checked to be NFC | 0.308 s, 2.0 MB | 0.310 s, 2.0 MB | 0.428 s, 2.2 MB | 0.492 s, 2.1 MB | 0.378 s, 2.4 MB | 0.381 s, 2.6 MB |
| Size, stripped | 179 KB | 179 KB | 450 KB | 498 KB | 362 KB | 442 KB |
| Lines | 111 | 111 | 143 | 143 | 133 | 133 |

Rust is ICU4X, `icu_properties` 2.4.0 and `icu_normalizer` 2.4.0, with their default features, writing into
a text it keeps; built once more with the feature `unicode-normalization`, its normal forms are those of the
crate `unicode-normalization` 0.1.26, the one most used, and its categories still ICU4X's. Zig's library has
neither, so the Zig program calls ICU4C 79.1, as the trial of compression calls zlib where Zig has no
compressor: its normal forms take UTF-16, and each line is turned into UTF-16 and back, which is part of
what is measured. ICU4C 79 has the tables of Unicode 18.0, SG and Python those of 17.0; no character of the
text is new in 18.0, and every build writes the same bytes. Nothing had to be made faster for this table.

