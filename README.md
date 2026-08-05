<div align="center">
  <h1>Base58 Turbo</h1>
  <p><strong>The absolute fastest general-purpose Base58 implementation.</strong></p>

  [![Crates.io](https://img.shields.io/crates/v/base58-turbo.svg?style=for-the-badge&color=fc8d62)](https://crates.io/crates/base58-turbo)
  [![License](https://img.shields.io/crates/l/base58-turbo.svg?style=for-the-badge&color=8da0cb)](https://crates.io/crates/base58-turbo)
  [![CI](https://img.shields.io/github/actions/workflow/status/hacer-bark/base58-turbo/tests.yml?label=CI&style=for-the-badge&color=e78ac3)](https://github.com/hacer-bark/base58-turbo/actions/workflows/tests.yml)
</div>

<br/>

`base58-turbo` is a production-grade library engineered for **High Frequency Trading (HFT)**, **Blockchain Nodes**, and **Mission-Critical Servers** where CPU cycles are scarce and Undefined Behavior is not an option — the crate is `#![forbid(unsafe_code)]`.

It aligns with **modern hardware reality** without sacrificing portability. By utilizing hyper-optimized scalar kernels, matrix multiplication arithmetic, and SWAR (SIMD Within A Register) zero handling, `base58-turbo` achieves blazing fast speeds **WITHOUT** requiring dedicated SIMD instructions (like AVX-512 or NEON) — and without a single `unsafe` block.

## Quick Start

### Installation

```toml
[dependencies]
base58-turbo = "0.1"
```

### Encoding

```rust
use base58_turbo::BITCOIN;

fn main() {
    let data = b"Hello World";

    // Returns Result<String, Error>
    let encoded = BITCOIN.encode(data).unwrap();

    assert_eq!(encoded, "JxF12TrwUP45BMd");
}
```

### Decoding

```rust
use base58_turbo::BITCOIN;

fn main() {
    let encoded = "JxF12TrwUP45BMd";

    // Returns Result<Vec<u8>, Error>
    let decoded = BITCOIN.decode(encoded).unwrap();

    assert_eq!(decoded, b"Hello World");
}
```

### Zero-Allocation (Stack)

For scenarios where heap allocation is too slow (e.g., hot paths), write directly to stack buffers:

```rust
use base58_turbo::BITCOIN;

fn main() {
    let input = b"Hello World";
    let mut output = [0u8; 64];

    // Returns Result<usize, Error>
    let len = BITCOIN.encode_into(input, &mut output).unwrap();
    let encoded = std::str::from_utf8(&output[..len]).unwrap();

    assert_eq!(encoded, "JxF12TrwUP45BMd");
}
```

### Native Monero Chunking (XMR)

`base58-turbo` includes native, highly-optimized support for Monero's specific block-chunked Base58 format. This processes payload strictly in 8-byte blocks padded to 11 characters. 

```rust
use base58_turbo::xmr;

fn main() {
    let payload = b"Hello World"; // Typically 69-byte addresses
    
    // Returns Result<String, Error>
    let encoded = xmr::encode(payload).unwrap();
    
    // Returns Result<Vec<u8>, Error>
    let decoded = xmr::decode(&encoded).unwrap();
}
```

Zero-allocation `xmr::encode_into` and `xmr::decode_into` APIs are also provided!

## Engines

Supports multiple Base58 alphabets:
- `BITCOIN`: Standard Bitcoin alphabet.
- `MONERO`: Monero alphabet.
- `RIPPLE`: Ripple alphabet.
- `FLICKR`: Flickr alphabet.
- `Engine::new(&[u8; 58])`: Custom alphabets.

## Compatibility & Stability

### Minimum Supported Rust Version (MSRV)
**This crate requires Rust 1.87.0 or newer.**

### Public API Stability
The public API (traits, structs, and error types) is considered **Stable**.
*   We adhere to **Semantic Versioning**.
*   The current API surface will remain valid and backward-compatible throughout the `0.1.x` lifecycle.

## Performance & Architecture

`base58-turbo` is **the fastest Base58 implementation without SIMD**, and it frequently **beats heavily SIMD-optimized implementations** (like `five8`) in head-to-head benchmarks. We achieve this by fully respecting how modern CPUs actually execute instructions.

### Why Are We Faster Than SIMD?
We bypass hardware-specific SIMD registers entirely and extract maximum performance directly from standard ALUs using intelligent byte-chunking:

*   **Native `u64` Decoding (Shrinking):** When decoding, Base58 strings mathematically shrink into bytes. Because there is no data expansion, we can natively process chunks in `u64` registers. This essentially doubles our throughput natively, allowing our general loop to hit the physical limits of hardware without any fancy SIMD or pre-computed paths.
*   **`u32` -> `u64` Expansion (Encoding):** When encoding, bytes expand into Base58. If we tried to process `u64` chunks, the multiplication step would overflow into `u128`, which absolutely destroys CPU performance. Instead, we use `u32` chunks that expand safely into `u64` for heavy arithmetic.
*   **Matrix Multiplication Arithmetic:** To bypass the `u32` encoding limitation, we provide **hardcoded, pre-computed tables for common sizes** (25, 32, and 64 bytes), and fold arbitrarily long input in whole 64-byte blocks against the same table. This matrix-multiplication approach avoids expensive divisions entirely.
*   **2-Byte Lookup Tables (LUT)**: We emit two characters at a time during encoding (a single 16-bit write) using a pre-computed `58 * 58` table, drastically reducing branch mispredictions in the hot path.
*   **No `unsafe`:** every kernel is shaped — fixed-size arrays, `chunks_exact`, slice patterns — so LLVM can prove the bounds checks are redundant and remove them on its own, instead of a human asserting it with `unsafe`.

### Benchmarks

Numbers pending re-measurement against the current `#![forbid(unsafe_code)]` implementation. Run `cargo bench` locally in the meantime — `benches/encoding_bench.rs` compares against `bs58`, `base58`, `five8`, and `base58-monero`.

## Safety & Verification

Achieving maximum throughput must not cost memory safety, so the crate carries `#![forbid(unsafe_code)]` at the top of `lib.rs` — the compiler rejects any `unsafe` block anywhere in the crate, including in future contributions. There is no pointer arithmetic and no manually-asserted invariant to audit; every bounds check that matters to performance is elided by the compiler because the code is shaped to make that provable, not because it was told to trust itself.

That leaves ordinary logic bugs as the only class of failure worth guarding against, which is what the test suite is for: exact conformance vectors, every kernel-dispatch and scratch-buffer boundary, and randomized cross-validation against `bs58`, `base58`, `five8`, and `base58-monero` on every push. See [.github/workflows/tests.yml](.github/workflows/tests.yml) for exactly what runs.

## Feature Flags

| Feature | Default | Description |
| :--- | :---: | :--- |
| `serde` | | Enables `serde` serialization/deserialization for Config and Engine |
| `std` | on | Enables `String` and `Vec` support. Disable for `no_std` |

## License

Licensed under either of

- [Apache License, Version 2.0](https://github.com/hacer-bark/base58-turbo/blob/main/LICENSE-APACHE)
- [MIT license](https://github.com/hacer-bark/base58-turbo/blob/main/LICENSE-MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this crate, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
