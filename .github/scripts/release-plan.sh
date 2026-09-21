#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

# Keep the historical entrypoint useful for local callers while the workflow
# names the stronger artifact-based preflight explicitly.
exec bash "$SCRIPT_DIR/release-cohort-preflight.sh"
