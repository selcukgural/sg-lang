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
| `&dyn Fn(A) -> B`, `&mut dyn FnMut(A) -> B` | `read Func<A, B>` |
| `Box<dyn Fn(A) -> B>`, `Box<dyn FnMut(A) -> B>` | `Func<A, B>*` |
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

`checked` borrows the access for the duration of the expression. A borrow at the head of a `for` or `match` lasts for
the whole statement, and a local variable cannot hold this borrow. If several lines are needed, a block is written in
the same form as `lock`:

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
public using FolderRef = shared checked Folder

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

A counter shared by two threads. Because a panic stops the process, there is no lock poisoning and no `.unwrap()`:

```
void work(shared locked int counter) {
    lock counter as c {              // c: ref int, the lock is released when the block ends
        c += 1
    }
}

void main() {
    var counter = new shared locked 0
    var t1 = spawn work(share counter)
    var t2 = spawn work(share counter)
    t1.join()
    t2.join()
    lock counter as c { print(c) }
}
```

If a `spawn` argument carries a borrow, an unlocked `checked` or a `Func`, it is a compile error. Because the counter
is always atomic, Rust's `Send` and `Sync` distinction comes down to this single rule.

### Generics and interfaces

Generic code is compiled separately for each type (monomorphization): as fast as C, at the cost of a larger binary.

A method is written in the struct body. `this` is the first parameter and, like the other parameters, states how it is
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

public struct File : Disposable {                // Rust: Drop
    int handle

    public void dispose(ref this) {              // called automatically when the owner dies
        close(this.handle)
    }
}
```

All fields of a `Copyable` type must be `Copyable`, and it cannot be combined with `Disposable`. On a generic type,
`: Copyable` is valid if the type parameters are also `Copyable`. Because a panic does not unwind, `dispose` does not
run during a panic.

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
name is an error. A condition is written with `when`: `Circle(r) when r > 100 => ...`. An arm body is an expression or
a block; in a block the last line is the value, and `return` exits the function.

`match x` behaves like `var y = x`:

| Syntax | Arm variables | `x` afterwards |
|---|---|---|
| `match s` (`s` is the owner) | Moved | Cannot be used |
| `match read s` | `read` borrow | Can be used |
| `match ref s` | `ref` borrow, fields can be modified | Can be used |
| `match s` (`s` is already `read` or `ref`) | Same kind of borrow | Can be used |

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

`HashMap` and `List` are the values themselves and manage their own heap memory, so they do not take `new`. In a
collection initializer each element is an `add` call:

```
var items = List<int> { 1, 2, 3 }
var vars = HashMap<string, int> { {"x", 10}, {"y", 20} }

var other = HashMap<string, int>()
other.add("z", 30)
```

### Integer overflow

The default is safe; the unchecked path is written on purpose:

```
var a = x + y                                    // panics on overflow, in release too
var b = x.tryAdd(y) ?? return Error(TooLarge)    // int?, for external input
var c = x.wrappingAdd(y)                         // wraps on purpose, for hot loops
```

Conversion to a smaller type follows the same rule:

```
long big = ...
var a = (int)big                                 // panics if it does not fit: value 5000000000 does not fit in int; big.tryToInt()
var b = big.tryToInt() ?? return Error(TooLarge) // int?
var c = big.wrappingToInt()                      // truncates on purpose
var d = (byte)300                                // error: 300 does not fit in byte
long e = a                                       // widening conversion is implicit
var f = (int)3.7                                 // 3; NaN or out of range panics
```

On floating-point numbers the operators behave as the hardware does, while `equals` and `compare` give a total order (C#):

```
double nan = 0.0 / 0.0
nan == nan                                       // false
nan.equals(nan)                                  // true
values.sort()                                    // works, NaN comes first
var m = HashMap<double, int>()                   // error: double is not Hashable
```

`print` does not return an error; if it cannot write, the program stops. Whoever wants to handle the error uses
`Console.out`:

```
print("hello")                                   // stops if the output is closed, exit code 1
try Console.out.writeLine("hello")               // Result<void, IoError>
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
expected type, like a numeric literal:

```
byte b = '+'                                     // byte
char c = '+'                                     // character
char e = 'é'
byte x = 'é'                                     // error: does not fit in one byte. Hint: char x = 'é'
var v = '+'                                      // byte
for ch in text.chars() { }                       // ch: char
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
normal exit without being closed or moved elsewhere, it is a compile error:

```
Result<void, IoError> save(read string path) {
    var writer = try File.createText(path)
    try writer.writeLine("done")                 // an error exit through try is exempt, dispose runs there
    return Ok()                                  // error: writer was not closed. Hint: try writer.close()
}

_ = writer.close()                               // giving up the close error on purpose
```

The check looks only at local variables. If the value is moved into a struct field or a list, the compiler stops
tracking it, and closing becomes the responsibility of whoever wrote that struct. In C# the same idea is an analyzer
warning (CA2000); Rust has no such check.

### Function values and lambdas

A lambda is written as in C#. In the type `Func<int, bool>` the last parameter is the return type. Because `void` is a
type, there is no separate `Action`: `Func<int, void>`. `return` and `try` exit the lambda, and a block body needs
`return`.

| Use | Syntax | Heap |
|---|---|---|
| For the duration of the call | `read Func<A, B>` | None |
| Stored or returned | `Func<A, B>*` | With `new` |

Capture is one sentence: without `new` the lambda borrows, with `new` it moves.

```
public bool any(read this, read Func<read T, bool> test)

long limit = 500
var large = orders.filter(o => o.amount >= limit)    // limit is borrowed
orders.forEach(o => { total += o.amount })           // total is a writable borrow

Func<read Order, bool>* minAmount(long limit) {
    return new o => o.amount >= limit                // limit was copied
}
```

A bare `Func` cannot be a variable type, its size is unknown. Locally, `var` needs a type, as in C#:
`var add = (int x) => x + 1`. If there is an expected type, the parameter type is not written.

A lambda can modify what it captures, with nothing extra to write. The compiler behaves as if it had added `checked`
to a captured variable that is written to, and a panic happens only on a real conflict:

```
Func<int>* makeCounter() {
    int n = 0
    return new () => {                               // n was moved into the lambda
        n += 1
        return n
    }
}
```

Writing afterwards to a variable copied with `new` is a compile error, since the lambda would keep the old value. If
the lambda writes, the variable counts as moved even if it is `Copyable`. Capture is per field:
`this.items.removeAll(x => x > this.limit)` captures only `limit`. A captured owned value cannot be moved out inside
the lambda. A `new` lambda that captures a borrow needs `from`:
`Func<read Order, bool>* startsWith(read string text) from text`.

A lambda cannot cross to a thread; thread work starts with `spawn f(args)`.

A function name can be written in place of a lambda. For a method bound to an object, a lambda is written:

```
bool isPaid(read Order o) { return o.paid }

orders.removeAll(isPaid)                             // same as o => isPaid(o)
orders.removeAll(Order.isCancelled)                  // method, by type name
Func<read Order, bool>* test = new isPaid            // new if it will be stored
orders.removeAll(this.isValid)                       // error. Hint: o => this.isValid(o)
```

List methods run immediately and return a new list. The full example is in the [example programs](05-examples.md) file.

### Slices

A window onto someone else's data: pointer + length. What `read string` is for text, `read byte[]` is for raw data.

```
Result<Request, HttpError> parse(read byte[] data) from data    // Rust: &[u8], C#: ReadOnlySpan<byte>
void fill(ref byte[] target, byte value)                        // Rust: &mut [u8], C#: Span<byte>

parse(read buffer)                               // a List<byte> is passed as a slice, like string → read string
var part = data[..4]                             // slice of a slice, no copy. Ends may be omitted: [..b], [a..], [..]
var raw = "GET /".asBytes()                      // from text to bytes, free
var text = try string.fromUtf8(data[..end])      // from bytes to text: validates, does not copy
byte[] copy = data                               // error: byte[] is not a type on its own.
                                                 // Hint: List<byte> to own, read byte[] for a window
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
*Chosen on 5 November 2026. The notation rules are in the [decisions and open questions](04-decisions-and-open-questions.md) file.*

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

*Source: conversation of 4 November 2026. The notation is not a decision yet; a rough first attempt close to Zig.*

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
