//! TigerSmartChain Security Engine - Main Binary
//! 
//! Run security checks from command line for:
//! - Address analysis
//! - Contract analysis  
//! - Transaction analysis
//! - Token analysis

use clap::{Parser, ValueEnum};
use tigersmartchain_security::{
    SecurityEngine, SecurityReport, RiskLevel,
};
use std::str::FromStr;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "tigersmartchain-security")]
#[command(about = "TigerSmartChain Security Engine", long_about = None)]
struct Cli {
    /// Address to analyze (Ethereum format: 0x...)
    #[arg(short, long)]
    address: Option<String>,

    /// Contract address to analyze
    #[arg(short = 'c', long)]
    contract: Option<String>,

    /// Transaction hash to analyze
    #[arg(short = 't', long)]
    transaction: Option<String>,

    /// Token contract to analyze
    #[arg(short, long)]
    token: Option<String>,

    /// Output format
    #[arg(short, long, default_value = "json")]
    format: OutputFormat,

    /// Risk threshold for exit code
    #[arg(long, default_value = "medium")]
    threshold: RiskLevelArg,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// API key for external services
    #[arg(long)]
    api_key: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[derive(serde::Serialize, serde::Deserialize)]
enum OutputFormat {
    Json,
    Text,
    Table,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum RiskLevelArg {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

impl FromStr for RiskLevelArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "safe" => Ok(RiskLevelArg::Safe),
            "low" => Ok(RiskLevelArg::Low),
            "medium" => Ok(RiskLevelArg::Medium),
            "high" => Ok(RiskLevelArg::High),
            "critical" => Ok(RiskLevelArg::Critical),
            _ => Err(format!("Unknown risk level: {}", s)),
        }
    }
}

impl From<RiskLevelArg> for RiskLevel {
    fn from(arg: RiskLevelArg) -> Self {
        match arg {
            RiskLevelArg::Safe => RiskLevel::Safe,
            RiskLevelArg::Low => RiskLevel::Low,
            RiskLevelArg::Medium => RiskLevel::Medium,
            RiskLevelArg::High => RiskLevel::High,
            RiskLevelArg::Critical => RiskLevel::Critical,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let cli = Cli::parse();
    
    let filter = if cli.verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };
    
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Create security engine
    let mut engine = SecurityEngine::new();
    
    if let Some(api_key) = &cli.api_key {
        engine = engine.with_api_key(api_key.clone());
    }

    // Run analysis based on input
    let report = if let Some(address) = &cli.address {
        engine.analyze_address(address).await?
    } else if let Some(contract) = &cli.contract {
        engine.analyze_contract(contract).await?
    } else if let Some(transaction) = &cli.transaction {
        engine.analyze_transaction(transaction).await?
    } else if let Some(token) = &cli.token {
        engine.analyze_token(token).await?
    } else {
        println!("Please provide --address, --contract, --transaction, or --token");
        std::process::exit(1);
    };

    // Output results
    match cli.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Text => {
            print_report_text(&report);
        }
        OutputFormat::Table => {
            print_report_table(&report);
        }
    }

    // Exit with appropriate code
    let threshold: RiskLevel = cli.threshold.into();
    if report.overall_risk.as_float() >= threshold.as_float() {
        std::process::exit(1);
    }

    Ok(())
}

fn print_report_text(report: &SecurityReport) {
    println!("\n=== TigerSmartChain Security Report ===\n");
    println!("Overall Risk: {:?}", report.overall_risk);
    println!();
    
    if !report.warnings.is_empty() {
        println!("Warnings:");
        for warning in &report.warnings {
            println!("  - {}", warning);
        }
        println!();
    }

    println!("Details:");
    println!("  Phishing Score: {:.2}", report.phishing_score);
    println!("  Scam Score: {:.2}", report.scam_score);
    println!("  Honeypot Score: {:.2}", report.honeypot_score);
    println!("  Blacklist Match: {}", report.blacklist_match);
    println!("  Transaction Risk: {:.2}", report.transaction_risk);
    println!("  Anomaly Score: {:.2}", report.anomaly_score);
}

fn print_report_table(report: &SecurityReport) {
    use std::fmt::Write as _;
    
    let mut output = String::new();
    
    writeln!(output, "+------------------------------------------+")?;
    writeln!(output, "|   TigerSmartChain Security Report    |")?;
    writeln!(output, "+------------------------------------------+")?;
    writeln!(output, "| Overall Risk: {:24} |", format!("{:?}", report.overall_risk))?;
    writeln!(output, "+------------------------------------------+")?;
    writeln!(output, "| Metric              | Score           |")?;
    writeln!(output, "+------------------------------------------+")?;
    writeln!(output, "| Phishing           | {:15.2} |", report.phishing_score)?;
    writeln!(output, "| Scam               | {:15.2} |", report.scam_score)?;
    writeln!(output, "| Honeypot           | {:15.2} |", report.honeypot_score)?;
    writeln!(output, "| Blacklist          | {:15} |", if report.blacklist_match { "YES" } else { "NO" })?;
    writeln!(output, "| Transaction Risk   | {:15.2} |", report.transaction_risk)?;
    writeln!(output, "| Anomaly            | {:15.2} |", report.anomaly_score)?;
    writeln!(output, "+------------------------------------------+")?;

    if !report.warnings.is_empty() {
        writeln!(output, "\nWarnings:")?;
        for warning in &report.warnings {
            writeln!(output, "  - {}", warning)?;
        }
    }

    println!("{}", output);
}