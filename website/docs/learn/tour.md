---
title: A tour of SG
description: The language in a dozen short programs, each of which runs.
---

# A tour of SG

Each program on this page runs as it is: save it as `tour.sg` and run `sg tour.sg`, or build it with
`sg build tour.sg`. The interpreter and the compiled program print the same.

## Hello

```sg
void main() {
    print("Hello, world")
}
```

`void main()` is where a program starts. A line ends a statement: there are no semicolons.

## Numbers

```sg
void main() {
    var count = 3
    long big = count
    byte small = (byte)count
    print($"{count} {big} {small:x2}")
    print(int.MaxValue.wrappingAdd(1))
}
```

It prints:

```text
3 3 03
-2147483648
```

`var` takes the type of its value. An integer becomes a wider one on its own (`int` to `long`), and only where
every value fits; a narrower one is written with a cast, which stops the program if the value does not fit.
Integer overflow stops a debug build and the interpreter, and wraps in a release build, as in Rust;
`wrappingAdd` wraps everywhere, and `tryAdd` gives `null` instead. `{small:x2}` writes the value in hex.

## Structs, enums and `match`

```sg
struct Point : Copyable, Formattable {
    int x
    int y

    int lengthSquared(read this) {
        return this.x * this.x + this.y * this.y
    }

    void move(ref this, int dx, int dy) {
        this.x += dx
        this.y += dy
    }
}

enum Shape : Formattable {
    Circle(int radius)
    Rect(int width, int height)
    Empty
}

int area(read Shape shape) {
    return match shape {
        Circle(radius) => 3 * radius * radius
        Rect(width, height) => width * height
        Empty => 0
    }
}

void main() {
    var p = Point { x = 3, y = 4 }
    p.move(1, 1)
    print($"{p} {p.lengthSquared()}")
    print(area(read Shape.Rect(2, 5)))
}
```

It prints:

```text
Point { x = 4, y = 5 } 41
10
```

A method says how it uses the value it is called on: `read this` looks at it, `ref this` may change it. An
enum's variants carry values, and `match` must cover every variant: the compiler says which one is missing.
`Copyable` copies a value where another would be moved, and `Formattable` lets `print` write it.

## `null` and errors

```sg
int? find(read List<string> names, read string wanted) {
    for i in 0..names.length() {
        if names[i] == wanted {
            return (int)i
        }
    }
    return null
}

Result<int, string> parsePort(read string text) {
    var port = int.parse(text) ?? return Error($"`{text}` is not a number".toString())
    if port < 1 || port > 65535 {
        return Error($"{port} is not a port".toString())
    }
    return Ok(port)
}

Result<void, string> run() {
    var port = try parsePort("8080")
    print($"listening on {port}")
    _ = try parsePort("eighty")
    return Ok()
}

void main() {
    var names = List<string> { "ada".clone(), "grace".clone() }
    var at = find(read names, "grace")
    if at is not null {
        print($"found at {at}")
    }
    print(find(read names, "alan") ?? -1)
    match run() {
        Ok => print("done")
        Error(why) => print(why)
    }
}
```

It prints:

```text
found at 1
-1
listening on 8080
`eighty` is not a number
```

A value that may be missing is a `T?`, and the compiler makes the program look before it uses one: after
`if at is not null`, `at` is an `int`. `??` gives a value for the missing case. An error is a value too:
`Result<T, E>` holds either a value (`Ok`) or an error (`Error`); `try` passes the error on to the caller, and
`??` can `return` early.

## Owning and borrowing

Every value has one owner. Giving it to another variable moves it, and the old name can no longer be used:

```sg error
void show(read string text) {
    print(text)
}

void main() {
    var name = "Pasha".clone()
    show(read name)
    var moved = name
    print(name)
}
```

```text
error: `name` was moved and can no longer be used
  --> tour.sg:9:11
 9 |     print(name)
   |           ^^^^
  it was moved here:
  --> tour.sg:8:17
 8 |     var moved = name
   |                 ^^^^
```

`read` lends a value to look at and `ref` lends it to change, for as long as the call or the block that
borrows it; the compiler checks that nothing else changes or frees it meanwhile. There is no garbage
collector: a value is freed when its owner ends.

```sg
void addAll(ref List<int> into, read List<int> from) {
    for item in from {
        into.add(item)
    }
}

void main() {
    var all = List<int> { 1, 2 }
    var more = List<int> { 3, 4 }
    addAll(ref all, read more)
    print(all)
    var ages = HashMap<string, int>()
    ages.add("ada".clone(), 36)
    print(ages.get("ada") ?? 0)
}
```

It prints:

```text
[1, 2, 3, 4]
36
```

## Tasks and channels

```sg
long sum(long from, long to) {
    long total = 0
    for i in from..to {
        total += i
    }
    return total
}

void main() {
    var first = Task.run(sum, 0, 1000)
    var second = Task.run(sum, 1000, 2000)
    print(first.wait() + second.wait())

    var jobs = Channel<int>(4)
    var sender = jobs.sender()
    _ = sender.send(7)
    _ = sender.send(8)
    print($"{jobs.receive() ?? 0} {jobs.receive() ?? 0}")
}
```

It prints:

```text
1999000
7 8
```

`Task.run` starts a function as a task, and `wait` gives what it returned. A task is always waited for, and
what goes to another task owns all it holds, so tasks share nothing by accident. A channel's items go from its
senders to its receivers; it ends once every sender has ended.

## Tests

```sg test
int twice(int value) {
    return value * 2
}

test "twice doubles" {
    assert(twice(21) == 42)
}

void main() {
    print(twice(21))
}
```

`sg test tour.sg` runs the tests, and `sg test tour.sg --compiled` runs them compiled:

```text
ok    twice doubles
1 test(s) passed
```

## Where next

[SG for Rust programmers](./for-rust.md) puts each idea of Rust beside the way SG writes it. The language
reference and the library are written out with the preview; until then, the repository has them in full:
[the syntax](https://github.com/selcukgural/sg-lang/blob/main/design/02-syntax-draft.md), [the decisions and the rules
as built](https://github.com/selcukgural/sg-lang/blob/main/design/04-decisions-and-open-questions.md) and [the
examples](https://github.com/selcukgural/sg-lang/tree/main/interpreter/examples).
