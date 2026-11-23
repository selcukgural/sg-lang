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
    /// The types of the fields of each struct that holds only plain data (numbers, `bool`,
    /// `char`, raw pointers, and arrays and structs of them), in the order they are declared:
    /// what lies in raw memory, laid out as C lays it out. Filled in by the checker.
    pub plain: crate::raw::PlainTypes,
    /// What each function of C takes and gives, under its full name. Filled in by the checker.
    pub foreign: std::collections::HashMap<String, (Vec<Ty>, Ty)>,
    /// The variables of C declared in `extern` blocks (`extern { int opterr }`).
    pub cvars: Vec<CVarDecl>,
    /// The type of each variable of C, under its full name. Filled in by the checker.
    pub cvar_types: std::collections::HashMap<String, Ty>,
    /// Every `extern` block, with the module it is written in, also one that declares nothing:
    /// `extern "curl" { }` names a library that the C file of its module needs.
    pub blocks: Vec<(String, Foreign)>,
    /// What each `export` function takes and gives, under its full name. Filled in by the
    /// checker.
    pub exported: std::collections::BTreeMap<String, (Vec<Ty>, Ty)>,
    /// The full names of the functions of C declared with `...` after their parameters:
    /// each call gives what follows them as the types of its own arguments.
    pub variadic: std::collections::BTreeSet<String>,
    /// Built as a library C calls (`sg build --library` or `--shared`): no `main`, and at
    /// least one `export` function.
    pub library: bool,
    /// Built without an operating system (`sg build --freestanding`): no `main`, at least one
    /// `export` function, the program's `onPanic`, and nothing that needs a system.
    pub freestanding: bool,
    /// Compiled for a machine other than this one (`--target`): a `const assert` that measures a
    /// type is left to the compiler of C, which knows that machine's layout.
    pub other_machine: bool,
    /// The functions of the language's own module a program built without an operating system
    /// leaves out: those that need a system, which nothing it calls reaches. Filled in by the
    /// checker.
    pub left_out: std::collections::BTreeSet<String>,
    /// The folders `-L` names, where the libraries the `extern` blocks name are looked for
    /// first: absolute, in the order they were given.
    pub lib_dirs: Vec<std::path::PathBuf>,
    /// The folder of the program's first file, absolute: on Windows the interpreter looks for
    /// the DLLs of its libraries there, as a program built from it looks in its own folder.
    pub home: Option<std::path::PathBuf>,
    /// Every `const`, of a module and of a block, in the order of the text. One written in a
    /// block is kept here too, under a name of its own (see `ConstDecl::local`).
    pub consts: Vec<ConstDecl>,
    /// What each `const` is, under its full name, with its type: found by the checker when the
    /// program is compiled, and read by the interpreter and the back end.
    pub const_values: std::collections::BTreeMap<String, (Ty, ConstVal)>,
    /// The files `embed("...")` names, in the order they are written, and their bytes once they
    /// are read (before the checker runs).
    pub embeds: Vec<Embed>,
    /// The count each `[Name]` written in a type stands for, by where the name is written.
    /// Filled in by the checker.
    pub sizes: std::collections::BTreeMap<usize, usize>,
    /// Every `static`, of a module and of a block, in the order of the text. One written in a
    /// block is kept here too, under a name of its own, as a const is.
    pub statics: Vec<StaticDecl>,
    /// The size and the alignment of each struct and enum, at most what C gives them. Filled in
    /// by the checker.
    pub value_layouts: std::collections::HashMap<String, (u64, u64)>,
}

/// `const int BlockSize = 4096`: a value known when the program is compiled.
pub struct ConstDecl {
    pub public: bool,
    /// The full name of a const of a module (`shop.MaxItems`); for one written in a block, the
    /// name followed by `#` and where it is written (`Masks#812`), which no other name has.
    pub name: String,
    pub name_span: Span,
    /// As written; `int[]` is given the count of the items by the parser for `{ ... }`, and by
    /// the checker for `embed`.
    pub ty: TypeExpr,
    pub init: Expr,
    /// Written in a block: the name as written there.
    pub local: Option<String>,
    /// The consts of the blocks around it that it sees, as written and by their full names,
    /// the nearest last.
    pub visible: Vec<(String, String)>,
    /// `const assert(cond, "message")`: a `bool` const with no name of its own (`assert#812`),
    /// which must hold when the program is compiled.
    pub assert: Option<ConstAssert>,
}

/// What a `const assert` says when its condition does not hold, and where it is written.
#[derive(Clone)]
pub struct ConstAssert {
    pub message: Option<String>,
    pub span: Span,
}

/// `static Table Crc = makeCrcTable()`: one value for the whole program, which lives until it
/// ends and is never assigned. `static atomic long Hits = atomic 0` is changed through its atomic
/// methods, `static locked Config Settings = Config.defaults()` inside a `lock` block.
pub struct StaticDecl {
    pub public: bool,
    /// The full name, as for a const: `shop.Hits`, or `Hits#812` for one written in a block.
    pub name: String,
    pub name_span: Span,
    /// Written `static locked T`: reached only inside a `lock` block.
    pub locked: bool,
    /// The type of the value, without `locked`.
    pub ty: TypeExpr,
    /// The function that gives the value, `<name>#init`, in `Program::funcs`: its body returns
    /// what the static is given. Called the first time the static is used, unless the value
    /// is found when the program is compiled (see `Program::static_values`).
    pub init: String,
    /// Written in a block: the name as written there.
    pub local: Option<String>,
    /// The consts and statics of the blocks around it that its value sees, as written and by
    /// their full names, the nearest last.
    pub consts: Vec<(String, String)>,
    pub statics: Vec<(String, String)>,
    /// Set by the checker: the value is found when the program is compiled, from what the rule
    /// of a const allows, and lies in the program from its start.
    pub value: Option<ConstVal>,
}

/// The name of the function that gives the value of the static `name`.
pub fn static_init(name: &str) -> String {
    format!("{name}#init")
}

/// A value a `const` holds: numbers, `bool`, `char`, a text, and arrays, structs and enums of
/// them. Plain data that every thread may read.
#[derive(Clone, Debug, PartialEq)]
pub enum ConstVal {
    Int(IntTy, i128),
    /// A `float` as the `double` that holds it.
    Float(FloatTy, f64),
    Bool(bool),
    Char(char),
    Str(String),
    Array(Vec<ConstVal>),
    /// A `byte[N]`: the bytes as they are, so that a file of `embed` stays small.
    Bytes(std::sync::Arc<[u8]>),
    /// A struct and its fields, in the order they are declared.
    Struct(String, Vec<ConstVal>),
    /// An enum, the position of the variant and what it carries.
    Enum(String, usize, Vec<ConstVal>),
}

/// `embed("logo.gif")`: a file beside the source file that names it, as written, and its bytes.
pub struct Embed {
    pub written: String,
    pub span: Span,
    pub bytes: Option<std::sync::Arc<[u8]>>,
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
    /// The modules `uses` names, by their full names.
    pub uses: Vec<(String, Span)>,
    pub map: std::sync::Arc<ModuleMap>,
}

/// How the files of one package write the names of modules (choice 1555). A package's modules
/// are kept under the name the program gives it: the root files of the package `xml` are the
/// module `xml`, its folder `reader` is `xml.reader`. Inside the package `reader` is written, and
/// the packages it uses by the names its own `sg.project` gives them. The program's own files
/// write every name as it is kept.
#[derive(Clone, Default, Debug)]
pub struct ModuleMap {
    /// The module of the package's root files; empty for the program's own files.
    pub prefix: String,
    /// The packages the package (or the program) names, each by the name it writes and the
    /// module of that package's root files.
    pub packages: Vec<(String, String)>,
    /// The module of the root files of every package of the program.
    pub every_package: std::sync::Arc<Vec<String>>,
}

impl ModuleMap {
    /// The full name of the module written as `written` in a file of this package.
    pub fn full(&self, written: &str) -> String {
        let (first, rest) = written.split_once('.').map_or((written, None), |(first, rest)| (first, Some(rest)));
        if let Some((_, root)) = self.packages.iter().find(|(name, _)| name == first) {
            return match rest {
                Some(rest) => format!("{root}.{rest}"),
                None => root.clone(),
            };
        }
        if self.prefix.is_empty() || first == crate::source::STD_MODULE {
            return written.to_string();
        }
        format!("{}.{written}", self.prefix)
    }

    /// A name of a type or a function written with its module in front (`reader.Node`), with the
    /// module turned into its full name.
    pub fn full_name(&self, written: &str) -> String {
        match written.rsplit_once('.') {
            Some((module, name)) => format!("{}.{name}", self.full(module)),
            None => written.to_string(),
        }
    }

    /// The package a module is in: the module of its root files, or empty for the program's own.
    pub fn package_of<'a>(&self, module: &'a str) -> &'a str {
        self.every_package.iter().find(|root| module == root.as_str() || module.strip_prefix(root.as_str()).is_some_and(|rest| rest.starts_with('.'))).map_or("", |root| &module[..root.len()])
    }

    /// Whether a file of this package may name `module`: one of its own package, of a package it
    /// names, or the language's own.
    pub fn reaches(&self, module: &str) -> bool {
        let package = self.package_of(module);
        package == self.prefix || module == crate::source::STD_MODULE || self.packages.iter().any(|(_, root)| root == package)
    }

    /// How a file of this package writes the module kept as `full`: the other way of `full`.
    pub fn written(&self, full: &str) -> String {
        for (name, root) in &self.packages {
            if full == root {
                return name.clone();
            }
            if let Some(rest) = full.strip_prefix(root.as_str()).and_then(|rest| rest.strip_prefix('.')) {
                return format!("{name}.{rest}");
            }
        }
        match full.strip_prefix(self.prefix.as_str()).and_then(|rest| rest.strip_prefix('.')) {
            Some(rest) if !self.prefix.is_empty() => rest.to_string(),
            _ => full.to_string(),
        }
    }
}

/// Whether a file of `from` may name `module` by the rule of `internal` (choice 1561): a module
/// in a folder named `internal` is named only from inside the folder that holds `internal`.
/// Gives the folder it is kept in when it may not.
pub fn internal_to<'a>(from: &str, module: &'a str) -> Option<&'a str> {
    let mut at: usize = 0;
    for part in module.split('.') {
        if part == "internal" {
            let parent = &module[..at.saturating_sub(1)];
            let inside = parent.is_empty() || from == parent || from.strip_prefix(parent).is_some_and(|rest| rest.starts_with('.'));
            if !inside {
                return Some(parent);
            }
        }
        at += part.len() + 1;
    }
    None
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
    /// Declared without a body in an `extern` block: a type of C whose insides SG does not
    /// see, used only behind a raw pointer (`raw sqlite3`).
    pub opaque: bool,
    /// How C lays it out: `union`, `packed`, `align(N)`, and the bits left empty.
    pub layout: LayoutDecl,
}

/// What a struct says about how C lays it out, beyond the types of its fields.
#[derive(Default)]
pub struct LayoutDecl {
    /// Written `union Bits { ... }`: every field starts where the value starts.
    pub union: bool,
    /// Written `packed struct Header { ... }`: no padding, an alignment of 1.
    pub packed: Option<Span>,
    /// Written `align(64) struct Slot { ... }`: at least that alignment.
    pub align: Option<(u64, Span)>,
    /// `uint _ : 4`: bits C leaves empty, before the field at `before` (the count of fields
    /// for those after the last one).
    pub gaps: Vec<Gap>,
}

/// `uint _ : 4`, or `uint _ : 0`, which starts the next field at a new unit of its type.
pub struct Gap {
    pub before: usize,
    pub ty: TypeExpr,
    pub bits: u64,
    pub span: Span,
}

impl StructDecl {
    /// Written with `: Disposable`: its `dispose` runs when its last owner ends.
    pub fn disposable(&self) -> bool {
        self.interfaces.iter().any(|(name, _)| name == "Disposable")
    }
}

pub struct EnumDecl {
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub sources: Vec<(String, Span)>,
    pub interfaces: Vec<(String, Span)>,
    /// `enum Block : byte`: the integer type of the numbers its variants have.
    pub numbers: Option<(IntTy, Span)>,
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
    /// `Image = 0x2C` in an enum with numbers: the const that holds the number, by its full
    /// name (`Image#812`). A variant without one is one more than the variant above it.
    pub number: Option<String>,
}

pub struct FieldDecl {
    pub public: bool,
    /// `readonly`: given when the value is built, never changed afterwards.
    pub readonly: bool,
    pub ty: TypeExpr,
    pub name: String,
    pub span: Span,
    /// `uint version : 4`: a field of that many bits, as C has it.
    pub bits: Option<(u64, Span)>,
    /// `int port = 80`: what a struct literal that leaves the field out gives it.
    pub default: Option<FieldDefault>,
}

/// The default value of a field or of a parameter.
#[derive(Clone)]
pub enum FieldDefault {
    /// A value known when the program is compiled, held by a const the parser makes for it, by
    /// its full name (`port#812`).
    Const(String),
    /// `null`, an empty collection (`List<int>()`), which allocates nothing, or a literal that
    /// gives no field (`Options {}`), made of the default values of the fields: put into the
    /// literal or the call as if it were written there.
    Expr(Expr),
}

#[derive(Clone)]
pub struct Func {
    pub public: bool,
    /// Written with `async`: a call gives a `Task` of what `ret` names, and the body can
    /// `await`.
    pub is_async: bool,
    /// Written with `export`: C calls it by its name as written (`export int compare(raw void
    /// a, raw void b)`), and its name is a `CFunc`.
    pub export: bool,
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
    /// Declared in an `extern` block: a function of C, with no body.
    pub foreign: Option<Foreign>,
    /// Declared with `...` after its parameters: a function of C that takes more arguments,
    /// of the types each call gives (`int snprintf(raw byte s, nuint n, read CString f, ...)`).
    pub variadic: bool,
}

/// Where a C function comes from: `extern "z" { ... }` names the library `z` (linked as
/// `-lz`); `extern { ... }` is the C library itself; `extern "./native.c" { ... }` is a C file
/// of the module's folder, compiled with the program.
#[derive(Clone)]
pub struct Foreign {
    /// The names of the library, as the systems call it (`extern "z", "zlib"`): the first one
    /// the C compiler finds is linked. Empty for a C file or for C's own library.
    pub libs: Vec<String>,
    /// `./native.c` as written: a C file beside the program, in place of a library.
    pub file: Option<String>,
    /// Where that file lies, found once the program is read. Set before the checker runs.
    pub path: Option<String>,
    /// The word `extern` and the name of the library after it.
    pub span: Span,
}

/// `int opterr` in an `extern` block: a variable of C, read and written inside `unsafe`.
pub struct CVarDecl {
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub ty: TypeExpr,
    pub foreign: Foreign,
}

/// One name after `from`: a parameter (`input`), the source of the type a method belongs to
/// (`source`), or the source of a parameter's type (`lx.source`).
#[derive(Clone)]
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
    /// `List<T>.repeat(item, count)`: `count` copies of a `Copyable` item, in one block (1648).
    ListRepeat,
    /// `List<T>.withCapacity(n)`: an empty list with room for `n` items, taken at once (1652).
    ListWithCapacity,
    /// `string.fromCString(p)`: copies the text C ends with a zero byte, checked to be UTF-8.
    FromCString,
    /// `CString.from(text)`: the text with a zero byte after it, for C.
    ToCString,
    /// A function declared in an `extern` block: a function of C.
    Extern,
    /// A function of C declared with `...`: what this call gives it, the parameters it
    /// declares and then the types of the arguments after them.
    Variadic(Vec<Ty>),
    /// `buffer.pointer()`, written only as an argument of a C function: the address of the
    /// items of a list, an array or a slice.
    Pointer,
    /// `int.parse(text)`, `double.parse(text)`, `int.parseHex(text)`: the type is the one in
    /// the `Result` it gives; the number is the base the digits are read in (10 or 16).
    Parse(u32),
    /// `ulong.fromLittleEndian(bytes)`, `ushort.fromBigEndian(bytes)`: the integer that exactly
    /// as many bytes as the type takes write, the first byte the lowest (or the highest).
    FromBytes { ty: IntTy, big: bool },
    /// `Header.fromBytes(bytes)`: a struct or a union of plain data that every pattern of bits
    /// is a value of, out of exactly as many bytes as it takes, as they lie in memory.
    StructFromBytes(String),
    /// `Block.fromValue(n)`: the variant of an enum with numbers that has the number, or `null`.
    FromValue(String),
    /// `File.readText(path)`, `File.openText(path)` and the other functions of `File`.
    Io(IoCall),
    /// `File.readTextAsync(path)` and the other async functions of `File`: a task that gives
    /// what the function without `Async` gives. A token may come last.
    IoAsync(IoCall),
    /// `reader.readLine(ref line)`, `writer.write(x)` and the other methods of a
    /// `StreamReader` or a `StreamWriter`, `Stdin` and `Stdout` among them.
    Stream(StreamCall),
    /// `reader.readLineAsync(ref line)` and the other async methods of a `StreamReader`. A
    /// token may come last.
    StreamAsync(StreamCall),
    /// What makes, cancels and looks at a `CancellationSource` or a `CancellationToken`.
    Cancel(CancelCall),
    /// The methods of a `Channel<T>` and a `Sender<T>`: `channel.sender()`, `.clone()`,
    /// `sender.send(x)` and `channel.receive()`. With `true`, a token was given last.
    Chan(ChanCall, bool),
    /// `sender.sendAsync(x)` and `channel.receiveAsync()`, awaited where they are written. With
    /// `true`, a token was given last.
    ChanAsync(ChanCall, bool),
    /// `TaskGroup()`, `TaskGroup(limit)` and the methods of a group of tasks. With `true`, a
    /// token was given last (`whenRoom`, `whenAll`).
    Group(GroupCall, bool),
    /// `Memory.free(p)`, `Memory.copy(source, target, count)`.
    Memory(MemoryCall),
    /// `TcpListener.bind(address)`, `listener.accept()`, `TcpStream.connect(address)`,
    /// `stream.receive(ref buffer)` and the other functions of the network.
    Net(NetCall),
    /// `listener.acceptAsync()`, `TcpStream.connectAsync(address)`, `stream.receiveAsync(ref
    /// buffer)`, `stream.sendAsync(bytes)`: the same work as a task. A token may come last.
    NetAsync(NetCall),
    /// A call through a function value kept in a variable: `test(order)`.
    Value,
    /// A call through a function value kept in a field: `this.test(order)`; the field's position.
    FieldValue(usize),
    /// A call through a `CFunc` kept in a variable: a call of C, with what the function takes.
    CValue(Vec<Ty>),
    /// A call through a `CFunc` kept in a field: the field's position and what it takes.
    CFieldValue(usize, Vec<Ty>),
    /// `Task.run(work, x)`: starts the function given first with the arguments after it.
    TaskRun,
    /// `t.wait()`: waits for the task and gives what its function returned; takes the task.
    Wait,
    /// `Task.whenAll(tasks)`: takes a `List<Task<R>>` and gives a task of what each gave, in
    /// the order of the list. Awaited or waited for where it is written.
    WhenAll,
    /// `Task.whenAny(ref tasks)`: gives what the first task of the list to end gave, and
    /// takes that task out of the list. Awaited or waited for where it is written.
    WhenAny,
    /// `OperatingSystem.isWindows()` and its two siblings: known when the program is built.
    Os(Os),
    /// `Task.delay(ms)`: a task that ends when that many milliseconds have gone by. With a
    /// token (`Task.delay(ms, read token)`, the `bool`), it ends early when the token is
    /// cancelled, and gives a `Result<void, Cancelled>`.
    Delay(bool),
    /// `Clock.monotonic()`, `Clock.wall()` and `Clock.offsetAt(seconds)`: the readings of the
    /// clocks and of the time zone of the system. Only the language's own module calls them.
    Clock(ClockCall),
    /// `Entropy.fill(ref bytes)`: random bytes of the system into a `ref byte[]`. Only the
    /// language's own module calls it.
    Entropy,
    /// The calls of the file system that `File` does not have a word for, and that only the
    /// language's own module makes (`System.list`, `System.status`), with `File.delete`,
    /// `File.move`, `File.replace` and `File.copy`.
    System(SystemCall),
    /// `count.fetchAdd(1)`, `Atomic.fence(MemoryOrder.Release)`: an atomic operation, with the
    /// order it keeps and, for `compareExchange`, the order it keeps when it fails. The orders
    /// written are taken out of the arguments.
    Atomic { op: AtomicOp, order: Order, failure: Order },
    /// `slot.take()`: gives the value and leaves `null`.
    Take,
    /// `slot.replace(value)`: gives the old value and leaves the new one.
    Replace,
}

/// The functions of `Memory` that are called on raw memory; `Memory.allocate<T>(n)` is an
/// `Allocate`, since it names a type.
/// An operating system `OperatingSystem` asks about.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Os {
    Windows,
    Linux,
    MacOS,
}

impl Os {
    /// Whether the interpreter, and the program the back end builds, run on it: they run where
    /// they are built, since nothing is built for another system yet.
    pub fn here(self) -> bool {
        match self {
            Os::Windows => cfg!(windows),
            Os::Linux => cfg!(target_os = "linux"),
            Os::MacOS => cfg!(target_os = "macos"),
        }
    }
}

/// The calls of `Clock`, which only the language's own module makes.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ClockCall {
    /// `Clock.monotonic()`: the nanoseconds of the monotonic clock, as a `long`.
    Monotonic,
    /// `Clock.wall()`: the nanoseconds of the clock of the system since 1970, as a `long`.
    Wall,
    /// `Clock.wallSeconds()`: the whole seconds of the clock of the system since 1970, as a
    /// `long`, read where the system keeps them cheaply (a `Date` of HTTP needs no more); it may be
    /// a few milliseconds behind `wall()`.
    WallSeconds,
    /// `Clock.offsetAt(seconds)`: how many seconds the time zone of the system is ahead of UTC
    /// at that many seconds since 1970, as an `int`; 0 when the system does not say.
    OffsetAt,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MemoryCall {
    /// `Memory.free(p)`: gives back what `Memory.allocate` gave; `null` is let be.
    Free,
    /// `Memory.copy(source, target, count)`: `count` items, also when the two overlap.
    Copy,
    /// `C.errno()`: what the last call of C on this thread left in `errno`.
    Errno,
    /// `C.setErrno(n)`: what the next call of C finds in `errno`.
    SetErrno,
    /// `C.lastError()`: the system's last error on this thread: `errno` on Linux and macOS,
    /// `GetLastError()` on Windows.
    LastError,
    /// `C.setLastError(n)`: what the next call finds as the system's last error.
    SetLastError,
    /// `IoError.fromCode(n)`: the `IoError` for a code of the system, as `C.lastError()` gives.
    IoErrorOf,
    /// `IoError.fromErrno(n)`: the `IoError` for a number of C's library, as `C.errno()` gives.
    /// The same as `fromCode` on Linux and macOS; on Windows the two numbers differ.
    IoErrorOfErrno,
    /// `Memory.addressOf(ref x)`: the address of a value, for C to give back to a callback.
    AddressOf,
    /// `Memory.view(p, count)`: a `read T[]` that looks at `count` items from `p`.
    View,
    /// `Memory.viewRef(p, count)`: a `ref T[]` that writes the `count` items from `p`.
    ViewRef,
    /// `Memory.readUnaligned(p)`: the item at `p`, which may lie at any address.
    ReadUnaligned,
    /// `Memory.writeUnaligned(p, value)`: writes the item at `p`, which may lie at any address.
    WriteUnaligned,
    /// `Memory.readVolatile(p[i].status)`: the place in raw memory, read once, as written,
    /// never left out or merged with another read.
    ReadVolatile,
    /// `Memory.writeVolatile(p[i].status, value)`: the place in raw memory, written once, as
    /// written.
    WriteVolatile,
    /// `Memory.zeroSecurely(ref key)`: zeros over the items, which the C compiler keeps even
    /// when nothing reads them afterwards.
    ZeroSecurely,
}

/// The order an atomic operation keeps with the reads and writes around it, as C11 names them:
/// `MemoryOrder` of the language's own module.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Order {
    Relaxed,
    Acquire,
    Release,
    AcquireRelease,
    SequentiallyConsistent,
}

impl Order {
    /// In the order of the variants of `MemoryOrder`.
    pub const ALL: [Order; 5] = [Order::Relaxed, Order::Acquire, Order::Release, Order::AcquireRelease, Order::SequentiallyConsistent];

    pub fn name(self) -> &'static str {
        match self {
            Order::Relaxed => "Relaxed",
            Order::Acquire => "Acquire",
            Order::Release => "Release",
            Order::AcquireRelease => "AcquireRelease",
            Order::SequentiallyConsistent => "SequentiallyConsistent",
        }
    }

    /// The constant of GCC's and Clang's `__atomic` builtins.
    pub fn c_name(self) -> &'static str {
        match self {
            Order::Relaxed => "__ATOMIC_RELAXED",
            Order::Acquire => "__ATOMIC_ACQUIRE",
            Order::Release => "__ATOMIC_RELEASE",
            Order::AcquireRelease => "__ATOMIC_ACQ_REL",
            Order::SequentiallyConsistent => "__ATOMIC_SEQ_CST",
        }
    }
}

/// What an atomic value is asked to do, and the functions of `Atomic`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AtomicOp {
    Load,
    Store,
    Exchange,
    /// `compareExchange`, or with `true` `compareExchangeWeak`, which may fail when the value
    /// was the one expected.
    CompareExchange(bool),
    /// `fetchAdd`, or with `true` `wrappingFetchAdd`.
    FetchAdd(bool),
    /// `fetchSub`, or with `true` `wrappingFetchSub`.
    FetchSub(bool),
    FetchAnd,
    FetchOr,
    FetchXor,
    /// `Atomic.fence(order)`
    Fence,
    /// `Atomic.compilerFence(order)`: an order for the compiler alone, as C's
    /// `atomic_signal_fence`.
    CompilerFence,
    /// `Atomic.spinLoop()`: the processor is told the thread spins waiting.
    SpinLoop,
}

impl AtomicOp {
    pub fn name(self) -> &'static str {
        match self {
            AtomicOp::Load => "load",
            AtomicOp::Store => "store",
            AtomicOp::Exchange => "exchange",
            AtomicOp::CompareExchange(false) => "compareExchange",
            AtomicOp::CompareExchange(true) => "compareExchangeWeak",
            AtomicOp::FetchAdd(false) => "fetchAdd",
            AtomicOp::FetchAdd(true) => "wrappingFetchAdd",
            AtomicOp::FetchSub(false) => "fetchSub",
            AtomicOp::FetchSub(true) => "wrappingFetchSub",
            AtomicOp::FetchAnd => "fetchAnd",
            AtomicOp::FetchOr => "fetchOr",
            AtomicOp::FetchXor => "fetchXor",
            AtomicOp::Fence => "fence",
            AtomicOp::CompilerFence => "compilerFence",
            AtomicOp::SpinLoop => "spinLoop",
        }
    }
}

/// The functions of a `CancellationSource` and a `CancellationToken`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CancelCall {
    /// `CancellationSource()`
    New,
    /// `CancellationSource.after(ms)`: a source that cancels itself when that many
    /// milliseconds have gone by.
    After,
    /// `CancellationSource.linked(read token)`: a source that is cancelled when the token's
    /// source is, as well as by itself; cancelling it leaves the token's source as it is (1775).
    Linked,
    /// `source.cancel()`
    Cancel,
    /// `source.cancelAfter(ms)`: the source cancels itself when that many milliseconds have
    /// gone by from now, in place of any time set before (C#'s `CancelAfter`); nothing once it
    /// is cancelled.
    CancelAfter,
    /// `source.token()`: one more token of the source.
    Token,
    /// `source.isCancelled()`, `token.isCancelled()`
    IsCancelled,
    /// `token.canBeCancelled()` (1785): whether the token has a source, so is not
    /// `CancellationToken.None`.
    CanBeCancelled,
    /// `token.check()`: `Error(Cancelled { })` once the token is cancelled, `Ok()` before.
    Check,
    /// `token.clone()`: one more token of the same source.
    Clone,
}

/// What is done with a `TaskGroup`: tasks that give nothing, each let go of as soon as it ends,
/// and waited for together (choices 1716 to 1719).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GroupCall {
    /// `TaskGroup()`, `TaskGroup(limit)` (the `bool`: whether a limit is given).
    New(bool),
    /// `group.add(task)`: the task is the group's from now on; it neither waits nor refuses.
    Add,
    /// `group.count()`: how many of its tasks have not ended.
    Count,
    /// `group.whenRoom()`: a task that ends once fewer tasks than the limit run.
    WhenRoom,
    /// `group.whenAll()`: takes the group, and ends once every task in it has ended. With a
    /// token, a cancel lets the tasks go (choice 1691's mark) and still waits for them.
    WhenAll,
}

/// What is done with the ends of a channel.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ChanCall {
    /// `channel.sender()`: one more end that sends into the channel.
    Sender,
    /// `channel.clone()`, `sender.clone()`: one more end of the same kind.
    Clone,
    /// `sender.send(x)`: the item goes into the channel, waiting while it is full; `false` when
    /// no end receives any more, and the item is dropped.
    Send,
    /// `channel.receive()`: the first item, waiting while there is none; `null` once every end
    /// that sends has ended and nothing is left.
    Receive,
}

/// The functions of the network: a `TcpListener` and a `TcpStream`. Each that can fail gives a
/// `Result` whose error is `IoError`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum NetCall {
    /// `TcpListener.bind(address)`: a listener on `host:port`, without a name lookup.
    Bind,
    /// `listener.port()`: the port it listens on, the one the system chose for port 0.
    Port,
    /// `listener.accept()`: the next connection that comes, as a `TcpStream`.
    Accept,
    /// `TcpStream.connect(address)`: a connection to `host:port`, the host looked up by name.
    Connect,
    /// `stream.receive(ref buffer)`: what has come, as many bytes as the buffer holds at
    /// most; gives how many, `0` at the end.
    Receive,
    /// `stream.send(bytes)`: all the bytes go out.
    Send,
    /// `stream.sendPieces(read pieces)`: the bytes of every window of a `List<read byte[]>`
    /// go out, in order, with as few calls of the system as it takes (`writev`).
    SendPieces,
    /// `stream.reader()`, `stream.reader(limit)`: a `StreamReader` that reads the connection
    /// through a block of its own and borrows it; a line or a text longer than the limit
    /// (64 KB unless given) is an error.
    Reader,
}

impl NetCall {
    /// The name as it is written after the dot.
    pub fn name(self) -> &'static str {
        match self {
            NetCall::Bind => "bind",
            NetCall::Port => "port",
            NetCall::Accept => "accept",
            NetCall::Connect => "connect",
            NetCall::Receive => "read",
            NetCall::Send => "write",
            NetCall::SendPieces => "writev",
            NetCall::Reader => "reader",
        }
    }
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
    /// `File.openWrite(path)`: a `StreamWriter` from the start of the file, which keeps what it
    /// holds; a missing file is made.
    OpenWrite,
}

/// The calls of the file system and of the environment beyond reading and writing a file. Those
/// of `File` are called by any program; the others only by the language's own module, which
/// builds `Directory`, `FileInfo` and `Environment` on them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SystemCall {
    /// `File.delete(path)`: `unlink`.
    Delete,
    /// `File.move(from, to)`: a rename that fails when the target is there.
    Move,
    /// `File.replace(from, to)`: a rename that takes the place of the target.
    Replace,
    /// `File.copy(from, to)`: what the file holds and its permissions, into a new file.
    Copy,
    /// `System.createDirectory(path)`: `mkdir`.
    CreateDirectory,
    /// `System.removeDirectory(path)`: `rmdir`, of an empty folder.
    RemoveDirectory,
    /// `System.removeAll(path)`: a folder with all it holds, links not followed.
    RemoveAll,
    /// `System.list(path, ref names, ref kinds)`: the names of a folder in the order of their
    /// bytes, and the kind of each.
    List,
    /// `System.status(path, ref fields)`: the kind, the size and the time of the last write.
    Status,
    /// `System.current()`: the folder the program works in.
    Current,
    /// `System.makeTemp(template)`: `mkdtemp`.
    MakeTemp,
    /// `System.makeTempFile(template)`: `mkstemp`, a new empty file only the user may read and
    /// write, closed again.
    MakeTempFile,
    /// `System.tempFolder()`: the folder for files that do not have to stay, as Windows gives it.
    TempFolder,
    /// `System.variable(name, ref bytes)`: a variable of the environment, `false` when it is not set.
    Variable,
    /// `System.colourTerminal()`: the standard error is a terminal that shows colours (on Windows,
    /// a console that takes the escape sequences, turned on if it was not), for `Log` (choice 1455).
    ColourTerminal,
    /// `System.charCategory(c)`: the general category of a character, its place among the
    /// variants of `UnicodeCategory` (choice 1476), from the Unicode tables.
    CharCategory,
    /// `System.charFold(c)`: the simple case folding of a character (the C and S lines of
    /// CaseFolding.txt), by which `(?i)` of a `Regex` compares (choice 1477).
    CharFold,
    /// `System.normalize(text, compose, compat)`: the text in a normal form of Unicode (choice
    /// 1599): C with `compose`, D without, KC and KD with `compat`.
    Normalize,
    /// `System.isNormalized(text, compose, compat)`: whether the text is in that form.
    IsNormalized,
    /// `System.crc32Hardware(state, read bytes)`: the register of a CRC-32 after the bytes, worked
    /// out by the processor's own instructions (choice 1656), or -1 where there are none to use
    /// (and always when interpreted): `std` then works it out itself.
    Crc32Hardware,
    /// `System.sha256Hardware(ref state, read blocks)`: whole blocks of 64 bytes taken into the
    /// eight words of a SHA-256 by the processor's SHA instructions (choice 1657); `false`, with
    /// nothing done, where there are none (and always when interpreted).
    Sha256Hardware,
    /// `System.executable(path)`: 1 for a file this program may run, 2 for a file it may not, 0
    /// for no file.
    Executable,
    /// `System.run(path, read words, read folder, clear, read names, read values, read kept,
    /// input, read text, capture, ref output, ref ended)`: starts a program and waits for it.
    /// What it wrote is kept in `output` when it is UTF-8; `ended` gets how it ended and where
    /// the first byte that is not UTF-8 is, or -1.
    Run,
    /// `System.start(path, read words, read folder, clear, read names, read values, read kept,
    /// input, read text, output, error, ref made)`: starts a program and goes on. Its input is
    /// this program's (`input` 0), nothing (1), `text` written by a thread (2) or a pipe (3); its
    /// output and its standard error are pipes when asked for. `made` gets the number of the
    /// program, then the ends of the pipes kept here, or -1.
    Start,
    /// `System.wait(pid, ref ended)`: waits for the program to end and takes it away; `ended` gets
    /// 0 and the code, or 1 and the signal that stopped it.
    Wait,
    /// `System.waitAsync(pid, read token, ref ended)`: the same without holding the thread. A
    /// cancel ends the wait and leaves the program to be stopped.
    WaitAsync,
    /// `System.signal(pid, number)`: `kill`. The program is not taken away before it is waited
    /// for, so the number names no other program.
    Signal,
    /// `System.signalListen(mask)`: a listener of the signals whose kinds are the bits of `mask`;
    /// gives its number. A kind is the place of a `Signal` among its variants.
    SignalListen,
    /// `System.signalNext(listener)`: waits for the next signal of the listener; gives its kind.
    SignalNext,
    /// `System.signalNextAsync(listener, read token, ref kind)`: the same without holding the
    /// thread. A cancel ends the wait; the signal that comes later stays for the next call.
    SignalNextAsync,
    /// `System.signalStop(listener)`: the listener ends; a kind no listener is left for does what
    /// it did before the first.
    SignalStop,
    /// `System.signalRaise(kind)`: the signal to this program, as C's `raise`.
    SignalRaise,
    /// `System.signalNumber(kind)`: the number of the signal on this system.
    SignalNumber,
    /// `System.reader(end)`: the reader of the end of a pipe made by `start`, which it owns.
    PipeReader,
    /// `System.writer(end)`: the writer of the end of a pipe made by `start`, which it owns.
    PipeWriter,
    /// `System.udpBind(v6, high, low, port)`: a datagram socket bound to the address; gives the
    /// socket of the system. An address goes to the system as four values: whether it is v6, the
    /// high and the low 64 bits of the host (a v4 host is the last 32), and the port.
    UdpBind,
    /// `System.udpConnect(fd, v6, high, low, port)`: the address `send` sends to and `receive`
    /// receives from.
    UdpConnect,
    /// `System.udpSend(fd, read bytes, to, v6, high, low, port)`: one datagram, to the address
    /// when `to`, and to that of `connect` otherwise. One the system cannot send whole is
    /// `MessageTooLong`.
    UdpSend,
    /// `System.udpSendAsync(..., read token)`: the same without holding the thread.
    UdpSendAsync,
    /// `System.udpReceive(fd, ref buffer, ref bits)`: one datagram into the buffer; `bits` gets
    /// how many bytes, then the address it came from (v6, high, low, port). One longer than the
    /// buffer is `MessageTooLong`.
    UdpReceive,
    /// `System.udpReceiveAsync(..., read token)`: the same without holding the thread.
    UdpReceiveAsync,
    /// `System.socketAddress(fd, remote, ref bits)`: the address the socket is bound to, or the one
    /// it is connected to; `false` when it has none.
    SocketName,
    /// `System.streamAddress(read stream, remote, ref bits)`: the same for a `TcpStream`.
    StreamName,
    /// `System.closeSocket(fd)`: `close`.
    CloseSocket,
    /// `System.lookup(host, ref bits)`: the addresses of a host name, three numbers each (v6,
    /// high, low).
    Lookup,
    /// `System.lookupAsync(host, ref bits, read token)`: the same on a thread of its own, which a
    /// cancel leaves.
    LookupAsync,
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
            IoCall::OpenWrite => "openWrite",
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
    /// `reader.readSome(ref buffer)`: what the block holds, or what one read of the system
    /// gives when it is empty, at most what the buffer holds; gives how many, `0` at the end.
    ReadSome,
    /// `reader.fill()`: one more read of the system behind what the block holds; `false` at
    /// the end, `TooLong` when the block holds its limit already.
    Fill,
    /// `reader.buffered()`: what the block holds and has not been given out, lent as a
    /// `read byte[]`.
    Buffered,
    /// `reader.consume(n)`: the first `n` bytes the block holds are given out.
    Consume,
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
    /// `reader.seek(from)`, `writer.seek(from)`: where the next read or write starts, given by a
    /// `SeekFrom`; gives the new place, counted from the start of the file.
    Seek,
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
    /// `atomic` in front of the name, with how many `raw` are written before it: `atomic int`
    /// and `shared atomic int` are 0, `raw atomic int` is 1. `atomic raw Node` is 0 with a
    /// `raw` after it: an atomic pointer.
    pub atomic: Option<u8>,
    /// How many times `raw` is written in front of the name: `raw byte` is 1, `raw raw byte`
    /// 2. A raw pointer goes around the type before `?` does, as a holder does.
    pub raw: u8,
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

#[derive(Clone, PartialEq)]
pub enum Suffix {
    /// `*`
    Pointer,
    /// `?`
    Nullable,
    /// `[8]`: a fixed number of items.
    Array(usize),
    /// `[BlockSize]`: as many items as an integer `const` says, by the name written there and
    /// where it is written.
    Named(String, Span),
    /// `[]`: a slice, behind `read` or `ref`.
    Slice,
}

#[derive(Clone)]
pub struct Param {
    pub ty: TypeExpr,
    pub name: String,
    pub span: Span,
    /// `int port = 80`: what a call that leaves the argument out gives (choices 1620-1622).
    pub default: Option<FieldDefault>,
}

pub type Block = Vec<Stmt>;

#[derive(Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone)]
pub enum StmtKind {
    /// `var x = e` or `int x = e`
    Let { ty: Option<TypeExpr>, name: String, name_span: Span, init: Expr },
    /// `x = e`, `a.b += e`. `through`: the target is a `ref` variable and the write goes to
    /// what it borrows (set by the checker).
    Assign { target: Expr, op: Option<BinOp>, value: Expr, through: bool },
    /// `after`: the names of an `is` in the condition that the code after the statement sees
    /// (`if r is not Ok(value) { return }`), set by the checker.
    If { cond: Expr, then: Block, els: Option<Block>, after: Vec<String> },
    While { cond: Expr, body: Block, after: Vec<String>, label: Option<Label> },
    /// `for i in a..b`; `reversed`: `for i in (a..b).reversed()`, from `b - 1` down to `a`.
    For { var: String, var_span: Span, from: Expr, to: Expr, body: Block, reversed: bool, label: Option<Label> },
    /// `for x in xs`, `for x in read xs`, `for x in ref xs`. `by`: the items are lent to the
    /// loop, not handed over (set by the checker).
    ForEach { var: String, var_span: Span, iter: Expr, body: Block, by: Option<Mode>, label: Option<Label> },
    /// `read folder as f { ... }`, `ref folder as f { ... }`: what a `shared` value holds,
    /// lent under a name for the block. `value` is the `shared` value (opened by the checker).
    As { mutable: bool, value: Expr, name: String, name_span: Span, body: Block },
    /// `lock total as c { ... }`, `lock a as x, b as y { ... }`: what `shared locked` values
    /// hold, each lent to change under a name for the block, while no other task reaches it.
    /// `lock jobs as q when q.length() > 0 { ... }`: the block waits, with the locks let go,
    /// until `when` holds, which is computed under the locks with each name lent to read; a
    /// `lock` block on one of the values that ends wakes it to look again. `awaited`: written
    /// `await lock`, in an async function, where the task waits and its thread goes on.
    Lock { items: Vec<LockItem>, when: Option<Box<Expr>>, awaited: bool, body: Block },
    Return(Option<Expr>),
    /// `break`, or `break outer`: leaves the loop named `outer` from a loop inside it.
    Break(Option<Label>),
    /// `continue`, or `continue outer`: the next round of the loop named `outer`.
    Continue(Option<Label>),
    Expr(Expr),
    /// `_ = e`: runs `e` and drops its value on purpose.
    Discard(Expr),
    /// `unsafe { ... }`: statements that may do what the checker cannot see is safe, such as
    /// reading and writing raw memory.
    Unsafe(Block),
    /// `const int Size = 64` in a block: the name as written and the full name of the const,
    /// which `Program::consts` holds. Seen from here to the end of the block.
    Const { name: String, name_span: Span, full: String },
    /// `static int Hits = ...` in a block: the name as written and the full name of the static,
    /// which `Program::statics` holds. Seen from here to the end of the block.
    Static { name: String, name_span: Span, full: String },
}

/// The name of a loop (`outer: for ...`), or the loop a `break` or `continue` names, and where
/// it is written.
#[derive(Clone)]
pub struct Label {
    pub name: String,
    pub span: Span,
}

/// One value of a `lock` block and the name of what it holds.
#[derive(Clone)]
pub struct LockItem {
    pub value: Expr,
    pub name: String,
    pub name_span: Span,
}

#[derive(Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    /// The type of the value, written by the checker; `void` until then. The back end reads it.
    pub ty: Ty,
    /// Written by the checker: this use moves the value out of its place. The back end
    /// empties the place, so that what is dropped later is dropped once.
    pub moved: bool,
}

#[derive(Clone)]
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
    /// `p with { x = 3 }`: the value of `base`, a struct, with the fields written given anew.
    With { base: Box<Expr>, fields: Vec<FieldInit> },
    /// `new e`: puts the value on the heap, gives `T*`.
    New(Box<Expr>),
    /// `new shared e`: puts the value on the heap with a count of its owners, gives `shared T`.
    /// `new shared checked e` gives `shared checked T`, `new shared locked e` gives
    /// `shared locked T`.
    NewShared { value: Box<Expr>, checked: bool, locked: bool },
    /// `atomic e`: the value as an `atomic T`.
    Atomic(Box<Expr>),
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
    /// `unsafe e`: a value whose computation may do what the checker cannot see is safe, such
    /// as reading raw memory. A prefix, like `try` and `await`.
    Unsafe(Box<Expr>),
    /// `Memory.allocate<T>(n)`: room for `n` items of `T`, all zero, as a `raw T`. `args` holds
    /// what was written between the parentheses.
    Allocate { item: TypeExpr, args: Vec<Expr> },
    /// `Memory.sizeOf<T>()`, or with `align` `Memory.alignOf<T>()`: a `nint` known when the
    /// program is compiled. The checker fills in `ty` with the type it names.
    SizeOf { item: TypeExpr, align: bool, of: Ty },
    /// `(raw byte)p`: a raw pointer read as a pointer to another type.
    RawCast { ty: TypeExpr, expr: Box<Expr> },
    /// The address of a variable of C, by its full name, as a `raw T`: the checker turns a use
    /// of the variable `opterr` into `CVar("opterr")[0]`, an item of raw memory.
    CVar(String),
    /// `await e`: waits, in an async function, for the task `e` and gives what it returns.
    /// `await f(x)` with `f` async runs `f` as part of this task, without a task of its own.
    Await(Box<Expr>),
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

    /// Built by the checker from the name of a `const` (not one of text, which becomes the text
    /// itself): its full name. A value of its type, which no one changes, there for the whole run.
    Const(String),
    /// Built by the checker from the name of a `static`: its full name. A `read` borrow of the
    /// one value, or the value itself for a number, a `bool` and a `char`; for `static locked`,
    /// what a `lock` block takes.
    Static(String),
    /// `embed("logo.gif")`, by its place in `Program::embeds`: the bytes of the file, as a
    /// `byte[N]`. Written only where a `const` is given its value.
    Embed(usize),

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
    /// A `raw T`: the item at that many items after the address, in raw memory.
    Raw,
}

/// One item between the braces of a collection: `1`, or `{"x", 10}` for a key and a value.
#[derive(Clone)]
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
/// The first character of the name a `match` read from `a?.b` binds: no program can write
/// it, and it tells that `match` from one that is written.
pub const OR_NULL: char = '?';

#[derive(Clone)]
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

#[derive(Clone)]
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
#[derive(Clone)]
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

    /// Whether the place lies in raw memory: `p[i]`, `p[i].x`, `p[i].values[2]`. It is not
    /// lent, and reading it copies what lies there. Known once the checker has set the kinds.
    pub fn in_raw_memory(&self) -> bool {
        match &self.kind {
            ExprKind::Index { kind: IndexKind::Raw, .. } => true,
            ExprKind::Field { recv, .. } | ExprKind::Index { recv, kind: IndexKind::Array, .. } => recv.in_raw_memory(),
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
