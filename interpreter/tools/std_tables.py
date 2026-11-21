#!/usr/bin/env python3
"""Writes the tables of numbers that the language's own module holds as consts, as zlib keeps
crc32.h: computed here once, written into the source as numbers.

    python3 -I tools/std_tables.py           writes them into src/std
    python3 -I tools/std_tables.py --check   says whether what src/std holds is what this makes

A const is made of numbers, texts, other consts and operators, and calls no function (choice
1606), so a table that is computed is computed here (choice 1608). Each table lies in its file
between two marked lines; what lies between them is replaced, the rest of the file is left as
it is.
"""

import os
import sys
import textwrap

STD = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src", "std")
BEGIN = "// The tables below are made by tools/std_tables.py: run it again rather than editing them."
END = "// End of the tables made by tools/std_tables.py."


def crc32_tables():
    """Sixteen tables of 256 numbers for CRC-32 (reflected, polynomial 0xEDB88320), slicing by
    16: table `k` (from `k * 256`) gives what a byte is worth when `k` more bytes follow it."""
    tables = [0] * 4096
    for n in range(256):
        c = n
        for _ in range(8):
            c = 0xEDB88320 ^ (c >> 1) if c & 1 else c >> 1
        tables[n] = c
    for n in range(256):
        for k in range(1, 16):
            c = tables[(k - 1) * 256 + n]
            tables[k * 256 + n] = tables[c & 255] ^ (c >> 8)
    return tables


# RFC 1951, 3.2.5: the extra bits of each length code and of each distance code.
EXTRA_LENGTH = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0]
EXTRA_DIST = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13]


def length_codes():
    """The length code of a length less 3."""
    codes = [0] * 256
    length = 0
    for code in range(28):
        for _ in range(1 << EXTRA_LENGTH[code]):
            codes[length] = code
            length += 1
    codes[255] = 28
    return codes


def dist_codes():
    """The distance code of a distance less 1: below 256 the distance itself, above it 256 and
    the distance divided by 128."""
    codes = [0] * 512
    dist = 0
    for code in range(16):
        for _ in range(1 << EXTRA_DIST[code]):
            codes[dist] = code
            dist += 1
    dist >>= 7
    for code in range(16, 30):
        for _ in range(1 << (EXTRA_DIST[code] - 7)):
            codes[256 + dist] = code
            dist += 1
    return codes


def static_lengths():
    """The lengths of the fixed code of literals and lengths (RFC 1951, 3.2.6)."""
    return [8 if n <= 143 else 9 if n <= 255 else 7 if n <= 279 else 8 for n in range(288)]


def static_codes():
    """The codes of the fixed code of literals and lengths, their bits reversed as they are sent."""
    lengths = static_lengths()
    counts = [0] * 16
    for length in lengths:
        counts[length] += 1
    following = [0] * 16
    code = 0
    for bits in range(1, 16):
        code = (code + counts[bits - 1]) << 1
        following[bits] = code
    codes = []
    for length in lengths:
        reversed_code = int(format(following[length], f"0{length}b")[::-1], 2)
        codes.append(reversed_code)
        following[length] += 1
    return codes


def written(about, ty, name, numbers, per_line, form):
    """A const of the array type `ty[len]`, its numbers `per_line` to a line, under a comment
    made of the docstring of the function that computes it."""
    lines = ["// " + line for line in textwrap.wrap(" ".join(about.__doc__.split()), 100)]
    lines.append(f"const {ty}[{len(numbers)}] {name} = {{")
    for start in range(0, len(numbers), per_line):
        part = ", ".join(form(number) for number in numbers[start:start + per_line])
        lines.append(f"    {part}{',' if start + per_line < len(numbers) else ''}")
    lines.append("}")
    return "\n".join(lines)


def tables():
    """Each file and the text of the tables it holds."""
    hexadecimal = lambda number: f"0x{number >> 16:04X}_{number & 0xFFFF:04X}"
    return {
        "checksum.sg": written(crc32_tables, "uint", "Crc32Tables", crc32_tables(), 8, hexadecimal),
        "compression.sg": "\n\n".join([
            written(length_codes, "byte", "DeflateLengthCode", length_codes(), 32, str),
            written(dist_codes, "byte", "DeflateDistCode", dist_codes(), 32, str),
            written(static_lengths, "int", "DeflateStaticLength", static_lengths(), 32, str),
            written(static_codes, "int", "DeflateStaticCode", static_codes(), 16, str),
        ]),
    }


def replaced(text, made, file):
    """The text of a file with what lies between the marked lines replaced by `made`."""
    begin = text.find(BEGIN)
    end = text.find(END)
    if begin < 0 or end < begin:
        sys.exit(f"{file} has no lines that mark where the tables lie")
    return text[:begin] + BEGIN + "\n" + made + "\n" + text[end:]


def main():
    check = sys.argv[1:] == ["--check"]
    if sys.argv[1:] not in ([], ["--check"]):
        sys.exit("usage: python3 -I tools/std_tables.py [--check]")
    differ = []
    for file, made in tables().items():
        path = os.path.join(STD, file)
        with open(path, encoding="utf-8", newline="") as source:
            text = source.read()
        new = replaced(text, made, file)
        if new == text:
            continue
        differ.append(file)
        if not check:
            with open(path, "w", encoding="utf-8", newline="") as source:
                source.write(new)
    if check and differ:
        sys.exit(f"the tables differ from what tools/std_tables.py makes: {', '.join(differ)}")
    print("written: " + ", ".join(differ) if differ and not check else "the tables are as tools/std_tables.py makes them")


main()
