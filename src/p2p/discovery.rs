use libp2p::{Multiaddr, PeerId};

pub fn is_usable_saved_addr(addr: &Multiaddr) -> bool {
    let addr = addr.to_string();
    !(addr.contains("/p2p-circuit") && !addr.contains("/p2p/"))
}

pub fn bootstrap_peer_id(addr: &Multiaddr) -> Option<PeerId> {
    match addr.iter().last() {
        Some(libp2p::multiaddr::Protocol::P2p(peer_id)) => Some(peer_id),
        _ => None,
    }
}
