package gateway

import (
	"net/http"

	"github.com/gin-gonic/gin"
)

// Security centre aggregates.
//
// The security centre page used to invent everything: eight alerts with made-up
// addresses (several of them not valid hex), six threat contracts with names
// like "FakeToken" and victim counts, five phishing domains, and a list of
// "major exploit events" naming PolyNetwork, Wormhole, Ronin, Euler and Curve
// with multi-hundred-million dollar figures. Those are real incidents on other
// chains. Presenting them on TigerScan, next to this chain's own contract
// counts, reads as if they happened here.
//
// The page now reports only what the indexer holds: the malicious_contracts,
// phishing_urls and scam_tokens tables, and the verification state of this
// chain's own contracts and tokens.

// GetSecurityOverview returns the counts behind the security centre's gauges,
// together with the verification coverage of this chain's own contracts and
// tokens.
//
// GET /security/overview
func (h *Handler) GetSecurityOverview(c *gin.Context) {
	ctx := c.Request.Context()

	threats, err := h.queryOne(ctx, `
		SELECT
			(SELECT count(*) FROM malicious_contracts WHERE is_active) AS malicious_active,
			(SELECT count(*) FROM malicious_contracts) AS malicious_total,
			(SELECT count(*) FROM phishing_urls WHERE is_active) AS phishing_active,
			(SELECT count(*) FROM phishing_urls) AS phishing_total,
			(SELECT count(*) FROM scam_tokens WHERE is_active) AS scam_active,
			(SELECT count(*) FROM scam_tokens) AS scam_total,
			(SELECT count(*) FROM malicious_contracts WHERE is_active AND threat_type = 'honeypot') AS honeypots,
			(SELECT COALESCE(AVG(confidence), 0) FROM malicious_contracts WHERE is_active) AS average_confidence
	`)
	if err != nil {
		dbError(c, err)
		return
	}

	// Verification coverage is the one security number this chain can state
	// about itself without a third-party feed: how much of the deployed code has
	// published source.
	coverage, err := h.queryOne(ctx, `
		SELECT
			(SELECT count(*) FROM contracts) AS contracts_total,
			(SELECT count(*) FROM contracts WHERE is_verified) AS contracts_verified,
			(SELECT count(*) FROM tokens) AS tokens_total,
			(SELECT count(*) FROM tokens WHERE is_verified) AS tokens_verified,
			(SELECT count(*) FROM tokens WHERE is_spam) AS tokens_flagged_spam
	`)
	if err != nil {
		dbError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"threats": gin.H{
			"maliciousActive":   rowValue(threats, "malicious_active"),
			"maliciousTotal":    rowValue(threats, "malicious_total"),
			"phishingActive":    rowValue(threats, "phishing_active"),
			"phishingTotal":     rowValue(threats, "phishing_total"),
			"scamActive":        rowValue(threats, "scam_active"),
			"scamTotal":         rowValue(threats, "scam_total"),
			"honeypots":         rowValue(threats, "honeypots"),
			"averageConfidence": rowValue(threats, "average_confidence"),
		},
		"coverage": gin.H{
			"contractsTotal":    rowValue(coverage, "contracts_total"),
			"contractsVerified": rowValue(coverage, "contracts_verified"),
			"tokensTotal":       rowValue(coverage, "tokens_total"),
			"tokensVerified":    rowValue(coverage, "tokens_verified"),
			"tokensFlaggedSpam": rowValue(coverage, "tokens_flagged_spam"),
		},
	})
}

// GetSecurityThreatSeries returns, for each of the last 14 days, how many
// threats were first recorded. first_seen is a unix timestamp on all three
// tables, so the same expression works for each.
//
// GET /security/threat-series
func (h *Handler) GetSecurityThreatSeries(c *gin.Context) {
	ctx := c.Request.Context()
	days := paramInt(c, "days", 14)
	since := nowSeconds() - int64(days)*24*60*60

	rows, err := h.queryRows(ctx, `
		WITH buckets AS (
			SELECT to_timestamp(floor($1::bigint / 86400) * 86400) - (n * interval '1 day') AS day
			FROM generate_series(0, $2::int - 1) AS n
		),
		events AS (
			SELECT first_seen, 'malicious' AS kind FROM malicious_contracts WHERE first_seen >= $1
			UNION ALL
			SELECT first_seen, 'phishing' FROM phishing_urls WHERE first_seen >= $1
			UNION ALL
			SELECT first_seen, 'scam' FROM scam_tokens WHERE first_seen >= $1
		)
		SELECT
			b.day,
			COALESCE(SUM((e.kind = 'malicious')::int), 0) AS malicious,
			COALESCE(SUM((e.kind = 'phishing')::int), 0) AS phishing,
			COALESCE(SUM((e.kind = 'scam')::int), 0) AS scam,
			COUNT(e.kind) AS total
		FROM buckets b
		LEFT JOIN events e
			ON e.first_seen >= extract(epoch FROM b.day)
			AND e.first_seen < extract(epoch FROM b.day) + 86400
		GROUP BY b.day
		ORDER BY b.day ASC
	`, since, days)
	if err != nil {
		dbError(c, err)
		return
	}

	respondList(c, rows, len(rows))
}
