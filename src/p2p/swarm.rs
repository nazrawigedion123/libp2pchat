use futures::stream::StreamExt;
use libp2p::{Multiaddr, PeerId, Swarm, gossipsub};
use std::{error::Error, path::PathBuf};
use tokio::{
    io::{self, AsyncBufReadExt},
    select,
    sync::mpsc::{Receiver, Sender},
};

use crate::{
    identity,
    protocol::{self, Message},
    services::peers::PeerService,
    storage::PeerStorage,
};

use super::{
    behaviour::MyBehaviour,
    control::{ControlRequest, ControlResponse, PeerInfo as ControlPeerInfo},
    discovery,
    events,
    relay::RelayManager,
    transport,
};

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

    /// Reserve an address on a remote relay by listening on a `/p2p-circuit` multiaddress.
    pub fn reserve_relay(&mut self, relay_addr: Multiaddr) -> Result<(), Box<dyn Error>> {
        self.swarm.listen_on(relay_addr)?;
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

    fn handle_control_request(
        &mut self,
        request: ControlRequest,
        db: &PeerStorage,
        relay_mgr: &mut RelayManager,
    ) -> ControlResponse {
        match request {
            ControlRequest::ListPeers => match db.load_all_peers() {
                Ok(all_peers) => {
                    let mut peers_map = std::collections::BTreeMap::new();
                    for (peer_id, address) in all_peers {
                        peers_map
                            .entry(peer_id)
                            .or_insert_with(Vec::new)
                            .push(address);
                    }
                    let peers = peers_map
                        .into_iter()
                        .map(|(peer_id, addresses)| ControlPeerInfo { peer_id, addresses })
                        .collect();
                    ControlResponse::Peers(peers)
                }
                Err(err) => ControlResponse::Error(err.to_string()),
            },
            ControlRequest::GetPeer(peer_id) => match db.load_all_peers() {
                Ok(all_peers) => {
                    let addresses = all_peers
                        .into_iter()
                        .filter_map(|(id, addr)| if id == peer_id { Some(addr) } else { None })
                        .collect();
                    ControlResponse::Peer(Some(ControlPeerInfo { peer_id, addresses }))
                }
                Err(err) => ControlResponse::Error(err.to_string()),
            },
            ControlRequest::SendMessage(message) => {
                self.send(message);
                ControlResponse::MessageAck
            }
            ControlRequest::SendDirect { peer_id, message } => {
                self.send_direct(peer_id, message);
                ControlResponse::MessageAck
            }
            ControlRequest::ReserveRelay(relay_addr) => match self.reserve_relay(relay_addr) {
                Ok(()) => ControlResponse::MessageAck,
                Err(err) => ControlResponse::Error(err.to_string()),
            },
            ControlRequest::GetRelayStatus => ControlResponse::RelayStatus(relay_mgr.status().clone()),
            ControlRequest::GetLocalPeerId => ControlResponse::LocalPeerId(self.local_peer_id().clone()),
        }
    }

    pub async fn run_event_loop(
        mut self,
        db: PeerStorage,
        mut control_rx: Receiver<ControlRequest>,
        control_resp_tx: Sender<ControlResponse>,
    ) -> Result<(), Box<dyn Error>> {
        let mut stdin = io::BufReader::new(io::stdin()).lines();
        let mut relay_mgr = RelayManager::new();
        let control_resp_tx = control_resp_tx;

        loop {
            select! {
                Ok(Some(line)) = stdin.next_line() => self.handle_input(line),
                event = self.swarm.select_next_some() => {
                    let mut peer_svc = PeerService::new(&mut self, &db);
                    events::handle_swarm_event(&mut peer_svc, &mut relay_mgr, event);
                }
                Some(request) = control_rx.recv() => {
                    let response = self.handle_control_request(request, &db, &mut relay_mgr);
                    let _ = control_resp_tx.send(response).await;
                }
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

    let (_control_tx, control_rx) = tokio::sync::mpsc::channel(32);
    let (control_resp_tx, mut control_resp_rx) = tokio::sync::mpsc::channel(32);

    tokio::spawn(async move {
        while let Some(response) = control_resp_rx.recv().await {
            println!("Control response: {response:?}");
        }
    });

    node.run_event_loop(db, control_rx, control_resp_tx).await
}
