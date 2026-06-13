#![forbid(unsafe_code)]
use std::sync::Arc;
use discord_bot_service::{DiscordBot, DiscordConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Discord Bot Service");
    
    let config = DiscordConfig {
        bot_token: std::env::var("DISCORD_BOT_TOKEN").unwrap_or_default(),
        webhook_url: std::env::var("DISCORD_WEBHOOK_URL").unwrap_or_default(),
        channel_id: std::env::var("DISCORD_CHANNEL_ID").unwrap_or_default(),
    };
    
    let bot = Arc::new(DiscordBot::new(config));
    
    // Example: Send a test message
    // bot.send_price_alert("0x1234", 100.0, AlertCondition::PriceAbove).await;
    
    println!("Discord Bot Service running on port 9003");
    
    // Keep running
    tokio::signal::ctrl_c().await?;
    Ok(())
}