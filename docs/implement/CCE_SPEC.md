# CCE — Canonical Content Encoding (fossic-cce-v1)

**Status:** v1 specification · 2026-06-12
**Scope:** This document specifies the canonical content encoding used by fossic for content-addressed event identity. Implementations in Rust, Python, and TypeScript all conform to this spec. Third-party implementations against this spec are supported.

---

## 1. Why this exists separately from msgpack

Fossic stores event payloads as msgpack. msgpack is excellent for storage: compact, fast, schema-flexible. But msgpack is a poor *canonical* encoding — its output varies with library version, struct field declaration order, integer width selection, float precision, and other encoder configuration. Computing event identity over msgpack bytes means identity is coupled to an encoding chosen for different reasons.

CCE decouples identity from storage. Storage stays msgpack. Identity uses CCE, which is purpose-built to be deterministic across implementations and stable across library versions. The cost is one encoding pass per appended event; the bench measured this at ~200μs for a 40KB payload (≈1% of the append path).

CCE is a small spec. The reference implementation is ~100 lines of Rust, ~80 lines of Python, ~80 lines of TypeScript. The full test vector file has ~50 vectors covering every type and every edge case.

---

## 2. Type tags

CCE encodes values as `(tag, body)` pairs. All integers in bodies are little-endian.

| Tag | Type | Body |
|---|---|---|
| `0x00` | null | (empty) |
| `0x01` | bool | 1 byte: `0x00` (false) or `0x01` (true) |
| `0x02` | integer | 8 bytes: signed i64, little-endian |
| `0x03` | float | 8 bytes: f64, little-endian, with canonicalization (see §3.3) |
| `0x04` | string | 4 bytes: u32 length (LE), then NFC-normalized UTF-8 bytes |
| `0x05` | bytes | 4 bytes: u32 length (LE), then raw bytes |
| `0x06` | array | 4 bytes: u32 count (LE), then count CCE-encoded values |
| `0x07` | map | 4 bytes: u32 pair count (LE), then count `(CCE-encoded key, CCE-encoded value)` pairs, sorted by byte-lex of the CCE-encoded key |
| `0x08` | tagged | 2 bytes: u16 variant tag (LE), then one CCE-encoded payload (for enums/sum types) |

Tags `0x09` through `0xFF` are reserved.

---

## 3. Canonicalization rules

### 3.1 Integers

All integers normalize to signed i64 regardless of source representation. `1u8` and `1u32` and `1i64` all encode to `02 01 00 00 00 00 00 00 00` (tag `0x02`, body = 1 as little-endian i64).

Unsigned integers larger than i64::MAX are not representable in CCE. Encoders MUST reject `u64` values > 2^63 - 1 with an error rather than silently wrapping. This is a constraint, not a feature: payloads designed for fossic should fit in i64.

### 3.2 Strings

UTF-8 bytes after **NFC normalization**. The same logical string with different Unicode normalization forms produces the same CCE encoding. NFC is chosen because it is the most common form for typed input.

Implementations MUST normalize at encode time. Skipping normalization is a conformance failure.

The 4-byte length prefix is u32 (max 4 GiB per string). Implementations SHOULD reject strings larger than 64 MiB at encode time and surface this as a configuration choice if a consumer wants to lift the cap.

### 3.3 Floats

f64 in IEEE 754 binary64. Canonicalization rules:

- **NaN normalization:** all NaN values encode to the canonical quiet NaN bit pattern `0x7FF8000000000000` (little-endian: `00 00 00 00 00 00 F8 7F`).
- **Negative zero:** `-0.0` encodes identically to `+0.0` (i.e., `00 00 00 00 00 00 00 00`).
- **Subnormals:** preserved as-is.
- **Infinities:** preserved as-is (`+Inf` = `0x7FF0000000000000`, `-Inf` = `0xFFF0000000000000`).

These rules eliminate the most common float-non-determinism failure modes. Floats with subtle precision differences (e.g., from different math libraries) still produce different encodings — fossic does not perform precision normalization. Consumers that need precision-tolerant identity should round their floats before passing them to fossic.

### 3.4 Maps

Keys are encoded first, then sorted by **byte-lex of the CCE-encoded key**. The sort is over raw encoded bytes, not over the logical key value. This makes the sort independent of locale, collation, and any other source of variance.

Maps with duplicate keys (after CCE encoding) are a conformance error. Implementations MUST reject duplicate keys at encode time.

Keys can be any CCE-encodable value, not just strings. In practice consumers use strings; the spec allows integer and bytes keys for completeness.

### 3.5 Arrays

Arrays preserve order. No sorting, no deduplication.

### 3.6 Tagged variants

For Rust enums, TypeScript discriminated unions, and Python sum types. The variant tag is a u16 specific to the type — consumers define their own variant-tag-to-name mapping in their schema. CCE only cares that the bytes are deterministic.

Most consumers will not use the tagged variant tag directly; serializers handle it. The tag exists so that two structurally identical payloads with different sum-type interpretations produce different identities.

### 3.7 Order of struct/object fields

Consumers serialize structs/objects to CCE maps (tag `0x07`). The map sort applies. Struct field declaration order does not affect CCE output. Renaming a field changes the CCE encoding (because the key string changes); reordering fields does not.

---

## 4. Event identity derivation

```
event_id = blake3(
    "fossic-cce-v1\0"
    || cce_encode_string(event_type)
    || cce_encode_uint_as_i64(type_version)
    || cce_encode_optional_bytes(causation_id)
    || cce_encode(payload)
)
```

Where:

- `"fossic-cce-v1\0"` is the literal byte string `66 6F 73 73 69 63 2D 63 63 65 2D 76 31 00` (14 bytes). The `\0` is a deliberate separator that no UTF-8 string can contain.
- `cce_encode_string(event_type)` produces tag `0x04` + length + bytes.
- `cce_encode_uint_as_i64(type_version)` produces tag `0x02` + 8 bytes. `type_version` is u32 in the schema but encodes as i64 in CCE (per §3.1).
- `cce_encode_optional_bytes(causation_id)` produces tag `0x00` (null) if causation_id is None, or tag `0x05` (bytes) + length + 32 bytes of the causation_id otherwise.
- `cce_encode(payload)` produces the full canonical encoding of the payload value.

The result is 32 bytes (blake3 output length). This is the event's `id` field in the schema.

### Why causation_id is in the hash

Two semantically identical events at different positions in a causal chain produce different ids. This is the correct behavior for fossic's use case — "identical-payload events at different causal positions" are distinguishable historical events, not duplicates to be merged.

Consumers that want pure content-of-payload addressing (i.e., dedup identical payloads regardless of position) can use the `payload_cce_hash` derived independently:

```python
payload_only_hash = blake3(b"fossic-cce-v1\0payload\0" + cce_encode(payload))
```

This is not stored in the schema by default; consumers that need it can put it in `indexed_tags` for cross-stream content-based deduplication queries.

---

## 5. Conformance and test vectors

Every CCE implementation MUST pass the test vector file `cce-test-vectors.json` shipped with the fossic crate. The vectors cover:

- All nine tag types with simple values
- All canonicalization rules (NaN, -0.0, NFC, integer width normalization)
- Map key sorting with mixed-type keys
- Nested structures (map-of-arrays, array-of-maps)
- Boundary cases (empty string, empty map, empty array, max-length string)
- The full event-id derivation chain with example payloads from rhyzome and bons.ai event types
- A "trip test" set: for each vector, the spec implementation's output is recorded as the expected bytes.

The reference Rust implementation runs this test vector file as part of CI. Python and TypeScript implementations include their own copies of the same vectors and run them as part of their package CI.

Adding new vectors is allowed in spec minor versions (v1.1, v1.2). Removing or changing vectors requires a new major version (v2) because it changes existing event identities.

---

## 6. Version evolution

The leading `"fossic-cce-v1\0"` prefix exists specifically to enable safe future evolution. If a future CCE version (v2) is designed:

- New events encode with `"fossic-cce-v2\0"` and a new schema entry records the version per store.
- Old events keep their v1 IDs forever (computed at original append time).
- Reducers don't care because they see decoded payloads, not bytes.
- The store schema gains a `cce_version` column on a per-event basis if mixed versions need to coexist within one store (unlikely but supported).

v1 is expected to be stable for the long term. The spec is small, the canonicalization rules cover the common failure modes, and no consumer has expressed a need for changes.

---

## 7. Implementation notes

### 7.1 Rust reference implementation

The reference implementation lives in the `fossic` crate as the `cce` module. It depends only on `unicode-normalization` (for NFC) and `blake3`. Total LOC: ~120 lines including doc comments. The implementation is `#[no_std]`-friendly and allocates exactly one `Vec<u8>` per encode call.

### 7.2 Python implementation

Pure Python via the `fossic._cce` module. Depends on `unicodedata` (stdlib) and the `blake3` PyPI package. Total LOC: ~100 lines. Benchmarked at ~200μs for a 40KB payload on the bench hardware (matches the Rust path closely because PyO3 avoids re-encoding across the FFI boundary — the Python implementation is for verification and standalone use, not the hot path).

**PyO3 version target.** The fossic Python binding targets PyO3 ≥ 0.26 (released October 2025) for both the main API surface and the CCE module. PyO3 0.26 renamed the core GIL-handling APIs — `Python::with_gil → Python::attach` and `Python::allow_threads → Python::detach` — to reflect the introduction of free-threaded Python 3.13+/3.14. Earlier PyO3 versions are not supported; their threading model has changed in ways that affect subscription delivery semantics. Consumers using fossic from Python don't need to know PyO3 version details (the API surface is stable); this constraint applies only to anyone building fossic from source or contributing to the binding.

### 7.3 TypeScript implementation

Pure TypeScript with a wasm-blake3 dependency. Depends on no other runtime libraries. Total LOC: ~100 lines including type definitions. Used by the standalone time-travel viewer demo (if shipped) and by test harnesses.

### 7.4 Streaming encode

The reference implementations are non-streaming — they buffer the full CCE output before hashing. For payloads up to ~64 MiB this is fine. Consumers with larger payloads should not be using fossic anyway (the storage path stops being efficient long before the encode path matters), so streaming CCE is not in v1.

---

## 8. What CCE deliberately does not solve

- **Schema evolution.** That's the upcaster protocol. CCE is only about computing a stable identity over a given snapshot of a payload's logical value.
- **Compression.** That's storage. CCE is for hashing; the input is in memory and small relative to network/disk.
- **Cross-spec compatibility.** CCE is not designed to interoperate with other canonical encodings (dCBOR, JCS, etc.). It is a fossic-internal spec, used by fossic implementations and any third party that wants to compute fossic-compatible identities.
- **Cryptographic non-malleability.** blake3 provides this property; CCE only ensures the input bytes to blake3 are canonical.

---

*End of CCE spec.*
