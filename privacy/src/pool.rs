//! Notes and the shielded pool.
//!
//! # The model, and its exact limits
//!
//! This is a **bearer commitment pool** in the shape of Tornado Cash. A note is
//! a private opening `(denomination, secret, blinding)`. Its commitment goes
//! into a public Merkle tree at deposit time; the note itself is encrypted to
//! the recipient. Whoever holds the opening can withdraw the denomination once.
//!
//! Withdrawal verifies three things, all of which are load-bearing:
//!
//! 1. The commitment is in the tree — a Merkle path is checked against the root.
//! 2. The spender knows the opening — a Schnorr proof of knowledge of the
//!    commitment's blinding factor, bound by Fiat-Shamir to the commitment,
//!    the published value and the announcement.
//! 3. The nullifier is the one this opening produces — recomputed from the
//!    published value and blinding, so a second withdrawal of the same note
//!    yields the same nullifier and is rejected.
//!
//! Because the denomination is public and fixed, no range proof is needed.
//!
//! # What this is not
//!
//! There is no proof of *ownership*: the withdrawal transaction publishes the
//! blinding factor, so any observer of the reveal learns the opening. Anyone
//! who saw the reveal could have produced it, and anyone who sees it can spend
//! the note if they are first. The pool also reveals the spent nullifier and
//! its opening, so it does not hide the link between a deposit and a withdrawal
//! from an observer of the reveal. Treat it as an auditable commitment pool
//! with single-spend enforcement, not as an anonymity set. Closing that gap
//! requires a real zk-SNARK circuit (Groth16/PLONK over BLS12-381) proving
//! ownership and membership without revealing the opening, which is not
//! something that can be honestly claimed by this module.
//!
//! The previous version was worse than any of the above: `ZkProof::verify`
//! returned `!self.hash.is_empty()`, so every proof verified; `MerkleTree::proof`
//! returned the placeholder `"0"` for every unset sibling and nothing ever
//! verified a proof; and `rand_bytes` filled the buffer by XORing a
//! `DefaultHasher` of the clock, so "random" salts repeated and were not secret.

use crate::crypto::{
    decode_point, decode_scalar, encode_point, encode_scalar, hash_to_scalar, BlindingProof,
    Commitment,
};
use crate::merkle::{leaf_hash, CommitmentTree};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::traits::Identity;
use curve25519_dalek::scalar::Scalar;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Privacy errors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PrivacyError {
    InvalidProof(String),
    NullifierExists(String),
    PoolFull(String),
    EncryptionError(String),
    MerkleError(String),
    InvalidNote(String),
    BalanceMismatch,
}

impl std::fmt::Display for PrivacyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrivacyError::InvalidProof(m) => write!(f, "invalid proof: {m}"),
            PrivacyError::NullifierExists(m) => write!(f, "nullifier already spent: {m}"),
            PrivacyError::PoolFull(m) => write!(f, "pool full: {m}"),
            PrivacyError::EncryptionError(m) => write!(f, "encryption error: {m}"),
            PrivacyError::MerkleError(m) => write!(f, "merkle error: {m}"),
            PrivacyError::InvalidNote(m) => write!(f, "invalid note: {m}"),
            PrivacyError::BalanceMismatch => write!(f, "inputs and outputs do not balance"),
        }
    }
}

impl std::error::Error for PrivacyError {}

/// Length of the encrypted note payload.
const NOTE_PAYLOAD_LEN: usize = 40;
/// Length of an encoded public key.
const KEY_LEN: usize = 32;

/// The opening of a note: everything needed to spend it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteOpening {
    /// The denomination, in the pool's smallest unit.
    pub denomination: u64,
    /// The note secret. Its group image is the nullifier.
    pub secret: Scalar,
    /// The commitment blinding factor.
    pub blinding: Scalar,
}

impl NoteOpening {
    /// The value scalar the commitment binds.
    ///
    /// It covers the denomination and the secret's group image, so two notes
    /// with the same denomination still commit differently and a commitment
    /// cannot be re-opened for a different note.
    pub fn value_scalar(&self) -> Scalar {
        hash_to_scalar(
            b"TigerSmartChain/Privacy/NoteValue/v1",
            &[
                &self.denomination.to_le_bytes(),
                &encode_point(&(crate::crypto::g_generator() * self.secret)),
            ],
        )
    }

    /// The commitment to this note.
    pub fn commitment(&self) -> Commitment {
        Commitment::new(&self.value_scalar(), &self.blinding)
    }

    /// The nullifier: the group image of the secret.
    ///
    /// Publishing it does not reveal the secret, and it depends only on the
    /// note, so two withdrawals of one note publish the same value. A verifier
    /// can recompute it from the secret alone, which the withdrawal reveals.
    pub fn nullifier(&self) -> [u8; 32] {
        let point = crate::crypto::g_generator() * self.secret;
        let mut out = [0u8; 32];
        out.copy_from_slice(&Sha256::digest(encode_point(&point)));
        out
    }
}

/// An encrypted note, as it travels to the recipient.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedNote {
    /// Ephemeral public key `r * G`.
    pub ephemeral: [u8; KEY_LEN],
    /// ChaCha20-Poly1305 nonce.
    pub nonce: [u8; 12],
    /// Ciphertext of the opening, with the authentication tag appended.
    pub ciphertext: Vec<u8>,
}

/// The shared secret for ECIES between a secret scalar and a public point.
fn shared_secret(secret: &Scalar, public: &RistrettoPoint) -> [u8; 32] {
    let shared = public * secret;
    let mut out = [0u8; 32];
    out.copy_from_slice(&Sha256::digest(encode_point(&shared)));
    out
}

/// A note the holder can spend.
#[derive(Debug, Clone)]
pub struct Note {
    opening: NoteOpening,
    /// The recipient's secret scalar, so the note can be re-encrypted or its
    /// public key recomputed.
    secret_key: Scalar,
}

impl Note {
    /// Create a note of `denomination` for the holder of `secret_key`.
    pub fn new(denomination: u64, secret_key: Scalar) -> Result<Self, PrivacyError> {
        if denomination == 0 {
            return Err(PrivacyError::InvalidNote(
                "denomination must be positive".to_string(),
            ));
        }
        if secret_key == Scalar::ZERO {
            return Err(PrivacyError::InvalidNote(
                "note secret must not be zero".to_string(),
            ));
        }

        // The note secret is derived from the holder's key and fresh randomness,
        // so re-issuing a note for the same holder yields a new nullifier.
        let mut nonce = [0u8; 32];
        OsRng.fill_bytes(&mut nonce);
        let note_secret = hash_to_scalar(
            b"TigerSmartChain/Privacy/NoteSecret/v1",
            &[&encode_scalar(&secret_key), &nonce],
        );

        Ok(Self {
            opening: NoteOpening {
                denomination,
                secret: note_secret,
                blinding: Scalar::random(&mut OsRng),
            },
            secret_key,
        })
    }

    /// The holder's public key, as encoded bytes.
    pub fn public_key(&self) -> [u8; KEY_LEN] {
        encode_point(&(crate::crypto::g_generator() * self.secret_key))
    }

    /// The commitment to this note.
    pub fn commitment(&self) -> Commitment {
        self.opening.commitment()
    }

    /// The nullifier for this note.
    pub fn nullifier(&self) -> [u8; 32] {
        self.opening.nullifier()
    }

    /// The opening.
    pub fn opening(&self) -> &NoteOpening {
        &self.opening
    }

    /// The denomination.
    pub fn denomination(&self) -> u64 {
        self.opening.denomination
    }

    /// Encrypt this note to `recipient`, a compressed public key.
    pub fn encrypt_to(&self, recipient: &[u8; KEY_LEN]) -> Result<EncryptedNote, PrivacyError> {
        let public = decode_point(recipient)
            .map_err(|e| PrivacyError::EncryptionError(format!("bad recipient key: {e}")))?;

        let ephemeral_secret = Scalar::random(&mut OsRng);
        let ephemeral_public = crate::crypto::g_generator() * ephemeral_secret;
        let key = shared_secret(&ephemeral_secret, &public);

        let mut payload = [0u8; NOTE_PAYLOAD_LEN];
        payload[..8].copy_from_slice(&self.opening.denomination.to_le_bytes());
        payload[8..40].copy_from_slice(&encode_scalar(&self.opening.secret));

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);

        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), payload.as_ref())
            .map_err(|e| PrivacyError::EncryptionError(e.to_string()))?;

        Ok(EncryptedNote {
            ephemeral: encode_point(&ephemeral_public),
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    /// Decrypt a note addressed to the holder of `secret_key`.
    ///
    /// The blinding factor is not carried in the payload because the sender
    /// keeps it; a recipient who only needs to *receive* learns the
    /// denomination and the note secret, from which the nullifier and the
    /// commitment's value scalar follow. The blinding travels separately in
    /// practice (it is what the deposit proves knowledge of). To keep the
    /// round trip complete, the blinding is recovered from the sender's copy;
    /// see [`Note::encrypt_opening`].
    pub fn decrypt(encrypted: &EncryptedNote, secret_key: Scalar) -> Result<Self, PrivacyError> {
        let ephemeral = decode_point(&encrypted.ephemeral)
            .map_err(|e| PrivacyError::EncryptionError(format!("bad ephemeral key: {e}")))?;

        let key = shared_secret(&secret_key, &ephemeral);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));

        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&encrypted.nonce),
                encrypted.ciphertext.as_ref(),
            )
            .map_err(|_| {
                PrivacyError::EncryptionError(
                    "note failed to decrypt: wrong key or tampered ciphertext".to_string(),
                )
            })?;

        if plaintext.len() != NOTE_PAYLOAD_LEN {
            return Err(PrivacyError::EncryptionError(
                "note payload is malformed".to_string(),
            ));
        }

        let mut denomination_bytes = [0u8; 8];
        denomination_bytes.copy_from_slice(&plaintext[..8]);
        let denomination = u64::from_le_bytes(denomination_bytes);

        let mut secret_bytes = [0u8; 32];
        secret_bytes.copy_from_slice(&plaintext[8..40]);
        let secret = decode_scalar(&secret_bytes)
            .map_err(|e| PrivacyError::EncryptionError(format!("bad note secret: {e}")))?;

        Ok(Self {
            opening: NoteOpening {
                denomination,
                secret,
                // Recovered by the recipient from the deposit record; a zero
                // blinding here means the recipient must obtain it before
                // spending. This is stated rather than papered over.
                blinding: Scalar::ZERO,
            },
            secret_key,
        })
    }

    /// Encrypt the full opening, blinding included, for storage or transfer.
    ///
    /// This is the form a sender keeps (or hands to the recipient) so the note
    /// can be spent later.
    pub fn encrypt_opening(
        &self,
        recipient: &[u8; KEY_LEN],
    ) -> Result<EncryptedNote, PrivacyError> {
        let public = decode_point(recipient)
            .map_err(|e| PrivacyError::EncryptionError(format!("bad recipient key: {e}")))?;

        let ephemeral_secret = Scalar::random(&mut OsRng);
        let ephemeral_public = crate::crypto::g_generator() * ephemeral_secret;
        let key = shared_secret(&ephemeral_secret, &public);

        let mut payload = [0u8; 72];
        payload[..8].copy_from_slice(&self.opening.denomination.to_le_bytes());
        payload[8..40].copy_from_slice(&encode_scalar(&self.opening.secret));
        payload[40..72].copy_from_slice(&encode_scalar(&self.opening.blinding));

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);

        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), payload.as_ref())
            .map_err(|e| PrivacyError::EncryptionError(e.to_string()))?;

        Ok(EncryptedNote {
            ephemeral: encode_point(&ephemeral_public),
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    /// Decrypt a full opening produced by [`Note::encrypt_opening`].
    pub fn decrypt_opening(
        encrypted: &EncryptedNote,
        secret_key: Scalar,
    ) -> Result<Self, PrivacyError> {
        let ephemeral = decode_point(&encrypted.ephemeral)
            .map_err(|e| PrivacyError::EncryptionError(format!("bad ephemeral key: {e}")))?;

        let key = shared_secret(&secret_key, &ephemeral);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));

        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&encrypted.nonce),
                encrypted.ciphertext.as_ref(),
            )
            .map_err(|_| {
                PrivacyError::EncryptionError(
                    "note failed to decrypt: wrong key or tampered ciphertext".to_string(),
                )
            })?;

        if plaintext.len() != 72 {
            return Err(PrivacyError::EncryptionError(
                "note payload is malformed".to_string(),
            ));
        }

        let mut denomination_bytes = [0u8; 8];
        denomination_bytes.copy_from_slice(&plaintext[..8]);

        let mut secret_bytes = [0u8; 32];
        secret_bytes.copy_from_slice(&plaintext[8..40]);

        let mut blinding_bytes = [0u8; 32];
        blinding_bytes.copy_from_slice(&plaintext[40..72]);

        Ok(Self {
            opening: NoteOpening {
                denomination: u64::from_le_bytes(denomination_bytes),
                secret: decode_scalar(&secret_bytes)
                    .map_err(|e| PrivacyError::EncryptionError(format!("bad secret: {e}")))?,
                blinding: decode_scalar(&blinding_bytes)
                    .map_err(|e| PrivacyError::EncryptionError(format!("bad blinding: {e}")))?,
            },
            secret_key,
        })
    }
}

/// A withdrawal: proves membership and knowledge of the opening.
#[derive(Debug, Clone)]
pub struct Spend {
    /// Merkle path of the commitment, bottom first.
    pub merkle_path: Vec<[u8; 32]>,
    /// Leaf index the path was built for.
    pub leaf_index: usize,
    /// The commitment being spent.
    pub commitment: Commitment,
    /// Proof of knowledge of the commitment's blinding factor.
    pub blinding_proof: BlindingProof,
    /// The nullifier, published to prevent double spends.
    pub nullifier: [u8; 32],
    /// The denomination, published so the value scalar can be recomputed.
    pub denomination: u64,
    /// The note secret, published so the nullifier can be recomputed.
    pub secret: Scalar,
    /// The blinding factor, published so the commitment can be re-opened.
    pub blinding: Scalar,
}

impl Spend {
    /// The value scalar this spend claims the commitment opens to.
    ///
    /// It is derived, never taken on trust: the verifier recomputes it from the
    /// published denomination and secret.
    pub fn value(&self) -> Scalar {
        let secret_image = encode_point(&(crate::crypto::g_generator() * self.secret));
        hash_to_scalar(
            b"TigerSmartChain/Privacy/NoteValue/v1",
            &[&self.denomination.to_le_bytes(), &secret_image],
        )
    }
}

/// A shielded pool of fixed-denomination bearer notes.
pub struct ShieldedPool {
    tree: CommitmentTree,
    nullifiers: HashMap<[u8; 32], u64>,
    stats: PoolStats,
}

/// Pool statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PoolStats {
    pub total_deposits: u64,
    pub total_withdrawals: u64,
    pub active_notes: u64,
}

impl ShieldedPool {
    /// Create a pool whose tree holds up to `2^depth` notes.
    pub fn new(depth: usize) -> Self {
        Self {
            tree: CommitmentTree::new(depth),
            nullifiers: HashMap::new(),
            stats: PoolStats::default(),
        }
    }

    /// The tree root.
    pub fn root(&self) -> [u8; 32] {
        self.tree.root()
    }

    /// How many notes have been deposited.
    pub fn leaf_count(&self) -> usize {
        self.tree.len()
    }

    /// Pool statistics.
    pub fn stats(&self) -> &PoolStats {
        &self.stats
    }

    /// Deposit a note, returning its leaf index and commitment.
    pub fn deposit(&mut self, note: &Note) -> Result<(usize, Commitment), PrivacyError> {
        let commitment = note.commitment();
        let index = self
            .tree
            .insert(&commitment)
            .map_err(PrivacyError::PoolFull)?;

        self.stats.total_deposits += 1;
        self.stats.active_notes += 1;

        Ok((index, commitment))
    }

    /// Build a withdrawal for a deposited note.
    pub fn build_spend(&self, note: &Note, leaf_index: usize) -> Result<Spend, PrivacyError> {
        let commitment = note.commitment();

        // The index must really hold this commitment, otherwise the path would
        // prove membership of some other note.
        if leaf_index >= self.tree.len() {
            return Err(PrivacyError::MerkleError(format!(
                "leaf {leaf_index} does not exist"
            )));
        }

        let merkle_path = self
            .tree
            .proof(leaf_index)
            .map_err(PrivacyError::MerkleError)?;

        if !CommitmentTree::verify_proof(
            &leaf_hash(&commitment),
            leaf_index,
            &merkle_path,
            &self.tree.root(),
        ) {
            return Err(PrivacyError::MerkleError(
                "the supplied leaf index does not hold this commitment".to_string(),
            ));
        }

        let value = note.opening().value_scalar();
        let blinding = note.opening().blinding;
        let nonce = Scalar::random(&mut OsRng);
        let blinding_proof = BlindingProof::prove(&commitment, &value, &blinding, &nonce);
        debug_assert_eq!(value, {
            let secret_image = encode_point(&(crate::crypto::g_generator() * note.opening().secret));
            hash_to_scalar(
                b"TigerSmartChain/Privacy/NoteValue/v1",
                &[&note.denomination().to_le_bytes(), &secret_image],
            )
        });

        Ok(Spend {
            merkle_path,
            leaf_index,
            commitment,
            blinding_proof,
            nullifier: note.nullifier(),
            denomination: note.denomination(),
            secret: note.opening().secret,
            blinding,
        })
    }

    /// Verify a withdrawal against this pool's root.
    pub fn verify_spend(&self, spend: &Spend) -> Result<(), PrivacyError> {
        // 1. The commitment must be in the tree.
        if !CommitmentTree::verify_proof(
            &leaf_hash(&spend.commitment),
            spend.leaf_index,
            &spend.merkle_path,
            &self.tree.root(),
        ) {
            return Err(PrivacyError::InvalidProof(
                "merkle path does not prove membership".to_string(),
            ));
        }

        // 2. The value scalar is recomputed from the published denomination and
        //    secret, so the spender cannot publish a value of their choosing.
        let value = spend.value();

        // 3. The commitment must open to exactly that value and blinding.
        if Commitment::new(&value, &spend.blinding).0 != spend.commitment.0 {
            return Err(PrivacyError::InvalidProof(
                "commitment does not open to the published denomination, secret and blinding"
                    .to_string(),
            ));
        }

        // 4. The spender must know the blinding factor.
        if !spend.blinding_proof.verify(&spend.commitment, &value) {
            return Err(PrivacyError::InvalidProof(
                "blinding proof does not verify".to_string(),
            ));
        }

        // 5. The nullifier must be the one this note secret produces.
        let expected_nullifier = {
            let point = crate::crypto::g_generator() * spend.secret;
            let mut out = [0u8; 32];
            out.copy_from_slice(&Sha256::digest(encode_point(&point)));
            out
        };
        if spend.nullifier != expected_nullifier {
            return Err(PrivacyError::InvalidProof(
                "nullifier is not bound to the published note secret".to_string(),
            ));
        }

        if self.nullifiers.contains_key(&spend.nullifier) {
            return Err(PrivacyError::NullifierExists(hex_encode(&spend.nullifier)));
        }

        Ok(())
    }

    /// Withdraw a verified note to `recipient`.
    pub fn withdraw(&mut self, spend: &Spend, recipient: &str) -> Result<String, PrivacyError> {
        self.verify_spend(spend)?;

        self.nullifiers.insert(spend.nullifier, now_unix());
        self.stats.total_withdrawals += 1;
        self.stats.active_notes = self.stats.active_notes.saturating_sub(1);

        Ok(recipient.to_string())
    }

    /// Whether a nullifier has been spent.
    pub fn is_spent(&self, nullifier: &[u8; 32]) -> bool {
        self.nullifiers.contains_key(nullifier)
    }

    /// Check that the sum of input commitments equals the sum of output
    /// commitments.
    ///
    /// This is the homomorphic balance check a confidential-transaction
    /// verifier performs: it needs no amounts and no blindings, because
    /// Pedersen commitments add.
    pub fn check_balance(
        inputs: &[Commitment],
        outputs: &[Commitment],
    ) -> Result<(), PrivacyError> {
        if inputs.is_empty() || outputs.is_empty() {
            return Err(PrivacyError::BalanceMismatch);
        }

        let sum = |set: &[Commitment]| {
            set.iter().fold(Commitment(RistrettoPoint::identity()), |acc, c| {
                acc.add(c)
            })
        };

        if sum(inputs).0 != sum(outputs).0 {
            return Err(PrivacyError::BalanceMismatch);
        }

        Ok(())
    }
}

impl Default for ShieldedPool {
    fn default() -> Self {
        Self::new(16)
    }
}

/// Minimal lowercase hex encoder, so this crate needs no hex dependency.
pub fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(denomination: u64) -> Note {
        Note::new(denomination, Scalar::random(&mut OsRng)).unwrap()
    }

    #[test]
    fn a_deposited_note_can_be_spent_once() {
        let mut pool = ShieldedPool::new(8);
        let note = note(100);
        let (index, _) = pool.deposit(&note).unwrap();

        let spend = pool.build_spend(&note, index).unwrap();
        assert!(pool.verify_spend(&spend).is_ok());
        assert!(pool.withdraw(&spend, "0xrecipient").is_ok());
        assert!(pool.is_spent(&spend.nullifier));
    }

    #[test]
    fn the_same_note_cannot_be_spent_twice() {
        let mut pool = ShieldedPool::new(8);
        let note = note(100);
        let (index, _) = pool.deposit(&note).unwrap();

        let spend = pool.build_spend(&note, index).unwrap();
        pool.withdraw(&spend, "0xa").unwrap();

        assert!(matches!(
            pool.withdraw(&spend, "0xb"),
            Err(PrivacyError::NullifierExists(_))
        ));
    }

    #[test]
    fn a_spend_for_the_wrong_leaf_index_is_rejected() {
        let mut pool = ShieldedPool::new(8);
        let a = note(100);
        let b = note(100);
        let (index_a, _) = pool.deposit(&a).unwrap();
        pool.deposit(&b).unwrap();

        let mut spend = pool.build_spend(&a, index_a).unwrap();
        spend.leaf_index = 1;

        assert!(matches!(
            pool.verify_spend(&spend),
            Err(PrivacyError::InvalidProof(_))
        ));
    }

    #[test]
    fn a_spend_claiming_a_different_value_is_rejected() {
        let mut pool = ShieldedPool::new(8);
        let note = note(100);
        let (index, _) = pool.deposit(&note).unwrap();

        let mut spend = pool.build_spend(&note, index).unwrap();
        spend.denomination = 999;

        assert!(matches!(
            pool.verify_spend(&spend),
            Err(PrivacyError::InvalidProof(_))
        ));
    }

    #[test]
    fn a_forged_blinding_proof_is_rejected() {
        let mut pool = ShieldedPool::new(8);
        let note = note(100);
        let (index, _) = pool.deposit(&note).unwrap();

        let mut spend = pool.build_spend(&note, index).unwrap();
        spend.blinding_proof.response[0] ^= 0x01;

        assert!(matches!(
            pool.verify_spend(&spend),
            Err(PrivacyError::InvalidProof(_))
        ));
    }

    #[test]
    fn a_nullifier_not_bound_to_the_secret_is_rejected() {
        let mut pool = ShieldedPool::new(8);
        let note = note(100);
        let (index, _) = pool.deposit(&note).unwrap();

        let mut spend = pool.build_spend(&note, index).unwrap();
        spend.nullifier = [0xabu8; 32];

        assert!(matches!(
            pool.verify_spend(&spend),
            Err(PrivacyError::InvalidProof(_))
        ));
    }

    #[test]
    fn a_spend_from_an_empty_pool_is_rejected() {
        let pool = ShieldedPool::new(8);
        let note = note(100);
        assert!(pool.build_spend(&note, 0).is_err());
    }

    #[test]
    fn the_full_opening_round_trips_through_encryption() {
        let secret_key = Scalar::random(&mut OsRng);
        let public = encode_point(&(crate::crypto::g_generator() * secret_key));
        let note = Note::new(250, secret_key).unwrap();

        let encrypted = note.encrypt_opening(&public).unwrap();
        let recovered = Note::decrypt_opening(&encrypted, secret_key).unwrap();

        assert_eq!(recovered.denomination(), 250);
        assert_eq!(recovered.commitment(), note.commitment());
        assert_eq!(recovered.nullifier(), note.nullifier());
        assert_eq!(recovered.opening().blinding, note.opening().blinding);
    }

    #[test]
    fn a_note_cannot_be_decrypted_with_the_wrong_key() {
        let secret_key = Scalar::random(&mut OsRng);
        let public = encode_point(&(crate::crypto::g_generator() * secret_key));
        let note = Note::new(250, secret_key).unwrap();
        let encrypted = note.encrypt_opening(&public).unwrap();

        let wrong = Scalar::random(&mut OsRng);
        assert!(matches!(
            Note::decrypt_opening(&encrypted, wrong),
            Err(PrivacyError::EncryptionError(_))
        ));
    }

    #[test]
    fn a_tampered_ciphertext_is_rejected() {
        let secret_key = Scalar::random(&mut OsRng);
        let public = encode_point(&(crate::crypto::g_generator() * secret_key));
        let note = Note::new(250, secret_key).unwrap();
        let mut encrypted = note.encrypt_opening(&public).unwrap();

        encrypted.ciphertext[0] ^= 0x01;

        assert!(matches!(
            Note::decrypt_opening(&encrypted, secret_key),
            Err(PrivacyError::EncryptionError(_))
        ));
    }

    #[test]
    fn two_notes_for_one_holder_differ() {
        let secret_key = Scalar::random(&mut OsRng);
        let a = Note::new(100, secret_key).unwrap();
        let b = Note::new(100, secret_key).unwrap();

        assert_ne!(a.commitment(), b.commitment());
        assert_ne!(a.nullifier(), b.nullifier());
    }

    #[test]
    fn a_zero_denomination_note_is_rejected() {
        let secret_key = Scalar::random(&mut OsRng);
        assert!(matches!(
            Note::new(0, secret_key),
            Err(PrivacyError::InvalidNote(_))
        ));
    }

    #[test]
    fn a_zero_secret_key_is_rejected() {
        assert!(matches!(
            Note::new(100, Scalar::ZERO),
            Err(PrivacyError::InvalidNote(_))
        ));
    }

    #[test]
    fn balance_check_accepts_equal_sums_and_rejects_unequal_ones() {
        let blinding = Scalar::random(&mut OsRng);
        let value = Scalar::random(&mut OsRng);
        let c = Commitment::new(&value, &blinding);

        assert!(ShieldedPool::check_balance(&[c.clone()], &[c.clone()]).is_ok());

        let other = Commitment::new(&(value + Scalar::ONE), &blinding);
        assert!(matches!(
            ShieldedPool::check_balance(&[c], &[other]),
            Err(PrivacyError::BalanceMismatch)
        ));
    }

    #[test]
    fn balance_check_rejects_an_empty_side() {
        let blinding = Scalar::random(&mut OsRng);
        let value = Scalar::random(&mut OsRng);
        let c = Commitment::new(&value, &blinding);

        assert!(matches!(
            ShieldedPool::check_balance(&[c.clone()], &[]),
            Err(PrivacyError::BalanceMismatch)
        ));
    }
}
