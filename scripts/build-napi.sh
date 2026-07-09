#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Build all 5 fossic-node prebuilt .node addons for the current platform.
# Cross-compilation targets require the appropriate linker (see below).
#
# Usage:
#   ./scripts/build-napi.sh                          # native target only
#   ./scripts/build-napi.sh x86_64-unknown-linux-gnu
#   ./scripts/build-napi.sh aarch64-unknown-linux-gnu  # needs gcc-aarch64-linux-gnu
#   ./scripts/build-napi.sh all                        # all targets (requires toolchains)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if [ ! -d "fossic-node" ]; then
  echo "ERROR: fossic-node/ does not exist yet. Build prebuilts after the Node binding track lands."
  exit 1
fi

TARGETS=(
  "x86_64-unknown-linux-gnu"
  "aarch64-unknown-linux-gnu"
  "aarch64-apple-darwin"
  "x86_64-apple-darwin"
  "x86_64-pc-windows-msvc"
)

TARGET="${1:-native}"
mkdir -p dist/node

cd fossic-node
npm ci

if [ "$TARGET" = "native" ]; then
  echo "==> Building native target..."
  npx @napi-rs/cli build --platform --release
elif [ "$TARGET" = "all" ]; then
  for t in "${TARGETS[@]}"; do
    echo "==> Building $t..."
    # Cross-compile linker setup
    if [ "$t" = "aarch64-unknown-linux-gnu" ]; then
      if ! command -v aarch64-linux-gnu-gcc &>/dev/null; then
        echo "  WARNING: aarch64-linux-gnu-gcc not found, skipping $t"
        continue
      fi
      export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
      export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
    fi
    rustup target add "$t" 2>/dev/null || true
    npx @napi-rs/cli build --platform --target "$t" --release
  done
else
  echo "==> Building target: $TARGET..."
  rustup target add "$TARGET" 2>/dev/null || true
  npx @napi-rs/cli build --platform --target "$TARGET" --release
fi

echo ""
echo "==> Built prebuilts:"
ls -lh ./*.node 2>/dev/null || echo "(none in current directory)"
cp ./*.node "$REPO_ROOT/dist/node/" 2>/dev/null || true
echo "Artifacts in dist/node/:"
ls -lh "$REPO_ROOT/dist/node/"
