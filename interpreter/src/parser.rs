//! Hand-written recursive descent parser.

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::lexer::{lex, Part, Tok, Token};
use crate::types::{FloatTy, IntTy, Ty};

/// Words that are in the design but not in the interpreter yet. They get an explicit
/// message instead of "unknown name".
const NOT_YET: &[&str] = &[
    "interface",
];

/// Words of the language that are read where they stand, not names: `shared T`, `weak T`,
/// `shared checked T`, `shared locked T`, `share x`, `weak x`,
/// `lock x as c { }`, `async int f()`, `await t`, `raw byte p`, `unsafe { }`.
/// `using` is kept: a type alias was written with it once, and the error that meets it names `alias`.
const WORDS: &[&str] = &["shared", "weak", "share", "using", "checked", "locked", "lock", "async", "await", "raw", "unsafe", "extern", "export", "atomic", "const", "static"];

type Res<T> = Result<T, Diag>;

pub fn parse(src: &str) -> Res<Program> {
    parse_files(src, &[(0, src.len(), String::new(), Default::default())])
}

/// A file of a program: where it lies in the text, its module and how it writes the names of
/// modules.
pub type FileIn = (usize, usize, String, std::sync::Arc<crate::ast::ModuleMap>);

/// Parses the files of a program, each given as where it lies in `src` and the module it
/// belongs to. What a module defines gets its full name here: `Product` in the folder `stock`
/// is `stock.Product`.
pub fn parse_files(src: &str, files: &[FileIn]) -> Res<Program> {
    let mut program = Program { structs: Vec::new(), enums: Vec::new(), funcs: Vec::new(), tests: Vec::new(), aliases: Vec::new(), files: Vec::new(), plain: Default::default(), foreign: Default::default(), cvars: Vec::new(), cvar_types: Default::default(), blocks: Vec::new(), exported: Default::default(), variadic: Default::default(), library: false, freestanding: false, other_machine: false, left_out: Default::default(), lib_dirs: Vec::new(), home: None, consts: Vec::new(), const_values: Default::default(), embeds: Vec::new(), sizes: Default::default(), statics: Vec::new(), value_layouts: Default::default(), windows: Default::default() };
    for (start, end, module, map) in files {
        let toks = lex(src, Span::new(*start, *end), false)?;
        let mut parser = Parser { src, map: map.clone(), toks, i: 0, struct_literals: true, visible: Vec::new(), embed_base: program.embeds.len(), embeds: Vec::new(), in_text: false, consts_found: Vec::new(), in_const: false, visible_statics: Vec::new(), statics_found: Vec::new(), depth: 0, declaring: None };
        let (structs, enums, funcs, aliases) = (program.structs.len(), program.enums.len(), program.funcs.len(), program.aliases.len());
        let (cvars, blocks, consts, statics) = (program.cvars.len(), program.blocks.len(), program.consts.len(), program.statics.len());
        let uses = parser.file(&mut program)?;
        program.embeds.append(&mut parser.embeds);
        program.blocks[blocks..].iter_mut().for_each(|(of, _)| *of = module.clone());
        if !module.is_empty() {
            let full = |name: &mut String| *name = format!("{module}.{name}");
            program.structs[structs..].iter_mut().for_each(|decl| full(&mut decl.name));
            program.enums[enums..].iter_mut().for_each(|decl| full(&mut decl.name));
            program.funcs[funcs..].iter_mut().for_each(|func| full(&mut func.name));
            program.aliases[aliases..].iter_mut().for_each(|alias| full(&mut alias.name));
            program.cvars[cvars..].iter_mut().for_each(|var| full(&mut var.name));
            program.consts[consts..].iter_mut().for_each(|decl| full(&mut decl.name));
            for decl in &mut program.statics[statics..] {
                full(&mut decl.name);
                decl.init = static_init(&decl.name);
            }
        }
        program.consts.append(&mut parser.consts_found);
        // A static of a block, and the function that gives its value.
        for (decl, init) in parser.statics_found.drain(..) {
            program.statics.push(decl);
            program.funcs.push(init);
        }
        program.files.push(FileScope { start: *start, end: *end, module: module.clone(), uses, map: map.clone() });
    }
    Ok(program)
}

/// How deep a program may nest blocks, expressions, the operators of a chain (`a + b + c`),
/// the fields, calls and indexes read one after another, types and patterns.
pub const MAX_NESTING: usize = 1000;

struct Parser<'a> {
    src: &'a str,
    /// How the file writes the names of modules: a name with a module in front is kept by the
    /// full name of the module.
    map: std::sync::Arc<crate::ast::ModuleMap>,
    toks: Vec<Token>,
    i: usize,
    /// Off while parsing a condition: there `x {` starts the block, not a struct literal.
    struct_literals: bool,
    /// The consts of the blocks being read, as written and by their full names: a block opens
    /// a list and closes it.
    visible: Vec<Vec<(String, String)>>,
    /// The files `embed` names in this file, which come after `embed_base` others.
    embeds: Vec<Embed>,
    embed_base: usize,
    /// Reading the code inside `{ }` of a `$"..."`.
    in_text: bool,
    /// The consts written in blocks, which the program keeps with the others.
    consts_found: Vec<ConstDecl>,
    /// Reading what a const is given: `embed` may be written there.
    in_const: bool,
    /// The statics of the blocks being read, as `visible` holds the consts.
    visible_statics: Vec<Vec<(String, String)>>,
    /// The statics written in blocks, with the functions that give their values.
    statics_found: Vec<(StaticDecl, Func)>,
    /// How deep the tree being read is nested: blocks, expressions, the operators of a chain,
    /// types and patterns. At most `MAX_NESTING`.
    depth: usize,
    /// The variable whose value is being read (`var n = ...`), for the hints that name it.
    declaring: Option<String>,
}

impl<'a> Parser<'a> {
    /// Reads one level deeper; the level is given back however the reading ends.
    fn nest<T>(&mut self, read: impl FnOnce(&mut Self) -> Res<T>) -> Res<T> {
        let before = self.depth;
        let found = self.deeper().and_then(|()| read(self));
        self.depth = before;
        found
    }

    /// Reads what nests the tree by its own count (`deeper`), and gives the level back.
    fn keep<T>(&mut self, read: impl FnOnce(&mut Self) -> Res<T>) -> Res<T> {
        let before = self.depth;
        let found = read(self);
        self.depth = before;
        found
    }

    /// One level deeper, at most `MAX_NESTING`: the checker, the interpreter and the back end
    /// walk the tree by recursion, and a tree without a bound would overflow their stack.
    fn deeper(&mut self) -> Res<()> {
        self.depth += 1;
        match self.depth > MAX_NESTING {
            true => Err(Diag::error(self.span(), format!("this is nested more than {MAX_NESTING} deep"))
                .hint("give a part a name of its own (`var part = ...`) or a function of its own")),
            false => Ok(()),
        }
    }

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
        let span = self.span();
        // Words of other languages between two conditions, where a value has just ended.
        let joins = self.after_value() && !wanted.contains("name");
        match self.peek() {
            Tok::Or if joins => {
                return diag
                    .note("`or` joins patterns, after `is` and in an arm of a `match`; two conditions are joined with `||`")
                    .hint(format!("`{}`", self.line_with(span.start, span.end, "||")));
            }
            Tok::Ident(word) if joins && word == "and" => {
                return diag
                    .note("two conditions are joined with `&&`")
                    .hint(format!("`{}`", self.line_with(span.start, span.end, "&&")));
            }
            // `readonly int limit = 3` in a function.
            Tok::Readonly if wanted == "an expression" => {
                return diag
                    .note("`readonly` is written on a field of a struct; a value that does not change is a `const`, or a variable that is not assigned again")
                    .hint(format!("`{}`", self.line_with(span.start, span.end, "const")));
            }
            Tok::Ident(word) if joins && word == "when" => {
                return diag
                    .note("`when` is the condition of an arm of a `match`; after `is`, a condition is joined with `&&`")
                    .hint(format!("`{}`", self.line_with(span.start, span.end, "&&")));
            }
            _ => {}
        }
        // `if not ready`: `not` is a word only after `is`, so here it was read as a name.
        if self.i >= 1 && matches!(&self.toks[self.i - 1].tok, Tok::Ident(word) if word == "not")
            && (self.i < 2 || self.toks[self.i - 2].tok != Tok::Is)
            && self.starts_value()
        {
            let not = self.toks[self.i - 1].span;
            let found = &self.src[span.start..span.end];
            if wanted == "`=>`" {
                return diag
                    .note("`not` is written only right after `is`, as in `is not null`; a pattern of a `match` has no `not`")
                    .hint(format!("an arm for `{found}` first, then one for the rest: `{found} => ...` and `_ => ...`"));
            }
            return diag
                .note("`not` is written only right after `is`, as in `is not null`; a condition is turned around with `!`")
                .hint(format!("`{}`", self.line_with(not.start, span.start, "!")));
        }
        match self.peek() {
            // An interface of one's own was put off with generics (12 November), in the language.
            Tok::Ident(name) if name == "interface" => diag.note(
                "an `interface` of your own is not in the language yet: it was put off with generics; the built-in ones are written after the name of a struct or an enum: `struct Label : Formattable`",
            ),
            Tok::Ident(name) if NOT_YET.contains(&name.as_str()) => {
                diag.note(format!("`{name}` is not in the interpreter yet"))
            }
            Tok::Ident(name) if WORDS.contains(&name.as_str()) => {
                diag.note(format!("`{name}` is a word of the language, not a name"))
            }
            // `var r = 0..5`, `print(0..5)`: a range is not a value.
            Tok::DotDot => diag.note("a range is not a value: it is written at the head of a `for` (`for i in 0..5`) or between the brackets of an index (`xs[1..3]`)"),
            // `void read(...)`: a word that has a token of its own, where a name was wanted.
            word if word.is_word() && wanted.ends_with("name") => {
                diag.note(format!("{} is a word of the language, not a name", word.describe()))
            }
            _ => diag,
        }
    }

    /// No expression starts at the next token. `x++` and `++x` are written `x += 1`, and a
    /// fractional number has digits before its point: `.5` is `0.5`.
    fn no_expression(&self) -> Diag {
        let diag = self.unexpected("an expression");
        let span = self.span();
        let bytes = self.src.as_bytes();
        let name_at = |range: std::ops::Range<usize>| {
            let text = &self.src[range];
            Some(text).filter(|text| !text.is_empty() && text.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.'))
        };
        // `x++` stops at its second `+`, `x--` at what follows it (the second `-` is a sign).
        let before = match self.peek() {
            Tok::Plus if self.src[..span.start].ends_with('+') => &self.src[..span.start + 1],
            _ => self.src[..span.start].trim_end(),
        };
        if let Some(sign) = ["++", "--"].into_iter().find(|sign| before.ends_with(sign)) {
            let end = before.len() - 2;
            let start = self.src[..end].rfind(['\n', ' ', '\t', '(', '{', ';']).map_or(0, |at| at + 1);
            if let Some(name) = name_at(start..end) {
                return diag.hint(format!("there is no `{sign}`: `{name} {}= 1`", &sign[..1]));
            }
        }
        match self.peek() {
            Tok::Plus | Tok::Minus => {
                let sign = bytes[span.start];
                // `++x`
                if bytes.get(span.end) == Some(&sign) {
                    let end = self.src[span.end + 1..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.')).map_or(self.src.len(), |at| span.end + 1 + at);
                    if let Some(name) = name_at(span.end + 1..end) {
                        return diag.hint(format!("there is no `{0}{0}`: `{name} {0}= 1`", sign as char));
                    }
                }
                diag
            }
            Tok::Dot if bytes.get(span.end).is_some_and(u8::is_ascii_digit) => {
                let end = self.src[span.end..].find(|c: char| !(c.is_ascii_digit() || c == '_')).map_or(self.src.len(), |at| span.end + at);
                diag.hint(format!("a fractional number has a digit before its point: `0.{}`", &self.src[span.end..end]))
            }
            _ => diag,
        }
    }

    /// Whether the token before the next one ends a value: a name, a literal, `)` or `]`.
    fn after_value(&self) -> bool {
        self.i >= 1
            && match &self.toks[self.i - 1].tok {
                Tok::Ident(name) => !WORDS.contains(&name.as_str()),
                Tok::Int(_) | Tok::Float(_) | Tok::Str(_) | Tok::Interp(_) | Tok::Char(_) => true,
                Tok::True | Tok::False | Tok::Null | Tok::RParen | Tok::RBracket => true,
                _ => false,
            }
    }

    /// Whether the next token can start a value.
    fn starts_value(&self) -> bool {
        matches!(
            self.peek(),
            Tok::Ident(_) | Tok::Int(_) | Tok::Float(_) | Tok::Str(_) | Tok::Interp(_) | Tok::Char(_) | Tok::True | Tok::False | Tok::LParen | Tok::Bang
        )
    }

    /// The line the next token is on, without its indent and a `{` at its end, with the text
    /// from `from` to `to` written `with` instead: what a hint shows of the user's own line.
    fn line_with(&self, from: usize, to: usize, with: &str) -> String {
        let start = self.src[..from].rfind('\n').map_or(0, |at| at + 1);
        let end = self.src[to..].find('\n').map_or(self.src.len(), |at| to + at);
        let before = self.src[start..from].trim_start();
        let after = self.src[to..end].trim_end();
        let after = after.strip_suffix('{').unwrap_or(after).trim_end();
        let (before, after) = match with {
            "!" => (before.to_string(), after.trim_start().to_string()),
            _ => {
                let (before, after) = (before.trim_end(), after.trim_start());
                let before = if before.is_empty() { String::new() } else { format!("{before} ") };
                let after = if after.is_empty() { String::new() } else { format!(" {after}") };
                (before, after)
            }
        };
        format!("{before}{with}{after}")
    }

    /// Whether the next token is the word `word` (one of `WORDS`).
    fn at_word(&self, word: &str) -> bool {
        matches!(self.peek(), Tok::Ident(name) if name == word)
    }

    /// Whether a type alias starts here: `alias Name = T`. `alias` is a word only at the top of
    /// a file, in front of a name that no `(` follows; elsewhere it is a name like any other, and
    /// `alias f() { }` is a function that gives an `alias`.
    fn at_alias(&self) -> bool {
        self.at_word("alias") && matches!(self.peek_at(1), Tok::Ident(_)) && *self.peek_at(2) != Tok::LParen
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
        self.nest(|parser| parser.type_expr_in())
    }

    fn type_expr_in(&mut self) -> Res<TypeExpr> {
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
        // `raw byte`, `raw raw byte`: an address of what follows. `atomic` may come before the
        // `raw`s (`atomic raw Node`, an atomic pointer) or after them (`raw atomic int`).
        let mut raw = 0u8;
        let mut atomic = None;
        loop {
            if self.at_word("atomic") {
                if atomic.is_some() {
                    return Err(Diag::error(self.span(), "`atomic` is written once"));
                }
                atomic = Some(raw);
                self.bump();
                continue;
            }
            if !self.at_word("raw") {
                break;
            }
            self.bump();
            raw = raw.saturating_add(1);
            if self.at_word("shared") || self.at_word("weak") || self.at_word("checked") || self.at_word("locked") {
                return Err(Diag::error(self.span(), "a raw pointer points at plain data, which has one owner and no count")
                    .note("`raw` is written right before the type it points at: `raw byte`, `shared raw byte`"));
            }
        }
        let (mut name, _) = self.ident("a type name")?;
        // `billing.Line`: the module is written in front.
        while *self.peek() == Tok::Dot && matches!(self.peek_at(1), Tok::Ident(_)) {
            self.bump();
            name = format!("{name}.{}", self.ident("a type name")?.0);
        }
        let name = self.map.full_name(&name);
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
                // `int?? count`: the lexer reads `??` as one sign.
                Tok::QuestionQuestion => {
                    let written = format!("{}?", &self.src[start.start..self.prev_end()]);
                    return Err(Diag::error(self.span(), "`??` is not a type: a value is either there or `null`")
                        .hint(format!("one `?`: `{written}`")));
                }
                // `int[8]` is an array, `byte[]` a slice.
                Tok::LBracket => {
                    self.bump();
                    let start_of_count = self.i;
                    match *self.peek() {
                        Tok::RBracket => suffixes.push(Suffix::Slice),
                        Tok::Int(length) => {
                            let Ok(length) = usize::try_from(length) else {
                                return Err(Diag::error(self.span(), "an array cannot have that many items"));
                            };
                            suffixes.push(Suffix::Array(length));
                            self.bump();
                            if *self.peek() != Tok::RBracket {
                                return Err(self.count_not_written(start_of_count));
                            }
                        }
                        // `byte[BlockSize]`: the count an integer const holds. A const of a block
                        // is named by its full name, which only the parser knows.
                        Tok::Ident(ref written) if !WORDS.contains(&written.as_str()) => {
                            let mut name = written.clone();
                            let at = self.bump().span;
                            while *self.peek() == Tok::Dot && matches!(self.peek_at(1), Tok::Ident(_)) {
                                self.bump();
                                name = format!("{name}.{}", self.ident("the name of a const")?.0);
                            }
                            let full = self.local_const(&name).unwrap_or_else(|| self.map.full_name(&name));
                            suffixes.push(Suffix::Named(full, Span::new(at.start, self.prev_end())));
                            if *self.peek() != Tok::RBracket {
                                return Err(self.count_not_written(start_of_count));
                            }
                        }
                        _ => {
                            return Err(self
                                .unexpected("the number of items or `]`")
                                .note("the length of an array is a number or an integer `const` written in its type: `int[8]`, `byte[BlockSize]`; a slice is written `read int[]`"))
                        }
                    }
                }
                _ => break,
            }
            self.bump();
        }
        Ok(TypeExpr { mode, holder, checked, locked, atomic, raw, name, args, suffixes, span: Span::new(start.start, self.prev_end()) })
    }

    /// The full name of a const of a block around here, written `name`.
    fn local_const(&self, name: &str) -> Option<String> {
        self.visible.iter().rev().flat_map(|scope| scope.iter().rev()).find(|(written, _)| written == name).map(|(_, full)| full.clone())
    }

    /// `const int BlockSize = 4096`, `const int[] Primes = { 2, 3, 5, 7 }`: the word `const` is
    /// next. `{ ... }` gives the items of an array, and a type written `T[]` takes their count.
    fn const_decl(&mut self) -> Res<ConstDecl> {
        let start = self.span();
        self.bump();
        if matches!(self.peek(), Tok::Ident(word) if word == "assert") && *self.peek_at(1) == Tok::LParen {
            return self.const_assert(start);
        }
        if *self.peek() == Tok::Var {
            return Err(Diag::error(self.span(), "a const has its type written: what it holds is part of what it says")
                .hint("`const int Size = 64`"));
        }
        let mut ty = self.type_expr()?;
        let (name, name_span) = match self.peek() {
            Tok::Assign => {
                return Err(Diag::error(self.span(), "a const has a type and a name: `const int Size = 64`"));
            }
            _ => self.ident("the name of the const")?,
        };
        self.expect(Tok::Assign, "`=`").map_err(|d| d.hint(format!("a const is given its value where it is declared: `const {} {name} = ...`", &self.src[ty.span.start..ty.span.end])))?;
        let visible: Vec<(String, String)> = self.visible.iter().flatten().cloned().collect();
        self.in_const = true;
        let init = self.const_value(&mut ty, &name);
        self.in_const = false;
        let init = init?;
        Ok(ConstDecl { public: false, name, name_span, ty, init, local: None, visible, assert: None })
    }

    /// `const assert(cond)` or `const assert(cond, "message")`, after `const`: a `bool` const
    /// named `assert#` and where it is written, which no other name has.
    fn const_assert(&mut self, start: Span) -> Res<ConstDecl> {
        let (_, name_span) = self.ident("`assert`")?;
        self.bump();
        let visible: Vec<(String, String)> = self.visible.iter().flatten().cloned().collect();
        self.in_const = true;
        let cond = self.expr();
        self.in_const = false;
        let init = cond?;
        let message = match self.eat(&Tok::Comma) {
            false => None,
            true => match self.peek().clone() {
                Tok::Str(text) => {
                    self.bump();
                    Some(text)
                }
                Tok::Interp(_) => {
                    return Err(Diag::error(self.span(), "the message of a `const assert` is a text written out, with nothing put into it")
                        .note("it is said when the program is compiled; the values of a comparison are shown beside it"))
                }
                _ => return Err(self.unexpected("the message, written out as a text")),
            },
        };
        self.expect(Tok::RParen, "`)`")?;
        let span = Span::new(start.start, self.prev_end());
        let ty = TypeExpr { mode: None, holder: None, checked: false, locked: false, atomic: None, raw: 0, name: "bool".into(), args: Vec::new(), suffixes: Vec::new(), span: name_span };
        let name = format!("assert#{}", start.start);
        Ok(ConstDecl { public: false, name, name_span, ty, init, local: None, visible, assert: Some(ConstAssert { message, span }) })
    }

    /// `static Table Crc = makeCrcTable()`, `static atomic long Hits = atomic 0`, `static locked
    /// Config Settings = Config.defaults()`: the word `static` is next. Gives the static and the
    /// function that gives its value, named after it (`static_init`), whose body returns what
    /// is written after `=`.
    fn static_decl(&mut self) -> Res<(StaticDecl, Func)> {
        self.bump();
        if *self.peek() == Tok::Var {
            return Err(Diag::error(self.span(), "a static has its type written").hint("`static int Seed = seed()`"));
        }
        let locked = self.at_word("locked");
        if locked {
            self.bump();
        }
        let ty = self.type_expr()?;
        if let Some(mode) = ty.mode {
            let word = if mode == Mode::Read { "read" } else { "ref" };
            return Err(Diag::error(ty.span, format!("a static owns its value, and `{word}` makes it a borrow"))
                .note("a static lives until the program ends, longer than anything it could borrow from"));
        }
        if locked && (ty.holder.is_some() || ty.atomic.is_some()) {
            return Err(Diag::error(ty.span, "`static locked` holds the value itself, which is reached inside a `lock` block")
                .hint("`static locked Config Settings = Config.defaults()`"));
        }
        let (name, name_span) = match self.peek() {
            Tok::Assign => return Err(Diag::error(self.span(), "a static has a type and a name: `static int Seed = seed()`")),
            _ => self.ident("the name of the static")?,
        };
        let written = &self.src[ty.span.start..ty.span.end];
        let shown = if locked { format!("locked {written}") } else { written.to_string() };
        self.expect(Tok::Assign, "`=`").map_err(|d| d.hint(format!("a static is given its value where it is declared: `static {shown} {name} = ...`")))?;
        let init = self.expr()?;
        let consts: Vec<(String, String)> = self.visible.iter().flatten().cloned().collect();
        let statics: Vec<(String, String)> = self.visible_statics.iter().flatten().cloned().collect();
        let span = init.span;
        let body = vec![Stmt { kind: StmtKind::Return(Some(init)), span }];
        let func = Func { public: false, is_async: false, export: false, ret: ty.clone(), name: static_init(&name), name_span, this: None, this_ty: None, params: Vec::new(), from: Vec::new(), body, foreign: None, variadic: false };
        let decl = StaticDecl { public: false, init: func.name.clone(), name, name_span, locked, ty, local: None, consts, statics, value: None };
        Ok((decl, func))
    }

    /// What a const is given: an expression, or `{ ... }` for the items of an array.
    fn const_value(&mut self, ty: &mut TypeExpr, name: &str) -> Res<Expr> {
        Ok(match *self.peek() == Tok::LBrace {
            false => self.expr()?,
            true => {
                let start = self.span();
                let mut values = Vec::new();
                for mut item in self.items()? {
                    if item.braced {
                        return Err(Diag::error(item.span, "an item of the array is one value, written without braces")
                            .note("an array of arrays is not in the language yet"));
                    }
                    values.push(item.values.remove(0));
                }
                let span = Span::new(start.start, self.prev_end());
                match ty.suffixes.last() {
                    Some(Suffix::Slice) if values.is_empty() => {
                        return Err(Diag::error(span, "an array has at least one item"));
                    }
                    Some(Suffix::Slice) => {
                        *ty.suffixes.last_mut().expect("just matched") = Suffix::Array(values.len());
                    }
                    Some(Suffix::Array(_) | Suffix::Named(..)) => {}
                    _ => {
                        return Err(Diag::error(span, "`{ ... }` gives the items of an array, and this const is not one")
                            .hint(format!("`const {}[] {name} = {{ ... }}`", &self.src[ty.span.start..ty.span.end])));
                    }
                }
                Expr { kind: ExprKind::Array { ty: ty.clone(), args: Vec::new(), items: Some(values) }, span, ty: Ty::Void, moved: false }
            }
        })
    }

    /// `alias NodeRef = shared Node`
    fn alias_decl(&mut self, public: bool) -> Res<AliasDecl> {
        self.bump();
        let (name, name_span) = self.ident("the new name of the type")?;
        if *self.peek() == Tok::Lt {
            return Err(Diag::error(self.span(), "an alias with type parameters is not in the language yet")
                .note("generics of your own are deferred: `alias Link<T> = ...` comes with them"));
        }
        self.expect(Tok::Assign, "`=`").map_err(|d| d.note("an alias is written `alias NodeRef = shared Node`"))?;
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
                let module = self.map.full(&module);
                if uses.iter().any(|(other, _)| *other == module) {
                    return Err(Diag::error(span, format!("`use {}` is written twice", self.map.written(&module))));
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
            let export_span = self.span();
            let export = self.at_word("export");
            if export {
                self.bump();
                // `export public int f()`: the words come in the other order (#45).
                if *self.peek() == Tok::Public {
                    return Err(Diag::error(self.span(), "`public` is written before `export`").hint("`public export int name(...) { ... }`"));
                }
                if matches!(self.peek(), Tok::Struct | Tok::Enum) || self.at_alias() || self.at_word("extern") || (matches!(self.peek(), Tok::Ident(word) if word == "test") && matches!(self.peek_at(1), Tok::Str(_))) {
                    return Err(Diag::error(export_span, "`export` is written in front of a function that C calls").hint("`export int compare(raw void a, raw void b) { ... }`"));
                }
                if self.at_word("async") {
                    return Err(Diag::error(export_span, "a function that C calls is not async: C cannot wait for a task").hint("start the work with `Task.run` inside it, or call it from SG"));
                }
            }
            let async_span = self.span();
            let is_async = self.at_word("async");
            if is_async {
                self.bump();
                if matches!(self.peek(), Tok::Struct | Tok::Enum) || self.at_alias() || (matches!(self.peek(), Tok::Ident(word) if word == "test") && matches!(self.peek_at(1), Tok::Str(_))) {
                    return Err(Diag::error(async_span, "`async` is written in front of a function").hint("`async int count(string path) { ... }`"));
                }
            }
            if self.at_word("extern") {
                if public {
                    return Err(Diag::error(public_span, "`public` is written on each function of an `extern` block, not on the block")
                        .hint("`extern \"z\" { public int compress(...) }`"));
                }
                if is_async {
                    return Err(Diag::error(async_span, "a C function is not async").hint("a long call is awaited as a task of its own: `await Task.run(work, x)`"));
                }
                self.extern_block(program)?;
                if *self.peek() != Tok::Eof {
                    self.expect(Tok::Newline, "end of line")?;
                }
                self.skip_newlines();
                continue;
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
                // The word an alias was written with before choice 1920.
                let start = self.span().start;
                let end = self.src[start..].find('\n').map_or(self.src.len(), |at| start + at);
                let line = self.src[start..end].split("//").next().unwrap_or("").trim_end();
                return Err(Diag::error(self.span(), "a type alias is written with `alias`")
                    .hint(format!("`{}alias{}`", if public { "public " } else { "" }, &line["using".len()..])));
            } else if self.at_alias() {
                let decl = self.alias_decl(public)?;
                program.aliases.push(decl);
                // A type can end in `>` or `*`, which at the end of a line would join it to
                // the next one: the line ends where the text says it does.
                if self.src[self.prev_end()..self.span().start].contains('\n') {
                    self.skip_newlines();
                    continue;
                }
            } else if self.at_word("align") && *self.peek_at(1) == Tok::LParen && !self.at_layout() {
                return Err(self.misplaced_align());
            } else if *self.peek() == Tok::Struct || self.at_layout() {
                let mut decl = self.struct_decl()?;
                decl.public = public;
                program.structs.push(decl);
            } else if *self.peek() == Tok::Enum {
                let mut decl = self.enum_decl()?;
                decl.public = public;
                program.enums.push(decl);
            } else if self.at_word("const") {
                if export || is_async {
                    let word = if export { export_span } else { async_span };
                    return Err(Diag::error(word, format!("`{}` is written in front of a function", self.src[word.start..word.end].trim())));
                }
                let mut decl = self.const_decl()?;
                decl.public = public;
                program.consts.push(decl);
            } else if self.at_word("static") {
                if export || is_async {
                    let word = if export { export_span } else { async_span };
                    return Err(Diag::error(word, format!("`{}` is written in front of a function", self.src[word.start..word.end].trim())));
                }
                let (mut decl, init) = self.static_decl()?;
                decl.public = public;
                program.statics.push(decl);
                program.funcs.push(init);
            } else {
                // A type that was read and refused (`int?? find()`) keeps its own hint.
                let started = matches!(self.peek(), Tok::Ident(_)) && matches!(self.peek_at(1), Tok::QuestionQuestion);
                let ret = self.type_expr().map_err(|d| match started {
                    true => d,
                    false => d.hint("only `use` lines, structs, enums, aliases, consts, statics, functions, `extern` blocks and tests are written at the top level: `void main() { ... }`"),
                })?;
                let (name, name_span) = self.ident("a function name")?;
                let mut func = self.func(ret, name, name_span, false)?;
                func.public = public;
                func.is_async = is_async;
                func.export = export;
                program.funcs.push(func);
            }
            if *self.peek() != Tok::Eof {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        Ok(uses)
    }

    /// `extern "z" { int compress(raw byte dest, ref nuint destLength, ...) }`: functions of C,
    /// each declared on a line of its own, with no body. `extern { }` is the C library itself.
    fn extern_block(&mut self, program: &mut Program) -> Res<()> {
        let start = self.bump().span;
        // `extern "z", "zlib"`: the names a library has on the systems, the first found linked.
        let mut names = Vec::new();
        if let Tok::Str(name) = self.peek().clone() {
            self.bump();
            names.push(name);
            while self.eat(&Tok::Comma) {
                match self.peek().clone() {
                    Tok::Str(name) => {
                        self.bump();
                        names.push(name);
                    }
                    _ => return Err(self.unexpected("the name of a library")),
                }
            }
        }
        let span = Span::new(start.start, self.prev_end());
        // `./native.c`: a C file of the module's folder, in place of a library.
        if let Some(written) = names.iter().find(|written| written.starts_with("./") || written.ends_with(".c")) {
            if names.len() > 1 {
                return Err(Diag::error(span, format!("`{written}` is a C file, and an `extern` block takes one C file or the names of one library"))
                    .hint("a C file and a library are two `extern` blocks"));
            }
        }
        let (libs, file) = match names.pop() {
            Some(written) if names.is_empty() && written.starts_with("./") => (Vec::new(), Some(c_file_path(&written, span)?)),
            // A file above the module's folder is not one of its C files (#45).
            Some(written) if names.is_empty() && written.ends_with(".c") && written.split('/').any(|part| part == "..") => {
                return Err(Diag::error(span, format!("`{written}` is not the name of a library, nor a C file of this module"))
                    .note("a C file beside the program lies in the module's folder or below it, without `..`")
                    .hint("move the file into the folder and write it from there: `extern \"./native.c\" { ... }`"))
            }
            Some(written) if names.is_empty() && written.ends_with(".c") => {
                return Err(Diag::error(span, format!("`{written}` is not the name of a library, nor a C file of this module"))
                    .hint(format!("a C file beside the program is written from the module's folder: `extern \"./{}\"`", written.trim_start_matches('/'))))
            }
            Some(last) => {
                names.push(last);
                (names, None)
            }
            None => (Vec::new(), None),
        };
        let foreign = Foreign { libs, file, path: None, span };
        program.blocks.push((String::new(), foreign.clone()));
        self.open_brace()?;
        self.skip_newlines();
        while *self.peek() != Tok::RBrace {
            if *self.peek() == Tok::Eof {
                return Err(self.unexpected("`}`"));
            }
            let public = self.eat(&Tok::Public);
            // `struct sqlite3`: a type of C seen only behind a raw pointer.
            if *self.peek() == Tok::Struct {
                self.bump();
                let (name, name_span) = self.ident("the name of a type of C")?;
                if matches!(self.peek(), Tok::Colon | Tok::Lt) {
                    return Err(Diag::error(self.span(), "a type of C declared in an `extern` block is only a name: SG does not see what it holds")
                        .note("a struct that SG lays out as C does is written outside the `extern` block"));
                }
                if *self.peek() == Tok::LBrace {
                    return Err(Diag::error(self.span(), "a type of C declared in an `extern` block has no body: SG does not see what it holds")
                        .hint(format!("a struct whose fields SG reads is written outside the `extern` block: `struct {name} {{ ... }}`")));
                }
                let decl = StructDecl { public, name, name_span, sources: Vec::new(), interfaces: Vec::new(), fields: Vec::new(), methods: Vec::new(), opaque: true, layout: LayoutDecl::default() };
                program.structs.push(decl);
                if *self.peek() != Tok::RBrace {
                    self.expect(Tok::Newline, "end of line")?;
                }
                self.skip_newlines();
                continue;
            }
            if self.at_word("async") {
                return Err(Diag::error(self.span(), "a C function is not async").hint("a long call is awaited as a task of its own: `await Task.run(work, x)`"));
            }
            let ret = self.type_expr()?;
            let (name, name_span) = self.ident("the name of a C function")?;
            // `int opterr`: a variable of C.
            if matches!(self.peek(), Tok::Newline | Tok::RBrace) {
                program.cvars.push(CVarDecl { public, name, name_span, ty: ret, foreign: foreign.clone() });
                if *self.peek() != Tok::RBrace {
                    self.expect(Tok::Newline, "end of line")?;
                }
                self.skip_newlines();
                continue;
            }
            if *self.peek() == Tok::Assign {
                return Err(Diag::error(self.span(), "a variable of C has no value here: C gives it one")
                    .hint(format!("`{name}` is given a value inside `unsafe`: `unsafe {{ {name} = ... }}`")));
            }
            self.expect(Tok::LParen, "`(`")?;
            let mut params: Vec<Param> = Vec::new();
            // `...` after the parameters: each call gives more arguments, of their own types.
            let mut variadic = false;
            while *self.peek() != Tok::RParen {
                if let Some(span) = self.ellipsis() {
                    if params.is_empty() {
                        return Err(Diag::error(span, "a C function takes at least one parameter before `...`, as C asks")
                            .note("C reads what follows from where the last named parameter ends"));
                    }
                    if *self.peek() != Tok::RParen {
                        return Err(Diag::error(span, "`...` is written last: what C takes after it has no names"));
                    }
                    variadic = true;
                    break;
                }
                let ty = self.type_expr()?;
                if matches!(self.peek(), Tok::Comma | Tok::RParen) {
                    return Err(Diag::error(ty.span, "a parameter of a C function has a name too, as in any function")
                        .hint(format!("`{} name`", &self.src[ty.span.start..ty.span.end])));
                }
                let (name, span) = self.ident("a parameter name")?;
                if *self.peek() == Tok::Assign {
                    return Err(Diag::error(self.span(), "a parameter of a C function has no default value: C is given every argument")
                        .hint("a function of your own that calls it can have one"));
                }
                params.push(Param { ty, name, span, default: None });
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(Tok::RParen, "`)`")?;
            if *self.peek() == Tok::LBrace {
                return Err(Diag::error(self.span(), "a C function has no body: it is declared here, and C has it")
                    .hint("a function of your own is written outside the `extern` block"));
            }
            let func = Func { public, is_async: false, export: false, ret, name, name_span, this: None, this_ty: None, params, from: Vec::new(), body: Vec::new(), foreign: Some(foreign.clone()), variadic };
            program.funcs.push(func);
            if *self.peek() != Tok::RBrace {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        self.bump();
        Ok(())
    }

    /// Whether a struct or a union laid out as C lays it out begins here: `union Bits {`,
    /// `packed struct Header {`, `align(64) struct Slot {`. The words are names elsewhere.
    fn at_layout(&self) -> bool {
        let union_at = |n: usize| matches!(self.peek_at(n), Tok::Ident(word) if word == "union") && matches!(self.peek_at(n + 1), Tok::Ident(_));
        let mut at = 0;
        let mut marked = false;
        loop {
            match self.peek_at(at) {
                Tok::Ident(word) if word == "packed" => at += 1,
                Tok::Ident(word) if word == "align" && *self.peek_at(at + 1) == Tok::LParen && matches!(self.peek_at(at + 2), Tok::Int(_)) && *self.peek_at(at + 3) == Tok::RParen => at += 4,
                _ => break,
            }
            marked = true;
        }
        union_at(at) || (marked && *self.peek_at(at) == Tok::Struct)
    }

    /// `align(` at the top level where no struct or union laid out by it follows (#45): the
    /// error says what goes where.
    fn misplaced_align(&self) -> Diag {
        let start = self.span();
        let Tok::Int(value) = *self.peek_at(2) else {
            return Diag::error(start, "`align` takes a number of bytes, written as it is").hint("`align(64) struct Slot { ... }`");
        };
        if *self.peek_at(3) != Tok::RParen {
            return Diag::error(start, "`align` takes a number of bytes, written as it is").hint("`align(64) struct Slot { ... }`");
        }
        let after = self.peek_at(4);
        let span = self.toks.get(self.i + 4).map_or(start, |token| token.span);
        match after {
            Tok::Public => Diag::error(span, "`public` is written before `align`").hint(format!("`public align({value}) struct ...`")),
            Tok::Enum => Diag::error(span, "`align` is written in front of a struct or a union, and an enum is laid out by the compiler")
                .hint(format!("an enum aligned for C is held by a struct: `align({value}) struct ... {{ ... }}`")),
            _ => Diag::error(start, "`align` is written in front of a struct or a union").hint(format!("`align({value}) struct Slot {{ ... }}`")),
        }
    }

    /// `struct Name { ... }`, `union Name { ... }`, with `packed` or `align(N)` in front.
    fn struct_decl(&mut self) -> Res<StructDecl> {
        let mut layout = LayoutDecl::default();
        loop {
            if self.at_word("packed") {
                let span = self.bump().span;
                if layout.packed.is_some() {
                    return Err(Diag::error(span, "`packed` is written once"));
                }
                layout.packed = Some(span);
            } else if self.at_word("align") && *self.peek_at(1) == Tok::LParen {
                let start = self.bump().span.start;
                self.bump();
                let Tok::Int(value) = *self.peek() else { return Err(self.unexpected("a number of bytes")) };
                self.bump();
                self.expect(Tok::RParen, "`)`")?;
                let span = Span::new(start, self.prev_end());
                if layout.align.is_some() {
                    return Err(Diag::error(span, "`align` is written once"));
                }
                layout.align = Some((value, span));
            } else {
                break;
            }
        }
        if let (Some(packed), Some((_, align))) = (layout.packed, layout.align) {
            return Err(Diag::error(Span::new(packed.start.min(align.start), packed.end.max(align.end)), "a type is `packed` or `align(N)`, not both")
                .note("`packed` takes the padding out and makes the alignment 1; `align` makes it larger"));
        }
        if self.at_word("union") {
            layout.union = true;
            self.bump();
        } else if *self.peek() == Tok::Struct {
            self.bump();
        } else {
            return Err(self.unexpected("`struct` or `union`"));
        }
        let what = if layout.union { "a union name" } else { "a struct name" };
        let (name, name_span) = self.ident(what)?;
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
            let async_span = self.span();
            let is_async = self.eat_async()?;
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
                method.is_async = is_async;
                methods.push(method);
            } else {
                if is_async {
                    return Err(Diag::error(async_span, "`async` is written in front of a function, not of a field"));
                }
                // `uint version : 4`: a field of that many bits.
                let bits = if *self.peek() == Tok::Colon {
                    let start = self.bump().span.start;
                    let Tok::Int(width) = *self.peek() else {
                        return Err(self.unexpected("a number of bits").hint(format!("`{} {member} : 4`", &self.src[ty.span.start..ty.span.end])));
                    };
                    self.bump();
                    Some((width, Span::new(start, self.prev_end())))
                } else {
                    None
                };
                if member == "_" {
                    // `uint _ : 4`: bits left empty, which have no name to reach them by.
                    let Some((width, _)) = bits else {
                        return Err(Diag::error(span, "a field without a name is a number of bits left empty").hint(format!("`{} _ : 4`", &self.src[ty.span.start..ty.span.end])));
                    };
                    if public || readonly {
                        return Err(Diag::error(span, "bits left empty have no name, so they are neither `public` nor `readonly`"));
                    }
                    layout.gaps.push(Gap { before: fields.len(), ty, bits: width, span: Span::new(span.start, self.prev_end()) });
                } else {
                    let default = if *self.peek() == Tok::Assign { Some(self.field_default(&name, &member, span, &ty, layout.union)?) } else { None };
                    fields.push(FieldDecl { public, readonly, ty, name: member, span, bits, default });
                }
            }
            if *self.peek() != Tok::RBrace {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        self.bump();
        Ok(StructDecl { public: false, name, name_span, sources, interfaces, fields, methods, opaque: false, layout })
    }

    /// `int port = 80` in a struct: the default value of the field. `null` and an empty
    /// collection are put into a literal as written; anything else is a value known when the
    /// program is compiled, held by a const of its own, of the type the field has without its
    /// `?`.
    fn field_default(&mut self, owner: &str, field: &str, span: Span, ty: &TypeExpr, union: bool) -> Res<FieldDefault> {
        let assign = self.bump().span;
        if union {
            return Err(Diag::error(assign, "a field of a union has no default value")
                .note("a union is built with the one field it holds"));
        }
        if ty.mode.is_some() {
            return Err(Diag::error(assign, "a field that borrows has no default value")
                .note("what it borrows is given when the value is built"));
        }
        let visible: Vec<(String, String)> = self.visible.iter().flatten().cloned().collect();
        self.in_const = true;
        let init = self.expr();
        self.in_const = false;
        let init = init?;
        match &init.kind {
            ExprKind::Null => return Ok(FieldDefault::Expr(init)),
            ExprKind::Collection { args, items: None, .. } if args.is_empty() => return Ok(FieldDefault::Expr(init)),
            // `Options {}`: made of the default values of its fields, each found already.
            ExprKind::StructLit { fields, .. } if fields.is_empty() => return Ok(FieldDefault::Expr(init)),
            _ => {}
        }
        let mut ty = ty.clone();
        if ty.suffixes.last() == Some(&Suffix::Nullable) {
            ty.suffixes.pop();
        }
        let full = format!("{field}#{}", span.start);
        self.consts_found.push(ConstDecl { public: false, name: full.clone(), name_span: span, ty, init, local: Some(format!("{owner}.{field}")), visible, assert: None });
        Ok(FieldDefault::Const(full))
    }

    /// `int port = 80` in the parameters of a function: what a call that leaves the argument
    /// out gives (choices 1620-1622). The rule of the default value of a field: `null` and an
    /// empty collection are put into the call as written; anything else is a value known when
    /// the program is compiled, held by a const of its own, of the type the parameter has
    /// without `read` and its `?`.
    fn param_default(&mut self, func: &str, param: &str, span: Span, ty: &TypeExpr) -> Res<FieldDefault> {
        let assign = self.bump().span;
        if ty.mode == Some(Mode::Ref) {
            return Err(Diag::error(assign, "a `ref` parameter has no default value: there would be nothing for the function to change")
                .note("a call that leaves an argument out is given a value, and a `ref` parameter is given a place"));
        }
        let visible: Vec<(String, String)> = self.visible.iter().flatten().cloned().collect();
        self.in_const = true;
        let init = self.expr();
        self.in_const = false;
        let init = init?;
        match &init.kind {
            ExprKind::Null => return Ok(FieldDefault::Expr(init)),
            ExprKind::Collection { args, items: None, .. } if args.is_empty() => return Ok(FieldDefault::Expr(init)),
            // `Options {}`: made of the default values of its fields, each found already.
            ExprKind::StructLit { fields, .. } if fields.is_empty() => return Ok(FieldDefault::Expr(init)),
            // The token that is never cancelled, which a const cannot hold.
            ExprKind::Field { recv, name, .. } if name == "None" && matches!(&recv.kind, ExprKind::Var(owner) if owner == "CancellationToken") => return Ok(FieldDefault::Expr(init)),
            _ => {}
        }
        let mut ty = ty.clone();
        ty.mode = None;
        if ty.suffixes.last() == Some(&Suffix::Nullable) {
            ty.suffixes.pop();
        }
        let full = format!("{param}#{}", span.start);
        self.consts_found.push(ConstDecl { public: false, name: full.clone(), name_span: span, ty, init, local: Some(format!("{func}({param})")), visible, assert: None });
        Ok(FieldDefault::Const(full))
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
        match self.eat(&Tok::Colon) {
            true => self.interface_list(),
            false => Ok(Vec::new()),
        }
    }

    /// The names after `:`, separated by commas.
    fn interface_list(&mut self) -> Res<Vec<(String, Span)>> {
        let mut list = Vec::new();
        loop {
            // `: unsafe Sendable`: a promise of the writer, kept with the name.
            if self.at_word("unsafe") {
                let start = self.bump().span;
                let (name, span) = self.ident("an interface name")?;
                list.push((format!("unsafe {name}"), start.to(span)));
            } else {
                list.push(self.ident("an interface name")?);
            }
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        Ok(list)
    }

    /// `enum Shape { Circle(int radius)` / `Empty }`: one variant per line.
    fn enum_decl(&mut self) -> Res<EnumDecl> {
        self.bump();
        let (name, name_span) = self.ident("an enum name")?;
        let sources = self.sources()?;
        // `enum Block : byte, Copyable`: the integer type of its numbers comes first.
        let mut numbers = None;
        if *self.peek() == Tok::Colon {
            if let Tok::Ident(word) = self.peek_at(1).clone() {
                if let Some(ty) = IntTy::from_name(&word) {
                    self.bump();
                    let span = self.bump().span;
                    if ty.is_c() {
                        return Err(Diag::error(span, format!("the numbers of an enum are of a type with one size on every system, and C's `{word}` has a size of its own on each"))
                            .hint("`int` or `long`"));
                    }
                    numbers = Some((ty, span));
                }
            }
        }
        let interfaces = match numbers {
            Some(_) if self.eat(&Tok::Comma) => self.interface_list()?,
            Some(_) => Vec::new(),
            None => self.interfaces()?,
        };
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
                && matches!(self.peek_at(1), Tok::LParen | Tok::Assign | Tok::Newline | Tok::RBrace | Tok::Comma | Tok::Eof);
            if !variant_line {
                let public = self.eat(&Tok::Public);
                let is_async = self.eat_async()?;
                let ty = self.type_expr()?;
                let (member, span) = self.ident("a method name")?;
                if *self.peek() != Tok::LParen {
                    return Err(Diag::error(span, "an enum holds variants and functions, not fields")
                        .hint(format!("a value every variant carries is written in each variant: `Name({} {member})`", &self.src[ty.span.start..ty.span.end])));
                }
                let mut method = self.func(ty, member, span, true)?;
                method.public = public;
                method.is_async = is_async;
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
                    fields.push(FieldDecl { public: true, readonly: false, ty, name: field, span, bits: None, default: None });
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
            // `Image = 0x2C`: a number, held by a const of the enum's integer type.
            let mut number = None;
            if *self.peek() == Tok::Assign {
                let assign = self.bump().span;
                let Some((ty, ty_span)) = numbers else {
                    return Err(Diag::error(assign, "a variant has a number only in an enum that names the integer type of its numbers")
                        .hint(format!("`enum {name} : int`")));
                };
                if !fields.is_empty() {
                    return Err(Diag::error(span, "a variant with a number carries nothing"));
                }
                let full = format!("{variant}#{}", span.start);
                let ty = TypeExpr { mode: None, holder: None, checked: false, locked: false, atomic: None, raw: 0, name: ty.name().to_string(), args: Vec::new(), suffixes: Vec::new(), span: ty_span };
                let visible: Vec<(String, String)> = self.visible.iter().flatten().cloned().collect();
                self.in_const = true;
                let init = self.expr();
                self.in_const = false;
                self.consts_found.push(ConstDecl { public: false, name: full.clone(), name_span: span, ty, init: init?, local: Some(format!("{name}.{variant}")), visible, assert: None });
                number = Some(full);
            }
            if numbers.is_some() && !fields.is_empty() {
                return Err(Diag::error(span, "the variants of an enum with numbers carry nothing")
                    .note("a number stands for one variant, and a variant that carries values is more than a number"));
            }
            variants.push(VariantDecl { name: variant, span, fields, number });
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
        Ok(EnumDecl { public: false, name, name_span, sources, interfaces, numbers, variants, methods })
    }

    /// The rest of a function after its return type and name. In a struct body the first
    /// parameter may be `this`, `read this` or `ref this`.
    fn func(&mut self, ret: TypeExpr, name: String, name_span: Span, in_struct: bool) -> Res<Func> {
        let name_text = name.clone();
        self.expect(Tok::LParen, "`(`")?;
        let mut this = None;
        let mut this_ty = None;
        let mut params: Vec<Param> = Vec::new();
        while *self.peek() != Tok::RParen {
            let start = self.span();
            if let Some(span) = self.ellipsis() {
                return Err(Diag::error(span, "a function of SG takes a fixed count of arguments: only a function of C, declared in an `extern` block, takes `...`")
                    .hint("several values of one type are given as a slice: `read int[] values`"));
            }
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
                    // `int answer(void)`, as C writes a function without parameters (#45).
                    if ty.name == "void" && params.is_empty() && ty.suffixes.is_empty() && ty.raw == 0 && ty.mode.is_none() && *self.peek() == Tok::RParen {
                        return Err(Diag::error(ty.span, "a function without parameters has nothing between its parentheses")
                            .hint(format!("`{name_text}()`")));
                    }
                    let (name, span) = self.ident("a parameter name")?;
                    let default = match *self.peek() == Tok::Assign {
                        true => Some(self.param_default(&name_text, &name, span, &ty)?),
                        false => None,
                    };
                    // A call leaves arguments out only at its end (choice 1620).
                    if default.is_none() {
                        if let Some(before) = params.iter().rev().find(|param| param.default.is_some()) {
                            return Err(Diag::error(span, format!("`{name}` comes after `{}`, which has a default value, so it needs one too", before.name))
                                .note("a call leaves arguments out only at its end, so the parameters with a default value come last"));
                        }
                    }
                    params.push(Param { ty, name, span, default });
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
        Ok(Func { public: false, is_async: false, export: false, ret, name, name_span, this, this_ty, params, from, body, foreign: None, variadic: false })
    }

    /// `...` as a parameter, taken with its span: `..` and `.` written together.
    fn ellipsis(&mut self) -> Option<Span> {
        if *self.peek() != Tok::DotDot || *self.peek_at(1) != Tok::Dot {
            return None;
        }
        let (first, second) = (self.toks[self.i].span, self.toks[self.i + 1].span);
        if first.end != second.start {
            return None;
        }
        self.bump();
        self.bump();
        Some(first.to(second))
    }

    /// `async` in front of a method, as in front of a function at the top level.
    fn eat_async(&mut self) -> Res<bool> {
        if self.at_word("export") {
            return Err(Diag::error(self.span(), "a method is not called by C: `export` is written on a function at the top level")
                .hint("C reaches a value through the pointer it is given: `export void step(raw void data) { var s = unsafe (ref State)data ... }`"));
        }
        if self.at_word("async") {
            self.bump();
            return Ok(true);
        }
        Ok(false)
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
        self.nest(|parser| parser.block_in())
    }

    fn block_in(&mut self) -> Res<Block> {
        self.open_brace()?;
        let outer = std::mem::replace(&mut self.struct_literals, true);
        self.visible.push(Vec::new());
        self.visible_statics.push(Vec::new());
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
        self.visible.pop();
        self.visible_statics.pop();
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
        while matches!(self.peek_at(first), Tok::Ident(word) if word == "raw" || word == "atomic") {
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
                // `int?? count = null` reads as a declaration, which the type then refuses.
                Tok::QuestionQuestion if matches!(self.peek_at(n + 1), Tok::Ident(_)) && *self.peek_at(n + 2) == Tok::Assign => n += 1,
                Tok::LBracket if *self.peek_at(n + 1) == Tok::RBracket => n += 2,
                Tok::LBracket if matches!(self.peek_at(n + 1), Tok::Int(_) | Tok::Ident(_)) && *self.peek_at(n + 2) == Tok::RBracket => n += 3,
                Tok::LBracket if self.type_word(first) => match self.count_len(n) {
                    Some(len) => n += len,
                    None => break,
                },
                _ => break,
            }
        }
        Some(n - start)
    }

    /// Whether the name `n` tokens ahead is one of a type, as far as the parser can tell: a
    /// type of the language written in small letters (`int`, `byte`), or a name that starts with
    /// a capital letter, as the names of types do and those of variables do not.
    fn type_word(&self, n: usize) -> bool {
        match self.peek_at(n) {
            Tok::Ident(word) => Ty::from_name(word).is_some() || word.starts_with(|c: char| c.is_ascii_uppercase()),
            _ => false,
        }
    }

    /// How many tokens `[ ... ]` takes, `n` tokens ahead, when what is inside is more than a
    /// number or a name on one line (`[Rows * Cols]`); `None` otherwise.
    fn count_len(&self, n: usize) -> Option<usize> {
        let mut depth = 0;
        let mut at = n + 1;
        loop {
            match self.peek_at(at) {
                Tok::LParen | Tok::LBracket => depth += 1,
                Tok::RParen => depth -= 1,
                Tok::RBracket if depth == 0 => break,
                Tok::RBracket => depth -= 1,
                Tok::Newline | Tok::Eof | Tok::LBrace | Tok::RBrace | Tok::Comma | Tok::Assign => return None,
                _ => {}
            }
            at += 1;
        }
        (at - n > 2).then_some(at - n + 1)
    }

    /// The count of an array written as more than a number or the name of a const, from token
    /// `start` (just after `[`) to the `]`.
    fn count_not_written(&self, start: usize) -> Diag {
        let mut end = self.i;
        while end + 1 < self.toks.len() && !matches!(self.toks[end].tok, Tok::RBracket | Tok::Newline | Tok::Eof) {
            end += 1;
        }
        let span = Span::new(self.toks[start].span.start, self.toks[end.max(start + 1) - 1].span.end);
        let written = &self.src[span.start..span.end];
        Diag::error(span, "the count of an array is a number or the name of a const, not a calculation")
            .hint(format!("give it a name first: `const int Count = {written}`, then `[Count]`"))
    }

    /// The type comes first: `int x = 1`, `Node*? list = null`, `Result<int, E> r = f()`. A
    /// type and then a name is a declaration.
    fn at_declaration(&self) -> bool {
        // `await task` is not a variable `task` of the type `await`, nor `unsafe p[0] = 1` one of
        // the type `unsafe`.
        !self.at_word("await") && !self.at_word("unsafe") && self.type_len(0).is_some_and(|n| matches!(self.peek_at(n), Tok::Ident(_)) && !self.at_cond(n))
    }

    /// Whether the `?` that ends what reads as a type of `n` tokens begins `c ? a : b`
    /// instead: `flag ? x : y`, `flag ? x.y : z`. A name that ends the line, or is followed by
    /// `=`, is declared (`int? x = 1`).
    fn at_cond(&self, n: usize) -> bool {
        *self.peek_at(n - 1) == Tok::Question && !matches!(self.peek_at(n + 1), Tok::Assign | Tok::Newline | Tok::Eof | Tok::RBrace)
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
                let init = self.declared_value(&name)?;
                StmtKind::Let { ty: None, name, name_span, init }
            }
            Tok::If => self.if_stmt()?,
            Tok::While => {
                self.bump();
                let cond = self.head_expr(false)?;
                let body = self.block()?;
                StmtKind::While { cond, body, after: Vec::new(), label: None }
            }
            Tok::For => {
                self.bump();
                let (var, var_span) = self.ident("a loop variable")?;
                self.expect(Tok::In, "`in`")?;
                if let Some((from, to)) = self.reversed_range()? {
                    let body = self.block()?;
                    StmtKind::For { var, var_span, from, to, body, reversed: true, label: None }
                } else {
                    let mark = self.i;
                    let mut from = self.head_expr(true)?;
                    // What goes over is read up to `..`, which binds more loosely than a
                    // comparison; anything else is read whole, as an `if` reads its condition
                    // (`for x in c ? a : b {`).
                    if matches!(self.peek(), Tok::Question | Tok::QuestionQuestion | Tok::AndAnd | Tok::OrOr | Tok::Eq | Tok::Ne | Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge | Tok::Is) {
                        self.i = mark;
                        from = self.head_expr(false)?;
                    }
                    if *self.peek() == Tok::DotDotEq {
                        return Err(Diag::error(self.span(), "`..=` is written only in a pattern; a `for` goes up to its end, without it")
                            .hint("`for i in a..b + 1`"));
                    }
                    if self.eat(&Tok::DotDot) {
                        let to = self.head_expr(true)?;
                        let body = self.block()?;
                        StmtKind::For { var, var_span, from, to, body, reversed: false, label: None }
                    } else {
                        let body = self.block()?;
                        StmtKind::ForEach { var, var_span, iter: from, body, by: None, label: None }
                    }
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
            // `read int[] part = read xs`: a variable takes a borrow from its value, not from a
            // word in its type (#37).
            Tok::Read | Tok::Ref if self.type_len(1).is_some_and(|n| matches!(self.peek_at(n + 1), Tok::Ident(_)) && *self.peek_at(n + 2) == Tok::Assign) => {
                let word = if self.bump().tok == Tok::Ref { "ref" } else { "read" };
                let ty = self.type_expr()?;
                let (name, _) = self.ident("a variable name")?;
                self.bump();
                let init = self.expr()?;
                let (ty_text, init_text) = (&self.src[ty.span.start..ty.span.end], &self.src[init.span.start..init.span.end]);
                let hint = match init_text.strip_prefix(word).is_some_and(|rest| rest.starts_with(' ')) {
                    true => format!("`var {name} = {init_text}` borrows"),
                    false => format!("`{ty_text} {name} = {init_text}` keeps a value of its own; `var {name} = {word} ...` borrows one"),
                };
                return Err(Diag::error(Span::new(start.start, ty.span.end), format!("a variable is not declared with `{word}` in its type; it borrows when its value is a borrow")).hint(hint));
            }
            // `lock total as c { ... }`, `lock a as x, b as y { ... }`, `lock q as x when c { ... }`
            Tok::Ident(word) if word == "lock" => self.lock_stmt(false)?,
            // `await lock q as x when c { ... }`, in an async function.
            Tok::Ident(word) if word == "await" && matches!(self.peek_at(1), Tok::Ident(next) if next == "lock") => {
                self.bump();
                self.lock_stmt(true)?
            }
            // `const int Size = 64`: seen from here to the end of the block.
            Tok::Ident(word) if word == "const" => {
                let mut decl = self.const_decl()?;
                if decl.assert.is_some() {
                    // No name to be seen: what it checks is checked with the consts.
                    let (name, name_span) = (decl.name.clone(), decl.name_span);
                    decl.local = Some(name.clone());
                    self.consts_found.push(decl);
                    return Ok(Ok(Stmt { kind: StmtKind::Const { name: name.clone(), name_span, full: name }, span: Span::new(start.start, self.prev_end()) }));
                }
                let written = std::mem::take(&mut decl.name);
                let full = format!("{written}#{}", decl.name_span.start);
                decl.name = full.clone();
                decl.local = Some(written.clone());
                let name_span = decl.name_span;
                self.consts_found.push(decl);
                self.visible.last_mut().expect("a statement is inside a block").push((written.clone(), full.clone()));
                StmtKind::Const { name: written, name_span, full }
            }
            // `static int Hits = ...`: one value for the whole program, its name seen from here to
            // the end of the block.
            Tok::Ident(word) if word == "static" => {
                let (mut decl, mut init) = self.static_decl()?;
                let written = std::mem::take(&mut decl.name);
                let full = format!("{written}#{}", decl.name_span.start);
                decl.name = full.clone();
                decl.init = static_init(&full);
                init.name = decl.init.clone();
                decl.local = Some(written.clone());
                let name_span = decl.name_span;
                self.statics_found.push((decl, init));
                self.visible_statics.last_mut().expect("a statement is inside a block").push((written.clone(), full.clone()));
                StmtKind::Static { name: written, name_span, full }
            }
            // `unsafe { ... }`; `unsafe` in front of a value is read as an expression.
            Tok::Ident(word) if word == "unsafe" && *self.peek_at(1) == Tok::LBrace => {
                self.bump();
                StmtKind::Unsafe(self.block()?)
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
                StmtKind::Break(self.loop_named()?)
            }
            Tok::Continue => {
                self.bump();
                StmtKind::Continue(self.loop_named()?)
            }
            // `outer: for ...`, `outer: while ...`: a loop with a name, which `break outer` and
            // `continue outer` leave or go on with from a loop inside it.
            Tok::Ident(_) if *self.peek_at(1) == Tok::Colon => {
                let (name, span) = self.ident("the name of a loop")?;
                self.bump();
                if !matches!(self.peek(), Tok::For | Tok::While) {
                    return Err(self.unexpected("`for` or `while`").note("a name followed by `:` names the loop after it: `outer: for i in 0..n { ... }`"));
                }
                let mut stmt = self.stmt()?;
                let label = Some(Label { name, span });
                match &mut stmt.kind {
                    StmtKind::While { label: named, .. } | StmtKind::For { label: named, .. } | StmtKind::ForEach { label: named, .. } => *named = label,
                    _ => unreachable!("a `for` or a `while` was read"),
                }
                stmt.span = start.to(stmt.span);
                return Ok(Ok(stmt));
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
            | Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge | Tok::QuestionQuestion | Tok::Question | Tok::Colon | Tok::Amp | Tok::Pipe | Tok::Caret | Tok::Shl => {
                let text = self.peek().describe();
                // `++count`
                if *self.peek() == Tok::Plus && *self.peek_at(1) == Tok::Plus {
                    if let Tok::Ident(name) = self.peek_at(2) {
                        return Err(Diag::error(start, format!("a line cannot start with {text}")).hint(format!("there is no `++`: `{name} += 1`")));
                    }
                }
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
                let init = self.declared_value(&name)?;
                StmtKind::Let { ty: Some(ty), name, name_span, init }
            }
            _ => {
                let expr = self.expr()?;
                if let Some(op) = assign_op(self.peek()) {
                    if let ExprKind::Unsafe(target) = &expr.kind {
                        let text = &self.src[target.span.start..target.span.end];
                        return Err(Diag::error(expr.span, "`unsafe` in front of a value is for reading it; a write is a statement")
                            .hint(format!("write it inside a block: `unsafe {{ {text} = ... }}`")));
                    }
                    // `p?.x = 5`: `?.` reads (the `match` it is read as is no place).
                    if let ExprKind::Match { subject, arms, .. } = &expr.kind {
                        if matches!(arms.get(1).map(|arm| &arm.pattern.kind), Some(PatternKind::Bind(held)) if held.starts_with(OR_NULL)) {
                            let (tested, plain) = (self.src[subject.span.start..subject.span.end].to_string(), self.src[expr.span.start..expr.span.end].replace("?.", "."));
                            let (at, sign) = (expr.span, op.map_or("=".to_string(), |op| format!("{}=", op.symbol())));
                            self.bump();
                            let value = self.expr()?;
                            let value = &self.src[value.span.start..value.span.end];
                            return Err(Diag::error(at, "`?.` only reads: a field is written through a value that is there")
                                .hint(format!("test it first: `if {tested} is not null {{ {plain} {sign} {value} }}`")));
                        }
                    }
                    if !expr.is_place() {
                        let diag = Diag::error(expr.span, "the left side of an assignment must be a variable, a field or an item");
                        // `counts.getOrAdd(word, 0) += 1`: what a call lends is written through a name.
                        return Err(match &expr.kind {
                            ExprKind::Method { .. } | ExprKind::Call { .. } => {
                                let text = &self.src[expr.span.start..expr.span.end];
                                diag.hint(format!("a call is not a place to write to; if it gives a `ref`, write through a variable: `var slot = {text}`, then `slot {}= ...`", op.map_or("", |op| op.symbol())))
                            }
                            _ => diag,
                        });
                    }
                    self.bump();
                    let value = self.expr()?;
                    match (&expr.kind, op) {
                        (ExprKind::Var(name), None) if name == "_" => StmtKind::Discard(value),
                        _ => StmtKind::Assign { target: expr, op, value, through: false },
                    }
                } else if tail_ok(self) {
                    return Ok(Err(expr));
                } else if is_statement(&expr) {
                    StmtKind::Expr(expr)
                } else if let ExprKind::Cond { cond, .. } = &expr.kind {
                    let text = &self.src[cond.span.start..cond.span.end];
                    return Err(Diag::error(expr.span, "`?:` gives a value; it is not a statement")
                        .hint(format!("to do one thing or the other, write an `if`: `if {text} {{ ... }} else {{ ... }}`")));
                } else {
                    let diag = Diag::error(expr.span, "the value of this expression is not used");
                    // `count--` with a call on the next line reads as `count - -call()`.
                    if let ExprKind::Binary(BinOp::Sub, left, right) = &expr.kind {
                        if matches!(right.kind, ExprKind::Unary(UnOp::Neg, _)) && self.src[left.span.end..].trim_start().starts_with("--") {
                            let name = &self.src[left.span.start..left.span.end];
                            return Err(diag.hint(format!("there is no `--`: `{name} -= 1`")));
                        }
                    }
                    return Err(diag.hint("only a call, an assignment or a declaration can be a statement"));
                }
            }
        };
        Ok(Ok(Stmt { kind, span: Span::new(start.start, self.prev_end()) }))
    }

    /// Whether `read x as name {` starts here: the word `as` follows the value.
    /// The name after `break` or `continue`, if one is written on the same line.
    fn loop_named(&mut self) -> Res<Option<Label>> {
        if !matches!(self.peek(), Tok::Ident(_)) {
            return Ok(None);
        }
        let (name, span) = self.ident("the name of a loop")?;
        Ok(Some(Label { name, span }))
    }

    /// `(a..b).reversed()` at the head of a `for`: the range, gone over from its end down.
    /// Anything else in front of `{` is left to be read as before.
    fn reversed_range(&mut self) -> Res<Option<(Expr, Expr)>> {
        if *self.peek() != Tok::LParen {
            return Ok(None);
        }
        let at = self.i;
        self.bump();
        let from = match self.head_expr(true) {
            Ok(from) if *self.peek() == Tok::DotDot => from,
            _ => {
                self.i = at;
                return Ok(None);
            }
        };
        self.bump();
        let to = self.head_expr(true)?;
        let open = self.span();
        self.expect(Tok::RParen, "`)`")?;
        let reversed = *self.peek() == Tok::Dot && matches!(self.peek_at(1), Tok::Ident(word) if word == "reversed") && *self.peek_at(2) == Tok::LParen && *self.peek_at(3) == Tok::RParen;
        if !reversed {
            return Err(Diag::error(open.to(self.span()), "a range in brackets is gone over with `.reversed()`")
                .hint("`for i in (0..n).reversed()` goes from `n - 1` down to `0`; `for i in 0..n` goes up, with no brackets"));
        }
        for _ in 0..4 {
            self.bump();
        }
        Ok(Some((from, to)))
    }

    /// `lock a as x, b as y when c { ... }`, from the word `lock`; `when c` may be left out.
    fn lock_stmt(&mut self, awaited: bool) -> Res<StmtKind> {
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
        let when = match self.peek() {
            Tok::Ident(word) if word == "when" => {
                self.bump();
                Some(Box::new(self.head_expr(false)?))
            }
            _ => None,
        };
        let body = self.block()?;
        Ok(StmtKind::Lock { items, when, awaited, body })
    }

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

    /// Whether the word `select` at `at` tokens from here starts a `select`: `{` follows it,
    /// or a token in brackets and then `{`. Where `{` would start the block of an `if`, a
    /// `while`, a `for` or a `match`, the word is a name.
    fn at_select(&self, at: usize) -> bool {
        if !self.struct_literals {
            return false;
        }
        match self.peek_at(at + 1) {
            Tok::LBrace => true,
            Tok::LParen => {
                let mut depth = 0usize;
                let mut n = at + 1;
                loop {
                    match self.peek_at(n) {
                        Tok::LParen => depth += 1,
                        Tok::RParen => {
                            depth -= 1;
                            if depth == 0 {
                                return *self.peek_at(n + 1) == Tok::LBrace;
                            }
                        }
                        Tok::Eof => return false,
                        _ => {}
                    }
                    n += 1;
                }
            }
            _ => false,
        }
    }

    /// `select [(token)] { arm => body ... }`, from the word `select` (`start` is where the
    /// expression starts, at `await` when it is written). An arm is `ended`, `cancelled`, a
    /// call with `as name` (a receive) or a call without it (a send); the checker sees that
    /// each call is the one it should be.
    fn select_expr(&mut self, start: Span) -> Res<Expr> {
        self.bump();
        let token = match *self.peek() == Tok::LParen {
            true => {
                self.bump();
                let token = self.bracketed(|p| p.expr())?;
                self.expect(Tok::RParen, "`)`")?;
                Some(Box::new(token))
            }
            false => None,
        };
        self.open_brace()?;
        let outer = std::mem::replace(&mut self.struct_literals, true);
        let mut arms = Vec::new();
        self.skip_newlines();
        while *self.peek() != Tok::RBrace {
            if *self.peek() == Tok::Eof {
                return Err(self.unexpected("`}`"));
            }
            let arm_start = self.span();
            let op = match self.peek() {
                Tok::Ident(word) if (word == "ended" || word == "cancelled") && *self.peek_at(1) == Tok::FatArrow => {
                    let ended = word == "ended";
                    self.bump();
                    if ended { SelectOp::Ended } else { SelectOp::Cancelled }
                }
                _ => {
                    let call = self.expr()?;
                    match self.at_word("as") {
                        true => {
                            self.bump();
                            let (name, name_span) = self.ident("a name for the item received, or `_`")?;
                            SelectOp::Receive { call, name, name_span }
                        }
                        false => SelectOp::Send { call, kept: false },
                    }
                }
            };
            if matches!(self.peek(), Tok::Ident(word) if word == "when") {
                return Err(Diag::error(self.span(), "an arm of a `select` has no condition")
                    .note("an arm whose channel has ended is never chosen, so none needs to be turned off by hand"));
            }
            self.expect(Tok::FatArrow, "`=>`")?;
            let (body, value) = match self.peek() {
                Tok::LBrace => self.arm_block()?,
                _ => match self.stmt_or_tail(Tail::Always)? {
                    Ok(stmt) => (vec![stmt], None),
                    Err(value) => (Vec::new(), Some(value)),
                },
            };
            arms.push(SelectArm { op, body, value, span: Span::new(arm_start.start, self.prev_end()) });
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
        Ok(Expr { ty: Ty::Void, moved: false, kind: ExprKind::Select { arms, token, awaited: false }, span })
    }

    /// The block of a `match` arm: statements, and the last line may be the value.
    fn arm_block(&mut self) -> Res<(Block, Option<Expr>)> {
        self.open_brace()?;
        let outer = std::mem::replace(&mut self.struct_literals, true);
        self.visible.push(Vec::new());
        self.visible_statics.push(Vec::new());
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
        self.visible.pop();
        self.visible_statics.pop();
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
        // `0..=int.MaxValue`: a constant is not an end of a range yet.
        if let (Tok::Ident(owner), Tok::Dot, Tok::Ident(name)) = (self.peek(), self.peek_at(1), self.peek_at(2)) {
            let span = self.span().to(self.toks[self.i + 2].span);
            return Err(Diag::error(span, "a range in a pattern ends with a number or a character")
                .note(format!("a constant such as `{owner}.{name}` is not an end of a range yet; write its number")));
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
        self.nest(|parser| parser.pattern_at_in(inside))
    }

    fn pattern_at_in(&mut self, inside: bool) -> Res<Pattern> {
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
                // `int.MinValue..=0`: a constant is not an end of a range yet.
                if let (Tok::Dot, Tok::Ident(constant), Tok::DotDot | Tok::DotDotEq) = (self.peek(), self.peek_at(1), self.peek_at(2)) {
                    let span = start.to(self.toks[self.i + 1].span);
                    return Err(Diag::error(span, "a range in a pattern goes from a number or a character to another")
                        .note(format!("a constant such as `{name}.{constant}` is not an end of a range yet; write its number")));
                }
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

    /// The value a variable is declared with, read with its name at hand for the hints.
    fn declared_value(&mut self, name: &str) -> Res<Expr> {
        let outer = self.declaring.replace(name.to_string());
        let value = self.expr();
        self.declaring = outer;
        value
    }

    /// `c ? a : b` binds loosest, then `a ?? b`; both group to the right, as in C#.
    fn expr(&mut self) -> Res<Expr> {
        self.nest(|parser| parser.expr_in())
    }

    fn expr_in(&mut self) -> Res<Expr> {
        let cond = self.coalesce()?;
        if *self.peek() != Tok::Question {
            return Ok(cond);
        }
        self.bump();
        let then = self.cond_arm()?;
        if !self.eat(&Tok::Colon) {
            // `: b` on the next line: the line ended the statement, as for a line that starts
            // with `?`.
            let mut ahead = 0;
            while *self.peek_at(ahead) == Tok::Newline {
                ahead += 1;
            }
            if ahead > 0 && *self.peek_at(ahead) == Tok::Colon {
                let at = self.toks[self.i + ahead].span;
                return Err(Diag::error(at, "a line cannot start with `:`")
                    .hint("the statement ended on the previous line; to continue it, put `:` at the end of that line"));
            }
            return Err(self.unexpected("`:`").note("`c ? a : b` gives `a` where `c` holds, otherwise `b`"));
        }
        let els = self.cond_arm()?;
        let span = cond.span.to(els.span);
        Ok(Expr { ty: Ty::Void, moved: false, kind: ExprKind::Cond { cond: Box::new(cond), then: Box::new(then), els: Box::new(els) }, span })
    }

    /// An arm of `?:`: a value, never a way out (choice 1915).
    fn cond_arm(&mut self) -> Res<Expr> {
        if *self.peek() == Tok::Return {
            return Err(Diag::error(self.span(), "an arm of `?:` gives a value; it cannot leave the function")
                .hint("leave with an `if`, or with `?? return` on a value that may be `null`"));
        }
        self.expr()
    }

    fn coalesce(&mut self) -> Res<Expr> {
        let left = self.binary(0)?;
        let mark = self.span();
        if !self.eat(&Tok::QuestionQuestion) {
            return Ok(left);
        }
        let written = self.src[left.span.start..left.span.end].to_string();
        // `n ??= 3`, which C# has.
        if *self.peek() == Tok::Assign && self.toks[self.i].span.start == mark.end {
            let at = Span::new(mark.start, self.bump().span.end);
            let value = self.expr()?;
            let value = &self.src[value.span.start..value.span.end];
            return Err(Diag::error(at, "there is no `??=`").hint(format!("`{written} = {written} ?? {value}`")));
        }
        // `?? break`: only `return` leaves from the right of `??`.
        if matches!(self.peek(), Tok::Break | Tok::Continue) {
            let word = if *self.peek() == Tok::Break { "break" } else { "continue" };
            let name = self.declaring.clone().unwrap_or_else(|| "value".to_string());
            return Err(Diag::error(self.span(), format!("`{word}` cannot stand on the right of `??`: only `return` leaves from there"))
                .hint(format!("test it with `is`: on a `Result`, `if {written} is not Ok({name}) {{ {word} }}`; on a `T?`, `var {name} = {written}`, then `if {name} is null {{ {word} }}`")));
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
            self.coalesce()?
        };
        let span = left.span.to(right.span);
        Ok(Expr { ty: Ty::Void, moved: false, kind: ExprKind::Coalesce { left: Box::new(left), right: Box::new(right), on_result: false }, span })
    }

    /// From loosest to tightest: `||`, `&&`, equality, comparison and `is`, `|`, `^`, `&`,
    /// shifts, additive, multiplicative. The same as C# except that `&`, `^` and `|` bind
    /// tighter than a comparison, so `flags & mask == 0` tests the masked value.
    fn binary(&mut self, level: usize) -> Res<Expr> {
        self.keep(|parser| parser.binary_in(level))
    }

    fn binary_in(&mut self, level: usize) -> Res<Expr> {
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
            return self.with_expr();
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
            let op_span = self.bump().span;
            // `ready &&= ok`, from Ruby and JavaScript: there is no such assignment.
            if matches!(op, BinOp::And | BinOp::Or) && *self.peek() == Tok::Assign && self.span().start == op_span.end {
                let symbol = if op == BinOp::And { "&&" } else { "||" };
                let text = &self.src[left.span.start..left.span.end];
                return Err(Diag::error(op_span.to(self.span()), format!("there is no `{symbol}=`"))
                    .hint(format!("write it out: `{text} = {text} {symbol} ...`")));
            }
            // Each operator of a chain nests the tree one more: `a + b + c` is `(a + b) + c`.
            self.deeper()?;
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

    /// `p with { x = 3 }`: a copy of a struct with the fields written changed, as in C#. It
    /// binds tighter than any binary operator and looser than a prefix, so `try load() with {
    /// ... }` changes what `try` gives.
    fn with_expr(&mut self) -> Res<Expr> {
        let mut expr = self.unary()?;
        while self.at_word("with") && *self.peek_at(1) == Tok::LBrace {
            if !self.struct_literals {
                return Err(Diag::error(self.span(), "`with` needs parentheses here, where `{` starts the block")
                    .hint(format!("`({} with {{ ... }})`", &self.src[expr.span.start..expr.span.end])));
            }
            self.bump();
            let fields = self.struct_fields()?;
            let span = Span::new(expr.span.start, self.prev_end());
            expr = Expr { ty: Ty::Void, moved: false, kind: ExprKind::With { base: Box::new(expr), fields }, span };
        }
        Ok(expr)
    }

    fn unary(&mut self) -> Res<Expr> {
        self.keep(|parser| parser.unary_in())
    }

    fn unary_in(&mut self) -> Res<Expr> {
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
                let operand = self.nest(|parser| parser.unary())?;
                Ok(wrap(|e| ExprKind::Unary(UnOp::Neg, e), operand))
            }
            Tok::Bang => {
                self.bump();
                let operand = self.nest(|parser| parser.unary())?;
                Ok(wrap(|e| ExprKind::Unary(UnOp::Not, e), operand))
            }
            Tok::Tilde => {
                self.bump();
                let operand = self.nest(|parser| parser.unary())?;
                Ok(wrap(|e| ExprKind::Unary(UnOp::BitNot, e), operand))
            }
            // `^n` counts from the end of what is indexed (1921), and only there.
            Tok::Caret => Err(Diag::error(start, "`^` counts from the end of what is indexed, so it is written first between `[ ]`: `xs[^1]`, `xs[..^1]`")
                .note("an index from the end is not a value of its own; the bit inverse is `~x`")),
            Tok::New => {
                self.bump();
                if self.at_word("shared") {
                    self.bump();
                    let checked = self.at_word("checked");
                    let locked = self.at_word("locked");
                    if checked || locked {
                        self.bump();
                    }
                    if self.at_word("checked") || self.at_word("locked") {
                        return Err(Diag::error(self.span(), "a value with several owners is `checked` or `locked`, not both")
                            .note("`checked` lets one task change it, each access checked; `locked` lets several tasks change it, inside a `lock` block"));
                    }
                    let operand = self.nest(|parser| parser.unary())?;
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
                let operand = self.nest(|parser| parser.unary())?;
                Ok(wrap(ExprKind::New, operand))
            }
            Tok::Ident(word) if word == "share" => {
                self.bump();
                let operand = self.nest(|parser| parser.unary())?;
                Ok(wrap(ExprKind::Share, operand))
            }
            // `atomic 0`: the value as an `atomic int`, like `locked` in `new shared locked 0`.
            // `atomic uint[3] { ... }` is an array of atomic values, since no array is one.
            Tok::Ident(word) if word == "atomic" => {
                self.bump();
                let mut operand = self.nest(|parser| parser.unary())?;
                if let ExprKind::Array { ty, .. } = &mut operand.kind {
                    if ty.atomic.is_none() && ty.raw == 0 && ty.holder.is_none() {
                        ty.atomic = Some(0);
                        operand.span = start.to(operand.span);
                        return Ok(operand);
                    }
                }
                Ok(wrap(ExprKind::Atomic, operand))
            }
            Tok::Ident(word) if word == "weak" => {
                self.bump();
                let operand = self.nest(|parser| parser.unary())?;
                Ok(wrap(ExprKind::WeakOf, operand))
            }
            // `locked 0` for `new shared locked 0`: a value with several owners is made with
            // `new shared`, and the hint shows the value as written.
            Tok::Ident(word) if word == "checked" || word == "locked" => {
                let word = word.clone();
                self.bump();
                let written = match self.nest(|parser| parser.unary()) {
                    Ok(operand) => self.src[operand.span.start..operand.span.end].to_string(),
                    Err(_) => "...".to_string(),
                };
                Err(Diag::error(start, format!("a `{word}` value has several owners, and is made with `new shared {word}`"))
                    .hint(format!("`new shared {word} {written}`")))
            }
            // `await select { ... }`: the select waits as part of the task.
            Tok::Ident(word) if word == "await" && matches!(self.peek_at(1), Tok::Ident(next) if next == "select") && self.at_select(1) => {
                self.bump();
                let mut select = self.select_expr(start)?;
                if let ExprKind::Select { awaited, .. } = &mut select.kind {
                    *awaited = true;
                }
                Ok(select)
            }
            Tok::Ident(word) if word == "await" => {
                self.bump();
                let operand = self.nest(|parser| parser.unary())?;
                Ok(wrap(ExprKind::Await, operand))
            }
            // A prefix, like `try`: `unsafe p[0] + 1` reads one item and adds to it.
            Tok::Ident(word) if word == "unsafe" => {
                self.bump();
                if *self.peek() == Tok::LBrace {
                    return Err(Diag::error(start.to(self.span()), "`unsafe { }` is a block of statements, not a value")
                        .hint("for a value, write `unsafe` in front of it: `var first = unsafe p[0]`"));
                }
                let operand = self.nest(|parser| parser.unary())?;
                Ok(wrap(ExprKind::Unsafe, operand))
            }
            // `(raw byte)p`: a raw pointer read as one to another type; `(CFunc<int, int>)p`:
            // an address as a function of C; `(ref State)data`: an address lent as a value.
            Tok::LParen if matches!(self.peek_at(1), Tok::Ident(word) if word == "raw" || word == "CFunc") || self.lends_address() => {
                self.bump();
                let ty = self.type_expr()?;
                self.expect(Tok::RParen, "`)`")?;
                let operand = self.nest(|parser| parser.unary())?;
                let span = start.to(operand.span);
                Ok(Expr { ty: Ty::Void, moved: false, kind: ExprKind::RawCast { ty, expr: Box::new(operand) }, span })
            }
            // A prefix, like `await`: `try a().b()` tries the whole chain.
            Tok::Try => {
                self.bump();
                let operand = self.nest(|parser| parser.unary())?;
                Ok(wrap(|e| ExprKind::Try { expr: e, kind: TryKind::Null }, operand))
            }
            Tok::Read => {
                self.bump();
                let operand = self.nest(|parser| parser.unary())?;
                Ok(wrap(|e| ExprKind::Borrow { mutable: false, expr: e }, operand))
            }
            Tok::Ref => {
                self.bump();
                let operand = self.nest(|parser| parser.unary())?;
                Ok(wrap(|e| ExprKind::Borrow { mutable: true, expr: e }, operand))
            }
            // `(int)x`, `(char)code`: a number type, `char` or `bool` alone in parentheses is a cast.
            Tok::LParen => match self.cast_type() {
                Some(ty) => {
                    self.bump();
                    self.bump();
                    self.bump();
                    let operand = self.nest(|parser| parser.unary())?;
                    let span = start.to(operand.span);
                    Ok(Expr { ty: Ty::Void, moved: false, kind: ExprKind::Cast { ty, expr: Box::new(operand) }, span })
                }
                // `(Block)code`: a type of the program in parentheses, then a value.
                None if matches!(self.peek_at(1), Tok::Ident(name) if name.starts_with(char::is_uppercase))
                    && *self.peek_at(2) == Tok::RParen
                    && matches!(self.peek_at(3), Tok::Ident(_) | Tok::Int(_)) =>
                {
                    // The checker tells an alias of a number type from anything else (choice 2214).
                    let Tok::Ident(name) = self.peek_at(1).clone() else { unreachable!("tested above") };
                    self.bump();
                    self.bump();
                    self.bump();
                    let operand = self.nest(|parser| parser.unary())?;
                    let span = start.to(operand.span);
                    Ok(Expr { ty: Ty::Void, moved: false, kind: ExprKind::Cast { ty: Ty::Struct(name), expr: Box::new(operand) }, span })
                }
                None => self.postfix(),
            },
            _ => self.postfix(),
        }
    }

    /// `(ref State)data`, `(read State)data`: a type behind `ref` or `read` alone in
    /// parentheses, with a value right after it.
    fn lends_address(&self) -> bool {
        if !matches!(self.peek_at(1), Tok::Read | Tok::Ref) {
            return false;
        }
        let Some(n) = self.type_len(2) else { return false };
        *self.peek_at(2 + n) == Tok::RParen && matches!(self.peek_at(3 + n), Tok::Ident(_) | Tok::LParen)
    }

    fn cast_type(&self) -> Option<Ty> {
        let Tok::Ident(name) = self.peek_at(1) else { return None };
        if *self.peek_at(2) != Tok::RParen {
            return None;
        }
        // `(bool)n` is read as a cast too, for the checker to say that a number is not a `bool`.
        Ty::from_name(name).filter(|ty| ty.is_number() || matches!(ty, Ty::Char | Ty::Bool))
    }

    fn postfix(&mut self) -> Res<Expr> {
        let expr = self.primary()?;
        self.postfix_from(expr)
    }

    /// The fields, methods and items read after `expr`.
    fn postfix_from(&mut self, expr: Expr) -> Res<Expr> {
        self.keep(|parser| parser.postfix_from_in(expr))
    }

    fn postfix_from_in(&mut self, mut expr: Expr) -> Res<Expr> {
        loop {
            // Each field, call or index read after another nests the tree one more.
            self.deeper()?;
            if *self.peek() == Tok::LBracket {
                expr = self.index_or_slice(expr)?;
                continue;
            }
            // `a?.b.c`: if `a` is `null`, so is the whole chain, and the rest of it does not
            // run (C#). Read as `match a { null => null, var held => held.b.c }`, where `held`
            // has a name no program can write; the checker lends `a` when it is a place.
            if *self.peek() == Tok::Question && *self.peek_at(1) == Tok::Dot {
                let mark = self.bump().span;
                let held = format!("{OR_NULL}{}", mark.start);
                let start = expr.span.start;
                let stand_in = Expr { ty: Ty::Void, moved: false, kind: ExprKind::Var(held.clone()), span: expr.span };
                let rest = self.postfix_from(stand_in)?;
                let null = Expr { ty: Ty::Void, moved: false, kind: ExprKind::Null, span: mark };
                let arms = vec![
                    Arm { pattern: Pattern { kind: PatternKind::Null, span: mark }, guard: None, body: Vec::new(), value: Some(null), span: mark },
                    Arm { pattern: Pattern { kind: PatternKind::Bind(held), span: mark }, guard: None, body: Vec::new(), span: rest.span, value: Some(rest) },
                ];
                let kind = ExprKind::Match { subject: Box::new(expr), arms, borrowed: false };
                return Ok(Expr { ty: Ty::Void, moved: false, kind, span: Span::new(start, self.prev_end()) });
            }
            if !self.eat(&Tok::Dot) {
                break;
            }
            // `file.lock()`: the word `lock` is the name of a method after a dot (choice 1986),
            // and a name nowhere else.
            let (name, name_span) = match (self.peek(), self.peek_at(1)) {
                (Tok::Ident(word), Tok::LParen) if word == "lock" => ("lock".to_string(), self.bump().span),
                _ => self.ident("a field or method name")?,
            };
            // `billing.Line { ... }`: a struct with its module written in front.
            if *self.peek() == Tok::LBrace && self.struct_literals {
                if let Some(module) = dotted(&expr) {
                    let start = expr.span.start;
                    let fields = self.struct_fields()?;
                    let kind = ExprKind::StructLit { name: format!("{}.{name}", self.map.full(&module)), name_span: Span::new(start, name_span.end), fields };
                    expr = Expr { ty: Ty::Void, moved: false, kind, span: Span::new(start, self.prev_end()) };
                    continue;
                }
            }
            // `Memory.sizeOf<int>()`, `Memory.alignOf<int>()`: what C has for the type.
            if (name == "sizeOf" || name == "alignOf") && *self.peek() == Tok::Lt && matches!(&expr.kind, ExprKind::Var(owner) if owner == "Memory") {
                let start = expr.span.start;
                self.bump();
                let item = self.type_expr()?;
                self.expect(Tok::Gt, "`>`")?;
                self.expect(Tok::LParen, "`(`")?;
                self.expect(Tok::RParen, "`)`")?;
                let kind = ExprKind::SizeOf { item, align: name == "alignOf", of: Ty::Void };
                expr = Expr { ty: Ty::Void, moved: false, kind, span: Span::new(start, self.prev_end()) };
                continue;
            }
            // `Memory.allocate<int>(n)`: the type of the items is written after the name.
            if name == "allocate" && *self.peek() == Tok::Lt && matches!(&expr.kind, ExprKind::Var(owner) if owner == "Memory") {
                let start = expr.span.start;
                self.bump();
                let item = self.type_expr()?;
                self.expect(Tok::Gt, "`>`")?;
                if *self.peek() != Tok::LParen {
                    return Err(self.unexpected("`(`").hint("`Memory.allocate<int>(count)`"));
                }
                let args = self.args()?;
                expr = Expr { ty: Ty::Void, moved: false, kind: ExprKind::Allocate { item, args }, span: Span::new(start, self.prev_end()) };
                continue;
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
                _ => Some(Box::new(p.index_end()?)),
            };
            if *p.peek() == Tok::DotDotEq {
                return Err(Diag::error(p.span(), "`..=` is written only in a pattern; a range of a text or a slice stops before its end")
                    .hint("`[a..b + 1]`"));
            }
            if !p.eat(&Tok::DotDot) {
                let index = from.expect("read above when there is no `..`");
                // `xs[^0]` is past the last item whatever the length (1923).
                if let ExprKind::Binary(BinOp::Sub, end, count) = &index.kind {
                    if matches!(end.kind, ExprKind::End) && matches!(count.kind, ExprKind::Int(0, _)) {
                        return Err(Diag::error(index.span, "`^0` is the end itself, past the last item, so this index is always out of range")
                            .hint("the last item is `^1`; `^0` is written as the end of a range: `[a..^0]`"));
                    }
                }
                return Ok(ExprKind::Index { recv, index, kind: IndexKind::Text, unchecked: false });
            }
            let to = match p.peek() {
                Tok::RBracket => None,
                _ => Some(Box::new(p.index_end()?)),
            };
            Ok(ExprKind::Slice { recv, from, to })
        })?;
        self.expect(Tok::RBracket, "`]`")?;
        Ok(Expr { ty: Ty::Void, moved: false, kind, span: Span::new(start, self.prev_end()) })
    }

    /// An index or an end of a range. `^n` counts from the end (1921) and is read as `End - n`,
    /// `End` being the length of what is indexed. What follows `^` is one operand, as after
    /// `-`; an operator after it is refused, since `^k + 1` could be read either way (1924).
    fn index_end(&mut self) -> Res<Expr> {
        if *self.peek() != Tok::Caret {
            return self.expr();
        }
        let mark = self.bump().span;
        let at = self.i;
        let count = self.unary()?;
        if matches!(count.kind, ExprKind::Int(value, _) if value < 0) {
            return Err(Diag::error(mark.to(count.span), "a count from the end is not negative: `^1` is the last item")
                .hint("an index from the start is written without `^`"));
        }
        if !matches!(self.peek(), Tok::RBracket | Tok::DotDot | Tok::DotDotEq) {
            self.i = at;
            if let Ok(whole) = self.expr() {
                if matches!(self.peek(), Tok::RBracket | Tok::DotDot | Tok::DotDotEq) {
                    let diag = Diag::error(mark.to(whole.span), "`^` counts one operand from the end; what it counts is put in parentheses");
                    return Err(diag.hint(self.counted_back_hint(&whole)));
                }
            }
            self.i = at;
            self.unary()?;
            return Err(self.unexpected("`]` or `..`"));
        }
        let end = Expr { ty: Ty::Void, moved: false, kind: ExprKind::End, span: mark };
        let span = mark.to(count.span);
        Ok(Expr { ty: Ty::Void, moved: false, kind: ExprKind::Binary(BinOp::Sub, Box::new(end), Box::new(count)), span })
    }

    /// The hint for `xs[^k - 1]`, which reads two ways: `^(k - 1)` counts `k - 1` from the end,
    /// and one before `^k` is `^(k + 1)`. A reading that gives no item (`^(1 - 1)` is `^0`) is
    /// left out, and a count of numbers is worked out.
    fn counted_back_hint(&self, whole: &Expr) -> String {
        let text = &self.src[whole.span.start..whole.span.end];
        let first = format!("`^({text})` counts `{text}` from the end");
        let ExprKind::Binary(op @ (BinOp::Add | BinOp::Sub), left, right) = &whole.kind else { return format!("`^({text})`") };
        if matches!(left.kind, ExprKind::Binary(..)) {
            return format!("`^({text})`");
        }
        let (l, r) = (&self.src[left.span.start..left.span.end], &self.src[right.span.start..right.span.end]);
        let (word, back) = if *op == BinOp::Sub { ("before", "+") } else { ("after", "-") };
        let numbers = match (&left.kind, &right.kind) {
            (ExprKind::Int(a, _), ExprKind::Int(b, _)) => Some((*a, *b)),
            _ => None,
        };
        let (whole_count, other) = match numbers {
            Some((a, b)) => {
                let (whole_count, other) = if *op == BinOp::Sub { (a - b, a + b) } else { (a + b, a - b) };
                (Some(whole_count), match other > 0 {
                    true => format!("`^{other}`"),
                    false => String::new(),
                })
            }
            None => (None, format!("`^({l} {back} {r})`")),
        };
        let second = format!("{r} {word} `^{l}` is {other}");
        match (whole_count.is_some_and(|count| count <= 0), other.is_empty()) {
            (true, false) => second,
            (false, true) | (true, true) => first,
            (false, false) => format!("{first}; {second}"),
        }
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
            // `select { ... }`, `select (read token) { ... }`: a word only where it starts one
            // (1998); `var select = db.prepare(...)` is a name.
            Tok::Ident(ref word) if word == "select" && self.at_select(0) => return self.select_expr(start),
            Tok::LParen => {
                self.bump();
                let inner = self.bracketed(|p| p.expr())?;
                self.expect(Tok::RParen, "`)`")?;
                return Ok(Expr { ty: Ty::Void, moved: false, kind: inner.kind, span: Span::new(start.start, self.prev_end()) });
            }
            // `for x in List<int> { 1, 2 } {`: the `{` starts the block, so the items are not read.
            Tok::Ident(_) if !self.struct_literals && self.at_braced_collection() => {
                let n = self.type_len(0).expect("a type is written here");
                let ty = &self.src[self.toks[self.i].span.start..self.toks[self.i + n - 1].span.end];
                return Err(Diag::error(self.span(), "a collection with its items needs parentheses here, where `{` starts the block")
                    .hint(format!("`({ty} {{ ... }})`")));
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
            // `embed("logo.gif")`: the bytes of a file, read when the program is compiled.
            Tok::Ident(ref word) if word == "embed" && *self.peek_at(1) == Tok::LParen => {
                self.bump();
                let args = self.args()?;
                let span = Span::new(start.start, self.prev_end());
                let written = match args.as_slice() {
                    [Expr { kind: ExprKind::Str(path), .. }] => path.clone(),
                    _ => {
                        return Err(Diag::error(span, "`embed` takes the path of a file, written out as a text")
                            .hint("`const byte[] Logo = embed(\"logo.gif\")`")
                            .note("the file is read when the program is compiled, so its path is known then"));
                    }
                };
                if self.in_text || !self.in_const {
                    return Err(Diag::error(span, "`embed` is written where a const is given its value")
                        .hint("`const byte[] Logo = embed(\"logo.gif\")`, then `Logo` where the bytes are needed"));
                }
                embed_path(&written, span)?;
                self.embeds.push(Embed { written, span, bytes: None });
                ExprKind::Embed(self.embed_base + self.embeds.len() - 1)
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
                return Err(self.unexpected("an expression").note("inside an expression `return` is written only on the right side of `??` or as an arm of a `match`"))
            }
            _ => return Err(self.no_expression()),
        };
        Ok(Expr { ty: Ty::Void, moved: false, kind, span: Span::new(start.start, self.prev_end()) })
    }

    /// `{ value = 1, next = null }`; fields are separated by commas, line ends do not matter.
    fn struct_fields(&mut self) -> Res<Vec<FieldInit>> {
        let name = match &self.toks[self.i.saturating_sub(1)].tok {
            Tok::Ident(name) => name.clone(),
            _ => String::new(),
        };
        self.bump();
        let fields = self.bracketed(|p| {
            let mut fields = Vec::new();
            p.skip_newlines();
            while *p.peek() != Tok::RBrace {
                // `Scores { 1, 2 }` with `alias Scores = List<int>`: items, not fields.
                let items = fields.is_empty() && name.starts_with(char::is_uppercase)
                    && (matches!(p.peek(), Tok::Int(_) | Tok::Float(_) | Tok::Str(_) | Tok::Interp(_) | Tok::Char(_) | Tok::True | Tok::False | Tok::Minus)
                        || matches!(p.peek(), Tok::Ident(_)) && matches!(p.peek_at(1), Tok::Comma | Tok::RBrace));
                let (name, span) = p.ident("a field name").map_err(|diag| match items {
                    true => diag.note(format!(
                        "`{name} {{ ... }}` builds a struct, one `field = value` at a time; a collection is built by the name of its type, not by an alias: `List<int> {{ 1, 2 }}`"
                    )),
                    false => diag,
                })?;
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
            && (*self.peek_at(n) == Tok::LParen
                || (*self.peek_at(n) == Tok::LBrace && self.struct_literals)
                // `List<byte>.repeat(0, n)`: a function of the type, read as a method of the
                // collection written without parentheses (1648).
                || (*self.peek_at(n) == Tok::Dot && matches!(self.peek_at(n + 1), Tok::Ident(_)) && *self.peek_at(n + 2) == Tok::LParen))
    }

    /// Whether a type with `<...>` and then `{` is written here: `List<int> { 1, 2 }`.
    fn at_braced_collection(&self) -> bool {
        if *self.peek_at(1) != Tok::Lt {
            return false;
        }
        let Some(n) = self.type_len(0) else { return false };
        *self.peek_at(n - 1) == Tok::Gt && *self.peek_at(n) == Tok::LBrace
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
                    let mut inner = Parser { src: self.src, map: self.map.clone(), toks, i: 0, struct_literals: true, visible: Vec::new(), embeds: Vec::new(), embed_base: 0, in_text: true, consts_found: Vec::new(), in_const: false, visible_statics: Vec::new(), statics_found: Vec::new(), depth: self.depth, declaring: None };
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
/// Whether an expression can stand alone as a statement: a call, a `match`, `try` and `await`
/// of one, and `unsafe` in front of any of them.
fn is_statement(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Call { .. } | ExprKind::Method { .. } | ExprKind::Match { .. } | ExprKind::Select { .. } | ExprKind::Try { .. } | ExprKind::Await(_) => true,
        ExprKind::Unsafe(inner) => is_statement(inner),
        _ => false,
    }
}

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

/// `./native.c` and `./c/helpers.c`: a C file in the module's folder or below it. It ends in
/// `.c`, and no part of it leaves the folder (`..`) or is empty.
/// The path of a file of `embed`: in the folder of the file it is written in or below it,
/// written with `/`, without `..`, `.` or an empty part, and not from the root or a drive (as
/// Go's `//go:embed` takes it). Where a link leads is looked at when the file is read.
fn embed_path(written: &str, span: Span) -> Res<()> {
    let parts_fine = written.split('/').all(|part| !part.is_empty() && part != "." && part != "..");
    let chars_fine = !written.chars().any(|c| c == '\\' || c == ':' || c.is_control());
    // `./motd.txt`: the file may well be there; the `./` is what is not written (#45).
    if let Some(rest) = written.strip_prefix("./").filter(|rest| embed_path(rest, span).is_ok()) {
        return Err(Diag::error(span, format!("the path of a file of `embed` is written without `./`: `{rest}`"))
            .note("it is read from the folder of the file it is written in")
            .hint(format!("`embed(\"{rest}\")`")));
    }
    if !parts_fine || !chars_fine {
        return Err(Diag::error(span, format!("`{written}` is not a file beside this one"))
            .note("a file of `embed` lies in the folder of the file it is written in or below it, and its path is written from there with `/`, without `..`")
            .hint("`embed(\"logo.gif\")`, `embed(\"images/logo.gif\")`"));
    }
    Ok(())
}

fn c_file_path(written: &str, span: Span) -> Res<String> {
    let rest = &written[2..];
    let parts_fine = rest.split('/').all(|part| !part.is_empty() && part != "." && part != "..");
    let chars_fine = rest.chars().all(|c| c.is_ascii_alphanumeric() || "_-./+".contains(c));
    if !rest.ends_with(".c") || rest.len() <= 2 || !parts_fine || !chars_fine {
        return Err(Diag::error(span, format!("`{written}` is not a C file of this module"))
            .note("a C file beside the program lies in the module's folder or below it, ends in `.c`, and its path is letters, digits and `_ - . +`, without `..`")
            .hint("`extern \"./native.c\" { ... }`"));
    }
    Ok(written.to_string())
}
