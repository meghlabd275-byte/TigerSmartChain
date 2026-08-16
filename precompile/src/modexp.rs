//! Modexp Precompile (address 0x05)
//!
//! Modular exponentiation: result = base^exp mod modulus.
//! Input layout:
//!   [0..32]   base length  (Bsize, big-endian)
//!   [32..64]  exp length   (Esize, big-endian)
//!   [64..96]  mod length   (Msize, big-endian)
//!   [96..]    base || exp || modulus  (each big-endian, left-aligned)
//! Output: (Msize) bytes, the result right-padded to Msize.

use num_bigint::{BigInt, Sign};
use num_traits::Zero;

/// Read a 32-byte big-endian length from `data` at `offset`.
fn read_len(data: &[u8], offset: usize) -> Result<usize, String> {
    if data.len() < offset + 32 {
        return Err("input too short for length word".to_string());
    }
    let mut len = 0usize;
    for i in 0..32 {
        len = len
            .checked_shl(8)
            .ok_or("length overflow")?
            .checked_add(data[offset + i] as usize)
            .ok_or("length overflow")?;
    }
    Ok(len)
}

/// Modexp precompile.
pub fn modexp(input: &[u8], _gas: u64) -> Result<Vec<u8>, String> {
    let bsize = read_len(input, 0)?;
    let esize = read_len(input, 32)?;
    let msize = read_len(input, 64)?;

    // Cap sizes to avoid denial of service via absurd length words.
    if bsize > 1024 * 1024 || esize > 1024 * 1024 || msize > 1024 * 1024 {
        return Err("modexp input too large".to_string());
    }

    let body_start = 96;
    let needed = body_start + bsize + esize + msize;
    let padded = if input.len() < needed {
        let mut v = input.to_vec();
        v.resize(needed, 0);
        v
    } else {
        input.to_vec()
    };

    let base = &padded[body_start..body_start + bsize];
    let exp = &padded[body_start + bsize..body_start + bsize + esize];
    let modulus = &padded[body_start + bsize + esize..body_start + bsize + esize + msize];

    let base_int = BigInt::from_bytes_be(Sign::Plus, strip_leading(base));
    let exp_int = BigInt::from_bytes_be(Sign::Plus, strip_leading(exp));
    let mod_int = BigInt::from_bytes_be(Sign::Plus, strip_leading(modulus));

    let result = if mod_int.is_zero() {
        // modulus 0 => result is 0
        BigInt::from(0u32)
    } else {
        base_int.modpow(&exp_int, &mod_int)
    };

    // Output is msize bytes, big-endian, right-padded.
    let mut bytes = result.to_bytes_be().1;
    if bytes.len() < msize {
        let mut out = vec![0u8; msize];
        out[msize - bytes.len()..].copy_from_slice(&bytes);
        bytes = out;
    } else if bytes.len() > msize {
        // keep only the last msize bytes (shouldn't happen for modpow < modulus)
        bytes = bytes[bytes.len() - msize..].to_vec();
    }
    Ok(bytes)
}

/// Strip leading zero bytes (BigInt handles this, but keeps input canonical).
fn strip_leading(b: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < b.len() && b[i] == 0 {
        i += 1;
    }
    &b[i..]
}

/// Get precompile address
pub fn get_address() -> u64 {
    0x05
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u256_be(n: u64) -> [u8; 32] {
        let mut b = [0u8; 32];
        b[24..].copy_from_slice(&n.to_be_bytes());
        b
    }

    #[test]
    fn test_modexp_simple() {
        // 2^10 mod 17 = 1024 mod 17 = 4
        let mut input = Vec::new();
        input.extend_from_slice(&u256_be(1)); // bsize
        input.extend_from_slice(&u256_be(1)); // esize
        input.extend_from_slice(&u256_be(1)); // msize
        input.push(2);  // base
        input.push(10); // exp
        input.push(17); // modulus
        let out = modexp(&input, 0).unwrap();
        assert_eq!(out, vec![4]);
    }

    #[test]
    fn test_modexp_zero_modulus() {
        let mut input = Vec::new();
        input.extend_from_slice(&u256_be(1));
        input.extend_from_slice(&u256_be(1));
        input.extend_from_slice(&u256_be(1));
        input.push(5);
        input.push(3);
        input.push(0); // modulus 0
        let out = modexp(&input, 0).unwrap();
        assert_eq!(out, vec![0]);
    }

    #[test]
    fn test_modexp_large() {
        // 3^200 mod 7. Use a 2-byte modulus to check padding.
        // 3^200 mod 7: 3^6=729=729-104*7=729-728=1 mod 7, 200=6*33+2, 3^2=9=2 mod7
        let mut input = Vec::new();
        input.extend_from_slice(&u256_be(1));
        input.extend_from_slice(&u256_be(1));
        input.extend_from_slice(&u256_be(2)); // msize 2
        input.push(3);
        input.push(200);
        input.push(0);
        input.push(7);
        let out = modexp(&input, 0).unwrap();
        assert_eq!(out, vec![0, 2]);
    }
}
