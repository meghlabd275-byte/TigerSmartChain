//! Phishing domain detection.
//!
//! The previous version had a regex that matched the real
//! `tigersmartchain.com`, so the production domain was reported as phishing,
//! and a "typosquatting" check that tested
//! `domain.contains(&target_name.replace("tiger", "tiger"))` — a no-op
//! replacement, making the test `domain.contains("tigersmartchain")` guarded by
//! a condition on digits, which is not a typosquatting test at all. Its
//! `fetch_phishing_list` returned an empty vector and never contacted anything.
//!
//! What follows is a real detector: it normalizes the input, treats the
//! protected domains as explicitly safe, and flags a domain only when it is on
//! a fetched blocklist or is a genuine lookalike of a protected domain
//! (edit distance, homoglyphs, or a brand name on an unrelated suffix).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A single phishing finding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhishingMatch {
    pub domain: String,
    pub target: String,
    pub similarity: f64,
    pub detection_method: String,
    pub confidence: f64,
    pub reported: bool,
    pub source: String,
}

/// A domain the detector protects.
#[derive(Debug, Clone)]
pub struct ProtectedDomain {
    /// Human-readable owner of the domain.
    pub name: String,
    /// The registrable domain, e.g. `tigersmartchain.com`.
    pub domain: String,
    /// Brand tokens that must not appear on an unrelated domain.
    pub brands: Vec<String>,
}

/// Public suffixes with more than one label.
///
/// A full public-suffix list is far larger than this; these are the ones a
/// brand is plausibly impersonated under. Anything else falls back to taking
/// the last two labels, which is the correct answer for `.com`, `.io` and so on.
const MULTI_LABEL_SUFFIXES: &[&str] = &[
    "co.uk", "org.uk", "ac.uk", "gov.uk", "co.jp", "or.jp", "ne.jp", "com.au", "net.au", "org.au",
    "co.nz", "com.br", "com.cn", "com.mx", "com.tr", "co.in", "co.kr", "co.za", "com.sg",
    "com.hk", "com.tw", "co.id", "com.ar", "com.co", "com.pe", "com.ph", "com.my", "com.vn",
    "co.th", "com.ua", "com.ru", "co.il", "com.pk", "com.ng", "co.ke", "com.eg", "com.sa",
];

/// Confusable characters mapped to the ASCII letter they imitate.
///
/// Digits map to the letter they *look like*, not the one they would produce on
/// a keyboard: `1` looks like an `i`, so `t1gersmartchain` folds onto the real
/// name. Mapping `1` to `l` — a common convention — folds it to
/// `tlgersmartchaln` and misses the impersonation entirely.
///
/// Accented Latin letters are folded to their base letter because
/// `Url::parse` converts a non-ASCII host to punycode, and two names that differ
/// only by an accent would otherwise compare as unrelated strings.
const HOMOGLYPHS: &[(char, char)] = &[
    // Digits that resemble letters.
    ('0', 'o'),
    ('1', 'i'),
    ('2', 'z'),
    ('3', 'e'),
    ('4', 'a'),
    ('5', 's'),
    ('6', 'g'),
    ('7', 't'),
    ('8', 'b'),
    ('9', 'g'),
    // Punctuation.
    ('$', 's'),
    ('@', 'a'),
    ('!', 'i'),
    ('|', 'l'),
    ('+', 't'),
    // Greek.
    ('α', 'a'),
    ('β', 'b'),
    ('ε', 'e'),
    ('ι', 'i'),
    ('κ', 'k'),
    ('ν', 'v'),
    ('ο', 'o'),
    ('ρ', 'p'),
    ('τ', 't'),
    ('υ', 'u'),
    ('χ', 'x'),
    ('ω', 'w'),
    // Cyrillic.
    ('а', 'a'),
    ('в', 'b'),
    ('е', 'e'),
    ('к', 'k'),
    ('м', 'm'),
    ('н', 'h'),
    ('о', 'o'),
    ('р', 'p'),
    ('с', 'c'),
    ('у', 'y'),
    ('х', 'x'),
    ('ѕ', 's'),
    ('і', 'i'),
    ('ј', 'j'),
    // Latin letters that merely look like others.
    ('ɑ', 'a'),
    ('ɡ', 'g'),
    ('ɩ', 'i'),
    ('ɯ', 'm'),
    ('ʀ', 'r'),
    ('ᴍ', 'm'),
    // Accented Latin letters, folded to their base letter.
    ('à', 'a'),
    ('á', 'a'),
    ('â', 'a'),
    ('ã', 'a'),
    ('ä', 'a'),
    ('å', 'a'),
    ('ç', 'c'),
    ('è', 'e'),
    ('é', 'e'),
    ('ê', 'e'),
    ('ë', 'e'),
    ('ì', 'i'),
    ('í', 'i'),
    ('î', 'i'),
    ('ï', 'i'),
    ('ñ', 'n'),
    ('ò', 'o'),
    ('ó', 'o'),
    ('ô', 'o'),
    ('õ', 'o'),
    ('ö', 'o'),
    ('ø', 'o'),
    ('ù', 'u'),
    ('ú', 'u'),
    ('û', 'u'),
    ('ü', 'u'),
    ('ý', 'y'),
    ('ÿ', 'y'),
    ('š', 's'),
    ('ž', 'z'),
    ('ł', 'l'),
    ('ð', 'd'),
    ('þ', 't'),
];

/// The phishing detector.
pub struct PhishingDetector {
    /// Domains confirmed as phishing, from the local list and any fetched feed.
    blocked: HashSet<String>,
    /// Domains the detector must never flag.
    protected: Vec<ProtectedDomain>,
    /// Feeds to pull blocklists from.
    feeds: Vec<String>,
    client: reqwest::Client,
}

impl PhishingDetector {
    /// A detector seeded with the built-in protected domains and no blocklist.
    ///
    /// The blocklist starts empty on purpose: a hardcoded list of "known
    /// phishing domains" is stale the day it is written. It is populated by
    /// [`PhishingDetector::refresh_feeds`].
    pub fn new() -> Self {
        Self {
            blocked: HashSet::new(),
            protected: Self::default_protected(),
            feeds: vec![
                "https://openphish.com/feed.txt".to_string(),
                "https://raw.githubusercontent.com/Phishing-Database/Phishing.Database/master/phishing-domains-ACTIVE.txt".to_string(),
            ],
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .unwrap_or_default(),
        }
    }

    /// The domains that belong to the project and its peers.
    fn default_protected() -> Vec<ProtectedDomain> {
        vec![
            ProtectedDomain {
                name: "TigerSmartChain".to_string(),
                domain: "tigersmartchain.com".to_string(),
                brands: vec!["tigersmartchain".to_string(), "tigerchain".to_string()],
            },
            ProtectedDomain {
                name: "TigerScan".to_string(),
                domain: "tigerscan.io".to_string(),
                brands: vec!["tigerscan".to_string()],
            },
            ProtectedDomain {
                name: "BNB Chain".to_string(),
                domain: "bnbchain.org".to_string(),
                brands: vec!["bnbchain".to_string()],
            },
            ProtectedDomain {
                name: "Binance".to_string(),
                domain: "binance.com".to_string(),
                brands: vec!["binance".to_string()],
            },
        ]
    }

    /// Add a domain to the blocklist.
    pub fn block(&mut self, domain: &str) {
        if let Some(domain) = normalize_domain(domain) {
            self.blocked.insert(domain);
        }
    }

    /// Number of blocked domains.
    pub fn blocked_count(&self) -> usize {
        self.blocked.len()
    }

    /// Fetch every configured feed and add the domains to the blocklist.
    ///
    /// Returns the number of new domains. A feed that fails is skipped; the
    /// error is reported rather than hidden, so an operator can tell a feed
    /// outage from an empty feed.
    pub async fn refresh_feeds(&mut self) -> usize {
        let mut added = 0usize;

        for feed in self.feeds.clone() {
            let response = match self.client.get(&feed).send().await {
                Ok(response) => response,
                Err(e) => {
                    tracing::warn!("phishing feed {feed} unreachable: {e}");
                    continue;
                }
            };

            if !response.status().is_success() {
                tracing::warn!("phishing feed {feed} returned {}", response.status());
                continue;
            }

            let body = match response.text().await {
                Ok(body) => body,
                Err(e) => {
                    tracing::warn!("phishing feed {feed} could not be read: {e}");
                    continue;
                }
            };

            added += self.ingest_feed(&body);
        }

        added
    }

    /// Add the domains in a feed body to the blocklist.
    ///
    /// OpenPhish lines are bare URLs; the phishing-database feed is one domain
    /// per line. Both are handled by extracting a host when the line parses as
    /// a URL and treating it as a domain otherwise.
    pub fn ingest_feed(&mut self, body: &str) -> usize {
        let mut added = 0usize;

        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let candidate = line
                .split_whitespace()
                .next()
                .and_then(normalize_domain);

            if let Some(domain) = candidate {
                if !self.is_protected(&domain) && self.blocked.insert(domain) {
                    added += 1;
                }
            }
        }

        added
    }

    /// Whether the domain belongs to a protected owner.
    pub fn is_protected(&self, domain: &str) -> bool {
        let Some(domain) = normalize_domain(domain) else {
            return false;
        };

        self.protected
            .iter()
            .any(|p| domain == p.domain || domain.ends_with(&format!(".{}", p.domain)))
    }

    /// Check a domain or URL.
    ///
    /// Returns `None` when the input is malformed, is protected, or shows no
    /// sign of impersonation.
    pub fn check_domain(&self, input: &str) -> Option<PhishingMatch> {
        let domain = normalize_domain(input)?;

        // The real domain is never flagged, whatever else the rules say.
        if self.is_protected(&domain) {
            return None;
        }

        if self.blocked.contains(&domain) {
            return Some(PhishingMatch {
                domain: domain.clone(),
                target: self.find_target(&domain).unwrap_or_else(|| "Unknown".to_string()),
                similarity: 1.0,
                detection_method: "blocklist".to_string(),
                confidence: 1.0,
                reported: true,
                source: "blocklist".to_string(),
            });
        }

        self.lookalike(&domain)
    }

    /// Check a full URL.
    pub fn analyze_url(&self, url: &str) -> Option<PhishingMatch> {
        self.check_domain(url)
    }

    /// Detect a lookalike of a protected domain.
    fn lookalike(&self, domain: &str) -> Option<PhishingMatch> {
        let registrable = registrable_domain(domain);

        // An attacker can register the punycode spelling of a name that looks
        // like the real one, so decode any `xn--` labels before comparing.
        let decoded = decode_punycode_host(&registrable);
        let decoded_registrable = registrable_domain(&decoded);

        let candidates = if decoded_registrable == registrable {
            vec![registrable.clone()]
        } else {
            vec![registrable.clone(), decoded_registrable]
        };

        for target in &self.protected {
            let folded_target = fold_homoglyphs(&target.domain);

            for candidate in &candidates {
                let folded = fold_homoglyphs(candidate);

                // A brand name inside the registrable domain, on a suffix the
                // brand does not own, is impersonation regardless of edit
                // distance: `tigersmartchain.xyz` is not a typo of anything.
                if let Some(brand) = target
                    .brands
                    .iter()
                    .find(|brand| candidate.contains(brand.as_str()))
                {
                    return Some(PhishingMatch {
                        domain: domain.to_string(),
                        target: target.name.clone(),
                        similarity: brand.len() as f64 / candidate.len().max(1) as f64,
                        detection_method: "brand_on_foreign_domain".to_string(),
                        confidence: 0.95,
                        reported: false,
                        source: "brand_match".to_string(),
                    });
                }

                // Identical once substitutions are undone: `t1gersmartchain`
                // and `tiger-smartchain` both fold onto the real name.
                if folded == folded_target && *candidate != target.domain {
                    return Some(PhishingMatch {
                        domain: domain.to_string(),
                        target: target.name.clone(),
                        similarity: 1.0,
                        detection_method: "homoglyph".to_string(),
                        confidence: 0.95,
                        reported: false,
                        source: "homoglyph_detection".to_string(),
                    });
                }

                // Otherwise measure how close the names are, literally and
                // after undoing substitutions.
                let literal = levenshtein(candidate, &target.domain);
                let folded_distance = levenshtein(&folded, &folded_target);
                let distance = literal.min(folded_distance);

                let length = candidate.len().max(target.domain.len());
                let similarity = 1.0 - (distance as f64 / length as f64);

                // Distance 1 or 2 on a name this long is a deliberate
                // misspelling; beyond that the names are simply different.
                if distance > 0 && distance <= 2 && length >= 6 {
                    let method = if folded_distance < literal {
                        "homoglyph"
                    } else {
                        "typosquatting"
                    };

                    return Some(PhishingMatch {
                        domain: domain.to_string(),
                        target: target.name.clone(),
                        similarity,
                        detection_method: method.to_string(),
                        confidence: if distance == 1 { 0.9 } else { 0.75 },
                        reported: false,
                        source: format!("{method}_detection"),
                    });
                }
            }
        }

        None
    }

    /// The protected owner a domain appears to impersonate.
    pub fn find_target(&self, domain: &str) -> Option<String> {
        let registrable = registrable_domain(domain);
        self.protected
            .iter()
            .find(|p| {
                registrable.contains(&p.domain) || registrable.contains(&p.name.to_lowercase())
            })
            .map(|p| p.name.clone())
    }
}

impl Default for PhishingDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize a domain or URL to a bare lowercase hostname.
///
/// Handles a full URL, a bare domain, a trailing dot, a port, userinfo, and
/// internationalized names (which `Url` converts to punycode, so
/// `tigersmartchaín.com` and `xn--tigersmartchan-...` compare equal).
pub fn normalize_domain(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parsed = if trimmed.contains("://") {
        url::Url::parse(trimmed).ok()?
    } else {
        // A bare hostname is not a valid relative URL, so give it a scheme.
        url::Url::parse(&format!("http://{trimmed}")).ok()?
    };

    let host = parsed.host_str()?.to_lowercase();
    let host = host.strip_suffix('.').unwrap_or(&host).to_string();

    if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok() {
        // An IP address is not a domain name and cannot be a lookalike.
        return None;
    }

    // A hostname with no dot is a single-label name such as `localhost`; it is
    // still a valid host to check, but it has no registrable domain.
    Some(host)
}

/// The registrable domain of a hostname: the public suffix plus one label.
pub fn registrable_domain(host: &str) -> String {
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    if labels.len() <= 2 {
        return labels.join(".");
    }

    let last_two = format!("{}.{}", labels[labels.len() - 2], labels[labels.len() - 1]);
    if MULTI_LABEL_SUFFIXES.contains(&last_two.as_str()) && labels.len() >= 3 {
        format!("{}.{}", labels[labels.len() - 3], last_two)
    } else {
        last_two
    }
}

/// Fold a name to a canonical form for comparison.
///
/// Case is lowered, confusable characters become the letter they imitate,
/// separators are dropped, and the `rn`/`m` pair is unified. Both sides of a
/// comparison are folded, so `t1gersm4rtcha1n` and `tigersmartchain` become the
/// same string while `tigersmartchian` differs by one transposition.
pub fn fold_homoglyphs(input: &str) -> String {
    let mut out = String::with_capacity(input.len());

    for ch in input.chars() {
        let lowered = ch.to_ascii_lowercase();

        if lowered == '-' || lowered == '_' {
            // Separators are decorative in a lookalike: `tiger-smartchain`
            // folds to the same string as `tigersmartchain`.
            continue;
        }

        let mapped = HOMOGLYPHS
            .iter()
            .find(|(from, _)| *from == lowered)
            .map(|(_, to)| *to)
            .unwrap_or(lowered);

        out.push(mapped);
    }

    // `rn` is a common stand-in for `m`; unify after mapping so `tigerrn…` and
    // `tigerm…` compare equal.
    out.replace("rn", "m")
}

/// Punycode decoding constants, from RFC 3492 section 5.
const PUNYCODE_BASE: u32 = 36;
const PUNYCODE_TMIN: u32 = 1;
const PUNYCODE_TMAX: u32 = 26;
const PUNYCODE_SKEW: u32 = 38;
const PUNYCODE_DAMP: u32 = 700;
const PUNYCODE_INITIAL_BIAS: u32 = 72;
const PUNYCODE_INITIAL_N: u32 = 128;

/// The bias adaptation function, RFC 3492 section 6.1.
fn punycode_adapt(mut delta: u32, num_points: u32, first_time: bool) -> u32 {
    delta = if first_time { delta / PUNYCODE_DAMP } else { delta / 2 };
    delta += delta / num_points.max(1);

    let mut k = 0;
    while delta > ((PUNYCODE_BASE - PUNYCODE_TMIN) * PUNYCODE_TMAX) / 2 {
        delta /= PUNYCODE_BASE - PUNYCODE_TMIN;
        k += PUNYCODE_BASE;
    }

    k + (((PUNYCODE_BASE - PUNYCODE_TMIN + 1) * delta) / (delta + PUNYCODE_SKEW))
}

/// The value of a punycode digit character.
fn punycode_digit(c: char) -> Option<u32> {
    match c {
        'a'..='z' => Some(c as u32 - 'a' as u32),
        'A'..='Z' => Some(c as u32 - 'A' as u32),
        '0'..='9' => Some(c as u32 - '0' as u32 + 26),
        _ => None,
    }
}

/// Decode one `xn--` label to its Unicode form.
///
/// Returns `None` for anything malformed, so a hand-written `xn--` label that
/// does not decode is treated as an ordinary string rather than as an error.
pub fn decode_punycode_label(label: &str) -> Option<String> {
    let encoded = label.strip_prefix("xn--")?;
    if encoded.is_empty() {
        return None;
    }

    let mut output: Vec<char> = Vec::new();
    let mut input = encoded;

    // Everything before the last delimiter is literal ASCII.
    if let Some(index) = input.rfind('-') {
        for c in input[..index].chars() {
            if !c.is_ascii() {
                return None;
            }
            output.push(c);
        }
        input = &input[index + 1..];
    }

    let chars: Vec<char> = input.chars().collect();
    let mut position = 0usize;
    let mut n = PUNYCODE_INITIAL_N;
    let mut i: u32 = 0;
    let mut bias = PUNYCODE_INITIAL_BIAS;

    while position < chars.len() {
        let old_i = i;
        let mut weight: u32 = 1;
        let mut k = PUNYCODE_BASE;

        loop {
            let digit = punycode_digit(*chars.get(position)?)?;
            position += 1;

            i = i.checked_add(digit.checked_mul(weight)?)?;

            let threshold = if k <= bias {
                PUNYCODE_TMIN
            } else if k >= bias + PUNYCODE_TMAX {
                PUNYCODE_TMAX
            } else {
                k - bias
            };

            if digit < threshold {
                break;
            }

            weight = weight.checked_mul(PUNYCODE_BASE - threshold)?;
            k += PUNYCODE_BASE;
        }

        let out_len = output.len() as u32 + 1;
        bias = punycode_adapt(i - old_i, out_len, old_i == 0);
        n = n.checked_add(i / out_len)?;
        i %= out_len;

        let decoded = char::from_u32(n)?;
        output.insert(i as usize, decoded);
        i += 1;
    }

    Some(output.into_iter().collect())
}

/// Decode every `xn--` label in a hostname.
///
/// A hostname with no such label is returned unchanged, so callers can always
/// decode without checking first.
pub fn decode_punycode_host(host: &str) -> String {
    host.split('.')
        .map(|label| {
            if label.starts_with("xn--") {
                decode_punycode_label(label).unwrap_or_else(|| label.to_string())
            } else {
                label.to_string()
            }
        })
        .collect::<Vec<String>>()
        .join(".")
}

/// Edit distance where transposing two adjacent characters costs one edit.
///
/// This is the optimal-string-alignment variant, which is what a typo model
/// needs: `chian` for `chain` is one slip of the fingers, not two edits. Plain
/// Levenshtein scores it two, which pushes the domain outside the threshold and
/// misses the most common impersonation there is.
///
/// Very long inputs fall back to the length difference, which bounds the work
/// without letting a pathological input stall the detector.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    if a.len().saturating_mul(b.len()) > 250_000 {
        return a.len().abs_diff(b.len());
    }

    // Three rows: the current, the previous, and the one before that, which the
    // transposition rule reads.
    let mut two_back = vec![0usize; b.len() + 1];
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];

    for i in 1..=a.len() {
        current[0] = i;

        for j in 1..=b.len() {
            let substitution = previous[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            let insertion = current[j - 1] + 1;
            let deletion = previous[j] + 1;
            let mut best = substitution.min(insertion).min(deletion);

            // A transposition of a[i-2],a[i-1] against b[j-2],b[j-1].
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(two_back[j - 2] + 1);
            }

            current[j] = best;
        }

        std::mem::swap(&mut two_back, &mut previous);
        std::mem::swap(&mut previous, &mut current);
    }

    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector_with_blocklist(domains: &[&str]) -> PhishingDetector {
        let mut detector = PhishingDetector::new();
        for domain in domains {
            detector.block(domain);
        }
        detector
    }

    #[test]
    fn the_real_domain_is_never_flagged() {
        let detector = detector_with_blocklist(&[]);

        assert!(detector.check_domain("tigersmartchain.com").is_none());
        assert!(detector.check_domain("https://tigersmartchain.com/swap").is_none());
        assert!(detector.check_domain("scan.tigersmartchain.com").is_none());
        assert!(detector.check_domain("TigerSmartChain.com").is_none());
    }

    #[test]
    fn an_unrelated_domain_is_not_flagged() {
        let detector = detector_with_blocklist(&[]);

        assert!(detector.check_domain("google.com").is_none());
        assert!(detector.check_domain("github.com").is_none());
        assert!(detector.check_domain("example.org").is_none());
    }

    #[test]
    fn a_blocklisted_domain_is_flagged_with_full_confidence() {
        let detector = detector_with_blocklist(&["tigersmartchaiin.com"]);

        let found = detector.check_domain("tigersmartchaiin.com").unwrap();
        assert_eq!(found.detection_method, "blocklist");
        assert_eq!(found.confidence, 1.0);
        assert!(found.reported);
    }

    #[test]
    fn a_one_character_misspelling_is_a_typosquat() {
        let detector = detector_with_blocklist(&[]);

        // "tigersmartchian" transposes two letters of the real name.
        let found = detector.check_domain("tigersmartchian.com").unwrap();
        assert_eq!(found.detection_method, "typosquatting");
        assert_eq!(found.target, "TigerSmartChain");
        assert!(found.confidence >= 0.9);
    }

    #[test]
    fn a_digit_for_letter_substitution_is_a_homoglyph() {
        let detector = detector_with_blocklist(&[]);

        let found = detector.check_domain("t1gersmartchain.com").unwrap();
        assert_eq!(found.detection_method, "homoglyph");
        assert_eq!(found.target, "TigerSmartChain");
    }

    #[test]
    fn the_brand_on_a_foreign_suffix_is_flagged() {
        let detector = detector_with_blocklist(&[]);

        let found = detector.check_domain("tigersmartchain.xyz").unwrap();
        assert_eq!(found.detection_method, "brand_on_foreign_domain");
        assert_eq!(found.target, "TigerSmartChain");
    }

    #[test]
    fn a_brand_buried_in_a_longer_name_is_flagged() {
        let detector = detector_with_blocklist(&[]);

        let found = detector.check_domain("tigersmartchain-defi.com").unwrap();
        assert_eq!(found.target, "TigerSmartChain");
    }

    #[test]
    fn a_similar_but_unrelated_domain_is_not_flagged() {
        let detector = detector_with_blocklist(&[]);

        // Four edits away: the names are simply different.
        assert!(detector.check_domain("tigerfoods.com").is_none());
    }

    #[test]
    fn a_peers_real_domain_is_protected_too() {
        let detector = detector_with_blocklist(&[]);

        assert!(detector.check_domain("binance.com").is_none());
        assert!(detector.check_domain("bnbchain.org").is_none());

        // But a lookalike of it is not.
        assert!(detector.check_domain("binancee.com").is_some());
    }

    #[test]
    fn a_subdomain_of_a_protected_domain_is_protected() {
        let detector = detector_with_blocklist(&[]);

        assert!(detector.check_domain("app.tigersmartchain.com").is_none());
        assert!(detector.check_domain("a.b.tigersmartchain.com").is_none());
    }

    #[test]
    fn a_url_with_userinfo_is_checked_on_the_real_host() {
        let detector = detector_with_blocklist(&["evil.example"]);

        // The host is evil.example; tigersmartchain.com is only the userinfo.
        let found = detector
            .analyze_url("https://tigersmartchain.com@evil.example/login")
            .unwrap();
        assert_eq!(found.domain, "evil.example");
    }

    #[test]
    fn an_ip_address_is_not_treated_as_a_domain() {
        let detector = detector_with_blocklist(&[]);

        assert!(detector.check_domain("192.0.2.1").is_none());
        assert!(detector.analyze_url("http://192.0.2.1/login").is_none());
    }

    #[test]
    fn a_trailing_dot_and_mixed_case_normalize() {
        let detector = detector_with_blocklist(&["evil.example"]);

        assert!(detector.check_domain("EVIL.example.").is_some());
    }

    #[test]
    fn a_feed_body_adds_domains_and_skips_protected_ones() {
        let mut detector = PhishingDetector::new();

        let feed = "\
# comment
evil.example
http://phish.example/login
tigersmartchain.com
binance.com

";
        let added = detector.ingest_feed(feed);

        assert_eq!(added, 2);
        assert!(detector.check_domain("evil.example").is_some());
        assert!(detector.check_domain("phish.example").is_some());
        // The protected domains in the feed were ignored.
        assert!(detector.check_domain("tigersmartchain.com").is_none());
        assert!(detector.check_domain("binance.com").is_none());
    }

    #[test]
    fn the_same_domain_is_only_counted_once() {
        let mut detector = PhishingDetector::new();

        assert_eq!(detector.ingest_feed("evil.example\nevil.example\n"), 1);
        assert_eq!(detector.blocked_count(), 1);
    }

    #[test]
    fn registrable_domain_handles_plain_and_multi_label_suffixes() {
        assert_eq!(registrable_domain("tigersmartchain.com"), "tigersmartchain.com");
        assert_eq!(registrable_domain("scan.tigersmartchain.com"), "tigersmartchain.com");
        assert_eq!(registrable_domain("a.b.c.example.co.uk"), "example.co.uk");
        assert_eq!(registrable_domain("localhost"), "localhost");
    }

    #[test]
    fn levenshtein_is_correct_on_known_pairs() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("abc", "abd"), 1);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn homoglyph_folding_undoes_substitutions() {
        assert_eq!(fold_homoglyphs("t1gersm4rtcha1n"), "tigersmartchain");
        assert_eq!(fold_homoglyphs("tiger-smartchain"), "tigersmartchain");
        assert_eq!(fold_homoglyphs("rn"), "m");
        assert_eq!(fold_homoglyphs("TIGERSMARTCHAIN"), "tigersmartchain");
    }

    #[test]
    fn an_empty_or_malformed_input_is_rejected() {
        let detector = PhishingDetector::new();

        assert!(detector.check_domain("").is_none());
        assert!(detector.check_domain("   ").is_none());
        assert!(detector.check_domain("http://").is_none());
    }

    #[test]
    fn punycode_and_unicode_forms_of_a_name_agree() {
        let detector = detector_with_blocklist(&[]);

        // U+00ED, "i" with an acute, normalizes to its punycode form, so the
        // accented name and the ASCII-encoded form are compared as one
        // lookalike rather than one of them slipping through as an unknown
        // string.
        let unicode = detector.check_domain("tigersmartcha\u{ed}n.com");
        let punycode = detector.check_domain("xn--tigersmartchan-dmb.com");

        assert!(unicode.is_some());
        assert_eq!(
            unicode.unwrap().detection_method,
            punycode.unwrap().detection_method
        );
    }
}
