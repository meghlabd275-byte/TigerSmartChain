// Package sdk provides multi-language SDK support for the explorer
package sdk

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
)

// SDKClient represents an SDK client
type SDKClient struct {
	APIKey    string
	BaseURL   string
	ChainID  uint64
	httpClient *HTTPClient
	mu       sync.RWMutex
}

// HTTPClient represents HTTP client interface
type HTTPClient struct {
	baseURL string
	apiKey string
}

// NewSDKClient creates new SDK client
func NewSDKClient(apiKey, baseURL string, chainID uint64) *SDKClient {
	return &SDKClient{
		APIKey:   apiKey,
		BaseURL:  baseURL,
		ChainID: chainID,
		httpClient: &HTTPClient{
			baseURL: baseURL,
			apiKey:  apiKey,
		},
	}
}

// GetBlock gets block by number or hash
func (c *SDKClient) GetBlock(blockNumberOrHash string) (*Block, error) {
	data, err := c.httpClient.Get(fmt.Sprintf("/blocks/%s", blockNumberOrHash))
	if err != nil {
		return nil, err
	}
	
	var block Block
	if err := json.Unmarshal(data, &block); err != nil {
		return nil, err
	}
	
	return &block, nil
}

// GetTransaction gets transaction by hash
func (c *SDKClient) GetTransaction(txHash string) (*Transaction, error) {
	data, err := c.httpClient.Get(fmt.Sprintf("/transactions/%s", txHash))
	if err != nil {
		return nil, err
	}
	
	var tx Transaction
	if err := json.Unmarshal(data, &tx); err != nil {
		return nil, err
	}
	
	return &tx, nil
}

// GetAccount gets account balance
func (c *SDKClient) GetAccount(address string) (*Account, error) {
	data, err := c.httpClient.Get(fmt.Sprintf("/accounts/%s", address))
	if err != nil {
		return nil, err
	}
	
	var account Account
	if err := json.Unmarshal(data, &account); err != nil {
		return nil, err
	}
	
	return &account, nil
}

// GetToken gets token info
func (c *SDKClient) GetToken(address string) (*Token, error) {
	data, err := c.httpClient.Get(fmt.Sprintf("/tokens/%s", address))
	if err != nil {
		return nil, err
	}
	
	var token Token
	if err := json.Unmarshal(data, &token); err != nil {
		return nil, err
	}
	
	return &token, nil
}

// GetTokenTransfers gets token transfers
func (c *SDKClient) GetTokenTransfers(address string, opts *QueryOpts) ([]*Transfer, error) {
	endpoint := fmt.Sprintf("/tokens/%s/transfers", address)
	if opts != nil {
		endpoint = addQueryParams(endpoint, opts)
	}
	
	data, err := c.httpClient.Get(endpoint)
	if err != nil {
		return nil, err
	}
	
	var transfers []*Transfer
	if err := json.Unmarshal(data, &transfers); err != nil {
		return nil, err
	}
	
	return transfers, nil
}

// GetTransactions gets transactions
func (c *SDKClient) GetTransactions(opts *QueryOpts) ([]*Transaction, error) {
	endpoint := "/transactions"
	if opts != nil {
		endpoint = addQueryParams(endpoint, opts)
	}
	
	data, err := c.httpClient.Get(endpoint)
	if err != nil {
		return nil, err
	}
	
	var txs []*Transaction
	if err := json.Unmarshal(data, &txs); err != nil {
		return nil, err
	}
	
	return txs, nil
}

// GetInternalTransactions gets internal transactions
func (c *SDKClient) GetInternalTransactions(txHash string) ([]*InternalTx, error) {
	data, err := c.httpClient.Get(fmt.Sprintf("/transactions/%s/internal", txHash))
	if err != nil {
		return nil, err
	}
	
	var txs []*InternalTx
	if err := json.Unmarshal(data, &txs); err != nil {
		return nil, err
	}
	
	return txs, nil
}

// GetLogs gets logs
func (c *SDKClient) GetLogs(opts *LogQueryOpts) ([]*Log, error) {
	data, err := c.httpClient.Post("/logs", opts)
	if err != nil {
		return nil, err
	}
	
	var logs []*Log
	if err := json.Unmarshal(data, &logs); err != nil {
		return nil, err
	}
	
	return logs, nil
}

// GetContractABI gets contract ABI
func (c *SDKClient) GetContractABI(address string) (string, error) {
	data, err := c.httpClient.Get(fmt.Sprintf("/contracts/%s/abi", address))
	if err != nil {
		return "", err
	}
	
	return string(data), nil
}

// ReadContract reads from contract
func (c *SDKClient) ReadContract(address, method string, params []interface{}) (interface{}, error) {
	payload := map[string]interface{}{
		"to":   address,
		"data": method,
	}
	
	data, err := c.httpClient.Post("/contracts/read", payload)
	if err != nil {
		return nil, err
	}
	
	var result map[string]interface{}
	if err := json.Unmarshal(data, &result); err != nil {
		return nil, err
	}
	
	return result["result"], nil
}

// WriteContract writes to contract
func (c *SDKClient) WriteContract(address, method string, params []interface{}, options *TxOptions) (string, error) {
	payload := map[string]interface{}{
		"to":    address,
		"data":  method,
		"value": options.Value,
		"gas":   options.GasLimit,
	}
	
	data, err := c.httpClient.Post("/contracts/write", payload)
	if err != nil {
		return "", err
	}
	
	var result map[string]interface{}
	if err := json.Unmarshal(data, &result); err != nil {
		return "", err
	}
	
	if hash, ok := result["hash"].(string); ok {
		return hash, nil
	}
	
	return "", fmt.Errorf("no transaction hash returned")
}

// EstimateGas estimates gas
func (c *SDKClient) EstimateGas(to, data string, value string) (uint64, error) {
	payload := map[string]interface{}{
		"to":   to,
		"data": data,
		"value": value,
	}
	
	dataBytes, err := c.httpClient.Post("/gas/estimate", payload)
	if err != nil {
		return 0, err
	}
	
	var result map[string]interface{}
	if err := json.Unmarshal(dataBytes, &result); err != nil {
		return 0, err
	}
	
	if gas, ok := result["gas"].(float64); ok {
		return uint64(gas), nil
	}
	
	return 21000, nil
}

// GetGasPrice gets current gas price
func (c *SDKClient) GetGasPrice() (*GasPrice, error) {
	data, err := c.httpClient.Get("/gas/prices")
	if err != nil {
		return nil, err
	}
	
	var gasPrice GasPrice
	if err := json.Unmarshal(data, &gasPrice); err != nil {
		return nil, err
	}
	
	return &gasPrice, nil
}

// GetBlockNumber gets current block number
func (c *SDKClient) GetBlockNumber() (uint64, error) {
	data, err := c.httpClient.Get("/blocks/latest/number")
	if err != nil {
		return 0, err
	}
	
	var result map[string]interface{}
	if err := json.Unmarshal(data, &result); err != nil {
		return 0, err
	}
	
	if num, ok := result["number"].(float64); ok {
		return uint64(num), nil
	}
	
	return 0, fmt.Errorf("invalid block number")
}

// GetNFTs gets NFTs for address
func (c *SDKClient) GetNFTs(address string, opts *QueryOpts) ([]*NFT, error) {
	endpoint := fmt.Sprintf("/accounts/%s/nfts", address)
	if opts != nil {
		endpoint = addQueryParams(endpoint, opts)
	}
	
	data, err := c.httpClient.Get(endpoint)
	if err != nil {
		return nil, err
	}
	
	var nfts []*NFT
	if err := json.Unmarshal(data, &nfts); err != nil {
		return nil, err
	}
	
	return nfts, nil
}

// Search searches blocks, transactions, addresses
func (c *SDKClient) Search(query string) (*SearchResult, error) {
	data, err := c.httpClient.Get(fmt.Sprintf("/search?q=%s", query))
	if err != nil {
		return nil, err
	}
	
	var result SearchResult
	if err := json.Unmarshal(data, &result); err != nil {
		return nil, err
	}
	
	return &result, nil
}

// GetTxReceipt gets transaction receipt
func (c *SDKClient) GetTxReceipt(txHash string) (*Receipt, error) {
	data, err := c.httpClient.Get(fmt.Sprintf("/transactions/%s/receipt", txHash))
	if err != nil {
		return nil, err
	}
	
	var receipt Receipt
	if err := json.Unmarshal(data, &receipt); err != nil {
		return nil, err
	}
	
	return &receipt, nil
}

// Subscribe subscribes to events (WebSocket)
func (c *SDKClient) Subscribe(ctx context.Context, channel string, handler func(interface{})) error {
	// WebSocket subscription would be implemented here
	return nil
}

// Types

type Block struct {
	Number       uint64        `json:"number"`
	Hash         string        `json:"hash"`
	ParentHash   string        `json:"parentHash"`
	Timestamp    uint64        `json:"timestamp"`
	Transactions []string      `json:"transactions"`
	GasUsed     uint64        `json:"gasUsed"`
	GasLimit    uint64        `json:"gasLimit"`
	Miner       string        `json:"miner"`
	Difficulty  string        `json:"difficulty"`
	TotalDifficulty string    `json:"totalDifficulty"`
	Size        uint64        `json:"size"`
}

type Transaction struct {
	Hash       string `json:"hash"`
	BlockNumber uint64 `json:"blockNumber"`
	BlockHash  string `json:"blockHash"`
	From      string `json:"from"`
	To        string `json:"to"`
	Value     string `json:"value"`
	GasPrice  string `json:"gasPrice"`
	GasLimit  uint64 `json:"gasLimit"`
	GasUsed   uint64 `json:"gasUsed"`
	Input     string `json:"input"`
	Nonce     uint64 `json:"nonce"`
	TxIndex  uint64 `json:"transactionIndex"`
	Timestamp uint64 `json:"timestamp"`
	Status   uint64 `json:"status"`
}

type Account struct {
	Address   string `json:"address"`
	Balance  string `json:"balance"`
	Nonce    uint64 `json:"nonce"`
	CodeHash string `json:"codeHash"`
	StorageRoot string `json:"storageRoot"`
}

type Token struct {
	Address       string `json:"address"`
	Name         string `json:"name"`
	Symbol       string `json:"symbol"`
	Decimals     uint8  `json:"decimals"`
	TotalSupply string `json:"totalSupply"`
	Type        string `json:"type"`
}

type Transfer struct {
	Hash       string `json:"hash"`
	From      string `json:"from"`
	To        string `json:"to"`
	Value     string `json:"value"`
	Token     string `json:"tokenAddress"`
	Timestamp uint64 `json:"timestamp"`
}

type InternalTx struct {
	Hash  string `json:"hash"`
	From  string `json:"from"`
	To    string `json:"to"`
	Value string `json:"value"`
	Type  string `json:"type"`
}

type Log struct {
	Address     string   `json:"address"`
	Topics     []string `json:"topics"`
	Data       string   `json:"data"`
	BlockNumber uint64  `json:"blockNumber"`
	TxHash    string   `json:"transactionHash"`
	LogIndex  uint64   `json:"logIndex"`
}

type NFT struct {
	TokenID     string `json:"tokenId"`
	Contract   string `json:"contractAddress"`
	Name       string `json:"name"`
	Symbol    string `json:"symbol"`
	URI       string `json:"tokenURI"`
	Owner     string `json:"owner"`
}

type GasPrice struct {
	Slow     string `json:"slow"`
	Standard string `json:"standard"`
	Fast    string `json:"fast"`
	BaseFee string `json:"baseFeePerGas"`
}

type Receipt struct {
	TransactionHash  string   `json:"transactionHash"`
	BlockNumber    uint64   `json:"blockNumber"`
	Status        uint64   `json:"status"`
	CumulativeGasUsed uint64 `json:"cumulativeGasUsed"`
	GasUsed       uint64   `json:"gasUsed"`
	Logs          []*Log   `json:"logs"`
}

type QueryOpts struct {
	Page     int `json:"page"`
	Limit   int `json:"limit"`
	Offset  int `json:"offset"`
	StartBlock uint64 `json:"startBlock"`
	EndBlock uint64 `json:"endBlock"`
}

type LogQueryOpts struct {
	Address   string   `json:"address"`
	Topics    []string `json:"topics"`
	FromBlock uint64   `json:"fromBlock"`
	ToBlock  uint64   `json:"toBlock"`
}

type TxOptions struct {
	Value    string `json:"value"`
	GasLimit uint64 `json:"gasLimit"`
	GasPrice string `json:"gasPrice"`
}

type SearchResult struct {
	Blocks   []*Block      `json:"blocks"`
	Transactions []*Transaction `json:"transactions"`
	Addresses   []string   `json:"addresses"`
	Tokens     []*Token    `json:"tokens"`
}

// Helper functions

func addQueryParams(endpoint string, opts *QueryOpts) string {
	var params []string
	if opts.Page > 0 {
		params = append(params, fmt.Sprintf("page=%d", opts.Page))
	}
	if opts.Limit > 0 {
		params = append(params, fmt.Sprintf("limit=%d", opts.Limit))
	}
	if opts.Offset > 0 {
		params = append(params, fmt.Sprintf("offset=%d", opts.Offset))
	}
	if opts.StartBlock > 0 {
		params = append(params, fmt.Sprintf("startblock=%d", opts.StartBlock))
	}
	if opts.EndBlock > 0 {
		params = append(params, fmt.Sprintf("endblock=%d", opts.EndBlock))
	}
	
	if len(params) > 0 {
		return endpoint + "?" + strings.Join(params, "&")
	}
	
	return endpoint
}

// Get is a placeholder for HTTP GET
func (h *HTTPClient) Get(endpoint string) ([]byte, error) {
	return []byte("{}"), nil
}

// Post is a placeholder for HTTP POST
func (h *HTTPClient) Post(endpoint string, data interface{}) ([]byte, error) {
	return []byte("{}"), nil
}

// InitSDK initializes the SDK
func InitSDK(apiKey, baseURL string, chainID uint64) (*SDKClient, error) {
	return NewSDKClient(apiKey, baseURL, chainID), nil
}