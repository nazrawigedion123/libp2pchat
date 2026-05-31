use libp2p::{
    autonat, dcutr, gossipsub, identify, kad, mdns, relay, request_response,
    swarm::NetworkBehaviour,
};
use std::{error::Error, time::Duration};
use tokio::io;

use crate::protocol::Message;

#[derive(NetworkBehaviour)]
pub struct MyBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub direct_messaging: request_response::cbor::Behaviour<Message, Message>,
    pub mdns: mdns::tokio::Behaviour,
    pub identify: identify::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub dcutr: dcutr::Behaviour,
    pub relay_server: relay::Behaviour,
    pub relay_client: relay::client::Behaviour,
    pub autonat: autonat::Behaviour,
}

pub fn build_behaviour(
    key: &libp2p::identity::Keypair,
    relay_client: relay::client::Behaviour,
) -> Result<MyBehaviour, Box<dyn Error + Send + Sync>> {
    let peer_id = key.public().to_peer_id();

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(1))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .build()
        .map_err(io::Error::other)?;

    let gossipsub = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(key.clone()),
        gossipsub_config,
    )?;

    let direct_messaging = request_response::cbor::Behaviour::new(
        [(
            libp2p::StreamProtocol::new("/direct-app-proto/1.0.0"),
            request_response::ProtocolSupport::Full,
        )],
        request_response::Config::default(),
    );
    let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;

    let identify = identify::Behaviour::new(identify::Config::new(
        "/chat-proto/1.0.0".to_string(),
        key.public(),
    ));

    let store = kad::store::MemoryStore::new(peer_id);
    let kademlia = kad::Behaviour::new(peer_id, store);
    let dcutr = dcutr::Behaviour::new(peer_id);
    let relay_server = relay::Behaviour::new(peer_id, relay::Config::default());
    let autonat = autonat::Behaviour::new(peer_id, autonat::Config::default());

    Ok(MyBehaviour {
        gossipsub,
        direct_messaging,
        mdns,
        identify,
        kademlia,
        dcutr,
        relay_server,
        relay_client,
        autonat,
    })
}
