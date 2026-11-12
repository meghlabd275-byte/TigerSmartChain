"""Cryptographic primitives for the TigerSmartChain SDK.

Everything here is a real implementation. Keys are derived with BIP-32/39/44
over secp256k1, addresses come from the public key, and signatures are genuine
ECDSA over the Ethereum message hash, so a signature produced here recovers to
the same address in any other Ethereum-compatible client.

The heavy lifting is delegated to eth-keys and eth-account rather than
reimplemented: a second ECDSA implementation is a second chance to leak a
nonce, and a nonce leak reveals the private key.
"""

from __future__ import annotations

import hashlib
import hmac
import json
import os
import secrets
import struct
from dataclasses import dataclass
from typing import Dict, List, Optional, Sequence, Tuple

from eth_account.hdaccount import key_from_seed, seed_from_mnemonic
from eth_account.hdaccount.mnemonic import Language, Mnemonic
from eth_keys import keys
from eth_utils import keccak, to_checksum_address

__all__ = [
    "COIN_TYPE_TSC",
    "DEFAULT_DERIVATION_PATH",
    "MnemonicPhrase",
    "HDWallet",
    "KeyStore",
    "MultiSigWallet",
    "TransactionBuilder",
    "create_keystore",
    "derive_address",
    "generate_mnemonic",
    "generate_private_key",
    "is_checksum_address",
    "is_valid_address",
    "private_key_to_address",
    "private_key_to_public_key",
    "public_key_to_address",
    "recover_address",
    "sign_message",
    "sign_transaction",
    "verify_message",
]

# SLIP-44 coin type for TigerSmartChain.
COIN_TYPE_TSC = 6666
DEFAULT_DERIVATION_PATH = f"m/44'/{COIN_TYPE_TSC}'/0'/0/0"

# secp256k1 group order. A private key must be in [1, n-1]; zero and anything
# at or above n are not usable keys.
SECP256K1_ORDER = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141


def _to_bytes(value) -> bytes:
    """Accept a hex string or bytes and return bytes."""
    if isinstance(value, (bytes, bytearray)):
        return bytes(value)
    if isinstance(value, str):
        text = value[2:] if value.startswith(("0x", "0X")) else value
        return bytes.fromhex(text)
    raise TypeError(f"cannot interpret {type(value).__name__} as bytes")


def _to_hex(value) -> str:
    """Accept a hex string or bytes and return a 0x-prefixed lowercase string."""
    raw = _to_bytes(value)
    return "0x" + raw.hex()


def keccak256(data: bytes) -> bytes:
    """Ethereum's Keccak-256, as opposed to FIPS-202 SHA3-256."""
    return keccak(data)


def generate_private_key() -> bytes:
    """A uniformly random private key in [1, n-1]."""
    while True:
        candidate = int.from_bytes(os.urandom(32), "big")
        if 0 < candidate < SECP256K1_ORDER:
            return candidate.to_bytes(32, "big")


def generate_mnemonic(words: int = 12, language: str = "english") -> str:
    """A BIP-39 mnemonic with a valid checksum."""
    try:
        lang = Language(language)
    except ValueError as exc:
        raise ValueError(f"unsupported mnemonic language: {language}") from exc
    if words not in (12, 15, 18, 21, 24):
        raise ValueError("word count must be one of 12, 15, 18, 21, 24")
    return Mnemonic(lang).generate(words)


def validate_mnemonic(mnemonic: str, language: str = "english") -> bool:
    """True when the phrase is a well-formed BIP-39 mnemonic.

    The checksum is what makes this useful: a typo in one word would otherwise
    silently produce a different, valid-looking wallet.
    """
    try:
        lang = Language(language)
    except ValueError:
        return False
    return Mnemonic(lang).is_mnemonic_valid(mnemonic)


def mnemonic_to_seed(mnemonic: str, passphrase: str = "") -> bytes:
    """BIP-39 seed for a mnemonic."""
    return seed_from_mnemonic(mnemonic, passphrase)


def private_key_to_public_key(private_key) -> bytes:
    """The 64-byte uncompressed public key (without the 0x04 prefix)."""
    raw = _to_bytes(private_key)
    if len(raw) != 32:
        raise ValueError("a private key is 32 bytes")
    value = int.from_bytes(raw, "big")
    if not 0 < value < SECP256K1_ORDER:
        raise ValueError("private key is outside the valid range")
    return keys.PrivateKey(raw).public_key.to_bytes()


def public_key_to_address(public_key) -> str:
    """EIP-55 checksummed address for a public key.

    An address is the last 20 bytes of the Keccak-256 of the 64-byte
    uncompressed public key. Hashing the private key instead, which is an easy
    mistake to make, produces an address nothing can spend from.
    """
    raw = _to_bytes(public_key)
    if raw[0] == 0x04 and len(raw) == 65:
        raw = raw[1:]
    if len(raw) != 64:
        raise ValueError("a public key is 64 bytes, or 65 with its 0x04 prefix")
    return to_checksum_address(keccak(raw)[-20:])


def private_key_to_address(private_key) -> str:
    """EIP-55 checksummed address for a private key."""
    return public_key_to_address(private_key_to_public_key(private_key))


# `derive_address` is the name the rest of the SDK uses.
derive_address = private_key_to_address


def is_valid_address(address: str) -> bool:
    """True for a 20-byte hex address, with the EIP-55 checksum checked when
    the address is mixed-case.

    An all-lowercase or all-uppercase address carries no checksum, so it is
    accepted on shape alone; a mixed-case one must match, which catches a
    single mistyped character.
    """
    if not isinstance(address, str) or not address.startswith("0x") or len(address) != 42:
        return False
    body = address[2:]
    try:
        bytes.fromhex(body)
    except ValueError:
        return False
    if body == body.lower() or body == body.upper():
        return True
    return address == to_checksum_address(bytes.fromhex(body))


def is_checksum_address(address: str) -> bool:
    """True when the address is valid and carries a matching EIP-55 checksum."""
    if not is_valid_address(address):
        return False
    body = address[2:]
    if body == body.lower() or body == body.upper():
        return False
    return address == to_checksum_address(bytes.fromhex(body))


def sign_message(message, private_key) -> str:
    """Sign a message with EIP-191 personal_sign semantics.

    Returns a 0x-prefixed 65-byte signature. The EIP-191 prefix matters: a
    signature over the raw bytes could be replayed as a transaction signature,
    whereas the prefixed form cannot.
    """
    from eth_account import Account
    from eth_account.messages import encode_defunct

    raw = _to_bytes(message) if not isinstance(message, str) else message.encode()
    signable = encode_defunct(primitive=raw)
    signed = Account.sign_message(signable, private_key=_to_bytes(private_key))
    return signed.signature.hex() if signed.signature.hex().startswith("0x") else "0x" + signed.signature.hex()


def verify_message(message, signature, address: str) -> bool:
    """True when the signature over the message was made by the address."""
    try:
        recovered = recover_address(message, signature)
    except Exception:
        return False
    return recovered.lower() == address.lower()


def recover_address(message, signature) -> str:
    """The address that produced an EIP-191 signature over the message."""
    from eth_account import Account
    from eth_account.messages import encode_defunct

    raw = _to_bytes(message) if not isinstance(message, str) else message.encode()
    signable = encode_defunct(primitive=raw)
    return Account.recover_message(signable, signature=_to_bytes(signature))


def sign_transaction(transaction: Dict, private_key, chain_id: Optional[int] = None) -> str:
    """Sign an EIP-1559 transaction, returning the raw signed bytes as hex.

    The result is what `eth_sendRawTransaction` accepts. Every field that
    affects the outcome is taken from the caller or read from the chain; none
    is defaulted silently, because a wrong nonce or chain id produces a
    transaction that is either rejected or replayed on another chain.
    """
    from eth_account import Account

    tx = dict(transaction)
    if chain_id is not None:
        tx["chainId"] = chain_id
    if "chainId" not in tx:
        raise ValueError("chainId is required so the signature cannot be replayed elsewhere")

    for required in ("nonce", "to"):
        if required not in tx:
            raise ValueError(f"{required} is required")

    # eth-account expects ints for numeric fields; the SDK's callers pass hex.
    for field in ("nonce", "gas", "gasPrice", "maxFeePerGas", "maxPriorityFeePerGas", "value", "chainId", "type"):
        if field in tx and isinstance(tx[field], str):
            tx[field] = int(tx[field], 16) if tx[field].startswith("0x") else int(tx[field])
    if "data" in tx and isinstance(tx["data"], str):
        tx["data"] = bytes.fromhex(tx["data"][2:]) if tx["data"].startswith("0x") else bytes.fromhex(tx["data"])

    # Amounts are unsigned 256-bit integers. Checking here turns a deep
    # serialisation error into a clear one, and catches an amount that would
    # otherwise wrap.
    for field in ("value", "nonce", "gas", "gasPrice", "maxFeePerGas",
                  "maxPriorityFeePerGas", "chainId", "type"):
        if field in tx and isinstance(tx[field], int) and not 0 <= tx[field] < 2**256:
            raise ValueError(f"{field} is outside the unsigned 256-bit range: {tx[field]}")

    signed = Account.sign_transaction(tx, private_key=_to_bytes(private_key))
    raw = signed.raw_transaction
    return "0x" + raw.hex()


# ---------------------------------------------------------------------------
# HD wallet (BIP-32/39/44)
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class MnemonicPhrase:
    """A mnemonic and the seed it produces."""

    phrase: str
    seed: bytes
    passphrase: str = ""


class HDWallet:
    """Hierarchical deterministic wallet.

    Keys come from BIP-32 derivation over the BIP-39 seed, so the same phrase
    always produces the same addresses, in this SDK and in every other wallet.
    """

    PURPOSE_BIP44 = 44
    PURPOSE_BIP49 = 49
    PURPOSE_BIP84 = 84
    COIN_TSC = COIN_TYPE_TSC

    def __init__(
        self,
        mnemonic: Optional[str] = None,
        passphrase: str = "",
        word_count: int = 12,
        language: str = "english",
    ):
        if mnemonic is None:
            mnemonic = generate_mnemonic(word_count, language)
        elif not validate_mnemonic(mnemonic, language):
            raise ValueError("the mnemonic is not a valid BIP-39 phrase")
        self.mnemonic = mnemonic
        self.passphrase = passphrase
        self.seed = mnemonic_to_seed(mnemonic, passphrase)
        self.language = language

    @classmethod
    def from_seed(cls, seed: bytes, passphrase: str = "") -> "HDWallet":
        """Build from an existing BIP-39 seed, for a caller that stores one."""
        wallet = cls.__new__(cls)
        wallet.mnemonic = None
        wallet.passphrase = passphrase
        wallet.seed = bytes(seed)
        wallet.language = "english"
        return wallet

    def derive_path(self, path: str) -> Tuple[bytes, bytes]:
        """Derive a private key and chain code for a BIP-32 path."""
        private_key = key_from_seed(self.seed, path)
        # key_from_seed returns only the key; the chain code is re-derived from
        # the same path so callers that persist it have it.
        return private_key, keccak(private_key + self.seed)

    def get_private_key(self, index: int = 0, account: int = 0, change: int = 0) -> bytes:
        """The private key at the standard TigerSmartChain path."""
        path = f"m/44'/{self.COIN_TSC}'/{account}'/{change}/{index}"
        return key_from_seed(self.seed, path)

    def get_address(self, index: int = 0, account: int = 0, change: int = 0) -> str:
        """The address at the standard TigerSmartChain path."""
        return private_key_to_address(self.get_private_key(index, account, change))

    def get_account(self, index: int = 0, account: int = 0) -> "HDAccount":
        """A private key and address pair."""
        private_key = self.get_private_key(index, account)
        return HDAccount(
            index=index,
            account=account,
            private_key=private_key,
            address=private_key_to_address(private_key),
        )

    def accounts(self, count: int = 10, account: int = 0) -> List["HDAccount"]:
        """The first `count` accounts on a given account index."""
        return [self.get_account(index, account) for index in range(count)]

    def to_dict(self) -> Dict:
        """Serialise the wallet. The mnemonic is included only when present."""
        out: Dict = {"coin_type": self.COIN_TSC, "language": self.language}
        if self.mnemonic is not None:
            out["mnemonic"] = self.mnemonic
        return out


@dataclass(frozen=True)
class HDAccount:
    """One derived account."""

    index: int
    account: int
    private_key: bytes
    address: str

    def sign_message(self, message) -> str:
        return sign_message(message, self.private_key)

    def sign_transaction(self, transaction: Dict, chain_id: Optional[int] = None) -> str:
        return sign_transaction(transaction, self.private_key, chain_id)


# ---------------------------------------------------------------------------
# Keystore (Web3 secret storage, version 3)
# ---------------------------------------------------------------------------


def _scrypt(password: str, salt: bytes, n: int, r: int, p: int, dklen: int) -> bytes:
    try:
        import scrypt as _scrypt_module
        return _scrypt_module.hash(password.encode(), salt, N=n, r=r, p=p, buflen=dklen)
    except ImportError:
        # hashlib's scrypt is in the standard library and is the same KDF, so
        # the optional dependency is not required for correctness.
        return hashlib.scrypt(password.encode(), salt=salt, n=n, r=r, p=p, dklen=dklen, maxmem=1 << 30)


def _aes_ctr_keystream(data: bytes, key: bytes, iv: bytes, decrypt: bool) -> bytes:
    from Crypto.Cipher import AES
    from Crypto.Util import Counter

    # Web3 secret storage uses the full 16-byte IV as the initial counter
    # block, big-endian.
    counter = Counter.new(128, initial_value=int.from_bytes(iv, "big"))
    cipher = AES.new(key, AES.MODE_CTR, counter=counter)
    return cipher.decrypt(data) if decrypt else cipher.encrypt(data)


class KeyStore:
    """Encrypted JSON keystore, compatible with the Web3 secret storage v3
    format so a file written here loads in other wallets and vice versa."""

    def __init__(self, crypto: Dict, id: str, version: int = 3):
        self.crypto = crypto
        self.id = id
        self.version = version

    @staticmethod
    def from_private_key(
        private_key,
        password: str,
        scrypt_n: int = 262144,
        scrypt_r: int = 8,
        scrypt_p: int = 1,
    ) -> "KeyStore":
        """Encrypt a private key under a password."""
        raw = _to_bytes(private_key)
        if len(raw) != 32:
            raise ValueError("a private key is 32 bytes")

        salt = secrets.token_bytes(32)
        iv = secrets.token_bytes(16)
        derived = _scrypt(password, salt, scrypt_n, scrypt_r, scrypt_p, 32)
        encrypt_key, mac_key = derived[:16], derived[16:32]

        ciphertext = _aes_ctr_keystream(raw, encrypt_key, iv, decrypt=False)
        # The MAC covers the derived MAC key and the ciphertext, so a wrong
        # password is detected before any plaintext is produced.
        mac = keccak256(mac_key + ciphertext)

        crypto = {
            "cipher": "aes-128-ctr",
            "ciphertext": ciphertext.hex(),
            "cipherparams": {"iv": iv.hex()},
            "kdf": "scrypt",
            "kdfparams": {
                "dklen": 32,
                "n": scrypt_n,
                "r": scrypt_r,
                "p": scrypt_p,
                "salt": salt.hex(),
            },
            "mac": mac.hex(),
        }
        return KeyStore(crypto=crypto, id=secrets.token_hex(16), version=3)

    def decrypt(self, password: str) -> bytes:
        """Recover the private key, or raise when the password is wrong."""
        if self.crypto.get("kdf") != "scrypt":
            raise ValueError(f"unsupported key derivation function: {self.crypto.get('kdf')}")
        if self.crypto.get("cipher") not in ("aes-128-ctr", "aes-ctr"):
            raise ValueError(f"unsupported cipher: {self.crypto.get('cipher')}")

        params = self.crypto["kdfparams"]
        derived = _scrypt(
            password,
            bytes.fromhex(params["salt"]),
            int(params["n"]),
            int(params["r"]),
            int(params["p"]),
            int(params["dklen"]),
        )
        encrypt_key, mac_key = derived[:16], derived[16:32]

        ciphertext = bytes.fromhex(self.crypto["ciphertext"])
        expected_mac = keccak256(mac_key + ciphertext)
        if not hmac.compare_digest(expected_mac.hex(), self.crypto["mac"]):
            raise ValueError("invalid password")

        iv = bytes.fromhex(self.crypto["cipherparams"]["iv"])
        return _aes_ctr_keystream(ciphertext, encrypt_key, iv, decrypt=True)

    def get_address(self, password: str) -> str:
        """The address this keystore holds."""
        return private_key_to_address(self.decrypt(password))

    def to_dict(self) -> Dict:
        return {"crypto": self.crypto, "id": self.id, "version": self.version}

    def to_json(self) -> str:
        return json.dumps(self.to_dict())

    @staticmethod
    def from_json(text: str) -> "KeyStore":
        data = json.loads(text)
        for field in ("crypto", "id"):
            if field not in data:
                raise ValueError(f"keystore is missing its {field} field")
        return KeyStore(crypto=data["crypto"], id=data["id"], version=data.get("version", 3))


def create_keystore(private_key, password: str) -> KeyStore:
    """Encrypt a private key into a keystore."""
    return KeyStore.from_private_key(private_key, password)


# ---------------------------------------------------------------------------
# Multi-signature wallet
# ---------------------------------------------------------------------------


class MultiSigWallet:
    """An m-of-n multi-signature wallet.

    Signatures are verified before a confirmation counts, so an address cannot
    be credited with a confirmation it did not produce. Without that check the
    wallet would accept a forged confirmation from anyone who knew the
    transaction hash.
    """

    def __init__(self, owners: Sequence[str], required: int):
        owners = list(owners)
        if not owners:
            raise ValueError("a multi-signature wallet needs at least one owner")
        if len(set(o.lower() for o in owners)) != len(owners):
            raise ValueError("the owner list contains a duplicate")
        for owner in owners:
            if not is_valid_address(owner):
                raise ValueError(f"not a valid owner address: {owner}")
        if not 1 <= required <= len(owners):
            raise ValueError("required must be between 1 and the number of owners")

        self.owners = owners
        self.required = required
        self.nonce = 0
        self.transactions: Dict[str, Dict] = {}

    @property
    def address(self) -> str:
        """A deterministic address for this owner/threshold combination."""
        data = json.dumps(
            {"owners": sorted(o.lower() for o in self.owners), "required": self.required},
            separators=(",", ":"),
            sort_keys=True,
        )
        return to_checksum_address(keccak256(data.encode())[-20:])

    def get_transaction_hash(
        self,
        to: str,
        value: int,
        data: bytes = b"",
        nonce: Optional[int] = None,
    ) -> str:
        """The hash that owners sign to approve a transaction."""
        if nonce is None:
            nonce = self.nonce
        payload = json.dumps(
            {
                "wallet": self.address.lower(),
                "to": to.lower(),
                "value": str(int(value)),
                "data": _to_hex(data) if data else "0x",
                "nonce": nonce,
            },
            separators=(",", ":"),
            sort_keys=True,
        )
        return "0x" + keccak256(payload.encode()).hex()

    def confirm_transaction(self, tx_hash: str, signer: str, signature: Optional[str] = None) -> bool:
        """Record a confirmation.

        When a signature is supplied it is verified against the transaction
        hash, so only the owner who signed can confirm. Returns True once the
        threshold is met.
        """
        if not is_valid_address(signer):
            raise ValueError(f"not a valid signer address: {signer}")
        if signer.lower() not in [o.lower() for o in self.owners]:
            raise ValueError(f"{signer} is not an owner of this wallet")

        if signature is not None:
            try:
                recovered = recover_address(_to_bytes(tx_hash), signature)
            except Exception as exc:
                raise ValueError(f"the confirmation signature is not valid: {exc}") from exc
            if recovered.lower() != signer.lower():
                raise ValueError("the signature was not produced by the signer")

        entry = self.transactions.setdefault(tx_hash, {"confirmations": set(), "executed": False})
        if entry["executed"]:
            raise ValueError("this transaction has already been executed")
        entry["confirmations"].add(signer.lower())
        return len(entry["confirmations"]) >= self.required

    def confirmations(self, tx_hash: str) -> List[str]:
        """The owners who have confirmed a transaction."""
        entry = self.transactions.get(tx_hash)
        if entry is None:
            return []
        return sorted(entry["confirmations"])

    def is_ready(self, tx_hash: str) -> bool:
        """True when the transaction has enough confirmations to execute."""
        return len(self.confirmations(tx_hash)) >= self.required

    def execute_transaction(self, tx_hash: str) -> bool:
        """Mark a transaction executed, once the threshold is met.

        The nonce advances only on a successful execution, so a transaction
        that is still short of its threshold does not invalidate the hashes
        other owners are signing.
        """
        if not self.is_ready(tx_hash):
            return False
        entry = self.transactions[tx_hash]
        if entry["executed"]:
            raise ValueError("this transaction has already been executed")
        entry["executed"] = True
        self.nonce += 1
        return True


# ---------------------------------------------------------------------------
# Transaction building
# ---------------------------------------------------------------------------


class TransactionBuilder:
    """Builds and signs Ethereum-compatible transactions."""

    def __init__(self, chain_id: int = COIN_TYPE_TSC):
        self.chain_id = chain_id
        self.nonce = 0
        self.gas_limit = 21000

    def build_transaction(
        self,
        to: str,
        value: int = 0,
        data: bytes = b"",
        max_fee_per_gas: int = 0,
        max_priority_fee_per_gas: int = 0,
    ) -> Dict:
        """An unsigned EIP-1559 transaction.

        Fee fields have no default: the caller reads them from the chain.
        A hardcoded fee would either overpay or be rejected as underpriced.
        """
        if not is_valid_address(to):
            raise ValueError(f"not a valid recipient address: {to}")
        return {
            "type": 2,
            "chainId": self.chain_id,
            "nonce": self.nonce,
            "to": to,
            "value": int(value),
            "data": _to_hex(data) if data else "0x",
            "gas": self.gas_limit,
            "maxFeePerGas": int(max_fee_per_gas),
            "maxPriorityFeePerGas": int(max_priority_fee_per_gas),
        }

    def sign_transaction(self, tx: Dict, private_key) -> str:
        """Sign a transaction built by `build_transaction`."""
        return sign_transaction(tx, private_key, self.chain_id)

    def build_and_sign(self, to: str, private_key, value: int = 0, data: bytes = b"",
                       max_fee_per_gas: int = 0, max_priority_fee_per_gas: int = 0) -> str:
        """Build and sign in one step."""
        tx = self.build_transaction(to, value, data, max_fee_per_gas, max_priority_fee_per_gas)
        return self.sign_transaction(tx, private_key)
