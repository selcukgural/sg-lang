---
name: var
kind: keyword
summary: Declares a variable; its type comes from its value.
see: read, ref, match, const
learn: ../docs/learn/book/02-values.md#variables
---

## Forms

| Form | What it is |
|---|---|
| `var name = value` | a variable of the type of its value |
| `T name = value` | the same, with the type written |
| `var x = read place` | a variable that borrows, to look |
| `var x = ref place` | a variable that borrows, to change |
| `var other => ...` | in a `match`, an arm that takes the value under a name |
| `_ = value` | no variable: the value is dropped on purpose |

## What it means

`var` declares a variable and gives it a value in one line; the type is the type of the value. A whole number
that fits is an `int` and a larger one a `long`, a number with a point a `double`, `"text"` a `string`, `'c'`
a `char`. Write the type in place of `var` when the value does not say it: `long total = 0`.

Every variable has a value from the start and can be changed, but it keeps its type. Every variable must be read
somewhere, since one that is not is a leftover or a mistake. A name means one thing in a function: a block inside
cannot declare a variable with the name of one around it.

A variable owns its value, unless its value is a borrow: `var x = read place` and `var x = ref place` keep a
borrow, which lasts until the last use of `x`. A borrowed number used as a value is copied: `var before = n`,
with `n` a `ref int`, is a number of its own.

## Examples

### The type from the value

```sg
void main() {
    var count = 3
    var ratio = 0.5
    var name = "Pasha"
    var letter = 'P'
    var ready = true
    count += 1
    print($"{count} {ratio} {name} {letter} {ready}")
}
```

```text
4 0.5 Pasha P true
```

`count` is an `int`, `ratio` a `double`, `name` a `string`, `letter` a `char` and `ready` a `bool`.

### The type written

```sg
void main() {
    long total = 0
    byte small = 200
    double half = 1
    var big = 3000000000
    total += 5
    print($"{total} {small} {half} {big}")
}
```

```text
5 200 1 3000000000
```

`0` alone would be an `int`; `long total = 0` makes it a `long`. `big` is a `long` with no type written, since
the number does not fit in an `int`.

### A variable that borrows

```sg
void main() {
    var counts = List<int> { 1, 2, 3 }
    var slot = ref counts[1]
    slot += 40
    print(counts)
}
```

```text
[1, 42, 3]
```

Writing to `slot` writes to the item it borrows. While `slot` is used, `counts` cannot be touched in any other
way.

### A borrowed number is copied

```sg
void bump(ref int n) {
    var before = n
    n += 1
    print($"{before} -> {n}")
}

void main() {
    var hits = 7
    bump(ref hits)
    print(hits)
}
```

```text
7 -> 8
8
```

`before` keeps the number as it was. A second borrow would be written `var other = ref n`.

### A name in a `match`

```sg
string describe(int code) {
    return match code {
        200 => "fine"
        404 => "not found"
        var other => $"code {other}".toString()
    }
}

void main() {
    print(describe(200))
    print(describe(418))
}
```

```text
fine
code 418
```

`var other` takes every value the arms above did not, under a name.

## What the compiler refuses

### A value of another type

```sg error
void main() {
    var count = 3
    count = 1.5
}
```

```text
error: expected `int`, found `double`
  --> example.sg:3:13
 3 |     count = 1.5
   |             ^^^
  hint: convert with `(int)1.5`; it drops the fraction, and a value that does not fit panics, or becomes the nearest `int` in a release build
```

### A variable nothing reads

```sg error
void main() {
    var count = 3
    int limit = 10
    print(limit)
}
```

```text
error: `count` is never used
  --> example.sg:2:9
 2 |     var count = 3
   |         ^^^^^
  hint: take the line away; to run what it is given and drop the value on purpose, write `_ = ...`
```

### A name that hides another

```sg error
void main() {
    var count = 3
    if count > 2 {
        var count = 4
        print(count)
    }
}
```

```text
error: `count` is already a local variable of a block around this one, and a new name cannot hide it
```

### A value whose type cannot be found

```sg error
void main() {
    var result = Ok(1)
    print(result ?? 0)
}
```

```text
error: the type of this `Result` cannot be found from `Ok(...)` alone
  --> example.sg:2:18
 2 |     var result = Ok(1)
   |                  ^^^^^
  hint: write the type where the value goes, for example `Result<int, MyError> x = Ok(...)`
```

Nothing in `Ok(1)` says what the error would be. A line is read on its own, so the type is written there:
`Result<int, string> result = Ok(1)`.
