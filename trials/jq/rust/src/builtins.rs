//! What jq computes from values: the operators, indexing and slicing, paths, and the built-in
//! functions that take values only.

use crate::input::parse_text;
use crate::number::{Decimal, double_text};
use crate::output::to_json;
use crate::parser::Op;
use crate::value::{Error, Key, Map, Result, Value, checked, checked_as, sorted_keys};
use regex::{Regex, RegexBuilder};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;

/// A value as jq names it in its errors: its kind and a little of its JSON.
pub fn describe(value: &Value) -> String {
    let mut json = to_json(value);
    if json.len() > 11 {
        let mut end = 10;
        while !json.is_char_boundary(end) {
            end -= 1;
        }
        json.truncate(end);
        json.push_str("...");
    }
    format!("{} ({json})", value.type_name())
}

pub fn error(message: impl Into<String>) -> Error {
    Value::string(message)
}

fn type_error(value: &Value, message: &str) -> Error {
    error(format!("{} {message}", describe(value)))
}

fn type_error2(a: &Value, b: &Value, message: &str) -> Error {
    error(format!("{} and {} {message}", describe(a), describe(b)))
}

/// `a op b`, jq's arithmetic and comparisons. `a` is taken, so that a text, an array or an object
/// held only here is added to in place.
pub fn binary(op: Op, a: Value, b: Value) -> Result<Value> {
    if matches!(op, Op::Add | Op::Mul | Op::Div | Op::Mod) {
        return binary_unchecked(op, a, b);
    }
    if matches!(op, Op::Eq | Op::Ne) {
        return checked_as("Equality check too deep", || binary_unchecked(op, a, b));
    }
    checked(|| binary_unchecked(op, a, b))
}

fn binary_unchecked(op: Op, a: Value, b: Value) -> Result<Value> {
    match op {
        Op::Add => add(a, b),
        Op::Sub => subtract(a, b),
        Op::Mul => multiply(a, b),
        Op::Div => divide(a, b),
        Op::Mod => modulo(a, b),
        Op::Eq => Ok(Value::from_bool(a.equals(&b))),
        Op::Ne => Ok(Value::from_bool(!a.equals(&b))),
        Op::Lt => Ok(Value::from_bool(a.compare(&b) == Ordering::Less)),
        Op::Le => Ok(Value::from_bool(a.compare(&b) != Ordering::Greater)),
        Op::Gt => Ok(Value::from_bool(a.compare(&b) == Ordering::Greater)),
        Op::Ge => Ok(Value::from_bool(a.compare(&b) != Ordering::Less)),
    }
}

fn add(a: Value, b: Value) -> Result<Value> {
    match (a, b) {
        (Value::Null, b) => Ok(b),
        (a, Value::Null) => Ok(a),
        (Value::String(mut x), Value::String(y)) => {
            Rc::make_mut(&mut x).push_str(&y);
            Ok(Value::String(x))
        }
        (Value::Array(mut x), Value::Array(y)) => {
            Rc::make_mut(&mut x).extend(y.iter().cloned());
            Ok(Value::Array(x))
        }
        (Value::Object(mut x), Value::Object(y)) => {
            let map = Rc::make_mut(&mut x);
            for (k, v) in y.iter() {
                map.insert(k.clone(), v.clone());
            }
            Ok(Value::Object(x))
        }
        (a, b) => match (a.number(), b.number()) {
            (Some(x), Some(y)) => Ok(Value::Number(x + y)),
            _ => Err(type_error2(&a, &b, "cannot be added")),
        },
    }
}

fn subtract(a: Value, b: Value) -> Result<Value> {
    if let (Some(x), Some(y)) = (a.number(), b.number()) {
        return Ok(Value::Number(x - y));
    }
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            Ok(Value::array(x.iter().filter(|v| !y.iter().any(|w| v.equals(w))).cloned().collect()))
        }
        (a, b) => Err(type_error2(&a, &b, "cannot be subtracted")),
    }
}

fn multiply(a: Value, b: Value) -> Result<Value> {
    if let (Some(x), Some(y)) = (a.number(), b.number()) {
        return Ok(Value::Number(x * y));
    }
    match (a, b) {
        (Value::String(s), n) | (n, Value::String(s)) if n.is_number() => {
            let d = n.number().unwrap();
            if d < 0.0 || d.is_nan() {
                return Ok(Value::Null);
            }
            let times = if d > i32::MAX as f64 { i32::MAX as usize } else { d as usize };
            Ok(Value::string(s.repeat(times)))
        }
        (Value::Object(x), Value::Object(y)) => Ok(Value::Object(Rc::new(merge_deep((*x).clone(), &y)))),
        (a, b) => Err(type_error2(&a, &b, "cannot be multiplied")),
    }
}

fn merge_deep(mut a: Map, b: &Map) -> Map {
    for (k, v) in b.iter() {
        let merged = match (a.get(k.0.as_str()), v) {
            (Some(Value::Object(x)), Value::Object(y)) => Value::Object(Rc::new(merge_deep((**x).clone(), y))),
            _ => v.clone(),
        };
        a.insert(k.clone(), merged);
    }
    a
}

fn divide(a: Value, b: Value) -> Result<Value> {
    if let (Some(x), Some(y)) = (a.number(), b.number()) {
        if y == 0.0 {
            return Err(type_error2(&a, &b, "cannot be divided because the divisor is zero"));
        }
        return Ok(Value::Number(x / y));
    }
    match (&a, &b) {
        (Value::String(x), Value::String(y)) => Ok(split(x, y)),
        _ => Err(type_error2(&a, &b, "cannot be divided")),
    }
}

/// jq's `%`: both sides cut to whole numbers (saturating), the sign of the left kept.
fn modulo(a: Value, b: Value) -> Result<Value> {
    let (Some(x), Some(y)) = (a.number(), b.number()) else {
        return Err(type_error2(&a, &b, "cannot be divided"));
    };
    if x.is_nan() || y.is_nan() {
        return Ok(Value::Number(f64::NAN));
    }
    let divisor = y as i64;
    if divisor == 0 {
        return Err(type_error2(&a, &b, "cannot be divided because the divisor is zero"));
    }
    let r = if divisor == -1 { 0 } else { (x as i64) % divisor };
    Ok(Value::Number(r as f64))
}

/// A text split by another, as `split/1` and `/` split it.
fn split(text: &str, separator: &str) -> Value {
    if text.is_empty() {
        return Value::array(Vec::new());
    }
    let parts: Vec<Value> = if separator.is_empty() {
        text.chars().map(|c| Value::string(c.to_string())).collect()
    } else {
        text.split(separator).map(Value::string).collect()
    };
    Value::array(parts)
}

/// An index of an array, cut toward zero as jq cuts it, from the end when negative; `None` past
/// either end.
fn array_index(length: usize, index: f64) -> Option<usize> {
    if index.is_nan() {
        return None;
    }
    let mut i = index as i64;
    if i < 0 {
        i += length as i64;
    }
    if i < 0 || i >= length as i64 { None } else { Some(i as usize) }
}

/// The start and end of a slice, as jq's `parse_slice` takes them.
fn slice_bounds(length: usize, from: &Value, to: &Value) -> Result<(usize, usize)> {
    let len = length as f64;
    let bound = |v: &Value, default: f64| -> Result<f64> {
        match v {
            Value::Null => Ok(default),
            _ => v.number().ok_or_else(|| error("Start and end indices of an array slice must be numbers")),
        }
    };
    let mut start = bound(from, 0.0)?;
    let mut end = bound(to, len)?;
    if start < 0.0 {
        start += len;
    }
    if end < 0.0 {
        end += len;
    }
    start = start.clamp(0.0, len);
    if end > len {
        end = len;
    }
    if end < start {
        end = start;
    }
    let start = start as usize;
    let end = if end > end.trunc() { end as usize + 1 } else { end as usize };
    Ok((start, end.max(start)))
}

/// `.[from:to]` of an array, a text (by characters) or null.
pub fn slice(value: &Value, from: &Value, to: &Value) -> Result<Value> {
    match value {
        Value::Null => Ok(Value::Null),
        Value::Array(items) => {
            let (s, e) = slice_bounds(items.len(), from, to)?;
            Ok(Value::array(items[s..e].to_vec()))
        }
        Value::String(text) => {
            let count = text.chars().count();
            let (s, e) = slice_bounds(count, from, to)?;
            Ok(Value::string(text.chars().skip(s).take(e - s).collect::<String>()))
        }
        _ => Err(error(format!("Cannot index {} with object", value.type_name()))),
    }
}

/// `.[key]`, as jq's `jv_get` takes it.
pub fn index(value: &Value, key: &Value) -> Result<Value> {
    match (value, key) {
        (Value::Object(map), Value::String(k)) => Ok(map.get(k.as_str()).cloned().unwrap_or(Value::Null)),
        (Value::Array(items), k) if k.is_number() => {
            Ok(array_index(items.len(), k.number().unwrap()).map(|i| items[i].clone()).unwrap_or(Value::Null))
        }
        (Value::Null, Value::String(_) | Value::Object(_)) => Ok(Value::Null),
        (Value::Null, k) if k.is_number() => Ok(Value::Null),
        (Value::Array(_) | Value::String(_), Value::Object(bounds)) => {
            match (bounds.get("start"), bounds.get("end")) {
                (Some(from), Some(to)) => slice(value, from, to),
                _ => Err(error("Start and end indices of an array slice must be numbers")),
            }
        }
        (Value::Array(items), Value::Array(part)) => Ok(indices(items, part)),
        (Value::Object(_), k) if k.is_number() => Err(error("Cannot index object with number")),
        (v, Value::String(k)) => Err(error(format!("Cannot index {} with \"{k}\"", v.type_name()))),
        (v, k) => Err(error(format!("Cannot index {} with {}", v.type_name(), k.type_name()))),
    }
}

/// Where an array holds another as a run of its items (`.[[1,2]]`).
fn indices(items: &[Value], part: &[Value]) -> Value {
    let mut found = Vec::new();
    for i in 0..items.len() {
        if !part.is_empty()
            && i + part.len() <= items.len()
            && items[i..i + part.len()].iter().zip(part).all(|(a, b)| a.equals(b))
        {
            found.push(Value::Number(i as f64));
        }
    }
    Value::array(found)
}

/// `getpath(path)`.
pub fn get_path(value: &Value, keys: &[Value]) -> Result<Value> {
    let mut current = value.clone();
    for key in keys.iter() {
        if let Value::Null = current {
            return Ok(Value::Null);
        }
        current = index(&current, key)?;
    }
    Ok(current)
}

/// `setpath(path; value)`, changing `root` in place where it is held only here.
pub fn set_path(root: &mut Value, path: &[Value], value: Value) -> Result<()> {
    let Some((key, rest)) = path.split_first() else {
        *root = value;
        return Ok(());
    };
    if let Value::Null = root {
        match key {
            Value::String(_) => *root = Value::Object(Rc::new(Map::new())),
            Value::Object(_) => *root = Value::array(Vec::new()),
            k if k.is_number() => *root = Value::array(Vec::new()),
            k => return Err(error(format!("Cannot index null with {}", k.type_name()))),
        }
    }
    match (root, key) {
        (Value::Object(map), Value::String(k)) => {
            let map = Rc::make_mut(map);
            let entry = map.entry(Key(k.clone())).or_insert(Value::Null);
            set_path(entry, rest, value)
        }
        (Value::Array(items), k) if k.is_number() => {
            let d = k.number().unwrap();
            let items = Rc::make_mut(items);
            let mut i = if d.is_nan() { 0 } else { d as i64 };
            if i < 0 {
                i += items.len() as i64;
                if i < 0 {
                    return Err(error("Out of bounds negative array index"));
                }
            }
            let i = i as usize;
            if i >= items.len() {
                if i > 536_870_912 {
                    return Err(error("Array index too large"));
                }
                items.resize(i + 1, Value::Null);
            }
            let mut slot = std::mem::replace(&mut items[i], Value::Null);
            let result = set_path(&mut slot, rest, value);
            items[i] = slot;
            result
        }
        (Value::Array(items), Value::Object(bounds)) => {
            let from = bounds.get("start").cloned().unwrap_or(Value::Null);
            let to = bounds.get("end").cloned().unwrap_or(Value::Null);
            let (s, e) = slice_bounds(items.len(), &from, &to)?;
            let mut part = Value::array(items[s..e].to_vec());
            set_path(&mut part, rest, value)?;
            let Value::Array(part) = part else {
                return Err(error("A slice of an array can only be assigned another array"));
            };
            Rc::make_mut(items).splice(s..e, part.iter().cloned());
            Ok(())
        }
        (root, Value::Object(_)) => Err(error(format!("Cannot update field at object index of {}", root.type_name()))),
        (root, key) => Err(match index(root, key) {
            Err(e) => e,
            Ok(_) => error(format!("Cannot index {} with {}", root.type_name(), key.type_name())),
        }),
    }
}

/// `delpaths(paths)`: the paths sorted, then deleted from the last, as jq deletes them.
pub fn delete_paths(value: Value, paths: &Value) -> Result<Value> {
    let Value::Array(paths) = paths else {
        return Err(error("Paths must be specified as an array"));
    };
    let mut paths: Vec<Value> = paths.to_vec();
    paths.sort_by(|a, b| a.sort_compare(b));
    let mut lists = Vec::with_capacity(paths.len());
    for p in &paths {
        match p {
            Value::Array(keys) => lists.push(keys.clone()),
            _ => return Err(error("Path must be specified as an array")),
        }
    }
    if lists.is_empty() {
        return Ok(value);
    }
    if lists[0].is_empty() {
        return Ok(Value::Null);
    }
    delete_sorted(value, &lists, 0)
}

fn delete_sorted(mut value: Value, paths: &[Rc<Vec<Value>>], depth: usize) -> Result<Value> {
    let mut whole = Vec::new();
    let mut i = 0;
    while i < paths.len() {
        let key = &paths[i][depth];
        let mut j = i;
        while j < paths.len() && paths[j][depth].equals(key) {
            j += 1;
        }
        if paths[i].len() == depth + 1 {
            whole.push(key.clone());
        } else {
            let inner = index(&value, key)?;
            if !matches!(inner, Value::Null) {
                let inner = delete_sorted(inner, &paths[i..j], depth + 1)?;
                set_path(&mut value, std::slice::from_ref(key), inner)?;
            }
        }
        i = j;
    }
    delete_keys(value, &whole)
}

/// Deletes the members or items named, all at once.
fn delete_keys(value: Value, keys: &[Value]) -> Result<Value> {
    if keys.is_empty() {
        return Ok(value);
    }
    match value {
        Value::Null => Ok(Value::Null),
        Value::Array(items) => {
            let length = items.len();
            let mut gone = vec![false; length];
            for key in keys {
                match key {
                    k if k.is_number() => {
                        let d = k.number().unwrap();
                        let i = if d < 0.0 { length as i64 + d as i64 } else { d as i64 };
                        if i >= 0 && (i as usize) < length {
                            gone[i as usize] = true;
                        }
                    }
                    Value::Object(bounds) => {
                        let from = bounds.get("start").cloned().unwrap_or(Value::Null);
                        let to = bounds.get("end").cloned().unwrap_or(Value::Null);
                        let (s, e) = slice_bounds(length, &from, &to)?;
                        gone[s..e].iter_mut().for_each(|g| *g = true);
                    }
                    k => return Err(error(format!("Cannot delete {} element of array", k.type_name()))),
                }
            }
            let kept = items.iter().zip(gone).filter(|(_, g)| !g).map(|(v, _)| v.clone()).collect();
            Ok(Value::array(kept))
        }
        Value::Object(mut map) => {
            for key in keys {
                let Value::String(k) = key else {
                    return Err(error(format!("Cannot delete field at index of {}", key.type_name())));
                };
                if map.contains_key(k.as_str()) {
                    Rc::make_mut(&mut map).shift_remove(k.as_str());
                }
            }
            Ok(Value::Object(map))
        }
        v => Err(error(format!("Cannot delete fields from {}", v.type_name()))),
    }
}

/// The built-in functions that take only values.
#[derive(Clone, Copy, Debug)]
pub enum Native {
    Not,
    Length,
    Utf8ByteLength,
    Keys,
    KeysUnsorted,
    Floor,
    Sqrt,
    ToString,
    ToNumber,
    ToJson,
    FromJson,
    Type,
    Sort,
    Unique,
    Min,
    Max,
    AsciiDowncase,
    AsciiUpcase,
    Has,
    Split,
    StartsWith,
    EndsWith,
    SetPath,
    DelPaths,
    SortBy,
    GroupBy,
    UniqueBy,
    MinBy,
    MaxBy,
    Match,
    Csv,
    Tsv,
    Base64,
    Base64Decode,
    Uri,
}

impl Native {
    /// The function of this name and number of arguments.
    pub fn find(name: &str, arity: usize) -> Option<Native> {
        use Native::*;
        Some(match (name, arity) {
            ("not", 0) => Not,
            ("length", 0) => Length,
            ("utf8bytelength", 0) => Utf8ByteLength,
            ("keys", 0) => Keys,
            ("keys_unsorted", 0) => KeysUnsorted,
            ("floor", 0) => Floor,
            ("sqrt", 0) => Sqrt,
            ("tostring", 0) => ToString,
            ("tonumber", 0) => ToNumber,
            ("tojson", 0) => ToJson,
            ("fromjson", 0) => FromJson,
            ("type", 0) => Type,
            ("sort", 0) => Sort,
            ("unique", 0) => Unique,
            ("min", 0) => Min,
            ("max", 0) => Max,
            ("ascii_downcase", 0) => AsciiDowncase,
            ("ascii_upcase", 0) => AsciiUpcase,
            ("has", 1) => Has,
            ("split", 1) => Split,
            ("startswith", 1) => StartsWith,
            ("endswith", 1) => EndsWith,
            ("setpath", 2) => SetPath,
            ("delpaths", 1) => DelPaths,
            ("_sort_by_impl", 1) => SortBy,
            ("_group_by_impl", 1) => GroupBy,
            ("_unique_by_impl", 1) => UniqueBy,
            ("_min_by_impl", 1) => MinBy,
            ("_max_by_impl", 1) => MaxBy,
            ("_match_impl", 3) => Match,
            _ => return None,
        })
    }

    /// The format `@name`: `None` when there is no such format.
    pub fn format(name: &str) -> Option<Native> {
        Some(match name {
            "text" => Native::ToString,
            "json" => Native::ToJson,
            "csv" => Native::Csv,
            "tsv" => Native::Tsv,
            "base64" => Native::Base64,
            "base64d" => Native::Base64Decode,
            "uri" => Native::Uri,
            _ => return None,
        })
    }

    /// Calls the function on its input and its arguments.
    pub fn call(self, input: Value, args: &[Value]) -> Result<Value> {
        use Native::*;
        if matches!(self, Sort | Unique | Min | Max | SortBy | GroupBy | UniqueBy | MinBy | MaxBy | DelPaths) {
            return checked(|| self.call_unchecked(input, args));
        }
        self.call_unchecked(input, args)
    }

    fn call_unchecked(self, input: Value, args: &[Value]) -> Result<Value> {
        use Native::*;
        match self {
            Not => Ok(Value::from_bool(!input.truthy())),
            Length => length(input),
            Utf8ByteLength => match &input {
                Value::String(s) => Ok(Value::Number(s.len() as f64)),
                _ => Err(type_error(&input, "only strings have UTF-8 byte length")),
            },
            Keys | KeysUnsorted => match &input {
                Value::Object(map) => {
                    let keys = if matches!(self, Keys) {
                        sorted_keys(map).into_iter().map(|k| Value::String(k.clone())).collect()
                    } else {
                        map.keys().map(|k| Value::String(k.0.clone())).collect()
                    };
                    Ok(Value::array(keys))
                }
                Value::Array(items) => Ok(Value::array((0..items.len()).map(|i| Value::Number(i as f64)).collect())),
                _ => Err(type_error(&input, "has no keys")),
            },
            Floor | Sqrt => match input.number() {
                Some(d) => Ok(Value::Number(if matches!(self, Floor) { d.floor() } else { d.sqrt() })),
                None => Err(type_error(&input, "number required")),
            },
            ToString => Ok(match input {
                Value::String(_) => input,
                _ => Value::string(to_json(&input)),
            }),
            ToNumber => to_number(input),
            ToJson => Ok(Value::string(to_json(&input))),
            FromJson => match &input {
                Value::String(s) => parse_text(s).map_err(error),
                _ => Err(type_error(&input, "only strings can be parsed")),
            },
            Type => Ok(Value::string(input.type_name())),
            Sort => {
                let items = array_of(&input, "cannot be sorted, as it is not an array")?;
                let mut items = items.to_vec();
                items.sort_by(|a, b| a.sort_compare(b));
                Ok(Value::array(items))
            }
            Unique => {
                let items = array_of(&input, "cannot be sorted, as it is not an array")?;
                let groups = group(items, items);
                Ok(Value::array(groups.into_iter().map(|g| g[0].clone()).collect()))
            }
            Min | Max => {
                let items = array_of(&input, "cannot be sorted, as it is not an array")?;
                Ok(min_max(items, items, matches!(self, Min)))
            }
            AsciiDowncase | AsciiUpcase => match &input {
                Value::String(s) => Ok(Value::string(if matches!(self, AsciiDowncase) {
                    s.to_ascii_lowercase()
                } else {
                    s.to_ascii_uppercase()
                })),
                _ => Err(type_error(&input, "cannot be exploded")),
            },
            Has => has(&input, &args[0]),
            Split => match (&input, &args[0]) {
                (Value::String(s), Value::String(sep)) => Ok(split(s, sep)),
                _ => Err(error("split input and separator must be strings")),
            },
            StartsWith | EndsWith => match (&input, &args[0]) {
                (Value::String(s), Value::String(t)) => {
                    Ok(Value::from_bool(if matches!(self, StartsWith) { s.starts_with(&**t) } else { s.ends_with(&**t) }))
                }
                _ => Err(error(if matches!(self, StartsWith) {
                    "startswith() requires string inputs"
                } else {
                    "endswith() requires string inputs"
                })),
            },
            SetPath => {
                let Value::Array(path) = &args[0] else {
                    return Err(error("Path must be specified as an array"));
                };
                let mut root = input;
                set_path(&mut root, path, args[1].clone())?;
                Ok(root)
            }
            DelPaths => delete_paths(input, &args[0]),
            SortBy | GroupBy | UniqueBy | MinBy | MaxBy => {
                let items = array_of(&input, "cannot be sorted, as it is not an array")?;
                let Value::Array(keys) = &args[0] else {
                    return Err(error("the keys are not an array"));
                };
                Ok(match self {
                    SortBy => {
                        let mut order: Vec<usize> = (0..items.len()).collect();
                        order.sort_by(|&a, &b| keys[a].sort_compare(&keys[b]));
                        Value::array(order.into_iter().map(|i| items[i].clone()).collect())
                    }
                    GroupBy => {
                        Value::array(group(items, keys).into_iter().map(|g| Value::array(g.into_iter().cloned().collect())).collect())
                    }
                    UniqueBy => Value::array(group(items, keys).into_iter().map(|g| g[0].clone()).collect()),
                    _ => min_max(items, keys, matches!(self, MinBy)),
                })
            }
            Match => test(&input, &args[0], &args[1]),
            Csv | Tsv => csv(&input, matches!(self, Csv)),
            Base64 | Base64Decode | Uri => {
                let text = match input {
                    Value::String(s) => s,
                    other => Rc::new(to_json(&other)),
                };
                match self {
                    Base64 => Ok(Value::string(base64(text.as_bytes()))),
                    Base64Decode => base64_decode(&text),
                    _ => Ok(Value::string(uri(&text))),
                }
            }
        }
    }
}

fn array_of<'a>(value: &'a Value, message: &str) -> Result<&'a [Value]> {
    match value {
        Value::Array(items) => Ok(items),
        _ => Err(type_error(value, message)),
    }
}

fn length(input: Value) -> Result<Value> {
    Ok(match &input {
        Value::Null => Value::Number(0.0),
        Value::False | Value::True => return Err(type_error(&input, "has no length")),
        Value::String(s) => Value::Number(s.chars().count() as f64),
        Value::Array(items) => Value::Number(items.len() as f64),
        Value::Object(map) => Value::Number(map.len() as f64),
        number => {
            if number.number().unwrap().is_sign_negative() {
                number.negate().unwrap()
            } else {
                input
            }
        }
    })
}

fn has(input: &Value, key: &Value) -> Result<Value> {
    match (input, key) {
        (Value::Null, _) => Ok(Value::False),
        (Value::Object(map), Value::String(k)) => Ok(Value::from_bool(map.contains_key(k.as_str()))),
        (Value::Array(items), k) if k.is_number() => {
            let d = k.number().unwrap();
            Ok(Value::from_bool(!d.is_nan() && d as i64 >= 0 && (d as i64) < items.len() as i64))
        }
        _ => Err(error(format!("Cannot check whether {} has a {} key", input.type_name(), key.type_name()))),
    }
}

/// `tonumber`: a text read as decNumber reads one, keeping its literal.
fn to_number(input: Value) -> Result<Value> {
    if input.is_number() {
        return Ok(input);
    }
    let Value::String(text) = &input else {
        return Err(type_error(&input, "cannot be parsed as a number"));
    };
    if let Some(decimal) = Decimal::parse(text.as_bytes()) {
        return Ok(Value::literal(decimal));
    }
    let (negative, word) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, &text[..]),
    };
    let lower = word.to_ascii_lowercase();
    match lower.as_str() {
        "inf" | "infinity" => Ok(Value::infinite_literal(if negative { f64::NEG_INFINITY } else { f64::INFINITY })),
        "nan" => Ok(Value::Number(f64::NAN)),
        _ => Err(type_error(&input, "cannot be parsed as a number")),
    }
}

/// Groups of equal keys, in the order of the keys sorted (stable).
fn group<'a>(items: &'a [Value], keys: &[Value]) -> Vec<Vec<&'a Value>> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| keys[a].sort_compare(&keys[b]));
    let mut groups: Vec<Vec<&Value>> = Vec::new();
    let mut last: Option<usize> = None;
    for i in order {
        match last {
            Some(l) if keys[l].equals(&keys[i]) => groups.last_mut().unwrap().push(&items[i]),
            _ => groups.push(vec![&items[i]]),
        }
        last = Some(i);
    }
    groups
}

/// The item with the least key (the first of equals) or the greatest (the last of equals).
fn min_max(items: &[Value], keys: &[Value], min: bool) -> Value {
    let mut best: Option<usize> = None;
    for i in 0..items.len() {
        let better = match best {
            None => true,
            Some(b) => {
                let o = keys[i].compare(&keys[b]);
                if min { o == Ordering::Less } else { o != Ordering::Less }
            }
        };
        if better {
            best = Some(i);
        }
    }
    best.map(|i| items[i].clone()).unwrap_or(Value::Null)
}

thread_local! {
    static REGEXES: RefCell<HashMap<(String, String), Rc<Regex>>> = RefCell::new(HashMap::new());
}

/// `test(re; flags)`, with the `regex` crate's syntax.
fn test(input: &Value, pattern: &Value, flags: &Value) -> Result<Value> {
    let Value::String(text) = input else {
        return Err(type_error(input, "cannot be matched, as it is not a string"));
    };
    let Value::String(pattern) = pattern else {
        return Err(type_error(pattern, "is not a string"));
    };
    let flags = match flags {
        Value::Null => String::new(),
        Value::String(f) => (**f).clone(),
        other => return Err(type_error(other, "is not a string")),
    };
    let mut not_empty = false;
    let mut builder = RegexBuilder::new(&oniguruma_end(pattern));
    for c in flags.chars() {
        match c {
            'g' | 's' | 'l' => {}
            'i' => {
                builder.case_insensitive(true);
            }
            'x' => {
                builder.ignore_whitespace(true);
            }
            'n' => not_empty = true,
            'p' => {
                builder.dot_matches_new_line(true);
            }
            _ => return Err(error(format!("{flags} is not a valid modifier string"))),
        }
    }
    let key = ((**pattern).clone(), flags.clone());
    let regex = REGEXES.with(|cache| -> Result<Rc<Regex>> {
        if let Some(r) = cache.borrow().get(&key) {
            return Ok(r.clone());
        }
        let r = Rc::new(builder.build().map_err(|e| error(format!("{pattern} (at offset 0) is not a valid regex: {e}")))?);
        cache.borrow_mut().insert(key, r.clone());
        Ok(r)
    })?;
    let found = if not_empty { regex.find_iter(text).any(|m| !m.as_str().is_empty()) } else { regex.is_match(text) };
    Ok(Value::from_bool(found))
}

/// The pattern with each `$` (and `\\Z`) outside a class made Oniguruma's: the end of the text or the place
/// before a last new line (`(?:\n?\z)`, which may take the new line: `test` only asks whether).
fn oniguruma_end(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 8);
    let mut chars = pattern.chars().peekable();
    let mut class = 0;
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'Z') && class == 0 => {
                chars.next();
                out.push_str("(?:\\n?\\z)");
            }
            '\\' => {
                out.push(c);
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '[' => {
                out.push(c);
                class += 1;
                // A `]` straight after `[` or `[^` is a member of the class.
                if chars.peek() == Some(&'^') {
                    out.push(chars.next().unwrap());
                }
                if chars.peek() == Some(&']') {
                    out.push(chars.next().unwrap());
                }
            }
            ']' if class > 0 => {
                out.push(c);
                class -= 1;
            }
            '$' if class == 0 => out.push_str("(?:\\n?\\z)"),
            _ => out.push(c),
        }
    }
    out
}

/// `@csv` and `@tsv`.
fn csv(input: &Value, comma: bool) -> Result<Value> {
    let Value::Array(items) = input else {
        return Err(type_error(input, if comma { "cannot be csv-formatted, only array" } else { "cannot be tsv-formatted, only array" }));
    };
    let mut line = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            line.push(if comma { ',' } else { '\t' });
        }
        match item {
            Value::Null => {}
            Value::True => line.push_str("true"),
            Value::False => line.push_str("false"),
            Value::String(s) => {
                if comma {
                    line.push('"');
                    line.push_str(&s.replace('"', "\"\""));
                    line.push('"');
                } else {
                    for c in s.chars() {
                        match c {
                            '\\' => line.push_str("\\\\"),
                            '\t' => line.push_str("\\t"),
                            '\n' => line.push_str("\\n"),
                            '\r' => line.push_str("\\r"),
                            c => line.push(c),
                        }
                    }
                }
            }
            Value::Number(d) | Value::Exact(d) => {
                if !d.is_nan() {
                    line.push_str(&double_text(*d));
                }
            }
            Value::Literal(l) => line.push_str(&l.text),
            other => {
                return Err(type_error(other, if comma { "is not valid in a csv row" } else { "is not valid in a tsv row" }));
            }
        }
    }
    Ok(Value::string(line))
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `@base64d` as jq reads it: up to the first `=`, every other byte an error; the bytes that are
/// not UTF-8 replaced as jq replaces them.
fn base64_decode(text: &str) -> Result<Value> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3 + 3);
    let mut code: u32 = 0;
    let mut count = 0;
    for &b in text.as_bytes() {
        if b == b'=' {
            break;
        }
        let Some(v) = BASE64.iter().position(|&c| c == b) else {
            return Err(error(format!("{} is not valid base64 data", describe(&Value::string(text)))));
        };
        code = (code << 6) | v as u32;
        count += 1;
        if count == 4 {
            out.extend_from_slice(&[(code >> 16) as u8, (code >> 8) as u8, code as u8]);
            code = 0;
            count = 0;
        }
    }
    match count {
        3 => out.extend_from_slice(&[(code >> 10) as u8, (code >> 2) as u8]),
        2 => out.push((code >> 4) as u8),
        1 => return Err(error(format!("{} trailing base64 byte found", describe(&Value::string(text))))),
        _ => {}
    }
    Ok(Value::string(crate::input::replace_bad_utf8(&out)))
}

fn uri(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
