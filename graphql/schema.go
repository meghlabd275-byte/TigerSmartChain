package graphql

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"strconv"

	"github.com/graphql-go/graphql"
)

const (
	defaultPageSize = 20
	maxPageSize     = 100
)

// buildSchema assembles the executable schema.
func (s *Server) buildSchema() (graphql.Schema, error) {
	blockType := graphql.NewObject(graphql.ObjectConfig{
		Name: "Block",
		Fields: graphql.Fields{
			"number":       &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"hash":         &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"parentHash":   &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"timestamp":    &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"txCount":      &graphql.Field{Type: graphql.NewNonNull(graphql.Int)},
			"uncleCount":   &graphql.Field{Type: graphql.NewNonNull(graphql.Int)},
			"gasUsed":      &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"gasLimit":     &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"miner":        &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"size":         &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"stateRoot":    &graphql.Field{Type: graphql.String},
			"transactions": &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(graphql.String)))},
		},
	})

	transactionType := graphql.NewObject(graphql.ObjectConfig{
		Name: "Transaction",
		Fields: graphql.Fields{
			"hash":             &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"nonce":            &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"blockNumber":      &graphql.Field{Type: graphql.String},
			"blockHash":        &graphql.Field{Type: graphql.String},
			"transactionIndex": &graphql.Field{Type: graphql.Int},
			"from":             &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"to":               &graphql.Field{Type: graphql.String},
			"value":            &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"gasPrice":         &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"gasLimit":         &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"gasUsed":          &graphql.Field{Type: graphql.String},
			"input":            &graphql.Field{Type: graphql.String},
			"status":           &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"timestamp":        &graphql.Field{Type: graphql.String},
			"contractAddress":  &graphql.Field{Type: graphql.String},
		},
	})

	tokenType := graphql.NewObject(graphql.ObjectConfig{
		Name: "Token",
		Fields: graphql.Fields{
			"address":        &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"name":           &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"symbol":         &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"decimals":       &graphql.Field{Type: graphql.NewNonNull(graphql.Int)},
			"totalSupply":    &graphql.Field{Type: graphql.String},
			"holdersCount":   &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"transfersCount": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"priceUsd":       &graphql.Field{Type: graphql.String},
			"marketCap":      &graphql.Field{Type: graphql.String},
			"volume24h":      &graphql.Field{Type: graphql.String},
			"isVerified":     &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
			"holders":        &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(graphql.String)))},
		},
	})

	tokenHolderType := graphql.NewObject(graphql.ObjectConfig{
		Name: "TokenHolder",
		Fields: graphql.Fields{
			"tokenAddress": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"address":      &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"balance":      &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"percent":      &graphql.Field{Type: graphql.String},
		},
	})

	nftType := graphql.NewObject(graphql.ObjectConfig{
		Name: "NFT",
		Fields: graphql.Fields{
			"tokenAddress": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"tokenId":      &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"name":         &graphql.Field{Type: graphql.String},
			"owner":        &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"uri":          &graphql.Field{Type: graphql.String},
			"imageUrl":     &graphql.Field{Type: graphql.String},
			"attributes":   &graphql.Field{Type: graphql.String},
		},
	})

	accountType := graphql.NewObject(graphql.ObjectConfig{
		Name: "Account",
		Fields: graphql.Fields{
			"address":       &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"balance":       &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"nonce":         &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"codeHash":      &graphql.Field{Type: graphql.String},
			"isContract":    &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
			"isVerified":    &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
			"tokenHoldings": &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(tokenHolderType)))},
		},
	})

	validatorType := graphql.NewObject(graphql.ObjectConfig{
		Name: "Validator",
		Fields: graphql.Fields{
			"address":        &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"name":           &graphql.Field{Type: graphql.String},
			"commissionRate": &graphql.Field{Type: graphql.String},
			"totalStake":     &graphql.Field{Type: graphql.String},
			"delegation":     &graphql.Field{Type: graphql.String},
			"uptime":         &graphql.Field{Type: graphql.String},
			"blocksCount":    &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"missesCount":    &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"isActive":       &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
			"isJailed":       &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		},
	})

	searchResultType := graphql.NewObject(graphql.ObjectConfig{
		Name: "SearchResult",
		Fields: graphql.Fields{
			"blocks":       &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(blockType)))},
			"transactions": &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(transactionType)))},
			"tokens":       &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(tokenType)))},
			"accounts":     &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(accountType)))},
		},
	})

	chainStatsType := graphql.NewObject(graphql.ObjectConfig{
		Name: "ChainStats",
		Fields: graphql.Fields{
			"totalBlocks":       &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"totalTransactions": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"totalAddresses":    &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"latestBlock":       &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"averageGasPrice":   &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
			"tps":               &graphql.Field{Type: graphql.NewNonNull(graphql.Float)},
			"tokensTracked":     &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		},
	})

	pageInfoType := graphql.NewObject(graphql.ObjectConfig{
		Name: "PageInfo",
		Fields: graphql.Fields{
			"endCursor":   &graphql.Field{Type: graphql.String},
			"hasNextPage": &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		},
	})

	blockPageType := graphql.NewObject(graphql.ObjectConfig{
		Name:   "BlockPage",
		Fields: graphql.Fields{"items": &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(blockType)))}, "pageInfo": &graphql.Field{Type: graphql.NewNonNull(pageInfoType)}},
	})
	transactionPageType := graphql.NewObject(graphql.ObjectConfig{
		Name:   "TransactionPage",
		Fields: graphql.Fields{"items": &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(transactionType)))}, "pageInfo": &graphql.Field{Type: graphql.NewNonNull(pageInfoType)}},
	})
	tokenPageType := graphql.NewObject(graphql.ObjectConfig{
		Name:   "TokenPage",
		Fields: graphql.Fields{"items": &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(tokenType)))}, "pageInfo": &graphql.Field{Type: graphql.NewNonNull(pageInfoType)}},
	})
	nftPageType := graphql.NewObject(graphql.ObjectConfig{
		Name:   "NFTPage",
		Fields: graphql.Fields{"items": &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(nftType)))}, "pageInfo": &graphql.Field{Type: graphql.NewNonNull(pageInfoType)}},
	})

	pageArgs := graphql.FieldConfigArgument{
		"first": &graphql.ArgumentConfig{Type: graphql.Int, DefaultValue: defaultPageSize},
		"after": &graphql.ArgumentConfig{Type: graphql.String},
	}

	queryType := graphql.NewObject(graphql.ObjectConfig{
		Name: "Query",
		Fields: graphql.Fields{
			"blocks": &graphql.Field{
				Type: blockPageType,
				Args: pageArgs,
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					offset, err := cursorOffset(p.Args)
					if err != nil {
						return nil, err
					}
					items, err := s.blocks(p.Context, pageSize(p.Args), offset, nil, nil)
					if err != nil {
						return nil, err
					}
					return pageResult(items, offset, pageSize(p.Args)), nil
				},
			},
			"block": &graphql.Field{
				Type: blockType,
				Args: graphql.FieldConfigArgument{
					"number": &graphql.ArgumentConfig{Type: graphql.String},
					"hash":   &graphql.ArgumentConfig{Type: graphql.String},
				},
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					number, _ := p.Args["number"].(string)
					hash, _ := p.Args["hash"].(string)
					if number == "" && hash == "" {
						return nil, fmt.Errorf("either number or hash is required")
					}
					items, err := s.blocks(p.Context, 1, 0, ptrString(number), ptrString(hash))
					if err != nil {
						return nil, err
					}
					if len(items) == 0 {
						return nil, nil
					}
					return items[0], nil
				},
			},
			"transactions": &graphql.Field{
				Type: transactionPageType,
				Args: mergeArgs(pageArgs, graphql.FieldConfigArgument{
					"from":        &graphql.ArgumentConfig{Type: graphql.String},
					"to":          &graphql.ArgumentConfig{Type: graphql.String},
					"blockNumber": &graphql.ArgumentConfig{Type: graphql.String},
				}),
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					offset, err := cursorOffset(p.Args)
					if err != nil {
						return nil, err
					}
					filter := transactionFilter{
						from:   optString(p.Args, "from"),
						to:     optString(p.Args, "to"),
						block:  optString(p.Args, "blockNumber"),
						limit:  pageSize(p.Args),
						offset: offset,
					}
					items, err := s.transactions(p.Context, filter)
					if err != nil {
						return nil, err
					}
					return pageResult(items, offset, pageSize(p.Args)), nil
				},
			},
			"transaction": &graphql.Field{
				Type: transactionType,
				Args: graphql.FieldConfigArgument{"hash": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)}},
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					hash, _ := p.Args["hash"].(string)
					items, err := s.transactions(p.Context, transactionFilter{hash: hash, limit: 1})
					if err != nil {
						return nil, err
					}
					if len(items) == 0 {
						return nil, nil
					}
					return items[0], nil
				},
			},
			"tokens": &graphql.Field{
				Type: tokenPageType,
				Args: mergeArgs(pageArgs, graphql.FieldConfigArgument{"search": &graphql.ArgumentConfig{Type: graphql.String}}),
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					offset, err := cursorOffset(p.Args)
					if err != nil {
						return nil, err
					}
					items, err := s.tokens(p.Context, pageSize(p.Args), offset, optString(p.Args, "search"))
					if err != nil {
						return nil, err
					}
					return pageResult(items, offset, pageSize(p.Args)), nil
				},
			},
			"token": &graphql.Field{
				Type: tokenType,
				Args: graphql.FieldConfigArgument{"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)}},
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					address, _ := p.Args["address"].(string)
					items, err := s.tokens(p.Context, 1, 0, nil, address)
					if err != nil {
						return nil, err
					}
					if len(items) == 0 {
						return nil, nil
					}
					return items[0], nil
				},
			},
			"nfts": &graphql.Field{
				Type: nftPageType,
				Args: mergeArgs(pageArgs, graphql.FieldConfigArgument{
					"collection": &graphql.ArgumentConfig{Type: graphql.String},
					"owner":      &graphql.ArgumentConfig{Type: graphql.String},
				}),
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					offset, err := cursorOffset(p.Args)
					if err != nil {
						return nil, err
					}
					items, err := s.nfts(p.Context, pageSize(p.Args), offset, optString(p.Args, "collection"), optString(p.Args, "owner"))
					if err != nil {
						return nil, err
					}
					return pageResult(items, offset, pageSize(p.Args)), nil
				},
			},
			"account": &graphql.Field{
				Type: accountType,
				Args: graphql.FieldConfigArgument{"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)}},
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					address, _ := p.Args["address"].(string)
					return s.account(p.Context, address)
				},
			},
			"validators": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(validatorType))),
				Args: graphql.FieldConfigArgument{
					"first":  &graphql.ArgumentConfig{Type: graphql.Int, DefaultValue: defaultPageSize},
					"active": &graphql.ArgumentConfig{Type: graphql.Boolean},
				},
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					active, hasActive := p.Args["active"].(bool)
					return s.validators(p.Context, pageSize(p.Args), active, hasActive)
				},
			},
			"search": &graphql.Field{
				Type: searchResultType,
				Args: graphql.FieldConfigArgument{"query": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)}},
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					query, _ := p.Args["query"].(string)
					return s.search(p.Context, query)
				},
			},
			"chainStats": &graphql.Field{
				Type: chainStatsType,
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					return s.chainStats(p.Context)
				},
			},
		},
	})

	subscriptionType := graphql.NewObject(graphql.ObjectConfig{
		Name: "Subscription",
		Fields: graphql.Fields{
			"newBlocks": &graphql.Field{
				Type: graphql.NewNonNull(blockType),
				Args: graphql.FieldConfigArgument{"pollIntervalSeconds": &graphql.ArgumentConfig{Type: graphql.Int, DefaultValue: 2}},
				Subscribe: func(p graphql.ResolveParams) (interface{}, error) {
					interval, _ := p.Args["pollIntervalSeconds"].(int)
					if interval < 1 || interval > 30 {
						interval = 2
					}
					return s.hub.subscribeBlocks(p.Context, interval), nil
				},
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					block, ok := p.Source.(*Block)
					if !ok {
						return nil, nil
					}
					return block, nil
				},
			},
			"newTransactions": &graphql.Field{
				Type: graphql.NewNonNull(transactionType),
				Args: graphql.FieldConfigArgument{
					"pollIntervalSeconds": &graphql.ArgumentConfig{Type: graphql.Int, DefaultValue: 2},
					"address":             &graphql.ArgumentConfig{Type: graphql.String},
				},
				Subscribe: func(p graphql.ResolveParams) (interface{}, error) {
					interval, _ := p.Args["pollIntervalSeconds"].(int)
					if interval < 1 || interval > 30 {
						interval = 2
					}
					address, _ := p.Args["address"].(string)
					return s.hub.subscribeTransactions(p.Context, interval, address), nil
				},
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					tx, ok := p.Source.(*Transaction)
					if !ok {
						return nil, nil
					}
					return tx, nil
				},
			},
		},
	})

	return graphql.NewSchema(graphql.SchemaConfig{
		Query:        queryType,
		Subscription: subscriptionType,
	})
}

// -----------------------------------------------------------------------------
// GraphQL types returned to clients
// -----------------------------------------------------------------------------

// Block is a block header together with the hashes of its transactions.
type Block struct {
	Number       string   `json:"number"`
	Hash         string   `json:"hash"`
	ParentHash   string   `json:"parentHash"`
	Timestamp    string   `json:"timestamp"`
	TxCount      int      `json:"txCount"`
	UncleCount   int      `json:"uncleCount"`
	GasUsed      string   `json:"gasUsed"`
	GasLimit     string   `json:"gasLimit"`
	Miner        string   `json:"miner"`
	Size         string   `json:"size"`
	StateRoot    *string  `json:"stateRoot"`
	Transactions []string `json:"transactions"`
}

// Transaction is a confirmed or pending transaction.
type Transaction struct {
	Hash             string  `json:"hash"`
	Nonce            string  `json:"nonce"`
	BlockNumber      *string `json:"blockNumber"`
	BlockHash        *string `json:"blockHash"`
	TransactionIndex *int    `json:"transactionIndex"`
	From             string  `json:"from"`
	To               *string `json:"to"`
	Value            string  `json:"value"`
	GasPrice         string  `json:"gasPrice"`
	GasLimit         string  `json:"gasLimit"`
	GasUsed          *string `json:"gasUsed"`
	Input            *string `json:"input"`
	Status           string  `json:"status"`
	Timestamp        *string `json:"timestamp"`
	ContractAddress  *string `json:"contractAddress"`
}

// Token is an ERC-20 style token tracked by the indexer.
type Token struct {
	Address        string   `json:"address"`
	Name           string   `json:"name"`
	Symbol         string   `json:"symbol"`
	Decimals       int      `json:"decimals"`
	TotalSupply    *string  `json:"totalSupply"`
	HoldersCount   string   `json:"holdersCount"`
	TransfersCount string   `json:"transfersCount"`
	PriceUsd       *string  `json:"priceUsd"`
	MarketCap      *string  `json:"marketCap"`
	Volume24h      *string  `json:"volume24h"`
	IsVerified     bool     `json:"isVerified"`
	Holders        []string `json:"holders"`
}

// TokenHolder is a token balance for one address.
type TokenHolder struct {
	TokenAddress string  `json:"tokenAddress"`
	Address      string  `json:"address"`
	Balance      string  `json:"balance"`
	Percent      *string `json:"percent"`
}

// NFT is a non-fungible token instance.
type NFT struct {
	TokenAddress string  `json:"tokenAddress"`
	TokenID      string  `json:"tokenId"`
	Name         *string `json:"name"`
	Owner        string  `json:"owner"`
	URI          *string `json:"uri"`
	ImageURL     *string `json:"imageUrl"`
	Attributes   *string `json:"attributes"`
}

// Account is an address with its balance and token holdings.
type Account struct {
	Address       string        `json:"address"`
	Balance       string        `json:"balance"`
	Nonce         string        `json:"nonce"`
	CodeHash      *string       `json:"codeHash"`
	IsContract    bool          `json:"isContract"`
	IsVerified    bool          `json:"isVerified"`
	TokenHoldings []TokenHolder `json:"tokenHoldings"`
}

// Validator is a consensus participant.
type Validator struct {
	Address        string  `json:"address"`
	Name           *string `json:"name"`
	CommissionRate *string `json:"commissionRate"`
	TotalStake     *string `json:"totalStake"`
	Delegation     *string `json:"delegation"`
	Uptime         *string `json:"uptime"`
	BlocksCount    string  `json:"blocksCount"`
	MissesCount    string  `json:"missesCount"`
	IsActive       bool    `json:"isActive"`
	IsJailed       bool    `json:"isJailed"`
}

// SearchResult groups the entities that match a free-text query.
type SearchResult struct {
	Blocks       []*Block       `json:"blocks"`
	Transactions []*Transaction `json:"transactions"`
	Tokens       []*Token       `json:"tokens"`
	Accounts     []*Account     `json:"accounts"`
}

// ChainStats is the aggregate indexer summary.
type ChainStats struct {
	TotalBlocks       string  `json:"totalBlocks"`
	TotalTransactions string  `json:"totalTransactions"`
	TotalAddresses    string  `json:"totalAddresses"`
	LatestBlock       string  `json:"latestBlock"`
	AverageGasPrice   string  `json:"averageGasPrice"`
	TPS               float64 `json:"tps"`
	TokensTracked     string  `json:"tokensTracked"`
}

// PageInfo describes pagination state for a connection.
type PageInfo struct {
	EndCursor   *string `json:"endCursor"`
	HasNextPage bool    `json:"hasNextPage"`
}

type blockPage struct {
	Items    []*Block `json:"items"`
	PageInfo PageInfo `json:"pageInfo"`
}

type transactionPage struct {
	Items    []*Transaction `json:"items"`
	PageInfo PageInfo       `json:"pageInfo"`
}

type tokenPage struct {
	Items    []*Token `json:"items"`
	PageInfo PageInfo `json:"pageInfo"`
}

type nftPage struct {
	Items    []*NFT   `json:"items"`
	PageInfo PageInfo `json:"pageInfo"`
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

func mergeArgs(base, extra graphql.FieldConfigArgument) graphql.FieldConfigArgument {
	out := graphql.FieldConfigArgument{}
	for k, v := range base {
		out[k] = v
	}
	for k, v := range extra {
		out[k] = v
	}
	return out
}

func optString(args map[string]interface{}, key string) *string {
	value, ok := args[key].(string)
	if !ok || value == "" {
		return nil
	}
	return &value
}

func ptrString(value string) *string {
	if value == "" {
		return nil
	}
	return &value
}

func pageSize(args map[string]interface{}) int {
	size, ok := args["first"].(int)
	if !ok || size <= 0 {
		return defaultPageSize
	}
	if size > maxPageSize {
		return maxPageSize
	}
	return size
}

// cursorOffset decodes the opaque "after" cursor into a row offset.
func cursorOffset(args map[string]interface{}) (int, error) {
	after, ok := args["after"].(string)
	if !ok || after == "" {
		return 0, nil
	}

	raw, err := base64.RawURLEncoding.DecodeString(after)
	if err != nil {
		return 0, fmt.Errorf("invalid cursor")
	}

	offset, err := strconv.Atoi(string(raw))
	if err != nil || offset < 0 {
		return 0, fmt.Errorf("invalid cursor")
	}
	return offset, nil
}

func encodeCursor(offset int) *string {
	value := base64.RawURLEncoding.EncodeToString([]byte(strconv.Itoa(offset)))
	return &value
}

// pageResult wraps a page of rows and reports whether more rows remain.
func pageResult[T any](items []T, offset, limit int) map[string]interface{} {
	info := PageInfo{HasNextPage: len(items) == limit}
	if len(items) > 0 {
		info.EndCursor = encodeCursor(offset + len(items))
	}
	return map[string]interface{}{
		"items":    items,
		"pageInfo": info,
	}
}

func jsonOrNil(raw []byte) *string {
	if len(raw) == 0 {
		return nil
	}
	var probe interface{}
	if err := json.Unmarshal(raw, &probe); err != nil {
		return nil
	}
	text := string(raw)
	return &text
}
