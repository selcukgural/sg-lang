#!/usr/bin/env python3
"""Compares what SG, Rust's `regex` and PCRE2 wrote for the same cases (choice 1841):
`compare.py PREFIX [--skip-empty] [--shown N]` reads PREFIX-{sg,rust,pcre}.txt and .out. A case
where Rust and PCRE2 agree and SG does not, or where both compile a pattern SG refuses, is a
failure: it is shown, and the exit code is 1. The others are counted by who stands apart, each a
known way of its engine (see README.md). With `--skip-empty`, for bytes, the empty matches of
`findAll` are left out, which each places its own way where the bytes are not UTF-8 or inside a
character (Rust's `regex::bytes`)."""
import sys

args = [a for a in sys.argv[1:] if not a.startswith("--")]
prefix = args[0]
skip_empty = "--skip-empty" in sys.argv
shown = int(sys.argv[sys.argv.index("--shown") + 1]) if "--shown" in sys.argv else 10
cases = open(f"{prefix}-sg.txt").read().splitlines()
outs = [open(f"{prefix}-{k}.out").read().splitlines() for k in ("sg", "rust", "pcre")]
for k, out in zip(("sg", "rust", "pcre"), outs):
    if len(out) != len(cases):
        sys.exit(f"{prefix}-{k}.out has {len(out)} lines for {len(cases)} cases")


def norm(line):
    """A line with the message of an error left out, and empty matches too with `--skip-empty`."""
    if line.startswith("E"):
        return "E"
    if not skip_empty or " |" not in line:
        return line
    head, groups = line.split(" |", 1)
    flag, _, spans = head.partition(" ")
    kept = [s for s in spans.split(",") if s and s.split("-")[0] != s.split("-")[1]]
    return f"{flag} {','.join(kept)} |{groups}"


counts = {}
failures = []
for i in range(len(cases)):
    s, r, p = (norm(out[i]) for out in outs)
    if s == r == p:
        key = "same"
    elif r == p and r == "E":
        key = "SG compiles what both refuse (their limits of size)"
    elif r == p and s == "E":
        key = "FAIL: SG refuses what both compile"
    elif r == p:
        key = "FAIL: SG alone"
    elif s == r:
        key = "PCRE2 alone"
    elif s == p:
        key = "Rust alone"
    else:
        key = "each its own"
    counts[key] = counts.get(key, 0) + 1
    if key.startswith("FAIL"):
        failures.append(i)

for i in failures[:shown]:
    pattern, text = (bytes.fromhex(h) for h in cases[i].split(" "))
    print(f"{pattern.decode()!r} on {text[:80]!r}")
    for k, out in zip(("sg", "rust", "pcre"), outs):
        print(f"   {k:4}: {out[i][:200]}")
for key in sorted(counts, key=lambda k: (k != "same", k)):
    print(f"  {counts[key]:>9,}  {key}")
sys.exit(1 if failures else 0)
