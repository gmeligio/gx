FROM rust:slim-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release --quiet

FROM ghcr.io/charmbracelet/vhs:v0.10.0
RUN apt-get update --allow-releaseinfo-change \
    && apt-get install -y --no-install-recommends git \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/gx /usr/local/bin/gx
