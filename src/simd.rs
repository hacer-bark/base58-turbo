//! AVX2 encoding kernels, gated behind the `unsafe-simd` feature.
//!
//! This is the only module in the crate that contains `unsafe`. It is compiled
//! out entirely unless `unsafe-simd` is enabled, and even then every entry point
//! is reached only after a runtime `avx2` check, so a binary built with the
//! feature still runs correctly on pre-AVX2 hardware.
//!
//! # Approach
//!
//! The scalar kernels convert through Base 58^5 with 32-bit input words, which
//! suits scalar code: few digits, few multiplies. That shape is a poor fit for
//! AVX2, where the useful multiply (`vpmuludq`) only does four lanes and the
//! carry sweep needs 64-bit division.
//!
//! These kernels use **byte-sized input chunks and a Base 58^2 intermediate**
//! instead. Every consequence of that choice pays off in vector code:
//!
//! * `vpmaddwd` multiplies sixteen 16-bit pairs per uop, four times the density
//!   of `vpmuludq`;
//! * a column accumulates 32 products of `byte * 58^2`, so it peaks at 2^25 and
//!   the whole pipeline stays in 32-bit lanes (8-wide) rather than 64-bit;
//! * after one real division the values fit `u16`, so the remaining carry rounds
//!   have quotients of at most 3 and then 1, and compare-subtract replaces
//!   division outright;
//! * 22 digits at two characters each is exactly the 44-character maximum for a
//!   32-byte input, so emission is one `vpmulhuw` per sixteen digits and a
//!   two-way byte interleave.
//!
//! The digit-to-character map is four `vpshufb` lookups selected on bits 4 and
//! 5, which keeps arbitrary alphabets working.

#![allow(unsafe_code)]
// Edition 2024 makes `unsafe fn` bodies safe by default; these kernels are
// wall-to-wall intrinsics, so opt the module back into the older behaviour
// rather than wrapping every line.
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(
    // Intrinsics are imported wholesale; naming 36 of them adds no clarity.
    clippy::wildcard_imports,
    // The module is private, but pub(crate) documents the intended reach.
    clippy::redundant_pub_crate,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_ptr_alignment,
    clippy::too_many_lines,
    clippy::inline_always,
    clippy::doc_markdown
)]

use crate::{Config, Error};

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

/// 58^2, the intermediate radix.
const R2: u64 = 3364;

/// Base 58^2 digits in a 32-byte value: 3364^22 > 2^256 > 3364^21.
const NDIG: usize = 22;

/// Digit vectors: 22 digits padded to three 8-lane vectors.
const NV: usize = 3;

/// Leading padding lanes, so digit `d` lives in lane `d + PAD`.
const PAD: usize = NV * 8 - NDIG;

/// `(c * MAGIC) >> MAGIC_SH == c / 58^2` for every `c < 2^25`, verified
/// exhaustively over every multiple of the radix and its predecessor.
const MAGIC: i64 = 40_855_813;
const MAGIC_SH: i32 = 37;

/// `(x * M58) >> 16 == x / 58` for every `x < 3366`, so `vpmulhuw` does the
/// digit split in a single uop.
const M58: u16 = 1130;

// ----------------------------------------------------------------------
// Weight table
// ----------------------------------------------------------------------

/// Base 58^2 digits of `256^(31 - byte_index)`, most significant first.
const fn weight_digits(byte_index: usize) -> [u32; NDIG] {
    // 256^(31 - byte_index) as a little-endian base-2^32 bignum.
    let shift = 8 * (31 - byte_index);
    let mut val = [0u32; 8];
    val[shift / 32] = 1u32 << (shift % 32);

    let mut out = [0u32; NDIG];
    let mut k = NDIG;
    while k > 0 {
        k -= 1;
        let mut rem = 0u64;
        let mut j = 8;
        while j > 0 {
            j -= 1;
            let cur = (val[j] as u64) + (rem << 32);
            val[j] = (cur / R2) as u32;
            rem = cur % R2;
        }
        out[k] = rem as u32;
    }
    out
}

/// Weights packed for `vpmaddwd`: `PAIRS[p][v]` lane `j` holds the weights of
/// input bytes `2p` and `2p+1` for digit `v * 8 + j - PAD`, as two `u16`.
///
/// `vpmaddwd` multiplies 16-bit pairs and adds them within each 32-bit lane, so
/// one instruction folds two input bytes into eight digit columns at once.
const fn gen_pairs() -> [[[u32; 8]; NV]; 16] {
    let mut out = [[[0u32; 8]; NV]; 16];
    let mut p = 0;
    while p < 16 {
        let d0 = weight_digits(2 * p);
        let d1 = weight_digits(2 * p + 1);
        let mut v = 0;
        while v < NV {
            let mut j = 0;
            while j < 8 {
                let idx = v * 8 + j;
                if idx >= PAD {
                    let d = idx - PAD;
                    if d < NDIG {
                        out[p][v][j] = d0[d] | (d1[d] << 16);
                    }
                }
                j += 1;
            }
            v += 1;
        }
        p += 1;
    }
    out
}

const PAIRS: [[[u32; 8]; NV]; 16] = gen_pairs();

/// Byte pairs that reach each digit vector. The weight of byte `i` needs fewer
/// digits as `i` grows, so the table is triangular and the high digit vectors
/// only need the leading pairs: 30 multiplies instead of a dense 48.
const PAIRS_FOR_V: [usize; NV] = [4, 10, 16];

// ----------------------------------------------------------------------
// Runtime detection
// ----------------------------------------------------------------------

/// Whether AVX2 is available, resolved once.
///
/// `is_x86_feature_detected!` caches internally, but this keeps the check to a
/// single relaxed load on the hot path.
#[inline]
pub(crate) fn avx2_available() -> bool {
    use core::sync::atomic::{AtomicU8, Ordering};
    static STATE: AtomicU8 = AtomicU8::new(0);
    match STATE.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            let ok = is_x86_feature_detected!("avx2");
            STATE.store(u8::from(!ok) + 1, Ordering::Relaxed);
            ok
        }
    }
}

// ----------------------------------------------------------------------
// Kernel
// ----------------------------------------------------------------------

/// Shifts the 24-lane digit array down one 32-bit lane, so a carry moves from
/// digit `k` to digit `k - 1`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn shift_down(cur: __m256i, next: __m256i) -> __m256i {
    let t = _mm256_permute2x128_si256(cur, next, 0x21);
    _mm256_alignr_epi8(t, cur, 4)
}

/// As [`shift_down`], for 16-bit lanes.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn shift_down_w(cur: __m256i, next: __m256i) -> __m256i {
    let t = _mm256_permute2x128_si256(cur, next, 0x21);
    _mm256_alignr_epi8(t, cur, 2)
}

/// The four 16-entry `vpshufb` tables covering alphabet indices 0..58.
#[derive(Clone, Copy)]
pub(crate) struct AlphaTables([__m256i; 4]);

impl AlphaTables {
    /// Builds the shuffle tables for `config`'s alphabet.
    ///
    /// Tables 0..3 want alphabet bytes 0, 16, 32 and 48. The first three are
    /// plain 16-byte loads, but 48..64 would run past the 58-byte alphabet, so
    /// that one loads the in-bounds window 42..58 and shifts it down by 6 --
    /// `_mm_bsrli_si128` also zero-fills the six lanes past index 57, which are
    /// never selected anyway. Avoiding a staging copy matters because this runs
    /// on every single-input encode.
    #[target_feature(enable = "avx2")]
    pub(crate) unsafe fn new(config: &Config) -> Self {
        let a = config.alphabet.as_ptr();
        Self([
            _mm256_broadcastsi128_si256(_mm_loadu_si128(a.cast())),
            _mm256_broadcastsi128_si256(_mm_loadu_si128(a.add(16).cast())),
            _mm256_broadcastsi128_si256(_mm_loadu_si128(a.add(32).cast())),
            _mm256_broadcastsi128_si256(_mm_bsrli_si128(_mm_loadu_si128(a.add(42).cast()), 6)),
        ])
    }
}

/// Maps digit values (0..58) to alphabet characters.
///
/// Indices are 6 bits, so bits 4 and 5 pick the 16-entry table. A 16-bit left
/// shift by 3 (resp. 2) moves each byte's bit 4 (resp. 5) onto its own bit 7,
/// which is the bit `vpblendvb` tests -- cheaper than comparing the high nibble.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn map_alpha(v: __m256i, t: &AlphaTables) -> __m256i {
    let m4 = _mm256_slli_epi16(v, 3);
    let m5 = _mm256_slli_epi16(v, 2);
    let a = _mm256_blendv_epi8(
        _mm256_shuffle_epi8(t.0[0], v),
        _mm256_shuffle_epi8(t.0[1], v),
        m4,
    );
    let b = _mm256_blendv_epi8(
        _mm256_shuffle_epi8(t.0[2], v),
        _mm256_shuffle_epi8(t.0[3], v),
        m4,
    );
    _mm256_blendv_epi8(a, b, m5)
}

/// Loads 32 bytes as 16 `vpmaddwd` operands, one per byte pair, and returns the
/// leading zero-byte count from the same vector.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn load_pairs(src: &[u8; 32], w: &mut [u32; 16]) -> usize {
    let full = _mm256_loadu_si256(src.as_ptr().cast());
    let zmask =
        _mm256_movemask_epi8(_mm256_cmpeq_epi8(full, _mm256_setzero_si256())).cast_unsigned();
    let zeros = (!zmask).trailing_zeros() as usize;
    // Zero-extending bytes to u16 puts (b[2p], b[2p+1]) in adjacent u16 slots,
    // which is exactly one 32-bit vpmaddwd operand per pair.
    _mm256_storeu_si256(
        w.as_mut_ptr().cast(),
        _mm256_cvtepu8_epi16(_mm256_castsi256_si128(full)),
    );
    _mm256_storeu_si256(
        w.as_mut_ptr().add(8).cast(),
        _mm256_cvtepu8_epi16(_mm256_extracti128_si256(full, 1)),
    );
    zeros
}

/// Matrix-multiplies the byte pairs into 24 unnormalized Base 58^2 columns.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn matrix(w: &[u32; 16]) -> [__m256i; NV] {
    let mut acc = [_mm256_setzero_si256(); NV];
    let mut p = 0;
    while p < 16 {
        let a = _mm256_set1_epi32(w[p] as i32);
        let mut v = 0;
        while v < NV {
            if p < PAIRS_FOR_V[v] {
                acc[v] = _mm256_add_epi32(
                    acc[v],
                    _mm256_madd_epi16(a, _mm256_loadu_si256(PAIRS[p][v].as_ptr().cast())),
                );
            }
            v += 1;
        }
        p += 1;
    }
    acc
}

/// Carry-normalizes the columns and returns the digits as two `u16` vectors.
///
/// Round 1 is the only true division. After it every value is below 13337, so
/// rounds 2 and 3 have quotients of at most 3 and then 1 and use compare-
/// subtract, 16 lanes at a time.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn normalize(acc: [__m256i; NV]) -> (__m256i, __m256i) {
    let zero = _mm256_setzero_si256();
    let mv = _mm256_set1_epi64x(MAGIC);
    // [58^2, 0] per 32-bit lane: q is under 2^14, so vpmaddwd yields q * 58^2 in
    // one uop where vpmulld would cost two and ten cycles of latency.
    let rmul = _mm256_set1_epi32(R2 as i32);

    let q = |c: __m256i| -> __m256i {
        let e = _mm256_srli_epi64(_mm256_mul_epu32(c, mv), MAGIC_SH);
        let o = _mm256_srli_epi64(_mm256_mul_epu32(_mm256_srli_epi64(c, 32), mv), MAGIC_SH);
        _mm256_blend_epi32(e, _mm256_slli_epi64(o, 32), 0b1010_1010)
    };
    let (q0, q1, q2) = (q(acc[0]), q(acc[1]), q(acc[2]));
    let v0 = _mm256_add_epi32(
        _mm256_sub_epi32(acc[0], _mm256_madd_epi16(q0, rmul)),
        shift_down(q0, q1),
    );
    let v1 = _mm256_add_epi32(
        _mm256_sub_epi32(acc[1], _mm256_madd_epi16(q1, rmul)),
        shift_down(q1, q2),
    );
    let v2 = _mm256_add_epi32(
        _mm256_sub_epi32(acc[2], _mm256_madd_epi16(q2, rmul)),
        shift_down(q2, zero),
    );

    // Every value now fits u16, so the rest runs 16 lanes wide.
    let mut wa = _mm256_permute4x64_epi64(_mm256_packus_epi32(v0, v1), 0b11_01_10_00);
    let mut wb = _mm256_permute4x64_epi64(_mm256_packus_epi32(v2, zero), 0b11_01_10_00);

    let rw = _mm256_set1_epi16(R2 as i16);
    let t1 = _mm256_set1_epi16(3363);
    let t2 = _mm256_set1_epi16(6727);
    let t3 = _mm256_set1_epi16(10091);
    let q3 = |c: __m256i| {
        _mm256_sub_epi16(
            _mm256_sub_epi16(
                _mm256_sub_epi16(zero, _mm256_cmpgt_epi16(c, t1)),
                _mm256_cmpgt_epi16(c, t2),
            ),
            _mm256_cmpgt_epi16(c, t3),
        )
    };
    let (qa, qb) = (q3(wa), q3(wb));
    wa = _mm256_add_epi16(
        _mm256_sub_epi16(wa, _mm256_mullo_epi16(qa, rw)),
        shift_down_w(qa, qb),
    );
    wb = _mm256_add_epi16(
        _mm256_sub_epi16(wb, _mm256_mullo_epi16(qb, rw)),
        shift_down_w(qb, zero),
    );

    // Round 3: the AND must use the compare mask, not the count, since 1 & 3364
    // is zero.
    let ga = _mm256_cmpgt_epi16(wa, t1);
    let gb = _mm256_cmpgt_epi16(wb, t1);
    wa = _mm256_add_epi16(
        _mm256_sub_epi16(wa, _mm256_and_si256(ga, rw)),
        shift_down_w(_mm256_sub_epi16(zero, ga), _mm256_sub_epi16(zero, gb)),
    );
    wb = _mm256_add_epi16(
        _mm256_sub_epi16(wb, _mm256_and_si256(gb, rw)),
        shift_down_w(_mm256_sub_epi16(zero, gb), zero),
    );
    (wa, wb)
}

/// Splits each digit into its two characters: `[hi, lo]` per `u16` lane, which
/// is already the output byte order once stored little-endian.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn to_chars(w: __m256i) -> __m256i {
    let hi = _mm256_mulhi_epu16(w, _mm256_set1_epi16(M58 as i16));
    let lo = _mm256_sub_epi16(w, _mm256_mullo_epi16(hi, _mm256_set1_epi16(58)));
    _mm256_or_si256(hi, _mm256_slli_epi16(lo, 8))
}

/// Writes the characters for one encode into `dst`, returning the length.
///
/// `zeros` is the input's leading zero-byte count, which base58 renders as that
/// many `alphabet[0]` characters ahead of the value.
///
/// # Safety
///
/// AVX2 must be available. `dst` must hold `zeros + 44` bytes, or at least 44
/// when `zeros` is 0.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn emit(
    ca: __m256i,
    cb: __m256i,
    tabs: &AlphaTables,
    zeros: usize,
    alpha0: u8,
    dst: &mut [u8],
) -> usize {
    // The character vectors already are the output byte sequence, so a zero byte
    // is a leading zero character.
    let z = _mm256_setzero_si256();
    let s0 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(ca, z)).cast_unsigned();
    let s1 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(cb, z)).cast_unsigned();
    let sig = (!(u64::from(s0) | (u64::from(s1) << 32))) >> (2 * PAD);
    let first = if sig == 0 {
        2 * NDIG
    } else {
        sig.trailing_zeros() as usize
    };
    let total = 2 * NDIG - first;

    let mut raw = [0u8; 64];
    _mm256_storeu_si256(raw.as_mut_ptr().cast(), map_alpha(ca, tabs));
    _mm256_storeu_si256(raw.as_mut_ptr().add(32).cast(), map_alpha(cb, tabs));
    let sp = raw.as_ptr().add(2 * PAD + first);

    if zeros == 0 && total >= 32 {
        // The common case. A 32-byte store plus a 16-byte store at the tail
        // cover [0, total) exactly and never write past it.
        _mm256_storeu_si256(dst.as_mut_ptr().cast(), _mm256_loadu_si256(sp.cast()));
        _mm_storeu_si128(
            dst.as_mut_ptr().add(total - 16).cast(),
            _mm_loadu_si128(sp.add(total - 16).cast()),
        );
        return total;
    }

    // Leading zeros are rare in practice; take the narrow path rather than risk
    // a wide store running past the caller's buffer.
    for slot in &mut dst[..zeros] {
        *slot = alpha0;
    }
    core::ptr::copy_nonoverlapping(sp, dst.as_mut_ptr().add(zeros), total);
    zeros + total
}

/// Encodes 32 bytes, including any leading zero run.
///
/// # Safety
///
/// AVX2 must be available and `dst` must hold at least 44 bytes.
#[target_feature(enable = "avx2")]
pub(crate) unsafe fn encode_32_full(
    src: &[u8; 32],
    tabs: &AlphaTables,
    alpha0: u8,
    dst: &mut [u8],
) -> usize {
    let mut w = [0u32; 16];
    let zeros = load_pairs(src, &mut w);
    let (wa, wb) = normalize(matrix(&w));
    emit(to_chars(wa), to_chars(wb), tabs, zeros, alpha0, dst)
}

/// Encodes exactly 32 bytes whose leading byte is non-zero.
///
/// Returns the number of characters written, always 43 or 44: a value with a
/// non-zero leading byte is at least 2^248, which needs 43 Base58 characters,
/// and a 32-byte value never needs more than 44.
///
/// # Safety
///
/// The caller must have verified AVX2 support, and `dst` must hold at least 44
/// bytes.
#[target_feature(enable = "avx2")]
pub(crate) unsafe fn encode_32(src: &[u8; 32], tabs: &AlphaTables, dst: &mut [u8]) -> usize {
    debug_assert!(src[0] != 0, "caller strips the leading zero run");
    debug_assert!(dst.len() >= 44);

    let mut w = [0u32; 16];
    let _ = load_pairs(src, &mut w);
    let (wa, wb) = normalize(matrix(&w));
    emit(to_chars(wa), to_chars(wb), tabs, 0, 0, dst)
}

/// Encodes `n` 32-byte inputs with three encodes in flight at once.
///
/// A single encode is latency-bound: it measures around IPC 2.8 with the vector
/// ports barely half busy, because the whole kernel is one dependency chain.
/// Interleaving three independent inputs fills those gaps and is worth about
/// 15% per input over calling [`encode_32`] in a loop.
///
/// # Safety
///
/// The caller must have verified AVX2 support. `src` and `dst` must hold `n`
/// elements, and every `dst` element must be writable for 44 bytes.
#[target_feature(enable = "avx2")]
pub(crate) unsafe fn encode_32_x3(
    src: &[[u8; 32]],
    tabs: &AlphaTables,
    alpha0: u8,
    dst: &mut [[u8; 44]],
    lens: &mut [u8],
) {
    const K: usize = 3;
    let n = src.len();
    let mut base = 0;

    while base + K <= n {
        // Staggering the three inputs stage by stage is the point: each stage's
        // three copies are independent, so they fill each other's latency.
        let mut w = [[0u32; 16]; K];
        let mut zeros = [0usize; K];
        for k in 0..K {
            zeros[k] = load_pairs(&src[base + k], &mut w[k]);
        }
        let mut acc = [[_mm256_setzero_si256(); NV]; K];
        for k in 0..K {
            acc[k] = matrix(&w[k]);
        }
        for k in 0..K {
            let (wa, wb) = normalize(acc[k]);
            lens[base + k] = emit(
                to_chars(wa),
                to_chars(wb),
                tabs,
                zeros[k],
                alpha0,
                &mut dst[base + k][..],
            ) as u8;
        }
        base += K;
    }

    while base < n {
        let mut w = [0u32; 16];
        let zeros = load_pairs(&src[base], &mut w);
        let (wa, wb) = normalize(matrix(&w));
        lens[base] = emit(
            to_chars(wa),
            to_chars(wb),
            tabs,
            zeros,
            alpha0,
            &mut dst[base][..],
        ) as u8;
        base += 1;
    }
}

// ----------------------------------------------------------------------
// 64-byte kernel
// ----------------------------------------------------------------------

/// Base 58^2 digits in a 64-byte value: 3364^44 > 2^512 > 3364^43.
const NDIG64: usize = 44;

/// 44 digits padded to six 8-lane 32-bit vectors.
const NV64: usize = 6;

/// Three 16-lane vectors once the values fit `u16`.
const NW64: usize = 3;

const PAD64: usize = NV64 * 8 - NDIG64;

/// Base 58^2 digits of `256^(63 - byte_index)`, most significant first.
const fn weight_digits_64(byte_index: usize) -> [u32; NDIG64] {
    let shift = 8 * (63 - byte_index);
    let mut val = [0u32; 16];
    val[shift / 32] = 1u32 << (shift % 32);

    let mut out = [0u32; NDIG64];
    let mut k = NDIG64;
    while k > 0 {
        k -= 1;
        let mut rem = 0u64;
        let mut j = 16;
        while j > 0 {
            j -= 1;
            let cur = (val[j] as u64) + (rem << 32);
            val[j] = (cur / R2) as u32;
            rem = cur % R2;
        }
        out[k] = rem as u32;
    }
    out
}

/// As [`PAIRS`], for 64-byte inputs: 32 byte-pairs by 6 digit vectors.
const fn gen_pairs_64() -> [[[u32; 8]; NV64]; 32] {
    let mut out = [[[0u32; 8]; NV64]; 32];
    let mut p = 0;
    while p < 32 {
        let d0 = weight_digits_64(2 * p);
        let d1 = weight_digits_64(2 * p + 1);
        let mut v = 0;
        while v < NV64 {
            let mut j = 0;
            while j < 8 {
                let idx = v * 8 + j;
                if idx >= PAD64 {
                    let d = idx - PAD64;
                    if d < NDIG64 {
                        out[p][v][j] = d0[d] | (d1[d] << 16);
                    }
                }
                j += 1;
            }
            v += 1;
        }
        p += 1;
    }
    out
}

const PAIRS64: [[[u32; 8]; NV64]; 32] = gen_pairs_64();

/// Byte pairs reaching each digit vector; 104 multiplies instead of a dense 192.
/// The matrix hardcodes these bounds; `matrix_ranges_match_table` checks them.
#[cfg(test)]
const PAIRS_FOR_V64: [usize; NV64] = [3, 9, 14, 20, 26, 32];

/// Loads 64 bytes as 32 `vpmaddwd` operands and returns the leading zero count.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn load_pairs_64(src: &[u8; 64], w: &mut [u32; 32]) -> usize {
    let lo = _mm256_loadu_si256(src.as_ptr().cast());
    let hi = _mm256_loadu_si256(src.as_ptr().add(32).cast());
    let zero = _mm256_setzero_si256();
    let m = u64::from(_mm256_movemask_epi8(_mm256_cmpeq_epi8(lo, zero)).cast_unsigned())
        | (u64::from(_mm256_movemask_epi8(_mm256_cmpeq_epi8(hi, zero)).cast_unsigned()) << 32);
    let zeros = (!m).trailing_zeros() as usize;

    _mm256_storeu_si256(
        w.as_mut_ptr().cast(),
        _mm256_cvtepu8_epi16(_mm256_castsi256_si128(lo)),
    );
    _mm256_storeu_si256(
        w.as_mut_ptr().add(8).cast(),
        _mm256_cvtepu8_epi16(_mm256_extracti128_si256(lo, 1)),
    );
    _mm256_storeu_si256(
        w.as_mut_ptr().add(16).cast(),
        _mm256_cvtepu8_epi16(_mm256_castsi256_si128(hi)),
    );
    _mm256_storeu_si256(
        w.as_mut_ptr().add(24).cast(),
        _mm256_cvtepu8_epi16(_mm256_extracti128_si256(hi, 1)),
    );
    zeros
}

/// Encodes 64 bytes, including any leading zero run.
///
/// Same shape as the 32-byte kernel: a column here sums 64 products and peaks at
/// 2^25.7, still 39x inside a signed lane, so the identical magic constant and
/// the identical `u16` tail both carry over. Round 2 needs five compare steps
/// rather than three because the first quotient is twice as large.
///
/// # Safety
///
/// AVX2 must be available and `dst` must hold at least 88 bytes.
#[target_feature(enable = "avx2")]
pub(crate) unsafe fn encode_64_full(
    src: &[u8; 64],
    tabs: &AlphaTables,
    alpha0: u8,
    dst: &mut [u8],
) -> usize {
    let zero = _mm256_setzero_si256();
    let mut pairs = [0u32; 32];
    let zeros = load_pairs_64(src, &mut pairs);

    // --- matrix ---
    //
    // Spelled out as explicit ranges rather than a nested loop over
    // `PAIRS_FOR_V64`: at this size LLVM gives up on unrolling the loop form and
    // emits a runtime loop, which measured 3x slower. The ranges below are that
    // table transposed -- digit vector v takes byte pairs 0..PAIRS_FOR_V64[v].
    let mut a0 = zero;
    let mut a1 = zero;
    let mut a2 = zero;
    let mut a3 = zero;
    let mut a4 = zero;
    let mut a5 = zero;
    macro_rules! madd {
        ($acc:ident, $p:expr, $v:expr, $b:expr) => {
            $acc = _mm256_add_epi32(
                $acc,
                _mm256_madd_epi16($b, _mm256_loadu_si256(PAIRS64[$p][$v].as_ptr().cast())),
            );
        };
    }
    for p in 0..3 {
        let b = _mm256_set1_epi32(pairs[p] as i32);
        madd!(a0, p, 0, b);
        madd!(a1, p, 1, b);
        madd!(a2, p, 2, b);
        madd!(a3, p, 3, b);
        madd!(a4, p, 4, b);
        madd!(a5, p, 5, b);
    }
    for p in 3..9 {
        let b = _mm256_set1_epi32(pairs[p] as i32);
        madd!(a1, p, 1, b);
        madd!(a2, p, 2, b);
        madd!(a3, p, 3, b);
        madd!(a4, p, 4, b);
        madd!(a5, p, 5, b);
    }
    for p in 9..14 {
        let b = _mm256_set1_epi32(pairs[p] as i32);
        madd!(a2, p, 2, b);
        madd!(a3, p, 3, b);
        madd!(a4, p, 4, b);
        madd!(a5, p, 5, b);
    }
    for p in 14..20 {
        let b = _mm256_set1_epi32(pairs[p] as i32);
        madd!(a3, p, 3, b);
        madd!(a4, p, 4, b);
        madd!(a5, p, 5, b);
    }
    for p in 20..26 {
        let b = _mm256_set1_epi32(pairs[p] as i32);
        madd!(a4, p, 4, b);
        madd!(a5, p, 5, b);
    }
    for p in 26..32 {
        let b = _mm256_set1_epi32(pairs[p] as i32);
        madd!(a5, p, 5, b);
    }
    let acc = [a0, a1, a2, a3, a4, a5];

    // --- round 1: the only true divide ---
    let mv = _mm256_set1_epi64x(MAGIC);
    let rmul = _mm256_set1_epi32(R2 as i32);
    // Walking the digit vectors from least significant upward means only the
    // current quotient and its successor are ever live, instead of all six.
    let mut v32 = [zero; NV64];
    let mut qn = zero;
    let mut idx = NV64;
    while idx > 0 {
        idx -= 1;
        let c = acc[idx];
        let e = _mm256_srli_epi64(_mm256_mul_epu32(c, mv), MAGIC_SH);
        let o = _mm256_srli_epi64(_mm256_mul_epu32(_mm256_srli_epi64(c, 32), mv), MAGIC_SH);
        let qi = _mm256_blend_epi32(e, _mm256_slli_epi64(o, 32), 0b1010_1010);
        v32[idx] = _mm256_add_epi32(
            _mm256_sub_epi32(c, _mm256_madd_epi16(qi, rmul)),
            shift_down(qi, qn),
        );
        qn = qi;
    }

    // --- pack to u16 (every value is now < 19678) ---
    let mut wv = [zero; NW64];
    let mut k = 0;
    while k < NW64 {
        wv[k] = _mm256_permute4x64_epi64(
            _mm256_packus_epi32(v32[2 * k], v32[2 * k + 1]),
            0b11_01_10_00,
        );
        k += 1;
    }

    // --- round 2: quotient <= 5, so five compare-subtract steps ---
    let rw = _mm256_set1_epi16(R2 as i16);
    let t = [
        _mm256_set1_epi16(3363),
        _mm256_set1_epi16(6727),
        _mm256_set1_epi16(10091),
        _mm256_set1_epi16(13455),
        _mm256_set1_epi16(16819),
    ];
    let mut qw = [zero; NW64];
    let mut k = 0;
    while k < NW64 {
        let c = wv[k];
        let mut qi = zero;
        let mut step = 0;
        while step < 5 {
            qi = _mm256_sub_epi16(qi, _mm256_cmpgt_epi16(c, t[step]));
            step += 1;
        }
        qw[k] = qi;
        wv[k] = _mm256_sub_epi16(c, _mm256_mullo_epi16(qi, rw));
        k += 1;
    }
    let mut k = 0;
    while k < NW64 {
        let nxt = if k + 1 == NW64 { zero } else { qw[k + 1] };
        wv[k] = _mm256_add_epi16(wv[k], shift_down_w(qw[k], nxt));
        k += 1;
    }

    // --- round 3: quotient <= 1 ---
    let mut gw = [zero; NW64];
    let mut k = 0;
    while k < NW64 {
        let g = _mm256_cmpgt_epi16(wv[k], t[0]);
        gw[k] = g;
        wv[k] = _mm256_sub_epi16(wv[k], _mm256_and_si256(g, rw));
        k += 1;
    }
    let mut k = 0;
    while k < NW64 {
        let nxt = if k + 1 == NW64 {
            zero
        } else {
            _mm256_sub_epi16(zero, gw[k + 1])
        };
        wv[k] = _mm256_add_epi16(wv[k], shift_down_w(_mm256_sub_epi16(zero, gw[k]), nxt));
        k += 1;
    }

    // --- emit ---
    let c0 = to_chars(wv[0]);
    let c1 = to_chars(wv[1]);
    let c2 = to_chars(wv[2]);

    let z0 = u128::from(_mm256_movemask_epi8(_mm256_cmpeq_epi8(c0, zero)).cast_unsigned());
    let z1 = u128::from(_mm256_movemask_epi8(_mm256_cmpeq_epi8(c1, zero)).cast_unsigned());
    let z2 = u128::from(_mm256_movemask_epi8(_mm256_cmpeq_epi8(c2, zero)).cast_unsigned());
    let zmask = z0 | (z1 << 32) | (z2 << 64);
    // Only the low 96 bits are real character slots.
    let valid = (1u128 << 96) - 1;
    let sig = ((!zmask) & valid) >> (2 * PAD64);
    let first = if sig == 0 {
        2 * NDIG64
    } else {
        sig.trailing_zeros() as usize
    };
    let total = 2 * NDIG64 - first;

    let mut raw = [0u8; 96];
    _mm256_storeu_si256(raw.as_mut_ptr().cast(), map_alpha(c0, tabs));
    _mm256_storeu_si256(raw.as_mut_ptr().add(32).cast(), map_alpha(c1, tabs));
    _mm256_storeu_si256(raw.as_mut_ptr().add(64).cast(), map_alpha(c2, tabs));
    let sp = raw.as_ptr().add(2 * PAD64 + first);

    if zeros == 0 && total >= 64 {
        // total is 87 or 88 here, so three 32-byte stores tile [0, total) exactly.
        _mm256_storeu_si256(dst.as_mut_ptr().cast(), _mm256_loadu_si256(sp.cast()));
        _mm256_storeu_si256(
            dst.as_mut_ptr().add(32).cast(),
            _mm256_loadu_si256(sp.add(32).cast()),
        );
        _mm256_storeu_si256(
            dst.as_mut_ptr().add(total - 32).cast(),
            _mm256_loadu_si256(sp.add(total - 32).cast()),
        );
        return total;
    }

    for slot in &mut dst[..zeros] {
        *slot = alpha0;
    }
    core::ptr::copy_nonoverlapping(sp, dst.as_mut_ptr().add(zeros), total);
    zeros + total
}

// ======================================================================
// Decoding
// ======================================================================
//
// The scalar decoder is a Horner chain over the bignum, and counters put 92 of
// its 210 cycles at 44 characters into the character parse alone — front-end
// bound at 316 instructions, not limited by any one port. So the parse is
// attacked by shrinking the instruction count, and the arithmetic by moving
// the multiplies off port 1.
//
// **Parse.** The base64-style trick of a per-high-nibble additive offset does
// not work for base58: inside the `0x4_` group the Bitcoin alphabet maps `A..H`
// with one offset and `J..N` with another, because `I` is a hole. So the five
// high-nibble groups spanning ASCII 0x30..=0x7F are run as real 16-entry
// `vpshufb` tables — which are simply consecutive windows of the `decode_map`
// the crate already builds, so arbitrary alphabets keep working with no extra
// state. Anything outside that range is not an error; the kernel declines and
// the scalar path, which handles any alphabet, answers instead.
//
// Covering all eight groups would be fully general in one pass, but eight live
// tables plus working registers do not fit in 16 ymm: that version measured 63
// cycles against a ~20 cycle instruction budget, the gap being spills.
//
// **Fold.** Four characters become one base-58^4 digit in two instructions:
// `vpmaddubsw` by [58,1] gives pair values below 3364, then `vpmaddwd` by
// [3364,1] gives group values below 58^4. Neither crosses lanes, so dword `k`
// is exactly characters `4k..4k+4` in memory order.
//
// **Matrix.** Same flat radix conversion as the scalar path, but `vpmuludq`
// does four 32x32->64 products per uop instead of one `IMUL` on port 1. The
// accumulator needs no rebalancing: a 24-bit digit by a 32-bit limb is 56 bits,
// and at most 32 land in a column, so a 64-bit lane holds the sum with room
// left. Normalizing is carry-and-shift because the output radix is 2^32.

/// Largest exponent of 58^4 the decode table covers: enough for 128 characters.
const DMAXE: usize = 32;
/// Lanes per weight row: 58^124 needs 23 limbs, rounded to a whole vector.
const DVL: usize = 24;
/// Digit scratch: 8 slots of headroom, up to 32 digits, then store slack.
const DSCR: usize = 48;
/// Index of matrix row 0 (the scalar head digit) within the scratch.
const DROW0: usize = 7;
/// Character-count bounds of the vector path. The floor is what makes the
/// end-pinned 32-byte load legal; the ceiling bounds the table and the code.
const DMIN_CHARS: usize = 32;
const DMAX_CHARS: usize = 128;

const DP4: u64 = 11_316_496; // 58^4

/// 58^(4e) for e = 0..`DMAXE`, base-2^32 limbs zero-extended into 64-bit lanes.
///
/// One table shared by every length. Row `r` of an `R`-digit problem is the
/// weight 58^(4*(R-1-r)), so keying on the exponent instead of on `(R, r)`
/// collapses what would otherwise be a separate table per monomorphization.
const fn dweights() -> [[u64; DVL]; DMAXE] {
    let mut table = [[0u64; DVL]; DMAXE];
    let mut cur = [0u32; DVL];
    cur[0] = 1;
    let mut exp = 0;
    while exp < DMAXE {
        let mut limb = 0;
        while limb < DVL {
            table[exp][limb] = cur[limb] as u64;
            limb += 1;
        }
        // cur *= 58^4
        let mut carry = 0u64;
        let mut limb = 0;
        while limb < DVL {
            let prod = cur[limb] as u64 * DP4 + carry;
            cur[limb] = prod as u32;
            carry = prod >> 32;
            limb += 1;
        }
        exp += 1;
    }
    table
}

/// How many 4-lane vectors each exponent's weight actually occupies.
///
/// The table is triangular: 58^0 fills one limb, 58^124 fills twenty-three.
/// This has to be an explicit per-row constant bound — looping to the widest
/// row instead leaves LLVM emitting `vpmuludq` against the all-zero tail
/// vectors, roughly doubling the multiply count.
const fn dvecs() -> [usize; DMAXE] {
    let table = dweights();
    let mut counts = [0usize; DMAXE];
    let mut exp = 0;
    while exp < DMAXE {
        let mut hi = 0;
        let mut limb = 0;
        while limb < DVL {
            if table[exp][limb] != 0 {
                hi = limb + 1;
            }
            limb += 1;
        }
        counts[exp] = hi.div_ceil(4);
        exp += 1;
    }
    counts
}

static DWEIGHTS: [[u64; DVL]; DMAXE] = dweights();
static DVECS: [usize; DMAXE] = dvecs();

/// Parses `C` characters into scratch rows `DROW0..DROW0+R`.
///
/// Returns `false` if any character is invalid or outside ASCII 0x30..=0x7F,
/// leaving the caller to retry on the scalar path.
///
/// # Safety
///
/// AVX2 must be available, `src.len()` must equal `C`, and `C` must be at least
/// 32 so the end-pinned load stays inside `src`.
#[target_feature(enable = "avx2")]
unsafe fn dparse<const C: usize, const R: usize, const NV: usize>(
    config: &Config,
    src: &[u8],
    dig: &mut [u32; DSCR],
) -> bool {
    let m = config.decode_map.as_ptr();
    let t: [__m256i; 5] = core::array::from_fn(|i| {
        _mm256_broadcastsi128_si256(_mm_loadu_si128(m.add(16 * (3 + i)).cast()))
    });

    let p = src.as_ptr();
    let head = C - 4 * (R - 1);
    let mut suspect = _mm256_setzero_si256();

    for v in 0..NV {
        // Every vector but the last starts on a group boundary after the head;
        // the last is pinned to the end of the input so its load stays in
        // bounds. Overlapping vectors recompute the same digits with the same
        // values, so the overlap is harmless.
        let off = if v == NV - 1 { C - 32 } else { head + 32 * v };
        let ch = _mm256_loadu_si256(p.add(off).cast());

        let lo = _mm256_and_si256(ch, _mm256_set1_epi8(0x0f));
        let hi = _mm256_and_si256(_mm256_srli_epi16(ch, 4), _mm256_set1_epi8(0x0f));
        let mut acc = _mm256_setzero_si256();
        let mut covered = _mm256_setzero_si256();
        // Exactly one group matches a character, so the masked lookups compose
        // by OR. That is also one uop cheaper than `vpblendvb` here, and lets
        // the combine be a tree rather than a chain.
        for (i, tab) in t.iter().enumerate() {
            let sel = _mm256_cmpeq_epi8(hi, _mm256_set1_epi8((3 + i) as i8));
            covered = _mm256_or_si256(covered, sel);
            acc = _mm256_or_si256(acc, _mm256_and_si256(_mm256_shuffle_epi8(*tab, lo), sel));
        }
        // Invalid characters carry 0xFF from `decode_map`; uncovered ones match
        // no group at all. Both end up with the sign bit set.
        suspect = _mm256_or_si256(
            suspect,
            _mm256_or_si256(acc, _mm256_andnot_si256(covered, _mm256_set1_epi8(-1i8))),
        );

        let folded = _mm256_madd_epi16(
            _mm256_maddubs_epi16(acc, _mm256_set1_epi16(0x013a)),
            _mm256_set1_epi32(0x0001_0d24),
        );
        // The 8 slots of headroom are not slack: with fewer than 8 groups the
        // end-pinned load reaches back past the head, landing at a negative
        // group index. Those extra lanes are real characters, just unwanted.
        let base = (DROW0 + 1) as isize + (off as isize - head as isize) / 4;
        _mm256_storeu_si256(dig.as_mut_ptr().offset(base).cast(), folded);
    }

    // Head: at most four characters, scalar.
    let map = &config.decode_map;
    let mut acc = 0u32;
    let mut bad = 0u8;
    for i in 0..head {
        let x = map[src[i] as usize];
        bad |= x;
        acc = acc * 58 + u32::from(x);
    }
    dig[DROW0] = acc;

    _mm256_movemask_epi8(suspect) == 0 && bad & 0x80 == 0
}

/// Multiplies `R` digits by the weight matrix and normalizes to base-2^32 limbs.
///
/// # Safety
///
/// AVX2 must be available and `VV` must equal `ceil(L / 4)`.
#[target_feature(enable = "avx2")]
unsafe fn dmatrix<const R: usize, const L: usize, const VV: usize>(
    dig: &[u32; DSCR],
) -> [u32; DVL] {
    let mut acc = [_mm256_setzero_si256(); VV];

    for r in 0..R {
        let e = R - 1 - r;
        let d = _mm256_set1_epi64x(i64::from(dig[DROW0 + r]));
        let row = DWEIGHTS[e].as_ptr();
        // Only the vectors this exponent actually occupies; the bound must stay
        // a per-row constant so the triangular zeros never reach the pipeline.
        for (i, slot) in acc.iter_mut().enumerate().take(DVECS[e]) {
            let w = _mm256_loadu_si256(row.add(4 * i).cast());
            *slot = _mm256_add_epi64(*slot, _mm256_mul_epu32(d, w));
        }
    }

    let mut lane = [0u64; DVL];
    for (i, &vec) in acc.iter().enumerate() {
        _mm256_storeu_si256(lane.as_mut_ptr().add(4 * i).cast(), vec);
    }

    // Every lane is under 2^61 and every carry under 2^29, so one pass suffices.
    let mut limb = [0u32; DVL];
    let mut carry = 0u64;
    for (slot, &col) in limb[..L].iter_mut().zip(lane[..L].iter()) {
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
/// produce; since 58 < 256 the longest is `NBMIN + 1`, so the length costs one
/// `leading_zeros` and both likely copies are constant-size.
#[inline]
fn demit<const L: usize, const NBMIN: usize>(
    limb: &[u32; DVL],
    dst: &mut [u8],
) -> Result<usize, Error> {
    let mut be = [0u8; 4 * DVL];
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

/// # Safety
///
/// AVX2 must be available and the const parameters must match `C` as generated
/// in the dispatch table below.
#[target_feature(enable = "avx2")]
unsafe fn drun<
    const C: usize,
    const R: usize,
    const L: usize,
    const NB: usize,
    const NV: usize,
    const VV: usize,
>(
    config: &Config,
    src: &[u8],
    dst: &mut [u8],
) -> Option<Result<usize, Error>> {
    let mut dig = [0u32; DSCR];
    if !dparse::<C, R, NV>(config, src, &mut dig) {
        return None;
    }
    let limb = dmatrix::<R, L, VV>(&dig);
    Some(demit::<L, NB>(&limb, dst))
}

macro_rules! ddispatch {
    ($cfg:expr, $src:expr, $dst:expr, $n:expr,
     $( ($C:literal, $R:literal, $L:literal, $NB:literal, $NV:literal, $VV:literal) ),* $(,)?) => {
        match $n {
            $( $C => unsafe { drun::<$C, $R, $L, $NB, $NV, $VV>($cfg, $src, $dst) }, )*
            _ => None,
        }
    };
}

/// AVX2 decode of a zero-stripped payload.
///
/// Returns `None` when this kernel does not apply — no AVX2, a length outside
/// `DMIN_CHARS..=DMAX_CHARS`, or a character outside the ASCII range the five
/// resident tables cover — in which case the caller falls back to the scalar
/// path, which handles every alphabet.
pub(crate) fn decode_payload(
    config: &Config,
    src: &[u8],
    dst: &mut [u8],
) -> Option<Result<usize, Error>> {
    if src.len() < DMIN_CHARS || src.len() > DMAX_CHARS || !avx2_available() {
        return None;
    }
    ddispatch!(
        config,
        src,
        dst,
        src.len(),
        (32, 8, 6, 23, 1, 2),
        (33, 9, 7, 24, 1, 2),
        (34, 9, 7, 25, 1, 2),
        (35, 9, 7, 25, 1, 2),
        (36, 9, 7, 26, 1, 2),
        (37, 10, 7, 27, 2, 2),
        (38, 10, 7, 28, 2, 2),
        (39, 10, 8, 28, 2, 2),
        (40, 10, 8, 29, 2, 2),
        (41, 11, 8, 30, 2, 2),
        (42, 11, 8, 31, 2, 2),
        (43, 11, 8, 31, 2, 2),
        (44, 11, 9, 32, 2, 3),
        (45, 12, 9, 33, 2, 3),
        (46, 12, 9, 33, 2, 3),
        (47, 12, 9, 34, 2, 3),
        (48, 12, 9, 35, 2, 3),
        (49, 13, 9, 36, 2, 3),
        (50, 13, 10, 36, 2, 3),
        (51, 13, 10, 37, 2, 3),
        (52, 13, 10, 38, 2, 3),
        (53, 14, 10, 39, 2, 3),
        (54, 14, 10, 39, 2, 3),
        (55, 14, 11, 40, 2, 3),
        (56, 14, 11, 41, 2, 3),
        (57, 15, 11, 42, 2, 3),
        (58, 15, 11, 42, 2, 3),
        (59, 15, 11, 43, 2, 3),
        (60, 15, 11, 44, 2, 3),
        (61, 16, 12, 44, 2, 3),
        (62, 16, 12, 45, 2, 3),
        (63, 16, 12, 46, 2, 3),
        (64, 16, 12, 47, 2, 3),
        (65, 17, 12, 47, 2, 3),
        (66, 17, 13, 48, 2, 4),
        (67, 17, 13, 49, 2, 4),
        (68, 17, 13, 50, 2, 4),
        (69, 18, 13, 50, 3, 4),
        (70, 18, 13, 51, 3, 4),
        (71, 18, 13, 52, 3, 4),
        (72, 18, 14, 52, 3, 4),
        (73, 19, 14, 53, 3, 4),
        (74, 19, 14, 54, 3, 4),
        (75, 19, 14, 55, 3, 4),
        (76, 19, 14, 55, 3, 4),
        (77, 20, 15, 56, 3, 4),
        (78, 20, 15, 57, 3, 4),
        (79, 20, 15, 58, 3, 4),
        (80, 20, 15, 58, 3, 4),
        (81, 21, 15, 59, 3, 4),
        (82, 21, 16, 60, 3, 4),
        (83, 21, 16, 61, 3, 4),
        (84, 21, 16, 61, 3, 4),
        (85, 22, 16, 62, 3, 4),
        (86, 22, 16, 63, 3, 4),
        (87, 22, 16, 63, 3, 4),
        (88, 22, 17, 64, 3, 5),
        (89, 23, 17, 65, 3, 5),
        (90, 23, 17, 66, 3, 5),
        (91, 23, 17, 66, 3, 5),
        (92, 23, 17, 67, 3, 5),
        (93, 24, 18, 68, 3, 5),
        (94, 24, 18, 69, 3, 5),
        (95, 24, 18, 69, 3, 5),
        (96, 24, 18, 70, 3, 5),
        (97, 25, 18, 71, 3, 5),
        (98, 25, 18, 72, 3, 5),
        (99, 25, 19, 72, 3, 5),
        (100, 25, 19, 73, 3, 5),
        (101, 26, 19, 74, 4, 5),
        (102, 26, 19, 74, 4, 5),
        (103, 26, 19, 75, 4, 5),
        (104, 26, 20, 76, 4, 5),
        (105, 27, 20, 77, 4, 5),
        (106, 27, 20, 77, 4, 5),
        (107, 27, 20, 78, 4, 5),
        (108, 27, 20, 79, 4, 5),
        (109, 28, 20, 80, 4, 5),
        (110, 28, 21, 80, 4, 6),
        (111, 28, 21, 81, 4, 6),
        (112, 28, 21, 82, 4, 6),
        (113, 29, 21, 83, 4, 6),
        (114, 29, 21, 83, 4, 6),
        (115, 29, 22, 84, 4, 6),
        (116, 29, 22, 85, 4, 6),
        (117, 30, 22, 85, 4, 6),
        (118, 30, 22, 86, 4, 6),
        (119, 30, 22, 87, 4, 6),
        (120, 30, 22, 88, 4, 6),
        (121, 31, 23, 88, 4, 6),
        (122, 31, 23, 89, 4, 6),
        (123, 31, 23, 90, 4, 6),
        (124, 31, 23, 91, 4, 6),
        (125, 32, 23, 91, 4, 6),
        (126, 32, 24, 92, 4, 6),
        (127, 32, 24, 93, 4, 6),
        (128, 32, 24, 93, 4, 6)
    )
}

#[cfg(test)]
mod tests {

    /// The vector decode path must actually be taken for the lengths it claims.
    ///
    /// This is a tripwire as much as a test: a kernel that quietly stops being
    /// selected still passes every correctness test, because the scalar
    /// fallback answers instead.
    #[test]
    fn decode_path_is_live_and_matches_scalar() {
        if !avx2_available() {
            return;
        }
        // One engine materialized at a time: each carries a 3364-entry encode
        // LUT, so an array of them would be a 20 KB stack frame.
        for which in 0..3 {
            let engine = match which {
                0 => crate::BITCOIN,
                1 => crate::RIPPLE,
                _ => crate::FLICKR,
            };
            let config = engine.config();
            let mut seed = 0x9E37_79B9_7F4A_7C15u64;
            let mut next = move || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            };
            for chars in DMIN_CHARS..=DMAX_CHARS {
                for _ in 0..8 {
                    let input: Vec<u8> = (0..chars)
                        .map(|i| {
                            // Never start with the zero character: the kernel
                            // contracts on a payload that has already had its
                            // leading zeros stripped.
                            let d = (next() >> 33) % 58;
                            config.alphabet[if i == 0 {
                                d.max(1) as usize
                            } else {
                                d as usize
                            }]
                        })
                        .collect();

                    let mut fast = [0u8; 128];
                    let taken = decode_payload(config, &input, &mut fast);
                    assert!(taken.is_some(), "vector path declined {chars} chars");
                    let n = taken.unwrap().unwrap();

                    let want = engine.decode(&input).unwrap();
                    assert_eq!(&fast[..n], &want[..], "chars={chars}");
                }
            }
        }
    }

    /// Characters outside the resident ASCII window must be declined, not
    /// mis-decoded, so the scalar path can answer for exotic alphabets.
    #[test]
    fn decode_path_declines_out_of_range_characters() {
        if !avx2_available() {
            return;
        }
        let config = crate::BITCOIN.config();
        for pos in 0..44 {
            let mut input = [config.alphabet[7]; 44];
            input[pos] = b'\x20'; // below the 0x30 floor
            let mut dst = [0u8; 64];
            assert!(
                decode_payload(config, &input, &mut dst).is_none(),
                "pos={pos}"
            );
        }
    }
    use super::*;

    /// The 64-byte matrix hardcodes which byte pairs reach which digit vector.
    /// If the table is ever regenerated, those ranges must still bound every
    /// non-zero weight, or the kernel would silently drop terms.
    #[test]
    fn matrix_ranges_match_table() {
        for (v, &bound) in PAIRS_FOR_V64.iter().enumerate() {
            for (p, row) in PAIRS64.iter().enumerate() {
                let nonzero = row[v].iter().any(|&x| x != 0);
                assert!(
                    !nonzero || p < bound,
                    "digit vector {v} has a non-zero weight at pair {p}, past bound {bound}"
                );
            }
        }
    }

    /// Same for the 32-byte matrix.
    #[test]
    fn matrix_ranges_match_table_32() {
        for (v, &bound) in PAIRS_FOR_V.iter().enumerate() {
            for (p, row) in PAIRS.iter().enumerate() {
                let nonzero = row[v].iter().any(|&x| x != 0);
                assert!(
                    !nonzero || p < bound,
                    "digit vector {v} has a non-zero weight at pair {p}, past bound {bound}"
                );
            }
        }
    }
}
