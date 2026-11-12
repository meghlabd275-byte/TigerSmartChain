package gateway

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
)

// Every selector below was cross-checked against an independent keccak256
// implementation. They are hardcoded on purpose: the point of the test is to
// catch the code and the hash drifting apart, which is exactly how
// burn(uint256) came to be published as 0x2e1a7d4d.
func TestSelectorForMatchesKnownSelectors(t *testing.T) {
	cases := map[string]string{
		"transfer(address,uint256)":                 "a9059cbb",
		"transferFrom(address,address,uint256)":     "23b872dd",
		"approve(address,uint256)":                  "095ea7b3",
		"mint(address,uint256)":                     "40c10f19",
		"burn(uint256)":                             "42966c68",
		"withdraw(uint256)":                         "2e1a7d4d",
		"balanceOf(address)":                        "70a08231",
		"totalSupply()":                             "18160ddd",
		"decimals()":                                "313ce567",
		"symbol()":                                  "95d89b41",
		"name()":                                    "06fdde03",
		"ownerOf(uint256)":                          "6352211e",
		"tokenURI(uint256)":                         "c87b56dd",
		"safeTransferFrom(address,address,uint256)": "42842e0e",
		"safeBatchTransferFrom(address,address,uint256[],uint256[],bytes)": "2eb2c2d6",
		"stake(uint256)":   "a694fc3a",
		"unstake(uint256)": "2e17de78",
		"getReward()":      "3d18b912",
		"deposit()":        "d0e30db0",
	}

	for sig, want := range cases {
		got, err := SelectorFor(sig)
		if err != nil {
			t.Errorf("SelectorFor(%q): %v", sig, err)
			continue
		}
		if got != want {
			t.Errorf("SelectorFor(%q) = %s, want %s", sig, got, want)
		}
	}
}

// The bug this guards against: 0x2e1a7d4d was labelled burn(uint256) in the
// tools page. It is withdraw(uint256). burn(uint256) is 0x42966c68.
func TestBurnAndWithdrawSelectorsAreDistinct(t *testing.T) {
	burn, err := SelectorFor("burn(uint256)")
	if err != nil {
		t.Fatal(err)
	}
	withdraw, err := SelectorFor("withdraw(uint256)")
	if err != nil {
		t.Fatal(err)
	}
	if burn == withdraw {
		t.Fatalf("burn and withdraw produced the same selector %s", burn)
	}
	if burn != "42966c68" {
		t.Errorf("burn(uint256) = %s, want 42966c68", burn)
	}
	if withdraw != "2e1a7d4d" {
		t.Errorf("withdraw(uint256) = %s, want 2e1a7d4d", withdraw)
	}
}

// keccak256 is over the canonical text, so aliases have to be resolved before
// hashing or the selector belongs to no contract.
func TestSelectorForCanonicalisesAliases(t *testing.T) {
	aliases := map[string]string{
		"function transfer(address to, uint256 amount)": "transfer(address,uint256)",
		"transfer (address,uint256)":                    "transfer(address,uint256)",
		"transfer(address,uint)":                        "transfer(address,uint256)",
		"transfer(address,uint)   ":                     "transfer(address,uint256)",
	}
	for input, canonical := range aliases {
		got, err := SelectorFor(input)
		if err != nil {
			t.Errorf("SelectorFor(%q): %v", input, err)
			continue
		}
		want, err := SelectorFor(canonical)
		if err != nil {
			t.Fatal(err)
		}
		if got != want {
			t.Errorf("SelectorFor(%q) = %s, want %s (same as %q)", input, got, want, canonical)
		}
	}
}

func TestTopicForMatchesKnownTopics(t *testing.T) {
	cases := map[string]string{
		"Transfer(address,address,uint256)":                       "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
		"Approval(address,address,uint256)":                       "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925",
		"TransferSingle(address,address,address,uint256,uint256)": "0xc3d58168c5ae7397731d063d5bbf3d657854427343f4c083240f7aacaa2d0f62",
		"OwnershipTransferred(address,address)":                   "0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0",
	}
	for sig, want := range cases {
		got, err := TopicFor(sig)
		if err != nil {
			t.Errorf("TopicFor(%q): %v", sig, err)
			continue
		}
		if got != want {
			t.Errorf("TopicFor(%q) = %s, want %s", sig, got, want)
		}
	}
}

// A topic is 32 bytes and a selector is 4; confusing them is what put a wrong
// topic into the indexer.
func TestTopicAndSelectorAreDifferentLengths(t *testing.T) {
	selector, err := SelectorFor("transfer(address,uint256)")
	if err != nil {
		t.Fatal(err)
	}
	topic, err := TopicFor("Transfer(address,address,uint256)")
	if err != nil {
		t.Fatal(err)
	}
	if len(selector) != 8 {
		t.Errorf("selector %q has %d hex chars, want 8", selector, len(selector))
	}
	if len(topic) != 66 {
		t.Errorf("topic %s has %d chars, want 66", topic, len(topic))
	}
}

func TestSelectorForRejectsMalformedInput(t *testing.T) {
	bad := []string{
		"",
		"   ",
		"transfer",
		"transfer(",
		"transfer(address",
		"(address,uint256)",
		"transfer(address,notatype)",
		"1transfer(address)",
	}
	for _, input := range bad {
		if got, err := SelectorFor(input); err == nil {
			t.Errorf("SelectorFor(%q) = %s, want an error", input, got)
		}
	}
}

// The canonical lists are shipped to users, so a typo in one of them is a
// wrong selector in the UI. This parses every entry.
func TestCanonicalListsAreAllValid(t *testing.T) {
	for _, sig := range canonicalFunctions {
		if _, err := SelectorFor(sig); err != nil {
			t.Errorf("canonical function %q does not parse: %v", sig, err)
		}
	}
	for _, sig := range canonicalEvents {
		if _, err := TopicFor(sig); err != nil {
			t.Errorf("canonical event %q does not parse: %v", sig, err)
		}
	}
}

// The indexer publishes these topic values in its source. They must agree with
// what the tools endpoint computes, or the explorer will label a log with the
// wrong event.
func TestPublishedTopicsMatchComputedOnes(t *testing.T) {
	published := map[string]string{
		"Transfer(address,address,uint256)": "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
		"Approval(address,address,uint256)": "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925",
	}
	for sig, want := range published {
		got, err := TopicFor(sig)
		if err != nil {
			t.Fatal(err)
		}
		if got != want {
			t.Errorf("published topic for %q is %s but keccak256 gives %s", sig, want, got)
		}
	}
}

// jsonRequest builds a gin context with a JSON body, for the POST tools
// endpoints that bind from the request body.
func jsonRequest(method, target, body string) (*gin.Context, *httptest.ResponseRecorder) {
	gin.SetMode(gin.TestMode)
	rec := httptest.NewRecorder()
	c, _ := gin.CreateTestContext(rec)
	c.Request = httptest.NewRequest(method, target, strings.NewReader(body))
	c.Request.Header.Set("Content-Type", "application/json")
	return c, rec
}

// ComputeSignature needs no database, so it is testable directly.
func TestComputeSignatureEndpoint(t *testing.T) {
	h := &Handler{}

	c, rec := jsonRequest(http.MethodPost, "/tools/signature",
		`{"signature":"transfer(address,uint256)"}`)
	h.ComputeSignature(c)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", rec.Code, rec.Body.String())
	}
	var resp map[string]interface{}
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	if resp["selector"] != "0xa9059cbb" {
		t.Errorf("selector = %v, want 0xa9059cbb", resp["selector"])
	}
}

func TestComputeSignatureRejectsBadInput(t *testing.T) {
	h := &Handler{}
	c, rec := jsonRequest(http.MethodPost, "/tools/signature", `{"signature":"nonsense("}`)
	h.ComputeSignature(c)
	if rec.Code != http.StatusBadRequest {
		t.Errorf("status = %d, want 400", rec.Code)
	}
}

// EncodeCall must produce real calldata. The old page returned the string
// "Encoded data would appear here...".
func TestEncodeCallProducesRealCalldata(t *testing.T) {
	h := &Handler{}
	body := `{"signature":"transfer(address,uint256)","args":["0x000000000000000000000000000000000000dEaD","1000000000000000000"]}`
	c, rec := jsonRequest(http.MethodPost, "/tools/encode-call", body)
	h.EncodeCall(c)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", rec.Code, rec.Body.String())
	}
	var resp map[string]string
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	// selector(4 bytes) + address(32) + amount(32) = 68 bytes = 136 hex chars + "0x".
	if len(resp["calldata"]) != 138 {
		t.Errorf("calldata length = %d, want 138: %s", len(resp["calldata"]), resp["calldata"])
	}
	if !strings.HasPrefix(resp["calldata"], "0xa9059cbb") {
		t.Errorf("calldata = %s, want it to start with the transfer selector", resp["calldata"])
	}
	// 1e18 in the last 32-byte word.
	if !strings.HasSuffix(resp["calldata"], "0000000000000000000000000000000000000000000000000de0b6b3a7640000") {
		t.Errorf("amount word is wrong: %s", resp["calldata"])
	}
}

func TestEncodeCallRejectsWrongArgumentCount(t *testing.T) {
	h := &Handler{}
	c, rec := jsonRequest(http.MethodPost, "/tools/encode-call",
		`{"signature":"transfer(address,uint256)","args":["0x000000000000000000000000000000000000dEaD"]}`)
	h.EncodeCall(c)
	if rec.Code != http.StatusBadRequest {
		t.Errorf("status = %d, want 400", rec.Code)
	}
}

// A uint256 larger than a JSON number can hold has to survive as a string.
func TestEncodeCallHandlesLargeUint256(t *testing.T) {
	h := &Handler{}
	body := `{"signature":"transfer(address,uint256)","args":["0x000000000000000000000000000000000000dEaD","115792089237316195423570985008687907853269984665640564039457584007913129639935"]}`
	c, rec := jsonRequest(http.MethodPost, "/tools/encode-call", body)
	h.EncodeCall(c)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", rec.Code, rec.Body.String())
	}
	var resp map[string]string
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	if !strings.HasSuffix(resp["calldata"], strings.Repeat("f", 64)) {
		t.Errorf("max uint256 did not encode as all-f: %s", resp["calldata"])
	}
}

// Values that do not fit the declared width must be rejected rather than
// silently truncated, which would encode a different call.
func TestEncodeCallRejectsOutOfRangeInteger(t *testing.T) {
	h := &Handler{}
	body := `{"signature":"foo(uint8)","args":["300"]}`
	c, rec := jsonRequest(http.MethodPost, "/tools/encode-call", body)
	h.EncodeCall(c)
	if rec.Code != http.StatusBadRequest {
		t.Errorf("status = %d, want 400 for uint8 holding 300", rec.Code)
	}
}

func TestEncodeEventTopicEndpoint(t *testing.T) {
	h := &Handler{}
	c, rec := jsonRequest(http.MethodPost, "/tools/encode-event-topic",
		`{"signature":"Transfer(address,address,uint256)"}`)
	h.EncodeEventTopic(c)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", rec.Code, rec.Body.String())
	}
	var resp map[string]string
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	if resp["topic0"] != "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef" {
		t.Errorf("topic0 = %s", resp["topic0"])
	}
}

func TestGetSignatureRejectsMalformedSelector(t *testing.T) {
	h := &Handler{}
	for _, bad := range []string{"0x1234", "zzzzzzzz", "0x1234567890"} {
		c, rec := jsonRequest(http.MethodGet, "/tools/signature/"+bad, "")
		c.Params = gin.Params{{Key: "selector", Value: bad}}
		h.GetSignature(c)
		if rec.Code != http.StatusBadRequest {
			t.Errorf("selector %q: status = %d, want 400", bad, rec.Code)
		}
	}
}
