// Package receipt provides block receipt and log management.
package receipt

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"sync"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/rlp"
)

// Receipt represents transaction receipt.
type Receipt struct {
	PostState         common.Hash `json:"postState"`         // Root of state trie after tx
	BlockNumber       uint64      `json:"blockNumber"`      // Block number
	BlockHash         common.Hash `json:"blockHash"`        // Block hash
	TransactionIndex  uint        `json:"transactionIndex"` // Transaction index in block
	CumulativeGasUsed uint64      `json:"cumulativeGasUsed"` // Total gas used in block
	GasUsed           uint64      `json:"gasUsed"`          // Gas used by this tx
	ContractAddress   *common.Address `json:"contractAddress"` // Created contract address
	Status            uint64      `json:"status"`            // 1 = success, 0 = failure
	Logs              []*Log      `json:"logs"`             // Event logs
	Bloom             Bloom       `json:"bloom"`            // Bloom filter
}

// Log represents an event log.
type Log struct {
	Address     common.Address   `json:"address"`     // Contract address
	Topics      []common.Hash   `json:"topics"`      // Event topics
	Data        []byte          `json:"data"`        // Event data
	BlockNumber uint64         `json:"blockNumber"` // Block number
	BlockHash   common.Hash    `json:"blockHash"`   // Block hash
	TxHash      common.Hash    `json:"transactionHash"` // Transaction hash
	TxIndex     uint           `json:"transactionIndex"` // Transaction index
	LogIndex    uint           `json:"logIndex"`    // Log index in block
	Removed     bool           `json:"removed"`     // Removed due to reorg
}

// Bloom represents bloom filter.
type Bloom [256]byte

// RLP encoding for receipt
func (r *Receipt) EncodeRLP(w *rlp.Stream) error {
	return rlp.Encode(w, []interface{}{
		r.PostState,
		r.CumulativeGasUsed,
		r.LogsBloom(),
		r.Status,
		r.GasUsed,
		r.ContractAddress,
		r.Logs,
	})
}

// RLP decoding for receipt
func (r *Receipt) DecodeRLP(s *rlp.Stream) error {
	var raw struct {
		PostState         common.Hash
		CumulativeGasUsed uint64
		Bloom            Bloom
		Status           uint64
		GasUsed          uint64
		ContractAddress  *common.Address
		Logs             []*Log
	}

	if err := s.Decode(&raw); err != nil {
		return err
	}

	r.PostState = raw.PostState
	r.CumulativeGasUsed = raw.CumulativeGasUsed
	r.Bloom = raw.Bloom
	r.Status = raw.Status
	r.GasUsed = raw.GasUsed
	r.ContractAddress = raw.ContractAddress
	r.Logs = raw.Logs

	return nil
}

// LogsBloom creates bloom filter from logs.
func (r *Receipt) LogsBloom() Bloom {
	var bloom Bloom

	for _, log := range r.Logs {
		// Add address to bloom
		bloom.add(log.Address.Bytes())

		// Add topics to bloom
		for _, topic := range log.Topics {
			bloom.add(topic.Bytes())
		}
	}

	return bloom
}

// Add adds log to receipt.
func (r *Receipt) Add(log *Log) {
	r.Logs = append(r.Logs, log)
	r.Bloom = r.LogsBloom()
}

// MarshalJSON marshals receipt to JSON.
func (r *Receipt) MarshalJSON() ([]byte, error) {
	type ReceiptJSON struct {
		PostState         string     `json:"postState"`
		BlockNumber       string     `json:"blockNumber"`
		BlockHash         string     `json:"blockHash"`
		TransactionIndex  string     `json:"transactionIndex"`
		CumulativeGasUsed string     `json:"cumulativeGasUsed"`
		GasUsed          string     `json:"gasUsed"`
		ContractAddress  string     `json:"contractAddress"`
		Status           string     `json:"status"`
		Logs             []*Log     `json:"logs"`
		Bloom            string     `json:"bloom"`
	}

	return json.Marshal(ReceiptJSON{
		PostState:         r.PostState.Hex(),
		BlockNumber:       fmt.Sprintf("0x%x", r.BlockNumber),
		BlockHash:        r.BlockHash.Hex(),
		TransactionIndex: fmt.Sprintf("0x%x", r.TransactionIndex),
		CumulativeGasUsed: fmt.Sprintf("0x%x", r.CumulativeGasUsed),
		GasUsed:          fmt.Sprintf("0x%x", r.GasUsed),
		ContractAddress:  r.ContractAddress.Hex(),
		Status:           fmt.Sprintf("0x%x", r.Status),
		Logs:             r.Logs,
		Bloom:            r.Bloom.Hex(),
	})
}

// UnmarshalJSON unmarshals receipt from JSON.
func (r *Receipt) UnmarshalJSON(data []byte) error {
	type ReceiptJSON struct {
		PostState         string     `json:"postState"`
		BlockNumber       string     `json:"blockNumber"`
		BlockHash         string     `json:"blockHash"`
		TransactionIndex  string     `json:"transactionIndex"`
		CumulativeGasUsed string     `json:"cumulativeGasUsed"`
		GasUsed          string     `json:"gasUsed"`
		ContractAddress  string     `json:"contractAddress"`
		Status           string     `json:"status"`
		Logs             []*Log     `json:"logs"`
		Bloom            string     `json:"bloom"`
	}

	var raw ReceiptJSON
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}

	r.PostState = common.HexToHash(raw.PostState)
	r.BlockNumber = common.HexToUint64(raw.BlockNumber)
	r.BlockHash = common.HexToHash(raw.BlockHash)
	r.TransactionIndex = uint(common.HexToUint64(raw.TransactionIndex))
	r.CumulativeGasUsed = common.HexToUint64(raw.CumulativeGasUsed)
	r.GasUsed = common.HexToUint64(raw.GasUsed)

	if raw.ContractAddress != "" && raw.ContractAddress != "0x" {
		addr := common.HexToAddress(raw.ContractAddress)
		r.ContractAddress = &addr
	}

	r.Status = common.HexToUint64(raw.Status)
	r.Logs = raw.Logs

	if raw.Bloom != "" {
		r.Bloom = common.HexToBloom(raw.Bloom)
	}

	return nil
}

// add adds bytes to bloom filter.
func (b *Bloom) add(data []byte) {
	hash := crypto.Keccak256(data)

	// Set bits based on hash
	for i := 0; i < 6; i++ {
		bitIndex := binary.BigEndian.Uint16(hash[i:i+2]) % 2048
		byteIndex := bitIndex / 8
		bitMask := byte(1 << (7 - bitIndex%8))
		b[byteIndex] |= bitMask
	}
}

// Hex returns bloom as hex string.
func (b Bloom) Hex() string {
	return common.Bytes2Hex(b[:])
}

// LogsBloom returns bloom filter for receipts.
func LogsBloom(receipts []*Receipt) Bloom {
	var bloom Bloom
	for _, receipt := range receipts {
		for _, log := range receipt.Logs {
			bloom.add(log.Address.Bytes())
			for _, topic := range log.Topics {
				bloom.add(topic.Bytes())
			}
		}
	}
	return bloom
}

// FilteredLogs returns logs matching the filter.
func FilteredLogs(receipts []*Receipt, filter *Filter) []*Log {
	var results []*Log

	for _, receipt := range receipts {
		for _, log := range receipt.Logs {
			if filter.Matches(log) {
				results = append(results, log)
			}
		}
	}

	return results
}

// Filter represents log filter.
type Filter struct {
	Addresses []common.Address
	Topics    [][]common.Hash
	FromBlock uint64
	ToBlock   uint64
}

// Matches returns true if log matches filter.
func (f *Filter) Matches(log *Log) bool {
	// Check block range
	if f.FromBlock > 0 && log.BlockNumber < f.FromBlock {
		return false
	}
	if f.ToBlock > 0 && log.BlockNumber > f.ToBlock {
		return false
	}

	// Check addresses
	if len(f.Addresses) > 0 {
		matched := false
		for _, addr := range f.Addresses {
			if log.Address == addr {
				matched = true
				break
			}
		}
		if !matched {
			return false
		}
	}

	// Check topics
	for i, topic := range f.Topics {
		if len(topic) > 0 {
			if i >= len(log.Topics) {
				return false
			}
			matched := false
			for _, t := range topic {
				if log.Topics[i] == t {
					matched = true
					break
				}
			}
			if !matched {
				return false
			}
		}
	}

	return true
}

// BloomLookup looks up blooms for addresses.
func BloomLookup(bloom Bloom, addr common.Address) bool {
	return bloom.check(addr.Bytes())
}

// check checks if address is in bloom.
func (b Bloom) check(data []byte) bool {
	hash := crypto.Keccak256(data)

	for i := 0; i < 6; i++ {
		bitIndex := binary.BigEndian.Uint16(hash[i:i+2]) % 2048
		byteIndex := bitIndex / 8
		bitMask := byte(1 << (7 - bitIndex%8))
		if (b[byteIndex] & bitMask) == 0 {
			return false
		}
	}

	return true
}

// Receipts represents receipt storage.
type Receipts struct {
	mu    sync.RWMutex
	data  map[common.Hash][]*Receipt
	byNum map[uint64][]*Receipt
}

// NewReceipts creates new receipts storage.
func NewReceipts() *Receipts {
	return &Receipts{
		data:  make(map[common.Hash][]*Receipt),
		byNum: make(map[uint64][]*Receipt),
	}
}

// Put stores receipts for a block.
func (r *Receipts) Put(blockHash common.Hash, blockNumber uint64, receipts []*Receipt) {
	r.mu.Lock()
	defer r.mu.Unlock()

	r.data[blockHash] = receipts
	r.byNum[blockNumber] = receipts
}

// GetByHash returns receipts by block hash.
func (r *Receipts) GetByHash(blockHash common.Hash) []*Receipt {
	r.mu.RLock()
	defer r.mu.RUnlock()

	return r.data[blockHash]
}

// GetByNumber returns receipts by block number.
func (r *Receipts) GetByNumber(blockNumber uint64) []*Receipt {
	r.mu.RLock()
	defer r.mu.RUnlock()

	return r.byNum[blockNumber]
}

// Delete removes receipts.
func (r *Receipts) Delete(blockHash common.Hash, blockNumber uint64) {
	r.mu.Lock()
	defer r.mu.Unlock()

	delete(r.data, blockHash)
	delete(r.byNum, blockNumber)
}

// Range iterates over receipts.
func (r *Receipts) Range(from, to uint64, fn func(uint64, []*Receipt) bool) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	for i := from; i <= to; i++ {
		receipts := r.byNum[i]
		if !fn(i, receipts) {
			break
		}
	}
}

var _ = json.Marshal
