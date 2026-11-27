---
name: interface
kind: keyword
summary: What a type can do; eight are built in and named after the colon, and a program cannot write its own yet.
see: struct, enum, this, unsafe, HashSet
learn: ../docs/learn/book/05-structs-and-enums.md#built-in-interfaces
---

## Forms

| Form | What it is |
|---|---|
| `struct Name : Copyable, Formattable` | a struct that takes built-in interfaces |
| `enum Name : Formattable`, `enum Name : byte, Comparable` | an enum that takes them; with numbers, the integer type comes first |
| `Copyable` | the value is copied where another would be moved |
| `Clonable` | `.clone()` gives a copy that owns its own memory |
| `Formattable` | `print` and `$"{value}"` write it; your own `void format(read this, ref Formatter f)` |
| `Equatable` | `==`, `!=`, `contains` and `indexOf`; your own `bool equals(read this, read T other)` |
| `Hashable` | a key of a `HashMap`, an item of a `HashSet`; the type is `Equatable` too |
| `Comparable` | `<`, `>`, `sort`, `binarySearch`; always your own `int compare(read this, read T other)` |
| `Disposable` | your `void dispose(ref this)` runs where the last owner ends |
| `Closable` | your `Result<void, E> close(this)` must be called before the value ends |
| `unsafe Sendable` | the writer's promise that a struct with raw pointers may go to another task: see `unsafe` |
| `interface Name { ... }` | refused: interfaces of your own are put off |

## What it means

An interface says what a type can do. A type takes interfaces by naming them after a colon:
`struct Point : Copyable, Formattable`. Nothing is there unless the type asks for it, so a program never pays
for a copy, a comparison or a hash it did not want. A struct that is not `Formattable` cannot be printed, one
that is not `Equatable` cannot be compared with `==`, and the error says which interface to add.

The language has eight interfaces, and a program cannot add to them: `interface` is a word of the language, but
an `interface` of your own was put off, together with generics and `where`. An interface is not a type either:
a function cannot take "any `Formattable` value". Each use is checked where the compiler sees the type, and
costs a plain call of that type's method when the program runs.

For most interfaces the compiler writes the method from the fields, and each field must be of a type that
has the interface too: a `Formattable` struct is written the way it is built, two `Equatable` values are equal
when their fields are, and `.clone()` clones field by field. A type may write the method itself instead,
`format`, `equals` or `clone`, with the form the table shows. `Comparable` is always written, since an order
made from the order of the fields is rarely the one that is meant; an enum with numbers that writes none is
ordered by its numbers. A `Hashable` type takes its `equals` and its hash from its fields for now.

`Copyable` has no method: it asks that every field is a number, a `bool`, a `char` or another `Copyable` type.
A `string` or a `List` owns memory on the heap, and copying it means allocating, which is written `.clone()`.
`Disposable` and `Closable` are about the end of a value. `dispose` is never called by the program; it runs
once, where the last owner ends. `close` is called by the program, and a local or a parameter left unclosed
is a compile error, so the error that closing gives is never lost. Both are for structs for now.

A method of the right name does not take the interface: the name after the colon does. A `format` in a struct
that is not `Formattable` is a method like any other, and `print` does not use it.

## Examples

### Asking for what a type can do

```sg
struct Point : Formattable, Equatable {
    int x
    int y
}

void main() {
    var a = Point { x = 3, y = 4 }
    var b = Point { x = 3, y = 4 }
    print(a)
    print(a == b)
    print(a != Point { x = 0, y = 0 })
}
```

```text
Point { x = 3, y = 4 }
true
true
```

The compiler writes both methods from the fields: `print` writes the value the way it is built, and `==`
compares `x` and `y`.

### Copied, or cloned

```sg
struct Size : Copyable, Formattable {
    int width
    int height
}

struct Order : Clonable, Formattable {
    string customer
    List<string> items
}

void main() {
    var small = Size { width = 2, height = 3 }
    var big = small
    big.width = 20
    print($"{small} {big}")

    var first = Order { customer = "Ada", items = List<string> { "tea" } }
    var second = first.clone()
    second.items.add("cups")
    print(first)
    print(second)
}
```

```text
Size { width = 2, height = 3 } Size { width = 20, height = 3 }
Order { customer = "Ada", items = ["tea"] }
Order { customer = "Ada", items = ["tea", "cups"] }
```

`var big = small` copies a `Size`, so both can be used. An `Order` holds a text and a list, so it cannot be
`Copyable`; `.clone()` makes a second order with its own text and its own list, and the line shows where the
allocation is.

### Methods of your own

```sg
struct Time : Copyable, Formattable, Equatable, Hashable, Comparable {
    int hour
    int minute

    int compare(read this, read Time other) {
        if this.hour != other.hour {
            return this.hour.compare(other.hour)
        }
        return this.minute.compare(other.minute)
    }

    void format(read this, ref Formatter f) {
        f.write($"{this.hour:d2}:{this.minute:d2}")
    }
}

void main() {
    var trains = List<Time> { Time { hour = 14, minute = 5 }, Time { hour = 9, minute = 30 }, Time { hour = 9, minute = 5 } }
    trains.sort()
    print(trains)
    print(trains[0] < trains[1])

    var seen = HashSet<Time>()
    for t in read trains {
        _ = seen.add(t)
    }
    print(seen.contains(Time { hour = 9, minute = 30 }))
    print(seen.contains(Time { hour = 9, minute = 31 }))
}
```

```text
[09:05, 09:30, 14:05]
true
true
false
```

`compare` gives a negative number when this time comes first and 0 when the two are equal; `sort` and `<` call
it. `format` writes the time the way people read it, in the list too. `==` and the hash come from the fields.
`this.hour.compare(other.hour)` is used in place of a subtraction, which could overflow for numbers far apart.

### Equal in your own way, and an enum in the order of its numbers

```sg
struct Tag : Equatable, Formattable {
    string name

    bool equals(read this, read Tag other) {
        return this.name.toLower() == other.name.toLower()
    }
}

enum Level : byte, Comparable, Formattable {
    Debug = 10
    Error = 40
    Info = 20
}

void main() {
    var tags = List<Tag> { Tag { name = "Draft" }, Tag { name = "urgent" } }
    print(tags.contains(read Tag { name = "URGENT" }))
    print(tags.indexOf(read Tag { name = "draft" }) ?? -1)

    var levels = List<Level> { Level.Error, Level.Debug, Level.Info }
    levels.sort()
    print(levels)
    print(Level.Info < Level.Error)
}
```

```text
true
0
[Debug, Info, Error]
true
```

`Tag` writes its own `equals`, which does not care about case; `==`, `contains` and `indexOf` all use it.
`Level` writes no `compare`, so it is put in the order of its numbers, not of its lines.

### When a value ends: `Disposable` and `Closable`

```sg
struct Lease : Disposable {
    string room

    void dispose(ref this) {
        print($"{this.room} is free again")
    }
}

struct Report : Closable {
    List<string> lines

    Result<void, string> close(this) {
        if this.lines.length() == 0 {
            return Error("the report is empty")
        }
        print($"sent {this.lines.length()} lines")
        return Ok()
    }
}

Result<void, string> main() {
    var lease = Lease { room = "B12" }
    var report = Report { lines = List<string>() }
    report.lines.add($"meeting in {lease.room}".toString())
    report.lines.add("bring the slides")
    try report.close()
    print("done")
    return Ok()
}
```

```text
sent 2 lines
done
B12 is free again
```

The program closes the report itself, and `try` hands on the error closing may give. Nothing calls `dispose`:
it runs when `lease` ends, at the end of `main`.

## What the compiler refuses

### An interface of your own

```sg error
interface Shape {
    double area(read this)
}

struct Square : Shape {
    double side
}

void main() {
    print(Square { side = 2.0 }.side)
}
```

```text
error: expected a type name, found `interface`
  `interface` is not in the interpreter yet
  --> example.sg:1:1
 1 | interface Shape {
   | ^^^^^^^^^
```

Interfaces of your own were put off with generics, so `interface` starts nothing yet. A method that every
kind of shape has is written in each type, or the shapes are the variants of one enum, whose method looks at
each with `match`.

### A name that is no interface

```sg error
struct Square : Printable {
    double side
}

void main() {
    print(Square { side = 2.0 }.side)
}
```

```text
error: there is no interface named `Printable`
  interfaces for now: `Copyable`, `Clonable`, `Formattable`, `Equatable`, `Hashable`, `Comparable`, `Disposable` and `Closable`; your own are not in the interpreter yet
  --> example.sg:1:17
 1 | struct Square : Printable {
   |                 ^^^^^^^^^
```

A name close to one of the eight, such as `Formatable`, gets that one as the hint.

### An interface used as a type

```sg error
void show(read Formattable value) {
    print(value)
}

void main() {
    show(read 3)
}
```

```text
error: there is no type named `Formattable`
  built-in types for now: the integer types, float, double, bool, char, string
  --> example.sg:1:11
 1 | void show(read Formattable value) {
   |           ^^^^^^^^^^^^^^^^
```

An interface says what a type can do; it is not a type a value has. A function takes the type itself.

### A field that does not have the interface

```sg error
struct Track : Copyable {
    int seconds
    List<string> artists
}

void main() {
}
```

```text
error: `Track` cannot be `Copyable`: its field `artists` is a `List<string>`, and a `List<string>` is not `Copyable`
  a `List<string>` owns memory on the heap; a copy would have to allocate, and that is written `.clone()`
  --> example.sg:3:5
 3 |     List<string> artists
   |     ^^^^^^^^^^^^
```

### `Comparable` without `compare`

```sg error
struct Track : Comparable {
    string title
    int seconds
}

void main() {
}
```

```text
error: `Track` is `Comparable`, so it needs a `compare` method
  --> example.sg:1:8
 1 | struct Track : Comparable {
   |        ^^^^^
  hint: `int compare(read this, read Track other)`: negative if this one comes first, 0 if they are equal
```

### `Hashable` without `Equatable`

```sg error
struct Track : Hashable {
    string title
    int seconds
}

void main() {
}
```

```text
error: `Track` is `Hashable`, so it must be `Equatable` too: a key is found by its hash and then compared
  --> example.sg:1:8
 1 | struct Track : Hashable {
   |        ^^^^^
  hint: `Track : Equatable, Hashable`
```

### `==` on a type that did not ask for it

```sg error
struct Track {
    string title
    int seconds
}

void main() {
    var a = Track { title = "Intro", seconds = 60 }
    var b = Track { title = "Intro", seconds = 60 }
    print(a == b)
}
```

```text
error: `==` compares the values, and `Track` is not `Equatable`
  --> example.sg:9:11
 9 |     print(a == b)
   |           ^^^^^^
  hint: add it to the definition: `Track : Equatable`
```

### `Copyable` and `Disposable` together

```sg error
struct Handle : Copyable, Disposable {
    int number

    void dispose(ref this) {
        print("closed")
    }
}

void main() {
}
```

```text
error: `Handle` cannot be both `Copyable` and `Disposable`: each copy would be disposed of
  a copy of a value that gives something back when it ends would give it back twice
  --> example.sg:1:17
 1 | struct Handle : Copyable, Disposable {
   |                 ^^^^^^^^
```
