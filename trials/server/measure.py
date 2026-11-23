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
    python3 trials/server/measure.py --ramp       the test of the breaking point (1760), each server and
                                                  protocol: the load rises in stages of 10 seconds until
                                                  the server answers less than half of it in time; a line
                                                  for each stage as it ends, where each server broke, and
                                                  with --runs N how far apart N runs lie (`--native
                                                  --ramp` on this machine)
    python3 trials/server/measure.py --robust     the stand (1751, 1755, 1759): each server, the heavy
                                                  level of `load/stand.c` (a million resets, 100,000
                                                  clients beside 1,024 descriptors, 10,000 that do not
                                                  read, a million requests that are not the protocol,
                                                  SIGTERM with 50,000 open), SG also under the
                                                  sanitizers; whether each stood, and its descriptors
                                                  and memory (ROBUST_SCALE=0.05 for a short try)

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
# The test of the breaking point (1760): each connection asks once every THINK_MS, an answer later
# than DEADLINE_MS after its request was due is a failure, and the load rises by `step` requests a
# second in each stage of `seconds`. On Linux the connections come from four addresses, for ports
# enough for 100,000 of them; elsewhere the load may hold 10,240 files.
THINK_MS = 20
DEADLINE_MS = 100
# The server has two cores, as the pods of the video it follows had two (lesson 215 of Anton Putra's
# tutorials), and the load the other six, so that the load is never what gives out first.
RAMP_SERVER_CORES, RAMP_LOAD_CORES, RAMP_THREADS = "0-1", "2-7", 6
RAMP = ({"from": 25000, "step": 25000, "seconds": 10, "stages": 60, "conns": 100000, "sources": 4} if LINUX else
        {"from": 20000, "step": 20000, "seconds": 10, "stages": 40, "conns": 10000, "sources": 1})
for key in RAMP:  # RAMP_SECONDS=3 RAMP_STAGES=4 for a short look
    if os.environ.get(f"RAMP_{key.upper()}"):
        RAMP[key] = int(os.environ[f"RAMP_{key.upper()}"])

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
    run(["cc", "-std=c11", "-O2", "-Wall", "-o", str(BIN / "stand"), str(HERE / "load" / "stand.c")])
    for program in ["resp", "http"]:
        for lang in languages:
            if lang == "sg-limit":
                continue
            out = BIN / f"{program}-{lang}"
            src = HERE / program / lang
            if lang in ("sg", "sg-by-hand"):
                # `sg-by-hand`: the HTTP server written before `std` had HTTP (choice 1771)
                if not (src / f"{program}.sg").exists():
                    continue
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
    def __init__(self, path, port, pin, extra=(), cores=SERVER_CORES):
        cmd = [str(path), str(port), *extra]
        if pin and LINUX:
            cmd = ["taskset", "-c", cores] + cmd
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


def load(bins, args, protocol, port, timeout=HUNG_SECONDS, cores=LOAD_CORES):
    cmd = [str(BIN / "load"), protocol, f"127.0.0.1:{port}"] + args
    if LINUX:
        cmd = ["taskset", "-c", cores] + cmd
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


# ---- the breaking point

def cpu_of(pid):
    """The time of the processor a process has spent, in seconds, or None once it is gone."""
    try:
        if LINUX:
            parts = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
            return (int(parts[11]) + int(parts[12])) / os.sysconf("SC_CLK_TCK")
        out = subprocess.run(["ps", "-o", "time=", "-p", str(pid)], capture_output=True, text=True,
                             env={**os.environ, "LC_ALL": "C"}).stdout.strip().replace(",", ".")
        if not out:
            return None
        return sum(float(x) * 60 ** i for i, x in enumerate(reversed(out.split(":"))))
    except (OSError, ValueError, IndexError):
        return None


def looked_at(server):
    """Memory, threads and open files of a server, on any system."""
    if server.proc.poll() is not None:
        return {"rss_kb": 0, "hwm_kb": 0, "threads": 0, "fds": 0}
    st = server.status()
    fds = server.fds()
    if not LINUX:
        env = {**os.environ, "LC_ALL": "C"}
        threads = subprocess.run(["ps", "-M", "-p", str(server.pid)], capture_output=True, text=True, env=env).stdout
        st["threads"] = max(len(threads.splitlines()) - 1, 0)
        files = subprocess.run(["lsof", "-n", "-P", "-p", str(server.pid)], capture_output=True, text=True, env=env).stdout
        fds = max(len(files.splitlines()) - 1, 0)
    return {**st, "fds": fds}


def breaks(stages):
    """Where a server broke (1760): the knee, the first stage whose p99 is ten times that of the
    first stages or more than the deadline; saturation, the first that is answered less than 95% of
    what was asked; availability, the first in which less than 99.9% is answered in time; and the
    most it answered in time in a second."""
    if not stages:
        return {}
    base = min(s["p99_us"] for s in stages[:2] if s["p99_us"] > 0) if any(s["p99_us"] > 0 for s in stages[:2]) else 0
    first = lambda test: next((s["rate"] for s in stages if test(s)), None)
    peak = max(stages, key=lambda s: s["ok_per_second"])
    return {
        "base_p99_us": base,
        "knee": first(lambda s: s["p99_us"] > 10 * base or s["p99_us"] > DEADLINE_MS * 1000),
        "saturation": first(lambda s: s["asked"] and (s["ok"] + s["late"]) < 0.95 * s["asked"]),
        "availability": first(lambda s: s["availability"] < 0.999),
        "peak_ok_per_second": peak["ok_per_second"],
        "peak_rate": peak["rate"],
    }


def ramp_row(protocol, lang, s):
    load_cores = f"{s['load_cores']:4.2f}" if s["load_cores"] >= 0 else "   ?"
    return (f"{protocol:4} {lang:5} {s['stage']:3} asked {s['asked'] / RAMP['seconds']:>9,.0f}/s  in time {s['ok_per_second']:>9,.0f}/s  "
            f"avail {s['availability'] * 100:6.2f}%  p50 {fmt_us(s['p50_us']):>9}  p99 {fmt_us(s['p99_us']):>9}  "
            f"conns {s['conns']:>6,}  refused {s['refused']:>5,}  server {s['server_cores']:4.2f} cores "
            f"{s['rss_kb'] / 1024:7.1f} MB {s['threads']:3} thr {s['fds']:>6} fds  load {load_cores} cores")


def ramp_one(bins, protocol, lang):
    port = next_port()
    server = Server(bins[(protocol, lang)], port, pin=True, extra=SERVER_ARGS.get(lang, ()), cores=RAMP_SERVER_CORES)
    try:
        time.sleep(0.2)
        base = looked_at(server)
        if protocol == "resp":
            fill = load(bins, ["--fill", "--conns", "8", "--pipeline", "16", "--keys", str(KEYS)], "resp", port)
            r = finish(fill, server, f"ramp {lang}: fill")
            if r["errors"] or r["wrong"]:
                return {"failed": f"fill: {r['errors']} errors, {r['wrong']} wrong", "stages": []}
        args = ["--threads", str(RAMP_THREADS), "--keys", str(KEYS), "--ramp", f"{RAMP['from']},{RAMP['step']},{RAMP['seconds']}",
                "--stages", str(RAMP["stages"]), "--conns", str(RAMP["conns"]), "--think", str(THINK_MS),
                "--deadline", str(DEADLINE_MS)]
        if RAMP["sources"] > 1:
            args += ["--sources", str(RAMP["sources"])]
        proc = load(bins, args, protocol, port, cores=RAMP_LOAD_CORES)
        # a load that does not end is stopped, with what the server was doing written down
        limit = RAMP["stages"] * RAMP["seconds"] + HUNG_SECONDS
        hung = []
        def watch():
            try:
                proc.wait(timeout=limit)
            except subprocess.TimeoutExpired:
                hung.append(server.where())
                proc.kill()
        import threading
        threading.Thread(target=watch, daemon=True).start()
        stages = []
        t_prev, s_prev, l_prev = time.time(), cpu_of(server.pid) or 0, cpu_of(proc.pid) or 0
        for line in proc.stdout:
            if not line.startswith('{"stage"'):
                continue
            d = json.loads(line)
            now = time.time()
            s_now = cpu_of(server.pid)
            l_now = cpu_of(proc.pid)
            d["server_cores"] = round(((s_now if s_now is not None else s_prev) - s_prev) / (now - t_prev), 3)
            # the load may have ended by the time its last line is read: then its share is not known
            d["load_cores"] = round((l_now - l_prev) / (now - t_prev), 3) if l_now is not None and l_now >= l_prev else -1
            d.update(looked_at(server))
            d["server_alive"] = server.proc.poll() is None
            t_prev, s_prev, l_prev = now, s_now if s_now is not None else s_prev, l_now if l_now is not None else l_prev
            stages.append(d)
            print(ramp_row(protocol, lang, d), flush=True)
        err = proc.stderr.read()
        proc.wait()
        result = {"stages": stages, "base": base, "breaks": breaks(stages), "load_stderr": err.strip()[-500:],
                  "server_died": server.proc.poll() is not None}
        if result["server_died"]:
            result["server_stderr"] = server.proc.stderr.read().decode(errors="replace")[-1000:]
            result["server_exit"] = server.proc.returncode
        if hung:
            result["failed"] = f"the load did not end after {limit} s"
            result["where"] = hung[0]
        return result
    finally:
        server.stop()


def ramp(bins, languages, protocols, info, runs=1):
    if LINUX:
        print(f"# the server on cores {RAMP_SERVER_CORES}, the load on {RAMP_LOAD_CORES} with {RAMP_THREADS} threads", flush=True)
    print(f"# the breaking point: from {RAMP['from']:,} requests a second, {RAMP['step']:,} more every "
          f"{RAMP['seconds']} s; each connection asks every {THINK_MS} ms (up to {RAMP['conns']:,} connections); "
          f"an answer later than {DEADLINE_MS} ms is a failure", flush=True)
    results = {}
    for protocol in protocols:
        for lang in languages:
            if lang == "sg-limit" or (lang == "sg-by-hand" and protocol != "http"):
                continue
            tries = []
            for n in range(runs):
                r = ramp_one(bins, protocol, lang)
                tries.append(r)
                b = r.get("breaks", {})
                if b:
                    print(f"# {protocol} {lang}{f' run {n + 1}/{runs}' if runs > 1 else ''}: knee at {b['knee'] or '-'}, "
                          f"saturated at {b['saturation'] or '-'}, availability below 99.9% at {b['availability'] or '-'}, "
                          f"most in time {b['peak_ok_per_second']:,.0f}/s; the server {'DIED' if r['server_died'] else 'stayed up'}", flush=True)
            # one run keeps its stages; several keep each, and how far their breaking points lie apart
            r = tries[0] if runs == 1 else {"runs": tries}
            if runs > 1:
                spread = {}
                for key in ["knee", "saturation", "availability", "peak_ok_per_second"]:
                    values = sorted(t["breaks"][key] for t in tries if t.get("breaks", {}).get(key) is not None)
                    if values:
                        spread[key] = {"least": values[0], "median": values[len(values) // 2], "most": values[-1]}
                r["spread"] = spread
                p = spread.get("peak_ok_per_second")
                if p:
                    print(f"# {protocol} {lang} over {runs} runs: most in time {p['least']:,.0f} / {p['median']:,.0f} / "
                          f"{p['most']:,.0f} (least / median / most)", flush=True)
            results[f"{protocol} {lang}"] = r
    OUT.mkdir(parents=True, exist_ok=True)
    stamp = time.strftime("%Y-%m-%d-%H%M")
    where = "linux" if LINUX else platform.system().lower()
    path = OUT / f"ramp-{where}-{stamp}.json"
    path.write_text(json.dumps({"machine": info, "ramp": RAMP, "think_ms": THINK_MS, "deadline_ms": DEADLINE_MS,
                                "server_cores": RAMP_SERVER_CORES if LINUX else "all", "load_threads": RAMP_THREADS,
                                "results": results}, indent=1))
    print(f"# written to {path}")


# ---- the stand (1751, 1755, 1759)

# The heavy level of the stand, as each server stands it (1759): the counts are those of all the
# rounds together; ROBUST_SCALE makes them smaller for a short try. "sg" alone: what only the SG
# servers have flags for (--idle-limit, --grace; the others are not compared there, 1751 g).
ROBUST_SCALE = float(os.environ.get("ROBUST_SCALE", "1"))
ROBUST = [
    {"name": "answers", "args": "answers --conns 250000 --at-once 5000 --rounds 4"},
    {"name": "fds", "args": "fds --nofile 1024 --conns 100000 --rounds 3"},
    {"name": "reset", "args": "reset --conns 250000 --at-once 100000 --rounds 4"},
    {"name": "half", "args": "half --conns 250000 --at-once 10000 --rounds 4"},
    {"name": "split", "args": "split --conns 25000 --at-once 2000 --rounds 4"},
    {"name": "trickle", "args": "trickle --conns 25000 --at-once 5000 --rounds 3"},
    {"name": "noread", "args": "noread --conns 10000 --value 4096 --rounds 3"},
    {"name": "noread-limit", "args": "noread --conns 10000 --value 4096 --rounds 3 --idle 5000",
     "server": ["--idle-limit", "5000"], "only": "sg"},
    {"name": "term", "args": "term --conns 50000 --grace 2000 --margin 3000", "server": ["--grace", "2000"], "only": "sg"},
    {"name": "fuzz", "args": "fuzz --conns 1000000 --at-once 500 --rounds 1"},
    {"name": "fuzz-sanitized", "args": "fuzz --conns 1000000 --at-once 500 --rounds 1 --no-memory", "only": "sg-sanitized"},
    {"name": "reset-sanitized", "args": "reset --conns 250000 --at-once 50000 --rounds 1 --no-memory", "only": "sg-sanitized"},
    {"name": "term-sanitized", "args": "term --conns 50000 --grace 2000 --margin 30000 --no-memory",
     "server": ["--grace", "2000"], "only": "sg-sanitized"},
]
COUNTS = {"--conns", "--at-once"}
# A server that holds more is stopped by the stand, before the machine runs out (the image is also
# given a limit of memory, `in_podman`).
ROBUST_MOST_RSS_MB = 6000


def scaled(args):
    words = args.split()
    for i, w in enumerate(words[:-1]):
        if w in COUNTS:
            words[i + 1] = str(max(1, int(int(words[i + 1]) * ROBUST_SCALE)))
    return words


def build_sanitized(protocol):
    out = BIN / f"{protocol}-sg-sanitized"
    c = BIN / f"{protocol}-sg-sanitized.c"
    with open(c, "w") as f:
        run([str(SG), "emit-c", str(HERE / protocol / "sg" / f"{protocol}.sg"), "--release"], stdout=f)
    run(["cc", "-std=c11", "-O1", "-g", "-fsanitize=address,undefined", "-fno-sanitize-recover=all", "-o", str(out),
         str(c), "-lm", "-pthread"])
    return out


def robust(bins, languages, protocols, info):
    print(f"# the stand, heavy level{f' x{ROBUST_SCALE}' if ROBUST_SCALE != 1 else ''}; the server on cores "
          f"{SERVER_CORES}, the stand on {LOAD_CORES}", flush=True)
    results = []
    for protocol in protocols:
        sanitized = build_sanitized(protocol)
        for case in ROBUST:
            for lang in languages + ["sg-sanitized"]:
                only = case.get("only")
                if lang == "sg-limit" or (only and lang != only) or (not only and lang == "sg-sanitized") or (protocol, lang) not in bins and lang != "sg-sanitized":
                    continue
                server = sanitized if lang == "sg-sanitized" else bins[(protocol, lang)]
                cmd = [str(BIN / "stand"), protocol, *scaled(case["args"]), "--sources", "4",
                       "--most-rss", str(ROBUST_MOST_RSS_MB), "--sanity", "5000", "--timeout", "3600", "--",
                       *(["taskset", "-c", SERVER_CORES] if LINUX else []), str(server), "PORT", *case.get("server", [])]
                if LINUX:
                    cmd = ["taskset", "-c", LOAD_CORES] + cmd
                started = time.time()
                p = subprocess.run(cmd, capture_output=True, text=True, timeout=3700)
                line = next((l for l in p.stdout.splitlines() if l.startswith("{")), None)
                r = json.loads(line) if line else {"stood": False}
                r.update({"protocol": protocol, "case": case["name"], "lang": lang, "exit": p.returncode,
                          "errors": p.stderr.strip().splitlines()[:20], "wall_seconds": round(time.time() - started, 1)})
                results.append(r)
                said = "stood" if r.get("stood") else f"FELL ({r['errors'][0] if r['errors'] else f'exit {p.returncode}'})"
                print(f"{protocol:4} {case['name']:16} {lang:12} {said}; descriptors {r.get('fds_base')} -> "
                      f"{r.get('fds_after')}, idle KB {r.get('idle_kb')}, peak KB {r.get('peak_kb')}, "
                      f"{r['wall_seconds']} s", flush=True)
    OUT.mkdir(parents=True, exist_ok=True)
    stamp = time.strftime("%Y-%m-%d-%H%M")
    where = "linux" if LINUX else platform.system().lower()
    path = OUT / f"robust-{where}-{stamp}.json"
    path.write_text(json.dumps({"machine": info, "scale": ROBUST_SCALE, "cases": ROBUST, "results": results}, indent=1))
    print(f"# written to {path}")


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
    ap.add_argument("--ramp", action="store_true")
    ap.add_argument("--robust", action="store_true")
    ap.add_argument("--protocols", default="resp,http")
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
    if args.ramp:
        return ramp(bins, languages, args.protocols.split(","), machine(), args.runs)
    if args.robust:
        return robust(bins, languages, args.protocols.split(","), machine())
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
            if (lang == "sg-limit" and case["protocol"] != "resp") or (lang == "sg-by-hand" and case["protocol"] != "http"):
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
    # the stand's servers may grow past what the machine has: a limit, so that the kernel stops
    # the server in the image and not the machine (the stand stops it before, --most-rss)
    limit = ["--memory", "7g"] if "--robust" in argv else []
    cmd = ["podman", "run", "--rm", "--ulimit", "nofile=524288:524288", *limit,
           "-e", f"ROBUST_SCALE={ROBUST_SCALE}",
           "--sysctl", "net.core.somaxconn=65535", "--sysctl", "net.ipv4.tcp_max_syn_backlog=65535",
           "-v", f"{ROOT}:/src:ro", "-v", "sg-linux-target:/work/interpreter/target",
           "-v", "sg-server-cargo:/usr/local/cargo/registry", "-v", "sg-server-target:/work/trials/target/server-bin-linux",
           "-v", f"{results}:/results",
           "localhost/sg-server", "bash", "-c", inner]
    return subprocess.run(cmd).returncode


if __name__ == "__main__":
    sys.exit(main() or 0)
