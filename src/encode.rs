//! Low-level Base58 encoding kernel.
//!
//! [`encode_slice`] is the zero-allocation primitive behind [`crate::Engine::encode_into`].
//! Prefer the [`crate::Engine`] methods unless you specifically need this unchecked entry point.

use crate::{Config, Error};

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
/// Result: `table[i][k]` = coefficient for `input_chunk[i]` contributing to `output_digit[k]`.
///
/// Each `as u32` below is a Base 58^5 remainder, which is always below
/// `RADIX_58_5 < 2^32` by construction.
#[allow(clippy::cast_possible_truncation)]
const fn generate_weights<const INPUTS: usize, const OUTPUTS: usize>() -> [[u32; OUTPUTS]; INPUTS] {
    let mut table = [[0u32; OUTPUTS]; INPUTS];
    let mut val = [0u32; 24];
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

/// Computes 2^512 as a little-endian Base 58^5 bignum.
///
/// 58^5 is just under 2^29.3, so 2^512 needs 18 digits (17 would only reach 2^498).
///
/// `current / RADIX_58_5` fits `u32` because `current < 2^32 * RADIX_58_5`, and
/// `rem` is a remainder of division by `RADIX_58_5 < 2^32`.
#[allow(clippy::cast_possible_truncation)]
const fn pow2_512_base_58_5() -> [u32; 18] {
    let mut val = [0u32; 24];
    val[16] = 1; // 2^(32*16)

    let mut out = [0u32; 18];
    let mut c = 0;
    while c < 18 {
        let mut rem = 0u64;
        let mut j = 24;
        while j > 0 {
            j -= 1;
            let current = (val[j] as u64) + (rem << 32);
            val[j] = (current / RADIX_58_5) as u32;
            rem = current % RADIX_58_5;
        }
        out[c] = rem as u32;
        c += 1;
    }
    out
}

// ----------------------------------------------------------------------
// Precomputed Weight Tables
// ----------------------------------------------------------------------

/// Powers of 58, from 58^0 up to 58^9.
const POW_58: [u64; 10] = {
    let mut table = [1u64; 10];
    let mut i = 1;
    while i < 10 {
        table[i] = table[i - 1] * 58;
        i += 1;
    }
    table
};

/// Limb count at which column-major (Comba) accumulation starts beating row-major.
const COMBA_MIN_LIMBS: usize = 96;

/// 2^512, the per-block multiplier for the 64-byte Horner loop.
const P_512: [u32; 18] = pow2_512_base_58_5();

// 25 bytes -> 7 input chunks (1x u8, 6x u32) -> 7 output digits
const TABLE_25: [[u32; 7]; 7] = generate_weights::<7, 7>();

// 32 bytes -> 8 input chunks (8x u32) -> 8 output digits
const TABLE_32: [[u32; 8]; 8] = generate_weights::<8, 8>();

// 64 bytes -> 16 input chunks -> 18 output digits
const TABLE_64: [[u32; 18]; 16] = generate_weights::<16, 18>();

/// Limb budget per length class for the general kernel, which now only runs
/// above 64 bytes.
const MEDIUM_LIMBS: usize = 96; // <= 320 bytes -> 88 limbs
const LARGE_LIMBS: usize = 288; // <= 1024 bytes -> 280 limbs

// ----------------------------------------------------------------------
// Memory Helpers
// ----------------------------------------------------------------------

/// Big-endian u32 load from the front of a slice.
///
/// # Panics
///
/// Panics if `src` has fewer than 4 bytes.
#[inline]
fn load_be_u32(src: &[u8]) -> u32 {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&src[..4]);
    u32::from_be_bytes(bytes)
}

// ----------------------------------------------------------------------
// Emission Helpers
// ----------------------------------------------------------------------

/// Number of Base58 characters needed to write `val`, which is a Base 58^10 digit
/// and so always below 58^10.
///
/// A balanced search over the ten possible answers: four compares against constants,
/// versus up to ten serial divisions for the equivalent divide-until-zero loop.
#[inline]
const fn digit_len(val: u64) -> usize {
    if val < POW_58[5] {
        if val < POW_58[2] {
            if val < POW_58[1] { 1 } else { 2 }
        } else if val < POW_58[3] {
            3
        } else if val < POW_58[4] {
            4
        } else {
            5
        }
    } else if val < POW_58[7] {
        if val < POW_58[6] { 6 } else { 7 }
    } else if val < POW_58[8] {
        8
    } else if val < POW_58[9] {
        9
    } else {
        10
    }
}

/// Emits exactly 10 characters for a full `u64` digit (Base 58^10).
///
/// Taking the destination as a fixed-size array makes every write a constant index,
/// so the whole body is bounds-check free.
#[inline]
fn emit_full_block(config: &Config, mut val: u64, out: &mut [u8; 10]) {
    // Split into three groups: 2 high chars, then two groups of 4.
    let low = (val % RADIX_58_4) as usize;
    val /= RADIX_58_4;
    let mid = (val % RADIX_58_4) as usize;
    let high = (val / RADIX_58_4) as usize;

    let lut = &config.lut_58_squared;
    out[0..2].copy_from_slice(&lut[high].to_be_bytes());
    out[2..4].copy_from_slice(&lut[mid / 3364].to_be_bytes());
    out[4..6].copy_from_slice(&lut[mid % 3364].to_be_bytes());
    out[6..8].copy_from_slice(&lut[low / 3364].to_be_bytes());
    out[8..10].copy_from_slice(&lut[low % 3364].to_be_bytes());
}

/// Emits the 1 to 10 characters of the most significant digit, filling `out` exactly.
#[inline]
fn emit_partial_block(config: &Config, mut val: u64, out: &mut [u8]) {
    let mut n = out.len();

    // Four at a time while there is room, using the squared lookup table.
    while n >= 4 {
        let rem = val % RADIX_58_4;
        val /= RADIX_58_4;

        let hi = (rem / 3364) as usize;
        let lo = (rem % 3364) as usize;

        out[n - 2..n].copy_from_slice(&config.lut_58_squared[lo].to_be_bytes());
        out[n - 4..n - 2].copy_from_slice(&config.lut_58_squared[hi].to_be_bytes());
        n -= 4;
    }

    while n > 0 {
        n -= 1;
        out[n] = config.alphabet[(val % 58) as usize];
        val /= 58;
    }
}

/// Writes the bignum digits into `dst` and returns the number of bytes written.
///
/// `digits` is little-endian Base 58^10, so `digits[0]` lands at the far right of
/// the output and the most significant digit supplies the leading, short chunk.
#[inline]
fn write_digits_to_string(config: &Config, digits: &[u64], dst: &mut [u8]) -> usize {
    let Some((&msb, rest)) = digits.split_last() else {
        return 0;
    };

    let last_chunk_len = digit_len(msb);
    let total_len = rest.len() * 10 + last_chunk_len;

    let (head, body) = dst[..total_len].split_at_mut(last_chunk_len);

    // `rchunks_exact_mut` walks right to left, matching the little-endian digits.
    // Each window is exactly 10 bytes, so it converts to an array reference for free.
    for (window, &digit) in body.rchunks_exact_mut(10).zip(rest) {
        // `rchunks_exact_mut(10)` guarantees every `window` is exactly 10 bytes.
        let arr: &mut [u8; 10] = window.try_into().unwrap_or_else(|_| unreachable!());
        emit_full_block(config, digit, arr);
    }
    emit_partial_block(config, msb, head);

    total_len
}

// ----------------------------------------------------------------------
// Arithmetic Kernels (Fixed Size)
// ----------------------------------------------------------------------

/// Optimized kernel for 25 bytes.
#[inline]
fn process_fixed_25(src: &[u8; 25], out: &mut [u64; 4]) -> usize {
    // 1. Read Inputs (1x u8 + 6x u32)
    let mut input = [0u32; 7];
    input[0] = u32::from(src[0]);
    for (slot, chunk) in input[1..].iter_mut().zip(src[1..].chunks_exact(4)) {
        *slot = load_be_u32(chunk);
    }

    // 2. Matrix Multiplication (Base 58^5)
    let mut digits_5 = [0u64; 8];
    for (k, slot) in digits_5[1..].iter_mut().enumerate() {
        let mut sum = 0u64;
        for (&x, row) in input.iter().zip(TABLE_25.iter()) {
            sum += u64::from(x) * u64::from(row[k]);
        }
        *slot = sum;
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
///
/// The `as u64` truncations in the reduction step are remainders/quotients of
/// division by `RADIX_58_5 < 2^32`, so they always fit `u64`.
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn process_fixed_32(src: &[u8; 32], out_digits: &mut [u64; 5]) -> usize {
    // 1. Read Inputs (8x u32)
    let mut input = [0u32; 8];
    for (slot, chunk) in input.iter_mut().zip(src.chunks_exact(4)) {
        *slot = load_be_u32(chunk);
    }

    // 2. Matrix Multiplication
    let mut digits_5 = [0u128; 9];
    for (k, slot) in digits_5[1..].iter_mut().enumerate() {
        let mut sum = 0u128;
        for (&x, row) in input.iter().zip(TABLE_32.iter()) {
            sum += u128::from(x) * u128::from(row[k]);
        }
        *slot = sum;
    }

    // 3. Reduction
    let mut carry = 0u64;
    let mut final_5 = [0u64; 9];
    let radix = u128::from(RADIX_58_5);
    for k in (1..9).rev() {
        let val = digits_5[k] + u128::from(carry);
        final_5[k] = (val % radix) as u64;
        carry = (val / radix) as u64;
    }
    final_5[0] = digits_5[0] as u64 + carry;

    // 4. Pack into Base 58^10
    out_digits[0] = final_5[7] * RADIX_58_5 + final_5[8];
    out_digits[1] = final_5[5] * RADIX_58_5 + final_5[6];
    out_digits[2] = final_5[3] * RADIX_58_5 + final_5[4];
    out_digits[3] = final_5[1] * RADIX_58_5 + final_5[2];
    out_digits[4] = final_5[0];

    if out_digits[4] > 0 { 5 } else { 4 }
}

/// Converts 64 bytes into 19 big-endian Base 58^5 accumulator slots.
///
/// The matrix multiply is split into two batches of 8 inputs so the u64 accumulators
/// cannot overflow: 8 * 2^32 * 58^5 < 2^64.
#[inline]
fn matrix_64(src: &[u8; 64]) -> [u64; 19] {
    let mut input = [0u32; 16];
    for (slot, chunk) in input.iter_mut().zip(src.chunks_exact(4)) {
        *slot = load_be_u32(chunk);
    }

    let mut acc = [0u64; 19];
    let mut carry = 0u64;

    for (k, slot) in acc[1..].iter_mut().enumerate() {
        let mut sum = 0u64;
        for (&x, row) in input[..8].iter().zip(TABLE_64[..8].iter()) {
            sum += u64::from(x) * u64::from(row[k]);
        }
        *slot = sum;
    }
    for k in (1..19).rev() {
        let val = acc[k] + carry;
        acc[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    acc[0] += carry;
    carry = 0;

    for (k, slot) in acc[1..].iter_mut().enumerate() {
        let mut sum = 0u64;
        for (&x, row) in input[8..].iter().zip(TABLE_64[8..].iter()) {
            sum += u64::from(x) * u64::from(row[k]);
        }
        *slot += sum;
    }
    for k in (1..19).rev() {
        let val = acc[k] + carry;
        acc[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    acc[0] += carry;

    acc
}

/// Encodes 32 bytes.
///
/// `src` never starts with a zero byte here: [`encode_slice`] strips the leading
/// zero run before dispatching.
#[inline]
fn encode_fixed_32(src: &[u8; 32], dst: &mut [u8], config: &Config) -> usize {
    let mut digits = [0u64; 5];
    let n = process_fixed_32(src, &mut digits);
    write_digits_to_string(config, &digits[..n], dst)
}

/// Encodes 64 bytes.
///
/// As with [`encode_fixed_32`], `src` never starts with a zero byte here.
#[inline]
fn encode_fixed_64(src: &[u8; 64], dst: &mut [u8], config: &Config) -> usize {
    let mut digits = [0u64; 10];
    let n = process_fixed_64(src, &mut digits);
    write_digits_to_string(config, &digits[..n], dst)
}

/// Optimized kernel for 64 bytes.
#[inline]
fn process_fixed_64(src: &[u8; 64], out_digits: &mut [u64; 10]) -> usize {
    let digits = matrix_64(src);

    // Pack into Base 58^10. `digits` is big-endian, `out_digits` little-endian.
    for (i, slot) in out_digits[..9].iter_mut().enumerate() {
        let k = 17 - 2 * i;
        *slot = digits[k] * RADIX_58_5 + digits[k + 1];
    }
    out_digits[9] = digits[0];

    if out_digits[9] > 0 { 10 } else { 9 }
}

// ----------------------------------------------------------------------
// Small-Input Matrix Kernel (<= 64 bytes)
// ----------------------------------------------------------------------
//
// The fixed 25/32/64 kernels above are fast because every trip count is a
// compile-time constant, so the dot products unroll and vectorize. Every other
// length under 64 bytes used to fall through to `process_general`, whose Horner
// loop costs one division per limb per 4 input bytes; at 31 bytes that is a
// serial chain of 36 divisions against the 9 of a matrix multiply.
//
// This kernel gives every length the fixed-kernel treatment. The input is
// right-aligned into the low word-rows of `TABLE_64`, so one table serves all
// widths, and W is a const generic so each length class monomorphizes into its
// own unrolled body.

/// Digit count per width, **rounded up to an even number**.
///
/// The rounding is not padding for its own sake. An odd digit count leaves a
/// scalar remainder on the 2-wide SSE2 dot product and measures as a hard cliff:
/// at W = 13 the natural 15 digits ran slower than 16 did. One extra
/// provably-zero leading digit costs `W` multiply-adds and one sweep limb and
/// buys back an even trip count. `dispatch_widths_match_spec` checks the
/// literals in the dispatch against this table.
#[cfg(test)]
const DIGITS_FOR_W: [usize; 17] = [0, 2, 4, 4, 6, 6, 8, 8, 10, 10, 12, 14, 14, 16, 16, 18, 18];

/// The bottom-right `W x D` corner of [`TABLE_64`]: rows for the low `W` input
/// words, columns for the low `D` digits.
///
/// Dropping the high columns is exact, not an approximation: a row for word `i`
/// carries the weight `2^(32*i) < 2^(32*W)`, which needs at most `D` digits, so
/// every entry above them is zero. Verified exhaustively for all 16 widths.
const fn sub_table<const W: usize, const D: usize>() -> [[u32; D]; W] {
    let mut t = [[0u32; D]; W];
    let mut r = 0;
    while r < W {
        let mut c = 0;
        while c < D {
            t[r][c] = TABLE_64[16 - W + r][18 - D + c];
            c += 1;
        }
        r += 1;
    }
    t
}

/// Carrier for the per-`(W, D)` sub-table. An item inside a function body cannot
/// see that function's generic parameters, so the table hangs off a generic impl.
struct Tab<const W: usize, const D: usize>;

impl<const W: usize, const D: usize> Tab<W, D> {
    const T: [[u32; D]; W] = sub_table::<W, D>();
}

/// Loads `src` right-aligned into `W` big-endian words, most significant first.
///
/// The most significant word holds the `len - 4*(W-1)` leading bytes, which is
/// 1 to 4, so lengths that are not a multiple of 4 need no separate tail pass.
#[inline]
fn load_words<const W: usize>(src: &[u8]) -> [u32; W] {
    let mut words = [0u32; W];
    let head = src.len() - 4 * (W - 1);

    let mut top = 0u32;
    for &b in &src[..head] {
        top = (top << 8) | u32::from(b);
    }
    words[0] = top;

    for (slot, chunk) in words[1..].iter_mut().zip(src[head..].chunks_exact(4)) {
        // `chunks_exact(4)` guarantees every `chunk` is exactly 4 bytes.
        let bytes: [u8; 4] = chunk.try_into().unwrap_or_else(|_| unreachable!());
        *slot = u32::from_be_bytes(bytes);
    }
    words
}

/// Matrix-multiplies `W` right-aligned words into `D` normalized, big-endian
/// Base 58^5 digits.
///
/// A u64 column accumulates `W` products of `x * p` with `x < 2^32` and
/// `p < 58^5`. Checked exactly against the real table rather than bounded
/// loosely: 11 rows is the widest batch that cannot overflow, so `W >= 12` runs
/// as two halves with a carry sweep after each. After the first sweep every
/// digit is below `RADIX_58_5`, which leaves room for the second batch's sum.
///
/// The `as u32` truncations are remainders of division by `RADIX_58_5 < 2^32`.
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn matrix<const W: usize, const D: usize>(words: &[u32; W]) -> [u64; D] {
    let table = &Tab::<W, D>::T;
    let mut acc = [0u64; D];

    // Rows this batch covers; `W >= 12` overflows a u64 column, so it runs the
    // low half first and folds the high half in after a reducing sweep.
    let first = if W <= 11 { 0 } else { W / 2 };

    for (k, slot) in acc.iter_mut().enumerate() {
        let mut sum = 0u64;
        for (&x, row) in words[first..].iter().zip(table[first..].iter()) {
            sum += u64::from(x) * u64::from(row[k]);
        }
        *slot = sum;
    }

    if first != 0 {
        sweep(&mut acc);
        for (k, slot) in acc.iter_mut().enumerate() {
            let mut sum = 0u64;
            for (&x, row) in words[..first].iter().zip(table[..first].iter()) {
                sum += u64::from(x) * u64::from(row[k]);
            }
            *slot += sum;
        }
    }

    sweep(&mut acc);
    acc
}

/// Carry-normalizes big-endian Base 58^5 digits in place.
#[inline]
fn sweep<const D: usize>(acc: &mut [u64; D]) {
    let mut carry = 0u64;
    for slot in acc.iter_mut().rev() {
        let val = *slot + carry;
        *slot = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    debug_assert_eq!(carry, 0, "the top digit absorbs the final carry");
}

/// Packs big-endian Base 58^5 digits into little-endian Base 58^10 digits.
///
/// `D` is always even, so the digits pair up exactly.
#[inline]
fn pack_pairs<const D: usize, const H: usize>(acc: &[u64; D], out: &mut [u64; H]) -> usize {
    for (n, slot) in out.iter_mut().enumerate() {
        let k = D - 2 * n;
        *slot = acc[k - 2] * RADIX_58_5 + acc[k - 1];
    }

    let mut count = H;
    while count > 1 && out[count - 1] == 0 {
        count -= 1;
    }
    count
}

/// Encodes 1 to 64 bytes through the matrix kernel for width `W`.
/// Kept `inline`: outlining the sixteen monomorphizations was measured both
/// ways and costs 4% on small inputs (1.52x -> 1.46x) without buying back the
/// >64-byte path, so the digit arrays are better off staying in registers.
#[inline]
fn process_small<const W: usize, const D: usize, const H: usize>(
    src: &[u8],
    dst: &mut [u8],
    config: &Config,
) -> usize {
    let words = load_words::<W>(src);
    let acc = matrix::<W, D>(&words);
    let mut digits = [0u64; H];
    let n = pack_pairs::<D, H>(&acc, &mut digits);
    write_digits_to_string(config, &digits[..n], dst)
}

// ----------------------------------------------------------------------
// General Arithmetic Kernel
// ----------------------------------------------------------------------

/// Converts one 64-byte block into little-endian Base 58^5 digits.
///
/// Returns the digit count (18, or 19 in the defensive overflow case).
///
/// `v` is a Base 58^5 digit from `matrix_64`, always below `RADIX_58_5 < 2^32`.
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn block_64_to_digits(src: &[u8; 64], out: &mut [u32; 19]) -> usize {
    let acc = matrix_64(src);

    // The matrix emits big-endian digits; the bignum state is little-endian.
    for (slot, &v) in out.iter_mut().rev().zip(acc.iter()) {
        *slot = v as u32;
    }

    if out[18] == 0 { 18 } else { 19 }
}

/// Folds whole 64-byte blocks into the bignum: `state = state * 2^512 + block`.
///
/// Schoolbook multiply by the 18-limb constant `P_512`, accumulated in u64. Each
/// output column takes at most 18 products of `(58^5-1)^2 < 2^58.6`, so the column
/// stays under `18 * 2^58.6 = 2^62.8` and nothing overflows before the single carry
/// pass at the end. That is one division per limb per 64 bytes, against sixteen for
/// the scalar Horner loop, and the multiply itself carries no loop-borne dependency.
///
/// The reduction stays a separate sweep on purpose. Folding it into the column loop
/// puts the divide latency directly in the carry chain, where 18 multiplies are not
/// enough to hide it; kept apart, the multiply has no loop-borne dependency at all
/// and the divide chain runs at its own throughput.
///
/// The `as u32` truncations are remainders of division by `RADIX_58_5 < 2^32`,
/// so they always fit `u32`.
#[inline(never)]
#[allow(clippy::cast_possible_truncation)]
fn absorb_blocks(src: &[u8], digits_5: &mut [u32], tmp: &mut [u64], mut count_5: usize) -> usize {
    let mut blk = [0u32; 19];

    for block in src.chunks_exact(64) {
        // `chunks_exact(64)` guarantees every `block` is exactly 64 bytes.
        let block: &[u8; 64] = block.try_into().unwrap_or_else(|_| unreachable!());
        let blk_count = block_64_to_digits(block, &mut blk);
        let n = count_5 + 18;

        // Terms are state[c-j] * P[j] for every j indexing both operands.
        if count_5 < COMBA_MIN_LIMBS {
            tmp[..n].fill(0);
            for (i, &limb) in digits_5[..count_5].iter().enumerate() {
                let a = u64::from(limb);
                for (slot, &p) in tmp[i..i + 18].iter_mut().zip(P_512.iter()) {
                    *slot += a * u64::from(p);
                }
            }
        } else {
            let mid_lo = 17.min(n);
            let mid_hi = count_5.max(mid_lo);

            // Leading ramp: fewer than 18 terms because the column is near the base.
            for c in 0..mid_lo {
                let k_max = c.min(count_5 - 1);
                let sum = digits_5[..=k_max]
                    .iter()
                    .zip(P_512[c - k_max..=c].iter().rev())
                    .map(|(&a, &p)| u64::from(a) * u64::from(p))
                    .sum();
                tmp[c] = sum;
            }
            // Middle: exactly 18 terms. `windows` hands out fixed-width slices, so
            // the inner trip count is constant and the compiler unrolls it.
            // 18 terms of (58^5-1)^2 < 2^58.6 leaves the column under 2^62.8.
            for (slot, window) in tmp[mid_lo..mid_hi]
                .iter_mut()
                .zip(digits_5[mid_lo - 17..].windows(18))
            {
                *slot = window
                    .iter()
                    .zip(P_512.iter().rev())
                    .map(|(&a, &p)| u64::from(a) * u64::from(p))
                    .sum();
            }
            // Trailing ramp: the column runs past the top limb of the state.
            for c in mid_hi..n {
                let sum = digits_5[c - 17..count_5]
                    .iter()
                    .zip(P_512[c + 1 - count_5..].iter().rev())
                    .map(|(&a, &p)| u64::from(a) * u64::from(p))
                    .sum();
                tmp[c] = sum;
            }
        }

        for (slot, &d) in tmp.iter_mut().zip(blk[..blk_count].iter()) {
            *slot += u64::from(d);
        }

        let mut carry = 0u64;
        for (&t, slot) in tmp[..n].iter().zip(digits_5[..n].iter_mut()) {
            let val = t + carry;
            *slot = (val % RADIX_58_5) as u32;
            carry = val / RADIX_58_5;
        }
        debug_assert_eq!(carry, 0, "state * 2^512 + block fits in count_5 + 18 limbs");

        count_5 = n;
        while count_5 > 1 && digits_5[count_5 - 1] == 0 {
            count_5 -= 1;
        }
    }

    count_5
}

/// Multiplies the state by `2^shift_bits` and adds `chunk`, in place.
///
/// The `as u32` truncations are remainders of division by `RADIX_58_5 < 2^32`,
/// so they always fit `u32`.
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn horner_step(digits_5: &mut [u32], count_5: &mut usize, multiplier: u64, chunk: u64) {
    let mut carry = chunk;
    for slot in &mut digits_5[..*count_5] {
        let val = u64::from(*slot) * multiplier + carry;
        *slot = (val % RADIX_58_5) as u32;
        carry = val / RADIX_58_5;
    }
    while carry > 0 {
        digits_5[*count_5] = (carry % RADIX_58_5) as u32;
        carry /= RADIX_58_5;
        *count_5 += 1;
    }
}

/// General processor for variable lengths.
/// Internal State: Base 58^5 (u32 array).
///
/// Deliberately `inline` rather than `inline(always)`: the dispatch below calls it
/// from four length classes, and duplicating the whole body into each one costs more
/// in instruction cache than the call saves.
///
/// The `as u32` truncations are remainders of division by `RADIX_58_5 < 2^32`, and
/// the `as u64` truncations pack two Base 58^5 digits (each `< RADIX_58_5 < 2^32`)
/// into a Base 58^10 digit, so both always fit their target type.
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn process_general(
    mut src: &[u8],
    digits_5: &mut [u32],
    tmp: &mut [u64],
    out_digits: &mut [u64],
) -> usize {
    let mut count_5 = 1;

    // ----------------------------------------------------------------------
    // 1. Head
    // ----------------------------------------------------------------------
    //
    // Everything that is not a whole 64-byte block is consumed first, while the
    // bignum is still short. The scalar Horner loop below costs one division per
    // limb per 4 bytes, so it must never run against a grown bignum.

    let mut blocks: &[u8] = &[];
    if src.len() >= 64 {
        let b = src.len() / 64;
        let head = src.len() % 64;

        if b >= 2 || head >= 16 {
            let (h, rest) = src.split_at(head);
            if head == 0 {
                // Nothing to scalar-process: seed the state from the first block.
                let mut blk = [0u32; 19];
                let block: &[u8; 64] = rest[..64].try_into().unwrap_or_else(|_| unreachable!());
                count_5 = block_64_to_digits(block, &mut blk);
                digits_5[..19].copy_from_slice(&blk);
                blocks = &rest[64..];
            } else {
                blocks = rest;
            }
            src = h;
        } else {
            // Exactly one block and a short remainder. Seeding from the leading
            // 64 bytes is free, so running the scalar loop over the few trailing
            // bytes is cheaper than paying a full multiply to fold the block in.
            let mut blk = [0u32; 19];
            let block: &[u8; 64] = src[..64].try_into().unwrap_or_else(|_| unreachable!());
            count_5 = block_64_to_digits(block, &mut blk);
            digits_5[..19].copy_from_slice(&blk);
            src = &src[64..];
        }
    }

    if src.len() >= 32 {
        // --- 32-Byte Initialization ---
        let mut input = [0u32; 8];
        for (slot, chunk) in input.iter_mut().zip(src[..32].chunks_exact(4)) {
            *slot = load_be_u32(chunk);
        }

        let mut acc = [0u64; 9];
        for (k, slot) in acc[1..].iter_mut().enumerate() {
            let mut sum = 0u64;
            for (&x, row) in input.iter().zip(TABLE_32.iter()) {
                sum += u64::from(x) * u64::from(row[k]);
            }
            *slot = sum;
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

        src = &src[32..];
    }

    // ----------------------------------------------------------------------
    // 2. Accumulate Remaining Data
    // ----------------------------------------------------------------------

    let mut chunks = src.chunks_exact(4);
    for chunk in chunks.by_ref() {
        horner_step(
            digits_5,
            &mut count_5,
            1u64 << 32,
            u64::from(load_be_u32(chunk)),
        );
    }

    let tail = chunks.remainder();
    if !tail.is_empty() {
        let mut chunk = 0u64;
        for &byte in tail {
            chunk = (chunk << 8) | u64::from(byte);
        }
        horner_step(digits_5, &mut count_5, 1u64 << (8 * tail.len()), chunk);
    }

    // ----------------------------------------------------------------------
    // 3. Block Horner: state = state * 2^512 + block
    // ----------------------------------------------------------------------

    if !blocks.is_empty() {
        count_5 = absorb_blocks(blocks, digits_5, tmp, count_5);
    }

    // ----------------------------------------------------------------------
    // 4. Final Packing (Base 58^5 u32 -> Base 58^10 u64)
    // ----------------------------------------------------------------------

    let mut out_count = 0;
    for (pair, slot) in digits_5[..count_5].chunks(2).zip(out_digits.iter_mut()) {
        let low = u64::from(pair[0]);
        let high = if pair.len() > 1 {
            u64::from(pair[1])
        } else {
            0
        };
        *slot = high * RADIX_58_5 + low;
        out_count += 1;
    }

    out_count
}

// ----------------------------------------------------------------------
// Entry Point
// ----------------------------------------------------------------------

/// Writes the encoded leading-zero run of `input` into `dst`, returning its length.
#[inline]
fn write_leading_zeros(input: &[u8], dst: &mut [u8], z_char: u8) -> usize {
    // Splatted across 8 bytes so leading zeros can be written a word at a time.
    let z_pattern = 0x0101_0101_0101_0101_u64 * u64::from(z_char);

    let mut zeros = 0;
    while input.len() - zeros >= 8 {
        // The `>= 8` guard above guarantees this 8-byte slice always exists.
        let word_bytes: [u8; 8] = input[zeros..zeros + 8]
            .try_into()
            .unwrap_or_else(|_| unreachable!());
        let word = u64::from_ne_bytes(word_bytes);
        if word != 0 {
            break;
        }
        dst[zeros..zeros + 8].copy_from_slice(&z_pattern.to_ne_bytes());
        zeros += 8;
    }
    while zeros < input.len() && input[zeros] == 0 {
        dst[zeros] = z_char;
        zeros += 1;
    }
    zeros
}

/// Encodes `input` into `dst`, returning the number of bytes written.
///
/// This is the zero-allocation kernel behind [`crate::Engine::encode_into`]; unlike
/// that method it does not check `dst` is large enough up front. Capped at a
/// 1024-byte input so its scratch can live on the stack; larger inputs go through
/// [`crate::Engine::encode`], which falls back to heap scratch.
///
/// `dst` must hold at least `Engine::encoded_len(input.len())` bytes.
///
/// # Errors
///
/// Returns [`Error::InputTooBig`] if `input` exceeds 1024 bytes.
///
/// # Panics
///
/// Panics if `dst` is too small to hold the result.
#[inline]
pub fn encode_slice(input: &[u8], dst: &mut [u8], config: &Config) -> Result<usize, Error> {
    if input.len() > 1024 {
        return Err(Error::InputTooBig);
    }

    let zeros = write_leading_zeros(input, dst, config.alphabet[0]);

    let src = &input[zeros..];
    if src.is_empty() {
        return Ok(zeros);
    }

    // Dispatch to kernel: each arm owns scratch sized for its length class, so
    // the zeroing cost stays proportional to the work being done.
    let dst = &mut dst[zeros..];
    let written = match src.len() {
        25 => {
            let mut digits = [0u64; 4];
            let src: &[u8; 25] = src.try_into().unwrap_or_else(|_| unreachable!());
            let n = process_fixed_25(src, &mut digits);
            write_digits_to_string(config, &digits[..n], dst)
        }
        32 => {
            let src: &[u8; 32] = src.try_into().unwrap_or_else(|_| unreachable!());
            encode_fixed_32(src, dst, config)
        }
        64 => {
            let src: &[u8; 64] = src.try_into().unwrap_or_else(|_| unreachable!());
            encode_fixed_64(src, dst, config)
        }
        // Every other length up to 64 bytes goes through the matrix kernel,
        // dispatched on its word count so W and D are compile-time constants.
        len if len <= 64 => match len.div_ceil(4) {
            1 => process_small::<1, 2, 1>(src, dst, config),
            2 => process_small::<2, 4, 2>(src, dst, config),
            3 => process_small::<3, 4, 2>(src, dst, config),
            4 => process_small::<4, 6, 3>(src, dst, config),
            5 => process_small::<5, 6, 3>(src, dst, config),
            6 => process_small::<6, 8, 4>(src, dst, config),
            7 => process_small::<7, 8, 4>(src, dst, config),
            8 => process_small::<8, 10, 5>(src, dst, config),
            9 => process_small::<9, 10, 5>(src, dst, config),
            10 => process_small::<10, 12, 6>(src, dst, config),
            11 => process_small::<11, 14, 7>(src, dst, config),
            12 => process_small::<12, 14, 7>(src, dst, config),
            13 => process_small::<13, 16, 8>(src, dst, config),
            14 => process_small::<14, 16, 8>(src, dst, config),
            15 => process_small::<15, 18, 9>(src, dst, config),
            _ => process_small::<16, 18, 9>(src, dst, config),
        },
        len if len <= 320 => {
            let mut limbs = [0u32; MEDIUM_LIMBS];
            let mut tmp = [0u64; MEDIUM_LIMBS];
            let mut digits = [0u64; MEDIUM_LIMBS / 2];
            let n = process_general(src, &mut limbs, &mut tmp, &mut digits);
            write_digits_to_string(config, &digits[..n], dst)
        }
        _ => {
            let mut limbs = [0u32; LARGE_LIMBS];
            let mut tmp = [0u64; LARGE_LIMBS];
            let mut digits = [0u64; LARGE_LIMBS / 2];
            let n = process_general(src, &mut limbs, &mut tmp, &mut digits);
            write_digits_to_string(config, &digits[..n], dst)
        }
    };

    Ok(zeros + written)
}

/// Encodes `input` into `dst` using heap-allocated scratch, with no size limit.
///
/// This backs [`crate::Engine::encode`] for inputs larger than [`encode_slice`]'s
/// stack-scratch ceiling.
///
/// `dst` must hold at least `Engine::encoded_len(input.len())` bytes.
///
/// # Panics
///
/// Panics if `dst` is too small to hold the result.
#[cfg(feature = "std")]
pub fn encode_slice_unbounded(input: &[u8], dst: &mut [u8], config: &Config) -> usize {
    let zeros = write_leading_zeros(input, dst, config.alphabet[0]);

    let src = &input[zeros..];
    if src.is_empty() {
        return zeros;
    }

    let dst = &mut dst[zeros..];

    // Scratch sized generously for `src.len()`; mirrors the `LARGE_LIMBS` formula,
    // scaled up instead of capped.
    let limb_budget = (src.len() * 8).div_ceil(29) + 2;
    let mut limbs = vec![0u32; limb_budget];
    let mut tmp = vec![0u64; limb_budget];
    let mut digits = vec![0u64; limb_budget / 2 + 1];

    let n = process_general(src, &mut limbs, &mut tmp, &mut digits);
    let written = write_digits_to_string(config, &digits[..n], dst);

    zeros + written
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
    fn digit_len_matches_division() {
        let reference = |mut v: u64| {
            let mut n = 0;
            loop {
                n += 1;
                v /= 58;
                if v == 0 {
                    return n;
                }
            }
        };
        for (k, &pow) in POW_58.iter().enumerate() {
            for delta in [0u64, 1, 2] {
                let v = pow + delta;
                assert_eq!(digit_len(v), reference(v), "58^{k} + {delta}");
                if pow > delta {
                    let v = pow - delta - 1;
                    assert_eq!(digit_len(v), reference(v), "58^{k} - {}", delta + 1);
                }
            }
        }
    }

    #[test]
    fn p_512_is_two_to_the_512() {
        // Reconstruct 2^512 from the limbs and compare against a shifted bignum.
        let mut acc = [0u32; 24];
        acc[16] = 1;

        let mut reference = [0u32; 18];
        let mut val = acc;
        for slot in reference.iter_mut() {
            let mut rem = 0u64;
            for word in val.iter_mut().rev() {
                let current = (*word as u64) + (rem << 32);
                *word = (current / RADIX_58_5) as u32;
                rem = current % RADIX_58_5;
            }
            *slot = rem as u32;
        }
        assert_eq!(P_512, reference);
    }

    #[test]
    fn scratch_classes_are_large_enough() {
        // Every length must fit the limb budget of the class it dispatches to.
        let limbs = |len: usize| (len * 8).div_ceil(29) + 2;
        assert!(limbs(320) <= MEDIUM_LIMBS, "medium class too tight");
        assert!(limbs(1024) <= LARGE_LIMBS, "large class too tight");
    }

    /// Schoolbook base-256 -> base-58 long division. Slow, obviously correct.
    fn encode_reference(input: &[u8], config: &Config) -> Vec<u8> {
        let zeros = input.iter().take_while(|&&b| b == 0).count();
        let mut digits: Vec<u8> = Vec::new();
        for &byte in &input[zeros..] {
            let mut carry = u32::from(byte);
            for d in digits.iter_mut() {
                let v = u32::from(*d) * 256 + carry;
                *d = (v % 58) as u8;
                carry = v / 58;
            }
            while carry > 0 {
                digits.push((carry % 58) as u8);
                carry /= 58;
            }
        }
        let mut out = vec![config.alphabet[0]; zeros];
        out.extend(digits.iter().rev().map(|&d| config.alphabet[d as usize]));
        out
    }

    /// Digits needed to write the largest `W`-word value in Base 58^5.
    fn digits_needed(w: usize) -> usize {
        let mut v = max_w_word_value(w);
        let mut n = 0;
        while !v.is_empty() {
            v = div_small(&v, RADIX_58_5);
            n += 1;
        }
        n
    }

    /// `2^(32*w) - 1` as little-endian base-2^32 limbs.
    fn max_w_word_value(w: usize) -> Vec<u64> {
        vec![u64::from(u32::MAX); w]
    }

    fn div_small(v: &[u64], d: u64) -> Vec<u64> {
        let mut out = vec![0u64; v.len()];
        let mut rem = 0u64;
        for i in (0..v.len()).rev() {
            let cur = (rem << 32) | v[i];
            out[i] = cur / d;
            rem = cur % d;
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    #[test]
    fn digits_for_w_is_even_and_sufficient() {
        for w in 1..=16 {
            let d = DIGITS_FOR_W[w];
            assert_eq!(d % 2, 0, "W={w}: D must be even for the 2-wide dot product");
            assert!(d <= 18, "W={w}: D={d} exceeds TABLE_64's 18 columns");
            assert!(
                d >= digits_needed(w),
                "W={w}: D={d} cannot hold {} digits",
                digits_needed(w)
            );
        }
    }

    #[test]
    fn sub_table_drops_only_zero_columns() {
        // Taking the low D columns is exact only if everything above them is zero.
        for w in 1..=16usize {
            let d = DIGITS_FOR_W[w];
            for r in (16 - w)..16 {
                for (k, &entry) in TABLE_64[r].iter().enumerate().take(18 - d) {
                    assert_eq!(entry, 0, "W={w}: TABLE_64[{r}][{k}] would be dropped");
                }
            }
        }
    }

    #[test]
    fn matrix_columns_cannot_overflow_u64() {
        // Worst case: every input word is u32::MAX. Mirrors `matrix`'s batching:
        // one batch to W=11, then a split at W/2.
        let x = u64::from(u32::MAX);
        for w in 1..=16usize {
            let d = DIGITS_FOR_W[w];
            let rows: Vec<usize> = ((16 - w)..16).collect();
            let col_sum = |batch: &[usize], k: usize| -> u128 {
                batch
                    .iter()
                    .map(|&r| u128::from(x) * u128::from(TABLE_64[r][18 - d + k]))
                    .sum()
            };
            let limit = u128::from(u64::MAX);
            if w <= 11 {
                for k in 0..d {
                    assert!(col_sum(&rows, k) <= limit, "W={w} col {k} overflows");
                }
            } else {
                // `matrix` accumulates the low half first, sweeps, then folds in
                // the high half on top of digits already below the radix.
                let half = w / 2;
                for k in 0..d {
                    assert!(
                        col_sum(&rows[half..], k) <= limit,
                        "W={w} first batch col {k}"
                    );
                    assert!(
                        col_sum(&rows[..half], k) + u128::from(RADIX_58_5) <= limit,
                        "W={w} second batch col {k}"
                    );
                }
            }
        }
    }

    #[test]
    fn matrix_kernel_matches_reference_for_every_small_length() {
        // The matrix kernel owns every length up to 64 except 25/32/64.
        for len in 1..=64usize {
            for pattern in 0..5u8 {
                let data: Vec<u8> = (0..len)
                    .map(|i| match pattern {
                        0 => (i as u8).wrapping_mul(31).wrapping_add(7),
                        1 => 0xff,
                        2 => u8::from(i + 1 == len),
                        3 => {
                            if i < len / 2 {
                                0
                            } else {
                                0xff
                            }
                        }
                        _ => 0x80,
                    })
                    .collect();
                let want = encode_reference(&data, BITCOIN.config());
                let got = BITCOIN.encode(&data).unwrap();
                assert_eq!(
                    got.as_bytes(),
                    &want[..],
                    "len {len} pattern {pattern} data {data:?}"
                );
            }
        }
    }

    #[test]
    fn dispatch_boundaries_round_trip() {
        // Lengths that sit on a class or kernel boundary.
        for len in [
            1, 24, 25, 26, 31, 32, 33, 63, 64, 65, 79, 80, 128, 319, 320, 321, 1023, 1024,
        ] {
            let data: Vec<u8> = (0..len)
                .map(|i| (i as u8).wrapping_mul(31).wrapping_add(7))
                .collect();
            let encoded = BITCOIN.encode(&data).unwrap();
            assert_eq!(BITCOIN.decode(&encoded).unwrap(), data, "len {len}");
        }
    }
}
