---
title: Why SG
description: The idea behind SG, the three principles that decide most questions, and what it looks like.
sidebar_position: 1
---

# Why SG

SG is a small systems language: about as small and as fast as C, memory safe like Rust, and written in a
notation that reads like C#.

## The idea

Most of what makes Rust safe is a handful of rules about who owns a value and who may look at it. SG keeps
those rules and tries to leave out nearly everything else. The aim is a language you can hold in your head the
way you can hold C, where the compiler still catches the use after free, the dangling pointer and the
forgotten error.

Three principles decide most questions:

1. **The safe way is the default; the risky way is chosen on purpose.** If something can go wrong, the
   language stops you or makes you say so.
2. **What can be caught when compiling is not left to run time**, and an error whose fix is known comes with
   the fix.
3. **Small and fast come first.** A feature has to pay for itself twice: in what it costs when the program
   runs, and in what it costs to learn. Lambdas, your own generics and your own interfaces were taken out or
   put off for that reason.

No single language is the model. The ownership rules are Rust's, the notation is close to C#'s, modules work
as in Go, tests are written as in Zig.

## A taste

```sg panic
struct Product {
    string code
    long price
}

enum StockError : Formattable {
    Unknown(string code)
}

// A look at a product, or nothing. `from products`: what comes back points into the list.
read Product? find(read List<Product> products, read string code) from products {
    for product in read products {
        if product.code == code {
            return product
        }
    }
    return null
}

Result<long, StockError> priceOf(read List<Product> products, read string code) {
    var product = find(products, code) ?? return Error(StockError.Unknown(code.clone()))
    return Ok(product.price)
}

Result<void, StockError> main() {
    var products = List<Product> {
        Product { code = "pen", price = 1500 },
        Product { code = "notebook", price = 4000 },
    }
    var total = try priceOf(read products, "pen") + try priceOf(read products, "notebook")
    print($"total: {total} cents")
    print(try priceOf(read products, "ink"))
    return Ok()
}
```

```text
total: 5500 cents
error: Unknown("ink")
```

A few things to notice:

- **A value has one owner**, and it is freed when the owner ends. There is no garbage collector and no
  `free` to forget.
- **`read` lends a value to look at, `ref` lends it to change.** The word is written where the value is lent,
  so the call site shows what a function can do to your data.
- **`T?` is a value that may be missing, `Result<T, E>` one that may have failed.** Neither can be used
  before it is opened, with `??`, `try`, `is null` or `match`. There are no exceptions and no null pointer to
  follow by accident.
- **`find` gives back a pointer into the list**, and the compiler knows it. In the compiled program it is the
  plain pointer you would write in C.

## What the compiler tells you

Using a value after it was given away:

```sg error
void main() {
    var names = List<string> { "Ada", "Grace" }
    var kept = names
    print(names.length())
    print(kept.length())
}
```

```text
error: `names` was moved and can no longer be used
  --> example.sg:4:11
 4 |     print(names.length())
   |           ^^^^^
  it was moved here:
  --> example.sg:3:16
 3 |     var kept = names
   |                ^^^^^
```

Returning a borrow of something that is about to disappear:

```sg error
read string longest(read List<string> names) {
    var best = names[0].clone()
    for name in read names {
        if name.length() > best.length() {
            best = name.clone()
        }
    }
    return read best
}

void main() {
    var names = List<string> { "Ada", "Grace" }
    print(longest(read names))
}
```

```text
error: this returns a borrow of `best`, and `best` is gone when the function returns
  --> example.sg:8:12
 8 |     return read best
   |            ^^^^^^^^^
  hint: return the value itself instead of a borrow of it
```

## Is it fast?

That is the claim, so it is measured. Each benchmark is written twice, in SG and in C by hand, and both go
through the same C compiler with `-O2`. Of the 61 programs, 37 take the time of C within 5%, 13 are faster
and 11 are slower. Some of them (time of SG divided by time of C; 1.00 is the same speed):

| Program | SG / C |
|---|---|
| Recursive `fib(40)` | 0.98 |
| A linked list of a million nodes, built, walked and freed | 1.01 |
| A list of a million numbers, filled, changed, added up | 1.02 |
| Three hundred million lookups that give back a borrow or nothing | 1.01 |
| Counting in a hash table | 1.20 |
| Printing two million structs | 0.47 |

The hash table is slower because it uses a keyed hash by default, which protects against crafted keys; with a
plain hash it is 0.99. The same programs were measured on Windows 11 too (64-bit Arm, in a virtual machine):
most take the time of C there as well, and those that lean on Windows' C library (`printf`, `fgets`) are
several times faster in SG. These are small programs on two machines, not proof: [SG against C](../benchmarks/against-c.md).

Real programs are measured too, written the same way in SG, Rust and Zig: [the trials](../benchmarks/programs.md).
Each module of the library is measured the same way against Rust's crates and Zig's own library, JSON first,
where SG reads, walks and writes 100 MB faster than `serde_json` and Zig's `std.json`, in less memory. Against
the faster of Rust and Zig, SG is faster in 8 of the 25 cases and slower in 14:
[every case](../benchmarks/index.md#at-a-glance-against-rust-and-zig).
