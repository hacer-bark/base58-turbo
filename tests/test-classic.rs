use base58_turbo::{BITCOIN, RIPPLE, FLICKR, Error, Engine};

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
        (b"abcdefghijklmnopqrstuvwxyz", "3yxU3u1igY8WkgtjK92fbJQCd4BZiiT1v25f"),
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
    // The library limits encoded string input to 512 bytes.
    let big_string = "1".repeat(513);
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
fn test_roundtrip_random_data() {
    let mut seed: u64 = rand::random();
    fn next_byte(seed: &mut u64) -> u8 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (*seed >> 56) as u8
    }

    // Limit binary input to ~373 bytes to ensure encoded string stays <= 512.
    let lengths = (0..100).chain((100..373).step_by(13));

    for len in lengths {
        let mut input = Vec::with_capacity(len);
        for _ in 0..len {
            input.push(next_byte(&mut seed));
        }

        let encoded = BITCOIN.encode(&input).unwrap();
        let decoded = BITCOIN.decode(&encoded).unwrap();

        assert_eq!(input, decoded, "Self-consistency mismatch at length {}", len);
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

    // Range 0..373 (Safe limit for base58-turbo buffer)
    for len in (0..373).step_by(7) {
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

    for len in (0..373).step_by(9) {
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

        assert_eq!(actual_str, expected_str, "Ripple encoding mismatch at len {}", len);

        let decoded_bytes = RIPPLE.decode(&expected_str).expect("Turbo decode failed");
        assert_eq!(decoded_bytes, input, "Ripple decoding mismatch at len {}", len);
    }
}
