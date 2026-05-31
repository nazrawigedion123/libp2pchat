use libp2p::{dcutr, gossipsub, identify, mdns, relay, request_response, swarm::SwarmEvent};

use crate::{protocol, protocol::Message, storage::PeerStorage};

use super::{behaviour::MyBehaviour, swarm::P2PNode};

pub fn handle_swarm_event(
    node: &mut P2PNode,
    event: SwarmEvent<<MyBehaviour as libp2p::swarm::NetworkBehaviour>::ToSwarm>,
    db: &PeerStorage,
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
                let _ = node
                    .swarm
                    .behaviour_mut()
                    .direct_messaging
                    .send_response(channel, response_receipt);
            }
            request_response::Message::Response { response, .. } => {
                display_received_message("Direct Receipt Confirmation", peer_id, response);
            }
        },

        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::Identify(
            identify::Event::Received { peer_id, info, .. },
        )) => {
            node.add_explicit_peer(&peer_id);
            for addr in info.listen_addrs {
                node.add_peer_address(peer_id, addr.clone());
                if let Err(e) = db.save_peer(&peer_id, &addr) {
                    eprintln!("Database write failure: {e}");
                }
            }
        }
        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::Mdns(
            mdns::Event::Discovered(list),
        )) => {
            for (peer_id, addr) in list {
                node.add_explicit_peer(&peer_id);
                node.add_peer_address(peer_id, addr.clone());
                if let Err(e) = db.save_peer(&peer_id, &addr) {
                    eprintln!("Database write failure: {e}");
                }
                println!("mDNS discovered peer {peer_id} at {addr}");
            }
        }
        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::Mdns(mdns::Event::Expired(
            list,
        ))) => {
            for (peer_id, _addr) in list {
                node.remove_explicit_peer(&peer_id);
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
            node.add_explicit_peer(&peer_id);
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
            node.remove_explicit_peer(&peer_id);
            println!("    Connection closed with: {peer_id}");
        }
        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => match peer_id {
            Some(peer_id) => eprintln!("Dial failed for peer {peer_id}: {error}"),
            None => eprintln!("Dial failed: {error}"),
        },
        SwarmEvent::NewListenAddr { address, .. } => {
            println!("Listening on {address}");
        }

        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::RelayServer(
            relay::Event::ReservationReqAccepted { src_peer_id, .. },
        )) => {
            println!("Relay server: Accepted reservation request from peer: {src_peer_id}");
        }
        SwarmEvent::Behaviour(super::behaviour::MyBehaviourEvent::RelayClient(
            relay::client::Event::ReservationReqAccepted { relay_peer_id, .. },
        )) => {
            println!(
                "Relay client: Successfully registered reservation through proxy relay: {relay_peer_id}"
            );
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
