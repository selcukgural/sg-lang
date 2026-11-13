//! Types as the checker sees them.

use std::collections::HashSet;
use std::fmt;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum IntTy {
    Byte,
    Int,
    Long,
    Nint,
}

impl IntTy {
    pub fn from_name(name: &str) -> Option<IntTy> {
        match name {
            "byte" => Some(IntTy::Byte),
            "int" => Some(IntTy::Int),
            "long" => Some(IntTy::Long),
            "nint" => Some(IntTy::Nint),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            IntTy::Byte => "byte",
            IntTy::Int => "int",
            IntTy::Long => "long",
            IntTy::Nint => "nint",
        }
    }

    pub fn fits(self, value: i128) -> bool {
        match self {
            IntTy::Byte => u8::try_from(value).is_ok(),
            IntTy::Int => i32::try_from(value).is_ok(),
            IntTy::Long | IntTy::Nint => i64::try_from(value).is_ok(),
        }
    }

    pub fn wrap(self, value: i128) -> i64 {
        match self {
            IntTy::Byte => value as u8 as i64,
            IntTy::Int => value as i32 as i64,
            IntTy::Long | IntTy::Nint => value as i64,
        }
    }

    /// `byte` is narrower than `int`, which is narrower than `long` and `nint`.
    fn rank(self) -> u8 {
        match self {
            IntTy::Byte => 0,
            IntTy::Int => 1,
            IntTy::Long | IntTy::Nint => 2,
        }
    }

    /// Whether a value of this type becomes `to` on its own, without a cast.
    pub fn widens_to(self, to: IntTy) -> bool {
        self.rank() < to.rank()
    }

    /// Common type of two integer types: the narrower one widens to the other.
    pub fn unify(self, other: IntTy) -> IntTy {
        match (self, other) {
            (a, b) if a == b => a,
            (a, b) if a.widens_to(b) => b,
            (a, b) if b.widens_to(a) => a,
            _ => IntTy::Long,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Ty {
    Int(IntTy),
    Bool,
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
    /// `read T`: read-only borrow.
    Read(Box<Ty>),
    /// `ref T`: writable borrow.
    Ref(Box<Ty>),
}

impl Ty {
    pub fn from_name(name: &str) -> Option<Ty> {
        match name {
            "bool" => Some(Ty::Bool),
            "string" => Some(Ty::Str),
            "void" => Some(Ty::Void),
            "Formatter" => Some(Ty::Formatter),
            _ => IntTy::from_name(name).map(Ty::Int),
        }
    }

    /// Whether using a value of this type leaves the original usable. Everything else moves.
    /// `declared` holds the structs and enums written with `: Copyable`. `ref` counts as
    /// copyable until there is a borrow checker.
    pub fn is_copyable(&self, declared: &HashSet<String>) -> bool {
        match self {
            Ty::Int(_) | Ty::Bool | Ty::Fmt | Ty::Void | Ty::Never | Ty::Null | Ty::Read(_) | Ty::Ref(_) | Ty::Func(..) => true,
            Ty::Nullable(inner) | Ty::Array(inner, _) => inner.is_copyable(declared),
            Ty::Result(ok, error) => ok.is_copyable(declared) && error.is_copyable(declared),
            Ty::Struct(name) | Ty::Enum(name) => declared.contains(name),
            Ty::Str | Ty::Ptr(_) | Ty::Formatter | Ty::List(_) | Ty::Map(..) | Ty::Set(_) | Ty::Entry(..) | Ty::Slice(_) => false,
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
            Ty::Bool => f.write_str("bool"),
            Ty::Str => f.write_str("string"),
            Ty::Fmt => f.write_str("formatted text"),
            Ty::Void => f.write_str("void"),
            Ty::Never => f.write_str("never"),
            Ty::Null => f.write_str("null"),
            Ty::Struct(name) | Ty::Enum(name) => f.write_str(name),
            Ty::Formatter => f.write_str("Formatter"),
            Ty::Ptr(inner) => write!(f, "{inner}*"),
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
            Ty::Read(inner) => write!(f, "read {inner}"),
            Ty::Ref(inner) => write!(f, "ref {inner}"),
        }
    }
}
