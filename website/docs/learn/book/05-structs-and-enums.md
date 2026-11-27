---
title: Structs and enums
description: Your own types, the methods on them, the built-in interfaces they take, and the match that looks inside them.
sidebar_position: 5
---

# Structs and enums

A struct puts several values together under one name. An enum is a value that is one of several variants, and each
variant can carry values of its own. Between them they describe almost all the data of a program, and `match`
is how a program looks inside them.

## Structs and their fields

```sg
struct Server : Formattable {
    string host = "localhost"
    int port = 8080
    bool secure
}

void main() {
    var local = Server { secure = false }
    var remote = Server { host = "example.org", port = 443, secure = true }
    print(local)
    print(remote.host)
    remote.port = 8443
    print(remote.port)
}
```

It prints:

```text
Server { host = "localhost", port = 8080, secure = false }
example.org
8443
```

A field is written type first, one per line. A struct is built by naming its fields, `Server { port = 443, ... }`,
in any order. A field with a default value may be left out, and gets the default. A default is a value the
compiler can find on its own when it compiles the program (a number, a text, a const, a call it can run then),
`null`, or an empty collection.

A field without a default must be given. Every field gets a value when the struct is built, so there is never
memory that holds whatever happened to be there:

```sg error
struct Server {
    string host = "localhost"
    int port = 8080
    bool secure
}

void main() {
    var local = Server { port = 80 }
}
```

```text
error: `Server` is missing `secure`
  every field gets a value; there is no uninitialized memory
  --> example.sg:8:17
 8 |     var local = Server { port = 80 }
   |                 ^^^^^^^^^^^^^^^^^^^^
```

## A copy with some fields changed: `with`

```sg
struct Point : Copyable, Formattable {
    int x
    int y
}

struct Label : Formattable {
    string text
    int size
}

void main() {
    var p = Point { x = 1, y = 2 }
    var q = p with { x = 5 }
    print(p)
    print(q)

    var title = Label { text = "Report", size = 12 }
    var big = title with { size = 24 }
    print(big)
}
```

It prints:

```text
Point { x = 1, y = 2 }
Point { x = 5, y = 2 }
Label { text = "Report", size = 24 }
```

`p with { x = 5 }` is a new value, with the fields you write changed and the others as they were. A `Copyable`
value such as `Point` is copied, so `p` is still there. Any other value is moved into the new one: after
`title with { ... }`, `title` can no longer be used, and the compiler says so if you try. Nothing is copied that
you did not ask for.

## Methods

A method is written inside the body of its type. Its first parameter is `this`, and the way it is written says
what the method does with the value it is called on:

```sg
struct Counter : Formattable {
    string name
    int count

    Counter create(string name) {
        return Counter { name = name, count = 0 }
    }

    int value(read this) {
        return this.count
    }

    void add(ref this, int by) {
        this.count += by
    }

    string finish(this) {
        return $"{this.name}: {this.count}".toString()
    }
}

void main() {
    var hits = Counter("hits")
    hits.add(2)
    hits.add(3)
    print(hits.value())
    var report = hits.finish()
    print(report)
}
```

It prints:

```text
5
hits: 5
```

- `read this` looks at the value and changes nothing.
- `ref this` may change it: `this.count += by`.
- `this` alone takes the value itself, so the method owns it and the caller's variable is moved. After
  `hits.finish()`, `hits` is gone. (On a `Copyable` type the method gets a copy instead.)
- A function without `this` belongs to the type and is called on it: `Counter.create("hits")`. A function
  named `create` is the type's constructor, and `Counter("hits")` is short for `Counter.create("hits")`.

Fields are always reached through `this.`, so a field and a local variable never look alike.

The compiler holds callers to these promises. A value lent with `read` cannot be given to a method that takes
`ref this`:

```sg error
struct Counter {
    string name
    int count

    void add(ref this, int by) {
        this.count += by
    }
}

void show(read Counter counter) {
    counter.add(1)
}

void main() {
    var hits = Counter { name = "hits", count = 0 }
    show(read hits)
}
```

```text
error: `add` takes `ref this`, but this value is reached through a `read` borrow
  --> example.sg:11:5
 11 |     counter.add(1)
    |     ^^^^^^^
  hint: take the value as `ref` instead of `read`
```

What `read`, `ref` and moving mean in full is the subject of [Ownership and borrowing](./07-ownership.md).

## Built-in interfaces

A type says what it can do by naming interfaces after a colon: `struct Point : Copyable, Formattable`. Nothing is
there unless you ask for it, so you never pay for a copy, a comparison or a hash you did not want. The
compiler writes the code for each interface from the fields, unless you write a method of your own.

| Interface | What it gives | What it asks for |
|---|---|---|
| `Copyable` | The value is copied where another would be moved | Every field is `Copyable` (numbers, `bool`, `char`, other `Copyable` types) |
| `Clonable` | `.clone()`, a copy that owns its own memory, and `makeUnique` on a `shared` value of it | Every field can be cloned (not a `raw T`, a `Disposable` value or a borrow), or your own `T clone(read this)` |
| `Formattable` | `print`, `$"{value}"` and the writers take it | Every field is `Formattable`, or your own `void format(read this, ref Formatter f)` |
| `Equatable` | `==` and `!=`, `indexOf` and `contains` on a list | Every field is `Equatable`, or your own `bool equals(read this, read T other)` |
| `Hashable` | The type can be a key of a `HashMap` and an item of a `HashSet` | The type is `Equatable` too, and every field is `Hashable` |
| `Comparable` | `<`, `<=`, `>`, `>=`, `sort` and `binarySearch` | Your own `int compare(read this, read T other)`: negative if this comes first, 0 if equal |

A text owns memory on the heap, so copying one means allocating, which SG never does behind your back. A struct
with a `string` field cannot be `Copyable`, and the compiler says why:

```sg error
struct User : Copyable {
    int id
    string name
}

void main() {
}
```

```text
error: `User` cannot be `Copyable`: its field `name` is a `string`, and a `string` is not `Copyable`
  a `string` owns memory on the heap; a copy would have to allocate, and that is written `.clone()`
  --> example.sg:3:5
 3 |     string name
   |     ^^^^^^
```

Here is one type with four of them, writing its own `format` and `compare`, and taking `==` and the hash from
its fields:

```sg
struct Version : Copyable, Formattable, Equatable, Hashable, Comparable {
    int major
    int minor

    int compare(read this, read Version other) {
        if this.major != other.major {
            return this.major.compare(other.major)
        }
        return this.minor.compare(other.minor)
    }

    void format(read this, ref Formatter f) {
        f.write($"v{this.major}.{this.minor}")
    }
}

void main() {
    var old = Version { major = 1, minor = 9 }
    var next = Version { major = 2, minor = 0 }
    print($"{old} < {next}: {old < next}")
    print(old == Version { major = 1, minor = 9 })

    var versions = List<Version> { next, Version { major = 1, minor = 2 }, old }
    versions.sort()
    print(versions)

    var released = HashSet<Version>()
    print(released.add(old))
    print(released.contains(Version { major = 1, minor = 9 }))
}
```

It prints:

```text
v1.9 < v2.0: true
true
[v1.2, v1.9, v2.0]
true
true
```

`this.major.compare(other.major)` is used instead of `this.major - other.major`, which would overflow for numbers
far apart. A fractional number is not `Hashable`, so a struct with a `double` field cannot be either; and for
now a `Hashable` type takes its `equals` and its hash from its fields, not from methods you write.

Two more interfaces, `Disposable` and `Closable`, run code when a value ends: closing a file, giving back what a
library of C made.

## Enums

An enum lists its variants, one per line. A variant may carry values, named and typed like parameters, and the
enum may have methods after them:

```sg
enum Shape : Formattable {
    Circle(int radius)
    Rect(int width, int height)
    Empty

    int area(read this) {
        return match this {
            Circle(r) => 3 * r * r
            Rect(w, h) => w * h
            Empty => 0
        }
    }
}

void main() {
    var shapes = List<Shape> { Shape.Circle(2), Shape.Rect(3, 4), Shape.Empty }
    for shape in read shapes {
        print($"{shape}: {shape.area()}")
    }
}
```

It prints:

```text
Circle(2): 12
Rect(3, 4): 12
Empty: 0
```

A variant is built with the enum's name in front: `Shape.Circle(2)`, `Shape.Empty`. A `Shape` takes the room of
its largest variant plus a tag that says which one it holds, and it lives where the variable is, with no
allocation. This is what other languages do with a class hierarchy, a tagged union or a set of constants.

## Enums with numbers

An enum can stand for numbers, as the values of a file format or a protocol do. It names its integer type first,
and its variants carry nothing:

```sg
enum Block : byte, Copyable, Formattable {
    Extension = 0x21
    Image = 0x2C
    Trailer = 0x3B
}

enum Level : int, Copyable, Comparable, Formattable {
    High = 30
    Low = 10
    Middle
}

string describe(byte code) {
    return match Block.fromValue(code) {
        null => $"no block starts with {code:x2}".toString()
        var block => $"{block} starts with {(byte)block:x2}".toString()
    }
}

void main() {
    print((byte)Block.Image)
    print(describe(0x3B))
    print(describe(0x00))

    print((int)Level.Middle)
    print(Level.Low < Level.High)
    var levels = List<Level> { Level.High, Level.Middle, Level.Low }
    levels.sort()
    print(levels)
}
```

It prints:

```text
44
Trailer starts with 3b
no block starts with 00
11
true
[Low, Middle, High]
```

A variant without a number has one more than the variant above (`Middle` is 11). A cast, `(byte)block`, gives
the number. The other way can fail, so `Block.fromValue(b)` gives a `Block?`: the variant, or `null` for a byte
no variant has. `Comparable` on such an enum orders by the numbers, with no `compare` to write. An enum without
numbers has none to give: `(int)Shape.Empty` is a compile error, since the order of the lines is not a number.

## `match`

`match` looks at a value and runs the first arm whose pattern fits. It must cover every variant, so adding a
variant to an enum shows you every place that has to handle it:

```sg error
enum Shape {
    Circle(int radius)
    Rect(int width, int height)
    Empty
}

int sides(read Shape shape) {
    return match shape {
        Circle(_) => 0
        Rect(_, _) => 4
    }
}

void main() {
    print(sides(read Shape.Empty))
}
```

```text
error: this `match` does not handle `Empty`
  every variant of `Shape` needs an arm
  --> example.sg:8:18
 8 |     return match shape {
   |                  ^^^^^
  hint: add an arm for each, or `_ => ...` for everything else
```

A pattern names each value a variant carries, or skips it with `_`. An arm can add a condition with `when`;
numbers come in ranges that take both ends (`1..=9`); patterns join with `or`; `match` works on integers,
characters, texts and `bool` as well as on enums; and `match` is a value when it is written where a value goes:

```sg
enum Shape {
    Circle(int radius)
    Rect(int width, int height)
    Empty
}

string describe(read Shape shape) {
    return match shape {
        Circle(r) when r > 100 => "a big circle"
        Circle(_) => "a circle"
        Rect(w, h) when w == h => "a square"
        Rect(_, _) => "a rectangle"
        Empty => "nothing"
    }
}

string size(int n) {
    return match n {
        0 => "none"
        1..=9 => "a few"
        10 or 12 => "ten or a dozen"
        _ => "many"
    }
}

void main() {
    print(describe(read Shape.Circle(500)))
    print(describe(read Shape.Rect(3, 3)))
    print(describe(read Shape.Rect(3, 4)))
    print($"{size(0)}, {size(4)}, {size(12)}, {size(99)}")

    var command = "stop"
    match command {
        "go" => print("going")
        "stop" or "halt" => print("stopping")
        _ => print("what?")
    }
}
```

It prints:

```text
a big circle
a square
a rectangle
none, a few, ten or a dozen, many
stopping
```

An arm with `when` does not count as covering its variant, since the condition may be false: without the plain
`Circle(_)` arm, the compiler would say that `Circle` is not handled.

Patterns go inside each other, and a value written in a pattern must be equal for the arm to run. An arm can be
a block of statements:

```sg
enum Color {
    Red
    Green
    Blue
}

enum Command {
    Paint(Color color)
    Move(int dx, int dy)
    Stop
}

void run(read Command command, ref int x) {
    match command {
        Paint(Red) => print("paint it red")
        Paint(_) => print("paint it another colour")
        Move(0, 0) => print("stay")
        Move(dx, _) => {
            x += dx
            print($"now at {x}")
        }
        Stop => print("stop")
    }
}

void main() {
    var x = 0
    var commands = List<Command> {
        Command.Paint(Color.Red),
        Command.Paint(Color.Blue),
        Command.Move(0, 0),
        Command.Move(5, 1),
        Command.Stop
    }
    for command in read commands {
        run(command, ref x)
    }
}
```

It prints:

```text
paint it red
paint it another colour
stay
now at 5
stop
```

An arm can also leave the function with `return`, in a `match` that gives a value too: in
`var percent = match code { ... }`, an arm `_ => return 0` ends the function for a code no other arm takes, and
`percent` is given a value only by the arms that stay.

## `is`: one variant

When only one variant matters, `is` tests for it in a condition. The names it binds are seen in the block, and
along `&&`:

```sg
enum Shape {
    Circle(int radius)
    Rect(int width, int height)
    Empty
}

void main() {
    var shape = Shape.Rect(3, 4)
    if shape is Rect(w, h) {
        print($"{w} by {h}")
    }
    if shape is not Empty {
        print("something is there")
    }
    if shape is Rect(w, _) && w > 2 {
        print("wide enough")
    }
}
```

It prints:

```text
3 by 4
something is there
wide enough
```

`is not` works the other way round: after `if r is not Ok(n) { return }`, the name `n` is seen in the rest of
the block, since the code only gets there when the pattern fit. The next chapter, [`null` and
errors](./06-null-and-errors.md), uses that with `Result`.
