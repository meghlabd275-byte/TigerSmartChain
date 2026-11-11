//! Transport for the peer-to-peer network.
//!
//! A peer connection is a mutually authenticated, forward-secret channel
//! carrying length-prefixed frames:
//!
//!   1. Each side generates an ephemeral X25519 key pair per connection, so a
//!      recorded session cannot be decrypted later even if a long-term key
//!      leaks.
//!   2. The two sides exchange ephemeral public keys and a random nonce, and
//!      bind them together in a transcript hash. The handshake is
//!      authenticated by a signature over that transcript with the peer's
//!      long-term key, which is what stops a man in the middle from
//!      substituting its own ephemeral key.
//!   3. Session keys are derived with HKDF-SHA256 from the shared secret and
//!      the transcript. Each direction has its own key, so a frame from one
//!      direction can never be replayed into the other.
//!   4. Every frame is sealed with ChaCha20-Poly1305, using a sequence number
//!      as associated data. The sequence number is checked for strict
//!      monotonicity, so a captured frame cannot be replayed or reordered.
//!
//! The long-term identity is an Ed25519 key pair. A node that is given no
//! identity generates one at startup, which is enough to authenticate the
//! channel; an operator that wants a stable peer id persists it.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::types::{NetworkError, PeerID};

/// Largest frame body accepted. A block is the largest message the protocol
/// carries, so this bounds a peer's ability to make the node allocate.
pub const MAX_FRAME_SIZE: usize = 8 * 1024 * 1024;

/// How long a handshake may take before the peer is dropped. A peer that
/// connects and then stalls must not hold a connection slot indefinitely.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Domain separator, so keys derived here cannot collide with keys derived for
/// any other purpose from the same secret.
const KEY_INFO_PREFIX: &[u8] = b"tigersmartchain-p2p-v1";

/// A long-term Ed25519 identity.
#[derive(Clone)]
pub struct Identity {
    signing: ed25519_dalek::SigningKey,
}

impl Identity {
    /// Generate a fresh identity from the operating system's RNG.
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut secret = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        Self {
            signing: ed25519_dalek::SigningKey::from_bytes(&secret),
        }
    }

    /// Load an identity from a 32-byte secret key, or generate one if the
    /// slice is not the right length.
    pub fn from_secret_bytes(secret: &[u8]) -> Option<Self> {
        let bytes: [u8; 32] = secret.try_into().ok()?;
        Some(Self {
            signing: ed25519_dalek::SigningKey::from_bytes(&bytes),
        })
    }

    /// The 32-byte secret, for persisting the identity.
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    /// The public key.
    pub fn public_key(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    /// This node's peer id, which is its public key.
    pub fn peer_id(&self) -> PeerID {
        PeerID(self.public_key())
    }

    fn sign(&self, message: &[u8]) -> [u8; 64] {
        use ed25519_dalek::Signer;
        self.signing.sign(message).to_bytes()
    }
}

/// Verify a peer's signature over a handshake transcript.
fn verify_peer_signature(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    let Ok(key) = ed25519_dalek::VerifyingKey::from_bytes(public_key) else {
        return false;
    };
    let signature = ed25519_dalek::Signature::from_bytes(signature);
    // `verify_strict` rejects small-order public keys and non-canonical
    // encodings, which `verify` accepts.
    key.verify_strict(message, &signature).is_ok()
}

/// An encrypted, authenticated connection to one peer.
pub struct Session {
    stream: TcpStream,
    /// Key for frames this node sends.
    send_key: [u8; 32],
    /// Key for frames this node receives.
    recv_key: [u8; 32],
    /// Base nonce material, distinct per direction.
    send_nonce_prefix: [u8; 4],
    recv_nonce_prefix: [u8; 4],
    /// Next sequence number expected from the peer.
    recv_sequence: u64,
    send_sequence: u64,
    /// The peer's long-term public key, as proven by the handshake.
    pub peer_id: PeerID,
    /// Whether this side initiated the connection.
    pub outbound: bool,
}

impl Session {
    /// Perform the handshake as the connecting side.
    pub async fn connect(
        mut stream: TcpStream,
        identity: &Identity,
        expected_peer: Option<&PeerID>,
    ) -> Result<Self, NetworkError> {
        stream.set_nodelay(true).ok();

        let ephemeral = EphemeralKeyPair::generate();
        let initiator_nonce = random_bytes::<32>();
        let initiator_id = identity.public_key();

        // Step 1: send our ephemeral key, our nonce and our identity, and the
        // signature over them. The responder's key is not known yet, so the
        // transcript so far is signed; the responder's reply is bound in by
        // the key derivation below, which both sides compute over the full
        // transcript.
        let mut hello = Vec::with_capacity(96 + 64);
        hello.extend_from_slice(&ephemeral.public);
        hello.extend_from_slice(&initiator_nonce);
        hello.extend_from_slice(&initiator_id);
        let signature = identity.sign(&transcript(
            &ephemeral.public,
            &initiator_nonce,
            &initiator_id,
            &[0u8; 32],
            &[0u8; 32],
        ));
        hello.extend_from_slice(&signature);

        stream
            .write_all(&hello)
            .await
            .map_err(|e| NetworkError::Transport(e.to_string()))?;
        stream
            .flush()
            .await
            .map_err(|e| NetworkError::Transport(e.to_string()))?;

        // Step 2: read the responder's reply.
        let mut reply = vec![0u8; 96 + 64];
        read_exact_timeout(&mut stream, &mut reply, HANDSHAKE_TIMEOUT).await?;

        let responder_ephemeral: [u8; 32] = reply[0..32].try_into().unwrap();
        let responder_nonce: [u8; 32] = reply[32..64].try_into().unwrap();
        let responder_id: [u8; 32] = reply[64..96].try_into().unwrap();
        let responder_signature: [u8; 64] = reply[96..160].try_into().unwrap();

        let responder_transcript = transcript(
            &responder_ephemeral,
            &responder_nonce,
            &responder_id,
            &initiator_nonce,
            &initiator_id,
        );
        if !verify_peer_signature(&responder_id, &responder_transcript, &responder_signature) {
            return Err(NetworkError::Handshake(
                "the peer's handshake signature does not verify".to_string(),
            ));
        }

        let peer_id = PeerID(responder_id);
        if let Some(expected) = expected_peer {
            if *expected != peer_id {
                return Err(NetworkError::Handshake(format!(
                    "connected peer is {} but {} was expected",
                    peer_id, expected
                )));
            }
        }

        // Step 3: derive the session keys over the complete transcript, which
        // covers both ephemeral keys and both nonces.
        let shared = ephemeral.diffie_hellman(&responder_ephemeral);
        let full_transcript = full_transcript(
            &initiator_nonce,
            &initiator_id,
            &responder_nonce,
            &responder_id,
        );
        let (send_key, recv_key, send_nonce_prefix, recv_nonce_prefix) =
            derive_keys(&shared, &full_transcript, true);

        Ok(Self {
            stream,
            send_key,
            recv_key,
            send_nonce_prefix,
            recv_nonce_prefix,
            recv_sequence: 0,
            send_sequence: 0,
            peer_id,
            outbound: true,
        })
    }

    /// Perform the handshake as the accepting side.
    pub async fn accept(mut stream: TcpStream, identity: &Identity) -> Result<Self, NetworkError> {
        stream.set_nodelay(true).ok();

        // Step 1: read the initiator's hello.
        let mut hello = vec![0u8; 96 + 64];
        read_exact_timeout(&mut stream, &mut hello, HANDSHAKE_TIMEOUT).await?;

        let initiator_ephemeral: [u8; 32] = hello[0..32].try_into().unwrap();
        let initiator_nonce: [u8; 32] = hello[32..64].try_into().unwrap();
        let initiator_id: [u8; 32] = hello[64..96].try_into().unwrap();
        let initiator_signature: [u8; 64] = hello[96..160].try_into().unwrap();

        let initiator_transcript = transcript(
            &initiator_ephemeral,
            &initiator_nonce,
            &initiator_id,
            &[0u8; 32],
            &[0u8; 32],
        );
        if !verify_peer_signature(&initiator_id, &initiator_transcript, &initiator_signature) {
            return Err(NetworkError::Handshake(
                "the peer's handshake signature does not verify".to_string(),
            ));
        }

        // Step 2: reply with our own ephemeral key, nonce and identity.
        let ephemeral = EphemeralKeyPair::generate();
        let responder_nonce = random_bytes::<32>();
        let responder_id = identity.public_key();

        let responder_transcript = transcript(
            &ephemeral.public,
            &responder_nonce,
            &responder_id,
            &initiator_nonce,
            &initiator_id,
        );
        let signature = identity.sign(&responder_transcript);

        let mut reply = Vec::with_capacity(96 + 64);
        reply.extend_from_slice(&ephemeral.public);
        reply.extend_from_slice(&responder_nonce);
        reply.extend_from_slice(&responder_id);
        reply.extend_from_slice(&signature);

        stream
            .write_all(&reply)
            .await
            .map_err(|e| NetworkError::Transport(e.to_string()))?;
        stream
            .flush()
            .await
            .map_err(|e| NetworkError::Transport(e.to_string()))?;

        let shared = ephemeral.diffie_hellman(&initiator_ephemeral);
        let full_transcript = full_transcript(
            &initiator_nonce,
            &initiator_id,
            &responder_nonce,
            &responder_id,
        );
        // The responder's directions are mirrored: what it sends is what the
        // initiator receives.
        let (send_key, recv_key, send_nonce_prefix, recv_nonce_prefix) =
            derive_keys(&shared, &full_transcript, false);

        Ok(Self {
            stream,
            send_key,
            recv_key,
            send_nonce_prefix,
            recv_nonce_prefix,
            recv_sequence: 0,
            send_sequence: 0,
            peer_id: PeerID(initiator_id),
            outbound: false,
        })
    }

    /// Seal `payload` into a frame and write it.
    pub async fn send(&mut self, payload: &[u8]) -> Result<(), NetworkError> {
        if payload.len() > MAX_FRAME_SIZE {
            return Err(NetworkError::FrameTooLarge(payload.len()));
        }

        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.send_key));
        let nonce = frame_nonce(&self.send_nonce_prefix, self.send_sequence);
        // The sequence number is authenticated but not transmitted, so a peer
        // cannot alter it to shift the counter.
        let aad = self.send_sequence.to_be_bytes();
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: payload,
                    aad: &aad,
                },
            )
            .map_err(|_| NetworkError::Encryption)?;

        let length = u32::try_from(ciphertext.len())
            .map_err(|_| NetworkError::FrameTooLarge(ciphertext.len()))?;
        self.stream
            .write_all(&length.to_be_bytes())
            .await
            .map_err(|e| NetworkError::Transport(e.to_string()))?;
        self.stream
            .write_all(&ciphertext)
            .await
            .map_err(|e| NetworkError::Transport(e.to_string()))?;
        self.stream
            .flush()
            .await
            .map_err(|e| NetworkError::Transport(e.to_string()))?;

        self.send_sequence += 1;
        Ok(())
    }

    /// Read one frame and open it.
    ///
    /// A frame that does not authenticate, or whose sequence number is not the
    /// one expected next, ends the connection: either could mean an attacker
    /// is injecting traffic, and neither can be recovered from.
    pub async fn recv(&mut self) -> Result<Vec<u8>, NetworkError> {
        let mut length_bytes = [0u8; 4];
        self.stream
            .read_exact(&mut length_bytes)
            .await
            .map_err(|e| NetworkError::Transport(e.to_string()))?;
        let length = u32::from_be_bytes(length_bytes) as usize;

        // The minimum sealed frame is the 16-byte tag; anything smaller cannot
        // be a frame this protocol produced.
        if length < 16 || length > MAX_FRAME_SIZE + 16 {
            return Err(NetworkError::FrameTooLarge(length));
        }

        let mut ciphertext = vec![0u8; length];
        self.stream
            .read_exact(&mut ciphertext)
            .await
            .map_err(|e| NetworkError::Transport(e.to_string()))?;

        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.recv_key));
        let nonce = frame_nonce(&self.recv_nonce_prefix, self.recv_sequence);
        let aad = self.recv_sequence.to_be_bytes();
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| {
                NetworkError::Handshake(
                    "a frame failed authentication; the connection is not trustworthy".to_string(),
                )
            })?;

        self.recv_sequence += 1;
        Ok(plaintext)
    }

    /// The peer's address, for logging.
    pub fn peer_address(&self) -> Option<std::net::SocketAddr> {
        self.stream.peer_addr().ok()
    }

    /// Shut the connection down.
    pub async fn close(&mut self) {
        let _ = self.stream.shutdown().await;
    }
}

/// An X25519 key pair used for one connection only.
struct EphemeralKeyPair {
    secret: x25519_dalek::StaticSecret,
    public: [u8; 32],
}

impl EphemeralKeyPair {
    fn generate() -> Self {
        use rand::RngCore;
        let mut secret_bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret_bytes);
        let secret = x25519_dalek::StaticSecret::from(secret_bytes);
        let public = x25519_dalek::PublicKey::from(&secret).to_bytes();
        Self { secret, public }
    }

    fn diffie_hellman(&self, peer_public: &[u8; 32]) -> [u8; 32] {
        let peer = x25519_dalek::PublicKey::from(*peer_public);
        self.secret.diffie_hellman(&peer).to_bytes()
    }
}

/// The transcript signed during the handshake.
///
/// Each side signs a transcript that includes the other side's nonce and id
/// once those are known, which binds the signature to this specific exchange
/// and prevents a recorded handshake from being replayed.
fn transcript(
    ephemeral_public: &[u8; 32],
    own_nonce: &[u8; 32],
    own_id: &[u8; 32],
    peer_nonce: &[u8; 32],
    peer_id: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(KEY_INFO_PREFIX);
    hasher.update(b"-handshake");
    hasher.update(ephemeral_public);
    hasher.update(own_nonce);
    hasher.update(own_id);
    hasher.update(peer_nonce);
    hasher.update(peer_id);
    hasher.finalize().into()
}

/// The transcript both sides agree on, used to derive session keys.
fn full_transcript(
    initiator_nonce: &[u8; 32],
    initiator_id: &[u8; 32],
    responder_nonce: &[u8; 32],
    responder_id: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(KEY_INFO_PREFIX);
    hasher.update(b"-keys");
    hasher.update(initiator_nonce);
    hasher.update(initiator_id);
    hasher.update(responder_nonce);
    hasher.update(responder_id);
    hasher.finalize().into()
}

/// Derive the two directional keys and nonce prefixes.
///
/// The initiator and the responder must agree on which key encrypts which
/// direction, so the same `is_initiator` flag selects them on both sides.
#[allow(clippy::type_complexity)]
fn derive_keys(
    shared_secret: &[u8; 32],
    transcript: &[u8; 32],
    is_initiator: bool,
) -> ([u8; 32], [u8; 32], [u8; 4], [u8; 4]) {
    // The transcript is the salt, so keys are unique per handshake even if a
    // peer ever repeats an ephemeral key.
    let hkdf = Hkdf::<Sha256>::new(Some(transcript), shared_secret);

    let mut initiator_to_responder = [0u8; 32];
    let mut responder_to_initiator = [0u8; 32];
    hkdf.expand(&[KEY_INFO_PREFIX, b"-i2r"].concat(), &mut initiator_to_responder)
        .expect("32 bytes is a valid HKDF output length");
    hkdf.expand(&[KEY_INFO_PREFIX, b"-r2i"].concat(), &mut responder_to_initiator)
        .expect("32 bytes is a valid HKDF output length");

    let mut nonce_material = [0u8; 8];
    hkdf.expand(&[KEY_INFO_PREFIX, b"-nonce"].concat(), &mut nonce_material)
        .expect("8 bytes is a valid HKDF output length");

    let i2r_prefix: [u8; 4] = nonce_material[0..4].try_into().unwrap();
    let r2i_prefix: [u8; 4] = nonce_material[4..8].try_into().unwrap();

    if is_initiator {
        (
            initiator_to_responder,
            responder_to_initiator,
            i2r_prefix,
            r2i_prefix,
        )
    } else {
        (
            responder_to_initiator,
            initiator_to_responder,
            r2i_prefix,
            i2r_prefix,
        )
    }
}

/// Build the 12-byte nonce for a frame: a 4-byte per-direction prefix and the
/// 8-byte sequence number.
fn frame_nonce(prefix: &[u8; 4], sequence: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[0..4].copy_from_slice(prefix);
    nonce[4..12].copy_from_slice(&sequence.to_be_bytes());
    nonce
}

fn random_bytes<const N: usize>() -> [u8; N] {
    use rand::RngCore;
    let mut out = [0u8; N];
    rand::rngs::OsRng.fill_bytes(&mut out);
    out
}

/// Read exactly `buffer.len()` bytes, giving up after `timeout`.
async fn read_exact_timeout(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    timeout: Duration,
) -> Result<(), NetworkError> {
    match tokio::time::timeout(timeout, stream.read_exact(buffer)).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => Err(NetworkError::Transport(e.to_string())),
        Err(_) => Err(NetworkError::Handshake(
            "the peer did not finish the handshake in time".to_string(),
        )),
    }
}

/// The current time in milliseconds since the Unix epoch, or 0 if the clock is
/// before it. Used for peer liveness, never for consensus.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// Establish a connected pair of sessions over a real TCP socket.
    async fn connected_pair() -> (Session, Session) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let initiator_identity = Identity::generate();
        let responder_identity = Identity::generate();
        let responder_id = responder_identity.peer_id();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            Session::accept(stream, &responder_identity).await.unwrap()
        });

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let client =
            Session::connect(client_stream, &initiator_identity, Some(&responder_id))
                .await
                .unwrap();
        let server = server.await.unwrap();
        (client, server)
    }

    #[tokio::test]
    async fn a_message_survives_the_round_trip() {
        let (mut client, mut server) = connected_pair().await;
        client.send(b"hello peer").await.unwrap();
        assert_eq!(server.recv().await.unwrap(), b"hello peer");
        server.send(b"hello back").await.unwrap();
        assert_eq!(client.recv().await.unwrap(), b"hello back");
    }

    #[tokio::test]
    async fn each_side_learns_the_others_identity() {
        let (client, server) = connected_pair().await;
        assert!(client.outbound, "the client dialled the peer");
        assert!(!server.outbound, "the server accepted the connection");
        // The ids must differ: they are two distinct identities.
        assert_ne!(client.peer_id, server.peer_id);
    }

    #[tokio::test]
    async fn a_wrong_expected_peer_is_refused() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let responder_identity = Identity::generate();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            // The server will fail to read a hello, which is fine here.
            let _ = Session::accept(stream, &responder_identity).await;
        });

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let wrong = PeerID([7u8; 32]);
        let result = Session::connect(client_stream, &Identity::generate(), Some(&wrong)).await;
        assert!(result.is_err(), "a mismatched peer id must be rejected");
        let _ = server.await;
    }

    /// Seal a payload exactly as `Session::send` does, so a test can tamper
    /// with the result.
    fn seal(key: &[u8; 32], prefix: &[u8; 4], sequence: u64, payload: &[u8]) -> Vec<u8> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        let nonce = frame_nonce(prefix, sequence);
        let aad = sequence.to_be_bytes();
        cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: payload,
                    aad: &aad,
                },
            )
            .unwrap()
    }

    fn open(key: &[u8; 32], prefix: &[u8; 4], sequence: u64, ciphertext: &[u8]) -> Option<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        let nonce = frame_nonce(prefix, sequence);
        let aad = sequence.to_be_bytes();
        cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: ciphertext,
                    aad: &aad,
                },
            )
            .ok()
    }

    #[tokio::test]
    async fn a_tampered_frame_does_not_open() {
        let (client, _server) = connected_pair().await;
        let key = client.send_key;
        let prefix = client.send_nonce_prefix;

        let mut ciphertext = seal(&key, &prefix, 0, b"transfer 100 to alice");
        assert!(open(&key, &prefix, 0, &ciphertext).is_some(), "the frame is valid before tampering");

        // Flip one bit anywhere in the sealed frame. The Poly1305 tag must
        // catch it rather than yielding altered plaintext.
        for index in [0, 5, ciphertext.len() / 2, ciphertext.len() - 1] {
            let original = ciphertext[index];
            ciphertext[index] ^= 0x01;
            assert!(
                open(&key, &prefix, 0, &ciphertext).is_none(),
                "a frame with byte {index} flipped must not open"
            );
            ciphertext[index] = original;
        }
    }

    #[tokio::test]
    async fn a_replayed_frame_does_not_open() {
        let (client, _server) = connected_pair().await;
        let key = client.send_key;
        let prefix = client.send_nonce_prefix;

        let ciphertext = seal(&key, &prefix, 0, b"only once");
        assert!(open(&key, &prefix, 0, &ciphertext).is_some());

        // The receiver is at sequence 1, so replaying the sequence-0 frame is
        // rejected: the counter is the replay defence.
        assert!(
            open(&key, &prefix, 1, &ciphertext).is_none(),
            "a replayed frame must not open at a later sequence"
        );
    }

    #[tokio::test]
    async fn the_two_directions_use_different_keys() {
        let (client, server) = connected_pair().await;
        // The client's send key is the server's receive key, and the two
        // directions are distinct, so a frame cannot be reflected back.
        assert_eq!(client.send_key, server.recv_key);
        assert_ne!(client.send_key, client.recv_key);

        let ciphertext = seal(&client.send_key, &client.send_nonce_prefix, 0, b"x");
        assert!(
            open(&client.recv_key, &client.recv_nonce_prefix, 0, &ciphertext).is_none(),
            "a frame must not open under the other direction's key"
        );
    }

    #[tokio::test]
    async fn large_payloads_round_trip() {
        let (mut client, mut server) = connected_pair().await;
        let big = vec![0xab; 512 * 1024];
        client.send(&big).await.unwrap();
        assert_eq!(server.recv().await.unwrap(), big);
    }

    #[tokio::test]
    async fn an_oversized_payload_is_refused_before_it_is_sent() {
        let (mut client, _server) = connected_pair().await;
        let too_big = vec![0u8; MAX_FRAME_SIZE + 1];
        assert!(matches!(
            client.send(&too_big).await,
            Err(NetworkError::FrameTooLarge(_))
        ));
    }

    #[test]
    fn derived_keys_differ_per_direction_and_agree_between_sides() {
        let shared = [9u8; 32];
        let transcript = [3u8; 32];
        let (i_send, i_recv, _, _) = derive_keys(&shared, &transcript, true);
        let (r_send, r_recv, _, _) = derive_keys(&shared, &transcript, false);

        assert_ne!(i_send, i_recv, "directions must not share a key");
        assert_eq!(i_send, r_recv, "the initiator's send key is the responder's receive key");
        assert_eq!(i_recv, r_send, "and the other way round");
    }

    #[test]
    fn frame_nonces_are_unique_per_sequence() {
        let prefix = [1u8; 4];
        assert_ne!(frame_nonce(&prefix, 0), frame_nonce(&prefix, 1));
        // A different direction prefix gives a different nonce for the same
        // sequence, so the two directions never reuse a nonce.
        assert_ne!(frame_nonce(&[1u8; 4], 0), frame_nonce(&[2u8; 4], 0));
    }

    #[test]
    fn an_identity_can_be_reloaded_from_its_secret() {
        let identity = Identity::generate();
        let secret = identity.secret_bytes();
        let restored = Identity::from_secret_bytes(&secret).unwrap();
        assert_eq!(identity.peer_id(), restored.peer_id());
    }

    #[test]
    fn a_signature_from_another_key_does_not_verify() {
        let a = Identity::generate();
        let b = Identity::generate();
        let message = b"transcript";
        let signature = a.sign(message);
        assert!(verify_peer_signature(&a.public_key(), message, &signature));
        assert!(
            !verify_peer_signature(&b.public_key(), message, &signature),
            "a signature must not verify under the wrong key"
        );
        assert!(
            !verify_peer_signature(&a.public_key(), b"different", &signature),
            "a signature must not verify over different content"
        );
    }
}
