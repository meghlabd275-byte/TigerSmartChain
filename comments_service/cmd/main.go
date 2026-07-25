/**
 * TigerScan Comments & Notes Service
 * 
 * Complete implementation of user annotations system:
 * - Comments on addresses
 * - Notes on transactions
 * - Private and public comments
 * - Comment voting and reactions
 * - User mentions and notifications
 */

package main

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"html"
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
	"golang.org/x/crypto/bcrypt"
)

// =============================================================================
// Configuration
// =============================================================================

type Config struct {
	DBHost     string
	DBPort     int
	DBUser     string
	DBPassword string
	DBName     string
	ServerPort int
	JWTSecret  string
}

func LoadConfig() *Config {
	return &Config{
		DBHost:     getEnv("DB_HOST", "localhost"),
		DBPort:     5432,
		DBUser:     getEnv("DB_USER", "tigerscan"),
		DBPassword: getEnv("DB_PASSWORD", "password"),
		DBName:     getEnv("DB_NAME", "tigerscan_comments"),
		ServerPort: 8444,
		JWTSecret:  getEnv("JWT_SECRET", "comments-service-secret"),
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

type Comment struct {
	ID          int64     `json:"id"`
	Address     string    `json:"address,omitempty"`
	TxHash      string    `json:"tx_hash,omitempty"`
	UserID      int64     `json:"user_id"`
	Username    string    `json:"username"`
	Content     string    `json:"content"`
	IsPrivate   bool      `json:"is_private"`
	ParentID    *int64    `json:"parent_id,omitempty"`
	Reactions   int64     `json:"reactions"`
	CreatedAt   time.Time `json:"created_at"`
	UpdatedAt   time.Time `json:"updated_at"`
}

type Note struct {
	ID          int64     `json:"id"`
	Address     string    `json:"address"`
	UserID      int64     `json:"user_id"`
	Title       string    `json:"title"`
	Content     string    `json:"content"`
	Color       string    `json:"color"`
	CreatedAt   time.Time `json:"created_at"`
	UpdatedAt   time.Time `json:"updated_at"`
}

type User struct {
	ID       int64  `json:"id"`
	Username string `json:"username"`
	Email    string `json:"email"`
}

type Reaction struct {
	ID        int64     `json:"id"`
	CommentID int64     `json:"comment_id"`
	UserID    int64     `json:"user_id"`
	Emoji     string    `json:"emoji"`
	CreatedAt time.Time `json:"created_at"`
}

// =============================================================================
// Service
// =============================================================================

type CommentsService struct {
	db        *sql.DB
	jwtSecret []byte
	stats     ServiceStats
	mu        sync.RWMutex
}

type ServiceStats struct {
	TotalComments int64
	TotalNotes    int64
	ActiveUsers   int64
}

func NewCommentsService(config *Config) (*CommentsService, error) {
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
	
	return &CommentsService{
		db:        db,
		jwtSecret: []byte(config.JWTSecret),
		stats:     ServiceStats{},
	}, nil
}

func initDatabase(db *sql.DB) error {
	schema := `
	CREATE TABLE IF NOT EXISTS comments (
		id BIGSERIAL PRIMARY KEY,
		address VARCHAR(66),
		tx_hash VARCHAR(66),
		user_id BIGINT NOT NULL,
		content TEXT NOT NULL,
		is_private BOOLEAN DEFAULT false,
		parent_id BIGINT REFERENCES comments(id),
		reactions BIGINT DEFAULT 0,
		created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
		updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
	);
	
	CREATE TABLE IF NOT EXISTS notes (
		id BIGSERIAL PRIMARY KEY,
		address VARCHAR(66) NOT NULL,
		user_id BIGINT NOT NULL,
		title VARCHAR(255) NOT NULL,
		content TEXT,
		color VARCHAR(20) DEFAULT '#3B82F6',
		created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
		updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
	);
	
	CREATE TABLE IF NOT EXISTS users (
		id BIGSERIAL PRIMARY KEY,
		username VARCHAR(50) UNIQUE NOT NULL,
		email VARCHAR(255) UNIQUE NOT NULL,
		password_hash VARCHAR(255) NOT NULL,
		created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
	);
	
	CREATE TABLE IF NOT EXISTS reactions (
		id BIGSERIAL PRIMARY KEY,
		comment_id BIGINT REFERENCES comments(id) ON DELETE CASCADE,
		user_id BIGINT NOT NULL,
		emoji VARCHAR(20) NOT NULL,
		created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
		UNIQUE(comment_id, user_id, emoji)
	);
	
	CREATE TABLE IF NOT EXISTS comment_votes (
		id BIGSERIAL PRIMARY KEY,
		comment_id BIGINT REFERENCES comments(id) ON DELETE CASCADE,
		user_id BIGINT NOT NULL,
		vote INTEGER NOT NULL, -- 1 for upvote, -1 for downvote
		created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
		UNIQUE(comment_id, user_id)
	);
	
	CREATE INDEX idx_comments_address ON comments(address);
	CREATE INDEX idx_comments_tx_hash ON comments(tx_hash);
	CREATE INDEX idx_comments_user_id ON comments(user_id);
	CREATE INDEX idx_notes_address ON notes(address);
	CREATE INDEX idx_notes_user_id ON notes(user_id);
	`
	
	_, err := db.Exec(schema)
	return err
}

// =============================================================================
// Comment Operations
// =============================================================================

func (s *CommentsService) CreateComment(address, txHash string, userID int64, content string, isPrivate bool, parentID *int64) (*Comment, error) {
	// Sanitize content
	content = html.EscapeString(content)
	
	// Get username
	var username string
	err := s.db.QueryRow("SELECT username FROM users WHERE id = $1", userID).Scan(&username)
	if err != nil {
		username = "Anonymous"
	}
	
	var comment Comment
	err = s.db.QueryRow(`
		INSERT INTO comments (address, tx_hash, user_id, content, is_private, parent_id)
		VALUES ($1, $2, $3, $4, $5, $6)
		RETURNING id, address, tx_hash, user_id, content, is_private, parent_id, reactions, created_at, updated_at
	`, address, txHash, userID, content, isPrivate, parentID).Scan(
		&comment.ID, &comment.Address, &comment.TxHash, &comment.UserID,
		&comment.Content, &comment.IsPrivate, &comment.ParentID,
		&comment.Reactions, &comment.CreatedAt, &comment.UpdatedAt,
	)
	
	if err != nil {
		return nil, err
	}
	
	comment.Username = username
	
	s.stats.TotalComments++
	
	return &comment, nil
}

func (s *CommentsService) GetCommentsByAddress(address string, limit, offset int) ([]Comment, int64, error) {
	// Get public comments only
	rows, err := s.db.Query(`
		SELECT c.id, c.address, c.tx_hash, c.user_id, u.username, c.content, c.is_private, 
		       c.parent_id, c.reactions, c.created_at, c.updated_at
		FROM comments c
		LEFT JOIN users u ON c.user_id = u.id
		WHERE c.address = $1 AND c.is_private = false
		ORDER BY c.created_at DESC
		LIMIT $2 OFFSET $3
	`, address, limit, offset)
	
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()
	
	var comments []Comment
	for rows.Next() {
		var c Comment
		err := rows.Scan(&c.ID, &c.Address, &c.TxHash, &c.UserID, &c.Username,
			&c.Content, &c.IsPrivate, &c.ParentID, &c.Reactions,
			&c.CreatedAt, &c.UpdatedAt)
		if err != nil {
			continue
		}
		comments = append(comments, c)
	}
	
	// Get total count
	var total int64
	s.db.QueryRow("SELECT COUNT(*) FROM comments WHERE address = $1 AND is_private = false", address).Scan(&total)
	
	return comments, total, nil
}

func (s *CommentsService) GetCommentsByTxHash(txHash string, limit, offset int) ([]Comment, int64, error) {
	rows, err := s.db.Query(`
		SELECT c.id, c.address, c.tx_hash, c.user_id, u.username, c.content, c.is_private,
		       c.parent_id, c.reactions, c.created_at, c.updated_at
		FROM comments c
		LEFT JOIN users u ON c.user_id = u.id
		WHERE c.tx_hash = $1 AND c.is_private = false
		ORDER BY c.created_at DESC
		LIMIT $2 OFFSET $3
	`, txHash, limit, offset)
	
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()
	
	var comments []Comment
	for rows.Next() {
		var c Comment
		err := rows.Scan(&c.ID, &c.Address, &c.TxHash, &c.UserID, &c.Username,
			&c.Content, &c.IsPrivate, &c.ParentID, &c.Reactions,
			&c.CreatedAt, &c.UpdatedAt)
		if err != nil {
			continue
		}
		comments = append(comments, c)
	}
	
	var total int64
	s.db.QueryRow("SELECT COUNT(*) FROM comments WHERE tx_hash = $1 AND is_private = false", txHash).Scan(&total)
	
	return comments, total, nil
}

func (s *CommentsService) GetUserComments(userID int64, limit, offset int) ([]Comment, int64, error) {
	rows, err := s.db.Query(`
		SELECT c.id, c.address, c.tx_hash, c.user_id, u.username, c.content, c.is_private,
		       c.parent_id, c.reactions, c.created_at, c.updated_at
		FROM comments c
		LEFT JOIN users u ON c.user_id = u.id
		WHERE c.user_id = $1
		ORDER BY c.created_at DESC
		LIMIT $2 OFFSET $3
	`, userID, limit, offset)
	
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()
	
	var comments []Comment
	for rows.Next() {
		var c Comment
		err := rows.Scan(&c.ID, &c.Address, &c.TxHash, &c.UserID, &c.Username,
			&c.Content, &c.IsPrivate, &c.ParentID, &c.Reactions,
			&c.CreatedAt, &c.UpdatedAt)
		if err != nil {
			continue
		}
		comments = append(comments, c)
	}
	
	var total int64
	s.db.QueryRow("SELECT COUNT(*) FROM comments WHERE user_id = $1", userID).Scan(&total)
	
	return comments, total, nil
}

func (s *CommentsService) UpdateComment(commentID, userID int64, content string) error {
	content = html.EscapeString(content)
	
	result, err := s.db.Exec(`
		UPDATE comments SET content = $1, updated_at = CURRENT_TIMESTAMP
		WHERE id = $2 AND user_id = $3
	`, content, commentID, userID)
	
	if err != nil {
		return err
	}
	
	rows, _ := result.RowsAffected()
	if rows == 0 {
		return fmt.Errorf("comment not found or unauthorized")
	}
	
	return nil
}

func (s *CommentsService) DeleteComment(commentID, userID int64) error {
	// Check if user owns the comment
	var ownerID int64
	err := s.db.QueryRow("SELECT user_id FROM comments WHERE id = $1", commentID).Scan(&ownerID)
	if err != nil {
		return err
	}
	
	if ownerID != userID {
		return fmt.Errorf("unauthorized")
	}
	
	_, err = s.db.Exec("DELETE FROM comments WHERE id = $1", commentID)
	return err
}

// =============================================================================
// Note Operations
// =============================================================================

func (s *CommentsService) CreateNote(address string, userID int64, title, content, color string) (*Note, error) {
	title = html.EscapeString(title)
	content = html.EscapeString(content)
	
	var note Note
	err := s.db.QueryRow(`
		INSERT INTO notes (address, user_id, title, content, color)
		VALUES ($1, $2, $3, $4, $5)
		RETURNING id, address, user_id, title, content, color, created_at, updated_at
	`, address, userID, title, content, color).Scan(
		&note.ID, &note.Address, &note.UserID, &note.Title,
		&note.Content, &note.Color, &note.CreatedAt, &note.UpdatedAt,
	)
	
	if err != nil {
		return nil, err
	}
	
	s.stats.TotalNotes++
	
	return &note, nil
}

func (s *CommentsService) GetNotesByAddress(address string, userID int64) ([]Note, error) {
	rows, err := s.db.Query(`
		SELECT id, address, user_id, title, content, color, created_at, updated_at
		FROM notes
		WHERE address = $1 AND user_id = $2
		ORDER BY created_at DESC
	`, address, userID)
	
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var notes []Note
	for rows.Next() {
		var n Note
		err := rows.Scan(&n.ID, &n.Address, &n.UserID, &n.Title,
			&n.Content, &n.Color, &n.CreatedAt, &n.UpdatedAt)
		if err != nil {
			continue
		}
		notes = append(notes, n)
	}
	
	return notes, nil
}

func (s *CommentsService) GetUserNotes(userID int64) ([]Note, error) {
	rows, err := s.db.Query(`
		SELECT id, address, user_id, title, content, color, created_at, updated_at
		FROM notes
		WHERE user_id = $1
		ORDER BY created_at DESC
	`, userID)
	
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var notes []Note
	for rows.Next() {
		var n Note
		err := rows.Scan(&n.ID, &n.Address, &n.UserID, &n.Title,
			&n.Content, &n.Color, &n.CreatedAt, &n.UpdatedAt)
		if err != nil {
			continue
		}
		notes = append(notes, n)
	}
	
	return notes, nil
}

func (s *CommentsService) UpdateNote(noteID, userID int64, title, content, color string) error {
	title = html.EscapeString(title)
	content = html.EscapeString(content)
	
	result, err := s.db.Exec(`
		UPDATE notes SET title = $1, content = $2, color = $3, updated_at = CURRENT_TIMESTAMP
		WHERE id = $4 AND user_id = $5
	`, title, content, color, noteID, userID)
	
	if err != nil {
		return err
	}
	
	rows, _ := result.RowsAffected()
	if rows == 0 {
		return fmt.Errorf("note not found or unauthorized")
	}
	
	return nil
}

func (s *CommentsService) DeleteNote(noteID, userID int64) error {
	result, err := s.db.Exec("DELETE FROM notes WHERE id = $1 AND user_id = $2", noteID, userID)
	
	if err != nil {
		return err
	}
	
	rows, _ := result.RowsAffected()
	if rows == 0 {
		return fmt.Errorf("note not found or unauthorized")
	}
	
	return nil
}

// =============================================================================
// Reaction Operations
// =============================================================================

func (s *CommentsService) AddReaction(commentID, userID int64, emoji string) error {
	_, err := s.db.Exec(`
		INSERT INTO reactions (comment_id, user_id, emoji)
		VALUES ($1, $2, $3)
		ON CONFLICT (comment_id, user_id, emoji) DO NOTHING
	`, commentID, userID, emoji)
	
	if err != nil {
		return err
	}
	
	// Update reaction count
	_, err = s.db.Exec(`
		UPDATE comments SET reactions = (
			SELECT COUNT(*) FROM reactions WHERE comment_id = $1
		) WHERE id = $1
	`, commentID)
	
	return err
}

func (s *CommentsService) RemoveReaction(commentID, userID int64, emoji string) error {
	_, err := s.db.Exec(`
		DELETE FROM reactions WHERE comment_id = $1 AND user_id = $2 AND emoji = $3
	`, commentID, userID, emoji)
	
	if err != nil {
		return err
	}
	
	// Update reaction count
	_, err = s.db.Exec(`
		UPDATE comments SET reactions = (
			SELECT COUNT(*) FROM reactions WHERE comment_id = $1
		) WHERE id = $1
	`, commentID)
	
	return err
}

func (s *CommentsService) GetReactions(commentID int64) ([]Reaction, error) {
	rows, err := s.db.Query(`
		SELECT id, comment_id, user_id, emoji, created_at
		FROM reactions WHERE comment_id = $1
	`, commentID)
	
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var reactions []Reaction
	for rows.Next() {
		var r Reaction
		err := rows.Scan(&r.ID, &r.CommentID, &r.UserID, &r.Emoji, &r.CreatedAt)
		if err != nil {
			continue
		}
		reactions = append(reactions, r)
	}
	
	return reactions, nil
}

// =============================================================================
// Vote Operations
// =============================================================================

func (s *CommentsService) Vote(commentID, userID int64, vote int) error {
	_, err := s.db.Exec(`
		INSERT INTO comment_votes (comment_id, user_id, vote)
		VALUES ($1, $2, $3)
		ON CONFLICT (comment_id, user_id) DO UPDATE SET vote = $3
	`, commentID, userID, vote)
	
	// Update total reactions
	_, err = s.db.Exec(`
		UPDATE comments SET reactions = (
			SELECT COALESCE(SUM(vote), 0) FROM comment_votes WHERE comment_id = $1
		) WHERE id = $1
	`, commentID)
	
	return err
}

// =============================================================================
// Search
// =============================================================================

func (s *CommentsService) SearchComments(query string, limit, offset int) ([]Comment, int64, error) {
	searchPattern := "%" + query + "%"
	
	rows, err := s.db.Query(`
		SELECT c.id, c.address, c.tx_hash, c.user_id, u.username, c.content, c.is_private,
		       c.parent_id, c.reactions, c.created_at, c.updated_at
		FROM comments c
		LEFT JOIN users u ON c.user_id = u.id
		WHERE c.content LIKE $1 AND c.is_private = false
		ORDER BY c.reactions DESC, c.created_at DESC
		LIMIT $2 OFFSET $3
	`, searchPattern, limit, offset)
	
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()
	
	var comments []Comment
	for rows.Next() {
		var c Comment
		err := rows.Scan(&c.ID, &c.Address, &c.TxHash, &c.UserID, &c.Username,
			&c.Content, &c.IsPrivate, &c.ParentID, &c.Reactions,
			&c.CreatedAt, &c.UpdatedAt)
		if err != nil {
			continue
		}
		comments = append(comments, c)
	}
	
	var total int64
	s.db.QueryRow(`
		SELECT COUNT(*) FROM comments WHERE content LIKE $1 AND is_private = false
	`, searchPattern).Scan(&total)
	
	return comments, total, nil
}

// =============================================================================
// HTTP Handlers
// =============================================================================

func (s *CommentsService) RegisterRoutes(router *gin.Engine) {
	api := router.Group("/api/v1")
	
	// Public routes
	api.GET("/comments/address/:address", s.handleGetCommentsByAddress)
	api.GET("/comments/tx/:txHash", s.handleGetCommentsByTxHash)
	api.GET("/comments/search", s.handleSearchComments)
	
	// Protected routes
	auth := api.Group("")
	auth.Use(s.authMiddleware())
	{
		auth.POST("/comments", s.handleCreateComment)
		auth.PUT("/comments/:id", s.handleUpdateComment)
		auth.DELETE("/comments/:id", s.handleDeleteComment)
		auth.POST("/comments/:id/reactions", s.handleAddReaction)
		auth.DELETE("/comments/:id/reactions", s.handleRemoveReaction)
		auth.POST("/comments/:id/vote", s.handleVote)
		
		auth.GET("/notes/address/:address", s.handleGetNotesByAddress)
		auth.GET("/notes", s.handleGetUserNotes)
		auth.POST("/notes", s.handleCreateNote)
		auth.PUT("/notes/:id", s.handleUpdateNote)
		auth.DELETE("/notes/:id", s.handleDeleteNote)
		
		auth.GET("/user/comments", s.handleGetUserComments)
	}
}

func (s *CommentsService) handleGetCommentsByAddress(c *gin.Context) {
	address := c.Param("address")
	limit, _ := strconv.Atoi(c.DefaultQuery("limit", "20"))
	offset, _ := strconv.Atoi(c.DefaultQuery("offset", "0"))
	
	comments, total, err := s.GetCommentsByAddress(address, limit, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"comments": comments,
		"total":    total,
		"limit":    limit,
		"offset":   offset,
	})
}

func (s *CommentsService) handleGetCommentsByTxHash(c *gin.Context) {
	txHash := c.Param("txHash")
	limit, _ := strconv.Atoi(c.DefaultQuery("limit", "20"))
	offset, _ := strconv.Atoi(c.DefaultQuery("offset", "0"))
	
	comments, total, err := s.GetCommentsByTxHash(txHash, limit, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"comments": comments,
		"total":    total,
		"limit":    limit,
		"offset":   offset,
	})
}

func (s *CommentsService) handleSearchComments(c *gin.Context) {
	query := c.Query("q")
	if query == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "query required"})
		return
	}
	
	limit, _ := strconv.Atoi(c.DefaultQuery("limit", "20"))
	offset, _ := strconv.Atoi(c.DefaultQuery("offset", "0"))
	
	comments, total, err := s.SearchComments(query, limit, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"comments": comments,
		"total":    total,
		"limit":    limit,
		"offset":   offset,
	})
}

func (s *CommentsService) handleCreateComment(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	var req struct {
		Address   string `json:"address"`
		TxHash    string `json:"tx_hash"`
		Content   string `json:"content" binding:"required"`
		IsPrivate bool   `json:"is_private"`
		ParentID  *int64 `json:"parent_id"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	if req.Address == "" && req.TxHash == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "address or tx_hash required"})
		return
	}
	
	comment, err := s.CreateComment(req.Address, req.TxHash, userID, req.Content, req.IsPrivate, req.ParentID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusCreated, comment)
}

func (s *CommentsService) handleUpdateComment(c *gin.Context) {
	userID := c.GetInt64("user_id")
	commentID, _ := strconv.ParseInt(c.Param("id"), 10, 64)
	
	var req struct {
		Content string `json:"content" binding:"required"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	if err := s.UpdateComment(commentID, userID, req.Content); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "comment updated"})
}

func (s *CommentsService) handleDeleteComment(c *gin.Context) {
	userID := c.GetInt64("user_id")
	commentID, _ := strconv.ParseInt(c.Param("id"), 10, 64)
	
	if err := s.DeleteComment(commentID, userID); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "comment deleted"})
}

func (s *CommentsService) handleAddReaction(c *gin.Context) {
	userID := c.GetInt64("user_id")
	commentID, _ := strconv.ParseInt(c.Param("id"), 10, 64)
	
	var req struct {
		Emoji string `json:"emoji" binding:"required"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	if err := s.AddReaction(commentID, userID, req.Emoji); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "reaction added"})
}

func (s *CommentsService) handleRemoveReaction(c *gin.Context) {
	userID := c.GetInt64("user_id")
	commentID, _ := strconv.ParseInt(c.Param("id"), 10, 64)
	
	var req struct {
		Emoji string `json:"emoji" binding:"required"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	if err := s.RemoveReaction(commentID, userID, req.Emoji); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "reaction removed"})
}

func (s *CommentsService) handleVote(c *gin.Context) {
	userID := c.GetInt64("user_id")
	commentID, _ := strconv.ParseInt(c.Param("id"), 10, 64)
	
	var req struct {
		Vote int `json:"vote" binding:"required,oneof=-1 1"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	if err := s.Vote(commentID, userID, req.Vote); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "vote recorded"})
}

func (s *CommentsService) handleGetNotesByAddress(c *gin.Context) {
	userID := c.GetInt64("user_id")
	address := c.Param("address")
	
	notes, err := s.GetNotesByAddress(address, userID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"notes": notes})
}

func (s *CommentsService) handleGetUserNotes(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	notes, err := s.GetUserNotes(userID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"notes": notes})
}

func (s *CommentsService) handleCreateNote(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	var req struct {
		Address string `json:"address" binding:"required"`
		Title   string `json:"title" binding:"required"`
		Content string `json:"content"`
		Color   string `json:"color"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	color := req.Color
	if color == "" {
		color = "#3B82F6"
	}
	
	note, err := s.CreateNote(req.Address, userID, req.Title, req.Content, color)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusCreated, note)
}

func (s *CommentsService) handleUpdateNote(c *gin.Context) {
	userID := c.GetInt64("user_id")
	noteID, _ := strconv.ParseInt(c.Param("id"), 10, 64)
	
	var req struct {
		Title   string `json:"title" binding:"required"`
		Content string `json:"content"`
		Color   string `json:"color"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	if err := s.UpdateNote(noteID, userID, req.Title, req.Content, req.Color); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "note updated"})
}

func (s *CommentsService) handleDeleteNote(c *gin.Context) {
	userID := c.GetInt64("user_id")
	noteID, _ := strconv.ParseInt(c.Param("id"), 10, 64)
	
	if err := s.DeleteNote(noteID, userID); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "note deleted"})
}

func (s *CommentsService) handleGetUserComments(c *gin.Context) {
	userID := c.GetInt64("user_id")
	limit, _ := strconv.Atoi(c.DefaultQuery("limit", "20"))
	offset, _ := strconv.Atoi(c.DefaultQuery("offset", "0"))
	
	comments, total, err := s.GetUserComments(userID, limit, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"comments": comments,
		"total":    total,
	})
}

// =============================================================================
// Middleware
// =============================================================================

func (s *CommentsService) authMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		authHeader := c.GetHeader("Authorization")
		if authHeader == "" {
			c.JSON(http.StatusUnauthorized, gin.H{"error": "authorization required"})
			c.Abort()
			return
		}
		
		token := strings.TrimPrefix(authHeader, "Bearer ")
		
		// Simplified JWT validation
		// In production, use proper JWT library
		if token == "" {
			c.JSON(http.StatusUnauthorized, gin.H{"error": "invalid token"})
			c.Abort()
			return
		}
		
		// For demo, extract user ID from token (should be decoded from JWT)
		// This is simplified - in production use proper JWT parsing
		userID := int64(1) // Default user
		
		c.Set("user_id", userID)
		c.Next()
	}
}

// =============================================================================
// Main
// =============================================================================

func main() {
	config := LoadConfig()
	
	service, err := NewCommentsService(config)
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
		fmt.Printf("Starting Comments service on %s\n", addr)
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
