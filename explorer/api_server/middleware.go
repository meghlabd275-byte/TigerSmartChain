// TigerSmartChain API Server - middleware and configuration helpers.

package main

import (
	"context"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
)

// envStr returns the environment variable or a default.
func envStr(name, def string) string {
	if v := strings.TrimSpace(os.Getenv(name)); v != "" {
		return v
	}
	return def
}

// envInt returns the environment variable parsed as an int, or a default.
func envInt(name string, def int) int {
	v := strings.TrimSpace(os.Getenv(name))
	if v == "" {
		return def
	}
	n, err := strconv.Atoi(v)
	if err != nil {
		return def
	}
	return n
}

// envList splits a comma-separated environment variable into a list.
func envList(name string, def []string) []string {
	raw := strings.TrimSpace(os.Getenv(name))
	if raw == "" {
		return def
	}
	parts := strings.Split(raw, ",")
	out := make([]string, 0, len(parts))
	for _, p := range parts {
		if p = strings.TrimSpace(p); p != "" {
			out = append(out, p)
		}
	}
	if len(out) == 0 {
		return def
	}
	return out
}

// securityHeaders sets the response headers that harden every browser-facing
// response: no MIME sniffing, no framing, no referrer leakage, and a strict
// content security policy for the JSON API.
func securityHeaders() gin.HandlerFunc {
	return func(c *gin.Context) {
		h := c.Writer.Header()
		h.Set("X-Content-Type-Options", "nosniff")
		h.Set("X-Frame-Options", "DENY")
		h.Set("Referrer-Policy", "no-referrer")
		h.Set("Cross-Origin-Resource-Policy", "same-site")
		h.Set("Content-Security-Policy", "default-src 'none'; frame-ancestors 'none'")
		h.Set("Permissions-Policy", "geolocation=(), microphone=(), camera=()")
		if c.Request.TLS != nil {
			h.Set("Strict-Transport-Security", "max-age=31536000; includeSubDomains")
		}
		c.Next()
	}
}

// apiKeyAuth requires a valid API key with the "read" or "*" scope on every
// request. Keys are presented in the X-API-Key header or as a bearer token.
func (h *Handler) apiKeyAuth() gin.HandlerFunc {
	return func(c *gin.Context) {
		key := strings.TrimSpace(c.GetHeader("X-API-Key"))
		if key == "" {
			if auth := c.GetHeader("Authorization"); strings.HasPrefix(auth, "Bearer ") {
				key = strings.TrimSpace(strings.TrimPrefix(auth, "Bearer "))
			}
		}
		if key == "" {
			c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "API key required"})
			return
		}
		if !h.apiKeys.Validate(key, "read") {
			c.AbortWithStatusJSON(http.StatusForbidden, gin.H{"error": "invalid or insufficient API key"})
			return
		}
		h.apiKeys.Increment(key)
		c.Next()
	}
}

// Health reports process liveness. It never touches the database so it stays
// cheap enough for a load balancer to poll.
func (h *Handler) Health(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"time":   time.Now().UTC().Format(time.RFC3339),
	})
}

// Ready reports whether the server can serve traffic, which requires a live
// database connection and, when configured, a reachable node.
func (h *Handler) Ready(c *gin.Context) {
	ctx, cancel := context.WithTimeout(c.Request.Context(), 3*time.Second)
	defer cancel()

	if err := h.db.PingContext(ctx); err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"status": "unavailable", "database": "down"})
		return
	}

	body := gin.H{"status": "ready", "database": "up"}
	if h.nodeRPCURL != "" {
		if _, err := h.rpcCall(ctx, "eth_chainId"); err != nil {
			body["node"] = "down"
			c.JSON(http.StatusServiceUnavailable, body)
			return
		}
		body["node"] = "up"
	}
	c.JSON(http.StatusOK, body)
}
