#![no_main]
use libfuzzer_sys::fuzz_target;
use base58_turbo::{Engine, BITCOIN, MONERO, RIPPLE, FLICKR, Error};

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // 1. Select Engine
    let (engine, payload) = match data[0] % 6 {
        0 => (BITCOIN, &data[1..]),
        1 => (MONERO, &data[1..]),
        2 => (RIPPLE, &data[1..]),
        3 => (FLICKR, &data[1..]),
        4 => {
            // Try to create a custom alphabet if we have enough data
            if data.len() >= 59 {
                let mut alphabet = [0u8; 58];
                alphabet.copy_from_slice(&data[1..59]);
                match Engine::new(&alphabet) {
                    Ok(e) => (e, &data[59..]),
                    Err(_) => (BITCOIN, &data[1..]), // Fallback on invalid alphabet
                }
            } else {
                (BITCOIN, &data[1..])
            }
        },
        _ => {
            // Test invalid alphabet creation
            if data.len() >= 59 {
                let alphabet = [b'a'; 58]; // Duplicate chars
                assert!(Engine::new(&alphabet).is_err());
            }
            (BITCOIN, &data[1..])
        }
    };

    // ----------------------------------------------------------------------
    // 2. Stress Test: ENCODE
    // ----------------------------------------------------------------------
    let encode_result = engine.encode(payload);

    match encode_result {
        Ok(encoded_string) => {
            // [Invariant]: If we got Ok, input MUST be <= 1024
            assert!(payload.len() <= 1024, "API succeeded on input > 1024 bytes! len={}", payload.len());

            // ----------------------------------------------------------------------
            // 3. Stress Test: ROUND TRIP
            // ----------------------------------------------------------------------
            let decode_result = engine.decode(&encoded_string);
            match decode_result {
                Ok(decoded_data) => {
                    assert_eq!(payload, decoded_data.as_slice(), "Round trip mismatch");
                },
                Err(e) => {
                    panic!("Failed to decode valid round-trip data: {:?}", e);
                }
            }

            // Test decode_into with exact buffer
            let max_dec_len = engine.decoded_len(encoded_string.len());
            let mut buf = vec![0u8; max_dec_len];
            let len = engine.decode_into(&encoded_string, &mut buf).unwrap();
            assert_eq!(len, payload.len());
            assert_eq!(&buf[..len], payload);

            // Test decode_into with too small buffer
            if len > 0 {
                let mut small_buf = vec![0u8; len - 1];
                assert_eq!(engine.decode_into(&encoded_string, &mut small_buf), Err(Error::BufferTooSmall));
            }
        },
        Err(e) => {
            match e {
                Error::InputTooBig => {
                    assert!(payload.len() > 1024, "InputTooBig returned for small input len={}", payload.len());
                },
                _ => panic!("Unexpected error during encode: {:?}", e),
            }
        }
    }

    // ----------------------------------------------------------------------
    // 4. Stress Test: DECODE RANDOM GARBAGE
    // ----------------------------------------------------------------------
    let garbage_result = engine.decode(payload);
    match garbage_result {
        Ok(decoded) => {
            // If it succeeded, round trip it back
            let _re_encoded = engine.encode(&decoded).unwrap();
            // Note: Base58 can have multiple encodings for same data if leading zeros are involved in some implementations,
            // but base58-turbo should be consistent.
            // Actually, any valid Base58 string should decode to SOMETHING.
            // If we decode 'payload' (which is random bytes), and it succeeds,
            // then 'payload' must be a valid Base58 string.
            // Re-encoding it might not result in the EXACT same string if the input had leading '1's (in Bitcoin)
            // that were not "minimal". But base58-turbo's encode(decode(x)) should be stable.
        },
        Err(e) => {
            match e {
                Error::InvalidCharacter => {},
                Error::InputTooBig => {
                    // Can happen if string is > 2048 OR if represented value > 1024 bytes
                },
                Error::BufferTooSmall => panic!("Allocating API returned BufferTooSmall"),
                Error::WrongAlphabet => panic!("Decode returned WrongAlphabet"),
            }
        }
    }

    // ----------------------------------------------------------------------
    // 5. Stress Test: ZERO-ALLOCATION BOUNDS CHECKS
    // ----------------------------------------------------------------------
    if !payload.is_empty() && payload.len() <= 1024 {
        let mut tiny_buf = [0u8; 0];
        let res = engine.encode_into(payload, &mut tiny_buf);
        assert_eq!(res, Err(Error::BufferTooSmall));

        let mut tiny_buf = [0u8; 0];
        let res = engine.decode_into(payload, &mut tiny_buf);
        // decode_into might return InvalidCharacter first if payload is garbage
        match res {
            Err(Error::BufferTooSmall) | Err(Error::InvalidCharacter) => {},
            _ => panic!("Unexpected result for tiny decode buffer: {:?}", res),
        }
    }
});
