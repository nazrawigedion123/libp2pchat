# --- Build Stage ---
# Changed from 1.75 to 1.85+ to support Rust Edition 2024
FROM rust:1.85-bookworm AS builder
WORKDIR /usr/src/app

# Install standard build tools for C bindings
RUN apt-get update && apt-get install -y build-essential clang

COPY . .

# Ensure your static library "libgovpn.a" location can be found by the linker
# Adjust "native=" to point to the exact relative directory of your .a file if it's inside /vpn
ENV RUSTFLAGS="-L native=/usr/src/app/vpn"

RUN cargo build --release

# --- Runtime Stage ---
FROM debian:bookworm-slim
WORKDIR /app

# Install runtime dependencies if your Go VPN library uses dynamic libc hooks
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary over
COPY --from=builder /usr/src/app/target/release/libp2p-chat /app/libp2p-chat

# Create an application data directory matching our mount target
RUN mkdir -p /data

# Run the binary pointing directly to the persistent /data directory for SQLite/keys
ENTRYPOINT ["/app/libp2p-chat"]
CMD ["bootstrap-node", "8500", "9000", "0.0.0.0:9000", "bootstrap"]