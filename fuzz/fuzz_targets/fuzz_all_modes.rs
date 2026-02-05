#![no_main]
use libfuzzer_sys::fuzz_target;
use base58_turbo::*;

fuzz_target!(|data: &[u8]| {
    // 1. Enforce API contract (Max 1KB input)
    if data.is_empty() || data.len() > 512 {
        return;
    }

    // 2. Encode
    // Base58 expansion is ~1.37x. 1024 * 1.37 = 1403. Add safety padding.
    let enc_cap = 2048; 
    let mut encoded = Vec::<u8>::with_capacity(enc_cap);
    let enc_len;
    
    unsafe {
        enc_len = encode_slice_unsafe(data, encoded.as_mut_ptr());
        encoded.set_len(enc_len);
    };

    // 3. Decode (Round Trip)
    let mut decoded = vec![0u8; 2048]; 
    let dec_len;

    unsafe {
        // We expect success because we just encoded it ourselves
        dec_len = decode_slice_unsafe(&encoded, &mut decoded).expect("Valid input failed to decode");
    };
    
    decoded.truncate(dec_len);
    assert_eq!(decoded.as_slice(), data, "Round trip mismatch");
});
