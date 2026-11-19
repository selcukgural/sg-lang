#!/usr/bin/env python3
"""Compiles each program three ways and prints the best of five runs of each: SG as a release
build (overflow wraps), SG with overflow checked, and the C written by hand. All three go through
the C compiler `sg build` uses (`cc` on Linux and macOS, Clang on Windows) with `-O2`, without
fused multiply-add (SG's rule for fractional numbers). Run from the `interpreter` folder, on any of
the three systems:

    python3 bench/run.py                  every program
    python3 bench/run.py fib nbody        only these
    python3 bench/run.py --runs 1 spawn   one run of each instead of the best of five, for a
                                          program that takes long (starting 5,000 programs on
                                          Windows)
    python3 bench/run.py -L folder ...    the libraries of `crc`, `checksum`, `sha256`, `deflate`,
                                          `sqlite`, `curl` and `json` (cJSON) are also looked for there (on Windows, vcpkg's `lib`
                                          and `bin`; their headers through `CPATH`)

A program whose C needs a library writes it on its first line, `/* link: -lz */`, and where the
library has another name on Windows on its second, `/* link on windows: -lzlib */`. SG writes the
libraries of its `extern` blocks on the first line of its C (`/* sg-link: ... */`).
"""

import argparse
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WINDOWS = os.name == "nt"
EXE = ".exe" if WINDOWS else ""
OUT = ROOT / "target" / "bench"

PROGRAMS = """fib primes collatz words list expr parse vector counts show lookup buffer bits hash nbody
numbers floats fields lines copy bytes table blocks keywords classes rewrite tree tasks async
asynclines tasklist asynccopy echo echosync echowork netlines ring crc deflate clock dates rng
secure listing status spawn capture pipelines pipesasync spawnasync case letters udpecho
udpechosync inline callback sqlite curl signals bitfields atomics queue statics checksum sha256 json log""".split()


def compiler():
    """The C compiler `sg build` uses (`src/cc.rs`)."""
    if not WINDOWS:
        return "cc"
    if os.environ.get("CC"):
        return os.environ["CC"]
    if shutil.which("clang"):
        return "clang"
    return r"C:\Program Files\LLVM\bin\clang.exe"


CC = compiler()
# On Windows as `sg build` compiles: LLVM's linker and C's library linked into the program.
FLAGS = ["-std=c11", "-O2", "-ffp-contract=off"] + (["-fuse-ld=lld", "-fms-runtime-lib=static"] if WINDOWS else [])
# The C written by hand calls `fopen` and its kin, which Microsoft's headers warn against (SG's
# runtime says the same in its C).
C_FLAGS = FLAGS + (["-D_CRT_SECURE_NO_WARNINGS"] if WINDOWS else [])
SYSTEM = [] if WINDOWS else ["-lm", "-pthread"]
SG = str(ROOT / "target" / "release" / f"sg{EXE}")


def run(command, **kwargs):
    done = subprocess.run(command, **kwargs)
    if done.returncode != 0:
        sys.exit(f"failed: {' '.join(map(str, command))}")
    return done


def first_lines(path, count):
    with open(path, encoding="utf-8") as text:
        return [text.readline() for _ in range(count)]


def libraries_of_c(path):
    """The libraries the C written by hand names for this system."""
    lines = first_lines(path, 2)
    if WINDOWS:
        found = re.match(r"/\* link on windows: (.*) \*/$", lines[1].strip())
        if found:
            return found.group(1).split()
    found = re.match(r"/\* link: (.*) \*/$", lines[0].strip())
    return found.group(1).split() if found else []


def libraries_of_sg(c_file):
    found = re.match(r"/\* sg-link: (.*) \*/$", first_lines(c_file, 1)[0].strip())
    return found.group(1).split() if found else []


RUNS = 5


def best(program, given, env):
    """The best of `RUNS` runs, in seconds."""
    times = []
    for _ in range(RUNS):
        with open(given, "rb") if given else open(os.devnull, "rb") as stdin:
            start = time.perf_counter()
            subprocess.run([program], stdin=stdin, stdout=subprocess.DEVNULL, check=True, env=env)
            times.append(time.perf_counter() - start)
    return min(times)


def printed(program, given, env):
    """What a program prints; a line of C on Windows ends with `\\r\\n`, one of SG with `\\n`."""
    with open(given, "rb") if given else open(os.devnull, "rb") as stdin:
        out = subprocess.run([program], stdin=stdin, stdout=subprocess.PIPE, check=True, env=env).stdout
    return out.replace(b"\r\n", b"\n")


# A program whose C cannot print what SG prints, and what of it is compared instead: `case.c` maps
# one character to one by the tables of the C library, so how much it writes depends on the system
# (Windows' C library writes three bytes more each line); the size of the text it starts from is the
# same.
FIRST_FIELD_ONLY = {"case"}


def same_output(name, outputs):
    if name in FIRST_FIELD_ONLY:
        outputs = [out.split(b",")[0] for out in outputs]
    return all(out == outputs[0] for out in outputs)


def make_inputs():
    """The input of the programs that read one, and the folder `listing` and `status` look at."""
    lines = OUT / "lines.in"
    if not lines.exists():
        r = random.Random(7)
        levels = ["INFO", "INFO", "INFO", "WARN", "ERROR", "DEBUG"]
        words = ["could", "not", "reach", "the", "database", "user", "request", "took", "longer", "than", "expected", "café", "retry"]
        with open(lines, "w", encoding="utf-8", newline="") as out:
            for i in range(2000000):
                text = " ".join(r.choice(words) for _ in range(r.randint(3, 14)))
                out.write(f"{r.choice(levels)} {r.randint(1, 5000)} {text}{chr(13) if i % 5 == 0 else ''}\n")
    document = OUT / "json.in"
    if not document.exists():
        r = random.Random(11)
        cities = ["London", "İstanbul", "São Paulo", "Zürich", "Kraków", "東京", "New York"]
        # Words as JSON writes them, escapes and all.
        words = ["quiet", r'\"quoted\"', r"tab\there", r"line\nbreak", "café", "naïve", r"back\\slash", r"\u00e9t\u00e9", "plain"]
        with open(document, "w", encoding="utf-8", newline="") as out:
            out.write("[")
            for i in range(100000):
                if i > 0:
                    out.write(",\n")
                bio = " ".join(r.choice(words) for _ in range(r.randint(2, 12)))
                tags = ", ".join(f'"tag{r.randint(1, 50)}"' for _ in range(r.randint(0, 5)))
                out.write(
                    f'{{"id": {r.randint(1, 2**40)}, "name": "user_{i}", "email": "user{i}@example.com", '
                    f'"score": {r.randint(0, 9999)}.{r.randint(1, 99):02d}{r.randint(1, 9)}, "ratio": {r.randint(1, 99999)}.{r.randint(1, 9)}e-{r.randint(1, 30)}, '
                    f'"active": {"true" if r.random() < 0.5 else "false"}, "manager": null, "tags": [{tags}], '
                    f'"address": {{"city": "{r.choice(cities)}", "zip": "{r.randint(10000, 99999)}", "lines": ["{r.randint(1, 999)} Main St", ""]}}, '
                    f'"bio": "{bio}"}}'
                )
            out.write("]\n")
    for name in ["asynclines", "netlines"]:
        shutil.copyfile(lines, OUT / f"{name}.in")
    listing = OUT / "listing"
    shutil.rmtree(listing, ignore_errors=True)
    listing.mkdir()
    for i in range(10000):
        (listing / f"file-{i:05}.txt").write_text("x" * (i % 17))


def row(name, release, checked, c, ratio):
    print(f"{name:<10} {release:>12} {checked:>12} {c:>10} {ratio:>14}", flush=True)


def measure(name, dirs, env):
    folders = [arg for folder in dirs for arg in ("-L", folder)]
    release = OUT / f"{name}-release{EXE}"
    run([SG, "build", f"bench/{name}.sg", "--release", "-o", release, *folders], cwd=ROOT, stdout=subprocess.DEVNULL)
    # The checked build, optimised like the others, to see what the check alone costs.
    c_file = OUT / f"{name}-checked.c"
    with open(c_file, "w", encoding="utf-8", newline="\n") as out:
        run([SG, "emit-c", f"bench/{name}.sg", *folders], cwd=ROOT, stdout=out)
    checked = OUT / f"{name}-checked{EXE}"
    run([CC, *FLAGS, "-o", checked, c_file, *folders, *libraries_of_sg(c_file), *SYSTEM], cwd=ROOT)
    c = OUT / f"{name}-c{EXE}"
    run([CC, *C_FLAGS, "-o", c, f"bench/{name}.c", *folders, *libraries_of_c(ROOT / "bench" / f"{name}.c"), *SYSTEM], cwd=ROOT)
    given = OUT / f"{name}.in"
    given = given if given.exists() else None
    # Each runs once before it is timed, which also leaves out the first start of a new program
    # (which on Windows took seconds now and then).
    if not same_output(name, [printed(program, given, env) for program in (release, checked, c)]):
        sys.exit(f"{name}: the programs print different things")
    times = [best(program, given, env) for program in (release, checked, c)]
    row(name, f"{times[0]:.3f}", f"{times[1]:.3f}", f"{times[2]:.3f}", f"{times[0] / times[2]:.2f}")


def measure_library(env):
    """A library C calls: one program of C (`library_main.c`) linked against the library built
    from `library.sg`, as a release build and with overflow checked, and against the same library
    written in C (`library.c`), each a static library compiled with `-O2`."""
    name = "library"
    static = "library.lib" if WINDOWS else "liblibrary.a"
    kinds = {kind: OUT / f"lib-{kind}" for kind in ("release", "checked", "c")}
    for folder in kinds.values():
        folder.mkdir(parents=True, exist_ok=True)
    run([SG, "build", f"bench/{name}.sg", "--library", "--release", "-o", kinds["release"] / static], cwd=ROOT, stdout=subprocess.DEVNULL)
    with open(kinds["checked"] / "library.c", "w", encoding="utf-8", newline="\n") as out:
        run([SG, "emit-c", f"bench/{name}.sg", "--library"], cwd=ROOT, stdout=out)
    sources = {"checked": kinds["checked"] / "library.c", "c": ROOT / "bench" / f"{name}.c"}
    for kind, source in sources.items():
        objects = kinds[kind] / "library.o"
        run([CC, *(C_FLAGS if kind == "c" else FLAGS), *([] if WINDOWS else ["-fPIC"]), "-c", "-o", objects, source], cwd=ROOT)
        archive = kinds[kind] / static
        archive.unlink(missing_ok=True)
        if WINDOWS:
            run([str(Path(CC).with_name("llvm-lib.exe")) if Path(CC).is_absolute() else "llvm-lib", f"/out:{archive}", objects])
        else:
            run(["ar", "rcs", archive, objects])
    programs = {}
    for kind, folder in kinds.items():
        programs[kind] = OUT / f"{name}-{kind}{EXE}"
        # On Windows the header declares the functions of a DLL unless told the library is static.
        static_define = ["-DLIBRARY_STATIC"] if WINDOWS else []
        run([CC, *C_FLAGS, *static_define, f"-I{kinds['release']}", "-o", programs[kind], f"bench/{name}_main.c", folder / static, *SYSTEM], cwd=ROOT)
    if not printed(programs["release"], None, env) == printed(programs["checked"], None, env) == printed(programs["c"], None, env):
        sys.exit(f"{name}: the programs print different things")
    times = [best(programs[kind], None, env) for kind in ("release", "checked", "c")]
    row(name, f"{times[0]:.3f}", f"{times[1]:.3f}", f"{times[2]:.3f}", f"{times[0] / times[2]:.2f}")


def measure_kernel():
    """The kernel of `examples/kernel` against the same kernel written in C (`kernel.c`), by size:
    both built with `-O2` for RISC-V, each function in a section of its own, and linked with the
    kernel's linker script, leaving out what nothing calls (`--gc-sections`); the code and the data
    that are loaded (`text`) are compared, the stack and the zeroed memory left out. Booted in QEMU,
    they must write the same, but the SG kernel's line about its heap. It needs a Clang that knows
    RISC-V (`CC`, or on Windows the one `sg build` finds), LLD and `qemu-system-riscv64`."""
    kcc = CC if WINDOWS else os.environ.get("CC", "clang")
    qemu = shutil.which("qemu-system-riscv64")
    if not qemu and WINDOWS and Path(r"C:\Program Files\qemu\qemu-system-riscv64.exe").exists():
        qemu = r"C:\Program Files\qemu\qemu-system-riscv64.exe"
    probe = Path(tempfile.gettempdir()) / "sg-bench-probe.c"
    probe.write_text("int f(void) { return 1; }\n")
    knows = subprocess.run([kcc, "--target=riscv64-unknown-elf", "-c", "-o", str(probe.with_suffix(".o")), str(probe)], capture_output=True).returncode == 0
    if not knows or not qemu:
        print("kernel: not measured, it needs a Clang that knows RISC-V (CC), LLD and qemu-system-riscv64")
        return
    machine = ["--target=riscv64-unknown-elf", "-mcmodel=medany", "-march=rv64gc", "-mabi=lp64d"]
    env = dict(os.environ, CC=kcc)
    run([SG, "build", "examples/kernel", "--freestanding", "--release", "--target", "riscv64-unknown-elf", "-o", OUT / "kernel-sg.o", "--", *machine[1:]], cwd=ROOT, env=env, stdout=subprocess.DEVNULL)
    run([kcc, *machine, "-std=c11", "-O2", "-ffreestanding", "-fno-stack-protector", "-ffunction-sections", "-fdata-sections", "-c", "-o", OUT / "kernel-c.o", "bench/kernel.c"], cwd=ROOT)
    said = {}
    for kind in ("sg", "c"):
        elf = OUT / f"kernel-{kind}.elf"
        run([kcc, *machine, "-nostdlib", "-fuse-ld=lld", "-Wl,--gc-sections", "-T", "examples/kernel/kernel.ld", OUT / f"kernel-{kind}.o", "-o", elf], cwd=ROOT)
        out = subprocess.run([qemu, "-machine", "virt", "-bios", "none", "-m", "128M", "-nographic", "-kernel", str(elf)], stdin=subprocess.DEVNULL, capture_output=True, check=True, timeout=120).stdout
        said[kind] = [line for line in out.decode().replace("\r", "").splitlines() if not line.startswith("memory: ")]
    if said["sg"] != said["c"]:
        sys.exit("kernel: the two kernels write different things")
    size_tool = shutil.which("llvm-size") or str(Path(shutil.which(kcc) or kcc).with_name(f"llvm-size{EXE}"))
    sizes = {}
    for kind in ("sg", "c"):
        text = run([size_tool, OUT / f"kernel-{kind}.elf"], capture_output=True).stdout.decode().splitlines()[1].split()
        sizes[kind] = int(text[0]) + int(text[1])
    row("kernel", f"{sizes['sg']} B", "", f"{sizes['c']} B", f"{sizes['sg'] / sizes['c']:.2f}")


def main():
    parser = argparse.ArgumentParser(description="SG against C written by hand")
    parser.add_argument("--runs", type=int, default=5, help="the runs of each program, of which the best is shown")
    parser.add_argument("-L", dest="dirs", action="append", default=[], help="a folder where libraries lie")
    parser.add_argument("names", nargs="*", help="the programs to measure (all when none)")
    args = parser.parse_args()
    global RUNS
    RUNS = max(1, args.runs)
    names = args.names or PROGRAMS + ["library", "kernel"]
    unknown = [name for name in names if name not in PROGRAMS + ["library", "kernel"]]
    if unknown:
        sys.exit(f"no such program: {' '.join(unknown)}")
    run(["cargo", "build", "--release", "--quiet"], cwd=ROOT)
    OUT.mkdir(parents=True, exist_ok=True)
    make_inputs()
    # On Windows, which has neither, `cat` and `true` are built for the programs that start them
    # (`bench/windows`), first on `PATH`; the DLLs of `-L` are found by the programs of C through
    # `PATH` too, and `sg build` copies them beside the programs it builds.
    env = dict(os.environ)
    if WINDOWS:
        tools = OUT / "tools"
        tools.mkdir(exist_ok=True)
        for tool in ("cat", "true"):
            run([CC, *C_FLAGS, "-o", tools / f"{tool}.exe", f"bench/windows/{tool}.c"], cwd=ROOT)
        env["PATH"] = os.pathsep.join([str(tools), *args.dirs, env.get("PATH", "")])
    row("program", "release (s)", "checked (s)", "C (s)", "release / C")
    for name in names:
        if name == "library":
            measure_library(env)
        elif name == "kernel":
            measure_kernel()
        else:
            measure(name, args.dirs, env)


if __name__ == "__main__":
    main()
