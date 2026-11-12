"""Wallet support for the TigerSmartChain Python SDK.

The implementations live in `crypto.py`; this module re-exports them under the
names the SDK has historically used, so existing callers keep working while
there is a single implementation of each primitive.

An earlier version of this file defined its own HD wallet and transaction
builder. Those did not work: the file did not parse, `private_to_public`
returned a hash of the private key rather than the secp256k1 public key,
addresses were derived from the private key instead of the public key, and
`sign_transaction` appended random bytes instead of producing a signature. Any
funds sent to an address it generated would have been unspendable.
"""

from __future__ import annotations

from typing import Dict, List, Optional, Tuple

from .crypto import (
    COIN_TYPE_TSC,
    DEFAULT_DERIVATION_PATH,
    HDAccount,
    HDWallet,
    KeyStore,
    MnemonicPhrase,
    MultiSigWallet,
    TransactionBuilder,
    create_keystore,
    derive_address,
    generate_mnemonic,
    generate_private_key,
    is_checksum_address,
    is_valid_address,
    keccak256,
    mnemonic_to_seed,
    private_key_to_address,
    private_key_to_public_key,
    public_key_to_address,
    recover_address,
    sign_message,
    sign_transaction,
    validate_mnemonic,
    verify_message,
)

__all__ = [
    "COIN_TYPE_TSC",
    "DEFAULT_DERIVATION_PATH",
    "HDWallet",
    "HDAccount",
    "KeyStore",
    "MnemonicPhrase",
    "MultiSigWallet",
    "TransactionBuilder",
    "WalletFactory",
    "create_keystore",
    "derive_address",
    "encrypt_aes_ctr",
    "decrypt_aes_ctr",
    "generate_mnemonic",
    "generate_private_key",
    "is_checksum_address",
    "is_valid_address",
    "keccak256",
    "mnemonic_to_seed",
    "pbkdf2",
    "private_key_to_address",
    "private_key_to_public_key",
    "public_key_to_address",
    "recover_address",
    "scrypt_kdf",
    "sha3_256",
    "sign_message",
    "sign_transaction",
    "validate_mnemonic",
    "verify_message",
]


# ---------------------------------------------------------------------------
# Names kept for callers that imported the old helpers directly
# ---------------------------------------------------------------------------


def sha3_256(data: bytes) -> bytes:
    """Keccak-256, which is what Ethereum means by "sha3".

    FIPS-202 SHA3-256 differs in one padding byte and produces a different
    digest, so the two are not interchangeable. This returns the Ethereum one.
    """
    return keccak256(data)


def pbkdf2(password, salt: bytes, iterations: int, keylen: int) -> bytes:
    """PBKDF2-HMAC-SHA512.

    Note the order of the arguments: the historical signature here put the
    iteration count before the salt, which is the opposite of `hashlib`. The
    callers in this repository use this order, so it is preserved.
    """
    import hashlib

    material = password.encode() if isinstance(password, str) else bytes(password)
    return hashlib.pbkdf2_hmac("sha512", material, salt, iterations, dklen=keylen)


def scrypt_kdf(password, salt: bytes, n: int = 262144, r: int = 8, p: int = 1,
               keylen: int = 32) -> bytes:
    """scrypt key derivation."""
    import hashlib

    material = password.encode() if isinstance(password, str) else bytes(password)
    return hashlib.scrypt(material, salt=salt, n=n, r=r, p=p, dklen=keylen, maxmem=1 << 30)


def encrypt_aes_ctr(data: bytes, key: bytes, iv: bytes) -> bytes:
    """AES-128-CTR encryption with the IV as the initial counter block."""
    from Crypto.Cipher import AES
    from Crypto.Util import Counter

    counter = Counter.new(128, initial_value=int.from_bytes(iv, "big"))
    return AES.new(key, AES.MODE_CTR, counter=counter).encrypt(data)


def decrypt_aes_ctr(data: bytes, key: bytes, iv: bytes) -> bytes:
    """AES-128-CTR decryption."""
    from Crypto.Cipher import AES
    from Crypto.Util import Counter

    counter = Counter.new(128, initial_value=int.from_bytes(iv, "big"))
    return AES.new(key, AES.MODE_CTR, counter=counter).decrypt(data)


class WalletFactory:
    """Constructs the wallet types."""

    @staticmethod
    def create_hd_wallet(mnemonic: Optional[str] = None, passphrase: str = "",
                         word_count: int = 12) -> HDWallet:
        """A deterministic wallet, from a phrase or a fresh one."""
        return HDWallet(mnemonic=mnemonic, passphrase=passphrase, word_count=word_count)

    @staticmethod
    def create_keystore(private_key, password: str) -> KeyStore:
        """An encrypted keystore holding a private key."""
        return create_keystore(private_key, password)

    @staticmethod
    def create_multisig(owners: List[str], required: int) -> MultiSigWallet:
        """An m-of-n multi-signature wallet."""
        return MultiSigWallet(owners, required)
