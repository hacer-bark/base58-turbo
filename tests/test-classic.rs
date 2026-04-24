use base58_turbo::{BITCOIN, Engine, Error, FLICKR, RIPPLE};

// ======================================================================
// 1. Standard Conformance Tests (Bitcoin Alphabet)
// ======================================================================

#[test]
fn test_bitcoin_vectors() {
    // Standard test vectors.
    // Note: b" " is ASCII 32. In Bitcoin Base58, index 32 is 'Z'.
    let tests: &[(&[u8], &str)] = &[
        (b"", ""),
        (b" ", "Z"),
        (b"-", "n"),
        (b"0", "q"),
        (b"1", "r"),
        (b"-1", "4SU"),
        (b"11", "4k8"),
        (b"abc", "ZiCa"),
        (b"1234598760", "3mJr7AoUXx2Wqd"),
        (
            b"abcdefghijklmnopqrstuvwxyz",
            "3yxU3u1igY8WkgtjK92fbJQCd4BZiiT1v25f",
        ),
        (
            b"00000000000000000000000000000000000000000000000000000000000000",
            "3sN2THZeE9Eh9eYrwkvZqNstbHGvrxSAM7gXUXvyFQP8XvQLUqNCS27icwUeDT7ckHm4FUHM2mTVh1vbLmk7y",
        ),
    ];

    for (input, expected) in tests {
        // Encode
        let result = BITCOIN.encode(input).expect("Encoding failed");
        assert_eq!(&result, expected, "Failed encoding input: {:?}", input);

        // Decode
        let decoded = BITCOIN.decode(expected).expect("Decoding failed");
        assert_eq!(&decoded, *input, "Failed decoding input: {}", expected);
    }
}

#[test]
fn test_leading_zeros() {
    // In Bitcoin Base58, leading zero bytes (0x00) become leading '1's.
    let tests: &[(&[u8], &str)] = &[
        (b"\x00", "1"),
        (b"\x00\x00", "11"),
        (b"\x00\x00\x00", "111"),
        (b"\x00\x00\x01", "112"),
        (b"\x00hello", "1Cn8eVZg"),
    ];

    for (input, expected) in tests {
        let encoded = BITCOIN.encode(input).unwrap();
        assert_eq!(encoded, *expected);

        let decoded = BITCOIN.decode(expected).unwrap();
        assert_eq!(decoded, *input);
    }
}

// ======================================================================
// 2. Alternative Engine Tests (Ripple, Flickr)
// ======================================================================

#[test]
fn test_ripple_engine() {
    // Ripple alphabet: "rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz"
    let input = b"Hello World";
    let encoded = RIPPLE.encode(input).unwrap();

    // Ensure it's NOT the Bitcoin output
    assert_ne!(encoded, "JxF12TrwUP45BMd");

    // Ensure Roundtrip works
    let decoded = RIPPLE.decode(&encoded).unwrap();
    assert_eq!(decoded, input);
}

#[test]
fn test_flickr_engine() {
    // Flickr alphabet: "123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ"
    let input = b"Rust is fast";
    let encoded = FLICKR.encode(input).unwrap();

    let decoded = FLICKR.decode(&encoded).unwrap();
    assert_eq!(decoded, input);
}

#[test]
fn test_custom_engine() {
    // Create a custom alphabet
    let alphabet = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ123456789";
    let engine = Engine::new(alphabet).unwrap();

    let input = vec![0, 255, 10, 20];
    let encoded = engine.encode(&input).unwrap();
    let decoded = engine.decode(&encoded).unwrap();

    assert_eq!(input, decoded);
}

// ======================================================================
// 3. Low-Level API Tests (No-Alloc / Buffer)
// ======================================================================

#[test]
fn test_encode_into_buffer() {
    let input = b"hello";
    let mut output = [0u8; 100];

    let len = BITCOIN.encode_into(input, &mut output).unwrap();
    let result_str = std::str::from_utf8(&output[..len]).unwrap();

    assert_eq!(result_str, "Cn8eVZg");
}

#[test]
fn test_decode_into_buffer() {
    let input = "Cn8eVZg"; // "hello"
    let mut output = [0u8; 100];

    let len = BITCOIN.decode_into(input, &mut output).unwrap();
    let result_slice = &output[..len];

    assert_eq!(result_slice, b"hello");
}

#[test]
fn test_encode_into_exact_buffer_size() {
    let input = b"hello";
    let req_len = BITCOIN.encoded_len(input.len());

    let mut output = vec![0u8; req_len];
    let len = BITCOIN.encode_into(input, &mut output).unwrap();

    assert_eq!(len, 7);
    assert_eq!(&output[..len], b"Cn8eVZg");
}

// ======================================================================
// 4. Error Handling Tests
// ======================================================================

#[test]
fn test_error_invalid_char() {
    // '0' is not in Bitcoin Base58
    let bad_input = "Cn8eVZ0";
    let err = BITCOIN.decode(bad_input).unwrap_err();
    assert_eq!(err, Error::InvalidCharacter);
}

#[test]
fn test_error_buffer_too_small_encode() {
    let input = b"hello world";
    let mut small_buf = [0u8; 5]; // Too small

    let err = BITCOIN.encode_into(input, &mut small_buf).unwrap_err();
    assert_eq!(err, Error::BufferTooSmall);
}

#[test]
fn test_error_buffer_too_small_decode() {
    let input = "StV1DL6CwTryKyV"; // "hello world"
    let mut small_buf = [0u8; 5]; // Too small

    let err = BITCOIN.decode_into(input, &mut small_buf).unwrap_err();
    assert_eq!(err, Error::BufferTooSmall);
}

#[test]
fn test_error_input_too_big_explicit() {
    // The library limits encoded string input to 1024 bytes.
    let big_string = "1".repeat(1025);
    let err_dec = BITCOIN.decode(&big_string).unwrap_err();
    assert_eq!(err_dec, Error::InputTooBig);
}

#[test]
fn test_invalid_alphabet_duplicates() {
    // This alphabet has two 'a's and is missing 'b'
    // "a...a" collision
    let bad_alpha = *b"a123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxa";

    match Engine::new(&bad_alpha) {
        Err(Error::WrongAlphabet) => (), // Pass
        _ => panic!("Should have failed with WrongAlphabet"),
    }
}

// ======================================================================
// 5. Property / Fuzz Testing (Self-Consistency)
// ======================================================================

#[test]
#[cfg(not(miri))]
fn test_roundtrip_random_data() {
    let mut seed: u64 = rand::random();
    fn next_byte(seed: &mut u64) -> u8 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (*seed >> 56) as u8
    }

    // Limit binary input to ~740 bytes to ensure encoded string stays <= 1024.
    let lengths = (0..100).chain((100..740).step_by(13));

    for len in lengths {
        let mut input = Vec::with_capacity(len);
        for _ in 0..len {
            input.push(next_byte(&mut seed));
        }

        let encoded = BITCOIN.encode(&input).unwrap();
        let decoded = BITCOIN.decode(&encoded).unwrap();

        assert_eq!(
            input, decoded,
            "Self-consistency mismatch at length {}",
            len
        );
    }
}

// ======================================================================
// 6. Cross-Validation against 'bs58' crate
// ======================================================================

#[test]
#[cfg(not(miri))]
fn test_vs_bs58_crate_bitcoin() {
    let mut seed: u64 = rand::random();
    fn next_byte(seed: &mut u64) -> u8 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (*seed >> 56) as u8
    }

    // Range 0..740 (Safe limit for base58-turbo buffer)
    for len in (0..740).step_by(7) {
        let mut input = Vec::with_capacity(len);
        for _ in 0..len {
            input.push(next_byte(&mut seed));
        }

        // 1. Encode with trusted `bs58` crate
        let expected_str = bs58::encode(&input)
            .with_alphabet(bs58::Alphabet::BITCOIN)
            .into_string();

        // 2. Encode with `base58-turbo`
        let actual_str = BITCOIN.encode(&input).expect("Turbo encode failed");

        // 3. Assert Encoding Match
        assert_eq!(actual_str, expected_str, "Encoding mismatch at len {}", len);

        // 4. Decode using `base58-turbo` (Input generated by `bs58`)
        let decoded_bytes = BITCOIN.decode(&expected_str).expect("Turbo decode failed");

        // 5. Assert Decoding Match
        assert_eq!(decoded_bytes, input, "Decoding mismatch at len {}", len);
    }
}

#[test]
#[cfg(not(miri))]
fn test_vs_bs58_crate_ripple() {
    let mut seed: u64 = rand::random();
    fn next_byte(seed: &mut u64) -> u8 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (*seed >> 56) as u8
    }

    for len in (0..740).step_by(9) {
        let mut input = Vec::with_capacity(len);
        for _ in 0..len {
            input.push(next_byte(&mut seed));
        }

        // 1. Encode with trusted `bs58` crate using RIPPLE alphabet
        let expected_str = bs58::encode(&input)
            .with_alphabet(bs58::Alphabet::RIPPLE)
            .into_string();

        // 2. Encode with `base58-turbo` RIPPLE engine
        let actual_str = RIPPLE.encode(&input).expect("Turbo encode failed");

        assert_eq!(
            actual_str, expected_str,
            "Ripple encoding mismatch at len {}",
            len
        );

        let decoded_bytes = RIPPLE.decode(&expected_str).expect("Turbo decode failed");
        assert_eq!(
            decoded_bytes, input,
            "Ripple decoding mismatch at len {}",
            len
        );
    }
}

// ======================================================================
// 7. Additional tests
// ======================================================================

#[test]
fn test_monero_engine() {
    let input = b"Hello World";
    let encoded = base58_turbo::MONERO.encode(input).unwrap();
    let decoded = base58_turbo::MONERO.decode(&encoded).unwrap();
    assert_eq!(decoded, input);
}

#[test]
fn test_empty_input() {
    assert_eq!(BITCOIN.encode(b"").unwrap(), "");
    assert_eq!(BITCOIN.decode("").unwrap(), b"");

    let mut out = [0u8; 10];
    assert_eq!(BITCOIN.encode_into(b"", &mut out).unwrap(), 0);
    assert_eq!(BITCOIN.decode_into("", &mut out).unwrap(), 0);
}

#[test]
fn test_input_too_big_encode() {
    let input = [0u8; 1025];
    assert_eq!(BITCOIN.encode(&input).unwrap_err(), Error::InputTooBig);

    let mut out = [0u8; 2048];
    assert_eq!(
        BITCOIN.encode_into(&input, &mut out).unwrap_err(),
        Error::InputTooBig
    );
}

#[test]
fn test_decode_buffer_too_small_edge_cases() {
    // Case 1: leading zeros buffer too small
    let input = "111"; // 3 leading zeros
    let mut out = [0u8; 2];
    assert_eq!(
        BITCOIN.decode_into(input, &mut out).unwrap_err(),
        Error::BufferTooSmall
    );

    // Case 2: payload too big for remaining buffer
    let input = "112"; // 2 leading zeros + '2' (decoded to 0x01)
    let mut out = [0u8; 2];
    // decoded_len for "112" is 3. output.len() is 2.
    assert_eq!(
        BITCOIN.decode_into(input, &mut out).unwrap_err(),
        Error::BufferTooSmall
    );
}

#[test]
fn test_decode_normalization_and_memmove() {
    // We need an input that decodes to something where out_idx > 0 after emission phase
    // but before normalization.
    // The emission phase writes in blocks of 8 bytes (u64).
    // If the decoded value is small, it will have leading zeros in the bignum.
    let input = "2"; // Decodes to 0x01
    let mut out = [0u8; 10];
    let len = BITCOIN.decode_into(input, &mut out).unwrap();
    assert_eq!(len, 1);
    assert_eq!(out[0], 0x01);
}

#[test]
fn test_encode_vectorized_zeros_only() {
    let input = [0u8; 16];
    let encoded = BITCOIN.encode(&input).unwrap();
    assert_eq!(encoded, "1".repeat(16));
}

#[test]
fn test_decode_invalid_char_in_chunk() {
    // "2222222222" is a 10-char chunk.
    // Replacing one with '0' (invalid)
    let input = "2222022222";
    assert_eq!(BITCOIN.decode(input).unwrap_err(), Error::InvalidCharacter);
}

#[test]
fn test_decode_large_payload_buffer_too_small() {
    // Payload that is larger than the provided buffer during emission
    // "JxF12TrwUP45BMd" is "Hello World" (11 bytes)
    let input = "JxF12TrwUP45BMd";
    let mut out = [0u8; 5];
    // Note: decoded_len returns input.len() which is 15.
    // So decode_into will fail early because 15 > 5.
    assert_eq!(
        BITCOIN.decode_into(input, &mut out).unwrap_err(),
        Error::BufferTooSmall
    );
}

#[test]
fn test_config_new_alphabet_uniqueness() {
    let mut alphabet = *b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    alphabet[57] = alphabet[0]; // Duplicate
    assert_eq!(
        base58_turbo::Config::new(&alphabet).unwrap_err(),
        Error::WrongAlphabet
    );
}

#[test]
fn test_error_display() {
    assert_eq!(
        format!("{}", Error::InvalidCharacter),
        "invalid character in base58 string"
    );
    assert_eq!(
        format!("{}", Error::BufferTooSmall),
        "output buffer too small"
    );
    assert_eq!(format!("{}", Error::InputTooBig), "input data too big");
    assert_eq!(
        format!("{}", Error::WrongAlphabet),
        "input alphabet has duplicate chars"
    );
}

#[test]
fn test_engine_config_access() {
    let config = BITCOIN.config();
    assert_eq!(config.alphabet[0], b'1');
}

#[test]
fn test_len_calculators() {
    assert_eq!(BITCOIN.encoded_len(0), 1);
    assert_eq!(BITCOIN.decoded_len(0), 0);
    assert!(BITCOIN.encoded_len(1024) >= 1024);
    assert_eq!(BITCOIN.decoded_len(2048), 2048);
}

#[cfg(feature = "serde")]
#[test]
fn test_serde_config_engine() {
    let alpha = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let config = base58_turbo::Config::new(alpha).unwrap();
    let engine = base58_turbo::Engine::new(alpha).unwrap();

    // Serialize Config
    let conf_json = serde_json::to_string(&config).unwrap();
    assert_eq!(
        conf_json,
        format!("\"{}\"", std::str::from_utf8(alpha).unwrap())
    );

    // Deserialize Config
    let de_conf: base58_turbo::Config = serde_json::from_str(&conf_json).unwrap();
    assert_eq!(de_conf.alphabet, config.alphabet);

    // Serialize Engine
    let eng_json = serde_json::to_string(&engine).unwrap();
    assert_eq!(eng_json, conf_json);

    // Deserialize Engine
    let de_eng: base58_turbo::Engine = serde_json::from_str(&eng_json).unwrap();
    assert_eq!(de_eng.config().alphabet, engine.config().alphabet);

    // Test Error: wrong length alphabet in serde
    let res: Result<base58_turbo::Config, _> = serde_json::from_str("\"abc\"");
    assert!(res.is_err());
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("expected exactly 58-byte alphabet")
    );

    // Test Error: duplicate chars in alphabet via serde
    let mut bad_alpha = *b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    bad_alpha[57] = bad_alpha[0];
    let bad_alpha_str = std::str::from_utf8(&bad_alpha).unwrap();
    let res: Result<base58_turbo::Config, _> =
        serde_json::from_str(&format!("\"{}\"", bad_alpha_str));
    assert!(res.is_err());

    // Test Error: duplicate chars in alphabet via engine serde
    let res_eng: Result<base58_turbo::Engine, _> =
        serde_json::from_str(&format!("\"{}\"", bad_alpha_str));
    assert!(res_eng.is_err());
}
