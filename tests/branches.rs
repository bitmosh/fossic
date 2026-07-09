// SPDX-License-Identifier: Apache-2.0
use fossic::{Append, CreateBranch, Error, OpenOptions, Store};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

fn setup_stream(store: &Store, stream_id: &str) {
    store.declare_stream(stream_id, "test", None).unwrap();
}

fn append_n(store: &Store, stream_id: &str, branch: &str, n: u64) {
    for i in 0..n {
        store
            .append(Append {
                stream_id: stream_id.to_string(),
                branch: branch.to_string(),
                event_type: "E".to_string(),
                payload: serde_json::json!({"seq": i}),
                ..Default::default()
            })
            .unwrap();
    }
}

// ── create_branch ─────────────────────────────────────────────────────────────

#[test]
fn create_branch_basic() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");
    append_n(&store, "test/s", "main", 5);

    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "feature/v2".to_string(),
            parent_id: "main".to_string(),
            parent_version: 4,
            description: Some("v2 experiment".to_string()),
            alternatives: None,
        })
        .unwrap();

    let branches = store.list_branches("test/s").unwrap();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].id, "feature/v2");
    assert_eq!(branches[0].parent_id, "main");
    assert_eq!(branches[0].parent_version, 4);
    assert_eq!(branches[0].lifecycle, "ephemeral");
}

#[test]
fn create_branch_with_alternatives() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");

    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "strategy-a".to_string(),
            parent_id: "main".to_string(),
            parent_version: 0,
            description: None,
            alternatives: Some(serde_json::json!([
                {"name": "a", "score": 0.9},
                {"name": "b", "score": 0.7},
            ])),
        })
        .unwrap();

    let branches = store.list_branches("test/s").unwrap();
    let alt = branches[0].alternatives.as_ref().unwrap();
    assert!(alt.is_array());
    assert_eq!(alt.as_array().unwrap().len(), 2);
}

#[test]
fn create_branch_alternatives_not_array_fails() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");

    let result = store.create_branch(&CreateBranch {
        stream_id: "test/s".to_string(),
        branch_id: "b1".to_string(),
        parent_id: "main".to_string(),
        parent_version: 0,
        description: None,
        alternatives: Some(serde_json::json!({"not": "array"})),
    });
    assert!(
        matches!(result, Err(Error::InvalidAlternatives)),
        "expected InvalidAlternatives, got {:?}",
        result
    );
}

#[test]
fn create_main_branch_fails() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");

    let result = store.create_branch(&CreateBranch {
        stream_id: "test/s".to_string(),
        branch_id: "main".to_string(),
        parent_id: "main".to_string(),
        parent_version: 0,
        description: None,
        alternatives: None,
    });
    assert!(
        matches!(result, Err(Error::InvalidBranchId { .. })),
        "expected InvalidBranchId for 'main'"
    );
}

#[test]
fn create_branch_empty_id_fails() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");

    let result = store.create_branch(&CreateBranch {
        stream_id: "test/s".to_string(),
        branch_id: "".to_string(),
        parent_id: "main".to_string(),
        parent_version: 0,
        description: None,
        alternatives: None,
    });
    assert!(matches!(result, Err(Error::InvalidBranchId { .. })));
}

#[test]
fn create_branch_with_space_fails() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");

    let result = store.create_branch(&CreateBranch {
        stream_id: "test/s".to_string(),
        branch_id: "branch with spaces".to_string(),
        parent_id: "main".to_string(),
        parent_version: 0,
        description: None,
        alternatives: None,
    });
    assert!(matches!(result, Err(Error::InvalidBranchId { .. })));
}

// ── lifecycle ─────────────────────────────────────────────────────────────────

#[test]
fn promote_branch_from_ephemeral() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");
    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "b1".to_string(),
            parent_id: "main".to_string(),
            parent_version: 0,
            description: None,
            alternatives: None,
        })
        .unwrap();

    store
        .promote_branch("test/s", "b1", Some("tests passed"))
        .unwrap();

    let branches = store.list_branches("test/s").unwrap();
    assert_eq!(branches[0].lifecycle, "promoted");
    assert!(branches[0].closed_at.is_some());
    assert_eq!(branches[0].closed_reason.as_deref(), Some("tests passed"));
}

#[test]
fn promote_branch_idempotent() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");
    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "b1".to_string(),
            parent_id: "main".to_string(),
            parent_version: 0,
            description: None,
            alternatives: None,
        })
        .unwrap();
    store.promote_branch("test/s", "b1", None).unwrap();
    store.promote_branch("test/s", "b1", None).unwrap(); // second call: no error
}

#[test]
fn mark_dead_end_from_ephemeral() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");
    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "b1".to_string(),
            parent_id: "main".to_string(),
            parent_version: 0,
            description: None,
            alternatives: None,
        })
        .unwrap();

    store
        .mark_branch_dead_end("test/s", "b1", Some("similarity > 0.95"))
        .unwrap();

    let branches = store.list_branches("test/s").unwrap();
    assert_eq!(branches[0].lifecycle, "dead_end");
    assert_eq!(
        branches[0].closed_reason.as_deref(),
        Some("similarity > 0.95")
    );
}

#[test]
fn mark_dead_end_idempotent() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");
    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "b1".to_string(),
            parent_id: "main".to_string(),
            parent_version: 0,
            description: None,
            alternatives: None,
        })
        .unwrap();
    store.mark_branch_dead_end("test/s", "b1", None).unwrap();
    store.mark_branch_dead_end("test/s", "b1", None).unwrap(); // second call: no error
}

#[test]
fn promote_dead_end_fails() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");
    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "b1".to_string(),
            parent_id: "main".to_string(),
            parent_version: 0,
            description: None,
            alternatives: None,
        })
        .unwrap();
    store.mark_branch_dead_end("test/s", "b1", None).unwrap();

    let result = store.promote_branch("test/s", "b1", None);
    assert!(
        matches!(result, Err(Error::BranchLifecycleError { .. })),
        "promoting a dead_end branch must fail"
    );
}

#[test]
fn dead_end_promoted_branch_fails() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");
    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "b1".to_string(),
            parent_id: "main".to_string(),
            parent_version: 0,
            description: None,
            alternatives: None,
        })
        .unwrap();
    store.promote_branch("test/s", "b1", None).unwrap();

    let result = store.mark_branch_dead_end("test/s", "b1", None);
    assert!(
        matches!(result, Err(Error::BranchLifecycleError { .. })),
        "marking a promoted branch as dead_end must fail"
    );
}

#[test]
fn promote_nonexistent_branch_fails() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");

    let result = store.promote_branch("test/s", "nonexistent", None);
    assert!(matches!(result, Err(Error::BranchNotFound { .. })));
}

// ── list_branches ─────────────────────────────────────────────────────────────

#[test]
fn list_branches_ordered_by_creation() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");

    for name in &["branch-c", "branch-a", "branch-b"] {
        store
            .create_branch(&CreateBranch {
                stream_id: "test/s".to_string(),
                branch_id: name.to_string(),
                parent_id: "main".to_string(),
                parent_version: 0,
                description: None,
                alternatives: None,
            })
            .unwrap();
    }

    let branches = store.list_branches("test/s").unwrap();
    assert_eq!(branches.len(), 3);
    // Ordered by created_at ascending — insertion order.
    assert_eq!(branches[0].id, "branch-c");
    assert_eq!(branches[1].id, "branch-a");
    assert_eq!(branches[2].id, "branch-b");
}

#[test]
fn list_branches_empty_stream() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");
    let branches = store.list_branches("test/s").unwrap();
    assert!(branches.is_empty());
}

// ── resolve_chain ─────────────────────────────────────────────────────────────

#[test]
fn resolve_chain_main_is_single_segment() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");

    let chain = store.resolve_chain("test/s", "main").unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].branch_id, "main");
    assert!(chain[0].to_version.is_none());
}

#[test]
fn resolve_chain_one_level() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");
    append_n(&store, "test/s", "main", 10);

    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "feature".to_string(),
            parent_id: "main".to_string(),
            parent_version: 5,
            description: None,
            alternatives: None,
        })
        .unwrap();

    let chain = store.resolve_chain("test/s", "feature").unwrap();
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].branch_id, "main");
    assert_eq!(chain[0].to_version, Some(5));
    assert_eq!(chain[1].branch_id, "feature");
    assert!(chain[1].to_version.is_none());
}

#[test]
fn resolve_chain_two_levels() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");
    append_n(&store, "test/s", "main", 10);

    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "mid".to_string(),
            parent_id: "main".to_string(),
            parent_version: 7,
            description: None,
            alternatives: None,
        })
        .unwrap();
    append_n(&store, "test/s", "mid", 5);

    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "leaf".to_string(),
            parent_id: "mid".to_string(),
            parent_version: 3,
            description: None,
            alternatives: None,
        })
        .unwrap();

    let chain = store.resolve_chain("test/s", "leaf").unwrap();
    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0].branch_id, "main");
    assert_eq!(chain[0].to_version, Some(7));
    assert_eq!(chain[1].branch_id, "mid");
    assert_eq!(chain[1].to_version, Some(3));
    assert_eq!(chain[2].branch_id, "leaf");
    assert!(chain[2].to_version.is_none());
}

#[test]
fn resolve_chain_cached_after_first_call() {
    let (store, _dir) = open_tmp();
    setup_stream(&store, "test/s");
    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "b1".to_string(),
            parent_id: "main".to_string(),
            parent_version: 0,
            description: None,
            alternatives: None,
        })
        .unwrap();

    let chain1 = store.resolve_chain("test/s", "b1").unwrap();
    let chain2 = store.resolve_chain("test/s", "b1").unwrap();
    // Both calls return identical results.
    assert_eq!(chain1.len(), chain2.len());
    assert_eq!(chain1[0].branch_id, chain2[0].branch_id);
}
