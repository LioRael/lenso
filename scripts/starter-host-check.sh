#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

cp -R "$repo_root/templates/starter-host" "$tmp_dir/lenso-starter-host"

python3 - "$tmp_dir/lenso-starter-host/Cargo.toml" "$repo_root" <<'PY'
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
    raise SystemExit("starter host dependency shape changed")
cargo_toml.write_text(updated)
PY

cargo check --manifest-path "$tmp_dir/lenso-starter-host/Cargo.toml" --bins
cargo test --manifest-path "$tmp_dir/lenso-starter-host/Cargo.toml" --lib
