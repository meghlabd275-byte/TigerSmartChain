package gateway

import (
	"encoding/hex"
	"strings"
	"testing"
)

func TestGenerateAPIKeyIsPrefixedAndHighEntropy(t *testing.T) {
	key, hash, err := GenerateAPIKey()
	if err != nil {
		t.Fatalf("GenerateAPIKey: %v", err)
	}

	if !strings.HasPrefix(key, APIKeyPrefix) {
		t.Fatalf("key %q is missing the %q prefix", key, APIKeyPrefix)
	}
	if len(key) != len(APIKeyPrefix)+apiKeyRandomBytes*2 {
		t.Fatalf("key has length %d, want %d", len(key), len(APIKeyPrefix)+apiKeyRandomBytes*2)
	}
	if !IsAPIKeyShape(key) {
		t.Fatalf("a freshly generated key %q should have a valid shape", key)
	}

	// The hash is SHA-256, so 32 bytes rendered as 64 hex characters.
	if len(hash) != 64 {
		t.Fatalf("hash has length %d, want 64", len(hash))
	}
	if _, err := hex.DecodeString(hash); err != nil {
		t.Fatalf("hash is not hex: %v", err)
	}
	if strings.Contains(hash, key) {
		t.Fatal("the hash must not contain the plaintext key")
	}
}

func TestGenerateAPIKeyNeverRepeats(t *testing.T) {
	const n = 512
	seen := make(map[string]struct{}, n)
	for i := 0; i < n; i++ {
		key, _, err := GenerateAPIKey()
		if err != nil {
			t.Fatalf("GenerateAPIKey: %v", err)
		}
		if _, dup := seen[key]; dup {
			t.Fatalf("key %q was generated twice", key)
		}
		seen[key] = struct{}{}
	}
}

func TestHashAPIKeyIsStableAndPrefixInsensitive(t *testing.T) {
	key, hash, err := GenerateAPIKey()
	if err != nil {
		t.Fatalf("GenerateAPIKey: %v", err)
	}

	if HashAPIKey(key) != hash {
		t.Fatal("HashAPIKey(key) should equal the hash returned by GenerateAPIKey")
	}
	// Hashing the body alone must give the same digest, so a key can never be
	// stored under two different hashes.
	body := strings.TrimPrefix(key, APIKeyPrefix)
	if HashAPIKey(body) != hash {
		t.Fatal("the prefix must not change the digest")
	}
	if HashAPIKey("  "+key+"  ") != hash {
		t.Fatal("surrounding whitespace must not change the digest")
	}
}

func TestHashAPIKeySeparatesDifferentKeys(t *testing.T) {
	a, _, err := GenerateAPIKey()
	if err != nil {
		t.Fatalf("GenerateAPIKey: %v", err)
	}
	b, _, err := GenerateAPIKey()
	if err != nil {
		t.Fatalf("GenerateAPIKey: %v", err)
	}
	if HashAPIKey(a) == HashAPIKey(b) {
		t.Fatal("two distinct keys must not share a hash")
	}
}

func TestIsAPIKeyShapeRejectsMalformedInput(t *testing.T) {
	cases := map[string]string{
		"empty":             "",
		"no prefix":         strings.Repeat("a", 64),
		"wrong prefix":      "sk_" + strings.Repeat("a", 64),
		"too short":         APIKeyPrefix + strings.Repeat("a", 63),
		"too long":          APIKeyPrefix + strings.Repeat("a", 65),
		"non-hex body":      APIKeyPrefix + strings.Repeat("z", 64),
		"prefix only":       APIKeyPrefix,
		"sql in the body":   APIKeyPrefix + "' OR 1=1 --",
		"newline smuggling": APIKeyPrefix + strings.Repeat("a", 63) + "\n",
	}

	for name, value := range cases {
		if IsAPIKeyShape(value) {
			t.Errorf("%s: %q should be rejected", name, value)
		}
	}
}

func TestExtractAPIKeyPrefersBearerThenHeader(t *testing.T) {
	cases := []struct {
		name          string
		authorization string
		xAPIKey       string
		want          string
	}{
		{"bearer", "Bearer tsk_abc", "", "tsk_abc"},
		{"x-api-key", "", "tsk_def", "tsk_def"},
		{"bearer wins", "Bearer tsk_abc", "tsk_def", "tsk_abc"},
		{"neither", "", "", ""},
		{"non-bearer scheme is ignored", "Basic dXNlcjpwYXNz", "", ""},
		{"bearer with padding is trimmed", "Bearer   tsk_abc  ", "", "tsk_abc"},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			c := newTestContext(t)
			if tc.authorization != "" {
				c.Request.Header.Set("Authorization", tc.authorization)
			}
			if tc.xAPIKey != "" {
				c.Request.Header.Set("X-API-Key", tc.xAPIKey)
			}
			if got := ExtractAPIKey(c); got != tc.want {
				t.Fatalf("ExtractAPIKey = %q, want %q", got, tc.want)
			}
		})
	}
}
