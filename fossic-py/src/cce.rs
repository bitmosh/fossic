use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::types::py_to_json;

/// Encode a Python value using the fossic CCE canonical encoding.
///
/// This is the same encoder used internally by ``append()`` to derive the
/// content-addressed event ID. Production code rarely needs to call this
/// directly; it is available for conformance testing and tooling that needs
/// to pre-compute event IDs without committing an event.
///
/// See ``docs/implement/CCE_SPEC.md`` for the canonical encoding format.
///
/// :param value: Any JSON-serialisable Python value (int, float, str, bool,
///     None, list, dict). The value is serialised to JSON then re-parsed to
///     ``serde_json::Value`` for encoding.
/// :returns: The raw CCE-encoded bytes.
#[pyfunction]
pub fn cce_encode_value<'py>(
    py: Python<'py>,
    value: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let json_value = py_to_json(py, &value)?;
    let mut out = Vec::new();
    fossic::cce::encode_value(&mut out, &json_value)
        .map_err(|e| PyValueError::new_err(format!("CCE encode error: {e}")))?;
    Ok(PyBytes::new(py, &out))
}

/// Encode raw bytes using the fossic CCE bytes encoding (tag 0x05 + length + data).
///
/// Used to verify CCE vector cases with input type ``bytes``.
///
/// :param data: The raw bytes to encode.
/// :returns: The CCE-encoded bytes including tag and length prefix.
#[pyfunction]
pub fn cce_encode_bytes_raw<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyBytes> {
    let mut out = Vec::new();
    fossic::cce::encode_bytes(&mut out, data);
    PyBytes::new(py, &out)
}

/// Encode an f64 by its raw IEEE 754 bit pattern, applying CCE canonicalization.
///
/// CCE §3.3: NaN is canonicalized to quiet NaN (0x7FF8000000000000); negative
/// zero (0x8000000000000000) is canonicalized to positive zero. All other f64
/// values (including ±Inf and subnormals) are preserved. The result is
/// ``[0x03] ++ f64_le_bytes``.
///
/// Used to verify CCE vector cases with input type ``f64_bits``.
///
/// :param bits_hex: 16-character lowercase hex string representing the
///     big-endian u64 bit pattern of the f64.
/// :returns: The CCE-encoded bytes (9 bytes: tag 0x03 + 8 LE bytes).
#[pyfunction]
pub fn cce_encode_f64_bits<'py>(py: Python<'py>, bits_hex: &str) -> PyResult<Bound<'py, PyBytes>> {
    let raw = u64::from_str_radix(bits_hex, 16)
        .map_err(|e| PyValueError::new_err(format!("invalid bits_hex: {e}")))?;
    let f = f64::from_bits(raw);
    let canonical = if f.is_nan() {
        f64::from_bits(0x7FF8_0000_0000_0000u64)
    } else if f.to_bits() == 0x8000_0000_0000_0000u64 {
        0.0f64
    } else {
        f
    };
    let mut out = vec![0x03u8];
    out.extend_from_slice(&canonical.to_le_bytes());
    Ok(PyBytes::new(py, &out))
}
