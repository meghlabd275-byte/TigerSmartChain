package db

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

type Database struct {
	pool *pgxpool.Pool
}

func NewDatabase() (*Database, error) {
	connStr := os.Getenv("DATABASE_URL")
	if connStr == "" {
		connStr = "postgres://postgres:postgres@localhost:5432/tigerscan"
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	pool, err := pgxpool.New(ctx, connStr)
	if err != nil {
		return nil, fmt.Errorf("failed to create connection pool: %w", err)
	}

	if err := pool.Ping(ctx); err != nil {
		return nil, fmt.Errorf("failed to ping database: %w", err)
	}

	return &Database{pool: pool}, nil
}

func (d *Database) Close() {
	d.pool.Close()
}

// scanBlockCols are the columns returned for a block row.
type blockRow struct {
	Number            int64
	Hash              string
	ParentHash        string
	Nonce             string
	GasLimit          int64
	GasUsed           int64
	Timestamp         int64
	Miner             string
	Size              int64
	BaseFeePerGas     *int64
	TransactionsCount int64
	UnclesCount       int64
}

func (b blockRow) toMap() map[string]interface{} {
	m := map[string]interface{}{
		"number":             b.Number,
		"hash":               b.Hash,
		"parent_hash":        b.ParentHash,
		"nonce":              b.Nonce,
		"gas_limit":          b.GasLimit,
		"gas_used":           b.GasUsed,
		"timestamp":          b.Timestamp,
		"miner":              b.Miner,
		"size":               b.Size,
		"transactions_count": b.TransactionsCount,
		"uncles_count":       b.UnclesCount,
	}
	if b.BaseFeePerGas != nil {
		m["base_fee_per_gas"] = *b.BaseFeePerGas
	}
	return m
}

// Block queries
func (d *Database) GetBlock(ctx context.Context, number uint64) (map[string]interface{}, error) {
	var b blockRow
	err := d.pool.QueryRow(ctx, `
		SELECT number, hash, parent_hash, nonce, gas_limit, gas_used, timestamp,
		       miner, size, base_fee_per_gas, transactions_count, uncles_count
		FROM blocks WHERE number = $1
	`, number).Scan(
		&b.Number, &b.Hash, &b.ParentHash, &b.Nonce,
		&b.GasLimit, &b.GasUsed, &b.Timestamp, &b.Miner,
		&b.Size, &b.BaseFeePerGas, &b.TransactionsCount, &b.UnclesCount,
	)
	if err != nil {
		return nil, err
	}
	return b.toMap(), nil
}

func (d *Database) GetLatestBlock(ctx context.Context) (map[string]interface{}, error) {
	var b blockRow
	err := d.pool.QueryRow(ctx, `
		SELECT number, hash, parent_hash, nonce, gas_limit, gas_used, timestamp,
		       miner, size, base_fee_per_gas, transactions_count, uncles_count
		FROM blocks ORDER BY number DESC LIMIT 1
	`).Scan(
		&b.Number, &b.Hash, &b.ParentHash, &b.Nonce,
		&b.GasLimit, &b.GasUsed, &b.Timestamp, &b.Miner,
		&b.Size, &b.BaseFeePerGas, &b.TransactionsCount, &b.UnclesCount,
	)
	if err != nil {
		return nil, err
	}
	return b.toMap(), nil
}

func (d *Database) GetBlocks(ctx context.Context, page, limit int) ([]map[string]interface{}, int, error) {
	offset := (page - 1) * limit

	var total int
	err := d.pool.QueryRow(ctx, "SELECT COUNT(*) FROM blocks").Scan(&total)
	if err != nil {
		return nil, 0, err
	}

	rows, err := d.pool.Query(ctx, `
		SELECT number, hash, parent_hash, gas_used, gas_limit, timestamp, miner, transactions_count
		FROM blocks ORDER BY number DESC LIMIT $1 OFFSET $2
	`, limit, offset)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()

	var blocks []map[string]interface{}
	for rows.Next() {
		var number, gasUsed, gasLimit, timestamp, txCount int64
		var hash, parentHash, miner string
		if err := rows.Scan(&number, &hash, &parentHash, &gasUsed, &gasLimit, &timestamp, &miner, &txCount); err != nil {
			return nil, 0, err
		}
		blocks = append(blocks, map[string]interface{}{
			"number":             number,
			"hash":               hash,
			"parent_hash":        parentHash,
			"gas_used":           gasUsed,
			"gas_limit":          gasLimit,
			"timestamp":          timestamp,
			"miner":              miner,
			"transactions_count": txCount,
		})
	}

	return blocks, total, nil
}

// Transaction queries
func (d *Database) GetTransaction(ctx context.Context, hash string) (map[string]interface{}, error) {
	var (
		txHash, blockHash, fromAddr, toAddr, value, gasPrice, nonce, input, txType, status string
		blockNumber, gas, timestamp                                                       int64
	)
	err := d.pool.QueryRow(ctx, `
		SELECT hash, block_number, block_hash, from_address, to_address, value,
		       gas_price, gas, nonce, input, tx_type, status, timestamp
		FROM transactions WHERE hash = $1
	`, hash).Scan(
		&txHash, &blockNumber, &blockHash, &fromAddr, &toAddr, &value,
		&gasPrice, &gas, &nonce, &input, &txType, &status, &timestamp,
	)
	if err != nil {
		return nil, err
	}
	return map[string]interface{}{
		"hash":          txHash,
		"block_number":  blockNumber,
		"block_hash":    blockHash,
		"from_address":  fromAddr,
		"to_address":    toAddr,
		"value":         value,
		"gas_price":     gasPrice,
		"gas":           gas,
		"nonce":         nonce,
		"input":         input,
		"tx_type":       txType,
		"status":        status,
		"timestamp":     timestamp,
	}, nil
}

func (d *Database) GetTransactions(ctx context.Context, address string, page, limit int) ([]map[string]interface{}, int, error) {
	offset := (page - 1) * limit

	var total int
	err := d.pool.QueryRow(ctx, `
		SELECT COUNT(*) FROM transactions
		WHERE from_address = $1 OR to_address = $1
	`, address).Scan(&total)
	if err != nil {
		return nil, 0, err
	}

	rows, err := d.pool.Query(ctx, `
		SELECT hash, block_number, from_address, to_address, value, gas_price, status, timestamp
		FROM transactions
		WHERE from_address = $1 OR to_address = $1
		ORDER BY block_number DESC LIMIT $2 OFFSET $3
	`, address, limit, offset)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()

	var txs []map[string]interface{}
	for rows.Next() {
		var hash, fromAddr, toAddr, value, gasPrice, status string
		var blockNumber, timestamp int64
		if err := rows.Scan(&hash, &blockNumber, &fromAddr, &toAddr, &value, &gasPrice, &status, &timestamp); err != nil {
			return nil, 0, err
		}
		txs = append(txs, map[string]interface{}{
			"hash":          hash,
			"block_number":  blockNumber,
			"from_address":  fromAddr,
			"to_address":    toAddr,
			"value":         value,
			"gas_price":     gasPrice,
			"status":        status,
			"timestamp":     timestamp,
		})
	}

	return txs, total, nil
}

// Token queries
func (d *Database) GetToken(ctx context.Context, address string) (map[string]interface{}, error) {
	var (
		addr, name, symbol, tokenType, logoURL           string
		decimals                                         int
		totalSupply                                      string
		price, priceChange24h, marketCap, volume24h      float64
		holdersCount, transfersCount                     int64
		isVerified, isSpam                               bool
	)
	err := d.pool.QueryRow(ctx, `
		SELECT address, name, symbol, decimals, total_supply, type,
		       price, price_change_24h, market_cap, volume_24h,
		       holders_count, transfers_count, is_verified, is_spam, logo_url
		FROM tokens WHERE address = $1
	`, address).Scan(
		&addr, &name, &symbol, &decimals, &totalSupply, &tokenType,
		&price, &priceChange24h, &marketCap, &volume24h,
		&holdersCount, &transfersCount, &isVerified, &isSpam, &logoURL,
	)
	if err != nil {
		return nil, err
	}
	return map[string]interface{}{
		"address":           addr,
		"name":              name,
		"symbol":            symbol,
		"decimals":          decimals,
		"total_supply":      totalSupply,
		"type":              tokenType,
		"price":             price,
		"price_change_24h":  priceChange24h,
		"market_cap":        marketCap,
		"volume_24h":        volume24h,
		"holders_count":     holdersCount,
		"transfers_count":   transfersCount,
		"is_verified":       isVerified,
		"is_spam":           isSpam,
		"logo_url":          logoURL,
	}, nil
}

func (d *Database) GetTokenHolders(ctx context.Context, address string, page, limit int) ([]map[string]interface{}, int, error) {
	offset := (page - 1) * limit

	var total int
	err := d.pool.QueryRow(ctx, `
		SELECT COUNT(*) FROM token_holders WHERE token_address = $1
	`, address).Scan(&total)
	if err != nil {
		return nil, 0, err
	}

	rows, err := d.pool.Query(ctx, `
		SELECT address, balance, percentage
		FROM token_holders
		WHERE token_address = $1
		ORDER BY balance DESC LIMIT $2 OFFSET $3
	`, address, limit, offset)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()

	var holders []map[string]interface{}
	for rows.Next() {
		var addr, balance string
		var percentage float64
		if err := rows.Scan(&addr, &balance, &percentage); err != nil {
			return nil, 0, err
		}
		holders = append(holders, map[string]interface{}{
			"address":    addr,
			"balance":    balance,
			"percentage": percentage,
		})
	}

	return holders, total, nil
}

func (d *Database) GetTokenTransfers(ctx context.Context, address string, page, limit int) ([]map[string]interface{}, int, error) {
	offset := (page - 1) * limit

	var total int
	err := d.pool.QueryRow(ctx, `
		SELECT COUNT(*) FROM token_transfers WHERE token_address = $1
	`, address).Scan(&total)
	if err != nil {
		return nil, 0, err
	}

	rows, err := d.pool.Query(ctx, `
		SELECT transaction_hash, from_address, to_address, value, block_number, timestamp
		FROM token_transfers
		WHERE token_address = $1
		ORDER BY block_number DESC LIMIT $2 OFFSET $3
	`, address, limit, offset)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()

	var transfers []map[string]interface{}
	for rows.Next() {
		var txHash, fromAddr, toAddr, value string
		var blockNumber, timestamp int64
		if err := rows.Scan(&txHash, &fromAddr, &toAddr, &value, &blockNumber, &timestamp); err != nil {
			return nil, 0, err
		}
		transfers = append(transfers, map[string]interface{}{
			"transaction_hash": txHash,
			"from_address":     fromAddr,
			"to_address":       toAddr,
			"value":            value,
			"block_number":     blockNumber,
			"timestamp":        timestamp,
		})
	}

	return transfers, total, nil
}

// NFT queries
func (d *Database) GetNFTCollection(ctx context.Context, address string) (map[string]interface{}, error) {
	var (
		addr, name, symbol, collectionType, imageURL            string
		totalSupply, mintedCount, ownerCount                    int64
		floorPrice, averagePrice, volume24h, volume7d, volume30d float64
	)
	err := d.pool.QueryRow(ctx, `
		SELECT address, name, symbol, type, total_supply, minted_count,
		       owner_count, floor_price, average_price, volume_24h, volume_7d,
		       volume_30d, image_url
		FROM nft_collections WHERE address = $1
	`, address).Scan(
		&addr, &name, &symbol, &collectionType, &totalSupply, &mintedCount,
		&ownerCount, &floorPrice, &averagePrice, &volume24h, &volume7d,
		&volume30d, &imageURL,
	)
	if err != nil {
		return nil, err
	}
	return map[string]interface{}{
		"address":       addr,
		"name":          name,
		"symbol":        symbol,
		"type":          collectionType,
		"total_supply":  totalSupply,
		"minted_count":  mintedCount,
		"owner_count":   ownerCount,
		"floor_price":   floorPrice,
		"average_price": averagePrice,
		"volume_24h":    volume24h,
		"volume_7d":     volume7d,
		"volume_30d":    volume30d,
		"image_url":     imageURL,
	}, nil
}

func (d *Database) GetNFTFloorPrice(ctx context.Context, address string) (map[string]interface{}, error) {
	var floor, average, volume24h float64
	err := d.pool.QueryRow(ctx, `
		SELECT floor_price, average_price, volume_24h
		FROM nft_collections WHERE address = $1
	`, address).Scan(&floor, &average, &volume24h)
	if err != nil {
		return nil, err
	}
	return map[string]interface{}{
		"floor":      floor,
		"average":    average,
		"volume_24h": volume24h,
	}, nil
}

// Internal transaction queries
func (d *Database) GetInternalTransactions(ctx context.Context, txHash string) ([]map[string]interface{}, error) {
	rows, err := d.pool.Query(ctx, `
		SELECT transaction_hash, block_number, from_address, to_address,
		       value, call_type, gas, input, output
		FROM traces WHERE transaction_hash = $1
	`, txHash)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var traces []map[string]interface{}
	for rows.Next() {
		var txHash, fromAddr, toAddr, value, callType, input, output string
		var blockNumber, gas int64
		if err := rows.Scan(&txHash, &blockNumber, &fromAddr, &toAddr, &value, &callType, &gas, &input, &output); err != nil {
			return nil, err
		}
		traces = append(traces, map[string]interface{}{
			"transaction_hash": txHash,
			"block_number":     blockNumber,
			"from_address":     fromAddr,
			"to_address":       toAddr,
			"value":            value,
			"call_type":        callType,
			"gas":              gas,
			"input":            input,
			"output":           output,
		})
	}

	return traces, nil
}

// Network stats
func (d *Database) GetNetworkStats(ctx context.Context) (map[string]interface{}, error) {
	var (
		totalBlocks, totalTxs, totalAddresses, totalContracts, totalTokens, blockHeight int64
		avgGasUsed                                                                       *float64
	)
	err := d.pool.QueryRow(ctx, `
		SELECT
			(SELECT COUNT(*) FROM blocks) as total_blocks,
			(SELECT COUNT(*) FROM transactions) as total_transactions,
			(SELECT COUNT(DISTINCT from_address) FROM transactions) as total_addresses,
			(SELECT COUNT(*) FROM contracts) as total_contracts,
			(SELECT COUNT(*) FROM tokens) as total_tokens,
			(SELECT AVG(gas_used) FROM blocks WHERE timestamp > NOW() - INTERVAL '24 hours') as avg_gas_used,
			(SELECT MAX(number) FROM blocks) - (SELECT MIN(number) FROM blocks) as block_height
	`).Scan(
		&totalBlocks, &totalTxs, &totalAddresses,
		&totalContracts, &totalTokens, &avgGasUsed, &blockHeight,
	)
	if err != nil {
		return nil, err
	}
	stats := map[string]interface{}{
		"total_blocks":        totalBlocks,
		"total_transactions":  totalTxs,
		"total_addresses":     totalAddresses,
		"total_contracts":     totalContracts,
		"total_tokens":        totalTokens,
		"block_height":        blockHeight,
	}
	if avgGasUsed != nil {
		stats["avg_gas_used"] = *avgGasUsed
	}
	return stats, nil
}

// DEX queries
func (d *Database) GetDexPairs(ctx context.Context, page, limit int) ([]map[string]interface{}, int, error) {
	offset := (page - 1) * limit

	var total int
	err := d.pool.QueryRow(ctx, "SELECT COUNT(*) FROM dex_pairs").Scan(&total)
	if err != nil {
		return nil, 0, err
	}

	rows, err := d.pool.Query(ctx, `
		SELECT address, token0_address, token1_address, token0_symbol, token1_symbol,
		       reserve0, reserve1, liquidity, volume_24h, volume_7d
		FROM dex_pairs ORDER BY volume_24h DESC LIMIT $1 OFFSET $2
	`, limit, offset)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()

	var pairs []map[string]interface{}
	for rows.Next() {
		var addr, t0a, t1a, t0s, t1s, reserve0, reserve1, liquidity, vol24, vol7 string
		if err := rows.Scan(&addr, &t0a, &t1a, &t0s, &t1s, &reserve0, &reserve1, &liquidity, &vol24, &vol7); err != nil {
			return nil, 0, err
		}
		pairs = append(pairs, map[string]interface{}{
			"address":          addr,
			"token0_address":   t0a,
			"token1_address":   t1a,
			"token0_symbol":    t0s,
			"token1_symbol":    t1s,
			"reserve0":         reserve0,
			"reserve1":         reserve1,
			"liquidity":        liquidity,
			"volume_24h":       vol24,
			"volume_7d":        vol7,
		})
	}

	return pairs, total, nil
}

// Governance queries
func (d *Database) GetGovernanceProposals(ctx context.Context, page, limit int) ([]map[string]interface{}, int, error) {
	offset := (page - 1) * limit

	var total int
	err := d.pool.QueryRow(ctx, "SELECT COUNT(*) FROM governance_proposals").Scan(&total)
	if err != nil {
		return nil, 0, err
	}

	rows, err := d.pool.Query(ctx, `
		SELECT id, title, description, status, vote_count, start_block, end_block, created_at
		FROM governance_proposals ORDER BY created_at DESC LIMIT $1 OFFSET $2
	`, limit, offset)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()

	var proposals []map[string]interface{}
	for rows.Next() {
		var id, voteCount, startBlock, endBlock int64
		var title, description, status string
		var createdAt time.Time
		if err := rows.Scan(&id, &title, &description, &status, &voteCount, &startBlock, &endBlock, &createdAt); err != nil {
			return nil, 0, err
		}
		proposals = append(proposals, map[string]interface{}{
			"id":          id,
			"title":       title,
			"description": description,
			"status":      status,
			"vote_count":  voteCount,
			"start_block": startBlock,
			"end_block":   endBlock,
			"created_at":  createdAt,
		})
	}

	return proposals, total, nil
}

// Address queries
func (d *Database) GetAddress(ctx context.Context, address string) (map[string]interface{}, error) {
	var (
		addr, balance                                string
		isContract                                   bool
		txCount, firstSeen, lastSeen                 int64
		totalReceived, totalSent                     string
	)
	err := d.pool.QueryRow(ctx, `
		SELECT address, balance, is_contract, tx_count, first_seen_block,
		       last_seen_block, total_received, total_sent
		FROM addresses WHERE address = $1
	`, address).Scan(
		&addr, &balance, &isContract, &txCount, &firstSeen, &lastSeen,
		&totalReceived, &totalSent,
	)
	if err != nil {
		return nil, err
	}
	return map[string]interface{}{
		"address":          addr,
		"balance":          balance,
		"is_contract":      isContract,
		"tx_count":         txCount,
		"first_seen_block": firstSeen,
		"last_seen_block":  lastSeen,
		"total_received":   totalReceived,
		"total_sent":       totalSent,
	}, nil
}

func (d *Database) GetAddressTokens(ctx context.Context, address string) ([]map[string]interface{}, error) {
	rows, err := d.pool.Query(ctx, `
		SELECT t.address, t.name, t.symbol, t.logo_url, tb.balance
		FROM token_balances tb
		JOIN tokens t ON tb.token_address = t.address
		WHERE tb.address = $1 AND tb.balance > 0
		ORDER BY tb.balance DESC
	`, address)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var tokens []map[string]interface{}
	for rows.Next() {
		var addr, name, symbol, logoURL, balance string
		if err := rows.Scan(&addr, &name, &symbol, &logoURL, &balance); err != nil {
			return nil, err
		}
		tokens = append(tokens, map[string]interface{}{
			"address":  addr,
			"name":     name,
			"symbol":   symbol,
			"logo_url": logoURL,
			"balance":  balance,
		})
	}

	return tokens, nil
}
