---
name: shared
kind: keyword
summary: shared T: a value on the heap with several owners, only read.
see: weak, share, checked, locked, new
learn: ../docs/learn/book/07-ownership.md#several-owners-shared-t-and-weak-t
---

## Forms

| Form | What it is |
|---|---|
| `new shared x` | puts `x` on the heap and gives its first owner |
| `share x` | one more owner of the same value |
| `shared T`, `shared T?` | the type of an owner, one that may be missing |
| `weak x` | a look that does not keep the value alive |
| `share w` | an owner from a `weak` look, or `null` once the value is gone |
| `same(a, b)` | whether two owners point to one value |
| `x.makeUnique()` | a `ref T` to change: the value itself, or a copy of it |
| `shared checked T` | changed through any owner, every access checked |
| `shared locked T` | changed by tasks, inside `lock` |

## What it means

Most values have one owner. Some have none that is clearly first: one configuration read by several servers,
the nodes of a graph. `new shared x` puts the value on the heap, and `share x` makes one more owner of it,
without a copy. The value is let go when its last owner ends.

What a `shared T` points to is only read. Any owner could change it under a borrow another owner holds, and the
compiler cannot see all the owners. A value changed through several owners is a `shared checked T`, whose every
access is checked while the program runs, or a `shared locked T`, which tasks change one at a time.

The count of owners is atomic, so an owner can go to another task. It costs on `share` and where an owner
ends; a borrow of a `shared` value does not touch it.

Owners that point at each other in a circle keep each other alive. The side that does not own is made `weak`;
the interpreter, the tests and a debug build report such a circle when the program ends.

## Examples

### Two owners of one value

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
    var config = new shared Config { host = "localhost", port = 8080 }
    var api = Server { name = "api", config = share config }
    var admin = Server { name = "admin", config = share config }
    print($"{api.name} {api.config.host}:{api.config.port}")
    print($"{admin.name} {admin.config.host}:{admin.config.port}")
    print(same(api.config, admin.config))
}
```

```text
api localhost:8080
admin localhost:8080
true
```

Both servers own the one configuration; it is let go after the last of them ends.

### A look that does not keep it alive

```sg
struct Config {
    string host
}

void show(read weak Config look) {
    var owner = share look
    if owner is null {
        print("gone")
    } else {
        print(owner.host)
    }
}

void main() {
    var config = new shared Config { host = "localhost" }
    var look = weak config
    show(read look)
    config = new shared Config { host = "example.org" }
    show(read look)
}
```

```text
localhost
gone
```

When `config` gets a new value, the old one has no owner left. `share look` then gives `null`.

### Changed by copying first

```sg
struct Config : Clonable {
    string host
    int port
}

void main() {
    var config = new shared Config { host = "localhost", port = 8080 }
    var alone = config.makeUnique()
    alone.port = 9090
    print($"{config.host}:{config.port}")

    var other = share config
    var mine = config.makeUnique()
    mine.host = "example.org"
    print($"{config.host}:{config.port} {other.host}:{other.port} {same(config, other)}")
}
```

```text
localhost:9090
example.org:9090 localhost:9090 false
```

While `config` is the only owner, `makeUnique` changes the value where it lies. Once `other` shares it,
`config` gets a copy of its own first, and `other` keeps what it had.

### Changed through any owner

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
    print($"{mine.balance} {mine.history}")
}
```

```text
70 [50, 20]
```

A `shared checked` value is lent to one that changes it or to any number that read it, never both at once.

### Owners in two tasks

```sg
struct Table {
    List<string> words
}

long countLong(shared Table table, nint from, nint to) {
    long n = 0
    for i in from..to {
        if table.words[i].length() > 4 {
            n += 1
        }
    }
    return n
}

void main() {
    var words = List<string>()
    for w in "shared values go to other tasks without a copy".splitWhitespace() {
        words.add(w.clone())
    }
    var table = new shared Table { words = words }
    var half = table.words.length() / 2
    var low = Task.run(countLong, share table, 0, half)
    var high = Task.run(countLong, share table, half, table.words.length())
    print(low.wait() + high.wait())
}
```

```text
5
```

Each task owns the table while it runs, and neither copies the words.

## What stops the program

### A `shared checked` value lent twice

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

`mine` and `alias` are two owners of one account, so the compiler cannot tell; the check finds it when it
happens, in every build.

## What the compiler refuses

### Writing through a `shared` value

```sg error
struct Config {
    string host
    int port
}

void main() {
    var config = new shared Config { host = "localhost", port = 8080 }
    var other = share config
    other.port = 9090
    print(config.port)
}
```

```text
error: this field is reached through a `shared` value, which is only read and cannot be written
  a `shared` value has several owners, and none of them changes it alone; a value that is changed through several owners is a `shared checked T`, whose every access is checked against the borrows of it in progress
  --> example.sg:9:5
 9 |     other.port = 9090
   |     ^^^^^^^^^^
```

### Reaching through a `weak` look

```sg error
struct City {
    int id
}

void main() {
    var home = new shared City { id = 1 }
    var look = weak home
    print(look.id)
}
```

```text
error: this is `weak City`: what it looks at may be gone, so `.id` cannot reach it
  --> example.sg:8:11
 8 |     print(look.id)
   |           ^^^^
  hint: ask for an owner first: `var owner = share look`, which is `null` if it is gone
```

### A `shared` value as a key

```sg error
struct City {
    int id
}

void main() {
    var seen = HashSet<shared City>()
    _ = seen.add(new shared City { id = 1 })
}
```

```text
error: `shared City` cannot be an item of a `HashSet`: it is not `Hashable`
  --> example.sg:6:24
 6 |     var seen = HashSet<shared City>()
   |                        ^^^^^^^^^^^
  hint: keep something that tells the values apart instead, such as an `id` field: `HashSet<int>`
```

What it points to may change through `checked`, and a key that changes breaks the table.
