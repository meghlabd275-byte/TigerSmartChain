// Package graphql serves the TigerScan read API over GraphQL.
package graphql

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/graphql-go/graphql"
	"github.com/graphql-go/graphql/language/ast"
	"github.com/graphql-go/graphql/language/parser"
	"github.com/graphql-go/graphql/language/source"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds GraphQL server configuration.
type Config struct {
	Port           string
	DBURL          string
	RedisURL       string
	MaxComplexity  int
	MaxDepth       int
	RateLimit      int
	CacheEnabled   bool
	CacheTTL       time.Duration
	AllowedOrigins []string
	Debug          bool
}

// Server represents the GraphQL server.
type Server struct {
	cfg      *Config
	pool     *pgxpool.Pool
	redis    *redis.Client
	schema   graphql.Schema
	hub      *subscriptionHub
	httpSrv  *http.Server
	stopOnce sync.Once
}

// NewServer connects to the backing stores and builds the executable schema.
func NewServer(cfg *Config) (*Server, error) {
	if cfg.MaxComplexity <= 0 {
		cfg.MaxComplexity = 500
	}
	if cfg.MaxDepth <= 0 {
		cfg.MaxDepth = 12
	}
	if cfg.RateLimit <= 0 {
		cfg.RateLimit = 300
	}
	if cfg.CacheTTL <= 0 {
		cfg.CacheTTL = 5 * time.Second
	}

	ctx := context.Background()

	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("connect to database: %w", err)
	}
	if err := pool.Ping(ctx); err != nil {
		pool.Close()
		return nil, fmt.Errorf("ping database: %w", err)
	}

	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, DB: 1})
	if err := rdb.Ping(ctx).Err(); err != nil {
		pool.Close()
		return nil, fmt.Errorf("connect to redis: %w", err)
	}

	srv := &Server{
		cfg:   cfg,
		pool:  pool,
		redis: rdb,
		hub:   newSubscriptionHub(),
	}

	schema, err := srv.buildSchema()
	if err != nil {
		pool.Close()
		return nil, err
	}
	srv.schema = schema

	return srv, nil
}

// Start begins serving GraphQL over HTTP and WebSocket.
func (s *Server) Start() error {
	mux := http.NewServeMux()

	handler := &wsHandler{srv: s}
	mux.Handle("/graphql", handler)
	mux.Handle("/ws", handler)
	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		if err := s.pool.Ping(r.Context()); err != nil {
			http.Error(w, "database unavailable", http.StatusServiceUnavailable)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
	})

	if s.cfg.Debug {
		mux.HandleFunc("/graphiql", s.serveGraphiQL)
	}

	s.httpSrv = &http.Server{
		Addr:              ":" + s.cfg.Port,
		Handler:           s.securityHeaders(s.cors(mux)),
		ReadHeaderTimeout: 10 * time.Second,
		ReadTimeout:       30 * time.Second,
		WriteTimeout:      60 * time.Second,
		IdleTimeout:       120 * time.Second,
	}

	go s.hub.run(context.Background(), s.pool)

	return s.httpSrv.ListenAndServe()
}

// Stop shuts the HTTP server and background workers down.
func (s *Server) Stop(ctx context.Context) error {
	s.stopOnce.Do(func() {
		s.hub.stop()
	})
	s.pool.Close()
	if err := s.redis.Close(); err != nil {
		return err
	}
	if s.httpSrv == nil {
		return nil
	}
	return s.httpSrv.Shutdown(ctx)
}

func (s *Server) securityHeaders(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("X-Frame-Options", "DENY")
		w.Header().Set("Referrer-Policy", "no-referrer")
		w.Header().Set("Strict-Transport-Security", "max-age=31536000; includeSubDomains")
		next.ServeHTTP(w, r)
	})
}

// cors allows only the origins listed in the configuration.
func (s *Server) cors(next http.Handler) http.Handler {
	allowed := make(map[string]struct{}, len(s.cfg.AllowedOrigins))
	for _, origin := range s.cfg.AllowedOrigins {
		allowed[strings.TrimRight(origin, "/")] = struct{}{}
	}

	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		origin := r.Header.Get("Origin")
		if origin != "" {
			if _, ok := allowed[strings.TrimRight(origin, "/")]; !ok {
				http.Error(w, "origin not allowed", http.StatusForbidden)
				return
			}
			w.Header().Set("Access-Control-Allow-Origin", origin)
			w.Header().Set("Vary", "Origin")
			w.Header().Set("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
			w.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization")
			w.Header().Set("Access-Control-Max-Age", "600")
		}
		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		next.ServeHTTP(w, r)
	})
}

func (s *Server) serveGraphiQL(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	fmt.Fprint(w, `<!DOCTYPE html>
<html>
<head>
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/graphiql@3/graphiql.min.css"/>
</head>
<body style="margin:0">
<div id="graphiql" style="height:100vh"></div>
<script crossorigin src="https://cdn.jsdelivr.net/npm/react@18/umd/react.production.min.js"></script>
<script crossorigin src="https://cdn.jsdelivr.net/npm/react-dom@18/umd/react-dom.production.min.js"></script>
<script crossorigin src="https://cdn.jsdelivr.net/npm/graphiql@3/graphiql.min.js"></script>
<script>
ReactDOM.createRoot(document.getElementById('graphiql')).render(
  React.createElement(GraphiQL, {fetcher: GraphiQL.createFetcher({url: '/graphql'})})
);
</script>
</body>
</html>`)
}

// -----------------------------------------------------------------------------
// Request handling
// -----------------------------------------------------------------------------

type graphQLRequest struct {
	Query         string                 `json:"query"`
	OperationName string                 `json:"operationName"`
	Variables     map[string]interface{} `json:"variables"`
}

func (s *Server) serveHTTP(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost && r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	allowed, err := s.allow(r)
	if err != nil {
		http.Error(w, "rate limit unavailable", http.StatusServiceUnavailable)
		return
	}
	if !allowed {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}

	var req graphQLRequest
	if r.Method == http.MethodGet {
		req.Query = r.URL.Query().Get("query")
		req.OperationName = r.URL.Query().Get("operationName")
		if raw := r.URL.Query().Get("variables"); raw != "" {
			if err := json.Unmarshal([]byte(raw), &req.Variables); err != nil {
				writeGraphQL(w, nil, fmt.Errorf("invalid variables: %w", err))
				return
			}
		}
	} else {
		defer r.Body.Close()
		dec := json.NewDecoder(http.MaxBytesReader(w, r.Body, 1<<20))
		if err := dec.Decode(&req); err != nil {
			writeGraphQL(w, nil, fmt.Errorf("invalid request body: %w", err))
			return
		}
	}

	result, err := s.execute(r.Context(), req)
	if err != nil {
		writeGraphQL(w, nil, err)
		return
	}
	writeGraphQL(w, result, nil)
}

func writeGraphQL(w http.ResponseWriter, result *graphql.Result, err error) {
	w.Header().Set("Content-Type", "application/json")
	status := http.StatusOK
	payload := map[string]interface{}{}

	if err != nil {
		payload["errors"] = []map[string]interface{}{{"message": err.Error()}}
		status = http.StatusBadRequest
	} else {
		payload["data"] = result.Data
		if len(result.Errors) > 0 {
			errs := make([]map[string]interface{}, 0, len(result.Errors))
			for _, e := range result.Errors {
				errs = append(errs, map[string]interface{}{"message": e.Message})
			}
			payload["errors"] = errs
		}
	}

	w.WriteHeader(status)
	json.NewEncoder(w).Encode(payload)
}

// execute validates the document against the limits and runs it.
func (s *Server) execute(ctx context.Context, req graphQLRequest) (*graphql.Result, error) {
	if strings.TrimSpace(req.Query) == "" {
		return nil, fmt.Errorf("query must not be empty")
	}

	if err := s.validateQuery(req.Query); err != nil {
		return nil, err
	}

	d := &deps{
		pool:  s.pool,
		redis: s.redis,
		cache: newResultCache(s.redis, s.cfg.CacheEnabled, s.cfg.CacheTTL),
	}

	return graphql.Do(graphql.Params{
		Schema:         s.schema,
		RequestString:  req.Query,
		VariableValues: req.Variables,
		OperationName:  req.OperationName,
		Context:        withDeps(ctx, d),
	}), nil
}

// validateQuery enforces depth and complexity limits and, in production mode,
// rejects introspection so the schema is not enumerable.
func (s *Server) validateQuery(query string) error {
	doc, err := parser.Parse(parser.ParseParams{
		Source: source.NewSource(&source.Source{Body: []byte(query)}),
	})
	if err != nil {
		return fmt.Errorf("parse query: %w", err)
	}

	for _, def := range doc.Definitions {
		op, ok := def.(*ast.OperationDefinition)
		if !ok {
			continue
		}
		if !s.cfg.Debug && op.Operation == "query" && usesIntrospection(op.SelectionSet) {
			return fmt.Errorf("introspection is disabled")
		}
		if depth := selectionDepth(op.SelectionSet, 0); depth > s.cfg.MaxDepth {
			return fmt.Errorf("query depth %d exceeds limit %d", depth, s.cfg.MaxDepth)
		}
		if cost := selectionCost(op.SelectionSet, 1); cost > s.cfg.MaxComplexity {
			return fmt.Errorf("query cost %d exceeds limit %d", cost, s.cfg.MaxComplexity)
		}
	}

	return nil
}

func usesIntrospection(sel *ast.SelectionSet) bool {
	if sel == nil {
		return false
	}
	for _, item := range sel.Selections {
		field, ok := item.(*ast.Field)
		if !ok {
			continue
		}
		if name := field.Name.Value; name == "__schema" || name == "__type" {
			return true
		}
		if usesIntrospection(field.SelectionSet) {
			return true
		}
	}
	return false
}

func selectionDepth(sel *ast.SelectionSet, current int) int {
	if sel == nil {
		return current
	}
	max := current
	for _, item := range sel.Selections {
		switch node := item.(type) {
		case *ast.Field:
			if d := selectionDepth(node.SelectionSet, current+1); d > max {
				max = d
			}
		case *ast.InlineFragment:
			if d := selectionDepth(node.SelectionSet, current); d > max {
				max = d
			}
		}
	}
	return max
}

// selectionCost weights list fields more heavily than scalar fields, which
// approximates the work the resolvers perform.
func selectionCost(sel *ast.SelectionSet, weight int) int {
	if sel == nil {
		return 0
	}
	cost := 0
	for _, item := range sel.Selections {
		field, ok := item.(*ast.Field)
		if !ok {
			continue
		}
		fieldCost := weight
		for _, arg := range field.Arguments {
			if arg.Name.Value == "first" || arg.Name.Value == "skip" {
				fieldCost *= 5
			}
		}
		cost += fieldCost + selectionCost(field.SelectionSet, weight)
	}
	return cost
}

// allow applies a fixed-window rate limit keyed on the client address.
func (s *Server) allow(r *http.Request) (bool, error) {
	ip := clientIP(r)
	key := fmt.Sprintf("graphql:rl:%s:%d", ip, time.Now().Unix()/60)

	count, err := s.redis.Incr(r.Context(), key).Result()
	if err != nil {
		return false, err
	}
	if count == 1 {
		s.redis.Expire(r.Context(), key, 2*time.Minute)
	}
	return count <= int64(s.cfg.RateLimit), nil
}

func clientIP(r *http.Request) string {
	if fwd := r.Header.Get("X-Forwarded-For"); fwd != "" {
		parts := strings.Split(fwd, ",")
		return strings.TrimSpace(parts[0])
	}
	host := r.RemoteAddr
	if i := strings.LastIndex(host, ":"); i > 0 {
		host = host[:i]
	}
	return host
}

var _ = log.Printf
