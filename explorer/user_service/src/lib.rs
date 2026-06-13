//! TigerScan User Service
//! Watchlists, Notes, Alerts, Custom Dashboards with encryption

use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use thiserror::Error;
use uuid::Uuid;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum UserError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Encryption error: {0}")]
    Encryption(String),
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub encryption_key: String,
    pub jwt_secret: String,
    pub token_expiry_hours: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            encryption_key: std::env::var("ENCRYPTION_KEY").unwrap_or_else(|_| "default-key-change-in-production".to_string()),
            jwt_secret: std::env::var("JWT_SECRET").unwrap_or_else(|_| "jwt-secret-change-in-production".to_string()),
            token_expiry_hours: 24,
        }
    }
}

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub username: String,
    pub password_hash: String,
    pub created_at: i64,
    pub verified: bool,
    pub preferences: UserPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub theme: String,
    pub currency: String,
    pub timezone: String,
    pub notifications: NotificationSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub email: bool,
    pub push: bool,
    pub telegram: bool,
    pub price_alerts: bool,
    pub tx_alerts: bool,
    pub nft_alerts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Watchlist {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub addresses: Vec<WatchlistItem>,
    pub tokens: Vec<WatchlistToken>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistItem {
    pub address: String,
    pub label: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistToken {
    pub token_address: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressNote {
    pub id: String,
    pub user_id: String,
    pub address: String,
    pub note: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub user_id: String,
    pub alert_type: AlertType,
    pub condition: AlertCondition,
    pub notification_channels: Vec<NotificationChannel>,
    pub active: bool,
    pub triggered_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    PriceAbove,
    PriceBelow,
    PriceChange,
    TxConfirmed,
    TokenTransfer,
    NftTransfer,
    WhaleMovement,
    GasAbove,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertCondition {
    pub address: Option<String>,
    pub threshold: Option<String>,
    pub percentage: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub channel_type: ChannelType,
    pub destination: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Email,
    Telegram,
    Discord,
    Sms,
    Push,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomDashboard {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub widgets: Vec<DashboardWidget>,
    pub layout: DashboardLayout,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardWidget {
    pub widget_type: WidgetType,
    pub config: serde_json::Value,
    pub position: WidgetPosition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetPosition {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetType {
    Portfolio,
    Watchlist,
    PriceChart,
    RecentTransactions,
    GasTracker,
    NftGallery,
    Alerts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardLayout {
    pub columns: u32,
    pub rows: u32,
}

// ============================================================================
// Auth Service
// ============================================================================

pub struct AuthService {
    config: Config,
    state: Arc<RwLock<AuthState>>,
}

#[derive(Debug)]
pub struct AuthState {
    pub users: HashMap<String, User>,
    pub sessions: HashMap<String, Session>,
    pub password_resets: HashMap<String, PasswordReset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: String,
    pub token: String,
    pub expires_at: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordReset {
    pub user_id: String,
    pub token: String,
    pub expires_at: i64,
}

impl AuthService {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(AuthState {
                users: HashMap::new(),
                sessions: HashMap::new(),
                password_resets: HashMap::new(),
            })),
        }
    }

    /// Register new user
    pub fn register(&self, email: &str, username: &str, password: &str) -> Result<User, UserError> {
        let state = self.state.read();
        
        // Check if email exists
        for user in state.users.values() {
            if user.email == email {
                return Err(UserError::InvalidInput("Email already registered".to_string()));
            }
        }
        
        let password_hash = self.hash_password(password);
        
        let user = User {
            id: Uuid::new_v4().to_string(),
            email: email.to_string(),
            username: username.to_string(),
            password_hash,
            created_at: Utc::now().timestamp(),
            verified: false,
            preferences: UserPreferences {
                theme: "dark".to_string(),
                currency: "USD".to_string(),
                timezone: "UTC".to_string(),
                notifications: NotificationSettings {
                    email: true,
                    push: false,
                    telegram: false,
                    price_alerts: true,
                    tx_alerts: true,
                    nft_alerts: true,
                },
            },
        };
        
        drop(state);
        let mut state = self.state.write();
        state.users.insert(user.id.clone(), user.clone());
        
        Ok(user)
    }

    /// Login user
    pub fn login(&self, email: &str, password: &str) -> Result<Session, UserError> {
        let state = self.state.read();
        
        let user = state.users.values()
            .find(|u| u.email == email)
            .ok_or(UserError::Unauthorized)?;
        
        let password_hash = self.hash_password(password);
        if user.password_hash != password_hash {
            return Err(UserError::Unauthorized);
        }
        
        let session = Session {
            user_id: user.id.clone(),
            token: Uuid::new_v4().to_string(),
            expires_at: Utc::now().timestamp() + self.config.token_expiry_hours * 3600,
            created_at: Utc::now().timestamp(),
        };
        
        drop(state);
        let mut state = self.state.write();
        state.sessions.insert(session.token.clone(), session.clone());
        
        Ok(session)
    }

    /// Logout user
    pub fn logout(&self, token: &str) {
        let mut state = self.state.write();
        state.sessions.remove(token);
    }

    /// Verify session
    pub fn verify_session(&self, token: &str) -> Result<User, UserError> {
        let state = self.state.read();
        
        let session = state.sessions.get(token)
            .ok_or(UserError::Unauthorized)?;
        
        if session.expires_at < Utc::now().timestamp() {
            return Err(UserError::Unauthorized);
        }
        
        let user = state.users.get(&session.user_id)
            .ok_or(UserError::NotFound("User not found".to_string()))?;
        
        Ok(user.clone())
    }

    /// Hash password with SHA-256
    fn hash_password(&self, password: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        hasher.update(self.config.encryption_key.as_bytes());
        hex::encode(hasher.finalize())
    }
}

// ============================================================================
// Watchlist Service
// ============================================================================

pub struct WatchlistService {
    state: Arc<RwLock<WatchlistState>>,
}

#[derive(Debug, Default)]
pub struct WatchlistState {
    pub watchlists: HashMap<String, Watchlist>,
}

impl WatchlistService {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(WatchlistState::default())),
        }
    }

    /// Create watchlist
    pub fn create(&self, user_id: &str, name: &str) -> Watchlist {
        let watchlist = Watchlist {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            name: name.to_string(),
            addresses: vec![],
            tokens: vec![],
            created_at: Utc::now().timestamp(),
            updated_at: Utc::now().timestamp(),
        };
        
        let mut state = self.state.write();
        state.watchlists.insert(watchlist.id.clone(), watchlist.clone());
        watchlist
    }

    /// Add address to watchlist
    pub fn add_address(&self, watchlist_id: &str, address: &str, label: &str, note: Option<String>) -> Result<Watchlist, UserError> {
        let mut state = self.state.write();
        
        let watchlist = state.watchlists.get_mut(watchlist_id)
            .ok_or(UserError::NotFound("Watchlist not found".to_string()))?;
        
        watchlist.addresses.push(WatchlistItem {
            address: address.to_string(),
            label: label.to_string(),
            note,
        });
        watchlist.updated_at = Utc::now().timestamp();
        
        Ok(watchlist.clone())
    }

    /// Get user's watchlists
    pub fn get_user_watchlists(&self, user_id: &str) -> Vec<Watchlist> {
        let state = self.state.read();
        state.watchlists.values()
            .filter(|w| w.user_id == user_id)
            .cloned()
            .collect()
    }

    /// Delete watchlist
    pub fn delete(&self, watchlist_id: &str, user_id: &str) -> Result<(), UserError> {
        let mut state = self.state.write();
        
        if let Some(watchlist) = state.watchlists.get(watchlist_id) {
            if watchlist.user_id != user_id {
                return Err(UserError::Unauthorized);
            }
            state.watchlists.remove(watchlist_id);
            Ok(())
        } else {
            Err(UserError::NotFound("Watchlist not found".to_string()))
        }
    }
}

// ============================================================================
// Notes Service
// ============================================================================

pub struct NotesService {
    state: Arc<RwLock<NotesState>>,
}

#[derive(Debug, Default)]
pub struct NotesState {
    pub notes: HashMap<String, AddressNote>,
}

impl NotesService {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(NotesState::default())),
        }
    }

    /// Create/update note for address
    pub fn save_note(&self, user_id: &str, address: &str, note: &str, tags: Vec<String>) -> AddressNote {
        let mut state = self.state.write();
        
        // Find existing or create new
        let existing_key = state.notes.keys()
            .find(|k| state.notes.get(*k).map(|n| n.user_id == user_id && n.address == address).unwrap_or(false))
            .cloned();
        
        let address_note = if let Some(key) = existing_key {
            let existing = state.notes.get_mut(&key).unwrap();
            existing.note = note.to_string();
            existing.tags = tags;
            existing.updated_at = Utc::now().timestamp();
            existing.clone()
        } else {
            let new_note = AddressNote {
                id: Uuid::new_v4().to_string(),
                user_id: user_id.to_string(),
                address: address.to_string(),
                note: note.to_string(),
                tags,
                created_at: Utc::now().timestamp(),
                updated_at: Utc::now().timestamp(),
            };
            state.notes.insert(new_note.id.clone(), new_note.clone());
            new_note
        };
        
        address_note
    }

    /// Get user's notes
    pub fn get_user_notes(&self, user_id: &str) -> Vec<AddressNote> {
        let state = self.state.read();
        state.notes.values()
            .filter(|n| n.user_id == user_id)
            .cloned()
            .collect()
    }

    /// Delete note
    pub fn delete_note(&self, note_id: &str, user_id: &str) -> Result<(), UserError> {
        let mut state = self.state.write();
        
        if let Some(note) = state.notes.get(note_id) {
            if note.user_id != user_id {
                return Err(UserError::Unauthorized);
            }
            state.notes.remove(note_id);
            Ok(())
        } else {
            Err(UserError::NotFound("Note not found".to_string()))
        }
    }
}

// ============================================================================
// Alerts Service
// ============================================================================

pub struct AlertsService {
    state: Arc<RwLock<AlertsState>>,
}

#[derive(Debug, Default)]
pub struct AlertsState {
    pub alerts: HashMap<String, Alert>,
}

impl AlertsService {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(AlertsState::default())),
        }
    }

    /// Create alert
    pub fn create(&self, user_id: &str, alert_type: AlertType, condition: AlertCondition, channels: Vec<NotificationChannel>) -> Alert {
        let alert = Alert {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            alert_type,
            condition,
            notification_channels: channels,
            active: true,
            triggered_at: None,
            created_at: Utc::now().timestamp(),
        };
        
        let mut state = self.state.write();
        state.alerts.insert(alert.id.clone(), alert.clone());
        alert
    }

    /// Get user's alerts
    pub fn get_user_alerts(&self, user_id: &str) -> Vec<Alert> {
        let state = self.state.read();
        state.alerts.values()
            .filter(|a| a.user_id == user_id)
            .cloned()
            .collect()
    }

    /// Toggle alert
    pub fn toggle_alert(&self, alert_id: &str, user_id: &str, active: bool) -> Result<Alert, UserError> {
        let mut state = self.state.write();
        
        let alert = state.alerts.get_mut(alert_id)
            .ok_or(UserError::NotFound("Alert not found".to_string()))?;
        
        if alert.user_id != user_id {
            return Err(UserError::Unauthorized);
        }
        
        alert.active = active;
        Ok(alert.clone())
    }

    /// Delete alert
    pub fn delete(&self, alert_id: &str, user_id: &str) -> Result<(), UserError> {
        let mut state = self.state.write();
        
        if let Some(alert) = state.alerts.get(alert_id) {
            if alert.user_id != user_id {
                return Err(UserError::Unauthorized);
            }
            state.alerts.remove(alert_id);
            Ok(())
        } else {
            Err(UserError::NotFound("Alert not found".to_string()))
        }
    }
}

// ============================================================================
// Dashboard Service
// ============================================================================

pub struct DashboardService {
    state: Arc<RwLock<DashboardState>>,
}

#[derive(Debug, Default)]
pub struct DashboardState {
    pub dashboards: HashMap<String, CustomDashboard>,
}

impl DashboardService {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(DashboardState::default())),
        }
    }

    /// Create dashboard
    pub fn create(&self, user_id: &str, name: &str, layout: DashboardLayout) -> CustomDashboard {
        let dashboard = CustomDashboard {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            name: name.to_string(),
            widgets: vec![],
            layout,
            created_at: Utc::now().timestamp(),
            updated_at: Utc::now().timestamp(),
        };
        
        let mut state = self.state.write();
        state.dashboards.insert(dashboard.id.clone(), dashboard.clone());
        dashboard
    }

    /// Add widget
    pub fn add_widget(&self, dashboard_id: &str, user_id: &str, widget: DashboardWidget) -> Result<CustomDashboard, UserError> {
        let mut state = self.state.write();
        
        let dashboard = state.dashboards.get_mut(dashboard_id)
            .ok_or(UserError::NotFound("Dashboard not found".to_string()))?;
        
        if dashboard.user_id != user_id {
            return Err(UserError::Unauthorized);
        }
        
        dashboard.widgets.push(widget);
        dashboard.updated_at = Utc::now().timestamp();
        
        Ok(dashboard.clone())
    }

    /// Get user dashboards
    pub fn get_user_dashboards(&self, user_id: &str) -> Vec<CustomDashboard> {
        let state = self.state.read();
        state.dashboards.values()
            .filter(|d| d.user_id == user_id)
            .cloned()
            .collect()
    }
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub success: bool,
    pub token: Option<String>,
    pub user: Option<User>,
    pub error: Option<String>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watchlist() {
        let service = WatchlistService::new();
        
        let watchlist = service.create("user1", "My Watchlist");
        assert_eq!(watchlist.user_id, "user1");
        
        let updated = service.add_address(&watchlist.id, "0x1234", "Test", Some("Note".to_string())).unwrap();
        assert_eq!(updated.addresses.len(), 1);
    }

    #[test]
    fn test_notes() {
        let service = NotesService::new();
        
        let note = service.save_note("user1", "0x1234", "Important", vec!["tag1".to_string()]);
        assert_eq!(note.address, "0x1234");
    }

    #[test]
    fn test_alerts() {
        let service = AlertsService::new();
        
        let alert = service.create(
            "user1",
            AlertType::PriceAbove,
            AlertCondition {
                address: Some("0x1234".to_string()),
                threshold: Some("1000".to_string()),
                percentage: None,
            },
            vec![NotificationChannel {
                channel_type: ChannelType::Email,
                destination: "user@test.com".to_string(),
            }],
        );
        
        assert!(alert.active);
    }
}