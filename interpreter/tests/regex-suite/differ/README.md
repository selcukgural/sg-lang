# The differential test of `Regex`

`Regex` held to Rust's `regex` and to PCRE2 on random patterns and texts (choice 1841), run by hand
when `Regex` changes, as the suites of `trials/jq/tests` are:

```sh
python3 interpreter/tests/regex-suite/differ/run.py          # about half an hour
python3 interpreter/tests/regex-suite/differ/run.py --quick  # about a minute
```

It needs Rust, Zig and PCRE2 (`brew install pcre2`, or `-Dpcre2=/prefix` in `zig/build.zig`), and
builds into `interpreter/target/regex-differ`.

| File | What it is |
|---|---|
| `gen.py` | Random patterns, each written three ways that mean the same, and texts to search: short, long (`--long 2000`), or bytes with sequences that are not UTF-8 of each kind (`--bytes`) |
| `sg/differ.sg` | `isMatch`, `findAll` and `captures` of each case with SG's `Regex`; with `bytes`, `isMatchBytes`, `findAllBytes` and `capturesBytes` |
| `rust/` | The same with the `regex` crate, pinned in `Cargo.lock` (1.13.2 on 24 November 2026); with `bytes`, `regex::bytes` |
| `zig/` | The same with PCRE2, Zig having no regular expressions; with `bytes`, `MATCH_INVALID_UTF` |
| `compare.py` | The three outputs side by side, case by case |
| `run.py` | All of it: four seeds of 500,000 short cases, 30,000 long ones, two seeds of 500,000 in bytes |

A case fails when Rust and PCRE2 agree and SG does not, or when both compile a pattern SG refuses.
Every other difference is a known way of one engine, counted and not failed:

- **Rust's `regex`** leaves out of `captures` the groups inside a repetition of `{0}`, refuses a
  compiled program past 10 MB, and, in bytes, gives an empty match inside a character of UTF-8 and
  takes `(?-u:\b)` between the bytes of one.
- **PCRE2** backtracks: past its limit of matching a search ends with an error (`L -47`), and some
  patterns repeating empty groups take another path; it does not fold `\p{Lu}` under `(?i)`, and in
  bytes it starts no search inside a sequence that is not UTF-8, not even `^` at the start.
- **SG** compiles some large patterns both refuse, its limit being 100,000 steps (choice 1462).

What the generator writes is held to what the three mean alike: `\d \w \s \b` are written out as
ASCII classes for Rust and PCRE2, `\Q...\E` is escaped for Rust, and PCRE2 runs with `$` only at the
end and `^` after a last line break, as SG's. Left out: `(?x)`, which SG alone has; `{,n}`, which
SG reads as text, as Go does; counts past 1000, which SG refuses (1462); names given twice, which
SG refuses and Go takes.

On 24 November 2026, `run.py` took 25 minutes on an Apple M3 Max for 2,030,000 cases of text and
1,000,000 of bytes (SG 7.5 s for each 500,000, PCRE2 20 to 46 s, Rust about 190 s, most of it compiling
the large patterns): SG agreed wherever the two agreed, and refused no pattern both compiled. Go's `regexp`, whose syntax `Regex` follows, was held
to the same before: no difference in what was matched, three in what is a pattern (a group of flags
alone before a repetition, `a(?m)+`, which Go reads as `a+`; nested counts past 1000 in all, which
Go refuses; and names given twice, which Go takes).
