//! Syntax tree. The checker fills in a few resolved details (literal types, field
//! indexes, implicit conversions) so the interpreter does not repeat its work.

use crate::diag::Span;
use crate::types::{FloatTy, IntTy, Ty};

pub struct Program {
    pub structs: Vec<StructDecl>,
    pub enums: Vec<EnumDecl>,
    pub funcs: Vec<Func>,
    pub tests: Vec<TestDecl>,
    /// `using FolderRef = shared Folder`: another name for a type.
    pub aliases: Vec<AliasDecl>,
    /// The files the program was read from, in the order of their text.
    pub files: Vec<FileScope>,
}

/// One file: where its text lies, the module it belongs to (empty for the root, otherwise the
/// path of its folder: `shop.stock`) and the modules it names with `use`.
///
/// A type or a function of a module is kept under its full name (`stock.Product`); in the root
/// the full name is the name alone. The checker turns every name written in the program into
/// the full one.
#[derive(Clone)]
pub struct FileScope {
    pub start: usize,
    pub end: usize,
    pub module: String,
    pub uses: Vec<(String, Span)>,
}

/// The module in a full name: `shop.stock` in `shop.stock.Product`, nothing for a name of the
/// root.
pub fn module_of(full: &str) -> &str {
    full.rsplit_once('.').map_or("", |(module, _)| module)
}

/// A full name as it is written out by `print`: without its module.
pub fn short_name(full: &str) -> &str {
    full.rsplit_once('.').map_or(full, |(_, name)| name)
}

/// `test "name" { ... }`: compiled and run only by the test command.
pub struct TestDecl {
    pub name: String,
    pub name_span: Span,
    pub body: Block,
}

/// `using NodeRef = shared Node`: a second name for a type, not a new type.
pub struct AliasDecl {
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub ty: TypeExpr,
}

pub struct StructDecl {
    /// Written with `public`: other modules see it.
    pub public: bool,
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
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub sources: Vec<(String, Span)>,
    pub interfaces: Vec<(String, Span)>,
    pub variants: Vec<VariantDecl>,
    /// Functions written in the enum body after or between the variants, with or without
    /// `this`.
    pub methods: Vec<Func>,
}

/// `Circle(int radius)` or `Empty`
pub struct VariantDecl {
    pub name: String,
    pub span: Span,
    pub fields: Vec<FieldDecl>,
}

pub struct FieldDecl {
    pub public: bool,
    /// `readonly`: given when the value is built, never changed afterwards.
    pub readonly: bool,
    pub ty: TypeExpr,
    pub name: String,
    pub span: Span,
}

pub struct Func {
    pub public: bool,
    pub ret: TypeExpr,
    pub name: String,
    pub name_span: Span,
    /// How a method takes its receiver. `None` for a function without `this`.
    pub this: Option<This>,
    /// The type written in front of `this` (`read FolderRef this`): the method is called on
    /// the value that holds the struct, not on the struct.
    pub this_ty: Option<TypeExpr>,
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
    /// A method of a struct, called on a value. `whole`: the method takes the value that
    /// holds the struct (`read FolderRef this`), not the struct.
    Method { owner: String, this: This, whole: bool },
    /// `string.join(separator, read names)`
    Join,
    /// `string.fromUtf8(bytes)`
    FromUtf8,
    /// `string.withCapacity(n)`
    WithCapacity,
    /// `int.parse(text)`, `double.parse(text)`, `int.parseHex(text)`: the type is the one in
    /// the `Result` it gives; the number is the base the digits are read in (10 or 16).
    Parse(u32),
    /// `ulong.fromLittleEndian(bytes)`, `ushort.fromBigEndian(bytes)`: the integer that exactly
    /// as many bytes as the type takes write, the first byte the lowest (or the highest).
    FromBytes { ty: IntTy, big: bool },
    /// `File.readText(path)`, `File.openText(path)` and the other functions of `File`.
    Io(IoCall),
    /// `reader.readLine(ref line)`, `writer.write(x)` and the other methods of a
    /// `StreamReader` or a `StreamWriter`, `Stdin` and `Stdout` among them.
    Stream(StreamCall),
    /// A call through a function value kept in a variable: `test(order)`.
    Value,
    /// A call through a function value kept in a field: `this.test(order)`; the field's position.
    FieldValue(usize),
    /// `Task.run(work, x)`: starts the function given first with the arguments after it.
    TaskRun,
    /// `t.wait()`: waits for the task and gives what its function returned; takes the task.
    Wait,
    /// `slot.take()`: gives the value and leaves `null`.
    Take,
    /// `slot.replace(value)`: gives the old value and leaves the new one.
    Replace,
}

/// The functions that read and write files and the input of the program. Each gives a
/// `Result` whose error is `IoError`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum IoCall {
    /// `File.readText(path)`: the whole file as text, checked to be UTF-8.
    ReadText,
    /// `File.readBytes(path)`: the whole file as a `List<byte>`.
    ReadBytes,
    /// `File.writeText(path, text)`: the file is made, or emptied, and gets the text.
    WriteText,
    /// `File.writeBytes(path, bytes)`
    WriteBytes,
    /// `File.appendText(path, text)`: the text goes after what the file holds; a missing file is made.
    AppendText,
    /// `File.openText(path)`: a `StreamReader` that reads the file piece by piece.
    OpenText,
    /// `File.createText(path)`: a `StreamWriter` into the file, which is made or emptied.
    CreateText,
    /// `File.openAppend(path)`: a `StreamWriter` that writes after what the file holds; a
    /// missing file is made.
    OpenAppend,
}

impl IoCall {
    /// The name as it is written after the dot.
    pub fn name(self) -> &'static str {
        match self {
            IoCall::ReadText => "readText",
            IoCall::ReadBytes => "readBytes",
            IoCall::WriteText => "writeText",
            IoCall::WriteBytes => "writeBytes",
            IoCall::AppendText => "appendText",
            IoCall::OpenText => "openText",
            IoCall::CreateText => "createText",
            IoCall::OpenAppend => "openAppend",
        }
    }
}

/// The methods of `StreamReader` and `StreamWriter`. Each that reads or writes gives a
/// `Result` whose error is `IoError`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StreamCall {
    /// `reader.readLine(ref line)`: the next line into `line`, `false` at the end.
    ReadLine,
    /// `reader.readText()`: what is left, as text.
    ReadText,
    /// `reader.lines()`: each line as a text of its own.
    Lines,
    /// `reader.readBytes(ref buffer)`: bytes into the buffer, as many as it holds unless the
    /// end comes first; gives how many.
    ReadBytes,
    /// `writer.write(x)`: what `print` takes, without a line end.
    Write,
    /// `writer.writeLine(x)`: as `write`, and a `\n`.
    WriteLine,
    /// `writer.writeBytes(bytes)`: the bytes as they are.
    WriteBytes,
    /// `writer.flush()`: what the buffer holds goes out.
    Flush,
    /// `writer.close()`: the buffer goes out and the file is closed; takes the writer.
    Close,
}

/// A type as written: `read Node*?`
#[derive(Clone)]
pub struct TypeExpr {
    pub mode: Option<Mode>,
    /// `shared` or `weak` in front of the name.
    pub holder: Option<Holder>,
    /// `checked` after the holder: `shared checked T`.
    pub checked: bool,
    /// `locked` after the holder: `shared locked T`.
    pub locked: bool,
    /// With its module when one is written: `billing.Line`.
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

/// What holds a value that has several owners.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Holder {
    /// `shared T`: an owner.
    Shared,
    /// `weak T`: looks without owning.
    Weak,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Suffix {
    /// `*`
    Pointer,
    /// `?`
    Nullable,
    /// `[8]`: a fixed number of items.
    Array(usize),
    /// `[]`: a slice, behind `read` or `ref`.
    Slice,
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
    /// `after`: the names of an `is` in the condition that the code after the statement sees
    /// (`if r is not Ok(value) { return }`), set by the checker.
    If { cond: Expr, then: Block, els: Option<Block>, after: Vec<String> },
    While { cond: Expr, body: Block, after: Vec<String> },
    /// `for i in a..b`
    For { var: String, var_span: Span, from: Expr, to: Expr, body: Block },
    /// `for x in xs`, `for x in read xs`, `for x in ref xs`. `by`: the items are lent to the
    /// loop, not handed over (set by the checker).
    ForEach { var: String, var_span: Span, iter: Expr, body: Block, by: Option<Mode> },
    /// `read folder as f { ... }`, `ref folder as f { ... }`: what a `shared` value holds,
    /// lent under a name for the block. `value` is the `shared` value (opened by the checker).
    As { mutable: bool, value: Expr, name: String, name_span: Span, body: Block },
    /// `lock total as c { ... }`, `lock a as x, b as y { ... }`: what `shared locked` values
    /// hold, each lent to change under a name for the block, while no other task reaches it.
    Lock { items: Vec<LockItem>, body: Block },
    Return(Option<Expr>),
    Break,
    Continue,
    Expr(Expr),
    /// `_ = e`: runs `e` and drops its value on purpose.
    Discard(Expr),
}

/// One value of a `lock` block and the name of what it holds.
pub struct LockItem {
    pub value: Expr,
    pub name: String,
    pub name_span: Span,
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
    Int(i128, IntTy),
    /// A fractional number as written, without the `_`; the type is decided like an `Int`'s.
    /// A whole number written where a fractional one goes becomes one of these.
    Float(String, FloatTy),
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
    /// `int[3] { 1, 2, 3 }`, `byte[16](255)`, `byte[16]`. `ty` is the whole type, `int[3]`.
    /// `items` is `None` when no braces were written.
    Array { ty: TypeExpr, args: Vec<Expr>, items: Option<Vec<Expr>> },
    /// `text[a..b]`, `text[..b]`, `text[a..]`, `text[..]`, and the same on a list, an array or
    /// a slice.
    Slice { recv: Box<Expr>, from: Option<Box<Expr>>, to: Option<Box<Expr>> },
    /// `(int)x`, `(double)n`: `ty` is an integer or a fractional type.
    Cast { ty: Ty, expr: Box<Expr> },
    /// `Node { value = 1, next = null }`
    StructLit { name: String, name_span: Span, fields: Vec<FieldInit> },
    /// `new e`: puts the value on the heap, gives `T*`.
    New(Box<Expr>),
    /// `new shared e`: puts the value on the heap with a count of its owners, gives `shared T`.
    /// `new shared checked e` gives `shared checked T`, `new shared locked e` gives
    /// `shared locked T`.
    NewShared { value: Box<Expr>, checked: bool, locked: bool },
    /// `share e`: one more owner of a `shared T`, or of what a `weak T` looks at (`null` once
    /// it is gone). Does not move `e`.
    Share(Box<Expr>),
    /// `weak e`: looks at a `shared T` without owning it. Does not move `e`.
    WeakOf(Box<Expr>),
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
    /// Inserted by the checker: a borrow of a `T*` (or of a `shared T`) used as a borrow of
    /// the `T` it owns.
    Unbox(Box<Expr>),

    /// Built by the checker from `int.MaxValue`, `double.NaN`: a constant of a number type.
    Constant { ty: Ty, name: String },

    /// Built by the checker from the name of a function used as a value: `isPaid`.
    FuncRef(String),

    /// Inserted by the checker: a borrow of a list or an array used as a slice.
    AsSlice(Box<Expr>),
    /// Inserted by the checker: an `int` used where a wider integer is expected, or a number
    /// where a fractional type that holds every value of it is expected. `Ty` is the type
    /// it becomes.
    Widen(Ty, Box<Expr>),
    /// Inserted by the checker: a borrow of a copyable value used as the value itself.
    Load(Box<Expr>),
    /// Inserted by the checker around a `shared checked T` whose value is reached: the access
    /// is checked against the borrows of it in progress, and a borrow that goes on after it
    /// is recorded (see `Opening`). Gives the same value.
    Open { inner: Box<Expr>, how: Opening },
    /// Inserted by the checker: the borrows of `shared checked` values recorded while this is
    /// computed end when it ends (a call whose result borrows nothing, a condition).
    Within(Box<Expr>),
}

/// How a `shared checked T` is reached.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Opening {
    /// Read once: a value copied out, a field on the way to another.
    Look,
    /// Changed once: the target of an assignment.
    Change,
    /// Lent to read for longer: to a call, to a `for`, a `match`, an `as` block.
    Lend,
    /// Lent to change for longer.
    LendChange,
    /// The receiver of a method that changes it: lent to read while the arguments are
    /// computed, then to change for the call.
    Reserve,
}

/// What `x[i]` reaches into.
#[derive(Clone, Copy, PartialEq)]
pub enum IndexKind {
    /// A `string`: one byte, read only.
    Text,
    List,
    Map,
    /// A `T[N]` or a slice.
    Array,
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

#[derive(Clone)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Clone)]
pub enum PatternKind {
    /// `_`
    Wildcard,
    /// `var other`: matches everything and takes the value.
    Bind(String),
    /// `42`, `-1`: a number, for a `match` on an integer.
    Int(i128),
    /// `'+'`. The checker turns it into an `Int`.
    Char(char),
    /// `"if"`: a text, for a `match` on text.
    Str(String),
    /// `Circle(r)`, `Rect(w, _)`, `Add(Number(a), _)`, `Empty`. `args` is `None` when no
    /// parentheses were written; each one is a pattern for what the variant carries there.
    /// Inside the parentheses a name alone takes the value (`Bind`), unless the checker finds
    /// that it names a variant that carries nothing. `index` is the variant's position, set
    /// by the checker.
    Variant { name: String, args: Option<Vec<Pattern>>, index: usize },
    /// `Add(_, _) or Multiply(_, _)`: a value that matches one of the patterns. Each one names
    /// the same values, with the same types.
    Or(Vec<Pattern>),
    /// `'a'..='z'`, `0..=9`: a number or a character from the first to the last, both taken.
    /// Each end is an `Int` or a `Char`; the checker turns both into `Int`.
    Range(Box<Pattern>, Box<Pattern>),
    /// `null`, for a value that may be `null`.
    Null,
    /// `true`, `false`.
    Bool(bool),
}

impl Pattern {
    /// Whether the pattern takes every value: `_` or a name.
    pub fn takes_all(&self) -> bool {
        matches!(self.kind, PatternKind::Wildcard | PatternKind::Bind(_))
    }

    /// Whether a pattern is written inside this one in place of a name or `_`.
    pub fn is_nested(&self) -> bool {
        match &self.kind {
            PatternKind::Variant { args: Some(args), .. } => args.iter().any(|arg| !arg.takes_all()),
            _ => false,
        }
    }

    /// Whether the pattern needs the search of `exhaust` to tell what it takes: a pattern
    /// inside a pattern, an `or`, a range, `null`, `true` or `false`.
    pub fn needs_search(&self) -> bool {
        match &self.kind {
            PatternKind::Variant { args: Some(args), .. } => args.iter().any(|arg| !arg.takes_all()),
            PatternKind::Or(_) | PatternKind::Range(..) | PatternKind::Null | PatternKind::Bool(_) => true,
            _ => false,
        }
    }

    /// The names the pattern declares, in the order they are written; of an `or`, those of its
    /// first pattern.
    pub fn names(&self) -> Vec<&str> {
        let mut found = Vec::new();
        self.collect_names(&mut found);
        found
    }

    fn collect_names<'p>(&'p self, found: &mut Vec<&'p str>) {
        match &self.kind {
            PatternKind::Bind(name) => found.push(name),
            PatternKind::Variant { args: Some(args), .. } => args.iter().for_each(|arg| arg.collect_names(found)),
            PatternKind::Or(alternatives) => alternatives[0].collect_names(found),
            _ => {}
        }
    }
}

pub struct FieldInit {
    pub name: String,
    pub span: Span,
    pub value: Expr,
    /// Position of the field in the struct, set by the checker.
    pub index: usize,
}

// A value is far larger than a piece of text, as it was before the spec came; a formatted
// text has few parts, and a box for each value would cost more than it saves.
#[allow(clippy::large_enum_variant)]
pub enum InterpPart {
    Text(String),
    /// A value, and how it is written when the text says more than "as `print` writes it".
    Expr(Expr, Option<Spec>),
}

/// What follows the value inside `{ }`: `{n,5}`, `{name,-10}`, `{b:X2}`, `{price,8:F2}`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Spec {
    /// Characters the value takes at least, filled with spaces: on the left (the value
    /// stands to the right) when positive, on the right when negative. 0 when none is written.
    pub width: i32,
    /// The letter after `:` and the number after it.
    pub letter: Option<(Letter, Option<u32>)>,
    /// Where the part after the value is written, for the errors about it.
    pub span: Span,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Letter {
    /// `x`: an integer in hex with small letters; the number is the digits it has at least.
    Hex,
    /// `X`: the same with capital letters.
    HexUpper,
    /// `b`: an integer in binary.
    Binary,
    /// `D`: an integer in decimal, with at least this many digits.
    Decimal,
    /// `F`: a fractional number with exactly this many digits after the point.
    Fixed,
}

impl Letter {
    /// As it is written after `:`.
    pub fn name(self) -> char {
        match self {
            Letter::Hex => 'x',
            Letter::HexUpper => 'X',
            Letter::Binary => 'b',
            Letter::Decimal => 'D',
            Letter::Fixed => 'F',
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
    /// `~x`: every bit turned over.
    BitNot,
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
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
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
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
        }
    }

    /// `<<` and `>>`: the right side is a count, not a value of the same kind as the left.
    pub fn is_shift(self) -> bool {
        matches!(self, BinOp::Shl | BinOp::Shr)
    }

    pub fn is_comparison(self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }
}

impl Expr {
    /// A variable, a field or an item: something that has a location and can be borrowed.
    pub fn is_place(&self) -> bool {
        match &self.kind {
            ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. } => true,
            // A `shared checked` value opened where it stands is still where it was.
            ExprKind::Open { inner, .. } => inner.is_place(),
            _ => false,
        }
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
