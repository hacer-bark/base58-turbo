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

use base58_turbo::{BITCOIN, Config, Engine, Error, FLICKR, MONERO, RIPPLE};
use rand::{RngExt, rng};

/// Every predefined alphabet, plus a shuffled one so nothing can depend on the
/// standard ordering.
fn engines() -> Vec<Engine> {
    vec![
        BITCOIN,
        RIPPLE,
        FLICKR,
        Engine::new(b"zyxwvutsrqponmkjihgfedcba987654321ZYXWVUTSRQPNMLKJHGFEDCBA").unwrap(),
    ]
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

/// Schoolbook base-58 decode: repeated multiply-accumulate over base-256 bytes.
fn reference_decode(input: &[u8], config: &Config) -> Option<Vec<u8>> {
    let zero = config.alphabet()[0];
    let lz = input.iter().take_while(|&&b| b == zero).count();
    let mut num: Vec<u8> = vec![0];
    for &ch in &input[lz..] {
        let d = config.decode_map()[ch as usize];
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

// ======================================================================
// 1. Conformance Vectors (Bitcoin Alphabet)
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

// ======================================================================
// 2. Zero-Allocation API
// ======================================================================

#[test]
fn test_encode_into_exact_buffer_size() {
    let input = b"hello";
    let mut output = vec![0u8; BITCOIN.encoded_len(input.len())];

    let len = BITCOIN.encode_into(input, &mut output).unwrap();
    assert_eq!(&output[..len], b"Cn8eVZg");
}

#[test]
fn test_decode_into_buffer() {
    let mut output = [0u8; 100];
    let len = BITCOIN.decode_into("Cn8eVZg", &mut output).unwrap();
    assert_eq!(&output[..len], b"hello");
}

#[test]
fn test_empty_input() {
    assert_eq!(BITCOIN.encode(b"").unwrap(), "");
    assert_eq!(BITCOIN.decode("").unwrap(), b"");

    let mut out = [0u8; 10];
    assert_eq!(BITCOIN.encode_into(b"", &mut out).unwrap(), 0);
    assert_eq!(BITCOIN.decode_into("", &mut out).unwrap(), 0);
}

// ======================================================================
// 3. Errors & Formatting
// ======================================================================

#[test]
fn test_error_invalid_char() {
    // '0' is not in the Bitcoin alphabet.
    assert_eq!(BITCOIN.decode("Cn8eVZ0"), Err(Error::InvalidCharacter));
    // In the middle of a full 10-character decode chunk.
    assert_eq!(BITCOIN.decode("2222022222"), Err(Error::InvalidCharacter));
}

#[test]
fn test_error_buffer_too_small() {
    let mut out = [0u8; 5];
    assert_eq!(
        BITCOIN.encode_into(b"hello world", &mut out),
        Err(Error::BufferTooSmall)
    );
    // `decode_into` requires `decoded_len` (the input length) even when the
    // result would fit.
    assert_eq!(
        BITCOIN.decode_into("StV1DL6CwTryKyV", &mut out),
        Err(Error::BufferTooSmall)
    );
}

#[test]
fn test_error_input_too_big_into() {
    // The zero-allocation `_into` API keeps its stack-scratch size limit.
    let mut out = [0u8; 4096];
    assert_eq!(
        BITCOIN.encode_into([0u8; 1025], &mut out),
        Err(Error::InputTooBig)
    );
    assert_eq!(
        BITCOIN.decode_into("1".repeat(2049), &mut out),
        Err(Error::InputTooBig)
    );
}

#[test]
fn test_error_wrong_alphabet() {
    // 'a' repeats.
    let duplicate = *b"a123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxa";
    assert_eq!(Engine::new(&duplicate).unwrap_err(), Error::WrongAlphabet);

    let mut non_ascii = *b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    non_ascii[0] = 0xC3;
    assert_eq!(Config::new(&non_ascii).unwrap_err(), Error::WrongAlphabet);
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
        "alphabet has a duplicate or non-ASCII char"
    );
}

#[test]
fn test_debug_shows_only_the_alphabet() {
    let text = format!("{RIPPLE:?}");
    assert!(text.contains("rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz"));
    assert!(text.len() < 200, "{text}");
}

// ======================================================================
// 4. Kernel & Scratch-Class Boundaries
// ======================================================================
//
// The encoder dispatches the matrix kernel up to 56 bytes, the fixed 64-byte
// kernel for 57..=64, and general-kernel scratch classes up to 128, 320 and 1024
// bytes. The decoder's classes switch at 64 and 512 characters (about 46 and 375
// bytes). These lengths and their neighbours are where an off-by-one would show.

#[test]
fn test_roundtrip_boundary_lengths() {
    let lens = [
        0, 1, 3, 4, 15, 16, 23, 24, 25, 26, 31, 32, 33, 40, 41, 44, 45, 46, 48, 55, 56, 57, 60, 63,
        64, 65, 79, 80, 81, 95, 96, 127, 128, 129, 191, 192, 255, 256, 319, 320, 321, 375, 376,
        511, 512, 639, 640, 1023, 1024,
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
    for &zeros in &[0usize, 1, 7, 8, 9, 16, 24, 32, 64] {
        for &body in &[0usize, 1, 24, 25, 32, 64, 65, 128] {
            let mut input = vec![0u8; zeros];
            input.extend(rng().random_iter::<u8>().take(body));

            let encoded = BITCOIN.encode(&input).unwrap();
            let decoded = BITCOIN.decode(&encoded).unwrap();
            assert_eq!(decoded, input, "mismatch at zeros={zeros} body={body}");
        }
    }
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

    // A long zero prefix leaves the unbounded path a short payload, below the
    // general kernel's 64-byte block.
    for body in [1usize, 31, 32, 63, 64, 65] {
        let mut input = vec![0u8; 1100];
        input.extend(rng().random_iter::<u8>().take(body));
        let encoded = BITCOIN.encode(&input).unwrap();
        assert_eq!(BITCOIN.decode(&encoded).unwrap(), input, "body {body}");
    }

    let big_string = "1".repeat(5_000);
    assert_eq!(BITCOIN.decode(&big_string).unwrap(), vec![0u8; 5_000]);
}

// ======================================================================
// 5. Round Trips on Every Alphabet
// ======================================================================

#[test]
fn test_roundtrip_random_every_engine() {
    let lens = (0..=128).chain((129..900).step_by(13));
    for engine in engines() {
        for len in lens.clone() {
            let input = rng().random_iter::<u8>().take(len).collect::<Vec<_>>();
            let encoded = engine.encode(&input).unwrap();
            assert_eq!(engine.decode(&encoded).unwrap(), input, "len {len}");
        }
    }
}

// ======================================================================
// 6. Cross-Validation Against Other Base58 Crates
// ======================================================================

#[test]
fn test_vs_bs58_crate() {
    let cases = [
        (&BITCOIN, bs58::Alphabet::BITCOIN),
        (&RIPPLE, bs58::Alphabet::RIPPLE),
        (&MONERO, bs58::Alphabet::MONERO),
    ];
    for (engine, alphabet) in cases {
        for len in (0..900).step_by(7) {
            let input = rng().random_iter::<u8>().take(len).collect::<Vec<_>>();

            let expected = bs58::encode(&input).with_alphabet(alphabet).into_string();
            assert_eq!(engine.encode(&input).unwrap(), expected, "encode len {len}");
            assert_eq!(engine.decode(&expected).unwrap(), input, "decode len {len}");
        }
    }
}

#[test]
fn test_vs_base58_crate_bitcoin() {
    use base58::ToBase58;

    for len in (0..900).step_by(11) {
        let input = rng().random_iter::<u8>().take(len).collect::<Vec<_>>();

        let expected = input.to_base58();
        assert_eq!(
            BITCOIN.encode(&input).unwrap(),
            expected,
            "encode len {len}"
        );
        assert_eq!(
            BITCOIN.decode(&expected).unwrap(),
            input,
            "decode len {len}"
        );
    }
}

#[test]
fn test_vs_five8_crate_bitcoin() {
    use five8::{decode_32, decode_64, encode_32, encode_64};

    // five8 only supports fixed 32- and 64-byte payloads.
    for _ in 0..100 {
        let input: [u8; 32] = rng().random();
        let mut buf = [0u8; 44];
        let n = encode_32(&input, &mut buf) as usize;
        let expected = std::str::from_utf8(&buf[..n]).unwrap();
        assert_eq!(BITCOIN.encode(input).unwrap(), expected);
        let mut decoded = [0u8; 32];
        decode_32(expected, &mut decoded).unwrap();
        assert_eq!(BITCOIN.decode(expected).unwrap(), decoded);

        let input: [u8; 64] = rng().random();
        let mut buf = [0u8; 88];
        let n = encode_64(&input, &mut buf) as usize;
        let expected = std::str::from_utf8(&buf[..n]).unwrap();
        assert_eq!(BITCOIN.encode(input).unwrap(), expected);
        let mut decoded = [0u8; 64];
        decode_64(expected, &mut decoded).unwrap();
        assert_eq!(BITCOIN.decode(expected).unwrap(), decoded);
    }
}

// ======================================================================
// 7. Encoder vs. an Independent Reference
// ======================================================================

/// Worst-case shapes at every length the small kernels own: all-max bytes push
/// the column sums to their limit, a lone low bit gives the shortest output.
#[test]
fn encode_matches_reference_for_structured_inputs() {
    let alphabet = *BITCOIN.config().alphabet();
    for len in 1..=64usize {
        let shapes: [Vec<u8>; 5] = [
            rng().random_iter().take(len).collect(),
            vec![0xff; len],
            (0..len).map(|i| u8::from(i + 1 == len)).collect(),
            (0..len)
                .map(|i| if i < len / 2 { 0 } else { 0xff })
                .collect(),
            vec![0x80; len],
        ];
        for data in shapes {
            assert_eq!(
                BITCOIN.encode(&data).unwrap(),
                reference_base58(&data, alphabet),
                "len {len} data {data:?}"
            );
        }
    }
}

/// The two hottest widths, at every leading-zero run length (which changes both
/// the kernel and the output length) and in bulk.
#[test]
fn encode_matches_reference_for_32_and_64_byte_inputs() {
    let mut rng = rng();
    for engine in engines() {
        let alphabet = *engine.config().alphabet();
        for len in [32usize, 64] {
            for z in 0..=len {
                for _ in 0..16 {
                    let mut d: Vec<u8> = (0..len)
                        .map(|k| if k < z { 0 } else { rng.random() })
                        .collect();
                    if z < len && d[z] == 0 {
                        d[z] = 1; // keep the run length exactly z
                    }
                    let want = reference_base58(&d, alphabet);
                    assert_eq!(engine.encode(&d).unwrap(), want, "len={len} z={z}");
                    assert_eq!(engine.decode(&want).unwrap(), d, "len={len} z={z}");
                }
            }
            for _ in 0..10_000 {
                let d: Vec<u8> = (0..len).map(|_| rng.random()).collect();
                assert_eq!(engine.encode(&d).unwrap(), reference_base58(&d, alphabet));
            }
        }
    }
}

#[test]
fn batch_matches_single() {
    let mut rng = rng();
    for n in [0usize, 1, 2, 3, 7, 8, 17, 64] {
        let inputs: Vec<[u8; 32]> = (0..n)
            .map(|i| {
                let mut d: [u8; 32] = rng.random();
                d[..i % 4].fill(0);
                d
            })
            .collect();
        let mut out = vec![[0u8; 44]; n];
        let mut lens = vec![0u8; n];
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

/// The kernels must stay inside a destination sized to exactly `encoded_len`,
/// which is the contract `encode_into` enforces.
#[test]
fn encode_respects_exact_output_buffers() {
    let mut rng = rng();
    for len in [32usize, 64] {
        let cap = BITCOIN.encoded_len(len);
        for i in 0..2000 {
            // Mix in zero-prefixed inputs, which dispatch to a shorter kernel.
            let z = if i % 8 == 0 { i % len } else { 0 };
            let data: Vec<u8> = (0..len)
                .map(|k| if k < z { 0 } else { rng.random() })
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

// ======================================================================
// 8. Decoder vs. an Independent Reference
// ======================================================================
//
// Payloads up to 24 characters decode through a flat weight matrix, longer ones
// through the bignum Horner loop. Both are checked against a schoolbook
// reference that shares no code with either.

#[test]
fn decode_matches_reference_at_every_length() {
    let mut rng = rng();
    for engine in engines() {
        let cfg = engine.config();
        // Every payload length through the matrix path and well past it,
        // crossed with short leading-zero runs.
        for len in 0..=136usize {
            for zeros in 0..=len.min(4) {
                for trial in 0..12 {
                    let mut input = vec![cfg.alphabet()[0]; zeros];
                    for _ in zeros..len {
                        // Trials 0 and 1 pin the extremes: an all-max payload
                        // exercises the top carry out of the matrix, an all-min
                        // one exercises the shortest output.
                        let idx = match trial {
                            0 => 57,
                            1 => 1,
                            _ => rng.random_range(0..58),
                        };
                        input.push(cfg.alphabet()[idx]);
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
fn decode_rejects_invalid_characters_everywhere() {
    let mut rng = rng();
    for engine in engines() {
        let cfg = engine.config();
        // An invalid byte at every position, at every length: characters are
        // validated per group, and the partial groups sit at different offsets.
        for len in 1..=136usize {
            for pos in 0..len {
                let mut input: Vec<u8> = (0..len)
                    .map(|_| cfg.alphabet()[rng.random_range(0..58)])
                    .collect();
                for bad in [0x00u8, 0x2f, 0x30, 0x7f, 0x80, 0xff] {
                    if cfg.decode_map()[bad as usize] & 0x80 == 0 {
                        continue; // actually valid in this alphabet
                    }
                    input[pos] = bad;
                    assert_eq!(
                        engine.decode(&input),
                        Err(Error::InvalidCharacter),
                        "len={len} pos={pos} byte={bad:#04x}"
                    );
                }
            }
        }
    }
}

#[test]
fn decode_into_respects_exact_output_buffers() {
    let mut rng = rng();
    for engine in engines() {
        let cfg = engine.config();
        for len in 1..=136usize {
            let input: Vec<u8> = (0..len)
                .map(|_| cfg.alphabet()[rng.random_range(0..58)])
                .collect();
            let want = reference_decode(&input, cfg).unwrap();

            // `decode_into` contracts on `decoded_len`, an upper bound, so the
            // tightest legal buffer is that, with a guard byte past the end.
            let cap = engine.decoded_len(input.len());
            let mut buf = vec![0xAAu8; cap + 1];
            let n = engine.decode_into(&input, &mut buf[..cap]).unwrap();
            assert_eq!(&buf[..n], &want[..], "len={len}");
            assert_eq!(buf[cap], 0xAA, "overran at len={len}");

            // Anything smaller must be refused, not truncated.
            assert_eq!(
                engine.decode_into(&input, &mut buf[..cap - 1]),
                Err(Error::BufferTooSmall),
                "len={len}"
            );
        }
    }
}
