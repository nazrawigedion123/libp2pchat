use libp2p::{Multiaddr, PeerId, request_response::ResponseChannel};

use crate::{
    p2p::swarm::P2PNode,
    protocol::Message,
    storage::PeerStorage,
};

/// Service responsible for peer lifecycle management:
/// discovery, connection, disconnection, and persistence.
///
/// Encapsulates all P2P swarm and database interactions so
/// event handlers remain thin and focused on routing.
pub struct PeerService<'a> {
    node: &'a mut P2PNode,
    db: &'a PeerStorage,
}

impl<'a> PeerService<'a> {
    /// Creates a new peer service with the required dependencies.
    pub fn new(node: &'a mut P2PNode, db: &'a PeerStorage) -> Self {
        Self { node, db }
    }

    /// Sends an ACK response back through a direct-message channel.
    pub fn send_direct_response(
        &mut self,
        channel: ResponseChannel<Message>,
        response: Message,
    ) -> Result<(), ()> {
        self.node
            .swarm
            .behaviour_mut()
            .direct_messaging
            .send_response(channel, response)
            .map_err(|_| ())
    }

    /// Handles a newly discovered peer via Identify or mDNS.
    ///
    /// Steps:
    ///   1. Register the peer in the gossipsub explicit peer list
    ///      so we receive their published messages.
    ///   2. Register the peer's listen addresses in the Kademlia
    ///      DHT routing table for future lookups.
    ///   3. Persist each address to the SQLite database so the
    ///      peer can be reconnected after a restart.
    pub fn on_peer_discovered(&mut self, peer_id: PeerId, addresses: Vec<Multiaddr>) {
        // Step 1: Register as an explicit gossipsub peer
        self.node.add_explicit_peer(&peer_id);

        // Step 2: Register each address with Kademlia and persist to database
        for addr in &addresses {
            self.node.add_peer_address(peer_id, addr.clone());
            if let Err(e) = self.db.save_peer(&peer_id, addr) {
                eprintln!("Database write failure: {e}");
            }
        }
    }

    /// Handles an established direct connection (TCP/QUIC) to a peer.
    ///
    /// Steps:
    ///   1. Add the peer to the gossipsub explicit list so their
    ///      published messages are accepted.
    pub fn on_peer_connected(&mut self, peer_id: PeerId) {
        // Step 1: Register as an explicit gossipsub peer
        self.node.add_explicit_peer(&peer_id);
    }

    /// Handles a peer disconnection (mDNS expiry or connection closed).
    ///
    /// Steps:
    ///   1. Remove the peer from the gossipsub explicit list so we
    ///      no longer accept their messages.
    pub fn on_peer_disconnected(&mut self, peer_id: PeerId) {
        // Step 1: Remove from gossipsub explicit list
        self.node.remove_explicit_peer(&peer_id);
    }
}
