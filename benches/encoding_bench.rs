use criterion::{
    criterion_group, criterion_main, AxisScale, BenchmarkId, Criterion,
    PlotConfiguration, Throughput,
};
use std::hint::black_box;
use rand::Rng;
use std::env;
use std::time::Duration;

// 0. Turbo code
use base58_turbo::*;
// 1. The bs58
use bs58::{encode as encode_std, decode as decode_std, Alphabet as AlphabetStd};
// 2. The base58
use base58::{FromBase58, ToBase58};
// 3. The bsv58
use bsv58::{encode as encode_simd, decode as decode_simd};
// 4. The b58
use b58::{encode as encode_b58, decode as decode_b58};
// 5. The five8
use five8::{decode_32, decode_64, encode_32, encode_64};

fn generate_random_data(size: usize) -> Vec<u8> {
    let mut data = vec![0u8; size];
    rand::rng().fill(&mut data[..]);
    data
}

/// Helper to check if a specific engine should be benchmarked based on ENV vars.
/// Usage: `BENCH_TARGET=turbo cargo bench` or `BENCH_TARGET=all cargo bench`
fn should_run(target_name: &str) -> bool {
    let var = env::var("BENCH_TARGET").unwrap_or_else(|_| "all".to_string());
    if var == "all" {
        return true;
    }
    var.to_lowercase().eq(&target_name.to_lowercase())
}

fn bench_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("Base58_Performances");

    // Logarithmic scaling is essential for viewing 32B vs 10MB
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    group.measurement_time(Duration::from_secs(5));
    group.warm_up_time(Duration::from_secs(3));
    group.noise_threshold(0.05);
    group.sample_size(50);

    let sizes = [32, 48, 64, 69, 128, 256];

    for size in sizes.iter() {
        let input_data = generate_random_data(*size);

        // ======================================================================
        // ENCODE
        // ======================================================================
        group.throughput(Throughput::Bytes(*size as u64));

        // 1a. Base58 Turbo (Allocating)
        if should_run("turbo") {
            group.bench_with_input(BenchmarkId::new("Encode/Turbo", size), &input_data, |b, d| {
                b.iter(|| {
                    let required_cap = (d.len() * 138 / 100) + 1;
                    let mut buffer = Vec::<u8>::with_capacity(required_cap);

                    unsafe {
                        let len = encode_slice_unsafe(black_box(d), buffer.as_mut_ptr());
                        buffer.set_len(len);
                    };

                    black_box(buffer);
                })
            });
        }

        // // 2. bs58 Standard
        // if should_run("bs58")  {
        //     group.bench_with_input(BenchmarkId::new("Encode/bs58", size), &input_data, |b, d| {
        //         b.iter(|| encode_std(black_box(d)).with_alphabet(black_box(AlphabetStd::BITCOIN)).into_string())
        //     });
        // }

        // // 3. b58 Standard
        // if should_run("b58")  {
        //     group.bench_with_input(BenchmarkId::new("Encode/b58", size), &input_data, |b, d| {
        //         b.iter(|| encode_b58(black_box(d)))
        //     });
        // }

        // // 4. Base58 Classic
        // if should_run("base58") {
        //     group.bench_with_input(BenchmarkId::new("Encode/base58", size), &input_data, |b, d| {
        //         b.iter(|| black_box(d).to_base58())
        //     });
        // }

        // // 5. vbs58 "SIMD"
        // if should_run("vbs58") {
        //     group.bench_with_input(BenchmarkId::new("Encode/vbs58", size), &input_data, |b, d| {
        //         b.iter(|| encode_simd(black_box(d)))
        //     });
        // }

        // 5a. five8-32 "non-general code"
        if should_run("five8") && *size == 32 {
            group.bench_with_input(BenchmarkId::new("Encode/five8", size), &input_data, |b, d| {
                b.iter(|| {
                    let mut buffer = [0u8; 44];
                    let static_bytes: [u8; 32] = d.as_slice().try_into().unwrap();
                    encode_32(&black_box(static_bytes), &mut buffer);

                    black_box(buffer);
                })
            });
        }

        // 5b. five8-64 "non-general code"
        if should_run("five8") && *size == 64 {
            group.bench_with_input(BenchmarkId::new("Encode/five8", size), &input_data, |b, d| {
                b.iter(|| {
                    let mut buffer = [0u8; 88];
                    let static_bytes: [u8; 64] = d.as_slice().try_into().unwrap();
                    encode_64(&black_box(static_bytes), &mut buffer);

                    black_box(buffer);
                })
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_comparison);
criterion_main!(benches);
