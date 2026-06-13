# fossic — canonical test runner
#
# Usage:
#   just test        # all three binding test suites (Rust + Python + Node)
#   just test-rust   # Rust workspace only (faster during Rust-only dev)
#   just test-py     # Python only (faster during Python-only dev)
#   just test-node   # Node only (faster during Node-only dev)
#
# First run: ~2 min (Python venv + maturin release build + npm install).
# Subsequent runs: ~30 s (cargo incremental, maturin incremental, cached deps).

# Show available targets
default:
    @just --list

# ── Full suite ────────────────────────────────────────────────────────────────

# Run Rust, Python, and Node test suites; report counts; exit non-zero on any failure
test:
    #!/usr/bin/env bash
    set -euo pipefail

    RUST_TMP=$(mktemp)
    PY_TMP=$(mktemp)
    NODE_TMP=$(mktemp)
    cleanup() { rm -f "$RUST_TMP" "$PY_TMP" "$NODE_TMP"; }
    trap cleanup EXIT

    EXIT_RUST=0
    EXIT_PY=0
    EXIT_NODE=0

    # ── Rust ────────────────────────────────────────────────────────────────
    echo ""
    echo "━━━ Rust ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    cargo test --workspace --all-features > >(tee "$RUST_TMP") 2>&1 || EXIT_RUST=$?

    # ── Python ──────────────────────────────────────────────────────────────
    echo ""
    echo "━━━ Python ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    if [ ! -f .venv-test/bin/maturin ] || [ ! -f .venv-test/bin/pytest ]; then
        echo "==> First run: creating Python test venv (.venv-test/)..."
        python3 -m venv .venv-test
        .venv-test/bin/pip install --quiet maturin pytest
    fi
    (cd fossic-py && ../.venv-test/bin/maturin develop --release)
    PYTHONPATH="fossic-py/python" .venv-test/bin/pytest fossic-py/tests/ -v > >(tee "$PY_TMP") 2>&1 || EXIT_PY=$?

    # ── Node ────────────────────────────────────────────────────────────────
    echo ""
    echo "━━━ Node ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    if [ ! -d fossic-node/node_modules ]; then
        echo "==> First run: installing fossic-node dependencies..."
        (cd fossic-node && npm install)
    fi
    (cd fossic-node && npm run build)
    (cd fossic-node && npm test) > >(tee "$NODE_TMP") 2>&1 || EXIT_NODE=$?

    # ── Summary ─────────────────────────────────────────────────────────────
    RUST_PASS=$(grep -oP 'ok\. \K\d+(?= passed)' "$RUST_TMP" | awk '{s+=$1} END{print s+0}')
    PY_PASS=$(grep -oP '\d+(?= passed)' "$PY_TMP" | tail -1); PY_PASS=${PY_PASS:-0}
    NODE_PASS=$(grep -oP 'Tests\s+\K\d+' "$NODE_TMP" | head -1); NODE_PASS=${NODE_PASS:-?}

    echo ""
    echo "━━━ Summary ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    printf "  Rust   : %s passed\n" "$RUST_PASS"
    printf "  Python : %s passed\n" "$PY_PASS"
    printf "  Node   : %s passed\n" "$NODE_PASS"

    if [ "$EXIT_RUST" -ne 0 ] || [ "$EXIT_PY" -ne 0 ] || [ "$EXIT_NODE" -ne 0 ]; then
        echo ""
        echo "ERROR: one or more test suites failed"
        exit 1
    fi
    echo "All tests passed."

# ── Per-binding targets (for development iteration) ───────────────────────────

# Rust workspace tests (all features, includes fossic-tauri integration tests)
test-rust:
    cargo test --workspace --all-features

# Python tests (creates .venv-test if absent, always rebuilds maturin extension)
test-py:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f .venv-test/bin/maturin ] || [ ! -f .venv-test/bin/pytest ]; then
        echo "==> First run: creating Python test venv (.venv-test/)..."
        python3 -m venv .venv-test
        .venv-test/bin/pip install --quiet maturin pytest
    fi
    (cd fossic-py && ../.venv-test/bin/maturin develop --release)
    PYTHONPATH="fossic-py/python" .venv-test/bin/pytest fossic-py/tests/ -v

# Node tests (installs deps if absent, always rebuilds native module)
test-node:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -d fossic-node/node_modules ]; then
        echo "==> First run: installing fossic-node dependencies..."
        (cd fossic-node && npm install)
    fi
    (cd fossic-node && npm run build)
    (cd fossic-node && npm test)
