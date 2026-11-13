//! Derive the Ethereum address for a private key given on the command line.
//! Used to turn a sealing key into the validator address pinned in genesis.
fn main() {
    for arg in std::env::args().skip(1) {
        let hex = arg.trim_start_matches("0x");
        let bytes = hex::decode(hex).expect("private key must be hex");
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        let address = tiger_crypto::private_key_to_address(&key).expect("valid key");
        println!("0x{}", hex_encode(&address));
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
