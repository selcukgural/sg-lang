# Example programs

*6 November 2026. The examples were written by hand, before there was any tool to check them. Since 12 November 2026 the [interpreter](../interpreter/README.md) runs a growing part of the language; the stack example runs there as `examples/stack.sg`, and the calculator as `examples/calculator.sg` (the differences from the text below are listed in the interpreter's README); the stock and invoice example with its tests runs as `examples/shop`. The aim is to
put the notation under stress. The decisions are in the [decisions](04-decisions-and-open-questions.md) file.*

## Calculator

Reads an expression such as `1 + 2 * (x - 4)` from a file, parses it and evaluates it. The syntax tree does not copy the
source text; variable names are borrowed from the input.

### Lexer

```
public enum Token<from source> : Copyable {
    Number(int value)
    Name(read string text)           // part of the input, not a copy
    Plus
    Minus
    Star
    Slash
    LeftParen
    RightParen
    End
}

public enum SyntaxError : Formattable {
    BadCharacter(int position)
    NumberTooLarge(int position)
    Unexpected(int position)
}

bool isDigit(byte c) {
    return c >= '0' && c <= '9'
}

bool isLetter(byte c) {
    return c >= 'a' && c <= 'z'
}

public struct Lexer<from source> {
    read string input
    int pos

    public Lexer create(read string input) from input {
        return Lexer { input = input, pos = 0 }
    }

    public Result<Token, SyntaxError> next(ref this) from source {
        this.skipSpaces()
        if this.pos >= this.input.length() {
            return Ok(Token.End)
        }

        int start = this.pos
        byte c = this.input[start]

        if isDigit(c) {
            return this.number(start)
        }
        if isLetter(c) {
            return Ok(this.name(start))
        }

        this.pos += 1
        return match c {
            '+' => Ok(Token.Plus)
            '-' => Ok(Token.Minus)
            '*' => Ok(Token.Star)
            '/' => Ok(Token.Slash)
            '(' => Ok(Token.LeftParen)
            ')' => Ok(Token.RightParen)
            _ => Error(SyntaxError.BadCharacter(start))
        }
    }

    void skipSpaces(ref this) {
        while this.pos < this.input.length() && this.input[this.pos] == ' ' {
            this.pos += 1
        }
    }

    Result<Token, SyntaxError> number(ref this, int start) from source {
        int value = 0
        while this.pos < this.input.length() && isDigit(this.input[this.pos]) {
            int digit = this.input[this.pos] - '0'
            value = value.tryMultiply(10) ?? return Error(SyntaxError.NumberTooLarge(start))
            value = value.tryAdd(digit) ?? return Error(SyntaxError.NumberTooLarge(start))
            this.pos += 1
        }
        return Ok(Token.Number(value))
    }

    Token name(ref this, int start) from source {
        while this.pos < this.input.length() && isLetter(this.input[this.pos]) {
            this.pos += 1
        }
        return Token.Name(this.input[start..this.pos])
    }
}
```

### Parser

```
public enum Operator : Copyable {
    Add
    Subtract
    Multiply
    Divide
}

public enum Expr<from source> {
    Number(int value)
    Variable(read string name)
    Negate(Expr* operand)
    Binary(Operator op, Expr* left, Expr* right)
}

public struct Parser<from source> {
    Lexer lexer
    Token current

    Result<Parser, SyntaxError> create(read string input) from input {
        var lexer = Lexer(input)
        var first = try lexer.next()         // first borrows the input, not the lexer
        return Ok(Parser { lexer = lexer, current = first })
    }

    Result<void, SyntaxError> advance(ref this) {
        this.current = try this.lexer.next()
        return Ok()
    }

    Result<Expr*, SyntaxError> expression(ref this) from source {
        var left = try this.term()
        while true {
            Operator op = match this.current {
                Plus => Operator.Add
                Minus => Operator.Subtract
                _ => return Ok(left)
            }
            try this.advance()
            var right = try this.term()
            left = new Expr.Binary(op, left, right)
        }
    }

    Result<Expr*, SyntaxError> term(ref this) from source {
        var left = try this.factor()
        while true {
            Operator op = match this.current {
                Star => Operator.Multiply
                Slash => Operator.Divide
                _ => return Ok(left)
            }
            try this.advance()
            var right = try this.factor()
            left = new Expr.Binary(op, left, right)
        }
    }

    Result<Expr*, SyntaxError> factor(ref this) from source {
        var token = this.current             // Token is Copyable, it is copied
        int position = this.lexer.pos
        try this.advance()

        return match token {
            Number(v) => Ok(new Expr.Number(v))
            Name(text) => Ok(new Expr.Variable(text))
            Minus => Ok(new Expr.Negate(try this.factor()))
            LeftParen => {
                var inner = try this.expression()
                if this.current is not RightParen {
                    return Error(SyntaxError.Unexpected(this.lexer.pos))
                }
                try this.advance()
                Ok(inner)
            }
            _ => Error(SyntaxError.Unexpected(position))
        }
    }
}

public Result<Expr*, SyntaxError> parse(read string input) from input {
    var parser = try Parser(input)
    var tree = try parser.expression()
    if parser.current is not End {
        return Error(SyntaxError.Unexpected(parser.lexer.pos))
    }
    return Ok(tree)
}
```

### Evaluator

```
public enum EvalError : Formattable {
    UnknownName(string name)             // owned: so the error can outlive the input
    DivideByZero
    Overflow
}

Result<int, EvalError> apply(Operator op, int a, int b) {
    return match op {
        Add => Ok(a.tryAdd(b) ?? return Error(EvalError.Overflow))
        Subtract => Ok(a.trySubtract(b) ?? return Error(EvalError.Overflow))
        Multiply => Ok(a.tryMultiply(b) ?? return Error(EvalError.Overflow))
        Divide when b == 0 => Error(EvalError.DivideByZero)
        Divide => Ok(a.tryDivide(b) ?? return Error(EvalError.Overflow))
    }
}

public Result<int, EvalError> eval(read Expr e, read HashMap<string, int> vars) {
    return match e {                     // e is read, so the branch variables are read too
        Number(v) => Ok(v)
        Variable(name) => Ok(vars.get(name) ?? return Error(EvalError.UnknownName(name.clone())))
        Negate(operand) => Ok(-(try eval(operand, vars)))
        Binary(op, left, right) => {
            var a = try eval(left, vars)
            var b = try eval(right, vars)
            apply(op, a, b)
        }
    }
}
```

### Program

```
public enum AppError : Formattable {
    Syntax(SyntaxError cause)
    Eval(EvalError cause)
    Io(IoError cause)
}

Result<int, AppError> run(read string source, read HashMap<string, int> vars) {
    var tree = try parse(source)         // SyntaxError becomes AppError.Syntax
    return Ok(try eval(read tree, vars)) // EvalError becomes AppError.Eval
}

Result<void, AppError> main() {
    var source = try File.readText("input.txt")
    var vars = HashMap<string, int> { {"x", 10} }

    print(try run(read source, read vars))
    return Ok()
}                                        // on error it is printed, exit code 1
```

## Stack: moving a field through `ref`

```
public struct Stack {
    Node*? top

    public void push(ref this, int v) {
        this.top = new Node { value = v, next = this.top.take() }   // through ref, a field is moved only with take
    }

    public int? pop(ref this) {
        var node = try this.top.take()   // returns null if empty, top is now null
        this.top = node.next
        return node.value
    }                                    // node is freed here
}
```

## Threads: counting the lines of two files

`join` gives the worker's return value. Because a panic stops the process, there is no equivalent of Rust's
`.join().unwrap()`.

```
Result<void, IoError> countLines(string path, shared locked int total) {
    var text = try File.readText(read path)
    int lines = 0
    for line in text.lines() {
        lines += 1
    }
    lock total as c {
        c += lines
    }
    return Ok()
}

Result<void, IoError> main() {
    var total = new shared locked 0
    var first = spawn countLines("a.txt", share total)
    var second = spawn countLines("b.txt", share total)
    try first.join()
    try second.join()
    lock total as c { print(c) }
    return Ok()
}
```

## `List`: `for`, index and `raw`

> **12 November 2026.** Generics of your own are deferred, so `List<T>` is known to the compiler and is not written in
> the language. The definition below is kept as a sketch of how it could be; the three forms of `for` and `xs[i]`
> at the end of the section are how the built-in list behaves.

Shows that the standard library can be written in the language itself. `unsafe` stays only on the lines that touch
memory; the user outside does not see it.

```
public interface Iterator<T> {
    T? next(ref this)
}

public struct List<T> : Disposable {
    raw T items
    nint count
    nint capacity

    public List<T> create() {
        return List<T> { items = Memory.allocate<T>(0), count = 0, capacity = 0 }
    }

    public nint length(read this) {
        return this.count
    }

    public void add(ref this, T value) {
        if this.count == this.capacity {
            this.grow()
        }
        unsafe { this.items.put(this.count, value) }
        this.count += 1
    }

    void grow(ref this) {
        nint larger = max(4, this.capacity * 2)
        var bigger = Memory.allocate<T>(larger)
        unsafe {
            Memory.moveItems(this.items, bigger, this.count)
            Memory.free(this.items)
        }
        this.items = bigger
        this.capacity = larger
    }

    public read T? get(read this, nint i) {           // null if out of bounds
        if i < 0 || i >= this.count {
            return null
        }
        unsafe { return read this.items[i] }
    }

    public read T at(read this, nint i) {             // reading xs[i] calls this
        return this.get(i) ?? panic("index out of bounds")
    }

    public ref T refAt(ref this, nint i) {            // xs[i] = v calls this
        if i < 0 || i >= this.count {
            panic("index out of bounds")
        }
        unsafe { return ref this.items[i] }
    }

    public ListItems<T> readItems(read this) from this {
        return ListItems { list = this, index = 0 }
    }

    public void dispose(ref this) {
        for i in 0..this.count {
            unsafe { _ = this.items.take(i) }
        }
        unsafe { Memory.free(this.items) }
    }
}

public struct ListItems<from list, T> : Iterator<read T from list> {
    read List<T> list
    nint index

    public read T? next(ref this) from list {
        var item = this.list.get(this.index)     // list is read, so the borrow is tied to the list, not to this
        this.index += 1
        return item
    }
}
```

The three forms of `for` map to three methods with the same naming scheme. If the value is already an `Iterator`, it is
used directly: `text.lines()`, `0..n`.

```
int sum(read List<int> xs) {
    int total = 0
    for x in xs {                        // xs is read, so readItems is called
        total += x
    }
    return total
}

void doubleAll(ref List<int> xs) {
    for x in xs {                        // xs is ref, so refItems is called, x is ref int
        x *= 2
    }
}

void main() {
    var names = List<string> { "ada", "grace" }  // literals become strings without allocation
    print(names[0])                      // names.at(0)
    for name in names {                  // takeItems is called, name is owned
        consume(name)
    }
}
```

## Log summary: reading and writing files, `dispose`

Reads the file `app.log` line by line and writes the line count and total duration for each level to `summary.txt`.
An input line: `ERROR 350 could not connect to the database`.

```
enum LogError : Formattable {
    Io(IoError cause)
    BadLine(long number, string text)    // owned: the error outlives the line
}

struct Stats : Copyable {
    int count
    long totalMs
}

struct Entry<from line> {
    read string level                    // part of the line, not a copy
    int ms
}

Entry? parseLine(read string line) from line {
    var parts = line.split(' ')
    var level = try parts.next()         // returns null if the line has ended
    var text = try parts.next()
    int ms = int.parse(text) ?? return null
    return Entry { level = level, ms = ms }
}

Result<HashMap<string, Stats>, LogError> summarize(read string path) {
    var reader = try File.openText(path) // StreamReader : Disposable
    var stats = HashMap<string, Stats>()
    string line = ""                     // capacity 0, grows on the first read
    long number = 0

    while try reader.readLine(ref line) {
        number += 1
        var entry = parseLine(read line) ?? return Error(LogError.BadLine(number, line.clone()))
        var item = stats.getOrAdd(entry.level, Stats { count = 0, totalMs = 0 })
        item.count += 1
        item.totalMs += entry.ms
    }
    return Ok(stats)
}                                        // reader is closed on every exit, including an exit through try

Result<void, IoError> writeReport(read HashMap<string, Stats> stats, read string path) {
    var writer = try File.createText(path)   // StreamWriter : Closable
    for item in stats {
        try writer.writeLine($"{item.key}: {item.value.count} lines, {item.value.totalMs} ms")
    }
    try writer.close()                   // the buffer is written to disk here, the error shows up here
    return Ok()
}

Result<void, LogError> main() {
    var stats = try summarize("app.log")
    try writeReport(read stats, "summary.txt")
    return Ok()
}
```

Because a single `string` buffer is reused, there is no heap allocation per line. Since `readLine` changes the buffer,
`entry` cannot outlive one loop iteration; the compiler checks this. There is also a simple way, which allocates a new
`string` for every line:

```
for item in reader.lines() {             // item: Result<string, IoError>
    var line = try item
    ...
}
```

`Iterator.next` cannot lend from its own buffer, because `next` would change that buffer on the following call. So
`lines()` returns an owned `string`, and `readLine` is the way that reuses the buffer. After the first read error
`lines()` ends; it does not return the same error forever.

Compile errors of this example:

```
var last = ""
while try reader.readLine(ref line) {
    var entry = try parseLine(read line)
    last = entry.level
}                                        // error: readLine changes line, last borrows from it
print(last)

Result<void, IoError> save(read string path) {
    var writer = try File.createText(path)
    try writer.writeLine("done")
    return Ok()                          // error: writer was not closed, a write error would be lost. Hint: try writer.close()
}

try writer.close()
try writer.writeLine("late")             // error: writer was moved by close

var r = int.parse(text)                  // error: r is never used, a Result cannot be dropped silently
```

| Error | Hint |
|---|---|
| A `Closable` local variable ends without being closed | `try writer.close()`, or `_ = writer.close()` to discard it on purpose |
| A borrow outlives the `readLine` iteration | `string last`, `last = entry.level.clone()` |
| `$"..."` cannot be used as a `string` | `$"...".toString()` |

## Folder tree: `shared`, `checked`, `weak`, `where`

Sums folder sizes in a tree and prints the three largest folders with their paths. A folder owns its subfolders and only
looks at its parent. In C#, holding a reference to the parent is free: the GC collects the cycle. Here, if the child also
owned its parent, the two objects would keep each other alive, the count would never reach zero, and the memory would
never be returned. That is why the `parent` field is `weak`.

```
public using FolderRef = shared checked Folder   // the same type, not a new type

public struct Folder {
    readonly string name
    long bytes                               // itself and everything below it
    weak checked Folder? parent              // null at the root, not an owner
    List<FolderRef> children                 // owner: keeps the subfolders alive

    public Folder create(string name, weak checked Folder? parent) {
        return Folder {
            name = name,
            bytes = 0,
            parent = parent,
            children = List<FolderRef>()
        }
    }

    // Returns the subfolder, creating it if missing. The receiver is FolderRef because the child must be given weak this
    public FolderRef child(read FolderRef this, read string name) {
        for item in read this.children {     // the read borrow lasts for the whole loop
            if item.name == name {
                return share item
            }
        }
        var created = new FolderRef(name.clone(), weak this)
        this.children.add(share created)     // write borrow for the duration of the expression
        return created
    }

    // Adds to itself and to all of its parents
    public void addBytes(read FolderRef this, long amount) {
        FolderRef? cur = share this
        while cur is not null {
            cur.bytes += amount
            cur = share cur.parent           // null at the root
        }
    }

    public string path(read FolderRef this) {
        var names = List<string>()
        FolderRef? cur = share this
        while cur is not null {
            names.add(cur.name.clone())
            cur = share cur.parent
        }
        names.reverse()
        return string.join("/", read names)
    }
}

void addFile(read FolderRef root, read string folder, long bytes) {
    var cur = share root
    for name in folder.split('/') {
        cur = cur.child(name)
    }
    cur.addBytes(bytes)
}
```

The list that keeps the largest ones (written for `Ranked`: generics of your own are deferred):

```
public struct TopList {
    List<Ranked> items
    nint limit

    public TopList create(nint limit) {
        return TopList { items = List<Ranked>(), limit = limit }
    }

    public void add(ref this, Ranked value) {
        this.items.add(value)
        this.items.sort()
        if this.items.length() > this.limit {
            _ = this.items.removeLast()
        }
    }
}

struct Ranked : Comparable, Formattable {
    long bytes
    string path

    public int compare(read this, read Ranked other) {
        return other.bytes.compare(this.bytes)   // larger first
    }

    public void format(read this, ref Formatter f) {
        f.write($"{this.bytes}  {this.path}")
    }
}

void collect(read FolderRef folder, ref TopList top) {
    top.add(Ranked { bytes = folder.bytes, path = folder.path() })
    for child in read folder.children {  // folder is being read for the whole loop
        collect(child, ref top)          // path walks up but only reads, no conflict
    }
}

void report(read TopList top) {
    for item in read top.items {
        print($"{item}")
    }
}

void main() {
    var root = new FolderRef("root", null)
    addFile(read root, "docs/2026", 1200)
    addFile(read root, "docs/2025", 800)
    addFile(read root, "photos", 5000)

    var top = TopList(3)
    collect(read root, ref top)
    report(read top)
}                                        // root drops to zero, the tree is freed, weak does not get in the way
```

When a borrow is needed across several lines, a block is written in the same form as `lock`:

```
read folder as f {                       // f: read Folder, being read for the whole block
    print($"{f.name}: {f.bytes}")
    for child in read f.children {
        ...
    }
}

ref folder as f {                        // f: ref Folder
    f.bytes = 0
    f.children.clear()
}
```

Errors seen at compile time:

```
var kids = read folder.children          // error: a checked borrow ends with the expression, kids cannot be used after it

for child in read folder.children {
    folder.children.add(item)            // error: the same variable is being read for the whole loop, this line would panic every time it ran
}

shared Folder f = ...
f.bytes = 0                              // error: shared is read-only

spawn collect(share root, ...)           // error: checked cannot cross to a thread

var plain = Folder("x", null)
plain.path()                             // error: path needs a FolderRef receiver
```

| Error | Hint |
|---|---|
| A `checked` borrow outlives the expression | `read folder as f { ... }` |
| The same variable is being read in a loop and is written to | Loop over a copy: `for child in folder.children.clone()` |
| Write through `shared` | The field type is `shared checked Folder` |
| `checked` passed to a thread | `shared locked Folder` |
| Method that needs a `FolderRef` receiver | `new FolderRef(...)` |

A conflict the compiler cannot see panics at run time, because the second access comes through a different variable:

```
void touchAll(read FolderRef folder) {
    for child in read folder.children {  // folder is being read
        child.addBytes(1)                // panic: addBytes walks up and writes to folder
    }
}
```

```
panic: Folder could not be opened for writing, it is being read at line 12
hint: loop over a copy, for child in folder.children.clone()
```

## Order rules: function values, `enum`

Filters and sums a list of orders, and checks it with rules added at run time. Until 12 November 2026 this example was
written with lambdas that captured their data; lambdas were removed, and it is written with what remains: a `for`
loop, a named function as a value, and an `enum` for a rule that carries data.

```
struct Order {
    string customer
    long amount
    bool paid
}

// A rule that needs no data of its own is a named function.
bool isPaid(read Order o) {
    return o.paid
}

// A rule that carries data is a variant: the data is in the value, where the owner is clear.
enum OrderRule {
    Check(Func<read Order, bool> test)
    MinAmount(long limit)
    CustomerIs(string name)
}

bool passes(read OrderRule rule, read Order order) {
    return match rule {
        Check(test) => test(order)                   // a function value is called like a function
        MinAmount(limit) => order.amount >= limit
        CustomerIs(name) => order.customer == name
    }
}

struct Validator {
    List<OrderRule> rules

    public Validator create() {
        return Validator { rules = List<OrderRule>() }
    }

    public void add(ref this, OrderRule rule) {
        this.rules.add(rule)
    }

    public bool accepts(read this, read Order order) {
        for rule in read this.rules {
            if !passes(rule, order) {
                return false
            }
        }
        return true
    }
}

void main() {
    var orders = List<Order> {
        Order { customer = "ada", amount = 1200, paid = true },
        Order { customer = "grace", amount = 300, paid = false },
        Order { customer = "ada", amount = 80, paid = true }
    }

    var rules = Validator()
    rules.add(OrderRule.Check(isPaid))               // the name of the function is the value
    rules.add(OrderRule.MinAmount(100))
    rules.add(OrderRule.CustomerIs("ada"))

    long total = 0
    var accepted = List<read Order>()                // borrows the orders, copies nothing
    for o in read orders {
        if rules.accepts(o) {
            accepted.add(o)
            total += o.amount
        }
    }
    print($"{accepted.length()} orders, {total} in total")
}
```

| With a lambda, before | Now | What it costs |
|---|---|---|
| `orders.filter(o => o.amount >= limit)` | A `for` with an `if` | Three lines for one; the machine code is the same |
| `orders.forEach(o => { total += o.amount })` | `for o in read orders { total += o.amount }` | Nothing; the run-time check on `total` is gone |
| `new o => o.amount >= limit` kept in a list | `OrderRule.MinAmount(limit)` | A variant and an arm; no heap, and the owner of `limit` is the value |
| `new o => { seen += 1 ... }`, a rule with a state of its own | The state is a field of the struct that owns the rules | The state is visible where it lives |
| `orders.sortBy(o => read o.customer)` | `: Comparable` on the type and `orders.sort()` | One order per type; another order needs a wrapper type |

### Decisions this example led to

On 12 November 2026: the function value as the address of a named function, and the removal of lambda notation,
capture and the list methods that took a lambda. The reasons are in the decisions table ("Function value") and under
"Dropped decisions".

## Cities and roads: graph, cycle, `weak` edge

Finds the route between cities that uses the fewest roads. Roads go both ways: Ankara points to Konya, and Konya points
to Ankara. In the folder tree the cycle was in one place (the child looks at its parent); here every road is a cycle.
The rule stays the same: **there is one owner, the rest only look.** The map owns the cities, and the roads are `weak`.

```
public using CityRef = shared checked City

public struct City {
    readonly int id
    readonly string name
    List<weak checked City> roads            // looks at the neighbours, not an owner

    public City create(int id, string name) {
        return City { id = id, name = name, roads = List<weak checked City>() }
    }
}

public struct RoadMap {
    List<CityRef> cities                     // the map owns the cities
    int nextId

    public RoadMap create() {
        return RoadMap { cities = List<CityRef>(), nextId = 0 }
    }

    public CityRef addCity(ref this, string name) {
        var city = new CityRef(this.nextId, name)
        this.nextId += 1
        this.cities.add(share city)
        return city
    }

    // Also takes the ownership held by the caller: city is moved, the caller can no longer use it
    public void remove(ref this, CityRef city) {
        this.cities.removeAll(c => same(c, read city))
    }                                        // city is dropped here, the city dies if there is no other owner
}

void connect(read CityRef a, read CityRef b) {
    a.roads.add(weak b)
    b.roads.add(weak a)
}
```

The route is found with a breadth-first search. Visited cities are tracked by the city's `id`:

```
// The route with the fewest roads, null if there is no route
List<string>? route(read CityRef start, read CityRef goal) {
    var seen = HashSet<int> { start.id }
    var cameFrom = HashMap<int, CityRef>()       // city -> the city it was reached from
    var waiting = List<CityRef> { share start }
    nint next = 0

    while next < waiting.length() {
        var cur = share waiting[next]
        next += 1
        if same(read cur, goal) {
            return pathTo(read cameFrom, goal)
        }
        for road in read cur.roads {         // cur is being read for the whole loop
            var city = share road            // CityRef?, null if the city was removed
            if city is null { continue }
            if seen.contains(city.id) { continue }
            seen.add(city.id)
            cameFrom.add(city.id, share cur) // share touches the count, not the contents: no conflict
            waiting.add(city)
        }
    }
    return null
}

List<string> pathTo(read HashMap<int, CityRef> cameFrom, read CityRef goal) {
    var names = List<string>()
    CityRef? cur = share goal
    while cur is not null {
        names.add(cur.name.clone())
        cur = share cameFrom.get(cur.id)     // null at the start city
    }
    names.reverse()
    return names
}

void show(read CityRef start, read CityRef goal) {
    var path = route(start, goal)
    if path is null {
        print("no route")
    } else {
        print(string.join(" > ", read path))
    }
}

void main() {
    var map = RoadMap()
    var izmir = map.addCity("Izmir")
    var ankara = map.addCity("Ankara")
    var konya = map.addCity("Konya")
    var adana = map.addCity("Adana")

    connect(read izmir, read konya)
    connect(read konya, read ankara)
    connect(read konya, read adana)

    show(read izmir, read ankara)            // Izmir > Konya > Ankara

    map.remove(konya)                        // konya was moved, the city is dead
    show(read izmir, read ankara)            // no route: the roads that look at Konya give null
}                                            // the map is dropped, the cities are freed
```

Roads that look at the removed city stay in the neighbours' lists; `share road` gives `null` for them. No dangling
address is followed, and no wrong city is reached either.

### Wrong version: if the roads were owners

The only difference is the type of the `roads` field:

```
List<CityRef> roads                          // owner: keeps the neighbour alive

void connect(read CityRef a, read CityRef b) {
    a.roads.add(share b)
    b.roads.add(share a)                     // a holds b, b holds a
}
```

The code compiles and runs correctly. When `main` ends the map is dropped, but Ankara and Konya keep each other alive:
the count never reaches zero, the memory is not returned, `dispose` does not run. After `map.remove(konya)` Konya is
still alive, and the route keeps going through it. Memory safety is not broken, but it is a silent mistake.

It cannot be made a compile error: the folder tree, which holds `List<FolderRef> children`, looks the same at the type
level and is correct. The compiler cannot tell from the type whether there will be a cycle. So in test and debug builds,
the `shared` objects that are still alive when the program ends are reported.

```
leak: program ended, 4 shared objects still alive
  4 x City, created at line 28
  field holding each other: City.roads
hint: the non-owning direction should be weak, List<weak checked City>
```

Errors seen at compile time:

```
if cur == goal { }                           // error: City is not Equatable. Hint: same(read cur, goal)

map.remove(konya)
show(read izmir, read konya)                 // error: konya was moved at line 61

var seen = HashSet<CityRef>()                    // error: CityRef is not Hashable. Hint: HashSet<int>, city.id
```

### Decisions this example led to

Approved on 11 November, in the [decision table](04-decisions-and-open-questions.md): the distinction between
`same(a, b)` and `==`, the leak report, `share` passing a null value through, the names `HashMap` and `HashSet`.

## Stock and invoice: module, `use`, `public`

The earlier examples were single files. This program is split into three folders: the warehouse, billing, and the
program itself. The question is: who sees a name written in a file?

```
shop/
    main                     program entry
    stock/
        product              Product, StockError, indexOf
        warehouse            Warehouse
    billing/
        invoice              Line, Invoice
```

Source files end in `.sg`; the trees and file names in this document leave the extension out. The rule is two sentences:
**a folder is a module, and a name not marked `public` stays inside the module.**

The `stock/product` file:

```
public struct Product {
    public readonly string code
    public readonly string name
    public long price                        // in cents; read and written from outside
    int quantity                             // no public: only the stock module sees it

    public Product create(string code, string name, long price) {
        return Product { code = code, name = name, price = price, quantity = 0 }
    }

    public int available(read this) {
        return this.quantity
    }
}

public enum StockError : Formattable, Equatable {    // the variants are visible together with the enum
    UnknownProduct(string code)
    NotEnough(string code, int wanted, int available)
}

// no public: every file inside stock can call it, the outside cannot see it
nint? indexOf(read List<Product> products, read string code) {
    nint i = 0
    while i < products.length() {
        if products[i].code == code {
            return i
        }
        i += 1
    }
    return null
}
```

The `stock/warehouse` file is in the same folder, so it is in the same module. No `use` is written for `Product` and
`indexOf`, and the `quantity` field is visible too:

```
public struct Warehouse {
    List<Product> products

    public Warehouse create() {
        return Warehouse { products = List<Product>() }
    }

    public void add(ref this, Product product) {
        this.products.add(product)
    }

    public Result<void, StockError> receive(ref this, read string code, int count) {
        var i = indexOf(read this.products, code) ?? return Error(StockError.UnknownProduct(code.clone()))
        this.products[i].quantity += count   // same module: quantity is visible
        return Ok()
    }

    public Result<void, StockError> ship(ref this, read string code, int count) {
        var i = indexOf(read this.products, code) ?? return Error(StockError.UnknownProduct(code.clone()))
        var have = this.products[i].quantity
        if have < count {
            return Error(StockError.NotEnough(code.clone(), count, have))
        }
        this.products[i].quantity -= count
        return Ok()
    }

    public read Product? get(read this, read string code) from this {
        var i = indexOf(read this.products, code) ?? return null
        return read this.products[i]
    }
}
```

The `billing/invoice` file is in another module. `use stock` brings in the `public` names of `stock`:

```
use stock

public struct Line {
    public readonly string name
    public readonly int count
    public readonly long total
}

public struct Invoice {
    public readonly string customer
    List<Line> lines

    public Invoice create(string customer) {
        return Invoice { customer = customer, lines = List<Line>() }
    }

    public Result<void, StockError> add(ref this, ref Warehouse warehouse, read string code, int count) {
        var product = warehouse.get(code) ?? return Error(StockError.UnknownProduct(code.clone()))
        var name = product.name.clone()
        var total = product.price * count    // last use of product, the read borrow ends here
        try warehouse.ship(code, count)      // if stock is short, no invoice line is added
        this.lines.add(Line { name = name, count = count, total = total })
        return Ok()
    }

    public long total(read this) {
        long sum = 0
        for line in read this.lines {
            sum += line.total
        }
        return sum
    }
}
```

The `main` file:

```
use stock
use billing

Result<void, StockError> main() {
    var warehouse = Warehouse()
    warehouse.add(Product("K-1", "pen", 1500))
    warehouse.add(Product("D-1", "notebook", 4000))
    try warehouse.receive("K-1", 100)
    try warehouse.receive("D-1", 20)

    var invoice = Invoice("ada")
    try invoice.add(ref warehouse, "K-1", 10)
    try invoice.add(ref warehouse, "D-1", 2)
    print($"{invoice.customer}: {invoice.total()} cents")    // ada: 23000 cents

    try invoice.add(ref warehouse, "D-1", 50)                // NotEnough("D-1", 50, 18), exit code 1
    return Ok()
}
```

The only way to change a stock count is through `receive` and `ship`, because `quantity` never leaves the module. No
extra rule ensures this; not writing `public` is enough.

Errors seen at compile time, from inside `billing` and `main`:

```
product.quantity = 5                         // error: quantity is not public, it stays inside the stock module

var p = Product { code = "X", name = "x", price = 1, quantity = 9 }
                                             // error: quantity is not visible, so Product cannot be built field by field here.
                                             // Hint: Product("X", "x", 1)

var i = indexOf(read items, "K-1")           // error: indexOf exists in the stock module but is not public

product.name = "eraser"                      // error: name is readonly

var w = Warehouse()                          // error if use stock is not written: Warehouse is unknown.
                                             // Hint: use stock at the top of the file
```

If two modules have the same name (suppose `stock` also had a `Line`):

```
use stock
use billing

var line = Line { ... }                      // error: Line exists in both stock and billing. Hint: billing.Line
var line = billing.Line { ... }              // the full name is always valid, even without use
```

From inside `stock`, a name that leaks out:

```
struct Shelf { ... }                         // not public

public Shelf shelfOf(read this, read string code) { ... }
                                             // error: a public function exposes the non-public type Shelf.
                                             // The error shows both lines: either Shelf becomes public or shelfOf stops being public
```

### Decisions this example led to

Approved on 11 November, in the [decision table](04-decisions-and-open-questions.md):

| Decision | Rust | C# |
|---|---|---|
| A module is a folder. There is no module declaration in the file, the name comes from the folder path: `use shop.stock`. Files in the same folder see each other without writing `use` | A file is a module, and `mod stock;` is also written in the parent file | `namespace` is written in the file and has nothing to do with the folder |
| A name not marked `public` is visible inside the module. One rule for types, functions, fields and methods, two levels | The same: without `pub` it is inside the module. There is a third level, `pub(crate)` | A field is private to the type, a type is open to the project (`internal`); there are five levels |
| `use stock` brings in all the `public` names directly. A clash is a compile error at the place of use, the full name is `stock.Line` | `use stock::Warehouse` lists the names one by one, `*` brings in all of them | `using Stock;` brings in all of them, the same clash rule |
| The keyword is `use` | The same keyword | `using` |

Two consequences follow from these decisions and are not separate rules: something `public` cannot expose a type that is
not `public` (a compile error in C#, a warning in Rust), and a struct with a field that is not visible cannot be built
field by field from outside (the same in Rust).

Three more small decisions were approved on the same day:

| Decision | Rust | C# |
|---|---|---|
| Modules can `use` each other mutually | Allowed | Allowed (Go forbids it) |
| The standard names used in the examples so far (`List`, `HashMap`, `File`, `print`) do not need `use` | `Vec` is available by default, `HashMap` and `File` need `use` | Available by default in a new project |
| An unused `use` is not an error, the editor shows it dimmed | Warning | Warning (an error in Go) |

Still open: the library boundary. In a library with more than one folder there is no level that means "my own folders
see it, the user does not" (`internal` in C#, `pub(crate)` in Rust). It will be handled when the package system is
designed.

## HTTP request: byte slice, zero-copy parsing

Turns bytes arriving from the network into an HTTP request. The request copies nothing: the method, the path, the
headers and the body are parts of the incoming data. Because the data arrives piece by piece, "not finished yet" is a
result too.

```
POST /orders HTTP/1.1\r\n
Host: example.com\r\n
Content-Length: 5\r\n
\r\n
hello
```

A byte slice is written `read byte[]`: a window onto someone else's bytes, a pointer plus a length. What `read string` is
for text, `read byte[]` is for raw data.

```
public enum HttpError : Formattable {
    Incomplete                               // the data has not ended, reading must continue
    BadRequestLine
    BadHeader(int line)
    BadLength
    NotText(TextError cause)                 // the header section is not valid UTF-8
}

public struct Header<from data> {
    public readonly read string name
    public readonly read string value
}

public struct Request<from data> {
    public readonly read string method
    public readonly read string path
    public readonly List<Header> headers
    public readonly read byte[] body

    // The returned value is part of the data, not of the request: it stays valid even if the request is dropped
    public read string? header(read this, read string name) from data {
        for h in read this.headers {
            if h.name.equalsIgnoreCase(name) {
                return h.value
            }
        }
        return null
    }
}

public Result<Request, HttpError> parse(read byte[] data) from data {
    var end = data.indexOf("\r\n\r\n".asBytes()) ?? return Error(HttpError.Incomplete)
    var head = try string.fromUtf8(data[..end]) // no copy, only a check; TextError becomes HttpError.NotText

    var lines = head.split("\r\n")
    var first = lines.next() ?? return Error(HttpError.BadRequestLine)
    var parts = first.split(' ')
    var method = parts.next() ?? return Error(HttpError.BadRequestLine)
    var path = parts.next() ?? return Error(HttpError.BadRequestLine)
    var version = parts.next() ?? return Error(HttpError.BadRequestLine)
    if version != "HTTP/1.1" {
        return Error(HttpError.BadRequestLine)
    }

    var headers = List<Header>()
    nint length = 0
    int number = 1
    for line in lines {
        number += 1
        var colon = line.indexOf(':') ?? return Error(HttpError.BadHeader(number))
        var name = line[..colon].trim()
        var value = line[colon + 1..].trim()
        if name.equalsIgnoreCase("content-length") {
            length = nint.parse(value) ?? return Error(HttpError.BadLength)
        }
        headers.add(Header { name = name, value = value })
    }
    if length < 0 {
        return Error(HttpError.BadLength)
    }

    nint bodyStart = end + 4
    if data.length() - bodyStart < length {  // written as bodyStart + length, a huge number would overflow and panic
        return Error(HttpError.Incomplete)
    }
    return Ok(Request {
        method = method,
        path = path,
        headers = headers,
        body = data[bodyStart..bodyStart + length]
    })
}
```

The data arrives in two pieces. The buffer is a `List<byte>`; where a slice is expected, `read buffer` is written, the same as passing a `string` where `read string` is expected:

```
Result<void, HttpError> main() {
    var buffer = List<byte>()
    buffer.addAll("POST /orders HTTP/1.1\r\nHost: exa".asBytes())

    if parse(read buffer) is Error(e) {
        print(e)                             // Incomplete
    }

    buffer.addAll("mple.com\r\nContent-Length: 5\r\n\r\nhello".asBytes())

    var request = try parse(read buffer)
    print($"{request.method} {request.path}")        // POST /orders
    print(request.header("host") ?? "none")          // example.com
    print(request.body.length())                     // 5
    return Ok()
}
```

The result of the first `parse` call ends together with the `if`, so the buffer can be appended to afterwards. While the
request is alive the buffer cannot be changed; the compiler checks this:

```
var request = try parse(read buffer)
buffer.clear()                               // error: buffer is borrowed, request reads from it
print(request.path)

Request load() {
    var buffer = readAll()
    return parse(read buffer) ?? panic("bad request")   // error: the request borrows from buffer, buffer dies here
}

parse(buffer)                                // error: List<byte> cannot be used as read byte[]. Hint: read buffer

parse("GET / HTTP/1.1\r\n\r\n")              // error: read string cannot be used as read byte[]. Hint: .asBytes()

void wipe(read byte[] data) {
    data[0] = 0                              // error: read byte[] cannot be written to. Hint: the parameter should be ref byte[]
}

byte[] copy = data                           // error: byte[] is not a type on its own.
                                             // Hint: List<byte> for an owner, read byte[] for a window
```

Before the buffer is emptied, anything that must be kept is copied: `var path = request.path.clone()`.

### Decisions this example led to

Approved on 11 November, in the [decision table](04-decisions-and-open-questions.md):

| Decision | Rust | C# |
|---|---|---|
| The slice type is `read T[]` (readable) and `ref T[]` (writable). A bare `T[]` is not a type, the owner is `List<T>` | `&[T]`, `&mut [T]`, the owner is `Vec<T>` | `ReadOnlySpan<T>`, `Span<T>`; `T[]` is an owning array on the heap |

| The ends of a range may be left out: `data[..end]`, `line[colon + 1..]`, `data[..]` | The same | The same |
| From text to bytes `text.asBytes()`, from bytes to text `string.fromUtf8(bytes)`: it checks, does not copy, and returns `Result<read string, TextError>` | `as_bytes()`, `str::from_utf8` | No zero-copy way |

Two simplifications on the same day, for the small-language goal: the copying `string.fromBytes` was removed (whoever
wants a copy writes `.clone()` on the `fromUtf8` result), and `List<T>` does not turn into a slice by itself, `buffer[..]`
is written. In both cases a single way of doing the job remained. The second one changed the same day: where a slice is
expected, `read buffer` is written, the same rule as for `string` (decisions of the
[sensor window](#sensor-window-fixed-size-array-heap-free-program) example).

Still open: there is no fixed-size array yet (such as `byte[16]`, with the length written in the type). The next example
tries it.

## Sensor window: fixed-size array, heap-free program

Keeps the average of the last 8 measurements and prints the device id as hex. There is not a single heap allocation in
the whole program: it is the counterpart of code written in C with `int values[8]`.

A fixed-size array is written `T[N]`: its length is part of the type, and the value sits where it is declared (on the
stack or inside the struct). The slice `read T[]` is a window onto someone else's data, `T[N]` is the data itself.

```
struct Window {
    int[8] values                            // inside the struct: Window is 48 bytes, one piece
    nint next
    nint count

    public Window create() {
        return Window { values = int[8], next = 0, count = 0 }       // all 8 elements are 0
    }

    public void add(ref this, int value) {
        this.values[this.next] = value       // bounds check, the same as List
        this.next = (this.next + 1) % this.values.length()
        if this.count < this.values.length() {
            this.count += 1
        }
    }

    public int? average(read this) {
        if this.count == 0 {
            return null
        }
        long total = 0
        for v in this.values[..this.count] { // slice of the filled part, no copy
            total += v
        }
        return (int)(total / this.count)
    }
}

// Turns each byte into two hex characters. The target is on the caller's stack, the function does not allocate
void toHex(read byte[] source, ref byte[] target) {
    var digits = "0123456789abcdef".asBytes()
    for i in 0..source.length() {
        target[i * 2] = digits[source[i] >> 4]
        target[i * 2 + 1] = digits[source[i] & 15]
    }
}

void main() {
    var readings = int[5] { 20, 22, 21, 90, 23 }     // writing the elements one by one
    var window = Window()
    for r in readings {
        window.add(r)
    }
    print(window.average() ?? 0)                     // 35

    var id = byte[4] { 0xde, 0xad, 0xbe, 0xef }
    var text = byte[8]                               // 8 bytes, on the stack, all 0
    toHex(read id, ref text)                         // an array is lent where a slice is expected, like List
    print(string.fromUtf8(read text) ?? panic("hex is ASCII"))       // deadbeef
}
```

What the compiler catches:

```
var buf = byte[16]                           // all 0; for another value, byte[16](255)
buf[16] = 1                                  // error: index 16, length 16. Both are known at compile time
buf[i] = 1                                   // i depends on data: panic if out of bounds, like List

var xs = int[3] { 1, 2 }                     // error: int[3] needs 3 elements, 2 were written
byte[4] small = buf                          // error: byte[16] cannot be used as byte[4]
toHex(id, ref text)                          // error: byte[4] cannot be used as read byte[]. Hint: read id
var names = string[2]("ada")                 // error: string is not Copyable, it cannot be repeated.
                                             // Hint: string[2] { "ada", "grace" }
var empty = string[2]                        // error: string has no zero value. Same hint

read byte[] make() {
    var buf = byte[16]
    return read buf                          // error: the slice borrows from buf, buf dies here
}
```

The last error compiles in C and returns a pointer to dead memory on the stack; here the borrow rule already catches it,
and no new rule was needed.

### Decisions this example led to

Approved on 11 November, in the [decision table](04-decisions-and-open-questions.md). It was weighed against the
small-language goal: without fixed arrays every buffer goes to the heap through `List`, and the goal of C-level
performance is not met; the feature adds no new keyword.

| Decision | Rust | C# | Cost |
|---|---|---|---|
| The type is `T[N]`, `N` an integer constant. It is a value, no heap. `length()` is known at compile time. `xs[i]` is bounds-checked, a known out-of-bounds index is a compile error | `[T; N]`, the same behaviour | No natural counterpart: `stackalloc` only for locals, `fixed` inside a struct only in `unsafe` | `T[]` (a slice, needs `read`/`ref`) and `T[N]` (a value) look alike |
| Construction: `int[3] { 1, 2, 3 }` writes the elements one by one, `byte[16](255)` puts the same value in every element (`T` must be `Copyable`), `byte[16]` on its own fills with zeros. There is no uninitialised memory | `[1, 2, 3]`, `[255; 16]`; no short form for zero | `new int[] { 1, 2, 3 }`, `new byte[16]` is always zero | The cost of zeroing a large buffer. A zero value is defined only for numbers, `bool` (`false`) and `T?` (`null`) |
| If `T` is `Copyable`, `T[N]` is `Copyable` too: assignment and parameter passing copy all the elements. Otherwise it is moved | The same | An array is a reference, it is not copied | If `byte[4096]` is passed by value, 4 KB is copied silently; the right thing is to pass a slice |
| Conversion to a slice: `read xs` and `ref xs` give a slice where a slice is expected. For `List<T>` and `T[N]`, the same rule as `string` → `read string`. `xs[..]` stays valid as range notation | `&buf` becomes a slice where a slice is expected, the same | Converts to `Span` without writing `read` | One conversion rule, an extension of the one for `string` |

The slice conversion rule replaced the `buffer[..]` decision of the HTTP example: a separate notation had to be memorised
for `List`, while `string` did the same job with `read`. That example was updated too.

Left out on purpose, because code can be written without them: making the length a generic parameter (`Ring<T, N>`;
"constant parameters" are already among the deferred items, which is why `Window` is fixed at 8), a named constant
(`const`; the length is read with `values.length()`), notation for two-dimensional arrays (not tried). A very large local
array overflows the stack; that topic belongs to the stack overflow item in the open questions.

## Stock tests: `test`, `assert`

The tests of the [Stock and invoice](#stock-and-invoice-module-use-public) example. The question is: where is a test
written, what does it see, and what happens when it fails? The new files sit in the folder of the module whose tests
they hold:

```
shop/
    stock/
        product
        warehouse
        tests                new
    billing/
        invoice
        tests                new
```

The `stock/tests` file. Because a folder is a module, this file is inside the `stock` module: it writes no `use`, and it
sees the non-`public` function `indexOf` and the `quantity` field.

```
// An ordinary function: there is no separate feature for shared setup
Result<Warehouse, StockError> penStock(int count) {
    var warehouse = Warehouse()
    warehouse.add(Product("K-1", "pen", 1500))
    try warehouse.receive("K-1", count)
    return Ok(warehouse)
}

test "indexOf returns null for an unknown code" {
    var products = List<Product> { Product("K-1", "pen", 1500) }
    assert(indexOf(read products, "K-1") is not null)
    assert(indexOf(read products, "X-9") is null)
}

test "received goods are added to stock" {
    var warehouse = try penStock(100)                // if an error is returned the test fails, the error is printed
    try warehouse.receive("K-1", 20)
    assert(warehouse.products[0].quantity == 120)    // same module: the non-public field is visible
}

test "nothing ships when stock is short" {
    var warehouse = try penStock(20)

    match warehouse.ship("K-1", 30) {
        Error(e) => assert(e == StockError.NotEnough("K-1", 30, 20))
        _ => panic("the shipment should have been rejected")
    }
    assert(warehouse.products[0].quantity == 20)     // stock did not change
}
```

To be able to write `e == ...`, `StockError` is now `Equatable`: `public enum StockError : Formattable, Equatable`. The
compiler generates the body. This was the only change the test required in the code under test.

The `billing/tests` file is in another module. It sees `stock` as a real user would, only through its `public` names:

```
use stock

test "invoice total and stock change together" {
    var warehouse = Warehouse()
    warehouse.add(Product("K-1", "pen", 1500))
    try warehouse.receive("K-1", 100)

    var invoice = Invoice("ada")
    try invoice.add(ref warehouse, "K-1", 10)

    assert(invoice.total() == 15000)
    assert(invoice.lines.length() == 1)              // lines is not public, but this is billing
    var pen = warehouse.get("K-1") ?? panic("K-1 was added")
    assert(pen.available() == 90)                    // quantity is not visible from here, the public way is used
}
```

The test command (the tool does not have a name yet) runs every `test` block. Suppose the `-=` inside `ship` was written
as `+=` by mistake:

```
stock     2 passed, 0 failed
billing   0 passed, 1 failed

failed: "invoice total and stock change together"   billing/tests:14
    assert(pen.available() == 90)
    left: 110
    right: 90
```

If an `assert` is false it panics; the message prints the expression itself and both sides of the comparison. An error
that exits through `try` also fails the test and is printed:

```
failed: "received goods are added to stock"   stock/tests:17
    try warehouse.receive("K-1", 20)
    error: UnknownProduct("K-1")
```

Because a panic stops the process, the test command runs each test in a separate process: when one fails, the others
keep running. The `shared` leak report decided earlier is also produced per test.

What the compiler catches:

```
warehouse.receive("K-1", 5)                  // error: Result was not used. Hint: try warehouse.receive("K-1", 5)
                                             // The same rule inside a test: a setup error does not pass silently

assert(warehouse.ship("K-1", 5))             // error: assert needs a bool, a Result was given

assert(warehouse.products[0].quantity == 90) // error inside billing/tests: products is not public

void check() {
    test "nested" { }                        // error: test is written only at the top level of a file
}
```

In a normal build `test` blocks are not compiled at all and do not enter the program.

### Decisions this example led to

Approved on 12 November, in the [decision table](04-decisions-and-open-questions.md). It was weighed against the
small-language goal: can code be written without tests? A separate `main` program can be written, but that program cannot
see names that are not `public`, stops at the first error and does not say which check failed. So the smallest form was
chosen: one keyword, one function. The block notation is from Zig.

| Decision | Rust | C# | Cost |
|---|---|---|---|
| `test "name" { }`: a block written at the top level of a file. Only the test command compiles and runs it | `#[test] fn name()`; its usual place is `#[cfg(test)] mod tests { use super::*; }` | A separate project, `[Fact] public void Name()`, a library (xUnit) | One keyword. It is a keyword only at the top level and when followed by a string, so parameters and variables named `test` (the examples have them) stay valid |
| A test belongs to the module of the folder it is written in: it sees every name of the module, and other modules through `use` and only by their `public` names | The same result, with `use super::*` | A test project sees only `public`, `InternalsVisibleTo` for the inside | No new rule, it follows from the module rule |
| `assert(condition)`: panics if false. The message prints the text of the expression and, for a comparison, the values of both sides (if they are `Formattable`). Also valid outside tests, and enabled in release too | `assert!`, `assert_eq!` (a separate macro that prints the values), `debug_assert!` (removed in release) | `Assert.Equal(expected, actual)` in a library; `Debug.Assert` is removed in release | Since there are no macros, the compiler knows `assert`, like `$"..."`. In return there is a single form, `assertEqual` is not needed |
| In a `test` block `try` works with every error type; the error fails the test and is printed (it must be `Formattable`, as in `main`) | A test can return `Result`; most code writes `.unwrap()` | Exception | The only place where `try` works without a target error type being written |
| A failed test is a panic. The test command runs each test in a separate process | A panic is caught per thread (unwinding) | The exception is caught | Process start-up time per test. Not a language rule, a tool decision |

The smaller option that was rejected: adding no keyword and looking at names as Go does (functions starting with `Test`
in a file ending in `_test`). Its cost: two invisible naming rules have to be known, and a test name cannot be a
sentence. The keyword was chosen by the criterion "no rule should be left that has to be known".

Left out on purpose, because code can be written without them: setup and teardown (an ordinary function and `dispose`),
parameterised tests (a `for` loop), tests that expect a panic (`#[should_panic]` in Rust; since every test is a separate
process this can be added later), mock objects, benchmarking.

## Compile errors

```
Expr* load() {
    string source = "1 + 2"
    return parse(read source) ?? panic("bad input")     // error: the tree borrows from source, source dies here
}

int sides(read Shape s) {
    return match s {                     // error: Empty is not handled
        Circle(r) => 0
        Rect(w, h) => 4
    }
}

parse(read source)                       // error: Result was not used. To discard it on purpose: _ = parse(read source)

void addAll(ref List<int> xs) {
    for x in read xs {
        xs.add(x)                        // error: xs is borrowed for the whole loop, it cannot be changed
    }
}

public int? pop(ref this) {
    var node = this.top                  // error: a field cannot be moved through ref. Hint: this.top.take()
    ...
}
```

Hint examples. An error whose fix is known comes with code that shows the fix:

```
error: xs is borrowed for the whole loop, it cannot be changed
   |
 3 |     for x in read xs {
   |              ------- the borrow starts here
 4 |         xs.add(x)
   |         ^^^^^^^^^ it is changed here
   |
hint: loop over a copy
   |
 3 |     for x in xs.clone() {
```

| Error | Hint |
|---|---|
| `Expr` carries a borrow, its source is not written | Add `from input` at the end |
| `read string` cannot be used as a `string` | `name.clone()` |
| `Result` was not used | `try f()`, or `_ = f()` to discard it on purpose |
| The `Empty` variant is not handled | Add an `Empty => ...` branch |
| `T?` used without a check | `if x is not null { }` or `x ?? default` |
| `x` used after it was moved | `read x` or `x.clone()` on the line where it was moved |
| The field written to is `readonly` | None: the intent of the assignment is not known, no hint is given |

A panic follows the same rule: it prints `index 5, length 3` and shows the safe way, `xs.get(i)`.

## What the examples revealed

All the decisions are in the [decision table](04-decisions-and-open-questions.md): naming the source at the type level,
integer overflow, implicit conversions, character and slice notation, `new`, constructor, collection initialisation,
field order, types that carry a borrow, `Copyable` and enum, `take`, `for` and ownership, string literal, `for` pattern,
index, `length`, `raw T` operations, number types, text formatting, the `string` guarantee, file API, the duration and
conflict of a `checked` borrow, `this` with a type, type alias, the form of `where`, equality, `weak T?`, copying
`shared T`, function value, `char`, panic and buffer, narrowing conversion, `print`
error, `NaN`, identity and equality, `shared` cycle, collection names, module, visibility, `use`, slice type, fixed-size
array, test, `assert`.

On 11 November the language was made smaller: automatic `Ok`, the heavy version of `Closable` (only the check that looks
at local variables remained), `ThreadFunc`, a block that produces a value on its own, and the flow check for moving a
field through `ref` were removed. The examples were updated accordingly: success is written with `return Ok(value)`.

On 12 November it was made smaller again: lambdas and capture were removed (a function value is the address of a
named function; "Order rules" was rewritten with a `for` loop and an `enum`), and generics of your own, `where` and
your own `interface` were deferred (`TopList` in "Folder tree" is written for one type, and the definition of `List`
is kept as a sketch).
