# ⚖️ Ecosystem Comparison

This project references and benchmarks against several external Base58 libraries. Below is an analysis of the current landscape, detailing performance characteristics and safety guarantees.

## 📊 Quick Feature Matrix

| Library | Language | Optimized Kernels | Verified Safety |
| :--- | :---: | :---: | :---: |
| **base58-turbo** | Rust | ✅ (Matrix / 58^10) | ✅ (Kani/MIRI) |
| [bs58](https://crates.io/crates/bs58) | Rust | ❌ (Naive Bignum) | ✅ (Safe Rust) |
| [base-x](https://github.com/base-x/base-x) | JS/Rust | ❌ | ❌ |
| [bitcoin-base58](https://github.com/bitcoin/bitcoin) | C++ | ❌ | ❌ |

## The Rust Ecosystem

### 1. [bs58](https://crates.io/crates/bs58)
The standard library for Base58 in Rust.
*   **Pros:** Rock-solid stability. Uses 100% Safe Rust by default. Supports many alphabets.
*   **Cons:** Performance is limited by a naive "multiply-by-58" bignum implementation. It performs a full division for every single character.
*   **Verdict:** Use this if you absolutely cannot have `unsafe` code and throughput is not a concern.

### 2. [base-x](https://crates.io/crates/base-x)
A generic base-conversion library.
*   **Pros:** Supports any radix.
*   **Cons:** Extremely slow compared to specialized libraries like `base58-turbo`. Not optimized for hardware-specific features.
*   **Verdict:** Good for prototyping obscure bases, not for production HFT or blockchain nodes.

## The C++ Ecosystem

### 1. [Bitcoin Core (Base58)](https://github.com/bitcoin/bitcoin/blob/master/src/base58.cpp)
The reference implementation for Bitcoin.
*   **Pros:** The industry standard for correctness.
*   **Cons:** Surprisingly slow. It uses a vector-based bignum that reallocates and performs byte-by-byte arithmetic.
*   **Verdict:** `base58-turbo` is significantly faster while providing better memory safety guarantees than C++.

## Why `base58-turbo`?

Most Base58 libraries are written for correctness and simplicity, often sacrificing performance. `base58-turbo` is the first library to apply **HFT-grade optimizations** to Base58:

1.  **Instruction Level Parallelism**: Our High-Radix (58^10) processing allows the CPU to work on multiple characters simultaneously.
2.  **Matrix Multiplication**: For standard block sizes (like 32-byte hashes or 64-byte signatures), we use a mathematical shortcut that removes ~80% of the division operations.
3.  **Formal Verification**: We bridge the gap between "unsafe speed" and "Safe Rust" by using mathematical proofs (Kani) to guarantee that our optimized kernels never crash.

---

> **🛡️ Final Safety Note:**
> While other "fast" libraries often skip checks to gain speed, `base58-turbo` maintains full validation. Every character is checked against the alphabet, and every buffer access is proven safe.
