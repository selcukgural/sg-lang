---
title: Ownership and borrowing
description: How SG frees every value at the right moment without a garbage collector, and the rules the compiler checks to keep that safe.
sidebar_position: 7
---

# Ownership and borrowing

SG has no garbage collector, and you never call `free`. Instead every value has an **owner**, and the value is
freed when its owner ends. The compiler follows owners and borrows through the whole program and refuses the
ones that could lead to a value used after it is freed, freed twice, or changed while someone else looks at it.
These checks are made when the program is compiled and cost nothing when it runs: what is left in the compiled
code is the `free` you would have written in C, in the place you would have written it. Only `shared checked`
values, near the end of this page, are checked while the program runs.

## One owner, and moves

A variable owns its value. Giving a value (a `string`, a `List`, a struct) to another variable or to a function
by value **moves** it: the new place owns it, and the old name is gone.

```sg error
int total(List<int> numbers) {
    var sum = 0
    for n in numbers {
        sum += n
    }
    return sum
}

void main() {
    var scores = List<int> { 90, 75, 82 }
    print(total(scores))
    print(scores.length())
}
```

```text
error: `scores` was moved and can no longer be used
  --> example.sg:12:11
 12 |     print(scores.length())
    |           ^^^^^^
  it was moved here:
  --> example.sg:11:17
 11 |     print(total(scores))
    |                 ^^^^^^
```

`total` owns the list now, and frees it when it returns. If `main` could still use `scores`, it would read freed
memory; if both freed it, the memory would be freed twice. With one owner there is exactly one place where each
value is freed, and the compiler knows it. A move inside a loop is caught too: `items.add(line)` in a `for` is
an error, since the second round would use a `line` the first round gave away.

## Values that are copied

Numbers, `bool` and `char` are copied, not moved. A struct is copied too when it says `: Copyable`:

```sg
struct Point : Copyable, Formattable {
    int x
    int y
}

void main() {
    var a = Point { x = 1, y = 2 }
    var b = a
    b.x = 10
    print($"{a} {b}")
}
```

It prints:

```text
Point { x = 1, y = 2 } Point { x = 10, y = 2 }
```

Only a struct whose fields are all copied can be `Copyable`. A copy of a `string` would need memory of its own,
and two values pointing to the same text would both free it, so the compiler refuses:

```sg error
struct Label : Copyable {
    string text
}

void main() {
    var label = Label { text = "x".clone() }
    print(label.text)
}
```

```text
error: `Label` cannot be `Copyable`: its field `text` is a `string`, and a `string` is not `Copyable`
  a `string` owns memory on the heap; a copy would have to allocate, and that is written `.clone()`
  --> example.sg:2:5
 2 |     string text
   |     ^^^^^^
```

## Borrowing: `read` and `ref`

Most functions only need to look at a value, or change it in place, and the caller keeps it. They take a
**borrow**: `read T` to look, `ref T` to change. The caller writes `read` or `ref` at the call too, so you see
at a glance what a call may do to your variables.

```sg
int longest(read List<string> words) {
    var best = 0
    for word in words {
        if word.length() > best {
            best = (int)word.length()
        }
    }
    return best
}

void shout(ref List<string> words) {
    for i in 0..words.length() {
        words[i] = words[i].toUpper()
    }
    words.add("!".clone())
}

void main() {
    var words = List<string> { "owner".clone(), "borrow".clone() }
    print(longest(read words))
    shout(ref words)
    print(words)
}
```

It prints:

```text
6
["OWNER", "BORROW", "!"]
```

Through a `read` borrow nothing can be changed: `scores.clear()` on a `read List<int>` is an error whose hint is
to take it as `ref`. A borrow given to a call lasts for the call. A borrow kept in a variable lasts until the
last use of that variable, not to the end of the block, so this runs:

```sg
void main() {
    var words = List<string> { "owner".clone(), "borrow".clone() }
    var first = read words[0]
    print(first)
    words.add("later".clone())
    print(words.length())
}
```

It prints:

```text
owner
3
```

## What the borrow check refuses

While a borrow is still going to be used, what it borrows from cannot be changed or moved. Here `clear` would
free the text that `first` points to:

```sg error
void main() {
    var words = List<string> { "owner".clone(), "borrow".clone() }
    var first = read words[0]
    words.clear()
    print(first)
}
```

```text
error: `words` cannot be changed here: `first` borrows from it and is used again later
  --> example.sg:4:5
 4 |     words.clear()
   |     ^^^^^
  `first` borrowed it here:
  --> example.sg:3:17
 3 |     var first = read words[0]
   |                 ^^^^^^^^^^^^^
  hint: finish with `first` before this line: a borrow ends at its last use
```

This is the rule that keeps a pointer from dangling: even `words.add` is refused here, because a list that grows
may move its items to a bigger block of memory.

A borrow cannot outlive what it borrows from. `guest` is freed at the end of its block, and `shown` would point
at freed memory after it:

```sg error
void main() {
    var name = "nobody".clone()
    var shown = read name
    if name.length() > 3 {
        var guest = "Pasha".clone()
        shown = read guest
    }
    print(shown)
}
```

```text
error: `shown` borrows from `guest` here, but `guest` is gone at the end of its block and `shown` is used after that
  --> example.sg:6:17
 6 |         shown = read guest
   |                 ^^^^^^^^^^
  hint: declare `guest` where `shown` is declared, so that it lives as long
```

The same holds for a function: `return read local` is an error, since the local is freed as the function
returns. A function may return a borrow of what it was lent: `read string longest(read List<string> words)` can
`return read words[at]`, and the caller's borrow then lasts as one of `words` would. With two borrowed
parameters the function says which one the result comes from, `from a` after the parameters.

A `ref` borrow must be the only borrow of its value. Two names that change the same place at once are how a
function ends up reading what it has just overwritten:

```sg error
void addTo(ref int total, ref int amount) {
    total += amount
    amount = 0
}

void main() {
    var balance = 10
    addTo(ref balance, ref balance)
    print(balance)
}
```

```text
error: `balance` is lent twice in one call, and a `ref` borrow must be the only one
  --> example.sg:8:28
 8 |     addTo(ref balance, ref balance)
   |                            ^^^^^^^
  the other borrow is here:
  --> example.sg:8:15
 8 |     addTo(ref balance, ref balance)
   |               ^^^^^^^
  hint: copy what is needed into a variable before the call
```

The same goes for variables: while `var a = ref count` is still used, `count` cannot be read, changed or lent
again, even by `print`.

## `.clone()`

When you need a second value of your own, ask for it: `.clone()` makes a new copy that owns its own memory.
It is never done behind your back, so every allocation is visible in the code. A struct gets a `clone` by
writing one:

```sg
struct Label : Formattable {
    string text
    int size

    Label clone(read this) {
        return Label { text = this.text.clone(), size = this.size }
    }
}

void main() {
    var name = "Pasha".clone()
    var copy = name.clone()
    copy.write(" the cat")
    print($"{name} / {copy}")

    var title = Label { text = "Ownership".clone(), size = 24 }
    var small = title.clone()
    small.size = 12
    print($"{title} {small}")
}
```

It prints:

```text
Pasha / Pasha the cat
Label { text = "Ownership", size = 24 } Label { text = "Ownership", size = 12 }
```

A struct or an enum that says `: Clonable` gets that `clone` without writing it: a copy field by field, each
field cloned as its type clones (a `string` copied, a `List` item by item, a `shared` field one more owner).
A type that writes its own `clone` keeps it. Lists, maps and sets clone item by item.

```sg
struct Label : Clonable, Formattable {
    string text
    int size
}

void main() {
    var title = Label { text = "Ownership".clone(), size = 24 }
    var small = title.clone()
    small.size = 12
    small.text.write(", short")
    print($"{title} {small}")

    var words = List<string> { "one".clone(), "two".clone() }
    var more = words.clone()
    more.add("three".clone())
    print($"{words} {more}")
}
```

It prints:

```text
Label { text = "Ownership", size = 24 } Label { text = "Ownership, short", size = 12 }
["one", "two"] ["one", "two", "three"]
```

A field that cannot be copied, such as a `raw T` or a `Disposable` value, makes `: Clonable` an error that
names it.

## Pointers: `T*` and `new`

`new x` puts a value on the heap and gives a `T*`, a pointer that owns it. A `T*?` may be `null`. A pointer
moves as any owner does, and the value behind it is freed when the pointer's owner ends, or when the pointer is
written over:

```sg
struct Node {
    int value
    Node*? left
    Node*? right
}

Node* leaf(int value) {
    return new Node { value = value, left = null, right = null }
}

int sum(read Node*? node) {
    if node is null {
        return 0
    }
    return node.value + sum(read node.left) + sum(read node.right)
}

void main() {
    var root = new Node { value = 1, left = leaf(2), right = leaf(3) }
    print(sum(read root))
    root.left = null
    print(sum(read root))
}
```

It prints:

```text
6
4
```

`root.left = null` frees the leaf that was there. Fields are reached through a pointer with `.`, and a borrow
of a `T*` goes where a borrow of a `T` is expected.

## Several owners: `shared T` and `weak T`

Some values have no single owner: one configuration read by several servers, the nodes of a graph.
`new shared x` makes a `shared T`, one of the owners of a value on the heap; `share x` makes one more owner without moving
`x`. The value is freed when its last owner ends. `weak x` gives a `weak T`, which looks at the value without
keeping it alive: `share w` gives a `shared T?`, `null` once the last owner is gone.

```sg
struct Config {
    string host
    int port
}

struct Server {
    string name
    shared Config config
}

void main() {
    var config = new shared Config { host = "localhost".clone(), port = 8080 }
    var api = Server { name = "api".clone(), config = share config }
    var admin = Server { name = "admin".clone(), config = share config }
    print($"{api.name} {api.config.host}:{api.config.port}")
    print(same(api.config, admin.config))

    var look = weak config
    print(share look is null)
    config = new shared Config { host = "example.org".clone(), port = 80 }
    print(share look is null)
    api = Server { name = "api".clone(), config = share config }
    admin = Server { name = "admin".clone(), config = share config }
    print(share look is null)
}
```

It prints:

```text
api localhost:8080
true
false
false
true
```

The first configuration lives on after `config` is given a new one, because the two servers still own it; it is
freed when they are replaced. `same(a, b)` tells whether two owners point to one value; `==` compares what they
point to. The count of owners is atomic, so a `shared` value can go to another task.

What a `shared T` points to is only read. If any owner could change it, it could change under a borrow another
owner holds, and the compiler cannot see all the owners:

```sg error
struct Config {
    string host
    int port
}

void main() {
    var config = new shared Config { host = "localhost".clone(), port = 8080 }
    var other = share config
    other.port = 9090
    print(config.port)
}
```

```text
error: this field is reached through a `shared` value, which is only read and cannot be written
  a `shared` value has several owners, and none of them changes it alone; a value that is changed through several owners is a `shared checked T`, whose every access is checked against the borrows of it in progress
```

To change one all the same, `x.makeUnique()` gives a `ref T`. When `x` is the only owner and no `weak` looks at
the value, it is the value itself, changed where it lies; otherwise `x` first gets a copy of its own (its type
must be `Clonable`), and the other owners keep what they had. Copy on write, as Rust's `Rc::make_mut`: a value
built up step by step through one owner is never copied.

```sg
struct Config : Clonable {
    string host
    int port
}

void main() {
    var config = new shared Config { host = "localhost".clone(), port = 8080 }
    // `config` is the only owner: it is changed where it lies.
    var alone = config.makeUnique()
    alone.port = 9090
    print($"{config.host}:{config.port}")

    // Another owner sees the old value: `config` gets a copy of its own first.
    var other = share config
    var mine = config.makeUnique()
    mine.host = "example.org".clone()
    print($"{config.host}:{config.port} {other.host}:{other.port} {same(config, other)}")
}
```

It prints:

```text
localhost:9090
example.org:9090 localhost:9090 false
```

## Changed through any owner: `shared checked T`

A `shared checked T` is changed through any of its owners. Each access is checked while the program runs: the
value may be lent to one user that changes it or to any number that read it, never both at once.

```sg
struct Account {
    long balance
    List<long> history

    void deposit(ref this, long amount) {
        this.balance += amount
        this.history.add(amount)
    }
}

void main() {
    var mine = new shared checked Account { balance = 0, history = List<long>() }
    var bank = share mine
    mine.deposit(50)
    bank.deposit(20)
    bank.balance -= 5
    print($"{mine.balance} {mine.history}")
    read mine as account {
        print(account.history.length())
    }
}
```

It prints:

```text
65 [50, 20]
2
```

A plain read or write borrows nothing after it; a call borrows for the call, and `read x as n { ... }` or
`ref x as n { ... }` for a block. A conflict the compiler can see through one variable, such as
`add(ref mine, read mine)`, is a compile error. One through another owner is found when it happens, and the
program stops with both lines, in the interpreter and in every compiled build:

```sg panic
struct Account {
    long balance

    void addFrom(ref this, read shared checked Account other) {
        this.balance += other.balance
    }
}

void main() {
    var mine = new shared checked Account { balance = 10 }
    var alias = share mine
    mine.addFrom(read alias)
}
```

```text
panic: this `Account` is being changed, so it cannot be read here
  a `shared checked` value is lent to one that changes it or to any number that read it, not both at once
  --> example.sg:5:25
 5 |         this.balance += other.balance
   |                         ^^^^^
  it is being changed from here:
  --> example.sg:12:5
 12 |     mine.addFrom(read alias)
    |     ^^^^
```

A `shared checked` value stays in the task that made it. Owners that point at each other in a circle keep each
other alive for ever; make the side that does not own `weak`, as a child's link to its parent, `weak checked Folder? parent`. The interpreter,
the tests and a debug build report such a cycle when the program ends, with the field that holds it (see
[Testing](./13-testing.md)).

## `Disposable`: code that runs at the end

A struct that holds something outside memory (a connection, a handle of C) says what to do when it ends with
`: Disposable` and a `void dispose(ref this)`. It runs once, where the last owner ends, before the fields are
freed:

```sg
struct Connection : Disposable {
    string name

    void dispose(ref this) {
        print($"close {this.name}")
    }
}

void send(Connection c) {
    print($"send on {c.name}")
}

void main() {
    var a = Connection { name = "a".clone() }
    if true {
        var b = Connection { name = "b".clone() }
        print("in the block")
    }
    send(Connection { name = "c".clone() })
    var first = new shared Connection { name = "d".clone() }
    var second = share first
    print("end of main")
}
```

It prints:

```text
in the block
close b
send on c
close c
end of main
close d
close a
```

Values that end together end in the reverse order of their names. A `Disposable` value is kept for its end, so
`a` needs no use. The program never calls `dispose` itself, so it cannot run twice:

```sg error
struct Connection : Disposable {
    string name

    void dispose(ref this) {
        print($"close {this.name}")
    }
}

void main() {
    var a = Connection { name = "a".clone() }
    a.dispose()
}
```

```text
error: `dispose` is not called by the program: it runs by itself, once, when the last owner of the value ends
  --> example.sg:11:7
 11 |     a.dispose()
    |       ^^^^^^^
  hint: to end it sooner, hand it to a function or a method that takes it by value; it ends where that one does
```

A `Disposable` value is not `Copyable` (each copy would be disposed of), and no field is moved out of one. A
panic and `exit` end the program without running `dispose`.
