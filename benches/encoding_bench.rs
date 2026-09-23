#![allow(
    missing_docs,
    missing_debug_implementations,
    unreachable_pub,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use criterion::{
    AxisScale, BenchmarkId, Criterion, PlotConfiguration, Throughput, criterion_group,
    criterion_main,
};
use rand::RngExt;
use std::env;
use std::hint::black_box;
use std::time::Duration;

// 1. Turbo code
use base58_turbo::*;
// 2. The bs58
use bs58::{decode as decode_std, encode as encode_std};
// 3. The base58
use base58::{FromBase58, ToBase58};
// 4. The five8
use five8::{decode_32, decode_64, encode_32, encode_64};
// 5. base58_monero
use base58_monero::base58 as base58_xmr;

fn generate_random_data(size: usize) -> Vec<u8> {
    let mut data = vec![0u8; size];
    rand::rng().fill(&mut data[..]);
    data
}

/// Helper to check if a specific engine should be benchmarked based on ENV vars.
/// Usage: `BENCH_TARGET=turbo cargo bench` or `BENCH_TARGET=all cargo bench`
fn should_run(target_name: &str) -> bool {
    let var = env::var("BENCH_TARGET").unwrap_or_else(|_| "turbo".to_string());
    let targets: Vec<String> = var.split(',').map(|s| s.trim().to_lowercase()).collect();
    if targets.contains(&"all".to_string()) {
        return true;
    }
    targets.contains(&target_name.to_lowercase())
}

fn bench_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("Base58_Performances");

    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    group.measurement_time(Duration::from_secs(5));
    group.warm_up_time(Duration::from_secs(3));
    group.noise_threshold(0.05);
    group.sample_size(50);

    let sizes = [16, 32, 48, 64, 128];

    // Fairness rules for this comparison:
    //
    //   * every crate that exposes a zero-allocation API is benchmarked through
    //     it, with the destination buffer allocated once, outside the timed loop;
    //   * `base58` and `base58-monero` have no such API, so they must allocate.
    //     Their rows are suffixed `(alloc)` rather than being compared silently
    //     against zero-allocation ones -- a heap allocation costs ~23 ns here,
    //     which is most of the runtime at these sizes;
    //   * fixed-size conversions (five8 needs `&[u8; 32]`) are hoisted out of the
    //     loop, so nobody is charged for setup the others do not pay;
    //   * each crate uses its own fastest path: bs58 keeps its default Bitcoin
    //     alphabet so it can specialise, rather than being handed one opaquely.

    for size in sizes.iter() {
        let input_data = generate_random_data(*size);

        // ======================================================================
        // ENCODE
        // ======================================================================
        group.throughput(Throughput::Bytes(*size as u64));

        // 1. Base58 Turbo (zero-allocation)
        if should_run("turbo") {
            group.bench_with_input(
                BenchmarkId::new("Encode/Turbo", size),
                &input_data,
                |b, d| {
                    let mut buf = vec![0u8; BITCOIN.encoded_len(d.len())];
                    b.iter(|| {
                        black_box(
                            BITCOIN
                                .encode_into(black_box(d), black_box(&mut buf))
                                .unwrap(),
                        )
                    });
                },
            );
        }

        // 2. bs58 (zero-allocation via `onto`, default Bitcoin alphabet)
        if should_run("bs58") {
            group.bench_with_input(
                BenchmarkId::new("Encode/bs58", size),
                &input_data,
                |b, d| {
                    let mut buf = vec![0u8; d.len() * 2 + 8];
                    b.iter(|| {
                        black_box(
                            encode_std(black_box(d))
                                .onto(black_box(&mut buf[..]))
                                .unwrap(),
                        )
                    });
                },
            );
        }

        // 3. base58 -- allocating only, no zero-copy API exists
        if should_run("base58") {
            group.bench_with_input(
                BenchmarkId::new("Encode/base58", size),
                &input_data,
                |b, d| b.iter(|| black_box(black_box(d).to_base58())),
            );
        }

        // 4. five8 -- fixed 32/64 only; the array conversion is hoisted out
        if should_run("five8") && *size == 32 {
            let arr: [u8; 32] = input_data.as_slice().try_into().unwrap();
            group.bench_with_input(BenchmarkId::new("Encode/five8", size), &arr, |b, d| {
                let mut buf = [0u8; 44];
                b.iter(|| {
                    let n = encode_32(black_box(d), black_box(&mut buf));
                    black_box((n, &buf));
                });
            });
        }
        if should_run("five8") && *size == 64 {
            let arr: [u8; 64] = input_data.as_slice().try_into().unwrap();
            group.bench_with_input(BenchmarkId::new("Encode/five8", size), &arr, |b, d| {
                let mut buf = [0u8; 88];
                b.iter(|| {
                    let n = encode_64(black_box(d), black_box(&mut buf));
                    black_box((n, &buf));
                });
            });
        }

        // 5. XMR: Turbo is zero-allocation, base58-monero allocates by design
        if should_run("xmr") {
            group.bench_with_input(
                BenchmarkId::new("Encode/Turbo_XMR", size),
                &input_data,
                |b, d| {
                    let mut buf = vec![0u8; d.len() * 2 + 16];
                    b.iter(|| {
                        black_box(
                            base58_turbo::xmr::encode_into(black_box(d), black_box(&mut buf))
                                .unwrap(),
                        )
                    });
                },
            );
            group.bench_with_input(
                BenchmarkId::new("Encode/base58_monero", size),
                &input_data,
                |b, d| b.iter(|| black_box(base58_xmr::encode(black_box(d)).unwrap())),
            );
        }

        // ======================================================================
        // DECODE
        // ======================================================================
        let encoded_str = encode_std(&input_data).into_string();
        group.throughput(Throughput::Bytes(encoded_str.len() as u64));

        // 1. Base58 Turbo (zero-allocation)
        if should_run("turbo") {
            group.bench_with_input(
                BenchmarkId::new("Decode/Turbo", size),
                encoded_str.as_bytes(),
                |b, d| {
                    let mut buf = vec![0u8; BITCOIN.decoded_len(d.len())];
                    b.iter(|| {
                        black_box(
                            BITCOIN
                                .decode_into(black_box(d), black_box(&mut buf))
                                .unwrap(),
                        )
                    });
                },
            );
        }

        // 2. bs58 (zero-allocation via `onto`)
        if should_run("bs58") {
            group.bench_with_input(
                BenchmarkId::new("Decode/bs58", size),
                &encoded_str,
                |b, d| {
                    let mut buf = vec![0u8; d.len()];
                    b.iter(|| {
                        black_box(
                            decode_std(black_box(d))
                                .onto(black_box(&mut buf[..]))
                                .unwrap(),
                        )
                    });
                },
            );
        }

        // 3. base58 -- allocating only
        if should_run("base58") {
            group.bench_with_input(
                BenchmarkId::new("Decode/base58", size),
                &encoded_str,
                |b, d| b.iter(|| black_box(black_box(d).from_base58().unwrap())),
            );
        }

        // 4. five8 -- fixed 32/64 only
        if should_run("five8") && *size == 32 {
            group.bench_with_input(
                BenchmarkId::new("Decode/five8", size),
                &encoded_str,
                |b, d| {
                    let mut buf = [0u8; 32];
                    b.iter(|| {
                        decode_32(black_box(d), black_box(&mut buf)).unwrap();
                        black_box(&buf);
                    });
                },
            );
        }
        if should_run("five8") && *size == 64 {
            group.bench_with_input(
                BenchmarkId::new("Decode/five8", size),
                &encoded_str,
                |b, d| {
                    let mut buf = [0u8; 64];
                    b.iter(|| {
                        decode_64(black_box(d), black_box(&mut buf)).unwrap();
                        black_box(&buf);
                    });
                },
            );
        }

        // 5. XMR
        if should_run("xmr") {
            let encoded_xmr = base58_turbo::xmr::encode(&input_data).unwrap();
            group.bench_with_input(
                BenchmarkId::new("Decode/Turbo_XMR", size),
                &encoded_xmr,
                |b, d| {
                    let mut buf = vec![0u8; d.len()];
                    b.iter(|| {
                        black_box(
                            base58_turbo::xmr::decode_into(black_box(d), black_box(&mut buf))
                                .unwrap(),
                        )
                    });
                },
            );
            group.bench_with_input(
                BenchmarkId::new("Decode/base58_monero", size),
                &encoded_xmr,
                |b, d| b.iter(|| black_box(base58_xmr::decode(black_box(d)).unwrap())),
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_comparison);
criterion_main!(benches);
