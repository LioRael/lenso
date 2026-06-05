set dotenv-load := true

api_pkg := "app-api"
worker_pkg := "app-worker"
migrate_pkg := "app-migrate"
ts_sdk_dir := "packages/ts-sdk"
runtime_console_dir := "apps/runtime-console"
compose_file := "infrastructure/local/docker-compose.yml"

default:
    @just --list

# Dependencies
install:
    pnpm --dir={{ts_sdk_dir}} install
    pnpm --dir={{runtime_console_dir}} install

install-ci:
    CI=true pnpm --dir={{ts_sdk_dir}} install --frozen-lockfile
    CI=true pnpm --dir={{runtime_console_dir}} install --frozen-lockfile

# Quality gates
fmt: rust-fmt console-fmt

fmt-check: rust-fmt-check console-fmt-check

check:
    just fmt-check
    just rust-check
    just test
    just generated-check
    just arch-check
    just sdk-check
    just console-check

test:
    cargo test --locked --workspace

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

console:
    pnpm --dir={{runtime_console_dir}} run dev

console-api:
    VITE_RUNTIME_CONSOLE_MODE=api VITE_API_BASE_URL=http://localhost:3000 pnpm --dir={{runtime_console_dir}} run dev

console-api-smoke:
    sh scripts/runtime-console-api-smoke.sh

embedded-admin-demo:
    sh scripts/embedded-admin-demo.sh

console-preview:
    pnpm --dir={{runtime_console_dir}} run preview

console-fmt:
    pnpm --dir={{runtime_console_dir}} run format

console-fmt-check:
    pnpm --dir={{runtime_console_dir}} run format:check

console-lint:
    pnpm --dir={{runtime_console_dir}} run lint

console-typecheck:
    pnpm --dir={{runtime_console_dir}} run typecheck

console-build:
    pnpm --dir={{runtime_console_dir}} run build

console-test:
    pnpm --dir={{runtime_console_dir}} run test

console-check:
    pnpm --dir={{runtime_console_dir}} run check

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

# Contracts and generated clients
generate: generate-contracts generate-ts-sdk

generate-contracts:
    cargo run --locked -p generate-contracts

contracts: generate-contracts

generate-ts-sdk:
    cargo run --locked -p generate-ts-sdk

generate-sdk: generate-ts-sdk

generated-check:
    @set -eu; \
    tmp=$(mktemp -d); \
    trap 'rm -rf "$tmp"' EXIT; \
    mkdir -p "$tmp/contracts" "$tmp/ts-generated"; \
    cp -R contracts/. "$tmp/contracts/"; \
    cp -R packages/ts-sdk/src/generated/. "$tmp/ts-generated/"; \
    just generate; \
    diff -ru "$tmp/contracts" contracts; \
    diff -ru "$tmp/ts-generated" packages/ts-sdk/src/generated

sdk-typecheck:
    pnpm --dir={{ts_sdk_dir}} run typecheck

sdk-build:
    pnpm --dir={{ts_sdk_dir}} run build

sdk-test:
    pnpm --dir={{ts_sdk_dir}} run test

sdk-check:
    pnpm --dir={{ts_sdk_dir}} run check

arch-check:
    cargo run --locked -p arch-check

ci:
    sh scripts/ci.sh
