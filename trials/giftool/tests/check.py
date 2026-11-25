#!/usr/bin/env python3
"""Holds builds of `giftool` to the model of `model.py` (choice 1880). Run from the repository's root:

    python3 -I trials/giftool/tests/check.py [--interpreted] [--gifsicle PATH] [--inputs DIR] BINARY...

The GIFs are those of `inputs.py` (made into `trials/target/giftool-tests` unless `--inputs` names a
folder), but for the large ones of the measurements. First the model is held to gifsicle: on each GIF that
is not broken its `--info` must be `gifsicle -I`'s text, and what gifsicle's `--unoptimize` writes must
draw as the model draws the input. Then each build is run on each GIF with each option of `OPTIONS`:
what it writes, read back by the model and drawn, must show, screen for screen, what the model draws of
the input, cropped, flipped and turned, and keep the delays, the loop and the comments; gifsicle must read
it with no warning; `--info` must be the model's text; a broken GIF must end with 1 and write nothing.
Every build must also write the same bytes as the first one named. `--interpreted` adds the SG version
run by the interpreter (`interpreter/target/release/sg`), on the small GIFs only.
"""

import argparse
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE))
import inputs  # noqa: E402
import model  # noqa: E402

OPTIONS = [
    [], ["--unoptimize"], ["-O1"], ["-O2"], ["--crop", "3,2+40x30"], ["--crop", "0,0+1x1"],
    ["--crop", "60,40+100000x100000"], ["--flip-horizontal"], ["--flip-vertical"], ["--rotate-90"],
    ["--rotate-180"], ["--rotate-270"], ["--crop", "5,1+25x33", "--flip-horizontal", "--rotate-90"],
    ["--crop", "2,3+31x17", "--flip-vertical", "--rotate-270"], ["-O2", "--crop", "1,1+50x40", "--rotate-270"],
    ["-O1", "--flip-horizontal", "--rotate-180"], ["--unoptimize", "--flip-vertical"], ["--unoptimize", "-O2"],
    ["--crop", "70000,0+5x5"],
]
WRONG_USES = [["-O3"], ["--crop", "1,2+3"], ["--crop", "1,2+0x4"], ["--rotate-90", "--rotate-180"],
              ["--flip-horizontal", "--flip-vertical"], ["--sideways"], ["a.gif", "b.gif"]]
SMALL = 200_000  # bytes: the GIFs the interpreter is run on


def run(cmd, data=None, timeout=600):
    return subprocess.run(cmd, input=data, capture_output=True, timeout=timeout)


def expected(g, opts):
    """What must be seen: the screens, the size, or None when the options must be refused (1)."""
    crop = None
    if "--crop" in opts:
        crop = model.parse_crop(opts[opts.index("--crop") + 1])
        if crop[0] >= g.width or crop[1] >= g.height:
            return None
    if not g.frames:
        return None
    flip = "h" if "--flip-horizontal" in opts else "v" if "--flip-vertical" in opts else None
    rot = 90 if "--rotate-90" in opts else 180 if "--rotate-180" in opts else 270 if "--rotate-270" in opts else 0
    return model.transform(model.composites(g), g.width, g.height, crop, flip, rot)


def too_many_colors(screens):
    """Whether a screen shows more colors than a frame written whole may hold (choice 1876)."""
    for s in screens:
        colors = set(s)
        if len(colors) > 256 or (len(colors) == 256 and model.CLEAR in colors):
            return True
    return False


def check_model(folder, gifsicle, problems):
    # `--info` is held to gifsicle 1.99; 1.96, Debian's, writes the comments of the end before the loop.
    version = run([gifsicle, "--version"], timeout=60).stdout.decode().splitlines()[0]
    same_info = "1.99" in version
    if not same_info:
        print(f"{version}: --info is held to gifsicle 1.99, not to this one")
    for path in sorted(folder.glob("*.gif")):
        data = path.read_bytes()
        try:
            g = model.read(data)
        except model.GifError:
            continue
        info = subprocess.run([gifsicle, "-I", path.name], cwd=folder, capture_output=True, timeout=60)
        if same_info and info.stdout.decode("latin-1") != model.info(g, path.name):
            problems.append(f"model: --info of {path.name} is not gifsicle's")
        if not g.frames:
            continue
        un = run([gifsicle, "--unoptimize", str(path)], timeout=120)
        if model.composites(model.read(un.stdout)) != model.composites(g):
            problems.append(f"model: gifsicle --unoptimize of {path.name} draws otherwise")


def check_build(name, command, folder, gifsicle, problems, small_only, reference):
    ran = 0
    for path in sorted(folder.glob("*.gif")):
        data = path.read_bytes()
        if small_only and len(data) > SMALL:
            continue
        broken = False
        try:
            g = model.read(data)
        except model.GifError:
            broken = True
            try:
                g = model.read(data, decode=False)
            except model.GifError:
                g = None
        # --info, from a file and from the input
        r = subprocess.run(command + ["--info", path.name], cwd=folder, capture_output=True, timeout=120)
        ran += 1
        if g is None:
            if r.returncode != 1 or r.stdout:
                problems.append(f"{name}: --info {path.name}: {r.returncode}, not 1 with nothing written")
        elif r.returncode != 0 or r.stdout.decode("latin-1") != model.info(g, path.name):
            problems.append(f"{name}: --info {path.name} is not gifsicle's text ({r.returncode})")
        if g is not None and not small_only:
            r = run(command + ["--info"], data)
            if r.stdout.decode("latin-1") != model.info(g, "<stdin>"):
                problems.append(f"{name}: --info of the input {path.name} is not gifsicle's text")
        for opts in OPTIONS:
            r = run(command + opts + [str(path)])
            ran += 1
            key = (path.name, tuple(opts))
            if reference is not None:
                if key in reference and reference[key] != (r.returncode, r.stdout):
                    problems.append(f"{name}: {path.name} {' '.join(opts)}: not the bytes of the first build")
            if broken:
                if r.returncode != 1 or r.stdout:
                    problems.append(f"{name}: {path.name} {' '.join(opts)}: broken, but {r.returncode}")
                continue
            want = expected(g, opts)
            if want is None:
                if r.returncode != 1 or r.stdout:
                    problems.append(f"{name}: {path.name} {' '.join(opts)}: {r.returncode}, not 1")
                continue
            screens, W, H = want
            together = "--unoptimize" in opts or any(o.startswith("-O") for o in opts)
            if together and r.returncode == 1 and b"256 colors" in r.stderr:
                # -O may fit where --unoptimize does not; refused only when some screen needs more
                if not too_many_colors(screens) and "--unoptimize" in opts and not any(o.startswith("-O") for o in opts):
                    problems.append(f"{name}: {path.name} {' '.join(opts)}: refused for its colors, but they fit")
                continue
            if r.returncode != 0:
                problems.append(f"{name}: {path.name} {' '.join(opts)}: ended with {r.returncode}: {r.stderr[:200]!r}")
                continue
            try:
                g2 = model.read(r.stdout)
            except model.GifError as e:
                problems.append(f"{name}: {path.name} {' '.join(opts)}: wrote what is not a GIF: {e}")
                continue
            if (g2.width, g2.height) != (W, H) or model.composites(g2) != screens:
                problems.append(f"{name}: {path.name} {' '.join(opts)}: the screens are not the input's")
                continue
            if [f.delay for f in g2.frames] != [f.delay for f in g.frames] or g2.loop != g.loop:
                problems.append(f"{name}: {path.name} {' '.join(opts)}: the delays or the loop changed")
            if [f.comments for f in g2.frames] != [f.comments for f in g.frames] or g2.end_comments != g.end_comments:
                problems.append(f"{name}: {path.name} {' '.join(opts)}: the comments changed")
            gs = run([gifsicle, "-I"], r.stdout, timeout=120)
            if gs.returncode != 0 or gs.stderr:
                problems.append(f"{name}: {path.name} {' '.join(opts)}: gifsicle says {gs.stderr[:200]!r}")
            # -o writes the same bytes into a file
            if opts == ["-O2"]:
                out = folder.parent / f"check-{os.getpid()}.gif"
                r2 = run(command + opts + ["-o", str(out), str(path)])
                if r2.returncode != 0 or out.read_bytes() != r.stdout:
                    problems.append(f"{name}: {path.name} -o: not what the output had")
                out.unlink(missing_ok=True)
    for opts in WRONG_USES:
        r = run(command + opts, b"")
        ran += 1
        if r.returncode != 2 or r.stdout:
            problems.append(f"{name}: {' '.join(opts)}: {r.returncode}, not 2")
    one = folder / "one-pixel.gif"
    for missing in (["--info", "missing.gif"], ["missing.gif"]):
        r = run(command + missing)
        ran += 1
        if r.returncode != 1 or r.stdout:
            problems.append(f"{name}: {' '.join(missing)}: {r.returncode}, not 1")
    r = run(command + ["--info", str(one), "missing.gif", str(one)])
    ran += 1
    if r.returncode != 1 or r.stdout.count(b"* ") != 2:
        problems.append(f"{name}: --info goes on past a missing file")
    # the output closed before anything is read
    p = subprocess.Popen(command + [str(folder / "test-animation.gif")], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    p.stdout.close()
    p.wait(timeout=60)
    ran += 1
    if p.returncode == 0:
        problems.append(f"{name}: a closed output ended with 0")
    return ran


def outputs(command, folder, small_only):
    got = {}
    for path in sorted(folder.glob("*.gif")):
        if small_only and path.stat().st_size > SMALL:
            continue
        for opts in OPTIONS:
            r = run(command + opts + [str(path)])
            got[(path.name, tuple(opts))] = (r.returncode, r.stdout)
    return got


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("binaries", nargs="*")
    ap.add_argument("--interpreted", action="store_true")
    ap.add_argument("--gifsicle", default="gifsicle")
    ap.add_argument("--inputs")
    args = ap.parse_args()
    folder = Path(args.inputs) if args.inputs else ROOT / "trials" / "target" / "giftool-tests"
    inputs.write_all(str(folder), measured=False)
    problems = []
    check_model(folder, args.gifsicle, problems)
    builds = [(b, [str(Path(b).resolve())], False) for b in args.binaries]
    if args.interpreted:
        sg = ROOT / "interpreter" / "target" / "release" / "sg"
        builds.append(("interpreted", [str(sg), str(ROOT / "trials" / "giftool" / "sg" / "giftool.sg")], True))
    reference = None
    total = 0
    for i, (name, command, small_only) in enumerate(builds):
        if i == 0:
            reference = outputs(command, folder, small_only)
        total += check_build(name, command, folder, args.gifsicle, problems, small_only, reference if i else None)
    for p in problems:
        print(p)
    print(f"{total} runs of {len(builds)} builds, {len(problems)} problems")
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()
