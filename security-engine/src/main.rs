//! TigerSmartChain security engine CLI.
//!
//! Every list the engine consults starts empty, so this binary is useful for
//! one-off lookups once the operator supplies the data: `--block-domain` and
//! `--blacklist` add entries, and `--refresh-feeds` pulls the phishing feeds.
//! Nothing is fabricated to fill a report.

use std::str::FromStr;
use tigersmartchain_security::{BlacklistCategory, RiskLevel, SecurityEngine};
use tracing_subscriber::{fmt, EnvFilter};

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Json,
    Text,
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "text" | "txt" => Ok(OutputFormat::Text),
            other => Err(format!("unknown output format: {other}")),
        }
    }
}

/// The command line.
#[derive(Debug)]
struct Args {
    address: Option<String>,
    contract: Option<String>,
    deployer: Option<String>,
    bytecode: Option<String>,
    token: Option<String>,
    token_name: Option<String>,
    token_symbol: Option<String>,
    block_domain: Vec<String>,
    blocklisted: Vec<String>,
    refresh_feeds: bool,
    format: OutputFormat,
    threshold: RiskLevel,
    verbose: bool,
}

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat::Json
    }
}

impl Default for Args {
    fn default() -> Self {
        Self {
            address: None,
            contract: None,
            deployer: None,
            bytecode: None,
            token: None,
            token_name: None,
            token_symbol: None,
            block_domain: Vec::new(),
            blocklisted: Vec::new(),
            refresh_feeds: false,
            format: OutputFormat::default(),
            threshold: RiskLevel::Medium,
            verbose: false,
        }
    }
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut argv = std::env::args().skip(1);

    while let Some(arg) = argv.next() {
        let mut take_value = |name: &str| -> Result<String, String> {
            argv.next()
                .ok_or_else(|| format!("{name} requires a value"))
        };

        match arg.as_str() {
            "--address" | "-a" => args.address = Some(take_value("--address")?),
            "--contract" | "-c" => args.contract = Some(take_value("--contract")?),
            "--deployer" => args.deployer = Some(take_value("--deployer")?),
            "--bytecode" => args.bytecode = Some(take_value("--bytecode")?),
            "--token" | "-t" => args.token = Some(take_value("--token")?),
            "--token-name" => args.token_name = Some(take_value("--token-name")?),
            "--token-symbol" => args.token_symbol = Some(take_value("--token-symbol")?),
            "--block-domain" => args.block_domain.push(take_value("--block-domain")?),
            "--blacklist" => args.blocklisted.push(take_value("--blacklist")?),
            "--refresh-feeds" => args.refresh_feeds = true,
            "--format" | "-f" => args.format = take_value("--format")?.parse()?,
            "--threshold" => {
                args.threshold = match take_value("--threshold")?.to_ascii_lowercase().as_str() {
                    "safe" => RiskLevel::Safe,
                    "low" => RiskLevel::Low,
                    "medium" => RiskLevel::Medium,
                    "high" => RiskLevel::High,
                    "critical" => RiskLevel::Critical,
                    other => return Err(format!("unknown risk level: {other}")),
                }
            }
            "--verbose" | "-v" => args.verbose = true,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(args)
}

fn print_usage() {
    println!(
        "\
tigersmartchain-security — analyse addresses, contracts and transactions

USAGE:
    tigersmartchain-security [OPTIONS]

INPUT (at least one):
    -a, --address <ADDR>        Address or domain to analyse
    -c, --contract <ADDR>       Contract address to analyse
        --deployer <ADDR>       Deployer address, checked against flagged deployers
        --bytecode <HEX>        Deployed bytecode for capability analysis
    -t, --token <ADDR>          Token contract to analyse
        --token-name <NAME>     Token name, for impersonation checks
        --token-symbol <SYM>    Token symbol, for impersonation checks

DATA:
        --block-domain <DOMAIN> Add a domain to the phishing blocklist (repeatable)
        --blacklist <ADDR>      Add an address to the blacklist (repeatable)
        --refresh-feeds         Fetch the configured phishing feeds

OUTPUT:
    -f, --format <json|text>    Default: json
        --threshold <LEVEL>     Exit non-zero at or above this level (safe|low|medium|high|critical)
    -v, --verbose               Debug logging
    -h, --help                  This message"
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("error: {e}\n");
            print_usage();
            std::process::exit(2);
        }
    };

    let filter = if args.verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"))
    };
    let _ = fmt().with_env_filter(filter).with_target(false).try_init();

    let mut engine = SecurityEngine::new();

    for domain in &args.block_domain {
        engine.block_domain(domain);
    }

    for address in &args.blocklisted {
        engine.blacklist_address(
            address,
            BlacklistCategory::Malicious,
            "supplied on the command line",
            0.9,
        );
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    if args.refresh_feeds {
        let added = runtime.block_on(engine.refresh_phishing_feeds());
        eprintln!("phishing feeds: {added} new domains");
    }

    let report = if let Some(address) = &args.address {
        engine.analyze_address(address)
    } else if let Some(contract) = &args.contract {
        engine.analyze_contract(contract, args.bytecode.as_deref(), args.deployer.as_deref())
    } else if let Some(token) = &args.token {
        engine.analyze_token(
            token,
            args.token_name.as_deref(),
            args.token_symbol.as_deref(),
            args.bytecode.as_deref(),
        )
    } else {
        eprintln!("error: nothing to analyse; supply --address, --contract or --token\n");
        print_usage();
        std::process::exit(2);
    };

    match args.format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        OutputFormat::Text => print_text(&report),
    }

    if report.overall_risk.is_at_least(args.threshold) {
        std::process::exit(1);
    }

    Ok(())
}

fn print_text(report: &tigersmartchain_security::SecurityReport) {
    println!("\n=== TigerSmartChain security report ===\n");
    println!("Overall risk : {:?} ({:.2})", report.overall_risk, report.risk_score);
    println!("Phishing     : {:.2}", report.phishing_score);
    println!("Scam         : {:.2}", report.scam_score);
    println!("Honeypot     : {:.2}", report.honeypot_score);
    println!("Blacklisted  : {}", report.blacklist_match);
    println!("Transaction  : {:.2}", report.transaction_risk);
    println!("Anomaly      : {:.2}", report.anomaly_score);

    if !report.warnings.is_empty() {
        println!("\nWarnings:");
        for warning in &report.warnings {
            println!("  - {warning}");
        }
    }
}
