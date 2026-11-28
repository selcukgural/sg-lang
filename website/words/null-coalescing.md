---
name: x ?? y
kind: sign
summary: The value of x, or y when x is null or an error.
see: T?, null, try, return, is, await
learn: ../docs/learn/book/06-null-and-errors.md#t-and-null
---

## Forms

| Form | What it is |
|---|---|
| `x ?? y` | on a `T?`: the value of `x`, or `y` when `x` is `null` |
| `r ?? y` | on a `Result`: the value of `r`, or `y` in place of the error |
| `a ?? b ?? c` | the first of them that holds a value; only the last one is never `null` |
| `x ?? return value` | the value, or a `return` from the function |
| `x ?? panic("...")`, `x ?? exit(code)` | the value, or the program stops |
| `(x ?? 0) + 1`, `(x ?? 0) == 1` | `??` binds more loosely than `+`, `==` and `&&`, so it is put in parentheses |

## What it means

`x ?? y` opens a value that may be missing. When `x` holds a value, that value is the result and `y` is never
computed. When `x` is `null`, the result is `y`. On a `Result` it is the same: the value of `Ok`, or `y` in place
of the error, which is let go.

The result is a `T`, never a `T?`, so `y` must be a `T`: a `T?` on the right needs a `??` of its own, and
`a ?? b ?? c` groups as `a ?? (b ?? c)`. A borrow of a value that can be copied gives the value: `xs.get(i) ??
0` is an `int`. On a `read T?` the result is a `read T`, with nothing taken out. On a `T?` that owns what it
holds, as a `string?` does, `??` takes the value: the variable is moved, and `read x ?? y` only looks.

The right side may also leave instead of giving a value. `?? return ...` returns from the function, and
`panic(...)` and `exit(code)` stop the program; after such a line the name holds a `T`. `return` is the only
statement written there: there is no `?? break` or `?? continue`.

`??` binds more loosely than every operator but `?:`. `n ?? 0 * 10` is `n ?? (0 * 10)`, and a sum or a
comparison of the result is written `(n ?? 0) * 10`. There is no `??=`.

`??` is a test of a flag, or of a pointer for a `T*?`, as an `if` is. In compiled code, an `await` on the right
of `??` is not taken yet: see [`await`](await.md).

## Examples

### A value in place of `null`

```sg
void main() {
    int? age = null
    print(age ?? 0)
    age = 36
    print(age ?? 0)
    string? nickname = null
    print(nickname ?? "no nickname")
    var scores = List<int> { 7, 9 }
    print(scores.get(1) ?? 0)
    print(scores.get(5) ?? 0)
}
```

```text
0
36
no nickname
9
0
```

`scores.get(i)` gives `null` past the end of the list, so the last line prints the fallback.

### In place of an error

```sg
void main() {
    for text in "12 x7 30".splitWhitespace() {
        var n = int.parse(text) ?? -1
        print($"{text} -> {n}")
    }
}
```

```text
12 -> 12
x7 -> -1
30 -> 30
```

`int.parse` gives a `Result<int, NumberError>`. `??` drops the error and gives `-1` in its place.

### The right side runs only when it is needed

```sg
int slowDefault() {
    print("computing the default")
    return 0
}

void main() {
    int? cached = 5
    print(cached ?? slowDefault())
    cached = null
    print(cached ?? slowDefault())
}
```

```text
5
computing the default
0
```

`slowDefault` is called once: the first time, `cached` held a value.

### Several fallbacks

```sg
void show(read string? fromFlag, read string? fromConfig) {
    print(fromFlag ?? fromConfig ?? "default.toml")
}

void main() {
    show("app.toml", "config.toml")
    show(null, "config.toml")
    show(null, null)
}
```

```text
app.toml
config.toml
default.toml
```

The chain takes the first value it finds. Only `"default.toml"`, at the end, is never `null`.

### Parentheses

```sg
void main() {
    int? n = 4
    print(n ?? 0 * 10)
    print((n ?? 0) * 10)
    print((n ?? 0) == 4)
}
```

```text
4
40
true
```

The first line is `n ?? (0 * 10)`, which is `4`.

### Looking without taking

```sg
void main() {
    string? nickname = "Pasha".clone()
    var shown = read nickname ?? "nobody"
    print(shown)
    print(nickname ?? "nobody")
}
```

```text
Pasha
Pasha
```

`nickname` owns its text, and `nickname ?? ...` would take it. `read nickname ?? ...` gives a `read string` that
looks at it, so `nickname` can be used again on the next line.

### Leaving the function

```sg
int doubleFirst(read List<int> numbers) {
    var first = numbers.get(0) ?? return -1
    return first * 2
}

Result<int, string> portOf(read string text) {
    var port = int.parse(text) ?? return Error($"not a port: {text}".toString())
    return Ok(port)
}

void main() {
    print(doubleFirst(read List<int> { 21 }))
    print(doubleFirst(read List<int>()))
    match portOf("80x") {
        Ok(port) => print(port)
        Error(why) => print(why)
    }
}
```

```text
42
-1
not a port: 80x
```

After the line with `?? return`, `first` is an `int` and `port` an `int`: the case where they were missing has
left.

### With `await`

```sg
async int? lookup(read string key) {
    await Task.delay(5)
    return key == "answer" ? 42 : null
}

async void main() {
    print(await lookup("answer") ?? 0)
    print(await lookup("question") ?? 0)
}
```

```text
42
0
```

`await` binds more tightly than `??`: the task is waited for first, and `??` opens what it gave.

## What stops the program

### `?? panic`

```sg panic
void main() {
    var settings = HashMap<string, string> { {"host", "localhost"} }
    print(settings.get("host") ?? panic("no host"))
    print(settings.get("port") ?? panic("no port"))
}
```

```text
localhost
panic: no port
  --> example.sg:4:35
 4 |     print(settings.get("port") ?? panic("no port"))
   |                                   ^^^^^^^^^^^^^^^^
```

`settings.get` gives a `read string?`, `null` for a key that is not there.

## What the compiler refuses

### A value that is never `null`

```sg error
void main() {
    int count = 5
    print(count ?? 0)
}
```

```text
error: this is `int`, which is never `null`, so `??` has nothing to do
  --> example.sg:3:11
 3 |     print(count ?? 0)
   |           ^^^^^
```

### A fallback that may be `null` too

```sg error
void main() {
    int? a = null
    int? b = null
    int? c = a ?? b
    print(c ?? 0)
}
```

```text
error: expected `int`, found `int?`
  --> example.sg:4:19
 4 |     int? c = a ?? b
   |                   ^
  hint: it may be `null`: give a fallback with `b ?? ...`, or test it with `is null`
```

### A `Result` that carries nothing

```sg error
Result<void, string> save() {
    return Ok()
}

void main() {
    var done = save() ?? 0
    print(done)
}
```

```text
error: this is `Result<void, string>`: success carries no value, so `??` has nothing to give
  --> example.sg:6:16
 6 |     var done = save() ?? 0
   |                ^^^^^^
  hint: pass the error on with `try`, or drop it on purpose with `_ = ...`
```

### A value used after `??` took it

```sg error
void main() {
    string? nickname = "Pasha".clone()
    print(nickname ?? "nobody")
    print(nickname ?? "nobody")
}
```

```text
error: `nickname` was moved and can no longer be used
  --> example.sg:4:11
 4 |     print(nickname ?? "nobody")
   |           ^^^^^^^^
  it was moved here:
  --> example.sg:3:11
 3 |     print(nickname ?? "nobody")
   |           ^^^^^^^^
  hint: `??` takes what `nickname` holds; to look at it without taking it: `read nickname ?? ...`
```

`read nickname ?? "nobody"` looks, as in the example above.

### A comparison without parentheses

```sg error
void main() {
    int? n = 4
    print(n ?? 0 == 4)
}
```

```text
error: expected `int`, found `bool`
  `??` binds more loosely than `==`: this reads `n ?? (0 == 4)`
  --> example.sg:3:16
 3 |     print(n ?? 0 == 4)
   |                ^^^^^^
  hint: to compare the value, write `(n ?? 0) == 4`
```

`n == 4` alone compares too: a `T?` is compared with a `T` as a value that is there, and `null` is equal to
nothing but `null`.

### `break` on the right

```sg error
void main() {
    var total = 0
    for text in "1 x 3".splitWhitespace() {
        var n = int.parse(text) ?? break
        total += n
    }
    print(total)
}
```

```text
error: `break` cannot stand on the right of `??`: only `return` leaves from there
  --> example.sg:4:36
 4 |         var n = int.parse(text) ?? break
   |                                    ^^^^^
  hint: test it with `is`: on a `Result`, `if int.parse(text) is not Ok(n) { break }`; on a `T?`, `var n = int.parse(text)`, then `if n is null { break }`
```

`int.parse` gives a `Result`, so the first form of the hint is the one here; after that `if`, `n` is an `int`.

### `??=`

```sg error
void main() {
    int? limit = null
    limit ??= 10
    print(limit ?? 0)
}
```

```text
error: there is no `??=`
  --> example.sg:3:11
 3 |     limit ??= 10
   |           ^^^
  hint: `limit = limit ?? 10`
```
