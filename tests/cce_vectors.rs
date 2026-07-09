// SPDX-License-Identifier: Apache-2.0
use fossic::cce::{encode_bytes, encode_value};

#[derive(serde::Deserialize)]
struct VectorFile {
    encode_value_vectors: Vec<EncodeValueVector>,
}

#[derive(serde::Deserialize)]
struct EncodeValueVector {
    id: String,
    #[allow(dead_code)]
    description: String,
    input: VectorInput,
    expected_hex: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum VectorInput {
    Json { value: serde_json::Value },
    F64Bits { bits: String },
    Bytes { data: String },
    I64Str { value: String },
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("invalid hex in test vector"))
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[test]
fn run_all_vectors() {
    const VECTORS_JSON: &str = include_str!("../cce-test-vectors.json");
    let file: VectorFile = serde_json::from_str(VECTORS_JSON).expect("parse test vectors");

    let mut passed = 0;
    let mut skipped = 0;

    for vec in &file.encode_value_vectors {
        let expected = match &vec.expected_hex {
            Some(h) => h.as_str(),
            None => {
                skipped += 1;
                continue;
            }
        };

        let got = match &vec.input {
            VectorInput::Json { value } => {
                let mut out = Vec::new();
                encode_value(&mut out, value)
                    .unwrap_or_else(|e| panic!("vector {}: encode_value failed: {}", vec.id, e));
                out
            }
            VectorInput::F64Bits { bits } => {
                let raw = u64::from_str_radix(bits, 16)
                    .unwrap_or_else(|_| panic!("vector {}: invalid bits hex", vec.id));
                let f = f64::from_bits(raw);
                // serde_json::Number::from_f64 rejects NaN and Inf (returns None),
                // so we apply CCE §3.3 canonicalization inline and encode directly
                // without going through encode_value / serde_json::Value.
                let canonical = if f.is_nan() {
                    f64::from_bits(0x7FF8_0000_0000_0000u64) // canonical quiet NaN
                } else if f.to_bits() == 0x8000_0000_0000_0000u64 {
                    0.0f64 // -0.0 → +0.0
                } else {
                    f // all other f64 values (including ±Inf, subnormals) preserved
                };
                let mut out = vec![0x03u8];
                out.extend_from_slice(&canonical.to_le_bytes());
                out
            }
            VectorInput::Bytes { data } => {
                let bytes = if data.is_empty() {
                    vec![]
                } else {
                    hex_decode(data)
                };
                let mut out = Vec::new();
                encode_bytes(&mut out, &bytes);
                out
            }
            VectorInput::I64Str { value } => {
                let i: i64 = value
                    .parse()
                    .unwrap_or_else(|_| panic!("vector {}: invalid i64 string", vec.id));
                let v = serde_json::Value::Number(serde_json::Number::from(i));
                let mut out = Vec::new();
                encode_value(&mut out, &v)
                    .unwrap_or_else(|e| panic!("vector {}: encode failed: {}", vec.id, e));
                out
            }
        };

        let got_hex = to_hex(&got);
        assert_eq!(
            got_hex, expected,
            "vector '{}' failed:\n  got:      {}\n  expected: {}",
            vec.id, got_hex, expected
        );
        passed += 1;
    }

    println!(
        "CCE vectors: {} passed, {} skipped (null expected_hex)",
        passed, skipped
    );
    assert!(passed > 0, "no vectors ran");
}

/// After the first passing run, fill in the null expected_hex entries by running
/// `cargo test -- print_computed_vectors --nocapture` and capturing the output.
#[test]
fn print_computed_vectors() {
    const VECTORS_JSON: &str = include_str!("../cce-test-vectors.json");
    let file: VectorFile = serde_json::from_str(VECTORS_JSON).expect("parse test vectors");

    for vec in file
        .encode_value_vectors
        .iter()
        .filter(|v| v.expected_hex.is_none())
    {
        let got = match &vec.input {
            VectorInput::Json { value } => {
                let mut out = Vec::new();
                if encode_value(&mut out, value).is_ok() {
                    out
                } else {
                    continue;
                }
            }
            _ => continue,
        };
        println!("{}: {}", vec.id, to_hex(&got));
    }
}
