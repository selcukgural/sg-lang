//! The fewest digits of a fractional number in compiled code: Dragonbox (Junekey Jeon, 2020),
//! written here in C, with its tables of powers of ten made by the compiler.
//!
//! A number is printed with the fewest decimal digits that read back as the same number, and of
//! those the digits nearest to it (choice 496): JavaScript, Rust, Java and Python print the same
//! digits. Dragonbox finds them with one or two multiplications by a power of ten held in 128
//! bits (64 for a `float`), where the first version wrote the number with 1, 2, ... 17 digits
//! and read each back. The interpreter prints with Rust's own shortest digits (`{:e}`), so the
//! two agree by the rule, whatever algorithm each uses.
//!
//! The tables are only in a program that prints a fractional number (`SG_FLOATS`).

use std::fmt::Write as _;
use std::sync::OnceLock;

/// `floor(k * log2(10))` for the `k` the tables cover, as Dragonbox computes it.
fn floor_log2_pow10(k: i32) -> i32 {
    (k * 1741647) >> 19
}

/// `floor(e * log10(2))`.
fn floor_log10_pow2(e: i32) -> i32 {
    (e * 315653) >> 20
}

/// `floor(e * log10(2) - log10(4 / 3))`: the power of ten of a power of two, whose number
/// below is nearer than the one above.
fn floor_log10_pow2_minus_log10_4_over_3(e: i32) -> i32 {
    (e * 631305 - 261663) >> 21
}

/// The powers of ten a format needs: from its smallest binary exponent to its largest, the
/// `k` of both ways Dragonbox picks one (`kappa` is 2 for a `double`, 1 for a `float`).
fn range(least: i32, most: i32, kappa: i32) -> (i32, i32) {
    let mut low = i32::MAX;
    let mut high = i32::MIN;
    for e in least..=most {
        for k in [kappa - floor_log10_pow2(e), -floor_log10_pow2_minus_log10_4_over_3(e)] {
            low = low.min(k);
            high = high.max(k);
        }
    }
    (low, high)
}

/// A whole number of any size, its 32-bit parts from the lowest.
#[derive(Clone)]
struct Big(Vec<u32>);

impl Big {
    fn one() -> Big {
        Big(vec![1])
    }

    fn times(&mut self, by: u32) {
        let mut carry = 0u64;
        for part in &mut self.0 {
            let n = *part as u64 * by as u64 + carry;
            *part = n as u32;
            carry = n >> 32;
        }
        if carry != 0 {
            self.0.push(carry as u32);
        }
    }

    fn bits(&self) -> u32 {
        let top = *self.0.last().expect("never empty");
        32 * self.0.len() as u32 - top.leading_zeros()
    }

    fn bit(&self, at: u32) -> bool {
        self.0.get((at / 32) as usize).is_some_and(|part| part >> (at % 32) & 1 == 1)
    }

    fn twice_plus(&mut self, one: bool) {
        let mut carry = one as u32;
        for part in &mut self.0 {
            let next = *part >> 31;
            *part = *part << 1 | carry;
            carry = next;
        }
        if carry != 0 {
            self.0.push(carry);
        }
    }

    fn at_least(&self, other: &Big) -> bool {
        let len = self.0.len().max(other.0.len());
        for i in (0..len).rev() {
            let (a, b) = (self.0.get(i).copied().unwrap_or(0), other.0.get(i).copied().unwrap_or(0));
            if a != b {
                return a > b;
            }
        }
        true
    }

    fn minus(&mut self, other: &Big) {
        let mut borrow = 0i64;
        for i in 0..self.0.len() {
            let n = self.0[i] as i64 - other.0.get(i).copied().unwrap_or(0) as i64 - borrow;
            self.0[i] = n as u32;
            borrow = (n < 0) as i64;
        }
        while self.0.len() > 1 && self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    fn is_zero(&self) -> bool {
        self.0.iter().all(|&part| part == 0)
    }
}

/// Ten to the `k`, scaled by a power of two to `width` bits with the top one set, rounded
/// up: `ceil(10^k * 2^(width - 1 - floor(k * log2(10))))`. Gives the bits from the lowest, in
/// a `u128` (at most 128).
fn power_of_ten(k: i32, width: u32) -> u128 {
    let mut ten = Big::one();
    for _ in 0..k.unsigned_abs() {
        ten.times(10);
    }
    let shift = width as i32 - 1 - floor_log2_pow10(k);
    let (mut value, exact) = if k >= 0 {
        assert_eq!(ten.bits() as i32 - 1, floor_log2_pow10(k), "the formula of log2(10) holds for {k}");
        if shift >= 0 {
            let mut value = 0u128;
            for at in 0..ten.bits() {
                value |= (ten.bit(at) as u128) << (at as i32 + shift);
            }
            (value, true)
        } else {
            let drop = (-shift) as u32;
            let mut value = 0u128;
            for at in drop..ten.bits() {
                value |= (ten.bit(at) as u128) << (at - drop);
            }
            (value, (0..drop).all(|at| !ten.bit(at)))
        }
    } else {
        // 2^shift divided by 10^-k, one bit at a time; the quotient has `width` bits.
        let mut rest = Big(vec![0]);
        let mut value = 0u128;
        for at in (0..=shift).rev() {
            rest.twice_plus(at == shift);
            value <<= 1;
            if rest.at_least(&ten) {
                rest.minus(&ten);
                value |= 1;
            }
        }
        (value, rest.is_zero())
    };
    if !exact {
        value += 1;
    }
    assert!(value >> (width - 1) == 1 && (width == 128 || value >> width == 0), "ten to the {k} has {width} bits");
    value
}

/// The C tables of both formats, made once: `sg_tens128` for a `double` (high and low halves)
/// and `sg_tens64` for a `float`, each from its smallest `k`.
pub fn tables() -> &'static str {
    static TABLES: OnceLock<String> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut out = String::new();
        let (low, high) = range(-1074, 2046 - 1075, 2);
        let _ = writeln!(out, "#define SG_TENS128_LEAST ({low})");
        out.push_str("static const unsigned long long sg_tens128[][2] = {\n");
        for k in low..=high {
            let value = power_of_ten(k, 128);
            let _ = writeln!(out, "{{0x{:016x}ULL,0x{:016x}ULL}},", (value >> 64) as u64, value as u64);
        }
        out.push_str("};\n");
        let (low, high) = range(-149, 254 - 150, 1);
        let _ = writeln!(out, "#define SG_TENS64_LEAST ({low})");
        out.push_str("static const unsigned long long sg_tens64[] = {\n");
        for k in low..=high {
            let _ = writeln!(out, "0x{:016x}ULL,", power_of_ten(k, 64) as u64);
        }
        out.push_str("};\n");
        out
    })
}

/// Dragonbox in C, after the runtime: the decimal digits of a positive finite number as a
/// whole number and a power of ten. Ties between two numbers of the fewest digits go to the
/// even one. The interval of a number is closed when its binary digits end in 0, as reading
/// a number back rounds a tie to the even one.
pub const DRAGONBOX: &str = r#"
static inline int sg_log2_pow10(int k) { return (k * 1741647) >> 19; }
static inline int sg_log10_pow2(int e) { return (e * 315653) >> 20; }
static inline int sg_log10_pow2_shorter(int e) { return (e * 631305 - 261663) >> 21; }

/* A `double`: `f` times 2 to the `e`. */
static void sg_dragon64(uint64_t bits, uint64_t* digits, int* exp) {
    uint64_t fraction = bits & ((1ULL << 52) - 1);
    int biased = (int)(bits >> 52 & 0x7ff);
    int e = biased ? biased - 1075 : -1074;
    if (biased > 1 && fraction == 0) {
        /* A power of two: the number below is twice as near as the one above. */
        int minus_k = sg_log10_pow2_shorter(e);
        int beta = e + sg_log2_pow10(-minus_k);
        uint64_t c = sg_tens128[-minus_k - SG_TENS128_LEAST][0];
        uint64_t xi = (c - (c >> 54)) >> (11 - beta);
        uint64_t zi = (c + (c >> 53)) >> (11 - beta);
        if (!(e >= 2 && e <= 3)) xi++;
        uint64_t q = zi / 10;
        if (q * 10 >= xi) { *digits = q; *exp = minus_k + 1; return; }
        q = ((c >> (10 - beta)) + 1) / 2;
        if ((q & 1) && e == -77) q--;
        else if (q < xi) q++;
        *digits = q; *exp = minus_k;
        return;
    }
    uint64_t f = biased ? fraction | (1ULL << 52) : fraction;
    bool even = (f & 1) == 0;
    uint64_t two_f = f << 1;
    int minus_k = sg_log10_pow2(e) - 2;
    int beta = e + sg_log2_pow10(-minus_k);
    const unsigned long long* c = sg_tens128[-minus_k - SG_TENS128_LEAST];
    uint32_t delta = (uint32_t)(c[0] >> (63 - beta));
    uint64_t u = (two_f | 1) << beta;
    unsigned __int128 z = (unsigned __int128)u * c[0] + (((unsigned __int128)u * c[1]) >> 64);
    uint64_t zi = (uint64_t)(z >> 64);
    bool z_whole = (uint64_t)z == 0;
    uint64_t q = zi / 1000;
    uint32_t r = (uint32_t)(zi - q * 1000);
    if (r < delta) {
        if (r == 0 && z_whole && !even) { q--; r = 1000; goto small; }
    } else if (r > delta) {
        goto small;
    } else {
        /* On the edge: the fraction of the left end decides. */
        uint64_t x = two_f - 1;
        unsigned __int128 low = (unsigned __int128)x * c[1];
        uint64_t high = x * c[0] + (uint64_t)(low >> 64);
        bool parity = (high >> (64 - beta)) & 1;
        bool whole = ((high << beta) | ((uint64_t)low >> (64 - beta))) == 0;
        if (!(parity | (whole & even))) goto small;
    }
    *digits = q; *exp = minus_k + 3;
    return;
small:
    q *= 10;
    {
        uint32_t dist = r - (delta / 2) + 50;
        bool approx_parity = ((dist ^ 50) & 1) != 0;
        bool divisible = dist % 100 == 0;
        q += dist / 100;
        if (divisible) {
            unsigned __int128 low = (unsigned __int128)two_f * c[1];
            uint64_t high = two_f * c[0] + (uint64_t)(low >> 64);
            bool parity = (high >> (64 - beta)) & 1;
            bool whole = ((high << beta) | ((uint64_t)low >> (64 - beta))) == 0;
            if (parity != approx_parity) q--;
            else if ((q & 1) && whole) q--;
        }
    }
    *digits = q; *exp = minus_k + 2;
}

/* A `float`, with a 64-bit power of ten. */
static void sg_dragon32(uint32_t bits, uint64_t* digits, int* exp) {
    uint32_t fraction = bits & ((1u << 23) - 1);
    int biased = (int)(bits >> 23 & 0xff);
    int e = biased ? biased - 150 : -149;
    if (biased > 1 && fraction == 0) {
        int minus_k = sg_log10_pow2_shorter(e);
        int beta = e + sg_log2_pow10(-minus_k);
        uint64_t c = sg_tens64[-minus_k - SG_TENS64_LEAST];
        uint32_t xi = (uint32_t)((c - (c >> 25)) >> (40 - beta));
        uint32_t zi = (uint32_t)((c + (c >> 24)) >> (40 - beta));
        if (!(e >= 2 && e <= 3)) xi++;
        uint32_t q = zi / 10;
        if (q * 10 >= xi) { *digits = q; *exp = minus_k + 1; return; }
        q = ((uint32_t)(c >> (39 - beta)) + 1) / 2;
        if ((q & 1) && e == -35) q--;
        else if (q < xi) q++;
        *digits = q; *exp = minus_k;
        return;
    }
    uint32_t f = biased ? fraction | (1u << 23) : fraction;
    bool even = (f & 1) == 0;
    uint32_t two_f = f << 1;
    int minus_k = sg_log10_pow2(e) - 1;
    int beta = e + sg_log2_pow10(-minus_k);
    uint64_t c = sg_tens64[-minus_k - SG_TENS64_LEAST];
    uint32_t delta = (uint32_t)(c >> (63 - beta));
    uint32_t u = (two_f | 1) << beta;
    uint64_t z = (uint64_t)(((unsigned __int128)u * c) >> 32);
    uint32_t zi = (uint32_t)(z >> 32);
    bool z_whole = (uint32_t)z == 0;
    uint32_t q = zi / 100;
    uint32_t r = zi - q * 100;
    if (r < delta) {
        if (r == 0 && z_whole && !even) { q--; r = 100; goto small; }
    } else if (r > delta) {
        goto small;
    } else {
        uint64_t p = (uint64_t)(two_f - 1) * c;
        bool parity = (p >> (64 - beta)) & 1;
        bool whole = (uint32_t)(p >> (32 - beta)) == 0;
        if (!(parity | (whole & even))) goto small;
    }
    *digits = q; *exp = minus_k + 2;
    return;
small:
    q *= 10;
    {
        uint32_t dist = r - (delta / 2) + 5;
        bool approx_parity = ((dist ^ 5) & 1) != 0;
        bool divisible = dist % 10 == 0;
        q += dist / 10;
        if (divisible) {
            uint64_t p = (uint64_t)two_f * c;
            bool parity = (p >> (64 - beta)) & 1;
            bool whole = (uint32_t)(p >> (32 - beta)) == 0;
            if (parity != approx_parity) q--;
            else if ((q & 1) && whole) q--;
        }
    }
    *digits = q; *exp = minus_k + 1;
}

static void sg_dragon(double v, bool single, uint64_t* digits, int* exp) {
    if (single) {
        float f = (float)v;
        uint32_t bits;
        memcpy(&bits, &f, 4);
        sg_dragon32(bits & 0x7fffffffu, digits, exp);
    } else {
        uint64_t bits;
        memcpy(&bits, &v, 8);
        sg_dragon64(bits & 0x7fffffffffffffffULL, digits, exp);
    }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_powers() {
        assert_eq!(power_of_ten(0, 128), 1u128 << 127);
        assert_eq!(power_of_ten(1, 64), 0xa000_0000_0000_0000);
        // 10^-1 = 0.1, as 0xcccc...cd: rounded up.
        assert_eq!(power_of_ten(-1, 64), 0xcccc_cccc_cccc_cccd);
    }

    #[test]
    fn ranges_of_dragonbox() {
        assert_eq!(range(-1074, 971, 2), (-292, 326));
        assert_eq!(range(-149, 104, 1), (-31, 46));
    }
}
