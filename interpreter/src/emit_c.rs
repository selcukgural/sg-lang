//! The first back end: writes a checked program as C.
//!
//! The checker has already done the work that makes a program safe; what is left to run time
//! are the same data-dependent failures the interpreter reports, with the same messages:
//! overflow, division by zero, a cast that does not fit, `assert` and `panic`.
//!
//! Only a part of the language is compiled so far: integers, `bool`, text, functions, structs,
//! enums with `match` and `is`, `Result` and `try`, `T*` and `new`, `T?`, `read` and `ref`, and
//! `print`. Anything else is an error that names what is missing, so nothing is compiled
//! wrongly in silence.
//!
//! How values are represented: `byte`, `int`, `long` and `nint` are fixed-size C integers; a
//! struct is a C struct passed by value; `read T` / `ref T` are pointers; a `string` is its
//! bytes, a length and a capacity, and a `read string` is a pointer and a length. A `T*` is a
//! pointer to one `malloc`ed `T`. A `T*?` is the same pointer, where `NULL` is `null`: it
//! costs nothing. Any other `T?` is the value and a flag beside it. An enum is a tag and,
//! beside it, room for the largest variant (a C union); a `match` is a chain of tests on the tag.
//! A `Result<T, E>` is such an enum with the variants `Ok(T value)` and `Error(E cause)`; `try`
//! is one test on the tag and, on an error, a `return`.
//!
//! Who frees: the checker has decided who owns each value, so the back end only follows it.
//! An owned value is dropped where its variable ends. A use that moves the value (marked by
//! the checker) empties the place it came from, so the later drop finds nothing to free.
//!
//! Order: `try`, `??` and `replace` are written as GNU statement expressions (clang and gcc
//! both have them), so they run where they stand, between what is written before and after.
//!
//! Leaving from inside an expression: `try` and `?? return` can leave the function in the
//! middle of a statement. They are compiled where nothing that would have to be freed has been
//! computed before them in that statement (see `Emitter::head`); elsewhere they are an error
//! that asks for a name.

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
    let enums = prog.enums.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let mut emitter = Emitter {
        src,
        path,
        structs,
        enums,
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
        maybe: 0,
        head: false,
        exits: 0,
    };
    // Every struct and enum is named first, so that a type can point at itself or at a later
    // one. Then the types themselves, each after the types it holds by value.
    for decl in &prog.structs {
        let _ = writeln!(emitter.types, "typedef struct S_{0} S_{0};", decl.name);
    }
    for decl in &prog.enums {
        let _ = writeln!(emitter.types, "typedef struct E_{0} E_{0};", decl.name);
    }
    let mut done: Vec<&str> = Vec::new();
    for decl in &prog.structs {
        emitter.struct_decl(decl, &mut done)?;
    }
    for decl in &prog.enums {
        emitter.enum_decl(decl, &mut done)?;
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

    // Where the program starts. An error that `main` ends with is written out, and the
    // program ends with the exit code 1.
    let main = prog.funcs.iter().find(|func| func.name == "main").expect("the checker asks for `main`");
    let start = match emitter.written(&main.ret)? {
        ret @ Ty::Result(..) => {
            let Ty::Result(_, error) = &ret else { unreachable!("matched above") };
            let cause = "result.u.v1.m_cause";
            let (format, values) = match &**error {
                Ty::Str => ("%.*s", format!("(int){cause}.len, (const char*){cause}.ptr")),
                Ty::Int(_) => ("%lld", format!("(long long){cause}")),
                Ty::Bool => ("%s", format!("{cause} ? \"true\" : \"false\"")),
                other => return Err(not_yet(main.ret.span, &format!("writing the `{other}` that `main` ends with"))),
            };
            let ctype = emitter.ctype(&ret, main.ret.span)?;
            let drop = if emitter.droppable(error) { emitter.drop_of(error, cause) } else { String::new() };
            format!(
                "int main(void) {{\n    {ctype} result = f_main();\n    if (result.tag != 0) {{\n        fflush(stdout);\n        \
                 fprintf(stderr, \"error: {format}\\n\", {values});\n        {drop}\n        return 1;\n    }}\n    return 0;\n}}\n"
            )
        }
        _ => "int main(void) {\n    f_main();\n    return 0;\n}\n".to_string(),
    };

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
    out.push_str(&start);
    Ok(out)
}

fn not_yet(span: Span, what: &str) -> Diag {
    Diag::error(span, format!("{what} is not in the C back end yet"))
        .note("the back end compiles integers, `bool`, text, functions, structs, enums and `match`, `Result` and `try`, `T*`, `T?`, `read` and `ref`, and `print`")
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
        Ty::Enum(name) => format!("E_{name}"),
        Ty::Ptr(inner) => format!("P_{}", mangle(inner)?),
        Ty::Nullable(inner) => match &**inner {
            // A pointer that may be `null` is the same pointer.
            Ty::Ptr(_) => mangle(inner)?,
            Ty::Nullable(_) => return None,
            _ => format!("O_{}", mangle(inner)?),
        },
        // The length of the first name keeps two different pairs from sharing a name.
        Ty::Result(ok, error) => {
            let ok = if **ok == Ty::Void { "void".to_string() } else { mangle(ok)? };
            format!("R{}_{ok}_{}", ok.len(), mangle(error)?)
        }
        _ => return None,
    })
}

/// A `T?` that is kept as a value and a flag: every `T?` but a pointer.
fn is_option(ty: &Ty) -> bool {
    matches!(ty, Ty::Nullable(inner) if !matches!(**inner, Ty::Ptr(_)))
}

/// The structs and enums a field of this type needs to be defined first.
fn held_types<'t>(ty: &'t Ty, found: &mut Vec<&'t str>) {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => found.push(name),
        Ty::Nullable(inner) | Ty::Ptr(inner) => held_types(inner, found),
        Ty::Result(ok, error) => {
            held_types(ok, found);
            held_types(error, found);
        }
        _ => {}
    }
}

/// A value that is written out in full where it is used: when it is computed cannot matter.
fn is_constant(expr: &Expr) -> bool {
    matches!(expr.kind, ExprKind::Int(..) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Null)
}

/// What one variant of an enum carries: the name, the type and the place in the source of each value.
type Carried = Vec<(String, Ty, Span)>;

/// The tag an enum has after its value was moved out: no variant, so nothing is dropped.
const MOVED_TAG: usize = 255;

/// What computing an expression can do besides giving its value. It decides whether the
/// order in which two expressions are computed can be seen.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Effect {
    /// Nothing: it reads what is there.
    Reads,
    /// It can stop the program: a checked operation.
    Stops,
    /// It calls a function, which can print or stop, but cannot change what its neighbours read.
    Acts,
    /// It can change a value that an expression beside it reads: it lends with `ref`.
    Changes,
}

/// One of several values that C would compute in an order of its own choosing: the arguments
/// of a call, the two sides of an operator, the fields of a literal.
struct Part {
    /// The C type of the value; empty if it cannot be kept in a variable (`panic(...)`).
    ctype: String,
    value: String,
    effect: Effect,
    /// A constant, or already kept in a variable: when it is computed cannot matter.
    settled: bool,
    /// The lines `pre` had before and after the value was written.
    pre: (usize, usize),
    /// Computing it can leave the function: it holds a `try` or a `?? return`.
    leaves: bool,
    span: Span,
}

/// Where the value of a `match` goes.
#[derive(Clone)]
enum Dest {
    /// Nowhere: the `match` is a statement.
    Unused,
    /// Into a C variable that is already declared, as a value of this type.
    Place(String, Ty),
    /// Out of the function: `return match ...`.
    Return,
}

/// A variable of the function being written.
struct Local {
    name: String,
    /// Its name in C: no two variables of a function share one, so an inner variable never
    /// hides an outer one that still has to be dropped.
    cname: String,
    ty: Ty,
    /// The value was moved out on every way that leads here, so there is nothing to drop.
    /// Only ever set when that is certain; when in doubt the drop is written and finds the
    /// place empty.
    gone: bool,
}

struct Emitter<'a> {
    src: &'a str,
    path: &'a str,
    structs: HashMap<&'a str, &'a StructDecl>,
    enums: HashMap<&'a str, &'a EnumDecl>,
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
    /// How many expressions that run only sometimes are being written (the right side of
    /// `??`, `&&` and `||`): a move in there is not certain.
    maybe: usize,
    /// The expression about to be written is computed while nothing that would have to be
    /// freed is waiting in its statement: no value that owns something has been computed
    /// before it and is still unnamed. Only there can `try` and `?? return` leave the function,
    /// because what they free on the way out is exactly what has a name: the variables, and
    /// the temporaries made so far. `value` reads it and clears it; an expression hands it on
    /// to the part of itself that is computed first.
    head: bool,
    /// How many ways out of the function were written inside expressions.
    exits: usize,
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
            Ty::Enum(name) => format!("E_{name}"),
            Ty::Result(..) => {
                let Some(name) = mangle(ty) else { return Err(not_yet(span, &format!("the type `{ty}`"))) };
                self.need(ty, span)?;
                format!("sg_{name}")
            }
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
            // The same shape as an enum with the variants `Ok(T value)` and `Error(E cause)`,
            // so `match` and `is` read both in one way.
            Ty::Result(ok, error) => {
                if !self.made.insert(name.clone()) {
                    return Ok(());
                }
                let (mut members, mut drops) = (String::new(), Vec::new());
                for (index, (field, held)) in [("value", ok), ("cause", error)].into_iter().enumerate() {
                    if **held == Ty::Void {
                        continue;
                    }
                    let _ = write!(members, "struct {{ {} m_{field}; }} v{index}; ", self.ctype(held, span)?);
                    if self.droppable(held) {
                        drops.push(format!("if (v->tag == {index}) {{ {} }}", self.drop_of(held, &format!("v->u.v{index}.m_{field}"))));
                    }
                }
                let _ = writeln!(self.types, "typedef struct {{ uint8_t tag; union {{ {members}}} u; }} sg_{name};");
                if !drops.is_empty() {
                    let _ = writeln!(self.glue_protos, "static inline void sg_drop_{name}(sg_{name}* v);\nstatic inline sg_{name} sg_move_{name}(sg_{name}* v);");
                    let _ = writeln!(self.glue, "static inline void sg_drop_{name}(sg_{name}* v) {{ {} }}", drops.join(" else "));
                    let _ = writeln!(self.glue, "static inline sg_{name} sg_move_{name}(sg_{name}* v) {{ sg_{name} out = *v; v->tag = {MOVED_TAG}; return out; }}");
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// A type as it is written in a signature or a field.
    fn written(&self, ty: &TypeExpr) -> Res<Ty> {
        let unknown = || not_yet(ty.span, &format!("the type `{}`", self.text(ty.span)));
        let mut found = match (Ty::from_name(&ty.name), ty.args.as_slice()) {
            (None, [ok, error]) if ty.name == "Result" => Ty::Result(Box::new(self.written(ok)?), Box::new(self.written(error)?)),
            (Some(base @ (Ty::Int(_) | Ty::Bool | Ty::Void | Ty::Str)), []) => base,
            (None, []) if self.structs.contains_key(ty.name.as_str()) => Ty::Struct(ty.name.clone()),
            (None, []) if self.enums.contains_key(ty.name.as_str()) => Ty::Enum(ty.name.clone()),
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
            Ty::Result(ok, error) => self.droppable(ok) || self.droppable(error),
            Ty::Struct(name) => self.structs[name.as_str()].fields.iter().any(|field| self.written(&field.ty).is_ok_and(|ty| self.droppable(&ty))),
            Ty::Enum(name) => {
                let mut fields = self.enums[name.as_str()].variants.iter().flat_map(|variant| &variant.fields);
                fields.any(|field| self.written(&field.ty).is_ok_and(|ty| self.droppable(&ty)))
            }
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

    /// What `match` and `is` see of an enum or a `Result`: for each variant, the name and the
    /// type of every value it carries. `None` for any other type. `span`: where to point for
    /// a `Result`, whose variants are written nowhere.
    fn variants(&self, ty: &Ty, span: Span) -> Res<Option<Vec<Carried>>> {
        match ty {
            Ty::Enum(name) => {
                let decl = self.enums[name.as_str()];
                let fields = |variant: &VariantDecl| variant.fields.iter().map(|field| Ok((field.name.clone(), self.written(&field.ty)?, field.ty.span))).collect();
                Ok(Some(decl.variants.iter().map(fields).collect::<Res<_>>()?))
            }
            Ty::Result(ok, error) => {
                let value = if **ok == Ty::Void { Vec::new() } else { vec![("value".to_string(), (**ok).clone(), span)] };
                Ok(Some(vec![value, vec![("cause".to_string(), (**error).clone(), span)]]))
            }
            _ => Ok(None),
        }
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
            let mut held = Vec::new();
            held_types(&ty, &mut held);
            for inner in held {
                self.type_decl(inner, done)?;
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

    /// The struct or the enum of this name, written before what holds it.
    fn type_decl(&mut self, name: &str, done: &mut Vec<&'a str>) -> Res<()> {
        if let Some(decl) = self.structs.get(name).copied() {
            return self.struct_decl(decl, done);
        }
        let decl: &'a EnumDecl = self.enums[name];
        self.enum_decl(decl, done)
    }

    /// An enum: a tag that says which variant it is and, for the variants that carry values,
    /// a union with room for the largest of them.
    fn enum_decl(&mut self, decl: &'a EnumDecl, done: &mut Vec<&'a str>) -> Res<()> {
        if done.contains(&decl.name.as_str()) {
            return Ok(());
        }
        done.push(&decl.name);
        if let Some((_, span)) = decl.sources.first() {
            return Err(not_yet(*span, "a type that carries a borrow"));
        }
        if decl.variants.len() >= MOVED_TAG {
            return Err(not_yet(decl.name_span, "an enum with this many variants"));
        }
        let name = &decl.name;
        let (mut members, mut cases) = (String::new(), String::new());
        for (index, variant) in decl.variants.iter().enumerate() {
            let (mut fields, mut drops) = (String::new(), String::new());
            for field in &variant.fields {
                let ty = self.written(&field.ty)?;
                if ty.is_borrow() {
                    return Err(not_yet(field.ty.span, "a field that borrows"));
                }
                let mut held = Vec::new();
                held_types(&ty, &mut held);
                for inner in held {
                    self.type_decl(inner, done)?;
                }
                let _ = write!(fields, "{} m_{}; ", self.ctype(&ty, field.ty.span)?, field.name);
                if self.droppable(&ty) {
                    let _ = write!(drops, "{} ", self.drop_of(&ty, &format!("v->u.v{index}.m_{}", field.name)));
                }
            }
            if !fields.is_empty() {
                let _ = writeln!(members, "        struct {{ {fields}}} v{index};");
            }
            if !drops.is_empty() {
                let _ = writeln!(cases, "    case {index}: {drops}break;");
            }
        }
        let union = if members.is_empty() { String::new() } else { format!("    union {{\n{members}    }} u;\n") };
        let _ = writeln!(self.types, "struct E_{name} {{\n    uint8_t tag;\n{union}}};");
        // What frees the values of the variant it is, and what takes the enum out of a place:
        // the place is left with a tag that is no variant.
        if !cases.is_empty() {
            let _ = writeln!(self.glue_protos, "static void sg_drop_E_{name}(E_{name}* v);\nstatic inline E_{name} sg_move_E_{name}(E_{name}* v);");
            let _ = writeln!(self.glue, "static void sg_drop_E_{name}(E_{name}* v) {{\n    switch (v->tag) {{\n{cases}    default: break;\n    }}\n}}");
            let _ = writeln!(self.glue, "static inline E_{name} sg_move_E_{name}(E_{name}* v) {{ E_{name} out = *v; v->tag = {MOVED_TAG}; return out; }}");
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
            scope.push(Local { name: "this".to_string(), cname: "v_this".to_string(), ty, gone: false });
        }
        for param in &func.params {
            let ty = self.written(&param.ty)?;
            params.push(format!("{} v_{}", self.ctype(&ty, param.ty.span)?, param.name));
            scope.push(Local { name: param.name.clone(), cname: format!("v_{}", param.name), ty, gone: false });
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
                if !local.gone && self.droppable(&local.ty) {
                    lines.push(self.drop_of(&local.ty, &local.cname));
                }
            }
        }
        lines
    }

    /// Which variables are known to be moved out, to come back to after a part of the
    /// program that runs only sometimes.
    fn gone(&self) -> Vec<Vec<bool>> {
        self.scopes.iter().map(|scope| scope.iter().map(|local| local.gone).collect()).collect()
    }

    /// After a block: a move made inside it is no longer certain; a value given inside it
    /// (`x = ...`) may be there.
    fn gone_only_if(&mut self, before: &[Vec<bool>]) {
        for (scope, before) in self.scopes.iter_mut().zip(before) {
            for (local, before) in scope.iter_mut().zip(before) {
                local.gone = local.gone && *before;
            }
        }
    }

    /// Back to what was known before a way out of the function was written.
    fn set_gone(&mut self, before: &[Vec<bool>]) {
        for (scope, before) in self.scopes.iter_mut().zip(before) {
            for (local, before) in scope.iter_mut().zip(before) {
                local.gone = *before;
            }
        }
    }

    /// A loop body runs again: what was moved before it may have a value by then.
    fn nothing_gone(&mut self) {
        for local in self.scopes.iter_mut().flatten() {
            local.gone = false;
        }
    }

    fn local_mut(&mut self, name: &str) -> &mut Local {
        self.scopes.iter_mut().rev().find_map(|scope| scope.iter_mut().rev().find(|local| local.name == name)).expect("names are checked before the back end runs")
    }

    fn local(&self, name: &str) -> &Local {
        self.scopes.iter().rev().find_map(|scope| scope.iter().rev().find(|local| local.name == name)).expect("names are checked before the back end runs")
    }

    fn declare(&mut self, name: &str, ty: Ty) -> String {
        self.temps += 1;
        let cname = format!("v_{name}_{}", self.temps);
        self.scopes.last_mut().expect("an open scope").push(Local { name: name.to_string(), cname: cname.clone(), ty, gone: false });
        cname
    }

    /// A block: its own scope, and at its end the drops of what it declared.
    fn block(&mut self, block: &'a Block, depth: usize, out: &mut String) -> Res<()> {
        self.block_with(block, depth, out, Vec::new())
    }

    /// A block that starts with names of its own: those of `if x is Circle(r)`. Each comes
    /// with the C line that declares it.
    fn block_with(&mut self, block: &'a Block, depth: usize, out: &mut String, names: Vec<(Local, String)>) -> Res<()> {
        let before = self.gone();
        self.scopes.push(Vec::new());
        for (local, line) in names {
            let _ = writeln!(out, "{}{line}", "    ".repeat(depth));
            self.scopes.last_mut().expect("an open scope").push(local);
        }
        self.stmts(block, depth, out)?;
        if !ends_by_leaving(block) {
            let pad = "    ".repeat(depth);
            for line in self.drops(self.scopes.len() - 1) {
                let _ = writeln!(out, "{pad}{line}");
            }
        }
        self.scopes.pop();
        self.gone_only_if(&before);
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
                let ctype = self.ctype(&ty, init.span)?;
                // A `match` writes its value into the variable; the variable is in scope only
                // after it, so an arm that leaves does not drop it.
                if matches!(init.kind, ExprKind::Match { .. }) {
                    self.temps += 1;
                    let cname = format!("v_{name}_{}", self.temps);
                    let _ = writeln!(out, "{pad}{ctype} {cname};");
                    self.match_stmt(init, &Dest::Place(cname.clone(), ty.clone()), depth, out)?;
                    self.scopes.last_mut().expect("an open scope").push(Local { name: name.clone(), cname, ty, gone: false });
                    return Ok(());
                }
                let value = self.head_value(init, &ty)?;
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
                let new = if matches!(value.kind, ExprKind::Match { .. }) {
                    let (ctype, temp) = (self.ctype(&held, value.span)?, self.temp());
                    let _ = writeln!(out, "{pad}{ctype} {temp};");
                    self.match_stmt(value, &Dest::Place(temp.clone(), held.clone()), depth, out)?;
                    temp
                } else {
                    self.head_value(value, &held)?
                };
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
                let (mut cond, names) = self.condition(cond)?;
                self.flush_pre(&pad, out);
                if !self.post.is_empty() {
                    let temp = self.temp();
                    let _ = writeln!(out, "{pad}bool {temp} = {cond};");
                    self.flush_post(&pad, out);
                    cond = temp;
                }
                let _ = writeln!(out, "{pad}if ({cond}) {{");
                self.block_with(then, depth + 1, out, names)?;
                if let Some(els) = els {
                    let _ = writeln!(out, "{pad}}} else {{");
                    self.block(els, depth + 1, out)?;
                }
                let _ = writeln!(out, "{pad}}}");
            }
            StmtKind::While { cond, body } => {
                self.nothing_gone();
                let (cond, names) = self.condition(cond)?;
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
                self.block_with(body, depth + 1, out, names)?;
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
                self.nothing_gone();
                self.loops.push(self.scopes.len());
                let _ = writeln!(out, "{pad}for (; {cname} < {end}; {cname}++) {{");
                self.block(body, depth + 1, out)?;
                self.loops.pop();
                self.scopes.pop();
                let _ = writeln!(out, "{pad}}}\n{pad}}}");
            }
            StmtKind::ForEach { iter, .. } => return Err(not_yet(iter.span, "a `for` over a collection")),
            StmtKind::Return(Some(value)) if matches!(value.kind, ExprKind::Match { .. }) => self.match_stmt(value, &Dest::Return, depth, out)?,
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
            StmtKind::Expr(expr) => self.expr_stmt(expr, depth, out)?,
        }
        Ok(())
    }

    /// An expression that stands as a statement.
    fn expr_stmt(&mut self, expr: &'a Expr, depth: usize, out: &mut String) -> Res<()> {
        let pad = "    ".repeat(depth);
        match &expr.kind {
            ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "print" => self.print(&args[0], &pad, out)?,
            ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "assert" => self.assert(expr, &args[0], &pad, out)?,
            ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "panic" => {
                let (format, values) = self.format(&args[0])?;
                let loc = self.loc(expr.span);
                self.flush_pre(&pad, out);
                let _ = writeln!(out, "{pad}sg_panicf({loc}, {}{values});", c_string(&format));
                self.post.clear();
            }
            ExprKind::Match { .. } => self.match_stmt(expr, &Dest::Unused, depth, out)?,
            _ => self.unused(expr, &pad, out)?,
        }
        Ok(())
    }

    /// The condition of an `if` or a `while`, and the names it gives to the block: those of
    /// `x is Circle(r)`.
    fn condition(&mut self, cond: &'a Expr) -> Res<(String, Vec<(Local, String)>)> {
        if !matches!(cond.kind, ExprKind::IsVariant { .. }) {
            return Ok((self.first(cond, true)?, Vec::new()));
        }
        let (test, names) = self.variant_test(cond)?;
        // The names point into the value: it has to be there for as long as the block runs.
        if !names.is_empty() && !self.post.is_empty() {
            return Err(not_yet(cond.span, "names taken from a value that has no name").hint("give the value a name first"));
        }
        Ok((test, names))
    }

    /// `x is Circle(r)`: a test on the tag. The names are borrows of what the variant carries.
    fn variant_test(&mut self, expr: &'a Expr) -> Res<(String, Vec<(Local, String)>)> {
        let ExprKind::IsVariant { expr: subject, negated, pattern, by_ref } = &expr.kind else {
            unreachable!("only called for an `is` test");
        };
        let PatternKind::Variant { index, args, .. } = &pattern.kind else {
            unreachable!("`is` only tests for a variant");
        };
        let mut place = self.look(subject)?;
        let mut ty = subject.ty.strip_borrow();
        while let Ty::Ptr(inner) = ty {
            place = format!("(*{place})");
            ty = inner;
        }
        let Some(variants) = self.variants(ty, subject.span)? else { return Err(not_yet(expr.span, &format!("`is` on `{ty}`"))) };
        let mut names = Vec::new();
        for (position, (name, _)) in args.iter().flatten().enumerate() {
            let Some(name) = name else { continue };
            let (field, held, span) = variants[*index][position].clone();
            let held = Box::new(held);
            let ty = if *by_ref { Ty::Ref(held) } else { Ty::Read(held) };
            let at = format!("{place}.u.v{index}.m_{field}");
            names.push(self.borrowed_name(name, ty, &at, span)?);
        }
        let op = if *negated { "!=" } else { "==" };
        Ok((format!("({place}.tag {op} {index})"), names))
    }

    /// A name that borrows what is at `at`, and the C line that declares it. The name is not
    /// in scope yet.
    fn borrowed_name(&mut self, name: &str, ty: Ty, at: &str, span: Span) -> Res<(Local, String)> {
        let ctype = self.ctype(&ty, span)?;
        self.temps += 1;
        let cname = format!("v_{name}_{}", self.temps);
        let init = if is_text_view(&ty) { format!("sg_view(&{at})") } else { format!("(&{at})") };
        let line = format!("{ctype} {cname} = {init};");
        Ok((Local { name: name.to_string(), cname, ty, gone: false }, line))
    }

    /// `match`, written as statements: a chain of tests on the tag (or on the number), each
    /// arm a block. An arm with `when` that does not hold goes on to the arms below it.
    ///
    /// A subject that is owned is taken apart: the names of an arm own what the variant
    /// carried, what the pattern skips is dropped, and a `T*` subject gives its box back. A
    /// subject that is borrowed is only looked at: the names are borrows.
    fn match_stmt(&mut self, expr: &'a Expr, dest: &Dest, depth: usize, out: &mut String) -> Res<()> {
        let ExprKind::Match { subject, arms, borrowed } = &expr.kind else {
            unreachable!("only called for a `match`");
        };
        let (pad, inner) = ("    ".repeat(depth), "    ".repeat(depth + 1));
        let value = self.first(subject, true)?;
        let ctype = self.ctype(&subject.ty, subject.span)?;
        let held = self.temp();
        self.flush_pre(&pad, out);
        let _ = writeln!(out, "{pad}{{");
        let _ = writeln!(out, "{inner}{ctype} {held} = {value};");
        if *borrowed && !self.post.is_empty() {
            return Err(not_yet(subject.span, "a `match` that borrows from a value that has no name").hint("give the value a name first"));
        }
        self.flush_post(&inner, out);

        // What the arms test: a number, or the enum behind the borrow or the pointer.
        let mut core = subject.ty.strip_borrow();
        let mut boxes = 0;
        while let Ty::Ptr(behind) = core {
            (core, boxes) = (behind, boxes + 1);
        }
        let number = match core {
            Ty::Int(ty) => Some((if subject.ty.is_borrow() { format!("(*{held})") } else { held.clone() }, *ty)),
            _ => None,
        };
        let variants = self.variants(core, subject.span)?;
        if (number.is_none() && variants.is_none()) || (number.is_some() && boxes > 0) {
            return Err(not_yet(subject.span, &format!("a `match` on `{}`", subject.ty)));
        }
        // The enum as C reaches it: through a pointer in every case.
        let at = match (*borrowed, boxes) {
            (true, 0) | (false, 1) => held.clone(),
            (true, 1) => format!("(*{held})"),
            (false, 0) => format!("(&{held})"),
            _ => return Err(not_yet(subject.span, &format!("a `match` on `{}`", subject.ty))),
        };
        // `read` if the value is reached through a `read` anywhere, otherwise `ref`.
        let mode = match (*borrowed, &subject.ty) {
            (false, _) => None,
            (true, Ty::Ref(behind)) if !behind.is_borrow() => Some(Mode::Ref),
            (true, _) => Some(Mode::Read),
        };

        let guarded = arms.iter().any(|arm| arm.guard.is_some());
        let end = format!("{held}_end");
        for (position, arm) in arms.iter().enumerate() {
            let last = position + 1 == arms.len();
            let test = match (&arm.pattern.kind, &number) {
                (PatternKind::Variant { index, .. }, _) => Some(format!("{at}->tag == {index}")),
                (PatternKind::Int(wanted), Some((number, ty))) => Some(match *wanted {
                    i64::MIN => format!("{number} == (({})(-9223372036854775807LL - 1))", int_ctype(*ty)),
                    wanted => format!("{number} == (({}){wanted}LL)", int_ctype(*ty)),
                }),
                _ => None,
            };
            // Every value has an arm (the checker saw to it), so the last one needs no test.
            let test = test.filter(|_| !(last && arm.guard.is_none()));
            let _ = match (&test, guarded || position == 0) {
                (Some(test), true) => writeln!(out, "{inner}if ({test}) {{"),
                (Some(test), false) => writeln!(out, "{inner}}} else if ({test}) {{"),
                (None, true) => writeln!(out, "{inner}{{"),
                (None, false) => writeln!(out, "{inner}}} else {{"),
            };
            let mut body = depth + 2;
            let mut body_pad = "    ".repeat(body);
            let before = self.gone();
            self.scopes.push(Vec::new());

            // The names of the pattern, and what an owned subject still has to give up once
            // the arm is chosen.
            let mut chosen = Vec::new();
            match &arm.pattern.kind {
                PatternKind::Bind(name) => {
                    let cname = self.declare(name, subject.ty.clone());
                    let _ = writeln!(out, "{body_pad}{ctype} {cname} = {held};");
                }
                PatternKind::Wildcard if !*borrowed && self.droppable(&subject.ty) => chosen.push(self.drop_of(&subject.ty, &held)),
                PatternKind::Variant { index, args, .. } => {
                    let fields = &variants.as_ref().expect("the checker allows a variant only for an enum")[*index];
                    for (place, (field, ty, span)) in fields.iter().cloned().enumerate() {
                        let value = format!("{at}->u.v{index}.m_{field}");
                        let name = args.as_ref().and_then(|args| args[place].0.as_ref());
                        match (name, mode) {
                            (Some(name), None) => {
                                let ctype = self.ctype(&ty, span)?;
                                let cname = self.declare(name, ty);
                                let _ = writeln!(out, "{body_pad}{ctype} {cname} = {value};");
                            }
                            (Some(name), Some(mode)) => {
                                let ty = if mode == Mode::Ref { Ty::Ref(Box::new(ty)) } else { Ty::Read(Box::new(ty)) };
                                let (local, line) = self.borrowed_name(name, ty, &value, span)?;
                                let _ = writeln!(out, "{body_pad}{line}");
                                self.scopes.last_mut().expect("an open scope").push(local);
                            }
                            (None, None) if self.droppable(&ty) => chosen.push(self.drop_of(&ty, &value)),
                            (None, _) => {}
                        }
                    }
                    if !*borrowed && boxes == 1 {
                        chosen.push(format!("free({held});"));
                    }
                }
                _ => {}
            }

            if let Some(guard) = &arm.guard {
                let mut cond = self.value(guard)?;
                self.flush_pre(&body_pad, out);
                if !self.post.is_empty() {
                    let temp = self.temp();
                    let _ = writeln!(out, "{body_pad}bool {temp} = {cond};");
                    self.flush_post(&body_pad, out);
                    cond = temp;
                }
                let _ = writeln!(out, "{body_pad}if ({cond}) {{");
                body += 1;
                body_pad = "    ".repeat(body);
            }
            for line in chosen {
                let _ = writeln!(out, "{body_pad}{line}");
            }
            self.stmts(&arm.body, body, out)?;
            if let Some(value) = &arm.value {
                self.arm_value(value, dest, body, out)?;
            }
            // An arm that leaves (or returns its value) has dropped what it had on the way out.
            let leaves = match &arm.value {
                Some(_) => matches!(dest, Dest::Return),
                None => ends_by_leaving(&arm.body),
            };
            if !leaves {
                for line in self.drops(self.scopes.len() - 1) {
                    let _ = writeln!(out, "{body_pad}{line}");
                }
                if guarded {
                    let _ = writeln!(out, "{body_pad}goto {end};");
                }
            }
            self.scopes.pop();
            self.gone_only_if(&before);
            if arm.guard.is_some() {
                let _ = writeln!(out, "{}}}", "    ".repeat(body - 1));
            }
            if guarded {
                let _ = writeln!(out, "{inner}}}");
            }
        }
        if guarded {
            let _ = writeln!(out, "{inner}{end}:;");
        } else {
            let _ = writeln!(out, "{inner}}}");
        }
        let _ = writeln!(out, "{pad}}}");
        Ok(())
    }

    /// The value an arm of a `match` ends with, sent where the value of the `match` goes.
    fn arm_value(&mut self, value: &'a Expr, dest: &Dest, depth: usize, out: &mut String) -> Res<()> {
        let pad = "    ".repeat(depth);
        match dest {
            // A `match` on the last line of an arm gives its value to the same place.
            _ if matches!(value.kind, ExprKind::Match { .. }) => self.match_stmt(value, dest, depth, out)?,
            // `panic(...)` gives no value: the program stops there.
            _ if value.ty == Ty::Never => self.expr_stmt(value, depth, out)?,
            Dest::Unused => self.expr_stmt(value, depth, out)?,
            Dest::Place(place, ty) => {
                let value = self.head_value(value, ty)?;
                self.flush_pre(&pad, out);
                let _ = writeln!(out, "{pad}{place} = {value};");
                self.flush_post(&pad, out);
            }
            Dest::Return => {
                for line in self.return_lines(Some(value), &[], value.span)? {
                    let _ = writeln!(out, "{pad}{line}");
                }
            }
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
    /// it goes. Nothing else of the statement has been computed yet, so a `try` or a
    /// `?? return` in it can leave the function and leave nothing behind.
    fn head_value(&mut self, expr: &'a Expr, want: &Ty) -> Res<String> {
        self.first_as(expr, want, true)
    }

    /// The value of the part of an expression that is computed first. `head`: whether the
    /// expression itself stood where leaving the function is possible (see `Emitter::head`).
    fn first(&mut self, expr: &'a Expr, head: bool) -> Res<String> {
        self.head = head;
        let value = self.value(expr);
        self.head = false;
        value
    }

    fn first_as(&mut self, expr: &'a Expr, want: &Ty, head: bool) -> Res<String> {
        self.head = head;
        let value = self.value_as(expr, want);
        self.head = false;
        value
    }

    /// Opens a `T?` or a `Result` kept in `temp`: gives what it holds, or runs `exit` (which
    /// leaves the function) when there is no value.
    fn open(&mut self, ty: &Ty, temp: &str, value: String, exit: Vec<String>, span: Span) -> Res<String> {
        let ctype = self.ctype(ty, span)?;
        let (test, held) = match ty {
            // Success that carries nothing gives nothing: the expression is a `void` one.
            Ty::Result(ok, _) if **ok == Ty::Void => (format!("{temp}.tag == 0"), String::new()),
            Ty::Result(..) => (format!("{temp}.tag == 0"), format!("{temp}.u.v0.m_value")),
            Ty::Nullable(_) if is_option(ty) => (format!("{temp}.has"), format!("{temp}.value")),
            Ty::Nullable(_) => (temp.to_string(), temp.to_string()),
            _ => return Err(not_yet(span, &format!("opening a `{ty}`"))),
        };
        self.exits += 1;
        Ok(format!("({{ {ctype} {temp} = {value}; if (!({test})) {{ {} }} {held}; }})", exit.join(" ")))
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
    fn call(&mut self, name: String, receiver: Option<Part>, args: &'a [Expr], head: bool) -> Res<String> {
        let params = self.sigs.get(&name).cloned().unwrap_or_default();
        // An argument can leave the function for as long as no argument before it owns anything.
        let mut head = head && receiver.is_none();
        let mut parts: Vec<Part> = receiver.into_iter().collect();
        for (position, arg) in args.iter().enumerate() {
            let before = self.mark();
            let (value, ty) = match params.get(position) {
                Some(want) => (self.first_as(arg, want, head)?, want.clone()),
                None => (self.first(arg, head)?, arg.ty.clone()),
            };
            head = head && !self.droppable(&ty);
            parts.push(self.part(arg, &ty, value, before));
        }
        let (values, names) = self.in_order(parts)?;
        Ok(sequence(&names, format!("{name}({})", values.join(", "))))
    }

    /// What computing the expression can do besides giving its value.
    fn effect(&self, expr: &Expr) -> Effect {
        let of = |inner: &Expr| self.effect(inner);
        let all = |list: &mut dyn Iterator<Item = &Expr>| list.map(|inner| self.effect(inner)).max().unwrap_or(Effect::Reads);
        match &expr.kind {
            ExprKind::Int(..) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Char(_) | ExprKind::Null | ExprKind::Var(_) | ExprKind::FuncRef(_) => Effect::Reads,
            ExprKind::Field { recv: inner, .. }
            | ExprKind::New(inner)
            | ExprKind::Load(inner)
            | ExprKind::Unbox(inner)
            | ExprKind::Widen(_, inner)
            | ExprKind::IsNull { expr: inner, .. }
            | ExprKind::IsVariant { expr: inner, .. }
            | ExprKind::Unary(UnOp::Not, inner) => of(inner),
            ExprKind::Unary(UnOp::Neg, inner) | ExprKind::Cast { expr: inner, .. } => of(inner).max(if self.checked { Effect::Stops } else { Effect::Reads }),
            ExprKind::Binary(op, left, right) => {
                let own = match op {
                    BinOp::Div | BinOp::Rem => Effect::Stops,
                    BinOp::Add | BinOp::Sub | BinOp::Mul if self.checked => Effect::Stops,
                    _ => Effect::Reads,
                };
                own.max(of(left)).max(of(right))
            }
            ExprKind::Index { recv, index, .. } => Effect::Stops.max(of(recv)).max(of(index)),
            ExprKind::Slice { recv, from, to } => Effect::Stops.max(of(recv)).max(all(&mut from.iter().chain(to).map(|inner| &**inner))),
            ExprKind::Interp(parts) => all(&mut parts.iter().filter_map(|part| match part {
                InterpPart::Expr(inner) => Some(inner),
                InterpPart::Text(_) => None,
            })),
            ExprKind::StructLit { fields, .. } => all(&mut fields.iter().map(|field| &field.value)),
            ExprKind::Variant { args, .. } => all(&mut args.iter()),
            ExprKind::Borrow { mutable: true, .. } => Effect::Changes,
            ExprKind::Borrow { expr: inner, .. } => of(inner),
            ExprKind::Coalesce { left, right, .. } => of(left).max(of(right)),
            ExprKind::Call { args, .. } => Effect::Acts.max(all(&mut args.iter())),
            ExprKind::Method { recv, name, args, callee, .. } => {
                let own = match callee {
                    Callee::Method { this: This::Ref, .. } | Callee::Take | Callee::Replace => Effect::Changes,
                    // The built-in methods of numbers and text compute a value and nothing else.
                    Callee::Plain if matches!(recv.ty.strip_borrow(), Ty::List(_) | Ty::Map(..) | Ty::Set(_) | Ty::Formatter) => Effect::Changes,
                    Callee::Plain if name == "clone" || name == "toString" => Effect::Acts,
                    Callee::Plain => Effect::Reads,
                    _ => Effect::Acts,
                };
                own.max(of(recv)).max(all(&mut args.iter()))
            }
            ExprKind::Try { .. } | ExprKind::Return(_) => Effect::Acts,
            ExprKind::Match { .. } | ExprKind::Collection { .. } => Effect::Changes,
        }
    }

    /// Where the writing of a part starts: how many lines `pre` has, and how many ways out
    /// of the function were written.
    fn mark(&self) -> (usize, usize) {
        (self.pre.len(), self.exits)
    }

    /// One of the values of a call, an operator or a literal. `before`: the `mark` taken
    /// when the value started to be written.
    fn part(&mut self, expr: &'a Expr, ty: &Ty, value: String, before: (usize, usize)) -> Part {
        let ctype = match (&expr.ty, ty) {
            (Ty::Never | Ty::Void, _) | (_, Ty::Never | Ty::Void | Ty::Null) => String::new(),
            _ => self.ctype(ty, expr.span).unwrap_or_default(),
        };
        Part { ctype, value, effect: self.effect(expr), settled: is_constant(expr), pre: (before.0, self.pre.len()), leaves: self.exits > before.1, span: expr.span }
    }

    /// Makes sure the parts are computed from left to right where the order can be seen: C
    /// leaves the order of the arguments of a call and of the operands of an operator open.
    /// Gives the parts as they are to be written, and the C declarations that have to run
    /// first (see `sequence`), which keep the earlier parts in variables.
    ///
    /// The order can be seen when two parts can each stop the program or call a function, or
    /// when one part can change what another reads. A part that put lines before the statement
    /// (a value kept in a temporary) is already computed there, so the parts before it go
    /// there too.
    ///
    /// A part that can leave the function frees the temporaries made before it on its way
    /// out, so it has to run before the lines of the parts after it; moving it there is the
    /// same step.
    fn in_order(&mut self, mut parts: Vec<Part>) -> Res<(Vec<String>, String)> {
        let values = |parts: &[Part]| parts.iter().map(|part| part.value.clone()).collect::<Vec<_>>();
        if parts.iter().any(|part| part.ctype.is_empty()) {
            // Nothing can be moved here; then nothing may need to be.
            if let Some(leaving) = parts.iter().position(|part| part.leaves) {
                if parts[leaving + 1..].iter().any(|part| part.pre.1 > part.pre.0) {
                    return Err(not_yet(parts[leaving].span, "leaving the function from here").hint("give the value a name first: `var x = try ...`"));
                }
            }
            return Ok((values(&parts), String::new()));
        }
        if let Some(last) = parts.iter().rposition(|part| part.pre.1 > part.pre.0) {
            for (added, part) in parts[..last].iter_mut().filter(|part| !part.settled).enumerate() {
                let temp = self.temp();
                self.pre.insert(part.pre.1 + added, format!("{} {temp} = {};", part.ctype, part.value));
                (part.value, part.settled, part.effect) = (temp, true, Effect::Reads);
            }
        }
        let open: Vec<usize> = (0..parts.len()).filter(|index| !parts[*index].settled).collect();
        let seen = open.iter().filter(|index| parts[**index].effect >= Effect::Stops).count() >= 2
            || (open.len() >= 2 && open.iter().any(|index| parts[*index].effect == Effect::Changes));
        let mut names = String::new();
        if seen {
            let last_effect = open.iter().rposition(|index| parts[*index].effect >= Effect::Stops).expect("an order that can be seen has an effect");
            // The last part can stay where it is: everything before it is already computed.
            let keep = if last_effect + 1 == open.len() { last_effect } else { last_effect + 1 };
            for index in &open[..keep] {
                let temp = self.temp();
                let _ = write!(names, "{} {temp} = {}; ", parts[*index].ctype, parts[*index].value);
                parts[*index].value = temp;
            }
        }
        Ok((values(&parts), names))
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
                // The values are given to `printf` as arguments, and C computes those in an
                // order of its own. Where the order can be seen, each is computed on a line
                // of its own first.
                let exprs: Vec<&Expr> = parts.iter().filter_map(|part| match part {
                    InterpPart::Expr(expr) if !matches!(expr.kind, ExprKind::Str(_)) => Some(expr),
                    _ => None,
                }).collect();
                let effects: Vec<Effect> = exprs.iter().map(|expr| self.effect(expr)).collect();
                let changes = effects.iter().position(|effect| *effect == Effect::Changes);
                let first = effects.iter().filter(|effect| **effect >= Effect::Stops).count() >= 2 || (changes.is_some() && exprs.len() >= 2);
                // A text is looked at where it is; a later value that could change it would
                // leave the look pointing at freed bytes.
                if let Some(changes) = changes {
                    if let Some(text) = exprs[..changes].iter().find(|expr| is_text(&expr.ty) || is_text_behind_pointer(&expr.ty)) {
                        return Err(not_yet(text.span, "a text written before a value that lends with `ref` on the same line")
                            .hint("give the later value a name first"));
                    }
                }
                for part in parts {
                    match part {
                        InterpPart::Text(text) => format.push_str(&text.replace('%', "%%")),
                        InterpPart::Expr(expr) => {
                            let from = values.len();
                            self.formatted(expr, &mut format, &mut values)?;
                            // A number or a `bool`: one value. (A text is already kept before the line.)
                            let text = is_text(&expr.ty) || is_text_behind_pointer(&expr.ty);
                            if first && !text && values.len() == from + 1 {
                                let (ctype, temp) = (if values[from].ends_with("\"false\"") { "const char*" } else { "long long" }, self.temp());
                                self.pre.push(format!("{ctype} {temp} = {};", values[from]));
                                values[from] = temp;
                            }
                        }
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
        // One value is the whole line: it can be `try ...`.
        self.head = !matches!(arg.kind, ExprKind::Interp(_));
        let formatted = self.format(arg);
        self.head = false;
        let (mut format, values) = formatted?;
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
                // Whatever is written here, the variable holds a value afterwards.
                self.local_mut(name).gone = false;
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
        if let ExprKind::Var(name) = &place.kind {
            self.local_mut(name).gone = false;
        }
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

    /// Two values that are computed one after the other, the left one first.
    fn two(&mut self, left: &'a Expr, right: &'a Expr, mut head: bool) -> Res<(Vec<String>, String)> {
        let mut parts = Vec::new();
        for side in [left, right] {
            let before = self.mark();
            let value = self.first(side, head)?;
            head = head && !self.droppable(&side.ty);
            parts.push(self.part(side, &side.ty, value, before));
        }
        self.in_order(parts)
    }

    /// The value of an expression as C writes it. A use that the checker marked as a move
    /// takes the value out of its place and leaves the place empty.
    fn value(&mut self, expr: &'a Expr) -> Res<String> {
        let head = std::mem::replace(&mut self.head, false);
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
                // A whole variable moved on a line that always runs: its drop is not written.
                if let (ExprKind::Var(name), 0) = (&expr.kind, self.maybe) {
                    let local = self.local_mut(name);
                    local.gone = !local.ty.is_borrow();
                }
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
            ExprKind::Load(inner) => Ok(format!("(*{})", self.first(inner, head)?)),
            // A borrow of a `T*` used as a borrow of the `T`: one step along the pointer.
            ExprKind::Unbox(inner) => Ok(format!("(*{})", self.value(inner)?)),
            ExprKind::New(inner) => {
                let Ty::Ptr(target) = &expr.ty else { return Err(not_yet(expr.span, "this `new`")) };
                self.ctype(&expr.ty, expr.span)?;
                let value = self.first_as(inner, target, head)?;
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
            // `x ?? return y`: with no value the function ends here.
            ExprKind::Coalesce { left, right, on_result } if matches!(right.kind, ExprKind::Return(_)) => {
                let ExprKind::Return(returned) = &right.kind else { unreachable!("matched above") };
                if !head {
                    return Err(not_yet(expr.span, "`?? return` in this part of a statement")
                        .hint("give the value a name first: `var x = ... ?? return ...`"));
                }
                let value = self.first(left, true)?;
                let temp = self.temp();
                // The `return` has its own temporaries; the ones made so far are dropped on its way out.
                let (pre, post) = (std::mem::take(&mut self.pre), std::mem::take(&mut self.post));
                // The error of a `Result` is dropped: the `return` gives one of its own.
                let mut pending = Vec::new();
                if let (true, Ty::Result(_, error)) = (*on_result, &left.ty) {
                    if self.droppable(error) {
                        pending.push(self.drop_of(error, &format!("{temp}.u.v1.m_cause")));
                    }
                }
                pending.extend(post.iter().cloned());
                // What the `return` moves is moved only on that way out.
                let before = self.gone();
                let exit = self.return_lines(returned.as_deref(), &pending, right.span);
                self.set_gone(&before);
                (self.pre, self.post) = (pre, post);
                self.open(&left.ty, &temp, value, exit?, left.span)
            }
            // The right side runs only when there is no value. On a `Result` the error is dropped first.
            ExprKind::Coalesce { left, right, on_result } => {
                let error = match (&left.ty, on_result) {
                    (Ty::Result(_, error), true) => Some(&**error),
                    (Ty::Nullable(_), false) => None,
                    _ => return Err(not_yet(left.span, &format!("`??` on `{}`", left.ty))),
                };
                let value = self.first(left, head)?;
                let before = (self.pre.len(), self.post.len());
                self.maybe += 1;
                let fallback = self.value(right);
                self.maybe -= 1;
                let fallback = fallback?;
                if before != (self.pre.len(), self.post.len()) {
                    return Err(not_yet(right.span, "a value that needs a temporary on the right of `??`"));
                }
                let (ctype, temp) = (self.ctype(&left.ty, left.span)?, self.temp());
                let (holds, held) = match &left.ty {
                    Ty::Result(..) => (format!("{temp}.tag == 0"), format!("{temp}.u.v0.m_value")),
                    ty if is_option(ty) => (format!("{temp}.has"), format!("{temp}.value")),
                    _ => (temp.clone(), temp.clone()),
                };
                // A right side that never gives a value (`panic`) still has to have the type in C.
                let fallback = if right.ty == Ty::Never { format!("({fallback}, {held})") } else { fallback };
                let fallback = match error.filter(|error| self.droppable(error)) {
                    Some(error) => format!("({{ {} {fallback}; }})", self.drop_of(error, &format!("{temp}.u.v1.m_cause"))),
                    None => fallback,
                };
                Ok(format!("({{ {ctype} {temp} = {value}; {holds} ? {held} : {fallback}; }})"))
            }
            // `try e`: with no value the function ends here, and gives its caller `null` or the error.
            ExprKind::Try { expr: inner, kind } => {
                if !head {
                    return Err(not_yet(expr.span, "`try` in this part of a statement")
                        .hint("give the value a name first: `var x = try ...`"));
                }
                let value = self.first(inner, true)?;
                let temp = self.temp();
                let ret = self.ret.clone();
                let returned = match kind {
                    TryKind::Null => self.lift(String::new(), &Ty::Null, &ret, expr.span)?,
                    TryKind::Result(convert) => {
                        if !matches!(ret, Ty::Result(..)) {
                            return Err(not_yet(expr.span, "`try` on a `Result` here"));
                        }
                        // The error goes to the caller as it is, or inside the variant of
                        // the caller's error type that carries it.
                        let cause = format!("{temp}.u.v1.m_cause");
                        let cause = match convert {
                            Some((owner, index)) => format!("((E_{owner}){{ .tag = {index}, .u.v{index} = {{ {cause} }} }})"),
                            None => cause,
                        };
                        format!("(({}){{ .tag = 1, .u.v1 = {{ {cause} }} }})", self.ctype(&ret, expr.span)?)
                    }
                };
                // What is returned is made of the opened value alone, so it can be written last.
                let mut exit = self.post.clone();
                exit.extend(self.drops(0));
                exit.push(format!("return {returned};"));
                self.open(&inner.ty, &temp, value, exit, inner.span)
            }
            ExprKind::Widen(ty, inner) => Ok(format!("(({}){})", int_ctype(*ty), self.first(inner, head)?)),
            ExprKind::Borrow { expr: inner, .. } if is_text_view(&expr.ty) => self.view(inner),
            ExprKind::Borrow { expr: inner, .. } if inner.is_place() => self.address(inner),
            // A value that has no name, lent: it is kept until the statement ends.
            ExprKind::Borrow { expr: inner, .. } if !inner.ty.is_borrow() => Ok(format!("(&{})", self.look(inner)?)),
            // A value of an enum, and `Ok(...)` and `Error(...)` of a `Result`.
            ExprKind::Variant { index, args, .. } => {
                let Some(variants) = self.variants(&expr.ty, expr.span)? else { return Err(not_yet(expr.span, &format!("a value of `{}`", expr.ty))) };
                let ctype = self.ctype(&expr.ty, expr.span)?;
                let mut parts = Vec::new();
                let mut head = head;
                for (arg, (_, want, _)) in args.iter().zip(&variants[*index]) {
                    let before = self.mark();
                    let value = self.first_as(arg, want, head)?;
                    head = head && !self.droppable(want);
                    parts.push(self.part(arg, want, value, before));
                }
                let (values, names) = self.in_order(parts)?;
                Ok(sequence(&names, match values.is_empty() {
                    true => format!("(({ctype}){{ .tag = {index} }})"),
                    false => format!("(({ctype}){{ .tag = {index}, .u.v{index} = {{ {} }} }})", values.join(", ")),
                }))
            }
            ExprKind::IsVariant { .. } => Ok(self.variant_test(expr)?.0),
            ExprKind::Match { .. } => Err(not_yet(expr.span, "a `match` inside a larger expression")
                .hint("give the value a name first: `var x = match ...`")),
            ExprKind::Unary(UnOp::Not, operand) => Ok(format!("(!{})", self.first(operand, head)?)),
            ExprKind::Unary(UnOp::Neg, operand) => {
                let Ty::Int(ty) = &expr.ty else { return Err(not_yet(expr.span, "this operator")) };
                let operand = self.first(operand, head)?;
                if !self.checked {
                    return Ok(format!("sg_wsub_{0}(({1})0, {operand})", int_name(*ty), int_ctype(*ty)));
                }
                let loc = self.loc(expr.span);
                Ok(format!("sg_neg_{}({operand}, {loc})", int_name(*ty)))
            }
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), left, right) => {
                let left = self.first(left, head)?;
                // The right side runs only sometimes, so it cannot need a line before the statement.
                let before = (self.pre.len(), self.post.len());
                self.maybe += 1;
                let right_value = self.value(right);
                self.maybe -= 1;
                let right_value = right_value?;
                if before != (self.pre.len(), self.post.len()) {
                    return Err(not_yet(right.span, &format!("a value that needs a temporary on the right of `{}`", op.symbol())));
                }
                Ok(format!("({left} {} {right_value})", op.symbol()))
            }
            ExprKind::Binary(op, left, right) if is_text(&left.ty) => {
                let view = Ty::Read(Box::new(Ty::Str));
                let mut parts = Vec::new();
                for side in [left, right] {
                    let before = self.mark();
                    let value = self.view(side)?;
                    parts.push(self.part(side, &view, value, before));
                }
                let (values, names) = self.in_order(parts)?;
                let (left, right) = (&values[0], &values[1]);
                Ok(sequence(&names, match op {
                    BinOp::Eq => format!("sg_eq({left}, {right})"),
                    BinOp::Ne => format!("(!sg_eq({left}, {right}))"),
                    _ => format!("(sg_cmp({left}, {right}) {} 0)", op.symbol()),
                }))
            }
            ExprKind::Binary(op, left, right) => {
                let operands = left.ty.strip_borrow().clone();
                let (values, names) = self.two(left, right, head)?;
                let (left, right) = (&values[0], &values[1]);
                match (&operands, op.is_comparison()) {
                    (Ty::Int(_) | Ty::Bool, true) => Ok(sequence(&names, format!("({left} {} {right})", op.symbol()))),
                    (Ty::Int(ty), false) => Ok(sequence(&names, self.arithmetic(*op, *ty, left, right, expr.span))),
                    (other, _) => Err(not_yet(expr.span, &format!("`{}` on `{other}`", op.symbol()))),
                }
            }
            // A narrowing conversion follows the overflow rule: checked in a debug build, cut
            // to size in a release build.
            ExprKind::Cast { ty, expr: inner } => {
                let inner = self.first(inner, head)?;
                if !self.checked {
                    return Ok(format!("(({})({inner}))", int_ctype(*ty)));
                }
                let loc = self.loc(expr.span);
                Ok(format!("sg_cast_{}((int64_t)({inner}), {loc})", int_name(*ty)))
            }
            // A byte of text and a range of it: checked in every build.
            ExprKind::Index { recv, index, kind: IndexKind::Text } => {
                let before = self.mark();
                let text = self.view(recv)?;
                let mut parts = vec![self.part(recv, &Ty::Read(Box::new(Ty::Str)), text, before)];
                let before = self.mark();
                let at = self.value(index)?;
                parts.push(self.part(index, &index.ty, at, before));
                let (values, names) = self.in_order(parts)?;
                let loc = self.loc(expr.span);
                Ok(sequence(&names, format!("sg_at({}, {}, {loc})", values[0], values[1])))
            }
            ExprKind::Slice { recv, from, to } => {
                let text = self.view(recv)?;
                if from.is_none() && to.is_none() {
                    return Ok(text);
                }
                // The text is named once: its length is one of the ends.
                let temp = self.temp();
                self.pre.push(format!("sg_str {temp} = {text};"));
                let (from, to, names) = match (from, to) {
                    (Some(from), Some(to)) => {
                        let (values, names) = self.two(from, to, false)?;
                        (values[0].clone(), values[1].clone(), names)
                    }
                    (Some(from), None) => (self.value(from)?, format!("{temp}.len"), String::new()),
                    (None, Some(to)) => ("0".to_string(), self.value(to)?, String::new()),
                    (None, None) => unreachable!("handled above"),
                };
                let loc = self.loc(expr.span);
                Ok(sequence(&names, format!("sg_slice({temp}, {from}, {to}, {loc})")))
            }
            ExprKind::StructLit { name, fields, .. } => {
                let mut parts = Vec::new();
                let decl: &'a StructDecl = self.structs[name.as_str()];
                let mut head = head;
                for field in fields {
                    let want = self.written(&decl.fields[field.index].ty)?;
                    let before = self.mark();
                    let value = self.first_as(&field.value, &want, head)?;
                    head = head && !self.droppable(&want);
                    parts.push(self.part(&field.value, &want, value, before));
                }
                let (values, names) = self.in_order(parts)?;
                let inits: Vec<String> = fields.iter().zip(values).map(|(field, value)| format!(".m_{} = {value}", field.name)).collect();
                Ok(sequence(&names, format!("((S_{name}){{ {} }})", inits.join(", "))))
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
                Callee::Plain => self.call(format!("f_{name}"), None, args, head),
                Callee::Static { owner, name } => self.call(format!("f_{owner}_{name}"), None, args, head),
                _ => Err(not_yet(expr.span, "a call through a function value")),
            },
            ExprKind::Method { recv, name, args, callee, .. } => match callee {
                Callee::Static { owner, name } => self.call(format!("f_{owner}_{name}"), None, args, head),
                Callee::Method { owner, this } => {
                    let held = recv.ty.strip_borrow().clone();
                    let before = self.mark();
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
                    let this_ty = match this {
                        This::Value => Ty::Struct(owner.clone()),
                        This::Read => Ty::Read(Box::new(Ty::Struct(owner.clone()))),
                        This::Ref => Ty::Ref(Box::new(Ty::Struct(owner.clone()))),
                    };
                    let receiver = self.part(recv, &this_ty, receiver, before);
                    self.call(format!("f_{owner}_{name}"), Some(receiver), args, false)
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
                    let (values, names) = self.two(recv, &args[0], head)?;
                    let (left, right) = (&values[0], &values[1]);
                    let builtin = match name.as_str() {
                        "tryAdd" => "add",
                        "trySubtract" => "sub",
                        "tryMultiply" => "mul",
                        _ => {
                            return Ok(sequence(&names, format!(
                                "({{ {int} {temp}a = {left}, {temp}b = {right}; {option} {temp} = {{ false, 0 }}; \
                                 if ({temp}b != 0 && !(({int})-1 < 0 && {temp}b == ({int})-1 && __builtin_sub_overflow(({int})0, {temp}a, &{temp}.value))) \
                                 {{ {temp}.has = true; {temp}.value = {temp}a / {temp}b; }} {temp}; }})"
                            )));
                        }
                    };
                    Ok(sequence(&names, format!("({{ {option} {temp}; {temp}.has = !__builtin_{builtin}_overflow({left}, {right}, &{temp}.value); {temp}; }})")))
                }
                Callee::Plain => {
                    let wrapping = match name.as_str() {
                        "wrappingAdd" => "wadd",
                        "wrappingSubtract" => "wsub",
                        "wrappingMultiply" => "wmul",
                        _ => return Err(not_yet(expr.span, &format!("the method `{name}`"))),
                    };
                    let Ty::Int(ty) = &expr.ty else { return Err(not_yet(expr.span, &format!("the method `{name}`"))) };
                    let (values, names) = self.two(recv, &args[0], head)?;
                    Ok(sequence(&names, format!("sg_{wrapping}_{}({}, {})", int_name(*ty), values[0], values[1])))
                }
                _ => Err(not_yet(expr.span, &format!("`{name}`"))),
            },
            _ => Err(not_yet(expr.span, "this expression")),
        }
    }
}

/// A C expression that first runs the declarations `names` (the earlier parts, kept in
/// variables) and then gives `value`.
fn sequence(names: &str, value: String) -> String {
    if names.is_empty() { value } else { format!("({{ {names}{value}; }})") }
}

/// Whether the block ends with a statement that leaves it: what follows would never run.
fn ends_by_leaving(block: &Block) -> bool {
    matches!(block.last().map(|stmt| &stmt.kind), Some(StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue))
}
