// Package block provides core block types for TigerSmartChain.
package block

import (
	"math/big"

	"github.com/tigersmartchain/tigersmartchain/pkg/crypto"
	"github.com/tigersmartchain/tigersmartchain/pkg/types"
)

// Block represents a complete block in the TigerSmartChain blockchain.
type Block struct {
	Header *Header
	Body   *Body
}

// Header contains the metadata for a block.
type Header struct {
	ParentHash  crypto.Hash
	Coinbase   types.Address
	Root       crypto.Hash
	TxHash     crypto.Hash
	ReceiptHash crypto.Hash
	Bloom      types.Bloom
	Difficulty *big.Int
	Number     *big.Int
	GasLimit   uint64
	GasUsed    uint64
	Time       uint64
	Extra      []byte
	MixDigest  crypto.Hash
	Nonce     uint64
}

// Body contains the transactions for a block.
type Body struct {
	Transactions []interface{}
	Uncles      []crypto.Hash
}

// NewBlock creates a new block.
func NewBlock(header *Header, txs []interface{}) *Block {
	return &Block{
		Header: header,
		Body: &Body{
			Transactions: txs,
			Uncles:     []crypto.Hash{},
		},
	}
}

// BlockHeader returns the block header.
func (b *Block) BlockHeader() *Header {
	return b.Header
}

// BlockBody returns the block body.
func (b *Block) BlockBody() *Body {
	return b.Body
}

// Hash returns the block hash.
func (b *Block) Hash() crypto.Hash {
	return crypto.Keccak256Hash()
}