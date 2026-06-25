---
name: libp2p-chat-rust-skill
description: P2P VPN tunnel application in Rust using libp2p control plane and Go data plane; handles distributed networking, NAT traversal, peer discovery, and FFI bridge to Go VPN tunnel
applyTo: ["**/*.rs", "Cargo.toml", "README.md", "HOWTO.md", "vpn/**/*"]
---

# libp2p-chat Rust Project Skill

**Domain**: P2P VPN Tunnel Application using libp2p and Go FFI

## Project Overview

This is a **decentralized P2P VPN tunnel application** written in Rust that combines:
- **libp2p** as the control plane (networking, peer discovery, hole-punching)
- **Go-based VPN tunnel** as the data plane (FFI bridge via C)

The application creates a mesh network where peers can communicate directly or relay through bootstrap nodes, with automatic NAT traversal capabilities.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│           Rust Application (libp2p)                 │
│                                                     │
│  ┌─────────────────────────────────────────────┐   │
│  │ Network Behaviour                           │   │
│  │ • Gossipsub (pub/sub chat)                  │   │
│  │ • Kademlia DHT (peer routing)               │   │
│  │ • mDNS (local discovery)                    │   │
│  │ • Identify (protocol negotiation)           │   │
│  │ • DCUtR (hole-punching)                     │   │
│  │ • Relay (circuit relay v2)                  │   │
│  │ • AutoNAT (NAT detection)                   │   │
│  └─────────────────────────────────────────────┘   │
│  • TCP transport (Noise + Yamux)                   │
│  • QUIC transport (UDP)                            │
│  • Request-response protocol (direct messaging)    │
│  • Persistent storage (SQLite, Ed25519 keypair)    │
└─────────────────────────────────────────────────────┘
         │
         │ FFI Bridge (C ABI)
         │
         ▼
┌─────────────────────────────────────────────────────┐
│         Go VPN Tunnel (libgovpn.a)                  │
│                                                     │
│  • StartDirectVPNTunnel(local_port, public_port,   │
│                          remote_addr)              │
│  • Runs in background thread                       │
└─────────────────────────────────────────────────────┘
```

## Project Structure

```
src/
├── main.rs                 # Entry point, CLI arg parsing, node data dir setup
├── identity/               # Keypair management
│   ├── keypair.rs         # Ed25519 keypair generation/persistence
│   ├── peer_id.rs         # PeerId derivation from keypair
│   └── mod.rs
├── p2p/                    # libp2p networking core
│   ├── behaviour.rs       # NetworkBehaviour definition (all protocols)
│   ├── discovery.rs       # Peer discovery orchestration
│   ├── events.rs          # Event handling (gossipsub, mDNS, identify, etc.)
│   ├── relay.rs           # Relay orchestration (placeholder)
│   ├── swarm.rs           # Main swarm loop and connection handling
│   ├── transport.rs       # TCP + QUIC transport configuration
│   └── mod.rs
├── protocol/               # Message formats and protocol definitions
│   ├── codec.rs           # CBOR serialization
│   ├── discovery.rs       # Discovery message types
│   ├── message.rs         # Chat, file chunk, RPC messages
│   ├── rpc.rs             # RPC request/response types
│   └── mod.rs
├── services/               # Business logic layer (mostly placeholders)
│   ├── chat.rs            # Chat message handling
│   ├── discovery.rs       # Service discovery logic
│   ├── files.rs           # File transfer (FileChunk handling)
│   ├── peers.rs           # Peer management
│   ├── rpc.rs             # RPC routing
│   └── mod.rs
├── storage/                # Data persistence
│   ├── mod.rs
│   ├── peers.rs           # Peer routing table CRUD
│   ├── settings.rs        # User settings (placeholder)
│   └── sqlite.rs          # SQLite connection management
└── vpn/                    # Go VPN tunnel integration
    ├── ffi.rs             # C FFI bindings (StartDirectVPNTunnel)
    ├── mod.rs
    ├── routes.rs          # VPN route management (placeholder)
    └── tunnel.rs          # Tunnel lifecycle management
```

## Key Concepts

### 1. **Bootstrap Node vs Peer Node**

**Bootstrap Node** (`bootstrap` flag):
- Entry point for new peers joining the network
- Listens on well-known ports (TCP + QUIC)
- Uses itself as remote VPN target initially
- Accepts relay reservations

**Peer Node** (multiaddr bootstrap address):
- Joins via bootstrap multiaddr: `/ip4/<IP>/tcp/<PORT>/p2p/<PEER_ID>`
- Connects to bootstrap node to discover other peers
- Attempts direct connections with hole-punching (DCUtR)
- Falls back to relay if direct connection fails

### 2. **Transport Layers**

| Protocol | Port | Details |
|----------|------|---------|
| TCP | `local_proxy_port` | Primary; Noise encryption + Yamux multiplexing |
| QUIC | `public_router_port` (UDP) | Alternative; lower latency, connection migration |
| Relay | Through bootstrap node | Used when direct connection impossible (NAT) |

### 3. **Key Protocols**

- **Gossipsub**: Topic-based pub/sub for broadcast chat messages (topic: `test-net`)
- **Kademlia**: DHT for peer routing (currently `MemoryStore`, lost on restart)
- **Identify**: Capability negotiation and protocol version exchange
- **mDNS**: Automatic local network peer discovery
- **DCUtR**: Hole-punching for direct connections after relay introduction
- **Request-Response**: Direct peer messaging via `/direct-app-proto/1.0.0`

### 4. **Persistence**

- **Ed25519 Keypair**: Stored at `<node_dir>/identity.key`, derived PeerId is stable
- **SQLite Peer Routing**: `<node_dir>/routing.db` stores peer addresses
- **Future**: Persistent Kademlia store (currently `MemoryStore`)

### 5. **FFI Integration with Go VPN**

```rust
// vpn/ffi.rs
#[link(name = "govpn", kind = "static")]
unsafe extern "C" {
    fn StartDirectVPNTunnel(local_port: i32, public_listen_port: i32, remote_addr: *const c_char);
}
```

- Spawns in background thread during app startup
- Uses C-compatible types (`c_char` not Rust `char`)
- Requires compiled `libgovpn.a` (from `vpn/` Go code)

## Build & Run

### Prerequisites

```bash
# Go VPN tunnel (builds libgovpn.a)
cd vpn && make && cd ..

# Rust dependencies
cargo build
```

### Run Three Nodes Locally

**Terminal 1 — Bootstrap Node A**:
```bash
RUST_LOG=info cargo run -- node-a 8500 9500 127.0.0.1:9501 bootstrap
# Copy the printed Peer ID
```

**Terminal 2 — Peer Node B**:
```bash
RUST_LOG=info cargo run -- node-b 8501 9501 127.0.0.1:9500 /ip4/127.0.0.1/tcp/8500/p2p/<BOOTSTRAP_PEER_ID>
```

**Terminal 3 — Peer Node C**:
```bash
RUST_LOG=info cargo run -- node-c 8502 9502 127.0.0.1:9500 /ip4/127.0.0.1/tcp/8500/p2p/<BOOTSTRAP_PEER_ID>
```

### Multi-Machine Setup

Replace `127.0.0.1` in bootstrap multiaddr with bootstrap node's public IP. Ensure TCP/UDP ports are open.

## Development Patterns

### 1. **Module Organization**

Each major subsystem (identity, p2p, protocol, services, storage, vpn) has:
- `mod.rs` — Public API and re-exports
- Specialized files for domain logic
- Clear responsibility boundaries

### 2. **Error Handling**

```rust
// Functions return Result<T, Box<dyn Error>>
fn some_func() -> Result<(), Box<dyn Error>> { ... }
```

### 3. **Event Loop Pattern** (in `p2p/swarm.rs`)

```rust
loop {
    select! {
        event = swarm.select_next_some() => {
            match event {
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(...)) => { ... }
                SwarmEvent::Behaviour(MyBehaviourEvent::Identify(...)) => { ... }
                // ... other events
                _ => {}
            }
        }
    }
}
```

### 4. **Message Passing**

- **Gossipsub**: Broadcast via `swarm.behaviour_mut().gossipsub.publish(...)`
- **Request-Response**: Direct peer queries via protocol handler
- **Storage**: SQLite CRUD through `storage::peers` module

### 5. **Configuration Structure**

```rust
pub struct NodeConfig {
    pub node_dir: PathBuf,          // <node_dir>/ for keypair + routing.db
    pub local_proxy_port: i32,      // TCP listening port
    pub bootstrap_mode: Option<String>, // "bootstrap" or multiaddr
}
```

## Dependencies

### Core libp2p Features

```toml
libp2p = { version = "0.54", features = [
    "gossipsub",    # Pub/sub
    "mdns",         # mDNS discovery
    "tcp",          # TCP transport
    "quic",         # QUIC transport
    "noise",        # Noise encryption
    "yamux",        # Stream multiplexing
    "dcutr",        # Hole-punching
    "identify",     # Protocol negotiation
    "kad",          # Kademlia DHT
    "autonat",      # NAT detection
    "relay",        # Circuit relay v2
    "request-response", # Direct messaging
    "cbor",         # CBOR serialization
    "macros",       # Derive macros
    "tokio",        # Async runtime
] }
```

### Other Key Dependencies

- **tokio**: Async runtime with `features = ["full"]`
- **futures**: Async utilities
- **rusqlite**: SQLite with bundled C library
- **serde + bincode**: Serialization
- **directories**: Platform-specific data dirs
- **libc**: C FFI types

## Common Tasks

### Adding a New Message Type

1. Define in `protocol/message.rs`
2. Implement serialization in `protocol/codec.rs`
3. Handle in `services/` module
4. Emit via Gossipsub or request-response protocol

### Adding a Peer Service

1. Create module in `services/` (e.g., `new_service.rs`)
2. Integrate event handling in `p2p/events.rs`
3. Persist state in `storage/` if needed

### Debugging Network Issues

```bash
# Enable verbose logging
RUST_LOG=debug cargo run -- ...

# Watch swarm events in p2p/swarm.rs event loop
# Check identified peers, routing table, relay connections
```

### Building Go VPN Component

```bash
cd vpn
make              # Builds libgovpn.a
cd ..
cargo build       # Embeds Go tunnel via FFI
```

## Implementation Status

| Area | Status | Notes |
|------|--------|-------|
| **Transport & Protocols** | ✅ Done | TCP/QUIC, all major protocols working |
| **Bootstrap & Discovery** | ✅ Done | mDNS, Kademlia, bootstrap nodes operational |
| **Hole-Punching** | ✅ Done | DCUtR + relay working for NAT traversal |
| **Persistence** | ✅ Partial | Keypair + routing table; Kademlia store pending |
| **Business Logic** | 🟡 Partial | Chat/RPC placeholders; needs implementation |
| **Advanced NAT** | ⬜ Pending | UPnP, WebRTC not yet implemented |
| **Monitoring** | ⬜ Pending | Prometheus metrics not yet added |

## Roadmap References

See `ROADMAP.md` for detailed feature matrix and implementation gaps.
See `README.md` for deployment info (Dockerfile, Fly.io CI/CD).

## Quick Reference Commands

```bash
# Build with Go VPN
(cd vpn && make) && cargo build --release

# Run bootstrap node
RUST_LOG=info cargo run -- bootstrap-node 8500 9500 127.0.0.1:9501 bootstrap

# Run peer node
RUST_LOG=info cargo run -- peer-node 8501 9501 127.0.0.1:9500 /ip4/127.0.0.1/tcp/8500/p2p/12D3KooXXXXXXX

# Test locally (three terminals)
# Terminal 1: cargo run -- node-a 8500 9500 127.0.0.1:9501 bootstrap
# Terminal 2: cargo run -- node-b 8501 9501 127.0.0.1:9500 /ip4/127.0.0.1/tcp/8500/p2p/12D3KooXXXXXXX
# Terminal 3: cargo run -- node-c 8502 9502 127.0.0.1:9500 /ip4/127.0.0.1/tcp/8500/p2p/12D3KooXXXXXXX

# Check compilation
cargo check

# Run tests (when added)
cargo test

# Clean build artifacts
cargo clean
```

## References

- **Project Docs**: README.md, HOWTO.md, ROADMAP.md, docs/implementation_steps.md
- **libp2p Docs**: https://docs.rs/libp2p/
- **libp2p Spec**: https://github.com/libp2p/specs
- **Go VPN Code**: vpn/ directory (makefile, vpn.go, libgovpn.h)
