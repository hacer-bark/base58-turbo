use base58_monero::base58;
use base58_turbo::xmr;

#[test]
fn test_xmr_chunking() {
    let b1 = [1u8, 0, 0, 0, 0, 0, 0, 0];
    let e1 = base58::encode(&b1).unwrap();
    let turbo_e1 = xmr::encode(&b1).unwrap();
    assert_eq!(turbo_e1, e1);
    assert_eq!(xmr::decode(&turbo_e1).unwrap(), b1);

    let b2 = [0u8, 0, 0, 0, 0, 0, 0, 1];
    let e2 = base58::encode(&b2).unwrap();
    let turbo_e2 = xmr::encode(&b2).unwrap();
    assert_eq!(turbo_e2, e2);
    assert_eq!(xmr::decode(&turbo_e2).unwrap(), b2);

    let b3 = [255u8, 255, 255];
    let e3 = base58::encode(&b3).unwrap();
    let turbo_e3 = xmr::encode(&b3).unwrap();
    assert_eq!(turbo_e3, e3);
    assert_eq!(xmr::decode(&turbo_e3).unwrap(), b3);
}

#[test]
#[cfg(not(miri))]
fn test_vs_base58_monero_random() {
    let mut seed: u64 = rand::random();
    fn next_byte(seed: &mut u64) -> u8 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (*seed >> 56) as u8
    }

    // Try various lengths to hit all block remainders and full 69-byte addresses
    for len in (0..200).step_by(3) {
        let mut input = Vec::with_capacity(len);
        for _ in 0..len {
            input.push(next_byte(&mut seed));
        }

        // 1. Encode with trusted `base58-monero` crate
        let expected_str = base58::encode(&input).unwrap();

        // 2. Encode with `xmr::encode`
        let actual_str = xmr::encode(&input).expect("Turbo XMR encode failed");
        assert_eq!(actual_str, expected_str, "XMR encoding mismatch at len {}", len);

        // 3. Decode with `xmr::decode`
        let decoded_bytes = xmr::decode(&expected_str).expect("Turbo XMR decode failed");
        assert_eq!(decoded_bytes, input, "XMR decoding mismatch at len {}", len);
    }
}
