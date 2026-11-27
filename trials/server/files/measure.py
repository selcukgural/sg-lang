#!/usr/bin/env python3
"""The file servers, measured (choices 1861-1871): SG, Rust, Zig and C under one load, `../load/load.c`
with `files`, on Linux, with nginx beside them and C sending with `sendfile` as a reference. Run from
the repository's root:

    python3 trials/server/files/measure.py              every case, on Linux in the image `sg-server`
    python3 trials/server/files/measure.py --native     on this machine, as it is
    python3 trials/server/files/measure.py --check      each server held to `check.py` and to the load's
                                                        `--check`, quickly, nothing measured
    python3 trials/server/files/measure.py --only files-1k-50,files-404 --runs 3

The machinery is `../measure.py`'s: each server afresh on cores 0-3, the load on 4-7 with four
threads, the memory and the time of the processor read from `/proc`, a run that hangs stopped with
what the server was doing written down. nginx is a master and its workers, all counted.

The files are written before anything is measured, into a folder of the machine the servers run on
(not the repository): byte O of file F is (7F + O) mod 251, which the load checks every answer
against. They are read once before each case, so that every server is measured from the cache of
the system, as a file server serving the same files again is.

The cases were chosen before any number was seen (1869):
"""

import argparse
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("lane", HERE.parent / "measure.py")
lane = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lane)

ROOT, BIN, OUT, LINUX, SG = lane.ROOT, lane.BIN, lane.OUT, lane.LINUX, lane.SG
# Zig twice (choice 1872's B): on `Io.Threaded`, its default, and on `Io.Evented`, its io_uring
# (`zig/uring.zig`), the fastest form of it on Linux, when Zig's library compiles it
LANGUAGES = ["sg", "rust", "zig", "zig-uring", "c"]
# measured beside the four, not one of them: nginx as it is run (sendfile on), C with sendfile, and
# SG's server opening its files through a `DirectoryRoot` (`--inside 1`, choice 2072), which follows
# no link out of the root: `check.py` holds the four to following one, so it is not held to it
BESIDE = ["c-sendfile", "nginx", "sg-root"]
# what a server is started with after its root
EXTRA = {"sg-root": ["--inside", "1"]}
GB = 1 << 30

# name -> the root it reads (count, size) and what the load is given
SETS = {"1k": (1000, 1024), "64k": (100, 65536), "1m": (20, 1 << 20), "many": (10000, 4096), "big": (1, GB)}


def cases():
    c = []
    for label, size in [("1k", "1 KB"), ("64k", "64 KB"), ("1m", "1 MB")]:
        for conns in [50, 1000]:
            c.append({"name": f"files-{label}-{conns}", "set": label, "conns": conns,
                      "what": f"{SETS[label][0]:,} files of {size}, {conns:,} connections"})
    c.append({"name": "files-1g", "set": "big", "conns": 1, "duration": 20, "what": "a file of 1 GB, one client"})
    c.append({"name": "files-range-4k", "set": "big", "conns": 50, "range": 4096,
              "what": "4 KB at random places of the file of 1 GB, 50 connections"})
    c.append({"name": "files-304", "set": "1k", "conns": 50, "since": True, "what": "304 Not Modified, 50 connections"})
    c.append({"name": "files-many", "set": "many", "conns": 50, "what": "10,000 files of 4 KB, 50 connections"})
    c.append({"name": "files-404", "set": "1k", "conns": 50, "missing": True, "what": "404 Not Found, 50 connections"})
    return c


__doc__ += "".join(f"    {c['name']:16} {c['what']}\n" for c in cases())


# ---- the files

def write_sets(top, sets):
    """Each set into `top/NAME/f/`, the bytes the load expects, made again only when missing."""
    period = bytes(i % 251 for i in range(251))
    block = period * (4 * 1024 * 1024 // 251 + 2)
    for name in sets:
        count, size = SETS[name]
        folder = top / name / "f"
        done = folder / ".done"
        if done.exists():
            continue
        folder.mkdir(parents=True, exist_ok=True)
        for k in range(count):
            start = (7 * k) % 251
            with open(folder / f"{k:06d}.bin", "wb") as f:
                left, at = size, start
                while left:
                    n = min(left, 4 * 1024 * 1024)
                    f.write(block[at:at + n])
                    at = (at + n) % 251
                    left -= n
        done.write_text("")
    return top


def warm(folder):
    """Every file read once, so that every server is measured from the cache."""
    for path in sorted(folder.rglob("*.bin")):
        with open(path, "rb") as f:
            while f.read(8 * 1024 * 1024):
                pass


# ---- building

def build(languages):
    BIN.mkdir(parents=True, exist_ok=True)
    lane.run(["cc", "-O2", "-Wall", "-pthread", "-o", str(BIN / "load"), str(HERE.parent / "load" / "load.c"), "-lm"])
    bins = {}
    for lang in languages:
        out = BIN / f"files-{lang}"
        if lang in ("sg", "sg-root"):
            lane.run([str(SG), "build", str(HERE / "sg" / "files.sg"), "-o", str(out), "--release"])
        elif lang == "c":
            lane.run(["cc", "-O2", "-Wall", "-pthread", "-o", str(out), str(HERE / "c" / "files.c")])
        elif lang == "c-sendfile":
            if not LINUX:
                continue
            lane.run(["cc", "-O2", "-Wall", "-pthread", "-DSENDFILE", "-o", str(out), str(HERE / "c" / "files.c")])
        elif lang == "rust":
            target = BIN / "cargo"
            lane.run(["cargo", "build", "--release", "--quiet", "--manifest-path", str(HERE / "rust" / "Cargo.toml")],
                     env={**os.environ, "CARGO_TARGET_DIR": str(target)})
            shutil.copy(target / "release" / "files", out)
        elif lang == "zig":
            lane.run(["zig", "build-exe", "-OReleaseFast", str(HERE / "zig" / "files.zig"), f"-femit-bin={out}"], cwd=BIN)
        elif lang == "zig-uring":
            if not LINUX:
                continue
            # Zig 0.17.1's `Io.Uring` does not compile (see `zig/uring.zig`): said, and left out
            made = subprocess.run(["zig", "build-exe", "-OReleaseFast", str(HERE / "zig" / "uring.zig"), f"-femit-bin={out}"],
                                  cwd=BIN, capture_output=True, text=True)
            if made.returncode != 0:
                error = next((l for l in made.stderr.splitlines() if "error:" in l), made.stderr.strip()[:300])
                print(f"# zig-uring is not measured: Zig's library does not compile it: {error}", flush=True)
                continue
        elif lang == "nginx":
            if not shutil.which("nginx"):
                continue
            out = BIN / "files-nginx"
            out.write_text(NGINX_WRAPPER)
            out.chmod(0o755)
        bins[lang] = out
    return bins


# nginx as it is run to serve files: a worker on each core of the server's four, each with room for
# 4,096 connections (it lays out every one when it starts, so a room far past the cases' 1,000 would
# be measured as its memory), its workers as the user of its master (so that the memory of each can
# be read: a worker of another user's hides its `smaps_rollup`), sendfile and
# tcp_nopush on, no ETag (the four write none), the types of the four's table; `If-Modified-Since`
# taken as the four take it (`before`: nginx's own default is `exact`, a date equal to the file's),
# and a 404 whose body is the four's, which the load holds each answer's length to. Started as
# `files-nginx PORT ROOT`, as the others are.
NGINX_WRAPPER = r"""#!/bin/sh
dir=$(mktemp -d)
cat > "$dir/nginx.conf" <<EOF
daemon off; master_process on; worker_processes 4; user root root; pid $dir/nginx.pid; error_log /dev/null crit;
worker_rlimit_nofile 524288;
events { worker_connections 4096; }
http {
  access_log off; sendfile on; tcp_nopush on; tcp_nodelay on; etag off; server_tokens off;
  keepalive_requests 1000000; keepalive_timeout 65; if_modified_since before;
  types { text/html html htm; text/css css; text/javascript js mjs; application/json json; text/plain txt;
    application/xml xml; image/svg+xml svg; image/png png; image/jpeg jpg jpeg; image/gif gif; image/webp webp;
    image/x-icon ico; application/wasm wasm; application/pdf pdf; application/zip zip; application/gzip gz;
    video/mp4 mp4; video/webm webm; audio/mpeg mp3; font/woff2 woff2; font/woff woff; }
  default_type application/octet-stream;
  server { listen $1 reuseport; root $2; autoindex off; error_page 404 = @missing;
    location @missing { default_type text/plain; return 404 "not found"; } }
}
EOF
exec nginx -c "$dir/nginx.conf" -p "$dir"
"""


class Tree(lane.Server):
    """A server of several processes (nginx): time summed over the process and its children, and memory
    as the sum of their proportional sets (Pss), so that the pages they share are counted once; Pss has
    no peak, so the most held is the same sum."""

    def pids(self):
        out = [self.pid]
        try:
            for task in Path(f"/proc/{self.pid}/task").iterdir():
                out += [int(p) for p in (task / "children").read_text().split()]
        except OSError:
            pass
        return out

    def status(self):
        total = {"rss_kb": 0, "hwm_kb": 0, "threads": 0}
        for pid in self.pids():
            try:
                fields = dict(line.split(":", 1) for line in Path(f"/proc/{pid}/status").read_text().splitlines() if ":" in line)
            except OSError:
                continue
            rollup = Path(f"/proc/{pid}/smaps_rollup").read_text()
            pss = next(int(line.split()[1]) for line in rollup.splitlines() if line.startswith("Pss:"))
            total["rss_kb"] += pss
            total["hwm_kb"] += pss
            total["threads"] += int(fields["Threads"])
        return total

    def cpu_seconds(self):
        return sum(lane.cpu_of(pid) or 0 for pid in self.pids())

    def fds(self):
        return sum(len(os.listdir(f"/proc/{pid}/fd")) for pid in self.pids() if os.path.exists(f"/proc/{pid}/fd"))


# ---- one case

def one(bins, case, lang, top):
    port = lane.next_port()
    root = str(top / case["set"])
    kind = Tree if lang == "nginx" else lane.Server
    server = kind(bins[lang], port, pin=True, extra=[root] + EXTRA.get(lang, []))
    try:
        time.sleep(0.2)
        base = {**server.status(), "fds": server.fds()}
        count, size = SETS[case["set"]]
        args = ["--threads", "4", "--duration", str(case.get("duration", 10)), "--warmup", str(case.get("warmup", 2)),
                "--conns", str(case["conns"]), "--files", str(count), "--size", str(size), "--check"]
        if case.get("range"):
            args += ["--range", str(case["range"])]
        if case.get("since"):
            args += ["--since"]
        if case.get("missing"):
            args += ["--missing"]
        if case["conns"] >= 1000:
            args += ["--sources", str(4 if LINUX else 1)]
        proc = lane.load(bins, args, "files", port)
        warm_s, measured = float(case.get("warmup", 2)), float(case.get("duration", 10))
        time.sleep(warm_s + 0.05)
        cpu_from, t0 = server.cpu_seconds(), time.time()
        time.sleep(measured / 2)
        during = {**server.status(), "fds": server.fds()}
        time.sleep(max(measured / 2 - 0.1, 0))
        cpu, wall = server.cpu_seconds() - cpu_from, time.time() - t0
        r = lane.finish(proc, server, f"{case['name']} {lang}")
        cpu *= measured / wall
        end = {**server.status(), "fds": server.fds()}
        result = {**r, "cpu_seconds": round(cpu, 3), "base": base, "during": during, "end": end}
        if r["answers"]:
            result["cpu_ms_per_1k"] = round(cpu * 1e6 / r["answers"], 3)
        if r["errors"] or r["wrong"] or r["exit"]:
            result["failed"] = f"{r['errors']} connections failed, {r['wrong']} answers wrong"
        return result
    finally:
        server.stop()


def describe(case, r):
    if "failed" in r:
        return f"FAILED {r['failed']} {r.get('stderr', '')[-200:]}"
    rate = f"{r['per_second']:>9,.0f}/s" if case["conns"] > 1 else f"{r['body_mb_per_second']:>8,.0f} MB/s"
    return (f"{rate}  p50 {lane.fmt_us(r['p50_us']):>9}  p99 {lane.fmt_us(r['p99_us']):>9}  "
            f"{r.get('body_mb_per_second', 0):>8,.0f} MB/s  cpu {r.get('cpu_ms_per_1k', 0):7.3f} ms/1k  "
            f"mem {r['during']['rss_kb'] / 1024:7.1f} MB (most {r['end']['hwm_kb'] / 1024:7.1f})  thr {r['during']['threads']}")


def median(rs):
    good = [r for r in rs if "failed" not in r]
    if len(good) < len(rs):
        return next(r for r in rs if "failed" in r)
    good.sort(key=lambda r: r["per_second"])
    return good[len(good) // 2]


# ---- the check

def check(languages):
    """Each server held to `check.py`, then to the load's own check, briefly, on each set."""
    bins = build(languages)
    top = Path(tempfile.mkdtemp(prefix="files-measure-"))
    failed = []
    try:
        commands = [f"{lang}={bins[lang]}" for lang in languages if lang in bins and lang not in ("nginx", "sg-root")]
        r = subprocess.run([sys.executable, str(HERE / "check.py"), *commands])
        if r.returncode:
            failed.append("check.py")
        SETS["big"] = (1, 64 << 20)  # the file of 1 GB, smaller for a quick look
        write_sets(top, ["1k", "64k", "big"])
        for lang in languages:
            if lang not in bins:
                continue
            for name, extra in [("1k", []), ("64k", []), ("1k", ["--since"]), ("1k", ["--missing"]), ("big", ["--range", "4096"]), ("big", [])]:
                port = lane.next_port()
                kind = Tree if lang == "nginx" else lane.Server
                server = kind(bins[lang], port, pin=False, extra=[str(top / name)] + EXTRA.get(lang, []))
                try:
                    count, size = SETS[name]
                    a = ["--check", "--conns", "8", "--duration", "0.5", "--warmup", "0", "--files", str(count), "--size", str(size), *extra]
                    res = lane.finish(lane.load(bins, a, "files", port), server, f"{lang} {' '.join(a)}")
                    ok = res["exit"] == 0 and res["answers"] > 0 and not res["errors"] and not res["wrong"]
                    print(f"files-{lang:10} {'ok  ' if ok else 'FAIL'} {name} {' '.join(extra)}: {res['answers']} answers", flush=True)
                    if not ok:
                        failed.append(f"{lang} {name} {' '.join(extra)}: {res['stderr']}")
                finally:
                    server.stop()
    finally:
        shutil.rmtree(top, ignore_errors=True)
    if failed:
        print("\n".join(failed), file=sys.stderr)
        return 1
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--native", action="store_true")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--only")
    parser.add_argument("--runs", type=int, default=1)
    parser.add_argument("--languages", default=",".join(LANGUAGES + BESIDE))
    args = parser.parse_args()
    if not args.native and not args.check and not LINUX:
        # the files on a volume of the machine's own file system (xfs), as a server in a container
        # serves them: the layers of the image (overlayfs) and tmpfs have no reads that are told not
        # to wait (`RWF_NOWAIT`), which SG asks for first (1872)
        return lane.in_podman(sys.argv[1:], script="trials/server/files/measure.py", volumes=["sg-server-files:/files"])
    languages = args.languages.split(",")
    if args.check:
        return check(languages)
    bins = build(languages)
    named = args.only.split(",") if args.only else None
    chosen = [c for c in cases() if not named or c["name"] in named]
    top = Path("/files" if Path("/files").is_dir() else "/tmp/sg-files" if LINUX else tempfile.gettempdir() + "/sg-files")
    write_sets(top, sorted({c["set"] for c in chosen}))
    info = lane.machine()
    print(f"# {info}", flush=True)
    results = {}
    for case in chosen:
        warm(top / case["set"])
        for lang in languages:
            if lang not in bins:
                continue
            r = median([one(bins, case, lang, top) for _ in range(args.runs)])
            results[f"{case['name']} {lang}"] = r
            print(f"{case['name']:16} {lang:10} {describe(case, r)}", flush=True)
    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / f"files-{time.strftime('%Y-%m-%d-%H%M')}.json"
    path.write_text(json.dumps({"machine": info, "cases": cases(), "results": results}, indent=1))
    print(f"# written to {path}")


if __name__ == "__main__":
    sys.exit(main() or 0)
