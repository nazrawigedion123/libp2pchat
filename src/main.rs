use futures::stream::StreamExt;
use libp2p::{
    Swarm, dcutr, gossipsub, identify, kad, mdns, noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};
use std::os::raw::c_char;
use std::thread;
use std::{env, ffi::CString};
use std::{error::Error, time::Duration};
use tokio::{io, io::AsyncBufReadExt, select};

#[link(name = "govpn", kind = "static")]
unsafe extern "C" {
    fn StartDirectVPNTunnel(local_port: i32, public_listen_port: i32, remote_addr: *const c_char);
}

#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    dcutr: dcutr::Behaviour,
}

struct AppConfig {
    local_proxy_port: i32,
    public_router_port: i32,
    remote_peer_internet_addr: String,
    bootstrap_mode: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = parse_args();

    start_go_vpn_tunnel(
        config.local_proxy_port,
        config.public_router_port,
        config.remote_peer_internet_addr,
    );

    tokio::time::sleep(Duration::from_secs(1)).await;

    let mut swarm = build_swarm()?;
    let topic = subscribe_to_chat_topic(&mut swarm)?;

    listen_on_local_transports(&mut swarm, config.local_proxy_port)?;

    println!("Local Peer ID: {}", swarm.local_peer_id());
    configure_bootstrap(&mut swarm, config.bootstrap_mode.as_deref())?;

    run_chat_event_loop(swarm, topic).await
}

fn parse_args() -> AppConfig {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        print_usage();
        std::process::exit(1);
    }

    AppConfig {
        local_proxy_port: args[1].parse().unwrap(),
        public_router_port: args[2].parse().unwrap(),
        remote_peer_internet_addr: args[3].clone(),
        bootstrap_mode: args.get(4).cloned(),
    }
}

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  As Bootstrap Node A:");
    eprintln!(
        "    cargo run -- <local_proxy_port> <public_listen_port> <remote_target_ip:port> bootstrap"
    );
    eprintln!("  As Peer Node B:");
    eprintln!(
        "    cargo run -- <local_proxy_port> <public_listen_port> <remote_target_ip:port> /ip4/127.0.0.1/tcp/8500/p2p/<BOOTSTRAP_PEER_ID>"
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
            StartDirectVPNTunnel(
                local_proxy_port,
                public_router_port,
                c_remote_addr.as_ptr(),
            );
        }
    });
}

fn build_swarm() -> Result<Swarm<MyBehaviour>, Box<dyn Error>> {
    let swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(build_behaviour)?
        .build();

    Ok(swarm)
}

fn build_behaviour(
    key: &libp2p::identity::Keypair,
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

    Ok(MyBehaviour {
        gossipsub,
        mdns,
        identify,
        kademlia,
        dcutr,
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
    match bootstrap_mode {
        Some("bootstrap") => {}
        Some(addr_str) => {
            let bootstrap_addr: libp2p::Multiaddr = addr_str.parse()?;
            if let Some(libp2p::multiaddr::Protocol::P2p(peer_id)) = bootstrap_addr.iter().last() {
                swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer_id, bootstrap_addr.clone());
                swarm.behaviour_mut().kademlia.bootstrap()?;
            } else {
                eprintln!(
                    "Error: Provided bootstrap multiaddress must contain the trailing /p2p/<PeerId>"
                );
                std::process::exit(1);
            }
        }
        None => {}
    }

    Ok(())
}

async fn run_chat_event_loop(
    mut swarm: Swarm<MyBehaviour>,
    topic: gossipsub::IdentTopic,
) -> Result<(), Box<dyn Error>> {
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                let _ = swarm
                    .behaviour_mut().gossipsub
                    .publish(topic.clone(), line.as_bytes());
            }
            event = swarm.select_next_some() => handle_swarm_event(event)
        }
    }
}

fn handle_swarm_event(event: SwarmEvent<MyBehaviourEvent>) {
    if let SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
        propagation_source: peer_id,
        message,
        ..
    })) = event {
        println!(
            "[{}] {}",
            peer_id,
            String::from_utf8_lossy(&message.data),
        );
    }
}
