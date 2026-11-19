#!/usr/bin/env python3
"""Makes src/unicode_tables.rs, the Unicode tables of SG, from the Unicode Character Database.

    python3 -I tools/unicode_tables.py [folder]

The four data files of the version below are read from `folder`; the ones missing are
downloaded into it first. Each file is checked against its SHA-256 before it is read, so the
tables come from these exact files. The data files are not kept in the repository; the tables
made from them are (src/unicode_tables.rs), and both the interpreter and the compiled C read
them, so the two give the same answers (choice 636).

When the Python running this knows the same version of Unicode (`unicodedata.unidata_version`),
every code point is checked against Python as well: the general category, and `upper()`,
`lower()` and `casefold()` of the character alone.
"""

import hashlib
import os
import sys
import tempfile
import unicodedata
import urllib.request

VERSION = "17.0.0"
FILES = {
    "UnicodeData.txt": "2e1efc1dcb59c575eedf5ccae60f95229f706ee6d031835247d843c11d96470c",
    "SpecialCasing.txt": "efc25faf19de21b92c1194c111c932e03d2a5eaf18194e33f1156e96de4c9588",
    "CaseFolding.txt": "ff8d8fefbf123574205085d6714c36149eb946d717a0c585c27f0f4ef58c4183",
    "DerivedCoreProperties.txt": "24c7fed1195c482faaefd5c1e7eb821c5ee1fb6de07ecdbaa64b56a99da22c08",
}
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src", "unicode_tables.rs")

# The bits of a record (see src/unicode.rs).
LETTER, NUMERIC, UPPER, LOWER, CONTROL, CASED, IGNORABLE = 1, 2, 4, 8, 16, 32, 64
SPECIAL_UPPER, SPECIAL_LOWER, SPECIAL_FOLD = 128, 256, 512
# The general category, five bits from bit 10: its place in this list, which is the order of the
# variants of `UnicodeCategory` (std/unicode.sg), with the surrogates, which no `char` is, last.
CATEGORIES = "Lu Ll Lt Lm Lo Mn Mc Me Nd Nl No Pc Pd Ps Pe Pi Pf Po Sm Sc Sk So Zs Zl Zp Cc Cf Co Cn Cs".split()
CATEGORY_SHIFT = 10

# The three levels of the lookup: TOP by the code >> 9, MID by the next five bits, LEAF by the
# last four.
TOP_SHIFT, MID_BITS, LEAF_BITS = 9, 5, 4
CODES = 0x110000

NOTICE = """\
// The data of these tables comes from the Unicode Character Database, under this notice:
//
// UNICODE LICENSE V3
//
// COPYRIGHT AND PERMISSION NOTICE
//
// Copyright © 1991-2025 Unicode, Inc.
//
// NOTICE TO USER: Carefully read the following legal agreement. BY
// DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
// SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
// TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
// DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of data files and any associated documentation (the "Data Files") or
// software and any associated documentation (the "Software") to deal in the
// Data Files or Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, and/or sell
// copies of the Data Files or Software, and to permit persons to whom the
// Data Files or Software are furnished to do so, provided that either (a)
// this copyright and permission notice appear with all copies of the Data
// Files or Software, or (b) this copyright and permission notice appear in
// associated Documentation.
//
// THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
// KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
// THIRD PARTY RIGHTS.
//
// IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
// BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
// OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
// WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
// ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
// FILES OR SOFTWARE.
//
// Except as contained in this notice, the name of a copyright holder shall
// not be used in advertising or otherwise to promote the sale, use or other
// dealings in these Data Files or Software without prior written
// authorization of the copyright holder.
"""


def fetch(folder):
    os.makedirs(folder, exist_ok=True)
    for name, digest in FILES.items():
        path = os.path.join(folder, name)
        if not os.path.exists(path):
            url = f"https://www.unicode.org/Public/{VERSION}/ucd/{name}"
            print(f"downloading {url}", file=sys.stderr)
            with urllib.request.urlopen(url, timeout=60) as response, open(path, "wb") as out:
                out.write(response.read())
        with open(path, "rb") as file:
            found = hashlib.sha256(file.read()).hexdigest()
        if found != digest:
            sys.exit(f"{path}: SHA-256 is {found}, expected {digest}")


def lines(folder, name):
    with open(os.path.join(folder, name), encoding="utf-8") as file:
        for line in file:
            line = line.split("#", 1)[0].strip()
            if line:
                yield [part.strip() for part in line.split(";")]


def codes(text):
    return [int(part, 16) for part in text.split()]


def read(folder):
    category = ["Cn"] * CODES
    upper, lower = {}, {}
    first = None
    for fields in lines(folder, "UnicodeData.txt"):
        code = int(fields[0], 16)
        if fields[1].endswith(", First>"):
            first = code
            continue
        if fields[1].endswith(", Last>"):
            for each in range(first, code + 1):
                category[each] = fields[2]
            continue
        category[code] = fields[2]
        if fields[12]:
            upper[code] = int(fields[12], 16)
        if fields[13]:
            lower[code] = int(fields[13], 16)

    cased, ignorable = set(), set()
    for fields in lines(folder, "DerivedCoreProperties.txt"):
        if fields[1] not in ("Cased", "Case_Ignorable"):
            continue
        low, _, high = fields[0].partition("..")
        span = range(int(low, 16), int(high or low, 16) + 1)
        (cased if fields[1] == "Cased" else ignorable).update(span)

    # Full case mappings that are not the simple one; the ones with a condition (a language,
    # or Final_Sigma, which the code does) are left out.
    full_upper, full_lower = {}, {}
    for fields in lines(folder, "SpecialCasing.txt"):
        if len(fields) > 4 and fields[4]:
            continue
        code = int(fields[0], 16)
        to_lower, to_upper = codes(fields[1]), codes(fields[3])
        if to_upper != [upper.get(code, code)]:
            full_upper[code] = to_upper
        if to_lower != [lower.get(code, code)]:
            full_lower[code] = to_lower

    # Case folding: C and F, not S (F is used in its place) and not T (Turkic, left out).
    # The delta of the record is the simple folding, C and S, which `(?i)` of a `Regex` compares
    # by (choice 1477); the full folding takes the table for every character with an F line, and
    # each S line has one, so S changes no full folding.
    fold, full_fold, simple_only = {}, {}, {}
    for fields in lines(folder, "CaseFolding.txt"):
        code, status, to = int(fields[0], 16), fields[1], codes(fields[2])
        if status == "C":
            fold[code] = to[0]
        elif status == "S":
            simple_only[code] = to[0]
        elif status == "F":
            full_fold[code] = to
    assert all(code in full_fold for code in simple_only), "an S line goes with an F line"
    fold.update(simple_only)
    return category, upper, lower, cased, ignorable, full_upper, full_lower, fold, full_fold


def build(data):
    category, upper, lower, cased, ignorable, full_upper, full_lower, fold, full_fold = data
    records, record_of, entries = [], {}, []
    for code in range(CODES):
        kind = category[code]
        bits = (
            (LETTER if kind[0] == "L" else 0)
            | (NUMERIC if kind[0] == "N" else 0)
            | (UPPER if kind == "Lu" else 0)
            | (LOWER if kind == "Ll" else 0)
            | (CONTROL if kind == "Cc" else 0)
            | (CASED if code in cased else 0)
            | (IGNORABLE if code in ignorable else 0)
            | (SPECIAL_UPPER if code in full_upper else 0)
            | (SPECIAL_LOWER if code in full_lower else 0)
            | (SPECIAL_FOLD if code in full_fold else 0)
            | (CATEGORIES.index(kind) << CATEGORY_SHIFT)
        )
        record = (bits, upper.get(code, code) - code, lower.get(code, code) - code, fold.get(code, code) - code)
        if record not in record_of:
            record_of[record] = len(records)
            records.append(record)
        entries.append(record_of[record])
    assert len(records) <= 256, "a leaf entry is one byte"

    leaf_size, mid_size = 1 << LEAF_BITS, 1 << MID_BITS
    leaves, leaf_of, leaf_index = [], {}, []
    for at in range(0, CODES, leaf_size):
        block = tuple(entries[at:at + leaf_size])
        if block not in leaf_of:
            leaf_of[block] = len(leaves)
            leaves.append(block)
        leaf_index.append(leaf_of[block])
    mids, mid_of, top = [], {}, []
    for at in range(0, len(leaf_index), mid_size):
        block = tuple(leaf_index[at:at + mid_size])
        if block not in mid_of:
            mid_of[block] = len(mids)
            mids.append(block)
        top.append(mid_of[block])
    assert len(mids) <= 256 and len(leaves) <= 65536
    assert len(top) == CODES >> TOP_SHIFT

    def special(table):
        return sorted((code, to + [0] * (3 - len(to))) for code, to in table.items())

    return {
        "records": records,
        "top": top,
        "mid": [index for block in mids for index in block],
        "leaf": [entry for block in leaves for entry in block],
        "upper": special(full_upper),
        "lower": special(full_lower),
        "fold": special(full_fold),
        "entries": entries,
    }


def check(tables):
    """Every code point against Python's own tables, when they are of the same version."""
    if unicodedata.unidata_version != VERSION:
        print(f"Python knows Unicode {unicodedata.unidata_version}, not {VERSION}: not checked", file=sys.stderr)
        return
    records, entries = tables["records"], tables["entries"]
    specials = {name: dict(tables[name]) for name in ("upper", "lower", "fold")}

    def full(code, name, delta, bit):
        bits = records[entries[code]][0]
        if bits & bit:
            return "".join(chr(each) for each in specials[name][code] if each)
        return chr(code + records[entries[code]][delta])

    wrong = 0
    for code in range(CODES):
        if 0xD800 <= code <= 0xDFFF:
            continue
        char = chr(code)
        kind = unicodedata.category(char)
        bits = records[entries[code]][0]
        expected = (
            (LETTER if kind[0] == "L" else 0) | (NUMERIC if kind[0] == "N" else 0) | (UPPER if kind == "Lu" else 0)
            | (LOWER if kind == "Ll" else 0) | (CONTROL if kind == "Cc" else 0)
        )
        if bits & 31 != expected:
            wrong += 1
            print(f"U+{code:04X}: category bits {bits & 31}, Python {expected}", file=sys.stderr)
        if CATEGORIES[bits >> CATEGORY_SHIFT] != kind:
            wrong += 1
            print(f"U+{code:04X}: category {CATEGORIES[bits >> CATEGORY_SHIFT]}, Python {kind}", file=sys.stderr)
        for name, delta, bit, python in (("upper", 1, SPECIAL_UPPER, char.upper()), ("lower", 2, SPECIAL_LOWER, char.lower()),
                                         ("fold", 3, SPECIAL_FOLD, char.casefold())):
            ours = full(code, name, delta, bit)
            if ours != python:
                wrong += 1
                print(f"U+{code:04X}: {name} {ours!r}, Python {python!r}", file=sys.stderr)
    if wrong:
        sys.exit(f"{wrong} differences from Python")
    print(f"checked every code point against Python's Unicode {VERSION}", file=sys.stderr)


def rows(values, width, per_line=24):
    out = []
    for at in range(0, len(values), per_line):
        out.append("    " + " ".join(f"{value:>{width}}," for value in values[at:at + per_line]))
    return "\n".join(out)


def write(tables):
    records = tables["records"]
    out = [
        f"// Made by tools/unicode_tables.py from the Unicode Character Database {VERSION}; do not edit.",
        "//",
        NOTICE.rstrip("\n"),
        "",
        "//! The Unicode tables: one record for each code point, found in three steps, and the",
        "//! mappings to more than one character. See src/unicode.rs.",
        "",
        "#![cfg_attr(rustfmt, rustfmt_skip)]",
        "",
        f'pub const VERSION: &str = "{VERSION}";',
        f"pub const TOP_SHIFT: u32 = {TOP_SHIFT};",
        f"pub const MID_BITS: u32 = {MID_BITS};",
        f"pub const LEAF_BITS: u32 = {LEAF_BITS};",
        f"pub const CATEGORY_SHIFT: u32 = {CATEGORY_SHIFT};",
        "",
        "/// The bits of what a character is (its general category from bit `CATEGORY_SHIFT`), then",
        "/// what its upper case, its lower case and its folded case add to its code (the simple",
        "/// mappings; the folding of the C and S lines of CaseFolding.txt).",
        f"pub const RECORDS: [(u16, i32, i32, i32); {len(records)}] = [",
        rows([f"({bits}, {up}, {low}, {fold})" for bits, up, low, fold in records], 0, 6),
        "];",
        f"pub const TOP: [u8; {len(tables['top'])}] = [",
        rows(tables["top"], 3),
        "];",
        f"pub const MID: [u16; {len(tables['mid'])}] = [",
        rows(tables["mid"], 3),
        "];",
        f"pub const LEAF: [u8; {len(tables['leaf'])}] = [",
        rows(tables["leaf"], 3),
        "];",
    ]
    for name, what in (("upper", "UPPER"), ("lower", "LOWER"), ("fold", "FOLD")):
        items = tables[name]
        out.append(f"/// The full {name} {'case' if name != 'fold' else 'folding'} of the characters it is more than one character for (or a")
        out.append("/// character other than the simple one), by code; the rest of the three is zero.")
        out.append(f"pub const SPECIAL_{what}: [(u32, [u32; 3]); {len(items)}] = [")
        out.append(rows([f"(0x{code:04X}, [0x{to[0]:04X}, 0x{to[1]:04X}, 0x{to[2]:04X}])" for code, to in items], 0, 3))
        out.append("];")
    with open(OUT, "w", encoding="utf-8") as file:
        file.write("\n".join(out) + "\n")
    size = len(tables["top"]) + 2 * len(tables["mid"]) + len(tables["leaf"]) + 16 * len(records)
    print(f"wrote {os.path.normpath(OUT)}: {len(records)} records, {size} bytes of lookup", file=sys.stderr)


def main():
    folder = sys.argv[1] if len(sys.argv) > 1 else os.path.join(tempfile.gettempdir(), f"ucd-{VERSION}")
    fetch(folder)
    tables = build(read(folder))
    check(tables)
    write(tables)


if __name__ == "__main__":
    main()
