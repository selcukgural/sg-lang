#!/bin/sh
# Compiles each program three ways and prints the best of five runs of each:
#   SG as a release build (overflow wraps), SG with overflow checked, and the C written by
#   hand. All three go through `cc -O2`. Run from the `interpreter` folder:
#     sh bench/run.sh
set -e
cargo build --release --quiet
out=target/bench
mkdir -p "$out"

best() {
    python3 - "$1" <<'PY'
import subprocess, sys, time
best = None
for _ in range(5):
    start = time.perf_counter()
    subprocess.run([sys.argv[1]], stdout=subprocess.DEVNULL, check=True)
    took = time.perf_counter() - start
    best = took if best is None else min(best, took)
print(f"{best:.3f}")
PY
}

printf "%-10s %12s %12s %10s %14s\n" program "release (s)" "checked (s)" "C (s)" "release / C"
for name in fib primes collatz words list expr parse vector counts show lookup buffer bits; do
    ./target/release/sg build "bench/$name.sg" --release -o "$out/$name-release"
    # The checked build, optimised like the others, to see what the check alone costs.
    ./target/release/sg emit-c "bench/$name.sg" > "$out/$name-checked.c"
    cc -std=c11 -O2 -o "$out/$name-checked" "$out/$name-checked.c"
    cc -std=c11 -O2 -o "$out/$name-c" "bench/$name.c"
    [ "$("$out/$name-release")" = "$("$out/$name-c")" ] || { echo "$name: the two programs print different things"; exit 1; }
    release=$(best "$out/$name-release")
    checked=$(best "$out/$name-checked")
    c=$(best "$out/$name-c")
    ratio=$(python3 -c "print(f'{$release / $c:.2f}')")
    printf "%-10s %12s %12s %10s %14s\n" "$name" "$release" "$checked" "$c" "$ratio"
done
