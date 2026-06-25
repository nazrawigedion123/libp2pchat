use libp2p::{Multiaddr, PeerId};

use crate::protocol::Message;
use super::relay::RelayState;

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub addresses: Vec<Multiaddr>,
}

#[derive(Debug)]
pub enum ControlRequest {
    ListPeers,
    GetPeer(PeerId),
    SendMessage(Message),
    SendDirect { peer_id: PeerId, message: Message },
    ReserveRelay(Multiaddr),
    GetRelayStatus,
    GetLocalPeerId,
}

#[derive(Debug)]
pub enum ControlResponse {
    Peers(Vec<PeerInfo>),
    Peer(Option<PeerInfo>),
    MessageAck,
    RelayStatus(RelayState),
    LocalPeerId(PeerId),
    Error(String),
}
