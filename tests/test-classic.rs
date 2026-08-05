#![allow(
    missing_docs,
    missing_debug_implementations,
    unreachable_pub,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use base58_turbo::{BITCOIN, Config, Engine, Error, FLICKR, RIPPLE};
use rand::{RngExt, rng};

// ======================================================================
// 1. Standard Conformance Tests (Bitcoin Alphabet)
// ======================================================================

#[test]
fn test_bitcoin_vectors() {
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
        let result = BITCOIN.encode(input).expect("encoding failed");
        assert_eq!(&result, expected, "encoding {input:?}");

        let decoded = BITCOIN.decode(expected).expect("decoding failed");
        assert_eq!(&decoded, *input, "decoding {expected}");
    }
}

#[test]
fn test_public_vectors() {
    // Vectors from the standard Bitcoin Base58 test suite (hex payload, base58 form).
    let vectors: &[(&[u8], &str)] = &[
        (&[0x61], "2g"),
        (&[0x62, 0x62, 0x62], "a3gV"),
        (&[0x63, 0x63, 0x63], "aPEr"),
        (b"simply a long string", "2cFupjhnEsSn59qHXstmK2ffpLv2"),
        (
            &[
                0x00, 0xeb, 0x15, 0x23, 0x1d, 0xfc, 0xeb, 0x60, 0x92, 0x58, 0x86, 0xb6, 0x7d, 0x06,
                0x52, 0x99, 0x92, 0x59, 0x15, 0xae, 0xb1, 0x72, 0xc0, 0x66, 0x47,
            ],
            "1NS17iag9jJgTHD1VXjvLCEnZuQ3rJDE9L",
        ),
    ];

    for (input, expected) in vectors {
        let actual = BITCOIN.encode(input).unwrap();
        assert_eq!(actual, *expected);

        let decoded = BITCOIN.decode(*expected).unwrap();
        assert_eq!(decoded, *input);
    }
}

#[test]
fn test_leading_zeros() {
    // Leading zero bytes (0x00) become leading '1's.
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

#[test]
fn test_encode_zero_prefix_only() {
    let input = [0u8; 16];
    let encoded = BITCOIN.encode(input).unwrap();
    assert_eq!(encoded, "1".repeat(16));
}

// ======================================================================
// 2. Alternative Engines (Ripple, Flickr, Custom)
// ======================================================================

#[test]
fn test_ripple_engine() {
    // Ripple alphabet: "rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz"
    let input = b"Hello World";
    let encoded = RIPPLE.encode(input).unwrap();

    assert_ne!(
        encoded, "JxF12TrwUP45BMd",
        "must differ from the Bitcoin alphabet"
    );

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
fn test_monero_engine() {
    let input = b"Hello World";
    let encoded = base58_turbo::MONERO.encode(input).unwrap();
    let decoded = base58_turbo::MONERO.decode(&encoded).unwrap();
    assert_eq!(decoded, input);
}

#[test]
fn test_custom_engine() {
    let alphabet = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ123456789";
    let engine = Engine::new(alphabet).unwrap();

    let input = vec![0, 255, 10, 20];
    let encoded = engine.encode(&input).unwrap();
    let decoded = engine.decode(&encoded).unwrap();

    assert_eq!(input, decoded);
}

// ======================================================================
// 3. Low-Level API (Zero-Allocation Buffers)
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
    assert_eq!(&output[..len], b"hello");
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

#[test]
fn test_empty_input() {
    assert_eq!(BITCOIN.encode(b"").unwrap(), "");
    assert_eq!(BITCOIN.decode("").unwrap(), b"");

    let mut out = [0u8; 10];
    assert_eq!(BITCOIN.encode_into(b"", &mut out).unwrap(), 0);
    assert_eq!(BITCOIN.decode_into("", &mut out).unwrap(), 0);
}

#[test]
fn test_len_calculators() {
    assert_eq!(BITCOIN.encoded_len(0), 1);
    assert_eq!(BITCOIN.decoded_len(0), 0);
    assert!(BITCOIN.encoded_len(1024) >= 1024);
    assert_eq!(BITCOIN.decoded_len(2048), 2048);
}

#[test]
fn test_engine_config_access() {
    let config = BITCOIN.config();
    assert_eq!(config.alphabet[0], b'1');
}

// ======================================================================
// 4. Error Handling
// ======================================================================

#[test]
fn test_error_invalid_char() {
    // '0' is not in the Bitcoin alphabet.
    let err = BITCOIN.decode("Cn8eVZ0").unwrap_err();
    assert_eq!(err, Error::InvalidCharacter);
}

#[test]
fn test_error_invalid_char_inside_full_chunk() {
    // "2222222222" is a full 10-char decode chunk; put the bad byte in the middle.
    let err = BITCOIN.decode("2222022222").unwrap_err();
    assert_eq!(err, Error::InvalidCharacter);
}

#[test]
fn test_error_buffer_too_small_encode() {
    let input = b"hello world";
    let mut small_buf = [0u8; 5];

    let err = BITCOIN.encode_into(input, &mut small_buf).unwrap_err();
    assert_eq!(err, Error::BufferTooSmall);
}

#[test]
fn test_error_buffer_too_small_decode() {
    let input = "StV1DL6CwTryKyV"; // "hello world"
    let mut small_buf = [0u8; 5];

    let err = BITCOIN.decode_into(input, &mut small_buf).unwrap_err();
    assert_eq!(err, Error::BufferTooSmall);
}

#[test]
fn test_error_buffer_too_small_leading_zeros() {
    // 3 leading zeros need 3 bytes of buffer before the payload is even reached.
    let mut out = [0u8; 2];
    assert_eq!(
        BITCOIN.decode_into("111", &mut out).unwrap_err(),
        Error::BufferTooSmall
    );
}

#[test]
fn test_error_buffer_too_small_payload_after_zeros() {
    // 2 leading zeros + '2' (decodes to 0x01): decoded_len("112") is 3, buffer is 2.
    let mut out = [0u8; 2];
    assert_eq!(
        BITCOIN.decode_into("112", &mut out).unwrap_err(),
        Error::BufferTooSmall
    );
}

#[test]
fn test_error_buffer_too_small_large_payload() {
    // decoded_len returns input.len() (15), which already exceeds the buffer.
    let mut out = [0u8; 5];
    assert_eq!(
        BITCOIN
            .decode_into("JxF12TrwUP45BMd", &mut out)
            .unwrap_err(),
        Error::BufferTooSmall
    );
}

#[test]
fn test_error_input_too_big_into() {
    // The zero-allocation `_into` API keeps its stack-scratch size limit.
    let input = [0u8; 1025];
    let mut out = [0u8; 2048];
    assert_eq!(
        BITCOIN.encode_into(input, &mut out).unwrap_err(),
        Error::InputTooBig
    );

    let big_string = "1".repeat(2049);
    let mut out = [0u8; 4096];
    assert_eq!(
        BITCOIN.decode_into(&big_string, &mut out).unwrap_err(),
        Error::InputTooBig
    );
}

#[test]
fn test_allocating_api_has_no_size_limit() {
    // The allocating `encode`/`decode` API has no size cap: inputs at, just past,
    // and well past the `_into` kernels' stack-scratch ceiling all round-trip.
    for len in [1023, 1024, 1025, 2000, 2048, 2049, 5_000, 50_000] {
        let input: Vec<u8> = rng().random_iter::<u8>().take(len).collect();
        let encoded = BITCOIN.encode(&input).unwrap();
        assert_eq!(BITCOIN.decode(&encoded).unwrap(), input, "len {len}");
    }

    // Zero-prefixed large inputs exercise the leading-zero scan feeding into the
    // unbounded payload path.
    for &zeros in &[0usize, 1, 8, 1024] {
        let mut input = vec![0u8; zeros];
        input.extend(rng().random_iter::<u8>().take(5_000));
        let encoded = BITCOIN.encode(&input).unwrap();
        assert_eq!(BITCOIN.decode(&encoded).unwrap(), input, "zeros {zeros}");
    }

    let big_string = "1".repeat(5_000);
    assert_eq!(BITCOIN.decode(&big_string).unwrap(), vec![0u8; 5_000]);
}

#[test]
fn test_error_wrong_alphabet_duplicate() {
    // Two 'a's, missing 'b'.
    let bad_alpha = *b"a123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxa";
    assert_eq!(Engine::new(&bad_alpha).unwrap_err(), Error::WrongAlphabet);
}

#[test]
fn test_error_wrong_alphabet_non_ascii() {
    let mut bad_alpha = *b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    bad_alpha[0] = 0xC3; // not ASCII
    assert_eq!(Config::new(&bad_alpha).unwrap_err(), Error::WrongAlphabet);
}

#[test]
fn test_error_display() {
    assert_eq!(
        Error::InvalidCharacter.to_string(),
        "invalid character in base58 string"
    );
    assert_eq!(Error::BufferTooSmall.to_string(), "output buffer too small");
    assert_eq!(Error::InputTooBig.to_string(), "input data too big");
    assert_eq!(
        Error::WrongAlphabet.to_string(),
        "input alphabet has duplicate chars"
    );
}

// ======================================================================
// 5. Kernel & Scratch-Class Boundaries
// ======================================================================
//
// The encoder dispatches dedicated kernels at 25, 32 and 64 bytes, and scratch
// buffers sized for <=24, <=64, <=320 and <=1024 bytes elsewhere. These lengths,
// and their immediate neighbours, are where an off-by-one in the dispatch would
// show up.

#[test]
fn test_roundtrip_boundary_lengths() {
    let lens = [
        0, 1, 3, 4, 15, 16, 23, 24, 25, 26, 31, 32, 33, 63, 64, 65, 79, 80, 81, 95, 96, 127, 128,
        129, 191, 192, 255, 256, 319, 320, 321, 511, 512, 639, 640, 1023, 1024,
    ];

    for len in lens {
        let input = rng().random_iter::<u8>().take(len).collect::<Vec<_>>();
        let encoded = BITCOIN.encode(&input).unwrap();
        let decoded = BITCOIN.decode(&encoded).unwrap();
        assert_eq!(decoded, input, "roundtrip mismatch at len {len}");
    }
}

#[test]
fn test_roundtrip_zero_prefix_at_boundaries() {
    // Every boundary length, prefixed with a range of zero counts, so the
    // "skip zeros, then dispatch on remaining length" split gets covered too.
    for &zeros in &[0usize, 1, 7, 8, 9, 24, 32, 64] {
        for &body in &[0usize, 1, 24, 25, 32, 64, 65, 128] {
            if zeros + body > 1024 {
                continue;
            }
            let mut input = vec![0u8; zeros];
            input.extend(rng().random_iter::<u8>().take(body));

            let encoded = BITCOIN.encode(&input).unwrap();
            let decoded = BITCOIN.decode(&encoded).unwrap();
            assert_eq!(decoded, input, "mismatch at zeros={zeros} body={body}");
        }
    }
}

#[test]
fn test_decode_large_payload_normalization() {
    // A single low-value digit leaves leading zero words inside the internal
    // bignum before the emission phase strips them back out.
    let mut out = [0u8; 10];
    let len = BITCOIN.decode_into("2", &mut out).unwrap();
    assert_eq!(len, 1);
    assert_eq!(out[0], 0x01);
}

// ======================================================================
// 6. Property Test: Self-Consistency
// ======================================================================

#[test]
fn test_roundtrip_random_data() {
    for len in (0..100).chain((100..900).step_by(13)) {
        let input = rng().random_iter::<u8>().take(len).collect::<Vec<_>>();
        let encoded = BITCOIN.encode(&input).unwrap();
        let decoded = BITCOIN.decode(&encoded).unwrap();
        assert_eq!(input, decoded, "self-consistency mismatch at length {len}");
    }
}

// ======================================================================
// 7. Cross-Validation Against Other Base58 Crates
// ======================================================================

#[test]
fn test_vs_bs58_crate_bitcoin() {
    for len in (0..900).step_by(7) {
        let input = rng().random_iter::<u8>().take(len).collect::<Vec<_>>();

        let expected = bs58::encode(&input)
            .with_alphabet(bs58::Alphabet::BITCOIN)
            .into_string();
        let actual = BITCOIN.encode(&input).unwrap();
        assert_eq!(actual, expected, "encoding mismatch at len {len}");

        let decoded = BITCOIN.decode(&expected).unwrap();
        assert_eq!(decoded, input, "decoding mismatch at len {len}");
    }
}

#[test]
fn test_vs_bs58_crate_ripple() {
    for len in (0..900).step_by(9) {
        let input = rng().random_iter::<u8>().take(len).collect::<Vec<_>>();

        let expected = bs58::encode(&input)
            .with_alphabet(bs58::Alphabet::RIPPLE)
            .into_string();
        let actual = RIPPLE.encode(&input).unwrap();
        assert_eq!(actual, expected, "ripple encoding mismatch at len {len}");

        let decoded = RIPPLE.decode(&expected).unwrap();
        assert_eq!(decoded, input, "ripple decoding mismatch at len {len}");
    }
}

#[test]
fn test_vs_bs58_crate_monero() {
    for len in (0..900).step_by(9) {
        let input = rng().random_iter::<u8>().take(len).collect::<Vec<_>>();

        let expected = bs58::encode(&input)
            .with_alphabet(bs58::Alphabet::MONERO)
            .into_string();
        let actual = base58_turbo::MONERO.encode(&input).unwrap();
        assert_eq!(actual, expected, "monero encoding mismatch at len {len}");

        let decoded = base58_turbo::MONERO.decode(&expected).unwrap();
        assert_eq!(decoded, input, "monero decoding mismatch at len {len}");
    }
}

#[test]
fn test_vs_base58_crate_bitcoin() {
    use base58::ToBase58;

    for len in (0..900).step_by(11) {
        let input = rng().random_iter::<u8>().take(len).collect::<Vec<_>>();

        let expected = input.to_base58();
        let actual = BITCOIN.encode(&input).unwrap();
        assert_eq!(actual, expected, "base58 crate mismatch at len {len}");

        let decoded = BITCOIN.decode(&expected).unwrap();
        assert_eq!(decoded, input, "base58 crate decode mismatch at len {len}");
    }
}

#[test]
fn test_vs_five8_crate_bitcoin() {
    use five8::{decode_32, decode_64};

    // five8 only supports fixed 32- and 64-byte payloads.
    for &len in &[32usize, 64] {
        let input = rng().random_iter::<u8>().take(len).collect::<Vec<_>>();
        let encoded = BITCOIN.encode(&input).unwrap();

        if len == 32 {
            let mut decoded = [0u8; 32];
            if decode_32(encoded.as_bytes(), &mut decoded).is_ok() {
                assert_eq!(decoded.as_slice(), input.as_slice());
            }
        } else {
            let mut decoded = [0u8; 64];
            if decode_64(encoded.as_bytes(), &mut decoded).is_ok() {
                assert_eq!(decoded.as_slice(), input.as_slice());
            }
        }
    }
}

// ======================================================================
// 8. Serde
// ======================================================================

#[cfg(feature = "serde")]
#[test]
fn test_serde_config_engine() {
    let alpha = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let config = Config::new(alpha).unwrap();
    let engine = Engine::new(alpha).unwrap();

    let conf_json = serde_json::to_string(&config).unwrap();
    assert_eq!(
        conf_json,
        format!("\"{}\"", std::str::from_utf8(alpha).unwrap())
    );

    let de_conf: Config = serde_json::from_str(&conf_json).unwrap();
    assert_eq!(de_conf.alphabet, config.alphabet);

    let eng_json = serde_json::to_string(&engine).unwrap();
    assert_eq!(eng_json, conf_json);

    let de_eng: Engine = serde_json::from_str(&eng_json).unwrap();
    assert_eq!(de_eng.config().alphabet, engine.config().alphabet);

    // Wrong-length alphabet.
    let res: Result<Config, _> = serde_json::from_str("\"abc\"");
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("expected exactly 58-byte alphabet")
    );

    // Duplicate characters, via both Config and Engine.
    let mut bad_alpha = *alpha;
    bad_alpha[57] = bad_alpha[0];
    let bad_alpha_str = std::str::from_utf8(&bad_alpha).unwrap();

    let res: Result<Config, _> = serde_json::from_str(&format!("\"{bad_alpha_str}\""));
    assert!(res.is_err());

    let res_eng: Result<Engine, _> = serde_json::from_str(&format!("\"{bad_alpha_str}\""));
    assert!(res_eng.is_err());
}
