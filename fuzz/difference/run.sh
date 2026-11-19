#!/usr/bin/env bash
# Fuzzing for differences in the Linux container of the tests (choices 1361-1368), without the
# network:
#   fuzz/difference/run.sh 7200 [jobs] [first seed]
# The repository is given read-only and copied in; `sg` and the generator are built there. What
# is found, with its seed, goes to the volume `sg-fuzz-data`, in /data/difference/<date>/found;
# the first programs the checker refused, in /data/difference/<date>/rejected.
set -euo pipefail
seconds=${1:-7200}
jobs=${2:-6}
first=${3:-}
cd "$(dirname "$0")/../.."
podman volume exists sg-fuzz-data || podman volume create sg-fuzz-data >/dev/null
exec podman run --rm --network none -v "$PWD":/src:ro -v sg-fuzz-data:/data \
  -e SECONDS_="$seconds" -e JOBS="$jobs" -e FIRST="$first" localhost/sg-test bash -c '
set -euo pipefail
mkdir /work && cd /src
tar --exclude=./interpreter/target --exclude=./fuzz/target --exclude=./fuzz/difference/target --exclude=./graphify-out -cf - . | tar -xf - -C /work
cd /work/interpreter && cargo build --release --quiet
cd /work/fuzz/difference && cargo build --release --quiet
out=/data/difference/$(date +%Y%m%d-%H%M%S)
mkdir -p "$out"
SG=/work/interpreter/target/release/sg CC=cc ./target/release/sg-difference run "$out" "$SECONDS_" "$JOBS" $FIRST
echo "results in $out"
'
