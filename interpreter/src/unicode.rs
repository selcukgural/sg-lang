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
