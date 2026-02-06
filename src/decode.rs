use crate::{Error, Config};

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

    // Standard schoolbook multiplication-with-carry
    for i in 0..len {
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

    for &byte in src {
        let digit = *unsafe { config.decode_map.get_unchecked(byte as usize) };
        if digit == 255 { return Err(Error::InvalidCharacter); }

        value = value * 58 + (digit as u64);
        multiplier *= 58;
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
    let mut bignum = [0u64; 128];
    let mut count = 1; 

    // Process full chunks of 10 characters (Base 58^10)
    // This reduces the bignum loop overhead by 10x.
    while src.len() >= 10 {
        // Unroll parsing for the fixed chunk size
        let mut chunk = 0u64;
        for i in 0..10 {
            let digit = *unsafe { config.decode_map.get_unchecked(*src.get_unchecked(i) as usize) };
            if digit == 255 { return Err(Error::InvalidCharacter); }
            chunk = chunk * 58 + (digit as u64);
        }
        
        unsafe { bignum_mul_add(&mut bignum, &mut count, RADIX_58_10, chunk) };
        src = &src[10..];
    }

    // Process remaining tail (1-9 characters)
    if !src.is_empty() {
        let (chunk, multiplier) = unsafe { parse_chunk(config, src) }?;
        unsafe { bignum_mul_add(&mut bignum, &mut count, multiplier, chunk) };
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
                if val > 0 || i + 1 < count { return Err(Error::BufferTooSmall); }
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
pub unsafe fn decode_slice_unsafe(input: &[u8], dst: &mut [u8], config: &Config) -> Result<usize, Error> {
    // Hard limit of 512 bytes.
    assert!(input.len() <= 512, "Input too big! {}", input.len());

    // 1. Handle Leading Zeros
    let zero_char = *unsafe { config.alphabet.get_unchecked(0) };

    let mut leading_zeros = 0;
    while leading_zeros < input.len() && *unsafe { input.get_unchecked(leading_zeros) } == zero_char {
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
