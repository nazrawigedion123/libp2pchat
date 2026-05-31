mod db;
mod identity;

use db::PeerStorage;
use directories::ProjectDirs;
use futures::stream::StreamExt;
use libp2p::{
    autonat, dcutr, gossipsub, identify, kad, mdns, noise, relay,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Swarm, Transport,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::thread;
use std::{env, ffi::CString};
use std::{error::Error, time::Duration};
use tokio::{io, io::AsyncBufReadExt, select};

#[link(name = "govpn", kind = "static")]
unsafe extern "C" {
    fn StartDirectVPNTunnel(local_port: i32, public_listen_port: i32, remote_addr: *const c_char);
}

// --- Application Protocol Schema Definition ---
#[derive(Serialize, Deserialize, Debug, Clone)]
enum Message {
    Chat(String),
    FileChunk {
        file_name: String,
        chunk_index: u64,
        data: Vec<u8>,
    },
    PeerInfo {
        alias: String,
        capabilities: Vec<String>,
    },
    ServiceDiscovery {
        service_type: String,
    },
    RPC {
        method: String,
        params: Vec<String>,
    },
}

#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    dcutr: dcutr::Behaviour,
    relay_server: relay::Behaviour,
    relay_client: relay::client::Behaviour,
    autonat: autonat::Behaviour,
}

struct AppConfig {
    node_name: String,
    local_proxy_port: i32,
    public_router_port: i32,
    remote_peer_internet_addr: String,
    bootstrap_mode: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = parse_args();

    // 1. Setup isolation directories under ~/.myapp/<node_name>
    let mut node_dir = ProjectDirs::from("", "", "myapp")
        .map(|p| p.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    node_dir.push(&config.node_name);
    fs::create_dir_all(&node_dir)?;

    // 2. Load or generate persistent identity
    let id_keys = identity::load_or_generate_identity(&node_dir)?;

    // 3. Initialize SQL storage
    let db = PeerStorage::init(&node_dir)?;

    start_go_vpn_tunnel(
        config.local_proxy_port,
        config.public_router_port,
        config.remote_peer_internet_addr,
    );

    tokio::time::sleep(Duration::from_secs(1)).await;

    // 4. Build swarm using the cryptographic keypair
    let mut swarm = build_swarm(&id_keys)?;
    let topic = subscribe_to_chat_topic(&mut swarm)?;

    // 5. Hydrate Kademlia routing table with peers saved inside peer.db
    let old_peers = db.load_all_peers()?;
    println!(
        "Loaded {} history peer connection(s) from SQL database.",
        old_peers.len()
    );
    for (peer_id, addr) in old_peers {
        // Skip malformed circuit proxies to avoid triggering MissingDstPeerId
        if addr.to_string().contains("/p2p-circuit") && !addr.to_string().contains("/p2p/") {
            continue;
        }
        swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
    }

    listen_on_local_transports(&mut swarm, config.local_proxy_port)?;

    println!("Local Peer ID: {}", swarm.local_peer_id());
    configure_bootstrap(&mut swarm, config.bootstrap_mode.as_deref())?;

    run_chat_event_loop(swarm, topic, db).await
}

fn parse_args() -> AppConfig {
    let args: Vec<String> = env::args().collect();
    if args.len() < 5 {
        print_usage();
        std::process::exit(1);
    }

    AppConfig {
        node_name: args[1].clone(),
        local_proxy_port: args[2].parse().unwrap(),
        public_router_port: args[3].parse().unwrap(),
        remote_peer_internet_addr: args[4].clone(),
        bootstrap_mode: args.get(5).cloned(),
    }
}

fn print_usage() {
    eprintln!("Usage:");
    eprintln!(
        "  As Bootstrap Node A:\n    cargo run -- <node_name> <local_proxy_port> <public_listen_port> <remote_target_ip:port> bootstrap"
    );
    eprintln!(
        "  As Peer Node B:\n    cargo run -- <node_name> <local_proxy_port> <public_listen_port> <remote_target_ip:port> /ip4/.../p2p/<BOOTSTRAP_PEER_ID>"
    );
}

fn start_go_vpn_tunnel(
    local_proxy_port: i32,
    public_router_port: i32,
    remote_peer_internet_addr: String,
) {
    thread::spawn(move || {
        let c_remote_addr =
            CString::new(remote_peer_internet_addr).expect("Invalid CString conversion");
        unsafe {
            StartDirectVPNTunnel(local_proxy_port, public_router_port, c_remote_addr.as_ptr());
        }
    });
}

fn build_swarm(key: &libp2p::identity::Keypair) -> Result<Swarm<MyBehaviour>, Box<dyn Error>> {
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

fn build_behaviour(
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
        mdns,
        identify,
        kademlia,
        dcutr,
        relay_server,
        relay_client,
        autonat,
    })
}

fn subscribe_to_chat_topic(
    swarm: &mut Swarm<MyBehaviour>,
) -> Result<gossipsub::IdentTopic, Box<dyn Error>> {
    let topic = gossipsub::IdentTopic::new("test-net");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
    Ok(topic)
}

fn listen_on_local_transports(
    swarm: &mut Swarm<MyBehaviour>,
    local_proxy_port: i32,
) -> Result<(), Box<dyn Error>> {
    let tcp_listen_multiaddr: libp2p::Multiaddr =
        format!("/ip4/127.0.0.1/tcp/{}", local_proxy_port).parse()?;
    let quic_listen_multiaddr: libp2p::Multiaddr =
        format!("/ip4/127.0.0.1/udp/{}/quic-v1", local_proxy_port).parse()?;

    swarm.listen_on(tcp_listen_multiaddr)?;
    swarm.listen_on(quic_listen_multiaddr)?;
    Ok(())
}

fn configure_bootstrap(
    swarm: &mut Swarm<MyBehaviour>,
    bootstrap_mode: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    if let Some(addr_str) = bootstrap_mode {
        if addr_str != "bootstrap" {
            let bootstrap_addr: libp2p::Multiaddr = addr_str.parse()?;
            if let Some(libp2p::multiaddr::Protocol::P2p(peer_id)) = bootstrap_addr.iter().last() {
                swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer_id, bootstrap_addr.clone());
                swarm.behaviour_mut().kademlia.bootstrap()?;
            } else {
                eprintln!("Error: Provided bootstrap multiaddress must contain the trailing /p2p/<PeerId>");
                std::process::exit(1);
            }
        }
    }
    Ok(())
}

async fn run_chat_event_loop(
    mut swarm: Swarm<MyBehaviour>,
    topic: gossipsub::IdentTopic,
    db: PeerStorage,
) -> Result<(), Box<dyn Error>> {
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                // Parse commands to demonstrate all protocol message variants, defaulting to Chat
                let app_msg = if line.starts_with("/rpc ") {
                    Message::RPC {
                        method: line.trim_start_matches("/rpc ").to_string(),
                        params: vec!["param1".to_string()],
                    }
                } else if line.starts_with("/file ") {
                    Message::FileChunk {
                        file_name: line.trim_start_matches("/file ").to_string(),
                        chunk_index: 0,
                        data: b"raw binary chunk payload mock".to_vec(),
                    }
                } else if line == "/discovery" {
                    Message::ServiceDiscovery { service_type: "vpn-node".to_string() }
                } else if line == "/info" {
                    Message::PeerInfo {
                        alias: "RustNode".to_string(),
                        capabilities: vec!["Gossip".to_string(), "Relay".to_string()],
                    }
                } else {
                    Message::Chat(line)
                };

                // Serialize using bincode to convert our Message enum to a byte stream
                match bincode::serialize(&app_msg) {
                    Ok(encoded_bytes) => {
                        if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic.clone(), encoded_bytes) {
                            eprintln!("Publish error: {e:?}");
                        }
                    }
                    Err(e) => eprintln!("Serialization error: {e}"),
                }
            }
            event = swarm.select_next_some() => handle_swarm_event(&mut swarm, event, &db)
        }
    }
}

fn handle_swarm_event(
    swarm: &mut Swarm<MyBehaviour>,
    event: SwarmEvent<MyBehaviourEvent>,
    db: &PeerStorage,
) {
    match event {
        SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
            propagation_source: peer_id,
            message,
            ..
        })) => {
            // Deserialize bytes back into our structured Message enum
            match bincode::deserialize::<Message>(&message.data) {
                Ok(app_message) => match app_message {
                    Message::Chat(text) => {
                        println!(" [{peer_id}] (Chat): {text}");
                    }
                    Message::FileChunk { file_name, chunk_index, data } => {
                        println!(" [{peer_id}] (File) Recv chunk {chunk_index} for '{file_name}' ({} bytes)", data.len());
                    }
                    Message::PeerInfo { alias, capabilities } => {
                        println!(" [{peer_id}] (Identity Info) Node name: {alias}, Specs: {capabilities:?}");
                    }
                    Message::ServiceDiscovery { service_type } => {
                        println!(" [{peer_id}] (Discovery Scan) Requesting matches for: {service_type}");
                    }
                    Message::RPC { method, params } => {
                        println!(" [{peer_id}] (RPC Invocation) Call: {method} with params: {params:?}");
                    }
                },
                Err(_) => {
                    println!(" [{peer_id}] Received untyped binary text chunk: {}", String::from_utf8_lossy(&message.data));
                }
            }
        }

        SwarmEvent::Behaviour(MyBehaviourEvent::Identify(identify::Event::Received {
            peer_id,
            info,
            ..
        })) => {
            for addr in info.listen_addrs {
                swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer_id, addr.clone());
                if let Err(e) = db.save_peer(&peer_id, &addr) {
                    eprintln!("Database write failure: {e}");
                }
            }
        }

        SwarmEvent::Behaviour(MyBehaviourEvent::RelayServer(
            relay::Event::ReservationReqAccepted { src_peer_id, .. },
        )) => {
            println!("Relay server: Accepted reservation request from peer: {src_peer_id}");
        }
        SwarmEvent::Behaviour(MyBehaviourEvent::RelayClient(
            relay::client::Event::ReservationReqAccepted { relay_peer_id, .. },
        )) => {
            println!("Relay client: Successfully registered reservation through proxy relay: {relay_peer_id}");
        }
        _ => {}
    }
}