//! Stage 3, borrows (choice 1593): programs that borrow text and lists, keep the borrows in
//! variables, in structs and in lists of structs, give them to functions as `ref` and `read`, and
//! change, replace and move what they borrow from, in `if`, `match`, `for`, `while`, with `break`
//! and `continue`, over `shared` lists and with `Disposable` values whose `dispose` runs at the end
//! of a block.
//!
//! Unlike stages 1 and 2, these programs are not written to be right: many break the rules of
//! borrowing, and the checker must refuse those. What it takes must run as the interpreter runs it,
//! and without a report of the address sanitizer: a borrow the checker let live past a change of
//! what it borrows from reads freed memory there. Every text is long enough to live on the heap,
//! and every change of a list or a text frees what was there, so that such a read is seen.

use crate::gen::Node;
use crate::rng::Rng;

/// The types and functions every program starts with.
const PRELUDE: &str = "\
struct Word<from source> : Copyable {
    read string text
    int n
}

struct Holder<from source> {
    ref List<string> items
}

struct Shelf<from source> {
    ref List<Word> words
}

struct Guard<from source> : Disposable {
    read string text
    void dispose(ref this) {
        print($\"done {this.text}\")
    }
}

struct Sweeper<from source> : Disposable {
    ref List<string> items
    void dispose(ref this) {
        refill(ref this.items)
    }
}

void refill(ref List<string> xs) {
    xs.clear()
    xs.add(\"a text put back by refill, long enough for the heap\".clone())
}

int g(ref List<string> xs) {
    refill(xs)
    return 1
}

void renew(ref string s) {
    s = \"a text made new by renew, long enough for the heap\".clone()
}

nint count(read List<string> xs) {
    return xs.length()
}

int grow(ref List<string> xs) {
    var n = (int)xs.length()
    xs.clear()
    for i in 0..n + 1 {
        xs.add($\"a text grown by grow, long enough for the heap {i}\".toString())
    }
    return n + 1
}

Word wrap(read string s) from s {
    return Word { text = s, n = 1 }
}

read string first(read List<string> xs) from xs {
    return read xs[0]
}

void wipeHolder(ref Holder h) {
    refill(ref h.items)
}

void take(List<string> xs) {
    print(xs.length())
}

int next(ref int n) {
    n += 1
    return n
}
";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    /// A `string` that owns its text.
    Text,
    /// A `List<string>`, never empty.
    List,
    /// A `shared List<string>`, never empty.
    Shared,
    /// A `read string`.
    View,
    /// A `Word`, which holds a `read string`.
    Word,
    /// A `Holder`, which holds a `ref List<string>`.
    Holder,
    /// A `List<Word>`, never empty.
    Words,
    /// A `Shelf`, which holds a `ref List<Word>`.
    Shelf,
    /// A `Guard`, `Disposable`, which holds a `read string`.
    Guard,
    /// A `List<Guard>` or a `Guard[1]`.
    Guards,
    /// A `Sweeper`, `Disposable`, whose `dispose` changes the list it holds.
    Sweeper,
    /// A `ref List<string>`: a parameter, or a variable `var p = ref xs`.
    RefList,
    /// A `ref string` parameter.
    RefText,
    /// The `int` of a loop.
    Int,
}

struct Var {
    name: String,
    kind: Kind,
    /// It may be given a new value: not a parameter or the variable of a loop.
    mutable: bool,
    /// Something reads it: a variable nothing reads is an error.
    read: bool,
    /// The type it was declared with, when it is a parameter.
    param: Option<&'static str>,
}

/// A helper function the program writes: its name and the kinds of its parameters.
struct Helper {
    name: String,
    params: Vec<&'static str>,
}

pub struct Borrow {
    rng: Rng,
    scopes: Vec<Vec<Var>>,
    names: usize,
    /// The loops around the statement being written.
    loops: usize,
    helpers: Vec<Helper>,
    texts: usize,
}

/// How many nodes the prelude is: the first of every program, never taken away by `fit`.
pub fn prelude_len() -> usize {
    PRELUDE.split("\n\n").count()
}

impl Borrow {
    pub fn new(seed: u64) -> Borrow {
        Borrow { rng: Rng::new(seed), scopes: Vec::new(), names: 0, loops: 0, helpers: Vec::new(), texts: 0 }
    }

    fn name(&mut self, prefix: &str) -> String {
        self.names += 1;
        format!("{prefix}{}", self.names)
    }

    /// The whole program: the prelude, then the helpers and `main`.
    pub fn program(mut self) -> Vec<Node> {
        let mut nodes: Vec<Node> = PRELUDE.split("\n\n").map(|item| Node { parts: vec![(item.trim_end().to_string(), Vec::new())], end: None }).collect();
        for _ in 0..self.rng.below(3) {
            nodes.push(self.helper());
        }
        self.scopes.push(Vec::new());
        let count = 6 + self.rng.below(8);
        let mut body = self.statements(0, count);
        body.extend(self.close_scope());
        nodes.push(Node::block("void main() {".to_string(), body));
        nodes
    }

    /// `void hN(ref List<string> p, ref string q, read string r)`, some of them, its body written
    /// with the parameters as what it has.
    fn helper(&mut self) -> Node {
        let name = self.name("h");
        let mut params = Vec::new();
        let mut list = Vec::new();
        self.scopes.push(Vec::new());
        for _ in 0..1 + self.rng.below(3) {
            let (kind, ty) = *self.rng.pick(&[(Kind::RefList, "ref List<string>"), (Kind::RefList, "ref List<string>"), (Kind::RefText, "ref string"), (Kind::View, "read string"), (Kind::Holder, "read Holder"), (Kind::Holder, "ref Holder")]);
            let param = self.name("p");
            list.push(format!("{ty} {param}"));
            params.push(ty);
            self.scopes.last_mut().unwrap().push(Var { name: param, kind, mutable: false, read: false, param: Some(ty) });
        }
        let count = 2 + self.rng.below(5);
        let mut body = self.statements(1, count);
        body.extend(self.close_scope());
        self.helpers.push(Helper { name: name.clone(), params });
        Node::block(format!("void {name}({}) {{", list.join(", ")), body)
    }

    /// The end of a scope: what nothing read is read, and some of the rest again, after all that
    /// came before.
    fn close_scope(&mut self) -> Vec<Node> {
        let scope = self.scopes.pop().unwrap_or_default();
        let mut nodes = Vec::new();
        for var in scope {
            if !var.read || self.rng.chance(60) {
                if let Some(line) = self.show(&var.name, var.kind) {
                    nodes.push(Node::line(format!("print({line})")));
                }
            }
        }
        nodes
    }

    /// What `print` shows of a variable, reading what it borrows.
    fn show(&mut self, name: &str, kind: Kind) -> Option<String> {
        Some(match kind {
            Kind::Text | Kind::View | Kind::List | Kind::Shared | Kind::RefList | Kind::RefText | Kind::Int => name.to_string(),
            Kind::Word | Kind::Guard => format!("{name}.text"),
            Kind::Holder | Kind::Sweeper => format!("{name}.items"),
            Kind::Words => format!("{name}[0].text"),
            Kind::Shelf => format!("{name}.words[0].text"),
            Kind::Guards => format!("{name}[0].text"),
        })
    }

    /// A variable in scope that `pick` takes, chosen at random and counted as read.
    fn var_of(&mut self, pick: impl Fn(&Var) -> bool) -> Option<(String, Kind)> {
        let found: Vec<(String, Kind)> = self.scopes.iter().flatten().filter(|var| pick(var)).map(|var| (var.name.clone(), var.kind)).collect();
        if found.is_empty() {
            return None;
        }
        let (name, kind) = self.rng.pick(&found).clone();
        self.set_read(&name, true);
        Some((name, kind))
    }

    fn set_read(&mut self, name: &str, read: bool) {
        for var in self.scopes.iter_mut().flatten().filter(|var| var.name == name) {
            var.read = read;
        }
    }


    fn kind_in_scope(&mut self, kinds: &[Kind]) -> Option<(String, Kind)> {
        self.var_of(|var| kinds.contains(&var.kind))
    }

    fn declare(&mut self, prefix: &str, kind: Kind) -> String {
        let name = self.name(prefix);
        self.scopes.last_mut().unwrap().push(Var { name: name.clone(), kind, mutable: true, read: false, param: None });
        name
    }

    /// A text literal long enough for the heap, each one of its own.
    fn long_text(&mut self) -> String {
        self.texts += 1;
        let word = *self.rng.pick(&["alpha", "bravo", "charlie", "delta", "echo"]);
        // A variable is chosen only where it is written: chosen is read.
        match self.rng.chance(30).then(|| self.var_of(|var| var.kind == Kind::Int)).flatten() {
            Some((int, _)) => format!("$\"{word} text {}, long enough for the heap {{{int}}}\".toString()", self.texts),
            None => format!("\"{word} text {}, long enough for the heap\".clone()", self.texts),
        }
    }

    fn texts_literal(&mut self) -> String {
        let items: Vec<String> = (0..1 + self.rng.below(2)).map(|_| self.long_text()).collect();
        format!("List<string> {{ {} }}", items.join(", "))
    }

    // --- What borrows ---

    /// A `read string` taken from something in scope.
    fn view(&mut self) -> Option<String> {
        let all = [Kind::Text, Kind::List, Kind::Shared, Kind::View, Kind::Word, Kind::Holder, Kind::Words, Kind::Shelf, Kind::Guard, Kind::RefList, Kind::RefText];
        let (name, kind) = self.kind_in_scope(&all)?;
        Some(match kind {
            Kind::Text | Kind::RefText => match self.rng.below(6) {
                0 => format!("wrap(read {name}).text"),
                _ => format!("read {name}"),
            },
            Kind::List => match self.rng.below(5) {
                0 => format!("first(read {name})"),
                _ => format!("read {name}[0]"),
            },
            Kind::RefList => match self.rng.below(5) {
                0 => format!("first({name})"),
                _ => format!("read {name}[0]"),
            },
            Kind::Shared => format!("read {name}[0]"),
            Kind::View => name,
            Kind::Word | Kind::Guard => format!("{name}.text"),
            Kind::Holder => format!("read {name}.items[0]"),
            Kind::Words => format!("{name}[0].text"),
            Kind::Shelf => format!("{name}.words[0].text"),
            _ => unreachable!(),
        })
    }

    /// A list given to a `ref List<string>` parameter.
    fn ref_list(&mut self) -> Option<String> {
        let (name, kind) = self.kind_in_scope(&[Kind::List, Kind::RefList, Kind::Holder, Kind::Sweeper])?;
        Some(match kind {
            Kind::List => format!("ref {name}"),
            Kind::RefList if self.rng.chance(50) => name,
            Kind::RefList => format!("ref {name}"),
            _ if self.rng.chance(30) => format!("{name}.items"),
            _ => format!("ref {name}.items"),
        })
    }

    /// A list given to a `read List<string>` parameter.
    fn read_list(&mut self) -> Option<String> {
        let (name, kind) = self.kind_in_scope(&[Kind::List, Kind::RefList, Kind::Holder, Kind::Shared])?;
        Some(match kind {
            Kind::List | Kind::Shared => format!("read {name}"),
            Kind::RefList if self.rng.chance(50) => name,
            Kind::RefList => format!("read {name}"),
            _ => format!("read {name}.items"),
        })
    }

    /// An `int` for a field: a number, or a call that changes a list.
    fn int_part(&mut self) -> String {
        match self.rng.below(4) {
            0 => match self.ref_list() {
                Some(list) => format!("g({list})"),
                None => "1".to_string(),
            },
            1 => match self.read_list() {
                Some(list) => format!("(int)count({list})"),
                None => "2".to_string(),
            },
            _ => self.rng.below(10).to_string(),
        }
    }

    fn word(&mut self) -> Option<String> {
        // The parts in either order: what lends and what changes, both ways round.
        let text = self.view()?;
        let n = self.int_part();
        Some(match self.rng.chance(50) {
            true => format!("Word {{ text = {text}, n = {n} }}"),
            false => format!("Word {{ n = {n}, text = {text} }}"),
        })
    }

    // --- Statements ---

    fn statements(&mut self, depth: usize, count: u64) -> Vec<Node> {
        let mut nodes = Vec::new();
        for _ in 0..count {
            if let Some(node) = self.statement(depth) {
                nodes.push(node);
            }
        }
        nodes
    }

    fn block(&mut self, depth: usize) -> Vec<Node> {
        self.scopes.push(Vec::new());
        let count = 1 + self.rng.below(4);
        let mut body = self.statements(depth, count);
        body.extend(self.close_scope());
        body
    }

    fn statement(&mut self, depth: usize) -> Option<Node> {
        let nested = depth < 3;
        for _ in 0..8 {
            // A statement not written reads nothing: what it chose is put back.
            let read: Vec<bool> = self.scopes.iter().flatten().map(|var| var.read).collect();
            let node = match self.rng.below(100) {
                0..=17 => self.own(),
                18..=35 => self.lend(),
                36..=53 => self.change(),
                54..=69 => self.use_(),
                70..=75 if nested => self.if_stmt(depth),
                76..=81 if nested && self.loops < 2 => self.for_range(depth),
                82..=85 if nested && self.loops < 2 => self.while_stmt(depth),
                86..=89 if nested && self.loops < 2 => self.for_items(depth),
                90..=92 if nested => self.match_stmt(depth),
                93..=95 if self.loops > 0 => self.leave(),
                96..=97 if nested => Some(Node::block("if true {".to_string(), self.block(depth + 1))),
                98..=99 => self.call_helper(),
                _ => None,
            };
            if node.is_some() {
                return node;
            }
            for (var, read) in self.scopes.iter_mut().flatten().zip(read) {
                var.read = read;
            }
        }
        None
    }

    /// A new owner: a text, a list, a `shared` list.
    fn own(&mut self) -> Option<Node> {
        Some(match self.rng.below(5) {
            0 | 1 => {
                let value = self.long_text();
                let name = self.declare("t", Kind::Text);
                Node::line(format!("var {name} = {value}"))
            }
            2 | 3 => {
                let value = self.texts_literal();
                let name = self.declare("x", Kind::List);
                Node::line(format!("var {name} = {value}"))
            }
            _ => {
                let value = self.texts_literal();
                let name = self.declare("a", Kind::Shared);
                Node::line(format!("var {name} = new shared {value}"))
            }
        })
    }

    /// A new borrow, or a new value of one.
    fn lend(&mut self) -> Option<Node> {
        Some(match self.rng.below(12) {
            0..=2 => {
                let value = self.view()?;
                let name = self.declare("r", Kind::View);
                Node::line(format!("var {name} = {value}"))
            }
            3..=4 => {
                let (name, _) = self.var_of(|var| var.mutable && var.kind == Kind::View)?;
                let value = self.view()?;
                Node::line(format!("{name} = {value}"))
            }
            5 => {
                let value = self.word()?;
                let name = self.declare("w", Kind::Word);
                Node::line(format!("var {name} = {value}"))
            }
            6 => {
                let list = self.ref_list()?;
                let list = if list.starts_with("ref ") { list } else { format!("ref {list}") };
                let (prefix, kind, ty, field) = *self.rng.pick(&[("h", Kind::Holder, "Holder", "items"), ("h", Kind::Holder, "Holder", "items"), ("z", Kind::Sweeper, "Sweeper", "items")]);
                let name = self.declare(prefix, kind);
                Node::line(format!("var {name} = {ty} {{ {field} = {list} }}"))
            }
            7 => {
                let items: Vec<String> = (0..1 + self.rng.below(2)).filter_map(|_| self.word()).collect();
                if items.is_empty() {
                    return None;
                }
                let name = self.declare("ws", Kind::Words);
                Node::line(format!("var {name} = List<Word> {{ {} }}", items.join(", ")))
            }
            8 => {
                let (name, kind) = self.kind_in_scope(&[Kind::Words, Kind::Shelf])?;
                let word = self.word()?;
                match kind {
                    Kind::Words => Node::line(format!("{name}.add({word})")),
                    _ => Node::line(format!("{name}.words.add({word})")),
                }
            }
            9 if self.rng.chance(50) => {
                // A `ref` kept in a variable: of a list, or copied out of a holder's field.
                let (from, kind) = self.kind_in_scope(&[Kind::List, Kind::Holder, Kind::RefList])?;
                let value = match kind {
                    Kind::List => format!("ref {from}"),
                    Kind::Holder => format!("{from}.items"),
                    _ => from,
                };
                let name = self.name("p");
                self.scopes.last_mut().unwrap().push(Var { name: name.clone(), kind: Kind::RefList, mutable: false, read: false, param: None });
                Node::line(format!("var {name} = {value}"))
            }
            9 => {
                let (words, _) = self.var_of(|var| var.kind == Kind::Words)?;
                let name = self.declare("sh", Kind::Shelf);
                Node::line(format!("var {name} = Shelf {{ words = ref {words} }}"))
            }
            10 => {
                let text = self.view()?;
                let name = self.declare("gd", Kind::Guard);
                Node::line(format!("var {name} = Guard {{ text = {text} }}"))
            }
            _ => {
                let text = self.view()?;
                let name = self.declare("gs", Kind::Guards);
                match self.rng.chance(50) {
                    true => Node::line(format!("var {name} = List<Guard> {{ Guard {{ text = {text} }} }}")),
                    false => Node::line(format!("var {name} = Guard[1] {{ Guard {{ text = {text} }} }}")),
                }
            }
        })
    }

    /// A change of an owner, which frees what it held: a new value, a list made new, a move.
    fn change(&mut self) -> Option<Node> {
        let before: Vec<String> = self.scopes.iter().flatten().filter(|var| var.read).map(|var| var.name.clone()).collect();
        let (name, kind) = self.var_of(|var| matches!(var.kind, Kind::Text | Kind::List | Kind::Shared | Kind::RefList | Kind::RefText | Kind::Holder | Kind::Words | Kind::Shelf))?;
        let mutable = self.scopes.iter().flatten().any(|var| var.name == name && var.mutable);
        // A new value given to it is not a read of it.
        let assigned = |this: &mut Self| this.set_read(&name, before.contains(&name));
        Some(match kind {
            Kind::Text | Kind::RefText => match self.rng.below(3) {
                0 if kind == Kind::RefText => Node::line(format!("renew({name})")),
                0 | 1 => Node::line(format!("renew(ref {name})")),
                _ if kind == Kind::Text && !mutable => return None,
                _ => {
                    assigned(self);
                    let value = self.long_text();
                    Node::line(format!("{name} = {value}"))
                }
            },
            Kind::List | Kind::RefList => {
                let given = if kind == Kind::List { format!("ref {name}") } else { name.clone() };
                match self.rng.below(7) {
                    0 => Node::line(format!("refill({given})")),
                    1 => Node::line(format!("_ = g({given})")),
                    2 => {
                        let value = self.long_text();
                        Node::line(format!("{name}[0] = {value}"))
                    }
                    3 => {
                        let value = self.long_text();
                        Node::line(format!("{name}.add({value})"))
                    }
                    4 if kind == Kind::List && mutable => {
                        assigned(self);
                        let value = self.texts_literal();
                        Node::line(format!("{name} = {value}"))
                    }
                    5 if kind == Kind::List && self.loops == 0 => {
                        self.forget(&name);
                        match self.rng.chance(50) {
                            true => Node::line(format!("take({name})")),
                            false => {
                                let moved = self.name("m");
                                Node::block("if true {".to_string(), vec![Node::line(format!("var {moved} = {name}")), Node::line(format!("print({moved})"))])
                            }
                        }
                    }
                    _ => Node::line(format!("refill(ref {name})")),
                }
            }
            // What a `shared` value holds is only read: it changes by a new value.
            Kind::Shared if mutable => {
                assigned(self);
                let value = self.texts_literal();
                Node::line(format!("{name} = new shared {value}"))
            }
            Kind::Holder => match self.rng.below(3) {
                0 => Node::line(format!("wipeHolder(ref {name})")),
                1 => Node::line(format!("refill({name}.items)")),
                _ => {
                    let value = self.long_text();
                    Node::line(format!("{name}.items[0] = {value}"))
                }
            },
            Kind::Words | Kind::Shelf => {
                let place = if kind == Kind::Words { name.clone() } else { format!("{name}.words") };
                let word = self.word()?;
                match self.rng.chance(50) {
                    true if kind == Kind::Words && mutable => {
                        assigned(self);
                        Node::line(format!("{place} = List<Word> {{ {word} }}"))
                    }
                    _ => Node::line(format!("{place}[0] = {word}")),
                }
            }
            _ => return None,
        })
    }

    /// A move takes the variable away: nothing after it names it.
    fn forget(&mut self, name: &str) {
        for scope in &mut self.scopes {
            scope.retain(|var| var.name != name);
        }
    }

    /// A read of what something holds or borrows.
    fn use_(&mut self) -> Option<Node> {
        if self.rng.chance(15) {
            if let Some(list) = self.read_list() {
                return Some(Node::line(format!("print(count({list}))")));
            }
        }
        let (name, kind) = self.var_of(|var| var.kind != Kind::Int)?;
        if matches!(kind, Kind::Text | Kind::View | Kind::RefText) && self.rng.chance(30) {
            return Some(Node::line(format!("print({name}.length())")));
        }
        let shown = self.show(&name, kind)?;
        Some(Node::line(format!("print({shown})")))
    }

    fn condition(&mut self) -> String {
        if self.rng.chance(60) {
            if let Some((int, _)) = self.var_of(|var| var.kind == Kind::Int) {
                let n = self.rng.below(3);
                return format!("{int} == {n}");
            }
        }
        if self.rng.chance(50) {
            if let Some(list) = self.read_list() {
                return format!("count({list}) > 1");
            }
        }
        match self.kind_in_scope(&[Kind::Text, Kind::View]) {
            Some((name, _)) => format!("{name}.length() > {}", 40 + self.rng.below(20)),
            None => "true".to_string(),
        }
    }

    fn if_stmt(&mut self, depth: usize) -> Option<Node> {
        let cond = self.condition();
        let then = self.block(depth + 1);
        Some(match self.rng.chance(50) {
            true => {
                let other = self.block(depth + 1);
                Node { parts: vec![(format!("if {cond} {{"), then), ("} else {".to_string(), other)], end: Some("}".to_string()) }
            }
            false => Node::block(format!("if {cond} {{"), then),
        })
    }

    /// `if cond { ...; break }` or `continue`, often with a borrow taken just before.
    fn leave(&mut self) -> Option<Node> {
        let cond = self.condition();
        let word = if self.rng.chance(50) { "break" } else { "continue" };
        self.scopes.push(Vec::new());
        let mut body = Vec::new();
        if let Some(node) = if self.rng.chance(50) { self.lend() } else { self.statement(3) } {
            body.push(node);
        }
        // What the block declared and nothing read is read before the way out.
        for var in self.scopes.pop().unwrap_or_default() {
            if !var.read {
                if let Some(shown) = self.show(&var.name, var.kind) {
                    body.push(Node::line(format!("print({shown})")));
                }
            }
        }
        body.push(Node::line(word.to_string()));
        Some(Node::block(format!("if {cond} {{"), body))
    }

    fn in_loop(&mut self, depth: usize, scope: Vec<Var>) -> Vec<Node> {
        self.loops += 1;
        self.scopes.push(scope);
        let count = 1 + self.rng.below(5);
        let mut body = self.statements(depth + 1, count);
        body.extend(self.close_scope());
        self.loops -= 1;
        body
    }

    fn for_range(&mut self, depth: usize) -> Option<Node> {
        let name = self.name("i");
        let end = 1 + self.rng.below(3);
        let body = self.in_loop(depth, vec![Var { name: name.clone(), kind: Kind::Int, mutable: false, read: false, param: None }]);
        Some(Node::block(format!("for {name} in 0..{end} {{"), body))
    }

    /// `var w = 0` and `while next(ref w) <= n`, counted in the condition, so that neither
    /// `continue` nor taking a statement away (`fit`, the reducer) leaves it without an end; and
    /// sometimes a condition that changes a list (`grow`), the count still bounding it.
    fn while_stmt(&mut self, depth: usize) -> Option<Node> {
        let name = self.name("n");
        let rounds = 1 + self.rng.below(3);
        let cond = match self.rng.chance(50).then(|| self.ref_list()).flatten() {
            Some(list) => format!("next(ref {name}) <= {rounds} && grow({list}) < 9"),
            None => format!("next(ref {name}) <= {rounds}"),
        };
        let body = self.in_loop(depth, Vec::new());
        Some(Node { parts: vec![(format!("var {name} = 0"), Vec::new()), (format!("while {cond} {{"), body)], end: Some("}".to_string()) })
    }

    /// `for x in read xs`: over a list, a `shared` list, a list of words.
    fn for_items(&mut self, depth: usize) -> Option<Node> {
        let (list, kind) = self.kind_in_scope(&[Kind::List, Kind::Shared, Kind::RefList, Kind::Words, Kind::Holder, Kind::Guards])?;
        let name = self.name("e");
        let (over, item) = match kind {
            Kind::Holder => (format!("read {list}.items"), Kind::View),
            Kind::Words => (format!("read {list}"), Kind::Word),
            Kind::Guards => (format!("read {list}"), Kind::Guard),
            _ => (format!("read {list}"), Kind::View),
        };
        let body = self.in_loop(depth, vec![Var { name: name.clone(), kind: item, mutable: false, read: false, param: None }]);
        Some(Node::block(format!("for {name} in {over} {{"), body))
    }

    fn match_stmt(&mut self, depth: usize) -> Option<Node> {
        let subject = match self.rng.chance(60).then(|| self.var_of(|var| var.kind == Kind::Int)).flatten() {
            Some((int, _)) => int,
            None => format!("count({})", self.read_list()?),
        };
        let mut arms = Vec::new();
        for n in 0..1 + self.rng.below(2) {
            let body = self.block(depth + 1);
            arms.push(Node::block(format!("{n} => {{"), body));
        }
        let body = self.block(depth + 1);
        arms.push(Node::block("_ => {".to_string(), body));
        Some(Node::block(format!("match {subject} {{"), arms))
    }

    fn call_helper(&mut self) -> Option<Node> {
        if self.helpers.is_empty() {
            return None;
        }
        let at = self.rng.below(self.helpers.len() as u64) as usize;
        let params = self.helpers[at].params.clone();
        let mut args = Vec::new();
        for ty in params {
            let arg = match ty {
                "ref List<string>" => self.ref_list()?,
                "ref string" => {
                    let (name, kind) = self.kind_in_scope(&[Kind::Text, Kind::RefText])?;
                    match kind {
                        Kind::RefText if self.rng.chance(50) => name,
                        _ => format!("ref {name}"),
                    }
                }
                // A holder lent, or a parameter given on as it is: a `read Holder` only to `read`,
                // a `ref Holder` to either.
                "read Holder" | "ref Holder" => {
                    let word = &ty[..ty.find(' ').unwrap()];
                    let (name, _) = self.var_of(|var| var.kind == Kind::Holder && (word == "read" || var.param != Some("read Holder")))?;
                    let param = self.scopes.iter().flatten().find(|var| var.name == name).and_then(|var| var.param);
                    match param {
                        Some(_) if self.rng.chance(50) => name,
                        Some("read Holder") => name,
                        _ => format!("{word} {name}"),
                    }
                }
                _ => self.view()?,
            };
            args.push(arg);
        }
        Some(Node::line(format!("{}({})", self.helpers[at].name, args.join(", "))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen::{render, render_mapped};

    #[test]
    fn a_seed_gives_one_program() {
        for seed in 0..50 {
            assert_eq!(render(&Borrow::new(seed).program()), render(&Borrow::new(seed).program()));
        }
        assert_ne!(render(&Borrow::new(1).program()), render(&Borrow::new(2).program()));
    }

    /// `fit` takes away the node a line names: every line has one, the prelude's are the first
    /// nodes, and the line of a statement is the one that node writes.
    #[test]
    fn every_line_names_the_node_that_wrote_it() {
        for seed in 0..50 {
            let nodes = Borrow::new(seed).program();
            let (text, lines) = render_mapped(&nodes);
            assert_eq!(text, render(&nodes));
            assert_eq!(text.lines().count(), lines.len());
            assert_eq!(lines[0], Some((Vec::new(), 0)));
            let main = text.lines().position(|line| line == "void main() {").unwrap();
            assert_eq!(lines[main], Some((Vec::new(), nodes.len() - 1)));
            for (at, line) in text.lines().enumerate() {
                if let Some((way, item)) = &lines[at] {
                    let mut list = &nodes;
                    for &(node, part) in way {
                        list = &list[node].parts[part].1;
                    }
                    let node = &list[*item];
                    let written = node.parts.iter().any(|(head, _)| head.lines().any(|head| line.trim() == head.trim())) || node.end.as_deref() == Some(line.trim());
                    assert!(written, "seed {seed}, line {}: {line}", at + 1);
                } else {
                    assert!(line.is_empty());
                }
            }
        }
    }
}
