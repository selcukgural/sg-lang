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

use std::collections::HashMap;

use crate::ast::*;
use crate::diag::{nearest, Diag, Span};
use crate::types::{IntTy, Ty};

type Res<T> = Result<T, Diag>;

const TYPES_NOT_YET: &[&str] =
    &["byte", "sbyte", "short", "ushort", "uint", "ulong", "nuint", "float", "double", "char"];

const BUILTINS: &[&str] = &["print", "assert", "panic"];

struct StructInfo {
    fields: Vec<(String, Ty)>,
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

type Names = HashMap<String, TypeKind>;

struct FuncSig {
    this: Option<This>,
    params: Vec<Ty>,
    ret: Ty,
}

fn func_sig(func: &Func, names: &Names) -> Res<FuncSig> {
    let mut params = Vec::new();
    for param in &func.params {
        params.push(resolve_type(&param.ty, names, false)?);
    }
    Ok(FuncSig { this: func.this, params, ret: resolve_type(&func.ret, names, true)? })
}

pub fn check(prog: &mut Program, src: &str) -> Res<()> {
    let mut names: Names = HashMap::new();
    let declared = prog
        .structs
        .iter()
        .map(|decl| (&decl.name, decl.name_span, TypeKind::Struct))
        .chain(prog.enums.iter().map(|decl| (&decl.name, decl.name_span, TypeKind::Enum)));
    for (name, span, kind) in declared {
        if Ty::from_name(name).is_some() || names.insert(name.clone(), kind).is_some() {
            return Err(Diag::error(span, format!("type `{name}` is defined twice")));
        }
    }

    let mut structs: HashMap<String, StructInfo> = HashMap::new();
    for decl in &prog.structs {
        let mut fields: Vec<(String, Ty)> = Vec::new();
        for field in &decl.fields {
            if fields.iter().any(|(name, _)| *name == field.name) {
                return Err(Diag::error(field.span, format!("field `{}` is declared twice", field.name)));
            }
            if field.ty.mode.is_some() {
                return Err(Diag::error(field.ty.span, "borrowed fields are not in the interpreter yet")
                    .note("they need the struct to name its source: `struct Token<from source>`"));
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
            if methods.insert(method.name.clone(), func_sig(method, &names)?).is_some() {
                return Err(Diag::error(method.name_span, format!("method `{}` is defined twice", method.name)));
            }
        }
        structs.insert(decl.name.clone(), StructInfo { fields, methods });
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
                if field.ty.mode.is_some() {
                    return Err(Diag::error(field.ty.span, "borrowed fields are not in the interpreter yet")
                        .note("they need the enum to name its source: `enum Token<from source>`"));
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

    let mut sigs: HashMap<String, FuncSig> = HashMap::new();
    for func in &prog.funcs {
        if BUILTINS.contains(&func.name.as_str()) {
            return Err(Diag::error(
                func.name_span,
                format!("`{}` is a built-in function and cannot be redefined", func.name),
            ));
        }
        if names.contains_key(func.name.as_str()) {
            return Err(Diag::error(func.name_span, format!("`{}` is already the name of a type", func.name)));
        }
        if sigs.insert(func.name.clone(), func_sig(func, &names)?).is_some() {
            return Err(Diag::error(func.name_span, format!("function `{}` is defined twice", func.name)));
        }
    }

    match prog.funcs.iter().find(|func| func.name == "main") {
        None => {
            return Err(Diag::error(Span::new(src.len(), src.len()), "there is no `main` function")
                .hint("a program starts at `void main() { ... }`"))
        }
        Some(main) if sigs["main"].ret != Ty::Void || !main.params.is_empty() => {
            return Err(Diag::error(main.name_span, "for now `main` can only be written as `void main()`"))
        }
        Some(_) => {}
    }

    let body = |func: &mut Func, sig: &FuncSig, owner: Option<&str>| -> Res<()> {
        let mut checker = Checker {
            names: &names,
            structs: &structs,
            enums: &enums,
            sigs: &sigs,
            scopes: vec![Scope::default()],
            loops: 0,
            ret: sig.ret.clone(),
            func_name: func.name.clone(),
            owner,
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
        }
        for (param, ty) in func.params.iter().zip(&sig.params) {
            checker.declare(&param.name, ty.clone(), param.span)?;
        }
        checker.block(&mut func.body, None)?;
        if sig.ret != Ty::Void && !leaves(&func.body, false) {
            return Err(Diag::error(
                func.name_span,
                format!("`{}` does not return a value on every path; it must return `{}`", func.name, sig.ret),
            ));
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
    Ok(())
}

fn resolve_type(ty: &TypeExpr, names: &Names, allow_void: bool) -> Res<Ty> {
    let mut resolved = match Ty::from_name(&ty.name) {
        Some(Ty::Void) if !allow_void || !ty.suffixes.is_empty() || ty.mode.is_some() => {
            return Err(Diag::error(ty.span, "`void` can only be a return type"));
        }
        Some(base) => base,
        None if names.get(ty.name.as_str()) == Some(&TypeKind::Struct) => Ty::Struct(ty.name.clone()),
        None if names.contains_key(ty.name.as_str()) => Ty::Enum(ty.name.clone()),
        None => {
            let diag = if TYPES_NOT_YET.contains(&ty.name.as_str()) {
                Diag::error(ty.span, format!("type `{}` is not in the interpreter yet", ty.name))
            } else {
                Diag::error(ty.span, format!("there is no type named `{}`", ty.name))
            };
            let known = names.keys().map(String::as_str).chain(["int", "long", "nint", "bool", "string"]);
            return Err(match nearest(&ty.name, known) {
                Some(close) => diag.hint(format!("`{close}`")),
                None => diag.note("built-in types for now: int, long, nint, bool, string"),
            });
        }
    };
    for suffix in &ty.suffixes {
        resolved = match suffix {
            Suffix::Pointer => Ty::Ptr(Box::new(resolved)),
            Suffix::Nullable if matches!(resolved, Ty::Nullable(_)) => {
                return Err(Diag::error(ty.span, "`??` is not a type: a value is either there or `null`"));
            }
            Suffix::Nullable => Ty::Nullable(Box::new(resolved)),
        };
    }
    Ok(match ty.mode {
        Some(Mode::Read) => Ty::Read(Box::new(resolved)),
        Some(Mode::Ref) => Ty::Ref(Box::new(resolved)),
        None => resolved,
    })
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
            Ty::Nullable(inner) => by_value(inner),
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

struct Var {
    name: String,
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
    structs: &'a HashMap<String, StructInfo>,
    enums: &'a HashMap<String, EnumInfo>,
    sigs: &'a HashMap<String, FuncSig>,
    scopes: Vec<Scope>,
    loops: usize,
    ret: Ty,
    func_name: String,
    /// The struct whose method is being checked.
    owner: Option<&'a str>,
    src: &'a str,
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
        scope.vars.push(Var { name: name.to_string(), declared: ty.clone(), current: ty, moved, assigned: 0 });
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
        let visible = self.scopes.iter().flat_map(|scope| scope.vars.iter().map(|var| var.name.as_str()));
        match nearest(name, visible) {
            Some(close) => diag.hint(format!("`{close}`")),
            None => diag,
        }
    }

    /// A variable that is about to be read: it must still hold its value.
    fn live_var(&mut self, name: &str, span: Span) -> Res<&mut Var> {
        let var = self.var(name, span)?;
        if let Some(moved) = var.moved.whole {
            return Err(Diag::error(span, format!("`{name}` was moved and can no longer be used")).related(moved, "it was moved here"));
        }
        Ok(var)
    }

    fn narrow(&mut self, name: &str) {
        let Some(var) = self.find(name) else { return };
        let Some(narrowed) = without_null(&var.current) else { return };
        let saved = (name.to_string(), std::mem::replace(&mut var.current, narrowed), var.assigned);
        self.scopes.last_mut().expect("an open scope").narrowed.push(saved);
    }

    fn pop_scope(&mut self) {
        let scope = self.scopes.pop().expect("an open scope");
        for (name, before, assigned) in scope.narrowed.into_iter().rev() {
            let Some(var) = self.find(&name) else { continue };
            var.current = if var.assigned == assigned { before } else { var.declared.clone() };
        }
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
        for (name, ty, span) in binds {
            self.declare_local(&name, ty, span)?;
        }
        for stmt in block.iter_mut() {
            self.stmt(stmt)?;
        }
        self.pop_scope();
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
                        Ty::Void => return Err(Diag::error(init.span, "this call does not return a value")),
                        Ty::Null => {
                            return Err(Diag::error(init.span, "`var` cannot find a type from `null` alone")
                                .hint(format!("write the type, for example `int? {name} = null`")))
                        }
                        ty => ty,
                    },
                };
                self.declare_local(name, ty, *name_span)?;
            }
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
                        if !writable {
                            return Err(Diag::error(target.span, "this field is reached through a `read` borrow and cannot be written")
                                .hint("take the value as `ref` instead of `read`"));
                        }
                        ty
                    }
                };
                if op.is_some() && !matches!(target_ty, Ty::Int(_)) {
                    return Err(Diag::error(
                        stmt_span,
                        format!("`{}=` needs an integer on the left, found `{target_ty}`", op.expect("checked").symbol()),
                    ));
                }
                self.typed(value, &target_ty)?;
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
                self.block_with(then, not_null(true), binds)?;
                let then_leaves = leaves(then, true);
                let after_then = self.moves();
                self.set_moves(&before);
                let mut else_leaves = false;
                if let Some(els) = els {
                    self.block(els, not_null(false))?;
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
                let binds = self.condition(cond)?;
                let test = narrowing(cond);
                let narrow = test.as_ref().filter(|(_, negated)| *negated).map(|(name, _)| name.as_str());
                self.loop_body(body, narrow, binds)?;
            }
            StmtKind::For { var, var_span, from, to, body } => {
                self.forget_narrowing_of_assigned(body, None);
                let from_ty = self.int_operand(from, None, "the start of the range")?;
                let to_ty = self.int_operand(to, Some(from_ty), "the end of the range")?;
                let ty = Ty::Int(from_ty.unify(to_ty));
                self.coerce(from, Ty::Int(from_ty), &ty)?;
                self.coerce(to, Ty::Int(to_ty), &ty)?;
                self.scopes.push(Scope::default());
                self.declare_local(var, ty, *var_span)?;
                self.loop_body(body, None, Vec::new())?;
                self.pop_scope();
            }
            StmtKind::Return(value) => match value {
                Some(value) if self.ret == Ty::Void => {
                    return Err(Diag::error(value.span, format!("`{}` is void and cannot return a value", self.func_name)));
                }
                Some(value) => {
                    let ret = self.ret.clone();
                    self.typed(value, &ret)?;
                }
                None if self.ret != Ty::Void => {
                    return Err(Diag::error(stmt_span, format!("`{}` must return a `{}` value", self.func_name, self.ret)));
                }
                None => {}
            },
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

    /// An expression whose value is not used: a statement, or an arm of a `match` statement.
    fn unused(&mut self, expr: &mut Expr) -> Res<()> {
        let span = expr.span;
        match &mut expr.kind {
            ExprKind::Match { subject, arms, borrowed } => {
                self.match_expr(subject, arms, borrowed, None, true)?;
            }
            ExprKind::Call { .. } | ExprKind::Method { .. } => {
                self.expr(expr, None)?;
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
            (Ty::Int(IntTy::Int), Ty::Int(to)) => {
                wrap(expr, |inner| ExprKind::Widen(*to, inner));
                Ok(())
            }
            (Ty::Int(_), Ty::Int(to)) => {
                mismatch(Some(format!("convert with `({})`; it panics if the value does not fit", to.name())))
            }
            (Ty::Null, _) if matches!(expected.strip_borrow(), Ty::Nullable(_)) => Ok(()),
            (Ty::Null, _) => Err(Diag::error(expr.span, format!("`{expected}` cannot be `null`"))
                .hint(format!("a value that may be missing has the type `{expected}?`"))),
            (Ty::Void, _) => Err(Diag::error(expr.span, "this call does not return a value, and one is needed here")),
            (Ty::Fmt, Ty::Str) => Err(Diag::error(expr.span, "expected `string`; `$\"...\"` does not become a `string` on its own")
                .hint("add `.toString()` at the end; that is where the allocation shows")),
            // `T` where `T?` is expected.
            (_, Ty::Nullable(inner)) => self.coerce(expr, found.clone(), inner),
            // A borrow of a copyable value stands in for the value.
            (Ty::Read(inner) | Ty::Ref(inner), _) if !expected.is_borrow() && inner.is_copyable() => {
                wrap(expr, ExprKind::Load);
                self.coerce(expr, (**inner).clone(), expected)
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
            (Ty::Ref(from), Ty::Read(to)) if from == to => Ok(()),
            (Ty::Read(from) | Ty::Ref(from), Ty::Read(to)) if **to == Ty::Nullable(from.clone()) => Ok(()),
            (Ty::Str, Ty::Read(to)) if **to == Ty::Str && matches!(expr.kind, ExprKind::Str(_)) => Ok(()),
            (_, Ty::Read(to)) if found == **to => mismatch(Some(format!("lend it with `read`: `read {text}`"))),
            (_, Ty::Ref(to)) if found == **to => mismatch(Some(format!("lend it with `ref`: `ref {text}`"))),
            _ => mismatch(None),
        }
    }

    /// The type of a variable or field, without using its value up. `None` if `expr` is
    /// not a place. The flag says whether it can be written to.
    fn place(&mut self, expr: &mut Expr) -> Res<Option<(Ty, bool)>> {
        let found = self.place_unchecked(expr)?;
        if found.is_some() {
            self.check_parts(expr, false)?;
        }
        Ok(found)
    }

    /// `place` without the check for fields that were moved out.
    fn place_unchecked(&mut self, expr: &mut Expr) -> Res<Option<(Ty, bool)>> {
        let span = expr.span;
        // `Shape.Empty` is a value, not a field.
        if let ExprKind::Field { recv, .. } = &expr.kind {
            if self.enum_named(recv).is_some() {
                return Ok(None);
            }
        }
        match &mut expr.kind {
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
                *index = position;
                Ok(Some((fields[position].1.clone(), writable)))
            }
            _ => Ok(None),
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
        let span = expr.span;
        if let Some((ty, _)) = self.place(expr)? {
            if !ty.is_copyable() {
                self.move_out(expr, &ty)?;
            }
            return Ok(ty);
        }

        // `Shape.Empty` and `Shape.Circle(2)` build a value of the enum.
        match &mut expr.kind {
            ExprKind::Field { recv, name, name_span, .. } => {
                let owner = self.enum_named(recv).expect("any other field is a place");
                let (index, ty) = self.variant(owner, name, *name_span, None, span)?;
                expr.kind = ExprKind::Variant { index, args: Vec::new() };
                return Ok(ty);
            }
            ExprKind::Method { recv, name, name_span, args, .. } => {
                if let Some(owner) = self.enum_named(recv) {
                    let (index, ty) = self.variant(owner, name, *name_span, Some(args), span)?;
                    let args = std::mem::take(args);
                    expr.kind = ExprKind::Variant { index, args };
                    return Ok(ty);
                }
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
                    return Err(match nearest(name, self.structs.keys().map(String::as_str)) {
                        Some(close) => diag.hint(format!("`{close}`")),
                        None => diag,
                    });
                };
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
                if !writable {
                    return Err(Diag::error(inner.span, "this is reached through a `read` borrow and cannot be lent as `ref`"));
                }
                Ok(Ty::Ref(base))
            }
            ExprKind::IsNull { expr: inner, .. } => {
                let ty = self.operand(inner, None)?;
                if !matches!(ty.strip_borrow(), Ty::Nullable(_) | Ty::Null) {
                    return Err(Diag::error(inner.span, format!("this is `{ty}`, which is never `null`")).note("only a `T?` can be `null`"));
                }
                Ok(Ty::Bool)
            }
            ExprKind::Coalesce(left, right) => {
                let nullable = expected.filter(|ty| !ty.is_borrow()).map(|ty| Ty::Nullable(Box::new(ty.clone())));
                let found = self.expr(left, nullable.as_ref())?;
                let Some(inner) = without_null(&found) else {
                    return Err(Diag::error(left.span, format!("this is `{found}`, which is never `null`, so `??` has nothing to do")));
                };
                self.typed(right, &inner)?;
                Ok(inner)
            }
            ExprKind::IsVariant { .. } => {
                self.is_variant(expr, false)?;
                Ok(Ty::Bool)
            }
            ExprKind::Match { subject, arms, borrowed } => self.match_expr(subject, arms, borrowed, expected, false),
            ExprKind::Var(_) | ExprKind::Field { .. } => unreachable!("places are handled above"),
            ExprKind::Widen(..) | ExprKind::Load(_) | ExprKind::Unbox(_) | ExprKind::Variant { .. } => {
                unreachable!("inserted only after checking")
            }
        }
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

    /// The enum a `match` or `is` looks at, behind pointers and borrows, and how the value
    /// is held: owned (`None`) or through a `read` or `ref` borrow.
    fn enum_behind(&self, ty: &Ty, span: Span, what: &str) -> Res<(&'a str, Option<Mode>)> {
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
                    let (key, _) = self.enums.get_key_value(name.as_str()).expect("the type was resolved");
                    return Ok((key.as_str(), mode));
                }
                Ty::Nullable(_) => {
                    return Err(Diag::error(span, format!("this is `{ty}` and may be `null`; {what} needs a value"))
                        .hint("test it first with `is not null`, or give a fallback with `??`"));
                }
                _ => {
                    return Err(Diag::error(span, format!("{what} needs a value of an enum, found `{ty}`"))
                        .note("patterns for numbers and text are not in the interpreter yet"));
                }
            };
        }
    }

    /// Resolves a pattern against the enum and gives the names it declares. With `mode` the
    /// names borrow from the value; without, they take what the value holds.
    fn pattern_binds(&mut self, pattern: &mut Pattern, owner: &str, mode: Option<Mode>, subject: &Ty) -> Res<Binds> {
        let span = pattern.span;
        let (name, args, index) = match &mut pattern.kind {
            PatternKind::Wildcard => return Ok(Vec::new()),
            PatternKind::Bind(name) => return Ok(vec![(name.clone(), subject.clone(), span)]),
            PatternKind::Variant { name, args, index } => (name, args, index),
        };
        let enums = self.enums;
        let variants = &enums[owner].variants;
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

    /// `x is Circle(r)`: looks at the value without taking it, so the names are borrows.
    /// They are in scope only when the test is the whole condition of an `if` or `while`.
    fn is_variant(&mut self, expr: &mut Expr, names_ok: bool) -> Res<Binds> {
        let ExprKind::IsVariant { expr: subject, negated, pattern, by_ref } = &mut expr.kind else {
            unreachable!("only called for an `is` test");
        };
        let ty = self.operand(subject, None)?;
        let (owner, mode) = self.enum_behind(&ty, subject.span, "`is`")?;
        *by_ref = mode == Some(Mode::Ref);
        let binds = self.pattern_binds(pattern, owner, mode.or(Some(Mode::Read)), &ty)?;
        if binds.is_empty() {
            return Ok(binds);
        }
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
        let (owner, mode) = self.enum_behind(&subject_ty, subject.span, "`match`")?;
        *borrowed = mode.is_some();
        let enums = self.enums;
        let variants = &enums[owner].variants;

        let mut handled = vec![false; variants.len()];
        let mut handled_all = false;
        let mut result: Option<Ty> = expected.cloned();
        let before = self.moves();
        let mut after: Option<Moves> = None;
        for arm in arms.iter_mut() {
            self.set_moves(&before);
            self.scopes.push(Scope::default());
            let binds = self.pattern_binds(&mut arm.pattern, owner, mode, &subject_ty)?;
            let position = match &arm.pattern.kind {
                PatternKind::Variant { index, .. } => Some(*index),
                _ => None,
            };
            if handled_all {
                return Err(Diag::error(arm.pattern.span, "this arm can never run: an arm above takes every value"));
            }
            if let Some(position) = position.filter(|position| handled[*position]) {
                return Err(Diag::error(
                    arm.pattern.span,
                    format!("this arm can never run: `{}` is already handled above", variants[position].0),
                ));
            }
            for (name, ty, span) in binds {
                self.declare_local(&name, ty, span)?;
            }
            match &mut arm.guard {
                Some(guard) => self.typed(guard, &Ty::Bool)?,
                // Only an arm without a condition settles its variant.
                None => match position {
                    Some(position) => handled[position] = true,
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
            self.pop_scope();
            // An arm that leaves does not reach the code after the `match`.
            if !leaves {
                if let Some(other) = &after {
                    self.merge_moves(other);
                }
                after = Some(self.moves());
            }
        }

        if !handled_all {
            let missing: Vec<String> =
                variants.iter().zip(&handled).filter(|(_, done)| !**done).map(|((name, _), _)| format!("`{name}`")).collect();
            if !missing.is_empty() {
                let diag = Diag::error(subject.span, format!("this `match` does not handle {}", missing.join(", ")))
                    .hint("add an arm for each, or `_ => ...` for everything else");
                let guarded = arms.iter().any(|arm| {
                    arm.guard.is_some() && matches!(&arm.pattern.kind, PatternKind::Variant { index, .. } if !handled[*index])
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

    fn printable(&self, ty: &Ty, span: Span) -> Res<()> {
        match ty.strip_borrow() {
            Ty::Int(_) | Ty::Bool | Ty::Str | Ty::Fmt => Ok(()),
            Ty::Void => Err(Diag::error(span, "this call does not return a value, and one is needed here")),
            other => Err(Diag::error(span, format!("a `{other}` value cannot be written as text yet"))
                .note("`Formattable` is not in the interpreter yet; for now: integers, `bool`, `string`")),
        }
    }

    fn args(&mut self, name: &str, args: &mut [Expr], params: &[Ty], span: Span) -> Res<()> {
        if args.len() != params.len() {
            return Err(Diag::error(span, format!("`{name}` takes {} argument(s), {} given", params.len(), args.len())));
        }
        for (arg, param) in args.iter_mut().zip(params) {
            self.typed(arg, param)?;
        }
        Ok(())
    }

    fn call(&mut self, name: &str, name_span: Span, args: &mut [Expr], callee: &mut Callee, span: Span) -> Res<Ty> {
        let arity = |count: usize| {
            if args.len() == count {
                Ok(())
            } else {
                Err(Diag::error(span, format!("`{name}` takes {count} argument(s), {} given", args.len())))
            }
        };
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
                let diag = Diag::error(recv.span, format!("there is no variable or type named `{owner}`"));
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
                let Some(this) = sig.this else {
                    return Err(Diag::error(name_span, format!("`{name}` does not take `this`, so it is called on the type"))
                        .hint(format!("`{owner}.{name}(...)`")));
                };
                match this {
                    This::Read => {}
                    This::Ref if !writable => {
                        return Err(Diag::error(recv.span, format!("`{name}` takes `ref this`, but this value is reached through a `read` borrow"))
                            .hint("take the value as `ref` instead of `read`"));
                    }
                    This::Ref => {}
                    This::Value if borrowed => {
                        return Err(Diag::error(recv.span, format!("`{name}` takes `this` by value and uses it up, but this value is only borrowed")));
                    }
                    This::Value if is_place => self.move_out(recv, &recv_ty)?,
                    This::Value => {}
                }
                self.args(name, args, &sig.params, span)?;
                *callee = Callee::Method { owner: owner.clone(), this };
                return Ok(sig.ret.clone());
            }
        }
        match name {
            "take" => return self.take(recv, &recv_ty, writable, is_place, args, callee, span),
            "replace" => return self.replace(recv, &recv_ty, writable, is_place, args, callee, span),
            _ => {}
        }

        let arity = |count: usize| {
            if args.len() == count {
                Ok(())
            } else {
                Err(Diag::error(span, format!("`{name}` takes {count} argument(s), {} given", args.len())))
            }
        };
        match (recv_ty.strip_borrow(), name) {
            (Ty::Int(ty), "wrappingAdd" | "wrappingSubtract" | "wrappingMultiply") => {
                arity(1)?;
                let other = self.int_operand(&mut args[0], Some(*ty), "the argument")?;
                let common = Ty::Int(ty.unify(other));
                self.coerce(recv, recv_ty.clone(), &common)?;
                self.coerce(&mut args[0], Ty::Int(other), &common)?;
                Ok(common)
            }
            (Ty::Int(_) | Ty::Bool | Ty::Fmt, "toString") => {
                arity(0)?;
                Ok(Ty::Str)
            }
            (Ty::Str, "length") => {
                arity(0)?;
                Ok(Ty::Int(IntTy::Nint))
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
        if !writable {
            return Err(Diag::error(recv.span, "this is reached through a `read` borrow, so nothing can be taken out of it")
                .hint("take the value as `ref` instead of `read`"));
        }
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
        if !writable {
            return Err(Diag::error(recv.span, "this is reached through a `read` borrow and cannot be replaced")
                .hint("take the value as `ref` instead of `read`"));
        }
        let slot = recv_ty.strip_borrow().clone();
        self.typed(&mut args[0], &slot)?;
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
            StmtKind::Return(Some(expr)) | StmtKind::Expr(expr) => taken_in(expr, names),
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
        ExprKind::Binary(_, left, right) | ExprKind::Coalesce(left, right) => {
            taken_in(left, names);
            taken_in(right, names);
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
        | ExprKind::Unary(_, inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::New(inner)
        | ExprKind::Borrow { expr: inner, .. }
        | ExprKind::IsNull { expr: inner, .. }
        | ExprKind::Widen(_, inner)
        | ExprKind::Load(inner) => taken_in(inner, names),
        ExprKind::Int(..) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Null | ExprKind::Var(_) => {}
    }
}

/// Replaces `expr` with a node built around it.
fn wrap(expr: &mut Expr, build: impl FnOnce(Box<Expr>) -> ExprKind) {
    let span = expr.span;
    let inner = std::mem::replace(expr, Expr { kind: ExprKind::Null, span });
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
        StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => breaks_in(expr),
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
