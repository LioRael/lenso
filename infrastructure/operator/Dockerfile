FROM rust:1.94-bookworm AS builder

WORKDIR /workspace
COPY . .
RUN cargo build --locked --release -p lenso-operator

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/lenso-operator /usr/local/bin/lenso-operator

USER 65532:65532
ENTRYPOINT ["/usr/local/bin/lenso-operator"]
