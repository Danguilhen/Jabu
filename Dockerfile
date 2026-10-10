# Multi-stage Dockerfile for Jabu

# Build stage
FROM rust:1.75-bookworm AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    libwebkit2gtk-4.1-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    libssl-dev \
    pkg-config \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install dioxus-cli
RUN cargo install dioxus-cli

# Set working directory
WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# Build dependencies (cached layer)
RUN mkdir -p crates/app/src && touch crates/app/src/main.rs && cargo build --release --workspace 2>/dev/null || true

# Copy source code
COPY . .

# Build application
RUN cargo build --release --workspace

# Runtime stage
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libwebkit2gtk-4.1-0 \
    libayatana-appindicator3-1 \
    librsvg2-common \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 -s /bin/bash jabu

# Copy built binaries
COPY --from=builder /app/target/release/jabu /usr/local/bin/jabu

# Set up data directory
RUN mkdir -p /home/jabu/.local/share/jabu && chown -R jabu:jabu /home/jabu

# Switch to non-root user
USER jabu
WORKDIR /home/jabu

# Default command
ENTRYPOINT ["jabu"]
CMD ["run"]