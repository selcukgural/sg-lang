---
name: for
kind: keyword
summary: Goes over the numbers of a range or the items of a collection, one round for each.
see: in, while, break, continue, read, ref
learn: ../docs/learn/book/08-collections.md#for-over-a-collection
---

## Forms

| Form | What it is |
|---|---|
| `for i in a..b` | the numbers from `a` up to `b`, without `b` |
| `for i in (a..b).reversed()` | the same numbers, the last first |
| `for _ in 0..n` | `n` rounds, with no name |
| `for x in read xs` | each item of `xs`, lent to look at |
| `for x in ref xs` | each item of `xs`, lent to change |
| `for x in xs` | `xs` handed over: each item is owned by its round |
| `for e in read map` | each entry of a `HashMap`, with `e.key` and `e.value` |
| `for c in text.chars()` | an iterator: the characters, words or lines of a text |
| `for x in channel` | each item a channel receives, until every sender has ended |
| `name: for ...` | a loop with a name, for `break name` and `continue name` |

## What it means

`for` runs its block once for each number of a range or each item of a collection. It goes over a range
`a..b`, a `List`, an array, a slice, a `HashMap`, a `HashSet`, a `Channel` and the iterators of a text, such as
`chars()` and `lines()`. Between the name and the items stands `in`. There is one name: for the positions too,
go over `0..xs.length()` and read `xs[i]`.

A range goes up to its end and leaves the end out; a range whose end is not above its start gives no round.
Its ends are found once, before the first round. The name of a range is a copy of the count, made at the start
of each round: the body may change it, and that changes only the round it is in. Its type is the type of the
ends.

The items of a collection are taken as a `match` takes a value. `for x in read xs` lends each item to look at,
`for x in ref xs` lends each item to change, and `for x in xs` hands the collection over, so `xs` is gone after
the loop. When `xs` is already a borrow, the items are lent the same way. While the loop runs, `xs` cannot be
changed or moved.

`break` leaves the loop and `continue` goes on with the next round. A loop with a name, `outer: for ...`, can be
left or gone on with from a loop inside it: `break outer`, `continue outer`.

## Examples

### Counting

```sg
void main() {
    long total = 0
    for i in 1..5 {
        total += i * i
    }
    print(total)
    for _ in 0..3 {
        print("again")
    }
    for n in 5..5 {
        print(n)
    }
}
```

```text
30
again
again
again
```

`1..5` is 1, 2, 3 and 4. `_` stands for a name the body does not need. `5..5` has no number in it, so its loop
runs no round.

### The count is a copy for each round

```sg
void main() {
    for i in 0..4 {
        i += 1
        print(i)
    }
}
```

```text
1
2
3
4
```

Adding to `i` changes only the round it is in; the next round starts from the next number of the range.

### The items of a list, lent

```sg
void main() {
    var prices = List<int> { 100, 250, 80 }
    for price in ref prices {
        price = price * 2
    }
    for price in read prices {
        print(price)
    }
    print(prices.length())
}
```

```text
200
500
160
3
```

`ref prices` lends each item to change, and writing to `price` writes into the list. `read prices` only looks.
The list is still there after both loops.

### A list handed over

```sg
void main() {
    var names = List<string> { "ada", "grace" }
    var shouting = List<string>()
    for name in names {
        shouting.add(name.toUpper())
    }
    print(shouting)
}
```

```text
["ADA", "GRACE"]
```

Without `read` or `ref`, the list is handed over to the loop, and each `name` is owned by its round. `names`
cannot be used after the loop.

### Positions and items

```sg
void main() {
    var names = List<string> { "ada", "grace" }
    for i in 0..names.length() {
        names.add($"{names[i]}!".toString())
        print($"{i}: {names[i]}")
    }
    print(names)
}
```

```text
0: ada
1: grace
["ada", "grace", "ada!", "grace!"]
```

A range over the positions borrows nothing, so the list may change inside the loop. The end,
`names.length()`, is found once before the first round, so the items added are not gone over. `i` is a
`nint`, the type of `length()`.

### A map and a set

```sg
void main() {
    var ages = HashMap<string, int> { {"ada", 36}, {"grace", 85} }
    for item in ref ages {
        item.value += 1
    }
    var names = List<string>()
    int total = 0
    for item in read ages {
        names.add(item.key.clone())
        total += item.value
    }
    names.sort()
    print($"{names} {total}")

    var seen = HashSet<int> { 3, 5, 8 }
    int sum = 0
    for n in read seen {
        sum += n
    }
    print(sum)
}
```

```text
["ada", "grace"] 123
16
```

Each item of a map has a `key` and a `value`; `ref` changes the values, never the keys. A map and a set keep no
order, so the names are sorted before they are printed.

### Arrays and slices

```sg
int total(read int[] items) {
    int sum = 0
    for x in items {
        sum += x
    }
    return sum
}

void main() {
    var primes = int[4] { 2, 3, 5, 7 }
    var xs = List<int> { 10, 20, 30, 40 }
    for x in xs[1..3] {
        print(x)
    }
    print(total(read primes))
    print(total(xs[2..]))
    print(xs.length())
}
```

```text
20
30
17
70
4
```

`xs[1..3]` is a window onto the items at 1 and 2, lent to the loop. `items` is already a borrow, so the loop in
`total` lends its items too.

### The characters, bytes, words and lines of a text

```sg
void main() {
    for c in "año".chars() {
        print(c)
    }
    for b in "año".asBytes() {
        print(b)
    }
    for word in " to be  or\tnot ".splitWhitespace() {
        print(word)
    }
    for line in "one\ntwo\n".lines() {
        print(line)
    }
}
```

```text
a
ñ
o
97
195
177
111
to
be
or
not
one
two
```

A `string` is not gone over by itself: `chars()` gives its characters, `asBytes()` its bytes (`ñ` is two bytes
in UTF-8), `splitWhitespace()` its words and `lines()` its lines.

### Counting down

```sg
void main() {
    var order = List<int>()
    for i in (1..6).reversed() {
        order.add(i)
    }
    print(order)

    var xs = List<string> { "a", "b", "c" }
    for i in (0..xs.length()).reversed() {
        print($"{i} {xs[i]}")
    }
}
```

```text
[5, 4, 3, 2, 1]
2 c
1 b
0 a
```

`(a..b).reversed()` counts from `b - 1` down to `a`. It is the safe way down to 0: the count never goes below
`a`, so an unsigned count stops at 0 without wrapping. A range in brackets is written only so, at the head of a `for`.

### `break` and `continue`

```sg
void main() {
    for n in 1..20 {
        if n % 3 == 0 {
            continue
        }
        if n > 7 {
            break
        }
        print(n)
    }
}
```

```text
1
2
4
5
7
```

`continue` skips the rest of the round for 3 and 6; `break` leaves the loop at 8.

### A loop with a name

```sg
void main() {
    var grid = List<List<int>> {
        List<int> { 1, 2, 3 },
        List<int> { 4, 5, 6 },
        List<int> { 7, 8, 9 }
    }
    search: for row in 0..grid.length() {
        for col in 0..grid[row].length() {
            if grid[row][col] == 5 {
                print($"found at {row}, {col}")
                break search
            }
        }
    }

    rows: for row in read grid {
        for value in read row {
            if value % 2 == 0 {
                print($"the first even item is {value}")
                continue rows
            }
        }
        print("no even item")
    }
}
```

```text
found at 1, 1
the first even item is 2
the first even item is 4
the first even item is 8
```

`break search` leaves both loops at once. `continue rows` goes on with the next row, so `"no even item"` is
printed only for a row where the inner loop ended by itself.

### Over what `try` opens

```sg
Result<List<int>, string> numbers(bool good) {
    if !good {
        return Error("no numbers".clone())
    }
    return Ok(List<int> { 1, 2, 3 })
}

Result<int, string> sum(bool good) {
    int total = 0
    for n in try numbers(good) {
        total += n
    }
    return Ok(total)
}

void main() {
    print(sum(true) ?? -1)
    print(sum(false) ?? -1)
}
```

```text
6
-1
```

The collection is computed once, before the first round. When `numbers` gives an error, `try` hands it to the
caller there, and no round runs.

### Over a channel

```sg
void greet(Sender<string> out) {
    _ = out.send("hello".clone())
    _ = out.send("world".clone())
}

void main() {
    var words = Channel<string>(4)
    var task = Task.run(greet, words.sender())
    for word in words {
        print(word)
    }
    task.wait()
}
```

```text
hello
world
```

A `for` over a channel receives each item as it comes, and ends when every sender has ended: here, when
`greet` returns.

## What the compiler refuses

### A text by itself

```sg error
void main() {
    for c in "hello" {
        print(c)
    }
}
```

```text
error: `for` does not go over a `string`
  --> example.sg:2:14
 2 |     for c in "hello" {
   |              ^^^^^^^
  hint: the characters: `for c in "hello".chars()`; the bytes: `for b in "hello".asBytes()`
```

### Something that has no items

```sg error
void main() {
    int count = 3
    for i in count {
        print(i)
    }
}
```

```text
error: `for` goes over a range `a..b`, a `List`, an array, a slice, a `HashMap`, a `HashSet`, a `Channel` or an iterator such as `text.chars()` or `reader.lines()`, found `int`
  --> example.sg:3:14
 3 |     for i in count {
   |              ^^^^^
  hint: the numbers from 0 up to it: `for i in 0..count`
```

The rounds of a count are written `for i in 0..count`.

### A range that takes its end

```sg error
void main() {
    for i in 1..=3 {
        print(i)
    }
}
```

```text
error: `..=` is written only in a pattern; a `for` goes up to its end, without it
  --> example.sg:2:15
 2 |     for i in 1..=3 {
   |               ^^^
  hint: `for i in a..b + 1`
```

### Changing the collection the loop goes over

```sg error
void main() {
    var queue = List<int> { 1, 2, 3 }
    for job in read queue {
        if job == 2 {
            queue.add(4)
        }
    }
}
```

```text
error: `queue` cannot be changed or moved while a `for` goes over `queue`
  --> example.sg:5:13
 5 |             queue.add(4)
   |             ^^^^^
  the loop is here:
  --> example.sg:3:16
 3 |     for job in read queue {
   |                ^^^^^^^^^^
  hint: go over the positions instead (`for i in 0..queue.length()`), or keep the changes in another list until the loop ends
```

A list that grew inside the loop could move its items to new memory under the loop's feet.

### A list used after the loop took it

```sg error
void main() {
    var names = List<string> { "ada", "grace" }
    for name in names {
        print(name)
    }
    print(names.length())
}
```

```text
error: `names` was moved and can no longer be used
  --> example.sg:6:11
 6 |     print(names.length())
   |           ^^^^^
  it was moved here:
  --> example.sg:3:17
 3 |     for name in names {
   |                 ^^^^^
```

`for name in read names` would only lend the items, and keep the list.

### The items of a set, changed

```sg error
void main() {
    var seen = HashSet<int> { 1, 2 }
    for n in ref seen {
        n += 1
    }
}
```

```text
error: the items of a `HashSet` cannot be changed in place: the set finds them by their value
  --> example.sg:3:14
 3 |     for n in ref seen {
   |              ^^^^^^^^
  hint: go over it with `read`, and use `remove` and `add` after the loop
```

### A name nothing reads

```sg error
void main() {
    for i in 0..3 {
    }
}
```

```text
error: `i` is never used
  --> example.sg:2:9
 2 |     for i in 0..3 {
   |         ^
  hint: when the loop needs no name: `for _ in 0..3`
```

### A loop name nothing uses

```sg error
void main() {
    outer: for i in 0..3 {
        for j in 0..3 {
            if j == 1 {
                break
            }
            print(i + j)
        }
    }
}
```

```text
error: no `break outer` or `continue outer` uses the name of this loop
  --> example.sg:2:5
 2 |     outer: for i in 0..3 {
   |     ^^^^^
  hint: a loop needs a name only for a loop inside it to leave it: remove `outer:`
```

### A name that hides another

```sg error
void main() {
    var i = 5
    for i in 0..3 {
        print(i)
    }
    print(i)
}
```

```text
error: `i` is already a local variable of a block around this one, and a new name cannot hide it
```
