//! DDoS Protection Module for TigerScan
//! Advanced distributed denial of service protection

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

// =============================================================================
// CONSTANTS
// =============================================================================

pub const DEFAULT_BURST_LIMIT: u32 = 100;
pub const DEFAULT_SUSTAINED_LIMIT: u32 = 50;
pub const DEFAULT_BLOCK_DURATION: u64 = 15; // minutes
pub const DEFAULT_ANALYSIS_WINDOW: u64 = 10; // seconds

// =============================================================================
// TYPES
// =============================================================================

/// DDoS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdosConfig {
    pub burst_limit: u32,
    pub sustained_limit: u32,
    pub block_duration: Duration,
    pub analysis_window: Duration,
    pub geo_block_enabled: bool,
    pub allowed_countries: Vec<String>,
    pub blocked_countries: Vec<String>,
}

impl Default for DdosConfig {
    fn default() -> Self {
        Self {
            burst_limit: DEFAULT_BURST_LIMIT,
            sustained_limit: DEFAULT_SUSTAINED_LIMIT,
            block_duration: Duration::from_secs(DEFAULT_BLOCK_DURATION * 60),
            analysis_window: Duration::from_secs(DEFAULT_ANALYSIS_WINDOW),
            geo_block_enabled: false,
            allowed_countries: vec![
                "US".to_string(),
                "GB".to_string(),
                "DE".to_string(),
                "JP".to_string(),
                "SG".to_string(),
                "AU".to_string(),
                "CA".to_string(),
            ],
            blocked_countries: Vec::new(),
        }
    }
}

/// IP statistics
#[derive(Debug, Clone)]
pub struct IpStats {
    pub requests: u32,
    pub first_seen: i64,
    pub last_seen: i64,
    pub user_agents: HashMap<String, u32>,
    pub endpoints: HashMap<String, u32>,
    pub countries: HashMap<String, u32>,
}

impl Default for IpStats {
    fn default() -> Self {
        Self {
            requests: 0,
            first_seen: Utc::now().timestamp(),
            last_seen: Utc::now().timestamp(),
            user_agents: HashMap::new(),
            endpoints: HashMap::new(),
            countries: HashMap::new(),
        }
    }
}

// =============================================================================
// PROTECTOR
// =============================================================================

/// DDoS Protector
pub struct DdosProtector {
    config: DdosConfig,
    ip_stats: RwLock<HashMap<String, IpStats>>,
    blocklist: RwLock<HashMap<String, i64>>,
    attack_log: RwLock<Vec<AttackLog>>,
}

impl Default for DdosProtector {
    fn default() -> Self {
        Self::new()
    }
}

impl DdosProtector {
    /// Create a new DDoS protector
    pub fn new() -> Self {
        Self {
            config: DdosConfig::default(),
            ip_stats: RwLock::new(HashMap::new()),
            blocklist: RwLock::new(HashMap::new()),
            attack_log: RwLock::new(Vec::new()),
        }
    }

    /// Create with custom config
    pub fn with_config(config: DdosConfig) -> Self {
        Self {
            config,
            ip_stats: RwLock::new(HashMap::new()),
            blocklist: RwLock::new(HashMap::new()),
            attack_log: RwLock::new(Vec::new()),
        }
    }

    // =============================================================================
    // PROTECTION
    // =============================================================================

    /// Check if IP is allowed
    pub fn check(&self, client_ip: &str) -> DdosResult {
        self.check_with_headers(client_ip, None, None)
    }

    /// Check with request details
    pub fn check_with_headers(
        &self,
        client_ip: &str,
        user_agent: Option<&str>,
        endpoint: Option<&str>,
    ) -> DdosResult {
        // Check if blocked
        if self.is_blocked(client_ip) {
            return DdosResult {
                allowed: false,
                reason: "blocked".to_string(),
                block_until: self.get_block_expiry(client_ip),
                threat_level: ThreatLevel::Critical,
            };
        }

        // Check rate limit
        if !self.check_rate_limit(client_ip) {
            self.log_attack(client_ip, "rate_limit_exceeded");
            return DdosResult {
                allowed: false,
                reason: "rate_limit_exceeded".to_string(),
                block_until: Some(Utc::now().timestamp() + 60),
                threat_level: ThreatLevel::High,
            };
        }

        // Check for suspicious patterns
        if let Some(ua) = user_agent {
            if self.is_suspicious_user_agent(ua) {
                self.log_attack(client_ip, "suspicious_user_agent");
                return DdosResult {
                    allowed: false,
                    reason: "suspicious_user_agent".to_string(),
                    block_until: None,
                    threat_level: ThreatLevel::Medium,
                };
            }
        }

        // Record request
        self.record_request(client_ip, user_agent, endpoint);

        DdosResult {
            allowed: true,
            reason: String::new(),
            block_until: None,
            threat_level: ThreatLevel::None,
        }
    }

    // =============================================================================
    // RATE LIMITING
    // =============================================================================

    fn check_rate_limit(&self, client_ip: &str) -> bool {
        let stats = match self.ip_stats.read() {
            Ok(s) => s,
            Err(_) => return true,
        };

        let ip_stats = match stats.get(client_ip) {
            Some(s) => s,
            None => return true,
        };

        // Check burst limit
        if ip_stats.requests > self.config.burst_limit {
            return false;
        }

        true
    }

    fn record_request(
        &self,
        client_ip: &str,
        user_agent: Option<&str>,
        endpoint: Option<&str>,
    ) {
        let mut stats = match self.ip_stats.write() {
            Ok(s) => s,
            Err(_) => return,
        };

        let ip_stats = stats
            .entry(client_ip.to_string())
            .or_insert_with(IpStats::default);

        ip_stats.requests += 1;
        ip_stats.last_seen = Utc::now().timestamp();

        if let Some(ua) = user_agent {
            *ip_stats.user_agents.entry(ua.to_string()).or_insert(0) += 1;
        }

        if let Some(ep) = endpoint {
            *ip_stats.endpoints.entry(ep.to_string()).or_insert(0) += 1;
        }
    }

    // =============================================================================
    // BLOCKING
    // =============================================================================

    fn is_blocked(&self, client_ip: &str) -> bool {
        let blocklist = match self.blocklist.read() {
            Ok(b) => b,
            Err(_) => return false,
        };

        if let Some(expiry) = blocklist.get(client_ip) {
            if Utc::now().timestamp() < *expiry {
                return true;
            }
        }

        false
    }

    fn get_block_expiry(&self, client_ip: &str) -> Option<i64> {
        let blocklist = match self.blocklist.read() {
            Ok(b) => b,
            Err(_) => return None,
        };

        blocklist.get(client_ip).copied()
    }

    /// Block an IP
    pub fn block_ip(&self, client_ip: &str) {
        let expiry = Utc::now().timestamp() + (self.config.block_duration.as_secs() as i64);

        if let Ok(mut blocklist) = self.blocklist.write() {
            blocklist.insert(client_ip.to_string(), expiry);
        }

        self.log_attack(client_ip, "blocked");
    }

    /// Unblock an IP
    pub fn unblock_ip(&self, client_ip: &str) {
        if let Ok(mut blocklist) = self.blocklist.write() {
            blocklist.remove(client_ip);
        }
    }

    // =============================================================================
    // PATTERN DETECTION
    // =============================================================================

    fn is_suspicious_user_agent(&self, user_agent: &str) -> bool {
        let suspicious = [
            "sqlmap",
            "nikto",
            "nmap",
            "masscan",
            "zap",
            "burp",
            "gobuster",
            "dirbuster",
            "wpscan",
            "metasploit",
        ];

        let ua_lower = user_agent.to_lowercase();
        suspicious.iter().any(|s| ua_lower.contains(s))
    }

    // =============================================================================
    // ATTACK LOGGING
    // =============================================================================

    fn log_attack(&self, client_ip: &str, attack_type: &str) {
        let log = AttackLog {
            timestamp: Utc::now().timestamp(),
            client_ip: client_ip.to_string(),
            attack_type: attack_type.to_string(),
            threat_level: ThreatLevel::High,
        };

        if let Ok(mut attack_log) = self.attack_log.write() {
            attack_log.push(log);
            
            // Keep only last 10000
            if attack_log.len() > 10000 {
                attack_log.drain(0..5000);
            }
        }
    }

    /// Get attack statistics
    pub fn get_stats(&self) -> DdosStats {
        let attack_log = match self.attack_log.read() {
            Ok(l) => l,
            Err(_) => return DdosStats::default(),
        };

        let total_attacks = attack_log.len();
        let blocked_ips = match self.blocklist.read() {
            Ok(b) => b.len(),
            Err(_) => 0,
        };

        DdosStats {
            total_attacks,
            blocked_ips,
            last_attack: attack_log.last().map(|l| l.timestamp),
        }
    }

    // =============================================================================
    // CLEANUP
    // =============================================================================

    /// Cleanup old entries
    pub fn cleanup(&self) {
        let now = Utc::now().timestamp();

        // Cleanup blocklist
        if let Ok(mut blocklist) = self.blocklist.write() {
            blocklist.retain(|_, expiry| *expiry > now);
        }

        // Cleanup stats
        if let Ok(mut stats) = self.ip_stats.write() {
            stats.retain(|_, stats| {
                now - stats.last_seen < 600 // 10 minutes
            });
        }
    }
}

// =============================================================================
// TYPES
// =============================================================================

/// DDoS check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdosResult {
    pub allowed: bool,
    pub reason: String,
    pub block_until: Option<i64>,
    pub threat_level: ThreatLevel,
}

/// Threat level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreatLevel {
    None,
    Low,
    Medium,
    High,
    Critical,
}

impl Default for ThreatLevel {
    fn default() -> Self {
        Self::None
    }
}

/// Attack log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackLog {
    pub timestamp: i64,
    pub client_ip: String,
    pub attack_type: String,
    pub threat_level: ThreatLevel,
}

/// DDoS statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdosStats {
    pub total_attacks: usize,
    pub blocked_ips: usize,
    pub last_attack: Option<i64>,
}

impl Default for DdosStats {
    fn default() -> Self {
        Self {
            total_attacks: 0,
            blocked_ips: 0,
            last_attack: None,
        }
    }
}

// =============================================================================
// MIDDLEWARE
// =============================================================================

/// Middleware function for extracting client IP
pub fn get_client_ip(
    remote_addr: &str,
    x_forwarded_for: Option<&str>,
    x_real_ip: Option<&str>,
) -> String {
    // Check X-Real-IP first
    if let Some(ip) = x_real_ip {
        return ip.to_string();
    }

    // Check X-Forwarded-For
    if let Some(ip) = x_forwarded_for {
        if let Some(first) = ip.split(',').next() {
            return first.trim().to_string();
        }
    }

    // Use remote address
    remote_addr.to_string()
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ddos_protector() {
        let protector = DdosProtector::new();
        
        let result = protector.check("192.168.1.1");
        
        assert!(result.allowed);
        assert_eq!(result.threat_level, ThreatLevel::None);
    }

    #[test]
    fn test_block_ip() {
        let protector = DdosProtector::new();
        
        protector.block_ip("192.168.1.100");
        
        let result = protector.check("192.168.1.100");
        
        assert!(!result.allowed);
        assert_eq!(result.reason, "blocked");
    }

    #[test]
    fn test_suspicious_user_agent() {
        let protector = DdosProtector::new();
        
        let result = protector.check_with_headers(
            "192.168.1.1",
            Some("sqlmap/1.0"),
            Some("/admin"),
        );
        
        assert!(!result.allowed);
        assert_eq!(result.reason, "suspicious_user_agent");
    }

    #[test]
    fn test_stats() {
        let protector = DdosProtector::new();
        
        protector.block_ip("192.168.1.1");
        
        let stats = protector.get_stats();
        
        assert!(stats.blocked_ips > 0);
    }
}