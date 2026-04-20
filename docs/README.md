# 📚 Technical Documentation

This directory contains detailed technical reports, formal verification proofs, and architectural decision records for `base58-turbo`.

## 📂 Index

### 🛡️ [Safety & Verification](verification.md)
**Target Audience:** Security Auditors, Systems Engineers
*   **Formal Verification:** How we use Kani to mathematically prove the absence of panics/overflows.
*   **UB Checks:** Details on MIRI usage and strict provenance.
*   **Threat Model:** What we protect against and our trust boundaries.

### 🏗️ [Architecture & Design](design.md)
**Target Audience:** Contributors, Curious Developers
*   **Matrix Multiplication Arithmetic:** How we use precomputed weights to speed up encoding.
*   **High-Radix Processing:** Our 58^5 and 58^10 optimization strategies.
*   **2-Byte LUT:** How we reduce branch pressure during character emission.

### ⚖️ [Ecosystem Comparison](ecosystem_comparison.md)
**Target Audience:** Architects, CTOs
*   **Turbo-Grade Optimization:** Why `base58-turbo` is faster than naive bignum implementations.
*   **Safety Matrix:** Feature matrix comparing `base58-turbo` against `bs58` and other alternatives.

### ❓ [Frequently Asked Questions (FAQ)](faq.md)
**Target Audience:** All Users
*   **Integration:** Using `no_std` and embedded environments.
*   **Performance:** Why Base58 is slower than Base64 and how we optimize it.
*   **Limits:** Information about the 1024-byte input limit.
