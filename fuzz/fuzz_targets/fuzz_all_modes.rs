#![no_main]
use libfuzzer_sys::fuzz_target;
use base58_turbo::{BITCOIN, Error};

fuzz_target!(|data: &[u8]| {
    // ----------------------------------------------------------------------
    // 1. Stress Test: ENCODE
    // ----------------------------------------------------------------------
    let encode_result = BITCOIN.encode(data);

    match encode_result {
        Ok(encoded_string) => {
            // [Invariant 1]: If we got Ok, input MUST be <= 512
            assert!(data.len() <= 512, "API succeeded on input > 512 bytes!");

            // ----------------------------------------------------------------------
            // 2. Stress Test: ROUND TRIP (Decode valid output)
            // ----------------------------------------------------------------------
            let decode_result = BITCOIN.decode(&encoded_string);
            match decode_result {
                Ok(decoded_data) => {
                    // [Invariant 2]: Decoded data MUST match original
                    assert_eq!(data, decoded_data.as_slice(), "Round trip mismatch");
                    assert!(data.len() <= 512, "API succeeded on input > 512 bytes!");
                },
                Err(e) => {
                    // [Invariant 3]: Valid encoding MUST decode successfully
                    match e {
                        Error::InputTooBig => {
                            assert!(encoded_string.len() > 512, "InputTooBig returned for small input len={}", encoded_string.len());
                        },
                        _ => panic!("Failed to decode valid round-trip data: {:?}", e),
                    }
                }
            }
        },
        Err(e) => {
            // [Invariant 4]: The only allowed error for Encode is InputTooBig
            // And it MUST only happen if len > 512
            match e {
                Error::InputTooBig => {
                    assert!(data.len() > 512, "InputTooBig returned for small input len={}", data.len());
                },
                _ => panic!("Unexpected error during encode: {:?}", e),
            }
        }
    }

    // ----------------------------------------------------------------------
    // 3. Stress Test: DECODE RANDOM GARBAGE
    // ----------------------------------------------------------------------
    // Throwing raw random bytes at the decoder.
    // It should mostly fail (InvalidCharacter), or succeed (if random bytes happen to be base58),
    // or fail with InputTooBig.
    // IT MUST NEVER PANIC.
    let garbage_result = BITCOIN.decode(data);
    match garbage_result {
        Ok(_) => { 
            // Valid base58 sequence found by chance. Allowed.
        },
        Err(e) => {
            match e {
                Error::InvalidCharacter => {}, // Expected for random bytes
                Error::InputTooBig => {
                    assert!(data.len() > 512, "InputTooBig returned for small input len={}", data.len());
                },
                Error::BufferTooSmall => panic!("Allocating API returned BufferTooSmall"),
            }
        }
    }

    // ----------------------------------------------------------------------
    // 4. Stress Test: ZERO-ALLOCATION BOUNDS CHECKS
    // ----------------------------------------------------------------------
    // If input is valid size, try to encode into a 0-byte buffer.
    // This verifies internal bounds checking works and doesn't write out of bounds.
    if !data.is_empty() && data.len() <= 512 {
        let mut tiny_buf = [0u8; 0];
        let res = BITCOIN.encode_into(data, &mut tiny_buf);
        match res {
            Err(Error::BufferTooSmall) => {}, // Correct behavior
            Err(Error::InputTooBig) => {}, // Also okay behavior
            Ok(_) => panic!("Managed to write into 0-byte buffer!"),
            Err(e) => panic!("Unexpected error type for tiny buffer: {:?}", e),
        }
    }
});
