//! The first back end: writes a checked program as C.
//!
//! The checker has already done the work that makes a program safe; what is left to run time
//! are the same data-dependent failures the interpreter reports, with the same messages:
//! overflow, division by zero, a cast that does not fit, `assert` and `panic`.
//!
//! Only a part of the language is compiled so far: integers, `bool`, text, functions, structs,
//! `T*` and `new`, `T?`, `read` and `ref`, and `print`. Anything else is an error that names
//! what is missing, so nothing is compiled wrongly in silence.
//!
//! How values are represented: `byte`, `int`, `long` and `nint` are fixed-size C integers; a
//! struct is a C struct passed by value; `read T` / `ref T` are pointers; a `string` is its
//! bytes, a length and a capacity, and a `read string` is a pointer and a length. A `T*` is a
//! pointer to one `malloc`ed `T`. A `T*?` is the same pointer, where `NULL` is `null`: it
//! costs nothing. Any other `T?` is the value and a flag beside it.
//!
//! Who frees: the checker has decided who owns each value, so the back end only follows it.
//! An owned value is dropped where its variable ends. A use that moves the value (marked by
//! the checker) empties the place it came from, so the later drop finds nothing to free.
//!
//! Order: `try`, `??` and `replace` are written as GNU statement expressions (clang and gcc
//! both have them), so they run where they stand, between what is written before and after.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use crate::ast::*;
use crate::diag::{snippet, Diag, Span};
use crate::types::{IntTy, Ty};

type Res<T> = Result<T, Diag>;

/// What every compiled program starts with: the checks, written once.
const RUNTIME: &str = r#"#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The generated code puts every operation in parentheses. */
#if defined(__clang__)
#pragma clang diagnostic ignored "-Wparentheses-equality"
#pragma clang diagnostic ignored "-Wgnu-statement-expression"
#endif

/* A panic is never the usual path: its code is kept out of the way of the code that runs. */
#define SG_COLD __attribute__((cold, noinline))

SG_COLD static _Noreturn void sg_die(const char* loc) {
    fputs(loc, stderr);
    exit(1);
}
SG_COLD static _Noreturn void sg_overflow(long long a, const char* op, long long b, const char* ty, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: overflow: %lld %s %lld does not fit in %s\n", a, op, b, ty);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_neg_overflow(long long a, const char* ty, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: overflow: -(%lld) does not fit in %s\n", a, ty);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_div_zero(long long a, const char* op, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: division by zero: %lld %s 0\n", a, op);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_cast_fail(long long v, const char* ty, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: value %lld does not fit in %s\n", v, ty);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_panic(const char* text, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: %s\n", text);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_assert_fail(const char* text, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n", text);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_assert_fail_i(const char* text, long long l, long long r, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n  left: %lld\n  right: %lld\n", text, l, r);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_assert_fail_b(const char* text, bool l, bool r, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n  left: %s\n  right: %s\n", text, l ? "true" : "false", r ? "true" : "false");
    sg_die(loc);
}

/* One set of checked operations for each integer type. Overflow is caught by the compiler's
   own overflow builtins: one branch that is never taken in a correct program. */
#define SG_INT(T, N, NAME) \
static inline T sg_add_##N(T a, T b, const char* loc) { T r; if (__builtin_expect(__builtin_add_overflow(a, b, &r), 0)) sg_overflow(a, "+", b, NAME, loc); return r; } \
static inline T sg_sub_##N(T a, T b, const char* loc) { T r; if (__builtin_expect(__builtin_sub_overflow(a, b, &r), 0)) sg_overflow(a, "-", b, NAME, loc); return r; } \
static inline T sg_mul_##N(T a, T b, const char* loc) { T r; if (__builtin_expect(__builtin_mul_overflow(a, b, &r), 0)) sg_overflow(a, "*", b, NAME, loc); return r; } \
static inline T sg_div_##N(T a, T b, const char* loc) { \
    if (__builtin_expect(b == 0, 0)) sg_div_zero(a, "/", loc); \
    if ((T)-1 < 0 && b == (T)-1) { T r; if (__builtin_sub_overflow((T)0, a, &r)) sg_overflow(a, "/", b, NAME, loc); return r; } \
    return a / b; } \
static inline T sg_rem_##N(T a, T b, const char* loc) { \
    if (__builtin_expect(b == 0, 0)) sg_div_zero(a, "%", loc); \
    if ((T)-1 < 0 && b == (T)-1) return 0; \
    return a % b; } \
static inline T sg_neg_##N(T a, const char* loc) { T r; if (__builtin_expect(__builtin_sub_overflow((T)0, a, &r), 0)) sg_neg_overflow(a, NAME, loc); return r; } \
static inline T sg_cast_##N(int64_t v, const char* loc) { T r = (T)v; if (__builtin_expect((int64_t)r != v, 0)) sg_cast_fail(v, NAME, loc); return r; } \
static inline T sg_wadd_##N(T a, T b) { return (T)((uint64_t)a + (uint64_t)b); } \
static inline T sg_wsub_##N(T a, T b) { return (T)((uint64_t)a - (uint64_t)b); } \
static inline T sg_wmul_##N(T a, T b) { return (T)((uint64_t)a * (uint64_t)b); }

SG_INT(uint8_t, byte, "byte")
SG_INT(int32_t, int, "int")
SG_INT(int64_t, long, "long")
SG_INT(int64_t, nint, "nint")

/* Text. A `read string` is a window: where the bytes are and how many. A `string` owns its
   bytes; capacity 0 means they are a constant inside the program, or were moved away, and
   there is nothing to free. */
typedef struct { const uint8_t* ptr; int64_t len; } sg_str;
typedef struct { uint8_t* ptr; int64_t len; int64_t cap; } sg_string;
#define SG_LIT(text, n) ((sg_string){ (uint8_t*)(text), (n), 0 })
#define SG_STR(text, n) ((sg_str){ (const uint8_t*)(text), (n) })
static inline sg_str sg_view(const sg_string* s) { return (sg_str){ s->ptr, s->len }; }
static inline void sg_drop_string(sg_string* s) { if (s->cap > 0) free(s->ptr); }
static inline sg_string sg_move_string(sg_string* s) { sg_string v = *s; s->cap = 0; return v; }
static sg_string sg_clone(sg_str s) {
    if (s.len == 0) return SG_LIT("", 0);
    uint8_t* bytes = malloc((size_t)s.len);
    if (!bytes) abort();
    memcpy(bytes, s.ptr, (size_t)s.len);
    return (sg_string){ bytes, s.len, s.len };
}
static inline bool sg_eq(sg_str a, sg_str b) { return a.len == b.len && (a.len == 0 || memcmp(a.ptr, b.ptr, (size_t)a.len) == 0); }
static inline int sg_cmp(sg_str a, sg_str b) {
    int64_t n = a.len < b.len ? a.len : b.len;
    int c = n ? memcmp(a.ptr, b.ptr, (size_t)n) : 0;
    return c ? c : (a.len > b.len) - (a.len < b.len);
}
SG_COLD static _Noreturn void sg_index_fail(long long at, long long len, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: index %lld is out of range: the length is %lld\n", at, len);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_range_fail(sg_str s, long long from, long long to, const char* loc) {
    fflush(stdout);
    if (from < 0 || to > s.len) {
        fprintf(stderr, "panic: range %lld..%lld is out of range: the length is %lld\n", from, to, (long long)s.len);
    } else if (from > to) {
        fprintf(stderr, "panic: range %lld..%lld ends before it starts\n", from, to);
    } else {
        /* One end is inside a character: find where that character starts and ends. */
        long long cut = (from < s.len && (s.ptr[from] & 0xC0) == 0x80) ? from : to;
        long long first = cut;
        while (first > 0 && (s.ptr[first] & 0xC0) == 0x80) first--;
        long long last = first + 1;
        while (last < s.len && (s.ptr[last] & 0xC0) == 0x80) last++;
        fprintf(stderr, "panic: range %lld..%lld cuts a character in half\n  `%.*s` takes bytes %lld..%lld; a `string` is always valid UTF-8\n",
                from, to, (int)(last - first), (const char*)s.ptr + first, first, last);
    }
    sg_die(loc);
}
/* A byte of the text; the index is checked in every build, because this is what keeps memory safe. */
static inline uint8_t sg_at(sg_str s, int64_t at, const char* loc) {
    if (__builtin_expect(at < 0 || at >= s.len, 0)) sg_index_fail(at, s.len, loc);
    return s.ptr[at];
}
static inline sg_str sg_slice(sg_str s, int64_t from, int64_t to, const char* loc) {
    if (__builtin_expect(from < 0 || to > s.len || from > to
            || (from < s.len && (s.ptr[from] & 0xC0) == 0x80) || (to < s.len && (s.ptr[to] & 0xC0) == 0x80), 0))
        sg_range_fail(s, from, to, loc);
    return (sg_str){ s.ptr + from, to - from };
}
static sg_string sg_format(const char* format, ...) {
    va_list count, write;
    va_start(count, format);
    va_copy(write, count);
    int n = vsnprintf(NULL, 0, format, count);
    va_end(count);
    if (n <= 0) { va_end(write); return SG_LIT("", 0); }
    uint8_t* bytes = malloc((size_t)n + 1);
    if (!bytes) abort();
    vsnprintf((char*)bytes, (size_t)n + 1, format, write);
    va_end(write);
    return (sg_string){ bytes, n, n };
}
SG_COLD static _Noreturn void sg_panicf(const char* loc, const char* format, ...) {
    va_list args;
    va_start(args, format);
    fflush(stdout);
    fputs("panic: ", stderr);
    vfprintf(stderr, format, args);
    va_end(args);
    fputc('\n', stderr);
    sg_die(loc);
}
static void sg_quoted(sg_str s) {
    fputc('"', stderr);
    for (int64_t i = 0; i < s.len; i++) {
        uint8_t c = s.ptr[i];
        if (c == '"' || c == '\\') { fputc('\\', stderr); fputc(c, stderr); }
        else if (c == '\n') fputs("\\n", stderr);
        else if (c == '\t') fputs("\\t", stderr);
        else if (c == '\r') fputs("\\r", stderr);
        else fputc(c, stderr);
    }
    fputc('"', stderr);
}
SG_COLD static _Noreturn void sg_assert_fail_s(const char* text, sg_str l, sg_str r, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n  left: ", text);
    sg_quoted(l);
    fputs("\n  right: ", stderr);
    sg_quoted(r);
    fputc('\n', stderr);
    sg_die(loc);
}
"#;

/// Writes the program as C. `path` names the source file in the panic messages. `checked`: a
/// debug build, where `+`, `-`, `*` and unary `-` panic on overflow; without it, a release
/// build, where they wrap.
pub fn emit(prog: &Program, src: &str, path: &str, checked: bool) -> Res<String> {
    let structs = prog.structs.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let mut emitter = Emitter {
        src,
        path,
        structs,
        checked,
        locs: Vec::new(),
        temps: 0,
        scopes: Vec::new(),
        loops: Vec::new(),
        pre: Vec::new(),
        post: Vec::new(),
        types: String::new(),
        glue_protos: String::new(),
        glue: String::new(),
        made: HashSet::new(),
        sigs: HashMap::new(),
        ret: Ty::Void,
    };
    if let Some(decl) = prog.enums.first() {
        return Err(not_yet(decl.name_span, "an enum"));
    }

    // Every struct is named first, so that a struct can point at itself or at a later one.
    // Then the structs themselves, each after the structs it holds by value.
    for decl in &prog.structs {
        let _ = writeln!(emitter.types, "typedef struct S_{0} S_{0};", decl.name);
    }
    let mut done: Vec<&str> = Vec::new();
    for decl in &prog.structs {
        emitter.struct_decl(decl, &mut done)?;
    }

    // What each function takes: an argument is shaped for the parameter it goes to.
    for func in &prog.funcs {
        let params = func.params.iter().map(|param| emitter.written(&param.ty)).collect::<Res<Vec<Ty>>>()?;
        emitter.sigs.insert(format!("f_{}", func.name), params);
    }
    for decl in &prog.structs {
        for method in &decl.methods {
            let params = method.params.iter().map(|param| emitter.written(&param.ty)).collect::<Res<Vec<Ty>>>()?;
            emitter.sigs.insert(format!("f_{}_{}", decl.name, method.name), params);
        }
    }

    let mut protos = String::new();
    let mut bodies = String::new();
    for func in &prog.funcs {
        emitter.func(func, None, &mut protos, &mut bodies)?;
    }
    for decl in &prog.structs {
        for method in &decl.methods {
            emitter.func(method, Some(&decl.name), &mut protos, &mut bodies)?;
        }
    }

    let mut out = String::from(RUNTIME);
    out.push('\n');
    out.push_str(&emitter.types);
    out.push('\n');
    out.push_str(&emitter.glue_protos);
    out.push('\n');
    out.push_str(&emitter.glue);
    for (number, text) in emitter.locs.iter().enumerate() {
        let _ = writeln!(out, "static const char sg_loc_{number}[] = {};", c_string(text));
    }
    out.push('\n');
    out.push_str(&protos);
    out.push('\n');
    out.push_str(&bodies);
    out.push_str("int main(void) {\n    f_main();\n    return 0;\n}\n");
    Ok(out)
}

fn not_yet(span: Span, what: &str) -> Diag {
    Diag::error(span, format!("{what} is not in the C back end yet"))
        .note("the back end compiles integers, `bool`, text, functions, structs, `T*`, `T?`, `read` and `ref`, and `print`")
}

/// A C string literal for any text.
fn c_string(text: &str) -> String {
    let mut out = String::from("\"");
    for byte in text.bytes() {
        match byte {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\t' => out.push_str("\\t"),
            // `??` followed by some characters is a trigraph in C.
            b'?' => out.push_str("\\?"),
            32..=126 => out.push(byte as char),
            // Three octal digits: nothing after them can be read as part of the escape.
            _ => {
                let _ = write!(out, "\\{byte:03o}");
            }
        }
    }
    out.push('"');
    out
}

fn int_name(ty: IntTy) -> &'static str {
    ty.name()
}

fn int_ctype(ty: IntTy) -> &'static str {
    match ty {
        IntTy::Byte => "uint8_t",
        IntTy::Int => "int32_t",
        IntTy::Long | IntTy::Nint => "int64_t",
    }
}

/// `read string`: the one borrow that is not a plain pointer.
fn is_text_view(ty: &Ty) -> bool {
    matches!(ty, Ty::Read(inner) if **inner == Ty::Str)
}

fn is_text(ty: &Ty) -> bool {
    *ty.strip_borrow() == Ty::Str
}

/// Whether this is text on the heap: `string*`, or a borrow of one.
fn is_text_behind_pointer(ty: &Ty) -> bool {
    let mut ty = ty.strip_borrow();
    let mut behind = false;
    while let Ty::Ptr(inner) = ty {
        (ty, behind) = (inner, true);
    }
    behind && *ty == Ty::Str
}

/// The name a type has inside the names of its helpers: `sg_drop_P_S_Node`. `None` for a type
/// the back end does not hold in a `T*` or a `T?` yet.
fn mangle(ty: &Ty) -> Option<String> {
    Some(match ty {
        Ty::Int(ty) => ty.name().to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Str => "string".to_string(),
        Ty::Struct(name) => format!("S_{name}"),
        Ty::Ptr(inner) => format!("P_{}", mangle(inner)?),
        Ty::Nullable(inner) => match &**inner {
            // A pointer that may be `null` is the same pointer.
            Ty::Ptr(_) => mangle(inner)?,
            Ty::Nullable(_) => return None,
            _ => format!("O_{}", mangle(inner)?),
        },
        _ => return None,
    })
}

/// A `T?` that is kept as a value and a flag: every `T?` but a pointer.
fn is_option(ty: &Ty) -> bool {
    matches!(ty, Ty::Nullable(inner) if !matches!(**inner, Ty::Ptr(_)))
}

/// The struct a field of this type needs to be defined first, if any.
fn held_struct(ty: &Ty) -> Option<&str> {
    match ty {
        Ty::Struct(name) => Some(name),
        Ty::Nullable(inner) | Ty::Ptr(inner) => held_struct(inner),
        _ => None,
    }
}

/// A variable of the function being written.
struct Local {
    name: String,
    /// Its name in C: no two variables of a function share one, so an inner variable never
    /// hides an outer one that still has to be dropped.
    cname: String,
    ty: Ty,
}

struct Emitter<'a> {
    src: &'a str,
    path: &'a str,
    structs: HashMap<&'a str, &'a StructDecl>,
    /// A debug build: overflow panics. Otherwise it wraps.
    checked: bool,
    /// The place of each line that can panic, as it is printed under the message.
    locs: Vec<String>,
    temps: usize,
    /// The variables in scope, innermost last.
    scopes: Vec<Vec<Local>>,
    /// For each loop around: how many scopes were open where it starts.
    loops: Vec<usize>,
    /// Lines that go before the statement being written: values kept in a temporary.
    pre: Vec<String>,
    /// Lines that go after it: those temporaries are dropped.
    post: Vec<String>,
    /// The C types: the structs, and a value-and-flag struct for each `T?` that is used.
    types: String,
    /// The helpers that drop, move and allocate each type that is used: first all their
    /// declarations, then their bodies, so that they can call each other.
    glue_protos: String,
    glue: String,
    /// The `T*` and `T?` types whose helpers are written.
    made: HashSet<String>,
    /// The parameter types of each function, by its C name.
    sigs: HashMap<String, Vec<Ty>>,
    /// What the function being written returns.
    ret: Ty,
}

impl<'a> Emitter<'a> {
    /// The name of the C constant that holds where `span` is in the source.
    fn loc(&mut self, span: Span) -> String {
        self.locs.push(snippet(span, self.src, self.path));
        format!("sg_loc_{}", self.locs.len() - 1)
    }

    fn temp(&mut self) -> String {
        self.temps += 1;
        format!("sg_t{}", self.temps)
    }

    fn text(&self, span: Span) -> &'a str {
        &self.src[span.start..span.end]
    }

    // ---- types ----

    /// The C type of a value.
    fn ctype(&mut self, ty: &Ty, span: Span) -> Res<String> {
        Ok(match ty {
            Ty::Int(ty) => int_ctype(*ty).to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::Void => "void".to_string(),
            Ty::Str => "sg_string".to_string(),
            Ty::Struct(name) => format!("S_{name}"),
            _ if is_text_view(ty) => "sg_str".to_string(),
            Ty::Read(inner) | Ty::Ref(inner) => format!("{}*", self.ctype(inner, span)?),
            Ty::Ptr(inner) | Ty::Nullable(inner) => {
                let Some(name) = mangle(ty) else { return Err(not_yet(span, &format!("the type `{ty}`"))) };
                self.need(ty, span)?;
                match ty {
                    Ty::Ptr(_) => format!("{}*", self.ctype(inner, span)?),
                    _ if is_option(ty) => format!("sg_{name}"),
                    _ => self.ctype(inner, span)?,
                }
            }
            other => return Err(not_yet(span, &format!("the type `{other}`"))),
        })
    }

    /// Writes, once, what a `T*` or a `T?` needs: its C type and the helpers that drop it,
    /// move it and, for a `T*`, allocate it.
    fn need(&mut self, ty: &Ty, span: Span) -> Res<()> {
        let Some(name) = mangle(ty) else { return Ok(()) };
        match ty {
            Ty::Ptr(inner) => {
                if !self.made.insert(name.clone()) {
                    return Ok(());
                }
                let target = self.ctype(inner, span)?;
                let drop_inner = match self.droppable(inner) {
                    true => format!("sg_drop_{}(p); ", mangle(inner).expect("it has a C type")),
                    false => String::new(),
                };
                let _ = writeln!(self.glue_protos, "static inline {target}* sg_new_{name}({target} value);");
                let _ = writeln!(self.glue_protos, "static inline {target}* sg_move_{name}({target}** v);");
                let _ = writeln!(self.glue_protos, "static inline void sg_drop_{name}({target}** v);");
                let _ = writeln!(
                    self.glue,
                    "static inline {target}* sg_new_{name}({target} value) {{\n    {target}* p = malloc(sizeof({target}));\n    if (!p) abort();\n    *p = value;\n    return p;\n}}"
                );
                let _ = writeln!(self.glue, "static inline {target}* sg_move_{name}({target}** v) {{ {target}* out = *v; *v = NULL; return out; }}");
                // A struct with one pointer to its own kind is a chain: it is freed in a loop,
                // link by link, so a long list does not need a deep stack. The loop drops the
                // other fields of each link itself.
                let mut chain = None;
                if let Ty::Struct(of) = &**inner {
                    let decl: &'a StructDecl = self.structs[of.as_str()];
                    let is_link = |field: &FieldDecl| self.written(&field.ty).is_ok_and(|field| field == *ty || field == Ty::Nullable(Box::new(ty.clone())));
                    let mut links = decl.fields.iter().filter(|field| is_link(field));
                    if let (Some(link), None) = (links.next(), links.next()) {
                        let mut rest = String::new();
                        for field in decl.fields.iter().filter(|field| field.name != link.name) {
                            let field_ty = self.written(&field.ty)?;
                            if self.droppable(&field_ty) {
                                let _ = write!(rest, "{} ", self.drop_of(&field_ty, &format!("p->m_{}", field.name)));
                            }
                        }
                        chain = Some((link.name.clone(), rest));
                    }
                }
                let _ = match chain {
                    Some((link, rest)) => writeln!(
                        self.glue,
                        "static inline void sg_drop_{name}({target}** v) {{\n    {target}* p = *v;\n    while (p) {{ {target}* next = p->m_{link}; {rest}free(p); p = next; }}\n}}"
                    ),
                    None => writeln!(self.glue, "static inline void sg_drop_{name}({target}** v) {{ {target}* p = *v; if (p) {{ {drop_inner}free(p); }} }}"),
                };
            }
            Ty::Nullable(inner) if is_option(ty) => {
                if !self.made.insert(name.clone()) {
                    return Ok(());
                }
                let value = self.ctype(inner, span)?;
                let _ = writeln!(self.types, "typedef struct {{ bool has; {value} value; }} sg_{name};");
                // Taking the value out leaves `null`.
                let _ = writeln!(self.glue_protos, "static inline sg_{name} sg_move_{name}(sg_{name}* v);");
                let _ = writeln!(self.glue, "static inline sg_{name} sg_move_{name}(sg_{name}* v) {{ sg_{name} out = *v; v->has = false; return out; }}");
                if self.droppable(inner) {
                    let inner = mangle(inner).expect("it has a C type");
                    let _ = writeln!(self.glue_protos, "static inline void sg_drop_{name}(sg_{name}* v);");
                    let _ = writeln!(self.glue, "static inline void sg_drop_{name}(sg_{name}* v) {{ if (v->has) sg_drop_{inner}(&v->value); }}");
                }
            }
            Ty::Nullable(inner) => self.need(inner, span)?,
            _ => {}
        }
        Ok(())
    }

    /// A type as it is written in a signature or a field.
    fn written(&self, ty: &TypeExpr) -> Res<Ty> {
        let unknown = || not_yet(ty.span, &format!("the type `{}`", self.text(ty.span)));
        if !ty.args.is_empty() {
            return Err(unknown());
        }
        let mut found = match Ty::from_name(&ty.name) {
            Some(base @ (Ty::Int(_) | Ty::Bool | Ty::Void | Ty::Str)) => base,
            None if self.structs.contains_key(ty.name.as_str()) => Ty::Struct(ty.name.clone()),
            _ => return Err(unknown()),
        };
        for suffix in &ty.suffixes {
            found = match suffix {
                Suffix::Pointer => Ty::Ptr(Box::new(found)),
                Suffix::Nullable => Ty::Nullable(Box::new(found)),
            };
            if mangle(&found).is_none() {
                return Err(unknown());
            }
        }
        Ok(match ty.mode {
            Some(Mode::Read) => Ty::Read(Box::new(found)),
            Some(Mode::Ref) => Ty::Ref(Box::new(found)),
            None => found,
        })
    }

    /// Whether a value of this type owns something that is freed when it ends.
    fn droppable(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Str | Ty::Ptr(_) => true,
            Ty::Nullable(inner) => self.droppable(inner),
            Ty::Struct(name) => self.structs[name.as_str()].fields.iter().any(|field| self.written(&field.ty).is_ok_and(|ty| self.droppable(&ty))),
            _ => false,
        }
    }

    /// The C statement that drops what is at `place`.
    fn drop_of(&self, ty: &Ty, place: &str) -> String {
        format!("sg_drop_{}(&{place});", mangle(ty).expect("what is dropped has a C type"))
    }

    /// The C expression that takes the value out of `place` and leaves it empty.
    fn move_of(&self, ty: &Ty, place: &str) -> String {
        format!("sg_move_{}(&{place})", mangle(ty).expect("what is moved has a C type"))
    }

    fn struct_decl(&mut self, decl: &'a StructDecl, done: &mut Vec<&'a str>) -> Res<()> {
        if done.contains(&decl.name.as_str()) {
            return Ok(());
        }
        done.push(&decl.name);
        if let Some((_, span)) = decl.sources.first() {
            return Err(not_yet(*span, "a type that carries a borrow"));
        }
        let name = &decl.name;
        let (mut fields, mut drops, mut forgets) = (String::new(), String::new(), String::new());
        for field in &decl.fields {
            let ty = self.written(&field.ty)?;
            if ty.is_borrow() {
                return Err(not_yet(field.ty.span, "a field that borrows"));
            }
            if let Some(inner) = held_struct(&ty) {
                let inner: &'a StructDecl = self.structs[inner];
                self.struct_decl(inner, done)?;
            }
            let _ = writeln!(fields, "    {} m_{};", self.ctype(&ty, field.ty.span)?, field.name);
            if self.droppable(&ty) {
                let place = format!("v->m_{}", field.name);
                let _ = writeln!(drops, "    {}", self.drop_of(&ty, &place));
                let _ = writeln!(forgets, "    (void){};", self.move_of(&ty, &place));
            }
        }
        let _ = writeln!(self.types, "struct S_{name} {{\n{fields}}};");
        // What frees the struct, and what takes it out of a place and leaves that place empty.
        if !drops.is_empty() {
            let _ = writeln!(self.glue_protos, "static inline void sg_drop_S_{name}(S_{name}* v);\nstatic inline S_{name} sg_move_S_{name}(S_{name}* v);");
            let _ = writeln!(self.glue, "static inline void sg_drop_S_{name}(S_{name}* v) {{\n{drops}}}");
            let _ = writeln!(self.glue, "static inline S_{name} sg_move_S_{name}(S_{name}* v) {{\n    S_{name} out = *v;\n{forgets}    return out;\n}}");
        }
        Ok(())
    }

    // ---- functions and statements ----

    fn func(&mut self, func: &'a Func, owner: Option<&str>, protos: &mut String, bodies: &mut String) -> Res<()> {
        let ret = self.written(&func.ret)?;
        let name = match owner {
            Some(owner) => format!("f_{owner}_{}", func.name),
            None => format!("f_{}", func.name),
        };
        let mut params = Vec::new();
        let mut scope = Vec::new();
        if let (Some(this), Some(owner)) = (func.this, owner) {
            let owner_ty = Ty::Struct(owner.to_string());
            let ty = match this {
                This::Value => owner_ty,
                This::Read => Ty::Read(Box::new(owner_ty)),
                This::Ref => Ty::Ref(Box::new(owner_ty)),
            };
            params.push(format!("{} v_this", self.ctype(&ty, func.name_span)?));
            scope.push(Local { name: "this".to_string(), cname: "v_this".to_string(), ty });
        }
        for param in &func.params {
            let ty = self.written(&param.ty)?;
            params.push(format!("{} v_{}", self.ctype(&ty, param.ty.span)?, param.name));
            scope.push(Local { name: param.name.clone(), cname: format!("v_{}", param.name), ty });
        }
        let params = if params.is_empty() { "void".to_string() } else { params.join(", ") };
        let head = format!("static {} {name}({params})", self.ctype(&ret, func.ret.span)?);
        let _ = writeln!(protos, "{head};");
        let _ = writeln!(bodies, "{head} {{");
        self.ret = ret;
        self.temps = 0;
        self.scopes = vec![scope];
        self.stmts(&func.body, 1, bodies)?;
        // What the function was given by value and still owns at its end.
        if !ends_by_leaving(&func.body) {
            for line in self.drops(0) {
                let _ = writeln!(bodies, "    {line}");
            }
        }
        self.scopes.clear();
        bodies.push_str("}\n\n");
        Ok(())
    }

    /// The drops for every variable in the scopes from `from` on, innermost and newest first.
    fn drops(&self, from: usize) -> Vec<String> {
        let mut lines = Vec::new();
        for scope in self.scopes[from..].iter().rev() {
            for local in scope.iter().rev() {
                if self.droppable(&local.ty) {
                    lines.push(self.drop_of(&local.ty, &local.cname));
                }
            }
        }
        lines
    }

    fn local(&self, name: &str) -> &Local {
        self.scopes.iter().rev().find_map(|scope| scope.iter().rev().find(|local| local.name == name)).expect("names are checked before the back end runs")
    }

    fn declare(&mut self, name: &str, ty: Ty) -> String {
        self.temps += 1;
        let cname = format!("v_{name}_{}", self.temps);
        self.scopes.last_mut().expect("an open scope").push(Local { name: name.to_string(), cname: cname.clone(), ty });
        cname
    }

    /// A block: its own scope, and at its end the drops of what it declared.
    fn block(&mut self, block: &'a Block, depth: usize, out: &mut String) -> Res<()> {
        self.scopes.push(Vec::new());
        self.stmts(block, depth, out)?;
        if !ends_by_leaving(block) {
            let pad = "    ".repeat(depth);
            for line in self.drops(self.scopes.len() - 1) {
                let _ = writeln!(out, "{pad}{line}");
            }
        }
        self.scopes.pop();
        Ok(())
    }

    fn stmts(&mut self, block: &'a Block, depth: usize, out: &mut String) -> Res<()> {
        for stmt in block {
            self.stmt(stmt, depth, out)?;
        }
        Ok(())
    }

    /// Writes the lines that go before the statement and forgets them.
    fn flush_pre(&mut self, pad: &str, out: &mut String) {
        for line in self.pre.drain(..) {
            let _ = writeln!(out, "{pad}{line}");
        }
    }

    fn flush_post(&mut self, pad: &str, out: &mut String) {
        for line in self.post.drain(..) {
            let _ = writeln!(out, "{pad}{line}");
        }
    }

    fn stmt(&mut self, stmt: &'a Stmt, depth: usize, out: &mut String) -> Res<()> {
        let pad = "    ".repeat(depth);
        match &stmt.kind {
            StmtKind::Let { ty, name, init, .. } => {
                let ty = match ty {
                    Some(written) => self.written(written)?,
                    None => init.ty.clone(),
                };
                let value = self.head_value(init, &ty)?;
                let ctype = self.ctype(&ty, init.span)?;
                self.flush_pre(&pad, out);
                let cname = self.declare(name, ty);
                let _ = writeln!(out, "{pad}{ctype} {cname} = {value};");
                self.flush_post(&pad, out);
            }
            StmtKind::Assign { target, op, value, through } => {
                // A `read` variable pointed at something else: the variable itself is assigned.
                let rebound = match &target.kind {
                    ExprKind::Var(name) if !through => Some(self.local(name)).filter(|local| local.ty.is_borrow()).map(|local| (local.cname.clone(), local.ty.clone())),
                    _ => None,
                };
                let rebinds = rebound.is_some();
                let (place, held) = match rebound {
                    Some(variable) => variable,
                    None => self.slot(target)?,
                };
                let new = self.head_value(value, &held)?;
                self.flush_pre(&pad, out);
                match op {
                    // The new value is ready before the old one is dropped: it may be made from it.
                    None if self.droppable(&held) && !rebinds => {
                        let (ctype, temp) = (self.ctype(&held, target.span)?, self.temp());
                        let _ = writeln!(out, "{pad}{{ {ctype} {temp} = {new}; {} {place} = {temp}; }}", self.drop_of(&held, &place));
                    }
                    None => {
                        let _ = writeln!(out, "{pad}{place} = {new};");
                    }
                    Some(op) => {
                        let Ty::Int(ty) = held else { return Err(not_yet(stmt.span, "this assignment")) };
                        let operation = self.arithmetic(*op, ty, &place, &new, stmt.span);
                        let _ = writeln!(out, "{pad}{place} = {operation};");
                    }
                }
                self.flush_post(&pad, out);
            }
            StmtKind::If { cond, then, els } => {
                let mut cond = self.value(cond)?;
                self.flush_pre(&pad, out);
                if !self.post.is_empty() {
                    let temp = self.temp();
                    let _ = writeln!(out, "{pad}bool {temp} = {cond};");
                    self.flush_post(&pad, out);
                    cond = temp;
                }
                let _ = writeln!(out, "{pad}if ({cond}) {{");
                self.block(then, depth + 1, out)?;
                if let Some(els) = els {
                    let _ = writeln!(out, "{pad}}} else {{");
                    self.block(els, depth + 1, out)?;
                }
                let _ = writeln!(out, "{pad}}}");
            }
            StmtKind::While { cond, body } => {
                let cond = self.value(cond)?;
                self.loops.push(self.scopes.len());
                if self.pre.is_empty() && self.post.is_empty() {
                    let _ = writeln!(out, "{pad}while ({cond}) {{");
                } else {
                    // The condition needs temporaries: they are made and dropped in every round.
                    let (inner, temp) = (format!("{pad}    "), self.temp());
                    let _ = writeln!(out, "{pad}while (1) {{");
                    self.flush_pre(&inner, out);
                    let _ = writeln!(out, "{inner}bool {temp} = {cond};");
                    self.flush_post(&inner, out);
                    let _ = writeln!(out, "{inner}if (!{temp}) break;");
                }
                self.block(body, depth + 1, out)?;
                self.loops.pop();
                let _ = writeln!(out, "{pad}}}");
            }
            StmtKind::For { var, from, to, body, .. } => {
                // The ends are computed once; the counter stops before the end, so it cannot overflow.
                let ty = self.ctype(&from.ty, from.span)?;
                let end = self.temp();
                let (from_value, to_value) = (self.value(from)?, self.value(to)?);
                self.flush_pre(&pad, out);
                self.scopes.push(Vec::new());
                let cname = self.declare(var, from.ty.clone());
                let _ = writeln!(out, "{pad}{{");
                let _ = writeln!(out, "{pad}{ty} {cname} = {from_value}; {ty} {end} = {to_value};");
                self.flush_post(&pad, out);
                self.loops.push(self.scopes.len());
                let _ = writeln!(out, "{pad}for (; {cname} < {end}; {cname}++) {{");
                self.block(body, depth + 1, out)?;
                self.loops.pop();
                self.scopes.pop();
                let _ = writeln!(out, "{pad}}}\n{pad}}}");
            }
            StmtKind::ForEach { iter, .. } => return Err(not_yet(iter.span, "a `for` over a collection")),
            StmtKind::Return(value) => {
                for line in self.return_lines(value.as_ref(), &[], stmt.span)? {
                    let _ = writeln!(out, "{pad}{line}");
                }
            }
            StmtKind::Break | StmtKind::Continue => {
                // What the loop body declared ends here.
                let from = *self.loops.last().expect("the checker allows these only in a loop");
                for line in self.drops(from) {
                    let _ = writeln!(out, "{pad}{line}");
                }
                let word = if matches!(stmt.kind, StmtKind::Break) { "break" } else { "continue" };
                let _ = writeln!(out, "{pad}{word};");
            }
            StmtKind::Discard(value) => self.unused(value, &pad, out)?,
            StmtKind::Expr(expr) => match &expr.kind {
                ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "print" => self.print(&args[0], &pad, out)?,
                ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "assert" => self.assert(expr, &args[0], &pad, out)?,
                ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "panic" => {
                    let (format, values) = self.format(&args[0])?;
                    let loc = self.loc(expr.span);
                    self.flush_pre(&pad, out);
                    let _ = writeln!(out, "{pad}sg_panicf({loc}, {}{values});", c_string(&format));
                    self.post.clear();
                }
                _ => self.unused(expr, &pad, out)?,
            },
        }
        Ok(())
    }

    /// The lines of a `return`: the value is ready before anything it may be made from is
    /// dropped. `pending`: what the statement around it still has to drop, when the `return`
    /// stands inside an expression (`x ?? return y`). Uses up the lines waiting in `pre` and `post`.
    fn return_lines(&mut self, value: Option<&'a Expr>, pending: &[String], span: Span) -> Res<Vec<String>> {
        let ret = self.ret.clone();
        let value = match value {
            Some(value) => Some(self.head_value(value, &ret)?),
            None => None,
        };
        let mut lines = std::mem::take(&mut self.pre);
        let post = std::mem::take(&mut self.post);
        let drops = self.drops(0);
        match value {
            Some(value) if !drops.is_empty() || !post.is_empty() || !pending.is_empty() => {
                let (ctype, temp) = (self.ctype(&ret, span)?, self.temp());
                lines.push(format!("{ctype} {temp} = {value};"));
                lines.extend(post);
                lines.extend(pending.iter().cloned());
                lines.extend(drops);
                lines.push(format!("return {temp};"));
            }
            Some(value) => lines.push(format!("return {value};")),
            None => {
                lines.extend(post);
                lines.extend(pending.iter().cloned());
                lines.extend(drops);
                lines.push("return;".to_string());
            }
        }
        Ok(lines)
    }

    /// The value of an expression that is the whole right side of a statement, shaped for where
    /// it goes. Only here can `try` and `?? return` leave the function: nothing else of the
    /// statement has been computed yet, so nothing is left behind.
    fn head_value(&mut self, expr: &'a Expr, want: &Ty) -> Res<String> {
        let opened = match &expr.kind {
            ExprKind::Try { expr: inner, kind: TryKind::Null } => {
                let value = self.value(inner)?;
                let ret = self.ret.clone();
                let null = self.lift(String::new(), &Ty::Null, &ret, expr.span)?;
                let mut exit = self.post.clone();
                exit.extend(self.drops(0));
                exit.push(format!("return {null};"));
                self.open(&inner.ty, value, exit, inner.span)?
            }
            ExprKind::Coalesce { left, right, on_result: false } if matches!(right.kind, ExprKind::Return(_)) => {
                let ExprKind::Return(returned) = &right.kind else { unreachable!("matched above") };
                let value = self.value(left)?;
                // The `return` has its own temporaries; the ones made so far are dropped on its way out.
                let (pre, post) = (std::mem::take(&mut self.pre), std::mem::take(&mut self.post));
                let exit = self.return_lines(returned.as_deref(), &post, right.span);
                (self.pre, self.post) = (pre, post);
                self.open(&left.ty, value, exit?, left.span)?
            }
            _ => return self.value_as(expr, want),
        };
        self.lift(opened, &expr.ty, want, expr.span)
    }

    /// Opens a `T?`: gives what it holds, or runs `exit` (which leaves the function) on `null`.
    fn open(&mut self, ty: &Ty, value: String, exit: Vec<String>, span: Span) -> Res<String> {
        if !matches!(ty, Ty::Nullable(_)) {
            return Err(not_yet(span, &format!("opening a `{ty}`")));
        }
        let (ctype, temp) = (self.ctype(ty, span)?, self.temp());
        let (test, held) = match is_option(ty) {
            true => (format!("{temp}.has"), format!("{temp}.value")),
            false => (temp.clone(), temp.clone()),
        };
        Ok(format!("({{ {ctype} {temp} = {value}; if (!{test}) {{ {} }} {held}; }})", exit.join(" ")))
    }

    /// The value of an expression, shaped for where it goes: a `T` that goes where a `T?` is
    /// expected gets its flag, and `null` is written for the type it stands for.
    fn value_as(&mut self, expr: &'a Expr, want: &Ty) -> Res<String> {
        let value = match expr.kind {
            ExprKind::Null => String::new(),
            _ => self.value(expr)?,
        };
        self.lift(value, &expr.ty, want, expr.span)
    }

    fn lift(&mut self, value: String, found: &Ty, want: &Ty, span: Span) -> Res<String> {
        match want {
            Ty::Nullable(inner) if is_option(want) => {
                let ctype = self.ctype(want, span)?;
                match found {
                    Ty::Nullable(_) | Ty::Never => Ok(value),
                    Ty::Null => Ok(format!("(({ctype}){{ false }})")),
                    _ => Ok(format!("(({ctype}){{ true, {} }})", self.lift(value, found, inner, span)?)),
                }
            }
            Ty::Nullable(inner) => match found {
                Ty::Null => Ok("NULL".to_string()),
                Ty::Nullable(_) | Ty::Never => Ok(value),
                _ => self.lift(value, found, inner, span),
            },
            _ if *found == Ty::Null => Err(not_yet(span, "`null` here")),
            // A pointer to a value is not a pointer to a value and a flag.
            Ty::Read(to) | Ty::Ref(to) if is_option(to) && found.is_borrow() && !matches!(found.strip_borrow(), Ty::Nullable(_)) => {
                Err(not_yet(span, &format!("a `{found}` lent as `{want}`")))
            }
            _ => Ok(value),
        }
    }

    /// A function called with its arguments, each shaped for its parameter.
    fn call(&mut self, name: String, receiver: Option<String>, args: &'a [Expr]) -> Res<String> {
        let params = self.sigs.get(&name).cloned().unwrap_or_default();
        let mut all: Vec<String> = receiver.into_iter().collect();
        for (position, arg) in args.iter().enumerate() {
            all.push(match params.get(position) {
                Some(want) => self.value_as(arg, want)?,
                None => self.value(arg)?,
            });
        }
        Ok(format!("{name}({})", all.join(", ")))
    }

    /// An expression whose value is not kept: it is computed, and dropped if it owns something.
    fn unused(&mut self, expr: &'a Expr, pad: &str, out: &mut String) -> Res<()> {
        let value = self.head_value(expr, &expr.ty)?;
        self.flush_pre(pad, out);
        if self.droppable(&expr.ty) {
            let (ctype, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
            let _ = writeln!(out, "{pad}{{ {ctype} {temp} = {value}; {} }}", self.drop_of(&expr.ty, &temp));
        } else if expr.ty == Ty::Void || expr.ty == Ty::Never {
            let _ = writeln!(out, "{pad}{value};");
        } else {
            let _ = writeln!(out, "{pad}(void)({value});");
        }
        self.flush_post(pad, out);
        Ok(())
    }

    /// What `printf` is told and what it is given, for `print`, `panic` and `.toString()`: a
    /// number, a `bool`, text, or `$"..."` made of those. The values come with a leading comma.
    fn format(&mut self, arg: &'a Expr) -> Res<(String, String)> {
        let mut format = String::new();
        let mut values = Vec::new();
        match &arg.kind {
            ExprKind::Str(text) => format.push_str(&text.replace('%', "%%")),
            ExprKind::Interp(parts) => {
                for part in parts {
                    match part {
                        InterpPart::Text(text) => format.push_str(&text.replace('%', "%%")),
                        InterpPart::Expr(expr) => self.formatted(expr, &mut format, &mut values)?,
                    }
                }
            }
            _ => self.formatted(arg, &mut format, &mut values)?,
        }
        Ok((format, values.iter().map(|value| format!(", {value}")).collect()))
    }

    /// One value inside a formatted line.
    fn formatted(&mut self, expr: &'a Expr, format: &mut String, values: &mut Vec<String>) -> Res<()> {
        if let ExprKind::Str(text) = &expr.kind {
            format.push_str(&text.replace('%', "%%"));
            return Ok(());
        }
        // A value on the heap is written like the value: `print(count)` with `int* count`.
        if let Ty::Ptr(_) = expr.ty.strip_borrow() {
            let mut place = self.look(expr)?;
            let mut ty = expr.ty.strip_borrow();
            while let Ty::Ptr(inner) = ty {
                place = format!("(*{place})");
                ty = inner;
            }
            match ty {
                Ty::Int(_) => {
                    format.push_str("%lld");
                    values.push(format!("(long long)({place})"));
                }
                Ty::Bool => {
                    format.push_str("%s");
                    values.push(format!("({place}) ? \"true\" : \"false\""));
                }
                Ty::Str => {
                    format.push_str("%.*s");
                    values.push(format!("(int){place}.len, (const char*){place}.ptr"));
                }
                other => return Err(not_yet(expr.span, &format!("writing a `{other}*` as text"))),
            }
            return Ok(());
        }
        if is_text(&expr.ty) {
            // The text is looked at twice (its length and its bytes), so it gets a name.
            let (view, temp) = (self.view(expr)?, self.temp());
            self.pre.push(format!("sg_str {temp} = {view};"));
            format.push_str("%.*s");
            values.push(format!("(int){temp}.len, (const char*){temp}.ptr"));
            return Ok(());
        }
        let value = self.value(expr)?;
        // What is printed may be a borrow of a number: `print(n)` with `read int n`.
        let value = if expr.ty.is_borrow() { format!("(*{value})") } else { value };
        match expr.ty.strip_borrow() {
            Ty::Int(_) => {
                format.push_str("%lld");
                values.push(format!("(long long)({value})"));
            }
            Ty::Bool => {
                format.push_str("%s");
                values.push(format!("({value}) ? \"true\" : \"false\""));
            }
            other => return Err(not_yet(expr.span, &format!("writing a `{other}` as text"))),
        }
        Ok(())
    }

    fn print(&mut self, arg: &'a Expr, pad: &str, out: &mut String) -> Res<()> {
        let (mut format, values) = self.format(arg)?;
        format.push('\n');
        self.flush_pre(pad, out);
        let _ = writeln!(out, "{pad}printf({}{values});", c_string(&format));
        self.flush_post(pad, out);
        Ok(())
    }

    /// `assert(cond)`: on failure the text of the condition and, for a comparison, both sides.
    fn assert(&mut self, call: &'a Expr, cond: &'a Expr, pad: &str, out: &mut String) -> Res<()> {
        let text = c_string(self.text(cond.span));
        let loc = self.loc(call.span);
        if let ExprKind::Binary(op, left, right) = &cond.kind {
            let sides = match left.ty.strip_borrow() {
                Ty::Int(_) => Some(("sg_assert_fail_i", "long long")),
                Ty::Bool => Some(("sg_assert_fail_b", "bool")),
                Ty::Str => Some(("sg_assert_fail_s", "")),
                _ => None,
            };
            if let (true, Some((fail, shown))) = (op.is_comparison(), sides) {
                let (l, r) = (self.temp(), self.temp());
                let line = if shown.is_empty() {
                    let (left, right) = (self.view(left)?, self.view(right)?);
                    let test = match op {
                        BinOp::Eq => format!("sg_eq({l}, {r})"),
                        BinOp::Ne => format!("!sg_eq({l}, {r})"),
                        _ => format!("sg_cmp({l}, {r}) {} 0", op.symbol()),
                    };
                    format!("{{ sg_str {l} = {left}; sg_str {r} = {right}; if (!({test})) {fail}({text}, {l}, {r}, {loc}); }}")
                } else {
                    let ty = self.ctype(left.ty.strip_borrow(), left.span)?;
                    let (left, right) = (self.value(left)?, self.value(right)?);
                    format!("{{ {ty} {l} = {left}; {ty} {r} = {right}; if (!({l} {} {r})) {fail}({text}, ({shown}){l}, ({shown}){r}, {loc}); }}", op.symbol())
                };
                self.flush_pre(pad, out);
                let _ = writeln!(out, "{pad}{line}");
                self.flush_post(pad, out);
                return Ok(());
            }
        }
        let cond = self.value(cond)?;
        self.flush_pre(pad, out);
        let _ = writeln!(out, "{pad}if (!({cond})) sg_assert_fail({text}, {loc});");
        self.flush_post(pad, out);
        Ok(())
    }

    // ---- expressions ----

    /// Whether a variable declared as a value and a flag is used here as the value alone: after
    /// `if x is not null` the checker gives `x` the type without the `?`.
    fn narrowed(&self, name: &str, used_as: &Ty) -> bool {
        is_option(self.local(name).ty.strip_borrow()) && !matches!(used_as.strip_borrow(), Ty::Nullable(_))
    }

    /// A place as C writes it: something that can be assigned to and whose address can be taken.
    /// A variable that is a pointer stands for what it points at.
    fn lvalue(&mut self, expr: &'a Expr) -> Res<String> {
        match &expr.kind {
            ExprKind::Var(name) => {
                let cname = self.local(name).cname.clone();
                let place = if expr.ty.is_borrow() && !is_text_view(&self.local(name).ty) { format!("(*{cname})") } else { cname };
                Ok(if self.narrowed(name, &expr.ty) { format!("{place}.value") } else { place })
            }
            // A field is reached through any number of `T*` on its own.
            ExprKind::Field { recv, name, .. } => {
                let mut place = self.lvalue(recv)?;
                let mut ty = recv.ty.strip_borrow();
                while let Ty::Ptr(inner) = ty {
                    place = format!("(*{place})");
                    ty = inner;
                }
                Ok(format!("{place}.m_{name}"))
            }
            _ => Err(not_yet(expr.span, "this place")),
        }
    }

    /// A place that is written to as a whole (`x = v`, `x.take()`, `x.replace(v)`) and the type
    /// it was declared with: a variable known to hold a value is still a `T?` slot.
    fn slot(&mut self, expr: &'a Expr) -> Res<(String, Ty)> {
        match &expr.kind {
            ExprKind::Var(name) => {
                let local = self.local(name);
                Ok(match local.ty.is_borrow() {
                    true => (format!("(*{})", local.cname), local.ty.strip_borrow().clone()),
                    false => (local.cname.clone(), local.ty.clone()),
                })
            }
            _ => Ok((self.lvalue(expr)?, expr.ty.strip_borrow().clone())),
        }
    }

    /// A value that is only looked at, as something C can name more than once. A value that
    /// has no name and owns something is kept in a temporary and dropped after the statement.
    fn look(&mut self, expr: &'a Expr) -> Res<String> {
        if matches!(expr.kind, ExprKind::Var(_) | ExprKind::Field { .. }) {
            return self.lvalue(expr);
        }
        let value = self.value(expr)?;
        if expr.ty.is_borrow() {
            return Ok(format!("(*{value})"));
        }
        let (ctype, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
        self.pre.push(format!("{ctype} {temp} = {value};"));
        if self.droppable(&expr.ty) {
            self.post.push(self.drop_of(&expr.ty, &temp));
        }
        Ok(temp)
    }

    /// The address of a place, for `read x`, `ref x` and the receiver of a method.
    fn address(&mut self, place: &'a Expr) -> Res<String> {
        match &place.kind {
            ExprKind::Var(name) if place.ty.is_borrow() && !is_text_view(&place.ty) && !self.narrowed(name, &place.ty) => Ok(self.local(name).cname.clone()),
            _ => Ok(format!("(&{})", self.lvalue(place)?)),
        }
    }

    /// Text that is only looked at, as a pointer and a length. An owned text that has no
    /// name (the result of a call) is kept in a temporary and dropped after the statement.
    fn view(&mut self, expr: &'a Expr) -> Res<String> {
        match (&expr.kind, &expr.ty) {
            (ExprKind::Str(text), _) => Ok(format!("SG_STR({}, {})", c_string(text), text.len())),
            (ExprKind::Borrow { expr: inner, .. }, _) if is_text(&inner.ty) => self.view(inner),
            (_, ty) if is_text_view(ty) => self.value(expr),
            (_, Ty::Ref(_)) => Ok(format!("sg_view({})", self.value(expr)?)),
            (ExprKind::Var(_) | ExprKind::Field { .. }, Ty::Str) => Ok(format!("sg_view(&{})", self.lvalue(expr)?)),
            (_, Ty::Str) => {
                let (value, temp) = (self.value(expr)?, self.temp());
                self.pre.push(format!("sg_string {temp} = {value};"));
                self.post.push(format!("sg_drop_string(&{temp});"));
                Ok(format!("sg_view(&{temp})"))
            }
            // Text on the heap is looked at where it is: `name.length()` with `string* name`.
            (_, ty) if is_text_behind_pointer(ty) => {
                let mut place = self.look(expr)?;
                let mut ty = ty.strip_borrow();
                while let Ty::Ptr(inner) = ty {
                    place = format!("(*{place})");
                    ty = inner;
                }
                Ok(format!("sg_view(&{place})"))
            }
            (_, other) => Err(not_yet(expr.span, &format!("a `{other}` used as text"))),
        }
    }

    /// An operation on two integers of the same type. In a debug build `+`, `-` and `*` panic
    /// on overflow; in a release build they wrap, written with unsigned arithmetic so that it
    /// is defined in C too. Division checks for zero in every build.
    fn arithmetic(&mut self, op: BinOp, ty: IntTy, left: &str, right: &str, span: Span) -> String {
        let (name, wrapping) = match op {
            BinOp::Add => ("add", Some("wadd")),
            BinOp::Sub => ("sub", Some("wsub")),
            BinOp::Mul => ("mul", Some("wmul")),
            BinOp::Div => ("div", None),
            _ => ("rem", None),
        };
        match (self.checked, wrapping) {
            (false, Some(wrapping)) => format!("sg_{wrapping}_{}({left}, {right})", int_name(ty)),
            _ => {
                let loc = self.loc(span);
                format!("sg_{name}_{}({left}, {right}, {loc})", int_name(ty))
            }
        }
    }

    /// The value of an expression as C writes it. A use that the checker marked as a move
    /// takes the value out of its place and leaves the place empty.
    fn value(&mut self, expr: &'a Expr) -> Res<String> {
        match &expr.kind {
            ExprKind::Int(value, ty) => Ok(match *value {
                i64::MIN => format!("(({})(-9223372036854775807LL - 1))", int_ctype(*ty)),
                value => format!("(({}){value}LL)", int_ctype(*ty)),
            }),
            ExprKind::Bool(value) => Ok(value.to_string()),
            // A constant is a `string` with no capacity: nothing is allocated and nothing freed.
            ExprKind::Str(text) => Ok(format!("SG_LIT({}, {})", c_string(text), text.len())),
            ExprKind::Var(_) | ExprKind::Field { .. } if expr.moved && self.droppable(&expr.ty) => {
                let place = self.lvalue(expr)?;
                Ok(self.move_of(&expr.ty, &place))
            }
            // A variable that holds a value and a flag, used as the value alone.
            ExprKind::Var(name) if self.narrowed(name, &expr.ty) => {
                let place = self.lvalue(expr)?;
                Ok(match &expr.ty {
                    ty if is_text_view(ty) => format!("sg_view(&{place})"),
                    ty if ty.is_borrow() => format!("(&{place})"),
                    _ => place,
                })
            }
            ExprKind::Var(name) => Ok(self.local(name).cname.clone()),
            ExprKind::Field { .. } => self.lvalue(expr),
            // A number read out of a pointer that has no name: the pointer is freed after the statement.
            ExprKind::Load(inner) if matches!(inner.ty, Ty::Ptr(_)) => Ok(format!("(*{})", self.look(inner)?)),
            ExprKind::Load(inner) => Ok(format!("(*{})", self.value(inner)?)),
            // A borrow of a `T*` used as a borrow of the `T`: one step along the pointer.
            ExprKind::Unbox(inner) => Ok(format!("(*{})", self.value(inner)?)),
            ExprKind::New(inner) => {
                let Ty::Ptr(target) = &expr.ty else { return Err(not_yet(expr.span, "this `new`")) };
                self.ctype(&expr.ty, expr.span)?;
                let value = self.value_as(inner, target)?;
                Ok(format!("sg_new_{}({value})", mangle(&expr.ty).expect("it has a C type")))
            }
            ExprKind::IsNull { expr: inner, negated } => {
                let ty = inner.ty.strip_borrow();
                if !matches!(ty, Ty::Nullable(_)) {
                    return Err(not_yet(expr.span, &format!("`is null` on `{ty}`")));
                }
                let subject = self.look(inner)?;
                let holds = if is_option(ty) { format!("{subject}.has") } else { format!("({subject} != NULL)") };
                Ok(if *negated { holds } else { format!("(!{holds})") })
            }
            ExprKind::Coalesce { right, .. } if matches!(right.kind, ExprKind::Return(_)) => Err(not_yet(expr.span, "`?? return` inside a larger expression")
                .hint("give the value a name first: `var x = ... ?? return ...`")),
            // The right side runs only when there is no value.
            ExprKind::Coalesce { left, right, on_result: false } => {
                if !matches!(left.ty, Ty::Nullable(_)) {
                    return Err(not_yet(left.span, &format!("`??` on `{}`", left.ty)));
                }
                let value = self.value(left)?;
                let before = (self.pre.len(), self.post.len());
                let fallback = self.value(right)?;
                if before != (self.pre.len(), self.post.len()) {
                    return Err(not_yet(right.span, "a value that needs a temporary on the right of `??`"));
                }
                let (ctype, temp) = (self.ctype(&left.ty, left.span)?, self.temp());
                let held = if is_option(&left.ty) { format!("{temp}.value") } else { temp.clone() };
                // A right side that never gives a value (`panic`) still has to have the type in C.
                let fallback = if right.ty == Ty::Never { format!("({fallback}, {held})") } else { fallback };
                let holds = if is_option(&left.ty) { format!("{temp}.has") } else { temp.clone() };
                Ok(format!("({{ {ctype} {temp} = {value}; {holds} ? {held} : {fallback}; }})"))
            }
            ExprKind::Try { kind: TryKind::Null, .. } => Err(not_yet(expr.span, "`try` inside a larger expression")
                .hint("give the value a name first: `var x = try ...`")),
            ExprKind::Widen(ty, inner) => Ok(format!("(({}){})", int_ctype(*ty), self.value(inner)?)),
            ExprKind::Borrow { expr: inner, .. } if is_text_view(&expr.ty) => self.view(inner),
            ExprKind::Borrow { expr: inner, .. } if inner.is_place() => self.address(inner),
            ExprKind::Unary(UnOp::Not, operand) => Ok(format!("(!{})", self.value(operand)?)),
            ExprKind::Unary(UnOp::Neg, operand) => {
                let Ty::Int(ty) = &expr.ty else { return Err(not_yet(expr.span, "this operator")) };
                let operand = self.value(operand)?;
                if !self.checked {
                    return Ok(format!("sg_wsub_{0}(({1})0, {operand})", int_name(*ty), int_ctype(*ty)));
                }
                let loc = self.loc(expr.span);
                Ok(format!("sg_neg_{}({operand}, {loc})", int_name(*ty)))
            }
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), left, right) => {
                let left = self.value(left)?;
                // The right side runs only sometimes, so it cannot need a line before the statement.
                let before = (self.pre.len(), self.post.len());
                let right_value = self.value(right)?;
                if before != (self.pre.len(), self.post.len()) {
                    return Err(not_yet(right.span, &format!("a value that needs a temporary on the right of `{}`", op.symbol())));
                }
                Ok(format!("({left} {} {right_value})", op.symbol()))
            }
            ExprKind::Binary(op, left, right) if is_text(&left.ty) => {
                let (left, right) = (self.view(left)?, self.view(right)?);
                Ok(match op {
                    BinOp::Eq => format!("sg_eq({left}, {right})"),
                    BinOp::Ne => format!("(!sg_eq({left}, {right}))"),
                    _ => format!("(sg_cmp({left}, {right}) {} 0)", op.symbol()),
                })
            }
            ExprKind::Binary(op, left, right) => {
                let (operands, left, right) = (left.ty.strip_borrow().clone(), self.value(left)?, self.value(right)?);
                match (&operands, op.is_comparison()) {
                    (Ty::Int(_) | Ty::Bool, true) => Ok(format!("({left} {} {right})", op.symbol())),
                    (Ty::Int(ty), false) => Ok(self.arithmetic(*op, *ty, &left, &right, expr.span)),
                    (other, _) => Err(not_yet(expr.span, &format!("`{}` on `{other}`", op.symbol()))),
                }
            }
            // A narrowing conversion follows the overflow rule: checked in a debug build, cut
            // to size in a release build.
            ExprKind::Cast { ty, expr: inner } => {
                let inner = self.value(inner)?;
                if !self.checked {
                    return Ok(format!("(({})({inner}))", int_ctype(*ty)));
                }
                let loc = self.loc(expr.span);
                Ok(format!("sg_cast_{}((int64_t)({inner}), {loc})", int_name(*ty)))
            }
            // A byte of text and a range of it: checked in every build.
            ExprKind::Index { recv, index, kind: IndexKind::Text } => {
                let (text, at, loc) = (self.view(recv)?, self.value(index)?, self.loc(expr.span));
                Ok(format!("sg_at({text}, {at}, {loc})"))
            }
            ExprKind::Slice { recv, from, to } => {
                let text = self.view(recv)?;
                if from.is_none() && to.is_none() {
                    return Ok(text);
                }
                // The text is named once: its length is one of the ends.
                let temp = self.temp();
                self.pre.push(format!("sg_str {temp} = {text};"));
                let from = match from {
                    Some(from) => self.value(from)?,
                    None => "0".to_string(),
                };
                let to = match to {
                    Some(to) => self.value(to)?,
                    None => format!("{temp}.len"),
                };
                let loc = self.loc(expr.span);
                Ok(format!("sg_slice({temp}, {from}, {to}, {loc})"))
            }
            ExprKind::StructLit { name, fields, .. } => {
                let mut inits = Vec::new();
                let decl: &'a StructDecl = self.structs[name.as_str()];
                for field in fields {
                    let want = self.written(&decl.fields[field.index].ty)?;
                    inits.push(format!(".m_{} = {}", field.name, self.value_as(&field.value, &want)?));
                }
                Ok(format!("((S_{name}){{ {} }})", inits.join(", ")))
            }
            ExprKind::Call { name, args, callee, .. } => match callee {
                // `x ?? panic("...")`: the program stops, so there is no value to shape.
                Callee::Plain if name == "panic" => {
                    let before = (self.pre.len(), self.post.len());
                    let (format, values) = self.format(&args[0])?;
                    if before != (self.pre.len(), self.post.len()) {
                        return Err(not_yet(expr.span, "a `panic` inside an expression whose message needs a temporary"));
                    }
                    let loc = self.loc(expr.span);
                    Ok(format!("sg_panicf({loc}, {}{values})", c_string(&format)))
                }
                Callee::Plain if matches!(name.as_str(), "print" | "assert") => Err(not_yet(expr.span, &format!("`{name}` used as a value"))),
                Callee::Plain => self.call(format!("f_{name}"), None, args),
                Callee::Static { owner, name } => self.call(format!("f_{owner}_{name}"), None, args),
                _ => Err(not_yet(expr.span, "a call through a function value")),
            },
            ExprKind::Method { recv, name, args, callee, .. } => match callee {
                Callee::Static { owner, name } => self.call(format!("f_{owner}_{name}"), None, args),
                Callee::Method { owner, this } => {
                    let held = recv.ty.strip_borrow().clone();
                    let receiver = match (this, &held) {
                        // A value that has no name: it is used up, or kept until the statement ends.
                        _ if !recv.is_place() => match (this, &held) {
                            (_, Ty::Ptr(_)) => return Err(not_yet(recv.span, "a method called on a `T*` that has no name")),
                            (This::Value, _) => self.value(recv)?,
                            _ if recv.ty.is_borrow() => self.value(recv)?,
                            _ => format!("(&{})", self.look(recv)?),
                        },
                        // `this` by value on a `T*`: the value leaves the heap and the box is freed.
                        (This::Value, Ty::Ptr(inner)) if recv.moved => {
                            let (place, ctype, temp) = (self.lvalue(recv)?, self.ctype(inner, recv.span)?, self.temp());
                            format!("({{ {ctype}* {temp} = {}; {ctype} {temp}v = *{temp}; free({temp}); {temp}v; }})", self.move_of(&held, &place))
                        }
                        (This::Value, Ty::Ptr(_)) => format!("(*{})", self.lvalue(recv)?),
                        (This::Value, _) if recv.moved && self.droppable(&held) => {
                            let place = self.lvalue(recv)?;
                            self.move_of(&held, &place)
                        }
                        (This::Value, _) => self.lvalue(recv)?,
                        // A method of `T` called on a `T*`: along the pointer.
                        (This::Read | This::Ref, _) => {
                            let mut address = self.address(recv)?;
                            let mut ty = &held;
                            while let Ty::Ptr(inner) = ty {
                                address = format!("(*{address})");
                                ty = inner;
                            }
                            address
                        }
                    };
                    self.call(format!("f_{owner}_{name}"), Some(receiver), args)
                }
                // `slot.take()` leaves `null`; `slot.replace(v)` leaves `v`. Both give what was there.
                Callee::Take => {
                    let (place, ty) = self.slot(recv)?;
                    self.ctype(&ty, recv.span)?;
                    Ok(self.move_of(&ty, &place))
                }
                Callee::Replace => {
                    let (place, ty) = self.slot(recv)?;
                    let new = self.value_as(&args[0], &ty)?;
                    let (ctype, temp) = (self.ctype(&ty, recv.span)?, self.temp());
                    Ok(format!("({{ {ctype} {temp}n = {new}; {ctype} {temp} = {place}; {place} = {temp}n; {temp}; }})"))
                }
                Callee::Plain if is_text(&recv.ty) || is_text_behind_pointer(&recv.ty) => match name.as_str() {
                    "length" => Ok(format!("{}.len", self.view(recv)?)),
                    // A copy is the one place where text is allocated.
                    "clone" => Ok(format!("sg_clone({})", self.view(recv)?)),
                    _ => Err(not_yet(expr.span, &format!("the method `{name}`"))),
                },
                Callee::Plain if name == "toString" => {
                    let (format, values) = self.format(recv)?;
                    Ok(format!("sg_format({}{values})", c_string(&format)))
                }
                // `null` where the plain operator would panic: overflow, and division by zero.
                Callee::Plain if matches!(name.as_str(), "tryAdd" | "trySubtract" | "tryMultiply" | "tryDivide") => {
                    let Ty::Nullable(inner) = &expr.ty else { return Err(not_yet(expr.span, &format!("the method `{name}`"))) };
                    let Ty::Int(ty) = &**inner else { return Err(not_yet(expr.span, &format!("the method `{name}`"))) };
                    let (option, int, temp) = (self.ctype(&expr.ty, expr.span)?, int_ctype(*ty), self.temp());
                    let (left, right) = (self.value(recv)?, self.value(&args[0])?);
                    let builtin = match name.as_str() {
                        "tryAdd" => "add",
                        "trySubtract" => "sub",
                        "tryMultiply" => "mul",
                        _ => {
                            return Ok(format!(
                                "({{ {int} {temp}a = {left}, {temp}b = {right}; {option} {temp} = {{ false, 0 }}; \
                                 if ({temp}b != 0 && !(({int})-1 < 0 && {temp}b == ({int})-1 && __builtin_sub_overflow(({int})0, {temp}a, &{temp}.value))) \
                                 {{ {temp}.has = true; {temp}.value = {temp}a / {temp}b; }} {temp}; }})"
                            ));
                        }
                    };
                    Ok(format!("({{ {option} {temp}; {temp}.has = !__builtin_{builtin}_overflow({left}, {right}, &{temp}.value); {temp}; }})"))
                }
                Callee::Plain => {
                    let wrapping = match name.as_str() {
                        "wrappingAdd" => "wadd",
                        "wrappingSubtract" => "wsub",
                        "wrappingMultiply" => "wmul",
                        _ => return Err(not_yet(expr.span, &format!("the method `{name}`"))),
                    };
                    let Ty::Int(ty) = &expr.ty else { return Err(not_yet(expr.span, &format!("the method `{name}`"))) };
                    Ok(format!("sg_{wrapping}_{}({}, {})", int_name(*ty), self.value(recv)?, self.value(&args[0])?))
                }
                _ => Err(not_yet(expr.span, &format!("`{name}`"))),
            },
            _ => Err(not_yet(expr.span, "this expression")),
        }
    }
}

/// Whether the block ends with a statement that leaves it: what follows would never run.
fn ends_by_leaving(block: &Block) -> bool {
    matches!(block.last().map(|stmt| &stmt.kind), Some(StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue))
}
