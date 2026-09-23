<div align="center">
  <h1>Base58 Turbo</h1>
  <p><strong>A Rust Base58 codec that decodes past 1.5 GiB/s with scalar kernels. 100% safe Rust, <code>#![forbid(unsafe_code)]</code>.</strong></p>

  [![Crates.io](https://img.shields.io/crates/v/base58-turbo.svg?style=for-the-badge&color=fc8d62)](https://crates.io/crates/base58-turbo)
  [![License](https://img.shields.io/crates/l/base58-turbo.svg?style=for-the-badge&color=8da0cb)](https://crates.io/crates/base58-turbo)
  [![CI](https://img.shields.io/github/actions/workflow/status/hacer-bark/base58-turbo/tests.yml?label=CI&style=for-the-badge&color=e78ac3)](https://github.com/hacer-bark/base58-turbo/actions/workflows/tests.yml)
  [![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg?style=for-the-badge&color=66c2a5)](https://github.com/rust-secure-code/safety-dance/)
</div>

<br/>

`base58-turbo` targets systems where CPU cycles are scarce. Every build is `#![forbid(unsafe_code)]` — there is no `unsafe` anywhere in the crate, on any target or feature combination. See [Safety & Verification](#safety--verification).

`base58-turbo` beats `bs58`, `base58`, and `base58-monero` at every measured payload size (7–40x `bs58` on decode), and beats `five8` everywhere except `five8`'s fixed-width 64-byte encoder. See [Benchmarks](#benchmarks).

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

**API stability:** Follows Semantic Versioning; while the crate is `0.x`, breaking changes bump the minor version. See [CHANGELOG.md](CHANGELOG.md).

## Benchmarks

Numbers come from `cargo bench` (`benches/encoding_bench.rs`) on AWS `c8a.large` (AMD EPYC 9R45), comparing `base58-turbo` against `bs58`, `base58`, `five8`, and `base58-monero` at payload sizes 16–128 bytes. `five8` only ships fixed-width 32/64-byte codecs.

At 48 bytes, decode runs at 1.57 GiB/s vs 95.5 MiB/s for `bs58` (+1584%) and 78.2 MiB/s for `base58` (+1958%). Against `five8`, decode wins at both sizes it supports (1.59 vs 1.10 GiB/s @ 32B, +45%; 1.57 vs 1.16 GiB/s @ 64B, +36%). Encode wins at 32B (1.23 vs 0.86 GiB/s, +43%) but loses at 64B (582 MiB/s vs 1.11 GiB/s, -49%). XMR (block-chunked, slower than flat) leads `base58-monero` by +155% decode / +414% encode at 48 bytes. Peak: 1.59 GiB/s decode, single-threaded.

Reproduce on a fresh checkout with nothing else running:

```bash
sudo apt update && sudo apt install -y build-essential git
curl --proto '=https' --tlsv1.3 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

git clone https://github.com/hacer-bark/base58-turbo
cd base58-turbo
RUSTFLAGS="-C target-cpu=native" BENCH_TARGET=all cargo bench 2>&1 | tee benches/results/raw.txt
python3 benches/scripts/plot_bench.py benches/results/raw.txt  # needs plotly + kaleido
```

<details>
<summary>Raw <code>cargo bench</code> output — AWS <code>c8a.large</code></summary>

See [`benches/results/c8a-large-latest.txt`](benches/results/c8a-large-latest.txt) for the
full output — 16 B through 128 B, every target.

</details>

## Safety & Verification

`#![forbid(unsafe_code)]` applies unconditionally in `lib.rs`: no feature flag, target, or configuration reintroduces `unsafe`. The compiler rejects any `unsafe` block anywhere in the crate — no pointer arithmetic, no manually-asserted invariant to audit. Performance comes entirely from the base conversion algorithm and from shaping the hot loops so the compiler can drop bounds checks on its own.

The test suite guards against ordinary logic bugs: exact conformance vectors, every kernel-dispatch and scratch-buffer boundary, and randomized cross-validation against `bs58`, `base58`, `five8`, and `base58-monero`. See [.github/workflows/tests.yml](.github/workflows/tests.yml).

## Feature Flags

| Feature | Default | Description |
| :--- | :---: | :--- |
| `std` | Yes | `String`/`Vec` support; disable for `no_std` |

## License

Licensed under the [0BSD license](https://github.com/hacer-bark/base58-turbo/blob/main/LICENSE).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this crate shall be licensed as above, without any additional terms or conditions.
