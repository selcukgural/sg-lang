//! Hand-written recursive descent parser.

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::lexer::{lex, Part, Tok, Token};
use crate::types::{FloatTy, IntTy, Ty};

/// Words that are in the design but not in the interpreter yet. They get an explicit
/// message instead of "unknown name".
const NOT_YET: &[&str] = &[
    "interface", "raw", "unsafe",
];

/// Words of the language that are read where they stand, not names: `shared T`, `weak T`,
/// `shared checked T`, `shared locked T`, `share x`, `weak x`, `using Name = T`,
/// `lock x as c { }`, `async int f()`, `await t`.
const WORDS: &[&str] = &["shared", "weak", "share", "using", "checked", "locked", "lock", "async", "await"];

type Res<T> = Result<T, Diag>;

pub fn parse(src: &str) -> Res<Program> {
    parse_files(src, &[(0, src.len(), String::new())])
}

/// Parses the files of a program, each given as where it lies in `src` and the module it
/// belongs to. What a module defines gets its full name here: `Product` in the folder `stock`
/// is `stock.Product`.
pub fn parse_files(src: &str, files: &[(usize, usize, String)]) -> Res<Program> {
    let mut program = Program { structs: Vec::new(), enums: Vec::new(), funcs: Vec::new(), tests: Vec::new(), aliases: Vec::new(), files: Vec::new() };
    for (start, end, module) in files {
        let toks = lex(src, Span::new(*start, *end), false)?;
        let mut parser = Parser { src, toks, i: 0, struct_literals: true };
        let (structs, enums, funcs, aliases) = (program.structs.len(), program.enums.len(), program.funcs.len(), program.aliases.len());
        let uses = parser.file(&mut program)?;
        if !module.is_empty() {
            let full = |name: &mut String| *name = format!("{module}.{name}");
            program.structs[structs..].iter_mut().for_each(|decl| full(&mut decl.name));
            program.enums[enums..].iter_mut().for_each(|decl| full(&mut decl.name));
            program.funcs[funcs..].iter_mut().for_each(|func| full(&mut func.name));
            program.aliases[aliases..].iter_mut().for_each(|alias| full(&mut alias.name));
        }
        program.files.push(FileScope { start: *start, end: *end, module: module.clone(), uses });
    }
    Ok(program)
}

struct Parser<'a> {
    src: &'a str,
    toks: Vec<Token>,
    i: usize,
    /// Off while parsing a condition: there `x {` starts the block, not a struct literal.
    struct_literals: bool,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Tok {
        &self.toks[self.i].tok
    }

    fn peek_at(&self, n: usize) -> &Tok {
        &self.toks[(self.i + n).min(self.toks.len() - 1)].tok
    }

    fn span(&self) -> Span {
        self.toks[self.i].span
    }

    fn prev_end(&self) -> usize {
        if self.i == 0 {
            0
        } else {
            self.toks[self.i - 1].span.end
        }
    }

    fn bump(&mut self) -> Token {
        let token = self.toks[self.i].clone();
        if self.i < self.toks.len() - 1 {
            self.i += 1;
        }
        token
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == tok {
            self.bump();
            true
        } else {
            false
        }
    }

    fn skip_newlines(&mut self) {
        while *self.peek() == Tok::Newline {
            self.bump();
        }
    }

    fn unexpected(&self, wanted: &str) -> Diag {
        let diag = Diag::error(self.span(), format!("expected {wanted}, found {}", self.peek().describe()));
        match self.peek() {
            Tok::Ident(name) if NOT_YET.contains(&name.as_str()) => {
                diag.note(format!("`{name}` is not in the interpreter yet"))
            }
            Tok::Ident(name) if WORDS.contains(&name.as_str()) => {
                diag.note(format!("`{name}` is a word of the language, not a name"))
            }
            _ => diag,
        }
    }

    /// Whether the next token is the word `word` (one of `WORDS`).
    fn at_word(&self, word: &str) -> bool {
        matches!(self.peek(), Tok::Ident(name) if name == word)
    }

    fn expect(&mut self, tok: Tok, wanted: &str) -> Res<Span> {
        if *self.peek() == tok {
            Ok(self.bump().span)
        } else {
            Err(self.unexpected(wanted))
        }
    }

    fn ident(&mut self, wanted: &str) -> Res<(String, Span)> {
        match self.peek() {
            Tok::Ident(name) if !NOT_YET.contains(&name.as_str()) && !WORDS.contains(&name.as_str()) => {
                let name = name.clone();
                Ok((name, self.bump().span))
            }
            _ => Err(self.unexpected(wanted)),
        }
    }

    /// `[read | ref] [shared | weak [checked | locked]] Name [<Type, ...>] { * | ? }`
    fn type_expr(&mut self) -> Res<TypeExpr> {
        let start = self.span();
        let mode = match self.peek() {
            Tok::Read => Some(Mode::Read),
            Tok::Ref => Some(Mode::Ref),
            _ => None,
        };
        if mode.is_some() {
            self.bump();
        }
        let holder = if self.at_word("shared") {
            Some(Holder::Shared)
        } else if self.at_word("weak") {
            Some(Holder::Weak)
        } else {
            None
        };
        let mut checked = false;
        let mut locked = false;
        if holder.is_some() {
            self.bump();
            checked = self.at_word("checked");
            locked = self.at_word("locked");
            if checked || locked {
                self.bump();
            }
            if self.at_word("checked") || self.at_word("locked") {
                return Err(Diag::error(self.span(), "a value with several owners is `checked` or `locked`, not both")
                    .note("`checked` lets one task change it, each access checked; `locked` lets several tasks change it, inside a `lock` block"));
            }
            if self.at_word("shared") || self.at_word("weak") {
                return Err(Diag::error(self.span(), "`shared` and `weak` are written once, in front of the type they hold"));
            }
        } else if self.at_word("checked") {
            return Err(Diag::error(self.span(), "`checked` is written after `shared` or `weak`: it is what makes a value with several owners changeable")
                .hint("`shared checked T` for an owner, `weak checked T` for a look at one"));
        } else if self.at_word("locked") {
            return Err(Diag::error(self.span(), "`locked` is written after `shared` or `weak`: several tasks own the value it holds")
                .hint("`shared locked T` for an owner, `weak locked T` for a look at one"));
        }
        let (mut name, _) = self.ident("a type name")?;
        // `billing.Line`: the module is written in front.
        while *self.peek() == Tok::Dot && matches!(self.peek_at(1), Tok::Ident(_)) {
            self.bump();
            name = format!("{name}.{}", self.ident("a type name")?.0);
        }
        let mut args = Vec::new();
        if self.eat(&Tok::Lt) {
            loop {
                args.push(self.type_expr()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(Tok::Gt, "`>`")?;
        }
        let mut suffixes = Vec::new();
        loop {
            match self.peek() {
                Tok::Star => suffixes.push(Suffix::Pointer),
                Tok::Question => suffixes.push(Suffix::Nullable),
                // `int[8]` is an array, `byte[]` a slice.
                Tok::LBracket => {
                    self.bump();
                    match *self.peek() {
                        Tok::RBracket => suffixes.push(Suffix::Slice),
                        Tok::Int(length) => {
                            let Ok(length) = usize::try_from(length) else {
                                return Err(Diag::error(self.span(), "an array cannot have that many items"));
                            };
                            suffixes.push(Suffix::Array(length));
                            self.bump();
                            if *self.peek() != Tok::RBracket {
                                return Err(self.unexpected("`]`"));
                            }
                        }
                        _ => {
                            return Err(self
                                .unexpected("the number of items or `]`")
                                .note("the length of an array is a number written in its type: `int[8]`; a slice is written `read int[]`"))
                        }
                    }
                }
                _ => break,
            }
            self.bump();
        }
        Ok(TypeExpr { mode, holder, checked, locked, name, args, suffixes, span: Span::new(start.start, self.prev_end()) })
    }

    /// `using NodeRef = shared Node`
    fn alias_decl(&mut self, public: bool) -> Res<AliasDecl> {
        self.bump();
        let (name, name_span) = self.ident("the new name of the type")?;
        if *self.peek() == Tok::Lt {
            return Err(Diag::error(self.span(), "an alias with type parameters is not in the interpreter yet")
                .note("generics of your own are deferred: `using Link<T> = ...` comes with them"));
        }
        self.expect(Tok::Assign, "`=`").map_err(|d| d.note("an alias is written `using NodeRef = shared Node`"))?;
        let ty = self.type_expr()?;
        Ok(AliasDecl { public, name, name_span, ty })
    }

    /// `use stock` / `use shop.stock`, one on a line.
    fn use_line(&mut self) -> Res<(String, Span)> {
        self.bump();
        let (mut module, start) = self.ident("the name of a module")?;
        while self.eat(&Tok::Dot) {
            module = format!("{module}.{}", self.ident("the name of a module")?.0);
        }
        Ok((module, Span::new(start.start, self.prev_end())))
    }

    /// One file: its `use` lines, then structs, enums, functions and tests. Gives the modules
    /// the file names with `use`.
    fn file(&mut self, program: &mut Program) -> Res<Vec<(String, Span)>> {
        let mut uses: Vec<(String, Span)> = Vec::new();
        let mut first = true;
        self.skip_newlines();
        while *self.peek() != Tok::Eof {
            if *self.peek() == Tok::Use {
                let line = self.span();
                if !first {
                    return Err(Diag::error(line, "`use` is written at the top of the file, before everything else"));
                }
                let (module, span) = self.use_line()?;
                if uses.iter().any(|(other, _)| *other == module) {
                    return Err(Diag::error(span, format!("`use {module}` is written twice")));
                }
                uses.push((module, span));
                if *self.peek() != Tok::Eof {
                    self.expect(Tok::Newline, "end of line")?;
                }
                self.skip_newlines();
                continue;
            }
            first = false;
            let public_span = self.span();
            let public = self.eat(&Tok::Public);
            let async_span = self.span();
            let is_async = self.at_word("async");
            if is_async {
                self.bump();
                if matches!(self.peek(), Tok::Struct | Tok::Enum) || self.at_word("using") || (matches!(self.peek(), Tok::Ident(word) if word == "test") && matches!(self.peek_at(1), Tok::Str(_))) {
                    return Err(Diag::error(async_span, "`async` is written in front of a function").hint("`async int count(string path) { ... }`"));
                }
            }
            // `test` is a word only here, in front of the name of a test.
            let at_test = matches!(self.peek(), Tok::Ident(word) if word == "test") && matches!(self.peek_at(1), Tok::Str(_));
            if at_test {
                if public {
                    return Err(Diag::error(public_span, "a test is not `public`: only the test command runs it"));
                }
                self.bump();
                let Tok::Str(name) = self.peek().clone() else { unreachable!("checked above") };
                let name_span = self.bump().span;
                let body = self.block()?;
                program.tests.push(TestDecl { name, name_span, body });
            } else if self.at_word("using") {
                let decl = self.alias_decl(public)?;
                program.aliases.push(decl);
                // A type can end in `>` or `*`, which at the end of a line would join it to
                // the next one: the line ends where the text says it does.
                if self.src[self.prev_end()..self.span().start].contains('\n') {
                    self.skip_newlines();
                    continue;
                }
            } else if *self.peek() == Tok::Struct {
                let mut decl = self.struct_decl()?;
                decl.public = public;
                program.structs.push(decl);
            } else if *self.peek() == Tok::Enum {
                let mut decl = self.enum_decl()?;
                decl.public = public;
                program.enums.push(decl);
            } else {
                let ret = self.type_expr().map_err(|d| {
                    d.hint("only structs, enums, functions and tests are written at the top level: `void main() { ... }`")
                })?;
                let (name, name_span) = self.ident("a function name")?;
                let mut func = self.func(ret, name, name_span, false)?;
                func.public = public;
                func.is_async = is_async;
                program.funcs.push(func);
            }
            if *self.peek() != Tok::Eof {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        Ok(uses)
    }

    fn struct_decl(&mut self) -> Res<StructDecl> {
        self.bump();
        let (name, name_span) = self.ident("a struct name")?;
        let sources = self.sources()?;
        let interfaces = self.interfaces()?;
        self.open_brace()?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        self.skip_newlines();
        while *self.peek() != Tok::RBrace {
            if *self.peek() == Tok::Eof {
                return Err(self.unexpected("`}`"));
            }
            let public = self.eat(&Tok::Public);
            self.no_async_method()?;
            let readonly_span = self.span();
            let readonly = self.eat(&Tok::Readonly);
            let ty = self.type_expr()?;
            let (member, span) = self.ident("a field or method name")?;
            if *self.peek() == Tok::LParen {
                if readonly {
                    return Err(Diag::error(readonly_span, "`readonly` is written on a field, not on a function"));
                }
                let mut method = self.func(ty, member, span, true)?;
                method.public = public;
                methods.push(method);
            } else {
                fields.push(FieldDecl { public, readonly, ty, name: member, span });
            }
            if *self.peek() != Tok::RBrace {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        self.bump();
        Ok(StructDecl { public: false, name, name_span, sources, interfaces, fields, methods })
    }

    /// `<from source>` after the name of a struct or an enum that carries a borrow.
    fn sources(&mut self) -> Res<Vec<(String, Span)>> {
        let mut list = Vec::new();
        if self.eat(&Tok::Lt) {
            loop {
                if !matches!(self.peek(), Tok::Ident(word) if word == "from") {
                    return Err(self.unexpected("`from`").note("a type names the source of what it borrows: `struct Token<from source>`; type parameters of your own are deferred"));
                }
                self.bump();
                list.push(self.ident("a name for the source")?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(Tok::Gt, "`>`")?;
        }
        Ok(list)
    }

    /// `: Copyable, Formattable` after the name of a struct or an enum.
    fn interfaces(&mut self) -> Res<Vec<(String, Span)>> {
        let mut list = Vec::new();
        if self.eat(&Tok::Colon) {
            loop {
                list.push(self.ident("an interface name")?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        Ok(list)
    }

    /// `enum Shape { Circle(int radius)` / `Empty }`: one variant per line.
    fn enum_decl(&mut self) -> Res<EnumDecl> {
        self.bump();
        let (name, name_span) = self.ident("an enum name")?;
        let sources = self.sources()?;
        let interfaces = self.interfaces()?;
        self.open_brace()?;
        let mut variants = Vec::new();
        let mut methods = Vec::new();
        self.skip_newlines();
        while *self.peek() != Tok::RBrace {
            if *self.peek() == Tok::Eof {
                return Err(self.unexpected("`}`"));
            }
            // A variant is a name alone on its line or followed by `(`; anything else is a
            // function: a type, then its name.
            let variant_line = matches!(self.peek(), Tok::Ident(_))
                && matches!(self.peek_at(1), Tok::LParen | Tok::Newline | Tok::RBrace | Tok::Comma | Tok::Eof);
            if !variant_line {
                let public = self.eat(&Tok::Public);
                self.no_async_method()?;
                let ty = self.type_expr()?;
                let (member, span) = self.ident("a method name")?;
                if *self.peek() != Tok::LParen {
                    return Err(Diag::error(span, "an enum holds variants and functions, not fields")
                        .hint(format!("a value every variant carries is written in each variant: `Name({} {member})`", &self.src[ty.span.start..ty.span.end])));
                }
                let mut method = self.func(ty, member, span, true)?;
                method.public = public;
                methods.push(method);
                if *self.peek() != Tok::RBrace {
                    self.expect(Tok::Newline, "end of line")?;
                }
                self.skip_newlines();
                continue;
            }
            let (variant, span) = self.ident("a variant name")?;
            let mut fields = Vec::new();
            if *self.peek() == Tok::LParen {
                let open = self.bump().span;
                while *self.peek() != Tok::RParen {
                    let ty = self.type_expr()?;
                    let (field, span) = self.ident("a field name")?;
                    fields.push(FieldDecl { public: true, readonly: false, ty, name: field, span });
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                let close = self.expect(Tok::RParen, "`)`")?;
                if fields.is_empty() {
                    return Err(Diag::error(open.to(close), "a variant that carries nothing is written without parentheses")
                        .hint(format!("`{variant}`")));
                }
            }
            variants.push(VariantDecl { name: variant, span, fields });
            if *self.peek() == Tok::Comma {
                return Err(Diag::error(self.span(), "variants are separated by line ends, not commas")
                    .hint("write each variant on its own line"));
            }
            if *self.peek() != Tok::RBrace {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        self.bump();
        Ok(EnumDecl { public: false, name, name_span, sources, interfaces, variants, methods })
    }

    /// The rest of a function after its return type and name. In a struct body the first
    /// parameter may be `this`, `read this` or `ref this`.
    fn func(&mut self, ret: TypeExpr, name: String, name_span: Span, in_struct: bool) -> Res<Func> {
        self.expect(Tok::LParen, "`(`")?;
        let mut this = None;
        let mut this_ty = None;
        let mut params: Vec<Param> = Vec::new();
        while *self.peek() != Tok::RParen {
            let start = self.span();
            let is_this = |tok: &Tok| matches!(tok, Tok::Ident(word) if word == "this");
            let mode = match self.peek() {
                tok if is_this(tok) => Some(This::Value),
                Tok::Read if is_this(self.peek_at(1)) => Some(This::Read),
                Tok::Ref if is_this(self.peek_at(1)) => Some(This::Ref),
                _ => None,
            };
            if let Some(mode) = mode {
                if mode != This::Value {
                    self.bump();
                }
                let span = start.to(self.bump().span);
                if !in_struct {
                    return Err(Diag::error(span, "`this` can only be a parameter of a method")
                        .hint("a method is written inside the body of its struct or enum"));
                }
                if this.is_some() || !params.is_empty() {
                    return Err(Diag::error(span, "`this` must be the first parameter"));
                }
                this = Some(mode);
            } else {
                let mut ty = self.type_expr()?;
                // `read FolderRef this`: `this` with a type, the value that holds the struct.
                if matches!(self.peek(), Tok::Ident(word) if word == "this") {
                    let span = start.to(self.bump().span);
                    if !in_struct {
                        return Err(Diag::error(span, "`this` can only be a parameter of a method")
                            .hint("a method is written inside the body of its struct or enum"));
                    }
                    if this.is_some() || !params.is_empty() {
                        return Err(Diag::error(span, "`this` must be the first parameter"));
                    }
                    let mode = match ty.mode.take() {
                        Some(Mode::Read) => This::Read,
                        Some(Mode::Ref) => {
                            return Err(Diag::error(span, "`this` with a type is taken with `read` or as it is, not with `ref`")
                                .hint("`read` lends the value that holds it, and what it holds is changed through that"));
                        }
                        None => This::Value,
                    };
                    this = Some(mode);
                    this_ty = Some(ty);
                } else {
                    let (name, span) = self.ident("a parameter name")?;
                    params.push(Param { ty, name, span });
                }
            }
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RParen, "`)`")?;
        // `from a, b`: where a returned borrow comes from.
        let mut from = Vec::new();
        if matches!(self.peek(), Tok::Ident(word) if word == "from") {
            self.bump();
            loop {
                let (name, start) = self.ident("the name of a parameter")?;
                let source = match self.eat(&Tok::Dot) {
                    true => Some(self.ident("the name of a source")?.0),
                    false => None,
                };
                from.push(FromItem { name, source, span: Span::new(start.start, self.prev_end()) });
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        let body = self.block()?;
        Ok(Func { public: false, is_async: false, ret, name, name_span, this, this_ty, params, from, body })
    }

    /// An async method is not here yet: `async` is written in front of a function only.
    fn no_async_method(&self) -> Res<()> {
        if self.at_word("async") {
            return Err(Diag::error(self.span(), "an `async` method is not in the interpreter yet")
                .hint("write it as an async function that takes the value: `async int count(read Store store)`"));
        }
        Ok(())
    }

    fn open_brace(&mut self) -> Res<()> {
        if *self.peek() == Tok::Newline && *self.peek_at(1) == Tok::LBrace {
            return Err(Diag::error(self.toks[self.i + 1].span, "`{` cannot start a new line")
                .hint("put `{` at the end of the previous line"));
        }
        self.expect(Tok::LBrace, "`{`")?;
        Ok(())
    }

    fn block(&mut self) -> Res<Block> {
        self.open_brace()?;
        let outer = std::mem::replace(&mut self.struct_literals, true);
        let mut stmts = Vec::new();
        self.skip_newlines();
        while *self.peek() != Tok::RBrace {
            if *self.peek() == Tok::Eof {
                return Err(self.unexpected("`}`"));
            }
            stmts.push(self.stmt()?);
            if *self.peek() != Tok::RBrace {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        self.bump();
        self.struct_literals = outer;
        Ok(stmts)
    }

    /// A condition or range bound: a struct literal must be parenthesized here.
    fn head_expr(&mut self, range_bound: bool) -> Res<Expr> {
        let outer = std::mem::replace(&mut self.struct_literals, false);
        let result = if range_bound { self.binary(BITWISE) } else { self.expr() };
        self.struct_literals = outer;
        result
    }

    /// How many tokens the type written `n` tokens ahead takes: a name, `<...>`, any `*` and
    /// `?`. `None` if no type is written there.
    fn type_len(&self, start: usize) -> Option<usize> {
        let holder = matches!(self.peek_at(start), Tok::Ident(word) if word == "shared" || word == "weak");
        let mut first = if holder { start + 1 } else { start };
        if holder && matches!(self.peek_at(first), Tok::Ident(word) if word == "checked" || word == "locked") {
            first += 1;
        }
        if !matches!(self.peek_at(first), Tok::Ident(_)) {
            return None;
        }
        let mut n = first + 1;
        while *self.peek_at(n) == Tok::Dot && matches!(self.peek_at(n + 1), Tok::Ident(_)) {
            n += 2;
        }
        if *self.peek_at(n) == Tok::Lt {
            loop {
                n += 1;
                if matches!(self.peek_at(n), Tok::Read | Tok::Ref) {
                    n += 1;
                }
                n += self.type_len(n)?;
                match self.peek_at(n) {
                    Tok::Comma => {}
                    Tok::Gt => break,
                    _ => return None,
                }
            }
            n += 1;
        }
        loop {
            match self.peek_at(n) {
                Tok::Star | Tok::Question => n += 1,
                Tok::LBracket if *self.peek_at(n + 1) == Tok::RBracket => n += 2,
                Tok::LBracket if matches!(self.peek_at(n + 1), Tok::Int(_)) && *self.peek_at(n + 2) == Tok::RBracket => n += 3,
                _ => break,
            }
        }
        Some(n - start)
    }

    /// The type comes first: `int x = 1`, `Node*? list = null`, `Result<int, E> r = f()`. A
    /// type and then a name is a declaration.
    fn at_declaration(&self) -> bool {
        // `await task` is not a variable `task` of the type `await`.
        !self.at_word("await") && self.type_len(0).is_some_and(|n| matches!(self.peek_at(n), Tok::Ident(_)))
    }

    /// Whether a declaration starts here and is followed by `=`.
    fn at_initialized_declaration(&self) -> bool {
        self.at_declaration() && self.type_len(0).is_some_and(|n| *self.peek_at(n + 1) == Tok::Assign)
    }

    /// Whether only line ends stand between here and the `}` that closes the block.
    fn at_block_end(&self) -> bool {
        let mut n = 0;
        while *self.peek_at(n) == Tok::Newline {
            n += 1;
        }
        *self.peek_at(n) == Tok::RBrace
    }

    fn stmt(&mut self) -> Res<Stmt> {
        match self.stmt_or_tail(Tail::Never)? {
            Ok(stmt) => Ok(stmt),
            Err(_) => unreachable!("a tail expression is only read when asked for"),
        }
    }

    /// A statement, or an expression that is the value of a `match` arm (see `Tail`).
    fn stmt_or_tail(&mut self, tail: Tail) -> Res<Result<Stmt, Expr>> {
        let tail_ok = |parser: &Self| match tail {
            Tail::Never => false,
            Tail::LastLine => parser.at_block_end(),
            Tail::Always => true,
        };
        let start = self.span();
        let kind = match self.peek().clone() {
            Tok::Var => {
                self.bump();
                let (name, name_span) = self.ident("a variable name")?;
                self.expect(Tok::Assign, "`=`")?;
                let init = self.expr()?;
                StmtKind::Let { ty: None, name, name_span, init }
            }
            Tok::If => self.if_stmt()?,
            Tok::While => {
                self.bump();
                let cond = self.head_expr(false)?;
                let body = self.block()?;
                StmtKind::While { cond, body, after: Vec::new() }
            }
            Tok::For => {
                self.bump();
                let (var, var_span) = self.ident("a loop variable")?;
                self.expect(Tok::In, "`in`")?;
                let from = self.head_expr(true)?;
                if *self.peek() == Tok::DotDotEq {
                    return Err(Diag::error(self.span(), "`..=` is written only in a pattern; a `for` goes up to its end, without it")
                        .hint("`for i in a..b + 1`"));
                }
                if self.eat(&Tok::DotDot) {
                    let to = self.head_expr(true)?;
                    let body = self.block()?;
                    StmtKind::For { var, var_span, from, to, body }
                } else {
                    let body = self.block()?;
                    StmtKind::ForEach { var, var_span, iter: from, body, by: None }
                }
            }
            // `read folder as f { ... }`; anything else that starts with `read` or `ref` is read
            // again as an expression.
            Tok::Read | Tok::Ref if self.as_block_ahead()? => {
                let mutable = self.bump().tok == Tok::Ref;
                let value = self.head_expr(false)?;
                self.bump();
                let (name, name_span) = self.ident("a name for what is opened")?;
                let body = self.block()?;
                StmtKind::As { mutable, value, name, name_span, body }
            }
            // `lock total as c { ... }`, `lock a as x, b as y { ... }`
            Tok::Ident(word) if word == "lock" => {
                self.bump();
                let mut items = Vec::new();
                loop {
                    let value = self.head_expr(false)?;
                    if !matches!(self.peek(), Tok::Ident(word) if word == "as") {
                        return Err(self.unexpected("`as` and a name for what is locked").hint("`lock total as c { c += 1 }`"));
                    }
                    self.bump();
                    let (name, name_span) = self.ident("a name for what is locked")?;
                    items.push(LockItem { value, name, name_span });
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                let body = self.block()?;
                StmtKind::Lock { items, body }
            }
            Tok::Return => {
                self.bump();
                if matches!(self.peek(), Tok::Newline | Tok::RBrace | Tok::Eof) {
                    StmtKind::Return(None)
                } else {
                    StmtKind::Return(Some(self.expr()?))
                }
            }
            Tok::Break => {
                self.bump();
                StmtKind::Break
            }
            Tok::Continue => {
                self.bump();
                StmtKind::Continue
            }
            Tok::Match => {
                let expr = self.match_expr()?;
                if tail_ok(self) {
                    return Ok(Err(expr));
                }
                StmtKind::Expr(expr)
            }
            Tok::Else => {
                return Err(Diag::error(start, "`else` cannot start a new line")
                    .hint("put `else` on the same line as `}`: `} else {`"));
            }
            Tok::AndAnd | Tok::OrOr | Tok::Plus | Tok::Star | Tok::Slash | Tok::Percent | Tok::Eq | Tok::Ne
            | Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge | Tok::QuestionQuestion | Tok::Amp | Tok::Pipe | Tok::Caret | Tok::Shl => {
                let text = self.peek().describe();
                return Err(Diag::error(start, format!("a line cannot start with {text}"))
                    .hint(format!("the statement ended on the previous line; to continue it, put {text} at the end of that line")));
            }
            Tok::Ident(word) if word == "test" && matches!(self.peek_at(1), Tok::Str(_)) => {
                return Err(Diag::error(start, "a test is written at the top level of the file, not inside a function"));
            }
            _ if self.at_declaration() && (tail == Tail::Never || self.at_initialized_declaration()) => {
                let ty = self.type_expr()?;
                let (name, name_span) = self.ident("a variable name")?;
                if *self.peek() != Tok::Assign {
                    let written = &self.src[ty.span.start..ty.span.end];
                    return Err(self
                        .unexpected("`=`")
                        .hint(format!("a variable is declared with an initial value: `{written} {name} = ...`")));
                }
                self.bump();
                let init = self.expr()?;
                StmtKind::Let { ty: Some(ty), name, name_span, init }
            }
            _ => {
                let expr = self.expr()?;
                if let Some(op) = assign_op(self.peek()) {
                    if !expr.is_place() {
                        return Err(Diag::error(expr.span, "the left side of an assignment must be a variable, a field or an item"));
                    }
                    self.bump();
                    let value = self.expr()?;
                    match (&expr.kind, op) {
                        (ExprKind::Var(name), None) if name == "_" => StmtKind::Discard(value),
                        _ => StmtKind::Assign { target: expr, op, value, through: false },
                    }
                } else if tail_ok(self) {
                    return Ok(Err(expr));
                } else if matches!(
                    expr.kind,
                    ExprKind::Call { .. } | ExprKind::Method { .. } | ExprKind::Match { .. } | ExprKind::Try { .. } | ExprKind::Await(_)
                ) {
                    StmtKind::Expr(expr)
                } else {
                    return Err(Diag::error(expr.span, "the value of this expression is not used")
                        .hint("only a call, an assignment or a declaration can be a statement"));
                }
            }
        };
        Ok(Ok(Stmt { kind, span: Span::new(start.start, self.prev_end()) }))
    }

    /// Whether `read x as name {` starts here: the word `as` follows the value.
    fn as_block_ahead(&mut self) -> Res<bool> {
        let at = self.i;
        self.bump();
        let found = self.head_expr(false).is_ok() && matches!(self.peek(), Tok::Ident(word) if word == "as");
        self.i = at;
        Ok(found)
    }

    /// `match subject { pattern [when condition] => body ... }`: one arm per line.
    fn match_expr(&mut self) -> Res<Expr> {
        let start = self.bump().span;
        let subject = self.head_expr(false)?;
        self.open_brace()?;
        let outer = std::mem::replace(&mut self.struct_literals, true);
        let mut arms = Vec::new();
        self.skip_newlines();
        while *self.peek() != Tok::RBrace {
            if *self.peek() == Tok::Eof {
                return Err(self.unexpected("`}`"));
            }
            let arm_start = self.span();
            let pattern = self.pattern(false)?;
            let guard = if matches!(self.peek(), Tok::Ident(word) if word == "when") {
                self.bump();
                Some(self.expr()?)
            } else {
                None
            };
            self.expect(Tok::FatArrow, "`=>`")?;
            let (body, value) = match self.peek() {
                Tok::LBrace => self.arm_block()?,
                _ => match self.stmt_or_tail(Tail::Always)? {
                    Ok(stmt) => (vec![stmt], None),
                    Err(value) => (Vec::new(), Some(value)),
                },
            };
            arms.push(Arm { pattern, guard, body, value, span: Span::new(arm_start.start, self.prev_end()) });
            if *self.peek() == Tok::Comma {
                return Err(Diag::error(self.span(), "arms are separated by line ends, not commas")
                    .hint("write each arm on its own line"));
            }
            if *self.peek() != Tok::RBrace {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        self.bump();
        self.struct_literals = outer;
        let span = Span::new(start.start, self.prev_end());
        Ok(Expr { ty: Ty::Void, moved: false, kind: ExprKind::Match { subject: Box::new(subject), arms, borrowed: false }, span })
    }

    /// The block of a `match` arm: statements, and the last line may be the value.
    fn arm_block(&mut self) -> Res<(Block, Option<Expr>)> {
        self.open_brace()?;
        let outer = std::mem::replace(&mut self.struct_literals, true);
        let mut stmts = Vec::new();
        let mut value = None;
        self.skip_newlines();
        while *self.peek() != Tok::RBrace {
            if *self.peek() == Tok::Eof {
                return Err(self.unexpected("`}`"));
            }
            match self.stmt_or_tail(Tail::LastLine)? {
                Ok(stmt) => stmts.push(stmt),
                Err(expr) => value = Some(expr),
            }
            if *self.peek() != Tok::RBrace {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        self.bump();
        self.struct_literals = outer;
        Ok((stmts, value))
    }

    /// `Circle(r)`, `Rect(w, _)`, `Add(Number(a), _)`, `Empty`, `"if"`, `42`, `_`, `var other`.
    /// `is_test`: after `is`, where `||` may follow the pattern as the operator.
    fn pattern(&mut self, is_test: bool) -> Res<Pattern> {
        self.alternatives(false, is_test)
    }

    /// `Add(_, _) or Multiply(_, _)`: one pattern, or several joined by `or`.
    fn alternatives(&mut self, inside: bool, is_test: bool) -> Res<Pattern> {
        let first = self.ranged(inside)?;
        let start = first.span.start;
        let mut list = vec![first];
        loop {
            match self.peek() {
                Tok::Or => {
                    self.bump();
                    self.skip_newlines();
                    list.push(self.ranged(inside)?);
                }
                Tok::Pipe => return Err(self.joined_wrong("|")),
                Tok::OrOr if !is_test => return Err(self.joined_wrong("||")),
                _ => break,
            }
        }
        if list.len() == 1 {
            return Ok(list.pop().expect("one pattern"));
        }
        Ok(Pattern { kind: PatternKind::Or(list), span: Span::new(start, self.prev_end()) })
    }

    /// `'+' | '-'` or `'+' || '-'` in a pattern.
    fn joined_wrong(&self, written: &str) -> Diag {
        Diag::error(self.span(), format!("patterns are joined with `or`, not `{written}`"))
            .note("`|` and `||` are operators on values; a pattern is not a value: `'+' or '-'`")
            .hint("`or`")
    }

    /// A pattern, or a range of numbers or characters: `'a'..='z'`, `0..=9`.
    fn ranged(&mut self, inside: bool) -> Res<Pattern> {
        let low = self.pattern_at(inside)?;
        let end = |pattern: &Pattern| match pattern.kind {
            PatternKind::Int(value) => Some(value.to_string()),
            PatternKind::Char(value) => Some(format!("'{}'", value.escape_default())),
            _ => None,
        };
        match self.peek() {
            Tok::DotDotEq => {}
            Tok::DotDot => {
                let diag = Diag::error(self.span(), "a range in a pattern takes its last value too, and is written with `..=`");
                let Some(first) = end(&low) else { return Err(diag) };
                return Err(match self.peek_at(1) {
                    Tok::Int(last) => diag.hint(format!("`{first}..={last}`")),
                    Tok::Char(last) => diag.hint(format!("`{first}..='{}'`", last.escape_default())),
                    _ => diag,
                });
            }
            _ => return Ok(low),
        }
        let dots = self.bump().span;
        if end(&low).is_none() {
            return Err(Diag::error(low.span.to(dots), "a range in a pattern goes from a number or a character to another"));
        }
        let high = self.pattern_at(inside)?;
        if end(&high).is_none() {
            return Err(Diag::error(high.span, "a range in a pattern ends with a number or a character"));
        }
        let span = low.span.to(high.span);
        Ok(Pattern { kind: PatternKind::Range(Box::new(low), Box::new(high)), span })
    }

    /// A pattern; `inside` is true within the parentheses of a variant, where a name alone
    /// takes the value.
    fn pattern_at(&mut self, inside: bool) -> Res<Pattern> {
        let start = self.span();
        let kind = match self.peek().clone() {
            Tok::Var if inside => {
                self.bump();
                let (name, span) = self.ident("a variable name")?;
                return Err(Diag::error(start.to(span), "inside the parentheses a name alone takes the value; `var` is not written there")
                    .hint(format!("`{name}`")));
            }
            Tok::Var => {
                self.bump();
                PatternKind::Bind(self.ident("a variable name")?.0)
            }
            Tok::Ident(word) if word == "_" => {
                self.bump();
                PatternKind::Wildcard
            }
            Tok::Null => {
                self.bump();
                PatternKind::Null
            }
            Tok::True | Tok::False => PatternKind::Bool(self.bump().tok == Tok::True),
            Tok::Char(value) => {
                self.bump();
                PatternKind::Char(value)
            }
            Tok::Int(_) | Tok::Float(_) | Tok::Minus => {
                let negative = self.eat(&Tok::Minus);
                if let Tok::Float(_) = self.peek() {
                    let span = start.to(self.span());
                    return Err(Diag::error(span, "a fractional number cannot be a pattern")
                        .note("two fractional numbers are seldom exactly equal; compare in an `if`: `if x < 0.5 { ... }`"));
                }
                let Tok::Int(value) = *self.peek() else { return Err(self.unexpected("a number")) };
                let span = start.to(self.bump().span);
                let value = if negative { -(value as i128) } else { value as i128 };
                if !(IntTy::Long.min()..=IntTy::Ulong.max()).contains(&value) {
                    return Err(Diag::error(span, format!("{value} does not fit in any integer type")));
                }
                PatternKind::Int(value)
            }
            Tok::Str(text) => {
                self.bump();
                PatternKind::Str(text)
            }
            Tok::Interp(_) => {
                return Err(Diag::error(start, "a pattern is a text as it is written, without `$`")
                    .note("to compare with a text that is built, use `==` in an `if` or a `when`"))
            }
            Tok::Ident(_) => {
                let (name, _) = self.ident("a variant name")?;
                if *self.peek() == Tok::Dot {
                    let diag = Diag::error(start.to(self.span()), "a pattern names the variant alone, without its enum");
                    return Err(match self.peek_at(1) {
                        Tok::Ident(variant) => diag.hint(format!("`{variant}`")),
                        _ => diag,
                    });
                }
                if inside && *self.peek() != Tok::LParen {
                    PatternKind::Bind(name)
                } else {
                    let mut args = None;
                    if self.eat(&Tok::LParen) {
                        let mut list = Vec::new();
                        while *self.peek() != Tok::RParen {
                            list.push(self.alternatives(true, false)?);
                            if !self.eat(&Tok::Comma) {
                                break;
                            }
                        }
                        self.expect(Tok::RParen, "`)`")?;
                        args = Some(list);
                    }
                    PatternKind::Variant { name, args, index: 0 }
                }
            }
            _ => {
                return Err(self
                    .unexpected("a pattern")
                    .note("a pattern is a variant such as `Circle(r)`, a number, a character, a text, a range such as `'a'..='z'`, `null`, `true`, `false`, `_`, or `var name`; patterns are joined with `or`"))
            }
        };
        Ok(Pattern { kind, span: Span::new(start.start, self.prev_end()) })
    }

    fn if_stmt(&mut self) -> Res<StmtKind> {
        self.bump();
        let cond = self.head_expr(false)?;
        if *self.peek() == Tok::Assign {
            return Err(Diag::error(self.span(), "an assignment cannot be a condition").hint("comparison is written `==`"));
        }
        let then = self.block()?;
        let els = if self.eat(&Tok::Else) {
            if *self.peek() == Tok::If {
                let start = self.span();
                let kind = self.if_stmt()?;
                Some(vec![Stmt { kind, span: Span::new(start.start, self.prev_end()) }])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(StmtKind::If { cond, then, els, after: Vec::new() })
    }

    /// `a ?? b` binds loosest and groups to the right, as in C#.
    fn expr(&mut self) -> Res<Expr> {
        let left = self.binary(0)?;
        if !self.eat(&Tok::QuestionQuestion) {
            return Ok(left);
        }
        // The right side can leave the function: `x ?? return Error(...)`.
        let right = if *self.peek() == Tok::Return {
            let start = self.bump().span;
            let value = match self.peek() {
                Tok::Newline | Tok::RBrace | Tok::RParen | Tok::Comma | Tok::Eof => None,
                _ => Some(Box::new(self.expr()?)),
            };
            Expr { ty: Ty::Void, moved: false, kind: ExprKind::Return(value), span: Span::new(start.start, self.prev_end()) }
        } else {
            self.expr()?
        };
        let span = left.span.to(right.span);
        Ok(Expr { ty: Ty::Void, moved: false, kind: ExprKind::Coalesce { left: Box::new(left), right: Box::new(right), on_result: false }, span })
    }

    /// From loosest to tightest: `||`, `&&`, equality, comparison and `is`, `|`, `^`, `&`,
    /// shifts, additive, multiplicative. The same as C# except that `&`, `^` and `|` bind
    /// tighter than a comparison, so `flags & mask == 0` tests the masked value.
    fn binary(&mut self, level: usize) -> Res<Expr> {
        const LEVELS: &[&[(Tok, BinOp)]] = &[
            &[(Tok::OrOr, BinOp::Or)],
            &[(Tok::AndAnd, BinOp::And)],
            &[(Tok::Eq, BinOp::Eq), (Tok::Ne, BinOp::Ne)],
            &[(Tok::Lt, BinOp::Lt), (Tok::Le, BinOp::Le), (Tok::Gt, BinOp::Gt), (Tok::Ge, BinOp::Ge)],
            &[(Tok::Pipe, BinOp::BitOr)],
            &[(Tok::Caret, BinOp::BitXor)],
            &[(Tok::Amp, BinOp::BitAnd)],
            &[(Tok::Shl, BinOp::Shl)],
            &[(Tok::Plus, BinOp::Add), (Tok::Minus, BinOp::Sub)],
            &[(Tok::Star, BinOp::Mul), (Tok::Slash, BinOp::Div), (Tok::Percent, BinOp::Rem)],
        ];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut left = self.binary(level + 1)?;
        loop {
            if level == COMPARISON && *self.peek() == Tok::Is {
                left = self.is_test(left)?;
                continue;
            }
            let op = if level == SHIFT && self.at_shift_right() {
                self.bump();
                BinOp::Shr
            } else if let Some(&(_, op)) = LEVELS[level].iter().find(|(tok, _)| tok == self.peek()) {
                op
            } else {
                return Ok(left);
            };
            self.bump();
            let right = self.binary(level + 1)?;
            let span = left.span.to(right.span);
            left = Expr { ty: Ty::Void, moved: false, kind: ExprKind::Binary(op, Box::new(left), Box::new(right)), span };
        }
    }

    /// `>>` is two `>` with nothing between them (see `Tok::Shl`).
    fn at_shift_right(&self) -> bool {
        *self.peek() == Tok::Gt && *self.peek_at(1) == Tok::Gt && self.toks[self.i].span.end == self.toks[self.i + 1].span.start
    }

    /// `x is null`, `x is not null`, `x is Circle(r)`, `x is not Circle`
    fn is_test(&mut self, operand: Expr) -> Res<Expr> {
        self.bump();
        let negated = matches!(self.peek(), Tok::Ident(word) if word == "not");
        if negated {
            self.bump();
        }
        let start = operand.span.start;
        let expr = Box::new(operand);
        let kind = match self.peek() {
            Tok::Null => {
                self.bump();
                ExprKind::IsNull { expr, negated }
            }
            Tok::Ident(word) if word != "_" => {
                let pattern = self.pattern(true)?;
                ExprKind::IsVariant { expr, negated, pattern, by_ref: false }
            }
            _ => return Err(self.unexpected("`null` or a variant name").note("`is` tests for `null` or for one variant of an enum")),
        };
        Ok(Expr { ty: Ty::Void, moved: false, kind, span: Span::new(start, self.prev_end()) })
    }

    fn unary(&mut self) -> Res<Expr> {
        let start = self.span();
        let wrap = |kind: fn(Box<Expr>) -> ExprKind, operand: Expr| {
            let span = start.to(operand.span);
            Expr { ty: Ty::Void, moved: false, kind: kind(Box::new(operand)), span }
        };
        match self.peek() {
            Tok::Minus => {
                self.bump();
                // `-2147483648` is one literal; read separately, 2147483648 would not fit in int.
                if let Tok::Int(value) = *self.peek() {
                    if !matches!(self.peek_at(1), Tok::Dot) {
                        let span = start.to(self.bump().span);
                        return int_literal(-(value as i128), span);
                    }
                }
                let operand = self.unary()?;
                Ok(wrap(|e| ExprKind::Unary(UnOp::Neg, e), operand))
            }
            Tok::Bang => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(|e| ExprKind::Unary(UnOp::Not, e), operand))
            }
            Tok::Tilde => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(|e| ExprKind::Unary(UnOp::BitNot, e), operand))
            }
            Tok::New => {
                self.bump();
                if self.at_word("shared") {
                    self.bump();
                    let checked = self.at_word("checked");
                    let locked = self.at_word("locked");
                    if checked || locked {
                        self.bump();
                    }
                    let operand = self.unary()?;
                    let span = start.to(operand.span);
                    return Ok(Expr { ty: Ty::Void, moved: false, kind: ExprKind::NewShared { value: Box::new(operand), checked, locked }, span });
                }
                if self.at_word("checked") || self.at_word("locked") {
                    let word = if self.at_word("checked") { "checked" } else { "locked" };
                    return Err(Diag::error(self.span(), format!("`{word}` is written after `shared`: `new shared {word} ...`")));
                }
                if self.at_word("weak") {
                    return Err(Diag::error(self.span(), "a `weak` value is not made with `new`: it looks at a `shared` one")
                        .hint("make it `shared` and look at it with `weak`: `var owner = new shared Node { ... }`, then `weak owner`"));
                }
                let operand = self.unary()?;
                Ok(wrap(ExprKind::New, operand))
            }
            Tok::Ident(word) if word == "share" => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(ExprKind::Share, operand))
            }
            Tok::Ident(word) if word == "weak" => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(ExprKind::WeakOf, operand))
            }
            Tok::Ident(word) if word == "await" => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(ExprKind::Await, operand))
            }
            // A prefix, like `await`: `try a().b()` tries the whole chain.
            Tok::Try => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(|e| ExprKind::Try { expr: e, kind: TryKind::Null }, operand))
            }
            Tok::Read => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(|e| ExprKind::Borrow { mutable: false, expr: e }, operand))
            }
            Tok::Ref => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(|e| ExprKind::Borrow { mutable: true, expr: e }, operand))
            }
            // `(int)x`, `(char)code`: a number type or `char` alone in parentheses is a cast.
            Tok::LParen => match self.cast_type() {
                Some(ty) => {
                    self.bump();
                    self.bump();
                    self.bump();
                    let operand = self.unary()?;
                    let span = start.to(operand.span);
                    Ok(Expr { ty: Ty::Void, moved: false, kind: ExprKind::Cast { ty, expr: Box::new(operand) }, span })
                }
                None => self.postfix(),
            },
            _ => self.postfix(),
        }
    }

    fn cast_type(&self) -> Option<Ty> {
        let Tok::Ident(name) = self.peek_at(1) else { return None };
        if *self.peek_at(2) != Tok::RParen {
            return None;
        }
        Ty::from_name(name).filter(|ty| ty.is_number() || *ty == Ty::Char)
    }

    fn postfix(&mut self) -> Res<Expr> {
        let mut expr = self.primary()?;
        loop {
            if *self.peek() == Tok::LBracket {
                expr = self.index_or_slice(expr)?;
                continue;
            }
            if !self.eat(&Tok::Dot) {
                break;
            }
            let (name, name_span) = self.ident("a field or method name")?;
            // `billing.Line { ... }`: a struct with its module written in front.
            if *self.peek() == Tok::LBrace && self.struct_literals {
                if let Some(module) = dotted(&expr) {
                    let start = expr.span.start;
                    let fields = self.struct_fields()?;
                    let kind = ExprKind::StructLit { name: format!("{module}.{name}"), name_span: Span::new(start, name_span.end), fields };
                    expr = Expr { ty: Ty::Void, moved: false, kind, span: Span::new(start, self.prev_end()) };
                    continue;
                }
            }
            let recv = Box::new(expr);
            let kind = if *self.peek() == Tok::LParen {
                let args = self.args()?;
                ExprKind::Method { recv, name, name_span, args, callee: Callee::Plain }
            } else {
                ExprKind::Field { recv, name, name_span, index: 0 }
            };
            let start = match &kind {
                ExprKind::Method { recv, .. } | ExprKind::Field { recv, .. } => recv.span.start,
                _ => unreachable!(),
            };
            expr = Expr { ty: Ty::Void, moved: false, kind, span: Span::new(start, self.prev_end()) };
        }
        Ok(expr)
    }

    /// `text[i]`, or a range of it: `text[a..b]`, `text[..b]`, `text[a..]`, `text[..]`.
    fn index_or_slice(&mut self, recv: Expr) -> Res<Expr> {
        self.bump();
        let start = recv.span.start;
        let recv = Box::new(recv);
        let kind = self.bracketed(|p| {
            let from = match p.peek() {
                Tok::DotDot | Tok::DotDotEq => None,
                _ => Some(Box::new(p.expr()?)),
            };
            if *p.peek() == Tok::DotDotEq {
                return Err(Diag::error(p.span(), "`..=` is written only in a pattern; a range of a text or a slice stops before its end")
                    .hint("`[a..b + 1]`"));
            }
            if !p.eat(&Tok::DotDot) {
                let index = from.expect("read above when there is no `..`");
                return Ok(ExprKind::Index { recv, index, kind: IndexKind::Text });
            }
            let to = match p.peek() {
                Tok::RBracket => None,
                _ => Some(Box::new(p.expr()?)),
            };
            Ok(ExprKind::Slice { recv, from, to })
        })?;
        self.expect(Tok::RBracket, "`]`")?;
        Ok(Expr { ty: Ty::Void, moved: false, kind, span: Span::new(start, self.prev_end()) })
    }

    /// Runs `inner` between brackets, where a struct literal is always unambiguous.
    fn bracketed<T>(&mut self, inner: impl FnOnce(&mut Self) -> Res<T>) -> Res<T> {
        let outer = std::mem::replace(&mut self.struct_literals, true);
        let result = inner(self);
        self.struct_literals = outer;
        result
    }

    fn args(&mut self) -> Res<Vec<Expr>> {
        self.expect(Tok::LParen, "`(`")?;
        let args = self.bracketed(|p| {
            let mut args = Vec::new();
            while *p.peek() != Tok::RParen {
                if matches!(p.peek(), Tok::Ident(_)) && *p.peek_at(1) == Tok::FatArrow {
                    return Err(Diag::error(p.span().to(p.toks[p.i + 1].span), "there are no lambdas in the language")
                        .hint("write a function and pass its name: `apply(double, 5)`"));
                }
                args.push(p.expr()?);
                if !p.eat(&Tok::Comma) {
                    break;
                }
            }
            Ok(args)
        })?;
        self.expect(Tok::RParen, "`)`")?;
        Ok(args)
    }

    fn primary(&mut self) -> Res<Expr> {
        let start = self.span();
        let kind = match self.peek().clone() {
            Tok::Int(value) => {
                self.bump();
                return int_literal(value as i128, start);
            }
            Tok::Float(text) => {
                self.bump();
                ExprKind::Float(text, FloatTy::Double)
            }
            Tok::True => {
                self.bump();
                ExprKind::Bool(true)
            }
            Tok::False => {
                self.bump();
                ExprKind::Bool(false)
            }
            Tok::Null => {
                self.bump();
                ExprKind::Null
            }
            Tok::Str(text) => {
                self.bump();
                ExprKind::Str(text)
            }
            Tok::Char(value) => {
                self.bump();
                ExprKind::Char(value)
            }
            Tok::Interp(parts) => {
                self.bump();
                ExprKind::Interp(self.interp_parts(parts)?)
            }
            Tok::Match => return self.match_expr(),
            Tok::LParen => {
                self.bump();
                let inner = self.bracketed(|p| p.expr())?;
                self.expect(Tok::RParen, "`)`")?;
                return Ok(Expr { ty: Ty::Void, moved: false, kind: inner.kind, span: Span::new(start.start, self.prev_end()) });
            }
            Tok::Ident(_) if self.at_collection() => {
                let ty = self.type_expr()?;
                let args = if *self.peek() == Tok::LParen { self.args()? } else { Vec::new() };
                let items = if *self.peek() == Tok::LBrace && self.struct_literals { Some(self.items()?) } else { None };
                ExprKind::Collection { ty, args, items }
            }
            Tok::Ident(_) if self.at_array() => {
                let ty = self.type_expr()?;
                let args = if *self.peek() == Tok::LParen { self.args()? } else { Vec::new() };
                let items = match *self.peek() == Tok::LBrace && self.struct_literals {
                    true => {
                        let mut values = Vec::new();
                        for mut item in self.items()? {
                            if item.braced {
                                return Err(Diag::error(item.span, "an item of an array is one value, written without braces").hint("`int[3] { 1, 2, 3 }`"));
                            }
                            values.push(item.values.remove(0));
                        }
                        Some(values)
                    }
                    false => None,
                };
                ExprKind::Array { ty, args, items }
            }
            Tok::Ident(_) => {
                let (name, name_span) = self.ident("an expression")?;
                if *self.peek() == Tok::LParen {
                    let args = self.args()?;
                    ExprKind::Call { name, name_span, args, callee: Callee::Plain }
                } else if *self.peek() == Tok::LBrace && self.struct_literals {
                    let fields = self.struct_fields()?;
                    ExprKind::StructLit { name, name_span, fields }
                } else {
                    ExprKind::Var(name)
                }
            }
            Tok::Return => {
                return Err(self.unexpected("an expression").note("inside an expression `return` is written only on the right side of `??`"))
            }
            _ => return Err(self.unexpected("an expression")),
        };
        Ok(Expr { ty: Ty::Void, moved: false, kind, span: Span::new(start.start, self.prev_end()) })
    }

    /// `{ value = 1, next = null }`; fields are separated by commas, line ends do not matter.
    fn struct_fields(&mut self) -> Res<Vec<FieldInit>> {
        self.bump();
        let fields = self.bracketed(|p| {
            let mut fields = Vec::new();
            p.skip_newlines();
            while *p.peek() != Tok::RBrace {
                let (name, span) = p.ident("a field name")?;
                p.expect(Tok::Assign, "`=`")?;
                let value = p.expr()?;
                fields.push(FieldInit { name, span, value, index: 0 });
                p.skip_newlines();
                if !p.eat(&Tok::Comma) {
                    break;
                }
                p.skip_newlines();
            }
            Ok(fields)
        })?;
        self.expect(Tok::RBrace, "`}`")?;
        Ok(fields)
    }

    /// Whether a type with `<...>` is written here and then built: `List<int>()`,
    /// `List<int> { 1, 2 }`. As in C#, `a < b > (c)` is read this way too.
    fn at_collection(&self) -> bool {
        if *self.peek_at(1) != Tok::Lt {
            return false;
        }
        let Some(n) = self.type_len(0) else { return false };
        *self.peek_at(n - 1) == Tok::Gt
            && (*self.peek_at(n) == Tok::LParen || (*self.peek_at(n) == Tok::LBrace && self.struct_literals))
    }

    /// Whether an array is built here: `int[3] { 1, 2, 3 }`, `byte[16](255)`, `int?[4]`. A
    /// name, a number in brackets and nothing after them (`byte[16]`) reads like an index; the
    /// checker tells the two apart, since only it knows whether the name is a type.
    fn at_array(&self) -> bool {
        let Some(n) = self.type_len(0) else { return false };
        if *self.peek_at(n - 1) != Tok::RBracket {
            return false;
        }
        let first = (0..n).find(|at| *self.peek_at(*at) == Tok::LBracket).expect("the type ends in `]`");
        let typed = (0..first).any(|at| matches!(self.peek_at(at), Tok::Lt | Tok::Star | Tok::Question));
        typed || *self.peek_at(n) == Tok::LParen || (*self.peek_at(n) == Tok::LBrace && self.struct_literals)
    }

    /// `{ 1, 2 }` or `{ {"x", 10}, {"y", 20} }`; line ends do not matter.
    fn items(&mut self) -> Res<Vec<Item>> {
        self.bump();
        let items = self.bracketed(|p| {
            let mut items = Vec::new();
            p.skip_newlines();
            while *p.peek() != Tok::RBrace {
                let start = p.span();
                let braced = p.eat(&Tok::LBrace);
                let mut values = Vec::new();
                if braced {
                    while *p.peek() != Tok::RBrace {
                        values.push(p.expr()?);
                        if !p.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    p.expect(Tok::RBrace, "`}`")?;
                } else {
                    values.push(p.expr()?);
                }
                items.push(Item { values, braced, span: Span::new(start.start, p.prev_end()) });
                p.skip_newlines();
                if !p.eat(&Tok::Comma) {
                    break;
                }
                p.skip_newlines();
            }
            Ok(items)
        })?;
        self.expect(Tok::RBrace, "`}`")?;
        Ok(items)
    }

    fn interp_parts(&mut self, parts: Vec<Part>) -> Res<Vec<InterpPart>> {
        let mut result = Vec::new();
        for part in parts {
            match part {
                Part::Text(text) => result.push(InterpPart::Text(text)),
                Part::Code(range, spec) => {
                    let toks = lex(self.src, range, true)?;
                    let mut inner = Parser { src: self.src, toks, i: 0, struct_literals: true };
                    let expr = inner.expr()?;
                    if *inner.peek() != Tok::Eof {
                        let diag = inner.unexpected("`}`");
                        return Err(match *inner.peek() == Tok::Comma {
                            true => diag.note("a width after `,` is a number written in the text: `{n,8}`"),
                            false => diag,
                        });
                    }
                    result.push(InterpPart::Expr(expr, spec));
                }
            }
        }
        Ok(result)
    }
}

/// Where an expression may stand as the value of a `match` arm instead of a statement.
#[derive(Clone, Copy, PartialEq)]
enum Tail {
    Never,
    /// On the last line of an arm's block.
    LastLine,
    /// In an arm written without a block: `Circle(r) => 3 * r * r`.
    Always,
}

const COMPARISON: usize = 3;
/// The loosest level that is not a comparison: where a range bound starts.
const BITWISE: usize = 4;
const SHIFT: usize = 7;

/// `a.b.c` written with names only, as one text.
pub fn dotted(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Var(name) => Some(name.clone()),
        ExprKind::Field { recv, name, .. } => Some(format!("{}.{name}", dotted(recv)?)),
        _ => None,
    }
}

/// A number fits some type if it lies between the least `long` and the largest `ulong`.
fn int_literal(value: i128, span: Span) -> Res<Expr> {
    if !(IntTy::Long.min()..=IntTy::Ulong.max()).contains(&value) {
        return Err(Diag::error(span, format!("{value} does not fit in any integer type")));
    }
    Ok(Expr { ty: Ty::Void, moved: false, kind: ExprKind::Int(value, IntTy::Int), span })
}

/// `Some` for an assignment operator; for compound assignment (`+=`) with the inner operator.
fn assign_op(tok: &Tok) -> Option<Option<BinOp>> {
    match tok {
        Tok::Assign => Some(None),
        Tok::PlusEq => Some(Some(BinOp::Add)),
        Tok::MinusEq => Some(Some(BinOp::Sub)),
        Tok::StarEq => Some(Some(BinOp::Mul)),
        Tok::SlashEq => Some(Some(BinOp::Div)),
        Tok::PercentEq => Some(Some(BinOp::Rem)),
        Tok::AmpEq => Some(Some(BinOp::BitAnd)),
        Tok::PipeEq => Some(Some(BinOp::BitOr)),
        Tok::CaretEq => Some(Some(BinOp::BitXor)),
        Tok::ShlEq => Some(Some(BinOp::Shl)),
        Tok::ShrEq => Some(Some(BinOp::Shr)),
        _ => None,
    }
}
