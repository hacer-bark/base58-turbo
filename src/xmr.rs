//! Monero-specific Base58 chunked encoding and decoding.
//!
//! Data is split into 8-byte blocks, each encoded as exactly 11 characters. A
//! shorter final block is padded to the fixed width for its byte count, so the
//! encoded length depends only on the input length. Blocks are converted
//! directly, one `u64` at a time, rather than through the general engine.

use crate::decode::{RADIX_58_10, parse_chunk, parse_chunk_10};
use crate::encode::{emit_full_block, emit_partial_block};
use crate::{Error, MONERO};

#[cfg(feature = "std")]
use std::string::String;
#[cfg(feature = "std")]
use std::vec::Vec;

/// Encoded width of a block, indexed by its byte count.
const XMR_ENCODED_SIZES: [usize; 9] = [0, 2, 3, 5, 6, 7, 9, 10, 11];

/// Byte count of a block, indexed by its encoded width; `None` where no block
/// encodes to that many characters.
const XMR_DECODED_SIZES: [Option<usize>; 12] = [
    Some(0),
    None,
    Some(1),
    Some(2),
    None,
    Some(3),
    Some(4),
    Some(5),
    None,
    Some(6),
    Some(7),
    Some(8),
];

/// Returns the exact encoded length for `input_len` bytes.
#[inline]
#[must_use]
pub const fn encoded_len(input_len: usize) -> usize {
    (input_len / 8)
        .saturating_mul(11)
        .saturating_add(XMR_ENCODED_SIZES[input_len % 8])
}

/// Returns the exact decoded length for `input_len` characters, or `None` if no
/// input encodes to that many characters.
#[inline]
#[must_use]
pub const fn decoded_len(input_len: usize) -> Option<usize> {
    match XMR_DECODED_SIZES[input_len % 11] {
        Some(tail) => Some(input_len / 11 * 8 + tail),
        None => None,
    }
}

/// Encodes `input` into `output`, returning the number of bytes written.
///
/// # Errors
///
/// Returns [`Error::BufferTooSmall`] if `output` is shorter than
/// [`encoded_len`] of the input.
pub fn encode_into<T: AsRef<[u8]>>(input: T, output: &mut [u8]) -> Result<usize, Error> {
    let input = input.as_ref();
    let len = encoded_len(input.len());
    let output = output.get_mut(..len).ok_or(Error::BufferTooSmall)?;
    let config = MONERO.config();

    let mut blocks = input.chunks_exact(8);
    let mut outs = output.chunks_exact_mut(11);
    for (block, out) in blocks.by_ref().zip(outs.by_ref()) {
        let block: [u8; 8] = block.try_into().unwrap_or_else(|_| unreachable!());
        let out: &mut [u8; 11] = out.try_into().unwrap_or_else(|_| unreachable!());
        let [top, rest @ ..] = out;

        let val = u64::from_be_bytes(block);
        // `u64::MAX / 58^10` is 42, so the top character is always in range.
        *top = config.alphabet[(val / RADIX_58_10) as usize];
        emit_full_block(config, val % RADIX_58_10, rest);
    }

    let tail = blocks.remainder();
    if !tail.is_empty() {
        let val = tail.iter().fold(0u64, |acc, &b| (acc << 8) | u64::from(b));
        emit_partial_block(config, val, outs.into_remainder());
    }

    Ok(len)
}

/// Decodes `input` into `output`, returning the number of bytes written.
///
/// # Errors
///
/// Returns [`Error::InvalidCharacter`] if `input` has an invalid length, contains
/// a character outside the alphabet, or has a block whose value overflows its
/// byte width, or [`Error::BufferTooSmall`] if `output` is shorter than
/// [`decoded_len`] of the input.
pub fn decode_into<T: AsRef<[u8]>>(input: T, output: &mut [u8]) -> Result<usize, Error> {
    let input = input.as_ref();
    let len = decoded_len(input.len()).ok_or(Error::InvalidCharacter)?;
    let output = output.get_mut(..len).ok_or(Error::BufferTooSmall)?;
    let config = MONERO.config();

    let mut blocks = input.chunks_exact(11);
    let mut outs = output.chunks_exact_mut(8);
    for (block, out) in blocks.by_ref().zip(outs.by_ref()) {
        let block: &[u8; 11] = block.try_into().unwrap_or_else(|_| unreachable!());
        let [top, rest @ ..] = block;

        // An invalid top character maps to 255, so it fails the same overflow
        // check as a valid one above 42.
        let val = u64::from(config.decode_map[usize::from(*top)])
            .checked_mul(RADIX_58_10)
            .and_then(|hi| hi.checked_add(parse_chunk_10(config, rest).ok()?))
            .ok_or(Error::InvalidCharacter)?;
        out.copy_from_slice(&val.to_be_bytes());
    }

    let tail = blocks.remainder();
    if !tail.is_empty() {
        let out = outs.into_remainder();
        let (val, _) = parse_chunk(config, tail)?;
        if val >> (8 * out.len()) != 0 {
            return Err(Error::InvalidCharacter);
        }
        out.copy_from_slice(&val.to_be_bytes()[8 - out.len()..]);
    }

    Ok(len)
}

/// Encodes `input` into a newly allocated Monero Base58 `String`.
///
/// # Errors
///
/// Returns [`Error::InputTooBig`] only if the output would exceed `isize::MAX`
/// bytes, which needs an input of about 1.5 GB on a 32-bit target.
#[cfg(feature = "std")]
pub fn encode<T: AsRef<[u8]>>(input: T) -> Result<String, Error> {
    let input = input.as_ref();
    let len = encoded_len(input.len());
    if len > isize::MAX as usize {
        return Err(Error::InputTooBig);
    }
    let mut out = vec![0u8; len];
    encode_into(input, &mut out)?;
    // The Monero alphabet is ASCII.
    String::from_utf8(out).map_err(|_| Error::WrongAlphabet)
}

/// Decodes `input` into a newly allocated `Vec<u8>`.
///
/// # Errors
///
/// Returns [`Error::InvalidCharacter`] under the same conditions as
/// [`decode_into`].
#[cfg(feature = "std")]
pub fn decode<T: AsRef<[u8]>>(input: T) -> Result<Vec<u8>, Error> {
    let input = input.as_ref();
    let mut out = vec![0u8; decoded_len(input.len()).ok_or(Error::InvalidCharacter)?];
    decode_into(input, &mut out)?;
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

    #[test]
    fn decoded_len_rejects_invalid_remainder() {
        assert_eq!(decoded_len(1), None);
        assert_eq!(decoded_len(4), None);
        assert_eq!(decoded_len(8), None);
    }

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

    /// "zz" is 3363, too big for the one byte a 2-character block holds.
    #[test]
    fn decode_into_rejects_excess_overflow() {
        let mut buf = [0u8; 1];
        assert_eq!(decode_into("zz", &mut buf), Err(Error::InvalidCharacter));
    }
}
