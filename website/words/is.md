---
name: is
kind: keyword
summary: Tests whether a value is one variant, or null, and names what it carries.
see: match, not, null, T?, or, ref
learn: ../docs/learn/book/05-structs-and-enums.md#is-one-variant
---

## Forms

| Form | What it is |
|---|---|
| `x is Circle(r)` | true when `x` is that variant; `r` borrows what it carries |
| `x is Rect(w, _)` | `_` skips a value; each value is named or skipped |
| `x is Empty` | a variant that carries nothing |
| `x is Ok(n)`, `x is Error(e)` | the same on a `Result` |
| `x is null` | true when a `T?` holds nothing |
| `x is not P` | the same test, the other way round; `not` is written only here |
| `x is Circle(r) && r > 2` | the names are seen on the right of `&&` and in the block |
| `ref x is Circle(r)` | the names borrow to change |
| `x is Number(1..=9 or 40)`, `x is Word("let")` | a pattern inside the parentheses, as in `match` |

## What it means

`is` asks whether a value has one shape: a variant of an enum, `Ok` or `Error` of a `Result`, or `null`. It gives
a `bool`, so it goes where a condition goes: an `if`, a `while`, `&&` and `||`, the condition of `?:`. An `is`
that names values goes where its names can be seen: on the left of `&&`, and an `is not` on the left of `||`.
`x is not P` is true when `x is P` is false.

The names in the pattern are seen only where the test is sure to have held: in the block of the `if` or `while`,
on the right of `&&`, and in the first arm of `?:`. For `is not` it is the other way round: the names are seen
where the test failed, on the right of `||`, in `else`, and after an `if` whose block always leaves (`return`,
`break`, `continue`). A name used anywhere else is an error.

`is` only looks. It never moves `x`, and its names are `read` borrows of what `x` carries. They are `ref` borrows
when `x` is a `ref` borrow, or when the test is written `ref x is ...`. To take the values out and own them,
write a `match`.

On a `T?`, `x is not null` also narrows: where it holds, `x` is a `T`. Unlike a `match`, `is` tests one pattern
and does not ask for every case. At its top it takes a variant or `null`, never a number or a range; inside the
parentheses go the patterns of `match`.

## Examples

### A variant in an `if`

```sg
enum Shape {
    Circle(int radius)
    Rect(int width, int height)
    Empty
}

void describe(read Shape shape) {
    if shape is Circle(r) {
        print($"a circle of radius {r}")
    }
    if shape is Rect(w, _) && w > 10 {
        print($"a wide rectangle, {w} across")
    }
    if shape is Empty {
        print("nothing")
    }
}

void main() {
    describe(read Shape.Circle(3))
    describe(read Shape.Rect(12, 2))
    describe(read Shape.Rect(4, 2))
    describe(read Shape.Empty)
}
```

```text
a circle of radius 3
a wide rectangle, 12 across
nothing
```

`r` is the radius the circle carries. `w` is seen on the right of `&&` too, so the narrow rectangle prints
nothing.

### `null`

```sg
void greet(read string? name) {
    if name is null {
        print("hello, stranger")
        return
    }
    print($"hello, {name}, {name.length()} letters")
}

void main() {
    greet(null)
    greet("Ada")
    string? nickname = null
    print(nickname is not null)
}
```

```text
hello, stranger
hello, Ada, 3 letters
false
```

After the `if`, which returns, `name` is a `read string` and has a `length()`. An `is` that names nothing is a
`bool` like any other, and can be printed.

### `is not`, and the names after the `if`

```sg
int portOf(read string text) {
    if int.parse(text) is not Ok(port) || port < 1 || port > 65535 {
        return 80
    }
    return port
}

void main() {
    print(portOf("8080"))
    print(portOf("http"))
    print(portOf("70000"))
}
```

```text
8080
80
80
```

`port` is seen on the right of `||`, where the parse did not fail, and after the `if`, whose block returns. The
`Result` that `int.parse` gives has no name; it stays until the end of the function.

### Patterns inside the parentheses

```sg
enum Token : Formattable {
    Number(int value)
    Word(string text)
    Plus
}

void main() {
    var tokens = List<Token> { Token.Number(2), Token.Plus, Token.Word("x".clone()), Token.Number(40) }
    for token in read tokens {
        if token is Number(1..=9 or 40) {
            print("a small number, or forty")
        }
        if token is Word("x") {
            print("the word x")
        }
        if token is not Plus {
            print(token)
        }
    }
}
```

```text
a small number, or forty
Number(2)
the word x
Word("x")
a small number, or forty
Number(40)
```

Ranges, `or` and texts test the value inside the variant, as they do in a `match`.

### In a `while`

```sg
void main() {
    var fields = List<string> { "12", "30", "x7", "5" }
    nint i = 0
    var total = 0
    while i < fields.length() && int.parse(read fields[i]) is Ok(n) {
        total += n
        i += 1
    }
    print($"{total} from the first {i} fields")
}
```

```text
42 from the first 2 fields
```

The loop goes on while the next field is a number, and `n` is that number in each round. It stops at `"x7"`.

### Changing what a variant carries

```sg
enum Shape : Formattable {
    Circle(int radius)
    Rect(int width, int height)
}

void grow(ref Shape shape) {
    if shape is Circle(r) {
        r += 1
    }
}

void main() {
    var shape = Shape.Rect(3, 4)
    if ref shape is Rect(w, _) {
        w = 10
    }
    print(shape)
    var round = Shape.Circle(1)
    grow(ref round)
    print(round)
}
```

```text
Rect(10, 4)
Circle(2)
```

`ref shape is` makes `w` a `ref int`, so writing it changes `shape`. In `grow`, `shape` is a `ref` borrow
already, and `r` is one too.

### Looking, then taking with `match`

```sg
enum Message {
    Text(string body)
    Quit
}

void keep(string body) {
    print($"kept {body}")
}

void main() {
    var m = Message.Text("hello".clone())
    if m is Text(body) {
        print(body.length())
    }
    match m {
        Text(body) => keep(body)
        Quit => print("quit")
    }
}
```

```text
5
kept hello
```

The `is` only looks, so `m` still owns its text after it. The `match` takes `m`, and there `body` owns the
text and can be given away.

## What the compiler refuses

### A variant without its values

```sg error
enum Shape {
    Circle(int radius)
    Rect(int width, int height)
}

void main() {
    var shape = Shape.Rect(3, 4)
    if shape is Rect {
        print("a rectangle")
    }
}
```

```text
error: `Rect` carries 2 value(s); the pattern must name or skip each
  --> example.sg:8:17
 8 |     if shape is Rect {
   |                 ^^^^
  hint: `Rect(width, height)`, or `Rect(_, _)` to skip them
```

### A name where the test may have failed

```sg error
enum Shape {
    Circle(int radius)
    Empty
}

void main() {
    var shape = Shape.Circle(2)
    if shape is Circle(r) {
        print("round")
    } else {
        print(r)
    }
}
```

```text
error: there is no variable named `r`
  the names of `is` are seen where its test is sure to have held: on the right of `&&`, in the block of its `if` or `while`, in the first arm of its `?:`, and after an `if` whose `else` leaves
  --> example.sg:11:15
 11 |         print(r)
    |               ^
  `r` is named by this `is`:
  --> example.sg:8:24
 8 |     if shape is Circle(r) {
   |                        ^
  hint: use the value where the name is seen, or take it out with `match`
```

### Giving away what `is` only borrows

```sg error
enum Message {
    Text(string body)
    Quit
}

void keep(string body) {
    print(body)
}

void main() {
    var m = Message.Text("hello".clone())
    if m is Text(body) {
        keep(body)
    }
}
```

```text
error: expected `string`, found `read string`
  --> example.sg:13:14
 13 |         keep(body)
    |              ^^^^
  hint: `read string` only borrows the text; to own a copy write `body.clone()`
```

`body` borrows from `m`. A `match m` takes the text out, as in the last example.

### A number in place of a variant

```sg error
void main() {
    var port = 8080
    if port is 1..=65535 {
        print("a port")
    }
}
```

```text
error: expected `null` or a variant name, found `1`
  `is` tests for `null` or for one variant of an enum
  --> example.sg:3:16
 3 |     if port is 1..=65535 {
   |                ^
```

A number is compared with `<` and `>`, or sorted by a `match`, where `1..=65535` is a pattern.
