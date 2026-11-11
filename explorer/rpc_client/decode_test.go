package rpc_client

import (
	"encoding/json"
	"math/big"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
)

// Realistic eth_getBlockByNumber result, trimmed to the fields the decoder
// consumes. Values are internally consistent so that a mis-parse is visible.
const blockJSON = `{
  "number": "0x10",
  "hash": "0x7b6f8a5c9d2e1f3a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2",
  "parentHash": "0x1111111111111111111111111111111111111111111111111111111111111111",
  "nonce": "0x0000000000000042",
  "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
  "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  "transactionsRoot": "0x2222222222222222222222222222222222222222222222222222222222222222",
  "stateRoot": "0x3333333333333333333333333333333333333333333333333333333333333333",
  "receiptsRoot": "0x4444444444444444444444444444444444444444444444444444444444444444",
  "miner": "0x5555555555555555555555555555555555555555",
  "difficulty": "0x0",
  "totalDifficulty": "0x0",
  "extraData": "0xd883010a0d846765746888676f312e32332e36856c696e7578",
  "size": "0x2a1",
  "gasLimit": "0x1c9c380",
  "gasUsed": "0x5208",
  "timestamp": "0x65000000",
  "transactions": [
    {
      "hash": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "nonce": "0x0",
      "blockHash": "0x7b6f8a5c9d2e1f3a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2",
      "blockNumber": "0x10",
      "transactionIndex": "0x0",
      "from": "0x6666666666666666666666666666666666666666",
      "to": "0x7777777777777777777777777777777777777777",
      "value": "0xde0b6b3a7640000",
      "gas": "0x5208",
      "gasPrice": "0x3b9aca00",
      "input": "0x",
      "type": "0x0",
      "chainId": "0x1a0a",
      "v": "0x3437",
      "r": "0x1",
      "s": "0x1"
    }
  ],
  "uncles": []
}`

func TestDecodeBlock(t *testing.T) {
	var result map[string]interface{}
	if err := json.Unmarshal([]byte(blockJSON), &result); err != nil {
		t.Fatalf("unmarshal fixture: %v", err)
	}

	block, err := decodeBlock(result)
	if err != nil {
		t.Fatalf("decodeBlock: %v", err)
	}

	if got := block.NumberU64(); got != 16 {
		t.Errorf("NumberU64 = %d, want 16", got)
	}
	if got := block.GasLimit(); got != 30000000 {
		t.Errorf("GasLimit = %d, want 30000000", got)
	}
	if got := block.GasUsed(); got != 21000 {
		t.Errorf("GasUsed = %d, want 21000", got)
	}
	if got := block.Time(); got != 0x65000000 {
		t.Errorf("Time = %d, want %d", got, 0x65000000)
	}
	if got := block.Nonce(); got != 0x42 {
		t.Errorf("Nonce = %d, want 66", got)
	}
	if got := block.Coinbase(); got != common.HexToAddress("0x5555555555555555555555555555555555555555") {
		t.Errorf("Coinbase = %s, want 0x5555...5555", got)
	}
	if n := len(block.Transactions()); n != 1 {
		t.Fatalf("transaction count = %d, want 1", n)
	}

	tx := block.Transactions()[0]
	if tx.Nonce() != 0 {
		t.Errorf("tx nonce = %d, want 0", tx.Nonce())
	}
	if got := tx.Value(); got.Cmp(big.NewInt(1_000_000_000_000_000_000)) != 0 {
		t.Errorf("tx value = %s, want 1e18", got)
	}
	if got := tx.To(); got == nil || *got != common.HexToAddress("0x7777777777777777777777777777777777777777") {
		t.Errorf("tx to = %v, want 0x7777...7777", got)
	}
	// chainId 6666 is TigerSmartChain's id.
	if got := tx.ChainId(); got == nil || got.Cmp(big.NewInt(6666)) != 0 {
		t.Errorf("tx chainId = %v, want 6666", got)
	}
}

// A block whose transactions arrive as bare hashes must be rejected: silently
// returning an empty body would undercount transactions everywhere downstream.
func TestDecodeBlockRejectsHashedTransactions(t *testing.T) {
	result := map[string]interface{}{
		"number":           "0x1",
		"parentHash":       "0x1111111111111111111111111111111111111111111111111111111111111111",
		"sha3Uncles":       "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
		"logsBloom":        "0x" + repeat("00", 256),
		"transactionsRoot": "0x2222222222222222222222222222222222222222222222222222222222222222",
		"stateRoot":        "0x3333333333333333333333333333333333333333333333333333333333333333",
		"receiptsRoot":     "0x4444444444444444444444444444444444444444444444444444444444444444",
		"miner":            "0x5555555555555555555555555555555555555555",
		"difficulty":       "0x0",
		"extraData":        "0x",
		"gasLimit":         "0x1c9c380",
		"gasUsed":          "0x0",
		"timestamp":        "0x1",
		"transactions": []interface{}{
			"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
		},
		"uncles": []interface{}{},
	}

	if _, err := decodeBlock(result); err == nil {
		t.Fatal("decodeBlock accepted hashed transactions, want an error")
	}
}

func TestDecodeBlockRejectsNil(t *testing.T) {
	if _, err := decodeBlock(nil); err == nil {
		t.Fatal("decodeBlock(nil) returned no error")
	}
}

const receiptJSON = `{
  "transactionHash": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "transactionIndex": "0x1",
  "blockHash": "0x7b6f8a5c9d2e1f3a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2",
  "blockNumber": "0x10",
  "from": "0x6666666666666666666666666666666666666666",
  "to": "0x7777777777777777777777777777777777777777",
  "cumulativeGasUsed": "0x5208",
  "gasUsed": "0x5208",
  "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  "contractAddress": null,
  "logs": [
    {
      "address": "0x7777777777777777777777777777777777777777",
      "topics": [
        "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
      ],
      "data": "0x0000000000000000000000000000000000000000000000000de0b6b3a7640000",
      "blockNumber": "0x10",
      "transactionHash": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "transactionIndex": "0x1",
      "blockHash": "0x7b6f8a5c9d2e1f3a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2",
      "logIndex": "0x0",
      "removed": false
    }
  ],
  "status": "0x1",
  "effectiveGasPrice": "0x3b9aca00",
  "type": "0x2"
}`

func TestDecodeReceipt(t *testing.T) {
	var result map[string]interface{}
	if err := json.Unmarshal([]byte(receiptJSON), &result); err != nil {
		t.Fatalf("unmarshal fixture: %v", err)
	}

	receipt, err := decodeReceipt(result)
	if err != nil {
		t.Fatalf("decodeReceipt: %v", err)
	}

	if receipt.Status != types.ReceiptStatusSuccessful {
		t.Errorf("Status = %d, want %d", receipt.Status, types.ReceiptStatusSuccessful)
	}
	if receipt.GasUsed != 21000 {
		t.Errorf("GasUsed = %d, want 21000", receipt.GasUsed)
	}
	if receipt.CumulativeGasUsed != 21000 {
		t.Errorf("CumulativeGasUsed = %d, want 21000", receipt.CumulativeGasUsed)
	}
	if n := len(receipt.Logs); n != 1 {
		t.Fatalf("log count = %d, want 1", n)
	}
	if got := receipt.Logs[0].Topics[0]; got != common.HexToHash("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef") {
		t.Errorf("log topic[0] = %s, want the ERC-20 Transfer signature", got)
	}
	if got := receipt.TxHash; got != common.HexToHash("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa") {
		t.Errorf("TxHash = %s, want 0xaaaa...aaaa", got)
	}
	if receipt.BlockNumber == nil || receipt.BlockNumber.Uint64() != 16 {
		t.Errorf("BlockNumber = %v, want 16", receipt.BlockNumber)
	}
}

func TestDecodeReceiptRejectsNil(t *testing.T) {
	if _, err := decodeReceipt(nil); err == nil {
		t.Fatal("decodeReceipt(nil) returned no error")
	}
}

const txJSON = `{
  "hash": "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "nonce": "0x7",
  "blockHash": "0x7b6f8a5c9d2e1f3a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2",
  "blockNumber": "0x10",
  "transactionIndex": "0x2",
  "from": "0x6666666666666666666666666666666666666666",
  "to": "0x7777777777777777777777777777777777777777",
  "value": "0x0",
  "gas": "0x5208",
  "gasPrice": "0x3b9aca00",
  "input": "0xa9059cbb",
  "type": "0x0",
  "chainId": "0x1a0a",
  "v": "0x3437",
  "r": "0x1",
  "s": "0x1"
}`

func TestDecodeTransaction(t *testing.T) {
	var result map[string]interface{}
	if err := json.Unmarshal([]byte(txJSON), &result); err != nil {
		t.Fatalf("unmarshal fixture: %v", err)
	}

	tx, err := decodeTransaction(result)
	if err != nil {
		t.Fatalf("decodeTransaction: %v", err)
	}

	if tx.Nonce() != 7 {
		t.Errorf("Nonce = %d, want 7", tx.Nonce())
	}
	if tx.Gas() != 21000 {
		t.Errorf("Gas = %d, want 21000", tx.Gas())
	}
	if tx.Value().Sign() != 0 {
		t.Errorf("Value = %s, want 0", tx.Value())
	}
	if got := tx.Data(); len(got) != 4 || got[0] != 0xa9 {
		t.Errorf("Data = %x, want the 4-byte transfer selector", got)
	}
	if got := tx.ChainId(); got == nil || got.Cmp(big.NewInt(6666)) != 0 {
		t.Errorf("ChainId = %v, want 6666", got)
	}
}

func TestDecodeTransactionRejectsNil(t *testing.T) {
	if _, err := decodeTransaction(nil); err == nil {
		t.Fatal("decodeTransaction(nil) returned no error")
	}
}

func repeat(s string, n int) string {
	out := make([]byte, 0, len(s)*n)
	for i := 0; i < n; i++ {
		out = append(out, s...)
	}
	return string(out)
}
