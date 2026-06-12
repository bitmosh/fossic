import type { AppendJs } from '../index.d.ts'

// ── Sequence counter ──────────────────────────────────────────────────────────
// Global counter ensures each call produces a unique payload → unique CCE hash
// → distinct event ID → INSERT (not INSERT OR IGNORE no-op) → is_new=true.

let _seq = 0

/** Return an `AppendJs` with a globally unique payload. */
export function uniqueEv(
  streamId: string,
  eventType = 'TestEvent',
): AppendJs {
  return {
    streamId,
    eventType,
    payload: { seq: _seq++ },
  }
}

// ── Async helpers ─────────────────────────────────────────────────────────────

/**
 * Poll `predicate` every `intervalMs` until it returns true or `timeoutMs` elapses.
 * Returns `true` on success, `false` on timeout.
 */
export async function waitFor(
  predicate: () => boolean,
  timeoutMs = 2000,
  intervalMs = 20,
): Promise<boolean> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    if (predicate()) return true
    await new Promise(r => setTimeout(r, intervalMs))
  }
  return false
}
