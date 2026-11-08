# Build from the repository root:
#   docker build -f deploy/Dockerfile -t jabu-host .
#
# The runtime image only contains the Host binary and CA certificates; the
# database lives on a volume mounted at /var/lib/jabu.

FROM rust:1.98.1-slim-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends git pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .
RUN cargo build --release -p host --bin jabu-host

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --home /var/lib/jabu --shell /usr/sbin/nologin jabu

COPY --from=builder /src/target/release/jabu-host /usr/local/bin/jabu-host

WORKDIR /var/lib/jabu
USER jabu
EXPOSE 8088

ENTRYPOINT ["jabu-host"]
CMD ["--config", "/etc/jabu/host.toml"]
