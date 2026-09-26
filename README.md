<div align="center">
  <h1>Base58 Turbo</h1>
  <p><strong>A high-throughput Rust Base58 codec with scalar kernels. 100% safe Rust, <code>#![forbid(unsafe_code)]</code>.</strong></p>

  [![Crates.io](https://img.shields.io/crates/v/base58-turbo.svg?style=for-the-badge&color=fc8d62)](https://crates.io/crates/base58-turbo)
  [![License](https://img.shields.io/crates/l/base58-turbo.svg?style=for-the-badge&color=8da0cb)](https://crates.io/crates/base58-turbo)
  [![CI](https://img.shields.io/github/actions/workflow/status/hacer-bark/base58-turbo/tests.yml?label=CI&style=for-the-badge&color=e78ac3)](https://github.com/hacer-bark/base58-turbo/actions/workflows/tests.yml)
  [![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg?style=for-the-badge&color=66c2a5)](https://github.com/rust-secure-code/safety-dance/)
</div>

<br/>

`base58-turbo` targets systems where CPU cycles are scarce. Every build is `#![forbid(unsafe_code)]` — there is no `unsafe` anywhere in the crate, on any target or feature combination. See [Safety & Verification](#safety--verification).

`base58-turbo` beats `bs58`, `base58`, and `base58-monero` at every measured payload size. It also beats `five8` on decode, with mixed encode results. See [Benchmarks](#benchmarks).

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

These results come from `cargo bench` on an AWS `c8a.large` (AMD EPYC 9R45). Throughput is the measured median, while the "faster" percentages compare time per operation and are rounded conservatively in the competing library's favor. Results will vary by CPU and input. `five8` only provides fixed-width 32/64-byte codecs.

### Standard Base58

#### Encode

| Payload | Turbo throughput | vs `bs58` | vs `base58` | vs `five8` |
| :---: | ---: | ---: | ---: | ---: |
| 16 B | 528 MiB/s | ~85% faster | ~90% faster | — |
| 32 B | 663 MiB/s | ~95% faster | ~95% faster | ~10% slower |
| 48 B | 622 MiB/s | ~95% faster | ~95% faster | — |
| 64 B | 757 MiB/s | ~95% faster | ~95% faster | ~10% faster |
| 128 B | 365 MiB/s | ~95% faster | ~95% faster | — |

#### Decode

| Payload | Turbo throughput | vs `bs58` | vs `base58` | vs `five8` |
| :---: | ---: | ---: | ---: | ---: |
| 16 B | 890 MiB/s | ~80% faster | ~90% faster | — |
| 32 B | 868 MiB/s | ~85% faster | ~90% faster | ~25% faster |
| 48 B | 843 MiB/s | ~90% faster | ~90% faster | — |
| 64 B | 1,011 MiB/s | ~90% faster | ~90% faster | ~55% faster |
| 128 B | 958 MiB/s | ~95% faster | ~90% faster | — |

### Monero Base58

XMR uses block-chunked Base58, so Turbo XMR is compared separately with `base58-monero`.

| Payload | Encode throughput | Encode vs `base58-monero` | Decode throughput | Decode vs `base58-monero` |
| :---: | ---: | ---: | ---: | ---: |
| 16 B | 924 MiB/s | ~90% faster | 1.42 GiB/s | ~85% faster |
| 32 B | 1.25 GiB/s | ~90% faster | 1.85 GiB/s | ~85% faster |
| 48 B | 1.33 GiB/s | ~90% faster | 2.05 GiB/s | ~90% faster |
| 64 B | 1.38 GiB/s | ~90% faster | 2.17 GiB/s | ~90% faster |
| 128 B | 1.47 GiB/s | ~90% faster | 2.43 GiB/s | ~90% faster |

Reproduce from a checkout with nothing else running:

```bash
RUSTFLAGS="-C target-cpu=native" BENCH_TARGET=all cargo bench
```

## Safety & Verification

`#![forbid(unsafe_code)]` applies unconditionally in `lib.rs`: no feature flag, target, or configuration reintroduces `unsafe`. The compiler rejects any `unsafe` block anywhere in the crate — no pointer arithmetic, no manually-asserted invariant to audit. Performance comes entirely from the base conversion algorithm and from shaping the hot loops so the compiler can drop bounds checks on its own.

The test suite guards against ordinary logic bugs: exact conformance vectors, every kernel-dispatch and scratch-buffer boundary, and randomized cross-validation against `bs58`, `base58`, `five8`, and `base58-monero`. See [.github/workflows/tests.yml](.github/workflows/tests.yml).

**Not constant-time.** Running time depends on the data (leading zeros, output length, where an invalid character sits), and stack scratch is not zeroed afterwards. Do not use it on secret material, such as private keys, where timing or memory disclosure matters.

## Feature Flags

| Feature | Default | Description |
| :--- | :---: | :--- |
| `std` | Yes | `String`/`Vec` support; disable for `no_std` |

## License

Licensed under the [0BSD license](https://github.com/hacer-bark/base58-turbo/blob/main/LICENSE).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this crate shall be licensed as above, without any additional terms or conditions.
