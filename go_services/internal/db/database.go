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

// Block queries
func (d *Database) GetBlock(ctx context.Context, number uint64) (map[string]interface{}, error) {
	var result map[string]interface{}
	
	err := d.pool.QueryRow(ctx, `
		SELECT number, hash, parent_hash, nonce, gas_limit, gas_used, timestamp, 
		       miner, size, base_fee_per_gas, transactions_count, uncles_count
		FROM blocks WHERE number = $1
	`, number).Scan(
		&result["number"], &result["hash"], &result["parent_hash"], &result["nonce"],
		&result["gas_limit"], &result["gas_used"], &result["timestamp"], &result["miner"],
		&result["size"], &result["base_fee_per_gas"], &result["transactions_count"], &result["uncles_count"],
	)
	
	return result, err
}

func (d *Database) GetLatestBlock(ctx context.Context) (map[string]interface{}, error) {
	var result map[string]interface{}
	
	err := d.pool.QueryRow(ctx, `
		SELECT number, hash, parent_hash, nonce, gas_limit, gas_used, timestamp, 
		       miner, size, base_fee_per_gas, transactions_count, uncles_count
		FROM blocks ORDER BY number DESC LIMIT 1
	`).Scan(
		&result["number"], &result["hash"], &result["parent_hash"], &result["nonce"],
		&result["gas_limit"], &result["gas_used"], &result["timestamp"], &result["miner"],
		&result["size"], &result["base_fee_per_gas"], &result["transactions_count"], &result["uncles_count"],
	)
	
	return result, err
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
		var b map[string]interface{}
		if err := rows.Scan(
			&b["number"], &b["hash"], &b["parent_hash"], &b["gas_used"], 
			&b["gas_limit"], &b["timestamp"], &b["miner"], &b["transactions_count"],
		); err != nil {
			return nil, 0, err
		}
		blocks = append(blocks, b)
	}
	
	return blocks, total, nil
}

// Transaction queries
func (d *Database) GetTransaction(ctx context.Context, hash string) (map[string]interface{}, error) {
	var result map[string]interface{}
	
	err := d.pool.QueryRow(ctx, `
		SELECT hash, block_number, block_hash, from_address, to_address, value, 
		       gas_price, gas, nonce, input, tx_type, status, timestamp
		FROM transactions WHERE hash = $1
	`, hash).Scan(
		&result["hash"], &result["block_number"], &result["block_hash"], 
		&result["from_address"], &result["to_address"], &result["value"],
		&result["gas_price"], &result["gas"], &result["nonce"], &result["input"],
		&result["tx_type"], &result["status"], &result["timestamp"],
	)
	
	return result, err
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
		var tx map[string]interface{}
		if err := rows.Scan(
			&tx["hash"], &tx["block_number"], &tx["from_address"], &tx["to_address"],
			&tx["value"], &tx["gas_price"], &tx["status"], &tx["timestamp"],
		); err != nil {
			return nil, 0, err
		}
		txs = append(txs, tx)
	}
	
	return txs, total, nil
}

// Token queries
func (d *Database) GetToken(ctx context.Context, address string) (map[string]interface{}, error) {
	var result map[string]interface{}
	
	err := d.pool.QueryRow(ctx, `
		SELECT address, name, symbol, decimals, total_supply, type, 
		       price, price_change_24h, market_cap, volume_24h, 
		       holders_count, transfers_count, is_verified, is_spam, logo_url
		FROM tokens WHERE address = $1
	`, address).Scan(
		&result["address"], &result["name"], &result["symbol"], &result["decimals"],
		&result["total_supply"], &result["type"], &result["price"], &result["price_change_24h"],
		&result["market_cap"], &result["volume_24h"], &result["holders_count"], 
		&result["transfers_count"], &result["is_verified"], &result["is_spam"], &result["logo_url"],
	)
	
	return result, err
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
		var h map[string]interface{}
		if err := rows.Scan(&h["address"], &h["balance"], &h["percentage"]); err != nil {
			return nil, 0, err
		}
		holders = append(holders, h)
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
		var t map[string]interface{}
		if err := rows.Scan(
			&t["transaction_hash"], &t["from_address"], &t["to_address"],
			&t["value"], &t["block_number"], &t["timestamp"],
		); err != nil {
			return nil, 0, err
		}
		transfers = append(transfers, t)
	}
	
	return transfers, total, nil
}

// NFT queries
func (d *Database) GetNFTCollection(ctx context.Context, address string) (map[string]interface{}, error) {
	var result map[string]interface{}
	
	err := d.pool.QueryRow(ctx, `
		SELECT address, name, symbol, type, total_supply, minted_count, 
		       owner_count, floor_price, average_price, volume_24h, volume_7d, 
		       volume_30d, image_url
		FROM nft_collections WHERE address = $1
	`, address).Scan(
		&result["address"], &result["name"], &result["symbol"], &result["type"],
		&result["total_supply"], &result["minted_count"], &result["owner_count"],
		&result["floor_price"], &result["average_price"], &result["volume_24h"],
		&result["volume_7d"], &result["volume_30d"], &result["image_url"],
	)
	
	return result, err
}

func (d *Database) GetNFTFloorPrice(ctx context.Context, address string) (map[string]interface{}, error) {
	var result map[string]interface{}
	
	err := d.pool.QueryRow(ctx, `
		SELECT floor_price, average_price, volume_24h
		FROM nft_collections WHERE address = $1
	`, address).Scan(
		&result["floor"], &result["average"], &result["volume_24h"],
	)
	
	return result, err
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
		var t map[string]interface{}
		if err := rows.Scan(
			&t["transaction_hash"], &t["block_number"], &t["from_address"],
			&t["to_address"], &t["value"], &t["call_type"], &t["gas"],
			&t["input"], &t["output"],
		); err != nil {
			return nil, err
		}
		traces = append(traces, t)
	}
	
	return traces, nil
}

// Network stats
func (d *Database) GetNetworkStats(ctx context.Context) (map[string]interface{}, error) {
	var stats map[string]interface{}
	
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
		&stats["total_blocks"], &stats["total_transactions"], &stats["total_addresses"],
		&stats["total_contracts"], &stats["total_tokens"], &stats["avg_gas_used"], &stats["block_height"],
	)
	
	return stats, err
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
		var p map[string]interface{}
		if err := rows.Scan(
			&p["address"], &p["token0_address"], &p["token1_address"],
			&p["token0_symbol"], &p["token1_symbol"], &p["reserve0"],
			&p["reserve1"], &p["liquidity"], &p["volume_24h"], &p["volume_7d"],
		); err != nil {
			return nil, 0, err
		}
		pairs = append(pairs, p)
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
		var p map[string]interface{}
		if err := rows.Scan(
			&p["id"], &p["title"], &p["description"], &p["status"],
			&p["vote_count"], &p["start_block"], &p["end_block"], &p["created_at"],
		); err != nil {
			return nil, 0, err
		}
		proposals = append(proposals, p)
	}
	
	return proposals, total, nil
}

// Address queries
func (d *Database) GetAddress(ctx context.Context, address string) (map[string]interface{}, error) {
	var result map[string]interface{}
	
	err := d.pool.QueryRow(ctx, `
		SELECT address, balance, is_contract, tx_count, first_seen_block, 
		       last_seen_block, total_received, total_sent
		FROM addresses WHERE address = $1
	`, address).Scan(
		&result["address"], &result["balance"], &result["is_contract"],
		&result["tx_count"], &result["first_seen_block"], &result["last_seen_block"],
		&result["total_received"], &result["total_sent"],
	)
	
	return result, err
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
		var t map[string]interface{}
		if err := rows.Scan(&t["address"], &t["name"], &t["symbol"], &t["logo_url"], &t["balance"]); err != nil {
			return nil, err
		}
		tokens = append(tokens, t)
	}
	
	return tokens, nil
}
