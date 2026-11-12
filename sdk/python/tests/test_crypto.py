"""Tests for the TigerSmartChain SDK cryptography.

The vectors here are externally published values, not values produced by this
code, so a change that breaks compatibility with other Ethereum tooling fails
the tests rather than being quietly baked in.
"""

import hashlib
import os
import sys

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from eth_account import Account  # noqa: E402
from eth_account.hdaccount import key_from_seed  # noqa: E402

from tigersmartchain import crypto  # noqa: E402

# The BIP-39 reference phrase from the Trezor test vectors. Its Ethereum
# address at m/44'/60'/0'/0/0 is the value every wallet in the ecosystem
# derives for it.
TREZOR_PHRASE = (
    "abandon abandon abandon abandon abandon abandon "
    "abandon abandon abandon abandon abandon about"
)
TREZOR_ETH_ADDRESS = "0x9858EfFD232B4033E47d90003D41EC34EcaEda94"

LEGAL_PHRASE = (
    "legal winner thank year wave sausage worth useful legal "
    "winner thank yellow"
)

# A small scrypt cost keeps the tests fast; the production default is used in
# the SDK, not here.
TEST_SCRYPT_N = 4096


class TestAddressDerivation:
    def test_matches_the_published_bip44_vector(self):
        seed = crypto.mnemonic_to_seed(TREZOR_PHRASE, "")
        private_key = key_from_seed(seed, "m/44'/60'/0'/0/0")
        assert crypto.private_key_to_address(private_key) == TREZOR_ETH_ADDRESS

    def test_address_is_not_the_zero_address(self):
        """The SDK used to return 0x000...0 for every key."""
        address = crypto.private_key_to_address(crypto.generate_private_key())
        assert address != "0x" + "0" * 40

    def test_address_is_the_last_20_bytes_of_the_public_key_hash(self):
        private_key = crypto.generate_private_key()
        public_key = crypto.private_key_to_public_key(private_key)
        expected = crypto.keccak256(public_key)[-20:]
        assert crypto.private_key_to_address(private_key).lower() == "0x" + expected.hex()

    def test_a_public_key_with_its_04_prefix_is_accepted(self):
        private_key = crypto.generate_private_key()
        bare = crypto.private_key_to_public_key(private_key)
        assert crypto.public_key_to_address(b"\x04" + bare) == crypto.public_key_to_address(bare)

    def test_a_wrong_length_public_key_is_rejected(self):
        with pytest.raises(ValueError):
            crypto.public_key_to_address(b"\x01" * 32)

    def test_the_zero_private_key_is_rejected(self):
        with pytest.raises(ValueError):
            crypto.private_key_to_address(b"\x00" * 32)

    def test_a_private_key_at_the_group_order_is_rejected(self):
        with pytest.raises(ValueError):
            crypto.private_key_to_address(crypto.SECP256K1_ORDER.to_bytes(32, "big"))


class TestHDWallet:
    def test_derivation_is_deterministic(self):
        wallet = crypto.HDWallet(mnemonic=TREZOR_PHRASE)
        assert wallet.get_address(0) == wallet.get_address(0)
        assert wallet.get_address(0) != wallet.get_address(1)

    def test_a_different_passphrase_gives_a_different_wallet(self):
        a = crypto.HDWallet(mnemonic=LEGAL_PHRASE, passphrase="")
        b = crypto.HDWallet(mnemonic=LEGAL_PHRASE, passphrase="TREZOR")
        assert a.get_address(0) != b.get_address(0)

    def test_a_generated_phrase_is_valid(self):
        assert crypto.validate_mnemonic(crypto.generate_mnemonic(12))

    def test_a_bad_checksum_is_rejected(self):
        """A typo must not silently produce a different wallet."""
        assert not crypto.validate_mnemonic(" ".join(["abandon"] * 12))

    def test_a_bad_phrase_raises_on_construction(self):
        with pytest.raises(ValueError):
            crypto.HDWallet(mnemonic=" ".join(["abandon"] * 12))

    def test_accounts_are_consistent_with_get_address(self):
        wallet = crypto.HDWallet(mnemonic=TREZOR_PHRASE)
        for account in wallet.accounts(3):
            assert account.address == wallet.get_address(account.index)
            assert account.address == crypto.private_key_to_address(account.private_key)


class TestSignatures:
    def test_a_signature_verifies(self):
        private_key = crypto.generate_private_key()
        address = crypto.private_key_to_address(private_key)
        signature = crypto.sign_message(b"hello", private_key)
        assert crypto.verify_message(b"hello", signature, address)

    def test_a_signature_over_a_different_message_does_not_verify(self):
        private_key = crypto.generate_private_key()
        address = crypto.private_key_to_address(private_key)
        signature = crypto.sign_message(b"hello", private_key)
        assert not crypto.verify_message(b"goodbye", signature, address)

    def test_a_signature_from_another_key_does_not_verify(self):
        private_key = crypto.generate_private_key()
        other = crypto.generate_private_key()
        signature = crypto.sign_message(b"hello", private_key)
        assert not crypto.verify_message(
            b"hello", signature, crypto.private_key_to_address(other)
        )

    def test_recovery_returns_the_signer(self):
        private_key = crypto.generate_private_key()
        address = crypto.private_key_to_address(private_key)
        signature = crypto.sign_message(b"payload", private_key)
        assert crypto.recover_address(b"payload", signature) == address

    def test_a_signature_is_65_bytes(self):
        private_key = crypto.generate_private_key()
        assert len(crypto.sign_message(b"x", private_key)) == 132  # 0x + 130 hex chars


class TestTransactionSigning:
    def test_a_signed_transaction_recovers_to_the_signer(self):
        private_key = crypto.generate_private_key()
        address = crypto.private_key_to_address(private_key)
        builder = crypto.TransactionBuilder(chain_id=crypto.COIN_TYPE_TSC)
        raw = builder.build_and_sign(
            to=address,
            private_key=private_key,
            value=10**18,
            max_fee_per_gas=10**9,
            max_priority_fee_per_gas=10**8,
        )
        assert Account.recover_transaction(raw).lower() == address.lower()

    def test_the_signed_transaction_is_eip1559_typed(self):
        private_key = crypto.generate_private_key()
        builder = crypto.TransactionBuilder(chain_id=crypto.COIN_TYPE_TSC)
        raw = builder.build_and_sign(
            to=crypto.private_key_to_address(private_key),
            private_key=private_key,
            max_fee_per_gas=10**9,
            max_priority_fee_per_gas=10**8,
        )
        assert raw.startswith("0x02")

    def test_signing_without_a_chain_id_is_refused(self):
        """Without a chain id the signature could be replayed on another chain."""
        private_key = crypto.generate_private_key()
        with pytest.raises(ValueError):
            crypto.sign_transaction({"nonce": 0, "to": "0x" + "11" * 20}, private_key)

    def test_an_invalid_recipient_is_refused(self):
        builder = crypto.TransactionBuilder()
        with pytest.raises(ValueError):
            builder.build_transaction(to="not-an-address")

    def test_a_negative_value_is_refused(self):
        builder = crypto.TransactionBuilder()
        tx = builder.build_transaction(to="0x" + "11" * 20)
        with pytest.raises(ValueError):
            crypto.sign_transaction({**tx, "value": -1}, crypto.generate_private_key())


class TestKeyStore:
    def test_round_trip(self):
        private_key = crypto.generate_private_key()
        store = crypto.KeyStore.from_private_key(private_key, "correct horse", scrypt_n=TEST_SCRYPT_N)
        assert store.decrypt("correct horse") == private_key

    def test_the_address_matches_the_key(self):
        private_key = crypto.generate_private_key()
        store = crypto.KeyStore.from_private_key(private_key, "pw", scrypt_n=TEST_SCRYPT_N)
        assert store.get_address("pw") == crypto.private_key_to_address(private_key)

    def test_a_wrong_password_is_rejected(self):
        store = crypto.KeyStore.from_private_key(
            crypto.generate_private_key(), "right", scrypt_n=TEST_SCRYPT_N
        )
        with pytest.raises(ValueError):
            store.decrypt("wrong")

    def test_a_tampered_ciphertext_is_detected(self):
        """The MAC must cover the ciphertext, or a flipped bit decrypts to a
        different key with no error."""
        store = crypto.KeyStore.from_private_key(
            crypto.generate_private_key(), "pw", scrypt_n=TEST_SCRYPT_N
        )
        tampered = {**store.crypto}
        ciphertext = bytearray(bytes.fromhex(tampered["ciphertext"]))
        ciphertext[0] ^= 0x01
        tampered["ciphertext"] = ciphertext.hex()
        broken = crypto.KeyStore(crypto=tampered, id=store.id)
        with pytest.raises(ValueError):
            broken.decrypt("pw")

    def test_json_round_trip(self):
        private_key = crypto.generate_private_key()
        store = crypto.KeyStore.from_private_key(private_key, "pw", scrypt_n=TEST_SCRYPT_N)
        restored = crypto.KeyStore.from_json(store.to_json())
        assert restored.decrypt("pw") == private_key

    def test_the_keystore_is_web3_secret_storage_shaped(self):
        store = crypto.KeyStore.from_private_key(
            crypto.generate_private_key(), "pw", scrypt_n=TEST_SCRYPT_N
        )
        assert store.version == 3
        assert store.crypto["cipher"] == "aes-128-ctr"
        assert store.crypto["kdf"] == "scrypt"
        for field in ("ciphertext", "cipherparams", "kdfparams", "mac"):
            assert field in store.crypto
        assert store.crypto["cipherparams"]["iv"]


class TestMultiSig:
    @staticmethod
    def _keys(count=3):
        """A mapping of address to private key for `count` fresh owners."""
        keys = {}
        while len(keys) < count:
            private_key = crypto.generate_private_key()
            keys[crypto.private_key_to_address(private_key)] = private_key
        return keys

    def test_the_threshold_must_be_reached(self):
        keys = self._keys()
        owners = list(keys)
        wallet = crypto.MultiSigWallet(owners, 2)
        tx_hash = wallet.get_transaction_hash(owners[0], 100)
        assert wallet.confirm_transaction(tx_hash, owners[0]) is False
        assert wallet.confirm_transaction(tx_hash, owners[1]) is True
        assert wallet.execute_transaction(tx_hash) is True

    def test_a_non_owner_cannot_confirm(self):
        owners = list(self._keys())
        outsider = crypto.private_key_to_address(crypto.generate_private_key())
        wallet = crypto.MultiSigWallet(owners, 1)
        tx_hash = wallet.get_transaction_hash(owners[0], 100)
        with pytest.raises(ValueError):
            wallet.confirm_transaction(tx_hash, outsider)

    def test_a_forged_signature_is_rejected(self):
        """A confirmation must be backed by a signature from the owner."""
        keys = self._keys()
        owners = list(keys)
        wallet = crypto.MultiSigWallet(owners, 1)
        tx_hash = wallet.get_transaction_hash(owners[0], 100)
        forged = crypto.sign_message(bytes.fromhex(tx_hash[2:]), crypto.generate_private_key())
        with pytest.raises(ValueError):
            wallet.confirm_transaction(tx_hash, owners[0], signature=forged)

    def test_a_genuine_signature_is_accepted(self):
        keys = self._keys()
        owners = list(keys)
        wallet = crypto.MultiSigWallet(owners, 1)
        tx_hash = wallet.get_transaction_hash(owners[0], 100)
        signature = crypto.sign_message(bytes.fromhex(tx_hash[2:]), keys[owners[0]])
        assert wallet.confirm_transaction(tx_hash, owners[0], signature=signature) is True

    def test_a_transaction_cannot_execute_twice(self):
        keys = self._keys()
        owners = list(keys)
        wallet = crypto.MultiSigWallet(owners, 1)
        tx_hash = wallet.get_transaction_hash(owners[0], 100)
        wallet.confirm_transaction(tx_hash, owners[0])
        assert wallet.execute_transaction(tx_hash) is True
        with pytest.raises(ValueError):
            wallet.execute_transaction(tx_hash)

    def test_the_nonce_advances_only_on_execution(self):
        keys = self._keys()
        owners = list(keys)
        wallet = crypto.MultiSigWallet(owners, 2)
        tx_hash = wallet.get_transaction_hash(owners[0], 100)
        wallet.confirm_transaction(tx_hash, owners[0])
        assert wallet.execute_transaction(tx_hash) is False
        assert wallet.nonce == 0
        wallet.confirm_transaction(tx_hash, owners[1])
        assert wallet.execute_transaction(tx_hash) is True
        assert wallet.nonce == 1

    def test_a_duplicate_owner_is_rejected(self):
        owner = crypto.private_key_to_address(crypto.generate_private_key())
        with pytest.raises(ValueError):
            crypto.MultiSigWallet([owner, owner], 1)

    def test_an_out_of_range_threshold_is_rejected(self):
        owners = list(self._keys())
        with pytest.raises(ValueError):
            crypto.MultiSigWallet(owners, 4)
        with pytest.raises(ValueError):
            crypto.MultiSigWallet(owners, 0)


class TestAddressValidation:
    def test_a_good_checksummed_address_is_valid(self):
        assert crypto.is_valid_address(TREZOR_ETH_ADDRESS)
        assert crypto.is_checksum_address(TREZOR_ETH_ADDRESS)

    def test_a_broken_checksum_is_rejected(self):
        last = TREZOR_ETH_ADDRESS[-1]
        broken = TREZOR_ETH_ADDRESS[:-1] + ("5" if last != "5" else "6")
        assert not crypto.is_valid_address(broken)

    def test_an_all_lowercase_address_is_accepted_without_a_checksum(self):
        assert crypto.is_valid_address(TREZOR_ETH_ADDRESS.lower())
        assert not crypto.is_checksum_address(TREZOR_ETH_ADDRESS.lower())

    def test_a_malformed_address_is_rejected(self):
        for bad in ("", "0x", "0x1234", "not an address", "0x" + "zz" * 20,
                    TREZOR_ETH_ADDRESS + "00"):
            assert not crypto.is_valid_address(bad)

    def test_a_short_address_is_rejected(self):
        assert not crypto.is_valid_address("0x9858EfFD232B4033E47d90003D41EC34EcaEd")


class TestKeccak:
    def test_the_empty_hash_matches_the_known_value(self):
        assert crypto.keccak256(b"").hex() == (
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        )

    def test_the_transfer_selector_matches_the_known_value(self):
        assert crypto.keccak256(b"transfer(address,uint256)")[:4].hex() == "a9059cbb"

    def test_keccak_is_not_fips_sha3(self):
        """The two differ in a padding byte; using the wrong one changes every
        hash the SDK produces."""
        assert crypto.keccak256(b"abc") != hashlib.sha3_256(b"abc").digest()
