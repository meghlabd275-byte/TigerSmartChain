// Package monitoring provides system monitoring service
package monitoring

import (
	"context"
	"fmt"
	"net/http"
	"runtime"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

type Config struct {
	DBURL          string
	RedisURL       string
	CheckInterval  time.Duration
	AlertThreshold float64
}

type SystemMetrics struct {
	CPUUsage      float64 `json:"cpuUsage"`
	MemoryUsage   float64 `json:"memoryUsage"`
	MemoryTotal   uint64 `json:"memoryTotal"`
	MemoryUsed    uint64 `json:"memoryUsed"`
	Goroutines    int    `json:"goroutines"`
	GCCount       uint32 `json:"gcCount"`
	HeapAlloc    uint64 `json:"heapAlloc"`
	HeapSys      uint64 `json:"heapSys"`
}

type ComponentStatus struct {
	Name        string  `json:"name"`
	Status     string  `json:"status"` // healthy, degraded, down
	Latency    float64 `json:"latency"`
	LastCheck  time.Time `json:"lastCheck"`
	Error      string  `json:"error,omitempty"`
}

type Alert struct {
	ID          string    `json:"id"`
	Level       string    `json:"level"` // info, warning, critical
	Component   string    `json:"component"`
	Message    string    `json:"message"`
	Timestamp  time.Time `json:"timestamp"`
	Resolved   bool      `json:"resolved"`
}

type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
	components map[string]*ComponentStatus
	alerts    []Alert
}

func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, err
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 22})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, err
	}
	srv := &Server{
		cfg: cfg,
		pool: pool,
		redis: rdb,
		components: make(map[string]*ComponentStatus),
		alerts: make([]Alert, 0),
	}
	go srv.startMonitor()
	return srv, nil
}

func (s *Server) startMonitor() {
	ticker := time.NewTicker(s.cfg.CheckInterval)
	defer ticker.Stop()
	for range ticker.C {
		s.checkComponents()
		s.collectMetrics()
	}
}

func (s *Server) checkComponents() {
	ctx := context.Background()
	
	components := []string{"database", "redis", "node", "api", "indexer"}
	
	for _, comp := range components {
		status := &ComponentStatus{
			Name:       comp,
			LastCheck:  time.Now(),
		}
		
		switch comp {
		case "database":
			start := time.Now()
			err := s.pool.QueryRow(ctx, "SELECT 1").Scan()
			status.Latency = time.Since(start).Milliseconds()
			if err != nil {
				status.Status = "down"
				status.Error = err.Error()
				s.createAlert("critical", comp, "Database is down: "+err.Error())
			} else if status.Latency > 1000 {
				status.Status = "degraded"
				s.createAlert("warning", comp, "Database latency high")
			} else {
				status.Status = "healthy"
			}
			
		case "redis":
			start := time.Now()
			err := s.redis.Ping(ctx).Err()
			status.Latency = time.Since(start).Milliseconds()
			if err != nil {
				status.Status = "down"
				status.Error = err.Error()
				s.createAlert("critical", comp, "Redis is down: "+err.Error())
			} else if status.Latency > 100 {
				status.Status = "degraded"
				s.createAlert("warning", comp, "Redis latency high")
			} else {
				status.Status = "healthy"
			}
		}
		
		s.mu.Lock()
		s.components[comp] = status
		s.mu.Unlock()
	}
}

func (s *Server) collectMetrics() {
	var m runtime.MemStats
	runtime.ReadMemStats(&m)
	
	metrics := SystemMetrics{
		CPUUsage:   getCPUUsage(),
		MemoryUsed: m.Alloc,
		MemoryTotal: m.Sys,
		Goroutines:  runtime.NumGoroutine(),
		GCCount:     m.NumGC,
		HeapAlloc:  m.HeapAlloc,
		HeapSys:    m.HeapSys,
	}
	
	data, _ := json.Marshal(metrics)
	s.redis.Set(context.Background(), "system:metrics", string(data), time.Minute)
}

func getCPUUsage() float64 {
	var m runtime.MemStats
	runtime.ReadMemStats(&m)
	return float64(m.Alloc) / float64(m.Sys) * 100
}

func (s *Server) createAlert(level, component, message string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	alert := Alert{
		ID:        fmt.Sprintf("alert_%d_%s", time.Now().Unix(), component),
		Level:     level,
		Component: component,
		Message:   message,
		Timestamp: time.Now(),
		Resolved: false,
	}
	
	s.alerts = append(s.alerts, alert)
	
	// Keep only last 100 alerts
	if len(s.alerts) > 100 {
		s.alerts = s.alerts[len(s.alerts)-100:]
	}
	
	// Publish to Redis for real-time alerts
	alertData, _ := json.Marshal(alert)
	s.redis.Publish(context.Background(), "alerts", string(alertData))
}

func (s *Server) GetSystemMetrics(ctx context.Context) (*SystemMetrics, error) {
	data, err := s.redis.Get(ctx, "system:metrics").Result()
	if err != nil {
		return nil, err
	}
	
	var metrics SystemMetrics
	json.Unmarshal([]byte(data), &metrics)
	return &metrics, nil
}

func (s *Server) GetComponentStatus(ctx context.Context) map[string]*ComponentStatus {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	status := make(map[string]*ComponentStatus)
	for k, v := range s.components {
		status[k] = v
	}
	return status
}

func (s *Server) GetAlerts(ctx context.Context, limit int) []Alert {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	if limit > len(s.alerts) {
		limit = len(s.alerts)
	}
	
	alerts := make([]Alert, limit)
	copy(alerts, s.alerts[len(s.alerts)-limit:])
	return alerts
}

func (s *Server) ResolveAlert(alertID string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	for i := range s.alerts {
		if s.alerts[i].ID == alertID {
			s.alerts[i].Resolved = true
			break
		}
	}
	return nil
}

func (s *Server) HealthCheck(w http.ResponseWriter, r *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	healthy := true
	for _, comp := range s.components {
		if comp.Status == "down" {
			healthy = false
			break
		}
	}
	
	if healthy {
		w.WriteHeader(http.StatusOK)
		fmt.Fprint(w, `{"status":"healthy"}`)
	} else {
		w.WriteHeader(http.StatusServiceUnavailable)
		fmt.Fprint(w, `{"status":"unhealthy"}`)
	}
}

func (s *Server) ReadyCheck(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	
	// Check database
	if err := s.pool.QueryRow(ctx, "SELECT 1").Scan(); err != nil {
		w.WriteHeader(http.StatusServiceUnavailable)
		fmt.Fprintf(w, `{"status":"not_ready","error":"database"}`)
		return
	}
	
	// Check Redis
	if err := s.redis.Ping(ctx).Err(); err != nil {
		w.WriteHeader(http.StatusServiceUnavailable)
		fmt.Fprintf(w, `{"status":"not_ready","error":"redis"}`)
		return
	}
	
	w.WriteHeader(http.StatusOK)
	fmt.Fprint(w, `{"status":"ready"}`)
}
