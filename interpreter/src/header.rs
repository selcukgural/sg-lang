//! The header of a library built from SG (`sg build --library` or `--shared`): the `export`
//! functions as C declares them, with the types they take and give, each under the name it has
//! in SG. A struct or an enum C holds by its address is declared without its fields; a struct
//! of plain data is written out, laid out as SG lays it out.

use std::fmt::Write;

use crate::ast::{short_name, Program};
use crate::types::{FloatTy, IntTy, Ty};

/// The header of the library `name` (its file is `name.h`); `links` are what a program that
/// links the static library adds after it.
pub fn header(prog: &Program, name: &str, links: &[String]) -> String {
    let guard = macro_name(name);
    let api = format!("{guard}_API");
    let funcs: Vec<&crate::ast::Func> = prog.funcs.iter().filter(|func| func.export).collect();

    // What the functions name: the types C holds by their address, and the plain structs,
    // each after those it holds.
    let mut handles: Vec<String> = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    for func in &funcs {
        let (params, ret) = &prog.exported[&func.name];
        for ty in params.iter().chain([ret]) {
            named(ty, prog, &mut handles, &mut plain);
        }
    }
    // A pointer to a function as a field of a plain struct is named by its struct and field.
    let mut callbacks: Callbacks = Vec::new();
    let mut callback_text = String::new();
    for full in &plain {
        let decl = prog.structs.iter().find(|decl| decl.name == *full).expect("a plain struct is declared");
        for (field, ty) in decl.fields.iter().zip(&prog.plain[full].fields) {
            let ty = match ty {
                Ty::Array(item, _) => item,
                other => other,
            };
            if matches!(ty, Ty::CFunc(..)) && callback(&callbacks, ty).is_none() {
                let typedef = format!("{}_{}", short_name(full), field.name);
                let _ = writeln!(callback_text, "typedef {};", declare(ty, &typedef, &callbacks));
                callbacks.push((ty.clone(), typedef));
            }
        }
    }

    let mut text = String::new();
    let _ = writeln!(text, "/* {name}.h: the functions of the library {name}, written in SG, as C calls them.");
    let _ = writeln!(text, "   Written by `sg build`; it is written again with the library, so it is not changed by hand.");
    let static_links = if links.is_empty() { String::new() } else { format!(" {}", links.join(" ")) };
    let _ = writeln!(text, "   A program links the static library with `lib{name}.a{static_links} -lm -pthread`,");
    let _ = writeln!(text, "   and the shared one with `-l{name}`. On Windows it links `{name}.lib{static_links}`, with");
    let _ = writeln!(text, "   {guard}_STATIC defined, or `{name}.dll.lib`, and finds `{name}.dll` beside itself. */");
    let _ = writeln!(text, "#ifndef {guard}_H\n#define {guard}_H\n");
    text.push_str("#include <stdbool.h>\n#include <stddef.h>\n#include <stdint.h>\n\n");
    let _ = writeln!(text, "/* On Windows a program that uses the DLL imports its functions; one that links the static");
    let _ = writeln!(text, "   library defines {guard}_STATIC first. */");
    let _ = writeln!(text, "#if defined(_WIN32) && !defined({guard}_STATIC)\n#define {api} __declspec(dllimport)\n#else\n#define {api}\n#endif\n");
    if !plain.is_empty() {
        text.push_str(crate::raw::C_ALIGNED);
        text.push('\n');
    }
    text.push_str("#ifdef __cplusplus\nextern \"C\" {\n#endif\n\n");
    if !handles.is_empty() {
        text.push_str("/* Values of SG that C holds by their address: made, lent and given back only through the\n   functions below. */\n");
        for full in &handles {
            let short = short_name(full);
            let _ = writeln!(text, "typedef struct {short} {short};");
        }
        text.push('\n');
    }
    text.push_str(&callback_text);
    if !plain.is_empty() {
        text.push_str("/* Plain data, laid out as SG lays it out. */\n");
        for full in &plain {
            let decl = prog.structs.iter().find(|decl| decl.name == *full).expect("a plain struct is declared");
            let names: Vec<&str> = decl.fields.iter().map(|field| field.name.as_str()).collect();
            let c_type = |ty: &Ty| match callback(&callbacks, ty) {
                Some(typedef) => typedef.to_string(),
                None => declare(ty, "", &callbacks),
            };
            text.push_str(&crate::raw::c_definition(short_name(full), &prog.plain[full], &c_type, &|at| names[at].to_string()));
        }
        text.push('\n');
    }
    for func in &funcs {
        let (params, ret) = &prog.exported[&func.name];
        let list: Vec<String> = params.iter().zip(&func.params).map(|(ty, param)| declare(ty, &param_name(&param.name), &callbacks)).collect();
        let list = if list.is_empty() { "void".to_string() } else { list.join(", ") };
        let _ = writeln!(text, "{api} {};", declare(ret, &format!("{}({list})", short_name(&func.name)), &callbacks));
    }
    text.push_str("\n#ifdef __cplusplus\n}\n#endif\n\n");
    let _ = writeln!(text, "#endif");
    text
}

/// The pointers to functions that fields of plain structs hold, each with the name of its
/// `typedef`.
type Callbacks = Vec<(Ty, String)>;

fn callback<'c>(callbacks: &'c Callbacks, ty: &Ty) -> Option<&'c str> {
    callbacks.iter().find(|(known, _)| known == ty).map(|(_, name)| name.as_str())
}

/// The name of the macros of a library: `calc-lib` gives `CALC_LIB`.
pub fn macro_name(name: &str) -> String {
    let mut out: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// A parameter named as a word of C is written with `_` after it.
fn param_name(name: &str) -> String {
    const WORDS: &[&str] = &["bool", "true", "false", "inline", "restrict", "signed", "unsigned", "typedef", "union", "volatile", "register", "auto", "extern", "goto", "sizeof", "static", "default", "case", "char", "short", "long", "const"];
    if WORDS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

/// Finds the types C holds by their address (`handles`) and the plain structs (`plain`, each
/// after those it holds) that `ty` names.
fn named(ty: &Ty, prog: &Program, handles: &mut Vec<String>, plain: &mut Vec<String>) {
    match ty {
        Ty::Ptr(inner) | Ty::Nullable(inner) | Ty::Read(inner) | Ty::Ref(inner) => match &**inner {
            Ty::Struct(name) | Ty::Enum(name) if !prog.plain.contains_key(name) => {
                if !handles.contains(name) {
                    handles.push(name.clone());
                }
            }
            other => named(other, prog, handles, plain),
        },
        Ty::Raw(inner) | Ty::Array(inner, _) | Ty::Atomic(inner) => named(inner, prog, handles, plain),
        Ty::CFunc(params, ret) => {
            for inner in params.iter().chain([&**ret]) {
                named(inner, prog, handles, plain);
            }
        }
        Ty::Struct(name) if prog.plain.contains_key(name) => {
            if plain.contains(name) {
                return;
            }
            for field in &prog.plain[name].fields {
                named(field, prog, handles, plain);
            }
            if !plain.contains(name) {
                plain.push(name.clone());
            }
        }
        // An opaque type of C, behind a `raw T`.
        Ty::Struct(name) if !handles.contains(name) => handles.push(name.clone()),
        _ => {}
    }
}

/// How C declares `inner` of type `ty`: `int32_t n`, `Calculator *c`, `void (*visit)(void *)`.
/// `inner` empty gives the type alone.
fn declare(ty: &Ty, inner: &str, callbacks: &Callbacks) -> String {
    let base = |name: &str| if inner.is_empty() { name.to_string() } else { format!("{name} {inner}") };
    match ty {
        Ty::Int(int) => base(int_type(*int)),
        Ty::Float(FloatTy::Float) => base("float"),
        Ty::Float(FloatTy::Double) => base("double"),
        Ty::Bool => base("bool"),
        Ty::Void => base("void"),
        Ty::Atomic(inner_ty) => declare(inner_ty, inner, callbacks),
        Ty::Struct(name) | Ty::Enum(name) => base(short_name(name)),
        Ty::Raw(target) | Ty::Ptr(target) | Ty::Ref(target) => declare(target, &format!("*{inner}"), callbacks),
        Ty::Nullable(target) => declare(target, inner, callbacks),
        Ty::Read(target) if **target == Ty::CString => base("const char *").replace("* ", "*"),
        Ty::Read(target) => format!("const {}", declare(target, &format!("*{inner}"), callbacks)),
        Ty::Array(item, length) => declare(item, &format!("{inner}[{length}]"), callbacks),
        Ty::CFunc(params, ret) => match callback(callbacks, ty) {
            Some(typedef) => base(typedef),
            None => {
                let list: Vec<String> = params.iter().map(|param| declare(param, "", callbacks)).collect();
                let list = if list.is_empty() { "void".to_string() } else { list.join(", ") };
                declare(ret, &format!("(*{inner})({list})"), callbacks)
            }
        },
        other => unreachable!("the checker gives C no `{other}`"),
    }
}

/// The C type of an integer, as compiled code has it.
fn int_type(ty: IntTy) -> &'static str {
    match ty {
        IntTy::Sbyte => "int8_t",
        IntTy::Byte => "uint8_t",
        IntTy::Short => "int16_t",
        IntTy::Ushort => "uint16_t",
        IntTy::Int => "int32_t",
        IntTy::Uint => "uint32_t",
        IntTy::Long | IntTy::Nint => "int64_t",
        IntTy::Ulong | IntTy::Nuint => "uint64_t",
        IntTy::CLong => "long",
        IntTy::CULong => "unsigned long",
    }
}
