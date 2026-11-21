---
title: Functions
description: Declaring functions, parameters by value, read and ref, returning, recursion, functions as values, and using what a call gives.
sidebar_position: 4
---

# Functions

A function is written as in C and C#: the type it returns, its name, its parameters each with the type first,
and a body in braces. Functions live at the top level of a file, in any order: a function can call one written
below it.

## Declaring and returning

```sg
int square(int x) {
    return x * x
}

void greet(read string name) {
    print($"Hello, {name}")
}

void main() {
    print(square(7))
    greet("Pasha")
    print(cube(3))
}

int cube(int x) {
    return x * square(x)
}
```

It prints:

```text
49
Hello, Pasha
27
```

`void` says a function returns nothing; a `void` function can leave early with a bare `return`. A function
that returns a value must reach a `return` on every path, and the compiler checks it:

```sg error
int sign(int x) {
    if x > 0 {
        return 1
    } else if x < 0 {
        return -1
    }
}

void main() {
    print(sign(3))
}
```

```text
error: `sign` does not return a value on every path; it must return `int`
  --> example.sg:1:5
 1 | int sign(int x) {
   |     ^^^^
```

An `else { return 0 }` at the end makes it whole. Each function has one name: there is no overloading, so
`int f(int x)` and `int f(long x)` in one program are an error (`function f is defined twice`).

## Default values

The last parameters may have default values. A call that leaves them out gets them, as if it had written
them there:

```sg
void connect(read string host, int port = 80, int retries = 3) {
    print($"{host}:{port}, {retries} tries")
}

void main() {
    connect("example.org")
    connect("example.org", 8080)
    connect("example.org", 8080, 5)
}
```

```text
example.org:80, 3 tries
example.org:8080, 3 tries
example.org:8080, 5 tries
```

A call leaves arguments out only at its end, so a parameter after one with a default value needs one too;
there are no named arguments. A default is a value known when the program is compiled, as a const is (a
number, a text, a const, a struct of them), or `null`, an empty collection (`List<int>()`), a literal that
gives no field (`Options {}`), or `CancellationToken.None`. It calls no function, so a call never runs what it
does not show:

```sg error
int roll() {
    return 4
}

void play(int seed = roll()) {
    print(seed)
}

void main() {
    play()
}
```

```text
error: the default value of a parameter calls no function
```

A `ref` parameter has no default value, since it is given a place to change, and neither has a function of C.
A call through a `Func` value gives every argument.

## Parameters by value

A parameter written with only its type is given by value. A number, a `bool`, a `char` and any other value that
can be copied arrives as a copy: the function can change its copy, and the caller's variable stays as it was.

```sg
int countDown(int from) {
    int steps = 0
    while from > 0 {
        from -= 2
        steps += 1
    }
    return steps
}

void main() {
    int start = 9
    print(countDown(start))
    print(start)
}
```

It prints:

```text
5
9
```

A value that owns memory, such as a `string` or a `List`, is not copied: giving it by value *moves* it into the
function, which then owns it, and the caller can no longer use it. That is how a function takes a value to keep.
Most functions only need to look at such a value, or to change it in place, and for that they borrow it.

## `read` and `ref` parameters

`read T` lends a value to look at, and `ref T` lends it to change. The caller writes the same word at the
call, so a reader of the call sees what can happen to each argument:

```sg
void addOne(ref int count) {
    count += 1
}

nint longest(read List<string> names) {
    nint best = 0
    for name in read names {
        best = best.max(name.length())
    }
    return best
}

void main() {
    var count = 1
    addOne(ref count)
    addOne(ref count)
    print(count)

    var names = List<string> { "ada", "grace", "alan" }
    print(longest(read names))
    names.add("barbara")
    print(longest(read names))
}
```

It prints:

```text
3
5
7
```

The borrow lasts for the call. While it lasts, the compiler checks that nothing else changes or frees the
value, so `longest` can never see `names` half changed; and once the call returns, `main` has `names` back
and can add to it. A function cannot write through a `read` borrow:

```sg error
void addOne(read int count) {
    count += 1
}

void main() {
    var count = 1
    addOne(read count)
    print(count)
}
```

```text
error: `count` is a `read` borrow and cannot be written through
  --> example.sg:2:5
 2 |     count += 1
   |     ^^^^^
```

Leaving the word out at the call is an error whose hint is the word:

```sg error
void addOne(ref int count) {
    count += 1
}

void main() {
    var count = 1
    addOne(count)
    print(count)
}
```

```text
error: expected `ref int`, found `int`
  --> example.sg:7:12
 7 |     addOne(count)
   |            ^^^^^
  hint: lend it with `ref`: `ref count`
```

A literal or another value made for the call needs no word when it goes to a `read` parameter:
`greet("Pasha")` above lends the text for the call. A function can also return a `read string` that looks into
a text it was lent, as `trim()` does. [Ownership](./07-ownership.md) has the rules of moving and borrowing in
full.

## Recursion

A function can call itself:

```sg
long factorial(int n) {
    if n <= 1 {
        return 1
    }
    return n * factorial(n - 1)
}

int fibonacci(int n) {
    if n < 2 {
        return n
    }
    return fibonacci(n - 1) + fibonacci(n - 2)
}

void main() {
    print(factorial(20))
    print(fibonacci(20))
}
```

It prints:

```text
2432902008176640000
6765
```

`n * factorial(n - 1)` multiplies an `int` by a `long`, so `n` becomes a `long` on its own. `factorial(21)`
would not fit in a `long`, and stops the program with an overflow panic, as any arithmetic does.

## Functions as values

`Func<int, bool>` is the type of a function that takes an `int` and returns a `bool`: the last type is what it
returns, and `Func<void>` takes nothing and returns nothing. The name of a function is such a value. It can be
given to another function, kept in a variable, a field or a list, and called like a function:

```sg
int twice(int x) {
    return x * 2
}

int square(int x) {
    return x * x
}

bool isEven(int x) {
    return x % 2 == 0
}

int apply(Func<int, int> f, int value) {
    return f(value)
}

int countWhere(read List<int> items, Func<int, bool> keep) {
    int count = 0
    for item in read items {
        if keep(item) {
            count += 1
        }
    }
    return count
}

void main() {
    print(apply(twice, 5))
    Func<int, int> step = square
    print(step(4))
    step = twice
    print(step(4))

    var steps = List<Func<int, int>> { twice, square, twice }
    int value = 3
    for each in read steps {
        value = each(value)
    }
    print(value)

    var numbers = List<int> { 1, 2, 3, 4, 5, 6 }
    print(countWhere(read numbers, isEven))
}
```

It prints:

```text
10
16
8
72
3
```

A `Func` is only the address of a named function. There are no lambdas and no closures, so a `Func` holds no
hidden state, costs nothing to copy, and can go to another task freely. Writing one is an error that says so:

```sg error
int apply(Func<int, int> f, int value) {
    return f(value)
}

void main() {
    print(apply(x => x * 2, 5))
}
```

```text
error: there are no lambdas in the language
  --> example.sg:6:17
 6 |     print(apply(x => x * 2, 5))
   |                 ^^^^
  hint: write a function and pass its name: `apply(double, 5)`
```

The types of a `Func` must match exactly: a `Func<int, bool>` does not go where a `Func<int, int>` is
expected. A `Func` can take borrows too: `Func<read string, nint>` is a function that looks at a text and
returns a length.

## Using what a call gives

A value that a call returns must be used: kept, passed on, or compared. A call whose value is dropped is an
error, because a value that nobody looks at is usually a mistake, such as an error never checked or a new text
taken for a change in place:

```sg error
int square(int x) {
    return x * x
}

void main() {
    square(4)
}
```

```text
error: the `int` that `square` gives is not used
  --> example.sg:6:5
 6 |     square(4)
   |     ^^^^^^^^^
  hint: keep it: `var value = square(4)`; to drop it on purpose, `_ = square(4)`
```

The same rule catches a method that returns a new value instead of changing its own:

```sg error
void main() {
    var name = "Pasha"
    name.replaceAll("P", "B")
    print(name)
}
```

```text
error: the new text that `replaceAll` gives is not used: it leaves `name` as it was
  --> example.sg:3:5
 3 |     name.replaceAll("P", "B")
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^
  hint: to change `name`, assign the new text to it: `name = name.replaceAll("P", "B")`
```

When a value really is not needed, `_ =` drops it on purpose, and a reader sees that it was meant:

```sg
int square(int x) {
    return x * x
}

void main() {
    _ = square(4)
    _ = int.parse("not a number")
    print("done")
}
```

It prints:

```text
done
```

The rule has no exceptions; it holds for every function and method, a `Result` included.
