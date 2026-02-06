//! # Base64 Turbo
//!
//! [![Crates.io](https://img.shields.io/crates/v/base64-turbo.svg)](https://crates.io/crates/base64-turbo)
//! [![Documentation](https://docs.rs/base64-turbo/badge.svg)](https://docs.rs/base64-turbo)
//! [![License](https://img.shields.io/github/license/hacer-bark/base64-turbo)](https://github.com/hacer-bark/base64-turbo/blob/main/LICENSE)
//! [![Kani Verified](https://img.shields.io/github/actions/workflow/status/hacer-bark/base64-turbo/verification.yml?label=Kani%20Verified)](https://github.com/hacer-bark/base64-turbo/actions/workflows/verification.yml)
//! [![MIRI Verified](https://img.shields.io/github/actions/workflow/status/hacer-bark/base64-turbo/miri.yml?label=MIRI%20Verified)](https://github.com/hacer-bark/base64-turbo/actions/workflows/miri.yml)
//! [![Logic Tests](https://img.shields.io/github/actions/workflow/status/hacer-bark/base64-turbo/tests.yml?label=Logic%20Tests)](https://github.com/hacer-bark/base64-turbo/actions/workflows/tests.yml)
//!
//! A SIMD-accelerated Base64 encoder/decoder for Rust, optimized for high-throughput systems.
//!
//! This crate provides runtime CPU detection to utilize AVX2, SSE4.1, or AVX512 (via feature flag) intrinsics.
//! It includes a highly optimized scalar fallback for non-SIMD targets and supports `no_std` environments.
//!
//! ## Usage
//!
//! Add this to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! base64-turbo = "0.1"
//! ```
//!
//! ### Basic API (Allocating)
//!
//! Standard usage for general applications. Requires the `std` feature (enabled by default).
//!
//! ```rust
//! # #[cfg(feature = "std")]
//! # {
//! use base64_turbo::STANDARD;
//!
//! let data = b"Hello world";
//!
//! // Encode to String
//! let encoded = STANDARD.encode(data);
//! assert_eq!(encoded, "SGVsbG8gd29ybGQ=");
//!
//! // Decode to Vec<u8>
//! let decoded = STANDARD.decode(&encoded).unwrap();
//! assert_eq!(decoded, data);
//! # }
//! ```
//!
//! ### Zero-Allocation API (Slice-based)
//!
//! For low-latency scenarios or `no_std` environments where heap allocation is undesirable.
//! These methods write directly into a user-provided mutable slice.
//!
//! ```rust
//! use base64_turbo::STANDARD;
//!
//! let input = b"Raw bytes";
//! let mut output = [0u8; 64]; // Pre-allocated stack buffer
//!
//! // Returns Result<usize, Error> indicating bytes written
//! let len = STANDARD.encode_into(input, &mut output).unwrap();
//!
//! assert_eq!(&output[..len], b"UmF3IGJ5dGVz");
//! ```
//!
//! ## Feature Flags
//!
//! This crate is highly configurable via Cargo features:
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | **`std`** | **Yes** | Enables `String` and `Vec` support. Disable this for `no_std` environments. |
//! | **`simd`** | **Yes** | Enables runtime detection for AVX2 and SSE4.1 intrinsics. If disabled or unsupported by hardware, the crate falls back to scalar logic automatic. |
//! | **`parallel`** | **No** | Enables [Rayon](https://crates.io/crates/rayon) support. Automatically parallelizes processing for payloads larger than 512KB. Recommended only for massive data ingestion tasks. |
//! | **`avx512`** | **No** | Enables AVX512 intrinsics. |
//! | **`unstable`** | **No** | Enables access to the raw, unsafe functions. |
//!
//! ## Safety & Verification
//!
//! This crate utilizes `unsafe` code for SIMD intrinsics and pointer arithmetic to achieve maximum performance.
//!
//! *   **Formal Verification (Kani):** Scalar (Done), SSE4.1 (In Progress), AVX2 (Done), AVX512 (In Progress) code mathematic proven to be UB free and panic free.
//! *   **MIRI Tests:** Core SIMD logic and scalar fallbacks are verified with **MIRI** (Undefined Behavior checker) in CI.
//! *   **Fuzzing:** The codebase is fuzz-tested via `cargo-fuzz`.
//! *   **Fallback:** Invalid or unsupported hardware instruction sets are detected at runtime, ensuring safe fallback to scalar code.
//! 
//! **[Learn More](https://github.com/hacer-bark/base64-turbo/blob/main/docs/verification.md)**: Details on our threat model and formal verification strategy.

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![doc(issue_tracker_base_url = "https://github.com/hacer-bark/base58-turbo/issues/")]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(unused_qualifications)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod encode;
mod decode;
use encode::encode_slice_unsafe;
use decode::decode_slice_unsafe;

// ======================================================================
// Errors
// ======================================================================

/// Errors that can occur during Base58 encoding or decoding operations or alphabet creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// An invalid character was encountered (not in the alphabet).
    InvalidCharacter,
    /// The output buffer is too small to hold the result.
    BufferTooSmall,
    /// The input data is too big to process. Limit is 512 bytes.
    InputTooBig,
    /// The input alphabet has duplicate chars.
    WrongAlphabet,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::InvalidCharacter => write!(f, "invalid character in base58 string"),
            Error::BufferTooSmall => write!(f, "output buffer too small"),
            Error::InputTooBig => write!(f, "input data too big (max 512 bytes)"),
            Error::WrongAlphabet => write!(f, "input alphabet has duplicate chars"),
        }
    }
}

// Enable std::error::Error trait when the 'std' feature is active
#[cfg(feature = "std")]
impl std::error::Error for Error {}

// ======================================================================
// Configuration & Types
// ======================================================================

/// Internal configuration containing pre-computed tables for an alphabet.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Alphabet of chars for encoding and decoding.
    pub alphabet: [u8; 58],
    /// Pre-computed map of values for decoding.
    pub decode_map: [u8; 256],
    /// Pre-computed LUT of squared values for encoding.
    pub lut_58_squared: [u16; 3364],
}

impl Config {
    /// Creates a new configuration from a 58-byte alphabet.
    /// Checks that all characters are unique.
    pub const fn new(alphabet: &[u8; 58]) -> Result<Self, Error> {
        // 1. Generate Decode Map & Check Uniqueness
        let mut map = [255u8; 256];
        let mut i = 0;

        while i < 58 {
            let byte = alphabet[i];

            // Uniqueness Check:
            // If the map position is not 255, it means we already saw this byte.
            if map[byte as usize] != 255 {
                return Err(Error::WrongAlphabet);
            }

            map[byte as usize] = i as u8;
            i += 1;
        }

        // 2. Return valid Config
        Ok(Self {
            alphabet: *alphabet,
            decode_map: map,
            lut_58_squared: gen_lut_squared(alphabet),
        })
    }
}

/// A Base58 Encoder/Decoder Engine.
#[derive(Debug, Clone, Copy)]
pub struct Engine {
    config: Config,
}

// ======================================================================
// Pre-defined Engines
// ======================================================================

/// Standard Bitcoin Base58 Engine.
pub const BITCOIN: Engine = match Engine::new(b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz") {
    Ok(e) => e,
    Err(_) => panic!("Invalid Bitcoin alphabet definition"),
};

/// Monero Base58 Engine.
pub const MONERO: Engine = match Engine::new(b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz") {
    Ok(e) => e,
    Err(_) => panic!("Invalid Monero alphabet definition"),
};

/// Ripple Base58 Engine.
pub const RIPPLE: Engine = match Engine::new(b"rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz") {
    Ok(e) => e,
    Err(_) => panic!("Invalid Ripple alphabet definition"),
};

/// Flickr Base58 Engine.
pub const FLICKR: Engine = match Engine::new(b"123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ") {
    Ok(e) => e,
    Err(_) => panic!("Invalid Flickr alphabet definition"),
};

// ======================================================================
// Const Table Generators
// ======================================================================

const fn gen_lut_squared(alphabet: &[u8; 58]) -> [u16; 3364] {
    let mut table = [0u16; 3364];
    let mut i = 0;
    while i < 3364 {
        let c1 = alphabet[i / 58];
        let c2 = alphabet[i % 58];
        // Store as Big Endian u16 for direct memory write
        table[i] = ((c1 as u16) << 8) | (c2 as u16);
        i += 1;
    }
    table
}

// ======================================================================
// Engine Implementation
// ======================================================================

impl Engine {
    /// Constructs a new Engine with a custom alphabet.
    /// Returns Error::WrongAlphabet if the alphabet contains duplicates.
    pub const fn new(alphabet: &[u8; 58]) -> Result<Self, Error> {
        match Config::new(alphabet) {
            Ok(c) => Ok(Self { config: c }),
            Err(e) => Err(e),
        }
    }

    /// Returns the internal configuration.
    #[inline(always)]
    pub const fn config(&self) -> &Config {
        &self.config
    }

    // ======================================================================
    // Length Calculators
    // ======================================================================

    /// Returns the maximum possible length of the encoded data.
    /// Base58 expansion is ~137%. We add padding for safety.
    #[inline]
    #[must_use]
    pub const fn encoded_len(&self, input_len: usize) -> usize {
        (input_len.saturating_mul(137) / 100).saturating_add(1)
    }

    /// Returns the maximum possible length of the decoded data.
    /// Base58 '1's map 1:1 to bytes. We cannot assume compression.
    /// The worst-case decoded size is equal to the input string length.
    #[inline]
    #[must_use]
    pub const fn decoded_len(&self, input_len: usize) -> usize {
        input_len
    }

    // ======================================================================
    // Zero-Allocation APIs
    // ======================================================================

    /// Encodes `input` into the `output` buffer.
    /// Returns the actual number of bytes written.
    #[inline]
    pub fn encode_into<T: AsRef<[u8]>>(
        &self,
        input: T,
        output: &mut [u8],
    ) -> Result<usize, Error> {
        let input = input.as_ref();
        if input.is_empty() { return Ok(0); }
        if input.len() > 512 { return  Err(Error::InputTooBig); }

        let req_len = self.encoded_len(input.len());
        if output.len() < req_len { return Err(Error::BufferTooSmall); }

        // SAFETY: 
        // 1. We checked output has sufficient capacity above.
        // 2. We assume `encode_slice_unsafe` respects the pointer limits.
        // 3. We assume `encode_slice_unsafe` uses `self.config` for the alphabet.
        let actual_len = unsafe { 
            encode_slice_unsafe(
                input,
                output.as_mut_ptr(),
                &self.config,
            ) 
        };

        Ok(actual_len)
    }

    /// Decodes `input` into the `output` buffer.
    /// Returns the actual number of bytes written.
    #[inline]
    pub fn decode_into<T: AsRef<[u8]>>(
        &self,
        input: T,
        output: &mut [u8],
    ) -> Result<usize, Error> {
        let input = input.as_ref();
        if input.is_empty() { return Ok(0); }
        if input.len() > 512 { return  Err(Error::InputTooBig); }

        // While decoding implies shrinking, we must ensure buffer is enough for the worst case.
        // However, standard usage usually provides a buffer size == input size or calculated decoded_len.
        // The safest check is:
        let req_len = self.decoded_len(input.len());
        if output.len() < req_len { return Err(Error::BufferTooSmall); }

        // SAFETY:
        // 1. `decode_slice_unsafe` performs bounds checks internally or logic ensures it.
        // 2. We pass the slice `output` via mutable reference, guaranteeing validity.
        unsafe { 
            decode_slice_unsafe(
                input,
                output,
                &self.config,
            ) 
        }
    }

    // ========================================================================
    // Allocating APIs (std)
    // ========================================================================

    /// Encodes `input` into the newly allocated `String`.
    /// Returns the `String`.
    #[inline]
    #[cfg(feature = "std")]
    pub fn encode<T: AsRef<[u8]>>(&self, input: T) -> Result<String, Error> {
        let input = input.as_ref();
        if input.is_empty() { return Ok(String::new()); }
        if input.len() > 512 { return  Err(Error::InputTooBig); }

        let max_len = self.encoded_len(input.len());
        let mut out = Vec::with_capacity(max_len);

        // SAFETY: 
        // We set the length to `max_len` to allow the unsafe kernel to write into the uninitialized capacity.
        // We MUST successfully overwrite or truncate this before returning.
        #[allow(clippy::uninit_vec)]
        unsafe { out.set_len(max_len); }

        match self.encode_into(input, &mut out) {
            Ok(actual_len) => {
                // SAFETY: The kernel reported `actual_len` bytes were written.
                // Truncate the vector to remove the remaining uninitialized tail.
                unsafe { out.set_len(actual_len); }

                // SAFETY: Base58 is always valid ASCII, which is valid UTF-8.
                unsafe { Ok(String::from_utf8_unchecked(out)) }
            }
            Err(_) => {
                // This branch should technically be unreachable if `encoded_len` is correct
                // and `Vec::with_capacity` succeeded.
                // Prevent returning uninitialized memory if logic fails.
                unsafe { out.set_len(0); }
                panic!("Base58 encoding failed due to insufficient buffer (logic error).");
            }
        }
    }

    /// Decodes `input` into the newly allocated `Vec<u8>`.
    /// Returns the `Vec<u8>`.
    #[inline]
    #[cfg(feature = "std")]
    pub fn decode<T: AsRef<[u8]>>(&self, input: T) -> Result<Vec<u8>, Error> {
        let input = input.as_ref();
        if input.is_empty() { return Ok(Vec::new()); }
        if input.len() > 512 { return  Err(Error::InputTooBig); }

        let max_len = self.decoded_len(input.len());
        let mut out = Vec::with_capacity(max_len);

        // SAFETY: Expose uninitialized buffer to the decoder.
        #[allow(clippy::uninit_vec)]
        unsafe { out.set_len(max_len); }

        match self.decode_into(input, &mut out) {
            Ok(actual_len) => {
                // SAFETY: Success. Truncate to actual size.
                unsafe { out.set_len(actual_len); }
                Ok(out)
            }
            Err(e) => {
                // SAFETY: Failure. Clear length to prevent access to junk data.
                unsafe { out.set_len(0); }
                Err(e)
            }
        }
    }
}

#[cfg(kani)]
mod kani_verification_scalar {
    use super::*;
    use crate::{Config, STANDARD as TURBO_STANDARD, STANDARD_NO_PAD as TURBO_STANDARD_NO_PAD};

    // Magic number
    // It handles 2 loops unroll + tail.
    const INPUT_LEN: usize = 17;

    fn encoded_size(len: usize, padding: bool) -> usize {
        if padding { TURBO_STANDARD.encoded_len(len) } else { TURBO_STANDARD_NO_PAD.encoded_len(len) }
    }

    #[kani::proof]
    #[kani::unwind(18)]
    fn check_roundtrip_safety() {
        // Symbolic Config
        let config = Config {
            url_safe: kani::any(),
            padding: kani::any(),
        };

        // Symbolic Input
        let input: [u8; INPUT_LEN] = kani::any();

        // Setup Buffers
        let enc_len = encoded_size(INPUT_LEN, config.padding);
        let mut enc_buf = [0u8; 64];
        let mut dec_buf = [0u8; 64];

        unsafe {
            // Encode
            encode_slice_unsafe(&config, &input, enc_buf.as_mut_ptr());

            // Decode
            let src_slice = &enc_buf[..enc_len];
            let written = decode_slice_unsafe(&config, src_slice, dec_buf.as_mut_ptr()).expect("Decoder failed");

            // Verification
            assert_eq!(&dec_buf[..written], &input, "AVX2 Roundtrip Failed");
        }
    }

    #[kani::proof]
    #[kani::unwind(18)]
    fn check_decoder_robustness() {
        // Symbolic Config
        let config = Config {
            url_safe: kani::any(),
            padding: kani::any(),
        };

        // Symbolic Input (Random Garbage)
        let input: [u8; INPUT_LEN] = kani::any();

        // Setup Buffer
        let mut dec_buf = [0u8; 64];

        unsafe {
            // We verify what function NEVER panics/crashes
            let _ = decode_slice_unsafe(&config, &input, dec_buf.as_mut_ptr());
        }
    }
}
