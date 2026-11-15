//! Types as the checker sees them.

use std::collections::HashSet;
use std::fmt;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum IntTy {
    Sbyte,
    Byte,
    Short,
    Ushort,
    Int,
    Uint,
    Long,
    Ulong,
    Nint,
    Nuint,
}

impl IntTy {
    /// Every integer type, narrowest first: where a common type is looked for.
    pub const ALL: [IntTy; 10] = [
        IntTy::Sbyte,
        IntTy::Byte,
        IntTy::Short,
        IntTy::Ushort,
        IntTy::Int,
        IntTy::Uint,
        IntTy::Long,
        IntTy::Ulong,
        IntTy::Nint,
        IntTy::Nuint,
    ];

    pub fn from_name(name: &str) -> Option<IntTy> {
        IntTy::ALL.into_iter().find(|ty| ty.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            IntTy::Sbyte => "sbyte",
            IntTy::Byte => "byte",
            IntTy::Short => "short",
            IntTy::Ushort => "ushort",
            IntTy::Int => "int",
            IntTy::Uint => "uint",
            IntTy::Long => "long",
            IntTy::Ulong => "ulong",
            IntTy::Nint => "nint",
            IntTy::Nuint => "nuint",
        }
    }

    /// The name as it is written inside a method name: `Int` in `tryToInt`.
    pub fn title(self) -> String {
        let name = self.name();
        format!("{}{}", name[..1].to_uppercase(), &name[1..])
    }

    pub fn signed(self) -> bool {
        matches!(self, IntTy::Sbyte | IntTy::Short | IntTy::Int | IntTy::Long | IntTy::Nint)
    }

    /// How many bits a value takes: what a shift count has to stay under. `nint` and `nuint`
    /// take 64 bits on every machine the language runs on so far.
    pub fn bits(self) -> u32 {
        match self {
            IntTy::Sbyte | IntTy::Byte => 8,
            IntTy::Short | IntTy::Ushort => 16,
            IntTy::Int | IntTy::Uint => 32,
            IntTy::Long | IntTy::Ulong | IntTy::Nint | IntTy::Nuint => 64,
        }
    }

    /// The fewest bits the type can have on any machine: a pointer is at least 32 bits, so
    /// `nint` is promised no more than `int` is.
    fn least_bits(self) -> u32 {
        match self {
            IntTy::Nint | IntTy::Nuint => 32,
            other => other.bits(),
        }
    }

    pub fn min(self) -> i128 {
        if self.signed() {
            -(1i128 << (self.bits() - 1))
        } else {
            0
        }
    }

    pub fn max(self) -> i128 {
        if self.signed() {
            (1i128 << (self.bits() - 1)) - 1
        } else {
            (1i128 << self.bits()) - 1
        }
    }

    pub fn fits(self, value: i128) -> bool {
        (self.min()..=self.max()).contains(&value)
    }

    /// The value the bits that stay give, read in this type: what wrapping arithmetic and a
    /// release build leave.
    pub fn wrap(self, value: i128) -> i128 {
        match self {
            IntTy::Sbyte => value as i8 as i128,
            IntTy::Byte => value as u8 as i128,
            IntTy::Short => value as i16 as i128,
            IntTy::Ushort => value as u16 as i128,
            IntTy::Int => value as i32 as i128,
            IntTy::Uint => value as u32 as i128,
            IntTy::Long | IntTy::Nint => value as i64 as i128,
            IntTy::Ulong | IntTy::Nuint => value as u64 as i128,
        }
    }

    /// Whether a value of this type becomes `to` on its own, without a cast: only when `to`
    /// holds every value of this type, on every machine. A signed type never becomes an
    /// unsigned one; an unsigned one becomes a signed one that has a bit more.
    pub fn widens_to(self, to: IntTy) -> bool {
        if self == to {
            return false;
        }
        match (self.signed(), to.signed()) {
            (true, false) => false,
            (false, true) => self.bits() < to.least_bits(),
            _ => self.bits() <= to.least_bits(),
        }
    }

    /// Common type of two integer types: the one the other widens to. `None` when neither
    /// holds every value of the other, as with `int` and `uint`.
    pub fn unify(self, other: IntTy) -> Option<IntTy> {
        match (self, other) {
            (a, b) if a == b => Some(a),
            (a, b) if a.widens_to(b) => Some(b),
            (a, b) if b.widens_to(a) => Some(a),
            _ => None,
        }
    }

    /// The narrowest type that holds every value of both, if there is one: `long` for `int`
    /// and `uint`. Only the types of a fixed size are offered.
    pub fn holding_both(self, other: IntTy) -> Option<IntTy> {
        IntTy::ALL[..8].iter().copied().find(|to| (self == *to || self.widens_to(*to)) && (other == *to || other.widens_to(*to)))
    }
}

/// The two fractional types: IEEE 754 binary32 and binary64.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FloatTy {
    Float,
    Double,
}

impl FloatTy {
    pub fn from_name(name: &str) -> Option<FloatTy> {
        match name {
            "float" => Some(FloatTy::Float),
            "double" => Some(FloatTy::Double),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FloatTy::Float => "float",
            FloatTy::Double => "double",
        }
    }

    /// The bits of the significand, the hidden one counted: every integer up to 2 to this
    /// power is held exactly.
    pub fn digits(self) -> u32 {
        match self {
            FloatTy::Float => 24,
            FloatTy::Double => 53,
        }
    }

    /// Whether every value of the integer type is held exactly, on every machine: `int`
    /// becomes a `double` on its own, `long` and `nint` do not.
    pub fn holds(self, int: IntTy) -> bool {
        int.bits() - u32::from(int.signed()) <= self.digits()
    }

    /// Whether the integer is held exactly: what a number written in the program needs to
    /// stand where a fractional number goes.
    pub fn exact(self, value: i128) -> bool {
        match self {
            FloatTy::Float => value as f32 as i128 == value,
            FloatTy::Double => value as f64 as i128 == value,
        }
    }
}

/// What goes over a text, one part after the other: `text.chars()`, `text.split(',')`,
/// `text.splitWhitespace()`, `text.lines()`. It looks at the text where it is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TextIter {
    Chars,
    Split,
    SplitWhitespace,
    Lines,
    /// `reader.lines()`: each line of a file or of the input as a text of its own, until the
    /// end or the first error. It writes through the reader it reads from.
    ReadLines,
}

impl TextIter {
    /// The name it has in messages. It cannot be written in a program yet.
    pub fn name(self) -> &'static str {
        match self {
            TextIter::Chars => "Chars",
            TextIter::Split => "Split",
            TextIter::SplitWhitespace => "SplitWhitespace",
            TextIter::Lines => "Lines",
            TextIter::ReadLines => "ReadLines",
        }
    }

    /// What each round gives: a character, or a look at a part of the text.
    pub fn item(self) -> Ty {
        match self {
            TextIter::Chars => Ty::Char,
            TextIter::ReadLines => Ty::Result(Box::new(Ty::Str), Box::new(Ty::Enum("IoError".to_string()))),
            _ => Ty::Read(Box::new(Ty::Str)),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Ty {
    Int(IntTy),
    Float(FloatTy),
    Bool,
    /// `char`: one Unicode character, 32 bits. Not a number: it is converted with a cast.
    Char,
    Str,
    /// Result of `$"..."`; not a `string`, becomes one with `.toString()`.
    Fmt,
    Void,
    /// Type of `panic(...)`: fits anywhere because the program does not continue.
    Never,
    /// Type of the `null` literal before it meets a `T?`.
    Null,
    Struct(String),
    Enum(String),
    /// `T*`: owning pointer.
    Ptr(Box<Ty>),
    /// `T?`
    Nullable(Box<Ty>),
    /// `Result<T, E>`: built in, with the variants `Ok(T value)` and `Error(E cause)`.
    Result(Box<Ty>, Box<Ty>),
    /// `List<T>`: built in, owns its items on the heap.
    List(Box<Ty>),
    /// `HashMap<K, V>`: built in.
    Map(Box<Ty>, Box<Ty>),
    /// `Func<A, B, R>`: the address of a named function that takes `A` and `B` and returns `R`.
    Func(Vec<Ty>, Box<Ty>),
    /// `HashSet<T>`: built in.
    Set(Box<Ty>),
    /// What a `for` over a `HashMap<K, V>` gives: a `key` and a `value`.
    Entry(Box<Ty>, Box<Ty>),
    /// What a `format` method writes into: `ref Formatter f`.
    Formatter,
    /// `T[N]`: a fixed number of items, kept where the value is.
    Array(Box<Ty>, usize),
    /// `T[]`: the items a slice looks at. Only ever behind `read` or `ref`.
    Slice(Box<Ty>),
    /// What `text.chars()`, `text.split(...)` and the others give: it borrows the text.
    TextIter(TextIter),
    /// `StreamReader`: a file or the input, read piece by piece. Closed when it ends.
    Reader,
    /// `StreamWriter`: a file or an output, written piece by piece through a buffer. A local
    /// one must be closed (`Closable`): `close` gives the error that writing the buffer met.
    Writer,
    /// `shared T`: one of several owners of a value on the heap. With `checked` (the `bool`),
    /// `shared checked T`: what it points to can be changed, and each access is checked at run
    /// time against the borrows of it in progress. Without it what it points to is only read.
    Shared(Box<Ty>, bool),
    /// `weak T` (`weak checked T` with the `bool`): looks at a `shared` value without owning
    /// it; `share w` gives `null` once the owners are gone.
    Weak(Box<Ty>, bool),
    /// `locked T`, only ever inside `shared` or `weak`: `shared locked T`. What it holds is
    /// reached only inside a `lock` block, where no other task reaches it.
    Locked(Box<Ty>),
    /// `Task<R>`: a function started with `Task.run`, to be waited for; `R` is what it returns.
    Task(Box<Ty>),
    /// `CancellationSource`: what cancels the tasks that were given its tokens. One owner;
    /// `cancel` works through a `read` borrow, since cancelling twice is cancelling once.
    Source,
    /// `CancellationToken`: what a task looks at to know whether it is to stop. Several tokens
    /// look at one source; `clone` gives one more.
    Token,
    /// `read T`: read-only borrow.
    Read(Box<Ty>),
    /// `ref T`: writable borrow.
    Ref(Box<Ty>),
}

impl Ty {
    /// An integer or a fractional number: what `+`, `<` and a cast work on.
    pub fn is_number(&self) -> bool {
        matches!(self, Ty::Int(_) | Ty::Float(_))
    }

    pub fn from_name(name: &str) -> Option<Ty> {
        match name {
            "bool" => Some(Ty::Bool),
            "char" => Some(Ty::Char),
            "string" => Some(Ty::Str),
            "void" => Some(Ty::Void),
            "Formatter" => Some(Ty::Formatter),
            "StreamReader" => Some(Ty::Reader),
            "StreamWriter" => Some(Ty::Writer),
            "CancellationSource" => Some(Ty::Source),
            "CancellationToken" => Some(Ty::Token),
            _ => IntTy::from_name(name).map(Ty::Int).or_else(|| FloatTy::from_name(name).map(Ty::Float)),
        }
    }

    /// Whether using a value of this type leaves the original usable. Everything else moves.
    /// `declared` holds the structs and enums written with `: Copyable`. `ref` counts as
    /// copyable until there is a borrow checker.
    pub fn is_copyable(&self, declared: &HashSet<String>) -> bool {
        match self {
            Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char | Ty::Fmt | Ty::Void | Ty::Never | Ty::Null | Ty::Read(_) | Ty::Ref(_) | Ty::Func(..) => true,
            Ty::Nullable(inner) | Ty::Array(inner, _) => inner.is_copyable(declared),
            Ty::Result(ok, error) => ok.is_copyable(declared) && error.is_copyable(declared),
            Ty::Struct(name) | Ty::Enum(name) => declared.contains(name),
            Ty::Str | Ty::Ptr(_) | Ty::Shared(..) | Ty::Weak(..) | Ty::Formatter | Ty::List(_) | Ty::Map(..) | Ty::Set(_) | Ty::Entry(..) | Ty::Slice(_) | Ty::TextIter(_) | Ty::Reader | Ty::Writer | Ty::Locked(_) | Ty::Task(_) | Ty::Source | Ty::Token => false,
        }
    }

    pub fn is_borrow(&self) -> bool {
        matches!(self, Ty::Read(_) | Ty::Ref(_))
    }

    /// The type behind `read` / `ref`.
    pub fn strip_borrow(&self) -> &Ty {
        match self {
            Ty::Read(inner) | Ty::Ref(inner) => inner.strip_borrow(),
            other => other,
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Ty::Int(ty) => f.write_str(ty.name()),
            Ty::Float(ty) => f.write_str(ty.name()),
            Ty::Bool => f.write_str("bool"),
            Ty::Char => f.write_str("char"),
            Ty::Str => f.write_str("string"),
            Ty::Fmt => f.write_str("formatted text"),
            Ty::Void => f.write_str("void"),
            Ty::Never => f.write_str("never"),
            Ty::Null => f.write_str("null"),
            Ty::Struct(name) | Ty::Enum(name) => f.write_str(name),
            Ty::Formatter => f.write_str("Formatter"),
            Ty::Ptr(inner) => write!(f, "{inner}*"),
            Ty::Shared(inner, checked) => write!(f, "shared {}{inner}", if *checked { "checked " } else { "" }),
            Ty::Weak(inner, checked) => write!(f, "weak {}{inner}", if *checked { "checked " } else { "" }),
            Ty::Nullable(inner) => write!(f, "{inner}?"),
            Ty::Result(ok, error) => write!(f, "Result<{ok}, {error}>"),
            Ty::List(item) => write!(f, "List<{item}>"),
            Ty::Map(key, value) => write!(f, "HashMap<{key}, {value}>"),
            Ty::Set(item) => write!(f, "HashSet<{item}>"),
            Ty::Func(params, ret) => {
                f.write_str("Func<")?;
                for param in params {
                    write!(f, "{param}, ")?;
                }
                write!(f, "{ret}>")
            }
            Ty::Entry(key, value) => write!(f, "entry of HashMap<{key}, {value}>"),
            Ty::Array(item, length) => write!(f, "{item}[{length}]"),
            Ty::Slice(item) => write!(f, "{item}[]"),
            Ty::TextIter(kind) => f.write_str(kind.name()),
            Ty::Reader => f.write_str("StreamReader"),
            Ty::Writer => f.write_str("StreamWriter"),
            Ty::Locked(inner) => write!(f, "locked {inner}"),
            Ty::Task(result) => write!(f, "Task<{result}>"),
            Ty::Source => f.write_str("CancellationSource"),
            Ty::Token => f.write_str("CancellationToken"),
            Ty::Read(inner) => write!(f, "read {inner}"),
            Ty::Ref(inner) => write!(f, "ref {inner}"),
        }
    }
}
