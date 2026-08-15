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

// ======================================================================
// AVX2 parity (feature `unsafe-simd`)
// ======================================================================

/// The AVX2 kernel must agree with the scalar one on every 32-byte input,
/// including the zero-prefixed shapes that change the output length.
#[test]
fn simd_matches_scalar_for_32_byte_inputs() {
    fn xs(s: &mut u64) -> u64 {
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    }
    let mut s = 0x243F_6A88_85A3_08D3u64;
    let engines = [BITCOIN, RIPPLE, FLICKR];

    for engine in engines {
        // Every leading-zero-run length, including all-zero.
        for z in 0..=32usize {
            for _ in 0..64 {
                let mut d = [0u8; 32];
                for k in z..32 {
                    d[k] = (xs(&mut s) >> 24) as u8;
                }
                if z < 32 && d[z] == 0 {
                    d[z] = 1; // keep the run length exactly z
                }
                let want = reference_base58(&d, engine.config().alphabet);
                assert_eq!(engine.encode(&d).unwrap(), want, "z={z} data={d:?}");
                assert_eq!(engine.decode(&want).unwrap(), d, "roundtrip z={z}");
            }
        }
        // Boundary values.
        for d in [[0u8; 32], [0xffu8; 32], {
            let mut v = [0u8; 32];
            v[31] = 1;
            v
        }] {
            let want = reference_base58(&d, engine.config().alphabet);
            assert_eq!(engine.encode(&d).unwrap(), want, "boundary {d:?}");
        }
        // Bulk random.
        for _ in 0..20_000 {
            let d: [u8; 32] = core::array::from_fn(|_| (xs(&mut s) >> 24) as u8);
            assert_eq!(
                engine.encode(&d).unwrap(),
                reference_base58(&d, engine.config().alphabet)
            );
        }
    }
}

#[test]
fn simd_batch_matches_single() {
    fn xs(s: &mut u64) -> u64 {
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    }
    let mut s = 0xDEAD_BEEF_1234_5678u64;
    // Sizes around the 3-wide stride so the scalar remainder path is covered.
    for n in [0usize, 1, 2, 3, 4, 5, 6, 7, 8, 17, 64] {
        let inputs: Vec<[u8; 32]> = (0..n)
            .map(|i| {
                core::array::from_fn(|k| {
                    if i % 5 == 0 && k < i % 4 {
                        0
                    } else {
                        (xs(&mut s) >> 24) as u8
                    }
                })
            })
            .collect();
        let mut out = vec![[0u8; 44]; n.max(1)];
        let mut lens = vec![0u8; n.max(1)];
        BITCOIN
            .encode_32_batch(&inputs, &mut out, &mut lens)
            .unwrap();
        for (i, input) in inputs.iter().enumerate() {
            let got = std::str::from_utf8(&out[i][..lens[i] as usize]).unwrap();
            assert_eq!(got, BITCOIN.encode(input).unwrap(), "n={n} i={i}");
        }
    }
    // Undersized buffers are rejected, not written past.
    let inputs = [[1u8; 32]; 4];
    let mut out = [[0u8; 44]; 2];
    let mut lens = [0u8; 4];
    assert_eq!(
        BITCOIN.encode_32_batch(&inputs, &mut out, &mut lens),
        Err(Error::BufferTooSmall)
    );
}

/// The 64-byte AVX2 kernel must agree with the scalar one everywhere too.
#[test]
fn simd_matches_scalar_for_64_byte_inputs() {
    fn xs(s: &mut u64) -> u64 {
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    }
    let mut s = 0x0BAD_C0DE_F00D_1337u64;
    for engine in [BITCOIN, RIPPLE, FLICKR] {
        for z in 0..=64usize {
            for _ in 0..24 {
                let mut d = [0u8; 64];
                for k in z..64 {
                    d[k] = (xs(&mut s) >> 24) as u8;
                }
                if z < 64 && d[z] == 0 {
                    d[z] = 1;
                }
                let want = reference_base58(&d, engine.config().alphabet);
                assert_eq!(engine.encode(&d).unwrap(), want, "z={z}");
                assert_eq!(engine.decode(&want).unwrap(), d, "roundtrip z={z}");
            }
        }
        for d in [[0u8; 64], [0xffu8; 64], {
            let mut v = [0u8; 64];
            v[63] = 1;
            v
        }] {
            assert_eq!(
                engine.encode(&d).unwrap(),
                reference_base58(&d, engine.config().alphabet)
            );
        }
        for _ in 0..20_000 {
            let d: [u8; 64] = core::array::from_fn(|_| (xs(&mut s) >> 24) as u8);
            assert_eq!(
                engine.encode(&d).unwrap(),
                reference_base58(&d, engine.config().alphabet)
            );
        }
    }
}

/// The wide SIMD stores must stay inside a destination sized to exactly
/// `encoded_len`, which is the contract `encode_into` enforces.
#[test]
fn simd_respects_exact_output_buffers() {
    fn xs(s: &mut u64) -> u64 {
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    }
    let mut s = 0xFEED_FACE_CAFE_D00Du64;
    for len in [32usize, 64] {
        let cap = BITCOIN.encoded_len(len);
        for i in 0..4000 {
            // Mix in zero-prefixed inputs, which take the narrow store path.
            let z = if i % 8 == 0 { i % len } else { 0 };
            let data: Vec<u8> = (0..len)
                .map(|k| if k < z { 0 } else { (xs(&mut s) >> 24) as u8 })
                .collect();
            let mut buf = vec![0xAAu8; cap + 8];
            let n = BITCOIN.encode_into(&data, &mut buf[..cap]).unwrap();
            assert_eq!(
                std::str::from_utf8(&buf[..n]).unwrap(),
                BITCOIN.encode(&data).unwrap(),
                "len={len} z={z}"
            );
            assert!(
                buf[cap..].iter().all(|&b| b == 0xAA),
                "wrote past a buffer of exactly encoded_len ({cap}) for len={len} z={z}"
            );
        }
    }
}

/// Independent schoolbook base-256 -> base-58 conversion.
fn reference_base58(input: &[u8], alphabet: [u8; 58]) -> String {
    let zeros = input.iter().take_while(|&&b| b == 0).count();
    let mut digits: Vec<u8> = Vec::new();
    for &byte in &input[zeros..] {
        let mut carry = u32::from(byte);
        for d in digits.iter_mut() {
            let v = u32::from(*d) * 256 + carry;
            *d = (v % 58) as u8;
            carry = v / 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut out = String::new();
    for _ in 0..zeros {
        out.push(alphabet[0] as char);
    }
    for &d in digits.iter().rev() {
        out.push(alphabet[d as usize] as char);
    }
    out
}

// ----------------------------------------------------------------------
// Decoder: differential tests for the weight-matrix paths
// ----------------------------------------------------------------------
//
// Decoding short payloads goes through a flat weight matrix rather than the
// bignum Horner loop, and with `unsafe-simd` a further AVX2 kernel handles
// 32..=128 characters. Both are checked here against a schoolbook reference
// that shares no code with either.

/// Schoolbook base-58 decode: repeated multiply-accumulate over base-256 bytes.
fn reference_decode(input: &[u8], config: &Config) -> Option<Vec<u8>> {
    let zero = config.alphabet[0];
    let lz = input.iter().take_while(|&&b| b == zero).count();
    let mut num: Vec<u8> = vec![0];
    for &ch in &input[lz..] {
        let d = config.decode_map[ch as usize];
        if d & 0x80 != 0 {
            return None;
        }
        let mut carry = u32::from(d);
        for byte in num.iter_mut().rev() {
            let v = u32::from(*byte) * 58 + carry;
            *byte = v as u8;
            carry = v >> 8;
        }
        while carry > 0 {
            num.insert(0, carry as u8);
            carry >>= 8;
        }
    }
    let start = num.iter().position(|&b| b != 0).unwrap_or(num.len());
    let mut out = vec![0u8; lz];
    out.extend_from_slice(&num[start..]);
    Some(out)
}

fn decode_test_engines() -> Vec<Engine> {
    vec![
        BITCOIN,
        RIPPLE,
        FLICKR,
        // A shuffled alphabet, so nothing can depend on the standard ordering.
        Engine::new(b"zyxwvutsrqponmkjihgfedcba987654321ZYXWVUTSRQPNMLKJHGFEDCBA").unwrap(),
    ]
}

#[test]
fn decode_matrix_matches_reference_exhaustively() {
    fn xs(s: &mut u64) -> u64 {
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    }
    let mut s = 0x1234_5678_9abc_def0u64;

    for engine in decode_test_engines() {
        let cfg = engine.config();
        // Every payload length across both matrix paths and past their ceiling,
        // crossed with every leading-zero run length.
        for len in 0..=136usize {
            for zeros in 0..=len.min(4) {
                for trial in 0..12 {
                    let mut input = vec![cfg.alphabet[0]; zeros];
                    for _ in zeros..len {
                        // Trials 0 and 1 pin the extremes: an all-max payload
                        // exercises the top carry out of the matrix, an all-min
                        // one exercises the shortest output.
                        let idx = match trial {
                            0 => 57,
                            1 => 1,
                            _ => ((xs(&mut s) >> 33) % 58) as usize,
                        };
                        input.push(cfg.alphabet[idx]);
                    }
                    let want = reference_decode(&input, cfg).unwrap();
                    assert_eq!(
                        engine.decode(&input).unwrap(),
                        want,
                        "len={len} zeros={zeros} trial={trial}"
                    );
                }
            }
        }
    }
}

#[test]
fn decode_matrix_rejects_invalid_characters() {
    for engine in decode_test_engines() {
        let cfg = engine.config();
        // An invalid byte at every position, at every length: the vector path
        // validates 32 characters at a time and the scalar head separately, so
        // position matters.
        for len in 1..=136usize {
            for pos in 0..len {
                let mut input = vec![cfg.alphabet[7]; len];
                for bad in [0x00u8, 0x2f, 0x30, 0x7f, 0x80, 0xff] {
                    if cfg.decode_map[bad as usize] & 0x80 == 0 {
                        continue; // actually valid in this alphabet
                    }
                    input[pos] = bad;
                    assert_eq!(
                        engine.decode(&input).unwrap_err(),
                        Error::InvalidCharacter,
                        "len={len} pos={pos} byte={bad:#04x}"
                    );
                }
            }
        }
    }
}

#[test]
fn decode_matrix_respects_exact_output_buffers() {
    for engine in decode_test_engines() {
        let cfg = engine.config();
        for len in 1..=136usize {
            let input: Vec<u8> = (0..len).map(|i| cfg.alphabet[(i * 7 + 3) % 58]).collect();
            let want = reference_decode(&input, cfg).unwrap();

            // `decode_into` contracts on `decoded_len`, an upper bound, so the
            // tightest legal buffer is that — with a guard byte past the end.
            let cap = engine.decoded_len(input.len());
            let mut buf = vec![0xAAu8; cap + 1];
            let n = engine.decode_into(&input, &mut buf[..cap]).unwrap();
            assert_eq!(&buf[..n], &want[..], "len={len}");
            assert_eq!(buf[cap], 0xAA, "overran at len={len}");

            // Anything smaller must be refused, not truncated.
            if cap > 0 {
                assert_eq!(
                    engine.decode_into(&input, &mut buf[..cap - 1]),
                    Err(Error::BufferTooSmall),
                    "len={len}"
                );
            }

            // The kernel itself takes a buffer sized to the *actual* output.
            // This is what catches a one-byte overrun from the constant-size
            // copies in the emit tail, which `decoded_len`'s slack would hide.
            let mut tight = vec![0xAAu8; want.len() + 1];
            let n = base58_turbo::decode::decode_slice(&input, &mut tight[..want.len()], cfg)
                .unwrap();
            assert_eq!(&tight[..n], &want[..], "tight len={len}");
            assert_eq!(tight[want.len()], 0xAA, "tight overran at len={len}");
        }
    }
}

#[test]
fn decode_matrix_round_trips_random_payloads() {
    fn xs(s: &mut u64) -> u64 {
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    }
    let mut s = 0xF00D_BA5E_0BAD_C0DEu64;
    for engine in decode_test_engines() {
        for len in 0..=100usize {
            for _ in 0..40 {
                let data: Vec<u8> = (0..len).map(|_| (xs(&mut s) >> 33) as u8).collect();
                let encoded = engine.encode(&data).unwrap();
                assert_eq!(engine.decode(&encoded).unwrap(), data, "len={len}");
            }
        }
    }
}
