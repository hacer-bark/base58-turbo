//! Monero-specific Base58 chunked encoding and decoding.
//!
//! Data is split into 8-byte blocks, each encoded as exactly 11 characters. A
//! shorter final block is padded to the fixed width for its byte count, so the
//! encoded length depends only on the input length.

use crate::{Error, MONERO};

#[cfg(feature = "std")]
use std::string::String;
#[cfg(feature = "std")]
use std::vec::Vec;

const XMR_ENCODED_SIZES: [usize; 9] = [0, 2, 3, 5, 6, 7, 9, 10, 11];

/// Returns the exact encoded length for `input_len` bytes.
#[inline]
#[must_use]
pub const fn encoded_len(input_len: usize) -> usize {
    let full_blocks = input_len / 8;
    let remainder = input_len % 8;
    (full_blocks * 11) + XMR_ENCODED_SIZES[remainder]
}

/// Returns the exact decoded length for `input_len` characters, or `None` if no
/// input encodes to that many characters.
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

/// Encodes `input` into `output`, returning the number of bytes written.
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

        let mut temp = [0u8; 11];
        let actual_len = MONERO.encode_into(chunk, &mut temp)?;

        // '1' is the Monero alphabet's zero digit.
        let pad_len = target_len.saturating_sub(actual_len);
        output[out_idx..out_idx + pad_len].fill(b'1');
        out_idx += pad_len;

        output[out_idx..out_idx + actual_len].copy_from_slice(&temp[..actual_len]);
        out_idx += actual_len;
    }

    Ok(out_idx)
}

/// Decodes `input` into `output`, returning the number of bytes written.
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
        let expected_bytes = XMR_ENCODED_SIZES
            .iter()
            .position(|&n| n == chunk_chars.len())
            .ok_or(Error::InvalidCharacter)?;

        // Leading '1's decode to zero bytes, so 11 characters can yield 11 bytes.
        let mut temp = [0u8; 11];
        let written = MONERO.decode_into(chunk_chars, &mut temp)?;

        if written > expected_bytes {
            let excess = written - expected_bytes;
            // The block's value overflows its byte width.
            if temp[..excess].iter().any(|&b| b != 0) {
                return Err(Error::InvalidCharacter);
            }
            output[out_idx..out_idx + expected_bytes].copy_from_slice(&temp[excess..written]);
            out_idx += expected_bytes;
        } else {
            let pad_len = expected_bytes - written;
            output[out_idx..out_idx + pad_len].fill(0);
            out_idx += pad_len;
            output[out_idx..out_idx + written].copy_from_slice(&temp[..written]);
            out_idx += written;
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

    // The Monero alphabet is ASCII, so this never fails.
    String::from_utf8(out).map_err(|_| Error::WrongAlphabet)
}

/// Decodes `input` into a newly allocated `Vec<u8>`.
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
