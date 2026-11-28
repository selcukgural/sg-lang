---
title: Null and errors
description: Values that may be missing, errors as values with Result and try, and the panics that stop a program.
sidebar_position: 6
---

# `null` and errors

SG has no value that every type can hold by accident. An `int` is always a number and a `string` is always a
text. A value that may be missing says so in its type, `T?`, and an operation that may fail says so in its
return type, `Result<T, E>`. The compiler then makes the program deal with the missing value and the error
before it goes on. Only a fault that depends on the data and could not be seen when the program was compiled
stops it, with a panic.

## `T?` and `null`

```sg
struct User {
    string name
    string? nickname
}

void main() {
    int? age = null
    print(age is null)
    age = 36
    if age is not null {
        print(age + 1)
    }

    var ada = User { name = "Ada", nickname = null }
    var grace = User { name = "Grace", nickname = "Amazing Grace" }
    print(ada.nickname ?? ada.name)
    print(grace.nickname ?? grace.name)
}
```

It prints:

```text
true
37
Ada
Amazing Grace
```

`T?` is a `T` or `null`, and `null` is written only where a `T?` goes. `x is null` and `x is not null` test it.
`a == b` compares two `T?` values, or a `T?` and a `T`: they are equal when both are `null` or both hold equal
values.
`a ?? b` gives `a` when it holds a value and `b` when it is `null`. A `Node*?` takes no more room than a
pointer; an `int?` is the number and a flag beside it.

A `T?` cannot be used as a `T` before it has been tested:

```sg error
void main() {
    string? name = "Ada"
    print(name.length())
}
```

```text
error: this is `string?` and may be `null`; `.length()` needs a value
  --> example.sg:3:11
 3 |     print(name.length())
   |           ^^^^
  hint: test it first: `if name is not null { ... }`
```

## Narrowing

After a test, the compiler knows which case the program is in, and the same name has the narrower type: inside
`if age is not null`, `age` is an `int`. This follows the conditions along `&&` and `||`, and past a `return`:

```sg
void greet(read string? name) {
    if name is null || name.length() == 0 {
        print("hello, stranger")
        return
    }
    print($"hello, {name}")
}

void main() {
    string? nobody = null
    string? empty = ""
    string? ada = "Ada"
    greet(read nobody)
    greet(read empty)
    greet(read ada)
    if ada is not null && ada.length() > 2 {
        print("a long enough name")
    }
}
```

It prints:

```text
hello, stranger
hello, stranger
hello, Ada
a long enough name
```

On the right of `||`, `name` is no longer `null`, or the left side would have been true. After the `if`, which
returns, it is not `null` either. Narrowing also works under `!` and after `while x is null { ... }`. Assigning a
value that is never `null` to a narrowed variable keeps it narrowed; assigning `null` or another `T?` ends it.

Only local variables and parameters are narrowed. A field could be changed between the test and the use (by a
method called in between, for one), so the compiler does not trust a test of it; you borrow the field into a
local and test that:

```sg
struct Address {
    string city
}

struct Person {
    string name
    Address? address
}

void show(read Person person) {
    var address = read person.address
    if address is not null {
        print(address.city)
    }
}

void main() {
    var ada = Person { name = "Ada", address = Address { city = "London" } }
    show(read ada)
}
```

It prints:

```text
London
```

## `?.`

`a?.b` is `null` when `a` is, and `a.b` otherwise; the rest of a chain does not run once a part is `null`. It
works for fields and for methods (`a?.m()`), and is often followed by `??`:

```sg
struct Address : Formattable {
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

It prints:

```text
Ada: London
Pasha: unknown
```

## `Result<T, E>`

A function that can fail returns a `Result<T, E>`: either `Ok(value)` or `Error(cause)`. The caller sees both
in the type and cannot take the value without deciding what happens to the error:

```sg
Result<int, string> half(int n) {
    if n % 2 != 0 {
        return Error($"{n} is odd".toString())
    }
    return Ok(n / 2)
}

void main() {
    match half(10) {
        Ok(value) => print(value)
        Error(why) => print(why)
    }
    match half(7) {
        Ok(value) => print(value)
        Error(why) => print(why)
    }
    print(half(7) ?? 0)
    if half(8) is Ok(value) && value > 3 {
        print($"big half: {value}")
    }
}
```

It prints:

```text
5
7 is odd
0
big half: 4
```

`match` and `is` see the two variants, `Ok` and `Error`, and `??` gives a fallback in place of the error. A
`Result` can never be dropped without a word, since the error would be lost:

```sg error
Result<void, string> save() {
    return Ok()
}

void main() {
    save()
}
```

```text
error: this `Result` is not used, so an error would be lost silently
  --> example.sg:6:5
 6 |     save()
   |     ^^^^^^
  hint: pass the error on with `try save()`, or drop it on purpose with `_ = save()`
```

`Result<void, E>` is a function that gives nothing when it works; it ends with `return Ok()`.

## Errors of your own, `try` and `?? return`

The error type is usually an enum of your own, with a variant for each way to fail and the values that say
more about it. `try x` gives the value of `x`, or returns its error from the function at once. When the
error is of another type, `try` puts it in the variant whose only value is of that type:

```sg
enum ConfigError : Formattable {
    Missing(string key)
    BadNumber(NumberError cause)
    OutOfRange(int value)
}

Result<int, ConfigError> parsePort(read string text) {
    var port = try int.parse(text)
    if port < 1 || port > 65535 {
        return Error(ConfigError.OutOfRange(port))
    }
    return Ok(port)
}

Result<int, ConfigError> portOf(read HashMap<string, string> settings) {
    var text = settings.get("port") ?? return Error(ConfigError.Missing("port"))
    return Ok(try parsePort(text))
}

void main() {
    var all = List<HashMap<string, string>> {
        HashMap<string, string> { {"port", "8080"} },
        HashMap<string, string> { {"port", "80x"} },
        HashMap<string, string> { {"port", "99999"} },
        HashMap<string, string>()
    }
    for settings in read all {
        match portOf(read settings) {
            Ok(port) => print($"port {port}")
            Error(error) => print($"error: {error}")
        }
    }
}
```

It prints:

```text
port 8080
error: BadNumber(Invalid(2))
error: OutOfRange(99999)
error: Missing("port")
```

`int.parse` gives a `Result<int, NumberError>`, and `try` turns its error into `ConfigError.BadNumber`, the one
variant that carries a `NumberError`. There is no conversion to write by hand; if no variant carries the type,
or two do, the compiler says so. `Invalid(2)` says where the text stopped being a number.

`settings.get("port")` gives a `read string?`, `null` for a missing key, and `?? return ...` leaves the function
in that case; after it, `text` is a `read string`. The right side of `??` can be a value, a `return` or a
`panic(...)`. `try` works on a `T?` too, in a function that returns a `T?`: it returns `null` when the value is
`null`.

`try` is only written in a function that can pass the error on:

```sg error
Result<int, string> parse() {
    return Ok(1)
}

int twice() {
    return 2 * try parse()
}

void main() {
    print(twice())
}
```

```text
error: `try` returns the error to the caller, but `twice` returns `int`
  --> example.sg:6:16
 6 |     return 2 * try parse()
   |                ^^^^^^^^^^^
  hint: make `twice` return a `Result<..., string>`, or handle the error here with `match` or `??`
```

## `Result<void, E> main()`

`main` can return a `Result` too, so a program can use `try` from the top. If it ends with an error, the error is
printed after `error: ` and the program exits with code 1; the error type must be `Formattable`, and here it
writes its own text. The second `check` fails:

```sg panic
enum CopyError : Formattable {
    TooLarge(long size, long limit)
    Empty

    void format(read this, ref Formatter f) {
        match this {
            TooLarge(size, limit) => f.write($"{size} bytes is more than the limit of {limit}")
            Empty => f.write("there is nothing to copy")
        }
    }
}

Result<void, CopyError> check(long size) {
    if size == 0 {
        return Error(CopyError.Empty)
    }
    if size > 1000 {
        return Error(CopyError.TooLarge(size, 1000))
    }
    return Ok()
}

Result<void, CopyError> main() {
    try check(10)
    print("10 bytes: fine")
    try check(5000)
    print("never printed")
    return Ok()
}
```

```text
10 bytes: fine
error: 5000 bytes is more than the limit of 1000
```

## Panics

Some faults cannot be known when the program is compiled, because they depend on the data: an index that came
from the input, a divisor that is 0. SG checks for them as the program runs, and when one happens it stops the
program with a panic: a message that says what went wrong and where, and the exit code 1. There is no
unwinding and no catching. A panic means the program has a bug, not that the world was unkind; what the world
can do to a program (a missing file, a bad number in the input) comes back as a `Result`.

What can be found before the program runs is a compile error instead, never a panic: an array index known to be
too large, a missing `match` arm, a `null` not tested for. This program compiles, and stops when it runs:

```sg panic
void main() {
    var scores = List<int> { 90, 75, 60 }
    var at = 3
    print(scores[at])
}
```

```text
panic: index 3 is out of range: the length is 3
  --> example.sg:4:11
 4 |     print(scores[at])
   |           ^^^^^^^^^^
  hint: `.get(i)` reads with `null` instead; before a write, compare the index with `.length()`
```

These panic:

- an index outside a list, an array, a slice or a text (`scores.get(at)` gives `null` instead);
- integer overflow, in the interpreter, in tests and in a debug build (`tryAdd` gives `null`, `wrappingAdd` wraps;
  a release build wraps);
- a division by zero, and a cast whose value does not fit (`(int)big`; `big.tryToInt()` gives `null`);
- `map[key]` for a key that is not in the `HashMap`, and `map.add` for one that is;
- `panic("...")`, written by you, and an `assert` that fails.

The checks stay on in a release build, which cannot turn them off. Only integer overflow (and a shift by more
bits than the type has) wraps there instead, as in Rust.

## `panic` and `assert`

`panic(message)` stops the program on purpose, for a case your code says cannot happen. `assert(condition)`
panics when the condition is false, and shows both sides of a comparison:

```sg panic
int percent(int part, int whole) {
    assert(whole > 0)
    return part * 100 / whole
}

void main() {
    print(percent(1, 4))
    print(percent(1, 0))
}
```

```text
25
panic: assert failed: whole > 0
  left:  0
  right: 0
  --> example.sg:2:5
 2 |     assert(whole > 0)
   |     ^^^^^^^^^^^^^^^^^
```

`x ?? panic("...")` is the way to say that a missing value is a bug: `var port = int.parse(text) ?? panic("the
port was checked above")`. Use it where you know more than the compiler does; where the data can really be
wrong, give the caller a `Result`.
