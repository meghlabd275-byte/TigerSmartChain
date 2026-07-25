/**
 * TigerScan Comments & Notes Service
 * 
 * High-performance Go service for user annotations on addresses, transactions, and blocks.
 * Supports comments, notes, tags, and custom labels.
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
	"github.com/gorilla/websocket"
	"github.com/redis/go-redis/v9"
	"github.com/google/uuid"
	"golang.org/x/crypto/bcrypt"
)

// Configuration
type Config struct {
	Port           int
	RedisURL       string
	PostgresURL    string
	JWTSecret      string
	MaxCommentLen  int
	MaxNotesPerUser int
}

// User types
type User struct {
	ID           string    `json:"id"`
	Email        string    `json:"email"`
	Username     string    `json:"username"`
	PasswordHash string    `json:"-"`
	CreatedAt    time.Time `json:"created_at"`
	UpdatedAt    time.Time `json:"updated_at"`
	IsPro        bool      `json:"is_pro"`
}

// Comment types
type Comment struct {
	ID         string    `json:"id"`
	UserID     string    `json:"user_id"`
	EntityType string    `json:"entity_type"` // address, tx, block, token, nft
	EntityAddr string    `json:"entity_addr"`
	Content    string    `json:"content"`
	CreatedAt  time.Time `json:"created_at"`
	UpdatedAt  time.Time `json:"updated_at"`
	Likes      int       `json:"likes"`
	Dislikes   int       `json:"dislikes"`
}

type Note struct {
	ID          string    `json:"id"`
	UserID     string    `json:"user_id"`
	EntityType string    `json:"entity_type"`
	EntityAddr string    `json:"entity_addr"`
	Title      string    `json:"title"`
	Content    string    `json:"content"`
	Color      string    `json:"color"` // yellow, blue, green, red, purple
	IsPublic   bool      `json:"is_public"`
	CreatedAt  time.Time `json:"created_at"`
	UpdatedAt  time.Time `json:"updated_at"`
}

type Tag struct {
	ID        string   `json:"id"`
	UserID    string   `json:"user_id"`
	Name      string   `json:"name"`
	Color     string   `json:"color"`
	EntityIDs []string `json:"entity_ids"`
}

type WatchlistItem struct {
	ID          string    `json:"id"`
	UserID     string    `json:"user_id"`
	EntityType string    `json:"entity_type"`
	EntityAddr string    `json:"entity_addr"`
	AlertType  string    `json:"alert_type"` // any_tx, incoming, outgoing, large_tx
	Threshold  string    `json:"threshold,omitempty"`
	CreatedAt  time.Time `json:"created_at"`
}

// Request/Response types
type RegisterRequest struct {
	Email    string `json:"email" binding:"required,email"`
	Username string `json:"username" binding:"required,min=3,max=30"`
	Password string `json:"password" binding:"required,min=8"`
}

type LoginRequest struct {
	Email    string `json:"email" binding:"required,email"`
	Password string `json:"password" binding:"required"`
}

type LoginResponse struct {
	Token string `json:"token"`
	User  User   `json:"user"`
}

type CreateCommentRequest struct {
	EntityType string `json:"entity_type" binding:"required"`
	EntityAddr string `json:"entity_addr" binding:"required"`
	Content    string `json:"content" binding:"required,min=1,max=1000"`
}

type CreateNoteRequest struct {
	EntityType string `json:"entity_type" binding:"required"`
	EntityAddr string `json:"entity_addr" binding:"required"`
	Title      string `json:"title" binding:"required,min=1,max=100"`
	Content    string `json:"content" binding:"required,min=1,max=5000"`
	Color      string `json:"color"`
	IsPublic   bool   `json:"is_public"`
}

type CreateTagRequest struct {
	Name      string   `json:"name" binding:"required,min=1,max=50"`
	Color     string   `json:"color" binding:"required"`
	EntityIDs []string `json:"entity_ids"`
}

type CreateWatchlistRequest struct {
	EntityType string `json:"entity_type" binding:"required"`
	EntityAddr string `json:"entity_addr" binding:"required"`
	AlertType  string `json:"alert_type" binding:"required"`
	Threshold string `json:"threshold"`
}

type APIResponse struct {
	Success bool        `json:"success"`
	Data    interface{} `json:"data,omitempty"`
	Error   string     `json:"error,omitempty"`
}

// Comments Service
type CommentsService struct {
	config   Config
	redis    *redis.Client
	wsHub   *WebSocketHub
	ctx      context.Context
	cancel   context.CancelFunc
}

type WebSocketHub struct {
	clients    map[*websocket.Conn]bool
	broadcast  chan []byte
	register   chan *websocket.Conn
	unregister chan *websocket.Conn
}

func NewWebSocketHub() *WebSocketHub {
	return &WebSocketHub{
		clients:    make(map[*websocket.Conn]bool),
		broadcast:  make(chan []byte, 256),
		register:   make(chan *websocket.Conn),
		unregister: make(chan *websocket.Conn),
	}
}

func (h *WebSocketHub) Run() {
	for {
		select {
		case client := <-h.register:
			h.clients[client] = true

		case client := <-h.unregister:
			if _, ok := h.clients[client]; ok {
				delete(h.clients, client)
				client.Close()
			}

		case message := <-h.broadcast:
			for client := range h.clients {
				err := client.WriteMessage(websocket.TextMessage, message)
				if err != nil {
					client.Close()
					delete(h.clients, client)
				}
			}
		}
	}
}

func NewCommentsService(config Config) (*CommentsService, error) {
	ctx, cancel := context.WithCancel(context.Background())

	redisClient := redis.NewClient(&redis.Options{
		Addr: config.RedisURL,
	})

	service := &CommentsService{
		config:   config,
		redis:    redisClient,
		wsHub:   NewWebSocketHub(),
		ctx:      ctx,
		cancel:   cancel,
	}

	return service, nil
}

func (s *CommentsService) Start() error {
	go s.wsHub.Run()
	go s.startHTTPServer()
	return nil
}

func (s *CommentsService) Stop() {
	s.cancel()
}

// User Management
func (s *CommentsService) Register(req RegisterRequest) (*User, error) {
	// Check if email exists
	key := fmt.Sprintf("user:email:%s", req.Email)
	exists, err := s.redis.Exists(s.ctx, key).Result()
	if err == nil && exists > 0 {
		return nil, fmt.Errorf("email already registered")
	}

	// Hash password
	hashedPassword, err := bcrypt.GenerateFromPassword([]byte(req.Password), bcrypt.DefaultCost)
	if err != nil {
		return nil, err
	}

	// Create user
	user := User{
		ID:        uuid.New().String(),
		Email:     req.Email,
		Username:  req.Username,
		CreatedAt: time.Now(),
		UpdatedAt: time.Now(),
	}

	// Store in Redis
	userKey := fmt.Sprintf("user:%s", user.ID)
	userJSON, _ := json.Marshal(user)
	s.redis.Set(s.ctx, userKey, userJSON, 0)

	emailKey := fmt.Sprintf("user:email:%s", req.Email)
	s.redis.Set(s.ctx, emailKey, user.ID, 0)

	pwdKey := fmt.Sprintf("user:%s:pwd", user.ID)
	s.redis.Set(s.ctx, pwdKey, string(hashedPassword), 0)

	return &user, nil
}

func (s *CommentsService) Login(req LoginRequest) (*LoginResponse, error) {
	// Find user by email
	emailKey := fmt.Sprintf("user:email:%s", req.Email)
	userID, err := s.redis.Get(s.ctx, emailKey).Result()
	if err != nil {
		return nil, fmt.Errorf("invalid credentials")
	}

	// Verify password
	pwdKey := fmt.Sprintf("user:%s:pwd", userID)
	storedHash, err := s.redis.Get(s.ctx, pwdKey).Result()
	if err != nil {
		return nil, fmt.Errorf("invalid credentials")
	}

	if err := bcrypt.CompareHashAndPassword([]byte(storedHash), []byte(req.Password)); err != nil {
		return nil, fmt.Errorf("invalid credentials")
	}

	// Get user
	userKey := fmt.Sprintf("user:%s", userID)
	userJSON, err := s.redis.Get(s.ctx, userKey).Result()
	if err != nil {
		return nil, err
	}

	var user User
	json.Unmarshal([]byte(userJSON), &user)

	// Generate token (simplified - use JWT in production)
	token := uuid.New().String()
	tokenKey := fmt.Sprintf("token:%s", token)
	s.redis.Set(s.ctx, tokenKey, user.ID, 24*time.Hour)

	return &LoginResponse{
		Token: token,
		User:  user,
	}, nil
}

func (s *CommentsService) getUserFromToken(token string) (*User, error) {
	tokenKey := fmt.Sprintf("token:%s", token)
	userID, err := s.redis.Get(s.ctx, tokenKey).Result()
	if err != nil {
		return nil, fmt.Errorf("invalid token")
	}

	userKey := fmt.Sprintf("user:%s", userID)
	userJSON, err := s.redis.Get(s.ctx, userKey).Result()
	if err != nil {
		return nil, err
	}

	var user User
	json.Unmarshal([]byte(userJSON), &user)

	return &user, nil
}

// Comments
func (s *CommentsService) CreateComment(userID string, req CreateCommentRequest) (*Comment, error) {
	comment := Comment{
		ID:         uuid.New().String(),
		UserID:     userID,
		EntityType: req.EntityType,
		EntityAddr: req.EntityAddr,
		Content:    req.Content,
		CreatedAt:  time.Now(),
		UpdatedAt:  time.Now(),
	}

	// Store in Redis
	commentKey := fmt.Sprintf("comment:%s", comment.ID)
	commentJSON, _ := json.Marshal(comment)
	s.redis.Set(s.ctx, commentKey, commentJSON, 0)

	// Add to entity index
	entityKey := fmt.Sprintf("entity:%s:%s:comments", req.EntityType, req.EntityAddr)
	s.redis.SAdd(s.ctx, entityKey, comment.ID)

	// Broadcast via WebSocket
	s.wsHub.broadcast <- []byte(`{"type":"new_comment","entity_type":"` + req.EntityType + `","entity_addr":"` + req.EntityAddr + `"}`)

	return &comment, nil
}

func (s *CommentsService) GetComments(entityType, entityAddr string) ([]Comment, error) {
	entityKey := fmt.Sprintf("entity:%s:%s:comments", entityType, entityAddr)
	commentIDs, err := s.redis.SMembers(s.ctx, entityKey).Result()
	if err != nil {
		return nil, err
	}

	var comments []Comment
	for _, id := range commentIDs {
		commentKey := fmt.Sprintf("comment:%s", id)
		commentJSON, err := s.redis.Get(s.ctx, commentKey).Result()
		if err != nil {
			continue
		}

		var comment Comment
		json.Unmarshal([]byte(commentJSON), &comment)
		comments = append(comments, comment)
	}

	return comments, nil
}

func (s *CommentsService) UpdateComment(commentID, userID, content string) (*Comment, error) {
	commentKey := fmt.Sprintf("comment:%s", commentID)
	commentJSON, err := s.redis.Get(s.ctx, commentKey).Result()
	if err != nil {
		return nil, fmt.Errorf("comment not found")
	}

	var comment Comment
	json.Unmarshal([]byte(commentJSON), &comment)

	if comment.UserID != userID {
		return nil, fmt.Errorf("unauthorized")
	}

	comment.Content = content
	comment.UpdatedAt = time.Now()

	updatedJSON, _ := json.Marshal(comment)
	s.redis.Set(s.ctx, commentKey, updatedJSON, 0)

	return &comment, nil
}

func (s *CommentsService) DeleteComment(commentID, userID string) error {
	commentKey := fmt.Sprintf("comment:%s", commentID)
	commentJSON, err := s.redis.Get(s.ctx, commentKey).Result()
	if err != nil {
		return fmt.Errorf("comment not found")
	}

	var comment Comment
	json.Unmarshal([]byte(commentJSON), &comment)

	if comment.UserID != userID {
		return fmt.Errorf("unauthorized")
	}

	// Remove from entity index
	entityKey := fmt.Sprintf("entity:%s:%s:comments", comment.EntityType, comment.EntityAddr)
	s.redis.SRem(s.ctx, entityKey, commentID)

	// Delete comment
	s.redis.Del(s.ctx, commentKey)

	return nil
}

// Notes
func (s *CommentsService) CreateNote(userID string, req CreateNoteRequest) (*Note, error) {
	color := req.Color
	if color == "" {
		color = "yellow"
	}

	note := Note{
		ID:          uuid.New().String(),
		UserID:      userID,
		EntityType:  req.EntityType,
		EntityAddr:  req.EntityAddr,
		Title:       req.Title,
		Content:     req.Content,
		Color:       color,
		IsPublic:    req.IsPublic,
		CreatedAt:   time.Now(),
		UpdatedAt:   time.Now(),
	}

	// Store
	noteKey := fmt.Sprintf("note:%s", note.ID)
	noteJSON, _ := json.Marshal(note)
	s.redis.Set(s.ctx, noteKey, noteJSON, 0)

	// User index
	userNotesKey := fmt.Sprintf("user:%s:notes", userID)
	s.redis.SAdd(s.ctx, userNotesKey, note.ID)

	return &note, nil
}

func (s *CommentsService) GetNotes(userID, entityType, entityAddr string) ([]Note, error) {
	notesKey := fmt.Sprintf("entity:%s:%s:notes", entityType, entityAddr)
	noteIDs, err := s.redis.SMembers(s.ctx, notesKey).Result()
	if err != nil {
		return nil, err
	}

	var notes []Note
	for _, id := range noteIDs {
		noteKey := fmt.Sprintf("note:%s", id)
		noteJSON, err := s.redis.Get(s.ctx, noteKey).Result()
		if err != nil {
			continue
		}

		var note Note
		json.Unmarshal([]byte(noteJSON), &note)

		// Only show user's own notes or public notes
		if note.UserID == userID || note.IsPublic {
			notes = append(notes, note)
		}
	}

	return notes, nil
}

func (s *CommentsService) GetUserNotes(userID string) ([]Note, error) {
	userNotesKey := fmt.Sprintf("user:%s:notes", userID)
	noteIDs, err := s.redis.SMembers(s.ctx, userNotesKey).Result()
	if err != nil {
		return nil, err
	}

	var notes []Note
	for _, id := range noteIDs {
		noteKey := fmt.Sprintf("note:%s", id)
		noteJSON, err := s.redis.Get(s.ctx, noteKey).Result()
		if err != nil {
			continue
		}

		var note Note
		json.Unmarshal([]byte(noteJSON), &note)
		notes = append(notes, note)
	}

	return notes, nil
}

// Watchlist
func (s *CommentsService) AddToWatchlist(userID string, req CreateWatchlistRequest) (*WatchlistItem, error) {
	item := WatchlistItem{
		ID:         uuid.New().String(),
		UserID:     userID,
		EntityType: req.EntityType,
		EntityAddr: req.EntityAddr,
		AlertType:  req.AlertType,
		Threshold:  req.Threshold,
		CreatedAt:  time.Now(),
	}

	itemKey := fmt.Sprintf("watchlist:%s", item.ID)
	itemJSON, _ := json.Marshal(item)
	s.redis.Set(s.ctx, itemKey, itemJSON, 0)

	userWatchlistKey := fmt.Sprintf("user:%s:watchlist", userID)
	s.redis.SAdd(s.ctx, userWatchlistKey, item.ID)

	return &item, nil
}

func (s *CommentsService) GetWatchlist(userID string) ([]WatchlistItem, error) {
	userWatchlistKey := fmt.Sprintf("user:%s:watchlist", userID)
	itemIDs, err := s.redis.SMembers(s.ctx, userWatchlistKey).Result()
	if err != nil {
		return nil, err
	}

	var items []WatchlistItem
	for _, id := range itemIDs {
		itemKey := fmt.Sprintf("watchlist:%s", id)
		itemJSON, err := s.redis.Get(s.ctx, itemKey).Result()
		if err != nil {
			continue
		}

		var item WatchlistItem
		json.Unmarshal([]byte(itemJSON), &item)
		items = append(items, item)
	}

	return items, nil
}

func (s *CommentsService) RemoveFromWatchlist(userID, itemID string) error {
	itemKey := fmt.Sprintf("watchlist:%s", itemID)
	itemJSON, err := s.redis.Get(s.ctx, itemKey).Result()
	if err != nil {
		return fmt.Errorf("item not found")
	}

	var item WatchlistItem
	json.Unmarshal([]byte(itemJSON), &item)

	if item.UserID != userID {
		return fmt.Errorf("unauthorized")
	}

	userWatchlistKey := fmt.Sprintf("user:%s:watchlist", userID)
	s.redis.SRem(s.ctx, userWatchlistKey, itemID)
	s.redis.Del(s.ctx, itemKey)

	return nil
}

// HTTP Handlers
func (s *CommentsService) registerRoutes(r *gin.Engine) {
	// Auth
	r.POST("/api/v1/auth/register", s.handleRegister)
	r.POST("/api/v1/auth/login", s.handleLogin)

	// Comments
	r.GET("/api/v1/comments/:entity_type/:entity_addr", s.handleGetComments)
	r.POST("/api/v1/comments", s.handleCreateComment)
	r.PUT("/api/v1/comments/:id", s.handleUpdateComment)
	r.DELETE("/api/v1/comments/:id", s.handleDeleteComment)

	// Notes
	r.GET("/api/v1/notes", s.handleGetUserNotes)
	r.GET("/api/v1/notes/:entity_type/:entity_addr", s.handleGetEntityNotes)
	r.POST("/api/v1/notes", s.handleCreateNote)

	// Watchlist
	r.GET("/api/v1/watchlist", s.handleGetWatchlist)
	r.POST("/api/v1/watchlist", s.handleAddToWatchlist)
	r.DELETE("/api/v1/watchlist/:id", s.handleRemoveFromWatchlist)

	// WebSocket
	r.GET("/ws", s.handleWebSocket)
}

func (s *CommentsService) handleRegister(c *gin.Context) {
	var req RegisterRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, APIResponse{Success: false, Error: err.Error()})
		return
	}

	user, err := s.Register(req)
	if err != nil {
		c.JSON(http.StatusBadRequest, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusCreated, APIResponse{Success: true, Data: user})
}

func (s *CommentsService) handleLogin(c *gin.Context) {
	var req LoginRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, APIResponse{Success: false, Error: err.Error()})
		return
	}

	response, err := s.Login(req)
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: response})
}

func (s *CommentsService) handleGetComments(c *gin.Context) {
	entityType := c.Param("entity_type")
	entityAddr := c.Param("entity_addr")

	comments, err := s.GetComments(entityType, entityAddr)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: comments})
}

func (s *CommentsService) handleCreateComment(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	user, err := s.getUserFromToken(token)
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "unauthorized"})
		return
	}

	var req CreateCommentRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, APIResponse{Success: false, Error: err.Error()})
		return
	}

	comment, err := s.CreateComment(user.ID, req)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusCreated, APIResponse{Success: true, Data: comment})
}

func (s *CommentsService) handleUpdateComment(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	user, err := s.getUserFromToken(token)
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "unauthorized"})
		return
	}

	commentID := c.Param("id")
	var req struct {
		Content string `json:"content"`
	}
	c.ShouldBindJSON(&req)

	comment, err := s.UpdateComment(commentID, user.ID, req.Content)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: comment})
}

func (s *CommentsService) handleDeleteComment(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	user, err := s.getUserFromToken(token)
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "unauthorized"})
		return
	}

	commentID := c.Param("id")

	if err := s.DeleteComment(commentID, user.ID); err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true})
}

func (s *CommentsService) handleGetUserNotes(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	user, err := s.getUserFromToken(token)
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "unauthorized"})
		return
	}

	notes, err := s.GetUserNotes(user.ID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: notes})
}

func (s *CommentsService) handleGetEntityNotes(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	var userID string
	user, err := s.getUserFromToken(token)
	if err != nil {
		userID = "anonymous"
	} else {
		userID = user.ID
	}

	entityType := c.Param("entity_type")
	entityAddr := c.Param("entity_addr")

	notes, err := s.GetNotes(userID, entityType, entityAddr)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: notes})
}

func (s *CommentsService) handleCreateNote(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	user, err := s.getUserFromToken(token)
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "unauthorized"})
		return
	}

	var req CreateNoteRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, APIResponse{Success: false, Error: err.Error()})
		return
	}

	note, err := s.CreateNote(user.ID, req)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusCreated, APIResponse{Success: true, Data: note})
}

func (s *CommentsService) handleGetWatchlist(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	user, err := s.getUserFromToken(token)
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "unauthorized"})
		return
	}

	watchlist, err := s.GetWatchlist(user.ID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: watchlist})
}

func (s *CommentsService) handleAddToWatchlist(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	user, err := s.getUserFromToken(token)
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "unauthorized"})
		return
	}

	var req CreateWatchlistRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, APIResponse{Success: false, Error: err.Error()})
		return
	}

	item, err := s.AddToWatchlist(user.ID, req)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusCreated, APIResponse{Success: true, Data: item})
}

func (s *CommentsService) handleRemoveFromWatchlist(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	user, err := s.getUserFromToken(token)
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "unauthorized"})
		return
	}

	itemID := c.Param("id")

	if err := s.RemoveFromWatchlist(user.ID, itemID); err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true})
}

func (s *CommentsService) handleWebSocket(c *gin.Context) {
	upgrader := websocket.Upgrader{
		CheckOrigin: func(r *http.Request) bool { return true },
	}

	conn, err := upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		return
	}

	s.wsHub.register <- conn
}

func (s *CommentsService) startHTTPServer() {
	r := gin.Default()
	s.registerRoutes(r)
	r.Run(fmt.Sprintf(":%d", s.config.Port))
}

func main() {
	config := Config{
		Port:           8090,
		RedisURL:       "localhost:6379",
		MaxCommentLen:  1000,
		MaxNotesPerUser: 100,
	}

	service, err := NewCommentsService(config)
	if err != nil {
		fmt.Printf("Failed to create service: %v\n", err)
		return
	}

	if err := service.Start(); err != nil {
		fmt.Printf("Failed to start service: %v\n", err)
		return
	}

	fmt.Println("Comments Service started on port", config.Port)
	select {}
}
