use libp2p::{PeerId, identity::Keypair};

pub fn derive_peer_id(keypair: &Keypair) -> PeerId {
    keypair.public().to_peer_id()
}
