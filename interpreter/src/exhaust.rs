//! Whether the arms of a `match` take every value, and whether an arm can still run, when
//! patterns are written inside patterns (`Add(Number(a), _)`).
//!
//! The arms are rows of a matrix and a value is a row too; a row is *useful* when some value
//! matches it and no row above it (Maranget, "Warnings for pattern matching", 2007). An arm
//! that is not useful never runs; a `match` takes every value when `_` is not useful after
//! the arms. When a row is useful, the search gives back a value that shows it, which is the
//! value named in the message.

use std::rc::Rc;

/// A pattern as the search sees it: only what decides which values it takes.
#[derive(Clone)]
pub enum Pat {
    /// `_` or a name.
    Any,
    /// A number, a character (by its code) or a text.
    Lit(Lit),
    /// A variant of an enum and the patterns for what it carries.
    Variant { of: Rc<Enum>, index: usize, args: Vec<Pat> },
}

#[derive(Clone, PartialEq)]
pub enum Lit {
    Num(i128),
    Text(String),
}

/// The variants of an enum, by name, with the number of values each carries.
pub struct Enum {
    pub names: Vec<String>,
    pub arities: Vec<usize>,
}

/// What the first pattern of a row tests for.
#[derive(PartialEq)]
enum Head<'p> {
    Lit(&'p Lit),
    Variant(usize),
}

/// The rows that can match a value with this head, each with its first pattern replaced by
/// the patterns for what the value carries (`arity` of them).
fn specialize(rows: &[Vec<Pat>], head: &Head, arity: usize) -> Vec<Vec<Pat>> {
    rows.iter()
        .filter_map(|row| {
            let carried = match &row[0] {
                Pat::Any => vec![Pat::Any; arity],
                Pat::Lit(lit) if *head == Head::Lit(lit) => Vec::new(),
                Pat::Variant { index, args, .. } if *head == Head::Variant(*index) => args.clone(),
                _ => return None,
            };
            Some(carried.into_iter().chain(row[1..].iter().cloned()).collect())
        })
        .collect()
}

/// The rows whose first pattern takes every value, without it.
fn rest(rows: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
    rows.iter().filter(|row| matches!(row[0], Pat::Any)).map(|row| row[1..].to_vec()).collect()
}

/// Puts the first `arity` values of `found` back under the variant they belong to.
fn rebuild(of: &Rc<Enum>, index: usize, mut found: Vec<Pat>) -> Vec<Pat> {
    let after = found.split_off(of.arities[index]);
    std::iter::once(Pat::Variant { of: of.clone(), index, args: found }).chain(after).collect()
}

/// A value that `row` matches and none of `rows` does, if there is one.
pub fn useful(rows: &[Vec<Pat>], row: &[Pat]) -> Option<Vec<Pat>> {
    let Some(first) = row.first() else {
        return rows.is_empty().then(Vec::new);
    };
    let tail = &row[1..];
    match first {
        Pat::Lit(lit) => {
            let found = useful(&specialize(rows, &Head::Lit(lit), 0), tail)?;
            Some(std::iter::once(first.clone()).chain(found).collect())
        }
        Pat::Variant { of, index, args } => {
            let inner: Vec<Pat> = args.iter().chain(tail).cloned().collect();
            let found = useful(&specialize(rows, &Head::Variant(*index), args.len()), &inner)?;
            Some(rebuild(of, *index, found))
        }
        Pat::Any => {
            let of = rows.iter().find_map(|row| match &row[0] {
                Pat::Variant { of, .. } => Some(of.clone()),
                _ => None,
            });
            let seen = |index: usize| rows.iter().any(|row| matches!(&row[0], Pat::Variant { index: other, .. } if *other == index));
            match of {
                // Every variant has a row: the value is one of them.
                Some(of) if (0..of.names.len()).all(seen) => (0..of.names.len()).find_map(|index| {
                    let inner: Vec<Pat> = std::iter::repeat_n(Pat::Any, of.arities[index]).chain(tail.iter().cloned()).collect();
                    let found = useful(&specialize(rows, &Head::Variant(index), of.arities[index]), &inner)?;
                    Some(rebuild(&of, index, found))
                }),
                // A value no head names: only the rows that take everything can match it.
                of => {
                    let found = useful(&rest(rows), tail)?;
                    let head = match of {
                        Some(of) => {
                            let index = (0..of.names.len()).find(|index| !seen(*index)).expect("a variant without a row");
                            Pat::Variant { args: vec![Pat::Any; of.arities[index]], of, index }
                        }
                        None => Pat::Any,
                    };
                    Some(std::iter::once(head).chain(found).collect())
                }
            }
        }
    }
}

/// A value as it is named in a message: `Add(Number(_), _)`.
pub fn show(pat: &Pat) -> String {
    match pat {
        Pat::Any => "_".to_string(),
        Pat::Lit(Lit::Num(value)) => value.to_string(),
        Pat::Lit(Lit::Text(text)) => format!("{text:?}"),
        Pat::Variant { of, index, args } if args.is_empty() => of.names[*index].clone(),
        Pat::Variant { of, index, args } => {
            let args: Vec<String> = args.iter().map(show).collect();
            format!("{}({})", of.names[*index], args.join(", "))
        }
    }
}
