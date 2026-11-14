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
use crate::types::{FloatTy, IntTy, TextIter, Ty};

type Res<T> = Result<T, Diag>;

/// The types of what goes over a text: they have names in messages, not yet in a program.
const TEXT_ITERATORS: &[&str] = &["Chars", "Split", "SplitWhitespace", "Lines"];

const INTERFACES_NOT_YET: &[&str] = &["Clonable", "Disposable", "Closable", "Iterator"];

const INTERFACES: &[&str] = &["Copyable", "Formattable", "Equatable", "Hashable", "Comparable"];

const BUILTINS: &[&str] = &["print", "printError", "assert", "panic", "exit", "Ok", "Error"];

/// Types the language brings along; a program cannot define another one under these names.
const BUILTIN_TYPES: &[&str] = &["Result", "List", "HashMap", "HashSet", "Func", "TextError", "NumberError", "IoError", "File", "Stdin", "Stdout", "Stderr"];

struct StructInfo {
    fields: Vec<(String, Ty)>,
    /// For each field: whether other modules see it (`public`).
    open: Vec<bool>,
    /// For each field: whether it is `readonly`.
    readonly: Vec<bool>,
    /// Functions written in the struct body, with or without `this`.
    methods: HashMap<String, FuncSig>,
}

struct EnumInfo {
    /// Variants in declaration order, each with the fields it carries.
    variants: Vec<(String, Vec<(String, Ty)>)>,
    /// Functions written in the enum body, with or without `this`.
    methods: HashMap<String, FuncSig>,
}

#[derive(Clone, Copy, PartialEq)]
enum TypeKind {
    Struct,
    Enum,
}

/// The types of the program, known before anything else is resolved.
#[derive(Default)]
struct Names {
    kinds: HashMap<String, TypeKind>,
    /// Types written with `: Hashable`: they can be the key of a `HashMap`.
    keys: HashSet<String>,
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
        self.kinds.contains_key(full) || self.funcs.contains(full)
    }

    /// The full name of the type or function that `written` means in the file at `at`: a name
    /// of the file's own module first, then a `public` name of a module the file names with
    /// `use`. With a module in front (`billing.Line`) no `use` is needed.
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
        if written == TEXT_ERROR || written == NUMBER_ERROR || written == IO_ERROR {
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
        let uses = self.file(at).map_or(&[][..], |file| file.uses.as_slice());
        let mut modules: Vec<&String> = self.modules.iter().collect();
        modules.sort();
        let module = modules.into_iter().find(|module| {
            let full = format!("{module}.{written}");
            self.known(&full) && self.public.contains(&full) && !uses.iter().any(|(used, _)| used == *module)
        })?;
        Some(format!("`use {module}` at the top of the file"))
    }
}

/// The first type in `ty` that other modules cannot see.
fn closed_in<'t>(ty: &'t Ty, names: &Names) -> Option<&'t str> {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => (!names.public.contains(name)).then_some(name.as_str()),
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) | Ty::Read(inner) | Ty::Ref(inner) => closed_in(inner, names),
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

/// The integer type a `tryTo...` or `wrappingTo...` method converts to: `tryToInt` gives an
/// `int?`. A conversion that cannot fail is an error, as is cutting a fractional number.
fn conversion(from: &Ty, name: &str, span: Span) -> Res<IntTy> {
    let (wrapping, rest) = match name.strip_prefix("tryTo") {
        Some(rest) => (false, rest),
        None => (true, name.strip_prefix("wrappingTo").expect("tested by the caller")),
    };
    let lower = rest.to_lowercase();
    let no_method = || Diag::error(span, format!("`{from}` has no method `{name}`"));
    let Some(to) = IntTy::from_name(&lower) else {
        if let Some(float) = FloatTy::from_name(&lower) {
            return Err(no_method().hint(format!("`({})x` never fails; it rounds", float.name())));
        }
        return Err(no_method().note("the conversions are named after the type: `tryToInt`, `tryToUlong`, `wrappingToByte`"));
    };
    let spelled = format!("{}{}", lower[..1].to_uppercase(), &lower[1..]);
    if rest != spelled {
        let right = if wrapping { "wrappingTo" } else { "tryTo" };
        return Err(no_method().hint(format!("`{right}{spelled}`")));
    }
    match from {
        Ty::Float(_) if wrapping => Err(no_method().note(format!(
            "a fractional number has no bits to cut to size; `tryTo{spelled}()` gives `null` when it does not fit"
        ))),
        Ty::Int(from) if *from == to => Err(Diag::error(span, format!("this is an `{}` already: there is nothing to convert", to.name()))),
        Ty::Int(from) if from.widens_to(to) => Err(Diag::error(span, format!("every `{}` fits in `{}`: there is nothing to convert", from.name(), to.name()))
            .hint(format!("it becomes one on its own: `{} x = value`", to.name()))),
        _ => Ok(to),
    }
}

/// The methods of `char` and `byte` that test a character against ASCII.
const ASCII_TESTS: &[&str] = &["isAscii", "isAsciiDigit", "isAsciiLetter", "isAsciiAlphanumeric", "isAsciiUpper", "isAsciiLower", "isAsciiWhitespace"];

/// The methods of `char` and of `string`, for a name that is not one of them.
const CHAR_METHODS: &[&str] = &[
    "isAscii", "isAsciiDigit", "isAsciiLetter", "isAsciiAlphanumeric", "isAsciiUpper", "isAsciiLower", "isAsciiWhitespace",
    "toAsciiUpper", "toAsciiLower", "isWhitespace", "utf8Length", "toString", "equals", "compare",
];
const TEXT_METHODS: &[&str] = &[
    "length", "clone", "asBytes", "write", "clear", "indexOf", "lastIndexOf", "contains", "startsWith", "endsWith", "trim", "trimStart", "trimEnd",
    "chars", "split", "splitWhitespace", "lines", "equalsIgnoreAsciiCase", "equals", "compare",
];

/// A method a `char` or a `string` does not have: the one it may have meant.
/// What a `StringBuilder` of C# is here.
const STRING_BUILDER: &str = "a `string` is built where it is: `var text = \"\"`, then `text.write(...)`";

fn text_method_hint(diag: Diag, target: &Ty, name: &str) -> Diag {
    let text = *target == Ty::Str;
    let lower = name.to_lowercase();
    match lower.as_str() {
        "toupper" | "tolower" | "touppercase" | "tolowercase" | "to_uppercase" | "to_lowercase" => {
            let ascii = if lower.contains("upper") { "toAsciiUpper" } else { "toAsciiLower" };
            return match text {
                true => diag.note(format!("changing the case of a text is not here yet; a `char` and a `byte` have `{ascii}`")),
                false => diag.hint(format!("`{ascii}()` changes the ASCII letters; the other characters need Unicode tables, which are not here yet")),
            };
        }
        "isdigit" | "isletter" | "isalphabetic" | "isnumeric" | "isupper" | "islower" | "isletterordigit" | "isalphanumeric" => {
            let ascii = match lower.as_str() {
                "isdigit" | "isnumeric" => "isAsciiDigit",
                "isletter" | "isalphabetic" => "isAsciiLetter",
                "isupper" => "isAsciiUpper",
                "islower" => "isAsciiLower",
                _ => "isAsciiAlphanumeric",
            };
            return diag.hint(format!("`{ascii}()`")).note("the tests that need Unicode tables are not here yet");
        }
        "substring" | "substr" | "slice" if text => return diag.hint("a range of the text: `text[a..b]`, `text[a..]`"),
        "equalsignorecase" if text => return diag.hint("`equalsIgnoreAsciiCase`: it treats the ASCII letters alone as the same in either case"),
        "charat" | "at" if text => return diag.hint("the characters, one after the other: `for c in text.chars()`; one byte: `text[i]`"),
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
    let ty = TypeExpr { mode: None, name: "nint".to_string(), args: Vec::new(), suffixes: Vec::new(), span: nowhere };
    let position = FieldDecl { public: true, readonly: true, ty, name: "position".to_string(), span: nowhere };
    StructDecl { public: true, name: TEXT_ERROR.to_string(), name_span: nowhere, sources: Vec::new(), interfaces, fields: vec![position], methods: Vec::new() }
}

/// What `int.parse` and the other `parse` functions give when the text is not a number.
const NUMBER_ERROR: &str = "NumberError";

/// The built-in `enum NumberError : Copyable, Formattable, Equatable { Invalid(nint position),
/// OutOfRange }`: the text is not a number from the byte at `position` on (the length of the
/// text when it ends too early), or it is one that the type does not hold.
fn number_error() -> EnumDecl {
    let nowhere = Span::new(0, 0);
    let interfaces = ["Copyable", "Formattable", "Equatable"].iter().map(|name| (name.to_string(), nowhere)).collect();
    let ty = TypeExpr { mode: None, name: "nint".to_string(), args: Vec::new(), suffixes: Vec::new(), span: nowhere };
    let position = FieldDecl { public: true, readonly: true, ty, name: "position".to_string(), span: nowhere };
    let variants = vec![
        VariantDecl { name: "Invalid".to_string(), span: nowhere, fields: vec![position] },
        VariantDecl { name: "OutOfRange".to_string(), span: nowhere, fields: Vec::new() },
    ];
    EnumDecl { public: true, name: NUMBER_ERROR.to_string(), name_span: nowhere, sources: Vec::new(), interfaces, variants, methods: Vec::new() }
}

/// What reading or writing a file, or the input of the program, gives when it fails.
const IO_ERROR: &str = "IoError";

/// The built-in `enum IoError : Copyable, Formattable, Equatable { NotFound, PermissionDenied,
/// IsDirectory, NoSpace, InvalidText(long position), Other(int code) }`. The first four are
/// what a program most often tells apart; `InvalidText` is a byte that is not UTF-8 where text
/// was read, counted from the start of the file or of the input; `Other` keeps the code the
/// system gave.
fn io_error() -> EnumDecl {
    let nowhere = Span::new(0, 0);
    let interfaces = ["Copyable", "Formattable", "Equatable"].iter().map(|name| (name.to_string(), nowhere)).collect();
    let field = |ty: &str, name: &str| {
        let ty = TypeExpr { mode: None, name: ty.to_string(), args: Vec::new(), suffixes: Vec::new(), span: nowhere };
        FieldDecl { public: true, readonly: true, ty, name: name.to_string(), span: nowhere }
    };
    let variant = |name: &str, fields: Vec<FieldDecl>| VariantDecl { name: name.to_string(), span: nowhere, fields };
    let variants = vec![
        variant("NotFound", Vec::new()),
        variant("PermissionDenied", Vec::new()),
        variant("IsDirectory", Vec::new()),
        variant("NoSpace", Vec::new()),
        variant("InvalidText", vec![field("long", "position")]),
        variant("Other", vec![field("int", "code")]),
    ];
    EnumDecl { public: true, name: IO_ERROR.to_string(), name_span: nowhere, sources: Vec::new(), interfaces, variants, methods: Vec::new() }
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
    this: Option<This>,
    params: Vec<Ty>,
    param_names: Vec<String>,
    ret: Ty,
    /// For a function that returns a borrow, or a value that carries one: where it may come
    /// from. Empty when it can only be a constant.
    sources: Vec<Source>,
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
        Ty::Read(_) | Ty::Ref(_) | Ty::TextIter(_) => true,
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
fn func_sig(func: &Func, names: &Names, owner: Option<&str>) -> Res<FuncSig> {
    let mut params = Vec::new();
    for param in &func.params {
        params.push(resolve_type(&param.ty, names, false)?);
    }
    let ret = resolve_type(&func.ret, names, true)?;
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
    Ok(FuncSig { public: func.public, this: func.this, params, param_names, ret, sources })
}

/// With `tests` the `test` blocks are checked too and `main` may be missing: that is what the
/// test command compiles. A normal run leaves the tests out.
pub fn check(prog: &mut Program, src: &str, tests: bool) -> Res<()> {
    let mut names = Names { files: prog.files.clone(), ..Names::default() };
    names.modules = prog.files.iter().filter(|file| !file.module.is_empty()).map(|file| file.module.clone()).collect();
    for file in &prog.files {
        for (module, span) in &file.uses {
            if names.modules.contains(module) {
                continue;
            }
            let diag = Diag::error(*span, format!("there is no module named `{module}`"));
            return Err(match nearest(module, names.modules.iter().map(String::as_str)) {
                Some(close) => diag.hint(format!("`use {close}`")),
                None => diag.note(format!("a module is a folder with source files, next to the files of the program: `{}/`", module.replace('.', "/"))),
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
    let types = prog
        .structs
        .iter()
        .map(|decl| (&decl.name, decl.name_span, decl.public))
        .chain(prog.enums.iter().map(|decl| (&decl.name, decl.name_span, decl.public)));
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
        .chain(prog.enums.iter().map(|decl| (&decl.name, decl.name_span, TypeKind::Enum)));
    for (name, span, kind) in declared {
        if BUILTIN_TYPES.contains(&short_name(name)) {
            return Err(Diag::error(span, format!("`{}` is a built-in type and cannot be redefined", short_name(name))));
        }
        if Ty::from_name(short_name(name)).is_some() || names.kinds.insert(name.clone(), kind).is_some() {
            return Err(Diag::error(span, format!("type `{name}` is defined twice")));
        }
    }
    names.kinds.insert(TEXT_ERROR.to_string(), TypeKind::Struct);
    names.public.insert(TEXT_ERROR.to_string());
    prog.structs.push(text_error());
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
                "Comparable" => comparable.insert(name.clone()),
                _ if INTERFACES_NOT_YET.contains(&interface.as_str()) => {
                    return Err(Diag::error(*span, format!("`{interface}` is not in the interpreter yet"))
                        .note("interfaces for now: `Copyable`, `Formattable`, `Equatable`, `Hashable` and `Comparable`"));
                }
                _ => {
                    let diag = Diag::error(*span, format!("there is no interface named `{interface}`"));
                    return Err(match nearest(interface, INTERFACES.iter().copied()) {
                        Some(close) => diag.hint(format!("`{close}`")),
                        None => diag.note("interfaces for now: `Copyable`, `Formattable`, `Equatable`, `Hashable` and `Comparable`; your own are not in the interpreter yet"),
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
            fields.push((field.name.clone(), resolve_type(&field.ty, &names, false)?));
        }
        let taken = fields.iter().map(|(name, _)| name.as_str());
        let methods = type_methods(&decl.name, &decl.methods, taken, "a field", &names, &formattable)?;
        let open = decl.fields.iter().map(|field| field.public).collect();
        let readonly = decl.fields.iter().map(|field| field.readonly).collect();
        structs.insert(decl.name.clone(), StructInfo { fields, open, readonly, methods });
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
                fields.push((field.name.clone(), resolve_type(&field.ty, &names, false)?));
            }
            variants.push((variant.name.clone(), fields));
        }
        if variants.is_empty() {
            return Err(Diag::error(decl.name_span, format!("enum `{}` has no variants, so it can never have a value", decl.name)));
        }
        let taken = variants.iter().map(|(name, _)| name.as_str());
        let methods = type_methods(&decl.name, &decl.methods, taken, "a variant", &names, &formattable)?;
        enums.insert(decl.name.clone(), EnumInfo { variants, methods });
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
    let sets = Promised { copyable: &copyable, formattable: &formattable, equatable: &equatable, keys: &names.keys, comparable: &comparable };
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

    let root_end = prog.files.first().map_or(src.len(), |file| file.end);
    match prog.funcs.iter().find(|func| func.name == "main") {
        None if tests => {}
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
                    .note("to see the command line: `void main(List<string> args)`"));
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

    let body = |func: &mut Func, sig: &FuncSig, owner: Option<&str>| -> Res<()> {
        let mut checker = Checker {
            names: &names,
            copyable: &copyable,
            formattable: &formattable,
            equatable: &equatable,
            comparable: &comparable,
            structs: &structs,
            enums: &enums,
            sigs: &sigs,
            scopes: vec![Scope::default()],
            locks: Vec::new(),
            borrows: Borrows::default(),
            loops: 0,
            loop_depths: Vec::new(),
            ret: sig.ret.clone(),
            sources: sig.sources.clone(),
            func_name: func.name.clone(),
            in_test: false,
            is_names: Vec::new(),
            owner,
            module: names.module(func.name_span),
            src,
        };
        if let (Some(this), Some(owner)) = (func.this, owner) {
            let ty = match names.kinds[owner] {
                TypeKind::Struct => Ty::Struct(owner.to_string()),
                TypeKind::Enum => Ty::Enum(owner.to_string()),
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
            if let Some(what) = std_stream(&param.name) {
                return Err(Diag::error(param.span, format!("`{}` is {what} of the program, so a parameter cannot have its name", param.name)));
            }
            checker.declare(&param.name, ty.clone(), param.span)?;
            checker.find(&param.name).expect("just declared").param = true;
        }
        checker.declare_streams(func.name_span)?;
        checker.block(&mut func.body, &[])?;
        // A writer given to the function by value is closed before its end.
        if !leaves(&func.body, false) {
            checker.unclosed(0, None)?;
        }
        checker.borrow_conflicts()?;
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
        Ok(())
    };
    for func in &mut prog.funcs {
        body(func, &sigs[&func.name], None)?;
    }
    for decl in &mut prog.structs {
        let owner = decl.name.as_str();
        for method in &mut decl.methods {
            body(method, &structs[owner].methods[&method.name], Some(owner))?;
        }
    }
    for decl in &mut prog.enums {
        let owner = decl.name.as_str();
        for method in &mut decl.methods {
            body(method, &enums[owner].methods[&method.name], Some(owner))?;
        }
    }
    if !tests {
        return Ok(());
    }
    for position in 0..prog.tests.len() {
        let test = &prog.tests[position];
        let test_span = test.name_span;
        if let Some(first) = prog.tests[..position].iter().find(|other| other.name == test.name && names.module(other.name_span) == names.module(test_span)) {
            return Err(Diag::error(test.name_span, format!("there are two tests named \"{}\"", test.name))
                .related(first.name_span, "the other one is here"));
        }
        let mut checker = Checker {
            names: &names,
            copyable: &copyable,
            formattable: &formattable,
            equatable: &equatable,
            comparable: &comparable,
            structs: &structs,
            enums: &enums,
            sigs: &sigs,
            scopes: vec![Scope::default()],
            locks: Vec::new(),
            borrows: Borrows::default(),
            loops: 0,
            loop_depths: Vec::new(),
            ret: Ty::Void,
            sources: Vec::new(),
            func_name: "test".to_string(),
            in_test: true,
            is_names: Vec::new(),
            owner: None,
            module: names.module(test_span),
            src,
        };
        checker.declare_streams(test_span)?;
        checker.block(&mut prog.tests[position].body, &[])?;
        checker.borrow_conflicts()?;
    }
    Ok(())
}

fn resolve_type(ty: &TypeExpr, names: &Names, allow_void: bool) -> Res<Ty> {
    if !matches!(ty.name.as_str(), "Result" | "List" | "HashMap" | "HashSet" | "Func") && !ty.args.is_empty() {
        return Err(Diag::error(ty.span, format!("`{}<...>`: generic types are not in the interpreter yet", ty.name))
            .note("the types written with `<` and `>` for now are `Result<T, E>`, `List<T>`, `HashMap<K, V>`, `HashSet<T>` and `Func<A, R>`"));
    }
    let no_borrow = |arg: &TypeExpr, of: &str| match arg.mode {
        Some(_) => Err(Diag::error(arg.span, format!("a `{of}` that holds borrows is not in the interpreter yet"))
            .note("it needs the borrow check, which is not here yet")),
        None => Ok(()),
    };
    let mut resolved = match Ty::from_name(&ty.name) {
        _ if ty.name == "List" => {
            let [item] = ty.args.as_slice() else {
                return Err(Diag::error(ty.span, "`List` is written with the type of its items: `List<T>`"));
            };
            no_borrow(item, "List")?;
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
        _ if ty.name == "HashSet" => {
            let [item] = ty.args.as_slice() else {
                return Err(Diag::error(ty.span, "`HashSet` is written with the type of its items: `HashSet<T>`"));
            };
            no_borrow(item, "HashSet")?;
            let item_ty = resolve_type(item, names, false)?;
            if !has(&item_ty, &names.keys, false) || matches!(item_ty, Ty::Ptr(_) | Ty::Nullable(_)) {
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
            if !has(&key_ty, &names.keys, false) || matches!(key_ty, Ty::Ptr(_) | Ty::Nullable(_)) {
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
        Some(Ty::Void) if !allow_void || !ty.suffixes.is_empty() || ty.mode.is_some() => {
            return Err(Diag::error(ty.span, "`void` can only be a return type"));
        }
        Some(base) => base,
        None => match names.full(&ty.name, ty.span)?.and_then(|full| names.kinds.get(&full).map(|kind| (full, *kind))) {
            Some((full, TypeKind::Struct)) => Ty::Struct(full),
            Some((full, TypeKind::Enum)) => Ty::Enum(full),
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
    for suffix in &ty.suffixes {
        if matches!(resolved, Ty::Slice(_)) {
            return Err(match suffix {
                Suffix::Nullable => Diag::error(ty.span, "a slice that may be missing is not in the interpreter yet"),
                _ => bare_slice(ty.span, &resolved),
            });
        }
        resolved = match suffix {
            Suffix::Array(0) => return Err(Diag::error(ty.span, "an array has at least one item")),
            Suffix::Array(_) if matches!(resolved, Ty::Array(..)) => {
                return Err(Diag::error(ty.span, "an array of arrays is not in the interpreter yet"));
            }
            Suffix::Array(length) => Ty::Array(Box::new(resolved), *length),
            Suffix::Slice => Ty::Slice(Box::new(resolved)),
            Suffix::Pointer => Ty::Ptr(Box::new(resolved)),
            Suffix::Nullable if matches!(resolved, Ty::Nullable(_)) => {
                return Err(Diag::error(ty.span, "`??` is not a type: a value is either there or `null`"));
            }
            Suffix::Nullable => Ty::Nullable(Box::new(resolved)),
        };
    }
    if ty.mode.is_none() && matches!(resolved, Ty::Slice(_)) {
        return Err(bare_slice(ty.span, &resolved));
    }
    Ok(match ty.mode {
        Some(Mode::Read) => Ty::Read(Box::new(resolved)),
        Some(Mode::Ref) => Ty::Ref(Box::new(resolved)),
        None => resolved,
    })
}

/// `byte[]` written without `read` or `ref`: a slice owns nothing, so it is not a type alone.
fn bare_slice(span: Span, slice: &Ty) -> Diag {
    let Ty::Slice(item) = slice else { unreachable!("called for a slice") };
    Diag::error(span, format!("`{slice}` is not a type on its own: a slice only looks at items that something else owns"))
        .hint(format!("`List<{item}>` to own the items, `read {slice}` for a window onto them"))
}

/// The number an index is known to be before the program runs.
fn known_int(expr: &Expr) -> Option<i128> {
    match &expr.kind {
        ExprKind::Int(value, _) => Some(*value),
        ExprKind::Widen(_, inner) => known_int(inner),
        _ => None,
    }
}

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
    if let Ty::Float(_) = ty {
        return Diag::error(span, format!("`{ty}` cannot be {what}: it is not `Hashable`"))
            .note("a fractional number is not `Hashable`: `0.0` and `-0.0` are equal and have different bits, and a sum seldom gives the exact key back");
    }
    let diag = Diag::error(span, format!("`{ty}` cannot be {what}: it is not `Equatable` and `Hashable`"));
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => diag.hint(format!("add them to the definition: `{name} : Equatable, Hashable`")),
        Ty::List(_) | Ty::Set(_) | Ty::Map(..) => diag.note("a collection can be compared with `==`, but it is not `Hashable`"),
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


/// `: Copyable` holds only if everything the type carries can be copied.
fn all_copyable<'d>(name: &str, fields: impl Iterator<Item = (&'d FieldDecl, &'d (String, Ty))>, copyable: &HashSet<String>) -> Res<()> {
    for (field, (_, ty)) in fields {
        if ty.is_copyable(copyable) {
            continue;
        }
        let diag = Diag::error(field.ty.span, format!("`{name}` cannot be `Copyable`: `{}` is a `{ty}`, which is not", field.name));
        return Err(match ty {
            Ty::Struct(inner) | Ty::Enum(inner) => diag.hint(format!("make `{inner}` `Copyable` too, or take `Copyable` off `{name}`")),
            Ty::Str | Ty::Ptr(_) | Ty::List(_) | Ty::Map(..) | Ty::Set(_) => diag.note(format!("a `{ty}` owns memory on the heap; a copy would have to allocate, and that is written `.clone()`")),
            _ => diag,
        });
    }
    Ok(())
}

/// Whether a value of this type is `Equatable` (or `Hashable`): numbers, `bool`, text, the
/// types in `declared`, and a `T*` or `T?` of those. With `collections`, a collection counts
/// if what it holds does: that is so for `Equatable`, not for `Hashable`.
///
/// A fractional number is `Equatable` and not `Hashable`, like a collection: two values that
/// `equals` calls the same, `0.0` and `-0.0`, have different bits.
fn has(ty: &Ty, declared: &HashSet<String>, collections: bool) -> bool {
    match ty {
        Ty::Int(_) | Ty::Bool | Ty::Char | Ty::Str => true,
        Ty::Float(_) => collections,
        Ty::Struct(name) | Ty::Enum(name) => declared.contains(name),
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::Read(inner) | Ty::Ref(inner) => has(inner, declared, collections),
        Ty::List(item) | Ty::Set(item) | Ty::Array(item, _) | Ty::Slice(item) => collections && has(item, declared, collections),
        Ty::Map(key, value) => collections && has(key, declared, collections) && has(value, declared, collections),
        _ => false,
    }
}

/// The struct or enum inside `ty` that is not in `declared`.
fn lacking<'t>(ty: &'t Ty, declared: &HashSet<String>) -> Option<&'t str> {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => (!declared.contains(name)).then_some(name.as_str()),
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) | Ty::Array(inner, _) | Ty::Slice(inner) => lacking(inner, declared),
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
            (Ty::List(_) | Ty::Set(_) | Ty::Map(..), _) if word == "Hashable" => diag.note("a collection can be compared with `==`, but it is not a key"),
            (Ty::Float(_), _) => diag.note("a fractional number is not `Hashable`"),
            (_, Some(inner)) => diag.hint(format!("make `{inner}` `{word}` too")),
            _ => diag,
        });
    }
    Ok(())
}

/// The type behind borrows and pointers.
fn peel(mut ty: &Ty) -> &Ty {
    while let Ty::Read(inner) | Ty::Ref(inner) | Ty::Ptr(inner) = ty {
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
        let diag = Diag::error(field.ty.span, format!("`{name}` cannot be written as text field by field: `{}` is a `{ty}`, which is not `Formattable`", field.name));
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
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::Read(inner) | Ty::Ref(inner) => can_format(inner, formattable),
        Ty::Result(ok, error) | Ty::Map(ok, error) => can_format(ok, formattable) && can_format(error, formattable),
        Ty::List(item) | Ty::Set(item) | Ty::Array(item, _) | Ty::Slice(item) => can_format(item, formattable),
        _ => false,
    }
}

/// The struct or enum inside `ty` that stands in the way of writing it as text.
fn unformattable<'t>(ty: &'t Ty, formattable: &HashSet<String>) -> Option<&'t str> {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => (!formattable.contains(name)).then_some(name.as_str()),
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::Read(inner) | Ty::Ref(inner) => unformattable(inner, formattable),
        Ty::Result(ok, error) | Ty::Map(ok, error) => unformattable(ok, formattable).or_else(|| unformattable(error, formattable)),
        Ty::List(item) | Ty::Set(item) | Ty::Array(item, _) | Ty::Slice(item) => unformattable(item, formattable),
        _ => None,
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
        members[from].iter().any(|ty| match by_value(ty) {
            Some(inner) => inner == target || reaches(inner, target, members, seen),
            None => false,
        })
    }
    fn by_value(ty: &Ty) -> Option<&str> {
        match ty {
            Ty::Struct(name) | Ty::Enum(name) => Some(name),
            Ty::Nullable(inner) | Ty::Array(inner, _) => by_value(inner),
            _ => None,
        }
    }

    for (field, (_, ty)) in fields {
        let Some(inner) = by_value(ty) else { continue };
        if inner == name || reaches(inner, name, members, &mut Vec::new()) {
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

/// Whether the expression gives a value that nothing owns after the statement: what a call
/// returned, a value being built. A text constant is there for the whole run, and a borrow
/// is judged by what it borrowed.
fn unnamed_owner(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Str(_) | ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::FuncRef(_) => false,
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
    /// The place is itself a borrow of a value that can be copied: the type to write to
    /// get a copy of the value instead of a second borrow.
    copy_as: Option<String>,
    /// The loops that were open where the holder was declared.
    holder_loops: Vec<usize>,
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
}

#[derive(Default)]
struct Scope {
    vars: Vec<Var>,
    /// Narrowings that end with this scope: variable, type before, assignment count at the time.
    narrowed: Vec<(String, Ty, u32)>,
}

type Moves = Vec<Vec<Moved>>;

struct Checker<'a> {
    names: &'a Names,
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
    /// How many scopes were open where each enclosing loop starts: a `break` ends those above.
    loop_depths: Vec<usize>,
    ret: Ty,
    /// Where a returned borrow may come from.
    sources: Vec<Source>,
    func_name: String,
    /// A `test` block is being checked: `try` passes any error on, and the test fails with it.
    in_test: bool,
    /// Every name an `is` pattern has given, with where: to explain a use of one after its
    /// `if` has ended.
    is_names: Vec<(String, Span)>,
    /// The struct whose method is being checked.
    owner: Option<&'a str>,
    /// The module the code being checked is in; empty for the root.
    module: &'a str,
    src: &'a str,
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

/// The variants a `match` or `is` chooses between: those of an enum, or `Ok` and `Error`.
struct Variants {
    /// The type as it is named in messages.
    owner: String,
    /// In declaration order, each with the fields it carries.
    list: Vec<(String, Vec<(String, Ty)>)>,
}

/// Names a pattern brings into the block it guards: name, type, where it is written.
type Binds = Vec<(String, Ty, Span)>;

/// `x is null` or `x is not null` on a plain variable: (name, is the test "not null").
type Narrowing = (String, bool);

/// Whether anything sees the names of an `is` pattern where it is written.
#[derive(Clone, Copy)]
enum NamesSeen {
    /// The right of `&&`, or the block of an `if` or `while`.
    Yes,
    /// Nothing: an `is` joined by `||`.
    AfterOr,
    /// Nothing: an `is` anywhere else.
    No,
}

/// The variables that a chain of `op` tests with `x is null` (`negated`: `x is not null`)
/// as one of its parts: `a is not null && b is not null` tests `a` and `b`.
fn null_tests(cond: &Expr, op: BinOp, negated: bool) -> Vec<String> {
    match &cond.kind {
        ExprKind::Binary(inner, left, right) if *inner == op => {
            let mut names = null_tests(left, op, negated);
            names.extend(null_tests(right, op, negated));
            names
        }
        _ => narrowing(cond).filter(|(_, not)| *not == negated).map(|(name, _)| name).into_iter().collect(),
    }
}

fn narrowing(cond: &Expr) -> Option<Narrowing> {
    match &cond.kind {
        ExprKind::IsNull { expr, negated } => match &expr.kind {
            ExprKind::Var(name) => Some((name.clone(), *negated)),
            _ => None,
        },
        _ => None,
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
        scope.vars.push(Var { id, decl_loops, loans: Vec::new(), param: false, name: name.to_string(), unopened: None, declared: ty.clone(), current: ty, moved, assigned: 0, at: span });
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
        self.declare(name, ty, span)
    }

    /// `Stdin`, `Stdout` and `Stderr`: every function and test has them, as `ref` parameters
    /// that the program gives. They are lent on and written through, never moved.
    fn declare_streams(&mut self, span: Span) -> Res<()> {
        for (name, ty) in [("Stdin", Ty::Reader), ("Stdout", Ty::Writer), ("Stderr", Ty::Writer)] {
            self.declare(name, Ty::Ref(Box::new(ty)), span)?;
            self.find(name).expect("just declared").param = true;
        }
        Ok(())
    }

    /// The `Closable` check: a `StreamWriter` kept in a local of the scopes from `from` on must
    /// be closed, or moved somewhere, before they end, since `close` is where the error of
    /// writing its buffer shows up. `exit`: the `return`, `break`, `continue` or `exit` that
    /// ends them, or `None` for the end of the innermost block.
    fn unclosed(&self, from: usize, exit: Option<(Span, &str)>) -> Res<()> {
        for var in self.scopes[from..].iter().flat_map(|scope| &scope.vars) {
            if var.declared != Ty::Writer || (var.moved.whole.is_some() && !var.moved.partly) {
                continue;
            }
            let name = &var.name;
            let hint = format!("`try {name}.close()`; to drop the error on purpose, `_ = {name}.close()`");
            let ends = if var.param { "the function ends" } else { "its block ends" };
            let diag = match (exit, var.moved.whole.is_some()) {
                (_, true) => Diag::error(var.at, format!("`{name}` is closed on some paths but not on all of them, so an error writing its buffer could be lost"))
                    .note("close it on every path, or once where the paths meet"),
                (Some((span, word)), false) => Diag::error(span, format!("`{name}` is not closed at this `{word}`, so an error writing its buffer would be lost"))
                    .related(var.at, "it is opened here"),
                (None, false) => Diag::error(var.at, format!("`{name}` is not closed before {ends}, so an error writing its buffer would be lost")),
            };
            let diag = match exit {
                Some((_, "exit")) => diag.note("`exit` ends the program at once: a writer that is not closed loses what its buffer holds"),
                _ => diag,
            };
            return Err(diag.hint(hint));
        }
        Ok(())
    }

    fn find(&mut self, name: &str) -> Option<&mut Var> {
        self.scopes.iter_mut().rev().find_map(|scope| scope.vars.iter_mut().rev().find(|var| var.name == name))
    }

    fn var(&mut self, name: &str, span: Span) -> Res<&mut Var> {
        if self.find(name).is_none() {
            return Err(self.unknown_var(name, span));
        }
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
        if let Some((_, at)) = self.is_names.iter().rev().find(|(named, at)| named == name && at.start < span.start) {
            return diag
                .related(*at, format!("`{name}` is named by this `is`"))
                .note("the names of `is` are seen on the right of `&&` and in the block of its `if` or `while`, not in `else` or after it")
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
        crate::parser::dotted(expr).filter(|path| self.names.modules.contains(path))
    }

    /// Turns the name of a type or a function into its full name, as the rest of the checker
    /// and the interpreter know it: `Product` is `stock.Product` after `use stock`, and
    /// `stock.count(xs)` is a call of the function `stock.count`.
    fn settle(&mut self, expr: &mut Expr) -> Res<()> {
        if let ExprKind::Field { recv, .. } | ExprKind::Method { recv, .. } = &mut expr.kind {
            let module = match self.names.modules.is_empty() {
                true => None,
                false => self.module_named(recv),
            };
            match module {
                Some(module) => {
                    expr.kind = match std::mem::replace(&mut expr.kind, ExprKind::Null) {
                        ExprKind::Field { name, .. } => ExprKind::Var(format!("{module}.{name}")),
                        ExprKind::Method { name, name_span, args, .. } => {
                            ExprKind::Call { name: format!("{module}.{name}"), name_span, args, callee: Callee::Plain }
                        }
                        _ => unreachable!("matched above"),
                    };
                }
                None if matches!(recv.kind, ExprKind::Var(_) | ExprKind::Field { .. }) => self.settle(recv)?,
                None => {}
            }
        }
        let span = expr.span;
        let (ExprKind::Var(written) | ExprKind::Call { name: written, .. } | ExprKind::StructLit { name: written, .. }) = &expr.kind else {
            return Ok(());
        };
        if written == "this" || BUILTINS.contains(&written.as_str()) || self.find(written).is_some() {
            return Ok(());
        }
        let (ExprKind::Var(name) | ExprKind::Call { name, .. } | ExprKind::StructLit { name, .. }) = &mut expr.kind else {
            unreachable!("matched above")
        };
        if let Some(full) = self.names.full(name, span)? {
            *name = full;
        }
        Ok(())
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
            return Err(Diag::error(span, format!("`{name}` was moved and can no longer be used")).related(moved, "it was moved here"));
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
        let Some((name, _)) = store_path(place) else { return Vec::new() };
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
                false => Vec::new(),
            },
            ExprKind::Load(inner) | ExprKind::Unbox(inner) | ExprKind::AsSlice(inner) | ExprKind::Widen(_, inner) | ExprKind::New(inner) | ExprKind::Try { expr: inner, .. } => {
                self.loans_of(inner, mutable)
            }
            ExprKind::Coalesce { left, right, .. } => union(&mut [&**left, &**right].into_iter()),
            // A value built from borrows carries them.
            ExprKind::StructLit { fields, .. } => union(&mut fields.iter().map(|field| &field.value)),
            ExprKind::Variant { args, .. } => union(&mut args.iter()),
            // What a call returns borrows from what its signature names: the one borrowed
            // parameter, or those after `from`. A built-in method lends from its receiver.
            ExprKind::Method { recv, name, args, callee, .. } => {
                // `slot.take()` and `slot.replace(v)` give what was in the place.
                if matches!(callee, Callee::Take | Callee::Replace) {
                    return self.loans_of(recv, mutable);
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
            _ => Vec::new(),
        }
    }

    /// A value that borrows is put into a place: a field, an item, or a variable of a type
    /// that carries a borrow. A local then holds what the value borrowed. Through a borrow
    /// (`this.current = token`) the value must come from what that same owner holds.
    fn stores(&mut self, target: &Expr, value: &Expr) -> Res<()> {
        let loans = self.loans_of(value, false);
        self.kept_unnamed(&loans, value, "the place it is put into")?;
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
        let loans = loans.into_iter().filter(|loan| loan.root != id);
        match path.is_empty() {
            true => var.loans = loans.collect(),
            false => var.loans.extend(loans),
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
        let copy_as = (var.declared.is_borrow() && behind.is_copyable(self.copyable)).then(|| behind.to_string());
        let at = self.point(expr.span.start);
        let mut found = Vec::new();
        for var in self.scopes.iter().flat_map(|scope| &scope.vars) {
            if var.id == root {
                continue;
            }
            for loan in &var.loans {
                let overlaps = path.is_empty() || loan.path.is_empty() || within(&path, &loan.path) || within(&loan.path, &path);
                if loan.root == root && !loan.held && overlaps && (loan.mutable || access != Access::Read) {
                    found.push(Clash {
                        holder: var.id,
                        holder_name: var.name.clone(),
                        owner_name: String::new(),
                        copy_as: copy_as.clone().filter(|_| loan.path.is_empty()),
                        holder_loops: var.decl_loops.clone(),
                        loan: loan.clone(),
                        access,
                        span: expr.span,
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
            let used_later = self.borrows.uses.iter().find(|(var, at)| {
                if *var != clash.holder {
                    return false;
                }
                // Not reached from here: the other branch of an `if` or a `match` around both.
                let other_branch = at.branches.iter().any(|(node, branch)| clash.at.branches.iter().any(|(n, b)| n == node && b != branch));
                let after = (at.pos, at.seq) > (clash.at.pos, clash.at.seq) && !other_branch;
                // A loop comes round again, unless the variable is new in every round.
                let next_round = clash.at.loops.iter().any(|l| !clash.holder_loops.contains(l) && at.loops.contains(l));
                after || next_round
            });
            let Some((_, later)) = used_later else { continue };
            let text = &self.src[clash.span.start..clash.span.end];
            let holder = &clash.holder_name;
            if clash.access == Access::Gone {
                let owner = &clash.owner_name;
                return Err(Diag::error(clash.span, format!("`{holder}` borrows from `{owner}` here, but `{owner}` is gone at the end of its block and `{holder}` is used after that"))
                    .hint(format!("declare `{owner}` where `{holder}` is declared, so that it lives as long")));
            }
            let diag = match (clash.access, clash.loan.mutable) {
                (Access::Read, _) => Diag::error(clash.span, format!("`{text}` cannot be used here: `{holder}` holds a `ref` borrow of it and is used again later")),
                (Access::Write, _) => Diag::error(clash.span, format!("`{text}` cannot be changed here: `{holder}` borrows from it and is used again later")),
                (Access::Move | Access::Gone, _) => Diag::error(clash.span, format!("`{text}` cannot be moved here: `{holder}` borrows from it and is used again later")),
            };
            let mut diag = diag.related(clash.loan.span, format!("`{holder}` borrowed it here"));
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
    /// one of them as `ref`. `recv`: the receiver of a method and whether it is taken as `ref`.
    fn lent_twice(&mut self, recv: Option<(&Expr, bool)>, args: &[Expr]) -> Res<()> {
        let mut lent: Vec<(usize, String, bool, Span)> = Vec::new();
        let places = recv.into_iter().chain(args.iter().filter_map(|arg| match &arg.kind {
            ExprKind::Borrow { mutable, expr } => Some((&**expr, *mutable)),
            _ => None,
        }));
        for (place, mutable) in places {
            let Some((name, path)) = store_path(place) else { continue };
            let Some(root) = self.find(name).map(|var| var.id) else { continue };
            let clash = lent.iter().find(|(other, other_path, other_mutable, _)| {
                let overlaps = path.is_empty() || other_path.is_empty() || within(&path, other_path) || within(other_path, &path);
                *other == root && overlaps && (mutable || *other_mutable)
            });
            if let Some((_, _, _, first)) = clash {
                let text = &self.src[place.span.start..place.span.end];
                return Err(Diag::error(place.span, format!("`{text}` is lent twice in one call, and a `ref` borrow must be the only one"))
                    .related(*first, "the other borrow is here")
                    .hint("copy what is needed into a variable before the call"));
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
            let lent = self.loans_of(part, false).iter().any(|loan| loan.root == root && overlaps(&loan.path));
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
        // A variable outside that still holds a borrow of an owner that ends here.
        let at = self.point(self.borrows.pos);
        for owner in scope.vars.iter().filter(|var| !var.declared.is_borrow()) {
            for holder in self.scopes.iter().flat_map(|outer| &outer.vars) {
                for loan in holder.loans.iter().filter(|loan| loan.root == owner.id && !loan.held) {
                    self.borrows.clashes.push(Clash {
                        holder: holder.id,
                        holder_name: holder.name.clone(),
                        owner_name: owner.name.clone(),
                        copy_as: None,
                        holder_loops: holder.decl_loops.clone(),
                        loan: loan.clone(),
                        access: Access::Gone,
                        span: loan.span,
                        at: at.clone(),
                    });
                }
            }
        }
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

    fn moves(&self) -> Moves {
        self.scopes.iter().map(|scope| scope.vars.iter().map(|var| var.moved.clone()).collect()).collect()
    }

    fn set_moves(&mut self, moves: &Moves) {
        for (scope, saved) in self.scopes.iter_mut().zip(moves) {
            for (var, moved) in scope.vars.iter_mut().zip(saved) {
                var.moved = moved.clone();
            }
        }
    }

    /// After two branches a variable is gone if either branch moved it.
    fn merge_moves(&mut self, other: &Moves) {
        for (scope, saved) in self.scopes.iter_mut().zip(other) {
            for (var, moved) in scope.vars.iter_mut().zip(saved) {
                var.moved.partly = var.moved.partly || moved.partly || var.moved.whole.is_some() != moved.whole.is_some();
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
        let reached = !leaves(block, self.loops > 0);
        self.pop_scope(reached)
    }

    /// Declares the names of a pattern. Those that are borrows hold what the pattern's value
    /// was lent from.
    fn declare_binds(&mut self, binds: Binds) -> Res<()> {
        let loans = std::mem::take(&mut self.borrows.binds);
        for (name, ty, span) in binds {
            let borrows = self.holds(&ty);
            self.declare_local(&name, ty, span)?;
            if borrows {
                self.find(&name).expect("just declared").loans = loans.clone();
            }
        }
        Ok(())
    }

    fn stmt(&mut self, stmt: &mut Stmt) -> Res<()> {
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
                        Ty::Read(inner) | Ty::Ref(inner) if matches!(init.kind, ExprKind::Var(_)) && inner.is_copyable(self.copyable) => {
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
                let holds_result = matches!(ty, Ty::Result(..));
                let loans = match self.holds(&ty) {
                    true => self.loans_of(init, matches!(ty, Ty::Ref(_))),
                    false => Vec::new(),
                };
                self.kept_unnamed(&loans, init, &format!("`{name}`"))?;
                self.declare_local(name, ty, *name_span)?;
                self.find(name).expect("just declared").loans = loans;
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
                _ => {}
            },
            StmtKind::Assign { target, op, value, through } => {
                // The right side is computed first; then the place is found and written (Rust's
                // order). What the place uses and the write itself count as after the right side,
                // so a borrow that the right side reads last ends before the write (`b = Box { n
                // = r.n + 1 }`), and a borrow the place goes through must outlive the right side.
                let (first_use, first_clash) = (self.borrows.uses.len(), self.borrows.clashes.len());
                let target_ty = match &target.kind {
                    ExprKind::Var(name) => {
                        let var = if op.is_some() { self.live_var(name, target.span)? } else { self.var(name, target.span)? };
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
                        let Some((ty, writable)) = self.place_unchecked(target)? else {
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
                        if !writable && self.is_entry_key(target) {
                            return Err(Diag::error(target.span, "the key of an entry cannot be changed: the map found the entry by it")
                                .hint("remove the entry and add it under the new key: `map.remove(key)`, `map.add(key, value)`"));
                        }
                        if !writable && target.in_item() {
                            return Err(Diag::error(target.span, "this item is reached through a `read` borrow and cannot be written")
                                .hint("take the value as `ref` instead of `read`"));
                        }
                        if !writable {
                            return Err(Diag::error(target.span, "this field is reached through a `read` borrow and cannot be written")
                                .hint("take the value as `ref` instead of `read`"));
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
                if let (Some(op), Ty::Float(_)) = (*op, &target_ty) {
                    if !matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem) {
                        return Err(Diag::error(stmt_span, format!("`{}=` needs an integer on the left, found `{target_ty}`", op.symbol()))
                            .note("the bit operators work on integers"));
                    }
                } else if op == &Some(BinOp::Add) && target_ty == Ty::Str {
                    let text = &self.src[target.span.start..target.span.end];
                    return Err(Diag::error(stmt_span, "`+=` needs a number on the left, found `string`")
                        .hint(format!("a text grows with `write`: `{text}.write(...)`")));
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
                    _ => {
                        self.typed(value, &target_ty)?;
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
                // A value that borrows, put into a field, an item or a variable of a type that
                // carries a borrow.
                if !rebinds && self.holds(&target_ty) {
                    self.stores(target, value)?;
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
                        let var = self.var(name, target.span)?;
                        if var.declared == Ty::Writer && (var.moved.whole.is_none() || var.moved.partly) {
                            return Err(Diag::error(target.span, format!("`{name}` is given another writer while the one it holds is still open, so an error writing its buffer would be lost"))
                                .related(var.at, "it is declared here")
                                .hint(format!("close it first: `try {name}.close()`; to drop the error on purpose, `_ = {name}.close()`")));
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
            StmtKind::If { cond, then, els } => {
                // The condition has a scope of its own, which the block shares and `else` does
                // not: the names of `x is Circle(r)` and what `x is not null` narrows.
                self.scopes.push(Scope::default());
                self.condition(cond)?;
                // Not `null` where the condition holds: `x is not null && ...`; where it does
                // not: `x is null || ...`.
                let when_true = null_tests(cond, BinOp::And, true);
                let when_false = null_tests(cond, BinOp::Or, false);

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
                    self.borrows.branches.push((node, 1));
                    self.block(els, &when_false)?;
                    self.borrows.branches.pop();
                    else_leaves = leaves(els, true);
                }
                // A branch that leaves does not reach the code after the `if`.
                match (then_leaves, else_leaves) {
                    (true, _) => {}
                    (false, true) => self.set_moves(&after_then),
                    (false, false) => self.merge_moves(&after_then),
                }
                // `if x is null { return }`: from here on `x` is not null.
                let after = match (then_leaves, else_leaves) {
                    (true, _) => when_false,
                    (false, true) => when_true,
                    (false, false) => Vec::new(),
                };
                for name in &after {
                    self.narrow(name);
                }
            }
            StmtKind::While { cond, body } => {
                self.forget_narrowing_of_assigned(body, Some(cond));
                let node = self.node();
                self.borrows.loops.push(node);
                self.scopes.push(Scope::default());
                let result = self.condition(cond).and_then(|_| self.loop_body(body, &[], Vec::new()));
                self.borrows.loops.pop();
                result?;
                self.pop_scope(true)?;
            }
            StmtKind::For { var, var_span, from, to, body } => {
                self.forget_narrowing_of_assigned(body, None);
                let from_ty = self.int_operand(from, None, "the start of the range")?;
                let to_ty = self.int_operand(to, Some(from_ty), "the end of the range")?;
                let ty = Ty::Int(self.common_int(from, Ty::Int(from_ty), to, Ty::Int(to_ty))?);
                let node = self.node();
                self.borrows.loops.push(node);
                self.scopes.push(Scope::default());
                self.declare_local(var, ty, *var_span)?;
                let result = self.loop_body(body, &[], Vec::new());
                self.borrows.loops.pop();
                result?;
                self.pop_scope(true)?;
            }
            StmtKind::ForEach { var, var_span, iter, body, by } => {
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
                let lock = store_path(source).and_then(|(name, path)| {
                    let text = self.src[source.span.start..source.span.end].to_string();
                    Some(Lock { var: self.var_id(name)?, path, text, positions: false, span: iter.span })
                });
                let found = self.expr(iter, None)?;
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
                let item = match mode {
                    None => item,
                    Some(Mode::Read) => Ty::Read(Box::new(item)),
                    Some(Mode::Ref) => Ty::Ref(Box::new(item)),
                };
                *by = mode;
                // The variable is new in every round, so an owned item can be used up.
                // A borrowed collection stays as it is until the loop ends.
                let locked = mode.is_some() && lock.is_some();
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
                    self.borrows.binds.clear();
                }
                let node = self.node();
                self.borrows.loops.push(node);
                let result = self.loop_body(body, &[], vec![(var.clone(), item, *var_span)]);
                self.borrows.loops.pop();
                if locked {
                    self.locks.pop();
                }
                for _ in 0..text_locks {
                    self.locks.pop();
                }
                result?;
            }
            StmtKind::Return(value) => self.returned(value.as_mut(), stmt_span)?,
            StmtKind::Break | StmtKind::Continue => {
                let word = if matches!(stmt.kind, StmtKind::Break) { "break" } else { "continue" };
                if self.loops == 0 {
                    return Err(Diag::error(stmt_span, format!("`{word}` can only be written inside a loop")));
                }
                let depth = *self.loop_depths.last().expect("inside a loop");
                self.unclosed(depth, Some((stmt_span, word)))?;
            }
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
    fn unused(&mut self, expr: &mut Expr) -> Res<()> {
        let span = expr.span;
        match &mut expr.kind {
            ExprKind::Match { subject, arms, borrowed } => {
                self.match_expr(subject, arms, borrowed, None, true)?;
            }
            ExprKind::Call { .. } | ExprKind::Method { .. } | ExprKind::Try { .. } => {
                if matches!(self.expr(expr, None)?, Ty::Result(..)) {
                    let text = &self.src[span.start..span.end];
                    return Err(Diag::error(span, "this `Result` is not used, so an error would be lost silently")
                        .hint(format!("pass the error on with `try {text}`, or drop it on purpose with `_ = {text}`")));
                }
            }
            _ => {
                return Err(Diag::error(span, "the value of this expression is not used")
                    .hint("only a call, an assignment or a declaration can be a statement"));
            }
        }
        Ok(())
    }

    /// The condition of an `if` or `while`, checked in the scope that its block shares: the
    /// names of `x is Circle(r)` and the narrowing of `x is not null`, alone or on the left of
    /// `&&`, hold in the block.
    fn condition(&mut self, cond: &mut Expr) -> Res<()> {
        self.link(cond, BinOp::And)
    }

    /// One part of a chain of `&&` (or of `||`), checked in the scope of the chain. On the
    /// left of `&&`, an `is` names values and `x is not null` narrows `x` for what comes after
    /// it; on the left of `||`, `x is null` does, since the right side runs only when it did
    /// not hold.
    fn link(&mut self, part: &mut Expr, op: BinOp) -> Res<()> {
        if let ExprKind::Binary(inner, left, right) = &mut part.kind {
            if *inner == op {
                self.link(left, op)?;
                self.link(right, op)?;
                part.ty = Ty::Bool;
                return Ok(());
            }
        }
        if matches!(part.kind, ExprKind::IsVariant { .. }) {
            let binds = self.is_variant(part, if op == BinOp::And { NamesSeen::Yes } else { NamesSeen::AfterOr })?;
            part.ty = Ty::Bool;
            self.is_names.extend(binds.iter().map(|(name, _, span)| (name.clone(), *span)));
            return self.declare_binds(binds);
        }
        self.typed(part, &Ty::Bool)?;
        if let Some((name, negated)) = narrowing(part) {
            if negated == (op == BinOp::And) {
                self.narrow(&name);
            }
        }
        Ok(())
    }

    /// A value moved inside a loop is gone when the next iteration starts.
    fn loop_body(&mut self, body: &mut Block, narrow: &[String], binds: Binds) -> Res<()> {
        let before = self.moves();
        self.loops += 1;
        self.loop_depths.push(self.scopes.len());
        let result = self.block_with(body, narrow, binds);
        self.loop_depths.pop();
        self.loops -= 1;
        result?;
        if leaves(body, true) {
            return Ok(());
        }
        for (scope, saved) in self.scopes.iter().zip(&before) {
            for (var, was) in scope.vars.iter().zip(saved) {
                let new_part = var.moved.parts.iter().find(|(path, _)| !was.parts.iter().any(|(old, _)| old == path));
                let (what, at) = match (var.moved.whole, was.whole, new_part) {
                    (Some(at), None, _) => (var.name.clone(), at),
                    (_, _, Some((path, at))) => (format!("{}.{path}", var.name), *at),
                    _ => continue,
                };
                return Err(Diag::error(at, format!("`{what}` is moved inside a loop, so the next iteration cannot use it")));
            }
        }
        Ok(())
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

    fn coerce(&mut self, expr: &mut Expr, found: Ty, expected: &Ty) -> Res<()> {
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
            (Ty::Int(from), Ty::Int(to)) if from.widens_to(*to) => {
                wrap(expr, |inner| ExprKind::Widen(Ty::Int(*to), inner));
                expr.ty = Ty::Int(*to);
                Ok(())
            }
            (Ty::Int(_), Ty::Int(to)) => {
                mismatch(Some(format!("convert with `({})`; it panics if the value does not fit", to.name())))
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
            (Ty::Null, _) if matches!(expected.strip_borrow(), Ty::Nullable(_)) => Ok(()),
            (Ty::Null, _) => Err(Diag::error(expr.span, format!("`{expected}` cannot be `null`"))
                .hint(format!("a value that may be missing has the type `{expected}?`"))),
            (Ty::Void, _) => Err(Diag::error(expr.span, "this call does not return a value, and one is needed here")),
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
            (Ty::Fmt, Ty::Str) => Err(Diag::error(expr.span, "expected `string`; `$\"...\"` does not become a `string` on its own")
                .hint("add `.toString()` at the end; that is where the allocation shows")),
            // `T` where `T?` is expected.
            (_, Ty::Nullable(inner)) => self.coerce(expr, found.clone(), inner),
            // A borrow of a copyable value stands in for the value.
            (Ty::Read(inner) | Ty::Ref(inner), _) if !expected.is_borrow() && inner.is_copyable(self.copyable) => {
                wrap(expr, ExprKind::Load);
                expr.ty = (**inner).clone();
                self.coerce(expr, (**inner).clone(), expected)
            }
            (Ty::Read(inner), Ty::Str) if **inner == Ty::Str => {
                mismatch(Some(format!("`{found}` only borrows the text; to own a copy write `{text}.clone()`")))
            }
            (Ty::Read(_) | Ty::Ref(_), _) if !expected.is_borrow() => {
                mismatch(Some(format!("`{found}` only borrows the value; `{expected}` needs to own it")))
            }
            // A borrow of a `T*` stands in for a borrow of the `T` it owns.
            (Ty::Read(from), Ty::Read(to)) | (Ty::Ref(from), Ty::Read(to) | Ty::Ref(to))
                if matches!(&**from, Ty::Ptr(inner) if inner == to) =>
            {
                wrap(expr, ExprKind::Unbox);
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
            // A text constant where a `read string` goes: lent as it is.
            (Ty::Str, Ty::Read(to)) if **to == Ty::Str && matches!(expr.kind, ExprKind::Str(_)) => {
                wrap(expr, |inner| ExprKind::Borrow { mutable: false, expr: inner });
                expr.ty = expected.clone();
                Ok(())
            }
            (_, Ty::Read(to)) if found == **to => mismatch(Some(format!("lend it with `read`: `read {text}`"))),
            (_, Ty::Ref(to)) if found == **to => mismatch(Some(format!("lend it with `ref`: `ref {text}`"))),
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
        let found = self.place_unchecked(expr)?;
        if found.is_some() {
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
        // `byte[16]`: a type name and a number in brackets build an array of zeros.
        if let ExprKind::Index { recv, index, .. } = &expr.kind {
            if let (ExprKind::Var(name), ExprKind::Int(length, _)) = (&recv.kind, &index.kind) {
                let is_type = Ty::from_name(name).is_some() || self.names.full(name, recv.span)?.is_some_and(|full| self.names.kinds.contains_key(&full));
                if self.find(name).is_none() && name != "this" && is_type && *length >= 0 {
                    let ty = TypeExpr { mode: None, name: name.clone(), args: Vec::new(), suffixes: vec![Suffix::Array(*length as usize)], span };
                    expr.kind = ExprKind::Array { ty, args: Vec::new(), items: None };
                    return Ok(None);
                }
            }
        }
        // `Shape.Empty` and `int.MaxValue` are values, not fields.
        if let ExprKind::Field { recv, .. } = &expr.kind {
            if self.enum_named(recv).is_some() || self.number_type_named(recv).is_some() {
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
                        Ty::Ref(inner) => {
                            writable = true;
                            *inner
                        }
                        Ty::Ptr(inner) => *inner,
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
                            return Err(match &recv.kind {
                                ExprKind::Var(var) => diag.hint(format!("test it first: `if {var} is not null {{ ... }}`")),
                                _ => diag.hint("copy it to a variable and test that with `is not null`; fields are not narrowed"),
                            });
                        }
                        Ty::Enum(_) => {
                            return Err(Diag::error(*name_span, format!("`{ty}` is an enum; what it carries depends on the variant"))
                                .hint("reach it with `match`, or test one variant: `if x is Circle(r) { ... }`"));
                        }
                        Ty::Result(..) => {
                            return Err(Diag::error(recv.span, format!("this is `{ty}`: either a value or an error; `.{name}` needs the value"))
                                .hint("open it first with `try`, `??` or `match`"));
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
                *index = position;
                Ok(Some((fields[position].1.clone(), writable && !info.readonly[position])))
            }
            // An item is a place too: `xs[i] = v`, `xs[i].name`, `ref xs[i]`.
            ExprKind::Index { recv, index, kind } => {
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
                        Ty::Ref(behind) => {
                            writable = true;
                            behind
                        }
                        Ty::Ptr(behind) => behind,
                        _ => break,
                    };
                }
                match inner {
                    Ty::Str => {
                        self.index(index, "an index")?;
                        *kind = IndexKind::Text;
                        Ok(Some((Ty::Int(IntTy::Byte), false)))
                    }
                    Ty::List(item) => {
                        self.index(index, "an index")?;
                        *kind = IndexKind::List;
                        Ok(Some(((**item).clone(), writable)))
                    }
                    Ty::Map(key, value) => {
                        self.typed(index, &self.key_param(key))?;
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
        let ty = self.expr_untyped(expr, expected)?;
        expr.ty = ty.clone();
        Ok(ty)
    }

    fn expr_untyped(&mut self, expr: &mut Expr, expected: Option<&Ty>) -> Res<Ty> {
        let span = expr.span;
        if let Some((ty, _)) = self.place(expr)? {
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
            let ty = Ty::Func(sig.params.clone(), Box::new(sig.ret.clone()));
            expr.kind = ExprKind::FuncRef(name.clone());
            return Ok(ty);
        }

        // `Shape.Empty` and `Shape.Circle(2)` build a value of the enum.
        match &mut expr.kind {
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
                // `Shape.unit()`: a function of the enum, not a variant.
                if let Some(owner) = self.enum_named(recv).filter(|owner| !self.methods_of(owner).contains_key(name.as_str())) {
                    let (index, ty) = self.variant(owner, name, *name_span, Some(args), span)?;
                    let args = std::mem::take(args);
                    expr.kind = ExprKind::Variant { owner: owner.to_string(), index, args };
                    return Ok(ty);
                }
            }
            ExprKind::Call { name, args, .. } if name == "Ok" || name == "Error" => {
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
                    Some(wanted) if wanted.fits(*value) => wanted,
                    Some(wanted) => {
                        // A number written in hex or in binary is shown as written, with its value.
                        let written = &self.src[span.start..span.end];
                        let shown = if written.starts_with("0x") || written.starts_with("0b") { format!("`{written}` is {value}, which") } else { value.to_string() };
                        let diag = Diag::error(span, format!("{shown} does not fit in {}", wanted.name()));
                        return Err(match *value < 0 && !wanted.signed() {
                            true => diag.note(format!("a `{}` is never negative", wanted.name())),
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
                let found = self.operand(operand, None)?;
                let from = found.strip_borrow().clone();
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
                    if !to.fits(value) {
                        let diag = Diag::error(span, format!("{value} does not fit in {}", to.name()));
                        return Err(match value < 0 && !to.signed() {
                            true => diag.note(format!("a `{}` is never negative", to.name())),
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
                // A field that cannot be seen cannot be given a value.
                if !self.inside(name) {
                    if let Some(position) = info.open.iter().position(|open| !open) {
                        let (short, field) = (short_name(name), &info.fields[position].0);
                        let diag = Diag::error(*name_span, format!("`{field}` is not public, so a `{short}` cannot be built field by field outside the `{}` module", module_of(name)));
                        return Err(match info.methods.get("create") {
                            Some(create) if create.public => diag.hint(format!("`{short}({})`", create.param_names.join(", "))),
                            _ => diag,
                        });
                    }
                }
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
                Ok(Ty::Ptr(Box::new(ty)))
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
                        return Err(Diag::error(recv.span, "this is reached through a `read` borrow, so a range of it cannot be lent as `ref`"));
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
                    return Err(Diag::error(inner.span, "this is reached through a `read` borrow and cannot be lent as `ref`"));
                }
                self.not_locked(inner, Access::Write)?;
                Ok(Ty::Ref(base))
            }
            ExprKind::IsNull { expr: inner, .. } => {
                let ty = self.operand(inner, None)?;
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
            ExprKind::Return(value) => {
                self.returned(value.as_deref_mut(), span)?;
                Ok(Ty::Never)
            }
            ExprKind::IsVariant { .. } => {
                self.is_variant(expr, NamesSeen::No)?;
                Ok(Ty::Bool)
            }
            ExprKind::Match { subject, arms, borrowed } => self.match_expr(subject, arms, borrowed, expected, false),
            ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. } => unreachable!("places are handled above"),
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
        let mut inner = &ty;
        loop {
            inner = match inner {
                Ty::Read(behind) => {
                    writable = false;
                    behind
                }
                Ty::Ref(behind) => {
                    writable = true;
                    behind
                }
                Ty::Ptr(behind) => behind,
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
            Ty::Nullable(value) if self.in_test => {
                Err(Diag::error(span, format!("`try` on `{value}?` returns `null` to the caller, and a test has no caller"))
                    .hint("say what is expected: `assert(x is not null)`, or give a fallback with `??`"))
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
                let ty = match mode {
                    None => ty.clone(),
                    Some(Mode::Read) => Ty::Read(Box::new(ty.clone())),
                    Some(Mode::Ref) => Ty::Ref(Box::new(ty.clone())),
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
                Ty::Int(int) => exhaust::Domain { name: int.name().to_string(), ranges: vec![(int.min(), int.max())], chars: false },
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
        if !ty.fits(value) {
            return Err(Diag::error(span, format!("{value} does not fit in {}, so this arm can never run", ty.name())));
        }
        pattern.kind = PatternKind::Int(value);
        Ok(Vec::new())
    }

    /// `x is Circle(r)`: looks at the value without taking it, so the names are borrows.
    /// They are seen by what comes after the test on the right of `&&`, and by the block of
    /// the `if` or `while` whose condition it is part of.
    fn is_variant(&mut self, expr: &mut Expr, seen: NamesSeen) -> Res<Binds> {
        let ExprKind::IsVariant { expr: subject, negated, pattern, by_ref } = &mut expr.kind else {
            unreachable!("only called for an `is` test");
        };
        let ty = self.operand(subject, None)?;
        let (variants, mode) = self.enum_behind(&ty, subject.span, "`is`")?;
        *by_ref = mode == Some(Mode::Ref);
        let binds = self.pattern_binds(pattern, &variants, mode.or(Some(Mode::Read)), &ty)?;
        if binds.is_empty() {
            return Ok(binds);
        }
        self.borrows.binds = self.place_loans(subject, *by_ref, subject.span);
        if *negated {
            return Err(Diag::error(pattern.span, "`is not` holds when the value is another variant, so there is nothing to name")
                .hint("write `_` for each value"));
        }
        let diag = Diag::error(pattern.span, "nothing here can use the names in this pattern")
            .note("the names of `is` are seen on the right of `&&` and in the block of the `if` or `while` it tests");
        match seen {
            NamesSeen::Yes => {}
            NamesSeen::AfterOr => {
                return Err(diag.note("with `||` the block also runs when this test does not hold, and then the names have no value")
                    .hint("test it alone in an `if` of its own, or use `match`"))
            }
            NamesSeen::No => return Err(diag.hint("write `_` for each value, or use `match`")),
        }
        Ok(binds)
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
        // Like `var y = subject`: an owned value is moved in, a borrow stays a borrow.
        let subject_ty = self.expr(subject, None)?;
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
        let subject_loans = match mode {
            Some(mode) => self.loans_of(subject, mode == Mode::Ref),
            None if self.holds(&subject_ty) => self.loans_of(subject, false),
            None => Vec::new(),
        };
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
        let mut result: Option<Ty> = expected.cloned();
        let before = self.moves();
        let mut after: Option<Moves> = None;
        for (arm_index, arm) in arms.iter_mut().enumerate() {
            self.set_moves(&before);
            self.scopes.push(Scope::default());
            self.borrows.branches.push((node, arm_index));
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
                let right_ty = self.operand(right, like(&left_ty).as_ref().or(wanted))?;
                (left_ty, right_ty)
            }
        };

        let undefined = || Diag::error(span, format!("operator `{}` is not defined for `{left_ty}` and `{right_ty}`", op.symbol()));
        // `a == b` is `a.equals(b)` and `a < b` is `a.compare(b) < 0`, for the types that have them.
        if let (Ty::Struct(name) | Ty::Enum(name), true, true) = (peel(&left_ty), peel(&left_ty) == peel(&right_ty), op.is_comparison()) {
            let symbol = op.symbol();
            if matches!(op, BinOp::Eq | BinOp::Ne) {
                if self.equatable.contains(name) {
                    return Ok(Ty::Bool);
                }
                return Err(Diag::error(span, format!("`{symbol}` compares the values, and `{name}` is not `Equatable`"))
                    .hint(format!("add it to the definition: `{name} : Equatable`")));
            }
            if self.comparable.contains(name) {
                return Ok(Ty::Bool);
            }
            return Err(Diag::error(span, format!("`{symbol}` puts the values in order, and `{name}` is not `Comparable`"))
                .hint(format!("add `: Comparable` to the definition and write `int compare(read this, read {name} other)`")));
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
                let common = Ty::Int(self.common_int(left, left_ty.clone(), right, right_ty.clone())?);
                Ok(if op.is_comparison() { Ty::Bool } else { common })
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
            (Ty::Bool, Ty::Bool) if matches!(op, BinOp::Eq | BinOp::Ne) => Ok(Ty::Bool),
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
        while let Ty::Ptr(behind) = inner {
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
        while let Ty::Ptr(behind) = inner {
            inner = behind.strip_borrow();
        }
        match inner {
            Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char | Ty::Str | Ty::Fmt => Ok(()),
            Ty::Struct(name) | Ty::Enum(name) if self.formattable.contains(name) => Ok(()),
            Ty::Struct(name) | Ty::Enum(name) => Err(Diag::error(span, format!("`{name}` cannot be written as text: it is not `Formattable`"))
                .hint(format!("add it to the definition: `{name} : Formattable`"))),
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
            other => Err(Diag::error(span, format!("a `{other}` value cannot be written as text"))),
        }
    }

    fn args(&mut self, name: &str, args: &mut [Expr], params: &[Ty], span: Span) -> Res<()> {
        if args.len() != params.len() {
            return Err(Diag::error(span, format!("`{name}` takes {} argument(s), {} given", params.len(), args.len())));
        }
        for (arg, param) in args.iter_mut().zip(params) {
            self.typed(arg, param)?;
        }
        self.lent_twice(None, args)
    }

    fn call(&mut self, name: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span) -> Res<Ty> {
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
            let Ty::Func(params, ret) = held else {
                return Err(Diag::error(name_span, format!("`{name}` is a `{held}`, not a function, and cannot be called")));
            };
            self.live_var(name, name_span)?;
            self.args(name, args, &params, span)?;
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
                if !self.structs[name].methods.contains_key("create") {
                    return Err(Diag::error(name_span, format!("`{name}` has no `create` function, so `{name}(...)` cannot build it"))
                        .hint(format!("write the fields: `{name} {{ ... }}`, or add a `create` function to the struct")));
                }
                self.static_call(name, "create", name_span, args, callee, span)
            }
            _ => {
                let sigs = self.sigs;
                let Some(sig) = sigs.get(name) else {
                    let diag = Diag::error(name_span, format!("there is no function named `{name}`"));
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
                self.args(name, args, &sig.params, span)?;
                Ok(sig.ret.clone())
            }
        }
    }

    /// `Stack.create(args)`: a function of the struct that does not take `this`.
    fn static_call(
        &mut self,
        owner: &str,
        name: &str,
        name_span: Span,
        args: &mut [Expr],
        callee: &mut Callee,
        span: Span,
    ) -> Res<Ty> {
        let methods = self.methods_of(owner);
        let Some(sig) = methods.get(name) else {
            let diag = Diag::error(name_span, format!("`{owner}` has no function `{name}`"));
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
        self.args(name, args, &sig.params, span)?;
        *callee = Callee::Static { owner: owner.to_string(), name: name.to_string() };
        Ok(sig.ret.clone())
    }

    /// `File.readText(path)`, `Stdin.readLine(ref line)`: what reads and writes files and
    /// the input. A path is a `read string`; each gives a `Result` with an `IoError`.
    fn io_call(&mut self, owner: &str, name: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span) -> Res<Ty> {
        let text = || Ty::Read(Box::new(Ty::Str));
        let bytes = Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
        let (call, params, ok) = match (owner, name) {
            ("File", "readText") => (IoCall::ReadText, vec![text()], Ty::Str),
            ("File", "readBytes") => (IoCall::ReadBytes, vec![text()], Ty::List(Box::new(Ty::Int(IntTy::Byte)))),
            ("File", "writeText") => (IoCall::WriteText, vec![text(), text()], Ty::Void),
            ("File", "writeBytes") => (IoCall::WriteBytes, vec![text(), bytes], Ty::Void),
            ("File", "appendText") => (IoCall::AppendText, vec![text(), text()], Ty::Void),
            ("File", "openText") => (IoCall::OpenText, vec![text()], Ty::Reader),
            ("File", "createText") => (IoCall::CreateText, vec![text()], Ty::Writer),
            ("File", "openAppend") => (IoCall::OpenAppend, vec![text()], Ty::Writer),
            _ => {
                let known = ["readText", "readBytes", "writeText", "writeBytes", "appendText", "openText", "createText", "openAppend"];
                let diag = Diag::error(name_span, format!("`{owner}` has no function `{name}`"));
                return Err(match nearest(name, known.iter().copied()) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag.note(format!("functions for now: {}", known.join(", "))),
                });
            }
        };
        self.args(name, args, &params, span)?;
        *callee = Callee::Io(call);
        Ok(Ty::Result(Box::new(ok), Box::new(Ty::Enum(IO_ERROR.to_string()))))
    }

    fn method(
        &mut self,
        recv: &mut Expr,
        name: &str,
        name_span: Span,
        args: &mut [Expr],
        callee: &mut Callee,
        span: Span,
    ) -> Res<Ty> {
        // `int.parse(text)`: the number the text writes, or where it stops being one.
        // `int.parseHex(text)`: the same in hex digits.
        if let Some(ty) = self.number_type_named(recv) {
            let radix = match (name, &ty) {
                ("parse", _) => 10,
                ("parseHex", Ty::Int(_)) => 16,
                ("parseHex", _) => {
                    return Err(Diag::error(name_span, format!("`parseHex` reads an integer, and `{ty}` is a fractional type"))
                        .hint(format!("`{ty}.parse`")))
                }
                _ => {
                    let diag = Diag::error(name_span, format!("`{ty}` has no function `{name}`"));
                    let functions = if matches!(ty, Ty::Int(_)) { "parse, parseHex" } else { "parse" };
                    return Err(match constant(&ty, name, name_span) {
                        Ok(()) => diag.hint(format!("`{name}` is a constant: `{ty}.{name}`, without parentheses")),
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
        // A type name in front of the dot: `Stack.create()`.
        if let ExprKind::Var(owner) = &recv.kind {
            if self.find(owner).is_none() && owner != "this" {
                if self.structs.contains_key(owner.as_str()) || self.enums.contains_key(owner.as_str()) {
                    return self.static_call(owner, name, name_span, args, callee, span);
                }
                if owner == "File" {
                    return self.io_call(owner, name, name_span, args, callee, span);
                }
                // `string.join(", ", read names)`
                // `string.fromUtf8(bytes)`: the bytes looked at as text, if they are text.
                if owner == "string" && name == "fromUtf8" {
                    let bytes = Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
                    self.args(name, args, &[bytes], span)?;
                    *callee = Callee::FromUtf8;
                    return Ok(Ty::Result(Box::new(Ty::Read(Box::new(Ty::Str))), Box::new(Ty::Struct(TEXT_ERROR.to_string()))));
                }
                // `string.withCapacity(n)`: an empty text with room for `n` bytes.
                if owner == "string" && name == "withCapacity" {
                    self.args(name, args, &[Ty::Int(IntTy::Nint)], span)?;
                    *callee = Callee::WithCapacity;
                    return Ok(Ty::Str);
                }
                if owner == "string" {
                    if name != "join" {
                        return Err(Diag::error(name_span, format!("`string` has no function `{name}`")).note("functions for now: join, fromUtf8, withCapacity"));
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
        if matches!(name, "sort" | "reverse") {
            if let ExprKind::Slice { recv: inner, from, to } = &mut recv.kind {
                let (window, writable) = self.range(inner, from, to)?;
                let Ty::Read(behind) = window else { unreachable!("a range is a `read` window") };
                if *behind != Ty::Str {
                    if let Some(diag) = self.readonly(inner) {
                        return Err(diag.note(format!("`{name}` changes the items of a range of it")));
                    }
                    if !writable {
                        return Err(Diag::error(inner.span, format!("`{name}` changes the items, but this is reached through a `read` borrow"))
                            .hint("take the value as `ref` instead of `read`"));
                    }
                    self.not_locked(inner, Access::Write)?;
                }
                recv.ty = if *behind == Ty::Str { Ty::Read(behind) } else { Ty::Ref(behind) };
            }
        }
        let (recv_ty, mut writable, is_place) = match &recv.kind {
            ExprKind::Slice { .. } if matches!(name, "sort" | "reverse") => (recv.ty.clone(), true, false),
            _ => match self.place(recv)? {
                Some((ty, writable)) => (ty, writable, true),
                None => (self.expr(recv, None)?, true, false),
            },
        };
        // Like field access, a method call looks through pointers and borrows.
        let mut borrowed = false;
        let mut target = &recv_ty;
        loop {
            target = match target {
                Ty::Read(inner) => {
                    (writable, borrowed) = (false, true);
                    inner
                }
                Ty::Ref(inner) => {
                    (writable, borrowed) = (true, true);
                    inner
                }
                Ty::Ptr(inner) => inner,
                _ => break,
            };
        }

        let structs = self.structs;
        if let Ty::Struct(owner) | Ty::Enum(owner) = target {
            if let Some(sig) = self.methods_of(owner).get(name) {
                if !sig.public && !self.inside(owner) {
                    return Err(self.not_public(name, owner, name_span));
                }
                let Some(this) = sig.this else {
                    return Err(Diag::error(name_span, format!("`{name}` does not take `this`, so it is called on the type"))
                        .hint(format!("`{owner}.{name}(...)`")));
                };
                match this {
                    This::Read => {}
                    This::Ref if !writable => {
                        if let Some(diag) = self.readonly(recv) {
                            return Err(diag.note(format!("`{name}` takes `ref this` and may change it")));
                        }
                        return Err(Diag::error(recv.span, format!("`{name}` takes `ref this`, but this value is reached through a `read` borrow"))
                            .hint("take the value as `ref` instead of `read`"));
                    }
                    This::Ref => self.not_locked(recv, Access::Write)?,
                    // A `Copyable` receiver is copied in, like an argument.
                    This::Value if target.is_copyable(self.copyable) => {}
                    This::Value if borrowed => {
                        return Err(Diag::error(recv.span, format!("`{name}` takes `this` by value and uses it up, but this value is only borrowed")));
                    }
                    This::Value if is_place => {
                        self.move_out(recv, &recv_ty)?;
                        recv.moved = true;
                    }
                    This::Value => {}
                }
                self.args(name, args, &sig.params, span)?;
                if is_place && this != This::Value {
                    self.lent_twice(Some((recv, this == This::Ref)), args)?;
                }
                *callee = Callee::Method { owner: owner.clone(), this };
                return Ok(sig.ret.clone());
            }
        }
        // A function value kept in a field: `this.test(order)`.
        if let Ty::Struct(owner) = target {
            let fields = &structs[owner].fields;
            if let Some(position) = fields.iter().position(|(field, _)| field == name) {
                if let Ty::Func(params, ret) = &fields[position].1 {
                    if !structs[owner].open[position] && !self.inside(owner) {
                        return Err(self.not_public(name, owner, name_span));
                    }
                    self.args(name, args, params, span)?;
                    *callee = Callee::FieldValue(position);
                    return Ok((**ret).clone());
                }
            }
        }
        match name {
            "take" => return self.take(recv, &recv_ty, writable, is_place, args, callee, span),
            "replace" => return self.replace(recv, &recv_ty, writable, is_place, args, callee, span),
            _ => {}
        }
        if matches!(target, Ty::List(_) | Ty::Map(..) | Ty::Set(_)) {
            return self.collection_method(recv, target, target, writable, name, name_span, args, span);
        }
        if matches!(target, Ty::Reader | Ty::Writer) {
            let target = target.clone();
            return self.stream_method(recv, &target, writable, borrowed, name, name_span, args, callee, span);
        }
        // An array and a slice have the methods of a list that only look, and the two that
        // change the order of the items without changing how many there are.
        if let Ty::Array(item, _) | Ty::Slice(item) = target {
            if !matches!(name, "length" | "get" | "indexOf" | "contains" | "sort" | "reverse") {
                let diag = Diag::error(name_span, format!("`{target}` has no method `{name}`"));
                return Err(match name {
                    "add" | "addAll" | "removeLast" | "clear" => diag.note(format!("`{target}` has a fixed number of items; a `List<{item}>` grows and shrinks")),
                    _ => match nearest(name, ["length", "get", "indexOf", "contains", "sort", "reverse"].into_iter()) {
                        Some(close) => diag.hint(format!("`{close}`")),
                        None => diag.note("methods for now: length, get, indexOf, contains, sort, reverse"),
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
            (Ty::Int(ty), "wrappingAdd" | "wrappingSubtract" | "wrappingMultiply") => {
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
                    return Err(Diag::error(recv.span, format!("`{name}` changes the text, but it is reached through a `read` borrow"))
                        .hint("take the text as `ref string` instead of `read string`"));
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
            // `x.pow(y)`, `y.atan2(x)`, `a.min(b)`: the two meet in one type, as with `+`.
            (Ty::Float(ty), "pow" | "atan2" | "min" | "max") => {
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
            // `big.tryToInt()`: `null` if it does not fit; `big.wrappingToInt()`: cut to size.
            (Ty::Int(_) | Ty::Float(_), _) if name.starts_with("tryTo") || name.starts_with("wrappingTo") => {
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
            (Ty::Str, "equalsIgnoreAsciiCase") => {
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
                    return Err(Diag::error(recv.span, "`next` moves the iterator on, and this one is reached through a `read` borrow"));
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
                Err(match &recv.kind {
                    ExprKind::Var(var) => diag.hint(format!("test it first: `if {var} is not null {{ ... }}`")),
                    _ => diag.hint("copy it to a variable and test that with `is not null`; fields are not narrowed"),
                })
            }
            _ => {
                let diag = Diag::error(name_span, format!("type `{target}` has no method `{name}`"));
                const FRACTIONAL: &[&str] = &[
                    "sqrt", "floor", "ceil", "round", "truncate", "exp", "log", "log2", "log10", "sin", "cos", "tan", "asin", "acos", "atan",
                    "atan2", "pow", "fusedMultiplyAdd",
                ];
                if matches!(target, Ty::Int(_)) && FRACTIONAL.contains(&name) {
                    let text = &self.src[recv.span.start..recv.span.end];
                    let given = if matches!(name, "atan2" | "pow" | "fusedMultiplyAdd") { "..." } else { "" };
                    return Err(diag.hint(format!("it is a method of `double`: `((double){text}).{name}({given})`")));
                }
                if matches!(target, Ty::Int(_)) && (ASCII_TESTS.contains(&name) || matches!(name, "toAsciiUpper" | "toAsciiLower")) {
                    return Err(diag.note("the ASCII methods are on `byte` and `char`"));
                }
                if matches!(target, Ty::Int(_)) && matches!(name, "isWhitespace" | "utf8Length") {
                    return Err(diag.note(format!("`{name}` is a method of `char`")));
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
        let changing = matches!(name, "add" | "addAll" | "clear" | "removeLast" | "reverse" | "remove" | "getOrAdd" | "sort");
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
        let can_order = item.is_some_and(|item| match item {
            Ty::Int(_) | Ty::Float(_) | Ty::Char | Ty::Str => true,
            Ty::Struct(name) | Ty::Enum(name) => self.comparable.contains(name),
            _ => false,
        });
        let compares = |item: &Ty, needs: &str| match item {
            _ if needs == "Equatable" && can_equal => Ok(()),
            _ if needs == "Comparable" && can_order => Ok(()),
            Ty::Struct(inner) | Ty::Enum(inner) => Err(Diag::error(name_span, format!("`{name}` compares the items, and `{inner}` is not `{needs}`"))
                .hint(format!("add it to the definition: `{inner} : {needs}`"))),
            Ty::List(_) | Ty::Set(_) | Ty::Map(..) if needs == "Comparable" => Err(Diag::error(name_span, format!("`{name}` puts the items in order, and a collection has no order of its own"))),
            other => Err(Diag::error(name_span, format!("`{name}` compares the items, and `{other}` is not `{needs}`"))
                .note(format!("`{needs}`: numbers{}, `char`, `string` and the types written with `: {needs}`", if needs == "Equatable" { ", `bool`" } else { "" }))),
        };
        let changes = || {
            if writable {
                Ok(())
            } else {
                let diag = Diag::error(recv.span, format!("`{name}` changes the {what}, but it is reached through a `read` borrow"));
                Err(match shown {
                    Ty::Slice(item) => diag.hint(format!("lend it as a `ref {item}[]`: `ref xs` or `ref xs[a..b]`")),
                    _ => diag.hint("take the value as `ref` instead of `read`"),
                })
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
                Ok(Ty::Void)
            }
            (Ty::List(item), "add") => {
                arity(1)?;
                changes()?;
                self.typed(&mut args[0], item)?;
                if self.holds(item) {
                    self.stores(recv, &args[0])?;
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
            // `null` where `xs[i]` would panic.
            (Ty::List(item), "get") => {
                arity(1)?;
                self.index(&mut args[0], "an index")?;
                Ok(Ty::Read(Box::new(nullable(item))))
            }
            (Ty::List(item), "removeLast") => {
                arity(0)?;
                changes()?;
                Ok(nullable(item))
            }
            (Ty::List(_), "reverse") => {
                arity(0)?;
                changes()?;
                Ok(Ty::Void)
            }
            (Ty::List(item), "indexOf") => {
                arity(1)?;
                compares(item, "Equatable")?;
                self.typed(&mut args[0], &self.key_param(item))?;
                Ok(nullable(&Ty::Int(IntTy::Nint)))
            }
            (Ty::List(item), "contains") => {
                arity(1)?;
                compares(item, "Equatable")?;
                self.typed(&mut args[0], &self.key_param(item))?;
                Ok(Ty::Bool)
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
                Ok(Ty::Bool)
            }
            (Ty::Set(item), "contains") => {
                arity(1)?;
                self.typed(&mut args[0], &self.key_param(item))?;
                Ok(Ty::Bool)
            }
            // `true` if the item was there.
            (Ty::Set(item), "remove") => {
                arity(1)?;
                changes()?;
                self.typed(&mut args[0], &self.key_param(item))?;
                Ok(Ty::Bool)
            }
            (Ty::Map(key, value), "add") => {
                arity(2)?;
                changes()?;
                self.typed(&mut args[0], key)?;
                self.typed(&mut args[1], value)?;
                if self.holds(value) {
                    self.stores(recv, &args[1])?;
                }
                Ok(Ty::Void)
            }
            (Ty::Map(key, value), "get") => {
                arity(1)?;
                self.typed(&mut args[0], &self.key_param(key))?;
                Ok(Ty::Read(Box::new(nullable(value))))
            }
            (Ty::Map(key, _), "contains") => {
                arity(1)?;
                self.typed(&mut args[0], &self.key_param(key))?;
                Ok(Ty::Bool)
            }
            (Ty::Map(key, value), "remove") => {
                arity(1)?;
                changes()?;
                self.typed(&mut args[0], &self.key_param(key))?;
                Ok(nullable(value))
            }
            (Ty::Map(key, value), "getOrAdd") => {
                arity(2)?;
                changes()?;
                self.typed(&mut args[0], &self.key_param(key))?;
                self.typed(&mut args[1], value)?;
                Ok(Ty::Ref(value.clone()))
            }
            _ => {
                let known: &[&str] = match target {
                    Ty::List(_) => &["add", "addAll", "get", "length", "indexOf", "contains", "sort", "removeLast", "reverse", "clear"],
                    Ty::Map(..) => &["add", "get", "getOrAdd", "contains", "remove", "length", "clear"],
                    _ => &["add", "contains", "remove", "length", "clear"],
                };
                // Names the same thing has in other languages.
                let other_name = match (target, name) {
                    (Ty::List(_), "extend" | "extendFromSlice" | "addRange" | "pushAll" | "appendAll" | "concat") => Some("addAll"),
                    (_, "push" | "append" | "insert" | "put" | "set") => Some("add"),
                    (_, "count" | "size" | "len" | "Count") => Some("length"),
                    (Ty::List(_), "pop") => Some("removeLast"),
                    (_, "containsKey" | "has" | "includes") => Some("contains"),
                    (Ty::List(_), "find") => Some("indexOf"),
                    (Ty::List(_), "at") => Some("get"),
                    _ => None,
                };
                let diag = Diag::error(name_span, format!("`{target}` has no method `{name}`"));
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
        name_span: Span,
        args: &mut [Expr],
        callee: &mut Callee,
        span: Span,
    ) -> Res<Ty> {
        let arity = |count: usize| match args.len() == count {
            true => Ok(()),
            false => Err(Diag::error(span, format!("`{name}` takes {count} argument(s), {} given", args.len()))),
        };
        let result = |ok: Ty| Ty::Result(Box::new(ok), Box::new(Ty::Enum(IO_ERROR.to_string())));
        let call = match (target, name) {
            (Ty::Reader, "readLine") => StreamCall::ReadLine,
            (Ty::Reader, "readText") => StreamCall::ReadText,
            (Ty::Reader, "lines") => StreamCall::Lines,
            (Ty::Reader, "readBytes") => StreamCall::ReadBytes,
            (Ty::Writer, "write") => StreamCall::Write,
            (Ty::Writer, "writeLine") => StreamCall::WriteLine,
            (Ty::Writer, "writeBytes") => StreamCall::WriteBytes,
            (Ty::Writer, "flush") => StreamCall::Flush,
            (Ty::Writer, "close") => StreamCall::Close,
            _ => {
                let known: &[&str] = match target {
                    Ty::Reader => &["readLine", "readText", "lines", "readBytes"],
                    _ => &["write", "writeLine", "writeBytes", "flush", "close"],
                };
                let diag = Diag::error(name_span, format!("`{target}` has no method `{name}`"));
                let diag = match nearest(name, known.iter().copied()) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag.note(format!("methods: {}", known.join(", "))),
                };
                return Err(match (target, name) {
                    (Ty::Reader, "close") => diag.note("a reader has nothing to lose: its file is closed when it ends"),
                    (Ty::Reader, "write" | "writeLine" | "writeBytes" | "flush") => diag.note("a `StreamReader` reads; `Stdout`, `Stderr`, `File.createText(path)` and `File.openAppend(path)` give a `StreamWriter`"),
                    (Ty::Writer, "readLine" | "readText" | "lines" | "readBytes") => diag.note("a `StreamWriter` writes; `Stdin` and `File.openText(path)` give a `StreamReader`"),
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
        if !writable {
            if let Some(diag) = self.readonly(recv) {
                return Err(diag.note(format!("`{name}` goes on from where the stream is")));
            }
            return Err(Diag::error(recv.span, format!("`{name}` goes on from where the stream is, but it is reached through a `read` borrow"))
                .hint(format!("take it as `ref {target}` instead of `read {target}`")));
        }
        let ty = match call {
            StreamCall::ReadLine => {
                self.args(name, args, &[Ty::Ref(Box::new(Ty::Str))], span)?;
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
            StreamCall::ReadBytes => {
                let lent_read = args.first().is_some_and(|arg| self.src[arg.span.start..arg.span.end].starts_with("read "));
                self.args(name, args, &[Ty::Ref(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))))], span).map_err(|diag| match lent_read {
                    true => diag.hint("`readBytes` writes into the buffer, so it is lent with `ref`"),
                    false => diag,
                })?;
                result(Ty::Int(IntTy::Nint))
            }
            StreamCall::WriteBytes => {
                self.args(name, args, &[Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))))], span)?;
                result(Ty::Void)
            }
            StreamCall::Write | StreamCall::WriteLine => {
                arity(1)?;
                let ty = self.operand(&mut args[0], None)?;
                self.printable(&ty, args[0].span)?;
                result(Ty::Void)
            }
            StreamCall::Flush | StreamCall::Close => {
                arity(0)?;
                result(Ty::Void)
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
            return Err(Diag::error(recv.span, "this is reached through a `read` borrow, so nothing can be taken out of it")
                .hint("take the value as `ref` instead of `read`"));
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
            return Err(Diag::error(span, format!("`replace` takes 1 argument(s), {} given", args.len())));
        }
        if !is_place {
            return Err(Diag::error(recv.span, "`replace` needs a variable or a field to put the new value in"));
        }
        if let Some(diag) = self.readonly(recv) {
            return Err(diag);
        }
        if !writable {
            return Err(Diag::error(recv.span, "this is reached through a `read` borrow and cannot be replaced")
                .hint("take the value as `ref` instead of `read`"));
        }
        self.not_locked(recv, Access::Write)?;
        let slot = recv_ty.strip_borrow().clone();
        self.typed(&mut args[0], &slot)?;
        if self.holds(&slot) {
            self.stores(recv, &args[0])?;
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
            StmtKind::If { cond, then, els } => {
                taken_in(cond, names);
                changed_in(then, names);
                if let Some(els) = els {
                    changed_in(els, names);
                }
            }
            StmtKind::While { cond, body } => {
                taken_in(cond, names);
                changed_in(body, names);
            }
            StmtKind::For { from, to, body, .. } => {
                taken_in(from, names);
                taken_in(to, names);
                changed_in(body, names);
            }
            StmtKind::ForEach { iter, body, .. } => {
                taken_in(iter, names);
                changed_in(body, names);
            }
            StmtKind::Return(Some(expr)) | StmtKind::Expr(expr) | StmtKind::Discard(expr) => taken_in(expr, names),
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
        }
    }
}

/// Collects the variables an expression empties with `x.take()`.
fn taken_in<'b>(expr: &'b Expr, names: &mut Vec<&'b str>) {
    match &expr.kind {
        ExprKind::Method { recv, name, args, .. } => {
            if let (ExprKind::Var(var), "take") = (&recv.kind, name.as_str()) {
                names.push(var);
            }
            taken_in(recv, names);
            args.iter().for_each(|arg| taken_in(arg, names));
        }
        ExprKind::Call { args, .. } => args.iter().for_each(|arg| taken_in(arg, names)),
        ExprKind::StructLit { fields, .. } => fields.iter().for_each(|field| taken_in(&field.value, names)),
        ExprKind::Interp(parts) => {
            for part in parts {
                if let InterpPart::Expr(inner, _) = part {
                    taken_in(inner, names);
                }
            }
        }
        ExprKind::Collection { args, items, .. } => {
            let given = items.iter().flatten().flat_map(|item| &item.values);
            args.iter().chain(given).for_each(|value| taken_in(value, names));
        }
        ExprKind::Array { args, items, .. } => args.iter().chain(items.iter().flatten()).for_each(|value| taken_in(value, names)),
        ExprKind::Binary(_, left, right) | ExprKind::Coalesce { left, right, .. } | ExprKind::Index { recv: left, index: right, .. } => {
            taken_in(left, names);
            taken_in(right, names);
        }
        ExprKind::Slice { recv, from, to } => {
            taken_in(recv, names);
            for end in [from, to].into_iter().flatten() {
                taken_in(end, names);
            }
        }
        ExprKind::Variant { args, .. } => args.iter().for_each(|arg| taken_in(arg, names)),
        ExprKind::Match { subject, arms, .. } => {
            taken_in(subject, names);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    taken_in(guard, names);
                }
                changed_in(&arm.body, names);
                if let Some(value) = &arm.value {
                    taken_in(value, names);
                }
            }
        }
        ExprKind::Field { recv: inner, .. }
        | ExprKind::IsVariant { expr: inner, .. }
        | ExprKind::Unbox(inner)
        | ExprKind::AsSlice(inner)
        | ExprKind::Try { expr: inner, .. }
        | ExprKind::Return(Some(inner))
        | ExprKind::Unary(_, inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::New(inner)
        | ExprKind::Borrow { expr: inner, .. }
        | ExprKind::IsNull { expr: inner, .. }
        | ExprKind::Widen(_, inner)
        | ExprKind::Load(inner) => taken_in(inner, names),
        ExprKind::Int(..)
        | ExprKind::Float(..)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Char(_)
        | ExprKind::Null
        | ExprKind::Var(_)
        | ExprKind::FuncRef(_)
        | ExprKind::Constant { .. }
        | ExprKind::Return(None) => {}
    }
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
        StmtKind::Break | StmtKind::Continue => in_loop,
        StmtKind::If { then, els: Some(els), .. } => leaves(then, in_loop) && leaves(els, in_loop),
        StmtKind::While { cond, body } => matches!(cond.kind, ExprKind::Bool(true)) && !has_break(body),
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

/// Whether a `break` leaves this loop (those of inner loops do not count).
fn has_break(block: &Block) -> bool {
    block.iter().any(|stmt| match &stmt.kind {
        StmtKind::Break => true,
        StmtKind::If { then, els, .. } => has_break(then) || els.as_ref().is_some_and(has_break),
        StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) | StmtKind::Discard(expr) => breaks_in(expr),
        StmtKind::Let { init: expr, .. } | StmtKind::Assign { value: expr, .. } => breaks_in(expr),
        _ => false,
    })
}

/// Whether a `match` here has an arm with a `break`.
fn breaks_in(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Match { arms, .. } => {
            arms.iter().any(|arm| has_break(&arm.body) || arm.value.as_ref().is_some_and(breaks_in))
        }
        _ => false,
    }
}
