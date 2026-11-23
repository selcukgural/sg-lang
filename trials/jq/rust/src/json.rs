//! The program's own value built straight from `serde_json`'s tokens (a `DeserializeSeed` of it),
//! with no tree of `serde_json`'s in between. With `arbitrary_precision`, `serde_json` gives each
//! number as its literal; nesting deeper than jq reads is refused as soon as it is met, so the
//! recursion of the reading never goes past jq's 10,000 levels.

use crate::number::Decimal;
use crate::value::{Key, Map, Value, trim_map, trim_vec};
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use std::fmt;
use std::rc::Rc;

/// jq's limit: a `[` or `{` opened when its parser holds this many is an error. jq's parser holds
/// each array and object, and the name of a member while its value is read.
const DEPTH: usize = 10_000;

/// The name under which `serde_json` (`arbitrary_precision`) hands a number's literal to a visitor,
/// as a map of one member.
const NUMBER: &str = "$serde_json::private::Number";

pub const TOO_DEEP: &str = "Exceeds depth limit for parsing";

/// One value read from the stream.
pub struct Read(pub Value);

impl<'de> de::Deserialize<'de> for Read {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Read, D::Error> {
        Seed { depth: 0 }.deserialize(deserializer).map(Read)
    }
}

/// A value read at a depth of jq's parser.
#[derive(Clone, Copy)]
struct Seed {
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for Seed {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Seed {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_bool<E>(self, b: bool) -> Result<Value, E> {
        Ok(Value::from_bool(b))
    }

    // `serde_json` gives a whole number that fits 64 bits as one; its literal is its digits.
    fn visit_u64<E>(self, n: u64) -> Result<Value, E> {
        Ok(whole(n as i128))
    }

    fn visit_i64<E>(self, n: i64) -> Result<Value, E> {
        Ok(whole(n as i128))
    }

    fn visit_str<E>(self, s: &str) -> Result<Value, E> {
        Ok(Value::string(s))
    }

    fn visit_string<E>(self, s: String) -> Result<Value, E> {
        Ok(Value::String(Rc::new(s)))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        if self.depth >= DEPTH {
            return Err(de::Error::custom(TOO_DEEP));
        }
        let inner = Seed { depth: self.depth + 1 };
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(item) = seq.next_element_seed(inner)? {
            items.push(item);
        }
        trim_vec(&mut items);
        Ok(Value::Array(Rc::new(items)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let first = map.next_key_seed(NameSeed)?;
        if let Some(Name::Number) = first {
            return map.next_value_seed(NumberSeed);
        }
        if self.depth >= DEPTH {
            return Err(de::Error::custom(TOO_DEEP));
        }
        let inner = Seed { depth: self.depth + 2 };
        let mut members = Map::new();
        let mut name = first;
        while let Some(n) = name {
            let key = match n {
                Name::Text(s) => s,
                Name::Number => NUMBER.to_string(),
            };
            let value = map.next_value_seed(inner)?;
            // A name given twice keeps its first place and its last value, as jq keeps it.
            members.insert(Key(Rc::new(key)), value);
            name = map.next_key_seed(NameSeed)?;
        }
        trim_map(&mut members);
        Ok(Value::Object(Rc::new(members)))
    }
}

/// A whole number read: up to 15 digits its double writes it the same way; past that its literal is
/// kept.
fn whole(n: i128) -> Value {
    if n.unsigned_abs() < 1_000_000_000_000_000 {
        Value::Exact(n as f64)
    } else {
        Value::literal(Decimal::parse(n.to_string().as_bytes()).expect("digits"))
    }
}

/// The name of a member, or the mark of a number's literal.
enum Name {
    Text(String),
    Number,
}

struct NameSeed;

impl<'de> DeserializeSeed<'de> for NameSeed {
    type Value = Name;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Name, D::Error> {
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for NameSeed {
    type Value = Name;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("the name of a member")
    }

    fn visit_str<E>(self, s: &str) -> Result<Name, E> {
        Ok(if s == NUMBER { Name::Number } else { Name::Text(s.to_string()) })
    }

    fn visit_string<E>(self, s: String) -> Result<Name, E> {
        Ok(if s == NUMBER { Name::Number } else { Name::Text(s) })
    }
}

/// A number's literal, read into the program's number.
struct NumberSeed;

impl<'de> DeserializeSeed<'de> for NumberSeed {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for NumberSeed {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a number's literal")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<Value, E> {
        Decimal::parse(s.as_bytes()).map(Value::literal).ok_or_else(|| E::custom("Invalid numeric literal"))
    }
}
