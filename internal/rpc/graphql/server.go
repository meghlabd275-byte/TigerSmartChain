// Package graphql provides GraphQL API server for TigerSmartChain.
// This implements a GraphQL interface for querying blockchain data.

package graphql

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
	"github.com/ethereum/go-ethereum/rpc"
)

// Server represents GraphQL server
type Server struct {
	httpServer *http.Server
	rpcClient *rpc.Client
}

// Config holds GraphQL server configuration
type Config struct {
	Address      string
	Port         int
	EnablePlayground bool
	EnableCors   bool
}

// NewServer creates new GraphQL server
func NewServer(cfg *Config, rpcEndpoint string) (*Server, error) {
	rpcClient, err := rpc.Dial(rpcEndpoint)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to RPC: %w", err)
	}

	srv := &Server{
		rpcClient: rpcClient,
	}

	addr := fmt.Sprintf("%s:%d", cfg.Address, cfg.Port)
	mux := http.NewServeMux()

	// GraphQL endpoint
	mux.HandleFunc("/graphql", srv.handleGraphQL)

	// GraphQL playground (optional)
	if cfg.EnablePlayground {
		mux.HandleFunc("/", srv.handlePlayground)
	}

	srv.httpServer = &http.Server{
		Addr:         addr,
		Handler:      corsMiddleware(mux, cfg.EnableCors),
		ReadTimeout:  10 * time.Second,
		WriteTimeout: 30 * time.Second,
	}

	return srv, nil
}

// Start starts the GraphQL server
func (s *Server) Start() error {
	return s.httpServer.ListenAndServe()
}

// Stop stops the GraphQL server
func (s *Server) Stop(ctx context.Context) error {
	return s.httpServer.Shutdown(ctx)
}

// handleGraphQL handles GraphQL queries
func (s *Server) handleGraphQL(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req struct {
		Query     string                 `json:"query"`
		Operation string                 `json:"operationName"`
		Variables map[string]interface{} `json:"variables"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "Invalid request", http.StatusBadRequest)
		return
	}

	// Execute query
	result, err := s.executeQuery(r.Context(), req.Query, req.Variables)
	if err != nil {
		w.WriteHeader(http.StatusOK)
		json.NewEncoder(w).Encode(map[string]interface{}{
			"errors": []map[string]string{
				{"message": err.Error()},
			},
		})
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"data": result,
	})
}

// handlePlayground serves GraphQL playground
func (s *Server) handlePlayground(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "text/html")
	w.Write([]byte(playgroundHTML))
}

// executeQuery executes a GraphQL query
func (s *Server) executeQuery(ctx context.Context, query string, vars map[string]interface{}) (interface{}, error) {
	// Parse and execute based on query type
	switch {
	case contains(query, "block"):
		return s.getBlock(ctx, vars)
	case contains(query, "transaction"):
		return s.getTransaction(ctx, vars)
	case contains(query, "address"):
		return s.getAddress(ctx, vars)
	case contains(query, "token"):
		return s.getToken(ctx, vars)
	default:
		return s.getBlock(ctx, vars)
	}
}

// getBlock returns block data
func (s *Server) getBlock(ctx context.Context, vars map[string]interface{}) (interface{}, error) {
	var blockNumber string
	if num, ok := vars["number"].(float64); ok {
		blockNumber = hexutil.EncodeUint64(uint64(num))
	} else if num, ok := vars["number"].(string); ok {
		blockNumber = num
	} else {
		blockNumber = "latest"
	}

	var result map[string]interface{}
	err := s.rpcClient.CallContext(ctx, &result, "eth_getBlockByNumber", blockNumber, true)
	return result, err
}

// getTransaction returns transaction data  
func (s *Server) getTransaction(ctx context.Context, vars map[string]interface{}) (interface{}, error) {
	hash, ok := vars["hash"].(string)
	if !ok {
		return nil, fmt.Errorf("missing hash parameter")
	}

	var result map[string]interface{}
	err := s.rpcClient.CallContext(ctx, &result, "eth_getTransactionByHash", common.HexToHash(hash))
	return result, err
}

// getAddress returns address data (balance, code, etc)
func (s *Server) getAddress(ctx context.Context, vars map[string]interface{}) (interface{}, error) {
	addr, ok := vars["address"].(string)
	if !ok {
		return nil, fmt.Errorf("missing address parameter")
	}

	var balance string
	err := s.rpcClient.CallContext(ctx, &balance, "eth_getBalance", addr, "latest")
	if err != nil {
		return nil, err
	}

	var code string
	s.rpcClient.CallContext(ctx, &code, "eth_getCode", addr, "latest")

	return map[string]interface{}{
		"address": addr,
		"balance": balance,
		"code":    code,
	}, nil
}

// getToken returns token data
func (s *Server) getToken(ctx context.Context, vars map[string]interface{}) (interface{}, error) {
	addr, ok := vars["address"].(string)
	if !ok {
		return nil, fmt.Errorf("missing address parameter")
	}

	// Get token name
	var name string
	s.rpcClient.CallContext(ctx, &name, "eth_call", map[string]interface{}{
		"to":   addr,
		"data": "0x06fdde3b", // name()
	}, "latest")

	var symbol string
	s.rpcClient.CallContext(ctx, &symbol, "eth_call", map[string]interface{}{
		"to":   addr,
		"data": "0x95d89b41", // symbol()
	}, "latest")

	var totalSupply string
	s.rpcClient.CallContext(ctx, &totalSupply, "eth_call", map[string]interface{}{
		"to":   addr,
		"data": "0x18160ddd", // totalSupply()
	}, "latest")

	return map[string]interface{}{
		"address":     addr,
		"name":        name,
		"symbol":     symbol,
		"totalSupply": totalSupply,
	}, nil
}

// corsMiddleware adds CORS headers
func corsMiddleware(next http.Handler, enable bool) http.Handler {
	if !enable {
		return next
	}
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Access-Control-Allow-Methods", "POST, GET, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "Content-Type")
		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusOK)
			return
		}
		next.ServeHTTP(w, r)
	})
}

// contains checks if string contains substring
func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(s) > 0 && containsAt(s, substr))
}

func containsAt(s, substr string) bool {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}

const playgroundHTML = `<!DOCTYPE html>
<html>
<head>
  <title>TigerSmartChain GraphQL</title>
  <link rel="stylesheet" href="https://unpkg.com/graphiql/graphiql.min.css" />
</head>
<body style="margin: 0;">
  <div id="graphiql" style="height: 100vh;"></div>
  <script crossorigin src="https://unpkg.com/react/umd/react.production.min.js"></script>
  <script crossorigin src="https://unpkg.com/react-dom/umd/react-dom.production.min.js"></script>
  <script crossorigin src="https://unpkg.com/graphiql/graphiql.min.js"></script>
  <script>
    GraphiQL.configure({ 
      fetcher: GraphiQL.createFetcher({ 
        url: '/graphql',
      }),
      defaultQuery: `{
  block(number: "latest") {
    number
    hash
    transactions {
      hash
      from
      to
      value
    }
  }
}`
    });
    ReactDOM.render(React.createElement(GraphiQL), document.getElementById('graphiql'));
  </script>
</body>
</html>`

var _ = json.Marshal    // Use JSON
var _ = hexutil.Encode // Use hexutil