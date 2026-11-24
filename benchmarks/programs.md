# Programs against Rust and Zig

Programs that people write, written in SG, in Rust and in Zig, the same algorithm each time and written
plainly. Their code is in [`trials/`](../trials/README.md), which also says how they are written; how they
are measured is in [How it is measured](method.md).

Apple M3 Max, macOS, 20 November 2026; 100 MB of text from [`trials/inputs/text.sg`](../trials/inputs/text.sg) (seed 1). SG is compiled by
the system's `cc` (Apple clang 21), Rust by its own LLVM (23.1), Zig 0.17.1 (LLVM too) with PCRE2 10.49.

## wc

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

## grep

Measured again on 24 November 2026 (slice 174), after the search in any case was made faster. Since slice 173
the three read a line as bytes and search it as bytes, so that a line that is not UTF-8 is written as it is
(choice 1840): SG with `readLineBytes` and `isMatchBytes`, Rust with `read_until` and `regex::bytes`, Zig with
PCRE2's `MATCH_INVALID_UTF`.

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| `-c ERROR` | 0.036 s, 2.0 MB | 0.036 s, 2.0 MB | 0.043 s, 2.8 MB | 0.036 s, 3.4 MB | 0.037 s, 3.6 MB |
| `-n timeout` | 0.044 s, 2.2 MB | 0.044 s, 2.1 MB | 0.046 s, 2.8 MB | 0.045 s, 3.5 MB | 0.047 s, 3.6 MB |
| `-c -i error` | 0.042 s, 2.0 MB | 0.048 s, 2.0 MB | 0.063 s, 3.0 MB | 0.047 s, 3.5 MB | 0.048 s, 3.6 MB |
| `-c '[a-z]+[0-9]+@example\.com (PUT\|DELETE)'` | 0.127 s, 2.1 MB | 0.139 s, 2.1 MB | 0.128 s, 3.1 MB | 0.105 s, 3.4 MB | 0.106 s, 3.6 MB |
| `'in [0-9]{4} ms: .*日本'` | 0.049 s, 2.2 MB | 0.052 s, 2.2 MB | 0.086 s, 3.2 MB | 0.049 s, 3.5 MB | 0.049 s, 3.6 MB |
| `-v -c INFO` | 0.037 s, 2.0 MB | 0.038 s, 2.0 MB | 0.046 s, 2.8 MB | 0.039 s, 3.4 MB | 0.040 s, 3.6 MB |
| Size, stripped | 181 KB | 181 KB | 1684 KB | 362 KB | 442 KB |
| Lines | 104 | 104 | 145 | 146 | 146 |

`-c -i error` took 0.065 s in slice 173 (checked 0.066 s), 1.44 times Zig's. It looked for the first two bytes
of `error` sixteen places at a time, and `er` is found about once a line; it now looks for the two rarest,
`rr`, found only where `error` is, and a last block that overlaps the one before it takes the place of the
places looked at one at a time at the end of each line. Reading the lines is 0.033 s of the 0.042 s. The
other cases moved by a millisecond at most.

Each line is no longer checked to be UTF-8, which each of the three did before searching it. Beside the
SG of the slice before, built from its own source the same day, best of nine: `-c ERROR` 0.044 → 0.034 s,
`-n timeout` 0.052 → 0.044 s, `-c -i error` 0.078 → 0.064 s, the address 0.136 → 0.133 s, `日本` 0.060 →
0.050 s, `-v -c INFO` 0.048 → 0.036 s, the same output. PCRE2's JIT gained most on the address, which it now
searches with no check of the bytes either: Zig is ahead there, 0.104 s against SG's 0.127 s and Rust's
0.129 s. SG's search of a `string`, which now checks the bytes it reads past ASCII, takes what it took: the
old `grep`, built by the compiler of this slice, is within a millisecond of what the old compiler gives.

The table before this slice, of 20 November, with `-i` and the address as the slices after it left them
(measured on 23 November):

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| `-c ERROR` | 0.046 s, 2.1 MB | 0.045 s, 2.1 MB | 0.066 s, 2.8 MB | 0.060 s, 3.4 MB | 0.061 s, 3.6 MB |
| `-n timeout` | 0.051 s, 2.2 MB | 0.050 s, 2.2 MB | 0.070 s, 2.8 MB | 0.069 s, 3.5 MB | 0.072 s, 3.6 MB |
| `-c -i error` | 0.075 s, 2.1 MB | 0.077 s, 2.1 MB | 0.086 s, 3.0 MB | 0.068 s, 3.5 MB | 0.070 s, 3.6 MB |
| `-c '[a-z]+[0-9]+@example\.com (PUT\|DELETE)'` | 0.133 s, 2.2 MB | 0.155 s, 2.2 MB | 0.148 s, 3.1 MB | 0.129 s, 3.5 MB | 0.131 s, 3.6 MB |
| `'in [0-9]{4} ms: .*日本'` | 0.061 s, 2.2 MB | 0.063 s, 2.2 MB | 0.110 s, 3.2 MB | 0.073 s, 3.5 MB | 0.073 s, 3.6 MB |
| `-v -c INFO` | 0.049 s, 2.1 MB | 0.048 s, 2.1 MB | 0.069 s, 2.8 MB | 0.062 s, 3.4 MB | 0.063 s, 3.6 MB |
| Size, stripped | 180 KB | 164 KB | 1684 KB | 362 KB | 442 KB |
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
(1538), and the same for where a match is (1539) brought them to 0.150 s and 0.228 s. Go's `regexp` took 1.13 s and 1.33 s on the two hardest.
In the hundred-and-sixty-fourth slice `-i` looks for `error` sixteen places at a time with the processor's
vector instructions, as Rust's `regex` does, and a pattern that is only a text in any case runs no DFA; the
DFA's loop reads its table without checking the index against the table's end in a release build (1803), which
a checked build still does (0.224 s). In the hundred-and-sixty-fifth the address is searched from its inner
text, `@example.com `, as Rust's `regex` does: what comes before it is read backwards to where a match starts
and the pattern forwards from there, so a line is read around its address and not from its start. It takes
0.133 s, from 0.189 s, ahead of Rust's 0.148 s; PCRE2's JIT, which turns the pattern into machine code, takes
0.129 s. The other cases were measured again in the same run and moved by a millisecond or two at most.

Rust's `\w` and `\d` are Unicode's unless `(?-u)` is written, while SG's are ASCII's as RE2's, and PCRE2
backtracks where SG and Rust never do: the patterns measured mean the same in all three.

## jq

The jq people use (`trials/jq/README.md`, choices 1804-1822), held byte for byte to jq 1.8.2 by 44 cases of
`measure.py jq --check` (every build gives jq's output and exit code), by the two suites of
`trials/jq/tests`, 3,687 cases written with the Rust version and 6,389 with the Zig version, each version
passing both. jq 1.8.2 (Homebrew) and jaq 3.2.0 are
measured beside them; jaq is another algorithm and is not held to jq's output. Apple M3 Max, macOS, 24
November 2026, after slice 171; 100 MB of records, of a record a line and of numbers from
[`trials/inputs/json.sg`](../trials/inputs/json.sg) (seed 1), 1 GB of records, and the inputs that break
things written by `measure.py`; the best of five.

| Case | SG | SG, checked | Rust | Zig | Zig, safe | jq 1.8.2 | jaq 3.2.0 |
|---|---|---|---|---|---|---|---|
| records, written back | 1.019 s, 713.2 MB | 1.119 s, 959.2 MB | 2.170 s, 1114.2 MB | 0.866 s, 557.7 MB | 1.027 s, 608.7 MB | 4.495 s, 1183.0 MB | 1.568 s, 1216.1 MB |
| select and an object | 0.888 s, 713.3 MB | 0.991 s, 959.3 MB | 2.057 s, 1114.2 MB | 0.726 s, 557.9 MB | 0.893 s, 608.8 MB | 1.478 s, 1131.4 MB | 1.062 s, 1216.3 MB |
| group_by | 1.326 s, 753.2 MB | 1.557 s, 1011.6 MB | 2.348 s, 1142.8 MB | 1.018 s, 624.5 MB | 1.237 s, 679.6 MB | 2.137 s, 1155.4 MB | 1.174 s, 1270.5 MB |
| sort_by | 1.435 s, 750.1 MB | 1.506 s, 1008.4 MB | 2.468 s, 1143.2 MB | 1.122 s, 624.0 MB | 1.326 s, 679.2 MB | 1.926 s, 1152.2 MB | 1.210 s, 1241.3 MB |
| csv, raw | 0.931 s, 713.3 MB | 1.036 s, 959.3 MB | 2.143 s, 1114.3 MB | 0.764 s, 557.8 MB | 0.930 s, 608.8 MB | 1.604 s, 1145.9 MB | 1.216 s, 1216.3 MB |
| counted by city | 0.958 s, 713.2 MB | 1.089 s, 959.3 MB | 2.093 s, 1114.2 MB | 0.748 s, 557.8 MB | 0.904 s, 608.8 MB | 1.950 s, 1129.4 MB | 0.865 s, 1220.3 MB |
| every number added | 2.033 s, 859.0 MB | 2.366 s, 1206.1 MB | 2.438 s, 965.8 MB | 0.975 s, 588.9 MB | 1.169 s, 628.2 MB | 6.019 s, 1545.1 MB | 4.563 s, 1043.4 MB |
| a record a line | 0.782 s, 3.8 MB | 0.880 s, 3.8 MB | 0.800 s, 3.8 MB | 0.658 s, 3.1 MB | 0.815 s, 3.2 MB | 1.307 s, 3.3 MB | 0.844 s, 99.7 MB |
| 1 GB written back | 9.711 s, 7080.6 MB | 10.813 s, 9540.0 MB | 19.643 s, 11115.0 MB | 8.293 s, 5536.8 MB | 9.907 s, 6028.8 MB | 33.841 s, 11852.7 MB | 13.394 s, 12125.4 MB |
| ten million lines out | 0.151 s, 2.5 MB | 0.134 s, 2.6 MB | 0.390 s, 3.1 MB | 0.165 s, 2.2 MB | 0.206 s, 2.5 MB | 1.912 s, 3.1 MB | 8.301 s, 4.4 MB |
| a text of 100 MB | 0.304 s, 222.1 MB | 0.334 s, 222.1 MB | 0.130 s, 331.1 MB | 0.217 s, 112.6 MB | 0.246 s, 112.8 MB | 0.701 s, 222.4 MB | 0.106 s, 337.1 MB |
| an object of ten million | 3.512 s, 1068.0 MB | 3.561 s, 1127.6 MB | 3.629 s, 1468.2 MB | 1.857 s, 1447.1 MB | 2.120 s, 1514.6 MB | 3.660 s, 2313.5 MB | 1.307 s, 1477.8 MB |
| ten million sorted | 0.132 s, 157.0 MB | 0.130 s, 157.1 MB | 0.266 s, 310.3 MB | 0.285 s, 309.0 MB | 0.396 s, 385.6 MB | 1.238 s, 1417.5 MB | 0.173 s, 413.2 MB |
| added to, a million times | 0.076 s, 19.7 MB | 0.089 s, 19.8 MB | 0.107 s, 20.4 MB | 0.073 s, 19.2 MB | 0.082 s, 19.4 MB | 0.259 s, 95.2 MB | 0.136 s, 34.8 MB |
| set by name, a million times | 0.555 s, 121.8 MB | 0.614 s, 121.8 MB | 0.377 s, 127.2 MB | 0.312 s, 123.3 MB | 0.408 s, 125.2 MB | 1.024 s, 164.5 MB | 0.437 s, 138.3 MB |
| a def a million deep | 0.164 s, 109.8 MB | 0.194 s, 156.2 MB | 0.087 s, 125.6 MB | 0.085 s, 156.2 MB | 0.100 s, 156.5 MB | 0.255 s, 150.3 MB | fails |
| a def ten million in tail | 0.906 s, 2.3 MB | 1.036 s, 2.4 MB | 0.389 s, 3.0 MB | 0.430 s, 2.3 MB | 0.520 s, 2.6 MB | 1.720 s, 3.1 MB | 1.448 s, 4.4 MB |
| nested ten million deep | 0.704 s, 1073.5 MB | 0.891 s, 1536.6 MB | 1.043 s, 1074.2 MB | 0.700 s, 1847.4 MB | 0.808 s, 2001.4 MB | 1.616 s, 309.6 MB | fails |
| Size, stripped | 488 KB | 537 KB | 2255 KB | 652 KB | 749 KB | — | — |
| Lines | 6394 | 6394 | 5027 | 5517 | 5517 | — | — |

What it shows:

- **SG is ahead of jq 1.8.2 in every case and of Rust in all but four**; Rust's `serde_json` builds a
  `Value` of its own before the program's, where SG's `JsonReader` (1814) and Zig's `Scanner` hand the
  tokens straight to the program. **Zig is ahead of SG in most cases**, by 18 to 30% where records are read
  and written; SG is ahead in ten million lines out and ten million sorted, and holds less memory in the
  object of ten million (1,068 MB against 1,447 MB), in ten million sorted and nested ten million deep.
- **Where SG is behind** (in [Slower than C](slower-than-c.md)): a call of a `def` (0.91 s against 0.43 s
  in tail position, and `set by name`, 0.56 s against 0.31 s); `every number added`, 2.03 s against
  0.98 s, where the rest is the machine of `..` and not the numbers, and 859 MB against 589 MB; the
  memory of the records (713 MB against 558 MB), where SG holds each fraction read with its literal in two
  blocks; and a text of 100 MB, which SG holds twice while it is read (222 MB against Zig's 113 MB).
- **Slices 170 and 171** (choices 1836, 1824, 1825, 1837): a function whose values may take a page of its
  stack touches it only near the bottom of the stack (a `def` ten million deep in tail position 1.23 s →
  0.91 s), and the three read and write numbers one way: a whole number of up to 15 digits is its
  `double`, any other keeps its literal and works out its `double` when wanted, two are compared by their
  `double`s first, and a whole `double` is written as an integer. SG had written every long number into
  text as it read it: every number added 3.05 s → 2.03 s, ten million lines out 0.66 s → 0.15 s (Zig
  0.46 s → 0.17 s). The fractions of the records are literals now, so SG holds 713 MB where it held 614 MB
  and its `sort_by` takes 1.44 s where it took 1.23 s; Zig's whole numbers no longer take a block (558 MB
  where 588 MB), and its `sort_by` compares `double`s (1.12 s where 1.43 s).
- **Found and fixed in the slice, from these cases**: a `shared` value was copied whole at each change
  (`makeUnique`, 1813: `added to, a million times` did not end in 300 s); `sort` merged pairs of one item,
  then two, whatever the order (it now merges the runs the items make: ten million sorted 1.16 s → 0.11 s);
  a value of SG's `jq` was 32 bytes where Rust's and Zig's are 16 (1 GB written back: 7.1 GB → 6.1 GB);
  `[f]` kept the room it grew by (nested ten million deep: 2.1 GB → 1.1 GB); `..` was jq's own `def` (every
  number added: 4.6 s → 3.0 s; Rust and Zig walk it with a stack of their own, and so does SG now).
- **jaq** fails where a value is nested ten million deep and where a `def` is a million deep (its stack
  overflows), writes another output or exit code in 9 of the 44 cases of the check (bad UTF-8, a byte order
  mark, numbers past a `double`, an error in the middle, 10,001 levels), and panics before it starts when a
  variable of the environment is not UTF-8 (`measure.py` sets one for another trial; jaq runs without it).
- The sizes are stripped; the lines are the trial's own, without comments. SG's version is the longest: it
  writes its own reader of numbers (decNumber's rules) and Perl's `$` for `test`, which Rust and Zig take
  from their libraries.

## kv

A store of keys and values with a log written before each change (`trials/kv/README.md`, choices
1844-1853). The three write the same log byte for byte; 16 cases of `measure.py kv --check` hold their output,
exit code and log to each other, and `trials/kv/tests/check.py` holds them to a model written in Python on
streams drawn at random, logs cut at every byte and with bits turned, and the store killed in the middle of its
sets (every set answered `OK` there after it starts again). Apple M3 Max, macOS, 24 November 2026, slice 175,
after the slots of a table kept 32 bits of each key's hash. A key is 16 bytes and a value 100; each run starts
in a folder made anew, empty or with a log Python wrote. The case of `always` is measured once after its
warming run, being the disk's time: a sync with `F_FULLFSYNC` takes about 4 ms on this machine.

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| A million sets, never synced | 0.240 s, 196.7 MB | 0.241 s, 196.7 MB | 0.317 s, 321.2 MB | 0.382 s, 216.7 MB | 0.446 s, 216.9 MB |
| A million sets, synced each block | 8.492 s, 197.2 MB | 8.556 s, 196.6 MB | 8.707 s, 321.2 MB | 9.085 s, 216.7 MB | 8.923 s, 216.9 MB |
| Ten thousand sets, each synced (once) | 41.938 s, 4.2 MB | 40.828 s, 4.5 MB | 40.445 s, 4.9 MB | 40.189 s, 4.4 MB | 40.232 s, 4.7 MB |
| A million gets, sets and deletes over 100,000 keys | 4.484 s, 35.4 MB | 4.612 s, 37.6 MB | 4.612 s, 24.6 MB | 4.760 s, 20.2 MB | 4.693 s, 20.4 MB |
| A value of 100 MB, set and read back | 0.097 s, 304.1 MB | 0.099 s, 304.1 MB | 0.099 s, 304.2 MB | 0.335 s, 302.2 MB | 0.355 s, 402.2 MB |
| A log of 1 GB read back | 2.697 s, 194.0 MB | 2.718 s, 194.0 MB | 2.374 s, 315.1 MB | 3.237 s, 216.6 MB | 3.355 s, 216.8 MB |
| Ten million keys read and compacted | 4.285 s, 1852.5 MB | 4.199 s, 1937.2 MB | 5.361 s, 1876.9 MB | 12.078 s, 1064.6 MB | 10.258 s, 1064.9 MB |
| Size, stripped | 100 KB | 116 KB | 385 KB | 394 KB | 474 KB |
| Lines | 369 | 369 | 367 | 348 | 348 |

- Three cases are the disk's: a sync of each block of the input, and of each set, take the same time in the
  three within the noise of the disk.
- SG holds less than Rust where the map is large: a key and a value are each a list of their own (24 bytes and
  the bytes), where Rust's `Vec` takes the same and its table keeps a whole hash; Zig keeps a key as a slice
  and its table is the smallest. Over 100,000 keys changing all the time SG holds the most, 35.4 MB against
  24.6 MB, in [Slower than C](slower-than-c.md).
- The log of 1 GB is read in 2.70 s against Rust's 2.37 s: the map's work, the reading and the CRC-32 taking
  0.3 s of it. Measured the same day before the table kept its hashes, SG took 2.67 s against Rust's 2.16 s
  (a quieter run) and the compaction of ten million keys 6.53 s against 4.82 s: the search compared the key
  of each entry it passed, and growing hashed every key again. In [Slower than C](slower-than-c.md).
- A compaction sorts the keys by their bytes. SG moves the entries out of the map to sort them and puts them
  back, since a list is not `Comparable`; Rust and Zig sort references to them. Zig's map grows by hashing
  each key again, which the compaction of ten million keys shows.
- The checked builds differ by the noise of the disk; with nothing on the disk, by 2% or less either way.

## hashsum

The SHA-256 of each file from a pool of workers, written in the order given, byte for byte as GNU's `sha256sum`
writes it (`trials/hashsum/README.md`, choices 1854-1860). 22 cases of `measure.py hashsum --check` hold the
output and exit code of the five builds to each other, and `trials/hashsum/tests/check.py` holds them to a model
of GNU's `sha256sum`, itself held to GNU on Linux. Apple M3 Max (16 processors), macOS, 24 November 2026, slice
176. Each worker reads a file in blocks of 64 KB; the files are random bytes Python made, read from the
system's cache after the warming run; without `-j` each program runs as many workers as there are processors.
The system's `sha256sum`, which hashes one file after another, is measured beside them for reference; it takes
no `-j`.

| Case | SG | SG, checked | Rust | Zig | Zig, safe | sha256sum |
|---|---|---|---|---|---|---|
| 20,000 files of 4 KB | 0.310 s, 4.4 MB | 0.304 s, 4.5 MB | 0.290 s, 4.5 MB | 0.302 s, 6.1 MB | 0.304 s, 7.0 MB | 0.385 s, 2.1 MB |
| 16 files of 64 MB | 0.049 s, 3.3 MB | 0.055 s, 3.3 MB | 0.052 s, 3.5 MB | 0.048 s, 3.9 MB | 0.048 s, 4.0 MB | 0.569 s, 1.8 MB |
| A file of 1 GB | 0.527 s, 2.3 MB | 0.528 s, 2.3 MB | 0.515 s, 2.6 MB | 0.463 s, 2.5 MB | 0.467 s, 3.6 MB | 0.569 s, 1.8 MB |
| 2,000 files of 1 KB to 16 MB | 0.322 s, 3.7 MB | 0.322 s, 3.7 MB | 0.315 s, 3.8 MB | 0.288 s, 4.3 MB | 0.289 s, 4.5 MB | 1.767 s, 1.9 MB |
| 16 files of 64 MB, `-j 1` | 0.529 s, 2.0 MB | 0.526 s, 2.0 MB | 0.516 s, 2.2 MB | 0.465 s, 2.2 MB | 0.467 s, 2.4 MB | — |
| 16 files of 64 MB, `-j 2` | 0.268 s, 2.1 MB | 0.269 s, 2.1 MB | 0.262 s, 2.3 MB | 0.239 s, 2.3 MB | 0.238 s, 2.5 MB | — |
| 16 files of 64 MB, `-j 4` | 0.138 s, 2.3 MB | 0.139 s, 2.2 MB | 0.135 s, 2.5 MB | 0.122 s, 2.5 MB | 0.123 s, 2.7 MB | — |
| 16 files of 64 MB, `-j 8` | 0.072 s, 2.7 MB | 0.073 s, 2.6 MB | 0.071 s, 2.8 MB | 0.065 s, 3.0 MB | 0.065 s, 3.1 MB | — |
| 16 files of 64 MB, `-j 16` | 0.052 s, 3.3 MB | 0.050 s, 3.3 MB | 0.051 s, 3.5 MB | 0.045 s, 3.9 MB | 0.046 s, 4.0 MB | — |
| 2,000 files of 1 KB to 16 MB, `-j 1` | 1.641 s, 2.4 MB | 1.645 s, 2.4 MB | 1.602 s, 2.5 MB | 1.450 s, 2.7 MB | 1.462 s, 2.8 MB | — |
| 2,000 files of 1 KB to 16 MB, `-j 2` | 1.096 s, 2.5 MB | 1.102 s, 2.5 MB | 1.075 s, 2.6 MB | 0.967 s, 2.8 MB | 0.975 s, 2.9 MB | — |
| 2,000 files of 1 KB to 16 MB, `-j 4` | 0.740 s, 2.7 MB | 0.741 s, 2.7 MB | 0.722 s, 2.8 MB | 0.652 s, 3.0 MB | 0.655 s, 3.1 MB | — |
| 2,000 files of 1 KB to 16 MB, `-j 8` | 0.494 s, 3.1 MB | 0.497 s, 3.0 MB | 0.489 s, 3.1 MB | 0.442 s, 3.5 MB | 0.446 s, 3.6 MB | — |
| 2,000 files of 1 KB to 16 MB, `-j 16` | 0.320 s, 3.7 MB | 0.321 s, 3.8 MB | 0.315 s, 3.9 MB | 0.287 s, 4.3 MB | 0.289 s, 4.5 MB | — |
| Size, stripped | 85 KB | 85 KB | 436 KB | 361 KB | 458 KB | — |
| Lines | 185 | 185 | 180 | 160 | 160 | — |

- The pools scale alike: from one worker to sixteen, SG's time on 16 files of 64 MB falls 10.2 times, Rust's
  10.1, Zig's 10.3. On 20,000 small files the three are within 7%: what a job costs is the opening of a file.
- What is left between them is the speed of SHA-256 on one processor: a file of 1 GB takes SG 0.527 s, Rust
  0.515 s, Zig 0.463 s, all three with the processor's SHA instructions. SG's way to them is in
  [Slower than C](slower-than-c.md) (SHA-256 one call at a time).
- SG holds the least memory with a worker or more at work: a worker's reads go straight into its own block. Before
  that, measured the same day, a `readSome` of a file read through the reader's block of 64 KB and copied out:
  sixteen workers held 4.9 MB and the mixed files took 0.330 s.
- The first SG version made the channel's sender after the workers had their receivers; the channel had ended
  for those that started first, and one worker of sixteen hashed every file: 16 files of 64 MB took 0.548 s.
  Such a program is now an error of the compiler (choice 1860).
- Rust is measured with `sha2` 0.11, which uses the processor's SHA instructions on this machine; 0.10 does not,
  and took 2.37 s for the file of 1 GB.
