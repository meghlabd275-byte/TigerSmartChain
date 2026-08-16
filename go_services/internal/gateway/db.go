package gateway

// Database-backed query layer for the API gateway.
//
// This replaces all getMockData()/generateMockData() placeholder responses
// with real queries against the PostgreSQL explorer database (schema in
// explorer/databases/postgres_schema/schema.sql) and JSON-RPC calls to the
// configured upstream node for live chain state.

import (
	"bytes"
	"context"
	cryptorand "crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5"
)

// rpcRequest is a JSON-RPC 2.0 request envelope.
type rpcRequest struct {
	JSONRPC string        `json:"jsonrpc"`
	ID      int           `json:"id"`
	Method  string        `json:"method"`
	Params  []interface{} `json:"params"`
}

// rpcCall issues a JSON-RPC call to the configured upstream node and returns
// the decoded `result` field.
func (h *Handler) rpcCall(ctx context.Context, method string, params []interface{}) (json.RawMessage, error) {
	if h.rpcURL == "" {
		return nil, fmt.Errorf("no RPC URL configured")
	}
	body, err := json.Marshal(rpcRequest{
		JSONRPC: "2.0",
		ID:      1,
		Method:  method,
		Params:  params,
	})
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, h.rpcURL, bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")

	client := &http.Client{Timeout: 15 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("RPC HTTP %d: %s", resp.StatusCode, string(raw))
	}

	var envelope struct {
		Result json.RawMessage `json:"result"`
		Error  json.RawMessage `json:"error"`
	}
	if err := json.Unmarshal(raw, &envelope); err != nil {
		return nil, fmt.Errorf("decode RPC response: %w", err)
	}
	if len(envelope.Error) > 0 && string(envelope.Error) != "null" {
		return nil, fmt.Errorf("RPC error: %s", string(envelope.Error))
	}
	if len(envelope.Result) == 0 {
		return nil, fmt.Errorf("RPC missing result")
	}
	return envelope.Result, nil
}

// queryRows runs a parameterized SQL query against the explorer database and
// returns each row as an ordered map keyed by column name.
func (h *Handler) queryRows(ctx context.Context, sql string, args ...interface{}) ([]map[string]interface{}, error) {
	if h.pool == nil {
		return nil, fmt.Errorf("database not available")
	}
	rows, err := h.pool.Query(ctx, sql, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	fields := rows.FieldDescriptions()
	var out []map[string]interface{}
	for rows.Next() {
		values, err := rows.Values()
		if err != nil {
			return nil, err
		}
		row := make(map[string]interface{}, len(fields))
		for i, fd := range fields {
			v := values[i]
			// Normalize numeric types into JSON-friendly forms.
			switch tv := v.(type) {
			case [16]byte:
				// pgx returns VARCHAR(42) hashes as [16]byte sometimes; convert to hex.
				row[string(fd.Name)] = fmt.Sprintf("%x", tv)
			default:
				row[string(fd.Name)] = tv
			}
		}
		out = append(out, row)
	}
	return out, rows.Err()
}

// queryOne runs a query expected to return a single row.
func (h *Handler) queryOne(ctx context.Context, sql string, args ...interface{}) (map[string]interface{}, error) {
	rows, err := h.queryRows(ctx, sql, args...)
	if err != nil {
		return nil, err
	}
	if len(rows) == 0 {
		return nil, nil
	}
	return rows[0], nil
}

// clampLimit bounds a page size to a safe maximum.
func clampLimit(n int) int {
	if n <= 0 || n > 500 {
		return 50
	}
	return n
}

// paramInt extracts and clamps a "limit"/"count" query parameter.
func paramInt(c *gin.Context, key string, def int) int {
	v := c.Query(key)
	if v == "" {
		return clampLimit(def)
	}
	n, err := strconv.Atoi(v)
	if err != nil {
		return clampLimit(def)
	}
	return clampLimit(n)
}

// paramOffset returns the SQL OFFSET for pagination.
func paramOffset(c *gin.Context) int {
	page, _ := strconv.Atoi(c.Query("page"))
	if page <= 0 {
		page = 1
	}
	limit := paramInt(c, "limit", 50)
	return (page - 1) * limit
}

// respondList writes a paginated list envelope with the rows and total.
func respondList(c *gin.Context, rows []map[string]interface{}, total int) {
	if rows == nil {
		rows = []map[string]interface{}{}
	}
	c.JSON(http.StatusOK, gin.H{
		"items": rows,
		"total": total,
		"page":  c.Query("page"),
		"limit": c.Query("limit"),
	})
}

// respondOne writes a single object, or 404 when nil.
func respondOne(c *gin.Context, row map[string]interface{}) {
	if row == nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "not found"})
		return
	}
	c.JSON(http.StatusOK, row)
}

// dbError writes a 502 when the database is unavailable.
func dbError(c *gin.Context, err error) {
	c.JSON(http.StatusServiceUnavailable, gin.H{
		"error": fmt.Sprintf("data source unavailable: %v", err),
	})
}

// rowValue safely extracts a value from a single-row query result. Returns nil
// when the row is nil so callers can pass it straight to gin.H without panics.
func rowValue(row map[string]interface{}, key string) interface{} {
	if row == nil {
		return nil
	}
	return row[key]
}

// listByBlockNumber returns rows for a given block number using the supplied SQL.
func (h *Handler) listByBlockNumber(c *gin.Context, sql string) {
	numStr := c.Param("number")
	num, err := strconv.ParseInt(numStr, 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"})
		return
	}
	limit := paramInt(c, "limit", 50)
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, sql, num, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// listByTxHash returns rows for a given transaction hash.
func (h *Handler) listByTxHash(c *gin.Context, sql string) {
	hash := c.Param("hash")
	if hash == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing hash"})
		return
	}
	limit := paramInt(c, "limit", 50)
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, sql, hash, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// listByAddress returns rows for a given address.
func (h *Handler) listByAddress(c *gin.Context, addrParam string, sql string) {
	addr := c.Param(addrParam)
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	limit := paramInt(c, "limit", 50)
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, sql, addr, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// countQuery returns a single integer count from the database.
func (h *Handler) countQuery(ctx context.Context, sql string, args ...interface{}) (int64, error) {
	row, err := h.queryOne(ctx, sql, args...)
	if err != nil || row == nil {
		return 0, err
	}
	for _, v := range row {
		switch tv := v.(type) {
		case int64:
			return tv, nil
		case int:
			return int64(tv), nil
		case float64:
			return int64(tv), nil
		}
	}
	return 0, nil
}

// scanRow scans a single row using a pgx.Row into a map.
func (h *Handler) scanRow(ctx context.Context, sql string, args ...interface{}) (pgx.Row, error) {
	if h.pool == nil {
		return nil, fmt.Errorf("database not available")
	}
	return h.pool.QueryRow(ctx, sql, args...), nil
}

// randomHex returns a cryptographically random hex string of the given byte
// length (output length = 2*n).
func randomHex(n int) string {
	b := make([]byte, n)
	_, _ = cryptorand.Read(b)
	return hex.EncodeToString(b)
}
