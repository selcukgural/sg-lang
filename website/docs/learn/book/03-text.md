---
title: Text
description: Owned text and text that is only looked at, literals, formatting, the common methods, building text, UTF-8 and parsing numbers.
sidebar_position: 3
---

# Text

Text in SG is UTF-8, and comes in two forms. A `string` owns its bytes: it can grow, and it frees them when it
ends. A `read string` looks at text that something else owns, such as a part of a `string`, and copies
nothing. Most methods that give back a part of a text give a `read string`, so trimming or splitting a line
costs no allocation; the compiler checks that the look never outlives what it looks at.

## Literals and escapes

A text is written in double quotes. Inside it, `\n` (new line), `\t` (tab), `\r`, `\0`, `\\`, `\"` and `\'`
stand for one character each, and `\u{...}` for the character of a code in hex:

```sg
void main() {
    print("a tab:\there, a quote: \" and a backslash: \\")
    print("e acute \u{e9}, a smile \u{1F600}")
    print("two\nlines")
}
```

It prints:

```text
a tab:	here, a quote: " and a backslash: \
e acute é, a smile 😀
two
lines
```

## Raw text

Text between `"""` and `"""` is raw: a backslash is a backslash and a quote is a quote, which suits paths and
patterns. Written over several lines, the text starts on the line after the opening quotes and ends with the line
before the closing ones, and the indentation of the closing quotes is taken off every line, so the text can be
indented with the code around it:

```sg
void main() {
    var path = """C:\Users\pasha\"notes".txt"""
    print(path)
    var poem = """
        Roses are red,
          violets are blue.
        """
    print(poem)
}
```

It prints:

```text
C:\Users\pasha\"notes".txt
Roses are red,
  violets are blue.
```

## Formatted text

`$"..."` puts values into a text, each in braces. A value can be any expression, and a format may follow a
colon: `x` or `X` for hex, `b` for binary and `D` for decimal, each with the least number of digits, and `F` for
a fraction with so many digits after the point. `e` and `g` write a fraction as C's `%e` and `%g` do: `e` with
an exponent and so many digits after the point, `g` with so many significant digits in the shorter of the two
ways, zeros at the end left out; without a number, 6. `E` and `G` write a capital `E`. A width after a comma
pads with spaces, in front, or after it when it is negative. `{{` and `}}` write a brace.

```sg
void main() {
    int n = 255
    int day = 7
    double price = 3.14159
    var name = "Pasha"
    print($"{n:x8} {n:X2} {n:b8}")
    print($"{day:D3} {price:F2} {n + day}")
    print($"{price:e2} {price:g3} {1e21:g} {0.0001:G} {1234567.0:G}")
    print($"[{name,8}] [{name,-8}] [{price,8:F2}]")
    print($"{{braces}} {name.length()}")
    print($"""a "quoted" {name}""")
}
```

It prints:

```text
000000ff FF 11111111
007 3.14 262
3.14e+00 3.14 1e+21 0.0001 1.23457E+06
[   Pasha] [Pasha   ] [    3.14]
{braces} 5
a "quoted" Pasha
```

A `$"..."` is not a `string` yet: `print` and `write` take it as it is and write it without making one. Where
a `string` is needed, `.toString()` makes it, and that is where the allocation shows. Numbers, `bool` and
`char` have `.toString()` too.

```sg error
void main() {
    int count = 3
    string label = $"{count} items"
    print(label)
}
```

```text
error: expected `string`; `$"..."` does not become a `string` on its own
  --> example.sg:3:20
 3 |     string label = $"{count} items"
   |                    ^^^^^^^^^^^^^^^^
  hint: add `.toString()` at the end; that is where the allocation shows
```

With `.toString()` it runs:

```sg
void main() {
    int count = 3
    string label = $"{count} items".toString()
    string number = count.toString()
    print(label)
    print(number)
}
```

It prints:

```text
3 items
3
```

## Looking at text

`length()` is the number of bytes. `contains`, `startsWith` and `endsWith` take a text or a `char`;
`indexOf` and `lastIndexOf` give the place in bytes, or `null` when it is not there. `trim()`, `trimStart()`
and `trimEnd()` give a `read string`, and so does a range: `text[a..b]`, `text[..b]`, `text[a..]`.

```sg
void main() {
    var line = "  Hello, world  "
    var text = line.trim()
    print($"[{text}] {text.length()}")
    print(text.contains("world"))
    print(text.startsWith("Hell"))
    print(text.endsWith('d'))
    print(text.indexOf('o') ?? -1)
    print(text.lastIndexOf("o") ?? -1)
    print(text.indexOf("xyz") ?? -1)
    print(text[0..5])
    print(text[7..])
    print("abc" < "abd")
}
```

It prints:

```text
[Hello, world] 12
true
true
true
4
8
-1
Hello
world
true
```

## New text from old

`replaceAll`, `repeat`, `toUpper` and `toLower` make a new `string`. A call of one whose result is not used
is an error, since it leaves the text as it was:

```sg
void main() {
    var name = "Hello, world"
    print(name.replaceAll("world", "SG"))
    print("ab".repeat(3))
    print(name.toUpper())
    print(name.toLower())
    print("straße".toUpper())
}
```

It prints:

```text
Hello, SG
ababab
HELLO, WORLD
hello, world
STRASSE
```

`toUpper` and `toLower` follow Unicode's full mappings, so `ß` becomes `SS`; no language is taken into account.

## Going over text

`split` (at a `char` or a text), `splitWhitespace()` and `lines()` give each part as a `read string`, with
nothing copied; `lines()` takes off the `\n` or `\r\n`. `chars()` gives each `char`. Kept in a variable, they
give one part at a time with `next()`, which is `null` at the end.

```sg
void main() {
    for part in "ada,36,London".split(',') {
        print(part)
    }
    for line in "first\nsecond\r\nthird".lines() {
        print($"[{line}]")
    }
    long words = 0
    for _ in "one two\tthree\nfour".splitWhitespace() {
        words += 1
    }
    print(words)
    var parts = "key=value".split('=')
    var key = parts.next() ?? ""
    var value = parts.next() ?? ""
    print($"{key} -> {value}")
}
```

It prints:

```text
ada
36
London
[first]
[second]
[third]
4
key -> value
```

## Bytes and characters

Since text is UTF-8, a character outside ASCII takes more than one byte. `length()` counts bytes, `text[i]`
reads one byte, and `chars()` goes by characters:

```sg
void main() {
    var word = "naïve"
    print($"{word.length()} bytes")
    int letters = 0
    for c in word.chars() {
        if c.isLetter() {
            letters += 1
        }
    }
    print($"{letters} letters")
    print('ï'.utf8Length())
    print(word[0])
    print(word[0] == 'n')
}
```

It prints:

```text
6 bytes
5 letters
2
110
true
```

A place given by `indexOf` is a byte place, so it can be used in a range at once. A range that would cut a
character in two panics.

## Building text

A `string` grows at its end with `write`, which takes whatever `print` takes: a text, a number, a `char`, a
`bool`, a `$"..."`. `clear()` empties it and keeps its memory, and `string.withCapacity(n)` starts with room
for `n` bytes, so a loop that builds a text allocates once. `+` and `+=` do not join texts: their hints point
to `$"..."` and `write`.

```sg
void main() {
    var line = string.withCapacity(64)
    for i in 1..4 {
        line.write(i)
        line.write(',')
    }
    line.write($" and {2.5:F1}")
    line.write(" done")
    print(line)
    line.clear()
    line.write(true)
    print(line)
}
```

It prints:

```text
1,2,3, and 2.5 done
true
```

## Owned or borrowed

A function that only reads a text takes it as `read string`; the caller lends a `string` with `read name`,
and a literal goes as it is. A `read string` that should be kept is made a `string` with `.clone()`:

```sg
nint vowels(read string text) {
    nint count = 0
    for c in text.chars() {
        if "aeiou".contains(c) {
            count += 1
        }
    }
    return count
}

void main() {
    var name = "Ada Lovelace".clone()
    print(vowels(read name))
    print(vowels("Grace"))
    string first = name[0..3].clone()
    name.clear()
    print(first)
}
```

It prints:

```text
5
2
Ada
```

`name[0..3]` only looks at `name`. Kept without `.clone()`, the look would still be in use when `name` is
cleared, and the compiler refuses that:

```sg error
void main() {
    var name = "Ada Lovelace".clone()
    var first = name[0..3]
    name.clear()
    print(first)
}
```

```text
error: `name` cannot be changed here: `first` borrows from it and is used again later
  --> example.sg:4:5
 4 |     name.clear()
   |     ^^^^
  `first` borrowed it here:
  --> example.sg:3:17
 3 |     var first = name[0..3]
   |                 ^^^^^^^^^^
  hint: finish with `first` before this line: a borrow ends at its last use
```

Printing `first` before `name.clear()` would be accepted too. [Functions](./04-functions.md) has more on
`read`, and [Ownership](./07-ownership.md) has the rules in full.

## Numbers from text

`int.parse(text)` reads a number, and the same exists on every number type. It gives a
`Result<int, NumberError>`: `Ok` with the number, or an `Error` that is `Invalid(position)`, the byte where
the text stops being a number, or `OutOfRange`, when the type does not hold it. `??` gives a value for the
error. `parse` reads a sign and digits, and for `double` and `float` a point and an exponent too; nothing else,
so no spaces and no `_`. `int.parseHex` reads hex digits without `0x`.

```sg
void main() {
    print(int.parse("123") ?? 0)
    match int.parse("12x") {
        Ok(value) => print(value)
        Error(why) => print(why)
    }
    match byte.parse("300") {
        Ok(value) => print(value)
        Error(why) => print(why)
    }
    print(double.parse("2.5e3") ?? 0.0)
    print(int.parseHex("ff") ?? 0)
    print(int.parse(" 1") is Error(_))
}
```

It prints:

```text
123
Invalid(2)
OutOfRange
2500
255
true
```

What `print` writes for a number, `parse` reads back as the same number.
