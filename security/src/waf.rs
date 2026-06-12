//! Web Application Firewall (WAF) for TigerScan
//! Rule-based filtering for SQL injection, XSS, path traversal, and more

use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

// =============================================================================
// TYPES
// =============================================================================

/// WAF Rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WafRule {
    pub id: String,
    pub name: String,
    pub pattern: String,
    pub action: WafAction,
    pub severity: WafSeverity,
    pub category: WafCategory,
    pub enabled: bool,
}

impl WafRule {
    /// Create a new rule
    pub fn new(
        id: &str,
        name: &str,
        pattern: &str,
        action: WafAction,
        severity: WafSeverity,
        category: WafCategory,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            pattern: pattern.to_string(),
            action,
            severity,
            category,
            enabled: true,
        }
    }
}

/// WAF Action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WafAction {
    Allow,
    Block,
    Log,
    Challenge,
}

impl Default for WafAction {
    fn default() -> Self {
        Self::Block
    }
}

/// WAF Severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WafSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for WafSeverity {
    fn default() -> Self {
        Self::Medium
    }
}

/// WAF Category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WafCategory {
    SqlInjection,
    Xss,
    PathTraversal,
    CommandInjection,
    LdapInjection,
    XmlInjection,
    Generic,
}

impl Default for WafCategory {
    fn default() -> Self {
        Self::Generic
    }
}

// =============================================================================
// WAF
// =============================================================================

/// Web Application Firewall
pub struct Waf {
    rules: Vec<WafRule>,
    compiled_rules: HashMap<String, Regex>,
    log_enabled: bool,
    block_mode: bool,
    log: RwLock<Vec<WafLog>>,
}

impl Default for Waf {
    fn default() -> Self {
        Self::new()
    }
}

impl Waf {
    /// Create a new WAF
    pub fn new() -> Self {
        let mut waf = Self {
            rules: Vec::new(),
            compiled_rules: HashMap::new(),
            log_enabled: true,
            block_mode: true,
            log: RwLock::new(Vec::new()),
        };
        
        // Add default rules
        waf.add_default_rules();
        
        waf
    }

    /// Add default security rules
    fn add_default_rules(&mut self) {
        let rules = vec![
            // SQL Injection
            WafRule::new(
                "SQL001", "SQL Injection UNION",
                r"(?i)(union\s+select|union\s+all\s+select)",
                WafAction::Block, WafSeverity::Critical, WafCategory::SqlInjection,
            ),
            WafRule::new(
                "SQL002", "SQL Injection OR",
                r"(?i)(\bor\b.*\b=\b|\b'\b|\b\"\b)",
                WafAction::Block, WafSeverity::High, WafCategory::SqlInjection,
            ),
            WafRule::new(
                "SQL003", "SQL Injection DROP",
                r"(?i)(drop\s+table|drop\s+database|delete\s+from)",
                WafAction::Block, WafSeverity::Critical, WafCategory::SqlInjection,
            ),
            WafRule::new(
                "SQL004", "SQL Injection EXEC",
                r"(?i)(exec\s*\(|xp_cmdshell|sp_executesql)",
                WafAction::Block, WafSeverity::Critical, WafCategory::SqlInjection,
            ),
            
            // XSS
            WafRule::new(
                "XSS001", "XSS Script Tag",
                r"(?i)<script[^>]*>.*?</script>",
                WafAction::Block, WafSeverity::Critical, WafCategory::Xss,
            ),
            WafRule::new(
                "XSS002", "XSS JavaScript",
                r"(?i)javascript:",
                WafAction::Block, WafSeverity::Critical, WafCategory::Xss,
            ),
            WafRule::new(
                "XSS003", "XSS Event Handler",
                r"(?i)on\w+\s*=",
                WafAction::Block, WafSeverity::High, WafCategory::Xss,
            ),
            WafRule::new(
                "XSS004", "XSS Object Tag",
                r"(?i)<object[^>]*>",
                WafAction::Block, WafSeverity::Critical, WafCategory::Xss,
            ),
            WafRule::new(
                "XSS005", "XSS Iframe",
                r"(?i)<iframe[^>]*>",
                WafAction::Block, WafSeverity::High, WafCategory::Xss,
            ),
            WafRule::new(
                "XSS006", "XSS Embed",
                r"(?i)<embed[^>]*>",
                WafAction::Block, WafSeverity::High, WafCategory::Xss,
            ),
            
            // Path Traversal
            WafRule::new(
                "PT001", "Path Traversal",
                r"(\.\./|\.\.\\|%2e%2e)",
                WafAction::Block, WafSeverity::High, WafCategory::PathTraversal,
            ),
            WafRule::new(
                "PT002", "Absolute Path",
                r"(?i)/etc/passwd|/windows/system32",
                WafAction::Block, WafSeverity::High, WafCategory::PathTraversal,
            ),
            
            // Command Injection
            WafRule::new(
                "CMD001", "Command Injection",
                r"(?i)(;\s*rm\s|;\s*ls\s|;\s*cat\s|;\s*wget\s|;\s*curl\s)",
                WafAction::Block, WafSeverity::Critical, WafCategory::CommandInjection,
            ),
            WafRule::new(
                "CMD002", "Pipe Command",
                r"(?i)\|\s*\w+",
                WafAction::Block, WafSeverity::High, WafCategory::CommandInjection,
            ),
            WafRule::new(
                "CMD003", "Backtick",
                r"`.*`",
                WafAction::Block, WafSeverity::High, WafCategory::CommandInjection,
            ),
            
            // LDAP Injection
            WafRule::new(
                "LDAP001", "LDAP Injection",
                r"(?i)(\*\)|\)\(|\(objectClass=)",
                WafAction::Block, WafSeverity::High, WafCategory::LdapInjection,
            ),
            
            // XML Injection
            WafRule::new(
                "XML001", "XML Injection",
                r"(?i)<!\[CDATA\[|<!ENTITY",
                WafAction::Block, WafSeverity::High, WafCategory::XmlInjection,
            ),
            
            // Generic
            WafRule::new(
                "GEN001", "Null Byte",
                r"\x00",
                WafAction::Block, WafSeverity::High, WafCategory::Generic,
            ),
            WafRule::new(
                "GEN002", "Double Encoding",
                r"%25",
                WafAction::Log, WafSeverity::Medium, WafCategory::Generic,
            ),
        ];
        
        for rule in rules {
            self.add_rule(rule);
        }
    }

    /// Add a rule
    pub fn add_rule(&mut self, rule: WafRule) {
        if rule.enabled {
            if let Ok(re) = Regex::new(&rule.pattern) {
                self.compiled_rules.insert(rule.id.clone(), re);
            }
        }
        self.rules.push(rule);
    }

    // =============================================================================
    // CHECKING
    // =============================================================================

    /// Check request content
    pub fn check(&self, content: &str) -> Vec<WafMatch> {
        self.check_with_options(content, true)
    }

    /// Check with options
    pub fn check_with_options(&self, content: &str, log_matches: bool) -> Vec<WafMatch> {
        let mut matches = Vec::new();
        
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }
            
            if let Some(re) = self.compiled_rules.get(&rule.id) {
                if re.is_match(content) {
                    matches.push(WafMatch {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        action: rule.action,
                        severity: rule.severity,
                        category: rule.category,
                    });
                    
                    if log_matches && self.log_enabled {
                        self.log_match(&rule, content);
                    }
                }
            }
        }
        
        matches
    }

    /// Check and get action
    pub fn check_request(&self, content: &str) -> WafResult {
        let matches = self.check(content);
        
        if matches.is_empty() {
            return WafResult {
                allowed: true,
                matched_rules: Vec::new(),
            };
        }
        
        // Check if any block
        let has_block = matches.iter().any(|m| m.action == WafAction::Block);
        
        WafResult {
            allowed: !has_block,
            matched_rules: matches,
        }
    }

    // =============================================================================
    // LOGGING
    // =============================================================================

    fn log_match(&self, rule: &WafRule, _content: &str) {
        if let Ok(mut log) = self.log.write() {
            log.push(WafLog {
                timestamp: Utc::now().timestamp(),
                rule_id: rule.id.clone(),
                rule_name: rule.name.clone(),
                category: rule.category,
                severity: rule.severity,
            });
            
            // Keep only last 10000
            if log.len() > 10000 {
                log.drain(0..5000);
            }
        }
    }

    /// Get WAF statistics
    pub fn get_stats(&self) -> WafStats {
        let log = match self.log.read() {
            Ok(l) => l,
            Err(_) => return WafStats::default(),
        };
        
        let mut by_category: HashMap<String, usize> = HashMap::new();
        let mut by_severity: HashMap<String, usize> = HashMap::new();
        
        for entry in &log {
            *by_category.entry(format!("{:?}", entry.category)).or_insert(0) += 1;
            *by_severity.entry(format!("{:?}", entry.severity)).or_insert(0) += 1;
        }
        
        WafStats {
            total_blocks: log.iter().filter(|l| l.severity == WafSeverity::Critical).count(),
            total_logs: log.len(),
            by_category,
            by_severity,
        }
    }

    // =============================================================================
    // RULE MANAGEMENT
    // =============================================================================

    /// Enable a rule
    pub fn enable_rule(&mut self, rule_id: &str) {
        for rule in &mut self.rules {
            if rule.id == rule_id {
                rule.enabled = true;
                if let Ok(re) = Regex::new(&rule.pattern) {
                    self.compiled_rules.insert(rule.id.clone(), re);
                }
                break;
            }
        }
    }

    /// Disable a rule
    pub fn disable_rule(&mut self, rule_id: &str) {
        for rule in &mut self.rules {
            if rule.id == rule_id {
                rule.enabled = false;
                self.compiled_rules.remove(&rule.id);
                break;
            }
        }
    }

    /// Get all rules
    pub fn get_rules(&self) -> Vec<WafRule> {
        self.rules.clone()
    }
}

// =============================================================================
// RESULT TYPES
// =============================================================================

/// WAF match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WafMatch {
    pub rule_id: String,
    pub rule_name: String,
    pub action: WafAction,
    pub severity: WafSeverity,
    pub category: WafCategory,
}

/// WAF result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WafResult {
    pub allowed: bool,
    pub matched_rules: Vec<WafMatch>,
}

/// WAF log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WafLog {
    pub timestamp: i64,
    pub rule_id: String,
    pub rule_name: String,
    pub category: WafCategory,
    pub severity: WafSeverity,
}

/// WAF statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WafStats {
    pub total_blocks: usize,
    pub total_logs: usize,
    pub by_category: HashMap<String, usize>,
    pub by_severity: HashMap<String, usize>,
}

impl Default for WafStats {
    fn default() -> Self {
        Self {
            total_blocks: 0,
            total_logs: 0,
            by_category: HashMap::new(),
            by_severity: HashMap::new(),
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_waf() {
        let waf = Waf::new();
        
        let rules = waf.get_rules();
        
        assert!(rules.len() >= 15);
    }

    #[test]
    fn test_sql_injection() {
        let waf = Waf::new();
        
        let result = waf.check_request("' OR '1'='1");
        
        assert!(!result.allowed);
        assert!(!result.matched_rules.is_empty());
    }

    #[test]
    fn test_xss() {
        let waf = Waf::new();
        
        let result = waf.check_request("<script>alert(1)</script>");
        
        assert!(!result.allowed);
    }

    #[test]
    fn test_path_traversal() {
        let waf = Waf::new();
        
        let result = waf.check_request("../../etc/passwd");
        
        assert!(!result.allowed);
    }

    #[test]
    fn test_command_injection() {
        let waf = Waf::new();
        
        let result = waf.check_request("; rm -rf /");
        
        assert!(!result.allowed);
    }

    #[test]
    fn test_stats() {
        let waf = Waf::new();
        
        // Trigger some blocks
        waf.check_request("<script>");
        waf.check_request("' OR '1'='1");
        
        let stats = waf.get_stats();
        
        assert!(stats.total_logs >= 2);
    }
}