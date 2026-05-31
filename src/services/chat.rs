use crate::protocol::Message;

pub fn parse_input(line: String) -> Message {
    Message::Chat(line)
}
