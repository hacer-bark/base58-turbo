//! Low-level Base58 decoding kernel.
//!
//! [`decode_slice`] is the zero-allocation primitive behind [`crate::Engine::decode_into`].
//! Prefer the [`crate::Engine`] methods unless you specifically need this unchecked entry point.
//!
//! Both kernels below read characters through [`crate::Config::lut_58_pow`], which
//! stores `digit(b) * 58^k` for `k` in 0..4. A group of four characters is then a
//! plain sum of four table entries: the positional weight is already folded into
//! the lookup, so the group costs three adds instead of three multiplies, and the
//! sum doubles as the validity check (see [`crate::BAD_DIGIT`]). Only the
//! cross-group weights, which are larger than 58^3, are still multiplies.

use crate::{BAD_DIGIT, Config, Error};

// ----------------------------------------------------------------------
// Constants & Lookups
// ----------------------------------------------------------------------

/// 58^4, the weight of one full character group.
const P4: u64 = 11_316_496;

/// 58^k for k = 0..4, used to weight a partial group.
const POW58: [u64; 4] = [1, 58, 3364, 195_112];

/// 58^10, the largest power of 58 below 2^64: ten characters per bignum step.
const RADIX_58_10: u64 = 430_804_206_899_405_824;

// ----------------------------------------------------------------------
// Scratch Sizing
// ----------------------------------------------------------------------

const SMALL_WORDS: usize = 10; // <= 64 chars
const MEDIUM_WORDS: usize = 66; // <= 512 chars
const LARGE_WORDS: usize = 132; // <= 2048 chars

/// Hard cap on the decoded size, in u64 words (1024 bytes).
const MAX_WORDS: usize = 128;

const _: () = assert!(MAX_WORDS + 2 <= LARGE_WORDS, "large class too tight");

/// Any bit at or above this one in a group sum means the group held a character
/// outside the alphabet.
///
/// The group-of-four parse sums four `lut_58_pow` entries and tests against this
/// once, which is only sound while the three assertions below hold.
const GROUP_BAD: u32 = 1 << 26;
/// Largest sum four in-alphabet [`crate::Config::lut_58_pow`] entries can reach.
const GROUP_MAX: u32 = 4 * 57 * 195_112;

const _: () = assert!(
    GROUP_MAX < GROUP_BAD,
    "a valid group must not reach the test bit"
);
const _: () = assert!(
    BAD_DIGIT >= GROUP_BAD,
    "the sentinel must reach the test bit"
);
const _: () = assert!(
    4 * BAD_DIGIT as u64 + GROUP_MAX as u64 <= u32::MAX as u64,
    "a group sum must not wrap"
);

// ----------------------------------------------------------------------
// Arithmetic Helpers
// ----------------------------------------------------------------------

/// `digits = digits * multiplier + addend` over little-endian u64 words.
/// Returns false if the result does not fit `digits`.
///
/// The `as u64` truncations keep the low half of a 128-bit value whose high half
/// is carried on.
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn bignum_mul_add(digits: &mut [u64], count: &mut usize, multiplier: u64, addend: u64) -> bool {
    let mut carry = u128::from(addend);
    let mul = u128::from(multiplier);
    let len = *count;

    // Unrolled 2x.
    let mut chunks = digits[..len].chunks_exact_mut(2);
    for pair in chunks.by_ref() {
        let r0 = u128::from(pair[0]) * mul + carry;
        pair[0] = r0 as u64;
        let r1 = u128::from(pair[1]) * mul + (r0 >> 64);
        pair[1] = r1 as u64;
        carry = r1 >> 64;
    }
    if let [last] = chunks.into_remainder() {
        let result = u128::from(*last) * mul + carry;
        *last = result as u64;
        carry = result >> 64;
    }

    if carry > 0 {
        if len >= digits.len() {
            return false;
        }
        digits[len] = carry as u64;
        *count += 1;
    }
    true
}

/// Parses a tail of 1-9 Base58 characters, returning `(value, 58^len)`.
#[inline]
fn parse_chunk(config: &Config, src: &[u8]) -> Result<(u64, u64), Error> {
    let lut = &config.lut_58_pow;
    let mut bad = 0u32;
    let mut value = 0u64;
    let mut multiplier = 1u64;

    // Groups of four fold to three adds, since the weight is already in the table.
    let mut rest = src;
    while rest.len() >= 4 {
        let group = lut[3][rest[0] as usize]
            + lut[2][rest[1] as usize]
            + lut[1][rest[2] as usize]
            + lut[0][rest[3] as usize];
        bad |= group;
        value = value * P4 + u64::from(group);
        multiplier *= P4;
        rest = &rest[4..];
    }

    let left = rest.len();
    let mut acc = 0u32;
    for (i, &byte) in rest.iter().enumerate() {
        acc += lut[left - 1 - i][byte as usize];
    }
    bad |= acc;
    value = value * POW58[left] + u64::from(acc);
    multiplier *= POW58[left];

    if bad >= GROUP_BAD {
        return Err(Error::InvalidCharacter);
    }

    Ok((value, multiplier))
}

/// Parses exactly 10 Base58 characters into a single Base 58^10 digit.
///
/// The characters are read as two groups of four plus a pair. Because
/// [`Config::lut_58_pow`] already carries the 58^k factor, a group costs three
/// adds and no multiply, and the ten table entries validate as a group: see
/// [`BAD_DIGIT`].
#[inline]
fn parse_chunk_10(config: &Config, src: &[u8; 10]) -> Result<u64, Error> {
    let lut = &config.lut_58_pow;

    let g0 = lut[3][src[0] as usize]
        + lut[2][src[1] as usize]
        + lut[1][src[2] as usize]
        + lut[0][src[3] as usize];
    let g1 = lut[3][src[4] as usize]
        + lut[2][src[5] as usize]
        + lut[1][src[6] as usize]
        + lut[0][src[7] as usize];
    let g2 = lut[1][src[8] as usize] + lut[0][src[9] as usize];

    if (g0 | g1 | g2) >= GROUP_BAD {
        return Err(Error::InvalidCharacter);
    }

    Ok((u64::from(g0) * P4 + u64::from(g1)) * 3364 + u64::from(g2))
}

// ----------------------------------------------------------------------
// Core Logic
// ----------------------------------------------------------------------

/// Accumulates the Base58 payload into `bignum`, returning the live word count.
///
/// `max_words` is an explicit ceiling on the live word count, checked in addition
/// to `bignum`'s own capacity; callers that want no ceiling beyond `bignum`'s
/// actual size can pass `bignum.len()`.
#[inline]
fn accumulate(
    config: &Config,
    src: &[u8],
    bignum: &mut [u64],
    max_words: usize,
) -> Result<usize, Error> {
    let mut count = 1;

    let mut chunks = src.chunks_exact(10);
    for chunk in chunks.by_ref() {
        // `chunks_exact(10)` guarantees every `chunk` is exactly 10 bytes.
        let chunk: &[u8; 10] = chunk.try_into().unwrap_or_else(|_| unreachable!());
        let value = parse_chunk_10(config, chunk)?;

        if !bignum_mul_add(bignum, &mut count, RADIX_58_10, value) {
            return Err(Error::InputTooBig);
        }
        if count > max_words {
            return Err(Error::InputTooBig);
        }
    }

    let tail = chunks.remainder();
    if !tail.is_empty() {
        let (value, multiplier) = parse_chunk(config, tail)?;
        if !bignum_mul_add(bignum, &mut count, multiplier, value) {
            return Err(Error::InputTooBig);
        }
    }

    if count > max_words {
        return Err(Error::InputTooBig);
    }

    Ok(count)
}

/// Writes the accumulated bignum out as big-endian bytes, left-aligned in `dst`.
/// Returns the number of bytes written.
///
/// The `val as u8` truncation takes the low byte of a word being shifted out.
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn emit(bignum: &[u64], dst: &mut [u8]) -> Result<usize, Error> {
    // Written backwards from the end of `dst`.
    let mut out_idx = dst.len();
    let mut i = 0;

    // Whole words, one 8-byte store each. The bounds check is per word rather
    // than per byte, and only the final word can straddle the buffer start.
    while i < bignum.len() && out_idx >= 8 {
        out_idx -= 8;
        dst[out_idx..out_idx + 8].copy_from_slice(&bignum[i].to_be_bytes());
        i += 1;
    }

    // Fewer than 8 bytes of room left: write what fits, then whatever remains
    // must be zero or the buffer really is too small.
    if i < bignum.len() {
        let mut val = bignum[i];
        while out_idx > 0 {
            out_idx -= 1;
            dst[out_idx] = val as u8;
            val >>= 8;
        }
        if val > 0 || i + 1 < bignum.len() {
            return Err(Error::BufferTooSmall);
        }
    }

    // Strip the zero bytes above the value's top byte; explicit leading zeros
    // were already written by the caller, ahead of this slice.
    let start = dst[out_idx..]
        .iter()
        .position(|&b| b != 0)
        .map_or(dst.len(), |off| out_idx + off);
    let length = dst.len() - start;

    dst.copy_within(start.., 0);

    Ok(length)
}

// ----------------------------------------------------------------------
// Weight-matrix path (short payloads)
// ----------------------------------------------------------------------
//
// `accumulate` is a Horner chain: each `bignum_mul_add` waits on the one
// before it, so for a short payload the kernel spends most of its time on a
// dependency chain rather than on work. The alternative is to evaluate the
// whole radix conversion flat.
//
// Group the characters into `R` digits of four, each below 58^4 < 2^24, and
// give digit `r` the weight 58^(4*(R-1-r)) written out in base 2^32. The value
// is then a single sum:
//
//     value = sum_r d_r * 58^(4*(R-1-r))
//
// Column `j` takes at most `R` products of a 24-bit digit by a 32-bit limb, so
// it peaks at 24+32+log2(R) bits and fits a u64 with room to spare. Because the
// output radix is 2^32, normalizing the columns is carry-and-shift — decode
// never divides, which is the one place it has an easier job than encode.
//
// The weights are triangular (58^0 fills one limb, 58^20 fills four), and LLVM
// folds the zero entries away because the table is a constant.
//
// In isolation this beats the Horner loop out to about 40 characters, but more
// arms made `decode_payload` too big to inline well: through the public API, 24
// is the measured sweet spot (i7-8750H). Outlining the dispatch was slower still.
// Re-measure before moving `MAT_MAX_CHARS`.

/// Longest payload, in characters, handled by the matrix path.
const MAT_MAX_CHARS: usize = 24;
/// Exponents of 58^4 the weight table covers: up to 58^20 for 24 characters.
const MAT_MAXE: usize = 6;
/// Base-2^32 limbs in the widest value, 58^24 - 1.
const MAT_MAXL: usize = 5;

/// 58^(4e) in base 2^32, little-endian, for e = 0..`MAT_MAXE`.
///
/// Keyed by exponent rather than by `(R, row)` so every length shares one table
/// instead of monomorphizing its own copy.
#[allow(clippy::cast_possible_truncation)]
const fn mat_weights() -> [[u32; MAT_MAXL]; MAT_MAXE] {
    let mut table = [[0u32; MAT_MAXL]; MAT_MAXE];
    let mut cur = [0u32; MAT_MAXL];
    cur[0] = 1;
    let mut exp = 0;
    while exp < MAT_MAXE {
        let mut limb = 0;
        while limb < MAT_MAXL {
            table[exp][limb] = cur[limb];
            limb += 1;
        }
        // cur *= 58^4
        let mut carry = 0u64;
        let mut limb = 0;
        while limb < MAT_MAXL {
            let prod = cur[limb] as u64 * P4 + carry;
            cur[limb] = prod as u32;
            carry = prod >> 32;
            limb += 1;
        }
        exp += 1;
    }
    table
}

static MAT_W: [[u32; MAT_MAXL]; MAT_MAXE] = mat_weights();

/// Parses `C` characters into `R` base-58^4 digits, left-padded.
///
/// Only the first group can be partial. Group sums are OR-ed and validated once
/// at the end.
#[inline]
fn mat_parse<const C: usize, const R: usize>(
    config: &Config,
    src: &[u8],
) -> Result<[u32; R], Error> {
    debug_assert_eq!(src.len(), C);
    let lut = &config.lut_58_pow;
    let mut digits = [0u32; R];

    let head = C - 4 * (R - 1);
    let mut acc = 0u32;
    let mut idx = 0;
    while idx < head {
        acc += lut[head - 1 - idx][src[idx] as usize];
        idx += 1;
    }
    digits[0] = acc;
    let mut bad = acc;

    let mut row = 1;
    while row < R {
        let at = head + 4 * (row - 1);
        let group = lut[3][src[at] as usize]
            + lut[2][src[at + 1] as usize]
            + lut[1][src[at + 2] as usize]
            + lut[0][src[at + 3] as usize];
        bad |= group;
        digits[row] = group;
        row += 1;
    }

    if bad >= GROUP_BAD {
        return Err(Error::InvalidCharacter);
    }
    Ok(digits)
}

/// Multiplies `R` digits by the weight matrix and normalizes to base-2^32 limbs.
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn mat_multiply<const R: usize, const L: usize>(d: &[u32; R]) -> [u32; MAT_MAXL] {
    let mut acc = [0u64; MAT_MAXL];
    for (r, &dr) in d.iter().enumerate() {
        let x = u64::from(dr);
        let row = &MAT_W[R - 1 - r];
        for (a, &w) in acc[..L].iter_mut().zip(row[..L].iter()) {
            *a += x * u64::from(w);
        }
    }

    // Every column is under 2^60 and every carry under 2^28, so one pass is
    // enough: no second sweep, and nothing to divide by.
    let mut limb = [0u32; MAT_MAXL];
    let mut carry = 0u64;
    for (slot, &col) in limb[..L].iter_mut().zip(acc[..L].iter()) {
        let val = col + carry;
        carry = val >> 32;
        *slot = val as u32;
    }
    debug_assert_eq!(carry, 0);
    limb
}

/// Writes `limb` (little-endian base 2^32) into `dst` as big-endian bytes.
///
/// `NBMIN` is the shortest output any valid payload of this character count can
/// produce. Since 58 < 256 the longest is `NBMIN + 1`, so both likely copies
/// are constant-size. The general arm only serves a payload that still has
/// leading zero characters.
#[inline]
fn mat_emit<const L: usize, const NBMIN: usize>(
    limb: &[u32; MAT_MAXL],
    dst: &mut [u8],
) -> Result<usize, Error> {
    let mut be = [0u8; 4 * MAT_MAXL];
    for (chunk, &word) in be[..4 * L].chunks_exact_mut(4).zip(limb[..L].iter().rev()) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }

    let mut top = L - 1;
    while top > 0 && limb[top] == 0 {
        top -= 1;
    }
    let nb = 4 * top + 4 - (limb[top].leading_zeros() / 8) as usize;

    if nb > dst.len() {
        return Err(Error::BufferTooSmall);
    }
    if nb == NBMIN {
        dst[..NBMIN].copy_from_slice(&be[4 * L - NBMIN..4 * L]);
    } else if nb == NBMIN + 1 {
        dst[..=NBMIN].copy_from_slice(&be[4 * L - NBMIN - 1..4 * L]);
    } else {
        dst[..nb].copy_from_slice(&be[4 * L - nb..4 * L]);
    }
    Ok(nb)
}

#[inline]
fn mat_run<const C: usize, const R: usize, const L: usize, const NBMIN: usize>(
    config: &Config,
    src: &[u8],
    dst: &mut [u8],
) -> Result<usize, Error> {
    let d = mat_parse::<C, R>(config, src)?;
    let limb = mat_multiply::<R, L>(&d);
    mat_emit::<L, NBMIN>(&limb, dst)
}

/// Dispatches to a monomorphized kernel per character count.
///
/// The trip counts have to be compile-time for LLVM to unroll and to fold the
/// triangular zeros; passing `R` and `L` as runtime values measured about twice
/// as slow on the encode side of this crate.
macro_rules! mat_dispatch {
    ($cfg:expr, $src:expr, $dst:expr, $n:expr,
     $( ($C:literal, $R:literal, $L:literal, $NB:literal) ),* $(,)?) => {
        match $n {
            $( $C => return mat_run::<$C, $R, $L, $NB>($cfg, $src, $dst), )*
            _ => {}
        }
    };
}

/// Decodes a payload with no leading zero characters, returning the bytes written.
#[inline]
fn decode_payload(config: &Config, src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
    if src.len() <= MAT_MAX_CHARS {
        mat_dispatch!(
            config,
            src,
            dst,
            src.len(),
            (1, 1, 1, 1),
            (2, 1, 1, 1),
            (3, 1, 1, 2),
            (4, 1, 1, 3),
            (5, 2, 1, 3),
            (6, 2, 2, 4),
            (7, 2, 2, 5),
            (8, 2, 2, 6),
            (9, 3, 2, 6),
            (10, 3, 2, 7),
            (11, 3, 3, 8),
            (12, 3, 3, 9),
            (13, 4, 3, 9),
            (14, 4, 3, 10),
            (15, 4, 3, 11),
            (16, 4, 3, 11),
            (17, 5, 4, 12),
            (18, 5, 4, 13),
            (19, 5, 4, 14),
            (20, 5, 4, 14),
            (21, 6, 4, 15),
            (22, 6, 5, 16),
            (23, 6, 5, 17),
            (24, 6, 5, 17)
        );
    }

    // Scratch sized for the length class, so the zeroing cost stays proportional
    // to the work being done.
    match src.len() {
        n if n <= 64 => {
            let mut bignum = [0u64; SMALL_WORDS];
            let count = accumulate(config, src, &mut bignum, MAX_WORDS)?;
            emit(&bignum[..count], dst)
        }
        n if n <= 512 => {
            let mut bignum = [0u64; MEDIUM_WORDS];
            let count = accumulate(config, src, &mut bignum, MAX_WORDS)?;
            emit(&bignum[..count], dst)
        }
        _ => {
            let mut bignum = [0u64; LARGE_WORDS];
            let count = accumulate(config, src, &mut bignum, MAX_WORDS)?;
            emit(&bignum[..count], dst)
        }
    }
}

/// Counts the leading `zero_char` run in `input`, without writing anything.
#[inline]
fn count_leading_zeros(input: &[u8], zero_char: u8) -> usize {
    // Eight bytes at a time against a splatted zero character, then byte-wise.
    let z_pattern = 0x0101_0101_0101_0101_u64 * u64::from(zero_char);

    let mut leading_zeros = 0;
    while input.len() - leading_zeros >= 8 {
        // The `>= 8` guard above guarantees this 8-byte slice always exists.
        let word_bytes: [u8; 8] = input[leading_zeros..leading_zeros + 8]
            .try_into()
            .unwrap_or_else(|_| unreachable!());
        let word = u64::from_ne_bytes(word_bytes);
        if word != z_pattern {
            break;
        }
        leading_zeros += 8;
    }
    while leading_zeros < input.len() && input[leading_zeros] == zero_char {
        leading_zeros += 1;
    }
    leading_zeros
}

// ----------------------------------------------------------------------
// Entry Point
// ----------------------------------------------------------------------

/// Decodes `input` into `dst`, returning the number of bytes written.
///
/// This is the zero-allocation kernel behind [`crate::Engine::decode_into`]; unlike
/// that method it does not check `dst` is large enough up front, relying instead on
/// [`Error::BufferTooSmall`] from the underlying write. Capped at a 2048-byte input
/// (1024-byte decoded output) so its scratch can live on the stack; larger inputs
/// go through [`crate::Engine::decode`], which falls back to heap scratch.
///
/// # Errors
///
/// Returns [`Error::InputTooBig`] if `input` exceeds 2048 bytes or decodes to more
/// than 1024 bytes, [`Error::BufferTooSmall`] if `dst` is not large enough, or
/// [`Error::InvalidCharacter`] if `input` contains a character outside the alphabet.
#[inline]
pub fn decode_slice(input: &[u8], dst: &mut [u8], config: &Config) -> Result<usize, Error> {
    if input.len() > 2048 {
        return Err(Error::InputTooBig);
    }

    let leading_zeros = count_leading_zeros(input, config.alphabet[0]);

    if leading_zeros > 1024 {
        return Err(Error::InputTooBig);
    }

    if leading_zeros > dst.len() {
        return Err(Error::BufferTooSmall);
    }

    dst[..leading_zeros].fill(0);

    let src = &input[leading_zeros..];
    if src.is_empty() {
        return Ok(leading_zeros);
    }

    let written_payload = decode_payload(config, src, &mut dst[leading_zeros..])?;
    let total_len = leading_zeros + written_payload;

    if total_len > 1024 {
        return Err(Error::InputTooBig);
    }

    Ok(total_len)
}

/// Decodes `input` into `dst` using heap-allocated scratch, with no size limit.
///
/// This backs [`crate::Engine::decode`] for inputs larger than [`decode_slice`]'s
/// stack-scratch ceiling.
///
/// # Errors
///
/// Returns [`Error::BufferTooSmall`] if `dst` is not large enough, or
/// [`Error::InvalidCharacter`] if `input` contains a character outside the alphabet.
#[cfg(feature = "std")]
pub fn decode_slice_unbounded(
    input: &[u8],
    dst: &mut [u8],
    config: &Config,
) -> Result<usize, Error> {
    let leading_zeros = count_leading_zeros(input, config.alphabet[0]);

    if leading_zeros > dst.len() {
        return Err(Error::BufferTooSmall);
    }
    dst[..leading_zeros].fill(0);

    let src = &input[leading_zeros..];
    if src.is_empty() {
        return Ok(leading_zeros);
    }

    // Scratch sized generously for `src.len()`; mirrors the `LARGE_WORDS` formula,
    // scaled up instead of capped.
    let word_budget = (src.len() * 5859).div_ceil(1000).div_ceil(64) + 2;
    let mut bignum = vec![0u64; word_budget];
    let count = accumulate(config, src, &mut bignum, word_budget)?;
    let written_payload = emit(&bignum[..count], &mut dst[leading_zeros..])?;

    Ok(leading_zeros + written_payload)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::all,
        clippy::pedantic,
        clippy::nursery,
        clippy::cargo,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )]

    use super::*;
    use crate::BITCOIN;

    #[test]
    fn test_bignum_mul_add_carry() {
        let mut digits = [0u64; 10];
        let mut count = 1;
        // (0 * 58) + 1 = 1
        bignum_mul_add(&mut digits, &mut count, 58, 1);
        assert_eq!(count, 1);
        assert_eq!(digits[0], 1);

        // (u64::MAX * 2) + 1 => carry
        digits[0] = u64::MAX;
        count = 1;
        bignum_mul_add(&mut digits, &mut count, 2, 1);
        assert_eq!(count, 2);
        // (2^64-1)*2 + 1 = 2^65 - 2 + 1 = 2^65 - 1.
        // Low 64 bits: all ones (u64::MAX).
        // High 64 bits: 1.
        assert_eq!(digits[0], u64::MAX);
        assert_eq!(digits[1], 1);
    }

    /// A carry that has nowhere to go -- `digits` is already at capacity --
    /// must report failure rather than write out of bounds.
    #[test]
    fn test_bignum_mul_add_rejects_overflow_when_out_of_capacity() {
        let mut digits = [u64::MAX; 1];
        let mut count = 1;
        assert!(!bignum_mul_add(&mut digits, &mut count, 2, 1));
        assert_eq!(count, 1, "count must not advance on failure");
    }

    /// `mat_weights` backs the `MAT_W` static, which is normally only evaluated
    /// at compile time; call it directly so its body runs at runtime too.
    #[test]
    fn mat_weights_matches_static_table() {
        assert_eq!(mat_weights(), MAT_W);
    }

    /// Drives all four of `accumulate`'s `InputTooBig` returns with undersized
    /// scratch or a tight `max_words`; the capacity ones are unreachable through
    /// the public API.
    #[test]
    fn accumulate_rejects_overflow_at_every_check() {
        let config = BITCOIN.config();

        // Chunk-of-10 call site: pre-load the sole word so the multiply-add
        // overflows it, forcing a grow that a 1-word bignum has no room for.
        let mut bignum = [u64::MAX; 1];
        assert_eq!(
            accumulate(config, b"zzzzzzzzzz", &mut bignum, 128),
            Err(Error::InputTooBig)
        );

        // Tail call site: pre-load the sole word near capacity so the tail's
        // multiply-add carries into a word that does not exist.
        let mut bignum = [u64::MAX, 0, 0, 0, 0];
        assert_eq!(
            accumulate(config, b"z", &mut bignum[..1], 128),
            Err(Error::InputTooBig)
        );

        // Mid-loop `max_words` check: scratch has room, but the ceiling does not.
        let mut bignum = [u64::MAX, 0, 0, 0, 0];
        assert_eq!(
            accumulate(config, b"zzzzzzzzzz", &mut bignum, 1),
            Err(Error::InputTooBig)
        );

        // Final `max_words` check, reached only through the tail (no full
        // chunk-of-10 loop iteration) after a successful multiply-add.
        let mut bignum = [u64::MAX, 0, 0, 0, 0, 0];
        assert_eq!(
            accumulate(config, b"2", &mut bignum, 1),
            Err(Error::InputTooBig)
        );
    }

    /// `emit`'s final-word `BufferTooSmall` branch needs a bignum whose top
    /// word is nonzero but does not fit in the room left after whole words are
    /// written; the public decode path never leaves that little slack.
    #[test]
    fn emit_rejects_buffer_too_small_on_partial_final_word() {
        let bignum = [0x0102_0304_0506_0708u64, 1];
        let mut dst = [0u8; 8];
        assert_eq!(emit(&bignum, &mut dst), Err(Error::BufferTooSmall));
    }

    /// `mat_emit`'s general length branch is unreachable from `decode_payload`,
    /// which strips leading zero characters first; call it with a mismatched
    /// `NBMIN`.
    #[test]
    fn mat_emit_general_length_branch() {
        let limb = [0xFFFF_FFFFu32, 0, 0, 0, 0];
        let mut dst = [0u8; 8];
        let n = mat_emit::<2, 1>(&limb, &mut dst).unwrap();
        assert_eq!(n, 4);
        assert_eq!(dst[..4], [0xFF, 0xFF, 0xFF, 0xFF]);
    }

    /// `decode_slice`'s oversized-input guard is shadowed by
    /// `Engine::decode_into`'s own check; call the kernel directly.
    #[test]
    fn decode_slice_rejects_oversized_input_directly() {
        let mut dst = [0u8; 4096];
        assert_eq!(
            decode_slice(&[b'1'; 2049], &mut dst, BITCOIN.config()),
            Err(Error::InputTooBig)
        );
    }

    /// `decode_slice`'s leading-zero `BufferTooSmall` guard, isolated from
    /// `Engine::decode_into`'s earlier buffer check.
    #[test]
    fn decode_slice_rejects_buffer_too_small_for_leading_zeros() {
        let mut dst = [0u8; 2];
        assert_eq!(
            decode_slice(b"1111", &mut dst, BITCOIN.config()),
            Err(Error::BufferTooSmall)
        );
    }

    /// `decode_slice`'s `total_len > 1024` guard: the payload fits `MAX_WORDS`,
    /// but the leading zeros push the total past 1024 bytes.
    #[test]
    fn decode_slice_rejects_total_length_over_1024_bytes() {
        let text = "1".repeat(50) + &"z".repeat(1380);
        let mut dst = vec![0u8; text.len()];
        assert_eq!(
            decode_slice(text.as_bytes(), &mut dst, BITCOIN.config()),
            Err(Error::InputTooBig)
        );
    }

    #[test]
    fn decode_into_rejects_payload_over_1024_bytes() {
        // 1400 non-zero characters decode to well over 1024 bytes, while the
        // input itself is under the 2048-character `_into` ceiling.
        let text = "z".repeat(1400);
        let mut out = vec![0u8; text.len()];
        assert_eq!(
            BITCOIN.decode_into(&text, &mut out),
            Err(Error::InputTooBig)
        );
    }

    /// `decode_slice_unbounded`'s `BufferTooSmall` guard never fires through
    /// `Engine::decode`, which always sizes `dst` to at least the input length;
    /// call the kernel directly with a deliberately undersized buffer.
    #[test]
    #[cfg(feature = "std")]
    fn decode_slice_unbounded_rejects_buffer_too_small() {
        let mut dst = [0u8; 1];
        assert_eq!(
            decode_slice_unbounded(b"111", &mut dst, BITCOIN.config()),
            Err(Error::BufferTooSmall)
        );
    }

    #[test]
    fn test_parse_chunk_invalid() {
        let config = BITCOIN.config();
        let res = parse_chunk(config, b"10"); // '0' is invalid
        assert!(res.is_err());
    }

    #[test]
    fn test_parse_chunk_10_matches_parse_chunk() {
        let config = BITCOIN.config();
        for sample in [b"1111111111", b"zzzzzzzzzz", b"123456789A", b"AzBy1Cx2Dw"] {
            let a = parse_chunk_10(config, sample).unwrap();
            let b = parse_chunk(config, sample).unwrap().0;
            assert_eq!(a, b, "{}", core::str::from_utf8(sample).unwrap());
        }
        assert!(parse_chunk_10(config, b"1234567890").is_err()); // '0' is invalid
    }

    #[test]
    fn test_decode_payload_buffer_too_small() {
        let config = BITCOIN.config();
        let mut dst = [0u8; 1];
        // "222" -> 58*59 + 1 = 3423. Needs 2 bytes.
        let res = decode_payload(config, b"222", &mut dst);
        assert_eq!(res.unwrap_err(), Error::BufferTooSmall);
    }

    #[test]
    fn scratch_classes_are_large_enough() {
        let words = |n: usize| {
            let bits = (n * 5859).div_ceil(1000);
            bits.div_ceil(64) + 1
        };
        assert!(words(64) <= SMALL_WORDS, "small class too tight");
        assert!(words(512) <= MEDIUM_WORDS, "medium class too tight");
    }

    #[test]
    fn decode_class_boundaries_round_trip() {
        for len in [1usize, 40, 45, 46, 64, 100, 370, 375, 376, 1024] {
            let data: Vec<u8> = (0..len)
                .map(|i| (i as u8).wrapping_mul(53).wrapping_add(9))
                .collect();
            let encoded = BITCOIN.encode(&data).unwrap();
            assert_eq!(BITCOIN.decode(&encoded).unwrap(), data, "len {len}");
        }
    }
}
