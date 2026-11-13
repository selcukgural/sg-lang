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

use crate::ast::*;
use crate::diag::{nearest, Diag, Span};
use crate::types::{IntTy, Ty};

type Res<T> = Result<T, Diag>;

const TYPES_NOT_YET: &[&str] =
    &["sbyte", "short", "ushort", "uint", "ulong", "nuint", "float", "double", "char"];

const INTERFACES_NOT_YET: &[&str] = &["Clonable", "Disposable", "Closable", "Iterator"];

const INTERFACES: &[&str] = &["Copyable", "Formattable", "Equatable", "Hashable", "Comparable"];

const BUILTINS: &[&str] = &["print", "assert", "panic", "Ok", "Error"];

/// Types the language brings along; a program cannot define another one under these names.
const BUILTIN_TYPES: &[&str] = &["Result", "List", "HashMap", "HashSet", "Func", "TextError"];

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
        if written == TEXT_ERROR {
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

/// Whether a value of this type is a borrow or carries one inside.
fn lends(ty: &Ty, names: &Names) -> bool {
    match ty {
        Ty::Read(_) | Ty::Ref(_) => true,
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
        let mut methods: HashMap<String, FuncSig> = HashMap::new();
        for method in &decl.methods {
            if method.name == "replace" {
                return Err(Diag::error(method.name_span, "`replace` is built in for every type and cannot be defined"));
            }
            if fields.iter().any(|(name, _)| *name == method.name) {
                return Err(Diag::error(
                    method.name_span,
                    format!("`{}` is already a field of `{}`", method.name, decl.name),
                ));
            }
            if methods.insert(method.name.clone(), func_sig(method, &names, Some(&decl.name))?).is_some() {
                return Err(Diag::error(method.name_span, format!("method `{}` is defined twice", method.name)));
            }
            // The body of `Formattable`, when the struct writes its own.
            if method.name == "format" && formattable.contains(&decl.name) {
                let sig = &methods["format"];
                let fits = sig.this == Some(This::Read) && sig.ret == Ty::Void && sig.params == [Ty::Ref(Box::new(Ty::Formatter))];
                if !fits {
                    return Err(Diag::error(method.name_span, "this is not the `format` that `Formattable` asks for")
                        .hint("`void format(read this, ref Formatter f)`"));
                }
            }
        }
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
        enums.insert(decl.name.clone(), EnumInfo { variants });
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
    for decl in &prog.structs {
        let info = &structs[&decl.name];
        let fields = || decl.fields.iter().zip(&info.fields);
        if copyable.contains(&decl.name) {
            all_copyable(&decl.name, fields(), &copyable)?;
        }
        if formattable.contains(&decl.name) && !info.methods.contains_key("format") {
            all_formattable(&decl.name, fields(), &formattable)?;
        }
        let name = &decl.name;
        let own = |method: &str| decl.methods.iter().find(|func| func.name == method);
        // `equals` and `compare` look at this value and another one of the same type.
        let takes_other = |method: &str| {
            let sig = &info.methods[method];
            sig.this == Some(This::Read) && sig.params == [Ty::Read(Box::new(Ty::Struct(name.clone())))]
        };
        if equatable.contains(name) {
            match own("equals") {
                Some(func) if !takes_other("equals") || info.methods["equals"].ret != Ty::Bool => {
                    return Err(Diag::error(func.name_span, "this is not the `equals` that `Equatable` asks for")
                        .hint(format!("`bool equals(read this, read {name} other)`")));
                }
                Some(_) => {}
                None => all_have(name, fields(), "Equatable", &equatable)?,
            }
        }
        if names.keys.contains(name) {
            if !equatable.contains(name) {
                return Err(Diag::error(decl.name_span, format!("`{name}` is `Hashable`, so it must be `Equatable` too: a key is found by its hash and then compared"))
                    .hint(format!("`{name} : Equatable, Hashable`")));
            }
            if let Some(func) = own("hash").or_else(|| own("equals")) {
                return Err(Diag::error(func.name_span, "a `Hashable` type with its own `equals` or `hash` is not in the interpreter yet")
                    .note("leave both out: they are made from the fields"));
            }
            all_have(name, fields(), "Hashable", &names.keys)?;
        }
        if comparable.contains(name) {
            match own("compare") {
                None => {
                    return Err(Diag::error(decl.name_span, format!("`{name}` is `Comparable`, so it needs a `compare` method"))
                        .hint(format!("`int compare(read this, read {name} other)`: negative if this one comes first, 0 if they are equal")));
                }
                Some(func) if !takes_other("compare") || info.methods["compare"].ret != Ty::Int(IntTy::Int) => {
                    return Err(Diag::error(func.name_span, "this is not the `compare` that `Comparable` asks for")
                        .hint(format!("`int compare(read this, read {name} other)`")));
                }
                Some(_) => {}
            }
        }
    }
    for decl in &prog.enums {
        let fields = || {
            let written = decl.variants.iter().flat_map(|variant| &variant.fields);
            written.zip(enums[&decl.name].variants.iter().flat_map(|(_, fields)| fields))
        };
        if copyable.contains(&decl.name) {
            all_copyable(&decl.name, fields(), &copyable)?;
        }
        if formattable.contains(&decl.name) {
            all_formattable(&decl.name, fields(), &formattable)?;
        }
        let name = &decl.name;
        if equatable.contains(name) {
            all_have(name, fields(), "Equatable", &equatable)?;
        }
        if names.keys.contains(name) {
            if !equatable.contains(name) {
                return Err(Diag::error(decl.name_span, format!("`{name}` is `Hashable`, so it must be `Equatable` too: a key is found by its hash and then compared"))
                    .hint(format!("`{name} : Equatable, Hashable`")));
            }
            all_have(name, fields(), "Hashable", &names.keys)?;
        }
        if comparable.contains(name) {
            return Err(Diag::error(decl.name_span, format!("`{name}` is `Comparable`, so it needs a `compare` method, and methods on enums are not in the interpreter yet")));
        }
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
            if !ret_ok || !main.params.is_empty() {
                return Err(Diag::error(main.name_span, "`main` is written `void main()` or `Result<void, E> main()`"));
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
            ret: sig.ret.clone(),
            sources: sig.sources.clone(),
            func_name: func.name.clone(),
            in_test: false,
            owner,
            module: names.module(func.name_span),
            src,
        };
        if let (Some(this), Some(owner)) = (func.this, owner) {
            let ty = Ty::Struct(owner.to_string());
            let ty = match this {
                This::Value => ty,
                This::Read => Ty::Read(Box::new(ty)),
                This::Ref => Ty::Ref(Box::new(ty)),
            };
            checker.declare("this", ty, func.name_span)?;
            checker.find("this").expect("just declared").param = true;
        }
        for (param, ty) in func.params.iter().zip(&sig.params) {
            checker.declare(&param.name, ty.clone(), param.span)?;
            checker.find(&param.name).expect("just declared").param = true;
        }
        checker.block(&mut func.body, None)?;
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
            ret: Ty::Void,
            sources: Vec::new(),
            func_name: "test".to_string(),
            in_test: true,
            owner: None,
            module: names.module(test_span),
            src,
        };
        checker.block(&mut prog.tests[position].body, None)?;
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
            let diag = if TYPES_NOT_YET.contains(&ty.name.as_str()) {
                Diag::error(ty.span, format!("type `{}` is not in the interpreter yet", ty.name))
            } else {
                Diag::error(ty.span, format!("there is no type named `{}`", ty.name))
            };
            if let Some(hint) = names.use_hint(&ty.name, ty.span) {
                return Err(diag.hint(hint));
            }
            let known = names.kinds.keys().map(String::as_str).chain(["byte", "int", "long", "nint", "bool", "string"]);
            return Err(match nearest(&ty.name, known) {
                Some(close) => diag.hint(format!("`{close}`")),
                None => diag.note("built-in types for now: byte, int, long, nint, bool, string"),
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
fn known_int(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::Int(value, _) => Some(*value),
        ExprKind::Widen(_, inner) => known_int(inner),
        _ => None,
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
    let diag = Diag::error(span, format!("`{ty}` cannot be {what}: it is not `Equatable` and `Hashable`"));
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => diag.hint(format!("add them to the definition: `{name} : Equatable, Hashable`")),
        Ty::List(_) | Ty::Set(_) | Ty::Map(..) => diag.note("a collection can be compared with `==`, but it is not `Hashable`"),
        _ => diag.note("integers, `bool`, `string` and the types written with `: Equatable, Hashable` are"),
    }
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
fn has(ty: &Ty, declared: &HashSet<String>, collections: bool) -> bool {
    match ty {
        Ty::Int(_) | Ty::Bool | Ty::Str => true,
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
        Ty::Int(_) | Ty::Bool | Ty::Str | Ty::Void => true,
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
}

/// What has been moved out of a variable.
#[derive(Clone, Default)]
struct Moved {
    /// Where the whole value was moved out, if it was.
    whole: Option<Span>,
    /// Fields that were moved out on their own: path (`next`, `first.name`) and where.
    parts: Vec<(String, Span)>,
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
    /// Structs written with `: Comparable`.
    comparable: &'a HashSet<String>,
    structs: &'a HashMap<String, StructInfo>,
    enums: &'a HashMap<String, EnumInfo>,
    sigs: &'a HashMap<String, FuncSig>,
    scopes: Vec<Scope>,
    /// The collections the enclosing `for` loops are going over.
    locks: Vec<Lock>,
    borrows: Borrows,
    loops: usize,
    ret: Ty,
    /// Where a returned borrow may come from.
    sources: Vec<Source>,
    func_name: String,
    /// A `test` block is being checked: `try` passes any error on, and the test fails with it.
    in_test: bool,
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
        scope.vars.push(Var { id, decl_loops, loans: Vec::new(), param: false, name: name.to_string(), unopened: None, declared: ty.clone(), current: ty, moved, assigned: 0 });
        Ok(())
    }

    /// A new local variable; `this` is not a name for one.
    fn declare_local(&mut self, name: &str, ty: Ty, span: Span) -> Res<()> {
        if name == "this" {
            return Err(Diag::error(span, "`this` is the receiver of a method and cannot name a variable"));
        }
        self.declare(name, ty, span)
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
        let is_field = self.owner.is_some_and(|owner| self.structs[owner].fields.iter().any(|(field, _)| field == name));
        if is_field && has_this {
            return diag.hint(format!("a field is reached through `this`: `this.{name}`"));
        }
        if let Some(hint) = self.names.use_hint(name, span) {
            return diag.hint(hint);
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
        let at = Point { pos: span.start, branches: self.borrows.branches.clone(), loops: self.borrows.loops.clone() };
        self.borrows.uses.push((id, at));
        self.borrows.pos = self.borrows.pos.max(span.start);
        Ok(self.find(name).expect("just found"))
    }

    // ---- borrows ----

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
                let sig = match callee {
                    Callee::Method { owner, .. } | Callee::Static { owner, .. } => self.structs[owner.as_str()].methods.get(name.as_str()),
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
                    Callee::Static { owner, name } => self.structs[owner.as_str()].methods.get(name.as_str()),
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
        let at = Point { pos: expr.span.start, branches: self.borrows.branches.clone(), loops: self.borrows.loops.clone() };
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
            let used_later = self.borrows.uses.iter().any(|(var, at)| {
                if *var != clash.holder {
                    return false;
                }
                // Not reached from here: the other branch of an `if` or a `match` around both.
                let other_branch = at.branches.iter().any(|(node, branch)| clash.at.branches.iter().any(|(n, b)| n == node && b != branch));
                let after = at.pos > clash.at.pos && !other_branch;
                // A loop comes round again, unless the variable is new in every round.
                let next_round = clash.at.loops.iter().any(|l| !clash.holder_loops.contains(l) && at.loops.contains(l));
                after || next_round
            });
            if !used_later {
                continue;
            }
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
            let diag = diag.related(clash.loan.span, format!("`{holder}` borrowed it here"));
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

    /// Which variable a name means right now: scope and position in the scope.
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
    fn pop_scope(&mut self) -> Res<()> {
        let scope = self.scopes.pop().expect("an open scope");
        // A variable outside that still holds a borrow of an owner that ends here.
        let at = Point { pos: self.borrows.pos, branches: self.borrows.branches.clone(), loops: self.borrows.loops.clone() };
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

    fn block(&mut self, block: &mut Block, narrow: Option<&str>) -> Res<()> {
        self.block_with(block, narrow, Vec::new())
    }

    /// A block that starts with the names of an `is` pattern in scope.
    fn block_with(&mut self, block: &mut Block, narrow: Option<&str>, binds: Binds) -> Res<()> {
        self.scopes.push(Scope::default());
        if let Some(name) = narrow {
            self.narrow(name);
        }
        self.declare_binds(binds)?;
        for stmt in block.iter_mut() {
            self.stmt(stmt)?;
        }
        self.pop_scope()
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
                if op.is_some() && !matches!(target_ty, Ty::Int(_)) {
                    return Err(Diag::error(
                        stmt_span,
                        format!("`{}=` needs an integer on the left, found `{target_ty}`", op.expect("checked").symbol()),
                    ));
                }
                self.typed(value, &target_ty)?;
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
                let binds = self.condition(cond)?;
                let test = narrowing(cond);
                let not_null = |wanted: bool| test.as_ref().filter(|(_, negated)| *negated == wanted).map(|(name, _)| name.as_str());

                let before = self.moves();
                let node = self.node();
                self.borrows.branches.push((node, 0));
                self.block_with(then, not_null(true), binds)?;
                self.borrows.branches.pop();
                let then_leaves = leaves(then, true);
                let after_then = self.moves();
                self.set_moves(&before);
                let mut else_leaves = false;
                if let Some(els) = els {
                    self.borrows.branches.push((node, 1));
                    self.block(els, not_null(false))?;
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
                if then_leaves {
                    if let Some(name) = not_null(false) {
                        self.narrow(name);
                    }
                }
                if else_leaves {
                    if let Some(name) = not_null(true) {
                        self.narrow(name);
                    }
                }
            }
            StmtKind::While { cond, body } => {
                self.forget_narrowing_of_assigned(body, Some(cond));
                let node = self.node();
                self.borrows.loops.push(node);
                let binds = self.condition(cond)?;
                let test = narrowing(cond);
                let narrow = test.as_ref().filter(|(_, negated)| *negated).map(|(name, _)| name.as_str());
                let result = self.loop_body(body, narrow, binds);
                self.borrows.loops.pop();
                result?;
            }
            StmtKind::For { var, var_span, from, to, body } => {
                self.forget_narrowing_of_assigned(body, None);
                let from_ty = self.int_operand(from, None, "the start of the range")?;
                let to_ty = self.int_operand(to, Some(from_ty), "the end of the range")?;
                let ty = Ty::Int(from_ty.unify(to_ty));
                self.coerce(from, Ty::Int(from_ty), &ty)?;
                self.coerce(to, Ty::Int(to_ty), &ty)?;
                let node = self.node();
                self.borrows.loops.push(node);
                self.scopes.push(Scope::default());
                self.declare_local(var, ty, *var_span)?;
                let result = self.loop_body(body, None, Vec::new());
                self.borrows.loops.pop();
                result?;
                self.pop_scope()?;
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
                        return Err(Diag::error(iter.span, "`for` does not go over a `string`")
                            .hint("the bytes: `for i in 0..text.length()` and `text[i]`")
                            .note("going over the characters (`text.chars()`) is not in the interpreter yet"));
                    }
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
                let node = self.node();
                self.borrows.loops.push(node);
                self.borrows.binds.clear();
                let result = self.loop_body(body, None, vec![(var.clone(), item, *var_span)]);
                self.borrows.loops.pop();
                if locked {
                    self.locks.pop();
                }
                result?;
            }
            StmtKind::Return(value) => self.returned(value.as_mut(), stmt_span)?,
            StmtKind::Break | StmtKind::Continue => {
                if self.loops == 0 {
                    let word = if matches!(stmt.kind, StmtKind::Break) { "break" } else { "continue" };
                    return Err(Diag::error(stmt_span, format!("`{word}` can only be written inside a loop")));
                }
            }
            StmtKind::Expr(expr) => {
                self.unused(expr)?;
            }
        }
        Ok(())
    }

    /// `return` or `return value`, as a statement or on the right side of `??`.
    fn returned(&mut self, value: Option<&mut Expr>, span: Span) -> Res<()> {
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

    /// The condition of an `if` or `while`. `x is Circle(r)` as the whole condition brings
    /// its names into the block.
    fn condition(&mut self, cond: &mut Expr) -> Res<Binds> {
        if matches!(cond.kind, ExprKind::IsVariant { .. }) {
            return self.is_variant(cond, true);
        }
        self.typed(cond, &Ty::Bool)?;
        Ok(Vec::new())
    }

    /// A value moved inside a loop is gone when the next iteration starts.
    fn loop_body(&mut self, body: &mut Block, narrow: Option<&str>, binds: Binds) -> Res<()> {
        let before = self.moves();
        self.loops += 1;
        self.block_with(body, narrow, binds)?;
        self.loops -= 1;
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
                wrap(expr, |inner| ExprKind::Widen(*to, inner));
                expr.ty = Ty::Int(*to);
                Ok(())
            }
            (Ty::Int(_), Ty::Int(to)) => {
                mismatch(Some(format!("convert with `({})`; it panics if the value does not fit", to.name())))
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
        // `Shape.Empty` is a value, not a field.
        if let ExprKind::Field { recv, .. } = &expr.kind {
            if self.enum_named(recv).is_some() {
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
                        if let Some(at) = known_int(index).filter(|at| *at < 0 || *at >= *length as i64) {
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
            ExprKind::Field { recv, name, name_span, .. } => {
                let owner = self.enum_named(recv).expect("any other field is a place");
                let (index, ty) = self.variant(owner, name, *name_span, None, span)?;
                expr.kind = ExprKind::Variant { owner: owner.to_string(), index, args: Vec::new() };
                return Ok(ty);
            }
            ExprKind::Method { recv, name, name_span, args, .. } => {
                if let Some(owner) = self.enum_named(recv) {
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

        match &mut expr.kind {
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
                    Some(wanted) if wanted.fits(*value as i128) => wanted,
                    Some(wanted) => {
                        return Err(Diag::error(span, format!("{value} does not fit in {}", wanted.name())));
                    }
                    None if IntTy::Int.fits(*value as i128) => IntTy::Int,
                    None => IntTy::Long,
                };
                Ok(Ty::Int(*ty))
            }
            ExprKind::Bool(_) => Ok(Ty::Bool),
            ExprKind::Str(_) => Ok(Ty::Str),
            // A character is a `byte`; from there it widens like any other integer.
            ExprKind::Char(value) => {
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
                    if let InterpPart::Expr(inner) = part {
                        let ty = self.operand(inner, None)?;
                        self.printable(&ty, inner.span)?;
                    }
                }
                Ok(Ty::Fmt)
            }
            ExprKind::Unary(UnOp::Neg, operand) => {
                let like = expected.and_then(|ty| match ty {
                    Ty::Int(ty) => Some(*ty),
                    _ => None,
                });
                let found = self.operand(operand, like.map(Ty::Int).as_ref())?;
                match found.strip_borrow() {
                    Ty::Int(IntTy::Byte) => {
                        let text = &self.src[operand.span.start..operand.span.end];
                        Err(Diag::error(span, "operator `-` is not defined for `byte`: a `byte` is never negative")
                            .hint(format!("convert first: `-(int){text}`")))
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
            ExprKind::Binary(BinOp::And | BinOp::Or, left, right) => {
                self.typed(left, &Ty::Bool)?;
                self.typed(right, &Ty::Bool)?;
                Ok(Ty::Bool)
            }
            ExprKind::Binary(op, left, right) => self.binary(*op, left, right, expected, span),
            ExprKind::Cast { ty, expr: operand } => {
                let found = self.operand(operand, None)?;
                match found.strip_borrow() {
                    Ty::Int(from) => {
                        let from = Ty::Int(*from);
                        self.coerce(operand, found, &from)?;
                        // A value that is known here and does not fit is not left to run time.
                        if let ExprKind::Int(value, _) = operand.kind {
                            if !ty.fits(value as i128) {
                                return Err(Diag::error(span, format!("{value} does not fit in {}", ty.name())));
                            }
                        }
                        Ok(Ty::Int(*ty))
                    }
                    other => Err(Diag::error(span, format!("a `{other}` value cannot be converted to `{}`", ty.name()))),
                }
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
                self.is_variant(expr, false)?;
                Ok(Ty::Bool)
            }
            ExprKind::Match { subject, arms, borrowed } => self.match_expr(subject, arms, borrowed, expected, false),
            ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. } => unreachable!("places are handled above"),
            ExprKind::Widen(..) | ExprKind::Load(_) | ExprKind::Unbox(_) | ExprKind::AsSlice(_) | ExprKind::Variant { .. } | ExprKind::FuncRef(_) => {
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
                        _ => diag.note("patterns for text are not in the interpreter yet; compare with `==`"),
                    });
                }
            };
        }
    }

    /// Resolves a pattern against the enum and gives the names it declares. With `mode` the
    /// names borrow from the value; without, they take what the value holds.
    fn pattern_binds(&mut self, pattern: &mut Pattern, of: &Variants, mode: Option<Mode>, subject: &Ty) -> Res<Binds> {
        let span = pattern.span;
        let (name, args, index) = match &mut pattern.kind {
            PatternKind::Wildcard => return Ok(Vec::new()),
            PatternKind::Bind(name) => return Ok(vec![(name.clone(), subject.clone(), span)]),
            PatternKind::Int(_) | PatternKind::Char(_) => {
                return Err(Diag::error(span, format!("`{}` is an enum; a pattern here names one of its variants", of.owner)));
            }
            PatternKind::Variant { name, args, index } => (name, args, index),
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
                return Ok(Vec::new());
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
        let mut binds = Vec::new();
        for ((arg, arg_span), (_, ty)) in args.iter().zip(fields) {
            let Some(arg) = arg else { continue };
            let ty = match mode {
                None => ty.clone(),
                Some(Mode::Read) => Ty::Read(Box::new(ty.clone())),
                Some(Mode::Ref) => Ty::Ref(Box::new(ty.clone())),
            };
            binds.push((arg.clone(), ty, *arg_span));
        }
        Ok(binds)
    }

    /// A pattern in a `match` on an integer: a number or a character that fits the type,
    /// `_`, or `var name`.
    fn int_pattern(&mut self, pattern: &mut Pattern, ty: IntTy, subject: &Ty) -> Res<Binds> {
        let span = pattern.span;
        let value = match &pattern.kind {
            PatternKind::Wildcard => return Ok(Vec::new()),
            PatternKind::Bind(name) => return Ok(vec![(name.clone(), subject.clone(), span)]),
            PatternKind::Int(value) => *value,
            PatternKind::Char(value) => byte_of(*value, span)?,
            PatternKind::Variant { name, args, .. } => {
                let diag = Diag::error(span, format!("this `match` looks at a `{}`, and `{name}` is not a number or a character", ty.name()));
                return Err(match args {
                    None => diag.hint(format!("to take any value under this name, write `var {name}`")),
                    Some(_) => diag,
                });
            }
        };
        if !ty.fits(value as i128) {
            return Err(Diag::error(span, format!("{value} does not fit in {}, so this arm can never run", ty.name())));
        }
        pattern.kind = PatternKind::Int(value);
        Ok(Vec::new())
    }

    /// `x is Circle(r)`: looks at the value without taking it, so the names are borrows.
    /// They are in scope only when the test is the whole condition of an `if` or `while`.
    fn is_variant(&mut self, expr: &mut Expr, names_ok: bool) -> Res<Binds> {
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
        if !names_ok {
            return Err(Diag::error(pattern.span, "the names in this pattern exist only when `is` is the whole condition of an `if` or `while`")
                .hint("write `_` for each value, or use `match`"));
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
        // On an integer the arms are numbers; on anything else, the variants of an enum.
        let number = match subject_ty.strip_borrow() {
            Ty::Int(ty) => Some(*ty),
            _ => None,
        };
        let (of, mode) = match number {
            Some(ty) => (Variants { owner: ty.name().to_string(), list: Vec::new() }, None),
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
        let mut handled: Vec<i64> = Vec::new();
        let mut handled_all = false;
        let mut result: Option<Ty> = expected.cloned();
        let before = self.moves();
        let mut after: Option<Moves> = None;
        for (arm_index, arm) in arms.iter_mut().enumerate() {
            self.set_moves(&before);
            self.scopes.push(Scope::default());
            self.borrows.branches.push((node, arm_index));
            let binds = match number {
                Some(ty) => self.int_pattern(&mut arm.pattern, ty, &subject_ty)?,
                None => self.pattern_binds(&mut arm.pattern, &of, mode, &subject_ty)?,
            };
            let position = match &arm.pattern.kind {
                PatternKind::Variant { index, .. } => Some(*index as i64),
                PatternKind::Int(value) => Some(*value),
                _ => None,
            };
            if handled_all {
                return Err(Diag::error(arm.pattern.span, "this arm can never run: an arm above takes every value"));
            }
            if let Some(position) = position.filter(|position| handled.contains(position)) {
                let what = match number {
                    Some(_) => &self.src[arm.pattern.span.start..arm.pattern.span.end],
                    None => variants[position as usize].0.as_str(),
                };
                return Err(Diag::error(arm.pattern.span, format!("this arm can never run: `{what}` is already handled above")));
            }
            self.borrows.binds = subject_loans.clone();
            self.declare_binds(binds)?;
            match &mut arm.guard {
                Some(guard) => {
                    let unmoved = self.moves();
                    self.typed(guard, &Ty::Bool)?;
                    self.only_looked(&unmoved)?;
                }
                // Only an arm without a condition settles its variant.
                None => match position {
                    Some(position) => handled.push(position),
                    None => handled_all = true,
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
            self.pop_scope()?;
            self.borrows.branches.pop();
            // An arm that leaves does not reach the code after the `match`.
            if !leaves {
                if let Some(other) = &after {
                    self.merge_moves(other);
                }
                after = Some(self.moves());
            }
        }

        if !handled_all && number.is_some() {
            return Err(Diag::error(subject.span, format!("this `match` does not handle every `{owner}` value"))
                .hint("add `_ => ...` for everything else"));
        }
        if !handled_all {
            let missing: Vec<String> = (0..variants.len())
                .filter(|position| !handled.contains(&(*position as i64)))
                .map(|position| format!("`{}`", variants[position].0))
                .collect();
            if !missing.is_empty() {
                let diag = Diag::error(subject.span, format!("this `match` does not handle {}", missing.join(", ")))
                    .hint("add an arm for each, or `_ => ...` for everything else");
                let guarded = arms.iter().any(|arm| {
                    arm.guard.is_some() && matches!(&arm.pattern.kind, PatternKind::Variant { index, .. } if !handled.contains(&(*index as i64)))
                });
                return Err(match guarded {
                    true => diag.note("an arm with `when` does not settle its variant: the condition may be false"),
                    false => diag.note(format!("every variant of `{owner}` needs an arm")),
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
        let wanted = if op.is_comparison() { None } else { expected.filter(|ty| matches!(ty, Ty::Int(_))) };
        let left_ty = self.operand(left, wanted)?;
        let like = match left_ty.strip_borrow() {
            Ty::Int(_) => Some(left_ty.strip_borrow().clone()),
            _ => None,
        };
        let right_ty = self.operand(right, like.as_ref().or(wanted))?;

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
            (Ty::Int(a), Ty::Int(b)) => {
                let common = Ty::Int(a.unify(*b));
                self.coerce(left, left_ty.clone(), &common)?;
                self.coerce(right, right_ty.clone(), &common)?;
                Ok(if op.is_comparison() { Ty::Bool } else { common })
            }
            (Ty::Str, Ty::Str) if op.is_comparison() => Ok(Ty::Bool),
            (Ty::Bool, Ty::Bool) if matches!(op, BinOp::Eq | BinOp::Ne) => Ok(Ty::Bool),
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

    /// What `print`, `$"..."` and `f.write` accept: numbers, `bool`, text, a struct or an enum
    /// that is `Formattable`, and a collection of such values; also behind a pointer.
    fn printable(&self, ty: &Ty, span: Span) -> Res<()> {
        let mut inner = ty.strip_borrow();
        while let Ty::Ptr(behind) = inner {
            inner = behind.strip_borrow();
        }
        match inner {
            Ty::Int(_) | Ty::Bool | Ty::Str | Ty::Fmt => Ok(()),
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
            "print" => {
                arity(1)?;
                let ty = self.operand(&mut args[0], None)?;
                self.printable(&ty, args[0].span)?;
                Ok(Ty::Void)
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
        let structs = self.structs;
        let methods = &structs[owner].methods;
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

    fn method(
        &mut self,
        recv: &mut Expr,
        name: &str,
        name_span: Span,
        args: &mut [Expr],
        callee: &mut Callee,
        span: Span,
    ) -> Res<Ty> {
        // A type name in front of the dot: `Stack.create()`.
        if let ExprKind::Var(owner) = &recv.kind {
            if self.find(owner).is_none() && owner != "this" {
                if self.structs.contains_key(owner.as_str()) {
                    return self.static_call(owner, name, name_span, args, callee, span);
                }
                // `string.join(", ", read names)`
                // `string.fromUtf8(bytes)`: the bytes looked at as text, if they are text.
                if owner == "string" && name == "fromUtf8" {
                    let bytes = Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
                    self.args(name, args, &[bytes], span)?;
                    *callee = Callee::FromUtf8;
                    return Ok(Ty::Result(Box::new(Ty::Read(Box::new(Ty::Str))), Box::new(Ty::Struct(TEXT_ERROR.to_string()))));
                }
                if owner == "string" {
                    if name != "join" {
                        return Err(Diag::error(name_span, format!("`string` has no function `{name}`")).note("functions for now: join, fromUtf8"));
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

        let (recv_ty, mut writable, is_place) = match self.place(recv)? {
            Some((ty, writable)) => (ty, writable, true),
            None => (self.expr(recv, None)?, true, false),
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
        if let Ty::Struct(owner) = target {
            if let Some(sig) = structs[owner].methods.get(name) {
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
            return self.collection_method(recv, target, writable, name, name_span, args, span);
        }
        // An array and a slice have the methods of a list that only look.
        if let Ty::Array(item, _) | Ty::Slice(item) = target {
            if !matches!(name, "length" | "get" | "indexOf" | "contains") {
                let diag = Diag::error(name_span, format!("`{target}` has no method `{name}`"));
                return Err(match name {
                    "add" | "removeLast" | "clear" => diag.note(format!("`{target}` has a fixed number of items; a `List<{item}>` grows and shrinks")),
                    _ => diag.note("methods for now: length, get, indexOf, contains"),
                });
            }
            return self.collection_method(recv, &Ty::List(item.clone()), writable, name, name_span, args, span);
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
        if matches!(target, Ty::Int(_)) {
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
                let common = Ty::Int(ty.unify(other));
                self.coerce(recv, recv_ty.clone(), &common)?;
                self.coerce(&mut args[0], Ty::Int(other), &common)?;
                Ok(common)
            }
            (Ty::Int(ty), "tryAdd" | "trySubtract" | "tryMultiply" | "tryDivide") => {
                arity(1)?;
                let other = self.int_operand(&mut args[0], Some(*ty), "the argument")?;
                let common = Ty::Int(ty.unify(other));
                self.coerce(recv, recv_ty.clone(), &common)?;
                self.coerce(&mut args[0], Ty::Int(other), &common)?;
                Ok(Ty::Nullable(Box::new(common)))
            }
            (Ty::Str, "clone") => {
                arity(0)?;
                Ok(Ty::Str)
            }
            (Ty::Int(_) | Ty::Bool | Ty::Fmt, "toString") => {
                arity(0)?;
                Ok(Ty::Str)
            }
            (Ty::Str, "length") => {
                arity(0)?;
                Ok(Ty::Int(IntTy::Nint))
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
                let common = Ty::Int(ty.unify(other));
                self.coerce(recv, recv_ty.clone(), &common)?;
                self.coerce(&mut args[0], Ty::Int(other), &common)?;
                Ok(if name == "compare" { Ty::Int(IntTy::Int) } else { Ty::Bool })
            }
            (Ty::Str | Ty::Bool, "compare" | "equals") if name == "equals" || *target == Ty::Str => {
                arity(1)?;
                let other = self.operand(&mut args[0], None)?;
                if other.strip_borrow() != target {
                    return Err(Diag::error(args[0].span, format!("expected `{target}`, found `{other}`")));
                }
                Ok(if name == "compare" { Ty::Int(IntTy::Int) } else { Ty::Bool })
            }
            // The position of the first such byte, `null` if there is none.
            (Ty::Str, "indexOf") => {
                arity(1)?;
                self.typed(&mut args[0], &Ty::Int(IntTy::Byte))?;
                Ok(Ty::Nullable(Box::new(Ty::Int(IntTy::Nint))))
            }
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
                let Ty::Struct(owner) = target else {
                    return Err(if name.starts_with("try") { diag.note(format!("`{name}` is not in the interpreter yet")) } else { diag });
                };
                let info = &structs[owner];
                if info.fields.iter().any(|(field, _)| field == name) {
                    return Err(diag.hint(format!("`{name}` is a field; read it without the parentheses")));
                }
                Err(match nearest(name, info.methods.keys().map(String::as_str)) {
                    Some(close) => diag.hint(format!("`{close}`")),
                    None => diag,
                })
            }
        }
    }

    /// A method of the built-in `List<T>` or `HashMap<K, V>`.
    #[allow(clippy::too_many_arguments)]
    fn collection_method(
        &mut self,
        recv: &Expr,
        target: &Ty,
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
        let what = match target {
            Ty::List(_) => "list",
            Ty::Map(..) => "map",
            _ => "set",
        };
        let changing = matches!(name, "add" | "clear" | "removeLast" | "reverse" | "remove" | "getOrAdd" | "sort");
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
            Ty::Int(_) | Ty::Str => true,
            Ty::Struct(name) => self.comparable.contains(name),
            _ => false,
        });
        let compares = |item: &Ty, needs: &str| match item {
            _ if needs == "Equatable" && can_equal => Ok(()),
            _ if needs == "Comparable" && can_order => Ok(()),
            Ty::Struct(inner) | Ty::Enum(inner) => Err(Diag::error(name_span, format!("`{name}` compares the items, and `{inner}` is not `{needs}`"))
                .hint(format!("add it to the definition: `{inner} : {needs}`"))),
            Ty::List(_) | Ty::Set(_) | Ty::Map(..) if needs == "Comparable" => Err(Diag::error(name_span, format!("`{name}` puts the items in order, and a collection has no order of its own"))),
            other => Err(Diag::error(name_span, format!("`{name}` compares the items, and `{other}` is not `{needs}`"))
                .note(format!("`{needs}`: integers{}, `string` and the types written with `: {needs}`", if needs == "Equatable" { ", `bool`" } else { "" }))),
        };
        let changes = || {
            if writable {
                Ok(())
            } else {
                Err(Diag::error(recv.span, format!("`{name}` changes the {what}, but it is reached through a `read` borrow"))
                    .hint("take the value as `ref` instead of `read`"))
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
                    Ty::List(_) => &["add", "get", "length", "indexOf", "contains", "sort", "removeLast", "reverse", "clear"],
                    Ty::Map(..) => &["add", "get", "getOrAdd", "contains", "remove", "length", "clear"],
                    _ => &["add", "contains", "remove", "length", "clear"],
                };
                // Names the same thing has in other languages.
                let other_name = match (target, name) {
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
                if let InterpPart::Expr(inner) = part {
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
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Char(_)
        | ExprKind::Null
        | ExprKind::Var(_)
        | ExprKind::FuncRef(_)
        | ExprKind::Return(None) => {}
    }
}

/// The value of a character written where a `byte` goes: it must take one byte in UTF-8.
fn byte_of(value: char, span: Span) -> Res<i64> {
    if value.is_ascii() {
        return Ok(value as i64);
    }
    Err(Diag::error(span, format!("`{value}` does not fit in one byte: it takes {} bytes in UTF-8", value.len_utf8()))
        .hint(format!("the type for any character is `char`: `char c = '{value}'`"))
        .note("`char` is not in the interpreter yet"))
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
        ExprKind::Call { name, .. } => name == "panic",
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
