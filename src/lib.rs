const DECODE_TABLE: [u8; 256] = {
    let mut table = [0xFFu8; 256];
    let mut i = 0;
    while i < 58 {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
};

const ALPHABET: [u8; 58] = *b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

// Base 58^10 (~4.3 * 10^17).
const RADIX_10: u64 = 430_804_206_899_405_824; 
const RADIX_128: u128 = RADIX_10 as u128;

// -----------------------------------------------------------------------------
// Lookup Table for "Idea 2" (Base 58^2)
// -----------------------------------------------------------------------------
// Maps integers 0..3363 to two Base58 characters packed in a u16 (Big Endian).
// Size: 3364 * 2 bytes = ~6.5KB (Fits perfectly in L1 Cache).
const LUT_58_SQUARED: [u16; 3364] = {
    let mut table = [0u16; 3364];
    let mut i = 0;
    while i < 3364 {
        let c1 = ALPHABET[i / 58];
        let c2 = ALPHABET[i % 58];
        // Pack into u16: High byte = first char, Low byte = second char
        table[i] = ((c1 as u16) << 8) | (c2 as u16);
        i += 1;
    }
    table
};

// -----------------------------------------------------------------------------
// Precomputed Powers for "Idea 1"
// -----------------------------------------------------------------------------
const POW64: [u64; 2] = {
    let r = RADIX_128;
    let val = 1u128 << 64;
    [ (val % r) as u64, (val / r) as u64 ]
};

const POW128: [u64; 3] = {
    let r = RADIX_128;
    let v64 = 1u128 << 64;
    let lo = (v64 % r) * (v64 % r);
    let mid = (v64 % r) * (v64 / r) * 2 + (lo / r);
    let hi = (v64 / r) * (v64 / r) + (mid / r);
    [ (lo % r) as u64, (mid % r) as u64, hi as u64 ]
};

const POW192: [u64; 4] = {
    let r = RADIX_128;
    let p1 = POW64;
    let p2 = POW128;
    let c0 = (p2[0] as u128) * (p1[0] as u128);
    let c1 = (p2[1] as u128) * (p1[0] as u128) + (p2[0] as u128) * (p1[1] as u128) + (c0 / r);
    let c2 = (p2[2] as u128) * (p1[0] as u128) + (p2[1] as u128) * (p1[1] as u128) + (c1 / r);
    let c3 = (p2[2] as u128) * (p1[1] as u128) + (c2 / r);
    [ (c0 % r) as u64, (c1 % r) as u64, (c2 % r) as u64, c3 as u64 ]
};

/// Encodes a byte slice into Base58 using high-radix arithmetic.
/// Uses a specialized helper for 32-byte inputs.
#[inline(always)]
pub unsafe fn encode_slice_unsafe(input: &[u8], mut dst: *mut u8) -> usize {
    let mut len = input.len();
    let mut src = input.as_ptr();
    let dst_start = dst;

    unsafe {
        // 1. Skip Leading Zeros (Vectorized)
        while len >= 8 {
            if (src as *const u64).read_unaligned() == 0 {
                (dst as *mut u64).write_unaligned(0x3131313131313131);
                dst = dst.add(8);
                src = src.add(8);
                len -= 8;
            } else {
                break;
            }
        }
        while len > 0 && *src == 0 {
            *dst = b'1';
            dst = dst.add(1);
            src = src.add(1);
            len -= 1;
        }

        if len == 0 {
            return dst.offset_from(dst_start) as usize;
        }

        if len == 32 {
            // Call the specialized helper for 32-byte inputs
            return encode_32_fixed(src, dst, dst_start);
        }

        // ---------------------------------------------------------------------
        // Generic Path (Variable Length)
        // ---------------------------------------------------------------------
        let mut radix_digits = [0u64; 128]; 
        let mut count = 1;

        while len >= 8 {
            let chunk = (src as *const u64).read_unaligned().to_be();
            let mut carry = chunk as u128;
            let mut i = 0;
            while i < count {
                let val = (radix_digits[i] as u128) * (1u128 << 64) + carry;
                radix_digits[i] = (val % RADIX_128) as u64;
                carry = val / RADIX_128;
                i += 1;
            }
            while carry > 0 {
                radix_digits[count] = (carry % RADIX_128) as u64;
                carry /= RADIX_128;
                count += 1;
            }
            src = src.add(8);
            len -= 8;
        }

        if len > 0 {
            let mut chunk = 0u64;
            let mut shift = 0;
            for _ in 0..len {
                chunk = (chunk << 8) | (*src as u64);
                src = src.add(1);
                shift += 8;
            }
            let mut carry = chunk as u128;
            let mut i = 0;
            while i < count {
                let val = (radix_digits[i] as u128) * (1u128 << shift) + carry;
                radix_digits[i] = (val % RADIX_128) as u64;
                carry = val / RADIX_128;
                i += 1;
            }
            while carry > 0 {
                radix_digits[count] = (carry % RADIX_128) as u64;
                carry /= RADIX_128;
                count += 1;
            }
        }

        // Generic Emission
        let mut last_val = radix_digits[count - 1];
        let mut last_chunk_len = 0;
        loop {
            last_chunk_len += 1;
            last_val /= 58;
            if last_val == 0 { break; }
        }
        
        let total_b58_len = (count - 1) * 10 + last_chunk_len;
        let mut out_ptr = dst.add(total_b58_len - 1);
        
        for i in 0..count {
            let mut val = radix_digits[i];
            let loops = if i == count - 1 { last_chunk_len } else { 10 };
            let mut k = 0;
            while k < loops {
                let rem = (val % 58) as usize;
                val /= 58;
                *out_ptr = *ALPHABET.get_unchecked(rem);
                out_ptr = out_ptr.sub(1);
                k += 1;
            }
        }

        dst.add(total_b58_len).offset_from(dst_start) as usize
    }
}

/// Specialized helper for exactly 32-byte inputs.
/// Combines "Parallel Powers" (Idea 1) and "Lookup Table" (Idea 2).
#[inline(always)]
unsafe fn encode_32_fixed(src: *const u8, dst: *mut u8, dst_start: *mut u8) -> usize {
    let mut radix_digits = [0u64; 5]; // Known max size for 32 bytes
    let mut count = 4; // Starts at 4, might grow to 5

    // 1. Parallel Accumulation (Idea 1)
    unsafe {
        let c0 = (src as *const u64).read_unaligned().to_be() as u128;
        let c1 = (src.add(8) as *const u64).read_unaligned().to_be() as u128;
        let c2 = (src.add(16) as *const u64).read_unaligned().to_be() as u128;
        let c3 = (src.add(24) as *const u64).read_unaligned().to_be() as u128;

        let s0 = c3 + c2 * (POW64[0] as u128) + c1 * (POW128[0] as u128) + c0 * (POW192[0] as u128);
        radix_digits[0] = (s0 % RADIX_128) as u64;
        let mut carry = s0 / RADIX_128;

        let s1 = carry + c2 * (POW64[1] as u128) + c1 * (POW128[1] as u128) + c0 * (POW192[1] as u128);
        radix_digits[1] = (s1 % RADIX_128) as u64;
        carry = s1 / RADIX_128;

        let s2 = carry + c1 * (POW128[2] as u128) + c0 * (POW192[2] as u128);
        radix_digits[2] = (s2 % RADIX_128) as u64;
        carry = s2 / RADIX_128;

        let s3 = carry + c0 * (POW192[3] as u128);
        radix_digits[3] = (s3 % RADIX_128) as u64;
        carry = s3 / RADIX_128;

        if carry > 0 {
            radix_digits[4] = carry as u64;
            count = 5;
        }

        // 2. Calculate Length
        let mut last_val = radix_digits[count - 1];
        let mut last_chunk_len = 0;
        loop {
            last_chunk_len += 1;
            last_val /= 58;
            if last_val == 0 { break; }
        }
        
        let total_b58_len = (count - 1) * 10 + last_chunk_len;
        
        // 3. Fast Emission with Lookup Table (Idea 2)
        // We process 2 chars at a time using LUT_58_SQUARED.
        // This halves the number of divisions.
        let mut out_ptr = dst.add(total_b58_len); // One past end

        for i in 0..count {
            let mut val = radix_digits[i];
            let loops = if i == count - 1 { last_chunk_len } else { 10 };
            
            // Unrolled loop for processing 2 chars at a time
            let mut k = 0;
            while k < loops {
                if k == loops - 1 && loops % 2 != 0 {
                    // Odd remaining character
                    out_ptr = out_ptr.sub(1);
                    *out_ptr = *ALPHABET.get_unchecked((val % 58) as usize);
                    val /= 58;
                    k += 1;
                } else {
                    // Process 2 chars
                    out_ptr = out_ptr.sub(2);
                    let idx = (val % 3364) as usize; // 58^2
                    val /= 3364;
                    
                    // Read 2 chars from LUT (u16)
                    let packed = *LUT_58_SQUARED.get_unchecked(idx);
                    
                    // Write 2 chars (LUT is packed Big Endian: High=First, Low=Second)
                    // But we are writing backwards, so we write Second then First.
                    // Wait, writing backwards means higher memory address gets earlier char? 
                    // No, we fill from right to left.
                    // String: [ ... C1, C2 ... ]
                    // Pointer at C2+1. Sub 2. Pointer at C1.
                    // Write C1 to *ptr, C2 to *ptr+1.
                    
                    // packed: [C1 C2] (u16)
                    *(out_ptr as *mut u16) = packed.to_be(); // Write as [C1, C2] in memory
                    
                    k += 2;
                }
            }
        }

        dst.add(total_b58_len).offset_from(dst_start) as usize
    }
}

// /// Decodes a Base58 byte slice using a highly optimized scalar algorithm.
// ///
// /// # Safety
// /// This function is **unsafe** and requires the caller to uphold strict memory contracts.
// ///
// /// * **Output Capacity**: `dst` must be large enough. Approx `len * 733 / 1000 + 1`.
// /// * **Pointer Validity**: `dst` must point to valid memory.
// #[inline(always)]
// pub unsafe fn decode_slice_unsafe(_config: &Config, input: &[u8], mut dst: *mut u8) -> Result<usize, Error> {
//     let mut len = input.len();
//     if len == 0 { return Ok(0); }

//     let mut src = input.as_ptr();
//     let dst_start = dst;

//     unsafe {
//         // 1. Handle Leading '1's (Base58 Zero)
//         // SWAR optimization: Check 8 bytes at a time for '1' (0x31).
//         while len >= 8 {
//             let chunk = (src as *const u64).read_unaligned();
//             // Check if all bytes are 0x31 ('1')
//             if chunk == 0x3131313131313131 {
//                 (dst as *mut u64).write_unaligned(0); // Write 8 zeros
//                 dst = dst.add(8);
//                 src = src.add(8);
//                 len -= 8;
//             } else {
//                 break;
//             }
//         }

//         // Handle remaining leading '1's
//         while len > 0 && *src == b'1' {
//             *dst = 0;
//             dst = dst.add(1);
//             src = src.add(1);
//             len -= 1;
//         }

//         // 2. Base Conversion (Base 58 -> Base 256)
//         // The scratch buffer for the bignum is `dst` itself (after the leading zeros).
//         // `bignum_start` points to the Big Endian byte array we are building.
//         let bignum_start = dst;
//         let mut bignum_len = 0; // Current length of our base-256 number

//         let src_end = src.add(len);
//         let table_ptr = DECODE_TABLE.as_ptr();

//         while src < src_end {
//             // Fast Table Lookup
//             // If char > 127, it's invalid (table size is 128).
//             let c = *src;
//             if c >= 128 { return Err(Error::InvalidCharacter); }
            
//             let digit = *table_ptr.add(c as usize);
//             if digit == 0xFF { return Err(Error::InvalidCharacter); }

//             // Bignum Math: bignum = bignum * 58 + digit
//             let mut carry = digit as u32;
            
//             // We iterate the bignum from end (LSB) to start (MSB) because we need to propagate carry.
//             // Note: bignum is stored in Big Endian in `dst` to match final output requirements.
//             // However, mathematically for `carry` propagation, it's easier to think LSB->MSB.
//             // Pointer arithmetic: `p` starts at the last byte written.
            
//             let mut i = 0;
//             // Iterate backwards from the end of the current bignum
//             let mut p = bignum_start.add(bignum_len).sub(1); 
            
//             while i < bignum_len {
//                 // val = byte * 58 + carry
//                 // Max val: 255 * 58 + 57 = 14847 (u16 is enough, u32 is fast)
//                 let val = (*p as u32) * 58 + carry;
                
//                 *p = val as u8; // implicit % 256
//                 carry = val >> 8; // implicit / 256
                
//                 if i < bignum_len {
//                      p = p.sub(1);
//                 }
//                 i += 1;
//             }

//             // If there is still a carry after processing all existing bytes, we grow the number.
//             // Since we are storing Big Endian, a new Most Significant Byte means shifting everything?
//             // NO. That is O(N^2) memmove.
//             // OPTIMIZATION:
//             // Standard scalar Base58 decoders usually accumulate into a temporary buffer or 
//             // accept that they fill `dst` from right-to-left. 
//             // BUT, to keep this API consistent and `unsafe` fast:
//             // The efficient way is to store the bignum *Little Endian* during calculation 
//             // and reverse it at the very end.

//             // RE-STRATEGY for Inner Loop:
//             // Treat `bignum_start` as Little Endian during the loop.
//             let mut ptr = bignum_start;
//             let mut k = 0;
//             let mut loop_carry = digit as u32;

//             while k < bignum_len {
//                 let val = (*ptr as u32) * 58 + loop_carry;
//                 *ptr = val as u8;
//                 loop_carry = val >> 8;
//                 ptr = ptr.add(1);
//                 k += 1;
//             }

//             while loop_carry > 0 {
//                 *ptr = loop_carry as u8;
//                 loop_carry >>= 8;
//                 ptr = ptr.add(1);
//                 bignum_len += 1;
//             }

//             src = src.add(1);
//         }

//         // 3. Final Reverse
//         // Our bignum is currently Little Endian at `bignum_start` with length `bignum_len`.
//         // We need it Big Endian.
//         if bignum_len > 0 {
//             let mut p_left = bignum_start;
//             let mut p_right = bignum_start.add(bignum_len - 1);
//             while p_left < p_right {
//                 let tmp = *p_left;
//                 *p_left = *p_right;
//                 *p_right = tmp;
//                 p_left = p_left.add(1);
//                 p_right = p_right.sub(1);
//             }
//         }

//         Ok(dst.add(bignum_len).offset_from(dst_start) as usize)
//     }
// }

#[cfg(test)]
mod exhaustive_tests {
    // --- Imports ---
    use super::*;
    use rand::{Rng, rng};
    use bs58::{encode, decode, Alphabet};

    fn random_bytes(len: usize) -> Vec<u8> {
        let mut rng = rng();
        (0..len).map(|_| rng.random()).collect()
    }

    #[test]
    fn test_data() {
        for i in 0..256 {
            let data = random_bytes(i);
            let encoded = encode(&data)
                .with_alphabet(Alphabet::BITCOIN)
                .into_string();

            let mut buff = vec![0u8; 1024];

            unsafe { encode_slice_unsafe(&data, buff.as_mut_ptr()) };

            assert_eq!(&buff[..encoded.len()], encoded.as_bytes(), "Failed at size {}", i);
        }
    }
}
