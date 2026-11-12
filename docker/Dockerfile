# TigerSmartChain node image.
#
# The node is the Rust binary in this repository, so the image is built from
# the Rust sources. A multi-stage build keeps the toolchain out of the runtime
# layer, and the runtime image carries only the binary, its CA bundle and curl
# for the health check.

FROM rust:1.82-bookworm AS builder

WORKDIR /src

# The crates are standalone (no workspace), so each one the node depends on is
# copied in. Manifests are copied before sources so Docker caches the
# dependency build separately from the source build.
COPY crypto/Cargo.toml crypto/Cargo.toml
COPY state/Cargo.toml state/Cargo.toml
COPY evm/Cargo.toml evm/Cargo.toml
COPY consensus/Cargo.toml consensus/Cargo.toml
COPY network/Cargo.toml network/Cargo.toml
COPY blockchain/Cargo.toml blockchain/Cargo.toml
COPY node/Cargo.toml node/Cargo.toml

COPY crypto/ crypto/
COPY state/ state/
COPY evm/ evm/
COPY consensus/ consensus/
COPY network/ network/
COPY blockchain/ blockchain/
COPY node/ node/

RUN cargo build --release --manifest-path node/Cargo.toml \
    && strip node/target/release/tigersmartchaind


FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 1000 tigersmartchain

COPY --from=builder /src/node/target/release/tigersmartchaind /usr/local/bin/tigersmartchaind

WORKDIR /home/tigersmartchain
USER tigersmartchain

# P2P, JSON-RPC, WebSocket JSON-RPC, metrics.
EXPOSE 30303 8545 8546 9090

# The node serves /health on its RPC port and reports unhealthy when it is not
# running or has no chain, so this check fails on a node that cannot serve.
HEALTHCHECK --interval=30s --timeout=10s --start-period=20s --retries=3 \
  CMD curl -fsS http://localhost:8545/health || exit 1

ENTRYPOINT ["tigersmartchaind"]
CMD ["--rpc-addr", "0.0.0.0:8545", "--ws-addr", "0.0.0.0:8546", "--listen-addr", "0.0.0.0:30303"]
