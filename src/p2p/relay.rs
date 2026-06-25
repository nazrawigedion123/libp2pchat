use libp2p::{Multiaddr, PeerId, relay};
use libp2p::relay::client;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct ReservationInfo {
    pub relay_peer_id: PeerId,
    pub expires_at: Option<Instant>,
    pub addresses: Vec<Multiaddr>,
}

#[derive(Debug, Clone)]
pub enum RelayState {
    Unavailable,
    ClientReserved {
        relay_peer_id: PeerId,
        expires_at: Option<Instant>,
    },
    ServerAvailable,
}

#[derive(Debug)]
pub struct RelayManager {
    state: RelayState,
    reservations: Vec<ReservationInfo>,
}

impl RelayManager {
    pub fn new() -> Self {
        Self {
            state: RelayState::Unavailable,
            reservations: Vec::new(),
        }
    }

    pub fn status(&self) -> &RelayState {
        &self.state
    }

    pub fn reservations(&self) -> &[ReservationInfo] {
        &self.reservations
    }

    pub fn handle_client_event(&mut self, event: client::Event) {
        match event {
            client::Event::ReservationReqAccepted { relay_peer_id, .. } => {
                let reservation = ReservationInfo {
                    relay_peer_id: relay_peer_id.clone(),
                    expires_at: None,
                    addresses: Vec::new(),
                };

                self.reservations.push(reservation);
                self.state = RelayState::ClientReserved {
                    relay_peer_id,
                    expires_at: None,
                };
            }
            client::Event::OutboundCircuitEstablished { relay_peer_id, .. } => {
                println!("Relay client outbound circuit established through relay: {relay_peer_id}");
            }
            client::Event::InboundCircuitEstablished { src_peer_id, .. } => {
                println!("Relay client inbound circuit established from: {src_peer_id}");
            }
        }
    }

    pub fn handle_server_event(&mut self, event: relay::Event) {
        match event {
            relay::Event::ReservationReqAccepted { src_peer_id, .. } => {
                println!("Relay server accepted reservation request from {src_peer_id}");
                self.state = RelayState::ServerAvailable;
            }
            relay::Event::ReservationReqDenied { src_peer_id } => {
                eprintln!("Relay server denied reservation from {src_peer_id}");
                self.state = RelayState::Unavailable;
            }
            relay::Event::ReservationTimedOut { src_peer_id } => {
                println!("Relay server reservation timed out for {src_peer_id}");
                self.state = RelayState::Unavailable;
            }
            _ => {}
        }
    }

    pub fn release(&mut self) {
        self.state = RelayState::Unavailable;
        self.reservations.clear();
    }

    pub fn reserve(&mut self, relay_peer_id: PeerId) -> Result<(), String> {
        self.state = RelayState::ClientReserved {
            relay_peer_id,
            expires_at: None,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::PeerId;

    fn random_peer_id() -> PeerId {
        PeerId::random()
    }

    #[test]
    fn relay_manager_transitions_to_client_reserved() {
        let mut manager = RelayManager::new();
        let relay_id = random_peer_id();

        assert!(matches!(manager.status(), RelayState::Unavailable));

        manager.reserve(relay_id.clone()).expect("reserve should succeed");

        assert!(matches!(manager.status(), RelayState::ClientReserved { .. }));

        if let RelayState::ClientReserved { relay_peer_id, .. } = manager.status() {
            assert_eq!(relay_peer_id, &relay_id);
        } else {
            panic!("Expected ClientReserved state");
        }
    }

    #[test]
    fn relay_manager_handles_client_reservation_event() {
        let mut manager = RelayManager::new();
        let relay_id = random_peer_id();
        let event = client::Event::ReservationReqAccepted {
            relay_peer_id: relay_id.clone(),
            renewal: false,
            limit: None,
        };

        manager.handle_client_event(event);

        assert!(matches!(manager.status(), RelayState::ClientReserved { .. }));
        assert_eq!(manager.reservations().len(), 1);
        assert_eq!(&manager.reservations()[0].relay_peer_id, &relay_id);
    }

    #[test]
    fn relay_manager_handles_server_reservation_denied() {
        let mut manager = RelayManager::new();
        let peer_id = random_peer_id();

        manager.handle_server_event(relay::Event::ReservationReqDenied { src_peer_id: peer_id });

        assert!(matches!(manager.status(), RelayState::Unavailable));
    }
}
