#!/usr/bin/env bash
set -euo pipefail
bun run release:version
bun install --ignore-scripts
bun install --frozen-lockfile --ignore-scripts
bun run fmt
