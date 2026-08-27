//! Monero-specific Base58 chunked encoding and decoding.
//!
//! Monero uses a custom chunked algorithm to ensure fixed-size addresses.
//! Data is broken up into 8-byte blocks, which are each encoded into 11
//! characters. The final block is padded to a specific size.

use crate::{Error, MONERO};

#[cfg(feature = "std")]
use std::string::String;
#[cfg(feature = "std")]
use std::vec::Vec;

const XMR_ENCODED_SIZES: [usize; 9] = [0, 2, 3, 5, 6, 7, 9, 10, 11];

/// Returns the maximum possible length of the encoded Monero data.
#[inline]
#[must_use]
pub const fn encoded_len(input_len: usize) -> usize {
    let full_blocks = input_len / 8;
    let remainder = input_len % 8;
    (full_blocks * 11) + XMR_ENCODED_SIZES[remainder]
}

/// Returns the exact length of the decoded Monero data based on string length.
/// Returns None if the string length is invalid for Monero Base58.
#[inline]
#[must_use]
pub const fn decoded_len(input_len: usize) -> Option<usize> {
    let full_blocks = input_len / 11;
    let remainder = input_len % 11;

    if remainder == 0 {
        Some(full_blocks * 8)
    } else {
        let mut rem_bytes = 0;
        let mut i = 1;
        while i <= 8 {
            if XMR_ENCODED_SIZES[i] == remainder {
                rem_bytes = i;
                break;
            }
            i += 1;
        }

        if rem_bytes == 0 {
            None // Invalid length
        } else {
            Some((full_blocks * 8) + rem_bytes)
        }
    }
}

/// Encodes a slice of bytes into a Monero Base58 string into the provided buffer.
/// Returns the number of bytes written.
///
/// # Errors
///
/// Returns [`Error::BufferTooSmall`] if `output` is not large enough, or an error
/// from the underlying per-chunk [`MONERO`] encoder.
pub fn encode_into<T: AsRef<[u8]>>(input: T, output: &mut [u8]) -> Result<usize, Error> {
    let input = input.as_ref();
    if input.is_empty() {
        return Ok(0);
    }

    let expected_len = encoded_len(input.len());
    if output.len() < expected_len {
        return Err(Error::BufferTooSmall);
    }

    let mut out_idx = 0;

    for chunk in input.chunks(8) {
        let target_len = XMR_ENCODED_SIZES[chunk.len()];

        // Max standard encoding length for 8 bytes is 11
        let mut temp = [0u8; 11];
        let actual_len = MONERO.encode_into(chunk, &mut temp)?;

        // Pad with '1's (alphabet[0] which is '1')
        let pad_len = target_len.saturating_sub(actual_len);
        for _ in 0..pad_len {
            output[out_idx] = b'1';
            out_idx += 1;
        }

        // Copy the actual encoded bytes
        output[out_idx..out_idx + actual_len].copy_from_slice(&temp[..actual_len]);
        out_idx += actual_len;
    }

    Ok(out_idx)
}

/// Decodes a Monero Base58 string into the provided buffer.
/// Returns the number of bytes written.
///
/// # Errors
///
/// Returns [`Error::InvalidCharacter`] if `input` has an invalid chunk length or
/// contains a character outside the alphabet, or [`Error::BufferTooSmall`] if
/// `output` is not large enough.
pub fn decode_into<T: AsRef<[u8]>>(input: T, output: &mut [u8]) -> Result<usize, Error> {
    let input = input.as_ref();
    if input.is_empty() {
        return Ok(0);
    }

    let expected_len = decoded_len(input.len()).ok_or(Error::InvalidCharacter)?;
    if output.len() < expected_len {
        return Err(Error::BufferTooSmall);
    }

    let mut out_idx = 0;

    for chunk_chars in input.chunks(11) {
        let chunk_len = chunk_chars.len();

        // Find expected decoded size
        let mut expected_bytes = 0;
        for (bytes, &chars_len) in XMR_ENCODED_SIZES.iter().enumerate() {
            if chars_len == chunk_len {
                expected_bytes = bytes;
                break;
            }
        }

        if expected_bytes == 0 {
            return Err(Error::InvalidCharacter);
        }

        let chunk_str = core::str::from_utf8(chunk_chars).map_err(|_| Error::InvalidCharacter)?;

        let mut temp = [0u8; 11]; // Up to 11 ones could decode to 11 bytes
        let decoded_len = MONERO.decode_into(chunk_str, &mut temp)?;

        if decoded_len > expected_bytes {
            let excess = decoded_len - expected_bytes;
            for &b in &temp[..excess] {
                if b != 0 {
                    return Err(Error::InvalidCharacter); // Overflow
                }
            }
            output[out_idx..out_idx + expected_bytes].copy_from_slice(&temp[excess..decoded_len]);
            out_idx += expected_bytes;
        } else {
            let pad_len = expected_bytes - decoded_len;
            for _ in 0..pad_len {
                output[out_idx] = 0;
                out_idx += 1;
            }
            output[out_idx..out_idx + decoded_len].copy_from_slice(&temp[..decoded_len]);
            out_idx += decoded_len;
        }
    }

    Ok(out_idx)
}

/// Encodes `input` into a newly allocated Monero Base58 `String`.
///
/// # Errors
///
/// Returns an error from the underlying [`encode_into`].
#[cfg(feature = "std")]
pub fn encode<T: AsRef<[u8]>>(input: T) -> Result<String, Error> {
    let input = input.as_ref();
    if input.is_empty() {
        return Ok(String::new());
    }

    let expected_len = encoded_len(input.len());
    let mut out = vec![0u8; expected_len];

    let actual_len = encode_into(input, &mut out)?;
    out.truncate(actual_len);

    // The Monero alphabet is ASCII, so this conversion always succeeds; the
    // error path is unreachable.
    String::from_utf8(out).map_err(|_| Error::WrongAlphabet)
}

/// Decodes `input` Monero Base58 string into a newly allocated `Vec<u8>`.
///
/// # Errors
///
/// Returns [`Error::InvalidCharacter`] if `input` has an invalid length, or an
/// error from the underlying [`decode_into`].
#[cfg(feature = "std")]
pub fn decode<T: AsRef<[u8]>>(input: T) -> Result<Vec<u8>, Error> {
    let input = input.as_ref();
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let expected_len = decoded_len(input.len()).ok_or(Error::InvalidCharacter)?;
    let mut out = vec![0u8; expected_len];

    let actual_len = decode_into(input, &mut out)?;
    out.truncate(actual_len);
    Ok(out)
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

    /// A character count with no entry in [`XMR_ENCODED_SIZES`] has no valid
    /// decoded length.
    #[test]
    fn decoded_len_rejects_invalid_remainder() {
        assert_eq!(decoded_len(1), None);
        assert_eq!(decoded_len(4), None);
        assert_eq!(decoded_len(8), None);
    }

    /// `encode`/`decode` special-case empty input themselves, so `encode_into`
    /// and `decode_into`'s own empty-input returns are only reachable by
    /// calling them directly.
    #[test]
    fn into_variants_handle_empty_input_directly() {
        let mut buf = [0u8; 11];
        assert_eq!(encode_into(b"", &mut buf), Ok(0));
        assert_eq!(decode_into("", &mut buf), Ok(0));
    }

    #[test]
    fn encode_into_rejects_buffer_too_small() {
        let mut buf = [0u8; 1];
        assert_eq!(encode_into(b"hello", &mut buf), Err(Error::BufferTooSmall));
    }

    #[test]
    fn decode_into_rejects_buffer_too_small() {
        // "zz" (2 chars) needs a 1-byte buffer; give it none.
        let mut buf: [u8; 0] = [];
        assert_eq!(decode_into("zz", &mut buf), Err(Error::BufferTooSmall));
    }

    /// A chunk whose character count matches no valid block size.
    #[test]
    fn decode_into_rejects_invalid_chunk_length() {
        let mut buf = [0u8; 16];
        assert_eq!(decode_into("zzzz", &mut buf), Err(Error::InvalidCharacter));
    }

    /// "zz" decodes (via the plain [`MONERO`] engine) to 2 bytes, but a 2-char
    /// chunk maps to only 1 expected byte, so the excess byte is checked for
    /// overflow -- and here it is nonzero.
    #[test]
    fn decode_into_rejects_excess_overflow() {
        let mut buf = [0u8; 1];
        assert_eq!(decode_into("zz", &mut buf), Err(Error::InvalidCharacter));
    }
}
