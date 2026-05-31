use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
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
