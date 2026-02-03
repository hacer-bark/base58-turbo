const ALPHABET: [u8; 58] = *b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

// Base 58^10 (~4.3 * 10^17)
const RADIX_10: u64 = 430_804_206_899_405_824;
const RADIX_128: u128 = RADIX_10 as u128;

// 58^4 = 11,316,496
const RADIX_58_4: u64 = 11_316_496;

// Base 58^5
const RADIX_58_5: u64 = 656_356_768;

// -----------------------------------------------------------------------------
// Lookup Tables & Constants
// -----------------------------------------------------------------------------

const LUT_58_SQUARED: [u16; 3364] = {
    let mut table = [0u16; 3364];
    let mut i = 0;
    while i < 3364 {
        let c1 = ALPHABET[i / 58];
        let c2 = ALPHABET[i % 58];
        table[i] = ((c1 as u16) << 8) | (c2 as u16);
        i += 1;
    }
    table
};

/// Computes the Base58^5 weights for a sequence of u32 chunks.
///
/// Logic:
/// For an input of `N` chunks, the chunk at index `i` (0=MSB) represents
/// the value `input[i] * (2^32)^(N - 1 - i)`.
/// This function calculates the value of `(2^32)^k` expressed in Base 58^5 digits.
const fn generate_weights<const INPUTS: usize, const OUTPUTS: usize>() -> [[u32; OUTPUTS]; INPUTS] {
    let mut table = [[0u32; OUTPUTS]; INPUTS];
    
    // A temporary BigInt buffer to hold powers of 2^32.
    // 24 chunks of u32 is enough for 96 bytes (covers 69, 64, 32, 25).
    // Stored Little Endian: val[0] is LSB.
    let mut val = [0u32; 24]; 
    val[0] = 1;

    // Iterate from LSB chunk (last row) to MSB chunk (first row)
    let mut i = 0;
    while i < INPUTS {
        // We fill the table from bottom up because we calculate powers 
        // 1, 2^32, (2^32)^2... iteratively.
        let row_idx = INPUTS - 1 - i;

        // Convert current `val` (which is 2^(32*i)) to Base 58^5 digits
        let mut temp_val = val; // Copy for destructive division
        
        // Fill columns from LSB (last) to MSB (first)
        let mut col_idx = OUTPUTS;
        while col_idx > 0 {
            col_idx -= 1;
            
            // BigInt Div/Mod by RADIX_58_5
            let mut rem = 0u64;
            let mut j = 24;
            while j > 0 {
                j -= 1;
                let current = (temp_val[j] as u64) + (rem << 32);
                temp_val[j] = (current / RADIX_58_5) as u32;
                rem = current % RADIX_58_5;
            }
            
            table[row_idx][col_idx] = rem as u32;
        }

        // Multiply `val` by 2^32 for the next iteration.
        // Since `val` is [u32] Little Endian, this is just a left-shift of the array indices.
        let mut j = 23;
        while j > 0 {
            val[j] = val[j - 1];
            j -= 1;
        }
        val[0] = 0;

        i += 1;
    }
    table
}

// -----------------------------------------------------------------------------
// Auto-Generated Constants
// -----------------------------------------------------------------------------

// 25 bytes: treated as 1x u8 Head (u32 cast) + 6x u32 Body = 7 Inputs.
// 25 bytes fits in 7 output digits of Base 58^5.
const TABLE_25: [[u32; 7]; 7] = generate_weights::<7, 7>();

// 32 bytes: 8x u32 Inputs. Fits in 8 output digits (plus carry).
const TABLE_32: [[u32; 8]; 8] = generate_weights::<8, 8>();

// 64 bytes: 16x u32 Inputs. Fits in 17 output digits (plus carry).
const TABLE_64: [[u32; 17]; 16] = generate_weights::<16, 17>();

// 69 bytes: 1x u8 Head + 16x u32 Body + 1x u32 Tail = 18 Inputs. 
const TABLE_69: [[u32; 19]; 18] = generate_weights::<18, 19>();

// -----------------------------------------------------------------------------
// Emission Helpers (The formatting logic)
// -----------------------------------------------------------------------------

/// Emits exactly 10 Base58 characters from a full Radix128 digit.
/// Uses unrolled table lookups for speed.
#[inline(always)]
unsafe fn emit_full_block(mut val: u64, out_ptr: &mut *mut u8) {
    // 2-2-2-2-2 grouping requires fewer divisions
    // Lower 4 chars
    let rem1 = (val % RADIX_58_4) as usize;
    val /= RADIX_58_4;
    
    // Middle 4 chars
    let rem2 = (val % RADIX_58_4) as usize;
    val /= RADIX_58_4;

    // Top 2 chars
    let rem3 = val as usize;

    unsafe {
        // Write backwards (pointer moves down)
        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16).write_unaligned(LUT_58_SQUARED.get_unchecked(rem1 % 3364).to_be());
        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16).write_unaligned(LUT_58_SQUARED.get_unchecked(rem1 / 3364).to_be());

        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16).write_unaligned(LUT_58_SQUARED.get_unchecked(rem2 % 3364).to_be());
        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16).write_unaligned(LUT_58_SQUARED.get_unchecked(rem2 / 3364).to_be());

        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16).write_unaligned(LUT_58_SQUARED.get_unchecked(rem3).to_be());
    }
}

/// Emits a variable number of characters (1 to 10) for the final most significant digit.
#[inline(always)]
unsafe fn emit_partial_block(mut val: u64, len: usize, mut out_ptr: *mut u8) {
    let mut loops = len;
    
    // Process in chunks of 4 if possible (Base 58^4)
    while loops >= 4 {
        let rem = val % RADIX_58_4;
        val /= RADIX_58_4;
        
        let high = (rem / 3364) as usize;
        let low = (rem % 3364) as usize;

        unsafe {
            out_ptr = out_ptr.sub(2);
            (out_ptr as *mut u16).write_unaligned(LUT_58_SQUARED.get_unchecked(low).to_be());
            out_ptr = out_ptr.sub(2);
            (out_ptr as *mut u16).write_unaligned(LUT_58_SQUARED.get_unchecked(high).to_be());
        }
        loops -= 4;
    }
    
    // Process remaining 1-3 chars one by one
    while loops > 0 {
        unsafe {
            out_ptr = out_ptr.sub(1);
            *out_ptr = *ALPHABET.get_unchecked((val % 58) as usize);
        }
        val /= 58;
        loops -= 1;
    }
}

/// Core function to convert computed Bignum digits into the final string.
/// Returns the final end-pointer.
#[inline(always)]
unsafe fn write_to_base58(digits: &[u64], count: usize, dst: *mut u8) -> *mut u8 {
    // 1. Calculate the length of the highest (last) digit
    let mut last_val = *unsafe { digits.get_unchecked(count - 1) };
    let mut last_chunk_len = 0;
    loop {
        last_chunk_len += 1;
        last_val /= 58;
        if last_val == 0 { break; }
    }

    // 2. Calculate exact total length
    let total_b58_len = (count - 1) * 10 + last_chunk_len;
    let mut out_ptr = unsafe { dst.add(total_b58_len) };
    let final_ptr = out_ptr; // Save for return calculation

    // 3. Emit all full 10-char blocks (Indices 0 to N-2)
    for i in 0..(count - 1) {
        unsafe { emit_full_block(*digits.get_unchecked(i), &mut out_ptr) };
    }

    // 4. Emit the last partial block
    unsafe { emit_partial_block(*digits.get_unchecked(count - 1), last_chunk_len, out_ptr) };

    final_ptr
}

// -----------------------------------------------------------------------------
// Math Kernels (The Arithmetic)
// -----------------------------------------------------------------------------

/// Parallel arithmetic kernel for exactly 25 bytes.
/// Uses Matrix Multiplication (Base 58^5) for high performance.
/// Output: 4 Base58^10 digits.
#[inline(always)]
unsafe fn process_fixed_25(src: *const u8, out: &mut [u64]) -> usize {
    // 1. Read Inputs
    // Input structure: [Head: u8] [Body: 6x u32]
    let mut input = [0u32; 7];

    unsafe {
        // Chunk 0: Head byte
        input[0] = *src as u32; 
        
        // Chunks 1..6: Body u32s (Big Endian)
        let src_u32 = src.add(1) as *const u32;
        // Unrolled read
        input[1] = src_u32.read_unaligned().to_be();
        input[2] = src_u32.add(1).read_unaligned().to_be();
        input[3] = src_u32.add(2).read_unaligned().to_be();
        input[4] = src_u32.add(3).read_unaligned().to_be();
        input[5] = src_u32.add(4).read_unaligned().to_be();
        input[6] = src_u32.add(5).read_unaligned().to_be();
    }

    // 2. Matrix Multiplication (Base 58^5)
    // We compute 7 output digits. Index 0 is reserved for MSB carry, 1..7 for table results.
    let mut digits_5 = [0u64; 8];

    // Row-Major accumulation is efficient here due to small size
    for i in 0..7 {
        let val = input[i] as u64;
        for k in 0..7 {
            digits_5[k + 1] += val * (TABLE_25[i][k] as u64);
        }
    }

    // 3. Reduction (Propagate carries from LSB to MSB)
    let mut carry = 0u64;
    let mut reduced = [0u64; 8];

    for k in (1..8).rev() {
        let val = digits_5[k] + carry;
        reduced[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    // MSB carry
    reduced[0] = digits_5[0] + carry;

    // 4. Pack into Base 58^10
    // We combine pairs of Base58^5 digits.
    // Index 0 (reduced[0]) is MSB. Index 7 is LSB.
    // Output out[0] corresponds to LSB.
    
    out[0] = reduced[6] * RADIX_58_5 + reduced[7];
    out[1] = reduced[4] * RADIX_58_5 + reduced[5];
    out[2] = reduced[2] * RADIX_58_5 + reduced[3];
    out[3] = reduced[0] * RADIX_58_5 + reduced[1]; 
    
    // 25 bytes always results in 4 Base58^10 digits (approx 35 chars)
    4
}

/// Optimized arithmetic kernel for exactly 32 bytes.
/// Uses Matrix Multiplication to calculate Base 58^5 digits, then packs them into Base 58^10.
#[inline(always)]
unsafe fn process_fixed_32(src: *const u8, out_digits: &mut [u64]) -> usize {
    // 1. Read input as 8 u32 chunks (Big Endian)
    let mut input = [0u32; 8];
    let src_u32 = src as *const u32;
    unsafe {
        input[0] = src_u32.read_unaligned().to_be();
        input[1] = src_u32.add(1).read_unaligned().to_be();
        input[2] = src_u32.add(2).read_unaligned().to_be();
        input[3] = src_u32.add(3).read_unaligned().to_be();
        input[4] = src_u32.add(4).read_unaligned().to_be();
        input[5] = src_u32.add(5).read_unaligned().to_be();
        input[6] = src_u32.add(6).read_unaligned().to_be();
        input[7] = src_u32.add(7).read_unaligned().to_be();
    }

    // 2. Matrix Multiplication (Base 58^5)
    // Digits: [0..9]. Index 0 is MSB (reserved for carry), Indices 1..8 are outputs from matrix.
    let mut digits_5 = [0u128; 9];
    
    for i in 0..8 {
        let val = input[i] as u128;
        // Table columns map to digits_5[1..9]
        for k in 0..8 {
            digits_5[k + 1] += val * (TABLE_32[i][k] as u128);
        }
    }

    // 3. Reduction
    let mut carry = 0u64;
    let mut final_5 = [0u64; 9];
    
    // Reduce indices 8 down to 1
    for k in (1..9).rev() {
        let val = digits_5[k] + (carry as u128);
        final_5[k] = (val % (RADIX_58_5 as u128)) as u64;
        carry = (val / (RADIX_58_5 as u128)) as u64;
    }
    // Index 0 absorbs the final carry
    final_5[0] = (digits_5[0] as u64) + carry;

    // 4. Pack into Base 58^10 (Little Endian for out_digits)
    // Pair (final_5[7], final_5[8]) -> out[0]
    out_digits[0] = final_5[7] * RADIX_58_5 + final_5[8];
    out_digits[1] = final_5[5] * RADIX_58_5 + final_5[6];
    out_digits[2] = final_5[3] * RADIX_58_5 + final_5[4];
    out_digits[3] = final_5[1] * RADIX_58_5 + final_5[2];
    out_digits[4] = final_5[0];

    // 5. Return count
    if out_digits[4] > 0 { 5 } else { 4 }
}

/// Highly Optimized kernel for exactly 64 bytes.
/// Strategies:
/// 1. Row-Major Access: Sequential table access for cache locality.
/// 2. u64 Accumulation: Significantly faster than u128.
/// 3. Split-Batching: Process 8 inputs, reduce, process 8 inputs. Prevents u64 overflow.
#[inline(always)]
unsafe fn process_fixed_64(src: *const u8, out_digits: &mut [u64]) -> usize {
    // 1. Read input as 16 u32 chunks (Big Endian)
    let mut input = [0u32; 16];
    let src_u32 = src as *const u32;
    unsafe {
        input[0] = src_u32.read_unaligned().to_be();
        input[1] = src_u32.add(1).read_unaligned().to_be();
        input[2] = src_u32.add(2).read_unaligned().to_be();
        input[3] = src_u32.add(3).read_unaligned().to_be();
        input[4] = src_u32.add(4).read_unaligned().to_be();
        input[5] = src_u32.add(5).read_unaligned().to_be();
        input[6] = src_u32.add(6).read_unaligned().to_be();
        input[7] = src_u32.add(7).read_unaligned().to_be();
        input[8] = src_u32.add(8).read_unaligned().to_be();
        input[9] = src_u32.add(9).read_unaligned().to_be();
        input[10] = src_u32.add(10).read_unaligned().to_be();
        input[11] = src_u32.add(11).read_unaligned().to_be();
        input[12] = src_u32.add(12).read_unaligned().to_be();
        input[13] = src_u32.add(13).read_unaligned().to_be();
        input[14] = src_u32.add(14).read_unaligned().to_be();
        input[15] = src_u32.add(15).read_unaligned().to_be();
    }

    // 2. Accumulate
    // We use u64 accumulators. To avoid overflow (max sum > u64::MAX),
    // we split processing into two batches of 8 inputs.
    let mut digits = [0u64; 18];

    // --- Batch 1: Inputs 0 to 7 ---
    for i in 0..8 {
        let val = input[i] as u64;
        // Inner loop: 17 columns. Compiler unrolls this perfectly with Row-Major access.
        for k in 0..17 {
            digits[k + 1] += val * (TABLE_64[i][k] as u64);
        }
    }

    // Intermediate Reduction (Carry Propagation)
    // This resets the accumulators to small values, making space for Batch 2.
    // Div/Rem by constant is optimized by compiler into Mul/Shift (very fast).
    let mut carry = 0u64;
    for k in (1..18).rev() {
        let val = digits[k] + carry;
        digits[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    digits[0] += carry;
    carry = 0; // Reset carry, digits[0] now holds the high part

    // --- Batch 2: Inputs 8 to 15 ---
    for i in 8..16 {
        let val = input[i] as u64;
        for k in 0..17 {
            digits[k + 1] += val * (TABLE_64[i][k] as u64);
        }
    }

    // Final Reduction
    for k in (1..18).rev() {
        let val = digits[k] + carry;
        digits[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    digits[0] += carry;

    // 3. Pack into Base 58^10
    // Combining pairs of Base58^5 digits
    out_digits[0] = digits[16] * RADIX_58_5 + digits[17];
    out_digits[1] = digits[14] * RADIX_58_5 + digits[15];
    out_digits[2] = digits[12] * RADIX_58_5 + digits[13];
    out_digits[3] = digits[10] * RADIX_58_5 + digits[11];
    out_digits[4] = digits[8]  * RADIX_58_5 + digits[9];
    out_digits[5] = digits[6]  * RADIX_58_5 + digits[7];
    out_digits[6] = digits[4]  * RADIX_58_5 + digits[5];
    out_digits[7] = digits[2]  * RADIX_58_5 + digits[3];
    out_digits[8] = digits[0]  * RADIX_58_5 + digits[1];

    if out_digits[8] > 0 { 9 } else { 8 }
}

/// Highly Optimized kernel for exactly 69 bytes.
/// Input: 18 chunks of u32 (1 Head + 16 Body + 1 Tail).
/// Output: 10 Base58^10 digits (packed from 20 Base58^5 digits).
#[inline(always)]
unsafe fn process_fixed_69(src: *const u8, out_digits: &mut [u64]) -> usize {
    // 1. Read input as 18 u32 chunks.
    let mut input = [0u32; 18];

    unsafe {
        // Chunk 0: Head (1 byte)
        input[0] = *src as u32;

        // Chunks 1..16: Read 64 bytes of body using 8 u64 loads (Fast)
        // src[1..65]
        let body_ptr = src.add(1) as *const u64;

        let v0 = body_ptr.read_unaligned().to_be();
        input[1] = (v0 >> 32) as u32; input[2] = v0 as u32;
        let v1 = body_ptr.add(1).read_unaligned().to_be();
        input[3] = (v1 >> 32) as u32; input[4] = v1 as u32;
        let v2 = body_ptr.add(2).read_unaligned().to_be();
        input[5] = (v2 >> 32) as u32; input[6] = v2 as u32;
        let v3 = body_ptr.add(3).read_unaligned().to_be();
        input[7] = (v3 >> 32) as u32; input[8] = v3 as u32;
        let v4 = body_ptr.add(4).read_unaligned().to_be();
        input[9] = (v4 >> 32) as u32; input[10] = v4 as u32;
        let v5 = body_ptr.add(5).read_unaligned().to_be();
        input[11] = (v5 >> 32) as u32; input[12] = v5 as u32;
        let v6 = body_ptr.add(6).read_unaligned().to_be();
        input[13] = (v6 >> 32) as u32; input[14] = v6 as u32;
        let v7 = body_ptr.add(7).read_unaligned().to_be();
        input[15] = (v7 >> 32) as u32; input[16] = v7 as u32;
        
        // Chunk 17: Remaining 4 bytes at src[65..69]
        input[17] = (src.add(65) as *const u32).read_unaligned().to_be();
    }

    // 2. Accumulate Columns
    // We need 20 digits total (indices 0..19).
    // Digits 1..19 come from the table (19 columns).
    // Digit 0 is the final carry.
    let mut digits = [0u64; 20];
    let mut carry = 0u64;

    // --- Batch 1: Inputs 0..5 ---
    for i in 0..6 {
        let val = input[i] as u64;
        // Map table cols 0..18 to digits 1..19
        for k in 0..19 {
            digits[k + 1] += val * (TABLE_69[i][k] as u64);
        }
    }
    // Reduction 1
    for k in (1..20).rev() {
        let val = digits[k] + carry;
        digits[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    digits[0] += carry;
    carry = 0;

    // --- Batch 2: Inputs 6..11 ---
    for i in 6..12 {
        let val = input[i] as u64;
        for k in 0..19 {
            digits[k + 1] += val * (TABLE_69[i][k] as u64);
        }
    }
    // Reduction 2
    for k in (1..20).rev() {
        let val = digits[k] + carry;
        digits[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    digits[0] += carry;
    carry = 0;

    // --- Batch 3: Inputs 12..17 ---
    for i in 12..18 {
        let val = input[i] as u64;
        for k in 0..19 {
            digits[k + 1] += val * (TABLE_69[i][k] as u64);
        }
    }
    // Final Reduction
    for k in (1..20).rev() {
        let val = digits[k] + carry;
        digits[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    digits[0] += carry;

    // 3. Pack into Base 58^10
    // We combine pairs of Base58^5 digits.
    // digits[0] is MSB, digits[19] is LSB.
    // Pairs: (18,19)->out[0] ... (0,1)->out[9]
    out_digits[0] = digits[18] * RADIX_58_5 + digits[19];
    out_digits[1] = digits[16] * RADIX_58_5 + digits[17];
    out_digits[2] = digits[14] * RADIX_58_5 + digits[15];
    out_digits[3] = digits[12] * RADIX_58_5 + digits[13];
    out_digits[4] = digits[10] * RADIX_58_5 + digits[11];
    out_digits[5] = digits[8]  * RADIX_58_5 + digits[9];
    out_digits[6] = digits[6]  * RADIX_58_5 + digits[7];
    out_digits[7] = digits[4]  * RADIX_58_5 + digits[5];
    out_digits[8] = digits[2]  * RADIX_58_5 + digits[3];
    out_digits[9] = digits[0]  * RADIX_58_5 + digits[1];

    // 4. Return count
    if out_digits[9] > 0 { 10 } else { 9 }
}

/// General arithmetic kernel for variable lengths.
/// Writes results into `out_digits` and returns the number of digits used.
#[inline(always)]
unsafe fn process_general(mut src: *const u8, mut len: usize, out_digits: &mut [u64]) -> usize {
    let mut count = 1;

    unsafe {
        // Process in 8-byte chunks (u64)
        while len >= 8 {
            let chunk = (src as *const u64).read_unaligned().to_be();
            let mut carry = chunk as u128;
            let mut i = 0;
            
            while i < count {
                let val = (*out_digits.get_unchecked(i) as u128) * (1u128 << 64) + carry;
                *out_digits.get_unchecked_mut(i) = (val % RADIX_128) as u64;
                carry = val / RADIX_128;
                i += 1;
            }
            while carry > 0 {
                *out_digits.get_unchecked_mut(count) = (carry % RADIX_128) as u64;
                carry /= RADIX_128;
                count += 1;
            }
            src = src.add(8);
            len -= 8;
        }

        // Process remaining 1-7 bytes
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
                let val = (*out_digits.get_unchecked(i) as u128) * (1u128 << shift) + carry;
                *out_digits.get_unchecked_mut(i) = (val % RADIX_128) as u64;
                carry = val / RADIX_128;
                i += 1;
            }
            while carry > 0 {
                *out_digits.get_unchecked_mut(count) = (carry % RADIX_128) as u64;
                carry /= RADIX_128;
                count += 1;
            }
        }
    }
    count
}

// -----------------------------------------------------------------------------
// Main Entry Point
// -----------------------------------------------------------------------------

#[inline(always)]
pub unsafe fn encode_slice_unsafe(input: &[u8], mut dst: *mut u8) -> usize {
    let mut len = input.len();
    let mut src = input.as_ptr();
    let dst_start = dst;

    unsafe {
        // 1. Skip Leading Zeros (Vectorized)
        while len >= 8 {
            if (src as *const u64).read_unaligned() == 0 {
                (dst as *mut u64).write_unaligned(0x3131313131313131); // '1' * 8
                dst = dst.add(8);
                src = src.add(8);
                len -= 8;
            } else {
                break;
            }
        }
        // Handle remaining scalar zeros
        while len > 0 && *src == 0 {
            *dst = b'1';
            dst = dst.add(1);
            src = src.add(1);
            len -= 1;
        }

        if len == 0 {
            return dst.offset_from(dst_start) as usize;
        }

        // 2. Calculate Base58 Digits (Radix 58^10)
        // We use a unified buffer size. 128 digits covers very large inputs (approx 1000 bytes).
        let mut radix_digits = [0u64; 128];
        let count;

        if len == 25 {
            // Dedicated Path: 25 bytes
            count = process_fixed_25(src, &mut radix_digits);
        } else if len == 32 {
            // Dedicated Path: 32 bytes
            count = process_fixed_32(src, &mut radix_digits);
        } else if len == 64 {
            // Dedicated Path: 64 bytes
            count = process_fixed_64(src, &mut radix_digits);
        } else if len == 69 {
            // Dedicated Path: 69 bytes
            count = process_fixed_69(src, &mut radix_digits);
        } else {
            // General Path: Variable length
            count = process_general(src, len, &mut radix_digits);
        }

        // 3. Emit String
        // All paths meet here for the final write
        let final_ptr = write_to_base58(&radix_digits, count, dst);

        final_ptr.offset_from(dst_start) as usize
    }
}
