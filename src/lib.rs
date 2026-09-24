//! # Base58 Turbo
//!
//! [![Crates.io](https://img.shields.io/crates/v/base58-turbo.svg)](https://crates.io/crates/base58-turbo)
//! [![Documentation](https://docs.rs/base58-turbo/badge.svg)](https://docs.rs/base58-turbo)
//! [![License](https://img.shields.io/github/license/hacer-bark/base58-turbo)](https://github.com/hacer-bark/base58-turbo/blob/main/LICENSE)
//! [![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)
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
//! | **`std`** | **Yes** | Enables `String` and `Vec` support. Disable this for `no_std` environments. |
//!
//! ## Safety & Verification
//!
//! The crate is `#![forbid(unsafe_code)]` unconditionally: no feature flag,
//! target, or configuration reintroduces `unsafe`. Performance comes entirely
//! from the base conversion algorithm and from shaping the hot loops so the
//! compiler can drop bounds checks on its own, not from bypassing them.
//!
//! *   **Tests:** exact conformance vectors, every kernel-dispatch and scratch-buffer
//!     boundary, and randomized cross-validation against `bs58`, `base58`, `five8`,
//!     and `base58-monero` (see `tests/`).
//! *   **Fuzzing:** `fuzz/fuzz_targets/fuzz_all_modes.rs` exercises encode/decode
//!     round-trips via `cargo fuzz`.

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![doc(issue_tracker_base_url = "https://github.com/hacer-bark/base58-turbo/issues/")]
#![forbid(elided_lifetimes_in_paths, unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

#[cfg(all(doctest, feature = "std"))]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

pub mod xmr;

pub mod decode;
pub mod encode;

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
    /// The input is too big for the zero-allocation `_into` API: over 1024 bytes
    /// to encode, or over 2048 characters or 1024 decoded bytes to decode. The
    /// allocating APIs only return it when a buffer would exceed `isize::MAX`
    /// bytes, which is reachable on 32-bit targets alone.
    InputTooBig,
    /// The alphabet has a duplicate or non-ASCII character.
    WrongAlphabet,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidCharacter => write!(f, "invalid character in base58 string"),
            Self::BufferTooSmall => write!(f, "output buffer too small"),
            Self::InputTooBig => write!(f, "input data too big"),
            Self::WrongAlphabet => write!(f, "alphabet has a duplicate or non-ASCII char"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

// ======================================================================
// Configuration & Types
// ======================================================================

/// Pre-computed lookup tables for one alphabet.
///
/// Only [`Config::new`] builds one, so the kernels can rely on the tables being
/// consistent with the alphabet.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    alphabet: [u8; 58],
    decode_map: [u8; 256],
    /// Two-character pairs for encoding: entry `i` is `alphabet[i / 58]`,
    /// `alphabet[i % 58]`, big-endian.
    lut_58_squared: [u16; 3364],
    /// Decode weights: `lut_58_pow[k][b]` is `digit(b) * 58^k`, or [`BAD_DIGIT`]
    /// when `b` is outside the alphabet.
    lut_58_pow: [[u32; 256]; 4],
}

impl Config {
    /// Creates a new configuration from a 58-byte alphabet.
    ///
    /// # Errors
    ///
    /// Returns [`Error::WrongAlphabet`] if the alphabet contains a non-ASCII byte
    /// or a duplicate character.
    pub const fn new(alphabet: &[u8; 58]) -> Result<Self, Error> {
        let mut map = [255u8; 256];
        let mut i: u8 = 0;

        while (i as usize) < 58 {
            let byte = alphabet[i as usize];

            // ASCII-only keeps `encode`'s `String` conversion infallible.
            if byte >= 0x80 || map[byte as usize] != 255 {
                return Err(Error::WrongAlphabet);
            }

            map[byte as usize] = i;
            i += 1;
        }

        Ok(Self {
            alphabet: *alphabet,
            decode_map: map,
            lut_58_squared: gen_lut_squared(alphabet),
            lut_58_pow: gen_lut_pow(&map),
        })
    }

    /// Returns the 58-character alphabet, indexed by digit value.
    #[inline]
    #[must_use]
    pub const fn alphabet(&self) -> &[u8; 58] {
        &self.alphabet
    }

    /// Returns the byte-to-digit map: `decode_map()[b]` is the digit value of
    /// byte `b`, or 255 when `b` is outside the alphabet.
    #[inline]
    #[must_use]
    pub const fn decode_map(&self) -> &[u8; 256] {
        &self.decode_map
    }
}

/// Sentinel stored in `Config::lut_58_pow` for a byte outside the alphabet.
///
/// A valid entry is at most `57 * 58^3 = 11_121_384`, so the sum of any four is
/// below `2^26`, while a sum containing at least one sentinel is at least `2^28`
/// and at most `2^30`. One test of the bits from 26 up therefore validates a
/// whole group of four characters, with no per-character branch.
pub(crate) const BAD_DIGIT: u32 = 1 << 28;

/// Builds the decode weight table: `[k][b] = digit(b) * 58^k` for `k` in 0..4.
const fn gen_lut_pow(map: &[u8; 256]) -> [[u32; 256]; 4] {
    let mut table = [[BAD_DIGIT; 256]; 4];
    let mut k = 0;
    while k < 4 {
        let mut pow: u32 = 1;
        let mut e = 0;
        while e < k {
            pow *= 58;
            e += 1;
        }
        let mut b = 0;
        while b < 256 {
            let digit = map[b];
            table[k][b] = if digit & 0x80 != 0 {
                BAD_DIGIT
            } else {
                digit as u32 * pow
            };
            b += 1;
        }
        k += 1;
    }
    table
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
        // Big-endian, so `to_be_bytes` yields the two chars in order.
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
    /// Returns [`Error::WrongAlphabet`] if the alphabet contains a non-ASCII byte
    /// or a duplicate character.
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

    /// Encodes a batch of 32-byte inputs.
    ///
    /// Each output record is a fixed 44 bytes, the longest a 32-byte input can
    /// encode to. `lens[i]` gives the meaningful length of `out[i]`, so the text
    /// for input `i` is `&out[i][..lens[i] as usize]`.
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
    pub fn encode_32_batch(
        &self,
        inputs: &[[u8; 32]],
        out: &mut [[u8; 44]],
        lens: &mut [u8],
    ) -> Result<(), Error> {
        if out.len() < inputs.len() || lens.len() < inputs.len() {
            return Err(Error::BufferTooSmall);
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

    /// Returns an upper bound on the encoded length.
    ///
    /// Base58 needs `log(256) / log(58) ≈ 1.366` characters per byte; this
    /// rounds that up to 1.37 and adds one.
    #[inline]
    #[must_use]
    pub const fn encoded_len(&self, input_len: usize) -> usize {
        encode::encoded_len(input_len)
    }

    /// Returns an upper bound on the decoded length: the input length, since each
    /// leading zero character decodes to one zero byte.
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
    /// `output` must hold at least [`Engine::encoded_len`] of the input, even when
    /// the result is shorter.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InputTooBig`] if `input` exceeds 1024 bytes, or
    /// [`Error::BufferTooSmall`] if `output` is shorter than
    /// [`Engine::encoded_len`] of the input.
    #[inline]
    pub fn encode_into<T: AsRef<[u8]>>(&self, input: T, output: &mut [u8]) -> Result<usize, Error> {
        encode_slice(input.as_ref(), output, &self.config)
    }

    /// Decodes `input` into the `output` buffer.
    /// Returns the actual number of bytes written.
    ///
    /// `output` must hold at least [`Engine::decoded_len`] of the input, even when
    /// the result is shorter. Bytes of `output` past the returned length are
    /// unspecified.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InputTooBig`] if `input` exceeds 2048 bytes or decodes to
    /// more than 1024 bytes, [`Error::BufferTooSmall`] if `output` is shorter than
    /// `input`, or [`Error::InvalidCharacter`] if `input` contains a character
    /// outside the alphabet.
    #[inline]
    pub fn decode_into<T: AsRef<[u8]>>(&self, input: T, output: &mut [u8]) -> Result<usize, Error> {
        let input = input.as_ref();
        if input.is_empty() {
            return Ok(0);
        }
        if input.len() > 2048 {
            return Err(Error::InputTooBig);
        }

        let req_len = self.decoded_len(input.len());
        if output.len() < req_len {
            return Err(Error::BufferTooSmall);
        }

        decode_slice(input, output, &self.config)
    }

    // ========================================================================
    // Allocating APIs (std)
    // ========================================================================

    /// Encodes `input` into a newly allocated `String`.
    ///
    /// Unlike [`Engine::encode_into`], there is no input size limit: inputs that
    /// exceed the zero-allocation kernel's stack-scratch ceiling fall back to heap
    /// scratch sized to `input`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InputTooBig`] only if the output or scratch would exceed
    /// `isize::MAX` bytes, which needs an input of about 1 GB on a 32-bit target.
    #[inline]
    #[cfg(feature = "std")]
    pub fn encode<T: AsRef<[u8]>>(&self, input: T) -> Result<String, Error> {
        let input = input.as_ref();
        if input.is_empty() {
            return Ok(String::new());
        }

        let max_len = self.encoded_len(input.len());
        if max_len > isize::MAX as usize {
            return Err(Error::InputTooBig);
        }
        let mut out = vec![0u8; max_len];

        let actual_len = match encode_slice(input, &mut out, &self.config) {
            Err(Error::InputTooBig) => encode_slice_unbounded(input, &mut out, &self.config)?,
            result => result?,
        };
        out.truncate(actual_len);

        // Always ASCII: `Config::new` rejects non-ASCII alphabets.
        String::from_utf8(out).map_err(|_| Error::WrongAlphabet)
    }

    /// Decodes `input` into a newly allocated `Vec<u8>`.
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
            Err(Error::InputTooBig) => decode_slice_unbounded(input, &mut out, &self.config)?,
            result => result?,
        };
        out.truncate(actual_len);
        Ok(out)
    }
}
