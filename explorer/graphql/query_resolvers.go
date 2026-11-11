package graphql

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"strconv"
	"strings"

	"github.com/graphql-go/graphql"
)

// ---------------------------------------------------------------------------
// whitelisted ORDER BY fragments
// ---------------------------------------------------------------------------

// orderFragments maps a caller-visible sort key to a constant SQL fragment.
// Callers may only ever select one of these values, so no user input reaches
// the ORDER BY clause.
var orderFragments = map[string]string{
	"number":    "number DESC",
	"timestamp": "timestamp DESC",
	"gasUsed":   "gas_used DESC",
	"gasLimit":  "gas_limit DESC",
	"txCount":   "tx_count DESC",
	"blockTime": "block_number DESC",
	"value":     "value DESC",
	"gasPrice":  "gas_price DESC",
	"balance":   "balance DESC",
	"stake":     "total_stake DESC NULLS LAST",
	"uptime":    "uptime DESC NULLS LAST",
	"blocks":    "blocks_count DESC NULLS LAST",
	"delegated": "delegation DESC NULLS LAST",
	"reward":    "amount DESC",
}

func orderFor(key, fallback string) string {
	if frag, ok := orderFragments[key]; ok {
		return frag
	}
	return fallback
}

// ---------------------------------------------------------------------------
// query resolvers
// ---------------------------------------------------------------------------

func (s *Server) resolveBlocks(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	where := []string{}
	args := []interface{}{}

	if v, ok := intArg(p, "fromBlock"); ok {
		args = append(args, v)
		where = append(where, fmt.Sprintf("number >= $%d", len(args)))
	}
	if v, ok := intArg(p, "toBlock"); ok {
		args = append(args, v)
		where = append(where, fmt.Sprintf("number <= $%d", len(args)))
	}
	if miner := strArg(p, "miner"); miner != "" {
		args = append(args, strings.ToLower(miner))
		where = append(where, fmt.Sprintf("LOWER(miner) = $%d", len(args)))
	}

	q := `SELECT ` + blockColumns + ` FROM blocks`
	if len(where) > 0 {
		q += " WHERE " + strings.Join(where, " AND ")
	}
	q += " ORDER BY number DESC LIMIT $" + fmt.Sprint(len(args)+1) + " OFFSET $" + fmt.Sprint(len(args)+2)
	args = append(args, limit, offset)

	rows, err := s.config.DB.QueryContext(p.Context, q, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*blockRow, 0, limit)
	for rows.Next() {
		r, err := scanBlock(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, r)
	}
	return out, rows.Err()
}

func (s *Server) resolveBlock(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	ident := strings.TrimSpace(strArg(p, "numberOrHash"))
	if ident == "" {
		return nil, fmt.Errorf("numberOrHash is required")
	}

	var (
		q    string
		args []interface{}
	)
	if isHex(ident, 64) {
		q = `SELECT ` + blockColumns + ` FROM blocks WHERE LOWER(hash) = LOWER($1)`
		args = []interface{}{ident}
	} else {
		n, err := strconv.ParseInt(ident, 10, 64)
		if err != nil {
			return nil, fmt.Errorf("numberOrHash must be a block number or a 32-byte hash")
		}
		q = `SELECT ` + blockColumns + ` FROM blocks WHERE number = $1`
		args = []interface{}{n}
	}

	rows, err := s.config.DB.QueryContext(p.Context, q, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}
	return scanBlock(rows)
}

func (s *Server) resolveLatestBlock(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT `+blockColumns+` FROM blocks ORDER BY number DESC LIMIT 1`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}
	return scanBlock(rows)
}

func (s *Server) resolveTransactions(p graphql.ResolveParams) (interface{}, error) {
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	where := []string{}
	args := []interface{}{}

	if from := strArg(p, "from"); from != "" {
		args = append(args, strings.ToLower(from))
		where = append(where, fmt.Sprintf("LOWER(from_address) = $%d", len(args)))
	}
	if to := strArg(p, "to"); to != "" {
		args = append(args, strings.ToLower(to))
		where = append(where, fmt.Sprintf("LOWER(to_address) = $%d", len(args)))
	}
	if addr := strArg(p, "address"); addr != "" {
		args = append(args, strings.ToLower(addr))
		n := len(args)
		where = append(where,
			fmt.Sprintf("(LOWER(from_address) = $%d OR LOWER(to_address) = $%d)", n, n))
	}
	if status := strArg(p, "status"); status != "" {
		args = append(args, strings.ToLower(status))
		where = append(where, fmt.Sprintf("status = $%d", len(args)))
	}

	clause := ""
	if len(where) > 0 {
		clause = strings.Join(where, " AND ")
	}
	return s.queryTxs(p.Context, clause, args, "block_number DESC NULLS LAST, transaction_index DESC", limit, offset)
}

func (s *Server) resolveTransaction(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	hash := strings.TrimSpace(strArg(p, "hash"))
	if !isHex(hash, 64) {
		return nil, fmt.Errorf("hash must be a 32-byte hex string")
	}
	rows, err := s.config.DB.QueryContext(p.Context,
		txSelect+` WHERE LOWER(hash) = LOWER($1)`, hash)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}
	return scanTx(rows)
}

func (s *Server) resolveTransactionReceipt(p graphql.ResolveParams) (interface{}, error) {
	hash := strings.TrimSpace(strArg(p, "hash"))
	if !isHex(hash, 64) {
		return nil, fmt.Errorf("hash must be a 32-byte hex string")
	}
	return resolveReceipt(p.Context, hash)
}

func (s *Server) resolveInternalTransactions(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	hash := strings.TrimSpace(strArg(p, "txHash"))
	if !isHex(hash, 64) {
		return nil, fmt.Errorf("txHash must be a 32-byte hex string")
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT transaction_hash, block_number, trace_index, subtrace_index, call_type,
		        from_address, to_address, value::text, gas, gas_used, input, output,
		        error, depth, creates, success
		 FROM internal_transactions WHERE LOWER(transaction_hash) = LOWER($1)
		 ORDER BY trace_index, subtrace_index LIMIT $2 OFFSET $3`, hash, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*internalTxRow, 0, limit)
	for rows.Next() {
		var (
			r                                              internalTxRow
			to, value, gas, gasUsed, input, output, errMsg sql.NullString
			creates                                        sql.NullString
		)
		if err := rows.Scan(&r.TransactionHash, &r.BlockNumber, &r.TraceIndex,
			&r.SubtraceIndex, &r.CallType, &r.From, &to, &value, &gas, &gasUsed,
			&input, &output, &errMsg, &r.Depth, &creates, &r.Success); err != nil {
			return nil, err
		}
		r.To = sp(to)
		r.Value = sp(value)
		r.Gas = sp(gas)
		r.GasUsed = sp(gasUsed)
		r.Input = sp(input)
		r.Output = sp(output)
		r.Error = sp(errMsg)
		r.Creates = sp(creates)
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolveTokens(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	where := []string{}
	args := []interface{}{}
	if search := strings.TrimSpace(strArg(p, "search")); search != "" {
		args = append(args, "%"+strings.ToLower(search)+"%")
		n := len(args)
		where = append(where, fmt.Sprintf("(LOWER(t.name) LIKE $%d OR LOWER(t.symbol) LIKE $%d OR LOWER(t.address) LIKE $%d)", n, n, n))
	}
	if verified, ok := boolArg(p, "verified"); ok {
		args = append(args, verified)
		where = append(where, fmt.Sprintf("t.is_verified = $%d", len(args)))
	}

	q := tokenSelect
	if len(where) > 0 {
		q += " WHERE " + strings.Join(where, " AND ")
	}
	q += " ORDER BY t.holders_count DESC NULLS LAST LIMIT $" +
		fmt.Sprint(len(args)+1) + " OFFSET $" + fmt.Sprint(len(args)+2)
	args = append(args, limit, offset)

	rows, err := s.config.DB.QueryContext(p.Context, q, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*tokenRow, 0, limit)
	for rows.Next() {
		r, err := scanTokenRow(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, r)
	}
	return out, rows.Err()
}

func (s *Server) resolveToken(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	rows, err := s.config.DB.QueryContext(p.Context,
		tokenSelect+` WHERE LOWER(t.address) = LOWER($1)`, address)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}
	return scanTokenRow(rows)
}

func (s *Server) resolveTokenHolders(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT address, balance::text, balance_usd::text, percent_holdings::text,
		        is_contract, updated_block
		 FROM token_holders WHERE LOWER(token_address) = LOWER($1)
		 ORDER BY balance DESC LIMIT $2 OFFSET $3`, address, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*tokenHolderRow, 0, limit)
	for rows.Next() {
		var (
			r                           tokenHolderRow
			balanceUSD, percentHoldings sql.NullString
			updatedBlock                sql.NullInt64
		)
		if err := rows.Scan(&r.Address, &r.Balance, &balanceUSD, &percentHoldings,
			&r.IsContract, &updatedBlock); err != nil {
			return nil, err
		}
		r.BalanceUSD = sp(balanceUSD)
		r.PercentHoldings = sp(percentHoldings)
		r.UpdatedBlock = ip(updatedBlock)
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolveTokenTransfers(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	args := []interface{}{address}
	where := "LOWER(token_address) = LOWER($1)"
	if from, ok := intArg(p, "fromBlock"); ok {
		args = append(args, from)
		where += fmt.Sprintf(" AND block_number >= $%d", len(args))
	}

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT tt.token_address, tt.from_address, tt.to_address, tt.value::text,
		        tt.transaction_hash, tt.block_number, tt.log_index, b.timestamp
		 FROM token_transfers tt
		 LEFT JOIN blocks b ON b.number = tt.block_number
		 WHERE `+where+`
		 ORDER BY tt.block_number DESC, tt.log_index DESC LIMIT $`+
			fmt.Sprint(len(args)+1)+` OFFSET $`+fmt.Sprint(len(args)+2),
		append(args, limit, offset)...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*tokenTransferRow, 0, limit)
	for rows.Next() {
		var (
			r         tokenTransferRow
			timestamp sql.NullInt64
		)
		if err := rows.Scan(&r.TokenAddress, &r.From, &r.To, &r.Value,
			&r.TransactionHash, &r.BlockNumber, &r.LogIndex, &timestamp); err != nil {
			return nil, err
		}
		r.Timestamp = ip(timestamp)
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolveTokenApprovals(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	args := []interface{}{address}
	where := "LOWER(token_address) = LOWER($1)"
	if owner := strArg(p, "owner"); owner != "" {
		if !isHex(owner, 40) {
			return nil, fmt.Errorf("owner must be a 20-byte hex string")
		}
		args = append(args, owner)
		where += fmt.Sprintf(" AND LOWER(owner) = LOWER($%d)", len(args))
	}
	if spender := strArg(p, "spender"); spender != "" {
		if !isHex(spender, 40) {
			return nil, fmt.Errorf("spender must be a 20-byte hex string")
		}
		args = append(args, spender)
		where += fmt.Sprintf(" AND LOWER(spender) = LOWER($%d)", len(args))
	}

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT token_address, owner, spender, value::text, transaction_hash,
		        block_number, log_index
		 FROM token_approvals WHERE `+where+`
		 ORDER BY block_number DESC, log_index DESC LIMIT $`+
			fmt.Sprint(len(args)+1)+` OFFSET $`+fmt.Sprint(len(args)+2),
		append(args, limit, offset)...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*tokenApprovalRow, 0, limit)
	for rows.Next() {
		var r tokenApprovalRow
		if err := rows.Scan(&r.TokenAddress, &r.Owner, &r.Spender, &r.Value,
			&r.TransactionHash, &r.BlockNumber, &r.LogIndex); err != nil {
			return nil, err
		}
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolveTokenPriceHistory(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}

	args := []interface{}{address}
	where := "LOWER(token_address) = LOWER($1)"
	if from, ok := intArg(p, "from"); ok {
		args = append(args, from)
		where += fmt.Sprintf(" AND timestamp >= $%d", len(args))
	}
	if to, ok := intArg(p, "to"); ok {
		args = append(args, to)
		where += fmt.Sprintf(" AND timestamp <= $%d", len(args))
	}

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT timestamp, price_usd::text FROM token_prices WHERE `+where+
			` ORDER BY timestamp ASC LIMIT $`+fmt.Sprint(len(args)+1),
		append(args, maxLimit)...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*pricePointRow, 0, 128)
	for rows.Next() {
		var (
			r   pricePointRow
			avg sql.NullString
		)
		if err := rows.Scan(&r.Timestamp, &avg); err != nil {
			return nil, err
		}
		r.AvgPrice = sp(avg)
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolveNFTCollections(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	args := []interface{}{}
	where := ""
	if search := strings.TrimSpace(strArg(p, "search")); search != "" {
		args = append(args, "%"+strings.ToLower(search)+"%")
		n := len(args)
		where = fmt.Sprintf(" WHERE (LOWER(name) LIKE $%d OR LOWER(address) LIKE $%d)", n, n)
	}

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT address, name, symbol, contract_type, total_supply, holders_count,
		        transfers_count, floor_price::text, volume_24h::text, market_cap::text,
		        description, image_url, external_url, banner_url, is_verified
		 FROM nft_collections`+where+
			` ORDER BY volume_24h DESC NULLS LAST LIMIT $`+fmt.Sprint(len(args)+1)+
			` OFFSET $`+fmt.Sprint(len(args)+2),
		append(args, limit, offset)...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*nftCollectionRow, 0, limit)
	for rows.Next() {
		r, err := scanNFTCollection(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, r)
	}
	return out, rows.Err()
}

func scanNFTCollection(rows *sql.Rows) (*nftCollectionRow, error) {
	var (
		r                                             nftCollectionRow
		symbol, floorPrice, volume, marketCap         sql.NullString
		description, imageURL, externalURL, bannerURL sql.NullString
		totalSupply                                   sql.NullInt64
	)
	if err := rows.Scan(&r.Address, &r.Name, &symbol, &r.ContractType, &totalSupply,
		&r.HoldersCount, &r.TransfersCount, &floorPrice, &volume, &marketCap,
		&description, &imageURL, &externalURL, &bannerURL, &r.IsVerified); err != nil {
		return nil, err
	}
	r.Symbol = sp(symbol)
	r.FloorPrice = sp(floorPrice)
	r.Volume24h = sp(volume)
	r.MarketCap = sp(marketCap)
	r.Description = sp(description)
	r.ImageURL = sp(imageURL)
	r.ExternalURL = sp(externalURL)
	r.BannerURL = sp(bannerURL)
	r.TotalSupply = ip(totalSupply)
	return &r, nil
}

func (s *Server) resolveNFTCollection(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT address, name, symbol, contract_type, total_supply, holders_count,
		        transfers_count, floor_price::text, volume_24h::text, market_cap::text,
		        description, image_url, external_url, banner_url, is_verified
		 FROM nft_collections WHERE LOWER(address) = LOWER($1)`, address)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}
	return scanNFTCollection(rows)
}

func (s *Server) resolveNFT(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	tokenID := strings.TrimSpace(strArg(p, "tokenId"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	if _, err := strconv.ParseUint(tokenID, 10, 64); err != nil {
		return nil, fmt.Errorf("tokenId must be a decimal integer")
	}

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT n.token_address, n.token_id::text, n.owner, n.uri, n.metadata::text,
		        n.name, n.description, n.image_url, n.external_url, n.attributes::text,
		        n.is_burned,
		        n.last_transfer_block,
		        EXTRACT(EPOCH FROM n.last_transfer_timestamp)::BIGINT
		 FROM nfts n WHERE LOWER(n.token_address) = LOWER($1) AND n.token_id = $2`,
		address, tokenID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}

	var (
		r                                                        nftRow
		uri, metadataJSON, name, description, imageURL, external sql.NullString
		attributesJSON                                           sql.NullString
		lastBlock, lastTimestamp                                 sql.NullInt64
	)
	if err := rows.Scan(&r.TokenAddress, &r.TokenID, &r.Owner, &uri, &metadataJSON,
		&name, &description, &imageURL, &external, &attributesJSON, &r.IsBurned,
		&lastBlock, &lastTimestamp); err != nil {
		return nil, err
	}
	r.URI = sp(uri)
	r.MetadataJSON = sp(metadataJSON)
	r.Name = sp(name)
	r.Description = sp(description)
	r.ImageURL = sp(imageURL)
	r.ExternalURL = sp(external)
	r.Attributes = parseNFTAttributes(attributesJSON)
	r.LastTransferBlock = ip(lastBlock)
	r.LastTransferTimestamp = ip(lastTimestamp)
	return &r, nil
}

func parseNFTAttributes(ns sql.NullString) []*nftAttributeRow {
	if !ns.Valid || strings.TrimSpace(ns.String) == "" {
		return []*nftAttributeRow{}
	}
	var raw []struct {
		TraitType   string `json:"trait_type"`
		Value       string `json:"value"`
		DisplayType string `json:"display_type"`
	}
	if err := json.Unmarshal([]byte(ns.String), &raw); err != nil {
		return []*nftAttributeRow{}
	}
	out := make([]*nftAttributeRow, 0, len(raw))
	for _, a := range raw {
		row := &nftAttributeRow{TraitType: a.TraitType, Value: a.Value}
		if a.DisplayType != "" {
			row.DisplayType = &a.DisplayType
		}
		out = append(out, row)
	}
	return out
}

func (s *Server) resolveNFTOwners(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT token_address, token_id::text, owner, updated_block
		 FROM nft_owners WHERE LOWER(token_address) = LOWER($1)
		 ORDER BY token_id LIMIT $2 OFFSET $3`, address, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*nftOwnerRow, 0, limit)
	for rows.Next() {
		var r nftOwnerRow
		if err := rows.Scan(&r.TokenAddress, &r.TokenID, &r.Owner, &r.UpdatedBlock); err != nil {
			return nil, err
		}
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolveNFTTransfers(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	args := []interface{}{address}
	where := "LOWER(nt.token_address) = LOWER($1)"
	if from, ok := intArg(p, "fromBlock"); ok {
		args = append(args, from)
		where += fmt.Sprintf(" AND nt.block_number >= $%d", len(args))
	}

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT nt.token_address, nt.from_address, nt.to_address, nt.token_id::text,
		        nt.value::text, nt.transaction_hash, nt.block_number, nt.log_index,
		        b.timestamp
		 FROM nft_transfers nt
		 LEFT JOIN blocks b ON b.number = nt.block_number
		 WHERE `+where+`
		 ORDER BY nt.block_number DESC, nt.log_index DESC LIMIT $`+
			fmt.Sprint(len(args)+1)+` OFFSET $`+fmt.Sprint(len(args)+2),
		append(args, limit, offset)...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*nftTransferRow, 0, limit)
	for rows.Next() {
		var (
			r         nftTransferRow
			value     sql.NullString
			timestamp sql.NullInt64
		)
		if err := rows.Scan(&r.TokenAddress, &r.From, &r.To, &r.TokenID, &value,
			&r.TransactionHash, &r.BlockNumber, &r.LogIndex, &timestamp); err != nil {
			return nil, err
		}
		r.Value = sp(value)
		r.Timestamp = ip(timestamp)
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolveNFTRarity(p graphql.ResolveParams) (interface{}, error) {
	address := strings.TrimSpace(strArg(p, "address"))
	tokenID := strings.TrimSpace(strArg(p, "tokenId"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	if _, err := strconv.ParseUint(tokenID, 10, 64); err != nil {
		return nil, fmt.Errorf("tokenId must be a decimal integer")
	}
	return resolveNFTRarity(p.Context, address, tokenID)
}

// resolveNFTActivity merges transfers and approvals for a collection into a
// single chronological feed.
func (s *Server) resolveNFTActivity(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT event_type, token_address, token_id, from_address, to_address,
		        transaction_hash, block_number, log_index, timestamp
		 FROM (
		     SELECT 'transfer' AS event_type, nt.token_address, nt.token_id::text AS token_id,
		            nt.from_address, nt.to_address, nt.transaction_hash, nt.block_number,
		            nt.log_index, b.timestamp
		     FROM nft_transfers nt
		     LEFT JOIN blocks b ON b.number = nt.block_number
		     WHERE LOWER(nt.token_address) = LOWER($1)
		     UNION ALL
		     SELECT 'approval', na.token_address, COALESCE(na.token_id::text, ''),
		            na.owner, COALESCE(na.approved, ''), na.transaction_hash,
		            na.block_number, na.log_index, b.timestamp
		     FROM nft_approvals na
		     LEFT JOIN blocks b ON b.number = na.block_number
		     WHERE LOWER(na.token_address) = LOWER($1)
		 ) AS activity
		 ORDER BY block_number DESC, log_index DESC LIMIT $2`, address, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*nftActivityRow, 0, limit)
	for rows.Next() {
		var (
			r         nftActivityRow
			timestamp sql.NullInt64
		)
		if err := rows.Scan(&r.EventType, &r.TokenAddress, &r.TokenID, &r.From, &r.To,
			&r.TransactionHash, &r.BlockNumber, &r.LogIndex, &timestamp); err != nil {
			return nil, err
		}
		r.Timestamp = ip(timestamp)
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolveAccount(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT address, balance::text, nonce, code_hash, is_contract, is_verified,
		        token_balance_count, nft_balance_count
		 FROM accounts WHERE LOWER(address) = LOWER($1)`, address)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}

	var (
		r                 accountRow
		balance, codeHash sql.NullString
	)
	if err := rows.Scan(&r.Address, &balance, &r.Nonce, &codeHash, &r.IsContract,
		&r.IsVerified, &r.TokenBalanceCount, &r.NFTBalanceCount); err != nil {
		return nil, err
	}
	r.Balance = sp(balance)
	r.CodeHash = sp(codeHash)
	return &r, nil
}

func (s *Server) resolveAccountTransactions(p graphql.ResolveParams) (interface{}, error) {
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))
	return s.queryTxs(p.Context,
		`(LOWER(from_address) = LOWER($1) OR LOWER(to_address) = LOWER($1))`,
		[]interface{}{address},
		"block_number DESC NULLS LAST, transaction_index DESC", limit, offset)
}

func (s *Server) resolveAccountTokens(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	rows, err := s.config.DB.QueryContext(p.Context,
		tokenSelect+`, th.balance::text, th.balance_usd::text
		 FROM token_holders th JOIN tokens t ON t.address = th.token_address
		 WHERE LOWER(th.address) = LOWER($1)
		 ORDER BY th.balance DESC LIMIT $2 OFFSET $3`, address, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*accountTokenRow, 0, limit)
	for rows.Next() {
		var balanceUSD sql.NullString
		var balance string
		tok, err := scanTokenRow(rows, &balance, &balanceUSD)
		if err != nil {
			return nil, err
		}
		out = append(out, &accountTokenRow{Token: tok, Balance: balance, BalanceUSD: sp(balanceUSD)})
	}
	return out, rows.Err()
}

func (s *Server) resolveAccountNFTs(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT token_address, token_id::text, owner, updated_block
		 FROM nft_owners WHERE LOWER(owner) = LOWER($1)
		 ORDER BY token_address, token_id LIMIT $2 OFFSET $3`, address, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*accountNFTRow, 0, limit)
	for rows.Next() {
		var (
			r            accountNFTRow
			updatedBlock sql.NullInt64
		)
		if err := rows.Scan(&r.TokenAddress, &r.TokenID, &r.Owner, &updatedBlock); err != nil {
			return nil, err
		}
		r.UpdatedBlock = ip(updatedBlock)
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolveContract(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT address, contract_name, compiler, compiler_version,
		        optimization_enabled, optimization_runs, evm_version, license_type,
		        source_code, abi::text, bytecode, runtime_bytecode, contract_type,
		        is_verified, verification_status,
		        EXTRACT(EPOCH FROM verified_at)::BIGINT
		 FROM contracts WHERE LOWER(address) = LOWER($1)`, address)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}
	return scanContract(rows)
}

func scanContract(rows *sql.Rows) (*contractRow, error) {
	var (
		r                                                    contractRow
		name, compiler, compilerVersion, evmVersion, license sql.NullString
		abi, bytecode, runtimeBytecode, contractType, status sql.NullString
		optimizationEnabled                                  sql.NullBool
		optimizationRuns                                     sql.NullInt64
		verifiedAt                                           sql.NullInt64
	)
	if err := rows.Scan(&r.Address, &name, &compiler, &compilerVersion,
		&optimizationEnabled, &optimizationRuns, &evmVersion, &license,
		&r.SourceCode, &abi, &bytecode, &runtimeBytecode, &contractType,
		&r.IsVerified, &status, &verifiedAt); err != nil {
		return nil, err
	}
	r.ContractName = sp(name)
	r.Compiler = sp(compiler)
	r.CompilerVersion = sp(compilerVersion)
	r.EvmVersion = sp(evmVersion)
	r.LicenseType = sp(license)
	r.ABI = sp(abi)
	r.Bytecode = sp(bytecode)
	r.RuntimeBytecode = sp(runtimeBytecode)
	r.ContractType = sp(contractType)
	r.VerificationStatus = sp(status)
	r.VerifiedAt = ip(verifiedAt)
	if optimizationEnabled.Valid {
		v := optimizationEnabled.Bool
		r.OptimizationEnabled = &v
	}
	if optimizationRuns.Valid {
		v := int(optimizationRuns.Int64)
		r.OptimizationRuns = &v
	}
	return &r, nil
}

func (s *Server) resolveContractSource(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT file_name, source_code, language, compiler_version
		 FROM verified_sources WHERE LOWER(contract_address) = LOWER($1)
		 ORDER BY file_name`, address)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*contractSourceRow, 0, 8)
	for rows.Next() {
		var (
			r                         contractSourceRow
			language, compilerVersion sql.NullString
		)
		if err := rows.Scan(&r.FileName, &r.SourceCode, &language, &compilerVersion); err != nil {
			return nil, err
		}
		r.Language = sp(language)
		r.CompilerVersion = sp(compilerVersion)
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolveContractABI(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	var abi sql.NullString
	err := s.config.DB.QueryRowContext(p.Context,
		`SELECT abi::text FROM contracts WHERE LOWER(address) = LOWER($1)`, address).Scan(&abi)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	if !abi.Valid {
		return nil, nil
	}
	return abi.String, nil
}

func (s *Server) resolveVerifiedContracts(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))

	args := []interface{}{}
	where := []string{"is_verified = TRUE"}
	if search := strings.TrimSpace(strArg(p, "search")); search != "" {
		args = append(args, "%"+strings.ToLower(search)+"%")
		n := len(args)
		where = append(where, fmt.Sprintf("(LOWER(contract_name) LIKE $%d OR LOWER(address) LIKE $%d)", n, n))
	}
	if license := strings.TrimSpace(strArg(p, "license")); license != "" {
		args = append(args, license)
		where = append(where, fmt.Sprintf("license_type = $%d", len(args)))
	}

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT address, contract_name, compiler_version, optimization_enabled,
		        optimization_runs, evm_version, license_type,
		        EXTRACT(EPOCH FROM verified_at)::BIGINT
		 FROM contracts WHERE `+strings.Join(where, " AND ")+`
		 ORDER BY verified_at DESC NULLS LAST LIMIT $`+fmt.Sprint(len(args)+1)+
			` OFFSET $`+fmt.Sprint(len(args)+2),
		append(args, limit, offset)...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*verifiedContractRow, 0, limit)
	for rows.Next() {
		var (
			r                                        verifiedContractRow
			name, compilerVersion, evmVersion, licen sql.NullString
			optimization                             sql.NullBool
			runs, verifiedAt                         sql.NullInt64
		)
		if err := rows.Scan(&r.Address, &name, &compilerVersion, &optimization,
			&runs, &evmVersion, &licen, &verifiedAt); err != nil {
			return nil, err
		}
		r.ContractName = sp(name)
		r.CompilerVersion = sp(compilerVersion)
		r.EvmVersion = sp(evmVersion)
		r.License = sp(licen)
		r.VerifiedAt = ip(verifiedAt)
		if optimization.Valid {
			v := optimization.Bool
			r.Optimization = &v
		}
		if runs.Valid {
			v := int(runs.Int64)
			r.OptimizationRuns = &v
		}
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolveValidators(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))
	offset := clampOffset(argOr(p, "offset", 0))
	order := orderFor(strArg(p, "orderBy"), "total_stake DESC NULLS LAST")

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT address, name, moniker, uptime::text, commission_rate::text,
		        total_stake::text, tokens::text, blocks_count, is_jailed, is_active, status
		 FROM validators ORDER BY `+order+` LIMIT $1 OFFSET $2`, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*validatorRow, 0, limit)
	for rows.Next() {
		r, err := scanValidator(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, r)
	}
	return out, rows.Err()
}

func scanValidator(rows *sql.Rows) (*validatorRow, error) {
	var (
		r                                               validatorRow
		name, moniker, uptime, commission, stake, token sql.NullString
		blocksCount                                     sql.NullInt64
		status                                          sql.NullString
	)
	if err := rows.Scan(&r.Address, &name, &moniker, &uptime, &commission, &stake,
		&token, &blocksCount, &r.IsJailed, &r.IsActive, &status); err != nil {
		return nil, err
	}
	r.Name = sp(name)
	r.Moniker = sp(moniker)
	r.Uptime = sp(uptime)
	r.CommissionRate = sp(commission)
	r.TotalStake = sp(stake)
	r.Tokens = sp(token)
	r.BlocksCount = ip(blocksCount)
	r.Status = sp(status)
	return &r, nil
}

func (s *Server) resolveValidator(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}
	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT address, name, moniker, uptime::text, commission_rate::text,
		        total_stake::text, tokens::text, blocks_count, is_jailed, is_active, status
		 FROM validators WHERE LOWER(address) = LOWER($1)`, address)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}
	return scanValidator(rows)
}

func (s *Server) resolveValidatorRewards(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	address := strings.TrimSpace(strArg(p, "address"))
	if !isHex(address, 40) {
		return nil, fmt.Errorf("address must be a 20-byte hex string")
	}

	args := []interface{}{address}
	where := "LOWER(br.validator) = LOWER($1)"
	if from, ok := intArg(p, "fromBlock"); ok {
		args = append(args, from)
		where += fmt.Sprintf(" AND br.block_number >= $%d", len(args))
	}
	if to, ok := intArg(p, "toBlock"); ok {
		args = append(args, to)
		where += fmt.Sprintf(" AND br.block_number <= $%d", len(args))
	}

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT br.block_number, br.block_hash, br.validator, br.amount::text,
		        br.transaction_hash, b.timestamp
		 FROM block_rewards br
		 LEFT JOIN blocks b ON b.number = br.block_number
		 WHERE `+where+` ORDER BY br.block_number DESC LIMIT $`+
			fmt.Sprint(len(args)+1),
		append(args, maxLimit)...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*validatorRewardRow, 0, 64)
	for rows.Next() {
		var (
			r         validatorRewardRow
			txHash    sql.NullString
			timestamp sql.NullInt64
		)
		if err := rows.Scan(&r.BlockNumber, &r.BlockHash, &r.Validator, &r.Amount,
			&txHash, &timestamp); err != nil {
			return nil, err
		}
		r.TransactionHash = sp(txHash)
		r.Timestamp = ip(timestamp)
		out = append(out, &r)
	}
	return out, rows.Err()
}

// resolveSearch classifies the query and returns the single matching entity.
func (s *Server) resolveSearch(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	query := strings.TrimSpace(strArg(p, "query"))
	if query == "" {
		return nil, fmt.Errorf("query is required")
	}

	switch {
	case isHex(query, 64):
		// A 32-byte value is either a block hash or a transaction hash.
		if tx, err := s.lookupTx(p.Context, query); err != nil {
			return nil, err
		} else if tx != nil {
			return tx, nil
		}
		return s.lookupBlockByHash(p.Context, query)

	case isHex(query, 40):
		// A 20-byte value is a token, a contract, or an account.
		if tok, err := s.lookupToken(p.Context, query); err != nil {
			return nil, err
		} else if tok != nil {
			return tok, nil
		}
		return s.lookupAccount(p.Context, query)

	default:
		n, err := strconv.ParseInt(query, 10, 64)
		if err != nil {
			return nil, fmt.Errorf("query must be a block number, a 20-byte address or a 32-byte hash")
		}
		return s.lookupBlockByNumber(p.Context, n)
	}
}

func (s *Server) lookupTx(ctx context.Context, hash string) (*txRow, error) {
	rows, err := s.config.DB.QueryContext(ctx,
		txSelect+` WHERE LOWER(hash) = LOWER($1)`, hash)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}
	return scanTx(rows)
}

func (s *Server) lookupBlockByHash(ctx context.Context, hash string) (interface{}, error) {
	rows, err := s.config.DB.QueryContext(ctx,
		`SELECT `+blockColumns+` FROM blocks WHERE LOWER(hash) = LOWER($1)`, hash)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}
	return scanBlock(rows)
}

func (s *Server) lookupBlockByNumber(ctx context.Context, number int64) (interface{}, error) {
	rows, err := s.config.DB.QueryContext(ctx,
		`SELECT `+blockColumns+` FROM blocks WHERE number = $1`, number)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}
	return scanBlock(rows)
}

func (s *Server) lookupToken(ctx context.Context, address string) (*tokenRow, error) {
	rows, err := s.config.DB.QueryContext(ctx,
		tokenSelect+` WHERE LOWER(t.address) = LOWER($1)`, address)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}
	return scanTokenRow(rows)
}

func (s *Server) lookupAccount(ctx context.Context, address string) (interface{}, error) {
	rows, err := s.config.DB.QueryContext(ctx,
		`SELECT address, balance::text, nonce, code_hash, is_contract, is_verified,
		        token_balance_count, nft_balance_count
		 FROM accounts WHERE LOWER(address) = LOWER($1)`, address)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	if !rows.Next() {
		return nil, rows.Err()
	}
	var (
		r                 accountRow
		balance, codeHash sql.NullString
	)
	if err := rows.Scan(&r.Address, &balance, &r.Nonce, &codeHash, &r.IsContract,
		&r.IsVerified, &r.TokenBalanceCount, &r.NFTBalanceCount); err != nil {
		return nil, err
	}
	r.Balance = sp(balance)
	r.CodeHash = sp(codeHash)
	return &r, nil
}

// resolveStats aggregates real counters from the indexed tables.
func (s *Server) resolveStats(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	var (
		r             networkStatsRow
		minTs, maxTs  sql.NullInt64
		latestBaseFee sql.NullInt64
	)
	err := s.config.DB.QueryRowContext(p.Context, `
		SELECT
			(SELECT COUNT(*) FROM blocks),
			(SELECT COUNT(*) FROM transactions),
			(SELECT COUNT(*) FROM tokens),
			(SELECT COUNT(*) FROM accounts),
			(SELECT COUNT(*) FROM contracts),
			(SELECT MIN(timestamp) FROM blocks),
			(SELECT MAX(timestamp) FROM blocks),
			(SELECT base_fee_per_gas FROM blocks ORDER BY number DESC LIMIT 1)
	`).Scan(&r.TotalBlocks, &r.TotalTransactions, &r.TotalTokens, &r.TotalAccounts,
		&r.TotalContracts, &minTs, &maxTs, &latestBaseFee)
	if err != nil {
		return nil, err
	}
	r.MinTimestamp = ip(minTs)
	r.MaxTimestamp = ip(maxTs)
	r.LatestBaseFee = ip(latestBaseFee)
	return &r, nil
}

func (s *Server) resolveGasPrice(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	// Percentiles over the most recent 100 blocks of recorded gas prices.
	var (
		r                            gasPriceRow
		low, standard, fast, baseFee sql.NullFloat64
	)
	err := s.config.DB.QueryRowContext(p.Context, `
		SELECT
			percentile_cont(0.25) WITHIN GROUP (ORDER BY gas_price),
			percentile_cont(0.50) WITHIN GROUP (ORDER BY gas_price),
			percentile_cont(0.90) WITHIN GROUP (ORDER BY gas_price),
			MAX(base_fee)
		FROM gas_prices
		WHERE timestamp >= (SELECT COALESCE(MAX(timestamp), 0) - 3600 FROM gas_prices)
	`).Scan(&low, &standard, &fast, &baseFee)
	if err != nil {
		return nil, err
	}
	if low.Valid {
		r.Low = &low.Float64
	}
	if standard.Valid {
		r.Standard = &standard.Float64
	}
	if fast.Valid {
		r.Fast = &fast.Float64
	}
	if baseFee.Valid {
		r.BaseFee = &baseFee.Float64
	}
	return &r, nil
}

func (s *Server) resolveGasHistory(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}

	args := []interface{}{}
	where := []string{}
	if from, ok := intArg(p, "from"); ok {
		args = append(args, from)
		where = append(where, fmt.Sprintf("timestamp >= $%d", len(args)))
	}
	if to, ok := intArg(p, "to"); ok {
		args = append(args, to)
		where = append(where, fmt.Sprintf("timestamp <= $%d", len(args)))
	}

	q := `SELECT (timestamp / 60) * 60 AS bucket, AVG(gas_price)::text,
	             MIN(gas_price)::text, MAX(gas_price)::text, COUNT(*)
	      FROM gas_prices`
	if len(where) > 0 {
		q += " WHERE " + strings.Join(where, " AND ")
	}
	q += ` GROUP BY bucket ORDER BY bucket ASC LIMIT $` + fmt.Sprint(len(args)+1)

	rows, err := s.config.DB.QueryContext(p.Context, q, append(args, maxLimit)...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*gasPricePointRow, 0, 128)
	for rows.Next() {
		var (
			r               gasPricePointRow
			avg, minP, maxP sql.NullString
		)
		if err := rows.Scan(&r.Timestamp, &avg, &minP, &maxP, &r.TransactionsCount); err != nil {
			return nil, err
		}
		r.AvgGasPrice = sp(avg)
		r.MinGasPrice = sp(minP)
		r.MaxGasPrice = sp(maxP)
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolvePendingTransactions(p graphql.ResolveParams) (interface{}, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	limit := clampLimit(argOr(p, "limit", defaultLimit))

	args := []interface{}{}
	where := []string{"(expires_at IS NULL OR expires_at > NOW())"}
	if from := strArg(p, "from"); from != "" {
		if !isHex(from, 40) {
			return nil, fmt.Errorf("from must be a 20-byte hex string")
		}
		args = append(args, from)
		where = append(where, fmt.Sprintf("LOWER(from_address) = LOWER($%d)", len(args)))
	}
	if to := strArg(p, "to"); to != "" {
		if !isHex(to, 40) {
			return nil, fmt.Errorf("to must be a 20-byte hex string")
		}
		args = append(args, to)
		where = append(where, fmt.Sprintf("LOWER(to_address) = LOWER($%d)", len(args)))
	}

	rows, err := s.config.DB.QueryContext(p.Context,
		`SELECT hash, nonce, from_address, to_address, value::text, gas_price,
		        gas_limit, input, EXTRACT(EPOCH FROM created_at)::BIGINT
		 FROM pending_transactions WHERE `+strings.Join(where, " AND ")+`
		 ORDER BY gas_price DESC LIMIT $`+fmt.Sprint(len(args)+1),
		append(args, limit)...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*pendingTxRow, 0, limit)
	for rows.Next() {
		var (
			r         pendingTxRow
			to, input sql.NullString
			createdAt sql.NullInt64
		)
		if err := rows.Scan(&r.Hash, &r.Nonce, &r.From, &to, &r.Value, &r.GasPrice,
			&r.GasLimit, &input, &createdAt); err != nil {
			return nil, err
		}
		r.To = sp(to)
		r.Input = sp(input)
		r.CreatedAt = ip(createdAt)
		out = append(out, &r)
	}
	return out, rows.Err()
}

func (s *Server) resolveStatsTPS(ctx context.Context) (float64, float64) {
	if s.config.DB == nil {
		return 0, 0
	}
	var (
		count int64
		span  sql.NullInt64
	)
	err := s.config.DB.QueryRowContext(ctx, `
		SELECT COUNT(*), MAX(timestamp) - MIN(timestamp)
		FROM blocks
		WHERE number >= (SELECT COALESCE(MAX(number), 0) - 100 FROM blocks)
	`).Scan(&count, &span)
	if err != nil || !span.Valid || span.Int64 <= 0 {
		return 0, 0
	}
	blockTime := float64(span.Int64) / float64(max64(count-1, 1))
	if blockTime <= 0 {
		return 0, 0
	}
	return float64(count) / float64(span.Int64), blockTime
}

func max64(a, b int64) int64 {
	if a > b {
		return a
	}
	return b
}

// argOr reads an integer argument, falling back to def when it is absent.
func argOr(p graphql.ResolveParams, name string, def int) int {
	if v, ok := intArg(p, name); ok {
		return v
	}
	return def
}
