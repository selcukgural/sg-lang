---
name: true
kind: value
summary: The bool that is not false; what a comparison gives when it holds.
see: bool, false, if, while, match, c ? a : b
learn: ../docs/learn/book/02-values.md#bool-and-char
---

## Forms

| Form | What it is |
|---|---|
| `bool ready = true`, `return true` | the value, kept or given back |
| `10 > 3`, `names.contains("Ada")` | what a comparison or a test gives when it holds |
| `while true { ... break }` | a loop that runs until a `break` or a `return` |
| `ready \|\| check()` | when `ready` is `true`, `check()` never runs |
| `true => ...`, `Lamp(true) => ...` | an arm of a `match`, at the top or inside a variant |
| `flag ? 1 : 0` | the number it stands for, written out |

## What it means

`true` is one of the two values of a [`bool`](bool.md); [`false`](false.md) is the other. It is what a
comparison gives when it holds, and what a test such as `contains` gives when the answer is yes. `!true` is
`false`.

`||` stops at `true`: when its left side is `true`, so is the whole, and the right side is not run. `&&` with a
`true` on the left gives its right side.

`true` is not a number. It is not `1`, `(int)true` is refused, and `flag ? 1 : 0` says which number it stands
for. It is printed as `true`, and `.toString()` makes that text. A text is turned into a `bool` by comparing it:
`word == "true"`.

A `while true` runs until a `break` or a `return` leaves it. A `match` on a `bool` has an arm `true` and one
`false`; `true` is a pattern inside a variant too, as in `Lamp(true)`. It is written in lower case only.

## Examples

### Where `true` comes from

```sg
void main() {
    bool ready = true
    var big = 10 > 3
    var names = HashSet<string>()
    var added = names.add("Ada")
    print($"{ready} {big} {added} {names.contains("Ada")}")
    print($"{ready == big} {!ready} {ready && big}")
    var text = true.toString()
    print($"{text} has {text.length()} letters")
}
```

```text
true true true true
true false true
true has 4 letters
```

`names.add` gives `true` when the item was not there before.

### `while true`

```sg
void main() {
    var n = 27
    var steps = 0
    var highest = n
    while true {
        if n == 1 {
            break
        }
        n = n % 2 == 0 ? n / 2 : 3 * n + 1
        highest = n > highest ? n : highest
        steps += 1
    }
    print($"{steps} steps, highest {highest}")
}
```

```text
111 steps, highest 9232
```

The test is inside the loop, where the value it needs is known; `break` leaves.

### `||` stops at `true`

```sg
bool askServer(string name) {
    print($"  asking the server about {name}")
    return false
}

void main() {
    var isAdmin = true
    if isAdmin || askServer("ada") {
        print("ada may come in")
    }
    print(true || askServer("b"))
    print(false || askServer("c"))
    print(true && askServer("d"))
}
```

```text
ada may come in
true
  asking the server about c
false
  asking the server about d
false
```

The server is never asked about `ada` or `b`: the left side was already `true`. `&&` needs its right side
when the left is `true`, so `d` is asked.

### In a `match`

```sg
enum Light {
    Lamp(bool on)
    Candle
}

string describe(read Light light) {
    return match light {
        Lamp(true) => "a lamp that is on"
        Lamp(false) => "a lamp that is off"
        Candle => "a candle"
    }
}

string sign(int n) {
    return match n >= 0 {
        true => "not negative"
        false => "negative"
    }
}

void main() {
    print(describe(read Light.Lamp(true)))
    print(describe(read Light.Lamp(1 > 2)))
    print(describe(read Light.Candle))
    print(sign(-3))
    print(sign(0))
}
```

```text
a lamp that is on
a lamp that is off
a candle
negative
not negative
```

An arm for `true` and one for `false` make the `match` whole, with no `_`.

### From text, and as a number

```sg
void main() {
    for word in "true false yes TRUE".splitWhitespace() {
        bool flag = word == "true"
        print($"{word,-5} -> {flag,-5} {(flag ? 1 : 0)}")
    }
    var count = 0
    var votes = List<bool> { true, false, true, true }
    for vote in read votes {
        count += vote ? 1 : 0
    }
    print($"{count} of {votes.length()} said yes")
}
```

```text
true  -> true  1
false -> false 0
yes   -> false 0
TRUE  -> false 0
3 of 4 said yes
```

Only the text `true`, in lower case, is `true` here; the comparison says exactly which texts count.

## What the compiler refuses

### `true` as a number

```sg error
void main() {
    print((int)true + 1)
}
```

```text
error: a `bool` value cannot be converted to `int`
  --> example.sg:2:11
 2 |     print((int)true + 1)
   |           ^^^^^^^^^
  hint: a `bool` is not a number: `true ? 1 : 0`
```

`(true ? 1 : 0) + 1` is `2`.

### `1` for `true`

```sg error
void main() {
    var n = 0
    while 1 {
        n += 1
        if n == 3 {
            break
        }
    }
    print(n)
}
```

```text
error: expected `bool`, found `int`
  --> example.sg:3:11
 3 |     while 1 {
   |           ^
  hint: a `bool` is `true` or `false`: write `true`
```

`while true` is the loop that runs until a `break`.

### A `bool?` as the condition

```sg error
void main() {
    bool? answer = true
    if answer {
        print("yes")
    }
}
```

```text
error: expected `bool`, found `bool?`
  --> example.sg:3:8
 3 |     if answer {
   |        ^^^^^^
  hint: it may be `null`: give a fallback with `answer ?? ...`, or test it with `is null`
```

`answer ?? false` and `answer == true` are `true` only when `answer` holds `true`.
