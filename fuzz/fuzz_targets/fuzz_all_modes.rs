#![no_main]
use base58_turbo::{BITCOIN, Engine, Error, FLICKR, MONERO, RIPPLE};
use libfuzzer_sys::fuzz_target;

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
        }
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
    // 2. Round trip: the allocating API has no size limit and cannot fail
    // ----------------------------------------------------------------------
    let encoded_string = engine.encode(payload).unwrap();
    assert_eq!(
        engine.decode(&encoded_string).unwrap(),
        payload,
        "Round trip mismatch"
    );

    // The zero-allocation API, within its limits.
    if payload.len() <= 1024 {
        let mut buf = vec![0u8; engine.decoded_len(encoded_string.len())];
        let len = engine.decode_into(&encoded_string, &mut buf).unwrap();
        assert_eq!(&buf[..len], payload);

        if len > 0 {
            let mut small_buf = vec![0u8; len - 1];
            assert_eq!(
                engine.decode_into(&encoded_string, &mut small_buf),
                Err(Error::BufferTooSmall)
            );
        }
    }

    // ----------------------------------------------------------------------
    // 3. Decode random garbage: valid Base58 is canonical, so it re-encodes exactly
    // ----------------------------------------------------------------------
    match engine.decode(payload) {
        Ok(decoded) => assert_eq!(engine.encode(&decoded).unwrap().as_bytes(), payload),
        Err(Error::InvalidCharacter) => {}
        Err(e) => panic!("Allocating decode returned {:?}", e),
    }

    // ----------------------------------------------------------------------
    // 4. Zero-allocation bounds checks
    // ----------------------------------------------------------------------
    if !payload.is_empty() && payload.len() <= 1024 {
        let mut tiny_buf = [0u8; 0];
        let res = engine.encode_into(payload, &mut tiny_buf);
        assert_eq!(res, Err(Error::BufferTooSmall));

        let mut tiny_buf = [0u8; 0];
        let res = engine.decode_into(payload, &mut tiny_buf);
        assert_eq!(res, Err(Error::BufferTooSmall));
    }
});
