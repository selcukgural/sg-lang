# Decisions and open questions

What has been decided, what is still open, and what comes next. The reasons for the dropped generation model are
in the [memory model](01-memory-model.md) file.

## The idea of the language

A language's reason to exist should fit in one sentence. The direction changed on 5 November, and the current
sentence is: **a systems language as fast as C, without macros, that gives Rust's compile-time guarantees in a
notation that reads like C#.** The solutions come from Rust, the naming and the notation are ours. No arena. A
side project / research, with no revenue goal.

## Core principle: safe default

*Decided on 6 November 2026.* That people can trust this language matters as much as speed. If something can go
wrong, the default behaviour is not to stay silent but to warn or to stop. The risky path is allowed, but it is chosen on purpose:

- Checks are on by default and give a panic or a compile error.
- The unchecked path is visible in the code: `wrappingAdd`, `unsafe { }`.
- The choice is made per line. There is no compiler setting that turns checks off for the whole program.
- An error that can be caught at compile time is not left to run time: the code does not compile, and the error shows in the editor while typing. Panic
  is only for cases whose outcome depends on data and cannot be known at compile time (such as an index typed by the user).

- An error whose fix is known comes with code that shows the fix. An error says "what to write" as much as "what is wrong".

For this reason the checking part of the compiler is written as a library that the language server will also use. The rules for hints:

- The suggested code must compile, and the compiler's tests verify this. Where there is no certainty, no hint is given.
- The risky path (`unsafe`, `wrappingAdd`) is never suggested as a hint. The safe path is shown.
- A mechanical hint is applied in the editor with one click.
- A panic message also prints the values and the safe path: `index 5, length 3`, `xs.get(i)`.

If a performance decision conflicts with this principle, the principle wins, and the loss is recovered through the path that is written on purpose. Every new decision is
tested against it.

The goal of the language is not to be complex and to avoid bad notation. On its own the principle always pushes towards more rules, and every compile
error rule also rejects some correct code and adds to the rules that must be learned. So before a new rule is added
it is weighed with these four questions:

1. Does the error it catches silently give a wrong result, or is it visible anyway?
2. How often does it fire in real code?
3. Can the fix be shown in one line and with a hint?
4. Does it need a new concept?

A rule that does not satisfy all of them can still be added, but its cost is then accepted knowingly.

*Added on 11 November 2026.* The goal is performance and simplicity like C: the language stays as small as possible. Before a new feature
is added, the question is "can this be written without it", and there are never two ways to do the same thing. On the same day the table was reviewed with this in mind,
and what was removed is in the [dropped decisions](#dropped-decisions) section.

*Added on 12 November 2026.* We are not tied to a single language. Rust's guarantees, C#'s notation, Go's simplicity, Zig's
smallness: what each language does best is taken in the form that fits us, and is not taken if it does not fit. A proposal is accepted not because
"Rust does it this way" but because it fits the goals of this language.

## Decisions made

| Topic | Decision | Reason |
|---|---|---|
| C compatibility | Unimportant | Performance comes first |
| Use-after-free | At compile time: single ownership and borrowing, Rust's rules | The error should appear at compile time, not in production. The problem is not the rules but Rust's notation |
| Arena | Not in the language | Going with Rust's solution is enough, no separate memory model is needed |
| Lifetime notation | No `'a`; `from a, b` at the end of the signature, with default rules | The name of the source variable is written instead of an abstract name. Details: [syntax](02-syntax-draft.md) |
| `from` and `where` | `from` states the source of the return value, `where` states a type constraint. If another parameter stores the borrow, `where r from name` | `where return from` was crowded, and it is clear that the subject is `return` most of the time |
| Read-only borrow | `read T` | It says directly what it does and is not confused with `for x in xs` |
| Owning pointer | `T*`, created with `new` | Short, and consistent with the `T*?` notation |
| Move | Implicit, the compile error shows the line where the value was moved | `list = push(move list, 1)` was getting crowded |
| Local borrow | `read T x` is not written. `var` infers the type, and the borrow is on the right-hand side: `var x = read items[0]` | `read` is a contract in a signature and in a field, and in a local it is information the compiler already has. `string x` asks for an owner; assigning a borrow is an error, a copy must be made |
| Null | No universal null: `T` cannot be empty, every type has `T?`, built into the language | Implicit null is a source of bugs; `Option<T>` would need generics, and magic values like `-1` should go too. `Node*?` is 8 bytes, `int?` is 8 bytes with a flag |
| Array | Slice (pointer + length), bounds check panics | |
| Panic | Only for programmer errors (array bounds, `checked` conflict), the whole process stops. The only exception is `print`, in the "`print` error" row | No unwinding. Errors expected from the environment are returned as values with `Result` and need no unwinding |
| Safety checks | On in release too, cannot be turned off | If they could be turned off, the design would lose its reason |
| Macros | None | |
| `T?` check | `if x is null`, `while x is not null`; flow narrowing on local variables and parameters | After the check the same name counts as `T`. Fields are not narrowed, a local copy is taken first |
| `T?` unwrapping | `x ?? default`, `x ?? panic("message")`; `orelse` is gone | No separate `throw.ifnull` is needed: `throw` suggests something that can be caught, while panic stands in for every type |
| Name of the empty value | `null` | Familiar from C#, but valid only in the `T?` type |
| Notation direction | C#: type-first declarations, `public`, `int`, `var`, `Node { value = v }` | No semicolons, `while` without parentheses |
| `T*?` notation | Postfix, read left to right | `Node*?` is a nullable pointer, `Node?*` is a pointer to a nullable value; C#'s pointer notation |
| `ref` | Writable borrow (`&mut T`); also written at the call site | Familiar from C#. Can also be used in a return type and in a field |
| `main` | `void main()` | Every function starts with its return type, no exceptions |
| Shared ownership | `shared T`, a second owner with `share x`, creation with `new shared x` | One name for `Rc` and `Arc`: the counter is always atomic. A non-atomic version can be added later if needed, the reverse is hard. The cost is paid only on `share` and when an owner lets go, a borrow does not touch the counter |
| Weak reference | `weak T`, `weak x`. The result of `share w` is `shared T?` | Not an owner, breaks cycles. `null` if the object is dead |
| Interior mutability | `checked T` (`RefCell`) borrows on access and panics on a conflict. Its duration is in the "`checked` borrow duration" row | A run-time check like the array bounds check, visible in the type |
| Lock | `locked T` (`Mutex`), access only inside a `lock x as c { }` block | A two-step operation cannot be left halfway. Since a panic stops the process, there is no poisoning and no `.unwrap()` |
| Escape hatch | `raw T`, dereference only inside `unsafe { }` | Since `T*` is the owning pointer, the raw pointer gets a separate name |
| `?` and borrows | `ref T?` is a borrow of the original variable of type `T?`; "the borrow may be absent" is written `(ref T)?`. `read T?` has a single meaning, `shared T?` says the pointer may be absent | A C# reader expects `ref T?` to read this way. Since `read` cannot write, the two readings do not differ. `T*` cannot be empty; null cannot be assigned through `ref T*`, the caller's type would be lying |
| Generics | C# notation, compiled separately for each type | No run-time cost, in exchange the binary grows |
| Methods | In the struct body, with `read this`, `ref this` or `this` as the first parameter. Field access must be written `this.x`. At the call, `read`/`ref` is not written for the receiver | `this` follows the same rule as the other parameters, and fields are not confused with local variables |
| Interface | C# notation: `struct Version : Comparable`, `where T : Comparable`, the implementing type is `Self`. Generic interfaces exist | Static dispatch in the first version |
| Copy and cleanup | The `Copyable`, `Clonable`, `Disposable` interfaces. The compiler generates the body of the first two | No macros. `dispose` does not run during a panic |
| Source name parameter | `Match<from text, from pattern>`, also for a single source: `Parser<from source>`. If the type has a single source, `from` is not written on the field, and if it has more than one it is: `read string found from text`. In a method `from source`, outside `from lx.source` | It is told apart inside the same `<>` as a type parameter. `from this.lexer.input` was pulling a private field into the signature. With a single source there is no ambiguity, and repeating it on the field is noise |
| Thread rule | An argument to `spawn` cannot carry a borrow, an unlocked `checked` or a `Func`, including one stored in a struct field | Since the counter is always atomic, Rust's `Send` and `Sync` distinction comes down to this single rule. `Func` is forbidden because its type does not show what it holds |
| `enum` | Variants carry data: `Circle(int radius)`. Construction with the full name: `Shape.Circle(2)`, `new Expr.Number(1)` | Takes a tag plus the size of the largest variant, no heap. An enum that contains itself needs `T*` |
| `match` | `match x { Circle(r) => ... }`, as an expression or a statement. A missing variant is a compile error. Short names in patterns | When a new variant is added, every place that needs a look gives an error |
| Patterns | `_` catches everything, `var other` also takes the value, an unknown bare name is an error. The condition is `when` | A mistyped name should not silently become "catch everything" |
| `match` and ownership | `match x` behaves like `var y = x`: it moves if `x` is an owner, `match read x` and `match ref x` borrow, and if `x` is already a borrow it gives the same kind of borrow | No new rule |
| Testing a single variant | `if s is Circle(r) { }`, the same word as `is null` | The counterpart of Rust's `if let` |
| Recoverable errors | `Result<T, E>` is an ordinary enum: `Ok(T value)`, `Error(E cause)`. Success is `return Ok(value)`, and for an operation that returns no value `Result<void, E>` and `return Ok()`. `main` can return it too, the error is printed and the exit code is 1 | No new type feature is needed, and because it is a value it can be stored and passed to generic code. The `int f() fails E` notation was ruled out for this reason |
| Error propagation | `try x`, a prefix like `await`. On an error it returns to the caller, on a `T?` it returns `null`. Parentheses in a chain: `(try a()).b()` | `?` is used as a type suffix. Returning through `try` is a normal exit, `dispose` runs |
| `Result` unwrapping | `x ?? default`, `x ?? panic("message")`. There can be a `return` on the right of `??` | The same operator as for `T?`, like `x ?? throw` in C# |
| Error conversion | `try` finds by itself the variant of the target error type whose only field is the source type. If there is none or more than one, a compile error | No hand-written `From` as in Rust, and no macro either |
| Unused `Result` | Compile error, and `_ = f()` to discard on purpose. A `Result` that is assigned to a variable and never unwrapped gives the same error, an exit through `try` is exempt | An error should not be lost silently |
| Integer overflow | Panic in release too. `tryAdd` returns `int?`, `wrappingAdd` wraps on purpose | The first application of the core principle: a silent wrong result cannot be the default. We deliberately differ from Rust, the cost is below, and a hot loop is rescued with `wrappingAdd` |
| Implicit conversion | `read T` stands in for `T` if `T` is `Copyable`. A `read T*` can be given to a `read T` parameter | Otherwise a dereference would be needed on every line |
| `new` | Required. It puts the value on the heap and gives a `T*`, it does not mean creating an object. `HashMap<K, V>()` takes no `new`, it is the value itself | A separate heap allocation should stay visible, and `var` should know which type is wanted. An implicit conversion can be added later, but cannot be removed |
| Constructor | `T(args)` is short for the call `T.create(args)`. `create` is an ordinary function and can return a `Result` | There is no half-built object, and no separate constructor concept is needed |
| Collection initialisation | `List<int> { 1, 2 }`, `HashMap<string, int> { {"x", 10} }`. Each item is an `add` call, and `reserve(n)` is called first. `HashMap()` and `add` are valid too | As in C#, works without macros, and user collections benefit as well |
| Field order | The compiler orders struct fields freely so that no gaps remain | Since C compatibility is unimportant, a memory gain at no cost |
| Characters and slices | `'+'` is a byte constant, `string` is UTF-8 and byte indexed, a slice is `text[a..b]` (C#). `read string` is pointer + length. Unicode operations are in the library. The single-character type is in the "`char`" row | The language core stays small, indexing is O(1) |
| Types that carry a borrow | Every type that contains a borrow declares its source: `enum Token<from source>`. The rule of not writing `from` holds only when `read` or `ref` is visible in the return type. If the borrow is hidden in the name of the type, `from` is required: `Result<Expr*, E> parse(read string input) from input` | The dependency should be readable from the signature. It is not written on parameters and locals: on a parameter the borrow ends with the call, and on a local a compile error shows it if there is a problem |
| `Copyable` and enums | The same as for structs: all fields of all variants must be `Copyable`. `read T` and `raw T` count; `ref T`, `T*`, `string`, `shared T`, `weak T`, `locked T`, `checked T` do not. On a generic type `: Copyable` holds if the type parameters are `Copyable` too | Without this, `T?` and `Result<T, E>` could never be copied |
| `take`, `replace`, `swap` | In the standard library: `x.take()` only on `T?`, gives the value and puts `null` in its place. `x.replace(newValue)` gives the old value, on every type. `swap(ref a, ref b)`. A field is moved through a `ref` only with these: `var node = this.top` is a compile error, the hint is `this.top.take()` | Rust's `mem::take` needs a default value, and we do not have that concept |
| `Disposable` and moving a field | Moving a field out of a `Disposable` type is a compile error | `dispose` should not see a half object |
| `for` and ownership | The `match` rule: `for x in xs` moves, `for x in read xs` and `for x in ref xs` borrow, and if `xs` is already a borrow it gives the same kind of borrow. Changing `xs` during the loop is a compile error | No new rule. The run-time `InvalidOperationException` of C# appears here at compile time |
| `readonly` field | Given only at construction, cannot be assigned afterwards. The whole object can be changed through a `ref` | Immutability should be checked at compile time. It does not replace `read`: `read` is a borrow, a `readonly` field owns its own data |
| `for` pattern | Works by pattern, like `foreach` in C#. `takeItems()` is called for `for x in xs`, `readItems()` for `read`, `refItems()` for `ref`, and the returned value implements `Iterator<T>`. If the value is already an `Iterator` it is used directly | It could not be written with an interface, because associated types and overloading on the receiver are deferred |
| Index | `xs[i]` is short for the call `xs.at(i)` when reading and `xs.refAt(i)` when writing | The `T(args)` pattern. No general operator definition is needed, and user collections benefit as well |
| `length` | A method everywhere: `xs.length()`, `text.length()` | Since a `public` field can be written from outside, `xs.length = 100` would corrupt the list |
| `raw T` operations | `Memory.allocate<T>(n)`, `Memory.free(p)`, `Memory.moveItems(a, b, n)`, `p.put(i, v)`, `p.take(i)`, `p[i]` | Memory operations are in the library, and dereference is still only inside `unsafe { }` |
| Number types | Based on the C# types: `byte`, `short`, `int`, `long`, their unsigned counterparts, the pointer-sized `nint` and `nuint`, `float`, `double`. `int` is 32 bits on every platform. Indexes and lengths are `nint` | No new names for a C# reader. If `int` were the index, `List` would stop at 2 billion items |
| String constant | The type of `"x"` is `read string`. Where a `string` is expected it becomes a `string` without allocating: capacity 0, data inside the program. It is not freed, and the first write or growth copies first. Only for constants, a `read string` variable needs `.clone()`. If a function with no borrow parameter returns `read string`, the source can only be a constant, and nothing needs to be written | No `.clone()` noise, and no hidden heap allocation either. To keep a constant in a struct field, `string` is enough, no separate source name is needed |
| Text formatting | `$"{a}: {b}"`, C# notation. It is a language feature, checked at compile time, and does not allocate. The result is a value that borrows its arguments and implements `Formattable`. For a `string`, `.toString()` is written. `{{` writes a curly brace | It has to be in the language because there are no macros. The only difference from C#: so that heap allocation stays visible, a `string` is not created by itself, and where a `string` is expected the hint is `.toString()` |
| `Formattable` | `void format(read this, ref Formatter f)`. `print` and `writeLine` accept it. If no body is written the compiler generates one, and all fields must be `Formattable`. The error type of `main` must be `Formattable` | A write error stays in the `Formatter`, and `writeLine` returns it. This way there is no `try` noise inside every `format` |
| `string` guarantee | Always valid UTF-8. A bad byte on reading gives `IoError.InvalidText(long position)`, the only way from bytes to text is `string.fromUtf8`, which returns a `Result`, and raw data is read as a `byte` slice. String constants are checked at compile time. A slice through the middle of a character panics. `text[i]` is read only, a byte cannot be written | Core principle: bad data should not silently become `�` (C#), and Unicode operations should not meet garbage |
| File API | C# names: `File.openText` gives a `StreamReader`, `File.createText` gives a `StreamWriter`. `reader.readLine(ref line)` reuses the buffer and returns `Result<bool, IoError>`. `reader.lines()` gives an owned `string` per line and ends after the first error. `int.parse` returns a `Result` | `Iterator.next` cannot lend from its own buffer, since `next` would change that buffer on the following call. And there is no loop that gives the same error forever |
| `HashMap.getOrAdd` | `map.getOrAdd(key, initial)` returns `ref V` and copies the key only if it is missing | One hash, one `clone`. `counts[k] += 1` panics if the key is missing |
| `dispose` order | The reverse of declaration | What is declared later may have borrowed from what is declared earlier |
| Close error | `writer.close()` takes `this`, writes the buffer and returns a `Result`. `Closable` is an empty marker (`StreamWriter : Closable`): if a local variable of such a type is dropped at a normal exit without being closed or moved elsewhere, it is a compile error, and the hint is `try writer.close()`. An error exit through `try` is exempt, `dispose` runs there. To give up on purpose, `_ = writer.close()`. If the value is moved into a struct field, a list or a `shared`, the compiler stops tracking it and there is no restriction | A forgotten `close` silently loses the write error. In C# the same idea is an analyser warning (CA2000), in Rust there is no check. It is an error because the language has no concept of a warning and the check is exact. The heavy version that tracked everywhere was removed on 11 November, and this version that looks only at locals was added the same day. Cost: one marker and one rule; a value moved into a field and dropped without being closed is not seen, and the responsibility lies with whoever writes the struct |
| `checked` borrow duration | An access borrows for the duration of the expression. A borrow at the head of a `for` or `match` lasts for the whole statement. A local variable cannot hold a `checked` borrow, compile error. A block for several lines: `read x as n { }`, `ref x as n { }` | With the "for the duration of the expression" rule it was impossible to loop over a `checked` list. The block has the same form as `lock x as c { }`, no new word is needed, and the duration of the borrow is visible in the code |
| `this` with a type | `this` can also take a type: `read FolderRef this`, `FolderRef this`. `read this` is short for `read Folder this`. The method is called only on a value of that type | `ref this` does not give the wrapper that holds the counter, so the method could not write `weak this` or `share this`. The method does not lock the object for the whole call, and code that walks upwards does not conflict with itself. `read shared checked this` was considered too, and since an alias cannot be written there, a single form remained |
| Type alias | `using FolderRef = shared checked Folder`, C# notation, can take `public`. It is not a new type but the same type, and it disappears at compile time. `new FolderRef(args)` means `new shared checked Folder(args)`, and `FolderRef?` is `shared checked Folder?` | Repeating `shared checked` was making signatures long. |
| `checked` conflict | A conflicting access on the same variable while a borrow is in progress is a compile error: `for x in read a.items { a.items.add(y) }`. A conflict that comes through another variable panics, and the message prints both lines | The `for` rule extended to `checked`: code that would panic every time it runs does not compile. Cost: location information per `checked` and one write on every borrow, the principle wins |
| Form of `where` | Several conditions as `where T : A, B`, a separate `where` for each subject, in any order: `where K : Hashable where r from name`. At type level `struct TopList<T> where T : Comparable`, and a function that uses the generic writes the condition again. A method can put a condition on the type's parameter: `void sort(ref this) where T : Comparable`, and if the type does not meet the condition only that call gives an error | C# notation. C# has no condition on a method, but without the deferred `extend` this is how `List<T>.sort()` is written |
| Equality | `Equatable` (`bool equals(read this, read Self other)`), `Hashable` (`long hash(read this)`). If no body is written the compiler generates one field by field. `a == b` is short for `a.equals(b)`; `a < b` is short for the call `a.compare(b) < 0`. `==` on a type that is not `Equatable` is a compile error. `HashMap<K, V> where K : Equatable, Hashable`. Numbers and `string` implement all three | The `xs[i]` pattern: no general operator definition is needed. There is no silent reference comparison as in C# |
| `weak T?` and `share` | The result of `share` is a single `null`: `shared T?`, whether it is not linked or the object is dead. `x is null` answers "is it linked", `share x` answers "is it alive". If the difference matters, `if x is not null { var up = share x ?? panic("parent was deleted") }` | There is no `T??`. The difference is not needed in most places, and where it is, two separate questions are asked |
| Copy of `shared T` | `Clonable`, and `.clone()` means `share`, it does not copy the content. `List<shared T>.clone()` shares the items | So that `for child in folder.children.clone()` can be written. For a single value the `share x` notation stays |
| Function value | `Func<int, bool>`, the last parameter is the return type. It is built into the language and takes any number of parameters. Since `void` is a type there is no `Action`: `Func<int, void>` | The C# name. In C# `Action` is needed only because `void` cannot be a type argument |
| Using `Func` | A parameter is always `read Func<...>`: it is a borrow for the duration of the call, with no heap. There is no `ref Func`. `Func<...>*` is an owner and is built with `new`. A lambda cannot cross to a thread, and there is no `shared Func`. A bare `Func` cannot be a variable type, its size is unknown. In a local, `var f = (int x) => x + 1` | No new ownership rule is needed, and heap allocation stays visible. The only exception to the deferral of using an interface as a type |
| Capture | Without `new` a lambda borrows, with `new` it moves, and a `Copyable` value is copied. There is no `move` keyword. Capture is per field: `this.items.removeAll(x => x > this.limit)` captures only `limit` | A one-sentence rule. Since there is no GC, the owner of what is captured must be clear |
| Changing what is captured | A lambda freely changes what it captured, a stored lambda too, and the user writes nothing extra. The compiler behaves as if it had added `checked` to every capture that is written to, and the check is per access. A panic happens only on a real conflict (if the same lambda is entered again and writes to `seen` while `for x in read seen` is in progress). If a `new` lambda writes to a variable, the variable counts as moved even if it is `Copyable` | A lambda that writes is correct code, and forbidding it catches no error, it rejects correct code. Since only the lambda touches what is captured, hiding the check is safe. The question "does it write" disappears from the signature, and `ref Func` becomes unnecessary. Rust checks at compile time, and for simplicity we left the rare case to run time |
| Writing after a copy | Writing later to a variable that was copied with `new` is a compile error, including in the next iteration of a loop. The fix is to take the copy into a separate variable: `var current = limit`. No hint is given because the intent cannot be known | In C# the lambda sees the new value, with us the copy keeps the old value. It adds no syntax and no concept, and the cost is one line only when the rule fires |
| Moving out from inside a lambda | A captured owned value cannot be moved out inside the lambda, the hint is `.clone()`. There is no single-use lambda | The biggest need was `spawn`, and that already takes a call. The ban can be relaxed later |
| Lambda body | `x => expression` or `x => { ... }`. The block is a function body and needs `return`. `return` and `try` leave the lambda, as in C# | A lambda is a function too. In a block in a `match` arm the last line is the value, and this difference was accepted knowingly |
| `char` | 32 bits, holds every Unicode character (`Rune` in C#). `'+'` fits the expected type like a number constant: `byte` where a `byte` is expected, `char` where a `char` is expected, and `var c = '+'` gives a `byte`. A constant that does not fit in one byte (`'é'`) is a compile error where a `byte` is expected, and the hint is `char`. To walk through text character by character, `text.chars()` | C#'s 16-bit `char` splits an emoji into two parts, and that mistake should not be carried over. Cost: the same name, a different size from C# |
| Panic and buffers | `dispose` does not run on a panic, the last lines in a file buffer are lost, and this was accepted. `stdout` is flushed before the panic message. `writer.autoFlush = true` (the C# name) writes every line at once and is the recommended way for logs | The half-written file of a crashed program is not to be trusted anyway, and trying to write a buffer in a broken state means running code. Cost: a log that does not turn on `autoFlush` loses the lines from the moment of the crash |
| Named function | A function name is written instead of a lambda where a `Func` is expected: `orders.removeAll(isPaid)`, and `new isPaid` if it is to be stored. Only a free function and the `Order.isPaid` form. For one bound to an object (`this.isValid`) a lambda is written | As in C#, no new concept. The form bound to an object would secretly borrow `this` |
| Narrowing conversion | Written `(int)x`, and it panics if the value does not fit (the overflow rule). `x.tryToInt()` returns `int?`, `x.wrappingToInt()` truncates on purpose. A conversion whose value is known at compile time and does not fit is a compile error: `(byte)300`. No warning on every conversion; hovering over it in the editor shows the possibility of a panic and the two methods, and the panic message shows the value and `tryToInt`. `(int)3.7` gives 3, `NaN` or out of range panics. A widening conversion (`int` → `long`) is implicit | C# truncates silently and a wrong number comes out. A warning on every line would also appear in correct code and would be ignored; `a + b` and `xs[i]` do not warn either. Cost: one comparison per conversion |
| `print` error | `print` does not return an error. If it cannot write, the program stops with a message, exit code 1. On a closed pipe (`program \| head`) it exits silently, like Unix tools. Whoever wants to handle the error writes `Console.out.writeLine(...)`, which returns a `Result` | If it returned an error, every line would need a `try`. Rust panics too, but it also prints a message on a closed pipe, and we differ there. Cost: the only exception to the rule that panic is "only for programmer errors" |
| `float`, `double` and `NaN` | The C# way. The `==` and `<` operators are as in the hardware: `NaN == NaN` is false. `equals` and `compare` give a total order: `NaN` is equal to itself and is the smallest, and `sort`, `max`, `contains` work. Both are `Equatable` and `Comparable`, not `Hashable`: `HashMap<double, V>` is a compile error | Rust splits equality and order into "partial" and "total", and `sort` does not compile on a list of fractional numbers; that means two more concepts. Cost: the rule "`a == b` means `a.equals(b)`" does not hold only for fractional numbers. The ban on keys can be relaxed later |
| `shared` cycle | It cannot be made a compile error: a tree without cycles also contains itself in its type. In test and debug builds, `shared` objects still alive when the program ends are reported: the count, the line where they were created, the field through which they hold each other, and the hint `weak`. No report in release | Rust counts a leak as safe and does not report it, an outside tool is needed. Cost: in release a leak stays silent. Example: [cities and roads](05-examples.md) |
| `share` and `null` | `share x` gives `null` if `x` is `null`, whether it is `weak` or not: `share cur.parent`, `share map.get(id)` | So that `if x is null` does not have to be written everywhere. `null` still stays `null`, nothing is hidden |
| Identity and equality | `a == b` compares content on every type, on `shared T` too; if `T` is not `Equatable` it is a compile error, and the hint is `same`. `same(a, b)` is `true` if two `shared` values point to the same object (`ReferenceEquals` in C#, `Rc::ptr_eq` in Rust). For a structure that refers back to itself (`City.roads`) the compiler cannot generate `equals`, and whoever wants it writes it by hand | In C# `==` looks at identity on a class and at content on a `string`, and one has to know which. Two words for two questions |
| Collection names | `HashMap<K, V>`, `HashSet<T>` (the Rust names; C# says `HashSet` too). `List<T>` stays. The old spelling was `Map`, changed on 11 November | The target audience comes from Rust. The name says that order is not guaranteed, and when an ordered version arrives the difference can be read from the name. `Vec` says nothing to a C# reader, `List` is clear to both sides |
| List methods | `filter` and `map` run at once and return a new list. `filter` borrows the items: `List<read T>` | `filter` because the word `where` is taken, `map` for C#'s `Select` |
| Module | A folder is a module. There is no module declaration in a file, the name comes from the folder path (`shop.stock`). Files in the same folder are one module and see each other without writing `use` | The Go way. In Rust a file is a module and `mod stock;` is written in the parent file, in C# `namespace` is written in the file and can drift away from the folder. Since there is nothing to write, there is nothing to write wrongly. Cost: a folder is created to split a large module |
| Visibility | A name without `public` is visible inside the module. One rule for types, functions, fields and methods, two levels. Enum variants are visible together with the enum. A `public` field can be written from outside, and if it should only be read, `public readonly` | Rust's rule (inside the module if there is no `pub`). In C# a field is private to the type and a type is open to the project; because there are free functions, that distinction would need a third word here. Cost: another file in the same module can change the field |
| Consequences of visibility | A `public` function, field or alias cannot expose a type that is not `public`, and the compile error shows both lines. A struct with a field that is not visible cannot be built outside with `T { ... }`, the hint is `T(args)` | Not a new rule, it follows from the visibility rule. The first is an error in C# and a warning in Rust; the second is the same in Rust too |
| `use` | `use stock` brings in all the `public` names of the module directly. It is written at the top of the file. If two modules have the same name, it is a compile error where the name is used, and the hint is the full name: `billing.Line`. The full name is valid without `use` too. On an unknown name the hint writes the missing `use` line | The word is from Rust, the behaviour is C#'s `using`: Rust lists names one by one (`use stock::Warehouse`). `using` is already taken for type aliases. Cost: which module a name comes from cannot be read from the code, the editor shows it |
| `use` details | Modules can `use` each other mutually. An unused `use` is not an error, the editor shows it dimmed. The basic standard names need no `use`: `List`, `HashMap`, `HashSet`, `string`, `Result`, `File`, `Console`, `print` and the others that appear in the earlier examples | Mutual use is allowed in Rust and C# too, and a ban (Go) pushes towards splitting code artificially. An unused `use` produces no wrong result, and if it were an error the code would not compile while experimenting. The ready names are as in new C# projects; in Rust `HashMap` and `File` need `use`. The exact list of ready names will be settled with the standard library |
| Slice type | `read T[]` is a readable window, `ref T[]` a writable one: pointer + length. A bare `T[]` is not a type, compile error; the hint is `List<T>` for an owner and `read T[]` for a window. What `read string` is for text, `read byte[]` is for raw data. Conversion to a slice: `read xs` and `ref xs` give a slice where a slice is expected (`parse(read buffer)`), for `List<T>` and `T[N]`; passing it without writing `read` is a compile error, and the hint is `read xs`. `xs[..]` stays valid as range notation | Rust's `&[T]` and `&mut [T]` types, in our notation. No new name is needed, `read` and `ref` are enough. In C# `T[]` is an owning array on the heap, here it is not: the same name, a different meaning, and the error hint says so. Conversion to a slice is the same rule as `string` → `read string`, and in Rust `&buf` works this way too. The first decision was `xs[..]` only; it made people memorise a separate notation for lists, and it changed on 11 November |
| Range | In the `a..b` notation the ends can be left out: `data[..end]` from the start, `data[start..]` to the end, `data[..]` the whole. The same on `string`, `List<T>` and slices | The same as Rust and C#. `data[0..end]` and `line[i..line.length()]` were noise, and no new concept is needed |
| Text and bytes | `text.asBytes()` views text as `read byte[]`, for free. `string.fromUtf8(bytes)` views bytes as text: it checks the UTF-8, does not copy, and returns `Result<read string, TextError>` | The Rust names (`as_bytes`, `str::from_utf8`). C# has no path without a copy. The `string` guarantee is kept: there is no unchecked conversion. The copying `string.fromBytes` was removed on 11 November, and whoever wants a copy writes `.clone()`: one way, and the copy is visible in the code |
| Fixed-size array | `T[N]`, where `N` is an integer constant: `int[8] values`. It is a value and stays where it is (on the stack or inside the struct), no heap. `length()` is known at compile time. `xs[i]` is bounds checked; if the index is known at compile time and out of bounds it is a compile error, and if it depends on data it panics. Construction: `int[3] { 1, 2, 3 }` writes the items (a compile error if the count does not match), `byte[16](255)` puts the same value in every item (`T` must be `Copyable`), and `byte[16]` alone fills with zero; a zero value exists only for numbers, `bool` (`false`) and `T?` (`null`), and for any other type it is a compile error. There is no uninitialised memory. If `T` is `Copyable` the array is `Copyable` too, otherwise it is moved (the struct rule). Conversion to a slice is in the "Slice type" row | Rust's `[T; N]` type, in our notation. C# has no natural counterpart (`stackalloc` only in locals, `fixed` only in `unsafe`). Without this every buffer goes to the heap through `List`, and C-level performance does not hold. For construction Rust writes `[1, 2, 3]` and `[255; 16]` and has no short form for zero; in C# `new byte[16]` is always zero, and the short form comes from there. Cost: `T[]` (a slice, needs `read`/`ref`) and `T[N]` (a value) look alike; a large array passed by value is copied silently; the cost of zeroing |
| Test | `test "name" { }`, a block written at the top level of a file. Only the test command compiles and runs it, it is not part of a normal build. `test` is a keyword only at the top level and when followed by a string, and it can be used as a name. A test belongs to the module of the folder it is written in: it sees every name of the module, and only the `public` names of other modules. In the block `try` works with every error type; an error fails the test and is printed (it must be `Formattable`, as for `main`). A failing test is a panic, and the test command runs each test in a separate process | Zig's notation. In Rust `#[test] fn` and `#[cfg(test)] mod tests`, in C# a separate project and a library, in Go a naming rule (a `_test` file, a function starting with `Test`). A separate `main` program cannot see what is not `public` and stops at the first error. Go's naming rule adds no word but makes people memorise two invisible rules, and a test name cannot be a sentence. The visibility follows from the module rule, it is not a new rule. Cost: one word; the only place where `try` works without a target error type being written; the time to start a process per test (a tooling decision) |
| `assert` | `assert(condition)`: panics if it is false. The message prints the text of the expression and, if it is a comparison, the values of both sides (if they are `Formattable`). Valid outside tests too, and on in release too. There is no `assertEqual` | A single form. In Rust three macros (`assert!`, `assert_eq!`, `debug_assert!`), in C# `Assert.Equal` is in a library and `Debug.Assert` is deleted in release. In the same direction as the rule that safety checks cannot be turned off. Cost: since there are no macros the compiler knows `assert`, like `$"..."` |
| End of line | No semicolons, the end of a line ends the statement. A statement continues on the next line in these cases: inside `(` or `[`; if the line ends with an operator, a comma, a dot or an opening parenthesis; if the next line starts with `.` (a method chain). A line cannot start with an operator, and the hint says to move the operator to the end of the previous line | Go's rule + Python's parenthesis rule; the leading `.` is for the chain notation of C# and Rust. A leading operator creates ambiguity (`- x`), a leading dot does not. Approved on 12 November together with the interpreter |
| Place of `{` and `else` | `{` at the end of the line, `else` on the same line as `}`: `if x {`, `} else {`. Writing them on a new line is a compile error, and the hint shows the right place | The Go way: a single form. C#'s usual layout (`{` on a new line) was deliberately not taken, there would be two ways to write the same thing |
| Statement | Only a call, an assignment and a declaration are written as a statement; `a + 2` alone is a compile error. A name at the start of a statement, followed by `*` and `?`, followed by a second name, is a declaration: `int x = 1`, `Node*? list = null` | The C# rule (CS0201). This way the distinction between `Node* n` and `a * b` is resolved with lookahead: `a * b` cannot be a statement anyway |
| Struct construction in a condition | In the head of an `if`, `while` and `for`, `{` starts the block; struct construction there needs parentheses: `if p == (Point { x = 1 }) {`. Everywhere else without parentheses | Rust's rule. `if x == Node { ... } {` cannot be told apart otherwise. Cost: two parentheses in a rare case |
| Assigning to a `ref` and a `read` variable | Assigning to a `ref` variable writes to what is borrowed: `void clear(ref int? slot) { slot = null }`. Assigning to a `read` variable rebinds the variable: `cur = read cur.next` | This is how a `ref` parameter works in C#, and no separate "write into it" operator (`*x = ...`) is needed. Since nothing can be written through a `read`, the only meaning of assigning to it is rebinding |
| Looking does not move | The two sides of an operator, a method receiver and the arguments of `print`, `assert` and `$"..."` are borrowed only for that moment: `name` can be used after `print(name)`. A value is moved when it is given to a variable, a field, a parameter or a `return` | `a == b` already means `a.equals(b)` and takes `read`; `print` too reads the `Formattable`. In Rust `println!` gives the same result because it is a macro |
| Language name | SG. Source files end in `.sg` | The author's initials; short, and free as an extension. Decided on 12 November 2026 |
| Method names `replace` and `take` | A struct cannot define a method named `replace`. `take` can be defined | `x.replace(value)` exists on every type, so a method of that name would clash with it. The built-in `take` exists only on `T?`, and a method is never called on a `T?`. Approved on 12 November with the interpreter's third slice |
| Name clashes | A method and a field of the same struct cannot share a name. A function cannot have the name of a type | A name on a struct should mean one thing. `Stack(...)` always means `Stack.create(...)`. Approved on 12 November |
| `Name.f()` | A variable called `Name` wins over a type called `Name` | The nearer name wins, as everywhere else. Approved on 12 November |
| `take` and narrowing | `take` on a variable known not to be null ends that knowledge: the variable is `T?` again | `take` puts `null` in its place. Approved on 12 November |
| `ref this` on a new value | A `ref this` method can be called on a value that was just built: `Stack().push(1)` | The same as Rust. No variable is needed just to make the call. Approved on 12 November |
| `this` by value through a `T*` | A method that takes `this` by value, called through a `T*`, uses the pointer up | Taking `this` by value moves the value, and the pointer is its owner. Approved on 12 November |
| Moving a field out of an owned value | A field can be moved out of an owned value. Moves are tracked per field: the other fields stay usable, and the value as a whole is unusable until the field is assigned again | Follows from the `take`/`replace` rule, which restricts only a move through a `ref`: an owner may give up a part. Implemented in the interpreter's third slice on 12 November |

## Deliberately deferred

- **comptime:** later.
- **Not in the first version:** async, macros.
- **After the first version:** implementing an interface outside the definition of the type (`extend`), using an interface
  as a type (`read Shape`), associated types, default method bodies, operator definitions, constant parameters, nested patterns
  (`Add(Number(a), b)`), a variable that `is not` opens into the outer scope, a format specifier inside `$"..."` (`{x:N2}`), a `sync` that pushes the data to disk after `close` (`Flush(true)` in C#), generic type aliases (`using Link<T> = shared checked T`), lazy list chains (like LINQ).

## Open questions

- Notation. The [C# variant](02-syntax-draft.md) is the current draft, and it will be stressed with example programs.
- Library boundary: in a library with more than one folder there is no level for "my own folders can see it, the user cannot"
  (`internal` in C#, `pub(crate)` in Rust). To be handled together with the package system.
- Stack overflow on deep structures: freeing a long linked list is recursive. For types that contain themselves the compiler could turn freeing into a loop,
  and the overflow could be a readable panic. Not tested.
- Inside `Func<read T, U>`, `U` cannot depend on the argument, so `orders.map(o => read o.customer)` does not compile and `.clone()` is needed. Whether this has to be overcome has not been tested.
- Type-first grammar without semicolons: the distinction between `Node* n` and `a * b`, and between `Node? x` and `a ? b : c`,
  needs lookahead. The end-of-line rule must also be settled at the interpreter stage. A `T { ... }` initialiser cannot be told apart from a block
  inside a condition (`if x == Node { ... } {`); parentheses could be required there, as in Rust.
  *12 November 2026:* the end-of-line rule, the place of `{`/`else` and the statement rule were approved and are in the decisions table. The `Node* n` and `Node? x`
  distinction was resolved by the statement rule. `T { ... }` inside a condition was also decided the same day ("Struct construction in a condition").
  This item is closed.

## Performance note

Notation does not affect the machine code: `read`, `ref`, `from` and type names disappear at compile time. Decisions about meaning do.
None of this has been measured, and what follows are expectations.

| Effect | Decision | Compared to Rust |
|---|---|---|
| Same | `T*`, `read`, `ref`, `T?`, `enum`, `match`, `Result`, `try`, monomorphization, bounds checks, `locked` | The same rules, the same optimisation information |
| Better | Panic does not unwind | Like Rust's `panic=abort` setting: no cleanup path is generated, and the binary gets smaller |
| Worse, on purpose | Overflow check is on in release | Rust does not check in release. There is a branch on every operation, and the real loss is in loops that vectorise. `wrappingAdd` recovers this loss |
| Worse | `shared` is always atomic | More expensive than `Rc`, only on `share` and when an owner lets go |
| Worse | A check on captures that a lambda writes to | Rust's `FnMut` is free. Only a lambda that writes pays, and for a lambda passed directly the compiler can remove the check |

This goal holds only with a back end at the level of LLVM. A tree-walking interpreter is for testing the language and gives no performance.

## Next steps

1. Reach 10-15 example programs in total, and compare the notation across 2-3 variants. Written so far: calculator, stack, threads, `List`, log
   summary, folder tree, order rules, cities and roads (graph), stock and invoice (modules, `use`), HTTP request (byte slice), sensor window (fixed-size array), stock tests (`test`, `assert`; approved on 12 November). The next example has not been chosen.
2. A tree-walking interpreter in Rust, along the path of Crafting Interpreters. Details:
   [roadmap](03-roadmap.md). Started on 12 November, the code is in the `interpreter/` folder. The first slice works: functions,
   `int`/`long`/`nint`/`bool`/`string`, `if`/`while`/`for`, `print`, `assert`, `$"..."`, overflow and conversion panics.
   Second slice (the same day): structs, `T*` and `new`, `T?` and `null` (`is null`, `??`, flow narrowing), `read`/`ref`,
   static type checking and the check for moved variables; the linked list in the syntax draft runs unchanged.
   Third slice (the same day): methods (`this`, `read this`, `ref this`), calls through the type and `T(args)`,
   `take`/`replace`, moving a field out of an owned value (tracked per field); the stack example runs (until `try` arrives,
   with `if node is null { return null }` inside `pop`). The six small choices made in this slice were approved on 12 November 2026 and are in the decisions table:
   [interpreter/README.md](../interpreter/README.md), "Rules from the third slice".
   The borrow check (`from`, conflicting `ref`) does not exist yet. Next: `enum` and `match`, then `Result` and `try`. The current state is in
   [interpreter/README.md](../interpreter/README.md). The code, comments and error messages are in English. Since 12 November 2026 the design documents are in English too.

## Dropped decisions

These no longer hold and are here for the record. Apart from the data race and recoverable error items, they were dropped on 5 November
together with the generation model and the arena:

- Run-time generation checks for use-after-free (generational references).
- An 8-byte pointer whose top 16 bits are the generation. The decision on generation width (16 bits, mask, no TBI) had been made
  the same day, and it lost its subject when the model was dropped.
- Retiring a slot that overflows.
- A slab allocator and a standard arena API as part of the language.
- `ref` being a second-class parameter mode that is only passed downwards.
- Data races being out of scope in the first version. With threads present, a non-atomic counter leads to a double free,
  so the subject touched memory safety; the thread rule is in the decisions table.
- Settling for escape analysis and avoiding lifetime notation altogether.
- Recoverable errors being left out because they need unwinding. The reasoning was wrong: an error returned as a value
  needs no unwinding (6 November).

Removed on 11 November to make the language smaller. All of them can be added back later, and it is hard to remove something once it has been added:

| Removed | Instead | Cost |
|---|---|---|
| The heavy version of `Closable`: tracking the value everywhere, spreading to structs, the ban on generics and `shared`, `IgnoreClose`, the deferred `try using var` | The version that looks only at local variables, "Close error" in the decisions table | If a value moved into a field or a collection is dropped without being closed, the compiler does not see it |
| Automatic `Ok` | `return Ok(value)`, `return Ok()` | A few characters on every success return |
| The "assign back on every exit" check when moving a field through a `ref` | Always `take` or `replace` | Negligible, Rust is the same |
| A block that produces a value on its own, `var x = { ... }` | The variable is declared and assigned inside `if/else`. The block in a `match` arm stays | A few lines |
| `ThreadFunc`, `shared Func` | Thread work starts with `spawn f(args)` | A work queue that takes lambdas (a thread pool) cannot be written in user code |
| `string.fromBytes` | `string.fromUtf8(bytes)`, `.clone()` for a copy | None |

## Notation mapping

The C# direction was chosen on 5 November. The conversation texts (`topics/`, the memory model and the notes below) are still
in the old notation:

| Old | New |
|---|---|
| `*T` | `T*` |
| `?*T` | `T*?` |
| `&T`, `&x` | `ref T`, `ref x` |
| `while cur \|n\|` | `while cur is not null` |
| `orelse` | `??` |
| `fn f(a: T) -> U` | `U f(T a)` |
| `a: T` (field) | `T a` |
| `Node { value: v }` | `Node { value = v }` |
| `i32` | `int` |

## Notes added during the transfer

This section was not in the 4 November conversation; it was noticed on 5 November while transferring to files. The conversation texts
were not changed, and the corrections are kept here.

### Gaps in the design

The memory model gaps in this section lost their subject when the generation model was dropped, and they are kept for the record.

- **An arena cannot be invalidated "in a single operation".** The model both keeps the generation in the slot header and says "when the arena
  dies all slot generations increase, a single operation". With a generation per slot, the work is proportional
  to the number of slots. For a truly single operation the generation must be at arena level, and then where the check
  looks, with the same pointer layout, has to be solved again.
- **ARM TBI does not cover 16 bits.** Solved: 16 bits and a mask were chosen, the reason is in the decisions table.
  The sentence "on ARM, TBI gives the mask for free" in the memory model does not hold.
- **Retired memory is not negligible.** The memory model says "the memory loss is negligible". With a 64-byte
  slot and 1 million alloc/free per second, at 16 bits that makes about 84 MB a day and about 30 GB a year
  (a calculation, not a measurement). The physical memory of a fully retired page must be given back to the operating
  system. The loss then shifts to the virtual address space, but a single long-lived object left on a page prevents
  the page from being given back.
- **If an empty page is given to another size class, generations can match by chance.** The slot boundaries shift, and an old
  pointer lands on a new slot. New generations must start above the highest old generation on the page.
- **What happens if `&` is taken to a field on the heap?** The model defines `&T` for local variables.
  `&n.value` is undefined. If this expression produces an unchecked `&T`, use-after-free comes back when the function it is
  passed down to frees the object. It looks as if a reference to a heap field has to produce a generation-checked
  `*T`.
- **2-10 percent is not this design's figure.** That measurement is for Vale's layout. Here, on every dereference,
  the slot is additionally computed from the page header and the generation is read from the header. The claim "cache behaviour is the same as
  C" also holds only for pointer size. The cost must be counted as unknown until it is measured.

### Mistakes in the conversation

- [Safe C attempts](../topics/06-safe-c-attempts.md): the statement "all six threw out macros" is
  wrong. The same text says that C3 added macros; and Checked C, being an extension of C,
  carries its preprocessor.
- [Go](../topics/05-go.md): `defer` was listed as an example of breaking Zig's principle, yet Zig
  has `defer` too. `panic`/`recover` and GC pauses should remain as the examples.
- [Roadmap](03-roadmap.md): "Rust belts" is not Niko Matsakis's but the RustBelt project of
  Ralf Jung and his team: https://plv.mpi-sws.org/rustbelt/
- [Zig](../topics/03-zig.md): `GeneralPurposeAllocator` was renamed `DebugAllocator` with 0.14.

### Reading list links

20 links were opened on 5 November. Two of them could not be verified by the automatic check and should be looked at in a browser:
Graydon's post (504) and the MIT Press page (403). Whether the error is permanent is not known. The information that
development of Vale has stopped comes from a secondary source and was not verified.
