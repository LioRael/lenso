set dotenv-load := true

api_pkg := "lenso-api"
worker_pkg := "lenso-worker"
migrate_pkg := "lenso-migrate"
compose_file := "infrastructure/local/docker-compose.yml"
cli_root := "../lenso-cli"

default:
    @just --list

# Quality gates
fmt: rust-fmt

fmt-check: rust-fmt-check

check:
    just fmt-check
    just rust-check
    just test
    just generated-check
    just arch-check
    just m6-skills-check
    just m6-docs-check

release-check:
    just check

release-plan:
    pnpm release:plan

release-plan-check:
    pnpm release:intent-check

release-version-check:
    sh scripts/verify-release-version.sh

package-readiness:
    sh scripts/package-readiness.sh

release-package:
    sh scripts/release-package.sh

first-user-smoke:
    sh scripts/first-user-smoke.sh

test:
    cargo test --locked --workspace
    cargo test --locked -p lenso --features host-transactions --test host_outbox_relay

rust-fmt:
    cargo fmt --all

rust-fmt-check:
    cargo fmt --all -- --check

rust-check:
    cargo check --locked --workspace --all-targets

# Apps
api:
    cargo run -p {{api_pkg}}

worker:
    cargo run -p {{worker_pkg}}

migrate:
    cargo run -p {{migrate_pkg}}

console-api-smoke:
    sh scripts/runtime-console-api-smoke.sh

console-api-fixture:
    sh scripts/runtime-console-api-fixture.sh

console-api-qa:
    sh scripts/runtime-console-api-qa.sh

console-build:
    sh scripts/build-runtime-console.sh

console-build-host host_root:
    test -f "{{host_root}}/Cargo.toml"
    LENSO_CONSOLE_DIST_DIR="{{host_root}}/.lenso/console/dist" LENSO_CONSOLE_EXTENSIONS_DIR="{{host_root}}/.lenso/console/extensions" just console-build

host-update-console host_root:
    test -f "{{host_root}}/Cargo.toml"
    cargo run --locked --manifest-path "{{cli_root}}/Cargo.toml" -- host update-console --repo-root "{{host_root}}"

host-serve host_root:
    test -f "{{host_root}}/Cargo.toml"
    cargo run --locked --manifest-path "{{cli_root}}/Cargo.toml" -- serve --repo-root "{{host_root}}"

# Local infrastructure
db-up:
    docker compose -f {{compose_file}} up -d postgres

observability-up:
    @set -eu; \
    compose="docker compose -f {{compose_file}} --profile observability"; \
    echo "Checking Docker daemon..."; \
    docker info >/dev/null; \
    echo "Validating local observability compose config..."; \
    ${compose} config >/dev/null; \
    echo "Starting Postgres and OpenTelemetry collector (OTLP gRPC :4317, HTTP :4318)..."; \
    ${compose} up --pull missing --wait --wait-timeout 45 postgres otel-collector; \
    ${compose} ps postgres otel-collector; \
    echo "Collector logs: docker compose -f {{compose_file}} --profile observability logs otel-collector"

otel-collector-up:
    @set -eu; \
    compose="docker compose -f {{compose_file}} --profile observability"; \
    echo "Checking Docker daemon..."; \
    docker info >/dev/null; \
    echo "Validating local observability compose config..."; \
    ${compose} config >/dev/null; \
    echo "Starting OpenTelemetry collector only (OTLP gRPC :4317, HTTP :4318)..."; \
    ${compose} up --pull missing --force-recreate --wait --wait-timeout 45 otel-collector; \
    ${compose} ps otel-collector

otel-smoke:
    just otel-collector-up
    @set -eu; \
    compose="docker compose -f {{compose_file}} --profile observability"; \
    endpoint="${OTEL_EXPORTER_OTLP_ENDPOINT:-http://localhost:4317}"; \
    echo "Emitting runtime telemetry smoke spans to $endpoint..."; \
    output=$(OTEL_EXPORTER_OTLP_ENDPOINT="$endpoint" cargo run --locked -p otel-smoke --quiet); \
    printf '%s\n' "$output"; \
    correlation_id=$(printf '%s\n' "$output" | sed -n 's/^OTEL_SMOKE_CORRELATION_ID=//p' | tail -1); \
    if [ -z "$correlation_id" ]; then \
        echo "otel-smoke did not print a correlation id" >&2; \
        exit 1; \
    fi; \
    echo "Waiting for collector debug output for $correlation_id..."; \
    found=0; \
    for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do \
        logs=$(${compose} logs --no-color otel-collector 2>/dev/null || true); \
        if printf '%s' "$logs" | grep -q "$correlation_id" \
            && printf '%s' "$logs" | grep -q "lenso.correlation_id" \
            && printf '%s' "$logs" | grep -q "lenso.story_id" \
            && printf '%s' "$logs" | grep -q "lenso.execution.kind" \
            && printf '%s' "$logs" | grep -q "lenso.outbox_event_id" \
            && printf '%s' "$logs" | grep -q "lenso.function_run_id"; then \
            found=1; \
            break; \
        fi; \
        sleep 1; \
    done; \
    if [ "$found" != "1" ]; then \
        echo "Collector did not expose the expected runtime telemetry attributes." >&2; \
        echo "Recent collector logs:" >&2; \
        ${compose} logs --no-color --tail 160 otel-collector >&2 || true; \
        exit 1; \
    fi; \
    echo "OTel smoke passed: collector received runtime correlation attributes for $correlation_id."

up: db-up

down:
    docker compose -f {{compose_file}} down

# Contracts
generate: generate-contracts

generate-contracts:
    cargo run --locked -p generate-contracts

contracts: generate-contracts

generated-check:
    @set -eu; \
    tmp=$(mktemp -d); \
    trap 'rm -rf "$tmp"' EXIT; \
    mkdir -p "$tmp/contracts"; \
    cp -R contracts/. "$tmp/contracts/"; \
    just generate; \
    diff -ru "$tmp/contracts" contracts

arch-check:
    cargo run --locked -p arch-check

m6-skills-check:
    python3 tools/check-m6-skills.py

m6-docs-check:
    python3 tools/check-m6-docs.py

ci:
    sh scripts/ci.sh
