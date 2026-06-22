# Roadmap

## Planned Steps

| # | Step | Status | Details |
|---|------|--------|---------|
| 1 | Go tunnel + libp2p TCP | ✅ Done | TCP transport with Noise encryption + Yamux multiplexing; Go VPN data plane via C FFI bridge |
| 2 | libp2p + Kademlia | ✅ Done | Kademlia DHT with `MemoryStore` for peer routing, integrated into `MyBehaviour` |
| 3 | Bootstrap nodes | ✅ Done | CLI `bootstrap` mode; kubo bootstrap peers as fallback; bootstrap address parsing from multiaddr |
| 4 | QUIC transport | ✅ Done | UDP-based QUIC v1 transport via `.with_quic()`; listens on `/udp/<port>/quic-v1` |
| 5 | Relay + DCUtR | ✅ Done | Circuit relay v2 (server + client); DCUtR hole-punching with success/failure logging |
| 6 | True decentralized NAT traversal | 🟡 Partial | Relay + DCUtR + AutoNAT work, but **UPnP** and **WebRTC** are missing |

## Status Legend

- ✅ Done — fully implemented and operational
- 🟡 Partial — basic implementation exists but missing pieces
- ⬜ Pending — not started

## Detailed Breakdown

### Transport

| Feature | Status | Notes |
|---------|--------|-------|
| TCP + Noise + Yamux | ✅ Done | |
| QUIC (v1) | ✅ Done | |
| Relay client transport | ✅ Done | Wired via `.with_other_transport()` |
| WebRTC | ⬜ Pending | Not implemented |
| UPnP / NAT-PMP | ⬜ Pending | `libp2p-upnp` in lock file but not wired |

### Protocol Handlers

| Handler | Status | Notes |
|---------|--------|-------|
| Gossipsub | ✅ Done | Pub/sub chat over `test-net` topic, signed messages, strict validation |
| Direct messaging (request-response) | ✅ Done | `/direct-app-proto/1.0.0` with CBOR codec |
| mDNS | ✅ Done | Local network peer discovery |
| Identify | ✅ Done | Protocol negotiation + capability exchange |
| Kademlia | ✅ Done | `MemoryStore` — peers lost on restart |
| DCUtR | ✅ Done | Hole-punching result logged |
| Relay server | ✅ Done | Accepts reservations |
| Relay client | ✅ Done | Registers reservations through relay |
| AutoNAT | ✅ Done | NAT status detection (default config) |

### Events Handling

| Event source | Status | Notes |
|--------------|--------|-------|
| Gossipsub (message, subscribe, unsubscribe) | ✅ Done | |
| Identify (received) | ✅ Done | |
| mDNS (discovered, expired) | ✅ Done | |
| DCUtR (result) | ✅ Done | |
| Relay (reservation accepted) | ✅ Done | |
| Connection (open, close, errors) | ✅ Done | |
| New listen address | ✅ Done | |
| Kademlia (routing updated, bootstrap finished) | ⬜ Pending | Not matched in event loop |
| AutoNAT (status) | ⬜ Pending | Not matched in event loop |

### Business Logic (Placeholders)

| Module | File | Status | Notes |
|--------|------|--------|-------|
| Relay orchestration | `src/p2p/relay.rs` | ⬜ Pending | Empty file |
| Peer management | `src/services/peers.rs` | ⬜ Pending | Empty file |
| File transfer | `src/services/files.rs` | ⬜ Pending | `FileChunk` message uses mock data |
| RPC handling | `src/services/rpc.rs` | ⬜ Pending | Empty file |
| Service discovery | `src/services/discovery.rs` | ⬜ Pending | Empty file |
| Settings storage | `src/storage/settings.rs` | ⬜ Pending | Empty file |
| VPN route management | `src/vpn/routes.rs` | ⬜ Pending | Empty file |

### Persistence & Storage

| Feature | Status | Notes |
|---------|--------|-------|
| Identity (Ed25519 keypair) | ✅ Done | Saved to `<node_dir>/identity.key` |
| SQLite peer routing table | ✅ Done | CRUD on `routing_table` |
| Persistent Kademlia store | ⬜ Pending | Currently `MemoryStore` |
| User settings | ⬜ Pending | Placeholder only |

### Deployment & Operations

| Feature | Status | Notes |
|---------|--------|-------|
| Dockerfile | ✅ Done | Multi-stage build |
| Fly.io config + CI/CD | ✅ Done | `fly.toml` + GitHub Actions |
| Prometheus / metrics | ⬜ Pending | Listed in README future enhancements |
| Config file support | ⬜ Pending | Listed in README future enhancements |
| Auto-reconnection / circuit relay | ⬜ Pending | Listed in README future enhancements |
