#!/usr/bin/env python3
"""The differential test of SG's `Regex` against Rust's `regex` and PCRE2 (choice 1841), run by hand
when `Regex` changes: builds the three programs (SG with `interpreter/target/release/sg`, Rust with
cargo, Zig with PCRE2), makes the cases with `gen.py`, runs them and compares with `compare.py`.
`run.py [--quick]`: four seeds of 500,000 short cases, 30,000 long ones and two seeds of 500,000 in
bytes; `--quick` one of each, of 50,000. Its files are in `interpreter/target/regex-differ`. The exit
code is 1 when SG differs where Rust and PCRE2 agree."""
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
INTERPRETER = HERE.parents[2]
OUT = INTERPRETER / "target" / "regex-differ"
SG = INTERPRETER / "target" / "release" / ("sg.exe" if os.name == "nt" else "sg")
# The longest a program may take on one set of cases: far more than it does (about a minute for
# 500,000), so that one that hangs fails saying which.
LIMIT = 1800


def run(command, **kw):
    result = subprocess.run([str(c) for c in command], capture_output=True, text=True, **kw)
    if result.returncode != 0:
        sys.exit(f"failed: {' '.join(map(str, command))}\n{result.stdout}{result.stderr}")
    return result


def build():
    OUT.mkdir(parents=True, exist_ok=True)
    if not SG.exists():
        run(["cargo", "build", "--release"], cwd=INTERPRETER)
    programs = {"sg": OUT / "differ-sg"}
    run([SG, "build", HERE / "sg" / "differ.sg", "--release", "-o", programs["sg"]])
    run(["cargo", "build", "--release", "--quiet", "--target-dir", OUT / "cargo"], cwd=HERE / "rust")
    programs["rust"] = OUT / "cargo" / "release" / ("differ.exe" if os.name == "nt" else "differ")
    run(["zig", "build", "-Doptimize=ReleaseFast", "--prefix", OUT / "zig", "--cache-dir", OUT / "zig-cache"], cwd=HERE / "zig")
    programs["pcre"] = OUT / "zig" / "bin" / "differ"
    pcre = shutil.which("pcre2-config")
    version = run([pcre, "--version"]).stdout.strip() if pcre else "unknown"
    lock = (HERE / "rust" / "Cargo.lock").read_text()
    regex = lock.split('name = "regex"\nversion = "', 1)[1].split('"', 1)[0]
    print(f"SG {SG}, Rust regex {regex}, PCRE2 {version}", flush=True)
    return programs


def one(programs, name, seed, count, extra):
    prefix = OUT / name
    run([sys.executable, HERE / "gen.py", seed, count, prefix, *extra])
    bytes_mode = ["bytes"] if "--bytes" in extra else []
    took = {}
    for kind, program in programs.items():
        start = time.monotonic()
        with open(f"{prefix}-{kind}.txt", "rb") as given, open(f"{prefix}-{kind}.out", "wb") as written:
            try:
                done = subprocess.run([program, *bytes_mode], stdin=given, stdout=written, timeout=LIMIT)
            except subprocess.TimeoutExpired:
                sys.exit(f"{kind} on {name} has run past {LIMIT} s: it hangs")
        if done.returncode != 0:
            sys.exit(f"{kind} on {name} ended with {done.returncode}")
        took[kind] = time.monotonic() - start
    print(f"{name}: {count:,} cases, seed {seed}; " + ", ".join(f"{k} {t:.1f} s" for k, t in took.items()), flush=True)
    compare = [sys.executable, HERE / "compare.py", prefix] + (["--skip-empty"] if bytes_mode else [])
    return subprocess.run([str(c) for c in compare]).returncode == 0


def main():
    quick = "--quick" in sys.argv
    big = 50_000 if quick else 500_000
    plan = [(f"short-{s}", s, big, []) for s in ([1] if quick else [1, 2, 3, 4])]
    plan.append(("long", 9, 3_000 if quick else 30_000, ["--long", "2000"]))
    plan += [(f"bytes-{s}", s, big, ["--bytes"]) for s in ([11] if quick else [11, 12])]
    programs = build()
    passed = [one(programs, name, seed, count, extra) for name, seed, count, extra in plan]
    if not all(passed):
        sys.exit("SG differs where Rust's regex and PCRE2 agree")
    print("SG agrees wherever the two agree")


main()
