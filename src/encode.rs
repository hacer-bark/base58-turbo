//! Base58 encoding kernels behind [`crate::Engine`].

use crate::{Config, Error, POW_58, RADIX_58_4, RADIX_58_5};

// ----------------------------------------------------------------------
// Table Generation
// ----------------------------------------------------------------------

/// Weights converting big-endian u32 words into big-endian Base 58^5 digits:
/// `table[i][k]` is word `i`'s contribution to digit `k`.
#[allow(clippy::cast_possible_truncation)]
const fn generate_weights<const INPUTS: usize, const OUTPUTS: usize>() -> [[u32; OUTPUTS]; INPUTS] {
    let mut table = [[0u32; OUTPUTS]; INPUTS];
    let mut val = [0u32; 24];
    val[0] = 1;

    let mut i = 0;
    while i < INPUTS {
        let row_idx = INPUTS - 1 - i;

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

        // val *= 2^32
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

/// 2^512 as a little-endian Base 58^5 bignum: 18 digits, since 58^5 < 2^29.3.
#[allow(clippy::cast_possible_truncation)]
const fn pow2_512_base_58_5() -> [u32; 18] {
    let mut val = [0u32; 24];
    val[16] = 1;

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

/// Limb count from which column-major (Comba) accumulation beats row-major.
const COMBA_MIN_LIMBS: usize = 32;

/// 2^512, the per-block multiplier for the 64-byte Horner loop.
const P_512: [u32; 18] = pow2_512_base_58_5();

const TABLE_64: [[u32; 18]; 16] = generate_weights::<16, 18>();

/// Accumulator lanes for the 64-byte kernel, and the zero columns that pad
/// [`TABLE_64`]'s 18 up to a whole number of them.
const LANES: usize = 5;
const G_PAD: usize = LANES * 4 - 18;

/// [`TABLE_64`] regrouped into 4-wide lanes, with column `k` at flat index
/// `G_PAD + k`. The leading zero columns keep each row a whole number of lanes,
/// so the multiply-add vectorizes with the accumulators held in registers.
const TABLE_64_G: [[[u32; 4]; LANES]; 16] = {
    let mut t = [[[0u32; 4]; LANES]; 16];
    let mut i = 0;
    while i < 16 {
        let mut k = 0;
        while k < 18 {
            t[i][(G_PAD + k) / 4][(G_PAD + k) % 4] = TABLE_64[i][k];
            k += 1;
        }
        i += 1;
    }
    t
};

/// The one column of [`TABLE_64`] whose sixteen products overflow a u64.
const MINI: usize = G_PAD + 16;

/// Where the carry-select sweep cuts the 18-digit chain into three.
const SWEEP_CUTS: [usize; 2] = [G_PAD + 6, G_PAD + 12];

/// Limb budget per length class for the general kernel, which runs above 64 bytes.
const SMALL_LIMBS: usize = 40; // <= 128 bytes -> 38 limbs
const MEDIUM_LIMBS: usize = 96; // <= 320 bytes -> 88 limbs
const LARGE_LIMBS: usize = 288; // <= 1024 bytes -> 280 limbs

// ----------------------------------------------------------------------
// Memory Helpers
// ----------------------------------------------------------------------

#[inline]
fn load_be_u32(src: &[u8]) -> u32 {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&src[..4]);
    u32::from_be_bytes(bytes)
}

// ----------------------------------------------------------------------
// Emission Helpers
// ----------------------------------------------------------------------

/// Number of Base58 characters needed to write `val < 58^10`, as a balanced
/// compare tree rather than a divide-until-zero loop.
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

/// Emits exactly 10 characters for a Base 58^10 digit.
#[inline]
pub(crate) fn emit_full_block(config: &Config, mut val: u64, out: &mut [u8; 10]) {
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
pub(crate) fn emit_partial_block(config: &Config, mut val: u64, out: &mut [u8]) {
    let mut n = out.len();

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

/// Writes little-endian Base 58^10 `digits` into `dst`, returning the length.
/// Only the most significant digit is written short.
#[inline]
fn write_digits_to_string(config: &Config, digits: &[u64], dst: &mut [u8]) -> usize {
    let Some((&msb, rest)) = digits.split_last() else {
        return 0;
    };

    let last_chunk_len = digit_len(msb);
    let total_len = rest.len() * 10 + last_chunk_len;

    let (head, body) = dst[..total_len].split_at_mut(last_chunk_len);

    for (window, &digit) in body.rchunks_exact_mut(10).zip(rest) {
        let arr: &mut [u8; 10] = window.try_into().unwrap_or_else(|_| unreachable!());
        emit_full_block(config, digit, arr);
    }
    emit_partial_block(config, msb, head);

    total_len
}

// ----------------------------------------------------------------------
// 64-Byte Kernel
// ----------------------------------------------------------------------

/// Converts 64 bytes into 18 normalized big-endian Base 58^5 digits, laid out
/// as in [`TABLE_64_G`].
///
/// Row-major on purpose: column-major lowers to gathers, about 4x slower. Only
/// column [`MINI`] can overflow a u64 across all sixteen rows (the rest peak at
/// 2^63.87), so it alone is reduced between the two halves.
#[inline]
fn matrix_64(src: &[u8; 64]) -> [[u64; 4]; LANES] {
    let mut input = [0u32; 16];
    for (slot, chunk) in input.iter_mut().zip(src.chunks_exact(4)) {
        *slot = load_be_u32(chunk);
    }

    let mut acc = [[0u64; 4]; LANES];

    // Indexed on purpose: a slice iterator here stops the unroll.
    for i in 0..8 {
        let x = u64::from(input[i]);
        let row = &TABLE_64_G[i];
        for (g, lane) in acc.iter_mut().enumerate() {
            for l in 0..4 {
                lane[l] += x * u64::from(row[g][l]);
            }
        }
    }

    acc[(MINI - 1) / 4][(MINI - 1) % 4] += acc[MINI / 4][MINI % 4] / RADIX_58_5;
    acc[MINI / 4][MINI % 4] %= RADIX_58_5;

    for i in 8..16 {
        let x = u64::from(input[i]);
        let row = &TABLE_64_G[i];
        for (g, lane) in acc.iter_mut().enumerate() {
            for l in 0..4 {
                lane[l] += x * u64::from(row[g][l]);
            }
        }
    }

    sweep_64(&mut acc);
    acc
}

/// Reduces `flat[from..to]` from the bottom up, returning the carry out.
#[inline]
fn chain(flat: &mut [u64], from: usize, to: usize, carry_in: u64) -> u64 {
    let mut carry = carry_in;
    for k in (from..to).rev() {
        let val = flat[k] + carry;
        flat[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    carry
}

/// Adds `carry` into an already-normalized `flat[from..to]`, returning the rest.
#[inline]
fn fold(flat: &mut [u64], from: usize, to: usize, mut carry: u64) -> u64 {
    for k in (from..to).rev() {
        if carry == 0 {
            break;
        }
        let val = flat[k] + carry;
        flat[k] = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    carry
}

/// Carry-normalizes the 18 digits as three independent chains, then folds each
/// carry-out into the (already normalized) chain above. A single 18-step divide
/// chain is what the 64-byte kernel would otherwise spend most of its time on.
#[inline]
fn sweep_64(acc: &mut [[u64; 4]; LANES]) {
    let flat = acc.as_flattened_mut();

    let [cut_lo, cut_hi] = SWEEP_CUTS;

    let carry_lo = chain(flat, cut_hi, LANES * 4, 0);
    let carry_mid = chain(flat, cut_lo, cut_hi, 0);
    let carry_hi = chain(flat, G_PAD, cut_lo, 0);
    debug_assert_eq!(carry_hi, 0, "18 digits always hold a 64-byte input");

    let carry = fold(flat, cut_lo, cut_hi, carry_lo) + carry_mid;
    let carry = fold(flat, G_PAD, cut_lo, carry);
    debug_assert_eq!(carry, 0, "the top digit absorbs the final carry");
}

/// Encodes 64 bytes, or a shorter input zero-extended to 64, so the leading zero
/// digits still need trimming.
#[inline]
fn encode_fixed_64(src: &[u8; 64], dst: &mut [u8], config: &Config) -> usize {
    let acc = matrix_64(src);
    let digits = acc.as_flattened();

    // Big-endian Base 58^5 -> little-endian Base 58^10.
    let mut out_digits = [0u64; 9];
    for (i, slot) in out_digits.iter_mut().enumerate() {
        let k = MINI - 2 * i;
        *slot = digits[k] * RADIX_58_5 + digits[k + 1];
    }

    let mut count = out_digits.len();
    while count > 1 && out_digits[count - 1] == 0 {
        count -= 1;
    }

    write_digits_to_string(config, &out_digits[..count], dst)
}

// ----------------------------------------------------------------------
// Small-Input Matrix Kernel (<= 56 bytes)
// ----------------------------------------------------------------------
//
// The input is right-aligned into the low rows of `TABLE_64`, and `W` is a const
// generic, so each width gets its own fully unrolled dot product instead of the
// general kernel's serial divisions.

/// Digit count per width, rounded up to whole 4-wide lanes: a scalar remainder
/// measured 1.7x slower at W = 12.
const DIGITS_FOR_W: [usize; 15] = [0, 4, 4, 4, 8, 8, 8, 8, 12, 12, 12, 16, 16, 16, 16];

/// The bottom-right `W x D` corner of [`TABLE_64`]. The dropped columns are all
/// zero, since `2^(32*W)` needs at most `D` digits.
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

/// Carrier for the per-`(W, D)` sub-table, since a fn-local item cannot use the
/// fn's generics.
struct Tab<const W: usize, const D: usize>;

impl<const W: usize, const D: usize> Tab<W, D> {
    const T: [[u32; D]; W] = sub_table::<W, D>();
}

/// Loads `src` right-aligned into `W` big-endian words; the top word takes the
/// 1 to 4 leading bytes.
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
        let bytes: [u8; 4] = chunk.try_into().unwrap_or_else(|_| unreachable!());
        *slot = u32::from_be_bytes(bytes);
    }
    words
}

/// Matrix-multiplies `W` right-aligned words into `D` normalized, big-endian
/// Base 58^5 digits.
///
/// Up to 11 rows fit a u64 column, so `W >= 12` runs as two halves with a
/// sweep in between.
#[inline]
fn matrix<const W: usize, const D: usize>(words: &[u32; W]) -> [u64; D] {
    let table = &Tab::<W, D>::T;
    let mut acc = [0u64; D];

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

/// Packs big-endian Base 58^5 digits (`D` is even) into the `D / 2` little-endian
/// Base 58^10 digits of `out`.
#[inline]
fn pack_pairs<const D: usize>(acc: &[u64; D], out: &mut [u64]) -> usize {
    for (n, slot) in out.iter_mut().enumerate() {
        let k = D - 2 * n;
        *slot = acc[k - 2] * RADIX_58_5 + acc[k - 1];
    }

    let mut count = out.len();
    while count > 1 && out[count - 1] == 0 {
        count -= 1;
    }
    count
}

/// Encodes 1 to 56 bytes. Outlining this measured 4% slower on small inputs.
#[inline]
fn process_small<const W: usize, const D: usize>(
    src: &[u8],
    dst: &mut [u8],
    config: &Config,
) -> usize {
    let words = load_words::<W>(src);
    let acc = matrix::<W, D>(&words);
    let mut digits = [0u64; DIGITS_FOR_W[14] / 2];
    let n = pack_pairs::<D>(&acc, &mut digits[..D / 2]);
    write_digits_to_string(config, &digits[..n], dst)
}

// ----------------------------------------------------------------------
// General Arithmetic Kernel
// ----------------------------------------------------------------------

/// Converts one 64-byte block into 18 little-endian Base 58^5 limbs.
#[inline]
fn block_64_to_digits(src: &[u8; 64]) -> [u64; 18] {
    let acc = matrix_64(src);
    let digits = acc.as_flattened();

    let mut out = [0u64; 18];
    for (slot, &v) in out.iter_mut().rev().zip(&digits[G_PAD..]) {
        *slot = v;
    }
    out
}

/// Folds whole 64-byte blocks into the bignum: `state = state * 2^512 + block`.
///
/// Schoolbook multiply by the 18-limb `P_512`, then one carry pass. A column is
/// at most 18 products of `(58^5-1)^2 < 2^58.6`, so it stays under 2^62.8.
///
/// Limbs are below 58^5, so the `as u32` truncations are lossless; they tell LLVM
/// both factors fit 32 bits, which lowers the multiply to `pmuludq` and lets it
/// load adjacent u64 limbs without a shuffle.
#[inline(never)]
#[allow(clippy::cast_possible_truncation)]
fn absorb_blocks(src: &[u8], digits_5: &mut [u64], tmp: &mut [u64], mut count_5: usize) -> usize {
    for block in src.chunks_exact(64) {
        let block: &[u8; 64] = block.try_into().unwrap_or_else(|_| unreachable!());
        let blk = block_64_to_digits(block);
        let n = count_5 + 18;

        if count_5 < COMBA_MIN_LIMBS {
            tmp[..n].fill(0);
            for (i, &limb) in digits_5[..count_5].iter().enumerate() {
                let a = u64::from(limb as u32);
                for (slot, &p) in tmp[i..i + 18].iter_mut().zip(P_512.iter()) {
                    *slot += a * u64::from(p);
                }
            }
        } else {
            let mid_lo = 17.min(n);
            let mid_hi = count_5.max(mid_lo);

            // Leading ramp: columns with fewer than 18 terms.
            for c in 0..mid_lo {
                let k_max = c.min(count_5 - 1);
                let sum = digits_5[..=k_max]
                    .iter()
                    .zip(P_512[c - k_max..=c].iter().rev())
                    .map(|(&a, &p)| u64::from(a as u32) * u64::from(p))
                    .sum();
                tmp[c] = sum;
            }
            // Middle: exactly 18 terms, a constant trip count that unrolls.
            for (slot, window) in tmp[mid_lo..mid_hi]
                .iter_mut()
                .zip(digits_5[mid_lo - 17..].windows(18))
            {
                *slot = window
                    .iter()
                    .zip(P_512.iter().rev())
                    .map(|(&a, &p)| u64::from(a as u32) * u64::from(p))
                    .sum();
            }
            // Trailing ramp: columns past the top limb of the state.
            for c in mid_hi..n {
                let sum = digits_5[c - 17..count_5]
                    .iter()
                    .zip(P_512[c + 1 - count_5..].iter().rev())
                    .map(|(&a, &p)| u64::from(a as u32) * u64::from(p))
                    .sum();
                tmp[c] = sum;
            }
        }

        for (slot, &d) in tmp.iter_mut().zip(&blk) {
            *slot += d;
        }

        normalize(&tmp[..n], digits_5);

        count_5 = n;
        while count_5 > 1 && digits_5[count_5 - 1] == 0 {
            count_5 -= 1;
        }
    }

    count_5
}

#[inline]
const fn carry_step(t: u64, carry: &mut u64, slot: &mut u64) {
    let val = t + *carry;
    *slot = val % RADIX_58_5;
    *carry = val / RADIX_58_5;
}

/// Carry-normalizes little-endian `tmp` into `digits` as two interleaved divide
/// chains, then folds the lower half's carry-out into the (normalized) upper half.
#[inline]
fn normalize(tmp: &[u64], digits: &mut [u64]) {
    let h = tmp.len() / 2;
    let (t_lo, t_hi) = tmp.split_at(h);
    let (d_lo, d_hi) = digits[..tmp.len()].split_at_mut(h);

    let (mut c_lo, mut c_hi) = (0u64, 0u64);
    for ((&a, &b), (x, y)) in t_lo
        .iter()
        .zip(t_hi)
        .zip(d_lo.iter_mut().zip(d_hi.iter_mut()))
    {
        carry_step(a, &mut c_lo, x);
        carry_step(b, &mut c_hi, y);
    }
    for (&b, y) in t_hi[h..].iter().zip(d_hi[h..].iter_mut()) {
        carry_step(b, &mut c_hi, y);
    }
    debug_assert_eq!(c_hi, 0, "state * 2^512 + block fits in count_5 + 18 limbs");

    for slot in d_hi {
        if c_lo == 0 {
            break;
        }
        let val = *slot + c_lo;
        *slot = val % RADIX_58_5;
        c_lo = val / RADIX_58_5;
    }
    debug_assert_eq!(c_lo, 0, "state * 2^512 + block fits in count_5 + 18 limbs");
}

/// `state = state * multiplier + chunk`, in place.
#[inline]
fn horner_step(digits_5: &mut [u64], count_5: &mut usize, multiplier: u64, chunk: u64) {
    let mut carry = chunk;
    for slot in &mut digits_5[..*count_5] {
        let val = *slot * multiplier + carry;
        *slot = val % RADIX_58_5;
        carry = val / RADIX_58_5;
    }
    while carry > 0 {
        digits_5[*count_5] = carry % RADIX_58_5;
        carry /= RADIX_58_5;
        *count_5 += 1;
    }
}

/// General kernel for inputs above 64 bytes, over little-endian Base 58^5 limbs.
///
/// Leaves the result packed into Base 58^10 at the front of `digits_5` and
/// returns its digit count.
#[inline]
fn process_general(mut src: &[u8], digits_5: &mut [u64], tmp: &mut [u64]) -> usize {
    let mut count_5 = 1;

    // The partial head goes first, while the state is still short: the scalar
    // Horner loop costs a division per limb per 4 bytes.

    let mut blocks: &[u8] = &[];
    if src.len() >= 64 {
        let b = src.len() / 64;
        let head = src.len() % 64;

        if b >= 2 || head >= 16 {
            let (h, rest) = src.split_at(head);
            if head == 0 {
                let block: &[u8; 64] = rest[..64].try_into().unwrap_or_else(|_| unreachable!());
                digits_5[..18].copy_from_slice(&block_64_to_digits(block));
                count_5 = 18;
                blocks = &rest[64..];
            } else {
                blocks = rest;
            }
            src = h;
        } else {
            // One block and a short tail: seed from the block and Horner the tail
            // in, cheaper than a full block multiply.
            let block: &[u8; 64] = src[..64].try_into().unwrap_or_else(|_| unreachable!());
            digits_5[..18].copy_from_slice(&block_64_to_digits(block));
            count_5 = 18;
            src = &src[64..];
        }
    }

    if src.len() >= 32 {
        // 32 bytes need 9 digits, so the top three of the twelve stay zero.
        let acc = matrix::<8, { DIGITS_FOR_W[8] }>(&load_words::<8>(&src[..32]));
        for (slot, &d) in digits_5[..9].iter_mut().zip(acc.iter().rev()) {
            *slot = d;
        }
        count_5 = if acc[3] == 0 { 8 } else { 9 };

        src = &src[32..];
    }

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

    if !blocks.is_empty() {
        count_5 = absorb_blocks(blocks, digits_5, tmp, count_5);
    }

    // Pack pairs in place: slot `i` reads `2i` and `2i + 1`, never behind itself.
    let packed = count_5.div_ceil(2);
    for i in 0..packed {
        let high = if 2 * i + 1 < count_5 {
            digits_5[2 * i + 1]
        } else {
            0
        };
        digits_5[i] = high * RADIX_58_5 + digits_5[2 * i];
    }
    packed
}

/// Runs the general kernel with `N` limbs of stack scratch, out of line so short
/// inputs do not pay for the large frame.
#[inline(never)]
fn encode_scratch<const N: usize>(src: &[u8], dst: &mut [u8], config: &Config) -> usize {
    let mut limbs = [0u64; N];
    let mut tmp = [0u64; N];
    let n = process_general(src, &mut limbs, &mut tmp);
    write_digits_to_string(config, &limbs[..n], dst)
}

// ----------------------------------------------------------------------
// Entry Point
// ----------------------------------------------------------------------

/// Writes the encoded leading-zero run of `input` into `dst`, returning its length.
#[inline]
fn write_leading_zeros(input: &[u8], dst: &mut [u8], z_char: u8) -> usize {
    let z_pattern = 0x0101_0101_0101_0101_u64 * u64::from(z_char);

    let mut zeros = 0;
    while input.len() - zeros >= 8 {
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

/// Upper bound on the encoded length of `input_len` bytes: `floor(1.37 * n) + 1`,
/// since Base58 needs `log(256) / log(58) ≈ 1.366` characters per byte.
///
/// Computed in u64 so it cannot overflow for any 32-bit length.
#[inline]
#[allow(clippy::cast_possible_truncation)]
pub(crate) const fn encoded_len(input_len: usize) -> usize {
    let len = (input_len as u64).saturating_mul(137) / 100 + 1;
    if len > usize::MAX as u64 {
        usize::MAX
    } else {
        len as usize
    }
}

/// Encodes `input` into `dst`, returning the number of bytes written.
///
/// This is the zero-allocation kernel behind [`crate::Engine::encode_into`], with
/// the same contract. Capped at a 1024-byte input so its scratch can live on the
/// stack; larger inputs go through [`encode_slice_unbounded`].
///
/// # Errors
///
/// Returns [`Error::InputTooBig`] if `input` exceeds 1024 bytes, or
/// [`Error::BufferTooSmall`] if `dst` is shorter than
/// [`crate::Engine::encoded_len`] of the input.
#[inline]
pub(crate) fn encode_slice(input: &[u8], dst: &mut [u8], config: &Config) -> Result<usize, Error> {
    if input.is_empty() {
        return Ok(0);
    }
    if input.len() > 1024 {
        return Err(Error::InputTooBig);
    }
    if dst.len() < encoded_len(input.len()) {
        return Err(Error::BufferTooSmall);
    }

    let zeros = write_leading_zeros(input, dst, config.alphabet[0]);

    let src = &input[zeros..];
    if src.is_empty() {
        return Ok(zeros);
    }

    // Each arm sizes its scratch to its length class, so zeroing stays proportional.
    let dst = &mut dst[zeros..];
    let written = match src.len() {
        64 => {
            let src: &[u8; 64] = src.try_into().unwrap_or_else(|_| unreachable!());
            encode_fixed_64(src, dst, config)
        }
        // Dispatched on word count.
        len if len <= 56 => match len.div_ceil(4) {
            1 => process_small::<1, { DIGITS_FOR_W[1] }>(src, dst, config),
            2 => process_small::<2, { DIGITS_FOR_W[2] }>(src, dst, config),
            3 => process_small::<3, { DIGITS_FOR_W[3] }>(src, dst, config),
            4 => process_small::<4, { DIGITS_FOR_W[4] }>(src, dst, config),
            5 => process_small::<5, { DIGITS_FOR_W[5] }>(src, dst, config),
            6 => process_small::<6, { DIGITS_FOR_W[6] }>(src, dst, config),
            7 => process_small::<7, { DIGITS_FOR_W[7] }>(src, dst, config),
            8 => process_small::<8, { DIGITS_FOR_W[8] }>(src, dst, config),
            9 => process_small::<9, { DIGITS_FOR_W[9] }>(src, dst, config),
            10 => process_small::<10, { DIGITS_FOR_W[10] }>(src, dst, config),
            11 => process_small::<11, { DIGITS_FOR_W[11] }>(src, dst, config),
            12 => process_small::<12, { DIGITS_FOR_W[12] }>(src, dst, config),
            13 => process_small::<13, { DIGITS_FOR_W[13] }>(src, dst, config),
            _ => process_small::<14, { DIGITS_FOR_W[14] }>(src, dst, config),
        },
        // Zero-extending is exact and beats the two widest matrix classes.
        len if len < 64 => {
            let mut padded = [0u8; 64];
            padded[64 - len..].copy_from_slice(src);
            encode_fixed_64(&padded, dst, config)
        }
        len if len <= 128 => encode_scratch::<SMALL_LIMBS>(src, dst, config),
        len if len <= 320 => encode_scratch::<MEDIUM_LIMBS>(src, dst, config),
        _ => encode_scratch::<LARGE_LIMBS>(src, dst, config),
    };

    Ok(zeros + written)
}

/// Encodes `input` into `dst` using heap-allocated scratch, with no size limit.
///
/// This backs [`crate::Engine::encode`] for inputs larger than [`encode_slice`]'s
/// stack-scratch ceiling.
///
/// # Errors
///
/// Returns [`Error::BufferTooSmall`] if `dst` is shorter than
/// [`crate::Engine::encoded_len`] of the input, or [`Error::InputTooBig`] if the
/// scratch would exceed `isize::MAX` bytes (about 1 GB of input on 32-bit).
#[cfg(feature = "std")]
pub(crate) fn encode_slice_unbounded(
    input: &[u8],
    dst: &mut [u8],
    config: &Config,
) -> Result<usize, Error> {
    if input.is_empty() {
        return Ok(0);
    }
    if dst.len() < encoded_len(input.len()) {
        return Err(Error::BufferTooSmall);
    }

    let zeros = write_leading_zeros(input, dst, config.alphabet[0]);
    let src = &input[zeros..];
    if src.is_empty() {
        return Ok(zeros);
    }

    // At least `ceil(8n / 29) + 2` limbs, the `LARGE_LIMBS` budget, without the
    // `8n` that overflows on 32-bit targets.
    let limb_budget = src.len() / 29 * 8 + 10;
    if limb_budget > isize::MAX as usize / size_of::<u64>() {
        return Err(Error::InputTooBig);
    }
    let mut limbs = vec![0u64; limb_budget];
    let mut tmp = vec![0u64; limb_budget];
    let n = process_general(src, &mut limbs, &mut tmp);
    let written = write_digits_to_string(config, &limbs[..n], &mut dst[zeros..]);

    Ok(zeros + written)
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
        for (k, &pow) in POW_58[..10].iter().enumerate() {
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

    /// Const generators only run at compile time otherwise; this gives them coverage.
    #[test]
    fn table_generators_match_the_precomputed_consts() {
        assert_eq!(generate_weights::<16, 18>(), TABLE_64);
        assert_eq!(pow2_512_base_58_5(), P_512);
    }

    #[test]
    fn sub_table_matches_direct_call() {
        assert_eq!(sub_table::<1, 4>(), Tab::<1, 4>::T);
        assert_eq!(sub_table::<14, 16>(), Tab::<14, 16>::T);
    }

    #[test]
    fn write_digits_to_string_handles_empty_digits() {
        let mut dst = [0u8; 4];
        assert_eq!(write_digits_to_string(BITCOIN.config(), &[], &mut dst), 0);
    }

    /// The no-block path is otherwise only reached via a long zero prefix.
    #[test]
    #[cfg(feature = "std")]
    fn process_general_below_64_bytes_matches_public_api() {
        use rand::{RngExt, rng};

        let mut data: Vec<u8> = rng().random_iter().take(40).collect();
        data[0] |= 1;
        let mut limbs = [0u64; MEDIUM_LIMBS];
        let mut tmp = [0u64; MEDIUM_LIMBS];
        let n = process_general(&data, &mut limbs, &mut tmp);

        let mut dst = [0u8; 64];
        let len = write_digits_to_string(BITCOIN.config(), &limbs[..n], &mut dst);
        assert_eq!(
            std::str::from_utf8(&dst[..len]).unwrap(),
            BITCOIN.encode(&data).unwrap()
        );
    }

    #[test]
    #[cfg(feature = "std")]
    fn encode_kernels_reject_bad_sizes() {
        let cfg = BITCOIN.config();
        let mut dst = [0u8; 4096];
        assert_eq!(
            encode_slice(&[1u8; 1025], &mut dst, cfg),
            Err(Error::InputTooBig)
        );
        assert_eq!(
            encode_slice(&[1u8; 10], &mut dst[..13], cfg),
            Err(Error::BufferTooSmall)
        );
        assert_eq!(
            encode_slice_unbounded(&[1u8; 1500], &mut dst[..2055], cfg),
            Err(Error::BufferTooSmall)
        );
        assert_eq!(encode_slice_unbounded(&[], &mut [], cfg), Ok(0));
    }

    #[test]
    fn encoded_len_matches_the_plain_formula() {
        let lens = (0..100_000).chain(u32::MAX as usize / 137..=u32::MAX as usize / 137 + 2);
        for n in lens.chain([u32::MAX as usize]) {
            assert_eq!(encoded_len(n) as u128, n as u128 * 137 / 100 + 1, "n={n}");
        }
    }

    #[test]
    #[cfg(feature = "std")]
    fn encode_unbounded_all_zero_large_input() {
        let input = vec![0u8; 2000];
        let encoded = BITCOIN.encode(&input).unwrap();
        assert_eq!(encoded, "1".repeat(2000));
        assert_eq!(BITCOIN.decode(&encoded).unwrap(), input);
    }

    #[test]
    fn scratch_classes_are_large_enough() {
        let limbs = |len: usize| (len * 8).div_ceil(29) + 2;
        assert!(limbs(128) <= SMALL_LIMBS, "small class too tight");
        assert!(limbs(320) <= MEDIUM_LIMBS, "medium class too tight");
        assert!(limbs(1024) <= LARGE_LIMBS, "large class too tight");
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
    fn digits_for_w_is_lane_aligned_and_sufficient() {
        for w in 1..=14 {
            let d = DIGITS_FOR_W[w];
            assert_eq!(d % 4, 0, "W={w}: D must be a whole number of 4-wide lanes");
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
        for w in 1..=14usize {
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
        for w in 1..=14usize {
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
}

/// Kani proof harnesses for individual encoder kernels (`cargo kani`).
///
/// Each harness calls a private kernel with a fixed-size array so lengths stay
/// compile-time constants for CBMC; going through the public `AsRef<[u8]>`
/// entry point left loops unbounded, and a whole-pipeline harness ran out of
/// memory.
#[cfg(kani)]
mod kani_tests {
    use super::*;

    /// `write_leading_zeros` returns the leading-zero-byte count and writes
    /// exactly that many `z_char` bytes, for every input of 0 to 64 bytes.
    #[kani::proof]
    #[kani::unwind(72)]
    fn write_leading_zeros_is_correct_for_all_lengths_0_to_64() {
        let full: [u8; 64] = kani::any();
        let len: usize = kani::any();
        kani::assume(len <= 64);

        let input = &full[..len];
        let mut dst: [u8; 64] = kani::any();
        let z_char: u8 = kani::any();

        let zeros = write_leading_zeros(input, &mut dst[..len], z_char);

        // Reference: a plain scan for the leading-zero-byte run length.
        let mut expected = 0usize;
        while expected < len && input[expected] == 0 {
            expected += 1;
        }

        assert_eq!(zeros, expected, "len={len}");
        for i in 0..zeros {
            assert_eq!(dst[i], z_char, "len={len} i={i}");
        }
    }

    /// The 32-byte matrix leaves every digit below the radix, so each packed
    /// pair fits the 10 characters `digit_len` and `emit_partial_block` assume,
    /// and its top three digits at zero, for all 32-byte inputs.
    #[kani::proof]
    #[kani::unwind(13)]
    fn matrix_32_digits_are_normalized() {
        let src: [u8; 32] = kani::any();
        let acc = matrix::<8, { DIGITS_FOR_W[8] }>(&load_words::<8>(&src));

        assert!(
            acc[..3].iter().all(|&d| d == 0),
            "32 bytes need only 9 digits"
        );
        assert!(
            acc.iter().all(|&d| d < RADIX_58_5),
            "digits must be normalized"
        );
    }

    /// `matrix_64` leaves all 18 live digits below the radix and the two
    /// padding columns at zero, for all 64-byte inputs.
    #[kani::proof]
    #[kani::unwind(20)]
    fn matrix_64_digits_are_normalized() {
        let src: [u8; 64] = kani::any();
        let acc = matrix_64(&src);
        let digits = acc.as_flattened();

        for (k, &d) in digits.iter().enumerate() {
            if k < G_PAD {
                assert_eq!(d, 0, "padding lanes must stay zero");
            } else {
                assert!(
                    d < RADIX_58_5,
                    "digit {k} must be normalized below the radix"
                );
            }
        }
    }

    /// `digit_len` is exact for every `u64`, clamped at 10 characters.
    #[kani::proof]
    fn digit_len_is_exact_for_all_u64() {
        let val: u64 = kani::any();
        let n = digit_len(val);

        assert!((1..=10).contains(&n));
        if n > 1 {
            assert!(val >= POW_58[n - 1], "n={n} too large for val={val}");
        }
        if n < 10 {
            assert!(val < POW_58[n], "n={n} too small for val={val}");
        }
    }
}
