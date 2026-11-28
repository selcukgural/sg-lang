---
name: as
kind: keyword
summary: Names a value lent for a block: a shared value opened, a locked value taken, an item a select receives.
see: read, ref, lock, locked, checked, shared
learn: ../docs/learn/book/07-ownership.md#changed-through-any-owner-shared-checked-t
---

## Forms

| Form | What it is |
|---|---|
| `read x as name { ... }` | opens a `shared` or `shared checked` value to read, for the block |
| `ref x as name { ... }` | opens a `shared checked` value to change, for the block |
| `lock x as name { ... }` | takes a `shared locked` value; `name` changes it for the block |
| `lock a as x, b as y { ... }` | takes two locked values at once, in a fixed order |
| `lock x as name when condition { ... }` | waits, with the lock let go, until the condition holds |
| `await lock x as name when condition { ... }` | the same in an `async` function |
| `ch.receive() as name => ...` | in a `select`, the arm that receives names its item; `as _` drops it |

## What it means

`as` gives a name to a value for the length of a block. The name borrows: inside the block the value is not
moved or assigned, and at the closing `}` the borrow ends. The block shows in the code how long the borrow
lasts.

`read x as name` and `ref x as name` open a value with several owners. Each access to a `shared checked` value
is checked while the program runs, and a borrow of what it holds ends with its line, so a variable cannot keep
one. A loop over one of its lists, or a result that borrows, such as `table.getOrAdd(word, 0)`, is written
inside the block, where the borrow lasts to the `}`. `read ... as` also opens a plain `shared` value; `ref ...
as` does not, since a plain `shared` value is only read. Any other value, a `T?` and a `weak` value are errors.

`lock x as name` takes a `shared locked` value, or a `static locked` one, and `name` is a `ref` to what it holds
for the block. Only one task is inside a `lock` block of the value at a time. With `when`, the task waits until
the condition, which sees the names as `read`, holds. A `lock` block itself never waits: no other `lock`, no
`wait` and no channel inside it, so locks cannot deadlock.

In a `select`, `receive() as name` names the item an arm receives; the item is owned by the arm.

There is no cast with `as`: a cast is written `(byte)total`. And `use` gives a module no second name.

## Examples

### Reading a shared value for a block

```sg
struct Account {
    string owner
    List<long> history
}

void main() {
    var mine = new shared checked Account { owner = "Ada".clone(), history = List<long> { 50, -20, 75 } }
    read mine as account {
        long total = 0
        for amount in read account.history {
            total += amount
        }
        print($"{account.owner}: {total} over {account.history.length()} entries")
    }
}
```

```text
Ada: 105 over 3 entries
```

`account` reads the value for the whole block, so the `for` can walk its list. While the block runs, no owner
of `mine` can change it.

### A borrow kept past its line

```sg
void main() {
    var counts = new shared checked HashMap<string, int>()
    var words = "the cat saw the dog and the bird".split(" ")
    for word in words {
        ref counts as table {
            var slot = table.getOrAdd(word, 0)
            slot += 1
        }
    }
    read counts as table {
        print(table["the"])
        print(table.length())
    }
}
```

```text
3
6
```

`getOrAdd` gives a borrow of a count inside the map. Outside a block it would end with its line; inside
`ref counts as table` it lasts as long as `slot` is used.

### Several tasks change one value

```sg
void record(shared locked int visits, int count) {
    for _ in 0..count {
        lock visits as v {
            v += 1
        }
    }
}

void main() {
    var visits = new shared locked 0
    var home = Task.run(record, share visits, 300)
    var shop = Task.run(record, share visits, 200)
    home.wait()
    shop.wait()
    lock visits as v {
        print($"{v} visits")
    }
}
```

```text
500 visits
```

`v` is the number itself, lent to change. Each addition is made by one task at a time, so none is lost.

### Two values, and a condition

```sg
struct Stock {
    int count
}

struct Orders {
    int filled
}

void sell(shared locked Stock stock, shared locked Orders orders, int amount) {
    lock stock as s, orders as o when s.count >= amount {
        s.count -= amount
        o.filled += 1
    }
}

void main() {
    var stock = new shared locked Stock { count = 0 }
    var orders = new shared locked Orders { filled = 0 }
    var first = Task.run(sell, share stock, share orders, 3)
    var second = Task.run(sell, share stock, share orders, 4)
    for _ in 0..10 {
        lock stock as s {
            s.count += 1
        }
    }
    first.wait()
    second.wait()
    lock stock as s, orders as o {
        print($"{o.filled} orders filled, {s.count} left")
    }
}
```

```text
2 orders filled, 3 left
```

Each seller waits, with both locks let go, until there is enough stock. The condition is looked at again each
time a `lock` block on one of the values ends, so the loop in `main` wakes them without a word more.

### Naming what a `select` receives

```sg
void produce(Sender<string> out, string name, int count) {
    for i in 0..count {
        _ = out.send($"{name} {i}".toString())
    }
}

void main() {
    var orders = Channel<string>(2)
    var alerts = Channel<string>(2)
    var a = Task.run(produce, orders.sender(), "order", 3)
    var b = Task.run(produce, alerts.sender(), "alert", 2)
    var seen = 0
    var alertCount = 0
    while true {
        select {
            orders.receive() as _ => seen += 1
            alerts.receive() as alert => {
                alertCount += 1
                print(alert)
            }
            ended => break
        }
    }
    a.wait()
    b.wait()
    print($"{seen} orders, {alertCount} alerts")
}
```

```text
alert 0
alert 1
3 orders, 2 alerts
```

`alert` is the text the arm received. `as _` takes an order and drops it, since only the count is wanted.

## What stops the program

### A change while the block reads

```sg panic
struct Account {
    long balance
}

void main() {
    var mine = new shared checked Account { balance = 100 }
    var bank = share mine
    read mine as account {
        bank.balance -= 30
        print(account.balance)
    }
}
```

```text
panic: this `Account` is being read, so it cannot be changed here
  a `shared checked` value is lent to one that changes it or to any number that read it, not both at once
  --> example.sg:9:9
 9 |         bank.balance -= 30
   |         ^^^^
  it is being read from here:
  --> example.sg:8:10
 8 |     read mine as account {
   |          ^^^^
```

`bank` is another owner of the same account, which the checker does not follow; the check while the program
runs finds it, in the interpreter and in every compiled build.

### One value locked twice

```sg panic
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
    transfer(share alice, share alice, 10)
}
```

```text
panic: `a` and `b` are one value, which a `lock` takes once
  --> example.sg:6:21
 6 |     lock from as a, to as b {
   |                     ^^
```

Two names would change one value at once. A transfer to oneself is tested for before the `lock`.

## What the compiler refuses

### A value that is not shared

```sg error
void main() {
    var scores = List<int> { 3, 5 }
    read scores as view {
        print(view.length())
    }
}
```

```text
error: `read ... as` opens a `shared` value for a block, and this is `List<int>`
  --> example.sg:3:10
 3 |     read scores as view {
   |          ^^^^^^
  hint: use `scores` as it is
```

A value with one owner is borrowed with `read scores` where it is needed, or `var view = read scores`.

### A borrow of a checked value kept in a variable

```sg error
void main() {
    var counts = new shared checked HashMap<string, int>()
    var slot = counts.getOrAdd("tea", 0)
    slot += 1
}
```

```text
error: this borrows what a `shared checked` value holds, and such a borrow ends with its line; `slot` would keep it
  --> example.sg:3:16
 3 |     var slot = counts.getOrAdd("tea", 0)
   |                ^^^^^^^^^^^^^^^^^^^^^^^^^
  it is borrowed here:
  --> example.sg:3:16
 3 |     var slot = counts.getOrAdd("tea", 0)
   |                ^^^^^^
  hint: lend it for a block instead: `ref counts as value { var slot = value.getOrAdd("tea", 0) ... }`
```

The block is `ref`, since `getOrAdd` and `slot += 1` change the map; inside it, `slot` keeps the borrow to the
`}`.

### A `lock` without a name

```sg error
void main() {
    var hits = new shared locked 0
    lock hits {
        hits += 1
    }
}
```

```text
error: expected `as` and a name for what is locked, found `{`
  --> example.sg:3:15
 3 |     lock hits {
   |               ^
  hint: `lock total as c { c += 1 }`
```

### A received item with no name

```sg error
void produce(Sender<int> out) {
    _ = out.send(1)
}

void main() {
    var numbers = Channel<int>(2)
    var task = Task.run(produce, numbers.sender())
    select {
        numbers.receive() => print("got one")
        ended => print("none")
    }
    task.wait()
}
```

```text
error: the item an arm of a `select` receives is given a name
  --> example.sg:9:9
 9 |         numbers.receive() => print("got one")
   |         ^^^^^^^^^^^^^^^^^
  hint: `numbers.receive() as item => ...`, or `numbers.receive() as _ => ...` to drop it
```
