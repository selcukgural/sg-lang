//! The tree of the filter turned into the tree the machine walks: each name bound to what it means
//! (a variable some frames up, a parameter, a definition, a built-in), the forms jq defines in its
//! own language desugared, and the built-ins jq writes in jq read from jq's own definitions.

use crate::builtins::Native;
use crate::single::is_single;
use crate::parser::{Assign, Ast, Def, Op, Piece, parse};
use crate::value::{KIND_ARRAY, KIND_BOOLEAN, KIND_NULL, KIND_NUMBER, KIND_OBJECT, KIND_STRING, Key, Map, Value};

/// The built-ins jq 1.8.2 defines in jq (from its `builtin.jq`), those of them this jq has.
const PRELUDE: &str = r#"
def sort_by(f): _sort_by_impl(map([f]));
def group_by(f): _group_by_impl(map([f]));
def unique_by(f): _unique_by_impl(map([f]));
def max_by(f): _max_by_impl(map([f]));
def min_by(f): _min_by_impl(map([f]));
def add(f): reduce f as $x (null; . + $x);
def add: add(.[]);
def del(f): delpaths([path(f)]);
def map_values(f): .[] |= f;
def recurse(f): def r: ., (f | r); r;
def to_entries: [keys_unsorted[] as $k | {key: $k, value: .[$k]}];
def from_entries: map({ (.key // .Key // .name // .Name):
  if has("value") then .value else .Value end }) | add // {};
def with_entries(f): to_entries | map(f) | from_entries;
def reverse: [.[length - 1 - range(0;length)]];
def paths: path(..)|select(length > 0);
def join($x): reduce .[] as $i (null;
            (if .==null then "" else .+$x end) +
            ($i | if type=="boolean" or type=="number" then tostring else .//"" end)
        ) // "";
def range($x): range(0;$x);
def ltrimstr($left): if startswith($left) then .[$left | length:] end;
def rtrimstr($right): if endswith($right) then .[:length - ($right | length)] end;
def test(re; mode): _match_impl(re; mode; true);
def test($val): ($val|type) as $vt | if $vt == "string" then test($val; null)
   elif $vt == "array" and ($val | length) > 1 then test($val[0]; $val[1])
   elif $vt == "array" and ($val | length) > 0 then test($val[0]; null)
   else error( $vt + " not a string or array") end;
def _while(cond; update):
     def _while:
         if cond then ., (update | _while) else empty end;
     _while;
def range($init; $upto; $by):
    if $by > 0 then $init|_while(. < $upto; . + $by)
  elif $by < 0 then $init|_while(. > $upto; . + $by)
  else empty end;
def all(generator; condition): isempty(generator|condition and empty);
def any(generator; condition): isempty(generator|condition or empty)|not;
def all(condition): all(.[]; condition);
def any(condition): any(.[]; condition);
def all: all(.[]; .);
def any: any(.[]; .);
def first: .[0];
def last: .[-1];
def in(xs): . as $x | xs | has($x);
.
"#;

/// A node of the tree the machine walks.
pub enum Ir {
    Identity,
    /// `..`
    Recurse,
    Literal(Value),
    /// `t.name`, `t["name"]`: a key known before. The last field of an access is whether it is
    /// optional (`t.name?`): its own failure then gives no output.
    Field(Box<Ir>, Value, bool),
    Index(Box<Ir>, Box<Ir>, bool),
    Slice(Box<Ir>, Box<Ir>, Box<Ir>, bool),
    Iterate(Box<Ir>, bool),
    /// `t?`
    Try(Box<Ir>),
    /// `[f]`
    Collect(Box<Ir>),
    /// A node of a single output, worked out at once (`single.rs`).
    Single(Box<Ir>),
    /// `f | @format`, a piece of `"\(f)"`.
    Format(Native, Box<Ir>),
    Object(Vec<(Ir, Ir)>),
    Neg(Box<Ir>),
    Pipe(Box<Ir>, Box<Ir>),
    Comma(Box<Ir>, Box<Ir>),
    Alternative(Box<Ir>, Box<Ir>),
    And(Box<Ir>, Box<Ir>),
    Or(Box<Ir>, Box<Ir>),
    Binary(Op, Box<Ir>, Box<Ir>),
    If(Box<Ir>, Box<Ir>, Box<Ir>),
    /// `reduce source as $x (init; update)`: `update` sees `$x` one frame up.
    Reduce(Box<Ir>, Box<Ir>, Box<Ir>),
    Foreach(Box<Ir>, Box<Ir>, Box<Ir>, Option<Box<Ir>>),
    /// `source as $x | body`
    As(Box<Ir>, Box<Ir>),
    /// A variable this many frames up.
    Var(usize),
    /// A call of a definition, which was made this many frames up.
    CallDef { def: usize, up: usize, args: Vec<Ir> },
    /// A call of a filter parameter: the frame this many up holds it at `index`.
    CallParam { up: usize, index: usize },
    Empty,
    Error(Option<Box<Ir>>),
    /// The input when its kind is one of these bits (`numbers`, `strings`, ...).
    TypeFilter(u8),
    /// A built-in taking values: each argument's outputs, the last argument's outermost.
    Native(Native, Vec<Ir>),
    Path(Box<Ir>),
    GetPath(Box<Ir>),
    Limit(Box<Ir>, Box<Ir>),
    First(Box<Ir>),
    Last(Box<Ir>),
    IsEmpty(Box<Ir>),
    Range(Box<Ir>, Box<Ir>),
    Input,
    Inputs,
    /// `lhs = rhs`
    Assign(Box<Ir>, Box<Ir>),
    /// `lhs |= f`
    Update(Box<Ir>, Box<Ir>),
    /// `lhs op= rhs`
    ArithmeticUpdate(Op, Box<Ir>, Box<Ir>),
    /// `lhs //= rhs`
    AlternativeUpdate(Box<Ir>, Box<Ir>),
}

/// A definition: its body (its parameters are the frame the call makes).
pub struct DefIr {
    pub body: Ir,
}

enum Entry {
    Var { name: String, frame: usize },
    Params { names: Vec<String>, frame: usize },
    Def { name: String, arity: usize, index: usize, frame: usize },
}

struct Compiler {
    scope: Vec<Entry>,
    depth: usize,
    defs: Vec<DefIr>,
}

pub type Result<T> = std::result::Result<T, String>;

/// The filter compiled, with the variables given (`--arg`) as the outermost frames, in order.
pub fn compile(filter: &str, vars: &[String]) -> Result<(Ir, Vec<DefIr>)> {
    let mut compiler = Compiler { scope: Vec::new(), depth: 0, defs: Vec::new() };
    let mut prelude = parse(PRELUDE).map_err(|e| format!("in the prelude: {e}"))?;
    while let Ast::Def(def, rest) = prelude {
        compiler.define(*def)?;
        prelude = *rest;
    }
    for name in vars {
        compiler.depth += 1;
        compiler.scope.push(Entry::Var { name: name.clone(), frame: compiler.depth });
    }
    let ast = parse(filter)?;
    let ir = compiler.compile(ast)?;
    Ok((ir, compiler.defs))
}

fn boxed(ir: Ir) -> Box<Ir> {
    Box::new(ir)
}

/// A node of a single output marked to be worked out at once (the simplest need no mark).
fn single(ir: Ir) -> Ir {
    match ir {
        Ir::Identity | Ir::Literal(_) | Ir::Var(_) | Ir::Single(_) => ir,
        ir if is_single(&ir) => Ir::Single(boxed(ir)),
        ir => ir,
    }
}

impl Compiler {
    /// Compiles a definition and leaves it in scope.
    fn define(&mut self, def: Def) -> Result<()> {
        let index = self.defs.len();
        self.defs.push(DefIr { body: Ir::Empty });
        self.scope.push(Entry::Def { name: def.name, arity: def.params.len(), index, frame: self.depth });
        let saved = (self.depth, self.scope.len());
        // `def f($a): body` is `def f(a): a as $a | body`.
        let mut body = def.body;
        for (name, is_value) in def.params.iter().rev() {
            if *is_value {
                body = Ast::As(Box::new(Ast::Call(name.clone(), Vec::new())), name.clone(), Box::new(body));
            }
        }
        if !def.params.is_empty() {
            self.depth += 1;
            let names = def.params.iter().map(|(n, _)| n.clone()).collect();
            self.scope.push(Entry::Params { names, frame: self.depth });
        }
        let body = self.compile(body)?;
        self.scope.truncate(saved.1);
        self.depth = saved.0;
        self.defs[index].body = body;
        Ok(())
    }

    fn with_var<T>(&mut self, name: String, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.depth += 1;
        self.scope.push(Entry::Var { name, frame: self.depth });
        let result = f(self);
        self.scope.pop();
        self.depth -= 1;
        result
    }

    fn compile(&mut self, ast: Ast) -> Result<Ir> {
        self.compile_node(ast).map(single)
    }

    fn compile_node(&mut self, ast: Ast) -> Result<Ir> {
        Ok(match ast {
            Ast::Identity => Ir::Identity,
            Ast::RecurseAll => self.call("recurse".into(), Vec::new())?,
            Ast::Literal(v) => Ir::Literal(v),
            Ast::Interpolation(format, pieces) => {
                let format = format.unwrap_or_else(|| "text".into());
                let native = Native::format(&format).ok_or_else(|| format!("{format} is not a valid format"))?;
                let mut result: Option<Ir> = None;
                for piece in pieces {
                    let ir = match piece {
                        Piece::Text(text) => Ir::Literal(Value::string(text)),
                        Piece::Value(ast) => single(Ir::Format(native, boxed(self.compile(ast)?))),
                    };
                    result = Some(match result {
                        None => ir,
                        Some(left) => Ir::Binary(Op::Add, boxed(left), boxed(ir)),
                    });
                }
                let result = result.unwrap_or(Ir::Literal(Value::string("")));
                match result {
                    // A text made only of one interpolation is still a text.
                    Ir::Format(..) | Ir::Single(_) => {
                        single(Ir::Binary(Op::Add, boxed(Ir::Literal(Value::string(""))), boxed(result)))
                    }
                    other => other,
                }
            }
            Ast::Format(name) => Ir::Native(Native::format(&name).ok_or_else(|| format!("{name} is not a valid format"))?, Vec::new()),
            Ast::Field(target, name) => Ir::Field(boxed(self.compile(*target)?), Value::string(name), false),
            Ast::Index(target, key) => {
                let target = self.compile(*target)?;
                let key = self.compile(*key)?;
                match key {
                    Ir::Literal(k) => Ir::Field(boxed(target), k, false),
                    key => Ir::Index(boxed(target), boxed(key), false),
                }
            }
            Ast::Optional(access) => match self.compile_node(*access)? {
                Ir::Field(t, k, _) => Ir::Field(t, k, true),
                Ir::Index(t, k, _) => Ir::Index(t, k, true),
                Ir::Slice(t, f, e, _) => Ir::Slice(t, f, e, true),
                Ir::Iterate(t, _) => Ir::Iterate(t, true),
                other => Ir::Try(boxed(other)),
            },
            Ast::Slice(target, from, to) => {
                let target = self.compile(*target)?;
                let from = match from {
                    Some(f) => self.compile(*f)?,
                    None => Ir::Literal(Value::Null),
                };
                let to = match to {
                    Some(t) => self.compile(*t)?,
                    None => Ir::Literal(Value::Null),
                };
                Ir::Slice(boxed(target), boxed(from), boxed(to), false)
            }
            Ast::Iterate(target) => Ir::Iterate(boxed(self.compile(*target)?), false),
            Ast::Try(body) => Ir::Try(boxed(self.compile(*body)?)),
            Ast::Array(None) => Ir::Literal(Value::array(Vec::new())),
            Ast::Array(Some(body)) => Ir::Collect(boxed(self.compile(*body)?)),
            Ast::Object(entries) => {
                let mut out = Vec::with_capacity(entries.len());
                for (k, v) in entries {
                    // A key that is a constant, as jq folds them, and not a text is refused here.
                    if let Some(v) = constant_of(&k)
                        && !matches!(v, Value::String(_)) {
                            return Err(format!("Cannot use {} as object key", v.type_name()));
                        }
                    let key = self.compile(k)?;
                    out.push((key, self.compile(v)?));
                }
                Ir::Object(out)
            }
            Ast::Neg(a) => Ir::Neg(boxed(self.compile(*a)?)),
            Ast::Pipe(a, b) => Ir::Pipe(boxed(self.compile(*a)?), boxed(self.compile(*b)?)),
            Ast::Comma(a, b) => Ir::Comma(boxed(self.compile(*a)?), boxed(self.compile(*b)?)),
            Ast::Alternative(a, b) => Ir::Alternative(boxed(self.compile(*a)?), boxed(self.compile(*b)?)),
            Ast::And(a, b) => Ir::And(boxed(self.compile(*a)?), boxed(self.compile(*b)?)),
            Ast::Or(a, b) => Ir::Or(boxed(self.compile(*a)?), boxed(self.compile(*b)?)),
            Ast::Binary(op, a, b) => Ir::Binary(op, boxed(self.compile(*a)?), boxed(self.compile(*b)?)),
            Ast::Assign(kind, lhs, rhs) => {
                let lhs = boxed(self.compile(*lhs)?);
                let rhs = boxed(self.compile(*rhs)?);
                match kind {
                    Assign::Set => Ir::Assign(lhs, rhs),
                    Assign::Update => Ir::Update(lhs, rhs),
                    Assign::Arithmetic(op) => Ir::ArithmeticUpdate(op, lhs, rhs),
                    Assign::Alternative => Ir::AlternativeUpdate(lhs, rhs),
                }
            }
            Ast::If(c, t, e) => Ir::If(boxed(self.compile(*c)?), boxed(self.compile(*t)?), boxed(self.compile(*e)?)),
            Ast::Reduce(source, name, init, update) => {
                let source = self.compile(*source)?;
                let init = self.compile(*init)?;
                let update = self.with_var(name, |c| c.compile(*update))?;
                Ir::Reduce(boxed(source), boxed(init), boxed(update))
            }
            Ast::Foreach(source, name, init, update, extract) => {
                let source = self.compile(*source)?;
                let init = self.compile(*init)?;
                let (update, extract) = self.with_var(name, |c| {
                    let update = c.compile(*update)?;
                    let extract = match extract {
                        Some(e) => Some(boxed(c.compile(*e)?)),
                        None => None,
                    };
                    Ok((update, extract))
                })?;
                Ir::Foreach(boxed(source), boxed(init), boxed(update), extract)
            }
            Ast::As(source, name, body) => {
                let source = self.compile(*source)?;
                let body = self.with_var(name, |c| c.compile(*body))?;
                Ir::As(boxed(source), boxed(body))
            }
            Ast::Var(name) => {
                for entry in self.scope.iter().rev() {
                    if let Entry::Var { name: n, frame } = entry
                        && *n == name {
                            return Ok(Ir::Var(self.depth - frame));
                        }
                }
                return Err(format!("${name} is not defined"));
            }
            Ast::Call(name, args) => self.call(name, args)?,
            Ast::Def(def, rest) => {
                let mark = self.scope.len();
                self.define(*def)?;
                let rest = self.compile(*rest);
                self.scope.truncate(mark);
                rest?
            }
        })
    }

    /// A call: a definition or a parameter in scope, else a built-in.
    fn call(&mut self, name: String, args: Vec<Ast>) -> Result<Ir> {
        for entry in self.scope.iter().rev() {
            match entry {
                Entry::Def { name: n, arity, index, frame } if *n == name && *arity == args.len() => {
                    let (def, up) = (*index, self.depth - frame);
                    let args = args.into_iter().map(|a| self.compile(a)).collect::<Result<Vec<_>>>()?;
                    return Ok(Ir::CallDef { def, up, args });
                }
                Entry::Params { names, frame } if args.is_empty() => {
                    if let Some(index) = names.iter().position(|n| *n == name) {
                        return Ok(Ir::CallParam { up: self.depth - frame, index });
                    }
                }
                _ => {}
            }
        }
        let arity = args.len();
        let mut args = args.into_iter().map(|a| self.compile(a)).collect::<Result<Vec<_>>>()?;
        let mut arg = || boxed(args.remove(0));
        let types = |mask| Ir::TypeFilter(mask);
        Ok(match (name.as_str(), arity) {
            ("empty", 0) => Ir::Empty,
            ("error", 0) => Ir::Error(None),
            ("error", 1) => Ir::Error(Some(arg())),
            ("select", 1) => Ir::If(arg(), boxed(Ir::Identity), boxed(Ir::Empty)),
            ("map", 1) => Ir::Collect(boxed(Ir::Pipe(boxed(Ir::Iterate(boxed(Ir::Identity), false)), arg()))),
            ("infinite", 0) => Ir::Literal(Value::Number(f64::INFINITY)),
            ("nan", 0) => Ir::Literal(Value::Number(f64::NAN)),
            ("recurse", 0) => Ir::Recurse,
            ("values", 0) => types(!KIND_NULL),
            ("nulls", 0) => types(KIND_NULL),
            ("booleans", 0) => types(KIND_BOOLEAN),
            ("numbers", 0) => types(KIND_NUMBER),
            ("strings", 0) => types(KIND_STRING),
            ("arrays", 0) => types(KIND_ARRAY),
            ("objects", 0) => types(KIND_OBJECT),
            ("iterables", 0) => types(KIND_ARRAY | KIND_OBJECT),
            ("scalars", 0) => types(KIND_NULL | KIND_BOOLEAN | KIND_NUMBER | KIND_STRING),
            ("path", 1) => Ir::Path(arg()),
            ("getpath", 1) => Ir::GetPath(arg()),
            ("limit", 2) => {
                let n = arg();
                Ir::Limit(n, arg())
            }
            ("first", 1) => Ir::First(arg()),
            ("last", 1) => Ir::Last(arg()),
            ("isempty", 1) => Ir::IsEmpty(arg()),
            ("range", 2) => {
                let from = arg();
                Ir::Range(from, arg())
            }
            ("input", 0) => Ir::Input,
            ("inputs", 0) => Ir::Inputs,
            _ => match Native::find(&name, arity) {
                Some(native) => Ir::Native(native, args),
                None => return Err(format!("{name}/{arity} is not defined")),
            },
        })
    }
}

/// The value of `ast` if it is a constant as jq folds them when it reads a filter: a literal, an
/// operator on two constants that gives no error, `[c, ...]` and `{"name": c, ...}`. A `-` before
/// one is not folded.
fn constant_of(ast: &Ast) -> Option<Value> {
    match ast {
        Ast::Literal(v) => Some(v.clone()),
        Ast::Binary(op, a, b) => crate::builtins::binary(*op, constant_of(a)?, constant_of(b)?).ok(),
        Ast::Array(None) => Some(Value::array(Vec::new())),
        Ast::Array(Some(body)) => {
            let mut items = Vec::new();
            constant_items(body, &mut items).then(|| Value::array(items))
        }
        Ast::Object(entries) => {
            let mut map = Map::new();
            for (k, v) in entries {
                let Value::String(name) = constant_of(k)? else { return None };
                map.insert(Key(name), constant_of(v)?);
            }
            Some(Value::Object(std::rc::Rc::new(map)))
        }
        _ => None,
    }
}

/// The items of `a, b, ...` when each is a constant. jq folds the chain `,` makes on its own, which
/// leans left: `[1, (2, 3)]` is not folded, `[(1, 2), 3]` is.
fn constant_items(ast: &Ast, items: &mut Vec<Value>) -> bool {
    if let Ast::Comma(a, b) = ast {
        if matches!(**b, Ast::Comma(..)) {
            return false;
        }
        return constant_items(a, items) && constant_items(b, items);
    }
    match constant_of(ast) {
        Some(v) => {
            items.push(v);
            true
        }
        None => false,
    }
}
