//! Block sealing: the proposer's signature over a block header.
//!
//! A validator set alone does not make a chain safe. If the only check on a
//! block is that its `miner` field names a validator, then anyone can produce
//! a block that claims to come from any validator: the field is just a string,
//! and nothing ties it to a key. Such a block would be accepted, so an
//! attacker could mint balances, censor transactions, or rewrite history
//! without ever holding a validator's key.
//!
//! The seal closes that hole. The proposer signs the header hash with the key
//! behind its address, and every node recovers the signer and checks that it
//! matches the claimed `miner`. Producing an acceptable block then requires
//! the private key, not just knowledge of the address.
//!
//! The signature is carried in the header's `extra_data` field. That field is
//! free-form by definition, is already part of the header hash, and is the
//! same place other Ethereum-family chains put their consensus seal, so a
//! block with a seal still hashes to the value every other client computes.

use crate::block::block_hash;
use crate::types::BlockHeader;
use crate::ValidationError;

/// Bytes in a seal: a 65-byte `r || s || v` secp256k1 signature.
pub const SEAL_LENGTH: usize = 65;

/// Marks an `extra_data` field as carrying a seal rather than free text.
///
/// The prefix keeps a sealed header distinguishable from a genesis header,
/// whose `extra_data` is a human-readable chain name.
pub const SEAL_MAGIC: [u8; 4] = *b"TSCS";

/// The number of the earliest block that must carry a seal.
///
/// Genesis is a configuration file rather than a produced block, so it is
/// exempt; every block after it is not.
pub const FIRST_SEALED_BLOCK: u64 = 1;

/// Sign `header` with `private_key`, returning the `extra_data` value that
/// carries the seal.
///
/// The signature commits to the header hash, so changing any field of the
/// header after signing invalidates it.
pub fn seal_header(
    header: &BlockHeader,
    private_key: &[u8; 32],
) -> Result<String, ValidationError> {
    // The seal is computed over the header as it stands, so `extra_data` must
    // be cleared first: its value cannot depend on itself.
    let mut unsigned = header.clone();
    unsigned.extra_data = "0x".to_string();
    let hash = block_hash(&unsigned)?;
    let digest = decode_hash(&hash)?;

    let signature = tiger_crypto::sign(&digest, private_key)
        .map_err(|_| ValidationError::MalformedField("seal: cannot sign header"))?;

    let mut bytes = Vec::with_capacity(SEAL_MAGIC.len() + SEAL_LENGTH);
    bytes.extend_from_slice(&SEAL_MAGIC);
    bytes.extend_from_slice(&signature.r());
    bytes.extend_from_slice(&signature.s());
    // `v` is the recovery id plus 27, the convention `ecrecover` and the
    // precompile use, so the seal is verifiable by the same code path as a
    // transaction signature.
    bytes.push(signature.recovery_id.saturating_add(27));

    Ok(format!("0x{}", crate::block::hex_encode(&bytes)))
}

/// The address that sealed `header`, if it carries a well-formed seal.
///
/// Returns `None` when the field is not a seal, is truncated, or does not
/// recover to any address, so a caller can tell "unsealed" from "badly
/// sealed" by also checking [`has_seal`].
pub fn recover_sealer(header: &BlockHeader) -> Option<[u8; 20]> {
    let raw = header.extra_data.strip_prefix("0x").unwrap_or(&header.extra_data);
    let bytes = decode_hex(raw)?;
    if bytes.len() != SEAL_MAGIC.len() + SEAL_LENGTH {
        return None;
    }
    if bytes[..SEAL_MAGIC.len()] != SEAL_MAGIC {
        return None;
    }

    let body = &bytes[SEAL_MAGIC.len()..];
    let signature = &body[..64];
    let v = body[64];

    let mut unsigned = header.clone();
    unsigned.extra_data = "0x".to_string();
    let digest = decode_hash(&block_hash(&unsigned).ok()?).ok()?;

    tiger_crypto::recover_sender(&digest, signature, v).ok()
}

/// Whether `header`'s `extra_data` is shaped like a seal.
pub fn has_seal(header: &BlockHeader) -> bool {
    let raw = header.extra_data.strip_prefix("0x").unwrap_or(&header.extra_data);
    match decode_hex(raw) {
        Some(bytes) => {
            bytes.len() == SEAL_MAGIC.len() + SEAL_LENGTH
                && bytes[..SEAL_MAGIC.len()] == SEAL_MAGIC
        }
        None => false,
    }
}

/// Check that `header` is sealed by the validator its `miner` field claims.
///
/// A block at or after [`FIRST_SEALED_BLOCK`] must carry a valid seal whose
/// signer is exactly `miner`. Both the absence of a seal and a seal from the
/// wrong key are rejected: either one would let a block be produced without
/// the claimed validator's key.
pub fn verify_seal(header: &BlockHeader) -> Result<(), ValidationError> {
    if header.number < FIRST_SEALED_BLOCK {
        return Ok(());
    }

    if !has_seal(header) {
        return Err(ValidationError::MissingSeal { number: header.number });
    }

    let signer = recover_sealer(header)
        .ok_or(ValidationError::InvalidSeal { number: header.number })?;

    let claimed = crate::block::decode_hex_field(&header.miner, Some(20))?;
    let mut miner = [0u8; 20];
    miner.copy_from_slice(&claimed);

    if signer != miner {
        return Err(ValidationError::SealSignerMismatch {
            number: header.number,
            signer: format!("0x{}", crate::block::hex_encode(&signer)),
            claimed: header.miner.clone(),
        });
    }

    Ok(())
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

fn decode_hash(hash: &str) -> Result<[u8; 32], ValidationError> {
    let bytes = crate::block::decode_hex_field(hash, Some(32))?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BlockHeader;

    fn header(number: u64) -> BlockHeader {
        BlockHeader {
            parent_hash: format!("0x{}", crate::block::hex_encode(&[0x11; 32])),
            sha3_uncles: format!("0x{}", crate::block::hex_encode(&[0u8; 32])),
            miner: "0x0000000000000000000000000000000000000000".to_string(),
            state_root: format!("0x{}", crate::block::hex_encode(&[0x22; 32])),
            transactions_root: format!("0x{}", crate::block::hex_encode(&[0x33; 32])),
            receipts_root: format!("0x{}", crate::block::hex_encode(&[0x44; 32])),
            logs_bloom: format!("0x{}", crate::block::hex_encode(&[0u8; 256])),
            difficulty: "0x0".to_string(),
            number,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 1_700_000_000,
            extra_data: "0x".to_string(),
            mix_hash: format!("0x{}", crate::block::hex_encode(&[0u8; 32])),
            nonce: "0x0000000000000000".to_string(),
            base_fee_per_gas: Some(1_000_000_000),
            withdrawals_root: None,
        }
    }

    #[test]
    fn a_sealed_header_recovers_its_signer() {
        let key = [0x11u8; 32];
        let address = tiger_crypto::private_key_to_address(&key).unwrap();
        let mut h = header(1);
        h.miner = format!("0x{}", crate::block::hex_encode(&address));

        h.extra_data = seal_header(&h, &key).unwrap();

        assert!(has_seal(&h));
        assert_eq!(recover_sealer(&h).unwrap(), address);
        assert!(verify_seal(&h).is_ok());
    }

    #[test]
    fn a_header_without_a_seal_is_rejected() {
        let key = [0x11u8; 32];
        let address = tiger_crypto::private_key_to_address(&key).unwrap();
        let mut h = header(1);
        h.miner = format!("0x{}", crate::block::hex_encode(&address));
        h.extra_data = "0x".to_string();

        assert!(!has_seal(&h));
        assert!(matches!(
            verify_seal(&h),
            Err(ValidationError::MissingSeal { .. })
        ));
    }

    /// The attack the seal exists to stop: claiming another validator's
    /// address without holding its key.
    #[test]
    fn a_seal_from_the_wrong_key_is_rejected() {
        let attacker = [0x11u8; 32];
        let victim = [0x22u8; 32];
        let victim_address = tiger_crypto::private_key_to_address(&victim).unwrap();

        let mut h = header(1);
        h.miner = format!("0x{}", crate::block::hex_encode(&victim_address));
        // The attacker signs, but claims to be the victim.
        h.extra_data = seal_header(&h, &attacker).unwrap();

        assert!(matches!(
            verify_seal(&h),
            Err(ValidationError::SealSignerMismatch { .. })
        ));
    }

    /// Changing any field after sealing must invalidate the seal, otherwise a
    /// relayer could rewrite a block in flight.
    #[test]
    fn tampering_with_a_sealed_header_invalidates_it() {
        let key = [0x11u8; 32];
        let address = tiger_crypto::private_key_to_address(&key).unwrap();
        let mut h = header(1);
        h.miner = format!("0x{}", crate::block::hex_encode(&address));
        h.extra_data = seal_header(&h, &key).unwrap();
        assert!(verify_seal(&h).is_ok());

        // Redirect the block reward by editing the header after the fact.
        h.state_root = format!("0x{}", crate::block::hex_encode(&[0x99; 32]));
        assert!(
            verify_seal(&h).is_err(),
            "a header edited after sealing must not verify"
        );
    }

    #[test]
    fn genesis_is_exempt_from_sealing() {
        let mut h = header(0);
        h.extra_data = format!("0x{}", crate::block::hex_encode(b"TigerSmartChain"));
        assert!(verify_seal(&h).is_ok());
    }

    #[test]
    fn a_truncated_seal_is_rejected() {
        let key = [0x11u8; 32];
        let address = tiger_crypto::private_key_to_address(&key).unwrap();
        let mut h = header(1);
        h.miner = format!("0x{}", crate::block::hex_encode(&address));
        let seal = seal_header(&h, &key).unwrap();
        // Drop the last byte of the signature.
        h.extra_data = seal[..seal.len() - 2].to_string();

        assert!(!has_seal(&h));
        assert!(verify_seal(&h).is_err());
    }
}
