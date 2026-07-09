package rpc

import (
	"context"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"time"

	"github.com/ethereum/go-ethereum"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/ethereum/go-ethereum/core/types"
)

// BSC RPC Client connects to Binance Smart Chain
type BSCClient struct {
	client  *ethclient.Client
	httpURL string
	wsURL   string
}

// NewBSCClient creates a new BSC RPC client
func NewBSCClient(httpURL, wsURL string) (*BSCClient, error) {
	client, err := ethclient.Dial(httpURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to BSC: %w", err)
	}
	return &BSCClient{client: client, httpURL: httpURL, wsURL: wsURL}, nil
}

func (b *BSCClient) GetBlockByNumber(ctx context.Context, num uint64) (*types.Block, error) {
	return b.client.BlockByNumber(ctx, big.NewInt(int64(num)))
}

func (b *BSCClient) GetLatestBlock(ctx context.Context) (*types.Block, error) {
	return b.client.BlockByNumber(ctx, nil)
}

func (b *BSCClient) GetTransactionByHash(ctx context.Context, hash common.Hash) (*types.Transaction, bool, error) {
	return b.client.TransactionByHash(ctx, hash)
}

func (b *BSCClient) GetTransactionReceipt(ctx context.Context, hash common.Hash) (*types.Receipt, error) {
	return b.client.TransactionReceipt(ctx, hash)
}

func (b *BSCClient) GetBalance(ctx context.Context, address common.Address) (*big.Int, error) {
	return b.client.BalanceAt(ctx, address, nil)
}

func (b *BSCClient) GetCode(ctx context.Context, address common.Address) ([]byte, error) {
	return b.client.CodeAt(ctx, address, nil)
}

func (b *BSCClient) GetStorageAt(ctx context.Context, address common.Address, slot common.Hash) ([]byte, error) {
	return b.client.StorageAt(ctx, address, slot, nil)
}

func (b *BSCClient) GetBlockNumber(ctx context.Context) (uint64, error) {
	header, err := b.client.HeaderByNumber(ctx, nil)
	if err != nil { return 0, err }
	return header.Number.Uint64(), nil
}

func (b *BSCClient) GetChainID(ctx context.Context) (*big.Int, error) {
	return b.client.ChainID(ctx)
}

func (b *BSCClient) FilterLogs(ctx context.Context, q ethereum.FilterQuery) ([]types.Log, error) {
	return b.client.FilterLogs(ctx, q)
}

func (b *BSCClient) GetNonce(ctx context.Context, address common.Address) (uint64, error) {
	return b.client.NonceAt(ctx, address, nil)
}

func (b *BSCClient) Close() { b.client.Close() }

// HTTPClient performs raw JSON-RPC calls
type HTTPClient struct {
	url string
	client *http.Client
}

func NewHTTPClient(url string) *HTTPClient {
	return &HTTPClient{url: url, client: &http.Client{Timeout: 30 * time.Second}}
}

func (c *HTTPClient) Call(method string, params ...interface{}) (json.RawMessage, error) {
	req := map[string]interface{}{"jsonrpc": "2.0", "method": method, "params": params, "id": 1}
	body, _ := json.Marshal(req)
	resp, err := c.client.Post(c.url, "application/json", body)
	if err != nil { return nil, err }
	defer resp.Body.Close()
	var result map[string]interface{}
	json.NewDecoder(resp.Body).Decode(&result)
	if errMsg, ok := result["error"]; ok { return nil, fmt.Errorf("%v", errMsg) }
	return json.RawMessage(result["result"].(string)), nil
}

func (c *HTTPClient) BlockNumber() (uint64, error) {
	result, _ := c.Call("eth_blockNumber")
	var num string
	json.Unmarshal(result, &num)
	var blockNum uint64
	fmt.Sscanf(num, "0x%x", &blockNum)
	return blockNum, nil
}

func (c *HTTPClient) GetBlockByNumber(blockNum uint64, fullTx bool) (map[string]interface{}, error) {
	result, _ := c.Call("eth_getBlockByNumber", fmt.Sprintf("0x%x", blockNum), fullTx)
	var block map[string]interface{}
	json.Unmarshal(result, &block)
	return block, nil
}

func (c *HTTPClient) GetTransactionByHash(hash string) (map[string]interface{}, error) {
	result, _ := c.Call("eth_getTransactionByHash", hash)
	var tx map[string]interface{}
	json.Unmarshal(result, &tx)
	return tx, nil
}

func (c *HTTPClient) GetTransactionReceipt(hash string) (map[string]interface{}, error) {
	result, _ := c.Call("eth_getTransactionReceipt", hash)
	var receipt map[string]interface{}
	json.Unmarshal(result, &receipt)
	return receipt, nil
}

func (c *HTTPClient) GetBalance(address, block string) (string, error) {
	result, _ := c.Call("eth_getBalance", address, block)
	var balance string
	json.Unmarshal(result, &balance)
	return balance, nil
}

func (c *HTTPClient) GetCode(address string) (string, error) {
	result, _ := c.Call("eth_getCode", address, "latest")
	var code string
	json.Unmarshal(result, &code)
	return code, nil
}

func (c *HTTPClient) GetStorageAt(address, slot string) (string, error) {
	result, _ := c.Call("eth_getStorageAt", address, slot, "latest")
	var value string
	json.Unmarshal(result, &value)
	return value, nil
}

func (c *HTTPClient) Call(from, to, data string) (string, error) {
	result, _ := c.Call("eth_call", map[string]string{"from": from, "to": to, "data": data}, "latest")
	var value string
	json.Unmarshal(result, &value)
	return value, nil
}

func (c *HTTPClient) GetLogs(filter map[string]interface{}) ([]map[string]interface{}, error) {
	result, _ := c.Call("eth_getLogs", filter)
	var logs []map[string]interface{}
	json.Unmarshal(result, &logs)
	return logs, nil
}

func (c *HTTPClient) GetChainID() (uint64, error) {
	result, _ := c.Call("eth_chainId")
	var chainID string
	json.Unmarshal(result, &chainID)
	var id uint64
	fmt.Sscanf(chainID, "0x%x", &id)
	return id, nil
}

func (c *HTTPClient) GasPrice() (string, error) {
	result, _ := c.Call("eth_gasPrice")
	var price string
	json.Unmarshal(result, &price)
	return price, nil
}
