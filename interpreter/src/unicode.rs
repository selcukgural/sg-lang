//! What a character is and what its case is, from the Unicode tables of `unicode_tables.rs`,
//! for the interpreter and, as C, for compiled code. Both read the same tables, so they give the
//! same answers whatever version of Unicode Rust's own `char` methods know (choice 636).
//!
//! Each code point has one record, found in three steps: `TOP` by the code shifted right by
//! nine, `MID` by the next five bits, `LEAF` by the last four. A record holds what the
//! character is (bits) and what its simple upper, lower and folded case add to its code. A
//! character whose full mapping is more than one character (`ß` to `SS`) or another character
//! than the simple one has a bit that sends the lookup to a short sorted table.
//!
//! `isLetter` is the general category L, `isNumeric` N, `isUpper` Lu, `isLower` Ll,
//! `isControl` Cc (choice 637). A character changes its case alone by the simple mapping
//! (`'ß'.toUpper()` is `'ß'`, choice 638); a text by the full one, with the lower case of `Σ`
//! at the end of a word `ς` (choice 639), as Rust's `to_uppercase` and `to_lowercase`. No
//! language is taken into account (choice 640). Folding for comparing without case is the
//! full one of CaseFolding.txt without the Turkic lines (choices 641, 642).

use crate::unicode_tables::*;
use std::fmt::Write as _;
use std::sync::OnceLock;

pub const LETTER: u16 = 1;
pub const NUMERIC: u16 = 2;
pub const UPPER: u16 = 4;
pub const LOWER: u16 = 8;
pub const CONTROL: u16 = 16;
const CASED: u16 = 32;
const IGNORABLE: u16 = 64;
const MANY_UPPER: u16 = 128;
const MANY_LOWER: u16 = 256;
const MANY_FOLD: u16 = 512;

fn record(c: char) -> (u16, i32, i32, i32) {
    let code = c as u32;
    let mid = (TOP[(code >> TOP_SHIFT) as usize] as usize) << MID_BITS;
    let leaf = (MID[mid | ((code >> LEAF_BITS) as usize & ((1 << MID_BITS) - 1))] as usize) << LEAF_BITS;
    RECORDS[LEAF[leaf | (code as usize & ((1 << LEAF_BITS) - 1))] as usize]
}

/// The general category of the character: its place in the list of `tools/unicode_tables.py`,
/// which is the order of the variants of `UnicodeCategory` (choice 1476).
pub fn category(c: char) -> u8 {
    (record(c).0 >> CATEGORY_SHIFT) as u8
}

/// The simple case folding: one character for one, from the C and S lines of CaseFolding.txt
/// (choice 1477).
pub fn simple_fold(c: char) -> char {
    moved(c, record(c).3)
}

/// Whether the character has one of the `bits`.
pub fn is(c: char, bits: u16) -> bool {
    record(c).0 & bits != 0
}

fn moved(c: char, by: i32) -> char {
    char::from_u32((c as i32 + by) as u32).expect("a case mapping gives a character")
}

/// The simple upper case: one character for one.
pub fn upper(c: char) -> char {
    moved(c, record(c).1)
}

pub fn lower(c: char) -> char {
    moved(c, record(c).2)
}

/// Which mapping: the delta of the record that holds it and the bit that sends it to a table.
#[derive(Clone, Copy)]
enum Case {
    Upper,
    Lower,
    Fold,
}

/// The full mapping of `c`, one to three characters, written to `out`.
fn map(c: char, case: Case, out: &mut String) {
    let (bits, up, low, fold) = record(c);
    let (special, table, delta): (u16, &[(u32, [u32; 3])], i32) = match case {
        Case::Upper => (MANY_UPPER, &SPECIAL_UPPER, up),
        Case::Lower => (MANY_LOWER, &SPECIAL_LOWER, low),
        Case::Fold => (MANY_FOLD, &SPECIAL_FOLD, fold),
    };
    if bits & special == 0 {
        out.push(moved(c, delta));
        return;
    }
    let at = table.binary_search_by_key(&(c as u32), |entry| entry.0).expect("a special mapping is in its table");
    for &code in table[at].1.iter().filter(|&&code| code != 0) {
        out.push(char::from_u32(code).expect("a case mapping gives a character"));
    }
}

/// Whether the first character that is not case-ignorable is cased (Final_Sigma's test).
fn cased_after_ignorable(mut chars: impl Iterator<Item = char>) -> bool {
    chars.find(|&c| !is(c, IGNORABLE)).is_some_and(|c| is(c, CASED))
}

/// `text.toUpper()`.
pub fn upper_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        map(c, Case::Upper, &mut out);
    }
    out
}

/// `text.toLower()`: `Σ` is `ς` after a cased letter (case-ignorable ones skipped) and not
/// before one.
pub fn lower_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (at, c) in text.char_indices() {
        if c == 'Σ' {
            let last = cased_after_ignorable(text[..at].chars().rev()) && !cased_after_ignorable(text[at + 2..].chars());
            out.push(if last { 'ς' } else { 'σ' });
        } else {
            map(c, Case::Lower, &mut out);
        }
    }
    out
}

/// `text.foldCase()`.
pub fn fold_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        map(c, Case::Fold, &mut out);
    }
    out
}

/// `a.equalsIgnoreCase(b)`: the same text once both are folded.
pub fn equals_ignore_case(a: &str, b: &str) -> bool {
    a == b || fold_text(a) == fold_text(b)
}

// ---------------------------------------------------------------------------------------------
// Normalization (UAX #15, choices 1599, 1611-1613): `text.normalize(form)` and
// `text.isNormalized(form)`. Each code point has a record of normalization, found in three steps
// as the one of case: its canonical combining class, the bits of the quick checks, and where its
// full decompositions (canonical and compatibility, each already in the canonical order) are in
// `NORM_DECOMPOSED`. The syllables of Hangul are decomposed and composed by Hangul's rule.
//
// A text is first looked at by the quick check: a character that is not Yes to it, or a mark
// whose class is lower than the one before it, ends the look. A text it does not end is the
// text normalized, and is copied as it is (ASCII is looked at a byte at a time). Otherwise the
// text up to the last starter that is Yes before that place is copied, and from there the text
// is normalized one piece at a time: a piece ends before a starter that is Yes, which nothing
// before it reorders or composes with, and from that starter the quick check looks again, what it
// says Yes to copied as it is (ICU's way: a text with a few characters to change is mostly copied). Each piece is decomposed, its marks put in the canonical
// order by a stable sort (never in quadratic time, however many marks follow one another,
// choice 1613), and composed again for C and KC.

const NFD_NO: u8 = 1;
const NFC_NO: u8 = 2;
const NFC_MAYBE: u8 = 4;
const NFKD_NO: u8 = 8;
const NFKC_NO: u8 = 16;
const NFKC_MAYBE: u8 = 32;
/// The character is the second of a pair that composes.
const COMBINES: u8 = 64;

const HANGUL_FIRST: u32 = 0xAC00;
const HANGUL_COUNT: u32 = 11172;
const HANGUL_L: u32 = 0x1100;
const HANGUL_V: u32 = 0x1161;
const HANGUL_T: u32 = 0x11A7;
const HANGUL_V_COUNT: u32 = 21;
const HANGUL_T_COUNT: u32 = 28;
const HANGUL_N_COUNT: u32 = HANGUL_V_COUNT * HANGUL_T_COUNT;

/// A character of a piece being normalized: its code, from bit 24 its class, and bit 21 when it
/// is the second of a pair that composes.
const PACKED_COMBINES: u32 = 1 << 21;
const PACKED_CODE: u32 = PACKED_COMBINES - 1;

fn norm_record(code: u32) -> (u8, u8, u16, u8, u16, u8) {
    let mid = (NORM_TOP[(code >> TOP_SHIFT) as usize] as usize) << MID_BITS;
    let leaf = (NORM_MID[mid | ((code >> LEAF_BITS) as usize & ((1 << MID_BITS) - 1))] as usize) << LEAF_BITS;
    NORM_RECORDS[NORM_LEAF[leaf | (code as usize & ((1 << LEAF_BITS) - 1))] as usize]
}

/// The four forms; `compose` for C and KC, `compat` for KC and KD.
#[derive(Clone, Copy)]
pub struct Form {
    pub compose: bool,
    pub compat: bool,
}

impl Form {
    /// The bits by which the quick check says a character may change: No, and Maybe for C and KC.
    fn stops(self) -> u8 {
        match (self.compose, self.compat) {
            (false, false) => NFD_NO,
            (false, true) => NFKD_NO,
            (true, false) => NFC_NO | NFC_MAYBE,
            (true, true) => NFKC_NO | NFKC_MAYBE,
        }
    }
}

/// Where the quick check ends: `None` when the text is the text normalized; otherwise the place of
/// the last starter that is Yes before the end (0 when there is none).
fn quick_end(text: &str, stops: u8) -> Option<usize> {
    let bytes = text.as_bytes();
    let (mut at, mut safe, mut last) = (0, 0, 0u8);
    while at < bytes.len() {
        if bytes[at] < 0x80 {
            safe = at;
            last = 0;
            at += 1;
            continue;
        }
        let c = text[at..].chars().next().expect("a character starts here");
        let (class, bits, ..) = norm_record(c as u32);
        if bits & stops != 0 || (class != 0 && class < last) {
            return Some(safe);
        }
        if class == 0 {
            safe = at;
        }
        last = class;
        at += c.len_utf8();
    }
    None
}

/// `text.normalize(form)`.
pub fn normalize(text: &str, form: Form) -> String {
    let stops = form.stops();
    let Some(start) = quick_end(text, stops) else { return text.to_string() };
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    out.push_str(&text[..start]);
    let mut piece = Vec::new();
    let mut at = start;
    while let Some(c) = text[at..].chars().next() {
        let code = c as u32;
        let (class, bits, canon_at, canon_len, compat_at, compat_len) = norm_record(code);
        if class == 0 && bits & stops == 0 && !piece.is_empty() {
            finish(&mut piece, form.compose, &mut out);
            // The quick check again from this starter: what it says Yes to is copied as it is.
            match quick_end(&text[at..], stops) {
                None => {
                    out.push_str(&text[at..]);
                    return out;
                }
                Some(0) => {}
                Some(yes) => {
                    out.push_str(&text[at..at + yes]);
                    at += yes;
                    continue;
                }
            }
        }
        at += c.len_utf8();
        if code.wrapping_sub(HANGUL_FIRST) < HANGUL_COUNT {
            let index = code - HANGUL_FIRST;
            piece.push(HANGUL_L + index / HANGUL_N_COUNT);
            piece.push(packed(HANGUL_V + index % HANGUL_N_COUNT / HANGUL_T_COUNT));
            if !index.is_multiple_of(HANGUL_T_COUNT) {
                piece.push(packed(HANGUL_T + index % HANGUL_T_COUNT));
            }
            continue;
        }
        let (at, count) = if form.compat { (compat_at, compat_len) } else { (canon_at, canon_len) };
        if count == 0 {
            piece.push(code | (u32::from(class) << 24) | if bits & COMBINES != 0 { PACKED_COMBINES } else { 0 });
        } else {
            piece.extend(NORM_DECOMPOSED[at as usize..at as usize + count as usize].iter().map(|&code| packed(code)));
        }
    }
    finish(&mut piece, form.compose, &mut out);
    out
}

/// A character of a decomposition with its class and whether it composes with one before it.
fn packed(code: u32) -> u32 {
    let (class, bits, ..) = norm_record(code);
    code | (u32::from(class) << 24) | if bits & COMBINES != 0 { PACKED_COMBINES } else { 0 }
}

/// A decomposed piece put in the canonical order, composed when asked, and written out.
fn finish(piece: &mut Vec<u32>, compose: bool, out: &mut String) {
    let mut at = 0;
    while at < piece.len() {
        if piece[at] >> 24 == 0 {
            at += 1;
            continue;
        }
        let end = piece[at..].iter().position(|&c| c >> 24 == 0).map_or(piece.len(), |n| at + n);
        if end - at > 1 {
            piece[at..end].sort_by_key(|&c| c >> 24);
        }
        at = end;
    }
    let mut kept = piece.len();
    if compose {
        kept = 0;
        // The last starter, and the class of the last character kept after it (none when it is
        // the one just before).
        let (mut starter, mut last): (Option<usize>, Option<u32>) = (None, None);
        for at in 0..piece.len() {
            let c = piece[at];
            let class = c >> 24;
            if let Some(first) = starter {
                if c & PACKED_COMBINES != 0 && last.is_none_or(|last| last < class) {
                    if let Some(made) = composed(piece[first] & PACKED_CODE, c & PACKED_CODE) {
                        piece[first] = made;
                        continue;
                    }
                }
            }
            if class == 0 {
                starter = Some(kept);
                last = None;
            } else {
                last = Some(class);
            }
            piece[kept] = c;
            kept += 1;
        }
    }
    for &c in &piece[..kept] {
        out.push(char::from_u32(c & PACKED_CODE).expect("normalization gives characters"));
    }
    piece.clear();
}

/// What `first` and `second` compose to, if they do.
fn composed(first: u32, second: u32) -> Option<u32> {
    if first.wrapping_sub(HANGUL_L) < 19 && second.wrapping_sub(HANGUL_V) < HANGUL_V_COUNT {
        return Some(HANGUL_FIRST + ((first - HANGUL_L) * HANGUL_V_COUNT + second - HANGUL_V) * HANGUL_T_COUNT);
    }
    if first.wrapping_sub(HANGUL_FIRST) < HANGUL_COUNT
        && (first - HANGUL_FIRST).is_multiple_of(HANGUL_T_COUNT)
        && second.wrapping_sub(HANGUL_T + 1) < HANGUL_T_COUNT - 1
    {
        return Some(first + second - HANGUL_T);
    }
    NORM_PAIRS.binary_search_by(|&(a, b, _)| (a, b).cmp(&(first, second))).ok().map(|at| NORM_PAIRS[at].2)
}

/// `text.isNormalized(form)`: by the quick check, and when it says Maybe, by the text normalized.
pub fn is_normalized(text: &str, form: Form) -> bool {
    let (no, maybe) = match (form.compose, form.compat) {
        (false, false) => (NFD_NO, 0),
        (false, true) => (NFKD_NO, 0),
        (true, false) => (NFC_NO, NFC_MAYBE),
        (true, true) => (NFKC_NO, NFKC_MAYBE),
    };
    let (mut last, mut unsure) = (0u8, false);
    for c in text.chars() {
        if c.is_ascii() {
            last = 0;
            continue;
        }
        let (class, bits, ..) = norm_record(c as u32);
        if bits & no != 0 || (class != 0 && class < last) {
            return false;
        }
        unsure |= bits & maybe != 0;
        last = class;
    }
    !unsure || normalize(text, form) == text
}

/// The tables and the functions of compiled code, after the runtime: only in a program that
/// calls a method that needs them.
pub fn c_runtime() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        let mut out = format!("\n/* The Unicode tables ({VERSION}): see src/unicode.rs and src/unicode_tables.rs. */\n");
        out.push_str("typedef struct { uint16_t bits; int32_t up, low, fold; } sg_uni_record;\n");
        out.push_str("static const sg_uni_record sg_uni_records[] = {");
        for (at, (bits, up, low, fold)) in RECORDS.iter().enumerate() {
            let _ = write!(out, "{}{{{bits},{up},{low},{fold}}},", if at % 8 == 0 { "\n" } else { "" });
        }
        out.push_str("\n};\n");
        let numbers = |out: &mut String, ctype: &str, name: &str, values: &mut dyn Iterator<Item = u32>| {
            let _ = write!(out, "static const {ctype} {name}[] = {{");
            for (at, value) in values.enumerate() {
                let _ = write!(out, "{}{value},", if at % 32 == 0 { "\n" } else { "" });
            }
            out.push_str("\n};\n");
        };
        numbers(&mut out, "uint8_t", "sg_uni_top", &mut TOP.iter().map(|&n| n as u32));
        numbers(&mut out, "uint16_t", "sg_uni_mid", &mut MID.iter().map(|&n| n as u32));
        numbers(&mut out, "uint8_t", "sg_uni_leaf", &mut LEAF.iter().map(|&n| n as u32));
        out.push_str("typedef struct { uint32_t code, to[3]; } sg_uni_special;\n");
        for (name, table) in [("upper", &SPECIAL_UPPER[..]), ("lower", &SPECIAL_LOWER[..]), ("fold", &SPECIAL_FOLD[..])] {
            let _ = write!(out, "static const sg_uni_special sg_uni_{name}[] = {{");
            for (at, (code, to)) in table.iter().enumerate() {
                let _ = write!(out, "{}{{{code},{{{},{},{}}}}},", if at % 6 == 0 { "\n" } else { "" }, to[0], to[1], to[2]);
            }
            let _ = writeln!(out, "\n}};\n#define SG_UNI_{}_COUNT {}", name.to_uppercase(), table.len());
        }
        let _ = writeln!(
            out,
            "#define SG_UNI_TOP_SHIFT {TOP_SHIFT}\n#define SG_UNI_MID_BITS {MID_BITS}\n#define SG_UNI_LEAF_BITS {LEAF_BITS}\n#define SG_UNI_CATEGORY_SHIFT {CATEGORY_SHIFT}"
        );
        out.push_str(C_FUNCTIONS);
        out
    })
}

/// The tables and the functions of normalization in compiled code, after the runtime: only in a
/// program that normalizes (choice 1611), apart from the tables of case.
pub fn c_normalization() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        let mut out = format!("\n/* The tables of normalization (Unicode {VERSION}): see src/unicode.rs and src/unicode_tables.rs. */\n");
        out.push_str("typedef struct { uint8_t ccc, bits; uint16_t canon_at; uint8_t canon_len, compat_len; uint16_t compat_at; } sg_norm_record;\n");
        out.push_str("static const sg_norm_record sg_norm_records[] = {");
        for (at, (class, bits, canon_at, canon_len, compat_at, compat_len)) in NORM_RECORDS.iter().enumerate() {
            let _ = write!(out, "{}{{{class},{bits},{canon_at},{canon_len},{compat_len},{compat_at}}},", if at % 8 == 0 { "\n" } else { "" });
        }
        out.push_str("\n};\n");
        let numbers = |out: &mut String, ctype: &str, name: &str, values: &mut dyn Iterator<Item = u32>| {
            let _ = write!(out, "static const {ctype} {name}[] = {{");
            for (at, value) in values.enumerate() {
                let _ = write!(out, "{}{value},", if at % 32 == 0 { "\n" } else { "" });
            }
            out.push_str("\n};\n");
        };
        numbers(&mut out, "uint8_t", "sg_norm_top", &mut NORM_TOP.iter().map(|&n| n as u32));
        numbers(&mut out, "uint16_t", "sg_norm_mid", &mut NORM_MID.iter().map(|&n| n as u32));
        numbers(&mut out, "uint16_t", "sg_norm_leaf", &mut NORM_LEAF.iter().map(|&n| n as u32));
        numbers(&mut out, "uint32_t", "sg_norm_decomposed", &mut NORM_DECOMPOSED.iter().copied());
        out.push_str("typedef struct { uint32_t first, second, made; } sg_norm_pair;\nstatic const sg_norm_pair sg_norm_pairs[] = {");
        for (at, (first, second, made)) in NORM_PAIRS.iter().enumerate() {
            let _ = write!(out, "{}{{{first},{second},{made}}},", if at % 8 == 0 { "\n" } else { "" });
        }
        let _ = writeln!(
            out,
            "\n}};\n#define SG_NORM_PAIR_COUNT {}\n#define SG_NORM_TOP_SHIFT {TOP_SHIFT}\n#define SG_NORM_MID_BITS {MID_BITS}\n#define SG_NORM_LEAF_BITS {LEAF_BITS}",
            NORM_PAIRS.len()
        );
        out.push_str(C_NORMALIZATION);
        out
    })
}

const C_NORMALIZATION: &str = r#"
#define SG_NORM_NFD_NO 1
#define SG_NORM_NFC_NO 2
#define SG_NORM_NFC_MAYBE 4
#define SG_NORM_NFKD_NO 8
#define SG_NORM_NFKC_NO 16
#define SG_NORM_NFKC_MAYBE 32
#define SG_NORM_COMBINES 64
#define SG_NORM_PACKED_COMBINES (1u << 21)
#define SG_NORM_PACKED_CODE (SG_NORM_PACKED_COMBINES - 1)
static inline const sg_norm_record* sg_norm(uint32_t c) {
    uint32_t mid = (uint32_t)sg_norm_top[c >> SG_NORM_TOP_SHIFT] << SG_NORM_MID_BITS;
    uint32_t leaf = (uint32_t)sg_norm_mid[mid | ((c >> SG_NORM_LEAF_BITS) & ((1u << SG_NORM_MID_BITS) - 1))] << SG_NORM_LEAF_BITS;
    return &sg_norm_records[sg_norm_leaf[leaf | (c & ((1u << SG_NORM_LEAF_BITS) - 1))]];
}
/* The bits by which the quick check says a character may change. */
static inline int sg_norm_stops(bool compose, bool compat) {
    return compose ? (compat ? SG_NORM_NFKC_NO | SG_NORM_NFKC_MAYBE : SG_NORM_NFC_NO | SG_NORM_NFC_MAYBE) : (compat ? SG_NORM_NFKD_NO : SG_NORM_NFD_NO);
}
/* Where the quick check ends: -1 when the text is the text normalized; otherwise the place of
   the last starter that is Yes before the end. */
static int64_t sg_norm_quick(sg_str s, int stops) {
    int64_t at = 0, safe = 0;
    int last = 0;
    while (at < s.len) {
        if (s.ptr[at] < 0x80) { safe = at; last = 0; at++; continue; }
        int n;
        const sg_norm_record* r = sg_norm(sg_decode(s.ptr + at, &n));
        if ((r->bits & stops) || (r->ccc && r->ccc < last)) return safe;
        if (!r->ccc) safe = at;
        last = r->ccc;
        at += n;
    }
    return -1;
}
/* A piece being normalized: each character with its class from bit 24 and, at bit 21, whether it
   is the second of a pair that composes. */
typedef struct { uint32_t* items; int64_t len, cap; } sg_norm_piece;
static inline void sg_norm_push(sg_norm_piece* p, uint32_t c) {
    if (p->len == p->cap) p->items = sg_grow(p->items, &p->cap, sizeof(uint32_t), _Alignof(uint32_t));
    p->items[p->len++] = c;
}
static inline uint32_t sg_norm_packed(uint32_t c) {
    const sg_norm_record* r = sg_norm(c);
    return c | ((uint32_t)r->ccc << 24) | ((r->bits & SG_NORM_COMBINES) ? SG_NORM_PACKED_COMBINES : 0);
}
/* What `first` and `second` compose to, or 0. */
static uint32_t sg_norm_composed(uint32_t first, uint32_t second) {
    if (first - 0x1100u < 19u && second - 0x1161u < 21u) return 0xAC00u + ((first - 0x1100u) * 21u + second - 0x1161u) * 28u;
    if (first - 0xAC00u < 11172u && (first - 0xAC00u) % 28u == 0 && second - 0x11A8u < 27u) return first + second - 0x11A7u;
    int64_t low = 0, high = SG_NORM_PAIR_COUNT;
    while (low < high) {
        int64_t mid = (low + high) / 2;
        const sg_norm_pair* p = &sg_norm_pairs[mid];
        if (p->first < first || (p->first == first && p->second < second)) low = mid + 1; else high = mid;
    }
    if (low < SG_NORM_PAIR_COUNT && sg_norm_pairs[low].first == first && sg_norm_pairs[low].second == second) return sg_norm_pairs[low].made;
    return 0;
}
/* A run of marks put in the order of their classes, those of one class kept in their order: by
   insertion when the run is short, by merging (in `spare`) when it is long, never in quadratic
   time. */
static void sg_norm_sort(uint32_t* a, int64_t n, sg_norm_piece* spare) {
    if (n <= 16) {
        for (int64_t i = 1; i < n; i++) {
            uint32_t x = a[i];
            int64_t j = i;
            while (j > 0 && (a[j - 1] >> 24) > (x >> 24)) { a[j] = a[j - 1]; j--; }
            a[j] = x;
        }
        return;
    }
    if (spare->cap < n) spare->items = sg_reserve(spare->items, &spare->cap, n, sizeof(uint32_t), _Alignof(uint32_t));
    uint32_t *from = a, *to = spare->items;
    for (int64_t width = 1; width < n; width *= 2) {
        for (int64_t lo = 0; lo < n; lo += 2 * width) {
            int64_t mid = lo + width < n ? lo + width : n, hi = lo + 2 * width < n ? lo + 2 * width : n;
            int64_t i = lo, j = mid, k = lo;
            while (i < mid && j < hi) to[k++] = (from[j] >> 24) < (from[i] >> 24) ? from[j++] : from[i++];
            while (i < mid) to[k++] = from[i++];
            while (j < hi) to[k++] = from[j++];
        }
        uint32_t* t = from; from = to; to = t;
    }
    if (from != a) memcpy(a, from, (size_t)n * sizeof(uint32_t));
}
/* A decomposed piece put in the canonical order, composed when asked, and written out. */
static void sg_norm_finish(sg_norm_piece* p, bool compose, sg_string* out, sg_norm_piece* spare) {
    uint32_t* items = p->items;
    for (int64_t at = 0; at < p->len; ) {
        if (!(items[at] >> 24)) { at++; continue; }
        int64_t end = at;
        while (end < p->len && (items[end] >> 24)) end++;
        if (end - at > 1) sg_norm_sort(items + at, end - at, spare);
        at = end;
    }
    int64_t kept = p->len;
    if (compose) {
        kept = 0;
        int64_t starter = -1;
        uint32_t last = 0; /* the class of the last character kept after the starter; 0 when none */
        for (int64_t at = 0; at < p->len; at++) {
            uint32_t c = items[at], class = c >> 24;
            if (starter >= 0 && (c & SG_NORM_PACKED_COMBINES) && (last == 0 || last < class)) {
                uint32_t made = sg_norm_composed(items[starter] & SG_NORM_PACKED_CODE, c & SG_NORM_PACKED_CODE);
                if (made) { items[starter] = made; continue; }
            }
            if (class == 0) { starter = kept; last = 0; } else last = class;
            items[kept++] = c;
        }
    }
    sg_room(out, kept * 4);
    for (int64_t at = 0; at < kept; at++) out->len += sg_utf8((char*)out->ptr + out->len, items[at] & SG_NORM_PACKED_CODE);
    p->len = 0;
}
/* `text.normalize(form)`: a new text. */
static sg_string sg_uni_normalize(sg_str s, bool compose, bool compat) {
    if (s.len == 0) return SG_LIT("", 0);
    sg_string out = { (uint8_t*)"", 0, 0 };
    int stops = sg_norm_stops(compose, compat);
    int64_t start = sg_norm_quick(s, stops);
    sg_room(&out, start < 0 ? s.len : s.len + s.len / 4);
    if (start < 0) {
        memcpy(out.ptr, s.ptr, (size_t)s.len);
        out.len = s.len;
        return out;
    }
    if (start) memcpy(out.ptr, s.ptr, (size_t)start);
    out.len = start;
    sg_norm_piece piece = { NULL, 0, 0 }, spare = { NULL, 0, 0 };
    for (int64_t at = start; at < s.len; ) {
        int n;
        uint32_t c = sg_decode(s.ptr + at, &n);
        const sg_norm_record* r = sg_norm(c);
        if (!r->ccc && !(r->bits & stops) && piece.len) {
            sg_norm_finish(&piece, compose, &out, &spare);
            /* The quick check again from this starter: what it says Yes to is copied as it is. */
            int64_t yes = sg_norm_quick((sg_str){ s.ptr + at, s.len - at }, stops);
            if (yes < 0) yes = s.len - at;
            if (yes > 0) {
                sg_room(&out, yes);
                memcpy(out.ptr + out.len, s.ptr + at, (size_t)yes);
                out.len += yes;
                at += yes;
                continue;
            }
        }
        at += n;
        if (c - 0xAC00u < 11172u) {
            uint32_t index = c - 0xAC00u;
            sg_norm_push(&piece, 0x1100u + index / 588u);
            sg_norm_push(&piece, sg_norm_packed(0x1161u + index % 588u / 28u));
            if (index % 28u) sg_norm_push(&piece, sg_norm_packed(0x11A7u + index % 28u));
            continue;
        }
        uint32_t count = compat ? r->compat_len : r->canon_len, from = compat ? r->compat_at : r->canon_at;
        if (!count) {
            sg_norm_push(&piece, c | ((uint32_t)r->ccc << 24) | ((r->bits & SG_NORM_COMBINES) ? SG_NORM_PACKED_COMBINES : 0));
            continue;
        }
        for (uint32_t i = 0; i < count; i++) sg_norm_push(&piece, sg_norm_packed(sg_norm_decomposed[from + i]));
    }
    sg_norm_finish(&piece, compose, &out, &spare);
    free(piece.items);
    free(spare.items);
    return out;
}
/* `text.isNormalized(form)`: by the quick check, and when it says Maybe, by the text normalized. */
static bool sg_uni_is_normalized(sg_str s, bool compose, bool compat) {
    int no = compose ? (compat ? SG_NORM_NFKC_NO : SG_NORM_NFC_NO) : (compat ? SG_NORM_NFKD_NO : SG_NORM_NFD_NO);
    int maybe = compose ? (compat ? SG_NORM_NFKC_MAYBE : SG_NORM_NFC_MAYBE) : 0;
    int last = 0;
    bool unsure = false;
    for (int64_t at = 0; at < s.len; ) {
        if (s.ptr[at] < 0x80) { last = 0; at++; continue; }
        int n;
        const sg_norm_record* r = sg_norm(sg_decode(s.ptr + at, &n));
        if ((r->bits & no) || (r->ccc && r->ccc < last)) return false;
        unsure |= (r->bits & maybe) != 0;
        last = r->ccc;
        at += n;
    }
    if (!unsure) return true;
    sg_string normal = sg_uni_normalize(s, compose, compat);
    bool same = normal.len == s.len && memcmp(normal.ptr, s.ptr, (size_t)s.len) == 0;
    if (normal.cap) free(normal.ptr);
    return same;
}
"#;

const C_FUNCTIONS: &str = r#"
#define SG_UNI_LETTER 1
#define SG_UNI_NUMERIC 2
#define SG_UNI_UPPER 4
#define SG_UNI_LOWER 8
#define SG_UNI_CONTROL 16
#define SG_UNI_CASED 32
#define SG_UNI_IGNORABLE 64
#define SG_UNI_SPECIAL_UPPER 128
#define SG_UNI_SPECIAL_LOWER 256
#define SG_UNI_SPECIAL_FOLD 512
static inline const sg_uni_record* sg_uni(uint32_t c) {
    uint32_t mid = (uint32_t)sg_uni_top[c >> SG_UNI_TOP_SHIFT] << SG_UNI_MID_BITS;
    uint32_t leaf = (uint32_t)sg_uni_mid[mid | ((c >> SG_UNI_LEAF_BITS) & ((1u << SG_UNI_MID_BITS) - 1))] << SG_UNI_LEAF_BITS;
    return &sg_uni_records[sg_uni_leaf[leaf | (c & ((1u << SG_UNI_LEAF_BITS) - 1))]];
}
static inline bool sg_uni_is(uint32_t c, int bits) { return (sg_uni(c)->bits & bits) != 0; }
static inline int32_t sg_uni_category(uint32_t c) { return (int32_t)(sg_uni(c)->bits >> SG_UNI_CATEGORY_SHIFT); }
static inline uint32_t sg_uni_upper1(uint32_t c) { return (uint32_t)((int32_t)c + sg_uni(c)->up); }
static inline uint32_t sg_uni_lower1(uint32_t c) { return (uint32_t)((int32_t)c + sg_uni(c)->low); }
static inline uint32_t sg_uni_fold1(uint32_t c) { return (uint32_t)((int32_t)c + sg_uni(c)->fold); }
/* The full mapping of `c` (0 upper, 1 lower, 2 fold): one to three characters into `out`. */
static int sg_uni_map(uint32_t c, int which, uint32_t out[3]) {
    const sg_uni_record* r = sg_uni(c);
    static const int bit[3] = { SG_UNI_SPECIAL_UPPER, SG_UNI_SPECIAL_LOWER, SG_UNI_SPECIAL_FOLD };
    if (!(r->bits & bit[which])) {
        out[0] = (uint32_t)((int32_t)c + (which == 0 ? r->up : which == 1 ? r->low : r->fold));
        return 1;
    }
    const sg_uni_special* table = which == 0 ? sg_uni_upper : which == 1 ? sg_uni_lower : sg_uni_fold;
    int low = 0, high = (which == 0 ? SG_UNI_UPPER_COUNT : which == 1 ? SG_UNI_LOWER_COUNT : SG_UNI_FOLD_COUNT) - 1;
    while (low < high) {
        int mid = (low + high) / 2;
        if (table[mid].code < c) low = mid + 1; else high = mid;
    }
    int n = 0;
    while (n < 3 && table[low].to[n]) { out[n] = table[low].to[n]; n++; }
    return n;
}
/* Final_Sigma: whether, going back from `at` (or on from it), the first character that is not
   case-ignorable is cased. */
static bool sg_uni_cased_before(const uint8_t* p, int64_t at) {
    while (at > 0) {
        int64_t start = at - 1;
        while (start > 0 && (p[start] & 0xC0) == 0x80) start--;
        int n;
        uint32_t c = sg_decode(p + start, &n);
        if (!sg_uni_is(c, SG_UNI_IGNORABLE)) return sg_uni_is(c, SG_UNI_CASED);
        at = start;
    }
    return false;
}
static bool sg_uni_cased_after(const uint8_t* p, int64_t at, int64_t len) {
    while (at < len) {
        int n;
        uint32_t c = sg_decode(p + at, &n);
        if (!sg_uni_is(c, SG_UNI_IGNORABLE)) return sg_uni_is(c, SG_UNI_CASED);
        at += n;
    }
    return false;
}
/* `text.toUpper()`, `toLower()`, `foldCase()`: a new text. An ASCII byte maps to one byte, so the
   room taken for the whole text at the start holds them; a character past ASCII takes up to
   three characters of four bytes, for which room is made before it is written. */
static sg_string sg_uni_text(sg_str s, int which) {
    if (s.len == 0) return SG_LIT("", 0);
    sg_string out = { (uint8_t*)"", 0, 0 };
    sg_room(&out, s.len);
    const uint8_t* p = s.ptr;
    for (int64_t at = 0; at < s.len; ) {
        uint8_t b = p[at];
        if (b < 0x80) {
            if (which == 0) b = (uint8_t)(b - (((unsigned)(b - 'a') < 26u) << 5));
            else b = (uint8_t)(b + (((unsigned)(b - 'A') < 26u) << 5));
            out.ptr[out.len++] = b;
            at++;
            continue;
        }
        int n;
        uint32_t c = sg_decode(p + at, &n), to[3];
        int count;
        if (which == 1 && c == 0x3A3) {
            bool last = sg_uni_cased_before(p, at) && !sg_uni_cased_after(p, at + n, s.len);
            to[0] = last ? 0x3C2 : 0x3C3;
            count = 1;
        } else {
            count = sg_uni_map(c, which, to);
        }
        at += n;
        sg_room(&out, 12 + (s.len - at));
        for (int i = 0; i < count; i++) out.len += sg_utf8((char*)out.ptr + out.len, to[i]);
    }
    return out;
}
/* The next folded character of `s` from `*at`, through a queue of the up to three a character
   folds to. */
typedef struct { sg_str s; int64_t at; uint32_t queue[3]; int head, count; } sg_uni_folder;
static inline bool sg_uni_fold_next(sg_uni_folder* f, uint32_t* out) {
    if (f->head < f->count) { *out = f->queue[f->head++]; return true; }
    if (f->at >= f->s.len) return false;
    uint8_t b = f->s.ptr[f->at];
    if (b < 0x80) {
        f->at++;
        *out = (uint32_t)(b + (((unsigned)(b - 'A') < 26u) << 5));
        return true;
    }
    int n;
    uint32_t c = sg_decode(f->s.ptr + f->at, &n);
    f->at += n;
    f->count = sg_uni_map(c, 2, f->queue);
    f->head = 1;
    *out = f->queue[0];
    return true;
}
/* `a.equalsIgnoreCase(b)`: the two folded one character after the other, nothing allocated. */
static bool sg_eq_case(sg_str a, sg_str b) {
    if (a.len == b.len && (a.len == 0 || memcmp(a.ptr, b.ptr, (size_t)a.len) == 0)) return true;
    sg_uni_folder x = { a, 0, { 0 }, 0, 0 }, y = { b, 0, { 0 }, 0, 0 };
    for (;;) {
        uint32_t c, d;
        bool more = sg_uni_fold_next(&x, &c), other = sg_uni_fold_next(&y, &d);
        if (more != other) return false;
        if (!more) return true;
        if (c != d) return false;
    }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases() {
        assert!(is('é', LETTER) && is('٣', NUMERIC) && is('É', UPPER) && !is('ª', LOWER));
        assert_eq!(upper('ß'), 'ß');
        assert_eq!(upper_text("straße"), "STRASSE");
        assert_eq!(lower_text("ΟΔΟΣ ΟΔΟΣ."), "οδος οδος.");
        assert_eq!(lower_text("Σ"), "σ");
        assert_eq!(lower_text("İ"), "i\u{307}");
        assert!(equals_ignore_case("Straße", "STRASSE"));
        assert!(!equals_ignore_case("İ", "i"));
    }

    /// Every line of Unicode's `NormalizationTest.txt` (`tests/normalization-suite`) through the
    /// interpreter's side, the invariants of its header each, and every other code point left
    /// as it is by all four forms; `check.sg` there does the same compiled.
    #[test]
    fn normalization_suite() {
        const C: Form = Form { compose: true, compat: false };
        const D: Form = Form { compose: false, compat: false };
        const KC: Form = Form { compose: true, compat: true };
        const KD: Form = Form { compose: false, compat: true };
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/normalization-suite/NormalizationTest.txt");
        let suite = std::fs::read_to_string(path).unwrap();
        let (mut lines, mut part1, mut listed) = (0, false, std::collections::HashSet::new());
        for line in suite.lines() {
            if let Some(part) = line.strip_prefix("@Part") {
                part1 = part.starts_with('1');
                continue;
            }
            let line = line.split('#').next().unwrap().trim();
            if line.is_empty() {
                continue;
            }
            let c: Vec<String> = line.split(';').take(5).map(|column| {
                column.split_whitespace().map(|code| char::from_u32(u32::from_str_radix(code, 16).unwrap()).unwrap()).collect()
            }).collect();
            if part1 {
                listed.insert(c[0].chars().next().unwrap());
            }
            for (at, source) in c.iter().enumerate() {
                let (nfc, nfd) = if at < 3 { (&c[1], &c[2]) } else { (&c[3], &c[4]) };
                assert_eq!(&normalize(source, C), nfc, "NFC of column {} of {line}", at + 1);
                assert_eq!(&normalize(source, D), nfd, "NFD of column {} of {line}", at + 1);
                assert_eq!(normalize(source, KC), c[3], "NFKC of column {} of {line}", at + 1);
                assert_eq!(normalize(source, KD), c[4], "NFKD of column {} of {line}", at + 1);
            }
            for form in [C, D, KC, KD] {
                for source in &c {
                    assert_eq!(is_normalized(source, form), normalize(source, form) == *source, "isNormalized of {source:?} in {line}");
                }
            }
            lines += 1;
        }
        assert_eq!(lines, 20034);
        for c in (0..0x110000).filter_map(char::from_u32).filter(|c| !listed.contains(c)) {
            let text = c.to_string();
            for form in [C, D, KC, KD] {
                assert_eq!(normalize(&text, form), text, "U+{:04X}", c as u32);
                assert!(is_normalized(&text, form), "U+{:04X}", c as u32);
            }
        }
    }

    /// Every code point through the interpreter's side: the same counts and sums as
    /// `tests/release/unicode_every.sg` prints compiled, which are those of Python's Unicode
    /// 17.0 (`unicodedata.category`, `upper`, `lower`, `casefold`).
    #[test]
    fn every_code_point() {
        let mix = |hash: i64, text: &str| text.chars().fold(hash, |h, c| h.wrapping_mul(31).wrapping_add(c as i64));
        let (mut counts, mut simple, mut up, mut low, mut fold) = ([0; 6], 0i64, 0i64, 0i64, 0i64);
        for c in (0..0x110000).filter_map(char::from_u32) {
            for (count, bits) in counts.iter_mut().zip([LETTER, NUMERIC, LETTER | NUMERIC, UPPER, LOWER, CONTROL]) {
                *count += is(c, bits) as i32;
            }
            simple = simple.wrapping_mul(31).wrapping_add(upper(c) as i64 * 3 + lower(c) as i64);
            let text = c.to_string();
            up = mix(up, &upper_text(&text));
            low = mix(low, &lower_text(&text));
            fold = mix(fold, &fold_text(&text));
        }
        assert_eq!(counts, [145672, 1924, 147596, 1886, 2283, 65]);
        assert_eq!(simple, -7616751936517192121);
        assert_eq!((up, low, fold), (3427389144985499900, 211420238425588086, 2474606087873800157));
    }
}
