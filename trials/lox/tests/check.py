#!/usr/bin/env python3
"""Holds builds of `lox` to the tests of "Crafting Interpreters" and to clox (choice 1888). Run from
the repository's root:

    python3 -I trials/lox/tests/check.py [--interpreted] [--stress-gc] [--ip-local] [--only DIR] BINARY...

Each test of the book that clox runs (all of `test/` but `scanning/`, `expressions/` and
`benchmark/`, as its own runner leaves them out for clox) is run by clox and by each build. Each must
do what the comments of the test ask (`// expect: ...`, `// Error at ...`, `// expect runtime error:
...`) as the book's runner checks it, and each build must write what clox writes, byte for byte, on
the output and the standard error, and end with its code. `--stress-gc` runs each build a second time
with the collector at every allocation, `--ip-local` once more with the loop of choice 1894 (a build that
is not SG, such as `clox-ip-local`, is named as it is); `--interpreted` adds the SG version run by the interpreter
(`interpreter/target/release/sg`). The extra programs of `trials/lox/tests/programs/` are run the same
way, held to clox only, and what clox cannot do, an expression nested deeper than its C stack takes,
is held to its answer or to SG's panic (`beyond_clox`); with its output closed after the first bytes a build
must end as clox does (`closed_pipe`).
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE))
import clox  # noqa: E402

LEFT_OUT = {"scanning", "expressions", "benchmark"}
# Programs of `programs/` run compiled only: the collector at every allocation makes a million objects
# take hours (as in clox built with `DEBUG_STRESS_GC`), and the interpreter takes three minutes.
LARGE = {"objects.lox"}

EXPECT_OUTPUT = re.compile(rb"// expect: ?(.*)")
EXPECT_ERROR = re.compile(rb"// (Error.*)")
ERROR_LINE = re.compile(rb"// \[((java|c) )?line (\d+)\] (Error.*)")
EXPECT_RUNTIME_ERROR = re.compile(rb"// expect runtime error: (.+)")
SYNTAX_ERROR = re.compile(rb"\[.*line (\d+)\] (Error.+)")
STACK_TRACE = re.compile(rb"\[line (\d+)\]")
NON_TEST = re.compile(rb"// nontest")


def run(cmd, timeout=120):
    try:
        r = subprocess.run(cmd, capture_output=True, timeout=timeout)
        return r.stdout, r.stderr, r.returncode
    except subprocess.TimeoutExpired:
        return b"", b"timed out\n", "timeout"


def expectations(path):
    """What the comments of the test ask, as the book's runner reads them; None for a non-test."""
    output, errors, runtime, runtime_line, code = [], set(), None, 0, 0
    for number, line in enumerate(path.read_bytes().split(b"\n"), 1):
        if NON_TEST.search(line):
            return None
        m = EXPECT_OUTPUT.search(line)
        if m:
            output.append(m.group(1))
            continue
        m = EXPECT_ERROR.search(line)
        if m:
            errors.add(b"[%d] " % number + m.group(1))
            code = 65
            continue
        m = ERROR_LINE.search(line)
        if m:
            if m.group(2) in (None, b"c"):
                errors.add(b"[" + m.group(3) + b"] " + m.group(4))
                code = 65
            continue
        m = EXPECT_RUNTIME_ERROR.search(line)
        if m:
            runtime, runtime_line, code = m.group(1), number, 70
    return output, errors, runtime, runtime_line, code


def held_to_book(expected, out, err, code):
    """What differs from the test's comments, as the book's runner says it; empty when nothing."""
    output, errors, runtime, runtime_line, want = expected
    problems = []
    lines = err.replace(b"\r\n", b"\n").split(b"\n")
    if lines and lines[-1] == b"":
        lines.pop()
    if runtime is not None:
        if not lines or lines[0] != runtime:
            problems.append(f"expected runtime error {runtime!r}, got {lines[:1]!r}")
        elif len(lines) < 2 or not STACK_TRACE.search(lines[1]):
            problems.append("expected a stack trace")
        elif int(STACK_TRACE.search(lines[1]).group(1)) != runtime_line:
            problems.append(f"runtime error on the wrong line: {lines[1]!r}, expected {runtime_line}")
    else:
        found = set()
        for line in lines:
            m = SYNTAX_ERROR.search(line)
            if m:
                found.add(b"[" + m.group(1) + b"] " + m.group(2))
            elif line:
                problems.append(f"unexpected error output {line!r}")
        if found != errors:
            problems.append(f"compile errors {sorted(found)!r}, expected {sorted(errors)!r}")
    if code != want:
        problems.append(f"exit code {code}, expected {want}")
    got = out.replace(b"\r\n", b"\n").split(b"\n")
    if got and got[-1] == b"":
        got.pop()
    if got != output:
        problems.append(f"output {got[:5]!r}..., expected {output[:5]!r}...")
    return problems


def beyond_clox(cmd, folder):
    """Where clox is no reference, as it ends on a signal: an expression nested 200,000 levels, which
    clox's compiler takes on the C stack until it is cut off (`SIGSEGV`), must be compiled and printed,
    and one nested 5,000,000 levels must end with SG's panic of a stack too deep, never a signal."""
    problems = []
    for depth, want in ((200_000, (b"1\n", 0)), (5_000_000, (b"", 1))):
        path = folder / f"nested-{depth}.lox"
        path.write_text("print " + "(" * depth + "1" + ")" * depth + ";\n")
        out, err, code = run(cmd + [str(path)], timeout=300)
        if (out, code) != want or (code == 1 and not err.startswith(b"panic: stack overflow")):
            problems.append(f"{' '.join(cmd[-2:])} nested {depth}: exit {code}, {out[:20]!r}, {err[:120]!r}")
    return problems


def closed_pipe(cmd, reference, folder):
    """A program printing a million lines into a pipe that is closed after the first bytes: the build
    must end as clox does (killed by `SIGPIPE` where the system sends it) and write nothing on the
    standard error."""
    path = folder / "many-lines.lox"
    path.write_text("var i = 0;\nwhile (i < 1000000) { print i; i = i + 1; }\n")
    ends = []
    for c in (reference, cmd):
        p = subprocess.Popen(c + [str(path)], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        first = p.stdout.read(10)
        p.stdout.close()
        err = p.stderr.read()
        ends.append((first, p.wait(timeout=120), err))
    if ends[0] != ends[1]:
        return [f"{' '.join(cmd[-2:])} with its output closed: {ends[1]!r}, clox {ends[0]!r}"]
    return []


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--interpreted", action="store_true")
    parser.add_argument("--stress-gc", action="store_true")
    parser.add_argument("--ip-local", action="store_true")
    parser.add_argument("--only", help="a folder of the tests, such as `closure`")
    parser.add_argument("--clox", help="clox to hold the builds to (built here when not given)")
    parser.add_argument("builds", nargs="*")
    args = parser.parse_args()
    reference = args.clox or str(clox.build())
    builds = [[b] for b in args.builds]
    if args.interpreted:
        sg = str(ROOT / "interpreter" / "target" / "release" / "sg")
        builds.append([sg, str(ROOT / "trials" / "lox" / "sg")])
    runs = []
    for b in builds:
        runs.append(b)
        sg = "clox" not in Path(b[0]).name
        if args.stress_gc and sg:
            runs.append(b + ["--stress-gc"])
        if args.ip_local and sg:
            runs.append(b + ["--ip-local"])
            if args.stress_gc:
                runs.append(b + ["--ip-local", "--stress-gc"])

    folder = clox.tests()
    tests = []
    for path in sorted(folder.rglob("*.lox")):
        part = path.relative_to(folder).parts
        if len(part) > 1 and part[0] in LEFT_OUT:
            continue
        if args.only and part[0] != args.only:
            continue
        tests.append((path, expectations(path)))
    extra = HERE / "programs"
    if not args.only and extra.exists():
        tests += [(path, None) for path in sorted(extra.glob("*.lox"))]

    problems = 0
    checked = 0
    for path, expected in tests:
        name = path.relative_to(path.parents[1])
        want = run([reference, str(path)])
        if expected is not None:
            for p in held_to_book(expected, *want):
                print(f"clox {name}: {p}")
                problems += 1
        for cmd in runs:
            if path.name in LARGE and path.parent == extra and ("--stress-gc" in cmd or cmd[0].endswith("/sg")):
                continue
            got = run(cmd + [str(path)])
            checked += 1
            if expected is not None:
                for p in held_to_book(expected, *got):
                    print(f"{' '.join(cmd[-2:])} {name}: {p}")
                    problems += 1
            for what, a, b in zip(("output", "error", "exit code"), want, got):
                if a != b:
                    print(f"{' '.join(cmd[-2:])} {name}: {what} differs from clox: {b!r:.300} != {a!r:.300}")
                    problems += 1
    folder = ROOT / "trials" / "target" / "lox"
    folder.mkdir(parents=True, exist_ok=True)
    for cmd in runs:
        if "clox" not in Path(cmd[0]).name and not cmd[0].endswith("/sg") and "--stress-gc" not in cmd:
            for p in beyond_clox(cmd, folder) + closed_pipe(cmd, [reference], folder):
                print(p)
                problems += 1
    print(f"{len(tests)} programs, {checked} runs, {problems} problem(s)")
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()
