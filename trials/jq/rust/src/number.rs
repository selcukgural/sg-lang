//! Numbers as jq 1.8.2 keeps and writes them.
//!
//! A number read (from the input or from the filter) keeps its literal, as jq keeps it in a decNumber:
//! written back with decNumber's to-scientific-string, compared exactly with another literal. Its
//! double is what decNumber gives when rounded to 17 digits and read by `strtod`, as jq gets it. A
//! number computed is a double, written in the fewest digits that read back the same, as jq's
//! `jvp_dtoa_fmt` writes it.

use std::cmp::Ordering;
use std::fmt::Write as _;

/// The largest adjusted exponent decNumber holds in jq's context; past it a literal is infinite.
const EMAX: i64 = 999_999_999;
/// The smallest exponent a coefficient may have in jq's context (Etiny); a number below it is
/// rounded to it.
const ETINY: i64 = -1_147_483_646;
/// An exponent is kept within this, so that adding to it never overflows.
const EXPONENT_LIMIT: i64 = 1 << 60;

/// A decimal number: its sign, the digits of its coefficient (ASCII, no leading zeros, "0" for
/// zero) and its exponent.
pub struct Decimal {
    pub negative: bool,
    pub digits: Vec<u8>,
    pub exponent: i64,
}

impl Decimal {
    /// Reads a decimal as decNumber reads one: a sign, digits with a point among or around them, and
    /// an exponent. `None` when it is not one (infinities and NaN are not read here).
    pub fn parse(text: &[u8]) -> Option<Decimal> {
        let mut at = 0;
        let mut negative = false;
        if let Some(&sign) = text.first()
            && (sign == b'-' || sign == b'+') {
                negative = sign == b'-';
                at = 1;
            }
        let mut digits = Vec::with_capacity(text.len());
        let mut seen = false;
        while at < text.len() && text[at].is_ascii_digit() {
            digits.push(text[at]);
            seen = true;
            at += 1;
        }
        let mut fraction = 0i64;
        if at < text.len() && text[at] == b'.' {
            at += 1;
            while at < text.len() && text[at].is_ascii_digit() {
                digits.push(text[at]);
                fraction += 1;
                seen = true;
                at += 1;
            }
        }
        if !seen {
            return None;
        }
        let mut exponent = 0i64;
        if at < text.len() && (text[at] == b'e' || text[at] == b'E') {
            at += 1;
            let mut minus = false;
            if at < text.len() && (text[at] == b'-' || text[at] == b'+') {
                minus = text[at] == b'-';
                at += 1;
            }
            let start = at;
            while at < text.len() && text[at].is_ascii_digit() {
                exponent = (exponent * 10 + i64::from(text[at] - b'0')).min(EXPONENT_LIMIT);
                at += 1;
            }
            if at == start {
                return None;
            }
            if minus {
                exponent = -exponent;
            }
        }
        if at != text.len() {
            return None;
        }
        let zeros = digits.iter().take_while(|&&d| d == b'0').count();
        digits.drain(..zeros.min(digits.len() - 1));
        Some(Decimal { negative, digits, exponent: exponent - fraction })
    }

    fn is_zero(&self) -> bool {
        self.digits == b"0"
    }

    /// The exponent of the first digit.
    fn adjusted(&self) -> i64 {
        self.exponent + self.digits.len() as i64 - 1
    }

    /// decNumber's to-scientific-string.
    pub fn to_text(&self) -> String {
        let digits = std::str::from_utf8(&self.digits).unwrap();
        let count = self.digits.len() as i64;
        let adjusted = self.adjusted();
        let mut text = String::with_capacity(self.digits.len() + 8);
        if self.negative {
            text.push('-');
        }
        if self.exponent <= 0 && adjusted >= -6 {
            if self.exponent == 0 {
                text.push_str(digits);
            } else {
                let point = count + self.exponent;
                if point > 0 {
                    text.push_str(&digits[..point as usize]);
                    text.push('.');
                    text.push_str(&digits[point as usize..]);
                } else {
                    text.push_str("0.");
                    for _ in 0..-point {
                        text.push('0');
                    }
                    text.push_str(digits);
                }
            }
        } else {
            text.push_str(&digits[..1]);
            if count > 1 {
                text.push('.');
                text.push_str(&digits[1..]);
            }
            let _ = write!(text, "E{}{}", if adjusted >= 0 { "+" } else { "-" }, adjusted.abs());
        }
        text
    }

    /// The double jq gives this decimal: rounded to 17 digits, then read.
    pub fn to_double(&self) -> f64 {
        let mut digits = self.digits.clone();
        let mut exponent = self.exponent;
        if digits.len() > 17 {
            let dropped = digits.len() as i64 - 17;
            digits = round(&digits, 17, true);
            exponent += dropped;
        }
        let mut text = String::with_capacity(digits.len() + 24);
        if self.negative {
            text.push('-');
        }
        text.push_str(std::str::from_utf8(&digits).unwrap());
        let _ = write!(text, "e{exponent}");
        text.parse::<f64>().unwrap_or(0.0)
    }
}

/// The first `keep` digits of `digits` rounded half to even, or half up (the caller moves the
/// exponent); keep may be 0 or less, when only whether it rounds up to one is left.
fn round(digits: &[u8], keep: i64, half_even: bool) -> Vec<u8> {
    if keep < 0 {
        return b"0".to_vec();
    }
    let keep = keep as usize;
    if keep >= digits.len() {
        return digits.to_vec();
    }
    let first = digits[keep];
    let rest_nonzero = digits[keep + 1..].iter().any(|&d| d != b'0');
    let last_odd = keep > 0 && (digits[keep - 1] - b'0') % 2 == 1;
    let up = first > b'5' || (first == b'5' && (!half_even || rest_nonzero || last_odd));
    let mut kept = digits[..keep].to_vec();
    if up {
        let mut at = kept.len();
        loop {
            if at == 0 {
                kept.insert(0, b'1');
                break;
            }
            at -= 1;
            if kept[at] == b'9' {
                kept[at] = b'0';
            } else {
                kept[at] += 1;
                break;
            }
        }
    }
    if kept.is_empty() {
        kept.push(b'0');
    }
    let zeros = kept.iter().take_while(|&&d| d == b'0').count();
    kept.drain(..zeros.min(kept.len() - 1));
    kept
}

/// What a literal is, as jq holds it.
pub enum Literal {
    /// A double that writes the literal the same way: nothing more need be kept.
    Exact(f64),
    /// The literal's text, as decNumber writes it, and its double.
    Text(f64, String),
    /// Too large for decNumber: its infinity.
    Infinite(f64),
}

/// The literal of a decimal read.
pub fn literal(mut decimal: Decimal) -> Literal {
    // A literal of at most 15 digits, without an exponent, that a double writes the same way: the
    // double is all it needs (most numbers read are such).
    let count = decimal.digits.len() as i64;
    if decimal.exponent <= 0 && count <= 15 {
        let exact = if decimal.is_zero() {
            decimal.exponent == 0
        } else {
            decimal.adjusted() >= -4 && (decimal.exponent == 0 || decimal.digits.last() != Some(&b'0'))
        };
        if exact {
            return Literal::Exact(decimal.to_double());
        }
    }
    if decimal.exponent < ETINY {
        // Rounded there half up, as jq's decNumber context rounds.
        let keep = count - (ETINY - decimal.exponent);
        decimal.digits = round(&decimal.digits, keep, false);
        decimal.exponent = ETINY;
    }
    if !decimal.is_zero() && decimal.adjusted() > EMAX {
        return Literal::Infinite(if decimal.negative { f64::NEG_INFINITY } else { f64::INFINITY });
    }
    Literal::Text(decimal.to_double(), decimal.to_text())
}

/// The text of a literal negated, as decNumber's minus gives it: zero loses its sign.
pub fn negate_text(text: &str) -> String {
    let zero = Decimal::parse(text.as_bytes()).is_some_and(|d| d.is_zero());
    if let Some(rest) = text.strip_prefix('-') {
        rest.to_string()
    } else if zero {
        text.to_string()
    } else {
        format!("-{text}")
    }
}

/// Two literals compared exactly, as decNumber compares them.
pub fn compare_texts(a: &str, b: &str) -> Ordering {
    let (Some(a), Some(b)) = (Decimal::parse(a.as_bytes()), Decimal::parse(b.as_bytes())) else {
        return Ordering::Equal;
    };
    let a_sign = if a.is_zero() { 0 } else if a.negative { -1 } else { 1 };
    let b_sign = if b.is_zero() { 0 } else if b.negative { -1 } else { 1 };
    if a_sign != b_sign || a_sign == 0 {
        return a_sign.cmp(&b_sign);
    }
    let magnitude = a.adjusted().cmp(&b.adjusted()).then_with(|| {
        let length = a.digits.len().max(b.digits.len());
        let digit = |d: &Decimal, i: usize| d.digits.get(i).copied().unwrap_or(b'0');
        (0..length).map(|i| digit(&a, i).cmp(&digit(&b, i))).find(|o| o.is_ne()).unwrap_or(Ordering::Equal)
    });
    if a_sign < 0 { magnitude.reverse() } else { magnitude }
}

/// A small buffer on the stack that `write!` writes into.
struct Small {
    bytes: [u8; 40],
    length: usize,
}

impl std::fmt::Write for Small {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let end = self.length + s.len();
        if end > self.bytes.len() {
            return Err(std::fmt::Error);
        }
        self.bytes[self.length..end].copy_from_slice(s.as_bytes());
        self.length = end;
        Ok(())
    }
}

/// Writes a double as jq 1.8.2 writes a number computed: NaN as `null`, infinities as the largest
/// double, the shortest digits that read back the same, with an exponent when the point would be 4
/// or more places left of the first digit or more than 15 right of the last.
pub fn write_double(out: &mut Vec<u8>, d: f64) {
    if d.is_nan() {
        out.extend_from_slice(b"null");
        return;
    }
    let d = d.clamp(-f64::MAX, f64::MAX);
    if d == 0.0 {
        out.extend_from_slice(if d.is_sign_negative() { b"-0" } else { b"0" });
        return;
    }
    let mut small = Small { bytes: [0; 40], length: 0 };
    if d.fract() == 0.0 && d.abs() < 1e16 {
        let _ = write!(small, "{}", d as i64);
        out.extend_from_slice(&small.bytes[..small.length]);
        return;
    }
    if d < 0.0 {
        out.push(b'-');
    }
    let _ = write!(small, "{:e}", d.abs());
    let text = &small.bytes[..small.length];
    let e = text.iter().position(|&c| c == b'e').unwrap();
    let mut digits = [0u8; 24];
    let mut count = 0;
    for &c in &text[..e] {
        if c != b'.' {
            digits[count] = c;
            count += 1;
        }
    }
    let digits = &digits[..count];
    let exponent: i64 = std::str::from_utf8(&text[e + 1..]).unwrap().parse().unwrap();
    let point = exponent + 1;
    let count = count as i64;
    if point <= -4 || point > count + 15 {
        out.push(digits[0]);
        if count > 1 {
            out.push(b'.');
            out.extend_from_slice(&digits[1..]);
        }
        out.push(b'e');
        out.push(if exponent < 0 { b'-' } else { b'+' });
        let magnitude = exponent.abs();
        if magnitude < 10 {
            out.push(b'0');
        }
        let mut small = Small { bytes: [0; 40], length: 0 };
        let _ = write!(small, "{magnitude}");
        out.extend_from_slice(&small.bytes[..small.length]);
    } else if point <= 0 {
        out.extend_from_slice(b"0.");
        for _ in 0..-point {
            out.push(b'0');
        }
        out.extend_from_slice(digits);
    } else if point >= count {
        out.extend_from_slice(digits);
        for _ in 0..point - count {
            out.push(b'0');
        }
    } else {
        out.extend_from_slice(&digits[..point as usize]);
        out.push(b'.');
        out.extend_from_slice(&digits[point as usize..]);
    }
}

/// A double as jq writes it, in a `String`.
pub fn double_text(d: f64) -> String {
    let mut out = Vec::new();
    write_double(&mut out, d);
    String::from_utf8(out).unwrap()
}
