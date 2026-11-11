package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

// DefaultChainID is the TigerSmartChain network id.
const DefaultChainID uint64 = 6666

// Client talks to the TigerScan REST API over HTTP.
type Client struct {
	baseURL string
	apiKey  string
	chainID uint64
	http    *http.Client
}

// NewClient builds a Client for the given API base URL.
func NewClient(baseURL, apiKey string, chainID uint64) *Client {
	return &Client{
		baseURL: strings.TrimRight(baseURL, "/"),
		apiKey:  apiKey,
		chainID: chainID,
		http:    &http.Client{Timeout: 30 * time.Second},
	}
}

// Block is a block as returned by GET /api/v1/blocks/{ref}.
type Block struct {
	Number       uint64            `json:"number"`
	Hash         string            `json:"hash"`
	ParentHash   string            `json:"parent_hash"`
	Miner        string            `json:"miner"`
	GasLimit     uint64            `json:"gas_limit"`
	GasUsed      uint64            `json:"gas_used"`
	Timestamp    uint64            `json:"timestamp"`
	TxCount      uint64            `json:"tx_count"`
	Size         uint64            `json:"size"`
	Transactions []json.RawMessage `json:"transactions"`
}

// Transaction is a transaction as returned by GET /api/v1/transactions/{hash}.
type Transaction struct {
	Hash        string `json:"hash"`
	BlockNumber uint64 `json:"block_number"`
	From        string `json:"from_address"`
	To          string `json:"to_address"`
	Value       string `json:"value"`
	GasPrice    string `json:"gas_price"`
	GasUsed     uint64 `json:"gas_used"`
	Nonce       uint64 `json:"nonce"`
	Status      int    `json:"status"`
}

// Account is an address summary as returned by GET /api/v1/accounts/{address}.
type Account struct {
	Address    string `json:"address"`
	Balance    string `json:"balance"`
	Nonce      uint64 `json:"nonce"`
	IsContract bool   `json:"is_contract"`
}

// Token is a token as returned by GET /api/v1/tokens/{address}.
type Token struct {
	Address     string `json:"address"`
	Name        string `json:"name"`
	Symbol      string `json:"symbol"`
	Decimals    int    `json:"decimals"`
	TotalSupply string `json:"total_supply"`
	Type        string `json:"type"`
}

// LogQueryOpts filters the log query issued by the logs command.
type LogQueryOpts struct {
	Address   string
	FromBlock uint64
	ToBlock   uint64
	Topic0    string
	Limit     int
}

func (c *Client) do(method, path string, query url.Values, body interface{}) (json.RawMessage, error) {
	u := c.baseURL + path
	if len(query) > 0 {
		u += "?" + query.Encode()
	}

	var reader io.Reader
	if body != nil {
		encoded, err := json.Marshal(body)
		if err != nil {
			return nil, err
		}
		reader = bytes.NewReader(encoded)
	}

	req, err := http.NewRequest(method, u, reader)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/json")
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	if c.apiKey != "" {
		req.Header.Set("X-API-Key", c.apiKey)
	}

	resp, err := c.http.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	raw, err := io.ReadAll(io.LimitReader(resp.Body, 64<<20))
	if err != nil {
		return nil, err
	}

	if resp.StatusCode != http.StatusOK {
		var apiErr struct {
			Error   string `json:"error"`
			Message string `json:"message"`
		}
		json.Unmarshal(raw, &apiErr)
		detail := apiErr.Error
		if detail == "" {
			detail = apiErr.Message
		}
		if detail == "" {
			detail = string(raw)
		}
		return nil, fmt.Errorf("%s %s: HTTP %d: %s", method, path, resp.StatusCode, strings.TrimSpace(detail))
	}

	return json.RawMessage(raw), nil
}

func (c *Client) get(path string, query url.Values) (json.RawMessage, error) {
	return c.do(http.MethodGet, path, query, nil)
}

// GetBlock fetches a block by number or hash.
func (c *Client) GetBlock(numberOrHash string) (*Block, error) {
	raw, err := c.get("/api/v1/blocks/"+url.PathEscape(numberOrHash), nil)
	if err != nil {
		return nil, err
	}
	var b Block
	if err := json.Unmarshal(raw, &b); err != nil {
		return nil, err
	}
	return &b, nil
}

// GetTransaction fetches a transaction by hash.
func (c *Client) GetTransaction(hash string) (*Transaction, error) {
	raw, err := c.get("/api/v1/transactions/"+url.PathEscape(hash), nil)
	if err != nil {
		return nil, err
	}
	var t Transaction
	if err := json.Unmarshal(raw, &t); err != nil {
		return nil, err
	}
	return &t, nil
}

// GetTxReceipt fetches the receipt for a transaction.
func (c *Client) GetTxReceipt(hash string) (json.RawMessage, error) {
	return c.get("/api/v1/transactions/"+url.PathEscape(hash)+"/receipt", nil)
}

// GetInternalTransactions fetches the internal (trace) transactions of a tx.
func (c *Client) GetInternalTransactions(hash string) (json.RawMessage, error) {
	return c.get("/api/v1/transactions/"+url.PathEscape(hash)+"/internal", nil)
}

// GetAccount fetches an address summary.
func (c *Client) GetAccount(address string) (*Account, error) {
	raw, err := c.get("/api/v1/accounts/"+url.PathEscape(address), nil)
	if err != nil {
		return nil, err
	}
	var a Account
	if err := json.Unmarshal(raw, &a); err != nil {
		return nil, err
	}
	return &a, nil
}

// GetToken fetches a token by contract address.
func (c *Client) GetToken(address string) (*Token, error) {
	raw, err := c.get("/api/v1/tokens/"+url.PathEscape(address), nil)
	if err != nil {
		return nil, err
	}
	var t Token
	if err := json.Unmarshal(raw, &t); err != nil {
		return nil, err
	}
	return &t, nil
}

// GetTokenTransfers lists token transfers for a token contract.
func (c *Client) GetTokenTransfers(address string, limit *int) (json.RawMessage, error) {
	q := url.Values{}
	if limit != nil {
		q.Set("limit", strconv.Itoa(*limit))
	}
	return c.get("/api/v1/tokens/"+url.PathEscape(address)+"/transfers", q)
}

// GetNFTs lists NFTs held by an address.
func (c *Client) GetNFTs(address string, limit *int) (json.RawMessage, error) {
	q := url.Values{}
	if limit != nil {
		q.Set("limit", strconv.Itoa(*limit))
	}
	return c.get("/api/v1/accounts/"+url.PathEscape(address)+"/nfts", q)
}

// GetLogs lists event logs matching the given filters.
func (c *Client) GetLogs(opts *LogQueryOpts) (json.RawMessage, error) {
	q := url.Values{}
	if opts != nil {
		if opts.Address != "" {
			q.Set("address", opts.Address)
		}
		if opts.FromBlock > 0 {
			q.Set("from_block", strconv.FormatUint(opts.FromBlock, 10))
		}
		if opts.ToBlock > 0 {
			q.Set("to_block", strconv.FormatUint(opts.ToBlock, 10))
		}
		if opts.Topic0 != "" {
			q.Set("topic0", opts.Topic0)
		}
		if opts.Limit > 0 {
			q.Set("limit", strconv.Itoa(opts.Limit))
		}
	}
	return c.get("/api/v1/logs", q)
}

// Search runs a cross-entity search.
func (c *Client) Search(query string) (json.RawMessage, error) {
	q := url.Values{}
	q.Set("q", query)
	return c.get("/api/v1/search", q)
}

// ReadContract performs a read-only eth_call. method must be a 4-byte hex
// selector and every parameter a 32-byte hex word.
func (c *Client) ReadContract(address, method string, params []string) (json.RawMessage, error) {
	return c.do(http.MethodPost, "/api/v1/contracts/"+url.PathEscape(address)+"/read", nil, map[string]interface{}{
		"method": method,
		"params": params,
	})
}

// WriteContract submits a signed raw transaction to the network.
func (c *Client) WriteContract(address, method string, params []string, rawTx string) (string, error) {
	raw, err := c.do(http.MethodPost, "/api/v1/contracts/"+url.PathEscape(address)+"/write", nil, map[string]interface{}{
		"method": method,
		"params": params,
		"raw":    rawTx,
	})
	if err != nil {
		return "", err
	}
	var out struct {
		Hash string `json:"hash"`
	}
	if err := json.Unmarshal(raw, &out); err != nil {
		return "", err
	}
	if out.Hash == "" {
		return "", fmt.Errorf("node did not return a transaction hash")
	}
	return out.Hash, nil
}

// GetGasPrice returns the current gas price snapshot.
func (c *Client) GetGasPrice() (json.RawMessage, error) {
	return c.get("/api/v1/gas", nil)
}

// GetBlockNumber returns the height of the latest indexed block.
func (c *Client) GetBlockNumber() (uint64, error) {
	raw, err := c.get("/api/v1/blocks/latest", nil)
	if err != nil {
		return 0, err
	}
	var b struct {
		Number uint64 `json:"number"`
	}
	if err := json.Unmarshal(raw, &b); err != nil {
		return 0, err
	}
	return b.Number, nil
}
