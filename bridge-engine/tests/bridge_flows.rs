//! Integration tests for the lock/mint/burn/unlock bridge flows.
//!
//! These exercise the *real* Ed25519 relayer signature verification and the
//! full state machine (no database: in-memory path).

use tigersmartchain_bridge::{
    generate_signing_key, sign_event_id, verify_relayer_signature, BridgeConfig, BridgeEngine,
    Chain, ChainConfig, FeeConfig,
};
use ed25519_dalek::VerifyingKey;

fn test_config(relayer_pubkeys: Vec<String>) -> BridgeConfig {
    BridgeConfig {
        chains: vec![
            ChainConfig {
                chain: Chain::TigerSmartChain,
                rpc_url: "http://localhost:8545".to_string(),
                contract_address: "0x1".to_string(),
                start_block: 0,
            },
            ChainConfig {
                chain: Chain::Ethereum,
                rpc_url: "http://localhost:8546".to_string(),
                contract_address: "0x2".to_string(),
                start_block: 0,
            },
        ],
        relayers: vec![],
        validators: vec![],
        relayers_pubkeys: relayer_pubkeys,
        signature_threshold: 1,
        confirmation_blocks: 1,
        fee: FeeConfig {
            flat_fee: "0".into(),
            percentage_fee: 0.0,
            min_fee: "0".into(),
            max_fee: "0".into(),
        },
        database_url: String::new(),
    }
}

fn pubkey_hex(sk: &ed25519_dalek::SigningKey) -> String {
    let vk: VerifyingKey = sk.verifying_key();
    format!("0x{}", hex::encode(vk.to_bytes()))
}

#[tokio::test]
async fn lock_mint_burn_unlock_happy_path() {
    let relayer = generate_signing_key();
    let config = test_config(vec![pubkey_hex(&relayer)]);
    let bridge = BridgeEngine::new(config);
    // No init() so no RPC/DB connection is attempted.

    // 1. Lock 1000 tokens on TigerSmartChain, bridging to Ethereum.
    let lock = bridge
        .lock(
            Chain::TigerSmartChain,
            Chain::Ethereum,
            "0xuser".to_string(),
            "0xtoken".to_string(),
            1000,
            "0xsourcetx".to_string(),
            None,
        )
        .await
        .expect("lock");
    assert_eq!(lock.amount, 1000);
    assert_eq!(lock.target_chain, Chain::Ethereum);
    assert!(lock.id.starts_with("0x"));

    // 2. Mint: relayer signs the lock event id.
    let sig = sign_event_id(&relayer, &lock.id);
    let mint = bridge
        .mint(&lock.id, &pubkey_hex(&relayer), &sig)
        .await
        .expect("mint");
    assert_eq!(mint.amount, 1000);
    assert_eq!(mint.new_balance, 1000);
    assert_eq!(
        bridge
            .wrapped_balance(Chain::Ethereum, "0xuser", "0xtoken")
            .await,
        1000
    );

    // 3. Burn 400 wrapped tokens on Ethereum to unlock on TigerSmartChain.
    let burn = bridge
        .burn(
            Chain::TigerSmartChain,
            Chain::Ethereum,
            "0xuser".to_string(),
            "0xtoken".to_string(),
            400,
            "0xburntx".to_string(),
        )
        .await
        .expect("burn");
    assert_eq!(burn.amount, 400);
    assert_eq!(
        bridge
            .wrapped_balance(Chain::Ethereum, "0xuser", "0xtoken")
            .await,
        600
    );

    // 4. Unlock: relayer signs the burn event id.
    let burn_sig = sign_event_id(&relayer, &burn.id);
    let unlock = bridge
        .unlock(&burn.id, &pubkey_hex(&relayer), &burn_sig)
        .await
        .expect("unlock");
    assert_eq!(unlock.amount, 400);
    assert_eq!(unlock.new_balance, 400);
    assert_eq!(
        bridge
            .unlocked_balance(Chain::TigerSmartChain, "0xuser", "0xtoken")
            .await,
        400
    );
}

#[tokio::test]
async fn mint_rejects_invalid_signature() {
    let relayer = generate_signing_key();
    let impostor = generate_signing_key();
    let config = test_config(vec![pubkey_hex(&relayer)]);
    let bridge = BridgeEngine::new(config);

    let lock = bridge
        .lock(
            Chain::TigerSmartChain,
            Chain::Ethereum,
            "0xuser".to_string(),
            "0xtoken".to_string(),
            500,
            "0xsourcetx".to_string(),
            None,
        )
        .await
        .unwrap();

    // Impostor signs the lock id with its own key (not the authorized key).
    let bad_sig = sign_event_id(&impostor, &lock.id);
    let err = bridge
        .mint(&lock.id, &pubkey_hex(&relayer), &bad_sig)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("unauthorized") || err.to_string().contains("invalid"),
        "got: {}",
        err
    );

    // No wrapped tokens were minted.
    assert_eq!(
        bridge
            .wrapped_balance(Chain::Ethereum, "0xuser", "0xtoken")
            .await,
        0
    );
}

#[tokio::test]
async fn mint_rejects_unauthorized_relayer_key() {
    let relayer = generate_signing_key();
    let unauthorized = generate_signing_key();
    // Only `relayer` is authorized.
    let config = test_config(vec![pubkey_hex(&relayer)]);
    let bridge = BridgeEngine::new(config);

    let lock = bridge
        .lock(
            Chain::TigerSmartChain,
            Chain::Ethereum,
            "0xuser".to_string(),
            "0xtoken".to_string(),
            500,
            "0xsourcetx".to_string(),
            None,
        )
        .await
        .unwrap();

    // `unauthorized` is not in the relayer set, even with a valid self-signature.
    let sig = sign_event_id(&unauthorized, &lock.id);
    let err = bridge
        .mint(&lock.id, &pubkey_hex(&unauthorized), &sig)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("unauthorized") || err.to_string().contains("invalid"),
        "got: {}",
        err
    );
}

#[tokio::test]
async fn burn_rejects_insufficient_wrapped_balance() {
    let relayer = generate_signing_key();
    let config = test_config(vec![pubkey_hex(&relayer)]);
    let bridge = BridgeEngine::new(config);

    // No prior mint, so wrapped balance is 0.
    let err = bridge
        .burn(
            Chain::TigerSmartChain,
            Chain::Ethereum,
            "0xuser".to_string(),
            "0xtoken".to_string(),
            100,
            "0xburntx".to_string(),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("insufficient"), "got: {}", err);
}

#[tokio::test]
async fn verify_relayer_signature_round_trip() {
    let sk = generate_signing_key();
    let event_id = "0xdeadbeef";
    let sig = sign_event_id(&sk, event_id);

    assert!(verify_relayer_signature(event_id, &pubkey_hex(&sk), &sig).unwrap());

    // Tampered message -> false.
    assert!(!verify_relayer_signature("0xtampered", &pubkey_hex(&sk), &sig).unwrap());

    // Wrong key -> false.
    let other = generate_signing_key();
    assert!(!verify_relayer_signature(event_id, &pubkey_hex(&other), &sig).unwrap());
}

#[tokio::test]
async fn mint_is_idempotent() {
    let relayer = generate_signing_key();
    let config = test_config(vec![pubkey_hex(&relayer)]);
    let bridge = BridgeEngine::new(config);

    let lock = bridge
        .lock(
            Chain::TigerSmartChain,
            Chain::Ethereum,
            "0xuser".to_string(),
            "0xtoken".to_string(),
            300,
            "0xsourcetx".to_string(),
            None,
        )
        .await
        .unwrap();

    let sig = sign_event_id(&relayer, &lock.id);
    let first = bridge
        .mint(&lock.id, &pubkey_hex(&relayer), &sig)
        .await
        .unwrap();
    assert_eq!(first.new_balance, 300);

    // Second mint with same attestation must not double-mint.
    let second = bridge
        .mint(&lock.id, &pubkey_hex(&relayer), &sig)
        .await
        .unwrap();
    assert_eq!(second.new_balance, 300);
    assert_eq!(
        bridge
            .wrapped_balance(Chain::Ethereum, "0xuser", "0xtoken")
            .await,
        300
    );
}
