#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/lenso-cohort-preflight-test.XXXXXX")"
trap 'rm -rf -- "$fixture"' EXIT
if command -v rustup >/dev/null 2>&1; then
  cargo_dir="$(dirname -- "$(rustup which cargo)")"
else
  cargo_dir="$(dirname -- "$(command -v cargo)")"
fi

mkdir -p "$fixture/.github/scripts" "$fixture/crates/cohort-alpha/src" "$fixture/crates/cohort-beta/src" "$fixture/crates/itoa/src" "$fixture/cargo-home"
cp "$script_dir/release-cohort-preflight.sh" "$script_dir/release-set.sh" "$fixture/.github/scripts/"

cat >"$fixture/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/cohort-alpha", "crates/cohort-beta", "crates/itoa"]
resolver = "2"

[patch.crates-io]
cohort-alpha = { path = "crates/cohort-alpha" }
EOF
cat >"$fixture/crates/cohort-alpha/Cargo.toml" <<'EOF'
[package]
name = "cohort-alpha"
version = "0.1.0"
edition = "2024"
publish = true
EOF
cat >"$fixture/crates/cohort-alpha/src/lib.rs" <<'EOF'
pub fn answer() -> u8 { 42 }
EOF
cat >"$fixture/crates/cohort-beta/Cargo.toml" <<'EOF'
[package]
name = "cohort-beta"
version = "0.1.0"
edition = "2024"
publish = true

[dependencies]
cohort-alpha = "=0.1.0"
itoa = { path = "../itoa", version = "=1.0.18" }
EOF
cat >"$fixture/crates/cohort-beta/src/lib.rs" <<'EOF'
pub fn answer() -> u8 { cohort_alpha::answer() }
EOF
cat >"$fixture/crates/itoa/Cargo.toml" <<'EOF'
[package]
name = "itoa"
version = "1.0.18"
edition = "2024"
publish = true
EOF
cat >"$fixture/crates/itoa/src/lib.rs" <<'EOF'
pub fn placeholder() {}
EOF

PATH="$cargo_dir:$PATH" CARGO_HOME="$fixture/cargo-home" cargo generate-lockfile --manifest-path "$fixture/Cargo.toml" --offline >/dev/null
git -C "$fixture" init -q
git -C "$fixture" -c user.name='Cohort test' -c user.email='cohort-test@example.invalid' \
  add Cargo.toml Cargo.lock crates .github
git -C "$fixture" -c user.name='Cohort test' -c user.email='cohort-test@example.invalid' \
  commit -qm 'Prepare release cohort fixture'
release_sha="$(git -C "$fixture" rev-parse HEAD)"

output="$(
  cd "$fixture"
  env PATH="$cargo_dir:$PATH" CARGO_HOME="$fixture/cargo-home" EXPECTED_RELEASE_SET='[{"package_name":"cohort-alpha","version":"0.1.0"},{"package_name":"cohort-beta","version":"0.1.0"}]' \
    RELEASE_SHA="$release_sha" bash .github/scripts/release-cohort-preflight.sh 2>&1
)" || {
  printf 'cohort preflight failed:\n%s\n' "$output" >&2
  exit 1
}
records="$(printf '%s\n' "$output" | sed -n 's/^Cohort artifact preflight completed: //p')"
if ! grep -Fxq 'Fetching exact registry package itoa@1.0.18' <<<"$output"; then
  printf 'cohort preflight did not fetch the exact out-of-cohort version:\n%s\n' "$output" >&2
  exit 1
fi
if ! jq -e '
  length == 2
  and (map(.package_name) | sort) == ["cohort-alpha", "cohort-beta"]
  and all(.[]; .version == "0.1.0" and (.sha256 | test("^[0-9a-f]{64}$")))
' <<<"$records" >/dev/null; then
  printf 'cohort preflight returned an incomplete artifact receipt:\n%s\n' "$output" >&2
  exit 1
fi
printf '%s\n' 'two-package cohort with an already-published workspace dependency passed'
