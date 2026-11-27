---
name: List
title: List<T>
kind: library-type
summary: A list of items that grows, kept in one block of memory.
see: read, ref
learn: ../docs/learn/book/08-collections.md
library: ../docs/library/collections.md#listt
---

## Forms

| Form | What it is |
|---|---|
| `List<T>()` | an empty list |
| `List<T> { a, b }` | a list with these items |
| `xs.add(item)` | adds the item at the end |
| `xs[i]`, `xs[^1]` | the item at `i`, the last item |
| `xs.length()` | the number of items |
| `for x in xs` | each item in turn |
| `read T[]`, `ref T[]` | a function that takes any list or array |
| `List<read T>` | a list of borrows, which copies nothing |

## What it means

A `List<T>` owns its items, one after another in one block of memory, and takes a bigger block when it grows.
When the list ends, its items end with it. Reading outside the list stops the program with a panic, never reads
another value's memory.

Every method is in [Collections](../docs/library/collections.md#listt). A function that only needs the items
takes `read T[]` or `ref T[]`, and then works for an array and for a part of a list as well.

## Examples

### Make one and add to it

```sg
void main() {
    var names = List<string>()
    names.add("Ada")
    names.add("Grace")
    print(names)
    print(names.length())
}
```

```text
["Ada", "Grace"]
2
```

### Read and write items

```sg
void main() {
    var xs = List<int> { 5, 3, 8 }
    print(xs[0])
    print(xs[^1])
    xs[1] += 10
    print(xs)
    print(xs.get(9) ?? -1)
}
```

```text
5
8
[5, 13, 8]
-1
```

`xs[^1]` counts from the end. `get` gives `null` outside the list instead of a panic, and `??` gives a value for
that case.

### Give it to a function that only reads

```sg
int total(read int[] values) {
    var sum = 0
    for v in values {
        sum += v
    }
    return sum
}

void main() {
    var xs = List<int> { 5, 3, 8, 1 }
    print(total(read xs))
    print(total(xs[1..3]))
}
```

```text
17
11
```

### Sort and search

```sg
void main() {
    var xs = List<int> { 5, 3, 8, 1 }
    xs.sort()
    print(xs)
    print(xs.binarySearch(8) ?? -1)
    print(xs.contains(4))
    print(xs.removeLast() ?? -1)
    print(xs)
}
```

```text
[1, 3, 5, 8]
3
false
8
[1, 3, 5]
```

### A list of borrows

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
}
```

```text
2 ada 300
```

`large` points at orders that `orders` owns and copies none of them. While `large` is used, `orders` cannot be
changed: that would leave `large` pointing at memory that moved.

## What stops the program

### An index outside the list

```sg panic
void main() {
    var xs = List<int> { 5, 3, 8 }
    print(xs[3])
}
```

```text
panic: index 3 is out of range: the length is 3
```

## What the compiler refuses

### Copying items that are not copied silently

```sg error
void main() {
    var names = List<string> { "Ada".clone() }
    var more = List<string>()
    more.addAll(read names)
}
```

```text
error: `addAll` copies the items, and `string` is not `Copyable`
  --> example.sg:4:10
 4 |     more.addAll(read names)
   |          ^^^^^^
  hint: add them one by one, each a copy: `more.add(item.clone())`
```

A text owns its memory, so a copy of it is asked for with `.clone()`, where you can see it.
