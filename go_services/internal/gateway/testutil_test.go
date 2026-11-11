package gateway

import (
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"
)

// newTestContext builds a gin context backed by a real request, which is what
// the header-reading helpers operate on.
func newTestContext(t *testing.T) *gin.Context {
	t.Helper()
	gin.SetMode(gin.TestMode)

	c, _ := gin.CreateTestContext(httptest.NewRecorder())
	c.Request = httptest.NewRequest("GET", "/api/v1/blocks", nil)
	return c
}
