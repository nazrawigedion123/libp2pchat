mod identity;
mod p2p;
mod protocol;
mod services;
mod storage;
mod vpn;

use directories::ProjectDirs;
use std::{env, error::Error, fs, path::PathBuf, time::Duration};

struct AppConfig {
    node_name: String,
    local_proxy_port: i32,
    public_router_port: i32,
    remote_peer_internet_addr: String,
    bootstrap_mode: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = parse_args();
    let node_dir = node_data_dir(&config.node_name)?;

    vpn::start_tunnel(
        config.local_proxy_port,
        config.public_router_port,
        config.remote_peer_internet_addr,
    );

    tokio::time::sleep(Duration::from_secs(1)).await;

    p2p::run_chat_node(p2p::NodeConfig {
        node_dir,
        local_proxy_port: config.local_proxy_port,
        bootstrap_mode: config.bootstrap_mode,
    })
    .await
}

fn parse_args() -> AppConfig {
    let args: Vec<String> = env::args().collect();
    if args.len() < 5 {
        print_usage();
        std::process::exit(1);
    }

    AppConfig {
        node_name: args[1].clone(),
        local_proxy_port: args[2].parse().unwrap(),
        public_router_port: args[3].parse().unwrap(),
        remote_peer_internet_addr: args[4].clone(),
        bootstrap_mode: args.get(5).cloned(),
    }
}

fn node_data_dir(node_name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let mut node_dir = if env::var("FLY_APP_NAME").is_ok() {
        PathBuf::from("/data")
    } else {
        ProjectDirs::from("", "", "myapp")
            .map(|p| p.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };
    node_dir.push(node_name);
    fs::create_dir_all(&node_dir)?;
    Ok(node_dir)
}

fn print_usage() {
    eprintln!("Usage:");
    eprintln!(
        "  As Bootstrap Node A:\n    cargo run -- <node_name> <local_proxy_port> <public_listen_port> <remote_target_ip:port> bootstrap"
    );
    eprintln!(
        "  As Peer Node B:\n    cargo run -- <node_name> <local_proxy_port> <public_listen_port> <remote_target_ip:port> /ip4/.../p2p/<BOOTSTRAP_PEER_ID>"
    );}
