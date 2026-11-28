---
name: new
kind: keyword
summary: Puts a value on the heap and gives the pointer that owns it.
see: T*, T?, null, shared, struct, enum
learn: ../docs/learn/book/07-ownership.md#pointers-t-and-new
---

## Forms

| Form | What it is |
|---|---|
| `new Point { x = 3, y = 4 }` | a struct on the heap; the value is a `Point*` |
| `new Expr.Number(1)` | a variant of an enum on the heap: an `Expr*` |
| `new x` | any other value: `new List<int> { 7, 9 }` is a `List<int>*` |
| `T*` | a pointer that owns the value it points to |
| `T*?` | the same, or `null` |
| `new shared x` | a value with several owners: see `shared` |

## What it means

`new` puts a value on the heap and gives a `T*`, a pointer that owns it. It does not mean "make an object": a
struct is made without it and lives where its variable is. A value is never put on the heap by itself, so each
`new` in the code is one allocation, and a `Point` is not taken where a `Point*` is wanted.

Through a `T*`, fields and methods are reached with `.`, as on the value itself, and `print` writes what it points
to. `==` and the order compare what two pointers own: `a == b` on two `Point*` compares the points, as on two
`int*` it compares the numbers. The other operators work on numbers, not on a `T*`; a number needs no `new`. A
borrow of a `T*` goes where a borrow of a `T` is wanted: `show(read p)` for `void show(read Point p)`.

A `T*` has one owner, as every value does. `var q = p` moves the pointer, not the value, and `p` can no longer be
used. The value is freed, with everything it owns, where its owner ends: at the end of its block, when the
pointer is written over, or at the end of the function it was given to. There is no `delete`, no garbage
collector and no count of owners.

A type that holds itself, such as the node of a list or a tree, needs a pointer for the part that repeats:
`Node*? next`. Without it the type would have no size.

## Examples

### A value on the heap

```sg
struct Point : Formattable {
    int x
    int y
}

void main() {
    Point* p = new Point { x = 3, y = 4 }
    p.x += 1
    print($"{p.x}, {p.y}")
    print(p)
}
```

```text
4, 4
Point { x = 4, y = 4 }
```

`p` is a `Point*`; `var p = new Point { ... }` gives it the same type. `p.x` reaches the field through the
pointer, and `print(p)` writes the point, not an address.

### When it is freed

```sg
struct Page : Disposable {
    int number

    void dispose(ref this) {
        print($"free page {this.number}")
    }
}

void main() {
    var page = new Page { number = 1 }
    if true {
        var scratch = new Page { number = 2 }
        print("in the block")
    }
    page = new Page { number = 3 }
    print("end of main")
}
```

```text
in the block
free page 2
free page 1
end of main
free page 3
```

`dispose` runs just before a `Page` is freed. Page 2 ends with its block, page 1 when `page` is written over, and
page 3 at the end of `main`.

### Moving a pointer

```sg
struct Page : Disposable {
    int number

    void dispose(ref this) {
        print($"free page {this.number}")
    }
}

Page* blank(int number) {
    return new Page { number = number }
}

void archive(Page* page) {
    print($"archive page {page.number}")
}

void main() {
    var page = blank(7)
    var moved = page
    print(moved.number)
    archive(moved)
    print("end of main")
}
```

```text
7
archive page 7
free page 7
end of main
```

The page is made once and never copied: `blank` gives the pointer back, `moved` takes it from `page`, and
`archive` takes it from `moved`. It is freed where `archive` ends, since `archive` was its last owner.

### Borrowing through a pointer

```sg
struct Account {
    string owner
    long balance
}

void deposit(ref Account account, long amount) {
    account.balance += amount
}

void show(read Account account) {
    print($"{account.owner}: {account.balance}")
}

void main() {
    var account = new Account { owner = "Ada".clone(), balance = 100 }
    deposit(ref account, 50)
    show(read account)
    var balance = ref account.balance
    balance -= 30
    show(read account)
}
```

```text
Ada: 150
Ada: 120
```

`deposit` and `show` take an `Account`, and are given the one behind the pointer. `ref account.balance` borrows
a field of it.

### A list that points to itself

```sg
struct Node {
    string task
    Node*? next
}

void main() {
    Node*? todo = null
    todo = new Node { task = "write tests".clone(), next = todo }
    todo = new Node { task = "fix the bug".clone(), next = todo }
    todo = new Node { task = "read the report".clone(), next = todo }

    var current = read todo
    while current is not null {
        print(current.task)
        current = read current.next
    }
}
```

```text
read the report
fix the bug
write tests
```

Each new node takes the list so far as its `next`, so the last one made comes first. `current` only borrows: it
walks the list without moving a node. The whole list is freed when `todo` ends.

### A tree changed in place

```sg
struct Node {
    int key
    Node*? left
    Node*? right
}

void insert(ref Node*? tree, int key) {
    if tree is null {
        tree = new Node { key = key, left = null, right = null }
        return
    }
    if key < tree.key {
        insert(ref tree.left, key)
    } else {
        insert(ref tree.right, key)
    }
}

void inOrder(read Node*? tree, ref List<int> out) {
    if tree is null {
        return
    }
    inOrder(read tree.left, ref out)
    out.add(tree.key)
    inOrder(read tree.right, ref out)
}

void main() {
    Node*? tree = null
    var keys = List<int> { 50, 20, 70, 10, 30 }
    for key in read keys {
        insert(ref tree, key)
    }
    var sorted = List<int>()
    inOrder(read tree, ref sorted)
    print(sorted)
}
```

```text
[10, 20, 30, 50, 70]
```

`insert` borrows the place where a node belongs, `ref tree.left` or `ref tree.right`, and puts a new node there
when it is empty. After `if tree is null { ... return }`, `tree` is a `Node*`, so `tree.key` needs no test.

## What the compiler refuses

### A type that holds itself

```sg error
struct Node {
    int value
    Node? next
}

void main() {
    var last = Node { value = 2, next = null }
    print(last.value)
}
```

```text
error: struct `Node` contains itself, so it has no finite size
  --> example.sg:3:5
 3 |     Node? next
   |     ^^^^^
  hint: put it behind a pointer: `Node*? next`
```

### A value where a pointer is wanted

```sg error
struct Node {
    int value
    Node*? next
}

void main() {
    var last = Node { value = 2, next = null }
    var first = Node { value = 1, next = last }
    print(first.value)
}
```

```text
error: expected `Node*`, found `Node`
  --> example.sg:8:42
 8 |     var first = Node { value = 1, next = last }
   |                                          ^^^^
```

`last` lives in `main`, not on the heap. `var last = new Node { ... }` makes it a `Node*`, which `next` takes.

### A pointer used after it moved

```sg error
struct Point {
    int x
    int y
}

void main() {
    var p = new Point { x = 3, y = 4 }
    var q = p
    print(p.x)
    print(q.y)
}
```

```text
error: `p` was moved and can no longer be used
  --> example.sg:9:11
 9 |     print(p.x)
   |           ^
  it was moved here:
  --> example.sg:8:13
 8 |     var q = p
   |             ^
```

A pointer is not copied, so two variables never own the same value. `var q = read p` would only borrow it.

### A pointer moved out through a borrow

```sg error
struct Node {
    string task
    Node*? next
}

void dropFirst(ref Node*? list) {
    if list is not null {
        list = list.next
    }
}

void main() {
    Node*? todo = new Node { task = "write tests".clone(), next = null }
    todo = new Node { task = "fix the bug".clone(), next = todo }
    dropFirst(ref todo)
    print(todo is null)
}
```

```text
error: a `Node*?` field cannot be moved out through a borrow
  --> example.sg:8:16
 8 |         list = list.next
   |                ^^^^^^^^^
  hint: take it with `list.next.take()`, which leaves `null` there
```

A borrow must leave the value whole. With `list = list.next.take()` the next node moves up, and the first node,
written over, is freed.
