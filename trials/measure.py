#!/usr/bin/env python3
"""Builds each trial in SG, Rust and Zig, checks that the three give the same output, and measures
them (choices 1418, 1525-1532, 1540-1542). The modules of `std` are measured the same way, each in
`trials/std/<module>` (choices 1553, 1629-1637). Run from the repository's root:

    python3 trials/measure.py               every trial: checked, then measured
    python3 trials/measure.py grep          only this one
    python3 trials/measure.py std/json      only the module of `std`
    python3 trials/measure.py std/compression std/zip
    python3 trials/measure.py std/hash std/checksum
    python3 trials/measure.py std/encoding std/unicode
    python3 trials/measure.py std/time std/random std/log
    python3 trials/measure.py std/net
    python3 trials/measure.py std/command_line std/environment std/path
    python3 trials/measure.py --check       only checked, nothing measured (what Linux runs)
    python3 trials/measure.py --runs 1      one run of each instead of the best of five

For each case the standard output and the exit code of the three must be the same, byte for byte;
the standard error is not compared, since each language words its errors its own way. Where the
programs write what they may each write their own way (compressed data, an archive), the case
names a check that opens what each wrote into `{out}` with Python's `gzip` or `zipfile` and
holds it to the input, and the size each wrote is shown under the table (1634, 1642, 1643). What
must come out the same everywhere but is too large for the standard output (Base64, normal forms
of Unicode) is written into `{out}` too and held to what Python's `base64` and `unicodedata` make
of the input (1660, 1662). What a log writes on the standard error is held to its form, each line a
time to the millisecond, the level and the message, and the messages, in order of their text, must
be the same everywhere (1670). Every program runs with `TZ=Europe/Istanbul` and without `SG_LOG` or
`RUST_LOG` (1668, 1670). The addresses `std/net` writes into `{out}` are held to what Python's
`ipaddress` writes of each line of the input (1681). `kv` runs in a folder made anew before each run, empty or
with a log Python wrote (`{kvdir}`), and the log it leaves is held to be the same, byte for byte, in every
build (1845); a case marked `once` is measured once after its warming run, being the disk's time (1849). A help each library words its own way is held
only to its exit code (1798). `std/environment` reads `SG_TRIAL_TEXT` and `SG_TRIAL_BYTES` (not
UTF-8), which every program is given, and `SG_TRIAL_MISSING`, which none is (1799).
Measured:
the time (one run to warm up, then the best of five), the most memory held (the peak resident set,
from `wait4`), the size of the program once stripped, and its lines that are neither blank nor
only a comment; for the command line and the environment the time is not shown (1553, 1801). SG is built with `sg build --release`, and once more with its arithmetic checked:
the C of `sg emit-c` compiled with `-O2` as `--release` compiles it (`sg build` alone compiles with
`-O0`, for a debugger); Rust with `cargo build --release`; Zig twice, `ReleaseFast` and `ReleaseSafe`
(which keeps its checks of bounds and overflow), with `zig build` where the trial has a `build.zig` (a
library of C to link) and `zig build-exe` where it has not. A trial may build Rust once more with a
feature people turn on (`flate2` with `zlib-rs`, 1632). Everything built and every input goes to
`trials/target`. The measurements follow the rules of fairness in `benchmarks/method.md` (1554): every case
is reported, also those SG loses. A run that has not ended after `HUNG_SECONDS` stops the measuring:
what its threads were doing (`sample` on macOS) and the files it held open (`lsof`) are written to
`trials/target/hung-PID.txt` before it is killed (1683). A measured run that exits with a code other
than 0 stops it too, so that a program that gave up early is never counted fast (1685).
"""

import argparse
import functools
import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TRIALS = ROOT / "trials"
OUT = TRIALS / "target"
SG = ROOT / "interpreter" / "target" / "release" / "sg"
MACOS = sys.platform == "darwin"
# Far longer than any case takes; a run still going then waits for something that never comes.
HUNG_SECONDS = 300

# The text of `inputs/text.sg`: megabytes and seed.
TEXT_MB = 100
TEXT_SEED = 1


def text_input():
    return OUT / f"text-{TEXT_MB}.log"


def small_input():
    return OUT / "text-2.log"


# The inputs made by `inputs/json.sg`: placeholder -> (kind, megabytes, file name). Made the first
# time a case names them.
JSON_INPUTS = {
    "records": ("records", 100, "records-100.json"),
    "lines": ("lines", 100, "lines-100.ndjson"),
    "numbers": ("numbers", 100, "numbers-100.json"),
    "small-records": ("records", 2, "records-2.json"),
    "small-lines": ("lines", 2, "lines-2.ndjson"),
    "small-numbers": ("numbers", 2, "numbers-2.json"),
    "records-1g": ("records", 1000, "records-1000.json"),
}
JSON_SEED = 1


# The other inputs, each made the first time a case names it: placeholder -> file name. The
# compressed ones are made by Python's `gzip` and `zipfile` (zlib at level 6), which none of the
# programs measured is.
OTHER_INPUTS = {
    "random": "random-100.bin",
    "text-gz": "text-100.log.gz",
    "small-gz": "text-2.log.gz",
    "records-lines": "lines-37.ndjson",
    "parts": "parts-100.zip",
    "small-parts": "parts-2.zip",
    "many": "many-100000.zip",
    "small-many": "many-1000.zip",
    "small-random": "random-2.bin",
    "random-b64": "random-100.b64",
    "small-random-b64": "random-2.b64",
    "unicode": "unicode-100.txt",
    "small-unicode": "unicode-2.txt",
    "unicode-nfd": "unicode-100-nfd.txt",
    "small-unicode-nfd": "unicode-2-nfd.txt",
    "time": "time-100.txt",
    "small-time": "time-2.txt",
    "addresses": "addresses-10000000.txt",
    "small-addresses": "addresses-1000.txt",
    # The hostile inputs of `jq` (1808), made here by Python.
    "jq-string": "jq-string-100.json",
    "jq-object": "jq-object-10000000.json",
    "jq-deep": "jq-deep-10000.json",
    "jq-deeper": "jq-deep-10001.json",
    "jq-deep-objects": "jq-deep-objects-5000.json",
    "jq-bad-utf8": "jq-bad-utf8.json",
    "jq-surrogate": "jq-surrogate.json",
    "jq-nul": "jq-nul.json",
    "jq-bom": "jq-bom.json",
    "jq-crlf": "jq-crlf.ndjson",
    "jq-numbers": "jq-numbers.json",
    "jq-cut": "records-1000-cut.json",
    "jq-midstream": "jq-midstream.ndjson",
    "jq-quirks": "jq-quirks.txt",
}
UNICODE_SEED = 1
TIME_SEED = 1
ADDRESS_SEED = 1
DOS_TIME = (2026, 11, 20, 13, 45, 30)


# The inputs of `kv` (choices 1844-1849), made here by Python from a seed: placeholder -> file name. A
# key is 16 bytes and a value 100, as the cases say; a log is written by Python's `zlib.crc32` and
# `struct`, which none of the programs measured is.
KV_INPUTS = {
    "kv-sets": "kv-sets-1000000.txt",
    "kv-sets-10k": "kv-sets-10000.txt",
    "kv-mixed": "kv-mixed-1000000.txt",
    "kv-big": "kv-big-100.txt",
    "kv-gets": "kv-gets-1000.txt",
    "kv-compact": "kv-compact.txt",
    "kv-commands": "kv-commands.txt",
    "kv-log-1g": "kv-log-1000.bin",
    "kv-log-10m": "kv-log-10000000.bin",
    "kv-log-small": "kv-log-small.bin",
    "kv-log-cut": "kv-log-cut.bin",
    "kv-log-flipped": "kv-log-flipped.bin",
    "kv-log-zeros": "kv-log-zeros.bin",
    "kv-log-empty": "kv-log-empty.bin",
    "kv-small-gets": "kv-small-gets.txt",
}
KV_SEED = 1


def kv_record(op, key, value):
    import struct
    import zlib
    body = struct.pack("<BII", op, len(key), len(value)) + key + value
    return struct.pack("<I", zlib.crc32(body)) + body


def kv_value(rng, pool, length=100):
    at = rng.randrange(len(pool) - length)
    return pool[at:at + length]


def kv_pool(rng):
    """A megabyte of printable bytes, spaces among them, that values are cut from."""
    letters = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 -_.:/"
    return bytes(letters[rng.randrange(len(letters))] for _ in range(1 << 20))


def kv_input(placeholder):
    import random
    path = OUT / "kv-inputs" / KV_INPUTS[placeholder]
    if path.exists():
        return path
    path.parent.mkdir(parents=True, exist_ok=True)
    rng = random.Random(f"{KV_SEED}-{placeholder}")
    pool = kv_pool(rng)
    key = lambda n: b"key:%012d" % n
    partial = path.with_name(path.name + ".part")
    with open(partial, "wb") as f:
        if placeholder in ("kv-sets", "kv-sets-10k"):
            count = 1_000_000 if placeholder == "kv-sets" else 10_000
            order = list(range(count))
            rng.shuffle(order)
            for n in order:
                f.write(b"SET " + key(n) + b" " + kv_value(rng, pool) + b"\n")
        elif placeholder == "kv-mixed":
            # A million commands over 100,000 keys: half `GET`, two fifths `SET`, a tenth `DEL`.
            for _ in range(1_000_000):
                r, n = rng.random(), rng.randrange(100_000)
                if r < 0.5:
                    f.write(b"GET " + key(n) + b"\n")
                elif r < 0.9:
                    f.write(b"SET " + key(n) + b" " + kv_value(rng, pool) + b"\n")
                else:
                    f.write(b"DEL " + key(n) + b"\n")
        elif placeholder == "kv-big":
            # A value of 100 MB, read back.
            f.write(b"SET big ")
            for _ in range(100):
                f.write(pool)
            f.write(b"\nGET big\n")
        elif placeholder in ("kv-gets", "kv-small-gets"):
            count = 1000 if placeholder == "kv-gets" else 200
            for _ in range(count):
                f.write(b"GET " + key(rng.randrange(1_000_000 if placeholder == "kv-gets" else 300)) + b"\n")
        elif placeholder == "kv-compact":
            f.write(b"COMPACT\n")
            for _ in range(1000):
                f.write(b"GET " + key(rng.randrange(10_000_000)) + b"\n")
        elif placeholder == "kv-commands":
            # What the commands are at their edges: an empty key and value, spaces in a value, bytes
            # that are not UTF-8, a CR before the line's end, lines that are not commands, a last
            # line without its end, and enough writes over few keys that the log is compacted.
            lines = [b"SET a 1", b"SET b two words", b"GET a", b"GET b", b"GET c", b"DEL a", b"DEL a",
                     b"GET a", b"SET  empty key", b"GET ", b"SET k ", b"GET k", b"SET \xff\xfe bytes \xc3\x28",
                     b"GET \xff\xfe", b"SET cr value\r", b"GET cr", b"FOO", b"SET x", b"set a 1", b"", b"GET",
                     b"DEL", b"COMPACT", b"GET b", b"COMPACT x", b" SET a 1", b"SETa 1"]
            for n in range(20_000):
                lines.append(b"SET hot%d " % (n % 7) + kv_value(rng, pool))
            lines += [b"GET hot%d" % n for n in range(8)]
            lines += [b"DEL hot%d" % n for n in range(3)]
            lines.append(b"GET b")
            f.write(b"\n".join(lines))
        elif placeholder in ("kv-log-1g", "kv-log-10m", "kv-log-small", "kv-log-cut", "kv-log-flipped", "kv-log-zeros"):
            if placeholder == "kv-log-1g":
                # About 1 GB: sets over a million keys, a twentieth deletes.
                written = 0
                while written < 1_000_000_000:
                    n = rng.randrange(1_000_000)
                    record = kv_record(2, key(n), b"") if rng.random() < 0.05 else kv_record(1, key(n), kv_value(rng, pool))
                    f.write(record)
                    written += len(record)
            elif placeholder == "kv-log-10m":
                # Ten million keys with values of 8 bytes, then two million of them written again.
                for n in range(10_000_000):
                    f.write(kv_record(1, key(n), b"%08d" % n))
                for _ in range(2_000_000):
                    n = rng.randrange(10_000_000)
                    f.write(kv_record(1, key(n), b"again%03d" % rng.randrange(1000)))
            else:
                records = [kv_record(1, key(rng.randrange(300)), kv_value(rng, pool, rng.randrange(0, 200)))
                           if rng.random() < 0.9 else kv_record(2, key(rng.randrange(300)), b"") for _ in range(2000)]
                data = b"".join(records)
                if placeholder == "kv-log-cut":
                    # Cut in the middle of its last record, as a crash in a write leaves it.
                    data = data[:len(data) - len(records[-1]) // 2]
                elif placeholder == "kv-log-flipped":
                    # A bit of a value turned in the middle: the store must not start.
                    middle = sum(len(r) for r in records[:1000]) + 20
                    data = data[:middle] + bytes([data[middle] ^ 0x10]) + data[middle + 1:]
                elif placeholder == "kv-log-zeros":
                    # What a file system may leave after a crash: the length grown, the bytes zeros.
                    data += bytes(4096)
                f.write(data)
    partial.rename(path)
    return path


def zip_of_parts(text):
    """The archive `std/zip` writes: the text in parts of 10,000 bytes."""
    return [(f"logs/part-{n:05d}.txt", text[at:at + 10000]) for n, at in enumerate(range(0, len(text), 10000))]


def write_zip(path, entries):
    import zipfile
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for name, data in entries:
            info = zipfile.ZipInfo(name, DOS_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, data, compresslevel=6)


def other_input(placeholder):
    import gzip
    path = OUT / OTHER_INPUTS[placeholder]
    if path.exists():
        return path
    if placeholder in ("random", "small-random"):
        maker = OUT / "bytes-input"
        if not maker.exists():
            run([SG, "build", TRIALS / "inputs" / "bytes.sg", "--release", "-o", maker])
        with open(path, "wb") as f:
            subprocess.run([maker, "100" if placeholder == "random" else "2", "1"], stdout=f, check=True)
    elif placeholder in ("random-b64", "small-random-b64"):
        import base64
        source = other_input(placeholder.removesuffix("-b64"))
        path.write_bytes(base64.b64encode(source.read_bytes()))
    elif placeholder in ("unicode", "small-unicode"):
        maker = OUT / "unicode-input"
        if not maker.exists():
            run([SG, "build", TRIALS / "inputs" / "unicode.sg", "--release", "-o", maker])
        with open(path, "wb") as f:
            megabytes = "100" if placeholder == "unicode" else "2"
            subprocess.run([maker, megabytes, str(UNICODE_SEED)], stdout=f, check=True)
    elif placeholder in ("time", "small-time"):
        maker = OUT / "time-input"
        if not maker.exists():
            run([SG, "build", TRIALS / "inputs" / "time.sg", "--release", "-o", maker])
        with open(path, "wb") as f:
            megabytes = "100" if placeholder == "time" else "2"
            subprocess.run([maker, megabytes, str(TIME_SEED)], stdout=f, check=True)
    elif placeholder in ("addresses", "small-addresses"):
        maker = OUT / "address-input"
        if not maker.exists():
            run([SG, "build", TRIALS / "inputs" / "address.sg", "--release", "-o", maker])
        with open(path, "wb") as f:
            count = "10000000" if placeholder == "addresses" else "1000"
            subprocess.run([maker, count, str(ADDRESS_SEED)], stdout=f, check=True)
    elif placeholder.startswith("jq-"):
        path.write_bytes(jq_input(placeholder))
    elif placeholder in ("unicode-nfd", "small-unicode-nfd"):
        import unicodedata
        source = other_input(placeholder.removesuffix("-nfd"))
        path.write_bytes(unicodedata.normalize("NFD", source.read_text(encoding="utf-8")).encode())
    elif placeholder in ("text-gz", "small-gz"):
        source = text_input() if placeholder == "text-gz" else small_input()
        path.write_bytes(gzip.compress(source.read_bytes(), compresslevel=6, mtime=0))
    elif placeholder == "records-lines":
        json_input("lines")
        with open(path, "wb") as f:
            subprocess.run([OUT / "json-input", "lines", "37", str(JSON_SEED)], stdout=f, check=True)
    elif placeholder in ("parts", "small-parts"):
        source = text_input() if placeholder == "parts" else small_input()
        write_zip(path, zip_of_parts(source.read_bytes()))
    else:
        count = 100000 if placeholder == "many" else 1000
        write_zip(path, [(f"dir-{i // 1000:03d}/file-{i:06d}.txt", f"entry {i}\n".encode()) for i in range(count)])
    return path


def jq_input(placeholder):
    """The bytes of a hostile input of `jq` (choice 1808)."""
    if placeholder == "jq-string":
        # One text of 100 MB, with letters past ASCII and escapes along it.
        body = ("lorem ipsum dolor sit amet, café 日本 " * 3000000)[:100_000_000]
        return b'"' + body.encode() + b' \\"end\\" \\u00e9"' + b"\n"
    if placeholder == "jq-object":
        return b"{" + b",".join(b'"k%d":%d' % (i, i) for i in range(10_000_000)) + b"}\n"
    if placeholder == "jq-deep":
        return b"[" * 10000 + b"1" + b"]" * 10000 + b"\n"
    if placeholder == "jq-deeper":
        return b"[" * 10001 + b"1" + b"]" * 10001 + b"\n"
    if placeholder == "jq-deep-objects":
        return b'{"a":' * 5000 + b"1" + b"}" * 5000 + b"\n"
    if placeholder == "jq-bad-utf8":
        # Each kind of bad UTF-8 inside a text, and around escapes.
        kinds = [b"a\xc3b", b"\xe2\x82c", b"\xf0\x9f\x98d", b"\xc0\xafe", b"\xed\xa0\x80f", b"\xff",
                 b"a\xe2\x82", b"\xf4\x90\x80\x80", b"\xe0\x80\x80", b"\xe4\\n", b"ok\xe4", b"\xe4\xb8\xad"]
        return b"\n".join(b'"' + k + b'"' for k in kinds) + b"\n"
    if placeholder == "jq-surrogate":
        return b'"a" "\\ud800" "b"\n'
    if placeholder == "jq-nul":
        return b'"a\\u0000b" {"\\u0000":0}\n'
    if placeholder == "jq-bom":
        return b"\xef\xbb\xbf" + b'{"a":1}\n'
    if placeholder == "jq-crlf":
        return b'{"a":1}\r\n{"a":2}\r\n[3]\r\n'
    if placeholder == "jq-numbers":
        return (b"[1e1000, -1e1000, -0, 0.0, 1e-400, 18446744073709551617, 100000000000000000001, "
                b"123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789, "
                b"0.1, 0.10000000000000000001, 1.5e3, 1.10, 2.0, 1e999999999, 1e-999999999999, 128218505994187403]\n")
    if placeholder == "jq-cut":
        source = json_input("records-1g").read_bytes()
        # Cut inside the last record, past its first member.
        return source[:len(source) - 200]
    if placeholder == "jq-midstream":
        return b'{"a":1}\n{"a":2}\n{"a":"three"}\n{"a":4}\n'
    if placeholder == "jq-quirks":
        return b"01 .5 nan\n"
    raise KeyError(placeholder)


def json_input(placeholder):
    kind, megabytes, name = JSON_INPUTS[placeholder]
    path = OUT / name
    if not path.exists():
        maker = OUT / "json-input"
        if not maker.exists():
            run([SG, "build", TRIALS / "inputs" / "json.sg", "--release", "-o", maker])
        with open(path, "wb") as f:
            subprocess.run([maker, kind, str(megabytes), str(JSON_SEED)], stdout=f, check=True)
    return path


# Each trial: the cases that are only checked (small, or the ways it fails), and those that are
# also measured. A case is (name, arguments, file given as the input or None). `{text}` and
# `{small}` stand for the inputs.
TRIALS_CASES = {
    "wc": {
        "check": [
            ("two files and one missing", ["{small}", "missing.txt", "{readme}"], None),
            ("the input", [], "{small}"),
            ("an empty file", ["{empty}"], None),
        ],
        "measure": [
            ("100 MB", ["{text}"], None),
            ("100 MB from the input", [], "{text}"),
        ],
    },
    "grep": {
        "check": [
            ("names and numbers", ["-n", "café", "{small}", "{readme}"], None),
            ("a missing file", ["ERROR", "{small}", "missing.txt"], None),
            ("a bad pattern", ["a(", "{small}"], None),
            ("nothing found", ["no such words here", "{small}"], None),
            ("the input, inverted", ["-v", "-c", "INFO", ], "{small}"),
            ("a bad option", ["-x", "a"], None),
        ],
        "measure": [
            ("a word, counted", ["-c", "ERROR", "{text}"], None),
            ("a word, printed with numbers", ["-n", "timeout", "{text}"], None),
            ("ignoring case", ["-c", "-i", "error"], "{text}"),
            ("an address and a method", ["-c", r"[a-z]+[0-9]+@example\.com (PUT|DELETE)", "{text}"], None),
            ("a range and Unicode", ["in [0-9]{4} ms: .*日本", "{text}"], None),
            ("inverted", ["-v", "-c", "INFO", "{text}"], None),
        ],
    },
    "jq": {
        "check": [
            ("records, compact", ["-c", ".[0:3]", "{small-records}"], None),
            ("select and an object", ["-c", ".[] | select(.active and .score > 5000) | {id, name, city: .address.city}", "{small-records}"], None),
            ("group_by", ["-c", "group_by(.address.city) | map({city: .[0].address.city, n: length, avg: (map(.score) | add / length)})", "{small-records}"], None),
            ("csv, raw", ["-r", ".[] | [.id, .name, .score] | @csv", "{small-records}"], None),
            ("a record a line", ["-c", "select((.tags | length) > 2) | .tags", "{small-lines}"], None),
            ("the input, slurped", ["-c", "-s", "map(.id) | length"], "{small-lines}"),
            ("inputs", ["-n", "-c", "reduce inputs as $r (0; . + $r.id)"], "{small-lines}"),
            ("arguments", ["-c", "--arg", "x", "late", "--argjson", "y", "[1]", "[$x, $y]"], "{jq-crlf}"),
            ("indent and sorted names", ["--indent", "3", "-S", ".[0]", "{small-records}"], None),
            ("tab", ["--tab", ".[1].address", "{small-records}"], None),
            ("-e", ["-e", ".[0].note"], "{small-records}"),
            ("a missing file", [".", "{jq-crlf}", "missing.json"], None),
            ("a filter that does not read", [".[", "{jq-crlf}"], None),
            ("an error at a value", [".a", "{jq-numbers}"], None),
            ("an error in the middle", ["-c", ".a + 1", "{jq-midstream}"], None),
            ("10,000 levels", ["-c", "[paths] | length", "{jq-deep}"], None),
            ("10,001 levels", ["-c", "length", "{jq-deeper}"], None),
            ("5,000 objects deep", ["-c", "tojson | length", "{jq-deep-objects}"], None),
            ("bad UTF-8", ["-c", "., length"], "{jq-bad-utf8}"),
            ("a high surrogate alone", ["-c", "."], "{jq-surrogate}"),
            ("a zero character", ["-c", ".", "{jq-nul}"], None),
            ("a byte order mark", ["-c", ".", "{jq-bom}"], None),
            ("lines ended with CR LF", ["-c", ".", "{jq-crlf}"], None),
            ("numbers past a double", ["-c", ".[], (map(. + 0)), sort, (.[0] < .[1])", "{jq-numbers}"], None),
            ("jq's quirks, refused", ["-c", "."], "{jq-quirks}", "quirk"),
            ("deep comparison", ["-n", "-c", "reduce range(10001) as $i (0; [.]) | [. == ., ([.] | [sort?] | length)]"], None),
        ],
        "measure": [
            ("records, written back", [".", "{records}"], None),
            ("select and an object", ["-c", ".[] | select(.active and .score > 5000) | {id, name, city: .address.city}", "{records}"], None),
            ("group_by", ["-c", "group_by(.address.city) | map({city: .[0].address.city, n: length, avg: (map(.score) | add / length)})", "{records}"], None),
            ("sort_by", ["-c", "sort_by(.balance) | .[0].id, .[-1].id", "{records}"], None),
            ("csv, raw", ["-r", ".[] | [.id, .name, .score] | @csv", "{records}"], None),
            ("counted by city", ["-c", "reduce .[] as $r ({}; .[$r.address.city] += 1)", "{records}"], None),
            ("every number added", ["-c", "[.. | numbers] | add", "{numbers}"], None),
            ("a record a line", ["-c", "select((.tags | length) > 2) | .tags", "{lines}"], None),
            ("1 GB written back", ["-c", ".", "{records-1g}"], None),
            ("ten million lines out", ["-n", "-c", "range(10000000)"], None),
            ("a text of 100 MB", ["-c", "length", "{jq-string}"], None),
            ("an object of ten million", ["-c", "length", "{jq-object}"], None),
            ("ten million sorted", ["-n", "-c", "[range(10000000)] | sort | length"], None),
            ("added to, a million times", ["-n", "-c", "reduce range(1000000) as $i ([]; . + [$i]) | length"], None),
            ("set by name, a million times", ["-n", "-c", "reduce range(1000000) as $i ({}; .[$i | tostring] = $i) | length"], None),
            ("a def a million deep", ["-n", "-c", "def f: if . < 1000000 then (. + 1 | f) + 0 else . end; 0 | f"], None),
            ("a def ten million in tail", ["-n", "-c", "def f: if . < 10000000 then . + 1 | f else . end; 0 | f"], None),
            ("nested ten million deep", ["-n", "-c", "reduce range(10000000) as $i (0; [.]) | length"], None),
        ],
    },
    # Each case runs in `{kvdir}`, made anew: `kv fresh` empty, `kv log:NAME` with the log of `{NAME}`.
    "kv": {
        "check": [
            ("the commands at their edges", ["--sync", "never", "{kvdir}"], "{kv-commands}", "kv", "fresh"),
            ("a log read back", ["{kvdir}"], "{kv-small-gets}", "kv", "log:kv-log-small"),
            ("a log cut in its last record", ["{kvdir}"], "{kv-small-gets}", "kv", "log:kv-log-cut"),
            ("a bit turned in the middle of the log", ["{kvdir}"], "{kv-small-gets}", "kv", "log:kv-log-flipped"),
            ("zeros after the last record", ["{kvdir}"], "{kv-small-gets}", "kv", "log:kv-log-zeros"),
            ("an empty log", ["{kvdir}"], "{kv-small-gets}", "kv", "log:kv-log-empty"),
            ("a log.new left by a compaction", ["{kvdir}"], "{kv-small-gets}", "kv", "leftover:kv-log-small"),
            ("a folder whose parent is missing", ["{kvdir}/no/such"], "{kv-small-gets}", "kv", "fresh"),
            ("options not known", ["--sync", "sometimes", "{kvdir}"], "{kv-small-gets}", "kv", "fresh"),
        ],
        "measure": [
            ("a million sets, never synced", ["--sync", "never", "{kvdir}"], "{kv-sets}", "kv", "fresh"),
            ("a million sets, synced each block", ["--sync", "batch", "{kvdir}"], "{kv-sets}", "kv", "fresh"),
            ("ten thousand sets, each synced", ["--sync", "always", "{kvdir}"], "{kv-sets-10k}", "kv", "fresh", "once"),
            ("a million gets, sets and deletes", ["--sync", "batch", "{kvdir}"], "{kv-mixed}", "kv", "fresh"),
            ("a value of 100 MB", ["--sync", "batch", "{kvdir}"], "{kv-big}", "kv", "fresh"),
            ("a log of 1 GB read back", ["{kvdir}"], "{kv-gets}", "kv", "log:kv-log-1g"),
            ("ten million keys read and compacted", ["{kvdir}"], "{kv-compact}", "kv", "log:kv-log-10m"),
        ],
    },
    "std/json": {
        "check": [
            ("records, summed", ["sum", "{small-records}"], None),
            ("records, written back", ["copy", "{small-records}"], None),
            ("a document a line", ["lines", "{small-lines}"], None),
            ("coordinates, summed", ["sum", "{small-numbers}"], None),
            ("coordinates, written back", ["copy", "{small-numbers}"], None),
            ("not JSON", ["sum", "{readme}"], None),
            ("a line that is not JSON", ["lines", "{readme}"], None),
            ("a missing file", ["sum", "missing.json"], None),
            ("a wrong use", ["walk", "{small-records}"], None),
        ],
        "measure": [
            ("records, read and summed", ["sum", "{records}"], None),
            ("records, read and written back", ["copy", "{records}"], None),
            ("a document a line, each read and summed", ["lines", "{lines}"], None),
            ("coordinates, read and summed", ["sum", "{numbers}"], None),
        ],
    },
    "std/compression": {
        "check": [
            ("a small text, compressed", ["compress", "{small}", "{out}"], None, "gzip"),
            ("an empty file, compressed", ["compress", "{empty}", "{out}"], None, "gzip"),
            ("a small text, opened", ["decompress", "{small-gz}"], None),
            ("records of a small file", ["records", "{small-lines}"], None, "stderr"),
            ("not gzip", ["decompress", "{readme}"], None),
            ("a missing file", ["compress", "missing.txt", "{out}"], None),
            ("a wrong use", ["compress", "{small}"], None),
        ],
        "measure": [
            ("100 MB of a log, compressed", ["compress", "{text}", "{out}"], None, "gzip"),
            ("100 MB of a log, opened", ["decompress", "{text-gz}"], None),
            ("99,640 records, each compressed and opened", ["records", "{records-lines}"], None, "stderr"),
            ("100 MB that do not compress, compressed", ["compress", "{random}", "{out}"], None, "gzip"),
        ],
    },
    "std/zip": {
        "check": [
            ("a small archive, written", ["write", "{small}", "{out}"], None, "zip"),
            ("a small archive, read", ["read", "{small-parts}"], None),
            ("1,000 entries, listed", ["list", "{small-many}"], None),
            ("not an archive", ["read", "{readme}"], None),
            ("a missing archive", ["list", "missing.zip"], None),
            ("a wrong use", ["read"], None),
        ],
        "measure": [
            ("100 MB in 10,001 entries, written", ["write", "{text}", "{out}"], None, "zip"),
            ("10,001 entries, read", ["read", "{parts}"], None),
            ("100,000 entries, listed", ["list", "{many}"], None),
        ],
    },
    "std/hash": {
        "check": [
            ("a small file, hashed", ["sha256", "{small}"], None),
            ("an empty file, hashed", ["sha256", "{empty}"], None),
            ("1,000 hashes, chained", ["chain", "1000"], None),
            ("each line of a small file, signed", ["hmac", "{small}"], None),
            ("a missing file", ["hmac", "missing.txt"], None),
            ("a wrong use", ["chain", "many"], None),
        ],
        "measure": [
            ("100 MB of a log, hashed", ["sha256", "{text}"], None),
            ("10,000,000 hashes, chained", ["chain", "10000000"], None),
            ("921,952 lines, each signed", ["hmac", "{text}"], None),
        ],
    },
    "std/checksum": {
        "check": [
            ("a small file, CRC-32", ["crc32", "{small}"], None),
            ("a small file, Adler-32", ["adler32", "{small}"], None),
            ("an empty file, Adler-32", ["adler32", "{empty}"], None),
            ("each line of a small file, CRC-32", ["lines", "{small}"], None),
            ("a missing file", ["crc32", "missing.txt"], None),
            ("a wrong use", ["sum", "{small}"], None),
        ],
        "measure": [
            ("100 MB of a log, CRC-32", ["crc32", "{text}"], None),
            ("100 MB of a log, Adler-32", ["adler32", "{text}"], None),
            ("921,952 lines, each a CRC-32", ["lines", "{text}"], None),
        ],
    },
    "std/encoding": {
        "check": [
            ("2 MB, written in Base64", ["encode", "{small-random}", "{out}"], None, "base64"),
            ("an empty file, written in Base64", ["encode", "{empty}", "{out}"], None, "base64"),
            ("2.7 MB of Base64, read back", ["decode", "{small-random-b64}", "{out}"], None, "unbase64"),
            ("1,000 values, in hex and back", ["hex", "1000"], None),
            ("1,000 tokens, in Base64 for URLs and back", ["tokens", "1000"], None),
            ("not Base64", ["decode", "{readme}", "{out}"], None),
            ("a missing file", ["encode", "missing.bin", "{out}"], None),
            ("a wrong use", ["hex", "many"], None),
        ],
        "measure": [
            ("100 MB, written in Base64", ["encode", "{random}", "{out}"], None, "base64"),
            ("133 MB of Base64, read back", ["decode", "{random-b64}", "{out}"], None, "unbase64"),
            ("10,000,000 values of 32 bytes, in hex and back", ["hex", "10000000"], None),
            ("10,000,000 tokens, in Base64 for URLs and back", ["tokens", "10000000"], None),
        ],
    },
    "std/unicode": {
        "check": [
            ("the categories of a small text", ["category", "{small-unicode}"], None),
            ("a small text in NFD, into NFC", ["nfc", "{small-unicode-nfd}", "{out}"], None, "nfc"),
            ("a small text in NFC, into NFD", ["nfd", "{small-unicode}", "{out}"], None, "nfd"),
            ("a small text, checked to be NFC", ["check", "{small-unicode}"], None),
            ("a small text in NFD, checked to be NFC", ["check", "{small-unicode-nfd}"], None),
            ("the categories of a log", ["category", "{small}"], None),
            ("not UTF-8", ["category", "{small-random}"], None),
            ("a missing file", ["check", "missing.txt"], None),
            ("a wrong use", ["nfkc", "{small}", "{out}"], None),
        ],
        "measure": [
            ("100 MB, the category of each character", ["category", "{unicode}"], None),
            ("121 MB in NFD, each line into NFC", ["nfc", "{unicode-nfd}", "{out}"], None, "nfc"),
            ("100 MB in NFC, each line into NFD", ["nfd", "{unicode}", "{out}"], None, "nfd"),
            ("950,206 lines, each checked to be NFC", ["check", "{unicode}"], None),
        ],
    },
    "std/time": {
        "check": [
            ("1,000 moments, written", ["format", "1000"], None),
            ("a small file of moments, read", ["parse", "{small-time}"], None),
            ("1,000 moments, in the time zone", ["local", "1000"], None),
            ("the clock, read 1,000 times", ["clock", "1000"], None),
            ("not moments", ["parse", "{readme}"], None),
            ("a missing file", ["parse", "missing.txt"], None),
            ("a wrong use", ["format", "many"], None),
        ],
        "measure": [
            ("10,000,000 moments, written in ISO 8601", ["format", "10000000"], None),
            ("3,407,285 moments in RFC 3339, read", ["parse", "{time}"], None),
            ("1,000,000 moments, in the time zone", ["local", "1000000"], None),
            ("the clock, read 10,000,000 times", ["clock", "10000000"], None),
        ],
    },
    "std/random": {
        "check": [
            ("1,000 numbers", ["numbers", "1000"], None),
            ("1,000 numbers in a range", ["range", "1000"], None),
            ("1 MB of bytes", ["fill", "1"], None),
            ("10 tokens", ["tokens", "10"], None),
            ("a wrong use", ["numbers", "many"], None),
        ],
        "measure": [
            ("100,000,000 numbers of 64 bits", ["numbers", "100000000"], None),
            ("100,000,000 numbers up to 1,000", ["range", "100000000"], None),
            ("100 MB of bytes", ["fill", "100"], None),
            ("1,000,000 tokens of the system", ["tokens", "1000000"], None),
        ],
    },
    "std/log": {
        "check": [
            ("1,000 lines", ["lines", "1000"], None, "log"),
            ("1,000 lines that are off", ["off", "1000"], None, "log"),
            ("1,000 lines from four tasks", ["tasks", "1000"], None, "log"),
            ("a wrong use", ["lines", "many"], None),
        ],
        "measure": [
            ("1,000,000 lines", ["lines", "1000000"], None, "log"),
            ("100,000,000 lines that are off", ["off", "100000000"], None, "log"),
            ("1,000,000 lines from four tasks", ["tasks", "1000000"], None, "log"),
        ],
    },
    "std/net": {
        "check": [
            ("echo, 64 bytes, async", ["echo", "async", "64", "8", "100"], None),
            ("echo, 64 bytes, blocking", ["echo", "blocking", "64", "8", "100"], None),
            ("echo, 64 KB, async", ["echo", "async", "65536", "2", "20"], None),
            ("echo, 64 KB, blocking", ["echo", "blocking", "65536", "2", "20"], None),
            ("100 connections at once", ["echo", "async", "64", "100", "10"], None),
            ("100 connections one after another, async", ["connect", "async", "100"], None),
            ("100 connections one after another, blocking", ["connect", "blocking", "100"], None),
            ("1 MB in pieces of 4 KB, async", ["stream", "async", "1", "4096"], None),
            ("1 MB in pieces of 64 KB, blocking", ["stream", "blocking", "1", "65536"], None),
            ("a small file in lines, async", ["lines", "async", "{small}"], None),
            ("a small file in lines, blocking", ["lines", "blocking", "{small}"], None),
            ("datagrams of 8 KB, async", ["udp", "async", "8192", "8", "100"], None),
            ("datagrams of 64 bytes, blocking", ["udp", "blocking", "64", "8", "100"], None),
            ("1,000 addresses", ["address", "{small-addresses}"], None),
            ("1,000 addresses, written", ["print", "{small-addresses}", "{out}"], None, "address"),
            ("lines that are not addresses", ["address", "{readme}"], None),
            ("a missing file", ["address", "missing.txt"], None),
            ("a wrong use", ["echo", "now", "64", "8", "100"], None),
        ],
        "measure": [
            ("echo, 64 bytes, async", ["echo", "async", "64", "8", "20000"], None),
            ("echo, 64 bytes, blocking", ["echo", "blocking", "64", "8", "20000"], None),
            ("echo, 4 KB, async", ["echo", "async", "4096", "8", "20000"], None),
            ("echo, 4 KB, blocking", ["echo", "blocking", "4096", "8", "20000"], None),
            ("echo, 64 KB, async", ["echo", "async", "65536", "8", "20000"], None),
            ("echo, 64 KB, blocking", ["echo", "blocking", "65536", "8", "20000"], None),
            ("1,000 connections at once, async", ["echo", "async", "64", "1000", "100"], None),
            ("50,000 connections one after another, async", ["connect", "async", "50000"], None),
            ("50,000 connections one after another, blocking", ["connect", "blocking", "50000"], None),
            ("1 GB in pieces of 4 KB, async", ["stream", "async", "1024", "4096"], None),
            ("1 GB in pieces of 4 KB, blocking", ["stream", "blocking", "1024", "4096"], None),
            ("1 GB in pieces of 64 KB, async", ["stream", "async", "1024", "65536"], None),
            ("1 GB in pieces of 64 KB, blocking", ["stream", "blocking", "1024", "65536"], None),
            ("100 MB in lines, async", ["lines", "async", "{text}"], None),
            ("100 MB in lines, blocking", ["lines", "blocking", "{text}"], None),
            ("datagrams of 64 bytes, async", ["udp", "async", "64", "8", "25000"], None),
            ("datagrams of 64 bytes, blocking", ["udp", "blocking", "64", "8", "25000"], None),
            ("datagrams of 1 KB, async", ["udp", "async", "1024", "8", "25000"], None),
            ("datagrams of 1 KB, blocking", ["udp", "blocking", "1024", "8", "25000"], None),
            ("datagrams of 8 KB, async", ["udp", "async", "8192", "8", "25000"], None),
            ("datagrams of 8 KB, blocking", ["udp", "blocking", "8192", "8", "25000"], None),
            ("10,000,000 addresses, read and written", ["address", "{addresses}"], None),
        ],
    },
    "std/command_line": {
        "check": [
            ("flags, options and words", ["-vf", "zip", "src"], None),
            ("nothing but the words", ["tar", "docs", "--", "--not-an-option", "-"], None),
            ("an option that is not one", ["--verbos", "zip", "src"], None),
            ("a word missing", ["-v", "zip"], None),
            ("an option with no value", ["zip", "src", "--output"], None),
        ],
        "measure": [
            ("a usual command line", ["-vfn", "--level=9", "-o", "out.zip", "zip", "src", "-xa*.tmp",
                                      "--exclude", "*.log", "notes.txt", "--", "-odd.txt", "-"], None),
            ("10,000 words", ["--dry-run", "zip", "src", *[f"file-{i}.txt" for i in range(10000)]], None),
            ("the help", ["--help"], None, "own"),
        ],
    },
    "std/environment": {
        "check": [
            ("a variable there, missing and not text",
             ["get", "SG_TRIAL_TEXT", "SG_TRIAL_MISSING", "SG_TRIAL_BYTES"], None),
            ("a wrong use", ["read", "SG_TRIAL_TEXT", "many"], None),
        ],
        "measure": [
            ("one variable", ["get", "SG_TRIAL_TEXT"], None),
            ("a variable read 1,000,000 times", ["read", "SG_TRIAL_TEXT", "1000000"], None),
        ],
    },
    "std/path": {
        "check": [
            ("paths a program meets", ["show", "logs/report.txt", "/report.txt", "café/日本.tar.gz",
                                       "a/b/README", "/usr/local/lib/libz.so.1", "./build.sg"], None),
            ("1,000 paths", ["many", "1000"], None),
            ("1,000 paths, joined", ["many", "1000", "join"], None),
            ("a wrong use", ["many", "all"], None),
            ("a wrong way", ["many", "1000", "glue"], None),
        ],
        "measure": [
            ("1,000,000 paths built and taken apart", ["many", "1000000", "push"], None),
            ("the same, SG joining each part into a new text", ["many", "1000000", "join"], None),
        ],
    },
}

# Rust built once more with a feature: trial -> (name of the build, arguments of `cargo build`).
RUST_FEATURES = {
    "std/compression": [("Rust zlib-rs", ["--features", "zlib-rs"])],
    "std/unicode": [("Rust unicode-normalization", ["--features", "unicode-normalization"])],
    "std/command_line": [("Rust lexopt", ["--no-default-features", "--features", "lexopt"])],
}

# The files of `rust/src` each Rust build leaves out of its lines, where one crate holds two ways.
RUST_LEFT_OUT = {
    "std/command_line": {"Rust": {"with_lexopt.rs"}, "Rust lexopt": {"with_clap.rs"}},
}

# The trials whose time is not shown, only their memory and size (1553, 1801).
NO_TIME = {"std/command_line", "std/environment"}


def fresh_copy(source, target):
    """A program copied as a new file: macOS kills one whose signed bytes were written over in
    place (the kernel keeps what it checked of the old file), with SIGKILL when it starts."""
    Path(target).unlink(missing_ok=True)
    shutil.copy(source, target)


def run(command, **kw):
    """Runs a command that must succeed, showing it when it does not."""
    result = subprocess.run(command, capture_output=True, text=True, **kw)
    if result.returncode != 0:
        sys.exit(f"failed: {' '.join(map(str, command))}\n{result.stdout}{result.stderr}")
    return result


def build_sg():
    if not SG.exists():
        run(["cargo", "build", "--release"], cwd=ROOT / "interpreter")


def make_inputs():
    OUT.mkdir(parents=True, exist_ok=True)
    maker = OUT / "text"
    run([SG, "build", TRIALS / "inputs" / "text.sg", "--release", "-o", maker])
    for path, megabytes in ((text_input(), TEXT_MB), (small_input(), 2)):
        if not path.exists():
            with open(path, "wb") as f:
                subprocess.run([maker, str(megabytes), str(TEXT_SEED)], stdout=f, check=True)
    (OUT / "empty.txt").write_bytes(b"")


def build(trial):
    """The programs of a trial: name of the build -> path of its executable."""
    folder = TRIALS / trial
    out = OUT / trial
    out.mkdir(parents=True, exist_ok=True)
    # `std/json` is built as `json`.
    full = trial
    trial = Path(trial).name
    sg_source = folder / "sg" / f"{trial}.sg"
    # A trial written in several files is the folder `sg/`.
    if not sg_source.exists():
        sg_source = folder / "sg"
    built = {}
    built["SG"] = out / f"{trial}-sg"
    run([SG, "build", sg_source, "--release", "-o", built["SG"]])
    built["SG checked"] = out / f"{trial}-sg-checked"
    c_file = out / f"{trial}-checked.c"
    c_file.write_text(run([SG, "emit-c", sg_source]).stdout)
    hardening = ["-fstack-clash-protection"] if sys.platform.startswith("linux") else []
    run([os.environ.get("CC") or "cc", "-std=c11", "-O2", "-ffp-contract=off", *hardening, c_file,
         "-o", built["SG checked"], "-lm", "-pthread"])
    run(["cargo", "build", "--release", "--quiet", "--target-dir", out / "cargo"], cwd=folder / "rust")
    built["Rust"] = out / f"{trial}-rust"
    fresh_copy(out / "cargo" / "release" / trial, built["Rust"])
    for name, features in RUST_FEATURES.get(full, []):
        tag = name.lower().replace(" ", "-")
        target = out / f"cargo-{tag}"
        run(["cargo", "build", "--release", "--quiet", *features, "--target-dir", target], cwd=folder / "rust")
        built[name] = out / f"{trial}-{tag}"
        fresh_copy(target / "release" / trial, built[name])
    zig = folder / "zig"
    for mode, name in (("ReleaseFast", "Zig"), ("ReleaseSafe", "Zig safe")):
        built[name] = out / f"{trial}-zig-{mode}"
        cache = ["--cache-dir", out / "zig-cache"]
        if (zig / "build.zig").exists():
            prefix = out / f"zig-{mode}"
            run(["zig", "build", f"-Doptimize={mode}", "--prefix", prefix, *cache], cwd=zig)
            fresh_copy(prefix / "bin" / trial, built[name])
        else:
            run(["zig", "build-exe", zig / f"{trial}.zig", "-O", mode, f"-femit-bin={built[name]}", *cache])
    # The tools people use, measured beside the three for reference (1810): jq itself, the output
    # every build is held to, and jaq, which is not the same algorithm and not always the same output.
    if full in REFERENCES:
        for name, path in REFERENCES[full]().items():
            built[name] = path
    return built


def jq_references():
    jaq = OUT / "jaq" / "bin" / "jaq"
    if not jaq.exists():
        run(["cargo", "install", "jaq", "--version", JAQ_VERSION, "--locked", "--root", OUT / "jaq"])
    jq = shutil.which("jq")
    version = run([jq, "--version"]).stdout.strip() if jq else ""
    if version != "jq-1.8.2":
        sys.exit(f"  jq 1.8.2 is needed for the trial of jq; found {version or 'none'}")
    # jaq 3.2.0 panics before it starts when a variable of the environment is not UTF-8 (it reads
    # them with `std::env::vars`, for `$ENV`), as `SG_TRIAL_BYTES` is; it runs without them.
    NEEDS_UTF8_ENVIRONMENT.add(str(jaq))
    return {"jq 1.8.2": Path(jq), "jaq": jaq}


# The programs run with only the variables of the environment that are UTF-8.
NEEDS_UTF8_ENVIRONMENT = set()


def UTF8_ENVIRONMENT():
    out = {}
    for name, value in os.environb.items():
        try:
            out[name.decode()] = value.decode()
        except UnicodeDecodeError:
            pass
    return out


JAQ_VERSION = "3.2.0"
REFERENCES = {"jq": jq_references}
# References whose output is not held to the others: their runs are measured, and a case where one
# writes another output or exit code is marked in the table.
LOOSE = {"jaq"}
REFERENCE_NAMES = {"jq 1.8.2", "jaq"}


def expand(words, trial):
    values = {
        "text": str(text_input()),
        "small": str(small_input()),
        "empty": str(OUT / "empty.txt"),
        "readme": str(ROOT / "README.md"),
        "out": str(OUT / trial / "out"),
    }
    words = [w.format(**values) if "{" in w and "}" in w and w.strip("{}") in values else w for w in words]
    words = [str(json_input(w.strip("{}"))) if w.startswith("{") and w.strip("{}") in JSON_INPUTS else w
             for w in words]
    words = [str(OUT / "kv" / "store") + w[len("{kvdir}"):] if w.startswith("{kvdir}") else w for w in words]
    words = [str(kv_input(w.strip("{}"))) if w.startswith("{") and w.strip("{}") in KV_INPUTS else w for w in words]
    return [str(other_input(w.strip("{}"))) if w.startswith("{") and w.strip("{}") in OTHER_INPUTS else w
            for w in words]


def kv_prepare(kind):
    """The folder of `kv`, made anew before a run: empty (`fresh`), with a log Python wrote
    (`log:NAME`), or with that log and a `log.new` a compaction left (`leftover:NAME`)."""
    store = OUT / "kv" / "store"
    if store.exists():
        shutil.rmtree(store)
    store.mkdir(parents=True)
    how, _, name = kind[1].partition(":")
    if name:
        shutil.copyfile(kv_input(name), store / "log")
    if how == "leftover":
        (store / "log.new").write_bytes(b"half of a compaction")


def kv_log():
    """What a run of `kv` left: its log, held as its SHA-256 and its length."""
    log = OUT / "kv" / "store" / "log"
    if not log.exists():
        return b"no log"
    data = log.read_bytes()
    return f"log {len(data)} bytes, {hashlib.sha256(data).hexdigest()}".encode()


def run_once(program, args, stdin_path, capture, errors=None):
    """Runs a program once: (exit code, standard output or None, seconds, peak memory in bytes).
    With `errors`, a list, the standard error is added to it."""
    stdin = open(stdin_path, "rb") if stdin_path else subprocess.DEVNULL
    stdout = subprocess.PIPE if capture else subprocess.DEVNULL
    # A file, not a pipe, so that a program writing much to both cannot wait on the other.
    stderr = tempfile.TemporaryFile() if errors is not None else subprocess.DEVNULL
    start = time.perf_counter()
    env = UTF8_ENVIRONMENT() if str(program) in NEEDS_UTF8_ENVIRONMENT else None
    process = subprocess.Popen([program, *args], stdin=stdin, stdout=stdout, stderr=stderr, env=env)
    hung = []
    watch = threading.Timer(HUNG_SECONDS, report_hung, (process, [program, *args], hung))
    watch.start()
    output = process.stdout.read() if capture else None
    _, status, usage = os.wait4(process.pid, 0)
    seconds = time.perf_counter() - start
    watch.cancel()
    if hung:
        sys.exit(f"  HUNG after {HUNG_SECONDS} s: {' '.join(map(str, [program, *args]))}; "
                 f"what it was doing is in {hung[0]}")
    process.returncode = os.waitstatus_to_exitcode(status)
    if stdin_path:
        stdin.close()
    if errors is not None:
        stderr.seek(0)
        errors.append(stderr.read())
        stderr.close()
    # macOS gives bytes, Linux kilobytes.
    peak = usage.ru_maxrss if MACOS else usage.ru_maxrss * 1024
    return process.returncode, output, seconds, peak


def report_hung(process, command, hung):
    """Writes what a run that has not ended was doing into `trials/target/hung-PID.txt`, then
    kills it."""
    path = OUT / f"hung-{process.pid}.txt"
    with open(path, "w") as f:
        f.write(" ".join(map(str, command)) + "\n\n")
        f.flush()
        looks = [["sample", str(process.pid), "1"]] if MACOS else []
        looks.append(["lsof", "-p", str(process.pid)])
        for look in looks:
            try:
                subprocess.run(look, stdout=f, stderr=subprocess.STDOUT, timeout=60)
            except (OSError, subprocess.TimeoutExpired) as error:
                f.write(f"{look[0]}: {error}\n")
            f.flush()
    hung.append(path)
    process.kill()


@functools.cache
def expected(kind, path):
    """What Python makes of the input of a case whose `{out}` must be exactly that: its Base64
    (`base64`), the bytes of its Base64 (`unbase64`), its NFC or NFD (`nfc`, `nfd`), each of its
    addresses as `ipaddress` writes it (`address`)."""
    import base64
    import ipaddress
    import unicodedata
    source = Path(path).read_bytes()
    if kind == "address":
        def written(line):
            host, port = line.rsplit(":", 1)
            if host.startswith("["):
                return f"[{ipaddress.IPv6Address(host[1:-1]).compressed}]:{port}\n"
            return f"{ipaddress.IPv4Address(host)}:{port}\n"
        return "".join(written(line) for line in source.decode().splitlines()).encode()
    if kind == "base64":
        return base64.b64encode(source)
    if kind == "unbase64":
        return base64.b64decode(source, validate=True)
    return unicodedata.normalize(kind.upper(), source.decode()).encode()


def log_lines(error):
    """The lines a log wrote, held to their form: how many, and a hash of their messages in order of
    their text, the same whatever order tasks wrote them in; or what is wrong."""
    import hashlib
    import re
    form = re.compile(rb"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z INFO  ([^\n]*)")
    lines = error.split(b"\n")
    if lines[-1] != b"":
        return b"malformed: the last line has no end"
    messages = []
    for line in lines[:-1]:
        seen = form.fullmatch(line)
        if seen is None:
            return b"malformed: " + line[:80]
        messages.append(seen.group(1))
    messages.sort()
    return f"log lines {len(messages)}, messages {hashlib.sha256(b'\n'.join(messages)).hexdigest()}".encode()


def verify(kind, args, error):
    """What a build wrote that the others may write another way, checked: (what it says, the
    note shown under the table, or None). `gzip`: `{out}` opens to the file compressed; `zip`:
    `{out}` is an archive whose every entry is the part of the file it should be, its CRC-32 right;
    `stderr`: the last word of the standard error, the bytes the program counted compressed;
    `base64`, `unbase64`, `nfc`, `nfd`, `address`: `{out}` is what Python makes of the input."""
    import gzip
    import zipfile
    if kind == "log":
        return log_lines(error), None
    if kind == "stderr":
        words = error.split()
        return b"", words[-1].decode() if words else "?"
    out = Path(args[-1])
    if not out.exists():
        return b"nothing written", "-"
    size = out.stat().st_size
    if kind in ("base64", "unbase64", "nfc", "nfd", "address"):
        same = out.read_bytes() == expected(kind, args[1])
        out.unlink()
        return f"what it wrote opens to its input: {same}".encode(), None
    source = Path(args[1]).read_bytes()
    if kind == "gzip":
        try:
            same = gzip.decompress(out.read_bytes()) == source
        except (OSError, EOFError) as e:
            same = f"not gzip: {e}"
    else:
        try:
            with zipfile.ZipFile(out) as z:
                same = z.testzip() is None and [(i.filename, z.read(i)) for i in z.infolist()] == zip_of_parts(source)
        except zipfile.BadZipFile as e:
            same = f"not an archive: {e}"
    out.unlink()
    return f"what it wrote opens to its input: {same}".encode(), f"{size:,} bytes"


def check(trial, built, cases, notes):
    """Every build must give the output and the exit code of the first. What a case's check
    notes of each build goes into `notes`: case -> build -> note."""
    ok = True
    for name, words, stdin, *kind in cases:
        args = expand(words, trial)
        stdin_path = expand([stdin], trial)[0] if stdin else None
        results = {}
        for build, path in built.items():
            errors = []
            if kind and kind[0] == "kv":
                kv_prepare(kind)
            code, output = run_once(path, args, stdin_path, True, errors)[:2]
            if kind and kind[0] == "kv":
                output += kv_log()
            elif kind == ["own"]:
                output = b""
            elif kind == ["quirk"]:
                # jq takes what the three refuse (its quirks, in `trials/jq/README.md`).
                if build == "jq 1.8.2":
                    continue
            elif kind:
                said, note = verify(kind[0], args, errors[0])
                wrong = said.startswith(b"malformed") if kind[0] == "log" else \
                    kind[0] != "stderr" and said != b"what it wrote opens to its input: True"
                if wrong:
                    ok = False
                    print(f"  WRONG {trial} / {name}: {build}: {said.decode()}")
                output += said
                if note is not None:
                    notes.setdefault(name, {})[build] = note
            # The output is held as its SHA-256 and its length, so that a case of 1 GB is not
            # kept once for each build.
            results[build] = (code, hashlib.sha256(output).digest(), len(output))
        first = next(iter(results.values()))
        for build, result in results.items():
            if build in LOOSE:
                if result != first:
                    notes.setdefault("loose", {}).setdefault(build, []).append(name)
                continue
            if result != first:
                ok = False
                print(f"  DIFFERENT {trial} / {name}: {build} gives exit {result[0]} and "
                      f"{result[2]} bytes, SG gives exit {first[0]} and {first[2]} bytes")
    print(f"  {trial}: {len(cases)} cases, {'the same output from every build' if ok else 'NOT the same'}")
    return ok


def measure(trial, built, cases, runs):
    rows = []
    for name, words, stdin, *kind in cases:
        args = expand(words, trial)
        stdin_path = expand([stdin], trial)[0] if stdin else None
        # A log writes into a file, as it would, not into `/dev/null`.
        errors = [] if kind == ["log"] else None
        row = {}
        for build, path in built.items():
            best_time, best_peak = None, None
            # One run to warm up, then `runs`; a run that fails stops the measuring, so that a
            # program that gave up early is never counted fast (1685).
            failed = False
            for run in range((1 if "once" in kind else runs) + 1):
                if kind and kind[0] == "kv":
                    kv_prepare(kind)
                code, _, seconds, peak = run_once(path, args, stdin_path, False, errors)
                if code != 0 and build in LOOSE | REFERENCE_NAMES:
                    # A reference that fails a case is shown so, not counted.
                    failed = True
                    break
                if code != 0:
                    sys.exit(f"  FAILED {trial} / {name}: {build} gave exit {code}")
                if errors:
                    errors.clear()
                if run == 0:
                    continue
                best_time = seconds if best_time is None else min(best_time, seconds)
                best_peak = peak if best_peak is None else min(best_peak, peak)
            row[build] = (None, None) if failed else (best_time, best_peak)
            # A line as each build is done, so that a long run is seen going on (and where it hangs).
            done = "fails" if failed else f"{best_time:.3f} s, {best_peak / 1048576:.1f} MB"
            print(f"  {trial} / {name}: {build} {done}", file=sys.stderr, flush=True)
        rows.append((name, row))
    return rows


def stripped_size(path):
    copy = path.with_name(path.name + "-stripped")
    fresh_copy(path, copy)
    subprocess.run(["strip", copy], check=True, capture_output=True)
    return copy.stat().st_size


def lines(folder, suffix, left_out=frozenset()):
    """Lines that are neither blank nor only a comment."""
    count = 0
    for path in sorted(folder.rglob(f"*{suffix}")):
        if path.name in left_out or "target" in path.parts or path.name == "build.zig" or ".zig-cache" in path.parts:
            continue
        for line in path.read_text(encoding="utf-8").splitlines():
            text = line.strip()
            if text and not text.startswith("//"):
                count += 1
    return count


def report(trial, built, rows, notes):
    folder = TRIALS / trial
    builds = list(built)
    print(f"\n### {trial}\n")
    print("| Case | " + " | ".join(builds) + " |")
    print("|---|" + "---|" * len(builds))
    for name, row in rows:
        if trial in NO_TIME:
            cells = [f"{row[b][1] / 1048576:.2f} MB" if row[b][1] is not None else "fails" for b in builds]
        else:
            cells = [f"{row[b][0]:.3f} s, {row[b][1] / 1048576:.1f} MB" if row[b][0] is not None else "fails" for b in builds]
        print(f"| {name} | " + " | ".join(cells) + " |")
    sizes = {b: stripped_size(p) for b, p in built.items() if b not in REFERENCE_NAMES}
    print("| Size, stripped | " + " | ".join(f"{sizes[b] / 1024:.0f} KB" if b in sizes else "—" for b in builds) + " |")
    left_out = RUST_LEFT_OUT.get(trial, {})
    counts = {"SG": lines(folder / "sg", ".sg"), "Zig": lines(folder / "zig", ".zig")}
    for b in builds:
        if b.startswith("Rust"):
            counts[b] = lines(folder / "rust", ".rs", left_out.get(b, frozenset()))
    print("| Lines | " + " | ".join("—" if b in REFERENCE_NAMES else str(counts.get(b, counts[b.split()[0]])) for b in builds) + " |")
    for build, cases in notes.get("loose", {}).items():
        print(f"\n{build} writes another output or exit code in: " + ", ".join(cases))
    measured = [name for name, _ in rows]
    for name in measured:
        if name in notes:
            print(f"\n{name}, the size written: " +
                  ", ".join(f"{b} {notes[name][b]}" for b in builds))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("trials", nargs="*", help="the trials to run (all when none)")
    parser.add_argument("--check", action="store_true", help="only check that the outputs are the same")
    parser.add_argument("--runs", type=int, default=5, help="runs of each case, the best kept")
    a = parser.parse_args()
    trials = a.trials or list(TRIALS_CASES)
    os.environ["TZ"] = "Europe/Istanbul"
    for name in ("SG_LOG", "RUST_LOG", "SG_TRIAL_MISSING"):
        os.environ.pop(name, None)
    os.environ["SG_TRIAL_TEXT"] = "/usr/local/bin:/usr/bin:/bin:/opt/sg/bin:/home/ayşe/.local/bin"
    os.environb[b"SG_TRIAL_BYTES"] = b"caf\xe9"
    build_sg()
    make_inputs()
    all_ok = True
    for trial in trials:
        cases = TRIALS_CASES[trial]
        built = build(trial)
        notes = {}
        all_ok &= check(trial, built, cases["check"] + cases["measure"], notes)
        if all_ok and not a.check:
            report(trial, built, measure(trial, built, cases["measure"], a.runs), notes)
    sys.exit(0 if all_ok else 1)


if __name__ == "__main__":
    main()
