//! The first back end: writes a checked program as C.
//!
//! The checker has already done the work that makes a program safe; what is left to run time
//! are the same data-dependent failures the interpreter reports, with the same messages:
//! overflow, division by zero, a cast that does not fit, `assert` and `panic`.
//!
//! Only a part of the language is compiled so far: integers, `bool`, functions, structs of
//! those, `read` and `ref`, and `print`. Anything else is an error that names what is missing,
//! so nothing is compiled wrongly in silence.
//!
//! How values are represented: `byte`, `int`, `long` and `nint` are fixed-size C integers, a
//! struct is a C struct passed by value, and `read T` / `ref T` are pointers.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::ast::*;
use crate::diag::{snippet, Diag, Span};
use crate::types::{IntTy, Ty};

type Res<T> = Result<T, Diag>;

/// What every compiled program starts with: the checks, written once.
const RUNTIME: &str = r#"#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

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
"#;

/// Writes the program as C. `path` names the source file in the panic messages. `checked`: a
/// debug build, where `+`, `-`, `*` and unary `-` panic on overflow; without it, a release
/// build, where they wrap.
pub fn emit(prog: &Program, src: &str, path: &str, checked: bool) -> Res<String> {
    let structs = prog.structs.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let mut emitter = Emitter { src, path, structs, checked, locs: Vec::new(), temps: 0 };
    if let Some(decl) = prog.enums.first() {
        return Err(not_yet(decl.name_span, "an enum"));
    }

    // Structs, each after the structs it holds by value.
    let mut types = String::new();
    let mut done: Vec<&str> = Vec::new();
    for decl in &prog.structs {
        emitter.struct_decl(decl, &mut done, &mut types)?;
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
    out.push_str(&types);
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
        .note("the back end compiles integers, `bool`, functions, structs of those, `read` and `ref`, and `print`")
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
            b'%' | b'?' | 32..=126 => out.push(byte as char),
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

struct Emitter<'a> {
    src: &'a str,
    path: &'a str,
    structs: HashMap<&'a str, &'a StructDecl>,
    /// A debug build: overflow panics. Otherwise it wraps.
    checked: bool,
    /// The place of each line that can panic, as it is printed under the message.
    locs: Vec<String>,
    temps: usize,
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

    /// The C type of a value.
    fn ctype(&self, ty: &Ty, span: Span) -> Res<String> {
        Ok(match ty {
            Ty::Int(ty) => int_ctype(*ty).to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::Void => "void".to_string(),
            Ty::Struct(name) => format!("S_{name}"),
            Ty::Read(inner) | Ty::Ref(inner) => format!("{}*", self.ctype(inner, span)?),
            other => return Err(not_yet(span, &format!("the type `{other}`"))),
        })
    }

    /// A type as it is written in a signature or a field.
    fn written(&self, ty: &TypeExpr) -> Res<Ty> {
        if !ty.args.is_empty() || !ty.suffixes.is_empty() {
            return Err(not_yet(ty.span, &format!("the type `{}`", self.text(ty.span))));
        }
        let base = match Ty::from_name(&ty.name) {
            Some(base @ (Ty::Int(_) | Ty::Bool | Ty::Void)) => base,
            None if self.structs.contains_key(ty.name.as_str()) => Ty::Struct(ty.name.clone()),
            _ => return Err(not_yet(ty.span, &format!("the type `{}`", self.text(ty.span)))),
        };
        Ok(match ty.mode {
            Some(Mode::Read) => Ty::Read(Box::new(base)),
            Some(Mode::Ref) => Ty::Ref(Box::new(base)),
            None => base,
        })
    }

    fn struct_decl(&mut self, decl: &'a StructDecl, done: &mut Vec<&'a str>, out: &mut String) -> Res<()> {
        if done.contains(&decl.name.as_str()) {
            return Ok(());
        }
        done.push(&decl.name);
        let mut fields = String::new();
        for field in &decl.fields {
            let ty = self.written(&field.ty)?;
            if ty.is_borrow() {
                return Err(not_yet(field.ty.span, "a field that borrows"));
            }
            if let Ty::Struct(inner) = &ty {
                let inner = self.structs[inner.as_str()];
                self.struct_decl(inner, done, out)?;
            }
            let _ = writeln!(fields, "    {} m_{};", self.ctype(&ty, field.ty.span)?, field.name);
        }
        let _ = writeln!(out, "typedef struct S_{0} {{\n{fields}}} S_{0};\n", decl.name);
        Ok(())
    }

    fn func(&mut self, func: &'a Func, owner: Option<&str>, protos: &mut String, bodies: &mut String) -> Res<()> {
        let ret = self.written(&func.ret)?;
        if ret.is_borrow() {
            return Err(not_yet(func.ret.span, "a function that returns a borrow"));
        }
        let name = match owner {
            Some(owner) => format!("f_{owner}_{}", func.name),
            None => format!("f_{}", func.name),
        };
        let mut params = Vec::new();
        if let (Some(this), Some(owner)) = (func.this, owner) {
            params.push(match this {
                This::Value => format!("S_{owner} v_this"),
                This::Read | This::Ref => format!("S_{owner}* v_this"),
            });
        }
        for param in &func.params {
            let ty = self.written(&param.ty)?;
            params.push(format!("{} v_{}", self.ctype(&ty, param.ty.span)?, param.name));
        }
        let params = if params.is_empty() { "void".to_string() } else { params.join(", ") };
        let head = format!("static {} {name}({params})", self.ctype(&ret, func.ret.span)?);
        let _ = writeln!(protos, "{head};");
        let _ = writeln!(bodies, "{head} {{");
        self.block(&func.body, 1, bodies)?;
        bodies.push_str("}\n\n");
        Ok(())
    }

    fn block(&mut self, block: &'a Block, depth: usize, out: &mut String) -> Res<()> {
        for stmt in block {
            self.stmt(stmt, depth, out)?;
        }
        Ok(())
    }

    fn stmt(&mut self, stmt: &'a Stmt, depth: usize, out: &mut String) -> Res<()> {
        let pad = "    ".repeat(depth);
        match &stmt.kind {
            StmtKind::Let { ty, name, init, .. } => {
                let ty = match ty {
                    Some(written) => self.written(written)?,
                    None => init.ty.clone(),
                };
                let value = self.value(init)?;
                let _ = writeln!(out, "{pad}{} v_{name} = {value};", self.ctype(&ty, init.span)?);
            }
            StmtKind::Assign { target, op, value, through } => {
                // A `read` variable pointed at something else: the pointer itself is assigned.
                let place = match (&target.kind, target.ty.is_borrow(), through) {
                    (ExprKind::Var(name), true, false) => format!("v_{name}"),
                    _ => self.lvalue(target)?,
                };
                let new = self.value(value)?;
                match op {
                    None => {
                        let _ = writeln!(out, "{pad}{place} = {new};");
                    }
                    Some(op) => {
                        let Ty::Int(ty) = target.ty.strip_borrow() else { return Err(not_yet(stmt.span, "this assignment")) };
                        let operation = self.arithmetic(*op, *ty, &place, &new, stmt.span);
                        let _ = writeln!(out, "{pad}{place} = {operation};");
                    }
                }
            }
            StmtKind::If { cond, then, els } => {
                let cond = self.value(cond)?;
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
                let _ = writeln!(out, "{pad}while ({cond}) {{");
                self.block(body, depth + 1, out)?;
                let _ = writeln!(out, "{pad}}}");
            }
            StmtKind::For { var, from, to, body, .. } => {
                // The ends are computed once; the counter stops before the end, so it cannot overflow.
                let ty = self.ctype(&from.ty, from.span)?;
                let end = self.temp();
                let (from, to) = (self.value(from)?, self.value(to)?);
                let _ = writeln!(out, "{pad}for ({ty} v_{var} = {from}, {end} = {to}; v_{var} < {end}; v_{var}++) {{");
                self.block(body, depth + 1, out)?;
                let _ = writeln!(out, "{pad}}}");
            }
            StmtKind::ForEach { iter, .. } => return Err(not_yet(iter.span, "a `for` over a collection")),
            StmtKind::Return(None) => {
                let _ = writeln!(out, "{pad}return;");
            }
            StmtKind::Return(Some(value)) => {
                let value = self.value(value)?;
                let _ = writeln!(out, "{pad}return {value};");
            }
            StmtKind::Break => {
                let _ = writeln!(out, "{pad}break;");
            }
            StmtKind::Continue => {
                let _ = writeln!(out, "{pad}continue;");
            }
            StmtKind::Discard(value) => {
                let value = self.value(value)?;
                let _ = writeln!(out, "{pad}(void)({value});");
            }
            StmtKind::Expr(expr) => match &expr.kind {
                ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "print" => self.print(&args[0], &pad, out)?,
                ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "assert" => self.assert(expr, &args[0], &pad, out)?,
                ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "panic" => {
                    let ExprKind::Str(text) = &args[0].kind else { return Err(not_yet(args[0].span, "a `panic` message that is built at run time")) };
                    let loc = self.loc(expr.span);
                    let _ = writeln!(out, "{pad}sg_panic({}, {loc});", c_string(text));
                }
                _ => {
                    let value = self.value(expr)?;
                    let _ = writeln!(out, "{pad}{value};");
                }
            },
        }
        Ok(())
    }

    /// `print(x)`: a number, a `bool`, a text constant, or `$"..."` made of those.
    fn print(&mut self, arg: &'a Expr, pad: &str, out: &mut String) -> Res<()> {
        let mut format = String::new();
        let mut values = Vec::new();
        match &arg.kind {
            ExprKind::Str(text) => format.push_str(&text.replace('%', "%%")),
            ExprKind::Interp(parts) => {
                for part in parts {
                    match part {
                        InterpPart::Text(text) => format.push_str(&text.replace('%', "%%")),
                        InterpPart::Expr(expr) => self.printed(expr, &mut format, &mut values)?,
                    }
                }
            }
            _ => self.printed(arg, &mut format, &mut values)?,
        }
        format.push('\n');
        let values: String = values.iter().map(|value| format!(", {value}")).collect();
        let _ = writeln!(out, "{pad}printf({}{values});", c_string(&format));
        Ok(())
    }

    /// One value inside a printed line: what `printf` is told, and what it is given.
    fn printed(&mut self, expr: &'a Expr, format: &mut String, values: &mut Vec<String>) -> Res<()> {
        if let ExprKind::Str(text) = &expr.kind {
            format.push_str(&text.replace('%', "%%"));
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
            other => return Err(not_yet(expr.span, &format!("printing a `{other}`"))),
        }
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
                _ => None,
            };
            if let (true, Some((fail, shown))) = (op.is_comparison(), sides) {
                let (l, r) = (self.temp(), self.temp());
                let ty = self.ctype(left.ty.strip_borrow(), left.span)?;
                let (left, right) = (self.value(left)?, self.value(right)?);
                let _ = writeln!(out, "{pad}{{ {ty} {l} = {left}; {ty} {r} = {right}; if (!({l} {} {r})) {fail}({text}, ({shown}){l}, ({shown}){r}, {loc}); }}", op.symbol());
                return Ok(());
            }
        }
        let cond = self.value(cond)?;
        let _ = writeln!(out, "{pad}if (!({cond})) sg_assert_fail({text}, {loc});");
        Ok(())
    }

    /// A place as C writes it: something that can be assigned to and whose address can be taken.
    fn lvalue(&mut self, expr: &'a Expr) -> Res<String> {
        match &expr.kind {
            ExprKind::Var(name) if expr.ty.is_borrow() => Ok(format!("(*v_{name})")),
            ExprKind::Var(name) => Ok(format!("v_{name}")),
            ExprKind::Field { recv, name, .. } => Ok(format!("{}.m_{name}", self.lvalue(recv)?)),
            _ => Err(not_yet(expr.span, "this place")),
        }
    }

    /// The address of a place, for `read x`, `ref x` and the receiver of a method.
    fn address(&mut self, place: &'a Expr) -> Res<String> {
        match (&place.kind, place.ty.is_borrow()) {
            (ExprKind::Var(name), true) => Ok(format!("v_{name}")),
            _ => Ok(format!("(&{})", self.lvalue(place)?)),
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

    fn args(&mut self, args: &'a [Expr]) -> Res<Vec<String>> {
        args.iter().map(|arg| self.value(arg)).collect()
    }

    fn value(&mut self, expr: &'a Expr) -> Res<String> {
        match &expr.kind {
            ExprKind::Int(value, ty) => Ok(match *value {
                i64::MIN => format!("(({})(-9223372036854775807LL - 1))", int_ctype(*ty)),
                value => format!("(({}){value}LL)", int_ctype(*ty)),
            }),
            ExprKind::Bool(value) => Ok(value.to_string()),
            ExprKind::Var(name) => Ok(format!("v_{name}")),
            ExprKind::Field { .. } => self.lvalue(expr),
            ExprKind::Load(inner) => Ok(format!("(*{})", self.value(inner)?)),
            ExprKind::Widen(ty, inner) => Ok(format!("(({}){})", int_ctype(*ty), self.value(inner)?)),
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
                Ok(format!("({} {} {})", self.value(left)?, op.symbol(), self.value(right)?))
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
            ExprKind::StructLit { name, fields, .. } => {
                let mut inits = Vec::new();
                for field in fields {
                    inits.push(format!(".m_{} = {}", field.name, self.value(&field.value)?));
                }
                Ok(format!("((S_{name}){{ {} }})", inits.join(", ")))
            }
            ExprKind::Call { name, args, callee, .. } => match callee {
                Callee::Plain if matches!(name.as_str(), "print" | "assert" | "panic") => Err(not_yet(expr.span, &format!("`{name}` used as a value"))),
                Callee::Plain => Ok(format!("f_{name}({})", self.args(args)?.join(", "))),
                Callee::Static { owner, name } => Ok(format!("f_{owner}_{name}({})", self.args(args)?.join(", "))),
                _ => Err(not_yet(expr.span, "a call through a function value")),
            },
            ExprKind::Method { recv, name, args, callee, .. } => match callee {
                Callee::Static { owner, name } => Ok(format!("f_{owner}_{name}({})", self.args(args)?.join(", "))),
                Callee::Method { owner, this } => {
                    if !recv.is_place() {
                        return Err(not_yet(recv.span, "a method called on a value that has no name"));
                    }
                    let mut all = vec![match this {
                        This::Value => self.lvalue(recv)?,
                        This::Read | This::Ref => self.address(recv)?,
                    }];
                    all.extend(self.args(args)?);
                    Ok(format!("f_{owner}_{name}({})", all.join(", ")))
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


