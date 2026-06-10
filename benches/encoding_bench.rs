use criterion::{
    criterion_group, criterion_main, AxisScale, BenchmarkId, Criterion,
    PlotConfiguration, Throughput,
};
use std::hint::black_box;
use rand::RngExt;
use std::env;
use std::time::Duration;

// 1. Turbo code
use base58_turbo::*;
// 2. The bs58
use bs58::{encode as encode_std, decode as decode_std, Alphabet as AlphabetStd};
// 3. The base58
use base58::{FromBase58, ToBase58};
// 4. The five8
use five8::{decode_32, decode_64, encode_32, encode_64};

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

    // Logarithmic scaling is essential for viewing 32B vs 10MB
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    group.measurement_time(Duration::from_secs(5));
    group.warm_up_time(Duration::from_secs(3));
    group.noise_threshold(0.05);
    group.sample_size(50);

    let sizes = [16, 24, 25, 32, 48, 64, 69, 128];

    for size in sizes.iter() {
        let input_data = generate_random_data(*size);

        // ======================================================================
        // ENCODE
        // ======================================================================
        group.throughput(Throughput::Bytes(*size as u64));

        // 1. Base58 Turbo (Allocating)
        if should_run("turbo") {
            group.bench_with_input(BenchmarkId::new("Encode/Turbo", size), &input_data, |b, d| {
                b.iter(|| BITCOIN.encode(black_box(d)).unwrap())
            });
        }

        // 2. bs58 Standard
        if should_run("bs58")  {
            group.bench_with_input(BenchmarkId::new("Encode/bs58", size), &input_data, |b, d| {
                b.iter(|| encode_std(black_box(d)).with_alphabet(black_box(AlphabetStd::BITCOIN)).into_string())
            });
        }

        // 3. Base58 Classic
        if should_run("base58") {
            group.bench_with_input(BenchmarkId::new("Encode/base58", size), &input_data, |b, d| {
                b.iter(|| black_box(d).to_base58())
            });
        }

        // 4a. five8-32 "non-general code"
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

        // 4b. five8-64 "non-general code"
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

        // ======================================================================
        // DECODE
        // ======================================================================
        let encoded_str = encode_std(&input_data).with_alphabet(black_box(AlphabetStd::BITCOIN)).into_string();
        group.throughput(Throughput::Bytes(encoded_str.len() as u64));

        // 1. Base58 Turbo (Allocating)
        if should_run("turbo") {
            group.bench_with_input(BenchmarkId::new("Decode/Turbo", size), encoded_str.as_bytes(), |b, d| {
                b.iter(|| BITCOIN.decode(black_box(d)).unwrap())
            });
        }

        // 2. bs58 Standard
        if should_run("bs58")  {
            group.bench_with_input(BenchmarkId::new("Decode/bs58", size), &encoded_str, |b, d| {
                b.iter(|| decode_std(black_box(d)).with_alphabet(black_box(AlphabetStd::BITCOIN)).into_vec().unwrap())
            });
        }

        // 3. Base58 Classic
        if should_run("base58") {
            group.bench_with_input(BenchmarkId::new("Decode/base58", size), &encoded_str, |b, d| {
                b.iter(|| black_box(d).from_base58().unwrap())
            });
        }

        // 4a. five8-32 "non-general code"
        if should_run("five8") && *size == 32 {
            group.bench_with_input(BenchmarkId::new("Decode/five8", size), &encoded_str, |b, d| {
                b.iter(|| {
                    let mut buffer = [0u8; 32];
                    decode_32(&black_box(d), &mut buffer).unwrap();

                    black_box(buffer);
                })
            });
        }

        // 4b. five8-64 "non-general code"
        if should_run("five8") && *size == 64 {
            group.bench_with_input(BenchmarkId::new("Decode/five8", size), &encoded_str, |b, d| {
                b.iter(|| {
                    let mut buffer = [0u8; 64];
                    decode_64(&black_box(d), &mut buffer).unwrap();

                    black_box(buffer);
                })
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_comparison);
criterion_main!(benches);
