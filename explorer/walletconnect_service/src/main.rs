#![forbid(unsafe_code)]
use walletconnect_service::{WalletConnectService, WalletConnectConfig, ClientMetadata};

fn main() {
    println!("Starting WalletConnect Service");
    
    let config = WalletConnectConfig {
        project_id: std::env::var("WALLETCONNECT_PROJECT_ID").unwrap_or_default(),
        relay_url: "wss://relay.walletconnect.com".to_string(),
        metadata: ClientMetadata {
            name: "TigerScan".to_string(),
            description: "TigerScan Blockchain Explorer".to_string(),
            url: "https://tigerscan.io".to_string(),
            icons: vec!["https://tigerscan.io/icon.png".to_string()],
        },
    };
    
    let service = WalletConnectService::new(config);
    let uri = service.create_proposal();
    println!("Pairing URI: {}", uri);
    
    println!("WalletConnect Service running on port 9006");
}