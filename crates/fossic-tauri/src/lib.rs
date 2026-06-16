//! Tauri 2 IPC companion crate for fossic.
//!
//! Register all fossic IPC commands by adding the plugin to your Tauri builder:
//!
//! ```rust,ignore
//! use fossic::{OpenOptions, Store};
//! use fossic_tauri::plugin;
//!
//! fn main() {
//!     let store = Store::open("store.db", OpenOptions::default()).unwrap();
//!     tauri::Builder::default()
//!         .plugin(plugin(store))
//!         .run(tauri::generate_context!())
//!         .expect("error while running tauri application");
//! }
//! ```
//!
//! If you need to manage the `Store` yourself (e.g. to share it with non-fossic
//! commands), use `register_commands` in your setup closure instead:
//!
//! ```rust,ignore
//! # use fossic::{OpenOptions, Store};
//! # use fossic_tauri::register_commands;
//! # fn main() {
//! tauri::Builder::default()
//!     .invoke_handler(tauri::generate_handler![
//!         fossic_tauri::commands::fossic_list_streams,
//!         fossic_tauri::commands::fossic_list_branches,
//!         fossic_tauri::commands::fossic_read_range,
//!         fossic_tauri::commands::fossic_read_one,
//!         fossic_tauri::commands::fossic_read_batch,
//!         fossic_tauri::commands::fossic_read_by_external_id,
//!         fossic_tauri::commands::fossic_read_state_at_version,
//!         fossic_tauri::commands::fossic_subscribe,
//!         fossic_tauri::commands::fossic_unsubscribe,
//!         fossic_tauri::commands::fossic_read_by_correlation,
//!         fossic_tauri::commands::fossic_walk_causation,
//!     ])
//!     .setup(|app| {
//!         let store = Store::open(
//!             app.path().app_data_dir()?.join("store.db"),
//!             OpenOptions::default(),
//!         )?;
//!         app.manage(store);
//!         register_commands(app)?;
//!         Ok(())
//!     })
//!     .run(tauri::generate_context!())
//!     .expect("error running tauri");
//! # }
//! ```

pub mod commands;
pub mod serialization;

use fossic::Store;
use parking_lot::Mutex;
use std::collections::HashMap;
use tauri::{Manager, Runtime};

// ── Subscription map ──────────────────────────────────────────────────────────

/// One entry in the subscription map — handle plus the query parameters that
/// created it, cached here so commands can report them without re-querying core.
struct SubscriptionEntry {
    handle: fossic::SubscriptionHandle,
    stream_pattern: String,
    branch: String,
}

/// Snapshot of a single subscription's state, suitable for IPC serialization.
#[derive(serde::Serialize, Clone)]
pub struct SubscriberSnapshot {
    pub subscription_id: String,
    pub stream_pattern: String,
    pub branch: String,
    pub degraded: bool,
    pub queue_depth: Option<usize>,
    pub queue_capacity: Option<usize>,
}

/// Tauri-managed state that tracks active fossic subscriptions.
///
/// The map keys are UUID strings allocated by `fossic_subscribe`.
/// Dropping an entry's handle unsubscribes; `fossic_unsubscribe` removes and drops it.
pub struct SubscriptionMap(Mutex<HashMap<String, SubscriptionEntry>>);

impl SubscriptionMap {
    pub fn new() -> Self {
        SubscriptionMap(Mutex::new(HashMap::new()))
    }

    pub fn insert(
        &self,
        id: String,
        handle: fossic::SubscriptionHandle,
        stream_pattern: String,
        branch: String,
    ) {
        self.0.lock().insert(id, SubscriptionEntry { handle, stream_pattern, branch });
    }

    pub fn remove(&self, id: &str) {
        self.0.lock().remove(id);
    }

    /// Snapshot all active subscriptions for `fossic_list_subscribers`.
    pub fn snapshot_all(&self) -> Vec<SubscriberSnapshot> {
        self.0.lock().iter().map(|(id, entry)| SubscriberSnapshot {
            subscription_id: id.clone(),
            stream_pattern: entry.stream_pattern.clone(),
            branch: entry.branch.clone(),
            degraded: entry.handle.is_degraded(),
            queue_depth: entry.handle.queue_depth(),
            queue_capacity: entry.handle.queue_capacity(),
        }).collect()
    }

    /// Snapshot a single subscription by UUID for `fossic_subscription_status`.
    pub fn snapshot_one(&self, id: &str) -> Option<SubscriberSnapshot> {
        self.0.lock().get(id).map(|entry| SubscriberSnapshot {
            subscription_id: id.to_string(),
            stream_pattern: entry.stream_pattern.clone(),
            branch: entry.branch.clone(),
            degraded: entry.handle.is_degraded(),
            queue_depth: entry.handle.queue_depth(),
            queue_capacity: entry.handle.queue_capacity(),
        })
    }
}

impl Default for SubscriptionMap {
    fn default() -> Self {
        Self::new()
    }
}

// ── Plugin entry point ────────────────────────────────────────────────────────

/// Build the fossic Tauri plugin, taking ownership of the `Store`.
///
/// This is the idiomatic Tauri 2 way to register fossic commands. The plugin
/// manages both the `Store` and the `SubscriptionMap` as Tauri state.
pub fn plugin<R: Runtime>(store: Store) -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("fossic")
        .invoke_handler(tauri::generate_handler![
            commands::fossic_list_streams,
            commands::fossic_list_branches,
            commands::fossic_read_range,
            commands::fossic_read_one,
            commands::fossic_read_batch,
            commands::fossic_read_by_external_id,
            commands::fossic_read_state_at_version,
            commands::fossic_subscribe,
            commands::fossic_unsubscribe,
            commands::fossic_list_subscribers,
            commands::fossic_subscription_status,
            commands::fossic_read_by_correlation,
            commands::fossic_walk_causation,
        ])
        .setup(move |app, _api| {
            app.manage(store);
            app.manage(SubscriptionMap::new());
            Ok(())
        })
        .build()
}

/// Register the fossic `SubscriptionMap` state on an existing `App`.
///
/// Use this when you manage the `Store` yourself (via `app.manage(store)`) and
/// only need fossic-tauri to set up the subscription tracking state.
///
/// You must also register the commands via `.invoke_handler(tauri::generate_handler![...])`.
pub fn register_commands<R: Runtime>(
    app: &mut tauri::App<R>,
) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(SubscriptionMap::new());
    Ok(())
}

// ── test-helpers feature ──────────────────────────────────────────────────────

#[cfg(feature = "test-helpers")]
pub fn plugin_with_test_helpers<R: Runtime>(store: Store) -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("fossic")
        .invoke_handler(tauri::generate_handler![
            commands::fossic_list_streams,
            commands::fossic_list_branches,
            commands::fossic_read_range,
            commands::fossic_read_one,
            commands::fossic_read_batch,
            commands::fossic_read_by_external_id,
            commands::fossic_read_state_at_version,
            commands::fossic_subscribe,
            commands::fossic_unsubscribe,
            commands::fossic_list_subscribers,
            commands::fossic_subscription_status,
            commands::fossic_read_by_correlation,
            commands::fossic_walk_causation,
            commands::fossic_dispatch_test_event,
        ])
        .setup(move |app, _api| {
            app.manage(store);
            app.manage(SubscriptionMap::new());
            Ok(())
        })
        .build()
}
