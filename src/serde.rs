//! # Serde Integration for `base58-turbo`
//!
//! **Transparent, zero-overhead, high-performance** conversion between Base58 strings
//! and binary data types (`Vec<u8>`, `[u8; N]`, `&[u8]`, `bytes::Bytes`, `Cow<[u8]>`, etc.).
//!
//! This module is only compiled when the **`serde`** feature is enabled.
//!
//! ## Why this exists
//!
//! When working with cryptographic data, blockchain payloads, API responses, etc.,
//! you almost always want to **store `Vec<u8>` internally** (for speed & memory efficiency)
//! but **serialize/deserialize as clean Base58 strings** in JSON, YAML, TOML, Bincode, etc.
//!
//! This module gives you exactly that with **one line of attribute** and zero boilerplate.
//!
//! ## Universal Alphabet
//!
//! `base58-turbo` exposes the standard universal Bitcoin Base58 alphabet for Serde operations,
//! as it is the most widely used format across systems (Bitcoin, Solana, IPFS, Monero, etc.).
//!
//! ## Support for Fixed-Size Arrays
//!
//! For fixed-length binary data like public keys, hashes, or seeds,
//! use the specialized modules: `base58_24`, `base58_32`, `base58_48`, or `base58_64`.
//!
//! These enforce **exact length** during deserialization, failing with a descriptive error
//! if the decoded data isn't precisely the expected size.
//!
//! ## Full Example
//!
//! ```rust
//! use serde::{Serialize, Deserialize};
//! use base58_turbo::serde::base58;       // Variable length (Vec<u8>)
//! use base58_turbo::serde::base58_32;    // Fixed 32 bytes ([u8; 32])
//! use base58_turbo::serde::base58_64;    // Fixed 64 bytes ([u8; 64])
//!
//! #[derive(Serialize, Deserialize, Debug)]
//! struct BlockHeader {
//!     /// Automatically becomes a Base58 string in JSON
//!     #[serde(with = "base58")]
//!     block_hash: Vec<u8>,
//!
//!     /// Fixed-size array with length enforcement (e.g. Solana Pubkey)
//!     #[serde(with = "base58_32")]
//!     public_key: [u8; 32],
//!
//!     /// Fixed 64-byte signature
//!     #[serde(with = "base58_64")]
//!     signature: [u8; 64],
//! }
//! ```
//!
//! Works with **any** Serde format (JSON, YAML, TOML, MessagePack, Bincode, Postcard…).

use serde::{de, Deserializer, Serializer};

use crate::BITCOIN;

/// **Universal** Base58 serialization (Variable Length).
///
/// This submodule provides Serde helper functions that convert binary data
/// to/from Base58 strings using the universal `BITCOIN` alphabet.
pub mod base58 {
    use super::*;

    /// Serializes any byte container as a Base58 string.
    ///
    /// ## Supported input types (serialization)
    ///
    /// - `Vec<u8>`
    /// - `&[u8]`
    /// - `[u8; N]` for any constant `N`
    /// - `&[u8; N]`
    /// - `Cow<[u8]>`
    /// - `bytes::Bytes`, `bytes::BytesMut`
    /// - Any type that implements `AsRef<[u8]>`
    ///
    /// ## Performance
    ///
    /// Uses the highly-optimized scalar path of `base58-turbo`.
    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: AsRef<[u8]>,
        S: Serializer,
    {
        let encoded = BITCOIN.encode(value.as_ref()).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&encoded)
    }

    /// Deserializes a Base58 string (or byte slice) into `Vec<u8>`.
    ///
    /// ## Accepted input formats
    ///
    /// - JSON string: `"JxF12TrwUP45BMd"`
    /// - `&str`
    /// - Raw bytes provided by some serializers (`visit_bytes`)
    ///
    /// ## Error handling
    ///
    /// Returns a clear Serde error with a helpful message when:
    /// - An invalid Base58 character is found
    /// - The input is too large (over 2048 bytes)
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Base58Visitor;

        impl<'de> de::Visitor<'de> for Base58Visitor {
            type Value = Vec<u8>;

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str(
                    "a valid Base58 string"
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<Vec<u8>, E>
            where
                E: de::Error,
            {
                BITCOIN.decode(value).map_err(de::Error::custom)
            }

            fn visit_string<E>(self, value: String) -> Result<Vec<u8>, E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<Vec<u8>, E>
            where
                E: de::Error,
            {
                BITCOIN.decode(value).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(Base58Visitor)
    }
}

/// **Universal** Base58 for fixed-size `[u8; 24]`.
pub mod base58_24 {
    use super::*;

    /// Serializes a 24-byte container as a Base58 string.
    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: AsRef<[u8]>,
        S: Serializer,
    {
        let bytes = value.as_ref();
        if bytes.len() != 24 {
            return Err(serde::ser::Error::custom(format!("expected 24 bytes, got {}", bytes.len())));
        }
        let encoded = BITCOIN.encode(bytes).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&encoded)
    }

    /// Deserializes a Base58 string into exactly `[u8; 24]`.
    ///
    /// Fails if the decoded data is not precisely 24 bytes.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 24], D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Base58Visitor;

        impl<'de> de::Visitor<'de> for Base58Visitor {
            type Value = [u8; 24];

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("a valid Base58 string representing exactly 24 bytes")
            }

            fn visit_str<E>(self, value: &str) -> Result<[u8; 24], E>
            where
                E: de::Error,
            {
                let vec = BITCOIN.decode(value).map_err(de::Error::custom)?;
                if vec.len() != 24 {
                    return Err(de::Error::custom(format!("expected 24 bytes, got {}", vec.len())));
                }
                Ok(vec.try_into().expect("length already checked"))
            }

            fn visit_string<E>(self, value: String) -> Result<[u8; 24], E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<[u8; 24], E>
            where
                E: de::Error,
            {
                let vec = BITCOIN.decode(value).map_err(de::Error::custom)?;
                if vec.len() != 24 {
                    return Err(de::Error::custom(format!("expected 24 bytes, got {}", vec.len())));
                }
                Ok(vec.try_into().expect("length already checked"))
            }
        }

        deserializer.deserialize_str(Base58Visitor)
    }
}

/// **Universal** Base58 for fixed-size `[u8; 32]`.
pub mod base58_32 {
    use super::*;

    /// Serializes a 32-byte container as a Base58 string.
    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: AsRef<[u8]>,
        S: Serializer,
    {
        let bytes = value.as_ref();
        if bytes.len() != 32 {
            return Err(serde::ser::Error::custom(format!("expected 32 bytes, got {}", bytes.len())));
        }
        let encoded = BITCOIN.encode(bytes).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&encoded)
    }

    /// Deserializes a Base58 string into exactly `[u8; 32]`.
    ///
    /// Fails if the decoded data is not precisely 32 bytes.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Base58Visitor;

        impl<'de> de::Visitor<'de> for Base58Visitor {
            type Value = [u8; 32];

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("a valid Base58 string representing exactly 32 bytes")
            }

            fn visit_str<E>(self, value: &str) -> Result<[u8; 32], E>
            where
                E: de::Error,
            {
                let vec = BITCOIN.decode(value).map_err(de::Error::custom)?;
                if vec.len() != 32 {
                    return Err(de::Error::custom(format!("expected 32 bytes, got {}", vec.len())));
                }
                Ok(vec.try_into().expect("length already checked"))
            }

            fn visit_string<E>(self, value: String) -> Result<[u8; 32], E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<[u8; 32], E>
            where
                E: de::Error,
            {
                let vec = BITCOIN.decode(value).map_err(de::Error::custom)?;
                if vec.len() != 32 {
                    return Err(de::Error::custom(format!("expected 32 bytes, got {}", vec.len())));
                }
                Ok(vec.try_into().expect("length already checked"))
            }
        }

        deserializer.deserialize_str(Base58Visitor)
    }
}

/// **Universal** Base58 for fixed-size `[u8; 48]`.
pub mod base58_48 {
    use super::*;

    /// Serializes a 48-byte container as a Base58 string.
    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: AsRef<[u8]>,
        S: Serializer,
    {
        let bytes = value.as_ref();
        if bytes.len() != 48 {
            return Err(serde::ser::Error::custom(format!("expected 48 bytes, got {}", bytes.len())));
        }
        let encoded = BITCOIN.encode(bytes).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&encoded)
    }

    /// Deserializes a Base58 string into exactly `[u8; 48]`.
    ///
    /// Fails if the decoded data is not precisely 48 bytes.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 48], D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Base58Visitor;

        impl<'de> de::Visitor<'de> for Base58Visitor {
            type Value = [u8; 48];

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("a valid Base58 string representing exactly 48 bytes")
            }

            fn visit_str<E>(self, value: &str) -> Result<[u8; 48], E>
            where
                E: de::Error,
            {
                let vec = BITCOIN.decode(value).map_err(de::Error::custom)?;
                if vec.len() != 48 {
                    return Err(de::Error::custom(format!("expected 48 bytes, got {}", vec.len())));
                }
                Ok(vec.try_into().expect("length already checked"))
            }

            fn visit_string<E>(self, value: String) -> Result<[u8; 48], E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<[u8; 48], E>
            where
                E: de::Error,
            {
                let vec = BITCOIN.decode(value).map_err(de::Error::custom)?;
                if vec.len() != 48 {
                    return Err(de::Error::custom(format!("expected 48 bytes, got {}", vec.len())));
                }
                Ok(vec.try_into().expect("length already checked"))
            }
        }

        deserializer.deserialize_str(Base58Visitor)
    }
}

/// **Universal** Base58 for fixed-size `[u8; 64]`.
pub mod base58_64 {
    use super::*;

    /// Serializes a 64-byte container as a Base58 string.
    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: AsRef<[u8]>,
        S: Serializer,
    {
        let bytes = value.as_ref();
        if bytes.len() != 64 {
            return Err(serde::ser::Error::custom(format!("expected 64 bytes, got {}", bytes.len())));
        }
        let encoded = BITCOIN.encode(bytes).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&encoded)
    }

    /// Deserializes a Base58 string into exactly `[u8; 64]`.
    ///
    /// Fails if the decoded data is not precisely 64 bytes.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 64], D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Base58Visitor;

        impl<'de> de::Visitor<'de> for Base58Visitor {
            type Value = [u8; 64];

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("a valid Base58 string representing exactly 64 bytes")
            }

            fn visit_str<E>(self, value: &str) -> Result<[u8; 64], E>
            where
                E: de::Error,
            {
                let vec = BITCOIN.decode(value).map_err(de::Error::custom)?;
                if vec.len() != 64 {
                    return Err(de::Error::custom(format!("expected 64 bytes, got {}", vec.len())));
                }
                Ok(vec.try_into().expect("length already checked"))
            }

            fn visit_string<E>(self, value: String) -> Result<[u8; 64], E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<[u8; 64], E>
            where
                E: de::Error,
            {
                let vec = BITCOIN.decode(value).map_err(de::Error::custom)?;
                if vec.len() != 64 {
                    return Err(de::Error::custom(format!("expected 64 bytes, got {}", vec.len())));
                }
                Ok(vec.try_into().expect("length already checked"))
            }
        }

        deserializer.deserialize_str(Base58Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use serde::de::{IntoDeserializer, value::{StringDeserializer, BytesDeserializer, Error as ValueError}};

    // ======================================================================
    // Test Structs
    // ======================================================================

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct VarLenPayload {
        #[serde(with = "base58")]
        data: Vec<u8>,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Fixed24Payload { #[serde(with = "base58_24")] data: [u8; 24], }
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Fixed32Payload { #[serde(with = "base58_32")] data: [u8; 32], }
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Fixed48Payload { #[serde(with = "base58_48")] data: [u8; 48], }
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Fixed64Payload { #[serde(with = "base58_64")] data: [u8; 64], }

    #[derive(Debug, Serialize)] struct Dyn24<'a> { #[serde(with = "base58_24")] data: &'a [u8], }
    #[derive(Debug, Serialize)] struct Dyn32<'a> { #[serde(with = "base58_32")] data: &'a [u8], }
    #[derive(Debug, Serialize)] struct Dyn48<'a> { #[serde(with = "base58_48")] data: &'a [u8], }
    #[derive(Debug, Serialize)] struct Dyn64<'a> { #[serde(with = "base58_64")] data: &'a [u8], }

    // ======================================================================
    // 1. Variable Length Tests
    // ======================================================================

    #[test]
    fn test_var_len_success() {
        let payload = VarLenPayload { data: b"Hello World".to_vec() };
        let serialized = serde_json::to_string(&payload).unwrap();
        assert_eq!(serialized, r#"{"data":"JxF12TrwUP45BMd"}"#);
        assert_eq!(payload, serde_json::from_str(&serialized).unwrap());
    }

    #[test]
    fn test_var_len_serialization_size_limit() {
        let payload = VarLenPayload { data: vec![0u8; 1025] };
        assert!(serde_json::to_string(&payload).is_err());
    }

    #[test]
    fn test_var_len_wrong_type_expecting() {
        let res: Result<VarLenPayload, _> = serde_json::from_str(r#"{"data":123}"#);
        assert!(res.unwrap_err().to_string().contains("a valid Base58 string"));
    }

    // ======================================================================
    // 2. Exhaustive Visitor Branch Coverage
    // ======================================================================

    #[test]
    fn test_exhaustive_visitors_var_len() {
        // visit_string
        let s = "JxF12TrwUP45BMd".to_string();
        let de_str: StringDeserializer<ValueError> = s.into_deserializer();
        assert!(base58::deserialize(de_str).is_ok());

        // visit_bytes
        let b = b"JxF12TrwUP45BMd".as_slice();
        let de_bytes: BytesDeserializer<'_, ValueError> = b.into_deserializer();
        assert!(base58::deserialize(de_bytes).is_ok());

        // Error Path: Invalid Chars
        let s_err = "0OIl".to_string();
        let de_err: StringDeserializer<ValueError> = s_err.into_deserializer();
        assert!(base58::deserialize(de_err).is_err());
    }

    macro_rules! test_fixed_visitors {
        ($module:ident, $size:expr) => {
            let valid_str = BITCOIN.encode(&[$size as u8; $size]).unwrap();
            let wrong_len_str = BITCOIN.encode(&[0u8; 1]).unwrap();

            // Success Paths
            let de_s: StringDeserializer<ValueError> = valid_str.clone().into_deserializer();
            assert!($module::deserialize(de_s).is_ok());
            
            let de_b: BytesDeserializer<'_, ValueError> = valid_str.as_bytes().into_deserializer();
            assert!($module::deserialize(de_b).is_ok());

            // Error Paths: Length
            let de_bad_s: StringDeserializer<ValueError> = wrong_len_str.clone().into_deserializer();
            assert!($module::deserialize(de_bad_s).is_err());

            // Error Paths: Invalid Chars
            let de_inv_s: StringDeserializer<ValueError> = "0OIl".to_string().into_deserializer();
            assert!($module::deserialize(de_inv_s).is_err());
        };
    }

    #[test]
    fn test_exhaustive_visitors_fixed_lens() {
        test_fixed_visitors!(base58_24, 24);
        test_fixed_visitors!(base58_32, 32);
        test_fixed_visitors!(base58_48, 48);
        test_fixed_visitors!(base58_64, 64);
    }

    // ======================================================================
    // 3. Length Constraint Tests
    // ======================================================================

    #[test]
    fn test_serialize_length_mismatch() {
        assert!(serde_json::to_string(&Dyn24 { data: &[0u8; 2] }).is_err());
        assert!(serde_json::to_string(&Dyn32 { data: &[0u8; 2] }).is_err());
        assert!(serde_json::to_string(&Dyn48 { data: &[0u8; 2] }).is_err());
        assert!(serde_json::to_string(&Dyn64 { data: &[0u8; 2] }).is_err());
    }

    #[test]
    fn test_fixed_lengths_wrong_type_expecting() {
        assert!(serde_json::from_str::<Fixed24Payload>(r#"{"data":0}"#).is_err());
        assert!(serde_json::from_str::<Fixed32Payload>(r#"{"data":0}"#).is_err());
        assert!(serde_json::from_str::<Fixed48Payload>(r#"{"data":0}"#).is_err());
        assert!(serde_json::from_str::<Fixed64Payload>(r#"{"data":0}"#).is_err());
    }
}
