use futures::stream::StreamExt;
use libp2p::{Multiaddr, PeerId, Swarm, gossipsub};
use std::{error::Error, path::PathBuf};
use tokio::{
    io::{self, AsyncBufReadExt},
    select,
};

use crate::{
    identity,
    protocol::{self, Message},
    storage::PeerStorage,
};

use super::{behaviour::MyBehaviour, discovery, events, transport};

pub struct NodeConfig {
    pub node_dir: PathBuf,
    pub local_proxy_port: i32,
    pub bootstrap_mode: Option<String>,
}

pub struct P2PNode {
    pub(crate) swarm: Swarm<MyBehaviour>,
    topic: gossipsub::IdentTopic,
}

impl P2PNode {
    pub fn new(key: &libp2p::identity::Keypair) -> Result<Self, Box<dyn Error>> {
        let mut swarm = transport::build_swarm(key)?;
        let topic = gossipsub::IdentTopic::new("test-net");
        swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

        Ok(Self { swarm, topic })
    }

    pub fn local_peer_id(&self) -> &PeerId {
        self.swarm.local_peer_id()
    }

    pub fn add_peer_address(&mut self, peer_id: PeerId, addr: Multiaddr) {
        self.swarm
            .behaviour_mut()
            .kademlia
            .add_address(&peer_id, addr);
    }

    pub fn add_explicit_peer(&mut self, peer_id: &PeerId) {
        self.swarm
            .behaviour_mut()
            .gossipsub
            .add_explicit_peer(peer_id);
    }

    pub fn remove_explicit_peer(&mut self, peer_id: &PeerId) {
        self.swarm
            .behaviour_mut()
            .gossipsub
            .remove_explicit_peer(peer_id);
    }

    pub fn listen(&mut self, local_proxy_port: i32) -> Result<(), Box<dyn Error>> {
        let tcp_listen_multiaddr: Multiaddr =
            format!("/ip4/0.0.0.0/tcp/{local_proxy_port}").parse()?;
        let quic_listen_multiaddr: Multiaddr =
            format!("/ip4/0.0.0.0/udp/{local_proxy_port}/quic-v1").parse()?;

        self.swarm.listen_on(tcp_listen_multiaddr)?;
        self.swarm.listen_on(quic_listen_multiaddr)?;
        Ok(())
    }

    pub fn connect(&mut self, bootstrap_mode: Option<&str>) -> Result<(), Box<dyn Error>> {
        if let Some(addr_str) = bootstrap_mode {
            if addr_str != "bootstrap" {
                let bootstrap_addr: Multiaddr = addr_str.parse()?;
                if let Some(peer_id) = discovery::bootstrap_peer_id(&bootstrap_addr) {
                    self.add_peer_address(peer_id, bootstrap_addr.clone());
                    self.add_explicit_peer(&peer_id);
                    self.swarm.dial(bootstrap_addr.clone())?;
                    self.swarm.behaviour_mut().kademlia.bootstrap()?;
                    println!("Dialing bootstrap peer {peer_id} at {bootstrap_addr}");
                } else {
                    eprintln!(
                        "Error: Provided bootstrap multiaddress must contain the trailing /p2p/<PeerId>"
                    );
                    std::process::exit(1);
                }
            }
        }
        Ok(())
    }

    pub fn send(&mut self, message: Message) {
        match protocol::codec::encode(&message) {
            Ok(encoded_bytes) => {
                if let Err(e) = self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(self.topic.clone(), encoded_bytes)
                {
                    eprintln!("Publish error: {e:?}");
                }
            }
            Err(e) => eprintln!("Serialization error: {e}"),
        }
    }

    pub fn send_direct(&mut self, target_peer_id: PeerId, message: Message) {
        self.swarm
            .behaviour_mut()
            .direct_messaging
            .send_request(&target_peer_id, message);
        println!("=> Direct message request sent to {target_peer_id}");
    }

    pub async fn run_event_loop(mut self, db: PeerStorage) -> Result<(), Box<dyn Error>> {
        let mut stdin = io::BufReader::new(io::stdin()).lines();

        loop {
            select! {
                Ok(Some(line)) = stdin.next_line() => self.handle_input(line),
                event = self.swarm.select_next_some() => events::handle_swarm_event(&mut self, event, &db)
            }
        }
    }

    fn handle_input(&mut self, line: String) {
        if line.starts_with("/direct ") {
            let parts: Vec<&str> = line.trim_start_matches("/direct ").splitn(2, ' ').collect();
            if parts.len() == 2 {
                match parts[0].parse::<PeerId>() {
                    Ok(target_peer_id) => {
                        self.send_direct(target_peer_id, Message::Chat(parts[1].to_string()));
                    }
                    Err(_) => eprintln!("System: Invalid target Peer ID format input string."),
                }
            } else {
                eprintln!("System Usage: /direct <PEER_ID> <MESSAGE>");
            }
            return;
        }

        let app_msg = if line.starts_with("/rpc ") {
            protocol::rpc::message(
                line.trim_start_matches("/rpc ").to_string(),
                vec!["param1".to_string()],
            )
        } else if line.starts_with("/file ") {
            Message::FileChunk {
                file_name: line.trim_start_matches("/file ").to_string(),
                chunk_index: 0,
                data: b"raw binary chunk payload mock".to_vec(),
            }
        } else if line == "/discovery" {
            protocol::discovery::service_query("vpn-node")
        } else if line == "/info" {
            Message::PeerInfo {
                alias: "RustNode".to_string(),
                capabilities: vec!["Gossip".to_string(), "Relay".to_string()],
            }
        } else {
            crate::services::chat::parse_input(line)
        };

        self.send(app_msg);
    }
}

pub async fn run_chat_node(config: NodeConfig) -> Result<(), Box<dyn Error>> {
    let id_keys = identity::load_or_generate_identity(&config.node_dir)?;
    let expected_peer_id = identity::derive_peer_id(&id_keys);
    let db = PeerStorage::init(&config.node_dir)?;

    let mut node = P2PNode::new(&id_keys)?;
    debug_assert_eq!(node.local_peer_id(), &expected_peer_id);

    let old_peers = db.load_all_peers()?;
    println!(
        "Loaded {} history peer connection(s) from SQL database.",
        old_peers.len()
    );
    for (peer_id, addr) in old_peers {
        if discovery::is_usable_saved_addr(&addr) {
            node.add_peer_address(peer_id, addr);
        }
    }

    node.listen(config.local_proxy_port)?;

    println!("Local Peer ID: {}", node.local_peer_id());
    node.connect(config.bootstrap_mode.as_deref())?;

    node.run_event_loop(db).await
}
