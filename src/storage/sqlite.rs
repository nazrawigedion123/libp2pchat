use rusqlite::Connection;
use std::path::Path;

pub fn open(node_dir: &Path) -> Result<Connection, rusqlite::Error> {
    Connection::open(node_dir.join("peer.db"))
}
