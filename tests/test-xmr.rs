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

use base58_monero::base58;
use base58_turbo::xmr;
use rand::{RngExt, rng};

#[test]
fn test_xmr_chunking() {
    // Monero's variant encodes fixed 8-byte blocks (with an 11-byte tail block),
    // so these exercise the chunk boundary directly.
    let cases: &[&[u8]] = &[
        &[1, 0, 0, 0, 0, 0, 0, 0],
        &[0, 0, 0, 0, 0, 0, 0, 1],
        &[255, 255, 255],
    ];

    for input in cases {
        let expected = base58::encode(input).unwrap();
        let actual = xmr::encode(input).unwrap();
        assert_eq!(actual, expected, "mismatch for {input:?}");
        assert_eq!(xmr::decode(&actual).unwrap(), *input);
    }
}

/// Every length from 0 to 128 bytes covers each block remainder many times over.
#[test]
fn test_vs_base58_monero_random() {
    for len in 0..=128 {
        let input = rng().random_iter::<u8>().take(len).collect::<Vec<_>>();

        let expected = base58::encode(&input).unwrap();
        let actual = xmr::encode(&input).expect("turbo xmr encode failed");
        assert_eq!(actual, expected, "xmr encoding mismatch at len {len}");

        let decoded = xmr::decode(&expected).expect("turbo xmr decode failed");
        assert_eq!(decoded, input, "xmr decoding mismatch at len {len}");
    }
}

/// Random strings, most of them not valid encodings: blocks that overflow their
/// byte width, out-of-alphabet characters, and invalid lengths must all be
/// rejected exactly when `base58-monero` rejects them.
#[test]
fn test_decode_arbitrary_strings_matches_base58_monero() {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut rng = rng();
    for _ in 0..20_000 {
        let len = rng.random_range(0..=34);
        let high = rng.random_range(1..=ALPHABET.len());
        let text: String = (0..len)
            .map(|_| {
                if rng.random_ratio(1, 200) {
                    '0'
                } else {
                    char::from(ALPHABET[rng.random_range(0..high)])
                }
            })
            .collect();

        let expected = base58::decode(&text).ok();
        assert_eq!(xmr::decode(&text).ok(), expected, "input {text:?}");
    }
}
