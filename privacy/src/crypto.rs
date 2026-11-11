//! Cryptographic primitives for the shielded pool.
//!
//! Everything here is real curve arithmetic on Ristretto255, not a stand-in.
//! The previous version represented a point as the two strings `"1"` and `"2"`
//! and implemented addition by formatting them into `"1 + 1"`, so no
//! arithmetic was performed at all.

use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
use curve25519_dalek::scalar::Scalar;
use sha2::{Digest, Sha512};

/// Length of a compressed Ristretto point, in bytes.
pub const POINT_LEN: usize = 32;

/// Length of a scalar, in bytes.
pub const SCALAR_LEN: usize = 32;

/// The basepoint.
pub fn g_generator() -> RistrettoPoint {
    RISTRETTO_BASEPOINT_POINT
}

/// The second generator of the Pedersen commitment scheme.
///
/// It is derived by hashing a fixed label to the curve, so nobody knows its
/// discrete log with respect to the basepoint. That is what makes the scheme
/// binding: a committer cannot open the same commitment to two different
/// value/blinding pairs.
pub fn h_generator() -> RistrettoPoint {
    RistrettoPoint::hash_from_bytes::<Sha512>(b"TigerSmartChain/Privacy/PedersenH/v1")
}

/// Encode a point as 32 bytes.
pub fn encode_point(point: &RistrettoPoint) -> [u8; POINT_LEN] {
    point.compress().to_bytes()
}

/// Decode a point, rejecting any encoding that is not a valid Ristretto point.
pub fn decode_point(bytes: &[u8]) -> Result<RistrettoPoint, String> {
    if bytes.len() != POINT_LEN {
        return Err(format!("point must be {POINT_LEN} bytes, got {}", bytes.len()));
    }
    let compressed = CompressedRistretto::from_slice(bytes)
        .map_err(|e| format!("invalid point encoding: {e}"))?;
    compressed
        .decompress()
        .ok_or_else(|| "point encoding is not a valid Ristretto point".to_string())
}

/// Encode a scalar canonically.
pub fn encode_scalar(scalar: &Scalar) -> [u8; SCALAR_LEN] {
    scalar.to_bytes()
}

/// Decode a scalar, rejecting non-canonical encodings (values at or above the
/// group order).
pub fn decode_scalar(bytes: &[u8]) -> Result<Scalar, String> {
    if bytes.len() != SCALAR_LEN {
        return Err(format!("scalar must be {SCALAR_LEN} bytes, got {}", bytes.len()));
    }
    let mut array = [0u8; SCALAR_LEN];
    array.copy_from_slice(bytes);
    Option::<Scalar>::from(Scalar::from_canonical_bytes(array))
        .ok_or_else(|| "scalar is not canonical".to_string())
}

/// Reduce arbitrary bytes to a scalar.
pub fn hash_to_scalar(domain: &[u8], parts: &[&[u8]]) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&hasher.finalize());
    Scalar::from_bytes_mod_order_wide(&wide)
}

/// A Pedersen commitment `value * G + blinding * H`, where `value` is itself a
/// scalar derived from the note's fields.
///
/// Because the value is a full scalar, a commitment binds the whole note
/// opening — the denomination *and* the owner's public key — not just the
/// amount. That is what stops anyone but the note's owner from spending it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commitment(pub RistrettoPoint);

impl Commitment {
    /// Commit to a value scalar with a blinding factor.
    pub fn new(value: &Scalar, blinding: &Scalar) -> Self {
        Commitment(g_generator() * value + h_generator() * blinding)
    }

    /// The compressed encoding.
    pub fn to_bytes(&self) -> [u8; POINT_LEN] {
        encode_point(&self.0)
    }

    /// Decode a commitment.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        Ok(Commitment(decode_point(bytes)?))
    }

    /// Homomorphic addition: `commit(a) + commit(b) == commit(a + b)` when the
    /// blindings add too. This is the property that lets a verifier check that
    /// inputs equal outputs without learning any amount.
    pub fn add(&self, other: &Commitment) -> Commitment {
        Commitment(self.0 + other.0)
    }
}

/// A Schnorr proof of knowledge of the blinding factor of a commitment.
///
/// Proves the prover knows `r` such that `C = value * G + r * H` for a stated
/// value. Made non-interactive with Fiat-Shamir over the commitment, the value
/// and the announcement, so a proof cannot be replayed against another
/// statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlindingProof {
    /// `R = k * H`.
    pub announce: [u8; POINT_LEN],
    /// `s = k + e * r`.
    pub response: [u8; SCALAR_LEN],
}

impl BlindingProof {
    /// Prove knowledge of `blinding` for a commitment to `value`.
    pub fn prove(
        commitment: &Commitment,
        value: &Scalar,
        blinding: &Scalar,
        nonce: &Scalar,
    ) -> BlindingProof {
        let announce = h_generator() * nonce;
        let challenge = Self::challenge(commitment, value, &announce);
        let response = nonce + challenge * blinding;

        BlindingProof {
            announce: encode_point(&announce),
            response: encode_scalar(&response),
        }
    }

    /// Verify against the commitment and the value it should hold.
    pub fn verify(&self, commitment: &Commitment, value: &Scalar) -> bool {
        let announce = match decode_point(&self.announce) {
            Ok(p) => p,
            Err(_) => return false,
        };
        let response = match decode_scalar(&self.response) {
            Ok(s) => s,
            Err(_) => return false,
        };

        let challenge = Self::challenge(commitment, value, &announce);

        // s * H must equal R + e * (C - value * G) = R + e * r * H.
        let expected = announce + (commitment.0 - g_generator() * value) * challenge;
        h_generator() * response == expected
    }

    fn challenge(commitment: &Commitment, value: &Scalar, announce: &RistrettoPoint) -> Scalar {
        hash_to_scalar(
            b"TigerSmartChain/Privacy/BlindingProof/v1",
            &[&commitment.to_bytes(), &value.to_bytes(), &encode_point(announce)],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn commitments_are_homomorphic() {
        let v1 = Scalar::random(&mut OsRng);
        let v2 = Scalar::random(&mut OsRng);
        let r1 = Scalar::random(&mut OsRng);
        let r2 = Scalar::random(&mut OsRng);

        let c1 = Commitment::new(&v1, &r1);
        let c2 = Commitment::new(&v2, &r2);
        let sum = Commitment::new(&(v1 + v2), &(r1 + r2));

        assert_eq!(c1.add(&c2), sum);
    }

    #[test]
    fn points_round_trip_through_their_encoding() {
        let point = h_generator() * Scalar::random(&mut OsRng);
        let encoded = encode_point(&point);
        assert_eq!(decode_point(&encoded).unwrap(), point);
    }

    #[test]
    fn an_invalid_point_encoding_is_rejected() {
        // 32 bytes that are not a valid Ristretto encoding.
        let junk = [0xffu8; POINT_LEN];
        assert!(decode_point(&junk).is_err());
        assert!(decode_point(&[0u8; 8]).is_err());
    }

    #[test]
    fn non_canonical_scalars_are_rejected() {
        // All-ones is far above the group order.
        assert!(decode_scalar(&[0xffu8; SCALAR_LEN]).is_err());

        let valid = Scalar::random(&mut OsRng);
        assert_eq!(decode_scalar(&valid.to_bytes()).unwrap(), valid);
    }

    #[test]
    fn a_blinding_proof_verifies_for_the_right_value() {
        let value = Scalar::random(&mut OsRng);
        let blinding = Scalar::random(&mut OsRng);
        let nonce = Scalar::random(&mut OsRng);

        let commitment = Commitment::new(&value, &blinding);
        let proof = BlindingProof::prove(&commitment, &value, &blinding, &nonce);

        assert!(proof.verify(&commitment, &value));
    }

    #[test]
    fn a_blinding_proof_fails_for_a_different_value() {
        let value = Scalar::random(&mut OsRng);
        let blinding = Scalar::random(&mut OsRng);
        let nonce = Scalar::random(&mut OsRng);

        let commitment = Commitment::new(&value, &blinding);
        let proof = BlindingProof::prove(&commitment, &value, &blinding, &nonce);

        let other = value + Scalar::ONE;
        assert!(!proof.verify(&commitment, &other));
    }

    #[test]
    fn a_blinding_proof_fails_against_a_different_commitment() {
        let value = Scalar::random(&mut OsRng);
        let blinding = Scalar::random(&mut OsRng);
        let nonce = Scalar::random(&mut OsRng);

        let commitment = Commitment::new(&value, &blinding);
        let proof = BlindingProof::prove(&commitment, &value, &blinding, &nonce);

        let other = Commitment::new(&value, &Scalar::random(&mut OsRng));
        assert!(!proof.verify(&other, &value));
    }

    #[test]
    fn a_forged_response_is_rejected() {
        let value = Scalar::random(&mut OsRng);
        let blinding = Scalar::random(&mut OsRng);
        let nonce = Scalar::random(&mut OsRng);

        let commitment = Commitment::new(&value, &blinding);
        let mut proof = BlindingProof::prove(&commitment, &value, &blinding, &nonce);
        proof.response[0] ^= 0x01;

        assert!(!proof.verify(&commitment, &value));
    }
}
