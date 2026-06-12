// Input Validation Module for TigerScan
// Comprehensive input validation with sanitization

package security

import (
	"crypto/subtle"
	"encoding/hex"
	"fmt"
	"net"
	"net/url"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// =============================================================================
// VALIDATOR
// =============================================================================

// Validator provides comprehensive input validation
type Validator struct {
	// Address validation
	addressRegex *regexp.Regexp
	
	// Transaction hash
	txHashRegex *regexp.Regexp
	
	// Block hash
	blockHashRegex *regexp.Regexp
	
	// Block number
	blockNumberRegex *regexp.Regexp
	
	// ENS domain
	ensRegex *regexp.Regexp
	
	// Token ID
	tokenIDRegex *regexp.Regexp
	
	// Contract address
	contractRegex *regexp.Regexp
	
	// Signature
	signatureRegex *regexp.Regexp
	
	// Email
	emailRegex *regexp.Regexp
	
	// URL
	urlRegex *regexp.Regexp
	
	// Safe string (no dangerous chars)
	safeStringRegex *regexp.Regexp
	
	// HTML (allowed tags)
	allowedTags []string
	
	// Dangerous patterns for path traversal
	pathTraversalPatterns []string
	
	// Dangerous SQL keywords
	sqlKeywords []string
	
	// Dangerous commands
	commandPatterns []string
}

// NewValidator creates a new validator
func NewValidator() *Validator {
	return &Validator{
		addressRegex:       regexp.MustCompile("^0x[a-fA-F0-9]{40}$"),
		txHashRegex:        regexp.MustCompile("^0x[a-fA-F0-9]{64}$"),
		blockHashRegex:    regexp.MustCompile("^0x[a-fA-F0-9]{64}$"),
		blockNumberRegex:  regexp.MustCompile("^\\d+$"),
		ensRegex:          regexp.MustCompile("^[a-zA-Z0-9][a-zA-Z0-9-]{0,62}\\.[a-zA-Z0-9-]{0,63}\\.(eth|xyz|luxe)$"),
		tokenIDRegex:      regexp.MustCompile("^\\d+$|^0x[a-fA-F0-9]+$"),
		contractRegex:    regexp.MustCompile("^0x[a-fA-F0-9]{40}$"),
		signatureRegex:   regexp.MustCompile("^0x[a-fA-F0-9]{130}$|^[a-fA-F0-9]{130}$"),
		emailRegex:        regexp.MustCompile("^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$"),
		urlRegex:          regexp.MustCompile("^https?://[a-zA-Z0-9.-]+(/[a-zA-Z0-9./_-]*)?$"),
		safeStringRegex:    regexp.MustCompile("^[a-zA-Z0-9_.-]+$"),
		allowedTags:       []string{"p", "br", "b", "i", "strong", "em", "code", "pre"},
		pathTraversalPatterns: []string{"../", "..\\", "%2e%2e", "%252e"},
		sqlKeywords:      []string{"UNION", "SELECT", "DROP", "DELETE", "INSERT", "UPDATE", "EXEC", "EXECUTE", "XP_"},
		commandPatterns:  []string{"rm ", "ls ", "cat ", "wget ", "curl ", "sh ", "bash "},
	}
}

// =============================================================================
// VALIDATION METHODS
// =============================================================================

// ValidateAddress validates an Ethereum address
func (v *Validator) ValidateAddress(addr string) bool {
	if addr == "" || len(addr) != 42 {
		return false
	}
	return v.addressRegex.MatchString(addr)
}

// ValidateTxHash validates a transaction hash
func (v *Validator) ValidateTxHash(hash string) bool {
	if hash == "" || len(hash) != 66 {
		return false
	}
	return v.txHashRegex.MatchString(hash)
}

// ValidateBlockHash validates a block hash
func (v *Validator) ValidateBlockHash(hash string) bool {
	if hash == "" || len(hash) != 66 {
		return false
	}
	return v.blockHashRegex.MatchString(hash)
}

// ValidateBlockNumber validates a block number
func (v *Validator) ValidateBlockNumber(num string) bool {
	if num == "" {
		return false
	}
	if !v.blockNumberRegex.MatchString(num) {
		return false
	}
	n, err := strconv.ParseUint(num, 10, 64)
	if err != nil {
		return false
	}
	// Reasonable block number range
	if n > 200000000 {
		return false
	}
	return true
}

// ValidateENS validates an ENS domain
func (v *Validator) ValidateENS(ens string) bool {
	if ens == "" || len(ens) > 100 {
		return false
	}
	return v.ensRegex.MatchString(ens)
}

// ValidateTokenID validates a token ID
func (v *Validator) ValidateTokenID(tokenID string) bool {
	if tokenID == "" {
		return false
	}
	return v.tokenIDRegex.MatchString(tokenID)
}

// ValidateContractAddress validates a contract address
func (v *Validator) ValidateContractAddress(addr string) bool {
	return v.ValidateAddress(addr)
}

// ValidateSignature validates an ECDSA signature
func (v *Validator) ValidateSignature(sig string) bool {
	if sig == "" {
		return false
	}
	return v.signatureRegex.MatchString(sig)
}

// ValidateEmail validates an email address
func (v *Validator) ValidateEmail(email string) bool {
	if email == "" || len(email) > 254 {
		return false
	}
	return v.emailRegex.MatchString(email)
}

// ValidateURL validates a URL
func (v *Validator) ValidateURL(rawURL string) bool {
	if rawURL == "" || len(rawURL) > 2048 {
		return false
	}
	
	parsed, err := url.Parse(rawURL)
	if err != nil {
		return false
	}
	
	// Only allow http and https
	if parsed.Scheme != "http" && parsed.Scheme != "https" {
		return false
	}
	
	// Must have host
	if parsed.Host == "" {
		return false
	}
	
	// Check for dangerous schemes
	if parsed.Scheme == "javascript" || parsed.Scheme == "data" || parsed.Scheme == "vbscript" {
		return false
	}
	
	return true
}

// ValidateSafeString validates a safe string (alphanumeric, dots, hyphens, underscores)
func (v *Validator) ValidateSafeString(s string) bool {
	if s == "" || len(s) > 256 {
		return false
	}
	return v.safeStringRegex.MatchString(s)
}

// =============================================================================
// SANITIZATION METHODS
// =============================================================================

// SanitizeAddress sanitizes an Ethereum address (adds 0x prefix if missing)
func (v *Validator) SanitizeAddress(addr string) string {
	addr = strings.ToLower(strings.TrimSpace(addr))
	if !strings.HasPrefix(addr, "0x") {
		addr = "0x" + addr
	}
	return addr
}

// SanitizeHex sanitizes hex string
func (v *Validator) SanitizeHex(hexStr string) string {
	hexStr = strings.TrimSpace(hexStr)
	hexStr = strings.ToLower(hexStr)
	
	// Remove 0x prefix if present
	hexStr = strings.TrimPrefix(hexStr, "0x")
	
	// Validate hex characters
	if !regexp.MustCompile("^[a-fA-F0-9]+$").MatchString(hexStr) {
		return ""
	}
	
	return hexStr
}

// SanitizeURL sanitizes a URL
func (v *Validator) SanitizeURL(rawURL string) string {
	parsed, err := url.Parse(rawURL)
	if err != nil {
		return ""
	}
	
	// Remove fragment
	parsed.Fragment = ""
	
	// Only allow http/https
	if parsed.Scheme != "http" && parsed.Scheme != "https" {
		return ""
	}
	
	// Remove userinfo
	parsed.User = nil
	
	return parsed.String()
}

// SanitizeHTML removes dangerous HTML
func (v *Validator) SanitizeHTML(html string) string {
	// Remove script tags
	scriptRegex := regexp.MustCompile(`(?i)<script[^>]*>.*?</script>`)
	html = scriptRegex.ReplaceAllString(html, "")
	
	// Remove object tags
	objectRegex := regexp.MustCompile(`(?i)<object[^>]*>.*?</object>`)
	html = objectRegex.ReplaceAllString(html, "")
	
	// Remove iframe
	iframeRegex := regexp.MustCompile(`(?i)<iframe[^>]*>.*?</iframe>`)
	html = iframeRegex.ReplaceAllString(html, "")
	
	// Remove on* event handlers
	eventRegex := regexp.MustCompile(`(?i)\s+on[a-z]+="[^"]*"`)
	html = eventRegex.ReplaceAllString(html, "")
	
	// Remove javascript: URLs
	jsRegex := regexp.MustCompile(`(?i)javascript:`)
	html = jsRegex.ReplaceAllString(html, "")
	
	// Remove data: URLs
	dataRegex := regexp.MustCompile(`(?i)data:`)
	html = dataRegex.ReplaceAllString(html, "")
	
	// Remove eval()
	evalRegex := regexp.MustCompile(`(?i)eval\(`)
	html = evalRegex.ReplaceAllString(html, "")
	
	return html
}

// SanitizeSQL removes SQL injection patterns
func (v *Validator) SanitizeSQL(input string) string {
	result := input
	
	for _, keyword := range v.sqlKeywords {
		// Replace keyword with placeholder
		regex := regexp.MustCompile(fmt.Sprintf(`(?i)\b%s\b`, regexp.QuoteMeta(keyword)))
		result = regex.ReplaceAllString(result, "")
	}
	
	return result
}

// SanitizePath prevents path traversal
func (v *Validator) SanitizePath(path string) string {
	// Remove path traversal patterns
	for _, pattern := range v.pathTraversalPatterns {
		path = strings.ReplaceAll(path, pattern, "")
	}
	
	// Remove null bytes
	path = strings.ReplaceAll(path, "\x00", "")
	
	// Normalize path
	path = strings.ReplaceAll(path, "\\", "/")
	
	// Remove leading slashes
	for strings.HasPrefix(path, "/") {
		path = path[1:]
	}
	
	// Block absolute paths
	if strings.HasPrefix(path, "/") || strings.Contains(path, "..") {
		return ""
	}
	
	return path
}

// =============================================================================
// SPECIALIZED VALIDATORS
// =============================================================================

// ValidateTokenTransfer validates a token transfer request
func (v *Validator) ValidateTokenTransfer(from, to, value, token string) error {
	if !v.ValidateAddress(from) {
		return fmt.Errorf("invalid from address")
	}
	if !v.ValidateAddress(to) {
		return fmt.Errorf("invalid to address")
	}
	if !v.ValidateAddress(token) {
		return fmt.Errorf("invalid token address")
	}
	if value == "" {
		return fmt.Errorf("invalid value")
	}
	return nil
}

// ValidateNFTTransfer validates an NFT transfer request
func (v *Validator) ValidateNFTTransfer(from, to, token, tokenID string) error {
	if !v.ValidateAddress(from) {
		return fmt.Errorf("invalid from address")
	}
	if !v.ValidateAddress(to) {
		return fmt.Errorf("invalid to address")
	}
	if !v.ValidateAddress(token) {
		return fmt.Errorf("invalid token address")
	}
	if !v.ValidateTokenID(tokenID) {
		return fmt.Errorf("invalid token ID")
	}
	return nil
}

// ValidateContractCall validates a contract call
func (v *Validator) ValidateContractCall(to, data string) error {
	if !v.ValidateAddress(to) {
		return fmt.Errorf("invalid contract address")
	}
	if data == "" {
		return fmt.Errorf("invalid data")
	}
	if !strings.HasPrefix(data, "0x") {
		return fmt.Errorf("data must start with 0x")
	}
	// Validate hex
	cleanData := strings.TrimPrefix(data, "0x")
	if !regexp.MustCompile("^[a-fA-F0-9]+$").MatchString(cleanData) {
		return fmt.Errorf("invalid data format")
	}
	// Limit data length (function selector + 100 args max)
	if len(cleanData) > 6400 {
		return fmt.Errorf("data too long")
	}
	return nil
}

// =============================================================================
// CRYPTOGRAPHIC VALIDATION
// =============================================================================

// ValidatePrivateKey validates a private key format
func (v *Validator) ValidatePrivateKey(key string) bool {
	key = strings.TrimSpace(key)
	
	// Must be 64 or 66 characters (with 0x)
	if len(key) != 64 && len(key) != 66 {
		return false
	}
	
	key = strings.TrimPrefix(key, "0x")
	
	// Must be valid hex
	_, err := hex.DecodeString(key)
	return err == nil
}

// ValidatePublicKey validates a public key format
func (v *Validator) ValidatePublicKey(key string) bool {
	key = strings.TrimSpace(key)
	
	// Must be 128 or 130 characters (compressed or with 0x)
	if len(key) != 128 && len(key) != 130 {
		return false
	}
	
	key = strings.TrimPrefix(key, "0x")
	
	// Must be valid hex
	_, err := hex.DecodeString(key)
	return err == nil
}

// =============================================================================
// CONSTANT-TIME COMPARISON
// =============================================================================

// ConstantTimeCompare performs constant-time comparison
func ConstantTimeCompare(a, b string) bool {
	return subtle.ConstantTimeCompare([]byte(a), []byte(b)) == 1
}

// ConstantTimeSelect selects based on condition in constant time
func ConstantTimeSelect(condition int, a, b []byte) []byte {
	return subtle.SecureSlice([]byte(a), []byte(b), condition)
}