use super::Message;

pub fn encode(message: &Message) -> Result<Vec<u8>, bincode::Error> {
    bincode::serialize(message)
}

pub fn decode(bytes: &[u8]) -> Result<Message, bincode::Error> {
    bincode::deserialize(bytes)
}
