package graphql

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/graphql-go/graphql"
	"github.com/graphql-go/graphql/language/ast"
	"github.com/graphql-go/graphql/language/parser"
	"github.com/graphql-go/graphql/language/source"
	"github.com/graphql-go/handler"
	"golang.org/x/time/rate"
)

// ---------------------------------------------------------------------------
// query type
// ---------------------------------------------------------------------------

func (s *Server) queryType() *graphql.Object {
	return graphql.NewObject(graphql.ObjectConfig{
		Name: "Query",
		Fields: graphql.Fields{
			"blocks": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(blockType))),
				Args: graphql.FieldConfigArgument{
					"limit":     limitArg(),
					"offset":    offsetArg(),
					"fromBlock": intArgCfg("lowest block number to include"),
					"toBlock":   intArgCfg("highest block number to include"),
					"miner":     strArgCfg("exact miner address"),
				},
				Resolve: s.resolveBlocks,
			},
			"block": &graphql.Field{
				Type: blockType,
				Args: graphql.FieldConfigArgument{
					"numberOrHash": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveBlock,
			},
			"latestBlock": &graphql.Field{Type: blockType, Resolve: s.resolveLatestBlock},

			"transactions": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(transactionType))),
				Args: graphql.FieldConfigArgument{
					"limit":   limitArg(),
					"offset":  offsetArg(),
					"from":    strArgCfg("sender address"),
					"to":      strArgCfg("recipient address"),
					"address": strArgCfg("address appearing as sender or recipient"),
					"status":  strArgCfg("transaction status"),
				},
				Resolve: s.resolveTransactions,
			},
			"transaction": &graphql.Field{
				Type: transactionType,
				Args: graphql.FieldConfigArgument{
					"hash": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveTransaction,
			},
			"transactionReceipt": &graphql.Field{
				Type: receiptType,
				Args: graphql.FieldConfigArgument{
					"hash": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveTransactionReceipt,
			},
			"internalTransactions": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(internalTxType))),
				Args: graphql.FieldConfigArgument{
					"txHash": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"limit":  limitArg(),
					"offset": offsetArg(),
				},
				Resolve: s.resolveInternalTransactions,
			},

			"tokens": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(tokenType))),
				Args: graphql.FieldConfigArgument{
					"limit":    limitArg(),
					"offset":   offsetArg(),
					"search":   strArgCfg("name, symbol or address fragment"),
					"verified": boolArgCfg("restrict to verified tokens"),
				},
				Resolve: s.resolveTokens,
			},
			"token": &graphql.Field{
				Type: tokenType,
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveToken,
			},
			"tokenHolders": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(tokenHolderType))),
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"limit":   limitArg(),
					"offset":  offsetArg(),
				},
				Resolve: s.resolveTokenHolders,
			},
			"tokenTransfers": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(tokenTransferType))),
				Args: graphql.FieldConfigArgument{
					"address":   &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"limit":     limitArg(),
					"offset":    offsetArg(),
					"fromBlock": intArgCfg("lowest block number to include"),
				},
				Resolve: s.resolveTokenTransfers,
			},
			"tokenApprovals": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(tokenApprovalType))),
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"owner":   strArgCfg("filter by owner"),
					"spender": strArgCfg("filter by spender"),
					"limit":   limitArg(),
					"offset":  offsetArg(),
				},
				Resolve: s.resolveTokenApprovals,
			},
			"tokenPriceHistory": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(pricePointType))),
				Args: graphql.FieldConfigArgument{
					"address":  &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"from":     intArgCfg("start of the window, unix seconds"),
					"to":       intArgCfg("end of the window, unix seconds"),
					"interval": &graphql.ArgumentConfig{Type: intervalEnum},
				},
				Resolve: s.resolveTokenPriceHistory,
			},

			"nftCollections": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(nftCollectionType))),
				Args: graphql.FieldConfigArgument{
					"limit":  limitArg(),
					"offset": offsetArg(),
					"search": strArgCfg("name or address fragment"),
				},
				Resolve: s.resolveNFTCollections,
			},
			"nftCollection": &graphql.Field{
				Type: nftCollectionType,
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveNFTCollection,
			},
			"nft": &graphql.Field{
				Type: nftType,
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"tokenId": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveNFT,
			},
			"nftOwners": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(nftOwnerType))),
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"limit":   limitArg(),
					"offset":  offsetArg(),
				},
				Resolve: s.resolveNFTOwners,
			},
			"nftTransfers": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(nftTransferType))),
				Args: graphql.FieldConfigArgument{
					"address":   &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"limit":     limitArg(),
					"offset":    offsetArg(),
					"fromBlock": intArgCfg("lowest block number to include"),
				},
				Resolve: s.resolveNFTTransfers,
			},
			"nftRarity": &graphql.Field{
				Type: nftRarityType,
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"tokenId": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveNFTRarity,
			},
			"nftActivity": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(nftActivityType))),
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"limit":   limitArg(),
				},
				Resolve: s.resolveNFTActivity,
			},

			"account": &graphql.Field{
				Type: accountType,
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveAccount,
			},
			"accountTransactions": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(transactionType))),
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"limit":   limitArg(),
					"offset":  offsetArg(),
				},
				Resolve: s.resolveAccountTransactions,
			},
			"accountTokens": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(accountTokenType))),
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"limit":   limitArg(),
					"offset":  offsetArg(),
				},
				Resolve: s.resolveAccountTokens,
			},
			"accountNFTs": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(accountNFTType))),
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"limit":   limitArg(),
					"offset":  offsetArg(),
				},
				Resolve: s.resolveAccountNFTs,
			},

			"contract": &graphql.Field{
				Type: contractType,
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveContract,
			},
			"contractSource": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(contractSourceType))),
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveContractSource,
			},
			"contractABI": &graphql.Field{
				Type: graphql.String,
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveContractABI,
			},
			"verifiedContracts": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(verifiedContractType))),
				Args: graphql.FieldConfigArgument{
					"limit":   limitArg(),
					"offset":  offsetArg(),
					"search":  strArgCfg("contract name or address fragment"),
					"license": strArgCfg("SPDX license identifier"),
				},
				Resolve: s.resolveVerifiedContracts,
			},

			"validators": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(validatorType))),
				Args: graphql.FieldConfigArgument{
					"limit":   limitArg(),
					"offset":  offsetArg(),
					"orderBy": &graphql.ArgumentConfig{Type: validatorOrderEnum},
				},
				Resolve: s.resolveValidators,
			},
			"validator": &graphql.Field{
				Type: validatorType,
				Args: graphql.FieldConfigArgument{
					"address": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveValidator,
			},
			"validatorRewards": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(validatorRewardType))),
				Args: graphql.FieldConfigArgument{
					"address":   &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
					"fromBlock": intArgCfg("lowest block number to include"),
					"toBlock":   intArgCfg("highest block number to include"),
				},
				Resolve: s.resolveValidatorRewards,
			},

			"search": &graphql.Field{
				Type: searchResultType,
				Args: graphql.FieldConfigArgument{
					"query": &graphql.ArgumentConfig{Type: graphql.NewNonNull(graphql.String)},
				},
				Resolve: s.resolveSearch,
			},
			"stats":    &graphql.Field{Type: networkStatsType, Resolve: s.resolveStats},
			"gasPrice": &graphql.Field{Type: gasPriceType, Resolve: s.resolveGasPrice},
			"gasHistory": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(gasPricePointType))),
				Args: graphql.FieldConfigArgument{
					"from":     intArgCfg("start of the window, unix seconds"),
					"to":       intArgCfg("end of the window, unix seconds"),
					"interval": &graphql.ArgumentConfig{Type: intervalEnum},
				},
				Resolve: s.resolveGasHistory,
			},
			"pendingTransactions": &graphql.Field{
				Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(pendingTransactionType))),
				Args: graphql.FieldConfigArgument{
					"limit": limitArg(),
					"from":  strArgCfg("filter by sender"),
					"to":    strArgCfg("filter by recipient"),
				},
				Resolve: s.resolvePendingTransactions,
			},
		},
	})
}

// ---------------------------------------------------------------------------
// mutation type
// ---------------------------------------------------------------------------

func (s *Server) mutationType() *graphql.Object {
	return graphql.NewObject(graphql.ObjectConfig{
		Name: "Mutation",
		Fields: graphql.Fields{
			"verifyContract": &graphql.Field{
				Type: graphql.NewNonNull(verifyContractResultType),
				Args: graphql.FieldConfigArgument{
					"input": &graphql.ArgumentConfig{Type: graphql.NewNonNull(verifyContractInputType)},
				},
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					input, ok := p.Args["input"].(map[string]interface{})
					if !ok {
						return nil, fmt.Errorf("input is required")
					}
					return s.resolveVerifyContract(p.Context, input)
				},
			},
			"verifyProxy": &graphql.Field{
				Type: graphql.NewNonNull(verifyProxyResultType),
				Args: graphql.FieldConfigArgument{
					"input": &graphql.ArgumentConfig{Type: graphql.NewNonNull(verifyProxyInputType)},
				},
				Resolve: func(p graphql.ResolveParams) (interface{}, error) {
					input, ok := p.Args["input"].(map[string]interface{})
					if !ok {
						return nil, fmt.Errorf("input is required")
					}
					return s.resolveVerifyProxy(p.Context, input)
				},
			},
		},
	})
}

// ---------------------------------------------------------------------------
// server lifecycle
// ---------------------------------------------------------------------------

// NewServer builds the executable schema and returns a ready-to-start server.
func NewServer(config *Config) (*Server, error) {
	if config == nil {
		config = &Config{}
	}
	if config.Port == 0 {
		config.Port = 8080
	}
	if config.ReadTimeout <= 0 {
		config.ReadTimeout = 15 * time.Second
	}
	if config.WriteTimeout <= 0 {
		config.WriteTimeout = 60 * time.Second
	}
	if config.RateLimit <= 0 {
		config.RateLimit = 100
	}
	if config.RateBurst <= 0 {
		config.RateBurst = 200
	}

	s := &Server{
		config:  config,
		metrics: &Metrics{},
		rateLimit: &RateLimiter{
			store:  make(map[string]*ClientLimiter),
			limit:  rate.Limit(config.RateLimit),
			burst:  config.RateBurst,
			global: rate.NewLimiter(rate.Limit(config.RateLimit*100), config.RateBurst*100),
		},
	}

	schema, err := graphql.NewSchema(graphql.SchemaConfig{
		Query:    s.queryType(),
		Mutation: s.mutationType(),
	})
	if err != nil {
		return nil, fmt.Errorf("build graphql schema: %w", err)
	}
	s.schema = schema

	s.gqlHandler = handler.New(&handler.Config{
		Schema:   &s.schema,
		Pretty:   false,
		GraphiQL: false,
	})

	return s, nil
}

// Start serves GraphQL over HTTP until Stop is called.
func (s *Server) Start() error {
	mux := http.NewServeMux()
	mux.Handle("/graphql", s.ginMiddlewareHTTP(s.gqlHandler))
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		status := "ok"
		code := http.StatusOK
		if s.config.DB != nil {
			if err := s.config.DB.PingContext(r.Context()); err != nil {
				status = "database unavailable"
				code = http.StatusServiceUnavailable
			}
		} else {
			status = "database not configured"
			code = http.StatusServiceUnavailable
		}
		w.WriteHeader(code)
		_ = json.NewEncoder(w).Encode(map[string]string{"status": status})
	})

	s.server = &http.Server{
		Addr:         fmt.Sprintf(":%d", s.config.Port),
		Handler:      mux,
		ReadTimeout:  s.config.ReadTimeout,
		WriteTimeout: s.config.WriteTimeout,
	}

	if err := s.server.ListenAndServe(); err != nil && err != http.ErrServerClosed {
		return err
	}
	return nil
}

// Stop shuts the HTTP server down gracefully.
func (s *Server) Stop(ctx context.Context) error {
	if s.server == nil {
		return nil
	}
	return s.server.Shutdown(ctx)
}

// Metrics returns a snapshot of the server counters.
func (s *Server) Metrics() map[string]interface{} {
	s.metrics.mu.RLock()
	defer s.metrics.mu.RUnlock()

	avg := float64(0)
	if s.metrics.LatencyCount > 0 {
		avg = float64(s.metrics.LatencySum.Microseconds()) / float64(s.metrics.LatencyCount)
	}

	return map[string]interface{}{
		"queries":        s.metrics.Queries,
		"mutations":      s.metrics.Mutations,
		"errors":         s.metrics.Errors,
		"cache_hits":     s.metrics.CacheHits,
		"cache_misses":   s.metrics.CacheMisses,
		"avg_latency_us": avg,
	}
}

// ---------------------------------------------------------------------------
// rate limiting
// ---------------------------------------------------------------------------

// allow reports whether the client identified by key may issue another request.
func (rl *RateLimiter) allow(key string) bool {
	if !rl.global.Allow() {
		return false
	}

	rl.mu.Lock()
	defer rl.mu.Unlock()

	now := time.Now()
	if len(rl.store) > maxRateClients {
		for k, v := range rl.store {
			if now.Sub(v.lastSeen) > rateLimiterTTL {
				delete(rl.store, k)
			}
		}
	}

	c, ok := rl.store[key]
	if !ok {
		c = &ClientLimiter{limiter: rate.NewLimiter(rl.limit, rl.burst)}
		rl.store[key] = c
	}
	c.lastSeen = now
	c.requests++

	return c.limiter.Allow()
}

func clientIP(r *http.Request) string {
	if forwarded := r.Header.Get("X-Forwarded-For"); forwarded != "" {
		first := strings.TrimSpace(strings.Split(forwarded, ",")[0])
		if first != "" {
			return first
		}
	}
	if realIP := strings.TrimSpace(r.Header.Get("X-Real-IP")); realIP != "" {
		return realIP
	}
	host, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		return r.RemoteAddr
	}
	return host
}

// ---------------------------------------------------------------------------
// HTTP middleware
// ---------------------------------------------------------------------------

// originAllowed reports whether the request Origin may read the response.
func (s *Server) originAllowed(r *http.Request) bool {
	origin := strings.TrimSpace(r.Header.Get("Origin"))
	if origin == "" {
		return true
	}

	u, err := url.Parse(origin)
	if err != nil {
		return false
	}

	// Same-host requests are always allowed.
	host := r.Host
	if h, _, err := net.SplitHostPort(r.Host); err == nil {
		host = h
	}
	if strings.EqualFold(u.Hostname(), host) {
		return true
	}

	for _, allowed := range s.config.AllowedOrigins {
		allowed = strings.TrimSpace(allowed)
		if allowed == "*" {
			return true
		}
		if strings.EqualFold(allowed, origin) || strings.EqualFold(allowed, u.Hostname()) {
			return true
		}
	}
	return false
}

// walkSelections reports whether any selection in the document is an
// introspection field.
func walkSelections(defs []ast.Node, found *bool) {
	for _, def := range defs {
		if *found {
			return
		}
		op, ok := def.(*ast.OperationDefinition)
		if !ok || op.SelectionSet == nil {
			continue
		}
		walkSelectionSet(op.SelectionSet, found)
	}
}

func walkSelectionSet(ss *ast.SelectionSet, found *bool) {
	if ss == nil || *found {
		return
	}
	for _, sel := range ss.Selections {
		if *found {
			return
		}
		f, ok := sel.(*ast.Field)
		if !ok || f.Name == nil {
			continue
		}
		if strings.HasPrefix(f.Name.Value, "__") {
			*found = true
			return
		}
		walkSelectionSet(f.SelectionSet, found)
	}
}

// selectionSetHasIntrospection reports whether a selection set contains an
// introspection field at any depth.
func selectionSetHasIntrospection(ss *ast.SelectionSet) bool {
	if ss == nil {
		return false
	}
	for _, sel := range ss.Selections {
		f, ok := sel.(*ast.Field)
		if !ok || f.Name == nil {
			continue
		}
		if strings.HasPrefix(f.Name.Value, "__") {
			return true
		}
		if selectionSetHasIntrospection(f.SelectionSet) {
			return true
		}
	}
	return false
}

// containsIntrospection reports whether the query document asks for schema
// introspection, which is refused unless explicitly enabled.
func containsIntrospection(query string) bool {
	if !strings.Contains(query, "__") {
		return false
	}
	doc, err := parser.Parse(parser.ParseParams{Source: source.NewSource(&source.Source{Body: []byte(query)})})
	if err != nil {
		// An unparsable document is rejected by the executor; treat it as
		// non-introspection here so the executor can report the syntax error.
		return false
	}

	var found bool
	for _, def := range doc.Definitions {
		op, ok := def.(*ast.OperationDefinition)
		if !ok || op.SelectionSet == nil {
			continue
		}
		if selectionSetHasIntrospection(op.SelectionSet) {
			found = true
			break
		}
	}
	return found
}

// serveHTTP is the shared request path for both the Gin middleware and the
// standalone HTTP server.
func (s *Server) serveHTTP(w http.ResponseWriter, r *http.Request) {
	start := time.Now()

	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Header().Set("X-Frame-Options", "DENY")
	w.Header().Set("Referrer-Policy", "no-referrer")

	if !s.originAllowed(r) {
		http.Error(w, `{"errors":[{"message":"origin not allowed"}]}`, http.StatusForbidden)
		return
	}
	if origin := r.Header.Get("Origin"); origin != "" {
		w.Header().Set("Access-Control-Allow-Origin", origin)
		w.Header().Set("Vary", "Origin")
		w.Header().Set("Access-Control-Allow-Methods", "POST, GET, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization")
	}
	if r.Method == http.MethodOptions {
		w.WriteHeader(http.StatusNoContent)
		return
	}
	if !s.rateLimit.allow(clientIP(r)) {
		w.Header().Set("Retry-After", "1")
		http.Error(w, `{"errors":[{"message":"rate limit exceeded"}]}`, http.StatusTooManyRequests)
		return
	}

	// The GraphQL document travels in the body for POST and in the query
	// string for GET. Read it once, then hand a replayable body to the
	// executor.
	var body []byte
	if r.Body != nil {
		body, _ = io.ReadAll(io.LimitReader(r.Body, 4<<20))
		r.Body.Close()
	}
	r.Body = io.NopCloser(bytes.NewReader(body))

	query := r.URL.Query().Get("query")
	if query == "" && len(body) > 0 {
		var payload struct {
			Query string `json:"query"`
		}
		if err := json.Unmarshal(body, &payload); err == nil {
			query = payload.Query
		}
	}

	if !s.config.EnableIntrospection && containsIntrospection(query) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusForbidden)
		_, _ = w.Write([]byte(`{"errors":[{"message":"introspection is disabled"}]}`))
		return
	}

	// The *Server is carried in the request context so nested field resolvers
	// can reach the database pool.
	ctx := withServer(r.Context(), s)
	s.gqlHandler.ContextHandler(ctx, w, r)

	elapsed := time.Since(start)
	isMutation := strings.HasPrefix(strings.TrimSpace(query), "mutation")

	s.metrics.mu.Lock()
	if isMutation {
		s.metrics.Mutations++
	} else {
		s.metrics.Queries++
	}
	s.metrics.LatencySum += elapsed
	s.metrics.LatencyCount++
	s.metrics.mu.Unlock()
}

func (s *Server) ginMiddlewareHTTP(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		s.serveHTTP(w, r)
	})
}

// GinMiddleware exposes the same request path to a Gin router.
func (s *Server) GinMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		s.serveHTTP(c.Writer, c.Request)
		c.Abort()
	}
}
