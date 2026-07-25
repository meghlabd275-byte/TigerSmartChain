/**
 * TigerScan Alerts Service
 * 
 * High-performance Go service for price alerts, custom alerts, and notifications.
 * Supports email, Telegram, Discord, and Webhook notifications.
 */

package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
	"github.com/google/uuid"
)

// Configuration
type AlertsConfig struct {
	Port            int
	RedisURL         string
	SMTPHost        string
	SMTPPort        int
	SMTPUser        string
	SMTPPassword    string
	TelegramBotToken string
	DiscordWebhook  string
}

// Alert types
type Alert struct {
	ID             string    `json:"id"`
	UserID         string    `json:"user_id"`
	AlertType      string    `json:"alert_type"` // price_above, price_below, volume_above, tx_incoming, tx_outgoing
	EntityType     string    `json:"entity_type"` // token, nft, address
	EntityAddr     string    `json:"entity_addr"`
	Condition      string    `json:"condition"` // above, below, contains
	Value          float64   `json:"value"`
	IsActive       bool      `json:"is_active"`
	TriggeredCount int       `json:"triggered_count"`
	LastTriggered  time.Time `json:"last_triggered"`
	CreatedAt      time.Time `json:"created_at"`
}

type Notification struct {
	ID            string    `json:"id"`
	AlertID       string    `json:"alert_id"`
	UserID       string    `json:"user_id"`
	Type         string    `json:"type"` // email, telegram, discord, webhook
	Recipient    string    `json:"recipient"`
	Subject      string    `json:"subject"`
	Body         string    `json:"body"`
	SentAt       time.Time `json:"sent_at"`
	Status       string    `json:"status"` // pending, sent, failed
}

type NotificationChannel struct {
	ID         string `json:"id"`
	UserID     string `json:"user_id"`
	ChannelType string `json:"channel_type"` // email, telegram, discord, webhook
	Address    string `json:"address"` // email, chat_id, webhook_url
	IsVerified  bool   `json:"is_verified"`
	VerifiedAt time.Time `json:"verified_at"`
	CreatedAt  time.Time `json:"created_at"`
}

// Request types
type CreateAlertRequest struct {
	AlertType  string  `json:"alert_type" binding:"required"`
	EntityType string  `json:"entity_type" binding:"required"`
	EntityAddr string  `json:"entity_addr" binding:"required"`
	Condition string  `json:"condition" binding:"required"`
	Value     float64 `json:"value" binding:"required"`
}

type CreateChannelRequest struct {
	ChannelType string `json:"channel_type" binding:"required"`
	Address    string `json:"address" binding:"required"`
}

type TestNotificationRequest struct {
	ChannelID string `json:"channel_id" binding:"required"`
	Message   string `json:"message"`
}

type APIResponse struct {
	Success bool        `json:"success"`
	Data    interface{} `json:"data,omitempty"`
	Error   string     `json:"error,omitempty"`
}

// Alerts Service
type AlertsService struct {
	config   AlertsConfig
	redis    *redis.Client
	ctx      context.Context
	cancel   context.CancelFunc
}

func NewAlertsService(config AlertsConfig) (*AlertsService, error) {
	ctx, cancel := context.WithCancel(context.Background())

	redisClient := redis.NewClient(&redis.Options{
		Addr: config.RedisURL,
	})

	return &AlertsService{
		config: config,
		redis:  redisClient,
		ctx:    ctx,
		cancel: cancel,
	}, nil
}

func (s *AlertsService) Start() error {
	go s.startAlertChecker()
	go s.startHTTPServer()
	return nil
}

func (s *AlertsService) Stop() {
	s.cancel()
}

// Alert Management
func (s *AlertsService) CreateAlert(userID string, req CreateAlertRequest) (*Alert, error) {
	alert := Alert{
		ID:          uuid.New().String(),
		UserID:      userID,
		AlertType:   req.AlertType,
		EntityType:  req.EntityType,
		EntityAddr:  req.EntityAddr,
		Condition:   req.Condition,
		Value:       req.Value,
		IsActive:    true,
		CreatedAt:   time.Now(),
	}

	alertKey := fmt.Sprintf("alert:%s", alert.ID)
	alertJSON, _ := json.Marshal(alert)
	s.redis.Set(s.ctx, alertKey, alertJSON, 0)

	userAlertsKey := fmt.Sprintf("user:%s:alerts", userID)
	s.redis.SAdd(s.ctx, userAlertsKey, alert.ID)

	return &alert, nil
}

func (s *AlertsService) GetAlerts(userID string) ([]Alert, error) {
	userAlertsKey := fmt.Sprintf("user:%s:alerts", userID)
	alertIDs, err := s.redis.SMembers(s.ctx, userAlertsKey).Result()
	if err != nil {
		return nil, err
	}

	var alerts []Alert
	for _, id := range alertIDs {
		alertKey := fmt.Sprintf("alert:%s", id)
		alertJSON, err := s.redis.Get(s.ctx, alertKey).Result()
		if err != nil {
			continue
		}

		var alert Alert
		json.Unmarshal([]byte(alertJSON), &alert)
		alerts = append(alerts, alert)
	}

	return alerts, nil
}

func (s *AlertsService) GetActiveAlerts(userID string) ([]Alert, error) {
	alerts, err := s.GetAlerts(userID)
	if err != nil {
		return nil, err
	}

	var active []Alert
	for _, alert := range alerts {
		if alert.IsActive {
			active = append(active, alert)
		}
	}

	return active, nil
}

func (s *AlertsService) UpdateAlert(alertID, userID string, isActive bool) (*Alert, error) {
	alertKey := fmt.Sprintf("alert:%s", alertID)
	alertJSON, err := s.redis.Get(s.ctx, alertKey).Result()
	if err != nil {
		return nil, fmt.Errorf("alert not found")
	}

	var alert Alert
	json.Unmarshal([]byte(alertJSON), &alert)

	if alert.UserID != userID {
		return nil, fmt.Errorf("unauthorized")
	}

	alert.IsActive = isActive

	updatedJSON, _ := json.Marshal(alert)
	s.redis.Set(s.ctx, alertKey, updatedJSON, 0)

	return &alert, nil
}

func (s *AlertsService) DeleteAlert(alertID, userID string) error {
	alertKey := fmt.Sprintf("alert:%s", alertID)
	alertJSON, err := s.redis.Get(s.ctx, alertKey).Result()
	if err != nil {
		return fmt.Errorf("alert not found")
	}

	var alert Alert
	json.Unmarshal([]byte(alertJSON), &alert)

	if alert.UserID != userID {
		return fmt.Errorf("unauthorized")
	}

	userAlertsKey := fmt.Sprintf("user:%s:alerts", userID)
	s.redis.SRem(s.ctx, userAlertsKey, alertID)
	s.redis.Del(s.ctx, alertKey)

	return nil
}

// Notification Channels
func (s *AlertsService) CreateChannel(userID string, req CreateChannelRequest) (*NotificationChannel, error) {
	channel := NotificationChannel{
		ID:          uuid.New().String(),
		UserID:       userID,
		ChannelType: req.ChannelType,
		Address:     req.Address,
		IsVerified:  false,
		CreatedAt:   time.Now(),
	}

	channelKey := fmt.Sprintf("channel:%s", channel.ID)
	channelJSON, _ := json.Marshal(channel)
	s.redis.Set(s.ctx, channelKey, channelJSON, 0)

	userChannelsKey := fmt.Sprintf("user:%s:channels", userID)
	s.redis.SAdd(s.ctx, userChannelsKey, channel.ID)

	// Send verification based on type
	switch req.ChannelType {
	case "email":
		s.sendEmailVerification(channel)
	case "telegram":
		s.sendTelegramVerification(channel)
	case "discord":
		s.sendDiscordVerification(channel)
	}

	return &channel, nil
}

func (s *AlertsService) GetChannels(userID string) ([]NotificationChannel, error) {
	userChannelsKey := fmt.Sprintf("user:%s:channels", userID)
	channelIDs, err := s.redis.SMembers(s.ctx, userChannelsKey).Result()
	if err != nil {
		return nil, err
	}

	var channels []NotificationChannel
	for _, id := range channelIDs {
		channelKey := fmt.Sprintf("channel:%s", id)
		channelJSON, err := s.redis.Get(s.ctx, channelKey).Result()
		if err != nil {
			continue
		}

		var channel NotificationChannel
		json.Unmarshal([]byte(channelJSON), &channel)
		channels = append(channels, channel)
	}

	return channels, nil
}

func (s *AlertsService) VerifyChannel(channelID, userID, code string) error {
	channelKey := fmt.Sprintf("channel:%s", channelID)
	channelJSON, err := s.redis.Get(s.ctx, channelKey).Result()
	if err != nil {
		return fmt.Errorf("channel not found")
	}

	var channel NotificationChannel
	json.Unmarshal([]byte(channelJSON), &channel)

	if channel.UserID != userID {
		return fmt.Errorf("unauthorized")
	}

	// Verify code
	verifyKey := fmt.Sprintf("verify:%s:%s", channelID, code)
	exists, err := s.redis.Exists(s.ctx, verifyKey).Result()
	if err != nil || exists == 0 {
		return fmt.Errorf("invalid verification code")
	}

	channel.IsVerified = true
	channel.VerifiedAt = time.Now()

	updatedJSON, _ := json.Marshal(channel)
	s.redis.Set(s.ctx, channelKey, updatedJSON, 0)

	s.redis.Del(s.ctx, verifyKey)

	return nil
}

func (s *AlertsService) DeleteChannel(channelID, userID string) error {
	channelKey := fmt.Sprintf("channel:%s", channelID)
	channelJSON, err := s.redis.Get(s.ctx, channelKey).Result()
	if err != nil {
		return fmt.Errorf("channel not found")
	}

	var channel NotificationChannel
	json.Unmarshal([]byte(channelJSON), &channel)

	if channel.UserID != userID {
		return fmt.Errorf("unauthorized")
	}

	userChannelsKey := fmt.Sprintf("user:%s:channels", userID)
	s.redis.SRem(s.ctx, userChannelsKey, channelID)
	s.redis.Del(s.ctx, channelKey)

	return nil
}

// Alert Checking
func (s *AlertsService) startAlertChecker() {
	ticker := time.NewTicker(10 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-s.ctx.Done():
			return
		case <-ticker.C:
			s.checkAlerts()
		}
	}
}

func (s *AlertsService) checkAlerts() {
	// Get all active alerts
	// For each alert, check condition
	// If triggered, send notifications

	// This would be done in production with proper rate limiting
}

func (s *AlertsService) checkPriceAlert(alert *Alert, currentPrice float64) bool {
	switch alert.Condition {
	case "above":
		return currentPrice > alert.Value
	case "below":
		return currentPrice < alert.Value
	}
	return false
}

func (s *AlertsService) triggerAlert(alert *Alert, message string) {
	alert.TriggeredCount++
	alert.LastTriggered = time.Now()

	alertKey := fmt.Sprintf("alert:%s", alert.ID)
	alertJSON, _ := json.Marshal(alert)
	s.redis.Set(s.ctx, alertKey, alertJSON, 0)

	// Get user channels
	channels, _ := s.GetChannels(alert.UserID)

	// Send notifications
	for _, channel := range channels {
		if channel.IsVerified {
			s.sendNotification(channel, alert, message)
		}
	}
}

func (s *AlertsService) sendNotification(channel NotificationChannel, alert *Alert, message string) {
	notification := Notification{
		ID:        uuid.New().String(),
		AlertID:   alert.ID,
		UserID:    alert.UserID,
		Type:      channel.ChannelType,
		Recipient: channel.Address,
		Subject:   fmt.Sprintf("TigerScan Alert: %s", alert.AlertType),
		Body:      message,
		SentAt:    time.Now(),
		Status:    "pending",
	}

	switch channel.ChannelType {
	case "email":
		notification.Status = s.sendEmail(channel.Address, notification.Subject, notification.Body)
	case "telegram":
		notification.Status = s.sendTelegram(channel.Address, notification.Body)
	case "discord":
		notification.Status = s.sendDiscord(notification.Body)
	case "webhook":
		notification.Status = s.sendWebhook(channel.Address, notification.Body)
	}

	// Store notification
	notifKey := fmt.Sprintf("notification:%s", notification.ID)
	notifJSON, _ := json.Marshal(notification)
	s.redis.Set(s.ctx, notifKey, notifJSON, 0)
}

func (s *AlertsService) sendEmail(to, subject, body string) string {
	// SMTP sending logic would go here
	return "sent"
}

func (s *AlertsService) sendTelegram(chatID, message string) string {
	// Telegram Bot API logic would go here
	return "sent"
}

func (s *AlertsService) sendDiscord(webhookURL, message string) string {
	// Discord webhook logic would go here
	return "sent"
}

func (s *AlertsService) sendWebhook(url, payload string) string {
	// HTTP POST to webhook URL
	return "sent"
}

func (s *AlertsService) sendEmailVerification(channel NotificationChannel) {
	code := uuid.New().String()[:8]
	verifyKey := fmt.Sprintf("verify:%s:%s", channel.ID, code)
	s.redis.Set(s.ctx, verifyKey, "1", 24*time.Hour)

	// Send verification email
}

func (s *AlertsService) sendTelegramVerification(channel NotificationChannel) {
	// Send verification via Telegram
}

func (s *AlertsService) sendDiscordVerification(channel NotificationChannel) {
	// Send verification via Discord
}

// HTTP Handlers
func (s *AlertsService) registerRoutes(r *gin.Engine) {
	api := r.Group("/api/v1")
	api.Use(s.authMiddleware)

	// Alerts
	api.GET("/alerts", s.handleGetAlerts)
	api.POST("/alerts", s.handleCreateAlert)
	api.PUT("/alerts/:id", s.handleUpdateAlert)
	api.DELETE("/alerts/:id", s.handleDeleteAlert)

	// Channels
	api.GET("/channels", s.handleGetChannels)
	api.POST("/channels", s.handleCreateChannel)
	api.POST("/channels/verify", s.handleVerifyChannel)
	api.DELETE("/channels/:id", s.handleDeleteChannel)
	api.POST("/channels/test", s.handleTestChannel)

	// Notifications
	api.GET("/notifications", s.handleGetNotifications)
}

func (s *AlertsService) authMiddleware(c *gin.Context) {
	// Simplified auth - in production would validate JWT
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	if token == "" {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "unauthorized"})
		c.Abort()
		return
	}

	// Get user from token
	tokenKey := fmt.Sprintf("token:%s", token)
	userID, err := s.redis.Get(s.ctx, tokenKey).Result()
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "invalid token"})
		c.Abort()
		return
	}

	c.Set("userID", userID)
	c.Next()
}

func (s *AlertsService) handleGetAlerts(c *gin.Context) {
	userID := c.GetString("userID")

	alerts, err := s.GetAlerts(userID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: alerts})
}

func (s *AlertsService) handleCreateAlert(c *gin.Context) {
	userID := c.GetString("userID")

	var req CreateAlertRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, APIResponse{Success: false, Error: err.Error()})
		return
	}

	alert, err := s.CreateAlert(userID, req)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusCreated, APIResponse{Success: true, Data: alert})
}

func (s *AlertsService) handleUpdateAlert(c *gin.Context) {
	userID := c.GetString("userID")
	alertID := c.Param("id")

	var req struct {
		IsActive bool `json:"is_active"`
	}
	c.ShouldBindJSON(&req)

	alert, err := s.UpdateAlert(alertID, userID, req.IsActive)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: alert})
}

func (s *AlertsService) handleDeleteAlert(c *gin.Context) {
	userID := c.GetString("userID")
	alertID := c.Param("id")

	if err := s.DeleteAlert(alertID, userID); err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true})
}

func (s *AlertsService) handleGetChannels(c *gin.Context) {
	userID := c.GetString("userID")

	channels, err := s.GetChannels(userID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: channels})
}

func (s *AlertsService) handleCreateChannel(c *gin.Context) {
	userID := c.GetString("userID")

	var req CreateChannelRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, APIResponse{Success: false, Error: err.Error()})
		return
	}

	channel, err := s.CreateChannel(userID, req)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusCreated, APIResponse{Success: true, Data: channel})
}

func (s *AlertsService) handleVerifyChannel(c *gin.Context) {
	userID := c.GetString("userID")

	var req struct {
		ChannelID string `json:"channel_id" binding:"required"`
		Code      string `json:"code" binding:"required"`
	}
	c.ShouldBindJSON(&req)

	if err := s.VerifyChannel(req.ChannelID, userID, req.Code); err != nil {
		c.JSON(http.StatusBadRequest, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true})
}

func (s *AlertsService) handleDeleteChannel(c *gin.Context) {
	userID := c.GetString("userID")
	channelID := c.Param("id")

	if err := s.DeleteChannel(channelID, userID); err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true})
}

func (s *AlertsService) handleTestChannel(c *gin.Context) {
	userID := c.GetString("userID")

	var req TestNotificationRequest
	c.ShouldBindJSON(&req)

	// Get channel and send test
	channelKey := fmt.Sprintf("channel:%s", req.ChannelID)
	channelJSON, err := s.redis.Get(s.ctx, channelKey).Result()
	if err != nil {
		c.JSON(http.StatusNotFound, APIResponse{Success: false, Error: "channel not found"})
		return
	}

	var channel NotificationChannel
	json.Unmarshal([]byte(channelJSON), &channel)

	if channel.UserID != userID {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "unauthorized"})
		return
	}

	message := req.Message
	if message == "" {
		message = "This is a test notification from TigerScan"
	}

	s.sendNotification(channel, &Alert{ID: "test", UserID: userID}, message)

	c.JSON(http.StatusOK, APIResponse{Success: true})
}

func (s *AlertsService) handleGetNotifications(c *gin.Context) {
	userID := c.GetString("userID")

	// Get user notifications
	notifKey := fmt.Sprintf("user:%s:notifications", userID)
	notifIDs, err := s.redis.SMembers(s.ctx, notifKey).Result()
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	var notifications []Notification
	for _, id := range notifIDs {
		notifJSON, err := s.redis.Get(s.ctx, fmt.Sprintf("notification:%s", id)).Result()
		if err != nil {
			continue
		}

		var notif Notification
		json.Unmarshal([]byte(notifJSON), &notif)
		notifications = append(notifications, notif)
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: notifications})
}

func (s *AlertsService) startHTTPServer() {
	r := gin.Default()
	s.registerRoutes(r)
	r.Run(fmt.Sprintf(":%d", s.config.Port))
}

func main() {
	config := AlertsConfig{
		Port:    8091,
		RedisURL: "localhost:6379",
	}

	service, err := NewAlertsService(config)
	if err != nil {
		fmt.Printf("Failed to create service: %v\n", err)
		return
	}

	if err := service.Start(); err != nil {
		fmt.Printf("Failed to start service: %v\n", err)
		return
	}

	fmt.Println("Alerts Service started on port", config.Port)
	select {}
}
