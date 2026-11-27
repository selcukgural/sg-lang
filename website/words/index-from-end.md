---
name: xs[^1]
kind: sign
summary: An index counted from the end: xs[^1] is the last item, xs[^n] is xs[xs.length() - n].
see: a..b, T[], List, string, T?
learn: ../docs/learn/book/08-collections.md#counting-from-the-end
library: ../docs/library/collections.md#slices
---

## Forms

| Form | What it is |
|---|---|
| `xs[^1]`, `xs[^2]` | the last item, the one before it |
| `xs[^n]`, `xs[^(k + 1)]` | the item `n` from the end: `xs[xs.length() - n]` |
| `xs[^1] = v`, `xs[^1] += v`, `ref xs[^1]` | written and lent as `xs[i]` is |
| `xs[..^1]`, `xs[1..^1]`, `xs[^3..]` | an end of a range counted from the end |
| `xs[a..^0]` | at an end of a range, `^0` is the length |
| `text[^1]` | the last byte of a text |

## What it means

`xs[^n]` is `xs[xs.length() - n]`: `^1` is the last item, `^2` the one before it, and `^n` on `n` items the
first. It is written on a list, an array, a slice and a text, where it counts the bytes. What is indexed is named
once and computed once, so `next()[^1]` calls `next` once, and the place costs what `length() - n` costs.

`^` also ends a range: `xs[..^1]` is every item but the last, `xs[1..^1]` leaves out the first and the last, and
`xs[^3..]` is the last three. At an end of a range `^0` is the length itself, and valid; as an index it is past
the last item, and an error.

`^` is written only first in an index or in an end of a range. It is not a value of its own: `var i = ^1` and
`xs.get(^1)` are errors; between two values, `a ^ b` is the exclusive or of their bits. What follows it is one
operand, as after `-`: a sum goes in parentheses, `xs[^(k + 1)]`. The count is any integer that widens to
`nint`.

A count that is a constant is checked when the program is compiled: `^0` as an index, a negative number after
`^`, and `^4` on an `int[3]` are errors. A count that is known only when the program runs is checked then, in
every build, and a count outside the items stops the program with what was written: `index ^4 is out of range`.
A `HashMap` has no end to count from, since its keys are not places in a row. There is no `last()`:
`xs.get(xs.length() - 1)` reads the last item, or `null` for an empty list.

## Examples

### The last items

```sg
void main() {
    var days = List<string> { "mon", "tue", "wed", "thu", "fri" }
    print(days[^1])
    print(days[^2])
    print(days[^5])
    var primes = int[4] { 2, 3, 5, 7 }
    print(primes[^1])
}
```

```text
fri
thu
mon
7
```

`days[^1]` is `days[days.length() - 1]`, with `days` written once. On five items `^5` is the first.

### The ends of a range

```sg
void main() {
    var xs = List<int> { 10, 20, 30, 40, 50 }
    print(xs[..^1])
    print(xs[1..^1])
    print(xs[^3..])
    print(xs[^3..^1])
    print(xs[2..^0])
    print(xs[^1..^1])
}
```

```text
[10, 20, 30, 40]
[20, 30, 40]
[30, 40, 50]
[30, 40]
[30, 40, 50]
[]
```

`xs[^3..^1]` is from the third item from the end up to the last, without the last. `^0` is the length, so
`xs[2..^0]` is `xs[2..]`.

### Writing at the end

```sg
void main() {
    var scores = List<int> { 7, 3, 9, 1, 5 }
    scores[^1] = 6
    scores[^2] += 10
    print(scores)
    scores[^3..].sort()
    print(scores)
    var tail = ref scores[^2..]
    tail[0] = 0
    print(scores)
}
```

```text
[7, 3, 9, 11, 6]
[7, 3, 6, 9, 11]
[7, 3, 6, 0, 11]
```

An item counted from the end is written, added to and lent as any item. `scores[^3..].sort()` puts only the last
three in order, and `ref scores[^2..]` lends the last two to write.

### A text

```sg
void main() {
    var line = "[42]"
    print(line[1..^1])
    print(line[^1])
    print((char)line[^1])
    var word = "año"
    print(word.length())
    print(word[..^1])
    print(word[^3])
}
```

```text
42
93
]
4
añ
195
```

A text is counted in bytes, as `text[i]` is: `line[^1]` is the byte of `]`, 93. `año` is four bytes, since `ñ`
takes two, and `word[^3]` is the first byte of `ñ`. A range from the end of a text gives a `read string`.

### A count that is computed

```sg
List<int> readings() {
    print("reading")
    return List<int> { 4, 8, 15, 16, 23, 42 }
}

void main() {
    var xs = List<int> { 10, 20, 30, 40 }
    for k in 1..xs.length() + 1 {
        print($"^{k} is {xs[^k]}")
    }
    var skip = 1
    print(xs[^(skip + 1)])
    byte back = 3
    print(xs[^back])
    print(readings()[^1])
}
```

```text
^1 is 40
^2 is 30
^3 is 20
^4 is 10
30
20
reading
42
```

The count may be any integer that widens to `nint`, such as a `byte`. A sum after `^` is put in parentheses.
`readings()` is called once: what is indexed is kept, and its length found from it.

### Lists, slices and consts

```sg
const int[4] Primes = int[4] { 2, 3, 5, 7 }
const int Largest = Primes[^1]

int lastOf(read int[] values) {
    return values[^1]
}

void main() {
    print(Largest)
    var grid = List<List<int>> {
        List<int> { 1, 2 },
        List<int> { 3, 4 }
    }
    grid[^1][^1] = 9
    print(grid)
    var xs = List<int> { 5, 6, 7, 8 }
    var front = xs[..2]
    print(lastOf(front))
    print(lastOf(read xs))
}
```

```text
7
[[1, 2], [3, 9]]
6
8
```

A slice counts from its own end: the last item of `front` is 6, though `xs` goes on. A const is indexed from
the end as from the start. `grid[^1][^1]` is the last item of the last list.

## What stops the program

### A count past the first item

```sg panic
void main() {
    var xs = List<int> { 10, 20, 30 }
    var back = 4
    print(xs[^back])
}
```

```text
panic: index ^4 is out of range: the length is 3
```

The message names the place as it was written, `^4`, not `-1`.

### The last item of an empty list

```sg panic
void main() {
    var xs = List<int>()
    print(xs[^1])
}
```

```text
panic: index ^1 is out of range: the length is 0
```

`xs.get(xs.length() - 1) ?? ...` reads the last item where the list may be empty. For a count that may be 0 or
negative it is a panic too: `index ^0 is out of range`.

### An end of a range outside

```sg panic
void main() {
    var xs = List<int> { 10, 20, 30 }
    var keep = 4
    print(xs[^keep..])
}
```

```text
panic: range bound ^4 is out of range: the length is 3
```

At an end of a range a count from 0 to the length is valid.

### The first and the last of one item

```sg panic
void swapInts(ref int a, ref int b) {
    var kept = a
    a = b
    b = kept
}

void main() {
    var xs = List<int> { 1, 2, 3 }
    swapInts(ref xs[0], ref xs[^1])
    print(xs)
    var one = List<int> { 7 }
    swapInts(ref one[0], ref one[^1])
}
```

```text
[3, 2, 1]
panic: `one[^1]` shares item 0 with `one[0]`, lent to this call too
```

On one item, `^1` is `0`, and two `ref` borrows of one item are not allowed at once.

## What the compiler refuses

### `^0` as an index

```sg error
void main() {
    var xs = List<int> { 10, 20, 30 }
    print(xs[^0])
}
```

```text
error: `^0` is the end itself, past the last item, so this index is always out of range
  --> example.sg:3:14
 3 |     print(xs[^0])
   |              ^^
  hint: the last item is `^1`; `^0` is written as the end of a range: `[a..^0]`
```

### A negative count

```sg error
void main() {
    var xs = List<int> { 10, 20, 30 }
    print(xs[^-1])
}
```

```text
error: a count from the end is not negative: `^1` is the last item
  --> example.sg:3:14
 3 |     print(xs[^-1])
   |              ^^^
  hint: an index from the start is written without `^`
```

### A sum after `^`

```sg error
void main() {
    var xs = List<int> { 10, 20, 30 }
    var k = 1
    print(xs[^k + 1])
}
```

```text
error: `^` counts one operand from the end; what it counts is put in parentheses
  --> example.sg:4:14
 4 |     print(xs[^k + 1])
   |              ^^^^^^
  hint: `^(k + 1)`
```

`^k + 1` could be read as `(length - k) + 1` or as `length - (k + 1)`, so neither is guessed.

### `^` outside the brackets

```sg error
void main() {
    var xs = List<int> { 10, 20, 30 }
    print(xs.get(^1) ?? 0)
}
```

```text
error: `^` counts from the end of what is indexed, so it is written first between `[ ]`: `xs[^1]`, `xs[..^1]`
  an index from the end is not a value of its own; the bit inverse is `~x`
  --> example.sg:3:18
 3 |     print(xs.get(^1) ?? 0)
   |                  ^
```

`var i = ^1` is refused the same way. `xs.get(xs.length() - 1)` gives the last item or `null`.

### Past the start of an array

```sg error
void main() {
    var a = int[3] { 1, 2, 3 }
    print(a[^4])
}
```

```text
error: index ^4 is out of range: the length is 3
  the items of an `int[3]` are at ^1 to ^3 from the end
  --> example.sg:3:11
 3 |     print(a[^4])
   |           ^^^^^
```

An array's length is part of its type, so a constant count is checked when the program is compiled.

### A map

```sg error
void main() {
    var ages = HashMap<int, int> { {1, 36} }
    print(ages[^1])
}
```

```text
error: a map has no end to count from: its keys are not places in a row
  --> example.sg:3:16
 3 |     print(ages[^1])
   |                ^^
  hint: `^` counts from the end of a list, an array, a slice or a text; a key is written as it is: `map[1]`
```
