//! The program's own value: null, false, true, a number, a text, an array and an object keeping
//! the order its names came in. Texts, arrays and objects are shared by a count of their holders
//! (`Rc`) and changed in place when there is only one (`Rc::make_mut`), as jq copies on write.

use crate::number::{self, Literal};
use indexmap::IndexMap;
use std::borrow::Borrow;
use std::cell::Cell;
use std::cmp::Ordering;
use std::rc::Rc;

/// The name of a member: a text shared with the values made from it (`keys`, `to_entries`).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Key(pub Rc<String>);

impl Borrow<str> for Key {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

/// The members of an object, in the order their names came in.
pub type Map = IndexMap<Key, Value>;

/// A number's literal: its text as decNumber writes it, and its double once it has been wanted
/// (choice 1824). A literal too large for decNumber is its infinity: written as the double is,
/// compared as an infinity.
pub struct Text {
    value: Cell<Option<f64>>,
    pub text: Box<str>,
    pub infinite: bool,
}

impl Text {
    /// A literal whose double is worked out when it is wanted.
    pub fn new(text: Box<str>) -> Text {
        Text { value: Cell::new(None), text, infinite: false }
    }

    /// Its double, worked out the first time it is wanted and kept.
    pub fn double(&self) -> f64 {
        if let Some(d) = self.value.get() {
            return d;
        }
        let d = number::text_double(&self.text);
        self.value.set(Some(d));
        d
    }
}

#[derive(Clone)]
pub enum Value {
    Null,
    False,
    True,
    /// A number computed.
    Number(f64),
    /// A number read whose double writes its literal the same way.
    Exact(f64),
    /// A number read, with its literal.
    Literal(Rc<Text>),
    String(Rc<String>),
    Array(Rc<Vec<Value>>),
    Object(Rc<Map>),
}

pub const KIND_NULL: u8 = 1;
pub const KIND_BOOLEAN: u8 = 2;
pub const KIND_NUMBER: u8 = 4;
pub const KIND_STRING: u8 = 8;
pub const KIND_ARRAY: u8 = 16;
pub const KIND_OBJECT: u8 = 32;

/// An error raised by the filter: any value, a text most often.
pub type Error = Value;
pub type Result<T> = std::result::Result<T, Error>;

/// How deep jq compares and writes values.
pub const MAX_DEPTH: usize = 10_000;

thread_local! {
    static TOO_DEEP: Cell<bool> = const { Cell::new(false) };
}

/// Marks a comparison too deep when it is; false (not equal) either way.
fn too_deep(depth: usize) -> bool {
    if depth > MAX_DEPTH {
        TOO_DEEP.set(true);
    }
    false
}

/// Runs what compares values, an error when a comparison went deeper than jq compares.
pub fn checked<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    checked_as("Comparison too deep", f)
}

/// The same, with jq's message for what went too deep (`Equality check too deep` for `==`).
pub fn checked_as<T>(message: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    TOO_DEEP.set(false);
    let result = f();
    if TOO_DEEP.replace(false) {
        return Err(Value::string(message));
    }
    result
}

impl Value {
    pub fn from_bool(b: bool) -> Value {
        if b { Value::True } else { Value::False }
    }

    pub fn string(s: impl Into<String>) -> Value {
        Value::String(Rc::new(s.into()))
    }

    pub fn array(items: Vec<Value>) -> Value {
        Value::Array(Rc::new(items))
    }

    /// A number read from `text` (JSON, the filter or `tonumber`), keeping its literal.
    pub fn literal(decimal: number::Decimal) -> Value {
        match number::literal(decimal) {
            Literal::Exact(d) => Value::Exact(d),
            Literal::Text(text) => Value::Literal(Rc::new(Text::new(text.into()))),
            Literal::Infinite(d) => Value::infinite_literal(d),
        }
    }

    /// A number of JSON's input, from its text (choices 1824, 1837): a whole number of at most 15
    /// digits is its double; one written as decNumber writes it keeps its text as it is; any other
    /// is made decNumber's form. `None` when it is not a number.
    pub fn read_number(text: &str) -> Option<Value> {
        if let Some(d) = number::small_whole(text.as_bytes()) {
            return Some(Value::Exact(d));
        }
        if number::written_as_decnumber(text.as_bytes()) {
            return Some(Value::Literal(Rc::new(Text::new(text.into()))));
        }
        number::Decimal::parse(text.as_bytes()).map(Value::literal)
    }

    /// decNumber's infinity, which jq writes as the largest double.
    pub fn infinite_literal(d: f64) -> Value {
        Value::Literal(Rc::new(Text { value: Cell::new(Some(d)), text: number::double_text(d).into(), infinite: true }))
    }

    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Null | Value::False)
    }

    pub fn number(&self) -> Option<f64> {
        match self {
            Value::Number(d) | Value::Exact(d) => Some(*d),
            Value::Literal(l) => Some(l.double()),
            _ => None,
        }
    }

    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_) | Value::Exact(_) | Value::Literal(_))
    }

    /// The order of kinds in comparisons: null, false, true, numbers, texts, arrays, objects.
    fn rank(&self) -> u8 {
        match self {
            Value::Null => 0,
            Value::False => 1,
            Value::True => 2,
            Value::Number(_) | Value::Exact(_) | Value::Literal(_) => 3,
            Value::String(_) => 4,
            Value::Array(_) => 5,
            Value::Object(_) => 6,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::False | Value::True => "boolean",
            Value::Number(_) | Value::Exact(_) | Value::Literal(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    /// A bit for the kind, for the filters `numbers`, `strings` and the others.
    pub fn kind_bit(&self) -> u8 {
        match self.rank() {
            0 => KIND_NULL,
            1 | 2 => KIND_BOOLEAN,
            3 => KIND_NUMBER,
            4 => KIND_STRING,
            5 => KIND_ARRAY,
            _ => KIND_OBJECT,
        }
    }

    /// The number's literal, when it was read and not changed.
    fn literal_text(&self) -> Option<std::borrow::Cow<'_, str>> {
        match self {
            Value::Exact(d) => Some(number::double_text(*d).into()),
            Value::Literal(l) => Some((*l.text).into()),
            _ => None,
        }
    }

    /// A number negated: a literal keeps its literal, as jq 1.8 keeps it.
    pub fn negate(&self) -> Option<Value> {
        Some(match self {
            Value::Number(d) => Value::Number(-d),
            Value::Exact(d) => Value::Exact(if *d == 0.0 { 0.0 } else { -d }),
            Value::Literal(l) => {
                let text = number::negate_text(&l.text);
                let negative = text.starts_with('-');
                let value = l.value.get().map(|d| if negative { -d.abs() } else { d.abs() });
                Value::Literal(Rc::new(Text { value: Cell::new(value), text: text.into(), infinite: l.infinite }))
            }
            _ => return None,
        })
    }

    /// jq's order (`jv_cmp`). NaN is below every number, as jq has it. Deeper than jq compares,
    /// it gives `Equal` and marks the comparison too deep (see `checked`).
    pub fn compare(&self, other: &Value) -> Ordering {
        self.compare_at(other, 0)
    }

    fn compare_at(&self, other: &Value, depth: usize) -> Ordering {
        if depth > MAX_DEPTH {
            TOO_DEEP.set(true);
            return Ordering::Equal;
        }
        let (ra, rb) = (self.rank(), other.rank());
        if ra != rb {
            return ra.cmp(&rb);
        }
        match (self, other) {
            (Value::String(a), Value::String(b)) => a.as_bytes().cmp(b.as_bytes()),
            (Value::Array(a), Value::Array(b)) => {
                for (x, y) in a.iter().zip(b.iter()) {
                    let o = x.compare_at(y, depth + 1);
                    if o.is_ne() {
                        return o;
                    }
                }
                a.len().cmp(&b.len())
            }
            (Value::Object(a), Value::Object(b)) => {
                // jq compares the sorted names as an array one level down, then the values.
                if depth + 1 > MAX_DEPTH || (depth + 2 > MAX_DEPTH && !a.is_empty() && !b.is_empty()) {
                    TOO_DEEP.set(true);
                    return Ordering::Equal;
                }
                let ka = sorted_keys(a);
                let kb = sorted_keys(b);
                for (x, y) in ka.iter().zip(kb.iter()) {
                    let o = x.as_bytes().cmp(y.as_bytes());
                    if o.is_ne() {
                        return o;
                    }
                }
                let o = ka.len().cmp(&kb.len());
                if o.is_ne() {
                    return o;
                }
                for k in ka {
                    let o = a[k.as_str()].compare_at(&b[k.as_str()], depth + 1);
                    if o.is_ne() {
                        return o;
                    }
                }
                Ordering::Equal
            }
            _ if ra == 3 => compare_numbers(self, other),
            _ => Ordering::Equal,
        }
    }

    /// jq's equality (`jv_equal`): the same value held twice is equal at once, at any depth.
    pub fn equals(&self, other: &Value) -> bool {
        self.equals_at(other, 0)
    }

    fn equals_at(&self, other: &Value, depth: usize) -> bool {
        match (self, other) {
            (Value::Array(a), Value::Array(b)) => {
                Rc::ptr_eq(a, b)
                    || (depth <= MAX_DEPTH
                        && a.len() == b.len()
                        && a.iter().zip(b.iter()).all(|(x, y)| x.equals_at(y, depth + 1)))
                    || too_deep(depth)
            }
            (Value::Object(a), Value::Object(b)) => {
                Rc::ptr_eq(a, b)
                    || (depth <= MAX_DEPTH
                        && a.len() == b.len()
                        && a.iter().all(|(k, x)| b.get(k.0.as_str()).is_some_and(|y| x.equals_at(y, depth + 1))))
                    || too_deep(depth)
            }
            (Value::String(a), Value::String(b)) => a == b,
            _ => self.compare_at(other, depth).is_eq(),
        }
    }

    /// The order `sort` uses: jq's, but with NaN equal to NaN, so that it is a total order.
    pub fn sort_compare(&self, other: &Value) -> Ordering {
        if let (Some(a), Some(b)) = (self.number(), other.number())
            && a.is_nan() && b.is_nan() {
                return Ordering::Equal;
            }
        self.compare(other)
    }
}

fn compare_numbers(a: &Value, b: &Value) -> Ordering {
    let (da, db) = (a.number().unwrap(), b.number().unwrap());
    // jq compares a NaN as if it were null: below every number, and below NaN too.
    if da.is_nan() {
        return Ordering::Less;
    }
    if db.is_nan() {
        return Ordering::Greater;
    }
    if da == db {
        // Two literals whose doubles are the same are compared exactly.
        let infinite = |v: &Value| matches!(v, Value::Literal(l) if l.infinite);
        if (infinite(a) || infinite(b)) && a.literal_text().is_some() && b.literal_text().is_some() {
            let o = infinite(a).cmp(&infinite(b));
            return if da < 0.0 { o.reverse() } else { o };
        }
        // Two of at most 15 digits whose doubles are the same are the same number (choice 1837).
        let short = |v: &Value| match v {
            Value::Exact(_) => true,
            Value::Literal(l) => number::ordered_by_double(&l.text),
            _ => false,
        };
        if let (Some(ta), Some(tb)) = (a.literal_text(), b.literal_text())
            && !(short(a) && short(b)) {
                return number::compare_texts(&ta, &tb);
            }
        return Ordering::Equal;
    }
    if da < db { Ordering::Less } else { Ordering::Greater }
}

/// What an array or object made whole may give back of the room it grew by: up to this many members.
const TRIM: usize = 4096;

/// An array made whole gives back the room it grew by, when it is small.
pub fn trim_vec(items: &mut Vec<Value>) {
    if items.len() <= TRIM {
        items.shrink_to_fit();
    }
}

/// An object made whole gives back the room it grew by, when it is small.
pub fn trim_map(map: &mut Map) {
    if map.len() <= TRIM {
        map.shrink_to_fit();
    }
}

/// The names of an object sorted by their bytes, as jq sorts them.
pub fn sorted_keys(map: &Map) -> Vec<&Rc<String>> {
    let mut keys: Vec<&Rc<String>> = map.keys().map(|k| &k.0).collect();
    keys.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    keys
}
