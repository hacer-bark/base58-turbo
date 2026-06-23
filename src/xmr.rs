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
#[cfg(feature = "std")]
pub fn encode<T: AsRef<[u8]>>(input: T) -> Result<String, Error> {
    let input = input.as_ref();
    if input.is_empty() {
        return Ok(String::new());
    }

    let expected_len = encoded_len(input.len());
    let mut out = Vec::with_capacity(expected_len);

    #[allow(clippy::uninit_vec)]
    unsafe {
        out.set_len(expected_len);
    }

    match encode_into(input, &mut out) {
        Ok(actual_len) => {
            unsafe { out.set_len(actual_len); }
            unsafe { Ok(String::from_utf8_unchecked(out)) }
        }
        Err(e) => {
            unsafe { out.set_len(0); }
            Err(e)
        }
    }
}

/// Decodes `input` Monero Base58 string into a newly allocated `Vec<u8>`.
#[cfg(feature = "std")]
pub fn decode<T: AsRef<[u8]>>(input: T) -> Result<Vec<u8>, Error> {
    let input = input.as_ref();
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let expected_len = decoded_len(input.len()).ok_or(Error::InvalidCharacter)?;
    let mut out = Vec::with_capacity(expected_len);

    #[allow(clippy::uninit_vec)]
    unsafe {
        out.set_len(expected_len);
    }

    match decode_into(input, &mut out) {
        Ok(actual_len) => {
            unsafe { out.set_len(actual_len); }
            Ok(out)
        }
        Err(e) => {
            unsafe { out.set_len(0); }
            Err(e)
        }
    }
}
