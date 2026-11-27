---
name: break
kind: keyword
summary: Leaves the loop it is in, or the loop it names, at once.
see: continue, for, while, match, return
learn: ../docs/learn/book/08-collections.md#loops-with-names-and-counting-down
---

## Forms

| Form | What it is |
|---|---|
| `break` | leaves the loop it is in |
| `break name` | leaves the loop named `name`, and every loop inside it |
| `name: for ...`, `name: while ...` | a loop with a name, for `break name` |
| `pattern => break` | an arm of a `match` or a `select` that leaves the loop around it |

## What it means

`break` stops a loop at once: the rest of the round is not run, and the program goes on after the loop. It
takes no value. Inside a loop within a loop, it leaves only the inner one. To leave
an outer loop, give it a name, `search: for ...`, and write `break search`; every loop inside it is left too.

A `break` belongs to the loops of its own function. A `match` is not a loop, so a `break` in an arm leaves the
loop around the `match`; so does a `break` in an arm of a `select`. A `break` may stand where a value is
expected, in an arm of a `match` that gives a value: when it runs, there is no value, and the loop is left.

Leaving early lets go of what the round owns. A value made in the round and not yet given away is freed at the
`break`, as at the end of the block; compiled code frees it there too. A value given away just before a `break`
is fine, since no next round will look for it. A file still open, or a task not yet waited for, is an error at
the `break`, as it would be at the end of its block.

After a loop, the compiler knows only what holds on every way out. A `while x is null` without a `break` ends
only when `x` holds a value; with a `break` that leaves it, `x` may still be `null` after the loop. A `break` of
an inner loop does not count. In the same way, a function that ends with a `while true` needs no `return` after
it only when no `break` leaves that loop.

## Examples

### Stopping at the first find

```sg
void main() {
    var temps = List<int> { 18, 21, 25, 31, 28, 33 }
    int? firstHot = null
    for t in read temps {
        if t > 30 {
            firstHot = t
            break
        }
    }
    if firstHot is null {
        print("no hot day")
    } else {
        print($"the first hot day: {firstHot}")
    }
}
```

```text
the first hot day: 31
```

The loop stops at 31, and 33 is never looked at. The loop also ends when no day is hot, so the code after it
tests which way it ended.

### Leaving the middle one of three loops

```sg
void main() {
    var teams = List<string> { "red", "blue" }
    var scores = List<List<List<int>>> {
        List<List<int>> { List<int> { 3, 4 }, List<int> { 0, 5 }, List<int> { 6, 1 } },
        List<List<int>> { List<int> { 2, 2 }, List<int> { 1, 9 } }
    }
    for t in 0..teams.length() {
        var total = 0
        rounds: for round in read scores[t] {
            for points in read round {
                if points == 0 {
                    break rounds
                }
                total += points
            }
        }
        print($"{teams[t]}: {total}")
    }
}
```

```text
red: 7
blue: 14
```

There are three loops, and the name is on the middle one. `break rounds` leaves the loop over the points and the
loop over the rounds, but the loop over the teams goes on with `blue`. A plain `break` would have left only the
points of one round.

### `break` in an arm of a `match`

```sg
enum Step {
    Add(int amount)
    Stop
}

void main() {
    var steps = List<Step> { Step.Add(3), Step.Add(4), Step.Stop, Step.Add(100) }
    var total = 0
    for step in read steps {
        match step {
            Add(n) => total += n
            Stop => break
        }
        print($"so far {total}")
    }
    print($"in all {total}")
}
```

```text
so far 3
so far 7
in all 7
```

`Stop => break` leaves the `for`, not only the `match`: `"so far"` is not printed for it, and `Add(100)` is never
reached.

### `break` where a value goes

```sg
void main() {
    var readings = List<int> { 5, 8, -1, 7 }
    var total = 0
    for r in read readings {
        total += match r {
            -1 => break
            var value => value
        }
    }
    print(total)
}
```

```text
13
```

The `match` gives a number to add, except for `-1`, where it gives none and leaves the loop. Nothing is added
for it, and `7` is not read.

### What a round owns is let go

```sg
void main() {
    var words = List<string> { "alpha", "", "beta", "stop", "gamma" }
    var shouted = List<string>()
    for word in read words {
        var loud = word.toUpper()
        if word.length() == 0 {
            continue
        }
        if word == "stop" {
            break
        }
        shouted.add(loud)
    }
    print(shouted)
}
```

```text
["ALPHA", "BETA"]
```

`loud` is a new text each round. At the `continue` and at the `break` it has not been given to the list, so it
is freed there, in the interpreter and in compiled code alike.

### A value given away, then `break`

```sg
void keep(List<string> names) {
    print($"kept {names.length()}: {names}")
}

void main() {
    var batch = List<string>()
    var words = List<string> { "one", "two", "three", "four" }
    for word in read words {
        batch.add(word.clone())
        if batch.length() == 3 {
            keep(batch)
            break
        }
    }
}
```

```text
kept 3: ["one", "two", "three"]
```

`keep` takes the list, so `batch` is gone. That would be an error if the loop could go on, since the next round
adds to `batch`; the `break` right after means no round does.

### Narrowing kept, narrowing lost

```sg
void main() {
    var rows = List<List<string>> {
        List<string> { "", "" },
        List<string> { "", "kitten", "cat" }
    }
    nint r = 0
    string? found = null
    while found is null {
        for word in read rows[r] {
            if word.length() > 0 {
                found = word.clone()
                break
            }
        }
        r += 1
    }
    print($"{found} in row {r - 1}, {found.length()} letters")
}
```

```text
kitten in row 1, 6 letters
```

The `break` leaves only the inner `for`. The `while` still ends only when its test fails, so after it `found` is a
`string`, and `found.length()` is allowed. Had the `break` left the `while`, `found` would need a test of its own
after the loop.

### A `break` of an inner loop, at the end of a function

```sg
int firstPair(read List<int> xs, int sum) {
    nint i = 0
    while true {
        nint j = i + 1
        while true {
            if j >= xs.length() {
                break
            }
            if xs[i] + xs[j] == sum {
                return (int)i
            }
            j += 1
        }
        i += 1
    }
}

void main() {
    var xs = List<int> { 1, 4, 6 }
    print(firstPair(read xs, 10))
}
```

```text
1
```

The `break` leaves the inner loop only, so nothing leaves the outer `while true` but `return`. The function needs
no `return` after it.

## What the compiler refuses

### `break` with a value

```sg error
void main() {
    for i in 0..3 {
        break 5
    }
}
```

```text
error: expected end of line, found `5`
  --> example.sg:3:15
 3 |         break 5
   |               ^
```

`break` gives nothing. To bring a value out of a loop, set a variable before the `break`, or put the loop in a
function and `return` the value.

### `break` in a function a loop calls

```sg error
void work(int n) {
    if n == 2 {
        break
    }
    print(n)
}

void main() {
    for i in 0..4 {
        work(i)
    }
}
```

```text
error: `break` can only be written inside a loop
  --> example.sg:3:9
 3 |         break
   |         ^^^^^
```

A `break` sees only the loops of its own function. `work` can `return`, or return a `bool` that the loop tests.

### `break` in a `select` with no loop around it

```sg error
void numbers(Sender<int> out) {
    _ = out.send(1)
}

void main() {
    var ints = Channel<int>(4)
    var a = Task.run(numbers, ints.sender())
    select {
        ints.receive() as n => print(n)
        ended => break
    }
    a.wait()
}
```

```text
error: `break` can only be written inside a loop
  --> example.sg:10:18
 10 |         ended => break
    |                  ^^^^^
```

A `break` in a `select` leaves the loop around it, never the `select` itself.

### A name no loop has

```sg error
void main() {
    for i in 0..3 {
        print(i)
        break outer
    }
}
```

```text
error: no loop around this `break` is named `outer`
  --> example.sg:4:15
 4 |         break outer
   |               ^^^^^
  hint: name the loop it leaves: `outer: for ...`
```

### The name of a loop around it, again

```sg error
void main() {
    outer: for i in 0..3 {
        outer: for j in 0..3 {
            break outer
        }
        print(i)
    }
}
```

```text
error: a loop around this one is already named `outer`
  --> example.sg:3:9
 3 |         outer: for j in 0..3 {
   |         ^^^^^
  it is named here:
```

`break outer` would not say which loop it leaves.

### A `while true` that `break name` leaves, at the end of a function

```sg error
int firstPair(read List<int> xs, int sum) {
    nint i = 0
    search: while true {
        nint j = i + 1
        while true {
            if j >= xs.length() {
                break
            }
            if xs[i] + xs[j] == sum {
                break search
            }
            j += 1
        }
        i += 1
    }
}

void main() {
    var xs = List<int> { 1, 4, 6 }
    print(firstPair(read xs, 10))
}
```

```text
error: `firstPair` does not return a value on every path; it must return `int`
  --> example.sg:1:5
 1 | int firstPair(read List<int> xs, int sum) {
   |     ^^^^^^^^^
```

`break search` leaves the outer loop from inside the inner one, and reaches the end of the function with nothing
to return.

### A `break` hidden in a value

```sg error
int sumUntilZero(read List<int> xs) {
    var total = 0
    nint i = 0
    while true {
        total += match xs[i] {
            0 => break
            var n => n
        }
        i += 1
    }
}

void main() {
    var xs = List<int> { 1, 2, 0 }
    print(sumUntilZero(read xs))
}
```

```text
error: `sumUntilZero` does not return a value on every path; it must return `int`
  --> example.sg:1:5
 1 | int sumUntilZero(read List<int> xs) {
   |     ^^^^^^^^^^^^
```

The compiler finds a `break` wherever it stands, here in an arm of a `match`. `return total` after the loop makes
it whole.

### A variable a `break` leaves maybe `null`

```sg error
void main() {
    var words = List<string> { "", "", "first" }
    nint i = 0
    string? found = null
    while found is null {
        if i == words.length() {
            break
        }
        if words[i].length() > 0 {
            found = words[i].clone()
        }
        i += 1
    }
    print(found.length())
}
```

```text
error: this is `string?` and may be `null`; `.length()` needs a value
  --> example.sg:14:11
 14 |     print(found.length())
    |           ^^^^^
  hint: test it first: `if found is not null { ... }`
```

The `break` leaves the loop while `found` is still `null`, so after the loop it is a `string?`. An
`if found is null { ... } else { ... }` after the loop tests it again.

### The names of `is not`, with a `break`

```sg error
void main() {
    var lines = List<string> { "a", "b", "7" }
    nint i = 0
    while int.parse(read lines[i]) is not Ok(value) {
        i += 1
        if i == lines.length() {
            break
        }
    }
    print(value)
}
```

```text
error: nothing here can use the names in this pattern
  after a `while`, the names of `is not` are seen only when the test is what ends the loop
  --> example.sg:4:43
 4 |     while int.parse(read lines[i]) is not Ok(value) {
   |                                           ^^^^^^^^^
  this `break` leaves the loop while the test may still hold:
```

Without the `break`, the loop could end only once `parse` gave `Ok`, and `value` would be seen after it.

### A file not closed at the `break`

```sg error
Result<void, IoError> main() {
    var names = List<string> { "a.txt", "b.txt", "c.txt" }
    for name in read names {
        var out = try File.createText(read name)
        if name == "b.txt" {
            break
        }
        try out.write("hello\n")
        try out.close()
    }
    return Ok()
}
```

```text
error: `out` is not closed at this `break`, so an error writing its buffer would be lost
  --> example.sg:6:13
 6 |             break
   |             ^^^^^
  it is opened here:
```

Closing can fail, and that error must be seen. `try out.close()` before the `break` closes it.
