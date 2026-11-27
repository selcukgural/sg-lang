---
name: a..b
kind: sign
summary: A range: the numbers from a up to b, without b; in a pattern, a..=b takes b too.
see: for, T[], xs[^1], match, or, string
learn: ../docs/learn/book/08-collections.md#slices
library: ../docs/library/collections.md#slices
---

## Forms

| Form | What it is |
|---|---|
| `for i in a..b` | the numbers from `a` up to `b`, without `b` |
| `for i in (a..b).reversed()` | the same numbers, the last first |
| `xs[a..b]`, `text[a..b]` | the items or the bytes from `a` up to `b`, without `b`, as a slice or a `read string` |
| `xs[..b]`, `xs[a..]`, `xs[..]` | from the start, to the end, the whole |
| `xs[1..^1]`, `xs[^2..]` | an end counted from the end: see [`xs[^1]`](index-from-end.md) |
| `ref xs[a..b]` | a part of a list, an array or a slice, to write |
| `'a'..='z'`, `0..=9`, `-5..=-1` | in a pattern: from the first value to the last, both taken |

## What it means

`a..b` is the numbers from `a` up to `b`. The end is not one of them: `0..3` is 0, 1 and 2, so `0..n` has `n`
numbers and `0..xs.length()` is every position of `xs`. A range is not a value of its own. It is written in three
places: at the head of a `for`, between the brackets of an index, and in a pattern.

In a `for`, the ends are integers and are found once, the left first, before the first round. The count has the
type the two ends share: `0..5` counts in `int`, `0..xs.length()` in `nint`, a `byte` start and a constant end
in `byte`. Two ends whose types do not mix (`int` and `nuint`) are an error. A range whose end is not above its
start gives no round. `(a..b).reversed()` goes over the same numbers from `b - 1` down to `a`.

Between the brackets of an index, a range takes a part of a list, an array, a slice or a text, and is a borrow:
a `read T[]`, or a `read string` of a text's bytes. `ref xs[a..b]` lends the part to write. Either end may be
left out, and either may count from the end with `^`. The ends take any integer that widens to `nint`. They are
checked against the length when the program runs, in every build: an end outside, an end before the start, or a
cut through a character of a text stops the program. A range that is empty, as `xs[2..2]` or `xs[3..]` on three
items, is not a mistake.

In a pattern a range is written `a..=b` and takes `b` too, so that `'a'..='z'` has the `z`. Its ends are numbers
or characters as written, the first not greater than the last. `..=` is written only in a pattern, and `..` is
an error there. A `for` that needs its end too goes to `b + 1`.

## Examples

### Counting

```sg
void main() {
    for i in 1..4 {
        print(i)
    }
    var total = 0
    for _ in 0..5 {
        total += 2
    }
    print(total)
    for i in 3..0 {
        print(i)
    }
}
```

```text
1
2
3
10
```

`1..4` is 1, 2 and 3. `0..5` is five rounds; `_` is the name when the body does not need the count. `3..0` ends
before it starts, so its loop runs no round.

### A part of a list, an array or a text

```sg
void main() {
    var xs = List<int> { 10, 20, 30, 40, 50 }
    print(xs[1..3])
    print(xs[..2])
    print(xs[3..])
    print(xs[..])
    var primes = int[4] { 2, 3, 5, 7 }
    print(primes[1..3])
    var line = "key=value"
    var at = line.indexOf('=') ?? 0
    print(line[..at])
    print(line[at + 1..])
}
```

```text
[20, 30]
[10, 20]
[40, 50]
[10, 20, 30, 40, 50]
[3, 5]
key
value
```

`xs[1..3]` is the items at 1 and 2. A range of a text counts bytes, as `indexOf` does, and is a `read string`
that looks at the text: nothing is copied.

### Empty ranges

```sg
void main() {
    var xs = List<int> { 10, 20, 30 }
    for n in 0..xs.length() + 1 {
        print($"{n}: {xs[n..]} {xs[..n]}")
    }
    var text = "abc"
    print(text[1..1].length())
}
```

```text
0: [10, 20, 30] []
1: [20, 30] [10]
2: [30] [10, 20]
3: [] [10, 20, 30]
0
```

`xs[3..]` on three items is empty, and so is `xs[..0]`: an end may be the length itself, one past the last item.
`n` is a `nint`, since `xs.length()` is one.

### The type of the count

```sg
void main() {
    byte first = 250
    for b in first..253 {
        print(b)
    }
    for big in 4999999999..5000000001 {
        print(big)
    }
    var all = List<byte>()
    for i in 0..256 {
        all.add((byte)i)
    }
    print($"{all.length()} {all[0]} {all[^1]}")
}
```

```text
250
251
252
4999999999
5000000000
256 0 255
```

`b` is a `byte`, the type of `first`, and `big` a `long`, since its ends do not fit an `int`. A count of a type
never reaches past the type: a range stops before its end, and 256 is not a `byte`. To go over every value of a
`byte`, count in `int` and convert.

### Ranges in patterns

```sg
string kind(char c) {
    return match c {
        'a'..='z' or 'A'..='Z' => "letter"
        '0'..='9' => "digit"
        _ => "other"
    }
}

string size(int n) {
    return match n {
        -5..=-1 => "a little below"
        0 => "none"
        1..=9 => "a few"
        10..=99 => "some"
        _ => "far"
    }
}

void main() {
    for c in "aZ7?".chars() {
        print($"{c} {kind(c)}")
    }
    print($"{size(-3)} {size(0)} {size(9)} {size(10)} {size(100)}")
}
```

```text
a letter
Z letter
7 digit
? other
a little below none a few some far
```

A pattern range takes both ends: `1..=9` has the 9. Ranges and values join with `or`.

### Counting down

```sg
void main() {
    var xs = List<string> { "a", "b", "c" }
    for i in (0..xs.length()).reversed() {
        print($"{i} {xs[i]}")
    }
    for i in (5..5).reversed() {
        print(i)
    }
}
```

```text
2 c
1 b
0 a
```

`(0..3).reversed()` is 2, 1 and 0. The count never goes below the start, so a count of an unsigned type, as
`uint`, stops at 0 without wrapping. `(5..5).reversed()` has no number in it. A range in brackets is written
only so, at the head of a `for`.

## What stops the program

### A range that ends before it starts

```sg panic
void main() {
    var xs = List<int> { 10, 20, 30, 40 }
    var from = 3
    print(xs[from..1])
}
```

```text
panic: range 3..1 ends before it starts
```

In a `for`, such a range runs no round. Between brackets it is a mistake in the data, not a request for nothing.

### An end past the items

```sg panic
void main() {
    var xs = List<int> { 10, 20, 30, 40 }
    var from = 9
    print(xs[from..])
}
```

```text
panic: range 9..4 is out of range: the length is 4
```

`xs[from..]` ends at the length, 4, so the message shows `9..4`.

### A cut through a character

```sg panic
void main() {
    var word = "año"
    print(word[0..2])
}
```

```text
panic: range 0..2 cuts a character in half
```

`ñ` is bytes 1 and 2 of the text, and a `string` is always valid UTF-8.

### An end that overflows

```sg panic
void main() {
    byte last = 255
    for b in (byte)250..last + 1 {
        print(b)
    }
}
```

```text
panic: overflow: 255 + 1 does not fit in byte
```

An end is a sum like any other: in a debug build and in the interpreter its overflow stops the program, and in a
release build it wraps, so this loop runs no round there. The count is a `byte`, which never holds 256: count
in `int` instead.

## What the compiler refuses

### `..=` outside a pattern

```sg error
void main() {
    var xs = List<int> { 10, 20, 30 }
    print(xs[0..=1])
}
```

```text
error: `..=` is written only in a pattern; a range of a text or a slice stops before its end
  --> example.sg:3:15
 3 |     print(xs[0..=1])
   |               ^^^
  hint: `[a..b + 1]`
```

A `for` refuses `1..=3` the same way, with the hint `for i in a..b + 1`.

### `..` in a pattern

```sg error
string kind(char c) {
    return match c {
        'a'..'z' => "small"
        _ => "other"
    }
}

void main() {
    print(kind('q'))
}
```

```text
error: a range in a pattern takes its last value too, and is written with `..=`
  --> example.sg:3:12
 3 |         'a'..'z' => "small"
   |            ^^
  hint: `'a'..='z'`
```

With `..`, `'a'..'z'` would quietly leave out the `z`.

### A range that takes nothing

```sg error
string size(int n) {
    return match n {
        9..=1 => "a few"
        _ => "other"
    }
}

void main() {
    print(size(4))
}
```

```text
error: this range takes nothing: its first value is greater than its last
  --> example.sg:3:9
 3 |         9..=1 => "a few"
   |         ^^^^^
  hint: the smaller value comes first: `1..=9`
```

### A name at the end of a pattern range

```sg error
const int Top = 9

string size(int n) {
    return match n {
        1..=Top => "a few"
        _ => "other"
    }
}

void main() {
    print(size(4))
}
```

```text
error: a range in a pattern ends with a number or a character
  --> example.sg:5:13
 5 |         1..=Top => "a few"
   |             ^^^
```

The ends of a pattern are written as they are, so that the compiler can count what the arms take.

### A range kept in a variable

```sg error
void main() {
    var firstThree = 0..3
    for i in firstThree {
        print(i)
    }
}
```

```text
error: expected end of line, found `..`
  --> example.sg:2:23
 2 |     var firstThree = 0..3
   |                       ^^
```

A range is not a value: it is written where it is used, at the head of the `for`.

### A range of characters

```sg error
void main() {
    for c in 'a'..'e' {
        print(c)
    }
}
```

```text
error: the start of the range must be an integer, found `char`
  --> example.sg:2:14
 2 |     for c in 'a'..'e' {
   |              ^^^
```

A `for` counts integers. The bytes of the letters are `(byte)'a'..(byte)'e'`, and the characters of a text
`"abcd".chars()`.

### Ends whose types do not mix

```sg error
void main() {
    int from = -2
    nuint to = 3
    for i in from..to {
        print(i)
    }
}
```

```text
error: `int` and `nuint` do not mix: neither holds every value of the other
  --> example.sg:4:14
 4 |     for i in from..to {
   |              ^^^^^^^^
  hint: convert one side to the type of the other: `(int)to`; it panics if the value does not fit
```
