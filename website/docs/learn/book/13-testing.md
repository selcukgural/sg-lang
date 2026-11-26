---
title: Testing
description: Tests written next to the code with `test` and `assert`, run by `sg test` in the interpreter or compiled, with leaks reported.
sidebar_position: 13
---

# Testing

Tests in SG are part of the language. A `test` block sits next to the code it tests, `assert` checks what
should hold, and `sg test` runs them. There is no framework to install and no naming convention to follow.

## `test` and `assert`

A test is `test "name" { ... }` at the top level of a file. `assert(condition)` stops the test when the
condition is false.

```sg test
int clamp(int value, int low, int high) {
    if value < low {
        return low
    }
    if value > high {
        return high
    }
    return value
}

test "a value inside the range is kept" {
    assert(clamp(5, 0, 10) == 5)
}

test "a value outside the range is moved to its end" {
    assert(clamp(-3, 0, 10) == 0)
    assert(clamp(42, 0, 10) == 10)
}

void main() {
    print(clamp(99, 0, 10))
}
```

`sg test example.sg` prints one line for each test and a count:

```text
ok    a value inside the range is kept
ok    a value outside the range is moved to its end
2 test(s) passed
```

`sg example.sg` and `sg build` leave the tests out: the program prints `10`, and the tests cost the built
program nothing. A file that holds only tests needs no `main` for `sg test`; run as a program, it is an error
whose hint is the test command. Each test starts with variables of its own, so one test cannot depend on what
another left behind. Two tests with the same name in one file are an error, and so is a `test` inside a
function.

## When a test fails

A test fails when an `assert` is false, when it panics, or when a `try` in it meets an error. The others still
run, and the command ends with exit code 1, so a script or a CI job sees it:

```sg nocheck
enum ParseError : Formattable {
    Empty
    NotANumber(string text)
}

Result<int, ParseError> parseAge(read string text) {
    if text.length() == 0 {
        return Error(ParseError.Empty)
    }
    var age = int.parse(text) ?? return Error(ParseError.NotANumber(text.clone()))
    return Ok(age)
}

test "a number is read" {
    assert(try parseAge("36") == 36)
}

test "a wrong expectation" {
    var ages = List<int> { 36, 45 }
    assert(ages == List<int> { 36, 54 })
}

test "an error passed on by try" {
    var age = try parseAge("forty")
    print(age)
}

test "a panic" {
    var ages = List<int>()
    print("before the panic")
    print(ages[0])
}

test "the tests after it still run" {
    assert(try parseAge("7") == 7)
}
```

```text
ok    a number is read
FAIL  a wrong expectation
panic: assert failed: ages == List<int> { 36, 54 }
  left: [36, 45]
  right: [36, 54]
  --> example.sg:20:5
 20 |     assert(ages == List<int> { 36, 54 })
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
FAIL  an error passed on by try
error: NotANumber("forty")
before the panic
FAIL  a panic
panic: index 0 is out of range: the length is 0
  --> example.sg:31:11
 31 |     print(ages[0])
    |           ^^^^^^^
  hint: `.get(i)` reads with `null` instead; before a write, compare the index with `.length()`
ok    the tests after it still run
3 of 5 test(s) failed
```

A failed `assert` writes the condition as it is written in the source, and for a comparison the value of each
side, also when they are lists, structs or enums. What a test prints comes before its line in the report.
Integer overflow is checked in tests, as in a debug build.

## `try` in a test

In a function, `try` hands an error to the caller. A test has no caller, so `try` takes the error of any type
and fails the test with it, as above. This keeps tests short: the happy path is written straight, and a
`match` is needed only where the test is about the error itself.

```sg test
enum ParseError : Formattable {
    Empty
    NotANumber(string text)
}

Result<int, ParseError> parseAge(read string text) {
    if text.length() == 0 {
        return Error(ParseError.Empty)
    }
    var age = int.parse(text) ?? return Error(ParseError.NotANumber(text.clone()))
    return Ok(age)
}

test "a number is read" {
    var age = try parseAge("36")
    assert(age == 36)
}

test "an empty text is an error" {
    match parseAge("") {
        Error(e) => assert(e is Empty)
        Ok(age) => panic($"read {age} from nothing")
    }
}

void main() {
    print(parseAge("12") ?? -1)
}
```

```text
ok    a number is read
ok    an empty text is an error
2 test(s) passed
```

`try` on a `T?` is different: it would give `null` to a caller, and a test has none. `sg test` refuses it:

```sg nocheck
int? find(read List<int> xs, int wanted) {
    for i in 0..xs.length() {
        if xs[i] == wanted {
            return (int)i
        }
    }
    return null
}

test "found" {
    var at = try find(read List<int> { 1, 2 }, 2)
    assert(at == 1)
}

void main() {
}
```

```text
error: `try` on `int?` returns `null` to the caller, and a test has no caller
  --> example.sg:11:14
 11 |     var at = try find(read List<int> { 1, 2 }, 2)
    |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  hint: say what is expected, which fails the test without it: `find(read List<int> { 1, 2 }, 2) ?? panic("...")`; or give a fallback with `??`
```

Write what you expect instead: `var at = find(read xs, 2) ?? panic("2 is in the list")` gives an `int`, or the
test stops with that message. The tests are checked only by the test command: `sg example.sg` and `sg build`
leave them out entirely, so run `sg test` before you call a change done.

## Compiled: `sg test --compiled`

`sg test` runs the tests in the interpreter, which needs no C compiler. `sg test example.sg --compiled` builds
them as a debug build and runs that. The report is the interpreter's, letter for letter, for passing and
failing tests alike. Compiled, each test runs in a process of its own:
the program of tests starts itself again for each one, so a test that crashes cannot take the others with it.
`sg build example.sg --test -o tests` keeps that program, and `./tests --only 0` runs the first test alone,
for a debugger ([Debugging a program](./15-debugging.md)): `gdb --args ./tests --only 0`.

## Tests of a module, and of a folder

A test in a module's folder belongs to the module: it sees the names that are not `public`, as the module's
other files do. A test in another module sees only what is public, as a real user would. In the program of
[Modules](./09-modules.md), a file of tests can go into `stock` beside `product.sg` and `warehouse.sg`:

```sg nocheck title="inventory/stock/tests.sg"
test "indexOf finds a code" {
    var products = List<Product> { Product("K-1".clone(), 1500) }
    assert(indexOf(read products, "K-1") is not null)
    assert(indexOf(read products, "X-9") is null)
}

test "received goods are counted" {
    var warehouse = Warehouse()
    warehouse.add(Product("K-1".clone(), 1500))
    assert(warehouse.receive("K-1", 20))
    assert(warehouse.products[0].quantity == 20)
}
```

`indexOf` and `quantity` are not public, and the test reaches them because it is inside `stock`. With a test
of the root added to `main.sg`:

```sg nocheck title="inventory/main.sg"
use stock

void main() {
    var warehouse = Warehouse()
    warehouse.add(Product("K-1".clone(), 1500))
    print(warehouse.available("K-1"))
}

test "an unknown code receives nothing" {
    var warehouse = Warehouse()
    assert(!warehouse.receive("X-9", 5))
    assert(warehouse.available("X-9") == 0)
}
```

`sg test inventory` runs the tests of the folder and of every module under it, each with its module in front
of its name; `--compiled` works here as well:

```text
$ sg test inventory
ok    an unknown code receives nothing
ok    stock: indexOf finds a code
ok    stock: received goods are counted
3 test(s) passed
```

`sg test inventory/main.sg` runs the tests of that file and of the modules it names. Two modules may have
tests with the same name; one module may not.

## Leaks fail a test

Owners of `shared checked` values that point at each other in a circle keep each other alive, and nothing
frees them (see [Ownership](./07-ownership.md)). SG does not let that pass in silence: a test that passes but
leaves `shared` values alive fails with a report of what is left, where it was made, and which field holds it.

```sg nocheck
struct Person {
    string name
    List<shared checked Person> friends
}

shared checked Person person(string name) {
    return new shared checked Person { name = name, friends = List<shared checked Person>() }
}

test "friends one way" {
    var ada = person("ada".clone())
    var grace = person("grace".clone())
    ada.friends.add(share grace)
    assert(ada.friends.length() == 1)
}

test "friends both ways" {
    var ada = person("ada".clone())
    var grace = person("grace".clone())
    ada.friends.add(share grace)
    grace.friends.add(share ada)
    assert(grace.friends[0].name == "ada")
}
```

```text
ok    friends one way
FAIL  friends both ways
leak: the test ended with 2 `shared` values still alive, which hold each other through `Person.friends`
  2 x `Person`, made here:
  --> example.sg:7:12
 7 |     return new shared checked Person { name = name, friends = List<shared checked Person>() }
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  hint: the side that does not own is `weak`: `List<weak checked Person>`
1 of 2 test(s) failed
```

The fix is the hint: a friend is looked at, not owned. With `List<weak checked Person>`, `weak grace` in place
of `share grace`, and `share grace.friends[0] ?? panic("ada is still alive")` to look, both tests pass.

The same report ends a program run by the interpreter, or built as a debug build, with exit code 1:

```sg panic
struct Person {
    string name
    List<shared checked Person> friends
}

void main() {
    var ada = new shared checked Person { name = "ada".clone(), friends = List<shared checked Person>() }
    ada.friends.add(share ada)
    print(ada.friends.length())
}
```

```text
1
leak: the program ended with 1 `shared` value still alive, which holds itself through `Person.friends`
  1 x `Person`, made here:
  --> example.sg:7:15
```

A debug build counts every block of memory its values take and give back, on every system, with no
sanitizer needed. A cycle of `shared` values is the only leak safe SG can make; any other block left over
would be a mistake in `sg` itself, and the report says so. A release build (`sg build --release`) has no
count and no report.
