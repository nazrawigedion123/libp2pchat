use super::Message;

pub fn message(method: String, params: Vec<String>) -> Message {
    Message::RPC { method, params }
}
