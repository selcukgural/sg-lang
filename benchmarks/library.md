# The library against Rust and Zig

Each module of `std` measured against the crates Rust's people use for the job and against Zig's own
library, as the trials are (1553, 1629-1637): the time, the best of five runs after one to warm up; the most
memory held; the size of the program stripped; and its lines. SG is built with `--release` and once more with
its arithmetic checked, Zig with `ReleaseFast` and `ReleaseSafe`. The machine, the compilers and the flags
are in [How it is measured](method.md), and so are the rules of fairness (1554). `python3 trials/measure.py std/json`
measures one module again.

Not measured yet: `atomic`, `files`, `process`, `regex`, `signals`.

The trial `grep` measures `Regex.isMatch` already ([Programs against Rust and Zig](programs.md#grep)); `regex` gets a table of its own,
with `findAll`, captures and replacing, in a later slice, as do `atomic`, `files`, `process` and `signals`,
which 1637 does not name. The command line and the environment are measured for their memory and size alone,
since their work takes microseconds (1553, 1801).

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

## time

`trials/std/time`: `format` writes ten million moments between 1900 and 2200 in ISO 8601 into a text kept
from one to the next, a quarter of them whole seconds and the rest with milliseconds, microseconds or
nanoseconds; `parse` reads 100 MB of moments in RFC 3339 (`trials/inputs/time.sg`: offsets from -14:00 to
+14:00 and `Z`, fractions of 0 to 9 digits) and takes each to UTC; `local` sees a million moments in the time
zone of the system, `TZ=Europe/Istanbul`; `clock` reads the monotonic clock ten million times. The moments
come from SplitMix64, and each program prints sums of what it wrote or read. Apple M3 Max, 22 November 2026:

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| 10,000,000 moments, written in ISO 8601 | 0.490 s, 1.8 MB | 0.597 s, 1.8 MB | 0.497 s, 5.4 MB | 0.700 s, 2.0 MB | 1.023 s, 2.1 MB |
| 3,407,285 moments in RFC 3339, read | 0.136 s, 1.9 MB | 0.138 s, 1.9 MB | 0.173 s, 5.4 MB | 0.125 s, 2.0 MB | 0.127 s, 2.2 MB |
| 1,000,000 moments, in the time zone | 0.203 s, 2.0 MB | 0.223 s, 2.0 MB | 0.054 s, 5.4 MB | 0.189 s, 2.2 MB | 0.193 s, 2.3 MB |
| The clock, read 10,000,000 times | 0.097 s, 1.8 MB | 0.099 s, 1.8 MB | 0.166 s, 5.3 MB | 0.158 s, 2.0 MB | 0.154 s, 2.1 MB |
| Size, stripped | 67 KB | 67 KB | 403 KB | 361 KB | 442 KB |
| Lines | 117 | 117 | 115 | 235 | 235 |

Rust is `chrono` 0.4.45 with its default features, writing with `to_rfc3339_opts(SecondsFormat::AutoSi,
true)`, four times as fast as `write!` with a format of `%` letters, reading with `parse_from_rfc3339`, and
seeing the zone with `with_timezone(&Local)`; its clock is `std::time::Instant`. Zig's library has no
calendar before 1970, no ISO 8601 and no time zones: its program writes the calendar by hand (the days of
Howard Hinnant), the text with `std.fmt`, reads RFC 3339 by hand, asks the C library's `localtime_r` for the
zone, and reads `std.Io.Clock.awake`. Before choice 1673 SG wrote the moments in 1.183 s, each part of a date
going through `snprintf`, and before 1674 read the clock in 0.178 s (`CLOCK_MONOTONIC` of macOS, where Rust
and Zig read `CLOCK_UPTIME_RAW`). `chrono` sees a zone 3.8 times as fast, having read its TZif file once;
SG asks `localtime_r` each time, as Zig does ([Slower than C](slower-than-c.md#the-library)).

## random

`trials/std/random`: `numbers` draws a hundred million numbers of 64 bits from the seed 1, `range` a hundred
million from 0 up to 1,000 into a histogram, `fill` 100 MB of bytes 64 KB at a time into one buffer, and
`tokens` a million tokens of 64 hex digits from the random bytes of the system. Zig's generator is not SG's,
so what is printed does not depend on the numbers drawn: whether the bits set, each bar of the histogram and
the mean of every 64th byte lie within six standard deviations of what they should be, and whether every
token is hex. Apple M3 Max, 22 November 2026:

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| 100,000,000 numbers of 64 bits | 0.100 s, 1.8 MB | 0.100 s, 1.8 MB | 0.100 s, 2.0 MB | 0.095 s, 2.0 MB | 0.272 s, 2.1 MB |
| 100,000,000 numbers up to 1,000 | 0.120 s, 1.9 MB | 0.111 s, 1.9 MB | 0.109 s, 2.0 MB | 0.101 s, 2.0 MB | 0.098 s, 2.1 MB |
| 100 MB of bytes | 0.014 s, 2.0 MB | 0.014 s, 2.0 MB | 0.014 s, 2.0 MB | 0.015 s, 2.0 MB | 0.034 s, 2.1 MB |
| 1,000,000 tokens of the system | 0.242 s, 1.9 MB | 0.243 s, 1.9 MB | 0.957 s, 2.0 MB | 0.051 s, 2.0 MB | 0.066 s, 2.1 MB |
| Size, stripped | 34 KB | 50 KB | 353 KB | 345 KB | 442 KB |
| Lines | 78 | 78 | 64 | 86 | 86 |

Rust is `rand` 0.10.4 with `rand_xoshiro` 0.8.2's `Xoshiro256StarStar::seed_from_u64`, SG's own generator
(xoshiro256\*\* seeded with SplitMix64), whose numbers and bytes were held to be SG's on a million draws; its
range is `random_range`, its tokens `SysRng` and `hex::encode_to_slice`. Zig is `std.Random.DefaultPrng`,
xoshiro256++, a cheaper step than xoshiro256\*\*, with `uintLessThan` and `bytes`, and its tokens `Io.random`,
a ChaCha kept in the process and seeded by the system; SG asks the system for each token (`arc4random_buf`),
as Rust's `SysRng` does (choice 1676). Before choice 1673 `fill` took 0.031 s, a byte at a time, and the
tokens 0.341 s, a digit at a time.

## log

`trials/std/log`: `lines` writes a million lines at the level Info, `request 12 took 84 ms`; `off` makes a
hundred million calls at the level Debug while the log writes from Info, none of which makes its text; `tasks`
has four tasks write a quarter of a million lines each at once. Each line goes to the standard error, a file,
as `2026-11-22T20:15:03.123Z INFO  ` and the message, and `measure.py` holds every line to that form and the
messages to be the same in every build. Apple M3 Max, 22 November 2026:

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| 1,000,000 lines | 1.864 s, 1.9 MB | 1.894 s, 1.9 MB | 1.851 s, 2.1 MB | 1.708 s, 2.0 MB | 1.805 s, 2.1 MB |
| 100,000,000 lines that are off | 0.052 s, 1.8 MB | 0.083 s, 1.8 MB | 0.059 s, 2.1 MB | 0.042 s, 2.0 MB | 0.065 s, 2.1 MB |
| 1,000,000 lines from four tasks | 2.761 s, 2.2 MB | 2.876 s, 2.2 MB | 3.642 s, 2.1 MB | 2.567 s, 2.1 MB | 2.615 s, 2.2 MB |
| Size, stripped | 68 KB | 84 KB | 473 KB | 361 KB | 458 KB |
| Lines | 47 | 47 | 50 | 99 | 99 |

Rust is `log` 0.4.35 with `env_logger` 0.11.12, given SG's form of line, its time to the millisecond and no
colour; Zig is `std.log` with a `logFn` that writes the same line under the lock of the standard error, the
calendar by hand. Each line is one write of the system in all three, which is most of the time. Zig's level
is fixed when the program is compiled, so its lines that are off are not in the program; SG reads its level,
which `SG_LOG` may set, as the program runs. Before choice 1673 four tasks took 4.209 s on macOS, waiting in
the kernel on the lock of C's stream.

## net

`trials/std/net`: each program is the server and its clients at once, tasks of one process over the loopback,
each case once with async functions and once with functions that block, one task on a thread of its own
(1678-1682). `echo` has 8 clients send messages of 64 bytes, 4 KB or 64 KB and wait for each to come back,
20,000 times each, and 1,000 clients do it at once 100 times; the server closes each connection once it has sent
back all of it, so that the wait of TCP after a close lies on its side and the ports of the clients are free again
(1684). `connect` makes 50,000 connections one after another, a byte there and back on each. `stream` sends 1 GB
one way, 4 KB or 64 KB at a time. `lines` sends 100 MB of a log, which the other side reads line by line.
`udp` has 8 sockets send 25,000 datagrams of 64 bytes, 1 KB or 8 KB each to one that sends each back. `address`
reads ten million addresses with their ports, half of them v6 in four ways of writing, from a new maker,
`trials/inputs/address.sg`, and writes each back into a text kept from one to the next; `measure.py` holds what
each program writes of 1,000 of them to what Python's `ipaddress` writes. Apple M3 Max, 22 November 2026:

| Case | SG | SG, checked | Rust | Zig | Zig safe |
|---|---|---|---|---|---|
| echo, 64 bytes, async | 1.546 s, 2.4 MB | 1.527 s, 2.4 MB | 1.185 s, 2.9 MB | 1.473 s, 2.8 MB | 1.464 s, 3.1 MB |
| echo, 64 bytes, blocking | 1.439 s, 2.2 MB | 1.451 s, 2.2 MB | 1.440 s, 2.5 MB | 1.464 s, 2.8 MB | 1.465 s, 3.2 MB |
| echo, 4 KB, async | 1.572 s, 2.5 MB | 1.548 s, 2.5 MB | 1.207 s, 3.0 MB | 1.483 s, 2.8 MB | 1.490 s, 3.2 MB |
| echo, 4 KB, blocking | 1.471 s, 2.3 MB | 1.478 s, 2.3 MB | 1.484 s, 2.6 MB | 1.495 s, 2.9 MB | 1.493 s, 3.2 MB |
| echo, 64 KB, async | 3.699 s, 3.9 MB | 3.709 s, 3.9 MB | 3.491 s, 4.4 MB | 3.579 s, 4.2 MB | 3.588 s, 4.8 MB |
| echo, 64 KB, blocking | 3.577 s, 3.8 MB | 3.599 s, 3.7 MB | 3.592 s, 4.0 MB | 3.586 s, 4.2 MB | 3.576 s, 4.8 MB |
| 1,000 connections at once, async | 0.959 s, 3.9 MB | 0.928 s, 3.9 MB | 0.765 s, 3.8 MB | 0.945 s, 9.5 MB | 0.956 s, 12.3 MB |
| 50,000 connections one after another, async | 4.000 s, 2.5 MB | 4.244 s, 2.6 MB | 4.690 s, 2.9 MB | 3.972 s, 2.1 MB | 4.046 s, 2.2 MB |
| 50,000 connections one after another, blocking | 3.946 s, 2.1 MB | 3.991 s, 2.1 MB | 4.073 s, 2.1 MB | 4.082 s, 2.1 MB | 4.086 s, 2.2 MB |
| 1 GB in pieces of 4 KB, async | 0.425 s, 2.4 MB | 0.399 s, 2.4 MB | 0.451 s, 2.9 MB | 0.413 s, 2.1 MB | 0.438 s, 2.2 MB |
| 1 GB in pieces of 4 KB, blocking | 0.414 s, 2.0 MB | 0.416 s, 2.0 MB | 0.380 s, 2.1 MB | 0.440 s, 2.1 MB | 0.427 s, 2.2 MB |
| 1 GB in pieces of 64 KB, async | 0.140 s, 2.5 MB | 0.141 s, 2.5 MB | 0.153 s, 3.1 MB | 0.165 s, 2.2 MB | 0.177 s, 2.3 MB |
| 1 GB in pieces of 64 KB, blocking | 0.166 s, 2.1 MB | 0.170 s, 2.1 MB | 0.166 s, 2.3 MB | 0.176 s, 2.2 MB | 0.168 s, 2.3 MB |
| 100 MB in lines, async | 0.053 s, 97.8 MB | 0.053 s, 97.8 MB | 0.067 s, 98.3 MB | 0.037 s, 97.5 MB | 0.035 s, 97.7 MB |
| 100 MB in lines, blocking | 0.047 s, 97.4 MB | 0.047 s, 97.5 MB | 0.070 s, 97.6 MB | 0.035 s, 97.5 MB | 0.036 s, 97.7 MB |
| datagrams of 64 bytes, async | 1.655 s, 2.3 MB | 1.662 s, 2.4 MB | 1.667 s, 2.9 MB | 1.600 s, 2.5 MB | 1.602 s, 2.7 MB |
| datagrams of 64 bytes, blocking | 1.585 s, 2.1 MB | 1.587 s, 2.1 MB | 1.595 s, 2.3 MB | 1.609 s, 2.5 MB | 1.589 s, 2.7 MB |
| datagrams of 1 KB, async | 1.683 s, 2.4 MB | 1.698 s, 2.4 MB | 1.700 s, 2.9 MB | 1.636 s, 2.5 MB | 1.629 s, 2.7 MB |
| datagrams of 1 KB, blocking | 1.613 s, 2.1 MB | 1.619 s, 2.1 MB | 1.614 s, 2.3 MB | 1.643 s, 2.5 MB | 1.628 s, 2.7 MB |
| datagrams of 8 KB, async | 1.781 s, 2.5 MB | 1.800 s, 2.5 MB | 1.786 s, 3.0 MB | 1.727 s, 2.6 MB | 1.724 s, 2.8 MB |
| datagrams of 8 KB, blocking | 1.713 s, 2.2 MB | 1.709 s, 2.2 MB | 1.701 s, 2.5 MB | 1.726 s, 2.6 MB | 1.739 s, 2.8 MB |
| 10,000,000 addresses, read and written | 1.488 s, 2.0 MB | 1.658 s, 2.0 MB | 2.018 s, 2.2 MB | 1.548 s, 2.1 MB | 2.120 s, 2.2 MB |
| Size, stripped | 150 KB | 183 KB | 938 KB | 377 KB | 474 KB |
| Lines | 559 | 559 | 529 | 331 | 331 |

Rust is `std::net` where the functions block, a thread for each task, and `tokio` 1.54.1 with the runtime of
several threads `#[tokio::main]` makes where they are async; Zig is `std.Io.net` on `Io.Threaded`, its default,
whose async functions are threads too (its loop on kqueue was not done on macOS). SG's lines are text, each
checked to be UTF-8, as Rust's `read_line` checks it; Zig's `takeDelimiter` gives bytes, and without the check
SG reads them in Zig's time. Since the 157th slice each thread of the pool has a loop of the network of its own
and a socket stays on the loop it was put on (1708-1714): the stream of 64 KB pieces now takes 0.92 times
`tokio`'s time (1.12 before) and 50,000 connections one after another 1.01 times Zig's (1.10 before), but where
the server and its clients talk back and forth inside one program, their sockets lie on the loops of different
threads and each message wakes another core: the echo of 64 bytes takes 1.30 times `tokio`'s time (1.01 before,
1.208 s) and 1,000 connections at once 1.25 times (0.788 s before). `tokio` waits on one loop, which keeps both
ends on one thread; [Slower than C](slower-than-c.md) keeps the case. Before choices 1687 to 1690 the addresses
took 2.262 s, a text made for every host, and 1,000 connections at once held 6.6 MB, each task carrying 600 bytes
for each call of the network it waits in.

### Several pieces at once

`trials/server/pieces/pieces.sg` and `pieces.c` send a head of 100 bytes and a body of 4 KB 200,000 times, or of
64 KB 30,000 times, over the loopback to a thread that reads and lets them go: after copying the two into one
buffer, and with `sendPieces` in SG and `writev` in C (1740). Linux in the virtual machine of the servers, the
nanoseconds of one message, medians of four runs, the sender and the reader on cores 0-3:

| Body | SG, copy and `send` | SG, `sendPieces` | C, copy and `send` | C, `writev` |
|---|---|---|---|---|
| 4 KB | 877 | 852 | 873 | 849 |
| 64 KB | 8,922 | 5,382 | 8,888 | 5,558 |

With a body of 64 KB the copy is two fifths of the time, and `sendPieces` takes it away, as `writev` does in C;
with 4 KB the call of the system is most of it, and the four are within 3%. Neither server of the server lane
sends this way: the store reads a value under the lock of its map, and the body of the HTTP server is 4 bytes.


## http

`std/http` is measured as the HTTP server of the server lane ([server.md](server.md)), which is written on
`HttpConnection` since the 162nd slice (`trials/server/http/sg`), against the server SG had before it, written
by hand on `TcpStream` (`trials/server/http/sg-by-hand`, choice 1771: no more than 3% behind it). Both answer
GET /ping with connections kept open; the module also reads every header to the rules of RFC 9112, writes a
`date`, and has its time limits on (10 s for a head, 60 s idle), which the hand-written server has only with
`--idle-limit`. The Linux machine of the lane (8 cores, the server on 4, the load of `load/load.c` on the other
4), three rounds taken in turn, the median, 23 November 2026; answers a second, and the processor's ticks the
server spent for each thousand answers:

| Case | By hand | By hand, `--idle-limit 60000` | `std/http` |
|---|---|---|---|
| 50 connections kept open | 1,348k/s, 0.228 | 1,375k/s, 0.227 | 1,375k/s, 0.226 |
| 1,000 connections kept open | 1,390k/s, 0.222 | 1,411k/s, 0.220 | 1,379k/s, 0.227 |

At 1,000 connections the module is 2.2% behind the hand-written server with the same limits. The first
version was 7.6% behind: it found the end of a head a byte at a time, sent through four async functions one
inside the other, and read the clock three times for each request, which costs much in a virtual machine;
a head is now looked through with `memchr`, an answer is sent from the function that wrote it, the `date` is
the second the kernel keeps, and `cancelAfter` counts from the kernel's last tick (choice 1786).

The checks of `Host` and of the target RFC 9112 asks for (choices 1790 to 1792) were measured the same way,
seven rounds at 1,000 connections, later the same day, when the machine gave every server a little less:

| 1,000 connections kept open | answers a second | ticks / 1,000 answers |
|---|---|---|
| By hand, `--idle-limit 60000` | 1,360k/s | 0.215 |
| `std/http` before the checks | 1,321k/s | 0.223 |
| `std/http` with the checks, bytes told apart by comparisons | 1,309k/s | 0.226 |
| `std/http` with the checks, bytes told apart by a table | 1,322k/s | 0.224 |

The checks cost about 2% with the comparisons; with a table of 256 bytes for what a byte of a token or of a host
may be, they cost nothing that shows, and the module stays 2.7% behind the hand-written server. At 50
connections the versions change places with the order they are run in: the one with the table 2.6% behind the
one before the checks when run after it, 1.5% ahead when run first.

### Against hyper

What the module answers to requests on the edges of HTTP/1.1 is set beside what hyper 1 answers to the
same bytes (choice 1794): `trials/server/http/conformance/compare.py` sends each of 62 requests, the 55 of
the golden test `ok_http_conformance` and seven more ways of smuggling a request, on a connection of its
own to two servers that answer alike (`echo.sg` and `hyper/`, with the same limits: a head of 8 KB, 100
headers, a body of 1 MB checked against its length first, as axum does), and writes the statuses that come
back and whether the server closed. On a Mac, 23 November 2026, 42 cases are the same and 20 differ:

| Requests | `std/http` | hyper | Why |
|---|---|---|---|
| No `Host`, two, or one that is not `host[:port]` (6 cases) | 400, closed | answered, kept open | RFC 9112 3.2 says 400 (a MUST); hyper leaves `Host` to the program |
| A target that is not a path, a whole `http` URL, or `*` for `OPTIONS` (4 cases: `p`, `GET *`, `ftp://`, a URL with a user) | 400, closed | answered, kept open | RFC 9112 3.2 and RFC 9110 4.2.4; hyper leaves it to the program |
| `Content-Length` and `Transfer-Encoding` both | 400, closed | the chunks read, answered, closed | RFC 9112 6.1 allows either; refusing is the safer of the two, as a proxy before the server may have read the length |
| `Transfer-Encoding: gzip, chunked`; `chunked, chunked` | 501; 400 | waits for the body | the module takes no coding but `chunked` and refuses `chunked` twice; hyper reads the chunks and leaves the rest to the program |
| `HTTP/2.0` in the request line | 505 | 400 | both refuse; 505 says why |
| `CONNECT` | 501 | given to the program | the module makes no tunnels |
| A request line longer than 8 KB | 414 | 431 | RFC 9112 3 asks 414 for a target too long |
| A header line longer than 8 KB | 431, closed | 431, closed or reset | the module reads what the client still sends before it closes (choice 1787); hyper closes at once, and the reset that sends can take the answer away; the client saw a reset in each of three runs here, and for the long request line in one |
| A chunk of 1 MB or more announced, its bytes not sent | 413 | waits for them | the module refuses the size as it reads it |
| A chunk size of 19 hex digits | 413 | 400 | both refuse |
| `Expect: 100-continue` with the body sent with the head | 200 | 100, then 200 | the module does not ask for a body it already has (RFC 9110 10.1.1 lets it) |

Where both refuse, what they refuse is the same: a length that differs, has a sign or is a list, a folded
line, a space before the colon, a bare CR, a zero byte, a version that is no version, a method or a header
name that is not a token, too many headers, a chunk size that is not hex. A `Host` of an IPv6 address is
looked at for the bytes it may hold, not for its whole form. A run takes about two minutes:

```sh
python3 trials/server/http/conformance/compare.py
```

## command_line

`trials/std/command_line`: the front of a packer of files, `pack [OPTIONS] FORMAT FOLDER [FILES]...`, with
four flags, three options (`--exclude` may be given again), two words in their places and the rest; it
prints what it read (1798). `a usual command line` mixes them in GNU's ways (`-vfn`, `--level=9`, `-xa*.tmp`,
`--` and a lone `-`); `10,000 words` gives the rest ten thousand names of files; `the help` prints the help,
which each library words its own way, so that case is held only to its exit code. Memory and size alone
(1801). Apple M3 Max, 23 November 2026:

| Case | SG | SG, checked | Rust `clap` | Rust `lexopt` | Zig | Zig, safe |
|---|---|---|---|---|---|---|
| a usual command line | 1.88 MB | 1.86 MB | 2.33 MB | 2.00 MB | 1.92 MB | 2.06 MB |
| 10,000 words | 2.78 MB | 2.77 MB | 5.02 MB | 3.17 MB | 2.56 MB | 2.67 MB |
| the help | 1.88 MB | 1.86 MB | 2.30 MB | 1.98 MB | 1.92 MB | 2.05 MB |
| Size, stripped | 83 KB | 83 KB | 730 KB | 369 KB | 361 KB | 442 KB |
| Lines | 34 | 34 | 75 | 100 | 149 | 149 |

Rust is `clap` 4.6.9 with its default features and `derive`, as most programs in Rust read their command
line, and once more `lexopt` 0.3.2, a parser as small as SG's, with the help written by hand (1796). Zig's
library has no parser, so the Zig program takes its arguments apart by hand, as the Zig compiler does (1797).
With ten thousand words Zig holds 0.22 MB less than SG: SG copies each word into the list of its name, where
Zig keeps a view of the arguments; `clap` keeps each word twice, as a text and as the value it parsed.

## environment

`trials/std/environment`: `get` reads a variable that is there, one that is not and one that is not UTF-8
(`caf\xe9`), each as text; `read` reads one variable a million times, each time as a new text, as
`Environment.get` gives it (1799). Memory and size alone (1801). Apple M3 Max, 23 November 2026:

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| one variable | 1.83 MB | 1.83 MB | 1.95 MB | 1.94 MB | 2.11 MB |
| a variable read 1,000,000 times | 1.88 MB | 1.91 MB | 1.97 MB | 1.97 MB | 2.12 MB |
| Size, stripped | 34 KB | 34 KB | 352 KB | 345 KB | 442 KB |
| Lines | 35 | 35 | 33 | 49 | 49 |

Rust is `std::env::var`. Zig 0.17 has no `getEnvVarOwned`, which 1799 named; its heir, `Environ.getAlloc`,
builds a map of the whole environment on each call, so the Zig program looks the variable up in the map Zig
gives `main` (`init.environ_map`), checks it is UTF-8 and copies it, the work SG and Rust do.

## path

`trials/std/path`: a million paths made from a seed, each built from one to four folders and a name, a fifth
of them from the root, with names past ASCII (`café`, `日本`) and one of six extensions or none; each is taken
apart into its name, its extension and its folder, and the bytes of each part are added up (1800). SG builds
each path with `Path.push` onto one text that grows (1802); the second case joins each part into a new text
with `Path.join`, which Rust and Zig run their one way again. Apple M3 Max, 23 November 2026:

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| 1,000,000 paths built and taken apart | 0.093 s, 1.9 MB | 0.096 s, 1.9 MB | 0.195 s, 2.1 MB | 0.103 s, 2.0 MB | 0.144 s, 2.1 MB |
| the same, SG joining each part into a new text | 0.126 s, 2.0 MB | 0.127 s, 2.0 MB | 0.197 s, 2.1 MB | 0.102 s, 2.0 MB | 0.145 s, 2.1 MB |
| Size, stripped | 50 KB | 50 KB | 352 KB | 345 KB | 442 KB |
| Lines | 71 | 71 | 56 | 77 | 77 |

Rust is `std::path`, a `PathBuf` pushed onto; Zig is `std.fs.path`, the parts of a path joined at once by
`path.join`. SG joining each part into a new text loses to Zig by 1.23 times: it makes four or five texts for
each path where Zig makes one, and `Path.push` is its answer. Before this slice `$"{a}/{b}".toString()` read a
format of `printf` twice, once to count and once to write, and SG took 0.210 s; a line made only of texts is
now copied straight into a text of its length, in every program, and took 0.121 s before `Path.push`.

The paths are those a program meets, and the three read them alike. Where they take a path apart their own
ways, nothing is measured; the differences are these:

| Path | SG | Rust | Zig |
|---|---|---|---|
| `a/file.` | no extension | an empty extension, shown `.` | the extension `.` |
| `logs/` | the name empty, the folder `logs` | the name `logs`, the folder empty | the name `logs`, no folder |
| `/` | the folder `/` | no folder | no folder |
| `a/..` | the name `..`, no extension | no name | the name `..`, the extension `.` |
| `a/.profile` | no extension | no extension | no extension |
