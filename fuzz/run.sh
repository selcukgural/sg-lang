#!/usr/bin/env bash
# Runs one fuzz target in the container, without the network (choices 1350-1355):
#   fuzz/run.sh check 10800      # the target (parse, check, regex, inflate or zip) and how many seconds
# The repository is given read-only and copied in. The corpus and what is found stay in the
# volume `sg-fuzz-data`: the corpus of each target in /data/corpus/<target>, the inputs that
# fail in /data/artifacts/<target>. The first run starts the corpus from the programs of
# `interpreter/tests/cases` and `interpreter/examples`, or, for `regex`, from the patterns of
# RE2's search tests, each with a line break after it, or, for `inflate`, from those programs
# compressed by gzip, each after the byte that says gzip, or, for `zip`, from the archives of
# `interpreter/tests/zip-suite`. JOBS sets how many run at once (6).
set -euo pipefail
target=${1:?the target: parse, check, regex, inflate or zip}
seconds=${2:-3600}
jobs=${JOBS:-6}
case $target in parse|check|regex|inflate|zip) ;; *) echo "the target is parse, check, regex, inflate or zip" >&2; exit 2 ;; esac
cd "$(dirname "$0")/.."
podman volume exists sg-fuzz-data || podman volume create sg-fuzz-data >/dev/null
exec podman run --rm --network none -v "$PWD":/src:ro -v sg-fuzz-data:/data \
  -e TARGET="$target" -e SECONDS_="$seconds" -e JOBS="$jobs" localhost/sg-fuzz bash -c '
set -euo pipefail
mkdir /work && cd /src
tar --exclude=./interpreter/target --exclude=./fuzz/target --exclude=./graphify-out --exclude=./trials/target -cf - . | tar -xf - -C /work
corpus=/data/corpus/$TARGET
mkdir -p "$corpus" /data/artifacts/$TARGET
if [ -z "$(ls -A "$corpus")" ] && [ "$TARGET" = regex ]; then
  i=0
  while IFS= read -r line; do i=$((i + 1)); printf "%s\nabc 123 aaa\n" "${line:1:${#line}-2}" > "$corpus/seed-$i"; done \
    < <(grep "^\"" /work/interpreter/tests/regex-suite/re2-search.txt)
elif [ -z "$(ls -A "$corpus")" ] && [ "$TARGET" = inflate ]; then
  i=0
  while IFS= read -r -d "" file; do i=$((i + 1)); { printf "\\002"; gzip -9c "$file"; } > "$corpus/seed-$i"; done \
    < <(find /work/interpreter/tests/cases -name "ok_*.sg" -size -8k -print0)
elif [ -z "$(ls -A "$corpus")" ] && [ "$TARGET" = zip ]; then
  cp /work/interpreter/tests/zip-suite/go/*.zip /work/interpreter/tests/zip-suite/made/*.zip \
    /work/interpreter/tests/zip-suite/others/*.zip "$corpus/"
elif [ -z "$(ls -A "$corpus")" ]; then
  i=0
  while IFS= read -r -d "" file; do i=$((i + 1)); cp "$file" "$corpus/seed-$i.sg"; done \
    < <(find /work/interpreter/tests/cases /work/interpreter/examples -name "*.sg" -print0)
fi
cd /work/interpreter
CARGO_TARGET_DIR=/data/target cargo fuzz run --fuzz-dir ../fuzz --release --debug-assertions "$TARGET" "$corpus" -- \
  -dict=../fuzz/sg.dict -timeout=10 -rss_limit_mb=2048 -max_len=65536 -fork="$JOBS" \
  -ignore_crashes=1 -ignore_timeouts=1 -ignore_ooms=1 -max_total_time="$SECONDS_" \
  -artifact_prefix=/data/artifacts/$TARGET/ -print_final_stats=1
'
