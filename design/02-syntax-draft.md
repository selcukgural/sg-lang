# Syntax draft

## Current variant: ownership, C#-style syntax

*5 November 2026. The rules come from Rust, the syntax is ours. Decisions and open questions are in the
[decisions](04-decisions-and-open-questions.md) file.*

### Name mapping

| Rust | Draft |
|---|---|
| `Box<T>` | `T*` |
| `Box::new(x)` | `new x` |
| `Option<T>` | `T?` |
| `&T` | `read T` |
| `&mut T` | `ref T` |
| `'a` | `from x` |
| `trait` | `interface` |
| `T: Ord` | `where T : Comparable` |
| `Result<T, E>`, `Err(e)` | `Result<T, E>`, `Error(e)` |
| `x?` | `try x` |
| `if let P = x` | `if x is P` |
| `fn(A) -> B` (a function pointer; closures have no counterpart) | `Func<A, B>` |
| `&[T]`, `&mut [T]` | `read T[]`, `ref T[]` |
| `pub` | `public` |
| `mod stock;` + file | folder, no declaration |
| `use stock::*` | `use stock` |

### Linked list

```
public struct Node {
    int value
    Node*? next                 // Node* is the owner
}

public Node* push(Node*? list, int v) {
    return new Node { value = v, next = list }   // list was moved here
}

public int sum(read Node*? list) {
    var total = 0
    var cur = list              // cur borrows the list
    while cur is not null {
        total += cur.value
        cur = read cur.next     // ownership does not change
    }
    return total
}

void main() {
    Node*? list = null
    list = push(list, 1)
    list = push(list, 2)
    print(sum(read list))
}                               // list is freed automatically here
```

### Compile errors

```
Node* first = push(null, 1)
Node* second = push(first, 2)
print(sum(read first))          // error: first was moved on the previous line

var items = List<int>()
items.add(1)
var x = read items[0]
items.add(2)                    // error: x is still in use, add may move the list
print(x)
```

### Source notation

```
// A single borrowed parameter and read on the return: nothing is written, the return comes from it
read int value(read Node* n)

// With more than one, from is required: the returned borrow may come from a or b
read string longest(read string a, read string b) from a, b

// from describes the return value, where describes the type constraint
read T max<T>(read T a, read T b) from a, b where T : Comparable

// Every type that holds a borrow declares its source
public struct Token<from source> {
    int kind
    read string text            // part of the input, not a copy
}

// If the borrow is hidden in the type name, from is required even with a single parameter
Token firstToken(read string input) from input

public struct Lexer<from source> {              // the source name is part of the type, the private field is not exposed
    read string input                            // single source: no from on the field
    int pos

    public Token next(ref this) from source      // the Token comes from the input, not from the lexer
}

Token peek(ref Lexer lx) from lx.source          // outside the type it is written as a path

// Rare case: if another parameter, not the return value, stores the borrow, the subject is written
void add(ref Registry r, read string name) where r from name

// A struct carrying two sources names them
public struct Match<from text, from pattern> {
    read string found from text
    read string rule from pattern
}
```

### Shared ownership

| Rust | Proposal | What it does |
|---|---|---|
| `Rc<T>`, `Arc<T>` | `shared T` | Several owners, the counter is always atomic |
| `Weak<T>` | `weak T` | Not an owner, does not keep the object alive |
| `RefCell<T>` | `checked T` | Borrow rules at runtime, a violation panics |
| `Mutex<T>` | `locked T` | Accessed only inside a `lock` block |
| `*mut T`, `*const T` | `raw T` | Dereference only inside `unsafe { }` |

`new` stays the only keyword that allocates memory. The counterpart of `Rc::clone` is `share`, so that the counter increment stays visible:

```
var a = new Node { value = 1 }                   // Node*
var b = new shared Node { value = 2 }            // shared Node
var c = share b                                  // second owner, the counter goes up
var d = b                                        // move, the counter does not change
var w = weak b                                   // weak Node
var e = share w                                  // shared Node?, null if the object is dead
```

`ref` is the original itself, and `T?` is part of the borrowed type. Since `read` cannot write, `read T?`
has a single meaning. `shared T?` says the pointer may be absent:

```
void clear(ref Node*? list)                      // Rust: &mut Option<Box<Node>>, the caller's variable may become null
(ref Node)? find(ref Tree t, int key)            // Rust: Option<&mut Node>, the borrow may be absent
read Node? find(read Tree t, int key)            // Rust: Option<&Node>
read byte[]? header(read this, read string name) from this   // Rust: Option<&[u8]>; only read (1774)
shared checked DNode? next                       // Rust: Option<Rc<RefCell<DNode>>>
```

`T*` cannot be null, and `null` cannot be assigned through `ref T*`. Three intents, three signatures:

```
void addAge(ref Person person)                   // modify the object
void finish(Person* person)                      // take the object and destroy it, the caller can no longer use p
void clear(ref Person*? person)                  // I may empty the variable
```

Doubly linked list:

```
public struct DNode {
    int value
    shared checked DNode? next
    weak checked DNode? prev
}

void main() {
    var a = new shared checked DNode { value = 1, next = null, prev = null }
    var b = new shared checked DNode { value = 2, next = null, prev = weak a }
    a.next = share b                 // write borrow for the duration of the expression, panics on conflict
    print(a.next.value)              // error: a.next may be null
}                                    // no cycle because prev is weak, both are freed
```

Each access to a `checked` value is checked against the borrows of it in progress: one that changes it, or any
number that read it. A read or a write borrows nothing after it (`a.count += 1`); a value lent to a call is borrowed
until the call returns, and a borrow at the head of a `for` or a `match` lasts for the whole statement. A local
variable cannot hold this borrow. If several lines are needed, a block is written in the same form as `lock`:

```
for child in read folder.children { ... }        // folder is being read for the whole loop

var kids = read folder.children                  // error: the borrow ends with the expression

for child in read folder.children {
    folder.children.add(item)                    // error: the same variable is being read for the whole loop
}

read folder as f { print(f.name) }               // f: read Folder, being read for the whole block
ref folder as f { f.bytes = 0 }                  // f: ref Folder
```

An alias for long types. It is not a new type and disappears at compile time. `this` can also take a type, so a method
can write `weak this` and `share this`:

```
public alias FolderRef = shared checked Folder

public struct Folder {
    weak checked Folder? parent
    List<FolderRef> children

    public FolderRef child(read FolderRef this, read string name) {
        var created = new FolderRef(name.clone(), weak this)
        this.children.add(share created)
        return created
    }
}
```

`read this` is short for `read Folder this`. The full example is in the [example programs](05-examples.md) file.

The result of `share` is a single `null`: `x is null` answers whether it is linked, `share x` answers whether it is
alive. On `shared T`, `.clone()` means `share`, and `List<shared T>.clone()` shares the elements.

A counter shared by two tasks. `Task.run` starts a function in parallel and gives a `Task<R>`, as in C#; the function
and its arguments are given apart, since there are no lambdas. Because a panic stops the process, there is no lock
poisoning and no `.unwrap()`:

```
void work(shared locked int counter) {
    lock counter as c {              // c: ref int, the lock is released when the block ends
        c += 1
    }
}

void main() {
    var counter = new shared locked 0
    var t1 = Task.run(work, share counter)
    var t2 = Task.run(work, share counter)
    t1.wait()                        // a task that is not waited for on every path is a compile error
    t2.wait()
    lock counter as c { print(c) }
}
```

If an argument of `Task.run` carries a borrow or an unlocked `checked`, it is a compile error. Because the counter
is always atomic, Rust's `Send` and `Sync` distinction comes down to this single rule. A `lock` block does not wait:
another `lock`, a `wait`, or a call of a function that does one inside it is a compile error, and two values are locked
together with `lock a as x, b as y { }`, so locks and waits cannot deadlock. The task a call gives can be waited for
in the same line: `Task.delay(100).wait()` pauses code that is not async. The rules are in [the decisions by topic](rules/README.md)
(rows ["Starting a task"](rules/concurrency.md#starting-a-task) to ["Tasks in the interpreter"](rules/concurrency.md#tasks-in-the-interpreter)), and async is to come the same way (["Async, the direction"](directions/async.md)).

A task that waits until a locked value is ready writes `when` after the names. The wait is at the head of the block
only, with the locks let go; the block itself still waits for nothing. Every `lock` block on the value that ends
wakes those that wait, so nothing is told by hand, as a condition variable must be (C's `cnd_signal`, C#'s
`Monitor.Pulse`, Rust's `notify_one`):

```
struct Jobs {
    List<int> items
    bool done                                    // set by the giver when no more will come
}

int consume(shared locked Jobs jobs) {
    var sum = 0
    while true {
        int? next = null
        lock jobs as q when q.items.length() > 0 || q.done {   // q is `read` in the condition, `ref` in the block
            next = q.items.removeLast()
        }
        if next is null { return sum }
        sum += next
    }
    return sum
}

void finish(shared locked Jobs jobs) {
    lock jobs as q { q.done = true }             // wakes the takers: one ends its wait, the others look again
}

async void take(shared locked Jobs jobs) {
    await lock jobs as q when q.items.length() > 0 { ... }   // in an async function: the task waits, its thread goes on
}

lock jobs as q when q.items.length() > 0 || Stop.load() { }  // error: an `atomic` changes without a `lock`, so it
                                                             // would not wake the task. The clock, random bytes and
                                                             // input and output are errors there too
```

A task left the only owner of what it waits for, with no `weak` look at it, waits for what no other task could
change: a panic, whose hint is the `done` field. The rows ["Waiting under a lock"](rules/concurrency.md#waiting-under-a-lock) of [the decisions by topic](rules/README.md) have the rest.

Tasks also hand each other items through a channel. `Channel<T>(n)` is the end that receives, with room for `n`
items; the senders it makes send. The last sender to end closes it, so there is no `close` to forget, as C#'s
`Writer.Complete()` and Go's `close` can be:

```
void produce(Sender<int> tx, int from) {
    for i in from..from + 100 {
        if !tx.send(i) { return }                // waits while the channel is full; `false`: nobody receives
    }
}                                                // `tx` ends here

int work(Channel<int> jobs) {
    var sum = 0
    for job in jobs { sum += job }               // ends once every sender has ended and nothing is left
    return sum
}

void main() {
    var jobs = Channel<int>(16)
    var a = Task.run(produce, jobs.sender(), 0)
    var b = Task.run(produce, jobs.sender(), 1000)
    var other = Task.run(work, jobs.clone())     // two receivers: each item goes to one of them
    var mine = work(jobs)
    a.wait()
    b.wait()
    print(mine + other.wait())
}

var tx = jobs.sender()
for job in jobs { }                              // error: `tx` lives on in this function, so the loop would never end
int? next = jobs.receive()                       // `null` at the end; `Channel<int?>` is an error
var got = jobs.receive(read token)               // Result<int?, Cancelled>
var item = await jobs.receiveAsync()             // in an async function; `await tx.sendAsync(x)` sends
```

A send or a receive waits, so it is not written in a `lock` block. The rows ["Channels"](directions/async.md#channels) of [the decisions by topic](rules/README.md) have
the rest.

`select` waits on several channels at once and runs the arm of one that can go on, a random one of those ready
together. An arm whose channel has ended is never chosen, and `ended` runs once all have, so a loop over several
channels ends by itself, which Go's channel that is always ready once closed does not:

```
while true {
    select {
        jobs.receive() as job => work(job)       // job: int, not int?
        urgent.receive() as u => handle(u)
        ended => break                           // `break` leaves the loop, not the select
    }
}

var next = select {                              // a value, as a `match` gives
    jobs.receive() as job => job
    results.send(last) => -1                     // chosen once `last` went in; elsewhere `last` is still ours
    ended => 0
}
var got = select (read token) { ... cancelled => ... }  // gives up on a cancel; no `default` arm
await select { ... }                             // in an async function
```

The row ["Waiting on several channels"](rules/concurrency.md#waiting-on-several-channels) has the rest.

### Generics and interfaces

> **Deferred on 12 November 2026.** Generics of your own, `where` and your own `interface` are not in the first
> version; this section is kept as the record of what was designed. What stays: the built-in generic types
> (`Result<T, E>`, `T?`, `List<T>`, `HashMap<K, V>`, `HashSet<T>`), the built-in interfaces written after `:`
> (`Copyable`, `Formattable`, `Equatable`, `Hashable`, `Comparable`), methods in the body of a struct or an enum, and the rule that
> `==` is `equals` and `<` is `compare`. A container of your own is written for the type it holds.

Generic code is compiled separately for each type (monomorphization): as fast as C, at the cost of a larger binary.

A method is written in the body of its struct or enum. `this` is the first parameter and, like the other parameters, states how it is
passed. A function without `this` is called on the type. Fields are accessed with `this.x`.

```
public struct Pair<A, B> {
    A first
    B second

    public read A left(read this) {              // Rust: &self
        return read this.first
    }

    public void setLeft(ref this, A value) {     // Rust: &mut self
        this.first = value
    }

    public B intoRight(this) {                   // Rust: self, the Pair is consumed here
        return this.second
    }
}
```

No `read` or `ref` is written for the receiver at the call: `p.setLeft(1)`.

An interface is written as in C#, and the implementing type states it next to its name. `Self` is the implementing type:

```
public interface Comparable {
    int compare(read this, read Self other)
}

public struct Version : Comparable {
    int major
    int minor

    public int compare(read this, read Version other) {
        if this.major != other.major {
            return this.major.compare(other.major)       // subtraction would overflow at the extremes
        }
        return this.minor.compare(other.minor)
    }
}

read T max<T>(read T a, read T b) from a, b where T : Comparable {
    if a.compare(b) > 0 {
        return a
    }
    return b
}
```

Multiple constraints are separated by commas, and each subject gets its own `where`. A method can put a constraint on
the type's parameter; if the type does not satisfy it, only that call is an error:

```
void report<T>(read TopList<T> top) where T : Comparable, Formattable

public struct TopList<T> where T : Comparable { ... }

public struct List<T> {
    public void sort(ref this) where T : Comparable
}
```

`==` and `<` are shorthand: `a == b` means `a.equals(b)`; `a < b` means `a.compare(b) < 0`. `==` on a type that is not
`Equatable` is a compile error. If the body is not written, the compiler generates it field by field:

```
public interface Equatable {
    bool equals(read this, read Self other)
}

public interface Hashable {
    long hash(read this)
}

public struct Point : Equatable, Hashable {      // can be a HashMap key
    int x
    int y
}
```

Copying and cleanup are interfaces too. Since there are no macros, the bodies of `Copyable` and `Clonable` are
generated field by field by the compiler:

```
public struct Point : Copyable {                 // assignment copies, does not move
    int x
    int y
}

public struct Config : Clonable {                // config.clone() is a deep copy
    string name
}

public struct Connection : Disposable {          // Rust: Drop
    raw sqlite3 db

    void dispose(ref this) {                     // runs by itself when the last owner ends
        _ = unsafe sqlite3_close(this.db)
    }
}
```

All fields of a `Copyable` type must be `Copyable`, and it cannot be combined with `Disposable`. On a generic type,
`: Copyable` is valid if the type parameters are also `Copyable`. Because a panic does not unwind, `dispose` does not
run during a panic, nor after `exit`.

`dispose` is written `void dispose(ref this)` and is never called by the program (`x.dispose()` is an error): it runs
once, where the last owner of the value ends, and then what the value holds ends, field by field. A method that takes
`this` ends the value where it ends, so its `dispose` runs there too; a type that frees something in such a method
keeps a field that says so. A field is never moved out of a `Disposable` value (`take` and `replace` leave it whole).
A type that can fail while it ends is also `Closable`: it has a `close(this)` that gives a `Result`, and a local of
it that is not closed is an error, as for a `StreamWriter`:

```
public struct Deflater : Disposable, Closable {
    raw ZStream stream
    bool ended

    public Result<List<byte>, ZlibError> close(this) { ... this.ended = true ... }
    void dispose(ref this) {
        unsafe {
            if !this.ended {
                _ = deflateEnd(this.stream)
            }
            Memory.free(this.stream)
        }
    }
}

var deflater = try Deflater(6)
try deflater.write(data)
var packed = try deflater.close()                // without it: an error, the end of the stream would be lost
```

A `readonly` field is set only at construction and cannot be assigned afterwards. It differs from `read`: `read` looks
at someone else's data, a `readonly` field owns its own data.

```
public struct Account {
    readonly int id
    int balance

    public void reset(ref this) {
        this.id = 0                              // compile error
        this.balance = 0
    }
}
```

The guarantee goes only as far as "the field cannot be assigned on its own": whoever holds a `ref Account` can replace
the whole object.

Source names are written with `from` so that type parameters and source names can be told apart inside the same `<>`:
`Match<from text, from pattern>`.

In the first version: generic structs, functions and interfaces (such as `Iterator<T>`), `where`, methods, `Self`,
static calls.

Left for later:

- Implementing an interface outside the type's definition (`extend`). Conditional implementations, such as `List<T>`
  being `Comparable` only if `T` is comparable, need this.
- Using an interface as a type: `read Shape`, `Shape*`, the counterpart of Rust's `dyn Shape`.
- Associated types, default method bodies, operator definitions, constant parameters such as `<int N>`.

### Enum and match

Variants carry data. It resembles the C# pattern of an `abstract record` with derived `record`s, but the value does not
go to the heap: it takes the tag plus the size of the largest variant. Full examples are in the
[example programs](05-examples.md) file.

```
public enum Shape {
    Circle(int radius)
    Rect(int width, int height)
    Empty
}

public enum Expr {                               // an enum that contains itself needs T*
    Number(int value)
    Add(Expr* left, Expr* right)
}

var s = Shape.Rect(3, 4)
var e = new Expr.Add(new Expr.Number(1), new Expr.Number(2))

int area(read Shape s) {
    return match s {
        Circle(r) => 3 * r * r
        Rect(w, h) => w * h
        Empty => 0
    }
}

if s is Circle(r) {
    print(r)
}
```

A missing variant is a compile error. `_` catches everything, `var other` also takes the value, and an unknown bare
name is an error. A condition is written with `when`: `Circle(r) when r > 100 => ...`. The condition only looks at values: it cannot
move one, because the arms below still need it if the condition does not hold. An arm body is an expression or
a block; in a block the last line is the value, and `return` exits the function.

`match x` behaves like `var y = x`:

| Syntax | Arm variables | `x` afterwards |
|---|---|---|
| `match s` (`s` is the owner) | Moved | Cannot be used |
| `match read s` | `read` borrow | Can be used |
| `match ref s` | `ref` borrow, fields can be modified | Can be used |
| `match s` (`s` is already `read` or `ref`) | Same kind of borrow | Can be used |

A pattern can stand where a name stands: inside the parentheses go a variant with its own parentheses, a variant that
carries nothing, a number, a character, a text, `_` or a name. Inside the parentheses a name alone is the variant when the
enum there has a variant of that name, otherwise it takes the value; a capitalised name that is not a variant is an error,
so a mistyped variant never takes everything. A `match` that misses a value is a compile error that names one, and so is
an arm the arms above already cover. A `match` on text has texts as its patterns and needs `_` or `var name`. A `match`
on a number or a character needs no `_` when its arms take every value.

Patterns are joined with `or`, as in C#, and each pattern of an `or` names the same values with the same types. Numbers and characters
come in ranges that take both ends, written `..=` (`'a'..='z'`); `..` stops before its end everywhere else, so it is not
a pattern. `true` and `false` are patterns, and so is `null` on a value that may be `null`; any other pattern there takes a
value that is present. Below an arm `null`, a name takes the value without `?`.

```
int kind(char c) {
    return match c {
        'a'..='z' or 'A'..='Z' or '_' => 1
        '0'..='9' => 2
        ' ' or '\t' or '\n' => 3
        _ => 0
    }
}

public enum Tree {
    Leaf(string label)
    Node(Tree*? left, Tree*? right)
}

string first(read Tree? t) {
    return match t {
        null => "none"
        Leaf(label) or Node(Leaf(label), _) or Node(null, Leaf(label)) => label.clone()
        Node(null, null) => "empty"
        Node(_, _) => "deeper"
    }
}

nint size(string? name) {
    return match name {
        null => 0
        var text => text.length()                // a `string`: `null` is taken above
    }
}
```

An enum has methods, written after or between its variants as in a struct; with `match this` it looks at itself.

```
public enum Expr {
    Number(int value)
    Negate(Expr* inner)
    Add(Expr* left, Expr* right)

    public int eval(read this) {
        return match this {
            Number(v) => v
            Negate(Negate(inner)) => inner.eval()        // two minus signs cancel
            Negate(inner) => -inner.eval()
            Add(Number(0), right) => right.eval()
            Add(left, right) => left.eval() + right.eval()
        }
    }
}

int keyword(read string word) {
    return match word {
        "if" => 1
        "else" => 2
        _ => 0
    }
}
```

An enum names the integer type of its numbers before its interfaces, and each variant then stands for a number; such
variants carry nothing. A variant without a number has one more than the variant above it, the first 0. The number is
a const of the enum's type: a number, consts, other variants read through a cast, and operators. Two variants with one
number, or a number that does not fit, are compile errors. An enum that names no type has no numbers: the order of its
lines is not one.

```
enum Block : byte, Copyable, Equatable, Comparable {
    Extension = 0x21
    Image = 0x2C
    Trailer = 0x3B
}

var code = (byte)Block.Image                     // 0x2C, read where the value is: nothing is moved
var wide = (int)Block.Trailer                    // the rules of a cast of a byte
var block = Block.fromValue(data[at]) ?? return Error(UnknownBlock(data[at]))   // a Block?: not every byte is a block
var bad = (Block)data[at]                        // error. Hint: `Block.fromValue(n)`
if Block.Extension < Block.Image { }             // in the order of the numbers, without a `compare` written
```

It goes to C as its number (`(int)mode`) and comes back through `fromValue`: not in a signature of C, a struct of plain
data or raw memory, where C could give a number that is no variant.

The names of `is` are seen on the right of `&&` and in the block of the `if` or `while` whose condition the chain is,
as in C#; not in `else`, after the `if` only when its `else` leaves, and not beside `||`, where the block could run
without them. `x is not null`
narrows `x` the same way, and `x is null` narrows it on the right of `||`, which runs only when `x` is there.

```
int firstPositive(read string text) {
    if int.parse(read text) is Ok(value) && value > 0 {    // `value` on the right and in the block
        return value
    }
    return 0
}

nint lengthOf(read string? text) {
    if text is null || text.length() == 0 {                // on the right of `||` the text is there
        return 0
    }
    return text.length()                                   // the block always leaves: not `null` here
}
```

`!` swaps what a test says, so `!(x is null)` narrows like `x is not null`, and a condition is read as a whole:
where `a && b` holds both sides held, where `a || b` fails both sides failed. A `while` ends only when its condition
fails, so what the failed condition says holds after the loop, unless a `break` leaves it earlier.

```
int area(int? width, int? height) {
    if !(width is null || height is null) {                // both are there
        return width * height
    }
    return 0
}

int firstFound(read List<int> xs) {
    int? found = null
    var wanted = 0
    while found is null {                                  // ends only when `found` is there
        wanted += 1
        found = find(read xs, wanted)
    }
    return found                                           // not `null` here: no `break` in the loop
}
```

The names of `is not` are seen where its test fails: on the right of `||`, in `else`, after an `if` whose block leaves,
and after a `while` that only its test ends. This is C#'s `if (o is not int n) return;`, and what Rust writes as
`let ... else`; `!(x is Ok(n))` is the same test.

```
int doubled(read string text) {
    if int.parse(read text) is not Ok(value) {             // `value` is seen where the test fails
        return -1
    }
    return value * 2                                       // the block left: `value` is here
}

int sum(read string a, read string b) {
    if int.parse(read a) is not Ok(x) || int.parse(read b) is not Ok(y) {
        return 0
    }
    return x + y                                           // both tests failed: both names are here
}
```

### `?.`

`a?.b` is `null` when `a` is, and `a.b` when it is not; the rest of the chain does not run (C#'s rule, also Swift's
and JavaScript's), so `a?.b.c` needs no second `?.`. The value is a `T?`, never a `T??`, and `??` gives a fallback:

```
int? peek(read this) {
    return this.top?.value                                 // `null` for an empty stack
}

var boss = person?.boss?.name ?? "nobody"
print(names.get(5)?.length() ?? -1)
cache?.items.add(key)                                      // a method that changes it: `cache` is lent as `ref`
none?.items.get(expensive())                               // `expensive` does not run
```

A field that is not `Copyable` is lent, not moved: `person?.name` is a `read string?`, and `person` stays lent while it
is used; `person?.name.clone()` owns its copy. A method that takes `this` uses the value up, as `.` does; from a value
with no name, a field is taken, as `make().name` takes it. `a?.b is not null` says that `a` is there:

```
if node?.next is not null {
    print(node.value)                                      // `node` is not `null` here
}
```

`?.` goes only on a `T?`: on a value that is never `null` the hint is `.`, and a `Result` is opened with `try` or `??`.
A method that gives nothing is called through `?.` on a line of its own. There is no `a?.b = 5` and no `a?[i]` yet:
`if a is not null { a.b = 5 }`.

### `?:`

`c ? a : b` is `a` where `c` holds and `b` otherwise, as in C#; only the arm chosen runs. `if` stays a statement, so
a value picked by a condition is written with `?:` (Rust writes `if c { a } else { b }`):

```
var word = n == 1 ? "item" : "items"
int sign(int n) {
    return n > 0 ? 1 : n < 0 ? -1 : 0                     // groups to the right: a chain reads as `else if`
}
var width = p is not null ? p.x : 0                      // `p` is narrowed in the first arm, as in an `if`
var doubled = r is Ok(v) ? v * 2 : 0                     // `v` is seen in the first arm only
var rest = r is not Ok(v) ? 0 : v                        // and with `is not`, in the second
var label = done ? null : name                           // a `string?`
print($"{(n % 2 == 0 ? "even" : "odd")}")                // in parentheses inside `{ }`, where `:` starts the format
```

The condition is a `bool`: a number or a `T?` is not one. The arms have one type, the rule of the arms of a `match`;
an arm that is only a number or `null` takes the type of the other arm, `null` making it a `T?`, and a borrow of a
`Copyable` value (the `v` above) gives the value, as with `??`. A borrow from an `is` name keeps what it was tested
on lent: after `var t = r is Ok(s) ? s : "none"`, `r` is lent while `t` is used. `?:` binds more
loosely than every other operator, `??` too: `a ?? b ? c : d` is `(a ?? b) ? c : d`. It gives a value, so it is not
a statement (`ready ? start() : stop()` is an error whose hint is the `if`), an arm that gives nothing is an error, and
an arm cannot leave the function: `return` goes in an `if` or on the right of `??`. A line can end with `?` or `:` to
go on on the next one, and cannot start with either. Compiled, `?:` is the `if` it reads as.

### Error model

Panic is for programmer errors. Conditions expected from the environment are returned as values with `Result`:

```
public enum Result<T, E> {                       // in the standard library
    Ok(T value)
    Error(E cause)
}

public enum ParseError {
    Empty
    BadDigit(int position)
}

Result<int, ParseError> parse(read string text) {
    if text.length() == 0 {
        return Error(ParseError.Empty)
    }
    ...
    return Ok(value)                             // success is also written explicitly, as in Rust
}

Result<int, ParseError> total(read string a, read string b) {
    var x = try parse(a)                         // if there is an error, it returns to the caller
    var y = try parse(b)
    return Ok(x + y)
}

var n = parse(s) ?? 0                            // the error is discarded
var m = parse(s) ?? panic("number expected")

match parse(s) {
    Ok(v) => print(v)
    Error(e) => print("invalid input")
}
```

`try` is a prefix, like `await`: `(try a()).b()`. On `T?` it returns `null`. The right side of `??` can be a `return`:
`lines.next() ?? return Error(ReadError.Empty)`. Returning through `try` is a normal exit, so `dispose` runs.

With different error types, `try` itself finds the variant in the target type whose single field is the source type:

```
public enum AppError {
    Parse(ParseError cause)
    Io(IoError cause)
}

Result<int, AppError> load(read string path) {
    var text = try File.readText(path)           // IoError becomes AppError.Io
    return Ok(try parse(read text))              // ParseError becomes AppError.Parse
}
```

A `Result` whose value is not used is a compile error. To discard it on purpose, write `_ = parse(s)`. A `Result` that
is assigned to a variable and never unwrapped gives the same error.

So is the value of every call: a function or a method of your own, a built-in method, one of the language's own module
and a function of C, also after `try`. A call made for what it does, whose value is not wanted, drops it with `_ =`,
where the reader sees it:

```
line.trim()                                      // error: the text that `trim` gives is not used
distance.abs()                                   // error: ... it leaves `distance` as it was; hint: `distance = distance.abs()`
xs.removeLast()                                  // error: the `int?` that `removeLast` gives is not used
try cmd.status()                                 // error: the `ExitStatus` that `status` gives is not used
noisy(4)                                         // error: the `int` that `noisy` gives is not used
_ = xs.removeLast()                              // the item is dropped on purpose; the list changed
_ = try cmd.status()                             // run for what it does; how it ended is dropped on purpose
```

A local variable whose value nothing reads is a compile error too, as in Go. Writing it is not reading it: not `x = 1`,
not `x += 1`, not a write to one of its fields or items. A write that goes through a borrow, a `shared` value or raw
memory reaches a place that lives on, and counts as a use; so does every other use, a method called on the variable
and `ref x` given to a call among them. A parameter may go unused, and so may a variable of a `Disposable` type (or of a
struct that holds one), whose use is its end. The name of a `for` or of a pattern that is not needed is written `_`:

```
var total = 10                                   // error: `total` is never used
var p = rect.corner                              // error: `p` is given values that are never read
p.x = 0                                          //   note: `p` is a copy of `rect.corner`; hint: `var p = ref rect.corner`
var q = ref rect.corner                          // a borrow: the write changes `rect`
q.x = 0
for _ in 0..3 { print("again") }                 // `for i in 0..3` with `i` unused is an error
if r is Ok(_) { print("a number") }              // so is `Ok(n)` with `n` unused
```

A value given to a variable after its last read (`n = 2` at the end) is not looked at yet.

A name means one thing in a function. A new local variable, the name of a `for` or of a pattern may not take the name of
a local variable or a parameter of a block around it that is still open, as in C#; Rust's shadowing, which makes a
second variable of the same name, is not there. Blocks side by side may use one name, and so may the code after a
block has ended:

```
string echo(read string text) {
    var text = text.toUpper()                    // error: `text` is already a parameter of `echo`
    ...
}
var total = 0
for x in read xs {
    var total = x * 2                            // error: `total` is already a local variable of a block around this one
}
if r is Ok(total) { }                            // the same error for the name of a pattern
for i in 0..2 { }
for i in 5..7 { }                                // another `i`: the first one has ended
if token is Number(v) { print(v) }
var v = 100                                      // so has the `v` of the pattern
```

### Construction

`new` puts the value on the heap and gives a `T*`; it does not mean creating an object:

```
var a = Expr.Number(1)                           // Expr: the value itself
var b = new Expr.Number(1)                       // Expr*: on the heap, owning pointer
```

`T(args)` is shorthand for the call `T.create(args)`. Because `create` is an ordinary function, it can return an error:

```
public struct Lexer<from source> {
    read string input
    int pos

    public Lexer create(read string input) from input {
        return Lexer { input = input, pos = 0 }
    }
}

var lexer = Lexer(input)
var parser = try Parser(input)                   // create returns a Result
```

A field can have a default value, which a literal that leaves the field out gives it, as in C# and Go; there is no
mark at the literal (Rust's `..`):

```
struct Config {
    string host = "localhost"
    int port = 80
    int retries = twice(3)                       // found when the program is compiled
    int? limit = 5
    string? note = null
    List<string> tags = List<string>()           // empty: nothing is allocated
}

var a = Config {}
var b = Config { port = 8080 }
```

A default is what a const can be, found once when the program is compiled, or `null`, or an empty collection, or a
literal that gives no field (`Options {}`); nothing runs where the literal is. A field that is not `public` still keeps a literal out of other modules,
with or without a default. A field that borrows, and one of a union, has none.

`with` makes a new value from a struct, with the fields written changed (C#'s `with`, Rust's `Point { x: 3, ..p }`):

```
var moved = p with { x = 3 }                     // `p` is moved, or copied when it is `Copyable`
var kept = job.clone() with { title = "copy" }   // a copy is written where it is made
```

The values the fields written held are dropped where `with` is, and a `Disposable` one is disposed of there. `with`
follows the rule of a literal: a struct with a field another module does not see is not copied with `with` there.
In a condition, where `{` starts the block, it needs parentheses, as a literal does.

`HashMap` and `List` are the values themselves and manage their own heap memory, so they do not take `new`. In a
collection initializer each element is an `add` call:

```
var items = List<int> { 1, 2, 3 }
var vars = HashMap<string, int> { {"x", 10}, {"y", 20} }

var other = HashMap<string, int>()
other.add("z", 30)
```

### Integer overflow

Rust's way (decided on 12 November 2026): a debug build and the tests panic on overflow, a release build wraps. Both
are defined; neither can break memory safety, because an index is still bounds checked. The checked and the wrapping
form can be written out and then mean the same in every build:

```
var a = x + y                                    // debug and tests: panics on overflow; release: wraps
var b = x.tryAdd(y) ?? return Error(TooLarge)    // int?, checked in every build, for external input
var c = x.wrappingAdd(y)                         // wraps in every build, where wrapping is what is meant
var s = x.saturatingAdd(y)                       // the nearest end of the type, in every build
var d = x / y                                    // division by zero panics in every build
var e = -x                                       // int.MinValue: panics; tryNegate() and wrappingNegate()
var f = x.abs()                                  // int.MinValue: panics as -x does; tryAbs(), wrappingAbs(),
                                                 //   and unsignedAbs() gives a uint that holds every answer
var g = size.abs()                               // error on a uint: it is never negative
var h = level.clamp(0, 255)                      // a low end above the high end panics; written so, an error
```

Conversion to a smaller type follows the same rule:

```
long big = ...
var a = (int)big                                 // debug and tests: panics if it does not fit; release: cut to size
var b = big.tryToInt() ?? return Error(TooLarge) // int?, checked in every build
var c = big.wrappingToInt()                      // cut to size in every build
var s = big.saturatingToInt()                    // int.MaxValue or int.MinValue if it does not fit
var d = (byte)300                                // error: 300 does not fit in byte
long e = a                                       // widening conversion is implicit
var f = (int)3.7                                 // 3; NaN or out of range panics
```

### Numbers and bit operators

A number can be written in hex, in octal and in binary, with `_` between digits. It is a number like any other and takes the
type it is used as:

```
var id = byte[4] { 0xde, 0xad, 0xbe, 0xef }
var flags = 0b1010                               // int
var mode = 0o755                                 // octal; 010 is ten, not eight
var million = 1_000_000
long wide = 0xdead_beef
int mask = 0xffff_ffff                           // error: `0xffff_ffff` is 4294967295, which does not fit in int
var x = 0XFF                                     // error: `0X` is written with a small letter. Hint: `0xFF`
```

The bit operators work on integers. `&`, `^` and `|` bind tighter than a comparison (not as in C and C#); among
themselves the order is C's, with the shifts just below `+` and `-`:

```
if flags & mask == 0 { }                         // (flags & mask) == 0
var bit = 1 << n + 1                             // 1 << (n + 1)
flags |= 0b100                                   // also &=, ^=, <<=, >>=
flags &= ~0b100
if flags & 2 { }                                 // error: expected bool, found int. Hint: flags & 2 != 0
var both = ready & done                          // error on two bools. Hint: &&
```

A shift has the type of its left side, and the count is any integer. A count below zero, or not below the bits of the
type, follows the overflow rule; the bits that `<<` pushes out are gone in every build:

```
byte high = b >> 4                               // a byte: `>>` brings in zeros on a byte and every unsigned type
var half = -16 >> 2                              // -4: `>>` carries the sign on the signed types
long wide = 1 << 40                              // the expected type reaches the left side: a long is shifted
var big = 1 << 40                                // error: a shift by 40 is out of range: `int` has 32 bits
var y = x << n                                   // n out of range: debug and tests panic; release cuts n to the bits of the type
var z = x << (n & 31)                            // the same in every build
```

The bits of an integer are counted, turned around and rotated by methods, as C23's `<stdbit.h>` has them. A count is
an `int`, so it mixes with an `int` and widens to an index; a signed number is looked at by its bits. A power of two
is a question of the unsigned types:

```
uint mask = 0b1011
var ones = mask.countOnes()                      // 3; also countZeros, leadingZeros, trailingZeros, leadingOnes, trailingOnes
var width = 32 - mask.leadingZeros()             // 4: an int
var turned = mask.rotateLeft(3)                  // the count cut to the bits of the type; rotateRight
var swapped = port.reverseBytes()                // also reverseBits
var size = wanted.nextPowerOfTwo()               // overflow rule: debug panics, release gives 0
var steps = size.log2()                          // rounded down; 0 panics in every build
var odd = count.isPowerOfTwo()                   // error on an `int`. Hint: `((uint)count).isPowerOfTwo()`
```

### Mixing integer types

The integer types are `sbyte`, `byte`, `short`, `ushort`, `int`, `uint`, `long`, `ulong`, `nint` and `nuint`. A type
widens to another on its own only when the other holds every one of its values on every machine (`nint` and `nuint`
count as at least 32 bits). A signed type never becomes an unsigned one on its own:

```
int change = -2
uint total = 10
long a = total                                   // every uint fits in a long
var b = total + change                           // error: `uint` and `int` do not mix. Hint: (long)change
uint c = change                                  // error: expected uint, found int. Hint: (uint)
var d = total + 1                                // a uint: the number takes the type of the other side
var e = -total                                   // error: a `uint` is never negative. Hint: -(long)total
var f = 0xffff_ffff_ffff_ffff                    // error: does not fit in long. Hint: ulong f = ...
ulong g = 0xffff_ffff_ffff_ffff                  // a number is unsigned only where the type says so
```

### Fractional numbers

`float` and `double` are IEEE 754 numbers of 32 and 64 bits. A fractional number has digits on both sides of
the point or an exponent, and like an integer it takes the type it is used as, with `double` where nothing
says otherwise. A whole number written where a fractional one goes is one:

```
var ratio = 0.75                                 // double
float speed = 1.5                                // no suffix: the type says float
var g = 6.674_30e-11                             // the exponent is written with a small e
double half = 1 / 2.0                            // 0.5; 1 / 2 alone is the int 0
var x = 1.5f                                     // error: a number has no `f` at its end. Hint: float x = 1.5
var big = 1e400                                  // error: 1e400 does not fit in double
```

A number becomes a fractional one on its own only when the type holds every value of it exactly, the
rule of the integers. `double` holds every `int` and `uint`, not every `long`; a `float` holds every
integer up to 16 bits. A fractional number never becomes an integer on its own:

```
int count = 7
long big = 1 << 40
double total = 3.5
var a = total / count                            // a double: every int fits in a double exactly
var b = total + big                              // error: `long` and `double` do not mix. Hint: (double)big
var c = total / (double)values.length()          // a length is a nint, which may be 64 bits
float f = total                                  // error: expected float, found double. Hint: (float)
int n = total                                    // error. Hint: (int), which drops the fraction
var d = (int)-3.7                                // -3; out of range or NaN follows the overflow rule
```

The arithmetic is the hardware's and never panics: a division by zero gives `Infinity` or `NaN`, and `%`
keeps the sign of the left side. A `float` is computed as a `float`. Each operation is rounded on its own:
`a * b + c` is never fused into one instruction, so compiled code and the interpreter give the same digits.
The bit operators are for integers only. A number is printed with the fewest digits that read back as the
same number, and of those the nearest (a tie goes to the even digit, as in JavaScript and Python):

```
print(0.1 + 0.2)                                 // 0.30000000000000004
print(2.0)                                       // 2
print(1e21)                                      // 1e+21
print((float)(1.0 / 3))                          // 0.33333334
print(x.sqrt())                                  // also abs, floor, ceil, round, truncate, isNaN, isInfinite, and
                                                 // the math of the next section
```

On floating-point numbers the operators behave as the hardware does, while `equals` and `compare` give a total order (C#):

```
double nan = 0.0 / 0.0
nan == nan                                       // false
nan.equals(nan)                                  // true
values.sort()                                    // works, NaN comes first
var m = HashMap<double, int>()                   // error: double is not Hashable
p == q                                           // two structs: their double fields are compared with equals
match x { 0.5 => ... }                           // error: a fractional number cannot be a pattern
```

`print` does not return an error; if it cannot write, the program stops. Whoever wants to handle the error uses
`Stdout`:

```
print("hello")                                   // stops if the output is closed, exit code 1
try Stdout.writeLine("hello")                    // Result<void, IoError>
```

### Numbers from text, constants and math

`parse` is called on the type and gives a `Result`. The error says where the text stops being a number, or that the
type does not hold it:

```
int ms = int.parse(text) ?? return null          // Result<int, NumberError>
var size = try ulong.parse(field)                // NumberError is passed on, or put into the caller's error
match long.parse(text) {
    Ok(value) => use(value)
    Error(Invalid(at)) => print($"not a number from byte {at}")
    Error(OutOfRange) => print("too large")
}
int.parse(" 12")                                 // Invalid(0): no spaces, no `_`, no `0x`; trim the text first
uint.parse("-1")                                 // Invalid(0): an unsigned type takes no `-`
double.parse("1e400")                            // Infinity, as in Rust, C# and C; OutOfRange is for integers
double.parse("Infinity")                         // what print writes, parse reads back: NaN, Infinity, -0
var n = int.parse(line)                          // error if line is a string. Hint: read line
byte red = byte.parseHex("ff") ?? 0              // 255; small or capital digits, no `0x`, on every integer type
int.parseHex("80000000")                         // OutOfRange: the value must fit, it is not taken as bits
```

The constants are C#'s names, on the type. A constant is a value of its type, not a number written out:

```
int.MinValue, int.MaxValue                       // and on every integer type; nint.MaxValue is the machine's
double.MaxValue, double.MinValue                 // MinValue is the most negative finite number
double.NaN, double.Infinity, double.Pi, double.E // and the same on float
uint u = int.MaxValue                            // error: expected uint, found int. Hint: (uint)
double.Epsilon                                   // error: left out, because C# and C mean two different numbers by it
```

The conversions that do not panic name the type they go to (see Integer overflow). One that cannot fail is an
error:

```
var a = big.tryToInt() ?? return Error(TooLarge) // int?
var b = big.wrappingToUshort()                   // the bits that fit
var c = x.tryToLong()                            // a double: toward zero, null for NaN and out of range
var d = smallByte.tryToInt()                     // error: every byte fits in int. Hint: int x = value
var e = x.wrappingToInt()                        // error on a double: it has no bits to cut to size
var f = (int)big                                 // panic: value 5000000000 does not fit in int
                                                 //   hint: tryToInt() gives null when the value does not fit, ...
```

Math is a set of methods, like `sqrt`. Two numbers meet as with `+`, and `min` and `max` give `NaN` if either side
is one (C#):

```
x.pow(2), x.exp(), x.log(), x.log2(), x.log10()  // log is the natural one
x.sin(), x.cos(), x.tan(), x.asin(), x.acos(), x.atan(), y.atan2(x)
a.fusedMultiplyAdd(b, c)                         // a * b + c, rounded once
a.min(b), a.max(b)                               // on integers too
count.sqrt()                                     // error on an int. Hint: ((double)count).sqrt()
x.sinh(), x.cosh(), x.tanh(), x.asinh(), x.acosh(), x.atanh()
x.cbrt(), x.exp2(), x.hypot(y), x.erf(), x.erfc(), x.gamma(), x.logGamma()
tiny.expMinusOne(), tiny.logOnePlus()            // e^x - 1 and log(1 + x), exact near zero
a % b, a.ieeeRemainder(b)                        // C's fmod and remainder: the quotient cut, or rounded
x.copySign(y), x.isSignNegative(), x.isFinite()
x.roundToEven()                                  // a half to the even neighbour; round() goes away from 0
x.fraction()                                     // what truncate() leaves, with the sign of x: C's modf
x.exponent()                                     // int?: 1000.0 is between 2^9 and 2^10, so 9; null for 0
x.scaleByPowerOfTwo(n)                           // x * 2^n: C's ldexp
x.nextUp(), x.nextDown()                         // the neighbours of x
x.clamp(0, 1)                                    // NaN stays NaN; an end that is NaN panics
x.fmod(y)                                        // error. Hint: `a % b` is C's `fmod`
```

A secret is wiped with zeros the C compiler keeps although nothing reads them again (C23's `memset_explicit`):

```
byte[32] key = ...
Memory.zeroSecurely(ref key)                     // a list, an array or a window of plain data; no unsafe
```

### Text formatting

`$"..."` is written as in C#; since there are no macros, it is a language feature. The expression inside `{}` is
checked at compile time, and `{{` writes a brace. It does not allocate on the heap: the result is a value that borrows
its arguments and implements `Formattable`:

```
print($"{name}: {count}")                        // no allocation
try writer.writeLine($"{a} + {b} = {a + b}")     // no allocation, the parts are written directly
string text = $"{name}: {count}".toString()      // the allocation is visible here
string bad = $"{name}"                           // error, hint: .toString()
```

After the value, as in C#, a width can follow a `,` and a letter a `:`. The width is the characters the value
takes at least, filled with spaces in front of it, or after it when the width is negative; it works on every
value `print` takes and counts characters, not bytes. The letter writes an integer in hex (`x`, `X`), in
binary (`b`) or in decimal (`D`) with at least so many digits, or a fractional number with so many digits
after the point (`F`). Both are part of the program and checked at compile time:

```
print($"#{r:X2}{g:X2}{b:X2}")                    // #FF8000
print($"{crc:x8} {flags:b8} {n:D3}")             // 00c0ffee 00000101 007
print($"{name,-10}|{count,5}|{price,8:F2}")      // "ada       |    7|    3.50"
print($"{(sbyte)-1:x}")                          // ff: a negative number is the bits of its type
print($"{2.5:F0} {0.125:F2}")                    // 2 0.12: the exact value, a tie to the even digit
print($"{price:x}")                              // error: `x` writes an integer in hex, and `price` is a `double`
print($"{count:F2}")                             // error. Hint: {(double)count:F2}
print($"{price:N2}")                             // error: group separators change from one country to another. Hint: F2
print($"{name,width}")                           // error: a width is a number written in the text
```

```
public interface Formattable {
    void format(read this, ref Formatter f)
}

public struct Version : Formattable {
    int major
    int minor

    public void format(read this, ref Formatter f) {
        f.write($"{this.major}.{this.minor}")
    }
}

public enum LogError : Formattable {             // if the body is not written, the compiler generates it
    Io(IoError cause)
    BadLine(long number, string text)
}
```

A write error stays in the `Formatter`, and `writeLine` returns it. No `try` is needed inside `format`.

### The `string` guarantee

A `string` is always valid UTF-8. Every path that could break the guarantee is closed:

```
var text = try File.readText(path)               // invalid byte: IoError.InvalidText(position)
var s = try string.fromUtf8(bytes)               // views the bytes as text, validates, does not copy
byte c = text[0]                                 // readable, indexed by byte
text[0] = 'a'                                    // error: bytes cannot be written into a string
var part = text[0..1]                            // panics if it lands in the middle of a character
```

The single character type `char` is 32 bits and holds any Unicode character (`Rune` in C#). `'+'` adapts to the
expected type, like a numeric literal: a `byte` where an integer goes, a `char` everywhere else:

```
byte b = '+'                                     // byte
char c = '+'                                     // character
char e = 'é'
byte x = 'é'                                     // error: does not fit in one byte. Hint: char x = 'é'
var v = '+'                                      // char (a byte until 13 November)
print('+')                                       // +
if text[i] == ':' { }                            // byte: text[i] is a byte
char s = '\u{1F600}'                             // a character by its code, in hex; also inside "..."
for ch in text.chars() { }                       // ch: char
```

A `char` is not a number. A cast goes to its code and back, and a code that is no character panics in every build,
because a text holds characters only:

```
int code = (int)c                                // never fails
char d = (char)(code + 1)                        // panics if no character has the code
char? f = code.tryToChar()                       // null instead
c + 1                                            // error: a char is not a number. Hint: (int)c
int y = c                                        // error, hint: (int)c
```

The tests that need no tables are ASCII and say so; they are on `byte` too, for a parser that goes byte by byte:

```
c.isAsciiDigit(), c.isAsciiLetter(), c.isAsciiAlphanumeric(), c.isAsciiWhitespace()
c.toAsciiUpper(), text[i].isAsciiDigit()
c.isWhitespace()                                 // Unicode's White_Space: what trim takes off
c.utf8Length()                                   // 1 to 4: the bytes it takes in a text
c.isDigit()                                      // error: no such method. Hint: isAsciiDigit()
```

The tests that need Unicode's tables (17.0) are on `char` only. A character changes its case one for one; a text by
the full mappings, into a new text. No language is taken into account: `"i".toUpper()` is `"I"` everywhere.

```
c.isLetter(), c.isNumeric(), c.isAlphanumeric()  // categories L and N: '٣' and '½' are numeric
c.isUpper(), c.isLower(), c.isControl()          // Lu, Ll, Cc
'ß'.toUpper()                                    // 'ß': one character for one
"straße".toUpper()                               // "STRASSE": a new string
"ΟΔΟΣ".toLower()                                 // "οδος": Σ at the end of a word is ς
"Straße".equalsIgnoreCase("STRASSE")             // true: both folded, nothing allocated
var key = name.foldCase()                        // a key that does not care about case
b.toUpper()                                      // error on a byte. Hint: toAsciiUpper()
"e\u{301}".normalize(NormalizationForm.C)        // "é": one character; D, KC, KD the other forms
"ﬁ²".normalize(NormalizationForm.KC)             // "fi2": what differs only in how it is shown made one
text.isNormalized(NormalizationForm.C)           // the quick check, then the text normalized if unsure
```

### Raw text

C#'s raw text: between three quotes or more, nothing is an escape, so quotes, backslashes and braces are
written as they are. On one line it ends at the first three quotes; on lines, the opening quotes end their
line and the closing quotes stand on a line of their own, and what stands in front of them is taken off every
line:

```
var path = """C:\Users\new\table.txt"""          // a backslash is a backslash
print("""a "quoted" word""")
var json = """
    {
      "name": "tea"
    }
    """                                          // {\n  "name": "tea"\n}: no line break at the end
var help = $"""
    usage: {name} [options]
    count: {count,3} {{each}}
    """                                          // `$` in front: values as in $"...", `{{` for a brace
var inner = """"a """ inside""""                 // four quotes to hold three
var bad = """
      first
    second                                       // error: indented less than the closing quotes
      """
```

A line break in it is always `\n`, whatever the file was saved with. A line with nothing on it may be indented
less. A raw text is a text like any other: a const, a pattern, an argument. Rust writes `r#"..."#` and takes
no indentation off; C#'s `$$"""` (a brace written once) is not taken, so `$"""` reads braces as `$"..."` does.

### Going over text

A text is split and trimmed without copying: each part is a `read string`, a look at the text where it is.

```
for c in text.chars() { }                        // each char
for field in line.split(',') { }                 // keeps the empty parts: "a,,b" gives three
for field in line.split(", ") { }                // the separator is a char or a read string
for word in line.splitWhitespace() { }           // skips the empty parts
for line in text.lines() { }                     // ends at \n; a \r before it is left out
var name = line[..colon].trim()                  // read string; trimStart and trimEnd too
line.indexOf(':'), line.lastIndexOf("--")        // nint?, in bytes
line.contains("GET"), line.startsWith('#'), line.endsWith(".sg")
name.equalsIgnoreAsciiCase("content-length")     // only the ASCII letters are folded
```

Replacing and repeating give a new text and leave the old one as it was, so a call whose result is not used is an
error. The name is not `replace`, because `x.replace(value)` already puts a new value in a place on every type:

```
var wide = line.replaceAll('\t', "    ")          // every place; old and new are a char or a read string
line = line.replaceAll("draft", "final")         // the new text takes the old one's place
print("-".repeat(20))                            // n is a nint; a negative n panics
line.replaceAll("a", "b")                        // error: the new text that `replaceAll` gives is not used
```

Kept in a variable, what `split` gives hands out one part at a time. A part borrows the text, not the iterator, and
`try` passes a missing part on as `null`:

```
Entry? parseLine(read string line) from line {
    var parts = line.split(' ')
    var level = try parts.next()                 // read string; null if the line has ended
    var text = try parts.next()
    int ms = int.parse(text) ?? return null
    return Entry { level = level, ms = ms }
}

for part in line.split(',') {
    line = "x"                                   // error: line cannot be changed while a for goes over it
}
for word in readLine().split(' ') { }            // error: the text has no name and is gone before the loop
line.split("")                                   // error: the separator is empty
```

### Building text

A `string` is where a text is built; there is no separate builder type. `write` adds at its end whatever `print`
takes, and a `$"..."` is written straight in, with no text in between:

```
var line = ""
line.write($"{name}: {count}")                   // the constant is copied on the first write
line.write(',')
var out = string.withCapacity(1 << 20)           // room for a megabyte, taken once
for entry in read entries {
    out.write($"{entry.name},{entry.size}\n")
}
line.clear()                                     // empty again; the memory is kept for the next round

void row(ref string out, int n) {                // built across functions through `ref string`
    out.write(n)
}
```

`write` changes the text, so what looks at it keeps it from being written, and a text is not written into itself:
it may move to a larger block while it grows.

```
s.write(s)                                       // error, hint: var copy = s.clone()
s.write(s.trim())                                // error: the part looks at s
s.write($"{s.length()}")                         // fine: the number is computed before anything is written
var head = s[..3]
s.write("x")                                     // error while head is used later
s += "x"                                         // error, hint: s.write("x")
```

### Files, the input and the command line

A file is read or written whole, or piece by piece through a `StreamReader` and a `StreamWriter`, as text or as bytes;
each call gives a `Result` whose error is the built-in `IoError`. The words of the command line come to `main`; the input is read line by
line into one text:

```
public enum IoError : Copyable, Formattable, Equatable {   // built in
    NotFound
    PermissionDenied
    IsDirectory
    NoSpace
    InvalidText(long position)                   // a byte that is not UTF-8, from the start of the file
    Other(int code)                              // the code the system gave
    Cancelled(Cancelled cancelled)               // async input and output whose token was cancelled
    ConnectionRefused                            // the network: nobody listens at the address
    ConnectionReset                              // the other side broke the connection off
    AddressInUse                                 // the address is taken
    HostNotFound                                 // the host could not be found, or it is not `host:port`
    LineTooLong                                  // the reader of a connection met a line longer than its limit
    NotSeekable                                  // seek on a pipe, the program's input, a writer at the end
    TooLong                                      // fill on a reader whose block holds its limit already
    AddressNotAvailable                          // not an address of the machine, or no local port left
    NotAFile                                     // File.openRegular met a pipe, a device or a socket
    NotATerminal                                 // no terminal where one was asked for
}

Result<void, IoError> main(List<string> args) {  // the words after the program's name
    var text = try File.readText(read args[0])   // checked to be UTF-8
    var bytes = try File.readBytes("data.bin")   // List<byte>
    try File.writeText("out.txt", read text)     // made, or emptied, and written
    try File.appendText("log.txt", "done\n")      // added at the end, made if missing
    try File.writeBytes("copy.bin", read bytes)

    string line = ""                             // one text for every line
    while try Stdin.readLine(ref line) {         // false at the end of the input
        print(line)                              // without its \n or \r\n
    }
    var rest = try Stdin.readText()              // what is left of the input

    var reader = try File.openText("app.log")    // StreamReader, read in blocks of 64 KB
    while try reader.readLine(ref line) { }      // the same methods as Stdin
    for item in reader.lines() {                 // item: Result<string, IoError>, a text of its own
        var next = try item
    }
    var view = reader.buffered()                 // read byte[]: what the block holds, where it lies
    reader.consume(view.length())                // given out; more than it holds is a panic
    var more = try reader.fill()                 // one more read behind what it holds; false at the end
    var writer = try File.createText("out.txt")  // StreamWriter, made or emptied
    try writer.write(42)                         // what print takes
    try writer.writeLine($" lines")
    try writer.close()                           // closing is checked: see below

    var log = try File.openAppend("log.txt")     // StreamWriter after what the file holds, made if missing
    try log.writeLine("started")
    try log.close()

    var input = try File.openText("data.bin")    // bytes come through the same reader
    var output = try File.createText("copy.bin")
    byte[4096] buffer = byte[4096]
    while true {
        var got = try input.readBytes(ref buffer) // fills the buffer unless the end comes first
        if got == 0 { break }                    // 0: the end
        try output.writeBytes(buffer[0..got])    // the bytes as they are
    }
    try output.close()
    return Ok()
}
```

A reader and a writer of a file move with `seek`, which takes a `SeekFrom` and gives the new place, counted from the
start of the file (Rust's `Seek`). `File.openWrite` writes from the start over what a file holds, so a part of it can
be written again; `File.createText` empties it, and a writer of `File.openAppend` writes only at the end and cannot
seek:

```
var writer = try File.createText("table.bin")
try writer.write("size=????\n")                  // the size is known only at the end
try writer.write(read body)
var size = try writer.seek(SeekFrom.End(0))      // what the buffer held is written first
_ = try writer.seek(SeekFrom.Start(5))           // the place is a value: dropped on purpose
try writer.write($"{size:D4}")
try writer.close()

var reader = try File.openText("table.bin")
_ = try reader.seek(SeekFrom.End(-16))           // the last 16 bytes; inside the block, the system is not asked
var here = try reader.seek(SeekFrom.Current(0))  // where it stands
var before = reader.seek(SeekFrom.Start(-1))     // Error(InvalidInput); a place past the end is no error
_ = Stdin.seek(SeekFrom.Start(0))                // error: the input of the program goes only forward

var patch = try File.openWrite("table.bin")      // keeps what the file holds; made if missing
try patch.write("SIZE")
try patch.close()
```

In an async function a reader seeks as it is, since it only asks the system where the file stands; a writer writes
what it holds first, so there it is `await writer.seekAsync(from)`, with a token that may come last.

`Stdin` is a `StreamReader`, and `Stdout` and `Stderr` are `StreamWriter`s. Every function has them, as if the program
had lent them as `ref` parameters, so one function serves a file and the program's own streams:

```
Result<int, IoError> count(ref StreamReader from) { ... }
var lines = try count(ref Stdin)                 // or: try count(ref reader)
try Stdout.writeLine("done")                     // where print writes, in order with it
Stdout.close()                                   // error: Stdout stays open until the program ends
var Stdin = 3                                    // error: the name of a stream of the program
```

`isTerminal()` tells whether a stream is a terminal, as `isatty` does: a program colours its output, or refuses to write
bytes that are not text, only where a person reads them. Every reader and writer has it, a file opened on `/dev/tty`
too; a connection and the pipe of another program are never one. It moves nothing, so a `read` borrow asks it, and it
does not wait, so an async function calls it as it is:

```
if !Stdout.isTerminal() { ... }                  // the output goes into a file or a pipe
bool shown = Stdin.isTerminal()                  // a person types the input
```

`printError` writes like `print`, to the standard error. `exit` ends the program with a code; it takes a `byte`, since
the system keeps eight bits, and it gives no value, so it can stand where one is needed:

```
printError($"wc: {name}: {cause}")               // what print wrote before goes out first
exit(256)                                        // error: 256 does not fit in byte
int n = int.parse(text) ?? exit(2)
var loaded = File.readText(path)
if loaded is Error(cause) {
    if cause == IoError.NotFound {               // IoError is Equatable
        print("no such file")
    }
}
```

### Closing files

`dispose` cannot return an error. Whoever wants to see the write error calls `close`, which returns a `Result`:

```
Result<void, IoError> save(read string path) {
    var writer = try File.createText(path)
    try writer.writeLine("done")
    try writer.close()                           // the buffer is written to disk, the error shows up here
    return Ok()
}
```

`StreamWriter : Closable` is a marker, an empty one like `Copyable`. If a local variable of this type is dropped on a
normal exit without being closed or moved elsewhere, it is a compile error. A normal exit is the end of its block,
`return`, `break`, `continue`, `exit`, and giving the variable another writer; an exit with an error (`try`,
`return Error(...)`) is exempt, because the caller gets the error that made the function stop:

```
Result<void, IoError> save(read string path) {
    var writer = try File.createText(path)
    try writer.writeLine("done")                 // an error exit through try is exempt, dispose runs there
    return Ok()                                  // error: writer was not closed. Hint: try writer.close()
}

_ = writer.close()                               // giving up the close error on purpose

if short {
    try writer.close()
}                                                // error: writer is closed on some paths but not on all
return Error(SaveError.Empty)                    // exempt: the writer writes its buffer as it ends
exit(2)                                          // error: exit ends the program, the buffer would be lost
```

The check looks only at local variables. If the value is moved into a struct field or a list, the compiler stops
tracking it, and closing becomes the responsibility of whoever wrote that struct. A writer that ends without `close`
writes its buffer and loses the error; a panic does not unwind, so there what the buffer holds is lost. In C# the same idea is an analyzer
warning (CA2000); Rust has no such check.

### Loops with a name, and counting down

A loop may have a name, so that a loop inside it can leave it or go on with its next round; the name is written
before `for` or `while`, as in C2y, Go and Java (Rust writes `'outer:`):

```
search: for row in read grid {
    for cell in read row {
        if cell == wanted {
            break search                         // leaves both loops
        }
        if cell < 0 {
            continue search                      // the next row
        }
    }
}
outer: for i in 0..n { print(i) }                // error: no `break outer` or `continue outer` uses the name
```

A name that is not used is an error, as is the name of a loop around it again; side by side, loops may have
the same name. What the rounds left own is let go on the way out, as with `break`. In compiled code it is a
`goto`.

A range is gone over from its end down with `.reversed()`: the same numbers as `a..b`, the last first, and
none when `a` is not below `b`. On an unsigned type it stops at the start without going below it:

```
for i in (0..xs.length()).reversed() { }         // xs.length() - 1 down to 0
for k in (0..n).reversed() { }                   // n is a uint: n - 1 down to 0, never wraps
for i in (0..3) { }                              // error: a range in brackets is gone over with `.reversed()`
```

Rust writes `(0..n).rev()`, Swift `(0..<n).reversed()`.

### Default values of parameters

The last parameters of a function may have default values, which a call that leaves them out gets as if it wrote
them, as in C#:

```
void connect(read string host, int port = 80, int retries = 3) { ... }

connect("example.org")                           // 80, 3
connect("example.org", 8080)                     // 8080, 3

string toJson(read this, JsonOptions options = JsonOptions {})
async Result<string, IoError> fetch(read string url, read CancellationToken token = CancellationToken.None)
```

A default is what the default value of a field can be, or `CancellationToken.None`; it calls no function, so a call
runs nothing it does not show. A call leaves arguments out only at its end, and there are no named arguments. A `ref`
parameter has none, nor a function of C, nor a method the language calls by itself (`format`, `equals`, `hash`,
`compare`, `dispose`). A call through a `Func` gives every argument.

### Function values

`Func<int, bool>` is the type of a function; the last parameter is the return type, and `Func<int, void>` returns
nothing. A value of it is the address of a named function and nothing more: it can be copied, it needs no heap, and it
carries no data. There is no lambda notation and nothing is captured (removed on 12 November 2026: capture brought a
run-time check and about ten rules).

```
bool isPaid(read Order o) {
    return o.paid
}

int count(read List<Order> orders, Func<read Order, bool> test) {
    int n = 0
    for o in orders {
        if test(o) {                             // called like a function
            n += 1
        }
    }
    return n
}

print(count(read orders, isPaid))                // the name of the function is the value
Func<read Order, bool> rule = isPaid             // kept in a variable or a field, copied freely
```

What a function needs comes in as a parameter. A rule that carries data of its own is a variant of an `enum`, and the
`match` does what the lambda body did:

```
enum OrderRule {
    Check(Func<read Order, bool> test)
    MinAmount(long limit)
}
```

A method bound to an object (`validator.accepts`) is not a function value. The full example is "Order rules" in the
[example programs](05-examples.md).

### Slices

A window onto someone else's data: pointer + length. What `read string` is for text, `read byte[]` is for raw data.

```
Result<Request, HttpError> parse(read byte[] data) from data    // Rust: &[u8], C#: ReadOnlySpan<byte>
void fill(ref byte[] target, byte value)                        // Rust: &mut [u8], C#: Span<byte>

parse(read buffer)                               // a List<byte> is passed as a slice, like string → read string
var part = data[..4]                             // slice of a slice, no copy. Ends may be omitted: [..b], [a..], [..]
var last = data[^1]                              // counted from the end: data[data.length() - 1]. C#: data[^1]
var body = line[1..^1]                           // without the first and the last; ^0 is the end itself
var tail = next()[^2..]                          // what is indexed is computed once: one call
var i = ^1                                       // error: ^ is written only first in [ ]; there is no index value
var x = xs[^k + 1]                               // error: what ^ counts is one operand. Hint: xs[^(k + 1)]
fill(ref buffer[4..], 0)                         // a range that is written through: ref in front, a ref byte[]
var part = make()[..4]                           // error: borrows from a value with no name, gone when the line ends.
                                                 // Hint: give the value a name first
var request = "GET /".asBytes()                  // from text to bytes, free
var text = try string.fromUtf8(data[..end])      // from bytes to text: validates, does not copy
byte[] copy = data                               // error: byte[] is not a type on its own.
                                                 // Hint: List<byte> to own, read byte[] for a window

buffer.addAll(block[..size])                     // the items copied to the end of a list: one memcpy.
                                                 // Rust: extend_from_slice, C#: AddRange
buffer.addAll(read buffer)                       // error: buffer is added to itself; it may move while it grows.
                                                 // Hint: buffer.add(buffer[i])
names.addAll(read others)                        // error: addAll copies the items, and string is not Copyable
scores[2..5].sort()                              // a range in front of sort or reverse is lent as ref:
                                                 // those items are put in order where they are
void sortAll(ref int[] items) { items.sort() }   // sort and reverse through a ref T[] and on an array;
                                                 // through a read T[] it is an error. Hint: ref xs
window[0..4].copyFrom(block[8..12])              // the items copied over as many: one memmove. Rust:
                                                 // copy_from_slice. Lengths that differ: a panic
buf[0..n].copyFrom(buf[k..k + n])                // two windows of one place: read first, as memmove does
var left = ref xs[..k]                           // two ref windows of one place that share no item: Rust's
var right = ref xs[k..]                          // split_at_mut. Checked when the second is taken: a panic
merge(ref xs[..mid], ref xs[mid..])              // if they share one. One call too
swap(ref xs[i], ref xs[j])                       // an item is a window of one: i == j is a panic
var a = ref xs[0..3]
var b = ref xs[2..5]                             // error: they share items every time; ends that are numbers
                                                 // are told apart when the program is compiled
var at = sorted.binarySearch(42)                 // nint?: the first item equal to 42, found by halving the
                                                 // items, which are in order (not checked); null if none
var from = sorted.lowerBound(10)                 // nint: the first place whose item is not less than 10
var count = sorted.upperBound(20) - from         // upperBound: greater than 20; so the items from 10 to 20
var inner = sorted[4..].binarySearch(42)         // on a list, an array, a slice and a range: from its start
sorted.binary_search(42)                         // error. Hint: `binarySearch`

var width = ushort.fromLittleEndian(data[6..8])  // a number out of exactly as many bytes as its type takes,
                                                 // the lowest first. Rust: u16::from_le_bytes, Go: binary.LittleEndian
var size = uint.fromBigEndian(data[at..at + 4])  // the highest first; the count is checked, then one load
var bad = ulong.fromLittleEndian(data[at..at + 4])
                                                 // error: ulong.fromLittleEndian reads 8 bytes, and this window has 4.
                                                 // Hint: data[at..at + 8]. A width that depends on data panics
buffer.addAll(read length.toBigEndian())         // the bytes of a number: a byte[4] for a uint, where the value is
```

### Fixed-size arrays

An array whose length is written in the type. It is a value: it lives on the stack or inside a struct and does not
allocate on the heap. A slice (`read T[]`) looks at someone else's data, `T[N]` is the data itself.

```
struct Window {
    int[8] values                                // Rust: [i32; 8]. Inside the struct, 32 bytes
}

var readings = int[5] { 20, 22, 21, 90, 23 }     // elements one by one; a compile error if the count does not match
var buffer = byte[4096]                          // all 0. Only for numbers, bool and T?
var mask = byte[16](255)                         // all 255; the value must be Copyable

buffer[4096] = 1                                 // error: index 4096, length 4096
buffer[i] = 1                                    // i depends on data: panics if out of bounds
var copy = readings                              // 5 elements are copied because int is Copyable
toHex(read id, ref text)                         // lent where a slice is expected, like List
```

The full example is in the [example programs](05-examples.md) file.

A buffer whose size is known only when the program runs is a list, made at its size in one block (1648, 1652):

```
var window = List<byte>.repeat(0, size)         // `size` zeros; zeros are fresh zeroed memory, nothing written
var marks = List<bool>.repeat(false, count)     // any Copyable item: Rust's vec![false; count]
var entries = List<Entry>.withCapacity(count)   // empty, with room for `count`: adding them never moves it
```

### Constants and statics

A const is a value known when the program is compiled, with its type written. At the top of a file (`public const` for
other modules) or in a block, where it is seen to the end of the block:

```
const int BlockSize = 4096
const int BufferSize = BlockSize * 4                     // the compiler computes it
public const double Tau = double.Pi * 2
const uint[] Masks = { 0x1, 0x3, 0x7, 0xF }              // the items counted: uint[4]
const byte[] Logo = embed("logo.gif")                    // the bytes of the file beside this one
const string Greeting = "hello"
const Point Origin = Point { x = 0, y = 0 }              // a Copyable struct of numbers
const int Twice = Masks[1] * 2                           // an item of another const
const int[256] Crc = crcTable()                          // error: a const calls no function (a `static` does)

var buffer = byte[BlockSize]                             // an integer const gives the count of an array
struct Ring { int[BlockSize] items }

match tag {
    Trailer => ...                                       // a const of a number, a char, a text or a bool
    _ => ...
}

Masks[2]                                                 // read where it lies, for the whole run
read uint[] masks() { return Masks }                     // so a borrow of it can be returned
var copy = Masks                                         // a copy, which can be changed
Masks[0] = 1                                             // error: `Masks` is a const
```

A const holds numbers, `bool`, `char`, a text, and arrays, structs and enums of numbers, `bool` and `char`;
not a `List`, an owned text or a pointer. Its value is made of numbers, texts, other consts, operators,
casts, arrays, structs and variants, as in C# and Go; an overflow there is a compile error. A const calls no
function: the compiler runs none of the program's code. A value a function finds is a static (below), and a
table the program needs from its start is written out, made by a script beside the code, as zlib's
`crc32.h` is.

A local never takes the name of a const it sees. In compiled code a number is written where it is used, and
a table is one `static const`, which nothing copies. The rules are in [the decisions by topic](rules/README.md) (["Consts"](rules/types.md#consts), ["Consts,
as built"](rules/types.md#consts-as-built), ["No call in a const"](rules/types.md#no-call-in-a-const)).

`const assert` checks a condition when the program is compiled, at the top of a file or in a block; it is
made as a const is, with no call, and leaves nothing in the program. The size and alignment of a type, which
differ from machine to machine, are measured only here:

```
const assert(BlockSize % 8 == 0)
const assert(Primes.length() == 4, "four primes")
const assert(Masks[3] == 0xF)
const assert(Memory.sizeOf<Header>() == 8, "a header is eight bytes, as the file has it")
const nint Size = Memory.sizeOf<Header>()                // error: a const holds the same value on every machine
const assert(BlockSize == 60)                            // error: const assert failed: BlockSize == 60
                                                         //   left:  64
                                                         //   right: 60
```

One that measures a type is checked for the machine the program is compiled for, and compiled code hands it
to C's `_Static_assert` as well, so a machine the checker did not run on checks it too. What a function gives is
checked by a `test`, or by `assert` when the program runs.
C11 writes `static_assert`, Rust `const _: () = assert!(...)`, Zig `comptime assert`.

A static is one value for the whole program, which lives until it ends and is never assigned. Its value
may be found when the program runs; the type is written, and one of three kinds:

```
static uint[256] Crc = crcTable()                        // read only; found the first time it is used
static atomic long Requests = atomic 0                   // changed by its atomic methods
static locked Config Settings = Config.defaults()        // changed inside a `lock` block

_ = Requests.fetchAdd(1)
lock Settings as s { s.port = 8080 }
var sum = crc(read data)                                 // Crc[i] in there: a `read` borrow of the one table
Requests = 0                                             // error: a static is never assigned

int nextId() {
    static atomic int Last = atomic 0                    // in a function: one for the program, seen only here
    return Last.fetchAdd(1) + 1
}
```

A value the rule of a const allows (numbers, other consts, operators, no call), also inside `atomic` and
`locked`, lies in the program from its start and costs nothing. Any other is found the first time the static
is used, once, by whichever task comes first; the others wait for it, and every later read is one load and one
branch. What finds it cannot wait (`lock`, `wait`, `await`, through the calls), and one that needs itself is a
compile error (`` `A` -> `B` -> `A` ``), or a panic when the circle goes through a function value.

Every task reaches a static, so what it holds follows the thread rule: no borrow, no `shared checked` value,
no `Task`. Nothing in it is closed or disposed of, since a static is never dropped: no file, stream or socket.
There is no static that changes freely and none that each thread has of its own: what changes is atomic or
locked. A number, a `bool`, a `char` or a function is read as a value, anything else lent as `read`; a
`static locked` value only in a `lock` block. A const cannot read a static. Each test starts with the statics
not used yet. The rules are in [the decisions by topic](rules/README.md) (["Globals, as decided"](rules/concurrency.md#globals-as-decided), ["Statics"](rules/types.md#statics), ["Statics, as built"](rules/types.md#statics-as-built)).

### Raw memory and calling C

What the checker cannot follow is written inside `unsafe`: an item of raw memory, moving or casting a raw pointer,
giving memory back, and a call of C. A `raw T` points at plain data (numbers, `bool`, `char`, raw pointers, and
arrays and structs of them), laid out as C lays it.

```
extern "z" {                                     // linked as -lz; extern { } is the C library itself
    CULong compressBound(CULong sourceLen)       // C names as C writes them; uLong is unsigned long
    int compress(raw byte dest, ref CULong destLen, raw byte source, CULong sourceLen)
}
extern {
    nuint strlen(read CString text)              // const char*, size_t
}

var room = unsafe compressBound((CULong)data.length())
var packed = Memory.allocate<byte>((nint)room)   // zeroed, C's calloc; needs no unsafe
CULong size = room
var code = unsafe compress(packed, ref size, data.pointer(), (CULong)data.length())
                                                 // pointer() only as an argument of a C function
var name = try CString.from("tea")               // a zero byte inside is a TextError
var length = unsafe strlen(read name)
var text = try unsafe string.fromCString(p)      // copies up to the zero byte, checks the UTF-8
unsafe {
    packed[0] = 1                                // an item of raw memory
    Memory.free(packed)                          // also memory C gave with malloc
}
```

C's `long` and `unsigned long` are `CLong` and `CULong`: 64 bits on Linux and macOS, 32 on Windows. `long` in
an `extern` block is `int64_t` on every system. The two are types of their own: an `int` becomes a `CLong` and
a `CLong` a `long` without a word, since that holds on every system, and the other way is a cast that panics
where the value does not fit (`(CLong)n`, `n.tryToCLong()`). They are compared and printed and have no
arithmetic, so a sum never overflows on one system only; a number written for one fits in 32 bits.

```
extern {
    CLong labs(CLong value)
}
CLong r = unsafe labs(-7)                        // an int becomes a CLong
long next = (long)r + 1                          // r + 1 is an error: compute in long
CLong back = (CLong)next                         // panics where it does not fit
CLong big = 3000000000                           // error: a number for C's long fits in 32 bits
```

A library has a name on each system: zlib is `-lz` on Linux and macOS and `zlib.lib` on Windows. A block gives
them all, and the first one the C compiler finds is linked; `-L folder` names a folder looked in first, where the
program also looks when it starts:

```
extern "z", "zlib" {                             // the first name found is linked
    uint crc32(uint crc, raw byte buf, uint len)
}
```

```
sg build main.sg -L /opt/zlib/lib                // looked in when linked and when the program starts
sg -L /opt/zlib/lib main.sg                      // the interpreter: before the file, whose words follow it
sg test main.sg --compiled -L /opt/zlib/lib
```

A struct of plain data is laid out as C lays it out, and goes to C and comes back by value. A type of C whose
insides SG does not see is declared in the `extern` block without a body, and is only pointed at:

```
extern "sqlite3" {
    struct sqlite3                               // C's `sqlite3`, seen only as `raw sqlite3`
    int sqlite3_open(read CString path, ref raw sqlite3 db)   // sqlite3**
    int sqlite3_close(raw sqlite3 db)
}
extern {
    Div div(int numer, int denom)                // a struct given back by value
}
struct Div {                                     // the fields of C's div_t, in its order
    int quot
    int rem
}
```

A module that declares the functions of a library without `public` can give its users a safe interface, and they
write no `unsafe`: a `Disposable` type holds what C made and gives it back when it ends.

What a header gives and a library does not (a macro, a `static inline` function, a builtin of the compiler,
assembly) comes through a C file beside the program, compiled with it in one unit; a variable of C is declared
without parentheses, and `errno` is read right after the call it belongs to:

```
extern "./native.c" {                            // a C file of the module's folder
    int count_bits(ulong n)                      // static inline: inlined where it is called
}
extern "curl" {                                  // a library the C file needs; it may declare nothing
}
extern {
    int open(read CString path, int flags)
    raw byte optarg                              // a variable of C
}

var bits = unsafe count_bits(0xff)
var value = try unsafe string.fromCString(optarg)   // read inside unsafe
int fd = unsafe open(read path, 0)
if fd < 0 {
    return Error(IoError.fromErrno(C.errno()))   // NotFound, PermissionDenied, ... or Other(code)
}
C.setErrno(0)                                    // what the next call of C finds
var failed = IoError.fromCode(C.lastError())    // the system's last error: GetLastError on Windows
var wrong = IoError.fromCode(C.errno())          // error: errno is C's number; IoError.fromErrno

nuint at = (nuint)p                              // an address as a number
var back = unsafe (raw int)at                    // and a number as an address
nint count = q - p                               // the items between two pointers of one type
bool before = p < q                              // the order of two addresses
```

A function of C that takes a varying count of arguments is declared with `...` after its parameters. What a
call gives after them says the type C reads: a value C would widen without a word (a `byte`, a `bool`, a
`float`) and a number written without a type are errors whose hint is the cast, since C reads there whatever
it looks for.

```
extern {
    int snprintf(raw byte s, nuint n, read CString format, ...)
    int sscanf(read CString text, read CString format, ...)
}

var length = unsafe snprintf(buffer.pointer(), 64, read format, count, (double)ratio, read name)
var found = unsafe sscanf(read input, read pattern, ref count, ref total)   // the places it writes
_ = unsafe snprintf(buffer.pointer(), 64, read format, 30)    // error: (int)30, or (long)30 for %ld
```

A function of SG that C calls is written with `export`: it takes and gives only what C has, and C sees its
name as it is written, without its module. Its name is a `CFunc`, a pointer to a function as C calls it;
a `Func` does not go to C. A `CFunc` that comes from C (from `dlsym`, or kept in a struct) is called inside
`unsafe`, as every call through a `CFunc` is. There are no lambdas and no global variables, so the state a
callback reaches goes through C's `void*`:

```
extern {
    void qsort(raw void base, nuint count, nuint size, CFunc<raw void, raw void, int> compare)
    int abs(int n)
}
extern "./events.c" {
    void listen(CFunc<raw void, int, void> callback, raw void data)
}

export int compareInts(raw void a, raw void b) { // C calls it as `compareInts`
    var x = unsafe ((raw int)a)[0]
    var y = unsafe ((raw int)b)[0]
    if x < y {
        return -1
    }
    if x > y {
        return 1
    }
    return 0
}

struct Tally {
    int calls
}

export void heard(raw void data, int value) {
    var tally = unsafe (ref Tally)data           // lent again from the address C gives back
    tally.calls += 1
}

unsafe qsort((raw void)items, count, 4, compareInts)
var tally = Tally { calls = 0 }
unsafe listen(heard, Memory.addressOf(ref tally))   // the address of the value, for C to keep
CFunc<int, int> f = abs                          // a function of C, by its address
var g = unsafe (CFunc<int, int>)dlsym(library, read name)
if g != null {
    print(unsafe g(-3))
}
```

That `tally` still lives, and has not moved, while C keeps its address is the program's promise; the
interpreter keeps the addresses it gave and panics on a wrong type, a value that has ended or one that has
moved. A panic inside a function C called ends the process with its message, since nothing unwinds through
C. A C file of the program calls an `export` function by its name: SG declares it before the file.

What C hands a callback is often a pointer and a length. `Memory.view` looks at it as a slice, so a list takes
it in one copy, as libcurl's write callback in `examples/curl` does:

```
export nuint takeBody(raw byte data, nuint size, nuint count, raw void state) {
    var received = unsafe (ref Received)state
    var length = (nint)(size * count)
    received.body.addAll(unsafe Memory.view(data, length))   // a read byte[] of the bytes C gave
    return (nuint)length
}
```

The window borrows `data`, so it does not outlive the variable that holds the pointer; that the memory is
there is the program's promise, inside `unsafe`. While a call of C waits (`curl_easy_perform`), the other
tasks of the program run, in the interpreter too.

What C lays out, SG writes as C has it. A `union` holds one of its fields at a time in the same bytes: it is
built with one, a field is written whole, and read inside `unsafe`, since which field it holds is not known.
A bit field takes a number of bits of its type; a value that does not fit is the rule of overflow, and it has
no address, so it is copied, never lent. A `packed` struct has no padding, and its fields are copied out.
`align(N)` puts a type at a multiple of N wherever it is kept. Each is laid out as C lays it out on the
machine, Windows' rule for bit fields too. A struct that every pattern of bits is a value of is read out of
bytes with no `unsafe`:

```
union Address {                                  // one of its fields at a time, in the same bytes
    uint value
    byte[4] octets
}

struct Ipv4 {
    byte headerLength : 4                        // a bit field: four bits of a byte
    byte version : 4
    byte service
    ushort totalLength
    uint _ : 0                                   // bits left empty, up to the next unit of a uint
    Address source
}

packed struct BmpHeader {                        // no padding: `size` lies at byte 2
    ushort kind
    uint size
}

align(64) struct Counter {                       // at a multiple of 64 bytes, on the heap too
    long count
}

var address = Address { octets = byte[4] { 10, 0, 0, 1 } }
print(unsafe address.value)                      // which field it holds is not known
address.value = 0x08080808                       // written whole: no unsafe
var ip = Ipv4.fromBytes(read packet)             // the bytes as they are; a wrong length is a panic
var header = BmpHeader.fromBytes(bytes[..6])
var size = header.size                           // copied out; `read header.size` is an error
print(Memory.sizeOf<BmpHeader>())                // 6, what C has
var window = unsafe Memory.viewRef(buffer, 64)   // a ref byte[] over raw memory, for C to read
var word = unsafe Memory.readUnaligned((raw uint)(buffer + 1))   // `p[0]` there is a panic in a debug build
```

### The clock

Four types come with the language: `Instant` for measuring, `Duration` for a span of time, `DateTime` for the
calendar in UTC, and `DateTimeOffset` for a moment as the clocks of a place show it. They are written in SG, in the language's own module, and seen from every module with nothing
written; a panic inside that module (a `Duration` too large) is shown at the line of the program that called it. The two clocks are two types, so the time between two readings is taken on one clock.

```
var start = Instant.now()                        // the monotonic clock: never goes back
work()
var took = start.elapsed()                       // a Duration
print($"took {took}")                            // took 1.5s; an Instant itself is not printed
if took > Duration.fromMilliseconds(200) {
    printError($"slow: {took.milliseconds()} ms")
}
var longer = took.plus(Duration.fromSeconds(1))  // methods, not operators: + is an error

var now = DateTime.now()                         // the clock of the system, in UTC; it can be set
print(now)                                       // 2026-11-15T09:30:05.120Z
var due = DateTime.utc(2026, 12, 31, 23, 59, 0) ?? panic("not a date")
                                                 // Result<DateTime, DateError>: InvalidDay for 30 February
print($"{due.year()}-{due.month()} {due.dayOfWeek()}, in {due.since(now).seconds()} s")
var epoch = DateTime.fromUnixSeconds(0)          // 1970-01-01T00:00:00Z

var here = now.toLocal()                         // the system's time zone: 2026-11-15T12:30:05.120+03:00
print($"{here.day():D2}.{here.month():D2}.{here.year()} {here.hour():D2}:{here.minute():D2}")
                                                 // a form of your own, from the parts: 15.11.2026 12:30
var meeting = DateTimeOffset.local(2026, 3, 29, 2, 30, 0)
                                                 // Result: SkippedLocalTime where summer time skips the hour
var east = now.withOffset(Duration.fromSeconds(5 * 3600)) ?? panic("an offset under a day")
var sent = DateTime.parse("2026-11-18T12:30:05+03:00") ?? panic("not RFC 3339")
                                                 // taken to UTC: 2026-11-18T09:30:05Z
```

A `Duration` is a `long` of nanoseconds and may be negative; its arithmetic follows the rule of the numbers (a
panic in a debug build, wrapping in a release build). A `DateTime` holds the years 1677 to 2262. A `DateTimeOffset` keeps its offset through `plus`;
`t.toUtc().plus(span).toLocal()` takes the offset of the new moment. Zones by name (`Europe/Istanbul`) come
later; `Task.delay` takes its milliseconds as an `int`.

### Random numbers

Two types, written in SG in the same module as the clock. `Random` is fast and set by a seed: the same seed gives
the same numbers in the interpreter, in compiled code and on every system, which is what a test or a simulation
needs. `SecureRandom` reads the random bytes of the system, which nobody can work out, for a key or a token.

```
var r = Random.fromSeed(42)                      // or Random.fromSystem(): different on every run
var die = r.nextInt(1, 7)                        // 1 to 6: the upper end is not included
var x = r.nextDouble()                           // from 0 up to 1
var coin = r.nextBool()
roll(ref r)                                      // lent with ref: there is no global generator
var none = r.nextInt(5, 5)                       // panic: no number to choose from
var other = r                                    // a move, not a copy: r cannot be used after it

var token = SecureRandom.hexString(32)           // 32 hex digits, 128 bits
var key = byte[32]
SecureRandom.fill(ref key)
```

A `Random` is not `Copyable`, because a copy would give the same numbers again. Its algorithm is xoshiro256\*\*,
seeded with SplitMix64; whoever sees some of its numbers can work out the rest, so it is never for a secret.
`SecureRandom` does not return an error: if the system ever gave no bytes, the program stops with a panic rather
than go on with bytes someone could guess. Shuffling a list and picking one of its items come later.

### Folders, paths and the environment

`File` deletes, moves and copies files; `Directory` makes, lists and removes folders; `Path` joins and takes
apart paths, which are plain text; `Environment` reads the variables the program was started with. Each call
of the system gives a `Result` with an `IoError`.

```
try File.move(read draft, read final)          // AlreadyExists if final is there: never written over
try File.replace(read temp, read config)       // takes the place of config in one step
try File.copy(read a, read b)                  // a new file; AlreadyExists if b is there
try File.delete(read old)                      // NotFound if it is not there

try Directory.createAll(read Path.join(read root, "logs/2026"))
for entry in try Directory.list(read root) {   // sorted by the bytes of the names
    print($"{entry.name} {entry.kind}")        // File, Directory, Link or Other
}
var info = try File.info(read final)           // info.kind, info.size, info.modified (a DateTime)
try Directory.deleteAll(read scratch)          // a link inside is removed, not followed

var work = try Directory.createTemp()          // a new folder only this user can go into
var scratch = try File.createTemp()            // a new empty file only this user can open; deleting it is ours
var home = Environment.get("HOME") ?? "/"      // a Result<string, VariableError>

try File.createLink("notes-2026.txt", read latest)  // ln -s: the target as it is given
var target = try File.readLink(read latest)    // "notes-2026.txt"; InvalidInput for what is no link
var itself = try File.linkInfo(read latest)    // the link, not followed: itself.kind is Link
try File.createHardLink(read final, read copy) // one more name of the same file
try File.setPermissions(read script, 0o755)    // info.permissions holds the same bits

var db = try File.openWrite(read store)
if !try db.tryLock() {                         // lock, lockShared, tryLockShared, unlock; lockAsync waits
    printError("another program has the store")
}
var map = try unsafe File.mapRead(read big)    // the file where the system keeps it, until map ends
var bytes = map.bytes()                        // a read byte[]

var watcher = try Directory.watchAll(read root) // watch for Directory.watch: what is in it alone
match try watcher.next() {                     // await watcher.nextAsync(read token) in an async function
    Created(path) => print($"new {path}")      // made, or a rename's new name
    Deleted(path) => print($"gone {path}")     // deleted, or a rename's old name
    Changed(path) => print($"written {path}")  // a file; a folder is never changed
    Lost(path) => print($"list {path} again")  // more changes than were kept
}
```

`File.move` never takes the place of a file: the system tells in the same step as the rename that the target
is there. `Path.join` puts a second path that starts with a root after the first (the root left out), and
joins a `..` as it is: it is for the paths a program makes itself. `Path.push(ref path, name)` adds a name to
the end of a path where it lies, by the same rules, so that a path built of many parts is not copied for each
(1802). On Windows `/` and `\` both separate the
parts of a path, `Path.join` writes `\` (`logs\today.txt`), and a path may start with a drive or a share
(`C:\`, `C:`, `\\server\share\`), which `Path.parent` keeps whole. A name from outside the program (a request, an archive) goes through
`Path.joinInside`, which gives an `InvalidInput`, the same on every system, for a name that could lead out of
the folder on any of them: an empty part, a root, a `\` or a `:`, a part that ends with `.` or a space (`..`
among them), a control character (C0, DEL or C1), the name of a device of Windows (`CON`, `nul.txt`). A link inside the
folder is still followed.

```
var saved = try Path.joinInside(read uploads, read request.name)   // "../etc/passwd" is an InvalidInput
```
 The variables of the environment are only read, and the folder the
program works in is not changed: both would be state that every thread sees change. `File.exists` and
`Directory.exists` are there, but what they say can be different the next moment; to use a file, open it and
look at the error.

### Which system

`OperatingSystem.isWindows()`, `OperatingSystem.isLinux()` and `OperatingSystem.isMacOS()` say which system the
program is built for (C#'s `OperatingSystem.IsWindows()`, Rust's `cfg!(windows)`). The answer is known when the
program is built: both branches of an `if` on it are checked on every system, so a mistake in the branch of
Windows is seen on macOS too, and only the branch of the system the program is built for is compiled.

```
var shell = "sh"
if OperatingSystem.isWindows() {
    shell = "cmd.exe"
}
```

A program built without an operating system (`--freestanding`) is on none of them.

### Other programs

`Command` runs another program and waits for it to end. There is no shell: each argument reaches the program as it
is written, so a text from outside never becomes a command of its own. A name without `/` is looked for in the
folders of `PATH` that start with `/`. On Windows a name without `/`, `\` or a drive is looked for as `name.exe`
(unless it has an extension) in the absolute folders of `PATH`, never in the folder the program works in; the
words go on one command line, quoted so that the program reads them back as they were written; a `.bat` or
`.cmd` runs only when named with its extension, through `cmd.exe`, its arguments escaped (a line break in one is
`InvalidInput`).

```
var git = Command("git")
git.args(read List<string> { "log", "-1", "--format=%s" })
git.directory(read repo)                       // the folder it works in; ours stays
git.setVariable("LANG", "C")                   // its environment is a table of its own
var subject = try git.output()                 // what it wrote, as text; its errors go to ours

var tar = Command("tar")
tar.args(read List<string> { "-czf", "backup.tgz", "notes" })
try tar.run()                                  // our output and input; Failed unless it ends with 0

var grep = Command("grep")
grep.args(read List<string> { "-q", "TODO", "notes.txt" })
var found = try grep.status()                  // ExitStatus.Code(1) is no error here
var sort = Command("sort")
sort.input(read lines)                         // written while the output is read
var sorted = try sort.output()
```

`run` and `output` give a `ProcessError`: `Io(IoError cause)` when the program could not be started (`NotFound`,
`PermissionDenied`, `InvalidInput` for a zero byte in a word) or wrote something that is not text, and
`Failed(ExitStatus status)` when it ended with anything but 0. `status` takes no end for an error and gives the
`ExitStatus`: `Code(int code)` or `Signal(int number)`; a code with its highest bit set, as Windows gives
(`0xC0000005`), is printed in hex. A program started while files are open gets none of them. Windows has no
signals to send: there `kill`, `terminate` and `signal` end the program with the code 1, and `Signal` never
comes back from `status`.

`start` starts the program and goes on. The streams asked for with `pipeInput`, `pipeOutput` and `pipeError` are
taken out of the `Process` it gives, once each, and `wait` gives how the program ended:

```
var sort = Command("sort")
sort.pipeInput()
sort.pipeOutput()
var process = try sort.start()                 // Result<Process, IoError>
var input = process.takeInput() ?? panic("asked for")   // StreamWriter?: null once taken
var output = process.takeOutput() ?? panic("asked for") // StreamReader?
try input.writeLine("pear")
try input.close()                              // the program sees the end of its input
var line = ""
while try output.readLine(ref line) { }
var status = try process.wait()                // takes the Process

var sleep = Command("sleep")
sleep.arg("60")
var waiting = try sleep.start()
print(waiting.id())                            // the number the system knows it by
waiting.kill()                                 // the signal 9; terminate() sends 15
var ended = try waiting.wait()                 // ExitStatus.Signal(9)
```

A `Process` in a local variable is waited for on every path, or it is a compile error; one that ends without
being waited for (a way out with an error, a field that ends) stops its program with the signal 9. These calls
wait, so an async function writes the async versions, which take a token that may come last; a cancel stops the
program:

```
var subject = try await git.outputAsync()      // also runAsync(), statusAsync()
var status = try await process.waitAsync(read source.token())
```

A stream goes into a file, comes from one, goes nowhere, or the standard error goes where the output goes, as a
shell's `>`, `>>`, `<` and `2>&1`; programs are joined by pipes with a `Pipeline`, which fails as `set -o
pipefail` says, but not for one before the last that `SIGPIPE` stopped; `tryWait` looks without waiting, and
`detach` lets a program go on after this one, which the runtime takes away once it ends:

```
var build = Command("make")
build.outputTo("build.log")                    // > build.log; appendOutputTo is >>
build.errorToOutput()                          // 2>&1
build.inputFrom("answers.txt")                 // < answers.txt; discardOutput() is > /dev/null
try build.run()

var pipeline = Pipeline()
pipeline.add(Command("yes"))                   // add takes the Command
var head = Command("head")
head.args(read List<string> { "-n", "3" })
pipeline.add(head)
var three = try pipeline.output()              // yes, stopped by SIGPIPE, did not fail
var ends = try pipeline.status()               // List<ExitStatus>, in order

var server = try Command("server").start()
if try server.tryWait() is null {              // ExitStatus?: null while it runs
    server.detach()                            // goes on; never a zombie
} else {
    print(try server.wait())                   // the end tryWait saw, at once
}
print(Process.currentId())                     // also parentId()
try Process.signalId(4242, Signal.Terminate)   // a program by its number; killId(id) sends 9
```

A program starts in a group or a session of its own, as a shell starts a job or a daemon; a job is put in front
at the terminal, and what became of it, paused, gone on or ended, is waited for:

```
var job = Command("vim")
job.foreground()                               // a group of its own, in front before it runs
var editor = try job.start()                   // also newGroup(), newSession(); Pipeline.newGroup()
match try editor.waitChange() {                // also tryWaitChange(), waitChangeAsync(token)
    Stopped(number) => {                       // Ctrl-Z: was paused by signal 20
        try Terminal.takeBack()                // this program in front again
        try Terminal.giveTo(read editor)       // fg
        editor.signalGroup(Signal.Continue)    // killGroup() sends 9 to the whole group
    }
    Continued => {}
    Ended(status) => try Terminal.takeBack()
}
print(editor.groupId())                        // int?: null in the group of this program
Signals.stopSelf()                             // SIGSTOP to this program; Signal has TerminalStop, Continue, Child...
```

A program can be started at a terminal made for it, read and typed into as a person at a terminal window would;
and a program can make its own terminal raw, to read each key as it is pressed:

```
var shell = Command("vim")
shell.terminal(24, 80)                         // a pseudo terminal (ConPTY on Windows); only start()
var vim = try shell.start()
var screen = vim.takeTerminalReader() ?? panic("asked for")   // what it shows: "\n" comes as "\r\n"
var keys = vim.takeTerminalWriter() ?? panic("asked for")     // what it reads as keys: "\r" is Enter
try keys.write(":q\r")
try vim.resizeTerminal(40, 120)                // it gets Signal.WindowChange (Linux and macOS)

var raw = try Terminal.rawMode()               // each byte as typed, no echo, Ctrl-C a byte
var size = try Terminal.size()                 // size.rows, size.columns of this program's terminal
try raw.restore()                              // also where it ends, at a panic, or a signal that ends the program
```

A method can be async too: `public async int total(read this) { ... }`. One that lends `this` is awaited where it
is written; one that takes `this` by value may start as a task of its own.

### Signals

A program does not write a handler of its own, which would run between any two steps of any thread. It listens
for some signals and takes them one at a time, as in Go and Tokio; while a listener lives, its signals do not
end the program:

```
var signals = Signals.listen(read List<Signal> { Signal.Interrupt, Signal.Terminate })
var first = signals.next()                     // waits: Signal.Interrupt when Ctrl-C is pressed
var next = await signals.nextAsync(read token) // Result<Signal, Cancelled>, without holding the thread
Signals.raise(Signal.User1)                    // to this program, as C's raise
waiting.signal(Signal.Hangup)                  // to a program started with Command
```

Several signals of one kind before a call are one, kinds come in the order they came, and every listener of a kind
gets it. Once the last listener of a kind ends, the kind does what it did before, so the next Ctrl-C ends the
program again. `Signal` is `Interrupt`, `Terminate`, `Hangup`, `Quit`, `User1`, `User2`, `WindowChange`, or one of
job control, `TerminalStop`, `Continue`, `TerminalInput`, `TerminalOutput`, `Child` and `Alarm`, with no numbers,
since they differ between systems. A server that stops cleanly cancels its accepts on the first signal
and waits for the connections it has (`examples/shutdown`). On Windows `Interrupt` is Ctrl-C and Ctrl-Break, and
`Terminate` is the console closed, the user logging off and the system shutting down, after which the program has
the few seconds Windows gives it to end; the other kinds never come there, and listening for them is no error.

### Atomic values and `volatile`

An atomic value is an integer, a `bool`, a `float`, a `double` or a raw pointer that every task reads and changes in
one step, through its methods. It is written `atomic T`, anywhere a value is (a field, a local, an item of a list or
an array, raw memory), takes the room of its `T`, and is made with `atomic e`. Its methods work through a `read`
borrow and through `shared`, so tasks share it as a `shared atomic T` or inside a `shared` struct:

```
struct Stats {
    atomic long count
    atomic bool open
}

var stats = new shared Stats { count = atomic 0, open = atomic true }
_ = stats.count.fetchAdd(1)                                 // gives the value before; checked for overflow
var seen = stats.count.load(MemoryOrder.Acquire)
stats.open.store(false, MemoryOrder.Release)
match stats.count.compareExchange(seen, seen + 10) {        // Result<long, long>
    Ok(before) => print($"written over {before}")
    Error(found) => print($"another task wrote {found}")
}
Atomic.fence(MemoryOrder.SequentiallyConsistent)
Atomic.spinLoop()                                           // in a loop that waits for a value
```

The methods are `load`, `store`, `exchange`, `compareExchange` and `compareExchangeWeak`; on numbers `fetchAdd` and
`fetchSub` (an integer also has `wrappingFetchAdd` and `wrappingFetchSub`); on integers and `bool` `fetchAnd`,
`fetchOr` and `fetchXor`. Each is sequentially consistent unless an order is written after the values, as a variant of
`MemoryOrder` (`Relaxed`, `Acquire`, `Release`, `AcquireRelease`, `SequentiallyConsistent`): an order the operation
cannot keep, such as a `load` that is `Release`, is a compile error. `count += 1` and `print(count)` are errors whose
hints are `fetchAdd` and `load`: an atomic value is never read or written without saying so. A struct whose raw
pointers reach memory that several tasks may use is written `struct Pool : unsafe Sendable`, the writer's promise,
and goes to another task.

`volatile` is not a type: a place in raw memory is read or written once, where it is written, with
`unsafe Memory.readVolatile(regs[0].status)` and `unsafe Memory.writeVolatile(regs[0].control, 1)`. It is for the
registers of a device and memory another program writes; it orders nothing between tasks, as C's `volatile` does
not (C#'s `volatile` is an acquire and a release, which SG writes as orders of `load` and `store`).

### A library for C

A folder of SG is built into a library a program of C links: `sg build calclib --library` gives `libcalclib.a`,
`--shared` gives `libcalclib.so` (`.dylib` on macOS), and each comes with `calclib.h`, the header of its `export`
functions. A library has no `main`. What C cannot see into, a struct of SG that holds a list, a text or a map, or an
enum, goes to C by its address:

```
struct Calculator : Disposable {                    // C sees: typedef struct Calculator Calculator;
    Scope scope
    string lastError
    ...
}

export Calculator* calc_new() {                     // C keeps it        Calculator *calc_new(void);
    return new Calculator { ... }
}

export int calc_eval(ref Calculator calc, read CString text, raw double out) {
    var source = string.fromCString(text) ?? return 4           // C's const char*, copied and checked
    ...
}

export double calc_get(read Calculator calc, read CString name) { ... }   // lent for the call

export void calc_free(Calculator*? calc) {          // given back: it ends here, `dispose` too; NULL is nothing
    _ = calc
}
```

No `unsafe` is written: what C gives is C's promise, as Rust's `extern "C" fn` takes `Box<T>` and `&T`. `NULL`
where the function takes none is a panic, and a panic in a library aborts the program, since nothing unwinds through
C. The interpreter also keeps every address it gave C, and catches one it did not give, one of another type, one
given back already, and one lent against the rules of borrowing (freed while a call it is lent to runs). A plain
struct is not held by its address: C takes it by value, with its fields in the header. Only the `export` functions
are seen from outside, so two libraries of SG live in one program; the runtime starts on first use, from any thread.
The full example is `interpreter/examples/calclib`, with a program of C that uses it.

### Without an operating system

`sg build kernel --freestanding --target riscv64-unknown-elf` gives `kernel.o`, which the code that starts the
machine is linked with and calls. There is no `main`: the machine calls the `export` functions. Two names of the root
module have a meaning there:

```
extern {                                            // what the machine gives, linked with it
    void uart_write(raw byte text, nuint length)
    raw byte heap_start()
}

export void kernelMain() {                          // called by the start code
    var name = "hart"
    var line = $"{name} 0 is up".toString()         // memory from `allocate`
    ...
}

void onPanic(read string message) {                 // every panic ends here; then the processor waits
    var bytes = message.asBytes()
    unsafe uart_write(bytes.pointer(), (nuint)bytes.length())
}

raw byte allocate(nuint size, nuint align) { ... }  // where every text, list and `new` comes from
void release(raw byte block, nuint size, nuint align) { ... }   // told the size `allocate` was asked
```

A program without `allocate` and `release` allocates nothing, and the build points at the statement that would.
What needs an operating system is a compile error where it is written, also behind a function of the language's own
module (`Instant.now` reads the clock of the system): `print`, files, the network, tasks and `async`, `lock`,
`HashMap` and `HashSet`, the functions of `<math.h>`. The rest of the language is there, atomics and raw memory with
`Memory.writeVolatile` for the registers of devices among it. The object takes nothing of C's library. The full
example is `interpreter/examples/bare`, run here by a machine of this computer.

The processor that panics waits for ever once `onPanic` returns; the other cores go on running, and stopping them
is the machine's, in `onPanic`.

A kernel drives its devices in SG and keeps in C only what SG has no words for: the start of the core, the vector of
its traps and its own registers, in a C file beside it that is compiled into the same object. There are no global
variables, so the state an interrupt reaches is given to the handler by the vector, from a register of the core
(`mscratch` on RISC-V):

```
extern "./machine.c" {                              // _start, the vector of traps, csrw, wfi
    void machine_set_trap_state(raw void state)
    void machine_enable_timer()
    void machine_wait()
}

struct Kernel {
    atomic ulong ticks                              // changed by the interrupt, read by the loop
    Timer timer
}

export void kernelMain() {
    var kernel = Kernel { ticks = atomic 0, timer = Timer.ofMachine(100_000) }
    unsafe machine_set_trap_state(Memory.addressOf(ref kernel))
    kernel.timer.startNext()
    unsafe machine_enable_timer()
    while kernel.ticks.load() < 5 {
        unsafe machine_wait()
    }
    ...
}

export void onTrap(raw void state, ulong cause, ulong pc) {    // called by the vector for every trap
    if cause == (ulong)1 << 63 | 7 {                // the timer's interrupt
        var kernel = unsafe (read Kernel)state
        _ = kernel.ticks.fetchAdd(1)
        kernel.timer.startNext()                    // Memory.writeVolatile of `mtimecmp`
        return
    }
    panic($"unexpected trap: cause {cause} at 0x{pc:x}")
}
```

`sg build kernel --freestanding --target riscv64-unknown-elf` takes the Clang `CC` names; the object is linked with
the kernel's linker script and nothing else. The full example is `interpreter/examples/kernel`, which QEMU boots.

### The network

A `TcpListener` takes connections and a `TcpStream` receives and sends bytes; an address is one text,
`host:port`. A `UdpSocket` sends and receives datagrams, each one a message that comes whole or not at all, and
says where it goes and where it came from with a `SocketAddress`:

```
var socket = try UdpSocket.bind(":5353")       // a number, localhost, or every v4 address; port 0 for any
var buffer = byte[1500]
var got = try socket.receiveFrom(ref buffer)   // Received: got.count bytes, from got.from
try socket.sendTo(buffer[..got.count], got.from)

var server = try SocketAddress.parse("[::1]:53")         // a number only: InvalidInput otherwise
var all = try SocketAddress.lookup("example.com:53")      // List<SocketAddress>, the name looked up
print(server)                                  // [::1]:53, as it is read
var client = try UdpSocket.bind(":0")
try client.connect(server)                     // send and receive need no address now
try client.send(question)
var n = try client.receive(ref buffer)         // ConnectionRefused when nobody listens there

var peer = try conn.remoteAddress()            // the other end of a TcpStream

var pieces = List<read byte[]>()               // made once, cleared in every round
pieces.clear()
pieces.add(head.asBytes())
pieces.add(body)
try conn.sendPieces(read pieces)               // writev: all of them, in order, with no copy
try conn.closeSending()                        // the other side reads the end; this side still receives
try conn.setNoDelay(false)                     // TCP_NODELAY is on by default; this asks for Nagle's wait
try conn.setKeepAlive(Duration.fromSeconds(30))  // null turns it off
try log.writePieces(read pieces)               // the same pieces into a file, one writev past the block
```

`[::]:8080` takes v4 too on every system, and gives a v4 peer as `127.0.0.1:port`.

A socket of the machine listens at a path instead of a port, and is received from and sent on as a TCP
connection is (Windows has it since 10 1803):

```
var listener = try UnixListener.bind("/run/app.sock")  // the file goes where the listener ends
var conn = try listener.accept()                       // a UnixStream: receive, send, reader(), closeSending()
var client = try UnixStream.connect("/run/app.sock")   // connectAsync in an async function
```

A path longer than the system's address holds (103 bytes on macOS, 107 on Linux and Windows) is
`IoError.TooLong`; a file there at the bind is `AddressInUse`; a listener with no room left refuses
(`ConnectionRefused`) on every system. A `UnixStream` has no address of the network and no option of TCP.

A datagram longer than the buffer, or one too long to send as one message, is `IoError.MessageTooLong`, where
Rust and Go cut it short without a word. Every method of a `UdpSocket` lends it with `read`, so a `shared` socket
serves several tasks at once; there is no `close`, it is closed where it ends. An async function writes the async
versions (`receiveFromAsync`, `sendToAsync`, `receiveAsync`, `sendAsync`, `SocketAddress.lookupAsync`), awaited
where they are written, with a token that may come last.

The network is the same on Linux, macOS and Windows. On Windows a listener and a datagram socket hold their
port alone, so a socket of another program that asks to share it cannot take it on `127.0.0.1` from one
bound to every address; and a datagram sent to a port nobody listens on is told
only to a connected socket, as a refused connection, never to the next receive of one that is not connected
(Windows' own way, which SG turns off). Windows tells of a refused connection after about two seconds, and
`localhost` there tries `::1` before `127.0.0.1`: a server meant for `localhost` is reached at once on
`127.0.0.1`.

### Modules and visibility

A folder is a module; there is no declaration in the file. A name without `public` stays inside the module; the rule
is the same for types, functions, fields and methods.

```
// stock/product
public struct Product {
    public readonly string name                  // readable from outside
    public long price                            // readable and writable from outside
    int quantity                                 // stock module only
}

nint? indexOf(read List<Product> products, read string code) { ... }   // stock module only

// stock/warehouse: same folder, no use needed
this.products[i].quantity += count

// billing/invoice: another module
use stock                                        // Rust: use stock::*, C#: using Stock;

var w = Warehouse()                              // public names come in directly
var line = billing.Line { ... }                  // full name: on a clash and without use
product.quantity = 5                             // error: quantity is not public
```

In nested folders the name is a path: `use shop.stock`. The full example is in the [example programs](05-examples.md)
file.

### Tests

`test "name" { }` is written at the top level of a file and is compiled only by the test command. A test belongs to
the module of the folder it is written in and also sees names that are not `public`. If `assert` is false it panics,
and the message prints the expression and both sides of the comparison.

```
// stock/tests: inside the stock module, no use needed
test "received goods are added to stock" {       // Rust: #[test] fn, C#: [Fact], Go: func TestXxx
    var warehouse = Warehouse()
    warehouse.add(Product("K-1", "pen", 1500))
    try warehouse.receive("K-1", 20)             // works with any error type; an error fails the test and is printed
    assert(warehouse.products[0].quantity == 20) // on failure: expression, left: ..., right: 20
}

assert(index < count)                            // also valid outside tests, enabled in release too
assert(warehouse.ship("K-1", 5))                 // error: assert needs a bool, a Result was given
```

Each test runs in a separate process; when one fails the others continue. There is no `assertEqual`, the single form
is `assert`. The full example is in the [example programs](05-examples.md) file.

## Previous variant: C# direction, generation model

Abandoned, kept for comparison.
*Chosen on 5 November 2026. The notation rules are in [the decisions by topic](rules/README.md).*

```
public struct Node {
    int value
    Node*? next
}

public Node* push(Node*? list, int v, ref Arena arena) {
    return arena.new(Node { value = v, next = list })   // Node*, generation-checked
}

public int sum(Node*? list) {
    var total = 0
    var cur = list
    while cur is not null {     // inside the loop cur: Node*
        total += cur.value      // dereference: the generation comparison happens here
        cur = cur.next          // after the assignment it is Node*? again, the condition checks again
    }
    return total
}

void main() {
    var arena = Arena.init()
    Node*? list = null
    list = push(list, 1, ref arena)   // ref is passed down, allowed
    list = push(list, 2, ref arena)
    print(sum(list))
    arena.deinit()                    // all generations are incremented
    print(sum(list))                  // panic: use after free, with the line number
}
```

## First draft: close to Zig

Kept for comparison, not the current notation.

*Written on 4 November 2026. The notation is not a decision yet; a rough first attempt close to Zig.*

The next step is example programs. Let the model's first test be a linked list, because that is where you see where a second-class stack reference will hurt. In a notation close to Zig it would look roughly like this:

```
struct Node { value: i32, next: ?*Node }

fn push(list: ?*Node, v: i32, a: &Arena) -> *Node {
    let n = a.new(Node { value: v, next: list })   // *Node, generation-checked
    return n
}

fn sum(list: ?*Node) -> i32 {
    var total = 0
    var cur = list
    while cur |n| {            // ?*Node is unwrapped, n: *Node
        total += n.value       // dereference: the generation comparison happens here
        cur = n.next
    }
    return total
}

fn main() {
    var arena = Arena.init()
    var list: ?*Node = null
    list = push(list, 1, &arena)   // &arena is passed down, allowed
    list = push(list, 2, &arena)
    print(sum(list))
    arena.deinit()                 // all generations are incremented
    print(sum(list))               // panic: use after free, with the line number
}
```

Three decisions are tested in this piece: `&arena` is only passed as an argument and is stored nowhere, and the compiler accepts this. `a.new` returns a heap pointer, which is why it can be stored in the `next` field. The last line shows the promise of the design: C would silently read garbage, here it panics.

In the next step we write this and two more programs (a small parser with an arena, a counter shared by two threads) and put the notation under stress.
