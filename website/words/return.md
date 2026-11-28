---
name: return
kind: keyword
summary: Ends the function, with its value.
see: void, try, match, while, read
learn: ../docs/learn/book/04-functions.md#declaring-and-returning
---

## Forms

| Form | What it is |
|---|---|
| `return value` | ends the function and gives the value to the caller |
| `return` | ends a `void` function |
| `return Ok(value)`, `return Error(e)`, `return Ok()` | ends a function that returns a `Result` |
| `return null` | ends a function that returns a `T?`, with no value |
| `return x`, with `read T` as the return type | gives a borrow of what a parameter holds |
| `x ?? return value` | the value of `x`, or a `return` |
| `P => return value` | an arm of a `match` that leaves the function |

## What it means

`return` ends the function at once, wherever it stands: in a loop, in an `if`, in an arm of a `match`. The value
after it must have the type the function returns. A `void` function ends at its last line, or earlier with a
bare `return`, and a `return` with a value there is an error.

A function that returns a value must reach a `return` on every path, and the compiler checks it. A function
that ends with a loop no `break` leaves (`while true`) needs no `return` after it.

The value is moved to the caller, not copied: a list built in the function goes to the caller as it is. What else
the function owns is let go on the way out.

A function can give back a borrow, written `read T` or `ref T` in its return type. The borrow must come from a
parameter, never from a variable of the function, which is gone once it returns. When the function borrows more
than one parameter, `from` after them says which the result can come from.

`return` is a statement. Inside an expression it is written only on the right of `??` and as an arm of a `match`;
an arm of `?:` cannot leave the function.

`main` returns nothing, `void main()`, or a `Result<void, E>`: an error it returns is printed after `error: `
and the program exits with code 1. `exit(code)` ends the program with any other code.

## Examples

### A value, and leaving early

```sg
int square(int x) {
    return x * x
}

void greet(read string name) {
    if name.length() == 0 {
        return
    }
    print($"hello, {name}")
}

void main() {
    print(square(7))
    greet("")
    greet("Pasha")
}
```

```text
49
hello, Pasha
```

The bare `return` ends `greet` before it prints, so the empty name prints nothing.

### From inside a loop

```sg
string? firstLong(read List<string> words) {
    for w in read words {
        if w.length() > 4 {
            return w.clone()
        }
    }
    return null
}

void main() {
    var animals = List<string> { "cat", "lion", "tiger" }
    print(firstLong(read animals) ?? "none")
    var short = List<string> { "ox" }
    print(firstLong(read short) ?? "none")
}
```

```text
tiger
none
```

The `return` in the loop ends the function at the first long word. When the loop ends without one, the
`return null` below it runs.

### The value of a `match`

```sg
string grade(int score) {
    return match score {
        90..=100 => "A"
        75..=89 => "B"
        _ => "C"
    }
}

void main() {
    print(grade(95))
    print(grade(80))
    print(grade(12))
}
```

```text
A
B
C
```

The whole `match` is the value returned.

### Leaving in the middle of a line

```sg
int discount(read string code) {
    var percent = match code {
        "SPRING" => 10
        "VIP" => 25
        _ => return 0
    }
    print($"{code}: {percent}%")
    return percent
}

int portOf(read string text) {
    var port = int.parse(text) ?? return 80
    return port
}

void main() {
    print(discount("VIP"))
    print(discount("FREE"))
    print(portOf("8080"))
    print(portOf("http"))
}
```

```text
VIP: 25%
25
0
8080
80
```

An unknown code takes the last arm, which returns 0 and prints nothing. `?? return 80` gives the number when the
text is one, and returns 80 when it is not. These are the two places a `return` goes inside a line.

### Giving the value to the caller

```sg
List<int> evens(int count) {
    var result = List<int>()
    var n = 0
    while result.length() < count {
        result.add(n)
        n += 2
    }
    return result
}

void main() {
    var xs = evens(4)
    xs.add(100)
    print(xs)
}
```

```text
[0, 2, 4, 6, 100]
```

`return result` moves the list to the caller: nothing is copied, and `xs` owns it from then on.

### A borrow

```sg
read string longest(read List<string> names) {
    nint best = 0
    for i in 0..names.length() {
        if names[i].length() > names[best].length() {
            best = i
        }
    }
    return names[best]
}

read string longer(read string a, read string b) from a, b {
    if a.length() >= b.length() {
        return a
    }
    return b
}

void main() {
    var names = List<string> { "Ada", "Grace", "Linus" }
    print(longest(read names))
    print(longer("Ada", "Grace"))
}
```

```text
Grace
Grace
```

`longest` gives back a borrow of an item of the list, not a copy of the text. `longer` borrows two parameters, so
`from a, b` says the result can come from either; the caller keeps both lent while it uses the result.

### From `main`

```sg file title="check/main.sg"
Result<void, string> main(List<string> args) {
    if args.length() == 0 {
        return Error("give a file name".clone())
    }
    print($"checking {args[0]}")
    return Ok()
}
```

```text
$ cd check
$ sg main.sg; echo "exit code $?"
error: give a file name
exit code 1
$ sg main.sg notes.txt; echo "exit code $?"
checking notes.txt
exit code 0
```

The error `main` returns is printed, and the program ends with code 1. `return Ok()` ends it with code 0.

## What the compiler refuses

### A path with no `return`

```sg error
string size(int n) {
    if n < 10 {
        return "small"
    } else if n < 100 {
        return "medium"
    }
}

void main() {
    print(size(5))
}
```

```text
error: `size` does not return a value on every path; it must return `string`
  --> example.sg:1:8
 1 | string size(int n) {
   |        ^^^^
```

A number of 100 or more reaches the end with nothing to give. An `else { return "large" }` makes it whole.

### A line after `return`

```sg error
int f() {
    return 1
    print("never")
}

void main() {
    print(f())
}
```

```text
error: this line is never run: the `return` before it leaves the function
  --> example.sg:3:5
 3 |     print("never")
   |     ^^^^^^^^^^^^^^
  hint: take it away, with the lines after it in the block
```

There are no warnings, so a line that can never run is an error, as an arm of a `match` that never runs is. The
same holds after `break`, `continue`, `panic(...)` and `exit(...)`, after an `if` whose branches all leave, and
after a `while true` that no `break` leaves.

### A bare `return` where a value is due

```sg error
Result<void, string> save() {
    return
}

void main() {
    _ = save()
}
```

```text
error: `save` must return a `Result<void, string>` value
  --> example.sg:2:5
 2 |     return
   |     ^^^^^^
  hint: success is written too: `return Ok()`
```

### A value from a `void` function

```sg error
void greet(read string name) {
    print(name)
    return 0
}

void main() {
    greet("Ada")
}
```

```text
error: `greet` is void and cannot return a value
  --> example.sg:3:12
 3 |     return 0
   |            ^
```

### A borrow of a variable of the function

```sg error
read string made() {
    var text = "Pasha".clone()
    return read text
}

void main() {
    print(made())
}
```

```text
error: this returns a borrow of `text`, and `text` is gone when the function returns
  --> example.sg:3:12
 3 |     return read text
   |            ^^^^^^^^^
  hint: return the value itself instead of a borrow of it
```

Written `string made()` with `return text`, the function gives the text itself to the caller. Written
`return text` in `read string made()`, the message says the same before anything is lent:

```sg error
read string made() {
    var text = "Pasha".clone()
    return text
}

void main() {
    print(made())
}
```

```text
error: expected `read string`, found `string`
  `text` is a variable of the function, gone when it returns: a borrow of it would outlive it
  --> example.sg:3:12
 3 |     return text
   |            ^^^^
  hint: return the value itself: `string made(...)`
```

### A borrow of two parameters, with no `from`

```sg error
read string longer(read string a, read string b) {
    if a.length() >= b.length() {
        return a
    }
    return b
}

void main() {
    print(longer("Ada", "Grace"))
}
```

```text
error: `longer` returns a borrow and borrows 2 parameters, so it says which of them the result comes from
  --> example.sg:1:13
 1 | read string longer(read string a, read string b) {
   |             ^^^^^^
  hint: after the parameters: `from a, b`, or only the ones it can come from
```

### `return` in an arm of `?:`

```sg error
int pick(bool b) {
    var x = b ? 1 : return 0
    return x
}

void main() {
    print(pick(true))
}
```

```text
error: an arm of `?:` gives a value; it cannot leave the function
  --> example.sg:2:21
 2 |     var x = b ? 1 : return 0
   |                     ^^^^^^
  hint: leave with an `if`, or with `?? return` on a value that may be `null`
```

### `return` inside another expression

```sg error
void show(int n) {
    print(n)
}

int f(int n) {
    show(return 2)
    return n
}

void main() {
    print(f(1))
}
```

```text
error: expected an expression, found `return`
  inside an expression `return` is written only on the right side of `??` or as an arm of a `match`
  --> example.sg:6:10
 6 |     show(return 2)
   |          ^^^^^^
```

### A `main` that returns a number

```sg error
int main() {
    return 3
}
```

```text
error: `main` is written `void main()` or `Result<void, E> main()`
  to see the command line: `void main(List<string> args)`; to await, `async` in front
  --> example.sg:1:5
 1 | int main() {
   |     ^^^^
```

The code a program ends with is given by `exit(3)`, or by a `Result` that `main` returns.
