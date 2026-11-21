---
title: Tasks
description: Task, Channel and Sender, shared locked values, atomics and cancellation, with their signatures.
sidebar_position: 6
---

# Tasks

This page lists the types of concurrent work and their methods. Why they are shaped as they are (the thread
rule, why a `lock` block never waits, why every task is waited for) is explained in the chapter
[Concurrency](../learn/book/10-concurrency.md) of the book.

What goes to another task owns all it holds: no borrow, no `shared checked` value, no `Task`, no raw pointer.
That is the thread rule, and it is checked when the program is compiled, also inside fields. Every type on this
page is built into the language, but `MemoryOrder`, which `std` defines.

## `Task<R>`

| Signature | What it does |
|---|---|
| `Task<R> Task.run(work, args...)` | Starts `work(args...)` as a task; `R` is what `work` returns. `work` is a function or a `Func` |
| `R wait(this)` | Waits for the task and gives what it returned; `try t.wait()` passes its error on |
| `await t` | The same in an async function, without holding the thread |
| `Task<void> Task.delay(int ms)` | A task that ends after so many milliseconds |
| `Task<Result<void, Cancelled>> Task.delay(int ms, read CancellationToken token)` | One that ends early, with `Cancelled`, when the token is cancelled |
| `Task<List<R>> Task.whenAll(List<Task<R>> tasks)` | What every task gave, in the order of the list |
| `Task<R> Task.whenAny(ref List<Task<R>> tasks)` | What the first task to end gave; it is taken out of the list |

An `async` function gives a `Task<R>` when it is called, as in C#: `async int count(string path)` is called as
`var t = count(path)`, and the task runs until it first waits. A list of tasks has only `add` and `length`, and
is given to `Task.whenAll` or emptied by a loop `while tasks.length() > 0`.

A task kept in a variable is waited for on every path, so no error is lost and no work cut off:

```sg error
int one() {
    return 1
}

void main() {
    var t = Task.run(one)
    if false {
        print(t.wait())
    }
}
```

```text
error: `t` is waited for on some paths but not on all of them
  wait for it on every path, or once where the paths meet
  every task is waited for: its error is not lost, and no work is cut off
```

```sg
async int slow(int n, int ms) {
    await Task.delay(ms)
    return n * 10
}

int square(int x) {
    return x * x
}

async void main() {
    var first = Task.run(square, 7)
    print(await first)

    var tasks = List<Task<int>>()
    tasks.add(slow(1, 30))
    tasks.add(slow(2, 10))
    tasks.add(slow(3, 20))
    print(await Task.whenAll(tasks))

    var more = List<Task<int>>()
    more.add(slow(4, 200))
    more.add(slow(5, 10))
    while more.length() > 0 {
        print(await Task.whenAny(ref more))
    }
}
```

It prints:

```text
49
[10, 20, 30]
50
40
```

## `Channel<T>` and `Sender<T>`

A channel carries items from its senders to its receivers, in the order they were sent. It has no `close`:
it ends once every sender has ended and nothing is left.

| Signature | What it does |
|---|---|
| `Channel<T>(nint capacity)` | The end that receives, with room for so many items (at least one) |
| `Sender<T> sender(read this)` | An end that sends |
| `clone(read this)` | One more end of either kind; several receivers share the items |
| `bool send(read this, T item)` on `Sender<T>` | Waits while the channel is full; `false` when no end receives any more (the item is dropped) |
| `T? receive(read this)` | Waits while it is empty; `null` once every sender has ended and nothing is left |
| `for item in jobs` | Receives until then |
| `sendAsync(item)`, `receiveAsync()` | The same in an async function, awaited |

A token may come last on all four (`tx.send(x, read token)`), which then give a `Result` whose error is
`Cancelled`: `Result<bool, Cancelled>` for `send`. An item is never a `T?`, since `null` means the end.

```sg
void produce(Sender<int> out, int count) {
    for i in 0..count {
        if !out.send(i * i) {
            return
        }
    }
}

int consume(Channel<int> jobs) {
    var total = 0
    for job in jobs {
        total += job
    }
    return total
}

void main() {
    var jobs = Channel<int>(4)
    var sender = jobs.sender()
    var other = sender.clone()
    var p1 = Task.run(produce, sender, 5)
    var p2 = Task.run(produce, other, 3)
    var c = Task.run(consume, jobs)
    p1.wait()
    p2.wait()
    print(c.wait())
}
```

It prints:

```text
35
```

## `shared locked T`

A value that several tasks own and change. What it holds follows the thread rule, and it is reached only inside
`lock`.

| Form | What it does |
|---|---|
| `new shared locked x` | Puts `x` on the heap, owned by this task |
| `share v` | One more owner, to give to another task |
| `lock v as c { ... }` | `c` is a `ref T` for the block |
| `lock a as x, b as y { ... }` | Takes two together, in a fixed order |
| `lock v as c when condition { ... }` | Waits, with the lock let go, until the condition holds |
| `await lock v as c when condition { ... }` | The same in an async function |

A `lock` block does not wait: no other `lock`, no `wait`, no send or receive, and no call of a function that
does one, followed through the calls. So locks cannot deadlock. A `when` is woken each time a `lock` block on
the value ends, so nothing is told by hand.

```sg
struct Queue {
    List<int> items
    bool done
}

int drain(shared locked Queue queue) {
    var total = 0
    while true {
        lock queue as q when q.items.length() > 0 || q.done {
            if q.items.length() == 0 {
                return total
            }
            total += q.items.removeLast() ?? 0
        }
    }
}

void main() {
    var queue = new shared locked Queue { items = List<int>(), done = false }
    var worker = Task.run(drain, share queue)
    for i in 1..5 {
        lock queue as q {
            q.items.add(i)
        }
    }
    lock queue as q {
        q.done = true
    }
    print(worker.wait())
}
```

It prints:

```text
10
```

```sg error
struct Box {
    int n
}

void main() {
    var a = new shared locked Box { n = 1 }
    var b = new shared locked Box { n = 2 }
    lock a as x {
        lock b as y {
            x.n += y.n
        }
    }
}
```

```text
error: a `lock` block does not take another lock
  a task that holds a lock never waits, so it always lets the lock go: locks and waits cannot deadlock
  --> example.sg:9:9
 9 |         lock b as y {
   |         ^^^^^^^^^^^^^
  hint: lock both in one block, which takes them in a fixed order: `lock a as x, b as y { ... }`
```

## `atomic T`

An integer, a `bool`, a `float`, a `double` or a `raw T` that tasks change without a lock. `atomic 0` makes one;
it lies wherever a value can (a field, a local, an array), and tasks share one as `new shared atomic 0`, since
every method works through `read`.

| Signature | What it does |
|---|---|
| `T load(read this)`, `void store(read this, T value)` | Reads, writes |
| `T exchange(read this, T value)` | Writes and gives the value before |
| `Result<T, T> compareExchange(read this, T expected, T desired)`, `compareExchangeWeak` | Writes `desired` if it holds `expected`: `Ok` with the value written over, `Error` with the one found. The weak one may fail for no reason, in a loop |
| `T fetchAdd(read this, T n)`, `fetchSub` | On numbers; the value before. Overflow follows the rule of the numbers |
| `T wrappingFetchAdd(read this, T n)` | Wraps in every build |
| `T fetchAnd(read this, T n)`, `fetchOr`, `fetchXor` | On integers and `bool` |
| `Atomic.fence(MemoryOrder order)`, `Atomic.compilerFence(order)`, `Atomic.spinLoop()` | A fence, one for the compiler only, a hint inside a spin loop |

Each operation is sequentially consistent unless an order is written last, a variant of `MemoryOrder`:
`Relaxed`, `Acquire`, `Release`, `AcquireRelease`, `SequentiallyConsistent`. `compareExchange` takes two, for
success and for failure. An order the operation cannot keep is a compile error.

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

    var count = atomic 9
    match count.compareExchange(9, 10) {
        Ok(was) => print($"wrote over {was}")
        Error(found) => print($"found {found}")
    }
    print(count.compareExchange(9, 11) is Error(10))
    print(count.fetchOr(0x100, MemoryOrder.AcquireRelease))
    print(count.load(MemoryOrder.Acquire))
    var top = atomic byte.MaxValue
    print(top.wrappingFetchAdd(1))
}
```

It prints:

```text
1000
wrote over 9
true
10
266
255
```

```sg error
void main() {
    var count = atomic 5
    print(count.load(MemoryOrder.Release))
}
```

```text
error: `load` cannot keep the order `Release`
  a load reads, so its order is `Relaxed`, `Acquire` or `SequentiallyConsistent`
```

## `CancellationSource` and `CancellationToken`

A source cancels; its tokens are looked at by the work, where the work chooses.

| Signature | What it does |
|---|---|
| `CancellationSource()` | A source |
| `CancellationSource CancellationSource.after(int ms)` | One that cancels by itself after so many milliseconds |
| `CancellationToken token(read this)` | A token of the source |
| `void cancel(read this)` | Cancels every token of it; again is nothing |
| `bool isCancelled(read this)` | On a source and on a token |
| `Result<void, Cancelled> check(read this)` on a token | `Error(Cancelled { })` once it is cancelled |
| `CancellationToken clone(read this)` | One more token |
| `CancellationToken.None` | A token never cancelled |

In a function that returns an `IoError`, `try token.check()` turns `Cancelled` into `IoError.Cancelled(...)`.
The async calls of files, the network, channels, `Task.delay` and other programs take a `read
CancellationToken` last, which may be left out: a function of the program does the same with a default value,
`read CancellationToken token = CancellationToken.None`.

```sg
Result<int, IoError> sum(read List<int> xs, read CancellationToken token) {
    var total = 0
    for x in xs {
        try token.check()
        total += x
    }
    return Ok(total)
}

async void main() {
    var source = CancellationSource()
    var token = source.token()
    var xs = List<int> { 1, 2, 3 }
    print(sum(read xs, read token) ?? -1)
    source.cancel()
    match sum(read xs, read token) {
        Ok(total) => print(total)
        Error(Cancelled(_)) => print("stopped")
        Error(cause) => print(cause)
    }
    var deadline = CancellationSource.after(10)
    var waiting = deadline.token()
    match await Task.delay(1000, read waiting) {
        Ok => print("waited")
        Error(_) => print("cut short")
    }
    print(CancellationToken.None.isCancelled())
}
```

It prints:

```text
6
stopped
cut short
false
```
