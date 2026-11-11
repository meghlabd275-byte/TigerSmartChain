//! BLAKE2b F compression function Precompile (address 0x09, EIP-152).
//!
//! Input is exactly 213 bytes:
//!   [0..4]     rounds, big-endian
//!   [4..68]    h, the 64-byte state (8 little-endian words)
//!   [68..196]  m, the 128-byte message block (16 little-endian words)
//!   [196..212] t, the 16-byte offset counter (2 little-endian words)
//!   [212]      final block flag, 0 or 1
//! Output is the updated 64-byte state. Gas cost equals the round count.

const IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

const SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

#[inline]
fn g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
}

/// Run the BLAKE2b compression function.
pub fn blake2f(input: &[u8], _gas: u64) -> Result<Vec<u8>, String> {
    if input.len() != 213 {
        return Err(format!("input must be exactly 213 bytes, got {}", input.len()));
    }
    let rounds = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    if input[212] > 1 {
        return Err("final block flag must be 0 or 1".to_string());
    }
    let f = input[212] == 1;

    let mut h = [0u64; 8];
    for (i, word) in h.iter_mut().enumerate() {
        let base = 4 + i * 8;
        *word = u64::from_le_bytes(input[base..base + 8].try_into().unwrap());
    }
    let mut m = [0u64; 16];
    for (i, word) in m.iter_mut().enumerate() {
        let base = 68 + i * 8;
        *word = u64::from_le_bytes(input[base..base + 8].try_into().unwrap());
    }
    let t0 = u64::from_le_bytes(input[196..204].try_into().unwrap());
    let t1 = u64::from_le_bytes(input[204..212].try_into().unwrap());

    let mut v = [0u64; 16];
    v[..8].copy_from_slice(&h);
    v[8..].copy_from_slice(&IV);
    v[12] ^= t0;
    v[13] ^= t1;
    if f {
        v[14] = !v[14];
    }

    for round in 0..rounds {
        let s = &SIGMA[round % 10];
        g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
        g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
        g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
        g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
        g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
        g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
        g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
        g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
    }

    let mut out = Vec::with_capacity(64);
    for i in 0..8 {
        h[i] ^= v[i] ^ v[i + 8];
        out.extend_from_slice(&h[i].to_le_bytes());
    }
    Ok(out)
}

/// Get precompile address
pub fn get_address() -> u64 {
    0x09
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real compression: `h` is the default BLAKE2b-512 parameter block
    /// (`IV[0] ^ 0x01010040`), the message is `"abc"` zero-padded, the offset
    /// counter is 3, and the final-block flag is set. The expected digest is
    /// `hashlib.blake2b(b"abc")`, so this pins the implementation to an
    /// independent reference rather than to itself.
    #[test]
    fn test_blake2f_matches_hashlib_blake2b() {
        let input = hex::decode(concat!(
            "0000000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f",
            "3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e13",
            "19cde05b61626300000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "000000000300000000000000000000000000000001"
        ))
        .unwrap();
        assert_eq!(input.len(), 213);

        let out = blake2f(&input, 0).unwrap();
        assert_eq!(
            hex::encode(&out),
            "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d1\
             7d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"
        );

        // The same block without the final flag is a different, well-defined
        // value: the flag must actually change the result.
        let mut non_final = input.clone();
        non_final[212] = 0;
        let out_non_final = blake2f(&non_final, 0).unwrap();
        assert_ne!(out_non_final, out);
    }

    #[test]
    fn test_blake2f_rejects_bad_length() {
        assert!(blake2f(&[0u8; 212], 0).is_err());
        assert!(blake2f(&[0u8; 214], 0).is_err());
    }

    #[test]
    fn test_blake2f_rejects_bad_flag() {
        let mut input = vec![0u8; 213];
        input[212] = 2;
        assert!(blake2f(&input, 0).is_err());
    }
}
