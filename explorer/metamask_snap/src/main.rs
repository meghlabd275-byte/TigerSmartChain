#![forbid(unsafe_code)]
use metamask_snap::{MetaMaskSnap, SnapConfig};

fn main() {
    println!("Starting MetaMask Snap Service");
    
    let config = SnapConfig {
        snap_id: "npm:@tigerscan/snap".to_string(),
        version: "1.0.0".to_string(),
        rpc_url: std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string()),
        chain_id: 1,
    };
    
    let snap = MetaMaskSnap::new(config);
    let manifest = snap.generate_manifest();
    
    println!("Snap Manifest: {}", serde_json::to_string_pretty(&manifest).unwrap());
    println!("\nMetaMask Snap Service running");
}