use libp2p::{Swarm, Transport, noise, relay, tcp, yamux};
use std::error::Error;

use super::behaviour::{MyBehaviour, build_behaviour};

pub fn build_swarm(key: &libp2p::identity::Keypair) -> Result<Swarm<MyBehaviour>, Box<dyn Error>> {
    let local_peer_id = key.public().to_peer_id();
    let (relay_transport, relay_behavior) = relay::client::new(local_peer_id);

    let swarm = libp2p::SwarmBuilder::with_existing_identity(key.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_other_transport(|key| {
            let noise_config = noise::Config::new(key)
                .expect("Signing libp2p noise keypair failed; this should never happen");

            relay_transport
                .upgrade(libp2p::core::upgrade::Version::V1)
                .authenticate(noise_config)
                .multiplex(yamux::Config::default())
        })?
        .with_behaviour(|_k| build_behaviour(key, relay_behavior))?
        .build();

    Ok(swarm)
}
