//! Whether the arms of a `match` take every value, and whether an arm can still run, when
//! the patterns say more than one variant, number or text each: patterns inside patterns
//! (`Add(Number(a), _)`), `or`, ranges (`'a'..='z'`), `null`, `true` and `false`.
//!
//! The arms are rows of a matrix and a value is a row too; a row is *useful* when some value
//! matches it and no row above it (Maranget, "Warnings for pattern matching", 2007). An arm
//! that is not useful never runs; a `match` takes every value when `_` is not useful after
//! the arms. When a row is useful, the search gives back a value that shows it, which is the
//! value named in the message.
//!
//! `bool` is an enum of `false` and `true` here, and a `T?` an enum of `null` and a value of
//! `T`. Numbers and characters are ranges: the values of their type are cut where the ranges
//! in the arms begin and end, and each piece is then taken or not as a whole.

use std::rc::Rc;

/// A pattern as the search sees it: only what decides which values it takes.
#[derive(Clone)]
pub enum Pat {
    /// `_` or a name.
    Any,
    /// A text.
    Text(String),
    /// The numbers (or character codes) from `low` to `high`, both taken, of the type `of`.
    Num { low: i128, high: i128, of: Rc<Domain> },
    /// A variant of an enum and the patterns for what it carries.
    Variant { of: Rc<Enum>, index: usize, args: Vec<Pat> },
    /// A value that one of the patterns takes.
    Or(Vec<Pat>),
}

/// The values of an integer type or of `char`.
pub struct Domain {
    /// `int`, `byte`, `char`: what the type is called in a message.
    pub name: String,
    /// The values, as ranges in order; `char` leaves out the surrogates.
    pub ranges: Vec<(i128, i128)>,
    pub chars: bool,
}

/// The variants of an enum, by name, with the number of values each carries. A variant
/// without a name is the value of a `T?` that is not `null`; it carries that value.
pub struct Enum {
    pub names: Vec<String>,
    pub arities: Vec<usize>,
}

/// What the first pattern of a row tests for.
enum Head<'p> {
    Text(&'p str),
    Num(i128, i128),
    Variant(usize),
}

/// The rows with every `or` at their head opened into one row for each of its patterns.
fn open(rows: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
    let mut opened = Vec::with_capacity(rows.len());
    for row in rows {
        match &row[0] {
            Pat::Or(alternatives) => {
                let each: Vec<Vec<Pat>> = alternatives.iter().map(|alternative| std::iter::once(alternative.clone()).chain(row[1..].iter().cloned()).collect()).collect();
                opened.extend(open(&each));
            }
            _ => opened.push(row.clone()),
        }
    }
    opened
}

/// The rows that can match a value with this head, each with its first pattern replaced by
/// the patterns for what the value carries (`arity` of them). A range at the head is cut so
/// that each row's range holds it whole or not at all.
fn specialize(rows: &[Vec<Pat>], head: &Head, arity: usize) -> Vec<Vec<Pat>> {
    rows.iter()
        .filter_map(|row| {
            let carried = match (&row[0], head) {
                (Pat::Any, _) => vec![Pat::Any; arity],
                (Pat::Text(text), Head::Text(wanted)) if text == wanted => Vec::new(),
                (Pat::Num { low, high, .. }, Head::Num(from, to)) if low <= from && to <= high => Vec::new(),
                (Pat::Variant { index, args, .. }, Head::Variant(wanted)) if index == wanted => args.clone(),
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

/// The ranges at the heads of the rows.
fn ranges(rows: &[Vec<Pat>]) -> Vec<(i128, i128)> {
    rows.iter()
        .filter_map(|row| match row[0] {
            Pat::Num { low, high, .. } => Some((low, high)),
            _ => None,
        })
        .collect()
}

/// `target` cut where one of `ranges` begins or ends: each piece lies inside a range or
/// outside it.
fn cut(target: &[(i128, i128)], ranges: &[(i128, i128)]) -> Vec<(i128, i128)> {
    let mut cuts: Vec<i128> = ranges.iter().flat_map(|&(low, high)| [low, high + 1]).collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut pieces = Vec::new();
    for &(from, to) in target {
        let mut start = from;
        for &at in cuts.iter().filter(|&&at| from < at && at <= to) {
            pieces.push((start, at - 1));
            start = at;
        }
        pieces.push((start, to));
    }
    pieces
}

/// A value that `row` matches and none of `rows` does, if there is one.
pub fn useful(rows: &[Vec<Pat>], row: &[Pat]) -> Option<Vec<Pat>> {
    let Some(first) = row.first() else {
        return rows.is_empty().then(Vec::new);
    };
    let rows = &open(rows);
    let tail = &row[1..];
    let with_head = |head: Pat, found: Vec<Pat>| std::iter::once(head).chain(found).collect();
    match first {
        Pat::Or(alternatives) => alternatives.iter().find_map(|alternative| {
            let row: Vec<Pat> = std::iter::once(alternative.clone()).chain(tail.iter().cloned()).collect();
            useful(rows, &row)
        }),
        Pat::Text(text) => {
            let found = useful(&specialize(rows, &Head::Text(text), 0), tail)?;
            Some(with_head(first.clone(), found))
        }
        Pat::Num { low, high, of } => cut(&[(*low, *high)], &ranges(rows)).into_iter().find_map(|(from, to)| {
            let found = useful(&specialize(rows, &Head::Num(from, to), 0), tail)?;
            Some(with_head(Pat::Num { low: from, high: to, of: of.clone() }, found))
        }),
        Pat::Variant { of, index, args } => {
            let inner: Vec<Pat> = args.iter().chain(tail).cloned().collect();
            let found = useful(&specialize(rows, &Head::Variant(*index), args.len()), &inner)?;
            Some(rebuild(of, *index, found))
        }
        Pat::Any => {
            let variants = rows.iter().find_map(|row| match &row[0] {
                Pat::Variant { of, .. } => Some(of.clone()),
                _ => None,
            });
            let numbers = rows.iter().find_map(|row| match &row[0] {
                Pat::Num { of, .. } => Some(of.clone()),
                _ => None,
            });
            if let Some(of) = numbers {
                let held = ranges(rows);
                let pieces = cut(&of.ranges, &held);
                let taken = |&(from, to): &(i128, i128)| held.iter().any(|&(low, high)| low <= from && to <= high);
                return match pieces.iter().find(|piece| !taken(piece)) {
                    // Every piece has a row: the value is in one of them.
                    None => pieces.iter().find_map(|&(from, to)| {
                        let found = useful(&specialize(rows, &Head::Num(from, to), 0), tail)?;
                        Some(with_head(Pat::Num { low: from, high: to, of: of.clone() }, found))
                    }),
                    // A piece no row names: only the rows that take everything can match it.
                    Some(&(from, to)) => {
                        let found = useful(&rest(rows), tail)?;
                        Some(with_head(Pat::Num { low: from, high: to, of }, found))
                    }
                };
            }
            let seen = |index: usize| rows.iter().any(|row| matches!(&row[0], Pat::Variant { index: other, .. } if *other == index));
            match variants {
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
                    Some(with_head(head, found))
                }
            }
        }
    }
}

/// A value as it is named in a message: `Add(Number(_), _)`, `10..=int.MaxValue`, `null`.
pub fn show(pat: &Pat) -> String {
    match pat {
        Pat::Any => "_".to_string(),
        Pat::Text(text) => format!("{text:?}"),
        Pat::Num { low, high, of } => {
            let end = |value: i128| {
                let first = of.ranges.first().map(|range| range.0);
                let last = of.ranges.last().map(|range| range.1);
                match u32::try_from(value).ok().and_then(char::from_u32) {
                    Some(found) if of.chars => crate::interp::quoted_char(found),
                    // The ends of the wide types are long numbers; a name reads better.
                    _ if !of.chars && Some(value) == first && value < -32768 => format!("{}.MinValue", of.name),
                    _ if !of.chars && Some(value) == last && value > 65535 => format!("{}.MaxValue", of.name),
                    _ => value.to_string(),
                }
            };
            match low == high {
                true => end(*low),
                false => format!("{}..={}", end(*low), end(*high)),
            }
        }
        Pat::Variant { of, index, args } if of.names[*index].is_empty() => show(&args[0]),
        Pat::Variant { of, index, args } if args.is_empty() => of.names[*index].clone(),
        Pat::Variant { of, index, args } => {
            let args: Vec<String> = args.iter().map(show).collect();
            format!("{}({})", of.names[*index], args.join(", "))
        }
        Pat::Or(alternatives) => alternatives.iter().map(show).collect::<Vec<_>>().join(" or "),
    }
}
