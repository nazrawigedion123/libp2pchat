use libp2p::{Multiaddr, PeerId};
use rusqlite::{params, Connection};
use std::path::Path;
use std::str::FromStr;

pub struct PeerStorage {
    conn: Connection,
}

impl PeerStorage {
    pub fn init(node_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let db_path = node_dir.join("peer.db");
        let conn = Connection::open(db_path)?;

        // Create table for known multiaddresses linked to peer IDs
        conn.execute(
            "CREATE TABLE IF NOT EXISTS routing_table (
                peer_id TEXT PRIMARY KEY,
                address TEXT NOT NULL
            )",
            [],
        )?;

        Ok(PeerStorage { conn })
    }

    pub fn save_peer(&self, peer_id: &PeerId, addr: &Multiaddr) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT OR REPLACE INTO routing_table (peer_id, address) VALUES (?1, ?2)",
            params![peer_id.to_string(), addr.to_string()],
        )?;
        Ok(())
    }

    pub fn load_all_peers(&self) -> Result<Vec<(PeerId, Multiaddr)>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare("SELECT peer_id, address FROM routing_table")?;
        let peer_iter = stmt.query_map([], |row| {
            let pid_str: String = row.get(0)?;
            let addr_str: String = row.get(1)?;
            Ok((pid_str, addr_str))
        })?;

        let mut results = Vec::new();
        for peer_res in peer_iter {
            let (pid_str, addr_str) = peer_res?;
            if let (Ok(peer_id), Ok(addr)) = (PeerId::from_str(&pid_str), Multiaddr::from_str(&addr_str)) {
                results.push((peer_id, addr));
            }
        }
        Ok(results)
    }
}