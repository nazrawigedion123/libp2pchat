// // src/main.rs
// use futures::stream::StreamExt;
// use libp2p::{
//     gossipsub, identify, mdns, noise,
//     swarm::{NetworkBehaviour, SwarmEvent},
//     tcp, yamux,
// };
// use std::os::raw::c_char;
// use std::thread;
// use std::{
//     collections::hash_map::DefaultHasher,
//     error::Error,
//     hash::{Hash, Hasher},
//     time::Duration,
// };
// use std::{env, ffi::CString};
// use tokio::{io, io::AsyncBufReadExt, select};
// use tracing_subscriber::EnvFilter;

// #[link(name = "govpn", kind = "static")]
// unsafe extern "C" {
//     // Links to our Go VPN method signature
//     fn StartDirectVPNTunnel(local_port: i32, public_listen_port: i32, remote_addr: *const char);
// }

// // We create a custom network behaviour that combines Gossipsub and Mdns.
// #[derive(NetworkBehaviour)]
// struct MyBehaviour {
//     gossipsub: gossipsub::Behaviour,
//     mdns: mdns::tokio::Behaviour,
//     identify: identify::Behaviour,
// }

// #[tokio::main]
// async fn main() -> Result<(), Box<dyn Error>> {
//     // let local_proxy_port = 8500; // Port where Rust & Go talk locally
//     // let public_router_port = 9500; // Port exposed to the public internet

//     // Target address of your friend's machine.
//     // Leave as "" if you are purely waiting for them to connect to you first.
//     // let remote_peer_internet_addr = "192.168.1.100:9500";

//     //test
//     let args: Vec<String> = env::args().collect();
//     if args.len() < 4 {
//         eprintln!(
//             "Usage: cargo run -- <local_proxy_port> <public_listen_port> <remote_target_ip:port>"
//         );
//         eprintln!("Example Node A: cargo run -- 8500 9500 127.0.0.1:9501");
//         eprintln!("Example Node B: cargo run -- 8501 9501 127.0.0.1:9500");
//         std::process::exit(1);
//     }

//     let local_proxy_port: i32 = args[1].parse().unwrap();
//     let public_router_port: i32 = args[2].parse().unwrap();
//     let remote_peer_internet_addr = args[3].clone();

//     //

//     println!("[Rust] Spin up Go P2P Tunnel in background...");
//     thread::spawn(move || {
//         let c_remote_addr =
//             CString::new(remote_peer_internet_addr).expect("Invalid CString conversion");
//         unsafe {
//             StartDirectVPNTunnel(
//                 local_proxy_port,
//                 public_router_port,
//                 c_remote_addr.as_ptr() as *const char,
//             );
//         }
//     });

//     // Allow the Go routine sockets to initialize cleanly
//     tokio::time::sleep(Duration::from_secs(1)).await;

//     let _ = tracing_subscriber::fmt()
//         .with_env_filter(EnvFilter::from_default_env())
//         .try_init();

//     let mut swarm = libp2p::SwarmBuilder::with_new_identity()
//         .with_tokio()
//         .with_tcp(
//             tcp::Config::default(),
//             noise::Config::new,
//             yamux::Config::default,
//         )?
//         .with_quic()
//         .with_behaviour(|key| {
//             // To content-address message, we can take the hash of message and use it as an ID.
//             let message_id_fn = |message: &gossipsub::Message| {
//                 let mut s = DefaultHasher::new();
//                 message.data.hash(&mut s);
//                 gossipsub::MessageId::from(s.finish().to_string())
//             };

//             // Set a custom gossipsub configuration
//             let gossipsub_config = gossipsub::ConfigBuilder::default()
//                 .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
//                 .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message
//                 // signing)
//                 .message_id_fn(message_id_fn) // content-address messages. No two messages of the same content will be propagated.
//                 .build()
//                 .map_err(io::Error::other)?; // Temporary hack because `build` does not return a proper `std::error::Error`.

//             // build a gossipsub network behaviour
//             let gossipsub = gossipsub::Behaviour::new(
//                 gossipsub::MessageAuthenticity::Signed(key.clone()),
//                 gossipsub_config,
//             )?;

//             let mdns =
//                 mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;

//             let identify = identify::Behaviour::new(identify::Config::new(
//                 "/chat-proto/1.0.0".to_string(),
//                 key.public(),
//             ));
//             Ok(MyBehaviour {
//                 gossipsub,
//                 mdns,
//                 identify,
//             })
//         })?
//         .build();

//     // Create a Gossipsub topic
//     let topic = gossipsub::IdentTopic::new("test-net");
//     // subscribes to our topic
//     swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

//     // Read full lines from stdin
//     let mut stdin = io::BufReader::new(io::stdin()).lines();

//     // Listen on all interfaces and whatever port the OS assigns
//     // swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
//     // swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
//     let listen_multiaddr = format!("/ip4/127.0.0.1/tcp/{}", local_proxy_port).parse()?;
//     swarm.listen_on(listen_multiaddr)?;
//     println!("[Rust] libp2p stack successfully bound to local VPN interface.");

//     println!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");

//     // Kick it off
//     loop {
//         select! {
//             Ok(Some(line)) = stdin.next_line() => {
//                 if let Err(e) = swarm
//                     .behaviour_mut().gossipsub
//                     .publish(topic.clone(), line.as_bytes()) {
//                     println!("Publish error: {e:?}");
//                 }
//             }
//             event = swarm.select_next_some() => match event {
//                 SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
//                     for (peer_id, _multiaddr) in list {
//                         println!("mDNS discovered a new peer: {peer_id}");
//                         swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
//                     }
//                 },
//                 SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
//                     for (peer_id, _multiaddr) in list {
//                         println!("mDNS discover peer has expired: {peer_id}");
//                         swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
//                     }
//                 },
//                 SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
//                     propagation_source: peer_id,
//                     message_id: id,
//                     message,
//                 })) => println!(
//                         "Got message: '{}' with id: {id} from peer: {peer_id}",
//                         String::from_utf8_lossy(&message.data),
//                     ),
//                 SwarmEvent::NewListenAddr { address, .. } => {
//                     println!("Local node is listening on {address}");
//                 }
//                 _ => {}
//             }
//         }
//     }
// }
use futures::stream::StreamExt;
use libp2p::{
    gossipsub, identify, kad, mdns, noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};
use std::os::raw::c_char; // Re-enabling the correct FFI type
use std::thread;
use std::{
    collections::hash_map::DefaultHasher,
    error::Error,
    hash::{Hash, Hasher},
    time::Duration,
};
use std::{env, ffi::CString};
use tokio::{io, io::AsyncBufReadExt, select};
use tracing_subscriber::EnvFilter;

#[link(name = "govpn", kind = "static")]
unsafe extern "C" {
    // Fixed: Uses c_char (1-byte) instead of primitive Rust char (4-byte)
    fn StartDirectVPNTunnel(local_port: i32, public_listen_port: i32, remote_addr: *const c_char);
}

#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage:");
        eprintln!("  As Bootstrap Node A:");
        eprintln!(
            "    cargo run -- <local_proxy_port> <public_listen_port> <remote_target_ip:port> bootstrap"
        );
        eprintln!("  As Peer Node B:");
        eprintln!(
            "    cargo run -- <local_proxy_port> <public_listen_port> <remote_target_ip:port> /ip4/127.0.0.1/tcp/8500/p2p/<BOOTSTRAP_PEER_ID>"
        );
        std::process::exit(1);
    }

    let local_proxy_port: i32 = args[1].parse().unwrap();
    let public_router_port: i32 = args[2].parse().unwrap();
    let remote_peer_internet_addr = args[3].clone();

    let bootstrap_mode = args.get(4).map(|s| s.as_str());

    println!("[Rust] Spin up Go P2P Tunnel in background...");
    thread::spawn(move || {
        let c_remote_addr =
            CString::new(remote_peer_internet_addr).expect("Invalid CString conversion");
        unsafe {
            StartDirectVPNTunnel(
                local_proxy_port,
                public_router_port,
                c_remote_addr.as_ptr(), // Cleanly passes the 1-byte char pointer
            );
        }
    });

    tokio::time::sleep(Duration::from_secs(1)).await;

    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|key| {
            let peer_id = key.public().to_peer_id();
            let message_id_fn = |message: &gossipsub::Message| {
                let mut s = DefaultHasher::new();
                message.data.hash(&mut s);
                gossipsub::MessageId::from(s.finish().to_string())
            };

            // Dropped heartbeat down to 1 second for fast local terminal testing
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(1))
                .validation_mode(gossipsub::ValidationMode::Strict)
                .message_id_fn(message_id_fn)
                .build()
                .map_err(io::Error::other)?;

            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )?;

            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;

            let identify = identify::Behaviour::new(identify::Config::new(
                "/chat-proto/1.0.0".to_string(),
                key.public(),
            ));

            //kadima
            let store = kad::store::MemoryStore::new(peer_id);
            // let kad_config = kad::Config::default();
            let kademlia = kad::Behaviour::new(peer_id, store);

            Ok(MyBehaviour {
                gossipsub,
                mdns,
                identify,
                kademlia,
            })
        })?
        .build();

    let topic = gossipsub::IdentTopic::new("test-net");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    
    let topic = gossipsub::IdentTopic::new("test-net");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    let mut stdin = io::BufReader::new(io::stdin()).lines();
  
    let listen_multiaddr: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", local_proxy_port).parse()?;
    swarm.listen_on(listen_multiaddr)?;
    // println!("Dialing remote libp2p node at {remote_libp2p_addr}");
    println!("[Rust] Local Peer ID: {}", swarm.local_peer_id());
    // handle boot starp mode
    match bootstrap_mode {
        Some("bootstrap") => {
            println!(
                "[Kademlia] Node is running as the Bootstrap point. Waiting for connections..."
            );
        }
        Some(addr_str) => {
            let bootstrap_addr: libp2p::Multiaddr = addr_str.parse()?;

            // Extract peer ID from the multiaddress string (expects format like /ip4/.../p2p/PeerId)
            if let Some(libp2p::multiaddr::Protocol::P2p(peer_id)) = bootstrap_addr.iter().last() {
                println!("[Kademlia] Seeding routing table with Bootstrap node: {peer_id}");

                // Add the bootstrap node address into Kademlia
                swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer_id, bootstrap_addr.clone());

                // Trigger the actual discovery phase
                swarm.behaviour_mut().kademlia.bootstrap()?;
            } else {
                eprintln!(
                    "[Error] Provided bootstrap multiaddress must contain the trailing /p2p/<PeerId>"
                );
                std::process::exit(1);
            }
        }
        None => {
            println!("[Warning] No bootstrapping mode provided. Operating in isolation mode.");
        }
    }

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                if let Err(e) = swarm
                    .behaviour_mut().gossipsub
                    .publish(topic.clone(), line.as_bytes()) {
                    println!("Publish error: {e:?}");
                }
            }
            event = swarm.select_next_some() => match event {
                // Visual Indicator 1: Raw TCP Stream connected
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    println!("[Network] Raw TCP connection established with peer: {peer_id}");
                }
                // Visual Indicator 2: App-level Handshake completed
                SwarmEvent::Behaviour(MyBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. })) => {
                    println!("[Network] Successfully identified peer protocol capability!");
                    println!("          Peer ID: {peer_id}");
                    println!("          Protocols Supported: {:?}", info.protocols);
                }
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source: peer_id,
                    message_id: id,
                    message,
                })) => println!(
                        "Got message: '{}' from peer: {peer_id}",
                        String::from_utf8_lossy(&message.data),
                    ),
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Local node is listening on {address}");
                }
                _ => {}
            }
        }
    }
}
