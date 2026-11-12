# Rust's syntax, the comparison with C#, and the "second C++" question

*Source: the conversation of 4 November 2026. The questions and answers are given as they were in the conversation; for correction notes see [decisions and open questions](../design/04-decisions-and-open-questions.md).*

## Question

> I also argue that a language should be simple, but here I think Rust really does have a syntax problem. It is on its way to becoming a second C++ in this respect :) Also, among the OOP languages C#'s syntax is quite clean; yes, it has a GC, but couldn't Rust have used a cleaner syntax like C#'s, why did it become so complex

## Answer

You are right on one point, but half of the problem is not syntax. Let me separate the two first.

**As syntax, Rust is actually close to C#.** `fn`, `let`, `struct`, `impl`, `match`, `<T>` for generics, `|x|` for closures. Taken one by one, none of these is heavier than C#. The feeling of complexity comes from here:

```rust
fn longest<'a, T: AsRef<str> + ?Sized>(a: &'a T, b: &'a T) -> &'a str
```

The noise in this line does not come from syntax but from **the information expressed in the type**. In C# the same function looks like this:

```csharp
string Longest(string a, string b)
```

But these two signatures do not say the same thing. The C# version carries a hidden assumption: "there is a GC, the string is on the heap, it does not matter how long the reference lives". The Rust version says "the returned reference cannot outlive whichever of a and b lives shorter". C# does not say this because it does not need to; the GC answers that question at run time.

So the real equation is this: **the information the GC hides from you has to be written in the signature in Rust.** Lifetimes, the `&` versus `&mut` distinction, the difference between `Box`/`Rc`/`Arc`/`RefCell`, `Send`/`Sync`: all of these are things the GC and the runtime decide in C#. Rust puts them in front of the person writing the code because it wants to prove them at compile time. This is not a syntax choice, it is information density.

**But there are also parts that really are syntax mistakes**, and here I agree with you:

- The `'a` lifetime notation is ugly. A single quote, it looks like an unclosed string. The design team admits this, but changing it after 1.0 is impossible.
- The `::` versus `.` duality, `turbofish` (`::<T>`), `?Sized`, `dyn`, `impl Trait` with different meanings in argument and return position. These are patches that piled up over ten years.
- Type towers like `Rc<RefCell<Option<Box<T>>>>`. In C# this is just `T`. Every layer has a meaning, but readability pays for it.
- The macro system (`macro_rules!`, proc macros) is a separate language. A corner much like C++'s template metaprogramming corner.
- Async: `Pin`, the `Future` traits, `async fn` in traits being missing for years. This is the area where Rust looks most "like C++".

**The "second C++?" question:** The similarity is real, but the difference matters too. C++'s complexity is cumulative: it carries C, C with Classes, 98, 11 and 20 on top of each other and cannot delete any of them. There are five ways to do the same job. In Rust there is one way for most features, and the edition mechanism (2015/2018/2021/2024) allows old mistakes to be fixed without breaking code. So Rust is growing, but it is not piling up layers the way C++ does. Even so, the corners of async and the trait system really have started to resemble C++; this is a legitimate criticism and it is voiced in the Rust community too.

**Could it have been cleaner?** Partly yes. In a few places Rust was too conservative:

- Lifetime elision could have been more aggressive. The compiler can infer most lifetimes, but the team chose to have them written out "so that they are visible when reading the signature". This is a choice of philosophy.
- Swift solved the same problem in a more hidden way: reference counting with ARC, `inout`, `borrowing`/`consuming` parameters. The syntax is clean, but the price is the runtime cost of the counters and leaks from reference cycles. So Swift is exactly the "Rust that looks like C#" experiment, and in exchange it gave up some of the guarantees.
- New languages such as Mojo, Hylo (formerly Val) and Carbon are researching exactly this question: "the same guarantees, less notation". Hylo's "mutable value semantics" idea removes references completely and so eliminates lifetimes. None of them has been proven in production yet.

**My view:** About two thirds of Rust's complexity is unavoidable, the information cost of memory safety without a GC. The other third is notation decisions made between 2010 and 2015 that can no longer be changed. C# looks clean because the runtime carries that information for you; you pay for it with GC pauses and with every object carrying a header on the heap. "Both C# syntax and Rust's guarantees" is something that does not exist today, but projects like Hylo show that it is possible in theory. Ten years from now Rust too will probably be remembered as "the first attempt" in this respect.
