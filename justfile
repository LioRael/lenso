set dotenv-load := true

fmt:
    cargo fmt --all

check:
    cargo fmt --all -- --check
    cargo check --workspace --all-targets
    cargo test --workspace
    cargo run -p arch-check
    just generate-contracts
    just generate-ts-sdk
    pnpm --dir packages/ts-sdk typecheck

test:
    cargo test --workspace

api:
    cargo run -p app-api

worker:
    cargo run -p app-worker

migrate:
    cargo run -p app-migrate

db-up:
    docker compose -f infrastructure/local/docker-compose.yml up -d postgres

up: db-up

down:
    docker compose -f infrastructure/local/docker-compose.yml down

generate-contracts:
    cargo run -p generate-contracts

contracts: generate-contracts

generate-ts-sdk:
    cargo run -p generate-ts-sdk

generate-sdk: generate-ts-sdk

arch-check:
    cargo run -p arch-check

ci:
    sh scripts/ci.sh
