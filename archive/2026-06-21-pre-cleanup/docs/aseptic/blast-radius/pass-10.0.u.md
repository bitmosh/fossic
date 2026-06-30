---
pass: 10.0.u
version: v0.10.0.u
date: 2026-06-12
summary: Expose CCE encoder from fossic-py and add parametrized conformance harness against all 30 canonical vectors.
---

# Blast Radius — Pass 10.0.u (v0.10.0.u)

## Files

### Modified
- `fossic-py/src/lib.rs` — added `mod cce;` and registered 3 CCE functions in the `_fossic` module
- `fossic-py/python/fossic/__init__.py` — imported `cce_encode_value`, `cce_encode_bytes_raw`, `cce_encode_f64_bits` from `_fossic`; added to `__all__`
- `docs/aseptic/TECH_DEBT.md` — TD-005 added (Python CCE conformance gap) and immediately resolved
- `docs/aseptic/README.md` — version bumped to `v0.10.0.u`

### Created
- `fossic-py/src/cce.rs` — PyO3 wrappers for the fossic Rust CCE encoder: `cce_encode_value`, `cce_encode_bytes_raw`, `cce_encode_f64_bits`
- `fossic-py/tests/test_cce_vectors.py` — parametrized conformance harness; 29 vectors pass, 1 skipped (`expected_hex: null`)
- `docs/aseptic/blast-radius/pass-10.0.u.md` — this file

### Deleted
- (none)

---

## Public APIs

### Added
- `cce_encode_value(value: Any) -> bytes` — CCE-encodes any JSON-serialisable Python value; same path used internally by `append()`
- `cce_encode_bytes_raw(data: bytes) -> bytes` — CCE bytes encoding (tag 0x05 + varint length + data); for testing vectors with input type `bytes`
- `cce_encode_f64_bits(bits_hex: str) -> bytes` — CCE f64 encoding from raw IEEE 754 bit pattern; applies NaN and negative-zero canonicalization

All three are importable from `fossic` directly.

### Modified (breaking)
- (none)

### Modified (non-breaking)
- (none)

### Removed
- (none)

---

## Schema changes

None.

---

## Configuration changes

None.

---

## Dependency changes

None.

---

## Behavior changes

None. The CCE encoder was already used internally by `append()`; this pass only exposes it to Python callers and test code. No stored event encoding, event ID derivation, or append path changed.

---

## Living report updates

Entries added to living reports this pass:

- TECH_DEBT: TD-005 (added) — Python CCE conformance gap (encoder internal to `append()`, not exposed)

Entries resolved this pass:

- TECH_DEBT: TD-005 (resolved) — exposed as `cce_encode_value`/`cce_encode_bytes_raw`/`cce_encode_f64_bits`; harness added in the same pass
