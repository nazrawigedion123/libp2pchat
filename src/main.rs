mod db;
mod identity;

use db::PeerStorage;
use directories::ProjectDirs;
use futures::stream::StreamExt;
use libp2p::{
    Swarm,
    Transport,
    autonat,
    dcutr,
    gossipsub,
    identify,
    kad,
    mdns,
    noise,
    relay,
    request_response, // <-- Added for point-to-point communication
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp,
    yamux,
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
    // Add the direct point-to-point request-response protocol behavior
    direct_messaging: request_response::cbor::Behaviour<Message, Message>,
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

    let mut node_dir = if std::env::var("FLY_APP_NAME").is_ok() {
        PathBuf::from("/data")
    } else {
        ProjectDirs::from("", "", "myapp")
            .map(|p| p.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };
    node_dir.push(&config.node_name);
    fs::create_dir_all(&node_dir)?;

    let id_keys = identity::load_or_generate_identity(&node_dir)?;
    let db = PeerStorage::init(&node_dir)?;

    start_go_vpn_tunnel(
        config.local_proxy_port,
        config.public_router_port,
        config.remote_peer_internet_addr,
    );

    tokio::time::sleep(Duration::from_secs(1)).await;

    let mut swarm = build_swarm(&id_keys)?;
    let topic = subscribe_to_chat_topic(&mut swarm)?;

    let old_peers = db.load_all_peers()?;
    println!(
        "Loaded {} history peer connection(s) from SQL database.",
        old_peers.len()
    );
    for (peer_id, addr) in old_peers {
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

    // Configure Direct request/response channel settings
    // Configure Direct request/response channel settings using StreamProtocol
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
    // Change 127.0.0.1 to 0.0.0.0 so libp2p can capture outside network interfaces
    let tcp_listen_multiaddr: libp2p::Multiaddr =
        format!("/ip4/0.0.0.0/tcp/{}", local_proxy_port).parse()?;
    let quic_listen_multiaddr: libp2p::Multiaddr =
        format!("/ip4/0.0.0.0/udp/{}/quic-v1", local_proxy_port).parse()?;

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
                swarm
                    .behaviour_mut()
                    .gossipsub
                    .add_explicit_peer(&peer_id);
                swarm.dial(bootstrap_addr.clone())?;
                swarm.behaviour_mut().kademlia.bootstrap()?;
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

async fn run_chat_event_loop(
    mut swarm: Swarm<MyBehaviour>,
    topic: gossipsub::IdentTopic,
    db: PeerStorage,
) -> Result<(), Box<dyn Error>> {
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                // Command layout pattern lookup: /direct <TARGET_PEER_ID> <YOUR CHAT TEXT HERE>
                if line.starts_with("/direct ") {
                    let parts: Vec<&str> = line.trim_start_matches("/direct ").splitn(2, ' ').collect();
                    if parts.len() == 2 {
                        if let Ok(target_peer_id) = parts[0].parse::<libp2p::PeerId>() {
                            let direct_msg = Message::Chat(parts[1].to_string());

                            // Send direct query request over the tracking pipeline
                            swarm.behaviour_mut().direct_messaging.send_request(&target_peer_id, direct_msg);
                            println!("=> Direct message request sent to {}", target_peer_id);
                        } else {
                            eprintln!("System: Invalid target Peer ID format input string.");
                        }
                    } else {
                        eprintln!("System Usage: /direct <PEER_ID> <MESSAGE>");
                    }
                    continue;
                }

                // Default fallback fallback paths parse standard gossip mesh commands
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
        // --- Core Application Protocol (Gossipsub Broadcasts) ---
        SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
            propagation_source: peer_id,
            message,
            ..
        })) => match bincode::deserialize::<Message>(&message.data) {
            Ok(app_message) => {
                display_received_message("Gossip Mesh Network", peer_id, app_message)
            }
            Err(_) => println!(" [{peer_id}] Received untyped binary text chunk via Gossipsub"),
        },
        SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Subscribed {
            peer_id,
            topic,
        })) => {
            println!("Gossipsub: peer {peer_id} subscribed to {topic}");
        }
        SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Unsubscribed {
            peer_id,
            topic,
        })) => {
            println!("Gossipsub: peer {peer_id} unsubscribed from {topic}");
        }

        // --- Core Application Protocol (Direct Request-Response Interceptions) ---
        SwarmEvent::Behaviour(MyBehaviourEvent::DirectMessaging(
            request_response::Event::Message {
                peer: peer_id,
                message,
            },
        )) => {
            match message {
                // Occurs when another single peer dials and passes a direct request payload down to us
                request_response::Message::Request {
                    request, channel, ..
                } => {
                    display_received_message("Direct Point-To-Point", peer_id, request.clone());

                    // Send an explicit acknowledgement receipt back to complete the round-trip transaction handshake
                    let response_receipt =
                        Message::Chat("ACK: Message delivered directly.".to_string());
                    let _ = swarm
                        .behaviour_mut()
                        .direct_messaging
                        .send_response(channel, response_receipt);
                }
                // Occurs when we receive the response back from a request we originally sent out
                request_response::Message::Response { response, .. } => {
                    display_received_message("Direct Receipt Confirmation", peer_id, response);
                }
            }
        }

        // --- Identity & Routing Engine Layer Updates ---
        SwarmEvent::Behaviour(MyBehaviourEvent::Identify(identify::Event::Received {
            peer_id,
            info,
            ..
        })) => {
            swarm
                .behaviour_mut()
                .gossipsub
                .add_explicit_peer(&peer_id);
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
        SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
            for (peer_id, addr) in list {
                swarm
                    .behaviour_mut()
                    .gossipsub
                    .add_explicit_peer(&peer_id);
                swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer_id, addr.clone());
                if let Err(e) = db.save_peer(&peer_id, &addr) {
                    eprintln!("Database write failure: {e}");
                }
                println!("mDNS discovered peer {peer_id} at {addr}");
            }
        }
        SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
            for (peer_id, _addr) in list {
                swarm
                    .behaviour_mut()
                    .gossipsub
                    .remove_explicit_peer(&peer_id);
            }
        }

        // --- Automatic Direct P2P Holepunching (DCUTR Tracking) ---
        // --- Automatic Direct P2P Holepunching (DCUTR Tracking) ---
        SwarmEvent::Behaviour(MyBehaviourEvent::Dcutr(libp2p::dcutr::Event {
            remote_peer_id,
            result,
        })) => match result {
            Ok(_) => println!("==> Hole punch succeeded with peer: {remote_peer_id}!"),
            Err(error) => {
                eprintln!("==> Hole punch failed with peer {remote_peer_id}. Reason: {error:?}")
            }
        },

        SwarmEvent::ConnectionEstablished {
            peer_id, endpoint, ..
        } => {
            swarm
                .behaviour_mut()
                .gossipsub
                .add_explicit_peer(&peer_id);
            let direct_type = if endpoint.is_dialer() {
                "Outbound"
            } else {
                "Inbound"
            };
            println!(
                "    Connection established directly ({direct_type}) with: {peer_id} via {}",
                endpoint.get_remote_address()
            );
        }
        SwarmEvent::ConnectionClosed { peer_id, .. } => {
            swarm
                .behaviour_mut()
                .gossipsub
                .remove_explicit_peer(&peer_id);
            println!("    Connection closed with: {peer_id}");
        }
        SwarmEvent::OutgoingConnectionError {
            peer_id,
            error,
            ..
        } => {
            match peer_id {
                Some(peer_id) => eprintln!("Dial failed for peer {peer_id}: {error}"),
                None => eprintln!("Dial failed: {error}"),
            }
        }
        SwarmEvent::NewListenAddr { address, .. } => {
            println!("Listening on {address}");
        }

        SwarmEvent::Behaviour(MyBehaviourEvent::RelayServer(
            relay::Event::ReservationReqAccepted { src_peer_id, .. },
        )) => {
            println!("Relay server: Accepted reservation request from peer: {src_peer_id}");
        }
        SwarmEvent::Behaviour(MyBehaviourEvent::RelayClient(
            relay::client::Event::ReservationReqAccepted { relay_peer_id, .. },
        )) => {
            println!(
                "Relay client: Successfully registered reservation through proxy relay: {relay_peer_id}"
            );
        }
        _ => {}
    }
}

// Small formatting helper helper utility function
fn display_received_message(source_context: &str, peer_id: libp2p::PeerId, msg: Message) {
    match msg {
        Message::Chat(text) => println!(" [{peer_id}] ({source_context} - Chat): {text}"),
        Message::FileChunk {
            file_name,
            chunk_index,
            data,
        } => {
            println!(
                " [{peer_id}] ({source_context} - File) Chunk {chunk_index} for '{file_name}' ({} bytes)",
                data.len()
            );
        }
        Message::PeerInfo {
            alias,
            capabilities,
        } => {
            println!(
                " [{peer_id}] ({source_context} - Metadata) Node: {alias}, Specs: {capabilities:?}"
            );
        }
        Message::ServiceDiscovery { service_type } => {
            println!(
                " [{peer_id}] ({source_context} - Discovery) Target scan type: {service_type}"
            );
        }
        Message::RPC { method, params } => {
            println!(
                " [{peer_id}] ({source_context} - RPC) Executing method '{method}' args: {params:?}"
            );
        }
    }
}
