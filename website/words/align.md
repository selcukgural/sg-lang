---
name: align
kind: keyword
summary: align(N) struct keeps every value of the type at a multiple of N bytes, wherever it is kept.
see: struct, packed, union, raw, new, atomic
learn: ../docs/learn/book/11-c.md#cs-types
---

## Forms

| Form | What it is |
|---|---|
| `align(64) struct Slot { ... }` | a struct whose values lie at a multiple of 64 bytes |
| `align(16) union Vector { ... }` | the same for a union |
| `public align(64) struct Slot { ... }` | with `public` in front, as for any type |
| `Memory.sizeOf<Slot>()`, `Memory.alignOf<Slot>()` | a multiple of 64, and 64 |

## What it means

`align(N)` in front of `struct` or `union` makes the alignment of the type at least `N`: a power of two up to
4096, and not below the alignment its fields ask for. Its size is rounded up to a multiple of `N`, so the items
of an array of it each start at one. Any struct may have it, also one with fields that own memory, such as a
`string` or a `List`.

Wherever a value of the type is kept, it lies at a multiple of `N`: in a variable, inside another struct, an
array or a union (which then take that alignment too, with empty bytes before it where needed), and on the
heap: behind `new`, in a `List` or a `HashMap`, in a `shared` value, in what a task is given, and in a block of
`Memory.allocate`. A C function lent one finds it where its type says. An item of raw memory reached where the
type may not lie stops the program in the interpreter and in a debug build, as for any type; a release build
tests nothing, and `Memory.readUnaligned` and `writeUnaligned` reach one anywhere.

It is for a layout that the machine or C asks for: a line of the cache to each value, so that tasks on
different cores do not pass one line between them; 16 or 32 bytes for the instructions that work on several
numbers at once; a page of 4096 bytes. A type is [`packed`](packed.md) or `align(N)`, not both, and a packed
struct holds no type written with `align(N)`. An alignment is written on a type, not on one field.

A block of `Memory.allocate` for a type aligned beyond 16 bytes is given back with `Memory.free` through a
pointer to that type: on Windows such a block is given back another way. The interpreter stops a block given
back through a pointer to another type; compiled code does not test it.

`align` is a word only in front of a declaration of a type: a variable or a function may still be named
`align`.

## Examples

### The size and the alignment

```sg
struct Plain {
    long count
    int kind
}

align(64) struct Slot {
    long count
    int kind
}

align(16) union Vector {
    float[4] lanes
    uint[4] bits
}

void main() {
    print($"{Memory.sizeOf<Plain>()} {Memory.alignOf<Plain>()}")
    print($"{Memory.sizeOf<Slot>()} {Memory.alignOf<Slot>()}")
    print($"{Memory.sizeOf<Vector>()} {Memory.alignOf<Vector>()}")
    var slot = Slot { count = 1, kind = 2 }
    slot.count += 41
    print($"{slot.count} {slot.kind}")
}
```

```text
16 8
64 64
16 16
42 2
```

`Plain` has the alignment of its `long`, and twelve bytes of fields rounded up to sixteen. `Slot` has the same
fields, at a multiple of 64, and is 64 bytes long. `Vector` would be 16 bytes at a multiple of 4; `align(16)`
puts it at a multiple of 16. The fields are used as in any struct.

### Inside other values

```sg
align(32) struct Cell : Copyable, Formattable {
    int value
}

struct Row {
    byte tag
    Cell[3] cells
}

union Either {
    byte small
    Cell cell
}

void main() {
    print($"{Memory.sizeOf<Row>()} {Memory.alignOf<Row>()}")
    print($"{Memory.sizeOf<Either>()} {Memory.alignOf<Either>()}")
    var row = Row { tag = 1, cells = Cell[3](Cell { value = 7 }) }
    row.cells[2].value = 9
    print(row.cells)
    var rows = List<Row>()
    for i in 0..3 {
        rows.add(Row { tag = (byte)i, cells = Cell[3](Cell { value = i }) })
    }
    print(rows[2].cells[0])
}
```

```text
128 32
32 32
[Cell { value = 7 }, Cell { value = 7 }, Cell { value = 9 }]
Cell { value = 2 }
```

In `Row` the cells start at byte 32, after 31 empty bytes, and take 96: 128 in all, at a multiple of 32. The
union takes the alignment and the size of `Cell`. In the list each row lies at a multiple of 32 too.

### A line of the cache to each task

```sg
align(64) struct Lane {
    atomic long hits
}

struct Board {
    Lane[4] lanes
}

void count(int lane, int times, shared Board board) {
    for _ in 0..times {
        _ = board.lanes[lane].hits.fetchAdd(1)
    }
}

void main() {
    print($"{Memory.sizeOf<Lane>()} {Memory.sizeOf<Board>()}")
    var board = new shared Board {
        lanes = Lane[4] { Lane { hits = atomic 0 }, Lane { hits = atomic 0 }, Lane { hits = atomic 0 }, Lane { hits = atomic 0 } }
    }
    var tasks = List<Task<void>>()
    for lane in 0..4 {
        tasks.add(Task.run(count, lane, 100000 * (lane + 1), share board))
    }
    Task.whenAll(tasks).wait()
    for lane in read board.lanes {
        print(lane.hits.load())
    }
}
```

```text
64 256
100000
200000
300000
400000
```

Each task adds to its own lane, and each lane fills 64 bytes, the size of a line of the cache on most
processors. Without `align(64)` the four lanes share 32 bytes, so every addition takes the line away from the
other cores: built with `--release`, with twenty million additions to each lane, that made the program ten
times slower on the machine these pages were checked on. The answer is the same either way.

### In raw memory

```sg
align(64) struct Slot : Copyable, Formattable {
    long count
}

void main() {
    var block = Memory.allocate<Slot>(4)
    print((nuint)block % 64)
    unsafe {
        for i in 0..4 {
            block[i] = Slot { count = (long)i * 10 }
        }
    }
    var view = unsafe Memory.view(block, 4)
    print(view[3])
    var next = unsafe (block + 1)
    print((nuint)next - (nuint)block)
    unsafe Memory.free(block)
}
```

```text
0
Slot { count = 30 }
64
```

`Memory.allocate` gives a block at a multiple of the alignment of its type, and the items lie 64 bytes apart.
The block is given back through the same `raw Slot`.

### Handed to C

```c title="slots/where.c"
#include <stdint.h>

uint32_t past(const void* p, uint32_t n) {
    return (uint32_t)((uintptr_t)p % n);
}
```

```sg file title="slots/main.sg"
extern "./where.c" {
    uint past(read Slot slot, uint n)
}

align(64) struct Slot : Copyable {
    long count
}

struct Holder {
    byte tag
    Slot slot
}

void main() {
    var local = Slot { count = 1 }
    var boxed = new Slot { count = 2 }
    var slots = List<Slot>()
    for i in 0..9 {
        slots.add(Slot { count = i })
    }
    var holders = List<Holder> { Holder { tag = 1, slot = Slot { count = 3 } } }
    print(unsafe past(read local, 64))
    print(unsafe past(read boxed, 64))
    print(unsafe past(read slots[7], 64))
    print(unsafe past(read holders[0].slot, 64))
}
```

```text
$ cd slots
$ sg main.sg
0
0
0
0
$ sg build main.sg -o slots
$ ./slots
0
0
0
0
```

C is told that a `Slot` lies at a multiple of 64 and may count on it. A variable, a value behind `new`, an item
of a list and a field of a struct in a list are each lent to C where they lie, at a multiple of 64, in the
interpreter as in the built program.

## What stops the program

### An item of raw memory where its type may not lie

```sg panic
align(64) struct Slot {
    long count
}

void main() {
    var block = Memory.allocate<Slot>(2)
    var bytes = unsafe (raw byte)block
    unsafe {
        var wrong = (raw Slot)(bytes + 16)
        wrong[0].count = 5
        Memory.free(block)
    }
}
```

```text
panic: raw memory is reached 16 byte(s) past a multiple of 64, the alignment of `Slot`: `Memory.readUnaligned` reads it there
  --> example.sg:10:9
 10 |         wrong[0].count = 5
    |         ^^^^^^^^
```

The cast itself does not stop the program; reaching the item does. A debug build stops there too; a release
build tests nothing.

## What the compiler refuses

### Not a power of two

```sg error
align(24) struct Slot {
    long count
}

void main() {
}
```

```text
error: an alignment is a power of two up to 4096, and this is 24
  --> example.sg:1:1
 1 | align(24) struct Slot {
   | ^^^^^^^^^
```

`align(8192)` and `align(0)` are refused the same way.

### Less than the fields ask for

```sg error
align(2) struct Slot {
    long count
}

void main() {
}
```

```text
error: `align(2)` asks for less than the 8 bytes its fields ask for
  `align(N)` makes the alignment larger; `packed` makes it 1
  --> example.sg:1:1
 1 | align(2) struct Slot {
   | ^^^^^^^^
```

### `packed` and `align` together

```sg error
align(8) packed struct Header {
    byte kind
}

void main() {
}
```

```text
error: a type is `packed` or `align(N)`, not both
  `packed` takes the padding out and makes the alignment 1; `align` makes it larger
  --> example.sg:1:1
 1 | align(8) packed struct Header {
   | ^^^^^^^^^^^^^^^
```

### An aligned type inside a packed struct

```sg error
align(16) struct Block {
    long a
}

packed struct Header {
    byte kind
    Block block
}

void main() {
}
```

```text
error: a `packed` struct holds no `Block`, which is `align(N)`
  Windows keeps that alignment inside a packed struct, the other systems do not
  --> example.sg:7:5
 7 |     Block block
   |     ^^^^^
```

The compilers of C would lay the struct out in two ways, so it would not be one layout on every system.

### Written twice

```sg error
align(16) align(32) struct Slot {
    long count
}

void main() {
}
```

```text
error: `align` is written once
  --> example.sg:1:11
 1 | align(16) align(32) struct Slot {
   |           ^^^^^^^^^
```
