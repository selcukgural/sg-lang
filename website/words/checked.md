---
name: checked
kind: keyword
summary: shared checked T: a shared value changed through any owner, each borrow of it checked while the program runs.
see: shared, weak, share, as, locked, ref
learn: ../docs/learn/book/07-ownership.md#changed-through-any-owner-shared-checked-t
---

## Forms

| Form | What it is |
|---|---|
| `new shared checked x` | puts `x` on the heap and gives its first owner |
| `shared checked T`, `shared checked T?` | the type of an owner, one that may be missing |
| `share x` | one more owner of the same value |
| `weak checked T` | a look that does not keep the value alive |
| `read x as name { ... }` | lends the value to read, for the block |
| `ref x as name { ... }` | lends the value to change, for the block |
| `alias FolderRef = shared checked Folder` | a name for the type; `new FolderRef { ... }` makes one |

## What it means

A `shared T` is only read, since an owner that changed it could change it under a borrow another owner holds.
A `shared checked T` is changed through any of its owners, as a `ref` changes a value: its fields, a method
that takes `ref this`, a method that changes a collection, `for x in ref`, and `ref x` where a `ref T` goes.
`checked` is written after `shared` or `weak` only; a value with one owner is changed through `ref`.

Each borrow of it is checked while the program runs: the value is lent to one that changes it or to any
number that read it, never both at once. A read or a write is checked once and borrows nothing after it, as
in `a.count += 1`. A value lent to a call is borrowed until the call returns; the head of a `for`, for the
loop; the value a `match` or an `is` with names looks into, for the whole `match` or `if`; and `read x as n`
or `ref x as n`, for the block. A borrow also covers each `shared checked` value on the way to it:
`read a.next.items` borrows `a` too, so nothing on the way is freed while it is lent.

A borrow of what it holds ends with its line, so a variable does not keep one, except inside an `as` block,
where it lasts to the `}`. The receiver of a method that changes it is lent to read while the arguments are
computed, and to change after them: `a.items.add(a.items.length())` works.

A conflict the compiler can see, through the same variable and the same fields, is a compile error: the line
would stop the program every time it ran. Items of a list are left to run time, as for any list:
`transfer(ref accounts[i], ref accounts[j], 30)` on a `List<shared checked Account>` runs, and stops when `i`
and `j` are the same. `ref x` also works through a `read` borrow of an owner, or of a list of owners, since what
is borrowed is the owner and not what it points to. One through another owner is found when it happens, and the program
stops with the line of the access and the line where the borrow began. The checks are made in the interpreter
and in every compiled build, release too, as an index is checked: they keep memory safe. They cost 16 bytes in
front of the counts of owners, for the borrows in progress, and a write on each borrow.

The checks belong to one task. A `shared checked` value, or a `weak checked` look, does not go to another
task, nor inside a field, a list or a `shared` value that goes. A value that several tasks change is
`shared locked`; a value is `checked` or `locked`, not both.

## Examples

### Changed through any owner

```sg
struct Visits {
    int count
    List<string> pages
}

struct Site {
    string name
    shared checked Visits visits
}

void main() {
    var visits = new shared checked Visits { count = 0, pages = List<string>() }
    var shop = Site { name = "shop", visits = share visits }
    var blog = Site { name = "blog", visits = share visits }
    shop.visits.count += 1
    shop.visits.pages.add("shop/cart")
    blog.visits.count += 1
    blog.visits.pages.add("blog/news")
    print($"{visits.count} {visits.pages}")
}
```

```text
2 ["shop/cart", "blog/news"]
```

Both sites own the one `Visits`, and each changes it. Each line borrows the value only while it runs, so no
two of them meet.

### Opened for a block

```sg
struct Stock {
    List<string> names
    List<int> counts
}

void main() {
    var stock = new shared checked Stock { names = List<string> { "tea", "milk", "bread" }, counts = List<int> { 3, 0, 5 } }
    var shop = share stock
    ref shop as s {
        for c in ref s.counts {
            if c == 0 {
                c = 10
            }
        }
    }
    read stock as s {
        var names = read s.names
        var counts = read s.counts
        for i in 0..names.length() {
            print($"{names[i]} {counts[i]}")
        }
    }
}
```

```text
tea 3
milk 10
bread 5
```

`ref shop as s` lends the value to change until its `}`, and `read stock as s` to read. Inside the second
block, `names` and `counts` keep a borrow over several lines, which outside a block they could not.

### Read twice at once, and changed in two steps

```sg
struct Scores {
    List<nint> points
}

nint best(read Scores a, read Scores b) {
    nint top = 0
    for p in read a.points {
        if p > top {
            top = p
        }
    }
    for p in read b.points {
        if p > top {
            top = p
        }
    }
    return top
}

void main() {
    var mine = new shared checked Scores { points = List<nint> { 3, 9 } }
    var team = share mine
    mine.points.add(mine.points.length())
    team.points.add(mine.points.length() * 4)
    print(mine.points)
    print(best(read mine, read team))
}
```

```text
[3, 9, 2, 12]
12
```

`mine` and `team` are one value, lent twice to read in one call, which any number of readers may be. In
`team.points.add(...)` the list is lent to read while the argument reads it, and to change only once the
argument is there.

### A tree that adds up to its root

```sg
alias FolderRef = shared checked Folder

struct Folder {
    string name
    long bytes
    weak checked Folder? parent
    List<FolderRef> children

    FolderRef add(read FolderRef this, string name, long bytes) {
        var child = new FolderRef { name = name, bytes = bytes, parent = weak this, children = List<FolderRef>() }
        this.children.add(share child)
        FolderRef? up = share this
        while up is not null {
            up.bytes += bytes
            up = share up.parent
        }
        return child
    }
}

void main() {
    var home = new FolderRef { name = "home".clone(), bytes = 0, parent = null, children = List<FolderRef>() }
    var docs = home.add("docs".clone(), 10)
    var notes = docs.add("notes".clone(), 5)
    _ = home.add("music".clone(), 100)
    print($"{home.bytes} {docs.bytes} {notes.bytes}")
    for child in read home.children {
        print(child.name)
    }
}
```

```text
115 15 5
docs
music
```

`read FolderRef this` borrows the owner, not the value, so the method can make `weak this` and `share this`,
and change the folder and each one above it. A folder owns its children and looks at its parent, so the tree
holds no circle.

### A list that walks both ways

```sg
struct Node {
    string word
    shared checked Node? next
    weak checked Node? prev
}

struct Line {
    shared checked Node? first
    weak checked Node? last

    void add(ref this, string word) {
        var tail = share this.last
        var node = new shared checked Node { word = word, next = null, prev = weak tail }
        if tail is null {
            this.first = share node
        } else {
            tail.next = share node
        }
        this.last = weak node
    }
}

void main() {
    var line = Line { first = null, last = null }
    for word in "the cat saw the dog".split(" ") {
        line.add(word.clone())
    }
    var at = share line.first
    while at is not null {
        print(at.word)
        at = share at.next
    }
    var back = share line.last
    while back is not null {
        print(back.word)
        back = share back.prev
    }
}
```

```text
the
cat
saw
the
dog
dog
the
saw
cat
the
```

Each node owns the next and looks at the one before it. `tail.next = share node` changes a node that the
list owns through another owner, `tail`. When `line` ends, the first node goes, then the next, down the line.

## What stops the program

### A change while a loop reads

```sg panic
struct Queue {
    List<int> jobs
}

void main() {
    var queue = new shared checked Queue { jobs = List<int> { 1, 2, 3 } }
    var worker = share queue
    for job in read queue.jobs {
        if job == 2 {
            worker.jobs.add(4)
        }
    }
}
```

```text
panic: this `Queue` is being read, so it cannot be changed here
  a `shared checked` value is lent to one that changes it or to any number that read it, not both at once
  --> example.sg:10:13
 10 |             worker.jobs.add(4)
    |             ^^^^^^
  it is being read from here:
  --> example.sg:8:21
 8 |     for job in read queue.jobs {
   |                     ^^^^^
```

`queue` and `worker` are two owners of one queue, so the compiler cannot tell. An `add` could move the items
the loop is reading; the check stops it first.

### Lent to change twice in one call

```sg panic
struct Account {
    long balance
}

void transfer(ref Account from, ref Account to, long amount) {
    from.balance -= amount
    to.balance += amount
}

void main() {
    var alice = new shared checked Account { balance = 100 }
    var bob = new shared checked Account { balance = 50 }
    transfer(ref alice, ref bob, 30)
    print($"{alice.balance} {bob.balance}")
    var payee = share alice
    transfer(ref alice, ref payee, 10)
}
```

```text
70 80
panic: this `Account` is being changed, so it cannot be changed here
  a `shared checked` value is lent to one that changes it or to any number that read it, not both at once
  --> example.sg:16:25
 16 |     transfer(ref alice, ref payee, 10)
    |                         ^^^^^^^^^
  it is being changed from here:
  --> example.sg:16:14
 16 |     transfer(ref alice, ref payee, 10)
    |              ^^^^^^^^^
```

The first transfer is between two accounts. The second would give one account to the call as two `ref`
borrows, which a `ref` never shares. `transfer(ref alice, ref alice, 10)` is refused by the compiler.

### A change on the way to what is lent

```sg panic
struct Node {
    List<int> items
    shared checked Node? next
}

void walk(read shared checked Node head, shared checked Node other) {
    match read head.next {
        null => print("end")
        var n => {
            for x in read n.items {
                other.next = null
                print(x)
            }
        }
    }
}

void main() {
    var tail = new shared checked Node { items = List<int> { 1, 2, 3 }, next = null }
    var head = new shared checked Node { items = List<int>(), next = tail }
    walk(read head, share head)
}
```

```text
panic: this `Node` is being read, so it cannot be changed here
  a `shared checked` value is lent to one that changes it or to any number that read it, not both at once
  --> example.sg:11:17
 11 |                 other.next = null
    |                 ^^^^^
  it is being read from here:
  --> example.sg:7:16
 7 |     match read head.next {
   |                ^^^^
```

`head.next` is the only owner of the second node. Setting it to `null` through `other` would free the node
whose items the loop reads, so the `match` holds `head` too, not only the node it looks into.

## What the compiler refuses

### A change the compiler can see

```sg error
struct Queue {
    List<int> jobs
}

void main() {
    var queue = new shared checked Queue { jobs = List<int> { 1, 2, 3 } }
    read queue as q {
        print(q.jobs.length())
        queue.jobs.add(4)
    }
}
```

```text
error: `queue` is being read for the whole block, so it cannot be changed here: this line would stop the program every time it ran
  a `shared checked` value is lent to one that changes it or to any number that read it, not both at once
  --> example.sg:9:9
 9 |         queue.jobs.add(4)
   |         ^^^^^
  it is being read from here:
  --> example.sg:7:10
 7 |     read queue as q {
   |          ^^^^^
  hint: open it with `ref queue as ...` and change it through that name
```

### A borrow kept past its line

```sg error
struct Queue {
    List<int> jobs
}

void main() {
    var queue = new shared checked Queue { jobs = List<int> { 1, 2, 3 } }
    var jobs = read queue.jobs
    print(jobs.length())
}
```

```text
error: this borrows what a `shared checked` value holds, and such a borrow ends with its line; `jobs` would keep it
  --> example.sg:7:16
 7 |     var jobs = read queue.jobs
   |                ^^^^^^^^^^^^^^^
  it is borrowed here:
  --> example.sg:7:21
 7 |     var jobs = read queue.jobs
   |                     ^^^^^
  hint: lend it for a block instead: `read queue as value { var jobs = read value.jobs ... }`
```

A variable would look at the list while another owner changes or frees it. The block shows how long the
borrow lasts, and the value is checked for all of it.

### `checked` alone

```sg error
struct Queue {
    checked List<int> jobs
}

void main() {
}
```

```text
error: `checked` is written after `shared` or `weak`: it is what makes a value with several owners changeable
  --> example.sg:2:5
 2 |     checked List<int> jobs
   |     ^^^^^^^
  hint: `shared checked T` for an owner, `weak checked T` for a look at one
```

A field with one owner is changed through its struct; nothing about it needs checking while the program runs.

### Started as a task of its own

```sg error
struct Queue {
    List<int> jobs
}

async nint fill(shared checked Queue queue) {
    await Task.delay(1)
    queue.jobs.add(4)
    return queue.jobs.length()
}

async void main() {
    var queue = new shared checked Queue { jobs = List<int>() }
    var task = fill(share queue)
    print(await task)
}
```

```text
error: `fill` takes a `shared checked Queue` here, and a call without `await` starts it as a task of its own, which this cannot go to: a `shared checked Queue` is checked within one task; one that several tasks change is `shared locked`
  what goes to another task, comes back from one or is held by a `locked` value owns all it holds: no borrow, no `shared checked` value, no `Task`
  --> example.sg:13:21
 13 |     var task = fill(share queue)
    |                     ^^^^^^^^^^^
```

`print(await fill(share queue))` runs `fill` in this task, and is taken.
