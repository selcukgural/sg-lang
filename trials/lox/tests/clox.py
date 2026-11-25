#!/usr/bin/env python3
"""clox and the tests of Lox, fetched at a commit of their own and built (choice 1888). Run from the
repository's root:

    python3 -I trials/lox/tests/clox.py

Nothing of them comes into the repository: the clone of "Crafting Interpreters" (MIT) goes into
`trials/target/lox/craftinginterpreters`, at `COMMIT`, and clox is built there by its own Makefile, in
release (`-O3 -flto`), with `$CC` or `cc`. Prints the path of clox, and of `clox-ip-local`: clox with
the place in the code held in a local variable, the first challenge of the chapter "Calls and
Functions" (choice 1894), made from `c/vm.c` by `ip_local` below and built the same way.
"""

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
REPOSITORY = "https://github.com/munificent/craftinginterpreters.git"
# The last commit of the book's repository when the trial was written (1 August 2024).
COMMIT = "4a840f70f69c6ddd17cfef4f6964f8e1bcd8c3d4"
FOLDER = ROOT / "trials" / "target" / "lox" / "craftinginterpreters"


def run(cmd, cwd=None):
    subprocess.run(cmd, cwd=cwd, check=True, timeout=600)


def fetch():
    """The clone at `COMMIT`."""
    if not (FOLDER / ".git").exists():
        FOLDER.parent.mkdir(parents=True, exist_ok=True)
        run(["git", "clone", "--quiet", "--filter=blob:none", "--no-checkout", REPOSITORY, str(FOLDER)])
    have = subprocess.run(["git", "rev-parse", "HEAD"], cwd=FOLDER, capture_output=True, text=True).stdout.strip()
    if have != COMMIT or not (FOLDER / "util" / "c.make").exists():
        run(["git", "-c", "advice.detachedHead=false", "checkout", "--quiet", "--force", COMMIT], cwd=FOLDER)
    return FOLDER


def build(cc=None):
    """clox, built in release; its path."""
    folder = fetch()
    clox = folder / "build" / "clox"
    if not clox.exists():
        cc = cc or os.environ.get("CC") or "cc"
        run(["make", "-f", "util/c.make", "NAME=clox", "MODE=release", "SOURCE_DIR=c", f"CC={cc}"], cwd=folder)
    return clox


def ip_local(source):
    """`run()` of clox's `vm.c` with `ip` in a local: read through `ip`, written back to the frame
    before anything that reads it there (a call, an error) and read again when the frame changes."""
    start = source.index("static InterpretResult run() {")
    end = source.index("#undef READ_BYTE", start)
    body = source[start:end]

    def swap(old, new, count=None):
        nonlocal body
        found = body.count(old)
        assert found and (count is None or found == count), (old, found)
        body = body.replace(old, new)

    swap("  CallFrame* frame = &vm.frames[vm.frameCount - 1];\n",
         "  CallFrame* frame = &vm.frames[vm.frameCount - 1];\n  register uint8_t* ip = frame->ip;\n", 1)
    swap("#define READ_BYTE() (*frame->ip++)", "#define READ_BYTE() (*ip++)", 1)
    swap("(frame->ip += 2, \\\n    (uint16_t)((frame->ip[-2] << 8) | frame->ip[-1]))",
         "(ip += 2, \\\n    (uint16_t)((ip[-2] << 8) | ip[-1]))", 1)
    swap("frame->ip += offset;", "ip += offset;")
    swap("frame->ip -= offset;", "ip -= offset;")
    body, changed = re.subn(r"(?<!CallFrame\* )frame = &vm\.frames\[vm\.frameCount - 1\];\n",
                            "frame = &vm.frames[vm.frameCount - 1]; ip = frame->ip;\n", body)
    assert changed, "no frame changes in run()"
    swap("runtimeError(", "frame->ip = ip; runtimeError(")
    for call in ("callValue(", "invoke(", "invokeFromClass(", "bindMethod("):
        swap("if (!" + call, "frame->ip = ip; if (!" + call)
    return source[:start] + body + source[end:]


def build_ip_local(cc=None):
    """clox with `ip` in a local, built in release; its path."""
    folder = fetch()
    clox = folder / "build" / "clox-ip-local"
    if not clox.exists():
        changed = folder / "c-ip-local"
        changed.mkdir(exist_ok=True)
        for path in (folder / "c").iterdir():
            if not path.is_file():
                continue
            text = path.read_text()
            (changed / path.name).write_text(ip_local(text) if path.name == "vm.c" else text)
        cc = cc or os.environ.get("CC") or "cc"
        run(["make", "-f", "util/c.make", "NAME=clox-ip-local", "MODE=release", "SOURCE_DIR=c-ip-local", f"CC={cc}"], cwd=folder)
    return clox


def source_lines():
    """The lines of clox's C that are code: neither blank nor a comment. The book keeps in its sources
    the code of earlier chapters, in comments of their own (`/* A Virtual Machine run < ... */`), and
    marks where each chapter's code starts and ends (`//> ...`, `//< ...`); none of it counts."""
    count = 0
    for path in sorted((fetch() / "c").glob("*.[ch]")):
        text = re.sub(r"/\*.*?\*/", "", path.read_text(), flags=re.S)
        for line in text.splitlines():
            line = line.strip()
            if line and not line.startswith("//"):
                count += 1
    return count


def tests():
    """The folder of the tests of Lox."""
    return fetch() / "test"


if __name__ == "__main__":
    print(build(sys.argv[1] if len(sys.argv) > 1 else None))
    print(build_ip_local(sys.argv[1] if len(sys.argv) > 1 else None))
