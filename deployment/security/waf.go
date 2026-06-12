// WAF (Web Application Firewall) for TigerScan
// Advanced web application firewall with rule-based filtering

package security

import (
	"context"
	"fmt"
	"net/http"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
)

// =============================================================================
// CONFIGURATION
// =============================================================================

// WAFConfig holds WAF configuration
type WAFConfig struct {
	RedisClient  *redis.Client
	KeyPrefix   string
	Rules       []Rule
	LogEnabled  bool
	BlockMode  bool
}

// Rule represents a WAF rule
type Rule struct {
	ID          string
	Name       string
	Pattern    string
	Action     string // block, log, challenge
	Severity   string // low, medium, high, critical
	Category  string // sql_injection, xss, path_traversal, etc
	Enabled   bool
}

// NewWAFConfig creates default WAF configuration
func NewWAFConfig(redisClient *redis.Client) *WAFConfig {
	return &WAFConfig{
		RedisClient: redisClient,
		KeyPrefix:  "tigerscan:waf:",
		LogEnabled: true,
		BlockMode: true,
		Rules: []Rule{
			// SQL Injection
			{ID: "SQL001", Name: "SQL Injection UNION", Pattern: `(?i)(union\s+select|union\s+all\s+select)`, Action: "block", Severity: "critical", Category: "sql_injection", Enabled: true},
			{ID: "SQL002", Name: "SQL Injection OR", Pattern: `(?i)(\bor\b.*\b=\b|\b'\b|\b\"\b)`, Action: "block", Severity: "high", Category: "sql_injection", Enabled: true},
			{ID: "SQL003", Name: "SQL Injection DROP", Pattern: `(?i)(drop\s+table|drop\s+database|delete\s+from)`, Action: "block", Severity: "critical", Category: "sql_injection", Enabled: true},
			{ID: "SQL004", Name: "SQL Injection EXEC", Pattern: `(?i)(exec\s*\(|xp_cmdshell|sp_executesql)`, Action: "block", Severity: "critical", Category: "sql_injection", Enabled: true},
			
			// XSS
			{ID: "XSS001", Name: "XSS Script Tag", Pattern: `(?i)<script[^>]*>.*?</script>`, Action: "block", Severity: "critical", Category: "xss", Enabled: true},
			{ID: "XSS002", Name: "XSS JavaScript", Pattern: `(?i)javascript:`, Action: "block", Severity: "critical", Category: "xss", Enabled: true},
			{ID: "XSS003", Name: "XSS Event Handler", Pattern: `(?i)on\w+\s*=`, Action: "block", Severity: "high", Category: "xss", Enabled: true},
			{ID: "XSS004", Name: "XSS Object Tag", Pattern: `(?i)<object[^>]*>`, Action: "block", Severity: "critical", Category: "xss", Enabled: true},
			{ID: "XSS005", Name: "XSS Iframe", Pattern: `(?i)<iframe[^>]*>`, Action: "block", Severity: "high", Category: "xss", Enabled: true},
			
			// Path Traversal
			{ID: "PT001", Name: "Path Traversal", Pattern: `(?i)(\.\./|\.\.\\|%2e%2e)`, Action: "block", Severity: "high", Category: "path_traversal", Enabled: true},
			{ID: "PT002", Name: "Absolute Path", Pattern: `(?i)/etc/passwd|/windows/system32`, Action: "block", Severity: "high", Category: "path_traversal", Enabled: true},
			
			// Command Injection
			{ID: "CMD001", Name: "Command Injection", Pattern: `(?i)(;\s*rm\s|;\s*ls\s|;\s*cat\s|;\s*wget\s|;\s*curl\s)`, Action: "block", Severity: "critical", Category: "command_injection", Enabled: true},
			{ID: "CMD002", Name: "Pipe Command", Pattern: `(?i)\|\s*\w+`, Action: "block", Severity: "high", Category: "command_injection", Enabled: true},
			{ID: "CMD003", Name: "Backtick", Pattern: "`.*`", Action: "block", Severity: "high", Category: "command_injection", Enabled: true},
			
			// LDAP Injection
			{ID: "LDAP001", Name: "LDAP Injection", Pattern: `(?i)(\*\)|\)\(|\(objectClass=`, Action: "block", Severity: "high", Category: "ldap_injection", Enabled: true},
			
			// XML Injection
			{ID: "XML001", Name: "XML Injection", Pattern: `(?i)<!\[CDATA\[|<!ENTITY`, Action: "block", Severity: "high", Category: "xml_injection", Enabled: true},
			
			// Generic attacks
			{ID: "GEN001", Name: "Null Byte", Pattern: `\x00`, Action: "block", Severity: "high", Category: "generic", Enabled: true},
			{ID: "GEN002", Name: "Double Encoding", Pattern: `%25`, Action: "log", Severity: "medium", Category: "generic", Enabled: true},
		},
	}
}

// =============================================================================
// WAF
// =============================================================================

// WAF provides web application firewall functionality
type WAF struct {
	config  *WAFConfig
	rules   map[string]*regexp.Regexp
	mu      sync.RWMutex
	logger  *WAFLogger
}

// WAFLogger logs WAF events
type WAFLogger struct {
	redis   *redis.Client
	keyPrefx string
	mu      sync.Mutex
}

// NewWAF creates a new WAF
func NewWAF(config *WAFConfig) *WAF {
	waf := &WAF{
		config: config,
		rules: make(map[string]*regexp.Regexp),
		logger: &WAFLogger{
			redis:   config.RedisClient,
			keyPrefx: config.KeyPrefix + "log:",
		},
	}
	
	// Compile rules
	for _, rule := range config.Rules {
		if rule.Enabled {
			re, err := regexp.Compile(rule.Pattern)
			if err == nil {
				waf.rules[rule.ID] = re
			}
		}
	}
	
	return waf
}

// Middleware returns Gin middleware
func (w *WAF) Middleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		ctx := c.Request.Context()
		
		// Check request
		matched := w.checkRequest(c.Request)
		
		if matched != nil {
			w.logger.log(ctx, matched)
			
			if w.config.BlockMode && matched.Action == "block" {
				c.AbortWithStatusJSON(http.StatusForbidden, gin.H{
					"status":  "error",
					"message": "Request blocked by security policy",
					"rule":   matched.Name,
				})
				return
			}
			
			if matched.Action == "challenge" {
				// Return challenge page
				c.HTML(http.StatusForbidden, "challenge.html", gin.H{
					"reason": matched.Name,
				})
				return
			}
		}
		
		c.Next()
	}
}

// =============================================================================
// CHECK LOGIC
// =============================================================================

func (w *WAF) checkRequest(req *http.Request) *Rule {
	// Check path
	path := req.URL.Path
	
	// Check query string
	query := req.URL.Query().Encode()
	
	// Check POST body (simplified)
	body := req.PostForm.Encode()
	
	// Check headers
	headers := []string{
		req.Header.Get("User-Agent"),
		req.Header.Get("Referer"),
		req.Header.Get("Cookie"),
		req.Header.Get("X-Forwarded-For"),
	}
	
	allContent := path + " " + query + " " + body + " " + strings.Join(headers, " ")
	
	// Test against all rules
	w.mu.RLock()
	defer w.mu.RUnlock()
	
	for _, rule := range w.config.Rules {
		if !rule.Enabled {
			continue
		}
		
		re, exists := w.rules[rule.ID]
		if !exists {
			continue
		}
		
		if re.MatchString(allContent) {
			return &rule
		}
	}
	
	return nil
}

func (l *WAFLogger) log(ctx context.Context, rule *Rule) {
	if l.redis == nil {
		return
	}
	
	key := l.keyPrefx + rule.Category + ":" + time.Now().Format("2006-01-02")
	
	data := fmt.Sprintf(`{"rule_id":"%s","rule_name":"%s","category":"%s","severity":"%s","timestamp":"%s"}`,
		rule.ID, rule.Name, rule.Category, rule.Severity, time.Now().Format(time.RFC3339))
	
	l.mu.Lock()
	defer l.mu.Unlock()
	
	l.redis.LPush(ctx, key, data)
	l.redis.LTrim(ctx, key, 0, 10000)
	l.redis.Expire(ctx, key, 7*24*time.Hour)
}

// =============================================================================
// RULE MANAGEMENT
// =============================================================================

// EnableRule enables a rule
func (w *WAF) EnableRule(ruleID string) {
	w.mu.Lock()
	defer w.mu.Unlock()
	
	for i := range w.config.Rules {
		if w.config.Rules[i].ID == ruleID {
			w.config.Rules[i].Enabled = true
			
			// Recompile
			re, err := regexp.Compile(w.config.Rules[i].Pattern)
			if err == nil {
				w.rules[ruleID] = re
			}
			break
		}
	}
}

// DisableRule disables a rule
func (w *WAF) DisableRule(ruleID string) {
	w.mu.Lock()
	defer w.mu.Unlock()
	
	for i := range w.config.Rules {
		if w.config.Rules[i].ID == ruleID {
			w.config.Rules[i].Enabled = false
			delete(w.rules, ruleID)
			break
		}
	}
}

// GetRules returns all rules
func (w *WAF) GetRules() []Rule {
	return w.config.Rules
}

// GetStats returns WAF statistics
func (w *WAF) GetStats(ctx context.Context) (map[string]int64, error) {
	if w.config.RedisClient == nil {
		return nil, nil
	}
	
	stats := make(map[string]int64)
	
	// Get counts by category
	categories := []string{"sql_injection", "xss", "path_traversal", "command_injection", "ldap_injection", "xml_injection", "generic"}
	
	for _, cat := range categories {
		key := w.config.KeyPrefix + "log:" + cat + ":*"
		keys, err := w.config.RedisClient.Keys(ctx, key).Result()
		if err != nil {
			continue
		}
		
		for _, k := range keys {
			count, _ := w.config.RedisClient.LLen(ctx, k).Result()
			stats[cat] += count
		}
	}
	
	return stats, nil
}