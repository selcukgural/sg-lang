# From Rust to SG

What a Rust programmer writes, and what SG writes for it. SG keeps Rust's promise (no use after free, no
data race, no `null` that was not asked for) with C#'s way of writing, and leaves out what costs more
complexity than it gives (closures, generics of your own, macros, lifetimes in the signature).

The **Status** column says how far each line is:

- **built**: runs in the interpreter and is compiled to C,
- **interpreter**: runs in the interpreter, compiled code is still to come,
- **designed**: decided in the design, not built yet,
- **not in SG**: left out on purpose; the note says what to write instead.

The details of each line are in the [decisions file](04-decisions-and-open-questions.md) and in the
[interpreter README](../interpreter/README.md). This file is brought up to date with every slice of the
language, in the same commit.

Last brought up to date: 14 November 2026, the fifty-seventh slice.

## Functions and variables

| Rust | SG | Status | Note |
|---|---|---|---|
| `fn add(a: i32, b: i32) -> i32 { a + b }` | `int add(int a, int b) { return a + b }` | built | The type comes first, as in C#; the last line is not the value, `return` is written |
| `fn main()` | `void main()` | built | |
| `fn main() -> Result<(), E>` | `Result<void, E> main()` | built | The error it ends with is printed (`E : Formattable`), exit code 1 |
| `std::env::args()` | `void main(List<string> args)` | built | The words after the program's name, checked to be UTF-8 |
| `let x = 1;` | `var x = 1` | built | No `;` at the end of a line |
| `let x: i64 = 1;` | `long x = 1` | built | |
| `let mut x = 1;` | `var x = 1` | built | Every variable can be given a new value; a field can be `readonly` |
| `type Id = u64;` | `using Id = ulong` | built | `public using` from a module |
| `std::process::exit(2)` | `exit(2)` | built | Takes a `byte`, so `exit(256)` is a compile error |
| `panic!("...")` | `panic("...")` | built | |
| `assert!(a == b)`, `assert_eq!(a, b)` | `assert(a == b)` | built | The message shows the expression and both sides |
| `println!("{}", x)` | `print(x)` | built | |
| `eprintln!("{}", x)` | `printError(x)` | built | |
| closures `\|x\| x + 1` | a named function | not in SG | A `Func` is the address of a named function |
| macros (`macro_rules!`, `#[derive]`) | none | not in SG | What `#[derive]` gives is an interface after `:` (below) |

## Numbers

| Rust | SG | Status | Note |
|---|---|---|---|
| `i8` `u8` `i16` `u16` `i32` `u32` `i64` `u64` | `sbyte` `byte` `short` `ushort` `int` `uint` `long` `ulong` | built | |
| `isize`, `usize` | `nint`, `nuint` | built | An index and a length are `nint` |
| `f32`, `f64` | `float`, `double` | built | |
| overflow: panic in debug, wrap in release | the same | built | Rust's rule, also for shifts and casts |
| `x as i32` (cuts the bits) | `(int)x` (panics if it does not fit) | built | `wrappingToInt()` cuts the bits, `tryToInt()` gives `null` |
| `i32::try_from(x).ok()` | `x.tryToInt()` | built | `int?` |
| `a.wrapping_add(b)` | `a.wrappingAdd(b)` | built | |
| `a.checked_add(b)` | `a.tryAdd(b)` | built | `int?` |
| `i32::MAX`, `f64::NAN`, `std::f64::consts::PI` | `int.MaxValue`, `double.NaN`, `double.Pi` | built | |
| `"42".parse::<i32>()` | `int.parse(text)` | built | `Result<int, NumberError>`; the error says where the text stops being a number |
| `i32::from_str_radix(s, 16)` | `int.parseHex(s)` | built | |
| `u64::from_le_bytes(b)` | `ulong.fromLittleEndian(bytes)` | built | Takes a slice of exactly 8 bytes; another width is a compile error where it shows |
| `n.to_le_bytes()` | `n.toLittleEndian()` | built | A `byte[8]` |
| `x.sqrt()`, `x.powf(y)`, `a.min(b)` | `x.sqrt()`, `x.pow(y)`, `a.min(b)` | built | |
| `0xff`, `0b1010`, `1_000_000` | the same | built | |
| `i32 + i64` (an error) | `int + long` is a `long` | built | An integer widens on its own only to a type that holds every value of it; `int + uint` is an error |

## Text

| Rust | SG | Status | Note |
|---|---|---|---|
| `String` | `string` | built | Owns its bytes, always UTF-8 |
| `&str` | `read string` | built | |
| `format!("{a}: {b}")` | `$"{a}: {b}"` | built | A formatted text; `.toString()` makes it a `string` |
| `{:>5}`, `{:<10}`, `{:02X}`, `{:08b}`, `{:.2}` | `{n,5}`, `{name,-10}`, `{b:X2}`, `{mask:b8}`, `{x:F2}` | built | C#'s widths and letters |
| `impl Display for T` | `T : Formattable` with `void format(read this, ref Formatter f)` | built | Without the method the text is made from the fields |
| `String::with_capacity(n)` | `string.withCapacity(n)` | built | |
| `s.push_str(x)`, `write!(s, ...)` | `s.write(x)` | built | Takes whatever `print` takes; `+=` on text is an error whose hint is `write` |
| `s.len()` | `s.length()` | built | In bytes |
| `s.as_bytes()` | `s.asBytes()` | built | `read byte[]` |
| `std::str::from_utf8(b)` | `string.fromUtf8(bytes)` | built | `Result<read string, TextError>` |
| `&s[a..b]` | `s[a..b]` | built | `read string` |
| `s.as_bytes()[i]` | `s[i]` | built | One byte |
| `s.chars()`, `s.split(',')`, `s.split_whitespace()`, `s.lines()` | the same, in camelCase | built | Parts look at the text, nothing copied |
| `s.trim()`, `s.find(x)`, `s.rfind(x)`, `s.contains(x)`, `s.starts_with(x)` | `s.trim()`, `s.indexOf(x)`, `s.lastIndexOf(x)`, `s.contains(x)`, `s.startsWith(x)` | built | `indexOf` gives a `nint?` |
| `s.replace(a, b)`, `s.repeat(n)` | `s.replaceAll(a, b)`, `s.repeat(n)` | built | `replace` is taken by `slot.replace(value)` |
| `s.eq_ignore_ascii_case(t)` | `s.equalsIgnoreAsciiCase(t)` | built | |
| `names.join(", ")` | `string.join(", ", read names)` | built | |
| `char` | `char` | built | `'+'` is a `byte` where an integer goes: Rust's `b'+'` |
| `s.to_uppercase()` | none yet | designed | Needs Unicode tables; `toAsciiUpper()` on a `char` is there |

## Collections

| Rust | SG | Status | Note |
|---|---|---|---|
| `Vec<T>` | `List<T>` | built | |
| `Vec::new()`, `vec![1, 2]` | `List<int>()`, `List<int> { 1, 2 }` | built | |
| `v.push(x)`, `v.pop()`, `v.len()` | `xs.add(x)`, `xs.removeLast()`, `xs.length()` | built | `removeLast` gives a `T?` |
| `v[i]`, `v.get(i)` | `xs[i]`, `xs.get(i)` | built | `get` gives a `T?` |
| `v.extend_from_slice(&s)` | `xs.addAll(s)` | built | For `Copyable` items |
| `v.sort()`, `v.reverse()`, `v.contains(&x)`, `v.iter().position(...)` | `xs.sort()`, `xs.reverse()`, `xs.contains(x)`, `xs.indexOf(x)` | built | `sort` is stable |
| `[i32; 3]`, `[0u8; 16]`, `[255u8; 16]` | `int[3]`, `byte[16]`, `byte[16](255)` | built | |
| `&[T]`, `&mut [T]` | `read T[]`, `ref T[]` | built | `read xs` gives one where a slice is expected |
| `HashMap<K, V>` | `HashMap<K, V>` | built | A hash table with a random key (SipHash), no order |
| `m.insert(k, v)` (on a new key) | `map.add(k, v)` | built | A key that is there already is a panic; `map[k] = v` changes it |
| `m.get(&k)`, `m[&k]` | `map.get(k)`, `map[k]` | built | `get` gives a `V?` |
| `*m.entry(k).or_insert(0) += 1` | `var count = map.getOrAdd(k, 0)`, then `count += 1` | built | `getOrAdd` gives a `ref V` |
| `m.contains_key(&k)`, `m.remove(&k)` | `map.contains(k)`, `map.remove(k)` | built | |
| `HashSet<T>`, `s.insert(x)` | `HashSet<T>`, `set.add(x)` | built | `add` gives `true` if the item was not there |
| `for x in v` / `&v` / `&mut v` | `for x in xs` / `read xs` / `ref xs` | built | |
| `for (k, v) in &m` | `for e in read map` with `e.key`, `e.value` | built | |
| `v.iter().map(...).filter(...)` | a `for` loop | not in SG | There are no closures |

## Structs, enums and methods

| Rust | SG | Status | Note |
|---|---|---|---|
| `struct P { x: i32 }` | `struct P { int x }` | built | One field per line |
| `P { x: 1 }` | `P { x = 1 }` | built | |
| `impl P { fn len(&self) -> i32 }` | `int len(read this)` in the struct body | built | Fields are `this.x` |
| `&mut self`, `self` | `ref this`, `this` | built | |
| `P::new(1)` | `P.create(1)`, or `P(1)` for short | built | A function without `this` is called on the type |
| `enum Shape { Circle(i32) }` | `enum Shape { Circle(int radius) }` | built | What a variant carries has a name |
| `Shape::Circle(2)` | `Shape.Circle(2)` | built | |
| `#[derive(Clone, Copy)]` | `struct P : Copyable` | built | |
| `#[derive(PartialEq)]` | `: Equatable` | built | Or its own `bool equals(read this, read P other)` |
| `#[derive(Hash)]` | `: Hashable` | built | Can be a key of a `HashMap` |
| `impl Ord` | `: Comparable` with `int compare(read this, read P other)` | built | Gives `<` and `sort` |
| `#[derive(Debug)]` | `: Formattable` | built | Written from the fields |
| `trait`, `impl Trait for T` | `interface` | designed | Interfaces of your own are put off; the five built-in ones are there |
| generics of your own (`struct Stack<T>`) | none | designed | Put off; `List`, `HashMap`, `HashSet`, `Result`, `Func`, `Task` are generic |
| `impl Drop` | `Disposable` | designed | |

## `match`, `if let` and `Option`

| Rust | SG | Status | Note |
|---|---|---|---|
| `match x { A(n) if n > 0 => ..., _ => ... }` | `match x { A(n) when n > 0 => ..., _ => ... }` | built | One arm per line |
| `A \| B` | `A or B` | built | |
| `'a'..='z'` | `'a'..='z'` | built | |
| `n @ A(_)` | none | not in SG | Not decided yet; listed in the interpreter README under "Not here yet" |
| `if let Some(c) = x.circle()` | `if x is Circle(r)` | built | The names reach along `&&` |
| `let Ok(v) = r else { return };` | `if r is not Ok(v) { return }`, then `v` | built | |
| `Option<T>` | `T?` | built | `T` itself is never `null` |
| `None`, `Some(x)` | `null`, `x` | built | |
| `if let Some(v) = x` | `if x is not null` | built | `x` is a `T` inside |
| `x.unwrap_or(d)` | `x ?? d` | built | |
| `x.expect("...")` | `x ?? panic("...")` | built | |
| `x?` on an `Option` | `try x` | built | |
| `x.take()`, `std::mem::replace(&mut x, v)` | `x.take()`, `x.replace(v)` | built | |

## Errors

| Rust | SG | Status | Note |
|---|---|---|---|
| `Result<T, E>`, `Ok(v)`, `Err(e)` | `Result<T, E>`, `Ok(v)`, `Error(e)` | built | `Result<void, E>` and `Ok()` for nothing |
| `x?` | `try x` | built | |
| `?` with `From` | `try x` | built | Wraps the error in the one variant of the caller's error type that carries it |
| `#[must_use]` on `Result` | always | built | A `Result` nobody looks at is an error |
| `let _ = f();` | `_ = f()` | built | |
| `r.unwrap_or(d)` | `r ?? d` | built | |
| `.unwrap()` | `?? panic("...")` | built | The reason is written |

## Ownership and borrowing

| Rust | SG | Status | Note |
|---|---|---|---|
| move on assignment and call | the same | built | A use after a move is an error that shows both places |
| `x.clone()` | `x.clone()` | built | |
| `&x`, `&mut x` | `read x`, `ref x` | built | Written at the call: `f(read x)` |
| `&T`, `&mut T` | `read T`, `ref T` | built | |
| `'a`, `fn f<'a>(a: &'a str) -> &'a str` | `read string f(read string a) from a` | built | Only a returned borrow names where it comes from |
| `struct Token<'a> { text: &'a str }` | `struct Token<from source> { read string text }` | built | |
| non-lexical lifetimes | a borrow lasts until its last use | built | |
| `Box<T>`, `Box::new(x)` | `T*`, `new x` | built | |
| `Option<Box<T>>` | `T*?` | built | |
| `Rc<T>`, `Arc<T>` | `shared T` | built | One type; the count is always atomic. What it holds is only read |
| `Rc::new(x)`, `Rc::clone(&a)` | `new shared x`, `share a` (or `a.clone()`) | built | |
| `Weak<T>`, `Rc::downgrade(&a)`, `w.upgrade()` | `weak T`, `weak a`, `share w` | built | `share w` gives a `shared T?` |
| `Rc::ptr_eq(&a, &b)` | `same(a, b)` | built | `==` compares what they point to |
| `Rc<RefCell<T>>` | `shared checked T` | built | Each access checked; a certain conflict is a compile error, another a panic with both lines |
| `a.borrow()`, `a.borrow_mut()` | nothing (each access), or `read a as n { }`, `ref a as n { }` | built | |
| a cycle of `Rc` leaks without a word | reported at the end of a program or a test | built | A debug build and the tests; the hint names the field to make `weak` |
| `RefCell<T>` alone, `Cell<T>` | `checked T` | designed | For now `checked` comes after `shared` or `weak` |
| `*const T`, `*mut T`, `unsafe { }` | `raw T`, `unsafe { }` | designed | |

## Tasks and locks

| Rust | SG | Status | Note |
|---|---|---|---|
| `Arc::new(Mutex::new(v))` on a value you already have | `new shared locked v` | interpreter | `v` is moved in; a value is locked from where it is made, never later in place |
| `thread::spawn(move \|\| work(x))` | `Task.run(work, x)` | interpreter | The function and what it is given are written apart |
| `JoinHandle<R>` | `Task<R>` | interpreter | |
| `h.join().unwrap()` | `t.wait()` | interpreter | A panic stops the process, so there is nothing to unwrap |
| dropping a `JoinHandle` (the thread goes on) | a compile error | interpreter | Every task is waited for on every path |
| `Send`, `Sync` | the thread rule | interpreter | No borrow, no `shared checked`, no `Task` crosses, also inside fields |
| `Arc<Mutex<T>>` | `shared locked T` | interpreter | |
| `let mut c = m.lock().unwrap(); *c += 1;` | `lock m as c { c += 1 }` | interpreter | What it holds is reached only inside the block |
| two locks, one after the other | `lock a as x, b as y { }` | interpreter | Taken in a fixed order; a lock inside a lock is an error |
| a lock held across a wait | a compile error | interpreter | Nothing in a `lock` block waits, so locks and waits cannot deadlock |
| lock poisoning | none | interpreter | A panic stops the process |
| `static`, `thread_local!` | none | not in SG | No global variables: what tasks share is in the arguments of `Task.run` |
| `async fn`, `.await`, tokio | `async`, `await t`, one built-in runtime | designed | C#'s behaviour, compiled as Rust compiles it ("Async, the direction") |

## Modules

| Rust | SG | Status | Note |
|---|---|---|---|
| `mod stock;` and a file | a folder `stock/` | built | The files of a folder see each other |
| `use stock::*` | `use stock` | built | Brings in the `public` names |
| `use shop::stock` | `use shop.stock` | built | |
| `stock::total(&xs)` | `stock.total(read xs)` | built | The full name works without `use` |
| `pub` | `public` | built | On a type, a function, a field, a method |
| `pub(crate)` | none | not in SG | A name is seen in its module or everywhere |

## Files and input

| Rust | SG | Status | Note |
|---|---|---|---|
| `fs::read_to_string(p)` | `File.readText(p)` | built | `Result<string, IoError>` |
| `fs::read(p)`, `fs::write(p, b)` | `File.readBytes(p)`, `File.writeBytes(p, b)` | built | |
| `BufReader::new(File::open(p)?)` | `File.openText(p)` | built | A `StreamReader` |
| `reader.read_line(&mut s)`, `reader.lines()` | `reader.readLine(ref line)`, `reader.lines()` | built | |
| `BufWriter::new(File::create(p)?)` | `File.createText(p)` | built | A `StreamWriter` |
| `writeln!(w, "{}", x)?` | `try w.writeLine(x)` | built | |
| a `BufWriter` dropped (its error lost) | `try w.close()` | built | A writer not closed on every path is a compile error |
| `io::stdin()`, `io::stdout()` | `Stdin`, `Stdout`, `Stderr` | built | |

## Tests and the tools

| Rust | SG | Status | Note |
|---|---|---|---|
| `#[test] fn name()` | `test "name" { }` | built | At the top level of the file, next to the code |
| `cargo test` | `sg test file.sg` | built | `sg test --compiled` runs them compiled |
| `cargo run` | `sg file.sg` | built | The interpreter |
| `cargo build`, `cargo build --release` | `sg build file.sg`, `sg build file.sg --release` | built | A debug build checks overflow and reports leaks |
| a test that leaks `Rc` passes | the test fails with the leak report | built | |
| Cargo, crates.io | none yet | designed | No package manager yet |
