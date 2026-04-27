# 📚 Technical Documentation

This directory contains detailed technical reports, formal verification proofs, and architectural decision records for `base58-turbo`.

## 📂 Index

### 🛡️ [Safety & Verification](verification.md)
**Target Audience:** Security Auditors, Systems Engineers
*   **Fuzzing & Memory Checks:** How we use continuous fuzzing and MemorySanitizer to ensure safety.
*   **UB Checks:** Details on MIRI usage and strict provenance.
*   **Threat Model:** What we protect against and our trust boundaries.

### 🏗️ [Architecture & Design](design.md)
**Target Audience:** Contributors, Curious Developers
*   **Matrix Multiplication Arithmetic:** How we use precomputed weights to speed up encoding.
*   **High-Radix Processing:** Our 58^5 and 58^10 optimization strategies.
*   **2-Byte LUT:** How we reduce branch pressure during character emission.

### ❓ [Frequently Asked Questions (FAQ)](faq.md)
**Target Audience:** All Users
*   **Integration:** Using `no_std` and embedded environments.
*   **Performance:** Why Base58 is slower than Base64 and how we optimize it.
*   **Limits:** Information about the 1024-byte/2048-byte input limits for encoding and decoding.
