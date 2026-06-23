<div align="center">
  <h1>Base58 Turbo</h1>
  <p><strong>The absolute fastest general-purpose Base58 implementation.</strong></p>

  [![Crates.io](https://img.shields.io/crates/v/base58-turbo.svg?style=for-the-badge&color=fc8d62)](https://crates.io/crates/base58-turbo)
  [![License](https://img.shields.io/crates/l/base58-turbo.svg?style=for-the-badge&color=8da0cb)](https://crates.io/crates/base58-turbo)
  [![MIRI Verified](https://img.shields.io/github/actions/workflow/status/hacer-bark/base58-turbo/miri.yml?label=MIRI%20Verified&style=for-the-badge&color=66c2a5)](https://github.com/hacer-bark/base58-turbo/actions/workflows/miri.yml)
  [![Logic Tests](https://img.shields.io/github/actions/workflow/status/hacer-bark/base58-turbo/tests.yml?label=Logic%20Tests&style=for-the-badge&color=e78ac3)](https://github.com/hacer-bark/base58-turbo/actions/workflows/tests.yml)
</div>

<br/>

`base58-turbo` is a production-grade library engineered for **High Frequency Trading (HFT)**, **Blockchain Nodes**, and **Mission-Critical Servers** where CPU cycles are scarce and Undefined Behavior (UB) is unacceptable.

It aligns with **modern hardware reality** without sacrificing portability. By utilizing hyper-optimized scalar kernels, matrix multiplication arithmetic, and SWAR (SIMD Within A Register) zero handling, `base58-turbo` achieves blazing fast speeds **WITHOUT** requiring dedicated SIMD instructions (like AVX-512 or NEON).

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
**This crate requires Rust 1.86.0 or newer.**
We utilize modern Rust features to guarantee safety and performance.

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
*   **Matrix Multiplication Arithmetic:** To bypass the `u32` encoding limitation, we provide **hardcoded, pre-computed tables for common sizes** (25, 32, 64, and 69 bytes). This matrix-multiplication approach avoids expensive divisions entirely.
*   **2-Byte Lookup Tables (LUT)**: We emit two characters at a time during encoding (a single 16-bit write) using a pre-computed `58 * 58` table, drastically reducing branch mispredictions in the hot path.

### Benchmarks

#### Standard Base58

| Operation       | `base58-turbo`                  | `bs58`                        | Speedup    |
|-----------------|---------------------------------|-------------------------------|------------|
| Encode (32B)    | 43.3 ns<br>(705 MiB/s)          | 885 ns<br>(34.5 MiB/s)        | **20.5×**  |
| Decode (32B)    | 38.6 ns<br>(1.06 GiB/s)         | 313 ns<br>(134 MiB/s)         | **8.1×**   |
| Encode (64B)    | 111 ns<br>(550 MiB/s)           | 3.97 µs<br>(15.4 MiB/s)       | **35.8×**  |
| Decode (64B)    | 74.1 ns<br>(1.11 GiB/s)         | 1.25 µs<br>(67.1 MiB/s)       | **16.9×**  |
| Encode (128B)   | 550 ns<br>(222 MiB/s)           | 17.4 µs<br>(7.02 MiB/s)       | **31.6×**  |
| Decode (128B)   | 175 ns<br>(952 MiB/s)           | 5.09 µs<br>(32.8 MiB/s)       | **29.0×**  |

#### Monero Chunked Base58 (XMR)

| Operation       | `base58-turbo::xmr`             | `base58-monero`               | Speedup    |
|-----------------|---------------------------------|-------------------------------|------------|
| Encode (32B)    | 139 ns<br>(219 MiB/s)           | 358 ns<br>(85 MiB/s)          | **2.5×**   |
| Decode (32B)    | 157 ns<br>(260 MiB/s)           | 235 ns<br>(173 MiB/s)         | **1.5×**   |
| Encode (64B)    | 246 ns<br>(247 MiB/s)           | 762 ns<br>(80 MiB/s)          | **3.1×**   |
| Decode (64B)    | 278 ns<br>(300 MiB/s)           | 441 ns<br>(189 MiB/s)         | **1.5×**   |
| Encode (69B)*   | 259 ns<br>(253 MiB/s)           | 906 ns<br>(72 MiB/s)          | **3.5×**   |
| Decode (69B)*   | 343 ns<br>(263 MiB/s)           | 668 ns<br>(135 MiB/s)         | **1.9×**   |

*\*\ 69 bytes is the standard Monero address payload size.*

## Safety & Verification

Achieving maximum throughput must not cost memory safety. While we leverage `unsafe` intrinsics and pointer arithmetic, we guarantee the absence of bugs using a "Swiss Cheese" model of verification layers.

*   **MIRI Verified:** Validates that no Undefined Behavior (UB) occurs during execution.
*   **MSan Audited:** MemorySanitizer confirms no logic is ever performed on uninitialized memory.
*   **Fuzz Tested:** Continuous fuzzing with zero failures.

**[Read the Verification Audit](https://github.com/hacer-bark/base58-turbo/blob/main/docs/verification.md)**

## Feature Flags

| Feature | Default | Description |
| :--- | :---: | :--- |
| `serde` | ❌ | Enables `serde` serialization/deserialization for Config and Engine |
| `std` | ✅ | Enables `String` and `Vec` support. Disable for `no_std` |

## Documentation

*   [**Architecture & Design**](https://github.com/hacer-bark/base58-turbo/blob/main/docs/design.md) - Deep dive into our bignum optimizations.
*   [**Safety & Verification**](https://github.com/hacer-bark/base58-turbo/blob/main/docs/verification.md) - Proofs, MIRI logs, and audit strategy.

## License

This project licensed under either the [MIT License](https://github.com/hacer-bark/base58-turbo/blob/main/LICENSE-MIT) or the [Apache License, Version 2.0](https://github.com/hacer-bark/base58-turbo/blob/main/LICENSE-APACHE) at your option.
