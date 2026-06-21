# SR-01 — Identity and Canonical Content Encoding

**Series:** Fossic State Reports · Document 1 of 9  
**Scope:** Content-addressed event identity, the CCE encoding protocol, `derive_event_id`, and the Python CCE API  
**Source files:** `src/cce.rs`, `src/types.rs`, `fossic-py/src/cce.rs`, `fossic-py/src/types.rs`

---

## 1. Design Rationale: Content-Addressed Identity

Every event in Fossic has a 32-byte identity derived by hashing the event's content with blake3. This is the most consequential design decision in the library. Understanding why it was made, and what the tradeoffs are, is the foundation for understanding everything else.

### Why not UUIDs or ULIDs?

UUID v4 is random. ULID is time-sortable random. Both have the same fundamental property: the identity is generated at write time by whoever is doing the writing, independently of what is being written. This means:

- Two writers who independently observe the same logical fact and try to record it create two distinct events. There is no way to detect or merge the duplicate without application-level deduplication logic.
- A consumer who wants to check "did I already process this?" must maintain an external seen-set keyed on the foreign ID it assigned.
- Replaying a stream from a backup produces different IDs than the originals, breaking any reference from another stream's `causation_id` or `correlation_id`.
- Distributed or multi-process writers cannot agree on identity without coordination.

### What content-addressing gives you

With content-addressed identity, the event ID is a deterministic function of the event's observable content: its type, its schema version, its causation chain, and its payload. This means:

**Deduplication at the storage layer.** If your relay agent delivers the same logical event twice — because of a crash-and-restart, a retry, or overlapping backfill and live subscription windows — the second append produces an identical ID. The primary key constraint catches it. No application-level seen-set needed. The `read_by_external_id` deduplication path is for cases where the event content itself may differ but you want idempotency on a foreign key.

**Cross-replica consistency.** Two independently-running stores that accept the same logical events (via relay or replication) arrive at the same IDs. Any reference from a `causation_id` in store A points to a valid `id` in store B if that event was also delivered to store B.

**Verifiable integrity.** Given an event's content, you can independently recompute its ID and compare it to what the store claims. If they differ, the event has been mutated after storage.

**Pre-computation.** A consumer can compute the ID an event *will* have before appending it. This enables optimistic concurrency patterns and prefetch caching.

### What you give up

**You cannot know the ID before you have the full payload.** If you are building the payload incrementally, you must finish it before you know the ID. This is occasionally inconvenient for logging patterns.

**Payloads that look the same produce the same ID.** Two events with the same type, version, causation, and payload are not just "equal" — they are the *same event* in Fossic's model. Appending them in the same stream at the same version is a conflict; appending them to the same stream at different versions is structurally possible but semantically odd (you would have two distinct version entries with the same primary key, which the schema prevents).

**Payload transforms change the ID.** This is covered in §9. If you register a transform on a stream, the ID reflects the transformed payload, not the original. Callers must be aware of this when pre-computing IDs.

### Why blake3?

blake3 is:
- **Fast.** Faster than SHA-256, SHA-3, and most other hash functions on modern hardware. For small event payloads (< 1 KB), this matters for append throughput.
- **Parallelizable** (for large inputs). The tree-based construction allows SIMD acceleration.
- **32-byte output.** Sufficient collision resistance for the expected event volumes. 2^128 collision resistance in the birthday bound.
- **Not a keyed MAC.** Fossic uses the unkeyed hash (blake3 in its basic form). There is no secret involved — the goal is content-addressing, not authentication.

---

## 2. The CCE Version Prefix

The version prefix is: `b"fossic-cce-v1\0"`

Breaking this down:
- `f o s s i c - c c e - v 1` — 13 ASCII bytes
- `\0` — one NUL byte (0x00)

**Total: 14 bytes.**

The NUL terminator is critical. Without it, a future version `fossic-cce-v10` would start with the same bytes as `fossic-cce-v1`, meaning a hash input for v1 encoding could be confused with a hash input for v10 encoding if the rest of the bytes happened to align. The NUL makes all version strings self-delimiting: no valid version name contains a NUL, so no version string is a byte-prefix of another.

### Where it appears

The version prefix is prepended to the hash input inside `derive_event_id` **only**. It is the first thing hashed. It does not appear when you call `cce_encode_value` standalone — that function returns pure CCE bytes for a single value, with no version prefix. The prefix is strictly a domain-separation marker for the ID derivation function.

If you are implementing a conforming CCE encoder to compute event IDs in another language, you must prepend exactly these 14 bytes before your encoded fields. If you are implementing just a CCE encoder (not ID derivation), the prefix is irrelevant.

---

## 3. CCE Type Tags: Complete Reference

CCE encodes JSON values (null, bool, integer, float, string, bytes, array, object/map). Every value begins with a 1-byte tag. The encoding is little-endian throughout.

### 3.1 NULL — Tag 0x00

```
[0x00]
```

Total: 1 byte.

Represents JSON `null`. No additional payload. This tag byte also serves as the "None" sentinel in `cce_encode_optional_bytes` when the causation_id is absent.

### 3.2 BOOL — Tag 0x01

```
[0x01][value_byte]
```

Total: 2 bytes.

- `false` → `[0x01, 0x00]`
- `true`  → `[0x01, 0x01]`

No other values are valid.

### 3.3 INT — Tag 0x02

```
[0x02][i64_little_endian: 8 bytes]
```

Total: 9 bytes.

All integer values — whether they originate as JSON integers, Rust `i32`, `u32`, `i64`, or `u64` — normalize to a signed 64-bit integer before encoding. The 8 bytes are the two's-complement little-endian representation.

**Overflow:** u64 values greater than `i64::MAX` (9,223,372,036,854,775,807) cannot be represented and produce `CceError::U64Overflow(value)`. This is the only arithmetic error in CCE — all other integer ranges fit.

**Examples:**

| Value | Bytes (hex) |
|-------|-------------|
| 0 | `02 00 00 00 00 00 00 00 00` |
| 1 | `02 01 00 00 00 00 00 00 00` |
| -1 | `02 ff ff ff ff ff ff ff ff` |
| 255 | `02 ff 00 00 00 00 00 00 00` |
| i64::MAX | `02 ff ff ff ff ff ff ff 7f` |
| i64::MIN | `02 00 00 00 00 00 00 00 80` |

### 3.4 FLOAT — Tag 0x03

```
[0x03][f64_little_endian: 8 bytes]
```

Total: 9 bytes.

The float value is canonicalized before its bits are written (see §4). The 8 bytes are the IEEE 754 double-precision little-endian bit pattern of the *canonicalized* value.

**Examples:**

| Value | Canonical bits (hex, big-endian) | CCE bytes |
|-------|----------------------------------|-----------|
| 0.0 | `0000000000000000` | `03 00 00 00 00 00 00 00 00` |
| -0.0 → +0.0 | `0000000000000000` | `03 00 00 00 00 00 00 00 00` |
| 1.0 | `3FF0000000000000` | `03 00 00 00 00 00 00 F0 3F` |
| -1.0 | `BFF0000000000000` | `03 00 00 00 00 00 00 F0 BF` |
| NaN (any) → quiet NaN | `7FF8000000000000` | `03 00 00 00 00 00 00 F8 7F` |
| +Inf | `7FF0000000000000` | `03 00 00 00 00 00 00 F0 7F` |

Note the byte reversal: CCE stores f64 in little-endian, so the most-significant byte of the big-endian representation becomes the last byte.

### 3.5 STRING — Tag 0x04

```
[0x04][length_u64_le: 8 bytes][nfc_utf8_bytes: length bytes]
```

Total: 9 + length bytes.

The string is **NFC-normalized before encoding** (see §5). The length field is the byte length of the NFC-normalized UTF-8, encoded as a u64 in little-endian.

**Size limit:** If the NFC-normalized UTF-8 byte length exceeds 67,108,864 bytes (64 MiB), CCE returns `CceError::StringTooLarge(byte_len)`.

**Examples:**

| String | NFC bytes | Length encoding | Full CCE |
|--------|-----------|-----------------|----------|
| `""` | (none) | `00 00 00 00 00 00 00 00` | `04 00 00 00 00 00 00 00 00` |
| `"a"` | `61` | `01 00 00 00 00 00 00 00` | `04 01 00 00 00 00 00 00 00 61` |

### 3.6 BYTES — Tag 0x05

```
[0x05][length_u64_le: 8 bytes][raw_bytes: length bytes]
```

Total: 9 + length bytes.

Raw byte sequences. No normalization applied. The length field uses the same u64 little-endian encoding as STRING.

This type does not appear in JSON encoding (JSON has no native bytes type), but appears in two places in Fossic:
1. `cce_encode_optional_bytes` uses tag 0x05 when encoding the causation_id (32 raw bytes).
2. The Python binding exposes `cce_encode_bytes_raw` for conformance testing.

**The NULL (0x00) / BYTES (0x05) duality in `cce_encode_optional_bytes`:**

```
if causation_id is None:
    emit [0x00]              // NULL tag — 1 byte
else:
    emit [0x05]              // BYTES tag
    emit [0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]  // 32 as u64 LE
    emit causation_id_bytes  // 32 raw bytes
```

Total when Some: 41 bytes. Total when None: 1 byte.

### 3.7 ARRAY — Tag 0x06

```
[0x06][element_count_u64_le: 8 bytes][cce(elem_0)][cce(elem_1)]...
```

Total: 9 bytes + sum of element encodings.

Elements are encoded in their original order. No reordering. The element count is encoded as a u64 little-endian before the elements.

**Empty array:** `[0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]` — 9 bytes.

**Nested arrays and maps** are encoded recursively. There is no depth limit specified, though stack depth is bounded by the recursion of `encode_value`.

### 3.8 MAP — Tag 0x07

```
[0x07][pair_count_u64_le: 8 bytes][cce(key_0)][cce(val_0)][cce(key_1)][cce(val_1)]...
```

Total: 9 bytes + sum of key and value encodings.

Maps are the most complex type. Key-value pairs are **not** stored in insertion order; they are **sorted** by the CCE encoding of the key (see §6). Duplicate keys produce `CceError::DuplicateKeys`.

JSON objects have string keys, so in practice all CCE map keys are STRING-encoded. The sort order is byte-lexicographic over the full `[0x04][len_u64_le][nfc_utf8]` encoding of the key.

---

## 4. Float Canonicalization

IEEE 754 double-precision floats have two values that would otherwise cause non-determinism in content-addressing:

### 4.1 Negative zero

IEEE 754 defines two representations of zero: positive zero (all bits zero) and negative zero (sign bit set, all other bits zero):

- Positive zero: bits = `0x0000_0000_0000_0000`
- Negative zero: bits = `0x8000_0000_0000_0000`

These compare equal under `==` in most languages, but their bit patterns differ. A hash over the bit pattern would treat them differently, breaking content-addressing for payloads that contain `-0.0`.

**CCE canonicalization:** negative zero → positive zero.

The check in Rust is:
```rust
if f.to_bits() == 0x8000_0000_0000_0000u64 {
    0.0f64  // positive zero
} else {
    f
}
```

### 4.2 NaN

IEEE 754 allows many bit patterns to represent NaN (Not a Number). The only requirement is that the exponent bits are all 1 and the mantissa is non-zero. This gives roughly 2^52 possible NaN bit patterns, divided into "quiet NaN" (quiet bit set) and "signaling NaN" (quiet bit clear) categories.

**CCE canonicalization:** any NaN → quiet NaN with bits `0x7FF8_0000_0000_0000`.

The check in Rust is:
```rust
if f.is_nan() {
    f64::from_bits(0x7FF8_0000_0000_0000u64)
} else {
    f  // already handled -0.0 above
}
```

In the actual implementation, the NaN check comes first, then the -0.0 check.

### 4.3 Values that pass through unchanged

Everything else — positive zero (already canonical), all non-zero finite values, positive infinity (`0x7FF0_0000_0000_0000`), negative infinity (`0xFFF0_0000_0000_0000`), and subnormal numbers — is encoded with its original bit pattern.

**Subnormals are preserved.** This is intentional: subnormals are fully specified values, not an artifact of representation, and should not be normalized away.

### 4.4 Combined canonicalization in the implementation

```rust
pub fn canonicalize_f64(f: f64) -> f64 {
    if f.is_nan() {
        f64::from_bits(0x7FF8_0000_0000_0000u64)
    } else if f.to_bits() == 0x8000_0000_0000_0000u64 {
        0.0f64
    } else {
        f
    }
}
```

After canonicalization, the value is written as 8 little-endian bytes with `canonical.to_le_bytes()`.

---

## 5. String NFC Normalization

Unicode defines multiple canonical forms for strings. Two strings that represent the "same" text may have different byte sequences depending on whether they use precomposed or decomposed character forms. For example:

- `"é"` can be encoded as the single codepoint U+00E9 (precomposed)
- `"é"` can also be encoded as U+0065 (e) followed by U+0301 (combining acute accent) (decomposed)

These are canonically equivalent in Unicode but have different UTF-8 byte sequences.

**CCE applies NFC normalization before encoding strings.** NFC (Canonical Decomposition followed by Canonical Composition) converts decomposed forms to their precomposed equivalents where they exist. After NFC:

- `"é"` is always U+00E9, regardless of how the input arrived.
- The byte length recorded in the STRING encoding reflects the NFC form.
- Two strings that are canonically equivalent produce identical CCE bytes.

### What NFC applies to

Only the CCE encoding path normalizes strings. Specifically:

- **`encode_value` for STRING:** applies NFC before measuring the byte length and writing the UTF-8 bytes.
- **`encode_string` helper:** same.
- **Event payloads:** the payload passed to `derive_event_id` is re-encoded through CCE, so all string values within the payload are NFC-normalized during ID derivation.

**What NFC does NOT apply to:**

- **The stored msgpack bytes.** After CCE encoding computes the ID, the payload is stored as msgpack. Msgpack is written from the original (pre-NFC) values. A string `"é"` in decomposed form might be stored in msgpack as decomposed and returned in decomposed form on read. Only the ID is computed from the NFC form.
- **`stream_id`, `event_type`, `external_id`** fields in the events table — these are stored as provided, not normalized. However, `event_type` goes through `encode_string` during ID derivation, so the *identity* of two events that differ only in the NFC form of their event_type will be the same.

### Practical implications

If you are pre-computing an event ID in Python or another language to compare with what the store will assign, you must NFC-normalize all string values in the payload before running your CCE encoder. The Python `unicodedata.normalize('NFC', s)` function does this.

---

## 6. Map Sort Order

JSON objects (maps) have no defined key order. Two JSON objects `{"a": 1, "b": 2}` and `{"b": 2, "a": 1}` represent the same value, but if encoded in key-insertion order they produce different byte sequences. CCE must impose a canonical key order.

**Sort key:** the CCE encoding of the key itself. Since JSON keys are strings, the sort is byte-lexicographic over:

```
[0x04][key_len_u64_le: 8 bytes][nfc_utf8_bytes: key_len bytes]
```

### Why sort by CCE encoding rather than by raw string?

Two reasons:

1. **Consistency.** The same normalization (NFC) that applies to all STRING values in CCE also applies to map keys. Sorting by the raw UTF-8 would silently differ from sorting by the CCE-encoded form if any key required NFC normalization.

2. **Extensibility.** If a future CCE version allowed non-string keys (integer keys, bytes keys), sorting by the CCE encoding of the key gives a total order over heterogeneous key types, since tags provide unambiguous type precedence.

### Sort algorithm

```
1. For each key in the map, compute its CCE encoding (STRING tag + len + nfc_bytes)
2. Sort the key-value pairs by their CCE-encoded key, byte-lexicographically
3. Encode pairs in sorted order: cce(key) || cce(value) for each pair
```

Because tag 0x04 is the first byte of every STRING key, all keys begin with the same byte. The sort then falls through to the length bytes (8 bytes, u64 LE) and then to the NFC UTF-8 content.

**Sort order examples:**

For short keys where lengths differ:
- `""` → `04 00 00 00 00 00 00 00 00` — sorts before all non-empty keys
- `"a"` → `04 01 00 00 00 00 00 00 00 61`
- `"aa"` → `04 02 00 00 00 00 00 00 00 61 61` — two-char key
- `"b"` → `04 01 00 00 00 00 00 00 00 62` — same length as "a", sorts after

For keys of equal length, sort falls through to the UTF-8 content bytes:
- `"a"` < `"b"` because `0x61` < `0x62` in the 10th byte.

### Duplicate key detection

During the sort pass, if two keys produce the same CCE encoding (i.e., they are the same string after NFC normalization), `CceError::DuplicateKeys` is returned. The duplicate check is done after sorting, by comparing adjacent CCE-encoded keys.

---

## 7. The `derive_event_id` Formula

This is the complete, exact formula for Fossic event ID derivation.

### Input fields

| Field | Type | Notes |
|-------|------|-------|
| `event_type` | `&str` | The event type name, e.g. `"UserCreated"` |
| `type_version` | `u32` | Schema version number, e.g. `1` |
| `causation_id` | `Option<&[u8; 32]>` | Optional 32-byte ID of the causing event |
| `payload` | `&serde_json::Value` | The event payload as a JSON value |

### Hash input construction

The blake3 hash is computed over the concatenation of these byte sequences, **in this exact order**:

```
version_prefix        = b"fossic-cce-v1\0"        // 14 bytes, always first

encoded_event_type    = cce_encode_string(event_type)
                      = [0x04]                      // 1 byte: STRING tag
                      + [len_u64_le: 8 bytes]       // byte length of NFC(event_type)
                      + [nfc_utf8(event_type)]       // variable

encoded_type_version  = [0x02]                      // 1 byte: INT tag
                      + [u32_as_i64_le: 8 bytes]    // type_version cast to i64, LE

encoded_causation_id  = if causation_id is None:
                            [0x00]                  // 1 byte: NULL tag
                        else:
                            [0x05]                  // 1 byte: BYTES tag
                          + [0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]  // 32 as u64 LE
                          + [causation_id_bytes: 32 bytes]

encoded_payload       = cce_encode(payload)         // full recursive encoding
```

**Hash input:**

```
hash_input = version_prefix
           || encoded_event_type
           || encoded_type_version
           || encoded_causation_id
           || encoded_payload
```

**Result:**

```
event_id = blake3::hash(&hash_input)  // 32 bytes
```

### Byte count breakdown (for a simple event)

For an event with:
- `event_type = "Foo"` (3 UTF-8 bytes after NFC)
- `type_version = 1`
- `causation_id = None`
- `payload = {}` (empty object)

```
version_prefix:   14 bytes  (b"fossic-cce-v1\0")
event_type CCE:   12 bytes  (1 tag + 8 len + 3 UTF-8)
type_version CCE:  9 bytes  (1 tag + 8 value)
causation_id CCE:  1 byte   (0x00 = NULL)
payload CCE:       9 bytes  (0x07 tag + 8 zero count for empty map)
                 ----------
Total hash input: 45 bytes
```

### What is NOT in the hash

The following fields are stored in the events table but are **not inputs to the ID derivation**:

- `stream_id` — the stream this event belongs to
- `branch` — the branch within the stream
- `version` — the monotonic position within (stream, branch)
- `timestamp_us` — wall clock at append time
- `correlation_id` — optional grouping ID
- `external_id` — consumer-supplied deduplication key
- `indexed_tags` — denormalized JSON projection

This has a critical implication: **the same logical event appended to two different streams, or at two different times, gets the same ID.** The ID is a fingerprint of the event's *content*, not its *location*.

---

## 8. Identity Uniqueness and Collision Handling

### Primary key constraint

The events table has:
```sql
id BLOB NOT NULL PRIMARY KEY
```

The primary key is the blake3 hash. If two append operations produce the same ID:

- If they target the same `(stream_id, branch)` with the same content, this is an exact duplicate. The second INSERT fails on the primary key constraint.
- If they target different `(stream_id, branch)` combinations, the second INSERT also fails on the primary key constraint. This is the consequence of content-addressing: the same event cannot appear in two streams with different IDs.

The `UNIQUE (stream_id, branch, version)` constraint is separate and provides the monotonic ordering guarantee within a stream.

### Collision probability

blake3 produces a 256-bit hash. For a store with N events, the probability of any collision is approximately N²/2²⁵⁷. At one billion events (10⁹), the collision probability is approximately 3×10⁻⁵⁹ — negligibly small for any practical purpose.

### Idempotent append consequence

Because duplicate IDs fail at the storage layer, appending the same event twice is automatically idempotent at the ID level. This is distinct from the `external_id` deduplication path:

- **Content-addressed idempotency:** same event_type + type_version + causation_id + payload → same ID → INSERT fails on PRIMARY KEY → no duplicate stored.
- **`external_id` idempotency:** different content but same consumer-supplied key → the application checks `read_by_external_id` before appending.

---

## 9. Payload Transforms and ID Ordering

Payload transforms (registered via `Store::register_payload_transform`) mutate the msgpack-encoded payload before CCE encoding computes the ID.

The pipeline for an append is:

```
original_payload (dict/JSON)
  → serialize to msgpack bytes
  → apply_transforms(stream_id, event_type, msgpack_bytes)  ← transforms here
  → transformed_msgpack_bytes
  → CCE-encode for ID derivation: cce_encode(&from_msgpack(transformed_msgpack_bytes))
  → blake3 hash → event_id
  → store (event_id, transformed_msgpack_bytes) in events table
```

**Consequence:** The stored ID reflects the *transformed* payload. If you register a transform that redacts a `secret_key` field, the ID will not include `secret_key` in its computation, and neither will the stored payload. A consumer reading the event back will see the redacted form; there is no way to recover the original from the store (assuming no other copies exist).

**Pre-computation impact:** If you want to pre-compute an event's ID before appending, you must apply the same transforms yourself. Pre-computing without transforms will produce a different ID than what the store will assign.

### Why transforms fire before CCE

The alternative — computing the ID from the original payload, then storing the transformed bytes — would mean the stored ID and stored payload are inconsistent: the ID is no longer a fingerprint of the stored content. Fossic maintains the invariant that the ID can always be recomputed from the stored payload.

---

## 10. The EventId Type

### Rust definition

`EventId` is a newtype wrapping `[u8; 32]`. Key interface:

```rust
// Construction
EventId::from_bytes(bytes: [u8; 32]) -> EventId
EventId::from_hex(s: &str) -> Result<EventId, Error>  // validates exactly 64 hex chars

// Access
eventid.as_bytes() -> &[u8; 32]
eventid.to_hex() -> String  // 64 lowercase hex chars

// Traits
impl PartialEq, Eq, Hash for EventId  // bitwise equality on 32 bytes
impl ToSql for EventId               // stores as BLOB
impl FromSql for EventId             // reads from BLOB, validates length
impl Display for EventId             // delegates to to_hex()
```

### SQLite storage

EventId implements `ToSql` and `FromSql` using the rusqlite `BLOB` type. The 32 raw bytes are stored without any encoding. This means:
- The `id` column is a 32-byte blob, not a hex string.
- When you see the id in a SQLite browser, it displays as binary. Use `hex(id)` in SQL to get the hex representation.
- Index range scans on the primary key work on the raw bytes, which is fine — blake3 output has no structure that would cause index skew.

### Error on invalid hex

`EventId::from_hex` requires exactly 64 lowercase-or-uppercase hex characters. Any other input produces `Error::InvalidEventId`. The error wraps the description string, not the invalid bytes (to avoid embedding arbitrary input in error messages).

---

## 11. Python API for CCE

The Python bindings expose four CCE functions in the `_fossic` native module (accessed through the `fossic` package). These are intended for conformance testing, tooling, and pre-computation — production event-appending code does not need to call them directly.

### `cce_encode_value(value) -> bytes`

Encodes any JSON-serializable Python value using CCE. The value passes through `json.dumps` → `serde_json::Value` → `encode_value`. Returns the raw CCE bytes.

```python
import fossic._fossic as _f

_f.cce_encode_value(None)         # b'\x00'
_f.cce_encode_value(True)         # b'\x01\x01'
_f.cce_encode_value(42)           # b'\x02\x2a\x00\x00\x00\x00\x00\x00\x00'
_f.cce_encode_value("hello")      # b'\x04\x05\x00\x00\x00\x00\x00\x00\x00hello'
_f.cce_encode_value([1, 2])       # ARRAY tag + count + two INT encodings
_f.cce_encode_value({"b": 1, "a": 2})  # MAP tag + sorted pairs (a before b)
```

Note: the dict `{"b": 1, "a": 2}` encodes with `a` before `b` because CCE sorts by the CCE-encoded key bytes. The input dict ordering is irrelevant.

### `cce_encode_bytes_raw(data: bytes) -> bytes`

Encodes raw bytes using the BYTES type (tag 0x05). Returns `[0x05] + [len_u64_le] + data`.

```python
_f.cce_encode_bytes_raw(b'\xde\xad\xbe\xef')
# b'\x05\x04\x00\x00\x00\x00\x00\x00\x00\xde\xad\xbe\xef'
```

### `cce_encode_f64_bits(bits_hex: str) -> bytes`

Takes a 16-character lowercase hex string representing the big-endian u64 bit pattern of an f64, applies float canonicalization, and returns the 9-byte CCE encoding (tag + 8 LE bytes).

```python
_f.cce_encode_f64_bits("0000000000000000")  # 0.0 → b'\x03\x00...\x00'
_f.cce_encode_f64_bits("8000000000000000")  # -0.0 → b'\x03\x00...\x00' (same as 0.0)
_f.cce_encode_f64_bits("7ff8000000000000")  # quiet NaN → canonical
_f.cce_encode_f64_bits("3ff0000000000000")  # 1.0 → b'\x03\x00\x00\x00\x00\x00\x00\xf0\x3f'
```

This function exists to let conformance tests verify specific IEEE 754 bit patterns without relying on Python's float parsing, which may normalize some bit patterns automatically.

### `compute_event_id(event_type, payload, type_version=1, causation_id=None) -> EventId`

Computes the content-addressed ID for a given event without writing to the store. The returned `EventId` is byte-identical to the one `Store.append()` would assign (assuming no payload transforms are registered).

```python
from fossic._fossic import compute_event_id

eid = compute_event_id("UserCreated", {"user_id": "u123", "email": "a@b.com"})
print(eid.hex())  # 64-char hex string

# With causation:
eid2 = compute_event_id(
    "OrderPlaced",
    {"order_id": "o456"},
    type_version=1,
    causation_id=eid,
)
```

**Caveats:**
- Does not apply payload transforms. If transforms are registered on the target stream, the pre-computed ID will not match.
- Payload must be JSON-serializable (`json.dumps` must not raise).
- The payload goes through `json.dumps` → `serde_json` → CCE. Python `float('nan')` and `float('-0.0')` are handled by CCE canonicalization, but `json.dumps` may refuse to serialize NaN by default — use `allow_nan=True` or avoid NaN in payloads.

---

## 12. CceError Variants

`CceError` is a sub-enum nested within the main `Error` type. It surfaces as `Error::Cce(CceError)` in Rust and as `StorageError` in Python (since there is no dedicated `CceError` Python exception — it wraps into the catch-all).

### `CceError::U64Overflow(u64)`

**When:** A `u64` value greater than `i64::MAX` (9,223,372,036,854,775,807) is passed to integer encoding. CCE integers are signed 64-bit, so u64 values in the upper half of the u64 range cannot be represented.

**Error message:** `"u64 value {n} exceeds i64::MAX; CCE integers are signed i64"`

**Mitigation:** Avoid u64 values above i64::MAX in event payloads. Use strings for very large integers (e.g., UUIDs, snowflake IDs) rather than raw integers.

### `CceError::DuplicateKeys`

**When:** A map (JSON object) contains two keys that produce the same CCE encoding. In practice this means two keys that are equal after NFC normalization — e.g., one key in composed form and one in decomposed form that happen to represent the same string.

**Error message:** `"duplicate map keys after CCE encoding"`

**When this fires in practice:** Almost never with normal JSON payloads. JSON parsers typically reject duplicate keys. This error catches the edge case of Unicode-equivalent keys after normalization.

### `CceError::StringTooLarge(usize)`

**When:** A string's NFC-normalized UTF-8 byte length exceeds 67,108,864 bytes (64 MiB).

**Error message:** `"string exceeds 64 MiB limit ({n} bytes)"`

**Mitigation:** Don't embed large blobs as strings in event payloads. Use a content store and put a reference (path, hash, URI) in the event instead.

---

## 13. Conformance Guide for Alternative Implementations

If you are implementing CCE in another language (to compute event IDs client-side, or to write a conformance test suite), these are the requirements that must be exactly right:

### 13.1 String handling

1. **NFC-normalize all strings before encoding.** This includes both map keys and string values anywhere in the payload. Use the Unicode standard NFC algorithm (Canonical Decomposition, Canonical Composition).
2. **Measure byte length after NFC normalization.** The length field in STRING encoding is the byte count of the NFC form, not the input form.
3. **Encode the NFC UTF-8 bytes.** Write the normalized bytes, not the original input bytes.

In Python:
```python
import unicodedata
nfc = unicodedata.normalize('NFC', s)
nfc_bytes = nfc.encode('utf-8')
```

### 13.2 Map key sorting

1. **CCE-encode each key** (as a STRING: tag 0x04 + len u64 LE + nfc bytes).
2. **Sort key-value pairs** by the CCE-encoded key bytes, byte-lexicographically.
3. **Detect duplicates** (adjacent equal CCE-encoded keys after sort) and return an error.
4. **Encode pairs in sorted order.**

Do not sort by the raw string or by Unicode codepoint order — sort by the full CCE byte encoding including the tag and length.

### 13.3 Float canonicalization

Order matters: check NaN first, then -0.0.

```python
import struct, math

def canonicalize_f64(f):
    if math.isnan(f):
        return struct.unpack('<d', bytes.fromhex('000000000000f87f'))[0]  # quiet NaN LE
    bits = struct.pack('>d', f)
    if bits == b'\x80\x00\x00\x00\x00\x00\x00\x00':  # -0.0
        return 0.0
    return f

def encode_float(f):
    return b'\x03' + struct.pack('<d', canonicalize_f64(f))
```

Note the endianness: `'>d'` (big-endian) for the bit-pattern comparison of -0.0, `'<d'` (little-endian) for the output bytes.

### 13.4 Integer encoding

Encode as i64 (signed 64-bit) in little-endian. If the value is a u64 greater than 9,223,372,036,854,775,807, return an error.

```python
import struct

def encode_int(n):
    if n < -2**63 or n > 2**63 - 1:
        raise OverflowError(f"integer {n} out of i64 range")
    return b'\x02' + struct.pack('<q', n)
```

### 13.5 Version prefix

The `derive_event_id` function must prepend exactly:

```python
VERSION_PREFIX = b'fossic-cce-v1\x00'  # 14 bytes
```

This is 13 ASCII bytes plus one NUL byte (0x00). Do not use `b'fossic-cce-v1'` (13 bytes, missing the NUL) — it will produce different hashes.

### 13.6 Complete Python reference implementation

```python
import struct, math, unicodedata, hashlib

# blake3 requires a third-party library: pip install blake3
import blake3 as _blake3

VERSION_PREFIX = b'fossic-cce-v1\x00'

def _encode(v) -> bytes:
    if v is None:
        return b'\x00'
    if isinstance(v, bool):
        return b'\x01\x01' if v else b'\x01\x00'
    if isinstance(v, int):
        if v < -2**63 or v > 2**63 - 1:
            raise OverflowError(f"integer {v} out of i64 range")
        return b'\x02' + struct.pack('<q', v)
    if isinstance(v, float):
        c = _canon_f64(v)
        return b'\x03' + struct.pack('<d', c)
    if isinstance(v, str):
        nfc = unicodedata.normalize('NFC', v)
        b = nfc.encode('utf-8')
        if len(b) > 67_108_864:
            raise ValueError(f"string too large: {len(b)} bytes")
        return b'\x04' + struct.pack('<Q', len(b)) + b
    if isinstance(v, bytes):
        return b'\x05' + struct.pack('<Q', len(v)) + v
    if isinstance(v, list):
        parts = [_encode(e) for e in v]
        return b'\x06' + struct.pack('<Q', len(parts)) + b''.join(parts)
    if isinstance(v, dict):
        pairs = []
        for k, val in v.items():
            ek = _encode(k)  # k must be str for JSON compat
            ev = _encode(val)
            pairs.append((ek, ev))
        pairs.sort(key=lambda x: x[0])
        # detect duplicates
        for i in range(len(pairs) - 1):
            if pairs[i][0] == pairs[i+1][0]:
                raise ValueError("duplicate map keys after CCE encoding")
        return b'\x07' + struct.pack('<Q', len(pairs)) + b''.join(k+v for k,v in pairs)
    raise TypeError(f"unsupported type: {type(v)}")

def _canon_f64(f):
    if math.isnan(f):
        return struct.unpack('<d', b'\x00\x00\x00\x00\x00\x00\xf8\x7f')[0]
    if struct.pack('>d', f) == b'\x80\x00\x00\x00\x00\x00\x00\x00':
        return 0.0
    return f

def derive_event_id(event_type: str, type_version: int, payload: dict,
                    causation_id: bytes | None = None) -> bytes:
    buf = VERSION_PREFIX
    buf += _encode(event_type)
    buf += b'\x02' + struct.pack('<q', type_version)
    if causation_id is None:
        buf += b'\x00'
    else:
        buf += b'\x05' + struct.pack('<Q', 32) + causation_id
    buf += _encode(payload)
    return _blake3.blake3(buf).digest()
```

---

## 14. Worked Examples

### Example 1: Simple event with no causation

```
event_type    = "PingReceived"
type_version  = 1
causation_id  = None
payload       = {"source": "monitor", "seq": 42}
```

Hash input construction:

```
version_prefix:     14 bytes — b"fossic-cce-v1\0"

event_type CCE:
  tag:              0x04
  len (12 bytes):   0c 00 00 00 00 00 00 00  (= 12 in u64 LE)
  utf8:             50 69 6e 67 52 65 63 65 69 76 65 64  ("PingReceived")
  total:            21 bytes

type_version CCE:
  tag:              0x02
  value (1):        01 00 00 00 00 00 00 00
  total:            9 bytes

causation_id CCE:
  None → NULL tag:  0x00
  total:            1 byte

payload CCE ({"source": "monitor", "seq": 42}):
  MAP tag:          0x07
  pair count (2):   02 00 00 00 00 00 00 00

  key "seq" CCE:    04 03 00 00 00 00 00 00 00 73 65 71
  val 42 CCE:       02 2a 00 00 00 00 00 00 00

  key "source" CCE: 04 06 00 00 00 00 00 00 00 73 6f 75 72 63 65
  val "monitor" CCE: 04 07 00 00 00 00 00 00 00 6d 6f 6e 69 74 6f 72

  Keys sorted: "seq" (len 3) vs "source" (len 6).
  "seq" CCE:    04 03 00 00 00 00 00 00 00 73 65 71
  "source" CCE: 04 06 00 00 00 00 00 00 00 73 6f 75 72 63 65
  Byte 2 of "seq" key: 0x03, byte 2 of "source" key: 0x06
  "seq" < "source" → "seq" comes first

  total payload CCE: 9 (map header) + 12 + 9 + 15 + 16 = 61 bytes

Total hash input: 14 + 21 + 9 + 1 + 61 = 106 bytes
```

The event_id = blake3(106 bytes) → 32 bytes → 64-char hex.

### Example 2: Same payload, different stream

If the same event is appended to `"foo/bar"` and `"baz/qux"`, the ID is identical — stream_id is not in the hash. Both streams would have a row with the same 32-byte id blob, and a second append to either stream from the same source would fail on the PRIMARY KEY constraint.

### Example 3: NFC normalization in action

```python
# NFD form: e + combining acute
payload_nfd = {"name": "résumé"}  # résumé

# NFC form: precomposed é
payload_nfc = {"name": "r\xe9sum\xe9"}  # résumé

# Both produce the same event ID because CCE NFC-normalizes strings
id_nfd = derive_event_id("ProfileUpdated", 1, payload_nfd)
id_nfc = derive_event_id("ProfileUpdated", 1, payload_nfc)
assert id_nfd == id_nfc  # True
```

---

## 15. Summary: CCE Invariants

The following invariants hold by construction and must not be violated by any code that interacts with Fossic at the storage level:

1. **Determinism:** The same (event_type, type_version, causation_id, payload) always produces the same EventId, regardless of when or where the computation runs.

2. **Content fingerprint:** The EventId is a fingerprint of the stored content (after transforms). Changing any of the four inputs changes the EventId.

3. **Uniqueness:** No two distinct events (different content) share an EventId with non-negligible probability (blake3 collision resistance).

4. **Idempotency:** Appending the same logical event twice produces the same EventId; the store's PRIMARY KEY constraint makes the second append a no-op (or error, depending on the SQL dialect).

5. **Location independence:** stream_id, branch, version, timestamp, correlation_id, and indexed_tags do not affect the EventId. An event moved to a different stream, or observed at a different time, retains its identity.

6. **NFC closure:** All string values in the CCE encoding are NFC-normalized. Two payloads that are canonical Unicode equivalents produce the same CCE encoding and the same EventId.

7. **Float uniqueness:** Each float value has exactly one canonical CCE encoding. -0.0 and +0.0 share the same encoding. All NaN bit patterns share one encoding.

8. **Map commutativity:** The order of keys in a JSON object does not affect the EventId. Maps are always serialized in CCE key-sorted order.
