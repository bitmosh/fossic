use crate::{
    cce::derive_event_id,
    error::Error,
    schema::now_us,
    stream::stream_exists_impl,
    types::{Append, EventId},
};
use rusqlite::{Connection, TransactionBehavior};

/// Outcome of a single append operation. Callers use this to build `StoredEvent`
/// for subscriber dispatch without re-reading from the database.
pub(crate) struct AppendOutcome {
    pub event_id: EventId,
    pub version: u64,
    pub timestamp_us: i64,
    pub payload_bytes: Vec<u8>,
    /// True if the event was newly inserted; false if a duplicate was skipped.
    pub is_new: bool,
}

/// Core append logic.
///
/// `payload` is the `serde_json::Value` used for CCE event-id derivation.
/// `payload_bytes` are the msgpack-encoded bytes to persist.
///
/// When no payload transform is registered for the stream, both are derived
/// from `a.payload`. When a transform has been applied upstream (in
/// `Store::append`), `payload` is the *transformed* value decoded back from
/// the transformed bytes, and `payload_bytes` are the transformed bytes. The
/// resulting id reflects the transformed payload, not the original.
pub(crate) fn append_impl(
    conn: &mut Connection,
    a: &Append,
    payload: serde_json::Value,
    payload_bytes: Vec<u8>,
) -> Result<AppendOutcome, Error> {
    // Validate indexed_tags is a JSON object when provided.
    if let Some(ref tags) = a.indexed_tags {
        if !tags.is_object() {
            return Err(Error::InvalidIndexedTags {
                got: match tags {
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::Bool(_) => "bool",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Null => "null",
                    serde_json::Value::Object(_) => unreachable!(),
                },
            });
        }
    }

    let causation_bytes = a.causation_id.as_ref().map(|id| *id.as_bytes());
    let event_id_bytes = derive_event_id(
        &a.event_type,
        a.type_version,
        causation_bytes.as_ref(),
        &payload,
    )?;
    let event_id = EventId::from_bytes(event_id_bytes);

    let indexed_tags_json = a.indexed_tags.as_ref().map(|t| t.to_string());
    let timestamp_us = a.timestamp_us.unwrap_or_else(now_us);

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    if !stream_exists_impl(&tx, &a.stream_id)? {
        return Err(Error::StreamNotDeclared {
            stream_id: a.stream_id.clone(),
        });
    }

    let next_version: i64 = tx.query_row(
        "SELECT COALESCE(MAX(version), -1) + 1 FROM events \
         WHERE stream_id = ?1 AND branch = ?2",
        rusqlite::params![a.stream_id, a.branch],
        |r| r.get(0),
    )?;

    let rows_changed = tx.execute(
        "INSERT OR IGNORE INTO events \
         (id, stream_id, branch, version, timestamp_us, causation_id, correlation_id, \
          event_type, type_version, payload, external_id, indexed_tags) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            event_id,
            a.stream_id,
            a.branch,
            next_version,
            timestamp_us,
            a.causation_id.as_ref(),
            a.correlation_id.as_ref(),
            a.event_type,
            a.type_version,
            payload_bytes,
            a.external_id,
            indexed_tags_json,
        ],
    )?;

    tx.commit()?;
    Ok(AppendOutcome {
        event_id,
        version: next_version as u64,
        timestamp_us,
        payload_bytes: rmp_serde::to_vec(&payload).unwrap_or_default(),
        is_new: rows_changed > 0,
    })
}

/// Like `append_impl`, but evaluates `condition` inside the IMMEDIATE transaction
/// before inserting. If `condition` returns `false`, the transaction is rolled back
/// and `Ok(None)` is returned. If it returns `true`, the append proceeds normally.
///
/// The condition runs after stream-existence is verified but before version assignment
/// and the INSERT. It receives a `&Connection` (actually the transaction dereffed) and
/// may execute any read queries against the current committed + in-transaction state.
/// It must not write.
pub(crate) fn append_if_impl<F>(
    conn: &mut Connection,
    a: &Append,
    payload: serde_json::Value,
    payload_bytes: Vec<u8>,
    condition: F,
) -> Result<Option<AppendOutcome>, Error>
where
    F: FnOnce(&Connection) -> Result<bool, Error>,
{
    if let Some(ref tags) = a.indexed_tags {
        if !tags.is_object() {
            return Err(Error::InvalidIndexedTags {
                got: match tags {
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::Bool(_) => "bool",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Null => "null",
                    serde_json::Value::Object(_) => unreachable!(),
                },
            });
        }
    }

    let causation_bytes = a.causation_id.as_ref().map(|id| *id.as_bytes());
    let event_id_bytes = derive_event_id(
        &a.event_type,
        a.type_version,
        causation_bytes.as_ref(),
        &payload,
    )?;
    let event_id = EventId::from_bytes(event_id_bytes);

    let indexed_tags_json = a.indexed_tags.as_ref().map(|t| t.to_string());
    let timestamp_us = a.timestamp_us.unwrap_or_else(now_us);

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    if !stream_exists_impl(&tx, &a.stream_id)? {
        return Err(Error::StreamNotDeclared {
            stream_id: a.stream_id.clone(),
        });
    }

    if !condition(&tx)? {
        // tx drops here, rolling back automatically (nothing was written)
        return Ok(None);
    }

    let next_version: i64 = tx.query_row(
        "SELECT COALESCE(MAX(version), -1) + 1 FROM events \
         WHERE stream_id = ?1 AND branch = ?2",
        rusqlite::params![a.stream_id, a.branch],
        |r| r.get(0),
    )?;

    let rows_changed = tx.execute(
        "INSERT OR IGNORE INTO events \
         (id, stream_id, branch, version, timestamp_us, causation_id, correlation_id, \
          event_type, type_version, payload, external_id, indexed_tags) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            event_id,
            a.stream_id,
            a.branch,
            next_version,
            timestamp_us,
            a.causation_id.as_ref(),
            a.correlation_id.as_ref(),
            a.event_type,
            a.type_version,
            payload_bytes,
            a.external_id,
            indexed_tags_json,
        ],
    )?;

    tx.commit()?;
    Ok(Some(AppendOutcome {
        event_id,
        version: next_version as u64,
        timestamp_us,
        payload_bytes: rmp_serde::to_vec(&payload).unwrap_or_default(),
        is_new: rows_changed > 0,
    }))
}

pub(crate) fn append_batch_impl(
    conn: &mut Connection,
    appends: &[Append],
    prepared_payloads: &[(serde_json::Value, Vec<u8>)],
) -> Result<Vec<AppendOutcome>, Error> {
    if appends.is_empty() {
        return Ok(Vec::new());
    }
    debug_assert_eq!(appends.len(), prepared_payloads.len());

    struct Encoded {
        event_id: EventId,
        payload_bytes: Vec<u8>,
        payload_for_outcome: Vec<u8>,
        indexed_tags_json: Option<String>,
        timestamp_us: i64,
    }

    let mut encoded = Vec::with_capacity(appends.len());
    for (a, (payload_val, payload_bytes)) in appends.iter().zip(prepared_payloads.iter()) {
        if let Some(ref tags) = a.indexed_tags {
            if !tags.is_object() {
                return Err(Error::InvalidIndexedTags {
                    got: match tags {
                        serde_json::Value::Array(_) => "array",
                        _ => "non-object",
                    },
                });
            }
        }
        let causation_bytes = a.causation_id.as_ref().map(|id| *id.as_bytes());
        let id_bytes = derive_event_id(
            &a.event_type,
            a.type_version,
            causation_bytes.as_ref(),
            payload_val,
        )?;
        let payload_for_outcome = rmp_serde::to_vec(payload_val).unwrap_or_default();
        encoded.push(Encoded {
            event_id: EventId::from_bytes(id_bytes),
            payload_bytes: payload_bytes.clone(),
            payload_for_outcome,
            indexed_tags_json: a.indexed_tags.as_ref().map(|t| t.to_string()),
            timestamp_us: a.timestamp_us.unwrap_or_else(now_us),
        });
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let mut outcomes = Vec::with_capacity(appends.len());
    for (a, enc) in appends.iter().zip(encoded.iter()) {
        if !stream_exists_impl(&tx, &a.stream_id)? {
            return Err(Error::StreamNotDeclared {
                stream_id: a.stream_id.clone(),
            });
        }

        let next_version: i64 = tx.query_row(
            "SELECT COALESCE(MAX(version), -1) + 1 FROM events \
             WHERE stream_id = ?1 AND branch = ?2",
            rusqlite::params![a.stream_id, a.branch],
            |r| r.get(0),
        )?;

        let rows_changed = tx.execute(
            "INSERT OR IGNORE INTO events \
             (id, stream_id, branch, version, timestamp_us, causation_id, correlation_id, \
              event_type, type_version, payload, external_id, indexed_tags) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                enc.event_id,
                a.stream_id,
                a.branch,
                next_version,
                enc.timestamp_us,
                a.causation_id.as_ref(),
                a.correlation_id.as_ref(),
                a.event_type,
                a.type_version,
                enc.payload_bytes,
                a.external_id,
                enc.indexed_tags_json,
            ],
        )?;

        outcomes.push(AppendOutcome {
            event_id: enc.event_id,
            version: next_version as u64,
            timestamp_us: enc.timestamp_us,
            payload_bytes: enc.payload_for_outcome.clone(),
            is_new: rows_changed > 0,
        });
    }

    tx.commit()?;
    Ok(outcomes)
}
