---
title: Tasks
description: Task, TaskGroup, Channel and Sender, shared locked values, atomics and cancellation, with their signatures.
sidebar_position: 6
---

# Tasks

This page lists the types of concurrent work and their methods. Why they are shaped as they are (the thread
rule, why a `lock` block never waits, why every task is waited for) is explained in the chapter
[Concurrency](../learn/book/10-concurrency.md) of the book.

What goes to another task owns all it holds: no borrow, no `shared checked` value, no `Task` or `TaskGroup`, no raw pointer.
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

### An exit with an error lets the tasks go

The one way out that leaves a task unawaited is an error (`try`, `return Error(...)`). There the tasks
are let go, and then waited for: every wait in them that can say it was cancelled ends at once with
`Cancelled` (the network, files and pipes, other programs, a name looked up, and a delay or a channel
given a token), and so does every such wait they start afterwards; a task they await is let go in turn.
Each goes out by its own way of an error, so nothing is cut in the middle of a line, and none holds the
program for ever. A delay or a channel without a token cannot say it was cancelled, so it runs on and is
waited for, as is work that never waits and a task of `Task.run` held in a call that blocks; give those a
token where they must end.

`Task.whenAll` waits for every task and gives what each gave, errors among them. To stop at the first
error, empty the list with `Task.whenAny` and `try`: the tasks left in it are let go.

```sg
async Result<int, IoError> waits(TcpListener listener) {
    match await listener.acceptAsync() {
        Ok(_) => return Ok(1)
        Error(e) => {
            print($"waits: {e}")
            return Error(e)
        }
    }
}

async Result<int, IoError> fails() {
    _ = try await File.readTextAsync("no such file")
    return Ok(2)
}

async Result<void, IoError> run() {
    var listener = try TcpListener.bind("127.0.0.1:0")
    var tasks = List<Task<Result<int, IoError>>>()
    tasks.add(waits(listener))
    tasks.add(fails())
    while tasks.length() > 0 {
        print(try await Task.whenAny(ref tasks))
    }
    return Ok()
}

void main() {
    if run().wait() is Error(e) {
        print($"run: {e}")
    }
}
```

It prints:

```text
waits: cancelled
run: not found
```

## `TaskGroup`

A group holds tasks that give nothing and lets go of each as soon as it ends, so a server that starts a task
for every connection keeps only the connections that are open. It is waited for as a task is: a group whose
tasks are not waited for on every path is a compile error, and an exit with an error lets them go and waits
for them, as above.

| Signature | What it does |
|---|---|
| `TaskGroup()` | A group with no limit |
| `TaskGroup(int limit)` | A group whose `whenRoom` waits while `limit` of its tasks run (at least one) |
| `void add(read this, Task<void> task)` | The task is the group's from now on; `add` neither waits nor refuses |
| `nint count(read this)` | How many of its tasks have not ended |
| `Task<void> whenRoom(read this)` | Ends once fewer than the limit run; at once without a limit |
| `Task<Result<void, Cancelled>> whenRoom(read this, read CancellationToken token)` | Ends early, with `Cancelled`, when the token is cancelled |
| `Task<void> whenAll(this)` | Takes the group, and ends once every task in it has ended |
| `Task<Result<void, Cancelled>> whenAll(this, read CancellationToken token)` | When the token is cancelled, lets the tasks left go and still waits for them; `Cancelled` then says they were cut |

`whenRoom` and `whenAll` are awaited, or waited for with `wait`, where they are written. A task that gives
something cannot be added, since what it gives would be lost: an `async void` function around it does what is
to be done with it, its error among it. A group is lent to a function with `read` (`add` works through it),
or given to one that waits for it, but it never goes to another task.

```sg
async void work(string name, int ms) {
    var own = CancellationSource()
    if await Task.delay(ms, read own.token()) is Error(_) {
        print($"{name} was cut")
        return
    }
    print($"{name} done")
}

async void main() {
    var group = TaskGroup(2)
    for i in 0..3 {
        await group.whenRoom()
        group.add(work($"job {i}".toString(), 100 * (i + 1)))
        print($"job {i} started, {group.count()} running")
    }
    await group.whenAll()

    var open = TaskGroup()
    open.add(work("quick", 50))
    open.add(work("slow", 5000))
    var grace = CancellationSource.after(500)
    if await open.whenAll(read grace.token()) is Error(_) {
        print("the tasks left were cut, and have ended")
    }
}
```

It prints:

```text
job 0 started, 1 running
job 1 started, 2 running
job 0 done
job 2 started, 2 running
job 1 done
job 2 done
quick done
slow was cut
the tasks left were cut, and have ended
```

### Writing a server

A server takes a connection, starts a task for it and takes the next. Four things keep it standing when the
clients misbehave or the machine is asked to stop, and none of them costs a server that does not use it:

- **A group for the connections.** `TaskGroup` lets go of a connection's task, and of the memory of its frame,
  when the connection ends. A `List<Task<R>>` emptied with `Task.whenAny` keeps every task until it is taken,
  and looks at the whole list each time.
- **A limit on connections.** `TaskGroup(10000)` and `await group.whenRoom()` before each `acceptAsync`: past
  the limit the server takes no new connection, and those that come wait in the system's queue of the
  listener, not in the server's memory.
- **A limit on time.** A connection that sends nothing, or a byte now and then and never a whole request,
  holds its buffer and a file of the system for ever. One `CancellationSource` for each connection, its token
  given to `receiveAsync`, and `idle.cancelAfter(ms)`: set before every read it closes a client that stays
  silent longer; set only when a request begins (nothing of one waits in the buffer) it also closes one that
  sends a request a byte at a time. Setting a later time is one atomic write.
- **A graceful end.** On a signal the accept loop stops (its token is cancelled), and `await
  group.whenAll(read grace)` waits for the connections that are open; once `grace` is cancelled it cuts
  those left, and still waits for them to end. Each connection goes out by its own error, so nothing is left
  half written.

A request is read where it lies: `conn.reader()` gives a block that starts at 4 KB and grows only as far as a
request needs, `buffered()` lends it, the parser takes what is whole, `consume(n)` gives that out, and
`fillAsync` reads more ([Reading from the block](files.md#reading-from-the-block)). An answer made of a head and a
body built apart goes out with `sendPiecesAsync(read pieces)`, a `List<read byte[]>` cleared and filled again in
every round, with no copy into one buffer. The store and the HTTP server of the measurements read this way, in
the time and memory of a buffer of their own.

An error of one connection stays in its task: the group takes tasks that give nothing, so the function of the
connection decides what to do with it (log it, say), and the server goes on.

```sg
async Result<void, IoError> serve(TcpStream conn, int idleLimit) {
    var idle = CancellationSource()
    var token = idle.token()
    var buffer = byte[4096](0)
    while true {
        idle.cancelAfter(idleLimit)
        var n = try await conn.receiveAsync(ref buffer, read token)
        if n == 0 {
            return Ok()
        }
        try await conn.sendAsync(buffer[0..n])
    }
}

async void connection(TcpStream conn) {
    if await serve(conn, 30000) is Error(e) {
        var line = $"a connection ended: {e}".toString()
        Log.warn(read line)
    }
}

async Result<void, IoError> listen(read string address, read CancellationToken stop) {
    var listener = try TcpListener.bind(read address)
    var group = TaskGroup(10000)
    while await group.whenRoom(read stop) is Ok {
        match await listener.acceptAsync(read stop) {
            Ok(conn) => group.add(connection(conn))
            Error(Cancelled(_)) => break
            Error(_) => await Task.delay(10)
        }
    }
    var grace = CancellationSource.after(10000)
    _ = await group.whenAll(read grace.token())
    return Ok()
}

async void main() {
    // Told to stop after a moment, where a server is told by a signal.
    var stop = CancellationSource.after(100)
    if await listen("127.0.0.1:0", read stop.token()) is Ok {
        print("stopped")
    }
}
```

It prints:

```text
stopped
```

Measured on Linux against the servers as they were ([Servers](../benchmarks/server.md#a-group-of-tasks-slow-clients-and-a-limit-on-a-request)):
the group costs nothing (1,361,000 answers a second with 50 connections, against 1,355,000 with a list), and an
idle connection still holds 4.98 KB. A limit on the time of a request costs 3 to 4% of the answers and a tenth
on the p99. Beside 10,000 clients that send a byte a second, it closes all of them within its 5 seconds, gives
their 10,000 files back and answers 100 busy connections within 807 µs at p99, where the same server without a
limit takes 2.75 ms, `tokio` 2.47 ms and C 5.36 ms.

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
| `void cancelAfter(read this, int ms)` | Cancels it `ms` from now, in place of any time set before (C#'s `CancelAfter`); 0 cancels now, and a cancelled source stays as it is |
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
