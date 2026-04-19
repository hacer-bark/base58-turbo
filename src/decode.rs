use crate::{Config, Error};

// ----------------------------------------------------------------------
// Constants & Lookups
// ----------------------------------------------------------------------

/// Base 58^10 (~4.3 * 10^17).
/// This fits in a u64, allowing us to process 10 characters per bignum iteration.
const RADIX_58_10: u64 = 430_804_206_899_405_824;

// ----------------------------------------------------------------------
// Arithmetic Helpers
// ----------------------------------------------------------------------

/// Multiplies the bignum by `multiplier` and adds `addend`.
///
/// `bignum = bignum * multiplier + addend`
///
/// Operates on Little Endian u64 digits.
#[inline(always)]
unsafe fn bignum_mul_add(digits: &mut [u64], count: &mut usize, multiplier: u64, addend: u64) {
    let mut carry = addend as u128;
    let mul = multiplier as u128;
    let len = *count;

    // Standard schoolbook multiplication-with-carry (unrolled 2x)
    let mut i = 0;
    while i + 1 < len {
        let digit0 = *unsafe { digits.get_unchecked(i) };
        let result0 = (digit0 as u128) * mul + carry;
        *unsafe { digits.get_unchecked_mut(i) } = result0 as u64;
        let carry0 = result0 >> 64;

        let digit1 = *unsafe { digits.get_unchecked(i + 1) };
        let result1 = (digit1 as u128) * mul + carry0;
        *unsafe { digits.get_unchecked_mut(i + 1) } = result1 as u64;
        carry = result1 >> 64;

        i += 2;
    }
    if i < len {
        let digit = *unsafe { digits.get_unchecked(i) };
        let result = (digit as u128) * mul + carry;
        *unsafe { digits.get_unchecked_mut(i) } = result as u64;
        carry = result >> 64;
    }

    // Expand bignum if there is a remaining carry
    if carry > 0 {
        *unsafe { digits.get_unchecked_mut(len) } = carry as u64;
        *count += 1;
    }
}

/// Parses a chunk of Base58 characters into a u64 value.
/// Also calculates the effective multiplier (58^len) for that chunk.
#[inline(always)]
unsafe fn parse_chunk(config: &Config, src: &[u8]) -> Result<(u64, u64), Error> {
    let mut value = 0u64;
    let mut multiplier = 1u64;
    let mut bad = 0u8;

    for &byte in src {
        let digit = *unsafe { config.decode_map.get_unchecked(byte as usize) };
        bad |= digit;

        value = value * 58 + (digit as u64);
        multiplier *= 58;
    }

    if bad & 0x80 != 0 {
        return Err(Error::InvalidCharacter);
    }

    Ok((value, multiplier))
}

// ----------------------------------------------------------------------
// Core Logic
// ----------------------------------------------------------------------

/// Decodes the payload into the destination buffer.
/// Returns the number of bytes written.
#[inline(always)]
unsafe fn decode_payload(config: &Config, mut src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
    // 1. Accumulation Phase
    // Use a stack-allocated buffer for the bignum (Little Endian u64s).
    // 128 u64s = 1024 bytes, sufficient for very large inputs.
    // Bypass zero-initialization to avoid memset overhead.
    let mut bignum_uninit = core::mem::MaybeUninit::<[u64; 128]>::uninit();
    let bignum_ptr = bignum_uninit.as_mut_ptr() as *mut u64;
    unsafe { *bignum_ptr = 0; }
    let bignum = unsafe { &mut *bignum_uninit.as_mut_ptr() };
    let mut count = 1;

    // Process full chunks of 10 characters (Base 58^10)
    // This reduces the bignum loop overhead by 10x.
    while src.len() >= 10 {
        // Unroll parsing for the fixed chunk size
        let d0 = *unsafe { config.decode_map.get_unchecked(*src.get_unchecked(0) as usize) } as u64;
        let d1 = *unsafe { config.decode_map.get_unchecked(*src.get_unchecked(1) as usize) } as u64;
        let d2 = *unsafe { config.decode_map.get_unchecked(*src.get_unchecked(2) as usize) } as u64;
        let d3 = *unsafe { config.decode_map.get_unchecked(*src.get_unchecked(3) as usize) } as u64;
        let d4 = *unsafe { config.decode_map.get_unchecked(*src.get_unchecked(4) as usize) } as u64;
        let d5 = *unsafe { config.decode_map.get_unchecked(*src.get_unchecked(5) as usize) } as u64;
        let d6 = *unsafe { config.decode_map.get_unchecked(*src.get_unchecked(6) as usize) } as u64;
        let d7 = *unsafe { config.decode_map.get_unchecked(*src.get_unchecked(7) as usize) } as u64;
        let d8 = *unsafe { config.decode_map.get_unchecked(*src.get_unchecked(8) as usize) } as u64;
        let d9 = *unsafe { config.decode_map.get_unchecked(*src.get_unchecked(9) as usize) } as u64;

        let invalid = (d0 | d1 | d2 | d3 | d4 | d5 | d6 | d7 | d8 | d9) & 0x80;
        if invalid != 0 {
            return Err(Error::InvalidCharacter);
        }

        let v01 = d0 * 58 + d1;
        let v23 = d2 * 58 + d3;
        let v45 = d4 * 58 + d5;
        let v67 = d6 * 58 + d7;
        let v89 = d8 * 58 + d9;

        let v03 = v01 * 3364 + v23;
        let v47 = v45 * 3364 + v67;

        let v07 = v03 * 11316496 + v47;
        let chunk = v07 * 3364 + v89;

        unsafe { bignum_mul_add(bignum, &mut count, RADIX_58_10, chunk) };
        src = &src[10..];
    }

    // Process remaining tail (1-9 characters)
    if !src.is_empty() {
        let (chunk, multiplier) = unsafe { parse_chunk(config, src) }?;
        unsafe { bignum_mul_add(bignum, &mut count, multiplier, chunk) };
    }

    // 2. Emission Phase
    // Convert Little Endian u64 words to a Big Endian byte stream.
    // We write backwards from the end of the destination buffer.
    let mut out_idx = dst.len();

    for i in 0..count {
        let mut val = bignum[i];

        // Unpack u64 into 8 bytes
        for _ in 0..8 {
            if out_idx == 0 {
                // If we run out of space but still have data, fail.
                if val > 0 || i + 1 < count {
                    return Err(Error::BufferTooSmall);
                }
                break;
            }
            out_idx -= 1;
            *unsafe { dst.get_unchecked_mut(out_idx) } = val as u8;
            val >>= 8;
        }
    }

    // 3. Normalization Phase
    // Skip leading zeros *written by the loop* (not the explicit leading zeros from step 1)
    while out_idx < dst.len() && *unsafe { dst.get_unchecked(out_idx) } == 0 {
        out_idx += 1;
    }

    let length = dst.len() - out_idx;

    // Move the valid payload to the start of the buffer (memmove)
    if out_idx > 0 {
        let ptr = dst.as_mut_ptr();
        unsafe { core::ptr::copy(ptr.add(out_idx), ptr, length) };
    } else if length > dst.len() {
        return Err(Error::BufferTooSmall);
    }

    Ok(length)
}

// ----------------------------------------------------------------------
// Entry Point
// ----------------------------------------------------------------------

#[inline(always)]
pub unsafe fn decode_slice_unsafe(
    input: &[u8],
    dst: &mut [u8],
    config: &Config,
) -> Result<usize, Error> {
    // Hard limit of 512 bytes.
    assert!(input.len() <= 512, "Input too big! {}", input.len());

    // 1. Handle Leading Zeros
    let zero_char = *unsafe { config.alphabet.get_unchecked(0) };

    let mut leading_zeros = 0;
    while leading_zeros < input.len() && *unsafe { input.get_unchecked(leading_zeros) } == zero_char
    {
        leading_zeros += 1;
    }

    if leading_zeros > dst.len() {
        return Err(Error::BufferTooSmall);
    }

    // Write the zeros
    if leading_zeros > 0 {
        unsafe { core::ptr::write_bytes(dst.as_mut_ptr(), 0, leading_zeros) };
    }

    // 2. Decode the rest (The Payload)
    let src = &input[leading_zeros..];
    if src.is_empty() {
        return Ok(leading_zeros);
    }

    let written_payload = unsafe { decode_payload(config, src, &mut dst[leading_zeros..]) }?;

    Ok(leading_zeros + written_payload)
}
