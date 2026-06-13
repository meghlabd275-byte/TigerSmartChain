// Package notifications provides notification service for email, SMS, push
package notifications

import (
	"context"
	"encoding/json"
	"fmt"
	"net/smtp"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

type Config struct {
	DBURL          string
	RedisURL       string
	SMTPHost      string
	SMTPPort      string
	SMTPUser      string
	SMTPPass      string
	FromEmail     string
}

type Notification struct {
	ID          string    `json:"id"`
	UserID     int       `json:"userId"`
	Type       string    `json:"type"` // email, sms, push, telegram
	Recipient  string    `json:"recipient"`
	Subject    string    `json:"subject"`
	Body       string    `json:"body"`
	Status     string    `json:"status"` // pending, sent, failed
	CreatedAt  time.Time `json:"createdAt"`
	SentAt    *time.Time `json:"sentAt,omitempty"`
}

type Subscription struct {
	ID        string    `json:"id"`
	UserID   int       `json:"userId"`
	Type     string    `json:"type"` // email, sms, push, telegram
	Endpoint string    `json:"endpoint"`
	Events   []string `json:"events"` // new_block, new_tx, price_alert, etc
	Active   bool      `json:"active"`
}

type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
}

func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, err
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 19})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, err
	}
	createTables(ctx, pool)
	srv := &Server{cfg: cfg, pool: pool, redis: rdb}
	go srv.startProcessor()
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) {
	pool.Exec(ctx, `CREATE TABLE IF NOT EXISTS notifications (id VARCHAR(36) PRIMARY KEY, user_id INTEGER NOT NULL, type VARCHAR(20) NOT NULL, recipient TEXT NOT NULL, subject TEXT, body TEXT NOT NULL, status VARCHAR(20) DEFAULT 'pending', created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, sent_at TIMESTAMP)`)
	pool.Exec(ctx, `CREATE TABLE IF NOT EXISTS subscriptions (id VARCHAR(36) PRIMARY KEY, user_id INTEGER NOT NULL, type VARCHAR(20) NOT NULL, endpoint TEXT NOT NULL, events TEXT[], active BOOLEAN DEFAULT TRUE)`)
	pool.Exec(ctx, `CREATE INDEX idx_notifications_user ON notifications(user_id)`)
	pool.Exec(ctx, `CREATE INDEX idx_subscriptions_user ON subscriptions(user_id)`)
}

func (s *Server) startProcessor() {
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()
	for range ticker.C {
		s.processQueue()
	}
}

func (s *Server) processQueue() {
	ctx := context.Background()
	
	rows, err := s.pool.Query(ctx, "SELECT id, user_id, type, recipient, subject, body FROM notifications WHERE status = 'pending' ORDER BY created_at ASC LIMIT 100")
	if err != nil {
		return
	}
	defer rows.Close()
	
	type notif struct {
		id, userID, ntype, recipient, subject, body string
	}
	
	var pending []notif
	for rows.Next() {
		var n notif
		if err := rows.Scan(&n.id, &n.userID, &n.ntype, &n.recipient, &n.subject, &n.body); err != nil {
			continue
		}
		pending = append(pending, n)
	}
	
	for _, n := range pending {
		var err error
		switch n.ntype {
		case "email":
			err = s.sendEmail(n.recipient, n.subject, n.body)
		case "telegram":
			err = s.sendTelegram(n.recipient, n.body)
		}
		
		if err != nil {
			s.pool.Exec(ctx, "UPDATE notifications SET status = 'failed' WHERE id = $1", n.id)
		} else {
			now := time.Now()
			s.pool.Exec(ctx, "UPDATE notifications SET status = 'sent', sent_at = $1 WHERE id = $2", now, n.id)
		}
	}
}

func (s *Server) sendEmail(to, subject, body string) error {
	msg := fmt.Sprintf("From: %s\r\nTo: %s\r\nSubject: %s\r\n\r\n%s", s.cfg.FromEmail, to, subject, body)
	
	err := smtp.SendMail(s.cfg.SMTPHost+":"+s.cfg.SMTPPort,
		smtp.PlainAuth("", s.cfg.SMTPUser, s.cfg.SMTPPass, s.cfg.SMTPHost),
		s.cfg.FromEmail, []string{to}, []byte(msg))
	
	return err
}

func (s *Server) sendTelegram(chatID, message string) error {
	return nil // Simplified - implement with actual Telegram API
}

func (s *Server) CreateNotification(userID int, ntype, recipient, subject, body string) (*Notification, error) {
	ctx := context.Background()
	
	id := fmt.Sprintf("notif_%d_%d", userID, time.Now().UnixNano())
	
	_, err := s.pool.Exec(ctx, "INSERT INTO notifications (id, user_id, type, recipient, subject, body) VALUES ($1, $2, $3, $4, $5, $6)",
		id, userID, ntype, recipient, subject, body)
	if err != nil {
		return nil, err
	}
	
	return &Notification{
		ID: id, UserID: userID, Type: ntype, Recipient: recipient,
		Subject: subject, Body: body, Status: "pending", CreatedAt: time.Now(),
	}, nil
}

func (s *Server) GetUserNotifications(ctx context.Context, userID int, limit int) ([]Notification, error) {
	rows, err := s.pool.Query(ctx, "SELECT id, user_id, type, recipient, subject, body, status, created_at, sent_at FROM notifications WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2", userID, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var notifs []Notification
	for rows.Next() {
		var n Notification
		if err := rows.Scan(&n.ID, &n.UserID, &n.Type, &n.Recipient, &n.Subject, &n.Body, &n.Status, &n.CreatedAt, &n.SentAt); err != nil {
			continue
		}
		notifs = append(notifs, n)
	}
	return notifs, nil
}

func (s *Server) Subscribe(userID int, ntype, endpoint string, events []string) (*Subscription, error) {
	ctx := context.Background()
	
	id := fmt.Sprintf("sub_%d_%d", userID, time.Now().UnixNano())
	eventsJSON, _ := json.Marshal(events)
	
	_, err := s.pool.Exec(ctx, "INSERT INTO subscriptions (id, user_id, type, endpoint, events) VALUES ($1, $2, $3, $4, $5)",
		id, userID, ntype, endpoint, string(eventsJSON))
	if err != nil {
		return nil, err
	}
	
	return &Subscription{ID: id, UserID: userID, Type: ntype, Endpoint: endpoint, Events: events, Active: true}, nil
}

func (s *Server) GetUserSubscriptions(ctx context.Context, userID int) ([]Subscription, error) {
	rows, err := s.pool.Query(ctx, "SELECT id, user_id, type, endpoint, events, active FROM subscriptions WHERE user_id = $1", userID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var subs []Subscription
	for rows.Next() {
		var s Subscription
		var eventsJSON string
		if err := rows.Scan(&s.ID, &s.UserID, &s.Type, &s.Endpoint, &eventsJSON, &s.Active); err != nil {
			continue
		}
		json.Unmarshal([]byte(eventsJSON), &s.Events)
		subs = append(subs, s)
	}
	return subs, nil
}

func (s *Server) NotifySubscribers(event string, message string) error {
	ctx := context.Background()
	
	rows, err := s.pool.Query(ctx, "SELECT user_id, type, endpoint FROM subscriptions WHERE active = true AND $1 = ANY(events)", event)
	if err != nil {
		return err
	}
	defer rows.Close()
	
	for rows.Next() {
		var userID int
		var ntype, endpoint string
		if err := rows.Scan(&userID, &ntype, &endpoint); err != nil {
			continue
		}
		s.CreateNotification(userID, ntype, endpoint, "TigerScan Alert", message)
	}
	
	return nil
}
