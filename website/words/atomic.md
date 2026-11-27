---
name: atomic
kind: keyword
summary: A number or a bool that tasks change without a lock, one step at a time.
see: shared, locked, lock
learn: ../docs/learn/book/10-concurrency.md#atomic-values
library: ../docs/library/tasks.md#atomic-t
---

## Forms

| Form | What it is |
|---|---|
| `atomic T` | the type: `T` is an integer, a `bool`, a `float`, a `double` or a `raw T` |
| `atomic 0` | a value of it |
| `new shared atomic 0` | one that several tasks own |
| `x.load()`, `x.store(v)` | reads it, writes it |
| `x.fetchAdd(n)`, `x.exchange(v)` | changes it in one step, and gives the value before |
| `x.compareExchange(expected, desired)` | writes only if it still holds `expected` |
| `x.load(MemoryOrder.Acquire)` | with an order written last |

## What it means

An atomic value is changed by the processor in one step, so two tasks that change it at the same time never lose
one of the changes. For a counter or a flag that is cheaper than a lock. Every method works through `read`, so
tasks share one as a `shared atomic T` and need no `lock` around it.

Each operation is sequentially consistent unless an order is written last: `Relaxed`, `Acquire`, `Release`,
`AcquireRelease` or `SequentiallyConsistent`, the variants of `MemoryOrder`. An order the operation cannot keep
is a compile error.

## Examples

### A counter

```sg
void main() {
    var count = atomic 0
    _ = count.fetchAdd(1)
    _ = count.fetchAdd(1)
    print(count.load())
}
```

```text
2
```

`fetchAdd` gives the value before it added. `_ =` says that value is not needed: SG does not let a value be
dropped without a word.

### Shared by two tasks

```sg
void bump(int times, shared atomic int total) {
    for _ in 0..times {
        _ = total.fetchAdd(1)
    }
}

void main() {
    var total = new shared atomic 0
    var a = Task.run(bump, 500, share total)
    var b = Task.run(bump, 500, share total)
    a.wait()
    b.wait()
    print(total.load())
}
```

```text
1000
```

`share total` gives each task one more owner of the same value. No addition is lost.

### Write only if nothing changed it

```sg
void main() {
    var state = atomic 9
    match state.compareExchange(9, 10) {
        Ok(was) => print($"wrote over {was}")
        Error(found) => print($"found {found}")
    }
    match state.compareExchange(9, 11) {
        Ok(was) => print($"wrote over {was}")
        Error(found) => print($"found {found}")
    }
}
```

```text
wrote over 9
found 10
```

The second call expected 9 but found 10, so it wrote nothing. This is the step under most structures built
without locks.

### A value handed from one task to another

```sg
void publish(shared atomic int data, shared atomic bool ready) {
    data.store(42, MemoryOrder.Relaxed)
    ready.store(true, MemoryOrder.Release)
}

void main() {
    var data = new shared atomic 0
    var ready = new shared atomic false
    var writer = Task.run(publish, share data, share ready)
    while !ready.load(MemoryOrder.Acquire) {
        Atomic.spinLoop()
    }
    print(data.load(MemoryOrder.Relaxed))
    writer.wait()
}
```

```text
42
```

`Release` on the store and `Acquire` on the load make what was written before the store seen after the load.
`Atomic.spinLoop()` tells the processor the loop is waiting.

## What the compiler refuses

### A change in two steps

```sg error
void main() {
    var count = atomic 5
    count += 1
}
```

```text
error: `+=` would read `count` and write it in two steps, and another task could change it between them
  --> example.sg:3:5
 3 |     count += 1
   |     ^^^^^^^^^^
  hint: one step: `_ = count.fetchAdd(1)`
```

### An order the operation cannot keep

```sg error
void main() {
    var count = atomic 5
    print(count.load(MemoryOrder.Release))
}
```

```text
error: `load` cannot keep the order `Release`
  a load reads, so its order is `Relaxed`, `Acquire` or `SequentiallyConsistent`
  --> example.sg:3:22
 3 |     print(count.load(MemoryOrder.Release))
   |                      ^^^^^^^^^^^^^^^^^^^
```

### A value the processor cannot change in one step

```sg error
void main() {
    var name = atomic "Pasha"
}
```

```text
error: an atomic value is an integer, a `bool`, a `float`, a `double` or a raw pointer, and this is `string`
  what the processor changes in one step; a larger value that several tasks change is a `shared locked T`
  --> example.sg:2:16
 2 |     var name = atomic "Pasha"
   |                ^^^^^^^^^^^^^^
```
