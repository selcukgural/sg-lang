---
title: Text
description: string, read string and char, with every method they have.
sidebar_position: 1
---

# Text

A `string` owns its bytes, which are always valid UTF-8, and grows at its end. A `read string` looks at text
that something else owns: a literal, a part of a `string`, what `trim` gives. Nothing is copied to make one,
and the compiler checks that the text it looks at lives as long as the view. A `char` is one Unicode
character. All three are built into the language.

Lengths and places in a text are counted in bytes, as a `nint`; going over the characters is `chars()`.

## Making text

| Signature | What it does |
|---|---|
| `"..."`, `"""..."""` | A literal; a raw literal has no escapes |
| `$"..."` | A formatted text, written where it goes without a `string` being made |
| `string toString()` on `$"..."` and on a number | The text as a new `string` |
| `string clone(read this)` | A copy that owns its bytes |
| `string string.withCapacity(nint n)` | An empty text with room for `n` bytes |
| `void write(ref this, x)` | Adds at the end whatever `print` takes |
| `void clear(ref this)` | Empties the text and keeps its memory |

```sg
void main() {
    var name = "Ada"
    var line = name.clone()
    line.write(" is ")
    line.write(36)
    line.write('!')
    print(line)

    string label = $"{name}: {line.length()} bytes".toString()
    print(label)

    var report = string.withCapacity(64)
    for i in 0..3 {
        report.write($"{i};")
    }
    print(report)
    report.clear()
    print(report.length())
    print("""C:\temp "quoted" text""")
}
```

It prints:

```text
Ada is 36!
Ada: 10 bytes
0;1;2;
0
C:\temp "quoted" text
```

A formatted text is not a `string` by itself, so that every allocation is written where it happens:

```sg error
void main() {
    var name = "Ada"
    string line = $"{name}"
    print(line)
}
```

```text
error: expected `string`; `$"..."` does not become a `string` on its own
  --> example.sg:3:19
 3 |     string line = $"{name}"
   |                   ^^^^^^^^^
  hint: add `.toString()` at the end; that is where the allocation shows
```

`text += "x"` is an error whose hint is `write`, and writing a text into itself (`s.write(s.trim())`) is an
error whose hint is `.clone()`.

## Bytes and views

| Signature | What it does |
|---|---|
| `nint length(read this)` | The length in bytes |
| `text[i]` | The byte at `i`; a panic outside the text, with the index and the length |
| `text[^n]` | The byte `n` from the end: `text[^1]` is the last |
| `text[a..b]`, `text[..b]`, `text[a..]`, `text[..]` | A `read string` of those bytes; a panic when the range cuts a character in half. An end may count from the end: `text[1..^1]` leaves out the first and the last byte |
| `read byte[] asBytes(read this)` | The bytes, to look at |
| `Result<read string, TextError> string.fromUtf8(read byte[] bytes)` | The bytes as text, or the place of the first byte that is not UTF-8 (`cause.position`) |

```sg
void main() {
    var word = "héllo"
    print(word.length())
    print(word[0])
    print(word[1..3])
    print(word[3..])
    print("Hi".asBytes())

    var bytes = List<byte> { 72, 255 }
    match string.fromUtf8(read bytes) {
        Ok(text) => print(text)
        Error(cause) => print($"not UTF-8 at byte {cause.position}")
    }
}
```

It prints:

```text
6
104
é
llo
[72, 105]
not UTF-8 at byte 1
```

`word[1..2]` would stop the program: `é` takes the bytes 1 and 2, and a `string` is always valid UTF-8.
`text[i] = 'a'` is a compile error for the same reason.

## Looking inside

| Signature | What it does |
|---|---|
| `nint? indexOf(read this, x)` | The place of the first `x`, a `char`, a `read string` or a `byte`, or `null` |
| `nint? lastIndexOf(read this, x)` | The place of the last one |
| `bool contains(read this, x)` | Whether `x` (a `char` or a `read string`) is in the text |
| `bool startsWith(read this, x)`, `bool endsWith(read this, x)` | Whether the text starts or ends with `x` |
| `bool equalsIgnoreAsciiCase(read this, read string other)` | Equal when `A`-`Z` and `a`-`z` are taken as the same |
| `bool equalsIgnoreCase(read this, read string other)` | Equal by Unicode's full case folding, with no allocation |
| `int compare(read this, read string other)`, `bool equals(read this, read string other)` | What `<` and `==` are short for: the order of the bytes |

```sg
void main() {
    var text = "hello, wörld"
    print(text.indexOf('o') ?? -1)
    print(text.lastIndexOf('o') ?? -1)
    print(text.indexOf("wö") ?? -1)
    print(text.indexOf('z') ?? -1)
    print(text.contains("llo"))
    print(text.startsWith("he") && text.endsWith('d'))
    print("Content-Length".equalsIgnoreAsciiCase("content-length"))
    print("STRASSE".equalsIgnoreCase("straße"))
    print("abc" < "abd")
    print("abc".compare("abd"))
}
```

It prints:

```text
4
4
7
-1
true
true
true
true
true
-1
```

## Parts of a text

Each of these gives views of the text, nothing copied. A `for` takes the parts one by one; kept in a `var`,
the iterator gives the next part with `next()`, and `null` at the end. While parts are in use, the text
cannot be changed or moved.

| Signature | What it gives |
|---|---|
| `read string trim(read this)`, `trimStart`, `trimEnd` | The text without the white space around it |
| `split(read this, x)` | The parts between each `x`, a `char` or a `read string`; `split("")` is an error |
| `splitWhitespace(read this)` | The words, apart at any Unicode white space |
| `lines(read this)` | The lines, without `\n` or `\r\n` |
| `chars(read this)` | Each `char` |

```sg
void main() {
    var line = "  name = Ada , 36  "
    print($"[{line.trim()}]")
    for part in line.split(',') {
        print($"<{part.trim()}>")
    }
    var fields = "k=v".split('=')
    print(fields.next() ?? "")
    print(fields.next() ?? "")
    print(fields.next() ?? "none")
    for word in " to be  or\tnot ".splitWhitespace() {
        print(word)
    }
    for l in "one\r\ntwo\n".lines() {
        print($"[{l}]")
    }
    for c in "aé😀".chars() {
        print($"{c} {(int)c} {c.utf8Length()}")
    }
}
```

It prints:

```text
[name = Ada , 36]
<name = Ada>
<36>
k
v
none
to
be
or
not
[one]
[two]
a 97 1
é 233 2
😀 128512 4
```

A method that only gives a value is an error when the value is not used, since it changes nothing:

```sg error
void main() {
    var name = "  Ada  "
    name.trim()
    print(name)
}
```

```text
error: the text that `trim` gives is not used: it leaves `name` as it was
  --> example.sg:3:5
 3 |     name.trim()
   |     ^^^^^^^^^^^
  hint: keep it in a variable: `var text = name.trim()`
```

## New text from text

| Signature | What it gives |
|---|---|
| `string replaceAll(read this, old, new)` | Every `old` replaced by `new`, each a `char` or a `read string` |
| `string repeat(read this, nint n)` | The text `n` times |
| `string toUpper(read this)`, `string toLower(read this)` | By Unicode's full mappings: `"straße".toUpper()` is `"STRASSE"`, and `Σ` at the end of a word becomes `ς` |
| `string foldCase(read this)` | The text as it is compared without case |
| `string string.join(read string separator, read List<string> parts)` | The parts with the separator between them |

No language is taken into account: `"i".toUpper()` is `"I"`.

```sg
void main() {
    print("a-b-c".replaceAll('-', ", "))
    print("ab".repeat(3))
    print("Straße".toUpper())
    print("ΟΔΟΣ".toLower())
    print("Straße".foldCase())
    var names = List<string> { "ada", "grace" }
    print(string.join(" and ", read names))
}
```

It prints:

```text
a, b, c
ababab
STRASSE
οδος
strasse
ada and grace
```

## Normal forms

What a reader takes for one text can be written in more than one way: `é` as one character, or as `e` and
U+0301 COMBINING ACUTE ACCENT. The two are not equal as they are written; in a normal form of Unicode
(UAX #15) they are one text.

| Signature | What it gives |
|---|---|
| `string normalize(read this, NormalizationForm form)` | A new text in that form |
| `bool isNormalized(read this, NormalizationForm form)` | Whether the text is in that form already |

`NormalizationForm` has four variants. `C` (NFC) composes a character and its marks into one character where
Unicode has one: what the web and most files write. `D` (NFD) decomposes each character into its base and its
marks, the marks in a fixed order. `KC` and `KD` also make one of what differs only in how it is shown (`ﬁ`
and `fi`, `²` and `2`, a full-width `Ａ` and `A`): they are for comparing and searching, not for keeping a
text, since what they drop is gone. A text already in the form is copied as it is, after a look at each
character, and ASCII is in every form.

```sg
void main() {
    var composed = "é"
    var decomposed = "e\u{301}"
    print(composed == decomposed)
    print(decomposed.normalize(NormalizationForm.C) == composed)
    print(composed.normalize(NormalizationForm.D).length())
    print("ﬁ²".normalize(NormalizationForm.KC))
    print(decomposed.isNormalized(NormalizationForm.C))
}
```

It prints:

```text
false
true
3
fi2
false
```

Each line of 100 MB of text in many writings goes into NFC, into NFD or through `isNormalized` as fast as
Rust's ICU4X or faster, and `c.category()` of every character takes 0.46 seconds on an Apple M3 Max, against
0.49 for ICU4X ([the tables](../benchmarks/library.md#unicode)).

## Writing values into text

Inside `$"..."` a width and a letter may follow a value, as in C#: `{n,5}` takes at least five characters with
spaces in front, `{n,-5}` with spaces after; `{n:D4}` writes at least four decimal digits, `{n:x}` and
`{n:X4}` hex, `{n:b8}` binary, `{price:F2}` two digits after the point; `{price,8:F3}` both. `{{` and `}}` write
a brace. A width counts characters, not bytes.

```sg
void main() {
    var n = 42
    var price = 3.14159
    print($"[{n,5}] [{n,-5}] [{n:D4}] [{n:x}] [{n:X4}] [{n:b8}]")
    print($"[{price:F2}] [{price,8:F3}] {{n}} [{"é",3}]")
}
```

It prints:

```text
[   42] [42   ] [0042] [2a] [002A] [00101010]
[3.14] [   3.142] {n} [  é]
```

## `char`

A `char` is written `'a'`, `'é'`, `'\n'` or `'\u{e9}'`. It is not a number: `(int)c` gives its code and
`(char)n` the character of a code, which panics when no character has it; `n.tryToChar()` gives `null` there.
`==`, `<`, `match`, `print` and keys of a `HashMap` work on it. A character of one byte, like `'+'`, is also a
`byte` where an integer is expected (`byte b = '+'`, `text[i] == ':'`).

| Signature | What it does |
|---|---|
| `bool isAsciiDigit(read this)`, `isAsciiLetter`, `isAsciiAlphanumeric`, `isAsciiUpper`, `isAsciiLower`, `isAsciiWhitespace`, `isAscii` | Tests on ASCII, on `char` and on `byte` |
| `toAsciiUpper(read this)`, `toAsciiLower(read this)` | `a`-`z` to `A`-`Z` and back; on `byte` too |
| `bool isLetter(read this)`, `isNumeric`, `isAlphanumeric`, `isUpper`, `isLower`, `isControl`, `isWhitespace` | Unicode 17.0's categories: L, N, L or N, Lu, Ll, Cc, and white space |
| `char toUpper(read this)`, `char toLower(read this)` | One character for one: `'ß'.toUpper()` is `'ß'` |
| `nint utf8Length(read this)` | How many bytes the character takes in UTF-8 |
| `UnicodeCategory category(read this)` | Its general category in Unicode 17.0: `UppercaseLetter`, `DecimalNumber`, `CurrencySymbol`, ... `Unassigned`; `abbreviation()` gives the short name, `Lu` |

```sg
void main() {
    var c = 'é'
    print($"{c} {(int)c} {c.utf8Length()}")
    print((char)65)
    print(0xD800.tryToChar() ?? '?')
    print('7'.isAsciiDigit())
    print('٣'.isNumeric())
    print('ß'.toUpper())
    print('Q'.isUpper() && 'q'.isLetter())
    print($"{'€'.category()} {'€'.category().abbreviation()}")
    byte b = 'x'
    print(b.toAsciiUpper())
}
```

It prints:

```text
é 233 2
A
?
true
true
ß
true
CurrencySymbol Sc
88
```

`toAsciiUpper` on a `byte` gives a `byte`, which prints as a number. Arithmetic on a `char` is refused:

```sg error
void main() {
    var c = 'a'
    print(c + 1)
}
```

```text
error: operator `+` is not defined for `char` and `int`
  a `char` is not a number: `(int)c` is its code, and `(char)n` the character of a code
```

Reading numbers out of text is on the page [Numbers](./numbers.md).
