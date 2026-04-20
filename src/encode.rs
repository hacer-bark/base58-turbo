use crate::Config;

// ----------------------------------------------------------------------
// Constants & Lookup Tables
// ----------------------------------------------------------------------

/// Base 58^4 (11,316,496)
const RADIX_58_4: u64 = 11_316_496;

/// Base 58^5 (656,356,768)
const RADIX_58_5: u64 = 656_356_768;

// ----------------------------------------------------------------------
// Table Generation
// ----------------------------------------------------------------------

/// Computes weights for converting Base 2^32 chunks into Base 58^5 digits.
/// Result: table[i][k] = coefficient for input_chunk[i] contributing to output_digit[k].
const fn generate_weights<const INPUTS: usize, const OUTPUTS: usize>() -> [[u32; OUTPUTS]; INPUTS] {
    let mut table = [[0u32; OUTPUTS]; INPUTS];
    let mut val = [0u32; 24]; // Temp bignum buffer
    val[0] = 1;

    let mut i = 0;
    while i < INPUTS {
        let row_idx = INPUTS - 1 - i;

        // Copy current power (val) to table row
        let mut k = 0;
        while k < OUTPUTS {
            table[row_idx][k] = val[k];
            k += 1;
        }

        // The inner conversion loop
        let mut temp_val = val;
        let mut col_idx = OUTPUTS;
        while col_idx > 0 {
            col_idx -= 1;
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

        // Prepare for next iteration: val = val * 2^32 (Logical Left Shift 32 bits)
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

// ----------------------------------------------------------------------
// Precomputed Weight Tables
// ----------------------------------------------------------------------

// 25 bytes -> 7 input chunks (1x u8, 6x u32) -> 7 output digits
const TABLE_25: [[u32; 7]; 7] = generate_weights::<7, 7>();

// 32 bytes -> 8 input chunks (8x u32) -> 8 output digits
const TABLE_32: [[u32; 8]; 8] = generate_weights::<8, 8>();

// 64 bytes -> 16 input chunks -> 18 output digits
const TABLE_64: [[u32; 18]; 16] = generate_weights::<16, 18>();

// 69 bytes -> 18 input chunks -> 20 output digits
const TABLE_69: [[u32; 20]; 18] = generate_weights::<18, 20>();

// ----------------------------------------------------------------------
// Memory Helpers
// ----------------------------------------------------------------------

#[inline(always)]
unsafe fn load_be_u32(ptr: *const u8) -> u32 {
    unsafe { (ptr as *const u32).read_unaligned().to_be() }
}

#[inline(always)]
unsafe fn load_be_u64(ptr: *const u8) -> u64 {
    unsafe { (ptr as *const u64).read_unaligned().to_be() }
}

// ----------------------------------------------------------------------
// Emission Helpers
// ----------------------------------------------------------------------

/// Emits exactly 10 characters for a full `u64` digit (Base 58^10).
#[inline(always)]
unsafe fn emit_full_block(config: &Config, mut val: u64, out_ptr: &mut *mut u8) {
    // Extract Low 4 chars
    let rem1 = (val % RADIX_58_4) as usize;
    val /= RADIX_58_4;

    // Extract Middle 4 chars
    let rem2 = (val % RADIX_58_4) as usize;
    val /= RADIX_58_4;

    // Extract High 2 chars
    let rem3 = val as usize;

    unsafe {
        // Write backwards using 2-byte lookup table
        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16)
            .write_unaligned(config.lut_58_squared.get_unchecked(rem1 % 3364).to_be());
        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16)
            .write_unaligned(config.lut_58_squared.get_unchecked(rem1 / 3364).to_be());

        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16)
            .write_unaligned(config.lut_58_squared.get_unchecked(rem2 % 3364).to_be());
        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16)
            .write_unaligned(config.lut_58_squared.get_unchecked(rem2 / 3364).to_be());

        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16).write_unaligned(config.lut_58_squared.get_unchecked(rem3).to_be());
    }
}

/// Emits 1 to 10 characters for the most significant digit.
#[inline(always)]
unsafe fn emit_partial_block(config: &Config, mut val: u64, len: usize, mut out_ptr: *mut u8) {
    let mut loops = len;

    // Optimize: Write 4 chars at a time
    while loops >= 4 {
        let rem = val % RADIX_58_4;
        val /= RADIX_58_4;

        let high = (rem / 3364) as usize;
        let low = (rem % 3364) as usize;

        unsafe {
            out_ptr = out_ptr.sub(2);
            (out_ptr as *mut u16).write_unaligned(config.lut_58_squared.get_unchecked(low).to_be());
            out_ptr = out_ptr.sub(2);
            (out_ptr as *mut u16)
                .write_unaligned(config.lut_58_squared.get_unchecked(high).to_be());
        }
        loops -= 4;
    }

    // Write remaining chars one by one
    while loops > 0 {
        unsafe {
            out_ptr = out_ptr.sub(1);
            *out_ptr = *config.alphabet.get_unchecked((val % 58) as usize);
        }
        val /= 58;
        loops -= 1;
    }
}

/// Orchestrates the writing of the bignum digits to the output buffer.
#[inline(always)]
unsafe fn write_digits_to_string(
    config: &Config,
    digits: &[u64],
    count: usize,
    dst_end: *mut u8,
) -> *mut u8 {
    // 1. Determine length of the most significant digit
    let mut last_val = *unsafe { digits.get_unchecked(count - 1) };
    let mut last_chunk_len = 0;
    loop {
        last_chunk_len += 1;
        last_val /= 58;
        if last_val == 0 {
            break;
        }
    }

    // 2. Set pointers
    let total_len = (count - 1) * 10 + last_chunk_len;
    let mut out_ptr = unsafe { dst_end.add(total_len) };
    let final_start_ptr = out_ptr;

    // 3. Emit full blocks
    for i in 0..(count - 1) {
        unsafe { emit_full_block(config, *digits.get_unchecked(i), &mut out_ptr) };
    }

    // 4. Emit MSB
    unsafe {
        emit_partial_block(
            config,
            *digits.get_unchecked(count - 1),
            last_chunk_len,
            out_ptr,
        )
    };

    final_start_ptr
}

// ----------------------------------------------------------------------
// Arithmetic Kernels (Fixed Size)
// ----------------------------------------------------------------------

/// Optimized kernel for 25 bytes.
#[inline(always)]
unsafe fn process_fixed_25(src: *const u8, out: &mut [u64]) -> usize {
    // 1. Read Inputs (1x u8 + 6x u32)
    let mut input = [0u32; 7];
    unsafe {
        input[0] = *src as u32;
        let src_u32 = src.add(1);
        input[1] = load_be_u32(src_u32);
        input[2] = load_be_u32(src_u32.add(4));
        input[3] = load_be_u32(src_u32.add(8));
        input[4] = load_be_u32(src_u32.add(12));
        input[5] = load_be_u32(src_u32.add(16));
        input[6] = load_be_u32(src_u32.add(20));
    }

    // 2. Matrix Multiplication (Base 58^5)
    let mut digits_5 = [0u64; 8];
    for i in 0..7 {
        let val = input[i] as u64;
        for k in 0..7 {
            digits_5[k + 1] += val * (TABLE_25[i][k] as u64);
        }
    }

    // 3. Reduction
    let mut carry = 0u64;
    let mut reduced = [0u64; 8];
    for k in (1..8).rev() {
        let val = digits_5[k] + carry;
        reduced[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    reduced[0] = digits_5[0] + carry;

    // 4. Pack into Base 58^10 (u64)
    out[0] = reduced[6] * RADIX_58_5 + reduced[7];
    out[1] = reduced[4] * RADIX_58_5 + reduced[5];
    out[2] = reduced[2] * RADIX_58_5 + reduced[3];
    out[3] = reduced[0] * RADIX_58_5 + reduced[1];

    4
}

/// Optimized kernel for 32 bytes.
#[inline(always)]
unsafe fn process_fixed_32(src: *const u8, out_digits: &mut [u64]) -> usize {
    // 1. Read Inputs (8x u32)
    let mut input = [0u32; 8];
    for i in 0..8 {
        input[i] = unsafe { load_be_u32(src.add(i * 4)) };
    }

    // 2. Matrix Multiplication
    let mut digits_5 = [0u128; 9];
    for i in 0..8 {
        let val = input[i] as u128;
        for k in 0..8 {
            digits_5[k + 1] += val * (TABLE_32[i][k] as u128);
        }
    }

    // 3. Reduction
    let mut carry = 0u64;
    let mut final_5 = [0u64; 9];
    for k in (1..9).rev() {
        let val = digits_5[k] + (carry as u128);
        final_5[k] = (val % (RADIX_58_5 as u128)) as u64;
        carry = (val / (RADIX_58_5 as u128)) as u64;
    }
    final_5[0] = (digits_5[0] as u64) + carry;

    // 4. Pack into Base 58^10
    out_digits[0] = final_5[7] * RADIX_58_5 + final_5[8];
    out_digits[1] = final_5[5] * RADIX_58_5 + final_5[6];
    out_digits[2] = final_5[3] * RADIX_58_5 + final_5[4];
    out_digits[3] = final_5[1] * RADIX_58_5 + final_5[2];
    out_digits[4] = final_5[0];

    if out_digits[4] > 0 { 5 } else { 4 }
}

/// Optimized kernel for 64 bytes.
#[inline(always)]
unsafe fn process_fixed_64(src: *const u8, out_digits: &mut [u64]) -> usize {
    // 1. Read Inputs (16x u32)
    let mut input = [0u32; 16];
    for i in 0..16 {
        input[i] = unsafe { load_be_u32(src.add(i * 4)) };
    }

    // 2. Accumulate (Split-Batching to avoid u64 overflow)
    // Needs 18 digits + 1 carry/overflow slot = 19
    let mut digits = [0u64; 19];

    // Batch 1: Inputs 0-7
    for i in 0..8 {
        let val = input[i] as u64;
        for k in 0..18 {
            digits[k + 1] += val * (TABLE_64[i][k] as u64);
        }
    }

    // Reduce Batch 1
    let mut carry = 0u64;
    for k in (1..19).rev() {
        let val = digits[k] + carry;
        digits[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    digits[0] += carry;
    carry = 0;

    // Batch 2: Inputs 8-15
    for i in 8..16 {
        let val = input[i] as u64;
        for k in 0..18 {
            digits[k + 1] += val * (TABLE_64[i][k] as u64);
        }
    }

    // Final Reduction
    for k in (1..19).rev() {
        let val = digits[k] + carry;
        digits[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    digits[0] += carry;

    // 3. Pack into Base 58^10
    out_digits[0] = digits[17] * RADIX_58_5 + digits[18];
    out_digits[1] = digits[15] * RADIX_58_5 + digits[16];
    out_digits[2] = digits[13] * RADIX_58_5 + digits[14];
    out_digits[3] = digits[11] * RADIX_58_5 + digits[12];
    out_digits[4] = digits[9] * RADIX_58_5 + digits[10];
    out_digits[5] = digits[7] * RADIX_58_5 + digits[8];
    out_digits[6] = digits[5] * RADIX_58_5 + digits[6];
    out_digits[7] = digits[3] * RADIX_58_5 + digits[4];
    out_digits[8] = digits[1] * RADIX_58_5 + digits[2];
    out_digits[9] = digits[0];

    if out_digits[9] > 0 { 10 } else { 9 }
}

/// Optimized kernel for 69 bytes.
#[inline(always)]
unsafe fn process_fixed_69(src: *const u8, out_digits: &mut [u64]) -> usize {
    // 1. Read Inputs (1x u8 + 16x u32 + 1x u32 tail)
    let mut input = [0u32; 18];

    unsafe {
        input[0] = *src as u32;

        // Efficient body reading using 64-bit loads
        let body_ptr = src.add(1);
        for i in 0..8 {
            let v = load_be_u64(body_ptr.add(i * 8));
            input[1 + i * 2] = (v >> 32) as u32;
            input[2 + i * 2] = v as u32;
        }
        input[17] = load_be_u32(src.add(65));
    }

    // 2. Accumulate (3 Batches)
    // Needs 20 digits + 1 carry slot = 21
    let mut digits = [0u64; 21];
    let mut carry = 0u64;

    // Helper closure to process a batch of 6 inputs
    let mut process_batch = |start_idx: usize| {
        for i in start_idx..(start_idx + 6) {
            let val = input[i] as u64;
            for k in 0..20 {
                digits[k + 1] += val * (TABLE_69[i][k] as u64);
            }
        }
        // Reduction
        for k in (1..21).rev() {
            let val = digits[k] + carry;
            digits[k] = val % RADIX_58_5;
            carry = val / RADIX_58_5;
        }
        digits[0] += carry;
        carry = 0;
    };

    process_batch(0);
    process_batch(6);
    process_batch(12);

    // 3. Pack
    out_digits[0] = digits[19] * RADIX_58_5 + digits[20];
    out_digits[1] = digits[17] * RADIX_58_5 + digits[18];
    out_digits[2] = digits[15] * RADIX_58_5 + digits[16];
    out_digits[3] = digits[13] * RADIX_58_5 + digits[14];
    out_digits[4] = digits[11] * RADIX_58_5 + digits[12];
    out_digits[5] = digits[9] * RADIX_58_5 + digits[10];
    out_digits[6] = digits[7] * RADIX_58_5 + digits[8];
    out_digits[7] = digits[5] * RADIX_58_5 + digits[6];
    out_digits[8] = digits[3] * RADIX_58_5 + digits[4];
    out_digits[9] = digits[1] * RADIX_58_5 + digits[2];
    out_digits[10] = digits[0];

    if out_digits[10] > 0 { 11 } else { 10 }
}

// ----------------------------------------------------------------------
// General Arithmetic Kernel
// ----------------------------------------------------------------------

/// General processor for variable lengths.
/// Internal State: Base 58^5 (u32 array).
#[inline(always)]
unsafe fn process_general(mut src: *const u8, mut len: usize, out_digits: &mut [u64]) -> usize {
    // Bypass zero-initialization to avoid memset overhead.
    let mut digits_5_uninit = core::mem::MaybeUninit::<[u32; 300]>::uninit();
    let digits_5_ptr = digits_5_uninit.as_mut_ptr() as *mut u32;
    unsafe {
        *digits_5_ptr = 0;
    }
    let digits_5 = unsafe { &mut *digits_5_uninit.as_mut_ptr() };
    let mut count_5 = 1;

    // ----------------------------------------------------------------------
    // 1. Smart Jump-Start
    // ----------------------------------------------------------------------

    if len >= 64 {
        // --- 64-Byte Initialization ---
        let mut input = [0u32; 16];
        let s_u64 = src as *const u64;

        for i in 0..8 {
            let v = unsafe { (s_u64.add(i)).read_unaligned().to_be() };
            input[i * 2] = (v >> 32) as u32;
            input[i * 2 + 1] = v as u32;
        }

        // Batch Matrix Mul (16 inputs -> 18 outputs (TABLE_64))
        // Needs 19 slots for accumulation
        let mut acc = [0u64; 19];
        let mut carry = 0u64;

        // Batch 1
        for i in 0..8 {
            let val = input[i] as u64;
            for k in 0..18 {
                acc[k + 1] += val * (TABLE_64[i][k] as u64);
            }
        }
        for k in (1..19).rev() {
            let val = acc[k] + carry;
            acc[k] = val % RADIX_58_5;
            carry = val / RADIX_58_5;
        }
        acc[0] += carry;
        carry = 0;

        // Batch 2
        for i in 8..16 {
            let val = input[i] as u64;
            for k in 0..18 {
                acc[k + 1] += val * (TABLE_64[i][k] as u64);
            }
        }
        for k in (1..19).rev() {
            let val = acc[k] + carry;
            acc[k] = val % RADIX_58_5;
            carry = val / RADIX_58_5;
        }
        acc[0] += carry;

        // Store to state (Little Endian)
        for k in 0..19 {
            digits_5[18 - k] = acc[k] as u32;
        }
        count_5 = if digits_5[18] == 0 { 18 } else { 19 };

        src = unsafe { src.add(64) };
        len -= 64;
    } else if len >= 32 {
        // --- 32-Byte Initialization (unchanged) ---
        let mut input = [0u32; 8];
        for i in 0..8 {
            input[i] = unsafe { load_be_u32(src.add(i * 4)) };
        }

        let mut acc = [0u64; 9];
        for i in 0..8 {
            let val = input[i] as u64;
            for k in 0..8 {
                acc[k + 1] += val * (TABLE_32[i][k] as u64);
            }
        }

        let mut carry = 0u64;
        for k in (1..9).rev() {
            let val = acc[k] + carry;
            digits_5[8 - k] = (val % RADIX_58_5) as u32;
            carry = val / RADIX_58_5;
        }
        let val = acc[0] + carry;
        digits_5[8] = (val % RADIX_58_5) as u32;

        count_5 = if digits_5[8] == 0 { 8 } else { 9 };

        src = unsafe { src.add(32) };
        len -= 32;
    }

    // ----------------------------------------------------------------------
    // 2. Accumulate Remaining Data
    // ----------------------------------------------------------------------

    unsafe {
        // Process 4-byte chunks
        while len >= 4 {
            let chunk = load_be_u32(src);

            // Bignum: digits_5 = digits_5 * (2^32) + chunk
            let mut carry = chunk as u64;
            let mut i = 0;
            while i < count_5 {
                let val = (*digits_5.get_unchecked(i) as u64) * (1u64 << 32) + carry;
                *digits_5.get_unchecked_mut(i) = (val % RADIX_58_5) as u32;
                carry = val / RADIX_58_5;
                i += 1;
            }
            while carry > 0 {
                *digits_5.get_unchecked_mut(count_5) = (carry % RADIX_58_5) as u32;
                carry /= RADIX_58_5;
                count_5 += 1;
            }

            src = src.add(4);
            len -= 4;
        }

        // Process Tail (1-3 bytes)
        if len > 0 {
            let mut chunk = 0u64;
            let mut shift = 0;
            for _ in 0..len {
                chunk = (chunk << 8) | (*src as u64);
                src = src.add(1);
                shift += 8;
            }

            let mut carry = chunk;
            let mut i = 0;
            while i < count_5 {
                let val = (*digits_5.get_unchecked(i) as u64) * (1u64 << shift) + carry;
                *digits_5.get_unchecked_mut(i) = (val % RADIX_58_5) as u32;
                carry = val / RADIX_58_5;
                i += 1;
            }
            while carry > 0 {
                *digits_5.get_unchecked_mut(count_5) = (carry % RADIX_58_5) as u32;
                carry /= RADIX_58_5;
                count_5 += 1;
            }
        }

        // ----------------------------------------------------------------------
        // 3. Final Packing (Base 58^5 u32 -> Base 58^10 u64)
        // ----------------------------------------------------------------------
        let mut out_count = 0;
        let mut i = 0;
        while i < count_5 {
            let low = *digits_5.get_unchecked(i) as u64;
            let high = if i + 1 < count_5 {
                *digits_5.get_unchecked(i + 1) as u64
            } else {
                0
            };

            *out_digits.get_unchecked_mut(out_count) = high * RADIX_58_5 + low;
            out_count += 1;
            i += 2;
        }

        out_count
    }
}

// ----------------------------------------------------------------------
// Entry Point
// ----------------------------------------------------------------------

#[inline(always)]
pub unsafe fn encode_slice_unsafe(input: &[u8], mut dst: *mut u8, config: &Config) -> usize {
    // Hard limit of 1024 bytes.
    assert!(input.len() <= 1024, "Input too big! {}", input.len());

    let mut len = input.len();
    let mut src = input.as_ptr();
    let dst_start = dst;

    // Fetch the specific zero character for this alphabet (e.g. '1' for Bitcoin, 'r' for Ripple)
    let z_char = *unsafe { config.alphabet.get_unchecked(0) };

    // Create a 8-byte pattern of the zero char for vectorized writing
    // e.g. if z_char is '1' (0x31), pattern is 0x3131313131313131
    let z_pattern = 0x0101010101010101 * (z_char as u64);

    unsafe {
        // 1. Skip and Write Leading Zeros
        while len >= 8 {
            if (src as *const u64).read_unaligned() == 0 {
                (dst as *mut u64).write_unaligned(z_pattern);
                dst = dst.add(8);
                src = src.add(8);
                len -= 8;
            } else {
                break;
            }
        }
        while len > 0 && *src == 0 {
            *dst = z_char;
            dst = dst.add(1);
            src = src.add(1);
            len -= 1;
        }

        if len == 0 {
            return dst.offset_from(dst_start) as usize;
        }
    }

    // 2. Dispatch to Kernel
    // Bypass zero-initialization to avoid memset overhead.
    let mut radix_digits_uninit = core::mem::MaybeUninit::<[u64; 160]>::uninit();
    let radix_digits = unsafe { &mut *radix_digits_uninit.as_mut_ptr() };

    let count = match len {
        25 => unsafe { process_fixed_25(src, radix_digits) },
        32 => unsafe { process_fixed_32(src, radix_digits) },
        64 => unsafe { process_fixed_64(src, radix_digits) },
        69 => unsafe { process_fixed_69(src, radix_digits) },
        _ => unsafe { process_general(src, len, radix_digits) },
    };

    // 3. Emit String
    let final_ptr = unsafe { write_digits_to_string(config, radix_digits, count, dst) };

    unsafe { final_ptr.offset_from(dst_start) as usize }
}

#[cfg(all(test, miri))]
mod base58_miri_coverage {
    use super::*;
    use rand::{RngExt, rng};

    // --- Mock Infrastructure ---
    fn random_bytes(len: usize) -> Vec<u8> {
        let mut rng = rng();
        (0..len).map(|_| rng.random()).collect()
    }

    /// Helper to verify Base58 encoding against the 'bs58' crate oracle
    fn verify_encode(config: &Config, input_len: usize) {
        let input = random_bytes(input_len);
        verify_exact(config, &input);
    }

    /// Helper to verify exact byte slices (useful for testing zero-prefixes)
    fn verify_exact(config: &Config, input: &[u8]) {
        // Oracle comparison (Assuming `bs58` crate in dev-dependencies)
        let expected = bs58::encode(input).into_string();

        // Calculate max safe length for Base58 (approx 138% of input)
        let len_required = (input.len() * 138) / 100 + 1;
        let mut dst = vec![0u8; len_required];

        let len = unsafe { encode_slice_unsafe(input, dst.as_mut_ptr(), config) };

        assert_eq!(
            std::str::from_utf8(&dst[..len]).unwrap(),
            expected,
            "Encode failed for input length: {}",
            input.len()
        );
    }

    // ----------------------------------------------------------------------
    // 1. Fixed Kernel Coverage
    // ----------------------------------------------------------------------

    #[test]
    fn miri_base58_encode_fixed_kernels() {
        // Substitute `Config::default()` with your actual constructor if needed
        let config =
            Config::new(b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz").unwrap();

        // Hits `process_fixed_25`
        verify_encode(&config, 25);

        // Hits `process_fixed_32`
        verify_encode(&config, 32);

        // Hits `process_fixed_64`
        verify_encode(&config, 64);

        // Hits `process_fixed_69`
        verify_encode(&config, 69);
    }

    // ----------------------------------------------------------------------
    // 2. General Kernel Coverage (Variable Lengths)
    // ----------------------------------------------------------------------

    #[test]
    fn miri_base58_encode_general_kernel() {
        let config =
            Config::new(b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz").unwrap();

        // Case: < 32 bytes (No jump start, hits 4-byte chunks and tail logic)
        verify_encode(&config, 3); // Tail only
        verify_encode(&config, 4); // Exactly one 4-byte chunk
        verify_encode(&config, 15); // Multiple 4-byte chunks + tail

        // Case: >= 32 bytes, < 64 bytes (Hits 32-byte jump start)
        verify_encode(&config, 33);
        verify_encode(&config, 45);

        // Case: >= 64 bytes (Hits 64-byte jump start)
        verify_encode(&config, 65);
        verify_encode(&config, 100);
    }

    // ----------------------------------------------------------------------
    // 3. Leading Zeros Logic Coverage
    // ----------------------------------------------------------------------

    #[test]
    fn miri_base58_encode_leading_zeros() {
        let config =
            Config::new(b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz").unwrap();

        // Single zero (Hits `while len > 0 && *src == 0`)
        verify_exact(&config, &[0]);

        // Few zeros (Hits single-byte zero loop)
        verify_exact(&config, &[0, 0, 0]);

        // Exactly 8 zeros (Hits the `u64` vectorized zero pattern check)
        verify_exact(&config, &[0, 0, 0, 0, 0, 0, 0, 0]);

        // More than 8 zeros (Hits vectorized loop + remainder loop)
        verify_exact(&config, &[0; 10]);

        // Entirely zeros matching a fixed kernel size (Ensures it bails early via `if len == 0`)
        verify_exact(&config, &[0; 25]);

        // Vectorized zeros followed by data (Hits break condition in vectorized loop)
        verify_exact(&config, &[0, 0, 0, 0, 0, 0, 0, 0, 255]);
    }

    // ----------------------------------------------------------------------
    // 4. Error / Panic Logic Coverage
    // ----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "Input too big!")]
    fn miri_base58_encode_panic_on_too_large() {
        let config =
            Config::new(b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz").unwrap();

        // Hard limit in code is 1024 bytes
        let input = vec![0u8; 1025];
        let mut dst = vec![0u8; 2000];

        unsafe {
            encode_slice_unsafe(&input, dst.as_mut_ptr(), &config);
        }
    }
}
