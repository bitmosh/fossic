//! Minimal example showing how to embed fossic in a Tauri 2 app.
//!
//! Run from the workspace root:
//!     cargo run -p fossic-tauri --example basic
//!
//! This example has no webview; it just opens the store, appends an event,
//! and reads it back — demonstrating that the fossic IPC state wires up.

fn main() {
    use fossic::{Append, OpenOptions, Store};
    use fossic_tauri::plugin;

    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("demo.db");
    let store = Store::open(&db, OpenOptions::default()).expect("open store");

    store
        .declare_stream("demo/events", "example", None)
        .expect("declare");
    let id = store
        .append(Append {
            stream_id: "demo/events".into(),
            event_type: "DemoEvent".into(),
            payload: serde_json::json!({"hello": "world"}),
            ..Default::default()
        })
        .expect("append");

    println!("appended event: {}", id.to_hex());

    let events = store
        .read_range(fossic::ReadQuery::stream("demo/events"))
        .expect("read");
    println!("read back {} event(s)", events.len());

    // Plugin registration (builder not run in this headless example).
    let _ = plugin::<tauri::Wry>(store);
    println!("fossic-tauri plugin constructed successfully");
}
