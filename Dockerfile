# --- Build Stage ---
FROM rust:1.75-bookworm AS builder
WORKDIR /usr/src/app

# Install standard C tools if you need to compile your Go library to a static archive (.a) first
RUN apt-get update && apt-get install -y build-essential clang

COPY . .

# Ensure your static library "libgovpn.a" is placed where the linker can see it
# (Adjust this path to match wherever your project looks for libgovpn.a)
ENV RUSTFLAGS="-L native=/usr/src/app"

RUN cargo build --release

# --- Runtime Stage ---
FROM debian:bookworm-slim
WORKDIR /app

# Copy the compiled binary over
COPY --from=builder /usr/src/app/target/release/libp2p-chat /app/libp2p-chat

# Create our app storage directory match
RUN mkdir -p /data

# Configure the node execution entrypoint to pass args: <node_name> <local_proxy_port> <public_listen_port> <remote_target_ip:port> bootstrap
ENTRYPOINT ["/app/libp2p-chat", "bootstrap-node", "8500", "9000", "0.0.0.0:9000", "bootstrap"]