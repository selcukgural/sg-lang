---
name: static
kind: keyword
summary: A value made once, the first time the program uses it.
see: const, atomic, shared, lock, public
learn: ../docs/learn/book/02-values.md#consts
---

## Forms

| Form | What it is |
|---|---|
| `static T Name = value` | one value for the whole program, only read |
| `static atomic T Name = atomic value` | one that tasks change with its [`atomic`](atomic.md) methods |
| `static locked T Name = value` | one that tasks change inside a `lock` block |
| `public static T Name = value` | a static other modules can reach |
| `static T Name = value` in a function | one for the whole program, whose name is seen only in that block |

## What it means

A `static` is one value for the program. It lives until the program ends, and it is never assigned. Its type is
always written. Every task can reach it, so it comes in three kinds: read only, `atomic`, and `locked`.

When the value is made as a [`const`](const.md) is, of numbers, texts, other consts and operators, it lies in
the program's data from the start, at no cost. Any other value, one a function finds, is found the first time
the static is used, once, whichever task comes first; a task that reads it meanwhile waits for it. A later read
is one load and one branch. Nothing is found before `main`, so a function that gives a static its value never
runs when the program does not read the static.

A static of a number, a `bool`, a `char` or a function is read as a value; any other is lent as `read`, so it
can be looked at but not changed. A `static atomic` value changes through its methods. A `static locked` value
is reached only in a `lock` block, where one task at a time holds it; it is the one place `locked` is written
without [`shared`](shared.md).

What finds a value cannot `lock`, `wait` or `await`, so finding cannot deadlock, and it cannot need the static
itself. What a static holds follows the rule of tasks: no borrow, no `Task`, no `shared checked` value outside a
lock. It is never dropped, so it cannot hold a file or another value that must be closed. There is no global
variable that any task assigns, and none for each thread.

## Examples

### Found the first time it is used

```sg
int[10] squares() {
    print("making the squares")
    var table = int[10]
    for n in 0..10 {
        table[n] = n * n
    }
    return table
}

static int[10] Squares = squares()

void main() {
    print("start")
    print(Squares[3])
    print(Squares[9])
}
```

```text
start
making the squares
9
81
```

`squares` runs when `Squares[3]` is first read, after `start`, and only once. A const could not be given this
value: a const calls no function.

### Known from the start

```sg
static int[4] Ports = int[4] { 80, 443, 8080, 8443 }
static string Host = "localhost"

void main() {
    for port in Ports {
        print($"{Host}:{port}")
    }
}
```

```text
localhost:80
localhost:443
localhost:8080
localhost:8443
```

These values are made as a const is, so they lie in the program from its start, and a read is not checked.

### Read by several tasks

```sg
List<string> load() {
    print("loading the words")
    return List<string> { "static", "atomic", "locked" }
}

static List<string> Words = load()

int count(int extra) {
    return (int)Words.length() + extra
}

void main() {
    print("start")
    var a = Task.run(count, 10)
    var b = Task.run(count, 20)
    print(a.wait() + b.wait())
    print(Words[0])
}
```

```text
start
loading the words
36
static
```

Both tasks read `Words`, and `load` runs once: the task that comes first finds the list, and the other waits for
it. A list cannot be a const, but it can be a static. Every use lends it as `read`, so a task may look at it and
no task may change it.

### A pattern compiled once

```sg
static Regex Port = Regex.of("""port (\d+)""")

void main() {
    var lines = List<string> { "port 8080", "no port", "port 22" }
    for line in lines {
        var found = Port.find(read line)
        if found is not null {
            print(found.of(read line))
        }
    }
}
```

```text
port 8080
port 22
```

The pattern is compiled the first time `Port` is read, not on each turn of the loop.

### A counter all tasks change

```sg
static atomic long Requests = atomic 0

void serve(int times) {
    for _ in 0..times {
        _ = Requests.fetchAdd(1)
    }
}

void main() {
    var tasks = List<Task<void>>()
    for _ in 0..4 {
        tasks.add(Task.run(serve, 250))
    }
    Task.whenAll(tasks).wait()
    print(Requests.load())
}
```

```text
1000
```

`static atomic` needs no owner and no `share`: every task reaches the same counter, and no addition is lost.

### A static in a function

```sg
int nextId() {
    static atomic int Last = atomic 0
    return Last.fetchAdd(1) + 1
}

void main() {
    print(nextId())
    print(nextId())
    print(nextId())
}
```

```text
1
2
3
```

There is one `Last` for the program, not one for each call. Its name is seen only inside `nextId`.

### Settings that change, under a lock

```sg
struct Settings {
    string host
    int port
}

Settings load() {
    print("reading the settings")
    return Settings { host = "localhost", port = 80 }
}

static locked Settings Current = load()

void usePort(int port) {
    lock Current as s {
        s.port = port
    }
}

void main() {
    print("start")
    var task = Task.run(usePort, 8080)
    task.wait()
    lock Current as s {
        print($"{s.host}:{s.port}")
    }
}
```

```text
start
reading the settings
localhost:8080
```

Inside `lock Current as s`, `s` is a `ref` to the settings, and no other task is in a `lock` block of `Current`
at the same time. Outside a `lock` block, `Current` cannot be reached at all.

### From another module

```sg file title="server/stats/counts.sg"
public static atomic long Requests = atomic 0

public void count() {
    _ = Requests.fetchAdd(1)
}
```

```sg file title="server/main.sg"
use stats

void main() {
    count()
    count()
    print(Requests.load())
}
```

```text
$ cd server
$ sg main.sg
2
```

`public` makes the static seen in every module that names `stats`; it is still one value for the program.

### Each test starts again

```sg test
static atomic int Hits = atomic 0

void hit() {
    _ = Hits.fetchAdd(1)
}

test "two hits" {
    hit()
    hit()
    assert(Hits.load() == 2)
}

test "each test starts with the statics as the program starts" {
    assert(Hits.load() == 0)
    hit()
    assert(Hits.load() == 1)
}

void main() {
    hit()
    print(Hits.load())
}
```

```text
ok    two hits
ok    each test starts with the statics as the program starts
2 test(s) passed
```

What one test changed in a static is gone in the next, and a value found in one test is found again. `sg test
--compiled` prints the same, each test running in a process of its own.

## What the compiler refuses

### Assigning a static

```sg error
static int Total = 3

void main() {
    Total += 1
}
```

```text
error: `Total` is a static, and a static is never assigned: it is read by every task
  a static that changes is `static atomic`, changed by its atomic methods, or `static locked`, changed inside a `lock` block
  --> example.sg:4:5
 4 |     Total += 1
   |     ^^^^^
```

### A static with no type

```sg error
static var Limit = 3

void main() {
    print(Limit)
}
```

```text
error: a static has its type written
  --> example.sg:1:8
 1 | static var Limit = 3
   |        ^^^
```

### A value that waits

```sg error
int work() {
    return 1
}

int find() {
    var task = Task.run(work)
    return task.wait()
}

static int Value = find()

void main() {
    print(Value)
}
```

```text
error: the value of the static `Value` is found where it is first used, and `find` waits
```

Other tasks may be waiting for `Value` while it is found, so what finds it never waits: no `wait`, no `lock`, no
`await`, in any function it calls.

### A static that needs itself

```sg error
static int First = first()
static int Second = second()

int first() {
    return Second + 1
}

int second() {
    return First + 1
}

void main() {
    print(First)
}
```

```text
error: the value of `First` needs itself: `First` -> `Second` -> `First`
```

The compiler follows the calls and the statics they read. A circle through a function value is found only when
the program runs:

```sg panic
static int Total = through(readTotal)

int readTotal() {
    return Total + 1
}

int through(Func<int> give) {
    return give()
}

void main() {
    print("start")
    print(Total)
}
```

```text
start
panic: the value of `Total` needs itself: it is read while it is being found
```

### A task in a static

```sg error
int work() {
    return 1
}

static Task<int> Work = Task.run(work)

void main() {
}
```

```text
error: a static cannot hold a `Task<int>`: a task is waited for by the task that started it, and by no other
  every task reaches a static, so what it holds owns all it holds: no borrow, no `shared checked` value, no `Task`
```

### A file in a static

```sg error
static StreamWriter Log = File.createText("log.txt") ?? panic("no log")

void main() {
}
```

```text
error: a static cannot hold a `StreamWriter`: a static is never dropped, so it would never be closed
  --> example.sg:1:8
 1 | static StreamWriter Log = File.createText("log.txt") ?? panic("no log")
   |        ^^^^^^^^^^^^
  hint: open it where it is used and lend it to the functions that need it, as a parameter
```

### A type that is not one

```sg error
static File Log = File.create("log.txt")

void main() {
}
```

```text
error: there is no type named `File`
  `File` is not a type but a set of functions: a file open to read is a `StreamReader`, one open to write a `StreamWriter`
  --> example.sg:1:8
 1 | static File Log = File.create("log.txt")
   |        ^^^^
```

### A static of a block that reads a variable

```sg error
void main() {
    var n = 3
    static int Twice = n * 2
    print(Twice)
}
```

```text
error: there is no variable named `n`
  a static sees consts and other statics, not variables: its value is found once, the first time it is read
  --> example.sg:3:24
 3 |     static int Twice = n * 2
   |                        ^
```

### A static of a block that nothing uses

```sg error
void main() {
    static atomic int Calls = atomic 0
    print("x")
}
```

```text
error: `Calls` is never used
  --> example.sg:2:23
 2 |     static atomic int Calls = atomic 0
   |                       ^^^^^
  hint: take the line away: a static of a block is seen only by the lines after it in the block
```

### A method that changes it

```sg error
static List<string> Names = List<string> { "a" }

void main() {
    Names.add("b")
    print(Names)
}
```

```text
error: `add` changes the list, but it is reached through the static `Names`
  a static that changes is `static atomic`, changed by its atomic methods, or `static locked`, changed inside a `lock` block
  --> example.sg:4:5
 4 |     Names.add("b")
   |     ^^^^^
```

### A local with the name of a static

```sg error
static int Limit = 3

void main() {
    var Limit = 4
    print(Limit)
}
```

```text
error: `Limit` is the name of a static, and a local cannot take it
```
