#!/usr/bin/env python3
"""Builds each trial in SG, Rust and Zig, checks that the three give the same output, and measures
them (choices 1418, 1525-1532, 1540-1542). Run from the repository's root:

    python3 trials/measure.py               every trial: checked, then measured
    python3 trials/measure.py grep          only this one
    python3 trials/measure.py --check       only checked, nothing measured (what Linux runs)
    python3 trials/measure.py --runs 1      one run of each instead of the best of five

For each case the standard output and the exit code of the three must be the same, byte for byte;
the standard error is not compared, since each language words its errors its own way. Measured:
the time (one run to warm up, then the best of five), the most memory held (the peak resident set,
from `wait4`), the size of the program once stripped, and its lines that are neither blank nor
only a comment. SG is built with `sg build --release`, and once more with its arithmetic checked:
the C of `sg emit-c` compiled with `-O2` as `--release` compiles it (`sg build` alone compiles with
`-O0`, for a debugger); Rust with `cargo build --release`; Zig twice, `ReleaseFast` and `ReleaseSafe`
(which keeps its checks of bounds and overflow), with `zig build` where the trial has a `build.zig` (a
library of C to link) and `zig build-exe` where it has not. Everything built and every input goes to
`trials/target`.
"""

import argparse
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TRIALS = ROOT / "trials"
OUT = TRIALS / "target"
SG = ROOT / "interpreter" / "target" / "release" / "sg"
MACOS = sys.platform == "darwin"

# The text of `inputs/text.sg`: megabytes and seed.
TEXT_MB = 100
TEXT_SEED = 1


def text_input():
    return OUT / f"text-{TEXT_MB}.log"


def small_input():
    return OUT / "text-2.log"


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
}


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
    sg_source = folder / "sg" / f"{trial}.sg"
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
    return built


def expand(words, trial):
    values = {
        "text": str(text_input()),
        "small": str(small_input()),
        "empty": str(OUT / "empty.txt"),
        "readme": str(ROOT / "README.md"),
    }
    return [w.format(**values) if "{" in w and "}" in w and w.strip("{}") in values else w for w in words]


def run_once(program, args, stdin_path, capture):
    """Runs a program once: (exit code, standard output or None, seconds, peak memory in bytes)."""
    stdin = open(stdin_path, "rb") if stdin_path else subprocess.DEVNULL
    stdout = subprocess.PIPE if capture else subprocess.DEVNULL
    start = time.perf_counter()
    process = subprocess.Popen([program, *args], stdin=stdin, stdout=stdout, stderr=subprocess.DEVNULL)
    output = process.stdout.read() if capture else None
    _, status, usage = os.wait4(process.pid, 0)
    seconds = time.perf_counter() - start
    process.returncode = os.waitstatus_to_exitcode(status)
    if stdin_path:
        stdin.close()
    # macOS gives bytes, Linux kilobytes.
    peak = usage.ru_maxrss if MACOS else usage.ru_maxrss * 1024
    return process.returncode, output, seconds, peak


def check(trial, built, cases):
    """Every build must give the output and the exit code of the first."""
    ok = True
    for name, words, stdin in cases:
        args = expand(words, trial)
        stdin_path = expand([stdin], trial)[0] if stdin else None
        results = {build: run_once(path, args, stdin_path, True)[:2] for build, path in built.items()}
        first = next(iter(results.values()))
        for build, result in results.items():
            if result != first:
                ok = False
                print(f"  DIFFERENT {trial} / {name}: {build} gives exit {result[0]} and "
                      f"{len(result[1])} bytes, SG gives exit {first[0]} and {len(first[1])} bytes")
    print(f"  {trial}: {len(cases)} cases, {'the same output from every build' if ok else 'NOT the same'}")
    return ok


def measure(trial, built, cases, runs):
    rows = []
    for name, words, stdin in cases:
        args = expand(words, trial)
        stdin_path = expand([stdin], trial)[0] if stdin else None
        row = {}
        for build, path in built.items():
            run_once(path, args, stdin_path, False)
            best_time, best_peak = None, None
            for _ in range(runs):
                _, _, seconds, peak = run_once(path, args, stdin_path, False)
                best_time = seconds if best_time is None else min(best_time, seconds)
                best_peak = peak if best_peak is None else min(best_peak, peak)
            row[build] = (best_time, best_peak)
        rows.append((name, row))
    return rows


def stripped_size(path):
    copy = path.with_name(path.name + "-stripped")
    fresh_copy(path, copy)
    subprocess.run(["strip", copy], check=True, capture_output=True)
    return copy.stat().st_size


def lines(folder, suffix):
    """Lines that are neither blank nor only a comment."""
    count = 0
    for path in sorted(folder.rglob(f"*{suffix}")):
        if "target" in path.parts or path.name == "build.zig" or ".zig-cache" in path.parts:
            continue
        for line in path.read_text(encoding="utf-8").splitlines():
            text = line.strip()
            if text and not text.startswith("//"):
                count += 1
    return count


def report(trial, built, rows):
    folder = TRIALS / trial
    builds = list(built)
    print(f"\n### {trial}\n")
    print("| Case | " + " | ".join(builds) + " |")
    print("|---|" + "---|" * len(builds))
    for name, row in rows:
        cells = [f"{row[b][0]:.3f} s, {row[b][1] / 1048576:.1f} MB" for b in builds]
        print(f"| {name} | " + " | ".join(cells) + " |")
    sizes = {b: stripped_size(p) for b, p in built.items()}
    print("| Size, stripped | " + " | ".join(f"{sizes[b] / 1024:.0f} KB" for b in builds) + " |")
    counts = {
        "SG": lines(folder / "sg", ".sg"),
        "SG checked": lines(folder / "sg", ".sg"),
        "Rust": lines(folder / "rust", ".rs"),
        "Zig": lines(folder / "zig", ".zig"),
        "Zig safe": lines(folder / "zig", ".zig"),
    }
    print("| Lines | " + " | ".join(str(counts[b]) for b in builds) + " |")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("trials", nargs="*", help="the trials to run (all when none)")
    parser.add_argument("--check", action="store_true", help="only check that the outputs are the same")
    parser.add_argument("--runs", type=int, default=5, help="runs of each case, the best kept")
    a = parser.parse_args()
    trials = a.trials or list(TRIALS_CASES)
    build_sg()
    make_inputs()
    all_ok = True
    for trial in trials:
        cases = TRIALS_CASES[trial]
        built = build(trial)
        all_ok &= check(trial, built, cases["check"] + cases["measure"])
        if all_ok and not a.check:
            report(trial, built, measure(trial, built, cases["measure"], a.runs))
    sys.exit(0 if all_ok else 1)


if __name__ == "__main__":
    main()
