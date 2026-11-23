---
title: Tasks, locks and channels
description: Running work at the same time without data races, with tasks, locked values, channels, async functions and atomic values.
sidebar_position: 10
---

# Tasks, locks and channels

SG runs work at the same time with tasks. The rules that make one value safe, one owner and borrows that end,
also make tasks safe: a task owns what it is given, values that several tasks change sit behind a lock, and
the compiler refuses a program in which two tasks could reach the same memory without one.

## Starting a task and waiting for it

`Task.run(work, arguments...)` starts `work` as a task and gives a `Task<R>`, where `R` is what `work` returns.
`wait()` waits for the task to end and gives that value.

```sg
long countPrimes(long from, long to) {
    long count = 0
    for n in from..to {
        if n < 2 {
            continue
        }
        var prime = true
        var d = 2
        while (long)d * d <= n {
            if n % d == 0 {
                prime = false
                break
            }
            d += 1
        }
        if prime {
            count += 1
        }
    }
    return count
}

void main() {
    var low = Task.run(countPrimes, 0, 50000)
    var high = Task.run(countPrimes, 50000, 100000)
    var both = low.wait() + high.wait()
    print($"{both} primes below 100000")
}
```

It prints:

```text
9592 primes below 100000
```

Every task is waited for, on every path through the function. A task left behind would lose its error and
could be cut off halfway, so the compiler says where it was forgotten:

```sg error
int twice(int n) {
    return n * 2
}

void main() {
    var task = Task.run(twice, 21)
    if true {
        return
    }
    print(task.wait())
}
```

```text
error: `task` is not waited for at this `return`
  every task is waited for: its error is not lost, and no work is cut off
  --> example.sg:8:9
 8 |         return
   |         ^^^^^^
  the task is kept here:
  --> example.sg:6:9
 6 |     var task = Task.run(twice, 21)
   |         ^^^^
  hint: `task.wait()`
```

The one exception is a way out with an error (`try`, `return Error(...)`): the tasks left there are let go,
then waited for. A wait in them that can say it was cancelled, on the network or a file, or a delay or a
channel given a token, ends at once with `Cancelled`, and each task goes out by its own way of an error, so a
server task that waits for clients does not hold the program for ever once the function has failed
([An exit with an error lets the tasks go](../../library/tasks.md#an-exit-with-an-error-lets-the-tasks-go)).

What a task is given, and what it gives back, owns all it holds. A borrow cannot go to a task, since the task
could outlive what it borrows from:

```sg error
int total(read List<int> numbers) {
    var sum = 0
    for n in numbers {
        sum += n
    }
    return sum
}

void main() {
    var numbers = List<int> { 1, 2, 3 }
    var task = Task.run(total, read numbers)
    print(task.wait())
}
```

```text
error: `total` takes a `read List<int>` here, which cannot go to another task: a `read List<int>` is a borrow, and the task could outlive what it borrows from
  --> example.sg:11:32
 11 |     var task = Task.run(total, read numbers)
    |                                ^^^^^^^^^^^^
  hint: give the task a value of its own: a `List<int>` to own, or a `shared List<int>` that both keep
```

Give the task the list itself (it moves), or `share` a `shared` value that several tasks only read. The same
rule refuses a raw pointer and a `shared checked` value, whose checks belong to one task.

In the interpreter a task runs when it is waited for, so a program prints the same on every run. A compiled
program starts a thread of the system for each task at once, so lines that tasks print may come in any
order there. The programs on this page print from one place, which keeps their output the same.

## Values that several tasks change

`new shared locked x` makes a value that several tasks own and change. What it holds is reached only inside
`lock v as name { ... }`, where `name` is a `ref` to it for the block:

```sg
struct Tally {
    int words
    int lines
}

void count(shared locked Tally tally, string line) {
    var words = 0
    for _ in line.split(" ") {
        words += 1
    }
    lock tally as t {
        t.words += words
        t.lines += 1
    }
}

void main() {
    var tally = new shared locked Tally { words = 0, lines = 0 }
    var a = Task.run(count, share tally, "the quick brown fox")
    var b = Task.run(count, share tally, "jumps over")
    var c = Task.run(count, share tally, "the lazy dog")
    a.wait()
    b.wait()
    c.wait()
    lock tally as t {
        print($"{t.lines} lines, {t.words} words")
    }
}
```

It prints:

```text
3 lines, 9 words
```

The work outside the lock runs in parallel; only the two additions take turns. A `lock` block never waits:
inside one there is no other `lock`, no `wait`, no send or receive on a channel, and no call of a function
that does one. A task that holds a lock therefore always lets it go, and locks cannot deadlock.

```sg error
void main() {
    var counter = new shared locked 0
    var other = new shared locked 0
    lock counter as c {
        lock other as o {
            o += c
        }
    }
}
```

```text
error: a `lock` block does not take another lock
  a task that holds a lock never waits, so it always lets the lock go: locks and waits cannot deadlock
  --> example.sg:5:9
 5 |         lock other as o {
   |         ^^^^^^^^^^^^^^^^^
  hint: lock both in one block, which takes them in a fixed order: `lock a as x, b as y { ... }`
```

Two values locked together are taken in a fixed order, whatever order the block names them in, so two
transfers that go opposite ways cannot hold one lock each:

```sg
struct Account {
    long balance
}

void transfer(shared locked Account from, shared locked Account to, long amount) {
    lock from as a, to as b {
        a.balance -= amount
        b.balance += amount
    }
}

void main() {
    var alice = new shared locked Account { balance = 100 }
    var bob = new shared locked Account { balance = 50 }
    var one = Task.run(transfer, share alice, share bob, 30)
    var two = Task.run(transfer, share bob, share alice, 10)
    one.wait()
    two.wait()
    lock alice as a, bob as b {
        print($"alice {a.balance}, bob {b.balance}")
    }
}
```

It prints:

```text
alice 80, bob 70
```

## Waiting for a condition

`lock v as name when condition { ... }` waits, with the lock let go, until the condition holds, then runs the
block under the lock. The condition is looked at again each time a `lock` block on the value ends, so no task
has to wake another by hand. A `done` field that the giver sets tells the takers that no more will come:

```sg
struct Queue {
    List<int> items
    bool done
}

// Takes items until the queue is empty and the giver says no more will come.
long take(shared locked Queue queue) {
    long sum = 0
    while true {
        int? item = null
        lock queue as q when q.items.length() > 0 || q.done {
            item = q.items.removeLast()
        }
        if item is null {
            return sum
        }
        sum += item
    }
    return sum
}

void main() {
    var queue = new shared locked Queue { items = List<int>(), done = false }
    var first = Task.run(take, share queue)
    var second = Task.run(take, share queue)
    for i in 1..101 {
        lock queue as q {
            q.items.add(i)
        }
    }
    lock queue as q {
        q.done = true
    }
    print(first.wait() + second.wait())
}
```

It prints:

```text
5050
```

The condition reads only the locked values: an atomic value, the clock or input would change without a
`lock` block ending, and nothing would look again.

## Channels

A channel carries items from tasks that send to tasks that receive. `Channel<T>(n)` is the end that receives,
with room for `n` items; `sender()` gives a `Sender<T>`, and `clone()` gives one more end of either kind.
`send` waits while the channel is full; `receive` waits while it is empty and gives `null` once every sender
has ended and nothing is left. There is no `close` to forget: a channel ends when its last sender does, and
`for item in channel` receives until then.

```sg
void produce(Sender<int> jobs, int count) {
    for i in 1..count + 1 {
        _ = jobs.send(i)
    }
}

void square(Channel<int> jobs, Sender<long> results) {
    for job in jobs {
        _ = results.send((long)job * job)
    }
}

void main() {
    var jobs = Channel<int>(8)
    var results = Channel<long>(8)
    var giver = Task.run(produce, jobs.sender(), 100)
    var one = Task.run(square, jobs.clone(), results.sender())
    var two = Task.run(square, jobs, results.sender())
    long total = 0
    for value in results {
        total += value
    }
    giver.wait()
    one.wait()
    two.wait()
    print(total)
}
```

It prints:

```text
338350
```

Each item goes to one of the receivers. `send` gives `false` when no end receives any more, and the item is
dropped. Since a `for` over a channel ends only when every sender has ended, a sender kept in the same
function would make it wait for ever, and the compiler refuses that:

```sg error
void main() {
    var words = Channel<string>(4)
    var sender = words.sender()
    _ = sender.send("one".clone())
    _ = sender.send("two".clone())
    for word in words {
        print(word)
    }
}
```

```text
error: this loop ends only when every sender of `words` has ended, and `sender` lives on in this function: it would wait for ever
  --> example.sg:6:17
 6 |     for word in words {
   |                 ^^^^^
  `sender` is made here:
  --> example.sg:3:9
 3 |     var sender = words.sender()
   |         ^^^^^^
  hint: give the last sender away, `Task.run(produce, sender)`, or make each sender where it is given: `Task.run(produce, words.sender())`
```

Given away, the sender ends with the function that took it, and the loop ends with it:

```sg
void fill(Sender<string> words) {
    _ = words.send("one".clone())
    _ = words.send("two".clone())
}

void main() {
    var words = Channel<string>(4)
    fill(words.sender())
    for word in words {
        print(word)
    }
    print(words.receive() ?? "the channel has ended")
}
```

It prints:

```text
one
two
the channel has ended
```

## Async functions

A task started with `Task.run` has a thread of its own. An `async` function is lighter: a call gives a
`Task<R>` that has started and runs until it first waits, then the caller goes on. `await t` waits for it
without holding the thread, so a few threads run many such tasks. `Task.delay(ms)` is a task that ends after so
many milliseconds, and `main` may be async too.

```sg
async int brew(string tea, int ms) {
    print($"{tea}: water on")
    await Task.delay(ms)
    print($"{tea}: ready")
    return ms / 100
}

async void main() {
    var green = brew("green", 300)
    var black = brew("black", 100)
    print("both started")
    var cups = await green + await black
    print($"{cups} cups")
}
```

It prints:

```text
green: water on
black: water on
both started
black: ready
green: ready
4 cups
```

An async function never holds its thread, since that would stop the other tasks that share it. So `wait` is
an error there, and so is input and output that is not async, followed through the functions it calls:

```sg error
async void main() {
    var text = File.readText("notes.txt")
    print(text is Ok(_))
}
```

```text
error: `File.readText` holds the thread until it is done, and an async function does not hold its thread
  the tasks of async functions share a few threads: one that holds its thread keeps the others from running
  --> example.sg:2:21
 2 |     var text = File.readText("notes.txt")
   |                     ^^^^^^^^
  hint: `await File.readTextAsync("notes.txt")` does the same work without holding the thread
```

`await f(read xs)` runs `f` as part of the awaiting task, so it may borrow; work that must hold a thread runs
apart with `await Task.run(work, x)`. Channels have `sendAsync` and `receiveAsync`, a locked value
`await lock ... when`. Tasks kept in a `List<Task<R>>` are awaited together with `Task.whenAll(tasks)`, which
gives their values in the order of the list, or one at a time as they end with `Task.whenAny(ref tasks)`.
Tasks that give nothing, such as the connections of a server, go in a `TaskGroup`, which lets go of each as
it ends: `group.add(task)`, `await group.whenRoom()` while a limit of them run, and `await group.whenAll()`
at the end ([Tasks](../../library/tasks.md#taskgroup) shows a server written with it).

## Cancellation

A `CancellationSource` cancels; its `token()` is handed to the work, which looks at it where it chooses:
`token.isCancelled()`, or `try token.check()`, which gives `Error(Cancelled { })`. `CancellationSource.after(ms)`
cancels itself on the clock, and `Task.delay(ms, read token)` ends early when its token is cancelled.

```sg
async Result<void, Cancelled> ticker(int every, CancellationToken token) {
    var count = 0
    while true {
        try await Task.delay(every, read token)
        count += 1
        print($"tick {count}")
    }
}

Result<long, Cancelled> sumUpTo(long limit, read CancellationToken token) {
    long sum = 0
    for i in 0..limit {
        if i % 1000 == 0 {
            try token.check()
        }
        sum += i
    }
    return Ok(sum)
}

async void main() {
    var timer = CancellationSource.after(1000)
    if await ticker(400, timer.token()) is Error(cause) {
        print($"the ticker stopped: {cause}")
    }

    var source = CancellationSource()
    var token = source.token()
    print(sumUpTo(10000, read token) ?? -1)
    source.cancel()
    print(sumUpTo(10000, read token) ?? -1)
}
```

It prints:

```text
tick 1
tick 2
the ticker stopped: Cancelled
49995000
-1
```

A token may also come last on a channel's `send` and `receive`, which then give a `Result` whose error is
`Cancelled`, and on the async calls of files, the network and other programs, where a cancel ends the wait. `source.cancelAfter(ms)` sets the time a source cancels itself again, from now: set before each read of a
connection, it closes one that stays silent too long. `CancellationSource.linked(read token)` makes a source that is
cancelled when the token's source is, and by itself: a function given its caller's token sets a limit of its
own beside it.

```sg
async Result<void, Cancelled> waitWithin(read CancellationToken token, int limit) {
    var mine = CancellationSource.linked(read token)
    mine.cancelAfter(limit)
    try await Task.delay(10000, read mine.token())
    return Ok()
}

async Result<void, IoError> main() {
    var caller = CancellationSource()
    match await waitWithin(read caller.token(), 10) {
        Ok => print("waited")
        Error(_) => print($"its own limit ended the wait; the caller's source is cancelled: {caller.isCancelled()}")
    }
    return Ok()
}
```

```text
its own limit ended the wait; the caller's source is cancelled: false
```

## Atomic values

For a counter or a flag, a lock is more than is needed. `atomic T` holds an integer, a `bool`, a `float`, a
`double` or a raw pointer that tasks change without a lock, through a `read` borrow or a `shared` owner:
`load`, `store`, `exchange`, `compareExchange`, `fetchAdd`, `fetchSub`, `fetchAnd`, `fetchOr` and `fetchXor`.
Each gives the value before it changed.

```sg
void count(shared atomic long hits, int times) {
    for _ in 0..times {
        _ = hits.fetchAdd(1, MemoryOrder.Relaxed)
    }
}

void main() {
    shared atomic long hits = new shared atomic 0
    var tasks = List<Task<void>>()
    for _ in 0..4 {
        tasks.add(Task.run(count, share hits, 10000))
    }
    Task.whenAll(tasks).wait()
    print(hits.load())
}
```

It prints:

```text
40000
```

Every operation is sequentially consistent unless an order is written after its values. `Relaxed` is enough
for a count that is read once the tasks have ended; `Release` on a store and `Acquire` on a load make what was
written before the store seen after the load:

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

It prints:

```text
42
```

An order the operation cannot keep is a compile error, not a surprise on another processor:

```sg error
void main() {
    atomic int ready = atomic 0
    ready.store(1, MemoryOrder.Acquire)
}
```

```text
error: `store` cannot keep the order `Acquire`
  a store writes, so its order is `Relaxed`, `Release` or `SequentiallyConsistent`
  --> example.sg:3:20
 3 |     ready.store(1, MemoryOrder.Acquire)
   |                    ^^^^^^^^^^^^^^^^^^^
```

`fetchAdd` follows the rule of overflow (a panic in a debug build, a wrap in a release build), and
`wrappingFetchAdd` wraps in both. `Atomic.fence(order)` orders memory without a value. With atomic values and
no `unsafe`, SG can build a queue without locks for many producers and consumers.
