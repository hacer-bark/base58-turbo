mod encode;
mod decode;
pub use encode::encode_slice_unsafe;
pub use decode::decode_slice_unsafe;

/// Errors that can occur during Base64 encoding or decoding operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// An invalid character was encountered during decoding.
    ///
    /// This occurs if the input contains bytes that do not belong to the
    /// selected Base64 alphabet (e.g., symbols not in the standard set) or
    /// if padding characters (`=`) appear in invalid positions.
    InvalidCharacter,

    /// The provided output buffer is too small to hold the result.
    ///
    /// This error is returned by the zero-allocation APIs (e.g., `encode_into`, `decode_into`)
    /// when the destination slice passed by the user does not have enough capacity
    /// to store the encoded or decoded data.
    BufferTooSmall,
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

            let mut buff = vec![0u8; 1024];

            let written = unsafe { encode_slice_unsafe(&data, buff.as_mut_ptr()) };

            assert_eq!(&buff[..written], encoded.as_bytes(), "Failed at size {}", i);

            let mut buff_dec = vec![0u8; 1024];
            let written_dec = unsafe { decode_slice_unsafe(&buff[..written], &mut buff_dec).unwrap() };

            assert_eq!(data, &buff_dec[..written_dec], "Failed at size {}", i);
        }
    }
}
