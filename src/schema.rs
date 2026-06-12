use crate::error::Error;
use rusqlite::Connection;

const CURRENT_SCHEMA_VERSION: u32 = 1;

const SCHEMA_V1: &str = "
CREATE TABLE events (
    id              BLOB    NOT NULL PRIMARY KEY,
    stream_id       TEXT    NOT NULL,
    branch          TEXT    NOT NULL DEFAULT 'main',
    version         INTEGER NOT NULL,
    timestamp_us    INTEGER NOT NULL,
    causation_id    BLOB,
    correlation_id  BLOB,
    event_type      TEXT    NOT NULL,
    type_version    INTEGER NOT NULL DEFAULT 1,
    payload         BLOB    NOT NULL,
    external_id     TEXT,
    indexed_tags    TEXT,
    UNIQUE (stream_id, branch, version)
);

CREATE INDEX idx_events_stream_branch_version
    ON events(stream_id, branch, version);
CREATE INDEX idx_events_correlation
    ON events(correlation_id) WHERE correlation_id IS NOT NULL;
CREATE INDEX idx_events_causation
    ON events(causation_id) WHERE causation_id IS NOT NULL;
CREATE INDEX idx_events_external_id
    ON events(stream_id, external_id) WHERE external_id IS NOT NULL;
CREATE INDEX idx_events_timestamp
    ON events(timestamp_us);
CREATE INDEX idx_events_type
    ON events(event_type);

CREATE TABLE branches (
    id              TEXT    NOT NULL,
    stream_id       TEXT    NOT NULL,
    parent_id       TEXT    NOT NULL,
    parent_version  INTEGER NOT NULL,
    description     TEXT,
    created_at      INTEGER NOT NULL,
    lifecycle       TEXT    NOT NULL DEFAULT 'ephemeral',
    closed_at       INTEGER,
    closed_reason   TEXT,
    alternatives    TEXT,
    PRIMARY KEY (stream_id, id)
);

CREATE INDEX idx_branches_stream ON branches(stream_id);
CREATE INDEX idx_branches_lifecycle ON branches(stream_id, lifecycle);

CREATE TABLE snapshots (
    stream_id            TEXT    NOT NULL,
    branch               TEXT    NOT NULL DEFAULT 'main',
    version              INTEGER NOT NULL,
    reducer_name         TEXT    NOT NULL,
    reducer_version      INTEGER NOT NULL DEFAULT 1,
    state_schema_version INTEGER NOT NULL DEFAULT 1,
    state_blob           BLOB    NOT NULL,
    created_at           INTEGER NOT NULL,
    PRIMARY KEY (stream_id, branch, reducer_name, state_schema_version, version)
);

CREATE INDEX idx_snapshots_lookup
    ON snapshots(stream_id, branch, reducer_name, state_schema_version, version DESC);

CREATE TABLE streams (
    id              TEXT    NOT NULL PRIMARY KEY,
    declared_by     TEXT    NOT NULL,
    declared_at     INTEGER NOT NULL,
    description     TEXT
);

CREATE TABLE stream_deks (
    stream_id       TEXT    NOT NULL PRIMARY KEY,
    key_id          TEXT    NOT NULL,
    created_at      INTEGER NOT NULL,
    shredded_at     INTEGER,
    shredded_reason TEXT
);

CREATE TABLE cursors (
    consumer_id     TEXT    NOT NULL,
    stream_id       TEXT    NOT NULL,
    branch          TEXT    NOT NULL DEFAULT 'main',
    version         INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    PRIMARY KEY (consumer_id, stream_id, branch)
);

CREATE TABLE upcasters_registered (
    event_type      TEXT    NOT NULL,
    from_version    INTEGER NOT NULL,
    to_version      INTEGER NOT NULL,
    registered_at   INTEGER NOT NULL,
    PRIMARY KEY (event_type, from_version, to_version)
);

CREATE TABLE meta (
    key             TEXT    NOT NULL PRIMARY KEY,
    value           TEXT    NOT NULL
);
";

/// Run schema migrations on `conn`. Called during `Store::open`.
pub(crate) fn run_migrations(conn: &Connection) -> Result<(), Error> {
    let stored: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap_or(0);

    if stored > CURRENT_SCHEMA_VERSION {
        return Err(Error::SchemaMismatch {
            stored,
            required: CURRENT_SCHEMA_VERSION,
        });
    }

    if stored == CURRENT_SCHEMA_VERSION {
        return Ok(());
    }

    // Fresh database: create the full v1 schema.
    conn.execute_batch(SCHEMA_V1)?;
    conn.execute_batch(&format!(
        "PRAGMA user_version = {};",
        CURRENT_SCHEMA_VERSION
    ))?;

    Ok(())
}

/// Write the required `meta` entries if they don't already exist.
pub(crate) fn bootstrap_meta(conn: &Connection, encryption_mode: &str) -> Result<(), Error> {
    let now_us = now_us();
    conn.execute_batch(&format!(
        "INSERT OR IGNORE INTO meta(key, value) VALUES
            ('fossic_schema_version', '{}'),
            ('cce_version', 'fossic-cce-v1'),
            ('created_at_us', '{}'),
            ('created_by_version', '{}'),
            ('encryption_mode', '{}');",
        CURRENT_SCHEMA_VERSION,
        now_us,
        env!("CARGO_PKG_VERSION"),
        encryption_mode,
    ))?;
    Ok(())
}

/// Declares `_fossic/system` idempotently. Inserts directly (bypasses user-facing
/// stream API) so the internal stream is always present regardless of user policy.
pub(crate) fn bootstrap_system_streams(conn: &Connection) -> Result<(), Error> {
    conn.execute(
        "INSERT OR IGNORE INTO streams(id, declared_by, declared_at, description) \
         VALUES ('_fossic/system', 'fossic', ?1, 'Internal fossic system events')",
        rusqlite::params![now_us()],
    )?;
    Ok(())
}

pub(crate) fn now_us() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as i64
}
