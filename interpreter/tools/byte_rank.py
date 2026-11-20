#!/usr/bin/env python3
"""Prints `sg_byte_rank`, how common each byte is in text, for the runtime of src/emit_c.rs.

    python3 -I tools/byte_rank.py

The bytes are counted in the text and code the repository keeps (its Markdown, Rust, SG, C,
Python, JSON and TOML, as `git ls-files` names them), and each is given its place among the 256:
0 for the rarest, 255 for the commonest, ties by the byte's value. A plain text is looked for by
its two rarest bytes (choice 1549), so the table is a guess at which bytes are rare: any table
gives the same answers, only sooner or later. It was made once and is made again only when wanted;
what it prints takes the place of the table in src/emit_c.rs.
"""

import collections
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
KINDS = ["*.md", "*.rs", "*.sg", "*.c", "*.py", "*.json", "*.toml"]


def main():
    names = subprocess.run(["git", "ls-files", *KINDS], cwd=ROOT, capture_output=True, text=True,
                           check=True).stdout.split("\n")
    counts = collections.Counter()
    for name in names:
        if name:
            counts.update((ROOT / name).read_bytes())
    order = sorted(range(256), key=lambda b: (counts[b], b))
    rank = [0] * 256
    for place, b in enumerate(order):
        rank[b] = place
    print("static const uint8_t sg_byte_rank[256] = {")
    for row in range(0, 256, 16):
        print("    " + ", ".join(f"{r:3}" for r in rank[row:row + 16]) + ",")
    print("};")


if __name__ == "__main__":
    main()
