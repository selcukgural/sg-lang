#!/usr/bin/env python3
r"""Random patterns written three ways that mean the same, for SG (Go's syntax), Rust's `regex` and
PCRE2, and texts to search (choice 1841): `gen.py SEED COUNT PREFIX [--long N] [--bytes]` writes
PREFIX-sg.txt, PREFIX-rust.txt and PREFIX-pcre.txt, a case a line: the hex of a pattern, a space,
the hex of a text. A text is up to 24 characters, or N with `--long`; with `--bytes`, about one
piece in seven is a sequence that is not UTF-8, of each kind. Only what the three mean alike is
written:
- `\d \D \w \W` are ASCII in SG and fold under (?i) (`k` matches the Kelvin sign); Rust's are
  Unicode and PCRE2's do not fold, so both get the class written out, `[0-9A-Za-z_]`.
- `\s \S` are `[\t\n\f\r ]` (PCRE2's has \v too); `\b \B` are ASCII (Rust: `(?-u:\b)`).
- Rust has no `\Q...\E` and no octal: the text is escaped, `\101` is `\x41`.
- PCRE2 runs with `$` only at the end and `^` after a last line break, as SG's.
Left out: (?x), which SG alone has; `{,n}`, which SG reads as text, as Go does; `**` and its kin
(PCRE2's possessive); repeated assertions and flags; names given twice; counts past 1000, which SG
refuses (choice 1462)."""
import os
import random
import sys

ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
BYTES = "--bytes" in sys.argv
TEXTMAX = int(sys.argv[sys.argv.index("--long") + 1]) if "--long" in sys.argv else 24
if "--long" in sys.argv:
    ARGS.remove(str(TEXTMAX))
ALPHA = list("abcABCkKsSxyz019_ -.\n\t") + ["\u212a", "\u017f", "ß", "ẞ", "i", "I", "İ", "ı", "é", "É",
                                            "日", "本", "😀", "\r", "\u00a0", "Σ", "σ", "ς", "\x00"]
LIT = list("abcABCkKsSxyz019_ ") + ["\u212a", "\u017f", "ß", "ẞ", "İ", "ı", "é", "É", "日", "😀", "Σ", "ς"]

D, W = "0-9", "0-9A-Za-z_"
SP = "\\t\\n\\f\\r "
# (sg, rust, pcre)
ESCAPES = [
    (r"\d", f"[{D}]", f"[{D}]"), (r"\D", f"[^{D}]", f"[^{D}]"), (r"\w", f"[{W}]", f"[{W}]"),
    (r"\W", f"[^{W}]", f"[^{W}]"), (r"\s", f"[{SP}]", f"[{SP}]"), (r"\S", f"[^{SP}]", f"[^{SP}]"),
    (r"\pL",) * 3, (r"\p{Lu}",) * 3, (r"\p{Ll}",) * 3, (r"\PL",) * 3, (r"\p{Nd}",) * 3,
    (r"\p{^Lu}", r"\P{Lu}", r"\p{^Lu}"), (r"\pN",) * 3, (r"\p{Zs}",) * 3, (r"\x41",) * 3,
    (r"\x{212A}",) * 3, (r"\x{1F600}",) * 3, (r"\101", r"\x41", r"\101"), (r"\n",) * 3, (r"\t",) * 3,
    (r"\.",) * 3, (r"\-",) * 3, (r"\$",) * 3, (r"\^",) * 3, (r"\|",) * 3, (r"\*",) * 3,
    (r"\a", r"\x07", r"\a"), (r"\f",) * 3, (r"\v", r"\x0B", r"\x0b"), (r"\r",) * 3, (r"\x{0}",) * 3,
]
ASSERTS = [("^",) * 3, ("$",) * 3, (r"\A",) * 3, (r"\z",) * 3, (r"\b", r"(?-u:\b)", r"\b"),
           (r"\B", r"(?-u:\B)", r"\B")]
POSIX = ["alpha", "digit", "space", "upper", "lower", "word", "alnum", "punct", "xdigit", "^alpha", "^space",
         "ascii", "blank", "cntrl", "graph", "print"]
FLAGS = ["i", "m", "s", "U"]
META = set(".*+?()[]{}|^$\\")


def same(s):
    return (s, s, s)


def class_item(r):
    k = r.random()
    if k < 0.5:
        return same(r.choice(LIT))
    if k < 0.6:
        return (r"\d", D, D)
    if k < 0.67:
        return (r"\w", W, W)
    if k < 0.72:
        return (r"\D", f"[^{D}]", r"\D")
    if k < 0.77:
        return (r"\s", SP, SP)
    if k < 0.84:
        return same(f"[:{r.choice(POSIX)}:]")
    if k < 0.9:
        return same(r.choice(["\\]", "\\[", "\\-", "\\^", "\\\\"]))
    a, b = sorted(r.sample("abckxyzABCKXYZ019", 2))
    return same(f"{a}-{b}")


def cls(r):
    items = [class_item(r) for _ in range(r.randint(1, 4))]
    neg = "^" if r.random() < 0.3 else ""
    return tuple(f"[{neg}{''.join(i[n] for i in items)}]" for n in range(3))


def join(parts, sep=""):
    return tuple(sep.join(p[n] for p in parts) for n in range(3))


def wrap(pre, inner, post=")"):
    return tuple(pre + inner[n] + post for n in range(3))


names = [0]


def atom(r, depth):
    """(pattern three ways, whether it may be repeated)"""
    k = r.random()
    if depth < 4 and k < 0.22:
        inner = alt(r, depth + 1)
        g = r.random()
        names[0] += 1
        if g < 0.5:
            return wrap("(", inner), True
        if g < 0.75:
            return wrap("(?:", inner), True
        if g < 0.85:
            return wrap(f"(?<n{names[0]}>", inner), True
        if g < 0.9:
            return wrap(f"(?P<p{names[0]}>", inner), True
        on = "".join(r.sample(FLAGS, r.randint(1, 2)))
        off = "-" + r.choice(FLAGS) if r.random() < 0.3 else ""
        if off and off[1] in on:
            off = ""
        return wrap(f"(?{on}{off}:", inner), True
    if k < 0.5:
        return same(r.choice(LIT)), True
    if k < 0.62:
        return r.choice(ESCAPES), True
    if k < 0.72:
        return cls(r), True
    if k < 0.8:
        return same("."), True
    if k < 0.9:
        return r.choice(ASSERTS), False
    if k < 0.95:
        text = "".join(r.choice("a.*+?()[]{}|^$") for _ in range(r.randint(1, 3)))
        rust = "".join("\\" + ch if ch in META else ch for ch in text)
        return (r"\Q" + text + r"\E", rust, r"\Q" + text + r"\E"), True
    on = "".join(r.sample(FLAGS, r.randint(1, 2)))
    return same(f"(?{on})" if r.random() < 0.8 else f"(?-{r.choice(FLAGS)})"), False


def repeat(r, a, can):
    if not can or r.random() < 0.6:
        return a
    op = r.choice(["*", "+", "?", "{2}", "{0}", "{1,}", "{0,2}", "{1,3}", "{2,1}", "{3,}", "{1000}"])
    if r.random() < 0.3:
        op += "?"
    return tuple(x + op for x in a)


def seq(r, depth):
    return join([repeat(r, *atom(r, depth)) for _ in range(r.randint(0, 4))])


def alt(r, depth):
    n = 1 if r.random() < 0.7 else r.randint(2, 3)
    return join([seq(r, depth) for _ in range(n)], "|")


BAD = [b"\xff", b"\x80", b"\xbf", b"\xc0\x80", b"\xc1\xbf", b"\xed\xa0\x80", b"\xf4\x90\x80\x80", b"\xf5\x80",
       b"\xe6\x97", b"\xc3", b"\xf0\x9f\x98", b"\xe0\x80\x80", b"\xf0\x80\x80\x80"]


def text(r):
    out = bytearray()
    for _ in range(r.randint(0, TEXTMAX)):
        if BYTES and r.random() < 0.15:
            out += r.choice(BAD)
        else:
            out += r.choice(ALPHA).encode()
    return bytes(out)


def main():
    seed, count, prefix = int(ARGS[0]), int(ARGS[1]), ARGS[2]
    r = random.Random(seed)
    files = [open(f"{prefix}-{k}.txt", "w") for k in ("sg", "rust", "pcre")]
    made = 0
    while made < count:
        names[0] = 0
        p = alt(r, 0)
        for _ in range(r.randint(1, 5)):
            t = text(r).hex()
            for f, x in zip(files, p):
                f.write(x.encode().hex() + " " + t + "\n")
            made += 1


main()
