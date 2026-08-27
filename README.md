<div align="center">
  <h1>Base58 Turbo</h1>
  <p><strong>A Rust Base58 codec that decodes past 2 GiB/s, with scalar kernels and an optional AVX2 path on x86/x86-64.</strong></p>

  [![Crates.io](https://img.shields.io/crates/v/base58-turbo.svg?style=for-the-badge&color=fc8d62)](https://crates.io/crates/base58-turbo)
  [![License](https://img.shields.io/crates/l/base58-turbo.svg?style=for-the-badge&color=8da0cb)](https://crates.io/crates/base58-turbo)
  [![CI](https://img.shields.io/github/actions/workflow/status/hacer-bark/base58-turbo/tests.yml?label=CI&style=for-the-badge&color=e78ac3)](https://github.com/hacer-bark/base58-turbo/actions/workflows/tests.yml)
</div>

<br/>

`base58-turbo` targets systems where CPU cycles are scarce. The default build enables `unsafe-simd`, which on x86/x86-64 adds one `unsafe` AVX2 module (`src/simd.rs`) — the crate's only `unsafe` code. Every other target, and any build with `unsafe-simd` off, is `#![forbid(unsafe_code)]`. See [Safety & Verification](#safety--verification).

`base58-turbo` beats `bs58`, `base58`, `base58-monero`, and `five8` at every payload size, peaking past 2 GiB/s on decode — over 2x `five8`'s best and 20x+ `bs58`. See [Benchmarks](#benchmarks).

<img alt="Base58 throughput by payload size on AWS c8a.large (AMD EPYC 9R45) — base58-turbo leads bs58, base58, base58-monero, and five8 at every size, decoding past 2 GiB/s" src="benches/results/throughput.png">

<p align="center"><sub>AWS <code>c8a.large</code> (AMD EPYC 9R45). See <a href="#benchmarks">Benchmarks</a> for a second box and reproduction steps.</sub></p>

## Quick Start

### Encoding

```rust
use base58_turbo::BITCOIN;

let encoded = BITCOIN.encode(b"Hello World").unwrap(); // Result<String, Error>
assert_eq!(encoded, "JxF12TrwUP45BMd");
```

### Decoding

```rust
use base58_turbo::BITCOIN;

let decoded = BITCOIN.decode("JxF12TrwUP45BMd").unwrap(); // Result<Vec<u8>, Error>
assert_eq!(decoded, b"Hello World");
```

### Zero-Allocation (Stack)

For hot paths where heap allocation is too slow, write directly to a stack buffer:

```rust
use base58_turbo::BITCOIN;

let mut output = [0u8; 64];
let len = BITCOIN.encode_into(b"Hello World", &mut output).unwrap(); // Result<usize, Error>
assert_eq!(std::str::from_utf8(&output[..len]).unwrap(), "JxF12TrwUP45BMd");
```

### Native Monero Chunking (XMR)

Native support for Monero's block-chunked Base58 format (8-byte blocks padded to 11 characters):

```rust
use base58_turbo::xmr;

let payload = b"Hello World"; // Typically 69-byte addresses
let encoded = xmr::encode(payload).unwrap(); // Result<String, Error>
let decoded = xmr::decode(&encoded).unwrap(); // Result<Vec<u8>, Error>
```

Zero-allocation `xmr::encode_into` / `xmr::decode_into` are also provided.

## Engines

- `BITCOIN`: Standard Bitcoin alphabet.
- `MONERO`: Monero alphabet.
- `RIPPLE`: Ripple alphabet.
- `FLICKR`: Flickr alphabet.
- `Engine::new(&[u8; 58])`: Custom alphabets.

## Compatibility & Stability

**MSRV:** Rust 1.87.0 or newer.

**API stability:** The public API (traits, structs, error types) is **Stable** and follows Semantic Versioning. It stays backward-compatible throughout the `0.3.x` lifecycle.

## Benchmarks

Numbers below come straight from `cargo bench` (`benches/encoding_bench.rs`), comparing `base58-turbo` against `bs58`, `base58`, `five8`, and `base58-monero` at payload sizes 16–128 bytes. `five8` only ships fixed-width 32/64-byte encoders. Default features are on, so `Turbo`/`Turbo_XMR` include the `unsafe-simd` AVX2 path — add `--no-default-features --features std` to bench the scalar kernels alone.

**AWS `c8a.large` (AMD EPYC 9R45), chart above:** at 48 bytes, decode hits 2.36 GiB/s vs 97.9 MiB/s for `bs58` (+2369%) and 79.2 MiB/s for `base58` (+2954%). Against `five8`: decode wins at both sizes it supports (2.19 vs 1.11 GiB/s @ 32B, +97%; 2.34 vs 1.17 GiB/s @ 64B, +100%), and encode wins too (1.26 vs 0.87 GiB/s @ 32B, +44%; 1.32 vs 1.00 GiB/s @ 64B, +33%). XMR (block-chunked, slower than flat) still leads `base58-monero` by +142% decode / +390% encode at 48 bytes. Peak: 2.36 GiB/s decode, single-threaded.

**AWS `c7i.large` (Intel Xeon Platinum 8488C), a smaller/cheaper box:**

<img alt="Base58 throughput by payload size on AWS c7i.large (Intel Xeon Platinum 8488C) — a smaller instance run with the same methodology" src="benches/results/throughput-c7i.png">

At 48 bytes: 1.59 GiB/s decode vs 84 MiB/s for `bs58` (+1790%) and 60 MiB/s for `base58` (+2553%). Against `five8`, decode wins clearly (1.38 vs 0.78 GiB/s @ 32B, +76%; 1.67 vs 0.70 GiB/s @ 64B, +139%), but encode is closer — `five8` edges ahead at 32B (0.70 vs 0.74 GiB/s, -5%) before `base58-turbo` retakes the lead at 64B (0.62 vs 0.55 GiB/s, +13%). Both machines show the same relative ordering and shape, differing by ~1.5x in absolute ceiling — the 2 GiB/s+ figure is a real peak on real hardware, not a universal constant.

Reproduce on a fresh checkout with nothing else running:

```bash
sudo apt update && sudo apt install -y build-essential git
curl --proto '=https' --tlsv1.3 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

git clone https://github.com/hacer-bark/base58-turbo
cd base58-turbo
RUSTFLAGS="-C target-cpu=native" BENCH_TARGET=all cargo bench 2>&1 | tee benches/results/raw.txt
python3 benches/scripts/plot_bench.py benches/results/raw.txt
```

<details>
<summary>Raw <code>cargo bench</code> output — AWS <code>c8a.large</code></summary>

See [`benches/results/c8a-large-latest.txt`](benches/results/c8a-large-latest.txt) for the
full output — 32 B through 10 MB, every target.

</details>

<details>
<summary>Raw <code>cargo bench</code> output — AWS <code>c7i.large</code></summary>

See [`benches/results/c7i-large-latest.txt`](benches/results/c7i-large-latest.txt) for the
full output — 32 B through 10 MB, every target.

</details>

## Safety & Verification

`#![forbid(unsafe_code)]` applies conditionally in `lib.rs`: active whenever `unsafe-simd` is off, or on any target other than x86/x86-64. There, the compiler rejects any `unsafe` block in the crate — no pointer arithmetic, no manually-asserted invariant to audit.

**`unsafe-simd` is a default feature.** On x86/x86-64 it compiles in `src/simd.rs`, one module of AVX2 intrinsics reached only after a runtime AVX2 check — the crate's only `unsafe` code. For the compiler-enforced guarantee instead:

```bash
cargo add base58-turbo --no-default-features --features std
```

This drops to the scalar kernels and reinstates `#![forbid(unsafe_code)]`. Runtime dispatch means `unsafe-simd` never breaks correctness on hardware without AVX2 — it just falls back to scalar — so the tradeoff is purely whether you want that module in your dependency tree.

The test suite guards against ordinary logic bugs in either configuration: exact conformance vectors, every kernel-dispatch and scratch-buffer boundary, randomized cross-validation against `bs58`, `base58`, `five8`, and `base58-monero`, and — with `unsafe-simd` on — AVX2 kernels checked against an independent schoolbook implementation. See [.github/workflows/tests.yml](.github/workflows/tests.yml).

## Feature Flags

| Feature | Default | Description |
| :--- | :---: | :--- |
| `serde` | No | `serde` serialization/deserialization for `Config` and `Engine` |
| `std` | Yes | `String`/`Vec` support; disable for `no_std` |
| `unsafe-simd` | Yes | AVX2 kernels on x86/x86-64, selected at runtime; the crate's only `unsafe` code. Disable for `#![forbid(unsafe_code)]` |

## License

Licensed under the [0BSD license](https://github.com/hacer-bark/base58-turbo/blob/main/LICENSE).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this crate shall be licensed as above, without any additional terms or conditions.
