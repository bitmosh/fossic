use crate::{error::Error, types::StoredEvent};
use std::collections::HashMap;

/// Transforms a msgpack-encoded payload from one `type_version` to the next.
///
/// Upcasters fire at **read time**, not append time. Stored events keep their
/// original `type_version` and original payload bytes. The id never changes.
///
/// Upcasters must be pure synchronous functions — no I/O, no side effects.
pub trait Upcaster: Send + Sync + 'static {
    fn upcast(&self, payload: &[u8]) -> Result<Vec<u8>, Error>;
}

struct UpcasterEntry {
    from: u32,
    to: u32,
    upcaster: Box<dyn Upcaster>,
}

/// In-memory registry of upcasters keyed by event_type.
/// Entries for each event_type are sorted by `from_version`.
#[derive(Default)]
pub(crate) struct UpcasterRegistry {
    entries: HashMap<String, Vec<UpcasterEntry>>,
}

impl UpcasterRegistry {
    pub fn register(&mut self, event_type: &str, from: u32, to: u32, upcaster: Box<dyn Upcaster>) {
        let vec = self.entries.entry(event_type.to_string()).or_default();
        vec.push(UpcasterEntry { from, to, upcaster });
        vec.sort_by_key(|e| e.from);
    }

    /// Walk the upcaster chain from `stored_version` as far as registered.
    ///
    /// The id of the returned `StoredEvent` is unchanged; only `payload` bytes change.
    ///
    /// The chain must be contiguous: each step's `from` must equal the previous `to`.
    /// A gap returns `UpcasterChainGap`. If no upcasters are registered for the
    /// event_type, or the stored version is already at/beyond the chain, returns
    /// the original bytes unchanged.
    pub fn apply(
        &self,
        event_type: &str,
        stored_version: u32,
        mut payload: Vec<u8>,
    ) -> Result<Vec<u8>, Error> {
        let entries = match self.entries.get(event_type) {
            Some(e) if !e.is_empty() => e,
            _ => return Ok(payload),
        };

        let mut current = stored_version;
        loop {
            let entry = entries.iter().find(|e| e.from == current);
            match entry {
                None => {
                    // No upcaster for current version. Check if this is a gap
                    // (there are upcasters for higher versions) or the end of chain.
                    let has_higher = entries.iter().any(|e| e.from > current);
                    if has_higher {
                        return Err(Error::UpcasterChainGap {
                            event_type: event_type.to_string(),
                            from: current,
                        });
                    }
                    break;
                }
                Some(e) => {
                    payload = e.upcaster.upcast(&payload)?;
                    current = e.to;
                }
            }
        }
        Ok(payload)
    }
}

/// Apply the upcaster chain to a single `StoredEvent`.
///
/// The event's `id` and `type_version` are unchanged; only `payload` bytes
/// are updated if upcasters are registered for the event type.
pub(crate) fn apply_upcaster(
    registry: &UpcasterRegistry,
    mut event: StoredEvent,
) -> Result<StoredEvent, Error> {
    event.payload = registry.apply(&event.event_type, event.type_version, event.payload)?;
    Ok(event)
}
