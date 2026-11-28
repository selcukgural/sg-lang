---
title: Unsafe code
description: What `unsafe` marks, how raw memory is reached, what the interpreter and the sanitizers check there, and why the rest of a program stays safe.
sidebar_position: 12
---

# Unsafe code

The checker of SG follows every value: who owns it, who borrows it and for how long. A few things are beyond
what it can follow: memory reached through a raw address, a call into C, a device's registers. Those are
written inside `unsafe`, which says "here the writer, not the compiler, keeps the rules". Outside `unsafe`,
the promise holds: no use after free, no read out of bounds, no data race. When memory does go wrong, the
cause is in an `unsafe` block or in C, and there are few of those to read.

## What `unsafe` marks

`unsafe { ... }` marks a block, and `unsafe` in front of a value marks one expression (`var first = unsafe
p[0]`). It is needed for:

- an item of raw memory (`p[i]`, read or written) and moving a pointer (`p + n`);
- a cast between raw pointers, from a number to one, from a `raw void` to a `CFunc`, and `(ref T)p` or
  `(read T)p` of an address `Memory.addressOf` gave;
- reading a field of a union;
- `Memory.free`, `Memory.copy`, `Memory.view`, `Memory.viewRef`, `Memory.addressOf`, volatile reads and
  writes, and `Memory.readUnaligned` and `Memory.writeUnaligned`;
- `File.mapRead`;
- a call of a function of C, a call through a `CFunc`, a variable of C, and `string.fromCString`.

The note of the error below names a few of them; `sg help unsafe` lists them all.

Making room with `Memory.allocate`, comparing pointers (`p == null`, `p < q`) and counting the items between
two (`q - p`) need no `unsafe`: none of them reaches memory. An `unsafe` that nothing needs is an error, so
the word stays a sign that something real happens there:

```sg error
void main() {
    var numbers = Memory.allocate<int>(4)
    unsafe {
        print(numbers == null)
    }
    unsafe Memory.free(numbers)
}
```

```text
error: nothing here needs `unsafe`
  `unsafe` is for what the checker cannot follow, such as an item of raw memory (`p[i]`), a field of a union read, `Memory.free` or a call of a C function; `sg help unsafe` lists every case
  --> example.sg:3:5
 3 |     unsafe {
   |     ^^^^^^
  hint: leave `unsafe` out
```

And what needs it is refused without it:

```sg error
void main() {
    var numbers = Memory.allocate<int>(4)
    numbers[0] = 1
    unsafe Memory.free(numbers)
}
```

```text
error: this reaches raw memory, whose size and life the checker does not know, so it is written inside `unsafe`
  --> example.sg:3:5
 3 |     numbers[0] = 1
   |     ^^^^^^^^^^
  hint: a write is a statement: `unsafe { numbers[0] = ... }`
```

## Raw memory

`raw T` is an address that owns nothing and may be `null`. It points at plain data: numbers, `bool`, `char`,
raw pointers, and arrays and structs of them, laid out as C lays them out. A `string` or a `List` owns memory
of its own, so a raw pointer never points at one. `Memory.allocate<T>(n)` gives room for `n` items, all zero
(C's `calloc`), and `Memory.free(p)` gives it back:

```sg
struct Point {
    int x
    int y
}

void main() {
    var points = Memory.allocate<Point>(3)
    unsafe {
        points[0] = Point { x = 1, y = 2 }
        points[1].x = 10
        points[2] = points[0]
        points[2].y += 5
    }
    for i in 0..3 {
        var p = unsafe points[i]
        print($"{p.x} {p.y}")
    }
    var second = unsafe (points + 1)
    print(second - points)
    print(Memory.sizeOf<Point>())
    unsafe Memory.free(points)
}
```

It prints:

```text
1 2
10 0
1 7
1
8
```

`(raw byte)p` reads the same memory as another type, `Memory.copy(source, target, count)` copies items (also
when the two overlap), and `Memory.readUnaligned(p)` and `Memory.writeUnaligned(p, v)` reach an item at an
address that is not aligned for its type. `Memory.sizeOf<T>()` and `Memory.alignOf<T>()` give what C has.
`Memory.free` also gives back memory that C allocated with `malloc`.

A raw pointer does not go to another task, since the checker cannot tell whether two tasks would reach what it
points at. A struct that keeps that safe itself says so with `: unsafe Sendable`, a promise of its writer.

## What is checked, and where

The interpreter remembers every block `Memory.allocate` gave. It checks each item against its block, and stops
the program with a panic outside it, after `Memory.free`, and through `null`:

```sg panic
void main() {
    var numbers = Memory.allocate<int>(4)
    for i in 0..5 {
        unsafe {
            numbers[i] = (int)i
        }
    }
    unsafe Memory.free(numbers)
}
```

```text
panic: raw memory is written outside its block: item 4, where the block holds 4 items of `int`
  --> example.sg:5:13
 5 |             numbers[i] = (int)i
   |             ^^^^^^^^^^
```

That program is accepted by the checker; the interpreter stops it as it runs. A pointer made from a number
(`(raw int)n`) or read out of a union is checked as the pointer made into that number was, or against the
block it lies in; one that matches neither, and the address `Memory.addressOf` gives, reach nothing there.
Memory C gave is not checked. Compiled code checks none of
this: a `raw T` is a plain `T*` of C, and a release build costs what C costs. The same program, built, writes
past the block and goes on. An item at an address that is not a multiple of its type's alignment is a panic in
the interpreter and in a debug build, and costs nothing in a release build.

So unsafe code is tried in the interpreter first, and the compiled program under the C compiler's
sanitizers, from the C that `sg emit-c` prints:

```bash
sg emit-c example.sg > example.c
cc -std=c11 -g -fsanitize=address,undefined example.c -o example -lm -pthread
./example
```

which stops at the bad write, with something like:

```text
==59557==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x602000000980 ...
WRITE of size 4 at 0x602000000980 thread T0
```

## Looking at raw memory as a slice

`Memory.view(p, count)` lends `count` items as a `read T[]`, and `Memory.viewRef(p, count)` as a `ref T[]`.
From there the safe tools of slices take over: `for`, indexing with bounds checks, `List.addAll`,
`string.fromUtf8`. That is how a struct keeps `unsafe` to a few lines and gives its users a safe interface:

```sg
// A fixed block of numbers behind a safe interface: `unsafe` stays inside the struct.
struct Block : Disposable {
    raw long items
    nint count

    Block create(nint count) {
        return Block { items = Memory.allocate<long>(count), count = count }
    }

    void set(ref this, nint at, long value) {
        if at < 0 || at >= this.count {
            panic($"item {at} is outside the block")
        }
        unsafe {
            this.items[at] = value
        }
    }

    long sum(read this) {
        long total = 0
        for item in unsafe Memory.view(this.items, this.count) {
            total += item
        }
        return total
    }

    void dispose(ref this) {
        unsafe Memory.free(this.items)
    }
}

void main() {
    var block = Block(4)
    for i in 0..4 {
        block.set(i, (long)i * 100)
    }
    print(block.sum())
}
```

It prints:

```text
600
```

The block is freed once, when its owner ends, and every index is checked by `set`. The window a view gives
borrows the variable that holds the pointer, so the checker keeps it from outliving that variable:

```sg error
read int[] look(raw int p, nint count) {
    return unsafe Memory.view(p, count)
}

void main() {
}
```

```text
error: this returns a borrow of `p`, and `p` is gone when the function returns
  --> example.sg:2:12
 2 |     return unsafe Memory.view(p, count)
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  hint: take `p` as `read`, so that the caller keeps it
```

## Volatile reads and writes

A device is driven through registers at fixed addresses, which the C compiler must not merge, move or leave
out. `Memory.readVolatile(place)` and `Memory.writeVolatile(place, value)` read or write a place in raw memory
exactly once, where they are written:

```sg
struct Device {
    uint status
    uint control
}

void main() {
    // A device's registers; here plain memory stands in for them.
    var regs = Memory.allocate<Device>(1)
    unsafe {
        Memory.writeVolatile(regs[0].control, 1)
        Memory.writeVolatile(regs[0].status, 0x80)
    }
    var status = unsafe Memory.readVolatile(regs[0].status)
    print($"{status:x2}")
    unsafe Memory.free(regs)
}
```

It prints:

```text
80
```

Volatile orders nothing between tasks; that is what atomic values are for. On a real machine the address comes
from a number, `unsafe (raw Device)0x1000_0000`, and the kernel described in
[Building programs](./14-building.md) drives its serial port and timer this way.

## Keeping unsafe code small

- Put raw memory and calls of C inside one module, and give the rest of the program SG's types: slices,
  `Result`, `Disposable` owners. A function of the module that is not `public` cannot be called from outside.
- Write the check in front of the `unsafe` line, as `set` does above, so the line itself is plainly correct.
- `Memory.zeroSecurely(ref key)` writes zeros over a secret in a way the C compiler keeps, with no `unsafe`.
- Run the tests in the interpreter, which checks raw memory, and the compiled build under the sanitizers.
