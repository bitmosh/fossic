#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Verify SLSA-3 provenance for a fossic release artifact using slsa-verifier.
#
# Requires: slsa-verifier (https://github.com/slsa-framework/slsa-verifier)
#   brew install slsa-verifier        # macOS
#   go install github.com/slsa-framework/slsa-verifier/v2/cli/slsa-verifier@latest
#
# Usage:
#   ./scripts/verify-attestation.sh fossic-0.1.0.crate
#   ./scripts/verify-attestation.sh fossic-0.1.0.crate v0.1.0
set -euo pipefail

ARTIFACT="${1:-}"
TAG="${2:-}"

if [ -z "$ARTIFACT" ]; then
  echo "Usage: $0 <artifact-path> [<tag>]"
  echo "Example: $0 dist/crate/fossic-0.1.0.crate v0.1.0"
  exit 1
fi

if ! command -v slsa-verifier &>/dev/null; then
  echo "ERROR: slsa-verifier not found."
  echo "Install from: https://github.com/slsa-framework/slsa-verifier/releases"
  echo "  or: go install github.com/slsa-framework/slsa-verifier/v2/cli/slsa-verifier@latest"
  exit 1
fi

GITHUB_REPO="lattica/fossic"  # Update to the canonical repo path.

VERIFY_ARGS=(
  "verify-artifact" "$ARTIFACT"
  "--provenance-repository" "github.com/$GITHUB_REPO"
  "--source-uri"            "github.com/$GITHUB_REPO"
)

if [ -n "$TAG" ]; then
  VERIFY_ARGS+=("--source-tag" "$TAG")
fi

echo "==> Verifying SLSA provenance for: $ARTIFACT"
echo "    Repository: $GITHUB_REPO"
[ -n "$TAG" ] && echo "    Tag:        $TAG"
echo ""

slsa-verifier "${VERIFY_ARGS[@]}"

echo ""
echo "Provenance verified."
