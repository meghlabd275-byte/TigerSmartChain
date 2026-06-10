// Package databases provides database interfaces for TigerSmartChain explorer.
// This is a production-ready implementation with Redis caching.
package databases

import (
	"context"
	"encoding/json"
	"fmt"
	"time"

	"github.com/redis/go-redis/v9"
)

// RedisConfig holds Redis connection configuration.
type RedisConfig struct {
	// Addr is the Redis address (host:port)
	Addr string
	// Password for Redis authentication
	Password string
	// DB number (0-15)
	DB int
	// PoolSize for connections
	PoolSize int
	// MinIdleConns minimum idle connections
	MinIdleConns int
}

// CacheService provides Redis caching for the explorer.
type CacheService struct {
	client *redis.Client
	config *RedisConfig
}

// NewCacheService creates a new Redis cache service.
func NewCacheService(config *RedisConfig) (*CacheService, error) {
	client := redis.NewClient(&redis.Options{
		Addr:     config.Addr,
		Password: config.Password,
		DB:       config.DB,
		PoolSize: config.PoolSize,
	})

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := client.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to Redis: %w", err)
	}

	return &CacheService{
		client: client,
		config: config,
	}, nil
}

// =============================================================================
// BLOCK CACHING
// =============================================================================

const (
	blockCacheTTL       = 5 * time.Minute
	blockCachePrefix     = "block:"
	latestBlockKey      = "latest:block"
	blockCountKey       = "blocks:count"
)

// CacheBlock caches a block.
func (c *CacheService) CacheBlock(blockNum uint64, data interface{}) error {
	key := fmt.Sprintf("%s%d", blockCachePrefix, blockNum)
	jsonData, err := json.Marshal(data)
	if err != nil {
		return err
	}
	return c.client.Set(context.Background(), key, jsonData, blockCacheTTL).Err()
}

// GetCachedBlock gets a cached block.
func (c *CacheService) GetCachedBlock(blockNum uint64, result interface{}) error {
	key := fmt.Sprintf("%s%d", blockCachePrefix, blockNum)
	data, err := c.client.Get(context.Background(), key).Result()
	if err != nil {
		return err
	}
	return json.Unmarshal([]byte(data), result)
}

// CacheLatestBlock caches the latest block number.
func (c *CacheService) CacheLatestBlock(blockNum uint64) error {
	return c.client.Set(context.Background(), latestBlockKey, blockNum, 0).Err()
}

// GetLatestBlock gets the cached latest block number.
func (c *CacheService) GetLatestBlock() (uint64, error) {
	val, err := c.client.Get(context.Background(), latestBlockKey).Uint64()
	return val, err
}

// =============================================================================
// TRANSACTION CACHING
// =============================================================================

const (
	txCacheTTL       = 10 * time.Minute
	txCachePrefix     = "tx:"
	txCountKey       = "txs:count"
)

// CacheTransaction caches a transaction.
func (c *CacheService) CacheTransaction(txHash string, data interface{}) error {
	key := fmt.Sprintf("%s%s", txCachePrefix, txHash)
	jsonData, err := json.Marshal(data)
	if err != nil {
		return err
	}
	return c.client.Set(context.Background(), key, jsonData, txCacheTTL).Err()
}

// GetCachedTransaction gets a cached transaction.
func (c *CacheService) GetCachedTransaction(txHash string, result interface{}) error {
	key := fmt.Sprintf("%s%s", txCachePrefix, txHash)
	data, err := c.client.Get(context.Background(), key).Result()
	if err != nil {
		return err
	}
	return json.Unmarshal([]byte(data), result)
}

// =============================================================================
// ADDRESS CACHING
// =============================================================================

const (
	addrCacheTTL     = 15 * time.Minute
	addrCachePrefix  = "addr:"
	addrBalancePrefix = "balance:"
)

// CacheAddressInfo caches address information.
func (c *CacheService) CacheAddressInfo(addr string, data interface{}) error {
	key := fmt.Sprintf("%s%s", addrCachePrefix, addr)
	jsonData, err := json.Marshal(data)
	if err != nil {
		return err
	}
	return c.client.Set(context.Background(), key, jsonData, addrCacheTTL).Err()
}

// GetCachedAddressInfo gets cached address information.
func (c *CacheService) GetCachedAddressInfo(addr string, result interface{}) error {
	key := fmt.Sprintf("%s%s", addrCachePrefix, addr)
	data, err := c.client.Get(context.Background(), key).Result()
	if err != nil {
		return err
	}
	return json.Unmarshal([]byte(data), result)
}

// CacheBalance caches address balance.
func (c *CacheService) CacheBalance(addr string, balance string) error {
	key := fmt.Sprintf("%s%s", addrBalancePrefix, addr)
	return c.client.Set(context.Background(), key, balance, addrCacheTTL).Err()
}

// GetCachedBalance gets cached balance.
func (c *CacheService) GetCachedBalance(addr string) (string, error) {
	key := fmt.Sprintf("%s%s", addrBalancePrefix, addr)
	return c.client.Get(context.Background(), key).Result()
}

// =============================================================================
// TOKEN CACHING
// =============================================================================

const (
	tokenCacheTTL     = 30 * time.Minute
	tokenCachePrefix = "token:"
	tokenHoldersPrefix = "holders:"
)

// CacheTokenInfo caches token information.
func (c *CacheService) CacheTokenInfo(tokenAddr string, data interface{}) error {
	key := fmt.Sprintf("%s%s", tokenCachePrefix, tokenAddr)
	jsonData, err := json.Marshal(data)
	if err != nil {
		return err
	}
	return c.client.Set(context.Background(), key, jsonData, tokenCacheTTL).Err()
}

// GetCachedTokenInfo gets cached token information.
func (c *CacheService) GetCachedTokenInfo(tokenAddr string, result interface{}) error {
	key := fmt.Sprintf("%s%s", tokenCachePrefix, tokenAddr)
	data, err := c.client.Get(context.Background(), key).Result()
	if err != nil {
		return err
	}
	return json.Unmarshal([]byte(data), result)
}

// =============================================================================
// BLOCKLIST CACHING
// =============================================================================

const (
	blocklistCacheTTL = time.Hour
	blocklistKey      = "blocklist"
)

// CacheBlocklist caches blocklist.
func (c *CacheService) CacheBlocklist(addrs []string) error {
	jsonData, err := json.Marshal(addrs)
	if err != nil {
		return err
	}
	return c.client.Set(context.Background(), blocklistKey, jsonData, blocklistCacheTTL).Err()
}

// GetBlocklist gets cached blocklist.
func (c *CacheService) GetBlocklist() ([]string, error) {
	data, err := c.client.Get(context.Background(), blocklistKey).Result()
	if err != nil {
		return nil, err
	}
	var addrs []string
	err = json.Unmarshal([]byte(data), &addrs)
	return addrs, err
}

// =============================================================================
// GAS PRICE CACHING
// =============================================================================

const (
	gasPriceCacheTTL = 30 * time.Second
	gasPriceKey      = "gas:price"
)

// CacheGasPrice caches gas price.
func (c *CacheService) CacheGasPrice(price uint64) error {
	return c.client.Set(context.Background(), gasPriceKey, price, gasPriceCacheTTL).Err()
}

// GetGasPrice gets cached gas price.
func (c *CacheService) GetGasPrice() (uint64, error) {
	return c.client.Get(context.Background(), gasPriceKey).Uint64()
}

// =============================================================================
// RATE LIMITING
// =============================================================================

const (
	rateLimitPrefix = "ratelimit:"
	rateLimitTTL   = 60 * time.Second
)

// CheckRateLimit checks if the request should be rate limited.
func (c *CacheService) CheckRateLimit(key string, limit int) (bool, error) {
	fullKey := fmt.Sprintf("%s%s", rateLimitPrefix, key)
	count, err := c.client.Incr(context.Background(), fullKey).Result()
	if err != nil {
		return false, err
	}

	if count == 1 {
		c.client.Expire(context.Background(), fullKey, rateLimitTTL)
	}

	return count <= int64(limit), nil
}

// =============================================================================
// GENERAL OPERATIONS
// =============================================================================

// Delete deletes a key from cache.
func (c *CacheService) Delete(key string) error {
	return c.client.Del(context.Background(), key).Err()
}

// Exists checks if a key exists.
func (c *CacheService) Exists(key string) (bool, error) {
	count, err := c.client.Exists(context.Background(), key).Result()
	return count > 0, err
}

// Expire sets expiration on a key.
func (c *CacheService) Expire(key string, ttl time.Duration) error {
	return c.client.Expire(context.Background(), key, ttl).Err()
}

// Flush flushes all keys in the current database.
func (c *CacheService) Flush() error {
	return c.client.FlushDB(context.Background()).Err()
}

// Ping pings the Redis server.
func (c *CacheService) Ping() error {
	return c.client.Ping(context.Background()).Err()
}

// Close closes the Redis connection.
func (c *CacheService) Close() error {
	return c.client.Close()
}