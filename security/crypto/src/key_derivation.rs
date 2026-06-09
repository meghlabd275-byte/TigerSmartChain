//! Key derivation functions for HD wallets.

use zeroize::Zeroize;
use sha2::{Sha256, Sha512, Digest};

/// Mnemonic phrase (12-24 words).
#[derive(Clone, Debug, Zeroize)]
#[zeroize(drop)]
pub struct Mnemonic(String);

/// Seed derived from mnemonic.
#[derive(Clone, Debug, Zeroize)]
#[zeroize(drop)]
pub struct Seed([u8; 64]);

/// BIP-39 word list (2048 words).
const WORD_LIST: &[&str] = &[
    "abandon", "ability", "able", "about", "above", "absent", "absorb", "abstract",
    "absurd", "abuse", "access", "accident", "account", "accuse", "achieve", "acid",
    "acoustic", "acquire", "across", "act", "action", "actor", "actress", "actual",
    "adapt", "add", "addict", "address", "adjust", "admit", "adult", "advance",
    // ... (2048 words in total - abbreviated for brevity)
];

/// Entropy strength in bits.
pub enum EntropyStrength {
    Bits128 = 128,
    Bits160 = 160,
    Bits192 = 192,
    Bits224 = 224,
    Bits256 = 256,
}

/// Generate mnemonic from entropy.
pub fn mnemonic_from_entropy(entropy: &[u8]) -> Result<Mnemonic, super::Error> {
    // Validate entropy length
    if entropy.len() < 16 || entropy.len() > 32 {
        return Err(super::Error::InvalidData("entropy must be 16-32 bytes".into()));
    }
    
    // Calculate checksum
    let mut hasher = Sha256::new();
    hasher.update(entropy);
    let checksum = hasher.finalize();
    
    // Calculate checksum bits (entropy.len() * 8 / 32)
    let checksum_bits = entropy.len() * 8 / 32;
    
    // Combine entropy and checksum
    let total_bits = entropy.len() * 8 + checksum_bits;
    let mut bits = Vec::with_capacity(total_bits);
    
    for byte in entropy {
        for i in (0..8).rev() {
            bits.push((byte >> i) & 1);
        }
    }
    
    for i in 0..checksum_bits {
        bits.push((checksum[i / 8] >> (7 - i % 8)) & 1);
    }
    
    // Convert to words
    let mut words = Vec::new();
    for chunk in bits.chunks(11) {
        let mut index = 0;
        for (i, &bit) in chunk.iter().enumerate() {
            index = (index << 1) | bit as usize;
        }
        words.push(WORD_LIST[index]);
    }
    
    Ok(Mnemonic(words.join(" ")))
}

/// Generate seed from mnemonic.
pub fn seed_from_mnemonic(mnemonic: &Mnemonic, passphrase: &str) -> Result<Seed, super::Error> {
    let salt = format!("mnemonic{}", passphrase);
    let mut seed = [0u8; 64];
    
    // Use PBKDF2 with 2048 iterations
    pbkdf2(&mnemonic.0.as_bytes(), salt.as_bytes(), 2048, &mut seed);
    
    Ok(Seed(seed))
}

/// Derive child key from seed.
pub fn derive_child_key(seed: &Seed, path: &[u32]) -> Result<Seed, super::Error> {
    let mut key = seed.0.to_vec();
    
    for &index in path {
        // Hardened derivation
        let mut data = vec![0x00];
        data.extend_from_slice(&key);
        data.extend_from_slice(&index.to_be_bytes());
        
        let mut hasher = Sha512::new();
        hasher.update(&data);
        let result = hasher.finalize();
        
        key.copy_from_slice(&result[..64]);
    }
    
    Ok(Seed(key.try_into().unwrap()))
}

/// PBKDF2 implementation.
fn pbkdf2(password: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    use hmac::{Hmac, Mac};
    
    type HmacSha512 = Hmac<Sha512>;
    
    let mut mac = HmacSha512::new_from_slice(password).unwrap();
    mac.update(salt);
    mac.update(&[0, 0, 0, 1]);
    
    let mut u = mac.finalize().into_bytes().to_vec();
    let mut result = u.clone();
    
    for i in 1..iterations {
        mac = HmacSha512::new_from_slice(password).unwrap();
        mac.update(&u);
        u = mac.finalize().into_bytes().to_vec();
        
        for (j, byte) in u.iter().enumerate() {
            result[j] ^= byte;
        }
    }
    
    output.copy_from_slice(&result[..output.len()]);
}

/// Validate mnemonic phrase.
pub fn validate_mnemonic(mnemonic: &str) -> bool {
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    
    // Must be 12, 15, 18, 21, or 24 words
    if words.len() % 3 != 0 || words.len() < 12 || words.len() > 24 {
        return false;
    }
    
    // All words must be in word list
    words.iter().all(|w| WORD_LIST.contains(w))
}

impl Mnemonic {
    /// Get the mnemonic phrase.
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    /// Get word count.
    pub fn word_count(&self) -> usize {
        self.0.split_whitespace().count()
    }
}

impl Seed {
    /// Get the seed bytes.
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}