use libp2p::{dcutr, gossipsub, identify, mdns, request_response, swarm::SwarmEvent};

use crate::{
    protocol, protocol::Message,
    services::peers::PeerService,
};

use super::{behaviour::MyBehaviour, relay::RelayManager};

pub fn handle_swarm_event(
    peer_svc: &mut PeerService,
    relay_mgr: &mut RelayManager,
    event: SwarmEvent<<MyBehaviour as libp2p::swarm::NetworkBehaviour>::ToSwarm>,
) {
    match event {
        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::Gossipsub(
            gossipsub::Event::Message {
                propagation_source: peer_id,
                message,
                ..
            },
        )) => match protocol::codec::decode(&message.data) {
            Ok(app_message) => {
                display_received_message("Gossip Mesh Network", peer_id, app_message)
            }
            Err(_) => println!(" [{peer_id}] Received untyped binary text chunk via Gossipsub"),
        },
        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::Gossipsub(
            gossipsub::Event::Subscribed { peer_id, topic },
        )) => {
            println!("Gossipsub: peer {peer_id} subscribed to {topic}");
        }
        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::Gossipsub(
            gossipsub::Event::Unsubscribed { peer_id, topic },
        )) => {
            println!("Gossipsub: peer {peer_id} unsubscribed from {topic}");
        }

        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::DirectMessaging(
            request_response::Event::Message {
                peer: peer_id,
                message,
            },
        )) => match message {
            request_response::Message::Request {
                request, channel, ..
            } => {
                display_received_message("Direct Point-To-Point", peer_id, request.clone());

                let response_receipt =
                    Message::Chat("ACK: Message delivered directly.".to_string());
                let _ = peer_svc.send_direct_response(channel, response_receipt);
            }
            request_response::Message::Response { response, .. } => {
                display_received_message("Direct Receipt Confirmation", peer_id, response);
            }
        },

        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::Identify(
            identify::Event::Received { peer_id, info, .. },
        )) => {
            peer_svc.on_peer_discovered(peer_id, info.listen_addrs);
        }
        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::Mdns(
            mdns::Event::Discovered(list),
        )) => {
            for (peer_id, addr) in list {
                peer_svc.on_peer_discovered(peer_id, vec![addr.clone()]);
                println!("mDNS discovered peer {peer_id} at {addr}");
            }
        }
        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::Mdns(mdns::Event::Expired(
            list,
        ))) => {
            for (peer_id, _addr) in list {
                peer_svc.on_peer_disconnected(peer_id);
            }
        }

        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::Dcutr(dcutr::Event {
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
            peer_svc.on_peer_connected(peer_id);
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
            peer_svc.on_peer_disconnected(peer_id);
            println!("    Connection closed with: {peer_id}");
        }
        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => match peer_id {
            Some(peer_id) => eprintln!("Dial failed for peer {peer_id}: {error}"),
            None => eprintln!("Dial failed: {error}"),
        },
        SwarmEvent::NewListenAddr { address, .. } => {
            println!("Listening on {address}");
        }

        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::RelayServer(event)) => {
            relay_mgr.handle_server_event(event);
        }
        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::RelayClient(event)) => {
            relay_mgr.handle_client_event(event);
        }
        SwarmEvent::Dialing { peer_id, .. } => {
            if let Some(peer_id) = peer_id {
                println!("Dialing peer {peer_id}");
            }
        }
        _ => {}
    }
}

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
