---
name: null
kind: value
summary: No value; only a type written T? may hold it, and it is tested with is null.
see: T?, is, match, x ?? y, x?.y, T*, raw
learn: ../docs/learn/book/06-null-and-errors.md#t-and-null
---

## Forms

| Form | What it is |
|---|---|
| `int? count = null` | a variable that holds nothing yet |
| `return null` | a function that returns a `T?` gives nothing back |
| `greet(null)` | an argument for a parameter of a `T?` |
| `User { nickname = null }`, `string? nickname = null` | a field that holds nothing, and a default of a field |
| `x is null`, `x is not null` | the test |
| `null => ...`, `Go(null)` | a pattern in a `match`, at the top or inside a variant |
| `ready ? value : null` | an arm of `?:` that makes it a `T?` |
| `p == null` | the test of a `raw` pointer, which is an address |

## What it means

`null` is the value of a `T?` that holds nothing. There is no null of every type: an `int`, a `string`, a
struct and a `T*` always hold a value, and `null` where one of them goes is an error. A pointer that may be
missing is written `Node*?`; compiled, its `null` is the address zero.

`null` has no type of its own. It takes the type of the place it goes to, as a number does: a variable with its
type written, a `return`, an argument, a field. So `var x = null` is an error: nothing says what `x` would hold.

A `T?` is tested with `is null` and `is not null`, never with `== null`. The test narrows the variable: where `x
is not null` holds, `x` is a `T`, and it stays one when a `T` is assigned to it; a `T?` or `null` assigned ends
that. In a `match`, an arm `null` takes the missing value, and a name in an arm below it is a `T`; so is a name
below `Go(null)`, in the same place of `Go`. Two `T?` values, or a `T?` and a `T`, are compared with `==`: they are
equal when both are `null` or both hold equal values. The one value compared with `== null` is a `raw` pointer,
which is an address as in C and is not a `T?`.

`null` is written as text only inside a value that is printed, such as a list or a struct: `[90, null, 75]`.
A `T?` alone is not written: it is given a fallback with `??`, or tested, before it is printed. `share` on a `weak` look
gives `null` once the value it looked at is gone.

## Examples

### Where `null` goes

```sg
struct User : Formattable {
    string name
    string? nickname = null
    int? age
}

int? half(int n) {
    if n % 2 != 0 {
        return null
    }
    return n / 2
}

void greet(read string? name) {
    print($"hello, {name ?? "stranger"}")
}

void main() {
    int? count = null
    print(count ?? -1)
    print(half(7) ?? -1)
    greet(null)
    greet("Ada")
    var ada = User { name = "Ada", age = null }
    print(ada)
}
```

```text
-1
-1
hello, stranger
hello, Ada
User { name = "Ada", nickname = null, age = null }
```

`nickname` has a default, so it is left out where `ada` is built; `age` has none and is given `null` by hand.

### `null` in a `match`

```sg
enum Command {
    Go(string? target)
    Stop
}

void run(read Command command) {
    match command {
        Go(null) => print("go where?")
        Go("home") => print("going home")
        Go(target) => print($"going to {target}")
        Stop => print("stopping")
    }
}

string greeting(read string? name) {
    return match name {
        null => "hello, stranger"
        "Ada" => "welcome back, Ada"
        var other => $"hello, {other}, {other.length()} letters".toString()
    }
}

void main() {
    run(read Command.Go(null))
    run(read Command.Go("home"))
    run(read Command.Go("Paris"))
    run(read Command.Stop)
    print(greeting(null))
    print(greeting("Ada"))
    print(greeting("Grace"))
}
```

```text
go where?
going home
going to Paris
stopping
hello, stranger
welcome back, Ada
hello, Grace, 5 letters
```

In `greeting`, `other` is below the arm `null`, so it is a `read string` with a `length()`. In `run`, `target` is
below `Go(null)`, so it is a `read string` too, and is written with no fallback.

### Written as text

```sg
struct Reading : Formattable {
    string place
    double? celsius
}

void main() {
    var scores = List<int?> { 90, null, 75 }
    print(scores)
    print(Reading { place = "Oslo", celsius = null })
    print(Reading { place = "Rome", celsius = 21.5 })
}
```

```text
[90, null, 75]
Reading { place = "Oslo", celsius = null }
Reading { place = "Rome", celsius = 21.5 }
```

### A `raw` pointer

```sg
void main() {
    raw int nothing = null
    var block = Memory.allocate<int>(1)
    print(nothing == null)
    print(block == null)
    unsafe Memory.free(block)
}
```

```text
true
false
```

A `raw int` is an address, as C's `int*` is, and may be `null` without a `?`. It is compared with `==`, and
`is null` is refused on it.

## What the compiler refuses

### `null` where a value must be

```sg error
int lengthOf(read string? text) {
    if text is null {
        return null
    }
    return (int)text.length()
}

void main() {
    print(lengthOf("Ada"))
}
```

```text
error: `int` cannot be `null`
  --> example.sg:3:16
 3 |         return null
   |                ^^^^
  hint: a value that may be missing has the type `int?`
```

### A pointer that must point

```sg error
struct Node {
    int value
    Node* next
}

void main() {
    var last = new Node { value = 1, next = null }
    print(last.value)
}
```

```text
error: `Node*` cannot be `null`
  --> example.sg:7:45
 7 |     var last = new Node { value = 1, next = null }
   |                                             ^^^^
  hint: a value that may be missing has the type `Node*?`
```

A `Node*` always owns a node, so a list whose end is `null` writes its link `Node*? next`.

### `var` with `null`

```sg error
void main() {
    var best = null
    print(best ?? 0)
}
```

```text
error: `var` cannot find a type from `null` alone
  --> example.sg:2:16
 2 |     var best = null
   |                ^^^^
  hint: write the type, for example `int? best = null`
```

### Comparing with `==`

```sg error
void main() {
    var ages = HashMap<string, int> { {"Ada", 36} }
    var age = ages.get("Bob")
    if age == null {
        print("no age")
    }
}
```

```text
error: `null` is not compared with `==`
  --> example.sg:4:8
 4 |     if age == null {
   |        ^^^^^^^^^^^
  hint: write `age is null`
```

`age is null` is the test, and in its `else`, `age` is narrowed.

### A test that is always false

```sg error
void main() {
    var names = List<string> { "Ada" }
    var count = names.length()
    if count is null {
        print("empty")
    }
}
```

```text
error: this is `nint`, which is never `null`
  only a `T?` can be `null`
  --> example.sg:4:8
 4 |     if count is null {
   |        ^^^^^
  hint: to test for zero: `count == 0`
```

An empty list has the length 0.

### An arm `null` on a value that has none

```sg error
void main() {
    int code = 200
    match code {
        null => print("no code")
        _ => print(code)
    }
}
```

```text
error: an `int` is never `null`
  only a value of a type written with `?` can be `null`
  --> example.sg:4:9
 4 |         null => print("no code")
   |         ^^^^
```
