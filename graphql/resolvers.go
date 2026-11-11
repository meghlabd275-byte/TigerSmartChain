package graphql

import (
	"context"
	"fmt"
	"strconv"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

// transactionFilter narrows a transaction query.
type transactionFilter struct {
	hash   string
	from   *string
	to     *string
	block  *string
	limit  int
	offset int
}

// blocks loads block headers ordered by height descending. When number or hash
// are supplied the result is restricted to that single block.
func (s *Server) blocks(ctx context.Context, limit, offset int, number, hash *string) ([]*Block, error) {
	d, err := depsFrom(ctx)
	if err != nil {
		return nil, err
	}

	const baseSelect = `
		SELECT b.number, b.hash, b.parent_hash, b.timestamp, b.tx_count, b.uncle_count,
		       b.gas_used, b.gas_limit, b.miner, b.size, b.state_root,
		       COALESCE(array_agg(t.hash ORDER BY t.transaction_index) FILTER (WHERE t.hash IS NOT NULL), '{}')
		FROM blocks b
		LEFT JOIN transactions t ON t.block_number = b.number`

	var (
		query string
		args  []interface{}
	)

	switch {
	case number != nil:
		height, err := parseBigInt(*number)
		if err != nil {
			return nil, fmt.Errorf("invalid block number: %w", err)
		}
		query = baseSelect + ` WHERE b.number = $1 GROUP BY b.id, b.number, b.hash, b.parent_hash, b.timestamp, b.tx_count, b.uncle_count, b.gas_used, b.gas_limit, b.miner, b.size, b.state_root`
		args = []interface{}{height}
	case hash != nil:
		query = baseSelect + ` WHERE b.hash = $1 GROUP BY b.id, b.number, b.hash, b.parent_hash, b.timestamp, b.tx_count, b.uncle_count, b.gas_used, b.gas_limit, b.miner, b.size, b.state_root`
		args = []interface{}{*hash}
	default:
		query = baseSelect + ` GROUP BY b.id, b.number, b.hash, b.parent_hash, b.timestamp, b.tx_count, b.uncle_count, b.gas_used, b.gas_limit, b.miner, b.size, b.state_root ORDER BY b.number DESC LIMIT $1 OFFSET $2`
		args = []interface{}{limit, offset}
	}

	rows, err := d.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	blocks := make([]*Block, 0, limit)
	for rows.Next() {
		var (
			number     int64
			hash       string
			parentHash string
			timestamp  int64
			txCount    int32
			uncleCount int32
			gasUsed    int64
			gasLimit   int64
			miner      string
			size       int64
			stateRoot  *string
			txHashes   []string
		)
		if err := rows.Scan(&number, &hash, &parentHash, &timestamp, &txCount, &uncleCount,
			&gasUsed, &gasLimit, &miner, &size, &stateRoot, &txHashes); err != nil {
			return nil, err
		}
		blocks = append(blocks, &Block{
			Number:       fmt.Sprintf("%d", number),
			Hash:         hash,
			ParentHash:   parentHash,
			Timestamp:    time.Unix(timestamp, 0).UTC().Format(time.RFC3339),
			TxCount:      int(txCount),
			UncleCount:   int(uncleCount),
			GasUsed:      fmt.Sprintf("%d", gasUsed),
			GasLimit:     fmt.Sprintf("%d", gasLimit),
			Miner:        miner,
			Size:         fmt.Sprintf("%d", size),
			StateRoot:    stateRoot,
			Transactions: txHashes,
		})
	}

	return blocks, rows.Err()
}

// transactions loads transactions matching the filter.
func (s *Server) transactions(ctx context.Context, filter transactionFilter) ([]*Transaction, error) {
	d, err := depsFrom(ctx)
	if err != nil {
		return nil, err
	}

	const baseSelect = `
		SELECT hash, nonce, block_number, block_hash, transaction_index, from_address, to_address,
		       value, gas_price, gas_limit, gas_used, input, status::text, block_timestamp, contract_address
		FROM transactions`

	conditions := make([]string, 0, 4)
	args := make([]interface{}, 0, 6)

	add := func(column string, value interface{}) {
		args = append(args, value)
		conditions = append(conditions, fmt.Sprintf("%s = $%d", column, len(args)))
	}

	if filter.hash != "" {
		add("hash", filter.hash)
	}
	if filter.from != nil {
		add("from_address", *filter.from)
	}
	if filter.to != nil {
		add("to_address", *filter.to)
	}
	if filter.block != nil {
		height, err := parseBigInt(*filter.block)
		if err != nil {
			return nil, fmt.Errorf("invalid block number: %w", err)
		}
		add("block_number", height)
	}

	query := baseSelect
	if len(conditions) > 0 {
		query += " WHERE " + strings.Join(conditions, " AND ")
	}

	if filter.hash != "" {
		query += " LIMIT 1"
	} else {
		args = append(args, filter.limit, filter.offset)
		query += fmt.Sprintf(" ORDER BY block_number DESC NULLS LAST, transaction_index DESC LIMIT $%d OFFSET $%d", len(args)-1, len(args))
	}

	rows, err := d.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	txs := make([]*Transaction, 0, filter.limit)
	for rows.Next() {
		tx, err := scanTransaction(rows)
		if err != nil {
			return nil, err
		}
		txs = append(txs, tx)
	}

	return txs, rows.Err()
}

func scanTransaction(row pgx.Row) (*Transaction, error) {
	var (
		hash         string
		nonce        int64
		blockNumber  *int64
		blockHash    *string
		txIndex      *int32
		from         string
		to           *string
		value        string
		gasPrice     int64
		gasLimit     int64
		gasUsed      *int64
		input        *string
		status       string
		blockTime    *time.Time
		contractAddr *string
	)

	if err := row.Scan(&hash, &nonce, &blockNumber, &blockHash, &txIndex, &from, &to,
		&value, &gasPrice, &gasLimit, &gasUsed, &input, &status, &blockTime, &contractAddr); err != nil {
		return nil, err
	}

	tx := &Transaction{
		Hash:            hash,
		Nonce:           fmt.Sprintf("%d", nonce),
		From:            from,
		To:              to,
		Value:           value,
		GasPrice:        fmt.Sprintf("%d", gasPrice),
		GasLimit:        fmt.Sprintf("%d", gasLimit),
		Input:           input,
		Status:          status,
		ContractAddress: contractAddr,
	}

	if blockNumber != nil {
		height := fmt.Sprintf("%d", *blockNumber)
		tx.BlockNumber = &height
	}
	tx.BlockHash = blockHash
	if txIndex != nil {
		index := int(*txIndex)
		tx.TransactionIndex = &index
	}
	if gasUsed != nil {
		used := fmt.Sprintf("%d", *gasUsed)
		tx.GasUsed = &used
	}
	if blockTime != nil {
		stamp := blockTime.UTC().Format(time.RFC3339)
		tx.Timestamp = &stamp
	}

	return tx, nil
}

// tokens lists tracked tokens ordered by transfer count. When address is set
// the query resolves a single token.
func (s *Server) tokens(ctx context.Context, limit, offset int, search *string, address ...string) ([]*Token, error) {
	d, err := depsFrom(ctx)
	if err != nil {
		return nil, err
	}

	const baseSelect = `
		SELECT t.address, t.name, t.symbol, t.decimals, t.total_supply, t.holders_count, t.transfers_count,
		       t.price_usd, t.market_cap, t.volume_24h, t.is_verified,
		       COALESCE((SELECT array_agg(th.address ORDER BY th.balance DESC) FROM (SELECT address, balance FROM token_holders WHERE token_address = t.address ORDER BY balance DESC LIMIT 20) th), '{}')
		FROM tokens t`

	conditions := []string{"t.is_spam = false"}
	args := make([]interface{}, 0, 4)

	if len(address) > 0 && address[0] != "" {
		args = append(args, address[0])
		conditions = append(conditions, fmt.Sprintf("t.address = $%d", len(args)))
	}
	if search != nil {
		args = append(args, "%"+*search+"%")
		conditions = append(conditions, fmt.Sprintf("(t.name ILIKE $%d OR t.symbol ILIKE $%d)", len(args), len(args)))
	}

	query := baseSelect + " WHERE " + strings.Join(conditions, " AND ") + " ORDER BY t.transfers_count DESC, t.address"
	if len(address) > 0 && address[0] != "" {
		query += " LIMIT 1"
	} else {
		args = append(args, limit, offset)
		query += fmt.Sprintf(" LIMIT $%d OFFSET $%d", len(args)-1, len(args))
	}

	rows, err := d.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	tokens := make([]*Token, 0, limit)
	for rows.Next() {
		var (
			addr           string
			name           string
			symbol         string
			decimals       int32
			totalSupply    *string
			holdersCount   int64
			transfersCount int64
			priceUsd       *string
			marketCap      *string
			volume24h      *string
			isVerified     bool
			holders        []string
		)
		if err := rows.Scan(&addr, &name, &symbol, &decimals, &totalSupply, &holdersCount, &transfersCount,
			&priceUsd, &marketCap, &volume24h, &isVerified, &holders); err != nil {
			return nil, err
		}
		tokens = append(tokens, &Token{
			Address:        addr,
			Name:           name,
			Symbol:         symbol,
			Decimals:       int(decimals),
			TotalSupply:    totalSupply,
			HoldersCount:   fmt.Sprintf("%d", holdersCount),
			TransfersCount: fmt.Sprintf("%d", transfersCount),
			PriceUsd:       priceUsd,
			MarketCap:      marketCap,
			Volume24h:      volume24h,
			IsVerified:     isVerified,
			Holders:        holders,
		})
	}

	return tokens, rows.Err()
}

// nfts lists NFT instances, optionally restricted to a collection or owner.
func (s *Server) nfts(ctx context.Context, limit, offset int, collection, owner *string) ([]*NFT, error) {
	d, err := depsFrom(ctx)
	if err != nil {
		return nil, err
	}

	const baseSelect = `
		SELECT token_address, token_id, name, owner, uri, image_url, attributes
		FROM nfts
		WHERE is_burned = false`

	conditions := []string{}
	args := make([]interface{}, 0, 4)

	if collection != nil {
		args = append(args, *collection)
		conditions = append(conditions, fmt.Sprintf("token_address = $%d", len(args)))
	}
	if owner != nil {
		args = append(args, *owner)
		conditions = append(conditions, fmt.Sprintf("owner = $%d", len(args)))
	}

	query := baseSelect
	if len(conditions) > 0 {
		query += " AND " + strings.Join(conditions, " AND ")
	}
	args = append(args, limit, offset)
	query += fmt.Sprintf(" ORDER BY last_transfer_block DESC NULLS LAST, token_id DESC LIMIT $%d OFFSET $%d", len(args)-1, len(args))

	rows, err := d.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	nfts := make([]*NFT, 0, limit)
	for rows.Next() {
		var (
			tokenAddress string
			tokenID      string
			name         *string
			owner        string
			uri          *string
			imageURL     *string
			attributes   []byte
		)
		if err := rows.Scan(&tokenAddress, &tokenID, &name, &owner, &uri, &imageURL, &attributes); err != nil {
			return nil, err
		}
		nfts = append(nfts, &NFT{
			TokenAddress: tokenAddress,
			TokenID:      tokenID,
			Name:         name,
			Owner:        owner,
			URI:          uri,
			ImageURL:     imageURL,
			Attributes:   jsonOrNil(attributes),
		})
	}

	return nfts, rows.Err()
}

// account loads one address with its token holdings.
func (s *Server) account(ctx context.Context, address string) (*Account, error) {
	d, err := depsFrom(ctx)
	if err != nil {
		return nil, err
	}

	var (
		balance    string
		nonce      int64
		codeHash   *string
		isContract bool
		isVerified bool
	)
	err = d.pool.QueryRow(ctx, `
		SELECT balance, nonce, code_hash, is_contract, is_verified
		FROM accounts WHERE address = $1`, address).Scan(&balance, &nonce, &codeHash, &isContract, &isVerified)
	if err != nil {
		return nil, err
	}

	account := &Account{
		Address:    address,
		Balance:    balance,
		Nonce:      fmt.Sprintf("%d", nonce),
		CodeHash:   codeHash,
		IsContract: isContract,
		IsVerified: isVerified,
	}

	rows, err := d.pool.Query(ctx, `
		SELECT token_address, address, balance, percent_holdings
		FROM token_holders WHERE address = $1 ORDER BY balance DESC LIMIT 50`, address)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	for rows.Next() {
		var (
			holder    TokenHolder
			percent   *string
			holderBal string
		)
		if err := rows.Scan(&holder.TokenAddress, &holder.Address, &holderBal, &percent); err != nil {
			return nil, err
		}
		holder.Balance = holderBal
		holder.Percent = percent
		account.TokenHoldings = append(account.TokenHoldings, holder)
	}

	return account, rows.Err()
}

// validators lists consensus participants ordered by stake.
func (s *Server) validators(ctx context.Context, limit int, active bool, filterActive bool) ([]*Validator, error) {
	d, err := depsFrom(ctx)
	if err != nil {
		return nil, err
	}

	query := `
		SELECT address, name, commission_rate, total_stake, delegation, uptime, blocks_count, misses_count, is_active, is_jailed
		FROM validators`
	args := []interface{}{}
	if filterActive {
		args = append(args, active)
		query += " WHERE is_active = $1"
	}
	args = append(args, limit)
	query += fmt.Sprintf(" ORDER BY total_stake DESC NULLS LAST LIMIT $%d", len(args))

	rows, err := d.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	validators := make([]*Validator, 0, limit)
	for rows.Next() {
		var (
			address    string
			name       *string
			commission *string
			totalStake *string
			delegation *string
			uptime     *string
			blocks     int64
			misses     int64
			isActive   bool
			isJailed   bool
		)
		if err := rows.Scan(&address, &name, &commission, &totalStake, &delegation, &uptime,
			&blocks, &misses, &isActive, &isJailed); err != nil {
			return nil, err
		}
		validators = append(validators, &Validator{
			Address:        address,
			Name:           name,
			CommissionRate: commission,
			TotalStake:     totalStake,
			Delegation:     delegation,
			Uptime:         uptime,
			BlocksCount:    fmt.Sprintf("%d", blocks),
			MissesCount:    fmt.Sprintf("%d", misses),
			IsActive:       isActive,
			IsJailed:       isJailed,
		})
	}

	return validators, rows.Err()
}

// search resolves a hash, address or block height to the entities it matches.
func (s *Server) search(ctx context.Context, query string) (*SearchResult, error) {
	query = strings.TrimSpace(query)
	if query == "" {
		return nil, fmt.Errorf("search query must not be empty")
	}

	result := &SearchResult{
		Blocks:       []*Block{},
		Transactions: []*Transaction{},
		Tokens:       []*Token{},
		Accounts:     []*Account{},
	}

	if height, err := parseBigInt(query); err == nil {
		if blocks, err := s.blocks(ctx, 1, 0, &query, nil); err == nil {
			result.Blocks = blocks
		}
		_ = height
	}

	if blocks, err := s.blocks(ctx, 1, 0, nil, &query); err == nil {
		result.Blocks = append(result.Blocks, blocks...)
	}

	if txs, err := s.transactions(ctx, transactionFilter{hash: query, limit: 1}); err == nil {
		result.Transactions = txs
	}

	if tokens, err := s.tokens(ctx, 1, 0, nil, query); err == nil {
		result.Tokens = tokens
	}

	if account, err := s.account(ctx, query); err == nil && account != nil {
		result.Accounts = []*Account{account}
	}

	return result, nil
}

// chainStats aggregates the indexer totals.
func (s *Server) chainStats(ctx context.Context) (*ChainStats, error) {
	d, err := depsFrom(ctx)
	if err != nil {
		return nil, err
	}

	var (
		totalBlocks       int64
		totalTransactions int64
		totalAddresses    int64
		tokensTracked     int64
		latestBlock       *int64
		averageGasPrice   *string
		txsLastHour       int64
	)

	err = d.pool.QueryRow(ctx, `
		SELECT
			(SELECT COUNT(*) FROM blocks),
			(SELECT COUNT(*) FROM transactions),
			(SELECT COUNT(*) FROM accounts),
			(SELECT COUNT(*) FROM tokens WHERE is_spam = false),
			(SELECT MAX(number) FROM blocks),
			(SELECT COALESCE(AVG(gas_price)::text, '0') FROM transactions WHERE block_timestamp > NOW() - INTERVAL '1 day'),
			(SELECT COUNT(*) FROM transactions WHERE block_timestamp > NOW() - INTERVAL '1 hour')`,
	).Scan(&totalBlocks, &totalTransactions, &totalAddresses, &tokensTracked, &latestBlock, &averageGasPrice, &txsLastHour)
	if err != nil {
		return nil, err
	}

	latest := "0"
	if latestBlock != nil {
		latest = fmt.Sprintf("%d", *latestBlock)
	}
	avg := "0"
	if averageGasPrice != nil {
		avg = *averageGasPrice
	}

	return &ChainStats{
		TotalBlocks:       fmt.Sprintf("%d", totalBlocks),
		TotalTransactions: fmt.Sprintf("%d", totalTransactions),
		TotalAddresses:    fmt.Sprintf("%d", totalAddresses),
		LatestBlock:       latest,
		AverageGasPrice:   avg,
		TPS:               float64(txsLastHour) / 3600.0,
		TokensTracked:     fmt.Sprintf("%d", tokensTracked),
	}, nil
}

// parseBigInt accepts both decimal and 0x-prefixed hex integers.
func parseBigInt(value string) (int64, error) {
	value = strings.TrimSpace(value)
	if value == "" {
		return 0, fmt.Errorf("empty value")
	}
	if strings.HasPrefix(value, "0x") || strings.HasPrefix(value, "0X") {
		var out int64
		if _, err := fmt.Sscanf(value, "%v", &out); err == nil {
			return out, nil
		}
		return strconv.ParseInt(value[2:], 16, 64)
	}
	return strconv.ParseInt(value, 10, 64)
}
