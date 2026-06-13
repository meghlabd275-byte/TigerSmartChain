//! TigerScan API Analytics Dashboard Service
//! Track API usage, rate limits, and generate analytics

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum AnalyticsError {
    #[error("Data error: {0}")]
    Data(String),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub retention_days: usize,
    pub max_requests_per_day: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            retention_days: 30,
            max_requests_per_day: 1_000_000,
        }
    }
}

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIRequest {
    pub id: String,
    pub method: String,
    pub endpoint: String,
    pub status_code: u16,
    pub response_time_ms: u64,
    pub ip: String,
    pub user_agent: String,
    pub api_key: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIAnalytics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_response_time: f64,
    pub p95_response_time: f64,
    pub p99_response_time: f64,
    pub requests_by_endpoint: HashMap<String, u64>,
    pub requests_by_status: HashMap<String, u64>,
    pub requests_by_method: HashMap<String, u64>,
    pub top_ips: Vec<IpStats>,
    pub top_api_keys: Vec<ApiKeyStats>,
    pub errors: Vec<ErrorStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpStats {
    pub ip: String,
    pub requests: u64,
    pub avg_response_time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyStats {
    pub api_key: String,
    pub requests: u64,
    pub avg_response_time: f64,
    pub rate_limit_hits: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorStats {
    pub endpoint: String,
    pub status_code: u16,
    pub count: u64,
    pub last_occurrence: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitInfo {
    pub ip: String,
    pub requests_today: u64,
    pub remaining: i64,
    pub reset_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub analytics: APIAnalytics,
    pub rate_limits: Vec<RateLimitInfo>,
    pub last_updated: i64,
}

// ============================================================================
// Analytics Service
// ============================================================================

pub struct AnalyticsService {
    config: Config,
    state: Arc<RwLock<AnalyticsState>>,
}

#[derive(Debug)]
pub struct AnalyticsState {
    pub requests: VecDeque<APIRequest>,
    pub requests_by_ip: HashMap<String, VecDeque<APIRequest>>,
    pub requests_by_key: HashMap<String, VecDeque<APIRequest>>,
    pub endpoint_stats: HashMap<String, EndpointStats>,
    pub total_requests: u64,
    pub total_errors: u64,
    pub response_times: Vec<u64>,
}

#[derive(Debug)]
pub struct EndpointStats {
    pub total_requests: u64,
    pub successful: u64,
    pub failed: u64,
    pub total_response_time: u64,
}

impl AnalyticsService {
    pub fn new(config: Config) -> Self {
        info!("Initializing API Analytics Service");
        
        let service = Self {
            config,
            state: Arc::new(RwLock::new(AnalyticsState {
                requests: VecDeque::new(),
                requests_by_ip: HashMap::new(),
                requests_by_key: HashMap::new(),
                endpoint_stats: HashMap::new(),
                total_requests: 0,
                total_errors: 0,
                response_times: Vec::new(),
            })),
        };
        
        info!("API Analytics Service initialized");
        service
    }

    /// Record an API request
    pub fn record_request(&self, request: APIRequest) {
        let mut state = self.state.write();
        
        // Add to main queue
        state.requests.push_back(request.clone());
        
        // Trim old requests
        while state.requests.len() > 100000 {
            state.requests.pop_front();
        }
        
        // Track by IP
        let ip_requests = state.requests_by_ip
            .entry(request.ip.clone())
            .or_insert_with(VecDeque::new);
        ip_requests.push_back(request.clone());
        
        // Track by API key
        if let Some(key) = &request.api_key {
            let key_requests = state.requests_by_key
                .entry(key.clone())
                .or_insert_with(VecDeque::new);
            key_requests.push_back(request.clone());
        }
        
        // Update endpoint stats
        let endpoint_stats = state.endpoint_stats
            .entry(request.endpoint.clone())
            .or_insert_with(|| EndpointStats {
                total_requests: 0,
                successful: 0,
                failed: 0,
                total_response_time: 0,
            });
        
        endpoint_stats.total_requests += 1;
        endpoint_stats.total_response_time += request.response_time_ms;
        
        if request.status_code >= 400 {
            endpoint_stats.failed += 1;
            state.total_errors += 1;
        } else {
            endpoint_stats.successful += 1;
        }
        
        // Update totals
        state.total_requests += 1;
        state.response_times.push(request.response_time_ms);
        
        // Keep response times manageable
        if state.response_times.len() > 10000 {
            state.response_times.drain(0..5000);
        }
    }

    /// Get analytics summary
    pub fn get_analytics(&self) -> APIAnalytics {
        let state = self.state.read();
        
        let total = state.total_requests;
        let errors = state.total_errors;
        
        // Calculate average response time
        let avg_response = if !state.response_times.is_empty() {
            state.response_times.iter().sum::<u64>() as f64 / state.response_times.len() as f64
        } else {
            0.0
        };
        
        // Calculate percentiles
        let mut sorted_times = state.response_times.clone();
        sorted_times.sort();
        
        let p95 = if !sorted_times.is_empty() {
            let idx = (sorted_times.len() as f64 * 0.95) as usize;
            sorted_times.get(idx).copied().unwrap_or(0) as f64
        } else {
            0.0
        };
        
        let p99 = if !sorted_times.is_empty() {
            let idx = (sorted_times.len() as f64 * 0.99) as usize;
            sorted_times.get(idx).copied().unwrap_or(0) as f64
        } else {
            0.0
        };
        
        // Requests by endpoint
        let mut by_endpoint: HashMap<String, u64> = HashMap::new();
        for (endpoint, stats) in &state.endpoint_stats {
            by_endpoint.insert(endpoint.clone(), stats.total_requests);
        }
        
        // Requests by status
        let mut by_status: HashMap<String, u64> = HashMap::new();
        for request in &state.requests {
            let status = format!("{}", request.status_code);
            *by_status.entry(status).or_insert(0) += 1;
        }
        
        // Requests by method
        let mut by_method: HashMap<String, u64> = HashMap::new();
        for request in &state.requests {
            *by_method.entry(request.method.clone()).or_insert(0) += 1;
        }
        
        // Top IPs
        let mut ip_stats: Vec<_> = state.requests_by_ip.iter()
            .map(|(ip, requests)| {
                let count = requests.len() as u64;
                let avg_time = if count > 0 {
                    requests.iter().map(|r| r.response_time_ms).sum::<u64>() as f64 / count as f64
                } else {
                    0.0
                };
                IpStats {
                    ip: ip.clone(),
                    requests: count,
                    avg_response_time: avg_time,
                }
            })
            .collect();
        
        ip_stats.sort_by(|a, b| b.requests.cmp(&a.requests));
        ip_stats.truncate(10);
        
        // Top API keys
        let mut key_stats: Vec<_> = state.requests_by_key.iter()
            .map(|(key, requests)| {
                let count = requests.len() as u64;
                let avg_time = if count > 0 {
                    requests.iter().map(|r| r.response_time_ms).sum::<u64>() as f64 / count as f64
                } else {
                    0.0
                };
                ApiKeyStats {
                    api_key: key.clone(),
                    requests: count,
                    avg_response_time: avg_time,
                    rate_limit_hits: 0,
                }
            })
            .collect();
        
        key_stats.sort_by(|a, b| b.requests.cmp(&a.requests));
        key_stats.truncate(10);
        
        // Errors
        let mut error_map: HashMap<(String, u16), u64> = HashMap::new();
        for request in state.requests.iter().filter(|r| r.status_code >= 400) {
            *error_map.entry((request.endpoint.clone(), request.status_code)).or_insert(0) += 1;
        }
        
        let errors: Vec<_> = error_map.into_iter()
            .map(|((endpoint, code), count)| ErrorStats {
                endpoint,
                status_code: code,
                count,
                last_occurrence: Utc::now().timestamp(),
            })
            .collect();
        
        APIAnalytics {
            total_requests: total,
            successful_requests: total - errors,
            failed_requests: errors,
            avg_response_time: avg_response,
            p95_response_time: p95,
            p99_response_time: p99,
            requests_by_endpoint: by_endpoint,
            requests_by_status: by_status,
            requests_by_method: by_method,
            top_ips: ip_stats,
            top_api_keys: key_stats,
            errors,
        }
    }

    /// Check rate limit for IP
    pub fn check_rate_limit(&self, ip: &str) -> RateLimitInfo {
        let state = self.state.read();
        
        let requests_today = state.requests_by_ip
            .get(ip)
            .map(|q| q.len() as u64)
            .unwrap_or(0);
        
        let remaining = (self.config.max_requests_per_day as i64 - requests_today as i64).max(0);
        
        // Reset at midnight UTC
        let now = Utc::now();
        let reset = now.date_naive().succ_opt()
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
            .unwrap_or(now);
        
        RateLimitInfo {
            ip: ip.to_string(),
            requests_today,
            remaining,
            reset_at: reset.and_utc().timestamp(),
        }
    }
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsApiRequest {
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsApiResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analytics() {
        let service = AnalyticsService::new(Config::default());
        
        service.record_request(APIRequest {
            id: "1".to_string(),
            method: "GET".to_string(),
            endpoint: "/blocks".to_string(),
            status_code: 200,
            response_time_ms: 50,
            ip: "127.0.0.1".to_string(),
            user_agent: "test".to_string(),
            api_key: None,
            timestamp: Utc::now().timestamp(),
        });
        
        let analytics = service.get_analytics();
        
        assert_eq!(analytics.total_requests, 1);
    }
}