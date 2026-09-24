# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Removed

- **Breaking:** the `unsafe-simd` feature and its AVX2 kernels. The crate is now
  `#![forbid(unsafe_code)]` on every target and feature combination.
- **Breaking:** the `serde` feature, the `base58_turbo::serde` module, and the
  `Serialize`/`Deserialize` impls for `Config` and `Engine`.

### Added

- `Config::alphabet()` and `Config::decode_map()` read-only accessors.
- Experimental Kani proof harnesses for the encoder kernels (`cargo kani`).

### Changed

- **Breaking:** `Config` fields are private; use `Config::new` and the new
  accessors. A hand-built `Config` with inconsistent tables could make the
  decoder overflow (a panic in debug builds) or return wrong output.
- License changed from `MIT OR Apache-2.0` to `0BSD`.
- Faster scalar encoder and decoder. Encoding above 64 bytes gained another
  10-30% (1 KiB: 51k -> 37k cycles on an i7-8750H).
- `xmr` converts each 8-byte block directly instead of through the general
  engine: 4-5x faster encode and decode.
- **Breaking:** `encode::encode_slice_unbounded` returns `Result<usize, Error>`.
  It and `encode::encode_slice` now return `Error::BufferTooSmall` for a short
  destination instead of panicking.
- `Engine::encode_32_batch` no longer requires the `std` feature.
- `Error::WrongAlphabet` now displays as "alphabet has a duplicate or non-ASCII
  char", since it also covers non-ASCII alphabets.

### Fixed

- `xmr::encoded_len` was documented as an upper bound; it is exact.
- On 32-bit targets, `Engine::encoded_len` saturated for inputs over ~31 MB,
  so `Engine::encode` panicked, and `Engine::decode` overflowed its scratch
  sizing past ~733 K characters.
- On 32-bit targets, `Engine::encode` and `xmr::encode` panicked with a capacity
  overflow for inputs of roughly 1 GB and up; they now return
  `Error::InputTooBig`.
- `Engine::decode_into` docs omitted the 1024-byte decoded-size limit and that
  `output` must be at least as long as `input`.
- Builds on the 1.87 MSRV again (a const only used in assertions tripped
  `dead_code`).
- Documentation, README benchmark figures, CI jobs, and the fuzz target no longer
  describe the removed `unsafe-simd` feature or the old 1024-byte limit on the
  allocating API.

## 0.3.0

### Added

- `unsafe-simd` feature, on by default: AVX2 encode and decode kernels on x86 and
  x86-64, chosen after a runtime CPU check. Builds without it stayed
  `#![forbid(unsafe_code)]`.
- `Engine::encode_32_batch` for batches of 32-byte inputs.
- A matrix encode kernel for short inputs and a weight-matrix decode path for
  payloads up to 24 characters.

## 0.2.0

### Added

- `xmr` module: Monero's block-chunked Base58 (`encode`, `decode`, `encode_into`,
  `decode_into`, `encoded_len`, `decoded_len`).
- Public `encode` and `decode` modules exposing the `encode_slice` / `decode_slice`
  kernels and their `_unbounded` heap-scratch variants.

### Changed

- The whole crate is safe Rust under `#![forbid(unsafe_code)]`.
- `Engine::encode` and `Engine::decode` no longer have a size limit; inputs past the
  zero-allocation ceiling fall back to heap scratch.

### Removed

- **Breaking:** the public `unsafe` kernels `encode_slice_unsafe` and
  `decode_slice_unsafe`.

## 0.1.0

### Added

- `Engine` with the `BITCOIN`, `MONERO`, `RIPPLE`, and `FLICKR` alphabets, plus
  custom alphabets through `Engine::new`.
- Allocating `encode` / `decode` (up to 1024 bytes / 2048 characters) and
  zero-allocation `encode_into` / `decode_into`.
- `no_std` support by disabling the default `std` feature.
- Optional `serde` feature.
- MSRV: Rust 1.87.
