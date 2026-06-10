# ❓ Frequently Asked Questions

## 🛡️ Safety & Verification

### Q: The crate uses `unsafe`. How can you claim it is safe?
**A:** We distinguish between "Safe Rust" (compiler-checked) and "Memory Safe" (verified via strict checking).
While we use `unsafe` pointers and intrinsics to achieve raw speed, we rely on a **Strict Verification Pipeline**. We have verified via continuous Fuzzing, MemorySanitizer, and MIRI that for the verified paths, **no possible input** (0..1024 bytes for encoding, 0..2048 bytes for decoding) can trigger a buffer overflow, segfault, or panic via the public API.

**[Read the Verification Report](./verification.md)**

### Q: Can I crash the library by passing garbage data?
**A:** **No.**
The decoder is resilient. If you pass invalid Base58 strings or malicious payloads, the library simply returns a `Result::Err`. It will **never** panic or cause Undefined Behavior (UB) as long as you use the Safe API.

### Q: Is there a limit to input size?
**A:** **Yes, 1024 bytes for encoding and 2048 bytes for decoding.**
To ensure we can use stack-allocated bignum buffers for maximum performance (no heap allocation), we currently limit inputs to 1024 bytes for encoding and 2048 bytes for decoding. This covers almost all use cases for Base58, including public keys, addresses, and IPFS CIDs.

### Q: Does this work on ARM (Apple Silicon / Raspberry Pi)?
**A:** **Yes.**
The library is written in portable Rust with optimized scalar kernels. It runs at full speed on ARM, leveraging 64-bit and 128-bit arithmetic for fast bignum operations.

## ⚡ Performance & Usage

### Q: Why is Base58 slower than Base64?
**A:** Base64 is a simple bit-shifting operation because 64 is a power of 2. Base58 requires arbitrary-precision division by 58, which is mathematically more complex. `base58-turbo` minimizes this overhead, but it will always be slower than Base64.

### Q: How do I calculate the buffer size for `encode_into`?
**A:** Use the helper functions:
```rust
let needed = BITCOIN.encoded_len(input.len());
let mut buf = vec![0u8; needed];
```

### Q: Does this work on `no_std` / Embedded systems?
**A:** **Yes.**
Simply disable the default `std` feature in your `Cargo.toml`. The library does not require a heap allocator if you use the `_into` (slice-based) APIs.
```toml
[dependencies]
base58-turbo = { version = "0.1", default-features = false }
```

## 🔌 Compatibility & Ecosystem

### Q: Is the output compatible with the `bs58` crate?
**A:** **Yes.**
We fully conform to the standard Base58 alphabets (Bitcoin, Monero, Ripple, Flickr). You can swap `base58-turbo` into any project using `bs58` without breaking data compatibility.

### Q: Why should I use this over `bs58`?
**A:** **Speed.**
If you are processing thousands of addresses or signatures per second (e.g., in a high-load blockchain indexer), `base58-turbo` will significantly reduce your CPU usage.

### Q: How can I trust this code?
**A:** **Trust the math, not the author.**
1.  Check the **[GitHub Actions](https://github.com/hacer-bark/base58-turbo/actions)** to see the live MIRI, Fuzzing, and MSan logs.
2.  Inspect the **GPG Signatures** on our commits.
3.  Read the **[Verification Report](./verification.md)** to understand our audit methodology.
