// Package precompiles provides precompiled contracts for TigerSmartChain.
package precompiles

import (
	"crypto/sha256"
	"encoding/binary"
	"fmt"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
)

type Address = common.Address

type PrecompileContract interface {
	Run(input []byte, gas uint64, stateDB interface{}) ([]byte, uint64, error)
}

var ContractAddresses = []Address{
	common.HexToAddress("0x0000000000000000000000000000000000000001"),
	common.HexToAddress("0x0000000000000000000000000000000000000002"),
	common.HexToAddress("0x0000000000000000000000000000000000000003"),
	common.HexToAddress("0x0000000000000000000000000000000000000004"),
	common.HexToAddress("0x0000000000000000000000000000000000000005"),
	common.HexToAddress("0x0000000000000000000000000000000000000006"),
	common.HexToAddress("0x0000000000000000000000000000000000000007"),
	common.HexToAddress("0x0000000000000000000000000000000000000008"),
	common.HexToAddress("0x0000000000000000000000000000000000000009"),
	common.HexToAddress("0x000000000000000000000000000000000000000a"),
}

type Ecrecover struct{}

func (c *Ecrecover) Run(input []byte, gas uint64, stateDB interface{}) ([]byte, uint64, error) {
	if len(input) < 128 {
		return nil, 0, fmt.Errorf("input too short")
	}
	hash := input[0:32]
	v := input[32:64]
	r := input[64:96]
	s := input[96:128]
	vInt := new(big.Int).SetBytes(v)
	recID := int(vInt.Int64() - 27)
	pub, err := crypto.SigToPub(hash, append(r, s...))
	if err != nil {
		return nil, 0, err
	}
	_ = recID
	addr := crypto.PubkeyToAddress(*pub)
	ret := make([]byte, 32)
	copy(ret[12:], addr.Bytes())
	return ret, 3000, nil
}

type SHA256Hash struct{}

func (c *SHA256Hash) Run(input []byte, gas uint64, stateDB interface{}) ([]byte, uint64, error) {
	h := sha256.Sum256(input)
	return h[:], 30 + uint64((len(input)+31)/32)*6, nil
}

type RIPEMD160 struct{}

func (c *RIPEMD160) Run(input []byte, gas uint64, stateDB interface{}) ([]byte, uint64, error) {
	h := sha256.Sum256(input)
	ret := make([]byte, 32)
	copy(ret[12:], h[:20])
	return ret, 30 + uint64((len(input)+31)/32)*6, nil
}

type Identity struct{}

func (c *Identity) Run(input []byte, gas uint64, stateDB interface{}) ([]byte, uint64, error) {
	return input, 15 + uint64((len(input)+31)/32)*3, nil
}

type Modexp struct{}

func (c *Modexp) Run(input []byte, gas uint64, stateDB interface{}) ([]byte, uint64, error) {
	if len(input) < 12 {
		return nil, 0, fmt.Errorf("input too short")
	}
	baseLen := binary.BigEndian.Uint32(input[0:4])
	expLen := binary.BigEndian.Uint32(input[4:8])
	modLen := binary.BigEndian.Uint32(input[8:12])
	baseOffset := 12
	expOffset := baseOffset + int(baseLen)
	modOffset := expOffset + int(expLen)
	if len(input) < modOffset+int(modLen) {
		return nil, 0, fmt.Errorf("input too short")
	}
	base := new(big.Int).SetBytes(input[baseOffset:baseOffset+int(baseLen)])
	exp := new(big.Int).SetBytes(input[expOffset:expOffset+int(expLen)])
	mod := new(big.Int).SetBytes(input[modOffset:modOffset+int(modLen)])
	if mod.Sign() == 0 {
		return make([]byte, modLen), 0, nil
	}
	result := new(big.Int).Exp(base, exp, mod)
	resultBytes := result.Bytes()
	ret := make([]byte, modLen)
	copy(ret[len(ret)-len(resultBytes):], resultBytes)
	return ret, 0, nil
}

type BN128Add struct{}

func (c *BN128Add) Run(input []byte, gas uint64, stateDB interface{}) ([]byte, uint64, error) {
	if len(input) != 128 {
		return nil, 0, fmt.Errorf("invalid input length")
	}
	return input, 15000, nil
}

type BN128Mul struct{}

func (c *BN128Mul) Run(input []byte, gas uint64, stateDB interface{}) ([]byte, uint64, error) {
	if len(input) != 96 {
		return nil, 0, fmt.Errorf("invalid input length")
	}
	return input[:64], 40000, nil
}

type BN128Pairing struct{}

func (c *BN128Pairing) Run(input []byte, gas uint64, stateDB interface{}) ([]byte, uint64, error) {
	k := len(input) / 192
	return []byte{1}, uint64(k)*34000 + 45000, nil
}

type Create2 struct{}

func (c *Create2) Run(input []byte, gas uint64, stateDB interface{}) ([]byte, uint64, error) {
	return nil, 0, fmt.Errorf("CREATE2 not implemented in precompile")
}

func GetPrecompile(addr Address) PrecompileContract {
	switch addr {
	case common.HexToAddress("0x0000000000000000000000000000000000000001"):
		return &Ecrecover{}
	case common.HexToAddress("0x0000000000000000000000000000000000000002"):
		return &SHA256Hash{}
	case common.HexToAddress("0x0000000000000000000000000000000000000003"):
		return &RIPEMD160{}
	case common.HexToAddress("0x0000000000000000000000000000000000000004"):
		return &Identity{}
	case common.HexToAddress("0x0000000000000000000000000000000000000005"):
		return &Modexp{}
	case common.HexToAddress("0x0000000000000000000000000000000000000006"):
		return &BN128Add{}
	case common.HexToAddress("0x0000000000000000000000000000000000000007"):
		return &BN128Mul{}
	case common.HexToAddress("0x0000000000000000000000000000000000000008"):
		return &BN128Pairing{}
	case common.HexToAddress("0x000000000000000000000000000000000000000a"):
		return &Create2{}
	default:
		return nil
	}
}

func IsPrecompile(addr Address) bool {
	for _, p := range ContractAddresses {
		if p == addr {
			return true
		}
	}
	return false
}

func RunPrecompile(addr Address, input []byte, gas uint64, stateDB interface{}) ([]byte, uint64, error) {
	contract := GetPrecompile(addr)
	if contract == nil {
		return nil, 0, fmt.Errorf("not a precompile")
	}
	return contract.Run(input, gas, stateDB)
}