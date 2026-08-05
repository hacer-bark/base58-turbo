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

#[test]
fn test_vs_base58_monero_random() {
    // Cover every block remainder (0..8) and a couple of full addresses.
    for len in (0..200).step_by(3) {
        let input = rng().random_iter::<u8>().take(len).collect::<Vec<_>>();

        let expected = base58::encode(&input).unwrap();
        let actual = xmr::encode(&input).expect("turbo xmr encode failed");
        assert_eq!(actual, expected, "xmr encoding mismatch at len {len}");

        let decoded = xmr::decode(&expected).expect("turbo xmr decode failed");
        assert_eq!(decoded, input, "xmr decoding mismatch at len {len}");
    }
}
