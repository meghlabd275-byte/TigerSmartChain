package security

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"math/big"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// fakeNode serves canned JSON-RPC responses and records the requests it saw.
type fakeNode struct {
	blockNumber string
	logs        []rpcLog
	allowance   string
	requests    []map[string]interface{}
}

// The old code returned a fixed hash for a revocation that never happened.
func TestRevokeApprovalBuildsApproveCalldata(t *testing.T) {
	s := &Server{cfg: &Config{}}
	owner := "0x1111111111111111111111111111111111111111"
	token := "0x2222222222222222222222222222222222222222"
	spender := "0x3333333333333333333333333333333333333333"

	got, err := s.RevokeApproval(context.Background(), owner, token, spender)
	if err != nil {
		t.Fatalf("RevokeApproval: %v", err)
	}

	// The previous implementation returned this exact hash regardless of input.
	if got == "0x0000000000000000000000000000000000000000000000000000000000000001" {
		t.Fatal("RevokeApproval returned the old fixed transaction hash")
	}

	// approve(spender, 0): selector 0x095ea7b3, the spender left-padded to 32
	// bytes, then 32 bytes of zero.
	want := "0x095ea7b3" +
		"0000000000000000000000003333333333333333333333333333333333333333" +
		"0000000000000000000000000000000000000000000000000000000000000000"
	if got != want {
		t.Fatalf("calldata mismatch\n got %s\nwant %s", got, want)
	}
}

func TestRevokeApprovalRejectsBadAddresses(t *testing.T) {
	s := &Server{cfg: &Config{}}
	good := "0x1111111111111111111111111111111111111111"
	for _, tc := range []struct{ owner, token, spender string }{
		{"not-an-address", good, good},
		{good, "0x1234", good},
		{good, good, ""},
		{good, good, "1111111111111111111111111111111111111111"}, // no 0x
	} {
		if _, err := s.RevokeApproval(context.Background(), tc.owner, tc.token, tc.spender); err == nil {
			t.Fatalf("expected an error for %+v", tc)
		}
	}
}

func TestCheckTokenApprovalsRequiresRPCURL(t *testing.T) {
	s := &Server{cfg: &Config{}}
	_, err := s.CheckTokenApprovals(context.Background(), "0x1111111111111111111111111111111111111111")
	if err == nil {
		t.Fatal("with no RPC URL the call must fail, not return an empty list")
	}
}

func TestCheckTokenApprovalsRejectsBadOwner(t *testing.T) {
	s := &Server{cfg: &Config{RPCURL: "http://127.0.0.1:1"}}
	if _, err := s.CheckTokenApprovals(context.Background(), "nope"); err == nil {
		t.Fatal("a malformed owner must be an error")
	}
}

// The old code always returned an empty list. A real approval must be found,
// and one that has been spent to zero must not be reported.
func TestCheckTokenApprovalsReadsLiveAllowances(t *testing.T) {
	owner := "0x1111111111111111111111111111111111111111"
	liveToken := "0x2222222222222222222222222222222222222222"
	spentToken := "0x4444444444444444444444444444444444444444"
	spender := "0x3333333333333333333333333333333333333333"

	topicOwner := "0x000000000000000000000000" + strings.TrimPrefix(owner, "0x")
	topicSpender := "0x000000000000000000000000" + strings.TrimPrefix(spender, "0x")

	node := &fakeNode{
		blockNumber: "0x100",
		logs: []rpcLog{
			{Address: liveToken, Topics: []string{approvalEventTopic, topicOwner, topicSpender}},
			{Address: spentToken, Topics: []string{approvalEventTopic, topicOwner, topicSpender}},
		},
	}
	// eth_call is called once per (token, spender). Answer by inspecting the
	// `to` field so one token reads as live and the other as spent.
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string `json:"method"`
			Params []struct {
				To string `json:"to"`
			} `json:"params"`
		}
		json.NewDecoder(r.Body).Decode(&req)
		var result interface{}
		switch req.Method {
		case "eth_blockNumber":
			result = node.blockNumber
		case "eth_getLogs":
			result = node.logs
		case "eth_call":
			if strings.EqualFold(req.Params[0].To, liveToken) {
				result = "0x" + strings.Repeat("0", 62) + "ff"
			} else {
				result = "0x" + strings.Repeat("0", 64)
			}
		}
		json.NewEncoder(w).Encode(map[string]interface{}{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	defer srv.Close()

	s := &Server{cfg: &Config{RPCURL: srv.URL}}
	got, err := s.CheckTokenApprovals(context.Background(), owner)
	if err != nil {
		t.Fatalf("CheckTokenApprovals: %v", err)
	}

	if len(got) != 1 {
		t.Fatalf("expected only the live approval, got %d: %+v", len(got), got)
	}
	if got[0].Token != liveToken {
		t.Fatalf("wrong token reported: %s", got[0].Token)
	}
	if got[0].Allowance != "255" {
		t.Fatalf("allowance decoded to %q, want 255", got[0].Allowance)
	}
	if got[0].Spender != spender {
		t.Fatalf("spender extracted as %q, want %s", got[0].Spender, spender)
	}
	if got[0].IsInfinite {
		t.Fatal("an allowance of 255 is not infinite")
	}
}

// An unlimited approval must be reported as infinite.
func TestCheckTokenApprovalsFlagsInfiniteAllowance(t *testing.T) {
	owner := "0x1111111111111111111111111111111111111111"
	token := "0x2222222222222222222222222222222222222222"
	spender := "0x3333333333333333333333333333333333333333"
	topicOwner := "0x000000000000000000000000" + strings.TrimPrefix(owner, "0x")
	topicSpender := "0x000000000000000000000000" + strings.TrimPrefix(spender, "0x")

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string `json:"method"`
		}
		json.NewDecoder(r.Body).Decode(&req)
		var result interface{}
		switch req.Method {
		case "eth_blockNumber":
			result = "0x10"
		case "eth_getLogs":
			result = []rpcLog{{Address: token, Topics: []string{approvalEventTopic, topicOwner, topicSpender}}}
		case "eth_call":
			result = "0x" + strings.Repeat("f", 64)
		}
		json.NewEncoder(w).Encode(map[string]interface{}{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	defer srv.Close()

	s := &Server{cfg: &Config{RPCURL: srv.URL}}
	got, err := s.CheckTokenApprovals(context.Background(), owner)
	if err != nil {
		t.Fatalf("CheckTokenApprovals: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("expected one approval, got %d", len(got))
	}
	if !got[0].IsInfinite {
		t.Fatalf("max uint256 must be flagged infinite, got %s", got[0].Allowance)
	}
	want := maxUint256.String()
	if got[0].Allowance != want {
		t.Fatalf("allowance %q, want %q", got[0].Allowance, want)
	}
}

// The eth_call must carry the real allowance selector and both addresses.
func TestCheckTokenApprovalsSendsCorrectAllowanceCall(t *testing.T) {
	owner := "0x1111111111111111111111111111111111111111"
	token := "0x2222222222222222222222222222222222222222"
	spender := "0x3333333333333333333333333333333333333333"
	topicOwner := "0x000000000000000000000000" + strings.TrimPrefix(owner, "0x")
	topicSpender := "0x000000000000000000000000" + strings.TrimPrefix(spender, "0x")

	var callData string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string `json:"method"`
			Params []struct {
				Data string `json:"data"`
			} `json:"params"`
		}
		json.NewDecoder(r.Body).Decode(&req)
		var result interface{}
		switch req.Method {
		case "eth_blockNumber":
			result = "0x10"
		case "eth_getLogs":
			result = []rpcLog{{Address: token, Topics: []string{approvalEventTopic, topicOwner, topicSpender}}}
		case "eth_call":
			callData = req.Params[0].Data
			result = "0x" + strings.Repeat("0", 63) + "1"
		}
		json.NewEncoder(w).Encode(map[string]interface{}{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	defer srv.Close()

	s := &Server{cfg: &Config{RPCURL: srv.URL}}
	if _, err := s.CheckTokenApprovals(context.Background(), owner); err != nil {
		t.Fatalf("CheckTokenApprovals: %v", err)
	}

	want := "0x" + hex.EncodeToString(erc20AllowanceSelector) +
		strings.Repeat("0", 24) + strings.TrimPrefix(owner, "0x") +
		strings.Repeat("0", 24) + strings.TrimPrefix(spender, "0x")
	if callData != want {
		t.Fatalf("eth_call data mismatch\n got %s\nwant %s", callData, want)
	}
}

// A node error must surface rather than be reported as "no approvals".
func TestCheckTokenApprovalsPropagatesNodeError(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(map[string]interface{}{
			"jsonrpc": "2.0",
			"id":      1,
			"error":   map[string]interface{}{"code": -32000, "message": "block range too large"},
		})
	}))
	defer srv.Close()

	s := &Server{cfg: &Config{RPCURL: srv.URL}}
	_, err := s.CheckTokenApprovals(context.Background(), "0x1111111111111111111111111111111111111111")
	if err == nil {
		t.Fatal("a node error must be returned, not swallowed into an empty list")
	}
	if !strings.Contains(err.Error(), "block range too large") {
		t.Fatalf("error does not carry the node message: %v", err)
	}
}

// A token that returns no data must not be reported as having a zero allowance.
func TestCheckTokenApprovalsSkipsTokenWithEmptyReturn(t *testing.T) {
	owner := "0x1111111111111111111111111111111111111111"
	token := "0x2222222222222222222222222222222222222222"
	spender := "0x3333333333333333333333333333333333333333"
	topicOwner := "0x000000000000000000000000" + strings.TrimPrefix(owner, "0x")
	topicSpender := "0x000000000000000000000000" + strings.TrimPrefix(spender, "0x")

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string `json:"method"`
		}
		json.NewDecoder(r.Body).Decode(&req)
		var result interface{}
		switch req.Method {
		case "eth_blockNumber":
			result = "0x10"
		case "eth_getLogs":
			result = []rpcLog{{Address: token, Topics: []string{approvalEventTopic, topicOwner, topicSpender}}}
		case "eth_call":
			result = "0x"
		}
		json.NewEncoder(w).Encode(map[string]interface{}{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	defer srv.Close()

	s := &Server{cfg: &Config{RPCURL: srv.URL}}
	got, err := s.CheckTokenApprovals(context.Background(), owner)
	if err != nil {
		t.Fatalf("CheckTokenApprovals: %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("an unreadable token must not produce an approval entry: %+v", got)
	}
}

func TestMaxUint256IsTwoToThe256MinusOne(t *testing.T) {
	want, _ := new(big.Int).SetString(strings.Repeat("f", 64), 16)
	if maxUint256.Cmp(want) != 0 {
		t.Fatalf("maxUint256 is wrong: %s", maxUint256)
	}
}
