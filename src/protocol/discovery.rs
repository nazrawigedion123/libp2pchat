use super::Message;

pub fn service_query(service_type: impl Into<String>) -> Message {
    Message::ServiceDiscovery {
        service_type: service_type.into(),
    }
}
