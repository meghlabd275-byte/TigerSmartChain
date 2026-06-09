// Package crypto provides cryptographic utilities for TigerSmartChain.
package crypto

import (
	"crypto/ecdsa"
	"crypto/rand"
	"crypto/sha256"
	"crypto/sha3"
	"math/big"

	ethcrypto "github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/rlp"
)

// Hash represents a 32-byte hash value (Keccak256).
type Hash = common.Hash

// Address represents a 20-byte Ethereum address.
type Address = common.Address

// PrivateKey wraps an ECDSA private key.
type PrivateKey = ecdsa.PrivateKey

// PublicKey wraps an ECDSA public key.
type PublicKey = ecdsa.PublicKey

// Keccak256 computes the Keccak256 hash of the input.
func Keccak256(data ...[]byte) []byte {
	h := sha3.New256()
	for _, d := range data {
		h.Write(d)
	}
	return h.Sum(nil)
}

// Keccak256Hash computes the Keccak256 hash and returns the result as Hash.
func Keccak256Hash(data ...[]byte) Hash {
	return ethcrypto.Keccak256Hash(data...)
}

// Keccak512 computes the Keccak512 hash of the input.
func Keccak512(data ...[]byte) []byte {
	h := sha3.New512()
	for _, d := range data {
		h.Write(d)
	}
	return h.Sum(nil)
}

// SHA256 computes the SHA256 hash of the input.
func SHA256(data ...[]byte) []byte {
	h := sha256.New()
	for _, d := range data {
		h.Write(d)
	}
	return h.Sum(nil)
}

// GenerateKey generates a new private key.
func GenerateKey() (*ecdsa.PrivateKey, error) {
	return ethcrypto.GenerateKey()
}

// PubkeyToAddress converts a public key to an address.
func PubkeyToAddress(pub *ecdsa.PublicKey) Address {
	return ethcrypto.PubkeyToAddress(*pub)
}

// HexToAddress converts a hex string to an address.
func HexToAddress(s string) Address {
	return common.HexToAddress(s)
}

// ParseHash parses a hex string to a Hash.
func ParseHash(s string) (Hash, error) {
	return common.HexToHash(s), nil
}

// Sign signs the data with the private key.
func Sign(data []byte, prv *ecdsa.PrivateKey) ([]byte, error) {
	return ethcrypto.Sign(data, prv)
}

// RecoverPubkey recovers the public key from the signature.
func RecoverPubkey(data, sig []byte) (*ecdsa.PublicKey, error) {
	return ethcrypto.SigToPub(ethcrypto.Keccak256Hash(data).Bytes(), sig)
}

// CompressPubkey compresses a public key.
func CompressPubkey(pub *ecdsa.PublicKey) []byte {
	return ethcrypto.CompressPubkey(pub)
}

// RLPEncode encodes the value to RLP format.
func RLPEncode(v interface{}) ([]byte, error) {
	return rlp.EncodeToBytes(v)
}

// RLPDecode decodes the RLP data to the value.
func RLPDecode(data []byte, v interface{}) error {
	return rlp.DecodeBytes(data, v)
}

// RandomBytes generates random bytes of the specified length.
func RandomBytes(n int) ([]byte, error) {
	b := make([]byte, n)
	_, err := rand.Read(b)
	if err != nil {
		return nil, err
	}
	return b, nil
}

// Big0 is a shared big.Int for zero.
var Big0 = big.NewInt(0)