# 🛡️ Safety & Verification

**Philosophy:** `Safety > Performance > Convenience`

At `base58-turbo`, we believe that speed is meaningless if it compromises stability. While this library achieves extreme performance by leveraging `unsafe` scalar kernels and pointer arithmetic, we do not rely on "hope" or "good practices" to prevent crashes.

Instead, we rely on **Mathematical Proofs**, **Strict Formal Audits**, and **Deterministic Analysis**.

## Verification Status Matrix

We use a "Swiss Cheese" model where multiple layers of verification cover each other's blind spots.

| Architecture | MIRI (UB Check) | MSan (Uninit Check) | Kani (Math Proof) | Fuzzing (2.5B+) | Status |
| :--- | :---: | :---: | :---: | :---: | :--- |
| **Scalar Kernels** | ✅ Passed | ✅ Passed | ✅ **Proven** | ✅ Passed | **Formally Verified** |
| **Fixed Size Kernels** | ✅ Passed | ✅ Passed | ✅ **Proven** | ✅ Passed | **Formally Verified** |

## Deep Dive: The Verification Layers

### 1. Unit & Integration Testing
We maintain a comprehensive suite of tests covering:
- **Standard Vectors**: Verification against known Base58 test vectors (Bitcoin, Ripple, Flickr).
- **Cross-Validation**: Every release is tested against the `bs58` crate oracle for millions of random inputs.
- **Edge Cases**: Specific tests for leading zeros, empty inputs, and maximum payload limits.

### 2. MIRI (Undefined Behavior Analysis)
We run our comprehensive deterministic test suite under [MIRI](https://github.com/rust-lang/miri), an interpreter that checks for Undefined Behavior according to the strict Rust memory model.

*   **Checks Performed:** Strict provenance tracking, alignment checks, out-of-bounds pointer arithmetic, and data races.
*   **Coverage:** Covers **100% of execution paths** for both general scalar logic and optimized fixed-size kernels.
*   **Strategy:** We utilize deterministic input generation to force the engine into every possible boundary condition to prove safe handling of pointers at register boundaries.

### 3. MemorySanitizer (MSan)
While MIRI checks for validity, **MemorySanitizer (MSan)** checks for **Initialization**.

*   **The Threat:** In high-performance code, reading uninitialized memory is a common source of non-deterministic bugs and security leaks (Information Disclosure).
*   **The Check:** We ensure no uninitialized data leaks into the output or influences the execution path logic.
*   **Guarantee:** We ensure that our algorithms never perform logic on garbage data derived from uninitialized buffers.

### 4. Formal Verification (Kani)
We use the [Kani Model Checker](https://model-checking.github.io/kani/) to mathematically prove the correctness of our logic.
*   **Safety Proofs**: We prove that our arithmetic kernels never panic and always stay within buffer bounds for all possible inputs.

### 5. Supply Chain Security
This repository adheres to strict **Supply Chain Security** protocols.

1.  **No Direct Commits:** All changes must go through a Pull Request (PR).
2.  **Required Checks:** A PR cannot be merged unless it passes 4 mandatory gates:
    *   ✅ **Kani Verification**
    *   ✅ **MSan Audit**
    *   ✅ **MIRI Audit**
    *   ✅ **Logic/Unit Tests**
3.  **GPG Signing:** All commits are cryptographically signed.

## ❓ FAQ

**Q: Does this crate use `unsafe` Rust?**
**A:** Yes, extensively. We use pointers and optimized fixed-size matrix multiplications to achieve speed. However, all `unsafe` blocks are encapsulated behind a Safe API and have been formally audited.

**Q: Is it safe to use in Production?**
**A:** Yes. It is **proven** to be memory-safe for all supported architectures. "Safe" here isn't an opinion; it's a result of symbolic execution and sanitizer analysis.
