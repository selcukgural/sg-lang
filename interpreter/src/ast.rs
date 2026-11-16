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
    pub plain: std::collections::HashMap<String, Vec<Ty>>,
    /// What each function of C takes and gives, under its full name. Filled in by the checker.
    pub foreign: std::collections::HashMap<String, (Vec<Ty>, Ty)>,
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
    /// Declared without a body in an `extern` block: a type of C whose insides SG does not
    /// see, used only behind a raw pointer (`raw sqlite3`).
    pub opaque: bool,
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
    /// Written with `async`: a call gives a `Task` of what `ret` names, and the body can
    /// `await`.
    pub is_async: bool,
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
}

/// Where a C function comes from: `extern "z" { ... }` names the library `z` (linked as
/// `-lz`); `extern { ... }` is the C library itself.
#[derive(Clone)]
pub struct Foreign {
    pub lib: Option<String>,
    /// The word `extern` and the name of the library after it.
    pub span: Span,
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
    /// `string.fromCString(p)`: copies the text C ends with a zero byte, checked to be UTF-8.
    FromCString,
    /// `CString.from(text)`: the text with a zero byte after it, for C.
    ToCString,
    /// A function declared in an `extern` block: a function of C.
    Extern,
    /// `buffer.pointer()`, written only as an argument of a C function: the address of the
    /// items of a list, an array or a slice.
    Pointer,
    /// `int.parse(text)`, `double.parse(text)`, `int.parseHex(text)`: the type is the one in
    /// the `Result` it gives; the number is the base the digits are read in (10 or 16).
    Parse(u32),
    /// `ulong.fromLittleEndian(bytes)`, `ushort.fromBigEndian(bytes)`: the integer that exactly
    /// as many bytes as the type takes write, the first byte the lowest (or the highest).
    FromBytes { ty: IntTy, big: bool },
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
    /// `Task.delay(ms)`: a task that ends when that many milliseconds have gone by. With a
    /// token (`Task.delay(ms, read token)`, the `bool`), it ends early when the token is
    /// cancelled, and gives a `Result<void, Cancelled>`.
    Delay(bool),
    /// `Clock.monotonic()` (`false`) and `Clock.wall()` (`true`): the nanoseconds of the
    /// monotonic clock and of the clock of the system since 1970, as a `long`. Only the
    /// language's own module calls them.
    Clock(bool),
    /// `Entropy.fill(ref bytes)`: random bytes of the system into a `ref byte[]`. Only the
    /// language's own module calls it.
    Entropy,
    /// The calls of the file system that `File` does not have a word for, and that only the
    /// language's own module makes (`System.list`, `System.status`), with `File.delete`,
    /// `File.move`, `File.replace` and `File.copy`.
    System(SystemCall),
    /// `slot.take()`: gives the value and leaves `null`.
    Take,
    /// `slot.replace(value)`: gives the old value and leaves the new one.
    Replace,
}

/// The functions of `Memory` that are called on raw memory; `Memory.allocate<T>(n)` is an
/// `Allocate`, since it names a type.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MemoryCall {
    /// `Memory.free(p)`: gives back what `Memory.allocate` gave; `null` is let be.
    Free,
    /// `Memory.copy(source, target, count)`: `count` items, also when the two overlap.
    Copy,
}

/// The functions of a `CancellationSource` and a `CancellationToken`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CancelCall {
    /// `CancellationSource()`
    New,
    /// `CancellationSource.after(ms)`: a source that cancels itself when that many
    /// milliseconds have gone by.
    After,
    /// `source.cancel()`
    Cancel,
    /// `source.token()`: one more token of the source.
    Token,
    /// `source.isCancelled()`, `token.isCancelled()`
    IsCancelled,
    /// `token.check()`: `Error(Cancelled { })` once the token is cancelled, `Ok()` before.
    Check,
    /// `token.clone()`: one more token of the same source.
    Clone,
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
    /// `System.variable(name, ref bytes)`: a variable of the environment, `false` when it is not set.
    Variable,
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
    /// `unsafe { ... }`: statements that may do what the checker cannot see is safe, such as
    /// reading and writing raw memory.
    Unsafe(Block),
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
    /// `unsafe e`: a value whose computation may do what the checker cannot see is safe, such
    /// as reading raw memory. A prefix, like `try` and `await`.
    Unsafe(Box<Expr>),
    /// `Memory.allocate<T>(n)`: room for `n` items of `T`, all zero, as a `raw T`. `args` holds
    /// what was written between the parentheses.
    Allocate { item: TypeExpr, args: Vec<Expr> },
    /// `(raw byte)p`: a raw pointer read as a pointer to another type.
    RawCast { ty: TypeExpr, expr: Box<Expr> },
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
