use std::{
    cmp::Ordering,
    collections::BinaryHeap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering as AOrdering},
        Arc, Weak,
    },
    time::Duration,
};

use crate::{error::Error, system_stream::SystemStreamWriter, types::SnapshotInfo};

// ── StoreOps ──────────────────────────────────────────────────────────────────

/// Capability surface the background executor needs from the store.
///
/// Implemented on `StoreInner` in `store.rs`. `BackgroundExecutor` holds a
/// `Weak<dyn StoreOps>` so the executor never keeps the store alive past its
/// natural drop — if the upgrade fails the task is silently skipped.
pub(crate) trait StoreOps: Send + Sync + 'static {
    fn bg_gc_orphaned_snapshots(&self) -> Result<usize, Error>;
    fn bg_take_snapshot(&self, stream_id: &str, branch: &str) -> Result<SnapshotInfo, Error>;
}

// ── TaskPriority ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
}

// ── TaskKind ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum TaskKind {
    GcOrphanSnapshots,
    TakeSnapshot {
        stream_id: String,
        branch: String,
    },
    /// One-shot or recurring user-supplied closure. The closure captures
    /// any context it needs (e.g. `Arc<HnswProvider>`) at scheduling time.
    /// Custom tasks are never `persist_on_drop` — set that field to `false`.
    Custom(std::sync::Arc<dyn Fn() + Send + Sync + 'static>),
}

impl TaskKind {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            TaskKind::GcOrphanSnapshots => "GcOrphanSnapshots",
            TaskKind::TakeSnapshot { .. } => "TakeSnapshot",
            TaskKind::Custom(_) => "Custom",
        }
    }
}

// ── BacklogTask ───────────────────────────────────────────────────────────────

pub struct BacklogTask {
    pub priority: TaskPriority,
    /// Absolute deadline in microseconds since Unix epoch.
    /// Earlier deadline wins when priorities are equal.
    pub deadline_us: i64,
    /// When `true`, a `DeferredTaskDropped` system event is emitted at shutdown
    /// if this task was not executed. When `false`, the task is logged and dropped.
    pub persist_on_drop: bool,
    pub kind: TaskKind,
    /// When `Some(d)`, the task re-queues itself with `deadline = now + d`
    /// after each successful execution. Recurring tasks run at most once per
    /// quiescence window regardless of their deadline.
    pub recurring_interval: Option<Duration>,
}

impl PartialEq for BacklogTask {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.deadline_us == other.deadline_us
    }
}
impl Eq for BacklogTask {}

impl Ord for BacklogTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first. Equal priority: earlier deadline first.
        // BinaryHeap is a max-heap, so we invert the deadline comparison.
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.deadline_us.cmp(&self.deadline_us))
    }
}

impl PartialOrd for BacklogTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// ── QuiescenceMonitor ─────────────────────────────────────────────────────────

/// Tracks the most recent write and subscription-dispatch timestamps so the
/// background executor can determine when the store is quiescent.
///
/// Both timestamps are initialised to `now_us()` at construction so the store
/// is never quiescent at open time.
pub(crate) struct QuiescenceMonitor {
    last_write_us: AtomicI64,
    last_subscription_dispatch_us: AtomicI64,
}

impl QuiescenceMonitor {
    pub(crate) fn new() -> Self {
        let now = crate::schema::now_us();
        QuiescenceMonitor {
            last_write_us: AtomicI64::new(now),
            last_subscription_dispatch_us: AtomicI64::new(now),
        }
    }

    /// Call after each successful event write (append / append_batch / append_if).
    pub(crate) fn note_write(&self) {
        self.last_write_us
            .store(crate::schema::now_us(), AOrdering::Relaxed);
    }

    /// Call after each subscription dispatch round.
    /// Wired into `start_dispatcher` in store.rs.
    pub(crate) fn note_dispatch(&self) {
        self.last_subscription_dispatch_us
            .store(crate::schema::now_us(), AOrdering::Relaxed);
    }

    /// Returns `true` when both write and dispatch timestamps are at least
    /// `window_us` microseconds in the past.
    pub(crate) fn is_quiescent(&self, window_us: i64) -> bool {
        let now = crate::schema::now_us();
        let lw = self.last_write_us.load(AOrdering::Relaxed);
        let ld = self.last_subscription_dispatch_us.load(AOrdering::Relaxed);
        (now - lw) >= window_us && (now - ld) >= window_us
    }
}

// ── BackgroundExecutor ────────────────────────────────────────────────────────

pub struct BackgroundExecutor {
    task_tx: crossbeam_channel::Sender<BacklogTask>,
    stop_flag: Arc<AtomicBool>,
    done_rx: crossbeam_channel::Receiver<()>,
    thread: Option<std::thread::JoinHandle<()>>,
    grace_timeout: Duration,
}

impl BackgroundExecutor {
    pub(crate) fn spawn(
        store_ops: Weak<dyn StoreOps>,
        quiescence: Arc<QuiescenceMonitor>,
        db_path: PathBuf,
        grace_timeout: Duration,
        quiescence_window_us: i64,
    ) -> Result<Self, Error> {
        let (task_tx, task_rx) = crossbeam_channel::unbounded::<BacklogTask>();
        let (done_tx, done_rx) = crossbeam_channel::bounded::<()>(1);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_thread = Arc::clone(&stop_flag);

        let handle = std::thread::Builder::new()
            .name("fossic-bg".to_string())
            .spawn(move || {
                bg_thread_loop(
                    store_ops,
                    quiescence,
                    task_rx,
                    stop_flag_thread,
                    db_path,
                    done_tx,
                    quiescence_window_us,
                );
            })
            .map_err(|e| Error::Internal(format!("fossic-bg spawn failed: {e}")))?;

        Ok(BackgroundExecutor {
            task_tx,
            stop_flag,
            done_rx,
            thread: Some(handle),
            grace_timeout,
        })
    }

    pub fn schedule(&self, task: BacklogTask) {
        let _ = self.task_tx.send(task);
    }
}

impl Drop for BackgroundExecutor {
    fn drop(&mut self) {
        self.stop_flag.store(true, AOrdering::Relaxed);
        if let Some(handle) = self.thread.take() {
            match self.done_rx.recv_timeout(self.grace_timeout) {
                Ok(()) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    let _ = handle.join();
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // Grace period exceeded — detach. The thread will finish when it
                    // next wakes from its 500ms sleep and sees the stop flag.
                    eprintln!(
                        "[WARN fossic] fossic-bg did not stop within grace period ({:?}) — detaching",
                        self.grace_timeout,
                    );
                }
            }
        }
    }
}

// ── Thread loop ───────────────────────────────────────────────────────────────

fn bg_thread_loop(
    store_ops: Weak<dyn StoreOps>,
    quiescence: Arc<QuiescenceMonitor>,
    task_rx: crossbeam_channel::Receiver<BacklogTask>,
    stop_flag: Arc<AtomicBool>,
    db_path: PathBuf,
    done_tx: crossbeam_channel::Sender<()>,
    quiescence_window_us: i64,
) {
    let mut heap: BinaryHeap<BacklogTask> = BinaryHeap::new();
    // Lazy-initialised on first DeferredTaskDropped emission at shutdown.
    // Third SystemStreamWriter instance — separate connection, same WAL file.
    let mut sys_writer: Option<SystemStreamWriter> = None;

    loop {
        std::thread::sleep(Duration::from_millis(500));

        // ── Stop-flag check ───────────────────────────────────────────────────
        if stop_flag.load(AOrdering::Relaxed) {
            // Drain remaining channel tasks into heap before emitting/dropping.
            while let Ok(task) = task_rx.try_recv() {
                heap.push(task);
            }
            // Emit DeferredTaskDropped for persist_on_drop tasks; log + drop others.
            while let Some(task) = heap.pop() {
                if task.persist_on_drop {
                    let payload = serde_json::json!({
                        "task": task.kind.name(),
                        "priority": format!("{:?}", task.priority),
                        "deadline_us": task.deadline_us,
                    });
                    if sys_writer.is_none() {
                        sys_writer = SystemStreamWriter::new(&db_path);
                    }
                    if let Some(ref mut w) = sys_writer {
                        w.emit("DeferredTaskDropped", &payload, None);
                    }
                } else {
                    eprintln!(
                        "[fossic] fossic-bg: dropping task '{}' (persist_on_drop=false)",
                        task.kind.name()
                    );
                }
            }
            break;
        }

        // ── Drain channel into heap ───────────────────────────────────────────
        while let Ok(task) = task_rx.try_recv() {
            heap.push(task);
        }

        // ── Quiescence gate ───────────────────────────────────────────────────
        if heap.is_empty() || !quiescence.is_quiescent(quiescence_window_us) {
            continue;
        }

        // ── Execute one task ──────────────────────────────────────────────────
        if let Some(task) = heap.pop() {
            let recurring = task.recurring_interval;
            if let Some(ops) = store_ops.upgrade() {
                execute_task(&*ops, &task);
            }
            // upgrade() returning None means the store has been dropped — skip.
            // Re-queue if recurring (regardless of whether upgrade succeeded —
            // if the store is gone the next upgrade will also fail and the task
            // will be dropped cleanly at the next stop-flag check).
            if let Some(interval) = recurring {
                heap.push(BacklogTask {
                    priority: task.priority,
                    deadline_us: crate::schema::now_us() + interval.as_micros() as i64,
                    persist_on_drop: task.persist_on_drop,
                    kind: task.kind.clone(),
                    recurring_interval: Some(interval),
                });
            }
        }
    }

    let _ = done_tx.send(());
}

fn execute_task(ops: &dyn StoreOps, task: &BacklogTask) {
    match &task.kind {
        TaskKind::GcOrphanSnapshots => {
            if let Err(e) = ops.bg_gc_orphaned_snapshots() {
                eprintln!("[WARN fossic] fossic-bg: GcOrphanSnapshots failed: {e}");
            }
        }
        TaskKind::TakeSnapshot { stream_id, branch } => {
            if let Err(e) = ops.bg_take_snapshot(stream_id, branch) {
                eprintln!(
                    "[WARN fossic] fossic-bg: TakeSnapshot({stream_id}, {branch}) failed: {e}"
                );
            }
        }
        TaskKind::Custom(f) => {
            let f = Arc::clone(f);
            // AssertUnwindSafe: Custom closures are not required to be UnwindSafe.
            // Partially-mutated state after a panic is acceptable — Custom tasks are
            // best-effort by design. Imposing UnwindSafe as a trait bound would break
            // the existing TaskKind::Custom API (SR-10 A-6, B-3 resolution).
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || f()));
            if let Err(panic_val) = result {
                let msg = if let Some(s) = panic_val.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_val.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "<non-string panic payload>".to_string()
                };
                eprintln!(
                    "[WARN fossic] fossic-bg: TaskKind::Custom panicked: {msg}; \
                     task discarded, executor continues",
                );
            }
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_priority_high_before_low() {
        let mut heap = BinaryHeap::new();
        heap.push(BacklogTask {
            priority: TaskPriority::Low,
            deadline_us: 1000,
            persist_on_drop: false,
            kind: TaskKind::GcOrphanSnapshots,
            recurring_interval: None,
        });
        heap.push(BacklogTask {
            priority: TaskPriority::High,
            deadline_us: 2000,
            persist_on_drop: false,
            kind: TaskKind::GcOrphanSnapshots,
            recurring_interval: None,
        });
        let first = heap.pop().unwrap();
        assert_eq!(first.priority, TaskPriority::High);
    }

    #[test]
    fn task_equal_priority_earlier_deadline_first() {
        let mut heap = BinaryHeap::new();
        heap.push(BacklogTask {
            priority: TaskPriority::Low,
            deadline_us: 2000,
            persist_on_drop: false,
            kind: TaskKind::GcOrphanSnapshots,
            recurring_interval: None,
        });
        heap.push(BacklogTask {
            priority: TaskPriority::Low,
            deadline_us: 1000,
            persist_on_drop: false,
            kind: TaskKind::GcOrphanSnapshots,
            recurring_interval: None,
        });
        let first = heap.pop().unwrap();
        assert_eq!(first.deadline_us, 1000);
    }

    #[test]
    fn quiescence_not_quiescent_immediately_after_write() {
        let qm = QuiescenceMonitor::new();
        qm.note_write();
        assert!(!qm.is_quiescent(2_000_000));
    }

    #[test]
    fn quiescence_not_quiescent_immediately_after_dispatch() {
        let qm = QuiescenceMonitor::new();
        qm.note_dispatch();
        assert!(!qm.is_quiescent(2_000_000));
    }

    #[test]
    fn custom_task_closure_executes() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };

        struct MockOps;
        impl StoreOps for MockOps {
            fn bg_gc_orphaned_snapshots(&self) -> Result<usize, crate::error::Error> {
                Ok(0)
            }
            fn bg_take_snapshot(
                &self,
                _: &str,
                _: &str,
            ) -> Result<crate::types::SnapshotInfo, crate::error::Error> {
                Err(crate::error::Error::NotImplemented { feature: "mock" })
            }
        }

        let ran = Arc::new(AtomicBool::new(false));
        let ran2 = Arc::clone(&ran);

        let task = BacklogTask {
            priority: TaskPriority::Normal,
            deadline_us: 0,
            persist_on_drop: false,
            kind: TaskKind::Custom(Arc::new(move || {
                ran2.store(true, Ordering::SeqCst);
            })),
            recurring_interval: None,
        };

        execute_task(&MockOps as &dyn StoreOps, &task);
        assert!(ran.load(Ordering::SeqCst), "Custom closure must execute");
    }
}
