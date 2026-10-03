#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd "$root"
: "${LENSO_STORE_PG_URL:?Use a dedicated local PostgreSQL fixture database}"
evidence="${LENSO_STORE_EVIDENCE_DIR:-$root/target/store-contract-evidence}"
mkdir -p "$evidence"
# One thin Rust build serves both providers; no generated Host or per-case build.
cargo test --locked -p lenso-test --test durable_store --no-run --message-format=json \
  > "$evidence/build.jsonl"
test_binary="$(python3 - "$evidence/build.jsonl" <<'PY'
import json
import sys
artifacts = [entry["executable"] for line in open(sys.argv[1])
             if (entry := json.loads(line)).get("reason") == "compiler-artifact"
             and entry["target"]["name"] == "durable_store" and entry.get("executable")]
assert len(artifacts) == 1, artifacts
print(artifacts[0])
PY
)"
for provider in postgres d1; do
  LENSO_STORE_PROVIDER="$provider" LENSO_STORE_TRACE_OUT="$evidence/$provider.json" \
    timeout --kill-after=5s 90s "$test_binary" --ignored --exact real_provider_store_corpus --nocapture \
    | tee "$evidence/$provider.txt"
  # An accidental zero-test exact filter must never turn this into a green gate.
  rg -q '^test real_provider_store_corpus \.\.\. ok$' "$evidence/$provider.txt"
done
LENSO_STORE_TRANSCRIPT_DIR="$evidence" \
  timeout --kill-after=5s 90s "$test_binary" --exact replay_provider_store_corpus --nocapture \
  | tee "$evidence/replay.txt"
rg -q '^test replay_provider_store_corpus \.\.\. ok$' "$evidence/replay.txt"
