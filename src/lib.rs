//! # Base58 Turbo
//!
//! [![Crates.io](https://img.shields.io/crates/v/base58-turbo.svg)](https://crates.io/crates/base58-turbo)
//! [![Documentation](https://docs.rs/base58-turbo/badge.svg)](https://docs.rs/base58-turbo)
//! [![License](https://img.shields.io/github/license/hacer-bark/base58-turbo)](https://github.com/hacer-bark/base58-turbo/blob/main/LICENSE-APACHE)
//!
//! *(The default build enables `unsafe-simd`, which adds one `unsafe` AVX2
//! module on x86/x86-64. Disable it — or build for a non-x86 target — for
//! `#![forbid(unsafe_code)]`.)*
//!
//! A high-performance Base58 encoder/decoder for Rust, optimized for high-throughput systems.
//!
//! This crate provides highly optimized scalar kernels for encoding and decoding,
//! supporting `no_std` environments and zero-allocation processing.
//!
//! ## Usage
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
//! | **`unsafe-simd`** | **Yes** | AVX2 encoding and decoding kernels on x86/x86-64, selected at runtime. Adds the crate's only `unsafe` code; no effect on other targets. |
//!
//! ## Safety & Verification
//!
//! `unsafe-simd` is on by default, and on x86/x86-64 it enables one `unsafe`
//! module, `src/simd.rs`, which holds the AVX2 kernels — the only `unsafe`
//! in the crate. The kernels are reached only after a runtime AVX2 check, so
//! a binary built with the feature still runs correctly on hardware without
//! AVX2, and every non-x86 target keeps the scalar path regardless of the
//! feature.
//!
//! Build with `default-features = false` (re-enabling `std` as needed) — or
//! target a non-x86 platform — and `#![forbid(unsafe_code)]` applies: the
//! compiler rejects any `unsafe` block anywhere in the crate. Performance in
//! that configuration comes entirely from the base conversion algorithm and
//! from shaping the hot loops so the compiler can drop bounds checks on its
//! own, not from bypassing them.
//!
//! *   **Tests:** exact conformance vectors, every kernel-dispatch and scratch-buffer
//!     boundary, and randomized cross-validation against `bs58`, `base58`, `five8`,
//!     and `base58-monero` (see `tests/`).
//! *   **SIMD parity:** with `unsafe-simd` on, the AVX2 kernels are checked
//!     against an independent schoolbook implementation for every leading-zero
//!     run length and across several alphabets, and the batch entry point is
//!     checked against the single-input one.
//! *   **Fuzzing:** `fuzz/fuzz_targets/fuzz_all_modes.rs` exercises encode/decode
//!     round-trips via `cargo fuzz`.

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![doc(issue_tracker_base_url = "https://github.com/hacer-bark/base58-turbo/issues/")]
#![cfg_attr(
    not(all(
        feature = "unsafe-simd",
        any(target_arch = "x86_64", target_arch = "x86")
    )),
    forbid(unsafe_code)
)]
#![forbid(elided_lifetimes_in_paths)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

#[cfg(all(doctest, feature = "std"))]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

// Use `serde` when enabled
#[cfg(feature = "serde")]
pub mod serde;

pub mod xmr;

pub mod decode;
pub mod encode;

#[cfg(all(
    feature = "unsafe-simd",
    any(target_arch = "x86_64", target_arch = "x86")
))]
mod simd;
use decode::decode_slice;
#[cfg(feature = "std")]
use decode::decode_slice_unbounded;
use encode::encode_slice;
#[cfg(feature = "std")]
use encode::encode_slice_unbounded;

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
    /// The input data is too big for the zero-allocation `_into` API. Limit is
    /// 1024 bytes (encode) or 2048 bytes (decode); the allocating [`Engine::encode`]
    /// / [`Engine::decode`] have no such limit.
    InputTooBig,
    /// The input alphabet has duplicate chars.
    WrongAlphabet,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidCharacter => write!(f, "invalid character in base58 string"),
            Self::BufferTooSmall => write!(f, "output buffer too small"),
            Self::InputTooBig => write!(f, "input data too big"),
            Self::WrongAlphabet => write!(f, "input alphabet has duplicate chars"),
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
    ///
    /// # Errors
    ///
    /// Returns [`Error::WrongAlphabet`] if the alphabet contains a non-ASCII byte
    /// or a duplicate character.
    pub const fn new(alphabet: &[u8; 58]) -> Result<Self, Error> {
        // 1. Generate Decode Map & Check Uniqueness
        let mut map = [255u8; 256];
        let mut i: u8 = 0;

        while (i as usize) < 58 {
            let byte = alphabet[i as usize];

            // ASCII Check:
            // `encode` returns a `String`, so the alphabet must be valid UTF-8 on
            // its own. Rejecting non-ASCII here is what lets that conversion be
            // infallible.
            if byte >= 0x80 {
                return Err(Error::WrongAlphabet);
            }

            // Uniqueness Check:
            // If the map position is not 255, it means we already saw this byte.
            if map[byte as usize] != 255 {
                return Err(Error::WrongAlphabet);
            }

            map[byte as usize] = i;
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

        impl ::serde::de::Visitor<'_> for AlphabetVisitor {
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
        Config::deserialize(deserializer).map(|config| Self { config })
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
    ///
    /// # Errors
    ///
    /// Returns [`Error::WrongAlphabet`] if the alphabet contains duplicates.
    pub const fn new(alphabet: &[u8; 58]) -> Result<Self, Error> {
        match Config::new(alphabet) {
            Ok(c) => Ok(Self { config: c }),
            Err(e) => Err(e),
        }
    }

    /// Returns the internal configuration.
    #[inline]
    #[must_use]
    pub const fn config(&self) -> &Config {
        &self.config
    }

    /// Encodes a batch of 32-byte inputs, three encodes in flight at a time.
    ///
    /// Requires the `unsafe-simd` feature and AVX2 at runtime; without either it
    /// falls back to [`Engine::encode_into`] per input, so results are identical
    /// either way.
    ///
    /// Each output record is a fixed 44 bytes, the longest a 32-byte input can
    /// encode to. `lens[i]` gives the meaningful length of `out[i]`, so the text
    /// for input `i` is `&out[i][..lens[i] as usize]`.
    ///
    /// A single encode is limited by its own dependency chain rather than by
    /// throughput, so interleaving independent inputs is worth roughly 15% per
    /// input over encoding them one at a time.
    ///
    /// # Errors
    ///
    /// Returns [`Error::BufferTooSmall`] if `out` or `lens` is shorter than
    /// `inputs`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use base58_turbo::BITCOIN;
    ///
    /// let inputs = [[7u8; 32], [9u8; 32]];
    /// let mut out = [[0u8; 44]; 2];
    /// let mut lens = [0u8; 2];
    ///
    /// BITCOIN.encode_32_batch(&inputs, &mut out, &mut lens).unwrap();
    /// let first = std::str::from_utf8(&out[0][..lens[0] as usize]).unwrap();
    /// assert_eq!(first, BITCOIN.encode(&inputs[0]).unwrap());
    /// ```
    #[cfg(feature = "std")]
    pub fn encode_32_batch(
        &self,
        inputs: &[[u8; 32]],
        out: &mut [[u8; 44]],
        lens: &mut [u8],
    ) -> Result<(), Error> {
        if out.len() < inputs.len() || lens.len() < inputs.len() {
            return Err(Error::BufferTooSmall);
        }

        #[cfg(all(
            feature = "unsafe-simd",
            any(target_arch = "x86_64", target_arch = "x86")
        ))]
        {
            if crate::simd::avx2_available() {
                // SAFETY: AVX2 was just confirmed present, and the length checks
                // above guarantee `out` and `lens` cover every input.
                unsafe {
                    let tabs = crate::simd::AlphaTables::new(&self.config);
                    crate::simd::encode_32_x3(
                        inputs,
                        &tabs,
                        self.config.alphabet[0],
                        &mut out[..inputs.len()],
                        &mut lens[..inputs.len()],
                    );
                }
                return Ok(());
            }
        }

        for (i, input) in inputs.iter().enumerate() {
            // A 32-byte input never encodes to more than 44 characters.
            #[allow(clippy::cast_possible_truncation)]
            {
                lens[i] = self.encode_into(input, &mut out[i])? as u8;
            }
        }
        Ok(())
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
    ///
    /// # Errors
    ///
    /// Returns [`Error::InputTooBig`] if `input` exceeds 1024 bytes, or
    /// [`Error::BufferTooSmall`] if `output` is not large enough.
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

        encode_slice(input, output, &self.config)
    }

    /// Decodes `input` into the `output` buffer.
    /// Returns the actual number of bytes written.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InputTooBig`] if `input` exceeds 2048 bytes,
    /// [`Error::BufferTooSmall`] if `output` is not large enough, or
    /// [`Error::InvalidCharacter`] if `input` contains a character outside the alphabet.
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

        decode_slice(input, output, &self.config)
    }

    // ========================================================================
    // Allocating APIs (std)
    // ========================================================================

    /// Encodes `input` into the newly allocated `String`.
    /// Returns the `String`.
    ///
    /// Unlike [`Engine::encode_into`], there is no input size limit: inputs that
    /// exceed the zero-allocation kernel's stack-scratch ceiling fall back to heap
    /// scratch sized to `input`.
    ///
    /// # Errors
    ///
    /// This can only fail if the alphabet were internally inconsistent, which
    /// [`Config::new`] already rejects at construction; the `Result` is kept for
    /// forward-compatibility.
    #[inline]
    #[cfg(feature = "std")]
    pub fn encode<T: AsRef<[u8]>>(&self, input: T) -> Result<String, Error> {
        let input = input.as_ref();
        if input.is_empty() {
            return Ok(String::new());
        }

        let max_len = self.encoded_len(input.len());
        let mut out = vec![0u8; max_len];

        let actual_len = match self.encode_into(input, &mut out) {
            Ok(n) => n,
            Err(Error::InputTooBig) => encode_slice_unbounded(input, &mut out, &self.config),
            Err(e) => return Err(e),
        };
        out.truncate(actual_len);

        // `Config::new` rejects non-ASCII alphabets, so the output is ASCII and
        // this conversion always succeeds; the error path is unreachable.
        String::from_utf8(out).map_err(|_| Error::WrongAlphabet)
    }

    /// Decodes `input` into the newly allocated `Vec<u8>`.
    /// Returns the `Vec<u8>`.
    ///
    /// Unlike [`Engine::decode_into`], there is no input size limit: inputs that
    /// exceed the zero-allocation kernel's stack-scratch ceiling (either in encoded
    /// length or decoded length) fall back to heap scratch sized to `input`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidCharacter`] if `input` contains a character outside
    /// the alphabet.
    #[inline]
    #[cfg(feature = "std")]
    pub fn decode<T: AsRef<[u8]>>(&self, input: T) -> Result<Vec<u8>, Error> {
        let input = input.as_ref();
        if input.is_empty() {
            return Ok(Vec::new());
        }

        let max_len = self.decoded_len(input.len());
        let mut out = vec![0u8; max_len];

        let actual_len = match self.decode_into(input, &mut out) {
            Ok(n) => n,
            Err(Error::InputTooBig) => decode_slice_unbounded(input, &mut out, &self.config)?,
            Err(e) => return Err(e),
        };
        out.truncate(actual_len);
        Ok(out)
    }
}
