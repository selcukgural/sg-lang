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

Last brought up to date: 16 November 2026, the eighty-sixth slice.

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
| the `unused_variables` lint (a warning) | nothing to write | built | A local variable nothing reads is a compile error: `var x = 1` alone, a variable only written (`x += 1`, `p.x = 0` on a copy), the name of a `for` or of a pattern. A write through `ref`, `shared` or raw memory is a use; a parameter and a `Disposable` value are not looked at |
| `let _guard = ...;` | `var guard = ...` | built | A variable of a `Disposable` type is kept for its end without being read; SG has no `_name` |
| `for _ in 0..n`, `Some(_)` | `for _ in 0..n`, `Ok(_)` | built | |
| the `unused_assignments` lint (`x = 2` never read after) | nothing | not in SG | Not looked at yet: a value given after the last read, or written over before it is read, passes; noted for later |
| `type Id = u64;` | `using Id = ulong` | built | `public using` from a module |
| `std::process::exit(2)` | `exit(2)` | built | Takes a `byte`, so `exit(256)` is a compile error |
| `panic!("...")` | `panic("...")` | built | |
| `#[track_caller]` on the standard library | nothing to write | built | A panic inside the language's own module (`r.nextInt(5, 5)`, `Duration.fromSeconds` of a count too large) is shown at the call of the program, without the hint about the module's code; a function of your own cannot ask for it |
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
| `println!("{}", x)` on an `f64` | `print(x)` | built | The same fewest digits, laid out as JavaScript does (`1e+21`, `2` for `2.0`); in a tie between two numbers of the fewest digits Rust takes the larger, SG the even one (`160057272853489.62`, Rust `...63`). Compiled with Dragonbox, as the `ryu` crate is used for speed in Rust |
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
| `s.to_uppercase()`, `s.to_lowercase()` | `s.toUpper()`, `s.toLower()` | built | The same full mappings and final sigma, from SG's own tables of Unicode 17.0 |
| `c.to_uppercase()` (an iterator) | `c.toUpper()` (a `char`) | built | One character for one: `'ß'.toUpper()` is `'ß'`; the text method gives `"SS"` |
| `c.is_alphabetic()`, `c.is_numeric()`, `c.is_alphanumeric()` | `c.isLetter()`, `c.isNumeric()`, `c.isAlphanumeric()` | built | `isLetter` is the category L, narrower than Alphabetic |
| `c.is_uppercase()`, `c.is_lowercase()`, `c.is_control()` | `c.isUpper()`, `c.isLower()`, `c.isControl()` | built | The categories Lu and Ll, narrower than Rust's derived properties |
| none (the `caseless` or `unicase` crates) | `s.equalsIgnoreCase(t)`, `s.foldCase()` | built | Full case folding, without the Turkic lines |
| the `unicode-normalization` and `unicode-segmentation` crates | none | not planned yet | |

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
| `for x in f()? { }` | `for x in try f() { }` | built | |
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
| `trait`, `impl Trait for T` | `interface` | designed | Interfaces of your own are put off; the seven built-in ones are there |
| generics of your own (`struct Stack<T>`) | none | designed | Put off; `List`, `HashMap`, `HashSet`, `Result`, `Func`, `Task` are generic |
| `impl Drop for T { fn drop(&mut self) }` | `struct T : Disposable` with `void dispose(ref this)` | built | Runs once where the last owner ends, then the fields end; not on a panic or `exit`. Only on a struct for now |
| `drop(x)`, `x.drop()` | none: hand it to a function that takes it by value | built | `x.dispose()` is an error; it runs by itself |
| moving a field out of a `Drop` type (E0509) | an error too | built | `take` and `replace` work, as `mem::take` and `mem::replace` do |
| a `close(self) -> Result` that Rust lets you forget | `struct T : Closable` with `Result<void, E> close(this)` | built | A local that is not closed is a compile error, as for a `StreamWriter` |

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
| `#[must_use]` on a function | nothing to write | built | The value of every call is an error when nobody uses it (`xs.removeLast()`, `try cmd.status()`, a function of your own alone); `_ = f()` drops it, as Rust's `let _ =` |
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
| `*const T`, `*mut T`, `unsafe { }` | `raw T`, `unsafe { }`, `unsafe` in front of a value | built | A `raw T` points at plain data (numbers, `bool`, `char`, raw pointers, arrays and structs of them) or is a `raw void`; it may be `null`. An `unsafe` that nothing needs is an error |
| `*p`, `*p.add(i)`, `(*p).x` | `p[0]`, `p[i]`, `p[i].x` | built | Inside `unsafe`; reading copies the item. A place in raw memory is not lent yet (`&*p`) |
| `p.add(n)`, `p.sub(n)`, `p as *mut u8`, `p.is_null()` | `p + n`, `p - n`, `(raw byte)p`, `p == null` | built | The first three inside `unsafe` |
| `p as usize`, `n as *mut T`, `p.offset_from(q)`, `p < q` | `(nuint)p`, `unsafe (raw T)n`, `p - q`, `p < q` | built | Only `(raw T)n` inside `unsafe`; `p - q` counts items of one type and is defined for any two pointers in compiled code, while the interpreter panics for two blocks |
| `alloc_zeroed(Layout::array::<T>(n))`, `dealloc` | `Memory.allocate<T>(n)`, `Memory.free(p)` | built | Zeroed memory; `free` inside `unsafe`. The interpreter catches an item outside its block, after `free`, and a block freed twice, as Miri does |
| `ptr::copy(src, dst, n)` | `Memory.copy(source, target, count)` | built | Inside `unsafe`; the two may overlap |
| `extern "C" { fn ... }`, `#[link(name = "z")]` | `extern "z" { ... }` | built | Every call inside `unsafe`; `extern { }` is the C library itself. C names as C writes them; private to the module unless `public` is written on a function |
| `c_int`, `size_t`, `*const T`, `*mut T` in a signature | `int`, `nuint`, `read T` or `raw T`, `ref T` or `raw T` | built | `ref` and `read` last for the call; `raw T` when C keeps the address. No `char` (a byte is `byte`) |
| `#[repr(C)] struct`, given and returned by value | a struct of plain data, given and returned by value | built | Every struct of plain data is laid out as C lays it out; no mark is written. No unions, bit fields or `packed` |
| `extern { type sqlite3; }`, an opaque `#[repr(C)] struct` | `extern "sqlite3" { struct sqlite3 }` | built | Only as `raw sqlite3`; `ref raw sqlite3` is C's `sqlite3**` |
| the `cc` crate and a `build.rs` that compiles a C file | `extern "./native.c" { ... }` | built | Compiled in the same unit as the program, so a `static inline` function is inlined; no build script |
| `extern { static mut optind: c_int; }` | `extern { int optind }` | built | Read and written inside `unsafe`, as an item of raw memory |
| `#[no_mangle] pub extern "C" fn compare(a: *const c_void, b: *const c_void) -> c_int` | `export int compare(raw void a, raw void b) { ... }` | built | C types only, no borrows; the name C sees is the name written, one in the program. A panic inside ends the process. Called from SG by its name, with no `unsafe` |
| `extern "C" fn(*const c_void) -> c_int`, `Option<extern "C" fn(...)>` | `CFunc<raw void, int>` | built | May be `null`, as a pointer of C may; the name of an `export` function or of a function of C is one. Every call through it is inside `unsafe`; a `null` one is a panic, not a crash |
| `transmute::<*mut c_void, extern "C" fn(...)>(dlsym(...))` | `unsafe (CFunc<int, int>)dlsym(...)` | built | From a `raw void`; back with `(raw void)f` |
| `&mut state as *mut State as *mut c_void`, then `&mut *(data as *mut State)` | `unsafe Memory.addressOf(ref state)`, then `unsafe (ref State)data` | built | That the value lives and stays where it is while C keeps the address is the program's promise; the interpreter panics on a wrong type, a value that ended or moved, and an address it did not give |
| a closure handed to C through a trampoline | an `export` function and a state reached through `void*` | not in SG | There are no closures; what a closure would capture is a struct whose address C gives back |
| `std::io::Error::last_os_error()`, `errno::set_errno` | `IoError.fromCode(C.errno())`, `C.setErrno(n)` | built | No `unsafe`; `errno` is read right after the call it belongs to |
| `CString::new(s)?`, `c.as_ptr()` | `try CString.from(s)`, `read c` as an argument | built | A zero byte inside is a `TextError` at its position |
| `CStr::from_ptr(p).to_str()?.to_owned()` | `try unsafe string.fromCString(p)` | built | Copies and checks the UTF-8 |
| `v.as_mut_ptr()`, `v.as_ptr()` | `v.pointer()` | built | Only as an argument of a C function, on a list, an array or a slice of plain data; it counts as a `ref` of `v` for the call |
| `libc::malloc` / `libc::free` beside `Box` | `Memory.allocate` / `Memory.free` | built | One allocator: what C gives with `malloc` goes back with `Memory.free` |
| a safe wrapper module around a `-sys` crate | a module whose functions of C are private | built | `examples/zlib` and `examples/sqlite`: their users write no `unsafe` |
| `extern "C" { fn printf(format: *const c_char, ...) -> c_int; }` | `extern { int printf(read CString format, ...) }` | built | As Rust: an `f32`, a small integer and a `bool` are errors after `...`, with the cast as the hint; a number written without a type is an error too, where Rust takes an `i32`. No `va_list` and no function of SG that takes `...` (Rust's `c_variadic` is unstable) |
| `rusqlite::Connection::open(path)?`, `conn.execute(...)`, `conn.prepare(...)` | `Database.open(path)`, `db.execute(...)`, `db.prepare(...)` of `examples/sqlite` | built | An example module, not a library of the language; a `Statement` may outlive its `Database`, as SQLite allows with `sqlite3_close_v2` |

## Tasks and locks

| Rust | SG | Status | Note |
|---|---|---|---|
| `Arc::new(Mutex::new(v))` on a value you already have | `new shared locked v` | built | `v` is moved in; a value is locked from where it is made, never later in place |
| `thread::spawn(move \|\| work(x))` | `Task.run(work, x)` | built | The function and what it is given are written apart |
| `JoinHandle<R>` | `Task<R>` | built | |
| `h.join().unwrap()` | `t.wait()` | built | A panic stops the process, so there is nothing to unwrap |
| dropping a `JoinHandle` (the thread goes on) | a compile error | built | Every task is waited for on every path |
| `Send`, `Sync` | the thread rule | built | No borrow, no `shared checked`, no `Task` crosses, also inside fields |
| `Arc<Mutex<T>>` | `shared locked T` | built | |
| `let mut c = m.lock().unwrap(); *c += 1;` | `lock m as c { c += 1 }` | built | What it holds is reached only inside the block |
| two locks, one after the other | `lock a as x, b as y { }` | built | Taken in a fixed order; a lock inside a lock is an error |
| a lock held across a wait | a compile error | built | Nothing in a `lock` block waits, so locks and waits cannot deadlock |
| lock poisoning | none | built | A panic stops the process |
| `static`, `thread_local!` | none | not in SG | No global variables: what tasks share is in the arguments of `Task.run` |
| `async fn`, `.await` | `async int f()`, `await t`, `await f(x)` | built | C#'s behaviour: a call starts the task at once; a call alone on a line is an error. Compiled as Rust compiles it ("Async, the direction") |
| `tokio::spawn(f(x))` | `var t = f(x)` (no `await`) | built | What it is given follows the thread rule; `await f(read x)` can borrow, as an awaited future does |
| `tokio::time::sleep(d).await` | `await Task.delay(ms)` | built | |
| `block_on(f())` | `f().wait()` in code that is not async | built | `wait` in an async function is an error |
| `spawn_blocking(work)` | `await Task.run(work, x)` | built | A call that holds the thread in an async function is an error, followed through the calls |
| `Box::pin(f(n - 1)).await` | `var t = f(n - 1)`, then `await t` | built | An async function that awaits itself is an error |
| `async fn` in an `impl` | `async int total(read this)` | built | A method that lends `this` is awaited where it is written; one that takes `this` by value may start as a task. `create`, `format`, `equals`, `hash` and `compare` are not async |
| `CancellationToken::new()` (tokio-util) | `CancellationSource()` | built | The source cancels, its tokens look; `CancellationSource.after(ms)` cancels itself |
| `token.child_token()`, `token.clone()` | `source.token()`, `token.clone()` | built | A token cannot cancel; a source has one owner, or is `shared` to be cancelled from another task |
| `token.cancel()` | `source.cancel()` | built | Through a `read` borrow, as `cancel(&self)` |
| `token.is_cancelled()` | `token.isCancelled()`, `try token.check()` | built | `check` gives `Error(Cancelled { })`, which `try` turns into `IoError.Cancelled(...)` |
| `select! { _ = token.cancelled() => ..., _ = sleep(d) => ... }` | `await Task.delay(ms, read token)` | built | Gives `Error(Cancelled { })` when the token is cancelled first |
| dropping a future to cancel it | none | not in SG | A task stops only where it looks at its token; a cancel ends a wait for the network or for a line of the input at once |
| `Vec<JoinHandle<T>>`, `JoinSet<T>` | `List<Task<T>>` | built | Kept only where a task is kept; its methods are `add` and `length`, and its tasks are taken out only by `Task.whenAll` and `Task.whenAny` |
| `join_all(handles).await` | `await Task.whenAll(tasks)` | built | What each gave, in the order of the list; an error is an item of the list, none is lost. `Task.whenAll(tasks).wait()` in code that is not async |
| `set.join_next().await` | `await Task.whenAny(ref tasks)` | built | What the first task to end gave, taken out of the list; nothing is cancelled. A `while tasks.length() > 0` loop around it empties the list, as the compiler sees |
| a `JoinSet` dropped (its tasks aborted) | none | not in SG | A list of tasks that is not emptied is a compile error; a way out with an error waits for what is left in it |
| `tokio::fs::read_to_string(p).await` | `await File.readTextAsync(read path)` | built | Each function of `File` that reads or writes a whole file has an `Async` version; a token may come last. Compiled, the work runs on a thread of input and output, as tokio's `spawn_blocking` does it |
| `reader.read_line(&mut line).await` | `await reader.readLineAsync(ref line)` | built | Also `readTextAsync`, `readBytesAsync`, on `Stdin` too; a line the block already holds is read in place, as tokio's `BufReader` does it |
| `BufReader::new(tokio::fs::File::open(p).await?)` | `try await File.openTextAsync(read path)` | built | Also `createTextAsync` and `openAppendAsync`, which give a `StreamWriter` |
| `writer.write_all(b).await`, `writer.flush().await` | `await writer.writeLineAsync(x)`, `writeAsync`, `writeBytesAsync`, `flushAsync()` | built | What the block of 64 KB can take is written in place, as tokio's `BufWriter` does; a full block goes out on a thread of input and output. On `Stdout` and `Stderr` too, in order with `print` |
| `writer.shutdown().await` | `try await writer.closeAsync()` | built | Takes the writer; a local writer left open is a compile error (`Closable`), as without async |
| a `BufWriter` dropped without `flush` (what it holds is lost) | a writer dropped in an async function | built | Written and closed on a thread of input and output, awaited where it is dropped: Rust has no async drop |
| tokio, async-std, smol | one built-in runtime | built | A pool of threads, one for each core, and a clock for delays; only a program with tasks carries them. Input and output the async way runs on threads of their own, started as they are needed (up to 512, as tokio's blocking threads), and the network on epoll or kqueue, waited on by a thread of the pool that has nothing to do, one at a time, as tokio's driver; one idle thread at a time is woken to look for work, as in tokio; a socket goes on the loop once, edge-triggered, and its tasks wait in its slot, as tokio's registration; each thread has a ring of 256 tasks that the others steal half of, and a slot for the task just woken, as tokio's `multi_thread` scheduler. Unlike tokio: a thread that looks for work takes from the slots of the others too, and the one that finds work wakes the next only if work is left |

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
| `fs::remove_file(p)` | `File.delete(read p)` | built | A missing file is `NotFound`, as in Rust; a folder is `IsDirectory` on every system |
| `fs::rename(a, b)` | `File.replace(read a, read b)` | built | Takes the place of `b` in one step |
| `fs::rename(a, b)` after a test that `b` is missing | `File.move(read a, read b)` | built | `AlreadyExists` when `b` is there, told in the same step as the rename (`RENAME_NOREPLACE`, `RENAME_EXCL`); Rust's `rename` has no such form |
| `fs::copy(a, b)` | `File.copy(read a, read b)` | built | Never takes the place of `b`: `AlreadyExists`, where Rust's `copy` writes over it. The permissions come along without setuid, setgid and sticky |
| `fs::metadata(p)?.len()`, `.is_file()`, `.modified()` | `var info = try File.info(read p)`, `info.size`, `info.kind`, `info.modified` | built | `modified` is a `DateTime`; a link is followed, as `metadata` does |
| `Path::new(p).is_file()`, `.is_dir()` | `File.exists(read p)`, `Directory.exists(read p)` | built | `false` when the path cannot be looked at, as in Rust |
| `fs::create_dir(p)`, `fs::create_dir_all(p)` | `Directory.create(read p)`, `Directory.createAll(read p)` | built | |
| `fs::remove_dir(p)`, `fs::remove_dir_all(p)` | `Directory.delete(read p)`, `Directory.deleteAll(read p)` | built | Neither follows a link inside; `deleteAll` of a path that is itself a link is `NotDirectory`, where Rust removes the link |
| `fs::read_dir(p)`, `entry.file_name()`, `entry.file_type()` | `try Directory.list(read p)`, `entry.name`, `entry.kind` | built | A `List<DirectoryEntry>` in the order of the bytes of the names, where Rust gives the order of the system; a name that is not UTF-8 is an `InvalidText`, where Rust gives an `OsString` |
| `env::current_dir()`, `env::temp_dir()` | `Directory.current()`, `Directory.temp()` | built | There is no `set_current_dir`: every thread would see it change |
| `tempfile::tempdir()` | `Directory.createTemp()` | built | In the language, not a crate: `mkdtemp`, a folder only the user can go into; removing it is the program's work |
| `path.join(name)`, `path.file_name()`, `path.extension()`, `path.parent()` | `Path.join(read path, name)`, `Path.fileName(read path)`, `Path.extension(read path)`, `Path.parent(read path)` | built | A path is a `string`. A second path that starts with `/` is put after the first, where Rust's `join` puts it in its place; the extension keeps its `.`, as in C# |
| `env::var("HOME")` | `Environment.get("HOME")` | built | A `Result<string, VariableError>` with `Missing` and `InvalidText(position)`, as Rust's `VarError` |
| `env::set_var`, `env::remove_var` | none | not in SG | Unsafe in Rust 2024 for the same reason: another thread may be reading. A program that starts another gives it an environment of its own |

## Other programs

| Rust | SG | Status | Note |
|---|---|---|---|
| `Command::new("git")` | `var git = Command("git")` | built | No shell, as in Rust. A name without `/` is looked for only in the folders of `PATH` that start with `/`, as Go does since 1.19; Rust also runs a program found through `.` or a relative folder |
| `.arg("log")`, `.args(["-1", "--oneline"])` | `git.arg("log")`, `git.args(read words)` | built | One call per line: the methods change the command (`ref this`) and give nothing, so they are not chained |
| `.current_dir(dir)` | `git.directory(read dir)` | built | A relative path of the program is taken from our folder, not from the program's |
| `.env(k, v)`, `.env_remove(k)`, `.env_clear()` | `git.setVariable(k, v)`, `git.removeVariable(k)`, `git.clearVariables()` | built | A table of the program's own; ours is never changed. A name with `=` or a zero byte is `IoError.InvalidInput` when it runs, as Rust's `InvalidInput` |
| `.status()?` and `status.success()` | `try git.run()` | built | `run` takes an end with anything but 0 for an error (`ProcessError.Failed`), as Go's `Run`; Rust leaves the check to the program |
| `.status()?` | `try git.status()` | built | Every end is a value: `ExitStatus.Code(code)` or `ExitStatus.Signal(number)`, where Rust has `code()` and `signal()` |
| `.output()?`, `String::from_utf8(out.stdout)?` | `try git.output()` | built | The output as a `string` (`InvalidText` if it is not UTF-8); the standard error goes to ours, not into the value, and an end with anything but 0 is an error |
| `.stdin(Stdio::piped())`, writing in a thread | `git.input(read text)` | built | Written while the output is read, so neither waits for the other; a program that stops reading is no error |
| `.spawn()?` | `var process = try git.start()` | built | Gives a `Process`, which runs while this program goes on |
| `.stdin(Stdio::piped())`, `.stdout(...)`, `.stderr(...)` | `git.pipeInput()`, `git.pipeOutput()`, `git.pipeError()` | built | Only `start` looks at them; a stream not asked for is ours, as Rust's default for `spawn` |
| `child.stdin.take().unwrap()` | `process.takeInput() ?? panic("...")` | built | `takeInput` gives a `StreamWriter?`, `takeOutput` and `takeError` a `StreamReader?`: `null` once taken, or when not asked for. The streams are owned and go to other tasks |
| `drop(stdin)` to send the end of the input | `try input.close()` | built | The writer is `Closable`; writing after the program stopped reading gives `IoError.ConnectionReset`, never the signal |
| `child.wait()?` | `try process.wait()` | built | Takes the `Process`, closing the input it still holds first, as Rust's `wait` does, and letting go of its readers |
| a `Child` dropped (the program goes on, a zombie once it ends) | a compile error; on a way out with an error, the program is stopped | built | A `Process` in a local variable is waited for on every path; one that ends without that (an error, a field) is stopped with the signal 9 and waited for, as tokio's `kill_on_drop(true)` |
| `child.kill()?` | `process.kill()` | built | Gives nothing: a program not waited for keeps its number, so the signal cannot reach another one |
| `libc::kill(pid, SIGTERM)` | `process.terminate()` | built | The polite stop, which the program may catch |
| `child.id()` | `process.id()` | built | |
| `child.try_wait()` | none | designed | A wait with a time limit is `waitAsync` with a token of `CancellationSource.after(ms)` |
| `tokio::process::Command::status().await` | `await git.statusAsync()`, `runAsync()`, `outputAsync()` | built | A token may come last; a cancel stops the program, as Go's `CommandContext` |
| `child.wait().await` | `await process.waitAsync()` | built | Waits on the loop, as tokio does: the pidfd on Linux, `EVFILT_PROC` on macOS; a cancel stops the program |

## Network

| Rust | SG | Status | Note |
|---|---|---|---|
| `TcpListener::bind("127.0.0.1:8080")?` | `try TcpListener.bind("127.0.0.1:8080")` | built | A number, `localhost` or `":8080"` (every v4 address); no name is looked up, so it never waits. Port 0 lets the system choose: `listener.port()` |
| `listener.accept()?` | `try listener.accept()` | built | Through a `read` borrow, as `accept(&self)`; gives the `TcpStream` alone, not the address of the other side |
| `TcpStream::connect("example.com:80")?` | `try TcpStream.connect("example.com:80")` | built | The host looked up by name, each address tried in turn |
| `stream.read(&mut buf)?` | `try stream.receive(ref buffer)` | built | What has come, how many, 0 at the end; `read` is a word of SG, and `receive` is C#'s name |
| `stream.write_all(&bytes)?` | `try stream.send(bytes)` | built | All of them; writing after the other side has gone is `ConnectionReset`, never `SIGPIPE`. Through a `read` borrow, as `Write for &TcpStream`, so it sends while a reader borrows the stream; not through a `shared` stream |
| `BufReader::new(&stream)`, `.read_line(&mut line)?`, `.lines()` | `var reader = stream.reader()`, `try reader.readLine(ref line)`, `reader.lines()` | built | A `StreamReader` that borrows the stream; `receive` while it is used again is a compile error, since it would take bytes from under its block. A line, or a text, longer than 64 KB is `IoError.LineTooLong` (`stream.reader(limit)` for another limit), as Go's `bufio.Scanner`; every read after it gives it too |
| `reader.fill_buf()?` / `BufRead` then `consume` | `try reader.readSome(ref buffer)` | built | On every `StreamReader`: what the block holds, or one read of the system when it is empty |
| `tokio::io::BufReader::new(stream).lines()` | `await reader.readLineAsync(ref line)` | built | A line the block holds is read in place; a cancel ends a wait, and what came stays in the block |
| `drop(stream)`, `stream.shutdown(Both)` | the stream ends | built | Closed where it ends; no `close` |
| `tokio::net::TcpListener`, `TcpStream`, `.await` | `await listener.acceptAsync()`, `connectAsync`, `receiveAsync`, `sendAsync` | built | A token may come last, and a cancel ends a wait at once; what came stays in the socket |
| `stream.peer_addr()?`, `stream.local_addr()?` | `try stream.remoteAddress()`, `stream.localAddress()` | built | A `SocketAddress`; the other side is `ConnectionReset` once it has broken the connection off |
| `stream.into_split()`, `Arc<TcpStream>` sent on from several threads, `stream.shutdown(Write)`, `set_nodelay` | none yet | designed | Later slices of the network |
| `UdpSocket::bind("127.0.0.1:0")?` | `try UdpSocket.bind("127.0.0.1:0")` | built | What `TcpListener.bind` takes; no name is looked up |
| `socket.send_to(&bytes, addr)?` | `try socket.sendTo(bytes, to)` | built | One datagram, whole: one too long to send as one message is `IoError.MessageTooLong`. `to` is a `SocketAddress`, not a text to look up |
| `let (n, from) = socket.recv_from(&mut buf)?` | `var got = try socket.receiveFrom(ref buffer)` | built | A `Received` with `got.count` and `got.from`. A datagram longer than the buffer is `MessageTooLong`, where Rust cuts it short |
| `socket.connect(addr)?`, `socket.send(&b)?`, `socket.recv(&mut buf)?` | `try socket.connect(to)`, `try socket.send(bytes)`, `try socket.receive(ref buffer)` | built | `ConnectionRefused` when the other machine says nobody listens |
| `socket.local_addr()?`, `socket.peer_addr()` | `socket.localAddress()`, `socket.remoteAddress()` | built | `remoteAddress` is a `SocketAddress?`: `null` before a `connect` |
| `Arc<UdpSocket>` shared by threads, `&self` methods | `shared UdpSocket`, every method through `read` | built | No `close`: it is closed where it ends |
| `tokio::net::UdpSocket`, `.recv_from(&mut buf).await` | `await socket.receiveFromAsync(ref buffer)`, `sendToAsync`, `receiveAsync`, `sendAsync` | built | A token may come last; a cancel ends a wait at once |
| `"[::1]:53".parse::<SocketAddr>()?` | `try SocketAddress.parse("[::1]:53")` | built | `IoError.InvalidInput` for a text that is not an address; printed as it is read (RFC 5952) |
| `("example.com", 53).to_socket_addrs()?` | `try SocketAddress.lookup("example.com:53")` | built | A `List<SocketAddress>`; `HostNotFound` when the name has none. `lookupAsync` looks it up on a thread |
| `addr.ip()`, `addr.port()` | `address.host()`, `address.port()` | built | `host()` is text: there is no `IpAddr` type |
| `socket.join_multicast_v4(...)`, `set_broadcast(true)`, `set_ttl`, `peek_from` | none yet | designed | Later |

## Time

| Rust | SG | Status | Note |
|---|---|---|---|
| `std::time::Instant::now()`, `start.elapsed()`, `later.duration_since(earlier)` | `Instant.now()`, `start.elapsed()`, `later.since(earlier)` | built | `since` gives a negative `Duration` where Rust's saturates at zero; an `Instant` is not printed |
| `std::time::Duration::from_millis(250)`, `d.as_millis()`, `d.as_secs_f64()` | `Duration.fromMilliseconds(250)`, `d.milliseconds()`, `d.totalSeconds()` | built | A `long` of nanoseconds that may be negative; `{:?}` and `print(d)` both write `1.5s` |
| `a + b`, `a - b`, `d * 3`, `a.checked_add(b)` | `a.plus(b)`, `a.minus(b)`, `d.times(3)` | built | Methods; an overflow follows the rule of the numbers, so there is no `checked_` form yet |
| `SystemTime::now()`, `.duration_since(UNIX_EPOCH)` | `DateTime.now()`, `t.unixSeconds()`, `t.unixMilliseconds()` | built | UTC, nanoseconds in a `long`: the years 1677 to 2262 |
| `chrono::Utc.with_ymd_and_hms(...)`, `t.year()`, `t.weekday()`, `t.to_rfc3339()` | `DateTime.utc(...)` (a `Result<DateTime, DateError>`), `t.year()`, `t.dayOfWeek()`, `print(t)` | built | In the language, not a crate; `2026-11-15T09:30:05Z` |
| `std::thread::sleep(d)` | `Task.delay(ms).wait()`; `await Task.delay(ms)` in async code | built | Milliseconds as an `int` |
| `chrono::Local`, `chrono-tz`, `NaiveDate::parse_from_str` | none yet | designed | Time zones and reading a date out of text come later |

## Random numbers

| Rust | SG | Status | Note |
|---|---|---|---|
| `rand::rngs::SmallRng::seed_from_u64(42)`, `rand_xoshiro::Xoshiro256StarStar` | `Random.fromSeed(42)` | built | In the language, not a crate: xoshiro256\*\* seeded with SplitMix64, the same numbers for the same seed in the interpreter, in compiled code and on every system |
| `SmallRng::from_entropy()`, `rand::thread_rng()` | `Random.fromSystem()` | built | No generator kept for the thread: the program makes one and lends it with `ref r` |
| `rng.gen_range(1..7)`, `rng.gen::<f64>()`, `rng.gen::<bool>()`, `rng.gen::<u64>()`, `rng.fill_bytes(&mut buf)` | `r.nextInt(1, 7)`, `r.nextLong(low, high)`, `r.nextDouble()`, `r.nextBool()`, `r.nextUlong()`, `r.fill(ref buf)` | built | An empty range is a panic, as in Rust. A `Random` is not `Copyable`, where Rust's `SmallRng` is `Clone` |
| `rand::rngs::OsRng`, `getrandom::getrandom(&mut buf)` | `SecureRandom.fill(ref buf)`, `SecureRandom.nextInt(low, high)`, `SecureRandom.nextUlong()` | built | Never an error: if the system gave no bytes, a panic, as `OsRng` panics |
| `hex::encode(bytes)` of random bytes | `SecureRandom.hexString(32)` | built | Lower-case hex digits, four random bits each |
| `slice.shuffle(&mut rng)`, `slice.choose(&mut rng)` | none yet | designed | Waits for generics or a built-in method; Fisher-Yates is four lines |
| `rand_distr::Normal` | none yet | designed | |

## Tests and the tools

| Rust | SG | Status | Note |
|---|---|---|---|
| `#[test] fn name()` | `test "name" { }` | built | At the top level of the file, next to the code |
| `cargo test` | `sg test file.sg` | built | `sg test --compiled` runs them compiled |
| `cargo run` | `sg file.sg` | built | The interpreter |
| `cargo build`, `cargo build --release` | `sg build file.sg`, `sg build file.sg --release` | built | A debug build checks overflow and reports leaks |
| a test that leaks `Rc` passes | the test fails with the leak report | built | |
| Cargo, crates.io | none yet | designed | No package manager yet |
