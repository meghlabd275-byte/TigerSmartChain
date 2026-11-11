package gateway

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
)

// API keys are bearer credentials, so the plaintext is shown to the caller
// exactly once and only a hash is persisted. A database read therefore never
// yields a usable key, and a leaked dump cannot be replayed against the API.
//
// The hash is a plain SHA-256 rather than a password hash because a key is 32
// bytes of CSPRNG output, not a human-chosen secret: there is no dictionary to
// attack, and the lookup has to be a single indexed equality against
// `api_keys.key_hash VARCHAR(64)`.

// APIKeyPrefix marks a TigerSmartChain API key.
const APIKeyPrefix = "tsk_"

// apiKeyRandomBytes is the entropy of a generated key. 32 bytes (256 bits) puts
// brute force far out of reach.
const apiKeyRandomBytes = 32

// APIKeyTTL is how long a positive validation result is cached in Redis. A
// short TTL keeps the hot path off the database while still noticing a revoked
// key promptly.
const APIKeyTTL = 60 * time.Second

// ErrAPIKeyNotFound is returned when a presented key is unknown, revoked or
// expired.
var ErrAPIKeyNotFound = errors.New("api key is unknown, revoked or expired")

// APIKey is the metadata of an issued key. The plaintext is deliberately absent.
type APIKey struct {
	ID         int64      `json:"id"`
	Name       string     `json:"name"`
	RateLimit  int        `json:"rate_limit"`
	DailyLimit int64      `json:"daily_limit"`
	IsActive   bool       `json:"is_active"`
	ExpiresAt  *time.Time `json:"expires_at,omitempty"`
}

// GenerateAPIKey returns a new plaintext key and the hash to persist.
func GenerateAPIKey() (plaintext string, hash string, err error) {
	buf := make([]byte, apiKeyRandomBytes)
	if _, err := rand.Read(buf); err != nil {
		return "", "", fmt.Errorf("generate api key: %w", err)
	}
	plaintext = APIKeyPrefix + hex.EncodeToString(buf)
	return plaintext, HashAPIKey(plaintext), nil
}

// HashAPIKey returns the lowercase hex SHA-256 of a key. It accepts the key
// with or without the `tsk_` prefix so a caller cannot accidentally create two
// hashes for the same credential.
func HashAPIKey(key string) string {
	key = strings.TrimSpace(key)
	key = strings.TrimPrefix(key, APIKeyPrefix)
	sum := sha256.Sum256([]byte(key))
	return hex.EncodeToString(sum[:])
}

// IsAPIKeyShape reports whether a string looks like a TigerSmartChain key, so
// malformed input is rejected before it reaches the database.
func IsAPIKeyShape(key string) bool {
	key = strings.TrimSpace(key)
	if !strings.HasPrefix(key, APIKeyPrefix) {
		return false
	}
	body := strings.TrimPrefix(key, APIKeyPrefix)
	if len(body) != apiKeyRandomBytes*2 {
		return false
	}
	_, err := hex.DecodeString(body)
	return err == nil
}

// ExtractAPIKey reads the credential from the request headers. `Authorization:
// Bearer <key>` takes precedence over `X-API-Key`.
func ExtractAPIKey(c *gin.Context) string {
	if header := c.GetHeader("Authorization"); header != "" {
		if rest, ok := strings.CutPrefix(header, "Bearer "); ok {
			return strings.TrimSpace(rest)
		}
	}
	return strings.TrimSpace(c.GetHeader("X-API-Key"))
}

// AuthenticateAPIKey resolves a presented key to its metadata, or returns
// ErrAPIKeyNotFound. The hash is compared with `subtle.ConstantTimeCompare` so
// a caller cannot learn the stored hash a byte at a time from response timing.
func (h *Handler) AuthenticateAPIKey(ctx context.Context, presented string) (*APIKey, error) {
	if !IsAPIKeyShape(presented) {
		return nil, ErrAPIKeyNotFound
	}

	want := HashAPIKey(presented)

	// The cache is keyed by the presented key's hash, so a cache hit can never
	// return another key's metadata.
	cacheKey := "apikey:" + want
	if h.redis != nil {
		if cached, err := h.redis.HGetAll(ctx, cacheKey).Result(); err == nil && len(cached) > 0 {
			if !constantTimeEqual(cached["hash"], want) {
				return nil, ErrAPIKeyNotFound
			}
			return apiKeyFromCache(cached), nil
		}
	}

	if h.pool == nil {
		return nil, fmt.Errorf("api key validation requires a database connection")
	}

	var (
		id         int64
		storedHash string
		name       string
		rateLimit  int
		dailyLimit *int64
		isActive   bool
		expiresAt  *time.Time
	)
	err := h.pool.QueryRow(ctx,
		`SELECT id, key_hash, key_name, rate_limit, daily_limit, is_active, expires_at
		   FROM api_keys WHERE key_hash = $1`, want,
	).Scan(&id, &storedHash, &name, &rateLimit, &dailyLimit, &isActive, &expiresAt)
	if err != nil {
		return nil, ErrAPIKeyNotFound
	}

	if subtle.ConstantTimeCompare([]byte(storedHash), []byte(want)) != 1 {
		return nil, ErrAPIKeyNotFound
	}
	if !isActive {
		return nil, ErrAPIKeyNotFound
	}
	if expiresAt != nil && expiresAt.Before(time.Now()) {
		return nil, ErrAPIKeyNotFound
	}

	key := &APIKey{
		ID:         id,
		Name:       name,
		RateLimit:  rateLimit,
		DailyLimit: dailyLimitValue(dailyLimit),
		IsActive:   isActive,
		ExpiresAt:  expiresAt,
	}

	if h.redis != nil {
		_ = h.redis.HSet(ctx, cacheKey, map[string]interface{}{
			"hash":        want,
			"id":          id,
			"name":        name,
			"rate_limit":  rateLimit,
			"daily_limit": key.DailyLimit,
			"is_active":   boolToInt(isActive),
		}).Err()
		_ = h.redis.Expire(ctx, cacheKey, APIKeyTTL).Err()
	}

	return key, nil
}

// InvalidateAPIKeyCache drops the cached validation of a key, so a revoke or a
// rate-limit change takes effect immediately instead of after APIKeyTTL.
func (h *Handler) InvalidateAPIKeyCache(ctx context.Context, plaintext string) {
	if h.redis == nil {
		return
	}
	_ = h.redis.Del(ctx, "apikey:"+HashAPIKey(plaintext)).Err()
}

func constantTimeEqual(a, b string) bool {
	return subtle.ConstantTimeCompare([]byte(a), []byte(b)) == 1
}

func boolToInt(v bool) int {
	if v {
		return 1
	}
	return 0
}

func dailyLimitValue(v *int64) int64 {
	if v == nil {
		return 0
	}
	return *v
}

func apiKeyFromCache(cached map[string]string) *APIKey {
	id, _ := parseInt64(cached["id"])
	rateLimit, _ := parseInt64(cached["rate_limit"])
	dailyLimit, _ := parseInt64(cached["daily_limit"])
	return &APIKey{
		ID:         id,
		Name:       cached["name"],
		RateLimit:  int(rateLimit),
		DailyLimit: dailyLimit,
		IsActive:   cached["is_active"] == "1",
	}
}

func parseInt64(s string) (int64, error) {
	var out int64
	if s == "" {
		return 0, nil
	}
	for _, r := range s {
		if r < '0' || r > '9' {
			return 0, errors.New("not a number")
		}
		out = out*10 + int64(r-'0')
	}
	return out, nil
}
