---
name: try
kind: keyword
summary: Opens a Result: its value, or the error handed to the caller.
see: match, is, null
learn: ../docs/learn/book/06-null-and-errors.md#errors-of-your-own-try-and--return
---

## Forms

| Form | What it is |
|---|---|
| `try f()` | the value of a `Result`, or a `return` of its error |
| `try x` | on a `T?`: the value, or a `return` of `null` |
| `(try f()).field` | a `try` in the middle of a chain |
| `x ?? value` | no `try`: the value, or this one in place of the error |
| `x ?? return ...` | no `try`: the value, or a `return` written out |

## What it means

A function that can fail returns a `Result<T, E>`, which is `Ok(value)` or `Error(cause)`. `try` opens it: on
`Ok` it gives the value, and on `Error` the function returns that error to its caller at once. It is an ordinary
return: what the function owns is let go on the way out, and nothing is unwound.

The function with the `try` must return a `Result` too. When its error type differs, `try` puts the error into
the one variant of that type that carries it. There is no conversion to write by hand; when no variant carries
it, or two do, the compiler says so.

On a `T?`, in a function that returns a `T?`, `try` gives the value or returns `null`.

`try` costs one test of the `Result`, as a C function that returns an error code costs one test of the code.

## Examples

### Pass the error on

```sg
Result<int, NumberError> sum(read string a, read string b) {
    var x = try int.parse(a)
    var y = try int.parse(b)
    return Ok(x + y)
}

void main() {
    print(sum("40", "2") ?? -1)
    match sum("40", "2x") {
        Ok(n) => print(n)
        Error(why) => print(why)
    }
}
```

```text
42
Invalid(1)
```

`int.parse("2x")` fails, so `sum` returns its error and never adds. `Invalid(1)` says the text stopped being a
number at byte 1.

### An error of your own

```sg
enum ConfigError : Formattable {
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

void main() {
    for text in "8080 80x 99999".splitWhitespace() {
        match parsePort(text) {
            Ok(port) => print($"port {port}")
            Error(error) => print($"error: {error}")
        }
    }
}
```

```text
port 8080
error: BadNumber(Invalid(2))
error: OutOfRange(99999)
```

`int.parse` fails with a `NumberError`, and `try` returns it as `ConfigError.BadNumber`, the one variant that
carries a `NumberError`.

### On a value that may be `null`

```sg
read string? secondWord(read string line) {
    var words = line.splitWhitespace()
    _ = try words.next()
    return try words.next()
}

void main() {
    print(secondWord("make it so") ?? "none")
    print(secondWord("alone") ?? "none")
}
```

```text
it
none
```

`words.next()` gives `null` when there are no more words, and `try` then returns `null` from `secondWord`.

### In the middle of a line

```sg
struct Point {
    int x
    int y
}

Result<Point, NumberError> parsePoint(read string text) {
    var parts = text.split(',')
    var x = try int.parse(parts.next() ?? "")
    var y = try int.parse(parts.next() ?? "")
    return Ok(Point { x = x, y = y })
}

Result<int, NumberError> sumOfX(read List<string> texts) {
    var total = 0
    for text in read texts {
        total += (try parsePoint(text)).x
    }
    return Ok(total)
}

void main() {
    var good = List<string> { "1,2", "3,4" }
    var bad = List<string> { "1,2", "x,4" }
    print(sumOfX(read good) ?? -1)
    print(sumOfX(read bad) ?? -1)
}
```

```text
4
-1
```

`try` binds to the whole call after it, so a field of what it opens is reached through brackets.

### From `main`

```sg panic
Result<int, NumberError> port(read string text) {
    return int.parse(text)
}

Result<void, NumberError> main() {
    var a = try port("8080")
    print($"first: {a}")
    var b = try port("80x")
    print($"second: {b}")
    return Ok()
}
```

```text
first: 8080
error: Invalid(2)
```

A `main` that returns a `Result` may use `try` too. An error it ends with is printed after `error: `, and the
program exits with code 1.

## What the compiler refuses

### `try` in a function that cannot pass the error on

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

### An error no variant carries

```sg error
enum AppError {
    Disk(int code)
}

Result<int, AppError> load(read string text) {
    return Ok(try int.parse(text))
}

void main() {
    print(load("1") ?? 0)
}
```

```text
error: `try` cannot turn `NumberError` into `AppError`
  --> example.sg:6:15
 6 |     return Ok(try int.parse(text))
   |               ^^^^^^^^^^^^^^^^^^^
  hint: add a variant to `AppError` that carries it: `Name(NumberError cause)`
```

### An error two variants carry

```sg error
enum AppError {
    First(NumberError cause)
    Second(NumberError cause)
}

Result<int, AppError> load(read string text) {
    return Ok(try int.parse(text))
}

void main() {
    print(load("1") ?? 0)
}
```

```text
error: `try` cannot choose: `First` and `Second` of `AppError` both carry `NumberError`
  --> example.sg:7:15
 7 |     return Ok(try int.parse(text))
   |               ^^^^^^^^^^^^^^^^^^^
  hint: open the `Result` with `match` and build the one you mean
```

### A `Result` left unopened

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
