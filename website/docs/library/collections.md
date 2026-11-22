---
title: Collections
description: Arrays, slices, List, HashMap and HashSet, with all their methods.
sidebar_position: 3
---

# Collections

Five kinds of collection are built into the language. An array `T[N]` has a fixed number of items, kept where
the value is. A slice, `read T[]` or `ref T[]`, looks at items that something else owns. A `List<T>` grows on
the heap. A `HashMap<K, V>` keeps a value under each key, and a `HashSet<T>` keeps each item once.

Places and lengths are `nint`. An index outside the items panics with the index and the length
(`index 3 is out of range: the length is 1`), and an index that is known when the program is compiled is a
compile error instead. `get` gives `null` where an index would panic.

In the signatures, `read T` is how a method that only looks takes an item or a key: a text as a `read
string`, a number or a `bool` by value. `read T?` is what such a method gives: an `int?` for numbers, a
`read string?` for text.

A collection is `Equatable` when its items are (`==`), and printed when its items can be: `[1, 2, 3]`,
`{"ada": 36}`, `{1, 2}`. A `HashMap` and a `HashSet` promise no order: go over a map's keys in order by
putting them in a list and sorting it.

## Owning and borrowing

A collection owns its items. `for x in xs` hands the collection over and `x` owns each item; `for x in read
xs` and `for x in ref xs` lend them. Passing a collection where a borrow is expected needs `read` or `ref`
written, and through `read` nothing changes:

```sg error
void grow(read List<int> xs) {
    xs.add(1)
}

void main() {
    var xs = List<int>()
    grow(read xs)
}
```

```text
error: `add` changes the list, but it is reached through a `read` borrow
  --> example.sg:2:5
 2 |     xs.add(1)
   |     ^^
  hint: take the value as `ref` instead of `read`
```

An item reached by its index is lent, not moved out: `names[0]` on a `List<string>` is a `read string`, and
`.clone()` makes a copy that owns its text. While that borrow is used, the list cannot change:

```sg error
void main() {
    var names = List<string> { "ada", "grace" }
    var first = names[0]
    names.clear()
    print(first)
}
```

```text
error: `names` cannot be changed here: `first` borrows from it and is used again later
  --> example.sg:4:5
 4 |     names.clear()
   |     ^^^^^
  `first` borrowed it here:
  --> example.sg:3:17
 3 |     var first = names[0]
   |                 ^^^^^^^^
  hint: finish with `first` before this line: a borrow ends at its last use
```

For the same reason a collection that a `for` goes over by borrowing cannot be changed inside the loop.

## `List<T>`

| Signature | What it does |
|---|---|
| `List<T>()`, `List<T> { a, b }` | An empty list, or one with these items |
| `List<T> List<T>.repeat(T item, nint count)` | `count` copies of a `Copyable` item, in one block: `List<byte>.repeat(0, 65536)` is a buffer of a size known when the program runs. Zeros come from fresh zeroed memory, nothing written; fewer than 0 items is a panic |
| `List<T> List<T>.withCapacity(nint count)` | An empty list with room for `count` items, taken at once, so that adding that many never moves it; fewer than 0 is a panic |
| `void add(ref this, T item)` | Adds the item at the end |
| `void addAll(ref this, read T[] items)` | Copies the items to the end, when they are `Copyable` |
| `nint length(read this)` | The number of items |
| `xs[i]`, `xs[i] = v`, `xs[i] += 1`, `ref xs[i]` | Reads, writes or lends one item; a panic outside the list |
| `read T? get(read this, nint i)` | The item, or `null` outside the list |
| `T? removeLast(ref this)` | Takes the last item out, or gives `null` when the list is empty |
| `void clear(ref this)` | Removes every item, and keeps the memory |
| `void reverse(ref this)` | Puts the items in the other order |
| `nint? indexOf(read this, read T item)`, `bool contains(read this, read T item)` | Where the first equal item is; for `Equatable` items |
| `void sort(ref this)` | Puts `Comparable` items in order (numbers and text are) |
| `void copyFrom(ref this, read T[] items)` | Copies `Copyable` items over as many; a panic if the lengths differ. The two may look at one place, as `memmove` |
| `nint? binarySearch(read this, read T item)` | On items in order: the place of the first equal one, or `null`, found by halving |
| `nint lowerBound(read this, read T item)`, `nint upperBound(read this, read T item)` | On items in order: the first place whose item is not less than `item`, or is greater than it |

`upperBound(b) - lowerBound(a)` counts the items from `a` up to `b`.

```sg
void main() {
    var xs = List<int> { 5, 3, 8 }
    xs.add(1)
    print($"{xs} {xs.length()} {xs[0]}")
    xs[1] += 10
    print(xs.get(9) ?? -1)
    print(xs.removeLast() ?? -1)
    xs.sort()
    print(xs)
    xs.addAll(read List<int> { 2, 2 })
    xs.sort()
    print(xs)
    print($"{xs.indexOf(13) ?? -1} {xs.contains(4)} {xs.binarySearch(8) ?? -1}")
    print($"{xs.lowerBound(2)} {xs.upperBound(2)}")
    var bytes = List<byte>()
    bytes.addAll("hi".asBytes())
    print(bytes)
    var buffer = List<byte>.repeat(0, 4)
    buffer[1] = 9
    var names = List<string>.withCapacity(2)
    names.add("ada")
    print($"{buffer} {names}")
}
```

It prints:

```text
[5, 3, 8, 1] 4 5
-1
1
[5, 8, 13]
[2, 2, 5, 8, 13]
4 false 3
0 2
[104, 105]
[0, 9, 0, 0] ["ada"]
```

`addAll` on text is refused, since a `string` is not copied silently; the hint is `more.add(item.clone())`.

### A list of borrows

A `List<read T>` holds borrows and copies nothing. Each value it borrowed from is followed on its own: changing
one, or letting it end, while the list is used later is a compile error. The borrows count until the list is
cleared, and `clear()` reads none of them: a list made outside a loop is filled again in each round, with no
memory taken for it, when it is cleared before it is used again, also after what it borrowed from has changed or
ended. A list made inside the loop needs no `clear` at all. `get` and `removeLast` give a `read T?`, and what `removeLast` takes out
borrows what the list borrowed, not the list. The searches take a borrow too: `numbers.indexOf(read 9)`.

```sg
struct Order {
    string customer
    long amount
}

void main() {
    var orders = List<Order> {
        Order { customer = "ada", amount = 1200 },
        Order { customer = "grace", amount = 300 },
        Order { customer = "ada", amount = 80 }
    }
    var large = List<read Order>()
    for o in read orders {
        if o.amount >= 100 {
            large.add(o)
        }
    }
    print($"{large.length()} {large[0].customer} {large[1].amount}")

    var out = List<byte>()
    var pieces = List<read byte[]>()
    var tail = "\n".clone()
    for i in 0..2 {
        pieces.clear()
        out.clear()
        out.add((byte)('a' + i))
        pieces.add(out[0..1])
        pieces.add(tail.asBytes())
        print(pieces)
    }
}
```

It prints:

```text
2 ada 300
[[97], [10]]
[[98], [10]]
```

In C the items are pointers, and a `read string` or a window is a start and a length, so a `List<read byte[]>`
is laid out as the array of pieces the system's `writev` takes, and `conn.sendPieces(read pieces)` gives it to
the system as it is. There is no `List<ref T>`, and a list of windows
has no `get` or `removeLast`, which would give a slice that may be missing.

## `T[N]`

An array has its length in its type and lies where it is kept: in a variable, or inside a struct.

| Signature | What it does |
|---|---|
| `int[3] { 1, 2, 3 }` | The items, every one of them |
| `byte[16](255)` | One value in every item |
| `byte[16]` | Zeros: 0, `false` or `null` in every item; a type with no zero value (text) needs the items written |
| `xs[i]`, `xs[i] = v` | Reads or writes one item |
| `length`, `get`, `indexOf`, `contains`, `binarySearch`, `lowerBound`, `upperBound`, `sort`, `reverse`, `copyFrom` | As on a `List` |

An array of items that can be copied is copied like a number; any other is moved.

```sg
void main() {
    var primes = int[4] { 7, 2, 5, 3 }
    var ones = byte[3](1)
    var zeros = int[2]
    print($"{primes} {ones} {zeros} {primes.length()}")
    primes.sort()
    var copy = primes
    copy[0] = 100
    print($"{primes} {copy[0]} {primes == copy}")
    print(primes.get(9) ?? -1)
}
```

It prints:

```text
[7, 2, 5, 3] [1, 1, 1] [0, 0] 4
[2, 3, 5, 7] 100 false
-1
```

## Slices

A slice is a window on the items of a list, an array or another slice. `read xs` and `ref xs` give one where a
slice is expected, and so does a range: `xs[a..b]`, `xs[..b]`, `xs[a..]`, `xs[..]`, and `ref xs[a..b]` to write
through. A function that takes `read T[]` takes a list and an array alike. A range lent as `ref` lends the whole
list or array, as in Rust: `f(ref xs[0..2], xs[2..4])` is an error, however far apart the two ranges are.

| Signature | What it does |
|---|---|
| `s[i]`, `s.length()`, `s.get(i)`, `for x in s` | As on a list; a slice lends its items |
| `indexOf`, `contains`, `binarySearch`, `lowerBound`, `upperBound` | As on a list, the places counted from the start of the slice |
| `sort`, `reverse`, `copyFrom`, `s[i] = v` | Only through a `ref T[]`; `xs[2..5].sort()` sorts those items where they are, `buf[0..n].copyFrom(buf[k..k + n])` copies inside one list |

```sg
int sum(read int[] items) {
    var total = 0
    for item in items {
        total += item
    }
    return total
}

void zero(ref int[] items) {
    for i in 0..items.length() {
        items[i] = 0
    }
}

void main() {
    var xs = List<int> { 1, 2, 3, 4, 5 }
    var fixed = int[3] { 10, 20, 30 }
    print($"{sum(read xs)} {sum(xs[1..3])} {sum(read fixed)}")
    zero(ref xs[3..])
    xs[..3].reverse()
    print(xs)
    var window = xs[1..]
    print($"{window.length()} {window[0]} {window.contains(0)}")
}
```

It prints:

```text
15 5 60
[3, 2, 1, 0, 0]
4 2 true
```

`byte[]` alone is not a type: it is `List<byte>` for an owner and `read byte[]` for a window, and the compiler
says so.

## `HashMap<K, V>`

A key is an integer, a `bool`, a `string`, or a type of your own written `: Equatable, Hashable`. A method that
only looks a key up takes text as `read string`, so a literal or a part of a text goes in as it is.

| Signature | What it does |
|---|---|
| `HashMap<K, V>()`, `HashMap<K, V> { {k, v} }` | An empty map, or one with these entries |
| `void add(ref this, K key, V value)` | Puts in a new key; a panic when the key is there already |
| `map[key]`, `map[key] = v`, `map[key] += 1` | Reads or writes what is under a key that is there; a panic for a missing one |
| `read V? get(read this, read K key)` | What is under the key, or `null` |
| `ref V getOrAdd(ref this, read K key, V initial)` | What is under the key, put in with `initial` first when it is missing |
| `bool contains(read this, read K key)` | Whether the key is there |
| `V? remove(ref this, read K key)` | Takes the entry out and gives its value, or `null` |
| `nint length(read this)`, `void clear(ref this)` | The number of entries; removes them all |

Neither a silent overwrite nor a key that appears by itself: `add` refuses a key that is there, and `map[key] =
v` one that is not. "Put or overwrite" is `getOrAdd`. Over a map, the variable of a `for` has a `key` and a
`value`.

```sg
void main() {
    var ages = HashMap<string, int> { {"ada", 36} }
    ages.add("grace", 85)
    ages["ada"] += 1
    print(ages["ada"])
    print(ages.get("alan") ?? 0)
    print($"{ages.contains("grace")} {ages.remove("grace") ?? 0} {ages.length()}")
    print(ages)

    var counts = HashMap<string, int>()
    for word in "to be or not to be".splitWhitespace() {
        var slot = counts.getOrAdd(word, 0)
        slot += 1
    }
    var words = List<string>()
    for entry in read counts {
        words.add($"{entry.key}={entry.value}".toString())
    }
    words.sort()
    print(words)
}
```

It prints:

```text
37
0
true 85 1
{"ada": 37}
["be=2", "not=1", "or=1", "to=2"]
```

## `HashSet<T>`

An item is what a key of a `HashMap` can be.

| Signature | What it does |
|---|---|
| `HashSet<T>()`, `HashSet<T> { a, b }` | An empty set, or one with these items |
| `bool add(ref this, T item)` | Puts the item in: `true` if it was not there |
| `bool remove(ref this, read T item)` | Takes it out: `true` if it was there |
| `bool contains(read this, read T item)` | Whether it is there |
| `nint length(read this)`, `void clear(ref this)` | The number of items; removes them all |

Adding what is already in a set is ordinary use, so `add` does not panic: `if !seen.add(id) { continue }`.

```sg
void main() {
    var seen = HashSet<int> { 1, 2 }
    print(seen.add(2))
    print(seen.add(3))
    print(seen.remove(1))
    print($"{seen.contains(1)} {seen.length()}")
    var words = HashSet<string>()
    for word in "a b a c a".splitWhitespace() {
        if !words.add(word.clone()) {
            print($"again: {word}")
        }
    }
}
```

It prints:

```text
false
true
true
false 2
again: a
again: a
```

A collection is not `Hashable` and not `Comparable`: it is not a key, and `xs < ys` is an error.
