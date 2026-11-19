---
title: Regular expressions
description: Regex, Match and Captures, patterns matched in a time that grows with the text and no more, and a pattern written in the program checked when it is compiled.
sidebar_position: 15
---

# Regular expressions

`Regex` finds patterns in text. It means what RE2, Go's `regexp` and Rust's `regex` mean: the time a match
takes grows with the length of the text and no more, so no pattern, and no text, can make a search take for
ever. That leaves out back references (`\1`) and look-around (`(?=...)`), which would need it. It is written
in SG in the module `std`.

```sg
static Regex Date = Regex.of("""(?<year>\d{4})-(?<month>\d\d)-(?<day>\d\d)""")

void main() {
    var text = "from 2026-11-20 to 2027-01-05"
    for m in Date.findAll(read text) {
        print(m.of(read text))
    }
    print(Date.replaceAll(read text, "$day.$month.$year"))
}
```

It prints:

```text
2026-11-20
2027-01-05
from 20.11.2026 to 05.01.2027
```

## A pattern of the program, and one made when it runs

`Regex.of` takes a pattern written in the program, and the compiler reads it: a pattern that is not a regular
expression is a compile error, so `Regex.of` gives a `Regex` and never fails when the program runs. Raw text,
`"""..."""`, keeps the backslashes from doubling. Kept in a `static`, the pattern is compiled the first time
the static is read, once, and any task may use it.

```sg error
static Regex Numbers = Regex.of("""(\d+""")

void main() {
    print(Numbers.isMatch("12"))
}
```

```text
error: this pattern is not a regular expression: column 1: missing closing )
```

A pattern made when the program runs, from its input or its command line, goes to `Regex.compile`, which
gives a `Result<Regex, RegexError>`. The error says where the pattern stops being one.

```sg
void main() {
    var patterns = List<string> { """\w+@\w+\.com""", "a{2,1}", "(?=x)" }
    for pattern in patterns {
        match Regex.compile(read pattern) {
            Ok(re) => print($"{re}: {re.isMatch("mail ada@example.com")}")
            Error(e) => print($"{pattern}: {e}")
        }
    }
}
```

It prints:

```text
\w+@\w+\.com: true
a{2,1}: column 2: invalid repeat count: at most 1000, and the second not below the first
(?=x): column 1: look-around is not supported: a match never looks ahead or behind
```

| Signature | What it does |
|---|---|
| `Regex Regex.of(read string pattern)` | A pattern written in the program, checked when the program is compiled |
| `Result<Regex, RegexError> Regex.compile(read string pattern)` | A pattern made when the program runs |
| `bool isMatch(read this, read string text)` | Whether the pattern matches anywhere in `text` |
| `Match? find(read this, read string text)` | The first match; `null` when there is none |
| `List<Match> findAll(read this, read string text)` | Every match, one after another, none overlapping |
| `Captures? captures(read this, read string text)` | Where each group of the first match matched |
| `string replaceAll(read this, read string text, read string replacement)` | `text` with each match replaced |
| `List<string> split(read this, read string text)` | The parts of `text` between the matches |
| `nint groupCount(read this)` | How many groups the pattern has |
| `read string pattern(read this)` | The pattern as it was written; `$"{re}"` writes it too |

## Matches and groups

A `Match` is where a match is: `start` and `end`, in bytes of the text. `m.of(text)` gives the text that
matched, and `m.length()` its bytes. `captures` gives a `Captures`: `get(0)` is the whole match, `get(1)` the
first group, `named("year")` the group of that name, each a `Match?` that is `null` for a group that took no
part in the match.

```sg
void main() {
    var pair = Regex.of("""(?<key>\w+)\s*=\s*(?<value>\w+)?""")
    var line = "retries = 3"
    var caps = pair.captures(read line) ?? panic("no match")
    print(caps.get(0)?.length() ?? 0)
    print(caps.named("key")?.of(read line) ?? "-")
    print(caps.named("value")?.of(read line) ?? "-")
    var empty = pair.captures("retries =") ?? panic("no match")
    print(empty.named("value") is null)
}
```

It prints:

```text
11
retries
3
true
```

Of the matches that start at the leftmost place, the one the pattern prefers is taken, as in Perl, Go and
Rust: `a|ab` takes `a`, `a+` as many as there are, `a+?` one.

## Replacing and splitting

In the replacement, `$1` or `${1}` is what group 1 matched, `$name` or `${name}` the group of that name, and
`$$` a `$`. A name after `$` is digits or a name, not both: `$1x` is group 1 and an `x`. A group the pattern
does not have is a panic that names it; a group that took no part writes nothing.

```sg
void main() {
    var swap = Regex.of("""(\w+)=(\w+)""")
    print(swap.replaceAll("a=1, b=2", "$2=$1"))
    print(swap.replaceAll("a=1", "${2}x$1"))
    print(Regex.of("""\d+""").replaceAll("costs 5", "$$$0"))
    print(Regex.of("""\s*,\s*""").split("a , b,c ,d"))
}
```

It prints:

```text
1=a, 2=b
1xa
costs $5
["a", "b", "c", "d"]
```

## The syntax

The syntax is Go's.

| Pattern | Matches |
|---|---|
| `.` | Any character but a line break; with `(?s)`, any |
| `[abc]`, `[a-z]`, `[^a-z]` | One character of the class, or not of it |
| `\d`, `\w`, `\s` | A digit, a letter, digit or `_`, a space: ASCII only, as in Go |
| `\D`, `\W`, `\S` | Any character but those |
| `\pL`, `\p{Lu}`, `\p{Nd}`, `\p{Sc}` | A character of a general category of Unicode, by its short name |
| `\PL`, `\p{^L}` | A character of no such category |
| `[[:alpha:]]`, `[[:^digit:]]` | The classes of POSIX, in ASCII |
| `x*`, `x+`, `x?` | None or more, one or more, none or one, as many as can be |
| `x{2}`, `x{2,}`, `x{2,5}` | That many; a count is at most 1000 |
| `x*?`, `x+?`, `x??`, `x{2,5}?` | The same, as few as can be |
| `xy`, `x\|y` | `x` then `y`; `x`, or else `y` |
| `(x)`, `(?<name>x)`, `(?:x)` | A group, a group with a name, a group that keeps nothing |
| `^`, `$` | The start and the end of the text; of a line with `(?m)` |
| `\A`, `\z` | The start and the end of the text, always |
| `\b`, `\B` | Between a letter of `\w` and another character, or not |
| `\x41`, `\x{1F600}`, `\101`, `\n`, `\t`, `\.` | One character, by its code or escaped |
| `\Q...\E` | The text between, as it is |
| `(?i)`, `(?m)`, `(?s)`, `(?U)` | Ignore case; `^` and `$` at lines; `.` takes a line break; swap greedy and lazy |
| `(?i:x)`, `(?-i)` | A flag only around `x`; a flag taken off |

`(?i)` compares the characters as Unicode's simple case folding does: `k`, `K` and the Kelvin sign `K` are one,
`ß` and `ẞ` are one, and `ı` and `İ` are not `i`, since no language is taken into account.

```sg
void main() {
    var word = Regex.of("""(?i)\bstra(ss|ß)e\b""")
    print(word.findAll("Straße, STRASSE, strasse, STRAẞE"))
    print(Regex.of("""\p{Lu}\p{Ll}+""").findAll("Çağ ŞEHİR Işık"))
}
```

It prints:

```text
[0..7, 9..16, 18..25, 27..35]
[0..5, 14..20]
```

## What it leaves out

A `string` is matched, not bytes; `\p` names the general categories and not the scripts (`\p{Greek}`); there
is no leftmost-longest match of POSIX, and no replacement made by a function.
