# Build stage
FROM rust:1.91-bookworm AS builder

WORKDIR /build

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY rustfmt.toml ./

# Copy all workspace members
COPY server ./server
COPY api ./api
COPY impls ./impls
COPY auth-impls ./auth-impls

# Build the application in release mode
RUN cargo build --locked --release --bin vss-server

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies and create an unprivileged runtime user
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system vss \
    && useradd --system --gid vss --home-dir /app --shell /usr/sbin/nologin vss \
    && mkdir -p /app \
    && chown vss:vss /app

WORKDIR /app

# Copy the compiled binary from builder
COPY --from=builder --chown=vss:vss /build/target/release/vss-server /app/vss-server

# Copy default configuration file
COPY --chown=vss:vss server/vss-server-config.toml /app/vss-server-config.toml

USER vss:vss

ENV VSS_BIND_ADDRESS=0.0.0.0:8080

EXPOSE 8080

# Run the server with the config file
CMD ["/app/vss-server", "/app/vss-server-config.toml"]
