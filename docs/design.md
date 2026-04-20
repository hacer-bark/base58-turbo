# 🏗️ Architecture & Design

This document details the internal engineering of `base58-turbo`. The design goal is to maximize throughput for **Memory-Safe Rust** by leveraging high-radix arithmetic, matrix multiplication, and minimizing CPU pipeline stalls.

## Design Philosophy: Reducing Division Pressure

Base58 is inherently slower than Base2-based encodings (like Base64) because it requires arbitrary-precision division by 58. On modern CPUs, integer division is one of the most expensive operations (20-80 cycles depending on the architecture).

`base58-turbo` optimizes this by:
1.  **Batch Processing**: Processing multiple bytes at once to reduce the number of bignum iterations.
2.  **High-Radix Arithmetic**: Using Base 58^5 and 58^10 digits instead of Base 58^1.
3.  **Matrix Multiplication Arithmetic**: Using precomputed weights to avoid divisions for common input sizes.

## 1. Matrix Multiplication Arithmetic (Encoding)

For common input sizes (25, 32, 64, 69 bytes), we bypass the standard "multiply-and-add" bignum loop. Instead, we treat the input as a vector and multiply it by a precomputed matrix of weights.

*   **Precomputed Weights**: For each input byte position, we precalculate its contribution to the final Base 58^5 digits.
*   **128-bit Accumulation**: We use `u128` to accumulate the sums of products, ensuring no overflow occurs during the matrix multiplication.
*   **Single-Pass Reduction**: A final reduction pass handles carries between digits, requiring significantly fewer divisions than the naive approach.

## 2. High-Radix Radix (58^10)

Standard Base58 implementations divide the entire bignum by 58 for every single output character. `base58-turbo` uses a higher radix:

*   **Base 58^10**: Since 58^10 (430,804,206,899,405,824) fits within a 64-bit integer, we can process 10 characters at a time in the bignum loop.
*   **Loop Unrolling**: We unroll the bignum multiplication and addition to maximize **Instruction Level Parallelism (ILP)**.
*   **SWAR-like Processing**: We treat 64-bit words as digits, allowing us to move 8 bytes of internal state with a single instruction.

## 3. 2-Byte Lookup Table (LUT)

During character emission (encoding), converting a numerical value to its alphabet representation is a bottleneck due to branch misprediction.

*   **Squared LUT**: We precompute a 3,364-entry table (`58 * 58`) that maps pairs of remainders to their 2-byte ASCII representations.
*   **Vectorized Emission**: We write 2 characters (16 bits) to the output buffer in a single unaligned write. This halves the number of branches and memory operations in the hot path.

## 4. Vectorized Zero Handling

Base58 has a unique rule where leading zeros in the input map 1:1 to the first character of the alphabet (e.g., '1' in Bitcoin).

*   **64-bit Pattern Matching**: We check for 8 leading zeros at once using 64-bit loads and comparisons.
*   **Vectorized Fill**: We use 64-bit patterns (e.g., `0x3131313131313131` for '1') to rapidly fill the output buffer, bypassing byte-by-byte loops.

## 5. Memory Safety & Verification

While we leverage `unsafe` pointers and intrinsics for speed, the codebase is strictly audited:

*   **Boundary Handling**: Every kernel is verified via **Kani Model Checking** to ensure it never reads or writes beyond its assigned slices.
*   **No Allocation**: The core kernels are `no_std` compatible and perform zero heap allocations, ensuring deterministic performance and memory usage.
*   **Provenance Verification**: **MIRI** is used in CI to ensure that pointer arithmetic adheres to the strict Rust memory model.

## Summary

`base58-turbo` bridges the gap between the flexibility of Rust and the raw performance of handcrafted assembly. By focusing on reducing division pressure and maximizing memory bandwidth utilization, it provides a state-of-the-art Base58 engine for performance-critical applications.
