use crate::system_stream::SystemStreamWriter;

pub(crate) fn emit_project_registered(
    writer: &mut SystemStreamWriter,
    source_store: &str,
    local_store_path: &str,
    subscribe_pattern: &str,
    project_description: &str,
) {
    let payload = serde_json::json!({
        "source_store": source_store,
        "local_store_path": local_store_path,
        "subscribe_pattern": subscribe_pattern,
        "project_description": project_description,
    });
    let tags = serde_json::json!({"source_store": source_store});
    writer.emit("ProjectRegistered", &payload, Some(&tags));
}

pub(crate) fn emit_relay_heartbeat(
    writer: &mut SystemStreamWriter,
    source_store: &str,
    last_event_version: i64,
    queue_lag: u64,
    uptime_us: i64,
) {
    let payload = serde_json::json!({
        "source_store": source_store,
        "last_event_version": last_event_version,
        "queue_lag": queue_lag,
        "uptime_us": uptime_us,
    });
    let tags = serde_json::json!({"source_store": source_store});
    writer.emit("RelayHeartbeat", &payload, Some(&tags));
}
