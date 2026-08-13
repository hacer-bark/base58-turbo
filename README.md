<div align="center">
  <h1>Base58 Turbo</h1>
  <p><strong>The absolute fastest general-purpose Base58 implementation.</strong></p>

  [![Crates.io](https://img.shields.io/crates/v/base58-turbo.svg?style=for-the-badge&color=fc8d62)](https://crates.io/crates/base58-turbo)
  [![License](https://img.shields.io/crates/l/base58-turbo.svg?style=for-the-badge&color=8da0cb)](https://crates.io/crates/base58-turbo)
  [![CI](https://img.shields.io/github/actions/workflow/status/hacer-bark/base58-turbo/tests.yml?label=CI&style=for-the-badge&color=e78ac3)](https://github.com/hacer-bark/base58-turbo/actions/workflows/tests.yml)
  [![Unsafe Forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg?style=for-the-badge&color=66c2a5)](https://github.com/rust-secure-code/safety-dance/)
</div>

<br/>

`base58-turbo` is a production-grade library engineered for **High Frequency Trading (HFT)**, **Blockchain Nodes**, and **Mission-Critical Servers** where CPU cycles are scarce and Undefined Behavior is not an option — the crate is `#![forbid(unsafe_code)]`.

It aligns with **modern hardware reality** without sacrificing portability. By utilizing hyper-optimized scalar kernels, matrix multiplication arithmetic, and SWAR (SIMD Within A Register) zero handling, `base58-turbo` achieves blazing fast speeds **WITHOUT** requiring dedicated SIMD instructions (like AVX-512 or NEON) — and without a single `unsafe` block.

<img alt="Base58 throughput by payload size — base58-turbo leads bs58, base58, five8, and base58-monero at every size" src="benches/results/throughput.png">

## Quick Start

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
*   The current API surface will remain valid and backward-compatible throughout the `0.2.x` lifecycle.

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

The chart at the top of this README is generated straight from a `cargo bench` run — same numbers, no cherry-picking. It compares `base58-turbo` against `bs58`, `base58`, `five8`, and `base58-monero` (`benches/encoding_bench.rs`) across payload sizes from 16 to 128 bytes. `five8` only ships fixed-width 32/64-byte encoders.

Measured on an AWS `c7i.large` (Ubuntu), a fresh checkout, and nothing else running:

```bash
sudo apt update && sudo apt install -y build-essential git
curl --proto '=https' --tlsv1.3 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

git clone https://github.com/hacer-bark/base58-turbo
cd base58-turbo
BENCH_TARGET=all cargo bench
```

The results are reproducible — re-run the command above and expect the same shape of numbers, modulo whatever the host machine's doing.

To regenerate the chart from a fresh run:

```bash
BENCH_TARGET=all cargo bench 2>&1 | tee benches/results/raw.txt
python3 benches/scripts/plot_bench.py benches/results/raw.txt
```

<details>
<summary>Raw <code>cargo bench</code> output</summary>

```
Benchmarking Base58_Performances/Encode/Turbo/16
  time:   [43.612 ns 43.638 ns 43.668 ns]
  thrpt:  [349.43 MiB/s 349.67 MiB/s 349.88 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/16
  time:   [243.64 ns 244.13 ns 244.79 ns]
  thrpt:  [62.335 MiB/s 62.504 MiB/s 62.628 MiB/s]

Benchmarking Base58_Performances/Encode/base58/16
  time:   [271.92 ns 274.57 ns 278.35 ns]
  thrpt:  [54.820 MiB/s 55.574 MiB/s 56.116 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo_XMR/16
  time:   [51.832 ns 51.886 ns 51.974 ns]
  thrpt:  [293.58 MiB/s 294.08 MiB/s 294.39 MiB/s]

Benchmarking Base58_Performances/Encode/base58_monero/16
  time:   [158.58 ns 158.76 ns 158.93 ns]
  thrpt:  [96.009 MiB/s 96.111 MiB/s 96.223 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/16
  time:   [29.228 ns 29.278 ns 29.329 ns]
  thrpt:  [715.36 MiB/s 716.60 MiB/s 717.83 MiB/s]

Benchmarking Base58_Performances/Decode/bs58/16
  time:   [95.380 ns 96.780 ns 99.339 ns]
  thrpt:  [211.20 MiB/s 216.79 MiB/s 219.97 MiB/s]

Benchmarking Base58_Performances/Decode/base58/16
  time:   [471.29 ns 471.56 ns 471.83 ns]
  thrpt:  [44.467 MiB/s 44.492 MiB/s 44.518 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo_XMR/16
  time:   [50.992 ns 51.028 ns 51.063 ns]
  thrpt:  [410.88 MiB/s 411.16 MiB/s 411.45 MiB/s]

Benchmarking Base58_Performances/Decode/base58_monero/16
  time:   [68.881 ns 68.927 ns 68.986 ns]
  thrpt:  [304.13 MiB/s 304.39 MiB/s 304.59 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo/32
  time:   [45.105 ns 45.124 ns 45.144 ns]
  thrpt:  [676.01 MiB/s 676.31 MiB/s 676.59 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/32
  time:   [875.96 ns 876.38 ns 876.82 ns]
  thrpt:  [34.805 MiB/s 34.822 MiB/s 34.839 MiB/s]

Benchmarking Base58_Performances/Encode/base58/32
  time:   [826.81 ns 827.53 ns 828.13 ns]
  thrpt:  [36.851 MiB/s 36.878 MiB/s 36.910 MiB/s]

Benchmarking Base58_Performances/Encode/five8/32
  time:   [65.479 ns 65.510 ns 65.542 ns]
  thrpt:  [465.62 MiB/s 465.85 MiB/s 466.07 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo_XMR/32
  time:   [82.971 ns 83.047 ns 83.135 ns]
  thrpt:  [367.08 MiB/s 367.47 MiB/s 367.81 MiB/s]

Benchmarking Base58_Performances/Encode/base58_monero/32
  time:   [263.55 ns 264.19 ns 264.63 ns]
  thrpt:  [115.32 MiB/s 115.51 MiB/s 115.79 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/32
  time:   [41.509 ns 41.632 ns 41.742 ns]
  thrpt:  [982.42 MiB/s 985.01 MiB/s 987.93 MiB/s]

Benchmarking Base58_Performances/Decode/bs58/32
  time:   [308.77 ns 309.17 ns 309.47 ns]
  thrpt:  [132.51 MiB/s 132.64 MiB/s 132.81 MiB/s]

Benchmarking Base58_Performances/Decode/base58/32
  time:   [853.71 ns 854.31 ns 854.92 ns]
  thrpt:  [47.967 MiB/s 48.002 MiB/s 48.035 MiB/s]

Benchmarking Base58_Performances/Decode/five8/32
  time:   [64.226 ns 64.278 ns 64.343 ns]
  thrpt:  [637.34 MiB/s 637.98 MiB/s 638.49 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo_XMR/32
  time:   [93.108 ns 93.185 ns 93.262 ns]
  thrpt:  [439.71 MiB/s 440.07 MiB/s 440.43 MiB/s]

Benchmarking Base58_Performances/Decode/base58_monero/32
  time:   [157.10 ns 157.22 ns 157.36 ns]
  thrpt:  [260.59 MiB/s 260.83 MiB/s 261.03 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo/48
  time:   [98.517 ns 98.570 ns 98.624 ns]
  thrpt:  [464.15 MiB/s 464.40 MiB/s 464.65 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/48
  time:   [2.1636 µs 2.1644 µs 2.1652 µs]
  thrpt:  [21.142 MiB/s 21.150 MiB/s 21.158 MiB/s]

Benchmarking Base58_Performances/Encode/base58/48
  time:   [1.8338 µs 1.8360 µs 1.8383 µs]
  thrpt:  [24.902 MiB/s 24.932 MiB/s 24.962 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo_XMR/48
  time:   [116.19 ns 116.31 ns 116.48 ns]
  thrpt:  [393.01 MiB/s 393.57 MiB/s 393.98 MiB/s]

Benchmarking Base58_Performances/Encode/base58_monero/48
  time:   [420.84 ns 421.64 ns 422.19 ns]
  thrpt:  [108.43 MiB/s 108.57 MiB/s 108.77 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/48
  time:   [54.451 ns 54.501 ns 54.551 ns]
  thrpt:  [1.1268 GiB/s 1.1278 GiB/s 1.1289 GiB/s]

Benchmarking Base58_Performances/Decode/bs58/48
  time:   [710.45 ns 710.98 ns 711.49 ns]
  thrpt:  [88.466 MiB/s 88.529 MiB/s 88.595 MiB/s]

Benchmarking Base58_Performances/Decode/base58/48
  time:   [1.2808 µs 1.2815 µs 1.2822 µs]
  thrpt:  [49.088 MiB/s 49.117 MiB/s 49.144 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo_XMR/48
  time:   [135.03 ns 135.29 ns 135.66 ns]
  thrpt:  [463.97 MiB/s 465.25 MiB/s 466.15 MiB/s]

Benchmarking Base58_Performances/Decode/base58_monero/48
  time:   [243.40 ns 243.49 ns 243.59 ns]
  thrpt:  [258.39 MiB/s 258.50 MiB/s 258.60 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo/64
  time:   [138.13 ns 138.22 ns 138.34 ns]
  thrpt:  [441.21 MiB/s 441.58 MiB/s 441.87 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/64
  time:   [4.0464 µs 4.0488 µs 4.0512 µs]
  thrpt:  [15.066 MiB/s 15.075 MiB/s 15.084 MiB/s]

Benchmarking Base58_Performances/Encode/base58/64
  time:   [3.3724 µs 3.3763 µs 3.3798 µs]
  thrpt:  [18.059 MiB/s 18.078 MiB/s 18.098 MiB/s]

Benchmarking Base58_Performances/Encode/five8/64
  time:   [205.09 ns 205.18 ns 205.27 ns]
  thrpt:  [297.33 MiB/s 297.47 MiB/s 297.61 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo_XMR/64
  time:   [171.70 ns 171.83 ns 172.01 ns]
  thrpt:  [354.84 MiB/s 355.21 MiB/s 355.47 MiB/s]

Benchmarking Base58_Performances/Encode/base58_monero/64
  time:   [522.58 ns 524.08 ns 525.00 ns]
  thrpt:  [116.26 MiB/s 116.46 MiB/s 116.80 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/64
  time:   [84.199 ns 84.252 ns 84.302 ns]
  thrpt:  [995.51 MiB/s 996.10 MiB/s 996.72 MiB/s]

Benchmarking Base58_Performances/Decode/bs58/64
  time:   [1.2751 µs 1.2758 µs 1.2765 µs]
  thrpt:  [65.747 MiB/s 65.782 MiB/s 65.816 MiB/s]

Benchmarking Base58_Performances/Decode/base58/64
  time:   [1.6920 µs 1.6931 µs 1.6941 µs]
  thrpt:  [49.537 MiB/s 49.569 MiB/s 49.600 MiB/s]

Benchmarking Base58_Performances/Decode/five8/64
  time:   [202.03 ns 202.76 ns 203.33 ns]
  thrpt:  [412.75 MiB/s 413.90 MiB/s 415.41 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo_XMR/64
  time:   [173.74 ns 173.99 ns 174.26 ns]
  thrpt:  [481.60 MiB/s 482.36 MiB/s 483.03 MiB/s]

Benchmarking Base58_Performances/Decode/base58_monero/64
  time:   [301.57 ns 301.81 ns 302.10 ns]
  thrpt:  [277.80 MiB/s 278.07 MiB/s 278.29 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo/128
  time:   [395.32 ns 395.93 ns 396.75 ns]
  thrpt:  [307.68 MiB/s 308.31 MiB/s 308.79 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/128
  time:   [17.807 µs 17.814 µs 17.822 µs]
  thrpt:  [6.8493 MiB/s 6.8524 MiB/s 6.8553 MiB/s]

Benchmarking Base58_Performances/Encode/base58/128
  time:   [14.306 µs 14.317 µs 14.329 µs]
  thrpt:  [8.5193 MiB/s 8.5263 MiB/s 8.5330 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo_XMR/128
  time:   [299.11 ns 299.38 ns 299.75 ns]
  thrpt:  [407.25 MiB/s 407.74 MiB/s 408.11 MiB/s]

Benchmarking Base58_Performances/Encode/base58_monero/128
  time:   [973.59 ns 976.19 ns 978.22 ns]
  thrpt:  [124.79 MiB/s 125.05 MiB/s 125.38 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/128
  time:   [176.35 ns 176.42 ns 176.49 ns]
  thrpt:  [945.60 MiB/s 945.97 MiB/s 946.36 MiB/s]

Benchmarking Base58_Performances/Decode/bs58/128
  time:   [5.2241 µs 5.2268 µs 5.2296 µs]
  thrpt:  [31.913 MiB/s 31.930 MiB/s 31.947 MiB/s]

Benchmarking Base58_Performances/Decode/base58/128
  time:   [3.2969 µs 3.3001 µs 3.3031 µs]
  thrpt:  [50.526 MiB/s 50.572 MiB/s 50.621 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo_XMR/128
  time:   [354.91 ns 355.14 ns 355.38 ns]
  thrpt:  [469.62 MiB/s 469.94 MiB/s 470.24 MiB/s]

Benchmarking Base58_Performances/Decode/base58_monero/128
  time:   [526.70 ns 527.17 ns 527.64 ns]
  thrpt:  [316.30 MiB/s 316.58 MiB/s 316.86 MiB/s]
```

</details>

## Safety & Verification

Achieving maximum throughput must not cost memory safety, so the crate carries `#![forbid(unsafe_code)]` at the top of `lib.rs` — the compiler rejects any `unsafe` block anywhere in the crate, including in future contributions. There is no pointer arithmetic and no manually-asserted invariant to audit; every bounds check that matters to performance is elided by the compiler because the code is shaped to make that provable, not because it was told to trust itself.

That leaves ordinary logic bugs as the only class of failure worth guarding against, which is what the test suite is for: exact conformance vectors, every kernel-dispatch and scratch-buffer boundary, and randomized cross-validation against `bs58`, `base58`, `five8`, and `base58-monero` on every push. See [.github/workflows/tests.yml](.github/workflows/tests.yml) for exactly what runs.

## Feature Flags

| Feature | Default | Description |
| :--- | :---: | :--- |
| `serde` | **No** | Enables `serde` serialization/deserialization for Config and Engine |
| `std` | **Yes** | Enables `String` and `Vec` support. Disable for `no_std` |

## License

Licensed under either of

- [Apache License, Version 2.0](https://github.com/hacer-bark/base58-turbo/blob/main/LICENSE-APACHE)
- [MIT license](https://github.com/hacer-bark/base58-turbo/blob/main/LICENSE-MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this crate, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
