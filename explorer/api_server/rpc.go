// TigerSmartChain API Server - JSON-RPC forwarding to the chain node.
//
// The explorer reads indexed history from PostgreSQL and live state from the
// node. This file implements the node side: a small, bounded JSON-RPC client
// used by the trace, contract-call and node-info handlers.

package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"sync/atomic"
	"time"
)

// rpcResponse is a single JSON-RPC 2.0 response envelope.
type rpcResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      uint64          `json:"id"`
	Result  json.RawMessage `json:"result"`
	Error   *rpcError       `json:"error"`
}

type rpcError struct {
	Code    int             `json:"code"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data,omitempty"`
}

func (e *rpcError) Error() string {
	return fmt.Sprintf("node returned error %d: %s", e.Code, e.Message)
}

// rpcCall issues a JSON-RPC request against the configured node and returns the
// decoded result. A nil result is returned when the node answers with null.
func (h *Handler) rpcCall(ctx context.Context, method string, params ...interface{}) (interface{}, error) {
	if h.nodeRPC == nil || strings.TrimSpace(h.nodeRPCURL) == "" {
		return nil, fmt.Errorf("node RPC endpoint is not configured")
	}
	if params == nil {
		params = []interface{}{}
	}

	id := atomic.AddUint64(&h.rpcID, 1)
	body, err := json.Marshal(map[string]interface{}{
		"jsonrpc": "2.0",
		"id":      id,
		"method":  method,
		"params":  params,
	})
	if err != nil {
		return nil, fmt.Errorf("encode %s request: %w", method, err)
	}

	reqCtx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()

	req, err := http.NewRequestWithContext(reqCtx, http.MethodPost, h.nodeRPCURL, bytes.NewReader(body))
	if err != nil {
		return nil, fmt.Errorf("build %s request: %w", method, err)
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := h.nodeRPC.Do(req)
	if err != nil {
		return nil, fmt.Errorf("%s request failed: %w", method, err)
	}
	defer resp.Body.Close()

	raw, err := io.ReadAll(io.LimitReader(resp.Body, 64<<20))
	if err != nil {
		return nil, fmt.Errorf("read %s response: %w", method, err)
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("%s returned HTTP %d", method, resp.StatusCode)
	}

	var out rpcResponse
	if err := json.Unmarshal(raw, &out); err != nil {
		return nil, fmt.Errorf("decode %s response: %w", method, err)
	}
	if out.Error != nil {
		return nil, out.Error
	}
	if len(out.Result) == 0 {
		return nil, nil
	}

	var result interface{}
	if err := json.Unmarshal(out.Result, &result); err != nil {
		return nil, fmt.Errorf("decode %s result: %w", method, err)
	}
	return result, nil
}
