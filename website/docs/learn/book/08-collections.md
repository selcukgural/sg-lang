---
title: Collections
description: Arrays, slices, lists, hash maps and hash sets, the loops that go over them, and what the compiler refuses while they do.
sidebar_position: 8
---

# Collections

SG has five ways to hold many values. An array `T[N]` has a fixed number of items and lives where it is declared.
A slice, `read T[]` or `ref T[]`, looks at items that something else owns. A `List<T>` grows. A
`HashMap<K, V>` finds a value by its key, and a `HashSet<T>` knows which items it holds. All five are built into
the language and checked by it: an index is checked, and a collection cannot change while a loop goes over it.

## Arrays

```sg
void main() {
    var primes = int[4] { 2, 3, 5, 7 }
    var zeros = byte[3]
    var sevens = int[5](7)
    print($"{primes} {zeros} {sevens}")
    print(primes.length())

    var copy = primes
    copy[0] = 100
    print($"{primes[0]} {copy[0]}")
    print(primes == int[4] { 2, 3, 5, 7 })
}
```

It prints:

```text
[2, 3, 5, 7] [0, 0, 0] [7, 7, 7, 7, 7]
4
2 100
true
```

The length is part of the type: an `int[4]` is four `int`s, kept in the variable or inside the struct that holds
it, with nothing on the heap. It is built from its items (`int[4] { 2, 3, 5, 7 }`), from one value for every
item (`int[5](7)`), or with zeros (`byte[3]`; `false` and `null` for those types). An array of `Copyable` items
is copied like a number, as `copy` shows; any other array is moved.

An index is checked. When the compiler knows it, a wrong one does not compile:

```sg error
void main() {
    var buffer = byte[16]
    buffer[16] = 1
}
```

```text
error: index 16 is out of range: the length is 16
  the items of a `byte[16]` are at 0 to 15
  --> example.sg:3:5
 3 |     buffer[16] = 1
   |     ^^^^^^^^^^
```

When it depends on the data, a wrong index is a panic as the program runs, and `get(i)` gives `null` instead
(see [`null` and errors](./06-null-and-errors.md)). Arrays also have `indexOf`, `contains`, `sort`, `reverse`
and the searches that lists have.

## Slices

A slice is a window onto items that a list or an array owns: a pointer and a length, nothing copied. A function
that only needs the items takes `read T[]`, and a function that writes them takes `ref T[]`; then it works for a
list, an array and any part of either:

```sg
int total(read int[] values) {
    var sum = 0
    for v in values {
        sum += v
    }
    return sum
}

void fill(ref int[] target, int value) {
    for i in 0..target.length() {
        target[i] = value
    }
}

void main() {
    var list = List<int> { 1, 2, 3, 4, 5, 6 }
    var array = int[4] { 10, 20, 30, 40 }
    print(total(read list))
    print(total(read array))

    print(list[1..3])
    print(total(list[1..3]))
    print(total(array[2..]))
    print(list[..2])

    fill(ref list[4..], 0)
    print(list)
    var mixed = int[5] { 5, 1, 4, 2, 3 }
    mixed[1..4].sort()
    print(mixed)
}
```

It prints:

```text
21
100
[2, 3]
5
70
[1, 2]
[1, 2, 3, 4, 0, 0]
[5, 1, 2, 4, 3]
```

`read list` and `read array` lend the whole collection as a slice. A range takes a part: `list[1..3]` is the
items at 1 and 2, the end not included; `array[2..]` runs to the end and `list[..2]` from the start. `ref` in
front, `ref list[4..]`, lends the part for writing, and `mixed[1..4].sort()` puts only those items in order,
where they are. While a slice that is used later borrows a list, the list cannot be changed: growing or clearing
it may move or free its items, and the slice would point at the old memory.

## Counting from the end

```sg
void main() {
    var days = List<string> { "mon", "tue", "wed", "thu", "fri" }
    print(days[^1])
    print(days[^2])
    print(days[1..^1])
    print(days[^2..])

    var line = "[42]"
    print(line[1..^1])

    days[^1] = "FRI"
    print(days)
}
```

It prints:

```text
fri
thu
["tue", "wed", "thu"]
["thu", "fri"]
42
["mon", "tue", "wed", "thu", "FRI"]
```

`days[^1]` is the last item and `days[^2]` the one before it: `^n` is `days.length() - n`, with the list named
once. It works where an index does, on a list, an array, a slice and a text, for reading and for writing, and at
either end of a range: `days[1..^1]` leaves out the first and the last, `days[^2..]` is the last two. As an index
`^0` would be past the last item, so it is an error; as the end of a range it is the length. A count outside the
list panics with what was written: `index ^9 is out of range: the length is 5`. `^` is written only between the
brackets; `var i = ^1` is an error.

## `List<T>`

```sg
void main() {
    var scores = List<int>()
    scores.add(72)
    scores.add(95)
    scores.add(60)
    scores.add(88)
    print($"{scores} has {scores.length()} items")

    scores[0] += 3
    print(scores[0])
    print(scores.get(10) ?? -1)

    var last = scores.removeLast() ?? 0
    print($"took {last}, left {scores}")

    scores.sort()
    print(scores)
    print(scores.contains(95))
    print(scores.binarySearch(75) ?? -1)
    print(scores.binarySearch(80) ?? -1)

    var names = List<string> { "grace", "ada", "edsger" }
    names.sort()
    print(string.join(", ", read names))
}
```

It prints:

```text
[72, 95, 60, 88] has 4 items
75
-1
took 88, left [75, 95, 60]
[60, 75, 95]
true
1
-1
ada, edsger, grace
```

A list is built empty, `List<int>()`, or with its items, `List<int> { 1, 2 }`. `add` puts an item at the end and
`length()` counts them. `xs[i]` reads and writes an item and panics outside the list; `get(i)` gives `null`
there. `removeLast()` takes the last item out and gives a `T?`, `null` for an empty list. `sort()` needs items
that are `Comparable` (numbers and texts are), and `indexOf` and `contains` items that are `Equatable`.
`binarySearch(v)` looks for a value in a sorted list by halving it, and gives its place or `null`. There are
also `reverse()`, `clear()`, `addAll(...)`, and `lowerBound` and `upperBound` for ranges of sorted values.

## `HashMap<K, V>`

```sg
void main() {
    var ages = HashMap<string, int> { {"ada", 36}, {"grace", 45} }
    ages.add("edsger", 72)
    ages["ada"] = 37
    print(ages["ada"])
    print(ages.get("alan") ?? 0)
    print(ages.contains("grace"))
    print(ages.length())

    var words = List<string> { "the", "cat", "saw", "the", "dog", "the", "cat" }
    var counts = HashMap<string, int>()
    for word in read words {
        var count = counts.getOrAdd(word, 0)
        count += 1
    }
    var keys = List<string>()
    for item in read counts {
        keys.add(item.key.clone())
    }
    keys.sort()
    for key in read keys {
        print($"{key}: {counts[key]}")
    }
}
```

It prints:

```text
37
0
true
3
cat: 2
dog: 1
saw: 1
the: 3
```

`add(key, value)` puts in a new key; a key that is already there is a panic, since `ages["ada"] = 37` is the
way to change one. `map[key]` reads and writes the value under a key that is there, and panics for one that is
not; `get(key)` gives `null` for it. `getOrAdd(key, initial)` finds the value in one lookup, putting `initial`
there first when the key was missing, and lends it for writing: counting words takes one line. `remove(key)`
takes a key out.

A key is a number, a `bool`, a `char`, a `string`, or a type of your own that is `Equatable, Hashable`. The hash
has a secret key drawn when the program starts, so input chosen by someone else cannot make all the keys
collide.

**The order of a map is not promised.** A `HashMap` is a hash table, and a `for` over it goes in whatever order
the table has. Today that is the order the keys were added in until one is removed, the same in the interpreter
and in compiled code, but a program must not count on it. When the order matters, sort the keys, as the program
above does.

## `HashSet<T>`

```sg
void main() {
    var seen = HashSet<string>()
    var lines = List<string> { "GET /", "GET /about", "GET /", "POST /login", "GET /about" }
    for line in read lines {
        if !seen.add(line.clone()) {
            print($"again: {line}")
        }
    }
    print(seen.length())
    print(seen.contains("GET /"))
    print(seen.remove("POST /login"))
    print(seen.remove("POST /login"))

    var stock = HashMap<string, int> { {"apples", 3}, {"pears", 0}, {"plums", 12} }
    var total = 0
    for item in read stock {
        total += item.value
    }
    print(total)
}
```

It prints:

```text
again: GET /
again: GET /about
3
true
true
false
15
```

A set takes the same items a map takes as keys, with no order either. `add` gives `true` when the item was new,
and `remove` gives `true` when it was there; the compiler makes you use what they give, or drop it with `_ =`.
`line.clone()` is there because `line` only borrows the text in the list, and the set keeps a text of its own.
Over a `HashMap`, the loop variable has a `key` and a `value`.

## `for` over a collection

How a `for` names the collection says what the loop may do with the items:

```sg
void main() {
    var prices = List<int> { 100, 250, 80 }
    for price in ref prices {
        price = price * 2
    }
    for price in read prices {
        print(price)
    }

    var names = List<string> { "ada", "grace" }
    var shouting = List<string>()
    for name in names {
        shouting.add(name.toUpper())
    }
    print(shouting)
}
```

It prints:

```text
200
500
160
["ADA", "GRACE"]
```

- `for x in read xs` lends each item to look at;
- `for x in ref xs` lends each item to change (over a `HashMap`, `ref` changes the values, never the keys);
- `for x in xs` hands the collection over: each item is owned by its round, and the list is gone after the loop.

The last one moves the list, so it cannot be used after:

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

## Changing a collection while a loop goes over it

A loop over a list holds a borrow of its items. If the list grew inside the loop, its items could move to new
memory under the loop's feet; in C++ that is undefined behaviour, in Java an exception at run time. In SG it
does not compile:

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

The hint gives the two ways out. A range over the positions borrows nothing; its end is found once, before the
first round, so the items added are not gone over. Or keep the changes and apply them after the loop:

```sg
void main() {
    var queue = List<int> { 1, 2, 3 }
    for i in 0..queue.length() {
        if queue[i] == 2 {
            queue.add(4)
        }
    }
    print(queue)

    var later = List<int>()
    for job in read queue {
        if job % 2 == 0 {
            later.add(job * 10)
        }
    }
    queue.addAll(read later)
    print(queue)
}
```

It prints:

```text
[1, 2, 3, 4]
[1, 2, 3, 4, 20, 40]
```

The same holds for a `HashMap`: `remove` inside a `for` over the map is refused. Changing another field of the
same struct, or another collection, is fine.

## Loops with names, and counting down

A loop can have a name, written before it with a colon. `break name` and `continue name` then act on that loop
from inside a loop within it. `(a..b).reversed()` counts from `b - 1` down to `a`:

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

    var order = List<int>()
    for i in (1..6).reversed() {
        order.add(i)
    }
    print(order)
}
```

It prints:

```text
found at 1, 1
the first even item is 2
the first even item is 4
the first even item is 8
[5, 4, 3, 2, 1]
```

A name that no `break` or `continue` uses is an error, so a name always means something. `reversed()` is the
safe way down to 0: `for i in (0..xs.length()).reversed()` never computes `-1`, which is where
`for (i = n - 1; i >= 0; i--)` with an unsigned `i` goes wrong in C. A range in a `for` leaves out its end; `..=` is
written only in a `match` pattern.
