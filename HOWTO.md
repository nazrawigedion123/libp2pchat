# How to Start a libp2p Chat Network

Commands to start a bootstrap node and two peer nodes.

## Terminal 1 — Bootstrap Node

```bash
RUST_LOG=info cargo run -- node-a 8500 9500 127.0.0.1:9501 bootstrap
```

After it starts, copy the peer ID from this line:

```
[Rust] Local Peer ID: <BOOTSTRAP_PEER_ID>
```

## Terminal 2 — Peer Node 1

Replace `<BOOTSTRAP_PEER_ID>` with the peer ID printed above:

```bash
RUST_LOG=info cargo run -- node-b 8501 9501 127.0.0.1:9500 /ip4/127.0.0.1/tcp/8500/p2p/<BOOTSTRAP_PEER_ID>
```

## Terminal 3 — Peer Node 2

```bash
RUST_LOG=info cargo run -- node-c 8502 9502 127.0.0.1:9500 /ip4/127.0.0.1/tcp/8500/p2p/<BOOTSTRAP_PEER_ID>
```

## Notes

- All nodes listen on TCP port `local_proxy_port` (args `8500`/`8501`/`8502`) and UDP port `public_router_port` (args `9500`/`9501`/`9502`) via QUIC.
- The bootstrap node uses `127.0.0.1:9501` as the remote VPN target; peer nodes point back to `127.0.0.1:9500` (the bootstrap's public router port).
- For machines on different networks, replace `127.0.0.1` with the bootstrap node's reachable IP and ensure the ports are open.
- You can also bootstrap over QUIC:
  ```
  /ip4/127.0.0.1/udp/8500/quic-v1/p2p/<BOOTSTRAP_PEER_ID>
  ```
