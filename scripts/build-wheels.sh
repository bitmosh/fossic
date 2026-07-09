#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Build all 15 fossic-py wheels locally using cibuildwheel.
# Requires: Docker (for Linux wheels), Python 3.x with cibuildwheel installed.
#
# Usage:
#   ./scripts/build-wheels.sh              # all platforms (Linux only without macOS/Windows SDK)
#   ./scripts/build-wheels.sh --platform linux
#   ./scripts/build-wheels.sh --platform macos
#   ./scripts/build-wheels.sh --platform windows
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if [ ! -d "fossic-py" ]; then
  echo "ERROR: fossic-py/ does not exist yet. Build wheels after the Python binding track lands."
  exit 1
fi

if ! command -v cibuildwheel &>/dev/null; then
  echo "ERROR: cibuildwheel not found. Install with: pip install cibuildwheel"
  exit 1
fi

PLATFORM="${1:-}"
ARGS=("fossic-py" "--config-file" ".cibuildwheel.toml" "--output-dir" "dist/wheels")

if [ -n "$PLATFORM" ]; then
  # Strip leading "--platform" flag if present
  PLATFORM="${PLATFORM#--platform=}"
  PLATFORM="${PLATFORM#--platform }"
  ARGS+=("--platform" "$PLATFORM")
fi

mkdir -p dist/wheels

echo "==> Building fossic-py wheels..."
cibuildwheel "${ARGS[@]}"

echo ""
echo "==> Built wheels:"
ls -lh dist/wheels/*.whl
echo ""
echo "==> SHA-256:"
sha256sum dist/wheels/*.whl
