use crate::{
    cce::derive_event_id,
    error::Error,
    schema::now_us,
    types::{EncryptionMode, EventId},
};
use rusqlite::{Connection, TransactionBehavior};

const PURGE_CONFIRM: &str = "I understand this breaks replay-from-zero";
const SYSTEM_STREAM: &str = "_fossic/system";
const PURGED_EVENT_TYPE: &str = "Purged";

/// Escape hatch for removing a single event. NOT for routine deletion.
///
/// The `confirm` parameter must be the literal string
/// `"I understand this breaks replay-from-zero"`. Any other value returns
/// `PurgeConfirmationError`.
///
/// Every call emits a WARN-level message to stderr documenting the purge.
/// A `fossic.Purged` audit event is appended to `_fossic/system` **before**
/// the original row is deleted; both writes happen in one transaction.
///
/// The original payload is never included in the Purged event.
pub(crate) fn purge_event_impl(
    conn: &mut Connection,
    id: EventId,
    confirm: &str,
    reason: &str,
    purged_by: &str,
) -> Result<(), Error> {
    if confirm != PURGE_CONFIRM {
        return Err(Error::PurgeConfirmationError {
            got: confirm.to_string(),
        });
    }

    let purged_at_us = now_us();

    // WARN-level security log to stderr.
    // TODO: upgrade to tracing::warn! or log::warn! when a logging framework is adopted.
    eprintln!(
        "[fossic WARN] purge_event called: id={id} reason=\"{reason}\" purged_by=\"{purged_by}\" \
         purged_at_us={purged_at_us}"
    );

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    // Fetch the original event metadata (not payload).
    let (original_event_type, original_stream_id, original_timestamp_us) = {
        let result: rusqlite::Result<(String, String, i64)> = tx.query_row(
            "SELECT event_type, stream_id, timestamp_us FROM events WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        );
        match result {
            Ok(row) => row,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Err(Error::EventNotFound {
                    id: id.to_hex(),
                });
            }
            Err(e) => return Err(Error::Sqlite(e)),
        }
    };

    // Build the Purged audit event payload (never includes original payload).
    let purged_payload = serde_json::json!({
        "event_id_purged": id.to_hex(),
        "original_event_type": original_event_type,
        "original_stream_id": original_stream_id,
        "original_timestamp_us": original_timestamp_us,
        "reason": reason,
        "purged_at_us": purged_at_us,
        "purged_by": purged_by,
    });

    // Ensure the _fossic/system stream is declared.
    tx.execute(
        "INSERT OR IGNORE INTO streams(id, declared_by, declared_at, description) \
         VALUES (?1, 'fossic-internal', 0, 'Internal fossic system events')",
        rusqlite::params![SYSTEM_STREAM],
    )?;

    // Derive and store the Purged audit event.
    let purged_id_bytes = derive_event_id(PURGED_EVENT_TYPE, 1, None, &purged_payload)?;
    let purged_id = EventId::from_bytes(purged_id_bytes);
    let purged_payload_bytes = rmp_serde::to_vec(&purged_payload)?;

    let next_version: i64 = tx.query_row(
        "SELECT COALESCE(MAX(version), -1) + 1 FROM events \
         WHERE stream_id = ?1 AND branch = 'main'",
        rusqlite::params![SYSTEM_STREAM],
        |r| r.get(0),
    )?;

    tx.execute(
        "INSERT OR IGNORE INTO events \
         (id, stream_id, branch, version, timestamp_us, event_type, type_version, payload) \
         VALUES (?1, ?2, 'main', ?3, ?4, ?5, 1, ?6)",
        rusqlite::params![
            purged_id,
            SYSTEM_STREAM,
            next_version,
            purged_at_us,
            PURGED_EVENT_TYPE,
            purged_payload_bytes,
        ],
    )?;

    // Delete the original event row.
    tx.execute("DELETE FROM events WHERE id = ?1", rusqlite::params![id])?;

    tx.commit()?;
    Ok(())
}

/// Crypto-shredding requires the store to be opened with `OsKeyring` or `EnvVar` encryption.
/// In plaintext mode this always returns `NotImplemented`.
pub(crate) fn shred_stream_impl(
    encryption: &EncryptionMode,
    _stream_id: &str,
    _reason: &str,
) -> Result<(), Error> {
    match encryption {
        EncryptionMode::Plaintext => Err(Error::NotImplemented {
            feature: "shred_stream requires encryption mode; \
                      open the store with OpenOptions::encryption = OsKeyring or EnvVar",
        }),
        EncryptionMode::OsKeyring | EncryptionMode::EnvVar(_) => Err(Error::NotImplemented {
            feature: "shred_stream (crypto-shredding implementation is a future track)",
        }),
    }
}
