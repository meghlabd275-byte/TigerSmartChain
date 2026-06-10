// Package tigersmartchain provides Go SDK for TigerSmartChain.
package tigersmartchain

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"strings"
	"sync"
	"time"
)

// =============================================================================
// CLIENT
// =============================================================================

// Client represents a TigerSmartChain JSON-RPC client.
type Client struct {
	rpcURL  string
	httpClient *http.Client

	mu     sync.RWMutex
	nonce  map[string]uint64

	chainID uint64
}

// NewClient creates a new TigerSmartChain client.
func NewClient(rpcURL string) (*Client, error) {
	client := &Client{
		rpcURL: rpcURL,
		httpClient: &http.Client{
			Timeout: 30 * time.Second,
		},
		nonce: make(map[string]uint64),
		chainID: 9001,
	}

	// Verify connection
	if _, err := client.BlockNumber(context.Background()); err != nil {
		return nil, fmt.Errorf("failed to connect: %w", err)
	}

	return client, nil
}

// =============================================================================
// CHAIN METHODS
// =============================================================================

// BlockNumber returns the current block number.
func (c *Client) BlockNumber(ctx context.Context) (uint64, error) {
	var result string
	if err := c.call(ctx, "eth_blockNumber", nil, &result); err != nil {
		return 0, err
	}

	return parseUint64(result)
}

// GetChainID returns the chain ID.
func (c *Client) GetChainID(ctx context.Context) (uint64, error) {
	var result string
	if err := c.call(ctx, "eth_chainId", nil, &result); err != nil {
		return 0, err
	}

	return parseUint64(result)
}

// =============================================================================
// ACCOUNT METHODS
// =============================================================================

// GetBalance returns the balance of an address.
func (c *Client) GetBalance(ctx context.Context, address string) (*big.Int, error) {
	var result string
	if err := c.call(ctx, "eth_getBalance", []interface{}{address, "latest"}, &result); err != nil {
		return nil, err
	}

	return parseBigInt(result)
}

// GetTransactionCount returns the transaction count for an address.
func (c *Client) GetTransactionCount(ctx context.Context, address string) (uint64, error) {
	var result string
	if err := c.call(ctx, "eth_getTransactionCount", []interface{}{address, "latest"}, &result); err != nil {
		return 0, err
	}

	return parseUint64(result)
}

// GetCode returns the contract code at an address.
func (c *Client) GetCode(ctx context.Context, address string) ([]byte, error) {
	var result string
	if err := c.call(ctx, "eth_getCode", []interface{}{address, "latest"}, &result); err != nil {
		return nil, err
	}

	return hex.DecodeString(strings.TrimPrefix(result, "0x"))
}

// GetStorageAt returns the storage value at an address.
func (c *Client) GetStorageAt(ctx context.Context, address string, key string) ([]byte, error) {
	var result string
	if err := c.call(ctx, "eth_getStorageAt", []interface{}{address, key, "latest"}, &result); err != nil {
		return nil, err
	}

	return hex.DecodeString(strings.TrimPrefix(result, "0x"))
}

// =============================================================================
// TRANSACTION METHODS
// =============================================================================

// SendTransaction sends a transaction.
func (c *Client) SendTransaction(ctx context.Context, tx *Transaction) (string, error) {
	// Get nonce
	if tx.Nonce == 0 {
		nonce, err := c.GetTransactionCount(ctx, tx.From)
		if err != nil {
			return "", err
		}
		tx.Nonce = nonce
	}

	// Set chain ID
	if tx.ChainID == 0 {
		tx.ChainID = c.chainID
	}

	// Set gas price
	if tx.GasPrice == 0 {
		gasPrice, err := c.GasPrice(ctx)
		if err != nil {
			return "", err
		}
		tx.GasPrice = gasPrice
	}

	// Estimate gas if not set
	if tx.GasLimit == 0 {
		gasLimit, err := c.EstimateGas(ctx, tx)
		if err != nil {
			return "", err
		}
		tx.GasLimit = gasLimit
	}

	var result string
	if err := c.call(ctx, "eth_sendTransaction", tx.toParams(), &result); err != nil {
		return "", err
	}

	return result, nil
}

// Call executes a message call.
func (c *Client) Call(ctx context.Context, msg *CallRequest) (string, error) {
	var result string
	if err := c.call(ctx, "eth_call", msg.toParams(), &result); err != nil {
		return "", err
	}

	return result, nil
}

// GetTransactionByHash returns a transaction by hash.
func (c *Client) GetTransactionByHash(ctx context.Context, hash string) (*TransactionReceipt, error) {
	var result *TransactionReceipt
	if err := c.call(ctx, "eth_getTransactionByHash", []interface{}{hash}, &result); err != nil {
		return nil, err
	}

	return result, nil
}

// GetTransactionReceipt returns the receipt for a transaction.
func (c *Client) GetTransactionReceipt(ctx context.Context, hash string) (*TransactionReceipt, error) {
	var result *TransactionReceipt
	if err := c.call(ctx, "eth_getTransactionReceipt", []interface{}{hash}, &result); err != nil {
		return nil, err
	}

	return result, nil
}

// WaitForTransaction waits for a transaction to be mined.
func (c *Client) WaitForTransaction(ctx context.Context, hash string, timeout time.Duration) (*TransactionReceipt, error) {
	ticker := time.NewTicker(time.Second)
	defer ticker.Stop()

	deadline := time.Now().Add(timeout)

	for {
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-ticker.C:
			receipt, err := c.GetTransactionReceipt(ctx, hash)
			if err != nil {
				continue
			}
			if receipt != nil && receipt.BlockNumber != "" {
				return receipt, nil
			}
			if time.Now().After(deadline) {
				return nil, fmt.Errorf("timeout waiting for transaction")
			}
		}
	}
}

// =============================================================================
// GAS METHODS
// =============================================================================

// GasPrice returns the current gas price.
func (c *Client) GasPrice(ctx context.Context) (uint64, error) {
	var result string
	if err := c.call(ctx, "eth_gasPrice", nil, &result); err != nil {
		return 0, err
	}

	return parseUint64(result)
}

// EstimateGas estimates gas for a transaction.
func (c *Client) EstimateGas(ctx context.Context, tx *Transaction) (uint64, error) {
	var result string
	if err := c.call(ctx, "eth_estimateGas", tx.toParams(), &result); err != nil {
		return 0, err
	}

	return parseUint64(result)
}

// =============================================================================
// BLOCK METHODS
// =============================================================================

// GetBlockByNumber returns a block by number.
func (c *Client) GetBlockByNumber(ctx context.Context, number uint64, full bool) (*Block, error) {
	var result *Block
	tag := fmt.Sprintf("0x%x", number)
	if err := c.call(ctx, "eth_getBlockByNumber", []interface{}{tag, full}, &result); err != nil {
		return nil, err
	}

	return result, nil
}

// GetBlockByHash returns a block by hash.
func (c *Client) GetBlockByHash(ctx context.Context, hash string, full bool) (*Block, error) {
	var result *Block
	if err := c.call(ctx, "eth_getBlockByHash", []interface{}{hash, full}, &result); err != nil {
		return nil, err
	}

	return result, nil
}

// =============================================================================
// EVENT LOGS
// =============================================================================

// GetLogs returns logs matching the filter.
func (c *Client) GetLogs(ctx context.Context, filter *FilterRequest) ([]Log, error) {
	var result []Log
	if err := c.call(ctx, "eth_getLogs", filter.toParams(), &result); err != nil {
		return nil, err
	}

	return result, nil
}

// NewFilter creates a new filter.
func (c *Client) NewFilter(ctx context.Context, filter *FilterRequest) (string, error) {
	var result string
	if err := c.call(ctx, "eth_newFilter", filter.toParams(), &result); err != nil {
		return "", err
	}

	return result, nil
}

// GetFilterChanges returns changes for a filter.
func (c *Client) GetFilterChanges(ctx context.Context, filterID string) ([]Log, error) {
	var result []Log
	if err := c.call(ctx, "eth_getFilterChanges", []interface{}{filterID}, &result); err != nil {
		return nil, err
	}

	return result, nil
}

// =============================================================================
// RAW RPC
// =============================================================================

// call makes a JSON-RPC call.
func (c *Client) call(ctx context.Context, method string, params interface{}, result interface{}) error {
	req := &RPCRequest{
		JSONRPC: "2.0",
		Method:  method,
		ID:     1,
	}

	if params != nil {
		req.Params = params
	}

	resp, err := c.httpClient.Post(c.rpcURL, "application/json", req.toReader())
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	rpcResp := &RPCResponse{}
	if err := json.NewDecoder(resp.Body).Decode(rpcResp); err != nil {
		return err
	}

	if rpcResp.Error.Code != 0 {
		return fmt.Errorf("RPC error: %s", rpcResp.Error.Message)
	}

	data, err := json.Marshal(rpcResp.Result)
	if err != nil {
		return err
	}

	return json.Unmarshal(data, result)
}

// =============================================================================
// DATA STRUCTURES
// =============================================================================

// Transaction represents a transaction.
type Transaction struct {
	From     string
	To       string
	Value    *big.Int
	Data     []byte
	GasLimit uint64
	GasPrice uint64
	Nonce    uint64
	ChainID  uint64
}

// CallRequest represents a call request.
type CallRequest struct {
	From  string  `json:"from,omitempty"`
	To    string  `json:"to"`
	Value string  `json:"value,omitempty"`
	Data  string  `json:"data,omitempty"`
}

// FilterRequest represents a filter request.
type FilterRequest struct {
	FromBlock string   `json:"fromBlock,omitempty"`
	ToBlock  string   `json:"toBlock,omitempty"`
	Address  string   `json:"address,omitempty"`
	Topics   []string `json:"topics,omitempty"`
}

// Block represents a block.
type Block struct {
	Number       string   `json:"number"`
	Hash         string   `json:"hash"`
	ParentHash   string   `json:"parentHash"`
	Nonce       string   `json:"nonce"`
	Timestamp   string   `json:"timestamp"`
	Transactions []Transaction `json:"transactions"`
}

// TransactionReceipt represents a transaction receipt.
type TransactionReceipt struct {
	TransactionHash  string `json:"transactionHash"`
	BlockNumber    string `json:"blockNumber"`
	BlockHash     string `json:"blockHash"`
	CumulativeGasUsed string `json:"cumulativeGasUsed"`
	GasUsed      string `json:"gasUsed"`
	Status       string `json:"status"`
}

// Log represents an event log.
type Log struct {
	Address string   `json:"address"`
	Topics  []string `json:"topics"`
	Data    string   `json:"data"`
	BlockNumber string `json:"blockNumber"`
}

// RPCRequest represents an RPC request.
type RPCRequest struct {
	JSONRPC string      `json:"jsonrpc"`
	Method string      `json:"method"`
	Params interface{} `json:"params,omitempty"`
	ID     int         `json:"id"`
}

// RPCResponse represents an RPC response.
type RPCResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	Result interface{}     `json:"result,omitempty"`
	Error  RPCError        `json:"error,omitempty"`
	ID     int             `json:"id"`
}

// RPCError represents an RPC error.
type RPCError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

// =============================================================================
// MARSHALING
// =============================================================================

func (tx *Transaction) toParams() []interface{} {
	params := map[string]interface{}{
		"from": tx.From,
		"to":   tx.To,
	}

	if tx.Value != nil {
		params["value"] = fmt.Sprintf("0x%x", tx.Value)
	}
	if len(tx.Data) > 0 {
		params["data"] = fmt.Sprintf("0x%x", tx.Data)
	}
	if tx.GasLimit > 0 {
		params["gas"] = fmt.Sprintf("0x%x", tx.GasLimit)
	}
	if tx.GasPrice > 0 {
		params["gasPrice"] = fmt.Sprintf("0x%x", tx.GasPrice)
	}
	if tx.Nonce > 0 {
		params["nonce"] = fmt.Sprintf("0x%x", tx.Nonce)
	}
	if tx.ChainID > 0 {
		params["chainId"] = fmt.Sprintf("0x%x", tx.ChainID)
	}

	return []interface{}{params}
}

func (cr *CallRequest) toParams() []interface{} {
	return []interface{}{cr}
}

func (fr *FilterRequest) toParams() []interface{} {
	return []interface{}{fr}
}

func (r *RPCRequest) toReader() *strings.Reader {
	data, _ := json.Marshal(r)
	return strings.NewReader(string(data))
}

// =============================================================================
// PARSING
// =============================================================================

func parseUint64(s string) (uint64, error) {
	s = strings.TrimPrefix(s, "0x")
	if s == "" {
		return 0, nil
	}
	return fmt.Sscanf(s, "%x", new(uint64))
}

func parseBigInt(s string) (*big.Int, error) {
	s = strings.TrimPrefix(s, "0x")
	if s == "" {
		return big.NewInt(0), nil
	}

	n := new(big.Int)
	_, err := fmt.Sscanf(s, "%x", n)
	return n, err
}

// =============================================================================
// INIT
// =============================================================================

func init() {
	_ = fmt.Sprintf
	_ = hex.Encode
}