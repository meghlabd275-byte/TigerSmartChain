//! BN128 / BN254 Precompiles
//!
//!   0x06 ecadd    : point addition on the BN254 G1 curve
//!   0x07 ecmul    : scalar multiplication on G1
//!   0x08 ecpairing: pairing check over G1/G2
//!
//! Uses ark-bn254 for real curve arithmetic. Points use affine coordinates,
//! 32-byte big-endian field elements. The point at infinity is (0,0).

use ark_bn254::{Bn254, Fq, Fq12, Fq2, Fr, G1Affine, G2Affine, G1Projective};
use ark_ec::{AffineRepr, CurveGroup, pairing::Pairing};
use ark_ff::{BigInteger, One, PrimeField};

/// Parse a 32-byte big-endian scalar into the base field Fq.
fn parse_fq(bytes: &[u8]) -> Result<Fq, String> {
    if bytes.len() != 32 {
        return Err("field element must be 32 bytes".to_string());
    }
    Ok(Fq::from_be_bytes_mod_order(bytes))
}

/// Parse the scalar field Fr from 32 bytes (used by ecmul).
fn parse_fr(bytes: &[u8]) -> Result<Fr, String> {
    if bytes.len() != 32 {
        return Err("scalar must be 32 bytes".to_string());
    }
    Ok(Fr::from_be_bytes_mod_order(bytes))
}

/// Parse a 64-byte G1 point (x||y). All-zero => point at infinity.
fn parse_g1(bytes: &[u8]) -> Result<G1Affine, String> {
    if bytes.len() != 64 {
        return Err("G1 point must be 64 bytes".to_string());
    }
    if bytes.iter().all(|&b| b == 0) {
        return Ok(G1Affine::identity());
    }
    let x = parse_fq(&bytes[0..32])?;
    let y = parse_fq(&bytes[32..64])?;
    let p = G1Affine::new(x, y);
    if !p.is_in_correct_subgroup_assuming_on_curve() {
        return Err("G1 point not in correct subgroup".to_string());
    }
    Ok(p)
}

/// Serialize a G1 point to 64 bytes (x||y); identity => all zeros.
fn serialize_g1(p: &G1Affine) -> Vec<u8> {
    if p.xy().is_none() {
        return vec![0u8; 64];
    }
    let (x, y) = p.xy().unwrap();
    let mut out = vec![0u8; 64];
    let xb = x.into_bigint().to_bytes_be();
    let yb = y.into_bigint().to_bytes_be();
    out[32 - xb.len()..32].copy_from_slice(&xb);
    out[64 - yb.len()..64].copy_from_slice(&yb);
    out
}

/// Parse a 128-byte G2 point: (x.c0, x.c1, y.c0, y.c1), each 32-byte big-endian.
fn parse_g2(bytes: &[u8]) -> Result<G2Affine, String> {
    if bytes.len() != 128 {
        return Err("G2 point must be 128 bytes".to_string());
    }
    if bytes.iter().all(|&b| b == 0) {
        return Ok(G2Affine::identity());
    }
    let x_c0 = parse_fq(&bytes[0..32])?;
    let x_c1 = parse_fq(&bytes[32..64])?;
    let y_c0 = parse_fq(&bytes[64..96])?;
    let y_c1 = parse_fq(&bytes[96..128])?;
    let x = Fq2::new(x_c0, x_c1);
    let y = Fq2::new(y_c0, y_c1);
    let p = G2Affine::new(x, y);
    if !p.is_in_correct_subgroup_assuming_on_curve() {
        return Err("G2 point not in correct subgroup".to_string());
    }
    Ok(p)
}

/// EC Add (0x06): input 128 bytes (two G1 points), output 64 bytes.
pub fn ecadd(input: &[u8], _gas: u64) -> Result<Vec<u8>, String> {
    if input.len() < 128 {
        return Err("ecadd input must be at least 128 bytes".to_string());
    }
    let p1 = parse_g1(&input[0..64])?;
    let p2 = parse_g1(&input[64..128])?;
    let sum: G1Projective = p1 + p2;
    Ok(serialize_g1(&G1Affine::from(sum)))
}

/// EC Mul (0x07): input 96 bytes (G1 point + 32-byte scalar), output 64 bytes.
pub fn ecmul(input: &[u8], _gas: u64) -> Result<Vec<u8>, String> {
    if input.len() < 96 {
        return Err("ecmul input must be at least 96 bytes".to_string());
    }
    let p = parse_g1(&input[0..64])?;
    let scalar = parse_fr(&input[64..96])?;
    let prod = p * scalar;
    Ok(serialize_g1(&G1Affine::from(prod)))
}

/// EC Pairing (0x08): input is a sequence of 192-byte pairs
/// (64-byte G1 || 128-byte G2). Returns 32 bytes: 1 if pairing holds, else 0.
pub fn ecpairing(input: &[u8], _gas: u64) -> Result<Vec<u8>, String> {
    if input.len() % 192 != 0 {
        return Err("pairing input length must be a multiple of 192".to_string());
    }
    let mut g1s: Vec<G1Affine> = Vec::new();
    let mut g2s: Vec<G2Affine> = Vec::new();
    for chunk in input.chunks(192) {
        let g1 = parse_g1(&chunk[0..64])?;
        let g2 = parse_g2(&chunk[64..192])?;
        // EIP-197: negate the G1 input of each pair.
        let neg_g1 = -g1;
        g1s.push(neg_g1);
        g2s.push(g2);
    }
    // multi_pairing computes prod e(p_i, q_i). Valid if == 1 in Fq12.
    let out_field = Bn254::multi_pairing(g1s.iter(), g2s.iter()).0;
    let result = out_field == Fq12::one();
    let mut out = vec![0u8; 32];
    if result {
        out[31] = 1;
    }
    Ok(out)
}

/// Get addresses
pub fn get_addresses() -> (u64, u64, u64) {
    (0x06, 0x07, 0x08)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecadd_identity() {
        let input = vec![0u8; 128];
        let out = ecadd(&input, 0).unwrap();
        assert_eq!(out, vec![0u8; 64]);
    }

    #[test]
    fn test_ecmul_by_zero() {
        let mut input = vec![0u8; 96];
        let g = G1Affine::generator();
        let g_ser = serialize_g1(&g);
        input[..64].copy_from_slice(&g_ser);
        let out = ecmul(&input, 0).unwrap();
        assert_eq!(out, vec![0u8; 64]);
    }

    #[test]
    fn test_ecmul_generator_one() {
        let mut input = vec![0u8; 96];
        let g = G1Affine::generator();
        let g_ser = serialize_g1(&g);
        input[..64].copy_from_slice(&g_ser);
        // scalar = 1 lives in bytes [64..96]; set its last byte.
        input[95] = 1;
        let out = ecmul(&input, 0).unwrap();
        assert_eq!(out, g_ser);
    }

    #[test]
    fn test_ecmul_generator_two() {
        // generator * 2 == generator + generator
        let mut input = vec![0u8; 96];
        let g = G1Affine::generator();
        let g_ser = serialize_g1(&g);
        input[..64].copy_from_slice(&g_ser);
        input[95] = 2;
        let out = ecmul(&input, 0).unwrap();
        let expected = serialize_g1(&G1Affine::from(g + g));
        assert_eq!(out, expected);
    }

    #[test]
    fn test_ecpairing_empty_is_one() {
        let out = ecpairing(&[], 0).unwrap();
        assert_eq!(out[31], 1);
    }

    #[test]
    fn test_ecpairing_identity_pairs_is_one() {
        let input = vec![0u8; 192];
        let out = ecpairing(&input, 0).unwrap();
        assert_eq!(out[31], 1);
    }
}
