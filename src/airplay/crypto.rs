use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use hkdf::Hkdf;
use sha2::Sha512;

/// Derive a symmetric key from a shared secret using HKDF-SHA512.
///
/// AirPlay uses HKDF with specific "info" strings to derive encryption keys
/// for different channels (control, data, events).
pub fn hkdf_derive(shared_secret: &[u8], salt: &[u8], info: &[u8], out: &mut [u8]) {
    let hk = Hkdf::<Sha512>::new(Some(salt), shared_secret);
    hk.expand(info, out)
        .expect("HKDF output length should be valid");
}

/// Encrypt a message with ChaCha20-Poly1305.
pub fn encrypt_chacha(key: &[u8; 32], nonce_bytes: &[u8; 12], plaintext: &[u8]) -> Vec<u8> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .encrypt(nonce, plaintext)
        .expect("encryption should not fail")
}

/// Decrypt a message with ChaCha20-Poly1305.
pub fn decrypt_chacha(
    key: &[u8; 32],
    nonce_bytes: &[u8; 12],
    ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("decryption failed: {}", e))
}

/// Build a little-endian 12-byte nonce from a 64-bit counter.
/// AirPlay pair-verify uses this pattern.
pub fn nonce_from_counter(counter: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[4..12].copy_from_slice(&counter.to_le_bytes());
    nonce
}
