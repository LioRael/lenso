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
    pnpm --dir {{ts_sdk_dir}} install
    pnpm --dir {{runtime_console_dir}} install

install-ci:
    CI=true pnpm --dir {{ts_sdk_dir}} install --frozen-lockfile
    CI=true pnpm --dir {{runtime_console_dir}} install --frozen-lockfile

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
    pnpm --dir {{runtime_console_dir}} dev

console-api:
    VITE_RUNTIME_CONSOLE_MODE=api VITE_API_BASE_URL=http://localhost:3000 pnpm --dir {{runtime_console_dir}} dev

console-preview:
    pnpm --dir {{runtime_console_dir}} preview

console-fmt:
    pnpm --dir {{runtime_console_dir}} format

console-fmt-check:
    pnpm --dir {{runtime_console_dir}} format:check

console-lint:
    pnpm --dir {{runtime_console_dir}} lint

console-typecheck:
    pnpm --dir {{runtime_console_dir}} typecheck

console-build:
    pnpm --dir {{runtime_console_dir}} build

console-check:
    pnpm --dir {{runtime_console_dir}} check

# Local infrastructure
db-up:
    docker compose -f {{compose_file}} up -d postgres

observability-up:
    docker compose -f {{compose_file}} --profile observability up -d

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

generated-check: generate
    git diff --exit-code -- contracts packages/ts-sdk/src/generated

sdk-typecheck:
    pnpm --dir {{ts_sdk_dir}} typecheck

sdk-build:
    pnpm --dir {{ts_sdk_dir}} build

sdk-check: sdk-typecheck

arch-check:
    cargo run --locked -p arch-check

ci:
    sh scripts/ci.sh
