use libp2p::identity::Keypair;
use std::fs;
use std::path::Path;

pub fn load_or_generate_identity(node_dir: &Path) -> Result<Keypair, Box<dyn std::error::Error>> {
    let key_path = node_dir.join("identity.key");

    if key_path.exists() {
        let bytes = fs::read(&key_path)?;
        let keypair = Keypair::from_protobuf_encoding(&bytes)?;
        Ok(keypair)
    } else {
        let new_keypair = Keypair::generate_ed25519();
        let bytes = new_keypair.to_protobuf_encoding()?;
        fs::write(&key_path, bytes)?;
        Ok(new_keypair)
    }
}
