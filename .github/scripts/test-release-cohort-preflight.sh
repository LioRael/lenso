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

mkdir -p "$fixture/.github/scripts" "$fixture/crates/cohort-alpha/src" "$fixture/crates/cohort-beta/src" "$fixture/crates/equivalent/src" "$fixture/crates/fnv/src" "$fixture/crates/test-only-fixture/src" "$fixture/cargo-home"
cp "$script_dir/release-cohort-preflight.sh" "$script_dir/release-set.sh" "$fixture/.github/scripts/"

cat >"$fixture/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/cohort-alpha", "crates/cohort-beta", "crates/equivalent", "crates/fnv", "crates/test-only-fixture"]
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

[dependencies]
fnv = { path = "../fnv", version = "=1.0.7" }
EOF
cat >"$fixture/crates/cohort-alpha/src/lib.rs" <<'EOF'
pub fn answer() -> u8 { std::mem::size_of::<fnv::FnvHasher>() as u8 }
EOF
cat >"$fixture/crates/cohort-beta/Cargo.toml" <<'EOF'
[package]
name = "cohort-beta"
version = "0.1.0"
edition = "2024"
publish = true

[dependencies]
cohort-alpha = "=0.1.0"

[dev-dependencies]
equivalent = { path = "../equivalent", version = "=1.0.2" }
test-only-fixture = { path = "../test-only-fixture" }
EOF
cat >"$fixture/crates/cohort-beta/src/lib.rs" <<'EOF'
pub fn answer() -> u8 { cohort_alpha::answer() }
EOF
cat >"$fixture/crates/equivalent/Cargo.toml" <<'EOF'
[package]
name = "equivalent"
version = "1.0.2"
edition = "2024"
publish = true
EOF
cat >"$fixture/crates/equivalent/src/lib.rs" <<'EOF'
pub fn placeholder() {}
EOF
cat >"$fixture/crates/fnv/Cargo.toml" <<'EOF'
[package]
name = "fnv"
version = "1.0.7"
edition = "2024"
publish = true
EOF
cat >"$fixture/crates/fnv/src/lib.rs" <<'EOF'
pub fn placeholder() {}
EOF
cat >"$fixture/crates/test-only-fixture/Cargo.toml" <<'EOF'
[package]
name = "test-only-fixture"
version = "0.1.0"
edition = "2024"
publish = false
EOF
cat >"$fixture/crates/test-only-fixture/src/lib.rs" <<'EOF'
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
if ! grep -Fxq 'Fetching exact registry package fnv@1.0.7' <<<"$output" ||
  ! grep -Fxq 'Staged exact registry source fnv@1.0.7' <<<"$output" ||
  ! grep -Fxq 'Fetching exact registry package equivalent@1.0.2' <<<"$output" ||
  ! grep -Fxq 'Normalized scratch lock only for omitted private dev fixture edges' <<<"$output"; then
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

scratch="$fixture"
source <(sed -n '/^verify_fixture_only_lock_change() {/,/^}/p' "$script_dir/release-cohort-preflight.sh")
lock_before=$'[[package]]\nname = "cohort-beta"\nversion = "0.1.0"\ndependencies = [\n "cohort-alpha",\n "test-only-fixture",\n]'
lock_allowed=$'[[package]]\nname = "cohort-beta"\nversion = "0.1.0"\ndependencies = [\n "cohort-alpha",\n]'
lock_bad_version=$'[[package]]\nname = "cohort-beta"\nversion = "0.1.1"\ndependencies = [\n "cohort-alpha",\n]'
lock_bad_checksum=$'[[package]]\nname = "cohort-beta"\nversion = "0.1.0"\nchecksum = "unexpected"\ndependencies = [\n "cohort-alpha",\n]'
lock_bad_edge=$'[[package]]\nname = "cohort-beta"\nversion = "0.1.0"\ndependencies = [\n]'
allowed_edge=$'cohort-beta\ttest-only-fixture'
printf '%s\n' "$lock_before" >"$fixture/before.lock"
printf '%s\n' "$lock_allowed" >"$fixture/allowed.lock"
printf '%s\n' "$allowed_edge" >"$fixture/allowed.tsv"
if ! verify_fixture_only_lock_change \
  "$fixture/before.lock" "$fixture/allowed.lock" "$fixture/allowed.tsv"; then
  printf '%s\n' 'fixture-only lock normalization was rejected' >&2
  exit 1
fi
for unexpected_lock in "$lock_bad_version" "$lock_bad_checksum" "$lock_bad_edge"; do
  printf '%s\n' "$unexpected_lock" >"$fixture/bad.lock"
  if verify_fixture_only_lock_change \
    "$fixture/before.lock" "$fixture/bad.lock" "$fixture/allowed.tsv"; then
    printf '%s\n' 'lock normalization accepted a non-fixture change' >&2
    exit 1
  fi
done

source <(sed -n '/^published_transitive_workspace_dependencies() {/,/^}/p' "$script_dir/release-cohort-preflight.sh")
workspace_metadata='{"packages":[{"name":"fnv","version":"1.0.7"},{"name":"equivalent","version":"1.0.2"}]}'
published_metadata='{"packages":[{"name":"fnv","version":"1.0.7","source":"registry+https://github.com/rust-lang/crates.io-index"},{"name":"equivalent","version":"1.0.2","source":"registry+https://github.com/rust-lang/crates.io-index"},{"name":"equivalent","version":"1.0.3","source":"registry+https://github.com/rust-lang/crates.io-index"},{"name":"equivalent","version":"1.0.2","source":"git+https://example.invalid/equivalent"},{"name":"unrelated","version":"1.0.0","source":"registry+https://github.com/rust-lang/crates.io-index"}]}'
transitive_workspace_dependencies="$(published_transitive_workspace_dependencies "$workspace_metadata" "$published_metadata" | sort)"
if [[ "$transitive_workspace_dependencies" != $'equivalent\t1.0.2\nfnv\t1.0.7' ]]; then
  printf 'transitive workspace dependency selection was not exact:\n%s\n' "$transitive_workspace_dependencies" >&2
  exit 1
fi
source <(sed -n '/^published_exact_workspace_requirements() {/,/^}/p' "$script_dir/release-cohort-preflight.sh")
published_requirement_metadata='{"packages":[{"name":"fnv","version":"1.0.7","source":"registry+https://github.com/rust-lang/crates.io-index","dependencies":[{"name":"equivalent","req":"=1.0.2","source":"registry+https://github.com/rust-lang/crates.io-index","kind":"dev"},{"name":"equivalent","req":"=1.0.3","source":"registry+https://github.com/rust-lang/crates.io-index","kind":"dev"},{"name":"fnv","req":"^1.0.7","source":"registry+https://github.com/rust-lang/crates.io-index","kind":"dev"},{"name":"equivalent","req":"=1.0.2","source":"git+https://example.invalid/equivalent","kind":"dev"}]}]}'
exact_requirements="$(published_exact_workspace_requirements "$workspace_metadata" "$published_requirement_metadata")"
if [[ "$exact_requirements" != $'equivalent\t1.0.2' ]]; then
  printf 'published exact workspace requirement selection was not exact:\n%s\n' "$exact_requirements" >&2
  exit 1
fi
printf '%s\n' 'two-package cohort with published runtime and private dev dependencies passed'
