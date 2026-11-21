# Deliberately deferred

- **comptime:** a const given the value of a call was built in the hundred-and-fourth slice and taken out in the hundred-and-forty-fifth (["No call in a const"](rules/types.md#no-call-in-a-const), choice 1606); a value a function finds is a `static`. The rest of comptime, making types when the program is compiled (Zig's way to generics), stays out.
- **Not in the first version:** macros. Async was in this line until 14 November 2026; its direction is now decided ([Async, the direction](directions/async.md)), and it is built after threads.
- **After the first version:** implementing an interface outside the definition of the type (`extend`), using an interface
  as a type (`read Shape`), associated types, default method bodies, operator definitions, constant parameters, a variable that `is not` opens into the outer scope, a `sync` that pushes the data to disk after `close` (`Flush(true)` in C#), generic type aliases (`using Link<T> = shared checked T`), lazy list chains (like LINQ).

*Decided on 15 November 2026: no overloads, for now.* Two functions or methods do not share a name; a second form takes a
name of its own (`nextUlong()` beside `nextLong(low, high)`). An overload costs nothing at run time, since the call is
chosen when the program is compiled, but it meets the rules SG already has: a number takes the type it is used as, a
`byte` widens by itself and a value goes where a `T?` goes, so `f(5)` beside `f(int)` and `f(long)` would need the
ranking of conversions that C++ and C# have; the name of a function is a value (`apply(double, 5)`) and would stop
naming one; and a message would list candidates instead of saying what is missing. Go, Rust and Zig have none. If
the need grows, the doors are overloads by the number of arguments only, with no ranking, or default values of
parameters; both cost nothing at run time. The second was taken on 21 November 2026 (choices 1620 to 1623: "Default
values of parameters"), so that `toJson` and `Json.parse` take their options without new names.

*Deferred on 15 November 2026: properties.* A struct has fields, and what does work is a method: `text.length()`,
`source.token()`. A property (C#'s `get` and `set`) would look like a field and run code, so `x.y` would no longer
tell that it is cheap (`source.token` costs an atomic count); what a getter gives, owned or lent, would be a second
contract; and a setter has no place in memory, so `ref x.y` and `x.y += 1` would need rules of their own. The common
use, read from outside and written from inside, is a `public readonly` field. Talked through again if a real need
shows up.

*Deferred on 12 November 2026: generics of your own.* They cost nothing at run time, but they are the largest source of
concepts in the design (type parameters, `where`, your own `interface`, associated types, conditional `Copyable`), and
eleven slices of the interpreter ran without them. `Result<T, E>`, `T?`, `List<T>`, `HashMap<K, V>` and `HashSet<T>`
are known to the compiler and stay generic; the built-in interfaces stay as names written after `:`. A container of your
own is written for the type it holds. If a real need shows up it is talked through with that example. The rows moved
out of [the decisions by topic](rules/README.md), as they were:

| Topic | Decision | Reason |
|---|---|---|
| <a id="generics"></a>Generics | C# notation, compiled separately for each type | No run-time cost, in exchange the binary grows |
| <a id="form-of-where"></a>Form of `where` | Several conditions as `where T : A, B`, a separate `where` for each subject, in any order: `where K : Hashable where r from name`. At type level `struct TopList<T> where T : Comparable`, and a function that uses the generic writes the condition again. A method can put a condition on the type's parameter: `void sort(ref this) where T : Comparable`, and if the type does not meet the condition only that call gives an error | C# notation. C# has no condition on a method, but without the deferred `extend` this is how `List<T>.sort()` is written |
| <a id="for-pattern"></a>`for` pattern | Works by pattern, like `foreach` in C#. `takeItems()` is called for `for x in xs`, `readItems()` for `read`, `refItems()` for `ref`, and the returned value implements `Iterator<T>`. If the value is already an `Iterator` it is used directly | It could not be written with an interface, because associated types and overloading on the receiver are deferred |
