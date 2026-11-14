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

If a `spawn` argument carries a borrow or an unlocked `checked`, it is a compile error. Because the counter
is always atomic, Rust's `Send` and `Sync` distinction comes down to this single rule.

### Generics and interfaces

> **Deferred on 12 November 2026.** Generics of your own, `where` and your own `interface` are not in the first
> version; this section is kept as the record of what was designed. What stays: the built-in generic types
> (`Result<T, E>`, `T?`, `List<T>`, `HashMap<K, V>`, `HashSet<T>`), the built-in interfaces written after `:`
> (`Copyable`, `Formattable`, `Equatable`, `Hashable`, `Comparable`), methods in the struct body, and the rule that
> `==` is `equals` and `<` is `compare`. A container of your own is written for the type it holds.

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

Rust's way (decided on 12 November 2026): a debug build and the tests panic on overflow, a release build wraps. Both
are defined; neither can break memory safety, because an index is still bounds checked. The checked and the wrapping
form can be written out and then mean the same in every build:

```
var a = x + y                                    // debug and tests: panics on overflow; release: wraps
var b = x.tryAdd(y) ?? return Error(TooLarge)    // int?, checked in every build, for external input
var c = x.wrappingAdd(y)                         // wraps in every build, where wrapping is what is meant
var d = x / y                                    // division by zero panics in every build
```

Conversion to a smaller type follows the same rule:

```
long big = ...
var a = (int)big                                 // debug and tests: panics if it does not fit; release: cut to size
var b = big.tryToInt() ?? return Error(TooLarge) // int?, checked in every build
var c = big.wrappingToInt()                      // cut to size in every build
var d = (byte)300                                // error: 300 does not fit in byte
long e = a                                       // widening conversion is implicit
var f = (int)3.7                                 // 3; NaN or out of range panics
```

### Numbers and bit operators

A number can be written in hex and in binary, with `_` between digits. It is a number like any other and takes the
type it is used as:

```
var id = byte[4] { 0xde, 0xad, 0xbe, 0xef }
var flags = 0b1010                               // int
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
same number:

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

`Stdin` is a `StreamReader`, and `Stdout` and `Stderr` are `StreamWriter`s. Every function has them, as if the program
had lent them as `ref` parameters, so one function serves a file and the program's own streams:

```
Result<int, IoError> count(ref StreamReader from) { ... }
var lines = try count(ref Stdin)                 // or: try count(ref reader)
try Stdout.writeLine("done")                     // where print writes, in order with it
Stdout.close()                                   // error: Stdout stays open until the program ends
var Stdin = 3                                    // error: the name of a stream of the program
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
fill(ref buffer[4..], 0)                         // a range that is written through: ref in front, a ref byte[]
var part = make()[..4]                           // error: borrows from a value with no name, gone when the line ends.
                                                 // Hint: give the value a name first
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
