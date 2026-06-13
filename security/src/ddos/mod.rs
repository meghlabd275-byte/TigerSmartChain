//! TigerSmartChain Security Module - DDoS Protection
//! 
//! Provides advanced DDoS protection with:
//! - IP reputation tracking
//! - Traffic analysis
//! - Anomaly detection
//! - Geo-blocking
//! - Bot detection
//! - Automatic mitigation

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ============================================================================
// CONSTANTS
// ============================================================================

pub const DEFAULT_SUSPICIOUS_THRESHOLD: u64 = 50;
pub const DEFAULT_BLOCK_THRESHOLD: u64 = 100;
pub const DEFAULT_WINDOW_SECS: u64 = 60;
pub const DEFAULT_BLOCK_DURATION_SECS: u64 = 3600;
pub const MIN_TRUST_SCORE: f64 = 0.0;
pub const MAX_TRUST_SCORE: f64 = 100.0;

// ============================================================================
// ERROR TYPES
// ============================================================================

#[derive(Debug, Clone)]
pub enum DdosError {
    ThreatDetected,
    Blocked,
    InvalidIp,
    MitigationFailed,
}

impl std::fmt::Display for DdosError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ThreatDetected => write!(f, "Threat detected"),
            Self::Blocked => write!(f, "IP blocked"),
            Self::InvalidIp => write!(f, "Invalid IP address"),
            Self::MitigationFailed => write!(f, "Mitigation failed"),
        }
    }
}

impl std::error::Error for DdosError {}

// ============================================================================
// CONFIG
// ============================================================================

#[derive(Debug, Clone)]
pub struct DdosConfig {
    pub suspicious_threshold: u64,
    pub block_threshold: u64,
    pub window_secs: u64,
    pub block_duration_secs: u64,
    pub enable_geo_blocking: bool,
    pub allowed_countries: Vec<String>,
    pub blocked_countries: Vec<String>,
    pub enable_bot_detection: bool,
}

impl Default for DdosConfig {
    fn default() -> Self {
        Self {
            suspicious_threshold: DEFAULT_SUSPICIOUS_THRESHOLD,
            block_threshold: DEFAULT_BLOCK_THRESHOLD,
            window_secs: DEFAULT_WINDOW_SECS,
            block_duration_secs: DEFAULT_BLOCK_DURATION_SECS,
            enable_geo_blocking: false,
            allowed_countries: Vec::new(),
            blocked_countries: Vec::new(),
            enable_bot_detection: true,
        }
    }
}

impl DdosConfig {
    pub fn strict() -> Self {
        Self {
            suspicious_threshold: 20,
            block_threshold: 50,
            window_secs: 60,
            block_duration_secs: 7200,
            enable_geo_blocking: true,
            allowed_countries: vec!["US".to_string(), "GB".to_string()],
            blocked_countries: vec!["CN".to_string(), "RU".to_string()],
            enable_bot_detection: true,
        }
    }
}

// ============================================================================
// IP REPUTATION
// ============================================================================

#[derive(Debug, Clone)]
pub struct IpReputation {
    pub ip: String,
    pub country: Option<String>,
    pub requests_total: u64,
    pub requests_window: u64,
    pub first_seen: Instant,
    pub last_seen: Instant,
    pub blocked: bool,
    pub blocked_until: Option<Instant>,
    pub trust_score: f64,
    pub is_bot: bool,
    pub is_proxy: bool,
    pub is_vpn: bool,
    pub attack_signatures: Vec<AttackSignature>,
    pub request_patterns: Vec<RequestInfo>,
}

#[derive(Debug, Clone)]
pub struct AttackSignature {
    pub attack_type: AttackType,
    pub severity: Severity,
    pub timestamp: Instant,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttackType {
    Flood,
    Replay,
    Amplification,
    SqlInjection,
    Xss,
    Csrf,
    BruteForce,
    Scraping,
    Bot,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct RequestInfo {
    pub path: String,
    pub method: String,
    pub timestamp: Instant,
    pub status_code: u16,
    pub bytes_sent: u64,
    pub user_agent: Option<String>,
}

impl IpReputation {
    pub fn new(ip: String) -> Self {
        Self {
            ip,
            country: None,
            requests_total: 0,
            requests_window: 0,
            first_seen: Instant::now(),
            last_seen: Instant::now(),
            blocked: false,
            blocked_until: None,
            trust_score: MAX_TRUST_SCORE,
            is_bot: false,
            is_proxy: false,
            is_vpn: false,
            attack_signatures: Vec::new(),
            request_patterns: Vec::new(),
        }
    }
    
    pub fn is_blocked(&self) -> bool {
        if self.blocked {
            if let Some(until) = self.blocked_until {
                return Instant::now() < until;
            }
            return true;
        }
        false
    }
    
    pub fn calculate_trust_score(&mut self) -> f64 {
        let mut score = MAX_TRUST_SCORE;
        
        // Reduce for attack signatures
        for sig in &self.attack_signatures {
            match sig.severity {
                Severity::Low => score -= 5.0,
                Severity::Medium => score -= 15.0,
                Severity::High => score -= 30.0,
                Severity::Critical => score -= 50.0,
            }
        }
        
        // Reduce for bot detection
        if self.is_bot {
            score -= 40.0;
        }
        
        // Reduce for proxy/VPN
        if self.is_proxy || self.is_vpn {
            score -= 20.0;
        }
        
        // Reduce for high request rate
        if self.requests_window > 1000 {
            score -= 30.0;
        } else if self.requests_window > 500 {
            score -= 15.0;
        }
        
        self.trust_score = score.max(MIN_TRUST_SCORE).min(MAX_TRUST_SCORE);
        self.trust_score
    }
}

// ============================================================================
// DDOS PROTECTOR
// ============================================================================

pub struct DdosProtector {
    config: DdosConfig,
    reputations: Arc<RwLock<HashMap<String, IpReputation>>>,
    whitelisted_ips: Arc<RwLock<Vec<String>>>,
    blacklisted_ips: Arc<RwLock<Vec<String>>>,
    attack_log: Arc<RwLock<Vec<AttackLog>>>,
}

#[derive(Debug, Clone)]
pub struct AttackLog {
    pub ip: String,
    pub attack_type: AttackType,
    pub severity: Severity,
    pub timestamp: Instant,
    pub details: String,
}

impl DdosProtector {
    pub fn new(config: DdosConfig) -> Self {
        Self {
            config,
            reputations: Arc::new(RwLock::new(HashMap::new())),
            whitelisted_ips: Arc::new(RwLock::new(Vec::new())),
            blacklisted_ips: Arc::new(RwLock::new(Vec::new())),
            attack_log: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Check if request should be allowed
    pub async fn check_request(
        &self,
        ip: &str,
        user_agent: Option<&str>,
        country: Option<&str>,
    ) -> Result<IpReputation, DdosError> {
        let window = Duration::from_secs(self.config.window_secs);
        
        // Check blacklist
        let blacklist = self.blacklisted_ips.read().await;
        if blacklist.iter().any(|b| b == ip) {
            return Err(DdosError::Blocked);
        }
        
        // Check whitelist
        let whitelist = self.whitelisted_ips.read().await;
        if whitelist.iter().any(|w| w == ip) {
            return Ok(IpReputation::new(ip.to_string()));
        }
        
        // Check geo-blocking
        if self.config.enable_geo_blocking {
            if let Some(country) = country {
                if !self.config.allowed_countries.is_empty() {
                    if !self.config.allowed_countries.contains(&country.to_string()) {
                        return Err(DdosError::ThreatDetected);
                    }
                }
                if self.config.blocked_countries.contains(&country.to_string()) {
                    return Err(DdosError::Blocked);
                }
            }
        }
        
        // Get or create reputation
        let mut reputations = self.reputations.write().await;
        let reputation = reputations
            .entry(ip.to_string())
            .or_insert_with(|| IpReputation::new(ip.to_string()));
        
        // Check if blocked
        if reputation.is_blocked() {
            return Err(DdosError::Blocked);
        }
        
        // Update requests
        reputation.requests_total += 1;
        reputation.requests_window += 1;
        reputation.last_seen = Instant::now();
        
        // Check bot detection
        if self.config.enable_bot_detection {
            if let Some(ua) = user_agent {
                reputation.is_bot = self.detect_bot(ua);
            }
        }
        
        // Update country
        if let Some(c) = country {
            reputation.country = Some(c.to_string());
        }
        
        // Check thresholds
        let trust_score = reputation.calculate_trust_score();
        
        if trust_score < 20.0 || reputation.requests_window >= self.config.block_threshold {
            // Block the IP
            reputation.blocked = true;
            reputation.blocked_until = Some(
                Instant::now() + Duration::from_secs(self.config.block_duration_secs)
            );
            
            // Log attack
            self.log_attack(
                ip,
                AttackType::Flood,
                Severity::Critical,
                "High request rate detected",
            ).await;
            
            return Err(DdosError::Blocked);
        }
        
        if trust_score < 50.0 || reputation.requests_window >= self.config.suspicious_threshold {
            // Log suspicious activity
            self.log_attack(
                ip,
                AttackType::Flood,
                Severity::Medium,
                "Suspicious request rate",
            ).await;
        }
        
        // Clean old requests
        let cutoff = Instant::now() - window;
        reputation.request_patterns.retain(|r| r.timestamp > cutoff);
        reputation.requests_window = reputation.request_patterns.len() as u64;
        
        Ok(reputation.clone())
    }
    
    /// Record a request
    pub async fn record_request(
        &self,
        ip: &str,
        path: &str,
        method: &str,
        status_code: u16,
    ) {
        let mut reputations = self.reputations.write().await;
        if let Some(rep) = reputations.get_mut(ip) {
            rep.request_patterns.push(RequestInfo {
                path: path.to_string(),
                method: method.to_string(),
                timestamp: Instant::now(),
                status_code,
                bytes_sent: 0,
                user_agent: None,
            });
        }
    }
    
    /// Detect bot user agent
    fn detect_bot(&self, user_agent: &str) -> bool {
        let bot_indicators = [
            "bot", "spider", "crawler", "scraper",
            "curl", "wget", "python", "java/",
            "httpclient", "go-http", "fetch",
        ];
        
        let ua_lower = user_agent.to_lowercase();
        bot_indicators.iter().any(|b| ua_lower.contains(b))
    }
    
    /// Log attack
    pub async fn log_attack(
        &self,
        ip: &str,
        attack_type: AttackType,
        severity: Severity,
        details: &str,
    ) {
        let log = self.attack_log.write().await;
        log.push(AttackLog {
            ip: ip.to_string(),
            attack_type,
            severity,
            timestamp: Instant::now(),
            details: details.to_string(),
        });
    }
    
    /// Whitelist IP
    pub async fn whitelist(&self, ip: &str) {
        let mut whitelist = self.whitelisted_ips.write().await;
        if !whitelist.contains(&ip.to_string()) {
            whitelist.push(ip.to_string());
        }
    }
    
    /// Blacklist IP
    pub async fn blacklist(&self, ip: &str) {
        let mut blacklist = self.blacklisted_ips.write().await;
        if !blacklist.contains(&ip.to_string()) {
            blacklist.push(ip.to_string());
        }
    }
    
    /// Get attack log
    pub async fn get_attack_log(&self, limit: usize) -> Vec<AttackLog> {
        let log = self.attack_log.read().await;
        log.iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }
    
    /// Get stats
    pub async fn stats(&self) -> DdosStats {
        let reputations = self.reputations.read().await;
        let blocked = reputations.values().filter(|r| r.is_blocked()).count();
        
        DdosStats {
            total_ips: reputations.len(),
            blocked_ips: blocked,
            whitelisted: self.whitelisted_ips.read().await.len(),
            blacklisted: self.blacklisted_ips.read().await.len(),
            attack_log_size: self.attack_log.read().await.len(),
        }
    }
}

// ============================================================================
// STATS
// ============================================================================

#[derive(Debug, Clone)]
pub struct DdosStats {
    pub total_ips: usize,
    pub blocked_ips: usize,
    pub whitelisted: usize,
    pub blacklisted: usize,
    pub attack_log_size: usize,
}

// ============================================================================
// EXPORT
// ============================================================================

pub use self::{
    config::DdosConfig,
    error::DdosError,
    reputation::{IpReputation, AttackSignature, AttackType, Severity, RequestInfo},
    protector::DdosProtector,
    log::AttackLog,
    stats::DdosStats,
};

mod config {
    pub use super::DdosConfig;
}

mod error {
    pub use super::DdosError;
}

mod reputation {
    pub use super::{IpReputation, AttackSignature, AttackType, Severity, RequestInfo};
}

mod protector {
    pub use super::DdosProtector;
}

mod log {
    pub use super::AttackLog;
}

mod stats {
    pub use super::DdosStats;
}