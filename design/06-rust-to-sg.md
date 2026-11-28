# From Rust to SG

What a Rust programmer writes, and what SG writes for it. SG keeps Rust's promise (no use after free, no
data race, no `null` that was not asked for) with C#'s way of writing, and leaves out what costs more
complexity than it gives (closures, generics of your own, macros, lifetimes in the signature).

The **Status** column says how far each line is:

- **built**: runs in the interpreter and is compiled to C,
- **interpreter**: runs in the interpreter, compiled code is still to come,
- **designed**: decided in the design, not built yet,
- **not in SG**: left out on purpose; the note says what to write instead.

The details of each line are in [the decisions by topic](rules/README.md) and in the
[choices of each slice](decisions/README.md). This file is brought up to date with every slice of the
language, in the same commit.

Last brought up to date: 27 November 2026, the hundred-and-sixty-sixth slice (`derive(Clone)`, `Rc::make_mut`, a reader of JSON's tokens, `(?x)`, the stack, 1813-1822), and after it 1823 and 1826; the hundred-and-sixty-seventh (JSON read and written with a loop, 1829); the hundred-and-sixty-eighth (JSON read and written by a recursion, with a list where the stack runs low, 1832-1834); the hundred-and-sixty-ninth (the walks of the interpreter measured, 1835: nothing of Rust changes); the hundred-and-seventieth (a large frame touched only near the bottom of the stack, 1836); the hundred-and-seventy-third (`regex::bytes`, `read_until` and `memmem` of bytes, 1840-1843); the hundred-and-seventy-fourth (`Regex` faster in any case: nothing of Rust changes); the hundred-and-seventy-fifth (`sync_data`, `set_len`, a folder synced, `Vec<u8>` as a key, 1852-1853), and after it `fs::rename` over a file that is open, on Windows too; the hundred-and-seventy-sixth (`available_parallelism`, 1856); the hundred-and-seventy-seventh (`tokio::fs` for the calls of the file system, `percent_encoding`, `httpdate`, a body of a known length, 1865-1867, 1871); the hundred-and-seventy-eighth (`File::metadata` of an open file, and a file opened only when it is a regular one, 1873); the hundred-and-seventy-ninth (a GIF tool, 1874-1881: nothing of Rust changes); the hundred-and-eightieth (`HashMap`'s lookup as hashbrown's, 1882: nothing of Rust changes); the hundred-and-eighty-first (an interpreter of Lox: `to_bits` and `from_bits`, `%e` and `%g`, a hash of a key of a fixed size as hashbrown's, 1891-1893); the hundred-and-eighty-second (`is_terminal`, 1901); the hundred-and-eighty-third (an `if` that gives a value is `c ? a : b`, 1910-1916); the hundred-and-eighty-fourth (`type X = T` is `alias X = T`, 1918-1920); the hundred-and-eighty-fifth (`v[v.len() - 1]` is `xs[^1]`, 1921-1926); the hundred-and-eighty-seventh (`RUSTFLAGS` and `CFLAGS` are the flags of C after `--`, 1935-1941); the hundred-and-eighty-eighth (`rust-gdb` on a debug build is gdb on a debug build, on the lines of SG, 1943-1949); the hundred-and-eighty-ninth (`OsString` is a `string` with what is not UTF-8 escaped, 1950-1959); the hundred-and-ninetieth (`UnixListener` and `UnixStream`, `shutdown(Write)`, `set_nodelay`, keep-alive, `[::]` for v4 too, `write_vectored` on a `BufWriter`, 1960-1972); the hundred-and-ninety-first (`symlink`, `hard_link`, `read_link`, `symlink_metadata`, `0o755`, `set_permissions`, `File::lock` and its kin, `memmap2::Mmap`, 1973-1986); the hundred-and-ninety-second (crossbeam's and tokio's `select!` over channels are `select`, 1987-2001); the hundred-and-ninety-third (threads that wait for each other end in a panic, compiled too, 2002-2010); the hundred-and-ninety-fourth (`split_at_mut`, `get_disjoint_mut` and `split_first_mut` are two windows of one place, held apart, 2011-2015); the hundred-and-ninety-sixth (`try_wait`, a `Stdio` of a file or of nothing, `2>&1`, `a | b`, a child let go of, `id`, `parent_id`, `kill` by a number, 2016-2029); the hundred-and-ninety-seventh (`process_group`, `setsid`, `killpg`, `tcsetpgrp`, `waitpid` with `WUNTRACED`, the signals of job control, 2023-2026, 2030-2041); the hundred-and-ninety-eighth (`portable_pty`, raw mode of `crossterm` or `termios`, the size of the terminal, 2042-2053), and after it `io::stdout()` into a pipe no one reads (2055); the hundred-and-ninety-ninth (`notify`'s watcher of a folder, 2056-2065); the two-hundredth (`cap-std`'s folder as a root, a file opened to read and write, `stream_position`, `tempfile::tempfile()`, 2066-2077); and on 28 November a `$"..."` kept in a variable borrows as `format_args!` does (2110).

## Functions and variables

| Rust | SG | Status | Note |
|---|---|---|---|
| `fn add(a: i32, b: i32) -> i32 { a + b }` | `int add(int a, int b) { return a + b }` | built | The type comes first, as in C#; the last line is not the value, `return` is written |
| `fn main()` | `void main()` | built | |
| `fn main() -> Result<(), E>` | `Result<void, E> main()` | built | The error it ends with is printed (`E : Formattable`), exit code 1; an output that cannot be written either is told after it (1603), where Rust drops that error |
| `std::env::args()` | `void main(List<string> args)` | built | The words after the program's name; a word that is not UTF-8 comes as a name of the system does (below), where Rust's `args()` panics |
| `std::env::args_os()`, `OsString`, `OsStr::as_encoded_bytes()`, `os_str.to_string_lossy()` | `string`, `Path.systemBytes(name)`, `Path.fromSystemBytes(bytes)` | built | No type of its own: a name of the system is a `string` in which each byte that is not UTF-8 is a character from U+10FF80 to U+10FFFF, which goes back to that byte when it is given to the system (Python's PEP 383, with characters of the last plane of private use in place of halves of pairs). Nothing is lost, as with `OsString`, and nothing is replaced, as `to_string_lossy` replaces it with U+FFFD |
| `let x = 1;` | `var x = 1` | built | No `;` at the end of a line |
| `let x: i64 = 1;` | `long x = 1` | built | |
| `let x = x.trim();` (shadowing) | `var trimmed = x.trim()` | built | A new variable takes a new name: a name of a block around it that is still open, local or parameter, is a compile error, as in C#. Blocks side by side, and code after a block has ended, may use a name again |
| `let mut x = 1;` | `var x = 1` | built | Every variable can be given a new value; a field can be `readonly` |
| the `unused_variables` lint (a warning) | nothing to write | built | A local variable nothing reads is a compile error: `var x = 1` alone, a variable only written (`x += 1`, `p.x = 0` on a copy), the name of a `for` or of a pattern. A write through `ref`, `shared` or raw memory is a use; a parameter and a `Disposable` value are not looked at. A `const` or `static` of a block that nothing after it names is an error too (2194) |
| `let _guard = ...;` | `var guard = ...` | built | A variable of a `Disposable` type is kept for its end without being read; SG has no `_name` |
| `for _ in 0..n`, `Some(_)` | `for _ in 0..n`, `Ok(_)` | built | |
| `'outer: for ... { break 'outer; }` | `outer: for ... { break outer }` | built | Also `continue outer` and `outer: while`; C2y's and Go's notation, with no `'`. A name not used is an error, and so is `break outer` in `outer` itself, where a plain `break` does the same (2193). `loop { }` is `while true { }` |
| the `unreachable_code` lint (a warning) | nothing to write | built | A line after `return`, `break`, `continue`, `panic(...)` or `exit(...)`, or after an `if`, `match` or `while true` that always leaves, is a compile error (2191) |
| `for i in (0..n).rev()` | `for i in (0..n).reversed()` | built | Only at the head of a `for`; `step_by` is not there |
| the `unused_assignments` lint (`x = 2` never read after) | nothing | not in SG | Not looked at yet: a value given after the last read, or written over before it is read, passes; noted for later |
| `type Id = u64;` | `alias Id = ulong` | built | `public alias` from a module; `Id::MAX`, `x as Id` and `Id::from_str` are `Id.MaxValue`, `(Id)x` and `Id.parse(text)` (2214) |
| `const BLOCK: usize = 4096;` | `const nint BlockSize = 4096` | built | At the top of a file (`public const` from a module) or in a block. Numbers, `bool`, `char`, a text, and arrays, structs and enums of numbers; the value is found when the program is compiled, an overflow there is a compile error. A name in PascalCase, as C#'s |
| `const MASKS: [u32; 4] = [1, 3, 7, 15];` | `const uint[] Masks = { 1, 3, 7, 15 }` | built | The items are counted. Used where it lies, as a `read` borrow that lasts for the run; `var copy = Masks` copies it |
| `const fn` called in a `const` | `static uint[256] Crc = crcTable()`, or the table written out | not planned | A const calls no function, as in C# and Go: the compiler runs none of the program's code (choice 1606; calls were in from the hundred-and-fourth slice to the hundred-and-forty-fifth). A value a function finds is a `static`, found the first time it is used (Rust's `LazyLock`); a table needed from the start is written out by a script, as zlib's `crc32.h` is |
| `include_bytes!("logo.gif")` | `embed("logo.gif")`, in a const: `const byte[] Logo = embed("logo.gif")` | built | C23's `#embed`; the path from the folder of the source file, a file of at most 64 MB in that folder or below it (no `..`, no root, not through a link that leads out; Rust takes any path) |
| `const _: () = assert!(size_of::<Header>() == 8);` | `const assert(Memory.sizeOf<Header>() == 8, "eight bytes")` | built | At the top of a file or in a block; the condition is made as a const is, with no call, and the error shows both sides of a comparison. One that measures a type is handed to C's `_Static_assert` too |
| `match x { MAX => ... }` (a const in a pattern) | `match x { Max => ... }` | built | A const of a number, a `char`, a text or a `bool`; a local never takes the name of a const it sees, so the name means the const |
| `static X: T = expr;` (a constant expression) | `static T X = expr` | built | Lies in the program's data from its start, with no check; a `static const` in C when it holds no atomic value |
| `static X: LazyLock<T> = LazyLock::new(f);`, `OnceLock` | `static T X = f()` | built | Found the first time it is used, once, whichever task comes first; the others wait. A later read is an acquire load and a branch. Read as a `read` borrow |
| `static HITS: AtomicU64 = AtomicU64::new(0);` | `static atomic ulong Hits = atomic 0` | built | Changed by its atomic methods, with the orders of C11 |
| `static CONFIG: Mutex<Config> = Mutex::new(...)`, `LazyLock<Mutex<T>>` | `static locked Config Settings = ...` | built | Reached only in a `lock` block, which never waits, so it cannot deadlock |
| `fn f() { static N: AtomicU32 = ...; }` | `static atomic int N = atomic 0` inside the function | built | One for the program; the name is seen only in that block |
| `static mut X: T` | none | not in SG | A global that any task assigns is a race the compiler cannot see; what changes is `atomic` or `locked` ("Globals, as decided") |
| `thread_local!` | none | not in SG | A task moves between the threads of a pool, so a value of each thread would not follow it; what belongs to a task is passed to it |
| `std::process::exit(2)` | `exit(2)` | built | Takes a `byte`, so `exit(256)` is a compile error |
| `panic!("...")` | `panic("...")` | built | |
| `#[track_caller]` on the standard library | nothing to write | built | A panic inside the language's own module (`r.nextInt(5, 5)`, `Duration.fromSeconds` of a count too large) is shown at the call of the program, without the hint about the module's code; a function of your own cannot ask for it |
| `assert!(a == b)`, `assert_eq!(a, b)` | `assert(a == b)` | built | The message shows the expression and both sides |
| `println!("{}", x)` | `print(x)` | built | Into an output that was closed (`program \| head`) Rust panics (exit code 101); SG ends by `SIGPIPE` on Linux and macOS, as C and Go do, and with 0 on Windows. Rust's `stdout` is a `LineWriter` behind a lock, written line by line; SG's compiled `print` gathers its lines in a block of 64 KB, as a `BufWriter` does, given on before `printError`, a call of C and the end, and line by line to a terminal (1545). Where it cannot write (a full disk) Rust panics, but drops silently what its block still holds at the end; SG ends with a message and 1 there too, interpreted and compiled (1547) |
| `eprintln!("{}", x)` | `printError(x)` | built | A line is one write of the system, however long. Into a standard error that was closed, Rust's `eprintln!` panics; SG ends by `SIGPIPE` on Linux and macOS, as `print` does, and goes on without the line on Windows (1601) |
| closures `\|x\| x + 1` | a named function | not in SG | A `Func` is the address of a named function |
| macros (`macro_rules!`, `#[derive]`) | none | not in SG | What `#[derive]` gives is an interface after `:` (below) |

## Numbers

| Rust | SG | Status | Note |
|---|---|---|---|
| `i8` `u8` `i16` `u16` `i32` `u32` `i64` `u64` | `sbyte` `byte` `short` `ushort` `int` `uint` `long` `ulong` | built | |
| `isize`, `usize` | `nint`, `nuint` | built | An index and a length are `nint` |
| `f32`, `f64` | `float`, `double` | built | |
| `println!("{}", x)` on an `f64` | `print(x)` | built | The same fewest digits, laid out as JavaScript does (`1e+21`, `2` for `2.0`); in a tie between two numbers of the fewest digits Rust takes the larger, SG the even one (`160057272853489.62`, Rust `...63`). Compiled with Dragonbox, as the `ryu` crate is used for speed in Rust |
| overflow: panic in debug, wrap in release | the same | built | Rust's rule, also for shifts and casts. An operation on numbers all known when the program is compiled that overflows or divides by zero is a compile error, as Rust's `arithmetic_overflow` and `unconditional_panic` lints deny it (2090) |
| `x as i32` (cuts the bits) | `(int)x` (panics if it does not fit) | built | `wrappingToInt()` cuts the bits, `tryToInt()` gives `null` |
| `i32::try_from(x).ok()` | `x.tryToInt()` | built | `int?` |
| `a.wrapping_add(b)` | `a.wrappingAdd(b)` | built | |
| `a.checked_add(b)` | `a.tryAdd(b)` | built | `int?` |
| `a.saturating_add(b)`, `saturating_sub`, `saturating_mul` | `a.saturatingAdd(b)`, `saturatingSubtract`, `saturatingMultiply` | built | On every integer type. `saturating_div` and `saturating_pow` are not in SG |
| `x.abs()` | `x.abs()` | built | On the signed types; `int.MinValue.abs()` follows the rule of overflow, as in Rust. On an unsigned type an error, where Rust has no `abs` there |
| `x.checked_abs()`, `x.wrapping_abs()`, `x.unsigned_abs()` | `x.tryAbs()`, `x.wrappingAbs()`, `x.unsignedAbs()` | built | |
| `x.checked_neg()`, `x.wrapping_neg()` | `x.tryNegate()`, `x.wrappingNegate()` | built | On every integer type, as in Rust; the panic of `-x` names them |
| `x.clamp(lo, hi)` | `x.clamp(low, high)` | built | Integers and fractional numbers; a low end above the high end panics, as in Rust, and is a compile error when both are written as numbers. On a fractional number `-0` is below `0`, as `min` and `max` have it |
| `i32::try_from(x).unwrap_or(if x < 0 { i32::MIN } else { i32::MAX })`; `x as i32` on an `f64` | `x.saturatingToInt()` | built | From every integer type and from `float` and `double`; `NaN` gives 0, as Rust's `as` |
| `i32::MAX`, `f64::NAN`, `std::f64::consts::PI` | `int.MaxValue`, `double.NaN`, `double.Pi` | built | |
| `"42".parse::<i32>()` | `int.parse(text)` | built | `Result<int, NumberError>`; the error says where the text stops being a number |
| `i32::from_str_radix(s, 16)` | `int.parseHex(s)` | built | |
| `u64::from_le_bytes(b)` | `ulong.fromLittleEndian(bytes)` | built | Takes a slice of exactly 8 bytes; another width is a compile error where it shows |
| `n.to_le_bytes()` | `n.toLittleEndian()` | built | A `byte[8]` |
| `n.count_ones()`, `n.leading_zeros()`, `n.trailing_zeros()`, `n.leading_ones()`, `n.trailing_ones()` | `n.countOnes()`, `n.leadingZeros()`, `n.trailingZeros()`, `n.leadingOnes()`, `n.trailingOnes()`; also `countZeros()` | built | An `int`, where Rust gives a `u32`: it mixes with an `int` and widens to an index. On every integer type, a signed one by its bits |
| `n.rotate_left(k)`, `n.rotate_right(k)` | `n.rotateLeft(k)`, `n.rotateRight(k)` | built | The count is any integer, cut to the width (a negative one turns the other way) |
| `n.swap_bytes()`, `n.reverse_bits()` | `n.reverseBytes()`, `n.reverseBits()` | built | |
| `n.is_power_of_two()`, `n.next_power_of_two()`, `n.ilog2()` | `n.isPowerOfTwo()`, `n.nextPowerOfTwo()`, `n.log2()` | built | On the unsigned types, as in Rust. `nextPowerOfTwo` follows the rule of overflow (0 in a release build, as Rust); `log2` of 0 panics in every build. `checked_next_power_of_two` is not in SG yet |
| `x.sqrt()`, `x.powf(y)`, `a.min(b)` | `x.sqrt()`, `x.pow(y)`, `a.min(b)` | built | |
| `x.sinh()`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `cbrt`, `exp2`, `x.hypot(y)` | the same names | built | From C's library in the interpreter too, where Rust writes `asinh` and its kin itself, so that both give the same digits on one machine |
| `x.exp_m1()`, `x.ln_1p()` | `x.expMinusOne()`, `x.logOnePlus()` | built | |
| `x.gamma()`, `x.ln_gamma()` (unstable) | `x.gamma()`, `x.logGamma()` | built | `logGamma` gives no sign; it calls `lgamma_r`, since `lgamma` writes one variable for every thread. `erf` and `erfc` too, which Rust has only in the `libm` crate |
| `a % b` on `f64` | `a % b` | built | C's `fmod` in both; `a.ieeeRemainder(b)` rounds the quotient to the nearest (C's `remainder`, which Rust has only in `libm`) |
| `x.copysign(y)`, `x.is_sign_negative()`, `x.is_finite()` | `x.copySign(y)`, `x.isSignNegative()`, `x.isFinite()` | built | |
| `x.round_ties_even()` | `x.roundToEven()` | built | C's `rint`; the rounding mode is never changed |
| `x.fract()` | `x.fraction()` | built | C's `modf`: `Infinity` gives 0, where Rust's `fract` gives `NaN` |
| `x.next_up()`, `x.next_down()` | `x.nextUp()`, `x.nextDown()` | built | |
| `x.to_bits()`, `f64::from_bits(b)`, `f32::from_bits(b)` | `x.toBits()`, `double.fromBits(b)`, `float.fromBits(b)` | built | A `ulong` for a `double`, a `uint` for a `float`; a `NaN` keeps its bits (choice 1891). `x as f64` on an `f32` is `(double)x` or the widening, which makes a signalling `NaN` quiet in every build (2091) |
| `libm::ilogb(x)`, `libm::ldexp(x, n)` | `x.exponent()`, `x.scaleByPowerOfTwo(n)` | built | `exponent` is an `int?`: `null` for 0, `NaN` and the infinities, where C gives a number of its own on each system |
| `0xff`, `0b1010`, `1_000_000` | the same | built | |
| `0o755` | the same | built | Octal, as in Rust; `010` is ten in both (1977) |
| `i32 + i64` (an error) | `int + long` is a `long` | built | An integer widens on its own only to a type that holds every value of it; `int + uint` is an error |

## Text

| Rust | SG | Status | Note |
|---|---|---|---|
| `String` | `string` | built | Owns its bytes, always UTF-8 |
| `&str` | `read string` | built | |
| `format!("{a}: {b}")` | `$"{a}: {b}"` | built | A formatted text; `.toString()` makes it a `string`. A text in it is written with its length, the byte 0 included, as in Rust. Kept in a variable it borrows its values until its last use, as `format_args!` does: they are not moved or written before then, and a value with no name cannot be kept (2110) |
| `r#"a "quoted" word"#`, `r"C:\dir"` | `"""a "quoted" word"""`, `"""C:\dir"""` | built | C#'s raw text; on lines, the indentation of the closing `"""` is taken off every line (Rust keeps it; `indoc!` takes it off). `$"""...{x}..."""` puts values in |
| `{:>5}`, `{:<10}`, `{:02X}`, `{:08b}`, `{:.2}` | `{n,5}`, `{name,-10}`, `{b:X2}`, `{mask:b8}`, `{x:F2}` | built | C#'s widths and letters |
| `{:.3e}`, C's `%g` (no `{}` of Rust writes it) | `{x:e3}`, `{x:g}`, `{x:g6}`, `{x:E2}`, `{x:G}` | built | C's `%e` and `%g`, rounded exactly: `3.142e+00`, `1e+21`, `1.23457e+06`; Rust's `{:e}` writes `3.142e0`. 6 digits when none is written (choice 1892) |
| `impl Display for T` | `T : Formattable` with `void format(read this, ref Formatter f)` | built | Without the method the text is made from the fields |
| `String::with_capacity(n)` | `string.withCapacity(n)` | built | |
| `s.push_str(x)`, `write!(s, ...)` | `s.write(x)` | built | Takes whatever `print` takes; `+=` on text is an error whose hint is `write` |
| `s.len()` | `s.length()` | built | In bytes |
| `s.as_bytes()` | `s.asBytes()` | built | `read byte[]` |
| `std::str::from_utf8(b)` | `string.fromUtf8(bytes)` | built | `Result<read string, TextError>` |
| `&s[a..b]` | `s[a..b]` | built | `read string` |
| `s.as_bytes()[i]` | `s[i]` | built | One byte |
| `s.chars()`, `s.split(',')`, `s.split_whitespace()`, `s.lines()` | the same, in camelCase | built | Parts look at the text, nothing copied |
| `s.trim()`, `s.find(x)`, `s.rfind(x)`, `s.contains(x)`, `s.starts_with(x)` | `s.trim()`, `s.indexOf(x)`, `s.lastIndexOf(x)`, `s.contains(x)`, `s.startsWith(x)` | built | `indexOf` gives a `nint?`. Compiled, a text is looked for by its two rarest bytes with vector instructions, as `memchr::memmem` under Rust's `find` (1549), and by Two-Way, as `memmem` turns to, where comparing at the places those bytes are found costs too much (a long text that repeats: 1595) |
| `s.replace(a, b)`, `s.repeat(n)` | `s.replaceAll(a, b)`, `s.repeat(n)` | built | `replace` is taken by `slot.replace(value)` |
| `s.eq_ignore_ascii_case(t)` | `s.equalsIgnoreAsciiCase(t)` | built | |
| `names.join(", ")` | `string.join(", ", read names)` | built | |
| `char` | `char` | built | `'+'` is a `byte` where an integer goes: Rust's `b'+'` |
| `s.to_uppercase()`, `s.to_lowercase()` | `s.toUpper()`, `s.toLower()` | built | The same full mappings and final sigma, from SG's own tables of Unicode 17.0 |
| `c.to_uppercase()` (an iterator) | `c.toUpper()` (a `char`) | built | One character for one: `'ß'.toUpper()` is `'ß'`; the text method gives `"SS"` |
| `c.is_alphabetic()`, `c.is_numeric()`, `c.is_alphanumeric()` | `c.isLetter()`, `c.isNumeric()`, `c.isAlphanumeric()` | built | `isLetter` is the category L, narrower than Alphabetic |
| `c.is_uppercase()`, `c.is_lowercase()`, `c.is_control()` | `c.isUpper()`, `c.isLower()`, `c.isControl()` | built | The categories Lu and Ll, narrower than Rust's derived properties |
| none (the `caseless` or `unicase` crates) | `s.equalsIgnoreCase(t)`, `s.foldCase()` | built | Full case folding, without the Turkic lines |
| none (the `unicode-general-category` crate) | `c.category()`, a `UnicodeCategory` | built | `UppercaseLetter` ... `Unassigned`; `abbreviation()` gives `Lu` |
| `s.nfc().collect::<String>()`, `nfd`, `nfkc`, `nfkd` (the `unicode-normalization` crate) | `s.normalize(NormalizationForm.C)`, `D`, `KC`, `KD` | built | A new `string`; a text already in the form is copied as it is after the quick check |
| `is_nfc(s)`, `is_nfd(s)`, ... (the same crate) | `s.isNormalized(NormalizationForm.C)` | built | The quick check, and the text normalized when it says Maybe |
| the `unicode-segmentation` crate | none | not planned yet | |

## Collections

| Rust | SG | Status | Note |
|---|---|---|---|
| `Vec<T>` | `List<T>` | built | |
| `Vec<&T>`, `Vec<&[u8]>` | `List<read T>`, `List<read byte[]>` | built | Each source is followed on its own, not under one lifetime; `xs.clear()` on a local list lets go of all of them, so one list is filled again in every round of a loop, which a `Vec<&'a T>` cannot be. No `List<ref T>` (1732, 1733) |
| `Vec::new()`, `vec![1, 2]` | `List<int>()`, `List<int> { 1, 2 }` | built | |
| `vec![0u8; n]`, `vec![x; n]` | `List<byte>.repeat(0, n)`, `List<T>.repeat(x, n)` | built | The item is `Copyable`; zeros come from `calloc`, as Rust's do |
| `Vec::with_capacity(n)` | `List<T>.withCapacity(n)` | built | No `reserve` on a list made already |
| `v.push(x)`, `v.pop()`, `v.len()` | `xs.add(x)`, `xs.removeLast()`, `xs.length()` | built | `removeLast` gives a `T?` |
| `v[i]`, `v.get(i)` | `xs[i]`, `xs.get(i)` | built | `get` gives a `T?`; the panic of `xs[i]` names it, where Rust's names nothing |
| `v[v.len() - 1]`, `v[v.len() - n]`, `&v[1..v.len() - 1]` | `xs[^1]`, `xs[^n]`, `xs[1..^1]` | built | C#'s index from the end: `^n` is `xs.length() - n`, with the list found once (`next()[^1]` is one call). It is written only inside `[ ]`: a list, an array, a slice, a text; `^0` is the end, past the last item, so it ends a range. Outside the list it panics naming `^n`. `v.last()`, which gives an `Option`, has no method of its own: `xs.get(xs.length() - 1)` (1921-1926) |
| `v.get(i).copied()` | `int? first = xs.get(i)` | built | A `read T?` is copied into a `T?` where one is written, when `T` is copyable; otherwise `xs.get(i)?.clone()` |
| `unsafe { *v.get_unchecked(i) }` | `xs[i]` | built | No counterpart in a program: `getUnchecked`, which reads without the check in a release build, is for the language's own module only, which reads the table of `Regex`'s DFA with it (1803). Where the index is known to be inside, the C compiler leaves out the check of `xs[i]` by itself (`for i in 0..xs.length()`) |
| `v.extend_from_slice(&s)` | `xs.addAll(s)` | built | For `Copyable` items |
| `dst.copy_from_slice(&src)` | `dst.copyFrom(src)` | built | For `Copyable` items; lengths that differ are a panic, as in Rust. `dst` is a list, an array, a `ref T[]` or a range (`buf[0..n].copyFrom(...)`) (1597) |
| `v.copy_within(k..k + n, 0)` | `v[0..n].copyFrom(v[k..k + n])` | built | The two may be windows of one place: copied as `memmove` copies (1609) |
| `memchr::memmem::find(&bytes, b"\r\n")` | `bytes.indexOf(read "\r\n".asBytes())` | built | Of a list, an array or a slice of bytes, by the search of text |
| `v.sort()`, `v.reverse()`, `v.contains(&x)`, `v.iter().position(...)` | `xs.sort()`, `xs.reverse()`, `xs.contains(x)`, `xs.indexOf(x)` | built | `sort` is stable; a `compare` that gives no single order leaves the items in some order and does not panic, where Rust's `sort_by` may |
| `v.binary_search(&x).ok()` | `xs.binarySearch(x)` | built | `nint?`, the first of equal items, where Rust gives any one of them; the place to insert is `lowerBound` |
| `v.partition_point(\|y\| *y < x)`, `v.partition_point(\|y\| *y <= x)` | `xs.lowerBound(x)`, `xs.upperBound(x)` | built | By the order of the items; with a function of its own (`binary_search_by`, `partition_point` on any test) not yet |
| `[i32; 3]`, `[0u8; 16]`, `[255u8; 16]` | `int[3]`, `byte[16]`, `byte[16](255)` | built | |
| *values of the type `[u8; N]` are too big for the target architecture* | an error: *a `byte[N]` is too big: a value takes at most 2 305 843 009 213 693 951 bytes* | built | Rust's bound is `isize::MAX`; SG's is 2^61 - 1, what Clang allows an array on a machine of 64 bits. Both touch a large frame page by page before it is used (Rust's stack probes, SG's `sg_probe_stack`); SG only where the frame may reach the bottom eighth of its thread's stack |
| `&[T]`, `&mut [T]` | `read T[]`, `ref T[]` | built | `read xs` gives one where a slice is expected |
| `HashMap<K, V>` | `HashMap<K, V>` | built | A hash table with a random key, no order: text and lists hashed with SipHash-1-3 as Rust's, a key of a fixed size (a number, `bool`, `char`) with a folded multiplication, as hashbrown's foldhash (choice 1893) |
| `m.insert(k, v)` (on a new key) | `map.add(k, v)` | built | A key that is there already is a panic; `map[k] = v` changes it |
| `m.get(&k)`, `m[&k]` | `map.get(k)`, `map[k]` | built | `get` gives a `V?` |
| `*m.entry(k).or_insert(0) += 1` | `var count = map.getOrAdd(k, 0)`, then `count += 1` | built | `getOrAdd` gives a `ref V` |
| `m.contains_key(&k)`, `m.remove(&k)` | `map.contains(k)`, `map.remove(k)` | built | |
| `HashMap<Vec<u8>, V>`, `m.get(&buf[a..b])` | `HashMap<List<byte>, V>`, `map.get(buf[a..b])` | built | A `List<T>` is a key when `T` is (1853), looked up by a `read T[]` as `Vec<u8>` is by `&[u8]`; `getOrAdd` copies the slice into a list of its own only when the key is new, which `entry` cannot (it takes the owned key first) |
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
| `struct Config { port: u16 = 80 }` (`default_field_values`, not stable yet), `Config { ..Default::default() }` | `struct Config { int port = 80 }`, `Config {}` | built | No `..` at the literal. A default is what a const can be (found when the program is compiled), `null`, an empty collection or a literal that gives no field |
| `fn connect(host: &str, port: Option<u16>)` called with `None`, or a builder, or a second name (`connect_with_port`) | `void connect(read string host, int port = 80)`, `connect("a")`, `connect("a", 8080)` | built | Rust has no default values of parameters. The last parameters have them; a default is what the default of a field can be, or `CancellationToken.None`; no named arguments |
| `Point { x: 3, ..p }` | `p with { x = 3 }` | built | C#'s notation. `p` is moved, or copied when it is `Copyable`; what the fields written held is dropped there. A struct with a field another module does not see is not copied with `with` there, as in Rust. `..*b` of a `Box` is `b.clone() with { ... }`: `with` takes a struct, not a `T*` or a `shared T` (2212) |
| `&mut self`, `self` | `ref this`, `this` | built | |
| `P::new(1)` | `P.create(1)`, or `P(1)` for short | built | A function without `this` is called on the type |
| `enum Shape { Circle(i32) }` | `enum Shape { Circle(int radius) }` | built | What a variant carries has a name |
| `#[repr(u8)] enum Block { Image = 0x2C, Trailer = 0x3B }` | `enum Block : byte { Image = 0x2C` / `Trailer = 0x3B }` | built | Its variants carry nothing; without a number a variant has one more than the one above, as in Rust. An enum that names no type has no numbers (Rust gives every enum without data some) |
| `block as u8` | `(byte)block` | built | Reads the number where the value is, without moving it |
| `Block::try_from(b)` (written by hand, or by a crate) | `Block.fromValue(b)` | built | A `Block?`; `(Block)b` is an error, since not every number is a variant |
| `#[derive(PartialOrd, Ord)]` on an enum with numbers | `: Comparable` without a `compare` | built | Ordered by the numbers, as Rust's derive. An enum without numbers, and a struct, still write their `compare` |
| `Shape::Circle(2)` | `Shape.Circle(2)` | built | |
| `#[derive(Clone, Copy)]` | `struct P : Copyable` | built | |
| `#[derive(Clone)]` | `struct Config : Clonable` | built | `.clone()` copies field by field, calling a field's own `clone`; `impl Clone` by hand is a `clone(read this)` of the type's own (1818) |
| `#[derive(PartialEq)]` | `: Equatable` | built | Or its own `bool equals(read this, read P other)`; `a.equals(read b)` is `a == b`, the generated one too, as `a.eq(&b)` is in Rust (2213) |
| `#[derive(Hash)]` | `: Hashable` | built | Can be a key of a `HashMap`; the generated hash is not called by name, where Rust's `Hash::hash` feeds a `Hasher` (2213) |
| `impl Ord` | `: Comparable` with `int compare(read this, read P other)` | built | Gives `<` and `sort` |
| `#[derive(Debug)]` | `: Formattable` | built | Written from the fields |
| `trait`, `impl Trait for T` | `interface` | designed | Interfaces of your own are put off; the eight built-in ones are there (`Copyable`, `Clonable`, `Formattable`, `Equatable`, `Hashable`, `Comparable`, `Disposable`, `Closable`), and `unsafe Sendable` |
| generics of your own (`struct Stack<T>`) | none | designed | Put off; `List`, `HashMap`, `HashSet`, `Result`, `Func`, `Task` are generic |
| `impl Drop for T { fn drop(&mut self) }` | `struct T : Disposable` with `void dispose(ref this)` | built | Runs once where the last owner ends, then the fields end; not on a panic or `exit`. Only on a struct for now |
| `drop(x)`, `x.drop()` | none: hand it to a function that takes it by value | built | `x.dispose()` is an error; it runs by itself |
| moving a field out of a `Drop` type (E0509) | an error too | built | `take` and `replace` work, as `mem::take` and `mem::replace` do |
| a `close(self) -> Result` that Rust lets you forget | `struct T : Closable` with `Result<void, E> close(this)` | built | A local that is not closed is a compile error, as for a `StreamWriter` |

## `match`, `if let` and `Option`

| Rust | SG | Status | Note |
|---|---|---|---|
| `match x { A(n) if n > 0 => ..., _ => ... }` | `match x { A(n) when n > 0 => ..., _ => ... }` | built | One arm per line |
| `let w = if n == 1 { "item" } else { "items" };` | `var w = n == 1 ? "item" : "items"` | built | C#'s `?:`; `if` is a statement. Only the arm chosen runs; `if let Some(x) = p { x.a } else { 0 }` is `p is not null ? p.a : 0`, with the names and the narrowing of an `if` (1910-1916) |
| `A \| B` | `A or B` | built | |
| `'a'..='z'` | `'a'..='z'` | built | |
| `n @ A(_)` | none | not in SG | Not decided yet; listed in [Not here yet](../interpreter/docs/not-here-yet.md) |
| `if let Some(c) = x.circle()` | `if x is Circle(r)` | built | The names reach along `&&` |
| `let p = match code { "VIP" => 25, _ => return 0 };` | `var p = match code { ... }` with an arm `_ => return 0` | built | `return` inside an expression: on the right of `??` and as an arm of a `match` (2085) |
| `let Ok(v) = r else { return };` | `if r is not Ok(v) { return }`, then `v` | built | |
| `Option<T>` | `T?` | built | `T` itself is never `null` |
| `Option<&[u8]>`, `fn header(&self, name: &str) -> Option<&[u8]>` | `read byte[]?`, `read byte[]? header(read this, read string name) from this` | built | A window, of length -1 when it is missing; only `read` (1774) |
| `None`, `Some(x)` | `null`, `x` | built | |
| `if let Some(v) = x` | `if x is not null` | built | `x` is a `T` inside |
| `x.unwrap_or(d)` | `x ?? d` | built | |
| `x.expect("...")` | `x ?? panic("...")` | built | |
| `x?` on an `Option` | `try x` | built | |
| `x.as_ref().map(\|p\| &p.name)`, `x.as_ref().and_then(\|p\| p.next.as_ref())` | `x?.name`, `x?.next?.name` | built | The rest of the chain does not run on `null`; a field that is not `Copyable` is lent (`read string?`), one that is copied (`int?`). `x?.name is not null` narrows `x` |
| `if let Some(p) = &mut x { p.items.push(1) }` | `x?.items.add(1)` | built | `x` is lent as `ref` when the chain calls a method that changes it |
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
| `&&mut T` only reads | a `ref` field reached through `read this` only reads | built | Nor is it given on as a `ref`, copied into a variable or put into a `ref` field (1592) |
| implicit reborrow: `wipe(xs)` with `xs: &mut Vec<T>` | `wipe(xs)` with `ref List<T> xs` | built | Lends `xs` again to be changed: no other borrow of it may be used after the call, nor given in the same call (1592) |
| drop check: a value with `Drop` that borrows outlives what it borrows | the same for a `Disposable` value | built | Its `dispose` at the end of the block is a use of the borrow; an owner declared after it is an error |
| two-phase borrows: `v.push(v.len())` is fine, `f(&v[0], g(&mut v))` is not | the same | built | What a part of a call lends lasts until the call runs; a later part may read it, not change or move it. The same for every value made of parts: a struct, `with`, a variant, a list, `==`, `??`, `$"..."` (1592) |
| `f(&mut v[0..2], &v[2..4])` is an error | `f(ref xs[0..2], xs[2..4])` goes | built | Two windows of one place go together when they share no item (2011); ends written as numbers are told apart when the program is compiled, `f(ref xs[0..2], xs[1..3])` is an error (2013) |
| `let (a, b) = v.split_at_mut(k)` | `var a = ref xs[..k]`, `var b = ref xs[k..]` | built | No pair: two windows of one place go together when they share no item, checked when the second is taken, a panic if they do; for `..k` beside `k..` the compiler of C leaves the check out (2011, 2012) |
| `let [a, b] = v.get_disjoint_mut([i, j])?` | `swap(ref xs[i], ref xs[j])` | built | An item is a window of one; the same item twice is a panic, where Rust gives an error (2014) |
| `let (first, rest) = v.split_first_mut()?` | `var first = ref xs[0]`, `var rest = ref xs[1..]` | built | An empty list is a panic of the index, as `xs[0]` is (2014) |
| `Box<T>`, `Box::new(x)` | `T*`, `new x` | built | `==` and `<` compare what two pointers own, as Rust's `PartialEq` for `Box` does; `*count + 1` has no form, and a number needs no `new` |
| `Option<Box<T>>` | `T*?` | built | |
| `Rc<T>`, `Arc<T>` | `shared T` | built | One type; the count is always atomic. What it holds is only read |
| `Rc::new(x)`, `Rc::clone(&a)` | `new shared x`, `share a` (or `a.clone()`) | built | |
| `Weak<T>`, `Rc::downgrade(&a)`, `w.upgrade()` | `weak T`, `weak a`, `share w` | built | `share w` gives a `shared T?`. A leak names the field to make `weak`, one that holds one value (a child's `parent`, as Rust's book has it) before a collection |
| `Rc::ptr_eq(&a, &b)` | `same(a, b)` | built | `==` compares what they point to |
| `Rc::make_mut(&mut a)` | `a.makeUnique()` | built | A `ref T` of the value when `a` is its only owner and no `weak` looks at it, else of a copy `a` owns from then on; `T` is `Clonable` (1813). `Rc::get_mut` has no counterpart |
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
| `zeroize::Zeroize::zeroize(&mut key)` (a crate) | `Memory.zeroSecurely(ref key)` | built | Takes a list, an array or a window of plain data, without `unsafe`; copies made before (a list that grew) are not wiped, as with the crate |
| `slice::from_raw_parts(p, n)` | `Memory.view(p, n)` | built | Inside `unsafe`; a `read T[]` that borrows the variable holding `p`, so it does not outlive it (Rust's lifetime there is unbounded). A count below 0 and `null` with items are panics |
| `slice::from_raw_parts_mut(p, n)` | `Memory.viewRef(p, n)` | built | Inside `unsafe`; a `ref T[]` that borrows the variable holding `p` as `ref`, so the pointer is not read and no second window is made from it while it is used |
| `p.read_unaligned()`, `p.write_unaligned(v)` | `Memory.readUnaligned(p)`, `Memory.writeUnaligned(p, v)` | built | Inside `unsafe`. Reaching a misaligned item with `p[i]` is a panic in the interpreter and a debug build, as Rust's debug checks of `*p` are, and costs nothing in a release build |
| `mem::size_of::<T>()`, `mem::align_of::<T>()` | `Memory.sizeOf<T>()`, `Memory.alignOf<T>()` | built | For a type of plain data, a `nint`; `offset_of!` is not in SG yet. A struct with no fields is 1, as in C++, where Rust has 0 |
| `#[repr(C)] union Bits { word: u32, number: f32 }` | `union Bits { uint word` / `float number }` | built | Built with one field, a field written whole without `unsafe` and read inside it, as in Rust; a field is not lent, and a union is not `Equatable` or `Formattable` by its fields |
| bit fields through bindgen's accessors, or the `bitfield` crate | `uint version : 4` in a struct | built | Rust has no bit fields; SG lays them out as C does on the machine (Windows' rule included). A value that does not fit is the rule of overflow; a bit field has no address |
| `#[repr(C, packed)]` | `packed struct Header { ... }` | built | A field is copied out, not lent (Rust's error E0793); no bit fields in it, and `packed(N)` is not in SG yet. `packed union` is taken too (alignment 1) |
| `#[repr(C, align(64))]` | `align(64) struct Slot { ... }` | built | Kept at a multiple of 64 on the heap too (`new`, `List`, `shared`) |
| `bytemuck::pod_read_unaligned::<Header>(&bytes)`, `zerocopy::FromBytes` | `Header.fromBytes(bytes)` | built | No `unsafe`: only a type that every pattern of bits is a value of; a window of another length is a panic |
| `extern "C" { fn ... }`, `#[link(name = "z")]` | `extern "z" { ... }` | built | Every call inside `unsafe`; `extern { }` is the C library itself. C names as C writes them; private to the module unless `public` is written on a function |
| `c_int`, `size_t`, `*const T`, `*mut T` in a signature | `int`, `nuint`, `read T` or `raw T`, `ref T` or `raw T` | built | `ref` and `read` last for the call; `raw T` when C keeps the address. No `char` (a byte is `byte`) |
| `c_long`, `c_ulong` | `CLong`, `CULong` | built | Rust's are other names of `i64` or `i32` after the system, so code that compiles on Linux may not on Windows. SG's are types of their own: an `int` becomes one and one becomes a `long` on their own, a `long` crosses with a cast, a number written for one fits in 32 bits, and they have no arithmetic (`(long)r + 1`) |
| `#[repr(C)] struct`, given and returned by value | a struct of plain data, given and returned by value | built | Every struct of plain data is laid out as C lays it out; no mark is written. Unions, bit fields, `packed` and `align` are below. A function of the program's own C file does not take a struct by value yet |
| `#[cfg_attr(windows, link(name = "zlib"))]` and `#[cfg_attr(not(windows), link(name = "z"))]` | `extern "z", "zlib", "libz" { ... }` | built | The names a library has on the systems (vcpkg's zlib is `libz.lib` since 1.4); the first one the C compiler finds is linked, asked by linking an empty program with it |
| `cargo:rustc-link-search=native=/opt/lib` in a `build.rs`, `-L /opt/lib` to `rustc` | `sg build main.sg -L /opt/lib`, `sg -L /opt/lib main.sg`, `sg test ... -L /opt/lib` | built | Also written into the program as an `rpath`, so it runs without `LD_LIBRARY_PATH`, which Cargo does not do; a folder others may write into is an error. On Windows, which has no `rpath`, `sg build` copies the DLLs it linked from there, and those they need, beside the program, which Cargo leaves to the user |
| `extern { type sqlite3; }`, an opaque `#[repr(C)] struct` | `extern "sqlite3" { struct sqlite3 }` | built | Only as `raw sqlite3`; `ref raw sqlite3` is C's `sqlite3**` |
| the `cc` crate and a `build.rs` that compiles a C file | `extern "./native.c" { ... }` | built | Compiled in the same unit as the program, so a `static inline` function is inlined; no build script |
| `extern { static mut optind: c_int; }` | `extern { int optind }` | built | Read and written inside `unsafe`, as an item of raw memory |
| `#[no_mangle] pub extern "C" fn compare(a: *const c_void, b: *const c_void) -> c_int` | `export int compare(raw void a, raw void b) { ... }` | built | C types, and the values of SG of the next rows; the name C sees is the name written, one in the program, and not one C's library or the runtime has, which `#[unsafe(no_mangle)]` may replace. A panic inside ends the process; any thread may call it. Called from SG by its name, with no `unsafe` |
| `extern "C" fn calc_new() -> Box<Calculator>`, `fn calc_free(c: Option<Box<Calculator>>)`, `fn calc_eval(c: &mut Calculator, ...)` | `export Calculator* calc_new()`, `export void calc_free(Calculator*? c)`, `export int calc_eval(ref Calculator c, ...)` | built | A struct of SG that is not plain data, or an enum, goes to C by its address, with no `unsafe`; C sees an opaque `typedef struct Calculator Calculator;`. `NULL` where none is taken is a panic; the interpreter also catches an address it did not give, of another type, freed already, or lent against the rules of borrowing |
| `extern "C" fn f(text: *const c_char)` with `CStr::from_ptr(text).to_str()?` | `export int f(read CString text)` with `try string.fromCString(text)` | built | C's `const char*`; copied and checked, with no `unsafe` |
| `crate-type = ["staticlib"]`, `["cdylib"]`, and cbindgen for the header | `sg build folder --library`, `--shared` | built | The header comes with the library; only the `export` functions are seen, a panic aborts, a shared library is not unloaded. On Windows `x.lib`, and `x.dll` with `x.dll.lib`, Rust's names. `examples/calclib` |
| `#![no_std]`, `#![no_main]`, built with `--target riscv64gc-unknown-none-elf` | `sg build folder --freestanding --target riscv64-unknown-elf` | built | An object the start code is linked with; no `main`, the machine calls the `export` functions. What needs a system is a compile error where it is written, also behind a function of the language's own module; `HashMap` is one (Rust's `alloc` has no `HashMap` either). 64-bit machines only. `examples/bare` |
| `#[panic_handler] fn panic(info: &PanicInfo) -> ! { ... }` | `void onPanic(read string message) { ... }` | built | Given the message as a program with a system writes it, place and hint included; when it returns, the processor waits for ever, as `-> !` asks of Rust's |
| `#[global_allocator] static A: MyAlloc;` with `unsafe impl GlobalAlloc` (`alloc(layout)`, `dealloc(ptr, layout)`) | `raw byte allocate(nuint size, nuint align)`, `void release(raw byte block, nuint size, nuint align)` | built | Told the size and the alignment, as `Layout` tells them. Without them nothing allocates: a statement that would is a compile error, where Rust without `alloc` has no `String` or `Vec` at all |
| `compiler_builtins` with `mem` (`memcpy`, `memset` for `no_std`) | nothing written | built | Weak functions of the object, `__divti3` and its kin too; the machine's own take their place |
| `#[riscv_rt::entry] fn main() -> ! { ... }`, with `riscv-rt`'s start and linker script | `export void kernelMain() { ... }`, called by `_start` in a C file beside the kernel (`extern "./machine.c"`), linked with the kernel's own linker script | built | The start of the core and the vector of traps are a few lines of assembly inside C, not a crate. `examples/kernel` |
| `static TICKS: AtomicU64`, changed in the handler of an interrupt | `static atomic ulong Ticks = atomic 0` | built | In the program's data from its start, so the handler reads it with no check; the rest of the kernel's state still reaches the handler through `mscratch`, as xv6 keeps a pointer for each core |
| `#[riscv_rt::core_interrupt(Interrupt::MachineTimer)] fn timer() { ... }` | `export void onTrap(raw void state, ulong cause, ulong pc)`, which tells the cause itself | built | One function for every trap, as the core has one vector |
| `riscv::register::mie::set_mtimer()`, `riscv::asm::wfi()` | `static inline` functions of the C file with `csrs` and `wfi`, called inside `unsafe` | built | Inlined, as the crate's are |
| `core::ptr::write_volatile(0x1000_0000 as *mut u8, b)` | `unsafe Memory.writeVolatile(uart[0], b)` with `var uart = unsafe (raw byte)0x1000_0000` | built | The devices of the kernel are driven in SG |
| `extern "C" fn(*const c_void) -> c_int`, `Option<extern "C" fn(...)>` | `CFunc<raw void, int>` | built | May be `null`, as a pointer of C may; the name of an `export` function or of a function of C is one. Every call through it is inside `unsafe`; a `null` one is a panic, not a crash |
| `transmute::<*mut c_void, extern "C" fn(...)>(dlsym(...))` | `unsafe (CFunc<int, int>)dlsym(...)` | built | From a `raw void`; back with `(raw void)f` |
| `&mut state as *mut State as *mut c_void`, then `&mut *(data as *mut State)` | `unsafe Memory.addressOf(ref state)`, then `unsafe (ref State)data` | built | That the value lives and stays where it is while C keeps the address is the program's promise; the interpreter panics on a wrong type, a value that ended or moved, and an address it did not give |
| a closure handed to C through a trampoline | an `export` function and a state reached through `void*` | not in SG | There are no closures; what a closure would capture is a struct whose address C gives back |
| `std::io::Error::last_os_error()` | `IoError.fromCode(C.lastError())` | built | The system's last error: `errno` on Linux and macOS, `GetLastError` on Windows; `C.setLastError(n)` sets it. Each call of C keeps it as it left it, beside `errno` |
| `#[cfg(windows)] extern "system" { fn Sleep(ms: u32); }` | `extern { void Sleep(uint ms) }`, called in `if OperatingSystem.isWindows() { ... }` | built | Declared on every system; a function of C called only in the branch of another system is not linked there |
| `std::io::Error::from_raw_os_error(errno())`, `errno::set_errno` | `IoError.fromErrno(C.errno())`, `C.setErrno(n)` | built | No `unsafe`; `errno` is read right after the call it belongs to. Rust has one function for both numbers, which on Windows differ; `IoError.fromCode(C.errno())` is an error |
| `CString::new(s)?`, `c.as_ptr()` | `try CString.from(s)`, `read c` as an argument | built | A zero byte inside is a `TextError` at its position |
| `CStr::from_ptr(p).to_str()?.to_owned()` | `try unsafe string.fromCString(p)` | built | Copies and checks the UTF-8 |
| `v.as_mut_ptr()`, `v.as_ptr()` | `v.pointer()` | built | Only as an argument of a C function, on a list, an array or a slice of plain data; it counts as a `ref` of `v` for the call |
| `libc::malloc` / `libc::free` beside `Box` | `Memory.allocate` / `Memory.free` | built | One allocator: what C gives with `malloc` goes back with `Memory.free` |
| a safe wrapper module around a `-sys` crate | a module whose functions of C are private | built | `examples/zlib`, `examples/sqlite` and `examples/curl`: their users write no `unsafe` |
| `extern "C" { fn printf(format: *const c_char, ...) -> c_int; }` | `extern { int printf(read CString format, ...) }` | built | As Rust: an `f32`, a small integer and a `bool` are errors after `...`, with the cast as the hint; a number written without a type is an error too, where Rust takes an `i32`. No `va_list` and no function of SG that takes `...` (Rust's `c_variadic` is unstable) |
| `rusqlite::Connection::open(path)?`, `conn.execute(...)`, `conn.prepare(...)` | `Database.open(path)`, `db.execute(...)`, `db.prepare(...)` of `examples/sqlite` | built | An example module, not a library of the language; a `Statement` may outlive its `Database`, as SQLite allows with `sqlite3_close_v2` |
| `curl::easy::Easy` with `write_function`, or `reqwest::blocking::get(url)?` | `Client.open()`, `client.get(url)`, `client.post(url, body, type)` of `examples/curl` | built | An example module; only `http` and `https`, certificates always checked, a limit on what a response brings. A 404 is a response, not an error. No async |
| `spawn_blocking` around a call of C that waits | the call itself | built | A call of C holds up only its own thread; in the interpreter the other tasks run while it waits |

## Tasks and locks

| Rust | SG | Status | Note |
|---|---|---|---|
| `Arc::new(Mutex::new(v))` on a value you already have | `new shared locked v` | built | `v` is moved in; a value is locked from where it is made, never later in place |
| `thread::spawn(move \|\| work(x))` | `Task.run(work, x)` | built | The function and what it is given are written apart. A thread of 1 GB of stack on every system, taken a page at a time as it is used, as `main`'s, where Rust's is 2 MB and `main`'s 8 MB (1821); 8 MB where the system gives no such thread. Windows included (`_beginthreadex`), and a lock is an `SRWLOCK` there, as Rust's `Mutex` is |
| `std::thread::available_parallelism()` | `Task.processorCount()` | built | An `nint`, at least 1, counted the same way; on Windows the process's affinity too |
| `thread::Builder::new().stack_size(n)` | nothing to write | built | Every task has 1 GB; a stack that overflows is `panic: stack overflow`, exit code 1, where Rust's says `thread '...' has overflowed its stack` and aborts (1815); the interpreter says the same, naming the call, about 380,000 calls deep (1823) |
| `JoinHandle<R>` | `Task<R>` | built | |
| `h.join().unwrap()` | `t.wait()` | built | A panic stops the process, so there is nothing to unwrap |
| dropping a `JoinHandle` (the thread goes on) | a compile error | built | Every task is waited for on every path |
| `Send`, `Sync` | the thread rule | built | No borrow, no `shared checked`, no `Task` crosses, also inside fields |
| `Arc<Mutex<T>>` | `shared locked T` | built | |
| `let mut c = m.lock().unwrap(); *c += 1;` | `lock m as c { c += 1 }` | built | What it holds is reached only inside the block |
| two locks, one after the other | `lock a as x, b as y { }` | built | Taken in a fixed order; a lock inside a lock is an error |
| a lock held across a wait | a compile error | built | Nothing in a `lock` block waits, so locks and waits cannot deadlock |
| lock poisoning | none | built | A panic stops the process |
| `let g = cv.wait_while(m.lock().unwrap(), \|q\| q.is_empty()).unwrap();` with `cv.notify_one()` where it changes | `lock m as q when q.length() > 0 { ... }` | built | No `Condvar` and no notify: every `lock` block on the value that ends wakes those that wait. The condition is computed under the lock, may not read an atomic or the clock, and the task that is left the only owner panics instead of waiting for ever |
| `tokio::sync::Notify` or an async `Condvar` beside a `Mutex` | `await lock m as q when ... { }` | built | The task waits and its thread goes on; the same `locked` value serves async and blocking code |
| `let (tx, rx) = mpsc::sync_channel(16);` | `var rx = Channel<int>(16)`, `var tx = rx.sender()` | built | No tuple: the channel is the end that receives and makes its senders. Always with room, at least one; no channel without a limit (`mpsc::channel()`) and none with no room (`sync_channel(0)`) |
| `tx.clone()`, `drop(tx)` | `tx.clone()`, the sender ends where its owner ends | built | The last sender to end closes the channel; there is no `close`, so none to forget |
| `tx.send(x).unwrap()` | `_ = tx.send(x)`, or `if !tx.send(x) { return }` | built | `false` when no end receives any more, and the item is dropped; Rust gives it back in the error |
| `rx.recv()`, `for x in rx` | `rx.receive()` (a `T?`, `null` at the end), `for x in rx` | built | The loop does not take the channel; one while a sender this function made of it lives is a compile error, Rust's best known mistake with channels |
| `Arc<Mutex<Receiver<T>>>` for a pool of workers, crossbeam's `rx.clone()` | `rx.clone()` | built | Several receivers share the items, each going to one of them. Rust makes `(tx, rx)` together; SG makes the receiver first, and its first `sender()` must come before the receiver is cloned or given away, a compile error otherwise |
| `rx.recv_timeout(d)`, `send_timeout` | `rx.receive(read token)`, `tx.send(x, read token)` with `CancellationSource.after(ms)` | built | A `Result` whose error is `Cancelled`; an item that is there is taken all the same |
| `tokio::sync::mpsc`, `rx.recv().await`, `tx.send(x).await` | `await rx.receiveAsync()`, `await tx.sendAsync(x)` | built | The same channel serves async and blocking code |
| crossbeam's `select! { recv(a) -> x => ..., send(b, v) -> r => ... }`, tokio's `select!` over `recv()` and `send()` | `select { a.receive() as x => ...  b.send(v) => ...  ended => ... }`, `await select { ... }` in async code | built | A random arm of those ready, as both do; an arm whose channel has ended is never chosen (crossbeam gives it an `Err`), and `ended` runs once all have; a `send` arm is chosen only when the item went in, and a variable sent keeps its value in the other arms. It gives a value as a `match` does |
| threads that wait on channels for each other, which hang for ever | the panic "every task waits, and nothing is left that could end a wait", at the wait | built | Rust and tokio leave such a program hanging; SG counts the threads that are awake and the waits for what is outside the tasks (a delay, a socket, input and output), as Go does, interpreted and compiled. A task that goes on without a wait pays nothing |
| crossbeam's `select!` with `default(d)`, tokio's `select!` with `sleep(d)` | `select (read token) { ... cancelled => ... }`, `CancellationSource.after(ms)` | built | No `default` arm: a time limit is a token |
| `rx.try_recv()` | none | not in SG | A loop that asks without waiting holds its thread |
| `AtomicI64::new(0)`, `AtomicBool`, `AtomicPtr<T>` | `atomic 0` of the type `atomic long`, `atomic bool`, `atomic raw T` | built | Any integer type, `bool`, `float`, `double` and a raw pointer; as large as its type and aligned to its size, in a struct, a list, an array or raw memory |
| `Arc<AtomicI64>` | `shared atomic long`, made with `new shared atomic 0` | built | Its methods work through `read` and `shared`, as Rust's through `&self` |
| `a.load(Ordering::SeqCst)`, `a.store(v, Ordering::Release)` | `a.load()`, `a.store(v, MemoryOrder.Release)` | built | Sequentially consistent unless an order is written; an order the operation cannot keep is a compile error, where Rust panics at run time |
| `a.fetch_add(1, Ordering::Relaxed)` | `a.fetchAdd(1, MemoryOrder.Relaxed)` | built | Gives the value before, as Rust's; checked for overflow in a debug build, where Rust wraps; `wrappingFetchAdd` wraps in every build. On `float` and `double` too |
| `a.compare_exchange(old, new, s, f)` | `a.compareExchange(old, new, s, f)` | built | A `Result<T, T>`, as Rust's; with one order, the order on failure is taken from it; `compareExchangeWeak` too |
| `fetch_max`, `fetch_min`, `fetch_nand`, `fetch_update` | none | not in SG | A loop of `compareExchangeWeak` |
| `atomic::fence(o)`, `compiler_fence(o)` | `Atomic.fence(o)`, `Atomic.compilerFence(o)` | built | |
| `std::hint::spin_loop()` | `Atomic.spinLoop()` | built | In the interpreter, where one task runs at a time, it lets the others run |
| `unsafe impl Send for T {}`, `unsafe impl Sync for T {}` | `struct T : unsafe Sendable` | built | The writer's promise that what its raw pointers reach is safe for several tasks; one word, as SG has one thread rule |
| `ptr::read_volatile(p)`, `ptr::write_volatile(p, v)` | `unsafe Memory.readVolatile(p[0])`, `unsafe Memory.writeVolatile(p[0].status, v)` | built | Takes a place in raw memory, so a field of a block of registers is reached as C's `regs->status` |
| C#'s `Interlocked.Increment(ref x)`, `Interlocked.Add(ref x, n)` | `_ = x.fetchAdd(1)`, `x.fetchAdd(n)` | built | C#'s `Add` gives the value after, `fetchAdd` the one before, as C and Rust; the value is `atomic`, so it is never read or written without an atomic operation |
| C#'s `Interlocked.CompareExchange(ref x, value, comparand)` | `x.compareExchange(comparand, value)` | built | The expected value first, as C and Rust have it |
| C#'s `Volatile.Read`, `Volatile.Write`, `volatile` fields | `x.load(MemoryOrder.Acquire)`, `x.store(v, MemoryOrder.Release)` | built | C#'s `volatile` orders reads and writes between threads; SG's `Memory.readVolatile` is C's, for devices, and orders nothing |
| `static mut`, `thread_local!` | none | not in SG | No global that is assigned freely and none for each thread: what tasks share is in the arguments of `Task.run` or in a `static` that is read only, atomic or locked |
| `async fn`, `.await` | `async int f()`, `await t`, `await f(x)` | built | C#'s behaviour: a call starts the task at once; a call alone on a line is an error. Compiled as Rust compiles it ("Async, the direction") |
| `tokio::spawn(f(x))` | `var t = f(x)` (no `await`) | built | What it is given follows the thread rule; `await f(read x)` can borrow, as an awaited future does |
| `tokio::time::sleep(d).await` | `await Task.delay(ms)` | built | On Windows the clock sleeps on a timer of high resolution, so a delay of 1 ms waits about 1 ms, not a tick of 16 ms |
| `block_on(f())` | `f().wait()` in code that is not async | built | `wait` in an async function is an error |
| `spawn_blocking(work)` | `await Task.run(work, x)` | built | A call that holds the thread in an async function is an error, followed through the calls |
| `Box::pin(f(n - 1)).await` | `var t = f(n - 1)`, then `await t` | built | An async function that awaits itself is an error |
| `async fn` in an `impl` | `async int total(read this)` | built | A method that lends `this` is awaited where it is written; one that takes `this` by value may start as a task. `create`, `format`, `equals`, `hash` and `compare` are not async |
| `CancellationToken::new()` (tokio-util) | `CancellationSource()` | built | The source cancels, its tokens look; `CancellationSource.after(ms)` cancels itself |
| `token.clone()` | `source.token()`, `token.clone()` | built | A token cannot cancel; a source has one owner, or is `shared` to be cancelled from another task |
| `token.child_token()`, then a time limit of its own | `CancellationSource.linked(read token)`, then `cancelAfter(ms)` | built | Cancelled with the token's source and by itself; one parent (1775) |
| `token.cancel()` | `source.cancel()` | built | Through a `read` borrow, as `cancel(&self)` |
| `Option<&CancellationToken>`, `None` for no token | `read CancellationToken token = CancellationToken.None`, `token.canBeCancelled()` | built | `false` for `None` (C#'s `CanBeCanceled`, 1785): a library links a source to the token only when it can cancel |
| `tokio::time::timeout(d, read)` around each read, `sleep.as_mut().reset(deadline)` | `source.cancelAfter(ms)` before the read, `receiveAsync(ref buffer, read token)` | built | One source for a connection, its time set again before each read; a later time is one atomic write. The read gives `IoError.Cancelled` when it runs out |
| `token.is_cancelled()` | `token.isCancelled()`, `try token.check()` | built | `check` gives `Error(Cancelled { })`, which `try` turns into `IoError.Cancelled(...)` |
| `select! { _ = token.cancelled() => ..., _ = sleep(d) => ... }` | `await Task.delay(ms, read token)` | built | Gives `Error(Cancelled { })` when the token is cancelled first |
| dropping a future to cancel it | none | not in SG | A task stops only where it looks at its token; a cancel ends a wait for the network or for a line of the input at once |
| `Vec<JoinHandle<T>>`, `JoinSet<T>` | `List<Task<T>>` | built | Kept only where a task is kept; its methods are `add` and `length`, and its tasks are taken out only by `Task.whenAll` and `Task.whenAny` |
| `join_all(handles).await` | `await Task.whenAll(tasks)` | built | What each gave, in the order of the list; an error is an item of the list, none is lost. `Task.whenAll(tasks).wait()` in code that is not async |
| `set.join_next().await` | `await Task.whenAny(ref tasks)` | built | What the first task to end gave, taken out of the list; nothing is cancelled. A `while tasks.length() > 0` loop around it empties the list, as the compiler sees |
| a `JoinSet` dropped (its tasks aborted), `try_join_all` | a way out with an error (`try await Task.whenAny(ref tasks)`) | built | A list of tasks that is not emptied is a compile error. A way out with an error lets what is left in it go, then waits for it: a wait that can say so ends with `Cancelled` and the task goes out by its own way of an error, where Tokio's abort stops a task at any `.await` without a word; a delay without a token runs on (1691-1693) |
| `JoinSet<()>` (or `TaskTracker`) for the connections of a server, `set.spawn(handle(conn))` | `var group = TaskGroup()`, `group.add(handle(conn))` | built | A task that ends is let go of at once, with its memory; a group takes tasks that give nothing, so the error of a connection is dealt with in its function. Waited for as a task is: `await group.whenAll()` |
| `Arc<Semaphore>` with `acquire_owned()` before `accept`, the permit moved into the task | `TaskGroup(limit)`, `await group.whenRoom()` before `acceptAsync` | built | The group counts its tasks itself: no permit to carry. `whenRoom(read token)` gives `Cancelled` when the server is told to stop |
| `tracker.close(); timeout(grace, tracker.wait()).await`, then `abort_all()` | `await group.whenAll(read grace)` | built | When `grace` is cancelled the tasks left are let go and still waited for: each goes out by its own error at a wait, not cut at any `.await` |
| `tokio::fs::read_to_string(p).await` | `await File.readTextAsync(read path)` | built | Each function of `File` that reads or writes a whole file has an `Async` version; a token may come last. Compiled, the work runs on a thread of input and output, as tokio's `spawn_blocking` does it |
| `reader.read_line(&mut line).await` | `await reader.readLineAsync(ref line)` | built | Also `readTextAsync`, `readBytesAsync`, on `Stdin` too; a line the block already holds is read in place, as tokio's `BufReader` does it |
| `BufReader::new(tokio::fs::File::open(p).await?)` | `try await File.openTextAsync(read path)` | built | Also `createTextAsync` and `openAppendAsync`, which give a `StreamWriter` |
| `tokio::fs::metadata(p).await`, `remove_file`, `rename`, `copy`, `create_dir_all`, `read_dir` | `await File.infoAsync(read p)`, `File.deleteAsync`, `moveAsync`, `replaceAsync`, `copyAsync`, `existsAsync`, `createTempAsync`, `Directory.createAsync`, `createAllAsync`, `deleteAsync`, `deleteAllAsync`, `syncAsync`, `listAsync`, `existsAsync`, `createTempAsync` | built | On a thread of input and output, as tokio's are; in an async function the calls without `Async` are a compile error, where `std::fs::metadata` in a tokio task compiles and holds the thread (1871). On Linux a stat, an open and a read are first tried where the kernel will not wait, and are handed over only when it would (1872); tokio hands every one over |
| `writer.write_all(b).await`, `writer.flush().await` | `await writer.writeLineAsync(x)`, `writeAsync`, `writeBytesAsync`, `flushAsync()` | built | What the block of 64 KB can take is written in place, as tokio's `BufWriter` does; a full block goes out on a thread of input and output. On `Stdout` and `Stderr` too, in order with `print` |
| `writer.shutdown().await` | `try await writer.closeAsync()` | built | Takes the writer; a local writer left open is a compile error (`Closable`), as without async |
| a `BufWriter` dropped without `flush` (what it holds is lost) | a writer dropped in an async function | built | Written and closed on a thread of input and output, awaited where it is dropped: Rust has no async drop |
| tokio, async-std, smol | one built-in runtime | built | A pool of threads, one for each core, and a clock for delays; only a program with tasks carries them. Input and output the async way runs on threads of their own, started as they are needed (up to 512, as tokio's blocking threads), and the network on epoll, kqueue or an I/O completion port, one loop for each thread of the pool, which waits on its own when it has nothing to do and keeps the sockets first waited for there (tokio has one driver, waited on by one thread at a time); one idle thread at a time is woken to look for work, as in tokio; a socket goes on the loop once, edge-triggered, and its tasks wait in its slot, as tokio's registration; each thread has a ring of 256 tasks that the others steal half of, and a slot for the task just woken, as tokio's `multi_thread` scheduler. Unlike tokio: a thread that looks for work takes from the slots of the others too, and the one that finds work wakes the next only if work is left; the clock's thread looks at the loop of a thread busy with one task, as Go's `sysmon` does |

## Modules

| Rust | SG | Status | Note |
|---|---|---|---|
| `mod stock;` and a file | a folder `stock/` | built | The files of a folder see each other |
| `use stock::*` | `use stock` | built | Brings in the `public` names |
| `use shop::stock` | `use shop.stock` | built | |
| `stock::total(&xs)` | `stock.total(read xs)` | built | The full name works without `use` |
| `pub` | `public` | built | On a type, a function, a field, a method |
| `pub(crate)` | a folder `internal/` | built | As Go's: a module in a folder `internal` is named only from inside the folder that holds it (1561); no third level of `public` |
| The prelude (`Option`, `Vec`, ...), shadowed by what is imported | the types of `std` (`Log`, `Json`, ...), seen everywhere | built | A type of the module or of a module named with `use` comes first; `std.Log` reaches the language's |

## Files and input

| Rust | SG | Status | Note |
|---|---|---|---|
| `fs::read_to_string(p)` | `File.readText(p)` | built | `Result<string, IoError>` |
| `fs::read(p)`, `fs::write(p, b)` | `File.readBytes(p)`, `File.writeBytes(p, b)` | built | |
| `BufReader::new(File::open(p)?)` | `File.openText(p)` | built | A `StreamReader` |
| `reader.read_line(&mut s)`, `reader.lines()` | `reader.readLine(ref line)`, `reader.lines()` | built | |
| `reader.read_until(b'\n', &mut bytes)` | `reader.readLineBytes(ref line)` | built | A `List<byte>` without its `\n` or `\r\n`, none of its bytes checked; `readLineBytesAsync` too |
| `BufWriter::new(File::create(p)?)` | `File.createText(p)` | built | A `StreamWriter` |
| `writer.write_all_vectored(&mut [IoSlice::new(a), IoSlice::new(b)])` on a `BufWriter` | `try writer.writePieces(read pieces)` | built | `pieces` a `List<read byte[]>`, every byte written as `writeBytes` of each would; what the block has room for is copied into it, more goes out after it with one `writev` on Linux and macOS, as `BufWriter::write_vectored` does; `writePiecesAsync` in an async function |
| `OpenOptions::new().write(true).create(true).open(p)?` | `File.openWrite(p)` | built | A `StreamWriter` from the start, over what the file holds |
| `OpenOptions::new().read(true).write(true).open(p)?`, then reads and writes of the one `File` | `var out = try File.openUpdate(p)`, `var reader = try out.openReader()` | built | The file must be there and is not emptied. No type that reads and writes: a writer, and readers of the same open file, each with a place of its own. The reader sees what the writer has flushed, and lets go of what its block holds when it seeks, as a stream of C opened with `"r+"` does (2073, 2074) |
| `file.seek(SeekFrom::Start(n))?` | `try reader.seek(SeekFrom.Start(n))`, `try writer.seek(...)` | built | `End(n)` and `Current(n)` too; gives the new place as a `long`. `Start` holds a `long`, and a place before the start is `InvalidInput`. A reader moves inside its block without the system for `Start` too, where `BufReader` does only for `seek_relative`; a writer writes what it holds first, as `BufWriter` does |
| `file.stream_position()?` | `try reader.position()`, `try writer.position()` | built | Where the next read or write starts; nothing moves, and a writer writes nothing (what it holds is counted). `Stdin`, `Stdout`, `Stderr` and the reader of a connection have none, a compile error; a pipe and a writer of `openAppend` are `NotSeekable` (2075) |
| `file.sync_data()?`, `file.sync_all()?` | `try writer.syncData()`, `try writer.sync()` | built | The writer's block is written first, as `BufWriter::flush` then `get_ref().sync_data()`; the same calls of the system, `F_FULLFSYNC` on macOS. A sync that failed stays the writer's error (1852) |
| `file.set_len(n)?` | `try writer.setLength(n)` | built | What the writer holds is written first; where it stands does not move, as with `set_len` |
| `File::open(dir)?.sync_all()?` | `try Directory.sync(path)` | built | Nothing on Windows, where Rust's would fail to open a folder |
| `eprintln!("{path}: {e}")` of an `io::Error` | `printError($"{path}: {cause}")` | built | `not found`, `invalid text at byte 812`: a sentence in lower case, as Go's; Rust writes the system's own words and the code (`No such file or directory (os error 2)`) |
| `ErrorKind::AddrNotAvailable` | `IoError.AddressNotAvailable` | built | `address not available`: an address not of the machine, or no local port left to connect from |
| (none: `File::open` of a FIFO waits for a writer) | `IoError.NotAFile` | built | `not a file`: `File.openRegular` met a pipe, a device or a socket and did not open it (1873) |
| `ErrorKind::NotSeekable` | `IoError.NotSeekable` | built | Also a writer of `openAppend`; `Stdin.seek` and `seek` on the reader of a connection are compile errors, where `Stdin` has no `Seek` in Rust |
| `tokio::io::AsyncSeekExt::seek` on a `BufWriter` | `try await writer.seekAsync(from)` | built | A reader seeks as it is in an async function, since it neither reads nor writes |
| `writeln!(w, "{}", x)?` | `try w.writeLine(x)` | built | |
| a `BufWriter` dropped (its error lost) | `try w.close()` | built | A writer not closed on every path is a compile error |
| `io::stdin()`, `io::stdout()` | `Stdin`, `Stdout`, `Stderr` | built | Into a pipe whose reader is gone, `Stdout` and `Stderr` as writers end the program as `print` and `printError` do (2055): by `SIGPIPE` on Linux and macOS; on Windows `Stdout` with 0 and `Stderr` with the error, where Rust gives `BrokenPipe` once the signal is ignored, as its runtime starts |
| `fs::remove_file(p)` | `File.delete(read p)` | built | A missing file is `NotFound`, as in Rust; a folder is `IsDirectory` on every system |
| `fs::rename(a, b)` | `File.replace(read a, read b)` | built | Takes the place of `b` in one step, even while `b` is open, on Windows as Rust does too |
| `fs::rename(a, b)` after a test that `b` is missing | `File.move(read a, read b)` | built | `AlreadyExists` when `b` is there, told in the same step as the rename (`RENAME_NOREPLACE`, `RENAME_EXCL`); Rust's `rename` has no such form |
| `fs::copy(a, b)` | `File.copy(read a, read b)` | built | Never takes the place of `b`: `AlreadyExists`, where Rust's `copy` writes over it. The permissions come along without setuid, setgid and sticky |
| `fs::metadata(p)?.len()`, `.is_file()`, `.modified()` | `var info = try File.info(read p)`, `info.size`, `info.kind`, `info.modified` | built | `modified` is a `DateTime`; a link is followed, as `metadata` does |
| `std::io::stdout().is_terminal()`, `file.is_terminal()` | `Stdout.isTerminal()`, `writer.isTerminal()` | built | A method of every `StreamReader` and `StreamWriter`, as `IsTerminal` is of Rust's streams and files; a connection or a pipe of another program is never one (1901) |
| `file.metadata()?` | `try reader.info()` | built | Of the open file (`fstat`); it waits for nothing, so an async function calls it as it is, where tokio's `file.metadata().await` goes to a thread (1873) |
| `let f = File::open(p)?; if !f.metadata()?.is_file() { ... }` | `File.openRegular(read p)`, `openRegularAsync` | built | Only a regular file is opened: a folder is `IsDirectory`, a pipe, a device or a socket `NotAFile`, refused before it is opened, where Rust's `File::open` of a FIFO waits for a writer (1873) |
| `Path::new(p).is_file()`, `.is_dir()` | `File.exists(read p)`, `Directory.exists(read p)` | built | `false` when the path cannot be looked at, as in Rust |
| `fs::create_dir(p)`, `fs::create_dir_all(p)` | `Directory.create(read p)`, `Directory.createAll(read p)` | built | |
| `fs::remove_dir(p)`, `fs::remove_dir_all(p)` | `Directory.delete(read p)`, `Directory.deleteAll(read p)` | built | Neither follows a link inside; `deleteAll` of a path that is itself a link is `NotDirectory`, where Rust removes the link |
| `fs::read_dir(p)`, `entry.file_name()`, `entry.file_type()` | `try Directory.list(read p)`, `entry.name`, `entry.kind` | built | A `List<DirectoryEntry>` in the order of the bytes of the names, where Rust gives the order of the system; a name that is not UTF-8 comes as a `string` all the same, with its bytes escaped, where Rust gives an `OsString` |
| `std::os::unix::fs::symlink(target, p)` (`os::windows::fs::symlink_file`, `symlink_dir`) | `try File.createLink(read target, read p)` | built | One call on every system: on Windows a link of a folder when the target is one, as Go decides; without the mode of a developer it is `PermissionDenied` (1973) |
| `fs::hard_link(a, b)` | `try File.createHardLink(read a, read b)` | built | |
| `fs::read_link(p)?` | `try File.readLink(read p)` | built | A `string`, with what is not UTF-8 escaped (189); a path that is no link is `InvalidInput`. On Windows `\??\C:\x` is given as `C:\x`, as Go gives it, where Rust gives `\\?\C:\x`, and a junction is no link (1975) |
| `fs::symlink_metadata(p)?` | `try File.linkInfo(read p)` | built | `kind` is `Link` for a symbolic link; on Windows a junction is `Other` (1976) |
| `fs::metadata(p)?.permissions().mode()` | `(try File.info(read p)).permissions` | built | An `int`, `st_mode & 0o7777`; on Windows `0o666` or `0o444` (a folder `0o777`, `0o555`), where Rust has only `readonly()` (1978) |
| `fs::set_permissions(p, Permissions::from_mode(0o644))` | `try File.setPermissions(read p, 0o644)` | built | Through a link; a mode past `0o7777` is `InvalidInput`; on Windows only the owner's bit of writing, kept as the attribute that a file may only be read (1978) |
| `file.lock()`, `lock_shared()`, `try_lock()`, `try_lock_shared()`, `unlock()` (Rust 1.89) | `try file.lock()`, `lockShared()`, `tryLock()`, `tryLockShared()`, `unlock()` on a `StreamReader` or a `StreamWriter` | built | `tryLock` gives `Result<bool, IoError>`, where Rust gives `Err(TryLockError::WouldBlock)`; a second lock changes the kind on Windows too; a writer writes its block before it lets go (1985); `await file.lockAsync(read token)` waits without a thread and a cancel ends it (1983); `lock` is a word that is a name after a dot (1986) |
| `unsafe { memmap2::Mmap::map(&file)? }` | `try unsafe File.mapRead(read p)`, `map.bytes()` | built | A crate in Rust, in the language here; `unsafe` for the same reason, the file may change under it. A `MappedFile` takes its file out of the memory where it ends; a mapping to write waits (1980) |
| `env::current_dir()`, `env::temp_dir()` | `Directory.current()`, `Directory.temp()` | built | There is no `set_current_dir`: every thread would see it change |
| `tempfile::tempdir()` | `Directory.createTemp()` | built | In the language, not a crate: `mkdtemp`, a folder only the user can go into; removing it is the program's work |
| `tempfile::NamedTempFile::new()` | `File.createTemp()` | built | `mkstemp`, a file only the user can open; it gives the path, and removing the file is the program's work: nothing deletes it when it ends |
| `tempfile::tempfile()` | `var out = try File.openTemp()`, `try out.openReader()` to read it back | built | A file with no name, gone when its last stream is closed, also when the program is ended from outside: `O_TMPFILE`, or `mkstemp` and its name taken away at once; on Windows `FILE_FLAG_DELETE_ON_CLOSE`, where a name is seen while it is open (2076) |
| `cap_std::fs::Dir::open_ambient_dir(p, ambient_authority())?`, and `dir.open(name)?` | `var root = try Directory.openRoot(read p)`, `try root.openText(name)` | built | In the language, not a crate. A link is followed while it stays inside the folder; a path that leads out, by `..`, from the root of the system or through a link, is `IoError.OutsideRoot`. `openRegular`, `createText`, `openWrite`, `openAppend`, `info`, `linkInfo`, `exists`, `list`, `createDirectory`, `delete`, `deleteDirectory`, `openRoot`, each with an `Async` form; `openat2` with `RESOLVE_BENEATH` on Linux and a walk part by part elsewhere, as `cap-std` does (2066-2071) |
| `root.join(name)` after checking `name` by hand, or `path.canonicalize()?.starts_with(&root)` | `try Path.joinInside(read root, name)` for the text alone; a `DirectoryRoot` when the file is opened | built | The check of a text cannot see a link in the folder, and a path looked at and then opened can change in between: a root finds the path from the handle of the folder |
| `path.join(name)`, `path.file_name()`, `path.extension()`, `path.parent()` | `Path.join(read path, name)`, `Path.fileName(read path)`, `Path.extension(read path)`, `Path.parent(read path)` | built | A path is a `string`. A second path that starts with a root is put after the first, the root left out, where Rust's `join` puts it in its place; the extension keeps its `.`, as in C#. On Windows `/` and `\` both separate, `join` writes `\`, and `C:\`, `C:` and `\\server\share\` are roots, as Rust reads them |
| `cfg!(windows)`, `cfg!(target_os = "linux")`, `cfg!(target_os = "macos")` | `OperatingSystem.isWindows()`, `isLinux()`, `isMacOS()` | built | Known when the program is built, as `cfg!` is: both branches are checked on every system and only one is compiled. There is no `#[cfg]` that leaves code unchecked |
| `path_buf.push(name)` | `Path.push(ref path, read name)` | built | The name added to the end of the text where it lies, by the rules of `Path.join` (a root at its start left out, where Rust's `push` puts it in the place of the path); a path built of many parts is not copied for each (1802) |
| a check of each component of a name from outside before `path.join(name)` (Go's `filepath.IsLocal`) | `try Path.joinInside(read folder, name)` | built | An `InvalidInput`, the same on every system, for `..`, a root, a `\`, a `:`, a part that ends with `.` or a space, a control character (C0, DEL, C1) and the devices of Windows; a link inside the folder is still followed |
| `env::var("HOME")`, `env::var_os("HOME")` | `Environment.get("HOME")` | built | A `Result<string, VariableError>` with `Missing` alone: a value that is not UTF-8 comes with its bytes escaped, where Rust's `var` gives `NotUnicode` and `var_os` an `OsString` |
| `env::set_var`, `env::remove_var` | none | not in SG | Unsafe in Rust 2024 for the same reason: another thread may be reading. A program that starts another gives it an environment of its own |
| `notify::recommended_watcher(tx)?` and `watcher.watch(p, RecursiveMode::NonRecursive)?` | `var watcher = try Directory.watch(read p)` | built | From when it returns; nothing of what is there. A file is `NotDirectory`; inotify, FSEvents and `ReadDirectoryChangesExW` under it, as `notify` has them (2056-2065) |
| `watcher.watch(p, RecursiveMode::Recursive)?` | `var watcher = try Directory.watchAll(read p)` | built | Folders made or moved in later are watched, and what is in them is told as made; a link is not followed |
| `rx.recv()?` of an `Event` | `try watcher.next()`, `await watcher.nextAsync(read token)` | built | A `FileChange`, one path each: `Created`, `Deleted`, `Changed` (a file written or its attributes; a folder never) or `Lost` (`notify`'s `Rescan`); an `EventKind::Modify(ModifyKind::Name(..))` is a `Deleted` of the old name and a `Created` of the new one on every system, where `notify` pairs them only on some. The same change twice in a row before it is taken is one, at most 16,384 wait; the folder itself gone gives `NotFound` |
| `notify_debouncer_full::new_debouncer(timeout, ..)` | a loop of `nextAsync` with a `CancellationSource.after(ms)` | not in SG | No waiting for a quiet time in `std` (2061) |

## Other programs

| Rust | SG | Status | Note |
|---|---|---|---|
| `Command::new("git")` | `var git = Command("git")` | built | No shell, as in Rust. A name without `/` is looked for only in the folders of `PATH` that start with `/`, as Go does since 1.19; Rust also runs a program found through `.` or a relative folder. On Windows only the absolute folders of `PATH`, with `.exe` added, where Rust also looks in the folder of the program and in the folders of the system; a batch file is escaped as Rust escapes it since 1.77.2 |
| `.arg("log")`, `.args(["-1", "--oneline"])` | `git.arg("log")`, `git.args(read words)` | built | One call per line: the methods change the command (`ref this`) and give nothing, so they are not chained |
| `.current_dir(dir)` | `git.directory(read dir)` | built | A relative path of the program is taken from our folder, not from the program's |
| `.env(k, v)`, `.env_remove(k)`, `.env_clear()` | `git.setVariable(k, v)`, `git.removeVariable(k)`, `git.clearVariables()` | built | A table of the program's own; ours is never changed. A name with `=` or a zero byte is `IoError.InvalidInput` when it runs, as Rust's `InvalidInput` |
| `.status()?` and `status.success()` | `try git.run()` | built | `run` takes an end with anything but 0 for an error (`ProcessError.Failed`), as Go's `Run`; Rust leaves the check to the program |
| `.status()?` | `try git.status()` | built | Every end is a value: `ExitStatus.Code(code)` or `ExitStatus.Signal(number)`, where Rust has `code()` and `signal()`. The code is an `int`, as Rust's `code()` gives an `i32`: Windows gives 32 bits, and one with its highest bit set is printed in hex |
| `.output()?`, `String::from_utf8(out.stdout)?` | `try git.output()` | built | The output as a `string` (`InvalidText` if it is not UTF-8); the standard error goes to ours, not into the value, and an end with anything but 0 is an error |
| `.stdin(Stdio::piped())`, writing in a thread | `git.input(read text)` | built | Written while the output is read, so neither waits for the other; a program that stops reading is no error |
| `.spawn()?` | `var process = try git.start()` | built | Gives a `Process`, which runs while this program goes on |
| `.stdin(Stdio::piped())`, `.stdout(...)`, `.stderr(...)` | `git.pipeInput()`, `git.pipeOutput()`, `git.pipeError()` | built | Only `start` looks at them; a stream not asked for is ours, as Rust's default for `spawn` |
| `child.stdin.take().unwrap()` | `process.takeInput() ?? panic("...")` | built | `takeInput` gives a `StreamWriter?`, `takeOutput` and `takeError` a `StreamReader?`: `null` once taken, or when not asked for. The streams are owned and go to other tasks |
| `drop(stdin)` to send the end of the input | `try input.close()` | built | The writer is `Closable`; writing after the program stopped reading gives `IoError.ConnectionReset`, never the signal |
| `child.wait()?` | `try process.wait()` | built | Takes the `Process`, closing the input it still holds first, as Rust's `wait` does, and letting go of its readers |
| a `Child` dropped (the program goes on, a zombie once it ends) | a compile error; on a way out with an error, the program is stopped | built | A `Process` in a local variable is waited for on every path; one that ends without that (an error, a field) is stopped with the signal 9 and waited for, as tokio's `kill_on_drop(true)` |
| `child.kill()?` | `process.kill()` | built | Gives nothing: a program not waited for keeps its number, so the signal cannot reach another one. On Windows `TerminateProcess` with the code 1, as Rust's |
| `env_clear()` | `command.clearVariables()` | built | On Windows the program keeps `SystemRoot`, as Go keeps it and Rust does not: without it Winsock does not start there |
| `libc::kill(pid, SIGTERM)` | `process.terminate()` | built | The polite stop, which the program may catch |
| `child.id()` | `process.id()` | built | |
| `child.try_wait()?` | `try process.tryWait()` | built | `null` while it runs; once seen ended, `wait` gives the same end and the `Process` stops nothing. A wait with a time limit is `waitAsync` with a token of `CancellationSource.after(ms)` |
| `.stdout(File::create(p)?)`, `.stdin(File::open(p)?)` | `git.outputTo(p)`, `git.appendOutputTo(p)`, `git.inputFrom(p)` | built | A path, opened as the program starts (`> p`, `>> p`, `< p`); one that cannot be is the error of `run` or `start`. An open `File` given instead is not here |
| `.stderr(Stdio::null())`, `.stdout(Stdio::null())` | `git.discardError()`, `git.discardOutput()` | built | |
| `2>&1` (`.stderr(out.try_clone()?)` of the same pipe or file) | `git.errorToOutput()` | built | Into the text of `output` too |
| `a.stdout(Stdio::piped())` and `b.stdin(a_child.stdout.take().unwrap())` (`a \| b`) | `var p = Pipeline()`, `p.add(a)`, `p.add(b)`, `try p.output()` | built | Joined by pipes of the system; fails as `set -o pipefail`, but not for one stopped by `SIGPIPE` before the last; `status()` gives every end |
| a `Child` forgotten on purpose (`std::mem::forget`, or a daemon) | `process.detach()` | built | The runtime takes it away once it ends (a `pidfd` or a kqueue watched by a thread), so no zombie stays |
| `std::process::id()`, `std::os::unix::process::parent_id()` | `Process.currentId()`, `Process.parentId()` | built | |
| `nix::sys::signal::kill(Pid::from_raw(n), SIGTERM)` | `Process.signalId(n, Signal.Terminate)`, `Process.killId(n)` | built | To a program this one need not have started; 0 or less is `InvalidInput`, none of that number `NotFound` |
| `tokio::process::Command::status().await` | `await git.statusAsync()`, `runAsync()`, `outputAsync()` | built | A token may come last; a cancel stops the program, as Go's `CommandContext` |
| `child.wait().await` | `await process.waitAsync()` | built | Waits on the loop, as tokio does: the pidfd on Linux, `EVFILT_PROC` on macOS; a cancel stops the program |
| `nix::sys::signal::kill(pid, Signal::SIGINT)` | `process.signal(Signal.Interrupt)` | built | Only to a program started with `Command`, by the signal's name; it gives nothing, as `kill()` |
| `command.process_group(0)` | `command.newGroup()` | built | Its `Process` stops the whole group when it is not waited for; Windows `CREATE_NEW_PROCESS_GROUP` |
| `pre_exec(\|\| { setsid(); Ok(()) })` | `command.newSession()` | built | No `unsafe` and no closure; Windows `DETACHED_PROCESS` |
| `nix::sys::signal::killpg(Pid::from_raw(pgid), SIGTERM)` | `process.signalGroup(Signal.Terminate)`, `process.killGroup()` | built | Only to the group of a program started in one of its own, else a panic; Windows sends only `Interrupt` (Ctrl-Break) to a group |
| `nix::unistd::getpgid(Some(pid))` | `process.groupId()` | built | `int?`: `null` in the group of this program |
| `nix::unistd::tcsetpgrp(tty, pgid)` with `SIGTTOU` ignored around it | `Terminal.giveTo(read process)`, `Terminal.takeBack()` | built | `SIGTTOU` held off by the call itself; no terminal is `IoError.NotATerminal`; nothing on Windows |
| `pre_exec` with `setpgid` and `tcsetpgrp` (a job in front) | `command.foreground()`, `pipeline.foreground()` | built | `fork` and `exec` for that one way, so no race; `run`, `output` and `status` put this program in front again |
| `waitpid(pid, WUNTRACED \| WCONTINUED)` (`nix::sys::wait`) | `process.waitChange()`, `tryWaitChange()`, `waitChangeAsync()` | built | A `ProcessChange`: `Stopped(number)`, `Continued`, `Ended(status)`; the async one listens for `Signal.Child` |

## Signals

| Rust | SG | Status | Note |
|---|---|---|---|
| `tokio::signal::unix::signal(SignalKind::interrupt())?` | `var signals = Signals.listen(read List<Signal> { Signal.Interrupt })` | built | One listener may take several kinds; while it lives they do not end the program. Once the last listener of a kind ends, the kind does what it did before, where Tokio keeps its handler for good |
| `sig.recv().await` | `await signals.nextAsync(read token)` | built | Gives `Result<Signal, Cancelled>`; a cancel ends the wait, and a signal that comes later stays. Several of one kind before a call are one, as in Tokio |
| `signal_hook::iterator::Signals::new([SIGINT, SIGTERM])?`, `.forever()` | `signals.next()` in a loop | built | Waits on the thread, which an async function does not do |
| `tokio::signal::ctrl_c().await` | `var ctrlC = Signals.listen(read List<Signal> { Signal.Interrupt })`, then `await ctrlC.nextAsync()` | built | Listening starts at `listen`, so a Ctrl-C that comes before the `await` is not lost. On Windows Ctrl-C and Ctrl-Break both, as Tokio's `ctrl_c` |
| `tokio::signal::windows::ctrl_close()`, `ctrl_logoff()`, `ctrl_shutdown()` | `Signals.listen(read List<Signal> { Signal.Terminate })` | built | One kind for the three, as Go has it; the program is held while it ends, within the time Windows gives |
| `ctrlc::set_handler(move \|\| ...)` | a task that awaits `nextAsync`, then cancels a source | built | No handler of the program's own; `examples/shutdown` stops its server that way |
| `libc::raise(libc::SIGUSR1)` | `Signals.raise(Signal.User1)` | built | What was printed is written out first |
| `libc::sigaction` with a handler of one's own | an `export` function given to `sigaction` inside `unsafe` | built | Nothing is checked inside it; when it and a listener take one signal, the last one set wins |
| `SignalKind::from_raw(SIGTSTP)`, `SIGCONT`, `SIGTTIN`, `SIGTTOU`, `SIGCHLD`, `SIGALRM` | `Signal.TerminalStop`, `Continue`, `TerminalInput`, `TerminalOutput`, `Child`, `Alarm` | built | Listened for, one does nothing of its own: Ctrl-Z does not pause a listener of `TerminalStop`; no `alarm()` |
| `libc::raise(libc::SIGSTOP)` | `Signals.stopSelf()` | built | What a shell does to itself; it returns once another program sends `Signal.Continue`; nothing on Windows |
| `portable_pty::native_pty_system().openpty(PtySize { rows, cols, .. })` and `pair.slave.spawn_command(cmd)` | `command.terminal(rows, columns)`, `command.start()` | built | A session of its own whose controlling terminal it is; ConPTY on Windows. Only `start`: `run`, `output` and `status` are a panic |
| `pair.master.try_clone_reader()`, `pair.master.take_writer()` | `process.takeTerminalReader()`, `takeTerminalWriter()` | built | `null` once taken; the reader ends when the program and those it started that hold the terminal have ended (Linux's `EIO` is the end) |
| `pair.master.resize(PtySize { rows, cols, .. })` | `process.resizeTerminal(rows, columns)` | built | Nothing once the program has ended; outside 1 to 65535 `InvalidInput` |
| `crossterm::terminal::enable_raw_mode()` / `disable_raw_mode()` (or `termios::cfmakeraw`) | `var raw = try Terminal.rawMode()`, `try raw.restore()` | built | Of the input of this program; put back where the `RawMode` ends, at a panic, at `exit`, and at a signal that ends the program with no listener |
| `crossterm::terminal::size()` (columns first), `terminal_size::terminal_size()` | `Terminal.size()` | built | A `TerminalSize` of `rows` and `columns`, rows first; of the output of this program; `NotATerminal` when it is none |
| `SIGSEGV`, real-time signals | none | not in SG | `Signal` has the kinds a program asks for; the others are the runtime's, or later |

## Network

| Rust | SG | Status | Note |
|---|---|---|---|
| `TcpListener::bind("127.0.0.1:8080")?` | `try TcpListener.bind("127.0.0.1:8080")` | built | A number, `localhost` or `":8080"` (every v4 address); no name is looked up, so it never waits. Port 0 lets the system choose: `listener.port()`. On Windows it holds the port alone (`SO_EXCLUSIVEADDRUSE`), which Rust's does not: bound to every address, its port is not taken on `127.0.0.1` by another program's socket that asks to share it |
| `listener.accept()?` | `try listener.accept()` | built | Through a `read` borrow, as `accept(&self)`; gives the `TcpStream` alone, not the address of the other side |
| `TcpStream::connect("example.com:80")?` | `try TcpStream.connect("example.com:80")` | built | The host looked up by name, each address tried in turn |
| `stream.read(&mut buf)?` | `try stream.receive(ref buffer)` | built | What has come, how many, 0 at the end; `read` is a word of SG, and `receive` is C#'s name |
| `stream.write_all(&bytes)?` | `try stream.send(bytes)` | built | All of them; writing after the other side has gone is `ConnectionReset`, never `SIGPIPE`. Through a `read` borrow, as `Write for &TcpStream`, so it sends while a reader borrows the stream; not through a `shared` stream |
| `BufReader::new(&stream)`, `.read_line(&mut line)?`, `.lines()` | `var reader = stream.reader()`, `try reader.readLine(ref line)`, `reader.lines()` | built | A `StreamReader` that borrows the stream; `receive` while it is used again is a compile error, since it would take bytes from under its block. A line, or a text, longer than 64 KB is `IoError.LineTooLong` (`stream.reader(limit)` for another limit), as Go's `bufio.Scanner`; every read after it gives it too |
| `reader.fill_buf()?` / `BufRead` then `consume` | `reader.buffered()`, `reader.consume(n)`, `try reader.fill()` | built | On every `StreamReader`, and `await reader.fillAsync(read token)`: three calls, since an async function cannot give a borrow; `fill` always reads more, where `fill_buf` gives the block at once when it holds bytes, and a block full at its limit is `IoError.TooLong`; `consume` past what it holds is a panic, where Rust cuts it short. `readSome(ref buffer)` copies what the block holds. On `Stdin`, where Rust's `fill_buf` is on the `StdinLock` of `stdin().lock()`, the first task that calls one of the three holds the input, and another task's read of it is a panic |
| `stream.write_vectored(&[IoSlice::new(head), IoSlice::new(body)])` | `try conn.sendPieces(read pieces)` | built | `pieces` a `List<read byte[]>`; sends all of them, as `write_all` would, and `sendPiecesAsync` in an async function |
| `tokio::io::BufReader::new(stream).lines()` | `await reader.readLineAsync(ref line)` | built | A line the block holds is read in place; a cancel ends a wait, and what came stays in the block |
| `drop(stream)`, `stream.shutdown(Both)` | the stream ends | built | Closed where it ends; no `close` |
| `stream.shutdown(Shutdown::Write)` | `try conn.closeSending()` | built | Through a `read` borrow; the other side reads the end, this side still receives. A second call does nothing, and a send after it is `ConnectionReset`. No `shutdown(Read)` |
| `stream.set_nodelay(true)?` | nothing: on by default | built | Every connection a listener takes or a connect makes has `TCP_NODELAY`, as in Go (Rust leaves Nagle's wait on, which on Linux holds a question sent in two writes 40 ms); `try conn.setNoDelay(false)` asks for the wait (1966) |
| `socket2::SockRef::from(&stream).set_tcp_keepalive(&TcpKeepalive::new().with_time(t).with_interval(t))` | `try conn.setKeepAlive(Duration.fromSeconds(30))` | built | std has none; the time between asks is the idle time, rounded up to whole seconds; `null` turns it off |
| `tokio::net::TcpListener`, `TcpStream`, `.await` | `await listener.acceptAsync()`, `connectAsync`, `receiveAsync`, `sendAsync` | built | A token may come last, and a cancel ends a wait at once; what came stays in the socket. A socket found empty or full waits for the loop at once, as Tokio's does; each call of an async function waits in one room of its frame, 224 bytes ([measured](../benchmarks/library.md#net)) |
| `stream.peer_addr()?`, `stream.local_addr()?` | `try stream.remoteAddress()`, `stream.localAddress()` | built | A `SocketAddress`; the other side is `ConnectionReset` once it has broken the connection off |
| `stream.into_split()`, `Arc<TcpStream>` sent on from several threads | none yet | designed | Later slices of the network |
| `TcpListener::bind("[::]:8080")?` | `try TcpListener.bind("[::]:8080")` | built | Takes v4 too on every system (`IPV6_V6ONLY` off); Rust leaves it to the system, which on Windows takes v6 alone. A v4 peer is `127.0.0.1:port`, where Rust gives `[::ffff:127.0.0.1]:port`; a `UdpSocket` of `[::]` the same |
| `std::os::unix::net::UnixListener::bind(path)?` | `try UnixListener.bind(path)` | built | Also on Windows 10 1803 and later. Its file is taken away where it ends, if it is still the one it made, which Rust leaves; a path too long is `TooLong`, where Rust says `InvalidInput` |
| `listener.accept()?` of a `UnixListener` | `try listener.accept()`, `await listener.acceptAsync()` | built | Gives a `UnixStream` alone |
| `UnixStream::connect(path)?`, `tokio::net::UnixStream::connect(path).await?` | `try UnixStream.connect(path)`, `await UnixStream.connectAsync(path)` | built | A listener with no room left refuses on Linux too, where Rust's would wait |
| `stream.read`, `write_all`, `BufReader::new(&stream)` of a `UnixStream` | `receive`, `send`, `sendPieces`, `reader()`, `closeSending()` | built | As a `TcpStream`; no `peer_addr`, `local_addr` or TCP's options |
| `UnixDatagram`, `UnixStream::pair()`, `SocketAddr::from_abstract_name`, `peer_cred()`, `SCM_RIGHTS` | none | not here | 1964 |
| `UdpSocket::bind("127.0.0.1:0")?` | `try UdpSocket.bind("127.0.0.1:0")` | built | What `TcpListener.bind` takes; no name is looked up. On Windows it holds the port alone, and a datagram that came back is told only to a connected socket, as on Linux and macOS; Rust's socket there fails its next receive with a reset, whoever the datagram was for |
| `socket.send_to(&bytes, addr)?` | `try socket.sendTo(bytes, to)` | built | One datagram, whole: one too long to send as one message is `IoError.MessageTooLong`. `to` is a `SocketAddress`, not a text to look up |
| `let (n, from) = socket.recv_from(&mut buf)?` | `var got = try socket.receiveFrom(ref buffer)` | built | A `Received` with `got.count` and `got.from`. A datagram longer than the buffer is `MessageTooLong`, where Rust cuts it short |
| `socket.connect(addr)?`, `socket.send(&b)?`, `socket.recv(&mut buf)?` | `try socket.connect(to)`, `try socket.send(bytes)`, `try socket.receive(ref buffer)` | built | `ConnectionRefused` when the other machine says nobody listens |
| `socket.local_addr()?`, `socket.peer_addr()` | `socket.localAddress()`, `socket.remoteAddress()` | built | `remoteAddress` is a `SocketAddress?`: `null` before a `connect` |
| `Arc<UdpSocket>` shared by threads, `&self` methods | `shared UdpSocket`, every method through `read` | built | No `close`: it is closed where it ends |
| `tokio::net::UdpSocket`, `.recv_from(&mut buf).await` | `await socket.receiveFromAsync(ref buffer)`, `sendToAsync`, `receiveAsync`, `sendAsync` | built | A token may come last; a cancel ends a wait at once |
| `"[::1]:53".parse::<SocketAddr>()?` | `try SocketAddress.parse("[::1]:53")` | built | `IoError.InvalidInput` for a text that is not an address; printed as it is read (RFC 5952) |
| `("example.com", 53).to_socket_addrs()?` | `try SocketAddress.lookup("example.com:53")` | built | A `List<SocketAddress>`; `HostNotFound` when the name has none. `lookupAsync` looks it up on a thread. On Windows the name goes to the system in UTF-16 (`GetAddrInfoW`), not through the code page as Rust's |
| `addr.ip()`, `addr.port()` | `address.host()`, `address.port()` | built | `host()` is text: there is no `IpAddr` type |
| `socket.join_multicast_v4(...)`, `set_broadcast(true)`, `set_ttl`, `peek_from` | none yet | designed | Later |

## HTTP

| Rust | SG | Status | Note |
|---|---|---|---|
| `http1::Builder::new().serve_connection(io, service_fn(answer))` (hyper) | `var conn = HttpConnection(stream)`, `while try await conn.receiveHeadAsync() { ... }` | built | No callback: the program pulls each request and answers it, as Zig's `std.http.Server` (1766); without `Async` the calls wait on the thread |
| `req.method()`, `req.uri()`, `req.headers().get("x")` | `var req = conn.request()`, `req.method()`, `req.target()`, `req.header("x")` | built | A view of the request where it lies in the connection's block, `HttpRequest<from conn>`: nothing copied (1767); a header is `read byte[]?`, in any case |
| `req.uri().authority()`, else the `Host` header | `req.host()` | built | The URL's host when the target is a whole URL, `Host`'s otherwise (1791); `Host` is checked to be `host[:port]` (1790) |
| `Limited::new(body, limit).collect().await` | `try await conn.receiveBodyAsync()`, then `conn.request().body()` | built | The whole body in the block, chunks put together in place; over `maxBodyBytes` 413 and closed; `100 Continue` when the client waits for it |
| `Response::builder().status(200).header(k, v).body(b)` | `try conn.addHeader(k, v)`, `try await conn.respondAsync(200, "text/plain", body)` | built | The module writes the status line, `date`, `content-length`, `content-type`, and `connection: close` when it closes |
| A `StreamBody` of chunks | `beginAsync(status, type)`, `writeAsync(part)`, `endAsync()` | built | In chunks, or for HTTP/1.0 until the connection closes |
| A `StreamBody` with `CONTENT_LENGTH` set (a file sent from `ReaderStream`) | `beginAsync(status, type, length)`, `writeAsync(part)`, `endAsync()` | built | `content-length`, no chunks; a part past the length is a panic, an end before it closes the connection and gives `InvalidInput`, where hyper closes it with an error of its own (1865) |
| `tower_http::services::ServeDir` | none | not in SG | A file server is written on `HttpConnection`: `trials/server/files/sg/files.sg` serves ranges, `If-Modified-Since` and folders in some 420 lines |
| `header_read_timeout`, `keep_alive(false)`, `graceful_shutdown()` | `HttpOptions { headTimeout, idleTimeout, maxHeadBytes, maxHeaders, maxBodyBytes }`, `conn.closeAfterAnswer()` | built | A malformed request is answered by the module (400, 413, 414, 431, 501, 505) and `conn.refused()` says with what; where SG and hyper answer differently, and why: [library.md](../benchmarks/library.md#against-hyper) |
| `pipeline_flush(true)` | Answers held while the next request is whole in the block, `flushAsync()` | built | Up to 64 KB sent together (1777) |
| hyper's client, HTTP/2, TLS | none yet | designed | Later slices (1765) |

## Time

| Rust | SG | Status | Note |
|---|---|---|---|
| `std::time::Instant::now()`, `start.elapsed()`, `later.duration_since(earlier)` | `Instant.now()`, `start.elapsed()`, `later.since(earlier)` | built | `since` gives a negative `Duration` where Rust's saturates at zero; an `Instant` is not printed; on macOS the same clock as Rust's, `CLOCK_UPTIME_RAW`, which stops while the machine sleeps |
| `std::time::Duration::from_millis(250)`, `d.as_millis()`, `d.as_secs_f64()` | `Duration.fromMilliseconds(250)`, `d.milliseconds()`, `d.totalSeconds()` | built | A `long` of nanoseconds that may be negative; `{:?}` and `print(d)` both write `1.5s` |
| `a + b`, `a - b`, `d * 3`, `a.checked_add(b)` | `a.plus(b)`, `a.minus(b)`, `d.times(3)` | built | Methods; an overflow follows the rule of the numbers, so there is no `checked_` form yet |
| `SystemTime::now()`, `.duration_since(UNIX_EPOCH)` | `DateTime.now()`, `t.unixSeconds()`, `t.unixMilliseconds()` | built | UTC, nanoseconds in a `long`: the years 1677 to 2262 |
| `chrono::Utc.with_ymd_and_hms(...)`, `t.year()`, `t.weekday()`, `t.to_rfc3339()` | `DateTime.utc(...)` (a `Result<DateTime, DateError>`), `t.year()`, `t.dayOfWeek()`, `print(t)` | built | In the language, not a crate; `2026-11-15T09:30:05Z` |
| `std::thread::sleep(d)` | `Task.delay(ms).wait()`; `await Task.delay(ms)` in async code | built | Milliseconds as an `int` |
| `chrono::Local::now()`, `t.with_timezone(&Local)`, `t.offset()` | `DateTime.now().toLocal()`, `t.toLocal()`, `seen.offset()` | built | A `DateTimeOffset`: the moment and its offset; the parts are those of the place |
| `t.with_timezone(&FixedOffset::east_opt(s)?)`, `.with_timezone(&Utc)` | `t.withOffset(Duration.fromSeconds(s))` (a `Result`), `seen.toUtc()` | built | `InvalidOffset` for a day or more, or a part of a second |
| `Local.with_ymd_and_hms(...)` (a `LocalResult`) | `DateTimeOffset.local(...)` (a `Result<DateTimeOffset, DateError>`) | built | `SkippedLocalTime` where chrono gives `None`; the earlier moment where it gives `Ambiguous` |
| `DateTime::parse_from_rfc3339(s)`, `t.to_rfc3339()` | `DateTimeOffset.parse(s)`, `DateTime.parse(s)` (to UTC), `print(t)` | built | `InvalidText(position)` says where the text stops being one |
| `httpdate::fmt_http_date(t)`, `httpdate::parse_http_date(s)` | `t.toHttpDate()`, `DateTime.parseHttpDate(s)` | built | The three forms of RFC 9110; the day of the week is held to the date, and the years are `DateTime`'s, 1677 to 2262, where `httpdate` takes 1970 to 9999 (1866) |
| `t.format("%d.%m.%Y")`, `NaiveDate::parse_from_str` | `$"{t.day():D2}.{t.month():D2}.{t.year()}"`; none | not in SG | A form is written with the parts and checked when compiled; a text in another form is read with `split` and `int.parse` |
| `chrono-tz` (`Europe::Istanbul`) | none yet | designed | Zones by name come later |

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

## Hex and Base64

| Rust | SG | Status | Note |
|---|---|---|---|
| `hex::encode(bytes)`, `hex::decode(text)` | `Hex.encode(read bytes)`, `Hex.decode(text)` | built | In the language's own module, not a crate; decoding gives `Result<List<byte>, DecodeError>` |
| `write!(s, "{:02x}", b)` in a loop | `Hex.encodeInto(ref s, read bytes)` | built | Appends to the text, no new one made |
| `STANDARD.encode_string(bytes, &mut s)`, `URL_SAFE_NO_PAD.encode_string(...)` | `Base64.encodeInto(ref s, read bytes)`, `Base64.encodeUrlInto(ref s, read bytes)` | built | Appends to the text |
| `STANDARD.decode_vec(text, &mut v)`, `hex::decode_to_slice(text, &mut buf)` | `Base64.decodeInto(ref v, text)`, `Base64.decodeUrlInto(ref v, text)`, `Hex.decodeInto(ref v, text)` | built | Adds to a `List<byte>`; an error leaves it as it was |
| `base64::engine::general_purpose::STANDARD.encode(bytes)`, `.decode(text)` | `Base64.encode(read bytes)`, `Base64.decode(text)` | built | Strict as the crate is: no white space, padding where it belongs, no bits left over |
| `URL_SAFE_NO_PAD.encode(bytes)`, `.decode(text)` | `Base64.encodeUrl(read bytes)`, `Base64.decodeUrl(text)` | built | `-` and `_`, no padding |
| `base64::DecodeError::InvalidByte(i, b)`, `InvalidLength` | `DecodeError.InvalidCharacter(position)`, `InvalidLength` | built | The place of the byte, not the byte |
| `percent_encoding::utf8_percent_encode(s, NON_ALPHANUMERIC)` | `Percent.encode(s.asBytes())` | built | Everything but RFC 3986's unreserved, upper-case digits |
| `percent_encoding::percent_decode_str(s).collect::<Vec<u8>>()` | `Percent.decode(s)` | built | Strict where the crate is lenient: a `%` without two hex digits is an error with its place, not kept as it is |

## Checksums and hashes

| Rust | SG | Status | Note |
|---|---|---|---|
| `crc32fast::hash(bytes)` | `Crc32.of(read bytes)` | built | In the language's own module; a `uint` |
| `let mut h = crc32fast::Hasher::new(); h.update(bytes); h.finalize()` | `var c = Crc32(); c.update(read bytes); c.value()` | built | `value()` does not end it: more may be taken in after it |
| `crc32fast` with the processor's CRC instructions | `Crc32` | built | Compiled, folded with ARMv8's PMULL and `crc32x` or x86-64's PCLMULQDQ where the processor has them, through tables elsewhere; the same bytes (1656) |
| `adler::adler32_slice(bytes)` | `Adler32.of(read bytes)` | built | zlib's checksum; `var a = Adler32(); a.update(read bytes); a.value()` piece by piece |
| `sha2::Sha256::digest(bytes)` | `Sha256.of(read bytes)` | built | A `byte[32]`; `Hex.encode(read hash)` writes it as text |
| `let mut h = Sha256::new(); h.update(bytes); h.finalize()` | `var h = Sha256(); h.update(read bytes); h.finish()` | built | `finish()` does not end it: more may be taken in after it |
| `Hmac::<Sha256>::new_from_slice(key)?.chain_update(data).finalize()` | `Hmac.sha256(read key, read data)` | built | One call; a key of any length |
| `mac.verify_slice(tag)` / `subtle::ConstantTimeEq` | `Bytes.equalInConstantTime(read a, read b)` | built | Never `==` for a tag |
| `sha2` with the processor's SHA instructions | `Sha256`, `Hmac` | built | Compiled, through ARMv8's SHA2 or x86-64's SHA-NI where the processor has them, plain SG elsewhere (1657) |

## The command line

| Rust | SG | Status | Note |
|---|---|---|---|
| `Command::new("gifcrop").about("Crops GIF images.")` (clap) | `var cl = CommandLine("gifcrop", "Crops GIF images.")` | built | In the language's own module |
| `.arg(Arg::new("verbose").short('v').long("verbose").action(ArgAction::SetTrue))` | `cl.flag("verbose", 'v', "Print each step")` | built | `null` for no short letter |
| `.arg(Arg::new("output").short('o').long("output").value_name("FILE"))` | `cl.option("output", 'o', "FILE", "Where to write")` | built | Given twice: `option` the last, `options` all |
| `.arg(Arg::new("input").required(true))` | `cl.positional("input", "The GIF to read")` | built | Always required, in the order declared |
| `.arg(Arg::new("files").num_args(0..))` | `cl.rest("files", "More GIFs")` | built | Once, last |
| `.version("1.2.0")` | `cl.version("1.2.0")` | built | `--version` only, no `-V` |
| `.get_matches()` | `cl.parseOrExit(read args)` | built | Help and version with 0, an error with 2 |
| `.try_get_matches_from(words)` | `cl.parse(read words)` | built | A `Result<Arguments, CommandLineError>` |
| `m.get_flag("verbose")` | `a.flag("verbose")` | built | A name not declared is a panic, as clap's is in a debug build |
| `m.get_one::<String>("output")` | `a.option("output")` | built | A `read string?`; no typed values: `int.parse` |
| `m.get_many::<String>("files")` | `a.rest()`, `a.options(name)` | built | |
| `#[derive(Parser)]` | none | | No reflection or macros: the command line is declared with calls |

## JSON

| Rust | SG | Status | Note |
|---|---|---|---|
| `serde_json::from_str::<Value>(text)` | `Json.parse(read text)` | built | A `Result<JsonValue, JsonError>`; strict, a name twice is an error; 128 levels, as serde_json; with `JsonOptions { maxDepth = n }` as deep as memory holds, a recursion that goes on with a list where the stack runs low, where serde_json's `unbounded_depth` needs `serde_stacker` |
| `Deserializer::from_str(text)` with `disable_recursion_limit()` | `Json.parse(read text, JsonOptions { maxDepth = 10000 })` | built | A depth of the program's choice, never none; read and written with a loop, so a depth of a million takes no stack (1829) |
| `Value::Null`, `Bool`, `Number`, `String`, `Array`, `Object` | `JsonValue.Null`, `Bool`, `Integer`, `Number`, `String`, `Array`, `Object` | built | A whole number that fits a `long` is an `Integer`, every other a `Number` |
| `serde_json::Map` (with `preserve_order`) | `JsonObject` | built | Members in order; `add` panics on a name that is there, `set` replaces |
| `v.get("name")`, `v.get(0)` | `v.get("name")`, `v.at(0)` | built | `null` when not there, never a panic |
| `v["user"]["name"]` | `v.get("user")?.get("name")` | built | No indexing that gives `Null` for what is missing |
| `v.as_str()`, `as_i64()`, `as_f64()`, `as_bool()`, `as_array()`, `as_object()`, `is_null()` | `asString()`, `asLong()`, `asDouble()`, `asBool()`, `asArray()`, `asObject()`, `isNull()` | built | |
| `map.insert(k, v)` | `o.set(k, v)`; `o.add(k, v)` for a new name | built | |
| `map.shift_remove(k)` | `o.remove(k)` | built | The others keep their order |
| `for (k, v) in map` | `var names = o.names()`, `var values = o.values()`, `for i in 0..names.length()` | built | Two lists in one order; SG has no tuples |
| `serde_json::to_string(&v)` | `v.toJson()`, `$"{v}"` | built | |
| `serde_json::to_string_pretty(&v)` | `v.toJson(JsonOptions { indent = 2 })` | built | serde_json writes any depth; `toJson` writes 128 levels, as many as `Json.parse` reads, and `JsonOptions { maxDepth = n }` more |
| `json!({"a": 1})` | `var o = JsonObject()`, `o.add("a", JsonValue.Integer(1))` | built | No macros |
| `#[derive(Serialize, Deserialize)]` | `toJson` and `fromJson` written by hand | | No reflection or macros |
| `serde_json::Deserializer::from_str(...).into_iter::<Value>()`, a stream of values | `JsonReader.create(options)`, `reader.next(read text, true)` | built | The tokens of values one after another, as text grows: `UnexpectedEnd` with `more` leaves the reader as it was, and `rebase(n)` lets the program drop what it read (1814) |
| `serde_json`'s `Deserializer` tokens with `RawValue` numbers, `arbitrary_precision` | `JsonToken.Number(text)` | built | A number as it is written; the program decides what it is. A name given twice is the program's to decide too ; `print(token)` writes `Name("id")`, as `{:?}` would (1826) |

## Log

| Rust | SG | Status | Note |
|---|---|---|---|
| `log::info!("listening on {port}")` (with `env_logger`) | `Log.info($"listening on {port}")` | built | To the standard error, `2026-11-19T20:15:03.123Z INFO  listening on 8080`; nothing to set up |
| `trace!`, `debug!`, `warn!`, `error!` | `Log.trace`, `Log.debug`, `Log.warn`, `Log.error` | built | A level that is off builds no text and runs no call inside it, as the macros do |
| `log::set_max_level(LevelFilter::Debug)` | `Log.setLevel(LogLevel.Debug)` | built | `LogLevel.Off` writes nothing |
| `log::max_level()`, `log_enabled!(Level::Debug)` | `Log.level()`, `Log.isOn(LogLevel.Debug)` | built | |
| `RUST_LOG=debug` | `SG_LOG=debug` | built | One level for the program, in any case; a value that is no level is told and `Info` is used |
| `RUST_LOG=my_crate=debug`, targets | none | | One level for the whole program |
| `env_logger`'s colours, `RUST_LOG_STYLE` | colours on a terminal, `NO_COLOR` | built | Only the level is coloured; no way to force colours on |
| `log::info!(target: ..., key = value; ...)`, `tracing` spans | none yet | | No structured fields, files or rotation |

## Regular expressions

| Rust | SG | Status | Note |
|---|---|---|---|
| `Regex::new(r"\d+").unwrap()` | `Regex.of("""\d+""")` | built | Only a pattern written in the program, checked when it is compiled; never fails when it runs |
| `static RE: LazyLock<Regex> = LazyLock::new(\|\| Regex::new(...).unwrap())` | `static Regex Re = Regex.of("...")` | built | Compiled the first time it is read; any task searches it. The states of its DFAs are kept in it behind a lock, where Rust's `regex` keeps a cache for each thread in a pool |
| `Regex::new(&pattern)` | `Regex.compile(read pattern)` | built | A `Result<Regex, RegexError>`; the error has the column |
| `re.is_match(text)`, `re.find(text)` | `re.isMatch(text)`, `re.find(text)` | built | A `Match?`: `start`, `end`, `m.of(text)`. Searched by lazy DFAs, as Rust's, after an inner text every match holds; Pike's machine only for the groups |
| `re.find_iter(text).collect()` | `re.findAll(text)` | built | A `List<Match>`; no iterator |
| `re.captures(text)`, `caps.get(1)`, `caps.name("y")` | `re.captures(text)`, `caps.get(1)`, `caps.named("y")` | built | Found as Rust finds them: where the match starts and ends first, then Pike's machine of every group only there, copying a thread's slots once, as Rust's PikeVM (1598) |
| `re.replace_all(text, "$2-$1")` | `re.replaceAll(text, "$2-$1")` | built | `$1x` is group 1 and `x`; a group the pattern lacks is a panic |
| `re.split(text)` | `re.split(text)` | built | As Go's `Split`: a `List<string>` |
| `\d`, `\w`, `\s` (Unicode) | `\d`, `\w`, `\s` (ASCII) | built | `\p{Nd}`, `\pL` for Unicode |
| `(?x)` | `(?x)` | built | Spaces and `#` comments outside a class passed over; in a class they stay, as in Perl and PCRE, where Rust passes over them there too (1822) |
| `\p{Greek}`, `\p{Letter}` | none yet | | `\p` names the general categories by their short names |
| `regex::bytes::Regex`, `re.is_match(&bytes)`, `re.find_iter(&bytes)` | `re.isMatchBytes(read bytes)`, `re.findAllBytes(read bytes)`, `findBytes`, `capturesBytes` | built | The same `Regex` searches text and bytes; a byte that is not UTF-8 matches nothing, and no empty match lies inside a character (Rust's `regex::bytes` gives one there) |
| `replace_all(text, \|caps\| ...)` | none | | No closures |
| `fancy-regex` (back references, look-around) | none | not planned | A match grows with the text and no more |

## Compression

| Rust | SG | Status | Note |
|---|---|---|---|
| `flate2::write::GzEncoder::new(Vec::new(), Compression::new(6))`, `write_all`, `finish` | `Gzip.compress(read bytes, 6)` | built | zlib's bytes at every level; a level outside 0 to 9 written in the program is a compile error |
| `flate2::read::GzDecoder::new(&bytes[..]).read_to_end(&mut out)` | `Gzip.decompress(read bytes, limit)` | built | Always a limit: `TooLarge` past it; flate2 leaves the bound to `take` |
| `flate2::{ZlibEncoder, DeflateEncoder}`, `miniz_oxide::deflate::compress_to_vec` | `Zlib.compress`, `Deflate.compress` | built | The same calls on each format |
| `GzEncoder::new(file, level)` | `Gzip.writer(writer, level)` | built | A `Compressor` over a `StreamWriter`: `writeBytes`, `close` (`Closable`) |
| `BufReader::new(GzDecoder::new(file)).lines()` | `Gzip.reader(reader, limit)`, `readLine` | built | A `Decompressor` over a `StreamReader`: `readSome`, `readBytes`, `readLine`, `readText` |
| `MultiGzDecoder` | `Gzip.decompress`, `Gzip.reader` | built | Members one after the other always open as one; bytes after the end are an error |
| `flate2::Error` / `io::Error` | `CompressError` | built | `Corrupt(position)`, `Truncated`, `ChecksumMismatch`, `TooLarge`, `TrailingData`, `Io(error)` |
| `GzBuilder::filename(...)`, `GzDecoder::header()` | none yet | | The header holds no name or time; a header read is not given |
| `Compress::compress(..., FlushCompress::Sync)`, a preset dictionary | none yet | | |
| `zip::ZipArchive::new(File::open(path)?)` | `ZipReader.open(read path)`, `ZipReader.of(reader)` | built | The directory is checked when it is opened: `InsecurePath` (the rule of `Path.joinInside`, part by part), `DuplicateName` (in any case), `Overlap` refuse the archive |
| `for i in 0..archive.len() { archive.by_index(i)? }`, `file.name()`, `file.size()` | `for entry in zip.entries()`, `entry.name`, `entry.size` | built | `compressedSize`, `modified` (a `DateTime` in UTC), `isDirectory`, `method` too; `entries()` lends the reader's list, so an entry read while going over it is copied first: `var entry = zip.entries()[i].clone()` |
| `io::copy(&mut archive.by_index(i)?, &mut out)` | `zip.copyTo(read entry, ref writer, limit)` | built | Always a limit (`TooLarge`); an entry that opens into another size than its own is `SizeMismatch` |
| `file.read_to_end(&mut bytes)` | `zip.readBytes(read entry, limit)` | built | |
| `file.enclosed_name()` | the names themselves | built | No name of an open archive can lead out of a folder: `..`, a root, `\\`, `:`, NUL and the device names of Windows are refused, on every system |
| `zip::ZipWriter::new(file)`, `start_file(name, options)`, `write_all`, `finish` | `ZipWriter.create(writer)`, `zip.add(name, read bytes, ZipMethod.Deflated, modified)`, `zip.close()` | built | The time is given for each entry; zip64 where it is needed; the writer refuses the names a reader refuses |
| `std::io::copy(&mut file, &mut zip)` after `start_file` | `zip.addFrom(name, ref reader, method, modified)` | built | A block at a time, with a data descriptor |
| `zip.add_directory(name, options)` | `zip.addDirectory(name, modified)` | built | |
| `zip::result::ZipError` | `ZipError` | built | `NotZip`, `Corrupt(position)`, `Truncated`, `Encrypted`, `UnsupportedMethod(code)`, `InsecurePath`, `DuplicateName`, `NameMismatch`, `Overlap`, `SizeMismatch`, `ChecksumMismatch`, `TooLarge`, `Io(error)` |
| `archive.extract(dir)` | none | not planned | Each entry is read and written where the program decides (1467) |
| `ZipArchive::by_name_decrypt`, `CompressionMethod::Bzip2`, `Zstd` | none yet | | Encryption and methods other than stored and deflated |

## Tests and the tools

| Rust | SG | Status | Note |
|---|---|---|---|
| `#[test] fn name()` | `test "name" { }` | built | At the top level of the file, next to the code |
| `cargo test` | `sg test file.sg` | built | `sg test --compiled` runs them compiled, each test in a process of its own (the program starts itself again with `--only N`), where Rust's harness runs them on threads of one process |
| `cargo run` | `sg file.sg` | built | The interpreter |
| `cargo build`, `cargo build --release` | `sg build file.sg`, `sg build file.sg --release` | built | A debug build checks overflow and reports leaks. What it makes on the way is in a folder of the temporary folder only its user opens, made new and removed at the end, where Cargo keeps it in `target/`. `sg build shop` from the folder above `shop` asks for `-o`, as `go build` does, where Cargo names the program after the package in `target/`. The C compiler is the one `CC` names, as Cargo's `cc` crate takes it |
| `RUSTFLAGS="-C target-cpu=native"`, `-C lto`, `CFLAGS` of the `cc` crate | `sg build file.sg -- -march=native -flto` | built | Flags of C after `--`, after sg's own, on every build. Where Cargo passes flags on in silence, sg warns on every build for those that change fractional results or take a guard away (`-ffast-math`, `-fno-stack-clash-protection`), and refuses those it owns or that change the layout of C (`-o`, `-std=`, `-m32`, `-funsigned-char`). No flag turns off a check: that is `--release` |
| `cargo build` and `rust-gdb target/debug/app`, `break main.rs:4`; `[profile.release] debug = true` | `sg build app.sg` and `gdb ./app` (lldb on macOS), `break main.sg:4`; `sg build app.sg --release -- -g` | built | The C names the line of SG of each statement (`#line`), so the debugger stops on lines of `.sg` files and the stack names them. The names are those of the C, `f_sum` for `sum`, `v_total_1` for `total`, where `rust-gdb` shows Rust's own names and types; there is no pretty printer of `List` and `HashMap` yet. On macOS what lldb reads is in `app.dSYM`, as Rust's with `split-debuginfo = "packed"`. A program with it names the full paths of its files, and SG has no `--remap-path-prefix` |
| a test that leaks `Rc` passes | the test fails with the leak report | built | |
| a leak is seen only under a tool (LSan, Valgrind, Miri) | a debug build counts the blocks of its values and reports those not given back | built | On every system, Windows too, where there is no LSan; raw memory is not counted |
| `cargo fuzz` | `fuzz/` (libFuzzer on the front end) and `fuzz/difference` (generated programs, interpreter against compiled code) | built | A crate apart, so `sg` keeps no dependencies; the second has a generator of its own, as Csmith is for C and rustlantis for Rust. With `--borrow` its programs borrow and are fitted to the checker, a statement taken away for each error, and what is taken must read no freed memory (1593) |
| `cargo mutants` | the same tool, on the functions named in `.cargo/mutants.toml` | built | Only the rules of safety, then `--in-diff` on what a change touches |
| rust-analyzer | `sg lsp` | built | In `sg` itself, not a program of its own: the first error as you type, and four hints applied with one click; no hover, definition or completion yet. It fetches no package: one the cache has not is a message, where rust-analyzer runs `cargo metadata`, which may fetch (1591). `editors/vscode` is the extension for VS Code |
| `rustup doc --std`, `rustc --explain E0502`, the keyword pages of the std docs, `cargo build --help` | `sg help read`, `sg help index`, `sg build --help` | built | The page of a word in the terminal, carried in `sg` itself, with no network and no browser; the same text as the manual page `sg-read(7)` and the page on the site (2082) |
| a suggestion marked `MachineApplicable` | an error with a fix (`Diag::fix`) | built | Given only where the code it writes is certain to check; the risky way (`_ =` on a `Result`) is never one |
| `Cargo.toml`, `[dependencies] xml = "1.4"` | `sg.project`, `require xml example.com/ada/xml 1.4.0` | built | A line for each thing, as Go's `go.mod` (1556); the name is the one written after `use` (1555) |
| `[patch]`, `path = "../xml"` | `local xml ../xml` | built | Read only in the program's own `sg.project`, as Go's `replace` |
| `Cargo.lock` | `sg.lock` | built | A line for each package, the ones used through others too: its address, version and the SHA-256 of its files (1571, 1572), its `unsafe` and its libraries of C; checked, not chosen again, when a program runs (1587) |
| crates.io, `cargo add`, `cargo update`, `cargo remove` | a repository and a tag, `sg add`, `sg update`, `sg remove`, `sg lock` | built | No registry: a package is where its author keeps it (1552), fetched by `git` (1558); versions chosen as Go chooses them, so a new release changes no build (1577); `sg lock` makes the lock fit `sg.project` without raising a version (1580) |
| `~/.cargo/registry`, `CARGO_HOME` | the cache, `SG_CACHE` | built | A read-only folder for each address, version and SHA-256 (`example.com/ada/xml@1.4.0/sha256-...`, as Go's `pkg/mod`), written by `sg` from git's objects, so no filter of git runs, and kept only once its SHA-256 is the lock's and its `package` line its address (1573, 1579, 1591). Its folders are read only too, as Go's: `chmod -R u+w` before removing it |
| `[source]` replacement, `cargo vendor` | `SG_MIRROR` | built | A folder of bare repositories, for a closed network, as Go's `GOPROXY=file://` (1582) |
| `build.rs` | none | not in SG | No code of a package runs while it is fetched or built (1559); a package calls a C library of the system, and gives no C to compile (1560) |
| `cargo-geiger` | the `unsafe` of a `require` line | built | A package with `unsafe` or `extern` is compiled only when the program's line for it says `unsafe`, found in its source (1562, 1578); one used through another is allowed on a line of its own (1583); `sg update` stops at a version that starts to use it |
