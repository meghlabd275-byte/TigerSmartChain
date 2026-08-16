//! EC Recovery Precompile (address 0x01)
//!
//! Recovers the Ethereum address that signed a message from the secp256k1
//! signature (hash, v, r, s). Input layout (128 bytes):
//!   [0..32]   hash
//!   [32..64]  v   (only the last byte is used; 27/28 -> 0/1)
//!   [64..96]  r
//!   [96..128] s
//! Output: 32 bytes, left-padded 20-byte address (big-endian).

use secp256k1::{Message, PublicKey, Secp256k1};
use sha3::Digest;
use secp256k1::ecdsa::{RecoverableSignature, RecoveryId};
use sha3::Keccak256;

/// Compare a 32-byte big-endian scalar against the secp256k1 curve order n.
/// Returns true if scalar < n (strictly; n itself is invalid for r/s).
fn scalar_lt_n(b: &[u8; 32]) -> bool {
    let n = secp256k1::constants::CURVE_ORDER;
    for i in 0..32 {
        if b[i] != n[i] {
            return b[i] < n[i];
        }
    }
    false // equal to n -> not strictly less
}

fn scalar_is_zero(b: &[u8; 32]) -> bool {
    b.iter().all(|&x| x == 0)
}

/// Ethereum ecrecover. Returns the 32-byte left-padded address on success.
pub fn ecrecover(input: &[u8], _gas: u64) -> Result<Vec<u8>, String> {
    if input.len() < 128 {
        return Err("input must be at least 128 bytes".to_string());
    }

    let hash = &input[0..32];
    let v_byte = input[63];
    let mut r_arr = [0u8; 32];
    let mut s_arr = [0u8; 32];
    r_arr.copy_from_slice(&input[64..96]);
    s_arr.copy_from_slice(&input[96..128]);

    let recovery_id_byte = match v_byte {
        27 => 0u8,
        28 => 1u8,
        _ => return Err("invalid recovery id (v must be 27 or 28)".to_string()),
    };

    if scalar_is_zero(&r_arr) || scalar_is_zero(&s_arr)
        || !scalar_lt_n(&r_arr) || !scalar_lt_n(&s_arr)
    {
        return Err("invalid signature scalar".to_string());
    }

    let recovery_id = RecoveryId::from_i32(recovery_id_byte as i32)
        .map_err(|_| "invalid recovery id".to_string())?;

    let secp = Secp256k1::verification_only();
    let msg = Message::from_digest_slice(hash)
        .map_err(|e| format!("invalid message hash: {}", e))?;

    let mut compact = [0u8; 64];
    compact[..32].copy_from_slice(&r_arr);
    compact[32..].copy_from_slice(&s_arr);
    let sig = RecoverableSignature::from_compact(&compact, recovery_id)
        .map_err(|e| format!("invalid recoverable signature: {}", e))?;
    let pubkey = secp.recover_ecdsa(&msg, &sig)
        .map_err(|e| format!("recovery failed: {}", e))?;

    let pk_bytes = pubkey.serialize_uncompressed();
    let mut hasher = Keccak256::new();
    hasher.update(&pk_bytes[1..65]);
    let digest = hasher.finalize();
    let address = &digest[12..32];

    let mut out = vec![0u8; 32];
    out[12..32].copy_from_slice(address);
    Ok(out)
}

/// Get precompile address
pub fn get_address() -> u64 {
    0x01
}

/// Recover a public key (helper exposed for tests/external use).
pub fn recover_public_key(hash: &[u8], v: u8, r: &[u8], s: &[u8]) -> Result<PublicKey, String> {
    let recovery_id_byte = match v {
        27 => 0u8,
        28 => 1u8,
        _ => return Err("invalid v".to_string()),
    };
    if r.len() != 32 || s.len() != 32 {
        return Err("r and s must be 32 bytes".to_string());
    }
    let mut compact = [0u8; 64];
    compact[..32].copy_from_slice(r);
    compact[32..].copy_from_slice(s);
    let recovery_id = RecoveryId::from_i32(recovery_id_byte as i32)
        .map_err(|_| "invalid recovery id".to_string())?;
    let secp = Secp256k1::verification_only();
    let msg = Message::from_digest_slice(hash)
        .map_err(|e| format!("invalid hash: {}", e))?;
    let sig = RecoverableSignature::from_compact(&compact, recovery_id)
        .map_err(|e| format!("invalid sig: {}", e))?;
    secp.recover_ecdsa(&msg, &sig)
        .map_err(|e| format!("recovery failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::SecretKey;

    #[test]
    fn test_ecrecover_round_trip() {
        let secp = Secp256k1::signing_only();
        let secret = SecretKey::from_slice(
            &hex::decode("4646464646464646464646464646464646464646464646464646464646464646").unwrap(),
        ).unwrap();
        let pubkey = secp256k1::PublicKey::from_secret_key(&secp, &secret);
        let pk_bytes = pubkey.serialize_uncompressed();
        let mut h = Keccak256::new();
        h.update(&pk_bytes[1..]);
        let expected_addr = h.finalize();
        let expected = &expected_addr[12..];

        let msg = Message::from_digest_slice(&[0u8; 32]).unwrap();
        let sig = secp.sign_ecdsa_recoverable(&msg, &secret);
        let (recov_id, compact) = sig.serialize_compact();
        let v = recov_id.to_i32() as u8 + 27;

        let mut input = vec![0u8; 128];
        input[63] = v;
        input[64..96].copy_from_slice(&compact[..32]);
        input[96..128].copy_from_slice(&compact[32..]);
        let out = ecrecover(&input, 3000).unwrap();
        assert_eq!(&out[12..32], expected);
    }

    #[test]
    fn test_ecrecover_invalid_short_input() {
        assert!(ecrecover(&[0u8; 10], 3000).is_err());
    }

    #[test]
    fn test_ecrecover_invalid_v() {
        let mut input = vec![0u8; 128];
        input[63] = 5;
        assert!(ecrecover(&input, 3000).is_err());
    }

    #[test]
    fn test_ecrecover_zero_signature_invalid() {
        let mut input = vec![0u8; 128];
        input[63] = 27;
        assert!(ecrecover(&input, 3000).is_err());
    }
}
