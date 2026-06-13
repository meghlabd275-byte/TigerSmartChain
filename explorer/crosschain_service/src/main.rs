#![forbid(unsafe_code)]
use crosschain_service::CrossChainService;

fn main() {
    println!("Starting Cross-chain Service");
    
    let service = CrossChainService::new();
    
    println!("Supported chains:");
    for chain in service.get_chains() {
        println!("  {} - {} ({})", chain.id, chain.name, chain.symbol);
    }
    
    println!("\nCross-chain Service running on port 9007");
}