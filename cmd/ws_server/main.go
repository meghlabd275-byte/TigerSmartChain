// Command ws-server runs the TigerScan WebSocket fan-out service.
//
// It is the process `Dockerfile.ws` builds. The `ws` package is a library with
// no entrypoint of its own, so without this file the compose service had no
// binary to run and `docker compose up` failed on an empty command.
package main

import (
	"log"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/tigersmartchain/tigersmartchain/ws"
)

// envOr returns the environment value for key, or fallback when it is unset or
// blank.
func envOr(key, fallback string) string {
	if v := strings.TrimSpace(os.Getenv(key)); v != "" {
		return v
	}
	return fallback
}

// envInt parses an integer setting, falling back on anything unparseable so a
// typo cannot silently disable a limit (a zero buffer or message size).
func envInt(key string, fallback int) int {
	raw := strings.TrimSpace(os.Getenv(key))
	if raw == "" {
		return fallback
	}
	n, err := strconv.Atoi(raw)
	if err != nil || n <= 0 {
		log.Printf("ignoring invalid %s=%q, using %d", key, raw, fallback)
		return fallback
	}
	return n
}

// splitCSV splits a comma-separated list, trimming blanks.
func splitCSV(raw string) []string {
	if strings.TrimSpace(raw) == "" {
		return nil
	}
	parts := strings.Split(raw, ",")
	out := make([]string, 0, len(parts))
	for _, p := range parts {
		if p = strings.TrimSpace(p); p != "" {
			out = append(out, p)
		}
	}
	return out
}

func main() {
	cfg := &ws.Config{
		Port:              envOr("WS_PORT", "8081"),
		ReadBufferSize:    envInt("WS_READ_BUFFER", 1024),
		WriteBufferSize:   envInt("WS_WRITE_BUFFER", 1024),
		PoolMaxReco:       envInt("WS_POOL_MAX_RECO", 10),
		DBURL:             envOr("DATABASE_URL", "postgres://postgres:postgres@localhost:5432/tigerscan"),
		RedisURL:          envOr("REDIS_URL", "localhost:6379"),
		HeartbeatInterval: 30 * time.Second,
		MaxMessageSize:    int64(envInt("WS_MAX_MESSAGE_SIZE", 1<<20)),
		// An empty list keeps the same-origin-only default: the origin check
		// always permits same-origin and non-browser clients, and only these
		// extra origins are allowed on top of that.
		AllowedOrigins: splitCSV(os.Getenv("WS_ALLOWED_ORIGINS")),
	}

	server, err := ws.NewServer(cfg)
	if err != nil {
		log.Fatalf("websocket server: %v", err)
	}
	defer server.Close()

	// Shut down cleanly on SIGINT/SIGTERM so in-flight clients are closed
	// rather than dropped when the container is stopped.
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		<-sigCh
		log.Println("shutting down websocket server")
		_ = server.Close()
		os.Exit(0)
	}()

	log.Printf("websocket server listening on :%s", cfg.Port)
	if err := server.Start(); err != nil {
		log.Fatalf("websocket server stopped: %v", err)
	}
}
