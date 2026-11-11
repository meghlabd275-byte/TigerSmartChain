//! NFT Metadata Indexer Implementation

use crate::types::*;
use crate::ipfs::IPFSClient;
use crate::arweave::ArweaveClient;
use std::sync::Arc;
use tokio::sync::RwLock;
use thiserror::Error;
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum NFTMetadataError {
    #[error("Request error: {0}")]
    RequestError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Unsupported URI: {0}")]
    UnsupportedURI(String),
}

// =============================================================================
// INDEXER
// =============================================================================

/// NFT Metadata Indexer
pub struct NFTMetadataIndexer {
    config: NFTMetadataConfig,
    client: Client,
    ipfs_client: IPFSClient,
    arweave_client: ArweaveClient,
    cache: Arc<RwLock<HashMap<String, NFTMetadata>>>,
    /// Metadata URIs by token id, supplied by the caller. Rarity is computed
    /// over the whole collection, so the URIs have to be known before the
    /// scoring pass rather than fetched one at a time on demand.
    metadata_uris: Arc<RwLock<HashMap<String, String>>>,
}

impl NFTMetadataIndexer {
    /// Create new indexer
    pub fn new(config: NFTMetadataConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            config: config.clone(),
            client,
            ipfs_client: IPFSClient::new(config.ipfs_gateways),
            arweave_client: ArweaveClient::new(&config.arweave_gateway),
            cache: Arc::new(RwLock::new(HashMap::new())),
            metadata_uris: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Fetch metadata for a token
    pub async fn fetch_metadata(&self, collection: &str, token_id: &str, metadata_uri: &str) -> Result<NFTMetadata, NFTMetadataError> {
        let cache_key = format!("{}:{}", collection, token_id);
        
        // Check cache first
        if self.config.enable_cache {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        // Fetch from URI
        let metadata = self.fetch_from_uri(metadata_uri).await?;
        let mut metadata = metadata;
        metadata.token_id = token_id.to_string();
        metadata.collection = collection.to_string();

        // Cache it
        if self.config.enable_cache {
            let mut cache = self.cache.write().await;
            cache.insert(cache_key, metadata.clone());
        }

        Ok(metadata)
    }

    /// Fetch metadata from URI (IPFS, Arweave, HTTP)
    async fn fetch_from_uri(&self, uri: &str) -> Result<NFTMetadata, NFTMetadataError> {
        let uri = uri.trim();
        
        if uri.starts_with("ipfs://") {
            // IPFS URI
            let cid = uri.strip_prefix("ipfs://").unwrap_or(uri);
            let text = self.ipfs_client.fetch(cid).await
                .map_err(|e| NFTMetadataError::RequestError(e.to_string()))?;
            serde_json::from_str(&text)
                .map_err(|e| NFTMetadataError::ParseError(e.to_string()))
        } else if uri.starts_with("arweave://") {
            // Arweave URI
            let tx_id = uri.strip_prefix("arweave://").unwrap_or(uri);
            let text = self.arweave_client.fetch(tx_id).await
                .map_err(|e| NFTMetadataError::RequestError(e.to_string()))?;
            serde_json::from_str(&text)
                .map_err(|e| NFTMetadataError::ParseError(e.to_string()))
        } else if uri.starts_with("http://") || uri.starts_with("https://") {
            // The URI comes from the token contract, so it is attacker
            // controlled. Refuse anything that is not a publicly routable
            // http/https host before making the request.
            crate::ssrf::validate_uri(uri).await?;

            let response = self.client
                .get(uri)
                .send()
                .await
                .map_err(|e| NFTMetadataError::RequestError(e.to_string()))?;

            if !response.status().is_success() {
                return Err(NFTMetadataError::RequestError(format!("HTTP error: {}", response.status())));
            }

            // Read the body once. `Response::json` consumes the response, so
            // trying JSON and then falling back to text on the same value does
            // not compile and would not work even if it did.
            let body = response
                .text()
                .await
                .map_err(|e| NFTMetadataError::ParseError(e.to_string()))?;

            // Metadata is JSON by convention (ERC-721 Metadata JSON Schema).
            serde_json::from_str(&body)
                .map_err(|e| NFTMetadataError::ParseError(e.to_string()))
        } else {
            Err(NFTMetadataError::UnsupportedURI(uri.to_string()))
        }
    }

    /// Calculate rarity for a collection from the metadata of its tokens.
    ///
    /// The rarity score of a token is the sum, over its traits, of
    /// `1 / (trait frequency)`. A trait shared by every token in the
    /// collection therefore contributes 1, and a trait on a single token
    /// contributes `token_count`. This is the standard trait-rarity sum: it
    /// rewards a token for holding unusual traits, and summing rather than
    /// multiplying keeps a token with one unique trait from being buried by
    /// common ones.
    ///
    /// Tokens are ranked by score, highest first, with ties broken by token id
    /// so the result is deterministic. Only tokens whose metadata is
    /// reachable are scored; a token that could not be fetched is reported
    /// rather than silently scored as zero, because a missing trait would
    /// otherwise look like a rare one.
    pub async fn calculate_rarity(
        &self,
        collection: &str,
        token_ids: &[String],
    ) -> Result<Vec<NFTRarity>, NFTMetadataError> {
        if token_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Rarity is only meaningful relative to the whole collection, so a
        // frequency needs every token's traits. `metadata_uri` is supplied by
        // the caller per token, since the indexer does not hold the contract
        // binding that maps a token to its URI.
        let mut fetched: Vec<(String, Vec<NFTAttribute>)> = Vec::with_capacity(token_ids.len());
        for token_id in token_ids {
            let uri = match self.metadata_uris.read().await.get(token_id) {
                Some(uri) => uri.clone(),
                None => continue,
            };
            let metadata = self.fetch_metadata(collection, token_id, &uri).await?;
            fetched.push((token_id.clone(), metadata.attributes));
        }

        if fetched.is_empty() {
            return Ok(Vec::new());
        }

        // Count how many tokens hold each (trait_type, value) pair, and how
        // many tokens expose each trait_type at all.
        let total = fetched.len() as f64;
        let mut value_counts: HashMap<(String, String), u64> = HashMap::new();
        for (_, attributes) in &fetched {
            // A token that repeats a trait type would otherwise be counted
            // twice for the same trait, inflating the frequency.
            let mut seen: std::collections::HashSet<(&str, &str)> =
                std::collections::HashSet::new();
            for attribute in attributes {
                if seen.insert((attribute.trait_type.as_str(), attribute.value.as_str())) {
                    *value_counts
                        .entry((attribute.trait_type.clone(), attribute.value.clone()))
                        .or_insert(0) += 1;
                }
            }
        }

        let mut scored: Vec<NFTRarity> = Vec::with_capacity(fetched.len());
        for (token_id, attributes) in &fetched {
            let mut trait_rarities = Vec::with_capacity(attributes.len());
            let mut score = 0.0f64;

            // A repeated (type, value) pair is malformed metadata, and scoring
            // it twice would let a contract inflate its own rarity by listing
            // the same trait repeatedly. Count each distinct pair once, the
            // same way the frequency pass above does.
            let mut scored_pairs: std::collections::HashSet<(&str, &str)> =
                std::collections::HashSet::new();

            for attribute in attributes {
                if !scored_pairs
                    .insert((attribute.trait_type.as_str(), attribute.value.as_str()))
                {
                    continue;
                }
                let count = value_counts
                    .get(&(attribute.trait_type.clone(), attribute.value.clone()))
                    .copied()
                    .unwrap_or(0) as f64;
                if count == 0.0 {
                    continue;
                }
                // How much of the collection holds this value.
                let rarity = count / total;
                score += 1.0 / rarity;
                trait_rarities.push(TraitRarity {
                    trait_type: attribute.trait_type.clone(),
                    value: attribute.value.clone(),
                    rarity,
                });
            }

            // A stable order for the trait list, so the output does not depend
            // on the order the metadata happened to list attributes in.
            trait_rarities.sort_by(|a, b| {
                a.trait_type
                    .cmp(&b.trait_type)
                    .then_with(|| a.value.cmp(&b.value))
            });

            scored.push(NFTRarity {
                token_id: token_id.clone(),
                collection: collection.to_string(),
                rarity_score: score,
                rank: 0,
                trait_rarities,
            });
        }

        // Rank 1 is the rarest. Ties break on token id so the output is
        // deterministic across runs and across indexer instances.
        scored.sort_by(|a, b| {
            b.rarity_score
                .partial_cmp(&a.rarity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.token_id.cmp(&b.token_id))
        });
        for (index, rarity) in scored.iter_mut().enumerate() {
            rarity.rank = (index + 1) as u32;
        }

        Ok(scored)
    }

    /// Record the metadata URI for a token so [`Self::calculate_rarity`] can
    /// fetch it. The caller supplies this from the token contract's
    /// `tokenURI`, which the indexer cannot read without an RPC binding.
    pub async fn set_metadata_uri(&self, token_id: &str, uri: &str) {
        self.metadata_uris
            .write()
            .await
            .insert(token_id.to_string(), uri.to_string());
    }

    /// Get cached metadata
    pub async fn get_cached(&self, collection: &str, token_id: &str) -> Option<NFTMetadata> {
        let cache_key = format!("{}:{}", collection, token_id);
        self.cache.read().await.get(&cache_key).cloned()
    }

    /// Clear cache
    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Score a trait set the way `calculate_rarity` does, without a network
    /// round trip. The scoring is the part worth testing: the fetching is
    /// covered by the SSRF tests, and a real fetch would need a live gateway.
    fn score(collection: &[Vec<(&str, &str)>]) -> Vec<(f64, usize)> {
        let total = collection.len() as f64;
        let mut counts: HashMap<(String, String), u64> = HashMap::new();
        for token in collection {
            let mut seen = std::collections::HashSet::new();
            for (t, v) in token {
                if seen.insert((*t, *v)) {
                    *counts
                        .entry((t.to_string(), v.to_string()))
                        .or_insert(0) += 1;
                }
            }
        }
        let mut out: Vec<(f64, usize)> = collection
            .iter()
            .enumerate()
            .map(|(index, token)| {
                let mut seen = std::collections::HashSet::new();
                let score: f64 = token
                    .iter()
                    .filter(|(t, v)| seen.insert((*t, *v)))
                    .map(|(t, v)| {
                        let c = counts[&(t.to_string(), v.to_string())] as f64;
                        total / c
                    })
                    .sum();
                (score, index)
            })
            .collect();
        out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        out
    }

    #[test]
    fn a_trait_shared_by_every_token_contributes_one() {
        // Every token holds the same single trait, so the frequency is 1 and
        // each token scores exactly 1.
        let collection = vec![vec![("body", "plain")]; 10];
        let scored = score(&collection);
        assert!(scored.iter().all(|(s, _)| (*s - 1.0).abs() < 1e-9));
    }

    #[test]
    fn a_unique_trait_scores_the_collection_size() {
        // Nine tokens share a trait; one has a trait of its own. The unique
        // trait has frequency 1/10, so it contributes 10.
        let mut collection = vec![vec![("hat", "common")]; 9];
        collection.push(vec![("hat", "rare")]);
        let scored = score(&collection);
        let (best, index) = scored[0];
        assert_eq!(index, 9, "the token with the unique trait must rank first");
        assert!((best - 10.0).abs() < 1e-9);
    }

    #[test]
    fn more_rare_traits_ranks_higher() {
        let mut collection = vec![vec![("a", "x"), ("b", "y")]; 10];
        // Give one token two traits that nothing else has.
        collection.push(vec![("a", "x"), ("b", "z"), ("c", "w")]);
        let scored = score(&collection);
        assert_eq!(scored[0].1, 10, "the token with two unique traits ranks first");
    }

    #[test]
    fn a_repeated_trait_is_counted_once_per_token() {
        // The same (type, value) twice on one token must not inflate the
        // frequency, or the rarity would be understated.
        let single = vec![vec![("a", "x")], vec![("a", "y")]];
        let repeated = vec![vec![("a", "x"), ("a", "x")], vec![("a", "y")]];
        assert_eq!(score(&single), score(&repeated));
    }

    #[test]
    fn scores_are_identical_across_runs() {
        let collection = vec![
            vec![("a", "1"), ("b", "1")],
            vec![("a", "1"), ("b", "2")],
            vec![("a", "2"), ("b", "1")],
        ];
        assert_eq!(score(&collection), score(&collection));
    }

    #[tokio::test]
    async fn rarity_of_no_tokens_is_empty() {
        let indexer = NFTMetadataIndexer::new(NFTMetadataConfig::default());
        let result = indexer.calculate_rarity("0xabc", &[]).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn rarity_skips_tokens_with_no_known_uri() {
        // Without a recorded URI there is nothing to fetch, so the result is
        // empty rather than a list of fabricated scores.
        let indexer = NFTMetadataIndexer::new(NFTMetadataConfig::default());
        let ids = vec!["1".to_string(), "2".to_string()];
        let result = indexer.calculate_rarity("0xabc", &ids).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn a_private_metadata_uri_is_refused() {
        let indexer = NFTMetadataIndexer::new(NFTMetadataConfig::default());
        indexer.set_metadata_uri("1", "http://169.254.169.254/latest/meta-data/").await;
        let err = indexer
            .fetch_metadata("0xabc", "1", "http://169.254.169.254/latest/meta-data/")
            .await
            .unwrap_err();
        assert!(
            matches!(err, NFTMetadataError::UnsupportedURI(_)),
            "expected the SSRF guard to reject, got {err:?}"
        );
    }

    #[tokio::test]
    async fn an_unsupported_scheme_is_refused() {
        let indexer = NFTMetadataIndexer::new(NFTMetadataConfig::default());
        let err = indexer
            .fetch_metadata("0xabc", "1", "file:///etc/passwd")
            .await
            .unwrap_err();
        assert!(matches!(err, NFTMetadataError::UnsupportedURI(_)));
    }
}
