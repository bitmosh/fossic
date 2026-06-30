//! CCE — Canonical Content Encoding.
//!
//! Purpose-built deterministic encoding for blake3 hashing. Decoupled from the
//! msgpack storage format so event identity never depends on encoder configuration.
//! See `CCE_SPEC.md` for the full specification.

use crate::error::CceError;
use unicode_normalization::UnicodeNormalization;

const TAG_NULL: u8 = 0x00;
const TAG_BOOL: u8 = 0x01;
const TAG_INT: u8 = 0x02;
const TAG_FLOAT: u8 = 0x03;
const TAG_STRING: u8 = 0x04;
const TAG_BYTES: u8 = 0x05;
const TAG_ARRAY: u8 = 0x06;
const TAG_MAP: u8 = 0x07;

const MAX_STRING_BYTES: usize = 64 * 1024 * 1024;

/// Prefix for all fossic-cce-v1 event ID derivations.
/// `"fossic-cce-v1\0"` as 14 literal bytes; the NUL is a separator no UTF-8
/// string can contain, making version collisions structurally impossible.
pub const CCE_PREFIX: &[u8] = b"fossic-cce-v1\0";

// ── Public encoding surface ────────────────────────────────────────────────────

/// Encode a `serde_json::Value` to CCE bytes and append to `out`.
pub fn encode_value(out: &mut Vec<u8>, value: &serde_json::Value) -> Result<(), CceError> {
    match value {
        serde_json::Value::Null => {
            out.push(TAG_NULL);
        }
        serde_json::Value::Bool(b) => {
            out.push(TAG_BOOL);
            out.push(if *b { 0x01 } else { 0x00 });
        }
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                encode_i64(out, i);
            } else if let Some(u) = n.as_u64() {
                return Err(CceError::U64Overflow(u));
            } else {
                let f = n
                    .as_f64()
                    .expect("serde_json Number must be i64, u64, or f64");
                encode_f64(out, f);
            }
        }
        serde_json::Value::String(s) => {
            encode_string(out, s)?;
        }
        serde_json::Value::Array(arr) => {
            out.push(TAG_ARRAY);
            let count = arr.len() as u32;
            out.extend_from_slice(&count.to_le_bytes());
            for item in arr {
                encode_value(out, item)?;
            }
        }
        serde_json::Value::Object(map) => {
            let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(map.len());
            for (k, v) in map {
                let mut key_bytes = Vec::new();
                encode_string(&mut key_bytes, k)?;
                let mut val_bytes = Vec::new();
                encode_value(&mut val_bytes, v)?;
                pairs.push((key_bytes, val_bytes));
            }
            // Sort by byte-lex of the CCE-encoded key (§3.4).
            pairs.sort_unstable_by(|a, b| a.0.cmp(&b.0));
            // Duplicate keys are a conformance error.
            for w in pairs.windows(2) {
                if w[0].0 == w[1].0 {
                    return Err(CceError::DuplicateKeys);
                }
            }
            out.push(TAG_MAP);
            out.extend_from_slice(&(pairs.len() as u32).to_le_bytes());
            for (k, v) in pairs {
                out.extend_from_slice(&k);
                out.extend_from_slice(&v);
            }
        }
    }
    Ok(())
}

/// Encode a string: `TAG_STRING || u32_LE(len) || NFC(utf8_bytes)`.
pub fn encode_string(out: &mut Vec<u8>, s: &str) -> Result<(), CceError> {
    let normalized: String = s.nfc().collect();
    let bytes = normalized.as_bytes();
    if bytes.len() > MAX_STRING_BYTES {
        return Err(CceError::StringTooLarge(bytes.len()));
    }
    out.push(TAG_STRING);
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

/// Encode raw bytes: `TAG_BYTES || u32_LE(len) || bytes`.
pub fn encode_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.push(TAG_BYTES);
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

/// Encode `Option<&[u8]>`: `TAG_NULL` if `None`, else `encode_bytes`.
pub fn encode_optional_bytes(out: &mut Vec<u8>, bytes: Option<&[u8]>) {
    match bytes {
        None => out.push(TAG_NULL),
        Some(b) => encode_bytes(out, b),
    }
}

/// Encode a u32 as i64 (per §3.1: all integers normalize to i64).
pub fn encode_u32_as_i64(out: &mut Vec<u8>, v: u32) {
    encode_i64(out, v as i64);
}

// ── Event ID derivation ───────────────────────────────────────────────────────

/// Derive the content-addressed event ID per CCE_SPEC §4.
///
/// ```text
/// event_id = blake3(
///     "fossic-cce-v1\0"
///     || cce_encode_string(event_type)
///     || cce_encode_uint_as_i64(type_version)
///     || cce_encode_optional_bytes(causation_id)
///     || cce_encode(payload)
/// )
/// ```
pub fn derive_event_id(
    event_type: &str,
    type_version: u32,
    causation_id: Option<&[u8; 32]>,
    payload: &serde_json::Value,
) -> Result<[u8; 32], CceError> {
    let mut buf = Vec::with_capacity(256);
    buf.extend_from_slice(CCE_PREFIX);
    encode_string(&mut buf, event_type)?;
    encode_u32_as_i64(&mut buf, type_version);
    encode_optional_bytes(&mut buf, causation_id.map(|b| b.as_slice()));
    encode_value(&mut buf, payload)?;
    Ok(*blake3::hash(&buf).as_bytes())
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn encode_i64(out: &mut Vec<u8>, v: i64) {
    out.push(TAG_INT);
    out.extend_from_slice(&v.to_le_bytes());
}

fn encode_f64(out: &mut Vec<u8>, v: f64) {
    out.push(TAG_FLOAT);
    out.extend_from_slice(&canonicalize_f64(v).to_le_bytes());
}

pub(crate) fn canonicalize_f64(v: f64) -> f64 {
    if v.is_nan() {
        // All NaN values → canonical quiet NaN 0x7FF8000000000000.
        f64::from_bits(0x7FF8_0000_0000_0000u64)
    } else if v == 0.0 {
        // -0.0 → +0.0 (both compare equal to 0.0).
        0.0f64
    } else {
        v
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(v: &serde_json::Value) -> Vec<u8> {
        let mut out = Vec::new();
        encode_value(&mut out, v).expect("encode_value failed");
        out
    }

    #[test]
    fn null() {
        assert_eq!(enc(&serde_json::Value::Null), vec![0x00]);
    }

    #[test]
    fn bool_values() {
        assert_eq!(enc(&serde_json::json!(true)), vec![0x01, 0x01]);
        assert_eq!(enc(&serde_json::json!(false)), vec![0x01, 0x00]);
    }

    #[test]
    fn integer_zero() {
        assert_eq!(
            enc(&serde_json::json!(0)),
            vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn integer_one() {
        assert_eq!(
            enc(&serde_json::json!(1)),
            vec![0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn integer_negative() {
        assert_eq!(
            enc(&serde_json::json!(-1)),
            vec![0x02, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
        );
    }

    #[test]
    fn integer_i64_min() {
        assert_eq!(
            enc(&serde_json::json!(-9223372036854775808i64)),
            vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80]
        );
    }

    #[test]
    fn float_zero() {
        assert_eq!(
            enc(&serde_json::json!(0.0f64)),
            vec![0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn float_negative_zero_canonicalizes() {
        let neg_zero = serde_json::Number::from_f64(-0.0f64).unwrap();
        let neg_zero_val = serde_json::Value::Number(neg_zero);
        // -0.0 must encode identically to +0.0
        assert_eq!(enc(&neg_zero_val), enc(&serde_json::json!(0.0f64)));
    }

    #[test]
    fn float_one() {
        let expected: Vec<u8> = {
            let bits = 1.0f64.to_le_bytes();
            let mut v = vec![0x03];
            v.extend_from_slice(&bits);
            v
        };
        assert_eq!(enc(&serde_json::json!(1.0f64)), expected);
    }

    #[test]
    fn nan_canonicalizes_to_quiet_nan() {
        // serde_json::Number::from_f64 rejects NaN (returns None), so we test
        // canonicalize_f64 directly. The JSON-value path is covered by the
        // f64_bits vectors in cce_vectors.rs.
        let canonical_bits = 0x7FF8_0000_0000_0000u64;
        assert_eq!(
            canonicalize_f64(f64::NAN).to_bits(),
            canonical_bits,
            "quiet NaN"
        );
        // Any NaN bit pattern should map to the same canonical quiet NaN.
        let signaling = f64::from_bits(0x7FF0_0000_0000_0001u64);
        assert_eq!(
            canonicalize_f64(signaling).to_bits(),
            canonical_bits,
            "signaling NaN"
        );
    }

    #[test]
    fn string_empty() {
        assert_eq!(
            enc(&serde_json::json!("")),
            vec![0x04, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn string_hello() {
        let mut expected = vec![0x04, 0x05, 0x00, 0x00, 0x00];
        expected.extend_from_slice(b"hello");
        assert_eq!(enc(&serde_json::json!("hello")), expected);
    }

    #[test]
    fn string_nfc_normalization() {
        // "café" can be written as NFC (U+00E9) or NFD (e + combining accent).
        // CCE must produce the same output for both.
        let nfc = "caf\u{00E9}";
        let nfd = "cafe\u{0301}";
        let mut out_nfc = Vec::new();
        let mut out_nfd = Vec::new();
        encode_string(&mut out_nfc, nfc).unwrap();
        encode_string(&mut out_nfd, nfd).unwrap();
        assert_eq!(out_nfc, out_nfd);
    }

    #[test]
    fn array_empty() {
        assert_eq!(
            enc(&serde_json::json!([])),
            vec![0x06, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn map_empty() {
        assert_eq!(
            enc(&serde_json::json!({})),
            vec![0x07, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn map_sorted_by_cce_encoded_key() {
        // {"b": 2, "a": 1} and {"a": 1, "b": 2} must produce identical bytes.
        // (serde_json::Map preserves insertion order, so we construct both orders)
        let ba = serde_json::json!({"b": 2, "a": 1});
        let ab = serde_json::json!({"a": 1, "b": 2});
        assert_eq!(enc(&ba), enc(&ab));
    }

    #[test]
    fn event_id_is_deterministic() {
        let id1 = derive_event_id("TestEvent", 1, None, &serde_json::json!({"x": 1})).unwrap();
        let id2 = derive_event_id("TestEvent", 1, None, &serde_json::json!({"x": 1})).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn event_id_differs_with_causation() {
        let no_cause = derive_event_id("TestEvent", 1, None, &serde_json::json!({"x": 1})).unwrap();
        let with_cause = derive_event_id(
            "TestEvent",
            1,
            Some(&[0xABu8; 32]),
            &serde_json::json!({"x": 1}),
        )
        .unwrap();
        assert_ne!(no_cause, with_cause);
    }

    #[test]
    fn event_id_differs_with_type_version() {
        let v1 = derive_event_id("TestEvent", 1, None, &serde_json::json!({})).unwrap();
        let v2 = derive_event_id("TestEvent", 2, None, &serde_json::json!({})).unwrap();
        assert_ne!(v1, v2);
    }
}
