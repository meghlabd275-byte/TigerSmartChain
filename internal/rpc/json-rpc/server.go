// Package jsonrpc provides JSON-RPC server for TigerSmartChain.
package jsonrpc

import (
	"encoding/json"
	"net/http"
)

// Server represents the JSON-RPC server.
type Server struct {
	// handlers maps method names to handlers
	handlers map[string]Handler

	// Backend for blockchain data
	backend *Backend
}

// Handler is a JSON-RPC method handler.
type Handler func(params json.RawMessage) (json.RawMessage, error)

// NewServer creates a new JSON-RPC server.
func NewServer() *Server {
	s := &Server{
		handlers: make(map[string]Handler),
	}
	s.registerHandlers()
	return s
}

// registerHandlers registers all JSON-RPC handlers.
func (s *Server) registerHandlers() {
	// Block methods
	s.handlers["eth_blockNumber"] = s.blockNumber
	s.handlers["eth_getBlockByNumber"] = s.getBlockByNumber
	s.handlers["eth_getBlockByHash"] = s.getBlockByHash
	s.handlers["eth_getBlockReceipts"] = s.getBlockReceipts
	s.handlers["eth_getUncleByBlockNumberAndIndex"] = s.getUncleByBlockNumberAndIndex
	s.handlers["eth_getUncleByBlockHashAndIndex"] = s.getUncleByBlockHashAndIndex

	// Transaction methods
	s.handlers["eth_getTransactionByHash"] = s.getTransactionByHash
	s.handlers["eth_getTransactionReceipt"] = s.getTransactionReceipt
	s.handlers["eth_getTransactionCount"] = s.getTransactionCount
	s.handlers["eth_sendRawTransaction"] = s.sendRawTransaction

	// State methods
	s.handlers["eth_getBalance"] = s.getBalance
	s.handlers["eth_getCode"] = s.getCode
	s.handlers["eth_getStorageAt"] = s.getStorageAt

	// Contract execution
	s.handlers["eth_call"] = s.call
	s.handlers["eth_estimateGas"] = s.estimateGas

	// Filter methods
	s.handlers["eth_newBlockFilter"] = s.newBlockFilter
	s.handlers["eth_newPendingTransactionFilter"] = s.newPendingTransactionFilter
	s.handlers["eth_newLogFilter"] = s.newLogFilter
	s.handlers["eth_getFilterChanges"] = s.getFilterChanges
	s.handlers["eth_getFilterLogs"] = s.getFilterLogs
	s.handlers["eth_uninstallFilter"] = s.uninstallFilter

	// Network methods
	s.handlers["net_version"] = s.netVersion
	s.handlers["net_listening"] = s.netListening
	s.handlers["net_peerCount"] = s.netPeerCount

	// Client methods
	s.handlers["web3_clientVersion"] = s.web3ClientVersion
	s.handlers["web3_sha3"] = s.web3Sha3

	// Sync methods
	s.handlers["eth_syncing"] = s.syncing

	// Gas price
	s.handlers["eth_gasPrice"] = s.gasPrice

	// Account methods
	s.handlers["eth_accounts"] = s.accounts
}

// SetBackend sets the backend for the server.
func (s *Server) SetBackend(backend *Backend) {
	s.backend = backend
}

// ServeHTTP serves the JSON-RPC request.
func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if r.Method != "POST" {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req Request
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		respondError(w, -32700, "Parse error")
		return
	}

	handler, ok := s.handlers[req.Method]
	if !ok {
		respondError(w, -32601, "Method not found")
		return
	}

	result, err := handler(req.Params)
	if err != nil {
		respondError(w, -32602, err.Error())
		return
	}

	respond(w, result)
}

// Request represents a JSON-RPC request.
type Request struct {
	JSONRPC string          `json:"jsonrpc"`
	Method  string        `json:"method"`
	Params  json.RawMessage `json:"params"`
	ID     interface{}   `json:"id"`
}

// Response represents a JSON-RPC response.
type Response struct {
	JSONRPC string      `json:"jsonrpc"`
	Result  interface{} `json:"result,omitempty"`
	Error   *Error      `json:"error,omitempty"`
	ID     interface{} `json:"id"`
}

// Error represents a JSON-RPC error.
type Error struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

func respond(w http.ResponseWriter, result json.RawMessage) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(Response{
		JSONRPC: "2.0",
		Result:  result,
	})
}

func respondError(w http.ResponseWriter, code int, message string) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(Response{
		JSONRPC: "2.0",
		Error:   &Error{Code: code, Message: message},
	})
}

// Handlers
func (s *Server) blockNumber(params json.RawMessage) (json.RawMessage, error) {
	return []byte(`"0x0"`), nil
}

func (s *Server) getBlockByNumber(params json.RawMessage) (json.RawMessage, error) {
	return []byte("null"), nil
}

func (s *Server) getBlockByHash(params json.RawMessage) (json.RawMessage, error) {
	return []byte("null"), nil
}

func (s *Server) getTransactionByHash(params json.RawMessage) (json.RawMessage, error) {
	return []byte("null"), nil
}

func (s *Server) getTransactionReceipt(params json.RawMessage) (json.RawMessage, error) {
	return []byte("null"), nil
}

func (s *Server) call(params json.RawMessage) (json.RawMessage, error) {
	return []byte(`"0x"`), nil
}

func (s *Server) sendRawTransaction(params json.RawMessage) (json.RawMessage, error) {
	return []byte(`"0x"`), nil
}

func (s *Server) estimateGas(params json.RawMessage) (json.RawMessage, error) {
	return []byte(`"0x5208"`), nil
}

func (s *Server) getBalance(params json.RawMessage) (json.RawMessage, error) {
	return []byte(`"0x0"`), nil
}

func (s *Server) getCode(params json.RawMessage) (json.RawMessage, error) {
	return []byte(`"0x"`), nil
}

func (s *Server) getStorageAt(params json.RawMessage) (json.RawMessage, error) {
	return []byte(`"0x"`), nil
}

func (s *Server) getTransactionCount(params json.RawMessage) (json.RawMessage, error) {
	return []byte(`"0x0"`), nil
}

func (s *Server) netVersion(params json.RawMessage) (json.RawMessage, error) {
	return []byte(`"9001"`), nil
}

func (s *Server) netListening(params json.RawMessage) (json.RawMessage, error) {
	return []byte("true"), nil
}

func (s *Server) web3ClientVersion(params json.RawMessage) (json.RawMessage, error) {
	return []byte(`"TigerSmartChain/v1.0.0/go1.21.5"`), nil
}

func (s *Server) web3Sha3(params json.RawMessage) (json.RawMessage, error) {
	return []byte(`"0x"`), nil
}

// Start starts the JSON-RPC server.
func Start(addr string) error {
	s := NewServer()
	return http.ListenAndServe(addr, s)
}