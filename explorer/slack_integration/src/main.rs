#![forbid(unsafe_code)]
use slack_integration::{SlackService, SlackConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Slack Integration Service");
    
    let config = SlackConfig {
        webhook_url: std::env::var("SLACK_WEBHOOK_URL").unwrap_or_default(),
        bot_token: std::env::var("SLACK_BOT_TOKEN").unwrap_or_default(),
        channel_id: std::env::var("SLACK_CHANNEL_ID").unwrap_or_default(),
    };
    
    let service = SlackService::new(config);
    
    // Example: Send a test message
    // service.send_price_alert("0x1234", 100.0, "above").await;
    
    println!("Slack Integration Service running on port 9005");
    tokio::signal::ctrl_c().await?;
    Ok(())
}