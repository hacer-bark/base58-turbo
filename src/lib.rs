//! # Base58 Turbo
//!
//! [![Crates.io](https://img.shields.io/crates/v/base58-turbo.svg)](https://crates.io/crates/base58-turbo)
//! [![Documentation](https://docs.rs/base58-turbo/badge.svg)](https://docs.rs/base58-turbo)
//! [![License](https://img.shields.io/github/license/hacer-bark/base58-turbo)](https://github.com/hacer-bark/base58-turbo/blob/main/LICENSE)
//! [![MIRI Verified](https://img.shields.io/github/actions/workflow/status/hacer-bark/base58-turbo/miri.yml?label=MIRI%20Verified)](https://github.com/hacer-bark/base58-turbo/actions/workflows/miri.yml)
//! [![Logic Tests](https://img.shields.io/github/actions/workflow/status/hacer-bark/base58-turbo/tests.yml?label=Logic%20Tests)](https://github.com/hacer-bark/base58-turbo/actions/workflows/tests.yml)
//!
//! A high-performance Base58 encoder/decoder for Rust, optimized for high-throughput systems.
//!
//! This crate provides highly optimized scalar kernels for encoding and decoding,
//! supporting `no_std` environments and zero-allocation processing.
//!
//! ## Usage
//!
//! Add this to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! base58-turbo = "0.1"
//! ```
//!
//! ### Basic API (Allocating)
//!
//! Standard usage for general applications. Requires the `std` feature (enabled by default).
//!
//! ```rust
//! use base58_turbo::BITCOIN;
//!
//! let data = b"Hello World";
//! let encoded = BITCOIN.encode(data).unwrap();
//! assert_eq!(encoded, "JxF12TrwUP45BMd");
//!
//! let decoded = BITCOIN.decode(&encoded).unwrap();
//! assert_eq!(decoded, data);
//! ```
//!
//! ### Zero-Allocation API (Slice-based)
//!
//! For low-latency scenarios or `no_std` environments where heap allocation is undesirable.
//! These methods write directly into a user-provided mutable slice.
//!
//! ```rust
//! use base58_turbo::BITCOIN;
//!
//! let data = b"Hello World";
//! let mut output = [0u8; 32];
//!
//! let len = BITCOIN.encode_into(data, &mut output).unwrap();
//! let encoded = std::str::from_utf8(&output[..len]).unwrap();
//! assert_eq!(encoded, "JxF12TrwUP45BMd");
//! ```
//!
//! ## Feature Flags
//!
//! This crate is lightweight and configurable via Cargo features:
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | **`serde`** | **No** | Enables `serde` serialization/deserialization for Config and Engine. |
//! | **`std`** | **Yes** | Enables `String` and `Vec` support. Disable this for `no_std` environments. |
//!
//! ## Safety & Verification
//!
//! This crate utilizes `unsafe` code for pointer arithmetic and optimized kernels to achieve maximum performance.
//!
//! *   **MIRI Tests:** Core logic and fallbacks are verified with **MIRI** (Undefined Behavior checker) in CI.
//! *   **MSan Audited:** MemorySanitizer confirms no logic is ever performed on uninitialized memory.
//! *   **Fuzzing:** The codebase is continuously fuzz-tested via `cargo-fuzz`.
//!
//! **[Learn More](https://github.com/hacer-bark/base58-turbo/blob/main/docs/verification.md)**: Details on our threat model and strict verification strategy.

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![doc(issue_tracker_base_url = "https://github.com/hacer-bark/base58-turbo/issues/")]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_debug_implementations, missing_docs, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

// Use `serde` when enabled
#[cfg(feature = "serde")]
pub mod serde;

pub mod xmr;

mod decode;
mod encode;
use decode::decode_slice_unsafe;
use encode::encode_slice_unsafe;

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
    /// The input data is too big to process. Limit is 1024 bytes (encode) or 2048 bytes (decode).
    InputTooBig,
    /// The input alphabet has duplicate chars.
    WrongAlphabet,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::InvalidCharacter => write!(f, "invalid character in base58 string"),
            Error::BufferTooSmall => write!(f, "output buffer too small"),
            Error::InputTooBig => write!(f, "input data too big"),
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

// 2. Add manual Serde implementations underneath
#[cfg(feature = "serde")]
impl ::serde::Serialize for Config {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        // The alphabet is guaranteed valid ASCII/UTF-8 by Config::new checks.
        // Serializing it as a string makes it clean in JSON/TOML.
        let alpha_str =
            core::str::from_utf8(&self.alphabet).map_err(::serde::ser::Error::custom)?;
        serializer.serialize_str(alpha_str)
    }
}

#[cfg(feature = "serde")]
impl<'de> ::serde::Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        struct AlphabetVisitor;

        impl<'de> ::serde::de::Visitor<'de> for AlphabetVisitor {
            type Value = Config;

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("a 58-character Base58 alphabet string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: ::serde::de::Error,
            {
                let bytes = v.as_bytes();
                if bytes.len() != 58 {
                    return Err(E::custom("expected exactly 58-byte alphabet"));
                }

                let mut alpha = [0u8; 58];
                alpha.copy_from_slice(bytes);

                // Re-calculate the LUTs and Maps automatically
                Config::new(&alpha).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(AlphabetVisitor)
    }
}

#[cfg(feature = "serde")]
impl ::serde::Serialize for Engine {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        self.config.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> ::serde::Deserialize<'de> for Engine {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        Config::deserialize(deserializer).map(|config| Engine { config })
    }
}

// ======================================================================
// Pre-defined Engines
// ======================================================================

/// Standard Bitcoin Base58 Engine.
pub const BITCOIN: Engine =
    match Engine::new(b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz") {
        Ok(e) => e,
        Err(_) => panic!("Invalid Bitcoin alphabet definition"),
    };

/// Monero Base58 Engine.
pub const MONERO: Engine =
    match Engine::new(b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz") {
        Ok(e) => e,
        Err(_) => panic!("Invalid Monero alphabet definition"),
    };

/// Ripple Base58 Engine.
pub const RIPPLE: Engine =
    match Engine::new(b"rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz") {
        Ok(e) => e,
        Err(_) => panic!("Invalid Ripple alphabet definition"),
    };

/// Flickr Base58 Engine.
pub const FLICKR: Engine =
    match Engine::new(b"123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ") {
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
    pub fn encode_into<T: AsRef<[u8]>>(&self, input: T, output: &mut [u8]) -> Result<usize, Error> {
        let input = input.as_ref();
        if input.is_empty() {
            return Ok(0);
        }
        if input.len() > 1024 {
            return Err(Error::InputTooBig);
        }

        let req_len = self.encoded_len(input.len());
        if output.len() < req_len {
            return Err(Error::BufferTooSmall);
        }

        // SAFETY:
        // 1. We checked output has sufficient capacity above.
        // 2. We assume `encode_slice_unsafe` respects the pointer limits.
        // 3. We assume `encode_slice_unsafe` uses `self.config` for the alphabet.
        let actual_len = unsafe { encode_slice_unsafe(input, output.as_mut_ptr(), &self.config) };

        Ok(actual_len)
    }

    /// Decodes `input` into the `output` buffer.
    /// Returns the actual number of bytes written.
    #[inline]
    pub fn decode_into<T: AsRef<[u8]>>(&self, input: T, output: &mut [u8]) -> Result<usize, Error> {
        let input = input.as_ref();
        if input.is_empty() {
            return Ok(0);
        }
        if input.len() > 2048 {
            return Err(Error::InputTooBig);
        }

        // While decoding implies shrinking, we must ensure buffer is enough for the worst case.
        // However, standard usage usually provides a buffer size == input size or calculated decoded_len.
        // The safest check is:
        let req_len = self.decoded_len(input.len());
        if output.len() < req_len {
            return Err(Error::BufferTooSmall);
        }

        // SAFETY:
        // 1. `decode_slice_unsafe` performs bounds checks internally or logic ensures it.
        // 2. We pass the slice `output` via mutable reference, guaranteeing validity.
        unsafe { decode_slice_unsafe(input, output, &self.config) }
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
        if input.is_empty() {
            return Ok(String::new());
        }
        if input.len() > 1024 {
            return Err(Error::InputTooBig);
        }

        let max_len = self.encoded_len(input.len());
        let mut out = Vec::with_capacity(max_len);

        // SAFETY:
        // We set the length to `max_len` to allow the unsafe kernel to write into the uninitialized capacity.
        // We MUST successfully overwrite or truncate this before returning.
        #[allow(clippy::uninit_vec)]
        unsafe {
            out.set_len(max_len);
        }

        match self.encode_into(input, &mut out) {
            Ok(actual_len) => {
                // SAFETY: The kernel reported `actual_len` bytes were written.
                // Truncate the vector to remove the remaining uninitialized tail.
                unsafe {
                    out.set_len(actual_len);
                }

                // SAFETY: Base58 is always valid ASCII, which is valid UTF-8.
                unsafe { Ok(String::from_utf8_unchecked(out)) }
            }
            Err(_) => {
                // This branch should technically be unreachable if `encoded_len` is correct
                // and `Vec::with_capacity` succeeded.
                // Prevent returning uninitialized memory if logic fails.
                unsafe {
                    out.set_len(0);
                }
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
        if input.is_empty() {
            return Ok(Vec::new());
        }
        if input.len() > 2048 {
            return Err(Error::InputTooBig);
        }

        let max_len = self.decoded_len(input.len());
        let mut out = Vec::with_capacity(max_len);

        // SAFETY: Expose uninitialized buffer to the decoder.
        #[allow(clippy::uninit_vec)]
        unsafe {
            out.set_len(max_len);
        }

        match self.decode_into(input, &mut out) {
            Ok(actual_len) => {
                // SAFETY: Success. Truncate to actual size.
                unsafe {
                    out.set_len(actual_len);
                }
                Ok(out)
            }
            Err(e) => {
                // SAFETY: Failure. Clear length to prevent access to junk data.
                unsafe {
                    out.set_len(0);
                }
                Err(e)
            }
        }
    }
}

#[cfg(all(test, miri))]
mod lib_miri_coverage {
    use super::*;

    #[test]
    fn miri_engine_lifecycle() {
        let alphabet = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
        let engine = Engine::new(alphabet).unwrap();

        let data = b"Miri Test Data";
        let encoded = engine.encode(data).unwrap();
        let decoded = engine.decode(&encoded).unwrap();

        assert_eq!(data, decoded.as_slice());
    }

    #[test]
    fn miri_all_predefined_engines() {
        let engines = [BITCOIN, MONERO, RIPPLE, FLICKR];
        let data = b"test";
        for engine in engines {
            let encoded = engine.encode(data).unwrap();
            let decoded = engine.decode(&encoded).unwrap();
            assert_eq!(data, decoded.as_slice());
        }
    }

    #[test]
    fn miri_config_errors() {
        let alphabet = [b'a'; 58];
        // Duplicate chars should fail
        assert!(Config::new(&alphabet).is_err());
    }
}
