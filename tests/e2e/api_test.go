// Package e2e provides end-to-end tests for TigerScan
package e2e

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"tigersmartchain/explorer/apps/api-pro"
	"tigersmartchain/explorer/apps/export"
	"tigersmartchain/explorer/apps/graphs"
	"tigersmartchain/explorer/apps/api-ws"
)

// TestConfig holds test configuration
type TestConfig struct {
	APIServerURL  string
	RPCServerURL string
	DBURL       string
	RedisURL    string
	APIKey      string
}

// Global test config
var config = &TestConfig{
	APIServerURL:  getEnv("TEST_API_URL", "http://localhost:8080"),
	RPCServerURL: getEnv("TEST_RPC_URL", "http://localhost:8545"),
	DBURL:       getEnv("TEST_DB_URL", "postgres://localhost:5432/tigerscan"),
	RedisURL:    getEnv("TEST_REDIS_URL", "redis://localhost:6379"),
	APIKey:      getEnv("TEST_API_KEY", "test-api-key"),
}

func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}

// TestMain runs before all tests
func TestMain(m *testing.M) {
	// Set test mode
	gin.SetMode(gin.TestMode)
	
	// Run tests
	os.Exit(m.Run())
}

// Integration tests for API Pro

func TestAPIPro_Integration(t *testing.T) {
	t.Skip("Requires running API server")
	
	t.Run("TokenList", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/tokens?page=1", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
		
		var result map[string]interface{}
		err = json.NewDecoder(resp.Body).Decode(&result)
		require.NoError(t, err)
		
		assert.Equal(t, "1", result["status"])
	})
	
	t.Run("TokenInfo", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/token/0x1234567890123456789012345678901234567890", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("TokenHolders", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/token/0x1234567890123456789012345678901234567890/holders?page=1", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("NFTList", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/nfts?page=1", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("BlockList", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/blocks?page=1", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("TransactionList", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/txs?page=1", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("NetworkStats", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/stats", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("GasTracker", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/stats/gas", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
}

// Integration tests for Export API

func TestExportAPI_Integration(t *testing.T) {
	t.Skip("Requires running API server")
	
	t.Run("ExportTransactions", func(t *testing.T) {
		url := fmt.Sprintf("%s/export?type=transactions&format=json&limit=100", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
		
		var result map[string]interface{}
		err = json.NewDecoder(resp.Body).Decode(&result)
		require.NoError(t, err)
		
		assert.Contains(t, result, "totalRecords")
	})
	
	t.Run("ExportTokenTransfers", func(t *testing.T) {
		url := fmt.Sprintf("%s/export?type=token_transfers&address=0x123&format=json", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("ExportBlocks", func(t *testing.T) {
		url := fmt.Sprintf("%s/export?type=blocks&format=csv", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("InvalidType", func(t *testing.T) {
		url := fmt.Sprintf("%s/export?type=invalid", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusBadRequest, resp.StatusCode)
	})
	
	t.Run("RateLimit", func(t *testing.T) {
		// Make multiple requests to test rate limiting
		for i := 0; i < 10; i++ {
			resp, err := http.Get(fmt.Sprintf("%s/api/v2/tokens", config.APIServerURL))
			require.NoError(t, err)
			resp.Body.Close()
			
			if resp.StatusCode == http.StatusTooManyRequests {
				t.Logf("Rate limited after %d requests", i+1)
				break
			}
		}
	})
}

// Integration tests for Graph API

func TestGraphAPI_Integration(t *testing.T) {
	t.Skip("Requires running API server")
	
	t.Run("TPSChart", func(t *testing.T) {
		url := fmt.Sprintf("%s/charts/tps?interval=24h", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
		
		var result map[string]interface{}
		err = json.NewDecoder(resp.Body).Decode(&result)
		require.NoError(t, err)
		
		assert.Contains(t, result, "series")
	})
	
	t.Run("GasChart", func(t *testing.T) {
		url := fmt.Sprintf("%s/charts/gas?interval=24h", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("TVLChart", func(t *testing.T) {
		url := fmt.Sprintf("%s/charts/tvl?interval=30d", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("TokenPriceChart", func(t *testing.T) {
		url := fmt.Sprintf("%s/charts/token/0x123?interval=30d", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("TransactionVolume", func(t *testing.T) {
		url := fmt.Sprintf("%s/charts/volume?interval=30d", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("GasHeatmap", func(t *testing.T) {
		url := fmt.Sprintf("%s/charts/heatmap?days=7", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
}

// Integration tests for WebSocket API

func TestWebSocketAPI_Integration(t *testing.T) {
	t.Skip("Requires running WebSocket server")
	
	t.Run("Connection", func(t *testing.T) {
		// This would require a WebSocket client
		// For now, just test the HTTP endpoints
		url := fmt.Sprintf("%s/ws/metrics", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("EventSubscriptions", func(t *testing.T) {
		url := fmt.Sprintf("%s/ws/subscriptions", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
}

// Integration tests for API Security

func TestAPISecurity_Integration(t *testing.T) {
	t.Skip("Requires running API server")
	
	t.Run("MissingAPIKey", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/tokens", config.APIServerURL)
		req, _ := http.NewRequest("GET", url, nil)
		
		client := &http.Client{}
		resp, err := client.Do(req)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		// Should return 401 or redirect
		assert.True(t, resp.StatusCode == http.StatusUnauthorized || resp.StatusCode == http.StatusFound)
	})
	
	t.Run("InvalidAPIKey", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/tokens", config.APIServerURL)
		req, _ := http.NewRequest("GET", url, nil)
		req.Header.Set("X-API-Key", "invalid-key")
		
		client := &http.Client{}
		resp, err := client.Do(req)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusUnauthorized, resp.StatusCode)
	})
	
	t.Run("ValidAPIKey", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/tokens", config.APIServerURL)
		req, _ := http.NewRequest("GET", url, nil)
		req.Header.Set("X-API-Key", config.APIKey)
		
		client := &http.Client{}
		resp, err := client.Do(req)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("SQLInjection", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/token/%s", config.APIServerURL, "1' OR '1'='1")
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		// Should not return SQL error
		assert.NotEqual(t, http.StatusInternalServerError, resp.StatusCode)
	})
	
	t.Run("XSSAttempt", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/token/%s", config.APIServerURL, "<script>alert(1)</script>")
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		// Should sanitize or reject
		assert.True(t, resp.StatusCode == http.StatusBadRequest || resp.StatusCode == http.StatusOK)
	})
}

// Integration tests for API Rate Limiting

func TestAPIRateLimiting_Integration(t *testing.T) {
	t.Skip("Requires running API server")
	
	t.Run("RateLimitEnforcement", func(t *testing.T) {
		// Make requests until rate limited
		successCount := 0
		rateLimited := false
		
		for i := 0; i < 100; i++ {
			resp, err := http.Get(fmt.Sprintf("%s/api/v2/tokens?page=%d", config.APIServerURL, i))
			if err != nil {
				break
			}
			resp.Body.Close()
			
			if resp.StatusCode == http.StatusTooManyRequests {
				rateLimited = true
				break
			}
			if resp.StatusCode == http.StatusOK {
				successCount++
			}
		}
		
		t.Logf("Successful requests: %d, Rate limited: %v", successCount, rateLimited)
	})
	
	t.Run("RateLimitHeaders", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/tokens", config.APIServerURL)
		req, _ := http.NewRequest("GET", url, nil)
		req.Header.Set("X-API-Key", config.APIKey)
		
		client := &http.Client{}
		resp, err := client.Do(req)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		// Check for rate limit headers
		remaining := resp.Header.Get("X-RateLimit-Remaining")
		limit := resp.Header.Get("X-RateLimit-Limit")
		
		t.Logf("Rate limit: %s, Remaining: %s", limit, remaining)
	})
	
	t.Run("RateLimitReset", func(t *testing.T) {
		// Wait for rate limit to reset
		t.Log("Waiting 60 seconds for rate limit reset...")
		time.Sleep(60 * time.Second)
		
		url := fmt.Sprintf("%s/api/v2/tokens", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
}

// Integration tests for API Response Times

func TestAPIResponseTimes_Integration(t *testing.T) {
	t.Skip("Requires running API server")
	
	t.Run("TokenListResponseTime", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/tokens", config.APIServerURL)
		
		start := time.Now()
		resp, err := http.Get(url)
		elapsed := time.Since(start)
		
		require.NoError(t, err)
		defer resp.Body.Close()
		
		t.Logf("Token list response time: %v", elapsed)
		assert.True(t, elapsed < 5*time.Second, "Response time too slow: %v", elapsed)
	})
	
	t.Run("TokenInfoResponseTime", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/token/0x1234567890123456789012345678901234567890", config.APIServerURL)
		
		start := time.Now()
		resp, err := http.Get(url)
		elapsed := time.Since(start)
		
		require.NoError(t, err)
		defer resp.Body.Close()
		
		t.Logf("Token info response time: %v", elapsed)
		assert.True(t, elapsed < 2*time.Second, "Response time too slow: %v", elapsed)
	})
}

// Integration tests for API Caching

func TestAPICaching_Integration(t *testing.T) {
	t.Skip("Requires running API server")
	
	t.Run("CacheHits", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/tokens", config.APIServerURL)
		
		// First request - cache miss
		start1 := time.Now()
		resp1, _ := http.Get(url)
		elapsed1 := time.Since(start1)
		resp1.Body.Close()
		
		// Second request - cache hit
		start2 := time.Now()
		resp2, _ := http.Get(url)
		elapsed2 := time.Since(start2)
		resp2.Body.Close()
		
		t.Logf("First request: %v, Second request: %v", elapsed1, elapsed2)
		assert.True(t, elapsed2 < elapsed1, "Second request should be faster due to caching")
	})
}

// Integration tests for API Error Handling

func TestAPIErrorHandling_Integration(t *testing.T) {
	t.Skip("Requires running API server")
	
	t.Run("InvalidAddress", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/token/0xinvalid", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusBadRequest, resp.StatusCode)
	})
	
	t.Run("InvalidPage", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/tokens?page=-1", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusBadRequest, resp.StatusCode)
	})
	
	t.Run("LargePage", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/tokens?page=99999999", config.APIServerURL)
		resp, err := http.Get(url)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusOK, resp.StatusCode)
	})
	
	t.Run("MalformedJSON", func(t *testing.T) {
		url := fmt.Sprintf("%s/api/v2/tokens", config.APIServerURL)
		req, _ := http.NewRequest("POST", url, strings.NewReader("{invalid json}"))
		req.Header.Set("Content-Type", "application/json")
		
		client := &http.Client{}
		resp, err := client.Do(req)
		require.NoError(t, err)
		defer resp.Body.Close()
		
		assert.Equal(t, http.StatusBadRequest, resp.StatusCode)
	})
}

// Benchmark tests

func BenchmarkTokenListAPI(b *testing.B) {
	b.Skip("Requires running API server")
	
	url := fmt.Sprintf("%s/api/v2/tokens", config.APIServerURL)
	
	for i := 0; i < b.N; i++ {
		resp, _ := http.Get(url)
		if resp != nil {
			resp.Body.Close()
		}
	}
}

func BenchmarkTokenInfoAPI(b *testing.B) {
	b.Skip("Requires running API server")
	
	url := fmt.Sprintf("%s/api/v2/token/0x1234567890123456789012345678901234567890", config.APIServerURL)
	
	for i := 0; i < b.N; i++ {
		resp, _ := http.Get(url)
		if resp != nil {
			resp.Body.Close()
		}
	}
}