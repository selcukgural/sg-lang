#!/usr/bin/env python3
"""The server lane, measured (choices 1698-1706): a store that speaks a part of RESP2 and an HTTP
server that answers GET /ping, each written in SG, Rust, Zig and C, driven by one load written in C
(`load/load.c`), on Linux. Run from the repository's root:

    python3 trials/server/measure.py              everything, on Linux in the image `sg-server`
                                                  (podman), the reference (1699)
    python3 trials/server/measure.py --native     on this machine, as it is
    python3 trials/server/measure.py --check      each server held to what it must answer, quickly,
                                                  nothing measured (what `cli.rs` runs, natively)
    python3 trials/server/measure.py --only resp-idle-10k,http-50
    python3 trials/server/measure.py --runs 3     each case three times, the median kept
    python3 trials/server/measure.py --soak       also the long runs, 30 minutes for each server and
                                                  protocol (SOAK_SECONDS for another length)

The image is built once: `podman build -t sg-server -f trials/server/Containerfile trials/server`.
In it the repository is copied in, SG built (its build kept in the volume `sg-linux-target`, as the
tests on Linux keep it), every server and the load built (kept in `sg-server-target`), and the
results written to `trials/target/server/` on this machine.

Each case starts its server afresh on cores 0-3 (`taskset`) and the load on cores 4-7, four threads
of it, so that the two do not take each other's time; a store is first given its 10,000 keys
(`--fill`), which is not measured. What is measured, from the load: answers a second, and the time
of each answer, p50, p99, p99.9 and the most, in the open loop counted from when the request was due
(no coordinated omission); from the server, through `/proc`: the memory it holds at the end of the
case (VmRSS) and the most it held (VmHWM), its threads, and the time of the processor it spent while
measured, given for each thousand answers. Idle connections are counted by the memory each adds
over the server just started, and by the time of the processor the server spends in 10 seconds with
all of them open and silent. A case where a connection failed or an answer was wrong is reported as
failed, with the count, never with its numbers. A run that has not ended after `HUNG_SECONDS` stops
everything, with what the server was doing written down first.

The cases were chosen before any number was seen (1702, 1706); the rates of the open loop are a
share of the answers a second of the slowest server with 50 connections, found by the first cases.
"""

import argparse
import json
import os
import platform
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = ROOT / "trials" / "server"
OUT = ROOT / "trials" / "target" / "server"
LINUX = sys.platform.startswith("linux")
# What is built, apart from the results, so that Linux and macOS each keep their own.
BIN = ROOT / "trials" / "target" / ("server-bin-linux" if LINUX else f"server-bin-{sys.platform}")
SG = ROOT / "interpreter" / "target" / "release" / "sg"
HUNG_SECONDS = 120
SERVER_CORES = "0-3"
LOAD_CORES = "4-7"
LANGUAGES = ["sg", "rust", "zig", "c"]
# SG's store with a limit on how long a request may take (`--idle-limit`), measured beside the others
# when named (`--languages sg,sg-limit`): its cost, and its gain against slow clients (1724).
LIMIT_MS = 5000
SERVER_ARGS = {"sg-limit": ["--idle-limit", str(LIMIT_MS)]}
KEYS = 10000
SOAK_SECONDS = int(os.environ.get("SOAK_SECONDS", "1800"))

# ---- the cases (1702, 1706): name -> what the load is given. `rate` is a share of the slowest
# server's answers a second with 50 connections; `fill` is the size of the values given first.

def cases():
    c = []
    for n, name in [(1000, "1k"), (10000, "10k"), (100000, "100k")]:
        c.append({"name": f"resp-idle-{name}", "protocol": "resp", "idle": n, "conns": 0, "duration": 10,
                  "warmup": 0, "what": f"{n:,} idle connections"})
    for conns in [1, 50, 1000]:
        for pipeline in [1, 16]:
            c.append({"name": f"resp-{conns}-p{pipeline}", "protocol": "resp", "conns": conns, "pipeline": pipeline,
                      "fill": 64, "what": f"{conns:,} connection{'s' if conns > 1 else ''}, {pipeline} in flight on each"})
    for share, label in [(0.1, "low"), (0.4, "middle"), (0.7, "high")]:
        c.append({"name": f"resp-rate-{label}", "protocol": "resp", "conns": 100, "share": share, "fill": 64,
                  "what": f"100 connections, {int(share * 100)}% of the slowest's answers a second"})
    c.append({"name": "resp-slow-10k", "protocol": "resp", "conns": 10000, "rate": 10000, "fill": 64,
              "what": "10,000 connections, one request a second each"})
    c.append({"name": "resp-idle-and-load", "protocol": "resp", "conns": 100, "idle": 10000, "share": 0.7,
              "fill": 64, "what": "10,000 idle connections and 100 at 70%"})
    # Slow clients (1724): each of 10,000 begins a SET of 1 MB once answered and sends a byte of it
    # every second; 100 busy connections beside them at 70%. A server with a limit on how long a
    # request may take closes them within the warm-up, one without keeps them all.
    c.append({"name": "resp-trickle-10k", "protocol": "resp", "conns": 100, "idle": 10000, "trickle": 1000,
              "share": 0.7, "fill": 64, "warmup": 8, "duration": 10,
              "what": "10,000 slow clients, a byte a second each, and 100 at 70%"})
    for size, label in [(4096, "4k"), (65536, "64k")]:
        c.append({"name": f"resp-value-{label}", "protocol": "resp", "conns": 50, "value": size, "fill": size,
                  "what": f"50 connections, values of {label.upper().replace('K', ' KB')}"})
    for conns in [50, 1000]:
        c.append({"name": f"http-{conns}", "protocol": "http", "conns": conns,
                  "what": f"{conns:,} connections kept open"})
    c.append({"name": "http-close", "protocol": "http", "conns": 50, "close": True,
              "what": "50 at a time, a connection for each request"})
    for share, label in [(0.1, "low"), (0.4, "middle"), (0.7, "high")]:
        c.append({"name": f"http-rate-{label}", "protocol": "http", "conns": 100, "share": share,
                  "what": f"100 connections, {int(share * 100)}% of the slowest's answers a second"})
    # The long run (1707): half the slowest's answers a second for 30 minutes, 1,000 connections, each
    # opened again after 1,000 answers; the server looked at every 10 seconds, and for 30 seconds
    # after the load stops.
    for protocol in ["resp", "http"]:
        c.append({"name": f"{protocol}-soak", "protocol": protocol, "conns": 1000, "share": 0.5,
                  "fill": 64 if protocol == "resp" else None, "duration": SOAK_SECONDS, "warmup": 10,
                  "reconnect": 1000, "soak": True,
                  "what": f"{SOAK_SECONDS // 60} minutes at 50%, 1,000 connections, each opened again after 1,000 answers"})
    return c


# ---- building

def run(cmd, **kw):
    return subprocess.run(cmd, check=True, **kw)


def build(languages, check_only=False):
    BIN.mkdir(parents=True, exist_ok=True)
    bins = {}
    run(["cc", "-O2", "-Wall", "-pthread", "-o", str(BIN / "load"), str(HERE / "load" / "load.c"), "-lm"])
    for program in ["resp", "http"]:
        for lang in languages:
            if lang == "sg-limit":
                continue
            out = BIN / f"{program}-{lang}"
            src = HERE / program / lang
            if lang == "sg":
                run([str(SG), "build", str(src / f"{program}.sg"), "-o", str(out), "--release"])
            elif lang == "c":
                run(["cc", "-O2", "-Wall", "-pthread", "-o", str(out), str(src / f"{program}.c")])
            elif lang == "rust":
                target = BIN / "cargo"
                run(["cargo", "build", "--release", "--quiet", "--manifest-path", str(src / "Cargo.toml")],
                    env={**os.environ, "CARGO_TARGET_DIR": str(target)})
                shutil.copy(target / "release" / program, out)
            elif lang == "zig":
                run(["zig", "build-exe", "-OReleaseFast", str(src / f"{program}.zig"), f"-femit-bin={out}"],
                    cwd=BIN)
            bins[(program, lang)] = out
    # the same program as `sg`, started with a limit (see SERVER_ARGS)
    if "sg-limit" in languages:
        if "sg" not in languages:
            run([str(SG), "build", str(HERE / "resp" / "sg" / "resp.sg"), "-o", str(BIN / "resp-sg"), "--release"])
        bins[("resp", "sg-limit")] = BIN / "resp-sg"
    return bins


# ---- one server

class Server:
    def __init__(self, path, port, pin, extra=()):
        cmd = [str(path), str(port), *extra]
        if pin and LINUX:
            cmd = ["taskset", "-c", SERVER_CORES] + cmd
        self.proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        self.pid = self.proc.pid
        self.port = port
        deadline = time.time() + 10
        while True:
            try:
                socket.create_connection(("127.0.0.1", port), timeout=0.2).close()
                break
            except OSError:
                if self.proc.poll() is not None or time.time() > deadline:
                    err = self.proc.stderr.read().decode(errors="replace") if self.proc.poll() is not None else ""
                    self.stop()
                    raise RuntimeError(f"{path} did not start listening on {port}: {err}")
                time.sleep(0.02)
        if pin and LINUX:
            # taskset runs the program in its place, so the pid is the server's
            pass

    def status(self):
        """VmRSS, VmHWM (KB) and threads, from /proc (Linux), or the RSS from ps elsewhere."""
        if LINUX:
            fields = {}
            for line in Path(f"/proc/{self.pid}/status").read_text().splitlines():
                key, _, value = line.partition(":")
                fields[key] = value.strip()
            kb = lambda k: int(fields[k].split()[0])
            return {"rss_kb": kb("VmRSS"), "hwm_kb": kb("VmHWM"), "threads": int(fields["Threads"])}
        rss = int(subprocess.run(["ps", "-o", "rss=", "-p", str(self.pid)], capture_output=True, text=True).stdout.strip() or 0)
        return {"rss_kb": rss, "hwm_kb": rss, "threads": 0}

    def cpu_seconds(self):
        if LINUX:
            parts = Path(f"/proc/{self.pid}/stat").read_text().rsplit(")", 1)[1].split()
            ticks = os.sysconf("SC_CLK_TCK")
            return (int(parts[11]) + int(parts[12])) / ticks
        out = subprocess.run(["ps", "-o", "time=", "-p", str(self.pid)], capture_output=True, text=True).stdout.strip()
        m, _, s = out.rpartition(":")
        return float(m or 0) * 60 + float(s or 0)

    def fds(self):
        try:
            return len(os.listdir(f"/proc/{self.pid}/fd"))
        except OSError:
            return -1

    def where(self):
        """What the server was doing, for a run that hung."""
        lines = []
        if LINUX:
            for task in sorted(Path(f"/proc/{self.pid}/task").iterdir()):
                try:
                    lines.append(f"{task.name} {(task / 'wchan').read_text()} {(task / 'stat').read_text().split()[2]}")
                except OSError:
                    pass
        return "\n".join(lines)

    def stop(self):
        if self.proc.poll() is None:
            self.proc.send_signal(signal.SIGKILL)
        self.proc.wait()


def load(bins, args, protocol, port, timeout=HUNG_SECONDS):
    cmd = [str(BIN / "load"), protocol, f"127.0.0.1:{port}"] + args
    if LINUX:
        cmd = ["taskset", "-c", LOAD_CORES] + cmd
    return subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def finish(proc, server, what):
    try:
        out, err = proc.communicate(timeout=HUNG_SECONDS)
    except subprocess.TimeoutExpired:
        OUT.mkdir(parents=True, exist_ok=True)
        note = OUT / f"hung-{server.pid}.txt"
        note.write_text(f"{what}\n{server.where()}\n")
        proc.kill()
        server.stop()
        sys.exit(f"measure: {what} has not ended after {HUNG_SECONDS} s; what the server was doing is in {note}")
    lines = [line for line in out.splitlines() if line.startswith("{")]
    if not lines:
        raise RuntimeError(f"{what}: the load wrote nothing it measured\n{err}")
    result = json.loads(lines[-1])
    result["stderr"] = err.strip()[-500:]
    result["exit"] = proc.returncode
    return result


# ---- one case

PORT = [20000]


def next_port():
    PORT[0] += 1
    if PORT[0] > 32000:
        PORT[0] = 20001
    return PORT[0]


def one(bins, case, lang, rate):
    program = case["protocol"]
    port = next_port()
    server = Server(bins[(program, lang)], port, pin=True, extra=SERVER_ARGS.get(lang, ()))
    try:
        time.sleep(0.2)
        base = {**server.status(), "fds": server.fds()}
        if case.get("fill"):
            fill = load(bins, ["--fill", "--conns", "8", "--pipeline", "16", "--keys", str(KEYS),
                               "--value", str(case["fill"])], "resp", port)
            r = finish(fill, server, f"{case['name']} {lang}: fill")
            if r["errors"] or r["wrong"]:
                return {"failed": f"fill: {r['errors']} errors, {r['wrong']} wrong", "stderr": r["stderr"]}
        args = ["--threads", "4", "--keys", str(KEYS), "--duration", str(case.get("duration", 10)),
                "--warmup", str(case.get("warmup", 2))]
        args += ["--conns", str(case["conns"]), "--pipeline", str(case.get("pipeline", 1))]
        if case.get("value"):
            args += ["--value", str(case["value"])]
        if case.get("close"):
            args += ["--close"]
        if case.get("reconnect"):
            args += ["--reconnect", str(case["reconnect"]), "--sources", str(4 if LINUX else 1)]
        if case.get("soak"):
            args += ["--report", "10"]
        if rate:
            args += ["--rate", str(int(rate))]
        idle = case.get("idle", 0)
        if idle:
            args += ["--idle", str(idle)] + ([] if case.get("reconnect") else ["--sources", str(4 if LINUX else 1)])
        if case.get("trickle"):
            args += ["--trickle", str(case["trickle"])]
        before_mem = server.status()
        proc = load(bins, args, program, port)
        ready_mem = None
        if idle:
            # the load says `ready` once every idle connection is open and answered
            line = proc.stdout.readline()
            if not line.startswith("ready"):
                rest, err = proc.communicate(timeout=HUNG_SECONDS)
                return {"failed": f"the idle connections: {line.strip()} {err.strip()[-300:]}"}
            time.sleep(0.5)
            ready_mem = server.status()
        if case.get("soak"):
            return soak(case, lang, server, proc, base)
        # the time of the processor is counted over the measured seconds alone, as the answers are
        warm, measured = float(case.get("warmup", 2)), float(case.get("duration", 10))
        time.sleep(warm + 0.05)
        cpu_from, t0 = server.cpu_seconds(), time.time()
        # the server in the middle of the measured time, while the load holds every connection (at
        # the end the load may have closed them already)
        time.sleep(max(measured / 2, 0))
        during = {**server.status(), "fds": server.fds()}
        time.sleep(max(measured / 2 - 0.1, 0))
        cpu, wall = server.cpu_seconds() - cpu_from, time.time() - t0
        r = finish(proc, server, f"{case['name']} {lang}")
        cpu *= measured / wall  # to the whole window
        end = {**server.status(), "fds": server.fds()}
        result = {**r, "cpu_seconds": round(cpu, 3), "wall": round(wall, 3), "base": base, "end": end, "during": during}
        if ready_mem:
            result["ready"] = ready_mem
            result["per_connection_kb"] = round((ready_mem["rss_kb"] - before_mem["rss_kb"]) / idle, 3)
            result["idle_ready_line"] = line.strip()
        if r["answers"]:
            result["cpu_ms_per_1k"] = round(cpu * 1e6 / r["answers"], 3)
        if r["errors"] or r["wrong"]:
            result["failed"] = f"{r['errors']} connections failed, {r['wrong']} answers wrong"
        return result
    finally:
        server.stop()


def soak(case, lang, server, proc, base):
    """The long run: the server looked at every 10 seconds while the load goes on, and for 30
    seconds after it stops; what the load measured in each 10 seconds comes after its end."""
    samples = []
    start = time.time()
    total = case["warmup"] + case["duration"]
    while proc.poll() is None and time.time() - start < total + HUNG_SECONDS:
        st = server.status()
        samples.append({"t": round(time.time() - start, 1), **st, "fds": server.fds(), "cpu": server.cpu_seconds()})
        time.sleep(10)
    out, err = proc.communicate(timeout=HUNG_SECONDS)
    lines = [json.loads(l) for l in out.splitlines() if l.startswith("{")]
    intervals = [l for l in lines if "interval" in l]
    r = lines[-1]
    # after the load: the server should go quiet, let its connections go and hold no more
    cpu0 = server.cpu_seconds()
    time.sleep(30)
    after = {**server.status(), "fds": server.fds(), "cpu_30s": round(server.cpu_seconds() - cpu0, 3)}
    result = {**r, "samples": samples, "intervals": intervals, "after": after, "base": base,
              "end": samples[-1] if samples else base, "stderr": err.strip()[-500:], "exit": proc.returncode}
    # the verdicts: the memory after the first 5 minutes, and the p99 of the last minute against the
    # fifth (1707)
    later = [x for x in samples if x["t"] >= case["warmup"] + 300]
    if later:
        result["rss_from_5min_kb"] = later[0]["rss_kb"]
        result["rss_last_kb"] = later[-1]["rss_kb"]
        result["rss_most_after_5min_kb"] = max(x["rss_kb"] for x in later)
    if len(intervals) >= 36:
        fifth = intervals[24:30]
        last = intervals[-6:]
        result["p99_fifth_minute_us"] = max(x["p99_us"] for x in fifth)
        result["p99_last_minute_us"] = max(x["p99_us"] for x in last)
    if r["errors"] or r["wrong"]:
        result["failed"] = f"{r['errors']} connections failed, {r['wrong']} answers wrong"
    return result


def median_of(results):
    good = [r for r in results if "failed" not in r]
    if len(good) < len(results):
        return next(r for r in results if "failed" in r)
    key = "per_connection_kb" if "per_connection_kb" in good[0] and not good[0]["answers"] else "per_second"
    good.sort(key=lambda r: r.get(key, 0))
    m = good[len(good) // 2]
    m["runs"] = [r.get(key) for r in good]
    return m


# ---- the table

def fmt_us(v):
    return f"{v / 1000:.2f} ms" if v >= 1000 else f"{v:.0f} µs"


def describe(case, r):
    if r is None:
        return "-"
    if "failed" in r:
        return "FAILED: " + r["failed"]
    mem = r["end"]["rss_kb"] / 1024
    if case.get("soak"):
        return (f"{r['per_second']:,.0f}/s p99 {fmt_us(r['p99_us'])} max {fmt_us(r['max_us'])}; "
                f"RSS at 5 min {r.get('rss_from_5min_kb', 0) / 1024:.1f} MB, last {r.get('rss_last_kb', 0) / 1024:.1f} MB, "
                f"most {r.get('rss_most_after_5min_kb', 0) / 1024:.1f} MB; p99 5th minute {fmt_us(r.get('p99_fifth_minute_us', 0))}, "
                f"last {fmt_us(r.get('p99_last_minute_us', 0))}; after: cpu {r['after']['cpu_30s']} s in 30 s, "
                f"{r['after']['fds']} fds (at start {r['base'].get('fds', '?')}), {r['after']['rss_kb'] / 1024:.1f} MB")
    if case.get("idle") and not case.get("conns"):
        return (f"{r['per_connection_kb']:.2f} KB/conn, {r['ready']['rss_kb'] / 1024:.1f} MB, "
                f"cpu {r['cpu_seconds']:.3f} s in 10 s, {r['ready']['threads']} threads")
    s = f"{r['per_second']:,.0f}/s p50 {fmt_us(r['p50_us'])} p99 {fmt_us(r['p99_us'])} p99.9 {fmt_us(r['p999_us'])}"
    s += f", {mem:.1f} MB"
    if case.get("trickle"):
        d = r.get("during", {})
        s += f"; under the load {d.get('rss_kb', 0) / 1024:.1f} MB, {d.get('fds', '?')} fds; {r.get('cut', 0):,} cut"
    if r.get("cpu_ms_per_1k") is not None:
        s += f", cpu {r['cpu_ms_per_1k']:.1f} ms/1k"
    return s


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--native", action="store_true")
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--only", default="")
    ap.add_argument("--languages", default=",".join(LANGUAGES))
    ap.add_argument("--runs", type=int, default=1)
    ap.add_argument("--soak", action="store_true")
    ap.add_argument("--table", metavar="RESULTS", help="print the tables of a run's results, in Markdown")
    args = ap.parse_args()

    if args.table:
        return table(args.table)
    if not args.native and not args.check and not LINUX:
        return in_podman(sys.argv[1:])

    languages = args.languages.split(",")
    if args.check:
        return check(languages)
    bins = build(languages)
    # the long runs only when named, or with --soak, after everything else
    named = args.only.split(",") if args.only else None
    chosen = [c for c in cases() if (c["name"] in named if named else (args.soak or not c.get("soak")))]
    results = {}
    slowest = {}
    info = machine()
    print(f"# {info}", flush=True)
    for case in chosen:
        if "share" in case:
            calibrate = "resp-50-p1" if case["protocol"] == "resp" else "http-50"
            if calibrate not in slowest:
                peaks = [results.get((calibrate, l), {}).get("per_second") for l in languages if l != "sg-limit"]
                peaks = [p for p in peaks if p]
                if not peaks:
                    cal = next(c for c in cases() if c["name"] == calibrate)
                    peaks = [one(bins, cal, l, None).get("per_second") or 0 for l in languages if l != "sg-limit"]
                slowest[calibrate] = min(p for p in peaks if p) if any(peaks) else 10000
            rate = slowest[calibrate] * case["share"]
        else:
            rate = case.get("rate")
        for lang in languages:
            if lang == "sg-limit" and case["protocol"] != "resp":
                continue
            rs = [one(bins, case, lang, rate) for _ in range(args.runs)]
            r = median_of(rs)
            r["rate_asked"] = rate
            results[(case["name"], lang)] = r
            print(f"{case['name']:20} {lang:5} {describe(case, r)}", flush=True)
    OUT.mkdir(parents=True, exist_ok=True)
    stamp = time.strftime("%Y-%m-%d-%H%M")
    path = OUT / f"results-{stamp}.json"
    path.write_text(json.dumps({"machine": info, "cases": cases(), "results": {f"{k[0]} {k[1]}": v for k, v in results.items()}}, indent=1))
    print(f"# written to {path}")


def cell(case, r):
    """One cell of the tables of `benchmarks/server.md`."""
    if r is None:
        return "-"
    if "failed" in r:
        return "failed: " + r["failed"].split(",")[0]
    if case.get("idle") and not case.get("conns"):
        return f"{r['per_connection_kb']:.2f} KB, {r['ready']['rss_kb'] / 1024:.1f} MB, {r['cpu_seconds']:.2f} s"
    s = f"{r['per_second'] / 1000:,.0f}k/s, {fmt_us(r['p99_us'])}"
    if case.get("share") or case.get("rate"):
        s = f"p99 {fmt_us(r['p99_us'])}, p99.9 {fmt_us(r['p999_us'])}"
    return s + f", {r['end']['rss_kb'] / 1024:.1f} MB"


def table(path):
    """The results of a run as the tables of `benchmarks/server.md`, in Markdown."""
    data = json.loads(Path(path).read_text())
    results = data["results"]
    print(f"<!-- {data['machine']} -->")
    for protocol, title in [("resp", "The store (RESP)"), ("http", "HTTP")]:
        print(f"\n### {title}\n")
        print("| Case | SG | Rust | Zig | C |")
        print("|---|---|---|---|---|")
        for case in cases():
            if case["protocol"] != protocol or case.get("soak"):
                continue
            row = [cell(case, results.get(f"{case['name']} {l}")) for l in LANGUAGES]
            if all(c == "-" for c in row):
                continue
            print(f"| {case['what']} | " + " | ".join(row) + " |")


def machine():
    info = f"{platform.system()} {platform.release()} {platform.machine()}, {os.cpu_count()} cores"
    try:
        rustc = subprocess.run(["rustc", "--version"], capture_output=True, text=True).stdout.strip()
        zig = subprocess.run(["zig", "version"], capture_output=True, text=True).stdout.strip()
        cc = subprocess.run(["cc", "--version"], capture_output=True, text=True).stdout.splitlines()[0]
        info += f"; {rustc}; zig {zig}; {cc}"
    except (OSError, IndexError):
        pass
    return info


def check(languages):
    """Each server held to what it must answer: a store filled and every answer checked byte for
    byte, also with values of 64 KB and with commands in flight; the HTTP server with connections
    kept and closed. Short runs, nothing measured."""
    have = [l for l in languages if l in ("sg", "c") or shutil.which({"rust": "cargo", "zig": "zig"}[l])]
    bins = build(have, check_only=True)
    failed = []
    for lang in have:
        for program, runs in [
            ("resp", [["--fill", "--conns", "4", "--pipeline", "16", "--keys", "2000"],
                      ["--check", "--keys", "2000", "--conns", "8", "--duration", "0.5", "--warmup", "0"],
                      ["--check", "--keys", "2000", "--conns", "8", "--pipeline", "16", "--duration", "0.5", "--warmup", "0"],
                      ["--fill", "--conns", "4", "--pipeline", "4", "--keys", "200", "--value", "65536"],
                      ["--check", "--keys", "200", "--value", "65536", "--conns", "4", "--duration", "0.5", "--warmup", "0"],
                      ["--idle", "200", "--conns", "4", "--duration", "0.5", "--warmup", "0"]]),
            ("http", [["--check", "--conns", "8", "--duration", "0.5", "--warmup", "0"],
                      ["--check", "--close", "--conns", "4", "--duration", "0.5", "--warmup", "0"],
                      ["--check", "--conns", "8", "--rate", "2000", "--duration", "0.5", "--warmup", "0"]]),
        ]:
            port = next_port()
            server = Server(bins[(program, lang)], port, pin=False)
            try:
                for a in runs:
                    r = finish(load(bins, a, program, port), server, f"{program} {lang} {' '.join(a)}")
                    ok = r["exit"] == 0 and r["answers"] > 0 and not r["errors"] and not r["wrong"]
                    print(f"{program}-{lang:5} {'ok  ' if ok else 'FAIL'} {' '.join(a)}: {r['answers']} answers", flush=True)
                    if not ok:
                        failed.append(f"{program}-{lang} {' '.join(a)}: {r['stderr']}")
            finally:
                server.stop()
    if failed:
        print("\n".join(failed), file=sys.stderr)
        return 1
    return 0


def in_podman(argv):
    """Runs this script on Linux in the image `sg-server`, the repository copied in."""
    results = OUT
    results.mkdir(parents=True, exist_ok=True)
    inner = ("mkdir -p /work && cd /src && tar --exclude=./interpreter/target --exclude=./graphify-out "
             "--exclude=./trials/target -cf - . | tar -xf - -C /work && cd /work/interpreter && "
             "cargo build --release --quiet && cd /work && mkdir -p trials/target && "
             "ln -s /results trials/target/server && python3 trials/server/measure.py --native " + " ".join(argv))
    cmd = ["podman", "run", "--rm", "--ulimit", "nofile=524288:524288",
           "--sysctl", "net.core.somaxconn=65535", "--sysctl", "net.ipv4.tcp_max_syn_backlog=65535",
           "-v", f"{ROOT}:/src:ro", "-v", "sg-linux-target:/work/interpreter/target",
           "-v", "sg-server-cargo:/usr/local/cargo/registry", "-v", "sg-server-target:/work/trials/target/server-bin-linux",
           "-v", f"{results}:/results",
           "localhost/sg-server", "bash", "-c", inner]
    return subprocess.run(cmd).returncode


if __name__ == "__main__":
    sys.exit(main() or 0)
