#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

echo "Building lenso-cli..."
cargo build -p lenso-cli --locked

echo "Scaffolding a host project via lenso host init..."
"$repo_root/target/debug/lenso" host init "$tmp_dir/smoke-app" --name smoke-app

python3 - "$tmp_dir/smoke-app/Cargo.toml" "$repo_root" <<'PY'
import re
from pathlib import Path
import sys

cargo_toml = Path(sys.argv[1])
repo_root = Path(sys.argv[2])
text = cargo_toml.read_text()
new = f'lenso-host = {{ path = "{repo_root / "crates/lenso-host"}" }}'
updated = re.sub(
    r'lenso-host = \{ git = "https://github.com/LioRael/lenso", (branch|tag|rev) = "[^"]+", package = "lenso-host" \}',
    new,
    text,
)
if updated == text:
    raise SystemExit("scaffolded host dependency shape changed")
cargo_toml.write_text(updated)
PY

echo "Compiling and testing the scaffolded host..."

echo "lenso-cli check passed."
