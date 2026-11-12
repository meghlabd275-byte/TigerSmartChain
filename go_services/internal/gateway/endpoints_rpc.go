package gateway

import (
	"context"
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
)

// rpcPassthroughTimeout bounds one forwarded call. A trace of a large block can
// take longer than a normal read, so it is wider than the gateway default.
const rpcPassthroughTimeout = 30 * time.Second

// rpcPassthroughMethods are the JSON-RPC methods the gateway will forward on
// behalf of a browser. Only read-only methods are listed.
//
// The gateway holds an authenticated connection to the node, so forwarding an
// arbitrary method would let any page visitor reach eth_sendRawTransaction,
// admin_*, or personal_* through it and bypass whatever policy sits in front of
// the node. The allow-list is the boundary.
var rpcPassthroughMethods = map[string]bool{
	// Chain state
	"eth_chainId":                             true,
	"eth_blockNumber":                         true,
	"eth_gasPrice":                            true,
	"eth_maxPriorityFeePerGas":                true,
	"eth_feeHistory":                          true,
	"eth_syncing":                             true,
	"eth_protocolVersion":                     true,
	"web3_clientVersion":                      true,
	"net_version":                             true,
	"eth_accounts":                            true,
	"eth_coinbase":                            true,
	"eth_mining":                              true,
	"eth_hashrate":                            true,
	"eth_getBalance":                          true,
	"eth_getTransactionCount":                 true,
	"eth_getCode":                             true,
	"eth_getStorageAt":                        true,
	"eth_getProof":                            true,
	"eth_getBlockByNumber":                    true,
	"eth_getBlockByHash":                      true,
	"eth_getBlockTransactionCountByNumber":    true,
	"eth_getBlockTransactionCountByHash":      true,
	"eth_getBlockReceipts":                    true,
	"eth_getUncleCountByBlockNumber":          true,
	"eth_getUncleCountByBlockHash":            true,
	"eth_getTransactionByHash":                true,
	"eth_getTransactionByBlockNumberAndIndex": true,
	"eth_getTransactionByBlockHashAndIndex":   true,
	"eth_getTransactionReceipt":               true,
	"eth_getLogs":                             true,
	"eth_call":                                true,
	"eth_estimateGas":                         true,
	// Simulation and tracing, all of which run against a snapshot.
	"debug_traceCall":               true,
	"debug_traceTransaction":        true,
	"debug_traceBlockByNumber":      true,
	"debug_traceBlockByHash":        true,
	"trace_call":                    true,
	"trace_transaction":             true,
	"trace_block":                   true,
	"trace_replayTransaction":       true,
	"trace_replayBlockTransactions": true,
	"trace_get":                     true,
	"trace_callMany":                true,
	"trace_filter":                  true,
}

// rpcRequestEnvelope is what a client posts to /rpc.
type rpcRequestEnvelope struct {
	Method string        `json:"method" binding:"required"`
	Params []interface{} `json:"params"`
}

// RPCPassthrough forwards one read-only JSON-RPC call to the node and returns
// the result verbatim. It exists so the explorer can run a real simulation —
// eth_estimateGas, eth_call, debug_traceCall — without every page needing its
// own server-side proxy. A method that is not on the allow-list is refused
// here rather than at the node.
func (h *Handler) RPCPassthrough(c *gin.Context) {
	var req rpcRequestEnvelope
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing method"})
		return
	}
	if !rpcPassthroughMethods[req.Method] {
		c.JSON(http.StatusForbidden, gin.H{
			"error":  "method not allowed",
			"method": req.Method,
		})
		return
	}

	ctx, cancel := context.WithTimeout(c.Request.Context(), rpcPassthroughTimeout)
	defer cancel()

	raw, err := h.rpcCall(ctx, req.Method, req.Params)
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "RPC call failed", "detail": err.Error()})
		return
	}
	c.Data(http.StatusOK, "application/json", raw)
}
