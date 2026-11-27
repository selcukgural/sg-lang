---
name: false
kind: value
summary: The bool that is not true; what a comparison gives when it does not hold, and the zero value of a bool.
see: bool, true, if, while, match, struct
learn: ../docs/learn/book/02-values.md#bool-and-char
---

## Forms

| Form | What it is |
|---|---|
| `var found = false`, `return false` | the value, kept or given back |
| `bool done = false` in a struct | the default of a field |
| `bool verbose = false` in a parameter | the default of a parameter |
| `bool[30]`, `bool[30](false)` | an array of `false`: alone, it is filled with its zero value |
| `i < n && xs[i] > 0` | when the left side is `false`, the right side never runs |
| `!found`, `!seen.add(x)` | `true` where the value is `false` |
| `false => ...`, `Lamp(false) => ...` | an arm of a `match`, at the top or inside a variant |

## What it means

`false` is one of the two values of a [`bool`](bool.md); [`true`](true.md) is the other. It is what a
comparison gives when it does not hold, and what a test gives when the answer is no: `contains` for an item
that is not there, `add` of a `HashSet` for an item that already was. `!false` is `true`.

`&&` stops at `false`: when its left side is `false`, so is the whole, and the right side is not run. That is
what makes `i < xs.length() && xs[i] > 0` safe. `||` with a `false` on the left gives its right side.

A variable and a field get no value by themselves: a struct literal that leaves out a `bool` field is an
error, unless the field has a default, `bool done = false`. A parameter may have one too. In an array, `false`
is the zero value: `bool[30]` alone holds thirty `false`.

`false` is not `0`: `bool found = 0` is refused, and so is `!` on a number. It is printed as `false`.

## Examples

### A default

```sg
struct Job : Formattable {
    string title
    bool done = false
}

void show(read List<Job> jobs, bool verbose = false) {
    for job in read jobs {
        if verbose || !job.done {
            print($"  {job}")
        }
    }
}

void main() {
    var jobs = List<Job> {
        Job { title = "write" },
        Job { title = "test", done = true },
    }
    print("open:")
    show(read jobs)
    print("all:")
    show(read jobs, true)
}
```

```text
open:
  Job { title = "write", done = false }
all:
  Job { title = "write", done = false }
  Job { title = "test", done = true }
```

`write` is built without `done`, and gets `false`. A call that leaves out `verbose` gives it `false`.

### `&&` stops at `false`

```sg
bool positiveAt(read List<int> items, nint i) {
    return i < items.length() && items[i] > 0
}

void main() {
    var items = List<int> { 4, -2 }
    print(positiveAt(read items, 0))
    print(positiveAt(read items, 1))
    print(positiveAt(read items, 9))
}
```

```text
true
false
false
```

For `9` the left side is `false`, and `items[9]`, which would stop the program, is never read.

### An array of `false`

```sg
void main() {
    var composite = bool[30]
    for i in 2..30 {
        if composite[i] {
            continue
        }
        nint j = i * i
        while j < 30 {
            composite[j] = true
            j += i
        }
    }
    var primes = List<nint>()
    for i in 2..30 {
        if !composite[i] {
            primes.add(i)
        }
    }
    print(primes)
}
```

```text
[2, 3, 5, 7, 11, 13, 17, 19, 23, 29]
```

Every item starts `false`; a number marked `true` has a smaller factor. What is still `false` at the end is a
prime.

### A test that says no

```sg
void main() {
    var seen = HashSet<string>()
    for word in "the cat saw the dog and the cat".splitWhitespace() {
        if !seen.add(word.clone()) {
            print($"again: {word}")
        }
    }
    print($"{seen.length()} words")
    print($"{seen.contains("bird")} {seen.add("bird".clone())} {seen.add("bird".clone())}")
}
```

```text
again: the
again: the
again: cat
5 words
false true false
```

`add` gives `false` for a word that was already there, and `!` turns that into the test.

### A flag that starts `false`

```sg
bool hasNegative(read List<int> numbers) {
    for n in read numbers {
        if n < 0 {
            return true
        }
    }
    return false
}

void main() {
    var found = false
    var lines = List<string> { "ok", "ok", "stop", "ok" }
    nint i = 0
    while !found && i < lines.length() {
        found = lines[i] == "stop"
        i += 1
    }
    print($"found: {found} after {i} lines")
    print(hasNegative(read List<int> { 3, 1 }))
    print(hasNegative(read List<int>()))
}
```

```text
found: true after 3 lines
false
false
```

`return false` after the loop is the answer when nothing was found, an empty list included.

## What the compiler refuses

### A field left out

```sg error
struct Job {
    string title
    bool done
}

void main() {
    var job = Job { title = "write" }
    print(job.done)
}
```

```text
error: `Job` is missing `done`
  every field gets a value; there is no uninitialized memory
  --> example.sg:7:15
 7 |     var job = Job { title = "write" }
   |               ^^^^^^^^^^^^^^^^^^^^^^^
```

`bool done = false` in the struct gives the field its default.

### `0` for `false`

```sg error
void main() {
    bool found = 0
    print(found)
}
```

```text
error: expected `bool`, found `int`
  --> example.sg:2:18
 2 |     bool found = 0
   |                  ^
```

The value is written `false`.

### `!` on a number

```sg error
void main() {
    var items = List<int>()
    if !items.length() {
        print("empty")
    }
}
```

```text
error: expected `bool`, found `nint`
  --> example.sg:3:9
 3 |     if !items.length() {
   |         ^^^^^^^^^^^^^^
  hint: a number is not a condition; compare it: `items.length() != 0`
```

`!` turns a `bool` around and nothing else. An empty list is `items.length() == 0`.
