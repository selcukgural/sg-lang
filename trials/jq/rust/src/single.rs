//! Single outputs at once (README, "How it is written" 4): a node that gives exactly one output and
//! calls no function of the filter is worked out directly, by recursion over the tree of the filter
//! (as deep as the filter is written, never as deep as the data), with no frame of the machine: `.`,
//! a literal, a variable, `.a`, `.[e]` and `.[a:b]` of such (not with `?`), negation, arithmetic,
//! comparison, `and`, `or` and `//` of such, `a | b` and `a as $x | b` of such, `{...}` and `"\(...)"`
//! of such, a format, a built-in of such arguments, and `if` whose parts are such. `[f]` of such an `f`,
//! and `[t[] | f]` with such a `t` and `f`, go over the items in one loop. (The update of `reduce`, when
//! it is such, is worked out at once by the machine and handed the state.)

use crate::builtins::{binary, describe, error, index, slice};
use crate::compile::Ir;
use crate::machine::Env;
use crate::value::{Key, Map, Result, Value, trim_vec};
use std::rc::Rc;

/// Whether a node gives exactly one output and calls no function of the filter.
pub fn is_single(ir: &Ir) -> bool {
    match ir {
        Ir::Identity | Ir::Literal(_) | Ir::Var(_) | Ir::Single(_) => true,
        Ir::Field(target, _, false) => is_single(target),
        Ir::Index(target, key, false) => is_single(target) && is_single(key),
        Ir::Slice(target, from, to, false) => is_single(target) && is_single(from) && is_single(to),
        Ir::Alternative(a, b) | Ir::Pipe(a, b) | Ir::As(a, b) => is_single(a) && is_single(b),
        Ir::Neg(a) | Ir::Format(_, a) => is_single(a),
        Ir::Binary(_, a, b) | Ir::And(a, b) | Ir::Or(a, b) => is_single(a) && is_single(b),
        Ir::If(c, t, e) => is_single(c) && is_single(t) && is_single(e),
        Ir::Object(entries) => entries.iter().all(|(k, v)| is_single(k) && is_single(v)),
        Ir::Native(_, args) => args.iter().all(is_single),
        Ir::Collect(body) => is_items(body) || is_single(body),
        _ => false,
    }
}

/// `t[]` of such: the items a loop goes over.
fn is_source(ir: &Ir) -> bool {
    matches!(ir, Ir::Iterate(target, _) if is_single(target))
}

/// The items, or the items each put through a single filter (`.[] | f`, as `map(f)` is).
fn is_items(ir: &Ir) -> bool {
    match ir {
        Ir::Pipe(source, f) => is_source(source) && is_single(f),
        other => is_source(other),
    }
}

/// The output of a node with the input lent: a literal or a variable takes none of it.
fn eval_lent(ir: &Ir, input: &Value, env: &Env) -> Result<Value> {
    match ir {
        Ir::Literal(v) => Ok(v.clone()),
        Ir::Var(up) => Ok(env.var(*up)),
        _ => eval(ir, input.clone(), env),
    }
}

/// The one output of a node `is_single` holds. The input is taken, so that the part that goes on
/// with it (the left of an operator, the branch of an `if`, the update of a `reduce`) holds it alone.
pub fn eval(ir: &Ir, input: Value, env: &Env) -> Result<Value> {
    match ir {
        Ir::Identity => Ok(input),
        Ir::Literal(v) => Ok(v.clone()),
        Ir::Var(up) => Ok(env.var(*up)),
        Ir::Single(inner) => eval(inner, input, env),
        Ir::Field(target, key, _) => {
            let t = eval(target, input, env)?;
            index(&t, key)
        }
        Ir::Index(target, key, _) => {
            let k = eval(key, input.clone(), env)?;
            let t = eval(target, input, env)?;
            index(&t, &k)
        }
        Ir::Neg(a) => {
            let v = eval(a, input, env)?;
            v.negate().ok_or_else(|| error(format!("{} cannot be negated", describe(&v))))
        }
        Ir::Format(native, a) => native.call(eval(a, input, env)?, &[]),
        Ir::Binary(op, a, b) => {
            let right = eval_lent(b, &input, env)?;
            let left = eval(a, input, env)?;
            binary(*op, left, right)
        }
        Ir::And(a, b) => {
            if !eval_lent(a, &input, env)?.truthy() {
                return Ok(Value::False);
            }
            Ok(Value::from_bool(eval(b, input, env)?.truthy()))
        }
        Ir::Or(a, b) => {
            if eval_lent(a, &input, env)?.truthy() {
                return Ok(Value::True);
            }
            Ok(Value::from_bool(eval(b, input, env)?.truthy()))
        }
        Ir::If(c, then, otherwise) => {
            let branch = if eval_lent(c, &input, env)?.truthy() { then } else { otherwise };
            eval(branch, input, env)
        }
        Ir::Slice(target, from, to, _) => {
            // The start first, then the end, then what is sliced, as jq runs them.
            let from = eval_lent(from, &input, env)?;
            let to = eval_lent(to, &input, env)?;
            let t = eval(target, input, env)?;
            slice(&t, &from, &to)
        }
        Ir::Alternative(a, b) => {
            // `//` catches no error in jq 1.8.2: an error on the left is the result's.
            let left = eval_lent(a, &input, env)?;
            if left.truthy() { Ok(left) } else { eval(b, input, env) }
        }
        Ir::Pipe(a, b) => eval(b, eval(a, input, env)?, env),
        Ir::As(source, body) => {
            let env = env.push_var(eval_lent(source, &input, env)?);
            eval(body, input, &env)
        }
        Ir::Object(_) | Ir::Native(..) | Ir::Collect(_) => eval_larger(ir, input, env),
        _ => unreachable!("not a node of a single output"),
    }
}

/// The nodes that build or loop, apart so that `eval`'s own frame stays small.
#[inline(never)]
fn eval_larger(ir: &Ir, input: Value, env: &Env) -> Result<Value> {
    match ir {
        Ir::Object(entries) => {
            let mut map = Map::with_capacity(entries.len());
            for (k, v) in entries {
                let key = match eval_lent(k, &input, env)? {
                    Value::String(s) => Key(s),
                    other => return Err(error(format!("Object keys must be strings, not {}", describe(&other)))),
                };
                map.insert(key, eval_lent(v, &input, env)?);
            }
            Ok(Value::Object(Rc::new(map)))
        }
        Ir::Native(native, args) => {
            // The last argument first, as jq runs a built-in's arguments.
            let mut values = vec![Value::Null; args.len()];
            for (slot, arg) in values.iter_mut().zip(args).rev() {
                *slot = eval_lent(arg, &input, env)?;
            }
            native.call(input, &values)
        }
        Ir::Collect(body) => {
            let mut items = Vec::new();
            match &**body {
                Ir::Pipe(source, f) if is_source(source) => {
                    items_of(source, input, env, &mut |item| {
                        items.push(eval(f, item, env)?);
                        Ok(())
                    })?;
                }
                source if is_source(source) => items_of(source, input, env, &mut |item| {
                    items.push(item);
                    Ok(())
                })?,
                single => items.push(eval(single, input, env)?),
            }
            trim_vec(&mut items);
            Ok(Value::array(items))
        }
        _ => unreachable!("not a node that builds or loops"),
    }
}

/// Gives each item of `t[]` to `each`, in order. The items of an array or an object
/// held only here are moved out of it.
fn items_of(source: &Ir, input: Value, env: &Env, each: &mut dyn FnMut(Value) -> Result<()>) -> Result<()> {
    match source {
        Ir::Iterate(target, optional) => match eval(target, input, env)? {
            Value::Array(items) => match Rc::try_unwrap(items) {
                Ok(items) => items.into_iter().try_for_each(each),
                Err(items) => items.iter().cloned().try_for_each(each),
            },
            Value::Object(map) => match Rc::try_unwrap(map) {
                Ok(map) => map.into_values().try_for_each(each),
                Err(map) => map.values().cloned().try_for_each(each),
            },
            _ if *optional => Ok(()),
            other => Err(error(format!("Cannot iterate over {}", describe(&other)))),
        },
        _ => unreachable!("not a source of items"),
    }
}
