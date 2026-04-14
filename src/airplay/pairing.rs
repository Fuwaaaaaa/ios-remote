use crate::airplay::crypto;
use ed25519_dalek::{Signer, SigningKey};
use x25519_dalek::{EphemeralSecret, PublicKey as X25519Public};
use tracing::{debug, info};

/// Transient pair-setup: a simplified pairing used when no PIN is required.
///
/// AirPlay supports "transient" pairing where the receiver accepts any sender
/// without a PIN. This is the mode Apple TV uses by default.
///
/// Flow:
///   1. iPhone sends its X25519 public key + Ed25519 public key
///   2. We respond with our X25519 public key + Ed25519 public key
///   3. Both sides derive a shared secret via X25519
///   4. Both sides sign the exchange with Ed25519 to prove identity
pub struct PairSetup {
    signing_key: SigningKey,
}

impl PairSetup {
    pub fn new(signing_key: SigningKey) -> Self {
        Self { signing_key }
    }

    /// Handle pair-setup request body and return the response body.
    ///
    /// For transient pairing, we parse the client's public keys,
    /// generate our ephemeral keys, and return them.
    pub fn handle(&self, request_body: &[u8]) -> Result<(Vec<u8>, Option<[u8; 32]>), String> {
        // Parse the request. In transient mode, the body contains a plist
        // or raw bytes depending on the pairing type.
        //
        // Type byte at offset 4: 0x00 = transient (no PIN)
        if request_body.len() < 4 {
            // Initial pair-setup request — respond with our capabilities
            info!("pair-setup: initial request, sending transient mode support");
            return Ok((vec![0x01, 0x00, 0x00, 0x00], None));
        }

        // For transient pairing, we generate an X25519 keypair
        let mut rng = rand::thread_rng();
        let our_secret = EphemeralSecret::random_from_rng(&mut rng);
        let our_public = X25519Public::from(&our_secret);

        // If the client sent their X25519 public key (32 bytes after header)
        if request_body.len() >= 36 {
            let client_public_bytes: [u8; 32] = request_body[4..36]
                .try_into()
                .map_err(|_| "invalid client public key length")?;
            let client_public = X25519Public::from(client_public_bytes);

            // Derive shared secret
            let shared_secret = our_secret.diffie_hellman(&client_public);

            // Derive session key using HKDF
            let mut session_key = [0u8; 32];
            crypto::hkdf_derive(
                shared_secret.as_bytes(),
                b"Pair-Setup-Encrypt-Salt",
                b"Pair-Setup-Encrypt-Info",
                &mut session_key,
            );

            debug!("pair-setup: shared secret derived, session key ready");

            // Build response: our X25519 public key + our Ed25519 public key
            let our_ed_public = self.signing_key.verifying_key().to_bytes();
            let mut response = Vec::with_capacity(64);
            response.extend_from_slice(our_public.as_bytes());
            response.extend_from_slice(&our_ed_public);

            return Ok((response, Some(session_key)));
        }

        // Fallback: just echo our public key
        Ok((our_public.as_bytes().to_vec(), None))
    }
}

/// Pair-verify: verify a previously paired device (or transient pair).
///
/// Flow:
///   1. Client sends X25519 public key (32 bytes)
///   2. Server responds with X25519 public key + encrypted Ed25519 signature
///   3. Client sends encrypted Ed25519 signature
///   4. Session key is derived for the connection
pub struct PairVerify {
    signing_key: SigningKey,
}

impl PairVerify {
    pub fn new(signing_key: SigningKey) -> Self {
        Self { signing_key }
    }

    /// Handle pair-verify step 1: client sends their X25519 public key.
    /// Returns (response_body, derived_session_key).
    pub fn step1(&self, client_x25519_pub: &[u8; 32]) -> Result<(Vec<u8>, [u8; 32]), String> {
        let mut rng = rand::thread_rng();

        // Generate our ephemeral X25519 keypair
        let our_secret = EphemeralSecret::random_from_rng(&mut rng);
        let our_public = X25519Public::from(&our_secret);

        // Derive shared secret
        let client_pub = X25519Public::from(*client_x25519_pub);
        let shared_secret = our_secret.diffie_hellman(&client_pub);

        // Derive session key
        let mut session_key = [0u8; 32];
        crypto::hkdf_derive(
            shared_secret.as_bytes(),
            b"Pair-Verify-Encrypt-Salt",
            b"Pair-Verify-Encrypt-Info",
            &mut session_key,
        );

        // Sign the exchange: sign(our_x25519_pub || client_x25519_pub)
        let mut to_sign = Vec::with_capacity(64);
        to_sign.extend_from_slice(our_public.as_bytes());
        to_sign.extend_from_slice(client_x25519_pub);
        let signature = self.signing_key.sign(&to_sign);

        // Encrypt the signature with the session key
        let nonce = crypto::nonce_from_counter(0);
        let encrypted_sig = crypto::encrypt_chacha(&session_key, &nonce, &signature.to_bytes());

        // Response: our X25519 public key (32) + encrypted signature
        let mut response = Vec::with_capacity(32 + encrypted_sig.len());
        response.extend_from_slice(our_public.as_bytes());
        response.extend_from_slice(&encrypted_sig);

        info!("pair-verify step 1 complete");

        Ok((response, session_key))
    }

    /// Handle pair-verify step 2: client sends their encrypted signature.
    /// Returns true if verification passed.
    pub fn step2(&self, _encrypted_client_sig: &[u8], _session_key: &[u8; 32]) -> Result<bool, String> {
        // In transient mode, we trust the client signature.
        // A full implementation would decrypt and verify the Ed25519 signature.
        info!("pair-verify step 2 complete (transient mode — accepting)");
        Ok(true)
    }
}

/// FairPlay setup handler.
///
/// FairPlay (fp-setup) is Apple's DRM key exchange used to encrypt the
/// mirroring stream. The FPLY messages have a specific binary format:
///   - Bytes 0-3: "FPLY" signature
///   - Byte 4: major version (2)
///   - Byte 5: minor version (1)
///   - Byte 6: phase number
///
/// For basic mirroring without DRM enforcement, we can respond with
/// pre-computed responses that satisfy the handshake.
pub struct FairPlaySetup {
    phase: u8,
}

impl FairPlaySetup {
    pub fn new() -> Self {
        Self { phase: 0 }
    }

    pub fn handle(&mut self, request_body: &[u8]) -> Result<Vec<u8>, String> {
        if request_body.len() < 4 || &request_body[0..4] != b"FPLY" {
            // Not an FPLY message — might be the initial setup request
            debug!("fp-setup: non-FPLY request ({} bytes)", request_body.len());
            self.phase += 1;
            return Ok(self.build_fply_response(1));
        }

        let phase = if request_body.len() > 6 {
            request_body[6]
        } else {
            self.phase
        };

        info!(phase, "fp-setup: handling phase");
        self.phase = phase + 1;

        Ok(self.build_fply_response(self.phase))
    }

    fn build_fply_response(&self, phase: u8) -> Vec<u8> {
        // Minimal FPLY response header
        let mut resp = Vec::with_capacity(32);
        resp.extend_from_slice(b"FPLY"); // signature
        resp.push(0x03); // major version
        resp.push(0x01); // minor version
        resp.push(phase); // current phase
        resp.push(0x00); // reserved

        // Pad with zeros to satisfy minimum expected length
        resp.resize(32, 0);
        resp
    }
}
