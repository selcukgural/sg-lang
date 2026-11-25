//! Static checks, done before the program runs: names, types, `null` safety, use after move,
//! `return` paths.
//!
//! The checker also resolves what the interpreter would otherwise have to work out again:
//! literal types, field indexes and implicit conversions are written into the tree.
//!
//! Moves are followed per variable and per field: `node.next` can be moved out of an owned
//! `node` while `node.value` stays usable.
//!
//! Not checked yet: borrow lifetimes (`from`), and conflicts between a `ref` and other borrows.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::*;
use crate::diag::{nearest, Diag, Span};
use crate::exhaust;
use crate::source::{std_types, STD_MODULE};
use crate::types::{FloatTy, IntTy, TextIter, Ty};

type Res<T> = Result<T, Diag>;

/// The types of what goes over a text: they have names in messages, not yet in a program.
const TEXT_ITERATORS: &[&str] = &["Chars", "Split", "SplitWhitespace", "Lines"];

const INTERFACES_NOT_YET: &[&str] = &["Iterator"];

const INTERFACES: &[&str] = &["Copyable", "Clonable", "Formattable", "Equatable", "Hashable", "Comparable", "Disposable", "Closable"];

const BUILTINS: &[&str] = &["print", "printError", "assert", "panic", "exit", "same", "Ok", "Error", "embed"];

/// Types the language brings along; a program cannot define another one under these names.
const BUILTIN_TYPES: &[&str] = &["Result", "List", "HashMap", "HashSet", "Func", "CFunc", "TextError", "NumberError", "IoError", "Cancelled", "File", "Stdin", "Stdout", "Stderr", "Task", "TaskGroup", "CancellationSource", "CancellationToken", "TcpListener", "TcpStream", "Memory", "C", "CString", "Duration", "Instant", "DateTime", "DateTimeOffset", "DayOfWeek", "DateError", "Random", "SecureRandom", "Directory", "DirectoryEntry", "FileInfo", "FileKind", "SeekFrom", "Path", "Environment", "VariableError", "OperatingSystem"];

struct StructInfo {
    fields: Vec<(String, Ty)>,
    /// For each field: whether other modules see it (`public`).
    open: Vec<bool>,
    /// For each field: whether it is `readonly`.
    readonly: Vec<bool>,
    /// For each field: what a literal that leaves it out gives it (`int port = 80`).
    defaults: Vec<Option<FieldDefault>>,
    /// Functions written in the struct body, with or without `this`.
    methods: HashMap<String, FuncSig>,
    /// Written with `: Disposable`: its `dispose` runs when its last owner ends.
    disposable: bool,
    /// Written with `: Closable`: a local that holds one is closed before it ends.
    closable: bool,
    /// A type of C declared in an `extern` block, seen only behind a raw pointer.
    opaque: bool,
    /// How C lays it out: `union`, `packed`, `align(N)`, bit fields and the bits left empty.
    /// Its `fields` are filled in once the struct is found to hold plain data.
    shape: crate::raw::Plain,
}

struct EnumInfo {
    /// Variants in declaration order, each with the fields it carries.
    variants: Vec<(String, Vec<(String, Ty)>)>,
    /// Functions written in the enum body, with or without `this`.
    methods: HashMap<String, FuncSig>,
    /// `enum Block : byte`: the integer type of the numbers of its variants.
    numbers: Option<IntTy>,
}

#[derive(Clone, Copy, PartialEq)]
enum TypeKind {
    Struct,
    Enum,
    /// `using NodeRef = shared Node`: another name for a type.
    Alias,
}

/// The types of the program, known before anything else is resolved.
#[derive(Default)]
struct Names {
    kinds: HashMap<String, TypeKind>,
    /// Types written with `: Hashable`: they can be the key of a `HashMap`.
    keys: HashSet<String>,
    /// Types written with `: Clonable`: `.clone()` makes a copy of them, field by field unless
    /// they write their own `clone` (choice 1818).
    clonable: HashSet<String>,
    /// Types that carry a borrow, each with the name of its source: `struct Token<from source>`.
    carriers: HashMap<String, String>,
    /// The files of the program: a name is looked up from the file it is written in.
    files: Vec<FileScope>,
    /// The modules of the program besides the root.
    modules: HashSet<String>,
    /// The functions, by full name.
    funcs: HashSet<String>,
    /// The types and functions written with `public`, by full name.
    public: HashSet<String>,
    /// Where each type and function is defined.
    spans: HashMap<String, Span>,
    /// What each alias names, by full name: `using NodeRef = shared Node`.
    aliases: HashMap<String, TypeExpr>,
    /// The types of C declared in `extern` blocks (`struct sqlite3`), by full name: they are
    /// written only behind a raw pointer.
    opaque: HashSet<String>,
    /// The variables of C declared in `extern` blocks (`int opterr`), by full name.
    cvars: HashSet<String>,
    /// What each of them holds, once the types are known.
    cvar_types: HashMap<String, Ty>,
    /// Types written with `: unsafe Sendable`: what their raw pointers reach, their writer
    /// promises, may be reached from several tasks, so they go to another task.
    sendable: HashSet<String>,
    /// The consts of the modules, by full name.
    consts: HashSet<String>,
    /// What each integer const holds, by full name (one of a block too), found before the types
    /// are, for the count of an array (`byte[BlockSize]`); or why it cannot be found then.
    sizes: HashMap<String, Result<i128, String>>,
    /// The size and the alignment of each struct and enum, at most what C gives them, once the
    /// types are known: a type written after that is measured against `VALUE_BYTES`.
    layouts: Option<HashMap<String, (u64, u64)>>,
    /// The statics of the modules, by full name.
    statics: HashSet<String>,
    /// What each static holds (one of a block too), once the types are known; for `static
    /// locked T`, the `T`.
    static_types: HashMap<String, Ty>,
    /// The statics written `static locked`.
    locked_statics: HashSet<String>,
}

thread_local! {
    /// The aliases being resolved, the outer first: one that names itself is found here.
    static RESOLVING: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

impl Names {
    fn file(&self, at: Span) -> Option<&FileScope> {
        self.files.iter().find(|file| at.start <= file.end)
    }

    /// The module of the file a position is in; empty for the root.
    fn module(&self, at: Span) -> &str {
        self.file(at).map_or("", |file| file.module.as_str())
    }

    fn known(&self, full: &str) -> bool {
        self.kinds.contains_key(full) || self.funcs.contains(full) || self.cvars.contains(full) || self.consts.contains(full) || self.statics.contains(full)
    }

    /// The full name of the type or function that `written` means in the file at `at`: a name
    /// of the file's own module first, then a `public` name of a module the file names with
    /// `use`, then a type of the language's own module. With a module in front (`billing.Line`)
    /// no `use` is needed.
    fn full(&self, written: &str, at: Span) -> Res<Option<String>> {
        let (module, uses) = self.file(at).map_or(("", &[][..]), |file| (file.module.as_str(), file.uses.as_slice()));
        let hidden = |name: &str, from: &str, full: &str| {
            let diag = Diag::error(at, format!("`{name}` exists in the `{from}` module but is not public"));
            match self.spans.get(full) {
                Some(span) => diag.related(*span, "it is defined here, without `public`"),
                None => diag,
            }
        };
        if let Some((from, name)) = written.rsplit_once('.') {
            if let Some(file) = self.file(at) {
                if self.known(written) && !file.map.reaches(from) {
                    return Ok(None);
                }
                if let Some(parent) = crate::ast::internal_to(module, from) {
                    if self.known(written) {
                        return Err(internal_diag(at, &file.map, from, parent));
                    }
                }
            }
            return match self.known(written) {
                true if from == module || self.public.contains(written) => Ok(Some(written.to_string())),
                true => Err(hidden(name, from, written)),
                false => Ok(None),
            };
        }
        let own = if module.is_empty() { written.to_string() } else { format!("{module}.{written}") };
        if self.known(&own) {
            return Ok(Some(own));
        }
        if written == TEXT_ERROR || written == NUMBER_ERROR || written == IO_ERROR || written == CANCELLED {
            return Ok(Some(written.to_string()));
        }
        let mut found: Vec<String> = Vec::new();
        let mut closed = None;
        for (used, _) in uses {
            let full = format!("{used}.{written}");
            match self.known(&full) {
                true if self.public.contains(&full) => found.push(full),
                true => closed = Some((used, full)),
                false => {}
            }
        }
        // The types of the language's own module are seen from every module, after the module's
        // own names and those of the modules it names with `use`: a type the program wrote is
        // never taken for one the language adds later (choice 1456); `std.Log` reaches it.
        if found.is_empty() && std_types().any(|name| name == written) {
            let full = format!("{STD_MODULE}.{written}");
            if self.known(&full) {
                return Ok(Some(full));
            }
        }
        match found.len() {
            1 => Ok(found.pop()),
            0 => match closed {
                Some((used, full)) => Err(hidden(written, used, &full)),
                None => Ok(None),
            },
            _ => {
                let modules: Vec<String> = found.iter().map(|full| format!("`{}`", module_of(full))).collect();
                Err(Diag::error(at, format!("`{written}` exists in {}", modules.join(" and ")))
                    .hint(format!("write the module in front: `{}`", found[0])))
            }
        }
    }

    /// For a name that was not found: the `use` line that would bring it in.
    fn use_hint(&self, written: &str, at: Span) -> Option<String> {
        let file = self.file(at)?;
        let mut modules: Vec<&String> = self.modules.iter().filter(|module| file.map.reaches(module) && crate::ast::internal_to(&file.module, module).is_none()).collect();
        modules.sort();
        let module = modules.into_iter().find(|module| {
            let full = format!("{module}.{written}");
            self.known(&full) && self.public.contains(&full) && !file.uses.iter().any(|(used, _)| used == *module)
        })?;
        Some(format!("`use {}` at the top of the file", file.map.written(module)))
    }
}

/// A module in a folder `internal`, named from outside the folder that holds it (choice 1561).
fn internal_diag(at: Span, map: &crate::ast::ModuleMap, module: &str, parent: &str) -> Diag {
    let (module, parent) = (map.written(module), map.written(parent));
    let holder = match parent.is_empty() {
        true => "the program's own modules".to_string(),
        false => format!("the modules inside `{parent}`"),
    };
    Diag::error(at, format!("`{module}` is in a folder `internal`: only {holder} name it")).note("a folder `internal` keeps what is inside it for the folder that holds it")
}

/// What to say about a type that lacks an interface: a type of the program can have it added, a
/// type of the language's own module cannot.
fn lacks(diag: Diag, name: &str, interface: &str) -> Diag {
    if module_of(name) != STD_MODULE {
        return diag.hint(format!("add it to the definition: `{name} : {interface}`"));
    }
    match (short_name(name), interface) {
        ("Instant", "Formattable") => diag
            .note("an `Instant` means something only next to another one")
            .hint("the time between two is a `Duration`, which is written: `start.elapsed()`, `later.since(earlier)`"),
        (short, _) => diag.note(format!("`{short}` is a built-in type, and it is not `{interface}`")),
    }
}

/// How a value of a type of the language's own module is made.
fn std_makers(short: &str) -> &'static str {
    match short {
        "Duration" => "`Duration.fromMilliseconds(n)`, `Duration.fromSeconds(n)`",
        "Instant" => "`Instant.now()`",
        "Random" => "`Random.fromSeed(seed)` gives the same numbers for the same seed, `Random.fromSystem()` different ones on every run",
        "SecureRandom" => "nothing is made: the functions are called on the type, `SecureRandom.fill(ref bytes)`, `SecureRandom.hexString(32)`",
        "Directory" => "nothing is made: the functions are called on the type, `Directory.list(read path)`, `Directory.create(read path)`",
        "Path" => "nothing is made: the functions are called on the type, `Path.join(read folder, name)`",
        "Environment" => "nothing is made: the functions are called on the type, `Environment.get(\"HOME\")`",
        "FileInfo" => "`File.info(read path)`",
        "DirectoryEntry" => "`Directory.list(read path)` gives one for each name in the folder",
        "SocketAddress" => "`SocketAddress.parse(\"127.0.0.1:53\")`, `SocketAddress.lookup(\"example.com:53\")`, `socket.localAddress()`",
        "UdpSocket" => "`UdpSocket.bind(\":0\")`",
        "Command" => "`Command(\"ls\")`, then `cmd.arg(word)`",
        "Process" => "`cmd.start()`",
        "Received" => "`socket.receiveFrom(ref buffer)`",
        "Hex" => "nothing is made: the functions are called on the type, `Hex.encode(read bytes)`, `Hex.decode(text)`",
        "Base64" => "nothing is made: the functions are called on the type, `Base64.encode(read bytes)`, `Base64.decode(text)`",
        "Crc32" => "`Crc32()`, then `c.update(read bytes)` and `c.value()`; or `Crc32.of(read bytes)` at once",
        "Sha256" => "`Sha256()`, then `h.update(read bytes)` and `h.finish()`; or `Sha256.of(read bytes)` at once",
        "Hmac" => "nothing is made: the function is called on the type, `Hmac.sha256(read key, read data)`",
        "Bytes" => "nothing is made: the function is called on the type, `Bytes.equalInConstantTime(read a, read b)`",
        "CommandLine" => "`CommandLine(\"name\", \"what it does\")`, then `cl.flag(...)`, `cl.option(...)`, `cl.positional(...)`",
        "Arguments" => "`cl.parse(read args)` or `cl.parseOrExit(read args)` of a `CommandLine` gives one",
        "Json" => "nothing is made: the function is called on the type, `Json.parse(text)`",
        "JsonObject" => "`JsonObject()`, then `o.add(\"name\", value)`",
        "JsonError" => "`Json.parse(text)` gives one for a text that is not JSON",
        "JsonReader" => "`JsonReader()`, then `reader.next(read text)` gives the tokens of the text one after another",
        "Regex" => "`Regex.of(\"pattern\")` for a pattern written in the program, `Regex.compile(pattern)` for one made when it runs",
        "Match" => "`regex.find(text)` gives one, and `regex.findAll(text)` a list of them",
        "Captures" => "`regex.captures(text)` gives one",
        "RegexError" => "`Regex.compile(pattern)` gives one for a pattern that is not a regular expression",
        "Adler32" => "`Adler32()`, then `a.update(read bytes)` and `a.value()`; or `Adler32.of(read bytes)` at once",
        "Deflate" => "nothing is made: the functions are called on the type, `Deflate.compress(read bytes, 6)`, `Deflate.decompress(read bytes, limit)`",
        "Zlib" => "nothing is made: the functions are called on the type, `Zlib.compress(read bytes, 6)`, `Zlib.decompress(read bytes, limit)`",
        "Gzip" => "nothing is made: the functions are called on the type, `Gzip.compress(read bytes, 6)`, `Gzip.decompress(read bytes, limit)`",
        "Compressor" => "`Gzip.writer(target, level)` gives one, and `Zlib.writer` and `Deflate.writer`",
        "Decompressor" => "`Gzip.reader(source, limit)` gives one, and `Zlib.reader` and `Deflate.reader`",
        "ZipReader" => "`ZipReader.open(read path)` gives one, or `ZipReader.of(source)` for a reader of a file",
        "ZipWriter" => "`ZipWriter.create(target)` gives one, then `zip.add(name, read bytes, ZipMethod.Deflated, modified)`",
        "ZipEntry" => "`reader.entries()` of a `ZipReader` gives one for each entry of the archive",
        "DateTime" => "`DateTime.utc(year, month, day, hour, minute, second)`, `DateTime.fromUnixSeconds(n)`",
        _ => "nothing is made: the functions are called on the type",
    }
}

/// The first type in `ty` that other modules cannot see.
fn closed_in<'t>(ty: &'t Ty, names: &Names) -> Option<&'t str> {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => (!names.public.contains(name)).then_some(name.as_str()),
        Ty::Ptr(inner) | Ty::Shared(inner, _) | Ty::Weak(inner, _) | Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) | Ty::Read(inner) | Ty::Ref(inner) => closed_in(inner, names),
        Ty::Result(a, b) | Ty::Map(a, b) | Ty::Entry(a, b) => closed_in(a, names).or_else(|| closed_in(b, names)),
        Ty::Func(params, ret) => params.iter().find_map(|param| closed_in(param, names)).or_else(|| closed_in(ret, names)),
        _ => None,
    }
}

/// A `public` name of a module shows a type the other modules cannot name.
fn exposes(what: &str, span: Span, ty: &Ty, names: &Names) -> Res<()> {
    let Some(closed) = closed_in(ty, names) else { return Ok(()) };
    let short = short_name(closed);
    let diag = Diag::error(span, format!("`{what}` is public, but it shows `{short}`, which is not"))
        .hint(format!("write `public` in front of `{short}`, or take it off `{what}`"));
    Err(match names.spans.get(closed) {
        Some(defined) => diag.related(*defined, format!("`{short}` is defined here, without `public`")),
        None => diag,
    })
}

/// One place a returned borrow may come from.
/// The constants of a number type: `int.MaxValue`, `double.NaN`.
fn constant(ty: &Ty, name: &str, span: Span) -> Res<()> {
    let names: &[&str] = match ty {
        Ty::Int(_) => &["MinValue", "MaxValue"],
        _ => &["MinValue", "MaxValue", "NaN", "Infinity", "Pi", "E"],
    };
    if names.contains(&name) {
        return Ok(());
    }
    let diag = Diag::error(span, format!("`{ty}` has no constant `{name}`"));
    if name == "Epsilon" && matches!(ty, Ty::Float(_)) {
        return Err(diag.note("left out on purpose: in C# `Epsilon` is the smallest number above zero, in C and Rust the gap between 1 and the next number"));
    }
    Err(match nearest(name, names.iter().copied()) {
        Some(close) => diag.hint(format!("`{ty}.{close}`")),
        None => diag.note(format!("constants: {}", names.join(", "))),
    })
}

/// `(T)x` for the fix of a conversion: the value in parentheses unless it is a name, a number
/// or a call, which a cast takes as it is.
fn cast_of(to: &str, expr: &Expr, text: &str) -> String {
    match is_simple(expr) {
        true => format!("({to}){text}"),
        false => format!("({to})({text})"),
    }
}

/// An expression a cast or a method call after it takes without parentheses.
fn is_simple(expr: &Expr) -> bool {
    matches!(peel_widen(expr).kind, ExprKind::Var(_) | ExprKind::Int(..) | ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::Call { .. } | ExprKind::Method { .. } | ExprKind::Const(_) | ExprKind::Str(_))
}

fn peel_widen(expr: &Expr) -> &Expr {
    match &expr.kind {
        ExprKind::Widen(_, inner) | ExprKind::Load(inner) => peel_widen(inner),
        _ => expr,
    }
}

/// The integer type a `tryTo...`, `wrappingTo...` or `saturatingTo...` method converts to:
/// `tryToInt` gives an `int?`. A conversion that cannot fail is an error, as is cutting a
/// fractional number.
fn conversion(from: &Ty, name: &str, span: Span) -> Res<IntTy> {
    let (kind, rest) = ["tryTo", "wrappingTo", "saturatingTo"]
        .into_iter()
        .find_map(|kind| name.strip_prefix(kind).map(|rest| (kind, rest)))
        .expect("tested by the caller");
    let wrapping = kind == "wrappingTo";
    let lower = rest.to_lowercase();
    let no_method = || Diag::error(span, format!("`{from}` has no method `{name}`"));
    let Some(to) = IntTy::ALL.into_iter().find(|ty| ty.name().to_lowercase() == lower) else {
        if let Some(float) = FloatTy::from_name(&lower) {
            return Err(no_method().hint(format!("`({})x` never fails; it rounds", float.name())));
        }
        return Err(no_method().note("the conversions are named after the type: `tryToInt`, `tryToUlong`, `wrappingToByte`, `saturatingToShort`"));
    };
    let spelled = to.title();
    if rest != spelled {
        return Err(no_method().hint(format!("`{kind}{spelled}`")));
    }
    match from {
        Ty::Float(_) if wrapping => Err(no_method().note(format!(
            "a fractional number has no bits to cut to size; `tryTo{spelled}()` gives `null` when it does not fit, and `saturatingTo{spelled}()` the nearest end"
        ))),
        Ty::Int(from) if *from == to => Err(Diag::error(span, format!("this is an `{}` already: there is nothing to convert", to.name()))),
        Ty::Int(from) if from.widens_to(to) => Err(Diag::error(span, format!("every `{}` fits in `{}`: there is nothing to convert", from.name(), to.name()))
            .hint(format!("it becomes one on its own: `{} x = value`", to.name()))),
        _ => Ok(to),
    }
}

/// The methods of `float` and `double` that take nothing and give the same type: the rest of
/// `<math.h>` after the ones of 162.
pub const FLOAT_ONE: &[&str] = &[
    "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "cbrt", "exp2", "expMinusOne", "logOnePlus", "erf", "erfc", "gamma", "logGamma", "fraction",
    "nextUp", "nextDown", "roundToEven",
];

/// The ends of `clamp`, when both are numbers written in the program: the low one must not be
/// above the high one.
fn clamp_ends(args: &[Expr]) -> Res<()> {
    fn known(expr: &Expr) -> Option<f64> {
        match &expr.kind {
            ExprKind::Int(value, _) => Some(*value as f64),
            ExprKind::Float(text, _) => text.parse().ok(),
            ExprKind::Unary(UnOp::Neg, inner) => known(inner).map(|value| -value),
            ExprKind::Widen(_, inner) => known(inner),
            _ => None,
        }
    }
    if let (Some(low), Some(high)) = (known(&args[0]), known(&args[1])) {
        if low > high {
            return Err(Diag::error(args[0].span.to(args[1].span), "the low end of `clamp` is above its high end, so no number lies between them")
                .note("`x.clamp(low, high)` takes the low end first"));
        }
    }
    Ok(())
}

/// `tryToInt`, `wrappingToByte`, `saturatingToShort` and the others: a conversion to an integer type.
pub fn is_conversion(name: &str) -> bool {
    name.starts_with("tryTo") || name.starts_with("wrappingTo") || name.starts_with("saturatingTo")
}

/// Whether a method on a value of this type is a method of `string`: the text itself, a borrow
/// of it, or a `string*`.
fn is_text_value(ty: &Ty) -> bool {
    match ty.strip_borrow() {
        Ty::Str => true,
        Ty::Ptr(inner) => *inner.strip_borrow() == Ty::Str,
        _ => false,
    }
}

/// A built-in method that only gives a value: computing it and throwing the value away does
/// nothing. The built-in methods that change their value and give one too (`removeLast`, the
/// `add` and `remove` of a set, the `remove` and `getOrAdd` of a map, `next`) are left out:
/// the change is what they are called for.
fn gives_only(name: &str, callee: &Callee) -> bool {
    match callee {
        Callee::Join | Callee::WithCapacity | Callee::ListRepeat | Callee::ListWithCapacity | Callee::FromBytes { .. } | Callee::StructFromBytes(_) | Callee::FromValue(_) => true,
        Callee::Cancel(call) => *call != CancelCall::Cancel && *call != CancelCall::Check,
        Callee::Net(NetCall::Port | NetCall::Reader) => true,
        Callee::Plain => !matches!(name, "removeLast" | "add" | "remove" | "getOrAdd" | "next" | "makeUnique"),
        Callee::Clone(_) => true,
        _ => false,
    }
}

/// The call under `try` and `await`, or the expression itself.
fn peel_try(expr: &Expr) -> &Expr {
    match &expr.kind {
        ExprKind::Try { expr, .. } | ExprKind::Await(expr) => peel_try(expr),
        _ => expr,
    }
}


/// The methods of `char` and `byte` that test a character against ASCII.
const ASCII_TESTS: &[&str] = &["isAscii", "isAsciiDigit", "isAsciiLetter", "isAsciiAlphanumeric", "isAsciiUpper", "isAsciiLower", "isAsciiWhitespace"];

/// The methods of `char` and of `string`, for a name that is not one of them.
const CHAR_METHODS: &[&str] = &[
    "isAscii", "isAsciiDigit", "isAsciiLetter", "isAsciiAlphanumeric", "isAsciiUpper", "isAsciiLower", "isAsciiWhitespace",
    "toAsciiUpper", "toAsciiLower", "isWhitespace", "utf8Length", "toString", "equals", "compare",
    "isLetter", "isNumeric", "isAlphanumeric", "isUpper", "isLower", "isControl", "toUpper", "toLower", "category",
];
/// The methods of `char` that need the Unicode tables.
const UNICODE_TESTS: &[&str] = &["isLetter", "isNumeric", "isAlphanumeric", "isUpper", "isLower", "isControl"];
const TEXT_METHODS: &[&str] = &[
    "length", "clone", "asBytes", "write", "clear", "indexOf", "lastIndexOf", "contains", "startsWith", "endsWith", "trim", "trimStart", "trimEnd",
    "chars", "split", "splitWhitespace", "lines", "replaceAll", "repeat", "equalsIgnoreAsciiCase", "equals", "compare",
    "toUpper", "toLower", "foldCase", "equalsIgnoreCase", "normalize", "isNormalized",
];

/// The searches in items that are in order, by the names other languages give them.
fn search_name(name: &str) -> Option<&'static str> {
    match name {
        "binary_search" | "BinarySearch" | "bsearch" | "search" => Some("binarySearch"),
        "lower_bound" | "partition_point" | "partitionPoint" | "bisect_left" | "bisectLeft" => Some("lowerBound"),
        "upper_bound" | "bisect_right" | "bisectRight" | "bisect" => Some("upperBound"),
        _ => None,
    }
}

/// A method a `char` or a `string` does not have: the one it may have meant.
/// What a `StringBuilder` of C# is here.
const STRING_BUILDER: &str = "a `string` is built where it is: `var text = \"\"`, then `text.write(...)`";

fn text_method_hint(diag: Diag, target: &Ty, name: &str) -> Diag {
    let text = *target == Ty::Str;
    let lower = name.to_lowercase();
    match lower.as_str() {
        "touppercase" | "tolowercase" | "to_uppercase" | "to_lowercase" | "toupperinvariant" | "tolowerinvariant" => {
            return diag.hint(format!("`{}()`", if lower.contains("upper") { "toUpper" } else { "toLower" }));
        }
        "isdigit" | "isasciidigit" | "is_digit" | "is_numeric" | "isnumber" if !text => {
            return diag.hint("`isAsciiDigit()`: the digits `parse` reads").note("`isNumeric()` says yes to the digits of every script, and to `'½'` and `'²'`, which `parse` does not read");
        }
        "isalphabetic" | "is_alphabetic" if !text => return diag.hint("`isLetter()`"),
        "isletterordigit" | "is_alphanumeric" if !text => return diag.hint("`isAlphanumeric()`"),
        "isuppercase" | "is_uppercase" if !text => return diag.hint("`isUpper()`"),
        "islowercase" | "is_lowercase" if !text => return diag.hint("`isLower()`"),
        "casefold" | "tocasefold" | "fold" | "folded" | "tofoldcase" if text => return diag.hint("`foldCase()`: the text as it is compared without case"),
        "nfc" | "nfd" | "nfkc" | "nfkd" | "tonfc" | "tonfd" | "tonfkc" | "tonfkd" | "normalise" | "normalized" if text => {
            let form = lower.trim_start_matches("to").trim_start_matches("nf").to_uppercase();
            let form = if matches!(form.as_str(), "C" | "D" | "KC" | "KD") { form } else { "C".to_string() };
            return diag.hint(format!("`normalize(NormalizationForm.{form})`: a new text in that normal form of Unicode"));
        }
        "is_nfc" | "is_nfd" | "is_nfkc" | "is_nfkd" | "isnfc" | "isnfd" | "isnfkc" | "isnfkd" | "isnormalised" if text => {
            let form = lower.trim_start_matches("is").trim_start_matches('_').trim_start_matches("nf").to_uppercase();
            let form = if matches!(form.as_str(), "C" | "D" | "KC" | "KD") { form } else { "C".to_string() };
            return diag.hint(format!("`isNormalized(NormalizationForm.{form})`"));
        }
        "totitle" | "totitlecase" | "capitalize" => return diag.note("the title case is not here yet"),
        "substring" | "substr" | "slice" if text => return diag.hint("a range of the text: `text[a..b]`, `text[a..]`"),
        "equals_ignore_case" | "equalsignorecaseinvariant" | "eq_ignore_case" if text => return diag.hint("`equalsIgnoreCase`"),
        "charat" | "at" if text => return diag.hint("the characters, one after the other: `for c in text.chars()`; one byte: `text[i]`"),
        "replacefirst" | "replacen" | "replace_all" | "replaceall" if text => {
            return diag.hint("`replaceAll(old, new)` gives a new text with every `old` replaced").note("replacing only the first ones is not here yet")
        }
        "repeat" | "times" if !text => return diag.hint("a text of one character repeated: `c.toString().repeat(n)`, or `\"-\".repeat(n)` for a constant"),
        "len" | "count" | "size" if text => return diag.hint("`length()`: the number of bytes"),
        "append" | "appendline" | "push" | "pushstr" | "push_str" | "add" | "concat" | "insert" if text => {
            return diag.hint("`write` adds at the end of the text: `text.write(...)`")
        }
        _ => {}
    }
    let names = if text { TEXT_METHODS } else { CHAR_METHODS };
    match nearest(name, names.iter().copied()) {
        Some(close) => diag.hint(format!("`{close}`")),
        None => diag,
    }
}

/// What `string.fromUtf8` gives when the bytes are not text.
const TEXT_ERROR: &str = "TextError";

/// The built-in `struct TextError : Copyable, Formattable, Equatable { public readonly nint
/// position }`: where the first byte lies that is not valid UTF-8. It is put among the structs
/// of the program, so everything that works on a struct works on it.
fn text_error() -> StructDecl {
    let nowhere = Span::new(0, 0);
    let interfaces = ["Copyable", "Formattable", "Equatable"].iter().map(|name| (name.to_string(), nowhere)).collect();
    let ty = TypeExpr { mode: None, holder: None, checked: false, locked: false, atomic: None, raw: 0, name: "nint".to_string(), args: Vec::new(), suffixes: Vec::new(), span: nowhere };
    let position = FieldDecl { public: true, readonly: true, ty, name: "position".to_string(), span: nowhere, bits: None, default: None };
    StructDecl { public: true, name: TEXT_ERROR.to_string(), name_span: nowhere, sources: Vec::new(), interfaces, fields: vec![position], methods: Vec::new(), opaque: false, layout: LayoutDecl::default() }
}

/// What `int.parse` and the other `parse` functions give when the text is not a number.
const NUMBER_ERROR: &str = "NumberError";

/// The built-in `enum NumberError : Copyable, Formattable, Equatable { Invalid(nint position),
/// OutOfRange }`: the text is not a number from the byte at `position` on (the length of the
/// text when it ends too early), or it is one that the type does not hold.
fn number_error() -> EnumDecl {
    let nowhere = Span::new(0, 0);
    let interfaces = ["Copyable", "Formattable", "Equatable"].iter().map(|name| (name.to_string(), nowhere)).collect();
    let ty = TypeExpr { mode: None, holder: None, checked: false, locked: false, atomic: None, raw: 0, name: "nint".to_string(), args: Vec::new(), suffixes: Vec::new(), span: nowhere };
    let position = FieldDecl { public: true, readonly: true, ty, name: "position".to_string(), span: nowhere, bits: None, default: None };
    let variants = vec![
        VariantDecl { name: "Invalid".to_string(), span: nowhere, fields: vec![position], number: None },
        VariantDecl { name: "OutOfRange".to_string(), span: nowhere, fields: Vec::new(), number: None },
    ];
    EnumDecl { public: true, name: NUMBER_ERROR.to_string(), name_span: nowhere, sources: Vec::new(), interfaces, numbers: None, variants, methods: Vec::new() }
}

/// What a cancelled wait gives: `token.check()`, `Task.delay(ms, read token)`.
const CANCELLED: &str = "Cancelled";

/// The built-in `struct Cancelled : Copyable, Formattable, Equatable { }`: the token of the
/// work was cancelled. It carries nothing; `Cancelled { }` makes one.
fn cancelled() -> StructDecl {
    let nowhere = Span::new(0, 0);
    let interfaces = ["Copyable", "Formattable", "Equatable"].iter().map(|name| (name.to_string(), nowhere)).collect();
    StructDecl { public: true, name: CANCELLED.to_string(), name_span: nowhere, sources: Vec::new(), interfaces, fields: Vec::new(), methods: Vec::new(), opaque: false, layout: LayoutDecl::default() }
}

/// What reading or writing a file, or the input of the program, gives when it fails.
const IO_ERROR: &str = "IoError";

/// The built-in `enum IoError : Copyable, Formattable, Equatable { NotFound, PermissionDenied,
/// IsDirectory, NoSpace, InvalidText(long position), Other(int code), Cancelled(Cancelled
/// cancelled), ConnectionRefused, ConnectionReset, AddressInUse, HostNotFound, LineTooLong,
/// AlreadyExists, NotDirectory, DirectoryNotEmpty, InvalidInput, MessageTooLong, NotSeekable,
/// TooLong, AddressNotAvailable, NotAFile }`. The first four are what a program most often tells apart; `InvalidText` is a byte that is not UTF-8
/// where text was read, counted from the start of the file or of the input; `Other` keeps the
/// code the system gave; `Cancelled` is async input or output whose token was cancelled, and
/// lets `try` turn the error of `token.check()` into an `IoError`. The last four are the
/// network's: nobody listens at the address, the other side broke the connection off, the
/// address is taken, and the host could not be found (or the address is not `host:port`).
/// `LineTooLong`: the reader of a connection met a line, or a text, longer than its limit.
/// `AlreadyExists`: a file or a folder is where one was to be made (`File.move`, `File.copy`,
/// `Directory.create`); `NotDirectory`: a part of the path, or the path of a folder, is a file
/// (or a link where a folder was to be removed); `DirectoryNotEmpty`: a folder to be removed
/// holds something. `InvalidInput`: what was given cannot be handed to the system, as an
/// argument of a program with a zero byte in it. `MessageTooLong`: a datagram the system cannot
/// send as one message, or one longer than the buffer it was received into. `NotSeekable`: `seek`
/// on a stream that is not a file opened by `File` (a pipe, the input of the program), or on a
/// writer that writes at the end (`File.openAppend`). `TooLong`: `fill` on a reader whose block
/// holds its limit already, which `consume` makes room in (1736). `AddressNotAvailable`: the
/// address is not one of the machine's, or no local port is left to connect from (1764).
/// `NotAFile`: `File.openRegular` met a pipe, a device or a socket, and did not open it (1873).
/// What an `IoError` is written as (1537), by variant: a short sentence in lower case, as Go's
/// errors are, followed by the number a variant carries (`invalid text at byte 812`,
/// `system error 5`); `Cancelled` writes no field. The interpreter and compiled code write it
/// from this table.
pub const IO_ERROR_TEXTS: [&str; 21] = [
    "not found",
    "permission denied",
    "is a folder",
    "no space left",
    "invalid text at byte ",
    "system error ",
    "cancelled",
    "connection refused",
    "connection reset",
    "address in use",
    "host not found",
    "line too long",
    "already exists",
    "not a folder",
    "folder not empty",
    "invalid input",
    "message too long",
    "cannot seek",
    "too long",
    "address not available",
    "not a file",
];

fn io_error() -> EnumDecl {
    let nowhere = Span::new(0, 0);
    let interfaces = ["Copyable", "Formattable", "Equatable"].iter().map(|name| (name.to_string(), nowhere)).collect();
    let field = |ty: &str, name: &str| {
        let ty = TypeExpr { mode: None, holder: None, checked: false, locked: false, atomic: None, raw: 0, name: ty.to_string(), args: Vec::new(), suffixes: Vec::new(), span: nowhere };
        FieldDecl { public: true, readonly: true, ty, name: name.to_string(), span: nowhere, bits: None, default: None }
    };
    let variant = |name: &str, fields: Vec<FieldDecl>| VariantDecl { name: name.to_string(), span: nowhere, fields, number: None };
    let variants = vec![
        variant("NotFound", Vec::new()),
        variant("PermissionDenied", Vec::new()),
        variant("IsDirectory", Vec::new()),
        variant("NoSpace", Vec::new()),
        variant("InvalidText", vec![field("long", "position")]),
        variant("Other", vec![field("int", "code")]),
        variant("Cancelled", vec![field(CANCELLED, "cancelled")]),
        variant("ConnectionRefused", Vec::new()),
        variant("ConnectionReset", Vec::new()),
        variant("AddressInUse", Vec::new()),
        variant("HostNotFound", Vec::new()),
        variant("LineTooLong", Vec::new()),
        variant("AlreadyExists", Vec::new()),
        variant("NotDirectory", Vec::new()),
        variant("DirectoryNotEmpty", Vec::new()),
        variant("InvalidInput", Vec::new()),
        variant("MessageTooLong", Vec::new()),
        variant("NotSeekable", Vec::new()),
        variant("TooLong", Vec::new()),
        variant("AddressNotAvailable", Vec::new()),
        variant("NotAFile", Vec::new()),
    ];
    EnumDecl { public: true, name: IO_ERROR.to_string(), name_span: nowhere, sources: Vec::new(), interfaces, numbers: None, variants, methods: Vec::new() }
}

#[derive(Clone, PartialEq)]
struct Source {
    /// The parameter, or `this`.
    param: String,
    /// Not the parameter itself but what it holds: `from source` in a method, `from lx.source`,
    /// or a parameter of a type that carries a borrow, taken by value.
    held: bool,
}

struct FuncSig {
    /// Written with `public`: other modules can call it.
    public: bool,
    /// Written with `async`: a call gives a `Task` of `ret`.
    is_async: bool,
    this: Option<This>,
    /// The type written in front of `this` (`read FolderRef this`), resolved: a `shared` value
    /// of the struct the method belongs to. The method is called on that value.
    holder: Option<(Ty, String)>,
    params: Vec<Ty>,
    param_names: Vec<String>,
    /// What a call that leaves the argument out gives, parameter by parameter (choices
    /// 1620-1622); only the last ones have one.
    defaults: Vec<Option<FieldDefault>>,
    ret: Ty,
    /// For a function that returns a borrow, or a value that carries one: where it may come
    /// from. Empty when it can only be a constant.
    sources: Vec<Source>,
    /// Declared in an `extern` block: a function of C, called inside `unsafe`.
    foreign: bool,
    /// Written with `export`: C calls it, and its name is a `CFunc`.
    export: bool,
    /// A function of C declared with `...`: it takes more arguments after `params`.
    variadic: bool,
}

/// What `Stdin`, `Stdout` and `Stderr` are, for messages; `None` for any other name.
fn std_stream(name: &str) -> Option<&'static str> {
    match name {
        "Stdin" => Some("the input"),
        "Stdout" => Some("the output"),
        "Stderr" => Some("the standard error"),
        _ => None,
    }
}

/// Whether a value of this type is a borrow or carries one inside.
fn lends(ty: &Ty, names: &Names) -> bool {
    match ty {
        Ty::Read(_) | Ty::Ref(_) | Ty::TextIter(_) | Ty::Reader(true) => true,
        Ty::Struct(name) | Ty::Enum(name) => names.carriers.contains_key(name),
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) | Ty::Array(inner, _) => lends(inner, names),
        Ty::Result(a, b) | Ty::Map(a, b) | Ty::Entry(a, b) => lends(a, names) || lends(b, names),
        _ => false,
    }
}

/// The struct or enum behind borrows and pointers, if it carries a borrow: its name and the
/// name of its source.
fn carrier<'n>(ty: &Ty, names: &'n Names) -> Option<(&'n str, &'n str)> {
    match peel(ty) {
        Ty::Struct(name) | Ty::Enum(name) => names.carriers.get_key_value(name).map(|(name, source)| (name.as_str(), source.as_str())),
        _ => None,
    }
}

/// `owner`: the struct whose body the function is written in.
/// Whether `T {}`, written as the default value of a field or a parameter, builds a value of
/// `ty` (a `T?` too).
fn literal_of(written: &str, ty: &Ty) -> bool {
    match ty {
        Ty::Nullable(inner) => literal_of(written, inner),
        Ty::Struct(full) => full == written || short_name(full) == written,
        _ => false,
    }
}

fn func_sig(func: &Func, names: &Names, owner: Option<&str>) -> Res<FuncSig> {
    let mut params = Vec::new();
    for param in &func.params {
        let ty = resolve_type(&param.ty, names, false)?;
        // `null` and an empty collection are checked here, where they are written, and not only
        // in the calls that leave the argument out, as the default value of a field is.
        if let Some(FieldDefault::Expr(written)) = &param.default {
            let held = ty.strip_borrow();
            match &written.kind {
                ExprKind::Null if !matches!(held, Ty::Nullable(_)) => {
                    return Err(Diag::error(written.span, format!("`{held}` cannot be `null`")).hint(format!("a value that may be missing has the type `{held}?`")));
                }
                ExprKind::Collection { ty: made, .. } => {
                    let made = resolve_type(made, names, false)?;
                    if made != *held {
                        return Err(Diag::error(written.span, format!("expected `{held}`, found `{made}`")));
                    }
                }
                ExprKind::StructLit { name, .. } if !literal_of(name, held) => {
                    return Err(Diag::error(written.span, format!("expected `{held}`, found `{name}`")));
                }
                ExprKind::Field { .. } if !matches!(held, Ty::Token) => {
                    return Err(Diag::error(written.span, format!("expected `{held}`, found `CancellationToken`")));
                }
                _ => {}
            }
        }
        params.push(ty);
    }
    let defaults = func.params.iter().map(|param| param.default.clone()).collect();
    let ret = resolve_type(&func.ret, names, true)?;
    let holder = match (&func.this_ty, owner) {
        (Some(written), Some(owner)) => {
            let ty = resolve_type(written, names, false)?;
            match &ty {
                Ty::Shared(inner, _) if matches!(&**inner, Ty::Struct(name) | Ty::Enum(name) if name == owner) => {
                    // An alias is shown by its name, as it was written.
                    let shown = match written.holder {
                        None => short_name(&written.name).to_string(),
                        Some(_) => ty.to_string(),
                    };
                    Some((ty, shown))
                }
                _ => {
                    let short = short_name(owner);
                    return Err(Diag::error(written.span, format!("the type in front of `this` is a value that holds `{short}`, and this is `{ty}`"))
                        .hint(format!("`read shared checked {short} this`, or an alias of it; `read this` for the `{short}` itself")));
                }
            }
        }
        _ => None,
    };
    let param_names: Vec<String> = func.params.iter().map(|param| param.name.clone()).collect();
    let own_source = owner.and_then(|owner| names.carriers.get(owner));

    // What a result can borrow from: a borrowed parameter, or one whose type carries a borrow.
    let receiver = match func.this {
        Some(This::Read | This::Ref) => Some(Source { param: "this".to_string(), held: false }),
        Some(This::Value) if own_source.is_some() => Some(Source { param: "this".to_string(), held: true }),
        _ => None,
    };
    let lenders = param_names.iter().zip(&params).filter(|(_, ty)| lends(ty, names));
    let borrowed: Vec<Source> = receiver
        .into_iter()
        .chain(lenders.map(|(name, ty)| Source { param: name.clone(), held: !ty.is_borrow() }))
        .collect();
    let returns_borrow = lends(&ret, names);

    let mut sources: Vec<Source> = Vec::new();
    for item in &func.from {
        let (name, span) = (&item.name, item.span);
        if !returns_borrow {
            return Err(Diag::error(span, format!("`from` says where a returned borrow comes from, and `{}` returns `{ret}`", func.name)));
        }
        let source = match &item.source {
            // `from lx.source`: what the parameter holds, not the parameter itself.
            Some(field) => {
                let ty = param_names.iter().position(|param| param == name).map(|position| &params[position]);
                match ty.and_then(|ty| carrier(ty, names)) {
                    Some((_, source)) if source == field => Source { param: name.clone(), held: true },
                    Some((ty, source)) => {
                        return Err(Diag::error(span, format!("`{ty}` has no source named `{field}`")).hint(format!("`from {name}.{source}`")));
                    }
                    None => {
                        return Err(Diag::error(span, format!("`{name}` is not a parameter whose type carries a borrow"))
                            .note("`from x.source` names the source of the type of `x`"));
                    }
                }
            }
            // `from source` in a method: what `this` holds.
            None if func.this.is_some() && own_source == Some(name) && !param_names.contains(name) => Source { param: "this".to_string(), held: true },
            None => match borrowed.iter().find(|source| source.param == *name) {
                Some(source) => source.clone(),
                None => {
                    let diag = Diag::error(span, format!("`{name}` is not a borrowed parameter of `{}`", func.name));
                    let by_value = param_names.contains(name) || (name == "this" && func.this == Some(This::Value));
                    return Err(match by_value {
                        true => diag.note(format!("`{name}` is taken by value and is gone when the function returns, so nothing can be borrowed from it")),
                        false => diag,
                    });
                }
            },
        };
        if sources.iter().any(|other| other.param == source.param) {
            return Err(Diag::error(span, format!("`{name}` is written twice")));
        }
        sources.push(source);
    }
    if returns_borrow && func.from.is_empty() {
        let written = |source: &Source| match (source.param.as_str(), own_source) {
            ("this", Some(own)) if source.held || !ret.is_borrow() => own.clone(),
            _ => source.param.clone(),
        };
        let list = borrowed.iter().map(written).collect::<Vec<_>>().join(", ");
        // A borrow hidden in the name of a type is always spelled out.
        if !ret.is_borrow() && !borrowed.is_empty() {
            return Err(Diag::error(func.name_span, format!("`{}` returns a `{ret}`, which carries a borrow, so it says where that comes from", func.name))
                .hint(format!("after the parameters: `from {list}`")));
        }
        if borrowed.len() > 1 {
            return Err(Diag::error(func.name_span, format!("`{}` returns a borrow and borrows {} parameters, so it says which of them the result comes from", func.name, borrowed.len()))
                .hint(format!("after the parameters: `from {list}`, or only the ones it can come from")));
        }
        sources = borrowed;
    }
    Ok(FuncSig { public: func.public, is_async: func.is_async, this: func.this, holder, params, param_names, defaults, ret, sources, foreign: func.foreign.is_some(), export: func.export, variadic: func.variadic })
}

/// With `tests` the `test` blocks are checked too and `main` may be missing: that is what the
/// test command compiles. A normal run leaves the tests out.
pub fn check(prog: &mut Program, src: &str, tests: bool) -> Res<()> {
    SIZES.with(|found| found.borrow_mut().clear());
    LOCKED.with(|found| found.borrow_mut().clear());
    RAW.with(|found| found.borrow_mut().clear());
    CFUNCS.with(|found| found.borrow_mut().clear());
    PATTERNS.with(|found| found.borrow_mut().clear());
    let mut names = Names { files: prog.files.clone(), ..Names::default() };
    names.modules = prog.files.iter().filter(|file| !file.module.is_empty()).map(|file| file.module.clone()).collect();
    for file in &prog.files {
        for (module, span) in &file.uses {
            if module == STD_MODULE && file.module != STD_MODULE {
                return Err(Diag::error(*span, "`std` is the language's own module, and its types are seen from every module")
                    .hint("take the line away: `Duration`, `Instant` and `DateTime` need no `use`"));
            }
            let map = &file.map;
            if names.modules.contains(module) && map.reaches(module) {
                if let Some(parent) = crate::ast::internal_to(&file.module, module) {
                    return Err(internal_diag(*span, map, module, parent));
                }
                continue;
            }
            let written = map.written(module);
            let diag = Diag::error(*span, format!("there is no module named `{written}`"));
            let seen = || names.modules.iter().filter(|other| map.reaches(other) && crate::ast::internal_to(&file.module, other).is_none());
            let whose = match map.prefix.is_empty() {
                true => "program",
                false => "package",
            };
            if let Some(inside) = crate::source::inner_modules(&file.module, &written).into_iter().find(|inside| seen().any(|other| other == inside)) {
                return Err(diag.note(format!("`use` names a module by its path from the folder of the {whose}")).hint(format!("`use {}`", map.written(&inside))));
            }
            let shown: Vec<String> = seen().map(|other| map.written(other)).collect();
            return Err(match nearest(&written, shown.iter().map(String::as_str)) {
                Some(close) => diag.hint(format!("`use {close}`")),
                None => diag.note(format!("a module is a folder with source files, next to the files of the {whose}: `{}/`", written.replace('.', "/"))),
            });
        }
    }
    for func in &prog.funcs {
        names.funcs.insert(func.name.clone());
        names.spans.insert(func.name.clone(), func.name_span);
        if func.public {
            names.public.insert(func.name.clone());
        }
    }
    for var in &prog.cvars {
        if names.funcs.contains(&var.name) || !names.cvars.insert(var.name.clone()) {
            let diag = Diag::error(var.name_span, format!("`{}` is declared twice in this module", short_name(&var.name)));
            return Err(match names.spans.get(&var.name) {
                Some(other) => diag.related(*other, "the other declaration"),
                None => diag,
            });
        }
        names.spans.insert(var.name.clone(), var.name_span);
        if var.public {
            names.public.insert(var.name.clone());
        }
    }
    for decl in prog.consts.iter().filter(|decl| decl.local.is_none()) {
        if names.funcs.contains(&decl.name) || names.cvars.contains(&decl.name) || !names.consts.insert(decl.name.clone()) {
            let diag = Diag::error(decl.name_span, format!("`{}` is declared twice in this module", short_name(&decl.name)));
            return Err(match names.spans.get(&decl.name) {
                Some(other) => diag.related(*other, "the other declaration"),
                None => diag,
            });
        }
        names.spans.insert(decl.name.clone(), decl.name_span);
        if decl.public {
            names.public.insert(decl.name.clone());
        }
    }
    for decl in prog.statics.iter().filter(|decl| decl.local.is_none()) {
        if names.funcs.contains(&decl.name) || names.cvars.contains(&decl.name) || names.consts.contains(&decl.name) || !names.statics.insert(decl.name.clone()) {
            let diag = Diag::error(decl.name_span, format!("`{}` is declared twice in this module", short_name(&decl.name)));
            return Err(match names.spans.get(&decl.name) {
                Some(other) => diag.related(*other, "the other declaration"),
                None => diag,
            });
        }
        names.spans.insert(decl.name.clone(), decl.name_span);
        if decl.public {
            names.public.insert(decl.name.clone());
        }
    }
    for decl in &prog.statics {
        if decl.locked {
            names.locked_statics.insert(decl.name.clone());
        }
    }
    let types = prog
        .structs
        .iter()
        .map(|decl| (&decl.name, decl.name_span, decl.public))
        .chain(prog.enums.iter().map(|decl| (&decl.name, decl.name_span, decl.public)))
        .chain(prog.aliases.iter().map(|decl| (&decl.name, decl.name_span, decl.public)));
    for (name, span, public) in types {
        names.spans.entry(name.clone()).or_insert(span);
        if public {
            names.public.insert(name.clone());
        }
    }
    let declared = prog
        .structs
        .iter()
        .map(|decl| (&decl.name, decl.name_span, TypeKind::Struct))
        .chain(prog.enums.iter().map(|decl| (&decl.name, decl.name_span, TypeKind::Enum)))
        .chain(prog.aliases.iter().map(|decl| (&decl.name, decl.name_span, TypeKind::Alias)));
    for (name, span, kind) in declared {
        if BUILTIN_TYPES.contains(&short_name(name)) && module_of(name) != STD_MODULE {
            return Err(Diag::error(span, format!("`{}` is a built-in type and cannot be redefined", short_name(name))));
        }
        if Ty::from_name(short_name(name)).is_some() || names.kinds.insert(name.clone(), kind).is_some() {
            return Err(Diag::error(span, format!("type `{name}` is defined twice")));
        }
        if names.consts.contains(name) {
            return Err(Diag::error(span, format!("`{}` is already the name of a const", short_name(name))).related(names.spans[name], "the const"));
        }
        if names.statics.contains(name) {
            return Err(Diag::error(span, format!("`{}` is already the name of a static", short_name(name))).related(names.spans[name], "the static"));
        }
    }
    for decl in &prog.statics {
        let short = decl.local.as_deref().unwrap_or_else(|| short_name(&decl.name));
        if BUILTINS.contains(&short) || BUILTIN_TYPES.contains(&short) || Ty::from_name(short).is_some() {
            return Err(Diag::error(decl.name_span, format!("`{short}` is a name of the language, so a static cannot have it")));
        }
    }
    for decl in &prog.consts {
        let short = decl.local.as_deref().unwrap_or_else(|| short_name(&decl.name));
        if BUILTINS.contains(&short) || BUILTIN_TYPES.contains(&short) || Ty::from_name(short).is_some() {
            return Err(Diag::error(decl.name_span, format!("`{short}` is a name of the language, so a const cannot have it")));
        }
    }
    // The count of an array may be an integer const: what it holds is found before the types.
    let by_name: HashMap<&str, &ConstDecl> = prog.consts.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    for decl in &prog.consts {
        let found = early_size(decl, &by_name, &names, &mut Vec::new());
        names.sizes.insert(decl.name.clone(), found);
    }
    names.opaque = prog.structs.iter().filter(|decl| decl.opaque).map(|decl| decl.name.clone()).collect();
    for alias in &prog.aliases {
        if let Some(mode) = alias.ty.mode {
            let word = if mode == Mode::Read { "read" } else { "ref" };
            return Err(Diag::error(alias.ty.span, "an alias names a type that owns its value, not a borrow")
                .hint(format!("name the type alone and write `{word}` where the borrow is: `{word} {}`", short_name(&alias.name))));
        }
        names.aliases.insert(alias.name.clone(), alias.ty.clone());
    }
    names.kinds.insert(TEXT_ERROR.to_string(), TypeKind::Struct);
    names.public.insert(TEXT_ERROR.to_string());
    prog.structs.push(text_error());
    names.kinds.insert(CANCELLED.to_string(), TypeKind::Struct);
    names.public.insert(CANCELLED.to_string());
    prog.structs.push(cancelled());
    names.kinds.insert(NUMBER_ERROR.to_string(), TypeKind::Enum);
    names.public.insert(NUMBER_ERROR.to_string());
    prog.enums.push(number_error());
    names.kinds.insert(IO_ERROR.to_string(), TypeKind::Enum);
    names.public.insert(IO_ERROR.to_string());
    prog.enums.push(io_error());

    // The types that carry a borrow name where it comes from.
    let sourced = prog
        .structs
        .iter()
        .map(|decl| (&decl.name, &decl.sources))
        .chain(prog.enums.iter().map(|decl| (&decl.name, &decl.sources)));
    for (name, sources) in sourced {
        match sources.as_slice() {
            [] => {}
            [(source, _)] => {
                names.carriers.insert(name.clone(), source.clone());
            }
            [_, (_, second), ..] => {
                return Err(Diag::error(*second, "a type with more than one source is not in the interpreter yet")
                    .note("for now a type that carries borrows has one source for all of them"));
            }
        }
    }

    // What each type says about itself after `:`.
    let mut copyable: HashSet<String> = HashSet::new();
    let mut formattable: HashSet<String> = HashSet::new();
    let mut equatable: HashSet<String> = HashSet::new();
    let mut comparable: HashSet<String> = HashSet::new();
    let said = prog
        .structs
        .iter()
        .map(|decl| (&decl.name, &decl.interfaces))
        .chain(prog.enums.iter().map(|decl| (&decl.name, &decl.interfaces)));
    for (name, interfaces) in said {
        for (position, (interface, span)) in interfaces.iter().enumerate() {
            if interfaces[..position].iter().any(|(earlier, _)| earlier == interface) {
                return Err(Diag::error(*span, format!("`{interface}` is written twice")));
            }
            match interface.as_str() {
                "Copyable" => copyable.insert(name.clone()),
                "Formattable" => formattable.insert(name.clone()),
                "Equatable" => equatable.insert(name.clone()),
                "Hashable" => names.keys.insert(name.clone()),
                "Clonable" => names.clonable.insert(name.clone()),
                "Comparable" => comparable.insert(name.clone()),
                "Disposable" | "Closable" if prog.enums.iter().any(|decl| decl.name == *name) => {
                    return Err(Diag::error(*span, format!("an enum cannot be `{interface}` for now: a struct can"))
                        .note("a struct that holds the enum can be, and its `dispose` sees the enum whole"));
                }
                "Disposable" | "Closable" => false,
                "unsafe Sendable" => names.sendable.insert(name.clone()),
                "Sendable" => {
                    return Err(Diag::error(*span, "`Sendable` is a promise the checker cannot check, so it is written `unsafe Sendable`")
                        .note("a type whose raw pointers reach what several tasks may reach at once, as its writer has made sure"));
                }
                _ if interface.starts_with("unsafe ") => {
                    return Err(Diag::error(*span, format!("`{interface}` is not an interface: `unsafe` goes only in front of `Sendable`"))
                        .note("the other interfaces are checked by the compiler, so they need no promise"));
                }
                _ if INTERFACES_NOT_YET.contains(&interface.as_str()) => {
                    return Err(Diag::error(*span, format!("`{interface}` is not in the interpreter yet"))
                        .note("interfaces for now: `Copyable`, `Clonable`, `Formattable`, `Equatable`, `Hashable`, `Comparable`, `Disposable` and `Closable`"));
                }
                _ => {
                    let diag = Diag::error(*span, format!("there is no interface named `{interface}`"));
                    return Err(match nearest(interface, INTERFACES.iter().copied()) {
                        Some(close) => diag.hint(format!("`{close}`")),
                        None => diag.note("interfaces for now: `Copyable`, `Clonable`, `Formattable`, `Equatable`, `Hashable`, `Comparable`, `Disposable` and `Closable`; your own are not in the interpreter yet"),
                    });
                }
            };
        }
    }

    let mut structs: HashMap<String, StructInfo> = HashMap::new();
    for decl in &prog.structs {
        let mut fields: Vec<(String, Ty)> = Vec::new();
        for field in &decl.fields {
            if fields.iter().any(|(name, _)| *name == field.name) {
                return Err(Diag::error(field.span, format!("field `{}` is declared twice", field.name)));
            }
            if field.ty.mode.is_some() && decl.sources.is_empty() {
                return Err(Diag::error(field.ty.span, format!("`{}` borrows, so `{}` names where the borrow comes from", field.name, decl.name))
                    .hint(format!("`struct {}<from source>`", decl.name)));
            }
            let field_ty = resolve_type(&field.ty, &names, false)?;
            if keeps_tasks(&field_ty) {
                return Err(task_kept(field.ty.span));
            }
            // A `readonly` field is read outside its module, and an atomic one is changed through
            // a `read` borrow: the word would promise what it cannot keep.
            if field.readonly && matches!(field_ty, Ty::Atomic(_)) {
                return Err(Diag::error(field.span, format!("an `{field_ty}` field is not `readonly`: its methods change it through a `read` borrow"))
                    .hint(format!("keep the field to its module and give a method that reads it: `{} {}(read this) {{ return this.{}.load() }}`", field_ty.strip_borrow().to_string().trim_start_matches("atomic "), field.name, field.name)));
            }
            // `null` and an empty collection are checked here, where they are written, and not
            // only in the literals that leave the field out.
            if let Some(FieldDefault::Expr(written)) = &field.default {
                match &written.kind {
                    ExprKind::Null if !matches!(field_ty, Ty::Nullable(_)) => {
                        return Err(Diag::error(written.span, format!("`{field_ty}` cannot be `null`")).hint(format!("a value that may be missing has the type `{field_ty}?`")));
                    }
                    ExprKind::Collection { ty, .. } => {
                        let made = resolve_type(ty, &names, false)?;
                        if made != field_ty {
                            return Err(Diag::error(written.span, format!("expected `{field_ty}`, found `{made}`")));
                        }
                    }
                    ExprKind::StructLit { name, .. } if !literal_of(name, &field_ty) => {
                        return Err(Diag::error(written.span, format!("expected `{field_ty}`, found `{name}`")));
                    }
                    _ => {}
                }
            }
            fields.push((field.name.clone(), field_ty));
        }
        let taken = fields.iter().map(|(name, _)| name.as_str());
        let methods = type_methods(&decl.name, &decl.methods, taken, "a field", &names, &formattable)?;
        let open = decl.fields.iter().map(|field| field.public).collect();
        let readonly = decl.fields.iter().map(|field| field.readonly).collect();
        let defaults = decl.fields.iter().map(|field| field.default.clone()).collect();
        let said = |interface: &str| decl.interfaces.iter().any(|(name, _)| name == interface);
        let (disposable, closable, opaque) = (said("Disposable"), said("Closable"), decl.opaque);
        let shape = crate::raw::Plain {
            fields: Vec::new(),
            bits: decl.fields.iter().map(|field| field.bits.map(|(width, _)| width as u32)).collect(),
            gaps: decl.layout.gaps.iter().map(|gap| Ok((gap.before, resolve_type(&gap.ty, &names, false)?, gap.bits as u32))).collect::<Res<_>>()?,
            union: decl.layout.union,
            packed: decl.layout.packed.is_some(),
            align: decl.layout.align.map_or(0, |(align, _)| align as usize),
        };
        structs.insert(decl.name.clone(), StructInfo { fields, open, readonly, defaults, methods, disposable, closable, opaque, shape });
    }

    // An enum with numbers that is `Comparable` and writes no `compare` is put in the order of
    // its numbers.
    for decl in prog.enums.iter_mut() {
        if let Some((numbers, _)) = decl.numbers {
            if decl.interfaces.iter().any(|(name, _)| name == "Comparable") && !decl.methods.iter().any(|func| func.name == "compare") {
                decl.methods.push(number_compare(decl, numbers));
            }
        }
    }
    let mut enums: HashMap<String, EnumInfo> = HashMap::new();
    for decl in &prog.enums {
        let mut variants: Vec<(String, Vec<(String, Ty)>)> = Vec::new();
        for variant in &decl.variants {
            if variants.iter().any(|(name, _)| *name == variant.name) {
                return Err(Diag::error(variant.span, format!("variant `{}` is declared twice", variant.name)));
            }
            let mut fields: Vec<(String, Ty)> = Vec::new();
            for field in &variant.fields {
                if fields.iter().any(|(name, _)| *name == field.name) {
                    return Err(Diag::error(field.span, format!("field `{}` is declared twice", field.name)));
                }
                if field.ty.mode.is_some() && decl.sources.is_empty() {
                    return Err(Diag::error(field.ty.span, format!("`{}` borrows, so `{}` names where the borrow comes from", field.name, decl.name))
                        .hint(format!("`enum {}<from source>`", decl.name)));
                }
                let field_ty = resolve_type(&field.ty, &names, false)?;
                if keeps_tasks(&field_ty) {
                    return Err(task_kept(field.ty.span));
                }
                fields.push((field.name.clone(), field_ty));
            }
            variants.push((variant.name.clone(), fields));
        }
        if variants.is_empty() {
            return Err(Diag::error(decl.name_span, format!("enum `{}` has no variants, so it can never have a value", decl.name)));
        }
        let taken = variants.iter().map(|(name, _)| name.as_str());
        let methods = type_methods(&decl.name, &decl.methods, taken, "a variant", &names, &formattable)?;
        // `Block.fromValue(b)` is made by the language for an enum with numbers.
        if let (Some(_), Some(func)) = (decl.numbers, decl.methods.iter().find(|func| func.name == "fromValue")) {
            return Err(Diag::error(func.name_span, format!("`{}` has numbers, and the language gives it `fromValue`", short_name(&decl.name))));
        }
        enums.insert(decl.name.clone(), EnumInfo { variants, methods, numbers: decl.numbers.map(|(ty, _)| ty) });
    }

    // What each type holds by value, to find a type that would have to contain itself.
    let mut members: HashMap<&str, Vec<&Ty>> = HashMap::new();
    for (name, info) in &structs {
        members.insert(name, info.fields.iter().map(|(_, ty)| ty).collect());
    }
    for (name, info) in &enums {
        members.insert(name, info.variants.iter().flat_map(|(_, fields)| fields.iter().map(|(_, ty)| ty)).collect());
    }
    for decl in &prog.structs {
        contains_itself("struct", &decl.name, decl.fields.iter().zip(&structs[&decl.name].fields), &members)?;
    }
    for decl in &prog.enums {
        let fields = decl.variants.iter().flat_map(|variant| &variant.fields);
        let types = enums[&decl.name].variants.iter().flat_map(|(_, fields)| fields);
        contains_itself("enum", &decl.name, fields.zip(types), &members)?;
    }
    // After that check: the layout rules walk the fields a struct holds by value, and would not
    // end on a struct that holds itself.
    layout_rules(prog, &structs)?;
    let layouts = value_layouts(prog, &structs, &enums)?;
    prog.value_layouts = layouts.clone();
    names.layouts = Some(layouts);
    // The signatures of the methods were read before the sizes were known.
    for (decl, methods) in prog.structs.iter().map(|decl| (&decl.name, &decl.methods)).chain(prog.enums.iter().map(|decl| (&decl.name, &decl.methods))) {
        for method in methods {
            func_sig(method, &names, Some(decl))?;
        }
    }

    // A type that holds a borrow, or a value that carries one, names its source; and a type
    // that names a source holds something that borrows.
    let held = prog
        .structs
        .iter()
        .map(|decl| (&decl.name, decl.name_span, "struct", decl.fields.iter().zip(&structs[&decl.name].fields).collect::<Vec<_>>()))
        .chain(prog.enums.iter().map(|decl| {
            let written = decl.variants.iter().flat_map(|variant| &variant.fields);
            let types = enums[&decl.name].variants.iter().flat_map(|(_, fields)| fields);
            (&decl.name, decl.name_span, "enum", written.zip(types).collect::<Vec<_>>())
        }));
    for (name, name_span, kind, fields) in held {
        let lender = fields.iter().find(|(_, (_, ty))| lends(ty, &names));
        match (lender, names.carriers.contains_key(name)) {
            (Some((field, (_, ty))), false) => {
                return Err(Diag::error(field.ty.span, format!("`{}` is a `{ty}`, which carries a borrow, so `{name}` names where the borrow comes from", field.name))
                    .hint(format!("`{kind} {name}<from source>`")));
            }
            (None, true) => {
                return Err(Diag::error(name_span, format!("`{name}` names a source, but nothing in it borrows"))
                    .hint(format!("`{kind} {name}`")));
            }
            _ => {}
        }
    }

    // What a type promises after `:` must hold for everything it carries.
    let sets = Promised { copyable: &copyable, formattable: &formattable, equatable: &equatable, keys: &names.keys, comparable: &comparable, clonable: &names.clonable };
    for decl in &prog.structs {
        let info = &structs[&decl.name];
        let fields = || decl.fields.iter().zip(&info.fields);
        promises(&decl.name, decl.name_span, Ty::Struct(decl.name.clone()), &decl.methods, &info.methods, fields, &sets)?;
    }
    for decl in &prog.enums {
        let info = &enums[&decl.name];
        let fields = || {
            let written = decl.variants.iter().flat_map(|variant| &variant.fields);
            written.zip(info.variants.iter().flat_map(|(_, fields)| fields))
        };
        promises(&decl.name, decl.name_span, Ty::Enum(decl.name.clone()), &decl.methods, &info.methods, fields, &sets)?;
    }

    disposal_rules(prog, &structs, &copyable)?;
    async_method_rules(prog)?;
    default_method_rules(prog)?;
    public_defaults(prog, &structs, &names)?;

    let mut sigs: HashMap<String, FuncSig> = HashMap::new();
    for func in &prog.funcs {
        let short = short_name(&func.name);
        if BUILTINS.contains(&short) {
            return Err(Diag::error(func.name_span, format!("`{short}` is a built-in function and cannot be redefined")));
        }
        if BUILTIN_TYPES.contains(&short) {
            return Err(Diag::error(func.name_span, format!("`{short}` is a built-in type, so a function cannot have its name")));
        }
        if names.kinds.contains_key(func.name.as_str()) {
            return Err(Diag::error(func.name_span, format!("`{}` is already the name of a type", func.name)));
        }
        if sigs.insert(func.name.clone(), func_sig(func, &names, None)?).is_some() {
            return Err(Diag::error(func.name_span, format!("function `{}` is defined twice", func.name)));
        }
    }

    // Raw pointers in fields and signatures point at plain data.
    raw_rule(&structs)?;

    // A function of C takes and gives what C can hold without a copy nobody sees.
    let mut by_c_name: HashMap<&str, &Func> = HashMap::new();
    for func in prog.funcs.iter().filter(|func| func.foreign.is_some()) {
        foreign_sig(func, &sigs[&func.name], &structs)?;
        prog.foreign.insert(func.name.clone(), (sigs[&func.name].params.clone(), sigs[&func.name].ret.clone()));
        if func.variadic {
            prog.variadic.insert(func.name.clone());
        }
        let c_name = short_name(&func.name);
        if let Some(other) = by_c_name.insert(c_name, func) {
            let (mine, theirs) = (&sigs[&func.name], &sigs[&other.name]);
            if mine.params != theirs.params || mine.ret != theirs.ret {
                return Err(Diag::error(func.name_span, format!("the C function `{c_name}` is declared twice, and the two declarations differ"))
                    .related(other.name_span, "the other declaration")
                    .note("C has one function of this name, so it is declared the same way everywhere"));
            }
        }
    }

    // A variable of C holds plain data, which C and SG lay out the same way.
    let mut c_vars: HashMap<&str, (&CVarDecl, Ty)> = HashMap::new();
    for var in &prog.cvars {
        let ty = resolve_type(&var.ty, &names, false)?;
        let c_name = short_name(&var.name);
        if BUILTINS.contains(&c_name) || BUILTIN_TYPES.contains(&c_name) || names.kinds.contains_key(var.name.as_str()) {
            return Err(Diag::error(var.name_span, format!("`{c_name}` is already the name of a type or a built-in function")));
        }
        if ty == Ty::Void || matches!(ty, Ty::Read(_) | Ty::Ref(_)) || not_plain(&ty, &structs).is_some() {
            let diag = Diag::error(var.ty.span, format!("the variable of C `{c_name}` cannot hold a `{ty}`"));
            return Err(match &ty {
                Ty::Char => diag.note("a `char` of SG is a Unicode character of 32 bits, and a `char` of C is a byte").hint("`byte`, or `sbyte` for a C `char` that is signed"),
                Ty::Str => diag.hint("C keeps a text as `raw byte`; `unsafe string.fromCString(p)` copies it"),
                Ty::Struct(name) if structs.get(name).is_some_and(|info| info.opaque) => diag.hint(format!("a type of C seen only behind a pointer is held as `raw {}`", short_name(name))),
                _ => diag.note("a variable of C holds plain data: numbers, `bool`, raw pointers, and arrays and structs of them"),
            });
        }
        if let Some((other, theirs)) = c_vars.get(c_name) {
            if *theirs != ty {
                return Err(Diag::error(var.name_span, format!("the variable of C `{c_name}` is declared twice, and the two declarations differ"))
                    .related(other.name_span, "the other declaration")
                    .note("C has one variable of this name, so it is declared the same way everywhere"));
            }
        }
        c_vars.insert(c_name, (var, ty.clone()));
        names.cvar_types.insert(var.name.clone(), ty.clone());
        prog.cvar_types.insert(var.name.clone(), ty);
    }
    for var in &prog.cvars {
        foreign_lib(&var.foreign)?;
    }
    for (_, foreign) in &prog.blocks {
        foreign_lib(foreign)?;
    }

    // Every alias names a type, also one that nothing uses yet.
    for alias in &prog.aliases {
        let ty = resolve_type(&alias.ty, &names, false)?;
        if alias.public && !module_of(&alias.name).is_empty() {
            exposes(short_name(&alias.name), alias.name_span, &ty, &names)?;
        }
    }

    // A function C calls has a signature of C and a name C can use, one in the whole program.
    let mut exported: HashMap<&str, &Func> = HashMap::new();
    for func in prog.funcs.iter().filter(|func| func.export) {
        let c_name = short_name(&func.name);
        let sig = &sigs[&func.name];
        if c_name == "main" {
            return Err(Diag::error(func.name_span, "`main` is where the program starts, and C does not call it")
                .hint("give the function C calls a name of its own"));
        }
        if C_WORDS.contains(&c_name) {
            return Err(Diag::error(func.name_span, format!("C cannot see a function of SG named `{c_name}`")).note(format!("`{c_name}` is a word of C")));
        }
        if let Some(why) = crate::emit_c::c_name_taken(c_name) {
            return Err(Diag::error(func.name_span, format!("C cannot see a function of SG named `{c_name}`")).note(why));
        }
        if let Some(other) = by_c_name.get(c_name) {
            return Err(Diag::error(func.name_span, format!("`{c_name}` is also the name of a function of C, and C has one function of a name"))
                .related(other.name_span, "the function of C"));
        }
        if let Some((var, _)) = c_vars.get(c_name) {
            return Err(Diag::error(func.name_span, format!("`{c_name}` is also the name of a variable of C, and C has one thing of a name"))
                .related(var.name_span, "the variable of C"));
        }
        if let Some(other) = exported.insert(c_name, func) {
            return Err(Diag::error(func.name_span, format!("C would see two functions named `{c_name}`"))
                .related(other.name_span, "the other one")
                .note("C sees the name of an `export` function as it is written, without its module"));
        }
        for (ty, written) in sig.params.iter().zip(&func.params) {
            if !c_value(ty, &structs) && !handle_param(ty, &structs) {
                let diag = Diag::error(written.ty.span, format!("C calls `{c_name}`, and cannot give it a `{ty}`"));
                return Err(match ty.strip_borrow() {
                    // C gives a text as its `const char*`.
                    Ty::Str | Ty::CString => diag.hint("C gives a text as `read CString`, its `const char*`, which `string.fromCString(text)` copies"),
                    _ => c_value_error(diag, ty, &structs),
                });
            }
        }
        if sig.ret != Ty::Void && !c_value(&sig.ret, &structs) && !handle_owned(&sig.ret, &structs) {
            let diag = Diag::error(func.ret.span, format!("C calls `{c_name}`, and cannot take a `{}` from it", sig.ret));
            return Err(match &sig.ret {
                Ty::Read(inner) | Ty::Ref(inner) if handle_type(inner, &structs) => {
                    diag.note("a borrow ends with the call, and C keeps what it is given").hint(format!("give C the value itself: `{inner}*`"))
                }
                other => c_value_error(diag, other, &structs),
            });
        }
    }

    // A library has no `main`: C starts it, through its `export` functions, which the header
    // names together with the types they take, each under the name it has in SG.
    if prog.library {
        let root_end = end_of_root(prog, src);
        if let Some(main) = prog.funcs.iter().find(|func| func.name == "main") {
            return Err(Diag::error(main.name_span, "a library has no `main`: C calls its `export` functions")
                .hint("a program that starts at `main` is built without `--library` and `--shared`"));
        }
        if exported.is_empty() {
            return Err(Diag::error(Span::new(root_end, root_end), "a library gives C its `export` functions, and this one has none")
                .hint("`export int add(int a, int b) { return a + b }`"));
        }
        header_names(prog, &sigs, &structs, &exported)?;
    }
    if prog.freestanding {
        freestanding_rules(prog, &sigs, exported.is_empty(), src)?;
    }

    for func in prog.funcs.iter().filter(|func| func.export) {
        let sig = &sigs[&func.name];
        prog.exported.insert(func.name.clone(), (sig.params.clone(), sig.ret.clone()));
    }

    // What a module shows to the others is made of what they can see. The root has no name, so
    // nothing can `use` it and `public` says nothing there.
    for func in prog.funcs.iter().filter(|func| func.public && !module_of(&func.name).is_empty()) {
        let sig = &sigs[&func.name];
        for ty in sig.params.iter().chain([&sig.ret]) {
            exposes(short_name(&func.name), func.name_span, ty, &names)?;
        }
    }
    for decl in prog.structs.iter().filter(|decl| decl.public && !module_of(&decl.name).is_empty()) {
        let info = &structs[&decl.name];
        for (field, (_, ty)) in decl.fields.iter().zip(&info.fields).filter(|(field, _)| field.public) {
            exposes(&field.name, field.span, ty, &names)?;
        }
        for method in decl.methods.iter().filter(|method| method.public) {
            let sig = &info.methods[&method.name];
            for ty in sig.params.iter().chain([&sig.ret]) {
                exposes(&method.name, method.name_span, ty, &names)?;
            }
        }
    }
    for decl in prog.enums.iter().filter(|decl| decl.public && !module_of(&decl.name).is_empty()) {
        let types = enums[&decl.name].variants.iter().flat_map(|(_, fields)| fields);
        for (field, (_, ty)) in decl.variants.iter().flat_map(|variant| &variant.fields).zip(types) {
            exposes(short_name(&decl.name), field.span, ty, &names)?;
        }
        for method in decl.methods.iter().filter(|method| method.public) {
            let sig = &enums[&decl.name].methods[&method.name];
            for ty in sig.params.iter().chain([&sig.ret]) {
                exposes(&method.name, method.name_span, ty, &names)?;
            }
        }
    }

    let root_end = end_of_root(prog, src);
    match prog.funcs.iter().find(|func| func.name == "main") {
        None if tests || prog.library || prog.freestanding => {}
        // A program with an `onPanic` is one for a machine without an operating system.
        None if prog.funcs.iter().any(|func| func.export) && prog.funcs.iter().any(|func| func.name == "onPanic") => {
            return Err(Diag::error(Span::new(root_end, root_end), "there is no `main` function")
                .hint("a program for a machine without an operating system, which starts it through its `export` functions, is built with `sg build <folder> --freestanding`"))
        }
        None if prog.funcs.iter().any(|func| func.export) => {
            return Err(Diag::error(Span::new(root_end, root_end), "there is no `main` function")
                .hint("a library that C calls through its `export` functions is built with `sg build <folder> --library`, or `--shared`"))
        }
        None if !prog.tests.is_empty() => {
            return Err(Diag::error(Span::new(root_end, root_end), "there is no `main` function")
                .hint("this file holds tests; they are run by the test command: `sg test <file>`"))
        }
        None => {
            return Err(Diag::error(Span::new(root_end, root_end), "there is no `main` function")
                .hint("a program starts at `void main() { ... }`"))
        }
        Some(main) => {
            let ret_ok = match &sigs["main"].ret {
                Ty::Void => true,
                Ty::Result(ok, _) => **ok == Ty::Void,
                _ => false,
            };
            // The words after the program's name on the command line: `main(List<string> args)`.
            let args_ok = match sigs["main"].params.as_slice() {
                [] => true,
                [Ty::List(item)] => **item == Ty::Str,
                _ => false,
            };
            if !ret_ok || !args_ok {
                return Err(Diag::error(main.name_span, "`main` is written `void main()` or `Result<void, E> main()`")
                    .note("to see the command line: `void main(List<string> args)`; to await, `async` in front"));
            }
            // The error `main` ends with is written out.
            if let Ty::Result(_, error) = &sigs["main"].ret {
                let written = match &**error {
                    Ty::Struct(name) | Ty::Enum(name) => formattable.contains(name),
                    other => can_format(other, &formattable),
                };
                if !written {
                    let diag = Diag::error(main.ret.args[1].span, format!("the error `main` ends with is printed, so `{error}` must be `Formattable`"));
                    return Err(match &**error {
                        Ty::Struct(name) | Ty::Enum(name) => diag.hint(format!("add it to the definition: `{name} : Formattable`")),
                        _ => diag,
                    });
                }
            }
        }
    }

    // The statics: what each holds is the type its function gives. Every task reaches it, for
    // the whole run.
    for decl in &prog.statics {
        let ty = sigs[&decl.init].ret.clone();
        static_type(decl, &ty, &structs, &enums, &names)?;
        if decl.public && !module_of(&decl.name).is_empty() {
            exposes(short_name(&decl.name), decl.name_span, &ty, &names)?;
        }
        names.static_types.insert(decl.name.clone(), ty);
    }

    // The consts: the type of each, then its initializer, checked, then what each holds, found by
    // running the initializers when the program is compiled.
    prog.plain = plain_types(&structs);
    let mut const_table = ConstTable { embeds: prog.embeds.iter().map(|embed| embed.bytes.as_ref().map_or(0, |bytes| bytes.len())).collect(), ..ConstTable::default() };
    // The consts the parser made for the default values of fields and of parameters speak of
    // the field or the parameter.
    let const_of = |default: &Option<FieldDefault>| match default {
        Some(FieldDefault::Const(full)) => Some(full.clone()),
        _ => None,
    };
    let methods = prog.structs.iter().flat_map(|decl| &decl.methods).chain(prog.enums.iter().flat_map(|decl| &decl.methods));
    let defaults: HashMap<String, &str> = prog
        .structs
        .iter()
        .flat_map(|decl| &decl.fields)
        .filter_map(|field| const_of(&field.default).map(|full| (full, "a field")))
        .chain(prog.funcs.iter().chain(methods).flat_map(|func| &func.params).filter_map(|param| const_of(&param.default).map(|full| (full, "a parameter"))))
        .collect();
    let of_field = |decl: &ConstDecl, mut diag: Diag| {
        if let Some(what) = defaults.get(&decl.name) {
            diag.msg = diag.msg.replacen("a const cannot hold a", &format!("the default value of {what} cannot be a"), 1).replacen("a const cannot read it", &format!("the default value of {what} cannot read it"), 1);
            diag.notes = vec![default_note(what)];
        }
        diag
    };
    for decl in &prog.consts {
        let ty = const_type(decl, &names, &structs, &enums, &copyable, &const_table.embeds).map_err(|diag| of_field(decl, diag))?;
        // What a module shows the others is made of what they can see.
        if decl.public && !module_of(&decl.name).is_empty() {
            exposes(short_name(&decl.name), decl.name_span, &ty, &names)?;
        }
        // What a const holds is written into the program: at most 64 MB. Known from the type,
        // before anything runs, except for a text.
        let size = type_bytes(&ty, &structs, &enums);
        if size > crate::EMBED_LIMIT {
            return Err(Diag::error(decl.ty.span, format!("a `{ty}` takes {} MB, and a const takes at most 64 MB", size.div_ceil(1 << 20)))
                .note("a const is written into the program; a larger value is a `static`, found when the program first uses it"));
        }
        const_table.types.insert(decl.name.clone(), ty);
    }
    // The consts the parser made for the numbers of variants.
    let numbers: HashSet<String> = prog.enums.iter().flat_map(|decl| &decl.variants).filter_map(|variant| variant.number.clone()).collect();
    for decl in &mut prog.consts {
        let ty = const_table.types[&decl.name].clone();
        let mut checker = Checker {
            names: &names,
            consts: &const_table,
            in_const: true,
            copyable: &copyable,
            formattable: &formattable,
            equatable: &equatable,
            comparable: &comparable,
            structs: &structs,
            enums: &enums,
            sigs: &sigs,
            scopes: vec![Scope { consts: decl.visible.clone(), ..Scope::default() }],
            locks: Vec::new(),
            borrows: Borrows::default(),
            loops: 0,
            loop_cx: Vec::new(),
            ret: Ty::Void,
            sources: Vec::new(),
            func_name: decl.name.clone(),
            in_test: false,
            is_names: Vec::new(),
            owner: None,
            module: names.module(decl.name_span),
            src,
            is_async: false,
            awaiting: false,
            unsafe_at: None,
            log_guarded: HashSet::new(),
            writing: false,
            written: Vec::new(),
            extern_arg: false,
            seen: crate::unread::Seen::default(),
        };
        let default_of = defaults.get(&decl.name).copied();
        checker.typed(&mut decl.init, &ty).map_err(|diag| match (diag.msg.starts_with("there is no variable named"), default_of) {
            (true, Some(what)) => diag.note(default_note(what)),
            (true, None) => diag.note("a const sees other consts, not variables: its value is found when the program is compiled"),
            (false, Some(what)) if diag.msg.contains("a const cannot read it") => {
                let mut diag = diag;
                diag.msg = diag.msg.replacen("a const cannot read it", &format!("the default value of {what} cannot read it"), 1);
                diag.hint = None;
                diag.note(default_note(what))
            }
            (false, _) => diag,
        })?;
        constant_only(&decl.init, decl.assert.is_some()).map_err(|mut diag| {
            // A const the parser made speaks of what was written.
            if diag.msg == CALLS_NONE {
                match () {
                    _ if default_of.is_some() => {
                        let what = default_of.unwrap_or_default();
                        diag.msg = format!("the default value of {what} calls no function");
                        diag.hint = None;
                        diag.notes = vec![default_note(what)];
                    }
                    _ if numbers.contains(&decl.name) => {
                        diag.msg = "the number of a variant calls no function".to_string();
                        diag.hint = None;
                    }
                    _ if decl.assert.is_some() => {
                        diag.msg = "a `const assert` calls no function".to_string();
                        diag.hint = Some("what a function gives is checked by a `test`, or by `assert` when the program runs".to_string());
                    }
                    _ => {}
                }
            }
            diag
        })?;
    }
    // Every const is found now, before any body: none calls a function (choice 1606).
    let all: HashSet<String> = prog.consts.iter().map(|decl| decl.name.clone()).collect();
    for (name, value) in crate::interp::find_consts(prog, src, &all)? {
        let ty = const_table.types[&name].clone();
        const_table.values.insert(name, (ty, value));
    }
    prog.const_values = const_table.values.clone();
    variant_numbers_hold(prog)?;

    // What gives the value of a static of a block sees the consts and statics of the blocks
    // around it.
    let sees: HashMap<String, Scope> = prog.statics.iter().map(|decl| (decl.init.clone(), Scope { consts: decl.consts.clone(), statics: decl.statics.clone(), ..Scope::default() })).collect();
    let body = |func: &mut Func, sig: &FuncSig, owner: Option<&str>, const_table: &ConstTable| -> Res<crate::unread::Seen> {
        let first = sees.get(&func.name).map_or_else(Scope::default, |scope| Scope { consts: scope.consts.clone(), statics: scope.statics.clone(), ..Scope::default() });
        let mut checker = Checker {
            names: &names,
            consts: const_table,
            in_const: false,
            copyable: &copyable,
            formattable: &formattable,
            equatable: &equatable,
            comparable: &comparable,
            structs: &structs,
            enums: &enums,
            sigs: &sigs,
            scopes: vec![first],
            locks: Vec::new(),
            borrows: Borrows::default(),
            loops: 0,
            loop_cx: Vec::new(),
            ret: sig.ret.clone(),
            sources: sig.sources.clone(),
            func_name: func.name.clone(),
            in_test: false,
            is_names: Vec::new(),
            owner,
            module: names.module(func.name_span),
            src,
            is_async: func.is_async,
            awaiting: false,
            unsafe_at: None,
            log_guarded: HashSet::new(),
            writing: false,
            written: Vec::new(),
            extern_arg: false,
            seen: crate::unread::Seen::default(),
        };
        if let (Some(this), Some(owner)) = (func.this, owner) {
            let ty = match (&sig.holder, names.kinds[owner]) {
                (Some((holder, _)), _) => holder.clone(),
                (None, TypeKind::Struct) => Ty::Struct(owner.to_string()),
                (None, TypeKind::Enum) => Ty::Enum(owner.to_string()),
                (None, TypeKind::Alias) => unreachable!("an alias has no methods"),
            };
            let ty = match this {
                This::Value => ty,
                This::Read => Ty::Read(Box::new(ty)),
                This::Ref => Ty::Ref(Box::new(ty)),
            };
            checker.declare("this", ty, func.name_span)?;
            checker.find("this").expect("just declared").param = true;
        }
        for (param, ty) in func.params.iter().zip(&sig.params) {
            // A default written as it is, `Options {}` above all, is checked once here, where it
            // is written, and not only in the calls that leave the argument out.
            if let Some(FieldDefault::Expr(written)) = &param.default {
                checker.typed(&mut written.clone(), ty.strip_borrow())?;
            }
            if let Some(what) = std_stream(&param.name) {
                return Err(Diag::error(param.span, format!("`{}` is {what} of the program, so a parameter cannot have its name", param.name)));
            }
            checker.not_a_const(&param.name, param.span)?;
            checker.declare(&param.name, ty.clone(), param.span)?;
            checker.find(&param.name).expect("just declared").param = true;
        }
        if func.is_async {
            checker.async_returns(func, &sig.ret)?;
        }
        checker.declare_streams(func.name_span)?;
        checker.block(&mut func.body, &[])?;
        // A writer given to the function by value is closed before its end.
        if !leaves(&func.body, false) {
            checker.unclosed(0, None)?;
        }
        checker.borrow_conflicts()?;
        crate::opening::open(&mut func.body, &|ty: &Ty| lends(ty, &names), src)?;
        if sig.ret != Ty::Void && !leaves(&func.body, false) {
            let diag = Diag::error(
                func.name_span,
                format!("`{}` does not return a value on every path; it must return `{}`", func.name, sig.ret),
            );
            return Err(match &sig.ret {
                Ty::Result(ok, _) if **ok == Ty::Void => diag.hint("success is written too: end with `return Ok()`"),
                _ => diag,
            });
        }
        Ok(checker.seen)
    };
    // Every body, in order: the functions, then the methods of the structs and of the enums.
    let mut places = Vec::new();
    places.extend(prog.funcs.iter().enumerate().filter(|(_, func)| func.foreign.is_none()).map(|(index, _)| Place::Func(index)));
    for (index, decl) in prog.structs.iter().enumerate() {
        places.extend((0..decl.methods.len()).map(|method| Place::Struct(index, method)));
    }
    for (index, decl) in prog.enums.iter().enumerate() {
        places.extend((0..decl.methods.len()).map(|method| Place::Enum(index, method)));
    }
    let owners: Vec<Option<String>> = places.iter().map(|place| place.owner(prog)).collect();
    let place_sigs: Vec<&FuncSig> = places
        .iter()
        .map(|place| match *place {
            Place::Func(index) => &sigs[&prog.funcs[index].name],
            Place::Struct(owner, method) => &structs[&prog.structs[owner].name].methods[&prog.structs[owner].methods[method].name],
            Place::Enum(owner, method) => &enums[&prog.enums[owner].name].methods[&prog.enums[owner].methods[method].name],
        })
        .collect();
    // What each body was found to use, in the order of the bodies, for the rule of values that
    // are never read: it comes last, after every other error.
    let mut seen: Vec<crate::unread::Seen> = Vec::new();
    for (index, place) in places.into_iter().enumerate() {
        seen.push(body(place.func_mut(prog), place_sigs[index], owners[index].as_deref(), &const_table)?);
    }
    const_asserts(prog, &const_table, &structs, src)?;
    if tests {
        seen.extend(check_tests(prog, &names, &const_table, &sets, &structs, &enums, &sigs, src)?);
    }
    thread_rule_of_locked(&structs, &enums, &names)?;
    raw_rule(&structs)?;
    // What lies in raw memory is laid out from the types of the fields.
    prog.plain = plain_types(&structs);
    crate::waits::check(prog)?;
    statics_found(prog, src, &structs, &enums)?;
    patterns_checked(prog, src)?;
    // A program without an operating system is held to the rule of its own.
    if prog.freestanding {
        // The types the program writes in its signatures and its fields.
        let mut declared: Vec<(Span, Ty)> = Vec::new();
        let own = |name: &str| module_of(name) != crate::source::STD_MODULE;
        let signature = |declared: &mut Vec<(Span, Ty)>, func: &Func, sig: &FuncSig| {
            declared.extend(func.params.iter().zip(&sig.params).map(|(param, ty)| (param.ty.span, ty.clone())));
            declared.push((func.ret.span, sig.ret.clone()));
        };
        for func in prog.funcs.iter().filter(|func| func.foreign.is_none() && own(&func.name)) {
            signature(&mut declared, func, &sigs[&func.name]);
        }
        for decl in prog.structs.iter().filter(|decl| own(&decl.name)) {
            let info = &structs[&decl.name];
            declared.extend(decl.fields.iter().zip(&info.fields).map(|(field, (_, ty))| (field.ty.span, ty.clone())));
            for method in &decl.methods {
                signature(&mut declared, method, &info.methods[&method.name]);
            }
        }
        for decl in prog.enums.iter().filter(|decl| own(&decl.name)) {
            let info = &enums[&decl.name];
            for (variant, (_, fields)) in decl.variants.iter().zip(&info.variants) {
                declared.extend(variant.fields.iter().zip(fields).map(|(field, (_, ty))| (field.ty.span, ty.clone())));
            }
            for method in &decl.methods {
                signature(&mut declared, method, &info.methods[&method.name]);
            }
        }
        prog.left_out = crate::freestanding::check(prog, &declared)?;
    }
    let bodies = prog.funcs.iter().filter(|func| func.foreign.is_none()).map(|func| &func.body);
    let methods = prog.structs.iter().flat_map(|decl| &decl.methods).chain(prog.enums.iter().flat_map(|decl| &decl.methods)).map(|method| &method.body);
    let tested = prog.tests.iter().map(|test| &test.body).take(if tests { usize::MAX } else { 0 });
    for (body, seen) in bodies.chain(methods).chain(tested).zip(&seen) {
        crate::unread::check(body, seen, &|ty: &Ty| disposes(ty, &structs), src)?;
    }
    // A count of an array found from the text of a const agrees with what the const holds.
    for (at, (count, full)) in SIZES.with(|sizes| sizes.borrow().clone()) {
        if !matches!(prog.const_values.get(&full), Some((_, ConstVal::Int(_, held))) if *held == count as i128) {
            return Err(Diag::error(Span::new(at, at + 1), format!("the count of this array was found as {count} before the types, and the const holds another value"))
                .note("the count of an array is found from integer consts and operators, before the types are known"));
        }
        prog.sizes.insert(at, count);
    }
    // A static whose value was found here needs no function to find it.
    let found: HashSet<String> = prog.statics.iter().filter(|decl| decl.value.is_some()).map(|decl| decl.init.clone()).collect();
    prog.funcs.retain(|func| !found.contains(&func.name));
    Ok(())
}

/// Each pattern written in the program for a `Regex`, read by the language's own module when the
/// program is compiled (`Regex.problem`, as a const is found): a pattern that is not a regular
/// expression is a compile error (choices 1463, 1469).
fn patterns_checked(prog: &Program, src: &str) -> Res<()> {
    let found = PATTERNS.with(|found| std::mem::take(&mut *found.borrow_mut()));
    for (span, text) in found.into_values() {
        let ty = Ty::Read(Box::new(Ty::Str));
        let pattern = Expr { kind: ExprKind::Str(text), span, ty, moved: false };
        let recv = Expr { kind: ExprKind::Var(STD_MODULE.to_string()), span, ty: Ty::Void, moved: false };
        let callee = Callee::Static { owner: format!("{STD_MODULE}.Regex"), name: "problem".to_string() };
        let call = ExprKind::Method { recv: Box::new(recv), name: "problem".to_string(), name_span: span, args: vec![pattern], callee };
        let init = Expr { kind: call, span, ty: Ty::Str, moved: false };
        let text_type = TypeExpr { mode: None, holder: None, checked: false, locked: false, atomic: None, raw: 0, name: "string".to_string(), args: Vec::new(), suffixes: Vec::new(), span };
        let decl = ConstDecl { public: false, name: format!("pattern#{}", span.start), name_span: span, ty: text_type, init, local: None, visible: Vec::new(), assert: None };
        match crate::interp::run_std(prog, src, &prog.const_values, &decl)? {
            ConstVal::Str(problem) if problem.is_empty() => {}
            ConstVal::Str(problem) => {
                return Err(Diag::error(span, format!("this pattern is not a regular expression: {problem}"))
                    .note("a pattern written in the program is checked when the program is compiled"));
            }
            _ => unreachable!("`Regex.problem` gives a text"),
        }
    }
    Ok(())
}

/// The statics, once every body is checked: the value of one given what the rule of a const
/// allows, of a type a const can hold (atomic and `locked` too), is found now and lies in the
/// program from its start; any other is found the first time the static is used, which must
/// not need the static itself, through the functions it calls and the statics they read.
fn statics_found(prog: &mut Program, src: &str, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>) -> Res<()> {
    let shown = |full: &str| full.split('#').next().map(|name| short_name(name).to_string()).unwrap_or_default();
    let given = |decl: &StaticDecl, prog: &Program| -> Expr {
        let func = prog.funcs.iter().find(|func| func.name == decl.init).expect("every static has the function of its value");
        match &func.body[0].kind {
            StmtKind::Return(Some(value)) => value.clone(),
            _ => unreachable!("the function of a static returns what it is given"),
        }
    };
    let mut found = Vec::new();
    for (position, decl) in prog.statics.iter().enumerate() {
        let value = given(decl, prog);
        if !plain_constant(&value) || !const_able(&value.ty, structs, enums, &mut Vec::new()) {
            continue;
        }
        let as_const = ConstDecl { public: false, name: decl.name.clone(), name_span: decl.name_span, ty: decl.ty.clone(), init: value, local: decl.local.clone(), visible: Vec::new(), assert: None };
        found.push((position, crate::interp::find_value(prog, src, &prog.const_values, &as_const)?));
    }
    for (position, value) in found {
        prog.statics[position].value = Some(value);
    }
    // What each static found when it is first used reads on the way, through the calls.
    let mut bodies: HashMap<String, &Func> = prog.funcs.iter().filter(|func| func.foreign.is_none()).map(|func| (func.name.clone(), func)).collect();
    for (owner, methods) in prog.structs.iter().map(|decl| (&decl.name, &decl.methods)).chain(prog.enums.iter().map(|decl| (&decl.name, &decl.methods))) {
        bodies.extend(methods.iter().map(|method| (format!("{owner}.{}", method.name), method)));
    }
    let mut reads: HashMap<&str, Vec<(String, Span)>> = HashMap::new();
    for decl in prog.statics.iter().filter(|decl| decl.value.is_none()) {
        reads.insert(&decl.name, crate::static_reads::statics_read(&given(decl, prog), &bodies));
    }
    // A static that needs itself, around any number of others.
    fn around<'p>(name: &'p str, reads: &HashMap<&'p str, Vec<(String, Span)>>, path: &mut Vec<&'p str>, done: &mut HashSet<&'p str>) -> Option<Vec<&'p str>> {
        if let Some(at) = path.iter().position(|each| *each == name) {
            let mut circle = path[at..].to_vec();
            circle.push(name);
            return Some(circle);
        }
        if !done.insert(name) {
            return None;
        }
        path.push(name);
        for (read, _) in reads.get(name).into_iter().flatten() {
            let Some((key, _)) = reads.get_key_value(read.as_str()) else { continue };
            if let Some(circle) = around(key, reads, path, done) {
                return Some(circle);
            }
        }
        path.pop();
        None
    }
    let mut done = HashSet::new();
    for decl in prog.statics.iter().filter(|decl| decl.value.is_none()) {
        if let Some(circle) = around(&decl.name, &reads, &mut Vec::new(), &mut done) {
            let first = prog.statics.iter().find(|each| each.name == circle[0]).expect("a static of the program");
            let at = reads[circle[0]].iter().find(|(read, _)| read == circle[1]).map(|(_, span)| *span).expect("read on the way");
            let path: Vec<String> = circle.iter().map(|name| format!("`{}`", shown(name))).collect();
            return Err(Diag::error(first.name_span, format!("the value of `{}` needs itself: {}", shown(circle[0]), path.join(" -> ")))
                .related(at, format!("`{}` is read here, on the way to its value", shown(circle[1])))
                .note("the value of a static is found the first time it is used, by running what it is given and every function that calls, so it cannot need the static itself"));
        }
    }
    Ok(())
}

/// What the value of a static is given is known when the program is compiled, by the rule of a
/// const that calls no function: numbers, other consts, operators, casts, and arrays, structs,
/// enums and atomic values of them.
fn plain_constant(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Int(..) | ExprKind::Float(..) | ExprKind::Bool(_) | ExprKind::Char(_) | ExprKind::Const(_) | ExprKind::Constant { .. } => true,
        ExprKind::Unary(_, inner) | ExprKind::Cast { expr: inner, .. } | ExprKind::Widen(_, inner) | ExprKind::Load(inner) | ExprKind::Atomic(inner) => plain_constant(inner),
        ExprKind::Binary(_, left, right) | ExprKind::Index { recv: left, index: right, .. } => plain_constant(left) && plain_constant(right),
        ExprKind::Field { recv, .. } => plain_constant(recv),
        ExprKind::Array { args, items, .. } => args.iter().chain(items.iter().flatten()).all(plain_constant),
        ExprKind::StructLit { fields, .. } => fields.iter().all(|field| plain_constant(&field.value)),
        ExprKind::Variant { args, .. } => args.iter().all(plain_constant),
        _ => false,
    }
}

/// A type whose value a const can hold, atomic values of numbers and of `bool` inside it too.
/// Whether a value of `ty` holds a `Disposable` value by value anywhere: in a field, an item, a
/// variant or a `T?` (choice 1592). Its `dispose` runs where such a value ends.
fn holds_disposable(ty: &Ty, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>, seen: &mut Vec<String>) -> bool {
    match ty {
        Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) | Ty::Array(inner, _) => holds_disposable(inner, structs, enums, seen),
        Ty::Map(key, value) | Ty::Result(key, value) => holds_disposable(key, structs, enums, seen) || holds_disposable(value, structs, enums, seen),
        Ty::Struct(name) | Ty::Enum(name) => {
            if seen.contains(name) {
                return false;
            }
            seen.push(name.clone());
            let found = match (structs.get(name), enums.get(name)) {
                (Some(info), _) => info.disposable || info.fields.iter().any(|(_, field)| holds_disposable(field, structs, enums, seen)),
                (_, Some(info)) => info.variants.iter().flat_map(|(_, fields)| fields).any(|(_, field)| holds_disposable(field, structs, enums, seen)),
                _ => false,
            };
            seen.pop();
            found
        }
        _ => false,
    }
}

fn const_able(ty: &Ty, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>, seen: &mut Vec<String>) -> bool {
    match ty {
        Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char => true,
        Ty::Atomic(inner) => matches!(**inner, Ty::Int(_) | Ty::Float(_) | Ty::Bool),
        Ty::Array(item, _) => const_able(item, structs, enums, seen),
        Ty::Struct(name) | Ty::Enum(name) => {
            if seen.contains(name) {
                return false;
            }
            seen.push(name.clone());
            let able = match (structs.get(name), enums.get(name)) {
                (Some(info), _) => !info.opaque && info.fields.iter().all(|(_, field)| const_able(field, structs, enums, seen)),
                (_, Some(info)) => info.variants.iter().flat_map(|(_, fields)| fields).all(|(_, field)| const_able(field, structs, enums, seen)),
                _ => false,
            };
            seen.pop();
            able
        }
        _ => false,
    }
}

/// Where a body lies in the program: a function, or a method of a struct or of an enum.
#[derive(Clone, Copy)]
enum Place {
    Func(usize),
    Struct(usize, usize),
    Enum(usize, usize),
}

impl Place {
    fn func_mut(self, prog: &mut Program) -> &mut Func {
        match self {
            Place::Func(index) => &mut prog.funcs[index],
            Place::Struct(owner, method) => &mut prog.structs[owner].methods[method],
            Place::Enum(owner, method) => &mut prog.enums[owner].methods[method],
        }
    }

    fn owner(self, prog: &Program) -> Option<String> {
        match self {
            Place::Func(_) => None,
            Place::Struct(owner, _) => Some(prog.structs[owner].name.clone()),
            Place::Enum(owner, _) => Some(prog.enums[owner].name.clone()),
        }
    }

}

/// Whether a `const assert` measures a type (`Memory.sizeOf`, `Memory.alignOf`) or reads a
/// limit of `nint` or `nuint`: what differs from machine to machine, which compiled code
/// checks again with the compiler of C.
pub fn measures_type(expr: &Expr) -> bool {
    let mut found = match &expr.kind {
        ExprKind::SizeOf { .. } => true,
        ExprKind::Constant { ty: Ty::Int(int), .. } => matches!(int, IntTy::Nint | IntTy::Nuint),
        _ => false,
    };
    crate::sums::children(expr, &mut |child| found |= measures_type(child));
    found
}

/// Each `const assert` holds, or is a compile error that says what it was found to be: for a
/// comparison, the value of each side too.
fn const_asserts(prog: &Program, table: &ConstTable, structs: &HashMap<String, StructInfo>, src: &str) -> Res<()> {
    for decl in &prog.consts {
        let Some(assert) = &decl.assert else { continue };
        if matches!(table.values.get(&decl.name), Some((_, ConstVal::Bool(true)))) || (prog.other_machine && measures_type(&decl.init)) {
            continue;
        }
        let mut diag = Diag::error(decl.init.span, match &assert.message {
            Some(message) => format!("const assert failed: {message}"),
            None => format!("const assert failed: {}", &src[decl.init.span.start..decl.init.span.end]),
        });
        if let ExprKind::Binary(op, left, right) = &decl.init.kind {
            if matches!(op, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge) {
                for (side, value) in [("left: ", left), ("right:", right)] {
                    let as_const = ConstDecl { public: false, name: format!("{}#{}", decl.name, side.trim_end_matches([':', ' '])), name_span: decl.name_span, ty: decl.ty.clone(), init: (**value).clone(), local: decl.local.clone(), visible: decl.visible.clone(), assert: None };
                    let found = crate::interp::find_value(prog, src, &table.values, &as_const)?;
                    diag = diag.note(format!("{side} {}", const_text(&found, structs)));
                }
            }
        }
        return Err(diag);
    }
    Ok(())
}

/// A value of a const as a program writes it, for the message of a `const assert`.
fn const_text(value: &ConstVal, structs: &HashMap<String, StructInfo>) -> String {
    let items = |items: &[ConstVal]| items.iter().map(|item| const_text(item, structs)).collect::<Vec<_>>().join(", ");
    match value {
        ConstVal::Int(_, n) => n.to_string(),
        ConstVal::Float(_, x) => x.to_string(),
        ConstVal::Bool(b) => b.to_string(),
        ConstVal::Char(c) => format!("{c:?}"),
        ConstVal::Str(text) => format!("{text:?}"),
        ConstVal::Array(values) => format!("{{ {} }}", items(values)),
        ConstVal::Bytes(bytes) => format!("{{ {} }}", bytes.iter().map(|byte| byte.to_string()).collect::<Vec<_>>().join(", ")),
        ConstVal::Struct(name, values) => {
            let fields = structs.get(name).map(|info| info.fields.iter().zip(values).map(|((field, _), value)| format!("{field} = {}", const_text(value, structs))).collect::<Vec<_>>().join(", ")).unwrap_or_else(|| items(values));
            format!("{} {{ {fields} }}", short_name(name))
        }
        ConstVal::Enum(name, _, values) if values.is_empty() => short_name(name).to_string(),
        ConstVal::Enum(name, _, values) => format!("{}({})", short_name(name), items(values)),
    }
}

/// The most bytes a value of one type takes. As in Rust, the largest object whose addresses
/// pointer arithmetic still counts right (`isize`), cut to what Clang accepts for an array on
/// a machine of 64 bits. A machine of 32 bits, once one is supported, would have 2^31 - 1.
pub const VALUE_BYTES: u64 = (1 << 61) - 1;

/// The size and the alignment of a value of `ty`, at most what C gives it (a field of bits
/// counted as its whole type, a tag as eight bytes); `None` past `VALUE_BYTES`. What lies
/// on the heap (a text, a list, a `shared T`) counts as the handle a value keeps.
pub(crate) fn value_layout(ty: &Ty, layouts: &HashMap<String, (u64, u64)>) -> Option<(u64, u64)> {
    let found = match ty {
        Ty::Int(int) if int.is_c() => (8, 8),
        Ty::Int(int) => (u64::from(int.bits() / 8), u64::from(int.bits() / 8)),
        Ty::Float(FloatTy::Float) | Ty::Char => (4, 4),
        Ty::Float(_) => (8, 8),
        Ty::Bool => (1, 1),
        Ty::Void | Ty::Never | Ty::Null => (0, 1),
        Ty::Array(item, count) => {
            let (size, align) = value_layout(item, layouts)?;
            (size.checked_mul(*count as u64)?, align)
        }
        Ty::Struct(name) | Ty::Enum(name) => *layouts.get(name).unwrap_or(&(0, 1)),
        Ty::Atomic(inner) => {
            let (size, _) = value_layout(inner, layouts)?;
            (size, size.max(1))
        }
        Ty::Nullable(inner) if matches!(**inner, Ty::Ptr(_) | Ty::Raw(_)) => (8, 8),
        // The value and a `bool` beside it.
        Ty::Nullable(inner) | Ty::Locked(inner) => {
            let (size, align) = value_layout(inner, layouts)?;
            (size.checked_add(align.max(8))?, align.max(8))
        }
        Ty::Result(ok, error) => {
            let (ok_size, ok_align) = value_layout(ok, layouts)?;
            let (error_size, error_align) = value_layout(error, layouts)?;
            (ok_size.max(error_size).checked_add(8)?, ok_align.max(error_align).max(8))
        }
        Ty::Entry(key, value) => {
            let (key_size, key_align) = value_layout(key, layouts)?;
            let (value_size, value_align) = value_layout(value, layouts)?;
            (key_size.checked_add(value_size)?.checked_add(key_align.max(value_align))?, key_align.max(value_align))
        }
        _ => (32, 8),
    };
    (found.0 <= VALUE_BYTES).then_some(found)
}

/// A type whose values would take more than `VALUE_BYTES` is an error.
fn too_big(ty: &Ty, layouts: &HashMap<String, (u64, u64)>, at: Span) -> Res<()> {
    match value_layout(ty, layouts) {
        Some(_) => Ok(()),
        None => Err(Diag::error(at, format!("a `{ty}` is too big: a value takes at most {} bytes", crate::interp::group_digits(VALUE_BYTES)))
            .note("that is the largest value whose addresses a pointer can count on a machine of 64 bits")
            .hint(match ty.strip_borrow() {
                Ty::Array(item, _) => format!("a `List<{item}>` keeps its items on the heap, as many as the memory holds"),
                _ => "keep the large part on the heap, in a `List<T>`".to_string(),
            })),
    }
}

/// The layout of every struct and enum, after the check that none holds itself. One that
/// would take more than `VALUE_BYTES` is an error at its name.
fn value_layouts(prog: &Program, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>) -> Res<HashMap<String, (u64, u64)>> {
    fn measure(name: &str, prog: &Program, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>, layouts: &mut HashMap<String, (u64, u64)>) -> Option<(u64, u64)> {
        if let Some(found) = layouts.get(name) {
            return Some(*found);
        }
        // What a type holds by value is measured first.
        let fields: Vec<&Ty> = match (structs.get(name), enums.get(name)) {
            (Some(info), _) => info.fields.iter().map(|(_, ty)| ty).collect(),
            (_, Some(info)) => info.variants.iter().flat_map(|(_, fields)| fields.iter().map(|(_, ty)| ty)).collect(),
            _ => Vec::new(),
        };
        for field in &fields {
            for other in by_value(field) {
                measure(other, prog, structs, enums, layouts)?;
            }
        }
        let round = |size: u64, align: u64| size.checked_next_multiple_of(align.max(1));
        let found = match (structs.get(name), enums.get(name)) {
            (Some(_), _) => {
                let decl = prog.structs.iter().find(|decl| decl.name == name).expect("a struct the checker knows");
                let packed = decl.layout.packed.is_some();
                let mut size = 0u64;
                let mut align = decl.layout.align.map_or(1, |(align, _)| align);
                for field in &fields {
                    let (field_size, field_align) = value_layout(field, layouts)?;
                    let field_align = if packed { 1 } else { field_align };
                    align = align.max(field_align);
                    size = match decl.layout.union {
                        true => size.max(field_size),
                        false => round(size, field_align)?.checked_add(field_size)?,
                    };
                }
                // The bits left empty, each counted as its whole type.
                size = size.checked_add(8u64.checked_mul(decl.layout.gaps.len() as u64)?)?;
                (round(size, align)?, align)
            }
            (_, Some(info)) => {
                let mut payload = 0u64;
                let mut align = 8u64;
                for (_, variant) in &info.variants {
                    let mut size = 0u64;
                    for (_, field) in variant {
                        let (field_size, field_align) = value_layout(field, layouts)?;
                        align = align.max(field_align);
                        size = round(size, field_align)?.checked_add(field_size)?;
                    }
                    payload = payload.max(size);
                }
                (round(payload.checked_add(8)?, align)?, align)
            }
            _ => (0, 1),
        };
        (found.0 <= VALUE_BYTES).then(|| {
            layouts.insert(name.to_string(), found);
            found
        })
    }
    let mut layouts = HashMap::new();
    let named = prog.structs.iter().filter(|decl| !decl.opaque).map(|decl| (&decl.name, decl.name_span)).chain(prog.enums.iter().map(|decl| (&decl.name, decl.name_span)));
    for (name, span) in named {
        if measure(name, prog, structs, enums, &mut layouts).is_none() {
            // A field too big on its own is shown where it is written.
            let written = prog.structs.iter().filter(|decl| &decl.name == name).flat_map(|decl| decl.fields.iter().zip(&structs[name].fields))
                .chain(prog.enums.iter().filter(|decl| &decl.name == name).flat_map(|decl| decl.variants.iter().flat_map(|variant| &variant.fields)).zip(enums.get(name).into_iter().flat_map(|info| info.variants.iter().flat_map(|(_, fields)| fields))));
            for (field, (_, ty)) in written {
                too_big(ty, &layouts, field.ty.span)?;
            }
            return Err(Diag::error(span, format!("`{}` is too big: a value takes at most {} bytes", short_name(name), crate::interp::group_digits(VALUE_BYTES)))
                .note("that is the largest value whose addresses a pointer can count on a machine of 64 bits")
                .hint("keep the large part on the heap, in a `List<T>`"));
        }
    }
    Ok(layouts)
}

/// How many bytes a value of the type of a const takes in the program, a text not counted.
fn type_bytes(ty: &Ty, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>) -> u64 {
    match ty {
        Ty::Int(int) => u64::from(int.bits() / 8),
        Ty::Float(FloatTy::Float) | Ty::Char => 4,
        Ty::Float(_) => 8,
        Ty::Bool => 1,
        Ty::Array(item, count) => type_bytes(item, structs, enums).saturating_mul(*count as u64),
        Ty::Struct(name) => structs[name].fields.iter().map(|(_, field)| type_bytes(field, structs, enums)).fold(0, u64::saturating_add),
        Ty::Enum(name) => 8 + enums[name].variants.iter().map(|(_, fields)| fields.iter().map(|(_, field)| type_bytes(field, structs, enums)).fold(0, u64::saturating_add)).max().unwrap_or(0),
        _ => 0,
    }
}

/// Whether a value is reached through a `shared checked` one, which the checker unboxes or opens.
fn through_checked(expr: &Expr) -> bool {
    match &expr.kind {
        _ if matches!(expr.ty.strip_borrow(), Ty::Shared(_, true)) => true,
        ExprKind::Unbox(inner) | ExprKind::Load(inner) | ExprKind::Open { inner, .. } => through_checked(inner),
        _ => false,
    }
}

/// Where what is missing from a program (`main`, an `export`, `onPanic`) is told: the end of the
/// last line of the root file that has something on it, not the empty line after it (choice
/// 1401).
fn end_of_root(prog: &Program, src: &str) -> usize {
    let Some(file) = prog.files.first() else { return src.len() };
    src[..file.end].trim_end().len().max(file.start)
}

/// What a program built without an operating system has: no `main`, since the code that starts
/// the machine calls its `export` functions, of which it has one at least; an `onPanic`, which
/// a panic goes to; and, to allocate, an `allocate` with its `release`. All three are written in
/// the root module, with these signatures.
fn freestanding_rules(prog: &Program, sigs: &HashMap<String, FuncSig>, no_exports: bool, src: &str) -> Res<()> {
    let root_end = end_of_root(prog, src);
    let find = |name: &str| prog.funcs.iter().find(|func| func.name == name && func.foreign.is_none());
    if let Some(main) = find("main") {
        return Err(Diag::error(main.name_span, "a program without an operating system has no `main`: the code that starts the machine calls its `export` functions")
            .hint("name the function the start code calls, and write `export` in front of it: `export void start() { ... }`"));
    }
    if no_exports {
        return Err(Diag::error(Span::new(root_end, root_end), "a program without an operating system is started through its `export` functions, and this one has none")
            .hint("`export void start() { ... }`, called by the code that starts the machine"));
    }
    let byte = Ty::Raw(Box::new(Ty::Int(IntTy::Byte)));
    let nuint = Ty::Int(IntTy::Nuint);
    let wanted = [
        ("onPanic", vec![Ty::Read(Box::new(Ty::Str))], Ty::Void, "void onPanic(read string message)"),
        ("allocate", vec![nuint.clone(), nuint.clone()], byte.clone(), "raw byte allocate(nuint size, nuint align)"),
        ("release", vec![byte, nuint.clone(), nuint], Ty::Void, "void release(raw byte block, nuint size, nuint align)"),
    ];
    for (name, params, ret, written) in &wanted {
        let Some(func) = find(name) else { continue };
        let sig = &sigs[*name];
        if func.is_async || func.export || sig.params != *params || sig.ret != *ret {
            let diag = Diag::error(func.name_span, format!("`{name}` is written `{written}`"));
            return Err(match *name {
                "onPanic" => diag.note("a panic gives `onPanic` its message; the processor waits for ever after it"),
                _ => diag.note("`allocate` and `release` are where a program without an operating system takes memory and gives it back: `release` is told the size and the alignment `allocate` was asked for"),
            });
        }
    }
    if find("onPanic").is_none() {
        return Err(Diag::error(Span::new(root_end, root_end), "a program without an operating system has an `onPanic` function, which a panic goes to")
            .hint("`void onPanic(read string message) { ... }`: it writes the message where it can be seen; the processor waits for ever after it"));
    }
    match (find("allocate"), find("release")) {
        (Some(func), None) => Err(Diag::error(func.name_span, "`allocate` has no `release`, which gives the memory back")
            .hint("`void release(raw byte block, nuint size, nuint align) { ... }`")),
        (None, Some(func)) => Err(Diag::error(func.name_span, "`release` has no `allocate`, which gives the memory")
            .hint("`raw byte allocate(nuint size, nuint align) { ... }`")),
        _ => Ok(()),
    }
}

/// Whether this is a raw pointer to a type of C declared in an `extern` block.
fn points_at_opaque(ty: &Ty, structs: &HashMap<String, StructInfo>) -> bool {
    matches!(ty.strip_borrow(), Ty::Raw(item) if matches!(&**item, Ty::Struct(name) if structs.get(name).is_some_and(|info| info.opaque)))
}

/// Whether a local of this type is closed before it ends: `None` for a `StreamWriter`, the
/// short name of a struct written with `: Closable`.
/// A `Process` of the language's own module: a program that runs until it is waited for.
fn is_process(ty: &Ty) -> bool {
    matches!(ty, Ty::Struct(name) if name == PROCESS)
}

const PROCESS: &str = "std.Process";

fn closable<'s>(ty: &'s Ty, structs: &HashMap<String, StructInfo>) -> Option<Option<&'s str>> {
    match ty {
        Ty::Writer => Some(None),
        Ty::Struct(name) if structs.get(name).is_some_and(|info| info.closable) => Some(Some(short_name(name))),
        _ => None,
    }
}

/// `: Disposable` and `: Closable` on a struct of the program: `dispose` is the one method that
/// runs when the last owner ends, written as `void dispose(ref this)`, and only there; `close`
/// takes the value and says how closing went.
fn disposal_rules(prog: &Program, structs: &HashMap<String, StructInfo>, copyable: &HashSet<String>) -> Res<()> {
    for decl in &prog.structs {
        let info = &structs[&decl.name];
        let short = short_name(&decl.name);
        let said = |interface: &str| decl.interfaces.iter().find(|(name, _)| name == interface).map(|(_, span)| *span);
        let dispose = decl.methods.iter().find(|method| method.name == "dispose");
        match (said("Disposable"), dispose) {
            (Some(span), None) => {
                return Err(Diag::error(span, format!("`{short}` is `Disposable`, so it says what is done when it ends"))
                    .hint("a method `void dispose(ref this) { ... }`"));
            }
            (None, Some(method)) => {
                return Err(Diag::error(method.name_span, "`dispose` runs by itself only on a `Disposable` type, and it is called by nothing else")
                    .hint(format!("`struct {short} : Disposable`, or another name for the method")));
            }
            (Some(_), Some(method)) => {
                let sig = &info.methods["dispose"];
                let shaped = sig.this == Some(This::Ref) && sig.holder.is_none() && sig.params.is_empty() && sig.ret == Ty::Void && !sig.is_async;
                if !shaped {
                    let diag = Diag::error(method.name_span, "`dispose` is written `void dispose(ref this)`: it takes the value it ends and nothing else, and gives nothing");
                    return Err(match (sig.is_async, sig.ret == Ty::Void) {
                        (true, _) => diag.note("it runs where the value ends, which may be in code that is not async, so it does not await"),
                        (false, false) => diag.note("nobody calls it, so there is nobody to give a value or an error to; a `close(this)` that gives a `Result` can, with `: Closable`"),
                        _ => diag,
                    });
                }
                if copyable.contains(&decl.name) {
                    let span = said("Copyable").expect("it is copyable");
                    return Err(Diag::error(span, format!("`{short}` cannot be both `Copyable` and `Disposable`: each copy would be disposed of"))
                        .note("a copy of a value that gives something back when it ends would give it back twice"));
                }
            }
            (None, None) => {}
        }
        if let Some(span) = said("Closable") {
            let close = info.methods.get("close");
            let shaped = close.is_some_and(|sig| sig.this == Some(This::Value) && sig.holder.is_none() && matches!(sig.ret, Ty::Result(..)) && !sig.is_async);
            if !shaped {
                let diag = Diag::error(span, format!("`{short}` is `Closable`, so it has a `close` that takes the value and says how closing went"));
                return Err(diag.hint(format!("`Result<void, E> close(this) {{ ... }}`, with the error type `{short}` gives")));
            }
        }
    }
    Ok(())
}

/// The methods the language calls by itself are not async: it calls them where it does not
/// await (`T(...)`, `print`, `==`, `sort`, a key of a map). `dispose` and `close` say so where
/// their shapes are checked.
fn async_method_rules(prog: &Program) -> Res<()> {
    let typed = prog.structs.iter().map(|decl| (&decl.interfaces, &decl.methods)).chain(prog.enums.iter().map(|decl| (&decl.interfaces, &decl.methods)));
    for (interfaces, methods) in typed {
        let said = |interface: &str| interfaces.iter().any(|(name, _)| name == interface);
        for method in methods.iter().filter(|method| method.is_async) {
            let used_by = match method.name.as_str() {
                "create" => "`T(...)`",
                "format" if said("Formattable") => "`print` and `$\"...\"`",
                "equals" if said("Equatable") => "`==` and `!=`",
                "hash" if said("Hashable") => "a `HashMap` and a `HashSet`",
                "compare" if said("Comparable") => "`<` and `sort`",
                _ => continue,
            };
            let diag = Diag::error(method.name_span, format!("`{}` is called by {used_by} without `await`, so it cannot be async", method.name));
            return Err(match method.name.as_str() {
                "create" => diag.hint("an async function of the type gives one when it is awaited: `public async T load(...)`, then `await T.load(...)`"),
                _ => diag,
            });
        }
    }
    Ok(())
}

/// The methods the language calls by itself are given every argument, so none of their
/// parameters has a default value (choice 1622).
fn default_method_rules(prog: &Program) -> Res<()> {
    let typed = prog.structs.iter().map(|decl| (&decl.interfaces, &decl.methods)).chain(prog.enums.iter().map(|decl| (&decl.interfaces, &decl.methods)));
    for (interfaces, methods) in typed {
        let said = |interface: &str| interfaces.iter().any(|(name, _)| name == interface);
        for method in methods {
            let Some(param) = method.params.iter().find(|param| param.default.is_some()) else { continue };
            let used_by = match method.name.as_str() {
                "format" if said("Formattable") => "`print` and `$\"...\"`",
                "equals" if said("Equatable") => "`==` and `!=`",
                "hash" if said("Hashable") => "a `HashMap` and a `HashSet`",
                "compare" if said("Comparable") => "`<` and `sort`",
                "dispose" if said("Disposable") => "the end of the value",
                _ => continue,
            };
            return Err(Diag::error(param.span, format!("`{}` is called by {used_by}, which gives it every argument, so `{}` has no default value", method.name, param.name)));
        }
    }
    Ok(())
}

/// A public function's default `T {}` is built by the calls of other modules, so every field of
/// `T` is open to them; found before any body, since a call may be checked before the function.
fn public_defaults(prog: &Program, structs: &HashMap<String, StructInfo>, names: &Names) -> Res<()> {
    let methods = prog.structs.iter().flat_map(|decl| &decl.methods).chain(prog.enums.iter().flat_map(|decl| &decl.methods));
    for func in prog.funcs.iter().chain(methods).filter(|func| func.public) {
        for param in &func.params {
            let Some(FieldDefault::Expr(written)) = &param.default else { continue };
            if !matches!(written.kind, ExprKind::StructLit { .. }) {
                continue;
            }
            let built = match resolve_type(&param.ty, names, false)?.strip_borrow().clone() {
                Ty::Nullable(inner) => *inner,
                held => held,
            };
            let Ty::Struct(full) = built else { continue };
            let Some(info) = structs.get(&full) else { continue };
            if let Some(position) = info.open.iter().position(|open| !open) {
                return Err(Diag::error(written.span, format!("`{}` is public, and a call of another module cannot build a `{}` field by field: `{}` is not public", short_name(&func.name), short_name(&full), info.fields[position].0))
                    .note("a default value is put into the call that leaves the argument out, as if it were written there"));
            }
        }
    }
    Ok(())
}

/// What a static may hold: every task reaches it, so it follows the thread rule, and it is never
/// dropped, so nothing in it is to be closed or disposed of.
fn static_type(decl: &StaticDecl, ty: &Ty, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>, names: &Names) -> Res<()> {
    let span = decl.ty.span;
    if matches!(ty, Ty::Void | Ty::Never) {
        return Err(Diag::error(span, "a static holds a value"));
    }
    if decl.locked && matches!(ty, Ty::Atomic(_)) {
        return Err(Diag::error(span, "`static locked` holds the value itself, which is reached inside a `lock` block"));
    }
    if let Some((part, at)) = stays(ty, structs, enums, names, &mut Vec::new()) {
        let what = if part == *ty { String::new() } else { format!(", which holds a `{part}`{at}") };
        return Err(Diag::error(span, format!("a static cannot hold a `{ty}`{what}: {}", why_it_stays(&part, names)))
            .note("every task reaches a static, so what it holds owns all it holds: no borrow, no `shared checked` value, no `Task`"));
    }
    if let Some((part, at)) = kept_open(ty, structs, enums, &mut Vec::new()) {
        let what = if part == *ty { String::new() } else { format!(", which holds a `{part}`{at}") };
        let diag = Diag::error(span, format!("a static cannot hold a `{ty}`{what}: a static is never dropped, so it would never be closed"));
        return Err(diag.hint("open it where it is used and lend it to the functions that need it, as a parameter"));
    }
    Ok(())
}

/// The part of a type that is closed or disposed of when it ends, and where it lies: a file, a
/// stream, a socket, a `Closable` or `Disposable` struct.
fn kept_open(ty: &Ty, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>, seen: &mut Vec<String>) -> Option<(Ty, String)> {
    let inner = |ty: &Ty, seen: &mut Vec<String>| kept_open(ty, structs, enums, seen);
    match ty {
        Ty::Reader(_) | Ty::Writer | Ty::Listener | Ty::Socket => Some((ty.clone(), String::new())),
        Ty::Struct(name) if structs.get(name).is_some_and(|info| info.disposable || info.closable) => Some((ty.clone(), String::new())),
        Ty::Shared(inner_ty, _) | Ty::Weak(inner_ty, _) | Ty::Locked(inner_ty) | Ty::Atomic(inner_ty) => inner(inner_ty, seen),
        Ty::Ptr(item) | Ty::Nullable(item) | Ty::List(item) | Ty::Set(item) | Ty::Array(item, _) => inner(item, seen),
        Ty::Result(a, b) | Ty::Map(a, b) => inner(a, seen).or_else(|| inner(b, seen)),
        Ty::Struct(name) | Ty::Enum(name) => {
            if seen.contains(name) {
                return None;
            }
            seen.push(name.clone());
            let fields: Vec<&(String, Ty)> = match (structs.get(name), enums.get(name)) {
                (Some(info), _) => info.fields.iter().collect(),
                (_, Some(info)) => info.variants.iter().flat_map(|(_, fields)| fields).collect(),
                _ => Vec::new(),
            };
            let found = fields.iter().find_map(|(field, field_ty)| {
                inner(field_ty, seen).map(|(part, at)| match at.is_empty() {
                    true => (part, format!(" (the field `{field}` of `{}`)", short_name(name))),
                    false => (part, at),
                })
            });
            seen.pop();
            found
        }
        _ => None,
    }
}

/// What each `shared locked T` holds follows the thread rule: several tasks reach it.
fn thread_rule_of_locked(structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>, names: &Names) -> Res<()> {
    for (inner, span) in LOCKED.with(|found| std::mem::take(&mut *found.borrow_mut())) {
        if let Some((part, at)) = stays(&inner, structs, enums, names, &mut Vec::new()) {
            let what = if part == inner { String::new() } else { format!(", which holds a `{part}`{at}") };
            return Err(Diag::error(span, format!("a `locked` value cannot hold a `{inner}`{what}: {}", why_it_stays(&part, names))).note(THREAD_RULE));
        }
    }
    Ok(())
}

/// A variable of this type has a use in its end alone: a `dispose` runs there, of its struct or
/// of a struct in its fields (when it is the last owner, for a `shared` one).
fn disposes(ty: &Ty, structs: &HashMap<String, StructInfo>) -> bool {
    fn looks(ty: &Ty, structs: &HashMap<String, StructInfo>, seen: &mut Vec<String>) -> bool {
        match ty {
            Ty::Struct(name) if !seen.contains(name) => {
                seen.push(name.clone());
                structs.get(name).is_some_and(|info| info.disposable || info.fields.iter().any(|(_, field)| looks(field, structs, seen)))
            }
            Ty::Nullable(inner) | Ty::Ptr(inner) | Ty::Shared(inner, _) => looks(inner, structs, seen),
            _ => false,
        }
    }
    looks(ty, structs, &mut Vec::new())
}

#[allow(clippy::too_many_arguments)]
fn check_tests(
    prog: &mut Program,
    names: &Names,
    consts: &ConstTable,
    sets: &Promised,
    structs: &HashMap<String, StructInfo>,
    enums: &HashMap<String, EnumInfo>,
    sigs: &HashMap<String, FuncSig>,
    src: &str,
) -> Res<Vec<crate::unread::Seen>> {
    let (copyable, formattable, equatable, comparable) = (sets.copyable, sets.formattable, sets.equatable, sets.comparable);
    let mut seen = Vec::new();
    for position in 0..prog.tests.len() {
        let test = &prog.tests[position];
        let test_span = test.name_span;
        if let Some(first) = prog.tests[..position].iter().find(|other| other.name == test.name && names.module(other.name_span) == names.module(test_span)) {
            return Err(Diag::error(test.name_span, format!("there are two tests named \"{}\"", test.name))
                .related(first.name_span, "the other one is here"));
        }
        let mut checker = Checker {
            names,
            consts,
            in_const: false,
            copyable,
            formattable,
            equatable,
            comparable,
            structs,
            enums,
            sigs,
            scopes: vec![Scope::default()],
            locks: Vec::new(),
            borrows: Borrows::default(),
            loops: 0,
            loop_cx: Vec::new(),
            ret: Ty::Void,
            sources: Vec::new(),
            func_name: "test".to_string(),
            in_test: true,
            is_names: Vec::new(),
            owner: None,
            module: names.module(test_span),
            src,
            is_async: false,
            awaiting: false,
            unsafe_at: None,
            log_guarded: HashSet::new(),
            writing: false,
            written: Vec::new(),
            extern_arg: false,
            seen: crate::unread::Seen::default(),
        };
        checker.declare_streams(test_span)?;
        checker.block(&mut prog.tests[position].body, &[])?;
        checker.borrow_conflicts()?;
        crate::opening::open(&mut prog.tests[position].body, &|ty: &Ty| lends(ty, names), src)?;
        seen.push(checker.seen);
    }
    Ok(seen)
}

fn resolve_type(ty: &TypeExpr, names: &Names, allow_void: bool) -> Res<Ty> {
    if !matches!(ty.name.as_str(), "Result" | "List" | "HashMap" | "HashSet" | "Func" | "CFunc" | "Task" | "Channel" | "Sender") && !ty.args.is_empty() {
        return Err(Diag::error(ty.span, format!("`{}<...>`: generic types are not in the interpreter yet", ty.name))
            .note("the types written with `<` and `>` for now are `Result<T, E>`, `List<T>`, `HashMap<K, V>`, `HashSet<T>`, `Func<A, R>`, `Task<R>`, `Channel<T>` and `Sender<T>`"));
    }
    let no_borrow = |arg: &TypeExpr, of: &str| match arg.mode {
        Some(_) => Err(Diag::error(arg.span, format!("a `{of}` that holds borrows is not in the interpreter yet"))
            .note("it needs the borrow check, which is not here yet")),
        None => Ok(()),
    };
    let mut resolved = match Ty::from_name(&ty.name) {
        // What a task started with `Task.run` gives when it is waited for.
        _ if ty.name == "Task" => {
            let [result] = ty.args.as_slice() else {
                return Err(Diag::error(ty.span, "`Task` is written with what the task returns: `Task<R>`")
                    .note("a task whose function returns nothing is a `Task<void>`"));
            };
            no_borrow(result, "Task")?;
            Ty::Task(Box::new(resolve_type(result, names, true)?))
        }
        // The two ends of a channel. An item is never `null`, which `receive` gives at the end.
        _ if ty.name == "Channel" || ty.name == "Sender" => {
            let [item] = ty.args.as_slice() else {
                return Err(Diag::error(ty.span, format!("`{0}` is written with the type of its items: `{0}<T>`", ty.name)));
            };
            no_borrow(item, &ty.name)?;
            let item_ty = resolve_type(item, names, false)?;
            if let Ty::Nullable(inner) = &item_ty {
                return Err(Diag::error(item.span, format!("the items of a channel are not `{item_ty}`: `receive` gives `null` when the channel has ended, and a `null` item would read the same"))
                    .hint(format!("an enum with a variant for the missing value, or a `List<{inner}>` of one item or none")));
            }
            match ty.name.as_str() {
                "Channel" => Ty::Channel(Box::new(item_ty)),
                _ => Ty::Sender(Box::new(item_ty)),
            }
        }
        _ if ty.name == "List" => {
            let [item] = ty.args.as_slice() else {
                return Err(Diag::error(ty.span, "`List` is written with the type of its items: `List<T>`"));
            };
            // A list of `read` borrows (1732); a list of `ref` borrows is not in the language.
            if item.mode == Some(Mode::Ref) {
                return Err(Diag::error(item.span, "a `List` of `ref` borrows is not in the language")
                    .note("a list lends its items with `read`; two `ref` borrows in one list could reach one place")
                    .hint("keep the values themselves in the list, and change them through it: `xs[i].field = v`"));
            }
            Ty::List(Box::new(resolve_type(item, names, false)?))
        }
        // The type of a function: what it takes, then what it returns.
        _ if ty.name == "Func" => {
            let Some((ret, params)) = ty.args.split_last() else {
                return Err(Diag::error(ty.span, "`Func` is written with the types of the parameters and then the return type: `Func<int, bool>`")
                    .note("a function that takes nothing and returns nothing is a `Func<void>`"));
            };
            if ret.mode.is_some() {
                return Err(Diag::error(ret.span, "a function value that returns a borrow is not in the interpreter yet")
                    .note("it needs the borrow check, which is not here yet"));
            }
            let mut resolved = Vec::new();
            for param in params {
                resolved.push(resolve_type(param, names, false)?);
            }
            Ty::Func(resolved, Box::new(resolve_type(ret, names, true)?))
        }
        // A pointer to a function as C calls it: what it takes, then what it returns, all of
        // them types of C (checked once the fields of every type are known).
        _ if ty.name == "CFunc" => {
            let Some((ret, params)) = ty.args.split_last() else {
                return Err(Diag::error(ty.span, "`CFunc` is written with the types of the parameters and then the return type: `CFunc<raw void, raw void, int>`")
                    .note("a function of C that takes nothing and returns nothing is a `CFunc<void>`"));
            };
            for arg in ty.args.iter() {
                if arg.mode.is_some() {
                    return Err(Diag::error(arg.span, "a function of C takes and gives values, not borrows")
                        .hint("a pointer C gives is a `raw T`, read inside `unsafe`"));
                }
            }
            let mut resolved = Vec::new();
            for param in params {
                resolved.push(resolve_type(param, names, false)?);
            }
            let found = Ty::CFunc(resolved, Box::new(resolve_type(ret, names, true)?));
            CFUNCS.with(|found_all| found_all.borrow_mut().push((found.clone(), ty.span)));
            found
        }
        _ if ty.name == "HashSet" => {
            let [item] = ty.args.as_slice() else {
                return Err(Diag::error(ty.span, "`HashSet` is written with the type of its items: `HashSet<T>`"));
            };
            no_borrow(item, "HashSet")?;
            let item_ty = resolve_type(item, names, false)?;
            if !has(&item_ty, &names.keys, false) || matches!(item_ty, Ty::Ptr(_) | Ty::Nullable(_) | Ty::Shared(..) | Ty::Weak(..)) {
                return Err(not_a_key(item.span, &item_ty, "an item of a `HashSet`"));
            }
            Ty::Set(Box::new(item_ty))
        }
        _ if ty.name == "HashMap" => {
            let [key, value] = ty.args.as_slice() else {
                return Err(Diag::error(ty.span, "`HashMap` is written with two types: `HashMap<K, V>`")
                    .note("`K` is the key, `V` is what is kept under it"));
            };
            no_borrow(key, "HashMap")?;
            no_borrow(value, "HashMap")?;
            let key_ty = resolve_type(key, names, false)?;
            if !has(&key_ty, &names.keys, false) || matches!(key_ty, Ty::Ptr(_) | Ty::Nullable(_) | Ty::Shared(..) | Ty::Weak(..)) {
                return Err(not_a_key(key.span, &key_ty, "the key of a `HashMap`"));
            }
            Ty::Map(Box::new(key_ty), Box::new(resolve_type(value, names, false)?))
        }
        _ if ty.name == "Result" => {
            let [ok, error] = ty.args.as_slice() else {
                return Err(Diag::error(ty.span, "`Result` is written with two types: `Result<T, E>`")
                    .note("`T` is what success gives (`void` for nothing), `E` is the error"));
            };
            // Success may be a borrow (`Result<read string, TextError>`); a function that
            // returns one says where it comes from, as for any type that carries a borrow.
            if error.mode.is_some() {
                return Err(Diag::error(error.span, "a `Result` whose error is a borrow is not in the interpreter yet"));
            }
            if error.name == "void" {
                return Err(Diag::error(error.span, "the error of a `Result` cannot be `void`")
                    .hint("a value that may be missing, with nothing to say about why, is a `T?`"));
            }
            Ty::Result(Box::new(resolve_type(ok, names, true)?), Box::new(resolve_type(error, names, false)?))
        }
        Some(Ty::Void) if ty.raw > 0 && ty.suffixes.is_empty() => Ty::Void,
        Some(Ty::Void) if !allow_void || !ty.suffixes.is_empty() || ty.mode.is_some() || ty.raw > 0 => {
            return Err(Diag::error(ty.span, "`void` can only be a return type"));
        }
        Some(base) => base,
        None => match names.full(&ty.name, ty.span)?.and_then(|full| names.kinds.get(&full).map(|kind| (full, *kind))) {
            Some((full, TypeKind::Struct)) if names.opaque.contains(&full) && (ty.raw == 0 || !ty.suffixes.is_empty() || ty.holder.is_some()) => {
                let short = short_name(&full);
                return Err(Diag::error(ty.span, format!("`{short}` is a type of C whose insides SG does not see, so it is only pointed at"))
                    .hint(format!("`raw {short}`, as C writes `{short}*`")));
            }
            Some((full, TypeKind::Struct)) => Ty::Struct(full),
            Some((full, TypeKind::Enum)) => Ty::Enum(full),
            Some((full, TypeKind::Alias)) => {
                if !ty.args.is_empty() {
                    return Err(Diag::error(ty.span, format!("`{}` is an alias and takes no types between `<` and `>`", ty.name)));
                }
                let target = &names.aliases[&full];
                let circle = RESOLVING.with(|stack| {
                    let stack = stack.borrow();
                    stack.iter().position(|name| *name == full).map(|at| stack[at..].iter().map(|name| format!("`{}`", short_name(name))).collect::<Vec<_>>())
                });
                if let Some(circle) = circle {
                    let msg = match circle.len() {
                        1 => format!("the alias {} names itself", circle[0]),
                        2 => format!("the aliases {} and {} name each other", circle[0], circle[1]),
                        _ => format!("the aliases {} name each other in a circle", circle.join(", ")),
                    };
                    return Err(Diag::error(target.span, msg).note("an alias is another name for a type that is written out in the end"));
                }
                RESOLVING.with(|stack| stack.borrow_mut().push(full.clone()));
                let resolved = resolve_type(target, names, false);
                RESOLVING.with(|stack| stack.borrow_mut().pop());
                let resolved = resolved?;
                if ty.holder.is_some() && matches!(resolved, Ty::Shared(..) | Ty::Weak(..)) {
                    return Err(Diag::error(ty.span, format!("`{}` is already `{resolved}`", ty.name)));
                }
                resolved
            }
            None => {
            if TEXT_ITERATORS.contains(&ty.name.as_str()) {
                return Err(Diag::error(ty.span, format!("the type `{}` cannot be written yet", ty.name))
                    .hint("keep it in a `var`, or go over it with `for`")
                    .note("it borrows the text it goes over; types that carry a borrow of a built-in type come with generics"));
            }
            let diag = {
                Diag::error(ty.span, format!("there is no type named `{}`", ty.name))
            };
            if ty.name == "StringBuilder" {
                return Err(diag.hint(STRING_BUILDER));
            }
            if let Some(hint) = names.use_hint(&ty.name, ty.span) {
                return Err(diag.hint(hint));
            }
            let known = names.kinds.keys().map(String::as_str).chain(["byte", "int", "long", "nint", "double", "bool", "char", "string"]);
            return Err(match nearest(&ty.name, known) {
                Some(close) => diag.hint(format!("`{close}`")),
                None => diag.note("built-in types for now: the integer types, float, double, bool, char, string"),
            });
            }
        },
    };
    // `shared Node?` is a `shared Node` that may be missing: the holder goes around the type
    // before `?` does.
    let mut holder = ty.holder;
    let mut raw = ty.raw;
    // `atomic int`, `raw atomic int`: the value is atomic before the suffixes and the `raw`s
    // around it; `atomic raw Node`: the pointers after `atomic` are taken in first.
    if let Some(before) = ty.atomic {
        if before < raw {
            if !ty.suffixes.is_empty() {
                return Err(Diag::error(ty.span, "an atomic pointer is written without `*`, `?` or `[...]` after it")
                    .hint("name the pointer with `using`: `using NodeRef = raw Node`, then `atomic NodeRef`"));
            }
            RAW.with(|found| found.borrow_mut().push((resolved.clone(), ty.span)));
            while raw > before {
                resolved = Ty::Raw(Box::new(resolved));
                raw -= 1;
            }
        }
        resolved = atomic_of(resolved, ty.span)?;
    }
    for suffix in &ty.suffixes {
        if *suffix == Suffix::Nullable {
            if raw > 0 {
                return Err(Diag::error(ty.span, "a raw pointer can be `null` already, so it is not written with `?`")
                    .hint("compare it: `p == null`"));
            }
            if let Some(word) = holder.take() {
                resolved = held(word, resolved, ty, names)?;
            }
        }
        // A slice that may be missing (1774) is lent: `read T[]?`, checked where the mode is.
        if matches!(resolved, Ty::Slice(_)) && *suffix != Suffix::Nullable {
            return Err(bare_slice(ty.span, &resolved));
        }
        resolved = match suffix {
            Suffix::Array(0) => return Err(Diag::error(ty.span, "an array has at least one item")),
            Suffix::Array(_) if matches!(resolved, Ty::Array(..)) => {
                return Err(Diag::error(ty.span, "an array of arrays is not in the interpreter yet"));
            }
            Suffix::Array(length) => Ty::Array(Box::new(resolved), *length),
            Suffix::Named(..) if matches!(resolved, Ty::Array(..)) => {
                return Err(Diag::error(ty.span, "an array of arrays is not in the interpreter yet"));
            }
            Suffix::Named(written, at) => Ty::Array(Box::new(resolved), named_size(written, *at, names)?),
            Suffix::Slice => Ty::Slice(Box::new(resolved)),
            Suffix::Pointer => Ty::Ptr(Box::new(resolved)),
            Suffix::Nullable if matches!(resolved, Ty::CFunc(..)) => {
                return Err(Diag::error(ty.span, "a `CFunc` may be `null` as it is, as a pointer of C may be")
                    .hint(format!("`{}`, compared with `null` before it is called", resolved)));
            }
            Suffix::Nullable if matches!(resolved, Ty::Atomic(_)) => {
                return Err(Diag::error(ty.span, format!("an `{resolved}` is always there, so it is not written with `?`"))
                    .note("an atomic pointer may be `null`: `atomic raw Node`"));
            }
            Suffix::Nullable if matches!(resolved, Ty::Nullable(_)) => {
                return Err(Diag::error(ty.span, "`??` is not a type: a value is either there or `null`"));
            }
            Suffix::Nullable => Ty::Nullable(Box::new(resolved)),
        };
    }
    if raw > 0 {
        RAW.with(|found| found.borrow_mut().push((resolved.clone(), ty.span)));
    }
    while raw > 0 {
        resolved = Ty::Raw(Box::new(resolved));
        raw -= 1;
    }
    if let Some(word) = holder {
        resolved = held(word, resolved, ty, names)?;
    }
    if ty.mode.is_none() && matches!(resolved, Ty::Slice(_)) {
        return Err(bare_slice(ty.span, &resolved));
    }
    // A slice that may be missing is only lent, and only to be read (1774).
    if let Ty::Nullable(slice) = &resolved {
        if matches!(**slice, Ty::Slice(_)) && ty.mode != Some(Mode::Read) {
            let Ty::Slice(item) = &**slice else { unreachable!() };
            return Err(match ty.mode {
                Some(Mode::Ref) => Diag::error(ty.span, format!("a `ref {slice}?` is not in the language: a slice that may be missing is only read"))
                    .hint(format!("`read {slice}?` to look at it, or `ref {slice}` once it is there")),
                _ => Diag::error(ty.span, format!("`{slice}?` is not a type on its own: a slice only looks at items that something else owns"))
                    .hint(format!("`read {slice}?` for a window that may be missing, `List<{item}>?` to own the items")),
            });
        }
    }
    // A `List<Task<R>>` is kept where a task is: its tasks are waited for with
    // `Task.whenAll` and `Task.whenAny`.
    let nested = match &resolved {
        Ty::Task(result) => task_inside(result),
        Ty::Group => false,
        Ty::List(item) => match &**item {
            Ty::Task(result) => task_inside(result),
            other => task_inside(other),
        },
        other => task_inside(other),
    };
    // A group is lent to a function that adds tasks to it, with `read` (1716).
    if resolved == Ty::Group && ty.mode == Some(Mode::Ref) {
        return Err(Diag::error(ty.span, "a `TaskGroup` is lent with `read`: `add` works through it, and only its owner waits for it")
            .hint("`read TaskGroup`"));
    }
    if nested || (ty.mode.is_some() && keeps_tasks(&resolved) && resolved != Ty::Group) {
        return Err(task_kept(ty.span));
    }
    if let Some(layouts) = &names.layouts {
        too_big(&resolved, layouts, ty.span)?;
    }
    Ok(match ty.mode {
        Some(Mode::Read) => Ty::Read(Box::new(resolved)),
        Some(Mode::Ref) => Ty::Ref(Box::new(resolved)),
        None => resolved,
    })
}

/// `atomic T` around a type that an atomic operation takes as one step: an integer, a `bool`,
/// a `float`, a `double` or a raw pointer.
/// The count of `byte[BlockSize]`: what the integer const holds. One of a block is written by
/// its full name, which the parser found.
fn named_size(written: &str, at: Span, names: &Names) -> Res<usize> {
    let full = match written.contains('#') {
        true => Some(written.to_string()),
        false => names.full(written, at)?.filter(|full| names.consts.contains(full)),
    };
    let shown = written.split('#').next().unwrap_or(written);
    let Some(full) = full else {
        let diag = Diag::error(at, format!("there is no const named `{shown}`"));
        return Err(match names.full(written, at)? {
            Some(_) => diag.note("the count of an array is a number or an integer `const`"),
            None => diag,
        });
    };
    let count = match names.sizes.get(&full) {
        Some(Ok(count)) => *count,
        Some(Err(why)) => return Err(Diag::error(at, format!("`{shown}` cannot give the count of an array: {why}"))),
        None => return Err(Diag::error(at, format!("`{shown}` cannot give the count of an array: it is not an integer")).note("the count of an array is a number or an integer `const`")),
    };
    if count <= 0 {
        return Err(Diag::error(at, format!("an array has at least one item, and `{shown}` is {count}")));
    }
    let Ok(count) = usize::try_from(count) else { return Err(Diag::error(at, "an array cannot have that many items")) };
    SIZES.with(|sizes| sizes.borrow_mut().insert(at.start, (count, full)));
    Ok(count)
}

/// What an integer const holds, found from its text before the types are known, for the count
/// of an array: numbers, other such consts, operators and casts to an integer type. The checker
/// finds every const again later, and the two must agree.
fn early_size(decl: &ConstDecl, consts: &HashMap<&str, &ConstDecl>, names: &Names, finding: &mut Vec<String>) -> Result<i128, String> {
    let Some(own) = IntTy::from_name(&decl.ty.name).filter(|_| decl.ty.suffixes.is_empty() && decl.ty.mode.is_none() && decl.ty.holder.is_none() && decl.ty.raw == 0 && decl.ty.atomic.is_none()) else {
        return Err("it is not an integer".to_string());
    };
    if finding.contains(&decl.name) {
        return Err("its value needs itself".to_string());
    }
    finding.push(decl.name.clone());
    let found = early_value(&decl.init, decl, consts, names, finding).and_then(|value| match own.fits(value) {
        true => Ok(value),
        false => Err(format!("{value} does not fit in {}", own.name())),
    });
    finding.pop();
    found
}

fn early_value(expr: &Expr, decl: &ConstDecl, consts: &HashMap<&str, &ConstDecl>, names: &Names, finding: &mut Vec<String>) -> Result<i128, String> {
    let made_of = "its value is made of more than numbers, integer consts and operators";
    let value = |expr: &Expr, finding: &mut Vec<String>| early_value(expr, decl, consts, names, finding);
    match &expr.kind {
        ExprKind::Int(value, _) => Ok(*value),
        ExprKind::Char(c) => Ok(*c as i128),
        ExprKind::Var(_) | ExprKind::Field { .. } => {
            if let ExprKind::Field { recv, name, .. } = &expr.kind {
                if let ExprKind::Var(owner) = &recv.kind {
                    if let Some(ty) = IntTy::from_name(owner) {
                        return match name.as_str() {
                            "MaxValue" => Ok(ty.max()),
                            "MinValue" => Ok(ty.min()),
                            _ => Err(made_of.to_string()),
                        };
                    }
                }
            }
            let written = crate::parser::dotted(expr).ok_or_else(|| made_of.to_string())?;
            let local = decl.visible.iter().rev().find(|(name, _)| *name == written).map(|(_, full)| full.clone());
            let full = match local {
                Some(full) => full,
                None => names.full(&written, expr.span).ok().flatten().filter(|full| names.consts.contains(full)).ok_or_else(|| made_of.to_string())?,
            };
            let other = consts.get(full.as_str()).ok_or_else(|| made_of.to_string())?;
            early_size(other, consts, names, finding)
        }
        ExprKind::Unary(UnOp::Neg, inner) => Ok(-value(inner, finding)?),
        ExprKind::Unary(UnOp::BitNot, inner) => {
            let own = IntTy::from_name(&decl.ty.name).expect("an integer const");
            Ok(own.wrap(!value(inner, finding)?))
        }
        ExprKind::Binary(op, left, right) => {
            let (a, b) = (value(left, finding)?, value(right, finding)?);
            let found = match op {
                BinOp::Add => a.checked_add(b),
                BinOp::Sub => a.checked_sub(b),
                BinOp::Mul => a.checked_mul(b),
                BinOp::Div | BinOp::Rem if b == 0 => return Err("it divides by zero".to_string()),
                BinOp::Div => a.checked_div(b),
                BinOp::Rem => a.checked_rem(b),
                BinOp::BitAnd => Some(a & b),
                BinOp::BitOr => Some(a | b),
                BinOp::BitXor => Some(a ^ b),
                BinOp::Shl if (0..64).contains(&b) => a.checked_shl(b as u32),
                BinOp::Shr if (0..64).contains(&b) => Some(a >> b),
                _ => None,
            };
            found.ok_or_else(|| made_of.to_string())
        }
        ExprKind::Cast { ty: Ty::Int(to), expr: inner } => {
            let found = value(inner, finding)?;
            match to.fits(found) {
                true => Ok(found),
                false => Err(format!("{found} does not fit in {}", to.name())),
            }
        }
        ExprKind::Call { .. } | ExprKind::Method { .. } => Err(CALLS_NONE.to_string()),
        _ => Err(made_of.to_string()),
    }
}

/// The type of a const, with the count of `T[]` taken from `embed` (`{ ... }` was counted by the
/// parser). What it holds is plain data the program can read from every thread.
fn const_type(decl: &ConstDecl, names: &Names, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>, copyable: &HashSet<String>, embeds: &[usize]) -> Res<Ty> {
    let written = &decl.ty;
    if written.mode.is_some() {
        return Err(Diag::error(written.span, "a const is not declared with `read` or `ref`: it is lent where it is used")
            .hint(format!("`const {} ...`", written.name)));
    }
    let mut whole = written.clone();
    let counted = matches!(whole.suffixes.last(), Some(Suffix::Slice));
    if counted {
        whole.suffixes.pop();
    }
    let mut ty = resolve_type(&whole, names, false)?;
    if counted {
        let count = match &decl.init.kind {
            ExprKind::Embed(index) => embeds[*index],
            // A call is refused as a call: a const calls no function (choice 1606).
            ExprKind::Call { name_span, .. } => return Err(call_in_const(*name_span)),
            ExprKind::Method { name, name_span, .. } if name != "length" && name != NUMBER_OF => return Err(call_in_const(*name_span)),
            _ => {
                return Err(Diag::error(written.span, "the count of the items of `T[]` is taken from `{ ... }` or from `embed`")
                    .hint("write the count: `T[N]`"));
            }
        };
        if count == 0 {
            return Err(Diag::error(decl.init.span, "the file is empty, and an array has at least one item"));
        }
        if matches!(ty, Ty::Array(..)) {
            return Err(Diag::error(written.span, "an array of arrays is not in the interpreter yet"));
        }
        ty = Ty::Array(Box::new(ty), count);
    }
    if let Some(why) = not_const_type(&ty, true, structs, enums, copyable) {
        return Err(Diag::error(written.span, format!("a const cannot hold a `{ty}`: {why}"))
            .note("a const holds numbers, `bool`, `char`, a text, and arrays, structs and enums of them"));
    }
    Ok(ty)
}

/// What the default value of a field can be.
/// What the default value of a field or of a parameter can be (`what`: "a field" or "a
/// parameter").
fn default_note(what: &str) -> String {
    let rest = match what {
        "a parameter" => ", a literal that gives no field such as `Options {}`, or `CancellationToken.None`; a value found when the program runs is given by the call",
        _ => ", or a literal that gives no field such as `Options {}`",
    };
    format!("the default value of {what} is found when the program is compiled, as a const's is: numbers, `bool`, `char`, a text, and arrays, structs and enums of them; or `null`, an empty collection such as `List<int>()`{rest}")
}

/// Why a const cannot hold a value of this type, if it cannot. `top`: the const itself, where a
/// text may be.
fn not_const_type(ty: &Ty, top: bool, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>, copyable: &HashSet<String>) -> Option<String> {
    match ty {
        Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char => None,
        Ty::Str if top => None,
        Ty::Str => Some("a text inside an array, a struct or an enum is not in a const yet".to_string()),
        Ty::Array(item, _) => not_const_type(item, false, structs, enums, copyable),
        Ty::Struct(name) => {
            let info = &structs[name];
            if info.shape.union {
                return Some(format!("`{}` is a union, which is read inside `unsafe`", short_name(name)));
            }
            if !copyable.contains(name) {
                return Some(format!("`{}` is not `Copyable`", short_name(name)));
            }
            info.fields.iter().find_map(|(_, field)| not_const_type(field, false, structs, enums, copyable))
        }
        Ty::Enum(name) => {
            if !copyable.contains(name) {
                return Some(format!("`{}` is not `Copyable`", short_name(name)));
            }
            enums[name].variants.iter().flat_map(|(_, fields)| fields).find_map(|(_, field)| not_const_type(field, false, structs, enums, copyable))
        }
        other => Some(format!("a `{other}` is not known when the program is compiled")),
    }
}

/// What a const is given is known when the program is compiled: numbers, texts, other consts,
/// operators, casts, calls, and arrays, structs and enums built of them.
/// `measures`: the const is a `const assert`, where `Memory.sizeOf` and `Memory.alignOf` are
/// written too.
fn constant_only(expr: &Expr, measures: bool) -> Res<()> {
    match &expr.kind {
        ExprKind::Int(..) | ExprKind::Float(..) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Char(_) | ExprKind::Const(_) | ExprKind::Embed(_) | ExprKind::Constant { .. } => Ok(()),
        ExprKind::Unary(_, inner) | ExprKind::Cast { expr: inner, .. } | ExprKind::Widen(_, inner) | ExprKind::Load(inner) | ExprKind::Field { recv: inner, .. } => constant_only(inner, measures),
        ExprKind::Binary(_, left, right) | ExprKind::Index { recv: left, index: right, .. } => {
            constant_only(left, measures)?;
            constant_only(right, measures)
        }
        ExprKind::Array { args, items, .. } => args.iter().chain(items.iter().flatten()).try_for_each(|arg| constant_only(arg, measures)),
        ExprKind::StructLit { fields, .. } => fields.iter().try_for_each(|field| constant_only(&field.value, measures)),
        ExprKind::Variant { args, .. } => args.iter().try_for_each(|arg| constant_only(arg, measures)),
        // The length of a text or of an array, and the number of a variant, are known from
        // the value.
        ExprKind::Method { recv, name, args, .. } if (name == "length" && args.is_empty()) || name == NUMBER_OF => constant_only(recv, measures),
        // A const calls no function (choice 1606): its value is found from what is written,
        // without running the program.
        ExprKind::Call { name_span, .. } | ExprKind::Method { name_span, .. } => Err(call_in_const(*name_span)),
        ExprKind::SizeOf { .. } if measures => Ok(()),
        ExprKind::SizeOf { item, align, .. } => Err(Diag::error(expr.span, format!("the {} of a type differs from machine to machine, and a const holds the same value on every one", if *align { "alignment" } else { "size" }))
            .hint(format!("check it with `const assert(Memory.{}<{}>() == ...)`, or find it when the program runs", if *align { "alignOf" } else { "sizeOf" }, item.name))),
        ExprKind::Var(name) => Err(Diag::error(expr.span, format!("`{name}` is a variable, and the value of a const is found when the program is compiled"))),
        _ => Err(Diag::error(expr.span, "this is not known when the program is compiled, and a const is given its value then")
            .note(CONST_MADE_OF)),
    }
}

/// The error of a call in a const; the checker says it of a field's default value, the number
/// of a variant and a `const assert` in their own words.
const CALLS_NONE: &str = "a const calls no function";

/// A call in a const, at the name called.
fn call_in_const(name_span: Span) -> Diag {
    Diag::error(name_span, CALLS_NONE)
        .note(CONST_MADE_OF)
        .hint("a value a function finds is a `static`, found when the program first uses it: `static T Name = f()`")
}

/// What a const is made of.
const CONST_MADE_OF: &str = "a const is made of numbers, texts, other consts, operators and casts, and arrays, structs and enums of them";

fn atomic_of(inner: Ty, span: Span) -> Res<Ty> {
    match inner {
        Ty::Int(int) if int.is_c() => Err(Diag::error(span, format!("an atomic value is changed with arithmetic, and C's `{}` has none", c_spelling(int)))
            .hint(format!("`atomic {}`", c_partner(int).name()))),
        Ty::Int(_) | Ty::Bool | Ty::Float(_) | Ty::Raw(_) => Ok(Ty::Atomic(Box::new(inner))),
        Ty::Atomic(_) => Err(Diag::error(span, "`atomic` is written once")),
        other => Err(Diag::error(span, format!("an atomic value is an integer, a `bool`, a `float`, a `double` or a raw pointer, and this is `{other}`"))
            .note("what the processor changes in one step; a larger value that several tasks change is a `shared locked T`")),
    }
}

/// The name of the language's own enum of the orders of atomic operations.
const MEMORY_ORDER: &str = "std.MemoryOrder";

/// The methods of an `atomic T`.
const ATOMIC_METHODS: &[&str] = &[
    "load", "store", "exchange", "compareExchange", "compareExchangeWeak", "fetchAdd", "fetchSub", "wrappingFetchAdd", "wrappingFetchSub", "fetchAnd", "fetchOr", "fetchXor",
];

/// Whether an `atomic T` of this `T` has the method.
fn atomic_takes(item: &Ty, method: &str) -> bool {
    match method {
        "load" | "store" | "exchange" | "compareExchange" | "compareExchangeWeak" => true,
        "fetchAdd" | "fetchSub" => matches!(item, Ty::Int(_) | Ty::Float(_)),
        "wrappingFetchAdd" | "wrappingFetchSub" => matches!(item, Ty::Int(_)),
        "fetchAnd" | "fetchOr" | "fetchXor" => matches!(item, Ty::Int(_) | Ty::Bool),
        _ => false,
    }
}

/// Why a `shared` value cannot be written through.
const SHARED_READ_ONLY: &str = "a `shared` value has several owners, and none of them changes it alone; a value that is changed through several owners is a `shared checked T`, whose every access is checked against the borrows of it in progress";

/// Whether the place is reached through a `shared` value: `node.value`, `nodes[0]` of a
/// `shared List<int> nodes`.
fn through_shared(place: &Expr) -> bool {
    through_holder(place, false)
}

/// Whether a place is reached through a `read` borrow, or through a `shared` value that is not
/// `checked`, which is only read: a field or an item of something whose type, as it was found,
/// says so.
fn through_read(place: &Expr) -> bool {
    let (ExprKind::Field { recv, .. } | ExprKind::Index { recv, .. }) = &place.kind else { return false };
    let mut ty = &recv.ty;
    loop {
        ty = match ty {
            Ty::Read(_) => return true,
            Ty::Shared(_, false) => return true,
            Ty::Ref(inner) | Ty::Ptr(inner) | Ty::Shared(inner, true) => inner,
            _ => break,
        };
    }
    through_read(recv)
}

/// Whether the place is reached through a `shared` value, or with `checked`, through a
/// `shared checked` one.
fn through_holder(place: &Expr, checked: bool) -> bool {
    let mut ty = &place.ty;
    while let Ty::Read(inner) | Ty::Ref(inner) | Ty::Ptr(inner) = ty {
        ty = inner;
    }
    if matches!(ty, Ty::Shared(_, flag) if *flag == checked) {
        return true;
    }
    match &place.kind {
        ExprKind::Field { recv, .. } | ExprKind::Index { recv, .. } | ExprKind::Slice { recv, .. } => through_holder(recv, checked),
        ExprKind::Load(inner) | ExprKind::Unbox(inner) | ExprKind::Borrow { expr: inner, .. } => through_holder(inner, checked),
        _ => false,
    }
}

/// `share x` or `weak x` on a value that is neither `shared` nor `weak`.
fn not_shared(word: &str, span: Span, found: &Ty) -> Diag {
    let diag = Diag::error(span, format!("`{word}` needs a `shared` value{}, and this is `{found}`", if word == "share" { " or a `weak` one" } else { "" }));
    match found {
        Ty::Null => diag,
        Ty::Ptr(inner) => diag.note(format!("a `{found}` has one owner; a value with several is made with `new shared`: `shared {inner}`")),
        _ => diag.hint("a value with several owners is made with `new shared`: `var owner = new shared Node { ... }`"),
    }
}

/// `shared T` or `weak T` around a type that can be held so: one that owns its value and
/// carries no borrow.
fn held(holder: Holder, inner: Ty, ty: &TypeExpr, names: &Names) -> Res<Ty> {
    let checked = ty.checked;
    if matches!(inner, Ty::Task(_) | Ty::Group) {
        return Err(task_kept(ty.span));
    }
    let word = if holder == Holder::Shared { "shared" } else { "weak" };
    let refuse = |what: String| Err(Diag::error(ty.span, format!("`{word}` cannot hold {what}")));
    match &inner {
        Ty::Ptr(behind) => {
            return Err(Diag::error(ty.span, format!("`{word}` cannot hold a `{inner}`: a `{word}` value is on the heap already"))
                .hint(format!("`{word} {behind}`")));
        }
        Ty::Nullable(behind) => {
            return Err(Diag::error(ty.span, format!("`{word}` cannot hold a `{inner}`: the `?` goes on the outside"))
                .hint(format!("`{word} {behind}?`, a `{word}` value that may be missing")));
        }
        Ty::Shared(..) | Ty::Weak(..) => return refuse(format!("a `{inner}`")),
        Ty::Slice(_) => return refuse("a slice: a slice is a borrow".to_string()),
        Ty::Void => return refuse("`void`".to_string()),
        _ => {}
    }
    if lends(&inner, names) {
        return Err(Diag::error(ty.span, format!("`{word}` cannot hold a `{inner}`, which carries a borrow"))
            .note("a `shared` value lives as long as its last owner, which can be longer than anything it borrows from"));
    }
    // What a `locked` value holds follows the thread rule, which needs the fields of every
    // type: it is checked once they are all known (`thread_rule_of_locked`).
    let inner = match ty.locked {
        true => {
            LOCKED.with(|found| found.borrow_mut().push((inner.clone(), ty.span)));
            Ty::Locked(Box::new(inner))
        }
        false => inner,
    };
    Ok(match holder {
        Holder::Shared => Ty::Shared(Box::new(inner), checked),
        Holder::Weak => Ty::Weak(Box::new(inner), checked),
    })
}

/// The words of C, which no function C calls may be named.
const C_WORDS: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else", "enum", "extern", "float", "for", "goto", "if",
    "inline", "int", "long", "register", "restrict", "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef", "union",
    "unsigned", "void", "volatile", "while", "asm", "typeof",
];

thread_local! {
    /// What each `shared locked T` written in the program holds, and where: checked against
    /// the thread rule once the fields of every type are known.
    static LOCKED: std::cell::RefCell<Vec<(Ty, Span)>> = const { std::cell::RefCell::new(Vec::new()) };
    /// What each `raw T` written in the program points at, and where: checked to be plain data
    /// once the fields of every type are known.
    static RAW: std::cell::RefCell<Vec<(Ty, Span)>> = const { std::cell::RefCell::new(Vec::new()) };
    /// Each `CFunc<...>` written in the program, and where: checked to take and give types of
    /// C once the fields of every type are known.
    static CFUNCS: std::cell::RefCell<Vec<(Ty, Span)>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The count each `[Name]` in a type stood for, by where the name is written, and the full
    /// name of the const.
    static SIZES: std::cell::RefCell<std::collections::BTreeMap<usize, (usize, String)>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    /// Each pattern written in the program and given to `Regex.compile` or `Regex.of`, and
    /// where: checked once every body is (choices 1463, 1469).
    static PATTERNS: std::cell::RefCell<std::collections::BTreeMap<usize, (Span, String)>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// Why a value may not go to another task, for the messages of the thread rule.
const THREAD_RULE: &str = "what goes to another task, comes back from one or is held by a `locked` value owns all it holds: no borrow, no `shared checked` value, no `Task`";

/// Whether a `Task` lies inside the type, not counting the types a `Func` takes and gives.
fn task_inside(ty: &Ty) -> bool {
    match ty {
        Ty::Task(_) | Ty::Group => true,
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) | Ty::Array(inner, _) | Ty::Slice(inner) => task_inside(inner),
        Ty::Shared(inner, _) | Ty::Weak(inner, _) | Ty::Locked(inner) | Ty::Read(inner) | Ty::Ref(inner) => task_inside(inner),
        Ty::Result(a, b) | Ty::Map(a, b) | Ty::Entry(a, b) => task_inside(a) || task_inside(b),
        _ => false,
    }
}

/// A `Task` written where the compiler could not see it waited for.
fn task_kept(span: Span) -> Diag {
    Diag::error(span, "a `Task` or a `TaskGroup` is kept only in a local variable, a parameter or a return value; a task also in a `List<Task<R>>` or a `TaskGroup`")
        .note("every task is waited for, and the compiler sees that only where the task stays in one function's hands")
}

/// A type whose value holds tasks that are waited for before it ends: a `Task`, or a
/// `List<Task<R>>`.
fn keeps_tasks(ty: &Ty) -> bool {
    match ty {
        Ty::Task(_) | Ty::Group => true,
        Ty::List(item) => matches!(**item, Ty::Task(_)),
        _ => false,
    }
}

/// `Task.whenAll(...)` or `Task.whenAny(...)`, as written, before it is checked.
fn is_when(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Method { recv, name, .. } if matches!(&recv.kind, ExprKind::Var(owner) if owner == "Task") => matches!(name.as_str(), "whenAll" | "whenAny"),
        // `group.whenRoom()`, `group.whenAll()` of a `TaskGroup`.
        ExprKind::Method { name, .. } => matches!(name.as_str(), "whenRoom" | "whenAll"),
        _ => false,
    }
}

/// The list of tasks a `while` empties: `while tasks.length() > 0`, also with `!= 0`, `0 <`
/// or `0 !=`. The loop ends only when the list has no task left.
fn emptied_by(cond: &Expr) -> Option<&str> {
    let ExprKind::Binary(op, left, right) = &cond.kind else { return None };
    let zero = |expr: &Expr| matches!(unwidened(expr).kind, ExprKind::Int(0, _));
    fn length(expr: &Expr) -> Option<&str> {
        match &unwidened(expr).kind {
            ExprKind::Method { recv, name, args, .. } if name == "length" && args.is_empty() && task_list(&recv.ty) => match &recv.kind {
                ExprKind::Var(list) => Some(list.as_str()),
                _ => None,
            },
            _ => None,
        }
    }
    match op {
        BinOp::Gt | BinOp::Ne if zero(right) => length(left),
        BinOp::Lt | BinOp::Ne if zero(left) => length(right),
        _ => None,
    }
}

/// A `List<Task<R>>`.
fn task_list(ty: &Ty) -> bool {
    matches!(ty, Ty::List(item) if matches!(**item, Ty::Task(_)))
}

/// How the tasks of the list `text`, a `List<Task<R>>`, are waited for: in an async function
/// or not. What they give is dropped on purpose unless they give nothing.
fn when_all(ty: &Ty, text: &str, is_async: bool) -> String {
    let gives = !matches!(ty, Ty::List(item) if matches!(&**item, Ty::Task(result) if **result == Ty::Void));
    let drop = if gives { "_ = " } else { "" };
    match is_async {
        true => format!("`{drop}await Task.whenAll({text})`"),
        false => format!("`{drop}Task.whenAll({text}).wait()`"),
    }
}

/// The part of a type that cannot go to another task (the thread rule), and where it lies:
/// `None` when all of it can. `seen`: the structs and enums being looked at, so that a type
/// that holds itself through a pointer ends.
fn stays(ty: &Ty, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>, names: &Names, seen: &mut Vec<String>) -> Option<(Ty, String)> {
    stays_in(ty, structs, enums, names, seen, false)
}

/// `stays`, inside a type written with `: unsafe Sendable` when `promised`: there a raw pointer
/// goes to another task, as its writer has promised.
fn stays_in(ty: &Ty, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>, names: &Names, seen: &mut Vec<String>, promised: bool) -> Option<(Ty, String)> {
    let stays = |ty: &Ty, seen: &mut Vec<String>| stays_in(ty, structs, enums, names, seen, promised);
    match ty {
        Ty::Read(_) | Ty::Ref(_) | Ty::Slice(_) | Ty::TextIter(_) | Ty::Reader(true) | Ty::Entry(..) | Ty::Formatter | Ty::Task(_) | Ty::Group => Some((ty.clone(), String::new())),
        Ty::Raw(_) if promised => None,
        Ty::Shared(_, true) | Ty::Weak(_, true) | Ty::Raw(_) => Some((ty.clone(), String::new())),
        Ty::Shared(inner, false) | Ty::Weak(inner, false) | Ty::Locked(inner) | Ty::Atomic(inner) => stays(inner, seen),
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) | Ty::Array(inner, _) | Ty::Channel(inner) | Ty::Sender(inner) => stays(inner, seen),
        Ty::Result(a, b) | Ty::Map(a, b) => stays(a, seen).or_else(|| stays(b, seen)),
        Ty::Struct(name) | Ty::Enum(name) => {
            let promised = promised || names.sendable.contains(name);
            if names.carriers.contains_key(name) {
                return Some((ty.clone(), String::new()));
            }
            if seen.contains(name) {
                return None;
            }
            seen.push(name.clone());
            let fields: Vec<&(String, Ty)> = match (structs.get(name), enums.get(name)) {
                (Some(info), _) => info.fields.iter().collect(),
                (_, Some(info)) => info.variants.iter().flat_map(|(_, fields)| fields).collect(),
                _ => Vec::new(),
            };
            let found = fields.iter().find_map(|(field, field_ty)| {
                stays_in(field_ty, structs, enums, names, seen, promised).map(|(part, at)| match at.is_empty() {
                    true => (part, format!(" (the field `{field}` of `{}`)", short_name(name))),
                    false => (part, at),
                })
            });
            seen.pop();
            found
        }
        _ => None,
    }
}

/// Whether a `StreamWriter` lies in the type: one left in a channel would be closed where the
/// channel ends, on a thread that may run async tasks.
fn holds_writer(ty: &Ty, structs: &HashMap<String, StructInfo>, enums: &HashMap<String, EnumInfo>, seen: &mut Vec<String>) -> bool {
    match ty {
        Ty::Writer => true,
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) | Ty::Array(inner, _) | Ty::Shared(inner, _) | Ty::Locked(inner) | Ty::Channel(inner) | Ty::Sender(inner) => {
            holds_writer(inner, structs, enums, seen)
        }
        Ty::Result(a, b) | Ty::Map(a, b) => holds_writer(a, structs, enums, seen) || holds_writer(b, structs, enums, seen),
        Ty::Struct(name) | Ty::Enum(name) => {
            if seen.contains(name) {
                return false;
            }
            seen.push(name.clone());
            let fields: Vec<&(String, Ty)> = match (structs.get(name), enums.get(name)) {
                (Some(info), _) => info.fields.iter().collect(),
                (_, Some(info)) => info.variants.iter().flat_map(|(_, fields)| fields).collect(),
                _ => Vec::new(),
            };
            fields.iter().any(|(_, field)| holds_writer(field, structs, enums, seen))
        }
        _ => false,
    }
}

/// Why the part `stays` found does not go to another task.
fn why_it_stays(part: &Ty, names: &Names) -> String {
    match part {
        Ty::Shared(..) | Ty::Weak(..) => format!("a `{part}` is checked within one task; one that several tasks change is `shared locked`"),
        Ty::Task(_) => "a task is waited for by the task that started it, and by no other".to_string(),
        Ty::Group => "a group of tasks is waited for by the task that made it, and by no other".to_string(),
        Ty::Raw(_) => "the checker does not follow a raw pointer, and two tasks could reach what it points at at once; a struct that keeps what it reaches safe for several tasks says so with `: unsafe Sendable`".to_string(),
        Ty::Reader(true) => "this `StreamReader` reads a connection it borrows, and the task could outlive the connection".to_string(),
        Ty::Struct(name) | Ty::Enum(name) if names.carriers.contains_key(name) => format!("a `{part}` carries a borrow, and the task could outlive what it borrows from"),
        _ => format!("a `{part}` is a borrow, and the task could outlive what it borrows from"),
    }
}

/// `byte[]` written without `read` or `ref`: a slice owns nothing, so it is not a type alone.
fn bare_slice(span: Span, slice: &Ty) -> Diag {
    let Ty::Slice(item) = slice else { unreachable!("called for a slice") };
    Diag::error(span, format!("`{slice}` is not a type on its own: a slice only looks at items that something else owns"))
        .hint(format!("`List<{item}>` to own the items, `read {slice}` for a window onto them"))
}

/// The level of `Gzip.compress(data, level)` and of the other formats: one written in the program
/// is 0 to 9, or a compile error (choice 1484); one made when the program runs is checked then.
fn level_written(arg: &Expr) -> Res<()> {
    let level = match &unwidened(arg).kind {
        ExprKind::Unary(UnOp::Neg, inner) => known_int(inner).map(|value| -value),
        _ => known_int(arg),
    };
    match level {
        Some(level) if !(0..=9).contains(&level) => Err(Diag::error(arg.span, format!("compression level {level}: a level is 0 (stored as it is) to 9 (smallest)"))
            .hint("6 is zlib's default, 1 the fastest that compresses")),
        _ => Ok(()),
    }
}

/// The number an index is known to be before the program runs.
fn known_int(expr: &Expr) -> Option<i128> {
    match &expr.kind {
        ExprKind::Int(value, _) => Some(*value),
        ExprKind::Widen(_, inner) => known_int(inner),
        _ => None,
    }
}

/// The expression under the widenings the checker put around it.
fn unwidened(expr: &Expr) -> &Expr {
    match &expr.kind {
        ExprKind::Widen(_, inner) => unwidened(inner),
        _ => expr,
    }
}

/// How many items a window is known to have before the program runs: an array (`read
/// buffer`), `xs[..8]`, `xs[2..10]`, and `xs[at..at + 8]` where `at` is a variable or a field
/// of one, which gives the same value at both ends.
fn known_width(src: &str, expr: &Expr) -> Option<i128> {
    match &expr.kind {
        ExprKind::AsSlice(inner) | ExprKind::Borrow { expr: inner, .. } => known_width(src, inner),
        ExprKind::Slice { recv, from, to } => {
            let length = match recv.ty.strip_borrow() {
                Ty::Array(_, length) => Some(*length as i128),
                _ => None,
            };
            let start = match from {
                None => Some(0),
                Some(from) => known_int(from),
            };
            let end = match to {
                None => length,
                Some(to) => known_int(to),
            };
            if let (Some(start), Some(end)) = (start, end) {
                return Some(end - start);
            }
            let (Some(from), Some(to)) = (from, to) else { return None };
            let (from, to) = (unwidened(from), unwidened(to));
            let ExprKind::Binary(BinOp::Add, left, right) = &to.kind else { return None };
            let left = unwidened(left);
            let same = steady(from) && src[left.span.start..left.span.end] == src[from.span.start..from.span.end];
            if same { known_int(right) } else { None }
        }
        _ => match expr.ty.strip_borrow() {
            Ty::Array(_, length) => Some(*length as i128),
            _ => None,
        },
    }
}

/// A variable, or a field of one: it gives the same value each time it is read in one
/// expression.
fn steady(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Var(_) | ExprKind::Static(_) => true,
        ExprKind::Field { recv, .. } => steady(recv),
        _ => false,
    }
}

/// The function or the method that reads or writes a number as bytes, for a name that is
/// not one (`from_le_bytes`, `fromBytes`, `toBytes`).
fn endian_hint(name: &str, from: bool) -> Option<&'static str> {
    let lower = name.to_lowercase().replace('_', "");
    if !(lower.contains("endian") || lower.contains("bytes")) {
        return None;
    }
    let big = lower.contains("big") || lower.contains("bebytes");
    Some(match (from, big) {
        (true, false) => "fromLittleEndian",
        (true, true) => "fromBigEndian",
        (false, false) => "toLittleEndian",
        (false, true) => "toBigEndian",
    })
}

/// `public int compare(read this, read Block other) { return ((byte)this).compare((byte)other) }`:
/// the order of an enum with numbers that writes none.
fn number_compare(decl: &EnumDecl, numbers: IntTy) -> Func {
    let span = decl.name_span;
    let ty = |mode: Option<Mode>, name: &str| TypeExpr { mode, holder: None, checked: false, locked: false, atomic: None, raw: 0, name: name.to_string(), args: Vec::new(), suffixes: Vec::new(), span };
    let expr = |kind: ExprKind| Expr { kind, span, ty: Ty::Void, moved: false };
    let number = |name: &str| expr(ExprKind::Cast { ty: Ty::Int(numbers), expr: Box::new(expr(ExprKind::Var(name.to_string()))) });
    let compared = expr(ExprKind::Method { recv: Box::new(number("this")), name: "compare".to_string(), name_span: span, args: vec![number("other")], callee: Callee::Plain });
    Func {
        public: true,
        is_async: false,
        export: false,
        ret: ty(None, "int"),
        name: "compare".to_string(),
        name_span: span,
        this: Some(This::Read),
        this_ty: None,
        params: vec![Param { ty: ty(Some(Mode::Read), short_name(&decl.name)), name: "other".to_string(), span, default: None }],
        from: Vec::new(),
        body: vec![Stmt { kind: StmtKind::Return(Some(compared)), span }],
        foreign: None,
        variadic: false,
    }
}

/// The numbers of the variants of an enum with numbers, in the order they are written: the
/// const written for a variant, or one more than the variant above it, the first 0.
pub fn variant_numbers(decl: &EnumDecl, values: &std::collections::BTreeMap<String, (Ty, ConstVal)>) -> Vec<i128> {
    let mut numbers: Vec<i128> = Vec::new();
    for variant in &decl.variants {
        numbers.push(match &variant.number {
            Some(full) => match values.get(full) {
                Some((_, ConstVal::Int(_, number))) => *number,
                _ => unreachable!("the number of a variant is found with the consts"),
            },
            None => numbers.last().map_or(0, |above| above + 1),
        });
    }
    numbers
}

/// The numbers of an enum are found before any function is checked, each fits its type, and
/// no two variants have the same.
fn variant_numbers_hold(prog: &Program) -> Res<()> {
    for decl in prog.enums.iter().filter(|decl| decl.numbers.is_some()) {
        let (ty, _) = decl.numbers.expect("filtered");
        let numbers = variant_numbers(decl, &prog.const_values);
        for (at, (variant, number)) in decl.variants.iter().zip(&numbers).enumerate() {
            if !ty.fits(*number) {
                return Err(Diag::error(variant.span, format!("{number} does not fit in {}", ty.name()))
                    .note("a variant without a number has one more than the variant above it"));
            }
            if let Some(same) = numbers[..at].iter().position(|other| other == number) {
                return Err(Diag::error(variant.span, format!("`{}` has the number {number}, which `{}` has already", variant.name, decl.variants[same].name))
                    .note("a number stands for one variant"));
            }
        }
    }
    Ok(())
}

/// The method the checker puts around a value of an enum with numbers that is cast to an
/// integer: it gives the number of the variant. No program can write the name.
pub const NUMBER_OF: &str = "#number";

/// The methods on the bits of an integer (C23's `<stdbit.h>`).
pub const BIT_METHODS: [&str; 13] = [
    "countOnes", "countZeros", "leadingZeros", "trailingZeros", "leadingOnes", "trailingOnes", "rotateLeft", "rotateRight",
    "reverseBytes", "reverseBits", "isPowerOfTwo", "nextPowerOfTwo", "log2",
];

/// The fractional type a number is expected to have, behind a borrow or a `?` too.
fn wanted_float(ty: &Ty) -> Option<FloatTy> {
    match ty.strip_borrow() {
        Ty::Float(ty) => Some(*ty),
        Ty::Nullable(inner) => wanted_float(inner),
        _ => None,
    }
}

/// A fractional number written in the program must not turn into infinity or into zero
/// in its type: `1e400` is an error, as in C# and Rust.
fn fractional_literal(text: &str, ty: FloatTy, span: Span) -> Res<()> {
    let (infinite, zero) = match ty {
        FloatTy::Float => text.parse::<f32>().map(|v| (v.is_infinite(), v == 0.0)),
        FloatTy::Double => text.parse::<f64>().map(|v| (v.is_infinite(), v == 0.0)),
    }
    .expect("the lexer read it");
    let mantissa = text.split('e').next().unwrap_or(text);
    if infinite {
        return Err(Diag::error(span, format!("{text} does not fit in {}", ty.name())));
    }
    if zero && mantissa.bytes().any(|c| matches!(c, b'1'..=b'9')) {
        return Err(Diag::error(span, format!("{text} is too small for {}: it would be 0", ty.name())));
    }
    Ok(())
}

/// Whether a fractional number, cut to its whole part, fits in the integer type.
pub fn float_fits(value: f64, ty: IntTy) -> bool {
    let value = value.trunc();
    !value.is_nan() && value >= ty.min() as f64 && value <= ty.max() as f64 && (value as i128) <= ty.max()
}

/// The signed type that holds every value of an unsigned one: what `-` needs first.
fn signed_holding(ty: IntTy) -> &'static str {
    match ty {
        IntTy::Byte | IntTy::Ushort => "int",
        _ => "long",
    }
}

/// The integer type of a side, `int` for one that is not an integer.
fn int_of(ty: &Ty) -> IntTy {
    match ty.strip_borrow() {
        Ty::Int(int) => *int,
        _ => IntTy::Int,
    }
}

/// The note and hint for a number written in the program that is too large for C's `long`
/// on Windows, though not on Linux or macOS.
fn c_too_large(diag: Diag, ty: IntTy, written: &str) -> Diag {
    let partner = c_partner(ty).name();
    diag.note(format!("C's `{}` is 32 bits on Windows, so a number written for it fits in 32 bits on every system", c_spelling(ty)))
        .hint(format!("a larger value is converted where the program runs, and panics where it does not fit: `{partner} n = {written}` and then `({})n`", ty.name()))
}

/// How C writes `CLong` and `CULong`.
fn c_spelling(ty: IntTy) -> &'static str {
    if ty.signed() { "long" } else { "unsigned long" }
}

/// The type of SG that holds every value of C's `long` on every system: where its arithmetic
/// is done.
fn c_partner(ty: IntTy) -> IntTy {
    if ty.signed() { IntTy::Long } else { IntTy::Ulong }
}

/// The error for arithmetic on C's `long`: a sum that overflows on Windows and not on Linux
/// is not written. The hint does it in `long` or `ulong`.
fn c_arithmetic(src: &str, symbol: &str, ty: IntTy, sides: &[(&Expr, IntTy)], span: Span) -> Diag {
    let partner = c_partner(ty).name();
    let texts: Vec<String> = sides
        .iter()
        .map(|(side, found)| {
            let text = &src[side.span.start..side.span.end];
            match found.is_c() && !matches!(side.kind, ExprKind::Int(..)) {
                true => format!("({partner}){text}"),
                false => text.to_string(),
            }
        })
        .collect();
    let written = match texts.as_slice() {
        [one] => format!("{symbol}{one}"),
        [a, b] => format!("{a} {symbol} {b}"),
        _ => unreachable!("one or two sides"),
    };
    Diag::error(span, format!("operator `{symbol}` is not defined for `{}`: C's `{}` is 32 bits on Windows and 64 elsewhere, so the same sum would overflow on one system only", ty.name(), c_spelling(ty)))
        .hint(format!("compute in `{partner}`: `{written}`"))
}

/// A shift count that is known before the program runs and can never be right is not left
/// to run time.
fn shift_count(ty: IntTy, count: &Expr) -> Res<()> {
    let bits = ty.bits() as i128;
    match known_int(count).filter(|by| *by < 0 || *by >= bits) {
        Some(by) => Err(Diag::error(count.span, format!("a shift by {by} is out of range: `{}` has {bits} bits", ty.name()))
            .hint(format!("the count goes from 0 to {}", bits - 1))),
        None => Ok(()),
    }
}

/// Whether a borrow of `from` (a list or an array) can stand where a slice `to` is expected.
fn as_slice(from: &Ty, to: &Ty) -> bool {
    match (from, to) {
        (Ty::List(item) | Ty::Array(item, _), Ty::Slice(wanted)) => item == wanted,
        _ => false,
    }
}

fn not_a_key(span: Span, ty: &Ty, what: &str) -> Diag {
    if let Ty::Shared(..) | Ty::Weak(..) = ty {
        return Diag::error(span, format!("`{ty}` cannot be {what}: it is not `Hashable`"))
            .hint("keep something that tells the values apart instead, such as an `id` field: `HashSet<int>`");
    }
    if let Ty::Float(_) = ty {
        return Diag::error(span, format!("`{ty}` cannot be {what}: it is not `Hashable`"))
            .note("a fractional number is not `Hashable`: `0.0` and `-0.0` are equal and have different bits, and a sum seldom gives the exact key back");
    }
    let diag = Diag::error(span, format!("`{ty}` cannot be {what}: it is not `Equatable` and `Hashable`"));
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => diag.hint(format!("add them to the definition: `{name} : Equatable, Hashable`")),
        Ty::List(_) => diag.note("a list is a key when what it holds is: integers, `bool`, `char`, `string` and the types written with `: Equatable, Hashable`"),
        Ty::Set(_) | Ty::Map(..) => diag.note("a set or a map can be compared with `==`, but it is not `Hashable`; a `List` is"),
        _ => diag.note("integers, `bool`, `char`, `string` and the types written with `: Equatable, Hashable` are"),
    }
}

/// The functions written in the body of a struct or an enum. `taken` are the names of its
/// fields or its variants, which a function cannot have.
fn type_methods<'t>(
    owner: &str,
    written: &[Func],
    taken: impl Iterator<Item = &'t str> + Clone,
    what: &str,
    names: &Names,
    formattable: &HashSet<String>,
) -> Res<HashMap<String, FuncSig>> {
    let mut methods: HashMap<String, FuncSig> = HashMap::new();
    for method in written {
        if method.name == "replace" {
            return Err(Diag::error(method.name_span, "`replace` is built in for every type and cannot be defined"));
        }
        if taken.clone().any(|name| name == method.name) {
            return Err(Diag::error(method.name_span, format!("`{}` is already {what} of `{owner}`", method.name)));
        }
        if methods.insert(method.name.clone(), func_sig(method, names, Some(owner))?).is_some() {
            return Err(Diag::error(method.name_span, format!("method `{}` is defined twice", method.name)));
        }
        // The body of `Formattable`, when the type writes its own.
        if method.name == "format" && formattable.contains(owner) {
            let sig = &methods["format"];
            let fits = sig.this == Some(This::Read) && sig.ret == Ty::Void && sig.params == [Ty::Ref(Box::new(Ty::Formatter))];
            if !fits {
                return Err(Diag::error(method.name_span, "this is not the `format` that `Formattable` asks for")
                    .hint("`void format(read this, ref Formatter f)`"));
            }
        }
    }
    Ok(methods)
}

/// The types that said each interface after `:`.
struct Promised<'s> {
    clonable: &'s HashSet<String>,
    copyable: &'s HashSet<String>,
    formattable: &'s HashSet<String>,
    equatable: &'s HashSet<String>,
    keys: &'s HashSet<String>,
    comparable: &'s HashSet<String>,
}

/// What a struct or an enum promises after `:` holds for everything it carries, and the
/// methods it writes itself for an interface (`format`, `equals`, `compare`) are the ones
/// the interface asks for.
fn promises<'d, I>(
    name: &str,
    name_span: Span,
    this_ty: Ty,
    written: &[Func],
    methods: &HashMap<String, FuncSig>,
    fields: impl Fn() -> I,
    sets: &Promised,
) -> Res<()>
where
    I: Iterator<Item = (&'d FieldDecl, &'d (String, Ty))>,
{
    if sets.copyable.contains(name) {
        all_copyable(name, fields(), sets.copyable)?;
    }
    if sets.formattable.contains(name) && !methods.contains_key("format") {
        all_formattable(name, fields(), sets.formattable)?;
    }
    let own = |method: &str| written.iter().find(|func| func.name == method);
    // `equals` and `compare` look at this value and another one of the same type.
    let takes_other = |method: &str| {
        let sig = &methods[method];
        sig.this == Some(This::Read) && sig.params == [Ty::Read(Box::new(this_ty.clone()))]
    };
    if sets.equatable.contains(name) {
        match own("equals") {
            Some(func) if !takes_other("equals") || methods["equals"].ret != Ty::Bool => {
                return Err(Diag::error(func.name_span, "this is not the `equals` that `Equatable` asks for")
                    .hint(format!("`bool equals(read this, read {name} other)`")));
            }
            Some(_) => {}
            None => all_have(name, fields(), "Equatable", sets.equatable)?,
        }
    }
    if sets.clonable.contains(name) {
        match own("clone") {
            Some(func) => {
                let sig = &methods["clone"];
                if sig.this != Some(This::Read) || !sig.params.is_empty() || sig.ret != this_ty {
                    return Err(Diag::error(func.name_span, "this is not the `clone` that `Clonable` asks for")
                        .hint(format!("`{name} clone(read this)`, or no `clone` at all: then it is made field by field")));
                }
            }
            None => all_clonable(name, fields(), sets)?,
        }
    }
    if sets.keys.contains(name) {
        if !sets.equatable.contains(name) {
            return Err(Diag::error(name_span, format!("`{name}` is `Hashable`, so it must be `Equatable` too: a key is found by its hash and then compared"))
                .hint(format!("`{name} : Equatable, Hashable`")));
        }
        if let Some(func) = own("hash").or_else(|| own("equals")) {
            return Err(Diag::error(func.name_span, "a `Hashable` type with its own `equals` or `hash` is not in the interpreter yet")
                .note("leave both out: they are made from the fields"));
        }
        all_have(name, fields(), "Hashable", sets.keys)?;
    }
    if sets.comparable.contains(name) {
        match own("compare") {
            None => {
                return Err(Diag::error(name_span, format!("`{name}` is `Comparable`, so it needs a `compare` method"))
                    .hint(format!("`int compare(read this, read {name} other)`: negative if this one comes first, 0 if they are equal")));
            }
            Some(func) if !takes_other("compare") || methods["compare"].ret != Ty::Int(IntTy::Int) => {
                return Err(Diag::error(func.name_span, "this is not the `compare` that `Comparable` asks for")
                    .hint(format!("`int compare(read this, read {name} other)`")));
            }
            Some(_) => {}
        }
    }
    Ok(())
}


/// Whether `.clone()` makes a copy of a value of this type (choice 1818): numbers, `bool`, `char`,
/// text, the types written `: Clonable` or `: Copyable`, a `shared` or `weak` value (one more
/// owner or looker, not a copy of what it points to), and a `T*`, a `T?`, a collection or an
/// array of those.
fn can_clone(ty: &Ty, clonable: &HashSet<String>, copyable: &HashSet<String>) -> bool {
    match ty {
        Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char | Ty::Str => true,
        Ty::Struct(name) | Ty::Enum(name) => clonable.contains(name) || ty.is_copyable(copyable),
        Ty::Shared(..) | Ty::Weak(..) => true,
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) | Ty::Array(inner, _) => can_clone(inner, clonable, copyable),
        Ty::Map(key, value) => can_clone(key, clonable, copyable) && can_clone(value, clonable, copyable),
        Ty::Result(ok, error) => can_clone(ok, clonable, copyable) && can_clone(error, clonable, copyable),
        Ty::Void => true,
        _ => false,
    }
}

/// `: Clonable` without a `clone` of its own is made field by field, so every field can be cloned.
fn all_clonable<'d>(name: &str, fields: impl Iterator<Item = (&'d FieldDecl, &'d (String, Ty))>, sets: &Promised) -> Res<()> {
    for (field, (_, ty)) in fields {
        if can_clone(ty, sets.clonable, sets.copyable) {
            continue;
        }
        let diag = Diag::error(field.ty.span, format!("`{name}` cannot be `Clonable` field by field: `{}` is a `{ty}`, which `clone` cannot copy", field.name));
        return Err(match lacking(ty, sets.clonable) {
            Some(inner) if !matches!(ty, Ty::Read(_) | Ty::Ref(_)) => diag.hint(format!("make `{inner}` `Clonable` too")),
            _ => match ty {
                Ty::Read(_) | Ty::Ref(_) => diag.note("a borrow is not copied: the copy would borrow from what the original does"),
                Ty::Atomic(_) => diag.note("a copy of an atomic value would be a second value, which the tasks that change the first never change"),
                _ => diag.note("a task, a stream, a process and the like have one owner; they are not copied"),
            },
        });
    }
    Ok(())
}

/// `: Copyable` holds only if everything the type carries can be copied.
fn all_copyable<'d>(name: &str, fields: impl Iterator<Item = (&'d FieldDecl, &'d (String, Ty))>, copyable: &HashSet<String>) -> Res<()> {
    for (field, (_, ty)) in fields {
        if ty.is_copyable(copyable) {
            continue;
        }
        let diag = Diag::error(field.ty.span, format!("`{name}` cannot be `Copyable`: its field `{}` is a `{ty}`, and a `{ty}` is not `Copyable`", field.name));
        return Err(match ty {
            Ty::Atomic(_) => diag.note("a copy of an atomic value would be a second value, which the tasks that change the first never change"),
            Ty::Struct(inner) | Ty::Enum(inner) => diag.hint(format!("make `{inner}` `Copyable` too, or take `Copyable` off `{name}`")),
            Ty::Str | Ty::Ptr(_) | Ty::List(_) | Ty::Map(..) | Ty::Set(_) => diag.note(format!("a `{ty}` owns memory on the heap; a copy would have to allocate, and that is written `.clone()`")),
            Ty::Shared(..) | Ty::Weak(..) => diag.note(format!("a `{ty}` counts its owners; one more is made with `share`, so that the count changes where it can be seen")),
            _ => diag,
        });
    }
    Ok(())
}

/// Whether a value of this type is `Equatable` (or `Hashable`): numbers, `bool`, text, the
/// types in `declared`, and a `T*` or `T?` of those. With `collections`, a collection counts
/// if what it holds does: that is so for `Equatable`; for `Hashable`, only a `List` is
/// (choice 1853), looked up by a slice of what it holds.
///
/// A fractional number is `Equatable` and not `Hashable`, like a collection: two values that
/// `equals` calls the same, `0.0` and `-0.0`, have different bits.
fn has(ty: &Ty, declared: &HashSet<String>, collections: bool) -> bool {
    match ty {
        Ty::Int(_) | Ty::Bool | Ty::Char | Ty::Str => true,
        Ty::Float(_) => collections,
        Ty::Struct(name) | Ty::Enum(name) => declared.contains(name),
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::Read(inner) | Ty::Ref(inner) => has(inner, declared, collections),
        // `==` looks at what a `shared` value points to; a key it is not, as a collection is not.
        Ty::Shared(inner, _) => collections && has(inner, declared, collections),
        Ty::List(item) => has(item, declared, collections),
        Ty::Set(item) | Ty::Array(item, _) | Ty::Slice(item) => collections && has(item, declared, collections),
        Ty::Map(key, value) => collections && has(key, declared, collections) && has(value, declared, collections),
        _ => false,
    }
}

/// The struct or enum inside `ty` that is not in `declared`.
fn lacking<'t>(ty: &'t Ty, declared: &HashSet<String>) -> Option<&'t str> {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => (!declared.contains(name)).then_some(name.as_str()),
        Ty::Ptr(inner) | Ty::Shared(inner, _) | Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) | Ty::Array(inner, _) | Ty::Slice(inner) => lacking(inner, declared),
        Ty::Map(key, value) => lacking(key, declared).or_else(|| lacking(value, declared)),
        _ => None,
    }
}

/// `: Equatable` and `: Hashable` without a body are made field by field, so every field
/// must have them.
fn all_have<'d>(name: &str, fields: impl Iterator<Item = (&'d FieldDecl, &'d (String, Ty))>, word: &str, declared: &HashSet<String>) -> Res<()> {
    for (field, (_, ty)) in fields {
        if has(ty, declared, word == "Equatable") {
            continue;
        }
        let diag = Diag::error(field.ty.span, format!("`{name}` cannot be `{word}` field by field: `{}` is a `{ty}`, which is not `{word}`", field.name));
        return Err(match (ty, lacking(ty, declared)) {
            (Ty::Set(_) | Ty::Map(..), _) if word == "Hashable" => diag.note("a set or a map can be compared with `==`, but it is not a key; a `List` is"),
            (Ty::Shared(..), _) if word == "Hashable" => diag.note("a `shared` value can be compared with `==`, but it is not a key"),
            (Ty::Weak(..), _) => diag.note("a `weak` value may point to nothing any more, so it has no content to compare; `same(share a, share b)` tells whether two point to one object"),
            (Ty::Float(_), _) => diag.note("a fractional number is not `Hashable`"),
            (_, Some(inner)) => diag.hint(format!("make `{inner}` `{word}` too")),
            _ => diag,
        });
    }
    Ok(())
}

/// The type behind borrows and pointers.
fn peel(mut ty: &Ty) -> &Ty {
    while let Ty::Read(inner) | Ty::Ref(inner) | Ty::Ptr(inner) | Ty::Shared(inner, _) = ty {
        ty = inner;
    }
    ty
}

/// Without a `format` of its own, a `Formattable` type is written field by field.
fn all_formattable<'d>(name: &str, fields: impl Iterator<Item = (&'d FieldDecl, &'d (String, Ty))>, formattable: &HashSet<String>) -> Res<()> {
    for (field, (_, ty)) in fields {
        if can_format(ty, formattable) {
            continue;
        }
        let article = if matches!(ty, Ty::Atomic(_)) { "an" } else { "a" };
        let diag = Diag::error(field.ty.span, format!("`{name}` cannot be written as text field by field: `{}` is {article} `{ty}`, which is not `Formattable`", field.name));
        let mut inner = ty;
        while let Ty::Nullable(behind) | Ty::List(behind) | Ty::Set(behind) | Ty::Array(behind, _) = inner {
            inner = behind;
        }
        if let Ty::Weak(..) = inner {
            return Err(diag
                .note("what a `weak` value looks at may be gone, and following it may come back to where it started")
                .hint("write `format` yourself, with the fields that say what the value is"));
        }
        return Err(match unformattable(ty, formattable) {
            Some(inner) => diag.hint(format!("make `{inner}` `Formattable` too, or write `format` yourself")),
            None => diag,
        });
    }
    Ok(())
}

/// Whether a value of this type can be written as text, on its own or inside another value.
fn can_format(ty: &Ty, formattable: &HashSet<String>) -> bool {
    match ty {
        Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char | Ty::Str | Ty::Void => true,
        Ty::Struct(_) | Ty::Enum(_) => unformattable(ty, formattable).is_none(),
        Ty::Ptr(inner) | Ty::Shared(inner, _) | Ty::Nullable(inner) | Ty::Read(inner) | Ty::Ref(inner) => can_format(inner, formattable),
        Ty::Result(ok, error) | Ty::Map(ok, error) => can_format(ok, formattable) && can_format(error, formattable),
        Ty::List(item) | Ty::Set(item) | Ty::Array(item, _) | Ty::Slice(item) => can_format(item, formattable),
        _ => false,
    }
}

/// The struct or enum inside `ty` that stands in the way of writing it as text.
fn unformattable<'t>(ty: &'t Ty, formattable: &HashSet<String>) -> Option<&'t str> {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => (!formattable.contains(name)).then_some(name.as_str()),
        Ty::Ptr(inner) | Ty::Shared(inner, _) | Ty::Nullable(inner) | Ty::Read(inner) | Ty::Ref(inner) => unformattable(inner, formattable),
        Ty::Result(ok, error) | Ty::Map(ok, error) => unformattable(ok, formattable).or_else(|| unformattable(error, formattable)),
        Ty::List(item) | Ty::Set(item) | Ty::Array(item, _) | Ty::Slice(item) => unformattable(item, formattable),
        _ => None,
    }
}

/// The structs and enums a value of `ty` holds in itself, those of both sides of a `Result` too
/// (1596): what its size is measured from (`value_layout`). A borrow, a pointer and what lies on
/// the heap are not held so.
fn by_value(ty: &Ty) -> Vec<&str> {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => vec![name],
        Ty::Nullable(inner) | Ty::Array(inner, _) | Ty::Atomic(inner) | Ty::Locked(inner) => by_value(inner),
        Ty::Result(ok, error) | Ty::Entry(ok, error) => [by_value(ok), by_value(error)].concat(),
        _ => Vec::new(),
    }
}

/// A type that holds itself by value would have no finite size.
fn contains_itself<'d>(
    kind: &str,
    name: &str,
    fields: impl Iterator<Item = (&'d FieldDecl, &'d (String, Ty))>,
    members: &HashMap<&str, Vec<&Ty>>,
) -> Res<()> {
    fn reaches(from: &str, target: &str, members: &HashMap<&str, Vec<&Ty>>, seen: &mut Vec<String>) -> bool {
        if seen.iter().any(|name| name == from) {
            return false;
        }
        seen.push(from.to_string());
        members[from].iter().any(|ty| by_value(ty).into_iter().any(|inner| inner == target || reaches(inner, target, members, seen)))
    }
    for (field, (_, ty)) in fields {
        let Some(inner) = by_value(ty).into_iter().find(|inner| *inner == name || reaches(inner, name, members, &mut Vec::new())) else { continue };
        {
            let written = ty.to_string();
            let pointer = written.replacen(inner, &format!("{inner}*"), 1);
            return Err(Diag::error(field.ty.span, format!("{kind} `{name}` contains itself, so it has no finite size"))
                .hint(format!("put it behind a pointer: `{pointer} {}`", field.name)));
        }
    }
    Ok(())
}

/// The root of a loan on a value that has no name: it is gone when its statement ends.
const UNNAMED: usize = usize::MAX;

/// Not a variable: among the loans of the names a pattern or a loop gives, it says that they
/// look into a `shared` value for their statement only, as the names of `lock` do (choice 1592):
/// another owner may change the value after it.
const OPENS: usize = usize::MAX - 1;

/// The mark `OPENS`, for the names of a statement over `place`, if it is reached through a
/// `shared` value.
fn opens_shared(place: &Expr) -> Option<Loan> {
    (through_holder(place, false) || through_holder(place, true)).then(|| Loan { root: OPENS, path: String::new(), mutable: false, held: false, span: place.span })
}

/// Whether the expression gives a value that nothing owns after the statement: what a call
/// returned, a value being built. A text constant is there for the whole run, and a borrow
/// is judged by what it borrowed.
/// What a place starts at: the value its fields and items are in (`wrap(x)` in `wrap(x).text`).
fn base_of(place: &Expr) -> &Expr {
    let mut base = place;
    while let ExprKind::Field { recv, .. } | ExprKind::Index { recv, .. } = &base.kind {
        base = recv;
    }
    base
}

fn unnamed_owner(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Str(_) | ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::FuncRef(_) | ExprKind::CVar(_) | ExprKind::Const(_) | ExprKind::Static(_) => false,
        ExprKind::Borrow { .. } | ExprKind::Slice { .. } | ExprKind::Unbox(_) | ExprKind::AsSlice(_) => false,
        _ => !expr.ty.is_borrow() && !matches!(expr.ty, Ty::Void | Ty::Never | Ty::Null),
    }
}

/// What a variable that holds a borrow has borrowed: the place it was lent from.
#[derive(Clone)]
struct Loan {
    /// The variable the place starts at.
    root: usize,
    /// The fields that lead from the variable to the place.
    path: String,
    /// Lent as `ref`.
    mutable: bool,
    /// Not a borrow of `root` itself but of what `root` holds: it lives as long as the
    /// source of `root`, and `root` stays free.
    held: bool,
    span: Span,
}

/// Whether a place is an item of a collection, or lies inside one.
fn reaches_item(place: &Expr) -> bool {
    match &place.kind {
        ExprKind::Index { .. } => true,
        ExprKind::Field { recv, .. } => reaches_item(recv),
        _ => false,
    }
}

/// A local list of borrows, which `clear()` lets go of (1733).
fn clears(var: &Var) -> bool {
    !var.param && matches!(&var.declared, Ty::List(item) if item.is_borrow())
}

/// Whether two loans lend the same place in the same way.
fn same_loan(a: &Loan, b: &Loan) -> bool {
    a.root == b.root && a.path == b.path && a.mutable == b.mutable && a.held == b.held
}

/// Where in the function something happens: its position, the branches of `if` and `match`
/// it is in, and the loops around it.
#[derive(Clone, Default)]
struct Point {
    pos: usize,
    /// The order in which the checker came to it: it decides between two points at the same
    /// position (the parts of the place an assignment writes, counted after its right side).
    seq: usize,
    /// In the place an assignment writes, which counts as after the right side: where that
    /// right side starts.
    placed: Option<usize>,
    /// For each `if` or `match` around: which one, and which branch of it.
    branches: Vec<(usize, usize)>,
    loops: Vec<usize>,
}

/// How a place is touched.
#[derive(Clone, Copy, PartialEq)]
enum Access {
    Read,
    Write,
    Move,
    /// The owner reached the end of its block.
    Gone,
}

/// A place touched while a variable held a borrow of it. It is an error only if that variable
/// is used again afterwards, which is known when the whole function has been read.
struct Clash {
    holder: usize,
    holder_name: String,
    /// For an owner that reached the end of its block: its name.
    owner_name: String,
    /// The owner is a name of `lock` or `read ... as`, closed at the end of its block.
    closed: bool,
    /// The place is itself a borrow of a value that can be copied: the type to write to
    /// get a copy of the value instead of a second borrow.
    copy_as: Option<String>,
    /// The loops that were open where the holder was declared.
    holder_loops: Vec<usize>,
    /// The holder is a local list of borrows, which `clear()` lets go of (1733).
    clears: bool,
    loan: Loan,
    access: Access,
    span: Span,
    at: Point,
}

/// What the borrow check collects while a function is checked.
#[derive(Default)]
struct Borrows {
    next_id: usize,
    /// The number of the last point made.
    seq: usize,
    /// Where the function is being read right now.
    branches: Vec<(usize, usize)>,
    loops: Vec<usize>,
    /// Every read of a variable: which one, and where.
    uses: Vec<(usize, Point)>,
    clashes: Vec<Clash>,
    /// What the names of the pattern being declared borrow from.
    binds: Vec<Loan>,
    /// How far the function has been read.
    pos: usize,
    /// Every touch of a place, in the order the checker came to them: the variable, the
    /// fields, how, and where. A call looks here for what its later parts did to what its
    /// earlier parts lent, and a loop for what its next round does to what a round lent.
    touched: Vec<(usize, String, Access, Span, Point)>,
    /// The names about to be declared are those of a `lock` or a `read ... as`.
    opening: bool,
    /// The uses that are the `dispose` of a value at the end of its block, by `Point::seq`.
    disposed: Vec<usize>,
    /// Every `clear` of a local collection that let go of what its items borrowed (1733): the
    /// variable, how many touches came before it, and where.
    released: Vec<(usize, usize, Point)>,
}

struct Var {
    /// Tells this variable from every other one of the function, also one of the same name.
    id: usize,
    /// The loops that were open where it was declared.
    decl_loops: Vec<usize>,
    /// What it has borrowed, if it holds a borrow.
    loans: Vec<Loan>,
    /// A parameter of the function (or `this`).
    param: bool,
    name: String,
    /// Set for a local that holds a `Result` until something looks at it: where it is declared.
    unopened: Option<Span>,
    declared: Ty,
    /// The type right now: `declared`, or narrowed after an `is null` test.
    current: Ty,
    moved: Moved,
    /// Counts assignments, to notice that a narrowing no longer holds.
    assigned: u32,
    /// Where it is declared.
    at: Span,
    /// For a `Sender` made from a channel in this function (`jobs.sender()`, or a `.clone()` of
    /// one), the `id` of the channel's variable: a `for` over that channel while this one lives
    /// would never end.
    sender_of: Option<usize>,
    /// For a channel made here (`var jobs = Channel<T>(n)`) that has no `Sender` yet, where it is
    /// made: until `jobs.sender()`, any other use of it is an error (1860), since a channel no
    /// sender holds has ended, and a receiver given it would find nothing and stop.
    unsent: Option<Span>,
    /// A name `lock` or `read ... as` gives what it opened: what it lends is closed at the
    /// end of the block, as a value that is gone there.
    opened: bool,
}

/// What has been moved out of a variable.
#[derive(Clone, Default)]
struct Moved {
    /// Where the whole value was moved out, if it was.
    whole: Option<Span>,
    /// Fields that were moved out on their own: path (`next`, `first.name`) and where.
    parts: Vec<(String, Span)>,
    /// The whole value was moved out on some of the paths that reach here, not on all:
    /// what tells a `StreamWriter` closed in one branch only.
    partly: bool,
    /// `whole` is a loop `while tasks.length() > 0`, after which a list of tasks is empty:
    /// its tasks count as waited for, and the list as used up.
    emptied: bool,
    /// In a saved state of the function (`Checker::moves`): what the variable borrowed then.
    /// A variable keeps its loans in `Var::loans`, and this stays empty there.
    loans: Vec<Loan>,
}

#[derive(Default)]
struct Scope {
    vars: Vec<Var>,
    /// The consts written in this block so far, as written and by their full names.
    consts: Vec<(String, String)>,
    /// The statics written in this block so far, the same way.
    statics: Vec<(String, String)>,
    /// Narrowings that end with this scope: variable, type before, assignment count at the time.
    narrowed: Vec<(String, Ty, u32)>,
}

type Moves = Vec<Vec<Moved>>;

/// What the checker knows of the consts: the type of each, and once they are found, what each
/// holds.
#[derive(Default)]
struct ConstTable {
    types: HashMap<String, Ty>,
    values: std::collections::BTreeMap<String, (Ty, ConstVal)>,
    /// How many bytes each file of `embed` holds.
    embeds: Vec<usize>,
}

impl ConstTable {
    /// The type a use of the const has: a number, a `bool`, a `char` or a text is a value; an
    /// array, a struct or an enum is a `read` borrow of the one value, which lasts for the run.
    fn use_type(&self, full: &str) -> Ty {
        match &self.types[full] {
            ty @ (Ty::Array(..) | Ty::Struct(_) | Ty::Enum(_)) => Ty::Read(Box::new(ty.clone())),
            ty => ty.clone(),
        }
    }
}

/// A loop being checked.
struct LoopCx {
    /// How many scopes were open where the loop starts: a `break` ends those above.
    depth: usize,
    label: Option<Label>,
    /// Whether a `break` or `continue` names the loop.
    used: bool,
    /// What was moved at each `break` that leaves the loop.
    breaks: Vec<Moves>,
    /// What was moved at each `continue` of the loop, and, once its body is checked, also at
    /// its end if the end is reached: where the rounds after the first start.
    continues: Vec<Moves>,
    /// What was moved when the loop started.
    before: Moves,
}

struct Checker<'a> {
    names: &'a Names,
    /// The calls `Log.debug(text)` and the others already put inside `if Log.isOn(...)`, by
    /// where they start (choice 1447).
    log_guarded: HashSet<usize>,
    consts: &'a ConstTable,
    /// The initializer of a const is being checked: what it is made of is known when the
    /// program is compiled.
    in_const: bool,
    /// Structs and enums written with `: Copyable`.
    copyable: &'a HashSet<String>,
    /// Structs and enums written with `: Formattable`.
    formattable: &'a HashSet<String>,
    /// Structs and enums written with `: Equatable`.
    equatable: &'a HashSet<String>,
    /// Structs and enums written with `: Comparable`.
    comparable: &'a HashSet<String>,
    structs: &'a HashMap<String, StructInfo>,
    enums: &'a HashMap<String, EnumInfo>,
    sigs: &'a HashMap<String, FuncSig>,
    scopes: Vec<Scope>,
    /// The collections the enclosing `for` loops are going over.
    locks: Vec<Lock>,
    borrows: Borrows,
    loops: usize,
    /// The enclosing loops, the innermost last.
    loop_cx: Vec<LoopCx>,
    ret: Ty,
    /// Where a returned borrow may come from.
    sources: Vec<Source>,
    func_name: String,
    /// A `test` block is being checked: `try` passes any error on, and the test fails with it.
    in_test: bool,
    /// Every name an `is` pattern has given, with where and whether it is seen where the test
    /// fails (`is not`): to explain a use of one where it is not seen.
    is_names: Vec<(String, Span, bool)>,
    /// The struct whose method is being checked.
    owner: Option<&'a str>,
    /// The module the code being checked is in; empty for the root.
    module: &'a str,
    src: &'a str,
    /// The function being checked is `async`: it can `await`, and it does not `wait`.
    is_async: bool,
    /// The call being checked is the one an `await` is written in front of: it runs as part
    /// of this task, so what it borrows stays here.
    awaiting: bool,
    /// The `unsafe` being checked, if any: where its word is, and whether anything in it
    /// needed it.
    unsafe_at: Option<(Span, bool)>,
    /// The place being checked is the target of an assignment: an `unsafe` it lacks is asked
    /// for as a block.
    writing: bool,
    /// The place an assignment writes and the places it is reached through, the place first,
    /// while it is found: a field of a union written whole needs no `unsafe`, one written into is
    /// an error, and an index inside them is read.
    written: Vec<Span>,
    /// The expression being checked is an argument of a function of C: `buffer.pointer()` may
    /// be written there.
    extern_arg: bool,
    /// The local variables and what each use of a name was found to be, for the rule of
    /// values that are never read (`unread`).
    seen: crate::unread::Seen,
}

/// A collection that a `for` is going over by borrowing it: nothing may change it until the
/// loop ends.
struct Lock {
    /// The variable it is reached from: scope and position in the scope.
    var: (usize, usize),
    /// The fields that lead from the variable to the collection.
    path: String,
    /// The collection as it is written at the loop.
    text: String,
    /// It is a `List`, so its positions can be gone over instead.
    positions: bool,
    span: Span,
}

/// The variable and fields a write to this place lands in: writing an item of `a.b` changes
/// `a.b`.
fn store_path(expr: &Expr) -> Option<(&str, String)> {
    match &expr.kind {
        ExprKind::Index { recv, .. } => store_path(recv),
        ExprKind::Field { recv, .. } if recv.in_item() => store_path(recv),
        _ => path_of(expr),
    }
}

/// The place a call is lent: that of the items a range looks at (`ref xs[0..2]`), as an item
/// is counted as its whole collection. Two ranges of one place clash however far apart they
/// are, as in Rust.
fn lent_path(expr: &Expr) -> Option<(&str, String)> {
    match &expr.kind {
        ExprKind::Slice { recv, .. } => lent_path(recv),
        _ => store_path(expr),
    }
}

/// A place as a message names it: `xs[0..2]` for the range written `ref xs[0..2]`.
fn range_named(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix("ref").filter(|rest| rest.starts_with(char::is_whitespace)).map_or(text, str::trim_start)
}

/// The variants a `match` or `is` chooses between: those of an enum, or `Ok` and `Error`.
struct Variants {
    /// The type as it is named in messages.
    owner: String,
    /// In declaration order, each with the fields it carries.
    list: Vec<(String, Vec<(String, Ty)>)>,
}

/// Names a pattern brings into the block it guards: name, type, where it is written.
type Binds = Vec<(String, Ty, Span)>;

/// The names of `is` patterns, each group with what its value was lent from.
type Named = Vec<(Binds, Vec<Loan>)>;

/// Where the code that comes after an `is` test sees its names.
#[derive(Clone, Copy)]
enum NamesSeen {
    /// Where the test holds: the right of `&&`, the block of an `if` or `while`.
    Holds,
    /// Where the test fails: the right of `||`, `else`, after an `if` whose block leaves.
    Fails,
    /// Nowhere: an `if` whose block does not leave and has no `else`, tested with `is not`.
    StaysIn,
    /// Nowhere: a `while` tested with `is not`, which this `break` can leave while the test
    /// still holds.
    Break(Span),
    /// Nowhere: an `is` anywhere else.
    No,
}

/// What a condition says about plain variables of a `T?` type: those that are not `null`
/// where it holds, and those that are not `null` where it does not. `x is not null` says `x`
/// where it holds, `x is null` where it does not, and `!` swaps the two. Where `a && b` holds,
/// both sides held; where it does not, only what both sides say when they fail is known.
/// `a || b` is the other way round.
fn null_facts(cond: &Expr) -> (Vec<String>, Vec<String>) {
    match &cond.kind {
        ExprKind::IsNull { expr, negated } => match &expr.kind {
            ExprKind::Var(name) if *negated => (vec![name.clone()], Vec::new()),
            ExprKind::Var(name) => (Vec::new(), vec![name.clone()]),
            // `a?.b is not null` holds only when `a` is there.
            _ => match or_null_root(expr) {
                Some(name) if *negated => (vec![name.to_string()], Vec::new()),
                Some(name) => (Vec::new(), vec![name.to_string()]),
                None => (Vec::new(), Vec::new()),
            },
        },
        ExprKind::Unary(UnOp::Not, inner) => {
            let (holds, fails) = null_facts(inner);
            (fails, holds)
        }
        ExprKind::Binary(op @ (BinOp::And | BinOp::Or), left, right) => {
            let (left_holds, left_fails) = null_facts(left);
            let (right_holds, right_fails) = null_facts(right);
            let both = |one: Vec<String>, other: Vec<String>| one.into_iter().filter(|name| other.contains(name)).collect();
            let either = |mut one: Vec<String>, other: Vec<String>| {
                one.extend(other.into_iter().filter(|name| !one.contains(name)).collect::<Vec<_>>());
                one
            };
            match op {
                BinOp::And => (either(left_holds, right_holds), both(left_fails, right_fails)),
                _ => (both(left_holds, right_holds), either(left_fails, right_fails)),
            }
        }
        _ => (Vec::new(), Vec::new()),
    }
}

/// The name the `match` read from `a?.b` binds, if `arms` are its arms.
fn or_null_held(arms: &[Arm]) -> Option<String> {
    match arms {
        [_, Arm { pattern: Pattern { kind: PatternKind::Bind(name), .. }, .. }] if name.starts_with(OR_NULL) => Some(name.clone()),
        _ => None,
    }
}

/// The variable a chain of `?.` starts from: `a` in `a?.b`, also once the checker lends it.
fn or_null_root(expr: &Expr) -> Option<&str> {
    let ExprKind::Match { subject, arms, .. } = &expr.kind else { return None };
    or_null_held(arms)?;
    match &subject.kind {
        ExprKind::Var(name) => Some(name),
        ExprKind::Borrow { expr: inner, .. } => match &inner.kind {
            ExprKind::Var(name) => Some(name),
            _ => None,
        },
        _ => None,
    }
}

/// The steps of the chain after `?.`, from the one read from `held` on: fields, methods and
/// items. A `?.` further on ends them.
fn chain_steps<'e>(expr: &'e Expr, held: &str, steps: &mut Vec<&'e Expr>) -> bool {
    let inner = match &expr.kind {
        ExprKind::Field { recv, .. } | ExprKind::Method { recv, .. } | ExprKind::Index { recv, .. } | ExprKind::Slice { recv, .. } => recv,
        ExprKind::Match { subject, .. } => return chain_steps(subject, held, steps),
        _ => return false,
    };
    let found = match &inner.kind {
        ExprKind::Var(name) if name == held => true,
        _ => chain_steps(inner, held, steps),
    };
    if found {
        steps.push(expr);
    }
    found
}

/// The type made one that may be `null`: `int` gives `int?`, `read string` gives `read
/// string?`, and a `T?` stays as it is.
fn nullable_of(ty: Ty) -> Ty {
    match ty {
        Ty::Nullable(_) | Ty::Null => ty,
        Ty::Read(inner) => Ty::Read(Box::new(nullable_of(*inner))),
        Ty::Ref(inner) => Ty::Ref(Box::new(nullable_of(*inner))),
        other => Ty::Nullable(Box::new(other)),
    }
}

/// What a whole condition says once it has run, as `null_facts` reads it, without the
/// variables it empties with `take`: when `b is null || b.take() is null` fails, `b` was
/// there and then was emptied.
fn condition_facts(cond: &Expr) -> (Vec<String>, Vec<String>) {
    let mut taken = Vec::new();
    taken_in(cond, &mut taken);
    let (holds, fails) = null_facts(cond);
    let kept = |names: Vec<String>| names.into_iter().filter(|name| !taken.contains(&name.as_str())).collect();
    (kept(holds), kept(fails))
}

/// Where the names of `is` are seen, for the notes of the errors about them.
const NAMES_SEEN: &str = "the names of `is` are seen where its test is sure to have held: on the right of `&&`, in the block of its `if` or `while`, and after an `if` whose `else` leaves";
const NOT_NAMES_SEEN: &str = "the names of `is not` are seen where its test is sure to have failed: on the right of `||`, in `else`, after an `if` whose block leaves, and after a `while` that only its test ends";

/// `x is Circle(r)` or `x is not Circle(r)` (`Some(false)`), or one of them under `!`
/// (`Some(true)`).
fn is_test(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::IsVariant { .. } => Some(false),
        ExprKind::Unary(UnOp::Not, inner) if matches!(inner.kind, ExprKind::IsVariant { .. }) => Some(true),
        _ => None,
    }
}

/// An `is` test that holds when the value is not what the pattern says: `x is not
/// Circle(r)`, `!(x is Circle(r))`.
fn tests_not(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::IsVariant { negated, .. } => *negated,
        ExprKind::Unary(UnOp::Not, inner) => !tests_not(inner),
        _ => false,
    }
}

/// A variable with the fields that lead from it to the place: `a.b.c` is (`a`, `b.c`).
/// `None` if the expression does not start at a variable.
fn path_of(expr: &Expr) -> Option<(&str, String)> {
    match &expr.kind {
        ExprKind::Var(name) => Some((name, String::new())),
        ExprKind::Field { recv, name, .. } => {
            let (var, mut path) = path_of(recv)?;
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(name);
            Some((var, path))
        }
        _ => None,
    }
}

/// Whether the field path `inner` is `outer` or lies inside it.
fn within(inner: &str, outer: &str) -> bool {
    inner.strip_prefix(outer).is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// `T?` without the `?`, also behind a borrow: `read T?` becomes `read T`.
fn without_null(ty: &Ty) -> Option<Ty> {
    match ty {
        Ty::Nullable(inner) => Some((**inner).clone()),
        Ty::Read(inner) => without_null(inner).map(|ty| Ty::Read(Box::new(ty))),
        Ty::Ref(inner) => without_null(inner).map(|ty| Ty::Ref(Box::new(ty))),
        _ => None,
    }
}

impl<'a> Checker<'a> {
    /// The functions written in the body of a struct or an enum.
    fn methods_of(&self, owner: &str) -> &'a HashMap<String, FuncSig> {
        let (structs, enums) = (self.structs, self.enums);
        match structs.get(owner) {
            Some(info) => &info.methods,
            None => &enums[owner].methods,
        }
    }

    // ---- variables ----

    fn declare(&mut self, name: &str, ty: Ty, span: Span) -> Res<()> {
        let scope = self.scopes.last_mut().expect("an open scope");
        if scope.vars.iter().any(|var| var.name == name) {
            return Err(Diag::error(span, format!("`{name}` is already declared in this scope")));
        }
        let moved = Moved::default();
        let id = self.borrows.next_id;
        self.borrows.next_id += 1;
        let decl_loops = self.borrows.loops.clone();
        scope.vars.push(Var { id, decl_loops, loans: Vec::new(), param: false, name: name.to_string(), unopened: None, declared: ty.clone(), current: ty, moved, assigned: 0, at: span, sender_of: None, unsent: None, opened: false });
        Ok(())
    }

    /// A new local variable; `this` is not a name for one.
    fn declare_local(&mut self, name: &str, ty: Ty, span: Span) -> Res<()> {
        if name == "this" {
            return Err(Diag::error(span, "`this` is the receiver of a method and cannot name a variable"));
        }
        if let Some(what) = std_stream(name) {
            return Err(Diag::error(span, format!("`{name}` is {what} of the program, so a variable cannot have its name")));
        }
        self.not_hiding(name, span)?;
        self.not_a_const(name, span)?;
        self.seen.locals.insert(span.start, (name.to_string(), ty.clone(), span));
        self.declare(name, ty, span)
    }

    /// The full name of the const a name written here means: one of a block around here, then
    /// one of a module. A local variable of the name comes first, and none can have it.
    fn const_named(&self, written: &str, span: Span) -> Res<Option<String>> {
        let local = self.scopes.iter().rev().flat_map(|scope| scope.consts.iter().rev()).find(|(name, _)| name == written);
        if let Some((_, full)) = local {
            return Ok(Some(full.clone()));
        }
        Ok(self.names.full(written, span)?.filter(|full| self.names.consts.contains(full)))
    }

    /// The full name of the static a name written here means: one of a block around here, then
    /// one of a module.
    fn static_named(&self, written: &str, span: Span) -> Res<Option<String>> {
        let local = self.scopes.iter().rev().flat_map(|scope| scope.statics.iter().rev()).find(|(name, _)| name == written);
        if let Some((_, full)) = local {
            return Ok(Some(full.clone()));
        }
        Ok(self.names.full(written, span)?.filter(|full| self.names.statics.contains(full)))
    }

    /// What the name of a static gives where it is used: a number, a `bool`, a `char` or a
    /// function is its value; anything else a `read` borrow of the one value, which lasts for
    /// the run. One of `static locked` is reached only by `lock`.
    fn static_use(&self, full: &str, span: Span) -> Res<Ty> {
        if self.names.locked_statics.contains(full) {
            let text = &self.src[span.start..span.end];
            return Err(Diag::error(span, format!("`{text}` is a `static locked` value, reached only inside a `lock` block, where no other task reaches it"))
                .hint(format!("`lock {text} as name {{ ... }}`")));
        }
        Ok(match &self.names.static_types[full] {
            ty @ (Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char | Ty::Func(..) | Ty::CFunc(..)) => ty.clone(),
            ty => Ty::Read(Box::new(ty.clone())),
        })
    }

    /// The error for a static written or lent to change.
    fn static_changed(name: &str, span: Span) -> Diag {
        Diag::error(span, format!("`{name}` is a static, and a static is never assigned: it is read by every task"))
            .note("a static that changes is `static atomic`, changed by its atomic methods, or `static locked`, changed inside a `lock` block")
    }

    /// The static a place is reached from, as it is written (`Table` in `Table[0]`), if it is one.
    fn static_root(&self, place: &Expr) -> Option<String> {
        let mut base = place;
        while let ExprKind::Field { recv, .. } | ExprKind::Index { recv, .. } = &base.kind {
            base = recv;
        }
        match &base.kind {
            ExprKind::Var(name) if name != "this" && self.var_named(name).is_none() => self.static_named(name, base.span).ok().flatten().map(|_| name.clone()),
            ExprKind::Static(full) => Some(full.split('#').next().map(|name| short_name(name).to_string()).unwrap_or_default()),
            _ => None,
        }
    }

    /// A local never takes the name of a const it sees: in a pattern the name is the const. Nor
    /// that of a static: a name means one thing in a function.
    fn not_a_const(&self, name: &str, span: Span) -> Res<()> {
        if name == "_" || name == "this" {
            return Ok(());
        }
        if let Some(full) = self.static_named(name, span).ok().flatten() {
            let diag = Diag::error(span, format!("`{name}` is the name of a static, and a local cannot take it"))
                .note("a name means one thing in a function")
                .hint("give the local another name");
            return Err(match self.names.spans.get(&full) {
                Some(at) => diag.related(*at, "the static"),
                None => diag,
            });
        }
        let Some(full) = self.const_named(name, span).ok().flatten() else { return Ok(()) };
        let diag = Diag::error(span, format!("`{name}` is the name of a const, and a local cannot take it"))
            .note("in a pattern the name stands for the const, so it means one thing in a function")
            .hint("give the local another name");
        Err(match self.names.spans.get(&full) {
            Some(at) => diag.related(*at, "the const"),
            None => diag,
        })
    }

    /// The const a place is reached from, as it is written (`Masks` in `Masks[0]`), if it is one.
    fn const_root(&self, place: &Expr) -> Option<String> {
        let mut base = place;
        while let ExprKind::Field { recv, .. } | ExprKind::Index { recv, .. } = &base.kind {
            base = recv;
        }
        match &base.kind {
            ExprKind::Var(name) if name != "this" && self.var_named(name).is_none() => self.const_named(name, base.span).ok().flatten().map(|_| name.clone()),
            ExprKind::Const(full) => Some(full.split('#').next().map(|name| short_name(name).to_string()).unwrap_or_default()),
            _ => None,
        }
    }

    /// The error for a const written or lent to change.
    fn const_changed(name: &str, span: Span) -> Diag {
        Diag::error(span, format!("`{name}` is a const: what it holds is fixed when the program is compiled"))
            .hint(format!("change a copy of it: `var copy = {name}`"))
    }

    /// A name never hides another: a new local may not take the name of a local or a parameter
    /// of a scope around it that is still open (C#'s rule, not Rust's shadowing). Blocks side by
    /// side may use one name, and so may code after a block has ended. `_` is not a name.
    fn not_hiding(&self, name: &str, span: Span) -> Res<()> {
        if name == "_" {
            return Ok(());
        }
        let outer = &self.scopes[..self.scopes.len() - 1];
        let Some(var) = outer.iter().rev().find_map(|scope| scope.vars.iter().rev().find(|var| var.name == name)) else {
            return Ok(());
        };
        let what = if var.param {
            format!("a parameter of `{}`", self.func_name)
        } else {
            "a local variable of a block around this one".to_string()
        };
        Err(Diag::error(span, format!("`{name}` is already {what}, and a new name cannot hide it"))
            .related(var.at, "it is declared here")
            .note("a name means one thing in a function: SG does not let an inner block declare it again, as C# does not")
            .hint("give the new variable another name"))
    }

    /// `Stdin`, `Stdout` and `Stderr`: every function and test has them, as `ref` parameters
    /// that the program gives. They are lent on and written through, never moved.
    fn declare_streams(&mut self, span: Span) -> Res<()> {
        for (name, ty) in [("Stdin", Ty::Reader(false)), ("Stdout", Ty::Writer), ("Stderr", Ty::Writer)] {
            self.declare(name, Ty::Ref(Box::new(ty)), span)?;
            self.find(name).expect("just declared").param = true;
        }
        Ok(())
    }

    /// The `Closable` check: a `StreamWriter` kept in a local of the scopes from `from` on must
    /// be closed, or moved somewhere, before they end, since `close` is where the error of
    /// writing its buffer shows up. `exit`: the `return`, `break`, `continue` or `exit` that
    /// ends them, or `None` for the end of the innermost block.
    ///
    /// A `Task` is the same: it is waited for before its variable ends (every task is waited
    /// for). An exit with an error is let go for both; the task is waited for there.
    fn unclosed(&self, from: usize, exit: Option<(Span, &str)>) -> Res<()> {
        for var in self.scopes[from..].iter().flat_map(|scope| &scope.vars) {
            if var.moved.whole.is_some() && !var.moved.partly {
                continue;
            }
            if keeps_tasks(&var.declared) {
                return Err(self.unwaited(var, exit));
            }
            if is_process(&var.declared) && var.name != "this" {
                return Err(self.unwaited_program(var, exit));
            }
            let Some(own) = closable(&var.declared, self.structs) else { continue };
            // The type's own methods are what closes it.
            if var.name == "this" {
                continue;
            }
            let name = &var.name;
            let hint = match (self.is_async, own) {
                (true, None) => format!("`try await {name}.closeAsync()`; to drop the error on purpose, `_ = await {name}.closeAsync()`"),
                _ => format!("`try {name}.close()`; to drop the error on purpose, `_ = {name}.close()`"),
            };
            let ends = if var.param { "the function ends" } else { "its block ends" };
            let lost = match own {
                None => "an error writing its buffer",
                Some(_) => "the error its `close` gives",
            };
            let diag = match (exit, var.moved.whole.is_some()) {
                (_, true) => Diag::error(var.at, format!("`{name}` is closed on some paths but not on all of them, so {lost} could be lost"))
                    .note("close it on every path, or once where the paths meet"),
                (Some((span, word)), false) => Diag::error(span, format!("`{name}` is not closed at this `{word}`, so {lost} would be lost"))
                    .related(var.at, if own.is_none() { "it is opened here" } else { "it is made here" }),
                (None, false) => Diag::error(var.at, format!("`{name}` is not closed before {ends}, so {lost} would be lost")),
            };
            let diag = match (exit, own) {
                (Some((_, "exit")), None) => diag.note("`exit` ends the program at once: a writer that is not closed loses what its buffer holds"),
                (Some((_, "exit")), Some(_)) => diag.note("`exit` ends the program at once, and nothing is closed or disposed of"),
                _ => diag,
            };
            return Err(diag.hint(hint));
        }
        Ok(())
    }

    /// What a `shared locked` value holds, reached outside a `lock` block.
    fn locked_outside(&self, span: Span) -> Diag {
        let text = &self.src[span.start..span.end];
        Diag::error(span, "what a `shared locked` value holds is reached only inside a `lock` block, where no other task reaches it")
            .hint(format!("`lock {text} as name {{ ... }}`"))
    }

    /// A program started with `start`, kept in `var`, is not waited for where its scope ends: it
    /// would be stopped there.
    fn unwaited_program(&self, var: &Var, exit: Option<(Span, &str)>) -> Diag {
        let name = &var.name;
        let ends = if var.param { "the function ends" } else { "its block ends" };
        let diag = match (exit, var.moved.whole.is_some()) {
            (_, true) => Diag::error(var.at, format!("the program in `{name}` is waited for on some paths but not on all of them"))
                .note("wait for it on every path, or once where the paths meet"),
            (Some((span, word)), false) => Diag::error(span, format!("the program in `{name}` is not waited for at this `{word}`"))
                .related(var.at, "it is started here"),
            (None, false) => Diag::error(var.at, format!("the program in `{name}` is not waited for before {ends}")),
        };
        let diag = match exit {
            Some((_, "exit")) => diag.note("`exit` ends this program at once, and leaves the other one running"),
            _ => diag.note("a program that is not waited for is stopped where its `Process` ends, with the signal 9"),
        };
        diag.hint(match self.is_async {
            true => format!("`var status = try await {name}.waitAsync()`; to stop it first, `{name}.kill()`"),
            false => format!("`var status = try {name}.wait()`; to stop it first, `{name}.kill()`"),
        })
    }

    /// A task kept in `var` is not waited for where its scope ends.
    fn unwaited(&self, var: &Var, exit: Option<(Span, &str)>) -> Diag {
        let name = &var.name;
        let ends = if var.param { "the function ends" } else { "its block ends" };
        if var.declared == Ty::Group {
            let diag = match (exit, var.moved.whole.is_some()) {
                (_, true) => Diag::error(var.at, format!("the tasks of `{name}` are waited for on some paths but not on all of them"))
                    .note("wait for them on every path, or once where the paths meet"),
                (Some((span, word)), false) => Diag::error(span, format!("the tasks of `{name}` are not waited for at this `{word}`"))
                    .related(var.at, "the group is made here"),
                (None, false) => Diag::error(var.at, format!("the tasks of `{name}` are not waited for before {ends}")),
            };
            let diag = match exit {
                Some((_, "exit")) => diag.note("`exit` ends the program at once, and would cut the tasks off"),
                _ => diag.note("every task is waited for: a group lets go of each task that ends, and `whenAll` waits for the rest"),
            };
            return diag.hint(match self.is_async {
                true => format!("`await {name}.whenAll()`; to cut them after a time, `await {name}.whenAll(read token)`"),
                false => format!("`{name}.whenAll().wait()`"),
            });
        }
        if task_list(&var.declared) {
            let diag = match (exit, var.moved.whole.is_some()) {
                (_, true) => Diag::error(var.at, format!("the tasks in `{name}` are waited for on some paths but not on all of them"))
                    .note("wait for them on every path, or once where the paths meet"),
                (Some((span, word)), false) => Diag::error(span, format!("the tasks in `{name}` are not waited for at this `{word}`"))
                    .related(var.at, "the list is kept here"),
                (None, false) => Diag::error(var.at, format!("the tasks in `{name}` are not waited for before {ends}")),
            };
            let diag = match exit {
                Some((_, "exit")) => diag.note("`exit` ends the program at once, and would cut the tasks off"),
                _ => diag.note(format!("a list of tasks is given to `Task.whenAll`, or emptied by a loop `while {name}.length() > 0` that takes them with `Task.whenAny`")),
            };
            return diag.hint(when_all(&var.declared, name, self.is_async));
        }
        let diag = match (exit, var.moved.whole.is_some()) {
            (_, true) => Diag::error(var.at, format!("`{name}` is waited for on some paths but not on all of them"))
                .note("wait for it on every path, or once where the paths meet"),
            (Some((span, word)), false) => Diag::error(span, format!("`{name}` is not waited for at this `{word}`"))
                .related(var.at, "the task is kept here"),
            (None, false) => Diag::error(var.at, format!("`{name}` is not waited for before {ends}")),
        };
        let diag = match exit {
            Some((_, "exit")) => diag.note("`exit` ends the program at once, and would cut the task off"),
            _ => diag.note("every task is waited for: its error is not lost, and no work is cut off"),
        };
        match self.is_async {
            true => diag.hint(format!("`await {name}`")),
            false => diag.hint(format!("`{name}.wait()`")),
        }
    }

    fn find(&mut self, name: &str) -> Option<&mut Var> {
        self.scopes.iter_mut().rev().find_map(|scope| scope.vars.iter_mut().rev().find(|var| var.name == name))
    }

    fn var(&mut self, name: &str, span: Span) -> Res<&mut Var> {
        let Some(var) = self.find(name) else {
            return Err(self.unknown_var(name, span));
        };
        let at = var.at.start;
        self.seen.uses.insert((span.start, name.to_string()), at);
        Ok(self.find(name).expect("just found"))
    }

    fn unknown_var(&mut self, name: &str, span: Span) -> Diag {
        let has_this = self.find("this").is_some();
        if name == "this" {
            return Diag::error(span, "there is no `this` here")
                .note("`this` exists in a method that takes it as its first parameter: `this`, `read this` or `ref this`");
        }
        let diag = Diag::error(span, format!("there is no variable named `{name}`"));
        let is_field = self.owner.is_some_and(|owner| self.structs.get(owner).is_some_and(|info| info.fields.iter().any(|(field, _)| field == name)));
        if is_field && has_this {
            return diag.hint(format!("a field is reached through `this`: `this.{name}`"));
        }
        if let Some(hint) = self.names.use_hint(name, span) {
            return diag.hint(hint);
        }
        if let Some((_, at, fails)) = self.is_names.iter().rev().find(|(named, at, _)| named == name && at.start < span.start) {
            return diag
                .related(*at, format!("`{name}` is named by this `is`"))
                .note(if *fails { NOT_NAMES_SEEN } else { NAMES_SEEN })
                .hint("use the value where the name is seen, or take it out with `match`");
        }
        let visible = self.scopes.iter().flat_map(|scope| scope.vars.iter().map(|var| var.name.as_str()));
        match nearest(name, visible) {
            Some(close) => diag.hint(format!("`{close}`")),
            None => diag,
        }
    }

    // ---- modules ----

    /// Whether the code being checked is in the module of this type: there it sees every
    /// field and function, elsewhere only the `public` ones.
    fn inside(&self, owner: &str) -> bool {
        module_of(owner) == self.module
    }

    fn not_public(&self, member: &str, owner: &str, span: Span) -> Diag {
        if module_of(owner) == STD_MODULE {
            return Diag::error(span, format!("`{}` keeps `{member}` to itself: its methods give what it holds", short_name(owner)));
        }
        Diag::error(span, format!("`{member}` is not public: it stays inside the `{}` module", module_of(owner)))
    }

    /// The module an expression names when it is written in front of a dot: `stock` in
    /// `stock.Product`. A variable of that name wins.
    fn module_named(&mut self, expr: &Expr) -> Option<String> {
        let mut head = expr;
        while let ExprKind::Field { recv, .. } = &head.kind {
            head = recv;
        }
        let ExprKind::Var(first) = &head.kind else { return None };
        if first == "this" || self.find(first).is_some() {
            return None;
        }
        let path = crate::parser::dotted(expr)?;
        let full = match self.names.file(expr.span) {
            Some(file) => file.map.full(&path),
            None => path,
        };
        self.names.modules.contains(&full).then_some(full)
    }

    /// Turns the name of a type or a function into its full name, as the rest of the checker
    /// and the interpreter know it: `Product` is `stock.Product` after `use stock`, and
    /// `stock.count(xs)` is a call of the function `stock.count`.
    /// `stock.Product` and `stock.find(id)`: a name of a module, written with the module in
    /// front, becomes one name. Gives whether it was one.
    fn unqualify(&mut self, expr: &mut Expr) -> bool {
        let (ExprKind::Field { recv, .. } | ExprKind::Method { recv, .. }) = &mut expr.kind else { return false };
        let module = match self.names.modules.is_empty() {
            true => None,
            false => self.module_named(recv),
        };
        let Some(module) = module else { return false };
        expr.kind = match std::mem::replace(&mut expr.kind, ExprKind::Null) {
            ExprKind::Field { name, .. } => ExprKind::Var(format!("{module}.{name}")),
            ExprKind::Method { name, name_span, args, .. } => ExprKind::Call { name: format!("{module}.{name}"), name_span, args, callee: Callee::Plain },
            _ => unreachable!("matched above"),
        };
        true
    }

    fn settle(&mut self, expr: &mut Expr) -> Res<()> {
        if !self.unqualify(expr) {
            if let ExprKind::Field { recv, .. } | ExprKind::Method { recv, .. } = &mut expr.kind {
                if matches!(recv.kind, ExprKind::Var(_) | ExprKind::Field { .. }) {
                    self.settle(recv)?;
                }
            }
        }
        let span = expr.span;
        let (ExprKind::Var(written) | ExprKind::Call { name: written, .. } | ExprKind::StructLit { name: written, .. }) = &expr.kind else {
            return Ok(());
        };
        if written == "this" || BUILTINS.contains(&written.as_str()) || self.find(written).is_some() {
            return Ok(());
        }
        let is_var = matches!(expr.kind, ExprKind::Var(_));
        // A const: a text is the text itself, as if it were written here; anything else names it.
        if is_var {
            if let Some(full) = self.const_named(&written.clone(), span)? {
                expr.kind = match (&self.consts.types.get(&full), self.consts.values.get(&full)) {
                    (_, Some((_, ConstVal::Str(text)))) => ExprKind::Str(text.clone()),
                    _ => ExprKind::Const(full),
                };
                return Ok(());
            }
            if let Some(full) = self.static_named(&written.clone(), span)? {
                if self.in_const {
                    return Err(Diag::error(span, format!("`{written}` is a static, and a const cannot read it: what a static holds is found when the program runs"))
                        .hint(format!("a `static` can be given the value: `static ... = {written}`")));
                }
                expr.kind = ExprKind::Static(full);
                return Ok(());
            }
        }
        let (ExprKind::Var(name) | ExprKind::Call { name, .. } | ExprKind::StructLit { name, .. }) = &mut expr.kind else {
            unreachable!("matched above")
        };
        if let Some(full) = self.names.full(name, span)? {
            *name = full;
        }
        // `opterr`, a variable of C: the item at its address, in raw memory.
        if is_var && self.names.cvars.contains(name.as_str()) {
            let name = name.clone();
            let at = |kind| Box::new(Expr { kind, span, ty: Ty::Void, moved: false });
            expr.kind = ExprKind::Index { recv: at(ExprKind::CVar(name)), index: at(ExprKind::Int(0, IntTy::Nint)), kind: IndexKind::Raw, unchecked: false };
            return Ok(());
        }
        // An alias of a struct or an enum builds it under the other name.
        if let Some(target) = self.alias_target(name, span)? {
            match target {
                Ty::Struct(full) | Ty::Enum(full) => *name = full,
                Ty::Shared(behind, _) => {
                    let written = short_name(name).to_string();
                    return Err(Diag::error(span, format!("`{written}` is `shared {behind}`, which is made with `new`"))
                        .hint(format!("`new {}`", self.src[span.start..span.end].trim())));
                }
                other @ (Ty::List(_) | Ty::Map(..) | Ty::Set(_)) => {
                    return Err(Diag::error(span, format!("`{}` is `{other}`; a collection is built by the name of its type", short_name(name)))
                        .hint(format!("`{other}()`")));
                }
                other => return Err(Diag::error(span, format!("`{}` is `{other}`, which is not built this way", short_name(name)))),
            }
        }
        Ok(())
    }

    /// What the `match` read from `a?.b` looks at, checked. A place is lent: `read`, or `ref`
    /// when the chain starts with a method that changes it. A method that uses its receiver up
    /// takes the place, as `.` does, and any other value is the `match`'s own. When what it
    /// looks at is lent, a place at the end of the chain is lent too, not moved: `a?.name`
    /// gives a `read string?`. Gives the type of what it looks at.
    fn or_null_subject(&mut self, subject: &mut Expr, arms: &mut [Arm], held: &str, unused: bool) -> Res<Ty> {
        if unused {
            arms[0].value = None;
        }
        let mut steps = Vec::new();
        if let Some(rest) = &arms[1].value {
            chain_steps(rest, held, &mut steps);
        }
        // The first method of the chain, reached through fields and items: how it takes its
        // receiver, and whether the receiver is what `a` holds.
        let mut this = None;
        let mut ty = self.peek_type(subject).and_then(|ty| without_null(&ty));
        for (position, step) in steps.iter().enumerate() {
            let Some(at) = ty.take() else { break };
            let mut at = at.strip_borrow().clone();
            while let Ty::Ptr(inner) = at {
                at = *inner;
            }
            match &step.kind {
                ExprKind::Method { name, .. } => {
                    this = self.method_this(&at, name).map(|this| (this, position == 0));
                    break;
                }
                ExprKind::Field { name, .. } => {
                    let Ty::Struct(owner) = &at else { break };
                    ty = self.structs.get(owner.as_str()).and_then(|info| info.fields.iter().find(|(field, _)| field == name)).map(|(_, ty)| ty.clone());
                }
                ExprKind::Index { .. } => {
                    ty = match at {
                        Ty::List(item) | Ty::Array(item, _) | Ty::Map(_, item) => Some(*item),
                        _ => None,
                    };
                }
                _ => break,
            }
        }
        let found = match this {
            Some((This::Value, true)) => None,
            _ => self.place(subject)?,
        };
        let this = this.map(|(this, _)| this);
        let ty = match found {
            Some((ty, writable)) => {
                self.maybe_null(&ty, subject)?;
                if Expr::in_raw_memory(subject) {
                    return Err(Diag::error(subject.span, "a place in raw memory is not lent yet: the checker cannot follow how long it lives")
                        .hint("copy it out first: `var item = unsafe ...`"));
                }
                if let Some(diag) = not_lent(subject, self.structs, self.src) {
                    return Err(diag);
                }
                let mutable = this == Some(This::Ref) && writable && self.readonly(subject).is_none();
                if mutable {
                    self.not_locked(subject, Access::Write)?;
                }
                let base = Box::new(ty.strip_borrow().clone());
                let lent = if mutable { Ty::Ref(base) } else { Ty::Read(base) };
                wrap(subject, |inner| ExprKind::Borrow { mutable, expr: inner });
                subject.ty = lent.clone();
                lent
            }
            None => {
                let ty = self.expr(subject, None)?;
                self.maybe_null(&ty, subject)?;
                ty
            }
        };
        if ty.is_borrow() {
            if let Some(rest) = &mut arms[1].value {
                if matches!(rest.kind, ExprKind::Field { .. } | ExprKind::Index { .. }) {
                    wrap(rest, |inner| ExprKind::Borrow { mutable: false, expr: inner });
                }
            }
        }
        Ok(ty)
    }

    /// `?.` goes on only from a value that may be `null`.
    fn maybe_null(&self, ty: &Ty, subject: &Expr) -> Res<()> {
        if without_null(ty).is_some() {
            return Ok(());
        }
        let text = &self.src[subject.span.start..subject.span.end];
        Err(match ty.strip_borrow() {
            Ty::Result(..) => Diag::error(subject.span, format!("`?.` goes past a `null`, and `{text}` is a `{ty}`, which holds a value or an error"))
                .hint(format!("`(try {text})` passes the error on, and `({text} ?? fallback)` puts a value in its place")),
            _ => Diag::error(subject.span, format!("`{text}` is a `{ty}`, which is never `null`")).hint("write `.` instead of `?.`"),
        })
    }

    /// The value of `a?.b` when `a` is there: a place lent that can be copied is copied, and
    /// the type is made a `T?` (one that already is stays as it is).
    fn held_value(&mut self, value: &mut Expr, ty: Ty) -> Res<Ty> {
        let mut ty = ty;
        if let (ExprKind::Borrow { mutable: false, .. }, Ty::Read(inner)) = (&value.kind, &ty) {
            if inner.is_copyable(self.copyable) {
                ty = (**inner).clone();
                let ExprKind::Borrow { expr: place, .. } = std::mem::replace(&mut value.kind, ExprKind::Null) else { unreachable!("matched above") };
                *value = *place;
            }
        }
        if ty == Ty::Void {
            let text = &self.src[value.span.start..value.span.end];
            return Err(Diag::error(value.span, format!("`{text}` gives nothing, so `?.` has no value to give"))
                .hint("write it on a line of its own"));
        }
        Ok(nullable_of(ty))
    }

    /// The type of a variable, or of a field of one, as far as it is known before it is
    /// checked: enough to find the method a chain starts with.
    fn peek_type(&mut self, expr: &Expr) -> Option<Ty> {
        match &expr.kind {
            ExprKind::Var(name) => self.var_named(name).map(|var| var.current.clone()),
            ExprKind::Field { recv, name, .. } => {
                let mut ty = self.peek_type(recv)?.strip_borrow().clone();
                while let Ty::Ptr(inner) = ty {
                    ty = *inner;
                }
                let Ty::Struct(owner) = &ty else { return None };
                self.structs.get(owner.as_str())?.fields.iter().find(|(field, _)| field == name).map(|(_, ty)| ty.clone())
            }
            _ => None,
        }
    }

    /// How a method of a value of this type takes `this`, when it is known.
    fn method_this(&self, ty: &Ty, name: &str) -> Option<This> {
        let mut ty = ty.strip_borrow();
        while let Ty::Ptr(inner) = ty {
            ty = inner;
        }
        match ty {
            Ty::Struct(owner) | Ty::Enum(owner) if self.structs.contains_key(owner.as_str()) || self.enums.contains_key(owner.as_str()) => {
                self.methods_of(owner).get(name).and_then(|sig| sig.this)
            }
            Ty::List(_) | Ty::Map(..) | Ty::Set(_) => Some(if crate::opening::changes(name) { This::Ref } else { This::Read }),
            _ => None,
        }
    }

    /// A struct is built field by field, by a literal or by `with`, only where every field is
    /// seen: in its module, or anywhere when every field is `public`.
    fn built_here(&self, name: &str, info: &StructInfo, span: Span, how: &str) -> Res<()> {
        if self.inside(name) {
            return Ok(());
        }
        let short = short_name(name);
        // A struct of options (`JsonOptions`) has every field public and none `readonly`: the
        // program writes what it wants. The others of `std` are made by their functions.
        let options = !info.open.is_empty() && info.open.iter().all(|open| *open) && !info.readonly.iter().any(|readonly| *readonly);
        if module_of(name) == STD_MODULE && !options {
            return Err(Diag::error(span, format!("a `{short}` is made by its functions, not field by field")).hint(std_makers(short)));
        }
        if let Some(position) = info.open.iter().position(|open| !open) {
            let field = &info.fields[position].0;
            let diag = Diag::error(span, format!("`{field}` is not public, so a `{short}` cannot be {how} outside the `{}` module", module_of(name)));
            return Err(match info.methods.get("create") {
                Some(create) if create.public && how.starts_with("built") => diag.hint(format!("`{short}({})`", create.param_names.join(", "))),
                _ => diag,
            });
        }
        Ok(())
    }

    /// The fields written in a literal or after `with`: each one of the struct, once, of its
    /// type. Gives which fields were written.
    fn field_values(&mut self, name: &str, info: &StructInfo, fields: &mut [FieldInit]) -> Res<Vec<bool>> {
        let mut given = vec![false; info.fields.len()];
        for init in fields.iter_mut() {
            let Some(position) = info.fields.iter().position(|(field, _)| *field == init.name) else {
                let diag = Diag::error(init.span, format!("`{name}` has no field `{}`", init.name));
                return Err(match nearest(&init.name, info.fields.iter().map(|(field, _)| field.as_str())) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag,
                });
            };
            if std::mem::replace(&mut given[position], true) {
                return Err(Diag::error(init.span, format!("field `{}` is given twice", init.name)));
            }
            init.index = position;
            self.typed(&mut init.value, &info.fields[position].1)?;
            if let Some(width) = info.shape.bits[position] {
                fits_bits(&init.value, &info.fields[position].1, width)?;
            }
        }
        Ok(given)
    }

    /// What an alias names, if `name` is one.
    fn alias_target(&self, name: &str, span: Span) -> Res<Option<Ty>> {
        match self.names.kinds.get(name) {
            Some(TypeKind::Alias) => Ok(Some(resolve_type(&self.names.aliases[name], self.names, false)?)),
            _ => Ok(None),
        }
        .map_err(|diag: Diag| diag.related(span, "the alias is used here"))
    }

    /// `new NodeRef(1)` or `new NodeRef { ... }` with an alias of a `shared` struct: the name
    /// becomes the struct's, and the caller makes it `new shared`.
    /// Gives whether the alias is `checked` and whether it is `locked`.
    fn shared_alias(&mut self, inner: &mut Expr) -> Res<Option<(bool, bool)>> {
        let span = inner.span;
        self.unqualify(inner);
        let (ExprKind::Call { name, .. } | ExprKind::StructLit { name, .. }) = &mut inner.kind else { return Ok(None) };
        if self.find(name).is_some() {
            return Ok(None);
        }
        let Some(full) = self.names.full(name, span)? else { return Ok(None) };
        match self.alias_target(&full, span)? {
            Some(Ty::Shared(behind, checked)) => match *behind {
                Ty::Struct(target) | Ty::Enum(target) => {
                    *name = target;
                    Ok(Some((checked, false)))
                }
                Ty::Locked(locked) if matches!(&*locked, Ty::Struct(_) | Ty::Enum(_)) => {
                    let (Ty::Struct(target) | Ty::Enum(target)) = *locked else { unreachable!("matched above") };
                    *name = target;
                    Ok(Some((false, true)))
                }
                other => Err(Diag::error(span, format!("`{}` is `shared {other}`, which is not built this way", short_name(&full)))),
            },
            Some(Ty::Weak(..)) => Err(Diag::error(span, "a `weak` value is not made with `new`: it looks at a `shared` one")),
            _ => Ok(None),
        }
    }

    /// An error about writing through a `read` borrow, said of a `shared` value when the
    /// place is reached through one.
    fn read_only(&self, place: &Expr, mut diag: Diag) -> Diag {
        if !through_shared(place) {
            return diag;
        }
        diag.msg = diag.msg.replace("a `read` borrow", "a `shared` value, which is only read");
        diag.hint = None;
        diag.note(SHARED_READ_ONLY)
    }

    /// A `ref` field reached through a `read` borrow (`w.items`, with `read W w`) only reads, as
    /// `&&mut T` does in Rust: given on as a `ref`, copied into a variable or put into a field,
    /// it would change what the `read` borrow only looks at (choice 1592).
    fn not_through_read(&self, place: &Expr) -> Res<()> {
        if !place.is_place() || !through_read(place) {
            return Ok(());
        }
        let text = self.src[place.span.start..place.span.end].trim();
        Err(self.read_only(
            place,
            Diag::error(place.span, format!("`{text}` is a `ref` reached through a `read` borrow, so it only reads here and cannot be given on as a `ref`"))
                .hint(format!("lend it to read: `read {text}`; to change it, take the value it is in as `ref`")),
        ))
    }

    /// The `readonly` field a place lies in or behind, as the error for writing to it.
    fn readonly(&self, place: &Expr) -> Option<Diag> {
        match &place.kind {
            ExprKind::Field { recv, name, name_span, index } => {
                if let Some(Ty::Struct(owner)) = self.place_ty(recv).as_ref().map(peel) {
                    if self.structs.get(owner).is_some_and(|info| info.readonly.get(*index) == Some(&true)) {
                        return Some(Diag::error(*name_span, format!("`{name}` is `readonly`: it is given when the value is built and not changed afterwards")));
                    }
                }
                self.readonly(recv)
            }
            ExprKind::Index { recv, .. } => self.readonly(recv),
            _ => None,
        }
    }

    /// A variable that is about to be read: it must still hold its value.
    fn live_var(&mut self, name: &str, span: Span) -> Res<&mut Var> {
        let var = self.var(name, span)?;
        if let Some(moved) = var.moved.whole {
            if var.moved.emptied {
                return Err(Diag::error(span, format!("`{name}` is used after the loop that emptied it"))
                    .related(moved, "after this loop its tasks count as waited for, and the list as used up")
                    .hint(format!("for more tasks, give it a new list first: `{name} = List<...>()`")));
            }
            let diag = Diag::error(span, format!("`{name}` was moved and can no longer be used")).related(moved, "it was moved here");
            // A generator is not copied, so that two never give the same numbers.
            if matches!(peel(&var.declared), Ty::Struct(ty) if ty == &format!("{STD_MODULE}.Random")) {
                return Err(diag
                    .note("a `Random` is not `Copyable`: a copy would give the same numbers again")
                    .hint(format!("lend it where it is needed: `ref {name}`; for numbers of their own, another `Random.fromSeed(seed)` or `Random.fromSystem()`")));
            }
            return Err(diag);
        }
        if let Some(made) = var.unsent {
            return Err(Diag::error(span, format!("`{name}` has no sender yet, and a channel no sender holds has ended: what receives from it now finds nothing and stops"))
                .related(made, "the channel is made here")
                .hint(format!("make its sender first, before it is cloned, given or received from: `var tx = {name}.sender()`")));
        }
        var.unopened = None;
        let id = var.id;
        let at = self.point(span.start);
        self.borrows.uses.push((id, at));
        self.borrows.pos = self.borrows.pos.max(span.start);
        Ok(self.find(name).expect("just found"))
    }

    // ---- borrows ----

    /// The point the checker is at, for a thing at this position.
    fn point(&mut self, pos: usize) -> Point {
        self.borrows.seq += 1;
        Point { pos, seq: self.borrows.seq, placed: None, branches: self.borrows.branches.clone(), loops: self.borrows.loops.clone() }
    }

    /// A number for the next `if`, `match` or loop, to tell them apart.
    fn node(&mut self) -> usize {
        self.borrows.next_id += 1;
        self.borrows.next_id
    }

    fn var_named(&self, name: &str) -> Option<&Var> {
        self.scopes.iter().rev().find_map(|scope| scope.vars.iter().rev().find(|var| var.name == name))
    }

    /// Whether a value of this type is a borrow or carries one.
    fn holds(&self, ty: &Ty) -> bool {
        lends(ty, self.names)
    }

    /// The type of a place that has been checked.
    fn place_ty(&self, place: &Expr) -> Option<Ty> {
        match &place.kind {
            ExprKind::Var(name) => self.var_named(name).map(|var| var.current.clone()),
            ExprKind::Field { recv, index, .. } => match peel(&self.place_ty(recv)?) {
                Ty::Struct(name) => self.structs.get(name)?.fields.get(*index).map(|(_, ty)| ty.clone()),
                Ty::Entry(key, value) => Some(if *index == 0 { (**key).clone() } else { (**value).clone() }),
                _ => None,
            },
            ExprKind::Index { recv, .. } => match peel(&self.place_ty(recv)?) {
                Ty::List(item) | Ty::Array(item, _) | Ty::Slice(item) => Some((**item).clone()),
                Ty::Map(_, value) => Some((**value).clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether the place lies behind a field that is itself a borrow (`this.input`, with
    /// `read string input`): lending it lends what the field borrowed, not the struct.
    fn through_borrow(&self, place: &Expr) -> bool {
        match &place.kind {
            ExprKind::Field { recv, .. } | ExprKind::Index { recv, .. } => {
                self.place_ty(place).is_some_and(|ty| self.holds(&ty)) || self.through_borrow(recv)
            }
            _ => false,
        }
    }

    /// What the variable a place starts at holds: its own loans, and for a parameter the
    /// mark that says "what this parameter holds".
    fn held_by(&self, place: &Expr, span: Span) -> Vec<Loan> {
        // A place in a value with no name holds what that value borrowed (choice 1592).
        let Some((name, _)) = store_path(place) else { return self.loans_of(base_of(place), false) };
        let Some(var) = self.var_named(name) else { return Vec::new() };
        let mut loans = var.loans.clone();
        if var.param && self.holds(&var.declared) {
            loans.push(Loan { root: var.id, path: String::new(), mutable: false, held: true, span });
        }
        loans
    }

    /// What lending this place gives the borrower: a loan on the place, and whatever the
    /// variable it starts at has borrowed itself.
    fn place_loans(&self, place: &Expr, mutable: bool, span: Span) -> Vec<Loan> {
        if self.through_borrow(place) {
            return self.held_by(place, span);
        }
        let Some((name, path)) = store_path(place) else {
            let mut loans = self.loans_of(place, mutable);
            if unnamed_owner(place) {
                loans.push(Loan { root: UNNAMED, path: String::new(), mutable, held: false, span: place.span });
            }
            return loans;
        };
        let Some(var) = self.var_named(name) else { return Vec::new() };
        let mut loans = self.held_by(place, span);
        loans.push(Loan { root: var.id, path, mutable, held: false, span });
        loans
    }

    /// What an argument gives a result that borrows from it. `held`: only what it holds.
    fn lent_by(&self, arg: &Expr, held: bool, mutable: bool) -> Vec<Loan> {
        let place = match &arg.kind {
            ExprKind::Borrow { expr, .. } => expr,
            _ => arg,
        };
        match held && place.is_place() {
            true => self.held_by(place, arg.span),
            false if arg.is_place() => self.place_loans(arg, mutable, arg.span),
            false if held => self.loans_of(arg, mutable),
            // A value with no name that is lent as it is: a method called on what a call gave.
            false => self.place_loans(arg, mutable, arg.span),
        }
    }

    /// A borrow of a value that has no name cannot be kept: the value is gone when the
    /// statement ends. `keeper`: what would go on looking at it.
    fn kept_unnamed(&self, loans: &[Loan], value: &Expr, keeper: &str) -> Res<()> {
        let Some(loan) = loans.iter().find(|loan| loan.root == UNNAMED) else { return Ok(()) };
        let text = &self.src[loan.span.start..loan.span.end];
        let diag = Diag::error(value.span, format!("this borrows from `{text}`, a value with no name: it is gone when the line ends, and {keeper} would still look at it"));
        Err(diag.hint(format!("give the value a name first: `var value = {text}`")))
    }

    /// What the value of an expression has borrowed. `mutable`: the value is a `ref`.
    fn loans_of(&self, expr: &Expr, mutable: bool) -> Vec<Loan> {
        let union = |parts: &mut dyn Iterator<Item = &Expr>| parts.flat_map(|part| self.loans_of(part, mutable)).collect::<Vec<_>>();
        match &expr.kind {
            ExprKind::Borrow { mutable, expr: inner } => self.place_loans(inner, *mutable, expr.span),
            // `ref xs[a..b]` is kept as the range alone, with the type of a `ref` slice.
            ExprKind::Slice { recv, .. } => self.place_loans(recv, matches!(expr.ty, Ty::Ref(_)), expr.span),
            ExprKind::Array { args, items, .. } => union(&mut args.iter().chain(items.iter().flatten())),
            // A copy of a borrow has borrowed what the original has. It also borrows through
            // the variable it is copied from: a `ref` copied out of `a` keeps `a` from being
            // used, and a parameter is where the borrow comes from.
            ExprKind::Var(name) => {
                let Some(var) = self.var_named(name) else { return Vec::new() };
                let mut loans = var.loans.clone();
                if var.declared.is_borrow() {
                    loans.push(Loan { root: var.id, path: String::new(), mutable, held: false, span: expr.span });
                } else if var.param && self.holds(&var.declared) {
                    loans.push(Loan { root: var.id, path: String::new(), mutable: false, held: true, span: expr.span });
                }
                loans
            }
            ExprKind::Field { .. } | ExprKind::Index { .. } => match self.through_borrow(expr) {
                true => self.held_by(expr, expr.span),
                // A part of a value with no name (`wrap(read text).text`) carries what that
                // value borrowed (choice 1592).
                false if store_path(expr).is_none() && (expr.ty.is_borrow() || self.holds(&expr.ty)) => self.loans_of(base_of(expr), mutable),
                false => Vec::new(),
            },
            ExprKind::Load(inner) | ExprKind::Unbox(inner) | ExprKind::AsSlice(inner) | ExprKind::Widen(_, inner) | ExprKind::New(inner) | ExprKind::Try { expr: inner, .. } => {
                self.loans_of(inner, mutable)
            }
            ExprKind::Coalesce { left, right, .. } => union(&mut [&**left, &**right].into_iter()),
            // A value built from borrows carries them.
            ExprKind::StructLit { fields, .. } => union(&mut fields.iter().map(|field| &field.value)),
            ExprKind::With { base, fields } => union(&mut std::iter::once(&**base).chain(fields.iter().map(|field| &field.value))),
            // What a `match` gives borrows from what its arms give; the names of an arm are gone
            // by now, and what they lent comes from what the `match` looks at.
            ExprKind::Match { subject, arms, .. } => {
                let mut loans = union(&mut arms.iter().filter_map(|arm| arm.value.as_ref()));
                if expr.ty.is_borrow() || self.holds(&expr.ty) {
                    loans.extend(self.loans_of(subject, mutable));
                }
                loans
            }
            ExprKind::Variant { args, .. } => union(&mut args.iter()),
            // What a call returns borrows from what its signature names: the one borrowed
            // parameter, or those after `from`. A built-in method lends from its receiver.
            ExprKind::Method { recv, name, args, callee, .. } => {
                // `$"...".toString()` writes the text out into one of its own: what the pieces
                // borrowed is read then, and the text keeps none of it.
                if name == "toString" && matches!(recv.ty.strip_borrow(), Ty::Fmt) && !self.holds(&expr.ty) {
                    return Vec::new();
                }
                // `slot.take()` and `slot.replace(v)` give what was in the place.
                if matches!(callee, Callee::Take | Callee::Replace) {
                    return self.loans_of(recv, mutable);
                }
                // `Memory.view(p, n)` looks through the variable that holds `p`, and
                // `Memory.viewRef(p, n)` writes through it.
                if let Callee::Memory(call @ (MemoryCall::View | MemoryCall::ViewRef)) = callee {
                    let pointer = match &args[0].kind {
                        ExprKind::Load(inner) => &**inner,
                        _ => &args[0],
                    };
                    return self.place_loans(pointer, *call == MemoryCall::ViewRef, expr.span);
                }
                // A part that an iterator gives borrows the text, not the iterator: after
                // `var word = words.next()` the `words` is free to go on.
                if matches!(recv.ty.strip_borrow(), Ty::TextIter(TextIter::ReadLines)) {
                    return Vec::new();
                }
                if matches!(recv.ty.strip_borrow(), Ty::TextIter(_)) {
                    return self.lent_by(recv, true, mutable);
                }
                // `reader.lines()` reads through the reader as long as it is used.
                if matches!(callee, Callee::Stream(StreamCall::Lines)) {
                    return self.lent_by(recv, false, true);
                }
                // `reader.buffered()` looks at the block of the reader: nothing reads it, or
                // gives out of it, while the bytes are used (1734).
                if matches!(callee, Callee::Stream(StreamCall::Buffered)) {
                    return self.lent_by(recv, false, false);
                }
                // `conn.reader()` reads the connection as long as it is used, and lets it send.
                if matches!(callee, Callee::Net(NetCall::Reader)) {
                    return self.lent_by(recv, false, false);
                }
                // `split` looks at its separator as long as it looks at the text.
                if name == "split" && matches!(expr.ty, Ty::TextIter(_)) && args[0].ty.is_borrow() {
                    let mut loans = self.lent_by(recv, false, mutable);
                    loans.extend(self.loans_of(&args[0], mutable));
                    return loans;
                }
                let sig = match callee {
                    Callee::Method { owner, .. } | Callee::Static { owner, .. } => self.methods_of(owner).get(name.as_str()),
                    _ => None,
                };
                let source_of = |param: &str| sig.and_then(|sig| sig.sources.iter().find(|source| source.param == param));
                let mut loans = match (sig, callee) {
                    (Some(_), _) => match source_of("this") {
                        Some(source) => self.lent_by(recv, source.held, mutable),
                        None => Vec::new(),
                    },
                    // What `removeLast` takes out is no longer in the list: it borrows what the
                    // list borrowed (1732), and the list is free to change.
                    (None, Callee::Plain) if name == "removeLast" => self.lent_by(recv, true, mutable),
                    (None, Callee::Plain) => self.lent_by(recv, false, mutable),
                    (None, _) => Vec::new(),
                };
                for (position, arg) in args.iter().enumerate() {
                    match (sig, callee) {
                        (Some(sig), _) => {
                            if let Some(source) = source_of(&sig.param_names[position]) {
                                loans.extend(self.lent_by(arg, source.held, mutable));
                            }
                        }
                        (None, Callee::Plain) => {}
                        (None, _) => loans.extend(self.loans_of(arg, mutable)),
                    }
                }
                loans
            }
            ExprKind::Call { name, args, callee, .. } => {
                let sigs = self.sigs;
                let sig = match callee {
                    Callee::Static { owner, name } => self.methods_of(owner).get(name.as_str()),
                    Callee::Plain => sigs.get(name.as_str()),
                    _ => None,
                };
                let mut loans = Vec::new();
                for (position, arg) in args.iter().enumerate() {
                    match sig {
                        Some(sig) => {
                            let source = sig.param_names.get(position).and_then(|param| sig.sources.iter().find(|source| source.param == *param));
                            if let Some(source) = source {
                                loans.extend(self.lent_by(arg, source.held, mutable));
                            }
                        }
                        None => loans.extend(self.loans_of(arg, mutable)),
                    }
                }
                loans
            }
            // The items of a collection being built carry what they borrowed.
            ExprKind::Collection { args, items, .. } => union(&mut args.iter().chain(items.iter().flatten().flat_map(|item| &item.values))),
            // A value put on the heap, or another owner of one, carries what it borrowed.
            ExprKind::NewShared { value: inner, .. } | ExprKind::Share(inner) | ExprKind::WeakOf(inner) | ExprKind::Unsafe(inner) | ExprKind::Await(inner) => {
                self.loans_of(inner, mutable)
            }
            ExprKind::Open { inner, .. } | ExprKind::Within(inner) => self.loans_of(inner, mutable),
            // A text being formatted looks at its values until it is written.
            ExprKind::Interp(parts) if matches!(expr.ty, Ty::Fmt) => union(&mut parts.iter().filter_map(|part| match part {
                InterpPart::Expr(inner, _) => Some(inner),
                _ => None,
            })),
            // A number, a `bool`, a text that lasts for the run, a raw pointer, a value of C or a
            // const: nothing that borrows from a variable of the function.
            ExprKind::Int(..) | ExprKind::Float(..) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Char(_) | ExprKind::Interp(_) | ExprKind::Null => Vec::new(),
            ExprKind::Unary(..) | ExprKind::Binary(..) | ExprKind::Cast { .. } | ExprKind::Atomic(_) | ExprKind::IsNull { .. } | ExprKind::IsVariant { .. } => Vec::new(),
            ExprKind::Allocate { .. } | ExprKind::SizeOf { .. } | ExprKind::RawCast { .. } | ExprKind::CVar(_) | ExprKind::Return(_) | ExprKind::Constant { .. } => Vec::new(),
            ExprKind::FuncRef(_) | ExprKind::Const(_) | ExprKind::Static(_) | ExprKind::Embed(..) => Vec::new(),
        }
    }

    /// A value that borrows is put into a place: a field, an item, or a variable of a type
    /// that carries a borrow. A local then holds what the value borrowed. Through a borrow
    /// (`this.current = token`) the value must come from what that same owner holds.
    /// `adds`: the value goes into a collection beside what is there (`xs.add(v)`).
    fn stores(&mut self, target: &Expr, value: &Expr, adds: bool) -> Res<()> {
        let loans = self.loans_of(value, false);
        self.kept_unnamed(&loans, value, "the place it is put into")?;
        // A place no variable of the function is (`map.getOrAdd(k, v).add(x)`, `pick(ref
        // all).add(x)`), or one behind a `ref` field (`w.items.add(x)`): what the value borrowed
        // is held by what that place borrows from, which outlives the line or the carrier
        // (choice 1592).
        let own_borrow = store_path(target).and_then(|(name, _)| self.var_named(name)).is_some_and(|var| var.declared.is_borrow());
        if store_path(target).is_none() || (self.through_borrow(target) && !own_borrow) {
            let owners = match store_path(target) {
                None => self.loans_of(target, true),
                Some(_) => self.held_by(target, target.span),
            };
            for owner in owners.iter().filter(|owner| owner.root != UNNAMED) {
                if owner.held {
                    // What a parameter holds is the caller's: only what it holds goes in.
                    if let Some(outside) = loans.iter().find(|loan| !(loan.held && loan.root == owner.root)) {
                        let text = &self.src[value.span.start..value.span.end];
                        let diag = Diag::error(value.span, format!("`{text}` borrows from somewhere else than the place it is put into, which the caller keeps after the call"));
                        let diag = match outside.span == value.span {
                            true => diag,
                            false => diag.related(outside.span, "it is borrowed here"),
                        };
                        return Err(diag.note("storing a borrow from outside into what a parameter holds is not in the language yet; build the value where its source is known"));
                    }
                    continue;
                }
                let Some((depth, position)) = self.var_at(owner.root) else { continue };
                let var = &mut self.scopes[depth].vars[position];
                for loan in loans.iter().filter(|loan| loan.root != owner.root) {
                    if !var.loans.iter().any(|known| same_loan(known, loan)) {
                        var.loans.push(loan.clone());
                    }
                }
            }
            return Ok(());
        }
        let Some((name, path)) = store_path(target) else { return Ok(()) };
        let Some(var) = self.find(name) else { return Ok(()) };
        let id = var.id;
        if var.declared.is_borrow() {
            if let Some(outside) = loans.iter().find(|loan| !(loan.held && loan.root == id)) {
                let text = &self.src[value.span.start..value.span.end];
                let diag = Diag::error(value.span, format!("`{text}` borrows from somewhere else than `{name}` does, and `{name}` would keep that borrow after the call"));
                let diag = match outside.span == value.span {
                    true => diag,
                    false => diag.related(outside.span, "it is borrowed here"),
                };
                return Err(diag
                    .note(format!("storing a borrow from outside into `{name}` is not in the language yet; build the value where its source is known")));
            }
            return Ok(());
        }
        // A new value for the whole variable replaces what it borrowed; a field, an item or
        // one more item adds to it.
        let loans = loans.into_iter().filter(|loan| loan.root != id);
        match path.is_empty() && !adds && !reaches_item(target) {
            true => var.loans = loans.collect(),
            false => {
                for loan in loans {
                    if !var.loans.iter().any(|known| same_loan(known, &loan)) {
                        var.loans.push(loan);
                    }
                }
            }
        }
        Ok(())
    }

    /// Notes that a place is touched. If a variable holds a borrow that this does not go
    /// together with (any touch with a `ref`; a change or a move with a `read`), it is kept
    /// as a possible error until it is known whether that variable is used again.
    fn touch(&mut self, expr: &Expr, access: Access) {
        let Some((name, path)) = store_path(expr) else { return };
        let Some(var) = self.find(name) else { return };
        let root = var.id;
        let behind = var.declared.strip_borrow().clone();
        let carrier = path.is_empty() && !var.declared.is_borrow() && var.loans.iter().any(|loan| loan.mutable && !loan.held);
        let copy_as = (var.declared.is_borrow() && behind.is_copyable(self.copyable)).then(|| behind.to_string());
        // Changing a place behind a `ref` field (`w.items.clear()`, with `ref List<string>
        // items`) changes what the field borrowed: it is touched too. So does giving away, as
        // `ref` or by value, a whole value that holds a `ref` (`wipe(ref w)`, `w.clear()` with
        // `ref this`, `consume(w)`): the code it goes to may change through it (choice 1592).
        let through: Vec<Loan> = match access != Access::Read && (carrier || self.through_borrow(expr)) {
            true => self.held_by(expr, expr.span).into_iter().filter(|loan| !loan.held && loan.root != root && (!carrier || loan.mutable)).collect(),
            false => Vec::new(),
        };
        self.touch_root(root, &path, access, expr.span, copy_as, root);
        // The variable written through holds that borrow itself: it does not clash with it.
        for loan in through {
            self.touch_root(loan.root, &loan.path, access, expr.span, None, root);
        }
    }

    /// `touch` of a place of the variable `root`; `by`: the variable it is reached through.
    fn touch_root(&mut self, root: usize, path: &str, access: Access, span: Span, copy_as: Option<String>, by: usize) {
        let at = self.point(span.start);
        self.borrows.touched.push((root, path.to_string(), access, span, at.clone()));
        let mut found = Vec::new();
        for var in self.scopes.iter().flat_map(|scope| &scope.vars) {
            if var.id == root || var.id == by {
                continue;
            }
            for loan in &var.loans {
                let overlaps = path.is_empty() || loan.path.is_empty() || within(path, &loan.path) || within(&loan.path, path);
                if loan.root == root && !loan.held && overlaps && (loan.mutable || access != Access::Read) {
                    found.push(Clash {
                        holder: var.id,
                        holder_name: var.name.clone(),
                        owner_name: String::new(),
                        closed: false,
                        copy_as: copy_as.clone().filter(|_| loan.path.is_empty()),
                        holder_loops: var.decl_loops.clone(),
                        clears: clears(var),
                        loan: loan.clone(),
                        access,
                        span,
                        at: at.clone(),
                    });
                }
            }
        }
        self.borrows.clashes.extend(found);
    }

    /// Called when the whole function has been read: a touch that did not go together with a
    /// borrow is an error if the variable that held the borrow is used afterwards.
    fn borrow_conflicts(&self) -> Res<()> {
        for clash in &self.borrows.clashes {
            let key = |at: &Point| (at.pos, at.seq);
            // Not reached from here: the other branch of an `if` or a `match` around both.
            let other_branch = |at: &Point| at.branches.iter().any(|(node, branch)| clash.at.branches.iter().any(|(n, b)| n == node && b != branch));
            let after = |at: &Point| key(at) > key(&clash.at) && !other_branch(at);
            // A loop comes round again, unless the variable is new in every round.
            let next_round = |at: &Point| clash.at.loops.iter().any(|l| !clash.holder_loops.contains(l) && at.loops.contains(l));
            // The borrows of a list count until it is cleared, and a `clear` reads none of them
            // (1748): a use is no clash when a `clear` of the holder lies on every way to it from
            // here. In the same round between the two, or in the round the use is in, before it
            // (the `clear` itself is a use at the same place); not in a branch or a loop the use is
            // outside of.
            let cleared_before = |at: &Point| {
                self.borrows.released.iter().any(|(holder, _, cleared)| {
                    let on_the_way = match (after(at), next_round(at)) {
                        (true, _) => after(cleared) && cleared.pos <= at.pos,
                        (false, true) => (after(cleared) && next_round(cleared)) || (next_round(cleared) && cleared.pos <= at.pos),
                        _ => false,
                    };
                    *holder == clash.holder
                        && on_the_way
                        && cleared.branches.iter().all(|branch| at.branches.contains(branch))
                        && cleared.loops.iter().all(|inner| at.loops.contains(inner))
                })
            };
            let used_later = self.borrows.uses.iter().find(|(var, at)| *var == clash.holder && (after(at) || next_round(at)) && !cleared_before(at));
            let Some((_, later)) = used_later else { continue };
            let text = &self.src[clash.span.start..clash.span.end];
            let holder = &clash.holder_name;
            if clash.access == Access::Gone && clash.closed {
                let owner = &clash.owner_name;
                return Err(Diag::error(clash.span, format!("`{holder}` borrows from `{owner}` here, but `{owner}` is closed at the end of its block and `{holder}` is used after that"))
                    .note(format!("what `{owner}` opens is lent for the block only: after it another task, or another owner, may change it"))
                    .hint("to keep something after the block, copy it there (`.clone()`) into a variable that owns it"));
            }
            if clash.access == Access::Gone {
                let owner = &clash.owner_name;
                let diag = Diag::error(clash.span, format!("`{holder}` borrows from `{owner}` here, but `{owner}` is gone at the end of its block and `{holder}` is used after that"));
                return Err(match clash.clears {
                    true => diag.hint(format!("`{holder}.clear()` before `{holder}` is used again, on every way to it: a list with no items borrows nothing; or declare `{owner}` where `{holder}` is declared")),
                    false => diag.hint(format!("declare `{owner}` where `{holder}` is declared, so that it lives as long")),
                });
            }
            let diag = match (clash.access, clash.loan.mutable) {
                (Access::Read, _) => Diag::error(clash.span, format!("`{text}` cannot be used here: `{holder}` holds a `ref` borrow of it and is used again later")),
                (Access::Write, _) => Diag::error(clash.span, format!("`{text}` cannot be changed here: `{holder}` borrows from it and is used again later")),
                (Access::Move | Access::Gone, _) => Diag::error(clash.span, format!("`{text}` cannot be moved here: `{holder}` borrows from it and is used again later")),
            };
            let mut diag = diag.related(clash.loan.span, format!("`{holder}` borrowed it here"));
            if self.borrows.disposed.contains(&later.seq) {
                return Err(diag
                    .note(format!("`{holder}` is `Disposable`: its `dispose` runs at the end of its block and uses it there"))
                    .hint(format!("put `{holder}` in a block of its own that ends before this line")));
            }
            // Borrowed further down in a loop, or where the touch is (one part lends what another
            // changes, `later(Window { part = buffer.all() }, view)`): the borrow is of the round
            // before (choice 1381).
            if clash.loan.span.start >= clash.span.start && !clash.at.loops.is_empty() {
                diag = diag.note(format!("the loop comes round again, and `{holder}` still holds the borrow of the round before"));
            }
            if clash.clears && !clash.at.loops.is_empty() {
                return Err(diag.hint(format!("`{holder}.clear()` before `{holder}` is used again, on every way to it: a list with no items borrows nothing")));
            }
            // Used by the place of the assignment whose right side touched it.
            if later.placed.is_some_and(|from| clash.at.placed.is_none() && (from..=later.pos).contains(&clash.at.pos)) {
                diag = diag.note(format!("the right side of an assignment is computed first, then the place on the left is found: `{holder}` is used there after this"));
            }
            return Err(match &clash.copy_as {
                Some(ty) => diag.hint(format!("`{holder}` is a second borrow of the same place; to copy the value instead, write its type: `{ty} {holder} = ...`")),
                None => diag.hint(format!("finish with `{holder}` before this line: a borrow ends at its last use")),
            });
        }
        Ok(())
    }

    /// The value a function returns as a borrow must come from where the signature says: a
    /// borrowed parameter. A borrow of something that lives only in the function would dangle.
    fn returned_borrow(&mut self, value: &Expr) -> Res<()> {
        let loans = self.loans_of(value, matches!(self.ret, Ty::Ref(_)));
        self.kept_unnamed(&loans, value, "the caller")?;
        for loan in loans {
            let Some(var) = self.scopes.iter().flat_map(|scope| &scope.vars).find(|var| var.id == loan.root) else { continue };
            let name = &var.name;
            if var.opened {
                return Err(Diag::error(value.span, format!("this returns a borrow of what `{name}` opened, and `{name}` is closed at the end of its block"))
                    .note(format!("what `{name}` opens is lent for the block only: after it another task, or another owner, may change it"))
                    .hint("return a copy: `.clone()`"));
            }
            let written = |source: &Source| match (source.held, source.param.as_str()) {
                (true, "this") => self.owner.and_then(|owner| self.names.carriers.get(owner)).cloned().unwrap_or_else(|| "this".to_string()),
                (true, param) => match self.var_named(param).and_then(|var| carrier(&var.declared, self.names)) {
                    Some((_, source)) if self.var_named(param).is_some_and(|var| var.declared.is_borrow()) => format!("{param}.{source}"),
                    _ => param.to_string(),
                },
                (false, param) => param.to_string(),
            };
            let named = self.sources.iter().find(|source| source.param == *name);
            let fits = match (loan.held, named) {
                (_, None) => false,
                // What the parameter holds is covered by naming the parameter itself too.
                (true, Some(_)) => true,
                (false, Some(source)) => !source.held,
            };
            if loan.held || (var.declared.is_borrow() && var.param) {
                if fits {
                    continue;
                }
                let wanted = written(&Source { param: name.clone(), held: loan.held });
                let said: Vec<String> = self.sources.iter().map(|source| format!("`{}`", written(source))).collect();
                let all: Vec<String> = self.sources.iter().filter(|source| source.param != *name).map(&written).chain([wanted.clone()]).collect();
                let diag = match said.is_empty() {
                    true => Diag::error(value.span, format!("this returns a borrow that comes from `{wanted}`, and the function does not say so")),
                    false => Diag::error(value.span, format!("this returns a borrow that comes from `{wanted}`, and `from` names only {}", said.join(", "))),
                };
                return Err(diag.hint(format!("`from {}`", all.join(", "))));
            }
            if var.declared.is_borrow() {
                // A local that holds a borrow: what it borrowed is in the list too.
                continue;
            }
            let diag = Diag::error(value.span, format!("this returns a borrow of `{name}`, and `{name}` is gone when the function returns"));
            let diag = match loan.span == value.span {
                true => diag,
                false => diag.related(loan.span, "it is borrowed here"),
            };
            return Err(match var.param {
                true => diag.hint(format!("take `{name}` as `read`, so that the caller keeps it")),
                false => diag.hint("return the value itself instead of a borrow of it"),
            });
        }
        Ok(())
    }

    /// Two borrows written in one call that do not go together: the same place lent twice,
    /// one of them as `ref`. `recv`: the receiver of a method and whether it is taken as `ref`;
    /// `params`: what the call takes, as far as it is known.
    ///
    /// A `ref` given on as it is (`wipe(xs)`, with `xs` a `ref` parameter, variable or field)
    /// lends what it borrows again, to be changed (choice 1592): the call changes that place, as
    /// it does one written `ref xs`, and it goes with no other borrow of it in the call.
    fn lent_twice(&mut self, recv: Option<(&Expr, bool)>, args: &[Expr], params: &[Ty]) -> Res<()> {
        let given = |at: usize, arg: &Expr| matches!(params.get(at), Some(Ty::Ref(_))) && arg.is_place() && !matches!(arg.kind, ExprKind::Borrow { .. });
        for (at, arg) in args.iter().enumerate() {
            if given(at, arg) {
                self.not_locked(arg, Access::Write)?;
            }
        }
        let places: Vec<(&Expr, bool)> = recv
            .into_iter()
            .chain(args.iter().enumerate().filter_map(|(at, arg)| match &arg.kind {
                _ if given(at, arg) => Some((arg, true)),
                ExprKind::Borrow { mutable, expr } => Some((&**expr, *mutable)),
                // `ref xs[a..b]`, kept as the range alone: its items are lent to be changed.
                ExprKind::Slice { .. } if matches!(arg.ty, Ty::Ref(_)) => Some((arg, true)),
                // The items whose address C is given may be written by it.
                ExprKind::Method { recv, callee: Callee::Pointer, .. } => Some((&**recv, true)),
                ExprKind::RawCast { expr, .. } => match &expr.kind {
                    ExprKind::Method { recv, callee: Callee::Pointer, .. } => Some((&**recv, true)),
                    _ => None,
                },
                _ => None,
            }))
            .collect();
        // An argument that looks at a place another one lends as `ref` without being written
        // as a borrow (`Hex.encodeInto(ref s, s.asBytes())`, `grow(ref s, s[1..])`): the call
        // may change the place while it reads the view (choice 1592).
        let changed: Vec<(usize, String, &Expr)> =
            places.iter().filter(|(_, mutable)| *mutable).filter_map(|(place, _)| lent_path(place).and_then(|(name, path)| self.var_named(name).map(|var| (var.id, path, *place)))).collect();
        if !changed.is_empty() {
            let lent_itself = |arg: &Expr| match &arg.kind {
                ExprKind::Borrow { .. } | ExprKind::Method { callee: Callee::Pointer, .. } => true,
                ExprKind::RawCast { expr, .. } => matches!(expr.kind, ExprKind::Method { callee: Callee::Pointer, .. }),
                _ => places.iter().any(|(place, _)| std::ptr::eq(*place, arg)),
            };
            for arg in args.iter().filter(|arg| !lent_itself(arg)) {
                for loan in self.part_loans(arg, false) {
                    let overlaps = |path: &str| path.is_empty() || loan.path.is_empty() || within(path, &loan.path) || within(&loan.path, path);
                    let Some((_, _, place)) = changed.iter().find(|(root, path, _)| *root == loan.root && overlaps(path)) else { continue };
                    let (text, into) = (self.src[arg.span.start..arg.span.end].trim(), range_named(&self.src[place.span.start..place.span.end]));
                    return Err(Diag::error(arg.span, format!("`{text}` looks at `{into}`, which this call is given to change"))
                        .related(place.span, "given as `ref` here")
                        .note(format!("the call may change `{into}` while `{text}` is read, and it would be read from what was there"))
                        .hint("make it a value of its own before the call (`.clone()`, `.toList()`), and give the call that"));
                }
            }
        }
        self.lent_places(places)
    }

    /// The places lent to one call: a `ref` borrow must be the only one of its place.
    fn lent_places(&mut self, places: Vec<(&Expr, bool)>) -> Res<()> {
        let mut lent: Vec<(usize, String, bool, Span)> = Vec::new();
        for (place, mutable) in places {
            let Some((name, path)) = lent_path(place) else { continue };
            let Some((root, declared)) = self.find(name).map(|var| (var.id, var.declared.clone())) else { continue };
            let clash = lent.iter().find(|(other, other_path, other_mutable, _)| {
                let overlaps = path.is_empty() || other_path.is_empty() || within(&path, other_path) || within(other_path, &path);
                *other == root && overlaps && (mutable || *other_mutable)
            });
            if let Some((_, _, _, first)) = clash {
                let text = range_named(&self.src[place.span.start..place.span.end]);
                // A `shared checked` value lent to change and to read stops the program when it
                // runs, as `opening.rs` says of one lent through two owners (choice 1401).
                let diag = match (path.is_empty(), declared.strip_borrow()) {
                    (true, Ty::Shared(_, true)) => Diag::error(place.span, format!("`{text}` is lent twice in one call, and one of them changes it: this line would stop the program every time it ran"))
                        .note("a `shared checked` value is lent to one that changes it or to any number that read it, not both at once"),
                    _ => Diag::error(place.span, format!("`{text}` is lent twice in one call, and a `ref` borrow must be the only one")),
                };
                return Err(diag.related(*first, "the other borrow is here").hint("copy what is needed into a variable before the call"));
            }
            lent.push((root, path, mutable, place.span));
        }
        Ok(())
    }

    /// `text.write(x)` where `x` looks at `text`: the text may move to a larger block while
    /// it is written, and `x` would be read from the old one. Numbers, `bool`s and characters
    /// in a `$"..."` are computed before anything is written, so they may come from it.
    /// `list.addAll(items)` where `items` looks at `list` is refused for the same reason.
    fn written_into_itself(&self, recv: &Expr, arg: &Expr) -> Res<()> {
        let Some((name, path)) = store_path(recv) else { return Ok(()) };
        let Some(root) = self.var_named(name).map(|var| var.id) else { return Ok(()) };
        let overlaps = |other: &str| path.is_empty() || other.is_empty() || within(&path, other) || within(other, &path);
        let mut parts = vec![arg];
        while let Some(part) = parts.pop() {
            if let ExprKind::Interp(pieces) = &part.kind {
                parts.extend(pieces.iter().filter_map(|piece| match piece {
                    InterpPart::Expr(inner, _) => Some(inner),
                    _ => None,
                }));
                continue;
            }
            if matches!(part.ty.strip_borrow(), Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char) {
                continue;
            }
            let same_place = store_path(part).is_some_and(|(other, other_path)| self.var_named(other).is_some_and(|var| var.id == root) && overlaps(&other_path));
            // The mark "what this parameter holds" is not the parameter's own fields: a named
            // place of it (`read this.extra` while `this.out` grows) is told apart by its path.
            let mut place = part;
            while let ExprKind::Borrow { expr: inner, .. } | ExprKind::AsSlice(inner) | ExprKind::Slice { recv: inner, .. } = &place.kind {
                place = inner;
            }
            let named = store_path(place).is_some();
            let lent = self.loans_of(part, false).iter().any(|loan| loan.root == root && !(loan.held && named) && overlaps(&loan.path));
            if same_place || lent {
                let (text, into) = (&self.src[part.span.start..part.span.end], &self.src[recv.span.start..recv.span.end]);
                let items = matches!(part.ty.strip_borrow(), Ty::Slice(_));
                let diag = match (text == into || text == format!("read {into}"), items) {
                    (true, false) => Diag::error(part.span, format!("`{into}` is written into itself"))
                        .note(format!("`{into}` may move to a larger block while it grows, and what is written would be read from the old one")),
                    (true, true) => Diag::error(part.span, format!("`{into}` is added to itself"))
                        .note(format!("`{into}` may move to a larger block while it grows, and the items would be read from the old one")),
                    (false, _) => Diag::error(part.span, format!("`{text}` looks at `{into}`, which this call {}", if items { "adds to" } else { "writes into" }))
                        .note(format!("`{into}` may move to a larger block while it grows, and `{text}` would be read from the old one")),
                };
                return Err(match part.ty.strip_borrow() {
                    Ty::Str => diag.hint(format!("copy it first: `var copy = {text}.clone()`")),
                    Ty::Slice(_) => diag.hint(format!("add the items one by one: `{into}.add({into}[i])` copies each before the list grows")),
                    _ => diag.hint("write it into a text of its own first"),
                });
            }
        }
        Ok(())
    }

    /// Which variable a name means right now: scope and position in the scope.
    /// Where the variable with this id is: its scope and its position in the scope.
    fn var_at(&self, id: usize) -> Option<(usize, usize)> {
        self.scopes.iter().enumerate().find_map(|(depth, scope)| scope.vars.iter().position(|var| var.id == id).map(|position| (depth, position)))
    }

    fn var_id(&self, name: &str) -> Option<(usize, usize)> {
        self.scopes.iter().enumerate().rev().find_map(|(depth, scope)| {
            scope.vars.iter().rposition(|var| var.name == name).map(|position| (depth, position))
        })
    }

    /// A place is about to be changed, lent as `ref` or moved. Rejects it while a `for` goes
    /// over it, and notes it for the borrows that variables hold.
    fn not_locked(&mut self, expr: &Expr, access: Access) -> Res<()> {
        self.touch(expr, access);
        if self.locks.is_empty() {
            return Ok(());
        }
        let Some((name, path)) = store_path(expr) else { return Ok(()) };
        let Some(id) = self.var_id(name) else { return Ok(()) };
        let overlaps = |lock: &Lock| path.is_empty() || lock.path.is_empty() || within(&path, &lock.path) || within(&lock.path, &path);
        let Some(lock) = self.locks.iter().find(|lock| lock.var == id && overlaps(lock)) else { return Ok(()) };
        let text = &self.src[expr.span.start..expr.span.end];
        let over = &lock.text;
        let diag = Diag::error(expr.span, format!("`{text}` cannot be changed or moved while a `for` goes over `{over}`"))
            .related(lock.span, "the loop is here");
        Err(match lock.positions {
            true => diag.hint(format!("go over the positions instead (`for i in 0..{over}.length()`), or keep the changes in another list until the loop ends")),
            false => diag.hint("keep the changes in a list until the loop ends, and apply them after it"),
        })
    }

    fn narrow(&mut self, name: &str) {
        let Some(var) = self.find(name) else { return };
        let Some(narrowed) = without_null(&var.current) else { return };
        let saved = (name.to_string(), std::mem::replace(&mut var.current, narrowed), var.assigned);
        self.scopes.last_mut().expect("an open scope").narrowed.push(saved);
    }

    /// Closes the scope. A `Result` kept in a variable that nothing looked at is an error:
    /// the failure it may hold would be lost silently.
    fn pop_scope(&mut self, reached: bool) -> Res<()> {
        if reached {
            self.unclosed(self.scopes.len() - 1, None)?;
        }
        let scope = self.scopes.pop().expect("an open scope");
        // A `Disposable` value that borrows is used once more where its block ends: its
        // `dispose` runs there. What it borrows from must still be there then, also an owner of
        // the same block declared after it, which is gone before it.
        for (index, var) in scope.vars.iter().enumerate() {
            let disposes = !var.declared.is_borrow() && holds_disposable(&var.declared, self.structs, self.enums, &mut Vec::new());
            if var.loans.is_empty() || !disposes || (var.moved.whole.is_some() && !var.moved.partly) {
                continue;
            }
            let at = self.point(self.borrows.pos);
            self.borrows.disposed.push(at.seq);
            self.borrows.uses.push((var.id, at));
            // Its `dispose` may change what its `ref` fields borrow (choice 1592).
            for loan in var.loans.iter().filter(|loan| loan.mutable && !loan.held) {
                self.touch_root(loan.root, &loan.path, Access::Write, var.at, None, var.id);
            }
            let later = var.loans.iter().filter(|loan| !loan.held).find_map(|loan| scope.vars[index + 1..].iter().find(|owner| owner.id == loan.root && !owner.declared.is_borrow()).map(|owner| (loan, owner)));
            if let Some((loan, owner)) = later {
                let (name, owner) = (&var.name, &owner.name);
                return Err(Diag::error(loan.span, format!("`{name}` borrows from `{owner}`, which is declared after it: at the end of the block `{owner}` is gone first, and the `dispose` of `{name}` would still look at it"))
                    .hint(format!("declare `{owner}` before `{name}`")));
            }
        }
        // A variable outside that still holds a borrow of an owner that ends here.
        self.scopes.push(scope);
        self.owners_gone(self.scopes.len() - 1);
        let scope = self.scopes.pop().expect("the scope just put back");
        if let Some((var, span)) = scope.vars.iter().find_map(|var| var.unopened.map(|span| (var, span))) {
            let name = &var.name;
            return Err(Diag::error(span, format!("`{name}` holds a `Result` that is never looked at, so an error would be lost"))
                .hint(format!("open it with `try {name}`, `{name} ?? ...` or `match {name} {{ ... }}`; to drop it on purpose write `_ = ...`")));
        }
        for (name, before, assigned) in scope.narrowed.into_iter().rev() {
            let Some(var) = self.find(&name) else { continue };
            var.current = if var.assigned == assigned { before } else { var.declared.clone() };
        }
        Ok(())
    }

    /// The owners of the scopes from `depth` up end here: each borrow of one that a variable of
    /// a scope below holds is a clash, an error if that variable is used afterwards.
    fn owners_gone(&mut self, depth: usize) {
        let at = self.point(self.borrows.pos);
        let (holders, ending) = self.scopes.split_at(depth);
        for owner in ending.iter().flat_map(|scope| &scope.vars).filter(|var| !var.declared.is_borrow() || var.opened) {
            for holder in holders.iter().flat_map(|outer| &outer.vars) {
                for loan in holder.loans.iter().filter(|loan| loan.root == owner.id && !loan.held) {
                    self.borrows.clashes.push(Clash {
                        holder: holder.id,
                        holder_name: holder.name.clone(),
                        owner_name: owner.name.clone(),
                        closed: owner.opened,
                        copy_as: None,
                        holder_loops: holder.decl_loops.clone(),
                        clears: clears(holder),
                        loan: loan.clone(),
                        access: Access::Gone,
                        span: loan.span,
                        at: at.clone(),
                    });
                }
            }
        }
    }

    /// What is moved out of each variable, and what each borrows: the state that the
    /// branches of an `if` or a `match` and the rounds of a loop start from and join.
    fn moves(&self) -> Moves {
        self.scopes.iter().map(|scope| scope.vars.iter().map(|var| Moved { loans: var.loans.clone(), ..var.moved.clone() }).collect()).collect()
    }

    fn set_moves(&mut self, moves: &Moves) {
        for (scope, saved) in self.scopes.iter_mut().zip(moves) {
            for (var, moved) in scope.vars.iter_mut().zip(saved) {
                var.loans = moved.loans.clone();
                var.moved = Moved { loans: Vec::new(), ..moved.clone() };
            }
        }
    }

    /// After two branches a variable is gone if either branch moved it, and it borrows what
    /// it borrowed on either.
    fn merge_moves(&mut self, other: &Moves) {
        for (scope, saved) in self.scopes.iter_mut().zip(other) {
            for (var, moved) in scope.vars.iter_mut().zip(saved) {
                for loan in &moved.loans {
                    if !var.loans.iter().any(|known| same_loan(known, loan)) {
                        var.loans.push(loan.clone());
                    }
                }
                var.moved.partly = var.moved.partly || moved.partly || var.moved.whole.is_some() != moved.whole.is_some();
                if var.moved.whole.is_none() {
                    var.moved.emptied = moved.emptied;
                }
                var.moved.whole = var.moved.whole.or(moved.whole);
                for part in &moved.parts {
                    if !var.moved.parts.iter().any(|(path, _)| *path == part.0) {
                        var.moved.parts.push(part.clone());
                    }
                }
            }
        }
    }

    /// A `when` condition only looks: if it moved a value and then did not hold, the arms
    /// below would run without it. `before`: what was moved when the condition started.
    fn only_looked(&self, before: &Moves) -> Res<()> {
        for (scope, before) in self.scopes.iter().zip(before) {
            for (var, before) in scope.vars.iter().zip(before) {
                let whole = var.moved.whole.filter(|_| before.whole.is_none()).map(|at| (var.name.clone(), at));
                let part = var.moved.parts.iter().find(|(path, _)| !before.parts.iter().any(|(old, _)| old == path));
                let Some((what, at)) = whole.or(part.map(|(path, at)| (format!("{}.{path}", var.name), *at))) else { continue };
                return Err(Diag::error(at, format!("`{what}` cannot be moved in a `when` condition"))
                    .note("if the condition does not hold, the arms below still need the value")
                    .hint("a condition only looks: lend the value with `read`, or test it inside the arm"));
            }
        }
        Ok(())
    }

    // ---- statements ----

    /// A block in which the variables `narrow` are known not to be `null`.
    fn block(&mut self, block: &mut Block, narrow: &[String]) -> Res<()> {
        self.block_with(block, narrow, Vec::new())
    }

    /// A block that starts with the names of a pattern in scope.
    fn block_with(&mut self, block: &mut Block, narrow: &[String], binds: Binds) -> Res<()> {
        self.scopes.push(Scope::default());
        for name in narrow {
            self.narrow(name);
        }
        self.declare_binds(binds)?;
        for stmt in block.iter_mut() {
            self.stmt(stmt)?;
        }
        // What happens where the block ends (a `dispose`, an owner gone) comes after all of it.
        if let Some(last) = block.last() {
            self.borrows.pos = self.borrows.pos.max(last.span.end);
        }
        let reached = !leaves(block, self.loops > 0);
        self.pop_scope(reached)
    }

    /// Declares the names of a pattern. Those that are borrows hold what the pattern's value
    /// was lent from.
    fn declare_binds(&mut self, binds: Binds) -> Res<()> {
        let mut loans = std::mem::take(&mut self.borrows.binds);
        let opened = std::mem::take(&mut self.borrows.opening) | loans.iter().any(|loan| loan.root == OPENS);
        loans.retain(|loan| loan.root != OPENS);
        for (name, ty, span) in binds {
            let borrows = self.holds(&ty);
            self.declare_local(&name, ty, span)?;
            let var = self.find(&name).expect("just declared");
            var.opened = opened;
            if borrows {
                var.loans = loans.clone();
            }
        }
        Ok(())
    }

    /// `Log.debug(text)` or `std.Log.debug(text)` as a statement: the level of the language's
    /// `Log` it writes at, when the call is not guarded yet and `Log` is the language's, not a
    /// variable, a const, a static or a type of the program (choice 1456).
    fn log_level_of(&self, expr: &Expr) -> Option<&'static str> {
        let ExprKind::Method { recv, name, .. } = &expr.kind else { return None };
        if self.log_guarded.contains(&expr.span.start) {
            return None;
        }
        let language = match &recv.kind {
            ExprKind::Var(owner) if owner == "Log" => {
                self.var_named("Log").is_none() && matches!(self.names.full("Log", recv.span), Ok(Some(full)) if full == format!("{STD_MODULE}.Log"))
            }
            ExprKind::Field { recv: module, name, .. } if name == "Log" => {
                matches!(&module.kind, ExprKind::Var(written) if written == STD_MODULE) && self.var_named(STD_MODULE).is_none()
            }
            _ => false,
        };
        if !language {
            return None;
        }
        Some(match name.as_str() {
            "trace" => "Trace",
            "debug" => "Debug",
            "info" => "Info",
            "warn" => "Warn",
            "error" => "Error",
            _ => return None,
        })
    }

    /// `Log.debug(text)` becomes `if Log.isOn(std.LogLevel.Debug) { Log.debug(text) }`, so that the
    /// text of a level that is off is never built (choice 1447); a `$"..."` given to it is made a
    /// `string` there, inside the `if`.
    fn guard_log(&mut self, stmt: &mut Stmt) {
        let StmtKind::Expr(expr) = &mut stmt.kind else { return };
        let Some(level) = self.log_level_of(expr) else { return };
        self.log_guarded.insert(expr.span.start);
        let span = expr.span;
        let ExprKind::Method { recv, name_span, args, .. } = &mut expr.kind else { unreachable!("tested above") };
        let name_span = *name_span;
        for arg in args.iter_mut() {
            if matches!(arg.kind, ExprKind::Interp(_)) {
                let at = arg.span;
                let text = std::mem::replace(arg, Expr { kind: ExprKind::Null, span: at, ty: Ty::Void, moved: false });
                let made = Expr {
                    kind: ExprKind::Method { recv: Box::new(text), name: "toString".to_string(), name_span: at, args: Vec::new(), callee: Callee::Plain },
                    span: at,
                    ty: Ty::Void,
                    moved: false,
                };
                *arg = Expr { kind: ExprKind::Borrow { mutable: false, expr: Box::new(made) }, span: at, ty: Ty::Void, moved: false };
            }
        }
        let at = |kind: ExprKind| Expr { kind, span: name_span, ty: Ty::Void, moved: false };
        let owner = |name: &str| at(ExprKind::Var(name.to_string()));
        let log = (**recv).clone();
        // `std.LogLevel.Debug`: the language's, whatever the program names `LogLevel`; a variable
        // named `std` hides the module, and then `LogLevel` is written alone.
        let levels = match self.var_named(STD_MODULE) {
            None => at(ExprKind::Field { recv: Box::new(owner(STD_MODULE)), name: "LogLevel".to_string(), name_span, index: 0 }),
            Some(_) => owner("LogLevel"),
        };
        let level = at(ExprKind::Field { recv: Box::new(levels), name: level.to_string(), name_span, index: 0 });
        let cond = Expr {
            kind: ExprKind::Method { recv: Box::new(log), name: "isOn".to_string(), name_span, args: vec![level], callee: Callee::Plain },
            span,
            ty: Ty::Void,
            moved: false,
        };
        let call = std::mem::replace(&mut stmt.kind, StmtKind::Break(None));
        stmt.kind = StmtKind::If { cond, then: vec![Stmt { kind: call, span: stmt.span }], els: None, after: Vec::new() };
    }

    fn stmt(&mut self, stmt: &mut Stmt) -> Res<()> {
        self.guard_log(stmt);
        let stmt_span = stmt.span;
        match &mut stmt.kind {
            StmtKind::Let { ty, name, name_span, init } => {
                let ty = match ty {
                    Some(written) => {
                        if written.mode.is_some() {
                            return Err(Diag::error(written.span, "a local variable is not declared with `read` or `ref`")
                                .hint(format!("let `var` find the type: `var {name} = ...`")));
                        }
                        let ty = resolve_type(written, self.names, false)?;
                        self.typed(init, &ty)?;
                        ty
                    }
                    None => match self.expr(init, None)? {
                        // A borrowed variable used as a value is the value behind it.
                        Ty::Read(inner) | Ty::Ref(inner) if matches!(init.kind, ExprKind::Var(_) | ExprKind::Const(_) | ExprKind::Static(_)) && inner.is_copyable(self.copyable) => {
                            wrap(init, ExprKind::Load);
                            init.ty = (*inner).clone();
                            *inner
                        }
                        Ty::Void => return Err(Diag::error(init.span, "this call does not return a value")),
                        Ty::Null => {
                            return Err(Diag::error(init.span, "`var` cannot find a type from `null` alone")
                                .hint(format!("write the type, for example `int? {name} = null`")))
                        }
                        ty => ty,
                    },
                };
                // A `ref` copied out of a place (`var b = w.items`) lends what it borrows again,
                // to be changed: it goes with no other borrow of that, and is not reached
                // through a `read` borrow (choice 1592).
                if matches!(ty, Ty::Ref(_)) && init.is_place() {
                    self.not_through_read(init)?;
                    self.not_locked(init, Access::Write)?;
                }
                let holds_result = matches!(ty, Ty::Result(..));
                let loans = match self.holds(&ty) {
                    true => self.loans_of(init, matches!(ty, Ty::Ref(_))),
                    false => Vec::new(),
                };
                self.kept_unnamed(&loans, init, &format!("`{name}`"))?;
                let sender_of = self.sender_of(init);
                let unsent = (matches!(ty, Ty::Channel(_)) && matches!(init.kind, ExprKind::Collection { .. })).then_some(*name_span);
                self.declare_local(name, ty, *name_span)?;
                self.find(name).expect("just declared").loans = loans;
                self.find(name).expect("just declared").sender_of = sender_of;
                self.find(name).expect("just declared").unsent = unsent;
                if holds_result {
                    self.find(name).expect("just declared").unopened = Some(*name_span);
                }
            }
            StmtKind::Discard(value) => match self.expr(value, None)? {
                Ty::Void => {
                    return Err(Diag::error(value.span, "this call does not return a value, so there is nothing to discard")
                        .hint("write the call alone, without `_ =`"));
                }
                Ty::Null => return Err(Diag::error(value.span, "there is nothing to discard in `null`")),
                ty if task_list(&ty) => {
                    return Err(Diag::error(value.span, "a list of tasks is not dropped: they would go on with no one to wait for them")
                        .note("every task is waited for, and none is cancelled without being told")
                        .hint(format!("wait for them: {}", when_all(&ty, &self.src[value.span.start..value.span.end], self.is_async))));
                }
                Ty::Group => {
                    return Err(Diag::error(value.span, "a group of tasks is not dropped: its tasks would go on with no one to wait for them")
                        .note("every task is waited for, and none is cancelled without being told")
                        .hint(match self.is_async {
                            true => "keep it in a variable and wait for its tasks: `await group.whenAll()`",
                            false => "keep it in a variable and wait for its tasks: `group.whenAll().wait()`",
                        }));
                }
                Ty::Task(_) => {
                    return Err(Diag::error(value.span, "a task is not dropped: it would go on with no one to wait for it")
                        .note("every task is waited for, and none is cancelled without being told")
                        .hint("keep it in a variable and wait for it: `task.wait()`"));
                }
                _ => {}
            },
            StmtKind::Assign { target, op, value, through } => {
                // The right side is computed first; then the place is found and written (Rust's
                // order). What the place uses and the write itself count as after the right side,
                // so a borrow that the right side reads last ends before the write (`b = Box { n
                // = r.n + 1 }`), and a borrow the place goes through must outlive the right side.
                let (first_use, first_clash) = (self.borrows.uses.len(), self.borrows.clashes.len());
                if let Some(name) = self.static_root(target) {
                    return Err(Checker::static_changed(&name, target.span));
                }
                if let Some(name) = self.const_root(target) {
                    return Err(Checker::const_changed(&name, target.span));
                }
                // `opterr = 0`: a variable of C is an item of raw memory.
                let cvar = match &target.kind {
                    ExprKind::Var(name) if name != "this" && self.find(name).is_none() => self.names.full(name, target.span)?.is_some_and(|full| self.names.cvars.contains(&full)),
                    _ => false,
                };
                if cvar {
                    self.settle(target)?;
                }
                let target_ty = match &target.kind {
                    ExprKind::Var(name) => {
                        // Writing through a `ref` variable uses the borrow it holds.
                        let through_ref = self.find(name).is_some_and(|var| matches!(var.declared, Ty::Ref(_)));
                        let var = if op.is_some() || through_ref { self.live_var(name, target.span)? } else { self.var(name, target.span)? };
                        match &var.declared {
                            // Assigning to a `ref` variable writes to what it borrows.
                            Ty::Ref(inner) => {
                                *through = true;
                                (**inner).clone()
                            }
                            Ty::Read(_) if op.is_some() => {
                                return Err(Diag::error(target.span, format!("`{name}` is a `read` borrow and cannot be written through")))
                            }
                            declared => declared.clone(),
                        }
                    }
                    _ => {
                        self.writing = true;
                        self.written = written_path(target);
                        let found = self.place_unchecked(target);
                        self.writing = false;
                        self.written.clear();
                        let Some((ty, writable)) = found? else {
                            return Err(Diag::error(target.span, "a variant of an enum is a value, not a place to write to"));
                        };
                        self.check_parts(target, true)?;
                        if matches!(target.kind, ExprKind::Index { kind: IndexKind::Text, .. }) {
                            return Err(Diag::error(target.span, "a byte cannot be written into a `string`")
                                .note("a `string` is always valid UTF-8; `text[i]` only reads"));
                        }
                        if let Some(diag) = self.readonly(target) {
                            return Err(diag);
                        }
                        // A field of a union is written whole, and nothing is written inside it:
                        // that would read the field first.
                        if let Some((field, direct)) = union_field(target, self.structs) {
                            let text = &self.src[field.start..field.end];
                            if !direct {
                                return Err(Diag::error(target.span, "a field of a union is written whole: what lies inside it is not written on its own")
                                    .hint(format!("read it, change the copy and write it back: `var part = unsafe {text}`, then `{text} = part`")));
                            }
                            if op.is_some() {
                                return Err(Diag::error(target.span, "a field of a union is read inside `unsafe`, so it is not changed in place")
                                    .hint(format!("`{text} = unsafe {text} + ...`")));
                            }
                        }
                        if !writable && self.is_entry_key(target) {
                            return Err(Diag::error(target.span, "the key of an entry cannot be changed: the map found the entry by it")
                                .hint("remove the entry and add it under the new key: `map.remove(key)`, `map.add(key, value)`"));
                        }
                        if !writable && target.in_item() {
                            return Err(self.read_only(target, Diag::error(target.span, "this item is reached through a `read` borrow and cannot be written")
                                .hint("take the value as `ref` instead of `read`")));
                        }
                        if !writable {
                            return Err(self.read_only(target, Diag::error(target.span, "this field is reached through a `read` borrow and cannot be written")
                                .hint("take the value as `ref` instead of `read`")));
                        }
                        ty
                    }
                };
                if let ExprKind::Var(name) = &target.kind {
                    if let Some(declared) = self.find(name).map(|var| var.declared.clone()) {
                        target.ty = declared;
                    }
                }
                // Pointing a `read` variable at something else changes nothing it looked at.
                let rebinds = match &target.kind {
                    ExprKind::Var(name) => self.find(name).is_some_and(|var| matches!(var.declared, Ty::Read(_))),
                    _ => false,
                };
                if !rebinds {
                    self.not_locked(target, Access::Write)?;
                }
                let (place_uses, place_clashes) = (first_use..self.borrows.uses.len(), first_clash..self.borrows.clashes.len());
                // An atomic value is changed by its methods, each one step; a whole new one is
                // written only as `atomic e`, where nothing else reaches the place.
                if let Ty::Atomic(_) = &target_ty {
                    let text = &self.src[target.span.start..target.span.end];
                    let value_text = &self.src[value.span.start..value.span.end];
                    if let Some(op) = *op {
                        let method = match op {
                            BinOp::Add => Some("fetchAdd"),
                            BinOp::Sub => Some("fetchSub"),
                            BinOp::BitAnd => Some("fetchAnd"),
                            BinOp::BitOr => Some("fetchOr"),
                            BinOp::BitXor => Some("fetchXor"),
                            _ => None,
                        };
                        let diag = Diag::error(stmt_span, format!("`{}=` would read `{text}` and write it in two steps, and another task could change it between them", op.symbol()));
                        return Err(match method {
                            Some(method) => diag.hint(format!("one step: `_ = {text}.{method}({value_text})`")),
                            None => diag.hint("read it, compute, and write it back with `compareExchange` in a loop, so that a change in between is seen"),
                        });
                    }
                }
                if let (Some(op), Ty::Float(_)) = (*op, &target_ty) {
                    if !matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem) {
                        return Err(Diag::error(stmt_span, format!("`{}=` needs an integer on the left, found `{target_ty}`", op.symbol()))
                            .note("the bit operators work on integers"));
                    }
                } else if op == &Some(BinOp::Add) && target_ty == Ty::Str {
                    let text = &self.src[target.span.start..target.span.end];
                    return Err(Diag::error(stmt_span, "`+=` needs a number on the left, found `string`")
                        .hint(format!("a text grows with `write`: `{text}.write(...)`")));
                } else if let (Some(op), Ty::Int(ty)) = (*op, &target_ty) {
                    if ty.is_c() {
                        let text = &self.src[target.span.start..target.span.end];
                        let value_text = &self.src[value.span.start..value.span.end];
                        let partner = c_partner(*ty).name();
                        return Err(Diag::error(stmt_span, format!("`{}=` is not defined for `{}`: C's `{}` is 32 bits on Windows and 64 elsewhere, so the same sum would overflow on one system only", op.symbol(), ty.name(), c_spelling(*ty)))
                            .hint(format!("compute in `{partner}`: `{text} = ({})(({partner}){text} {} {value_text})`", ty.name(), op.symbol())));
                    }
                } else if op.is_some() && !matches!(target_ty, Ty::Int(_)) {
                    return Err(Diag::error(
                        stmt_span,
                        match op.expect("checked") {
                            op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem) => format!("`{}=` needs a number on the left, found `{target_ty}`", op.symbol()),
                            op => format!("`{}=` needs an integer on the left, found `{target_ty}`", op.symbol()),
                        },
                    ));
                }
                match (*op, &target_ty) {
                    // `x <<= n`: the count is any integer, as in `x << n`.
                    (Some(op), Ty::Int(ty)) if op.is_shift() => {
                        self.int_operand(value, None, "a shift count")?;
                        shift_count(*ty, value)?;
                    }
                    // `count = 2` on an `atomic int` is meant as `count.store(2)`.
                    (None, Ty::Atomic(_)) => {
                        self.typed(value, &target_ty).map_err(|diag| match &value.ty {
                            found if !matches!(found.strip_borrow(), Ty::Atomic(_)) && !matches!(value.kind, ExprKind::Atomic(_)) => {
                                let (text, value_text) = (&self.src[target.span.start..target.span.end], &self.src[value.span.start..value.span.end]);
                                Diag::error(value.span, format!("an `{target_ty}` is written with `store`, and this is `{found}`")).hint(format!("`{text}.store({value_text})`"))
                            }
                            _ => diag,
                        })?;
                    }
                    // `count = 1` on a `read int`: `read 1` would borrow a value that is gone
                    // when the line ends, so the hint is the copy that can be changed.
                    (None, Ty::Read(inner)) if rebinds => {
                        let inner = (**inner).clone();
                        self.typed(value, &target_ty).map_err(|diag| {
                            let named = matches!(value.kind, ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. });
                            match (&target.kind, value.ty == inner && !named) {
                                (ExprKind::Var(name), true) => Diag::error(target.span, format!("`{name}` is a `read` borrow and cannot be written through"))
                                    .note(format!("`{name} = ...` only points it at another value it borrows"))
                                    .hint(format!("change a copy of your own: `var own = {name}`; or take `ref {inner}` to change the caller's")),
                                _ => diag,
                            }
                        })?;
                    }
                    _ => {
                        self.typed(value, &target_ty)?;
                    }
                }
                // A number written in the program that does not fit a bit field.
                if let (None, ExprKind::Field { recv, index, .. }) = (*op, &target.kind) {
                    if let Some(width) = struct_of(&recv.ty).and_then(|name| self.structs.get(name)).and_then(|info| info.shape.bits.get(*index).copied().flatten()) {
                        fits_bits(value, &target_ty, width)?;
                    }
                }
                for (_, at) in &mut self.borrows.uses[place_uses] {
                    at.pos = value.span.end;
                    at.placed = Some(value.span.start);
                }
                for clash in &mut self.borrows.clashes[place_clashes] {
                    clash.at.pos = value.span.end;
                    clash.at.placed = Some(value.span.start);
                }
                // The old value of a variable whose type holds a `Disposable` is disposed of
                // here, and its `dispose` still looks at what it borrowed (choice 1592).
                if let (None, ExprKind::Var(name)) = (*op, &target.kind) {
                    let old = self.var_named(name).filter(|var| !var.declared.is_borrow() && !var.loans.is_empty()).map(|var| (var.id, var.declared.clone()));
                    if let Some((id, _)) = old.filter(|(_, declared)| holds_disposable(declared, self.structs, self.enums, &mut Vec::new())) {
                        let at = self.point(value.span.end);
                        self.borrows.disposed.push(at.seq);
                        self.borrows.uses.push((id, at));
                    }
                }
                // A value that borrows, put into a field, an item or a variable of a type that
                // carries a borrow.
                if !rebinds && self.holds(&target_ty) {
                    self.stores(target, value, false)?;
                }
                // A `read` variable pointed at something else holds what the new value borrowed.
                if rebinds {
                    let loans = self.loans_of(value, false);
                    self.kept_unnamed(&loans, value, "the variable")?;
                    if let ExprKind::Var(name) = &target.kind {
                        let var = self.var(name, target.span)?;
                        let id = var.id;
                        var.loans = loans.into_iter().filter(|loan| loan.root != id).collect();
                    }
                }
                match path_of(target) {
                    Some((name, path)) if path.is_empty() => {
                        if let Some(what) = std_stream(name) {
                            return Err(Diag::error(target.span, format!("`{name}` is {what} of the program and cannot be given another value")));
                        }
                        let is_async = self.is_async;
                        let var = self.var(name, target.span)?;
                        if task_list(&var.declared) && (var.moved.whole.is_none() || var.moved.partly) {
                            return Err(Diag::error(target.span, format!("`{name}` is given another list while the tasks it holds are not waited for"))
                                .related(var.at, "it is declared here")
                                .hint(format!("wait for them first: {}", when_all(&var.declared, name, is_async))));
                        }
                        let var = self.var(name, target.span)?;
                        if matches!(var.declared, Ty::Task(_)) && (var.moved.whole.is_none() || var.moved.partly) {
                            return Err(Diag::error(target.span, format!("`{name}` is given another task while the one it holds is not waited for"))
                                .related(var.at, "it is declared here")
                                .hint(format!("wait for it first: `{name}.wait()`")));
                        }
                        let var = self.var(name, target.span)?;
                        if var.declared == Ty::Group && (var.moved.whole.is_none() || var.moved.partly) {
                            let wait = if is_async { format!("await {name}.whenAll()") } else { format!("{name}.whenAll().wait()") };
                            return Err(Diag::error(target.span, format!("`{name}` is given another group while the tasks it holds are not waited for"))
                                .related(var.at, "it is declared here")
                                .hint(format!("wait for them first: `{wait}`")));
                        }
                        let var = self.var(name, target.span)?;
                        if is_process(&var.declared) && (var.moved.whole.is_none() || var.moved.partly) {
                            let wait = if is_async { format!("try await {name}.waitAsync()") } else { format!("try {name}.wait()") };
                            return Err(Diag::error(target.span, format!("`{name}` is given another program while the one it holds is not waited for"))
                                .related(var.at, "it is declared here")
                                .hint(format!("wait for it first: `var status = {wait}`")));
                        }
                        let structs = self.structs;
                        let var = self.var(name, target.span)?;
                        if let Some(own) = closable(&var.declared, structs).filter(|_| var.moved.whole.is_none() || var.moved.partly) {
                            let hint = match (is_async, own) {
                                (true, None) => format!("close it first: `try await {name}.closeAsync()`; to drop the error on purpose, `_ = await {name}.closeAsync()`"),
                                _ => format!("close it first: `try {name}.close()`; to drop the error on purpose, `_ = {name}.close()`"),
                            };
                            let msg = match own {
                                None => format!("`{name}` is given another writer while the one it holds is still open, so an error writing its buffer would be lost"),
                                Some(short) => format!("`{name}` is given another `{short}` while the one it holds is not closed, so the error its `close` gives would be lost"),
                            };
                            return Err(Diag::error(target.span, msg).related(var.at, "it is declared here").hint(hint));
                        }
                        let var = self.var(name, target.span)?;
                        var.moved = Moved::default();
                        var.assigned += 1;
                        var.current = var.declared.clone();
                    }
                    // A field that was moved out holds a value again.
                    Some((name, path)) => {
                        let var = self.var(name, target.span)?;
                        var.moved.parts.retain(|(part, _)| !within(part, &path));
                    }
                    None => {}
                }
            }
            StmtKind::If { cond, then, els, after } => {
                // The condition has a scope of its own, which the block shares and `else` does
                // not: the names of `x is Circle(r)` and what `x is not null` narrows.
                self.scopes.push(Scope::default());
                let fails = if els.is_some() || leaves(then, true) { NamesSeen::Fails } else { NamesSeen::StaysIn };
                let (holds, fails) = self.condition(cond, fails)?;
                // Not `null` where the condition holds: `x is not null && ...`; where it does
                // not: `x is null || ...`, `!(x is not null)`.
                let (when_true, when_false) = condition_facts(cond);

                let before = self.moves();
                let node = self.node();
                self.borrows.branches.push((node, 0));
                self.block(then, &[])?;
                self.borrows.branches.pop();
                self.pop_scope(true)?;
                let then_leaves = leaves(then, true);
                let after_then = self.moves();
                self.set_moves(&before);
                let mut else_leaves = false;
                if let Some(els) = els {
                    // `if x is not Circle(r) { ... } else { ... }`: `else` sees `r`.
                    self.borrows.branches.push((node, 1));
                    self.scopes.push(Scope::default());
                    for (binds, loans) in fails.clone() {
                        self.declare_group(binds, loans)?;
                    }
                    let result = self.block(els, &when_false);
                    self.borrows.branches.pop();
                    result?;
                    self.pop_scope(!leaves(els, true))?;
                    else_leaves = leaves(els, true);
                }
                // A branch that leaves does not reach the code after the `if`.
                match (then_leaves, else_leaves) {
                    (true, _) => {}
                    (false, true) => self.set_moves(&after_then),
                    (false, false) => self.merge_moves(&after_then),
                }
                // `if x is null { return }`: from here on `x` is not null; `if r is not Ok(value)
                // { return }`: from here on `value` is seen.
                let (narrowed, named) = match (then_leaves, else_leaves) {
                    (true, _) => (when_false, fails),
                    (false, true) => (when_true, holds),
                    (false, false) => (Vec::new(), Vec::new()),
                };
                for name in &narrowed {
                    self.narrow(name);
                }
                *after = self.names_after(named, "if")?;
            }
            StmtKind::While { cond, body, after, label } => {
                self.forget_narrowing_of_assigned(body, Some(cond));
                let node = self.node();
                self.borrows.loops.push(node);
                self.scopes.push(Scope::default());
                let exit = first_break(body, label.as_ref());
                let fails = exit.map_or(NamesSeen::Fails, NamesSeen::Break);
                let head = self.borrows.touched.len();
                let result = self.condition(cond, fails).and_then(|(_, fails)| self.loop_body(body, &[], Vec::new(), label.as_ref(), Some(head)).map(|cx| (fails, cx)));
                self.borrows.loops.pop();
                let (fails, cx) = result?;
                self.pop_scope(true)?;
                // `while true` ends only by a `break`.
                self.after_loop(cx, !matches!(cond.kind, ExprKind::Bool(true)));
                // `while x is null { ... }`: the loop ends only when the condition fails, so
                // after it `x` is not null, unless a `break` leaves it earlier; the same for
                // the names of `while r is not Ok(value) { ... }`.
                if exit.is_none() {
                    for name in condition_facts(cond).1 {
                        self.narrow(&name);
                    }
                    *after = self.names_after(fails, "while")?;
                    // `while tasks.length() > 0 { ... }`: the loop ends only when the list has
                    // no task left, so after it every task in it was waited for.
                    if let Some(name) = emptied_by(cond) {
                        let var = self.var(name, cond.span)?;
                        if var.moved.whole.is_none() {
                            var.moved = Moved { whole: Some(cond.span), emptied: true, ..Moved::default() };
                        }
                    }
                }
            }
            StmtKind::For { var, var_span, from, to, body, label, .. } => {
                self.forget_narrowing_of_assigned(body, None);
                let from_ty = self.int_operand(from, None, "the start of the range")?;
                let to_ty = self.int_operand(to, Some(from_ty), "the end of the range")?;
                let common = self.common_int(from, Ty::Int(from_ty), to, Ty::Int(to_ty))?;
                // The counter of a range is added to, which C's `long` is not.
                if common.is_c() {
                    let partner = c_partner(common).name();
                    let (a, b) = (&self.src[from.span.start..from.span.end], &self.src[to.span.start..to.span.end]);
                    let wrap = |text: &str| if text.trim_start_matches('-').starts_with(|c: char| c.is_ascii_digit()) { text.to_string() } else { format!("({partner}){text}") };
                    return Err(Diag::error(from.span.to(to.span), format!("a range of `{}` is not counted: C's `{}` has no arithmetic", common.name(), c_spelling(common)))
                        .hint(format!("count in `{partner}`: `{}..{}`", wrap(a), wrap(b))));
                }
                let ty = Ty::Int(common);
                let node = self.node();
                self.borrows.loops.push(node);
                self.scopes.push(Scope::default());
                self.declare_local(var, ty, *var_span)?;
                let result = self.loop_body(body, &[], Vec::new(), label.as_ref(), None);
                self.borrows.loops.pop();
                let cx = result?;
                self.pop_scope(true)?;
                self.after_loop(cx, true);
            }
            StmtKind::ForEach { var, var_span, iter, body, by, label } => {
                self.forget_narrowing_of_assigned(body, None);
                // Like `match`: an owned collection is handed over to the loop, a borrowed one
                // lends its items.
                let source = match &iter.kind {
                    ExprKind::Borrow { expr, .. } => expr,
                    _ => &*iter,
                };
                // A range of a list is the list, as far as changing it goes.
                let source = match &source.kind {
                    ExprKind::Slice { recv, .. } => recv,
                    _ => source,
                };
                let source_text = self.src[source.span.start..source.span.end].to_string();
                let lock = store_path(source).and_then(|(name, path)| {
                    let text = self.src[source.span.start..source.span.end].to_string();
                    Some(Lock { var: self.var_id(name)?, path, text, positions: false, span: iter.span })
                });
                // A channel in a variable is received from, not handed over to the loop.
                let in_variable = match &iter.kind {
                    ExprKind::Var(name) => self.find(name).is_some_and(|var| matches!(var.current.strip_borrow(), Ty::Channel(_))),
                    _ => false,
                };
                let found = match in_variable {
                    true => self.place(iter)?.expect("a variable is a place").0,
                    false => self.expr(iter, None)?,
                };
                if let Ty::Channel(item) = found.strip_borrow() {
                    let item = (**item).clone();
                    self.channel_loop(iter)?;
                    *by = None;
                    self.borrows.binds.clear();
                    let node = self.node();
                    self.borrows.loops.push(node);
                    let result = self.loop_body(body, &[], vec![(var.clone(), item, *var_span)], label.as_ref(), None);
                    self.borrows.loops.pop();
                    self.after_loop(result?, true);
                    return Ok(());
                }
                let mut mode = None;
                let mut inner = &found;
                loop {
                    inner = match inner {
                        Ty::Read(behind) => {
                            mode = Some(Mode::Read);
                            behind
                        }
                        Ty::Ref(behind) => {
                            mode = mode.or(Some(Mode::Ref));
                            behind
                        }
                        Ty::Ptr(behind) => behind,
                        Ty::Locked(_) => return Err(self.locked_outside(iter.span)),
                        // What a `shared checked` value points to is lent as the loop asks.
                        Ty::Shared(behind, true) => {
                            mode = mode.or(Some(Mode::Read));
                            behind
                        }
                        Ty::Shared(behind, false) => {
                            mode = Some(Mode::Read);
                            behind
                        }
                        _ => break,
                    };
                }
                let item = match inner {
                    Ty::List(item) | Ty::Array(item, _) | Ty::Slice(item) => (**item).clone(),
                    Ty::Map(key, value) => Ty::Entry(key.clone(), value.clone()),
                    Ty::Set(_) if mode == Some(Mode::Ref) => {
                        return Err(Diag::error(iter.span, "the items of a `HashSet` cannot be changed in place: the set finds them by their value")
                            .hint("go over it with `read`, and use `remove` and `add` after the loop"));
                    }
                    Ty::Set(item) => (**item).clone(),
                    Ty::Str => {
                        let place = match &iter.kind {
                            ExprKind::Borrow { expr, .. } => expr.span,
                            _ => iter.span,
                        };
                        let text = &self.src[place.start..place.end];
                        return Err(Diag::error(iter.span, "`for` does not go over a `string`")
                            .hint(format!("the characters: `for c in {text}.chars()`; the bytes: `for b in {text}.asBytes()`")));
                    }
                    Ty::TextIter(_) if mode.is_some() => {
                        return Err(Diag::error(iter.span, "`for` takes an iterator as it is, not a borrow of it")
                            .hint("`for x in it`; the loop uses it up"));
                    }
                    Ty::TextIter(kind) => kind.item(),
                    Ty::Nullable(_) => {
                        return Err(Diag::error(iter.span, format!("this is `{found}` and may be `null`; `for` needs a value"))
                            .hint("test it first with `is not null`, or give a fallback with `??`"));
                    }
                    _ => {
                        return Err(Diag::error(iter.span, format!("`for` goes over a range `a..b`, a `List`, an array, a slice, a `HashMap` or a `HashSet`, found `{found}`")));
                    }
                };
                // A loop could leave a task of the list at a `break`: the tasks are waited for
                // first, and the loop goes over what they gave.
                if matches!(item, Ty::Task(_)) {
                    let list = match &iter.kind {
                        ExprKind::Borrow { expr, .. } => &self.src[expr.span.start..expr.span.end],
                        _ => &self.src[iter.span.start..iter.span.end],
                    };
                    let wait = match self.is_async {
                        true => format!("await Task.whenAll({list})"),
                        false => format!("Task.whenAll({list}).wait()"),
                    };
                    return Err(Diag::error(iter.span, "`for` does not go over a list of tasks: a `break` would leave the tasks after it with no one to wait for them")
                        .hint(format!("wait for them all and go over what they gave: `for value in {wait}`; or take them as they end: `while {list}.length() > 0 {{ ... Task.whenAny(ref {list}) ... }}`")));
                }
                let item = match mode {
                    None => item,
                    Some(Mode::Read) => Ty::Read(Box::new(item)),
                    Some(Mode::Ref) => Ty::Ref(Box::new(item)),
                };
                *by = mode;
                // The variable is new in every round, so an owned item can be used up.
                // A borrowed collection stays as it is until the loop ends.
                let locked = mode.is_some() && lock.is_some();
                let locked_var = lock.as_ref().map(|lock| lock.var);
                if locked {
                    let positions = matches!(found.strip_borrow(), Ty::List(_) | Ty::Ptr(_) | Ty::Array(..) | Ty::Slice(_));
                    self.locks.extend(lock.map(|lock| Lock { positions, ..lock }));
                }
                // What goes over a text keeps the text from changing while the loop runs, and
                // the parts it gives borrow the text.
                let mut text_locks = 0;
                if matches!(found, Ty::TextIter(_)) {
                    let loans = self.loans_of(iter, false);
                    self.kept_unnamed(&loans, iter, "the loop")?;
                    for loan in &loans {
                        let Some(at) = self.var_at(loan.root) else { continue };
                        let text = format!("{}{}{}", self.scopes[at.0].vars[at.1].name, if loan.path.is_empty() { "" } else { "." }, loan.path);
                        self.locks.push(Lock { var: at, path: loan.path.clone(), text, positions: false, span: iter.span });
                        text_locks += 1;
                    }
                    self.borrows.binds = loans;
                } else {
                    // An item lent by the loop borrows what the collection is: the variable
                    // that holds it, or what a borrow of it borrowed (`for x in s` with
                    // `var s = xs[0..2]`), and that is kept from changing while the loop runs.
                    // An owned item carries what the collection's items borrowed. Through a
                    // `shared` value each access is checked on its own (and a `shared checked`
                    // one is lent for the loop), and the loop keeps the variable as it is.
                    let shared = matches!(found.strip_borrow(), Ty::Shared(..));
                    let mut loans = if shared { Vec::new() } else { self.loans_of(iter, mode == Some(Mode::Ref)) };
                    // An item lent out of a `shared` value is for the round only (choice 1592).
                    if mode.is_some() {
                        loans.extend(opens_shared(iter));
                    }
                    if mode.is_some() {
                        for loan in loans.iter().filter(|loan| !loan.held) {
                            let Some(at) = self.var_at(loan.root) else { continue };
                            if locked_var == Some(at) {
                                continue;
                            }
                            let text = source_text.clone();
                            let positions = matches!(found.strip_borrow(), Ty::List(_) | Ty::Ptr(_) | Ty::Array(..) | Ty::Slice(_));
                            self.locks.push(Lock { var: at, path: loan.path.clone(), text, positions, span: iter.span });
                            text_locks += 1;
                        }
                    }
                    self.borrows.binds = loans;
                }
                let node = self.node();
                self.borrows.loops.push(node);
                let result = self.loop_body(body, &[], vec![(var.clone(), item, *var_span)], label.as_ref(), None);
                self.borrows.loops.pop();
                if locked {
                    self.locks.pop();
                }
                for _ in 0..text_locks {
                    self.locks.pop();
                }
                self.after_loop(result?, true);
            }
            StmtKind::As { mutable, value, name, name_span, body } => {
                let found = match self.place(value)? {
                    Some((ty, _)) => ty,
                    None => self.expr(value, None)?,
                };
                let word = if *mutable { "ref" } else { "read" };
                let text = self.src[value.span.start..value.span.end].to_string();
                let (inner, checked) = match found.strip_borrow() {
                    Ty::Shared(inner, _) if matches!(&**inner, Ty::Locked(_)) => {
                        return Err(Diag::error(value.span, format!("`{word} ... as` opens a `shared` value that one task uses, and this is `{found}`, which several tasks share"))
                            .hint(format!("`lock {text} as name {{ ... }}`")));
                    }
                    Ty::Shared(inner, checked) => ((**inner).clone(), *checked),
                    Ty::Nullable(inner) if matches!(&**inner, Ty::Shared(..)) => {
                        return Err(Diag::error(value.span, format!("this is `{found}` and may be `null`; `{word} ... as` needs a value"))
                            .hint(format!("test it first: `if {text} is not null {{ ... }}`")));
                    }
                    Ty::Weak(..) => {
                        return Err(Diag::error(value.span, format!("this is `{found}`: what it looks at may be gone"))
                            .hint(format!("ask for an owner first: `var owner = share {text}`, which is `null` if it is gone")));
                    }
                    other => {
                        return Err(Diag::error(value.span, format!("`{word} ... as` opens a `shared` value for a block, and this is `{other}`"))
                            .hint(format!("use `{text}` as it is")));
                    }
                };
                if *mutable && !checked {
                    return Err(Diag::error(value.span, format!("`ref ... as` changes what it holds, and a `{found}` is only read")).note(SHARED_READ_ONLY));
                }
                let ty = match mutable {
                    true => Ty::Ref(Box::new(inner)),
                    false => Ty::Read(Box::new(inner)),
                };
                // The name borrows what the value holds, so the value stays where it is for the block.
                self.borrows.binds = match value.is_place() {
                    true => self.place_loans(value, false, value.span),
                    false => Vec::new(),
                };
                self.borrows.opening = true;
                self.block_with(body, &[], vec![(name.clone(), ty, *name_span)])?;
            }
            StmtKind::Lock { items, when, awaited, body } => {
                if *awaited && !self.is_async {
                    return Err(Diag::error(stmt_span, "`await lock` is written in an async function")
                        .hint("here `lock ... when` waits without `await`: its thread sleeps until the condition holds"));
                }
                if *awaited && when.is_none() {
                    return Err(Diag::error(stmt_span, "a `lock` without `when` does not wait, so there is nothing to await")
                        .hint("drop the `await`: an ordinary `lock` is written as it is in an async function"));
                }
                if when.is_some() && !*awaited && self.is_async {
                    return Err(Diag::error(stmt_span, "`lock ... when` waits, and an async function awaits what waits: its task waits, and its thread goes on")
                        .hint("write `await lock ... when ...`"));
                }
                let mut binds = Vec::new();
                let mut loans = Vec::new();
                for position in 0..items.len() {
                    let item = &mut items[position];
                    // `lock Settings as s`: a `static locked` value.
                    if let ExprKind::Var(written) = &item.value.kind {
                        if self.find(written).is_none() {
                            if let Some(full) = self.static_named(&written.clone(), item.value.span)?.filter(|full| self.names.locked_statics.contains(full)) {
                                let held = self.names.static_types[&full].clone();
                                item.value.kind = ExprKind::Static(full);
                                item.value.ty = Ty::Read(Box::new(Ty::Shared(Box::new(Ty::Locked(Box::new(held))), false)));
                            }
                        }
                    }
                    let found = match &item.value.kind {
                        ExprKind::Static(_) => Some((item.value.ty.clone(), false)),
                        _ => self.place(&mut item.value)?,
                    };
                    let found = match found {
                        Some((ty, _)) => ty,
                        None => self.expr(&mut item.value, None)?,
                    };
                    let value = &item.value;
                    let text = self.src[value.span.start..value.span.end].to_string();
                    let inner = match found.strip_borrow() {
                        Ty::Shared(held, _) if matches!(&**held, Ty::Locked(_)) => {
                            let Ty::Locked(inner) = &**held else { unreachable!("matched above") };
                            (**inner).clone()
                        }
                        Ty::Nullable(inner) if matches!(&**inner, Ty::Shared(..)) => {
                            return Err(Diag::error(value.span, format!("this is `{found}` and may be `null`; `lock` needs a value"))
                                .hint(format!("test it first: `if {text} is not null {{ ... }}`")));
                        }
                        Ty::Weak(held, _) if matches!(&**held, Ty::Locked(_)) => {
                            return Err(Diag::error(value.span, format!("this is `{found}`: what it looks at may be gone"))
                                .hint(format!("ask for an owner first: `var owner = share {text}`, which is `null` if it is gone")));
                        }
                        Ty::Shared(_, true) => {
                            return Err(Diag::error(value.span, format!("`lock` is for a `shared locked` value, and this is `{found}`, which one task changes"))
                                .hint(format!("`ref {text} as name {{ ... }}` changes it for a block")));
                        }
                        other => {
                            return Err(Diag::error(value.span, format!("`lock` is for a `shared locked` value, which several tasks change, and this is `{other}`"))
                                .note("a value that several tasks change is made with `new shared locked`"));
                        }
                    };
                    for earlier in &items[..position] {
                        let item = &items[position];
                        if earlier.name == item.name {
                            return Err(Diag::error(item.name_span, format!("`{}` names two values of this `lock`", item.name)).related(earlier.name_span, "the first one"));
                        }
                        if steady(&earlier.value) && steady(&item.value) && self.src[earlier.value.span.start..earlier.value.span.end] == text {
                            return Err(Diag::error(item.value.span, format!("`{text}` is locked twice in one `lock`")).related(earlier.value.span, "the first time"));
                        }
                    }
                    let item = &items[position];
                    if item.value.is_place() {
                        loans.extend(self.place_loans(&item.value, false, item.value.span));
                    }
                    binds.push((item.name.clone(), Ty::Ref(Box::new(inner)), item.name_span));
                }
                // `when`: computed under the locks, each name lent to read.
                if let Some(when) = when {
                    self.scopes.push(Scope::default());
                    self.borrows.binds = loans.clone();
                    let reads = binds.iter().map(|(name, ty, span): &(String, Ty, Span)| (name.clone(), Ty::Read(Box::new(ty.strip_borrow().clone())), *span)).collect();
                    let checked = self.declare_binds(reads).and_then(|()| self.typed(when, &Ty::Bool));
                    // The names are lent to read here, whatever the parameters are.
                    checked.map_err(|mut diag| {
                        if diag.hint.as_deref() == Some("take the value as `ref` instead of `read`") {
                            diag.hint = Some("the condition only looks at what is locked; change it in the block".to_string());
                        }
                        diag
                    })?;
                    self.pop_scope(true)?;
                }
                // Each name borrows what its value holds, so the values stay where they are.
                self.borrows.binds = loans;
                self.borrows.opening = true;
                self.block_with(body, &[], binds)?;
            }
            StmtKind::Unsafe(body) => {
                self.enter_unsafe(stmt_span)?;
                let result = self.block(body, &[]);
                self.leave_unsafe(result)?;
            }
            StmtKind::Return(value) => self.returned(value.as_mut(), stmt_span)?,
            // A const of the block: seen from here to the end of the block. Its value was found
            // with the others.
            StmtKind::Const { name, name_span, full } => {
                if let Some(var) = self.var_named(name) {
                    return Err(Diag::error(*name_span, format!("`{name}` is already a local variable here, and a const cannot take its name"))
                        .related(var.at, "it is declared here"));
                }
                self.not_a_const(name, *name_span)?;
                self.scopes.last_mut().expect("an open scope").consts.push((name.clone(), full.clone()));
            }
            // A static of the block: seen from here to the end of the block.
            StmtKind::Static { name, name_span, full } => {
                if let Some(var) = self.var_named(name) {
                    return Err(Diag::error(*name_span, format!("`{name}` is already a local variable here, and a static cannot take its name"))
                        .related(var.at, "it is declared here"));
                }
                self.not_a_const(name, *name_span)?;
                self.scopes.last_mut().expect("an open scope").statics.push((name.clone(), full.clone()));
            }
            StmtKind::Break(label) => self.leave_round(label.as_ref(), true, stmt_span)?,
            StmtKind::Continue(label) => self.leave_round(label.as_ref(), false, stmt_span)?,
            StmtKind::Expr(expr) => {
                self.unused(expr)?;
            }
        }
        Ok(())
    }

    /// `return` or `return value`, as a statement or on the right side of `??`.
    fn returned(&mut self, value: Option<&mut Expr>, span: Span) -> Res<()> {
        // An exit with an error is let go, as an exit through `try` is: the error the caller
        // gets is the one that counts, and the writer's buffer is written as it ends.
        let error = value.as_ref().is_some_and(|value| matches!(&value.kind, ExprKind::Call { name, .. } if name == "Error"));
        self.returned_value(value, span)?;
        if !error {
            self.unclosed(0, Some((span, "return")))?;
        }
        Ok(())
    }

    fn returned_value(&mut self, value: Option<&mut Expr>, span: Span) -> Res<()> {
        match value {
            Some(value) if self.ret == Ty::Void => {
                Err(Diag::error(value.span, format!("`{}` is void and cannot return a value", self.func_name)))
            }
            Some(value) => {
                let ret = self.ret.clone();
                self.typed(value, &ret)?;
                match self.holds(&ret) {
                    true => self.returned_borrow(value),
                    false => Ok(()),
                }
            }
            None if self.ret != Ty::Void => {
                let diag = Diag::error(span, format!("`{}` must return a `{}` value", self.func_name, self.ret));
                Err(match &self.ret {
                    Ty::Result(ok, _) if **ok == Ty::Void => diag.hint("success is written too: `return Ok()`"),
                    _ => diag,
                })
            }
            None => Ok(()),
        }
    }

    /// An expression whose value is not used: a statement, or an arm of a `match` statement.
    /// The start of an `unsafe` block or of `unsafe` in front of a value, at `span`.
    fn enter_unsafe(&mut self, span: Span) -> Res<()> {
        let word = Span::new(span.start, span.start + "unsafe".len());
        if let Some((outer, _)) = self.unsafe_at {
            return Err(Diag::error(word, "this is already inside `unsafe`").related(outer, "the `unsafe` around it").hint("leave the inner `unsafe` out"));
        }
        self.unsafe_at = Some((word, false));
        Ok(())
    }

    /// The end of the `unsafe` that `enter_unsafe` started: something in it must have needed it.
    fn leave_unsafe<T>(&mut self, result: Res<T>) -> Res<T> {
        let (word, used) = self.unsafe_at.take().expect("an `unsafe` was entered");
        let value = result?;
        if !used {
            return Err(Diag::error(word, "nothing here needs `unsafe`")
                .note("`unsafe` is for what the checker cannot follow: an item of raw memory (`p[i]`), `p + n`, a cast between raw pointers or from a number to one, `Memory.free`, `Memory.copy`, `Memory.view`, `Memory.addressOf`, a call of a C function, a variable of C and `string.fromCString`")
                .hint("leave `unsafe` out"));
        }
        Ok(value)
    }

    /// What is checked at `span` does what the checker cannot follow (`what` says it), so it
    /// is written inside `unsafe`.
    fn needs_unsafe(&mut self, span: Span, what: &str) -> Res<()> {
        match &mut self.unsafe_at {
            Some((_, used)) => {
                *used = true;
                Ok(())
            }
            None => {
                let text = &self.src[span.start..span.end];
                let diag = Diag::error(span, format!("{what}, so it is written inside `unsafe`"));
                Err(match self.writing {
                    true => diag.hint(format!("a write is a statement: `unsafe {{ {text} = ... }}`")),
                    false => diag.hint(format!("`unsafe {text}`, or `unsafe {{ ... }}` around several statements")),
                })
            }
        }
    }

    fn unused(&mut self, expr: &mut Expr) -> Res<()> {
        let span = expr.span;
        if let ExprKind::Unsafe(inner) = &mut expr.kind {
            self.enter_unsafe(span)?;
            let result = self.unused(inner);
            self.leave_unsafe(result)?;
            unwrap_unsafe(expr);
            return Ok(());
        }
        match &mut expr.kind {
            ExprKind::Match { subject, arms, borrowed } => {
                self.match_expr(subject, arms, borrowed, None, true)?;
            }
            ExprKind::Call { .. } | ExprKind::Method { .. } | ExprKind::Try { .. } | ExprKind::Await(_) => {
                let ty = self.expr(expr, None)?;
                if task_list(&ty) {
                    let text = &self.src[span.start..span.end];
                    return Err(Diag::error(span, "the tasks of this list are not waited for")
                        .note("every task is waited for: its error is not lost, and no work is cut off when the program ends")
                        .hint(when_all(&ty, text, self.is_async)));
                }
                if ty == Ty::Group {
                    return Err(Diag::error(span, "this group of tasks is made and then not kept")
                        .hint(format!("keep it in a variable: `var group = {}`", &self.src[span.start..span.end])));
                }
                if matches!(ty, Ty::Task(_)) {
                    let text = &self.src[span.start..span.end];
                    let diag = Diag::error(span, "this task is started and then not waited for")
                        .note("every task is waited for: its error is not lost, and no work is cut off when the program ends");
                    return Err(match self.is_async {
                        true => diag.hint(format!("`await {text}`, or keep it and await it later: `var task = {text}`, then `await task`")),
                        false => diag.hint(format!("keep it and wait for it later: `var task = {text}`, then `task.wait()`")),
                    });
                }
                if matches!(expr.kind, ExprKind::Await(_)) && !matches!(ty, Ty::Void | Ty::Result(..)) {
                    let text = &self.src[span.start..span.end];
                    return Err(Diag::error(span, format!("the `{ty}` that `{text}` gives is not used"))
                        .hint(format!("keep it: `var value = {text}`; to drop it on purpose, `_ = {text}`"))
                        .fix(Span::new(span.start, span.start), "_ = "));
                }
                // `line.trim()` alone changes nothing: a built-in method that only gives a value
                // gives it to nobody.
                if let ExprKind::Method { recv, name, callee, .. } = &expr.kind {
                    // What a task gives is dropped only on purpose (an error is left to the
                    // check of a `Result` below).
                    if *callee == Callee::Wait && !matches!(ty, Ty::Void | Ty::Result(..)) {
                        let text = &self.src[span.start..span.end];
                        let task = &self.src[recv.span.start..recv.span.end];
                        return Err(Diag::error(span, format!("the `{ty}` that `{task}` gives is not used"))
                            .hint(format!("keep it: `var value = {text}`; to drop it on purpose, `_ = {text}`"))
                            .fix(Span::new(span.start, span.start), "_ = "));
                    }
                    if ty != Ty::Void && gives_only(name, callee) {
                        return Err(self.not_used(recv, name, &ty, span));
                    }
                }
                if matches!(ty, Ty::Result(..)) {
                    let text = &self.src[span.start..span.end];
                    return Err(Diag::error(span, "this `Result` is not used, so an error would be lost silently")
                        .hint(format!("pass the error on with `try {text}`, or drop it on purpose with `_ = {text}`")));
                }
                // Every call gives what it gives to be used: `Path.join(a, b)`, `try
                // command.status()`, `xs.removeLast()` or a function of your own alone is a value
                // nobody reads, and `_ =` drops one on purpose.
                if !matches!(ty, Ty::Void | Ty::Never) {
                    let text = &self.src[span.start..span.end];
                    let name = match &peel_try(expr).kind {
                        ExprKind::Call { name, .. } | ExprKind::Method { name, .. } => short_name(name),
                        _ => text,
                    };
                    return Err(Diag::error(span, format!("the `{ty}` that `{name}` gives is not used"))
                        .hint(format!("keep it: `var value = {text}`; to drop it on purpose, `_ = {text}`"))
                        .fix(Span::new(span.start, span.start), "_ = "));
                }
            }
            _ => {
                return Err(Diag::error(span, "the value of this expression is not used")
                    .hint("only a call, an assignment or a declaration can be a statement"));
            }
        }
        Ok(())
    }

    /// The error for a call of a built-in method whose value is not used: `line.trim()`,
    /// `x.abs()`, `xs.contains(3)` written alone.
    fn not_used(&self, recv: &Expr, name: &str, ty: &Ty, span: Span) -> Diag {
        let call = &self.src[span.start..span.end];
        let text = &self.src[recv.span.start..recv.span.end];
        let given = ty.strip_borrow();
        let what = match given {
            Ty::Str if name == "clone" => "the copy that `clone` makes".to_string(),
            // `trim` gives a look at the same text, not a new one.
            Ty::Str if matches!(ty, Ty::Read(_)) => format!("the text that `{name}` gives"),
            Ty::Str => format!("the new text that `{name}` gives"),
            _ if given == recv.ty.strip_borrow() => format!("the new value that `{name}` gives"),
            _ => format!("the value that `{name}` gives"),
        };
        let place = matches!(recv.kind, ExprKind::Var(_) | ExprKind::Field { .. }) && !matches!(recv.kind, ExprKind::Var(ref var) if var == "string");
        // `x.abs()` and `line.replaceAll(...)` are a new `x` and a new `line`; `xs.length()` is not.
        let changed = place && name != "clone" && given == recv.ty.strip_borrow();
        let keep = match given {
            Ty::Str => "text",
            Ty::Token => "token",
            _ => "value",
        };
        match changed {
            // Only a value of its own type that is not a borrow can be assigned back to it.
            true if !matches!(ty, Ty::Read(_)) => Diag::error(span, format!("{what} is not used: it leaves `{text}` as it was"))
                .hint(format!("to change `{text}`, assign the new {keep} to it: `{text} = {call}`")),
            true => Diag::error(span, format!("{what} is not used: it leaves `{text}` as it was"))
                .hint(format!("keep it in a variable: `var {keep} = {call}`")),
            false => Diag::error(span, format!("{what} is not used")).hint(format!("keep it in a variable: `var {keep} = {call}`")),
        }
    }

    /// The condition of an `if` or `while`, checked in the scope that its block shares: the
    /// names of `x is Circle(r)` and the narrowing of `x is not null`, alone or on the left of
    /// `&&`, hold in the block. Gives the names seen where the condition holds and those seen
    /// where it fails: `x is not Circle(r)` alone, or on the left of `||`. Those of a chain of
    /// `||` are checked in a scope of its own, which the block does not see. `fails`: where
    /// the code after the statement sees the names of `x is not Circle(r)` alone.
    fn condition(&mut self, cond: &mut Expr, fails: NamesSeen) -> Res<(Named, Named)> {
        if matches!(cond.kind, ExprKind::Binary(BinOp::Or, ..)) {
            self.scopes.push(Scope::default());
            let named = self.link(cond, BinOp::Or)?;
            self.pop_scope(true)?;
            return Ok((Vec::new(), named));
        }
        if let Some(flip) = is_test(cond).filter(|_| tests_not(cond)) {
            let named = self.named_test(cond, flip, fails, false)?;
            return Ok((Vec::new(), named));
        }
        Ok((self.link(cond, BinOp::And)?, Vec::new()))
    }

    /// One part of a chain of `&&` (or of `||`), checked in the scope of the chain. On the
    /// left of `&&`, an `is` names values and what holds when the part holds narrows for what
    /// comes after it (`x is not null`, `!(x is null)`); on the left of `||`, what holds when the
    /// part fails does (`x is null`, `x is not Circle(r)`), since the right side runs only when
    /// it did not hold. Gives the names it declared.
    fn link(&mut self, part: &mut Expr, op: BinOp) -> Res<Named> {
        if let ExprKind::Binary(inner, left, right) = &mut part.kind {
            if *inner == op {
                let mut named = self.link(left, op)?;
                named.extend(self.link(right, op)?);
                part.ty = Ty::Bool;
                return Ok(named);
            }
        }
        if let Some(flip) = is_test(part) {
            let seen = if op == BinOp::And { NamesSeen::Holds } else { NamesSeen::Fails };
            return self.named_test(part, flip, seen, true);
        }
        self.typed(part, &Ty::Bool)?;
        let (holds, fails) = null_facts(part);
        for name in if op == BinOp::And { holds } else { fails } {
            self.narrow(&name);
        }
        Ok(Vec::new())
    }

    /// `x is Circle(r)`, `x is not Circle(r)` or `!(x is Circle(r))` (`flip`), seen as `seen`
    /// says; with `declare`, its names are declared here.
    fn named_test(&mut self, part: &mut Expr, flip: bool, seen: NamesSeen, declare: bool) -> Res<Named> {
        part.ty = Ty::Bool;
        let test = match flip {
            true => match &mut part.kind {
                ExprKind::Unary(UnOp::Not, inner) => &mut **inner,
                _ => unreachable!("`flip` is an `is` under `!`"),
            },
            false => part,
        };
        let binds = self.is_variant(test, seen, flip)?;
        test.ty = Ty::Bool;
        let loans = std::mem::take(&mut self.borrows.binds);
        if binds.is_empty() {
            return Ok(Vec::new());
        }
        let fails = matches!(seen, NamesSeen::Fails | NamesSeen::StaysIn | NamesSeen::Break(_));
        self.is_names.extend(binds.iter().map(|(name, _, span)| (name.clone(), *span, fails)));
        if declare {
            self.declare_group(binds.clone(), loans.clone())?;
        }
        Ok(vec![(binds, loans)])
    }

    /// Declares the names of an `is` in the current scope; those that are borrows hold what
    /// its value was lent from.
    fn declare_group(&mut self, binds: Binds, loans: Vec<Loan>) -> Res<()> {
        self.borrows.binds = loans;
        self.declare_binds(binds)
    }

    /// The names of an `is` that the code after an `if` or `while` sees, declared in the
    /// scope around it. The value with no name that an `if` looks at stays until that scope
    /// ends; one that a `while` looks at is made again in every round, so a borrow of it is
    /// refused.
    fn names_after(&mut self, named: Named, keyword: &str) -> Res<Vec<String>> {
        let mut names = Vec::new();
        for (binds, loans) in named {
            for (name, ty, span) in &binds {
                // After an `if`, the value stays until the block around it ends; a `while` makes
                // it again in every round.
                if self.holds(ty) && keyword == "while" {
                    if let Some(loan) = loans.iter().find(|loan| loan.root == UNNAMED) {
                        let text = &self.src[loan.span.start..loan.span.end];
                        return Err(Diag::error(*span, format!("`{name}` borrows from `{text}`, a value with no name that the `while` makes again in every round, so the code after it cannot look at it"))
                            .hint(format!("give the value a name before the loop: `var value = {text}`, and change it in the loop")));
                    }
                }
                if let Some(var) = self.scopes.last().and_then(|scope| scope.vars.iter().find(|var| var.name == *name)) {
                    return Err(Diag::error(*span, format!("`{name}` is already declared in this scope"))
                        .related(var.at, "it is declared here")
                        .note(format!("the names of this `is` are seen after the `{keyword}`, in the same scope"))
                        .hint("give the name in the pattern another name"));
                }
                names.push(name.clone());
            }
            self.declare_group(binds, loans)?;
        }
        Ok(names)
    }

    /// `break` (`leaves`) or `continue`, with the name of the loop if one is written.
    fn leave_round(&mut self, label: Option<&Label>, leaves: bool, stmt_span: Span) -> Res<()> {
        let word = if leaves { "break" } else { "continue" };
        if self.loop_cx.is_empty() {
            return Err(Diag::error(stmt_span, format!("`{word}` can only be written inside a loop")));
        }
        let index = match label {
            None => self.loop_cx.len() - 1,
            Some(label) => match self.loop_cx.iter().rposition(|cx| cx.label.as_ref().is_some_and(|named| named.name == label.name)) {
                Some(index) => index,
                None => {
                    let named: Vec<String> = self.loop_cx.iter().rev().filter_map(|cx| cx.label.as_ref().map(|named| format!("`{}`", named.name))).collect();
                    let diag = Diag::error(label.span, format!("no loop around this `{word}` is named `{}`", label.name));
                    return Err(match named.is_empty() {
                        true => diag.hint(format!("name the loop it {}: `{}: for ...`", if leaves { "leaves" } else { "goes on with" }, label.name)),
                        false => diag.note(format!("the loops around it that have a name: {}", named.join(", "))),
                    });
                }
            },
        };
        let depth = self.loop_cx[index].depth;
        self.unclosed(depth, Some((stmt_span, word)))?;
        // The blocks between here and the loop end here, as they would at their ends: a
        // variable outside that borrows an owner of theirs holds a borrow of nothing when the
        // loop goes on or is left (choice 1592). The state saved below still names the borrow,
        // and is joined with the rest after the loop or at the next round.
        self.owners_gone(depth);
        let moves = self.moves();
        let cx = &mut self.loop_cx[index];
        cx.used |= label.is_some();
        if leaves {
            cx.breaks.push(moves);
        } else {
            cx.continues.push(moves);
        }
        Ok(())
    }

    /// A value moved inside a loop is gone when the next iteration starts: the end of the
    /// body and every `continue` of the loop start one. `label`: the name of the loop.
    /// `head`: where the touches of the loop's own head begin, a `while`'s condition, which runs
    /// again before every round (choice 1592).
    fn loop_body(&mut self, body: &mut Block, narrow: &[String], binds: Binds, label: Option<&Label>, head: Option<usize>) -> Res<LoopCx> {
        if let Some(label) = label {
            if let Some(outer) = self.loop_cx.iter().find_map(|cx| cx.label.as_ref().filter(|named| named.name == label.name)) {
                return Err(Diag::error(label.span, format!("a loop around this one is already named `{}`", label.name))
                    .related(outer.span, "it is named here")
                    .hint("give the inner loop a name of its own"));
            }
        }
        let before = self.moves();
        let mark = head.unwrap_or(self.borrows.touched.len());
        self.loops += 1;
        self.loop_cx.push(LoopCx { depth: self.scopes.len(), label: label.cloned(), used: false, breaks: Vec::new(), continues: Vec::new(), before: Vec::new() });
        let result = self.block_with(body, narrow, binds);
        let mut cx = self.loop_cx.pop().expect("pushed above");
        self.loops -= 1;
        result?;
        if let Some(label) = cx.label.as_ref().filter(|_| !cx.used) {
            return Err(Diag::error(label.span, format!("no `break {0}` or `continue {0}` uses the name of this loop", label.name))
                .hint(format!("a loop needs a name only for a loop inside it to leave it: remove `{}:`", label.name)));
        }
        let mut starts = std::mem::take(&mut cx.continues);
        if !leaves(body, true) {
            starts.push(self.moves());
        }
        for start in &starts {
            self.moved_for_next_round(&before, start)?;
            self.lent_for_next_round(&before, start, mark);
        }
        cx.continues = starts;
        cx.before = before;
        Ok(cx)
    }

    /// The error of a value that the loop moved and that the round starting with `start`
    /// would find gone, if there is one. `before`: what was moved when the loop started.
    fn moved_for_next_round(&self, before: &Moves, start: &Moves) -> Res<()> {
        for (scope, (saved, now)) in self.scopes.iter().zip(before.iter().zip(start)) {
            for ((var, was), moved) in scope.vars.iter().zip(saved).zip(now) {
                let new_part = moved.parts.iter().find(|(path, _)| !was.parts.iter().any(|(old, _)| old == path));
                let (what, at) = match (moved.whole, was.whole, new_part) {
                    (Some(at), None, _) => (var.name.clone(), at),
                    (_, _, Some((path, at))) => (format!("{}.{path}", var.name), *at),
                    _ => continue,
                };
                return Err(Diag::error(at, format!("`{what}` is moved inside a loop, so the next iteration cannot use it")));
            }
        }
        Ok(())
    }

    /// `xs.clear()` on a local collection that holds borrows: with no items left it borrows
    /// nothing, so its source is free again (1733), and a loop that clears it before it
    /// touches the source does not clash with the borrows of the round before; one that clears
    /// it after, before the list is used again, does not either (1748, in `borrow_conflicts`).
    /// Not through a parameter, whose items are the caller's.
    fn release(&mut self, recv: &Expr) {
        let ExprKind::Var(name) = &recv.kind else { return };
        let at = self.point(recv.span.start);
        let mark = self.borrows.touched.len();
        let Some(var) = self.find(name) else { return };
        if var.param || var.declared.is_borrow() {
            return;
        }
        var.loans.clear();
        let id = var.id;
        self.borrows.released.push((id, mark, at));
    }

    /// A variable from outside the loop that a round leaves with a new borrow holds it when
    /// the next round starts: what the body touches of that place may clash with it, as a
    /// touch in the same round would. `mark`: where the touches of the body start.
    fn lent_for_next_round(&mut self, before: &Moves, start: &Moves, mark: usize) {
        let mut found = Vec::new();
        for (scope, (saved, now)) in self.scopes.iter().zip(before.iter().zip(start)) {
            for ((var, was), then) in scope.vars.iter().zip(saved).zip(now) {
                for loan in then.loans.iter().filter(|loan| !loan.held && !was.loans.iter().any(|old| same_loan(old, loan))) {
                    for (index, (root, path, access, span, at)) in self.borrows.touched.iter().enumerate().skip(mark) {
                        let overlaps = path.is_empty() || loan.path.is_empty() || within(path, &loan.path) || within(&loan.path, path);
                        if *root != loan.root || *root == var.id || !overlaps || (!loan.mutable && *access == Access::Read) {
                            continue;
                        }
                        // Cleared earlier in every round that comes to the touch: what the round
                        // before borrowed is let go of by then.
                        let cleared = self.borrows.released.iter().any(|(holder, before, cleared_at)| {
                            *holder == var.id
                                && (mark..=index).contains(before)
                                && cleared_at.branches.iter().all(|branch| at.branches.contains(branch))
                                && cleared_at.loops.iter().all(|inner| at.loops.contains(inner))
                        });
                        if cleared {
                            continue;
                        }
                        found.push(Clash {
                            holder: var.id,
                            holder_name: var.name.clone(),
                            owner_name: String::new(),
                            closed: false,
                            copy_as: None,
                            holder_loops: var.decl_loops.clone(),
                            clears: clears(var),
                            loan: loan.clone(),
                            access: *access,
                            span: *span,
                            at: at.clone(),
                        });
                    }
                }
            }
        }
        self.borrows.clashes.extend(found);
    }

    /// What is moved after a loop: what was moved where a `break` left it, and, when the loop
    /// also ends by its head (`head`), what was moved before its first round and where each
    /// round ended, since it may run no round or end after any.
    fn after_loop(&mut self, cx: LoopCx, head: bool) {
        let mut exits = cx.breaks;
        if head {
            exits.push(cx.before);
            exits.extend(cx.continues);
        }
        let Some((first, rest)) = exits.split_first() else { return };
        self.set_moves(first);
        for other in rest {
            self.merge_moves(other);
        }
    }

    /// What was known about a variable before a loop does not hold inside it if the loop
    /// assigns the variable or empties it with `take`.
    fn forget_narrowing_of_assigned(&mut self, body: &Block, cond: Option<&Expr>) {
        let mut names = Vec::new();
        if let Some(cond) = cond {
            taken_in(cond, &mut names);
        }
        changed_in(body, &mut names);
        for scope in &mut self.scopes {
            for var in &mut scope.vars {
                if names.contains(&var.name.as_str()) {
                    var.current = var.declared.clone();
                    var.assigned += 1;
                }
            }
        }
    }

    // ---- expressions ----

    /// Checks `expr` and makes it fit `expected`, inserting implicit conversions.
    fn typed(&mut self, expr: &mut Expr, expected: &Ty) -> Res<()> {
        let found = self.expr(expr, Some(expected))?;
        self.coerce(expr, found, expected)
    }

    /// The right side of `??`. When it always leaves (`?? return list`, `?? panic(...)`), the
    /// code after it runs only if the right side did not, so what it moved is still there.
    fn fallback(&mut self, right: &mut Expr, expected: &Ty) -> Res<()> {
        let before = self.moves();
        self.typed(right, expected)?;
        if right.ty == Ty::Never {
            self.set_moves(&before);
        }
        Ok(())
    }

    /// How a `T?` that is not narrowed is tested before it is used: a variable in place; anything
    /// else in a variable first: a copy if `T` is `Copyable`, one more owner of a `shared` or
    /// `weak` value (`share`), or else lent with `read`, which leaves the value where it is (`var
    /// nick = read person.nick`); a field of a `shared checked` value is lent within a block that
    /// opens it, since a borrow of it ends with its line.
    fn test_first(&self, recv: &Expr, ty: &Ty) -> String {
        let text = self.src[recv.span.start..recv.span.end].trim();
        let owned = matches!(peel(ty), Ty::Nullable(inner) if matches!(**inner, Ty::Shared(..) | Ty::Weak(..)));
        match &recv.kind {
            ExprKind::Var(var) => format!("test it first: `if {var} is not null {{ ... }}`"),
            ExprKind::Field { name, .. } if ty.is_copyable(self.copyable) => format!("a field is not narrowed: copy it and test the copy: `var {name} = {text}`, then `if {name} is not null {{ ... }}`"),
            ExprKind::Field { name, .. } if owned => format!("a field is not narrowed: take one more owner of it and test that: `var {name} = share {text}`, then `if {name} is not null {{ ... }}`"),
            ExprKind::Field { recv: holder, name, .. } if through_checked(holder) => {
                let holder = self.src[holder.span.start..holder.span.end].trim();
                format!("a field is not narrowed: open `{holder}` for a block and lend the field there: `read {holder} as value {{ var {name} = read value.{name} ... }}`, then `if {name} is not null {{ ... }}`")
            }
            ExprKind::Field { name, .. } => format!("a field is not narrowed: lend it to a variable and test that: `var {name} = read {text}`, then `if {name} is not null {{ ... }}`"),
            ExprKind::Index { .. } if !ty.is_copyable(self.copyable) => format!("an item is not narrowed: lend it to a variable and test that: `var item = read {text}`, then `if item is not null {{ ... }}`"),
            _ => format!("keep it in a variable and test that: `var value = {text}`, then `if value is not null {{ ... }}`"),
        }
    }

    fn coerce(&mut self, expr: &mut Expr, found: Ty, expected: &Ty) -> Res<()> {
        if matches!((&found, expected), (Ty::Ref(_), Ty::Ref(_))) {
            self.not_through_read(expr)?;
        }
        if found == *expected || found == Ty::Never {
            return Ok(());
        }
        let text = &self.src[expr.span.start..expr.span.end];
        let mismatch = |hint: Option<String>| {
            let diag = Diag::error(expr.span, format!("expected `{expected}`, found `{found}`"));
            Err(match hint {
                Some(hint) => diag.hint(hint),
                None => diag,
            })
        };
        match (&found, expected) {
            // An atomic value is read and written by its methods, each one step.
            (Ty::Atomic(_), _) if !matches!(expected.strip_borrow(), Ty::Atomic(_)) => mismatch(Some(format!("read it: `{text}.load()`"))),
            (Ty::Read(inner) | Ty::Ref(inner), _) if matches!(**inner, Ty::Atomic(_)) && !matches!(expected.strip_borrow(), Ty::Atomic(_)) => {
                mismatch(Some(format!("read it: `{text}.load()`")))
            }
            (_, Ty::Atomic(inner)) if found.strip_borrow() == &**inner || matches!((found.strip_borrow(), &**inner), (Ty::Int(_), Ty::Int(_) | Ty::Float(_)) | (Ty::Null, Ty::Raw(_))) => {
                mismatch(Some(format!("make it atomic: `atomic {text}`")))
            }
            (Ty::Int(from), Ty::Int(to)) if from.widens_to(*to) => {
                wrap(expr, |inner| ExprKind::Widen(Ty::Int(*to), inner));
                expr.ty = Ty::Int(*to);
                Ok(())
            }
            (Ty::Int(_), Ty::Int(to)) => {
                let cast = cast_of(to.name(), expr, text);
                mismatch(Some(format!("convert with `({})`; it panics if the value does not fit", to.name()))).map_err(|diag| diag.fix(expr.span, cast))
            }
            (Ty::Int(from), Ty::Float(to)) if to.holds(*from) => {
                wrap(expr, |inner| ExprKind::Widen(expected.clone(), inner));
                expr.ty = expected.clone();
                Ok(())
            }
            (Ty::Float(FloatTy::Float), Ty::Float(FloatTy::Double)) => {
                wrap(expr, |inner| ExprKind::Widen(expected.clone(), inner));
                expr.ty = expected.clone();
                Ok(())
            }
            (Ty::Int(from), Ty::Float(to)) => mismatch(Some(format!(
                "a `{}` holds whole numbers exactly up to 2^{}, and `{}` goes past that; convert with `({})`, which rounds",
                to.name(),
                to.digits(),
                from.name(),
                to.name()
            ))),
            (Ty::Float(_), Ty::Float(to)) => mismatch(Some(format!("convert with `({})`, which rounds", to.name()))),
            (Ty::Float(_), Ty::Int(to)) => {
                mismatch(Some(format!("convert with `({})`; it drops the fraction, and panics if the value does not fit", to.name())))
            }
            // A `char` is not a number, and a number is not a `char`.
            (Ty::Char, Ty::Int(to)) => mismatch(Some(format!("the code of the character: `({}){text}`", to.name()))),
            (Ty::Int(_), Ty::Char) => {
                mismatch(Some(format!("`(char){text}` panics if no character has this code; `{text}.tryToChar()` gives `null` instead")))
            }
            (Ty::Char, Ty::Str) => mismatch(Some(format!("a text of one character: `{text}.toString()`"))),
            (Ty::Str, Ty::Char) if matches!(&expr.kind, ExprKind::Str(value) if value.chars().count() == 1) => {
                let ExprKind::Str(value) = &expr.kind else { unreachable!("tested above") };
                let written: String = value.chars().map(|found| match found {
                    '\n' => "\\n".to_string(),
                    '\t' => "\\t".to_string(),
                    '\r' => "\\r".to_string(),
                    '\0' => "\\0".to_string(),
                    '\'' => "\\'".to_string(),
                    '\\' => "\\\\".to_string(),
                    other => other.to_string(),
                }).collect();
                mismatch(Some(format!("a character is written in single quotes: `'{written}'`")))
            }
            // The habit from C: `if flags & 2 {`.
            (Ty::Int(_) | Ty::Float(_), Ty::Bool) => {
                let text = if matches!(expr.kind, ExprKind::Coalesce { .. }) { format!("({text})") } else { text.to_string() };
                mismatch(Some(format!("a number is not a condition; compare it: `{text} != 0`")))
            }
            (Ty::Null, _) if matches!(expected.strip_borrow(), Ty::Nullable(_) | Ty::Raw(_) | Ty::CFunc(..)) => Ok(()),
            (Ty::Null, _) => Err(Diag::error(expr.span, format!("`{expected}` cannot be `null`"))
                .hint(format!("a value that may be missing has the type `{expected}?`"))),
            (Ty::Void, _) => Err(Diag::error(expr.span, "this call does not return a value, and one is needed here")),
            // A reader of a connection is lent where a reader is lent.
            (Ty::Read(from) | Ty::Ref(from), Ty::Read(to)) | (Ty::Ref(from), Ty::Ref(to)) if **from == Ty::Reader(true) && **to == Ty::Reader(false) => {
                expr.ty = expected.clone();
                Ok(())
            }
            (Ty::Reader(true), Ty::Reader(false)) => Err(Diag::error(expr.span, "this `StreamReader` reads a connection it borrows, and a `StreamReader` that is not lent reads a file of its own")
                .note("it lives no longer than the connection: it is kept in a variable, and lent with `ref` to a function that reads it")),
            // A borrow of a list or an array where a slice goes: a window onto all of it.
            (Ty::Read(from) | Ty::Ref(from), Ty::Read(to)) | (Ty::Ref(from), Ty::Ref(to)) if as_slice(from, to) => {
                wrap(expr, ExprKind::AsSlice);
                expr.ty = expected.clone();
                Ok(())
            }
            (Ty::List(_) | Ty::Array(..), Ty::Read(to) | Ty::Ref(to)) if as_slice(&found, to) => {
                let word = if matches!(expected, Ty::Ref(_)) { "ref" } else { "read" };
                mismatch(Some(format!("lend it with `{word}`: `{word} {text}`")))
            }
            (Ty::Str, Ty::Read(to)) if **to == Ty::Slice(Box::new(Ty::Int(IntTy::Byte))) => {
                mismatch(Some(format!("the bytes of a text: `{text}.asBytes()`")))
            }
            (Ty::Read(from), Ty::Read(to)) if **from == Ty::Str && **to == Ty::Slice(Box::new(Ty::Int(IntTy::Byte))) => {
                mismatch(Some(format!("the bytes of a text: `{text}.asBytes()`")))
            }
            // The two clocks are two types, so that one is not measured against the other.
            (Ty::Struct(a), Ty::Struct(b)) if [a.as_str(), b.as_str()].iter().all(|name| matches!(*name, "std.Instant" | "std.DateTime")) => {
                Err(Diag::error(expr.span, format!("expected `{expected}`, found `{found}`"))
                    .note("an `Instant` is a reading of the monotonic clock and a `DateTime` one of the clock of the system, which can be set and go back; the time between two readings is taken on one clock"))
            }
            // A moment in UTC and one seen at an offset are two types, so that a local time is not
            // taken for UTC.
            (Ty::Struct(a), Ty::Struct(b)) if a == "std.DateTimeOffset" && b == "std.DateTime" => {
                mismatch(Some(format!("the moment in UTC: `{text}.toUtc()`")))
            }
            (Ty::Struct(a), Ty::Struct(b)) if a == "std.DateTime" && b == "std.DateTimeOffset" => {
                mismatch(Some(format!("the moment at the offset of the system's time zone: `{text}.toLocal()`, or at one of your own: `{text}.withOffset(offset)`")))
            }
            (Ty::Fmt, Ty::Str) => Err(Diag::error(expr.span, "expected `string`; `$\"...\"` does not become a `string` on its own")
                .hint("add `.toString()` at the end; that is where the allocation shows")),
            // A borrow of a `T?` where a `T?` goes (`int? first = xs.get(0)`): a copyable value is
            // copied out, or `null` (choice 1446); any other is only lent.
            (Ty::Read(inner) | Ty::Ref(inner), Ty::Nullable(_)) if **inner == *expected => {
                if !inner.is_copyable(self.copyable) {
                    return mismatch(Some(format!("`{found}` only borrows the value; `{expected}` needs to own it: `{text}?.clone()`")));
                }
                wrap(expr, ExprKind::Load);
                expr.ty = expected.clone();
                Ok(())
            }
            // `T` where `T?` is expected.
            (_, Ty::Nullable(inner)) => self.coerce(expr, found.clone(), inner),
            (Ty::Shared(from, a), Ty::Shared(to, b)) if from == to && a != b => {
                Err(Diag::error(expr.span, format!("expected `{expected}`, found `{found}`"))
                    .note("what a `shared checked` value holds can be changed through any of its owners, and what a `shared` one holds cannot: they are two types"))
            }
            // One more owner of a `shared` value is made with `share`, where it can be seen.
            (Ty::Read(inner) | Ty::Ref(inner), Ty::Shared(..)) if **inner == *expected => {
                mismatch(Some(format!("`{found}` only borrows it; one more owner is made with `share`: `share {text}`")))
            }
            // A borrow of a copyable value stands in for the value.
            (Ty::Read(inner) | Ty::Ref(inner), _) if !expected.is_borrow() && inner.is_copyable(self.copyable) => {
                wrap(expr, ExprKind::Load);
                expr.ty = (**inner).clone();
                self.coerce(expr, (**inner).clone(), expected)
            }
            (Ty::Read(inner), Ty::Str) if **inner == Ty::Str => {
                let copy = format!("{}.clone()", if is_simple(expr) { text.to_string() } else { format!("({text})") });
                mismatch(Some(format!("`{found}` only borrows the text; to own a copy write `{text}.clone()`"))).map_err(|diag| diag.fix(expr.span, copy))
            }
            (Ty::Read(_) | Ty::Ref(_), _) if !expected.is_borrow() => {
                mismatch(Some(format!("`{found}` only borrows the value; `{expected}` needs to own it")))
            }
            // What a `shared locked` value holds is lent only by `lock`.
            (Ty::Read(from) | Ty::Ref(from), Ty::Read(to) | Ty::Ref(to)) if matches!(&**from, Ty::Shared(inner, _) if matches!(&**inner, Ty::Locked(held) if held == to)) => {
                let shared = match &expr.kind {
                    ExprKind::Borrow { expr: inner, .. } => &self.src[inner.span.start..inner.span.end],
                    _ => text,
                };
                let word = if matches!(expected, Ty::Ref(_)) { "ref " } else { "" };
                Err(self.locked_outside(expr.span).hint(format!("lend what it holds inside a `lock` block: `lock {shared} as v {{ ... {word}v ... }}`")))
            }
            // A borrow of a `shared T` stands in for a `read` of the `T` it points to.
            (Ty::Read(from) | Ty::Ref(from), Ty::Read(to)) if matches!(&**from, Ty::Shared(inner, _) if inner == to) => {
                wrap(expr, ExprKind::Unbox);
                expr.ty = expected.clone();
                Ok(())
            }
            // What a `shared checked` value points to is lent to be changed with `ref`.
            // The node has the type it stands in for: `opening` lends a `ref` of it to be changed
            // (choice 1379).
            (Ty::Ref(from), Ty::Ref(to)) if matches!(&**from, Ty::Shared(inner, true) if inner == to) => {
                wrap(expr, ExprKind::Unbox);
                expr.ty = expected.clone();
                Ok(())
            }
            (Ty::Read(from), Ty::Ref(to)) if matches!(&**from, Ty::Shared(inner, true) if inner == to) => {
                let place = match &expr.kind {
                    ExprKind::Borrow { expr: inner, .. } => &self.src[inner.span.start..inner.span.end],
                    _ => text,
                };
                mismatch(Some(format!("lend what it points to with `ref`: `ref {place}`")))
            }
            (Ty::Shared(from, true), Ty::Ref(to)) if from == to => mismatch(Some(format!("lend what it points to with `ref`: `ref {text}`"))),
            (Ty::Read(from) | Ty::Ref(from), Ty::Ref(to)) if matches!(&**from, Ty::Shared(inner, _) if inner == to) => {
                Err(Diag::error(expr.span, format!("a `shared {to}` is only read, so it cannot be lent as `ref {to}`")).note(SHARED_READ_ONLY))
            }
            (Ty::Shared(from, _), Ty::Ref(to)) if from == to => {
                Err(Diag::error(expr.span, format!("a `shared {to}` is only read, so it cannot be lent as `ref {to}`")).note(SHARED_READ_ONLY))
            }
            (Ty::Shared(inner, _), Ty::Read(to)) if inner == to => mismatch(Some(format!("lend what it points to with `read`: `read {text}`"))),
            (Ty::Shared(inner, _), _) if **inner == *expected => {
                mismatch(Some(format!("`{found}` points to a value that others own too; lend it with `read`: `read {text}`")))
            }
            // A borrow of a `T*` stands in for a borrow of the `T` it owns.
            (Ty::Read(from), Ty::Read(to)) | (Ty::Ref(from), Ty::Read(to) | Ty::Ref(to))
                if matches!(&**from, Ty::Ptr(inner) if inner == to) =>
            {
                wrap(expr, ExprKind::Unbox);
                expr.ty = expected.clone();
                Ok(())
            }
            (Ty::Ptr(inner), Ty::Read(to)) if inner == to => mismatch(Some(format!("lend it with `read`: `read {text}`"))),
            (Ty::Ptr(inner), Ty::Ref(to)) if inner == to => mismatch(Some(format!("lend it with `ref`: `ref {text}`"))),
            // A `ref string` where a `read string` goes: a look at the same text.
            (Ty::Ref(from), Ty::Read(to)) if from == to && **from == Ty::Str => {
                wrap(expr, |inner| ExprKind::Borrow { mutable: false, expr: inner });
                expr.ty = expected.clone();
                Ok(())
            }
            (Ty::Ref(from), Ty::Read(to)) if from == to => Ok(()),
            (Ty::Read(from) | Ty::Ref(from), Ty::Read(to)) if **to == Ty::Nullable(from.clone()) => Ok(()),
            // A text constant where a `read string` or a `read string?` goes: lent as it is, as
            // `read "..."` would lend it (choice 1403).
            (Ty::Str, Ty::Read(to)) if matches!(expr.kind, ExprKind::Str(_)) && (**to == Ty::Str || **to == Ty::Nullable(Box::new(Ty::Str))) => {
                wrap(expr, |inner| ExprKind::Borrow { mutable: false, expr: inner });
                expr.ty = Ty::Read(Box::new(Ty::Str));
                Ok(())
            }
            (_, Ty::Read(to)) if found == **to => mismatch(Some(format!("lend it with `read`: `read {text}`"))),
            (_, Ty::Ref(to)) if found == **to => mismatch(Some(format!("lend it with `ref`: `ref {text}`"))),
            // `var total = new shared atomic 0` is an `int`; its type is given where it is made.
            (Ty::Shared(from, _), Ty::Shared(to, _)) if matches!((&**from, &**to), (Ty::Atomic(a), Ty::Atomic(b)) if matches!((&**a, &**b), (Ty::Int(_), Ty::Int(_)))) => {
                mismatch(Some(match &expr.kind {
                    ExprKind::Var(name) => format!("`new shared atomic 0` takes the type of an integer from where it goes; give `{name}` its type where it is made: `{expected} {name} = new shared atomic ...`"),
                    _ => format!("a number takes its type from where it goes: `{expected} value = new shared atomic ...`"),
                }))
            }
            // A value where a `read string?` goes: lent with `read`, as a value that is there.
            (_, Ty::Read(to)) if **to == Ty::Nullable(Box::new(found.clone())) => mismatch(Some(format!("lend it with `read`: `read {text}`"))),
            (Ty::Nullable(inner), _) if **inner == *expected => {
                mismatch(Some(format!("it may be `null`: give a fallback with `{text} ?? ...`, or test it with `is null`")))
            }
            (_, Ty::Result(ok, _)) if found == **ok => mismatch(Some(format!("success is written `Ok({text})`"))),
            (_, Ty::Result(_, error)) if found == **error => mismatch(Some(format!("an error is written `Error({text})`"))),
            (Ty::Result(ok, _), _) if **ok == *expected => {
                mismatch(Some(format!("open the `Result` first: `try {text}`, `{text} ?? ...` or `match`")))
            }
            _ => mismatch(None),
        }
    }

    /// The type of a variable or field, without using its value up. `None` if `expr` is
    /// not a place. The flag says whether it can be written to.
    fn place(&mut self, expr: &mut Expr) -> Res<Option<(Ty, bool)>> {
        let mark = self.borrows.touched.len();
        let found = self.place_unchecked(expr)?;
        if found.is_some() {
            // What a part lends lasts until the index or call runs, also where its value is only
            // looked at (`print(s[cut(ref s)])`, choice 1382), as in `expr`.
            if self.borrows.touched[mark..].iter().any(|touch| touch.2 != Access::Read) {
                self.lent_to_the_end(expr, mark)?;
            }
            self.check_parts(expr, false)?;
            self.touch(expr, Access::Read);
        }
        Ok(found)
    }

    /// `place` without the check for fields that were moved out.
    fn place_unchecked(&mut self, expr: &mut Expr) -> Res<Option<(Ty, bool)>> {
        let found = self.place_untyped(expr)?;
        if let Some((ty, _)) = &found {
            expr.ty = ty.clone();
        }
        Ok(found)
    }

    fn place_untyped(&mut self, expr: &mut Expr) -> Res<Option<(Ty, bool)>> {
        let span = expr.span;
        self.settle(expr)?;
        // `byte[16]`: a type name and a number in brackets build an array of zeros. An integer
        // const may give the number: `byte[BlockSize]`.
        if let ExprKind::Index { recv, index, .. } = &mut expr.kind {
            if let (ExprKind::Var(name), ExprKind::Var(count)) = (&recv.kind, &index.kind) {
                let is_type = Ty::from_name(name).is_some() || self.names.full(name, recv.span)?.is_some_and(|full| self.names.kinds.contains_key(&full));
                if is_type && self.var_named(name).is_none() && self.var_named(count).is_none() {
                    if let Some(full) = self.const_named(count, index.span)? {
                        match self.consts.values.get(&full) {
                            Some((_, ConstVal::Int(_, value))) => index.kind = ExprKind::Int(*value, IntTy::Nint),
                            _ => {
                                return Err(Diag::error(index.span, format!("`{count}` cannot give the count of an array: it is a `{}`", self.consts.types[&full]))
                                    .note("the count of an array is a number or an integer `const`"));
                            }
                        }
                    }
                }
            }
        }
        if let ExprKind::Index { recv, index, .. } = &expr.kind {
            if let (ExprKind::Var(name), ExprKind::Int(length, _)) = (&recv.kind, &index.kind) {
                let is_type = Ty::from_name(name).is_some() || self.names.full(name, recv.span)?.is_some_and(|full| self.names.kinds.contains_key(&full));
                if self.find(name).is_none() && name != "this" && is_type && *length >= 0 {
                    let ty = TypeExpr { mode: None, holder: None, checked: false, locked: false, atomic: None, raw: 0, name: name.clone(), args: Vec::new(), suffixes: vec![Suffix::Array(*length as usize)], span };
                    expr.kind = ExprKind::Array { ty, args: Vec::new(), items: None };
                    return Ok(None);
                }
            }
        }
        // `Shape.Empty` and `int.MaxValue` are values, not fields.
        if let ExprKind::Field { recv, .. } = &expr.kind {
            if self.enum_named(recv).is_some() || self.number_type_named(recv).is_some() || self.cancel_type_named(recv).is_some() {
                return Ok(None);
            }
        }
        match &mut expr.kind {
            // Not a variable, but the name of a function: a value, not a place.
            ExprKind::Var(name) if self.find(name).is_none() && self.sigs.contains_key(name.as_str()) && name != "main" => Ok(None),
            ExprKind::Var(name) => {
                let ty = self.live_var(name, span)?.current.clone();
                let writable = !matches!(ty, Ty::Read(_));
                Ok(Some((ty, writable)))
            }
            ExprKind::Field { recv, name, name_span, index } => {
                let (mut ty, mut writable) = match self.place_unchecked(recv)? {
                    Some(base) => base,
                    None => (self.expr(recv, None)?, true),
                };
                // Field access looks through pointers and borrows on its own.
                let struct_name = loop {
                    ty = match ty {
                        Ty::Read(inner) => {
                            writable = false;
                            *inner
                        }
                        // A `ref` reached through a `read` borrow (a `ref` field of `read
                        // this`) only reads, as `&&mut T` in Rust.
                        Ty::Ref(inner) => *inner,
                        Ty::Ptr(inner) => *inner,
                        Ty::Locked(_) => return Err(self.locked_outside(recv.span)),
                        // What a `shared` value points to is only read; through `shared
                        // checked` it is changed, however the owner itself was reached.
                        Ty::Shared(inner, checked) => {
                            writable = checked;
                            *inner
                        }
                        Ty::Struct(name) => break name,
                        // What a `for` over a `HashMap` gives. The key stays as it is: the
                        // map found the entry by it.
                        Ty::Entry(key, value) => {
                            let (position, field, writable) = match name.as_str() {
                                "key" => (0, *key, false),
                                "value" => (1, *value, writable),
                                _ => {
                                    return Err(Diag::error(*name_span, format!("an entry of a `HashMap` has a `key` and a `value`, no `{name}`")));
                                }
                            };
                            *index = position;
                            return Ok(Some((field, writable)));
                        }
                        Ty::List(_) | Ty::Map(..) | Ty::Set(_) if matches!(name.as_str(), "length" | "count" | "Count" | "size") => {
                            return Err(Diag::error(*name_span, format!("`{ty}` has no fields"))
                                .hint("the number of items is a method: `.length()`"));
                        }
                        Ty::Nullable(_) => {
                            let diag = Diag::error(recv.span, format!("this is `{ty}` and may be `null`; `.{name}` needs a value"));
                            return Err(diag.hint(self.test_first(recv, &ty)));
                        }
                        Ty::Enum(_) => {
                            return Err(Diag::error(*name_span, format!("`{ty}` is an enum; what it carries depends on the variant"))
                                .hint("reach it with `match`, or test one variant: `if x is Circle(r) { ... }`"));
                        }
                        Ty::Result(..) => {
                            return Err(Diag::error(recv.span, format!("this is `{ty}`: either a value or an error; `.{name}` needs the value"))
                                .hint("open it first with `try`, `??` or `match`"));
                        }
                        Ty::Weak(..) => {
                            let text = &self.src[recv.span.start..recv.span.end];
                            return Err(Diag::error(recv.span, format!("this is `{ty}`: what it looks at may be gone, so `.{name}` cannot reach it"))
                                .hint(format!("ask for an owner first: `var owner = share {text}`, which is `null` if it is gone")));
                        }
                        Ty::Raw(_) if points_at_opaque(&ty, self.structs) => {
                            return Err(Diag::error(*name_span, format!("a `{ty}` points at a type of C whose insides SG does not see")));
                        }
                        Ty::Raw(_) => {
                            let text = &self.src[recv.span.start..recv.span.end];
                            return Err(Diag::error(*name_span, format!("a `{ty}` has no fields: it is an address, and its items have them"))
                                .hint(format!("`{text}[0].{name}`")));
                        }
                        other => {
                            return Err(Diag::error(*name_span, format!("`{other}` has no fields")));
                        }
                    };
                };
                let fields = &self.structs[&struct_name].fields;
                let Some(position) = fields.iter().position(|(field, _)| field == name) else {
                    let diag = Diag::error(*name_span, format!("`{struct_name}` has no field `{name}`"));
                    return Err(match nearest(name, fields.iter().map(|(field, _)| field.as_str())) {
                        Some(close) => diag.hint(format!("`{close}`")),
                        None => diag,
                    });
                };
                let info = &self.structs[&struct_name];
                if !info.open[position] && !self.inside(&struct_name) {
                    return Err(self.not_public(name, &struct_name, *name_span));
                }
                // A field of a union is read as its own type, whichever field was written last;
                // written whole, it needs no `unsafe`.
                if info.shape.union && self.written.first() != Some(&span) {
                    // A field of a union that an assignment writes into: written whole, or not.
                    if self.written.contains(&span) {
                        let text = &self.src[span.start..span.end];
                        return Err(Diag::error(span, "a field of a union is written whole: what lies inside it is not written on its own")
                            .hint(format!("read it, change the copy and write it back: `var part = unsafe {text}`, then `{text} = part`")));
                    }
                    let field = (fields[position].1.clone(), writable && !info.readonly[position]);
                    *index = position;
                    // Read, even inside the place an assignment writes.
                    let writing = std::mem::replace(&mut self.writing, false);
                    let needed = self.needs_unsafe(span, "a field of a union is read as its own type, whichever field was written last");
                    self.writing = writing;
                    needed?;
                    return Ok(Some(field));
                }
                *index = position;
                Ok(Some((fields[position].1.clone(), writable && !info.readonly[position])))
            }
            // An item is a place too: `xs[i] = v`, `xs[i].name`, `ref xs[i]`.
            ExprKind::Index { recv, index, kind, .. } => {
                let (ty, mut writable) = match self.place_unchecked(recv)? {
                    Some(base) => base,
                    None => (self.expr(recv, None)?, true),
                };
                let mut inner = &ty;
                loop {
                    inner = match inner {
                        Ty::Read(behind) => {
                            writable = false;
                            behind
                        }
                        // A `ref` reached through a `read` borrow only reads.
                        Ty::Ref(behind) => behind,
                        Ty::Ptr(behind) => behind,
                        Ty::Locked(_) => return Err(self.locked_outside(recv.span)),
                        Ty::Shared(behind, checked) => {
                            writable = *checked;
                            behind
                        }
                        _ => break,
                    };
                }
                match inner {
                    Ty::Str => {
                        self.index(index, "an index")?;
                        *kind = IndexKind::Text;
                        Ok(Some((Ty::Int(IntTy::Byte), false)))
                    }
                    Ty::List(item) if matches!(**item, Ty::Task(_)) => {
                        let list = &self.src[recv.span.start..recv.span.end];
                        Err(Diag::error(span, "the tasks of a list are not reached one by one: they are taken out by `Task.whenAll` and `Task.whenAny`, which wait for them")
                            .hint(match self.is_async {
                                true => format!("`await Task.whenAny(ref {list})` takes out the first task to end, with what it gave"),
                                false => format!("`Task.whenAny(ref {list}).wait()` takes out the first task to end, with what it gave"),
                            }))
                    }
                    Ty::List(item) => {
                        self.index(index, "an index")?;
                        *kind = IndexKind::List;
                        Ok(Some(((**item).clone(), writable)))
                    }
                    Ty::Map(key, value) => {
                        self.typed(index, &self.table_key_param(key))?;
                        *kind = IndexKind::Map;
                        Ok(Some(((**value).clone(), writable)))
                    }
                    Ty::Array(item, length) => {
                        self.index(index, "an index")?;
                        // Both are known here, so it is not left to run time.
                        if let Some(at) = known_int(index).filter(|at| *at < 0 || *at >= *length as i128) {
                            return Err(Diag::error(span, format!("index {at} is out of range: the length is {length}"))
                                .note(format!("the items of a `{inner}` are at 0 to {}", length - 1)));
                        }
                        *kind = IndexKind::Array;
                        Ok(Some(((**item).clone(), writable)))
                    }
                    Ty::Slice(item) => {
                        self.index(index, "an index")?;
                        *kind = IndexKind::Array;
                        Ok(Some(((**item).clone(), writable)))
                    }
                    Ty::Raw(item) if **item == Ty::Void => {
                        let text = &self.src[recv.span.start..recv.span.end];
                        Err(Diag::error(span, "a `raw void` points at memory of no known type, so it has no items")
                            .hint(format!("say what lies there with a cast: `unsafe ((raw byte){text})[0]`")))
                    }
                    Ty::Raw(item) if matches!(&**item, Ty::Struct(name) if self.structs[name].opaque) => {
                        Err(Diag::error(span, format!("a `{ty}` points at a type of C whose insides and size SG does not know, so it has no items"))
                            .note("it is handed to the functions of C that know it"))
                    }
                    // Whatever reached the pointer, what it points at is not borrowed from it.
                    Ty::Raw(item) => {
                        let item = (**item).clone();
                        match &recv.kind {
                            ExprKind::CVar(name) => self.needs_unsafe(span, &format!("`{}` is a variable of C, which C may read and write at any time and from any thread", short_name(name)))?,
                            _ => self.needs_unsafe(span, "this reaches raw memory, whose size and life the checker does not know")?,
                        }
                        self.index(index, "an index")?;
                        *kind = IndexKind::Raw;
                        Ok(Some((item, true)))
                    }
                    _ => {
                        self.indexable(&ty, recv.span)?;
                        unreachable!("anything that can be indexed is handled above")
                    }
                }
            }
            _ => Ok(None),
        }
    }

    /// Whether the place is the `key` of what a `for` over a `HashMap` gives.
    fn is_entry_key(&mut self, expr: &Expr) -> bool {
        let ExprKind::Field { recv, name, .. } = &expr.kind else { return false };
        let ExprKind::Var(var) = &recv.kind else { return false };
        name == "key" && self.find(var).is_some_and(|var| matches!(var.current.strip_borrow(), Ty::Entry(..)))
    }

    /// How a method takes a key or an item it only looks at: what can be copied is copied,
    /// the rest (text, a struct that owns something) is lent.
    fn key_param(&self, key: &Ty) -> Ty {
        match key.is_copyable(self.copyable) {
            true => key.clone(),
            false => Ty::Read(Box::new(key.clone())),
        }
    }

    /// How a table takes a key it only looks up: as `key_param`, and a list as a slice of what it
    /// holds, as text is a `read string`, so a part of a buffer finds its key without being
    /// copied into a list (choice 1853). A list's own search of an item is not this: an item
    /// that is a list is a list there.
    fn table_key_param(&self, key: &Ty) -> Ty {
        match key {
            Ty::List(item) => Ty::Read(Box::new(Ty::Slice(item.clone()))),
            _ => self.key_param(key),
        }
    }

    /// Rejects the use of a place that overlaps a field that was moved out. When `assigning`,
    /// the place gets a new value, so only a moved field above it is in the way.
    fn check_parts(&mut self, expr: &Expr, assigning: bool) -> Res<()> {
        let Some((name, path)) = path_of(expr) else { return Ok(()) };
        let text = &self.src[expr.span.start..expr.span.end];
        let Some(var) = self.find(name) else { return Ok(()) };
        for (part, at) in &var.moved.parts {
            if within(&path, part) && (!assigning || path != *part) {
                return Err(Diag::error(expr.span, format!("`{name}.{part}` was moved and can no longer be used"))
                    .related(*at, "it was moved here"));
            }
            if !assigning && (path.is_empty() || within(part, &path)) {
                return Err(Diag::error(expr.span, format!("`{text}` cannot be used as a whole: `{name}.{part}` was moved out of it"))
                    .related(*at, "it was moved here")
                    .hint(format!("give `{name}.{part}` a value again, or use the other fields one by one")));
            }
        }
        Ok(())
    }

    /// Uses up the value at a place. A field can be moved out of a value that is owned;
    /// through a borrow the owner would be left with a hole.
    fn move_out(&mut self, expr: &Expr, ty: &Ty) -> Res<()> {
        let span = expr.span;
        let text = &self.src[span.start..span.end];
        // A collection cannot be left with a hole where an item was.
        if expr.in_item() {
            let hint = match ty {
                Ty::Nullable(_) => format!("borrow it: `read {text}`, or take it with `{text}.take()`, which leaves `null` there"),
                _ => format!("borrow it: `read {text}`, or swap it with `{text}.replace(value)`"),
            };
            return Err(Diag::error(span, format!("a `{ty}` cannot be moved out of an item of a collection")).hint(hint));
        }
        // A `Disposable` value is whole when its `dispose` runs.
        let mut part = expr;
        while let ExprKind::Field { recv, .. } = &part.kind {
            let mut holder = recv.ty.strip_borrow();
            while let Ty::Ptr(inner) = holder {
                holder = inner;
            }
            if let Ty::Struct(owner) = holder {
                if self.structs.get(owner).is_some_and(|info| info.disposable) {
                    let hint = match ty {
                        Ty::Nullable(_) => format!("take it with `{text}.take()`, which leaves `null` there, or borrow it: `read {text}`"),
                        _ => format!("swap it with `{text}.replace(value)`, or borrow it: `read {text}`"),
                    };
                    return Err(Diag::error(span, format!("a `{ty}` field cannot be moved out of a `{}`: it is `Disposable`, and its `dispose` sees it whole", short_name(owner))).hint(hint));
                }
            }
            part = recv;
        }
        if let ExprKind::Field { recv, .. } = &expr.kind {
            if through_shared(recv) {
                return Err(Diag::error(span, format!("a `{ty}` field cannot be moved out of a `shared` value: its other owners still look at it"))
                    .hint(format!("borrow it instead: `read {text}`")));
            }
            // Through `shared checked` the field can be swapped, so that no owner sees a hole.
            if through_holder(recv, true) {
                let hint = match ty {
                    Ty::Nullable(_) => format!("take it with `{text}.take()`, which leaves `null` there, or borrow it: `read {text}`"),
                    _ => format!("swap it with `{text}.replace(value)`, or borrow it: `read {text}`"),
                };
                return Err(Diag::error(span, format!("a `{ty}` field cannot be moved out of a `shared checked` value: its other owners still look at it")).hint(hint));
            }
        }
        self.not_locked(expr, Access::Move)?;
        let Some((name, path)) = path_of(expr) else { return Ok(()) };
        let Some(var) = self.find(name) else { return Ok(()) };
        if path.is_empty() {
            var.moved.whole = Some(span);
            return Ok(());
        }
        let hint = match (&var.current, ty) {
            (Ty::Read(_), _) => format!("borrow it instead: `read {text}`"),
            (Ty::Ref(_), Ty::Nullable(_)) => format!("take it with `{text}.take()`, which leaves `null` there"),
            (Ty::Ref(_), _) => format!("swap it with `{text}.replace(value)`, or borrow it: `read {text}`"),
            _ => {
                var.moved.parts.push((path, span));
                return Ok(());
            }
        };
        Err(Diag::error(span, format!("a `{ty}` field cannot be moved out through a borrow")).hint(hint))
    }

    /// An operand that is only looked at (`a + b`, `print(x)`, `x.length()`): a variable
    /// used this way is not moved.
    fn operand(&mut self, expr: &mut Expr, expected: Option<&Ty>) -> Res<Ty> {
        match self.place(expr)? {
            Some((ty, _)) => Ok(ty),
            None => self.expr(expr, expected),
        }
    }

    fn int_operand(&mut self, expr: &mut Expr, like: Option<IntTy>, what: &str) -> Res<IntTy> {
        let expected = like.map(Ty::Int);
        let found = self.operand(expr, expected.as_ref())?;
        match found.strip_borrow() {
            Ty::Int(ty) => {
                let ty = *ty;
                self.coerce(expr, found, &Ty::Int(ty))?;
                Ok(ty)
            }
            other => Err(Diag::error(expr.span, format!("{what} must be an integer, found `{other}`"))),
        }
    }

    /// The type two integers meet in: the one the other widens to. A number written in the
    /// program takes the type of the other side if it fits there (`1 + count` with a `uint
    /// count`). Two types that do not hold each other's values, like `int` and `uint`, meet
    /// only through a cast that the program writes.
    ///
    /// Both sides are brought to that type; `left_found` and `right_found` are what was found
    /// for them, a borrow of the integer or the integer itself.
    fn common_int(&mut self, left: &mut Expr, left_found: Ty, right: &mut Expr, right_found: Ty) -> Res<IntTy> {
        let (Ty::Int(a), Ty::Int(b)) = (left_found.strip_borrow().clone(), right_found.strip_borrow().clone()) else {
            unreachable!("only called for two integers");
        };
        let mut found = [left_found, right_found];
        let common = match a.unify(b) {
            Some(common) => Some(common),
            None => [(&mut *left, b, 0), (&mut *right, a, 1)].into_iter().find_map(|(side, to, at)| match &mut side.kind {
                ExprKind::Int(value, ty) if to.fits(*value) => {
                    *ty = to;
                    side.ty = Ty::Int(to);
                    found[at] = Ty::Int(to);
                    Some(to)
                }
                _ => None,
            }),
        };
        if let Some(common) = common {
            let [left_found, right_found] = found;
            self.coerce(left, left_found, &Ty::Int(common))?;
            self.coerce(right, right_found, &Ty::Int(common))?;
            return Ok(common);
        }
        let span = left.span.to(right.span);
        let diag = Diag::error(span, format!("`{}` and `{}` do not mix: neither holds every value of the other", a.name(), b.name()));
        let text = &self.src[right.span.start..right.span.end];
        let text = if matches!(right.kind, ExprKind::Binary(..)) { format!("({text})") } else { text.to_string() };
        Err(match a.holding_both(b) {
            Some(both) => diag.hint(format!("`{0}` holds both: `({0}){text}`", both.name())),
            None => diag.hint(format!("convert one side to the type of the other: `({}){text}`; it panics if the value does not fit", a.name())),
        })
    }

    /// The fractional type two numbers meet in, at least one of them fractional: `double`
    /// if either is one. An integer comes along if the type holds every value of it, and a
    /// number written in the program becomes a fractional one (`2 * x`).
    fn common_float(&mut self, left: &mut Expr, left_found: Ty, right: &mut Expr, right_found: Ty) -> Res<FloatTy> {
        let float_of = |ty: &Ty| match ty.strip_borrow() {
            Ty::Float(ty) => Some(*ty),
            _ => None,
        };
        let common = match (float_of(&left_found), float_of(&right_found)) {
            (Some(FloatTy::Float), Some(FloatTy::Float)) => FloatTy::Float,
            (Some(_), Some(_)) => FloatTy::Double,
            (Some(ty), None) | (None, Some(ty)) => ty,
            (None, None) => unreachable!("called with a fractional number on one side"),
        };
        let to = Ty::Float(common);
        let mut found = [left_found, right_found];
        for (side, at) in [(&mut *left, 0), (&mut *right, 1)] {
            if let ExprKind::Int(value, _) = side.kind {
                if !common.exact(value) {
                    return Err(Diag::error(side.span, format!("{value} is not held exactly by a `{}`", common.name()))
                        .note(format!("a `{}` holds every whole number up to 2^{}", common.name(), common.digits())));
                }
                side.kind = ExprKind::Float(value.to_string(), common);
                side.ty = to.clone();
                found[at] = to.clone();
            }
        }
        let [left_found, right_found] = found;
        for (side, found) in [(&mut *left, left_found), (&mut *right, right_found)] {
            if let Ty::Int(int) = found.strip_borrow() {
                if !common.holds(*int) {
                    let text = &self.src[side.span.start..side.span.end];
                    let text = if matches!(side.kind, ExprKind::Binary(..)) { format!("({text})") } else { text.to_string() };
                    return Err(Diag::error(side.span, format!("`{0}` and `{to}` do not mix: a `{to}` does not hold every `{0}` exactly", int.name()))
                        .hint(format!("convert it: `({to}){text}`, which rounds past 2^{}", common.digits())));
                }
            }
            self.coerce(side, found, &to)?;
        }
        Ok(common)
    }

    /// Checks an expression whose value is used. A variable of a non-copyable type is moved.
    fn expr(&mut self, expr: &mut Expr, expected: Option<&Ty>) -> Res<Ty> {
        let mark = self.borrows.touched.len();
        let ty = self.expr_untyped(expr, expected)?;
        unchecked_index(expr);
        expr.ty = ty.clone();
        if self.borrows.touched[mark..].iter().any(|touch| touch.2 != Access::Read) {
            self.lent_to_the_end(expr, mark)?;
        }
        Ok(ty)
    }

    /// What a part of a call lends it: the receiver a method looks at, a borrow given as an
    /// argument, or a value that carries one. The call runs after its last part, so this
    /// stays lent until then.
    fn part_loans(&self, part: &Expr, receiver: bool) -> Vec<Loan> {
        let loans = match &part.kind {
            ExprKind::Borrow { .. } => self.loans_of(part, false),
            ExprKind::Method { recv, callee: Callee::Pointer, .. } => self.place_loans(recv, true, part.span),
            ExprKind::RawCast { expr, .. } => match &expr.kind {
                ExprKind::Method { recv, callee: Callee::Pointer, .. } => self.place_loans(recv, true, part.span),
                _ => Vec::new(),
            },
            // A receiver is lent where it is, unless it is a number, which is copied.
            _ if part.is_place() && !part.moved && receiver => match part.ty.strip_borrow() {
                Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char => Vec::new(),
                _ => self.place_loans(part, false, part.span),
            },
            _ if part.is_place() && !part.moved && !part.ty.is_copyable(self.copyable) => self.place_loans(part, false, part.span),
            _ if part.ty.is_borrow() || self.holds(&part.ty) => self.loans_of(part, false),
            _ => Vec::new(),
        };
        loans.into_iter().filter(|loan| loan.root != UNNAMED && !loan.held).collect()
    }

    /// The parts of a call, an index or a range are computed from left to right, and the call
    /// runs after the last one: what an earlier part lent must not be changed or moved by a
    /// later one (`f(read xs[0], g(ref xs))`, `consume(read p, p)`, `s[change(ref s)..]`).
    /// A later part may read it.
    fn lent_to_the_end(&self, expr: &Expr, mark: usize) -> Res<()> {
        let (parts, what): (Vec<(&Expr, bool)>, &str) = match &expr.kind {
            ExprKind::Method { recv, args, .. } => (std::iter::once((&**recv, true)).chain(args.iter().map(|arg| (arg, false))).collect(), "call"),
            ExprKind::Call { args, .. } => (args.iter().map(|arg| (arg, false)).collect(), "call"),
            ExprKind::Index { recv, index, .. } => (vec![(&**recv, true), (&**index, false)], "index"),
            ExprKind::Slice { recv, from, to } => {
                (std::iter::once((&**recv, true)).chain(from.iter().chain(to.iter()).map(|bound| (&**bound, false))).collect(), "range")
            }
            // Every expression whose parts are computed one after another before it is made
            // (choice 1592): `Word { text = read xs[0], n = g(ref xs) }`, `xs[0] == take(ref xs)`.
            ExprKind::StructLit { fields, .. } => (fields.iter().map(|field| (&field.value, false)).collect(), "value"),
            ExprKind::With { base, fields } => (std::iter::once((&**base, false)).chain(fields.iter().map(|field| (&field.value, false))).collect(), "value"),
            ExprKind::Variant { args, .. } => (args.iter().map(|arg| (arg, false)).collect(), "value"),
            ExprKind::Array { args, items, .. } => (args.iter().chain(items.iter().flatten()).map(|part| (part, false)).collect(), "value"),
            ExprKind::Collection { args, items, .. } => (args.iter().chain(items.iter().flatten().flat_map(|item| &item.values)).map(|part| (part, false)).collect(), "value"),
            ExprKind::Binary(_, left, right) | ExprKind::Coalesce { left, right, .. } => (vec![(&**left, false), (&**right, false)], "value"),
            ExprKind::Interp(pieces) => (
                pieces
                    .iter()
                    .filter_map(|piece| match piece {
                        InterpPart::Expr(inner, _) => Some((inner, false)),
                        _ => None,
                    })
                    .collect(),
                "value",
            ),
            _ => return Ok(()),
        };
        let touched = &self.borrows.touched[mark..];
        for (part, receiver) in &parts {
            let loans = self.part_loans(part, *receiver);
            if loans.is_empty() {
                continue;
            }
            for (root, path, access, span, _) in touched {
                if *access == Access::Read || span.start < part.span.end || span.start >= expr.span.end {
                    continue;
                }
                let overlaps = |loan: &Loan| path.is_empty() || loan.path.is_empty() || within(path, &loan.path) || within(&loan.path, path);
                let Some(loan) = loans.iter().find(|loan| loan.root == *root && overlaps(loan)) else { continue };
                let text = self.src[span.start..span.end].trim();
                let earlier = self.src[part.span.start..part.span.end].trim();
                let given = match (*receiver, what) {
                    (true, "call") => format!("`{earlier}` is what this method is called on"),
                    (true, _) => format!("`{earlier}` is what this {what} looks into"),
                    (false, "value") => format!("`{earlier}` comes before it in this value"),
                    (false, _) => format!("`{earlier}` is given to this {what} before it"),
                };
                let done = if *access == Access::Move { "moved" } else { "changed" };
                let note = match what {
                    "value" => "the parts of a value are computed from left to right, and it is made after the last one".to_string(),
                    _ => format!("the parts of {} {what} are computed from left to right, and it runs after the last one", if what == "index" { "an" } else { "a" }),
                };
                let diag = Diag::error(*span, format!("`{text}` cannot be {done} here: {given}, and the {what} still uses it")).note(note);
                let diag = match (part.span.start..part.span.end).contains(&loan.span.start) && loan.span != part.span {
                    true => diag.related(loan.span, format!("lent to the {what} here")),
                    false => diag.related(part.span, format!("lent to the {what} here")),
                };
                let whole = parts.iter().map(|(later, _)| *later).find(|later| later.span.start <= span.start && span.start < later.span.end);
                return Err(match (access, whole) {
                    (Access::Move, _) | (_, None) => diag.hint(format!("the {what} cannot both look at `{earlier}` and take `{text}`: give it a copy of one of them")),
                    (_, Some(Expr { kind: ExprKind::Borrow { mutable: true, .. }, .. })) => diag.hint("a `ref` borrow must be the only one of its place: lend it once"),
                    (_, Some(whole)) => {
                        let whole = self.src[whole.span.start..whole.span.end].trim();
                        diag.hint(format!("compute it before the {what}: `var value = {whole}`, then give it `value`"))
                    }
                });
            }
        }
        Ok(())
    }

    fn expr_untyped(&mut self, expr: &mut Expr, expected: Option<&Ty>) -> Res<Ty> {
        let span = expr.span;
        // `read Masks`: a const array, struct or enum is lent already, for the whole run.
        // `ref Masks` would change it.
        if let ExprKind::Borrow { mutable, expr: inner } = &mut expr.kind {
            // `read Table`: a static is lent already, for the whole run, as a const is.
            if let Some(name) = self.static_root(inner) {
                if *mutable {
                    return Err(Checker::static_changed(&name, inner.span));
                }
                if matches!(inner.kind, ExprKind::Var(_)) {
                    let inner = std::mem::replace(&mut **inner, Expr { kind: ExprKind::Null, span, ty: Ty::Void, moved: false });
                    *expr = Expr { span, ..inner };
                    let ty = self.expr_untyped(expr, expected)?;
                    return match ty {
                        Ty::Read(_) => Ok(ty),
                        _ => {
                            let text = &self.src[span.start..span.end];
                            Err(Diag::error(span, format!("`{}` is a static of `{ty}`, which is copied where it is used, not lent", text.trim_start_matches("read").trim()))
                                .hint(format!("`{}`", text.trim_start_matches("read").trim())))
                        }
                    };
                }
            }
            if let Some(name) = self.const_root(inner) {
                if *mutable {
                    return Err(Checker::const_changed(&name, inner.span));
                }
                if matches!(inner.kind, ExprKind::Var(_)) {
                    let inner = std::mem::replace(&mut **inner, Expr { kind: ExprKind::Null, span, ty: Ty::Void, moved: false });
                    *expr = Expr { span, ..inner };
                    let ty = self.expr_untyped(expr, expected)?;
                    return match ty {
                        Ty::Read(_) => Ok(ty),
                        _ => {
                            let text = &self.src[span.start..span.end];
                            Err(Diag::error(span, format!("`{}` is a const of `{ty}`, which is copied where it is used, not lent", text.trim_start_matches("read").trim()))
                                .hint(format!("`{}`", text.trim_start_matches("read").trim())))
                        }
                    };
                }
            }
        }
        if let Some((ty, _)) = self.place(expr)? {
            // What lies in raw memory is plain data: reading it copies the bytes.
            if Expr::in_raw_memory(expr) {
                return Ok(ty);
            }
            if !ty.is_copyable(self.copyable) {
                // `xs[i]` reads the item where it is: what cannot be copied is lent.
                if matches!(expr.kind, ExprKind::Index { .. }) {
                    wrap(expr, |inner| ExprKind::Borrow { mutable: false, expr: inner });
                    return Ok(Ty::Read(Box::new(ty)));
                }
                self.move_out(expr, &ty)?;
                expr.moved = true;
            }
            return Ok(ty);
        }

        // The name of a function is a value: its address.
        if let ExprKind::Var(name) = &expr.kind {
            let sig = &self.sigs[name.as_str()];
            // Where a `CFunc` is expected, the name of a function C can call is its address.
            if let Some(Ty::CFunc(params, ret)) = expected.map(Ty::strip_borrow) {
                let short = short_name(name);
                if !sig.foreign && !sig.export {
                    return Err(Diag::error(expr.span, format!("C cannot call `{short}`: it is a function of SG"))
                        .hint(format!("a function C calls is written with `export`: `export {} {short}(...)`", sig.ret)));
                }
                if sig.variadic {
                    return Err(Diag::error(expr.span, format!("`{short}` takes `...`, and a `CFunc` points at a function that takes a fixed count of arguments"))
                        .note("C calls a function that takes `...` by a type that says so, and SG has none")
                        .hint(format!("call `{short}` from an `export` function of your own, and use that")));
                }
                if sig.params != *params || sig.ret != **ret {
                    let found = Ty::CFunc(sig.params.clone(), Box::new(sig.ret.clone()));
                    return Err(Diag::error(expr.span, format!("expected `{}`, found `{found}`", Ty::CFunc(params.clone(), ret.clone()))));
                }
                let ty = Ty::CFunc(sig.params.clone(), Box::new(sig.ret.clone()));
                expr.kind = ExprKind::FuncRef(name.clone());
                return Ok(ty);
            }
            if sig.foreign {
                return Err(Diag::error(expr.span, format!("`{}` is a function of C, and a `Func` holds the address of a function of SG", short_name(name)))
                    .hint("wrap the call in a function of your own and use that"));
            }
            if sig.is_async {
                return Err(Diag::error(expr.span, format!("`{}` is async, and an async function is called by its name, not kept as a value", short_name(name)))
                    .hint(format!("call it: `{}(...)` starts it as a task, `await {}(...)` runs it here", short_name(name), short_name(name))));
            }
            let ty = Ty::Func(sig.params.clone(), Box::new(sig.ret.clone()));
            expr.kind = ExprKind::FuncRef(name.clone());
            return Ok(ty);
        }

        // `new NodeRef(1)` with `using NodeRef = shared Node` is `new shared Node(1)`.
        if let ExprKind::New(inner) = &mut expr.kind {
            if let Some((checked, locked)) = self.shared_alias(inner)? {
                let ExprKind::New(inner) = std::mem::replace(&mut expr.kind, ExprKind::Null) else { unreachable!("matched above") };
                expr.kind = ExprKind::NewShared { value: inner, checked, locked };
            }
        }

        // `Shape.Empty` and `Shape.Circle(2)` build a value of the enum.
        match &mut expr.kind {
            ExprKind::Field { recv, name, name_span, .. } if self.cancel_type_named(recv).is_some() => {
                let ty = self.cancel_type_named(recv).expect("tested above");
                if ty != Ty::Token || name != "None" {
                    let diag = Diag::error(*name_span, format!("`{ty}` has no constant `{name}`"));
                    return Err(match ty {
                        Ty::Token => diag.hint("`CancellationToken.None` is the token that is never cancelled"),
                        _ => diag.hint("`CancellationSource()` makes a source; `CancellationSource.after(ms)` one that cancels itself"),
                    });
                }
                expr.kind = ExprKind::Constant { ty: ty.clone(), name: std::mem::take(name) };
                return Ok(ty);
            }
            ExprKind::Field { recv, name, name_span, .. } if self.number_type_named(recv).is_some() => {
                let ty = self.number_type_named(recv).expect("tested above");
                constant(&ty, name, *name_span)?;
                expr.kind = ExprKind::Constant { ty: ty.clone(), name: std::mem::take(name) };
                return Ok(ty);
            }
            ExprKind::Field { recv, name, name_span, .. } => {
                let owner = self.enum_named(recv).expect("any other field is a place");
                let (index, ty) = self.variant(owner, name, *name_span, None, span)?;
                expr.kind = ExprKind::Variant { owner: owner.to_string(), index, args: Vec::new() };
                return Ok(ty);
            }
            ExprKind::Method { recv, name, name_span, args, .. } => {
                // `jobs.sender()` gives the channel its first sender: from here it may be used (1860).
                if name == "sender" && args.is_empty() {
                    let channel = match &recv.kind {
                        ExprKind::Borrow { expr, .. } => &expr.kind,
                        kind => kind,
                    };
                    if let ExprKind::Var(channel) = channel {
                        if let Some(var) = self.find(channel) {
                            var.unsent = None;
                        }
                    }
                }
                // `Shape.unit()`: a function of the enum, not a variant.
                // `IoError.fromCode(n)` is a function of the built-in enum, checked with the calls.
                let built_in = matches!(name.as_str(), "fromCode" | "fromErrno") && matches!(&recv.kind, ExprKind::Var(owner) if owner == IO_ERROR);
                // `Block.fromValue(n)` is a function the language gives an enum with numbers.
                let from_value = |owner: &str| name == "fromValue" && self.enums[owner].numbers.is_some();
                if let Some(owner) = self.enum_named(recv).filter(|owner| !built_in && !self.methods_of(owner).contains_key(name.as_str()) && !from_value(owner)) {
                    // `await Shape.Circle(f())` awaits no call in it.
                    self.awaiting = false;
                    let (index, ty) = self.variant(owner, name, *name_span, Some(args), span)?;
                    let args = std::mem::take(args);
                    expr.kind = ExprKind::Variant { owner: owner.to_string(), index, args };
                    return Ok(ty);
                }
            }
            ExprKind::Call { name, args, .. } if name == "Ok" || name == "Error" => {
                self.awaiting = false;
                let (index, ty) = self.result_value(name == "Ok", args, expected, span)?;
                let args = std::mem::take(args);
                expr.kind = ExprKind::Variant { owner: "Result".to_string(), index, args };
                return Ok(ty);
            }
            _ => {}
        }

        // A whole number written where a fractional one goes is one: `double half = 1 / 2.0`.
        if let (ExprKind::Int(value, _), Some(FloatTy::Float | FloatTy::Double)) = (&expr.kind, expected.and_then(wanted_float)) {
            let (value, to) = (*value, expected.and_then(wanted_float).expect("tested above"));
            if !to.exact(value) {
                return Err(Diag::error(span, format!("{value} is not held exactly by a `{}`", to.name()))
                    .note(format!("a `{}` holds every whole number up to 2^{}", to.name(), to.digits())));
            }
            expr.kind = ExprKind::Float(value.to_string(), to);
        }
        match &mut expr.kind {
            // Put in after a function is checked; nothing is checked twice.
            ExprKind::Open { .. } | ExprKind::Within(_) => unreachable!("the opening of `shared checked` values comes after the check"),
            ExprKind::Unsafe(inner) => {
                self.enter_unsafe(span)?;
                let result = self.expr(inner, expected);
                let bare = matches!(inner.ty, Ty::Raw(_)) && inner.is_place();
                let ty = self.leave_unsafe(result).map_err(|diag| match bare {
                    // `unsafe p + 1` is `(unsafe p) + 1`, as with `try`.
                    true => diag.hint("`unsafe` takes the value right after it, as `try` does; for a sum, put it in parentheses: `unsafe (p + n)`"),
                    false => diag,
                })?;
                unwrap_unsafe(expr);
                Ok(ty)
            }
            // `Memory.sizeOf<T>()`, `Memory.alignOf<T>()`: what C has for a type of plain data.
            ExprKind::SizeOf { item, align, of } => {
                let call = if *align { "Memory.alignOf" } else { "Memory.sizeOf" };
                if let Some(full) = self.names.full(&item.name, item.span)?.filter(|full| self.names.opaque.contains(full) && item.raw == 0 && item.suffixes.is_empty()) {
                    return Err(Diag::error(item.span, format!("`{}` is a type of C whose size SG does not know", short_name(&full))));
                }
                if item.mode.is_some() {
                    return Err(Diag::error(item.span, format!("`{call}` takes a type of plain data, not a borrow")));
                }
                let ty = resolve_type(item, self.names, false)?;
                if let Some(part) = not_plain(&ty, self.structs) {
                    let diag = Diag::error(item.span, format!("`{call}` takes a type of plain data, and `{ty}` is not"));
                    let diag = if part == ty { diag } else { diag.note(format!("it holds a `{part}`")) };
                    return Err(diag.note("plain data is laid out as C lays it out, so its size is that of C; the size of another value is not part of the language"));
                }
                *of = ty;
                Ok(Ty::Int(IntTy::Nint))
            }
            // `Memory.allocate<int>(n)`: room for `n` items, all zero.
            ExprKind::Allocate { item, args } => {
                if let Some(full) = self.names.full(&item.name, item.span)?.filter(|full| self.names.opaque.contains(full) && item.raw == 0 && item.suffixes.is_empty()) {
                    let short = short_name(&full);
                    return Err(Diag::error(item.span, format!("`{short}` is a type of C whose size SG does not know, so only C makes one"))
                        .note(format!("a function of C gives a `raw {short}`, and another gives it back")));
                }
                let item_ty = resolve_type(item, self.names, false)?;
                RAW.with(|found| found.borrow_mut().push((item_ty.clone(), item.span)));
                if item.mode.is_some() {
                    return Err(Diag::error(item.span, "raw memory holds plain data, not borrows"));
                }
                self.args("Memory.allocate", args, &[Ty::Int(IntTy::Nint)], span)?;
                Ok(Ty::Raw(Box::new(item_ty)))
            }
            // `(raw byte)p`
            ExprKind::RawCast { ty, expr: inner } => {
                let target = resolve_type(ty, self.names, false)?.strip_borrow().clone();
                let void = Ty::Raw(Box::new(Ty::Void));
                // `(ref State)data`: the address `Memory.addressOf` gave, lent again.
                if let Some(mode) = ty.mode {
                    let found = self.expr(inner, Some(&void))?;
                    if *found.strip_borrow() != void {
                        return Err(Diag::error(inner.span, format!("a cast to `{} {target}` lends what lies at an address `Memory.addressOf` gave, and this is `{found}`", if mode == Mode::Ref { "ref" } else { "read" }))
                            .note("the address is a `raw void`, as C keeps it"));
                    }
                    self.coerce(inner, found, &void)?;
                    self.needs_unsafe(span, &format!("this lends a `{target}` that the checker cannot see is there and still lives"))?;
                    return Ok(match mode {
                        Mode::Ref => Ty::Ref(Box::new(target)),
                        Mode::Read => Ty::Read(Box::new(target)),
                    });
                }
                // `(CFunc<int, int>)p`: an address C gave, as a function of C.
                if let Ty::CFunc(..) = &target {
                    let found = self.expr(inner, Some(&void))?;
                    if *found.strip_borrow() != void {
                        let diag = Diag::error(inner.span, format!("a cast to `{target}` reads an address C gave as a function of C, and this is `{found}`"))
                            .note("the address is a `raw void`, as `dlsym` gives it");
                        return Err(match found.strip_borrow() {
                            Ty::Int(IntTy::Nint | IntTy::Nuint) => diag.hint(format!("a number is made an address first: `({target})(raw void){}`", &self.src[inner.span.start..inner.span.end])),
                            _ => diag,
                        });
                    }
                    self.coerce(inner, found, &void)?;
                    self.needs_unsafe(span, "this takes an address for a function of C, which may be no function, or one that takes other values")?;
                    return Ok(target);
                }
                if !matches!(target, Ty::Raw(_)) {
                    return Err(Diag::error(ty.span, format!("a cast with `raw` gives a raw pointer, and `{target}` is not one")));
                }
                // A number written here is an address: `(raw void)(nint)-1`, `(raw byte)0x1000`.
                let found = self.expr(inner, Some(&Ty::Int(IntTy::Nint)))?;
                let stripped = found.strip_borrow().clone();
                // The address of a function of C, as C's `void*`.
                if let Ty::CFunc(..) = stripped {
                    if target != void {
                        return Err(Diag::error(ty.span, format!("the address of a function of C is a `raw void`, and this is `{target}`")));
                    }
                    self.coerce(inner, found, &stripped)?;
                    return Ok(target);
                }
                if let Ty::Int(IntTy::Nint | IntTy::Nuint) = stripped {
                    self.coerce(inner, found, &stripped)?;
                    self.needs_unsafe(span, "this makes a pointer out of a number, which may be no address of anything")?;
                    return Ok(target);
                }
                if !matches!(stripped, Ty::Raw(_)) {
                    let diag = Diag::error(inner.span, format!("a cast to `{target}` reads a raw pointer as another, or an address as a pointer, and this is `{found}`"));
                    return Err(match stripped {
                        Ty::Int(_) => diag.hint(format!("an address is a `nint` or a `nuint`: `({target})(nuint){}`", &self.src[inner.span.start..inner.span.end])),
                        _ => diag.note("a raw pointer is made by `Memory.allocate<T>(n)`"),
                    });
                }
                self.coerce(inner, found, &stripped)?;
                self.needs_unsafe(span, "this reads a raw pointer as one to another type")?;
                Ok(target)
            }
            ExprKind::Float(text, ty) => {
                *ty = expected.and_then(wanted_float).unwrap_or(FloatTy::Double);
                fractional_literal(text, *ty, span)?;
                Ok(Ty::Float(*ty))
            }
            ExprKind::Int(value, ty) => {
                let wanted = expected.and_then(|ty| match ty.strip_borrow() {
                    Ty::Int(ty) => Some(*ty),
                    Ty::Nullable(inner) => match **inner {
                        Ty::Int(ty) => Some(ty),
                        _ => None,
                    },
                    _ => None,
                });
                *ty = match wanted {
                    Some(wanted) if wanted.fits_everywhere(*value) => wanted,
                    Some(wanted) => {
                        // A number written in hex or in binary is shown as written, with its value.
                        let written = &self.src[span.start..span.end];
                        let shown = if written.starts_with("0x") || written.starts_with("0b") { format!("`{written}` is {value}, which") } else { value.to_string() };
                        let diag = Diag::error(span, format!("{shown} does not fit in {}", wanted.name()));
                        return Err(match *value < 0 && !wanted.signed() {
                            true => diag.note(format!("a `{}` is never negative", wanted.name())),
                            false if wanted.is_c() => c_too_large(diag, wanted, written),
                            false => diag,
                        });
                    }
                    None if IntTy::Int.fits(*value) => IntTy::Int,
                    None if IntTy::Long.fits(*value) => IntTy::Long,
                    // Only a number past every `long`: it is never taken as unsigned on its own.
                    None => {
                        let written = &self.src[span.start..span.end];
                        return Err(Diag::error(span, format!("{written} does not fit in long"))
                            .hint(format!("write the type where it goes: `ulong x = {written}`")));
                    }
                };
                Ok(Ty::Int(*ty))
            }
            ExprKind::Bool(_) => Ok(Ty::Bool),
            ExprKind::Str(_) => Ok(Ty::Str),
            // A character is a `char`, and a `byte` where an integer goes: from there it
            // widens like any other integer.
            ExprKind::Char(value) => {
                let wants_int = expected.is_some_and(|ty| match ty.strip_borrow() {
                    Ty::Int(_) => true,
                    Ty::Nullable(inner) => matches!(**inner, Ty::Int(_)),
                    _ => false,
                });
                if !wants_int {
                    return Ok(Ty::Char);
                }
                let value = byte_of(*value, span)?;
                expr.kind = ExprKind::Int(value, IntTy::Byte);
                Ok(Ty::Int(IntTy::Byte))
            }
            ExprKind::Collection { ty, args, items } => {
                let built = resolve_type(ty, self.names, false)?;
                if matches!(built, Ty::Channel(_) | Ty::Sender(_)) {
                    return self.new_channel(&built, ty.span, args, items.is_some(), span);
                }
                if let Some(arg) = args.first() {
                    return Err(Diag::error(arg.span, format!("`{built}(...)` takes no arguments"))
                        .hint("the items are written in braces: `List<int> { 1, 2 }`"));
                }
                match &built {
                    Ty::List(item) => {
                        for given in items.iter_mut().flatten() {
                            if given.braced {
                                return Err(Diag::error(given.span, "an item of a `List` is one value, written without braces")
                                    .hint("`List<int> { 1, 2 }`"));
                            }
                            self.typed(&mut given.values[0], item)?;
                        }
                    }
                    Ty::Set(item) => {
                        for given in items.iter_mut().flatten() {
                            if given.braced {
                                return Err(Diag::error(given.span, "an item of a `HashSet` is one value, written without braces")
                                    .hint("`HashSet<int> { 1, 2 }`"));
                            }
                            self.typed(&mut given.values[0], item)?;
                        }
                    }
                    Ty::Map(key, value) => {
                        for given in items.iter_mut().flatten() {
                            let [key_given, value_given] = given.values.as_mut_slice() else {
                                return Err(Diag::error(given.span, "an item of a `HashMap` is a key and a value, in braces of their own")
                                    .hint("`HashMap<string, int> { {\"x\", 10}, {\"y\", 20} }`"));
                            };
                            self.typed(key_given, key)?;
                            self.typed(value_given, value)?;
                        }
                    }
                    other => return Err(Diag::error(ty.span, format!("a `{other}` is not built this way"))),
                }
                Ok(built)
            }
            ExprKind::Slice { recv, from, to } => Ok(self.range(recv, from, to)?.0),
            ExprKind::Array { ty, args, items } => {
                let built = resolve_type(ty, self.names, false)?;
                let Ty::Array(item, length) = &built else {
                    return Err(Diag::error(ty.span, format!("a `{built}` is not built this way")));
                };
                let one_by_one = format!("write the items one by one: `{built} {{ ... }}`");
                match (items, args.as_mut_slice()) {
                    (Some(_), [arg, ..]) => {
                        return Err(Diag::error(arg.span, "an array gets its items in braces or one value for all of them in parentheses, not both"));
                    }
                    (Some(items), []) => {
                        if items.len() != *length {
                            return Err(Diag::error(span, format!("`{built}` needs {length} item(s), {} written", items.len())));
                        }
                        for given in items.iter_mut() {
                            self.typed(given, item)?;
                        }
                    }
                    (None, [value]) => {
                        if !item.is_copyable(self.copyable) {
                            return Err(Diag::error(value.span, format!("`{item}` is not `Copyable`, so one value cannot be put in every item")).hint(one_by_one));
                        }
                        self.typed(value, item)?;
                    }
                    (None, []) => {
                        if !matches!(**item, Ty::Int(_) | Ty::Bool | Ty::Nullable(_)) {
                            return Err(Diag::error(span, format!("`{item}` has no zero value, so `{built}` alone cannot fill the array"))
                                .note("the zero values are 0 for a number, `false` for a `bool` and `null` for a `T?`")
                                .hint(one_by_one));
                        }
                    }
                    (None, [_, extra, ..]) => {
                        return Err(Diag::error(extra.span, format!("`{built}(...)` takes the one value that every item gets")));
                    }
                }
                Ok(built)
            }
            ExprKind::Null => Ok(Ty::Null),
            ExprKind::Interp(parts) => {
                for part in parts {
                    if let InterpPart::Expr(inner, spec) = part {
                        let ty = self.operand(inner, None)?;
                        self.printable(&ty, inner.span)?;
                        if let Some(Spec { letter: Some((letter, _)), span, .. }) = spec {
                            self.format_letter(&ty, *letter, *span, inner)?;
                        }
                    }
                }
                Ok(Ty::Fmt)
            }
            ExprKind::Unary(UnOp::Neg, operand) => {
                let like = expected.filter(|ty| ty.is_number());
                let found = self.operand(operand, like)?;
                match found.strip_borrow() {
                    Ty::Float(ty) => {
                        let ty = Ty::Float(*ty);
                        self.coerce(operand, found, &ty)?;
                        Ok(ty)
                    }
                    Ty::Int(ty) if ty.is_c() => Err(c_arithmetic(self.src, "-", *ty, &[(&**operand, *ty)], span)),
                    Ty::Int(ty) if !ty.signed() => {
                        let text = &self.src[operand.span.start..operand.span.end];
                        let (name, to) = (ty.name(), signed_holding(*ty));
                        Err(Diag::error(span, format!("operator `-` is not defined for `{name}`: a `{name}` is never negative"))
                            .hint(format!("convert first: `-({to}){text}`")))
                    }
                    Ty::Int(ty) => {
                        let ty = Ty::Int(*ty);
                        self.coerce(operand, found, &ty)?;
                        Ok(ty)
                    }
                    other => Err(Diag::error(span, format!("operator `-` is not defined for `{other}`"))),
                }
            }
            ExprKind::Unary(UnOp::Not, operand) => {
                self.typed(operand, &Ty::Bool)?;
                Ok(Ty::Bool)
            }
            ExprKind::Unary(UnOp::BitNot, operand) => {
                let like = expected.and_then(|ty| match ty {
                    Ty::Int(ty) => Some(*ty),
                    _ => None,
                });
                let found = self.operand(operand, like.map(Ty::Int).as_ref())?;
                match found.strip_borrow() {
                    Ty::Int(ty) if ty.is_c() => Err(c_arithmetic(self.src, "~", *ty, &[(&**operand, *ty)], span)),
                    Ty::Int(ty) => {
                        let ty = Ty::Int(*ty);
                        self.coerce(operand, found, &ty)?;
                        Ok(ty)
                    }
                    Ty::Bool => Err(Diag::error(span, "operator `~` is not defined for `bool`: it turns over the bits of an integer")
                        .hint("a `bool` is turned over with `!`")),
                    Ty::Float(_) => Err(Diag::error(span, format!("operator `~` is not defined for `{found}`: it turns over the bits of an integer"))),
                    other => Err(Diag::error(span, format!("operator `~` is not defined for `{other}`"))),
                }
            }
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), left, right) => {
                // What the left side names or narrows lasts until the chain ends.
                let op = *op;
                self.scopes.push(Scope::default());
                self.link(left, op)?;
                self.link(right, op)?;
                self.pop_scope(true)?;
                Ok(Ty::Bool)
            }
            ExprKind::Binary(op, left, right) => self.binary(*op, left, right, expected, span),
            ExprKind::Cast { ty, expr: operand } => {
                let mut found = self.operand(operand, None)?;
                let mut from = found.strip_borrow().clone();
                // `(byte)block`: the number of its variant, read where the value is; then the
                // cast is one of that integer.
                if let Ty::Enum(owner) = &from {
                    if let Some(numbers) = self.enums.get(owner.as_str()).and_then(|info| info.numbers) {
                        let name_span = operand.span;
                        wrap(operand, |recv| ExprKind::Method { recv, name: NUMBER_OF.to_string(), name_span, args: Vec::new(), callee: Callee::Plain });
                        operand.ty = Ty::Int(numbers);
                        (found, from) = (Ty::Int(numbers), Ty::Int(numbers));
                    }
                }
                // `(nuint)p`: the address, as a number. Nothing is read, so no `unsafe`.
                if let Ty::Raw(_) = &from {
                    if !matches!(&*ty, Ty::Int(IntTy::Nint | IntTy::Nuint)) {
                        let text = &self.src[operand.span.start..operand.span.end];
                        return Err(Diag::error(span, format!("an address is read as a number as wide as an address, and `{ty}` is not one"))
                            .hint(format!("`(nuint){text}`, or `(nint){text}`")));
                    }
                    self.coerce(operand, found, &from)?;
                    return Ok(ty.clone());
                }
                // A `char` is converted to and from integers only: its code.
                let fits = match (&from, &*ty) {
                    (Ty::Char, Ty::Int(_) | Ty::Char) | (Ty::Int(_), Ty::Char) => true,
                    (Ty::Char, _) | (_, Ty::Char) => false,
                    _ => from.is_number(),
                };
                if !fits {
                    let diag = Diag::error(span, format!("a `{from}` value cannot be converted to `{ty}`"));
                    return Err(match (&from, &*ty) {
                        (Ty::Char, Ty::Float(_)) | (Ty::Float(_), Ty::Char) => diag.note("a `char` becomes an integer, its code, and an integer becomes a `char`"),
                        (Ty::Str, Ty::Char) => diag.hint("the characters of a text: `for c in text.chars()`"),
                        (Ty::Char, Ty::Str) => diag.hint("`c.toString()`"),
                        (Ty::Enum(owner), Ty::Int(_)) => diag.note(format!("the variants of `{0}` have no numbers; an enum has them when it names their integer type: `enum {0} : int`", short_name(owner))),
                        _ => diag,
                    });
                }
                self.coerce(operand, found, &from)?;
                if let (ExprKind::Int(value, _), Ty::Char) = (&operand.kind, &*ty) {
                    if char_of(*value).is_none() {
                        return Err(Diag::error(span, format!("{value} is not the code of a character"))
                            .note("a character is from 0 to 0x10FFFF, and not from 0xD800 to 0xDFFF"));
                    }
                }
                // A value that is known here and does not fit is not left to run time.
                if let (ExprKind::Int(value, _), Ty::Int(to)) = (&operand.kind, &*ty) {
                    let (value, to) = (*value, *to);
                    if !to.fits_everywhere(value) {
                        let diag = Diag::error(span, format!("{value} does not fit in {}", to.name()));
                        return Err(match value < 0 && !to.signed() {
                            true => diag.note(format!("a `{}` is never negative", to.name())),
                            false if to.is_c() => c_too_large(diag, to, &self.src[operand.span.start..operand.span.end]),
                            false => diag,
                        });
                    }
                }
                if let (ExprKind::Float(text, _), Ty::Int(to)) = (&operand.kind, &*ty) {
                    let value: f64 = text.parse().expect("the lexer read it");
                    if !float_fits(value, *to) {
                        return Err(Diag::error(span, format!("{text} does not fit in {}", to.name())));
                    }
                }
                Ok(ty.clone())
            }
            ExprKind::Call { name, name_span, args, callee } => self.call(name, *name_span, args, callee, span),
            ExprKind::Method { recv, name, name_span, args, callee } => {
                self.method(recv, name, *name_span, args, callee, span)
            }
            ExprKind::StructLit { name, name_span, fields } => {
                let Some(info) = self.structs.get(name.as_str()) else {
                    let diag = Diag::error(*name_span, format!("there is no struct named `{name}`"));
                    if let Some(hint) = self.names.use_hint(name, span) {
                        return Err(diag.hint(hint));
                    }
                    return Err(match nearest(name, self.structs.keys().map(String::as_str)) {
                        Some(close) => diag.hint(format!("`{close}`")),
                        None => diag,
                    });
                };
                if info.opaque {
                    return Err(Diag::error(*name_span, format!("`{}` is a type of C whose insides SG does not see, so only C makes one", short_name(name)))
                        .note("a function of C gives a `raw` pointer to it"));
                }
                self.built_here(name, info, *name_span, "built field by field")?;
                let mut given = self.field_values(name, info, fields)?;
                if info.shape.union {
                    let short = short_name(name);
                    return match fields.len() {
                        1 => Ok(Ty::Struct(name.clone())),
                        0 => Err(Diag::error(span, format!("a union is built with one of its fields: `{short} {{ {} = ... }}`", info.fields[0].0))
                            .note("the bytes the field does not cover are zero")),
                        _ => Err(Diag::error(fields[1].span, format!("a union is built with one of its fields, and this gives {}", fields.len()))
                            .note("a union holds one field at a time: they all start where it starts")),
                    };
                }
                // A field left out takes its default value, as if it were written here.
                for (position, (field, field_ty)) in info.fields.iter().enumerate() {
                    if given[position] {
                        continue;
                    }
                    let mut value = match &info.defaults[position] {
                        None => continue,
                        Some(FieldDefault::Expr(written)) => written.clone(),
                        Some(FieldDefault::Const(full)) => {
                            let kind = match (self.consts.types.get(full), self.consts.values.get(full)) {
                                (_, Some((_, ConstVal::Str(text)))) => ExprKind::Str(text.clone()),
                                _ => ExprKind::Const(full.clone()),
                            };
                            Expr { kind, span, ty: Ty::Void, moved: false }
                        }
                    };
                    self.typed(&mut value, field_ty)?;
                    given[position] = true;
                    fields.push(FieldInit { name: field.clone(), span, value, index: position });
                }
                let missing: Vec<String> = info
                    .fields
                    .iter()
                    .zip(&given)
                    .filter(|(_, given)| !**given)
                    .map(|((field, _), _)| format!("`{field}`"))
                    .collect();
                if !missing.is_empty() {
                    return Err(Diag::error(span, format!("`{name}` is missing {}", missing.join(", ")))
                        .note("every field gets a value; there is no uninitialized memory"));
                }
                Ok(Ty::Struct(name.clone()))
            }
            // `p with { x = 3 }`: `p` is used as a value (moved, or copied when it can be), and
            // the fields written are given anew; the values they held are dropped there.
            ExprKind::With { base, fields } => {
                let found = self.expr(base, None)?;
                let name = match found.strip_borrow() {
                    Ty::Struct(name) => name.clone(),
                    other => {
                        return Err(Diag::error(base.span, format!("`with` makes a new value of a struct from another, and this is a `{other}`"))
                            .note("a struct is copied with its fields changed: `p with { x = 3 }`"));
                    }
                };
                let info = &self.structs[name.as_str()];
                if found.is_borrow() && !found.strip_borrow().is_copyable(self.copyable) {
                    let text = &self.src[base.span.start..base.span.end];
                    let diag = Diag::error(base.span, format!("`with` takes the value of `{text}`, which is lent here, and a `{}` is not `Copyable`", short_name(&name)));
                    return Err(match info.methods.contains_key("clone") {
                        true => diag.hint(format!("make a copy: `{text}.clone() with {{ ... }}`")),
                        false => diag,
                    });
                }
                self.coerce(base, found, &Ty::Struct(name.clone()))?;
                if info.shape.union {
                    return Err(Diag::error(span, format!("a union is built anew: `{} {{ {} = ... }}`", short_name(&name), info.fields[0].0))
                        .note("a union holds one field at a time"));
                }
                if fields.is_empty() {
                    return Err(Diag::error(span, "`with { }` changes nothing").hint("write at least one field: `p with { x = 3 }`"));
                }
                self.built_here(&name, info, base.span, "copied with `with`")?;
                self.field_values(&name, info, fields)?;
                Ok(Ty::Struct(name))
            }
            ExprKind::New(inner) => {
                let wanted = expected.and_then(|ty| match ty {
                    Ty::Ptr(inner) => Some((**inner).clone()),
                    Ty::Nullable(nullable) => match &**nullable {
                        Ty::Ptr(inner) => Some((**inner).clone()),
                        _ => None,
                    },
                    _ => None,
                });
                let ty = match wanted {
                    Some(wanted) => {
                        self.typed(inner, &wanted)?;
                        wanted
                    }
                    None => self.expr(inner, None)?,
                };
                if matches!(ty, Ty::Void | Ty::Null | Ty::Never) || ty.is_borrow() {
                    return Err(Diag::error(inner.span, format!("`new` needs a value to own, found `{ty}`")));
                }
                // A task on the heap would be waited for by nobody the compiler can see.
                if task_inside(&ty) {
                    return Err(task_kept(inner.span));
                }
                Ok(Ty::Ptr(Box::new(ty)))
            }
            ExprKind::NewShared { value: inner, checked, locked } => {
                let (checked, locked) = (*checked, *locked);
                let unlocked = |ty: &Ty| match ty {
                    Ty::Locked(inner) if locked => Some((**inner).clone()),
                    Ty::Locked(_) => None,
                    other if !locked => Some(other.clone()),
                    _ => None,
                };
                let wanted = expected.and_then(|ty| match ty {
                    Ty::Shared(inner, _) => unlocked(inner),
                    Ty::Nullable(nullable) => match &**nullable {
                        Ty::Shared(inner, _) => unlocked(inner),
                        _ => None,
                    },
                    _ => None,
                });
                let ty = match wanted {
                    Some(wanted) => {
                        self.typed(inner, &wanted)?;
                        wanted
                    }
                    None => self.expr(inner, None)?,
                };
                if matches!(ty, Ty::Void | Ty::Null | Ty::Never) || ty.is_borrow() {
                    return Err(Diag::error(inner.span, format!("`new shared` needs a value to own, found `{ty}`")));
                }
                if task_inside(&ty) {
                    return Err(task_kept(inner.span));
                }
                if let Ty::Shared(..) | Ty::Weak(..) | Ty::Ptr(_) | Ty::Nullable(_) = ty {
                    return Err(Diag::error(inner.span, format!("`new shared` needs a value to own, and this is a `{ty}`"))
                        .note("`new shared` puts a value on the heap with a count of its owners"));
                }
                if lends(&ty, self.names) {
                    return Err(Diag::error(inner.span, format!("a `shared` value cannot hold a `{ty}`, which carries a borrow"))
                        .note("a `shared` value lives as long as its last owner, which can be longer than anything it borrows from"));
                }
                if locked {
                    if let Some((part, at)) = stays(&ty, self.structs, self.enums, self.names, &mut Vec::new()) {
                        let what = if part == ty { String::new() } else { format!(", which holds a `{part}`{at}") };
                        return Err(Diag::error(inner.span, format!("a `locked` value cannot hold a `{ty}`{what}: {}", why_it_stays(&part, self.names)))
                            .note(THREAD_RULE));
                    }
                    return Ok(Ty::Shared(Box::new(Ty::Locked(Box::new(ty))), false));
                }
                Ok(Ty::Shared(Box::new(ty), checked))
            }
            // `atomic 0`: the value as an `atomic T`; a number takes the type that is expected.
            ExprKind::Atomic(inner) => {
                let wanted = match expected.map(Ty::strip_borrow) {
                    Some(Ty::Atomic(item)) => Some((**item).clone()),
                    Some(Ty::Shared(item, _) | Ty::Ptr(item)) => match &**item {
                        Ty::Atomic(item) => Some((**item).clone()),
                        _ => None,
                    },
                    _ => None,
                };
                let ty = match wanted {
                    Some(wanted) => {
                        self.typed(inner, &wanted)?;
                        wanted
                    }
                    None => {
                        let found = self.expr(inner, None)?;
                        let item = found.strip_borrow().clone();
                        if found.is_borrow() && item.is_copyable(self.copyable) {
                            self.coerce(inner, found, &item)?;
                        }
                        item
                    }
                };
                if ty == Ty::Null {
                    return Err(Diag::error(inner.span, "the type of an atomic pointer is not known from `null` alone")
                        .hint("write the type: `atomic raw Node head = atomic null`"));
                }
                atomic_of(ty, span)
            }
            ExprKind::Share(inner) => {
                let found = self.operand(inner, None)?;
                match found.strip_borrow() {
                    Ty::Shared(behind, checked) => Ok(Ty::Shared(behind.clone(), *checked)),
                    Ty::Nullable(nullable) if matches!(**nullable, Ty::Shared(..)) => Ok((**nullable).clone()).map(|ty| Ty::Nullable(Box::new(ty))),
                    // What a `weak` value looks at may be gone: then there is no owner to add.
                    Ty::Weak(behind, checked) => Ok(Ty::Nullable(Box::new(Ty::Shared(behind.clone(), *checked)))),
                    Ty::Nullable(nullable) if matches!(**nullable, Ty::Weak(..)) => {
                        let Ty::Weak(behind, checked) = &**nullable else { unreachable!("matched above") };
                        Ok(Ty::Nullable(Box::new(Ty::Shared(behind.clone(), *checked))))
                    }
                    other => Err(not_shared("share", inner.span, other)),
                }
            }
            ExprKind::WeakOf(inner) => {
                let found = self.operand(inner, None)?;
                match found.strip_borrow() {
                    Ty::Shared(behind, checked) | Ty::Weak(behind, checked) => Ok(Ty::Weak(behind.clone(), *checked)),
                    Ty::Nullable(nullable) if matches!(**nullable, Ty::Shared(..) | Ty::Weak(..)) => {
                        let (Ty::Shared(behind, checked) | Ty::Weak(behind, checked)) = &**nullable else { unreachable!("matched above") };
                        Ok(Ty::Nullable(Box::new(Ty::Weak(behind.clone(), *checked))))
                    }
                    other => Err(not_shared("weak", inner.span, other)),
                }
            }
            ExprKind::Borrow { mutable, expr: inner } => {
                // One way to write it: a range is a `read` window already. `ref` in front of
                // a range of items asks for a window that can be written through.
                if let ExprKind::Slice { recv, from, to } = &mut inner.kind {
                    let (window, writable) = self.range(recv, from, to)?;
                    let text = &self.src[inner.span.start..inner.span.end];
                    let Ty::Read(behind) = window else { unreachable!("a range is a `read` window") };
                    if !*mutable {
                        return Err(Diag::error(span, "a range is already a borrow, so `read` is not written in front of it").hint(format!("`{text}`")));
                    }
                    if *behind == Ty::Str {
                        return Err(Diag::error(span, "a range is already a borrow, so `ref` is not written in front of it")
                            .note("a range of a `string` is a `read string`; text is never written through"));
                    }
                    if let Some(diag) = self.readonly(recv) {
                        return Err(diag.note("so a range of it cannot be lent as `ref` either"));
                    }
                    if !writable {
                        return Err(self.read_only(recv, Diag::error(recv.span, "this is reached through a `read` borrow, so a range of it cannot be lent as `ref`")));
                    }
                    self.not_locked(recv, Access::Write)?;
                    // Kept as the range alone; its type says that it can be written through.
                    let ty = Ty::Ref(behind);
                    inner.ty = ty.clone();
                    let range = std::mem::replace(&mut inner.kind, ExprKind::Null);
                    expr.kind = range;
                    return Ok(ty);
                }
                let (ty, writable) = match self.place(inner)? {
                    Some(place) => place,
                    None if *mutable => {
                        return Err(Diag::error(inner.span, "`ref` needs a variable or a field to write to"));
                    }
                    None => (self.expr(inner, None)?, false),
                };
                if Expr::in_raw_memory(inner) {
                    let text = &self.src[inner.span.start..inner.span.end];
                    let diag = Diag::error(span, "a place in raw memory is not lent yet: the checker cannot follow how long it lives");
                    return Err(match mutable {
                        true => diag.hint(format!("write it where it is: `unsafe {{ {text} = ... }}`")),
                        false => diag.hint(format!("copy it out: `var item = unsafe {text}`")),
                    });
                }
                if let Some(diag) = not_lent(inner, self.structs, self.src) {
                    return Err(diag);
                }
                if matches!(ty, Ty::Void | Ty::Never) {
                    return Err(Diag::error(inner.span, "there is no value here to borrow"));
                }
                let base = Box::new(ty.strip_borrow().clone());
                if !*mutable {
                    return Ok(Ty::Read(base));
                }
                if let Some(diag) = self.readonly(inner) {
                    return Err(diag.note("so it cannot be lent as `ref` either"));
                }
                if !writable {
                    return Err(self.read_only(inner, Diag::error(inner.span, "this is reached through a `read` borrow and cannot be lent as `ref`")));
                }
                self.not_locked(inner, Access::Write)?;
                Ok(Ty::Ref(base))
            }
            ExprKind::IsNull { expr: inner, negated } => {
                let ty = self.operand(inner, None)?;
                if matches!(ty.strip_borrow(), Ty::Raw(_)) {
                    let text = &self.src[inner.span.start..inner.span.end];
                    let op = if *negated { "!=" } else { "==" };
                    return Err(Diag::error(span, "a raw pointer is not a `T?`: it is an address, compared with `null`").hint(format!("`{text} {op} null`")));
                }
                if !matches!(ty.strip_borrow(), Ty::Nullable(_) | Ty::Null) {
                    return Err(Diag::error(inner.span, format!("this is `{ty}`, which is never `null`")).note("only a `T?` can be `null`"));
                }
                Ok(Ty::Bool)
            }
            ExprKind::Coalesce { left, right, on_result } => {
                let nullable = expected.filter(|ty| !ty.is_borrow()).map(|ty| Ty::Nullable(Box::new(ty.clone())));
                let mut found = self.expr(left, nullable.as_ref())?;
                // A borrowed `T?` that can be copied stands in for the `T?` itself.
                if let Ty::Read(behind) | Ty::Ref(behind) = &found {
                    if matches!(**behind, Ty::Nullable(_)) && behind.is_copyable(self.copyable) {
                        let behind = (**behind).clone();
                        wrap(left, ExprKind::Load);
                        // The back end reads the type of what it opens.
                        left.ty = behind.clone();
                        found = behind;
                    }
                }
                // On a `Result` the error is dropped and the right side stands in.
                if let Ty::Result(ok, _) = &found {
                    if **ok == Ty::Void {
                        return Err(Diag::error(left.span, format!("this is `{found}`: success carries no value, so `??` has nothing to give"))
                            .hint("pass the error on with `try`, or drop it on purpose with `_ = ...`"));
                    }
                    *on_result = true;
                    self.fallback(right, ok)?;
                    return Ok((**ok).clone());
                }
                if matches!(found.strip_borrow(), Ty::Result(..)) {
                    return Err(Diag::error(left.span, format!("`??` takes the value out of the `Result`, and this `{found}` is only borrowed"))
                        .hint("look at it with `match` or `is`"));
                }
                let Some(inner) = without_null(&found) else {
                    return Err(Diag::error(left.span, format!("this is `{found}`, which is never `null`, so `??` has nothing to do")));
                };
                self.fallback(right, &inner)?;
                Ok(inner)
            }
            ExprKind::Try { expr: inner, kind } => self.try_expr(inner, kind, span),
            ExprKind::Await(inner) => self.await_expr(inner, span),
            ExprKind::Return(value) => {
                self.returned(value.as_deref_mut(), span)?;
                Ok(Ty::Never)
            }
            ExprKind::IsVariant { .. } => {
                self.is_variant(expr, NamesSeen::No, false)?;
                Ok(Ty::Bool)
            }
            ExprKind::Match { subject, arms, borrowed } => self.match_expr(subject, arms, borrowed, expected, false),
            ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. } => unreachable!("places are handled above"),
            ExprKind::Const(full) => Ok(self.consts.use_type(full)),
            ExprKind::Static(full) => self.static_use(full, expr.span),
            ExprKind::Embed(index) => match self.in_const {
                true => Ok(Ty::Array(Box::new(Ty::Int(IntTy::Byte)), self.consts.embeds[*index])),
                false => Err(Diag::error(span, "`embed` is written where a const is given its value")
                    .hint("`const byte[] Logo = embed(\"logo.gif\")`, then `Logo` where the bytes are needed")),
            },
            // The address of a variable of C: making it touches nothing.
            ExprKind::CVar(name) => Ok(Ty::Raw(Box::new(self.names.cvar_types[name.as_str()].clone()))),
            ExprKind::Widen(..) | ExprKind::Load(_) | ExprKind::Unbox(_) | ExprKind::AsSlice(_) | ExprKind::Variant { .. } | ExprKind::FuncRef(_) | ExprKind::Constant { .. } => {
                unreachable!("inserted only after checking")
            }
        }
    }

    /// `x[a..b]`: a window onto a text, a list, an array or a slice. Gives its type (as a
    /// `read` window) and whether the same range could be lent as `ref`.
    fn range(&mut self, recv: &mut Expr, from: &mut Option<Box<Expr>>, to: &mut Option<Box<Expr>>) -> Res<(Ty, bool)> {
        let (ty, mut writable, is_place) = match self.place(recv)? {
            Some((ty, writable)) => (ty, writable, true),
            None => (self.expr(recv, None)?, true, false),
        };
        // A range lends the items where they lie: not inside a `packed` struct, where they may
        // lie where their type cannot be reached.
        if is_place {
            if let Some(diag) = not_lent(recv, self.structs, self.src) {
                return Err(diag);
            }
        }
        let mut inner = &ty;
        loop {
            inner = match inner {
                Ty::Read(behind) => {
                    writable = false;
                    behind
                }
                // A `ref` reached through a `read` borrow only reads.
                Ty::Ref(behind) => behind,
                Ty::Ptr(behind) => behind,
                Ty::Locked(_) => return Err(self.locked_outside(recv.span)),
                Ty::Shared(behind, checked) => {
                    writable = *checked;
                    behind
                }
                _ => break,
            };
        }
        let window = match inner {
            Ty::Str => Ty::Str,
            Ty::List(item) | Ty::Array(item, _) | Ty::Slice(item) => Ty::Slice(item.clone()),
            _ => {
                self.indexable(&ty, recv.span)?;
                unreachable!("anything a range can be taken of is handled above")
            }
        };
        if let Some(from) = from {
            self.index(from, "the start of the range")?;
        }
        if let Some(to) = to {
            self.index(to, "the end of the range")?;
        }
        Ok((Ty::Read(Box::new(window)), writable && (is_place || ty.is_borrow())))
    }

    /// The message for a value that `x[i]` or `x[a..b]` cannot be written on.
    fn indexable(&self, ty: &Ty, span: Span) -> Res<()> {
        match ty.strip_borrow() {
            Ty::Str => Ok(()),
            Ty::Nullable(_) => Err(Diag::error(span, format!("this is `{ty}` and may be `null`; an index needs a value"))
                .hint("test it first with `is not null`, or give a fallback with `??`")),
            Ty::Fmt => Err(Diag::error(span, "`$\"...\"` is not a `string` and cannot be indexed").hint("add `.toString()` first")),
            other => Err(Diag::error(span, format!("`{other}` cannot be indexed"))
                .note("an index is written on a `string` (one byte), a `List`, an array, a slice and a `HashMap`")),
        }
    }

    /// An index or one end of a range: any integer that widens to `nint`.
    fn index(&mut self, expr: &mut Expr, what: &str) -> Res<()> {
        let ty = self.int_operand(expr, Some(IntTy::Nint), what)?;
        self.coerce(expr, Ty::Int(ty), &Ty::Int(IntTy::Nint))
    }

    // ---- enums ----

    /// The enum an expression names when it is written in front of a dot: `Shape` in
    /// `Shape.Circle(2)`. A variable of that name wins.
    /// The number type an expression names when it is written in front of a dot: `int` in
    /// `int.MaxValue` and in `int.parse(text)`. A variable of that name wins.
    fn number_type_named(&mut self, expr: &Expr) -> Option<Ty> {
        let ExprKind::Var(name) = &expr.kind else { return None };
        if self.find(name).is_some() {
            return None;
        }
        Ty::from_name(name).filter(Ty::is_number)
    }

    /// `CancellationSource` or `CancellationToken` written in front of a dot. A variable of
    /// that name wins.
    fn cancel_type_named(&mut self, expr: &Expr) -> Option<Ty> {
        let ExprKind::Var(name) = &expr.kind else { return None };
        if self.find(name).is_some() {
            return None;
        }
        Ty::from_name(name).filter(|ty| matches!(ty, Ty::Source | Ty::Token))
    }

    fn enum_named(&mut self, expr: &Expr) -> Option<&'a str> {
        let ExprKind::Var(name) = &expr.kind else { return None };
        if name == "this" || self.find(name).is_some() {
            return None;
        }
        self.enums.get_key_value(name.as_str()).map(|(key, _)| key.as_str())
    }

    /// `Shape.Circle(2)` or `Shape.Empty`. `args` is `None` when no parentheses were written.
    fn variant(&mut self, owner: &str, name: &str, name_span: Span, args: Option<&mut Vec<Expr>>, span: Span) -> Res<(usize, Ty)> {
        let enums = self.enums;
        let variants = &enums[owner].variants;
        let Some(position) = variants.iter().position(|(variant, _)| variant == name) else {
            let diag = Diag::error(name_span, format!("`{owner}` has no variant `{name}`"));
            return Err(match nearest(name, variants.iter().map(|(variant, _)| variant.as_str())) {
                Some(close) => diag.hint(format!("`{close}`")),
                None => diag,
            });
        };
        let fields = &variants[position].1;
        let names = fields.iter().map(|(field, _)| field.as_str()).collect::<Vec<_>>().join(", ");
        match args {
            None if !fields.is_empty() => {
                return Err(Diag::error(span, format!("`{name}` carries {} value(s) and cannot be built without them", fields.len()))
                    .hint(format!("`{owner}.{name}({names})`")));
            }
            None => {}
            Some(_) if fields.is_empty() => {
                return Err(Diag::error(span, format!("`{name}` carries nothing, so it is written without parentheses"))
                    .hint(format!("`{owner}.{name}`")));
            }
            Some(args) if args.len() != fields.len() => {
                return Err(Diag::error(span, format!("`{owner}.{name}` takes {} value(s) ({names}), {} given", fields.len(), args.len())));
            }
            Some(args) => {
                for (arg, (_, ty)) in args.iter_mut().zip(fields) {
                    self.typed(arg, ty)?;
                }
            }
        }
        Ok((position, Ty::Enum(owner.to_string())))
    }

    /// `Ok(value)`, `Ok()` or `Error(cause)`. The type comes from where the value goes, as
    /// for `null`: `Ok(1)` alone does not say what the error type is.
    fn result_value(&mut self, is_ok: bool, args: &mut [Expr], expected: Option<&Ty>, span: Span) -> Res<(usize, Ty)> {
        let word = if is_ok { "Ok" } else { "Error" };
        let target = expected.map(|ty| match ty {
            Ty::Nullable(inner) => &**inner,
            other => other,
        });
        let (ok, error) = match target {
            Some(Ty::Result(ok, error)) => ((**ok).clone(), (**error).clone()),
            Some(other) => {
                return Err(Diag::error(span, format!("expected `{other}`, found a `Result`"))
                    .note(format!("`{word}(...)` builds a `Result<T, E>`")));
            }
            None => {
                return Err(Diag::error(span, format!("the type of this `Result` cannot be found from `{word}(...)` alone"))
                    .hint(format!("write the type where the value goes, for example `Result<int, MyError> x = {word}(...)`")));
            }
        };
        match (is_ok, args) {
            (true, []) if ok == Ty::Void => {}
            (true, _) if ok == Ty::Void => {
                return Err(Diag::error(span, format!("success carries no value in `Result<void, {error}>`")).hint("`Ok()`"));
            }
            (true, [value]) => self.typed(value, &ok)?,
            (false, [cause]) => self.typed(cause, &error)?,
            (true, args) => {
                return Err(Diag::error(span, format!("`Ok` carries one value, the `{ok}`; {} given", args.len())));
            }
            (false, args) => {
                return Err(Diag::error(span, format!("`Error` carries one value, the `{error}`; {} given", args.len())));
            }
        }
        Ok((if is_ok { 0 } else { 1 }, Ty::Result(Box::new(ok), Box::new(error))))
    }

    /// `try e`: on a `Result` gives the value or returns the error to the caller; on a `T?`
    /// gives the value or returns `null`.
    fn try_expr(&mut self, inner: &mut Expr, kind: &mut TryKind, span: Span) -> Res<Ty> {
        let mut found = self.expr(inner, None)?;
        // A `read T?` that holds a value gives a `read T`, as `??` does: nothing is taken out.
        if let Ty::Read(behind) = &found {
            if let (Ty::Nullable(value), false, false) = (&**behind, behind.is_copyable(self.copyable), self.in_test) {
                if matches!(self.ret.strip_borrow(), Ty::Nullable(_)) {
                    *kind = TryKind::Null;
                    return Ok(Ty::Read(value.clone()));
                }
            }
        }
        if let Ty::Read(behind) | Ty::Ref(behind) = &found {
            if matches!(**behind, Ty::Result(..) | Ty::Nullable(_)) {
                if !behind.is_copyable(self.copyable) {
                    return Err(Diag::error(inner.span, format!("`try` takes the value out, and this `{found}` is only borrowed"))
                        .hint("look at it with `match` or `is`"));
                }
                let behind = (**behind).clone();
                wrap(inner, ExprKind::Load);
                inner.ty = behind.clone();
                found = behind;
            }
        }
        let func = self.func_name.clone();
        match found {
            // In a test an error of any type ends the test and is printed.
            Ty::Result(ok, error) if self.in_test => {
                let written = match &*error {
                    Ty::Struct(name) | Ty::Enum(name) => self.formattable.contains(name),
                    other => can_format(other, self.formattable),
                };
                if !written {
                    let diag = Diag::error(span, format!("an error that fails a test is printed, so `{error}` must be `Formattable`"));
                    return Err(match &*error {
                        Ty::Struct(name) | Ty::Enum(name) => diag.hint(format!("add it to the definition: `{name} : Formattable`")),
                        _ => diag,
                    });
                }
                *kind = TryKind::Result(None);
                Ok(*ok)
            }
            Ty::Result(ok, error) => {
                let Ty::Result(_, target) = &self.ret else {
                    return Err(Diag::error(span, format!("`try` returns the error to the caller, but `{func}` returns `{}`", self.ret))
                        .hint(format!("make `{func}` return a `Result<..., {error}>`, or handle the error here with `match` or `??`")));
                };
                let convert = if **target == *error { None } else { Some(self.error_variant(&error, target, span)?) };
                *kind = TryKind::Result(convert);
                Ok(*ok)
            }
            // `assert` does not narrow: `?? panic(...)` is what fails the test and gives the value.
            Ty::Nullable(value) if self.in_test => {
                let text = self.src[inner.span.start..inner.span.end].trim();
                let text = if text.contains('\n') { "..." } else { text };
                Err(Diag::error(span, format!("`try` on `{value}?` returns `null` to the caller, and a test has no caller"))
                    .hint(format!("say what is expected, which fails the test without it: `{text} ?? panic(\"...\")`; or give a fallback with `??`")))
            }
            Ty::Nullable(value) => {
                if !matches!(self.ret, Ty::Nullable(_)) {
                    let diag = Diag::error(span, format!("`try` on `{value}?` returns `null` to the caller, but `{func}` returns `{}`", self.ret));
                    return Err(match &self.ret {
                        Ty::Result(..) => diag.hint("say which error a missing value is: `... ?? return Error(...)`"),
                        _ => diag.hint("give a fallback with `??`, or test with `is null`"),
                    });
                }
                *kind = TryKind::Null;
                Ok(*value)
            }
            other => Err(Diag::error(inner.span, format!("`try` needs a `Result` or a `T?`, found `{other}`"))
                .note("there is no error here to pass on")),
        }
    }

    /// The variant of the caller's error type that `try` wraps a different error in: the one
    /// whose only field is that error.
    fn error_variant(&self, error: &Ty, target: &Ty, span: Span) -> Res<(String, usize)> {
        let mismatch = Diag::error(span, format!("`try` cannot turn `{error}` into `{target}`"));
        let Ty::Enum(name) = target else {
            return Err(mismatch.note(format!("this function returns errors of the type `{target}`")));
        };
        let variants = &self.enums[name].variants;
        let fits: Vec<usize> = (0..variants.len()).filter(|i| matches!(variants[*i].1.as_slice(), [(_, ty)] if ty == error)).collect();
        match fits.as_slice() {
            [index] => Ok((name.clone(), *index)),
            [] => Err(mismatch.hint(format!("add a variant to `{name}` that carries it: `Name({error} cause)`"))),
            _ => {
                let names: Vec<String> = fits.iter().map(|i| format!("`{}`", variants[*i].0)).collect();
                Err(Diag::error(span, format!("`try` cannot choose: {} of `{name}` both carry `{error}`", names.join(" and ")))
                    .hint("open the `Result` with `match` and build the one you mean"))
            }
        }
    }

    /// The enum a `match` or `is` looks at, behind pointers and borrows, and how the value
    /// is held: owned (`None`) or through a `read` or `ref` borrow. A `Result` counts as an
    /// enum with the variants `Ok` and `Error`.
    fn enum_behind(&self, ty: &Ty, span: Span, what: &str) -> Res<(Variants, Option<Mode>)> {
        let mut mode = None;
        let mut inner = ty;
        loop {
            inner = match inner {
                Ty::Read(behind) => {
                    mode = Some(Mode::Read);
                    behind
                }
                Ty::Ref(behind) => {
                    mode = mode.or(Some(Mode::Ref));
                    behind
                }
                Ty::Ptr(behind) => behind,
                Ty::Locked(_) => return Err(self.locked_outside(span)),
                Ty::Shared(behind, true) => {
                    mode = mode.or(Some(Mode::Read));
                    behind
                }
                Ty::Shared(behind, false) => {
                    mode = Some(Mode::Read);
                    behind
                }
                Ty::Enum(name) => {
                    let list = self.enums[name].variants.clone();
                    return Ok((Variants { owner: name.clone(), list }, mode));
                }
                Ty::Result(ok, error) => {
                    let value = if **ok == Ty::Void { Vec::new() } else { vec![("value".to_string(), (**ok).clone())] };
                    let list = vec![("Ok".to_string(), value), ("Error".to_string(), vec![("cause".to_string(), (**error).clone())])];
                    return Ok((Variants { owner: inner.to_string(), list }, mode));
                }
                Ty::Nullable(_) => {
                    return Err(Diag::error(span, format!("this is `{ty}` and may be `null`; {what} needs a value"))
                        .hint("test it first with `is not null`, or give a fallback with `??`"));
                }
                _ => {
                    let diag = Diag::error(span, format!("{what} needs a value of an enum, found `{ty}`"));
                    return Err(match inner {
                        Ty::Int(_) => diag.hint("a number is compared with `==`, or matched with `match`"),
                        Ty::Char => diag.hint("a character is compared with `==`, or matched with `match`"),
                        Ty::Str => diag.hint("a text is compared with `==`, or matched with `match`: `match word { \"if\" => ... }`"),
                        _ if what == "`match`" => diag.note("a `match` looks at an enum, an integer, a `char` or a text"),
                        _ => diag,
                    });
                }
            };
        }
    }

    /// Resolves a pattern against the enum and gives the names it declares. With `mode` the
    /// names borrow from the value; without, they take what the value holds.
    fn pattern_binds(&mut self, pattern: &mut Pattern, of: &Variants, mode: Option<Mode>, subject: &Ty) -> Res<Binds> {
        let span = pattern.span;
        match &pattern.kind {
            PatternKind::Wildcard => Ok(Vec::new()),
            PatternKind::Bind(name) => Ok(vec![(name.clone(), subject.clone(), span)]),
            PatternKind::Int(_) | PatternKind::Char(_) | PatternKind::Str(_) | PatternKind::Range(..) | PatternKind::Bool(_) => {
                Err(Diag::error(span, format!("`{}` is an enum; a pattern here names one of its variants", of.owner)))
            }
            PatternKind::Null => Err(Diag::error(span, format!("a `{subject}` is never `null`"))
                .note("only a value of a type written with `?` can be `null`")),
            PatternKind::Variant { .. } => {
                let mut binds = Vec::new();
                self.variant_pattern(pattern, of, mode, &mut binds)?;
                Ok(binds)
            }
            PatternKind::Or(_) => {
                let PatternKind::Or(alternatives) = &mut pattern.kind else { unreachable!("just matched") };
                let mut all = Vec::new();
                for alternative in alternatives.iter_mut() {
                    all.push(self.pattern_binds(alternative, of, mode, subject)?);
                }
                same_names(alternatives, all)
            }
        }
    }

    /// `Circle(r)`, `Add(Number(a), _)`: the variant is one of the enum's, and the patterns
    /// in the parentheses name, skip or test each value it carries.
    fn variant_pattern(&mut self, pattern: &mut Pattern, of: &Variants, mode: Option<Mode>, binds: &mut Binds) -> Res<()> {
        let span = pattern.span;
        let PatternKind::Variant { name, args, index } = &mut pattern.kind else {
            unreachable!("only called for a variant");
        };
        let (owner, variants) = (&of.owner, &of.list);
        let Some(position) = variants.iter().position(|(variant, _)| variant == name) else {
            let diag = Diag::error(span, format!("`{owner}` has no variant `{name}`"));
            return Err(match nearest(name, variants.iter().map(|(variant, _)| variant.as_str())) {
                Some(close) => diag.hint(format!("`{close}`")),
                None => diag.hint(format!("to take any value under this name, write `var {name}`")),
            });
        };
        *index = position;
        let fields = &variants[position].1;
        let names = fields.iter().map(|(field, _)| field.as_str()).collect::<Vec<_>>().join(", ");
        let Some(args) = args else {
            if fields.is_empty() {
                return Ok(());
            }
            let blanks = vec!["_"; fields.len()].join(", ");
            return Err(Diag::error(span, format!("`{name}` carries {} value(s); the pattern must name or skip each", fields.len()))
                .hint(format!("`{name}({names})`, or `{name}({blanks})` to skip them")));
        };
        if fields.is_empty() {
            return Err(Diag::error(span, format!("`{name}` carries nothing, so it is written without parentheses"))
                .hint(format!("`{name}`")));
        }
        if args.len() != fields.len() {
            return Err(Diag::error(span, format!("`{name}` carries {} value(s), the pattern has {}", fields.len(), args.len()))
                .hint(format!("`{name}({names})`; write `_` for a value that is not needed")));
        }
        for (arg, (_, ty)) in args.iter_mut().zip(fields) {
            self.inner_pattern(arg, ty, mode, binds)?;
        }
        Ok(())
    }

    /// A pattern inside the parentheses of a variant, for a value of type `ty` that is held
    /// as `mode` says. A name alone takes the value, unless it is a variant of the enum there:
    /// then it is that variant (`Paint(Red)`).
    fn inner_pattern(&mut self, pattern: &mut Pattern, ty: &Ty, mode: Option<Mode>, binds: &mut Binds) -> Res<()> {
        let span = pattern.span;
        let written = &self.src[span.start..span.end];
        match &mut pattern.kind {
            PatternKind::Or(alternatives) => {
                let mut all = Vec::new();
                for alternative in alternatives.iter_mut() {
                    let mut found = Vec::new();
                    self.inner_pattern(alternative, ty, mode, &mut found)?;
                    all.push(found);
                }
                binds.extend(same_names(alternatives, all)?);
                return Ok(());
            }
            PatternKind::Null => {
                return match ty.strip_borrow() {
                    Ty::Nullable(_) => Ok(()),
                    _ => Err(Diag::error(span, format!("this value is a `{ty}`, and a `{ty}` is never `null`"))
                        .note("only a value of a type written with `?` can be `null`")),
                };
            }
            PatternKind::Wildcard | PatternKind::Bind(_) => {}
            // Any other pattern takes a value that is not `null`: it looks at the value.
            _ if matches!(ty.strip_borrow(), Ty::Nullable(_)) => {
                let held = match ty {
                    Ty::Read(_) => Some(Mode::Read),
                    Ty::Ref(_) => Some(Mode::Ref),
                    _ => None,
                };
                let mode = match (mode, held) {
                    (Some(Mode::Read), _) | (_, Some(Mode::Read)) => Some(Mode::Read),
                    (Some(Mode::Ref), _) | (_, Some(Mode::Ref)) => Some(Mode::Ref),
                    _ => None,
                };
                let Ty::Nullable(inner) = ty.strip_borrow() else { unreachable!("just tested") };
                return self.inner_pattern(pattern, inner, mode, binds);
            }
            PatternKind::Bool(_) => {
                return match ty.strip_borrow() {
                    Ty::Bool => Ok(()),
                    _ => Err(Diag::error(span, format!("this value is a `{ty}`, and `{written}` is a `bool`"))),
                };
            }
            PatternKind::Range(..) => {
                return match ty.strip_borrow() {
                    Ty::Int(int) => self.int_pattern(pattern, *int, ty).map(drop),
                    Ty::Char => char_pattern(pattern, ty).map(drop),
                    _ => Err(Diag::error(span, format!("this value is a `{ty}`, and `{written}` is a range of numbers or characters"))),
                };
            }
            _ => {}
        }
        let inner_enum = match peel(ty) {
            Ty::Enum(_) | Ty::Result(..) => self.enum_behind(ty, span, "a pattern").ok(),
            _ => None,
        };
        if let PatternKind::Bind(name) = &pattern.kind {
            let variant = inner_enum.as_ref().is_some_and(|(of, _)| of.list.iter().any(|(variant, _)| variant == name));
            if variant {
                pattern.kind = PatternKind::Variant { name: name.clone(), args: None, index: 0 };
            } else if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                // A capital letter is where a variant was meant: `Empy` stays an error.
                return Err(match &inner_enum {
                    Some((of, _)) => {
                        let diag = Diag::error(span, format!("`{}` has no variant `{name}`", of.owner));
                        match nearest(name, of.list.iter().map(|(variant, _)| variant.as_str())) {
                            Some(close) => diag.hint(format!("`{close}`")),
                            None => diag.note("a name that takes the value starts with a small letter"),
                        }
                    }
                    None => Diag::error(span, format!("this value is a `{ty}`, and `{name}` is not a variant of it"))
                        .note("a name that takes the value starts with a small letter"),
                });
            }
        }
        let written = &self.src[span.start..span.end];
        match &mut pattern.kind {
            PatternKind::Or(_) | PatternKind::Null | PatternKind::Bool(_) | PatternKind::Range(..) => unreachable!("handled above"),
            PatternKind::Wildcard => Ok(()),
            PatternKind::Bind(name) => {
                // A value that is a borrow already (`Ok(text)` of a `Result<read string, E>`)
                // is that borrow, not a borrow of it; under `read` a `ref` is seen as `read`.
                let ty = match (mode, ty) {
                    (None, _) | (Some(_), Ty::Read(_)) | (Some(Mode::Ref), Ty::Ref(_)) => ty.clone(),
                    (Some(Mode::Read), Ty::Ref(inner)) => Ty::Read(inner.clone()),
                    (Some(Mode::Read), _) => Ty::Read(Box::new(ty.clone())),
                    (Some(Mode::Ref), _) => Ty::Ref(Box::new(ty.clone())),
                };
                binds.push((name.clone(), ty, span));
                Ok(())
            }
            PatternKind::Int(_) | PatternKind::Char(_) | PatternKind::Str(_) => {
                let kind = match &pattern.kind {
                    PatternKind::Int(_) => "a number",
                    PatternKind::Char(_) => "a character",
                    _ => "a text",
                };
                let mismatch = || Diag::error(span, format!("this value is a `{ty}`, and `{written}` is {kind}"));
                match (ty.strip_borrow(), &pattern.kind) {
                    (Ty::Int(int), PatternKind::Int(_) | PatternKind::Char(_)) => {
                        self.int_pattern(pattern, *int, ty)?;
                    }
                    (Ty::Char, PatternKind::Char(_)) => {
                        char_pattern(pattern, ty)?;
                    }
                    (Ty::Str, PatternKind::Str(_)) => {}
                    (Ty::Char, PatternKind::Int(value)) => {
                        let diag = mismatch();
                        return Err(match char_of(*value) {
                            Some(found) if !found.is_control() => diag.hint(format!("a character is written in quotes: `'{found}'`")),
                            _ => diag,
                        });
                    }
                    _ => return Err(mismatch()),
                }
                Ok(())
            }
            PatternKind::Variant { .. } => {
                let Some((of, held)) = inner_enum else {
                    return Err(Diag::error(span, format!("this value is a `{ty}`, and `{written}` names a variant of an enum")));
                };
                // Through a borrow the names borrow too; `read` anywhere makes them `read`.
                let mode = match (mode, held) {
                    (Some(Mode::Read), _) | (_, Some(Mode::Read)) => Some(Mode::Read),
                    (Some(Mode::Ref), _) | (_, Some(Mode::Ref)) => Some(Mode::Ref),
                    _ => None,
                };
                self.variant_pattern(pattern, &of, mode, binds)
            }
        }
    }

    /// The pattern as the search for missing values sees it (`exhaust`), for a value of `ty`.
    fn search_pattern(&self, pattern: &Pattern, ty: &Ty) -> exhaust::Pat {
        use exhaust::Pat;
        let core = ty.strip_borrow();
        if let Ty::Nullable(inner) = core {
            let shape = Rc::new(exhaust::Enum { names: vec!["null".to_string(), String::new()], arities: vec![0, 1] });
            return match &pattern.kind {
                PatternKind::Wildcard | PatternKind::Bind(_) => Pat::Any,
                PatternKind::Null => Pat::Variant { of: shape, index: 0, args: Vec::new() },
                PatternKind::Or(alternatives) => Pat::Or(alternatives.iter().map(|alternative| self.search_pattern(alternative, ty)).collect()),
                _ => Pat::Variant { of: shape, index: 1, args: vec![self.search_pattern(pattern, inner)] },
            };
        }
        let number = |low: i128, high: i128| {
            let of = match peel(core) {
                // C's `long` as wide as it is anywhere, so that a `match` covers it on every system.
                Ty::Int(int) => {
                    let widest = if int.is_c() { c_partner(*int) } else { *int };
                    exhaust::Domain { name: int.name().to_string(), ranges: vec![(widest.min(), widest.max())], chars: false }
                }
                _ => exhaust::Domain { name: "char".to_string(), ranges: vec![(0, 0xd7ff), (0xe000, 0x10ffff)], chars: true },
            };
            Pat::Num { low, high, of: Rc::new(of) }
        };
        match &pattern.kind {
            PatternKind::Wildcard | PatternKind::Bind(_) => Pat::Any,
            PatternKind::Int(value) => number(*value, *value),
            PatternKind::Char(value) => number(*value as i128, *value as i128),
            PatternKind::Range(low, high) => match (&low.kind, &high.kind) {
                (PatternKind::Int(low), PatternKind::Int(high)) => number(*low, *high),
                _ => unreachable!("the ends of a range are numbers once checked"),
            },
            PatternKind::Str(text) => Pat::Text(text.clone()),
            PatternKind::Bool(value) => {
                let shape = Rc::new(exhaust::Enum { names: vec!["false".to_string(), "true".to_string()], arities: vec![0, 0] });
                Pat::Variant { of: shape, index: *value as usize, args: Vec::new() }
            }
            PatternKind::Null => unreachable!("the checker allows `null` only for a `T?`"),
            PatternKind::Or(alternatives) => Pat::Or(alternatives.iter().map(|alternative| self.search_pattern(alternative, ty)).collect()),
            PatternKind::Variant { index, args, .. } => {
                let (of, _) = self.enum_behind(ty, pattern.span, "a pattern").expect("the pattern was checked against the type");
                let shape = Rc::new(exhaust::Enum {
                    names: of.list.iter().map(|(name, _)| name.clone()).collect(),
                    arities: of.list.iter().map(|(_, fields)| fields.len()).collect(),
                });
                let fields = &of.list[*index].1;
                let args = match args {
                    None => vec![Pat::Any; fields.len()],
                    Some(args) => args.iter().zip(fields).map(|(arg, (_, ty))| self.search_pattern(arg, ty)).collect(),
                };
                Pat::Variant { of: shape, index: *index, args }
            }
        }
    }

    /// A pattern in a `match` on an integer: a number or a character that fits the type,
    /// `_`, or `var name`.
    fn int_pattern(&mut self, pattern: &mut Pattern, ty: IntTy, subject: &Ty) -> Res<Binds> {
        let span = pattern.span;
        let value = match &mut pattern.kind {
            PatternKind::Wildcard => return Ok(Vec::new()),
            PatternKind::Bind(name) => return Ok(vec![(name.clone(), subject.clone(), span)]),
            PatternKind::Int(value) => *value,
            PatternKind::Char(value) => byte_of(*value, span)?,
            PatternKind::Or(alternatives) => {
                let mut all = Vec::new();
                for alternative in alternatives.iter_mut() {
                    all.push(self.int_pattern(alternative, ty, subject)?);
                }
                return same_names(alternatives, all);
            }
            PatternKind::Range(low, high) => {
                let shown = (written_end(low), written_end(high));
                self.int_pattern(low, ty, subject)?;
                self.int_pattern(high, ty, subject)?;
                return range_order(low, high, shown, span).map(|_| Vec::new());
            }
            PatternKind::Null => return Err(Diag::error(span, format!("a `{}` is never `null`", ty.name()))
                .note("only a value of a type written with `?` can be `null`")),
            PatternKind::Bool(value) => {
                return Err(Diag::error(span, format!("this `match` looks at a `{}`, and `{value}` is not a number", ty.name())));
            }
            PatternKind::Str(_) => {
                return Err(Diag::error(span, format!("this `match` looks at a `{}`, and a text is not a number", ty.name())));
            }
            PatternKind::Variant { name, args, .. } => {
                let diag = Diag::error(span, format!("this `match` looks at a `{}`, and `{name}` is not a number or a character", ty.name()));
                return Err(match args {
                    None => diag.hint(format!("to take any value under this name, write `var {name}`")),
                    Some(_) => diag,
                });
            }
        };
        if !ty.fits_everywhere(value) {
            return Err(match ty.is_c() {
                true => Diag::error(span, format!("{value} does not fit in {} on every system", ty.name()))
                    .note(format!("C's `{}` is 32 bits on Windows, so a number written for it fits in 32 bits on every system", c_spelling(ty))),
                false => Diag::error(span, format!("{value} does not fit in {}, so this arm can never run", ty.name())),
            });
        }
        pattern.kind = PatternKind::Int(value);
        Ok(Vec::new())
    }

    /// A name in a pattern that names a const stands for what the const holds: `Trailer` in
    /// `match tag { Trailer => ... }`. A variant of the enum looked at, of that name, comes first.
    fn const_patterns(&self, pattern: &mut Pattern, subject: &Ty, top: bool) -> Res<()> {
        let span = pattern.span;
        match &mut pattern.kind {
            PatternKind::Variant { name, args: None, .. } | PatternKind::Bind(name) => {
                let variant = top && matches!(peel(subject), Ty::Enum(owner) if self.enums.get(owner).is_some_and(|info| info.variants.iter().any(|(variant, _)| variant == name)));
                if variant {
                    return Ok(());
                }
                let Some(full) = self.const_named(name, span)? else { return Ok(()) };
                let (_, value) = self.consts.values.get(&full).expect("every const is found before the bodies");
                pattern.kind = match value {
                    ConstVal::Int(_, value) => PatternKind::Int(*value),
                    ConstVal::Char(value) => PatternKind::Char(*value),
                    ConstVal::Str(text) => PatternKind::Str(text.clone()),
                    ConstVal::Bool(value) => PatternKind::Bool(*value),
                    _ => {
                        return Err(Diag::error(span, format!("`{name}` is a const of `{}`, and a pattern of a const is a number, a character, a text or a `bool`", self.consts.types[&full]))
                            .hint(format!("compare it after `when`: `var value when value == {name}`")));
                    }
                };
                Ok(())
            }
            PatternKind::Variant { args: Some(args), .. } => args.iter_mut().try_for_each(|arg| self.const_patterns(arg, subject, false)),
            PatternKind::Or(alternatives) => alternatives.iter_mut().try_for_each(|alternative| self.const_patterns(alternative, subject, top)),
            PatternKind::Range(low, high) => {
                self.const_patterns(low, subject, false)?;
                self.const_patterns(high, subject, false)
            }
            _ => Ok(()),
        }
    }

    /// `x is Circle(r)`: looks at the value without taking it, so the names are borrows.
    /// They are seen by the code that runs only where the test holds (for `is not`, or under
    /// `!` when `flip`, where it fails), as `seen` says.
    fn is_variant(&mut self, expr: &mut Expr, seen: NamesSeen, flip: bool) -> Res<Binds> {
        let ExprKind::IsVariant { expr: subject, negated, pattern, by_ref } = &mut expr.kind else {
            unreachable!("only called for an `is` test");
        };
        let ty = self.operand(subject, None)?;
        let (variants, mode) = self.enum_behind(&ty, subject.span, "`is`")?;
        *by_ref = mode == Some(Mode::Ref);
        self.const_patterns(pattern, &ty, true)?;
        let binds = self.pattern_binds(pattern, &variants, mode.or(Some(Mode::Read)), &ty)?;
        if binds.is_empty() {
            return Ok(binds);
        }
        self.borrows.binds = self.place_loans(subject, *by_ref, subject.span);
        self.borrows.binds.extend(opens_shared(subject));
        let not = *negated != flip;
        let nowhere = Diag::error(pattern.span, "nothing here can use the names in this pattern");
        let seen_where = if not { NOT_NAMES_SEEN } else { NAMES_SEEN };
        match (seen, not) {
            (NamesSeen::Holds, false) | (NamesSeen::Fails, true) => Ok(binds),
            (NamesSeen::Holds, true) => {
                let what = if *negated { "`is not`" } else { "under `!`, `is`" };
                Err(Diag::error(pattern.span, format!("{what} holds when the value is another variant, so nothing here can use the names"))
                    .note(NOT_NAMES_SEEN)
                    .hint("write `_` for each value, or test `is` without `not`"))
            }
            (NamesSeen::Fails, false) => Err(nowhere
                .note("with `||` the block also runs when this test does not hold, and then the names have no value")
                .hint("test it alone in an `if` of its own, or use `match`")),
            (NamesSeen::StaysIn, _) => Err(nowhere
                .note(seen_where)
                .note("this `if` has no `else`, and the code after it also runs when the block has run")
                .hint("end the block with `return`, or use `match`")),
            (NamesSeen::Break(at), _) => Err(nowhere
                .related(at, "this `break` leaves the loop while the test may still hold")
                .note("after a `while`, the names of `is not` are seen only when the test is what ends the loop")
                .hint("test the value again after the loop, or use `match`")),
            (NamesSeen::No, _) => Err(nowhere.note(seen_where).hint("write `_` for each value, or use `match`")),
        }
    }

    /// `match subject { ... }`. As a statement (`unused`) the arms give no value.
    fn match_expr(
        &mut self,
        subject: &mut Expr,
        arms: &mut [Arm],
        borrowed: &mut bool,
        expected: Option<&Ty>,
        unused: bool,
    ) -> Res<Ty> {
        // Like `var y = subject`: an owned value is moved in, a borrow stays a borrow. The
        // `match` read from `a?.b` lends `a` when it is a place.
        let held = or_null_held(arms);
        let subject_ty = match &held {
            Some(held) => self.or_null_subject(subject, arms, held, unused)?,
            None => self.expr(subject, None)?,
        };
        // On an integer the arms are numbers, on a `char` characters, on a text texts; on a
        // `bool` or a `T?` patterns as inside a pattern; on anything else, the variants of an
        // enum.
        let number = match subject_ty.strip_borrow() {
            Ty::Int(ty) => Some(Literal::Int(*ty)),
            Ty::Char => Some(Literal::Char),
            Ty::Str => Some(Literal::Text),
            Ty::Bool | Ty::Nullable(_) => Some(Literal::Other),
            _ => None,
        };
        let (of, mode) = match number {
            Some(Literal::Int(ty)) => (Variants { owner: ty.name().to_string(), list: Vec::new() }, None),
            Some(Literal::Char) => (Variants { owner: "char".to_string(), list: Vec::new() }, None),
            Some(Literal::Text) => (Variants { owner: "string".to_string(), list: Vec::new() }, None),
            Some(Literal::Other) => {
                let mode = match &subject_ty {
                    Ty::Read(_) => Some(Mode::Read),
                    Ty::Ref(_) => Some(Mode::Ref),
                    _ => None,
                };
                (Variants { owner: subject_ty.strip_borrow().to_string(), list: Vec::new() }, mode)
            }
            None => self.enum_behind(&subject_ty, subject.span, "`match`")?,
        };
        *borrowed = mode.is_some();
        let (owner, variants) = (&of.owner, &of.list);
        // The names of the arms borrow from what the subject was lent from.
        let mut subject_loans = match mode {
            Some(mode) => self.loans_of(subject, mode == Mode::Ref),
            None if self.holds(&subject_ty) => self.loans_of(subject, false),
            None => Vec::new(),
        };
        if mode.is_some() {
            subject_loans.extend(opens_shared(subject));
        }
        let node = self.node();

        // What the arms without a condition have settled: variant positions, or numbers.
        let mut handled: Vec<i128> = Vec::new();
        let mut handled_texts: Vec<String> = Vec::new();
        let mut handled_all = false;
        // The variants an arm without a condition names, with patterns inside or not.
        let mut named: Vec<i128> = Vec::new();
        // With patterns inside patterns, the arms so far as the search for missing values sees
        // them (only those without a condition).
        // On a number, a character, a `bool` or a `T?` the search always runs: it counts the
        // values the arms take.
        let mut nested = matches!(number, Some(Literal::Int(_) | Literal::Char | Literal::Other));
        let mut rows: Vec<Vec<exhaust::Pat>> = Vec::new();
        // On a `T?`, whether an arm without a condition has taken `null`: a name below it
        // takes a value that is not `null`.
        let mut null_taken = false;
        // `a?.b` where no `T?` is expected gives the type of `b`, made a `T?`.
        let mut result: Option<Ty> = expected.filter(|ty| held.is_none() || without_null(ty).is_some()).cloned();
        let before = self.moves();
        let mut after: Option<Moves> = None;
        for (arm_index, arm) in arms.iter_mut().enumerate() {
            self.set_moves(&before);
            self.scopes.push(Scope::default());
            self.borrows.branches.push((node, arm_index));
            self.const_patterns(&mut arm.pattern, &subject_ty, true)?;
            let binds = match number {
                Some(Literal::Int(ty)) => self.int_pattern(&mut arm.pattern, ty, &subject_ty)?,
                Some(Literal::Char) => char_pattern(&mut arm.pattern, &subject_ty)?,
                Some(Literal::Text) => text_pattern(&mut arm.pattern, &subject_ty)?,
                Some(Literal::Other) => {
                    let mut binds = Vec::new();
                    self.inner_pattern(&mut arm.pattern, &subject_ty, None, &mut binds)?;
                    if let (PatternKind::Bind(_), true) = (&arm.pattern.kind, null_taken) {
                        binds[0].1 = not_null(&subject_ty);
                    }
                    binds
                }
                None => self.pattern_binds(&mut arm.pattern, &of, mode, &subject_ty)?,
            };
            let position = match &arm.pattern.kind {
                PatternKind::Variant { index, .. } => Some(*index as i128),
                PatternKind::Int(value) => Some(*value),
                _ => None,
            };
            let text = match &arm.pattern.kind {
                PatternKind::Str(text) => Some(text.clone()),
                _ => None,
            };
            if handled_all {
                return Err(Diag::error(arm.pattern.span, "this arm can never run: an arm above takes every value"));
            }
            let again = position.is_some_and(|position| handled.contains(&position)) || text.as_ref().is_some_and(|text| handled_texts.contains(text));
            if again {
                let what = match (number, position) {
                    (None, Some(position)) => variants[position as usize].0.as_str(),
                    _ => &self.src[arm.pattern.span.start..arm.pattern.span.end],
                };
                return Err(Diag::error(arm.pattern.span, format!("this arm can never run: `{what}` is already handled above")));
            }
            // With patterns inside patterns, an arm can be covered by several arms above.
            nested |= arm.pattern.needs_search();
            let row = vec![self.search_pattern(&arm.pattern, &subject_ty)];
            if nested && exhaust::useful(&rows, &row).is_none() {
                return Err(Diag::error(arm.pattern.span, "this arm can never run: the arms above take every value it matches"));
            }
            // So can a pattern of an `or`, by the arms above and the patterns before it.
            if let PatternKind::Or(alternatives) = &arm.pattern.kind {
                let mut above = rows.clone();
                for alternative in alternatives {
                    let row = vec![self.search_pattern(alternative, &subject_ty)];
                    if exhaust::useful(&above, &row).is_none() {
                        return Err(Diag::error(alternative.span, "this pattern can never match: the arms above and the patterns before it in the `or` take every value it matches"));
                    }
                    above.push(row);
                }
            }
            if arm.guard.is_none() {
                rows.push(row);
                if number.is_none() {
                    named.extend(top_variants(&arm.pattern));
                }
                null_taken |= takes_null(&arm.pattern);
            }
            self.borrows.binds = subject_loans.clone();
            self.declare_binds(binds)?;
            match &mut arm.guard {
                Some(guard) => {
                    let unmoved = self.moves();
                    self.typed(guard, &Ty::Bool)?;
                    self.only_looked(&unmoved)?;
                }
                // Only an arm without a condition settles its variant, and only when it takes
                // every value the variant carries.
                None => match (position, text) {
                    _ if arm.pattern.needs_search() => {}
                    (Some(position), _) => handled.push(position),
                    (None, Some(text)) => handled_texts.push(text),
                    (None, None) => handled_all = true,
                },
            }
            // `Some(v) => Log.debug(...)`: the call goes into the body, where it is guarded.
            if unused && arm.value.as_ref().is_some_and(|value| self.log_level_of(value).is_some()) {
                let value = arm.value.take().expect("tested above");
                arm.body.push(Stmt { span: value.span, kind: StmtKind::Expr(value) });
            }
            for stmt in arm.body.iter_mut() {
                self.stmt(stmt)?;
            }
            let leaves = arm_leaves(arm, self.loops > 0);
            match &mut arm.value {
                Some(value) if unused => self.unused(value)?,
                Some(value) => match &result {
                    Some(ty) => {
                        let ty = ty.clone();
                        self.typed(value, &ty)?;
                    }
                    None => match self.expr(value, None)? {
                        Ty::Never => {}
                        // The `null` of `a?.b` takes the type the other arm gives.
                        Ty::Null if held.is_some() => {}
                        ty if held.is_some() => result = Some(self.held_value(value, ty)?),
                        Ty::Null => {
                            return Err(Diag::error(value.span, "the type of this `match` cannot be found from `null` alone")
                                .hint("write the type where the result goes, for example `int? x = match ...`"));
                        }
                        ty => result = Some(ty),
                    },
                },
                None if unused || leaves => {}
                None => {
                    return Err(Diag::error(arm.span, "this arm gives no value, and the `match` is used as one")
                        .hint("the last line of the block is the value of the arm"));
                }
            }
            self.pop_scope(!leaves)?;
            self.borrows.branches.pop();
            // An arm that leaves does not reach the code after the `match`.
            if !leaves {
                if let Some(other) = &after {
                    self.merge_moves(other);
                }
                after = Some(self.moves());
            }
        }

        // With an `or`, a range, `null`, `true` or `false`, the search tells what is missing.
        let missing = match (handled_all, nested && number.is_some()) {
            (false, true) => exhaust::useful(&rows, &[exhaust::Pat::Any]).map(|found| exhaust::show(&found[0])),
            _ => None,
        };
        let guarded = arms.iter().any(|arm| arm.guard.is_some());
        match (missing, number) {
            (Some(value), Some(Literal::Other)) => {
                let diag = Diag::error(subject.span, format!("this `match` does not handle `{value}`"))
                    .hint("add an arm for it, or `_ => ...` for everything else");
                return Err(match guarded {
                    true => diag.note("an arm with `when` does not settle what it matches: the condition may be false"),
                    false => diag,
                });
            }
            (Some(value), _) if value != "_" => {
                return Err(Diag::error(subject.span, format!("this `match` does not handle every `{owner}` value"))
                    .note(format!("no arm takes `{value}`"))
                    .hint("add `_ => ...` for everything else"));
            }
            (None, Some(_)) if nested => {}
            _ if !handled_all && number.is_some() => {
                return Err(Diag::error(subject.span, format!("this `match` does not handle every `{owner}` value"))
                    .hint("add `_ => ...` for everything else"));
            }
            _ => {}
        }
        if !handled_all {
            let missing: Vec<String> = (0..variants.len())
                .filter(|position| !handled.contains(&(*position as i128)) && !named.contains(&(*position as i128)))
                .map(|position| format!("`{}`", variants[position].0))
                .collect();
            // An `or` with a name in it can take every variant without naming them.
            let covered = nested && exhaust::useful(&rows, &[exhaust::Pat::Any]).is_none();
            if !missing.is_empty() && !covered {
                let diag = Diag::error(subject.span, format!("this `match` does not handle {}", missing.join(", ")))
                    .hint("add an arm for each, or `_ => ...` for everything else");
                let guarded = arms.iter().any(|arm| {
                    arm.guard.is_some() && matches!(&arm.pattern.kind, PatternKind::Variant { index, .. } if !handled.contains(&(*index as i128)))
                });
                return Err(match guarded {
                    true => diag.note("an arm with `when` does not settle its variant: the condition may be false"),
                    false => diag.note(format!("every variant of `{owner}` needs an arm")),
                });
            }
            // Every variant has an arm; with patterns inside, some of its values may not.
            if let Some(found) = exhaust::useful(&rows, &[exhaust::Pat::Any]) {
                let value = exhaust::show(&found[0]);
                let diag = Diag::error(subject.span, format!("this `match` does not handle `{value}`"))
                    .hint("add an arm for it, or `_ => ...` for everything else");
                return Err(match arms.iter().any(|arm| arm.guard.is_some()) {
                    true => diag.note("an arm with `when` does not settle what it matches: the condition may be false"),
                    false => diag,
                });
            }
        }
        if let Some(after) = &after {
            self.set_moves(after);
        }
        if let (Some(_), Some(ty), Some(null)) = (&held, &result, &mut arms[0].value) {
            if null.ty == Ty::Null {
                let ty = ty.clone();
                self.typed(null, &ty)?;
            }
        }
        Ok(match (unused, after) {
            (_, None) => Ty::Never,
            (true, _) => Ty::Void,
            (false, _) => result.unwrap_or(Ty::Never),
        })
    }

    fn binary(&mut self, op: BinOp, left: &mut Expr, right: &mut Expr, expected: Option<&Ty>, span: Span) -> Res<Ty> {
        if op.is_shift() {
            return self.shift(op, left, right, expected, span);
        }
        let wanted = if op.is_comparison() { None } else { expected.filter(|ty| ty.is_number()) };
        let like = |ty: &Ty| match ty.strip_borrow() {
            Ty::Int(_) | Ty::Float(_) | Ty::Char => Some(ty.strip_borrow().clone()),
            _ => None,
        };
        // A number on the left past every `long` takes its type from the right:
        // `0xffff_ffff_ffff_ffff == top` with a `ulong top`. Only the types are looked at in
        // this order; the left side still runs first.
        let (left_ty, right_ty) = match &left.kind {
            ExprKind::Int(value, _) if !IntTy::Long.fits(*value) && !matches!(right.kind, ExprKind::Int(..)) => {
                let value = *value;
                let right_ty = self.operand(right, wanted)?;
                let fitting = like(&right_ty).filter(|ty| matches!(ty, Ty::Int(ty) if ty.fits(value)));
                (self.operand(left, fitting.as_ref().or(wanted))?, right_ty)
            }
            // `'a' == c`: the character takes its type from the right, as a number would.
            ExprKind::Char(_) if !matches!(right.kind, ExprKind::Char(_)) => {
                let right_ty = self.operand(right, wanted)?;
                (self.operand(left, like(&right_ty).as_ref().or(wanted))?, right_ty)
            }
            _ => {
                let left_ty = self.operand(left, wanted)?;
                // `p + n` moves a raw pointer by a count of items, an `nint`.
                let moved = matches!(left_ty.strip_borrow(), Ty::Raw(_)) && matches!(op, BinOp::Add | BinOp::Sub);
                let side = if moved { Some(Ty::Int(IntTy::Nint)) } else { like(&left_ty) };
                let right_ty = self.operand(right, side.as_ref().or(wanted))?;
                (left_ty, right_ty)
            }
        };

        for (side, ty) in [(&*left, &left_ty), (&*right, &right_ty)] {
            if matches!(peel(ty), Ty::Locked(_)) {
                return Err(self.locked_outside(side.span));
            }
        }
        // An atomic value is read with `load`, and changed in one step by its methods.
        if let Some(side) = [&*left, &*right].into_iter().find(|side| matches!(peel(&side.ty), Ty::Atomic(_))) {
            let text = &self.src[side.span.start..side.span.end];
            let diag = Diag::error(span, format!("operator `{}` is not defined for `{left_ty}` and `{right_ty}`", op.symbol()));
            return Err(diag.hint(format!("read it first: `{text}.load()`")).note("an atomic value is read once with `load`, so that another task cannot change it in the middle of the computation"));
        }
        // `n + 1` on an `int?`: the value may be missing, so it is tested first.
        if !matches!(op, BinOp::Eq | BinOp::Ne) {
            if let Some(side) = [&*left, &*right].into_iter().find(|side| matches!(peel(&side.ty), Ty::Nullable(_))) {
                let text = self.src[side.span.start..side.span.end].trim();
                return Err(Diag::error(span, format!("operator `{}` is not defined for `{left_ty}` and `{right_ty}`", op.symbol()))
                    .note(format!("`{text}` may be `null`"))
                    .hint(format!("{}; or give a fallback with `??`", self.test_first(side, &side.ty))));
            }
        }
        let undefined = || Diag::error(span, format!("operator `{}` is not defined for `{left_ty}` and `{right_ty}`", op.symbol()));
        // `a == b` is `a.equals(b)` and `a < b` is `a.compare(b) < 0`, for the types that have them.
        if let (Ty::Struct(name) | Ty::Enum(name), true, true) = (peel(&left_ty), peel(&left_ty) == peel(&right_ty), op.is_comparison()) {
            let symbol = op.symbol();
            if matches!(op, BinOp::Eq | BinOp::Ne) {
                if self.equatable.contains(name) {
                    return Ok(Ty::Bool);
                }
                let diag = Diag::error(span, format!("`{symbol}` compares the values, and `{name}` is not `Equatable`"));
                // Two `shared` values may be asked whether they are one object instead.
                if matches!(left_ty.strip_borrow(), Ty::Shared(..)) && matches!(right_ty.strip_borrow(), Ty::Shared(..)) {
                    let (a, b) = (&self.src[left.span.start..left.span.end], &self.src[right.span.start..right.span.end]);
                    let not = if op == BinOp::Ne { "!" } else { "" };
                    return Err(diag
                        .note("`==` looks at what two `shared` values point to; `same` tells whether they point to one object")
                        .hint(format!("`{not}same({a}, {b})`, or add `Equatable` to the definition: `{name} : Equatable`")));
                }
                return Err(diag.hint(format!("add it to the definition: `{name} : Equatable`")));
            }
            if self.comparable.contains(name) {
                return Ok(Ty::Bool);
            }
            let diag = Diag::error(span, format!("`{symbol}` puts the values in order, and `{name}` is not `Comparable`"));
            if module_of(name) == STD_MODULE {
                return Err(lacks(diag, name, "Comparable"));
            }
            return Err(diag.hint(format!("add `: Comparable` to the definition and write `int compare(read this, read {name} other)`")));
        }
        // A collection is `Equatable` if what it holds is; it has no order of its own.
        if let (collection @ (Ty::List(_) | Ty::Set(_) | Ty::Map(..) | Ty::Array(..) | Ty::Slice(_)), true, true) = (peel(&left_ty), peel(&left_ty) == peel(&right_ty), op.is_comparison()) {
            let symbol = op.symbol();
            if !matches!(op, BinOp::Eq | BinOp::Ne) {
                return Err(Diag::error(span, format!("`{symbol}` puts the values in order, and a `{collection}` has no order of its own"))
                    .note("a collection is compared with `==` and `!=` only"));
            }
            if has(collection, self.equatable, true) {
                return Ok(Ty::Bool);
            }
            let diag = Diag::error(span, format!("`{symbol}` compares what the collections hold, and a `{collection}` is not `Equatable`"));
            return Err(match lacking(collection, self.equatable) {
                Some(inner) => diag.hint(format!("add it to the definition: `{inner} : Equatable`")),
                None => diag,
            });
        }
        match (left_ty.strip_borrow(), right_ty.strip_borrow()) {
            (Ty::Int(_), Ty::Int(_)) => {
                let common = self.common_int(left, left_ty.clone(), right, right_ty.clone())?;
                if common.is_c() && !op.is_comparison() {
                    let sides = [(&*left, int_of(&left_ty)), (&*right, int_of(&right_ty))];
                    return Err(c_arithmetic(self.src, op.symbol(), common, &sides, span));
                }
                Ok(if op.is_comparison() { Ty::Bool } else { Ty::Int(common) })
            }
            (Ty::Int(_) | Ty::Float(_), Ty::Int(_) | Ty::Float(_)) => {
                if matches!(op, BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor) {
                    return Err(undefined().note("the bit operators work on integers"));
                }
                let common = Ty::Float(self.common_float(left, left_ty.clone(), right, right_ty.clone())?);
                Ok(if op.is_comparison() { Ty::Bool } else { common })
            }
            (Ty::Str, Ty::Str) if op.is_comparison() => Ok(Ty::Bool),
            (Ty::Char, Ty::Char) if op.is_comparison() => {
                self.coerce(left, left_ty.clone(), &Ty::Char)?;
                self.coerce(right, right_ty.clone(), &Ty::Char)?;
                Ok(Ty::Bool)
            }
            (Ty::Char, Ty::Int(_)) | (Ty::Int(_), Ty::Char) if op.is_comparison() => {
                let int = if matches!(left_ty.strip_borrow(), Ty::Int(_)) { &left_ty } else { &right_ty };
                Err(undefined().hint(format!("a `char` is not a number; compare its code, `(int)c`, or make the `{int}` a `char` with `(char)`")))
            }
            (Ty::Char, _) | (_, Ty::Char) if !op.is_comparison() => {
                Err(undefined().note("a `char` is not a number: `(int)c` is its code, and `(char)n` the character of a code"))
            }
            // A side may be a borrow of a `bool` (`B(a)` in a `match` on a borrowed value): the
            // values are compared, not where they are.
            (Ty::Bool, Ty::Bool) if matches!(op, BinOp::Eq | BinOp::Ne) => {
                self.coerce(left, left_ty.clone(), &Ty::Bool)?;
                self.coerce(right, right_ty.clone(), &Ty::Bool)?;
                Ok(Ty::Bool)
            }
            // The bit operators are for integers; two `bool`s have their own.
            (Ty::Bool, Ty::Bool) if matches!(op, BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor) => {
                let own = match op {
                    BinOp::BitAnd => "&&",
                    BinOp::BitOr => "||",
                    _ => "!=",
                };
                Err(undefined().hint(format!("for two `bool`s write `{own}`")))
            }
            (Ty::Str | Ty::Fmt, Ty::Str | Ty::Fmt) if op == BinOp::Add => {
                Err(undefined().hint("strings are joined with `$\"{a}{b}\"`"))
            }
            // `p < q`: the order of two addresses, as numbers. Nothing is read.
            (Ty::Raw(a), Ty::Raw(b)) if matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge) => match a == b {
                true => {
                    let raw = left_ty.strip_borrow().clone();
                    self.coerce(left, left_ty.clone(), &raw)?;
                    self.coerce(right, right_ty.clone(), &raw)?;
                    Ok(Ty::Bool)
                }
                false => Err(undefined().hint(format!("point both at one type with a cast: `unsafe (raw {a})q`"))),
            },
            // `p - q`: how many items lie from `q` to `p`. Nothing is read.
            (Ty::Raw(a), Ty::Raw(b)) if op == BinOp::Sub => {
                if a != b {
                    return Err(undefined().note("the items between two pointers are counted when both point at one type"));
                }
                if **a == Ty::Void {
                    let (p, q) = (&self.src[left.span.start..left.span.end], &self.src[right.span.start..right.span.end]);
                    return Err(Diag::error(span, "a `raw void` points at memory of no known type, so the items between two of them cannot be counted")
                        .hint(format!("count bytes with casts: `unsafe ((raw byte){p} - (raw byte){q})`")));
                }
                if matches!(&**a, Ty::Struct(name) if self.structs[name].opaque) {
                    return Err(Diag::error(span, format!("a `{}` points at a type of C whose size SG does not know, so the items between two of them cannot be counted", left_ty.strip_borrow())));
                }
                let raw = left_ty.strip_borrow().clone();
                self.coerce(left, left_ty.clone(), &raw)?;
                self.coerce(right, right_ty.clone(), &raw)?;
                Ok(Ty::Int(IntTy::Nint))
            }
            // Two addresses, or an address and `null`, are compared as they are.
            (Ty::Raw(a), Ty::Raw(b)) if matches!(op, BinOp::Eq | BinOp::Ne) => match a == b {
                true => {
                    let raw = left_ty.strip_borrow().clone();
                    self.coerce(left, left_ty.clone(), &raw)?;
                    self.coerce(right, right_ty.clone(), &raw)?;
                    Ok(Ty::Bool)
                }
                false => Err(undefined().hint(format!("point both at one type with a cast: `unsafe (raw {a})q`"))),
            },
            // A pointer to a function of C is compared with `null`, or with one of its type.
            (Ty::CFunc(..), Ty::Null) | (Ty::Null, Ty::CFunc(..)) if matches!(op, BinOp::Eq | BinOp::Ne) => {
                for (side, ty) in [(&mut *left, &left_ty), (&mut *right, &right_ty)] {
                    if let Ty::CFunc(..) = ty.strip_borrow() {
                        let func = ty.strip_borrow().clone();
                        self.coerce(side, ty.clone(), &func)?;
                    }
                }
                Ok(Ty::Bool)
            }
            (Ty::CFunc(..), Ty::CFunc(..)) if matches!(op, BinOp::Eq | BinOp::Ne) && left_ty.strip_borrow() == right_ty.strip_borrow() => {
                let func = left_ty.strip_borrow().clone();
                self.coerce(left, left_ty.clone(), &func)?;
                self.coerce(right, right_ty.clone(), &func)?;
                Ok(Ty::Bool)
            }
            (Ty::Raw(_), Ty::Null) | (Ty::Null, Ty::Raw(_)) if matches!(op, BinOp::Eq | BinOp::Ne) => {
                for (side, ty) in [(&mut *left, &left_ty), (&mut *right, &right_ty)] {
                    if let Ty::Raw(_) = ty.strip_borrow() {
                        let raw = ty.strip_borrow().clone();
                        self.coerce(side, ty.clone(), &raw)?;
                    }
                }
                Ok(Ty::Bool)
            }
            (Ty::Raw(item), Ty::Int(_)) if matches!(op, BinOp::Add | BinOp::Sub) && matches!(&**item, Ty::Struct(name) if self.structs[name].opaque) => {
                Err(Diag::error(span, format!("a `{}` points at a type of C whose size SG does not know, so it does not move by items", left_ty.strip_borrow())))
            }
            (Ty::Raw(item), Ty::Int(_)) if matches!(op, BinOp::Add | BinOp::Sub) && **item != Ty::Void => {
                let raw = left_ty.strip_borrow().clone();
                self.needs_unsafe(span, "this moves a raw pointer, which may then point outside its memory")?;
                self.coerce(left, left_ty.clone(), &raw)?;
                // The right side was checked once already: it is brought to an `nint` here,
                // not checked again.
                let Ty::Int(found) = *right_ty.strip_borrow() else { unreachable!("matched as an integer") };
                self.coerce(right, right_ty.clone(), &Ty::Int(found))?;
                self.coerce(right, Ty::Int(found), &Ty::Int(IntTy::Nint))?;
                Ok(raw)
            }
            (Ty::Raw(_), Ty::Int(_)) if matches!(op, BinOp::Add | BinOp::Sub) => {
                let text = &self.src[left.span.start..left.span.end];
                Err(Diag::error(span, "a `raw void` points at memory of no known type, so it does not move by items")
                    .hint(format!("say what lies there with a cast: `(raw byte){text}`")))
            }
            (Ty::Int(_), Ty::Raw(_)) if op == BinOp::Add => {
                let (a, b) = (&self.src[left.span.start..left.span.end], &self.src[right.span.start..right.span.end]);
                Err(undefined().hint(format!("the pointer is written first: `{b} + {a}`")))
            }
            (Ty::Raw(_), _) | (_, Ty::Raw(_)) => Err(undefined().note("a raw pointer is compared with `==` and `!=`, and moved by a count of items with `+` and `-`")),
            // Time is added and taken away by methods: the language defines no operators.
            (Ty::Struct(name), _) if module_of(name) == STD_MODULE && matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) => {
                let (a, b) = (&self.src[left.span.start..left.span.end], &self.src[right.span.start..right.span.end]);
                let call = match (op, short_name(name)) {
                    (BinOp::Add, _) => format!("`{a}.plus({b})`"),
                    (BinOp::Sub, "Duration") => format!("`{a}.minus({b})`"),
                    (BinOp::Sub, _) => format!("`{a}.since({b})` for the time between two, `{a}.minus({b})` for a moment before"),
                    _ => format!("`{a}.times({b})`"),
                };
                Err(undefined().hint(call))
            }
            (Ty::Null, _) | (_, Ty::Null) if matches!(op, BinOp::Eq | BinOp::Ne) => {
                let word = if op == BinOp::Eq { "is null" } else { "is not null" };
                Err(Diag::error(span, "`null` is not compared with `==`").hint(format!("write `x {word}`")))
            }
            _ => Err(undefined()),
        }
    }

    /// `a << n`, `a >> n`: the result has the type of `a`, and the count is any integer. The
    /// expected type reaches `a` alone: `long x = 1 << 40` shifts a `long`.
    fn shift(&mut self, op: BinOp, left: &mut Expr, right: &mut Expr, expected: Option<&Ty>, span: Span) -> Res<Ty> {
        let wanted = expected.filter(|ty| matches!(ty, Ty::Int(_)));
        let left_ty = self.operand(left, wanted)?;
        let right_ty = self.operand(right, None)?;
        let (Ty::Int(ty), Ty::Int(count)) = (left_ty.strip_borrow(), right_ty.strip_borrow()) else {
            return Err(Diag::error(span, format!("operator `{}` is not defined for `{left_ty}` and `{right_ty}`", op.symbol()))
                .note("it shifts the bits of an integer by a count that is an integer"));
        };
        let (ty, count) = (*ty, *count);
        if ty.is_c() {
            return Err(c_arithmetic(self.src, op.symbol(), ty, &[(&*left, ty), (&*right, count)], span));
        }
        self.coerce(left, left_ty, &Ty::Int(ty))?;
        self.coerce(right, right_ty, &Ty::Int(count))?;
        shift_count(ty, right)?;
        Ok(Ty::Int(ty))
    }

    /// What `print`, `$"..."` and `f.write` accept: numbers, `bool`, text, a struct or an enum
    /// that is `Formattable`, and a collection of such values; also behind a pointer.
    /// The letter after `:` in `{x:X2}` fits the value: `x`, `X`, `b` and `D` are for an
    /// integer, `F` for a fractional number (also behind a pointer, as `print` looks).
    fn format_letter(&self, ty: &Ty, letter: Letter, span: Span, value: &Expr) -> Res<()> {
        let mut inner = ty.strip_borrow();
        while let Ty::Ptr(behind) | Ty::Shared(behind, _) = inner {
            inner = behind.strip_borrow();
        }
        let text = &self.src[value.span.start..value.span.end];
        let spec = &self.src[span.start..span.end];
        let what = match letter {
            Letter::Hex | Letter::HexUpper => "an integer in hex",
            Letter::Binary => "an integer in binary",
            Letter::Decimal => "an integer with at least so many digits",
            Letter::Fixed => "a fractional number with so many digits after the point",
        };
        let name = letter.name();
        let shown = ty.to_string();
        let article = if shown.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
        let diag = || Diag::error(span, format!("`{name}` writes {what}, and `{text}` is {article} `{shown}`"));
        match (letter, inner) {
            (Letter::Fixed, Ty::Float(_)) => Ok(()),
            (Letter::Fixed, Ty::Int(_)) => Err(diag().hint(format!("make it a `double` first: `{{(double){text}{spec}}}`"))),
            (Letter::Fixed, _) => Err(diag()),
            (_, Ty::Int(_)) => Ok(()),
            (_, Ty::Char) => Err(diag().hint(format!("the code of the character: `{{(int){text}{spec}}}`"))),
            (Letter::Decimal, Ty::Float(_)) => Err(diag().hint("`F0` writes it with no digits after the point")),
            (_, _) => Err(diag()),
        }
    }

    fn printable(&self, ty: &Ty, span: Span) -> Res<()> {
        let mut inner = ty.strip_borrow();
        while let Ty::Ptr(behind) | Ty::Shared(behind, _) = inner {
            inner = behind.strip_borrow();
        }
        match inner {
            Ty::Locked(_) => Err(self.locked_outside(span)),
            Ty::Task(_) => Err(Diag::error(span, format!("a `{inner}` cannot be written as text")).hint("wait for it and write what it gives: `print(task.wait())`")),
            Ty::Group => Err(Diag::error(span, "a `TaskGroup` cannot be written as text").hint("`group.count()` is how many of its tasks run")),
            Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char | Ty::Str | Ty::Fmt => Ok(()),
            Ty::Struct(name) | Ty::Enum(name) if self.formattable.contains(name) => Ok(()),
            Ty::Struct(name) | Ty::Enum(name) => Err(lacks(Diag::error(span, format!("`{name}` cannot be written as text: it is not `Formattable`")), name, "Formattable")),
            Ty::Void => Err(Diag::error(span, "this call does not return a value, and one is needed here")),
            Ty::Nullable(_) => Err(Diag::error(span, format!("this is `{ty}` and may be `null`; there is no text for `null`"))
                .hint("give a fallback with `??`, or test with `is null`")),
            Ty::Result(..) => Err(Diag::error(span, format!("this is `{ty}`: either a value or an error"))
                .hint("open it first with `try`, `??` or `match`")),
            // A collection is written item by item, so the items must have a text.
            Ty::List(_) | Ty::Map(..) | Ty::Set(_) | Ty::Array(..) | Ty::Slice(_) if can_format(inner, self.formattable) => Ok(()),
            Ty::List(_) | Ty::Map(..) | Ty::Set(_) | Ty::Array(..) | Ty::Slice(_) => {
                let diag = Diag::error(span, format!("a `{inner}` cannot be written as text"));
                Err(match unformattable(inner, self.formattable) {
                    Some(item) => diag.hint(format!("its items are written one by one, so add `Formattable` to the definition: `{item} : Formattable`")),
                    None => diag,
                })
            }
            Ty::Raw(_) => Err(Diag::error(span, format!("a `{inner}` cannot be written as text: an address differs from one run to the next"))
                .hint("write what lies there: `unsafe p[0]`")),
            Ty::Weak(..) => Err(Diag::error(span, format!("a `{inner}` cannot be written as text: what it looks at may be gone"))
                .hint("ask for an owner first with `share`, which gives `null` if it is gone")),
            Ty::Atomic(_) => {
                let text = &self.src[span.start..span.end];
                Err(Diag::error(span, format!("an `{inner}` is not written as text: another task may change it while it is written"))
                    .hint(format!("read it first: `{text}.load()`")))
            }
            other => Err(Diag::error(span, format!("a `{other}` value cannot be written as text"))),
        }
    }

    /// The arguments a call leaves out at its end, given the default values of their parameters,
    /// as if they were written there (choices 1620-1622): `null` and an empty collection as
    /// written, any other value by its const; lent for a `read` parameter.
    fn fill_defaults(&mut self, name: &str, sig: &FuncSig, args: &mut Vec<Expr>, span: Span) -> Res<()> {
        let required = sig.defaults.iter().take_while(|default| default.is_none()).count();
        if required == sig.params.len() {
            return Ok(());
        }
        if args.len() < required || args.len() > sig.params.len() {
            return Err(Diag::error(span, format!("`{name}` takes {required} to {} argument(s), {} given", sig.params.len(), args.len()))
                .note(format!("the last {} have default values", sig.params.len() - required)));
        }
        let at = Span::new(span.end, span.end);
        let made = |kind: ExprKind| Expr { kind, span: at, ty: Ty::Void, moved: false };
        for position in args.len()..sig.params.len() {
            let value = match &sig.defaults[position] {
                Some(FieldDefault::Expr(written)) => Expr { span: at, ..written.clone() },
                Some(FieldDefault::Const(full)) => match self.consts.values.get(full) {
                    Some((_, ConstVal::Str(text))) => made(ExprKind::Str(text.clone())),
                    _ => made(ExprKind::Const(full.clone())),
                },
                None => unreachable!("only the last parameters have default values"),
            };
            args.push(match sig.params[position] {
                Ty::Read(_) => made(ExprKind::Borrow { mutable: false, expr: Box::new(value) }),
                _ => value,
            });
        }
        Ok(())
    }

    fn args(&mut self, name: &str, args: &mut [Expr], params: &[Ty], span: Span) -> Res<()> {
        if args.len() != params.len() {
            return Err(Diag::error(span, format!("`{name}` takes {} argument(s), {} given", params.len(), args.len())));
        }
        for (arg, param) in args.iter_mut().zip(params) {
            self.typed(arg, param)?;
        }
        self.lent_twice(None, args, params)
    }

    fn call(&mut self, name: &str, name_span: Span, args: &mut Vec<Expr>, callee: &mut Callee, span: Span) -> Res<Ty> {
        let awaited = std::mem::take(&mut self.awaiting);
        let arity = |count: usize| {
            if args.len() == count {
                Ok(())
            } else {
                Err(Diag::error(span, format!("`{name}` takes {count} argument(s), {} given", args.len())))
            }
        };
        // A variable that holds a function value is called like a function; it wins over a
        // function of the same name.
        if let Some(var) = self.find(name) {
            let held = var.current.strip_borrow().clone();
            if let Ty::CFunc(params, ret) = held {
                self.live_var(name, name_span)?;
                self.c_value_call(name, args, &params, span)?;
                *callee = Callee::CValue(params);
                return Ok(*ret);
            }
            let Ty::Func(params, ret) = held else {
                return Err(Diag::error(name_span, format!("`{name}` is a `{held}`, not a function, and cannot be called")));
            };
            self.live_var(name, name_span)?;
            self.args(name, args, &params, span).map_err(|diag| match args.len() < params.len() {
                true => diag.note(format!("a call through a `Func` is given every argument: `{name}` does not know the default values of the function it holds")),
                false => diag,
            })?;
            *callee = Callee::Value;
            return Ok(*ret);
        }
        match name {
            // `printError(x)` writes like `print`, to the standard error.
            "print" | "printError" => {
                arity(1)?;
                let ty = self.operand(&mut args[0], None)?;
                self.printable(&ty, args[0].span)?;
                Ok(Ty::Void)
            }
            // `exit(code)` ends the program at once with that code. A `byte`, so a code the
            // system would cut to its last eight bits (`exit(256)` is 0) is a compile error.
            "exit" => {
                arity(1)?;
                self.typed(&mut args[0], &Ty::Int(IntTy::Byte)).map_err(|diag| diag.note("an exit code is a `byte`: the system keeps eight bits, so 256 would end as 0"))?;
                self.unclosed(0, Some((span, "exit")))?;
                Ok(Ty::Never)
            }
            "assert" => {
                arity(1)?;
                self.typed(&mut args[0], &Ty::Bool)?;
                Ok(Ty::Void)
            }
            // `CancellationSource()`: a source that nothing has cancelled yet.
            "CancellationSource" => {
                arity(0).map_err(|diag| diag.hint("`CancellationSource.after(ms)` cancels itself after that many milliseconds"))?;
                *callee = Callee::Cancel(CancelCall::New);
                Ok(Ty::Source)
            }
            // `TaskGroup()`, `TaskGroup(limit)`: a group of tasks that lets go of each as it
            // ends (1716); `whenRoom` waits while `limit` of them run (1719).
            "TaskGroup" => {
                if args.len() > 1 {
                    return Err(Diag::error(span, format!("`TaskGroup` takes 0 or 1 argument(s), {} given", args.len()))
                        .hint("`TaskGroup()` has no limit; `TaskGroup(1000)` has `whenRoom` wait while 1,000 of its tasks run"));
                }
                let limited = args.len() == 1;
                if limited {
                    self.typed(&mut args[0], &Ty::Int(IntTy::Int)).map_err(|diag| diag.note("the limit of a group is an `int`: how many of its tasks may run before `whenRoom` waits"))?;
                    let written = match &args[0].kind {
                        ExprKind::Int(value, _) => Some(*value),
                        ExprKind::Unary(UnOp::Neg, inner) => match &inner.kind {
                            ExprKind::Int(value, _) => Some(-*value),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(value) = written.filter(|value| *value < 1) {
                        return Err(Diag::error(args[0].span, format!("a group lets at least one task run, and this limit is {value}"))
                            .hint("`TaskGroup()` has no limit"));
                    }
                }
                *callee = Callee::Group(GroupCall::New(limited), false);
                Ok(Ty::Group)
            }
            "CancellationToken" => Err(Diag::error(name_span, "a token is not made by itself: it comes from its source")
                .hint("`source.token()` gives a token of `source`; `CancellationToken.None` is never cancelled")),
            // `same(a, b)`: whether two `shared` values point to one object. It only looks at
            // them, as `print` does.
            "same" => {
                arity(2)?;
                let mut found = Vec::new();
                for arg in args.iter_mut() {
                    let ty = self.operand(arg, None)?;
                    match ty.strip_borrow() {
                        Ty::Shared(behind, _) => found.push((**behind).clone()),
                        Ty::Nullable(inner) if matches!(**inner, Ty::Shared(..)) => {
                            let diag = Diag::error(arg.span, format!("this is `{}` and may be `null`; `same` compares two objects", ty.strip_borrow()));
                            return Err(match &arg.kind {
                                ExprKind::Var(var) => diag.hint(format!("test it first: `if {var} is not null {{ ... }}`")),
                                _ => diag,
                            });
                        }
                        Ty::Weak(..) => {
                            return Err(Diag::error(arg.span, "a `weak` value may point to nothing any more; `same` compares two `shared` values")
                                .hint(format!("`share {}` gives a `shared` one, or `null` if it is gone", self.src[arg.span.start..arg.span.end].trim())));
                        }
                        other => {
                            let diag = Diag::error(arg.span, format!("`same` tells whether two `shared` values point to one object, and this is `{other}`"));
                            return Err(match has(other, self.equatable, true) {
                                true => diag.hint("`==` compares what two values hold"),
                                false => diag,
                            });
                        }
                    }
                }
                if found[0] != found[1] {
                    return Err(Diag::error(span, format!("`same` compares two values of one type, and these are `shared {}` and `shared {}`", found[0], found[1])));
                }
                Ok(Ty::Bool)
            }
            "panic" => {
                arity(1)?;
                let ty = self.operand(&mut args[0], None)?;
                if !matches!(ty.strip_borrow(), Ty::Str | Ty::Fmt) {
                    return Err(Diag::error(args[0].span, format!("`panic` needs a string, found `{ty}`")));
                }
                Ok(Ty::Never)
            }
            // `T(args)` is short for `T.create(args)`.
            _ if self.structs.contains_key(name) => {
                if !self.structs[name].methods.contains_key("create") && module_of(name) == STD_MODULE {
                    let short = short_name(name);
                    return Err(Diag::error(name_span, format!("a `{short}` is made by its functions, not with `{short}(...)`")).hint(std_makers(short)));
                }
                if !self.structs[name].methods.contains_key("create") {
                    return Err(Diag::error(name_span, format!("`{name}` has no `create` function, so `{name}(...)` cannot build it"))
                        .hint(format!("write the fields: `{name} {{ ... }}`, or add a `create` function to the struct")));
                }
                self.static_call(name, "create", name_span, args, callee, span, awaited)
            }
            _ => {
                let sigs = self.sigs;
                let Some(sig) = sigs.get(name) else {
                    let diag = Diag::error(name_span, format!("there is no function named `{name}`"));
                    if self.static_named(name, name_span).ok().flatten().is_some() {
                        return Err(diag.note(format!("`{name}` is a static, which a call does not read")).hint(format!("copy the function it holds to a local first: `var f = {name}`, then `f(...)`")));
                    }
                    if name == "StringBuilder" {
                        return Err(diag.hint(STRING_BUILDER));
                    }
                    if let Some(hint) = self.names.use_hint(name, name_span) {
                        return Err(diag.hint(hint));
                    }
                    let known = sigs.keys().map(String::as_str).chain(BUILTINS.iter().copied());
                    return Err(match nearest(name, known) {
                        Some(close) => diag.hint(format!("`{close}`")),
                        None => diag,
                    });
                };
                if sig.foreign {
                    return self.foreign_call(name, args, sig, callee, span);
                }
                self.fill_defaults(name, sig, args, span)?;
                if !sig.is_async {
                    self.args(name, args, &sig.params, span)?;
                    return Ok(sig.ret.clone());
                }
                // Called without `await`, the function starts as a task of its own, which may
                // run on another thread while this one goes on: what it takes and gives follows
                // the thread rule, as with `Task.run`.
                if !awaited {
                    let given: Vec<(Ty, Span)> = sig.params.iter().cloned().zip(args.iter().map(|arg| arg.span)).collect();
                    self.unawaited(short_name(name), &given, span)?;
                }
                self.args(name, args, &sig.params, span)?;
                Ok(Ty::Task(Box::new(sig.ret.clone())))
            }
        }
    }

    /// A call of a function of C: inside `unsafe`, since the checker cannot see what it does.
    /// `buffer.pointer()` is written only here, as an argument: C is given the address of the
    /// items for the time of the call, as it is given a `ref` borrow.
    fn foreign_call(&mut self, name: &str, args: &mut [Expr], sig: &FuncSig, callee: &mut Callee, span: Span) -> Res<Ty> {
        let short = short_name(name);
        if sig.variadic {
            if args.len() < sig.params.len() {
                return Err(Diag::error(span, format!("`{short}` takes at least {} argument(s), {} given", sig.params.len(), args.len())));
            }
            self.needs_unsafe(span, &format!("`{short}` is a function of C, and the checker cannot see what it does"))?;
            let (fixed, more) = args.split_at_mut(sig.params.len());
            self.c_typed(fixed, &sig.params)?;
            let mut params = sig.params.clone();
            for arg in more.iter_mut() {
                params.push(self.variadic_arg(arg)?);
            }
            self.lent_twice(None, args, &params)?;
            *callee = Callee::Variadic(params);
            return Ok(sig.ret.clone());
        }
        if args.len() != sig.params.len() {
            return Err(Diag::error(span, format!("`{short}` takes {} argument(s), {} given", sig.params.len(), args.len())));
        }
        self.needs_unsafe(span, &format!("`{short}` is a function of C, and the checker cannot see what it does"))?;
        self.c_args(args, &sig.params)?;
        *callee = Callee::Extern;
        Ok(sig.ret.clone())
    }

    /// A call through a `CFunc`: it may reach any function of C, so it is a call of C.
    fn c_value_call(&mut self, name: &str, args: &mut [Expr], params: &[Ty], span: Span) -> Res<()> {
        if args.len() != params.len() {
            return Err(Diag::error(span, format!("`{name}` takes {} argument(s), {} given", params.len(), args.len())));
        }
        self.needs_unsafe(span, &format!("`{name}` points at a function of C, and the checker cannot see what it does"))?;
        self.c_args(args, params)
    }

    /// The arguments of a call of C: `buffer.pointer()` is allowed among them.
    fn c_args(&mut self, args: &mut [Expr], params: &[Ty]) -> Res<()> {
        self.c_typed(args, params)?;
        self.lent_twice(None, args, params)
    }

    /// One argument given after the parameters of a function of C declared with `...`: C
    /// reads it as the type it has, which it says itself. C would widen a small integer, a
    /// `bool` and a `float`, and guess `int` for a number written without a type; here both
    /// are written out.
    fn variadic_arg(&mut self, arg: &mut Expr) -> Res<Ty> {
        let text = self.src[arg.span.start..arg.span.end].trim().to_string();
        let literal = match &arg.kind {
            ExprKind::Unary(UnOp::Neg, inner) => &inner.kind,
            other => other,
        };
        let untyped = |diag: Diag| diag.note("C reads each value after `...` as the type the function looks for, and only the call says what it gives");
        match literal {
            ExprKind::Int(..) | ExprKind::Char(_) => {
                return Err(untyped(Diag::error(arg.span, "this number has no type of its own, and C reads it after `...` as whatever it looks for"))
                    .hint(format!("say what C reads: `(int){text}`, or `(long){text}` where C takes a `long`")));
            }
            ExprKind::Float(..) => {
                return Err(untyped(Diag::error(arg.span, "this number has no type of its own, and C reads it after `...` as whatever it looks for"))
                    .hint(format!("`(double){text}`: C reads a fractional number after `...` as a `double`")));
            }
            ExprKind::Null => {
                return Err(untyped(Diag::error(arg.span, "`null` has no type of its own, and C reads it after `...` as whatever it looks for"))
                    .hint("a variable of the pointer C reads: `raw void none = null`, then `none`"));
            }
            _ => {}
        }
        let pointer = |expr: &Expr| matches!(&expr.kind, ExprKind::Method { name, .. } if name == "pointer");
        self.extern_arg = match &arg.kind {
            ExprKind::RawCast { expr, .. } => pointer(expr),
            _ => pointer(arg),
        };
        // The name of an `export` function is its address, as C takes it.
        let export = match &arg.kind {
            ExprKind::Var(name) if self.find(name).is_none() => {
                let full = self.names.full(name, arg.span)?.unwrap_or_else(|| name.clone());
                self.sigs.get(full.as_str()).filter(|sig| sig.export).map(|sig| Ty::CFunc(sig.params.clone(), Box::new(sig.ret.clone())))
            }
            _ => None,
        };
        let ty = self.operand(arg, export.as_ref());
        self.extern_arg = false;
        let ty = ty?;
        let structs = self.structs;
        let fine = match &ty {
            Ty::Int(int) => !matches!(int, IntTy::Sbyte | IntTy::Byte | IntTy::Short | IntTy::Ushort),
            Ty::Float(float) => *float == FloatTy::Double,
            Ty::Raw(_) | Ty::CFunc(..) => true,
            Ty::Read(inner) if **inner == Ty::CString => true,
            Ty::Read(inner) | Ty::Ref(inner) => not_plain(inner, structs).is_none() && !matches!(**inner, Ty::Array(..)),
            _ => false,
        };
        if fine {
            return Ok(ty);
        }
        let diag = Diag::error(arg.span, format!("a `{ty}` does not go to C after `...`"));
        Err(match &ty {
            Ty::Int(_) => diag.note("C reads it there as an `int`, made wider without a word").hint(format!("`(int){text}`")),
            Ty::Bool => diag.note("C reads it there as an `int`, made wider without a word").hint("give C an `int`: 1 for `true`, 0 for `false`"),
            Ty::Float(_) => diag.note("C reads it there as a `double`, made wider without a word").hint(format!("`(double){text}`")),
            Ty::Char => diag.note("a `char` of SG is a Unicode character of 32 bits, and C has no type of it").hint(format!("`(uint){text}`")),
            Ty::Struct(_) => diag.note("C is given a struct by value only where it declares one").hint(format!("its address, if C reads that: `ref {text}`")),
            Ty::Str => diag.hint("a text goes to C as `read CString`: `CString.from(text)` makes one"),
            Ty::CString => diag.hint(format!("a `CString` is lent to C: `read {text}`")),
            Ty::Func(..) => diag.hint("C calls a function of SG written with `export`"),
            _ => diag.note("after `...` C takes `int`, `uint`, `long`, `ulong`, `nint`, `nuint`, `double`, `raw T`, `CFunc`, `ref T` and `read T` of plain data, and `read CString`"),
        })
    }

    /// The arguments of a call of C, each as its parameter takes it.
    fn c_typed(&mut self, args: &mut [Expr], params: &[Ty]) -> Res<()> {
        for (arg, param) in args.iter_mut().zip(params) {
            let pointer = |expr: &Expr| matches!(&expr.kind, ExprKind::Method { name, .. } if name == "pointer");
            self.extern_arg = match &arg.kind {
                ExprKind::RawCast { expr, .. } => pointer(expr),
                _ => pointer(arg),
            };
            let checked = self.typed(arg, param);
            self.extern_arg = false;
            checked?;
        }
        Ok(())
    }

    /// `buffer.pointer()`: the address of the items of a list, an array or a slice of plain
    /// data, as a `raw T`, written only as an argument of a function of C.
    fn pointer_of(&mut self, recv: &Expr, recv_ty: &Ty, item: Ty, is_place: bool, args: &[Expr], span: Span) -> Res<Ty> {
        if !std::mem::take(&mut self.extern_arg) {
            return Err(Diag::error(span, "`pointer()` is written only as an argument of a C function")
                .note("C is given the address of the items for the time of the call; a pointer kept after it could point at items that have moved")
                .hint("memory that C keeps is taken with `Memory.allocate<T>(count)`"));
        }
        if !args.is_empty() {
            return Err(Diag::error(span, format!("`pointer` takes 0 argument(s), {} given", args.len())));
        }
        if let Some(part) = not_plain(&item, self.structs) {
            let diag = Diag::error(recv.span, format!("C is given the address of plain data, and the items are `{item}`"));
            return Err(match part == item {
                true => diag,
                false => diag.note(format!("they hold a `{part}`")),
            });
        }
        if !is_place && !recv_ty.is_borrow() {
            let text = &self.src[recv.span.start..recv.span.end];
            return Err(Diag::error(recv.span, "the items whose address C is given are kept in a variable")
                .hint(format!("`var buffer = {text}`, then `buffer.pointer()`")));
        }
        Ok(Ty::Raw(Box::new(item)))
    }

    /// What an async function returns comes back from its task, which may have run on another
    /// thread: it follows the thread rule.
    fn async_returns(&self, func: &Func, ret: &Ty) -> Res<()> {
        if let Some((part, at)) = stays(ret, self.structs, self.enums, self.names, &mut Vec::new()) {
            let what = if part == *ret { String::new() } else { format!(", which holds a `{part}`{at}") };
            return Err(Diag::error(func.ret.span, format!("the async function `{}` returns a `{ret}`{what}, which cannot come back from a task: {}", short_name(&func.name), why_it_stays(&part, self.names)))
                .note(THREAD_RULE));
        }
        Ok(())
    }

    /// `await task`, `await f(x)`: waits for the task without holding the thread, and gives
    /// what its function returned. Takes the task.
    fn await_expr(&mut self, inner: &mut Expr, span: Span) -> Res<Ty> {
        if !self.is_async {
            let text = &self.src[inner.span.start..inner.span.end];
            let diag = Diag::error(span, "`await` is written in an async function");
            return Err(match self.in_test {
                true => diag.hint(format!("a test waits for the task: `{text}.wait()`")),
                false => diag.hint(format!("make `{}` async (`async` in front of it), or wait for the task here: `{text}.wait()`", short_name(&self.func_name))),
            });
        }
        // `await stock.load(read xs)`: the function of a module is a call too.
        self.unqualify(inner);
        let (ty, is_place) = match self.place(inner)? {
            Some((ty, _)) => (ty, true),
            None => {
                if matches!(inner.kind, ExprKind::Call { .. } | ExprKind::Method { .. }) {
                    self.awaiting = true;
                }
                let ty = self.expr(inner, None);
                self.awaiting = false;
                (ty?, false)
            }
        };
        let Ty::Task(result) = ty.strip_borrow() else {
            let diag = Diag::error(inner.span, format!("`await` waits for a task, and this is a `{ty}`"));
            return Err(match &inner.kind {
                ExprKind::Call { name, .. } if self.sigs.contains_key(name.as_str()) => {
                    diag.hint(format!("`{}` is not async, so its call gives the value at once: drop the `await`", short_name(name)))
                }
                _ => diag,
            });
        };
        let result = (**result).clone();
        if ty.is_borrow() {
            return Err(Diag::error(inner.span, "`await` takes the task and uses it up, and this one is only borrowed").note("whoever started the task waits for it"));
        }
        if is_place {
            self.move_out(inner, &ty)?;
            inner.moved = true;
        }
        Ok(result)
    }

    /// What an async function or method takes when it is called without `await`, and so starts
    /// as a task of its own: each value, `this` first, must be able to go to another thread.
    fn unawaited(&self, shown: &str, given: &[(Ty, Span)], span: Span) -> Res<()> {
        for (param, at_span) in given {
            if let Some((part, at)) = stays(param, self.structs, self.enums, self.names, &mut Vec::new()) {
                let what = if part == *param { String::new() } else { format!(", which holds a `{part}`{at}") };
                let text = &self.src[span.start..span.end];
                let diag = Diag::error(*at_span, format!("`{shown}` takes a `{param}` here{what}, and a call without `await` starts it as a task of its own, which this cannot go to: {}", why_it_stays(&part, self.names)));
                return Err(match &part {
                    Ty::Read(_) | Ty::Ref(_) if self.is_async => diag.hint(format!("`await {text}` runs it as part of this task, where it can borrow")),
                    Ty::Read(inner) | Ty::Ref(inner) => diag.hint(format!("give the task a value of its own: a `{inner}` to own, or a `shared {inner}` that both keep")),
                    _ => diag.note(THREAD_RULE),
                });
            }
        }
        Ok(())
    }

    /// What a method gives: an async one a task, after the check of what a call without
    /// `await` takes (`this` is the value the method is called on, as it is taken).
    #[allow(clippy::too_many_arguments)]
    fn method_gives(&self, name: &str, sig: &FuncSig, this: Option<(Ty, Span)>, args: &[Expr], span: Span, awaited: bool) -> Res<Ty> {
        if !sig.is_async {
            return Ok(sig.ret.clone());
        }
        if !awaited {
            let mut given: Vec<(Ty, Span)> = this.into_iter().collect();
            given.extend(sig.params.iter().cloned().zip(args.iter().map(|arg| arg.span)));
            self.unawaited(name, &given, span)?;
        }
        Ok(Ty::Task(Box::new(sig.ret.clone())))
    }

    /// `Stack.create(args)`: a function of the struct that does not take `this`.
    #[allow(clippy::too_many_arguments)]
    fn static_call(
        &mut self,
        owner: &str,
        name: &str,
        name_span: Span,
        args: &mut Vec<Expr>,
        callee: &mut Callee,
        span: Span,
        awaited: bool,
    ) -> Res<Ty> {
        let methods = self.methods_of(owner);
        // `Header.fromBytes(bytes)`, unless the struct has a function of that name.
        if name == "fromBytes" && !methods.contains_key(name) && self.structs.contains_key(owner) {
            return self.struct_from_bytes(owner, name_span, args, callee, span);
        }
        // `Block.fromValue(n)`: the variant that has the number, or `null`.
        if let Some(numbers) = self.enums.get(owner).and_then(|info| info.numbers).filter(|_| name == "fromValue") {
            self.args(name, args, &[Ty::Int(numbers)], span)?;
            *callee = Callee::FromValue(owner.to_string());
            return Ok(Ty::Nullable(Box::new(Ty::Enum(owner.to_string()))));
        }
        let Some(sig) = methods.get(name) else {
            let diag = Diag::error(name_span, format!("`{owner}` has no function `{name}`"));
            if owner == format!("{STD_MODULE}.Environment") && matches!(name, "set" | "remove" | "unset" | "add") {
                return Err(diag.note("the variables of the environment are only read: setting one while another thread reads one is not safe in C"));
            }
            return Err(match nearest(name, methods.keys().map(String::as_str)) {
                Some(close) => diag.hint(format!("`{close}`")),
                None => diag,
            });
        };
        if !sig.public && !self.inside(owner) {
            return Err(self.not_public(name, owner, name_span));
        }
        if sig.this.is_some() {
            return Err(Diag::error(name_span, format!("`{name}` works on a value, not on the type `{owner}`"))
                .hint(format!("call it on a `{owner}` value: `value.{name}(...)`")));
        }
        let bits = owner == format!("{STD_MODULE}.SecureRandom") && name == "nextLong" && args.is_empty();
        self.fill_defaults(name, sig, args, span)?;
        self.args(name, args, &sig.params, span).map_err(|diag| match bits {
            true => diag.hint("`SecureRandom.nextUlong()` gives 64 random bits; `nextLong(low, high)` a number in a range"),
            false => diag,
        })?;
        if owner == format!("{STD_MODULE}.Regex") && matches!(name, "compile" | "of") {
            self.pattern_written(name, &args[0])?;
        }
        let compressing = ["Deflate", "Zlib", "Gzip"].iter().any(|format| owner == format!("{STD_MODULE}.{format}"));
        if compressing && matches!(name, "compress" | "writer") {
            level_written(&args[1])?;
        }
        *callee = Callee::Static { owner: owner.to_string(), name: name.to_string() };
        self.method_gives(name, sig, None, args, span, awaited)
    }

    /// The pattern of `Regex.compile(pattern)` or `Regex.of(pattern)`: a text written in the
    /// program is kept to be checked once every body is; `Regex.of` takes nothing else.
    fn pattern_written(&self, name: &str, arg: &Expr) -> Res<()> {
        let mut inner = peel_widen(arg);
        while let ExprKind::Borrow { expr, .. } = &inner.kind {
            inner = peel_widen(expr);
        }
        match &inner.kind {
            ExprKind::Str(text) => {
                PATTERNS.with(|found| found.borrow_mut().insert(arg.span.start, (arg.span, text.clone())));
                Ok(())
            }
            _ if name == "of" => Err(Diag::error(arg.span, "`Regex.of` takes a pattern written in the program, which is checked when the program is compiled")
                .hint("a pattern made when the program runs goes to `Regex.compile(pattern)`, which gives a `Result<Regex, RegexError>`")),
            _ => Ok(()),
        }
    }

    /// `ulong.fromLittleEndian(bytes)`: the integer that exactly as many bytes as the type
    /// takes write. Where the program shows how many bytes the window has, another number is
    /// a compile error; otherwise the count is checked when the program runs.
    fn number_from_bytes(&mut self, ty: &Ty, name: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span) -> Res<Ty> {
        let int = match ty {
            Ty::Int(int @ (IntTy::Short | IntTy::Ushort | IntTy::Int | IntTy::Uint | IntTy::Long | IntTy::Ulong)) => *int,
            Ty::Int(int @ (IntTy::Byte | IntTy::Sbyte)) => {
                // The name of the bytes as the program writes it: `data` in `data[..1]`.
                let bytes = match args.first().map(|arg| &arg.kind) {
                    Some(ExprKind::Slice { recv, .. } | ExprKind::Borrow { expr: recv, .. }) if steady(recv) => &self.src[recv.span.start..recv.span.end],
                    _ => "bytes",
                };
                let read = if *int == IntTy::Byte { format!("the byte itself: `{bytes}[i]`") } else { format!("`(sbyte){bytes}[i]`") };
                return Err(Diag::error(name_span, format!("a `{ty}` is one byte, and one byte has no order to be read in")).hint(read));
            }
            Ty::Int(int) => {
                let fixed = if int.signed() { "long" } else { "ulong" };
                let size = if int.is_c() { "the size of C's `long`, which depends on the system" } else { "the size of an address, which depends on the machine" };
                return Err(Diag::error(name_span, format!("`{ty}` has {size}; a number kept in bytes has a size of its own"))
                    .hint(format!("`{fixed}.{name}(...)` reads 8 bytes; a cast makes it `{ty}`")));
            }
            _ => {
                return Err(Diag::error(name_span, format!("`{name}` reads an integer, and `{ty}` is a fractional type"))
                    .note("reading a fractional number out of bytes is not here yet"));
            }
        };
        let bytes = Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
        self.args(name, args, &[bytes], span)?;
        let size = i128::from(int.bits() / 8);
        if let Some(given) = known_width(self.src, &args[0]) {
            if given != size {
                let diag = Diag::error(args[0].span, format!("`{ty}.{name}` reads {size} bytes, and this window has {given}"));
                let window = match &unwidened(&args[0]).kind {
                    // A start written as a number is added up: `data[4..8]`, not `data[4..4 + 4]`.
                    ExprKind::Slice { recv, from: Some(from), .. } => {
                        let recv_text = &self.src[recv.span.start..recv.span.end];
                        match &from.kind {
                            ExprKind::Int(at, _) => Some(format!("`{recv_text}[{at}..{}]`", at + size)),
                            _ => {
                                let from = &self.src[from.span.start..from.span.end];
                                Some(format!("`{recv_text}[{from}..{from} + {size}]`"))
                            }
                        }
                    }
                    ExprKind::Slice { recv, from: None, .. } => Some(format!("`{}[..{size}]`", &self.src[recv.span.start..recv.span.end])),
                    _ => None,
                };
                return Err(match window {
                    Some(window) => diag.hint(format!("a window of {size} bytes: {window}")),
                    None => diag,
                });
            }
        }
        *callee = Callee::FromBytes { ty: int, big: name == "fromBigEndian" };
        Ok(ty.clone())
    }

    /// `Header.fromBytes(bytes)`: a struct of plain data that every pattern of bits is a value of,
    /// read out of exactly as many bytes as it takes. Needs no `unsafe`: whatever the bytes are,
    /// the value is one of its type.
    fn struct_from_bytes(&mut self, owner: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span) -> Res<Ty> {
        let short = short_name(owner);
        let ty = Ty::Struct(owner.to_string());
        if let Some(part) = not_plain(&ty, self.structs) {
            return Err(Diag::error(name_span, format!("`{short}.fromBytes` reads plain data out of bytes, and `{short}` holds a `{part}`")));
        }
        if let Some(part) = not_any_bits(&ty, self.structs) {
            let why = match part {
                Ty::Bool => "a `bool` is 0 or 1",
                Ty::Char => "a `char` is a code of Unicode",
                _ => "an address made of any bytes is no address C gave",
            };
            return Err(Diag::error(name_span, format!("`{short}.fromBytes` takes any bytes, and `{short}` holds a `{part}`, which not every pattern of bits is"))
                .note(why)
                .hint("read the bytes into a struct of integers, and make the field of it from one"));
        }
        let bytes = Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
        self.args("fromBytes", args, &[bytes], span)?;
        let (size, _) = crate::raw::layout(&ty, &plain_types(self.structs));
        if let Some(given) = known_width(self.src, &args[0]) {
            if given != size as i128 {
                return Err(Diag::error(args[0].span, format!("`{short}.fromBytes` reads {size} bytes, and this window has {given}")));
            }
        }
        *callee = Callee::StructFromBytes(owner.to_string());
        Ok(ty)
    }

    /// `Task.run(work, a, b)`: starts `work(a, b)` as a task of its own. The arguments are
    /// checked as in a call; what the function takes and gives follows the thread rule.
    fn task_run(&mut self, name: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span, awaited: bool) -> Res<Ty> {
        // `Task.delay(ms)`: a task that only lets time go by. `Task.delay(ms, read token)`
        // ends early when the token is cancelled, and says which way it ended.
        if name == "delay" {
            if args.is_empty() || args.len() > 2 {
                return Err(Diag::error(span, format!("`Task.delay` takes 1 or 2 argument(s), {} given", args.len()))
                    .hint("`Task.delay(100)` ends after 100 milliseconds; `Task.delay(100, read token)` ends early when the token is cancelled"));
            }
            self.typed(&mut args[0], &Ty::Int(IntTy::Int)).map_err(|diag| {
                // `Duration` is the language's own type, so the message names no other.
                let span_given = diag.msg.ends_with("found `Duration`");
                let diag = diag.note("`Task.delay(ms)` takes the milliseconds as an `int`");
                match span_given {
                    true => diag.hint(format!("`Task.delay((int){}.milliseconds())`", &self.src[args[0].span.start..args[0].span.end])),
                    false => diag,
                }
            })?;
            if args.len() == 1 {
                *callee = Callee::Delay(false);
                return Ok(Ty::Task(Box::new(Ty::Void)));
            }
            self.token_arg(&mut args[1])?;
            self.awaited_here("Task.delay", awaited, &args[1..], span)?;
            *callee = Callee::Delay(true);
            return Ok(Ty::Task(Box::new(Ty::Result(Box::new(Ty::Void), Box::new(Ty::Struct(CANCELLED.to_string()))))));
        }
        if name == "whenAll" || name == "whenAny" {
            return self.when(name, args, callee, span, awaited);
        }
        // `Task.processorCount()`: how many processors the program may run on (1856).
        if name == "processorCount" {
            if !args.is_empty() {
                return Err(Diag::error(span, format!("`Task.processorCount` takes no argument, {} given", args.len()))
                    .hint("`Task.processorCount()` gives how many processors the program may run on"));
            }
            *callee = Callee::ProcessorCount;
            return Ok(Ty::Int(IntTy::Nint));
        }
        if name != "run" {
            let diag = Diag::error(name_span, format!("`Task` has the functions `run`, `delay`, `whenAll`, `whenAny` and `processorCount`, not `{name}`"));
            return Err(match nearest(name, ["run", "delay", "whenAll", "whenAny", "processorCount"].into_iter()) {
                Some(close) => diag.hint(format!("`Task.{close}`")),
                None => diag.hint("`Task.run(work, x)` starts `work(x)` as a task"),
            });
        }
        let Some((first, rest)) = args.split_first_mut() else {
            return Err(Diag::error(span, "`Task.run` takes the function to start, then what it is given: `Task.run(work, x)`"));
        };
        if let ExprKind::Call { name: called, .. } = &first.kind {
            let text = &self.src[first.span.start..first.span.end];
            let args = text.split_once('(').map_or("", |(_, args)| args.trim_end_matches(')'));
            let sep = if args.trim().is_empty() { "" } else { ", " };
            return Err(Diag::error(first.span, format!("this calls `{called}` here and gives `Task.run` what it returns"))
                .hint(format!("the function and what it is given are written apart: `Task.run({called}{sep}{args})`")));
        }
        let found = self.expr(first, None)?;
        let Ty::Func(params, ret) = found.strip_borrow().clone() else {
            return Err(Diag::error(first.span, format!("`Task.run` starts a function, and this is a `{found}`")).hint("`Task.run(work, x)` starts `work(x)` as a task"));
        };
        let func = self.src[first.span.start..first.span.end].to_string();
        if rest.len() != params.len() {
            return Err(Diag::error(span, format!("`{func}` takes {} argument(s), {} given after it", params.len(), rest.len())));
        }
        for (position, param) in params.iter().enumerate() {
            if let Some((part, at)) = stays(param, self.structs, self.enums, self.names, &mut Vec::new()) {
                let what = if part == *param { String::new() } else { format!(", which holds a `{part}`{at}") };
                let diag = Diag::error(rest[position].span, format!("`{func}` takes a `{param}` here{what}, which cannot go to another task: {}", why_it_stays(&part, self.names)));
                return Err(match &part {
                    Ty::Read(inner) | Ty::Ref(inner) => diag.hint(format!("give the task a value of its own: a `{inner}` to own, or a `shared {inner}` that both keep")),
                    _ => diag.note(THREAD_RULE),
                });
            }
        }
        if let Some((part, at)) = stays(&ret, self.structs, self.enums, self.names, &mut Vec::new()) {
            let what = if part == *ret { String::new() } else { format!(", which holds a `{part}`{at}") };
            return Err(Diag::error(first.span, format!("`{func}` returns a `{ret}`{what}, which cannot come back from another task: {}", why_it_stays(&part, self.names)))
                .note(THREAD_RULE));
        }
        self.args(&func, rest, &params, span)?;
        *callee = Callee::TaskRun;
        Ok(Ty::Task(ret))
    }

    /// `Task.whenAll(tasks)`: takes a `List<Task<R>>` and gives a task of what each gave, in
    /// the order of the list (a `Task<void>` when they give nothing). `Task.whenAny(ref tasks)`
    /// gives a task of what the first of them to end gave, and takes that task out of the list.
    /// Both are awaited, or waited for, where they are written: what they give is a task of
    /// their own only for that moment.
    fn when(&mut self, name: &str, args: &mut [Expr], callee: &mut Callee, span: Span, awaited: bool) -> Res<Ty> {
        let any = name == "whenAny";
        let written = if any { "Task.whenAny(ref tasks)" } else { "Task.whenAll(tasks)" };
        let [arg] = args else {
            return Err(Diag::error(span, format!("`Task.{name}` takes 1 argument, the list of tasks, {} given", args.len())).hint(format!("`{written}`")));
        };
        let text = self.src[arg.span.start..arg.span.end].to_string();
        let found = self.expr(arg, None)?;
        let list = match (any, &found) {
            (false, list) if task_list(list) => list.clone(),
            (true, Ty::Ref(list)) if task_list(list) => (**list).clone(),
            (true, list) if task_list(list) || task_list(list.strip_borrow()) => {
                let place = match &arg.kind {
                    ExprKind::Borrow { expr, .. } => &self.src[expr.span.start..expr.span.end],
                    _ => text.as_str(),
                };
                return Err(Diag::error(arg.span, "`Task.whenAny` takes the task that ends out of the list, so it is given the list with `ref`")
                    .hint(format!("`Task.whenAny(ref {place})`")));
            }
            (false, list) if task_list(list.strip_borrow()) => {
                return Err(Diag::error(arg.span, "`Task.whenAll` takes the list and every task in it, so it is given the list itself, not a borrow")
                    .note("the tasks that are left after `Task.whenAny` are waited for with `Task.whenAll` too"));
            }
            _ => {
                let diag = Diag::error(arg.span, format!("`Task.{name}` waits for the tasks of a `List<Task<R>>`, and this is a `{found}`"));
                return Err(match found.strip_borrow() {
                    Ty::Task(_) => diag.hint(match self.is_async {
                        true => format!("one task is awaited by itself: `await {text}`"),
                        false => format!("one task is waited for by itself: `{text}.wait()`"),
                    }),
                    _ => diag.note("tasks are put in a list with `add`: `tasks.add(count(path))`"),
                });
            }
        };
        if !awaited {
            let diag = Diag::error(span, format!("`Task.{name}` is awaited, or waited for, where it is written"));
            return Err(match self.is_async {
                true => diag.hint(format!("`await {}`", &self.src[span.start..span.end])),
                false => diag.hint(format!("`{}.wait()`", &self.src[span.start..span.end])),
            });
        }
        let Ty::List(item) = list else { unreachable!("a list of tasks") };
        let Ty::Task(result) = *item else { unreachable!("a list of tasks") };
        *callee = if any { Callee::WhenAny } else { Callee::WhenAll };
        Ok(Ty::Task(Box::new(match (*result, any) {
            (result, true) => result,
            (Ty::Void, false) => Ty::Void,
            (result, false) => Ty::List(Box::new(result)),
        })))
    }

    /// The orders written after the values of an atomic operation, at most `most` of them, each
    /// a variant of `MemoryOrder` written out, so that it is checked against the operation when
    /// the program is compiled. They are taken out of the arguments.
    fn orders(&mut self, op: AtomicOp, args: &mut Vec<Expr>, values: usize, most: usize, span: Span) -> Res<Vec<(Order, Span)>> {
        let name = op.name();
        if args.len() < values || args.len() > values + most {
            let diag = Diag::error(span, format!("`{name}` takes {} argument(s), {} given", match most {
                0 => values.to_string(),
                _ => format!("{values} to {}", values + most),
            }, args.len()));
            return Err(match op {
                AtomicOp::CompareExchange(_) => diag.note(format!("`{name}(expected, desired)`, then the order it keeps and the one it keeps when it fails, if not `SequentiallyConsistent`")),
                AtomicOp::Load => diag.note("`load()`, or `load(MemoryOrder.Acquire)` with an order"),
                AtomicOp::SpinLoop => diag,
                _ if values == 0 => diag.note(format!("`Atomic.{name}()`, or `Atomic.{name}(MemoryOrder.Acquire)` with an order")),
                _ => diag.note(format!("`{name}(value)`, or `{name}(value, MemoryOrder.Release)` with an order")),
            });
        }
        let mut found = Vec::new();
        for arg in &mut args[values..] {
            let ty = self.expr(arg, Some(&Ty::Enum(MEMORY_ORDER.to_string())))?;
            match &arg.kind {
                ExprKind::Variant { owner, index, .. } if owner == MEMORY_ORDER => found.push((Order::ALL[*index], arg.span)),
                _ if ty.strip_borrow() == &Ty::Enum(MEMORY_ORDER.to_string()) => {
                    return Err(Diag::error(arg.span, "the order of an atomic operation is written out, as a variant: `MemoryOrder.Acquire`")
                        .note("the compiler checks it against the operation, so it is known when the program is compiled"));
                }
                _ => {
                    return Err(Diag::error(arg.span, format!("the order of `{name}` is a `MemoryOrder`, and this is `{ty}`"))
                        .hint("`MemoryOrder.Relaxed`, `Acquire`, `Release`, `AcquireRelease` or `SequentiallyConsistent`"));
                }
            }
        }
        args.truncate(values);
        Ok(found)
    }

    /// `count.fetchAdd(1)`, `flag.compareExchange(false, true)`: an operation on an `atomic T`,
    /// sequentially consistent unless an order is written. `item` is the `T`.
    fn atomic_method(&mut self, item: &Ty, name: &str, name_span: Span, args: &mut Vec<Expr>, callee: &mut Callee, span: Span) -> Res<Ty> {
        let op = match name {
            "load" => AtomicOp::Load,
            "store" => AtomicOp::Store,
            "exchange" => AtomicOp::Exchange,
            "compareExchange" => AtomicOp::CompareExchange(false),
            "compareExchangeWeak" => AtomicOp::CompareExchange(true),
            "fetchAdd" => AtomicOp::FetchAdd(false),
            "wrappingFetchAdd" => AtomicOp::FetchAdd(true),
            "fetchSub" => AtomicOp::FetchSub(false),
            "wrappingFetchSub" => AtomicOp::FetchSub(true),
            "fetchAnd" => AtomicOp::FetchAnd,
            "fetchOr" => AtomicOp::FetchOr,
            "fetchXor" => AtomicOp::FetchXor,
            _ => {
                let atomic = Ty::Atomic(Box::new(item.clone()));
                let known: Vec<&str> = ATOMIC_METHODS.iter().copied().filter(|method| atomic_takes(item, method)).collect();
                let diag = Diag::error(name_span, format!("`{atomic}` has no method `{name}`"));
                return Err(match (name, nearest(name, known.iter().copied())) {
                    ("add" | "increment", _) => diag.hint("`fetchAdd(1)`, which gives the value before"),
                    ("sub" | "decrement", _) => diag.hint("`fetchSub(1)`, which gives the value before"),
                    ("get" | "read" | "value", _) => diag.hint("`load()`"),
                    ("set" | "write", _) => diag.hint("`store(value)`"),
                    ("swap", _) => diag.hint("`exchange(value)`, which gives the value before"),
                    ("compareAndSwap" | "compareAndSet" | "cas", _) => diag.hint("`compareExchange(expected, desired)`"),
                    (_, Some(close)) => diag.hint(format!("`{close}`")),
                    (_, None) => diag.note(format!("methods: {}", known.join(", "))),
                });
            }
        };
        if !atomic_takes(item, name) {
            let diag = Diag::error(name_span, format!("`{name}` is not a method of `atomic {item}`"));
            return Err(match (op, item) {
                (AtomicOp::FetchAdd(true) | AtomicOp::FetchSub(true), Ty::Float(_)) => diag.note("a fractional number does not wrap; `fetchAdd` and `fetchSub` add to it"),
                (AtomicOp::FetchAdd(_) | AtomicOp::FetchSub(_), Ty::Bool) => diag.note("a `bool` is not a number; `fetchAnd`, `fetchOr` and `fetchXor` change it"),
                (AtomicOp::FetchAnd | AtomicOp::FetchOr | AtomicOp::FetchXor, Ty::Float(_)) => diag.note("the bits of a fractional number are not taken one by one"),
                (_, Ty::Raw(_)) => diag.note("an atomic pointer is loaded, stored, exchanged and compared: `load`, `store`, `exchange`, `compareExchange`"),
                _ => diag,
            });
        }
        let (values, most) = match op {
            AtomicOp::Load => (0, 1),
            AtomicOp::CompareExchange(_) => (2, 2),
            _ => (1, 1),
        };
        let orders = self.orders(op, args, values, most, span)?;
        for arg in args.iter_mut() {
            self.typed(arg, item)?;
        }
        let order = orders.first().map_or(Order::SequentiallyConsistent, |(order, _)| *order);
        let refused = match op {
            AtomicOp::Load if matches!(order, Order::Release | Order::AcquireRelease) => Some("a load reads, so its order is `Relaxed`, `Acquire` or `SequentiallyConsistent`"),
            AtomicOp::Store if matches!(order, Order::Acquire | Order::AcquireRelease) => Some("a store writes, so its order is `Relaxed`, `Release` or `SequentiallyConsistent`"),
            _ => None,
        };
        if let Some(why) = refused {
            return Err(Diag::error(orders[0].1, format!("`{name}` cannot keep the order `{}`", order.name())).note(why));
        }
        // When it fails, `compareExchange` only reads: the order it keeps then is the one it
        // keeps when it succeeds, without the part that writes, unless another is written.
        let failure = match orders.get(1) {
            Some((failure, at)) => {
                if matches!(failure, Order::Release | Order::AcquireRelease) {
                    return Err(Diag::error(*at, format!("a `{name}` that fails only reads, so it cannot keep the order `{}`", failure.name()))
                        .note("the second order is `Relaxed`, `Acquire` or `SequentiallyConsistent`"));
                }
                let strength = |order: Order| match order {
                    Order::Relaxed | Order::Release => 0,
                    Order::Acquire | Order::AcquireRelease => 1,
                    Order::SequentiallyConsistent => 2,
                };
                if strength(*failure) > strength(order) {
                    return Err(Diag::error(*at, format!("a `{name}` that fails keeps no more order than one that succeeds, and `{}` is more than `{}`", failure.name(), order.name()))
                        .note("C asks this of the two orders"));
                }
                *failure
            }
            None => match order {
                Order::AcquireRelease => Order::Acquire,
                Order::Release => Order::Relaxed,
                other => other,
            },
        };
        *callee = Callee::Atomic { op, order, failure };
        Ok(match op {
            AtomicOp::Store => Ty::Void,
            AtomicOp::CompareExchange(_) => Ty::Result(Box::new(item.clone()), Box::new(item.clone())),
            _ => item.clone(),
        })
    }

    /// `Atomic.fence(order)`, `Atomic.compilerFence(order)` and `Atomic.spinLoop()`.
    fn atomic_static(&mut self, name: &str, name_span: Span, args: &mut Vec<Expr>, callee: &mut Callee, span: Span) -> Res<Ty> {
        let op = match name {
            "fence" => AtomicOp::Fence,
            "compilerFence" => AtomicOp::CompilerFence,
            "spinLoop" => AtomicOp::SpinLoop,
            _ => {
                let diag = Diag::error(name_span, format!("`Atomic` has no function `{name}`"));
                return Err(match nearest(name, ["fence", "compilerFence", "spinLoop"].into_iter()) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag.note("functions: fence, compilerFence, spinLoop; an atomic value is made with `atomic`: `atomic 0`"),
                });
            }
        };
        let most = if op == AtomicOp::SpinLoop { 0 } else { 1 };
        let orders = self.orders(op, args, 0, most, span)?;
        let order = orders.first().map_or(Order::SequentiallyConsistent, |(order, _)| *order);
        if order == Order::Relaxed {
            return Err(Diag::error(orders[0].1, format!("a `{name}` that is `Relaxed` orders nothing"))
                .note("its order is `Acquire`, `Release`, `AcquireRelease` or `SequentiallyConsistent`"));
        }
        *callee = Callee::Atomic { op, order, failure: order };
        Ok(Ty::Void)
    }

    /// `CancellationSource.after(ms)` and `CancellationSource.linked(read token)` (1775):
    /// functions of the source type. The token type has only its constant, `CancellationToken.None`.
    fn cancel_static(&mut self, owner: &str, name: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span) -> Res<Ty> {
        if owner == "CancellationToken" {
            return Err(Diag::error(name_span, format!("`CancellationToken` has no function `{name}`"))
                .hint("a token comes from its source: `source.token()`; `CancellationToken.None` is never cancelled"));
        }
        if name == "linked" {
            if args.len() != 1 {
                return Err(Diag::error(span, format!("`CancellationSource.linked` takes 1 argument(s), {} given", args.len())));
            }
            self.token_arg(&mut args[0])?;
            *callee = Callee::Cancel(CancelCall::Linked);
            return Ok(Ty::Source);
        }
        if name != "after" {
            let diag = Diag::error(name_span, format!("`CancellationSource` has the functions `after` and `linked`, not `{name}`"));
            return Err(match name {
                "create" | "new" => diag.hint("`CancellationSource()` makes a source"),
                "createLinkedTokenSource" | "child" | "childOf" | "withParent" => diag.hint("`CancellationSource.linked(read token)` is cancelled when the token's source is"),
                _ => diag.hint("`CancellationSource.after(ms)` cancels itself after that many milliseconds"),
            });
        }
        if args.len() != 1 {
            return Err(Diag::error(span, format!("`CancellationSource.after` takes 1 argument(s), {} given", args.len())));
        }
        self.typed(&mut args[0], &Ty::Int(IntTy::Int)).map_err(|diag| diag.note("`CancellationSource.after(ms)` takes the milliseconds as an `int`"))?;
        *callee = Callee::Cancel(CancelCall::After);
        Ok(Ty::Source)
    }

    /// `source.cancel()`, `source.token()`, `token.check()` and the other methods of a source
    /// and a token. All of them work through a `read` borrow: a token only looks, and
    /// cancelling twice is cancelling once.
    #[allow(clippy::too_many_arguments)]
    fn cancel_method(&mut self, recv: &Expr, target: &Ty, name: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span) -> Res<Ty> {
        let call = match (target, name) {
            (Ty::Source, "cancel") => CancelCall::Cancel,
            (Ty::Source, "cancelAfter") => CancelCall::CancelAfter,
            (Ty::Source, "token") => CancelCall::Token,
            (_, "isCancelled") => CancelCall::IsCancelled,
            (Ty::Token, "check") => CancelCall::Check,
            (Ty::Token, "canBeCancelled") => CancelCall::CanBeCancelled,
            (Ty::Token, "clone") => CancelCall::Clone,
            _ => {
                let known: &[&str] = match target {
                    Ty::Source => &["cancel", "cancelAfter", "token", "isCancelled"],
                    _ => &["isCancelled", "check", "clone", "canBeCancelled"],
                };
                let diag = Diag::error(name_span, format!("`{target}` has no method `{name}`"));
                let diag = match nearest(name, known.iter().copied()) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag.note(format!("methods: {}", known.join(", "))),
                };
                let text = &self.src[recv.span.start..recv.span.end];
                return Err(match (target, name) {
                    (Ty::Token, "cancel") => diag.note("a token only looks; its source cancels it: `source.cancel()`"),
                    (Ty::Token, "cancelAfter") => diag.note("a token only looks; its source cancels it: `source.cancelAfter(ms)`"),
                    (Ty::Source, "check") => diag.hint(format!("a token looks: `{text}.token().check()`, or `{text}.isCancelled()`")),
                    (Ty::Source, "canBeCancelled") => diag.note("a source can always be cancelled; `canBeCancelled()` tells a token of one from `CancellationToken.None`"),
                    (Ty::Source, "clone") => diag.note("a source has one owner; `token()` gives as many tokens of it as are needed"),
                    _ => diag,
                });
            }
        };
        if call == CancelCall::CancelAfter {
            if args.len() != 1 {
                return Err(Diag::error(span, format!("`cancelAfter` takes 1 argument(s), {} given", args.len()))
                    .hint(format!("`{}.cancelAfter(30000)` cancels it 30 seconds from now", &self.src[recv.span.start..recv.span.end])));
            }
            self.typed(&mut args[0], &Ty::Int(IntTy::Int)).map_err(|diag| diag.note("`cancelAfter(ms)` takes the milliseconds as an `int`"))?;
        } else if !args.is_empty() {
            return Err(Diag::error(span, format!("`{name}` takes 0 argument(s), {} given", args.len())));
        }
        *callee = Callee::Cancel(call);
        Ok(match call {
            CancelCall::Cancel | CancelCall::CancelAfter => Ty::Void,
            CancelCall::Token | CancelCall::Clone => Ty::Token,
            CancelCall::IsCancelled | CancelCall::CanBeCancelled => Ty::Bool,
            CancelCall::Check => Ty::Result(Box::new(Ty::Void), Box::new(Ty::Struct(CANCELLED.to_string()))),
            CancelCall::New | CancelCall::After | CancelCall::Linked => unreachable!("functions of the type, not methods"),
        })
    }

    /// The `id` of the channel a new `Sender` comes from, when this function made it: `var tx
    /// = jobs.sender()`, or `var more = tx.clone()` of such a sender.
    fn sender_of(&mut self, init: &Expr) -> Option<usize> {
        let ExprKind::Method { recv, callee: Callee::Chan(call, _), .. } = &init.kind else { return None };
        let recv = match &recv.kind {
            ExprKind::Borrow { expr, .. } => expr,
            _ => recv,
        };
        let ExprKind::Var(name) = &recv.kind else { return None };
        let var = self.find(name)?;
        match call {
            ChanCall::Sender => Some(var.id),
            ChanCall::Clone => var.sender_of,
            _ => None,
        }
    }

    /// `for x in jobs` ends only once every `Sender` of the channel has ended: one this function
    /// made, and has not given away on every path to here, would keep it from ever ending.
    fn channel_loop(&mut self, iter: &Expr) -> Res<()> {
        let source = match &iter.kind {
            ExprKind::Borrow { expr, .. } => expr,
            _ => iter,
        };
        let ExprKind::Var(channel) = &source.kind else { return Ok(()) };
        let Some(id) = self.find(channel).map(|var| var.id) else { return Ok(()) };
        let alive = self.scopes.iter().flat_map(|scope| &scope.vars).find(|var| var.sender_of == Some(id) && (var.moved.whole.is_none() || var.moved.partly));
        let Some(sender) = alive else { return Ok(()) };
        let (name, at) = (sender.name.clone(), sender.at);
        Err(Diag::error(iter.span, format!("this loop ends only when every sender of `{channel}` has ended, and `{name}` lives on in this function: it would wait for ever"))
            .related(at, format!("`{name}` is made here"))
            .hint(format!("give the last sender away, `Task.run(produce, {name})`, or make each sender where it is given: `Task.run(produce, {channel}.sender())`")))
    }

    /// `Channel<T>(capacity)`: a channel with room for that many items, at least one, whose
    /// items go to other tasks. A `Sender<T>` is made by the channel.
    fn new_channel(&mut self, built: &Ty, ty_span: Span, args: &mut [Expr], braced: bool, span: Span) -> Res<Ty> {
        let item = match built {
            Ty::Channel(item) => (**item).clone(),
            _ => {
                return Err(Diag::error(ty_span, format!("a `{built}` is made by its channel"))
                    .hint("`var tx = channel.sender()`"));
            }
        };
        if braced {
            return Err(Diag::error(span, "a channel is made empty, with room for a number of items: `Channel<int>(16)`")
                .hint("send the items into it: `tx.send(x)`"));
        }
        let [capacity] = args else {
            return Err(Diag::error(span, format!("`{built}(...)` takes how many items it has room for: `{built}(16)`"))
                .note("a channel always has a limit, so a sender that runs ahead waits instead of filling the memory"));
        };
        self.typed(capacity, &Ty::Int(IntTy::Nint))?;
        let written = match &unwidened(capacity).kind {
            ExprKind::Int(value, _) => Some(*value),
            ExprKind::Unary(UnOp::Neg, inner) => match &unwidened(inner).kind {
                ExprKind::Int(value, _) => Some(-*value),
                _ => None,
            },
            _ => None,
        };
        if let Some(value) = written.filter(|value| *value < 1) {
            return Err(Diag::error(capacity.span, format!("a channel has room for at least one item, and this is {value}"))
                .note("a sender that waits for its receiver is a task waited for: `task.wait()`"));
        }
        if let Some((part, at)) = stays(&item, self.structs, self.enums, self.names, &mut Vec::new()) {
            let what = if at.is_empty() { String::new() } else { at };
            return Err(Diag::error(ty_span, format!("an item of a channel goes to another task, and a `{item}` cannot{what}: {}", why_it_stays(&part, self.names))).note(THREAD_RULE));
        }
        if holds_writer(&item, self.structs, self.enums, &mut Vec::new()) {
            return Err(Diag::error(ty_span, format!("a channel of a `{item}`, which holds a `StreamWriter`, is not in the interpreter yet"))
                .note("a writer left in a channel would be closed where the channel ends, which async code has to await")
                .hint("send what is to be written, and keep the writer in the task that writes"));
        }
        Ok(built.clone())
    }

    /// The methods of a `TaskGroup` (1716 to 1719): `add(task)`, `count()` and `whenRoom()`
    /// through a `read` borrow, since the group keeps its tasks safe itself, and `whenAll()`,
    /// which takes the group. The last
    /// two are awaited, or waited for, where they are written; with a token last they give a
    /// `Result` whose error is `Cancelled`.
    #[allow(clippy::too_many_arguments)]
    fn group_method(&mut self, recv: &mut Expr, recv_ty: &Ty, is_place: bool, name: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span, awaited: bool) -> Res<Ty> {
        let text = self.src[recv.span.start..recv.span.end].to_string();
        let call = match name {
            "add" => GroupCall::Add,
            "count" => GroupCall::Count,
            "whenRoom" => GroupCall::WhenRoom,
            "whenAll" => GroupCall::WhenAll,
            _ => {
                let known = ["add", "count", "whenRoom", "whenAll"];
                let diag = Diag::error(name_span, format!("`TaskGroup` has no method `{name}`"));
                return Err(match name {
                    "wait" | "await" | "join" | "close" => diag.hint(match self.is_async {
                        true => format!("`await {text}.whenAll()` waits for its tasks"),
                        false => format!("`{text}.whenAll().wait()` waits for its tasks"),
                    }),
                    "cancel" => diag.note("a group's tasks are cut where it is waited for: `whenAll(read token)` lets them go once the token is cancelled, and still waits for them"),
                    "length" | "size" => diag.hint("`count()`"),
                    _ => match nearest(name, known.into_iter()) {
                        Some(close) => diag.hint(format!("`{close}`")),
                        None => diag.note(format!("methods: {}", known.join(", "))),
                    },
                });
            }
        };
        match call {
            GroupCall::Add => {
                if args.len() != 1 {
                    return Err(Diag::error(span, format!("`add` takes 1 argument(s), {} given", args.len())));
                }
                let task = Ty::Task(Box::new(Ty::Void));
                self.args(name, args, std::slice::from_ref(&task), span).map_err(|diag| match args[0].ty.strip_borrow() {
                    Ty::Task(result) if **result != Ty::Void => diag
                        .note("a group lets go of each task as it ends, so what a task gives would be lost: it takes tasks that give nothing")
                        .hint("start an `async void` function that does what is to be done with what the task gives, an error among it"),
                    _ => diag,
                })?;
                self.touch(recv, Access::Read);
                *callee = Callee::Group(GroupCall::Add, false);
                Ok(Ty::Void)
            }
            GroupCall::Count => {
                if !args.is_empty() {
                    return Err(Diag::error(span, format!("`count` takes 0 argument(s), {} given", args.len())));
                }
                self.touch(recv, Access::Read);
                *callee = Callee::Group(GroupCall::Count, false);
                Ok(Ty::Int(IntTy::Nint))
            }
            GroupCall::WhenRoom | GroupCall::WhenAll => {
                if args.len() > 1 {
                    return Err(Diag::error(span, format!("`{name}` takes 0 argument(s), or 1 with a token; {} given", args.len())));
                }
                let told = match args.first_mut() {
                    Some(token) => {
                        self.token_arg(token)?;
                        true
                    }
                    None => false,
                };
                if !awaited {
                    let diag = Diag::error(span, format!("`{name}` is awaited, or waited for, where it is written"));
                    let written = &self.src[span.start..span.end];
                    return Err(match self.is_async {
                        true => diag.hint(format!("`await {written}`")),
                        false => diag.hint(format!("`{written}.wait()`")),
                    });
                }
                if call == GroupCall::WhenAll {
                    if recv_ty.is_borrow() || !is_place {
                        return Err(Diag::error(recv.span, "`whenAll` takes the group and waits for every task in it, and this one is only borrowed")
                            .note("whoever made the group waits for its tasks"));
                    }
                    self.move_out(recv, recv_ty)?;
                    recv.moved = true;
                } else {
                    self.touch(recv, Access::Read);
                }
                *callee = Callee::Group(call, told);
                Ok(Ty::Task(Box::new(match told {
                    true => Ty::Result(Box::new(Ty::Void), Box::new(Ty::Struct(CANCELLED.to_string()))),
                    false => Ty::Void,
                })))
            }
            GroupCall::New(_) => unreachable!("a function of the type"),
        }
    }

    /// The methods of the ends of a channel: `channel.sender()`, `.clone()`, `sender.send(x)`,
    /// `channel.receive()` and their async versions, each through a `read` borrow, since the
    /// channel keeps its items safe for several tasks itself. A token may come last on those
    /// that wait; then what they give is in a `Result` whose error is `Cancelled`. The async
    /// versions lend the end, so they are awaited where they are written.
    #[allow(clippy::too_many_arguments)]
    fn chan_method(&mut self, recv: &mut Expr, target: &Ty, name: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span, awaited: bool) -> Res<Ty> {
        let (receives, item) = match target {
            Ty::Channel(item) => (true, (**item).clone()),
            Ty::Sender(item) => (false, (**item).clone()),
            _ => unreachable!("called for the ends of a channel"),
        };
        let (base, is_async) = match name.strip_suffix("Async") {
            Some(base) => (base, true),
            None => (name, false),
        };
        let text = self.src[recv.span.start..recv.span.end].to_string();
        let (call, params) = match (receives, base) {
            (true, "sender") if !is_async => (ChanCall::Sender, Vec::new()),
            (_, "clone") if !is_async => (ChanCall::Clone, Vec::new()),
            (false, "send") => (ChanCall::Send, vec![item.clone()]),
            (true, "receive") => (ChanCall::Receive, Vec::new()),
            _ => {
                let known: &[&str] = match receives {
                    true => &["receive", "receiveAsync", "sender", "clone"],
                    false => &["send", "sendAsync", "clone"],
                };
                let diag = Diag::error(name_span, format!("`{target}` has no method `{name}`"));
                return Err(match (receives, base) {
                    (true, "send") => diag.note("a `Channel<T>` is the end that receives; its senders send").hint(format!("`var tx = {text}.sender()`, then `tx.{name}(...)`")),
                    (false, "receive") => diag.note("a `Sender<T>` only sends; the `Channel<T>` it was made from receives"),
                    (false, "sender") => diag.hint(format!("one more sender: `{text}.clone()`")),
                    (_, "close" | "complete" | "dispose") => diag.note("a channel closes when its last `Sender` ends: there is nothing to call, and nothing to forget"),
                    (_, "tryReceive" | "trySend" | "tryRead" | "tryWrite" | "length" | "count" | "isEmpty" | "isFull") => {
                        diag.note("a loop that asks a channel without waiting would hold its thread; `receive` and `send` wait, and a token gives up after a time: `CancellationSource.after(ms)`")
                    }
                    _ => match nearest(name, known.iter().copied()) {
                        Some(close) => diag.hint(format!("`{close}`")),
                        None => diag.note(format!("methods: {}", known.join(", "))),
                    },
                });
            }
        };
        let waits = matches!(call, ChanCall::Send | ChanCall::Receive);
        let count = params.len();
        match waits {
            true if args.len() != count && args.len() != count + 1 => {
                return Err(Diag::error(span, format!("`{name}` takes {count} argument(s), or {} with a token last; {} given", count + 1, args.len())));
            }
            false if args.len() != count => return Err(Diag::error(span, format!("`{name}` takes {count} argument(s), {} given", args.len()))),
            _ => {}
        }
        let (given, token) = args.split_at_mut(count);
        self.args(name, given, &params, span)?;
        let told = match token.first_mut() {
            Some(token) => {
                self.token_arg(token)?;
                true
            }
            None => false,
        };
        self.touch(recv, Access::Read);
        let gives = match call {
            ChanCall::Sender => Ty::Sender(Box::new(item)),
            ChanCall::Clone => target.clone(),
            ChanCall::Send => Ty::Bool,
            ChanCall::Receive => Ty::Nullable(Box::new(item)),
        };
        let gives = match told {
            true => Ty::Result(Box::new(gives), Box::new(Ty::Struct(CANCELLED.to_string()))),
            false => gives,
        };
        if !is_async {
            *callee = Callee::Chan(call, told);
            return Ok(gives);
        }
        if !awaited {
            let text = &self.src[span.start..span.end];
            let diag = Diag::error(span, format!("`{name}` lends the end of the channel, and a call without `await` starts it as a task of its own, which a borrow cannot go to"));
            return Err(match self.is_async {
                true => diag.hint(format!("`await {text}` runs it as part of this task, where it can borrow")),
                false => diag.note(format!("a channel is awaited in an async function; elsewhere `{base}` does the same work")),
            });
        }
        *callee = Callee::ChanAsync(call, told);
        Ok(Ty::Task(Box::new(gives)))
    }

    /// `reader.readLineAsync(ref line)`, `Stdin.readTextAsync()`: the methods of a reader as a
    /// task, with a token that may come last. The task lends the reader, so the call is
    /// awaited where it is written.
    #[allow(clippy::too_many_arguments)]
    fn stream_async(
        &mut self,
        recv: &mut Expr,
        target: &Ty,
        writable: bool,
        borrowed: bool,
        base: &str,
        name: &str,
        name_span: Span,
        args: &mut [Expr],
        callee: &mut Callee,
        span: Span,
        awaited: bool,
    ) -> Res<Ty> {
        let count = match (target, base) {
            (Ty::Reader(_), "readText" | "fill") => 0,
            (Ty::Reader(_), "readLine" | "readLineBytes" | "readBytes" | "readSome") => 1,
            (Ty::Reader(_), "buffered" | "consume") => {
                let reader = &self.src[recv.span.start..recv.span.end];
                return Err(Diag::error(name_span, format!("a `StreamReader` has no `{name}`: `{base}` does not wait"))
                    .hint(format!("`{reader}.{base}({})`, in an async function too; `fillAsync` is what waits", if base == "consume" { "n" } else { "" })));
            }
            (Ty::Reader(_), "lines") => {
                let reader = &self.src[recv.span.start..recv.span.end];
                return Err(Diag::error(name_span, "a `StreamReader` has no `linesAsync`: a `for` does not await")
                    .hint(format!("read line by line: `while try await {reader}.readLineAsync(ref line) {{ ... }}`")));
            }
            (Ty::Writer, "write" | "writeLine" | "writeBytes") => 1,
            (Ty::Writer, "flush" | "syncData" | "sync") => 0,
            (Ty::Writer, "setLength") => 1,
            // `closeAsync` takes the writer, and is not cancelled: what the writer holds is
            // written, as when it ends without `close`.
            (Ty::Writer, "close") => {
                if let Some((var, what)) = match &recv.kind {
                    ExprKind::Var(var) => std_stream(var).map(|what| (var, what)),
                    _ => None,
                } {
                    return Err(Diag::error(recv.span, format!("`{var}` is {what} of the program and stays open until it ends"))
                        .hint(format!("`try await {var}.flushAsync()` writes what it holds")));
                }
                if borrowed {
                    return Err(Diag::error(recv.span, "`closeAsync` takes the writer and uses it up, but this writer is only borrowed")
                        .note("whoever owns the writer closes it")
                        .hint("`flushAsync()` writes what it holds through a borrow"));
                }
                if !args.is_empty() {
                    return Err(Diag::error(span, format!("`closeAsync` takes 0 argument(s), {} given", args.len()))
                        .note("closing is not cancelled, so it takes no token: what the writer holds is written and the file is closed"));
                }
                0
            }
            // A writer writes what it holds before it moves, so it seeks as a task; a reader only
            // asks the system where the file stands, quick as `File.delete`, and has no async seek.
            (Ty::Writer, "seek") => 1,
            (_, "seek") => {
                let stream = &self.src[recv.span.start..recv.span.end];
                return Err(Diag::error(name_span, "`StreamReader` has no async `seek`")
                    .note(format!("moving a reader is quick and is made as it is, also in an async function: `{stream}.seek(...)`")));
            }
            _ => {
                let known: &[&str] = match target {
                    Ty::Reader(_) => &["readLineAsync", "readLineBytesAsync", "readTextAsync", "readBytesAsync", "readSomeAsync", "fillAsync"],
                    _ => &["writeAsync", "writeLineAsync", "writeBytesAsync", "flushAsync", "closeAsync", "seekAsync", "syncDataAsync", "syncAsync", "setLengthAsync"],
                };
                let diag = Diag::error(name_span, format!("`{target}` has no method `{name}`"));
                return Err(match nearest(name, known.iter().copied()) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag.note(format!("async methods: {}", known.join(", "))),
                });
            }
        };
        if args.len() != count && args.len() != count + 1 {
            return Err(Diag::error(span, format!("`{name}` takes {count} argument(s), or {} with a token last; {} given", count + 1, args.len())));
        }
        let (given, token) = match args.len() == count + 1 {
            true => args.split_at_mut(count),
            false => (args, &mut [][..]),
        };
        let ty = self.stream_method(recv, target, writable, borrowed, base, name, name_span, given, callee, span)?;
        if let Some(token) = token.first_mut() {
            self.token_arg(token)?;
        }
        let Callee::Stream(call) = *callee else { unreachable!("a method of a stream") };
        if !awaited {
            let text = &self.src[span.start..span.end];
            let stream = if matches!(target, Ty::Reader(_)) { "reader" } else { "writer" };
            let diag = match call {
                StreamCall::Close => Diag::error(span, "`closeAsync` is awaited where it is written, so the file is closed before the next line"),
                _ => Diag::error(span, format!("`{name}` lends the {stream}, and a call without `await` starts it as a task of its own, which a borrow cannot go to")),
            };
            return Err(match (self.is_async, matches!(target, Ty::Reader(_))) {
                (true, _) if call == StreamCall::Close => diag.hint(format!("`await {text}`")),
                (true, _) => diag.hint(format!("`await {text}` runs it as part of this task, where it can borrow")),
                (false, true) => diag.note("async input is awaited in an async function; elsewhere the methods without `Async` do the same work"),
                (false, false) => diag.note("async output is awaited in an async function; elsewhere the methods without `Async` do the same work"),
            });
        }
        *callee = Callee::StreamAsync(call);
        Ok(Ty::Task(Box::new(ty)))
    }

    /// `Memory.free(p)`, `Memory.copy(source, target, count)`; `Memory.allocate<T>(n)` is an
    /// `Allocate`.
    ///
    /// `C.errno()` and `C.setErrno(n)`: the `errno` of the last call of C, and of the next.
    /// `C.lastError()` and `C.setLastError(n)`: the system's last error, `errno` on Linux and
    /// macOS and `GetLastError()` on Windows. None touches memory, so none is written inside
    /// `unsafe`.
    fn c_call(&mut self, name: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span) -> Res<Ty> {
        match name {
            "errno" => {
                self.args(name, args, &[], span)?;
                *callee = Callee::Memory(MemoryCall::Errno);
                Ok(Ty::Int(IntTy::Int))
            }
            "setErrno" => {
                self.args(name, args, &[Ty::Int(IntTy::Int)], span)?;
                *callee = Callee::Memory(MemoryCall::SetErrno);
                Ok(Ty::Void)
            }
            "lastError" => {
                self.args(name, args, &[], span)?;
                *callee = Callee::Memory(MemoryCall::LastError);
                Ok(Ty::Int(IntTy::Int))
            }
            "setLastError" => {
                self.args(name, args, &[Ty::Int(IntTy::Int)], span)?;
                *callee = Callee::Memory(MemoryCall::SetLastError);
                Ok(Ty::Void)
            }
            _ => {
                let diag = Diag::error(name_span, format!("`C` has no function `{name}`"));
                Err(match nearest(name, ["errno", "setErrno", "lastError", "setLastError"].into_iter()) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag.note("functions for now: errno, setErrno, lastError, setLastError; the functions of C are declared in an `extern` block"),
                })
            }
        }
    }

    fn memory_call(&mut self, name: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span) -> Res<Ty> {
        let pointer = |checker: &mut Self, arg: &mut Expr, what: &str| -> Res<Ty> {
            let found = checker.expr(arg, None)?;
            let stripped = found.strip_borrow().clone();
            if !matches!(stripped, Ty::Raw(_)) {
                return Err(Diag::error(arg.span, format!("{what} is a raw pointer, and this is `{found}`")));
            }
            checker.coerce(arg, found, &stripped)?;
            Ok(stripped)
        };
        match name {
            "allocate" => Err(Diag::error(name_span, "`Memory.allocate` names the type of what it makes room for").hint("`Memory.allocate<int>(count)`")),
            "sizeOf" | "alignOf" => Err(Diag::error(name_span, format!("`Memory.{name}` names the type it measures")).hint(format!("`Memory.{name}<int>()`"))),
            "free" => {
                if args.len() != 1 {
                    return Err(Diag::error(span, format!("`free` takes 1 argument(s), {} given", args.len())));
                }
                pointer(self, &mut args[0], "what `Memory.free` gives back")?;
                self.needs_unsafe(span, "`Memory.free` gives memory back, and a pointer to it may still be used")?;
                *callee = Callee::Memory(MemoryCall::Free);
                Ok(Ty::Void)
            }
            "copy" => {
                if args.len() != 3 {
                    return Err(Diag::error(span, format!("`copy` takes 3 argument(s), {} given", args.len()))
                        .note("`Memory.copy(source, target, count)` copies `count` items"));
                }
                let source = pointer(self, &mut args[0], "what `Memory.copy` copies from")?;
                let target = pointer(self, &mut args[1], "what `Memory.copy` copies to")?;
                if source == Ty::Raw(Box::new(Ty::Void)) {
                    return Err(Diag::error(args[0].span, "a `raw void` points at memory of no known type, so `Memory.copy` cannot count its items")
                        .hint("say what lies there with a cast: `(raw byte)p`"));
                }
                if source != target {
                    return Err(Diag::error(args[1].span, format!("`Memory.copy` copies between two pointers of one type, and these are `{source}` and `{target}`")));
                }
                self.index(&mut args[2], "a count of items")?;
                self.needs_unsafe(span, "`Memory.copy` writes raw memory, whose size the checker does not know")?;
                *callee = Callee::Memory(MemoryCall::Copy);
                Ok(Ty::Void)
            }
            // `Memory.addressOf(ref state)`: what a callback is given back by C.
            "addressOf" => {
                if args.len() != 1 {
                    return Err(Diag::error(span, format!("`addressOf` takes 1 argument(s), {} given", args.len())));
                }
                let named = |expr: &Expr| {
                    let mut at = expr;
                    loop {
                        match &at.kind {
                            ExprKind::Var(_) => return true,
                            ExprKind::Field { recv, .. } => at = recv,
                            _ => return false,
                        }
                    }
                };
                let lent = matches!(&args[0].kind, ExprKind::Borrow { mutable: true, expr } if named(expr));
                if !lent {
                    let text = &self.src[args[0].span.start..args[0].span.end];
                    let diag = Diag::error(args[0].span, "`Memory.addressOf` takes a variable or a field lent with `ref`");
                    return Err(match &args[0].kind {
                        ExprKind::Var(_) | ExprKind::Field { .. } => diag.hint(format!("`Memory.addressOf(ref {text})`")),
                        ExprKind::Borrow { mutable: false, expr } if named(expr) => diag.hint(format!("`Memory.addressOf(ref {})`", &self.src[expr.span.start..expr.span.end])),
                        _ => diag.note("the address must stay where the value is: a variable or a field keeps it, a value made in the call does not"),
                    });
                }
                self.expr(&mut args[0], None)?;
                self.needs_unsafe(span, "the address outlives the borrow, and the checker cannot see where it goes")?;
                *callee = Callee::Memory(MemoryCall::AddressOf);
                Ok(Ty::Raw(Box::new(Ty::Void)))
            }
            // `Memory.view(p, count)`: the items C put at `p`, as a window that looks at them
            // where they lie. It borrows the variable that holds the pointer, so it does not
            // outlive it. `Memory.viewRef(p, count)` writes them, and borrows it as `ref`.
            "view" | "viewRef" => {
                let writes = name == "viewRef";
                let call = format!("Memory.{name}");
                if args.len() != 2 {
                    return Err(Diag::error(span, format!("`{name}` takes 2 argument(s), {} given", args.len()))
                        .note(format!("`{call}(p, count)` {} `count` items from `p`", if writes { "writes" } else { "looks at" })));
                }
                let named = |expr: &Expr| {
                    let mut at = expr;
                    loop {
                        match &at.kind {
                            ExprKind::Var(_) => return true,
                            ExprKind::Field { recv, .. } => at = recv,
                            _ => return false,
                        }
                    }
                };
                if !named(&args[0]) {
                    let text = &self.src[args[0].span.start..args[0].span.end];
                    return Err(Diag::error(args[0].span, format!("`{call}` takes the pointer from a variable or a field, which the window borrows"))
                        .hint(format!("`var at = {text}` first, then `{call}(at, count)`"))
                        .note("the window borrows the variable that holds its pointer, and does not outlive it"));
                }
                let item = pointer(self, &mut args[0], &format!("what `{call}` {}", if writes { "writes" } else { "looks at" }))?;
                let Ty::Raw(item) = item else { unreachable!("`pointer` gives a raw pointer") };
                if *item == Ty::Void {
                    return Err(Diag::error(args[0].span, format!("a `raw void` points at memory of no known type, so `{call}` cannot count its items"))
                        .hint("say what lies there with a cast: `(raw byte)p`"));
                }
                self.index(&mut args[1], "a count of items")?;
                let verb = if writes { "writes" } else { "reads" };
                self.needs_unsafe(span, &format!("`{call}` {verb} raw memory, whose size the checker does not know"))?;
                Ok(match writes {
                    true => {
                        *callee = Callee::Memory(MemoryCall::ViewRef);
                        Ty::Ref(Box::new(Ty::Slice(item)))
                    }
                    false => {
                        *callee = Callee::Memory(MemoryCall::View);
                        Ty::Read(Box::new(Ty::Slice(item)))
                    }
                })
            }
            // `Memory.readUnaligned(p)` and `Memory.writeUnaligned(p, value)`: an item at an
            // address that need not be a multiple of its alignment.
            // `Memory.readVolatile(p[i].status)`: a place in raw memory, as it is written
            // elsewhere, so that a field of a block of registers is reached as C reaches it.
            "readVolatile" | "writeVolatile" => {
                let writes = name == "writeVolatile";
                let wanted = if writes { 2 } else { 1 };
                if args.len() != wanted {
                    let diag = Diag::error(span, format!("`{name}` takes {wanted} argument(s), {} given", args.len()));
                    return Err(diag.note(if writes { "`Memory.writeVolatile(p[0], value)` writes the item at `p`" } else { "`Memory.readVolatile(p[0])` reads the item at `p`" }));
                }
                self.needs_unsafe(span, &format!("`Memory.{name}` {} raw memory, which the checker does not follow", if writes { "writes" } else { "reads" }))?;
                let found = match self.place(&mut args[0])? {
                    Some((ty, _)) if Expr::in_raw_memory(&args[0]) => ty,
                    found => {
                        let found = match found {
                            Some((ty, _)) => ty,
                            None => self.expr(&mut args[0], None)?,
                        };
                        let text = &self.src[args[0].span.start..args[0].span.end];
                        let diag = Diag::error(args[0].span, format!("`Memory.{name}` {} a place in raw memory, and this is not one", if writes { "writes" } else { "reads" }))
                            .note("a volatile access is for memory the program does not own alone: the registers of a device, memory another program writes");
                        return Err(match found.strip_borrow() {
                            Ty::Raw(_) => diag.hint(format!("the item it points at: `{text}[0]`")),
                            _ => diag,
                        });
                    }
                };
                if let Some(field) = bit_field_at(&args[0], self.structs) {
                    let text = &self.src[field.start..field.end];
                    return Err(Diag::error(args[0].span, format!("`Memory.{name}` reaches a place by its address, and a bit field has none"))
                        .hint(format!("{} the struct that holds it whole: `{text}`", if writes { "write" } else { "read" })));
                }
                if let Some(diag) = not_lent(&args[0], self.structs, self.src).filter(|_| union_field(&args[0], self.structs).is_none()) {
                    return Err(diag);
                }
                if matches!(found, Ty::Atomic(_)) {
                    return Err(Diag::error(args[0].span, format!("an `{found}` is read and written by its own methods, not as `volatile`"))
                        .hint("`load` and `store`, inside `unsafe` for an item of raw memory"));
                }
                if writes {
                    let given = self.expr(&mut args[1], Some(&found))?;
                    self.coerce(&mut args[1], given, &found)?;
                }
                *callee = Callee::Memory(if writes { MemoryCall::WriteVolatile } else { MemoryCall::ReadVolatile });
                Ok(if writes { Ty::Void } else { found })
            }
            // `Memory.zeroSecurely(ref key)`: zeros that the C compiler does not leave out
            // because nothing reads them afterwards. A window is checked, so no `unsafe`.
            "zeroSecurely" => {
                if args.len() != 1 {
                    return Err(Diag::error(span, format!("`zeroSecurely` takes 1 argument(s), {} given", args.len()))
                        .note("`Memory.zeroSecurely(ref buffer)` writes zeros over the items"));
                }
                let found = self.expr(&mut args[0], None)?;
                let item = match &found {
                    Ty::Ref(inner) => match &**inner {
                        Ty::Slice(item) | Ty::Array(item, _) | Ty::List(item) => Some((**item).clone()),
                        _ => None,
                    },
                    _ => None,
                };
                let Some(item) = item else {
                    let text = &self.src[args[0].span.start..args[0].span.end];
                    let diag = Diag::error(args[0].span, format!("`Memory.zeroSecurely` writes the items of a `ref T[]`, and this is `{found}`"));
                    return Err(match found.strip_borrow() {
                        Ty::Array(..) | Ty::List(_) | Ty::Slice(_) if !matches!(found, Ty::Ref(_)) => {
                            diag.hint(format!("lend it with `ref`: `ref {}`", text.strip_prefix("read ").unwrap_or(text)))
                        }
                        _ => diag,
                    });
                };
                if let Some(part) = not_plain(&item, self.structs) {
                    let diag = Diag::error(args[0].span, format!("`Memory.zeroSecurely` writes zeros over plain data, and `{item}` is not"));
                    let diag = if part == item { diag } else { diag.note(format!("it holds a `{part}`")) };
                    return Err(diag.note("a zero is a value of plain data only: what owns memory or counts its owners would lose it"));
                }
                self.coerce(&mut args[0], found, &Ty::Ref(Box::new(Ty::Slice(Box::new(item)))))?;
                *callee = Callee::Memory(MemoryCall::ZeroSecurely);
                Ok(Ty::Void)
            }
            "readUnaligned" | "writeUnaligned" => {
                let writes = name == "writeUnaligned";
                let wanted = if writes { 2 } else { 1 };
                if args.len() != wanted {
                    let diag = Diag::error(span, format!("`{name}` takes {wanted} argument(s), {} given", args.len()));
                    return Err(diag.note(if writes { "`Memory.writeUnaligned(p, value)` writes the item at `p`" } else { "`Memory.readUnaligned(p)` reads the item at `p`" }));
                }
                let found = pointer(self, &mut args[0], &format!("what `Memory.{name}` {}", if writes { "writes" } else { "reads" }))?;
                let Ty::Raw(item) = found else { unreachable!("`pointer` gives a raw pointer") };
                if *item == Ty::Void {
                    return Err(Diag::error(args[0].span, format!("a `raw void` points at memory of no known type, so `Memory.{name}` does not know what lies there"))
                        .hint("say what lies there with a cast: `(raw uint)p`"));
                }
                if writes {
                    let given = self.expr(&mut args[1], Some(&item))?;
                    self.coerce(&mut args[1], given, &item)?;
                }
                self.needs_unsafe(span, &format!("`Memory.{name}` {} raw memory, which the checker does not follow", if writes { "writes" } else { "reads" }))?;
                *callee = Callee::Memory(if writes { MemoryCall::WriteUnaligned } else { MemoryCall::ReadUnaligned });
                Ok(if writes { Ty::Void } else { *item })
            }
            _ => {
                let diag = Diag::error(name_span, format!("`Memory` has no function `{name}`"));
                let all = ["allocate", "free", "copy", "addressOf", "view", "viewRef", "readUnaligned", "writeUnaligned", "readVolatile", "writeVolatile", "sizeOf", "alignOf", "zeroSecurely"];
                Err(match nearest(name, all.into_iter()) {
                    Some(close @ ("sizeOf" | "alignOf")) => diag.hint(format!("`Memory.{close}<int>()`")),
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag.note(format!("functions for now: {}", all.join(", "))),
                })
            }
        }
    }

    /// `File.readText(path)`, `Stdin.readLine(ref line)`: what reads and writes files and
    /// the input. A path is a `read string`; each gives a `Result` with an `IoError`.
    #[allow(clippy::too_many_arguments)]
    fn io_call(&mut self, owner: &str, name: &str, name_span: Span, args: &mut Vec<Expr>, callee: &mut Callee, span: Span, awaited: bool) -> Res<Ty> {
        let text = || Ty::Read(Box::new(Ty::Str));
        let bytes = Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
        // `File.readTextAsync(path)`: the same work as a task, with a token that may come last.
        let (base, is_async) = match name.strip_suffix("Async") {
            Some(base) => (base, true),
            None => (name, false),
        };
        let (call, params, ok) = match (owner, base) {
            ("File", "readText") => (IoCall::ReadText, vec![text()], Ty::Str),
            ("File", "readBytes") => (IoCall::ReadBytes, vec![text()], Ty::List(Box::new(Ty::Int(IntTy::Byte)))),
            ("File", "writeText") => (IoCall::WriteText, vec![text(), text()], Ty::Void),
            ("File", "writeBytes") => (IoCall::WriteBytes, vec![text(), bytes], Ty::Void),
            ("File", "appendText") => (IoCall::AppendText, vec![text(), text()], Ty::Void),
            ("File", "openText") => (IoCall::OpenText, vec![text()], Ty::Reader(false)),
            ("File", "openRegular") => (IoCall::OpenRegular, vec![text()], Ty::Reader(false)),
            ("File", "createText") => (IoCall::CreateText, vec![text()], Ty::Writer),
            ("File", "openAppend") => (IoCall::OpenAppend, vec![text()], Ty::Writer),
            ("File", "openWrite") => (IoCall::OpenWrite, vec![text()], Ty::Writer),
            ("File", "delete" | "move" | "replace" | "copy") if !is_async => {
                let (call, params) = match base {
                    "delete" => (SystemCall::Delete, vec![text()]),
                    "move" => (SystemCall::Move, vec![text(), text()]),
                    "replace" => (SystemCall::Replace, vec![text(), text()]),
                    _ => (SystemCall::Copy, vec![text(), text()]),
                };
                self.args(name, args, &params, span)?;
                *callee = Callee::System(call);
                return Ok(Ty::Result(Box::new(Ty::Void), Box::new(Ty::Enum(IO_ERROR.to_string()))));
            }
            // The same as a task, done on a thread of input and output (1871).
            ("File", "delete" | "move" | "replace" | "copy") => {
                let (call, params) = match base {
                    "delete" => (SystemCall::Delete, vec![text()]),
                    "move" => (SystemCall::Move, vec![text(), text()]),
                    "replace" => (SystemCall::Replace, vec![text(), text()]),
                    _ => (SystemCall::Copy, vec![text(), text()]),
                };
                self.async_args(name, args, &params, span)?;
                self.awaited_here(&format!("File.{name}"), awaited, args, span)?;
                *callee = Callee::SystemAsync(call);
                return Ok(Ty::Task(Box::new(Ty::Result(Box::new(Ty::Void), Box::new(Ty::Enum(IO_ERROR.to_string()))))));
            }
            // Done in the language's own module: `File.info(path)` is `FileInfo.of(path)`, and
            // `File.createTemp()` is `FileInfo.makeTemp()`; `File.infoAsync(path)` is
            // `FileInfo.ofAsync(path)` (1871).
            ("File", "info" | "exists" | "createTemp") => {
                let owner = format!("{STD_MODULE}.FileInfo");
                let inner = match (base, is_async) {
                    ("info", false) => "of",
                    ("exists", false) => "isFile",
                    (_, false) => "makeTemp",
                    ("info", true) => "ofAsync",
                    ("exists", true) => "isFileAsync",
                    (_, true) => "makeTempAsync",
                };
                let Some(sig) = self.methods_of(&owner).get(inner) else { unreachable!("a program that names `File.{base}` gets the file of `FileInfo`") };
                self.fill_defaults(name, sig, args, span)?;
                self.args(name, args, &sig.params, span)?;
                *callee = Callee::Static { owner, name: inner.to_string() };
                return self.method_gives(name, sig, None, args, span, awaited);
            }
            _ => {
                let known: &[&str] = match is_async {
                    true => &["readTextAsync", "readBytesAsync", "writeTextAsync", "writeBytesAsync", "appendTextAsync", "openTextAsync", "openRegularAsync", "createTextAsync", "openAppendAsync", "openWriteAsync", "deleteAsync", "moveAsync", "replaceAsync", "copyAsync", "infoAsync", "existsAsync", "createTempAsync"],
                    false => &["readText", "readBytes", "writeText", "writeBytes", "appendText", "openText", "openRegular", "createText", "openAppend", "openWrite", "delete", "move", "replace", "copy", "info", "exists", "createTemp"],
                };
                let diag = Diag::error(name_span, format!("`{owner}` has no function `{name}`"));
                return Err(match nearest(name, known.iter().copied()) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag.note(format!("functions for now: {}", known.join(", "))),
                });
            }
        };
        let result = Ty::Result(Box::new(ok), Box::new(Ty::Enum(IO_ERROR.to_string())));
        if !is_async {
            self.args(name, args, &params, span)?;
            *callee = Callee::Io(call);
            return Ok(result);
        }
        self.async_args(name, args, &params, span)?;
        self.awaited_here(&format!("File.{name}"), awaited, args, span)?;
        *callee = Callee::IoAsync(call);
        Ok(Ty::Task(Box::new(result)))
    }

    /// `System.list(path, ref names, ref kinds)` and the other calls of the system that the
    /// language's own module builds `Directory`, `FileInfo` and `Environment` on.
    fn system_call(&mut self, name: &str, args: &mut [Expr], callee: &mut Callee, span: Span, awaited: bool) -> Res<Ty> {
        let text = || Ty::Read(Box::new(Ty::Str));
        let bytes = || Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
        let io = |ok: Ty| Ty::Result(Box::new(ok), Box::new(Ty::Enum(IO_ERROR.to_string())));
        // The calls of the file system, and the same as a task done on a thread of input and
        // output while the task waits (1871).
        let disk = |name: &str| -> Option<(SystemCall, Vec<Ty>, Ty)> {
            Some(match name {
                "createDirectory" => (SystemCall::CreateDirectory, vec![text()], io(Ty::Void)),
                "removeDirectory" => (SystemCall::RemoveDirectory, vec![text()], io(Ty::Void)),
                "removeAll" => (SystemCall::RemoveAll, vec![text()], io(Ty::Void)),
                "syncDirectory" => (SystemCall::SyncDirectory, vec![text()], io(Ty::Void)),
                "list" => (
                    SystemCall::List,
                    vec![text(), Ty::Ref(Box::new(Ty::List(Box::new(Ty::Str)))), Ty::Ref(Box::new(Ty::List(Box::new(Ty::Int(IntTy::Byte)))))],
                    io(Ty::Void),
                ),
                "status" => (SystemCall::Status, vec![text(), Ty::Ref(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Long)))))], io(Ty::Void)),
                "makeTemp" => (SystemCall::MakeTemp, vec![text()], io(Ty::Str)),
                "makeTempFile" => (SystemCall::MakeTempFile, vec![text()], io(Ty::Str)),
                _ => return None,
            })
        };
        if let Some((call, params, ret)) = name.strip_suffix("Async").and_then(disk) {
            if !self.is_async {
                return Err(Diag::error(span, format!("`System.{name}` is awaited in an async function")));
            }
            self.async_args(name, args, &params, span)?;
            self.awaited_here(&format!("System.{name}"), awaited, args, span)?;
            *callee = Callee::SystemAsync(call);
            return Ok(Ty::Task(Box::new(ret)));
        }
        let (call, params, ret) = match name {
            "current" => (SystemCall::Current, Vec::new(), io(Ty::Str)),
            // `fstat` of the open file: no path is walked, so nothing waits, and an async
            // function calls it as it is (1873).
            "readerStatus" => (
                SystemCall::ReaderStatus,
                vec![Ty::Read(Box::new(Ty::Reader(false))), Ty::Ref(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Long)))))],
                io(Ty::Void),
            ),
            "tempFolder" => (SystemCall::TempFolder, Vec::new(), io(Ty::Str)),
            "variable" => (SystemCall::Variable, vec![text(), Ty::Ref(Box::new(Ty::List(Box::new(Ty::Int(IntTy::Byte)))))], Ty::Bool),
            "colourTerminal" => (SystemCall::ColourTerminal, Vec::new(), Ty::Bool),
            "stackLow" => (SystemCall::StackLow, Vec::new(), Ty::Bool),
            "charCategory" => (SystemCall::CharCategory, vec![Ty::Char], Ty::Int(IntTy::Int)),
            "charFold" => (SystemCall::CharFold, vec![Ty::Char], Ty::Char),
            "normalize" => (SystemCall::Normalize, vec![text(), Ty::Bool, Ty::Bool], Ty::Str),
            "isNormalized" => (SystemCall::IsNormalized, vec![text(), Ty::Bool, Ty::Bool], Ty::Bool),
            "crc32Hardware" => (SystemCall::Crc32Hardware, vec![Ty::Int(IntTy::Uint), bytes()], Ty::Int(IntTy::Long)),
            "sha256Hardware" => (
                SystemCall::Sha256Hardware,
                vec![Ty::Ref(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Uint))))), bytes()],
                Ty::Bool,
            ),
            "executable" => (SystemCall::Executable, vec![text()], Ty::Int(IntTy::Long)),
            "run" => {
                let texts = || Ty::Read(Box::new(Ty::List(Box::new(Ty::Str))));
                (
                    SystemCall::Run,
                    vec![
                        text(),
                        texts(),
                        text(),
                        Ty::Bool,
                        texts(),
                        texts(),
                        Ty::Read(Box::new(Ty::List(Box::new(Ty::Bool)))),
                        Ty::Int(IntTy::Long),
                        text(),
                        Ty::Bool,
                        Ty::Ref(Box::new(Ty::Str)),
                        Ty::Ref(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Long))))),
                    ],
                    io(Ty::Void),
                )
            }
            "start" => {
                let texts = || Ty::Read(Box::new(Ty::List(Box::new(Ty::Str))));
                (
                    SystemCall::Start,
                    vec![
                        text(),
                        texts(),
                        text(),
                        Ty::Bool,
                        texts(),
                        texts(),
                        Ty::Read(Box::new(Ty::List(Box::new(Ty::Bool)))),
                        Ty::Int(IntTy::Long),
                        text(),
                        Ty::Bool,
                        Ty::Bool,
                        Ty::Ref(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Long))))),
                    ],
                    io(Ty::Void),
                )
            }
            "wait" => (SystemCall::Wait, vec![Ty::Int(IntTy::Long), Ty::Ref(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Long)))))], Ty::Void),
            "waitAsync" => (
                SystemCall::WaitAsync,
                vec![Ty::Int(IntTy::Long), Ty::Read(Box::new(Ty::Token)), Ty::Ref(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Long)))))],
                Ty::Task(Box::new(io(Ty::Void))),
            ),
            "signal" => (SystemCall::Signal, vec![Ty::Int(IntTy::Long), Ty::Int(IntTy::Int)], Ty::Void),
            "signalListen" => (SystemCall::SignalListen, vec![Ty::Int(IntTy::Int)], Ty::Int(IntTy::Long)),
            "signalNext" => (SystemCall::SignalNext, vec![Ty::Int(IntTy::Long)], Ty::Int(IntTy::Long)),
            "signalNextAsync" => (
                SystemCall::SignalNextAsync,
                vec![Ty::Int(IntTy::Long), Ty::Read(Box::new(Ty::Token)), Ty::Ref(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Long)))))],
                Ty::Task(Box::new(Ty::Result(Box::new(Ty::Void), Box::new(Ty::Struct(CANCELLED.to_string()))))),
            ),
            "signalStop" => (SystemCall::SignalStop, vec![Ty::Int(IntTy::Long)], Ty::Void),
            "signalRaise" => (SystemCall::SignalRaise, vec![Ty::Int(IntTy::Int)], Ty::Void),
            "signalNumber" => (SystemCall::SignalNumber, vec![Ty::Int(IntTy::Int)], Ty::Int(IntTy::Int)),
            "reader" => (SystemCall::PipeReader, vec![Ty::Int(IntTy::Long)], Ty::Reader(false)),
            "writer" => (SystemCall::PipeWriter, vec![Ty::Int(IntTy::Long)], Ty::Writer),
            "udpBind" | "udpConnect" | "udpSend" | "udpSendAsync" | "udpReceive" | "udpReceiveAsync" | "socketAddress" | "streamAddress" | "closeSocket" | "lookup" | "lookupAsync" => {
                let fd = || Ty::Int(IntTy::Long);
                let address = || [Ty::Bool, Ty::Int(IntTy::Ulong), Ty::Int(IntTy::Ulong), Ty::Int(IntTy::Int)];
                let bits = || Ty::Ref(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Ulong)))));
                let buffer = |mode: fn(Box<Ty>) -> Ty| mode(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
                let token = || Ty::Read(Box::new(Ty::Token));
                let found = || Ty::Ref(Box::new(Ty::List(Box::new(Ty::Int(IntTy::Ulong)))));
                let task = |ok: Ty| Ty::Task(Box::new(io(ok)));
                let send = || [vec![fd(), buffer(Ty::Read), Ty::Bool], address().to_vec()].concat();
                match name {
                    "udpBind" => (SystemCall::UdpBind, address().to_vec(), io(fd())),
                    "udpConnect" => (SystemCall::UdpConnect, [vec![fd()], address().to_vec()].concat(), io(Ty::Void)),
                    "udpSend" => (SystemCall::UdpSend, send(), io(Ty::Void)),
                    "udpSendAsync" => (SystemCall::UdpSendAsync, [send(), vec![token()]].concat(), task(Ty::Void)),
                    "udpReceive" => (SystemCall::UdpReceive, vec![fd(), buffer(Ty::Ref), bits()], io(Ty::Void)),
                    "udpReceiveAsync" => (SystemCall::UdpReceiveAsync, vec![fd(), buffer(Ty::Ref), bits(), token()], task(Ty::Void)),
                    "socketAddress" => (SystemCall::SocketName, vec![fd(), Ty::Bool, bits()], Ty::Bool),
                    "streamAddress" => (SystemCall::StreamName, vec![Ty::Read(Box::new(Ty::Socket)), Ty::Bool, bits()], Ty::Bool),
                    "closeSocket" => (SystemCall::CloseSocket, vec![fd()], Ty::Void),
                    "lookup" => (SystemCall::Lookup, vec![text(), found()], io(Ty::Void)),
                    _ => (SystemCall::LookupAsync, vec![text(), found(), token()], task(Ty::Void)),
                }
            }
            _ => match disk(name) {
                Some(found) => found,
                None => return Err(Diag::error(span, format!("`System` has no function `{name}`"))),
            },
        };
        self.args(name, args, &params, span)?;
        if matches!(call, SystemCall::WaitAsync | SystemCall::UdpSendAsync | SystemCall::UdpReceiveAsync | SystemCall::LookupAsync | SystemCall::SignalNextAsync) {
            if !self.is_async {
                return Err(Diag::error(span, format!("`System.{name}` is awaited in an async function")));
            }
            self.awaited_here(&format!("System.{name}"), awaited, args, span)?;
        }
        *callee = Callee::System(call);
        Ok(ret)
    }

    /// The arguments of a built-in async function: those of the function without `Async`,
    /// then a token that may be left out.
    fn async_args(&mut self, name: &str, args: &mut [Expr], params: &[Ty], span: Span) -> Res<()> {
        if args.len() == params.len() + 1 {
            let (given, token) = args.split_at_mut(params.len());
            self.args(name, given, params, span)?;
            return self.token_arg(&mut token[0]);
        }
        if args.len() != params.len() {
            return Err(Diag::error(span, format!("`{name}` takes {} argument(s), or {} with a token last; {} given", params.len(), params.len() + 1, args.len())));
        }
        self.args(name, args, params, span)
    }

    /// The token a built-in function looks at: `read token`. The function keeps a token of
    /// its own for as long as it needs one, so the borrow ends with the call.
    fn token_arg(&mut self, arg: &mut Expr) -> Res<()> {
        self.typed(arg, &Ty::Read(Box::new(Ty::Token))).map_err(|diag| diag.note("a token is lent: `read token`, or `read source.token()`"))?;
        Ok(())
    }

    /// A built-in async function given a borrow is awaited where it is called: the borrow
    /// could not go to a task of its own.
    fn awaited_here(&self, name: &str, awaited: bool, args: &[Expr], span: Span) -> Res<()> {
        if awaited {
            return Ok(());
        }
        let Some(arg) = args.iter().find(|arg| arg.ty.is_borrow()) else { return Ok(()) };
        let text = &self.src[span.start..span.end];
        let diag = Diag::error(arg.span, format!("`{name}` is given a `{}` here, and a call without `await` starts it as a task of its own, which a borrow cannot go to", arg.ty));
        Err(match self.is_async {
            true => diag.hint(format!("`await {text}` runs it as part of this task, where it can borrow")),
            false => diag.note("async input and output is awaited in an async function; elsewhere the functions without `Async` do the same work"),
        })
    }

    /// `TcpListener.bind(address)`, `TcpStream.connect(address)` and `connectAsync`: the
    /// functions of the network types. An address is a `read string`, `host:port`.
    #[allow(clippy::too_many_arguments)]
    fn net_static(&mut self, owner: &str, name: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span, awaited: bool) -> Res<Ty> {
        let text = Ty::Read(Box::new(Ty::Str));
        let io = |ok: Ty| Ty::Result(Box::new(ok), Box::new(Ty::Enum(IO_ERROR.to_string())));
        match (owner, name) {
            ("TcpListener", "bind") => {
                self.args(name, args, &[text], span).map_err(|diag| diag.note("an address is `host:port`: `\"127.0.0.1:8080\"`, `\"[::1]:8080\"`, `\":8080\"` for every address of the machine"))?;
                *callee = Callee::Net(NetCall::Bind);
                Ok(io(Ty::Listener))
            }
            ("TcpListener", "bindAsync") => Err(Diag::error(name_span, "`TcpListener` has no `bindAsync`: binding does not wait")
                .note("the address is not looked up by name, so `bind` never waits for the network")
                .hint("`TcpListener.bind(address)`, in an async function too")),
            ("TcpStream", "connect") => {
                self.args(name, args, &[text], span).map_err(|diag| diag.note("an address is `host:port`: `\"example.com:80\"`, `\"127.0.0.1:8080\"`, `\"[::1]:8080\"`"))?;
                *callee = Callee::Net(NetCall::Connect);
                Ok(io(Ty::Socket))
            }
            ("TcpStream", "connectAsync") => {
                self.async_args(name, args, &[text], span)?;
                self.awaited_here(&format!("TcpStream.{name}"), awaited, args, span)?;
                *callee = Callee::NetAsync(NetCall::Connect);
                Ok(Ty::Task(Box::new(io(Ty::Socket))))
            }
            _ => {
                let known: &[&str] = match owner {
                    "TcpListener" => &["bind"],
                    _ => &["connect", "connectAsync"],
                };
                let diag = Diag::error(name_span, format!("`{owner}` has no function `{name}`"));
                Err(match (owner, name) {
                    ("TcpListener", "listen" | "create" | "new" | "open") => diag.hint("`TcpListener.bind(address)` listens on `host:port`"),
                    ("TcpStream", "open" | "create" | "new") => diag.hint("`TcpStream.connect(address)` connects to `host:port`"),
                    ("TcpStream", "bind") => diag.note("a `TcpListener` listens: `TcpListener.bind(address)`"),
                    ("TcpListener", "connect" | "connectAsync") => diag.note(format!("a `TcpStream` connects: `TcpStream.{name}(address)`")),
                    _ => match nearest(name, known.iter().copied()) {
                        Some(close) => diag.hint(format!("`{close}`")),
                        None => diag.note(format!("functions: {}", known.join(", "))),
                    },
                })
            }
        }
    }

    /// The methods of a `TcpListener` (`accept`, `acceptAsync`, `port`) and of a `TcpStream`
    /// (`receive`, `send` and their async versions, `reader`). A listener takes connections
    /// through a `read` borrow. A stream receives through a `ref`, since what comes is taken
    /// from it, and sends through a `read` borrow, so that it sends while a reader made with
    /// `reader()` borrows it; a reader holds that borrow, so `receive` cannot take bytes from
    /// under it. Neither is done through a `shared` stream, which several tasks may own.
    /// An async method lends what it is called on, so it is awaited where it is written.
    /// `shared`: the stream is reached through a `shared` value that is not `checked`.
    #[allow(clippy::too_many_arguments)]
    fn net_method(
        &mut self,
        recv: &mut Expr,
        target: &Ty,
        writable: bool,
        shared: bool,
        name: &str,
        name_span: Span,
        args: &mut [Expr],
        callee: &mut Callee,
        span: Span,
        awaited: bool,
    ) -> Res<Ty> {
        let io = |ok: Ty| Ty::Result(Box::new(ok), Box::new(Ty::Enum(IO_ERROR.to_string())));
        let (base, is_async) = match name.strip_suffix("Async") {
            Some(base) => (base, true),
            None => (name, false),
        };
        let bytes = |mode: fn(Box<Ty>) -> Ty| mode(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
        if (target, base) == (&Ty::Socket, "reader") {
            return self.socket_reader(recv, shared, name, is_async, args, callee, span);
        }
        let (call, params, ty) = match (target, base) {
            (Ty::Listener, "accept") => (NetCall::Accept, Vec::new(), io(Ty::Socket)),
            (Ty::Listener, "port") if !is_async => (NetCall::Port, Vec::new(), Ty::Int(IntTy::Int)),
            (Ty::Socket, "receive") => (NetCall::Receive, vec![bytes(Ty::Ref)], io(Ty::Int(IntTy::Nint))),
            (Ty::Socket, "send") => (NetCall::Send, vec![bytes(Ty::Read)], io(Ty::Void)),
            (Ty::Socket, "sendPieces") => (NetCall::SendPieces, vec![Ty::Read(Box::new(Ty::List(Box::new(bytes(Ty::Read)))))], io(Ty::Void)),
            _ => {
                let known: &[&str] = match target {
                    Ty::Listener => &["accept", "acceptAsync", "port"],
                    _ => &["receive", "receiveAsync", "send", "sendAsync", "sendPieces", "sendPiecesAsync", "reader"],
                };
                let diag = Diag::error(name_span, format!("`{target}` has no method `{name}`"));
                let other = match (target, base) {
                    (Ty::Socket, "recv" | "readBytes" | "readSome") => Some(if is_async { "receiveAsync" } else { "receive" }),
                    (Ty::Socket, "write" | "writeBytes" | "writeAll" | "sendAll") => Some(if is_async { "sendAsync" } else { "send" }),
                    (Ty::Socket, "writev" | "sendVectored" | "writeVectored" | "sendAllPieces") => Some(if is_async { "sendPiecesAsync" } else { "sendPieces" }),
                    _ => None,
                };
                return Err(match (target, base) {
                    (_, "close" | "shutdown" | "dispose") => diag.note(format!("a `{target}` has nothing to lose: it is closed when it ends")),
                    (Ty::Listener, "receive" | "send") => diag.note("a `TcpListener` takes connections: `listener.accept()` gives a `TcpStream`, which receives and sends"),
                    (Ty::Socket, "accept") => diag.note("a `TcpListener` takes connections: `TcpListener.bind(address)`, then `listener.accept()`"),
                    (Ty::Socket, "readLine" | "readLineBytes" | "readText" | "lines") => {
                        let conn = &self.src[recv.span.start..recv.span.end];
                        diag.note("a `TcpStream` receives bytes; a `StreamReader` reads lines and text")
                            .hint(format!("`var reader = {conn}.reader()`, then `reader.{name}(...)`"))
                    }
                    (Ty::Socket, "writeLine") => diag.note("a `TcpStream` sends bytes: `send(text.asBytes())`"),
                    (Ty::Listener, "port") => diag.hint("`port()` does not wait: `listener.port()`"),
                    _ => match other.or_else(|| nearest(name, known.iter().copied())) {
                        Some(close) => diag.hint(format!("`{close}`")),
                        None => diag.note(format!("methods: {}", known.join(", "))),
                    },
                });
            }
        };
        let count = params.len();
        if is_async {
            if args.len() != count && args.len() != count + 1 {
                return Err(Diag::error(span, format!("`{name}` takes {count} argument(s), or {} with a token last; {} given", count + 1, args.len())));
            }
        } else if args.len() != count {
            return Err(Diag::error(span, format!("`{name}` takes {count} argument(s), {} given", args.len())));
        }
        let (given, token) = args.split_at_mut(count);
        if call == NetCall::Receive {
            let lent_read = given.first().is_some_and(|arg| self.src[arg.span.start..arg.span.end].starts_with("read "));
            self.args(name, given, &params, span).map_err(|diag| match lent_read {
                true => diag.hint(format!("`{name}` writes into the buffer, so it is lent with `ref`")),
                false => diag,
            })?;
        } else if call == NetCall::SendPieces {
            let conn = &self.src[recv.span.start..recv.span.end];
            let send = if is_async { "await " } else { "" };
            let hint = format!("one run of bytes goes out with `send`: `{send}{conn}.send{}(...)`", if is_async { "Async" } else { "" });
            self.args(name, given, &params, span).map_err(|diag| match given.first().map(|arg| arg.ty.strip_borrow()) {
                Some(Ty::List(item) | Ty::Slice(item)) if **item == Ty::Int(IntTy::Byte) => diag.hint(hint),
                _ => diag,
            })?;
        } else {
            self.args(name, given, &params, span)?;
        }
        if let Some(token) = token.first_mut() {
            self.token_arg(token)?;
        }
        // `send` goes through a `read` borrow, so that a reader may borrow the stream while
        // it sends; not through a `shared` one, which tasks on other threads may own.
        if matches!(call, NetCall::Send | NetCall::SendPieces) {
            if shared {
                return Err(Diag::error(recv.span, format!("`{name}` cannot go through a `shared` connection: several tasks could send on it at once"))
                    .note("a connection is sent on by one task at a time; a connection read by one task and sent on by another is not here yet"));
            }
            self.touch(recv, Access::Read);
        }
        // Receiving takes what has come from the stream: it goes through a `ref`.
        if call == NetCall::Receive {
            if !writable {
                if let Some(diag) = self.readonly(recv) {
                    return Err(diag.note(format!("`{name}` goes on from where the connection is")));
                }
                return Err(self.read_only(recv, Diag::error(recv.span, format!("`{name}` goes on from where the connection is, but it is reached through a `read` borrow"))
                    .hint("take it as `ref TcpStream` instead of `read TcpStream`")));
            }
            self.not_locked(recv, Access::Write)?;
        }
        if !is_async {
            *callee = Callee::Net(call);
            return Ok(ty);
        }
        if !awaited {
            let text = &self.src[span.start..span.end];
            let what = if *target == Ty::Listener { "listener" } else { "connection" };
            let diag = Diag::error(span, format!("`{name}` lends the {what}, and a call without `await` starts it as a task of its own, which a borrow cannot go to"));
            return Err(match self.is_async {
                true => diag.hint(format!("`await {text}` runs it as part of this task, where it can borrow")),
                false => diag.note("the network is awaited in an async function; elsewhere the methods without `Async` do the same work"),
            });
        }
        *callee = Callee::NetAsync(call);
        Ok(Ty::Task(Box::new(ty)))
    }

    /// `stream.remoteAddress()` and `stream.localAddress()`: done by the language's own module,
    /// which is given the stream through a `read` borrow (`SocketAddress.remoteOf(read stream)`).
    /// Neither waits, so neither has an async version.
    fn stream_address(&mut self, recv: &mut Expr, name: &str, args: &mut Vec<Expr>, callee: &mut Callee, span: Span) -> Res<Ty> {
        if !args.is_empty() {
            return Err(Diag::error(span, format!("`{name}` takes no arguments, {} given", args.len())));
        }
        self.touch(recv, Access::Read);
        let owner = format!("{STD_MODULE}.SocketAddress");
        let inner = if name == "remoteAddress" { "remoteOf" } else { "localOf" };
        let Some(sig) = self.methods_of(&owner).get(inner) else { unreachable!("a program that names `{name}` gets the file of `SocketAddress`") };
        let ret = sig.ret.clone();
        let written = Expr { kind: ExprKind::Var(STD_MODULE.to_string()), span: recv.span, ty: Ty::Void, moved: false };
        let stream = std::mem::replace(recv, written);
        let (span, ty) = (stream.span, Ty::Read(Box::new(stream.ty.strip_borrow().clone())));
        args.push(Expr { kind: ExprKind::Borrow { mutable: false, expr: Box::new(stream) }, span, ty, moved: false });
        *callee = Callee::Static { owner, name: inner.to_string() };
        Ok(ret)
    }

    /// `reader.info()`: what the system knows of the file a reader reads (`fstat`), done by the
    /// language's own module, which is given the reader through a `read` borrow
    /// (`FileInfo.ofReader(read reader)`, choice 1873). No path is walked, so nothing waits, and it
    /// has no async version. Only a reader of a file has one: not the input of the program, nor
    /// the reader of a connection.
    fn reader_info(&mut self, recv: &mut Expr, target: &Ty, args: &mut Vec<Expr>, callee: &mut Callee, span: Span) -> Res<Ty> {
        if !args.is_empty() {
            return Err(Diag::error(span, format!("`info` takes no arguments, {} given", args.len())));
        }
        if let ExprKind::Var(var) = &recv.kind {
            if let Some(what) = std_stream(var) {
                return Err(Diag::error(recv.span, format!("`{var}` is {what} of the program, not a file it opened"))
                    .note("a reader of `File.openText(path)` or `File.openRegular(path)` has `info()`"));
            }
        }
        if *target == Ty::Reader(true) {
            return Err(Diag::error(recv.span, "the reader of a connection reads the network, not a file")
                .note("a reader of `File.openText(path)` or `File.openRegular(path)` has `info()`"));
        }
        self.touch(recv, Access::Read);
        let owner = format!("{STD_MODULE}.FileInfo");
        let Some(sig) = self.methods_of(&owner).get("ofReader") else { unreachable!("a program that names `info` and a reader gets the file of `FileInfo`") };
        let ret = sig.ret.clone();
        let written = Expr { kind: ExprKind::Var(STD_MODULE.to_string()), span: recv.span, ty: Ty::Void, moved: false };
        let reader = std::mem::replace(recv, written);
        let (span, ty) = (reader.span, Ty::Read(Box::new(Ty::Reader(false))));
        args.push(Expr { kind: ExprKind::Borrow { mutable: false, expr: Box::new(reader) }, span, ty, moved: false });
        *callee = Callee::Static { owner, name: "ofReader".to_string() };
        Ok(ret)
    }

    /// `c.category()`: the general category of a character, done by the language's own module
    /// (`UnicodeCategory.of(c)`, choice 1476).
    fn char_category(&mut self, recv: &mut Expr, args: &mut Vec<Expr>, callee: &mut Callee, span: Span) -> Res<Ty> {
        if !args.is_empty() {
            return Err(Diag::error(span, format!("`category` takes no arguments, {} given", args.len())));
        }
        let recv_ty = recv.ty.clone();
        self.coerce(recv, recv_ty, &Ty::Char)?;
        let owner = format!("{STD_MODULE}.UnicodeCategory");
        let Some(sig) = self.methods_of(&owner).get("of") else { unreachable!("a program that names `category` gets the file of `UnicodeCategory`") };
        let ret = sig.ret.clone();
        let written = Expr { kind: ExprKind::Var(STD_MODULE.to_string()), span: recv.span, ty: Ty::Void, moved: false };
        let c = std::mem::replace(recv, written);
        args.push(c);
        *callee = Callee::Static { owner, name: "of".to_string() };
        Ok(ret)
    }

    /// `text.normalize(form)` and `text.isNormalized(form)`: done by the language's own module,
    /// which is given the text through a `read` borrow (`NormalizationForm.normalize(read text,
    /// form)`, choice 1611).
    fn text_normalize(&mut self, recv: &mut Expr, name: &str, args: &mut Vec<Expr>, callee: &mut Callee, span: Span) -> Res<Ty> {
        let owner = format!("{STD_MODULE}.NormalizationForm");
        if args.len() != 1 {
            return Err(Diag::error(span, format!("`{name}` takes 1 argument, the form, {} given", args.len()))
                .hint(format!("`text.{name}(NormalizationForm.C)`; the forms are C, D, KC and KD")));
        }
        self.typed(&mut args[0], &Ty::Enum(owner.clone()))?;
        self.touch(recv, Access::Read);
        let Some(sig) = self.methods_of(&owner).get(name) else { unreachable!("a program that names `{name}` gets the file of `NormalizationForm`") };
        let ret = sig.ret.clone();
        let written = Expr { kind: ExprKind::Var(STD_MODULE.to_string()), span: recv.span, ty: Ty::Void, moved: false };
        let text = std::mem::replace(recv, written);
        let (text_span, ty) = (text.span, Ty::Read(Box::new(Ty::Str)));
        args.insert(0, Expr { kind: ExprKind::Borrow { mutable: false, expr: Box::new(text) }, span: text_span, ty, moved: false });
        *callee = Callee::Static { owner, name: name.to_string() };
        Ok(ret)
    }

    /// `conn.reader()`, `conn.reader(limit)`: a `StreamReader` that reads the connection
    /// through a block of its own and borrows it (`Ty::Reader(true)`). Making it does not wait,
    /// so it has no async version and is called in an async function as it is. Not through a
    /// `shared` connection, which tasks on several threads may own.
    #[allow(clippy::too_many_arguments)]
    fn socket_reader(&mut self, recv: &mut Expr, shared: bool, name: &str, is_async: bool, args: &mut [Expr], callee: &mut Callee, span: Span) -> Res<Ty> {
        let conn = &self.src[recv.span.start..recv.span.end];
        if is_async {
            return Err(Diag::error(span, format!("a `TcpStream` has no `{name}`: making a reader does not wait"))
                .hint(format!("`{conn}.reader()`, in an async function too; its methods with `Async` wait")));
        }
        if shared {
            return Err(Diag::error(recv.span, "a reader cannot read a `shared` connection: several tasks could read it at once")
                .note("a connection read by one task and sent on by another is not here yet"));
        }
        if args.len() > 1 {
            return Err(Diag::error(span, format!("`reader` takes 0 argument(s), or 1 (the longest line it reads, in bytes); {} given", args.len())));
        }
        self.args(name, args, &[Ty::Int(IntTy::Nint)][..args.len()], span)?;
        self.touch(recv, Access::Read);
        *callee = Callee::Net(NetCall::Reader);
        Ok(Ty::Reader(true))
    }

    fn method(
        &mut self,
        recv: &mut Expr,
        name: &str,
        name_span: Span,
        args: &mut Vec<Expr>,
        callee: &mut Callee,
        span: Span,
    ) -> Res<Ty> {
        // `await reader.readLineAsync(ref line)`: what `await` is written in front of.
        let awaited = std::mem::take(&mut self.awaiting);
        // `int.parse(text)`: the number the text writes, or where it stops being one.
        // `int.parseHex(text)`: the same in hex digits.
        if let Some(ty) = self.number_type_named(recv) {
            if matches!(name, "fromLittleEndian" | "fromBigEndian") {
                return self.number_from_bytes(&ty, name, name_span, args, callee, span);
            }
            let radix = match (name, &ty) {
                ("parse", _) => 10,
                ("parseHex", Ty::Int(_)) => 16,
                ("parseHex", _) => {
                    return Err(Diag::error(name_span, format!("`parseHex` reads an integer, and `{ty}` is a fractional type"))
                        .hint(format!("`{ty}.parse`")))
                }
                _ => {
                    let diag = Diag::error(name_span, format!("`{ty}` has no function `{name}`"));
                    let functions = if matches!(ty, Ty::Int(_)) { "parse, parseHex, fromLittleEndian, fromBigEndian" } else { "parse" };
                    return Err(match constant(&ty, name, name_span) {
                        Ok(()) => diag.hint(format!("`{name}` is a constant: `{ty}.{name}`, without parentheses")),
                        Err(_) if matches!(ty, Ty::Int(_)) && endian_hint(name, true).is_some() => {
                            diag.hint(format!("`{ty}.{}(bytes)`", endian_hint(name, true).expect("tested above")))
                        }
                        Err(_) => match nearest(name, functions.split(", ")) {
                            Some(close) => diag.hint(format!("`{close}`")),
                            None => diag.note(format!("functions for now: {functions}")),
                        },
                    });
                }
            };
            self.args(name, args, &[Ty::Read(Box::new(Ty::Str))], span)?;
            *callee = Callee::Parse(radix);
            return Ok(Ty::Result(Box::new(ty), Box::new(Ty::Enum(NUMBER_ERROR.to_string()))));
        }
        // `List<byte>.repeat(0, n)`: a function of the type, written where `List<byte>()` would
        // be, without its parentheses (1648).
        if let ExprKind::Collection { ty, args: none, items: None } = &recv.kind {
            if none.is_empty() && recv.span.end == ty.span.end {
                let built = resolve_type(ty, self.names, false)?;
                let Ty::List(item) = &built else {
                    return Err(Diag::error(name_span, format!("`{built}` has no function `{name}`")));
                };
                // `List<T>.withCapacity(n)`: an empty list with room for `n` items (1652).
                if name == "withCapacity" {
                    self.args(name, args, &[Ty::Int(IntTy::Nint)], span)?;
                    recv.ty = built.clone();
                    *callee = Callee::ListWithCapacity;
                    return Ok(built);
                }
                if name != "repeat" {
                    return Err(Diag::error(name_span, format!("`{built}` has no function `{name}`"))
                        .note("functions for now: repeat, withCapacity; an empty list is `List<T>()`"));
                }
                if !item.is_copyable(self.copyable) {
                    return Err(Diag::error(name_span, format!("`{item}` is not `Copyable`, so one item cannot be put in every place"))
                        .hint(format!("add the items one by one to a `{built}()`")));
                }
                self.args(name, args, &[(**item).clone(), Ty::Int(IntTy::Nint)], span)?;
                recv.ty = built.clone();
                *callee = Callee::ListRepeat;
                return Ok(built);
            }
        }
        // A type name in front of the dot: `Stack.create()`.
        if let ExprKind::Var(owner) = &recv.kind {
            if self.find(owner).is_none() && owner != "this" {
                // `IoError.fromCode(n)`: the error for a code of the system (`C.lastError()`);
                // `IoError.fromErrno(n)`: for a number of C's library (`C.errno()`). The two
                // differ on Windows, so the wrong one around the other's call is an error.
                if owner == IO_ERROR && matches!(name, "fromCode" | "fromErrno") {
                    self.args(name, args, &[Ty::Int(IntTy::Int)], span)?;
                    let (wrong, right, what) = match name {
                        "fromCode" => (MemoryCall::Errno, "fromErrno", "`C.errno()` is a number of C's library, and `fromCode` takes a code of the system"),
                        _ => (MemoryCall::LastError, "fromCode", "`C.lastError()` is a code of the system, and `fromErrno` takes a number of C's library"),
                    };
                    if matches!(&args[0].kind, ExprKind::Method { callee, .. } if *callee == Callee::Memory(wrong)) {
                        let text = &self.src[args[0].span.start..args[0].span.end];
                        return Err(Diag::error(args[0].span, what)
                            .note("the two are the same on Linux and macOS and differ on Windows")
                            .hint(format!("`IoError.{right}({text})`")));
                    }
                    *callee = Callee::Memory(if name == "fromCode" { MemoryCall::IoErrorOf } else { MemoryCall::IoErrorOfErrno });
                    return Ok(Ty::Enum(IO_ERROR.to_string()));
                }
                if self.structs.contains_key(owner.as_str()) || self.enums.contains_key(owner.as_str()) {
                    return self.static_call(owner, name, name_span, args, callee, span, awaited);
                }
                if owner == "File" {
                    return self.io_call(owner, name, name_span, args, callee, span, awaited);
                }
                if owner == "Memory" {
                    return self.memory_call(name, name_span, args, callee, span);
                }
                if owner == "Atomic" {
                    return self.atomic_static(name, name_span, args, callee, span);
                }
                if owner == "C" {
                    return self.c_call(name, name_span, args, callee, span);
                }
                // `OperatingSystem.isWindows()`: which system the program is built for, a constant
                // the C compiler is given; both branches of an `if` on it are checked everywhere.
                if owner == "OperatingSystem" {
                    let os = match name {
                        "isWindows" => Os::Windows,
                        "isLinux" => Os::Linux,
                        "isMacOS" => Os::MacOS,
                        _ => {
                            let diag = Diag::error(name_span, format!("`OperatingSystem` has no function `{name}`"));
                            return Err(match nearest(name, ["isWindows", "isLinux", "isMacOS"].into_iter()) {
                                Some(close) => diag.hint(format!("`OperatingSystem.{close}()`")),
                                None => diag.note("its functions: `isWindows()`, `isLinux()`, `isMacOS()`"),
                            });
                        }
                    };
                    self.args(name, args, &[], span)?;
                    *callee = Callee::Os(os);
                    return Ok(Ty::Bool);
                }
                // The two readings of the clock, which only the language's own module makes.
                if owner == "Clock" && self.names.module(span) == STD_MODULE && matches!(name, "monotonic" | "wall" | "wallSeconds") {
                    self.args(name, args, &[], span)?;
                    *callee = Callee::Clock(match name {
                        "wall" => ClockCall::Wall,
                        "wallSeconds" => ClockCall::WallSeconds,
                        _ => ClockCall::Monotonic,
                    });
                    return Ok(Ty::Int(IntTy::Long));
                }
                // The offset of the time zone of the system at a moment, in seconds.
                if owner == "Clock" && self.names.module(span) == STD_MODULE && name == "offsetAt" {
                    self.args(name, args, &[Ty::Int(IntTy::Long)], span)?;
                    *callee = Callee::Clock(ClockCall::OffsetAt);
                    return Ok(Ty::Int(IntTy::Int));
                }
                // The calls of the file system and of the environment that only the language's own
                // module makes; `Directory`, `FileInfo` and `Environment` are built on them.
                if owner == "System" && self.names.module(span) == STD_MODULE {
                    return self.system_call(name, args, callee, span, awaited);
                }
                // The random bytes of the system, which only the language's own module reads.
                if owner == "Entropy" && self.names.module(span) == STD_MODULE && name == "fill" {
                    self.args(name, args, &[Ty::Ref(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))))], span)?;
                    *callee = Callee::Entropy;
                    return Ok(Ty::Void);
                }
                if owner == "Task" {
                    return self.task_run(name, name_span, args, callee, span, awaited);
                }
                if owner == "CancellationSource" || owner == "CancellationToken" {
                    return self.cancel_static(owner, name, name_span, args, callee, span);
                }
                if owner == "TcpListener" || owner == "TcpStream" {
                    return self.net_static(owner, name, name_span, args, callee, span, awaited);
                }
                // `CString.from(text)`: the text with a zero byte after it; one inside is an error.
                if owner == "CString" {
                    if name != "from" {
                        let diag = Diag::error(name_span, format!("`CString` has no function `{name}`"));
                        return Err(diag.note("`CString.from(text)` makes one"));
                    }
                    self.args(name, args, &[Ty::Read(Box::new(Ty::Str))], span)?;
                    *callee = Callee::ToCString;
                    return Ok(Ty::Result(Box::new(Ty::CString), Box::new(Ty::Struct(TEXT_ERROR.to_string()))));
                }
                // `string.join(", ", read names)`
                // `string.fromUtf8(bytes)`: the bytes looked at as text, if they are text.
                if owner == "string" && name == "fromUtf8" {
                    let bytes = Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
                    self.args(name, args, &[bytes], span)?;
                    *callee = Callee::FromUtf8;
                    return Ok(Ty::Result(Box::new(Ty::Read(Box::new(Ty::Str))), Box::new(Ty::Struct(TEXT_ERROR.to_string()))));
                }
                // `string.fromCString(p)`: a copy of the text C ends with a zero byte, if it is text.
                if owner == "string" && name == "fromCString" {
                    if args.len() != 1 {
                        return Err(Diag::error(span, format!("`fromCString` takes 1 argument(s), {} given", args.len())));
                    }
                    let found = self.expr(&mut args[0], None)?;
                    // A `CString` is a text with its zero byte: reading it needs no `unsafe`. One
                    // C lends an `export` function is one C promised.
                    if *found.strip_borrow() == Ty::CString {
                        if !matches!(found, Ty::Read(_) | Ty::Ref(_)) {
                            return Err(Diag::error(args[0].span, "`string.fromCString` reads a `CString` it is lent").hint("lend it: `read text`"));
                        }
                        *callee = Callee::FromCString;
                        return Ok(Ty::Result(Box::new(Ty::Str), Box::new(Ty::Struct(TEXT_ERROR.to_string()))));
                    }
                    let bytes = Ty::Raw(Box::new(Ty::Int(IntTy::Byte)));
                    if *found.strip_borrow() != bytes && *found.strip_borrow() != Ty::Raw(Box::new(Ty::Int(IntTy::Sbyte))) {
                        return Err(Diag::error(args[0].span, format!("`string.fromCString` reads the bytes a `raw byte` points at, and this is `{found}`")));
                    }
                    let to = found.strip_borrow().clone();
                    self.coerce(&mut args[0], found, &to)?;
                    self.needs_unsafe(span, "`string.fromCString` reads memory up to a zero byte, which the checker cannot see is there")?;
                    *callee = Callee::FromCString;
                    return Ok(Ty::Result(Box::new(Ty::Str), Box::new(Ty::Struct(TEXT_ERROR.to_string()))));
                }
                // `string.withCapacity(n)`: an empty text with room for `n` bytes.
                if owner == "string" && name == "withCapacity" {
                    self.args(name, args, &[Ty::Int(IntTy::Nint)], span)?;
                    *callee = Callee::WithCapacity;
                    return Ok(Ty::Str);
                }
                if owner == "string" {
                    if name != "join" {
                        return Err(Diag::error(name_span, format!("`string` has no function `{name}`")).note("functions for now: join, fromUtf8, fromCString, withCapacity"));
                    }
                    let text = Ty::Read(Box::new(Ty::Str));
                    self.args(name, args, &[text, Ty::Read(Box::new(Ty::List(Box::new(Ty::Str))))], span)?;
                    *callee = Callee::Join;
                    return Ok(Ty::Str);
                }
                let diag = Diag::error(recv.span, format!("there is no variable or type named `{owner}`"));
                if let Some(hint) = self.names.use_hint(owner, recv.span) {
                    return Err(diag.hint(hint));
                }
                let vars = self.scopes.iter().flat_map(|scope| scope.vars.iter().map(|var| var.name.as_str()));
                return Err(match nearest(owner, vars.chain(self.structs.keys().map(String::as_str))) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag,
                });
            }
        }

        // `xs[a..b].sort()`: a range in front of a method that changes the items is lent as
        // the method needs, as a variable in front of it is; the range is then `ref xs[a..b]`.
        if matches!(name, "sort" | "reverse" | "copyFrom") {
            if let ExprKind::Slice { recv: inner, from, to } = &mut recv.kind {
                let (window, writable) = self.range(inner, from, to)?;
                let Ty::Read(behind) = window else { unreachable!("a range is a `read` window") };
                if *behind != Ty::Str {
                    if let Some(diag) = self.readonly(inner) {
                        return Err(diag.note(format!("`{name}` changes the items of a range of it")));
                    }
                    if !writable {
                        return Err(self.read_only(inner, Diag::error(inner.span, format!("`{name}` changes the items, but this is reached through a `read` borrow"))
                            .hint("take the value as `ref` instead of `read`")));
                    }
                    self.not_locked(inner, Access::Write)?;
                }
                recv.ty = if *behind == Ty::Str { Ty::Read(behind) } else { Ty::Ref(behind) };
            }
        }
        // `Task.whenAll(tasks).wait()`: waits for the task of the call where it is written.
        let when = name == "wait" && is_when(recv);
        let (recv_ty, mut writable, is_place) = match &recv.kind {
            ExprKind::Slice { .. } if matches!(name, "sort" | "reverse" | "copyFrom") => (recv.ty.clone(), true, false),
            _ if when => {
                self.awaiting = true;
                let ty = self.expr(recv, None);
                self.awaiting = false;
                (ty?, true, false)
            }
            _ => match self.place(recv)? {
                Some((ty, writable)) => (ty, writable, true),
                None => (self.expr(recv, None)?, true, false),
            },
        };
        // A method of a struct or an array is lent its receiver, and a place in raw memory is
        // not lent.
        if is_place && Expr::in_raw_memory(recv) && matches!(recv_ty, Ty::Struct(_) | Ty::Array(..)) {
            let text = &self.src[recv.span.start..recv.span.end];
            return Err(Diag::error(recv.span, "a place in raw memory is not lent yet, and a method is lent the value it is called on")
                .hint(format!("copy it out first: `var item = unsafe {text}`, then `item.{name}(...)`")));
        }
        if is_place && matches!(recv_ty.strip_borrow(), Ty::Struct(_) | Ty::Array(..)) {
            if let Some(diag) = not_lent(recv, self.structs, self.src) {
                return Err(diag.note("a method is lent the value it is called on"));
            }
        }
        // `buffer.pointer()`: the address of the items, for C.
        if name == "pointer" {
            let item = match recv_ty.strip_borrow() {
                Ty::List(item) | Ty::Array(item, _) | Ty::Slice(item) => Some((**item).clone()),
                _ => None,
            };
            if let Some(item) = item {
                let ty = self.pointer_of(recv, &recv_ty, item, is_place, args, span)?;
                *callee = Callee::Pointer;
                return Ok(ty);
            }
        }
        // `t.wait()`: waits for the task and takes it.
        if let Ty::Task(result) = recv_ty.strip_borrow() {
            let result = (**result).clone();
            if name != "wait" {
                return Err(Diag::error(name_span, format!("a `{}` has the method `wait`, not `{name}`", recv_ty.strip_borrow()))
                    .note("`wait` waits for the task and gives what its function returned"));
            }
            if !args.is_empty() {
                return Err(Diag::error(span, format!("`wait` takes 0 argument(s), {} given", args.len())));
            }
            if self.is_async {
                let task = &self.src[recv.span.start..recv.span.end];
                return Err(Diag::error(name_span, "an async function does not `wait`: that would hold the thread, which other tasks need, until the task ends")
                    .hint(format!("`await {task}` waits without holding the thread")));
            }
            if when {
                *callee = Callee::Wait;
                return Ok(result);
            }
            if recv_ty.is_borrow() {
                return Err(Diag::error(recv.span, "`wait` takes the task and uses it up, and this one is only borrowed").note("whoever started the task waits for it"));
            }
            // `Task.delay(100).wait()`: the task a call gives has no other owner, so it is
            // waited for where it is made.
            if !is_place {
                if !matches!(recv.kind, ExprKind::Call { .. } | ExprKind::Method { .. }) {
                    let task = &self.src[recv.span.start..recv.span.end];
                    return Err(Diag::error(recv.span, "`wait` takes the task it is called on, and this one is a part of a value that has no name")
                        .hint(format!("give the task a name first: `var task = {task}`, then `task.wait()`")));
                }
                *callee = Callee::Wait;
                return Ok(result);
            }
            self.move_out(recv, &recv_ty)?;
            recv.moved = true;
            *callee = Callee::Wait;
            return Ok(result);
        }
        if *recv_ty.strip_borrow() == Ty::Group {
            return self.group_method(recv, &recv_ty, is_place, name, name_span, args, callee, span, awaited || when);
        }
        // `.clone()` of a `shared` value is one more owner, as `share` is: what it points to
        // is not copied.
        if name == "clone" {
            if let Ty::Shared(..) = recv_ty.strip_borrow() {
                if !args.is_empty() {
                    return Err(Diag::error(span, format!("`clone` takes 0 argument(s), {} given", args.len())));
                }
                *callee = Callee::Plain;
                return Ok(recv_ty.strip_borrow().clone());
            }
        }
        // `x.makeUnique()` of a `shared T` (choice 1813): `ref T`, the value itself when `x` is its
        // only owner and nothing looks at it, otherwise a copy that `x` owns alone from then on.
        if name == "makeUnique" {
            if let Ty::Shared(inner, checked) = recv_ty.strip_borrow() {
                let text = &self.src[recv.span.start..recv.span.end];
                if !args.is_empty() {
                    return Err(Diag::error(span, format!("`makeUnique` takes 0 argument(s), {} given", args.len())));
                }
                if *checked || matches!(**inner, Ty::Locked(_)) {
                    return Err(Diag::error(name_span, format!("`makeUnique` is for a `shared` value, which is only read; a `{}` is changed where it is", recv_ty.strip_borrow()))
                        .hint(format!("`ref {text} as v {{ ... }}`")));
                }
                if !is_place {
                    return Err(Diag::error(recv.span, "`makeUnique` needs a variable or a field: it may put a copy there")
                        .hint(format!("give the value a name first: `var value = {text}`")));
                }
                if !can_clone(inner, &self.names.clonable, self.copyable) {
                    let diag = Diag::error(name_span, format!("`makeUnique` copies the value when it has other owners, and a `{inner}` cannot be cloned"));
                    return Err(match lacking(inner, &self.names.clonable) {
                        Some(lacks) => diag.hint(format!("make `{lacks}` `Clonable`")),
                        None => diag,
                    });
                }
                if let Some(diag) = self.readonly(recv) {
                    return Err(diag.note("`makeUnique` may put a copy in its place"));
                }
                if !writable {
                    return Err(Diag::error(recv.span, "this is reached through a `read` borrow, and `makeUnique` may put a copy in its place")
                        .hint("lend it as `ref` instead of `read`"));
                }
                self.not_locked(recv, Access::Write)?;
                *callee = Callee::Plain;
                return Ok(Ty::Ref(inner.clone()));
            }
        }
        // `.clone()` of a `Clonable` type without a `clone` of its own, and of a collection of what
        // can be cloned, is made by the language: a copy, field by field and item by item (1818).
        if name == "clone" {
            let mut target = recv_ty.strip_borrow();
            while let Ty::Ptr(inner) | Ty::Read(inner) | Ty::Ref(inner) = target {
                target = inner;
            }
            let made = match target {
                Ty::Struct(owner) | Ty::Enum(owner) => {
                    let own = self.structs.get(owner).is_some_and(|info| info.methods.contains_key("clone")) || self.enums.get(owner).is_some_and(|info| info.methods.contains_key("clone"));
                    !own && (self.names.clonable.contains(owner) || target.is_copyable(self.copyable))
                }
                Ty::List(_) | Ty::Set(_) | Ty::Map(..) => true,
                _ => false,
            };
            if made {
                if !args.is_empty() {
                    return Err(Diag::error(span, format!("`clone` takes 0 argument(s), {} given", args.len())));
                }
                if !can_clone(target, &self.names.clonable, self.copyable) {
                    let diag = Diag::error(name_span, format!("a `{target}` cannot be cloned: what it holds cannot be copied"));
                    return Err(match lacking(target, &self.names.clonable) {
                        Some(inner) => diag.hint(format!("make `{inner}` `Clonable`: `{} : Clonable`", inner.rsplit('.').next().unwrap_or(inner))),
                        None => diag.note("a task, a stream, a process and the like have one owner; they are not copied"),
                    });
                }
                *callee = Callee::Clone(target.clone());
                return Ok(target.clone());
            }
        }
        // Like field access, a method call looks through pointers and borrows.
        let mut borrowed = false;
        let mut shared = false;
        let mut target = &recv_ty;
        loop {
            target = match target {
                Ty::Read(inner) => {
                    (writable, borrowed) = (false, true);
                    inner
                }
                // A `ref` reached through a `read` borrow only reads.
                Ty::Ref(inner) => {
                    borrowed = true;
                    inner
                }
                Ty::Ptr(inner) => inner,
                Ty::Locked(_) => return Err(self.locked_outside(recv.span)),
                // A method only reads what a `shared` value points to, and cannot take it;
                // through `shared checked` it may change it.
                Ty::Shared(inner, checked) => {
                    (writable, borrowed) = (*checked, true);
                    shared |= !*checked;
                    inner
                }
                _ => break,
            };
        }
        // `count.fetchAdd(1)`: an atomic value is changed through any way it is reached, `read`
        // and `shared` too: each operation is one step, which no other task sees half done.
        if let Ty::Atomic(item) = target {
            let item = (**item).clone();
            return self.atomic_method(&item, name, name_span, args, callee, span);
        }

        let structs = self.structs;
        if let Ty::Struct(owner) | Ty::Enum(owner) = target {
            // A datagram socket sends each message as it is given, as a `TcpStream` does.
            if owner == &format!("{STD_MODULE}.UdpSocket") && matches!(name, "close" | "shutdown") {
                return Err(Diag::error(name_span, format!("`UdpSocket` has no method `{name}`")).note("a `UdpSocket` has nothing to lose: it is closed when it ends"));
            }
            if name == "dispose" && structs.get(owner).is_some_and(|info| info.disposable) {
                let diag = Diag::error(name_span, "`dispose` is not called by the program: it runs by itself, once, when the last owner of the value ends");
                let text = &self.src[recv.span.start..recv.span.end];
                return Err(match structs[owner].closable {
                    true => diag.hint(format!("to end it here and see how closing went, `try {text}.close()`")),
                    false => diag.hint("to end it sooner, hand it to a function or a method that takes it by value; it ends where that one does"),
                });
            }
            if let Some(sig) = self.methods_of(owner).get(name) {
                if !sig.public && !self.inside(owner) {
                    return Err(self.not_public(name, owner, name_span));
                }
                let Some(this) = sig.this else {
                    return Err(Diag::error(name_span, format!("`{name}` does not take `this`, so it is called on the type"))
                        .hint(format!("`{owner}.{name}(...)`")));
                };
                // `read FolderRef this`: called on the value that holds the struct.
                if let Some((holder, written)) = &sig.holder {
                    if recv_ty.strip_borrow() != holder {
                        let short = short_name(owner);
                        return Err(Diag::error(recv.span, format!("`{name}` is called on a `{written}`, and this is `{recv_ty}`"))
                            .note(format!("it takes the value that holds the `{short}`, so that it can write `share this` and `weak this`"))
                            .hint(format!("make the `{short}` with `new {written}(...)`")));
                    }
                    match this {
                        This::Value if !is_place || recv_ty.is_borrow() => {
                            if recv_ty.is_borrow() {
                                let text = &self.src[recv.span.start..recv.span.end];
                                return Err(Diag::error(recv.span, format!("`{name}` takes `this` by value, an owner of its own, but this value is only borrowed"))
                                    .hint(format!("give it one more owner: `var owner = share {text}`, then `owner.{name}(...)`")));
                            }
                        }
                        This::Value => {
                            self.move_out(recv, &recv_ty)?;
                            recv.moved = true;
                        }
                        _ => {}
                    }
                    self.fill_defaults(name, sig, args, span)?;
                    self.args(name, args, &sig.params, span)?;
                    if is_place && this == This::Read {
                        self.lent_twice(Some((recv, false)), args, &sig.params)?;
                    }
                    *callee = Callee::Method { owner: owner.clone(), this, whole: true };
                    let this_ty = match this {
                        This::Value => holder.clone(),
                        This::Read => Ty::Read(Box::new(holder.clone())),
                        This::Ref => Ty::Ref(Box::new(holder.clone())),
                    };
                    return self.method_gives(name, sig, Some((this_ty, recv.span)), args, span, awaited);
                }
                match this {
                    This::Read => {}
                    This::Ref if !writable => {
                        if let Some(diag) = self.readonly(recv) {
                            return Err(diag.note(format!("`{name}` takes `ref this` and may change it")));
                        }
                        return Err(self.read_only(recv, Diag::error(recv.span, format!("`{name}` takes `ref this`, but this value is reached through a `read` borrow"))
                            .hint("take the value as `ref` instead of `read`")));
                    }
                    This::Ref => self.not_locked(recv, Access::Write)?,
                    // A `Copyable` receiver is copied in, like an argument.
                    This::Value if target.is_copyable(self.copyable) => {}
                    This::Value if borrowed => {
                        if through_shared(recv) {
                            return Err(Diag::error(recv.span, format!("`{name}` takes `this` by value and uses it up, but this value is reached through a `shared` value, which its other owners still look at"))
                                .note(SHARED_READ_ONLY));
                        }
                        if through_holder(recv, true) {
                            return Err(Diag::error(recv.span, format!("`{name}` takes `this` by value and uses it up, but this value is reached through a `shared checked` value, which its other owners still look at")));
                        }
                        return Err(Diag::error(recv.span, format!("`{name}` takes `this` by value and uses it up, but this value is only borrowed")));
                    }
                    This::Value if is_place => {
                        self.move_out(recv, &recv_ty)?;
                        recv.moved = true;
                    }
                    This::Value => {}
                }
                let bits = owner == &format!("{STD_MODULE}.Random") && name == "nextLong" && args.is_empty();
                self.fill_defaults(name, sig, args, span)?;
                self.args(name, args, &sig.params, span).map_err(|diag| match bits {
                    true => diag.hint("`nextUlong()` gives 64 random bits; `nextLong(low, high)` a number in a range"),
                    false => diag,
                })?;
                if is_place && this != This::Value {
                    self.lent_twice(Some((recv, this == This::Ref)), args, &sig.params)?;
                }
                *callee = Callee::Method { owner: owner.clone(), this, whole: false };
                let this_ty = match this {
                    This::Value => target.clone(),
                    This::Read => Ty::Read(Box::new(target.clone())),
                    This::Ref => Ty::Ref(Box::new(target.clone())),
                };
                return self.method_gives(name, sig, Some((this_ty, recv.span)), args, span, awaited);
            }
        }
        // A function value kept in a field: `this.test(order)`.
        if let Ty::Struct(owner) = target {
            let fields = &structs[owner].fields;
            if let Some(position) = fields.iter().position(|(field, _)| field == name) {
                if let Ty::Func(params, ret) | Ty::CFunc(params, ret) = &fields[position].1 {
                    if !structs[owner].open[position] && !self.inside(owner) {
                        return Err(self.not_public(name, owner, name_span));
                    }
                    *callee = match &fields[position].1 {
                        Ty::CFunc(..) => {
                            self.c_value_call(name, args, params, span)?;
                            Callee::CFieldValue(position, params.clone())
                        }
                        _ => {
                            self.args(name, args, params, span)?;
                            Callee::FieldValue(position)
                        }
                    };
                    return Ok((**ret).clone());
                }
            }
        }
        match name {
            "take" => return self.take(recv, &recv_ty, writable, is_place, args, callee, span),
            "replace" => return self.replace(recv, &recv_ty, writable, is_place, args, callee, span),
            _ => {}
        }
        // A list of tasks only takes tasks and says how many it has: its tasks are taken out
        // by `Task.whenAll` and `Task.whenAny`, which wait for them.
        if task_list(target) {
            let list = &self.src[recv.span.start..recv.span.end];
            if !matches!(name, "add" | "length") {
                let diag = Diag::error(name_span, format!("a `{target}` has the methods `add` and `length`, not `{name}`"))
                    .note("its tasks are taken out by `Task.whenAll` and `Task.whenAny`, which wait for them");
                return Err(match name {
                    "removeLast" | "remove" | "get" | "clear" => diag.hint(match self.is_async {
                        true => format!("`await Task.whenAny(ref {list})` takes out the first task to end, with what it gave"),
                        false => format!("`Task.whenAny(ref {list}).wait()` takes out the first task to end, with what it gave"),
                    }),
                    _ => diag,
                });
            }
            if !is_place {
                return Err(Diag::error(recv.span, "this list of tasks has no name, so its tasks could not be waited for after this call")
                    .hint("keep it in a variable: `var tasks = ...`"));
            }
        }
        if matches!(target, Ty::List(_) | Ty::Map(..) | Ty::Set(_)) {
            return self.collection_method(recv, target, target, writable, name, name_span, args, span);
        }
        if matches!(target, Ty::Reader(_)) && name == "info" {
            let target = target.clone();
            return self.reader_info(recv, &target, args, callee, span);
        }
        if matches!(target, Ty::Reader(_) | Ty::Writer) {
            let target = target.clone();
            if let Some(base) = name.strip_suffix("Async") {
                return self.stream_async(recv, &target, writable, borrowed, base, name, name_span, args, callee, span, awaited);
            }
            return self.stream_method(recv, &target, writable, borrowed, name, name, name_span, args, callee, span);
        }
        if matches!(target, Ty::Source | Ty::Token) {
            let target = target.clone();
            return self.cancel_method(recv, &target, name, name_span, args, callee, span);
        }
        if matches!(target, Ty::Channel(_) | Ty::Sender(_)) {
            let target = target.clone();
            return self.chan_method(recv, &target, name, name_span, args, callee, span, awaited);
        }
        if *target == Ty::Socket && matches!(name, "remoteAddress" | "localAddress") {
            return self.stream_address(recv, name, args, callee, span);
        }
        if *target == Ty::Char && name == "category" {
            return self.char_category(recv, args, callee, span);
        }
        if *target == Ty::Str && matches!(name, "normalize" | "isNormalized") {
            return self.text_normalize(recv, name, args, callee, span);
        }
        if matches!(target, Ty::Listener | Ty::Socket) {
            let target = target.clone();
            return self.net_method(recv, &target, writable, shared, name, name_span, args, callee, span, awaited);
        }
        // An array and a slice have the methods of a list that only look, the two that
        // change the order of the items, and the one that copies over them, none changing how
        // many there are.
        if let Ty::Array(item, _) | Ty::Slice(item) = target {
            if !matches!(name, "length" | "get" | "getUnchecked" | "indexOf" | "contains" | "binarySearch" | "lowerBound" | "upperBound" | "sort" | "reverse" | "copyFrom") {
                let diag = Diag::error(name_span, format!("`{target}` has no method `{name}`"));
                return Err(match name {
                    "add" | "addAll" | "removeLast" | "clear" => diag.note(format!("`{target}` has a fixed number of items; a `List<{item}>` grows and shrinks")),
                    "copyFromSlice" | "copy_from_slice" => diag.hint("`copyFrom`"),
                    _ => match search_name(name).or_else(|| nearest(name, ["length", "get", "indexOf", "contains", "binarySearch", "lowerBound", "upperBound", "sort", "reverse", "copyFrom"].into_iter())) {
                        Some(close) => diag.hint(format!("`{close}`")),
                        None => diag.note("methods for now: length, get, indexOf, contains, binarySearch, lowerBound, upperBound, sort, reverse, copyFrom"),
                    },
                });
            }
            let shown = target.clone();
            return self.collection_method(recv, &Ty::List(item.clone()), &shown, writable, name, name_span, args, span);
        }

        let arity = |count: usize| {
            if args.len() == count {
                Ok(())
            } else {
                Err(Diag::error(span, format!("`{name}` takes {count} argument(s), {} given", args.len())))
            }
        };
        // A number behind a pointer is read out first: `count.wrappingAdd(1)` with `int* count`.
        let target = target.clone();
        let mut recv_ty = recv_ty;
        if target.is_number() || target == Ty::Char {
            while let Ty::Ptr(inner) | Ty::Read(inner) | Ty::Ref(inner) = recv_ty {
                wrap(recv, ExprKind::Load);
                recv.ty = (*inner).clone();
                recv_ty = *inner;
            }
        }
        let target = &target;
        match (target, name) {
            // C's `long` is compared, printed and converted, and nothing more.
            (Ty::Int(ty), _) if ty.is_c() && !(matches!(name, "toString" | "compare" | "equals") || is_conversion(name)) => {
                let text = &self.src[recv.span.start..recv.span.end];
                Err(Diag::error(name_span, format!("`{}` has no method `{name}`: C's `{}` has a size of its own on each system, so it is only compared, printed and converted", ty.name(), c_spelling(*ty)))
                    .hint(format!("work on a `{0}`: `(({0}){text}).{name}({1})`", c_partner(*ty).name(), if args.is_empty() { "" } else { "..." })))
            }
            (Ty::Int(ty), "wrappingAdd" | "wrappingSubtract" | "wrappingMultiply" | "saturatingAdd" | "saturatingSubtract" | "saturatingMultiply") => {
                arity(1)?;
                let other = self.int_operand(&mut args[0], Some(*ty), "the argument")?;
                let common = Ty::Int(self.common_int(recv, recv_ty.clone(), &mut args[0], Ty::Int(other))?);
                Ok(common)
            }
            (Ty::Int(ty), "tryAdd" | "trySubtract" | "tryMultiply" | "tryDivide") => {
                arity(1)?;
                let other = self.int_operand(&mut args[0], Some(*ty), "the argument")?;
                let common = Ty::Int(self.common_int(recv, recv_ty.clone(), &mut args[0], Ty::Int(other))?);
                Ok(Ty::Nullable(Box::new(common)))
            }
            // The bits of an integer, as C23's `<stdbit.h>` has them: counted, turned around and
            // rotated on every integer type, a count is an `int`.
            (Ty::Int(_), "countOnes" | "countZeros" | "leadingZeros" | "trailingZeros" | "leadingOnes" | "trailingOnes") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Int(IntTy::Int))
            }
            (Ty::Int(ty), "reverseBytes" | "reverseBits") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Int(*ty))
            }
            (Ty::Int(ty), "rotateLeft" | "rotateRight") => {
                arity(1)?;
                self.int_operand(&mut args[0], Some(IntTy::Int), "the count of a rotation")?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Int(*ty))
            }
            // A power of two is a question of a number that is never negative.
            (Ty::Int(ty), "isPowerOfTwo" | "nextPowerOfTwo" | "log2") => {
                arity(0)?;
                if ty.signed() {
                    let text = &self.src[recv.span.start..recv.span.end];
                    let unsigned = ty.unsigned().name();
                    return Err(Diag::error(name_span, format!("`{name}` is a method of the unsigned types, and `{}` is signed", ty.name()))
                        .hint(format!("`(({unsigned}){text}).{name}()`")));
                }
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(match name {
                    "isPowerOfTwo" => Ty::Bool,
                    "log2" => Ty::Int(IntTy::Int),
                    _ => Ty::Int(*ty),
                })
            }
            (Ty::Str, "clone") => {
                arity(0)?;
                Ok(Ty::Str)
            }
            // The ASCII tests and changes of a character, on a `char` and on a `byte`: what
            // a parser that goes byte by byte needs.
            (Ty::Char | Ty::Int(IntTy::Byte), _) if ASCII_TESTS.contains(&name) => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Bool)
            }
            (Ty::Char | Ty::Int(IntTy::Byte), "toAsciiUpper" | "toAsciiLower") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(target.clone())
            }
            // What the Unicode tables say of a character: a letter (category L), a number (N),
            // an upper or a lower case letter (Lu, Ll), a control character (Cc).
            (Ty::Char, _) if UNICODE_TESTS.contains(&name) => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Bool)
            }
            // The simple case mapping: one character for one (`'ß'.toUpper()` is `'ß'`).
            (Ty::Char, "toUpper" | "toLower") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Char)
            }
            // White space as Unicode has it: what `trim` takes off.
            (Ty::Char, "isWhitespace") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Bool)
            }
            // How many bytes the character takes in a text.
            (Ty::Char, "utf8Length") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Int(IntTy::Nint))
            }
            (Ty::Char, "compare" | "equals") => {
                arity(1)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                self.typed(&mut args[0], &Ty::Char)?;
                Ok(if name == "compare" { Ty::Int(IntTy::Int) } else { Ty::Bool })
            }
            // `n.toLittleEndian()`: the bytes of the number, the lowest first (or the highest
            // first with `toBigEndian`), in an array where the value is.
            (Ty::Int(ty), "toLittleEndian" | "toBigEndian") => {
                arity(0)?;
                let text = &self.src[recv.span.start..recv.span.end];
                match ty {
                    IntTy::Byte | IntTy::Sbyte => {
                        let item = if *ty == IntTy::Byte { text.to_string() } else { format!("(byte){text}") };
                        return Err(Diag::error(name_span, format!("a `{target}` is one byte, and one byte has no order to be written in"))
                            .hint(format!("an array of the one byte: `byte[1] {{ {item} }}`")));
                    }
                    IntTy::Nint | IntTy::Nuint => {
                        let fixed = if ty.signed() { "long" } else { "ulong" };
                        return Err(Diag::error(name_span, format!("`{target}` has the size of an address, which depends on the machine; a number kept in bytes has a size of its own"))
                            .hint(format!("make it a `{fixed}` first: `(({fixed}){text}).{name}()` gives 8 bytes")));
                    }
                    _ => {}
                }
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Array(Box::new(Ty::Int(IntTy::Byte)), (ty.bits() / 8) as usize))
            }
            // `code.tryToChar()`: `null` if no character has this code.
            (Ty::Int(_), "tryToChar") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Nullable(Box::new(Ty::Char)))
            }
            (Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char | Ty::Fmt, "toString") => {
                arity(0)?;
                Ok(Ty::Str)
            }
            (Ty::Str, "length") => {
                arity(0)?;
                Ok(Ty::Int(IntTy::Nint))
            }
            // A text grows at its end: `line.write($"{name}: {count}")`, and is emptied with
            // `clear`, which keeps its memory for the next round.
            (Ty::Str, "write" | "clear") => {
                arity(if name == "write" { 1 } else { 0 })?;
                if !writable {
                    if let Some(diag) = self.readonly(recv) {
                        return Err(diag.note(format!("`{name}` changes the text")));
                    }
                    return Err(self.read_only(recv, Diag::error(recv.span, format!("`{name}` changes the text, but it is reached through a `read` borrow"))
                        .hint("take the text as `ref string` instead of `read string`")));
                }
                if name == "write" {
                    let ty = self.operand(&mut args[0], None)?;
                    self.printable(&ty, args[0].span)?;
                    self.written_into_itself(recv, &args[0])?;
                }
                self.not_locked(recv, Access::Write)?;
                Ok(Ty::Void)
            }
            // The bytes of the text, looked at where they are.
            (Ty::Str, "asBytes") => {
                arity(0)?;
                Ok(Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte))))))
            }
            // What `==` and `<` are short for, on the built-in types too.
            (Ty::Int(ty), "compare" | "equals") => {
                arity(1)?;
                let other = self.int_operand(&mut args[0], Some(*ty), "the argument")?;
                self.common_int(recv, recv_ty.clone(), &mut args[0], Ty::Int(other))?;
                Ok(if name == "compare" { Ty::Int(IntTy::Int) } else { Ty::Bool })
            }
            // A total order: `NaN` equals itself and comes before every other value, and
            // `-0.0` equals `0.0`. The operators stay as the hardware has them.
            (Ty::Float(ty), "compare" | "equals") => {
                arity(1)?;
                let found = self.operand(&mut args[0], Some(&Ty::Float(*ty)))?;
                if !found.strip_borrow().is_number() {
                    return Err(Diag::error(args[0].span, format!("expected `{target}`, found `{found}`")));
                }
                self.common_float(recv, recv_ty.clone(), &mut args[0], found)?;
                Ok(if name == "compare" { Ty::Int(IntTy::Int) } else { Ty::Bool })
            }
            (Ty::Float(_), "abs" | "sqrt" | "floor" | "ceil" | "round" | "truncate") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(target.clone())
            }
            (Ty::Float(_), "exp" | "log" | "log2" | "log10" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(target.clone())
            }
            // The rest of `<math.h>`: `x.sinh()`, `x.gamma()`, `x.roundToEven()`, `x.nextUp()`.
            (Ty::Float(_), _) if FLOAT_ONE.contains(&name) => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(target.clone())
            }
            (Ty::Float(_), "isFinite" | "isSignNegative") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Bool)
            }
            // `x.exponent()`: the power of two of `x`, as C's `ilogb` has it; `null` for 0, `NaN`
            // and the infinities, for which C gives a number of its own on each system.
            (Ty::Float(_), "exponent") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Nullable(Box::new(Ty::Int(IntTy::Int))))
            }
            // `x.scaleByPowerOfTwo(n)`: `x` times 2 to the `n`, C's `ldexp`.
            (Ty::Float(_), "scaleByPowerOfTwo") => {
                arity(1)?;
                self.int_operand(&mut args[0], Some(IntTy::Int), "the power of two")?;
                if let Ty::Int(found) = args[0].ty {
                    if found != IntTy::Int {
                        self.coerce(&mut args[0], Ty::Int(found), &Ty::Int(IntTy::Int))?;
                    }
                }
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(target.clone())
            }
            // `x.clamp(low, high)`: the three meet in one type, as with `+`.
            (Ty::Float(ty), "clamp") => {
                arity(2)?;
                let mut common = *ty;
                let mut recv_found = recv_ty.clone();
                for at in 0..2 {
                    let found = self.operand(&mut args[at], Some(&Ty::Float(common)))?;
                    if !found.strip_borrow().is_number() {
                        return Err(Diag::error(args[at].span, format!("expected `{}`, found `{found}`", common.name())));
                    }
                    common = self.common_float(recv, recv_found, &mut args[at], found)?;
                    recv_found = Ty::Float(common);
                }
                if let Ty::Float(first) = args[0].ty.clone() {
                    if first != common {
                        self.coerce(&mut args[0], Ty::Float(first), &Ty::Float(common))?;
                    }
                }
                clamp_ends(args)?;
                Ok(Ty::Float(common))
            }
            (Ty::Int(ty), "clamp") => {
                arity(2)?;
                let mut common = *ty;
                let mut recv_found = recv_ty.clone();
                for end in args.iter_mut() {
                    let other = self.int_operand(end, Some(common), "an end of `clamp`")?;
                    common = self.common_int(recv, recv_found, end, Ty::Int(other))?;
                    recv_found = Ty::Int(common);
                }
                if let Ty::Int(first) = args[0].ty.clone() {
                    if first != common {
                        self.coerce(&mut args[0], Ty::Int(first), &Ty::Int(common))?;
                    }
                }
                clamp_ends(args)?;
                Ok(Ty::Int(common))
            }
            // `x.abs()` on the signed types: `int.MinValue.abs()` overflows as `-x` does;
            // `tryAbs` gives `null` there, `wrappingAbs` wraps, and `unsignedAbs` gives the
            // unsigned type of the same size, which holds every answer.
            (Ty::Int(ty), "abs" | "tryAbs" | "wrappingAbs" | "unsignedAbs") => {
                arity(0)?;
                if !ty.signed() {
                    return Err(Diag::error(name_span, format!("a `{}` is never negative: `{name}` would give it back as it is", ty.name())));
                }
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(match name {
                    "tryAbs" => Ty::Nullable(Box::new(Ty::Int(*ty))),
                    "unsignedAbs" => Ty::Int(ty.unsigned()),
                    _ => Ty::Int(*ty),
                })
            }
            // `x.tryNegate()` and `x.wrappingNegate()`: `-x` that gives `null` or wraps, on every
            // integer type (`0 - x` on the unsigned ones).
            (Ty::Int(ty), "tryNegate" | "wrappingNegate") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(if name == "tryNegate" { Ty::Nullable(Box::new(Ty::Int(*ty))) } else { Ty::Int(*ty) })
            }
            // `x.pow(y)`, `y.atan2(x)`, `a.min(b)`: the two meet in one type, as with `+`.
            (Ty::Float(ty), "pow" | "atan2" | "min" | "max" | "hypot" | "copySign" | "ieeeRemainder") => {
                arity(1)?;
                let found = self.operand(&mut args[0], Some(&Ty::Float(*ty)))?;
                if !found.strip_borrow().is_number() {
                    return Err(Diag::error(args[0].span, format!("expected `{target}`, found `{found}`")));
                }
                Ok(Ty::Float(self.common_float(recv, recv_ty.clone(), &mut args[0], found)?))
            }
            // `a.fusedMultiplyAdd(b, c)`: `a * b + c` rounded once.
            (Ty::Float(ty), "fusedMultiplyAdd") => {
                arity(2)?;
                let mut common = *ty;
                let mut recv_found = recv_ty.clone();
                for at in 0..2 {
                    let found = self.operand(&mut args[at], Some(&Ty::Float(common)))?;
                    if !found.strip_borrow().is_number() {
                        return Err(Diag::error(args[at].span, format!("expected `{}`, found `{found}`", common.name())));
                    }
                    common = self.common_float(recv, recv_found, &mut args[at], found)?;
                    recv_found = Ty::Float(common);
                }
                // The first argument came before the type grew to `double` for the second.
                if let Ty::Float(first) = args[0].ty.clone() {
                    if first != common {
                        self.coerce(&mut args[0], Ty::Float(first), &Ty::Float(common))?;
                    }
                }
                Ok(Ty::Float(common))
            }
            // As with `+`: `b.max(300)` with a `byte b` is an `int`.
            (Ty::Int(_), "min" | "max") => {
                arity(1)?;
                let other = self.int_operand(&mut args[0], None, "the argument")?;
                Ok(Ty::Int(self.common_int(recv, recv_ty.clone(), &mut args[0], Ty::Int(other))?))
            }
            // `big.tryToInt()`: `null` if it does not fit; `big.wrappingToInt()`: cut to size;
            // `big.saturatingToInt()`: the nearest end of the type.
            (Ty::Int(_) | Ty::Float(_), _) if is_conversion(name) => {
                arity(0)?;
                let to = conversion(target, name, name_span)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(if name.starts_with("tryTo") { Ty::Nullable(Box::new(Ty::Int(to))) } else { Ty::Int(to) })
            }
            (Ty::Float(_), "isNaN" | "isInfinite") => {
                arity(0)?;
                self.coerce(recv, recv_ty.clone(), target)?;
                Ok(Ty::Bool)
            }
            (Ty::Str | Ty::Bool, "compare" | "equals") if name == "equals" || *target == Ty::Str => {
                arity(1)?;
                let other = self.operand(&mut args[0], None)?;
                if other.strip_borrow() != target {
                    return Err(Diag::error(args[0].span, format!("expected `{target}`, found `{other}`")));
                }
                Ok(if name == "compare" { Ty::Int(IntTy::Int) } else { Ty::Bool })
            }
            // The position of the first (or the last) place where the character or the text
            // is found, in bytes; `null` if there is none. A `byte` is looked for as it is.
            (Ty::Str, "indexOf" | "lastIndexOf") => {
                arity(1)?;
                self.needle(&mut args[0], true, name)?;
                Ok(Ty::Nullable(Box::new(Ty::Int(IntTy::Nint))))
            }
            (Ty::Str, "contains" | "startsWith" | "endsWith") => {
                arity(1)?;
                self.needle(&mut args[0], false, name)?;
                Ok(Ty::Bool)
            }
            // The text without the white space at its ends: a look at the same text.
            (Ty::Str, "trim" | "trimStart" | "trimEnd") => {
                arity(0)?;
                Ok(Ty::Read(Box::new(Ty::Str)))
            }
            // A new text in the other case, by the full mappings (`"straße".toUpper()` is
            // `"STRASSE"`), or folded as it is compared without case.
            (Ty::Str, "toUpper" | "toLower" | "foldCase") => {
                arity(0)?;
                Ok(Ty::Str)
            }
            (Ty::Str, "equalsIgnoreAsciiCase" | "equalsIgnoreCase") => {
                arity(1)?;
                self.typed(&mut args[0], &Ty::Read(Box::new(Ty::Str)))?;
                Ok(Ty::Bool)
            }
            (Ty::Str, "chars" | "splitWhitespace" | "lines") => {
                arity(0)?;
                Ok(Ty::TextIter(match name {
                    "chars" => TextIter::Chars,
                    "splitWhitespace" => TextIter::SplitWhitespace,
                    _ => TextIter::Lines,
                }))
            }
            // A new text with every `old` replaced by `new`; the text itself stays as it was.
            (Ty::Str, "replaceAll") => {
                arity(2)?;
                if let ExprKind::Str(text) = &args[0].kind {
                    if text.is_empty() {
                        return Err(Diag::error(args[0].span, "the text that `replaceAll` looks for is empty, so there is nothing to replace")
                            .hint("to put a text between the characters, go over them: `for c in text.chars()`"));
                    }
                }
                self.needle(&mut args[0], false, name)?;
                let text = Ty::Read(Box::new(Ty::Str));
                let found = self.operand(&mut args[1], Some(&text))?;
                match found.strip_borrow() {
                    Ty::Char => self.coerce(&mut args[1], found, &Ty::Char)?,
                    Ty::Str => self.coerce(&mut args[1], found, &text)?,
                    _ => return Err(Diag::error(args[1].span, format!("`replaceAll` puts in a `char` or a `read string`, found `{found}`"))),
                }
                Ok(Ty::Str)
            }
            // The text `n` times over, as a new text.
            (Ty::Str, "repeat") => {
                arity(1)?;
                if matches!(args[0].kind, ExprKind::Unary(UnOp::Neg, _)) || matches!(args[0].kind, ExprKind::Int(value, _) if value < 0) {
                    return Err(Diag::error(args[0].span, "a text cannot be repeated a negative number of times"));
                }
                self.typed(&mut args[0], &Ty::Int(IntTy::Nint))?;
                Ok(Ty::Str)
            }
            (Ty::Str, "split") => {
                arity(1)?;
                if let ExprKind::Str(text) = &args[0].kind {
                    if text.is_empty() {
                        return Err(Diag::error(args[0].span, "the separator of `split` is empty, so there is nothing to split at")
                            .hint("the characters, one after the other: `text.chars()`"));
                    }
                }
                self.needle(&mut args[0], false, name)?;
                Ok(Ty::TextIter(TextIter::Split))
            }
            // The next part, or `null` when the text is used up.
            (Ty::TextIter(kind), "next") => {
                arity(0)?;
                if !writable {
                    return Err(self.read_only(recv, Diag::error(recv.span, "`next` moves the iterator on, and this one is reached through a `read` borrow")));
                }
                if is_place {
                    self.not_locked(recv, Access::Write)?;
                }
                Ok(match kind.item() {
                    Ty::Char => Ty::Nullable(Box::new(Ty::Char)),
                    item @ Ty::Result(..) => Ty::Nullable(Box::new(item)),
                    _ => Ty::Read(Box::new(Ty::Nullable(Box::new(Ty::Str)))),
                })
            }
            (Ty::TextIter(_), _) => Err(Diag::error(name_span, format!("`{target}` has no method `{name}`"))
                .note("it has `next()`, and `for` goes over it")),
            (Ty::Formatter, "write") => {
                arity(1)?;
                if !writable {
                    return Err(Diag::error(recv.span, "`write` needs the `Formatter` as `ref`"));
                }
                let ty = self.operand(&mut args[0], None)?;
                self.printable(&ty, args[0].span)?;
                Ok(Ty::Void)
            }
            (Ty::Nullable(_), _) => {
                let diag = Diag::error(recv.span, format!("this is `{recv_ty}` and may be `null`; `.{name}()` needs a value"));
                Err(diag.hint(self.test_first(recv, &recv_ty)))
            }
            _ => {
                let diag = Diag::error(name_span, format!("type `{target}` has no method `{name}`"));
                const FRACTIONAL: &[&str] = &[
                    "sqrt", "floor", "ceil", "round", "truncate", "exp", "log", "log2", "log10", "sin", "cos", "tan", "asin", "acos", "atan",
                    "atan2", "pow", "fusedMultiplyAdd", "hypot", "copySign", "ieeeRemainder", "scaleByPowerOfTwo", "exponent", "isFinite",
                    "isSignNegative", "isNaN", "isInfinite",
                ];
                if matches!(target, Ty::Int(_)) && (FRACTIONAL.contains(&name) || FLOAT_ONE.contains(&name)) {
                    let text = &self.src[recv.span.start..recv.span.end];
                    let given = if matches!(name, "atan2" | "pow" | "fusedMultiplyAdd" | "hypot" | "copySign" | "ieeeRemainder" | "scaleByPowerOfTwo") { "..." } else { "" };
                    return Err(diag.hint(format!("it is a method of `double`: `((double){text}).{name}({given})`")));
                }
                // The names of C's `<math.h>`, and of the methods of an integer, that are written
                // another way here.
                if matches!(target, Ty::Int(_) | Ty::Float(_)) {
                    let float = matches!(target, Ty::Float(_));
                    let instead = match name {
                        "fmod" => Some("`a % b` is C's `fmod`"),
                        "remainder" if float => Some("`a.ieeeRemainder(b)`; `a % b` is C's `fmod`"),
                        "rint" | "nearbyint" | "roundeven" | "roundTiesEven" => Some("`x.roundToEven()`"),
                        "tgamma" => Some("`x.gamma()`"),
                        "lgamma" => Some("`x.logGamma()`"),
                        "copysign" => Some("`x.copySign(y)`"),
                        "nextafter" | "nexttoward" => Some("`x.nextUp()` or `x.nextDown()`"),
                        "modf" | "fract" => Some("`x.fraction()`, and `x.truncate()` for the whole part"),
                        "frexp" => Some("`x.exponent()`, and `x.scaleByPowerOfTwo(-e)` for the rest"),
                        "ldexp" | "scalbn" | "scaleB" => Some("`x.scaleByPowerOfTwo(n)`"),
                        "ilogb" | "logb" => Some("`x.exponent()`"),
                        "expm1" | "expM1" => Some("`x.expMinusOne()`"),
                        "log1p" | "logP1" | "ln1p" => Some("`x.logOnePlus()`"),
                        "fabs" => Some("`x.abs()`"),
                        "fmin" | "fmax" => Some("`a.min(b)` and `a.max(b)`"),
                        "trunc" => Some("`x.truncate()`"),
                        "isfinite" => Some("`x.isFinite()`"),
                        "signbit" | "isNegative" => Some("`x.isSignNegative()`"),
                        _ => None,
                    };
                    if let Some(instead) = instead {
                        return Err(diag.hint(instead));
                    }
                }
                if matches!(target, Ty::Int(_)) && (ASCII_TESTS.contains(&name) || matches!(name, "toAsciiUpper" | "toAsciiLower")) {
                    return Err(diag.note("the ASCII methods are on `byte` and `char`"));
                }
                if matches!(target, Ty::Int(_)) && matches!(name, "isWhitespace" | "utf8Length") {
                    return Err(diag.note(format!("`{name}` is a method of `char`")));
                }
                // A byte is ASCII to these methods; the character of a byte past ASCII is half a
                // character.
                if matches!(target, Ty::Int(_)) && (UNICODE_TESTS.contains(&name) || matches!(name, "toUpper" | "toLower")) {
                    let ascii = match name {
                        "isLetter" => "isAsciiLetter",
                        "isNumeric" => "isAsciiDigit",
                        "isAlphanumeric" => "isAsciiAlphanumeric",
                        "isUpper" => "isAsciiUpper",
                        "isLower" => "isAsciiLower",
                        "toUpper" => "toAsciiUpper",
                        "toLower" => "toAsciiLower",
                        _ => "isAscii",
                    };
                    let note = format!("`{name}` reads the Unicode tables, and is a method of `char`");
                    let text = &self.src[recv.span.start..recv.span.end];
                    return Err(match name {
                        "isControl" => diag.hint(format!("the ASCII control characters: `{text} < 32 || {text} == 127`")).note(note),
                        _ => diag.hint(format!("`{ascii}()`")).note(note),
                    });
                }
                if let (Ty::Int(_), "fromLittleEndian" | "fromBigEndian") = (target, name) {
                    return Err(diag.hint(format!("`{name}` is a function of the type: `{target}.{name}(bytes)`")));
                }
                // C#'s and Java's `ToString()`, Rust's `to_string()`, `to_owned()`.
                if *target == Ty::Str && matches!(name, "toString" | "ToString" | "to_string" | "toOwned" | "to_owned" | "copy") {
                    let text = &self.src[recv.span.start..recv.span.end];
                    return Err(diag.hint(format!("it is a `string` already; a copy of one is `{text}.clone()`")));
                }
                if let (Ty::Int(_), Some(method)) = (target, endian_hint(name, false)) {
                    return Err(diag.hint(format!("`{method}()` gives the bytes of the number")));
                }
                if matches!(target, Ty::Char | Ty::Str) {
                    return Err(text_method_hint(diag, target, name));
                }
                let (Ty::Struct(owner) | Ty::Enum(owner)) = target else {
                    return Err(if name.starts_with("try") { diag.note(format!("`{name}` is not in the interpreter yet")) } else { diag });
                };
                if structs.get(owner).is_some_and(|info| info.fields.iter().any(|(field, _)| field == name)) {
                    return Err(diag.hint(format!("`{name}` is a field; read it without the parentheses")));
                }
                Err(match nearest(name, self.methods_of(owner).keys().map(String::as_str)) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag,
                })
            }
        }
    }

    /// What a text is searched for: a `char` or a `read string`, and a `byte` too where only
    /// a position comes back.
    fn needle(&mut self, arg: &mut Expr, bytes: bool, method: &str) -> Res<()> {
        let text = Ty::Read(Box::new(Ty::Str));
        let found = self.operand(arg, Some(&text))?;
        match found.strip_borrow() {
            Ty::Char => self.coerce(arg, found, &Ty::Char),
            Ty::Int(IntTy::Byte) if bytes => self.coerce(arg, found.clone(), &Ty::Int(IntTy::Byte)),
            Ty::Str => self.coerce(arg, found, &text),
            // A `shared string` lent with `read` is looked at where it lies, as a parameter takes it
            // (choice 1816).
            Ty::Shared(inner, _) if **inner == Ty::Str => self.coerce(arg, found, &text),
            Ty::Int(IntTy::Byte) => Err(Diag::error(arg.span, format!("`{method}` looks for a `char` or a `read string`, found `byte`"))
                .hint("the character of an ASCII byte: `(char)b`")),
            _ => Err(Diag::error(arg.span, format!("`{method}` looks for a `char` or a `read string`, found `{found}`"))),
        }
    }

    /// A method of the built-in `List<T>` or `HashMap<K, V>`.
    #[allow(clippy::too_many_arguments)]
    fn collection_method(
        &mut self,
        recv: &Expr,
        target: &Ty,
        shown: &Ty,
        writable: bool,
        name: &str,
        name_span: Span,
        args: &mut [Expr],
        span: Span,
    ) -> Res<Ty> {
        let arity = |count: usize| {
            if args.len() == count {
                Ok(())
            } else {
                Err(Diag::error(span, format!("`{name}` takes {count} argument(s), {} given", args.len())))
            }
        };
        let what = match shown {
            Ty::List(_) => "list",
            Ty::Map(..) => "map",
            Ty::Array(..) => "array",
            Ty::Slice(_) => "slice",
            _ => "set",
        };
        let changing = matches!(name, "add" | "addAll" | "clear" | "removeLast" | "reverse" | "remove" | "getOrAdd" | "sort" | "copyFrom");
        if changing && writable {
            self.not_locked(recv, Access::Write)?;
        }
        if changing && !writable {
            if let Some(diag) = self.readonly(recv) {
                return Err(diag.note(format!("`{name}` changes the {what}")));
            }
        }
        // What `indexOf`, `contains` and `sort` can compare until `Equatable` and `Comparable` exist.
        let item = match target {
            Ty::List(item) | Ty::Set(item) => Some(&**item),
            _ => None,
        };
        let can_equal = item.is_some_and(|item| has(item, self.equatable, true));
        // A list of borrows (1732) orders by what its items point at.
        let can_order = item.is_some_and(|item| match item.strip_borrow() {
            Ty::Int(_) | Ty::Float(_) | Ty::Char | Ty::Str => true,
            Ty::Struct(name) | Ty::Enum(name) => self.comparable.contains(name),
            _ => false,
        });
        let compares = |item: &Ty, needs: &str| match item {
            _ if needs == "Equatable" && can_equal => Ok(()),
            _ if needs == "Comparable" && can_order => Ok(()),
            Ty::Struct(inner) | Ty::Enum(inner) => Err(lacks(Diag::error(name_span, format!("`{name}` compares the items, and `{inner}` is not `{needs}`")), inner, needs)),
            Ty::List(_) | Ty::Set(_) | Ty::Map(..) if needs == "Comparable" => Err(Diag::error(name_span, format!("`{name}` {} the items in order, and a collection has no order of its own", if name == "sort" { "puts" } else { "looks for a place among" }))),
            other => Err(Diag::error(name_span, format!("`{name}` compares the items, and `{other}` is not `{needs}`"))
                .note(format!("`{needs}`: numbers{}, `char`, `string` and the types written with `: {needs}`", if needs == "Equatable" { ", `bool`" } else { "" }))),
        };
        let changes = || {
            if writable {
                Ok(())
            } else {
                let diag = Diag::error(recv.span, format!("`{name}` changes the {what}, but it is reached through a `read` borrow"));
                Err(self.read_only(recv, match shown {
                    Ty::Slice(item) => diag.hint(format!("lend it as a `ref {item}[]`: `ref xs` or `ref xs[a..b]`")),
                    _ => diag.hint("take the value as `ref` instead of `read`"),
                }))
            }
        };
        let nullable = |ty: &Ty| Ty::Nullable(Box::new(ty.clone()));
        match (target, name) {
            (_, "length") => {
                arity(0)?;
                Ok(Ty::Int(IntTy::Nint))
            }
            (_, "clear") => {
                arity(0)?;
                changes()?;
                self.release(recv);
                Ok(Ty::Void)
            }
            (Ty::List(item), "add") => {
                arity(1)?;
                changes()?;
                self.typed(&mut args[0], item)?;
                if self.holds(item) {
                    self.stores(recv, &args[0], true)?;
                }
                Ok(Ty::Void)
            }
            // The items of a slice, copied to the end: one copy of a block in compiled code.
            (Ty::List(item), "addAll") => {
                arity(1)?;
                changes()?;
                if self.holds(item) {
                    return Err(Diag::error(name_span, format!("`addAll` on a `List<{item}>` is not in the language yet: the items carry borrows"))
                        .hint("add them one by one with `add`"));
                }
                if !item.is_copyable(self.copyable) {
                    let diag = Diag::error(name_span, format!("`addAll` copies the items, and `{item}` is not `Copyable`"));
                    return Err(match &**item {
                        Ty::Str => diag.hint(format!("add them one by one, each a copy: `{}.add(item.clone())`", &self.src[recv.span.start..recv.span.end])),
                        Ty::Struct(inner) | Ty::Enum(inner) => diag.hint(format!("make `{inner}` `Copyable`, or add the items one by one with `add`")),
                        _ => diag.hint("add them one by one with `add`"),
                    });
                }
                self.args(name, args, &[Ty::Read(Box::new(Ty::Slice(item.clone())))], span)?;
                self.written_into_itself(recv, &args[0])?;
                Ok(Ty::Void)
            }
            // The items of a window of as many, copied over these: one `memmove` in compiled
            // code, so the two may be windows of one place (choice 1609). Lengths that differ
            // are a panic, as Rust's `copy_from_slice` and Zig's `@memcpy`.
            (Ty::List(item), "copyFrom") => {
                arity(1)?;
                changes()?;
                if self.holds(item) {
                    return Err(Diag::error(name_span, format!("`copyFrom` on items of `{item}` is not in the language yet: the items carry borrows"))
                        .hint("copy them one by one: `xs[i] = ys[i]`"));
                }
                if !item.is_copyable(self.copyable) {
                    let diag = Diag::error(name_span, format!("`copyFrom` copies the items, and `{item}` is not `Copyable`"));
                    return Err(match &**item {
                        Ty::Str => diag.hint("copy them one by one, each a copy: `xs[i] = ys[i].clone()`"),
                        Ty::Struct(inner) | Ty::Enum(inner) => diag.hint(format!("make `{inner}` `Copyable`, or copy the items one by one")),
                        _ => diag.hint("copy them one by one"),
                    });
                }
                self.args(name, args, &[Ty::Read(Box::new(Ty::Slice(item.clone())))], span)?;
                Ok(Ty::Void)
            }
            // `null` where `xs[i]` would panic. A list of windows (1732) gives a `read T[]?` (1774).
            // On a list of borrows (1732) both give the borrow itself, or `null`: `read T?`.
            (Ty::List(item), "get") => {
                arity(1)?;
                self.index(&mut args[0], "an index")?;
                Ok(Ty::Read(Box::new(nullable(item.strip_borrow()))))
            }
            // `xs[i]` without the check of the index in a release build (1803): only the
            // language's own module writes it, inside `unsafe`, where a loop has made sure of the
            // index another way (the moves of the DFA of `Regex`). The interpreter and a debug
            // build check it all the same. The checker then makes it the item itself
            // (`unchecked_index`).
            (Ty::List(item), "getUnchecked") => {
                if self.module != STD_MODULE {
                    let list = &self.src[recv.span.start..recv.span.end];
                    return Err(Diag::error(name_span, "`getUnchecked` is for the language's own module only")
                        .note("it reads an item without checking its index, which only `std` does, where the index is known to be inside")
                        .hint(format!("`{list}[i]`, which checks the index, or `{list}.get(i)`, which gives `null` past the end")));
                }
                arity(1)?;
                if item.is_borrow() || !item.is_copyable(self.copyable) {
                    return Err(Diag::error(span, format!("`getUnchecked` gives an item of a `Copyable` type, and this one is `{item}`")));
                }
                self.index(&mut args[0], "an index")?;
                self.needs_unsafe(span, "this reads an item without checking its index")?;
                Ok((**item).clone())
            }
            (Ty::List(item), "removeLast") => {
                arity(0)?;
                changes()?;
                Ok(match &**item {
                    Ty::Read(inner) => Ty::Read(Box::new(nullable(inner))),
                    _ => nullable(item),
                })
            }
            (Ty::List(_), "reverse") => {
                arity(0)?;
                changes()?;
                Ok(Ty::Void)
            }
            (Ty::List(item), "indexOf") => {
                arity(1)?;
                compares(item, "Equatable")?;
                // Bytes look for a run of bytes too, lent with `read`, as a text looks for a text:
                // `data.indexOf(read "\r\n".asBytes())` (choice 1843).
                let byte = Ty::Int(IntTy::Byte);
                if **item == byte {
                    let found = self.expr(&mut args[0], Some(&byte))?;
                    let run = matches!(found.strip_borrow(), Ty::List(of) | Ty::Array(of, _) | Ty::Slice(of) if **of == byte);
                    let wanted = if run { Ty::Read(Box::new(Ty::Slice(Box::new(byte)))) } else { byte };
                    self.coerce(&mut args[0], found, &wanted)?;
                    return Ok(nullable(&Ty::Int(IntTy::Nint)));
                }
                self.typed(&mut args[0], &self.key_param(item))?;
                Ok(nullable(&Ty::Int(IntTy::Nint)))
            }
            (Ty::List(item), "contains") => {
                arity(1)?;
                compares(item, "Equatable")?;
                self.typed(&mut args[0], &self.key_param(item))?;
                Ok(Ty::Bool)
            }
            // A search that halves the items each time: they are in order (`sort`), and that is
            // not checked, which would look at every one. Out of order, the place given is
            // some place in the list or `null`.
            (Ty::List(item), "binarySearch" | "lowerBound" | "upperBound") => {
                arity(1)?;
                compares(item, "Comparable")?;
                self.typed(&mut args[0], &self.key_param(item))?;
                Ok(match name {
                    "binarySearch" => nullable(&Ty::Int(IntTy::Nint)),
                    _ => Ty::Int(IntTy::Nint),
                })
            }
            (Ty::List(item), "sort") => {
                arity(0)?;
                changes()?;
                compares(item, "Comparable")?;
                Ok(Ty::Void)
            }
            // `true` if the item was not there before.
            (Ty::Set(item), "add") => {
                arity(1)?;
                changes()?;
                self.typed(&mut args[0], item)?;
                if self.holds(item) {
                    self.stores(recv, &args[0], true)?;
                }
                Ok(Ty::Bool)
            }
            (Ty::Set(item), "contains") => {
                arity(1)?;
                self.typed(&mut args[0], &self.table_key_param(item))?;
                Ok(Ty::Bool)
            }
            // `true` if the item was there.
            (Ty::Set(item), "remove") => {
                arity(1)?;
                changes()?;
                self.typed(&mut args[0], &self.table_key_param(item))?;
                Ok(Ty::Bool)
            }
            (Ty::Map(key, value), "add") => {
                arity(2)?;
                changes()?;
                self.typed(&mut args[0], key)?;
                self.typed(&mut args[1], value)?;
                if self.holds(key) {
                    self.stores(recv, &args[0], true)?;
                }
                if self.holds(value) {
                    self.stores(recv, &args[1], true)?;
                }
                Ok(Ty::Void)
            }
            (Ty::Map(key, value), "get") => {
                arity(1)?;
                self.typed(&mut args[0], &self.table_key_param(key))?;
                Ok(Ty::Read(Box::new(nullable(value))))
            }
            (Ty::Map(key, _), "contains") => {
                arity(1)?;
                self.typed(&mut args[0], &self.table_key_param(key))?;
                Ok(Ty::Bool)
            }
            (Ty::Map(key, value), "remove") => {
                arity(1)?;
                changes()?;
                self.typed(&mut args[0], &self.table_key_param(key))?;
                Ok(nullable(value))
            }
            (Ty::Map(key, value), "getOrAdd") => {
                arity(2)?;
                changes()?;
                self.typed(&mut args[0], &self.table_key_param(key))?;
                self.typed(&mut args[1], value)?;
                if self.holds(key) {
                    self.stores(recv, &args[0], true)?;
                }
                if self.holds(value) {
                    self.stores(recv, &args[1], true)?;
                }
                Ok(Ty::Ref(value.clone()))
            }
            _ => {
                let known: &[&str] = match target {
                    Ty::List(_) => &["add", "addAll", "get", "length", "indexOf", "contains", "binarySearch", "lowerBound", "upperBound", "sort", "removeLast", "reverse", "clear", "copyFrom"],
                    Ty::Map(..) => &["add", "get", "getOrAdd", "contains", "remove", "length", "clear"],
                    _ => &["add", "contains", "remove", "length", "clear"],
                };
                // Names the same thing has in other languages.
                let other_name = match (target, name) {
                    (Ty::List(_), "extend" | "extendFromSlice" | "addRange" | "pushAll" | "appendAll" | "concat") => Some("addAll"),
                    (Ty::List(_), "copyFromSlice" | "copy_from_slice") => Some("copyFrom"),
                    (_, "push" | "append" | "insert" | "put" | "set") => Some("add"),
                    (_, "count" | "size" | "len" | "Count") => Some("length"),
                    (Ty::List(_), "pop") => Some("removeLast"),
                    (_, "containsKey" | "has" | "includes") => Some("contains"),
                    (Ty::List(_), "find") => Some("indexOf"),
                    (Ty::List(_), _) if search_name(name).is_some() => search_name(name),
                    (Ty::List(_), "at") => Some("get"),
                    _ => None,
                };
                let diag = Diag::error(name_span, format!("`{target}` has no method `{name}`"));
                // `List<int>().repeat(0, n)`: a function of the type, written on a list (1648).
                if let (Ty::List(_), "repeat" | "filled" | "fill" | "withLength") = (target, name) {
                    return Err(diag.hint(format!("a list of `n` copies of an item is made by the type: `{}.repeat(item, n)`", target.strip_borrow())));
                }
                if let (Ty::List(_), "withCapacity" | "reserve" | "ensureCapacity") = (target, name) {
                    return Err(diag.hint(format!("room is taken when the list is made: `{}.withCapacity(n)`", target.strip_borrow())));
                }
                Err(match other_name.or_else(|| nearest(name, known.iter().copied())) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag.note(format!("methods for now: {}", known.join(", "))),
                })
            }
        }
    }

    /// The methods of `StreamReader` and `StreamWriter`. Reading and writing go on from where
    /// the stream is, so they need it as `ref`; `close` takes the writer.
    #[allow(clippy::too_many_arguments)]
    fn stream_method(
        &mut self,
        recv: &mut Expr,
        target: &Ty,
        writable: bool,
        borrowed: bool,
        name: &str,
        shown: &str,
        name_span: Span,
        args: &mut [Expr],
        callee: &mut Callee,
        span: Span,
    ) -> Res<Ty> {
        let arity = |count: usize| match args.len() == count {
            true => Ok(()),
            false => Err(Diag::error(span, format!("`{shown}` takes {count} argument(s), {} given", args.len()))),
        };
        let result = |ok: Ty| Ty::Result(Box::new(ok), Box::new(Ty::Enum(IO_ERROR.to_string())));
        let call = match (target, name) {
            (Ty::Reader(_), "readLine") => StreamCall::ReadLine,
            (Ty::Reader(_), "readLineBytes") => StreamCall::ReadLineBytes,
            (Ty::Reader(_), "readText") => StreamCall::ReadText,
            (Ty::Reader(_), "lines") => StreamCall::Lines,
            (Ty::Reader(_), "readBytes") => StreamCall::ReadBytes,
            (Ty::Reader(_), "readSome") => StreamCall::ReadSome,
            (Ty::Reader(_), "fill") => StreamCall::Fill,
            (Ty::Reader(_), "buffered") => StreamCall::Buffered,
            (Ty::Reader(_), "consume") => StreamCall::Consume,
            (Ty::Writer, "write") => StreamCall::Write,
            (Ty::Writer, "writeLine") => StreamCall::WriteLine,
            (Ty::Writer, "writeBytes") => StreamCall::WriteBytes,
            (Ty::Writer, "flush") => StreamCall::Flush,
            (Ty::Writer, "close") => StreamCall::Close,
            (Ty::Writer, "syncData") => StreamCall::SyncData,
            (Ty::Writer, "sync") => StreamCall::Sync,
            (Ty::Writer, "setLength") => StreamCall::SetLength,
            (_, "seek") => StreamCall::Seek,
            _ => {
                let known: &[&str] = match target {
                    Ty::Reader(_) => &["readLine", "readLineBytes", "readText", "lines", "readBytes", "readSome", "fill", "buffered", "consume", "seek", "info"],
                    _ => &["write", "writeLine", "writeBytes", "flush", "close", "seek", "syncData", "sync", "setLength"],
                };
                let diag = Diag::error(name_span, format!("`{target}` has no method `{name}`"));
                let diag = match nearest(name, known.iter().copied()) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag.note(format!("methods: {}", known.join(", "))),
                };
                return Err(match (target, name) {
                    (Ty::Reader(_), "close") => diag.note("a reader has nothing to lose: its file is closed when it ends"),
                    (Ty::Reader(_), "write" | "writeLine" | "writeBytes" | "flush" | "syncData" | "sync" | "setLength") => diag.note("a `StreamReader` reads; `Stdout`, `Stderr`, `File.createText(path)`, `File.openWrite(path)` and `File.openAppend(path)` give a `StreamWriter`"),
                    (Ty::Writer, "readLine" | "readLineBytes" | "readText" | "lines" | "readBytes" | "readSome" | "fill" | "buffered" | "consume") => diag.note("a `StreamWriter` writes; `Stdin` and `File.openText(path)` give a `StreamReader`"),
                    _ => diag,
                });
            }
        };
        *callee = Callee::Stream(call);
        if call == StreamCall::Close {
            arity(0)?;
            if let ExprKind::Var(var) = &recv.kind {
                if let Some(what) = std_stream(var) {
                    return Err(Diag::error(recv.span, format!("`{var}` is {what} of the program and stays open until it ends"))
                        .hint(format!("`try {var}.flush()` writes what it holds")));
                }
            }
            if borrowed {
                return Err(Diag::error(recv.span, "`close` takes the writer and uses it up, but this writer is only borrowed")
                    .note("whoever owns the writer closes it")
                    .hint("`flush()` writes what it holds through a borrow"));
            }
            self.move_out(recv, &Ty::Writer)?;
            recv.moved = true;
            return Ok(result(Ty::Void));
        }
        if call == StreamCall::Seek {
            if let ExprKind::Var(var) = &recv.kind {
                if let Some(what) = std_stream(var) {
                    let opened = if var == "Stdin" { "`File.openText(path)`" } else { "`File.createText(path)` or `File.openWrite(path)`" };
                    return Err(Diag::error(recv.span, format!("`{var}` is {what} of the program, which goes only forward"))
                        .note(format!("a file opened with {opened} can seek")));
                }
            }
            if *target == Ty::Reader(true) {
                return Err(Diag::error(recv.span, "the reader of a connection cannot seek: what has come over the network goes only forward")
                    .note("a file opened with `File.openText(path)` can seek"));
            }
        }
        // A standard stream is not a file the program holds: it may be a terminal or a pipe, and
        // a file it was sent into is the caller's to keep.
        if matches!(call, StreamCall::SyncData | StreamCall::Sync | StreamCall::SetLength) {
            if let ExprKind::Var(var) = &recv.kind {
                if let Some(what) = std_stream(var) {
                    let does = if call == StreamCall::SetLength { "has no length to set" } else { "is not a file the program can put on the disk" };
                    return Err(Diag::error(recv.span, format!("`{var}` is {what} of the program, which {does}"))
                        .note("a file opened with `File.createText(path)`, `File.openWrite(path)` or `File.openAppend(path)` can"));
                }
            }
        }
        if call == StreamCall::Buffered {
            arity(0)?;
            self.not_locked(recv, Access::Read)?;
            return Ok(Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte))))));
        }
        if !writable {
            let what = match call {
                StreamCall::Seek => "moves the stream",
                StreamCall::Consume => "gives out what the block holds",
                _ => "goes on from where the stream is",
            };
            if let Some(diag) = self.readonly(recv) {
                return Err(diag.note(format!("`{shown}` {what}")));
            }
            return Err(self.read_only(recv, Diag::error(recv.span, format!("`{shown}` {what}, but it is reached through a `read` borrow"))
                .hint(format!("take it as `ref {target}` instead of `read {target}`"))));
        }
        let ty = match call {
            StreamCall::ReadLine => {
                self.args(shown, args, &[Ty::Ref(Box::new(Ty::Str))], span)?;
                result(Ty::Bool)
            }
            StreamCall::ReadLineBytes => {
                self.args(shown, args, &[Ty::Ref(Box::new(Ty::List(Box::new(Ty::Int(IntTy::Byte)))))], span)?;
                result(Ty::Bool)
            }
            StreamCall::ReadText => {
                arity(0)?;
                result(Ty::Str)
            }
            StreamCall::Lines => {
                arity(0)?;
                Ty::TextIter(TextIter::ReadLines)
            }
            StreamCall::Fill => {
                arity(0)?;
                result(Ty::Bool)
            }
            StreamCall::Consume => {
                self.args(shown, args, &[Ty::Int(IntTy::Nint)], span)?;
                Ty::Void
            }
            StreamCall::Buffered => unreachable!("answered above"),
            StreamCall::ReadBytes | StreamCall::ReadSome => {
                let lent_read = args.first().is_some_and(|arg| self.src[arg.span.start..arg.span.end].starts_with("read "));
                self.args(shown, args, &[Ty::Ref(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))))], span).map_err(|diag| match lent_read {
                    true => diag.hint(format!("`{shown}` writes into the buffer, so it is lent with `ref`")),
                    false => diag,
                })?;
                result(Ty::Int(IntTy::Nint))
            }
            StreamCall::WriteBytes => {
                self.args(shown, args, &[Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))))], span)?;
                result(Ty::Void)
            }
            StreamCall::Write | StreamCall::WriteLine => {
                arity(1)?;
                let ty = self.operand(&mut args[0], None)?;
                self.printable(&ty, args[0].span)?;
                result(Ty::Void)
            }
            StreamCall::Flush | StreamCall::Close | StreamCall::SyncData | StreamCall::Sync => {
                arity(0)?;
                result(Ty::Void)
            }
            StreamCall::SetLength => {
                self.args(shown, args, &[Ty::Int(IntTy::Long)], span)?;
                result(Ty::Void)
            }
            StreamCall::Seek => {
                arity(1)?;
                let from = Ty::Enum(format!("{STD_MODULE}.SeekFrom"));
                let known = self.enums.contains_key(&format!("{STD_MODULE}.SeekFrom"));
                let ty = match known {
                    true => self.operand(&mut args[0], Some(&from))?,
                    false => self.operand(&mut args[0], None)?,
                };
                if ty != from {
                    let given = &self.src[args[0].span.start..args[0].span.end];
                    return Err(Diag::error(args[0].span, format!("`seek` takes a `SeekFrom`, and this is `{ty}`"))
                        .hint(format!("`SeekFrom.Start({given})` counts from the start of the file; `End` from its end, `Current` from where it stands")));
                }
                result(Ty::Int(IntTy::Long))
            }
        };
        self.not_locked(recv, Access::Write)?;
        Ok(ty)
    }

    /// `slot.take()`: gives the value of a `T?` place and leaves `null` there.
    #[allow(clippy::too_many_arguments)]
    fn take(
        &mut self,
        recv: &mut Expr,
        recv_ty: &Ty,
        writable: bool,
        is_place: bool,
        args: &[Expr],
        callee: &mut Callee,
        span: Span,
    ) -> Res<Ty> {
        if !args.is_empty() {
            return Err(Diag::error(span, format!("`take` takes 0 argument(s), {} given", args.len())));
        }
        if !is_place {
            return Err(Diag::error(recv.span, "`take` needs a variable or a field: it leaves `null` there"));
        }
        // A variable known to hold a value is still a `T?` slot.
        let declared = match &recv.kind {
            ExprKind::Var(name) => self.var(name, recv.span)?.declared.clone(),
            _ => recv_ty.clone(),
        };
        let slot = declared.strip_borrow();
        if !matches!(slot, Ty::Nullable(_)) {
            let text = &self.src[recv.span.start..recv.span.end];
            return Err(Diag::error(recv.span, format!("`take` leaves `null` behind, so it needs a `T?`; this is `{slot}`"))
                .hint(format!("to put another value in its place: `{text}.replace(value)`")));
        }
        if let Some(diag) = self.readonly(recv) {
            return Err(diag);
        }
        if !writable {
            return Err(self.read_only(recv, Diag::error(recv.span, "this is reached through a `read` borrow, so nothing can be taken out of it")
                .hint("take the value as `ref` instead of `read`")));
        }
        self.not_locked(recv, Access::Write)?;
        if let ExprKind::Var(name) = &recv.kind {
            let var = self.var(name, recv.span)?;
            var.current = var.declared.clone();
            var.assigned += 1;
        }
        *callee = Callee::Take;
        Ok(slot.clone())
    }

    /// `slot.replace(value)`: gives the old value of a place and leaves `value` there.
    #[allow(clippy::too_many_arguments)]
    fn replace(
        &mut self,
        recv: &mut Expr,
        recv_ty: &Ty,
        writable: bool,
        is_place: bool,
        args: &mut [Expr],
        callee: &mut Callee,
        span: Span,
    ) -> Res<Ty> {
        if args.len() != 1 {
            let diag = Diag::error(span, format!("`replace` takes 1 argument(s), {} given", args.len()));
            if args.len() == 2 && is_text_value(recv_ty) {
                let text = &self.src[recv.span.start..recv.span.end];
                let (old, new) = (&self.src[args[0].span.start..args[0].span.end], &self.src[args[1].span.start..args[1].span.end]);
                return Err(diag
                    .note("`replace(value)` puts a new value in the place and gives back the old one")
                    .hint(format!("a new text with every `{old}` replaced: `{text}.replaceAll({old}, {new})`")));
            }
            return Err(diag);
        }
        if !is_place {
            return Err(Diag::error(recv.span, "`replace` needs a variable or a field to put the new value in"));
        }
        if let Some(diag) = self.readonly(recv) {
            return Err(diag);
        }
        if !writable {
            return Err(self.read_only(recv, Diag::error(recv.span, "this is reached through a `read` borrow and cannot be replaced")
                .hint("take the value as `ref` instead of `read`")));
        }
        self.not_locked(recv, Access::Write)?;
        let slot = recv_ty.strip_borrow().clone();
        self.typed(&mut args[0], &slot)?;
        if self.holds(&slot) {
            self.stores(recv, &args[0], false)?;
        }
        *callee = Callee::Replace;
        Ok(slot)
    }
}

/// Collects the variables a block assigns or empties with `take`.
fn changed_in<'b>(block: &'b Block, names: &mut Vec<&'b str>) {
    for stmt in block {
        match &stmt.kind {
            StmtKind::Let { init, .. } => taken_in(init, names),
            StmtKind::Assign { target, value, .. } => {
                if let ExprKind::Var(name) = &target.kind {
                    names.push(name);
                }
                taken_in(target, names);
                taken_in(value, names);
            }
            StmtKind::If { cond, then, els, .. } => {
                taken_in(cond, names);
                changed_in(then, names);
                if let Some(els) = els {
                    changed_in(els, names);
                }
            }
            StmtKind::While { cond, body, .. } => {
                taken_in(cond, names);
                changed_in(body, names);
            }
            StmtKind::For { from, to, body, .. } => {
                taken_in(from, names);
                taken_in(to, names);
                changed_in(body, names);
            }
            StmtKind::ForEach { iter, body, .. } | StmtKind::As { value: iter, body, .. } => {
                taken_in(iter, names);
                changed_in(body, names);
            }
            StmtKind::Lock { items, when, body, .. } => {
                for item in items {
                    taken_in(&item.value, names);
                }
                if let Some(when) = when {
                    taken_in(when, names);
                }
                changed_in(body, names);
            }
            StmtKind::Unsafe(body) => changed_in(body, names),
            StmtKind::Return(Some(expr)) | StmtKind::Expr(expr) | StmtKind::Discard(expr) => taken_in(expr, names),
            StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Const { .. } | StmtKind::Static { .. } => {}
        }
    }
}

/// Collects the variables an expression empties with `x.take()`.
fn taken_in<'b>(expr: &'b Expr, names: &mut Vec<&'b str>) {
    if let ExprKind::Method { recv, name, .. } = &expr.kind {
        if let (ExprKind::Var(var), "take") = (&recv.kind, name.as_str()) {
            names.push(var);
        }
    }
    for part in parts(expr) {
        match part {
            Part::Expr(inner) => taken_in(inner, names),
            Part::Block(block) => changed_in(block, names),
        }
    }
}

/// What an expression is made of, one level down.
enum Part<'b> {
    Expr(&'b Expr),
    /// The block of an arm of a `match`.
    Block(&'b Block),
}

fn parts(expr: &Expr) -> Vec<Part<'_>> {
    let mut list = Vec::new();
    match &expr.kind {
        ExprKind::Method { recv, args, .. } => {
            list.push(Part::Expr(recv));
            list.extend(args.iter().map(Part::Expr));
        }
        ExprKind::Call { args, .. } | ExprKind::Variant { args, .. } | ExprKind::Allocate { args, .. } => list.extend(args.iter().map(Part::Expr)),
        ExprKind::StructLit { fields, .. } => list.extend(fields.iter().map(|field| Part::Expr(&field.value))),
        ExprKind::With { base, fields } => list.extend(std::iter::once(&**base).chain(fields.iter().map(|field| &field.value)).map(Part::Expr)),
        ExprKind::Interp(pieces) => {
            for piece in pieces {
                if let InterpPart::Expr(inner, _) = piece {
                    list.push(Part::Expr(inner));
                }
            }
        }
        ExprKind::Collection { args, items, .. } => {
            let given = items.iter().flatten().flat_map(|item| &item.values);
            list.extend(args.iter().chain(given).map(Part::Expr));
        }
        ExprKind::Array { args, items, .. } => list.extend(args.iter().chain(items.iter().flatten()).map(Part::Expr)),
        ExprKind::Binary(_, left, right) | ExprKind::Coalesce { left, right, .. } | ExprKind::Index { recv: left, index: right, .. } => {
            list.push(Part::Expr(left));
            list.push(Part::Expr(right));
        }
        ExprKind::Slice { recv, from, to } => {
            list.push(Part::Expr(recv));
            list.extend([from, to].into_iter().flatten().map(|end| Part::Expr(end)));
        }
        ExprKind::Match { subject, arms, .. } => {
            list.push(Part::Expr(subject));
            for arm in arms {
                list.extend(arm.guard.iter().map(Part::Expr));
                list.push(Part::Block(&arm.body));
                list.extend(arm.value.iter().map(Part::Expr));
            }
        }
        ExprKind::Field { recv: inner, .. }
        | ExprKind::IsVariant { expr: inner, .. }
        | ExprKind::Unbox(inner)
        | ExprKind::AsSlice(inner)
        | ExprKind::Try { expr: inner, .. }
        | ExprKind::Await(inner)
        | ExprKind::Unsafe(inner)
        | ExprKind::RawCast { expr: inner, .. }
        | ExprKind::Return(Some(inner))
        | ExprKind::Unary(_, inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::New(inner)
        | ExprKind::Atomic(inner)
        | ExprKind::NewShared { value: inner, .. }
        | ExprKind::Open { inner, .. }
        | ExprKind::Within(inner)
        | ExprKind::Share(inner)
        | ExprKind::WeakOf(inner)
        | ExprKind::Borrow { expr: inner, .. }
        | ExprKind::IsNull { expr: inner, .. }
        | ExprKind::Widen(_, inner)
        | ExprKind::Load(inner) => list.push(Part::Expr(inner)),
        ExprKind::Int(..)
        | ExprKind::Float(..)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Char(_)
        | ExprKind::Null
        | ExprKind::SizeOf { .. }
        | ExprKind::Var(_)
        | ExprKind::FuncRef(_)
        | ExprKind::CVar(_)
        | ExprKind::Constant { .. }
        | ExprKind::Const(_)
        | ExprKind::Static(_)
        | ExprKind::Embed(_)
        | ExprKind::Return(None) => {}
    }
    list
}

/// The character with this code, if there is one.
pub fn char_of(value: i128) -> Option<char> {
    u32::try_from(value).ok().and_then(char::from_u32)
}

/// The value of a character written where a `byte` goes: it must take one byte in UTF-8.
fn byte_of(value: char, span: Span) -> Res<i128> {
    if value.is_ascii() {
        return Ok(value as i128);
    }
    Err(Diag::error(span, format!("`{value}` does not fit in one byte: it takes {} bytes in UTF-8", value.len_utf8()))
        .hint(format!("the type for any character is `char`: `char c = '{value}'`")))
}

/// What the arms of a `match` on a value that is not an enum are written as. `Other` is a
/// `bool` or a `T?`, whose arms are checked as patterns inside a pattern are.
#[derive(Clone, Copy, PartialEq)]
enum Literal {
    Int(IntTy),
    Char,
    Text,
    Other,
}

/// The names of an `or`: each of its patterns names the same values, with the same types.
/// Gives those of the first.
fn same_names(alternatives: &[Pattern], all: Vec<Binds>) -> Res<Binds> {
    let first = &all[0];
    for (alternative, binds) in alternatives.iter().zip(&all).skip(1) {
        for (name, ty, _) in first {
            match binds.iter().find(|(other, _, _)| other == name) {
                None => {
                    return Err(Diag::error(alternative.span, format!("`{name}` is named in the first pattern of this `or` and not in this one"))
                        .note("every pattern of an `or` names the same values; write `_` in each for a value that is not needed"));
                }
                Some((_, other, span)) if other != ty => {
                    return Err(Diag::error(*span, format!("`{name}` is a `{ty}` in the first pattern of this `or` and a `{other}` here"))
                        .note("a name of an `or` has one type, whichever pattern took the value"));
                }
                Some(_) => {}
            }
        }
        if let Some((name, _, span)) = binds.iter().find(|(name, _, _)| !first.iter().any(|(other, _, _)| other == name)) {
            return Err(Diag::error(*span, format!("`{name}` is named here and not in the first pattern of this `or`"))
                .note("every pattern of an `or` names the same values; write `_` in each for a value that is not needed"));
        }
    }
    Ok(all.into_iter().next().unwrap_or_default())
}

/// The variants an arm names at its top, through `or`.
fn top_variants(pattern: &Pattern) -> Vec<i128> {
    match &pattern.kind {
        PatternKind::Variant { index, .. } => vec![*index as i128],
        PatternKind::Or(alternatives) => alternatives.iter().flat_map(top_variants).collect(),
        _ => Vec::new(),
    }
}

/// Whether the pattern takes `null` itself, alone or in an `or`.
fn takes_null(pattern: &Pattern) -> bool {
    match &pattern.kind {
        PatternKind::Null => true,
        PatternKind::Or(alternatives) => alternatives.iter().any(takes_null),
        _ => false,
    }
}

/// A `T?` once it is known not to be `null`: `T`, through the same borrow.
fn not_null(ty: &Ty) -> Ty {
    match ty {
        Ty::Read(inner) => Ty::Read(Box::new(not_null(inner))),
        Ty::Ref(inner) => Ty::Ref(Box::new(not_null(inner))),
        Ty::Nullable(inner) => (**inner).clone(),
        other => other.clone(),
    }
}

/// A range takes at least its first value: the first is not greater than the last.
fn range_order(low: &Pattern, high: &Pattern, shown: (String, String), span: Span) -> Res<()> {
    let (PatternKind::Int(first), PatternKind::Int(last)) = (&low.kind, &high.kind) else {
        unreachable!("the ends of a range are numbers once checked")
    };
    if first > last {
        return Err(Diag::error(span, "this range takes nothing: its first value is greater than its last")
            .hint(format!("the smaller value comes first: `{}..={}`", shown.1, shown.0)));
    }
    Ok(())
}

/// An end of a range as it is written: `9`, `'z'`.
fn written_end(end: &Pattern) -> String {
    match end.kind {
        PatternKind::Char(value) => crate::interp::quoted_char(value),
        PatternKind::Int(value) => value.to_string(),
        _ => unreachable!("the parser takes only a number or a character as an end"),
    }
}

/// An arm of a `match` on a text: a text as it is written, `_` or `var name`.
fn text_pattern(pattern: &mut Pattern, subject: &Ty) -> Res<Binds> {
    let span = pattern.span;
    match &mut pattern.kind {
        PatternKind::Wildcard | PatternKind::Str(_) => Ok(Vec::new()),
        PatternKind::Bind(name) => Ok(vec![(name.clone(), subject.clone(), span)]),
        PatternKind::Or(alternatives) => {
            let all = alternatives.iter_mut().map(|alternative| text_pattern(alternative, subject)).collect::<Res<Vec<_>>>()?;
            same_names(alternatives, all)
        }
        PatternKind::Range(..) => Err(Diag::error(span, "this `match` looks at a text, and a range is of numbers or characters")),
        PatternKind::Null => Err(Diag::error(span, "a `string` is never `null`").note("only a value of a type written with `?` can be `null`")),
        PatternKind::Bool(value) => Err(Diag::error(span, format!("this `match` looks at a text, and `{value}` is not one"))),
        PatternKind::Char(value) => Err(Diag::error(span, "this `match` looks at a text, and a character is not one")
            .hint(format!("a text is written in double quotes: `{:?}`", value.to_string()))),
        PatternKind::Int(_) => Err(Diag::error(span, "this `match` looks at a text, and a number is not one")),
        PatternKind::Variant { name, args, .. } => {
            let diag = Diag::error(span, format!("this `match` looks at a text, and `{name}` is not one"));
            Err(match args {
                None => diag.hint(format!("to take any value under this name, write `var {name}`")),
                Some(_) => diag,
            })
        }
    }
}

/// An arm of a `match` on a `char`: a character, `_` or `var name`. The character is kept as
/// its code.
fn char_pattern(pattern: &mut Pattern, subject: &Ty) -> Res<Binds> {
    let span = pattern.span;
    let value = match &mut pattern.kind {
        PatternKind::Wildcard => return Ok(Vec::new()),
        PatternKind::Bind(name) => return Ok(vec![(name.clone(), subject.clone(), span)]),
        PatternKind::Char(value) => *value as i128,
        PatternKind::Or(alternatives) => {
            let all = alternatives.iter_mut().map(|alternative| char_pattern(alternative, subject)).collect::<Res<Vec<_>>>()?;
            return same_names(alternatives, all);
        }
        PatternKind::Range(low, high) => {
            let shown = (written_end(low), written_end(high));
            char_pattern(low, subject)?;
            char_pattern(high, subject)?;
            return range_order(low, high, shown, span).map(|_| Vec::new());
        }
        PatternKind::Null => {
            return Err(Diag::error(span, "a `char` is never `null`").note("only a value of a type written with `?` can be `null`"));
        }
        PatternKind::Bool(value) => {
            return Err(Diag::error(span, format!("this `match` looks at a `char`, and `{value}` is not a character")));
        }
        PatternKind::Str(text) => {
            let diag = Diag::error(span, "this `match` looks at a `char`, and a text is not one");
            let mut chars = text.chars();
            return Err(match (chars.next(), chars.next()) {
                (Some(only), None) => diag.hint(format!("a character is written in single quotes: `{}`", crate::interp::quoted_char(only))),
                _ => diag,
            });
        }
        PatternKind::Int(value) => {
            let diag = Diag::error(span, "this `match` looks at a `char`, and a number is not one");
            return Err(match char_of(*value) {
                Some(found) if !found.is_control() => diag.hint(format!("a character is written in quotes: `'{found}'`")),
                _ => diag,
            });
        }
        PatternKind::Variant { name, args, .. } => {
            let diag = Diag::error(span, format!("this `match` looks at a `char`, and `{name}` is not a character"));
            return Err(match args {
                None => diag.hint(format!("to take any value under this name, write `var {name}`")),
                Some(_) => diag,
            });
        }
    };
    pattern.kind = PatternKind::Int(value);
    Ok(Vec::new())
}

/// Replaces `expr` with a node built around it.
/// `unsafe e`, once checked, is `e`: what `unsafe` allowed is decided.
/// `xs.getUnchecked(i)` of a list, an array or a slice, checked: made the item `xs[i]`, marked to
/// be read without checking the index (1803). A method of the program's own of that name stays one.
fn unchecked_index(expr: &mut Expr) {
    let ExprKind::Method { recv, name, callee: Callee::Plain, .. } = &expr.kind else { return };
    if name != "getUnchecked" {
        return;
    }
    let kind = match recv.ty.strip_borrow() {
        Ty::List(_) => IndexKind::List,
        Ty::Array(..) | Ty::Slice(_) => IndexKind::Array,
        _ => return,
    };
    let ExprKind::Method { recv, mut args, .. } = std::mem::replace(&mut expr.kind, ExprKind::Null) else { unreachable!("just matched") };
    expr.kind = ExprKind::Index { recv, index: Box::new(args.remove(0)), kind, unchecked: true };
}

fn unwrap_unsafe(expr: &mut Expr) {
    if let ExprKind::Unsafe(inner) = std::mem::replace(&mut expr.kind, ExprKind::Null) {
        expr.moved = inner.moved;
        expr.kind = inner.kind;
    }
}


/// Whether `name` is a type of the language: a module cannot have it (choice 1591), so that
/// `Hex.encode`, in the program and inside the language's own module, is always the type's.
pub fn is_language_type(name: &str) -> bool {
    const MORE: &[&str] = &["Channel", "Sender", "Atomic"];
    Ty::from_name(name).is_some() || BUILTIN_TYPES.contains(&name) || INTERFACES.contains(&name) || MORE.contains(&name) || crate::source::std_types().any(|ty| ty == name)
}

/// Whether `name` can name a library of C: as the C compiler links it, `-lz`, and nothing else.
pub fn is_library_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('-') && name.chars().all(|c| c.is_ascii_alphanumeric() || "_-.+".contains(c))
}

/// The name of a library is a name, so that nothing else reaches the C compiler; a block that
/// names it more than once names it once.
fn foreign_lib(foreign: &Foreign) -> Res<()> {
    for (at, lib) in foreign.libs.iter().enumerate() {
        if !is_library_name(lib) {
            return Err(Diag::error(foreign.span, format!("`{lib}` is not the name of a library"))
                .note("a library is named as the C compiler links it, `extern \"z\"` for `-lz`: letters, digits and `_ - . +`"));
        }
        if foreign.libs[..at].contains(lib) {
            return Err(Diag::error(foreign.span, format!("the library `{lib}` is named twice in this `extern` block"))
                .note("the names are those the library has on the systems, tried in the order they are written"));
        }
    }
    Ok(())
}

/// What a C function takes and gives: integers, fractional numbers and `bool` as C has them,
/// raw pointers, `ref T` and `read T` of plain data for the time of the call, and `read CString`
/// for a text. The name of its library is a name, so that nothing else reaches the C compiler.
fn foreign_sig(func: &Func, sig: &FuncSig, structs: &HashMap<String, StructInfo>) -> Res<()> {
    let foreign = func.foreign.as_ref().expect("a function of C");
    foreign_lib(foreign)?;
    let c_name = short_name(&func.name);
    let char_hint = |diag: Diag| diag.note("a `char` of SG is a Unicode character of 32 bits, and a `char` of C is a byte").hint("`byte`, or `sbyte` for a C `char` that is signed");
    for ((param, ty), written) in func.params.iter().zip(&sig.params).zip(&func.params) {
        let fine = match ty {
            Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Raw(_) | Ty::CFunc(..) => true,
            Ty::Read(inner) if **inner == Ty::CString => true,
            Ty::Read(inner) | Ty::Ref(inner) => not_plain(inner, structs).is_none() && !matches!(**inner, Ty::Array(..)),
            Ty::Struct(_) => not_plain(ty, structs).is_none(),
            _ => false,
        };
        if fine {
            continue;
        }
        let diag = Diag::error(written.ty.span, format!("the C function `{c_name}` cannot take a `{ty}`"));
        let diag = match ty.strip_borrow() {
            Ty::Char => char_hint(diag),
            Ty::Str => diag.hint("a text goes to C as `read CString`: `CString.from(text)` makes one"),
            Ty::CString => diag.hint("a `CString` is lent to C: `read CString`"),
            Ty::Array(..) | Ty::Slice(_) | Ty::List(_) => diag.hint(format!("C takes the address of the items and their count apart: `raw {} {}` and `nuint count`, given `buffer.pointer()`", match ty.strip_borrow() {
                Ty::Array(item, _) | Ty::Slice(item) | Ty::List(item) => item.to_string(),
                _ => unreachable!("matched above"),
            }, param.name)),
            Ty::Struct(_) => match not_plain(ty.strip_borrow(), structs) {
                Some(part) if part == *ty.strip_borrow() => diag.note(format!("`{part}` is not plain data: C gets numbers, `bool`, raw pointers, and arrays and structs of them")),
                Some(part) => diag.note(format!("it holds a `{part}`, which is not plain data: C gets numbers, `bool`, raw pointers, and arrays and structs of them")),
                None => diag,
            },
            _ => diag.note("C takes integers, `float`, `double`, `bool`, `raw T`, plain structs, `ref T` and `read T` of plain data, and `read CString`"),
        };
        return Err(diag);
    }
    let fine = matches!(sig.ret, Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Void | Ty::Raw(_) | Ty::CFunc(..)) || (matches!(sig.ret, Ty::Struct(_)) && not_plain(&sig.ret, structs).is_none());
    if !fine {
        let diag = Diag::error(func.ret.span, format!("the C function `{c_name}` cannot give a `{}`", sig.ret));
        return Err(match &sig.ret {
            Ty::Char => char_hint(diag),
            Ty::Str | Ty::CString => diag.hint("C gives a text as `raw byte`; `unsafe string.fromCString(p)` copies it"),
            Ty::Struct(_) => match not_plain(&sig.ret, structs) {
                Some(part) if part == sig.ret => diag.note(format!("`{part}` is not plain data: C gives numbers, `bool`, raw pointers, and arrays and structs of them")),
                Some(part) => diag.note(format!("it holds a `{part}`, which is not plain data: C gives numbers, `bool`, raw pointers, and arrays and structs of them")),
                None => diag,
            },
            _ => diag.note("C gives an integer, `float`, `double`, `bool`, `raw T`, a plain struct or nothing (`void`)"),
        });
    }
    Ok(())
}

/// Whether C takes and gives a value of this type as it is: an integer, a fractional number,
/// `bool`, a raw pointer, a pointer to a function of C, or a struct of plain data.
fn c_value(ty: &Ty, structs: &HashMap<String, StructInfo>) -> bool {
    match ty {
        Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Raw(_) | Ty::CFunc(..) => true,
        Ty::Struct(_) => not_plain(ty, structs).is_none(),
        _ => false,
    }
}

/// A type of SG that C holds only by its address: a struct that is not plain data and not
/// of C, or an enum. The header declares it without its fields (`typedef struct Calculator
/// Calculator;`).
fn handle_type(ty: &Ty, structs: &HashMap<String, StructInfo>) -> bool {
    match ty {
        Ty::Struct(name) => structs.get(name).is_some_and(|info| !info.opaque) && not_plain(ty, structs).is_some(),
        Ty::Enum(_) => true,
        _ => false,
    }
}

/// What an `export` function gives C to keep: `T*`, or `T*?`, which may be `null`.
fn handle_owned(ty: &Ty, structs: &HashMap<String, StructInfo>) -> bool {
    match ty {
        Ty::Ptr(inner) => handle_type(inner, structs),
        Ty::Nullable(inner) => matches!(&**inner, Ty::Ptr(target) if handle_type(target, structs)),
        _ => false,
    }
}

/// What an `export` function takes from C beyond the values of C: a value C keeps, given back
/// (`T*`, `T*?`), or lent for the call (`read T`, `ref T`).
fn handle_param(ty: &Ty, structs: &HashMap<String, StructInfo>) -> bool {
    match ty {
        // C's `const char*`, lent for the call, as a `read CString` is lent to a function of C.
        Ty::Read(inner) if **inner == Ty::CString => true,
        Ty::Read(inner) | Ty::Ref(inner) => handle_type(inner, structs),
        other => handle_owned(other, structs),
    }
}

/// The header of a library names each type of its `export` functions under the name it has in
/// SG, without its module, and each field of a plain struct under its own: the names must be
/// one each, apart from the functions, and not words of C.
fn header_names(prog: &Program, sigs: &HashMap<String, FuncSig>, structs: &HashMap<String, StructInfo>, exported: &HashMap<&str, &Func>) -> Res<()> {
    fn named(ty: &Ty, structs: &HashMap<String, StructInfo>, found: &mut Vec<String>) {
        match ty {
            Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::Read(inner) | Ty::Ref(inner) | Ty::Raw(inner) | Ty::Array(inner, _) | Ty::Atomic(inner) => named(inner, structs, found),
            Ty::CFunc(params, ret) => {
                for inner in params.iter().chain([&**ret]) {
                    named(inner, structs, found);
                }
            }
            Ty::Struct(name) | Ty::Enum(name) if !found.contains(name) => {
                found.push(name.clone());
                if let Some(info) = structs.get(name).filter(|info| !info.opaque && not_plain(ty, structs).is_none()) {
                    for (_, field) in &info.fields {
                        named(field, structs, found);
                    }
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    let mut funcs: Vec<&&Func> = exported.values().collect();
    funcs.sort_by_key(|func| func.name_span.start);
    for func in &funcs {
        let sig = &sigs[&func.name];
        for ty in sig.params.iter().chain([&sig.ret]) {
            named(ty, structs, &mut found);
        }
    }
    let span_of = |name: &str| {
        let structs = prog.structs.iter().map(|decl| (&decl.name, decl.name_span));
        structs.chain(prog.enums.iter().map(|decl| (&decl.name, decl.name_span))).find(|(full, _)| *full == name).map_or(Span::new(0, 0), |(_, span)| span)
    };
    let mut seen: HashMap<&str, &str> = HashMap::new();
    for full in &found {
        let short = short_name(full);
        let span = span_of(full);
        if C_WORDS.contains(&short) {
            return Err(Diag::error(span, format!("the header of the library names `{short}`, and `{short}` is a word of C")).hint("give the type another name"));
        }
        if let Some(other) = seen.insert(short, full) {
            return Err(Diag::error(span, format!("the header of the library would name two types `{short}`"))
                .related(span_of(other), "the other one")
                .note("C sees a type of an `export` function under its name in SG, without its module"));
        }
        if let Some(func) = exported.get(short) {
            return Err(Diag::error(span, format!("the header of the library would name both a type and a function `{short}`")).related(func.name_span, "the function"));
        }
        if let Some(info) = structs.get(full.as_str()).filter(|info| !info.opaque && not_plain(&Ty::Struct(full.clone()), structs).is_none()) {
            for (field, _) in &info.fields {
                if C_WORDS.contains(&field.as_str()) {
                    return Err(Diag::error(span, format!("the header of the library writes the field `{field}` of `{short}`, and `{field}` is a word of C"))
                        .hint("give the field another name"));
                }
            }
        }
    }
    Ok(())
}

/// What a message about a type C does not take or give adds.
fn c_value_error(diag: Diag, ty: &Ty, structs: &HashMap<String, StructInfo>) -> Diag {
    match ty {
        Ty::Read(inner) | Ty::Ref(inner) if matches!(**inner, Ty::Str | Ty::CString) => diag.hint("a text goes between C and SG as `raw byte`: `unsafe string.fromCString(p)` copies it"),
        Ty::Ptr(inner) | Ty::Nullable(inner) if matches!(**inner, Ty::Struct(_)) && not_plain(inner, structs).is_none() => {
            diag.note(format!("`{inner}` is plain data, which C holds itself")).hint(format!("give it by value, or as a `raw {inner}`"))
        }
        Ty::Read(inner) | Ty::Ref(inner) if not_plain(inner, structs).is_some() => diag.note("C gives a pointer as a `raw T`, which points at plain data"),
        Ty::Char => diag.note("a `char` of SG is a Unicode character of 32 bits, and a `char` of C is a byte").hint("`byte`, or `sbyte` for a C `char` that is signed"),
        Ty::Str | Ty::CString => diag.hint("a text goes between C and SG as `raw byte`: `unsafe string.fromCString(p)` copies it"),
        Ty::Read(inner) | Ty::Ref(inner) => diag.hint(format!("a pointer C gives is a `raw {inner}`, read inside `unsafe`")),
        Ty::Func(..) => diag.hint("a pointer to a function as C calls it is a `CFunc<...>`"),
        Ty::Struct(_) => match not_plain(ty, structs) {
            Some(part) if part == *ty => diag.note(format!("`{part}` is not plain data: C takes numbers, `bool`, raw pointers, and arrays and structs of them")),
            Some(part) => diag.note(format!("it holds a `{part}`, which is not plain data: C takes numbers, `bool`, raw pointers, and arrays and structs of them")),
            None => diag,
        },
        _ => diag.note("C takes and gives integers, `float`, `double`, `bool`, `raw T`, `CFunc<...>` and plain structs"),
    }
}

/// The part of a type that is not plain data, if there is one: what a `raw T` may point at
/// is numbers, `bool`, `char`, raw pointers, and arrays and structs of them.
fn not_plain(ty: &Ty, structs: &HashMap<String, StructInfo>) -> Option<Ty> {
    match ty {
        Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char | Ty::Raw(_) | Ty::CFunc(..) => None,
        Ty::Array(item, _) | Ty::Atomic(item) => not_plain(item, structs),
        Ty::Struct(name) if structs.get(name).is_some_and(|info| info.disposable || info.opaque) => Some(ty.clone()),
        Ty::Struct(name) => structs.get(name).and_then(|info| info.fields.iter().find_map(|(_, field)| not_plain(field, structs))),
        other => Some(other.clone()),
    }
}

/// The structs and unions of plain data, laid out as C lays them out.
fn plain_types(structs: &HashMap<String, StructInfo>) -> crate::raw::PlainTypes {
    let mut plain = crate::raw::PlainTypes::new();
    for (name, info) in structs {
        if not_plain(&Ty::Struct(name.clone()), structs).is_none() {
            let fields = info.fields.iter().map(|(_, ty)| ty.clone()).collect();
            plain.insert(name.clone(), crate::raw::Plain { fields, ..info.shape.clone() });
        }
    }
    plain
}

/// What a struct may say about how C lays it out: a union, a `packed` struct and a struct with
/// bit fields hold plain data; a bit field is an integer or a `bool` that fits its bits; a union
/// has no bit fields and is neither compared, hashed, ordered nor written out by its fields;
/// `align(N)` is a power of two that makes the alignment larger.
fn layout_rules(prog: &Program, structs: &HashMap<String, StructInfo>) -> Res<()> {
    for decl in prog.structs.iter().filter(|decl| !decl.opaque) {
        let info = &structs[&decl.name];
        let short = short_name(&decl.name);
        let bits_written = decl.fields.iter().find_map(|field| field.bits.map(|(_, span)| span)).or(decl.layout.gaps.first().map(|gap| gap.span));
        if decl.layout.union && decl.fields.is_empty() {
            return Err(Diag::error(decl.name_span, format!("union `{short}` has no fields")).note("a union holds one of its fields at a time, and C has no union of none"));
        }
        if let (true, Some(span)) = (decl.layout.union, bits_written) {
            return Err(Diag::error(span, "a union has no bit fields").note("a struct of bit fields can be a field of the union"));
        }
        if let (Some(_), Some(span)) = (decl.layout.packed, bits_written) {
            return Err(Diag::error(span, "a `packed` struct has no bit fields: the compilers of C lay the two out in different ways")
                .note("on Windows bit fields are packed by units of their type, elsewhere bit by bit"));
        }
        let named = match (decl.layout.union, decl.layout.packed.is_some(), bits_written.is_some()) {
            (true, _, _) => Some("a union"),
            (_, true, _) => Some("a `packed` struct"),
            (_, _, true) => Some("a struct with bit fields"),
            _ => None,
        };
        if let Some(named) = named {
            if let Some((_, span)) = decl.sources.first() {
                return Err(Diag::error(*span, format!("{named} holds plain data, and plain data borrows nothing")));
            }
            for (field, (_, ty)) in decl.fields.iter().zip(&info.fields) {
                if (decl.layout.union || decl.layout.packed.is_some()) && holds_atomic(ty, structs) {
                    let diag = Diag::error(field.ty.span, format!("{named} holds no atomic value"));
                    return Err(match decl.layout.union {
                        true => diag.note("a task that writes another field of the union writes over it, and no atomic operation sees that"),
                        false => diag.note("in a `packed` struct it may lie where an atomic operation cannot reach it in one step"),
                    });
                }
                if let Some(part) = not_plain(ty, structs) {
                    let diag = Diag::error(field.ty.span, format!("{named} holds plain data, and `{part}` is not"));
                    return Err(diag.note("plain data is numbers, `bool`, `char`, raw pointers, `CFunc`, and arrays, structs and unions of them: what C lays out"));
                }
            }
            for (interface, span) in &decl.interfaces {
                if matches!(interface.as_str(), "Disposable" | "Closable") {
                    return Err(Diag::error(*span, format!("{named} holds plain data, which has nothing to give back when it ends")));
                }
            }
        }
        if decl.layout.union {
            for field in &decl.fields {
                if field.readonly {
                    return Err(Diag::error(field.span, "a field of a union is not `readonly`: writing another field changes its bytes"));
                }
            }
            for (interface, span) in &decl.interfaces {
                let why = match interface.as_str() {
                    "Equatable" | "Hashable" | "Comparable" => "a union does not know which of its fields it holds, so it is not compared by them",
                    "Formattable" if !decl.methods.iter().any(|method| method.name == "format") => "a union does not know which of its fields it holds, so its text is not made from them",
                    _ => continue,
                };
                let diag = Diag::error(*span, format!("union `{short}` cannot be `{interface}`")).note(why);
                return Err(match interface.as_str() {
                    "Formattable" => diag.hint("write its own `void format(read this, ref Formatter f)`, which reads the field it holds inside `unsafe`"),
                    _ => diag.hint("read the field it holds inside `unsafe` and compare that"),
                });
            }
        }
        // Each bit field, and each run of bits left empty.
        let fields = decl.fields.iter().zip(&info.fields).filter_map(|(field, (_, ty))| field.bits.map(|(width, span)| (true, ty.clone(), width, span)));
        let gaps = decl.layout.gaps.iter().zip(&info.shape.gaps).map(|(gap, (_, ty, _))| (false, ty.clone(), gap.bits, gap.span));
        for (named, ty, width, span) in fields.chain(gaps) {
            let most = match &ty {
                // Its unit is 32 bits on Windows and 64 elsewhere, so the bits would lie apart.
                Ty::Int(int) if int.is_c() => {
                    return Err(Diag::error(span, format!("a bit field of `{}` is laid out by units of 32 bits on Windows and of 64 elsewhere", int.name()))
                        .hint(format!("a bit field of `{}` or `{}`", if int.signed() { "int" } else { "uint" }, c_partner(*int).name())));
                }
                Ty::Int(int) => int.bits() as u64,
                Ty::Bool => 1,
                other => return Err(Diag::error(span, format!("a bit field is an integer or a `bool`, and this is `{other}`"))),
            };
            if width > most {
                let diag = Diag::error(span, format!("a `{ty}` has {most} bit(s), and this asks for {width}"));
                return Err(match ty {
                    Ty::Bool => diag.note("a `bool` bit field takes 1 bit"),
                    _ => diag,
                });
            }
            if width == 0 && named {
                return Err(Diag::error(span, "a bit field has at least 1 bit").note("`uint _ : 0`, without a name, starts the next field at a new unit of its type"));
            }
        }
        if let Some((align, span)) = decl.layout.align {
            if !align.is_power_of_two() || align > 4096 {
                return Err(Diag::error(span, format!("an alignment is a power of two up to 4096, and this is {align}")));
            }
        }
    }
    // The alignment `align(N)` asks for is not below the one the fields ask for, and a `packed`
    // struct holds no type with an alignment of its own: Windows keeps it, the others do not.
    let plain = plain_types(structs);
    for decl in prog.structs.iter().filter(|decl| plain.contains_key(&decl.name)) {
        if let Some((align, span)) = decl.layout.align {
            let mut without = plain.clone();
            without.get_mut(&decl.name).expect("a struct of plain data").align = 0;
            let natural = crate::raw::layout(&Ty::Struct(decl.name.clone()), &without).1;
            if (align as usize) < natural {
                return Err(Diag::error(span, format!("`align({align})` asks for less than the {natural} bytes its fields ask for"))
                    .note("`align(N)` makes the alignment larger; `packed` makes it 1"));
            }
        }
        if decl.layout.packed.is_some() {
            for (field, ty) in decl.fields.iter().zip(&plain[&decl.name].fields) {
                if let Some(inner) = holds_aligned(ty, &plain) {
                    return Err(Diag::error(field.ty.span, format!("a `packed` struct holds no `{inner}`, which is `align(N)`"))
                        .note("Windows keeps that alignment inside a packed struct, the other systems do not"));
                }
            }
        }
    }
    Ok(())
}

/// The first part of a type of plain data that not every pattern of bits is a value of: a
/// `bool` (but a bit field of one), a `char`, a raw pointer or a `CFunc`.
fn not_any_bits(ty: &Ty, structs: &HashMap<String, StructInfo>) -> Option<Ty> {
    match ty {
        Ty::Int(_) | Ty::Float(_) => None,
        Ty::Array(item, _) => not_any_bits(item, structs),
        Ty::Struct(name) => {
            let info = &structs[name];
            info.fields.iter().zip(&info.shape.bits).find_map(|((_, field), bits)| match (field, bits) {
                (Ty::Bool, Some(_)) => None,
                _ => not_any_bits(field, structs),
            })
        }
        other => Some(other.clone()),
    }
}

/// The first type in `ty` written with `align(N)`.
fn holds_aligned(ty: &Ty, plain: &crate::raw::PlainTypes) -> Option<String> {
    match ty {
        Ty::Array(item, _) => holds_aligned(item, plain),
        Ty::Struct(name) if plain[name].align > 0 => Some(short_name(name).to_string()),
        Ty::Struct(name) => plain[name].fields.iter().find_map(|field| holds_aligned(field, plain)),
        _ => None,
    }
}

/// A number written in the program that does not fit a bit field of `width` bits is an error.
fn fits_bits(value: &Expr, ty: &Ty, width: u32) -> Res<()> {
    let number = match &value.kind {
        ExprKind::Int(number, _) => *number,
        ExprKind::Unary(UnOp::Neg, inner) => match inner.kind {
            ExprKind::Int(number, _) => -number,
            _ => return Ok(()),
        },
        _ => return Ok(()),
    };
    let Ty::Int(int) = ty else { return Ok(()) };
    let (low, high) = if int.signed() { (-(1i128 << (width - 1)), (1i128 << (width - 1)) - 1) } else { (0, (1i128 << width) - 1) };
    if number < low || number > high {
        return Err(Diag::error(value.span, format!("{number} does not fit in a bit field of {width} bit(s), which holds {low} to {high}")));
    }
    Ok(())
}

/// The place an assignment writes and each place it is reached through, the place first.
fn written_path(target: &Expr) -> Vec<Span> {
    let mut path = vec![target.span];
    let mut at = target;
    while let ExprKind::Field { recv, .. } | ExprKind::Index { recv, .. } = &at.kind {
        path.push(recv.span);
        at = recv;
    }
    path
}

/// The struct a field is reached in, behind pointers and borrows.
fn struct_of(ty: &Ty) -> Option<&str> {
    match ty {
        Ty::Read(inner) | Ty::Ref(inner) | Ty::Ptr(inner) | Ty::Shared(inner, _) => struct_of(inner),
        Ty::Struct(name) => Some(name),
        _ => None,
    }
}

/// Whether a place is reached through a field of a union: the field (its whole place), and
/// whether the place is that field itself.
fn union_field(place: &Expr, structs: &HashMap<String, StructInfo>) -> Option<(Span, bool)> {
    let mut at = place;
    let mut found = None;
    loop {
        match &at.kind {
            ExprKind::Field { recv, .. } => {
                if struct_of(&recv.ty).is_some_and(|name| structs.get(name).is_some_and(|info| info.shape.union)) {
                    found = Some((at.span, std::ptr::eq(at, place)));
                }
                at = recv;
            }
            ExprKind::Index { recv, .. } => at = recv,
            _ => return found,
        }
    }
}

/// Whether an atomic value lies in the type, in its arrays and in the structs it holds.
fn holds_atomic(ty: &Ty, structs: &HashMap<String, StructInfo>) -> bool {
    match ty {
        Ty::Atomic(_) => true,
        Ty::Array(item, _) => holds_atomic(item, structs),
        Ty::Struct(name) => structs.get(name).is_some_and(|info| info.fields.iter().any(|(_, field)| holds_atomic(field, structs))),
        _ => false,
    }
}

/// The struct that holds a bit field, when the place is one: what is read whole instead.
fn bit_field_at(place: &Expr, structs: &HashMap<String, StructInfo>) -> Option<Span> {
    let ExprKind::Field { recv, index, .. } = &place.kind else { return None };
    let info = struct_of(&recv.ty).and_then(|name| structs.get(name))?;
    info.shape.bits.get(*index).copied().flatten().map(|_| recv.span)
}

/// Why a place is not lent: a field of a union (which field it holds is not known), a bit
/// field (it has no address), or a field of a `packed` struct whose type needs an alignment
/// the field may not have. `None` when it is lent.
fn not_lent(place: &Expr, structs: &HashMap<String, StructInfo>, src: &str) -> Option<Diag> {
    let text = &src[place.span.start..place.span.end];
    if let Some((field, _)) = union_field(place, structs) {
        let field = &src[field.start..field.end];
        return Some(Diag::error(place.span, "a field of a union is not lent: the checker does not know that the union holds it")
            .hint(format!("copy it out: `var part = unsafe {field}`")));
    }
    let mut at = place;
    let mut plain = None;
    loop {
        match &at.kind {
            ExprKind::Field { recv, index, .. } => {
                if let Some(info) = struct_of(&recv.ty).and_then(|name| structs.get(name)) {
                    if std::ptr::eq(at, place) && info.shape.bits.get(*index).copied().flatten().is_some() {
                        return Some(Diag::error(place.span, "a bit field is not lent: it has no address of its own")
                            .hint(format!("copy it out: `var part = {text}`")));
                    }
                    if info.shape.packed {
                        let plain = plain.get_or_insert_with(|| plain_types(structs));
                        let (_, align) = crate::raw::layout(&info.fields[*index].1, plain);
                        if align > 1 {
                            let field = &src[at.span.start..at.span.end];
                            return Some(Diag::error(place.span, format!("a field of a `packed` struct is not lent: `{field}` may lie where a `{}` cannot be reached", info.fields[*index].1))
                                .hint(format!("copy it out: `var part = {field}`, and write it back whole")));
                        }
                    }
                }
                at = recv;
            }
            ExprKind::Index { recv, .. } => at = recv,
            _ => return None,
        }
    }
}

/// Every `raw T` resolved since the last call points at plain data or at `void`.
fn raw_rule(structs: &HashMap<String, StructInfo>) -> Res<()> {
    for (found, span) in CFUNCS.with(|found| std::mem::take(&mut *found.borrow_mut())) {
        let Ty::CFunc(params, ret) = &found else { unreachable!("only `CFunc` types are kept") };
        for ty in params.iter().chain(std::iter::once(&**ret)) {
            if (ty == &**ret && *ty == Ty::Void) || c_value(ty, structs) {
                continue;
            }
            return Err(c_value_error(Diag::error(span, format!("a function of C cannot take or give a `{ty}`")), ty, structs));
        }
    }
    for (inner, span) in RAW.with(|found| std::mem::take(&mut *found.borrow_mut())) {
        if inner == Ty::Void || matches!(&inner, Ty::Struct(name) if structs[name].opaque) {
            continue;
        }
        if let Some(part) = not_plain(&inner, structs) {
            let diag = Diag::error(span, format!("a raw pointer points at plain data, and `{inner}` is not"));
            let diag = match part == inner {
                true => diag,
                false => diag.note(format!("it holds a `{part}`")),
            };
            if matches!(&part, Ty::Struct(name) if structs[name].disposable) {
                return Err(diag.note("a `Disposable` value is disposed of when it ends, and nothing in raw memory ends"));
            }
            return Err(diag.note("plain data is numbers, `bool`, `char`, raw pointers, and arrays and structs of them: what owns nothing and can be copied byte by byte"));
        }
    }
    Ok(())
}

fn wrap(expr: &mut Expr, build: impl FnOnce(Box<Expr>) -> ExprKind) {
    let span = expr.span;
    let inner = std::mem::replace(expr, Expr { kind: ExprKind::Null, span, ty: Ty::Void, moved: false });
    expr.kind = build(Box::new(inner));
}

/// Whether control never reaches the end of the block: it ends in `return` or `panic` on
/// every path, or, with `in_loop`, in `break` or `continue`.
fn leaves(block: &Block, in_loop: bool) -> bool {
    let Some(last) = block.last() else { return false };
    match &last.kind {
        StmtKind::Return(_) => true,
        StmtKind::Break(_) | StmtKind::Continue(_) => in_loop,
        StmtKind::If { then, els: Some(els), .. } => leaves(then, in_loop) && leaves(els, in_loop),
        StmtKind::As { body, .. } | StmtKind::Lock { body, .. } | StmtKind::Unsafe(body) => leaves(body, in_loop),
        StmtKind::While { cond, body, label, .. } => matches!(cond.kind, ExprKind::Bool(true)) && first_break(body, label.as_ref()).is_none(),
        StmtKind::Expr(expr) => never_returns(expr, in_loop),
        _ => false,
    }
}

/// Whether evaluating the expression always leaves: `panic(...)`, or a `match` whose arms
/// all leave.
fn never_returns(expr: &Expr, in_loop: bool) -> bool {
    match &expr.kind {
        ExprKind::Call { name, callee: Callee::Plain, .. } => name == "panic" || name == "exit",
        ExprKind::Match { arms, .. } => arms.iter().all(|arm| arm_leaves(arm, in_loop)),
        _ => false,
    }
}

fn arm_leaves(arm: &Arm, in_loop: bool) -> bool {
    match &arm.value {
        Some(value) => never_returns(value, in_loop),
        None => leaves(&arm.body, in_loop),
    }
}

/// The first `break` that leaves this loop, also from inside a `match` anywhere in an
/// expression (`1 + match k { 1 => { break } _ => 2 }`): a `break` of no name outside the
/// loops inside it, and one that names this loop (`label`) anywhere.
fn first_break(block: &Block, label: Option<&Label>) -> Option<Span> {
    breaks_of(block, label.map(|label| label.name.as_str()), false)
}

/// `nested`: the block is inside a loop within the one looked at.
fn breaks_of(block: &Block, label: Option<&str>, nested: bool) -> Option<Span> {
    let inner = |body: &Block| label.and_then(|_| breaks_of(body, label, true));
    block.iter().find_map(|stmt| match &stmt.kind {
        StmtKind::Break(None) => (!nested).then_some(stmt.span),
        StmtKind::Break(Some(named)) => (label == Some(named.name.as_str())).then_some(stmt.span),
        StmtKind::If { cond, then, els, .. } => breaks_in(cond, label, nested).or_else(|| breaks_of(then, label, nested)).or_else(|| els.as_ref().and_then(|els| breaks_of(els, label, nested))),
        // The condition, the range and the collection are reached outside the inner loop.
        StmtKind::While { cond: expr, body, .. } | StmtKind::ForEach { iter: expr, body, .. } => breaks_in(expr, label, nested).or_else(|| inner(body)),
        StmtKind::For { from, to, body, .. } => breaks_in(from, label, nested).or_else(|| breaks_in(to, label, nested)).or_else(|| inner(body)),
        // The block of `as` is not a loop: a `break` in it leaves the loop around it.
        StmtKind::As { value, body, .. } => breaks_in(value, label, nested).or_else(|| breaks_of(body, label, nested)),
        StmtKind::Lock { items, when, body, .. } => items.iter().find_map(|item| breaks_in(&item.value, label, nested)).or_else(|| when.as_ref().and_then(|when| breaks_in(when, label, nested))).or_else(|| breaks_of(body, label, nested)),
        StmtKind::Unsafe(body) => breaks_of(body, label, nested),
        StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) | StmtKind::Discard(expr) | StmtKind::Let { init: expr, .. } => breaks_in(expr, label, nested),
        StmtKind::Assign { target, value, .. } => breaks_in(target, label, nested).or_else(|| breaks_in(value, label, nested)),
        StmtKind::Return(None) | StmtKind::Continue(_) | StmtKind::Const { .. } | StmtKind::Static { .. } => None,
    })
}

/// The first `break` in an arm of a `match` in this expression, as `breaks_of` finds them.
fn breaks_in(expr: &Expr, label: Option<&str>, nested: bool) -> Option<Span> {
    parts(expr).into_iter().find_map(|part| match part {
        Part::Expr(inner) => breaks_in(inner, label, nested),
        Part::Block(block) => breaks_of(block, label, nested),
    })
}
