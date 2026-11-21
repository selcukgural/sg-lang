#!/usr/bin/env python3
"""Makes src/unicode_tables.rs, the Unicode tables of SG, from the Unicode Character Database.

    python3 -I tools/unicode_tables.py [folder]

The five data files of the version below are read from `folder`; the ones missing are
downloaded into it first. Each file is checked against its SHA-256 before it is read, so the
tables come from these exact files. The data files are not kept in the repository; the tables
made from them are (src/unicode_tables.rs), and both the interpreter and the compiled C read
them, so the two give the same answers (choice 636).

When the Python running this knows the same version of Unicode (`unicodedata.unidata_version`),
every code point is checked against Python as well: the general category, `upper()`,
`lower()` and `casefold()` of the character alone, its combining class, the four normal forms
of it alone, and each pair that composes (choice 1614).
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
    "DerivedNormalizationProps.txt": "71fd6a206a2c0cdd41feb6b7f656aa31091db45e9cedc926985d718397f9e488",
}
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src", "unicode_tables.rs")

# The bits of a record (see src/unicode.rs).
LETTER, NUMERIC, UPPER, LOWER, CONTROL, CASED, IGNORABLE = 1, 2, 4, 8, 16, 32, 64
SPECIAL_UPPER, SPECIAL_LOWER, SPECIAL_FOLD = 128, 256, 512
# The general category, five bits from bit 10: its place in this list, which is the order of the
# variants of `UnicodeCategory` (std/unicode.sg), with the surrogates, which no `char` is, last.
CATEGORIES = "Lu Ll Lt Lm Lo Mn Mc Me Nd Nl No Pc Pd Ps Pe Pi Pf Po Sm Sc Sk So Zs Zl Zp Cc Cf Co Cn Cs".split()
CATEGORY_SHIFT = 10

# The bits of a record of normalization (see src/unicode.rs): what the quick check of each form
# says of the character (UAX #15, DerivedNormalizationProps.txt; a bit for No and one for Maybe),
# and whether it is the second of a pair that composes.
NFD_NO, NFC_NO, NFC_MAYBE, NFKD_NO, NFKC_NO, NFKC_MAYBE, COMBINES = 1, 2, 4, 8, 16, 32, 64
HANGUL_FIRST, HANGUL_COUNT = 0xAC00, 11172

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


def read_normalization(folder):
    """The combining class, the decomposition (the mapping of UnicodeData.txt, `compat` when it
    has a tag) and the quick checks and the exclusions from composition."""
    ccc, mapping = [0] * CODES, {}
    first = None
    for fields in lines(folder, "UnicodeData.txt"):
        code = int(fields[0], 16)
        if fields[1].endswith(", First>"):
            first = code
            continue
        if fields[1].endswith(", Last>"):
            assert int(fields[3]) == 0 and not fields[5], "a range has no class and no mapping"
            continue
        ccc[code] = int(fields[3])
        if fields[5]:
            rest = fields[5].partition(">")[2]
            compat = fields[5].startswith("<")
            mapping[code] = (compat, codes(rest if compat else fields[5]))
    quick, excluded = {}, set()
    bit = {("NFD_QC", "N"): NFD_NO, ("NFC_QC", "N"): NFC_NO, ("NFC_QC", "M"): NFC_MAYBE,
           ("NFKD_QC", "N"): NFKD_NO, ("NFKC_QC", "N"): NFKC_NO, ("NFKC_QC", "M"): NFKC_MAYBE}
    for fields in lines(folder, "DerivedNormalizationProps.txt"):
        low, _, high = fields[0].partition("..")
        span = range(int(low, 16), int(high or low, 16) + 1)
        if fields[1] == "Full_Composition_Exclusion":
            excluded.update(span)
        elif len(fields) > 2 and (fields[1], fields[2]) in bit:
            for code in span:
                quick[code] = quick.get(code, 0) | bit[(fields[1], fields[2])]
    return ccc, mapping, quick, excluded


def build_normalization(ccc, mapping, quick, excluded):
    """A record for each code point (its class, its bits and where its two full decompositions
    are in one list of code points), found in three steps as the records of case are, and the
    pairs that compose, sorted."""
    def full(code, compat):
        found = mapping.get(code)
        if found is None or (found[0] and not compat):
            return [code]
        return [each for part in found[1] for each in full(part, compat)]

    def ordered(codes_):
        # The canonical order: a stable sort of each run of characters with a class.
        out, at = list(codes_), 0
        while at < len(out):
            if ccc[out[at]] == 0:
                at += 1
                continue
            end = at
            while end < len(out) and ccc[out[end]] != 0:
                end += 1
            out[at:end] = sorted(out[at:end], key=lambda each: ccc[each])
            at = end
        return out

    pairs = {}
    for code, (compat, to) in mapping.items():
        if not compat and len(to) == 2 and code not in excluded:
            pairs[(to[0], to[1])] = code
    # The vowels and the final consonants of Hangul compose by Hangul's own rule.
    seconds = {second for _, second in pairs} | set(range(0x1161, 0x1176)) | set(range(0x11A8, 0x11C3))

    pool, place, records, record_of, entries = [], {}, [], {}, []

    def placed(codes_):
        if not codes_:
            return 0
        key = tuple(codes_)
        if key not in place:
            place[key] = len(pool)
            pool.extend(codes_)
        return place[key]

    for code in range(CODES):
        bits = quick.get(code, 0) | (COMBINES if code in seconds else 0)
        canon = compat = []
        if code in mapping and not HANGUL_FIRST <= code < HANGUL_FIRST + HANGUL_COUNT:
            canon = ordered(full(code, False))
            compat = ordered(full(code, True))
            if canon == [code]:
                canon = []
            if compat == [code]:
                compat = []
        record = (ccc[code], bits, placed(canon), len(canon), placed(compat), len(compat))
        if record not in record_of:
            record_of[record] = len(records)
            records.append(record)
        entries.append(record_of[record])
    assert len(records) <= 65536 and len(pool) <= 65536
    assert all(record[3] <= 255 and record[5] <= 255 for record in records)
    assert all(code < 1 << 21 for code in pool)
    # The second of a pair is Maybe to the quick checks of NFC and NFKC, which say the same (so
    # is a character that decomposes to one, U+113C5, which composing does not look for).
    for code in range(CODES):
        bits = records[entries[code]][1]
        assert bool(bits & NFC_MAYBE) == bool(bits & NFKC_MAYBE), f"U+{code:04X}"
        assert not bits & COMBINES or bits & NFC_MAYBE, f"U+{code:04X}"

    top, mid, leaf = levels(entries)
    return {
        "records": records,
        "top": top,
        "mid": mid,
        "leaf": leaf,
        "pool": pool,
        "pairs": sorted((first, second, made) for (first, second), made in pairs.items()),
        "entries": entries,
        "ccc": ccc,
    }


def levels(entries):
    """The three levels of a lookup of `entries`, one for each code point."""
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
    return top, [index for block in mids for index in block], [entry for block in leaves for entry in block]


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
    top, mid, leaf = levels(entries)

    def special(table):
        return sorted((code, to + [0] * (3 - len(to))) for code, to in table.items())

    return {
        "records": records,
        "top": top,
        "mid": mid,
        "leaf": leaf,
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


def check_normalization(norm):
    """Every code point against Python's own normalization, when it is of the same version: the
    combining class, the two decomposed forms of the character alone, and the pairs that
    compose, each one and that there are no others."""
    if unicodedata.unidata_version != VERSION:
        return
    records, entries, pool = norm["records"], norm["entries"], norm["pool"]
    found = {made for _, _, made in norm["pairs"]}
    wrong = 0

    def decomposed(code, compat):
        if HANGUL_FIRST <= code < HANGUL_FIRST + HANGUL_COUNT:
            index = code - HANGUL_FIRST
            return [0x1100 + index // 588, 0x1161 + index % 588 // 28] + ([0x11A7 + index % 28] if index % 28 else [])
        record = records[entries[code]]
        at, count = (record[4], record[5]) if compat else (record[2], record[3])
        return pool[at:at + count] if count else [code]

    for code in range(CODES):
        if 0xD800 <= code <= 0xDFFF:
            continue
        char = chr(code)
        if records[entries[code]][0] != unicodedata.combining(char):
            wrong += 1
            print(f"U+{code:04X}: class {records[entries[code]][0]}, Python {unicodedata.combining(char)}", file=sys.stderr)
        for form, compat in (("NFD", False), ("NFKD", True)):
            ours = "".join(chr(each) for each in decomposed(code, compat))
            if ours != unicodedata.normalize(form, char):
                wrong += 1
                print(f"U+{code:04X}: {form} {ours!r}, Python {unicodedata.normalize(form, char)!r}", file=sys.stderr)
        # Hangul's syllables compose by Hangul's rule, not by a pair.
        mapping = [] if HANGUL_FIRST <= code < HANGUL_FIRST + HANGUL_COUNT else unicodedata.decomposition(char).split()
        if len(mapping) == 2 and not mapping[0].startswith("<") and unicodedata.normalize("NFC", char) == char and code not in found:
            wrong += 1
            print(f"U+{code:04X}: Python composes it, no pair here", file=sys.stderr)
    for first, second, made in norm["pairs"]:
        if unicodedata.normalize("NFC", chr(first) + chr(second)) != chr(made):
            wrong += 1
            print(f"U+{first:04X} U+{second:04X}: composes to U+{made:04X}, not in Python", file=sys.stderr)
    if wrong:
        sys.exit(f"{wrong} differences from Python's normalization")
    print(f"checked the normalization of every code point against Python's Unicode {VERSION}", file=sys.stderr)


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
    norm = tables["norm"]
    out += [
        "",
        "/// Normalization (UAX #15). A record for each code point, found in three steps as the ones",
        "/// above: its canonical combining class, the bits of its quick checks (see src/unicode.rs),",
        "/// and where its full canonical and full compatibility decompositions are in",
        "/// `NORM_DECOMPOSED`, in the canonical order, with how many characters each is (none for a",
        "/// character that stays as it is, and for the syllables of Hangul, which are worked out).",
        f"pub static NORM_RECORDS: [(u8, u8, u16, u8, u16, u8); {len(norm['records'])}] = [",
        rows([f"({a}, {b}, {c}, {d}, {e}, {f})" for a, b, c, d, e, f in norm["records"]], 0, 6),
        "];",
        f"pub static NORM_TOP: [u8; {len(norm['top'])}] = [",
        rows(norm["top"], 3),
        "];",
        f"pub static NORM_MID: [u16; {len(norm['mid'])}] = [",
        rows(norm["mid"], 3),
        "];",
        f"pub static NORM_LEAF: [u16; {len(norm['leaf'])}] = [",
        rows(norm["leaf"], 4),
        "];",
        f"pub static NORM_DECOMPOSED: [u32; {len(norm['pool'])}] = [",
        rows([f"0x{code:04X}" for code in norm["pool"]], 0, 12),
        "];",
        "/// The pairs that compose to one character (the primary composites, the excluded ones left",
        "/// out), sorted: the first, the second, what they compose to. Hangul's are worked out.",
        f"pub static NORM_PAIRS: [(u32, u32, u32); {len(norm['pairs'])}] = [",
        rows([f"(0x{a:04X}, 0x{b:04X}, 0x{c:04X})" for a, b, c in norm["pairs"]], 0, 4),
        "];",
    ]
    with open(OUT, "w", encoding="utf-8") as file:
        file.write("\n".join(out) + "\n")
    size = len(tables["top"]) + 2 * len(tables["mid"]) + len(tables["leaf"]) + 16 * len(records)
    print(f"wrote {os.path.normpath(OUT)}: {len(records)} records, {size} bytes of lookup", file=sys.stderr)
    size = len(norm["top"]) + 2 * len(norm["mid"]) + 2 * len(norm["leaf"]) + 8 * len(norm["records"]) + 4 * len(norm["pool"]) + 12 * len(norm["pairs"])
    print(f"  and {len(norm['records'])} records of normalization, {len(norm['pairs'])} pairs, {size} bytes", file=sys.stderr)


def main():
    folder = sys.argv[1] if len(sys.argv) > 1 else os.path.join(tempfile.gettempdir(), f"ucd-{VERSION}")
    fetch(folder)
    tables = build(read(folder))
    tables["norm"] = build_normalization(*read_normalization(folder))
    check(tables)
    check_normalization(tables["norm"])
    write(tables)


if __name__ == "__main__":
    main()
