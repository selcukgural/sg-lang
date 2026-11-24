#!/usr/bin/env python3
"""The builds of trials/hashsum held to a model of GNU's `sha256sum` written in Python (choice 1858):
what each writes on the standard output and its exit code must be the model's, byte for byte (the
standard error is not compared: each language words its errors its own way, but a file that cannot be
read must be told there).

- names at their edges: a line's end, a `\\`, a CR, spaces, a name that starts with `-` (after `--`),
  a name of 255 bytes, a path deep in folders, a link, a broken link, a link to a folder;
- what cannot be read: a missing file, a folder, a file no one may read (left out when run as root,
  who may read it), a FIFO a writer fills, `/dev/null`;
- the standard input: `-` alone, no file at all, `-` three times (the first takes it all);
- the pool: every `-j` from 1 to 9, 64 and 1000, more workers than files, the same file 500 times,
  5,000 files of every size from 0 to 200 KB drawn at random, a file of 300 MB;
- the output closed early (it ends by SIGPIPE), and on Linux an output that is full (`/dev/full`:
  it ends with 1 and an error, as `sha256sum` does);
- options: `-j` without a number, with 0 and with a word, an option not known: the exit code 2.

    python3 -I trials/hashsum/tests/check.py [BINARY...] [--interpreted] [--gnu SHA256SUM] [-v]

With no BINARY, the builds `python3 trials/measure.py --check hashsum` left in trials/target/hashsum.
With `--interpreted`, SG's interpreter runs the cases too (interpreter/target/release/sg). With
`--gnu`, GNU's `sha256sum` is held to the model first, on every case it takes (all but `-j`). Files
are written in trials/target/hashsum-tests.
"""
import hashlib
import os
import random
import shutil
import signal
import stat
import subprocess
import sys
import threading

REPO = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
HERE = os.path.join(REPO, "trials", "target", "hashsum-tests")
BUILT = os.path.join(REPO, "trials", "target", "hashsum")
SG = os.path.join(REPO, "interpreter", "target", "release", "sg")
SOURCE = os.path.join(REPO, "trials", "hashsum", "sg", "hashsum.sg")
TIMEOUT = 300

args = sys.argv[1:]
VERBOSE = "-v" in args
INTERPRETED = "--interpreted" in args
GNU = args[args.index("--gnu") + 1] if "--gnu" in args else None
binaries = [a for a in args if not a.startswith("-") and a != GNU]
if not binaries:
    binaries = [os.path.join(BUILT, name) for name in
                ("hashsum-sg", "hashsum-sg-checked", "hashsum-rust", "hashsum-zig-ReleaseFast", "hashsum-zig-ReleaseSafe")]
    missing = [b for b in binaries if not os.path.exists(b)]
    if missing:
        sys.exit(f"not built: {', '.join(missing)}; run `python3 trials/measure.py --check hashsum` first")
BUILDS = {os.path.basename(b): [b] for b in binaries}
if INTERPRETED:
    BUILDS["sg interpreted"] = [SG, SOURCE]
ROOT_USER = hasattr(os, "geteuid") and os.geteuid() == 0


# The model.

def line(digest, name):
    escaped = any(c in name for c in (b"\\", b"\n", b"\r"))
    if escaped:
        name = name.replace(b"\\", b"\\\\").replace(b"\n", b"\\n").replace(b"\r", b"\\r")
    return (b"\\" if escaped else b"") + digest.encode() + b"  " + name + b"\n"


def model(paths, stdin):
    """What `sha256sum` writes and its exit code, for paths given in the folder HERE."""
    out, code, read = bytearray(), 0, False
    for path in paths or [b"-"]:
        if path == b"-":
            data = b"" if read else stdin
            read = True
            out += line(hashlib.sha256(data).hexdigest(), path)
            continue
        full = os.path.join(HERE.encode(), path)
        try:
            if stat.S_ISDIR(os.stat(full).st_mode):
                code = 1
                continue
            with open(full, "rb") as f:
                digest = hashlib.sha256(f.read()).hexdigest()
            out += line(digest, path)
        except OSError:
            code = 1
    return bytes(out), code


# Running the builds.

failures = []
counted = [0]


def run(argv, stdin=b"", stdout=subprocess.PIPE):
    done = subprocess.run(argv, input=stdin, stdout=stdout, stderr=subprocess.PIPE, cwd=HERE, timeout=TIMEOUT)
    return done.stdout, done.returncode, done.stderr


def held(name, words, stdin=b"", want=None, pool=False, heavy=False):
    """Every build (and GNU's, unless the case gives `-j`) on `words`, held to the model; a heavy
    case without the interpreter, which hashes in SG and runs one task at a time."""
    paths = []
    options = True
    skip = False
    for w in words:
        if skip:
            skip = False
        elif options and w == b"--":
            options = False
        elif options and w == b"-j":
            skip = True
        else:
            paths.append(w)
    want = want or model(paths, stdin)
    builds = {b: argv for b, argv in BUILDS.items() if not (heavy and b == "sg interpreted")}
    if GNU and not pool:
        builds = {"GNU sha256sum": [GNU], **builds}
    for build, argv in builds.items():
        counted[0] += 1
        out, code, err = run(argv + words, stdin)
        problems = []
        if out != want[0]:
            problems.append(f"output: {len(out)} bytes, the model {len(want[0])}")
        if code != want[1]:
            problems.append(f"exit {code}, the model {want[1]}")
        if code == 1 and not err:
            problems.append("a file not read, and nothing on the standard error")
        if problems:
            failures.append(f"{name} / {build}: {'; '.join(problems)}; stderr {err[:200]!r}")
            if VERBOSE:
                print(f"  FAIL {failures[-1]}")
        elif VERBOSE:
            print(f"  {name} / {build}: the same")


def write(name, data):
    path = os.path.join(HERE.encode(), name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)
    return name


# The cases.

def names(rng):
    edge = [b"a line\nends", b"back\\slash", b"c\rr", b"all\\three\n\r", b" spaces  around ", b"-starts",
            b"\xc3\xa7a\xc4\x9f\xc4\xb1", b"x" * 255, b"tab\there", b"\\", b"\n"]
    for n in edge:
        write(n, n * 3)
    write(b"deep/" * 40 + b"file", b"deep")
    os.symlink(b"a line\nends", os.path.join(HERE.encode(), b"link"))
    os.symlink(b"nowhere", os.path.join(HERE.encode(), b"broken"))
    os.makedirs(os.path.join(HERE, "folder"))
    os.symlink(b"folder", os.path.join(HERE.encode(), b"to-folder"))
    held("names at their edges", [b"--", *edge, b"deep/" * 40 + b"file"])
    held("links", [b"link", b"broken", b"to-folder", b"link"])
    held("a name like an option after --", [b"--", b"-starts", b"-"], b"in")


def unreadable(rng):
    write(b"fine", b"fine")
    held("missing, a folder, fine", [b"missing", b"folder", b"fine", b"/dev/null"])
    if not ROOT_USER:
        write(b"secret", b"secret")
        os.chmod(os.path.join(HERE, "secret"), 0)
        held("a file no one may read", [b"fine", b"secret", b"fine"])
    # A FIFO a writer fills: read as it is written.
    fifo = os.path.join(HERE, "fifo")
    data = bytes(rng.randrange(256) for _ in range(300_000))
    want = (line(hashlib.sha256(b"fine").hexdigest(), b"fine") + line(hashlib.sha256(data).hexdigest(), b"fifo"), 0)
    for build, argv in BUILDS.items():
        if os.path.exists(fifo):
            os.remove(fifo)
        os.mkfifo(fifo)

        def fill():
            with open(fifo, "wb") as f:
                for at in range(0, len(data), 7000):
                    f.write(data[at:at + 7000])
                    f.flush()

        writer = threading.Thread(target=fill, daemon=True)
        writer.start()
        counted[0] += 1
        out, code, err = run(argv + [b"fine", b"fifo"])
        writer.join(10)
        if (out, code) != want:
            failures.append(f"a FIFO / {build}: exit {code}, {len(out)} bytes; stderr {err[:200]!r}")


def the_input(rng):
    data = bytes(rng.randrange(256) for _ in range(200_000))
    held("the input alone", [b"-"], data)
    held("no file at all", [], data)
    held("the input three times", [b"-", b"fine", b"-", b"-"], data)


def pool(rng):
    files = []
    for n in range(5000):
        size = rng.choice([0, 1, 63, 64, 65, 4096, 65535, 65536, 65537, rng.randrange(200_000)])
        files.append(write(b"many/%05d" % n, rng.randbytes(size)))
    want = model(files, b"")
    for j in [*range(1, 10), 64, 1000]:
        held(f"5,000 files, -j {j}", [b"-j", str(j).encode(), *files], want=want, pool=True, heavy=True)
    held("5,000 files", files, heavy=True)
    held("200 files, -j 3", [b"-j", b"3", *files[:200]], pool=True)
    held("more workers than files", [b"-j", b"64", b"fine", b"many/00001"], pool=True)
    held("one file 500 times", [b"-j", b"8"] + [b"many/00002"] * 500, pool=True, heavy=True)
    big = write(b"big", rng.randbytes(300 << 20))
    held("a file of 300 MB among small ones", [b"-j", b"4", b"fine", big, b"fine", big], pool=True, heavy=True)


def closed_output(rng):
    files = [b"many/%05d" % n for n in range(5000)]
    for build, argv in BUILDS.items():
        if build == "sg interpreted":
            continue
        counted[0] += 1
        process = subprocess.Popen(argv + files, stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=HERE)
        process.stdout.read(100)
        process.stdout.close()
        process.wait(timeout=TIMEOUT)
        if process.returncode != -signal.SIGPIPE:
            failures.append(f"output closed / {build}: exit {process.returncode}, not SIGPIPE; "
                            f"stderr {process.stderr.read()[:200]!r}")
    if os.path.exists("/dev/full"):
        for build, argv in BUILDS.items():
            counted[0] += 1
            with open("/dev/full", "wb") as full:
                out, code, err = run(argv + files, stdout=full)
            if code != 1 or not err:
                failures.append(f"a full output / {build}: exit {code}, stderr {err[:200]!r}")


def options(rng):
    for words in ([b"-j"], [b"-j", b"0", b"fine"], [b"-j", b"many", b"fine"], [b"-x", b"fine"], [b"--jobs", b"fine"]):
        for build, argv in BUILDS.items():
            counted[0] += 1
            out, code, err = run(argv + words)
            if code != 2 or out or not err:
                failures.append(f"options {words} / {build}: exit {code}, {len(out)} bytes out")


def main():
    if os.path.exists(HERE):
        for folder, dirs, _ in os.walk(HERE):
            for d in dirs:
                os.chmod(os.path.join(folder, d), 0o755)
        subprocess.run(["chmod", "-R", "u+rw", HERE], check=False)
        shutil.rmtree(HERE)
    os.makedirs(HERE)
    rng = random.Random(1858)
    for part in (names, unreadable, the_input, pool, closed_output, options):
        before = counted[0], len(failures)
        part(rng)
        print(f"{part.__name__}: {counted[0] - before[0]} runs, {len(failures) - before[1]} failed", flush=True)
    for failure in failures[:40]:
        print(f"  FAIL {failure}")
    print(f"{counted[0]} runs of {len(BUILDS)} builds{' and GNU' if GNU else ''}, {len(failures)} failed")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
