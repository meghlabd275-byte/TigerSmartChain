package graphql

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

// serverCtxKey is the context key under which the active *Server travels into
// nested field resolvers.
type serverCtxKey struct{}

// withServer attaches s to ctx so that package-level nested resolvers can reach
// the configured database pool.
func withServer(ctx context.Context, s *Server) context.Context {
	return context.WithValue(ctx, serverCtxKey{}, s)
}

// serverFromCtx returns the *Server bound to ctx, or nil.
func serverFromCtx(ctx context.Context) *Server {
	if ctx == nil {
		return nil
	}
	if s, ok := ctx.Value(serverCtxKey{}).(*Server); ok {
		return s
	}
	return nil
}

// ---------------------------------------------------------------------------
// null helpers
// ---------------------------------------------------------------------------

func sp(ns sql.NullString) *string {
	if !ns.Valid {
		return nil
	}
	v := ns.String
	return &v
}

func ip(ni sql.NullInt64) *int64 {
	if !ni.Valid {
		return nil
	}
	v := ni.Int64
	return &v
}

func intPtr(ni sql.NullInt64) *int {
	if !ni.Valid {
		return nil
	}
	v := int(ni.Int64)
	return &v
}

// int64ToStringPtr renders a nullable 64-bit column as a decimal string so that
// NUMERIC/BIGINT amounts keep full precision through GraphQL.
func int64ToStringPtr(n sql.NullInt64) *string {
	if !n.Valid {
		return nil
	}
	v := strconv.FormatInt(n.Int64, 10)
	return &v
}

func nullStringPtr(p *string) interface{} {
	if p == nil {
		return nil
	}
	return *p
}

// ---------------------------------------------------------------------------
// block scanner
// ---------------------------------------------------------------------------

const blockColumns = `number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
	transactions_root, state_root, receipts_root, miner, difficulty, total_difficulty,
	gas_limit, gas_used, timestamp, size, extra_data, base_fee_per_gas, blob_gas_used,
	excess_blob_gas, parent_beacon_block_root, tx_count, uncle_count, reward, is_uncle`

func scanBlock(rows *sql.Rows) (*blockRow, error) {
	var (
		r                                                             blockRow
		nonce, sha3Uncles, logsBloom, txRoot, stateRoot, receiptsRoot sql.NullString
		difficulty, totalDifficulty, extraData, beaconRoot, reward    sql.NullString
		baseFee, blobGasUsed, excessBlobGas                           sql.NullInt64
	)
	if err := rows.Scan(
		&r.Number, &r.Hash, &r.ParentHash, &nonce, &sha3Uncles, &logsBloom,
		&txRoot, &stateRoot, &receiptsRoot, &r.Miner, &difficulty, &totalDifficulty,
		&r.GasLimit, &r.GasUsed, &r.Timestamp, &r.Size, &extraData, &baseFee,
		&blobGasUsed, &excessBlobGas, &beaconRoot, &r.TxCount, &r.UncleCount,
		&reward, &r.IsUncle,
	); err != nil {
		return nil, err
	}
	r.Nonce = sp(nonce)
	r.Sha3Uncles = sp(sha3Uncles)
	r.LogsBloom = sp(logsBloom)
	r.TransactionsRoot = sp(txRoot)
	r.StateRoot = sp(stateRoot)
	r.ReceiptsRoot = sp(receiptsRoot)
	r.Difficulty = sp(difficulty)
	r.TotalDifficulty = sp(totalDifficulty)
	r.ExtraData = sp(extraData)
	r.ParentBeaconBlockRoot = sp(beaconRoot)
	r.Reward = sp(reward)
	r.BaseFeePerGas = ip(baseFee)
	r.BlobGasUsed = ip(blobGasUsed)
	r.ExcessBlobGas = ip(excessBlobGas)
	return &r, nil
}

// ---------------------------------------------------------------------------
// transaction scanner
// ---------------------------------------------------------------------------

const txColumns = `hash, nonce, block_number, block_hash, transaction_index, from_address,
	to_address, value::text, gas_price, gas_limit, gas_used, max_fee_per_gas,
	max_priority_fee_per_gas, input, v, r, s, chain_id, transaction_type, status,
	cumulative_gas_used, effective_gas_price, contract_address,
	EXTRACT(EPOCH FROM block_timestamp)::BIGINT`

const txSelect = `SELECT ` + txColumns + ` FROM transactions`

func scanTx(rows *sql.Rows) (*txRow, error) {
	var (
		r                                                     txRow
		blockNumber, gasUsed, maxFee, maxPriorityFee, chainID sql.NullInt64
		txIndex, blockTimestamp                               sql.NullInt64
		cumulativeGasUsed, effectiveGasPrice                  sql.NullInt64
		blockHash, to, input, rSig, sSig, txType, contractAdr sql.NullString
	)
	if err := rows.Scan(
		&r.Hash, &r.Nonce, &blockNumber, &blockHash, &txIndex, &r.From,
		&to, &r.Value, &r.GasPrice, &r.GasLimit, &gasUsed, &maxFee,
		&maxPriorityFee, &input, &r.V, &rSig, &sSig, &chainID, &txType, &r.Status,
		&cumulativeGasUsed, &effectiveGasPrice, &contractAdr, &blockTimestamp,
	); err != nil {
		return nil, err
	}
	r.BlockNumber = ip(blockNumber)
	r.BlockHash = sp(blockHash)
	r.TransactionIndex = intPtr(txIndex)
	r.To = sp(to)
	r.GasUsed = ip(gasUsed)
	r.MaxFeePerGas = ip(maxFee)
	r.MaxPriorityFeePerGas = ip(maxPriorityFee)
	r.Input = sp(input)
	r.R = sp(rSig)
	r.S = sp(sSig)
	r.ChainID = ip(chainID)
	r.TransactionType = sp(txType)
	r.CumulativeGasUsed = int64ToStringPtr(cumulativeGasUsed)
	r.EffectiveGasPrice = int64ToStringPtr(effectiveGasPrice)
	r.ContractAddress = sp(contractAdr)
	r.BlockTimestamp = ip(blockTimestamp)
	return &r, nil
}

// queryTxs runs a transaction query with a caller-supplied WHERE fragment. The
// fragment must be a constant string; every user-supplied value travels as a
// bound parameter.
func (s *Server) queryTxs(ctx context.Context, where string, args []interface{}, order string, limit, offset int) ([]*txRow, error) {
	if s.config.DB == nil {
		return nil, errNoDatabase
	}
	q := txSelect
	if where != "" {
		q += " WHERE " + where
	}
	q += " ORDER BY " + order +
		" LIMIT $" + fmt.Sprint(len(args)+1) +
		" OFFSET $" + fmt.Sprint(len(args)+2)
	args = append(args, limit, offset)

	rows, err := s.config.DB.QueryContext(ctx, q, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*txRow, 0, limit)
	for rows.Next() {
		r, err := scanTx(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, r)
	}
	return out, rows.Err()
}

// ---------------------------------------------------------------------------
// nested field resolvers
// ---------------------------------------------------------------------------

func resolveReceipt(ctx context.Context, hash string) (interface{}, error) {
	s := serverFromCtx(ctx)
	if s == nil || s.config.DB == nil {
		return nil, errNoDatabase
	}
	row := s.config.DB.QueryRowContext(ctx,
		`SELECT hash, block_number, block_hash, contract_address, cumulative_gas_used,
		        gas_used, effective_gas_price, status, logs
		 FROM transactions WHERE hash = $1`, hash)

	var (
		r                                                     receiptRow
		blockNumber, gasUsed, cumulativeGasUsed, effectiveGas sql.NullInt64
		blockHash, contractAdr, logsJSON                      sql.NullString
	)
	if err := row.Scan(&r.TransactionHash, &blockNumber, &blockHash, &contractAdr,
		&cumulativeGasUsed, &gasUsed, &effectiveGas, &r.Status, &logsJSON); err != nil {
		if err == sql.ErrNoRows {
			return nil, nil
		}
		return nil, err
	}
	r.BlockNumber = ip(blockNumber)
	r.BlockHash = sp(blockHash)
	r.ContractAddress = sp(contractAdr)
	r.CumulativeGasUsed = int64ToStringPtr(cumulativeGasUsed)
	r.GasUsed = ip(gasUsed)
	r.EffectiveGasPrice = int64ToStringPtr(effectiveGas)
	r.Logs = parseLogs(logsJSON)
	return &r, nil
}

// parseLogs decodes the JSONB logs column written by the indexer.
func parseLogs(ns sql.NullString) []*logRow {
	if !ns.Valid || strings.TrimSpace(ns.String) == "" {
		return []*logRow{}
	}
	var raw []struct {
		Address          string   `json:"address"`
		Topics           []string `json:"topics"`
		Data             string   `json:"data"`
		LogIndex         *int     `json:"logIndex"`
		TransactionIndex *int     `json:"transactionIndex"`
		BlockNumber      *int64   `json:"blockNumber"`
	}
	if err := json.Unmarshal([]byte(ns.String), &raw); err != nil {
		return []*logRow{}
	}
	out := make([]*logRow, 0, len(raw))
	for _, l := range raw {
		row := &logRow{Address: l.Address, Topics: l.Topics, Data: l.Data}
		if row.Topics == nil {
			row.Topics = []string{}
		}
		if l.LogIndex != nil {
			row.LogIndex = *l.LogIndex
		}
		if l.TransactionIndex != nil {
			row.TransactionIndex = *l.TransactionIndex
		}
		if l.BlockNumber != nil {
			row.BlockNumber = *l.BlockNumber
		}
		out = append(out, row)
	}
	return out
}

func resolveBlockTransactions(ctx context.Context, number int64) (interface{}, error) {
	s := serverFromCtx(ctx)
	if s == nil || s.config.DB == nil {
		return nil, errNoDatabase
	}
	rows, err := s.config.DB.QueryContext(ctx,
		txSelect+` WHERE block_number = $1 ORDER BY transaction_index`, number)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*txRow, 0, 32)
	for rows.Next() {
		r, err := scanTx(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, r)
	}
	return out, rows.Err()
}

func resolveBlockUncles(ctx context.Context, number int64) (interface{}, error) {
	s := serverFromCtx(ctx)
	if s == nil || s.config.DB == nil {
		return nil, errNoDatabase
	}
	rows, err := s.config.DB.QueryContext(ctx,
		`SELECT number, hash, parent_hash, NULL::text, NULL::text, NULL::text,
		        NULL::text, NULL::text, NULL::text, miner, difficulty, NULL::text,
		        gas_limit, gas_used, timestamp, 0::bigint, NULL::text, NULL::bigint,
		        NULL::bigint, NULL::bigint, NULL::text, 0, 0, reward, TRUE
		 FROM uncles WHERE block_number = $1 ORDER BY number`, number)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]*blockRow, 0, 4)
	for rows.Next() {
		r, err := scanBlock(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, r)
	}
	return out, rows.Err()
}

func resolveNFTRarity(ctx context.Context, collection, tokenID string) (interface{}, error) {
	s := serverFromCtx(ctx)
	if s == nil || s.config.DB == nil {
		return nil, errNoDatabase
	}
	row := s.config.DB.QueryRowContext(ctx,
		`SELECT collection_address, token_id, rarity_score::text, rank, traits
		 FROM nft_rarity WHERE collection_address = $1 AND token_id = $2`,
		collection, tokenID)

	var (
		r           nftRarityRow
		rarityScore sql.NullString
		rank        sql.NullInt64
		traitsJSON  sql.NullString
	)
	if err := row.Scan(&r.CollectionAddress, &r.TokenID, &rarityScore, &rank, &traitsJSON); err != nil {
		if err == sql.ErrNoRows {
			return nil, nil
		}
		return nil, err
	}
	r.RarityScore = sp(rarityScore)
	if rank.Valid {
		v := int(rank.Int64)
		r.Rank = &v
	}
	r.Traits = parseTraits(traitsJSON)
	return &r, nil
}

func parseTraits(ns sql.NullString) []*traitRarityRow {
	if !ns.Valid || strings.TrimSpace(ns.String) == "" {
		return []*traitRarityRow{}
	}
	var raw []struct {
		TraitType  string   `json:"trait_type"`
		Value      string   `json:"value"`
		Occurrence *float64 `json:"occurrence"`
	}
	if err := json.Unmarshal([]byte(ns.String), &raw); err != nil {
		return []*traitRarityRow{}
	}
	out := make([]*traitRarityRow, 0, len(raw))
	for _, t := range raw {
		row := &traitRarityRow{TraitType: t.TraitType, Value: t.Value}
		if t.Occurrence != nil {
			v := fmt.Sprintf("%g", *t.Occurrence)
			row.Occurrence = &v
		}
		out = append(out, row)
	}
	return out
}

func resolveAccountNFTCollection(ctx context.Context, tokenAddress string) (interface{}, error) {
	return queryNFTCollection(ctx, tokenAddress)
}

// ---------------------------------------------------------------------------
// token scanning
// ---------------------------------------------------------------------------

const tokenColumns = `t.address, t.name, t.symbol, t.decimals, t.total_supply::text,
	t.holders_count, t.transfers_count, t.circulating_supply::text, t.price_usd::text,
	t.price_change_24h::text, t.market_cap::text, t.volume_24h::text, t.is_verified,
	t.is_spam, t.spam_score, t.contract_address, t.logo_url, t.description, t.website`

const tokenSelect = `SELECT ` + tokenColumns + ` FROM tokens t`

// scanTokenRow scans the tokenColumns projection followed by any extra
// destinations supplied by the caller.
func scanTokenRow(rows *sql.Rows, extra ...interface{}) (*tokenRow, error) {
	var (
		r                                                               tokenRow
		totalSupply, circulating, price, priceChange, marketCap, volume sql.NullString
		contractAdr, logo, description, website                         sql.NullString
	)
	args := []interface{}{&r.Address, &r.Name, &r.Symbol, &r.Decimals, &totalSupply,
		&r.HoldersCount, &r.TransfersCount, &circulating, &price, &priceChange,
		&marketCap, &volume, &r.IsVerified, &r.IsSpam, &r.SpamScore, &contractAdr,
		&logo, &description, &website}
	args = append(args, extra...)

	if err := rows.Scan(args...); err != nil {
		return nil, err
	}
	r.TotalSupply = sp(totalSupply)
	r.CirculatingSupply = sp(circulating)
	r.PriceUSD = sp(price)
	r.PriceChange24h = sp(priceChange)
	r.MarketCap = sp(marketCap)
	r.Volume24h = sp(volume)
	r.ContractAddress = sp(contractAdr)
	r.LogoURL = sp(logo)
	r.Description = sp(description)
	r.Website = sp(website)
	return &r, nil
}

func queryNFTCollection(ctx context.Context, address string) (interface{}, error) {
	s := serverFromCtx(ctx)
	if s == nil || s.config.DB == nil {
		return nil, errNoDatabase
	}
	row := s.config.DB.QueryRowContext(ctx,
		`SELECT address, name, symbol, contract_type, total_supply, holders_count,
		        transfers_count, floor_price::text, volume_24h::text, market_cap::text,
		        description, image_url, external_url, banner_url, is_verified
		 FROM nft_collections WHERE address = $1`, address)

	var (
		r                                             nftCollectionRow
		symbol, floorPrice, volume, marketCap         sql.NullString
		description, imageURL, externalURL, bannerURL sql.NullString
		totalSupply                                   sql.NullInt64
	)
	if err := row.Scan(&r.Address, &r.Name, &symbol, &r.ContractType, &totalSupply,
		&r.HoldersCount, &r.TransfersCount, &floorPrice, &volume, &marketCap,
		&description, &imageURL, &externalURL, &bannerURL, &r.IsVerified); err != nil {
		if err == sql.ErrNoRows {
			return nil, nil
		}
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

// ---------------------------------------------------------------------------
// contract verification
// ---------------------------------------------------------------------------

type verifyRequest struct {
	Address         string            `json:"address"`
	ContractName    string            `json:"contract_name"`
	CompilerVersion string            `json:"compiler_version"`
	Optimization    bool              `json:"optimization"`
	OptimizerRuns   int               `json:"optimizer_runs"`
	EVMVersion      string            `json:"evm_version"`
	License         string            `json:"license"`
	Sources         map[string]string `json:"sources"`
	LibraryLinks    map[string]string `json:"library_links"`
	ConstructorArgs string            `json:"constructor_args"`
}

type verifyResponse struct {
	Success bool   `json:"success"`
	Address string `json:"address"`
	Error   string `json:"error"`
}

// callVerifier forwards a verification request to the configured verification
// service and returns its verdict. When no service is configured the caller is
// told verification is unavailable; success is never assumed.
func (s *Server) callVerifier(ctx context.Context, path string, payload interface{}) (*verifyResponse, error) {
	if s.config.VerifierURL == "" {
		return &verifyResponse{
			Success: false,
			Error:   "contract verifier is not configured (set Config.VerifierURL)",
		}, nil
	}

	body, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}

	endpoint, err := url.Parse(strings.TrimRight(s.config.VerifierURL, "/") + path)
	if err != nil {
		return nil, fmt.Errorf("invalid verifier url: %w", err)
	}

	timeout := s.config.VerifierTimeout
	if timeout <= 0 {
		timeout = 30 * time.Second
	}
	reqCtx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()

	req, err := http.NewRequestWithContext(reqCtx, http.MethodPost, endpoint.String(), strings.NewReader(string(body)))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")

	client := &http.Client{Timeout: timeout}
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("verifier request failed: %w", err)
	}
	defer resp.Body.Close()

	data, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return nil, err
	}
	if resp.StatusCode < 200 || resp.StatusCode > 299 {
		return &verifyResponse{
			Success: false,
			Error:   fmt.Sprintf("verifier returned %s: %s", resp.Status, strings.TrimSpace(string(data))),
		}, nil
	}

	var out verifyResponse
	if err := json.Unmarshal(data, &out); err != nil {
		return nil, fmt.Errorf("verifier returned an unreadable response: %w", err)
	}
	return &out, nil
}

func (s *Server) resolveVerifyContract(ctx context.Context, input map[string]interface{}) (interface{}, error) {
	req := verifyRequest{
		Address:         argString(input, "address"),
		ContractName:    argString(input, "contractName"),
		CompilerVersion: argString(input, "compilerVersion"),
		Optimization:    argBool(input, "optimization"),
		OptimizerRuns:   argInt(input, "optimizerRuns"),
		EVMVersion:      argString(input, "evmVersion"),
		License:         argString(input, "license"),
		ConstructorArgs: argString(input, "constructorArgs"),
		Sources:         map[string]string{},
		LibraryLinks:    map[string]string{},
	}

	if srcs, ok := input["sources"].([]interface{}); ok {
		for _, raw := range srcs {
			if m, ok := raw.(map[string]interface{}); ok {
				req.Sources[argString(m, "fileName")] = argString(m, "content")
			}
		}
	}
	if links, ok := input["libraryLinks"].([]interface{}); ok {
		for _, raw := range links {
			if m, ok := raw.(map[string]interface{}); ok {
				req.LibraryLinks[argString(m, "name")] = argString(m, "address")
			}
		}
	}

	resp, err := s.callVerifier(ctx, "/verify", req)
	if err != nil {
		return nil, err
	}
	out := &verifyContractResult{Success: resp.Success}
	if resp.Address != "" {
		out.Address = &resp.Address
	}
	if resp.Error != "" {
		out.Error = &resp.Error
	}
	return out, nil
}

func (s *Server) resolveVerifyProxy(ctx context.Context, input map[string]interface{}) (interface{}, error) {
	payload := map[string]string{
		"proxy_address":    argString(input, "proxyAddress"),
		"implementation":   argString(input, "implementation"),
		"compiler_version": argString(input, "compilerVersion"),
		"proxy_type":       argString(input, "proxyType"),
	}
	resp, err := s.callVerifier(ctx, "/verify-proxy", payload)
	if err != nil {
		return nil, err
	}
	out := &verifyProxyResult{Success: resp.Success}
	if resp.Address != "" {
		out.Address = &resp.Address
	}
	if resp.Error != "" {
		out.Error = &resp.Error
	}
	return out, nil
}

func argString(m map[string]interface{}, key string) string {
	if v, ok := m[key].(string); ok {
		return v
	}
	return ""
}

func argBool(m map[string]interface{}, key string) bool {
	if v, ok := m[key].(bool); ok {
		return v
	}
	return false
}

func argInt(m map[string]interface{}, key string) int {
	switch v := m[key].(type) {
	case int:
		return v
	case int64:
		return int(v)
	case float64:
		return int(v)
	}
	return 0
}
