package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
)

func testRouter(t *testing.T) *gin.Engine {
	t.Helper()
	gin.SetMode(gin.TestMode)
	return newRouter(&Config{
		Security: SecurityConfig{},
		RateLimit: RateLimitConfig{
			RequestsPerSecond: 10000,
			Burst:             10000,
			Enabled:           true,
		},
	}, NewHandler(nil))
}

func do(t *testing.T, r *gin.Engine, method, path, body string, headers map[string]string) *httptest.ResponseRecorder {
	t.Helper()
	var reader *strings.Reader
	if body == "" {
		reader = strings.NewReader("")
	} else {
		reader = strings.NewReader(body)
	}
	req := httptest.NewRequest(method, path, reader)
	// httptest defaults the request host to example.com, which would make any
	// Origin header same-origin and cause the CORS middleware to skip the
	// request entirely. Pin a different host so CORS is actually exercised.
	req.Host = "api.tiger.example"
	if body != "" {
		req.Header.Set("Content-Type", "application/json")
	}
	for k, v := range headers {
		req.Header.Set(k, v)
	}
	w := httptest.NewRecorder()
	r.ServeHTTP(w, req)
	return w
}

// The router must expose every implemented handler, and registering them must
// not panic on a duplicate path (gin panics when two routes collide).
func TestRouteTableRegisters(t *testing.T) {
	r := testRouter(t)
	routes := r.Routes()

	want := []string{
		"GET /health",
		"GET /ready",
		"GET /api/v1/blocks",
		"GET /api/v1/transactions/:hash/trace",
		"GET /api/v1/transactions/:hash/state-diffs",
		"POST /api/v1/debug/trace",
		"POST /api/v1/debug/trace-call",
		"GET /api/v1/debug/trace-tx/:hash",
		"GET /api/v1/tokens/:address/distribution",
		"GET /api/v1/tokens/:address/price-history",
		"GET /api/v1/nfts/collections/:collection/floor-price",
		"GET /api/v1/nfts/collections/:collection/owners",
		"GET /api/v1/nfts/collections/:collection/:token_id/rarity",
		"GET /api/v1/governance/proposals",
		"GET /api/v1/governance/proposals/:id",
		"GET /api/v1/governance/proposals/:id/votes",
		"GET /api/v1/governance/delegates/:address",
		"GET /api/v1/mev/bundles",
		"GET /api/v1/mev/bundles/:hash",
		"GET /api/v1/mev/search",
		"GET /api/v1/accounts/:address/history",
		"GET /api/v1/logs",
		"GET /api/v1/bridges/transfers",
		"GET /api/v1/historical/balance/:address",
		"GET /api/v1/historical/state/:address",
		"GET /api/v1/historical/storage/:address/:slot",
		"GET /api/v1/node/block/latest",
		"GET /api/v1/node/chain-id",
		"GET /api/v1/node/gas-price",
		"GET /api/v1/node/peers",
		"GET /api/v1/node/syncing",
		"GET /api/v1/stats/network",
		"GET /api/v1/stats/gas",
	}

	seen := map[string]bool{}
	for _, rt := range routes {
		seen[rt.Method+" "+rt.Path] = true
	}
	for _, w := range want {
		if !seen[w] {
			t.Errorf("route not registered: %s", w)
		}
	}

	// No path may be registered twice for the same method.
	counts := map[string]int{}
	for _, rt := range routes {
		counts[rt.Method+" "+rt.Path]++
	}
	for k, n := range counts {
		if n > 1 {
			t.Errorf("duplicate route %s registered %d times", k, n)
		}
	}
}

// Health must answer without touching the database.
func TestHealthDoesNotNeedDatabase(t *testing.T) {
	r := testRouter(t)
	w := do(t, r, http.MethodGet, "/health", "", nil)
	if w.Code != http.StatusOK {
		t.Fatalf("health status = %d, want 200", w.Code)
	}
	var body map[string]interface{}
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatalf("health body is not JSON: %v", err)
	}
	if body["status"] != "ok" {
		t.Errorf("health status field = %v, want ok", body["status"])
	}
}

// Security headers must be present on every response.
func TestSecurityHeadersPresent(t *testing.T) {
	r := testRouter(t)
	w := do(t, r, http.MethodGet, "/health", "", nil)
	for _, h := range []string{
		"X-Content-Type-Options",
		"X-Frame-Options",
		"Referrer-Policy",
		"Content-Security-Policy",
	} {
		if w.Header().Get(h) == "" {
			t.Errorf("missing security header %s", h)
		}
	}
}

// A wildcard origin must not be paired with AllowCredentials, which browsers
// reject outright.
func TestCORSWildcardDisablesCredentials(t *testing.T) {
	gin.SetMode(gin.TestMode)
	r := newRouter(&Config{
		RateLimit: RateLimitConfig{RequestsPerSecond: 10000, Burst: 10000, Enabled: true},
	}, NewHandler(nil))

	w := do(t, r, http.MethodOptions, "/api/v1/blocks", "", map[string]string{
		"Origin":                        "https://example.com",
		"Access-Control-Request-Method": "GET",
	})
	if got := w.Header().Get("Access-Control-Allow-Credentials"); got != "" {
		t.Errorf("wildcard origin set Access-Control-Allow-Credentials=%q, want empty", got)
	}
	if got := w.Header().Get("Access-Control-Allow-Origin"); got != "*" {
		t.Errorf("Allow-Origin = %q, want *", got)
	}
}

// An explicit allow-list turns credentials on for that origin.
func TestCORSExplicitOriginAllowsCredentials(t *testing.T) {
	gin.SetMode(gin.TestMode)
	r := newRouter(&Config{
		Security:  SecurityConfig{CorsOrigins: []string{"https://scan.tiger.example"}},
		RateLimit: RateLimitConfig{RequestsPerSecond: 10000, Burst: 10000, Enabled: true},
	}, NewHandler(nil))

	w := do(t, r, http.MethodOptions, "/api/v1/blocks", "", map[string]string{
		"Origin":                        "https://scan.tiger.example",
		"Access-Control-Request-Method": "GET",
	})
	if got := w.Header().Get("Access-Control-Allow-Origin"); got != "https://scan.tiger.example" {
		t.Errorf("Allow-Origin = %q, want the allow-listed origin", got)
	}
	if got := w.Header().Get("Access-Control-Allow-Credentials"); got != "true" {
		t.Errorf("Allow-Credentials = %q, want true", got)
	}
}

// The API-key middleware must reject a missing key, an unknown key and a
// disabled key, and accept a valid one.
func TestAPIKeyAuth(t *testing.T) {
	gin.SetMode(gin.TestMode)
	h := NewHandler(nil)
	h.apiKeys.Add(&APIKey{Key: "valid-key", Name: "test", Scopes: []string{"read"}})
	h.apiKeys.Add(&APIKey{Key: "disabled-key", Name: "test", Scopes: []string{"read"}, Disabled: true})
	h.apiKeys.Add(&APIKey{Key: "narrow-key", Name: "test", Scopes: []string{"admin"}})

	r := newRouter(&Config{
		Security:  SecurityConfig{APIKeys: []string{"valid-key"}},
		RateLimit: RateLimitConfig{RequestsPerSecond: 10000, Burst: 10000, Enabled: true},
	}, h)

	cases := []struct {
		name    string
		headers map[string]string
		want    int
	}{
		{"missing key", nil, http.StatusUnauthorized},
		{"unknown key", map[string]string{"X-API-Key": "nope"}, http.StatusForbidden},
		{"disabled key", map[string]string{"X-API-Key": "disabled-key"}, http.StatusForbidden},
		{"wrong scope", map[string]string{"X-API-Key": "narrow-key"}, http.StatusForbidden},
		{"valid header", map[string]string{"X-API-Key": "valid-key"}, http.StatusOK},
		{"valid bearer", map[string]string{"Authorization": "Bearer valid-key"}, http.StatusOK},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			// /api/v1/blocks with no database returns 500 once authorised;
			// authentication failures short-circuit first.
			w := do(t, r, http.MethodGet, "/api/v1/blocks", "", tc.headers)
			if tc.want == http.StatusOK {
				if w.Code == http.StatusUnauthorized || w.Code == http.StatusForbidden {
					t.Fatalf("authorised request rejected: %d %s", w.Code, w.Body.String())
				}
				return
			}
			if w.Code != tc.want {
				t.Fatalf("status = %d, want %d (%s)", w.Code, tc.want, w.Body.String())
			}
		})
	}
}

// Health and readiness must stay reachable even when API keys are configured.
func TestHealthExemptFromAPIKey(t *testing.T) {
	gin.SetMode(gin.TestMode)
	r := newRouter(&Config{
		Security:  SecurityConfig{APIKeys: []string{"valid-key"}},
		RateLimit: RateLimitConfig{RequestsPerSecond: 10000, Burst: 10000, Enabled: true},
	}, NewHandler(nil))

	w := do(t, r, http.MethodGet, "/health", "", nil)
	if w.Code != http.StatusOK {
		t.Fatalf("health with API keys configured = %d, want 200", w.Code)
	}
}

// Malformed input must be rejected before any database work happens.
func TestInputValidation(t *testing.T) {
	r := testRouter(t)
	const validAddr = "0x1111111111111111111111111111111111111111"
	const validHash = "0x" + "ab" + "000000000000000000000000000000000000000000000000000000000000"

	cases := []struct {
		name   string
		method string
		path   string
		body   string
		want   int
	}{
		{"bad trace hash", http.MethodGet, "/api/v1/transactions/notahash/trace", "", http.StatusBadRequest},
		{"short trace hash", http.MethodGet, "/api/v1/transactions/0xabc/trace", "", http.StatusBadRequest},
		{"bad distribution address", http.MethodGet, "/api/v1/tokens/xyz/distribution", "", http.StatusBadRequest},
		{"bad rarity token id", http.MethodGet, "/api/v1/nfts/collections/" + validAddr + "/notanumber/rarity", "", http.StatusBadRequest},
		{"bad rarity address", http.MethodGet, "/api/v1/nfts/collections/nope/1/rarity", "", http.StatusBadRequest},
		{"bad historical block", http.MethodGet, "/api/v1/historical/balance/" + validAddr + "?block=-1", "", http.StatusBadRequest},
		{"missing block param", http.MethodGet, "/api/v1/historical/balance/" + validAddr, "", http.StatusBadRequest},
		{"missing mev query", http.MethodGet, "/api/v1/mev/search", "", http.StatusBadRequest},
		{"read contract bad body", http.MethodPost, "/api/v1/contracts/" + validAddr + "/read", `{}`, http.StatusBadRequest},
		{"read contract bad data", http.MethodPost, "/api/v1/contracts/" + validAddr + "/read", `{"data":"zz"}`, http.StatusBadRequest},
		{"write contract bad body", http.MethodPost, "/api/v1/contracts/" + validAddr + "/write", `{}`, http.StatusBadRequest},
		{"write contract bad hex", http.MethodPost, "/api/v1/contracts/" + validAddr + "/write", `{"signedTx":"0xzz"}`, http.StatusBadRequest},
		{"debug trace bad hash", http.MethodPost, "/api/v1/debug/trace", `{"txHash":"nope"}`, http.StatusBadRequest},
		{"debug trace call bad from", http.MethodPost, "/api/v1/debug/trace-call", `{"from":"nope"}`, http.StatusBadRequest},
		{"debug trace tx bad hash", http.MethodGet, "/api/v1/debug/trace-tx/nope", "", http.StatusBadRequest},
		{"logs bad address", http.MethodGet, "/api/v1/logs?address=nope", "", http.StatusBadRequest},
		{"logs bad hash", http.MethodGet, "/api/v1/logs?txHash=nope", "", http.StatusBadRequest},
		{"bridge bad address", http.MethodGet, "/api/v1/bridges/transfers?address=nope", "", http.StatusBadRequest},
		{"delegates bad address", http.MethodGet, "/api/v1/governance/delegates/nope", "", http.StatusBadRequest},
	}

	_ = validHash
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			w := do(t, r, tc.method, tc.path, tc.body, nil)
			if w.Code != tc.want {
				t.Fatalf("status = %d, want %d (%s)", w.Code, tc.want, w.Body.String())
			}
		})
	}
}

// Node-backed handlers must report that no node is configured instead of
// returning invented data.
func TestNodeHandlersWithoutNode(t *testing.T) {
	r := testRouter(t)
	paths := []string{
		"/api/v1/node/block/latest",
		"/api/v1/node/chain-id",
		"/api/v1/node/gas-price",
		"/api/v1/node/peers",
		"/api/v1/node/syncing",
	}
	for _, p := range paths {
		w := do(t, r, http.MethodGet, p, "", nil)
		if w.Code != http.StatusServiceUnavailable {
			t.Errorf("%s status = %d, want 503", p, w.Code)
		}
		if !strings.Contains(w.Body.String(), "not configured") {
			t.Errorf("%s body = %s, want a 'not configured' error", p, w.Body.String())
		}
	}
}

// A debug trace without a node must also fail loudly.
func TestDebugTraceWithoutNode(t *testing.T) {
	r := testRouter(t)
	hash := "0x" + strings.Repeat("ab", 32)
	w := do(t, r, http.MethodPost, "/api/v1/debug/trace",
		`{"txHash":"`+hash+`"}`, nil)
	if w.Code != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want 503 (%s)", w.Code, w.Body.String())
	}
}

// Validation helpers must accept exactly the canonical Ethereum encodings.
func TestValidationHelpers(t *testing.T) {
	valid := []string{
		"0x1111111111111111111111111111111111111111",
		"0xAbC0000000000000000000000000000000000001",
	}
	for _, a := range valid {
		if !isAddress(a) {
			t.Errorf("isAddress(%q) = false, want true", a)
		}
	}
	invalid := []string{
		"", "0x", "1111111111111111111111111111111111111111",
		"0x11111111111111111111111111111111111111",
		"0x111111111111111111111111111111111111111g",
		"0x11111111111111111111111111111111111111111",
	}
	for _, a := range invalid {
		if isAddress(a) {
			t.Errorf("isAddress(%q) = true, want false", a)
		}
	}

	if !isTxHash("0x" + strings.Repeat("ab", 32)) {
		t.Error("isTxHash rejected a valid hash")
	}
	for _, h := range []string{"", "0x", "0x" + strings.Repeat("ab", 31), "0x" + strings.Repeat("zz", 32)} {
		if isTxHash(h) {
			t.Errorf("isTxHash(%q) = true, want false", h)
		}
	}

	for _, d := range []string{"0x", "0xab", "0xabcd"} {
		if !isHexData(d) {
			t.Errorf("isHexData(%q) = false, want true", d)
		}
	}
	for _, d := range []string{"", "0xabc", "abc", "0xzz"} {
		if isHexData(d) {
			t.Errorf("isHexData(%q) = true, want false", d)
		}
	}
}

// hexQuantity must render decimal amounts as canonical Ethereum quantities.
func TestHexQuantity(t *testing.T) {
	cases := map[string]string{
		"0":     "0x0",
		"1":     "0x1",
		"15":    "0xf",
		"16":    "0x10",
		"255":   "0xff",
		"256":   "0x100",
		"":      "0x0",
		"abc":   "0x0",
		" 100 ": "0x64",
	}
	for in, want := range cases {
		if got := hexQuantity(in); got != want {
			t.Errorf("hexQuantity(%q) = %q, want %q", in, got, want)
		}
	}

	// A 256-bit value must survive intact, not be truncated.
	big256 := "115792089237316195423570985008687907853269984665640564039457584007913129639935"
	got := hexQuantity(big256)
	if !strings.HasPrefix(got, "0x") || len(got) != 66 {
		t.Errorf("hexQuantity(2^256-1) = %q, want a 0x-prefixed 64-digit quantity", got)
	}
}

// paginationFrom must clamp limits and refuse nonsensical pages.
func TestPaginationFrom(t *testing.T) {
	gin.SetMode(gin.TestMode)
	r := gin.New()
	var captured Pagination
	r.GET("/t", func(c *gin.Context) {
		captured = paginationFrom(c, 25)
		c.String(http.StatusOK, "ok")
	})

	cases := []struct {
		query string
		page  int
		limit int
	}{
		{"", 1, 25},
		{"?page=3&limit=10", 3, 10},
		{"?page=0", 1, 25},
		{"?page=-5", 1, 25},
		{"?limit=0", 1, 25},
		{"?limit=-1", 1, 25},
		{"?limit=1000000", 1, 1000000},
		{"?page=abc&limit=xyz", 1, 25},
	}
	for _, tc := range cases {
		do(t, r, http.MethodGet, "/t"+tc.query, "", nil)
		if captured.Page != tc.page || captured.Limit != tc.limit {
			t.Errorf("paginationFrom(%q) = page %d limit %d, want page %d limit %d",
				tc.query, captured.Page, captured.Limit, tc.page, tc.limit)
		}
	}
}

// The limit actually sent to SQL must be clamped to MaxLimit.
func TestPaginationGetLimitClamps(t *testing.T) {
	p := Pagination{Page: 1, Limit: 100000, MaxLimit: 100}
	if got := p.GetLimit(); got != 100 {
		t.Errorf("GetLimit() = %d, want 100", got)
	}
	p = Pagination{Page: 1, Limit: 0, MaxLimit: 100}
	if got := p.GetLimit(); got != 100 {
		t.Errorf("GetLimit() with zero limit = %d, want 100", got)
	}
	p = Pagination{Page: 2, Limit: 25, MaxLimit: 100}
	if got := p.Offset(); got != 25 {
		t.Errorf("Offset() = %d, want 25", got)
	}
}

// dsnValue must quote values that would otherwise corrupt a libpq string.
func TestDSNValueQuoting(t *testing.T) {
	cases := map[string]string{
		"simple":       "simple",
		"tigerscan":    "tigerscan",
		"has space":    "'has space'",
		"quote'inside": "'quote\\'inside'",
		"back\\slash":  "'back\\\\slash'",
		"host.example": "host.example",
	}
	for in, want := range cases {
		if got := dsnValue(in); got != want {
			t.Errorf("dsnValue(%q) = %q, want %q", in, got, want)
		}
	}
}

// Environment helpers must fall back cleanly on missing or malformed values.
func TestEnvHelpers(t *testing.T) {
	if got := envStr("TIGER_TEST_UNSET_VAR", "fallback"); got != "fallback" {
		t.Errorf("envStr default = %q, want fallback", got)
	}
	if got := envInt("TIGER_TEST_UNSET_VAR", 42); got != 42 {
		t.Errorf("envInt default = %d, want 42", got)
	}

	t.Setenv("TIGER_TEST_STR", "value")
	t.Setenv("TIGER_TEST_INT", "7")
	t.Setenv("TIGER_TEST_BAD_INT", "notanumber")
	t.Setenv("TIGER_TEST_LIST", "a, b ,c")

	if got := envStr("TIGER_TEST_STR", "fallback"); got != "value" {
		t.Errorf("envStr = %q, want value", got)
	}
	if got := envInt("TIGER_TEST_INT", 42); got != 7 {
		t.Errorf("envInt = %d, want 7", got)
	}
	if got := envInt("TIGER_TEST_BAD_INT", 42); got != 42 {
		t.Errorf("envInt on malformed value = %d, want 42", got)
	}
	got := envList("TIGER_TEST_LIST", nil)
	want := []string{"a", "b", "c"}
	if len(got) != len(want) {
		t.Fatalf("envList = %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("envList[%d] = %q, want %q", i, got[i], want[i])
		}
	}
	if got := envList("TIGER_TEST_UNSET_VAR", []string{"d"}); len(got) != 1 || got[0] != "d" {
		t.Errorf("envList default = %v, want [d]", got)
	}
}

// rawJSON must pass valid JSON through and yield nil for empty input.
func TestRawJSON(t *testing.T) {
	if rawJSON("") != nil {
		t.Error("rawJSON(\"\") should be nil")
	}
	if rawJSON("   ") != nil {
		t.Error("rawJSON(whitespace) should be nil")
	}
	out := rawJSON(`{"a":1}`)
	encoded, err := json.Marshal(out)
	if err != nil {
		t.Fatalf("rawJSON output is not marshalable: %v", err)
	}
	if string(encoded) != `{"a":1}` {
		t.Errorf("rawJSON round-trip = %s, want {\"a\":1}", encoded)
	}
}

// Handler construction must not require a database.
func TestNewHandlerWithoutDatabase(t *testing.T) {
	h := NewHandler(nil)
	if h.nodeRPC == nil {
		t.Error("nodeRPC client should be initialised")
	}
	if h.rateLimiter == nil || h.apiKeys == nil || h.webhooks == nil {
		t.Error("handler collaborators should be initialised")
	}
}

// The JSON-RPC forwarder must fail clearly when no endpoint is configured.
func TestRPCCallWithoutEndpoint(t *testing.T) {
	h := NewHandler(nil)
	if _, err := h.rpcCall(t.Context(), "eth_chainId"); err == nil {
		t.Fatal("rpcCall with no endpoint should error")
	} else if !strings.Contains(err.Error(), "not configured") {
		t.Errorf("error = %v, want a 'not configured' message", err)
	}
}

// The GCRA limiter reports whether a request is *limited*, not whether it is
// allowed. Reading that value the other way round rejects every honest request
// while admitting an unlimited flood, so pin the polarity explicitly.
func TestRateLimiterPolarity(t *testing.T) {
	rl, err := NewRateLimiter(RateLimitConfig{Enabled: true, RequestsPerSecond: 100, Burst: 100})
	if err != nil {
		t.Fatalf("NewRateLimiter: %v", err)
	}

	// A fresh client well under quota must be allowed.
	for i := 0; i < 10; i++ {
		allowed, _ := rl.Allow("ip:10.0.0.1")
		if !allowed {
			t.Fatalf("request %d of 10 rejected, want allowed", i+1)
		}
	}

	// A different client has its own independent budget.
	if allowed, _ := rl.Allow("ip:10.0.0.2"); !allowed {
		t.Fatal("second client rejected, want its own budget")
	}
}

// Exhausting a client's burst must start rejecting, with a retry hint.
func TestRateLimiterBlocksAfterBurst(t *testing.T) {
	rl, err := NewRateLimiter(RateLimitConfig{Enabled: true, RequestsPerSecond: 1, Burst: 5})
	if err != nil {
		t.Fatalf("NewRateLimiter: %v", err)
	}

	allowed := 0
	for i := 0; i < 100; i++ {
		ok, _ := rl.Allow("ip:10.0.0.9")
		if ok {
			allowed++
		}
	}
	// Burst is 5, so only a handful may pass; certainly not all 100.
	if allowed == 0 {
		t.Fatal("no requests allowed at all, want some to pass before the burst is spent")
	}
	if allowed >= 100 {
		t.Fatalf("allowed %d of 100 requests, want the limiter to engage", allowed)
	}
}

// A disabled limiter must let everything through.
func TestRateLimiterDisabledAllowsAll(t *testing.T) {
	rl, err := NewRateLimiter(RateLimitConfig{Enabled: false, RequestsPerSecond: 1, Burst: 1})
	if err != nil {
		t.Fatalf("NewRateLimiter: %v", err)
	}
	for i := 0; i < 1000; i++ {
		if allowed, _ := rl.Allow("ip:10.0.0.3"); !allowed {
			t.Fatalf("request %d rejected while limiter disabled", i+1)
		}
	}
}

// Banning must deny the client and expire on demand.
func TestRateLimiterBan(t *testing.T) {
	rl, err := NewRateLimiter(RateLimitConfig{Enabled: true, RequestsPerSecond: 1000, Burst: 1000})
	if err != nil {
		t.Fatalf("NewRateLimiter: %v", err)
	}
	const key = "ip:10.0.0.4"

	if rl.IsBanned(key) {
		t.Fatal("client reported banned before Ban was called")
	}
	rl.Ban(key, time.Minute)
	if !rl.IsBanned(key) {
		t.Fatal("client not banned after Ban")
	}
	rl.Unban(key)
	if rl.IsBanned(key) {
		t.Fatal("client still banned after Unban")
	}

	// An already-expired ban must not linger.
	rl.Ban(key, -time.Second)
	if rl.IsBanned(key) {
		t.Fatal("expired ban still reported as active")
	}
}

// clientKey must bucket by API key when present and by address otherwise, so
// one tenant cannot spend another tenant's quota.
func TestClientKeyBucketing(t *testing.T) {
	gin.SetMode(gin.TestMode)

	newCtx := func(headers map[string]string) *gin.Context {
		c, _ := gin.CreateTestContext(httptest.NewRecorder())
		c.Request = httptest.NewRequest(http.MethodGet, "/", nil)
		for k, v := range headers {
			c.Request.Header.Set(k, v)
		}
		return c
	}

	if got := clientKey(newCtx(nil)); !strings.HasPrefix(got, "ip:") {
		t.Errorf("anonymous clientKey = %q, want an ip: bucket", got)
	}
	if got := clientKey(newCtx(map[string]string{"X-API-Key": "abc"})); got != "key:abc" {
		t.Errorf("header clientKey = %q, want key:abc", got)
	}
	if got := clientKey(newCtx(map[string]string{"Authorization": "Bearer xyz"})); got != "key:xyz" {
		t.Errorf("bearer clientKey = %q, want key:xyz", got)
	}
	// Two different keys must not collide.
	if clientKey(newCtx(map[string]string{"X-API-Key": "a"})) ==
		clientKey(newCtx(map[string]string{"X-API-Key": "b"})) {
		t.Error("distinct API keys produced the same bucket")
	}
}

// A request that exceeds the limit must be answered with 429 and a Retry-After,
// not a silent pass.
func TestRouterEnforcesLimit(t *testing.T) {
	gin.SetMode(gin.TestMode)
	r := newRouter(&Config{
		RateLimit: RateLimitConfig{RequestsPerSecond: 1, Burst: 2, Enabled: true},
	}, NewHandler(nil))

	statuses := make([]int, 0, 40)
	for i := 0; i < 40; i++ {
		w := do(t, r, http.MethodGet, "/health", "", nil)
		statuses = append(statuses, w.Code)
		if w.Code == http.StatusTooManyRequests {
			if w.Header().Get("Retry-After") == "" {
				t.Error("429 response is missing Retry-After")
			}
			return
		}
	}
	t.Fatalf("no request was ever rate limited, statuses=%v", statuses)
}
