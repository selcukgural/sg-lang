---
name: alias
kind: keyword
summary: Gives a type a second name: the same type, which builds what it builds.
see: public, use, shared, new, struct, enum
learn: ../docs/learn/book/09-modules.md#type-aliases-alias
---

## Forms

| Form | What it is |
|---|---|
| `alias Scores = List<int>` | a second name of a type, at the top of a file |
| `public alias Stops = List<Stop>` | a name other modules can use too |
| `Scores`, `read Scores`, `List<Scores>`, `Scores?` | it goes wherever a type goes |
| `alias P = Point`, then `P { x = 1, y = 2 }`, `P(1, 2)`, `P.origin()` | it builds what its type builds |
| `alias NodeRef = shared Node`, then `new NodeRef { ... }` | `new NodeRef` is `new shared Node` |
| `alias Op = Func<int, int, int>` | the type of a function |
| `alias Link = NodeRef?` | an alias in an alias |

## What it means

`alias Name = Type` gives a type a second name. It is not a new type but the same one, so values pass between
the two names freely: a function that takes a `read Scores` is given a `List<int>`. The alias is gone at
compile time: a value prints as what its type prints, and a type the compiler names in a message is the type
itself (``expected `List<int>`, found `string` ``).

An alias builds what its type builds. For a struct that is `P { ... }`, `P(args)` when the struct has a
`create` function, and its other functions, `P.origin()`; for an enum its variants, `Why.Empty`; for a
`shared` type, `new NodeRef { ... }` or `new NodeRef(args)`. A collection is the exception: it is built by the
name of its type, `List<int> { 1, 2 }`, not by its alias. In a pattern a variant is named alone, as always.

An alias is written at the top level of a file, one on a line, and is seen by every file of its module, in any
order. Like a type, it stays in its module unless it is written `public alias`, and a `public` alias cannot
show a type that is not `public`.

An alias cannot be a borrow (`read Text` is written where the borrow is), cannot take type parameters, cannot
take the name of another type, and cannot come back to itself, directly or through other aliases. `shared`
cannot be put around an alias that is `shared` already.

`alias` is a word only at the top of a file, in front of a name and `=`. Elsewhere it is a name: a variable, a
field or a function can be called `alias`.

## Examples

### A shorter name for a collection

```sg
alias Scores = List<int>
alias Board = HashMap<string, Scores>

int best(read Scores scores) {
    var top = 0
    for score in scores {
        top = top.max(score)
    }
    return top
}

void main() {
    Board board = HashMap<string, Scores> { }
    board.add("ada", List<int> { 36, 41 })
    board.add("grace", List<int> { 45 })
    List<int> plain = List<int> { 7, 3 }
    print(best(read plain))
    var mine = board.get("ada")
    if mine is not null {
        print(best(mine))
    }
    print(board.length())
}
```

```text
7
41
2
```

`best` takes a `read Scores` and is given a `List<int>`: they are one type. `Board` names `Scores` in its own
type. A collection is still built by the name of its type, `HashMap<string, Scores> { }`.

### A struct under a second name

```sg
struct Point : Formattable, Equatable {
    int x
    int y

    Point create(int x, int y) {
        return Point { x = x, y = y }
    }

    Point origin() {
        return Point { x = 0, y = 0 }
    }
}

alias P = Point

P mirror(read P p) {
    return P(p.y, p.x)
}

void main() {
    var a = P { x = 1, y = 2 }
    var b = P(3, 4)
    Point c = mirror(read b)
    print(a)
    print(c)
    print(P.origin() == Point.origin())
}
```

```text
Point { x = 1, y = 2 }
Point { x = 4, y = 3 }
true
```

`P { ... }` builds field by field, `P(3, 4)` calls `create`, and `P.origin()` is the function of `Point`. The
value prints as a `Point`, since that is what it is.

### The type of a function

```sg
alias Op = Func<int, int, int>

int add(int a, int b) {
    return a + b
}

int times(int a, int b) {
    return a * b
}

int fold(read List<int> xs, Op op, int start) {
    var total = start
    for x in xs {
        total = op(total, x)
    }
    return total
}

void main() {
    var xs = List<int> { 1, 2, 3, 4 }
    var ops = List<Op> { add, times }
    print(fold(read xs, ops[0], 0))
    print(fold(read xs, ops[1], 1))
}
```

```text
10
24
```

`Op` is shorter than `Func<int, int, int>` in the signature of `fold` and in the list of operations.

### A shared node, and an alias in an alias

```sg
struct Node {
    int value
    Link next
}

alias NodeRef = shared Node
alias Link = NodeRef?

int sum(read Link list) {
    var total = 0
    Link at = share list
    while at is not null {
        total += at.value
        at = share at.next
    }
    return total
}

void main() {
    var last = new NodeRef { value = 2, next = null }
    Link first = new NodeRef { value = 1, next = share last }
    print(sum(read first))
    print(sum(read last))
}
```

```text
3
2
```

`new NodeRef { ... }` is `new shared Node { ... }`, and `Link` is `shared Node?`. `Node` names `Link` before
the line that declares it: the order of the lines does not matter. `last` is a `NodeRef`, and is given where a
`read Link` goes, as a `shared Node` is given where a `read shared Node?` goes.

### An enum and a `Result`

```sg
enum Problem : Formattable {
    Empty
    NotANumber(string text)
    OutOfRange(int value)
}

alias Why = Problem
alias Port = Result<int, Why>

Port parsePort(read string text) {
    if text.length() == 0 {
        return Error(Why.Empty)
    }
    var n = int.parse(text) ?? return Error(Why.NotANumber(text.clone()))
    if n < 1 || n > 65535 {
        return Error(Why.OutOfRange(n))
    }
    return Ok(n)
}

void main() {
    var texts = List<string> { "8080", "", "http", "70000" }
    for text in read texts {
        match parsePort(text) {
            Ok(port) => print($"port {port}")
            Error(OutOfRange(n)) => print($"{n} is out of range")
            Error(why) => print(why)
        }
    }
}
```

```text
port 8080
Empty
NotANumber("http")
70000 is out of range
```

`Why.Empty` builds a variant of `Problem`. In the arms of the `match` the variant is named alone,
`OutOfRange(n)`, without `Why` or `Problem` in front.

### A public alias, from another module

```text
trip/
  main.sg
  route/
    stop.sg
```

```sg file title="trip/route/stop.sg"
public struct Stop : Formattable {
    public string name
    public int minutes
}

public alias Stops = List<Stop>
alias Minutes = int

public Minutes total(read Stops stops) {
    Minutes sum = 0
    for stop in stops {
        sum += stop.minutes
    }
    return sum
}
```

```sg file title="trip/main.sg"
use route

void main() {
    Stops stops = List<Stop> { Stop { name = "mill".clone(), minutes = 12 }, Stop { name = "bridge".clone(), minutes = 30 } }
    route.Stops same = stops
    print(same)
    int minutes = total(read same)
    print(minutes)
}
```

```text
$ cd trip
$ sg main.sg
[Stop { name = "mill", minutes = 12 }, Stop { name = "bridge", minutes = 30 }]
42
```

`Stops` is public, so `main.sg` names it, alone after `use route` or in full, `route.Stops`. `Minutes` stays
inside `route`; `total` may still give one, since a `Minutes` is an `int`, which every module names, and
`main.sg` takes it as an `int`.

## What the compiler refuses

### A collection built by its alias

```sg error
alias Scores = List<int>

void main() {
    var scores = Scores()
    scores.add(3)
    print(scores)
}
```

```text
error: `Scores` is `List<int>`; a collection is built by the name of its type
  --> example.sg:4:18
 4 |     var scores = Scores()
   |                  ^^^^^^^^
  hint: `List<int>()`
```

The variable can still have the alias as its type: `Scores scores = List<int>()`.

### An alias of a borrow

```sg error
alias Text = read string

void main() {
    print("hello")
}
```

```text
error: an alias names a type that owns its value, not a borrow
  --> example.sg:1:14
 1 | alias Text = read string
   |              ^^^^^^^^^^^
  hint: name the type alone and write `read` where the borrow is: `read Text`
```

### Aliases that name each other

```sg error
alias Tree = Forest
alias Forest = List<Tree>

void main() {
    print("hello")
}
```

```text
error: the aliases `Forest` and `Tree` name each other
  an alias is another name for a type that is written out in the end
  --> example.sg:2:16
 2 | alias Forest = List<Tree>
   |                ^^^^^^^^^^
```

A type that holds itself is a struct: `struct Tree { List<Tree> children }`.

### `shared` around a shared alias

```sg error
struct Node {
    int value
}

alias NodeRef = shared Node
alias Again = shared NodeRef

void main() {
    print("hello")
}
```

```text
error: `NodeRef` is already `shared Node`
  --> example.sg:6:15
 6 | alias Again = shared NodeRef
   |               ^^^^^^^^^^^^^^
```

### An alias inside a function

```sg error
void main() {
    alias Count = int
    Count n = 3
    print(n)
}
```

```text
error: there is no type named `alias`
  a type alias is written at the top of a file, outside every function: `alias X = int`
  --> example.sg:2:5
 2 |     alias Count = int
   |     ^^^^^
```

Inside a function, `alias` is a name, so the line reads as a variable `Count` of a type `alias`.

### The word `using`

```sg error
using Scores = List<int>

void main() {
    print("hello")
}
```

```text
error: a type alias is written with `alias`
  --> example.sg:1:1
 1 | using Scores = List<int>
   | ^^^^^
  hint: `alias Scores = List<int>`
```

### A public alias that shows a private type

A file added to `trip/route/`:

```sg file title="trip/route/plan.sg"
struct Leg {
    int minutes
}

public alias Plan = List<Leg>
```

```text
$ cd trip
$ sg main.sg
error: `Plan` is public, but it shows `Leg`, which is not
  --> route/plan.sg:5:14
 5 | public alias Plan = List<Leg>
   |              ^^^^
  `Leg` is defined here, without `public`:
  --> route/plan.sg:1:8
 1 | struct Leg {
   |        ^^^
  hint: write `public` in front of `Leg`, or take it off `Plan`
```

Other modules could name `Plan` but not what it holds.
