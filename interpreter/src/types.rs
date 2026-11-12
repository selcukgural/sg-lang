//! Types as the checker sees them.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum IntTy {
    Int,
    Long,
    Nint,
}

impl IntTy {
    pub fn from_name(name: &str) -> Option<IntTy> {
        match name {
            "int" => Some(IntTy::Int),
            "long" => Some(IntTy::Long),
            "nint" => Some(IntTy::Nint),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            IntTy::Int => "int",
            IntTy::Long => "long",
            IntTy::Nint => "nint",
        }
    }

    pub fn fits(self, value: i128) -> bool {
        match self {
            IntTy::Int => i32::try_from(value).is_ok(),
            IntTy::Long | IntTy::Nint => i64::try_from(value).is_ok(),
        }
    }

    pub fn wrap(self, value: i128) -> i64 {
        match self {
            IntTy::Int => value as i32 as i64,
            IntTy::Long | IntTy::Nint => value as i64,
        }
    }

    /// Common type of two integer types: `int` widens to the other.
    pub fn unify(self, other: IntTy) -> IntTy {
        match (self, other) {
            (a, b) if a == b => a,
            (IntTy::Int, b) => b,
            (a, IntTy::Int) => a,
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
            _ => IntTy::from_name(name).map(Ty::Int),
        }
    }

    /// Whether using a value of this type leaves the original usable. Everything else moves.
    /// `ref` counts as copyable until there is a borrow checker.
    pub fn is_copyable(&self) -> bool {
        match self {
            Ty::Int(_) | Ty::Bool | Ty::Fmt | Ty::Void | Ty::Never | Ty::Null | Ty::Read(_) | Ty::Ref(_) => true,
            Ty::Nullable(inner) => inner.is_copyable(),
            Ty::Result(ok, error) => ok.is_copyable() && error.is_copyable(),
            Ty::Str | Ty::Struct(_) | Ty::Enum(_) | Ty::Ptr(_) => false,
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
            Ty::Ptr(inner) => write!(f, "{inner}*"),
            Ty::Nullable(inner) => write!(f, "{inner}?"),
            Ty::Result(ok, error) => write!(f, "Result<{ok}, {error}>"),
            Ty::Read(inner) => write!(f, "read {inner}"),
            Ty::Ref(inner) => write!(f, "ref {inner}"),
        }
    }
}
