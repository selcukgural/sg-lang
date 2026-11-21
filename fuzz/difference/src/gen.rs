//! The generator: from a seed, a program of SG that the checker takes, that ends, and whose
//! only output is what it prints (choices 1362, 1363, 1365). It keeps the type of every value it
//! writes, so that each expression has the type the place it stands in needs; it bounds every
//! loop and the depth of every recursion, and counts the steps the program may take, so that it
//! ends in a fraction of a second. Most operations that could panic are guarded (`i & 7` as an
//! index, `d | 1` as a divisor, the wrapping methods); about one in ten is not, and its panic,
//! message and code, must be the same interpreted and compiled.
//!
//! Stage 1: the integer types, `bool`, arithmetic, the bit operators, conversions, comparisons,
//! `if`, `match`, `for` and `while`, functions, recursion, arrays, consts, `print` and `$"..."`
//! with a format.
//!
//! The program is a tree of `Node`s, each a statement or a block, so that the reducer can take
//! any of them away (choice 1366).

use crate::rng::Rng;

/// A statement, or a block with its parts (`if c {` ... `} else {` ...) and the line that ends it.
#[derive(Clone, Debug)]
pub struct Node {
    pub parts: Vec<(String, Vec<Node>)>,
    pub end: Option<String>,
}

impl Node {
    pub(crate) fn line(text: String) -> Node {
        Node { parts: vec![(text, Vec::new())], end: None }
    }

    pub(crate) fn block(head: String, body: Vec<Node>) -> Node {
        Node { parts: vec![(head, body)], end: Some("}".to_string()) }
    }
}

/// The program as text.
pub fn render(nodes: &[Node]) -> String {
    let mut out = String::new();
    render_in(nodes, 0, &mut out);
    out
}

fn render_in(nodes: &[Node], indent: usize, out: &mut String) {
    render_walk(nodes, indent, out, &mut Vec::new(), &mut Vec::new());
}

/// Where a node stands in the tree: the way to the list that holds it (each step a node and one
/// of its parts) and its place in that list.
pub type Place = (Vec<(usize, usize)>, usize);

/// The program as text, and for each of its lines the node that wrote it (choice 1593: the
/// line the checker names is taken away).
pub fn render_mapped(nodes: &[Node]) -> (String, Vec<Option<Place>>) {
    let (mut out, mut lines) = (String::new(), Vec::new());
    render_walk(nodes, 0, &mut out, &mut Vec::new(), &mut lines);
    (out, lines)
}

fn render_walk(nodes: &[Node], indent: usize, out: &mut String, way: &mut Vec<(usize, usize)>, lines: &mut Vec<Option<Place>>) {
    for (at, node) in nodes.iter().enumerate() {
        let place = Some((way.clone(), at));
        for (part, (head, body)) in node.parts.iter().enumerate() {
            out.push_str(&"    ".repeat(indent));
            out.push_str(head);
            out.push('\n');
            lines.extend(std::iter::repeat_n(place.clone(), head.lines().count().max(1)));
            way.push((at, part));
            render_walk(body, indent + 1, out, way, lines);
            way.pop();
        }
        if let Some(end) = &node.end {
            out.push_str(&"    ".repeat(indent));
            out.push_str(end);
            out.push('\n');
            lines.push(place.clone());
        }
        if indent == 0 {
            out.push('\n');
            lines.push(None);
        }
    }
}

/// An integer type: its name, its bits, whether it is signed, and the fewest and most bits it
/// has on a machine (`nint` and `nuint` count as "at least 32", choice 126).
struct IntTy {
    name: &'static str,
    title: &'static str,
    bits: u32,
    signed: bool,
    least: u32,
    most: u32,
}

const INTS: [IntTy; 10] = [
    IntTy { name: "sbyte", title: "Sbyte", bits: 8, signed: true, least: 8, most: 8 },
    IntTy { name: "byte", title: "Byte", bits: 8, signed: false, least: 8, most: 8 },
    IntTy { name: "short", title: "Short", bits: 16, signed: true, least: 16, most: 16 },
    IntTy { name: "ushort", title: "Ushort", bits: 16, signed: false, least: 16, most: 16 },
    IntTy { name: "int", title: "Int", bits: 32, signed: true, least: 32, most: 32 },
    IntTy { name: "uint", title: "Uint", bits: 32, signed: false, least: 32, most: 32 },
    IntTy { name: "long", title: "Long", bits: 64, signed: true, least: 64, most: 64 },
    IntTy { name: "ulong", title: "Ulong", bits: 64, signed: false, least: 64, most: 64 },
    IntTy { name: "nint", title: "Nint", bits: 64, signed: true, least: 32, most: 64 },
    IntTy { name: "nuint", title: "Nuint", bits: 64, signed: false, least: 32, most: 64 },
];

const INT: usize = 4;

/// The largest value of a type on every machine (`nint` has at least 32 bits).
fn most(t: &IntTy) -> u64 {
    match t.signed {
        true => (1u64 << (t.least - 1)) - 1,
        false => u64::MAX >> (64 - t.least),
    }
}

/// A mask that leaves of a `from` a value that fits in `to`, and is itself a `from`.
fn fitting_mask(from: &IntTy, to: &IntTy) -> u64 {
    most(from).min(most(to))
}

/// `a op b` in a const, with the operands masked so that it fits in its type: a const that
/// overflows is an error, and a const calls no function such as `wrappingAdd`.
fn const_arithmetic(t: &IntTy, op: &str, a: &str, b: &str) -> String {
    let half = most(t) >> 1;
    match (op, t.signed) {
        ("+", _) => format!("((({a}) & {half}) + (({b}) & {half}))"),
        ("-", true) => format!("((({a}) & {half}) - (({b}) & {half}))"),
        // At least `half + 1` less at most `half`.
        ("-", false) => format!("((({a}) | {}) - (({b}) & {half}))", half + 1),
        _ => format!("((({a}) & 7) * (({b}) & 7))"),
    }
}

/// Whether every value of `from` fits in `to` on every machine, so that it becomes one on its own.
fn widens(from: usize, to: usize) -> bool {
    let (a, b) = (&INTS[from], &INTS[to]);
    if from == to {
        return false;
    }
    match (a.signed, b.signed) {
        (true, false) => false,
        (false, true) => a.most < b.least,
        _ => a.most <= b.least,
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Ty {
    Int(usize),
    Array(usize),
    /// Stage 2: a struct, an enum, an integer that may be `null`, a list, a text.
    Struct(usize),
    Enum(usize),
    Opt(usize),
    List(usize),
    Text,
}

struct Var {
    name: String,
    ty: Ty,
    read: bool,
    /// Whether a statement may give it a value: not a parameter, a loop's variable or the
    /// counter of a `while`.
    mutable: bool,
}

struct Func {
    name: String,
    ret: Option<usize>,
    params: Vec<usize>,
    /// The most steps a call of it takes.
    cost: u64,
    /// It gives a `Result<T, int>`, `T` being `ret`.
    result: bool,
}

/// The variants of an enum: each one's name and the types it carries.
type Variants = Vec<(String, Vec<usize>)>;

/// The most steps one function may take, its calls counted in.
const BUDGET: u64 = 20_000;

/// An expression and whether it has a type of its own: a number written alone takes the type of
/// the place it stands in, so where nothing gives one it is cast (`(byte)5`).
type Expr = (String, bool);

pub struct Gen {
    rng: Rng,
    scopes: Vec<Vec<Var>>,
    funcs: Vec<Func>,
    consts: Vec<(String, usize)>,
    names: usize,
    /// The loops around the statement being written: whether each is a `while`, whose counter a
    /// `continue` would step over.
    loops: Vec<bool>,
    /// How many times the statement being written may run in one call of its function.
    times: u64,
    /// The steps the function being written has taken so far.
    cost: u64,
    /// A const is being written: only what the checker can find when the program is compiled
    /// and that cannot overflow there.
    in_const: bool,
    /// The structs: name, and each field's name and type. All `Copyable`, so no value moves.
    structs: Vec<(String, Vec<(String, usize)>)>,
    /// The enums: name, and each variant's name and the types it carries.
    enums: Vec<(String, Variants)>,
    /// The function being written gives a `Result`, so `try` may stand in it.
    in_result: bool,
    /// How many indexes the expression being written is inside: an index may hold an item of
    /// another array, and that one an index, so past two an index is a number.
    indexing: u32,
    /// How many calls the expression being written is an argument of: past two, no call.
    calling: u32,
}

impl Gen {
    pub fn new(seed: u64) -> Gen {
        Gen { rng: Rng::new(seed), scopes: Vec::new(), funcs: Vec::new(), consts: Vec::new(), names: 0, loops: Vec::new(), times: 1, cost: 0, in_const: false, structs: Vec::new(), enums: Vec::new(), in_result: false, indexing: 0, calling: 0 }
    }

    fn name(&mut self, prefix: &str) -> String {
        self.names += 1;
        format!("{prefix}{}", self.names)
    }

    /// The whole program.
    pub fn program(mut self) -> Vec<Node> {
        let mut nodes = Vec::new();
        for _ in 0..self.rng.below(3) {
            nodes.push(self.struct_decl());
        }
        for _ in 0..self.rng.below(3) {
            nodes.push(self.enum_decl());
        }
        for _ in 0..self.rng.below(4) {
            nodes.push(self.constant());
        }
        for _ in 0..self.rng.below(5) {
            let node = match self.rng.below(10) {
                0..=1 => self.recursive(),
                2..=3 => self.result_function(),
                _ => self.function(),
            };
            nodes.push(node);
        }
        // `main` reads each const nothing else read, so none is left unused.
        self.cost = 0;
        self.times = 1;
        self.scopes.push(Vec::new());
        let mut body = self.statements(0);
        let consts: Vec<String> = self.consts.iter().map(|(name, _)| name.clone()).collect();
        for name in consts {
            body.push(Node::line(format!("print({name})")));
        }
        body.extend(self.close_scope());
        nodes.push(Node::block("void main() {".to_string(), body));
        nodes
    }

    fn constant(&mut self) -> Node {
        let ty = self.rng.below(INTS.len() as u64) as usize;
        self.in_const = true;
        let value = self.int_expr(ty, 2);
        self.in_const = false;
        let name = self.name("K");
        self.consts.push((name.clone(), ty));
        Node::line(format!("const {} {name} = {}", INTS[ty].name, value.0))
    }

    fn function(&mut self) -> Node {
        let name = self.name("f");
        let ret = if self.rng.chance(25) { None } else { Some(self.int_ty()) };
        let params: Vec<usize> = (0..self.rng.below(4)).map(|_| self.int_ty()).collect();
        self.cost = 0;
        self.times = 1;
        self.scopes.push(Vec::new());
        let mut list = Vec::new();
        for &ty in &params {
            let param = self.name("p");
            list.push(format!("{} {param}", INTS[ty].name));
            self.scopes.last_mut().unwrap().push(Var { name: param, ty: Ty::Int(ty), read: true, mutable: false });
        }
        // The locals live in a scope of their own inside that of the parameters, so the value
        // returned sees the parameters after the locals have been read.
        let mut body = self.block(0);
        if let Some(ret) = ret {
            let value = self.int_expr(ret, 3);
            body.push(Node::line(format!("return {}", value.0)));
        }
        self.scopes.pop();
        let head = format!("{} {name}({}) {{", ret.map_or("void", |ty| INTS[ty].name), list.join(", "));
        self.funcs.push(Func { name, ret, params, cost: self.cost + 1, result: false });
        Node::block(head, body)
    }

    /// A function that calls itself, its depth given by its first parameter.
    fn recursive(&mut self) -> Node {
        let name = self.name("r");
        let ty = self.int_ty();
        let (depth, acc) = (self.name("d"), self.name("a"));
        self.scopes.push(vec![
            Var { name: depth.clone(), ty: Ty::Int(INT), read: true, mutable: false },
            Var { name: acc.clone(), ty: Ty::Int(ty), read: true, mutable: false },
        ]);
        // Its expressions call nothing: the steps of a call are counted at its callers.
        let funcs = std::mem::take(&mut self.funcs);
        let next = self.int_expr(ty, 2);
        self.funcs = funcs;
        self.scopes.pop();
        let tyname = INTS[ty].name;
        let body = vec![
            Node::block(format!("if {depth} <= 0 {{"), vec![Node::line(format!("return {acc}"))]),
            Node::line(format!("return {name}({depth} - 1, {})", next.0)),
        ];
        // At most 30 deep (`call`), each a few steps.
        self.funcs.push(Func { name: name.clone(), ret: Some(ty), params: vec![INT, ty], cost: 31 * 4, result: false });
        Node::block(format!("{tyname} {name}(int {depth}, {tyname} {acc}) {{"), body)
    }

    fn int_ty(&mut self) -> usize {
        // `int` and `long` more often, as programs have them.
        match self.rng.below(14) {
            0..=2 => INT,
            3..=4 => 6,
            n => (n - 5) as usize % INTS.len(),
        }
    }

    // --- Statements ---

    fn statements(&mut self, depth: usize) -> Vec<Node> {
        let count = 1 + self.rng.below(if depth == 0 { 8 } else { 4 });
        let mut nodes = Vec::new();
        for _ in 0..count {
            if let Some(node) = self.statement(depth) {
                nodes.push(node);
            }
        }
        nodes
    }

    /// The prints that read each variable of the scope nothing read, and the end of the scope.
    fn close_scope(&mut self) -> Vec<Node> {
        let scope = self.scopes.pop().unwrap_or_default();
        // A value that may be `null` has no text; it is printed with what stands for `null`.
        scope
            .into_iter()
            .filter(|var| !var.read)
            .map(|var| match var.ty {
                Ty::Opt(_) => Node::line(format!("print({} ?? 0)", var.name)),
                _ => Node::line(format!("print({})", var.name)),
            })
            .collect()
    }

    fn block(&mut self, depth: usize) -> Vec<Node> {
        self.scopes.push(Vec::new());
        let mut body = self.statements(depth);
        body.extend(self.close_scope());
        body
    }

    fn statement(&mut self, depth: usize) -> Option<Node> {
        self.cost += self.times;
        let nested = depth < 3;
        Some(match self.rng.below(135) {
            0..=19 => self.declare(),
            20..=24 => self.declare_array(),
            25..=39 => self.assign()?,
            40..=44 => self.assign_item()?,
            45..=59 => self.print(),
            60..=67 if nested => {
                let cond = self.bool_expr(2);
                let then = self.block(depth + 1);
                match self.rng.chance(50) {
                    true => {
                        let other = self.block(depth + 1);
                        Node { parts: vec![(format!("if {cond} {{"), then), ("} else {".to_string(), other)], end: Some("}".to_string()) }
                    }
                    false => Node::block(format!("if {cond} {{"), then),
                }
            }
            68..=75 if nested && self.loops.len() < 2 => self.for_loop(depth),
            76..=79 if nested && self.loops.len() < 2 => self.while_loop(depth),
            80..=85 if nested => self.match_stmt(depth),
            86..=91 => self.call_stmt()?,
            92..=95 if !self.loops.is_empty() => {
                let cond = self.bool_expr(1);
                let word = if *self.loops.last().unwrap() || self.rng.chance(50) { "break" } else { "continue" };
                Node::block(format!("if {cond} {{"), vec![Node::line(word.to_string())])
            }
            96..=99 => self.print(),
            _ => self.statement_more(depth)?,
        })
    }

    fn declare(&mut self) -> Node {
        let ty = self.int_ty();
        let value = self.int_expr(ty, 3);
        let name = self.name("v");
        self.scopes.last_mut().unwrap().push(Var { name: name.clone(), ty: Ty::Int(ty), read: false, mutable: true });
        Node::line(format!("{} {name} = {}", INTS[ty].name, value.0))
    }

    fn declare_array(&mut self) -> Node {
        let ty = self.int_ty();
        let items: Vec<String> = (0..8).map(|_| self.int_expr(ty, 1).0).collect();
        let name = self.name("a");
        self.scopes.last_mut().unwrap().push(Var { name: name.clone(), ty: Ty::Array(ty), read: false, mutable: true });
        Node::line(format!("var {name} = {}[8] {{ {} }}", INTS[ty].name, items.join(", ")))
    }

    /// A variable in scope that `pick` takes, chosen at random.
    fn var_where(&mut self, pick: impl Fn(&Var) -> bool) -> Option<(String, Ty)> {
        let found: Vec<(String, Ty)> = self.scopes.iter().flatten().filter(|var| pick(var)).map(|var| (var.name.clone(), var.ty)).collect();
        if found.is_empty() {
            return None;
        }
        Some(self.rng.pick(&found).clone())
    }

    fn mark_read(&mut self, name: &str) {
        for var in self.scopes.iter_mut().flatten() {
            if var.name == name {
                var.read = true;
            }
        }
    }

    fn assign(&mut self) -> Option<Node> {
        let (name, ty) = self.var_where(|var| var.mutable && matches!(var.ty, Ty::Int(_)))?;
        let Ty::Int(ty) = ty else { unreachable!() };
        let value = self.int_expr(ty, 3);
        // `+=`, `-=` and `*=` panic on overflow; most often the wrapping method is written instead.
        let (op, word) = *self.rng.pick(&[("+=", "Add"), ("-=", "Subtract"), ("*=", "Multiply"), ("^=", ""), ("|=", ""), ("&=", ""), ("=", ""), ("=", "")]);
        Some(match word.is_empty() || self.rng.chance(10) {
            true => Node::line(format!("{name} {op} {}", value.0)),
            false => Node::line(format!("{name} = ({name}).wrapping{word}({})", value.0)),
        })
    }

    fn assign_item(&mut self) -> Option<Node> {
        let (name, ty) = self.var_where(|var| var.mutable && matches!(var.ty, Ty::Array(_)))?;
        let Ty::Array(ty) = ty else { unreachable!() };
        let index = self.index();
        let value = self.int_expr(ty, 2);
        Some(Node::line(format!("{name}[{index}] = {}", value.0)))
    }

    fn print(&mut self) -> Node {
        match self.rng.below(10) {
            0..=4 => {
                let ty = self.int_ty();
                let value = self.int_expr(ty, 3);
                Node::line(format!("print({})", self.typed(value, ty)))
            }
            5 => {
                let value = self.bool_expr(2);
                Node::line(format!("print({value})"))
            }
            6 => match self.var_where(|var| matches!(var.ty, Ty::Array(_))) {
                Some((name, _)) => {
                    self.mark_read(&name);
                    Node::line(format!("print({name})"))
                }
                None => self.print(),
            },
            _ => {
                let mut pieces = Vec::new();
                for _ in 0..1 + self.rng.below(3) {
                    let ty = self.int_ty();
                    let value = self.int_expr(ty, 2);
                    let value = self.typed(value, ty);
                    let spec = *self.rng.pick(&["", "", ":x", ":X", ":b", ":D3", ":x8", ":b4"]);
                    pieces.push(format!("{{{value}{spec}}}"));
                }
                Node::line(format!("print($\"{}\")", pieces.join(" ")))
            }
        }
    }

    fn for_loop(&mut self, depth: usize) -> Node {
        let start = self.rng.below(3);
        let end = start + self.rng.below(7);
        let name = self.name("i");
        let saved = self.times;
        self.times = self.times.saturating_mul((end - start).max(1));
        self.loops.push(false);
        self.scopes.push(vec![Var { name: name.clone(), ty: Ty::Int(INT), read: false, mutable: false }]);
        let mut body = self.statements(depth + 1);
        body.extend(self.close_scope());
        self.loops.pop();
        self.times = saved;
        Node::block(format!("for {name} in {start}..{end} {{"), body)
    }

    /// `var w = 0`, then `while w < n { ... w += 1 }`, with no `continue` that would step over
    /// the count.
    fn while_loop(&mut self, depth: usize) -> Node {
        let rounds = self.rng.below(6);
        let name = self.name("w");
        let saved = self.times;
        self.times = self.times.saturating_mul(rounds.max(1));
        self.loops.push(true);
        let mut body = self.block(depth + 1);
        body.push(Node::line(format!("{name} += 1")));
        self.loops.pop();
        self.times = saved;
        Node {
            parts: vec![(format!("var {name} = 0"), Vec::new()), (format!("while {name} < {rounds} {{"), body)],
            end: Some("}".to_string()),
        }
    }

    fn match_stmt(&mut self, depth: usize) -> Node {
        let ty = self.int_ty();
        let subject = self.int_expr(ty, 2);
        let subject = self.typed(subject, ty);
        let mut seen = Vec::new();
        let mut arms = Vec::new();
        for _ in 0..1 + self.rng.below(4) {
            let pattern = self.pattern(ty);
            if seen.contains(&pattern) {
                continue;
            }
            seen.push(pattern.clone());
            let body = self.block(depth + 1);
            arms.push(Node::block(format!("{pattern} => {{"), body));
        }
        let body = self.block(depth + 1);
        arms.push(Node::block("_ => {".to_string(), body));
        Node::block(format!("match {subject} {{"), arms)
    }

    /// A number of the type for a pattern: a small one, an end of the type, or any.
    fn pattern(&mut self, ty: usize) -> String {
        match self.rng.below(4) {
            0 | 1 => self.rng.below(10).to_string(),
            _ => self.number(ty).trim_start_matches('(').trim_end_matches(')').to_string(),
        }
    }

    fn call_stmt(&mut self) -> Option<Node> {
        let at = self.callable(|_| true)?;
        let call = self.call(at);
        Some(match self.funcs[at].ret {
            None => Node::line(call),
            Some(_) => Node::line(format!("_ = {call}")),
        })
    }

    /// A function the statement being written may call without its steps passing the budget.
    fn callable(&mut self, pick: impl Fn(&Func) -> bool) -> Option<usize> {
        let found: Vec<usize> = (0..self.funcs.len()).filter(|&at| pick(&self.funcs[at]) && self.cost + self.times.saturating_mul(self.funcs[at].cost) <= BUDGET).collect();
        if found.is_empty() || self.in_const || self.calling >= 2 {
            return None;
        }
        let at = *self.rng.pick(&found);
        self.cost += self.times.saturating_mul(self.funcs[at].cost);
        Some(at)
    }

    fn call(&mut self, at: usize) -> String {
        let params = self.funcs[at].params.clone();
        let recursive = self.funcs[at].name.starts_with('r');
        self.calling += 1;
        let mut args = Vec::new();
        for (n, ty) in params.into_iter().enumerate() {
            if recursive && n == 0 {
                args.push(self.rng.below(31).to_string());
            } else {
                args.push(self.int_expr(ty, 1).0);
            }
        }
        self.calling -= 1;
        format!("{}({})", self.funcs[at].name, args.join(", "))
    }

    // --- Expressions ---

    /// `value` with a type of its own.
    fn typed(&self, value: Expr, ty: usize) -> String {
        match value.1 {
            true => value.0,
            // A number only a `ulong` holds cannot be cast: it is read as a `long` first.
            false => match value.0.parse::<u64>() {
                Ok(big) if big > i64::MAX as u64 => format!("({}.MaxValue ^ {})", INTS[ty].name, !big),
                _ => format!("({}){}", INTS[ty].name, value.0),
            },
        }
    }

    /// A number of the type, written alone (it takes the type of where it stands).
    fn number(&mut self, ty: usize) -> String {
        let t = &INTS[ty];
        match self.rng.below(5) {
            0 | 1 => self.rng.below(10).to_string(),
            2 => self.rng.below(200).min(if t.signed { (1u64 << (t.bits - 1)) - 1 } else { u64::MAX >> (64 - t.bits) }).to_string(),
            _ => {
                let bits = self.rng.next() >> (64 - t.bits);
                if t.signed {
                    // Shifted back with its sign, and never the lowest, which is written `T.MinValue`.
                    let value = ((bits << (64 - t.bits)) as i64) >> (64 - t.bits);
                    let lowest = i64::MIN >> (64 - t.bits);
                    match value {
                        v if v == lowest => (v + 1).to_string(),
                        v if v < 0 => format!("({v})"),
                        v => v.to_string(),
                    }
                } else {
                    bits.to_string()
                }
            }
        }
    }

    fn int_expr(&mut self, ty: usize, depth: usize) -> Expr {
        if depth == 0 || self.rng.chance(25) {
            return self.int_leaf(ty);
        }
        let t = &INTS[ty];
        let (name, title, bits, signed) = (t.name, t.title, t.bits, t.signed);
        let d = depth - 1;
        match self.rng.below(13) {
            0..=2 => {
                let a = self.int_expr(ty, d);
                let a = self.typed(a, ty);
                let b = self.int_expr(ty, d).0;
                let (word, op) = *self.rng.pick(&[("Add", "+"), ("Subtract", "-"), ("Multiply", "*")]);
                // A const calls no function (choice 1606) and overflows nowhere: its operands
                // are masked so that the operator stays within the type on every machine.
                if self.in_const {
                    return (const_arithmetic(&INTS[ty], op, &a, &b), true);
                }
                match self.rng.below(40) {
                    0..=31 => (format!("({a}).wrapping{word}({b})"), true),
                    32..=37 => (format!("({a}).saturating{word}({b})"), true),
                    _ => (format!("({a} {op} {b})"), true),
                }
            }
            3 if !self.in_const => {
                let a = self.int_expr(ty, d);
                let a = self.typed(a, ty);
                let b = self.int_expr(ty, d);
                let op = if self.rng.chance(50) { "/" } else { "%" };
                match b.1 && self.rng.chance(5) {
                    true => (format!("({a} {op} {})", b.0), true),
                    false => (format!("({a} {op} ({} | 1))", b.0), true),
                }
            }
            4 => {
                let a = self.int_expr(ty, d);
                let a = self.typed(a, ty);
                let b = self.int_expr(ty, d).0;
                let op = *self.rng.pick(&["&", "|", "^"]);
                (format!("({a} {op} {b})"), true)
            }
            5 => {
                let a = self.int_expr(ty, d);
                let a = self.typed(a, ty);
                let op = if self.rng.chance(50) { "<<" } else { ">>" };
                let count = self.int_expr(INT, d.min(1));
                match count.1 && self.rng.chance(4) && !self.in_const {
                    true => (format!("({a} {op} {})", count.0), true),
                    false if count.1 => (format!("({a} {op} ({} & {}))", count.0, bits - 1), true),
                    false => (format!("({a} {op} {})", self.rng.below(bits as u64)), true),
                }
            }
            6 => {
                let a = self.int_expr(ty, d);
                let a = self.typed(a, ty);
                match signed && self.rng.chance(10) && !self.in_const {
                    true => (format!("(-{a})"), true),
                    false => (format!("(~{a})"), true),
                }
            }
            7..=8 => {
                let from = self.int_ty();
                if from == ty {
                    return self.int_expr(ty, d);
                }
                let value = self.int_expr(from, d);
                let value = self.typed(value, from);
                if widens(from, ty) {
                    return (format!("({name})({value})"), true);
                }
                if self.in_const {
                    return (format!("({name})(({value}) & {})", fitting_mask(&INTS[from], &INTS[ty])), true);
                }
                match self.rng.below(40) {
                    0..=31 => (format!("({value}).wrappingTo{title}()"), true),
                    32..=35 => (format!("({value}).saturatingTo{title}()"), true),
                    36..=38 => {
                        let other = self.number(ty);
                        (format!("(({value}).tryTo{title}() ?? {other})"), true)
                    }
                    _ => (format!("({name})({value})"), true),
                }
            }
            9 => {
                // Each way writes what it uses: an expression made and left out would mark as
                // read a variable the program never reads.
                let way = self.rng.below(4);
                if way == 3 && ty == INT && !self.in_const {
                    let from = self.int_ty();
                    let value = self.int_expr(from, d);
                    let value = self.typed(value, from);
                    let method = *self.rng.pick(&["countOnes", "countZeros", "leadingZeros", "trailingZeros"]);
                    return (format!("({value}).{method}()"), true);
                }
                let a = self.int_expr(ty, d);
                let a = self.typed(a, ty);
                match way {
                    0 | 1 if !self.in_const => {
                        // Typed: the two meet as with `+`, so `b.max(300)` with a `byte b` is an `int` (163).
                        let b = self.int_expr(ty, d);
                        let b = self.typed(b, ty);
                        let method = if way == 0 { "min" } else { "max" };
                        (format!("({a}).{method}({b})"), true)
                    }
                    2 if signed && !self.in_const && self.rng.chance(30) => (format!("({a}).abs()"), true),
                    _ => (format!("({a} ^ {})", self.number(ty)), true),
                }
            }
            10 => match self.callable(|func| func.ret == Some(ty) && !func.result) {
                Some(at) => (self.call(at), true),
                None => self.int_leaf(ty),
            },
            _ => self.int_leaf(ty),
        }
    }

    fn int_leaf(&mut self, ty: usize) -> Expr {
        let t = &INTS[ty];
        let (name, signed) = (t.name, t.signed);
        let roll = self.rng.below(100);
        if roll < 55 && !self.in_const {
            let found = self.var_where(|var| match var.ty {
                Ty::Int(of) => of == ty || widens(of, ty),
                Ty::Array(of) => of == ty,
                _ => false,
            });
            if let Some((var, of)) = found {
                self.mark_read(&var);
                return match of {
                    Ty::Int(of) if of == ty => (var, true),
                    Ty::Int(_) => (format!("({name}){var}"), true),
                    _ => {
                        let index = self.index();
                        (format!("{var}[{index}]"), true)
                    }
                };
            }
        }
        if roll < 62 && !self.in_const {
            if let Some(value) = self.int_from_more(ty) {
                return value;
            }
        }
        if roll < 70 {
            let found: Vec<(String, usize)> = self.consts.iter().filter(|(_, of)| *of == ty || widens(*of, ty)).cloned().collect();
            if !found.is_empty() {
                let (konst, of) = self.rng.pick(&found).clone();
                return match of == ty {
                    true => (konst, true),
                    false => (format!("({name}){konst}"), true),
                };
            }
        }
        match self.rng.below(12) {
            0 => (format!("{name}.MaxValue"), true),
            1 if signed => (format!("{name}.MinValue"), true),
            _ => (self.number(ty), false),
        }
    }

    /// An index of an array of 8: most often cut to `0..8` with `& 7`.
    fn index(&mut self) -> String {
        if self.indexing >= 2 {
            return self.rng.below(8).to_string();
        }
        self.indexing += 1;
        let value = self.int_expr(INT, 1);
        self.indexing -= 1;
        match value.1 {
            true if self.rng.chance(4) => value.0,
            true => format!("({} & 7)", value.0),
            false => self.rng.below(8).to_string(),
        }
    }

    fn bool_expr(&mut self, depth: usize) -> String {
        if self.rng.chance(15) {
            if let Some(test) = self.bool_from_more() {
                return test;
            }
        }
        if depth == 0 || self.rng.chance(30) {
            return self.comparison(0);
        }
        match self.rng.below(4) {
            0 => format!("({} && {})", self.bool_expr(depth - 1), self.bool_expr(depth - 1)),
            1 => format!("({} || {})", self.bool_expr(depth - 1), self.bool_expr(depth - 1)),
            2 => format!("!{}", self.bool_expr(depth - 1)),
            _ => self.comparison(depth - 1),
        }
    }

    fn comparison(&mut self, depth: usize) -> String {
        let ty = self.int_ty();
        let a = self.int_expr(ty, depth + 1);
        let a = self.typed(a, ty);
        let b = self.int_expr(ty, depth + 1).0;
        let op = *self.rng.pick(&["==", "!=", "<", "<=", ">", ">="]);
        format!("({a} {op} {b})")
    }
}

/// Stage 2 (choice 1365): structs, enums and `match` on them, an integer that may be `null`,
/// lists, text, and functions that give a `Result`. Every struct and enum is `Copyable`, and a
/// list or a text is never given away whole, so no value is moved: borrows are stage 3
/// (`borrow.rs`).
impl Gen {
    fn struct_decl(&mut self) -> Node {
        let name = self.name("S");
        let fields: Vec<(String, usize)> = (0..1 + self.rng.below(3)).map(|_| (self.name("x"), self.int_ty())).collect();
        let lines = fields.iter().map(|(field, ty)| Node::line(format!("{} {field}", INTS[*ty].name))).collect();
        self.structs.push((name.clone(), fields));
        Node::block(format!("struct {name} : Copyable, Formattable, Equatable {{"), lines)
    }

    fn enum_decl(&mut self) -> Node {
        let name = self.name("E");
        let mut variants = Vec::new();
        let mut lines = Vec::new();
        for _ in 0..1 + self.rng.below(3) {
            let variant = self.name("V");
            let carried: Vec<usize> = (0..self.rng.below(3)).map(|_| self.int_ty()).collect();
            let line = match carried.is_empty() {
                true => variant.clone(),
                false => {
                    let fields: Vec<String> = carried.iter().map(|ty| format!("{} {}", INTS[*ty].name, self.name("q"))).collect();
                    format!("{variant}({})", fields.join(", "))
                }
            };
            lines.push(Node::line(line));
            variants.push((variant, carried));
        }
        self.enums.push((name.clone(), variants));
        Node::block(format!("enum {name} : Copyable, Formattable, Equatable {{"), lines)
    }

    /// `Result<T, int> g(...)`: an error under a condition, and a value otherwise.
    fn result_function(&mut self) -> Node {
        let name = self.name("g");
        let ret = self.int_ty();
        let params: Vec<usize> = (0..self.rng.below(3)).map(|_| self.int_ty()).collect();
        self.cost = 0;
        self.times = 1;
        self.in_result = true;
        self.scopes.push(Vec::new());
        let mut list = Vec::new();
        for &ty in &params {
            let param = self.name("p");
            list.push(format!("{} {param}", INTS[ty].name));
            self.scopes.last_mut().unwrap().push(Var { name: param, ty: Ty::Int(ty), read: true, mutable: false });
        }
        let mut body = self.block(0);
        let cond = self.bool_expr(1);
        let error = self.int_expr(INT, 1).0;
        body.push(Node::block(format!("if {cond} {{"), vec![Node::line(format!("return Error({error})"))]));
        let value = self.int_expr(ret, 2).0;
        body.push(Node::line(format!("return Ok({value})")));
        self.scopes.pop();
        self.in_result = false;
        self.funcs.push(Func { name: name.clone(), ret: Some(ret), params, cost: self.cost + 2, result: true });
        Node::block(format!("Result<{}, int> {name}({}) {{", INTS[ret].name, list.join(", ")), body)
    }

    fn declare_var(&mut self, prefix: &str, ty: Ty) -> String {
        let name = self.name(prefix);
        self.scopes.last_mut().unwrap().push(Var { name: name.clone(), ty, read: false, mutable: true });
        name
    }

    fn struct_value(&mut self, k: usize) -> String {
        if self.rng.chance(40) {
            if let Some((var, _)) = self.var_where(|var| var.ty == Ty::Struct(k)) {
                self.mark_read(&var);
                return var;
            }
        }
        let (name, fields) = self.structs[k].clone();
        let values: Vec<String> = fields.iter().map(|(field, ty)| format!("{field} = {}", self.int_expr(*ty, 1).0)).collect();
        format!("{name} {{ {} }}", values.join(", "))
    }

    fn enum_value(&mut self, k: usize) -> String {
        if self.rng.chance(40) {
            if let Some((var, _)) = self.var_where(|var| var.ty == Ty::Enum(k)) {
                self.mark_read(&var);
                return var;
            }
        }
        let (name, variants) = self.enums[k].clone();
        let (variant, carried) = self.rng.pick(&variants).clone();
        match carried.is_empty() {
            true => format!("{name}.{variant}"),
            false => {
                let values: Vec<String> = carried.iter().map(|ty| self.int_expr(*ty, 1).0).collect();
                format!("{name}.{variant}({})", values.join(", "))
            }
        }
    }

    fn text_literal(&mut self) -> String {
        let length = self.rng.below(5);
        let letters: String = (0..length).map(|_| (b'a' + self.rng.below(4) as u8) as char).collect();
        format!("\"{letters}\"")
    }

    fn statement_more(&mut self, depth: usize) -> Option<Node> {
        let nested = depth < 3;
        Some(match self.rng.below(16) {
            0 if !self.structs.is_empty() => {
                let k = self.rng.below(self.structs.len() as u64) as usize;
                let value = self.struct_value(k);
                let name = self.declare_var("s", Ty::Struct(k));
                Node::line(format!("var {name} = {value}"))
            }
            1 => {
                let (var, ty) = self.var_where(|var| var.mutable && matches!(var.ty, Ty::Struct(_)))?;
                let Ty::Struct(k) = ty else { unreachable!() };
                let (field, fty) = self.rng.pick(&self.structs[k].1).clone();
                let value = self.int_expr(fty, 2).0;
                Node::line(format!("{var}.{field} = {value}"))
            }
            2 if !self.enums.is_empty() => {
                let k = self.rng.below(self.enums.len() as u64) as usize;
                let value = self.enum_value(k);
                let name = self.declare_var("e", Ty::Enum(k));
                Node::line(format!("var {name} = {value}"))
            }
            3 if nested && !self.enums.is_empty() => {
                let k = self.rng.below(self.enums.len() as u64) as usize;
                let subject = self.enum_value(k);
                let variants = self.enums[k].1.clone();
                let mut arms = Vec::new();
                for (variant, carried) in variants {
                    self.scopes.push(Vec::new());
                    let mut bound = Vec::new();
                    for ty in carried {
                        let name = self.name("b");
                        self.scopes.last_mut().unwrap().push(Var { name: name.clone(), ty: Ty::Int(ty), read: false, mutable: false });
                        bound.push(name);
                    }
                    let mut body = self.statements(depth + 1);
                    body.extend(self.close_scope());
                    let head = match bound.is_empty() {
                        true => format!("{variant} => {{"),
                        false => format!("{variant}({}) => {{", bound.join(", ")),
                    };
                    arms.push(Node::block(head, body));
                }
                Node::block(format!("match {subject} {{"), arms)
            }
            4 => {
                let ty = self.int_ty();
                let value = if self.rng.chance(30) { "null".to_string() } else { self.int_expr(ty, 2).0 };
                let name = self.declare_var("o", Ty::Opt(ty));
                Node::line(format!("{}? {name} = {value}", INTS[ty].name))
            }
            5 => {
                let (var, ty) = self.var_where(|var| var.mutable && matches!(var.ty, Ty::Opt(_)))?;
                let Ty::Opt(of) = ty else { unreachable!() };
                let value = if self.rng.chance(30) { "null".to_string() } else { self.int_expr(of, 2).0 };
                Node::line(format!("{var} = {value}"))
            }
            6 => {
                // Narrowed inside the block: the value is an integer there.
                let (var, ty) = self.var_where(|var| matches!(var.ty, Ty::Opt(_)))?;
                let Ty::Opt(of) = ty else { unreachable!() };
                self.mark_read(&var);
                let other = self.number(of);
                Node::block(format!("if {var} is not null {{"), vec![Node::line(format!("print({var} ^ {other})"))])
            }
            7 => {
                let ty = self.int_ty();
                let items: Vec<String> = (0..1 + self.rng.below(4)).map(|_| self.int_expr(ty, 1).0).collect();
                let name = self.declare_var("l", Ty::List(ty));
                Node::line(format!("var {name} = List<{}> {{ {} }}", INTS[ty].name, items.join(", ")))
            }
            8 => {
                let (var, ty) = self.var_where(|var| matches!(var.ty, Ty::List(_)))?;
                let Ty::List(of) = ty else { unreachable!() };
                self.mark_read(&var);
                let value = self.int_expr(of, 2).0;
                Node::line(format!("{var}.add({value})"))
            }
            9 => {
                let (var, ty) = self.var_where(|var| matches!(var.ty, Ty::List(_)))?;
                let Ty::List(of) = ty else { unreachable!() };
                let index = self.list_index(&var);
                let value = self.int_expr(of, 2).0;
                Node::line(format!("{var}[{index}] = {value}"))
            }
            10 => {
                let value = match self.rng.chance(50) {
                    true => self.text_literal(),
                    false => {
                        let ty = self.int_ty();
                        let value = self.int_expr(ty, 1);
                        format!("$\"{{{}}}-\".toString()", self.typed(value, ty))
                    }
                };
                let name = self.declare_var("t", Ty::Text);
                Node::line(format!("var {name} = {value}"))
            }
            11 => {
                let (var, _) = self.var_where(|var| var.mutable && var.ty == Ty::Text)?;
                self.mark_read(&var);
                let ty = self.int_ty();
                let value = self.int_expr(ty, 1);
                Node::line(format!("{var} = $\"{{{var}}}{{{}}}\".toString()", self.typed(value, ty)))
            }
            12 => {
                let (var, ty) = self.var_where(|var| !matches!(var.ty, Ty::Int(_)))?;
                self.mark_read(&var);
                match ty {
                    Ty::Opt(_) => Node::line(format!("print({var} ?? 0)")),
                    _ => Node::line(format!("print({var})")),
                }
            }
            13 if nested => {
                let at = self.callable(|func| func.result)?;
                let call = self.call(at);
                let ret = self.funcs[at].ret.unwrap();
                let mut arms = Vec::new();
                for (word, ty) in [("Ok", ret), ("Error", INT)] {
                    self.scopes.push(Vec::new());
                    let name = self.name("b");
                    self.scopes.last_mut().unwrap().push(Var { name: name.clone(), ty: Ty::Int(ty), read: false, mutable: false });
                    let mut body = self.statements(depth + 1);
                    body.extend(self.close_scope());
                    arms.push(Node::block(format!("{word}({name}) => {{"), body));
                }
                Node::block(format!("match {call} {{"), arms)
            }
            14 if self.in_result => {
                let at = self.callable(|func| func.result)?;
                let call = self.call(at);
                let ty = self.funcs[at].ret.unwrap();
                let name = self.declare_var("v", Ty::Int(ty));
                Node::line(format!("{} {name} = try {call}", INTS[ty].name))
            }
            _ => self.print(),
        })
    }

    /// An index of a list, most often cut to its length (a list here never gets shorter, and has
    /// an item from the start).
    fn list_index(&mut self, list: &str) -> String {
        // The cut reads the list (`length()`); an index that is not cut does not, and the list
        // written through it is not read by that (`l[5] = x` alone is a value never read).
        if self.indexing >= 2 {
            self.mark_read(list);
            return format!("(3 % {list}.length())");
        }
        self.indexing += 1;
        let value = self.int_expr(INT, 1);
        self.indexing -= 1;
        let value = self.typed(value, INT);
        match self.rng.chance(4) {
            true => value,
            false => {
                self.mark_read(list);
                format!("(({value} & 0x7fff) % {list}.length())")
            }
        }
    }

    /// An integer of type `ty` out of a value of stage 2, if there is one in scope.
    fn int_from_more(&mut self, ty: usize) -> Option<Expr> {
        let name = INTS[ty].name;
        let fits = |of: usize| of == ty || widens(of, ty);
        let cast = |text: String, of: usize| -> Expr {
            match of == ty {
                true => (text, true),
                false => (format!("({name})({text})"), true),
            }
        };
        let mut ways: Vec<u8> = Vec::new();
        let structs = self.structs.clone();
        if self.var_where(|var| matches!(var.ty, Ty::Struct(k) if structs[k].1.iter().any(|(_, of)| fits(*of)))).is_some() {
            ways.push(0);
        }
        if self.var_where(|var| matches!(var.ty, Ty::Opt(of) if fits(of))).is_some() {
            ways.push(1);
        }
        if self.var_where(|var| matches!(var.ty, Ty::List(of) if of == ty)).is_some() {
            ways.push(2);
        }
        if ty == 8 && self.var_where(|var| matches!(var.ty, Ty::List(_) | Ty::Text)).is_some() {
            ways.push(3);
        }
        if self.funcs.iter().any(|func| func.result && func.ret == Some(ty)) {
            ways.push(4);
        }
        if ways.is_empty() {
            return None;
        }
        match *self.rng.pick(&ways) {
            0 => {
                let (var, Ty::Struct(k)) = self.var_where(|var| matches!(var.ty, Ty::Struct(k) if structs[k].1.iter().any(|(_, of)| fits(*of))))? else { return None };
                self.mark_read(&var);
                let fields: Vec<(String, usize)> = structs[k].1.iter().filter(|(_, of)| fits(*of)).cloned().collect();
                let (field, of) = self.rng.pick(&fields).clone();
                Some(cast(format!("{var}.{field}"), of))
            }
            1 => {
                let (var, Ty::Opt(of)) = self.var_where(|var| matches!(var.ty, Ty::Opt(of) if fits(of)))? else { return None };
                self.mark_read(&var);
                let other = self.number(of);
                Some(cast(format!("({var} ?? {other})"), of))
            }
            2 => {
                let (var, _) = self.var_where(|var| matches!(var.ty, Ty::List(of) if of == ty))?;
                self.mark_read(&var);
                let index = self.list_index(&var);
                Some((format!("{var}[{index}]"), true))
            }
            3 => {
                let (var, _) = self.var_where(|var| matches!(var.ty, Ty::List(_) | Ty::Text))?;
                self.mark_read(&var);
                Some((format!("{var}.length()"), true))
            }
            _ => {
                let at = self.callable(|func| func.result && func.ret == Some(ty))?;
                let call = self.call(at);
                let other = self.number(ty);
                Some((format!("({call} ?? {other})"), true))
            }
        }
    }

    /// A test of a value of stage 2, if there is one in scope.
    fn bool_from_more(&mut self) -> Option<String> {
        // Not `x is null`: it narrows `x` where it holds, and the generator does not follow where
        // that is, so a `??` written there would be an error. The narrowing is tried apart, in a
        // block that only prints (`statement_more`).
        let (var, ty) = self.var_where(|var| !matches!(var.ty, Ty::Int(_) | Ty::Array(_) | Ty::List(_) | Ty::Opt(_)))?;
        self.mark_read(&var);
        Some(match ty {
            Ty::Struct(k) => {
                let other = self.struct_value(k);
                format!("({var} == {other})")
            }
            Ty::Enum(k) => {
                let other = self.enum_value(k);
                format!("({var} == {other})")
            }
            _ => {
                let method = *self.rng.pick(&["contains", "startsWith", "endsWith"]);
                let other = self.text_literal();
                format!("{var}.{method}({other})")
            }
        })
    }
}
