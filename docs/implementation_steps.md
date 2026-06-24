# Next Implementation: Short-term Tasks

This document lays out a step-by-step plan to complete the short-term work recommended earlier:

- Implement `services/peers.rs` to expose peer list/aliases/status.
- Implement `p2p/relay.rs` to surface relay/reservation status and control.
- Add RPC endpoints (or an `mpsc` command channel) so a GUI can query/send commands.
- Wire Kademlia/AutoNAT events into the swarm event loop.

Assumptions
- The libp2p swarm is implemented in `src/p2p/swarm.rs` and runs on Tokio.
- UI TUI code uses `tokio::sync::mpsc` channels (see `src/ui/mod.rs`).
- We want a minimal, well-typed, easily-testable control API before building a full Tauri GUI.

Goals
- Provide a small, documented control surface for a GUI or CLI to query node state and trigger actions.
- Implement missing business logic modules with clear boundaries and tests.
- Wire missing libp2p events into the main event loop so UI and services receive updates.

Prioritization
1. `services/peers.rs` — essential for UI and management.
2. `p2p/relay.rs` — relay status and control for NAT traversal.
3. Control API (`mpsc` command channel) — enables GUI integration.
4. Event wiring for Kademlia/AutoNAT — completes monitoring and behavior.

Step-by-step tasks

1) Implement `services/peers.rs`
- Purpose: central in-memory representation and persistence for known peers (aliases, addresses, connected state).
- Expose a small API:
  - `pub struct PeerStore` — holds `HashMap<PeerId, PeerInfo>` plus optional sqlite-backed persistence.
  - `impl PeerStore {`
    - `pub fn new(db_path: Option<&Path>) -> Result<Self>`
    - `pub fn upsert(&mut self, peer: PeerInfo)`
    - `pub fn set_connected(&mut self, peer_id: &PeerId, connected: bool)`
    - `pub fn list(&self) -> Vec<PeerInfo>`
    - `pub fn get(&self, peer_id: &PeerId) -> Option<&PeerInfo>`
  - `}`
- Wire points:
  - Called from `p2p/events.rs` or swarm event handler where `PeerConnected`/`PeerDisconnected` events are seen.
  - Persist alias updates to `src/storage/` (future step).
- Tests:
  - Unit tests for upsert/list/get and connected state.

2) Implement `p2p/relay.rs`
- Purpose: expose relay reservation status, allow requesting or releasing reservations, and surface relay addresses.
- API suggestions:
  - `pub enum RelayState { Unavailable, ClientReserved { expires_at: Instant }, ServerAvailable }`
  - `pub struct RelayManager { state: RelayState, reservations: Vec<ReservationInfo> }`
  - `impl RelayManager {`
    - `pub fn new() -> Self`
    - `pub fn handle_relay_event(&mut self, event: RelayEvent)` — called from swarm event loop
    - `pub async fn reserve(&mut self) -> Result<ReservationInfo>` — triggers reservation flow
    - `pub fn status(&self) -> RelayState`
  - `}`
- Wire points:
  - Call `RelayManager::handle_relay_event()` from the swarm event loop when relay-related events arrive.
  - Expose status via the control API (see below).
- Tests:
  - Unit test for state transitions based on mocked events.

3) Add control API (mpsc command channel)
- Rationale: fastest path to integrate UI and keep the node process single-binary; later you can add HTTP/JSON if needed for remote GUIs.
- Design (in `src/p2p/control.rs` or in `src/p2p/mod.rs`):
  - `enum ControlRequest { ListPeers, PeerInfo(PeerId), SendMessage(String), ReserveRelay, ReleaseRelay, GetStatus }`
  - `enum ControlResponse { Peers(Vec<PeerInfo>), Peer(PeerInfo), MessageAck, RelayReserved(ReservationInfo), Status(NodeStatus), Error(String) }`
  - Provide `pub fn spawn_control(rx: mpsc::Receiver<ControlRequest>, tx_resp: mpsc::Sender<ControlResponse>, shared_state: Arc<Mutex<State>>) -> JoinHandle<()>` where `shared_state` contains `PeerStore`, `RelayManager`, and `SwarmHandle` as needed.
- Integration:
  - The main app creates two channels: `control_tx` (to node) and `control_rx` (node receives). The UI/Tauri frontend uses `control_tx` to send requests and reads `control_resp_rx` for responses.
  - `src/ui/mod.rs` already accepts `events_rx` and `input_tx`; add an optional `control_response_rx` or reuse events channel for responses.
- Tests:
  - Spawn the control task in integration test and assert responses to `ListPeers` and `ReserveRelay`.

4) Wire Kademlia and AutoNAT events into the swarm event loop
- Identify where the swarm event loop handles events (likely `src/p2p/swarm.rs` or `src/p2p/events.rs`). Add match arms for the following events:
  - `libp2p::kad::KademliaEvent` variants (bootstrap finished, routing updates, query results) — translate into `UiEvent::SystemLog` or dedicated `UiEvent::KademliaUpdate`.
  - `libp2p::autonat::Event` variants — update `UiEvent::SystemLog` and maybe `PeerStore`/`RelayManager`.
- Implementation notes:
  - For each event, push a structured `UiEvent` into `events_tx` so UI can show status.
  - For Kademlia bootstrap finished, mark local node status and emit `UiEvent::SystemLog`.
- Tests:
  - Unit-level: simulate event objects and assert `events_tx` receives expected `UiEvent`.

Acceptance criteria
- `services/peers.rs` offers deterministic, tested API and is used from swarm event handling to keep peer list up-to-date.
- `p2p/relay.rs` exposes reservation status and accepts explicit reserve/release commands from the control API.
- A simple control API via `mpsc` exists and is exercised by a unit/integration test.
- Kademlia and AutoNAT events are surfaced to the UI via `UiEvent` variants.

Developer notes and example snippets

- Peer store example signature
```rust
pub struct PeerInfo {
    pub peer_id: libp2p::PeerId,
    pub alias: Option<String>,
    pub connected: bool,
    pub addresses: Vec<libp2p::Multiaddr>,
}

impl PeerStore {
    pub fn upsert(&mut self, peer: PeerInfo) { ... }
}
```

- Control API example
```rust
pub enum ControlRequest {
    ListPeers,
    SendMessage(String),
    ReserveRelay,
}

pub enum ControlResponse {
    Peers(Vec<PeerInfo>),
    Ok,
    Err(String),
}
```

- Where to wire events
  - `src/p2p/swarm.rs` -> modify `run_event_loop()` to call `peer_store.upsert()` and `relay_manager.handle_relay_event()` and to `events_tx.send(UiEvent::...)` on Kademlia/AutoNAT events.

Estimated effort (rough)
- `services/peers.rs`: 2–4 hours (impl + tests)
- `p2p/relay.rs`: 2–4 hours (impl + tests)
- Control API + integration: 2–3 hours (basic mpsc) or 5–8 hours (HTTP/JSON)
- Event wiring and tests: 2–4 hours

Next actions I can take for you
- Implement `services/peers.rs` with tests and open a PR in the workspace.
- Or scaffold the `src/p2p/control.rs` mpsc control task and unit tests first.

---

Created on: 2026-06-23
