package gateway

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"reflect"
	"regexp"
	"strings"

	"github.com/ethereum/go-ethereum/accounts/abi"
	"github.com/ethereum/go-ethereum/common"
	"github.com/gin-gonic/gin"
)

// Function selectors are the first four bytes of keccak256 over the canonical
// signature. They are computed here, never written down. The tools page used to
// carry a hand-written table that mapped 0x2e1a7d4d to "burn(uint256)" when
// that selector is withdraw(uint256); burn(uint256) is 0x42966c68. A wrong
// selector silently sends a different call than the user asked for, so the
// mapping has to come from the hash.
//
// Event topics are derived the same way and exposed through the same endpoint,
// because the two are routinely confused and an event topic is not a selector.

// canonicalFunctions are the standard function signatures the explorer offers
// out of the box. Only the human-readable text is listed; every selector is
// derived from it, so the two cannot drift apart.
var canonicalFunctions = []string{
	// ERC-20
	"transfer(address,uint256)",
	"transferFrom(address,address,uint256)",
	"approve(address,uint256)",
	"allowance(address,address)",
	"balanceOf(address)",
	"totalSupply()",
	"name()",
	"symbol()",
	"decimals()",
	"mint(address,uint256)",
	"burn(uint256)",
	"burnFrom(address,uint256)",
	// EIP-2612 permit
	"permit(address,address,uint256,uint256,uint8,bytes32,bytes32)",
	"nonces(address)",
	"DOMAIN_SEPARATOR()",
	// ERC-721
	"safeTransferFrom(address,address,uint256)",
	"safeTransferFrom(address,address,uint256,bytes)",
	"ownerOf(uint256)",
	"getApproved(uint256)",
	"setApprovalForAll(address,bool)",
	"isApprovedForAll(address,address)",
	"tokenURI(uint256)",
	// ERC-1155
	"safeBatchTransferFrom(address,address,uint256[],uint256[],bytes)",
	"balanceOfBatch(address[],uint256[])",
	"uri(uint256)",
	// Wrapped native
	"deposit()",
	"withdraw(uint256)",
	// Multicall / proxy
	"multicall(bytes[])",
	"aggregate((address,bytes)[])",
	"initialize(address)",
	"upgradeTo(address)",
	"upgradeToAndCall(address,bytes)",
	"implementation()",
	// Ownership / access control / pause
	"owner()",
	"transferOwnership(address)",
	"pause()",
	"unpause()",
	"paused()",
	"grantRole(bytes32,address)",
	"revokeRole(bytes32,address)",
	"hasRole(bytes32,address)",
	// Staking shapes common on this chain
	"stake(uint256)",
	"unstake(uint256)",
	"claimRewards()",
	"getReward()",
	"earned(address)",
}

// canonicalEvents are the standard event signatures, used for topic0 lookups.
var canonicalEvents = []string{
	"Transfer(address,address,uint256)",
	"Approval(address,address,uint256)",
	"TransferSingle(address,address,address,uint256,uint256)",
	"TransferBatch(address,address,address,uint256[],uint256[])",
	"ApprovalForAll(address,address,bool)",
	"OwnershipTransferred(address,address)",
	"Deposit(address,uint256)",
	"Withdrawal(address,uint256)",
	"Staked(address,uint256)",
	"Unstaked(address,uint256)",
	"RewardPaid(address,uint256)",
	"RoleGranted(bytes32,address,address)",
	"RoleRevoked(bytes32,address,address)",
	"Paused(address)",
	"Unpaused(address)",
	"Initialized(uint8)",
	"Upgraded(address)",
}

// NormaliseSignature reduces a signature to its canonical form: no parameter
// names, no whitespace, and no alias types.
//
// All three reductions are required. keccak256 is over the exact text, so
// hashing "function transfer(address to, uint256 amount)" would produce a
// selector that no contract implements; the name has to go, and uint has to
// become uint256, before the text is hashed.
func NormaliseSignature(signature string) string {
	s := strings.TrimSpace(signature)
	for _, prefix := range []string{"function ", "event ", "error "} {
		s = strings.TrimPrefix(s, prefix)
	}

	// Drop parameter names. A name is an identifier sitting between whitespace
	// and either a comma or the closing paren. This has to happen before the
	// whitespace is removed, otherwise "address to" would collapse into the
	// single token "addressto".
	s = parameterNamePattern.ReplaceAllString(s, "$1")

	// Now whitespace is only formatting.
	s = strings.Join(strings.Fields(s), "")

	// Resolve the aliases the ABI parser does not know about.
	s = uintAliasPattern.ReplaceAllString(s, "uint256")
	s = intAliasPattern.ReplaceAllString(s, "int256")
	return s
}

var (
	// Matches " to," / " amount)" and keeps the delimiter.
	parameterNamePattern = regexp.MustCompile(`\s+[A-Za-z_][A-Za-z0-9_]*(\s*[,)])`)
	// \b keeps uint256 from matching.
	uintAliasPattern = regexp.MustCompile(`\buint\b`)
	intAliasPattern  = regexp.MustCompile(`\bint\b`)
)

// signatureStructure accepts only the shapes ParseSelector can handle. The
// upstream parser indexes into its input without bounds checks in places, so
// feeding it arbitrary user text can panic; the shape is therefore checked
// first, and a recover below catches anything the check misses.
var signatureStructure = regexp.MustCompile(`^[A-Za-z_][A-Za-z0-9_]*\([A-Za-z0-9_\[\],()]*\)$`)

// parseSignature canonicalises a signature through the ABI parser and returns
// the canonical text plus the 32-byte keccak256 digest.
//
// Canonicalisation matters because keccak256 is over the exact text: a caller
// who types foo(uint) must get the same answer as foo(uint256). Hashing the raw
// input would produce a selector that no contract implements.
func parseSignature(signature string, isEvent bool) (canonical string, id []byte, err error) {
	sig := NormaliseSignature(signature)
	if sig == "" {
		return "", nil, fmt.Errorf("signature is empty")
	}
	if !signatureStructure.MatchString(sig) {
		return "", nil, fmt.Errorf("%q is not a signature of the form name(type,type)", sig)
	}

	// abi.ParseSelector is documented to return an error, but it indexes into
	// its input in several places and panics on some malformed strings. This
	// input is user-supplied, so the panic is contained rather than allowed to
	// take down the request.
	defer func() {
		if r := recover(); r != nil {
			canonical, id, err = "", nil, fmt.Errorf("%q is not a valid signature", sig)
		}
	}()

	parsed, err := abi.ParseSelector(sig)
	if err != nil {
		return "", nil, fmt.Errorf("%q is not a valid signature: %w", sig, err)
	}
	if !isIdentifierName(parsed.Name) {
		return "", nil, fmt.Errorf("%q is not a valid name", parsed.Name)
	}

	kind := "function"
	if isEvent {
		kind = "event"
	}
	definition, err := json.Marshal([]map[string]interface{}{{
		"type":   kind,
		"name":   parsed.Name,
		"inputs": parsed.Inputs,
	}})
	if err != nil {
		return "", nil, err
	}

	contract, err := abi.JSON(strings.NewReader(string(definition)))
	if err != nil {
		return "", nil, fmt.Errorf("%q is not a valid %s signature: %w", sig, kind, err)
	}

	if isEvent {
		ev, ok := contract.Events[parsed.Name]
		if !ok {
			return "", nil, fmt.Errorf("%q is not a valid event signature", sig)
		}
		return ev.Sig, ev.ID.Bytes(), nil
	}

	method, ok := contract.Methods[parsed.Name]
	if !ok {
		return "", nil, fmt.Errorf("%q is not a valid function signature", sig)
	}
	return method.Sig, method.ID, nil
}

// MethodFor returns the parsed method for a signature, canonicalised.
func MethodFor(signature string) (*abi.Method, error) {
	canonical, _, err := parseSignature(signature, false)
	if err != nil {
		return nil, err
	}
	parsed, err := abi.ParseSelector(canonical)
	if err != nil {
		return nil, err
	}
	definition, err := json.Marshal([]map[string]interface{}{{
		"type":   "function",
		"name":   parsed.Name,
		"inputs": parsed.Inputs,
	}})
	if err != nil {
		return nil, err
	}
	contract, err := abi.JSON(strings.NewReader(string(definition)))
	if err != nil {
		return nil, err
	}
	method, ok := contract.Methods[parsed.Name]
	if !ok {
		return nil, fmt.Errorf("%q is not a valid function signature", signature)
	}
	return &method, nil
}

// SelectorFor returns the 0x-prefixed 4-byte selector for a function signature.
func SelectorFor(signature string) (string, error) {
	_, id, err := parseSignature(signature, false)
	if err != nil {
		return "", err
	}
	return hex.EncodeToString(id[:4]), nil
}

// TopicFor returns the 0x-prefixed 32-byte topic0 for an event signature.
func TopicFor(signature string) (string, error) {
	_, id, err := parseSignature(signature, true)
	if err != nil {
		return "", err
	}
	return "0x" + hex.EncodeToString(id), nil
}

func isIdentifierName(s string) bool {
	if s == "" {
		return false
	}
	for i, r := range s {
		switch {
		case r == '_' || (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z'):
		case i > 0 && r >= '0' && r <= '9':
		default:
			return false
		}
	}
	return true
}

// GetSignature resolves a 4-byte selector to a canonical function signature.
//
// The lookup is two-stage: rows the indexer has learned from verified contract
// ABIs first, then the built-in list whose selectors are computed on demand.
// A miss is a 404 that says so, rather than the page's old behaviour of
// printing "Unknown function signature" for both real misses and malformed
// input.
func (h *Handler) GetSignature(c *gin.Context) {
	raw := strings.TrimSpace(c.Param("selector"))
	selector := strings.ToLower(strings.TrimPrefix(strings.TrimPrefix(raw, "0x"), "0X"))
	if len(selector) != 8 || !isHexString(selector) {
		c.JSON(http.StatusBadRequest, gin.H{
			"error": "a function selector is 4 bytes, e.g. 0xa9059cbb",
		})
		return
	}

	ctx := c.Request.Context()
	row, err := h.queryOne(ctx,
		`SELECT selector, signature, source FROM function_signatures WHERE selector = $1 LIMIT 1`,
		"0x"+selector)
	if err != nil {
		dbError(c, err)
		return
	}
	if row != nil {
		c.JSON(http.StatusOK, gin.H{
			"selector":  "0x" + selector,
			"signature": rowValue(row, "signature"),
			"source":    rowValue(row, "source"),
		})
		return
	}

	for _, sig := range canonicalFunctions {
		got, err := SelectorFor(sig)
		if err == nil && got == selector {
			c.JSON(http.StatusOK, gin.H{
				"selector":  "0x" + selector,
				"signature": sig,
				"source":    "canonical",
			})
			return
		}
	}

	c.JSON(http.StatusNotFound, gin.H{
		"selector": "0x" + selector,
		"error":    "selector is not in the indexed signature set",
	})
}

// ComputeSignature is the direction that is always answerable: it turns
// signatures into selectors or event topics. The tools page uses it as the
// primary lookup because a selector cannot be reversed in general.
func (h *Handler) ComputeSignature(c *gin.Context) {
	var req struct {
		Signature string `json:"signature"`
		Kind      string `json:"kind"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	sig := NormaliseSignature(req.Signature)
	if sig == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "signature is empty"})
		return
	}

	switch req.Kind {
	case "event":
		topic, err := TopicFor(sig)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		c.JSON(http.StatusOK, gin.H{"signature": sig, "kind": "event", "topic": topic})
	default:
		selector, err := SelectorFor(sig)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		c.JSON(http.StatusOK, gin.H{"signature": sig, "kind": "function", "selector": "0x" + selector})
	}
}

// EncodeCall ABI-encodes a call: the keccak selector followed by the packed
// arguments. The signature text is the type source, so nothing about the
// encoding is guessed. The old page printed the literal string "Encoded data
// would appear here..." and never encoded anything.
func (h *Handler) EncodeCall(c *gin.Context) {
	var req struct {
		Signature string            `json:"signature"`
		Args      []json.RawMessage `json:"args"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	method, err := MethodFor(req.Signature)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	sig := method.Sig
	if len(req.Args) != len(method.Inputs) {
		c.JSON(http.StatusBadRequest, gin.H{
			"error": fmt.Sprintf("%s takes %d argument(s), %d given", sig, len(method.Inputs), len(req.Args)),
		})
		return
	}

	values := make([]interface{}, len(req.Args))
	for i, raw := range req.Args {
		v, err := buildArgument(method.Inputs[i].Type, raw)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{
				"error": fmt.Sprintf("argument %d (%s): %v", i, method.Inputs[i].Type.String(), err),
			})
			return
		}
		values[i] = v
	}

	packed, err := method.Inputs.Pack(values...)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	calldata := append(append([]byte{}, method.ID...), packed...)

	c.JSON(http.StatusOK, gin.H{
		"signature": sig,
		"selector":  "0x" + hex.EncodeToString(method.ID),
		"calldata":  "0x" + hex.EncodeToString(calldata),
	})
}

// EncodeEventTopic0 hashes an event signature to its topic0. Kept separate from
// EncodeCall because a topic is 32 bytes and a selector is 4; treating one as
// the other is the mistake that put the wrong topic into the indexer.
func (h *Handler) EncodeEventTopic(c *gin.Context) {
	var req struct {
		Signature string `json:"signature"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	topic, err := TopicFor(req.Signature)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{
		"signature": NormaliseSignature(req.Signature),
		"topic0":    topic,
	})
}

// buildArgument converts one JSON argument into the Go value the ABI packer
// expects for the given type. Numbers may be given as JSON numbers or as
// decimal/hex strings, because a uint256 routinely exceeds what a JSON number
// can carry without losing precision.
func buildArgument(t abi.Type, raw json.RawMessage) (interface{}, error) {
	trimmed := strings.TrimSpace(string(raw))
	if trimmed == "" || trimmed == "null" {
		return nil, fmt.Errorf("missing value")
	}

	switch t.T {
	case abi.AddressTy:
		s, err := jsonString(raw)
		if err != nil {
			return nil, err
		}
		if !common.IsHexAddress(s) {
			return nil, fmt.Errorf("%q is not a 20-byte hex address", s)
		}
		return common.HexToAddress(s), nil

	case abi.BoolTy:
		var b bool
		if err := json.Unmarshal(raw, &b); err == nil {
			return b, nil
		}
		s, err := jsonString(raw)
		if err != nil {
			return nil, err
		}
		switch strings.ToLower(s) {
		case "true", "1":
			return true, nil
		case "false", "0":
			return false, nil
		}
		return nil, fmt.Errorf("%q is not a boolean", s)

	case abi.StringTy:
		var s string
		if err := json.Unmarshal(raw, &s); err != nil {
			return nil, fmt.Errorf("expected a string")
		}
		return s, nil

	case abi.BytesTy:
		s, err := jsonString(raw)
		if err != nil {
			return nil, err
		}
		return decodeHexBytes(s)

	case abi.FixedBytesTy:
		s, err := jsonString(raw)
		if err != nil {
			return nil, err
		}
		b, err := decodeHexBytes(s)
		if err != nil {
			return nil, err
		}
		if len(b) != t.Size {
			return nil, fmt.Errorf("bytes%d needs exactly %d bytes, got %d", t.Size, t.Size, len(b))
		}
		out := reflect.New(t.GetType()).Elem()
		reflect.Copy(out, reflect.ValueOf(b))
		return out.Interface(), nil

	case abi.IntTy, abi.UintTy:
		return buildInteger(t, trimmed)

	case abi.SliceTy, abi.ArrayTy:
		return buildSequence(t, raw)

	case abi.TupleTy:
		return buildTuple(t, raw)

	default:
		return nil, fmt.Errorf("type %s cannot be encoded", t.String())
	}
}

// buildInteger parses an integer of the declared width. Values are read as
// decimal or 0x-hex, and the width is enforced: a uint8 that receives 300 is
// rejected rather than silently truncated, which would encode a different call.
func buildInteger(t abi.Type, raw string) (interface{}, error) {
	s := strings.TrimSpace(raw)
	s = strings.Trim(s, `"`)
	n, ok := new(big.Int).SetString(s, 0)
	if !ok {
		return nil, fmt.Errorf("%q is not an integer (use decimal or 0x-prefixed hex)", s)
	}
	if t.T == abi.UintTy && n.Sign() < 0 {
		return nil, fmt.Errorf("%s cannot hold a negative value", t.String())
	}

	// BitLen is the magnitude; a signed type also needs one bit for the sign.
	limit := t.Size
	bits := n.BitLen()
	if t.T == abi.IntTy {
		if bits > limit-1 {
			return nil, fmt.Errorf("%s cannot hold %s", t.String(), s)
		}
	} else if bits > limit {
		return nil, fmt.Errorf("%s cannot hold %s", t.String(), s)
	}

	if t.Size == 256 {
		return n, nil
	}
	if t.T == abi.UintTy {
		return n.Uint64(), nil
	}
	return n.Int64(), nil
}

// buildSequence handles fixed and dynamic arrays, including nested ones.
func buildSequence(t abi.Type, raw json.RawMessage) (interface{}, error) {
	var items []json.RawMessage
	if err := json.Unmarshal(raw, &items); err != nil {
		return nil, fmt.Errorf("expected a JSON array of %s", t.Elem.String())
	}
	if t.T == abi.ArrayTy && len(items) != t.Size {
		return nil, fmt.Errorf("%s needs exactly %d element(s), got %d", t.String(), t.Size, len(items))
	}

	slice := reflect.MakeSlice(reflect.SliceOf(t.Elem.GetType()), len(items), len(items))
	for i, item := range items {
		v, err := buildArgument(*t.Elem, item)
		if err != nil {
			return nil, fmt.Errorf("element %d: %w", i, err)
		}
		rv := reflect.ValueOf(v)
		if rv.IsValid() {
			slice.Index(i).Set(rv)
		}
	}

	if t.T == abi.ArrayTy {
		arr := reflect.New(reflect.ArrayOf(t.Size, t.Elem.GetType())).Elem()
		reflect.Copy(arr, slice)
		return arr.Interface(), nil
	}
	return slice.Interface(), nil
}

// buildTuple fills a tuple's generated struct from a JSON object, or from a
// JSON array for callers that would rather pass positional values.
func buildTuple(t abi.Type, raw json.RawMessage) (interface{}, error) {
	out := reflect.New(t.TupleType).Elem()
	fields := t.TupleElems

	asObject := map[string]json.RawMessage{}
	if err := json.Unmarshal(raw, &asObject); err == nil && len(asObject) > 0 {
		for i, elem := range fields {
			name := t.TupleRawNames[i]
			value, ok := asObject[name]
			if !ok {
				// Fall back to a case-insensitive match so {"To": ...} works
				// for a component declared as "to".
				for k, v := range asObject {
					if strings.EqualFold(k, name) {
						value, ok = v, true
						break
					}
				}
			}
			if !ok {
				return nil, fmt.Errorf("tuple is missing component %q", name)
			}
			built, err := buildArgument(*elem, value)
			if err != nil {
				return nil, fmt.Errorf("component %q: %w", name, err)
			}
			out.Field(i).Set(reflect.ValueOf(built))
		}
		return out.Interface(), nil
	}

	var items []json.RawMessage
	if err := json.Unmarshal(raw, &items); err != nil {
		return nil, fmt.Errorf("expected a JSON object keyed by component name, or a JSON array")
	}
	if len(items) != len(fields) {
		return nil, fmt.Errorf("tuple needs %d component(s), got %d", len(fields), len(items))
	}
	for i, elem := range fields {
		built, err := buildArgument(*elem, items[i])
		if err != nil {
			return nil, fmt.Errorf("component %d: %w", i, err)
		}
		out.Field(i).Set(reflect.ValueOf(built))
	}
	return out.Interface(), nil
}

func jsonString(raw json.RawMessage) (string, error) {
	var s string
	if err := json.Unmarshal(raw, &s); err == nil {
		return s, nil
	}
	// Accept a bare token such as 0xabc without quotes.
	s = strings.TrimSpace(string(raw))
	if s == "" {
		return "", fmt.Errorf("expected a string")
	}
	return s, nil
}

func isHexString(s string) bool {
	if s == "" {
		return false
	}
	for _, r := range s {
		switch {
		case r >= '0' && r <= '9', r >= 'a' && r <= 'f', r >= 'A' && r <= 'F':
		default:
			return false
		}
	}
	return true
}

func decodeHexBytes(s string) ([]byte, error) {
	body := strings.TrimPrefix(strings.TrimPrefix(strings.TrimSpace(s), "0x"), "0X")
	if body == "" {
		return []byte{}, nil
	}
	if !isHexString(body) {
		return nil, fmt.Errorf("%q is not hex", s)
	}
	if len(body)%2 != 0 {
		body = "0" + body
	}
	return hex.DecodeString(body)
}

// SeedFunctionSignatures fills the signature table from the canonical lists.
// Every selector is computed, so a row can never disagree with keccak256. Rows
// the indexer learned from verified ABIs are left untouched.
func (h *Handler) SeedFunctionSignatures(ctx context.Context) error {
	if h.pool == nil {
		return fmt.Errorf("no database connection")
	}
	for _, sig := range canonicalFunctions {
		selector, err := SelectorFor(sig)
		if err != nil {
			return fmt.Errorf("canonical signature %q: %w", sig, err)
		}
		if _, err := h.pool.Exec(ctx, `
			INSERT INTO function_signatures (selector, signature, kind, source)
			VALUES ($1, $2, 'function', 'canonical')
			ON CONFLICT (selector) DO NOTHING
		`, "0x"+selector, sig); err != nil {
			return err
		}
	}
	for _, sig := range canonicalEvents {
		topic, err := TopicFor(sig)
		if err != nil {
			return fmt.Errorf("canonical event %q: %w", sig, err)
		}
		if _, err := h.pool.Exec(ctx, `
			INSERT INTO function_signatures (selector, signature, kind, source)
			VALUES ($1, $2, 'event', 'canonical')
			ON CONFLICT (selector) DO NOTHING
		`, topic, sig); err != nil {
			return err
		}
	}
	return nil
}
