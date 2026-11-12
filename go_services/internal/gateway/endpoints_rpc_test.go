package gateway

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"
)

// The passthrough is the only place a browser request reaches the node through
// the gateway, so the allow-list is a security boundary, not a convenience.
// These cases pin down that write and admin methods are refused before any
// network call is attempted.

func postRPC(t *testing.T, h *Handler, body string) *httptest.ResponseRecorder {
	t.Helper()
	gin.SetMode(gin.TestMode)

	rec := httptest.NewRecorder()
	c, _ := gin.CreateTestContext(rec)
	c.Request = httptest.NewRequest(http.MethodPost, "/api/v1/rpc", bytes.NewBufferString(body))
	c.Request.Header.Set("Content-Type", "application/json")

	h.RPCPassthrough(c)
	return rec
}

func TestRPCPassthroughRefusesWriteAndAdminMethods(t *testing.T) {
	h := NewHandler(Config{})

	for _, method := range []string{
		"eth_sendRawTransaction",
		"eth_sendTransaction",
		"eth_signTransaction",
		"eth_sign",
		"personal_unlockAccount",
		"admin_addPeer",
		"admin_shutdown",
		"debug_setHead",
		"miner_start",
		"txpool_content",
	} {
		body, _ := json.Marshal(map[string]interface{}{"method": method, "params": []interface{}{}})
		rec := postRPC(t, h, string(body))

		if rec.Code != http.StatusForbidden {
			t.Errorf("%s: got status %d, want 403 (body %s)", method, rec.Code, rec.Body.String())
		}
	}
}

func TestRPCPassthroughAllowsReadOnlyMethods(t *testing.T) {
	// With no upstream configured, rpcCall fails with "no RPC URL configured",
	// which is a 503. Reaching that error is what proves the method passed the
	// allow-list: a refused method would have returned 403 without trying.
	h := NewHandler(Config{})

	for _, method := range []string{
		"eth_call",
		"eth_estimateGas",
		"eth_getBalance",
		"debug_traceCall",
		"trace_call",
	} {
		body, _ := json.Marshal(map[string]interface{}{"method": method, "params": []interface{}{}})
		rec := postRPC(t, h, string(body))

		if rec.Code == http.StatusForbidden {
			t.Errorf("%s: refused by the allow-list, but it is read-only", method)
		}
	}
}

func TestRPCPassthroughRejectsAMissingMethod(t *testing.T) {
	h := NewHandler(Config{})
	rec := postRPC(t, h, `{"params":[]}`)

	if rec.Code != http.StatusBadRequest {
		t.Errorf("got status %d, want 400", rec.Code)
	}
}
