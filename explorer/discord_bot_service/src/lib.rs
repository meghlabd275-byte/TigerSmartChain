//! Discord Bot Service - Notifications and Commands

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    pub bot_token: String,
    pub webhook_url: String,
    pub channel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordMessage {
    pub content: String,
    pub embeds: Vec<DiscordEmbed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordEmbed {
    pub title: String,
    pub description: String,
    pub color: u32,
    pub fields: Vec<EmbedField>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceAlert {
    pub user_id: String,
    pub address: String,
    pub condition: AlertCondition,
    pub threshold: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertCondition {
    PriceAbove,
    PriceBelow,
    PercentChange,
}

pub struct DiscordBot {
    config: DiscordConfig,
    alerts: HashMap<String, Vec<PriceAlert>>,
}

impl DiscordBot {
    pub fn new(config: DiscordConfig) -> Self {
        Self { config, alerts: HashMap::new() }
    }

    /// Send message via webhook
    pub async fn send_message(&self, message: &DiscordMessage) -> Result<(), String> {
        let client = reqwest::Client::new();
        client.post(&self.config.webhook_url)
            .json(message)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Send price alert notification
    pub async fn send_price_alert(&self, address: &str, price: f64, condition: AlertCondition) {
        let condition_str = match condition {
            AlertCondition::PriceAbove => "above",
            AlertCondition::PriceBelow => "below",
            AlertCondition::PercentChange => "changed by",
        };
        
        let message = DiscordMessage {
            content: "🔔 Price Alert!".to_string(),
            embeds: vec![DiscordEmbed {
                title: "Price Alert Triggered".to_string(),
                description: format!("{} is now {} ${}", address, condition_str, price),
                color: 0xFF6B35,
                fields: vec![
                    EmbedField { name: "Token".to_string(), value: address.to_string(), inline: true },
                    EmbedField { name: "Price".to_string(), value: format!("${}", price), inline: true },
                ],
                timestamp: chrono::Utc::now().to_rfc3339(),
            }],
        };
        
        let _ = self.send_message(&message).await;
    }

    /// Send transaction notification
    pub async fn send_tx_notification(&self, tx_hash: &str, from: &str, to: &str, value: &str) {
        let message = DiscordMessage {
            content: "📝 New Transaction".to_string(),
            embeds: vec![DiscordEmbed {
                title: "Transaction Confirmed".to_string(),
                description: format!("Transaction {} confirmed", &tx_hash[..10]),
                color: 0x00CC88,
                fields: vec![
                    EmbedField { name: "From".to_string(), value: format!("{}...", &from[..8]), inline: true },
                    EmbedField { name: "To".to_string(), value: format!("{}...", &to[..8]), inline: true },
                    EmbedField { name: "Value".to_string(), value: value.to_string(), inline: false },
                ],
                timestamp: chrono::Utc::now().to_rfc3339(),
            }],
        };
        
        let _ = self.send_message(&message).await;
    }

    /// Send whale alert
    pub async fn send_whale_alert(&self, address: &str, value: f64) {
        let message = DiscordMessage {
            content: "🐋 Whale Alert!".to_string(),
            embeds: vec![DiscordEmbed {
                title: "Large Transaction Detected".to_string(),
                description: format!("Whale movement of ${} detected", value),
                color: 0xFF3333,
                fields: vec![
                    EmbedField { name: "Address".to_string(), value: address.to_string(), inline: true },
                    EmbedField { name: "Value".to_string(), value: format!("${}", value), inline: true },
                ],
                timestamp: chrono::Utc::now().to_rfc3339(),
            }],
        };
        
        let _ = self.send_message(&message).await;
    }

    /// Subscribe user to alerts
    pub fn subscribe_alert(&mut self, user_id: &str, alert: PriceAlert) {
        self.alerts.entry(user_id.to_string())
            .or_insert_with(Vec::new)
            .push(alert);
    }

    /// Unsubscribe user from alerts
    pub fn unsubscribe_alert(&mut self, user_id: &str, address: &str) {
        if let Some(alerts) = self.alerts.get_mut(user_id) {
            alerts.retain(|a| a.address != address);
        }
    }

    /// Check and send alerts
    pub async fn check_alerts(&self, address: &str, price: f64) {
        for (user_id, alerts) in &self.alerts {
            for alert in alerts {
                if alert.address == address {
                    let should_trigger = match alert.condition {
                        AlertCondition::PriceAbove => price > alert.threshold,
                        AlertCondition::PriceBelow => price < alert.threshold,
                        AlertCondition::PercentChange => false,
                    };
                    
                    if should_trigger {
                        self.send_price_alert(address, price, alert.condition).await;
                    }
                }
            }
        }
    }
}

// Bot commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotCommand {
    pub name: String,
    pub description: String,
    pub handler: CommandHandler,
}

pub enum CommandHandler {
    Price(String),
    Tx(String),
    Balance(String),
    Alert(String),
    Help,
}

pub fn get_commands() -> Vec<BotCommand> {
    vec![
        BotCommand { name: "price".to_string(), description: "Get token price".to_string(), handler: CommandHandler::Price("price".to_string()) },
        BotCommand { name: "tx".to_string(), description: "Get transaction info".to_string(), handler: CommandHandler::Tx("tx".to_string()) },
        BotCommand { name: "balance".to_string(), description: "Get address balance".to_string(), handler: CommandHandler::Balance("balance".to_string()) },
        BotCommand { name: "alert".to_string(), description: "Set price alert".to_string(), handler: CommandHandler::Alert("alert".to_string()) },
        BotCommand { name: "help".to_string(), description: "Show help".to_string(), handler: CommandHandler::Help },
    ]
}