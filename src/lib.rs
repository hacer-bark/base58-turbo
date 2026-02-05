
#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![doc(issue_tracker_base_url = "https://github.com/hacer-bark/base58-turbo/issues/")]
// #![deny(unsafe_op_in_unsafe_fn)]
// #![warn(missing_docs)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// An invalid character was encountered (not in the alphabet).
    InvalidCharacter,
    /// The output buffer is too small to hold the result.
    BufferTooSmall,
    /// The input data is too big to process. Limit is 512 bytes.s
    InputTooBig,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::InvalidCharacter => write!(f, "invalid character in base58 string"),
            Error::BufferTooSmall => write!(f, "output buffer too small"),
            Error::InputTooBig => write!(f, "input data too big (max 512 bytes)"),
        }
    }
}

// ======================================================================
// Configuration & Types
// ======================================================================

/// Internal configuration containing pre-computed tables for an alphabet.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub alphabet: [u8; 58],
    pub decode_map: [u8; 256],
    pub lut_58_squared: [u16; 3364],
}

impl Config {
    /// Creates a new configuration from a 58-byte alphabet.
    /// This is a `const fn`, allowing compile-time creation of Engines.
    pub const fn new(alphabet: &[u8; 58]) -> Self {
        Self {
            alphabet: *alphabet,
            decode_map: gen_decode_map(alphabet),
            lut_58_squared: gen_lut_squared(alphabet),
        }
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
pub const BITCOIN: Engine = Engine::new(b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz");

/// Ripple Base58 Engine.
pub const RIPPLE: Engine = Engine::new(b"rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz");

/// Flickr Base58 Engine.
pub const FLICKR: Engine = Engine::new(b"123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ");

// ======================================================================
// Const Table Generators
// ======================================================================

const fn gen_decode_map(alphabet: &[u8; 58]) -> [u8; 256] {
    let mut map = [255u8; 256];
    let mut i = 0;
    while i < 58 {
        map[alphabet[i] as usize] = i as u8;
        i += 1;
    }
    map
}

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
    pub const fn new(alphabet: &[u8; 58]) -> Self {
        Self {
            config: Config::new(alphabet),
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

#[cfg(test)]
mod exhaustive_tests {
    // --- Imports ---
    use super::*;
    use rand::{Rng, rng};
    use bs58::{encode, Alphabet};

    fn random_bytes(len: usize) -> Vec<u8> {
        let mut rng = rng();
        (0..len).map(|_| rng.random()).collect()
    }

    #[test]
    fn test_data() {
        for i in 0..256 {
            let data = random_bytes(i);
            let encoded = encode(&data)
                .with_alphabet(Alphabet::BITCOIN)
                .into_string();

            let encoded_my = BITCOIN.encode(&data).unwrap();
            assert_eq!(encoded_my, encoded, "Failed at size {}", i);

            let decoded_my = BITCOIN.decode(encoded_my).unwrap();
            assert_eq!(decoded_my, data, "Failed at size {}", i);
        }
    }
}
