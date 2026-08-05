//! Low-level Base58 decoding kernel.
//!
//! [`decode_slice`] is the zero-allocation primitive behind [`crate::Engine::decode_into`].
//! Prefer the [`crate::Engine`] methods unless you specifically need this unchecked entry point.

use crate::{Config, Error};

// ----------------------------------------------------------------------
// Constants & Lookups
// ----------------------------------------------------------------------

/// Base 58^10 (~4.3 * 10^17).
/// This fits in a u64, allowing us to process 10 characters per bignum iteration.
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

// ----------------------------------------------------------------------
// Arithmetic Helpers
// ----------------------------------------------------------------------

/// Multiplies the bignum by `multiplier` and adds `addend`.
///
/// `bignum = bignum * multiplier + addend`
///
/// Operates on Little Endian u64 digits. Returns true on success, false on overflow.
///
/// Every `as u64` truncation below discards only the high half of a 128-bit
/// product/carry that the surrounding arithmetic has already accounted for
/// (the high half is threaded through `carry`), so no bits that matter are lost.
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn bignum_mul_add(digits: &mut [u64], count: &mut usize, multiplier: u64, addend: u64) -> bool {
    let mut carry = u128::from(addend);
    let mul = u128::from(multiplier);
    let len = *count;

    // Standard schoolbook multiplication-with-carry (unrolled 2x)
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

    // Expand bignum if there is a remaining carry
    if carry > 0 {
        if len >= digits.len() {
            return false;
        }
        digits[len] = carry as u64;
        *count += 1;
    }
    true
}

/// Parses a chunk of Base58 characters into a u64 value.
/// Also calculates the effective multiplier (58^len) for that chunk.
#[inline]
fn parse_chunk(config: &Config, src: &[u8]) -> Result<(u64, u64), Error> {
    let mut value = 0u64;
    let mut multiplier = 1u64;
    let mut bad = 0u8;

    for &byte in src {
        // `byte as usize` is provably below 256, so this indexes without a check.
        let digit = config.decode_map[byte as usize];
        bad |= digit;

        value = value * 58 + u64::from(digit);
        multiplier *= 58;
    }

    if bad & 0x80 != 0 {
        return Err(Error::InvalidCharacter);
    }

    Ok((value, multiplier))
}

/// Parses exactly 10 Base58 characters into a single Base 58^10 digit.
///
/// The reduction tree is balanced rather than a Horner chain, so the ten lookups
/// and the multiplies they feed stay independent.
#[inline]
fn parse_chunk_10(config: &Config, src: &[u8; 10]) -> Result<u64, Error> {
    let map = &config.decode_map;
    let d0 = map[src[0] as usize];
    let d1 = map[src[1] as usize];
    let d2 = map[src[2] as usize];
    let d3 = map[src[3] as usize];
    let d4 = map[src[4] as usize];
    let d5 = map[src[5] as usize];
    let d6 = map[src[6] as usize];
    let d7 = map[src[7] as usize];
    let d8 = map[src[8] as usize];
    let d9 = map[src[9] as usize];

    if (d0 | d1 | d2 | d3 | d4 | d5 | d6 | d7 | d8 | d9) & 0x80 != 0 {
        return Err(Error::InvalidCharacter);
    }

    let v01 = u64::from(d0) * 58 + u64::from(d1);
    let v23 = u64::from(d2) * 58 + u64::from(d3);
    let v45 = u64::from(d4) * 58 + u64::from(d5);
    let v67 = u64::from(d6) * 58 + u64::from(d7);
    let v89 = u64::from(d8) * 58 + u64::from(d9);

    let v03 = v01 * 3364 + v23;
    let v47 = v45 * 3364 + v67;

    let v07 = v03 * 11_316_496 + v47;
    Ok(v07 * 3364 + v89)
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

    // Process full chunks of 10 characters (Base 58^10).
    // This reduces the bignum loop overhead by 10x.
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

    // Process remaining tail (1-9 characters)
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
/// The `val as u8` truncation below is the intended byte-at-a-time extraction of
/// a word being shifted right by 8 each iteration, not a lossy narrowing.
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn emit(bignum: &[u64], dst: &mut [u8]) -> Result<usize, Error> {
    // Convert Little Endian u64 words to a Big Endian byte stream.
    // We write backwards from the end of the destination buffer.
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

    // Skip leading zeros written by the loop (not the explicit leading zeros,
    // which the caller already placed ahead of this slice).
    let start = dst[out_idx..]
        .iter()
        .position(|&b| b != 0)
        .map_or(dst.len(), |off| out_idx + off);
    let length = dst.len() - start;

    // Move the valid payload to the start of the buffer (memmove)
    dst.copy_within(start.., 0);

    Ok(length)
}

/// Decodes the payload into the destination buffer.
/// Returns the number of bytes written.
#[inline]
fn decode_payload(config: &Config, src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
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
    // Scan 8 bytes at a time against a splatted zero character, then finish
    // byte-wise. Mirrors the vectorized skip on the encode side.
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

    // Write the zeros
    dst[..leading_zeros].fill(0);

    // Decode the rest (the payload)
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
