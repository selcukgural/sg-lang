---
name: async
kind: keyword
summary: Makes a function a light task, which may await and never holds its thread.
see: await, Task, TaskGroup, CancellationSource, shared
learn: ../docs/learn/book/10-concurrency.md#async-functions
library: ../docs/library/tasks.md#taskr
---

## Forms

| Form | What it is |
|---|---|
| `async T name(...) { ... }` | an async function: a call gives a `Task<T>` |
| `async void name(...) { ... }` | one that gives nothing: a call gives a `Task<void>` |
| `async void main()` | a program whose `main` may await |
| `async T name(read this, ...)` | an async method that lends `this`: it is awaited where it is called |
| `async T name(this, ...)` | an async method that takes `this`: a call may start it as a task |
| `var t = f(x)` | starts `f` as a task, which runs until it first waits |
| `await f(x)` | runs `f` as part of this task, and gives what it returns |
| `f(x).wait()` | waits for it from code that is not async |

## What it means

`async` in front of a function makes each call of it a light task. A call gives a `Task<T>`, where `T` is the
type written after `async`. The task starts at once and runs on the caller until it first waits; then the
caller goes on. A few threads run many such tasks, since a task that waits gives its thread to another.

An async function may `await`: wait for a task, a delay (`Task.delay(ms)`), a channel, a group of tasks, or the
async calls of files, the network and other programs. In exchange it never holds its thread. `wait`, the calls
of `File` and `Directory`, and reading the input are errors there, and so is a call of a function that does
one of them. Work that must hold a thread runs apart with `await Task.run(work, x)`. `print` is allowed.

A task started without `await` follows the rule of every task: what it is given owns all it holds, so no
borrow goes to it, and a `shared` value goes as one more owner. `await f(read xs)` ends before the caller goes
on, so it may borrow. Every task is waited for, on every path, with `await` or with `wait`.

An async function is not a value: it is called by its name, not kept in a variable or given to `Task.run`.

## Examples

### An async function

```sg
async int twice(int n) {
    await Task.delay(10)
    return n * 2
}

async void main() {
    var answer = await twice(21)
    print(answer)
}
```

```text
42
```

`twice` waits 10 milliseconds, then returns. `main` is async too, so it can `await` the call.

### A call starts the task at once

```sg
async int fetch(string name, int ms) {
    print($"{name}: asked")
    await Task.delay(ms)
    return (int)name.length()
}

async void main() {
    var first = fetch("first", 200)
    var second = fetch("second", 100)
    print("both asked")
    print(await first + await second)
}
```

```text
first: asked
second: asked
both asked
11
```

Each call runs until its first `await`, then `main` goes on. The two delays run at the same time, so the
program takes about 200 milliseconds, not 300.

### Waited for by code that is not async

```sg
async int twice(int n) {
    await Task.delay(10)
    return n * 2
}

void main() {
    var task = twice(21)
    print(task.wait())
    print(twice(4).wait())
}
```

```text
42
8
```

A function that is not async cannot `await`, but it can call an async function and `wait` for the task.

### Async methods

```sg
struct Order {
    string item
    int count

    async int weigh(read this) {
        await Task.delay(5)
        return this.count * 250
    }

    async string pack(this) {
        await Task.delay(10)
        return $"{this.count} x {this.item}".toString()
    }
}

async void main() {
    var tea = Order { item = "tea", count = 2 }
    print(await tea.weigh())
    var packing = tea.pack()
    var bread = Order { item = "bread", count = 1 }
    print(await bread.pack())
    print(await packing)
}
```

```text
500
1 x bread
2 x tea
```

`weigh` lends `this`, so it is awaited where it is called. `pack` takes the order, so `tea.pack()` starts a
task that owns it, and `tea` is not used after.

### Work that holds a thread

```sg
long sumSquares(long limit) {
    long total = 0
    for i in 0..limit {
        total += i * i
    }
    return total
}

async void main() {
    var heavy = Task.run(sumSquares, 1000000)
    await Task.delay(10)
    print("the delay is over")
    print(await heavy)
}
```

```text
the delay is over
333332833333500000
```

`Task.run` gives the work a thread of its own, so the async tasks are not held while it counts. Its task is
awaited like any other.

### Owners of one value in several tasks

```sg
struct Config {
    string greeting
    int pause
}

async string greet(shared Config config, string name) {
    await Task.delay(config.pause)
    return $"{config.greeting}, {name}".toString()
}

async void main() {
    var config = new shared Config { greeting = "Hello", pause = 20 }
    var first = greet(share config, "Ada")
    var second = greet(share config, "Pasha")
    print(await first)
    print(await second)
}
```

```text
Hello, Ada
Hello, Pasha
```

A task started without `await` cannot borrow `config`, since it could outlive it. `share config` gives each
task one more owner of the same value, with no copy.

### A group of tasks

```sg
async void download(int size, shared atomic int total) {
    await Task.delay(size)
    _ = total.fetchAdd(size)
}

async void main() {
    var total = new shared atomic 0
    var sizes = List<int> { 30, 10, 20, 40 }
    var group = TaskGroup(2)
    for size in sizes {
        await group.whenRoom()
        group.add(download(size, share total))
    }
    await group.whenAll()
    print($"{total.load()} bytes")
}
```

```text
100 bytes
```

A `TaskGroup` holds tasks that give nothing and lets go of each as it ends. `TaskGroup(2)` lets two run at a
time: `whenRoom` waits until one of them has ended. `whenAll` waits for the rest.

### A time limit

```sg
async Result<int, Cancelled> slowAnswer(int ms, read CancellationToken token) {
    try await Task.delay(ms, read token)
    return Ok(42)
}

async void main() {
    var limit = CancellationSource()
    limit.cancelAfter(100)
    var token = limit.token()
    print(await slowAnswer(20, read token) ?? -1)
    print(await slowAnswer(500, read token) ?? -1)
    print(token.isCancelled())
}
```

```text
42
-1
true
```

The source cancels itself 100 milliseconds after `cancelAfter`. The first answer comes in time. The second
delay is cut short with `Error(Cancelled { })`, which `try` passes on.

## What the compiler refuses

### `wait` in an async function

```sg error
async int twice(int n) {
    await Task.delay(10)
    return n * 2
}

async void main() {
    var task = twice(21)
    print(task.wait())
}
```

```text
error: an async function does not `wait`: that would hold the thread, which other tasks need, until the task ends
  --> example.sg:8:16
 8 |     print(task.wait())
   |                ^^^^
  hint: `await task` waits without holding the thread
```

### A call that holds the thread

```sg error
nint size(read string path) {
    var text = File.readText(read path) ?? ""
    return text.length()
}

nint report() {
    return size("notes.txt")
}

async void main() {
    print(report())
}
```

```text
error: `report` holds the thread until it is done, and an async function does not hold its thread
  the tasks of async functions share a few threads: one that holds its thread keeps the others from running
  --> example.sg:11:11
 11 |     print(report())
    |           ^^^^^^
  `report` calls `size`, which does `File.readText` here:
  --> example.sg:2:21
 2 |     var text = File.readText(read path) ?? ""
   |                     ^^^^^^^^
  hint: a function that does it runs on a thread of its own with `await Task.run(work, x)`
```

The compiler follows the calls, and shows the chain that reaches the read.

### A borrow given to a task

```sg error
async int total(read List<int> numbers) {
    await Task.delay(5)
    var sum = 0
    for n in numbers {
        sum += n
    }
    return sum
}

async void main() {
    var numbers = List<int> { 1, 2, 3 }
    var task = total(read numbers)
    print(await task)
}
```

```text
error: `total` takes a `read List<int>` here, and a call without `await` starts it as a task of its own, which this cannot go to: a `read List<int>` is a borrow, and the task could outlive what it borrows from
  --> example.sg:12:22
 12 |     var task = total(read numbers)
    |                      ^^^^^^^^^^^^
  hint: `await total(read numbers)` runs it as part of this task, where it can borrow
```

### An async function kept as a value

```sg error
async int answer() {
    return 42
}

void main() {
    var task = Task.run(answer)
    print(task.wait())
}
```

```text
error: `answer` is async, and an async function is called by its name, not kept as a value
  --> example.sg:6:25
 6 |     var task = Task.run(answer)
   |                         ^^^^^^
  hint: call it: `answer(...)` starts it as a task, `await answer(...)` runs it here
```

### An async function that awaits itself

```sg error
async long countdown(int n) {
    if n == 0 {
        return 0
    }
    await Task.delay(1)
    return n + await countdown(n - 1)
}

async void main() {
    print(await countdown(3))
}
```

```text
error: the async function `countdown` awaits itself
  an awaited call runs inside the task of its caller, so a task that holds itself would have no end
  --> example.sg:6:22
 6 |     return n + await countdown(n - 1)
   |                      ^^^^^^^^^
  hint: keep the call's task in a variable, which gives it a task of its own: `var task = f(x)`, then `await task`
```

With `var rest = countdown(n - 1)` and then `return n + await rest`, each call is a task of its own, and the
program prints `6`.

### `async` in front of a type

```sg error
async struct Point {
    int x
}

void main() {
}
```

```text
error: `async` is written in front of a function
  --> example.sg:1:1
 1 | async struct Point {
   | ^^^^^
  hint: `async int count(string path) { ... }`
```
