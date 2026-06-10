# 🏗️ Architecture & Design

This document details the internal engineering and primary design constraints of `base58-turbo`. 

## Core Constraint: Processing Multiple Bytes

Base58 conversion is fundamentally bottlenecked by arbitrary-precision division and multiplication by 58. To optimize this, the core strategy involves processing multiple bytes simultaneously using `u64` and `u32` types. However, the asymmetric nature of encoding (expanding data) versus decoding (shrinking data) dictates drastically different approaches for each pipeline.

### Encoding: The `u32` vs `u128` Bottleneck

A common question from developers reviewing the source might be code: *Why do we process `u32` chunks during encoding, when decoding processes `u64` chunks natively?*

The answer lies in type expansion during multiplication. During encoding, before the actual multiplication step, we must convert the `u32` chunk into a `u64`. This expansion provides the necessary mathematical space to perform heavy arithmetic (such as multiplication) without overflowing. Modern x64 CPUs chew through `u64` math natively without issues.

If we attempted to bump the initial byte processing to `u64` in one go, the multiplication step would require `u128` registers. This destroys performance, as CPUs struggle with native `u128` math operations, causing throughput to drop significantly.

Because we are constrained to `u32` processing in the general encoding loop, we compensate by providing **hardcoded, pre-computed paths for common sizes** (e.g., 25, 32, 64, and 69 bytes). These specialized kernels bypass the general bignum loop entirely, utilizing matrix multiplication with precomputed weights to maximize performance.

### Decoding: Native `u64` Processing

Decoding operates in the opposite direction: it *shrinks* the size of the bytes. 

Because the data is contracting, we do not face the same type-expansion constraints as encoding. This means we *can* natively process chunks as `u64` without needing to overflow into `u128` arithmetic. 

As a result, decoding achieves blazing fast performance natively—effectively doubling the throughput compared to the native encoding loop. Because the native `u64` processing extracts as much performance as possible across any payload length, **we do not use any specialized pre-computed paths or tables for decoding**. The general implementation is already running at the hardware limit without hitting a performance wall or requiring SIMD.

## Additional Optimizations

While the `u32`/`u64` asymmetry is the primary architectural driver, several other techniques are used to eliminate stalls:

### 1. 2-Byte Lookup Table (LUT)
During character emission in encoding, converting numerical values to the Base58 alphabet can cause branch mispredictions. We use a precomputed 3,364-entry table (`58 * 58`) that maps pairs of remainders directly to their 2-byte ASCII representation. We write these 2 characters (16 bits) to the output buffer in a single unaligned write, halving branch overhead.

### 2. High-Radix Base (58^10)
Standard implementations divide by 58 for every output character. We use Base 58^10 since 430,804,206,899,405,824 fits within a 64-bit integer, allowing us to process 10 characters per iteration in the bignum loop.

### 3. Vectorized Zero Handling
Base58 maps leading zeros to the first character of the alphabet (e.g., '1' in Bitcoin). We detect 8 leading zeros at once using 64-bit loads and rapidly fill the output buffer with a 64-bit pattern (e.g., `0x3131313131313131`), bypassing byte-by-byte loops.

### 4. Verification and Safety
The kernels are rigorously audited:
*   **No Allocation**: The core paths are `no_std` compatible and perform zero heap allocations.
*   **Provenance Verification**: **MIRI** is used in CI to ensure pointer arithmetic adheres strictly to the Rust memory model.
