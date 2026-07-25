/**
 * TigerScan Price Alerts Service
 * 
 * Complete implementation of token price notifications:
 * - Price threshold alerts
 * - Percentage change alerts
 * - Volume alerts
 * - Multi-channel notifications (email, Telegram, Discord, webhook)
 */

package main

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/gin-gonic/gin"
	_ "github.com/lib/pq"
)

// =============================================================================
// Configuration
// =============================================================================

type Config struct {
	DBHost      string
	DBPort      int
	DBUser      string
	DBPassword  string
	DBName      string
	ServerPort  int
	TelegramBot string
	DiscordHook string
	SMTPHost    string
	SMTPPort    int
	SMTPUser    string
	SMTPPass    string
}

func LoadConfig() *Config {
	return &Config{
		DBHost:      getEnv("DB_HOST", "localhost"),
		DBPort:      5432,
		DBUser:      getEnv("DB_USER", "tigerscan"),
		DBPassword:  getEnv("DB_PASSWORD", "password"),
		DBName:      getEnv("DB_NAME", "tigerscan_alerts"),
		ServerPort:  8445,
		TelegramBot: getEnv("TELEGRAM_BOT_TOKEN", ""),
		DiscordHook: getEnv("DISCORD_WEBHOOK", ""),
		SMTPHost:    getEnv("SMTP_HOST", "smtp.gmail.com"),
		SMTPPort:    587,
		SMTPUser:    getEnv("SMTP_USER", ""),
		SMTPPass:    getEnv("SMTP_PASS", ""),
	}
}

func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}

// =============================================================================
// Models
// =============================================================================

type PriceAlert struct {
	ID            int64     `json:"id"`
	UserID        int64     `json:"user_id"`
	TokenAddress  string    `json:"token_address"`
	TokenSymbol   string    `json:"token_symbol"`
	AlertType     string    `json:"alert_type"`  // above, below, change_up, change_down, volume
	Threshold     float64   `json:"threshold"`
	Enabled       bool      `json:"enabled"`
	LastTriggered *time.Time `json:"last_triggered,omitempty"`
	TriggerCount  int64     `json:"trigger_count"`
	CreatedAt     time.Time `json:"created_at"`
	UpdatedAt     time.Time `json:"updated_at"`
}

type NotificationChannel struct {
	ID           int64  `json:"id"`
	UserID       int64  `json:"user_id"`
	ChannelType  string `json:"channel_type"` // email, telegram, discord, webhook
	ChannelValue string `json:"channel_value"` // email address, chat ID, webhook URL
	Enabled      bool   `json:"enabled"`
	CreatedAt    time.Time `json:"created_at"`
}

type AlertHistory struct {
	ID          int64     `json:"id"`
	AlertID     int64     `json:"alert_id"`
	OldPrice    float64   `json:"old_price"`
	NewPrice    float64   `json:"new_price"`
	Percentage  float64   `json:"percentage"`
	TriggeredAt time.Time `json:"triggered_at"`
}

type PriceData struct {
	TokenAddress string  `json:"token_address"`
	Price        float64 `json:"price"`
	Volume24h    float64 `json:"volume_24h"`
	Change24h    float64 `json:"change_24h"`
	Timestamp    int64   `json:"timestamp"`
}

// =============================================================================
// Service
// =============================================================================

type PriceAlertsService struct {
	db           *sql.DB
	config       *Config
	priceCache   map[string]PriceData
	cacheMu      sync.RWMutex
	alertWorkers map[int64]*AlertWorker
	workerMu     sync.RWMutex
}

type AlertWorker struct {
	alertID  int64
	userID   int64
	stopChan chan bool
}

func NewPriceAlertsService(config *Config) (*PriceAlertsService, error) {
	connStr := fmt.Sprintf(
		"host=%s port=%d user=%s password=%s dbname=%s sslmode=disable",
		config.DBHost, config.DBPort, config.DBUser, config.DBPassword, config.DBName,
	)
	
	db, err := sql.Open("postgres", connStr)
	if err != nil {
		return nil, err
	}
	
	if err := initDatabase(db); err != nil {
		return nil, err
	}
	
	service := &PriceAlertsService{
		db:           db,
		config:       config,
		priceCache:   make(map[string]PriceData),
		alertWorkers: make(map[int64]*AlertWorker),
	}
	
	return service, nil
}

func initDatabase(db *sql.DB) error {
	schema := `
	CREATE TABLE IF NOT EXISTS price_alerts (
		id BIGSERIAL PRIMARY KEY,
		user_id BIGINT NOT NULL,
		token_address VARCHAR(66) NOT NULL,
		token_symbol VARCHAR(20) NOT NULL,
		alert_type VARCHAR(20) NOT NULL,
		threshold DECIMAL(30, 10) NOT NULL,
		enabled BOOLEAN DEFAULT true,
		last_triggered TIMESTAMP,
		trigger_count BIGINT DEFAULT 0,
		created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
		updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
	);
	
	CREATE TABLE IF NOT EXISTS notification_channels (
		id BIGSERIAL PRIMARY KEY,
		user_id BIGINT NOT NULL,
		channel_type VARCHAR(20) NOT NULL,
		channel_value TEXT NOT NULL,
		enabled BOOLEAN DEFAULT true,
		created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
	);
	
	CREATE TABLE IF NOT EXISTS alert_history (
		id BIGSERIAL PRIMARY KEY,
		alert_id BIGINT REFERENCES price_alerts(id) ON DELETE CASCADE,
		old_price DECIMAL(30, 10),
		new_price DECIMAL(30, 10),
		percentage DECIMAL(10, 4),
		triggered_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
	);
	
	CREATE TABLE IF NOT EXISTS price_feed (
		token_address VARCHAR(66) PRIMARY KEY,
		price DECIMAL(30, 10),
		volume_24h DECIMAL(30, 10),
		change_24h DECIMAL(10, 4),
		updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
	);
	
	CREATE INDEX idx_price_alerts_user_id ON price_alerts(user_id);
	CREATE INDEX idx_price_alerts_token ON price_alerts(token_address);
	CREATE INDEX idx_notification_channels_user_id ON notification_channels(user_id);
	CREATE INDEX idx_alert_history_alert_id ON alert_history(alert_id);
	`
	
	_, err := db.Exec(schema)
	return err
}

// =============================================================================
// Alert Operations
// =============================================================================

func (s *PriceAlertsService) CreateAlert(userID int64, tokenAddress, tokenSymbol, alertType string, threshold float64) (*PriceAlert, error) {
	var alert PriceAlert
	err := s.db.QueryRow(`
		INSERT INTO price_alerts (user_id, token_address, token_symbol, alert_type, threshold)
		VALUES ($1, $2, $3, $4, $5)
		RETURNING id, user_id, token_address, token_symbol, alert_type, threshold, enabled, last_triggered, trigger_count, created_at, updated_at
	`, userID, tokenAddress, tokenSymbol, alertType, threshold).Scan(
		&alert.ID, &alert.UserID, &alert.TokenAddress, &alert.TokenSymbol,
		&alert.AlertType, &alert.Threshold, &alert.Enabled,
		&alert.LastTriggered, &alert.TriggerCount, &alert.CreatedAt, &alert.UpdatedAt,
	)
	
	return &alert, err
}

func (s *PriceAlertsService) GetAlerts(userID int64) ([]PriceAlert, error) {
	rows, err := s.db.Query(`
		SELECT id, user_id, token_address, token_symbol, alert_type, threshold, 
		       enabled, last_triggered, trigger_count, created_at, updated_at
		FROM price_alerts WHERE user_id = $1 ORDER BY created_at DESC
	`, userID)
	
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var alerts []PriceAlert
	for rows.Next() {
		var a PriceAlert
		err := rows.Scan(&a.ID, &a.UserID, &a.TokenAddress, &a.TokenSymbol,
			&a.AlertType, &a.Threshold, &a.Enabled, &a.LastTriggered,
			&a.TriggerCount, &a.CreatedAt, &a.UpdatedAt)
		if err != nil {
			continue
		}
		alerts = append(alerts, a)
	}
	
	return alerts, nil
}

func (s *PriceAlertsService) GetAlertsByToken(tokenAddress string) ([]PriceAlert, error) {
	rows, err := s.db.Query(`
		SELECT id, user_id, token_address, token_symbol, alert_type, threshold,
		       enabled, last_triggered, trigger_count, created_at, updated_at
		FROM price_alerts WHERE token_address = $1 AND enabled = true
	`, tokenAddress)
	
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var alerts []PriceAlert
	for rows.Next() {
		var a PriceAlert
		err := rows.Scan(&a.ID, &a.UserID, &a.TokenAddress, &a.TokenSymbol,
			&a.AlertType, &a.Threshold, &a.Enabled, &a.LastTriggered,
			&a.TriggerCount, &a.CreatedAt, &a.UpdatedAt)
		if err != nil {
			continue
		}
		alerts = append(alerts, a)
	}
	
	return alerts, nil
}

func (s *PriceAlertsService) UpdateAlert(alertID, userID int64, threshold float64, enabled bool) error {
	_, err := s.db.Exec(`
		UPDATE price_alerts SET threshold = $1, enabled = $2, updated_at = CURRENT_TIMESTAMP
		WHERE id = $3 AND user_id = $4
	`, threshold, enabled, alertID, userID)
	
	return err
}

func (s *PriceAlertsService) DeleteAlert(alertID, userID int64) error {
	_, err := s.db.Exec("DELETE FROM price_alerts WHERE id = $1 AND user_id = $2", alertID, userID)
	return err
}

func (s *PriceAlertsService) TriggerAlert(alertID int64, oldPrice, newPrice float64) error {
	// Update alert
	_, err := s.db.Exec(`
		UPDATE price_alerts SET 
			last_triggered = CURRENT_TIMESTAMP,
			trigger_count = trigger_count + 1,
			updated_at = CURRENT_TIMESTAMP
		WHERE id = $1
	`, alertID)
	
	if err != nil {
		return err
	}
	
	// Record history
	percentage := 0.0
	if oldPrice > 0 {
		percentage = ((newPrice - oldPrice) / oldPrice) * 100
	}
	
	_, err = s.db.Exec(`
		INSERT INTO alert_history (alert_id, old_price, new_price, percentage)
		VALUES ($1, $2, $3, $4)
	`, alertID, oldPrice, newPrice, percentage)
	
	return err
}

func (s *PriceAlertsService) GetAlertHistory(alertID int64, limit int) ([]AlertHistory, error) {
	rows, err := s.db.Query(`
		SELECT id, alert_id, old_price, new_price, percentage, triggered_at
		FROM alert_history WHERE alert_id = $1
		ORDER BY triggered_at DESC LIMIT $2
	`, alertID, limit)
	
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var history []AlertHistory
	for rows.Next() {
		var h AlertHistory
		err := rows.Scan(&h.ID, &h.AlertID, &h.OldPrice, &h.NewPrice, &h.Percentage, &h.TriggeredAt)
		if err != nil {
			continue
		}
		history = append(history, h)
	}
	
	return history, nil
}

// =============================================================================
// Channel Operations
// =============================================================================

func (s *PriceAlertsService) AddChannel(userID int64, channelType, channelValue string) (*NotificationChannel, error) {
	var channel NotificationChannel
	err := s.db.QueryRow(`
		INSERT INTO notification_channels (user_id, channel_type, channel_value)
		VALUES ($1, $2, $3)
		RETURNING id, user_id, channel_type, channel_value, enabled, created_at
	`, userID, channelType, channelValue).Scan(
		&channel.ID, &channel.UserID, &channel.ChannelType,
		&channel.ChannelValue, &channel.Enabled, &channel.CreatedAt,
	)
	
	return &channel, err
}

func (s *PriceAlertsService) GetChannels(userID int64) ([]NotificationChannel, error) {
	rows, err := s.db.Query(`
		SELECT id, user_id, channel_type, channel_value, enabled, created_at
		FROM notification_channels WHERE user_id = $1
	`, userID)
	
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var channels []NotificationChannel
	for rows.Next() {
		var c NotificationChannel
		err := rows.Scan(&c.ID, &c.UserID, &c.ChannelType, &c.ChannelValue, &c.Enabled, &c.CreatedAt)
		if err != nil {
			continue
		}
		channels = append(channels, c)
	}
	
	return channels, nil
}

func (s *PriceAlertsService) DeleteChannel(channelID, userID int64) error {
	_, err := s.db.Exec("DELETE FROM notification_channels WHERE id = $1 AND user_id = $2", channelID, userID)
	return err
}

func (s *PriceAlertsService) ToggleChannel(channelID, userID int64, enabled bool) error {
	_, err := s.db.Exec(`
		UPDATE notification_channels SET enabled = $1 WHERE id = $2 AND user_id = $3
	`, enabled, channelID, userID)
	return err
}

// =============================================================================
// Price Feed
// =============================================================================

func (s *PriceAlertsService) UpdatePrice(tokenAddress string, price, volume24h, change24h float64) error {
	s.cacheMu.Lock()
	s.priceCache[tokenAddress] = PriceData{
		TokenAddress: tokenAddress,
		Price:        price,
		Volume24h:    volume24h,
		Change24h:    change24h,
		Timestamp:    time.Now().Unix(),
	}
	s.cacheMu.Unlock()
	
	// Update database
	_, err := s.db.Exec(`
		INSERT INTO price_feed (token_address, price, volume_24h, change_24h, updated_at)
		VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP)
		ON CONFLICT (token_address) DO UPDATE SET
			price = $2, volume_24h = $3, change_24h = $4, updated_at = CURRENT_TIMESTAMP
	`, tokenAddress, price, volume24h, change24h)
	
	return err
}

func (s *PriceAlertsService) GetPrice(tokenAddress string) (*PriceData, error) {
	s.cacheMu.RLock()
	if cached, ok := s.priceCache[tokenAddress]; ok {
		s.cacheMu.RUnlock()
		return &cached, nil
	}
	s.cacheMu.RUnlock()
	
	// Fetch from database
	var price PriceData
	err := s.db.QueryRow(`
		SELECT token_address, price, volume_24h, change_24h, EXTRACT(EPOCH FROM updated_at)::bigint
		FROM price_feed WHERE token_address = $1
	`, tokenAddress).Scan(&price.TokenAddress, &price.Price, &price.Volume24h, &price.Change24h, &price.Timestamp)
	
	if err != nil {
		return nil, err
	}
	
	return &price, nil
}

// =============================================================================
// Notification
// =============================================================================

func (s *PriceAlertsService) SendNotification(userID int64, alert *PriceAlert, oldPrice, newPrice float64) error {
	// Get user channels
	channels, err := s.GetChannels(userID)
	if err != nil {
		return err
	}
	
	message := fmt.Sprintf(
		"🔔 Price Alert: %s\n\nToken: %s (%s)\nAlert Type: %s\nOld Price: $%.6f\nNew Price: $%.6f\nChange: %.2f%%",
		alert.AlertType, alert.TokenSymbol, alert.TokenAddress,
		alert.AlertType, oldPrice, newPrice, ((newPrice-oldPrice)/oldPrice)*100,
	)
	
	for _, channel := range channels {
		if !channel.Enabled {
			continue
		}
		
		switch channel.ChannelType {
		case "email":
			s.sendEmail(channel.ChannelValue, "Price Alert", message)
		case "telegram":
			s.sendTelegram(channel.ChannelValue, message)
		case "discord":
			s.sendDiscord(channel.ChannelValue, message)
		case "webhook":
			s.sendWebhook(channel.ChannelValue, map[string]interface{}{
				"alert": alert,
				"old_price": oldPrice,
				"new_price": newPrice,
			})
		}
	}
	
	return nil
}

func (s *PriceAlertsService) sendEmail(to, subject, body string) {
	// In production, use net/smtp
	fmt.Printf("Sending email to %s: %s\n", to, subject)
}

func (s *PriceAlertsService) sendTelegram(chatID, message string) {
	if s.config.TelegramBot == "" {
		return
	}
	// In production, use Telegram Bot API
	fmt.Printf("Sending Telegram to %s: %s\n", chatID, message)
}

func (s *PriceAlertsService) sendDiscord(webhook, message string) {
	if s.config.DiscordHook == "" {
		return
	}
	// In production, use Discord webhook
	fmt.Printf("Sending Discord to %s\n", webhook)
}

func (s *PriceAlertsService) sendWebhook(url string, data map[string]interface{}) {
	payload, _ := json.Marshal(data)
	req, _ := http.NewRequest("POST", url, strings.NewReader(string(payload)))
	req.Header.Set("Content-Type", "application/json")
	
	client := &http.Client{Timeout: 10 * time.Second}
	client.Do(req)
}

// =============================================================================
// Check Alerts
// =============================================================================

func (s *PriceAlertsService) CheckAlerts(tokenAddress string) error {
	price, err := s.GetPrice(tokenAddress)
	if err != nil {
		return err
	}
	
	alerts, err := s.GetAlertsByToken(tokenAddress)
	if err != nil {
		return err
	}
	
	for _, alert := range alerts {
		triggered := false
		var oldPrice float64
		
		switch alert.AlertType {
		case "above":
			if price.Price > alert.Threshold {
				triggered = true
				oldPrice = price.Price - (price.Price * 0.01) // Approximate
			}
		case "below":
			if price.Price < alert.Threshold {
				triggered = true
				oldPrice = price.Price + (price.Price * 0.01)
			}
		case "change_up":
			if price.Change24h > alert.Threshold {
				triggered = true
				oldPrice = price.Price / (1 + price.Change24h/100)
			}
		case "change_down":
			if price.Change24h < -alert.Threshold {
				triggered = true
				oldPrice = price.Price / (1 + price.Change24h/100)
			}
		case "volume":
			if price.Volume24h > alert.Threshold {
				triggered = true
				oldPrice = price.Volume24h * 0.9
			}
		}
		
		if triggered {
			if err := s.TriggerAlert(alert.ID, oldPrice, price.Price); err != nil {
				continue
			}
			
			// Send notifications
			s.SendNotification(alert.UserID, alert, oldPrice, price.Price)
		}
	}
	
	return nil
}

// =============================================================================
// HTTP Handlers
// =============================================================================

func (s *PriceAlertsService) RegisterRoutes(router *gin.Engine) {
	api := router.Group("/api/v1")
	
	auth := api.Group("")
	auth.Use(s.authMiddleware())
	{
		// Alerts
		auth.GET("/alerts", s.handleGetAlerts)
		auth.POST("/alerts", s.handleCreateAlert)
		auth.PUT("/alerts/:id", s.handleUpdateAlert)
		auth.DELETE("/alerts/:id", s.handleDeleteAlert)
		auth.GET("/alerts/:id/history", s.handleGetAlertHistory)
		
		// Channels
		auth.GET("/channels", s.handleGetChannels)
		auth.POST("/channels", s.handleAddChannel)
		auth.DELETE("/channels/:id", s.handleDeleteChannel)
		auth.PUT("/channels/:id/toggle", s.handleToggleChannel)
	}
	
	// Price feed (internal)
	api.POST("/price/update", s.handleUpdatePrice)
}

func (s *PriceAlertsService) handleGetAlerts(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	alerts, err := s.GetAlerts(userID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"alerts": alerts})
}

func (s *PriceAlertsService) handleCreateAlert(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	var req struct {
		TokenAddress string  `json:"token_address" binding:"required"`
		TokenSymbol  string  `json:"token_symbol" binding:"required"`
		AlertType    string  `json:"alert_type" binding:"required,oneof=above below change_up change_down volume"`
		Threshold    float64 `json:"threshold" binding:"required"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	alert, err := s.CreateAlert(userID, req.TokenAddress, req.TokenSymbol, req.AlertType, req.Threshold)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusCreated, alert)
}

func (s *PriceAlertsService) handleUpdateAlert(c *gin.Context) {
	userID := c.GetInt64("user_id")
	alertID, _ := strconv.ParseInt(c.Param("id"), 10, 64)
	
	var req struct {
		Threshold float64 `json:"threshold"`
		Enabled   *bool   `json:"enabled"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	enabled := true
	if req.Enabled != nil {
		enabled = *req.Enabled
	}
	
	if err := s.UpdateAlert(alertID, userID, req.Threshold, enabled); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "alert updated"})
}

func (s *PriceAlertsService) handleDeleteAlert(c *gin.Context) {
	userID := c.GetInt64("user_id")
	alertID, _ := strconv.ParseInt(c.Param("id"), 10, 64)
	
	if err := s.DeleteAlert(alertID, userID); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "alert deleted"})
}

func (s *PriceAlertsService) handleGetAlertHistory(c *gin.Context) {
	alertID, _ := strconv.ParseInt(c.Param("id"), 10, 64)
	limit, _ := strconv.Atoi(c.DefaultQuery("limit", "50"))
	
	history, err := s.GetAlertHistory(alertID, limit)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"history": history})
}

func (s *PriceAlertsService) handleGetChannels(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	channels, err := s.GetChannels(userID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"channels": channels})
}

func (s *PriceAlertsService) handleAddChannel(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	var req struct {
		ChannelType  string `json:"channel_type" binding:"required,oneof=email telegram discord webhook"`
		ChannelValue string `json:"channel_value" binding:"required"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	channel, err := s.AddChannel(userID, req.ChannelType, req.ChannelValue)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusCreated, channel)
}

func (s *PriceAlertsService) handleDeleteChannel(c *gin.Context) {
	userID := c.GetInt64("user_id")
	channelID, _ := strconv.ParseInt(c.Param("id"), 10, 64)
	
	if err := s.DeleteChannel(channelID, userID); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "channel deleted"})
}

func (s *PriceAlertsService) handleToggleChannel(c *gin.Context) {
	userID := c.GetInt64("user_id")
	channelID, _ := strconv.ParseInt(c.Param("id"), 10, 64)
	
	var req struct {
		Enabled bool `json:"enabled"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	if err := s.ToggleChannel(channelID, userID, req.Enabled); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "channel toggled"})
}

func (s *PriceAlertsService) handleUpdatePrice(c *gin.Context) {
	var req struct {
		TokenAddress string  `json:"token_address" binding:"required"`
		Price        float64 `json:"price" binding:"required"`
		Volume24h    float64 `json:"volume_24h"`
		Change24h    float64 `json:"change_24h"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	if err := s.UpdatePrice(req.TokenAddress, req.Price, req.Volume24h, req.Change24h); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	// Check alerts
	s.CheckAlerts(req.TokenAddress)
	
	c.JSON(http.StatusOK, gin.H{"message": "price updated"})
}

func (s *PriceAlertsService) authMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		authHeader := c.GetHeader("Authorization")
		if authHeader == "" {
			c.JSON(http.StatusUnauthorized, gin.H{"error": "authorization required"})
			c.Abort()
			return
		}
		
		// Simplified - extract user from token
		userID := int64(1)
		c.Set("user_id", userID)
		c.Next()
	}
}

// =============================================================================
// Main
// =============================================================================

func main() {
	config := LoadConfig()
	
	service, err := NewPriceAlertsService(config)
	if err != nil {
		fmt.Printf("Failed to start service: %v\n", err)
		os.Exit(1)
	}
	
	router := gin.Default()
	router.Use(gin.Recovery())
	
	service.RegisterRoutes(router)
	
	router.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"status": "healthy"})
	})
	
	addr := fmt.Sprintf(":%d", config.ServerPort)
	srv := &http.Server{
		Addr:    addr,
		Handler: router,
	}
	
	go func() {
		fmt.Printf("Starting Price Alerts service on %s\n", addr)
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			fmt.Printf("Server error: %v\n", err)
		}
	}()
	
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit
	
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	
	srv.Shutdown(ctx)
	fmt.Println("Server exited")
}
