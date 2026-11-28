---
name: x?.y
kind: sign
summary: A field or a method of x, or null when x is null.
see: T?, null, x ?? y, is, try
learn: ../docs/learn/book/06-null-and-errors.md#3f-2e
---

## Forms

| Form | What it is |
|---|---|
| `x?.field` | the field of a `T?`, or `null` when `x` is `null` |
| `x?.method(a)` | what the method gives, or `null`; `a` is computed only when `x` holds a value |
| `x?.a.b`, `x?.a?.b` | a chain: once a part is `null`, the rest does not run |
| `x?.field ?? value` | the field, or a value in its place |
| `x?.change()` | on a line of its own: a method that gives nothing, called only when `x` holds a value |
| `x?.field is not null` | a test that narrows `x` too |

## What it means

`x?.y` reaches into a value that may be missing. When `x` holds a value, it is `x.y`; when `x` is `null`, it is
`null`, and nothing to the right of `?.` runs: no field is read, no method is called, and the arguments of the
method are not computed.

The result is a `U?`, where `U` is the type of the end of the chain, and never a `U??`: a field that is a `T?`
already gives a `T?`. One `?.` covers the whole chain after it, so `x?.a.b` needs a second `?.` only where `a`
itself may be `null`. The result is mostly given to `??`, tested with `is`, or kept in a variable.

A field that can be copied is copied: `p?.age` is an `int?`. A field that cannot is lent, not moved: `p?.name`
is a `read string?`, and `p?.name.clone()` owns its text. A method is called on the place itself, lent to look
or to change as the method asks.

`?.` is written on a `T?`, a `read T?` and a `T*?`. On a value that is never `null` it is an error, with the hint
to write `.`; on a `Result` it is an error too, since a `Result` holds an error, not `null`: `try` or `??` open
it. `?.` only reads: nothing is assigned through it, and there is no `?[i]`.

`?.` costs one test of the flag, or of the pointer for a `T*?`, as an `if` would.

## Examples

### A field that may be missing

```sg
struct Address {
    string city
}

struct Person {
    string name
    Address? address
}

void main() {
    var people = List<Person> {
        Person { name = "Ada", address = Address { city = "London" } },
        Person { name = "Pasha", address = null }
    }
    for person in read people {
        var city = person.address?.city ?? "unknown"
        print($"{person.name}: {city}")
    }
}
```

```text
Ada: London
Pasha: unknown
```

`person.address` is an `Address?`, so `person.address?.city` is a `read string?`, and `??` gives a text in place
of `null`.

### Methods

```sg
void show(read string? name) {
    var letters = name?.length() ?? 0
    print($"{letters} letters, {name?.toUpper() ?? "-"}")
}

void main() {
    show("Ada")
    show(null)
}
```

```text
3 letters, ADA
0 letters, -
```

`name?.length()` is a `nint?`, `null` when there is no name.

### A chain

```sg
struct Address {
    string city
    int zip
}

struct Person {
    string name
    Address? address
}

void main() {
    Person? ada = Person { name = "Ada", address = Address { city = "London", zip = 1815 } }
    Person? nobody = null
    print(ada?.address?.city ?? "?")
    print(nobody?.address?.city ?? "?")
    var zip = ada?.address?.zip
    print(zip ?? 0)
    print(ada?.name.length() ?? 0)
}
```

```text
London
?
1815
3
```

`zip` is an `int?`, not an `int??`. In `ada?.name.length()`, `name` is never `null`, so one `?.` is enough.

### Arguments that do not run

```sg
int next() {
    print("next runs")
    return 1
}

struct Box {
    int n

    int plus(read this, int k) {
        return this.n + k
    }
}

void main() {
    Box? none = null
    print(none?.plus(next()) ?? 0)
    Box? some = Box { n = 1 }
    print(some?.plus(next()) ?? 0)
}
```

```text
0
next runs
2
```

When `none` is `null`, `plus` is not called, and its argument `next()` is not computed either.

### A method that changes

```sg
struct Counter {
    int n

    void bump(ref this) {
        this.n += 1
    }
}

void main() {
    Counter? clicks = Counter { n = 0 }
    clicks?.bump()
    clicks?.bump()
    print(clicks?.n ?? -1)
    clicks = null
    clicks?.bump()
    print(clicks?.n ?? -1)
}
```

```text
2
-1
```

`bump` gives nothing, so `clicks?.bump()` stands on a line of its own. It changes `clicks` where it is, and does
nothing when `clicks` is `null`.

### A test that narrows

```sg
struct Address {
    string city
}

struct Person {
    Address? address
}

void main() {
    Person? p = Person { address = Address { city = "Rome" } }
    if p?.address is not null {
        print(p.address?.city ?? "")
    }
}
```

```text
Rome
```

Where `p?.address is not null` holds, `p` is not `null` either, so inside the `if` it is a `Person` and is
reached with `.`.

### Through a pointer

```sg
struct Node {
    int value
    Node*? next
}

void main() {
    Node*? head = new Node { value = 1, next = new Node { value = 2, next = null } }
    print(head?.value ?? 0)
    print(head?.next?.value ?? 0)
    print(head?.next?.next?.value ?? 0)
}
```

```text
1
2
0
```

On a `Node*?` the test is of the pointer: `null` is the address zero.

### Owning what a chain lends

```sg
struct Person {
    string name
}

string nameOf(read Person? person) {
    return person?.name.clone() ?? "nobody".clone()
}

void main() {
    Person? ada = Person { name = "Ada" }
    var name = nameOf(read ada)
    print(name)
    print(nameOf(null))
}
```

```text
Ada
nobody
```

`person?.name` is a `read string?` that looks into `ada`. `.clone()` at the end of the chain makes a `string?`
that owns its text, so it can be returned.

## What the compiler refuses

### A value that is never `null`

```sg error
struct Point {
    int x
}

void main() {
    var p = Point { x = 1 }
    print(p?.x)
}
```

```text
error: `p` is a `Point`, which is never `null`
  --> example.sg:7:11
 7 |     print(p?.x)
   |           ^
  hint: write `.` instead of `?.`
```

### A `Result`

```sg error
void main() {
    var r = int.parse("-4")
    print(r?.abs() ?? 0)
}
```

```text
error: `?.` goes past a `null`, and `r` is a `Result<int, NumberError>`, which holds a value or an error
  --> example.sg:3:11
 3 |     print(r?.abs() ?? 0)
   |           ^
  hint: `(try r)` passes the error on, and `(r ?? fallback)` puts a value in its place
```

### A method that gives nothing, used as a value

```sg error
struct Counter {
    int n

    void bump(ref this) {
        this.n += 1
    }
}

void main() {
    Counter? c = Counter { n = 1 }
    var done = c?.bump()
}
```

```text
error: `c?.bump()` gives nothing, so `?.` has no value to give
  --> example.sg:11:16
 11 |     var done = c?.bump()
    |                ^^^^^^^^^
  hint: write it on a line of its own
```

### Owning a lent field

```sg error
struct Person {
    string name
}

void main() {
    Person? p = Person { name = "Ada" }
    string? name = p?.name
    print(name ?? "")
}
```

```text
error: expected `string?`, found `read string?`
  --> example.sg:7:20
 7 |     string? name = p?.name
   |                    ^^^^^^^
  hint: `read string?` only borrows the text; to own a copy write `p?.name.clone()`
```

The types named are those of the whole `p?.name`. A method that gives another type is named the same way:
`int? n = name?.length()` is refused as ``expected `int?`, found `nint?` ``.

### Assigning through `?.`

```sg error
struct Point {
    int x
}

void main() {
    Point? p = Point { x = 1 }
    p?.x = 5
}
```

```text
error: `?.` only reads: a field is written through a value that is there
  --> example.sg:7:5
 7 |     p?.x = 5
   |     ^^^^
  hint: test it first: `if p is not null { p.x = 5 }`
```

### An item through `?.`

```sg error
void main() {
    List<int>? ys = List<int> { 1 }
    print(ys?[0])
}
```

```text
error: there is no `?[i]`: `?.` reaches fields and methods, not items
  --> example.sg:3:11
 3 |     print(ys?[0])
   |           ^^^^^^
  hint: `ys?.get(0)` gives `null` when `ys` is `null` or has no such item; or test it first: `if ys is not null { ... ys[0] ... }`
```
