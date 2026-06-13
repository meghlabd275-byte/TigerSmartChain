// Package webhook provides webhook service for real-time alerts
package webhook

import (
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net/http"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

type Config struct {
	DBURL          string
	RedisURL       string
	SecretKey     string
	RetryCount    int
	Timeout       time.Duration
}

type Webhook struct {
	ID          string    `json:"id"`
	UserID     int       `json:"userId"`
	URL        string    `json:"url"`
	Events     []string `json:"events"` // new_block, new_tx, etc
	Secret    string    `json:"secret"`
	IsActive   bool      `json:"isActive"`
	CreatedAt  time.Time `json:"createdAt"`
}

type WebhookEvent struct {
	ID          string    `json:"id"`
	WebhookID  string    `json:"webhookId"`
	Event     string    `json:"event"`
	Payload   string    `json:"payload"`
	Status    string    `json:"status"` // pending, sent, failed
	RetryCount int      `json:"retryCount"`
	CreatedAt  time.Time `json:"createdAt"`
	SentAt    *time.Time `json:"sentAt,omitempty"`
}

type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
	client *http.Client
}

func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, err
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 20})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, err
	}
	createTables(ctx, pool)
	srv := &Server{
		cfg: cfg,
		pool: pool,
		redis: rdb,
		client: &http.Client{Timeout: cfg.Timeout},
	}
	go srv.startProcessor()
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) {
	pool.Exec(ctx, `CREATE TABLE IF NOT EXISTS webhooks (id VARCHAR(36) PRIMARY KEY, user_id INTEGER NOT NULL, url TEXT NOT NULL, events TEXT[], secret VARCHAR(64), is_active BOOLEAN DEFAULT TRUE, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`)
	pool.Exec(ctx, `CREATE TABLE IF NOT EXISTS webhook_events (id VARCHAR(36) PRIMARY KEY, webhook_id VARCHAR(36) NOT NULL, event VARCHAR(50) NOT NULL, payload TEXT NOT NULL, status VARCHAR(20) DEFAULT 'pending', retry_count INTEGER DEFAULT 0, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, sent_at TIMESTAMP)`)
	pool.Exec(ctx, `CREATE INDEX idx_webhooks_user ON webhooks(user_id)`)
	pool.Exec(ctx, `CREATE INDEX idx_webhook_events_webhook ON webhook_events(webhook_id)`)
}

func (s *Server) startProcessor() {
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()
	for range ticker.C {
		s.processEvents()
	}
}

func (s *Server) processEvents() {
	ctx := context.Background()
	
	rows, err := s.pool.Query(ctx, "SELECT id, webhook_id, event, payload FROM webhook_events WHERE status = 'pending' AND retry_count < $1 ORDER BY created_at ASC LIMIT 100", s.cfg.RetryCount)
	if err != nil {
		return
	}
	defer rows.Close()
	
	type pendingEvent struct {
		id       string
		webhookID string
		event    string
		payload  string
	}
	
	var pending []pendingEvent
	for rows.Next() {
		var e pendingEvent
		if err := rows.Scan(&e.id, &e.webhookID, &e.event, &e.payload); err != nil {
			continue
		}
		pending = append(pending, e)
	}
	
	for _, e := range pending {
		s.sendWebhook(ctx, e.id, e.webhookID, e.event, e.payload)
	}
}

func (s *Server) sendWebhook(ctx context.Context, eventID, webhookID, event, payload string) {
	// Get webhook URL and secret
	var webhook Webhook
	err := s.pool.QueryRow(ctx, "SELECT id, url, secret FROM webhooks WHERE id = $1 AND is_active = true", webhookID).Scan(&webhook.ID, &webhook.URL, &webhook.Secret)
	if err != nil {
		s.pool.Exec(ctx, "UPDATE webhook_events SET status = 'failed' WHERE id = $1", eventID)
		return
	}
	
	// Sign payload
	signature := s.signPayload(payload, webhook.Secret)
	
	req, err := http.NewRequest("POST", webhook.URL, bytes.NewBufferString(payload))
	if err != nil {
		s.pool.Exec(ctx, "UPDATE webhook_events SET status = 'failed', retry_count = retry_count + 1 WHERE id = $1", eventID)
		return
	}
	
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Webhook-Signature", signature)
	req.Header.Set("X-Webhook-Event", event)
	
	resp, err := s.client.Do(req)
	if err != nil || resp.StatusCode >= 400 {
		s.pool.Exec(ctx, "UPDATE webhook_events SET status = 'failed', retry_count = retry_count + 1 WHERE id = $1", eventID)
		return
	}
	
	now := time.Now()
	s.pool.Exec(ctx, "UPDATE webhook_events SET status = 'sent', sent_at = $1 WHERE id = $2", now, eventID)
}

func (s *Server) signPayload(payload, secret string) string {
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(payload))
	return hex.EncodeToString(mac.Sum(nil))
}

func (s *Server) CreateWebhook(userID int, url string, events []string) (*Webhook, error) {
	ctx := context.Background()
	
	id := fmt.Sprintf("wh_%d_%d", userID, time.Now().UnixNano())
	secret := generateSecret(32)
	eventsJSON, _ := json.Marshal(events)
	
	_, err := s.pool.Exec(ctx, "INSERT INTO webhooks (id, user_id, url, events, secret) VALUES ($1, $2, $3, $4, $5)",
		id, userID, url, string(eventsJSON), secret)
	if err != nil {
		return nil, err
	}
	
	return &Webhook{
		ID: id, UserID: userID, URL: url, Events: events,
		Secret: secret, IsActive: true, CreatedAt: time.Now(),
	}, nil
}

func (s *Server) GetWebhooks(ctx context.Context, userID int) ([]Webhook, error) {
	rows, err := s.pool.Query(ctx, "SELECT id, user_id, url, events, secret, is_active, created_at FROM webhooks WHERE user_id = $1", userID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var webhooks []Webhook
	for rows.Next() {
		var w Webhook
		var eventsJSON string
		if err := rows.Scan(&w.ID, &w.UserID, &w.URL, &eventsJSON, &w.Secret, &w.IsActive, &w.CreatedAt); err != nil {
			continue
		}
		json.Unmarshal([]byte(eventsJSON), &w.Events)
		webhooks = append(webhooks, w)
	}
	return webhooks, nil
}

func (s *Server) DeleteWebhook(ctx context.Context, webhookID string) error {
	_, err := s.pool.Exec(ctx, "UPDATE webhooks SET is_active = false WHERE id = $1", webhookID)
	return err
}

func (s *Server) TriggerEvent(webhookID, event, payload string) error {
	ctx := context.Background()
	
	id := fmt.Sprintf("whe_%d_%d", time.Now().Unix(), len(event))
	
	_, err := s.pool.Exec(ctx, "INSERT INTO webhook_events (id, webhook_id, event, payload) VALUES ($1, $2, $3, $4)",
		id, webhookID, event, payload)
	return err
}

func (s *Server) TriggerEventAll(event, payload string) error {
	ctx := context.Background()
	
	rows, err := s.pool.Query(ctx, "SELECT id FROM webhooks WHERE is_active = true AND $1 = ANY(events)", event)
	if err != nil {
		return err
	}
	defer rows.Close()
	
	for rows.Next() {
		var webhookID string
		if err := rows.Scan(&webhookID); err != nil {
			continue
		}
		s.TriggerEvent(webhookID, event, payload)
	}
	
	return nil
}

func generateSecret(length int) string {
	chars := "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
	secret := make([]byte, length)
	for i := range secret {
		secret[i] = chars[i%len(chars)]
	}
	return string(secret)
}

func (s *Server) GetWebhookEvents(ctx context.Context, webhookID string, limit int) ([]WebhookEvent, error) {
	rows, err := s.pool.Query(ctx, "SELECT id, webhook_id, event, payload, status, retry_count, created_at, sent_at FROM webhook_events WHERE webhook_id = $1 ORDER BY created_at DESC LIMIT $2", webhookID, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var events []WebhookEvent
	for rows.Next() {
		var e WebhookEvent
		if err := rows.Scan(&e.ID, &e.WebhookID, &e.Event, &e.Payload, &e.Status, &e.RetryCount, &e.CreatedAt, &e.SentAt); err != nil {
			continue
		}
		events = append(events, e)
	}
	return events, nil
}

func VerifySignature(payload, signature, secret string) bool {
	expected := hmac.New(sha256.New, []byte(secret))
	expected.Write([]byte(payload))
	return hmac.Equal([]byte(hex.EncodeToString(expected.Sum(nil))), []byte(signature))
}
