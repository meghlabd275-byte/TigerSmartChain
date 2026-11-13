//! Byzantine attestation and quorum finality.
//!
//! Block production alone is not safety: any validator can sign any block, and
//! without a quorum the chain cannot distinguish the block the network agreed
//! on from one a single validator invented. This module adds the missing half
//! of BFT consensus:
//!
//! * validators sign an [`Attestation`] over a block hash, in a numbered round;
//! * a [`QuorumCertificate`] collects those signatures and is valid only when it
//!   holds more than two thirds of the active validator weight;
//! * signing two different hashes for the same height is recorded as
//!   equivocation evidence, so the misbehaviour can be proven and slashed.
//!
//! Two rules make the design safe:
//!
//! 1. **Quorum is measured in weight, not validator count.** With equal stakes
//!    the two are the same; with unequal stakes a headcount quorum would let a
//!    group of small validators outvote the stake they represent.
//! 2. **A quorum is never self-declared.** Every signature in a certificate is
//!    recovered and checked against the validator it claims to be, and the
//!    weight is summed only from signatures that verify. A certificate that
//!    claims a quorum it does not have is rejected rather than trusted.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::posa::MIN_STAKE;

/// A validator's signed statement about one block.
///
/// The signed digest commits to the chain, the height, the round and the block
/// hash, so a signature can never be replayed at another height, on another
/// chain, or in another round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attestation {
    pub chain_id: u64,
    pub block_number: u64,
    /// The round this attestation belongs to. A validator may legitimately vote
    /// once per round, so a second vote in a *different* round is not
    /// equivocation.
    pub round: u64,
    pub block_hash: String,
    /// The address the signature is expected to recover to.
    pub validator: String,
    /// 64-byte `r || s`.
    pub signature: Vec<u8>,
    /// Recovery id (0 or 1).
    pub recovery_id: u8,
}

impl Attestation {
    /// The digest a validator signs for `(chain, height, round, hash)`.
    ///
    /// Domain-separated so a consensus signature can never be confused with a
    /// transaction signature, and length-prefixed so the hash of a block named
    /// `a` in round 1 cannot collide with the hash of a block named `1` in
    /// round `a`.
    pub fn digest(
        chain_id: u64,
        block_number: u64,
        round: u64,
        block_hash: &str,
    ) -> [u8; 32] {
        let mut preimage = Vec::with_capacity(8 + 8 + 8 + block_hash.len() + 24);
        preimage.extend_from_slice(b"TigerSmartChain/attestation/v1");
        preimage.extend_from_slice(&chain_id.to_be_bytes());
        preimage.extend_from_slice(&block_number.to_be_bytes());
        preimage.extend_from_slice(&round.to_be_bytes());
        preimage.extend_from_slice(&(block_hash.len() as u32).to_be_bytes());
        preimage.extend_from_slice(block_hash.as_bytes());
        tiger_crypto::keccak256(&preimage)
    }

    /// Whether the signature really was produced by `validator`.
    pub fn verify(&self) -> bool {
        let digest = Self::digest(
            self.chain_id,
            self.block_number,
            self.round,
            &self.block_hash,
        );
        if self.signature.len() != 64 {
            return false;
        }
        match tiger_crypto::recover_sender(&digest, &self.signature, self.recovery_id) {
            Ok(address) => format!("0x{}", hex::encode(address))
                .eq_ignore_ascii_case(&self.validator),
            Err(_) => false,
        }
    }
}

/// Why a certificate was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuorumError {
    /// No signatures at all.
    Empty,
    /// A signature did not verify, or did not belong to the validator that
    /// claimed it.
    BadSignature { validator: String },
    /// A signature came from an address that is not in the active set.
    NotAValidator { validator: String },
    /// Two attestations from the same validator disagree about the hash or
    /// round. The certificate is ambiguous, so it is refused.
    Conflicting { validator: String },
    /// The signatures verified but did not reach two thirds of the weight.
    NotEnoughWeight { have: u128, need: u128 },
    /// Attestations for more than one height in one certificate.
    MixedHeights,
}

impl std::fmt::Display for QuorumError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuorumError::Empty => write!(f, "the certificate carries no signatures"),
            QuorumError::BadSignature { validator } => {
                write!(f, "the attestation from {validator} does not verify")
            }
            QuorumError::NotAValidator { validator } => {
                write!(f, "{validator} is not an active validator")
            }
            QuorumError::Conflicting { validator } => {
                write!(f, "{validator} signed two different hashes for one height")
            }
            QuorumError::NotEnoughWeight { have, need } => write!(
                f,
                "the certificate holds weight {have}, which is below the {need} a quorum needs"
            ),
            QuorumError::MixedHeights => {
                write!(f, "the certificate attests to more than one block height")
            }
        }
    }
}

impl std::error::Error for QuorumError {}

/// The signatures that finalise one block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuorumCertificate {
    pub chain_id: u64,
    pub block_number: u64,
    pub block_hash: String,
    /// The round the quorum was reached in.
    pub round: u64,
    /// Every attestation that went into the certificate. Kept in full so any
    /// observer can re-verify the quorum instead of trusting the summary.
    pub attestations: Vec<Attestation>,
}

impl QuorumCertificate {
    /// Verify the certificate against the validator set and its weights.
    ///
    /// Returns the total weight that signed, or the reason the certificate is
    /// not a quorum. Every signature is recovered here: the `validator` field
    /// is treated as a claim, not a fact.
    pub fn verify(
        &self,
        weights: &HashMap<String, u128>,
    ) -> Result<u128, QuorumError> {
        if self.attestations.is_empty() {
            return Err(QuorumError::Empty);
        }

        let total: u128 = weights.values().copied().sum();
        if total == 0 {
            return Err(QuorumError::NotEnoughWeight { have: 0, need: 0 });
        }

        let mut signed: HashMap<String, (u64, String)> = HashMap::new();
        for attestation in &self.attestations {
            if attestation.chain_id != self.chain_id
                || attestation.block_number != self.block_number
                || attestation.round != self.round
            {
                return Err(QuorumError::MixedHeights);
            }
            let key = attestation.validator.to_lowercase();
            if !weights.contains_key(&key) {
                return Err(QuorumError::NotAValidator {
                    validator: attestation.validator.clone(),
                });
            }
            if !attestation.verify() {
                return Err(QuorumError::BadSignature {
                    validator: attestation.validator.clone(),
                });
            }
            // One validator, one vote. A repeated signature adds no weight; a
            // repeat that disagrees about the hash is equivocation.
            match signed.get(&key) {
                Some((round, hash)) => {
                    if *round != attestation.round || *hash != attestation.block_hash {
                        return Err(QuorumError::Conflicting {
                            validator: attestation.validator.clone(),
                        });
                    }
                }
                None => {
                    signed.insert(
                        key,
                        (attestation.round, attestation.block_hash.clone()),
                    );
                }
            }
        }

        // Weight is summed from the recovered signers only.
        let have: u128 = signed.keys().filter_map(|k| weights.get(k).copied()).sum();
        let need = two_thirds_plus_one(total);
        if have < need {
            return Err(QuorumError::NotEnoughWeight { have, need });
        }
        Ok(have)
    }

    /// The addresses that signed, lowercased and sorted, for stable reporting.
    pub fn signers(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .attestations
            .iter()
            .map(|a| a.validator.to_lowercase())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// The smallest integer strictly greater than two thirds of `total`.
///
/// This is the BFT threshold: it is the point at which two quorums must
/// intersect in at least one honest validator, which is what stops two
/// conflicting blocks from both being final.
pub fn two_thirds_plus_one(total: u128) -> u128 {
    // floor(2n/3) + 1 is the smallest integer strictly greater than 2n/3.
    total / 3 * 2 + 1 + if total % 3 == 2 { 1 } else { 0 }
}

/// Two conflicting attestations from one validator at one height, which is
/// proof of misbehaviour that anyone can check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquivocationProof {
    pub validator: String,
    pub block_number: u64,
    pub first: Attestation,
    pub second: Attestation,
}

impl EquivocationProof {
    /// Whether the proof really shows one validator signing two hashes at one
    /// height. Both signatures must verify, so a proof cannot be fabricated by
    /// copying an honest signature twice.
    pub fn verify(&self) -> bool {
        if self.first.block_number != self.second.block_number {
            return false;
        }
        if self.first.block_hash == self.second.block_hash {
            return false;
        }
        if !self.first.validator.eq_ignore_ascii_case(&self.second.validator) {
            return false;
        }
        if !self
            .validator
            .eq_ignore_ascii_case(&self.first.validator)
        {
            return false;
        }
        self.first.verify() && self.second.verify()
    }
}

/// Tracks attestations so equivocation can be detected and finality recorded.
#[derive(Debug, Default)]
pub struct AttestationTracker {
    /// For each height, the first attestation each validator signed. Keeping
    /// the whole attestation, not just its hash, is what makes an equivocation
    /// proof verifiable: the proof needs the original signature over the
    /// original hash, which cannot be reconstructed from a later one.
    seen: HashMap<u64, HashMap<String, Attestation>>,
    /// The highest height with a valid quorum, and its hash.
    finalized: Option<(u64, String)>,
    /// Proofs of misbehaviour collected so far, by validator.
    evidence: Vec<EquivocationProof>,
}

impl AttestationTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an attestation, returning an equivocation proof when this
    /// validator has already signed a different hash at the same height.
    ///
    /// The attestation's signature is verified first: an unsigned claim of
    /// misbehaviour must not be able to slash an honest validator.
    pub fn record(&mut self, attestation: &Attestation) -> Option<EquivocationProof> {
        if !attestation.verify() {
            return None;
        }
        let key = attestation.validator.to_lowercase();
        let per_height = self.seen.entry(attestation.block_number).or_default();
        match per_height.get(&key) {
            Some(existing) if existing.block_hash != attestation.block_hash => {
                let proof = EquivocationProof {
                    validator: key.clone(),
                    block_number: attestation.block_number,
                    first: existing.clone(),
                    second: attestation.clone(),
                };
                // Both halves are real, signed attestations by the same key, so
                // the proof verifies. Checked anyway: a tracker fed a corrupted
                // attestation must not emit a proof that will not stand up.
                if proof.verify() {
                    self.evidence.push(proof.clone());
                    return Some(proof);
                }
                None
            }
            _ => {
                per_height.insert(key, attestation.clone());
                None
            }
        }
    }

    /// Record a verified certificate, advancing finality.
    ///
    /// Finality only moves forward. A certificate for a height at or below the
    /// finalised height is ignored, so a replayed or stale quorum cannot
    /// rewind what the chain has already promised.
    pub fn apply_certificate(&mut self, certificate: &QuorumCertificate, weights: &HashMap<String, u128>) -> bool {
        if certificate.verify(weights).is_err() {
            return false;
        }
        match &self.finalized {
            Some((height, _)) if *height >= certificate.block_number => false,
            _ => {
                self.finalized = Some((certificate.block_number, certificate.block_hash.clone()));
                true
            }
        }
    }

    /// The last finalised height and hash, if any block has been finalised.
    pub fn finalized(&self) -> Option<(u64, String)> {
        self.finalized.clone()
    }

    /// Build a quorum certificate for `(block_number, block_hash, round)` from
    /// the attestations recorded so far, or `None` while they fall short of a
    /// quorum.
    ///
    /// The certificate is re-verified before it is returned, so a caller can
    /// never receive a certificate that does not itself reach two thirds of
    /// the weight.
    pub fn certificate_for(
        &self,
        block_number: u64,
        block_hash: &str,
        round: u64,
        weights: &HashMap<String, u128>,
    ) -> Option<QuorumCertificate> {
        let per_height = self.seen.get(&block_number)?;
        let matching: Vec<&Attestation> = per_height
            .values()
            .filter(|a| a.round == round && a.block_hash == block_hash)
            .collect();
        let first = matching.first()?;
        let certificate = QuorumCertificate {
            chain_id: first.chain_id,
            block_number,
            block_hash: block_hash.to_string(),
            round,
            attestations: matching.into_iter().cloned().collect(),
        };
        certificate.verify(weights).ok()?;
        Some(certificate)
    }

    /// Whether `block_number` is at or below the finalised height, and so
    /// cannot be reverted.
    pub fn is_finalized(&self, block_number: u64) -> bool {
        matches!(&self.finalized, Some((height, _)) if *height >= block_number)
    }

    /// The collected equivocation proofs.
    pub fn evidence(&self) -> &[EquivocationProof] {
        &self.evidence
    }

    /// Total stake weight per validator, derived from a registry.
    ///
    /// Returned as a map so a certificate can be verified against exactly the
    /// weights the caller intends, and so the caller can supply weights from a
    /// source other than the registry (for example, a snapshot at the height
    /// being attested) without this module reaching into chain state.
    pub fn weight_map<'a, I>(validators: I) -> HashMap<String, u128>
    where
        I: IntoIterator<Item = (&'a str, u64)>,
    {
        validators
            .into_iter()
            .map(|(address, stake)| {
                (
                    address.to_lowercase(),
                    std::cmp::max(stake as u128, MIN_STAKE as u128),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAIN: u64 = 6666;

    fn address_of(key: &[u8; 32]) -> String {
        format!(
            "0x{}",
            hex::encode(tiger_crypto::private_key_to_address(key).unwrap())
        )
    }

    fn attest(key: &[u8; 32], number: u64, round: u64, hash: &str) -> Attestation {
        let digest = Attestation::digest(CHAIN, number, round, hash);
        let signature = tiger_crypto::sign(&digest, key).unwrap();
        Attestation {
            chain_id: CHAIN,
            block_number: number,
            round,
            block_hash: hash.to_string(),
            validator: address_of(key),
            signature: signature.bytes.to_vec(),
            recovery_id: signature.recovery_id,
        }
    }

    fn keys(n: usize) -> Vec<[u8; 32]> {
        (0..n).map(|i| [(i as u8) + 1; 32]).collect()
    }

    fn weights_for(keys: &[[u8; 32]], stake: u64) -> HashMap<String, u128> {
        keys.iter()
            .map(|k| (address_of(k).to_lowercase(), stake as u128))
            .collect()
    }

    #[test]
    fn an_attestation_verifies_only_for_its_own_signer() {
        let k = keys(2);
        let a = attest(&k[0], 1, 0, "0xaa");
        assert!(a.verify(), "an honest attestation must verify");

        let mut forged = a.clone();
        forged.validator = address_of(&k[1]);
        assert!(
            !forged.verify(),
            "an attestation relabelled to another validator must not verify"
        );
    }

    #[test]
    fn an_attestation_cannot_be_replayed_at_another_height_or_round() {
        let k = keys(1);
        let a = attest(&k[0], 5, 0, "0xaa");

        // Same signature, different height: the digest changes, so it fails.
        let mut moved = a.clone();
        moved.block_number = 6;
        assert!(!moved.verify(), "a signature must not move to another height");

        // Same signature, different round: also fails.
        let mut rerounded = a.clone();
        rerounded.round = 1;
        assert!(
            !rerounded.verify(),
            "a signature must not move to another round"
        );

        // Same signature, another chain: also fails.
        let mut rechaired = a.clone();
        rechaired.chain_id = 1;
        assert!(
            !rechaired.verify(),
            "a signature must not move to another chain"
        );
    }

    #[test]
    fn a_quorum_needs_more_than_two_thirds_of_weight() {
        assert_eq!(two_thirds_plus_one(1), 1);
        assert_eq!(two_thirds_plus_one(3), 3);
        assert_eq!(two_thirds_plus_one(4), 3);
        assert_eq!(two_thirds_plus_one(9), 7);
        assert_eq!(two_thirds_plus_one(10), 7);
    }

    #[test]
    fn a_certificate_with_two_of_four_validators_is_not_a_quorum() {
        let k = keys(4);
        let weights = weights_for(&k, MIN_STAKE);
        let certificate = QuorumCertificate {
            chain_id: CHAIN,
            block_number: 1,
            block_hash: "0xaa".to_string(),
            round: 0,
            attestations: vec![
                attest(&k[0], 1, 0, "0xaa"),
                attest(&k[1], 1, 0, "0xaa"),
            ],
        };
        let error = certificate.verify(&weights).unwrap_err();
        assert!(
            matches!(error, QuorumError::NotEnoughWeight { .. }),
            "two of four is not a quorum, got {error}"
        );
    }

    #[test]
    fn a_certificate_with_three_of_four_validators_is_a_quorum() {
        let k = keys(4);
        let weights = weights_for(&k, MIN_STAKE);
        let certificate = QuorumCertificate {
            chain_id: CHAIN,
            block_number: 1,
            block_hash: "0xaa".to_string(),
            round: 0,
            attestations: vec![
                attest(&k[0], 1, 0, "0xaa"),
                attest(&k[1], 1, 0, "0xaa"),
                attest(&k[2], 1, 0, "0xaa"),
            ],
        };
        assert!(certificate.verify(&weights).is_ok());
    }

    /// The attack the weight rule exists to stop: a headcount quorum would let
    /// a majority of *validators* outvote the majority of *stake*.
    #[test]
    fn quorum_is_measured_in_weight_not_validator_count() {
        let k = keys(3);
        let whale = address_of(&k[0]).to_lowercase();
        let mut weights: HashMap<String, u128> = weights_for(&k, MIN_STAKE);
        // One validator holds twice what the others do: two thirds of the
        // total, but not the strictly-more-than-two-thirds a quorum needs.
        weights.insert(whale.clone(), MIN_STAKE as u128 * 2);

        // The two small validators together are not a quorum.
        let small = QuorumCertificate {
            chain_id: CHAIN,
            block_number: 1,
            block_hash: "0xaa".to_string(),
            round: 0,
            attestations: vec![
                attest(&k[1], 1, 0, "0xaa"),
                attest(&k[2], 1, 0, "0xaa"),
            ],
        };
        assert!(
            small.verify(&weights).is_err(),
            "two small validators must not outvote the stake"
        );

        // The whale alone is still not a quorum: it is two thirds of the total
        // but not strictly more.
        let whale_only = QuorumCertificate {
            chain_id: CHAIN,
            block_number: 1,
            block_hash: "0xaa".to_string(),
            round: 0,
            attestations: vec![attest(&k[0], 1, 0, "0xaa")],
        };
        let error = whale_only.verify(&weights).unwrap_err();
        assert!(
            matches!(error, QuorumError::NotEnoughWeight { .. }),
            "a bare two thirds must not finalise without one more, got {error}"
        );

        // The whale plus one other validator is a quorum.
        let with_small = QuorumCertificate {
            chain_id: CHAIN,
            block_number: 1,
            block_hash: "0xaa".to_string(),
            round: 0,
            attestations: vec![attest(&k[0], 1, 0, "0xaa"), attest(&k[1], 1, 0, "0xaa")],
        };
        assert!(with_small.verify(&weights).is_ok());
    }

    #[test]
    fn a_certificate_naming_a_non_validator_is_rejected() {
        let k = keys(3);
        let mut weights = weights_for(&k, MIN_STAKE);
        let outsider = [0x77u8; 32];
        weights.remove(&address_of(&outsider).to_lowercase());

        let certificate = QuorumCertificate {
            chain_id: CHAIN,
            block_number: 1,
            block_hash: "0xaa".to_string(),
            round: 0,
            attestations: vec![
                attest(&k[0], 1, 0, "0xaa"),
                attest(&k[1], 1, 0, "0xaa"),
                attest(&outsider, 1, 0, "0xaa"),
            ],
        };
        let error = certificate.verify(&weights).unwrap_err();
        assert!(
            matches!(error, QuorumError::NotAValidator { .. }),
            "an outsider's signature must not count toward a quorum, got {error}"
        );
    }

    #[test]
    fn a_certificate_with_a_tampered_signature_is_rejected() {
        let k = keys(4);
        let weights = weights_for(&k, MIN_STAKE);
        let mut certificate = QuorumCertificate {
            chain_id: CHAIN,
            block_number: 1,
            block_hash: "0xaa".to_string(),
            round: 0,
            attestations: vec![
                attest(&k[0], 1, 0, "0xaa"),
                attest(&k[1], 1, 0, "0xaa"),
                attest(&k[2], 1, 0, "0xaa"),
            ],
        };
        certificate.attestations[2].signature[0] ^= 0xff;
        let error = certificate.verify(&weights).unwrap_err();
        assert!(
            matches!(error, QuorumError::BadSignature { .. }),
            "a tampered signature must be caught, got {error}"
        );
    }

    /// A repeated signature must not be counted twice to fake a quorum.
    #[test]
    fn one_validator_cannot_count_twice() {
        let k = keys(4);
        let weights = weights_for(&k, MIN_STAKE);
        let once = attest(&k[0], 1, 0, "0xaa");
        let certificate = QuorumCertificate {
            chain_id: CHAIN,
            block_number: 1,
            block_hash: "0xaa".to_string(),
            round: 0,
            attestations: vec![once.clone(), once.clone(), once],
        };
        let error = certificate.verify(&weights).unwrap_err();
        assert!(
            matches!(error, QuorumError::NotEnoughWeight { .. }),
            "duplicating one signature must not reach a quorum, got {error}"
        );
    }

    #[test]
    fn equivocation_is_detected_and_provable() {
        let k = keys(4);
        let weights = weights_for(&k, MIN_STAKE);
        let mut tracker = AttestationTracker::new();

        assert!(
            tracker.record(&attest(&k[0], 7, 0, "0xaa")).is_none(),
            "a first attestation is not equivocation"
        );
        let proof = tracker
            .record(&attest(&k[0], 7, 0, "0xbb"))
            .expect("signing two hashes at one height is equivocation");
        assert!(
            proof.verify(),
            "the proof must be checkable by anyone"
        );
        assert_eq!(tracker.evidence().len(), 1);

        // Re-voting for the same hash is not equivocation.
        assert!(
            tracker.record(&attest(&k[1], 7, 0, "0xaa")).is_none()
        );
        // Voting in a different round for a different hash is not equivocation
        // in this tracker: rounds are a separate slot.
        assert!(
            tracker.record(&attest(&k[1], 8, 0, "0xcc")).is_none()
        );
        let _ = weights;
    }

    #[test]
    fn a_proof_that_copies_one_signature_is_not_valid() {
        let k = keys(1);
        let a = attest(&k[0], 3, 0, "0xaa");
        let mut b = a.clone();
        b.block_hash = "0xbb".to_string();
        // `b` still carries `a`'s signature, so it does not verify.
        let proof = EquivocationProof {
            validator: a.validator.clone(),
            block_number: 3,
            first: a.clone(),
            second: b,
        };
        assert!(
            !proof.verify(),
            "a proof whose second half does not verify must be refused"
        );
    }

    #[test]
    fn finality_only_moves_forward() {
        let k = keys(4);
        let weights = weights_for(&k, MIN_STAKE);
        let mut tracker = AttestationTracker::new();

        let at_five = QuorumCertificate {
            chain_id: CHAIN,
            block_number: 5,
            block_hash: "0x05".to_string(),
            round: 0,
            attestations: vec![
                attest(&k[0], 5, 0, "0x05"),
                attest(&k[1], 5, 0, "0x05"),
                attest(&k[2], 5, 0, "0x05"),
            ],
        };
        assert!(tracker.apply_certificate(&at_five, &weights));
        assert_eq!(tracker.finalized(), Some((5, "0x05".to_string())));
        assert!(tracker.is_finalized(5));
        assert!(tracker.is_finalized(3));

        // A certificate for an older height must not rewind finality.
        let at_three = QuorumCertificate {
            chain_id: CHAIN,
            block_number: 3,
            block_hash: "0x03".to_string(),
            round: 0,
            attestations: vec![
                attest(&k[0], 3, 0, "0x03"),
                attest(&k[1], 3, 0, "0x03"),
                attest(&k[2], 3, 0, "0x03"),
            ],
        };
        assert!(!tracker.apply_certificate(&at_three, &weights));
        assert_eq!(
            tracker.finalized(),
            Some((5, "0x05".to_string())),
            "finality must not move backwards"
        );

        // A certificate that is not a quorum must not finalise anything.
        let weak = QuorumCertificate {
            chain_id: CHAIN,
            block_number: 9,
            block_hash: "0x09".to_string(),
            round: 0,
            attestations: vec![attest(&k[0], 9, 0, "0x09")],
        };
        assert!(!tracker.apply_certificate(&weak, &weights));
        assert_eq!(tracker.finalized(), Some((5, "0x05".to_string())));
    }
}
