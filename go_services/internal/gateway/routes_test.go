package gateway

import (
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
)

// TestRegisterAllRoutesHasNoConflicts registers every route and fails if gin
// rejects one. gin panics when a wildcard segment (:name) and a literal segment
// are registered at the same position in the same tree, which is what happens
// when a static path like /txs/pending is declared after /txs/:hash. A panic
// here means at least one endpoint is unreachable, so the whole set is checked
// at once rather than relying on a request to notice.
func TestRegisterAllRoutesHasNoConflicts(t *testing.T) {
	gin.SetMode(gin.TestMode)

	h := NewHandler(Config{})

	defer func() {
		if r := recover(); r != nil {
			t.Fatalf("RegisterAllRoutes panicked: %v", r)
		}
	}()

	router := gin.New()
	h.RegisterAllRoutes(router)

	routes := router.Routes()
	if len(routes) == 0 {
		t.Fatal("RegisterAllRoutes registered no routes")
	}

	seen := make(map[string]bool, len(routes))
	for _, route := range routes {
		key := route.Method + " " + route.Path
		if seen[key] {
			t.Errorf("duplicate route registered: %s", key)
		}
		seen[key] = true
	}

	// Every REST route lives under /api/v1. The WebSocket endpoints are mounted
	// at /ws instead, since a socket client is not an API consumer.
	for _, route := range routes {
		if !strings.HasPrefix(route.Path, "/api/v1") && !strings.HasPrefix(route.Path, "/ws") {
			t.Errorf("route outside /api/v1 and /ws: %s %s", route.Method, route.Path)
		}
	}

	t.Logf("registered %d routes", len(routes))
}
