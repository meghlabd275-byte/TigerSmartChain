//! Slack Integration Service

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    pub webhook_url: String,
    pub bot_token: String,
    pub channel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackMessage {
    pub text: String,
    pub channel: Option<String>,
    pub blocks: Option<Vec<SlackBlock>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackBlock {
    pub type: String,
    pub text: Option<SlackText>,
    pub elements: Option<Vec<SlackElement>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackText {
    pub type: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackElement {
    pub type: String,
    pub text: Option<String>,
    pub url: Option<String>,
}

pub struct SlackService {
    config: SlackConfig,
}

impl SlackService {
    pub fn new(config: SlackConfig) -> Self {
        Self { config }
    }

    /// Send message via webhook
    pub async fn send_message(&self, message: &str) -> Result<(), String> {
        let client = reqwest::Client::new();
        let payload = SlackMessage {
            text: message.to_string(),
            channel: None,
            blocks: None,
        };
        client.post(&self.config.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Send rich message with blocks
    pub async fn send_rich_message(&self, blocks: Vec<SlackBlock>) -> Result<(), String> {
        let client = reqwest::Client::new();
        let payload = SlackMessage {
            text: "TigerScan Alert".to_string(),
            channel: Some(self.config.channel_id.clone()),
            blocks: Some(blocks),
        };
        client.post(&self.config.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Send price alert
    pub async fn send_price_alert(&self, token: &str, price: f64, condition: &str) {
        let blocks = vec![
            SlackBlock {
                type: "section".to_string(),
                text: Some(SlackText {
                    type: "mrkdwn".to_string(),
                    text: format!("*🔔 Price Alert*"),
                }),
                elements: None,
            },
            SlackBlock {
                type: "section".to_string(),
                text: Some(SlackText {
                    type: "mrkdwn".to_string(),
                    text: format!("Token: `{}`\nCondition: {}\nPrice: ${:.2}", token, condition, price),
                }),
                elements: None,
            },
        ];
        let _ = self.send_rich_message(blocks).await;
    }

    /// Send transaction notification
    pub async fn send_tx_notification(&self, tx_hash: &str, from: &str, to: &str, value: &str) {
        let blocks = vec![
            SlackBlock {
                type: "section".to_string(),
                text: Some(SlackText {
                    type: "mrkdwn".to_string(),
                    text: format!("*📝 Transaction Confirmed*"),
                }),
                elements: None,
            },
            SlackBlock {
                type: "section".to_string(),
                text: Some(SlackText {
                    type: "mrkdwn".to_string(),
                    text: format!("Hash: `{}`\nFrom: `{}`\nTo: `{}`\nValue: {}", 
                        &tx_hash[..10], &from[..8], &to[..8], value),
                }),
                elements: None,
            },
        ];
        let _ = self.send_rich_message(blocks).await;
    }

    /// Send whale alert
    pub async fn send_whale_alert(&self, address: &str, value: f64) {
        let blocks = vec![
            SlackBlock {
                type: "section".to_string(),
                text: Some(SlackText {
                    type: "mrkdwn".to_string(),
                    text: "*🐋 Whale Alert!*".to_string(),
                }),
                elements: None,
            },
            SlackBlock {
                type: "section".to_string(),
                text: Some(SlackText {
                    type: "mrkdwn".to_string(),
                    text: format!("Address: `{}`\nValue: ${:.2}", address, value),
                }),
                elements: None,
            },
        ];
        let _ = self.send_rich_message(blocks).await;
    }
}

// Slash commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommand {
    pub command: String,
    pub text: String,
    pub user_id: String,
    pub channel_id: String,
    pub response_url: String,
}

impl SlackService {
    pub fn handle_slash_command(cmd: &SlashCommand) -> String {
        let parts: Vec<&str> = cmd.text.split_whitespace().collect();
        
        match parts.first() {
            Some(&"price") => format!("Token price for {}: $XXX", parts.get(1).unwrap_or(&"ETH")),
            Some(&"tx") => format!("Transaction info for {}: ...", parts.get(1).unwrap_or(&"0x..."))),
            Some(&"balance") => format!("Balance for {}: XXX TGR", parts.get(1).unwrap_or(&"0x..."))),
            Some(&"help") => format!("Available commands: price, tx, balance, help"),
            _ => "Unknown command. Try: price, tx, balance, help".to_string(),
        }
    }
}