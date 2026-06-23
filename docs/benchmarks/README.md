# ⚡ Benchmarks & Methodology

This directory contains detailed performance reports for `base58-turbo` across various hardware architectures.

## 📊 Benchmark Reports

### ☁️ Server / Data Center (HFT & Cloud)
High-performance environments.

*   **[Intel Xeon Platinum 8488C](./intel_xeon_8488c.md)**
    *   **Environment:** AWS `c7i.large`
    *   **Context:** Modern Cloud standard.

### 💻 Consumer / Runtime Scaling Analysis
Everyday development and consumer hardware environments.

*   **[Intel Core i7-8750H](./intel_i7_8750h.md)**
    *   **Environment:** Local Machine
    *   **Context:** Standard Consumer Architecture (AVX2).

## 🧪 Methodology

All benchmarks were conducted using [criterion.rs](https://github.com/bheisler/criterion.rs) to ensure statistical significance, utilizing a rigorous configuration to filter out OS noise.

### 1. Test Configuration
To ensure high confidence intervals, we utilize longer-than-average test durations and a rigorous configuration:

*   **Warm-up Time:** **3 seconds** (Primes branch predictor and caches).
*   **Measurement Time:** **5 seconds** per group.
*   **Noise Threshold:** **0.05** (5%).
*   **Sample Size:** **50 samples**.

### 2. Input Scaling
We benchmark against a spread of data sizes to capture performance characteristics across typical payload sizes.

| Size | Use Case |
| :--- | :--- |
| **16 B** | Small strings, UUIDs. |
| **24 B** | Short addresses. |
| **25 B** | Bitcoin Addresses (PubKeyHash/ScriptHash with checksum). |
| **32 B** | Cryptographic keys (Ed25519, secp256k1). |
| **48 B** | Larger keys or signatures. |
| **64 B** | Long signatures, combined keys. |
| **69 B** | Extended payload structures. |
| **128 B** | Small certificates or payloads. |

> **Note:** Benchmark plots use a **Logarithmic Scale** on the X-axis.

### 3. Comparison Targets (`BENCH_TARGET`)
Our benchmark suite is controlled via the `BENCH_TARGET` environment variable. This allows isolated comparisons between specific implementations. You can provide a comma-separated list of targets.

| Target | Description |
| :--- | :--- |
| `turbo` | **(Default)** The `base58-turbo` API. |
| `bs58` | The `bs58` crate. |
| `base58` | The classic `base58` crate. |
| `five8` | The `five8` crate. |
| `xmr` | Compares `base58-turbo::xmr` against the `base58-monero` crate. |
| `all` | Runs all of the above. |

### 4. Reproduction
You can reproduce these results locally using the following commands:

```bash
# Run comparison: base58-turbo vs bs58
BENCH_TARGET=turbo,bs58 cargo bench

# Run EVERYTHING
BENCH_TARGET=all cargo bench
```
