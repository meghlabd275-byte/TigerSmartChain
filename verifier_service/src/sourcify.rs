//! Sourcify integration.
//!
//! Sourcify publishes verified source code for a contract, keyed by chain and
//! address. It is the fallback a block explorer uses when a contract was not
//! verified on this chain: fetching Sourcify's record lets the explorer show
//! source and an ABI for a contract that was verified elsewhere, instead of
//! claiming nothing is known about it.
//!
//! The record is fetched from the v2 API:
//! `GET /v2/contract/{chain}/{address}?fields=compilation,abi,sources`. The
//! older `/files/any/...` endpoints are deprecated and answer a browser-only
//! notice rather than the record, so they are not used.
//!
//! A contract can be present in Sourcify but only partially matched, which is
//! why the source entries are inspected rather than assumed to carry content.

use super::types::*;

/// Sourcify's public instance. The client can be pointed at a self-hosted one
/// through [`Client::with_url`].
const DEFAULT_URL: &str = "https://sourcify.dev/server";

/// The fields the record needs. Asking for all of them would pull down the
/// creation and runtime bytecode of every contract, which is far more data
/// than an explorer showing source and an ABI needs.
const FIELDS: &str = "compilation,abi,sources";

/// How long to wait on Sourcify before giving up.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Sourcify Client
pub struct Client {
    url: String,
    http: reqwest::blocking::Client,
}

impl Client {
    /// A client against Sourcify's public instance.
    pub fn new() -> Self {
        Self::with_url(DEFAULT_URL)
    }

    /// A client against a specific Sourcify instance.
    ///
    /// A self-hosted Sourcify is the reason this exists: an explorer should not
    /// have to depend on someone else's service being up to show a verified
    /// contract.
    pub fn with_url(url: impl Into<String>) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self {
            url: url.into().trim_end_matches('/').to_string(),
            http,
        }
    }

    /// Fetch the verification record Sourcify holds for a contract.
    ///
    /// The record comes from Sourcify rather than from the caller, so the
    /// source and ABI describe the code that is actually deployed.
    ///
    /// @param chain_id The chain the contract is on. Sourcify keys records by
    /// chain, so the same address on two chains is two records.
    pub fn verify(
        &self,
        chain_id: u64,
        request: &VerificationRequest,
    ) -> Result<VerifiedContract, String> {
        let address = normalise_address(&request.address)?;
        let metadata = self.fetch_metadata(chain_id, &address)?;
        parse_metadata(&metadata, &address)
    }

    /// Whether Sourcify holds a usable record for a contract on this chain.
    ///
    /// A transport failure answers `false`, which is the safe direction: it
    /// never claims a contract is verified when it could not be checked.
    pub fn is_verified(&self, chain_id: u64, address: &str) -> bool {
        match normalise_address(address) {
            Ok(address) => self
                .fetch_metadata(chain_id, &address)
                .map(|m| parse_metadata(&m, &address).is_ok())
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    /// GET the verification record for a contract.
    fn fetch_metadata(&self, chain_id: u64, address: &str) -> Result<serde_json::Value, String> {
        let url = format!(
            "{}/v2/contract/{chain_id}/{address}?fields={FIELDS}",
            self.url
        );
        let response = self
            .http
            .get(&url)
            .send()
            .map_err(|e| format!("sourcify request failed: {e}"))?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(format!(
                "sourcify has no record for {address} on chain {chain_id}"
            ));
        }
        if !status.is_success() {
            return Err(format!("sourcify returned {status}"));
        }
        response
            .json()
            .map_err(|e| format!("sourcify returned an unreadable record: {e}"))
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

/// Validates an address and returns it lowercase with a `0x` prefix.
///
/// The prefix is required, not cosmetic: Sourcify's v2 API rejects a bare
/// forty-character address with `address must NOT have fewer than 42
/// characters`. Lowercasing keeps the URL canonical, since Sourcify accepts
/// any casing and an explorer should make one request per contract rather
/// than one per spelling.
fn normalise_address(address: &str) -> Result<String, String> {
    let trimmed = address.trim().trim_start_matches("0x");
    if trimmed.len() != 40 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("{address} is not a 20-byte address"));
    }
    Ok(format!("0x{}", trimmed.to_ascii_lowercase()))
}

/// Turns a Sourcify v2 record into a [`VerifiedContract`].
///
/// The compiler is at `compilation.compilerVersion` and the ABI is a
/// top-level array that has to be re-serialised. A record whose sources carry
/// only a hash is a partial match: Sourcify knows the code compiles to the
/// deployed bytecode but does not hold the source, so it is refused rather
/// than reported as verified source.
fn parse_metadata(metadata: &serde_json::Value, address: &str) -> Result<VerifiedContract, String> {
    let compiler_version = metadata
        .get("compilation")
        .and_then(|c| c.get("compilerVersion"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "sourcify record has no compiler version".to_string())?
        .to_string();

    let abi = metadata
        .get("abi")
        .ok_or_else(|| "sourcify record has no ABI".to_string())?;
    let abi = serde_json::to_string(abi).map_err(|e| format!("abi re-encode failed: {e}"))?;

    let source_code = collect_sources(metadata)?;

    Ok(VerifiedContract {
        address: address.to_string(),
        source_code,
        compiler_version,
        abi,
        bytecode: String::new(),
        verified_at: 0,
    })
}

/// Concatenates the sources Sourcify holds, labelled by path.
///
/// Multi-file contracts are the normal case, so the path is kept as a comment
/// header: without it the sources run together and the result does not
/// compile.
fn collect_sources(metadata: &serde_json::Value) -> Result<String, String> {
    let sources = metadata
        .get("sources")
        .and_then(|s| s.as_object())
        .ok_or_else(|| "sourcify record has no sources".to_string())?;

    let mut parts: Vec<(String, String)> = Vec::new();
    for (path, entry) in sources {
        if let Some(content) = entry.get("content").and_then(|c| c.as_str()) {
            parts.push((path.clone(), content.to_string()));
        }
    }

    if parts.is_empty() {
        return Err(
            "sourcify holds only source hashes for this contract, not the source".to_string(),
        );
    }

    parts.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::new();
    for (path, content) in parts {
        out.push_str(&format!("// {path}\n{content}\n\n"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata_with_source() -> serde_json::Value {
        serde_json::json!({
            "compilation": { "compilerVersion": "0.8.20+commit.a1b79de6" },
            "abi": [{ "type": "function", "name": "transfer" }],
            "sources": {
                "contracts/Token.sol": { "content": "contract Token {}" },
                "contracts/IThing.sol": { "content": "interface IThing {}" }
            }
        })
    }

    #[test]
    fn an_address_is_normalised_to_the_form_the_urls_use() {
        assert_eq!(
            normalise_address("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap(),
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
        // A bare address with no prefix is accepted and gains one, because the
        // API requires the prefix.
        assert_eq!(
            normalise_address("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
    }

    #[test]
    fn a_malformed_address_is_refused_rather_than_sent() {
        // Too short, too long, and not hex at all.
        assert!(normalise_address("0x1234").is_err());
        assert!(normalise_address(&format!("0x{}", "a".repeat(41))).is_err());
        assert!(normalise_address(&format!("0x{}", "z".repeat(40))).is_err());
    }

    #[test]
    fn a_record_is_read_into_a_verified_contract() {
        let record =
            parse_metadata(&metadata_with_source(), "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
                .unwrap();
        assert_eq!(record.compiler_version, "0.8.20+commit.a1b79de6");
        assert!(record.abi.contains("transfer"));
        // Both sources are present, each labelled with its path.
        assert!(record.source_code.contains("// contracts/Token.sol"));
        assert!(record.source_code.contains("// contracts/IThing.sol"));
        assert!(record.source_code.contains("contract Token {}"));
        assert!(record.source_code.contains("interface IThing {}"));
    }

    #[test]
    fn sources_are_ordered_so_the_same_record_always_reads_the_same_way() {
        let first = parse_metadata(&metadata_with_source(), "aa")
            .unwrap()
            .source_code;
        let second = parse_metadata(&metadata_with_source(), "aa")
            .unwrap()
            .source_code;
        assert_eq!(first, second);
        // The interface sorts before the token by path.
        let token = first.find("Token.sol").unwrap();
        let thing = first.find("IThing.sol").unwrap();
        assert!(thing < token, "sources must be ordered by path");
    }

    #[test]
    fn a_hash_only_record_is_refused_rather_than_reported_as_source() {
        // This is what a partial Sourcify match looks like: the code is known
        // to compile, but the source itself was never uploaded.
        let metadata = serde_json::json!({
            "compilation": { "compilerVersion": "0.8.20+commit.a1b79de6" },
            "abi": [],
            "sources": { "contracts/Token.sol": { "keccak256": "0xabc" } }
        });
        let err = parse_metadata(&metadata, "aa").unwrap_err();
        assert!(
            err.contains("only source hashes"),
            "expected a hash-only error, got: {err}"
        );
    }

    #[test]
    fn a_record_missing_its_compiler_or_abi_is_refused() {
        let no_compiler = serde_json::json!({
            "abi": [],
            "sources": { "a.sol": { "content": "contract A {}" } }
        });
        assert!(parse_metadata(&no_compiler, "aa")
            .unwrap_err()
            .contains("compiler version"));

        let no_abi = serde_json::json!({
            "compilation": { "compilerVersion": "0.8.20" },
            "sources": { "a.sol": { "content": "contract A {}" } }
        });
        assert!(parse_metadata(&no_abi, "aa").unwrap_err().contains("ABI"));

        let no_sources = serde_json::json!({
            "compilation": { "compilerVersion": "0.8.20" },
            "abi": []
        });
        assert!(parse_metadata(&no_sources, "aa")
            .unwrap_err()
            .contains("no sources"));
    }

    #[test]
    fn an_unreachable_instance_is_an_error_rather_than_a_false_verification() {
        // A port nothing listens on. The point is that a failure to reach
        // Sourcify must not be reported as "not verified": those are different
        // answers, and conflating them would let an outage look like a
        // contract that was never verified.
        let client = Client::with_url("http://127.0.0.1:1");
        let request = VerificationRequest {
            address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
            source_code: String::new(),
            compiler_version: String::new(),
            abi: String::new(),
        };
        let err = client.verify(1, &request).unwrap_err();
        assert!(
            err.contains("sourcify request failed"),
            "expected a transport error, got: {err}"
        );
        // is_verified answers false on a transport failure, which is the safe
        // direction: it never claims a contract is verified when it could not
        // be checked.
        assert!(!client.is_verified(1, "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"));
    }

    #[test]
    fn a_malformed_address_is_rejected_before_any_request() {
        let client = Client::with_url("http://127.0.0.1:1");
        let request = VerificationRequest {
            address: "not-an-address".to_string(),
            source_code: String::new(),
            compiler_version: String::new(),
            abi: String::new(),
        };
        let err = client.verify(1, &request).unwrap_err();
        assert!(err.contains("20-byte address"), "got: {err}");
    }
}
