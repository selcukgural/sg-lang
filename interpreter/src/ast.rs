//! Syntax tree. The checker fills in a few resolved details (literal types, field
//! indexes, implicit conversions) so the interpreter does not repeat its work.

use crate::diag::Span;
use crate::types::{IntTy, Ty};

pub struct Program {
    pub structs: Vec<StructDecl>,
    pub enums: Vec<EnumDecl>,
    pub funcs: Vec<Func>,
    pub tests: Vec<TestDecl>,
}

/// `test "name" { ... }`: compiled and run only by the test command.
pub struct TestDecl {
    pub name: String,
    pub name_span: Span,
    pub body: Block,
}

pub struct StructDecl {
    pub name: String,
    pub name_span: Span,
    /// The names in `struct Token<from source>`: where the borrows it carries come from.
    pub sources: Vec<(String, Span)>,
    /// The names after `:` in `struct Version : Copyable, Formattable`.
    pub interfaces: Vec<(String, Span)>,
    pub fields: Vec<FieldDecl>,
    /// Functions written in the struct body, with or without `this`.
    pub methods: Vec<Func>,
}

pub struct EnumDecl {
    pub name: String,
    pub name_span: Span,
    pub sources: Vec<(String, Span)>,
    pub interfaces: Vec<(String, Span)>,
    pub variants: Vec<VariantDecl>,
}

/// `Circle(int radius)` or `Empty`
pub struct VariantDecl {
    pub name: String,
    pub span: Span,
    pub fields: Vec<FieldDecl>,
}

pub struct FieldDecl {
    pub ty: TypeExpr,
    pub name: String,
    pub span: Span,
}

pub struct Func {
    pub ret: TypeExpr,
    pub name: String,
    pub name_span: Span,
    /// How a method takes its receiver. `None` for a function without `this`.
    pub this: Option<This>,
    pub params: Vec<Param>,
    /// What is written after `from`: where a returned borrow comes from.
    pub from: Vec<FromItem>,
    pub body: Block,
}

/// One name after `from`: a parameter (`input`), the source of the type a method belongs to
/// (`source`), or the source of a parameter's type (`lx.source`).
pub struct FromItem {
    pub name: String,
    pub source: Option<String>,
    pub span: Span,
}

/// The first parameter of a method: `this`, `read this` or `ref this`.
#[derive(Clone, Copy, PartialEq)]
pub enum This {
    /// The method uses the value up.
    Value,
    Read,
    Ref,
}

/// What a call runs. Set by the checker.
#[derive(Clone, PartialEq)]
pub enum Callee {
    /// A top-level function, a built-in function or a built-in method.
    Plain,
    /// A function of a struct that has no `this`: `Stack.create()`, `Stack()`.
    Static { owner: String, name: String },
    /// A method of a struct, called on a value.
    Method { owner: String, this: This },
    /// `string.join(separator, read names)`
    Join,
    /// A call through a function value kept in a variable: `test(order)`.
    Value,
    /// A call through a function value kept in a field: `this.test(order)`; the field's position.
    FieldValue(usize),
    /// `slot.take()`: gives the value and leaves `null`.
    Take,
    /// `slot.replace(value)`: gives the old value and leaves the new one.
    Replace,
}

/// A type as written: `read Node*?`
pub struct TypeExpr {
    pub mode: Option<Mode>,
    pub name: String,
    /// The types between `<` and `>`: `Result<int, ParseError>`.
    pub args: Vec<TypeExpr>,
    pub suffixes: Vec<Suffix>,
    pub span: Span,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Read,
    Ref,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Suffix {
    /// `*`
    Pointer,
    /// `?`
    Nullable,
}

pub struct Param {
    pub ty: TypeExpr,
    pub name: String,
    pub span: Span,
}

pub type Block = Vec<Stmt>;

pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

pub enum StmtKind {
    /// `var x = e` or `int x = e`
    Let { ty: Option<TypeExpr>, name: String, name_span: Span, init: Expr },
    /// `x = e`, `a.b += e`. `through`: the target is a `ref` variable and the write goes to
    /// what it borrows (set by the checker).
    Assign { target: Expr, op: Option<BinOp>, value: Expr, through: bool },
    If { cond: Expr, then: Block, els: Option<Block> },
    While { cond: Expr, body: Block },
    /// `for i in a..b`
    For { var: String, var_span: Span, from: Expr, to: Expr, body: Block },
    /// `for x in xs`, `for x in read xs`, `for x in ref xs`. `by`: the items are lent to the
    /// loop, not handed over (set by the checker).
    ForEach { var: String, var_span: Span, iter: Expr, body: Block, by: Option<Mode> },
    Return(Option<Expr>),
    Break,
    Continue,
    Expr(Expr),
    /// `_ = e`: runs `e` and drops its value on purpose.
    Discard(Expr),
}

pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    /// The type of the value, written by the checker; `void` until then. The back end reads it.
    pub ty: Ty,
    /// Written by the checker: this use moves the value out of its place. The back end
    /// empties the place, so that what is dropped later is dropped once.
    pub moved: bool,
}

pub enum ExprKind {
    /// The type is decided by the checker from where the literal is used.
    Int(i64, IntTy),
    Bool(bool),
    Str(String),
    /// `'+'`. The checker turns it into an `Int` of the type `byte`.
    Char(char),
    /// `$"{a}: {b}"`
    Interp(Vec<InterpPart>),
    Null,
    Var(String),
    /// `a.b`; `index` is the field's position, set by the checker.
    Field { recv: Box<Expr>, name: String, name_span: Span, index: usize },
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call { name: String, name_span: Span, args: Vec<Expr>, callee: Callee },
    Method { recv: Box<Expr>, name: String, name_span: Span, args: Vec<Expr>, callee: Callee },
    /// `text[i]`, `xs[i]`, `map[key]`. `kind` is set by the checker.
    Index { recv: Box<Expr>, index: Box<Expr>, kind: IndexKind },
    /// `List<int>()`, `List<int> { 1, 2 }`, `HashMap<string, int> { {"x", 10} }`. `items` is
    /// `None` when no braces were written.
    Collection { ty: TypeExpr, args: Vec<Expr>, items: Option<Vec<Item>> },
    /// `text[a..b]`, `text[..b]`, `text[a..]`, `text[..]`
    Slice { recv: Box<Expr>, from: Option<Box<Expr>>, to: Option<Box<Expr>> },
    /// `(int)x`
    Cast { ty: IntTy, expr: Box<Expr> },
    /// `Node { value = 1, next = null }`
    StructLit { name: String, name_span: Span, fields: Vec<FieldInit> },
    /// `new e`: puts the value on the heap, gives `T*`.
    New(Box<Expr>),
    /// `read e` / `ref e`
    Borrow { mutable: bool, expr: Box<Expr> },
    /// `e is null` / `e is not null`
    IsNull { expr: Box<Expr>, negated: bool },
    /// `a ?? b`. `on_result`: the left side is a `Result`, not a `T?` (set by the checker).
    Coalesce { left: Box<Expr>, right: Box<Expr>, on_result: bool },
    /// `try e`: gives the value, or returns the error (or `null`) to the caller.
    Try { expr: Box<Expr>, kind: TryKind },
    /// `return e` on the right side of `??`.
    Return(Option<Box<Expr>>),
    /// `e is Circle(r)` / `e is not Circle`. `by_ref`: the names are `ref` borrows, otherwise
    /// `read` (set by the checker).
    IsVariant { expr: Box<Expr>, negated: bool, pattern: Pattern, by_ref: bool },
    /// `match e { ... }`. `borrowed`: the names in the arms borrow from the value instead of
    /// taking what it holds (set by the checker).
    Match { subject: Box<Expr>, arms: Vec<Arm>, borrowed: bool },

    /// Built by the checker from `Shape.Circle(2)`, `Shape.Empty`, `Ok(1)` and `Error(e)`: the
    /// enum (`Result` for the last two), the variant's position and what it carries. `Ok` is
    /// 0 and `Error` is 1.
    Variant { owner: String, index: usize, args: Vec<Expr> },
    /// Inserted by the checker: a borrow of a `T*` used as a borrow of the `T` it owns.
    Unbox(Box<Expr>),

    /// Built by the checker from the name of a function used as a value: `isPaid`.
    FuncRef(String),

    /// Inserted by the checker: `int` value used where a wider integer is expected.
    Widen(IntTy, Box<Expr>),
    /// Inserted by the checker: a borrow of a copyable value used as the value itself.
    Load(Box<Expr>),
}

/// What `x[i]` reaches into.
#[derive(Clone, Copy, PartialEq)]
pub enum IndexKind {
    /// A `string`: one byte, read only.
    Text,
    List,
    Map,
}

/// One item between the braces of a collection: `1`, or `{"x", 10}` for a key and a value.
pub struct Item {
    pub values: Vec<Expr>,
    pub braced: bool,
    pub span: Span,
}

/// What `try` does when there is no value. Set by the checker.
#[derive(Clone, PartialEq)]
pub enum TryKind {
    /// On a `T?`: returns `null`.
    Null,
    /// On a `Result`: returns the error. With an enum and a position, the error is first
    /// wrapped in that variant of the caller's error type.
    Result(Option<(String, usize)>),
}

/// One arm of a `match`: `Circle(r) when r > 0 => body`. The body is a single expression
/// (`value` alone), a statement such as `return`, or a block whose last line may be the value.
pub struct Arm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Block,
    pub value: Option<Expr>,
    pub span: Span,
}

pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

pub enum PatternKind {
    /// `_`
    Wildcard,
    /// `var other`: matches everything and takes the value.
    Bind(String),
    /// `42`, `-1`: a number, for a `match` on an integer.
    Int(i64),
    /// `'+'`. The checker turns it into an `Int`.
    Char(char),
    /// `Circle(r)`, `Rect(w, _)`, `Empty`. A name is `None` for `_`. `args` is `None` when
    /// no parentheses were written. `index` is the variant's position, set by the checker.
    Variant { name: String, args: Option<Vec<(Option<String>, Span)>>, index: usize },
}

pub struct FieldInit {
    pub name: String,
    pub span: Span,
    pub value: Expr,
    /// Position of the field in the struct, set by the checker.
    pub index: usize,
}

pub enum InterpPart {
    Text(String),
    Expr(Expr),
}

#[derive(Clone, Copy, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }

    pub fn is_comparison(self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }
}

impl Expr {
    /// A variable, a field or an item: something that has a location and can be borrowed.
    pub fn is_place(&self) -> bool {
        matches!(self.kind, ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. })
    }

    /// Whether the place lies inside an item of a collection: `xs[i]`, `xs[i].name`.
    pub fn in_item(&self) -> bool {
        match &self.kind {
            ExprKind::Index { .. } => true,
            ExprKind::Field { recv, .. } => recv.in_item(),
            _ => false,
        }
    }
}
