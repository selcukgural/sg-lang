---
name: with
kind: keyword
summary: A copy of a struct with some fields changed: p with { x = 3 }.
see: struct, this, readonly, public, read
learn: ../docs/learn/book/05-structs-and-enums.md#a-copy-with-some-fields-changed-with
---

## Forms

| Form | What it is |
|---|---|
| `p with { x = 3 }` | a new value of `p`'s struct, with `x` changed and the other fields as they were |
| `p with { x = 3, y = 4 }` | several fields changed, in any order |
| `p.clone() with { ... }` | a copy of a value that is kept, or only lent |
| `this with { ... }` | in a method that takes `this`, the value it was called on, changed |
| `b with { corner = b.corner with { x = 0 } }` | a field changed inside a field |
| `if (p with { ... }) == q {` | in the head of `if`, `while` or `for`, in parentheses |

## What it means

`with` makes a new value of a struct from another one. The fields written in the braces get the values written
there; every other field is taken as it was. It is C#'s `with` on a record, and Rust's `Point { x: 3, ..p }`.

The value on the left is taken, as `var q = p` takes it. A `Copyable` struct is copied, so `p` is still there
after. Any other struct is moved into the new value, and its old name cannot be used again: nothing is copied
that is not written. To keep the old value, clone it where the copy is made: `p.clone() with { ... }`. A value
that is only lent, such as a `read` parameter, is taken only when it is `Copyable`.

The left side runs first, then the values in the braces in the order written, and only then are the old values
of the changed fields dropped; a `Disposable` one is disposed of there. A value in the braces cannot read a
struct that the left side has moved, so what it needs is read into a variable first.

Compiled, `with` costs what moving the struct costs: its bytes are moved or copied once and the fields written
are set, with no allocation. A text or a list in a field that is not written moves with the struct and is not
copied.

`with` builds a value as a literal does and keeps the literal's rules. Every field written must exist, once, with
a value of its type. A `readonly` field may be written, since the result is a new value. In another module,
`with` copies a struct only when that module sees all of its fields. It works on a struct, not on an enum, a
number, a `T*` or a `shared T`.

`with` binds tighter than an operator between two values and looser than one in front: `a == b with { ... }`
compares `a` with the changed `b`, and `try load() with { ... }` changes what `try` gives. In the head of an `if`, `while` or `for`,
where `{` starts the block, it goes in parentheses, as a literal does.

## Examples

### A copy with a field changed

```sg
struct Point : Copyable, Formattable {
    int x
    int y
}

struct Label : Formattable {
    string text
    int size
    bool bold = false
}

void main() {
    var p = Point { x = 1, y = 2 }
    var q = p with { x = 5 }
    print($"{p} {q}")

    var title = Label { text = "Report", size = 12 }
    var big = title with { size = 24, bold = true }
    print(big)
}
```

```text
Point { x = 1, y = 2 } Point { x = 5, y = 2 }
Label { text = "Report", size = 24, bold = true }
```

`Point` is `Copyable`, so `p` is copied and still there. `Label` holds a `string` and is not: `title` is moved
into `big`, its text with it, and is not used again.

### Keeping the old value

```sg
struct Label : Formattable, Clonable {
    string text
    int size
}

void main() {
    var title = Label { text = "Report", size = 12 }
    var big = title.clone() with { size = 24 }
    var draft = title with { text = "Draft" }
    print(big)
    print(draft)
}
```

```text
Label { text = "Report", size = 24 }
Label { text = "Draft", size = 12 }
```

`title.clone()` is a copy with a text of its own, and `with` takes that. The last use of `title` moves it, and
its old text is dropped where `"Draft"` takes its place.

### In a method

```sg
struct Settings : Formattable {
    string host
    int port = 80
    bool secure = false

    Settings secured(this) {
        return this with { port = 443, secure = true }
    }
}

void main() {
    var plain = Settings { host = "example.org" }
    var safe = plain.secured()
    print(safe)
}
```

```text
Settings { host = "example.org", port = 443, secure = true }
```

`secured` takes `this`, so it owns the settings and gives them back changed. `plain` is used up by the call.

### A lent value

```sg
struct Point : Copyable, Formattable {
    int x
    int y
}

Point shifted(read Point p) {
    return p with { x = p.x + 1 }
}

void main() {
    var p = Point { x = 1, y = 2 }
    print(shifted(read p))
    print(p)
}
```

```text
Point { x = 2, y = 2 }
Point { x = 1, y = 2 }
```

`p` is only lent to `shifted`, and `with` copies it, since `Point` is `Copyable`. As the left side is copied,
the braces may still read `p`.

### A field inside a field

```sg
struct Point : Copyable, Formattable, Equatable {
    int x
    int y
}

struct Box : Copyable, Formattable, Equatable {
    Point corner
    int width
    int height
}

void main() {
    var box = Box { corner = Point { x = 0, y = 0 }, width = 10, height = 5 }
    var moved = box with { corner = box.corner with { x = 3 } }
    print(moved)
    print((moved with { corner = box.corner }) == box)
    print(moved with { width = 2 } == box)
}
```

```text
Box { corner = Point { x = 3, y = 0 }, width = 10, height = 5 }
true
false
```

The inner `with` makes the new corner, the outer one the new box. In the last line `with` binds tighter than
`==`, so no parentheses are needed: the box with a width of 2 is compared with `box`.

### The old value of a field is dropped

```sg
struct LogFile : Disposable {
    string path

    void dispose(ref this) {
        print($"close {this.path}")
    }
}

struct Job {
    string name
    LogFile log
}

void main() {
    var job = Job { name = "build", log = LogFile { path = "a.log" } }
    print("start")
    var next = job with { log = LogFile { path = "b.log" } }
    print($"now {next.name} writes {next.log.path}")
}
```

```text
start
close a.log
now build writes b.log
close b.log
```

The `with` line takes `job`, puts the new log in, and disposes of the old one there. `name` moves to `next`
unchanged.

## What the compiler refuses

### A field the struct does not have

```sg error
struct Label {
    string text
    int size
}

void main() {
    var title = Label { text = "Report", size = 12 }
    var big = title with { sise = 24 }
    print(big.size)
}
```

```text
error: `Label` has no field `sise`
  --> example.sg:8:28
 8 |     var big = title with { sise = 24 }
   |                            ^^^^
  hint: `size`
```

### A field written twice

```sg error
struct Point : Copyable {
    int x
    int y
}

void main() {
    var p = Point { x = 1, y = 2 }
    var q = p with { x = 3, x = 4 }
    print(q.x)
}
```

```text
error: field `x` is given twice
  --> example.sg:8:29
 8 |     var q = p with { x = 3, x = 4 }
   |                             ^
```

### Nothing changed

```sg error
struct Point : Copyable {
    int x
    int y
}

void main() {
    var p = Point { x = 1, y = 2 }
    var q = p with { }
    print(q.x)
}
```

```text
error: `with { }` changes nothing
  --> example.sg:8:13
 8 |     var q = p with { }
   |             ^^^^^^^^^^
  hint: write at least one field: `p with { x = 3 }`
```

A copy of a `Copyable` value is `var q = p`; of another one, `p.clone()`.

### A value that is not a struct

```sg error
enum Shape {
    Circle(int radius)
    Rect(int width, int height)
}

void main() {
    var s = Shape.Rect(3, 4)
    var wide = s with { width = 5 }
    print(wide is Rect)
}
```

```text
error: `with` makes a new value of a struct from another, and this is a `Shape`
  a struct is copied with its fields changed: `p with { x = 3 }`
  --> example.sg:8:16
 8 |     var wide = s with { width = 5 }
   |                ^
```

A variant is built again instead: `Shape.Rect(5, 4)`.

### The old value used after it was moved

```sg error
struct Label {
    string text
    int size
}

void main() {
    var title = Label { text = "Report", size = 12 }
    var big = title with { size = 24 }
    print(title.text)
    print(big.size)
}
```

```text
error: `title` was moved and can no longer be used
  --> example.sg:9:11
 9 |     print(title.text)
   |           ^^^^^
  it was moved here:
  --> example.sg:8:15
 8 |     var big = title with { size = 24 }
   |               ^^^^^
```

`title.clone() with { ... }` keeps `title`, when `Label` is `: Clonable`.

### The moved value read in the braces

```sg error
struct Label {
    string text
    int size
}

void main() {
    var title = Label { text = "Report", size = 12 }
    var big = title with { size = title.size * 2 }
    print(big.size)
}
```

```text
error: `title` was moved and can no longer be used
  --> example.sg:8:35
 8 |     var big = title with { size = title.size * 2 }
   |                                   ^^^^^
  it was moved here:
  --> example.sg:8:15
 8 |     var big = title with { size = title.size * 2 }
   |               ^^^^^
```

The left side runs first and moves `title`. `var size = title.size * 2` on the line before, and
`title with { size = size }`, make the program right.

### A lent value that is not `Copyable`

```sg error
struct Label {
    string text
    int size
}

Label bigger(read Label label) {
    return label with { size = 24 }
}

void main() {
    var title = Label { text = "Report", size = 12 }
    print(bigger(read title).size)
}
```

```text
error: `with` takes the value of `label`, which is lent here, and a `Label` is not `Copyable`
  --> example.sg:7:12
 7 |     return label with { size = 24 }
   |            ^^^^^
```

`with` would move the text out of a label the caller still owns. With `: Clonable` on `Label`,
`label.clone() with { ... }` makes a copy first.

### Without parentheses in an `if`

```sg error
struct Point : Copyable, Equatable {
    int x
    int y
}

void main() {
    var p = Point { x = 1, y = 2 }
    if p with { x = 1 } == p {
        print("same")
    }
}
```

```text
error: `with` needs parentheses here, where `{` starts the block
  --> example.sg:8:10
 8 |     if p with { x = 1 } == p {
   |          ^^^^
  hint: `(p with { ... })`
```

### A field another module does not see

```sg file title="bank/accounts/account.sg"
public struct Account {
    public readonly string owner
    public string? note
    long cents

    public Account create(string owner) {
        return Account { owner = owner, note = null, cents = 0 }
    }
}
```

```sg file title="bank/main.sg"
use accounts

void main() {
    var account = Account("Ada")
    var other = account with { note = "rent" }
    print(other.note ?? "")
}
```

```text
$ cd bank
$ sg main.sg
error: `cents` is not public, so an `Account` cannot be copied with `with` outside the `accounts` module
  --> main.sg:5:17
 5 |     var other = account with { note = "rent" }
   |                 ^^^^^^^
```

`with` is a literal that writes every field, and `main.sg` cannot write `cents`. Inside `accounts`, the same
`with` is allowed. When every field is `public`, even a `readonly` one may be given a new value this way, as
a literal could.
