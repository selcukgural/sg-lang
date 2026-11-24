#!/usr/bin/env python3
"""The builds of trials/kv held to a model of the store written in Python (choice 1850), on the cases
that break things (choice 1849):

- streams of commands made at random, keys and values of any bytes but the line's end (and a key
  without spaces), lines that are not commands among them, each `--sync`, short and long enough
  that the log is compacted on its own;
- a log cut at every byte of its last records and at places drawn anywhere, which is a crash in the
  middle of a write: the record cut off is cut away, the store starts with what came before;
- a log with one bit turned at places drawn: a record whose CRC is wrong with more after it is
  damage, and the store does not start; in its last record, it is taken for a crash;
- logs written by each build read by each other;
- a crash: the store killed (SIGKILL) at a moment drawn while it is given sets, in each `--sync`,
  then started again: every set it answered `OK` must be there with its value. A SIGKILL leaves what
  was handed to the system in its cache, so this tests the program, not the disk: a machine that
  loses its power is not tested here;
- its output closed while it writes: it ends as a C program does, by SIGPIPE.

For each case every build must give the standard output, the exit code and the log, byte for byte,
that the model gives (the standard error is not compared: each language words its errors its own
way).

    python3 -I trials/kv/tests/check.py [BINARY...] [--interpreted] [--quick] [--only PART] [-v]

With no BINARY, the builds `python3 trials/measure.py --check kv` left in trials/target/kv. With
`--interpreted`, SG's interpreter runs the cases too (interpreter/target/release/sg); `--quick`
draws fewer cases; `--only` runs one part (options, streams, cuts, flips, each_reads_each,
closed_output, crash). Files are written in trials/target/kv-tests.
"""
import os
import random
import shutil
import signal
import struct
import subprocess
import sys
import threading
import time
import zlib

REPO = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
HERE = os.path.join(REPO, "trials", "target", "kv-tests")
BUILT = os.path.join(REPO, "trials", "target", "kv")
SG = os.path.join(REPO, "interpreter", "target", "release", "sg")
SOURCE = os.path.join(REPO, "trials", "kv", "sg", "kv.sg")
COMPACT_FROM = 1_048_576
TIMEOUT = 120

args = sys.argv[1:]
VERBOSE = "-v" in args
QUICK = "--quick" in args
INTERPRETED = "--interpreted" in args
ONLY = args[args.index("--only") + 1] if "--only" in args else None
binaries = [a for a in args if not a.startswith("-") and a != ONLY]
if not binaries:
    binaries = [os.path.join(BUILT, name) for name in
                ("kv-sg", "kv-sg-checked", "kv-rust", "kv-zig-ReleaseFast", "kv-zig-ReleaseSafe")]
    missing = [b for b in binaries if not os.path.exists(b)]
    if missing:
        sys.exit(f"not built: {', '.join(missing)}; run `python3 trials/measure.py --check kv` first")
BUILDS = {os.path.basename(b): [b] for b in binaries}
if INTERPRETED:
    BUILDS["sg interpreted"] = [SG, SOURCE]


# The model.

def record(op, key, value):
    body = struct.pack("<BII", op, len(key), len(value)) + key + value
    return struct.pack("<I", zlib.crc32(body)) + body


class Model:
    """The store as the programs keep it: what it answers, how it ends and the log it leaves."""

    def __init__(self, log):
        self.map = {}
        self.out = bytearray()
        self.code = 0
        self.live = 0
        self.log = bytearray(log)
        at, size = 0, len(log)
        self.damaged = False
        while at < size:
            if size - at < 13:
                break
            crc, op, k, v = struct.unpack_from("<IBII", log, at)
            length = 13 + k + v
            if length > size - at:
                break
            body = log[at + 4:at + length]
            right = zlib.crc32(body) == crc
            known = op == 1 or (op == 2 and v == 0)
            if not right or not known:
                if at + length == size and not right:
                    break
                self.damaged = True
                self.code = 1
                return
            key = bytes(log[at + 13:at + 13 + k])
            if op == 1:
                self.set(key, bytes(log[at + 13 + k:at + length]))
            else:
                self.delete(key)
            at += length
        del self.log[at:]

    def set(self, key, value):
        if key in self.map:
            self.live -= 13 + len(key) + len(self.map[key])
        self.map[key] = value
        self.live += 13 + len(key) + len(value)

    def delete(self, key):
        self.live -= 13 + len(key) + len(self.map.pop(key))

    def compact(self):
        self.log = bytearray(b"".join(record(1, k, self.map[k]) for k in sorted(self.map)))
        self.live = len(self.log)

    def changed(self):
        if len(self.log) > COMPACT_FROM and len(self.log) > 2 * self.live:
            self.compact()

    def line(self, line):
        s = line.find(b" ")
        if s < 0:
            if line == b"COMPACT":
                self.compact()
                self.out += b"OK\n"
            else:
                self.code = 1
            return
        word, rest = line[:s], line[s + 1:]
        if word == b"SET":
            k = rest.find(b" ")
            if k < 0:
                self.code = 1
                return
            key, value = rest[:k], rest[k + 1:]
            self.log += record(1, key, value)
            self.set(key, value)
            self.out += b"OK\n"
            self.changed()
        elif word == b"GET":
            self.out += self.map[rest] + b"\n" if rest in self.map else b"(nil)\n"
        elif word == b"DEL":
            if rest not in self.map:
                self.out += b"0\n"
                return
            self.log += record(2, rest, b"")
            self.delete(rest)
            self.out += b"1\n"
            self.changed()
        else:
            self.code = 1

    def run(self, commands):
        if self.damaged:
            return self
        lines = commands.split(b"\n")
        if lines[-1] == b"":
            lines.pop()
        for line in lines:
            self.line(line)
        return self


# Running the builds.

def folder(name):
    path = os.path.join(HERE, name)
    if os.path.exists(path):
        shutil.rmtree(path)
    os.makedirs(path)
    return path


def run(build, store, commands, sync=None):
    argv = BUILDS[build] + (["--sync", sync] if sync else []) + [store]
    done = subprocess.run(argv, input=commands, capture_output=True, timeout=TIMEOUT)
    log_path = os.path.join(store, "log")
    log = open(log_path, "rb").read() if os.path.exists(log_path) else b""
    return done.stdout, done.returncode, log, done.stderr


failures = []
counted = [0]


def held(name, build, got, model, log_before=None):
    """Holds what a build gave to what the model gives."""
    counted[0] += 1
    out, code, log, err = got
    want_log = bytes(model.log) if not model.damaged else log_before
    problems = []
    if out != bytes(model.out):
        problems.append(f"output: {len(out)} bytes, the model {len(model.out)}")
    if code != model.code:
        problems.append(f"exit {code}, the model {model.code}")
    if want_log is not None and log != want_log:
        problems.append(f"log: {len(log)} bytes, the model {len(want_log)}")
    if problems:
        failures.append(f"{name} / {build}: {'; '.join(problems)}; stderr: {err[:200]!r}")
        if VERBOSE:
            print(f"  FAIL {failures[-1]}")


# The cases.

def random_key(rng):
    alphabet = b"abcxyz019\xff\xfe\xc3\x28\r\t\x00"
    return bytes(rng.choice(alphabet) for _ in range(rng.choice([0, 1, 2, 3, 8])))


def random_value(rng):
    alphabet = b"ab c\xff\r\x00 "
    length = rng.choice([0, 1, 5, 40, 150, 400])
    return bytes(rng.choice(alphabet) for _ in range(length))


def random_commands(rng, count):
    keys = [random_key(rng) for _ in range(rng.choice([3, 20, 200]))]
    lines = []
    for _ in range(count):
        r = rng.random()
        key = rng.choice(keys)
        if r < 0.45:
            lines.append(b"SET " + key + b" " + random_value(rng))
        elif r < 0.75:
            lines.append(b"GET " + key)
        elif r < 0.9:
            lines.append(b"DEL " + key)
        elif r < 0.92:
            lines.append(b"COMPACT")
        else:
            lines.append(rng.choice([b"", b"FOO", b"SET", b"SET " + key, b"get " + key, b"COMPACT ", b" GET a", b"DEL"]))
    data = b"\n".join(lines)
    return data if rng.random() < 0.5 else data + b"\n"


def streams(rng):
    sizes = [10, 100, 1000, 20000] if not QUICK else [10, 100, 3000]
    for size in sizes:
        for draw in range(6 if not QUICK else 2):
            commands = random_commands(rng, size)
            sync = rng.choice(["always", "batch", "never"]) if size <= 1000 else rng.choice(["batch", "never"])
            model = Model(b"").run(commands)
            # The same again on what the first left: the log read back, then more commands.
            more = random_commands(rng, max(10, size // 10))
            again = Model(bytes(model.log)).run(more)
            for build in BUILDS:
                if size >= 20000 and build == "sg interpreted":
                    continue
                store = folder("stream")
                held(f"stream {size} #{draw} ({sync})", build, run(build, store, commands, sync), model)
                held(f"stream {size} #{draw}, again", build, run(build, store, more, sync), again)


def a_log(rng, count):
    model = Model(b"").run(b"\n".join(b"SET " + random_key(rng) + b" " + random_value(rng) if rng.random() < 0.85
                                      else b"DEL " + random_key(rng) for _ in range(count)))
    return bytes(model.log)


def all_gets(log):
    keys = set()
    at = 0
    while at + 13 <= len(log):
        _, _, k, v = struct.unpack_from("<IBII", log, at)
        keys.add(bytes(log[at + 13:at + 13 + k]))
        at += 13 + k + v
    return b"".join(b"GET " + k + b"\n" for k in sorted(keys) if b" " not in k and b"\n" not in k)


def write_log(store, data):
    with open(os.path.join(store, "log"), "wb") as f:
        f.write(data)


def cuts(rng):
    log = a_log(rng, 300)
    gets = all_gets(log)
    # The starts of the last three records.
    starts, at = [], 0
    while at < len(log):
        starts.append(at)
        _, _, k, v = struct.unpack_from("<IBII", log, at)
        at += 13 + k + v
    places = list(range(starts[-3], len(log) + 1))
    places += rng.sample(range(len(log)), 40 if QUICK else 200)
    for place in places:
        cut = log[:place]
        model = Model(cut).run(gets)
        for build in BUILDS:
            store = folder("cut")
            write_log(store, cut)
            held(f"log cut at {place} of {len(log)}", build, run(build, store, gets), model, cut)


def flips(rng):
    log = a_log(rng, 300)
    gets = all_gets(log)
    for _ in range(40 if QUICK else 300):
        place = rng.randrange(len(log))
        turned = bytearray(log)
        turned[place] ^= 1 << rng.randrange(8)
        turned = bytes(turned)
        model = Model(turned).run(gets)
        for build in BUILDS:
            store = folder("flip")
            write_log(store, turned)
            held(f"a bit turned at {place}", build, run(build, store, gets), model, turned)


def each_reads_each(rng):
    commands = random_commands(rng, 3000)
    for writer in BUILDS:
        store = folder("written")
        run(writer, store, commands, "never")
        log = open(os.path.join(store, "log"), "rb").read()
        gets = all_gets(log)
        model = Model(log).run(gets)
        for reader in BUILDS:
            other = folder("read")
            write_log(other, log)
            held(f"a log of {writer} read", reader, run(reader, other, gets), model, log)


def crash(rng):
    """Sets of keys one after another, each answered `OK`; the store killed at a moment drawn; then
    every key answered must be there with its value."""
    for build in BUILDS:
        if build == "sg interpreted":
            continue
        for sync in ("always", "batch", "never"):
            for draw in range(2 if QUICK else 5):
                store = folder("crash")
                value = lambda n: b"value-%d-" % n + bytes([97 + n % 26]) * (n % 300)
                process = subprocess.Popen(BUILDS[build] + ["--sync", sync, store], stdin=subprocess.PIPE,
                                           stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
                answered = [0]

                def feed():
                    try:
                        for n in range(10_000_000):
                            process.stdin.write(b"SET k%d " % n + value(n) + b"\n")
                            if n % 50 == 0:
                                process.stdin.flush()
                    except (BrokenPipeError, OSError, ValueError):
                        pass

                def read():
                    for line in process.stdout:
                        if line == b"OK\n":
                            answered[0] += 1

                feeder = threading.Thread(target=feed, daemon=True)
                reader = threading.Thread(target=read, daemon=True)
                feeder.start()
                reader.start()
                time.sleep(rng.uniform(0.05, 0.6))
                process.send_signal(signal.SIGKILL)
                process.wait()
                reader.join(5)
                feeder.join(5)
                # What the feeder left in the pipe's buffer has nowhere to go now.
                try:
                    process.stdin.close()
                except (BrokenPipeError, OSError):
                    pass
                count = answered[0]
                gets = b"".join(b"GET k%d\n" % n for n in range(count))
                out, code, _, err = run(build, store, gets)
                counted[0] += 1
                want = b"".join(value(n) + b"\n" for n in range(count))
                if code != 0 or out != want:
                    lost = sum(1 for got, wanted in zip(out.split(b"\n"), want.split(b"\n")) if got != wanted)
                    failures.append(f"crash ({sync}) / {build}: {count} answered, exit {code}, {lost} lost; stderr {err[:200]!r}")
                elif VERBOSE:
                    print(f"  crash ({sync}) / {build}: {count} answered, all there")


def closed_output(rng):
    store_commands = b"SET big " + b"x" * 1000 + b"\n" + b"GET big\n" * 20000
    for build in BUILDS:
        store = folder("closed")
        process = subprocess.Popen(BUILDS[build] + ["--sync", "never", store], stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        # Fed from a thread: the store writes while it reads, and a pipe holds 64 KB.
        def feed():
            try:
                process.stdin.write(store_commands)
                process.stdin.close()
            except (BrokenPipeError, OSError):
                pass
        feeder = threading.Thread(target=feed, daemon=True)
        feeder.start()
        process.stdout.read(1)
        process.stdout.close()
        process.wait(timeout=TIMEOUT)
        feeder.join(5)
        counted[0] += 1
        if process.returncode != -signal.SIGPIPE:
            failures.append(f"output closed / {build}: exit {process.returncode}, not SIGPIPE; "
                            f"stderr {process.stderr.read()[:200]!r}")


def options(rng):
    for argv, code in ((["--sync"], 2), (["--sync", "sometimes", "x"], 2), ([], 2), (["a", "b"], 2), (["-x", "a"], 2)):
        for build in BUILDS:
            done = subprocess.run(BUILDS[build] + argv, input=b"", capture_output=True, cwd=folder("options"), timeout=TIMEOUT)
            counted[0] += 1
            if done.returncode != code or done.stdout:
                failures.append(f"options {argv} / {build}: exit {done.returncode}, the model {code}")


def main():
    os.makedirs(HERE, exist_ok=True)
    rng = random.Random(1850)
    started = time.time()
    for part in (options, streams, cuts, flips, each_reads_each, closed_output, crash):
        if ONLY and part.__name__ != ONLY:
            continue
        before = counted[0], len(failures)
        part(rng)
        print(f"{part.__name__}: {counted[0] - before[0]} runs, {len(failures) - before[1]} failed "
              f"({time.time() - started:.0f} s)", flush=True)
    for failure in failures[:40]:
        print(f"  FAIL {failure}")
    print(f"{counted[0]} runs of {len(BUILDS)} builds, {len(failures)} failed")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
