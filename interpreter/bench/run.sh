#!/bin/sh
# Compiles each program three ways and prints the best of five runs of each:
#   SG as a release build (overflow wraps), SG with overflow checked, and the C written by
#   hand. All three go through `cc -O2`, without fused multiply-add (SG's rule for fractional
#   numbers). Run from the `interpreter` folder:
#     sh bench/run.sh
set -e
cargo build --release --quiet
out=target/bench
mkdir -p "$out"

# The input of a program that reads one: `lines` reads a log of two million lines.
python3 - "$out/lines.in" <<'PY'
import random, sys
r = random.Random(7)
levels = ["INFO", "INFO", "INFO", "WARN", "ERROR", "DEBUG"]
words = ["could", "not", "reach", "the", "database", "user", "request", "took", "longer", "than", "expected", "café", "retry"]
with open(sys.argv[1], "w", encoding="utf-8", newline="") as out:
    for i in range(2000000):
        text = " ".join(r.choice(words) for _ in range(r.randint(3, 14)))
        out.write(f"{r.choice(levels)} {r.randint(1, 5000)} {text}{chr(13) if i % 5 == 0 else ''}\n")
PY
# The folder `listing` and `status` look at: ten thousand files of a few bytes.
rm -rf "$out/listing"
mkdir -p "$out/listing"
python3 - "$out/listing" <<'PY'
import os, sys
for i in range(10000):
    with open(os.path.join(sys.argv[1], f"file-{i:05}.txt"), "w") as out:
        out.write("x" * (i % 17))
PY
cp "$out/lines.in" "$out/asynclines.in"
cp "$out/lines.in" "$out/netlines.in"

best() {
    python3 - "$1" "$2" <<'PY'
import os, subprocess, sys, time
best = None
for _ in range(5):
    given = open(sys.argv[2], "rb") if os.path.exists(sys.argv[2]) else subprocess.DEVNULL
    start = time.perf_counter()
    subprocess.run([sys.argv[1]], stdin=given, stdout=subprocess.DEVNULL, check=True)
    took = time.perf_counter() - start
    best = took if best is None else min(best, took)
print(f"{best:.3f}")
PY
}

printf "%-10s %12s %12s %10s %14s\n" program "release (s)" "checked (s)" "C (s)" "release / C"
for name in fib primes collatz words list expr parse vector counts show lookup buffer bits hash nbody numbers floats fields lines copy bytes table blocks keywords classes rewrite tree tasks async asynclines tasklist asynccopy echo echosync netlines ring crc deflate clock dates rng secure listing status spawn capture pipelines pipesasync spawnasync; do
    ./target/release/sg build "bench/$name.sg" --release -o "$out/$name-release"
    # The checked build, optimised like the others, to see what the check alone costs.
    ./target/release/sg emit-c "bench/$name.sg" > "$out/$name-checked.c"
    # The libraries a program links: SG writes them on the first line of its C, and the C
    # written by hand says them in a comment on its first line.
    sglibs=$(sed -n '1s|^/\* sg-link: \(.*\) \*/$|\1|p' "$out/$name-checked.c")
    clibs=$(sed -n '1s|^/\* link: \(.*\) \*/$|\1|p' "bench/$name.c")
    cc -std=c11 -O2 -ffp-contract=off -o "$out/$name-checked" "$out/$name-checked.c" $sglibs -lm -pthread
    cc -std=c11 -O2 -ffp-contract=off -o "$out/$name-c" "bench/$name.c" $clibs -lm -pthread
    input="$out/$name.in"
    [ -e "$input" ] || input=/dev/null
    [ "$("$out/$name-release" < "$input")" = "$("$out/$name-c" < "$input")" ] || { echo "$name: the two programs print different things"; exit 1; }
    release=$(best "$out/$name-release" "$input")
    checked=$(best "$out/$name-checked" "$input")
    c=$(best "$out/$name-c" "$input")
    ratio=$(python3 -c "print(f'{$release / $c:.2f}')")
    printf "%-10s %12s %12s %10s %14s\n" "$name" "$release" "$checked" "$c" "$ratio"
done
