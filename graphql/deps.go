package graphql

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/graphql-go/graphql"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// deps carries the request scoped dependencies that resolvers need.
type deps struct {
	pool  *pgxpool.Pool
	redis *redis.Client
	cache *resultCache
}

type depsKey struct{}

func withDeps(ctx context.Context, d *deps) context.Context {
	return context.WithValue(ctx, depsKey{}, d)
}

func depsFrom(ctx context.Context) (*deps, error) {
	d, ok := ctx.Value(depsKey{}).(*deps)
	if !ok || d == nil || d.pool == nil {
		return nil, fmt.Errorf("graphql dependencies are not available")
	}
	return d, nil
}

// resultCache memoises query results in Redis. Reads are best effort: a cache
// failure must never fail the request.
type resultCache struct {
	rdb     *redis.Client
	enabled bool
	ttl     time.Duration
}

func newResultCache(rdb *redis.Client, enabled bool, ttl time.Duration) *resultCache {
	return &resultCache{rdb: rdb, enabled: enabled, ttl: ttl}
}

func (c *resultCache) key(query, operationName string, variables map[string]interface{}) string {
	canonical := query + "\x00" + operationName

	if len(variables) > 0 {
		keys := make([]string, 0, len(variables))
		for k := range variables {
			keys = append(keys, k)
		}
		sort.Strings(keys)

		var b strings.Builder
		for _, k := range keys {
			fmt.Fprintf(&b, "%s=%v;", k, variables[k])
		}
		canonical += "\x00" + b.String()
	}

	sum := sha256.Sum256([]byte(canonical))
	return "graphql:cache:" + hex.EncodeToString(sum[:])
}

func (c *resultCache) get(ctx context.Context, key string) (*graphql.Result, bool) {
	if c == nil || !c.enabled {
		return nil, false
	}

	raw, err := c.rdb.Get(ctx, key).Bytes()
	if err != nil {
		return nil, false
	}

	var payload struct {
		Data interface{} `json:"data"`
	}
	if err := json.Unmarshal(raw, &payload); err != nil {
		return nil, false
	}

	return &graphql.Result{Data: payload.Data}, true
}

func (c *resultCache) set(ctx context.Context, key string, result *graphql.Result) {
	if c == nil || !c.enabled || result == nil {
		return
	}

	raw, err := json.Marshal(map[string]interface{}{"data": result.Data})
	if err != nil {
		return
	}
	c.rdb.Set(ctx, key, raw, c.ttl)
}
