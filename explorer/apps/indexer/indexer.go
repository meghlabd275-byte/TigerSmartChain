// Package indexer provides blockchain indexing for TigerScan.
package indexer

import (
	"fmt"
	"log"
	"sync"
	"time"
)

// Indexer indexes blockchain data for the explorer.
type Indexer struct {
	mu          sync.RWMutex
	blocks     map[uint64]*Block
	txs        map[string]*Transaction
	accounts   map[string]*Account
	tokens     map[string]*Token
	nfts       map[string]map[string]*NFT
	validators map[string]*Validator
}

// Block represents an indexed block.
type Block struct {
	Number        uint64    `json:"number"`
	Hash          string   `json:"hash"`
	ParentHash    string   `json:"parentHash"`
	Timestamp    uint64   `json:"timestamp"`
	Miner         string   `json:"miner"`
	GasUsed      uint64   `json:"gasUsed"`
	GasLimit     uint64   `json:"gasLimit"`
	Transactions []string `json:"transactions"`
	Reward       string   `json:"reward"`
	Size         uint64   `json:"size"`
}

// Transaction represents an indexed transaction.
type Transaction struct {
	Hash              string   `json:"hash"`
	BlockNumber       uint64   `json:"blockNumber"`
	BlockHash        string   `json:"blockHash"`
	From             string   `json:"from"`
	To               string   `json:"to"`
	Value            string   `json:"value"`
	Gas              uint64   `json:"gas"`
	GasPrice         string   `json:"gasPrice"`
	Nonce            uint64   `json:"nonce"`
	TransactionIndex uint64   `json:"transactionIndex"`
	Status          bool     `json:"status"`
	Input           string   `json:"input"`
}

// Account represents an indexed account.
type Account struct {
	Address     string  `json:"address"`
	Balance    string  `json:"balance"`
	TokenCount uint64  `json:"tokenCount"`
	NFTCount   uint64  `json:"nftCount"`
	TxCount    uint64   `json:"txCount"`
	IsContract bool    `json:"isContract"`
	Code      string  `json:"code,omitempty"`
}

// Token represents a TEP20 token.
type Token struct {
	Address      string `json:"address"`
	Name         string `json:"name"`
	Symbol       string `json:"symbol"`
	Decimals     uint8  `json:"decimals"`
	TotalSupply string `json:"totalSupply"`
	Holders     uint64 `json:"holders"`
	Transfers   uint64 `json:"transfers"`
}

// NFT represents an NFT.
type NFT struct {
	TokenID  string `json:"tokenId"`
	Address string `json:"address"`
	Owner   string `json:"owner"`
	URI     string `json:"uri"`
	Name    string `json:"name"`
	Symbol  string `json:"symbol"`
}

// Validator represents a validator.
type Validator struct {
	Address     string  `json:"address"`
	Name        string  `json:"name"`
	Stake       string  `json:"stake"`
	Commission uint8   `json:"commission"`
	Uptime     float64 `json:"uptime"`
	Delegators uint64  `json:"delegators"`
	Active     bool    `json:"active"`
	Jailed     bool    `json:"jailed"`
}

// New creates a new indexer.
func New() *Indexer {
	return &Indexer{
		blocks:     make(map[uint64]*Block),
		txs:        make(map[string]*Transaction),
		accounts:  make(map[string]*Account),
		tokens:    make(map[string]*Token),
		nfts:      make(map[string]map[string]*NFT),
		validators: make(map[string]*Validator),
	}
}

// IndexBlock indexes a new block.
func (i *Indexer) IndexBlock(block *Block) {
	i.mu.Lock()
	defer i.mu.Unlock()
	i.blocks[block.Number] = block
	log.Printf("Indexed block %d", block.Number)
}

// IndexTransaction indexes a new transaction.
func (i *Indexer) IndexTransaction(tx *Transaction) {
	i.mu.Lock()
	defer i.mu.Unlock()
	i.txs[tx.Hash] = tx
}

// IndexAccount indexes a new account.
func (i *Indexer) IndexAccount(acc *Account) {
	i.mu.Lock()
	defer i.mu.Unlock()
	i.accounts[acc.Address] = acc
}

// IndexToken indexes a new token.
func (i *Indexer) IndexToken(token *Token) {
	i.mu.Lock()
	defer i.mu.Unlock()
	i.tokens[token.Address] = token
}

// IndexNFT indexes a new NFT.
func (i *Indexer) IndexNFT(nft *NFT) {
	i.mu.Lock()
	defer i.mu.Unlock()
	if _, ok := i.nfts[nft.Address]; !ok {
		i.nfts[nft.Address] = make(map[string]*NFT)
	}
	i.nfts[nft.Address][nft.TokenID] = nft
}

// IndexValidator indexes a new validator.
func (i *Indexer) IndexValidator(v *Validator) {
	i.mu.Lock()
	defer i.mu.Unlock()
	i.validators[v.Address] = v
}

// GetBlock returns a block by number.
func (i *Indexer) GetBlock(number uint64) (*Block, error) {
	i.mu.RLock()
	defer i.mu.RUnlock()
	if b, ok := i.blocks[number]; ok {
		return b, nil
	}
	return nil, fmt.Errorf("block not found: %d", number)
}

// GetTransaction returns a transaction by hash.
func (i *Indexer) GetTransaction(hash string) (*Transaction, error) {
	i.mu.RLock()
	defer i.mu.RUnlock()
	if tx, ok := i.txs[hash]; ok {
		return tx, nil
	}
	return nil, fmt.Errorf("transaction not found: %s", hash)
}

// GetAccount returns an account by address.
func (i *Indexer) GetAccount(addr string) (*Account, error) {
	i.mu.RLock()
	defer i.mu.RUnlock()
	if acc, ok := i.accounts[addr]; ok {
		return acc, nil
	}
	return &Account{Address: addr, Balance: "0"}, nil
}

// GetToken returns a token by address.
func (i *Indexer) GetToken(addr string) (*Token, error) {
	i.mu.RLock()
	defer i.mu.RUnlock()
	if t, ok := i.tokens[addr]; ok {
		return t, nil
	}
	return nil, fmt.Errorf("token not found: %s", addr)
}

// GetValidator returns a validator by address.
func (i *Indexer) GetValidator(addr string) (*Validator, error) {
	i.mu.RLock()
	defer i.mu.RUnlock()
	if v, ok := i.validators[addr]; ok {
		return v, nil
	}
	return nil, fmt.Errorf("validator not found: %s", addr)
}

// GetBlocks returns all blocks.
func (i *Indexer) GetBlocks(limit, offset int) ([]*Block, error) {
	i.mu.RLock()
	defer i.mu.RUnlock()
	blocks := make([]*Block, 0)
	for _, b := range i.blocks {
		blocks = append(blocks, b)
	}
	return blocks, nil
}

// GetTransactions returns all transactions.
func (i *Indexer) GetTransactions(limit, offset int) ([]*Transaction, error) {
	i.mu.RLock()
	defer i.mu.RUnlock()
	txs := make([]*Transaction, 0)
	for _, tx := range i.txs {
		txs = append(txs, tx)
	}
	return txs, nil
}

// GetTokens returns all tokens.
func (i *Indexer) GetTokens() ([]*Token, error) {
	i.mu.RLock()
	defer i.mu.RUnlock()
	tokens := make([]*Token, 0)
	for _, t := range i.tokens {
		tokens = append(tokens, t)
	}
	return tokens, nil
}

// GetValidators returns all validators.
func (i *Indexer) GetValidators() ([]*Validator, error) {
	i.mu.RLock()
	defer i.mu.RUnlock()
	validators := make([]*Validator, 0)
	for _, v := range i.validators {
		validators = append(validators, v)
	}
	return validators, nil
}

// SyncBlock syncs a block from the chain.
func (i *Indexer) SyncBlock(number uint64) error {
	// In production, this would fetch from RPC
	block := &Block{
		Number:    number,
		Hash:    fmt.Sprintf("0x%x", number),
		Timestamp: uint64(time.Now().Unix()),
		GasUsed:  15000000,
		GasLimit: 30000000,
	}
	i.IndexBlock(block)
	return nil
}

// Start starts the indexer sync loop.
func (i *Indexer) Start() {
	log.Println("Indexer started")
	// Sync loop would run here
}

// Stop stops the indexer.
func (i *Indexer) Stop() {
	log.Println("Indexer stopped")
}