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
        (*out_ptr as *mut u16).write_unaligned(config.lut_58_squared.get_unchecked(rem1 % 3364).to_be());
        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16).write_unaligned(config.lut_58_squared.get_unchecked(rem1 / 3364).to_be());

        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16).write_unaligned(config.lut_58_squared.get_unchecked(rem2 % 3364).to_be());
        *out_ptr = out_ptr.sub(2);
        (*out_ptr as *mut u16).write_unaligned(config.lut_58_squared.get_unchecked(rem2 / 3364).to_be());

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
            (out_ptr as *mut u16).write_unaligned(config.lut_58_squared.get_unchecked(high).to_be());
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
unsafe fn write_digits_to_string(config: &Config, digits: &[u64], count: usize, dst_end: *mut u8) -> *mut u8 {
    // 1. Determine length of the most significant digit
    let mut last_val = *unsafe { digits.get_unchecked(count - 1) };
    let mut last_chunk_len = 0;
    loop {
        last_chunk_len += 1;
        last_val /= 58;
        if last_val == 0 { break; }
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
    unsafe { emit_partial_block(config, *digits.get_unchecked(count - 1), last_chunk_len, out_ptr) };

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
            input[1 + i*2] = (v >> 32) as u32;
            input[2 + i*2] = v as u32;
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
    let mut digits_5 = [0u32; 300];
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
            for k in 0..18 { acc[k + 1] += val * (TABLE_64[i][k] as u64); }
        }
        for k in (1..19).rev() {
            let val = acc[k] + carry; acc[k] = val % RADIX_58_5; carry = val / RADIX_58_5;
        }
        acc[0] += carry; carry = 0;

        // Batch 2
        for i in 8..16 {
            let val = input[i] as u64;
            for k in 0..18 { acc[k + 1] += val * (TABLE_64[i][k] as u64); }
        }
        for k in (1..19).rev() {
            let val = acc[k] + carry; acc[k] = val % RADIX_58_5; carry = val / RADIX_58_5;
        }
        acc[0] += carry;

        // Store to state (Little Endian)
        for k in 0..19 { digits_5[18 - k] = acc[k] as u32; }
        count_5 = if digits_5[18] == 0 { 18 } else { 19 };

        src = unsafe { src.add(64) };
        len -= 64;

    } else if len >= 32 {
        // --- 32-Byte Initialization (unchanged) ---
        let mut input = [0u32; 8];
        for i in 0..8 { input[i] = unsafe { load_be_u32(src.add(i*4)) }; }

        let mut acc = [0u64; 9];
        for i in 0..8 {
            let val = input[i] as u64;
            for k in 0..8 { acc[k + 1] += val * (TABLE_32[i][k] as u64); }
        }

        let mut carry = 0u64;
        for k in (1..9).rev() {
            let val = acc[k] + carry; digits_5[8 - k] = (val % RADIX_58_5) as u32; carry = val / RADIX_58_5;
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
            let high = if i + 1 < count_5 { *digits_5.get_unchecked(i + 1) as u64 } else { 0 };

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
    // Hard limit of 512 bytes.
    assert!(input.len() <= 512, "Input too big! {}", input.len());

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
    let mut radix_digits = [0u64; 128];
    let count = match len {
        25 => unsafe { process_fixed_25(src, &mut radix_digits) },
        32 => unsafe { process_fixed_32(src, &mut radix_digits) },
        64 => unsafe { process_fixed_64(src, &mut radix_digits) },
        69 => unsafe { process_fixed_69(src, &mut radix_digits) },
        _  => unsafe { process_general(src, len, &mut radix_digits) },
    };

    // 3. Emit String
    let final_ptr = unsafe { write_digits_to_string(config, &radix_digits, count, dst) };

    unsafe { final_ptr.offset_from(dst_start) as usize }
}

// Kani tests are not available yet. Thinking on how to optimize them for real world usage.

// #[cfg(kani)]
// mod kani_safety {
//     use super::*;

//     // -----------------------------------------------------------------
//     // 1. Mock Infrastructure (Safety Only)
//     // -----------------------------------------------------------------

//     /// Creates a valid Config struct but with dummy values.
//     /// 
//     /// Why this optimizes verification:
//     /// The solver doesn't need to calculate the exact Base58 characters.
//     /// It only needs to know that `lut_58_squared` has valid memory addresses.
//     /// Whether we write 'A' or '\0' to the buffer doesn't change Memory Safety.
//     fn get_safety_config() -> Config {
//         Config {
//             alphabet: [0u8; 58], 
//             lut_58_squared: [0u16; 3364],
//             decode_map: [0u8; 256],
//         }
//     }

//     /// The generic harness for safety checks.
//     /// It allocates a sufficiently large buffer and calls the unsafe function.
//     unsafe fn run_safety_check(input: &[u8]) {
//         let config = get_safety_config();
        
//         // Allocate an output buffer.
//         // Base58 expansion factor is ~1.37.
//         // For our largest test (69 bytes), we need ~95 bytes.
//         // We give 256 to be absolutely sure buffer size isn't the constraint 
//         // (unless checking buffer overrun logic specifically).
//         let mut output_buf = [0u8; 256];

//         // This call will fail verification if it:
//         // 1. Panics
//         // 2. Accesses input out of bounds
//         // 3. Writes output out of bounds
//         // 4. Performs invalid pointer arithmetic
//         unsafe { encode_slice_unsafe(
//             input,
//             output_buf.as_mut_ptr(),
//             &config
//         ); }
//     }

//     // -----------------------------------------------------------------
//     // 2. Safety Proofs (Kernel Coverage)
//     // -----------------------------------------------------------------

//     /// Verify 25-byte Kernel (Bitcoin Address size)
//     /// Checks `process_fixed_25` and the `emit_*` logic.
//     #[kani::proof]
//     #[kani::unwind(9)] // Unwind for fixed_25 loops + emission loops
//     fn safety_fixed_25() {
//         let input: [u8; 25] = kani::any();
//         unsafe { run_safety_check(&input) };
//     }

//     /// Verify 32-byte Kernel (Private Key size)
//     /// Checks `process_fixed_32`.
//     #[kani::proof]
//     #[kani::unwind(10)]
//     fn safety_fixed_32() {
//         let input: [u8; 32] = kani::any();
//         unsafe { run_safety_check(&input) };
//     }

//     /// Verify 64-byte Kernel (Signature size)
//     /// Checks `process_fixed_64`.
//     #[kani::proof]
//     #[kani::unwind(20)] // fixed_64 has loop of 18
//     fn safety_fixed_64() {
//         let input: [u8; 64] = kani::any();
//         unsafe { run_safety_check(&input) };
//     }

//     /// Verify 69-byte Kernel
//     /// Checks `process_fixed_69` (complex batching).
//     #[kani::proof]
//     #[kani::unwind(22)] 
//     fn safety_fixed_69() {
//         let input: [u8; 69] = kani::any();
//         unsafe { run_safety_check(&input) };
//     }

//     /// Verify General Path (Small)
//     /// Checks `process_general` loop logic for memory safety.
//     #[kani::proof]
//     #[kani::unwind(10)]
//     fn safety_general_small() {
//         // 5 bytes triggers the byte-by-byte accumulation loop
//         let input: [u8; 5] = kani::any();
//         unsafe { run_safety_check(&input) };
//     }

//     /// Verify General Path (Jump Start + Tail)
//     /// Checks the transition from 32-byte optimized block to byte tail.
//     #[kani::proof]
//     #[kani::unwind(15)]
//     fn safety_general_jump_start() {
//         // 33 bytes = 32 byte block + 1 byte tail
//         let input: [u8; 33] = kani::any();
//         unsafe { run_safety_check(&input) };
//     }

//     /// Verify Leading Zeros
//     /// Checks the pointer skipping logic at the start of `encode_slice_unsafe`.
//     #[kani::proof]
//     #[kani::unwind(20)]
//     fn safety_leading_zeros() {
//         // 16 bytes allows checking the 8-byte vectorized skip loop
//         let input: [u8; 16] = kani::any();
//         unsafe { run_safety_check(&input) };
//     }
// }
