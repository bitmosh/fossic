#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Run 5 synthetic consumer profiles against a local fossic store.
# Each profile exercises a distinct access pattern and prints timing.
#
# Usage:
#   ./scripts/consumer-profile-test.sh
#   ./scripts/consumer-profile-test.sh --profile bulk-write
#   ./scripts/consumer-profile-test.sh --update-baseline    # rewrite benchmarks/baseline.json
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PROFILE="${1:-all}"
UPDATE_BASELINE=false
if [ "$1" = "--update-baseline" ]; then
  PROFILE="all"
  UPDATE_BASELINE=true
fi
if [ "$2" = "--update-baseline" ]; then
  UPDATE_BASELINE=true
fi

if ! command -v python3 &>/dev/null; then
  echo "ERROR: python3 required"
  exit 1
fi

BENCH_SCRIPT="benchmarks/sqlite_wal_payload_sweep.py"
if [ ! -f "$BENCH_SCRIPT" ]; then
  echo "ERROR: $BENCH_SCRIPT not found. This script requires the benchmark suite."
  exit 1
fi

run_profile() {
  local name="$1"
  local scenario="$2"
  local description="$3"

  echo "──────────────────────────────────────────────────────────"
  echo "Profile: $name"
  echo "  $description"
  echo ""
  python3 "$BENCH_SCRIPT" --scenario "$scenario" --iterations 200
  echo ""
}

case "$PROFILE" in
  all|bulk-write)
    run_profile \
      "bulk-write" \
      "scenario_c_40kb" \
      "40 KB payload, 1000 sequential appends — simulates a high-throughput ingest agent."
    ;;&

  all|read-aggregate)
    run_profile \
      "read-aggregate" \
      "scenario_d_read_aggregate" \
      "Aggregate over 10 000 events, fold into running count — simulates a state-projection consumer."
    ;;&

  all|causation-walk)
    run_profile \
      "causation-walk" \
      "scenario_e_causation_walk" \
      "walk_causation forward, depth 20, 1000 leaf nodes — simulates a reactive event fan-out consumer."
    ;;&

  all|cursor-consumer)
    # Profile 4: cursor-based consumer with checkpointing
    echo "──────────────────────────────────────────────────────────"
    echo "Profile: cursor-consumer"
    echo "  Polling consumer: read 100-event page, advance cursor, repeat 50 cycles."
    echo "  Simulates a durable subscription with at-least-once delivery."
    echo ""
    python3 - <<'PYEOF'
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
# Placeholder: implement when fossic Python binding is available.
print("PLACEHOLDER: cursor-consumer profile requires fossic-py (Track E)")
PYEOF
    echo ""
    ;;&

  all|cross-stream)
    # Profile 5: cross-stream correlation
    echo "──────────────────────────────────────────────────────────"
    echo "Profile: cross-stream"
    echo "  read_by_correlation across 10 streams, 1 000 events each."
    echo "  Simulates a policy-scout audit sweep that follows event lineage."
    echo ""
    python3 - <<'PYEOF'
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
# Placeholder: implement when fossic Python binding is available.
print("PLACEHOLDER: cross-stream profile requires fossic-py (Track E)")
PYEOF
    echo ""
    ;;
esac

if $UPDATE_BASELINE; then
  echo "==> --update-baseline: regenerating benchmarks/baseline.json"
  python3 "$BENCH_SCRIPT" \
    --output "benchmarks/baseline.json" \
    --write-baseline
  echo "Baseline written to benchmarks/baseline.json"
fi
