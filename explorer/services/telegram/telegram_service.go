// Package telegram provides Telegram bot and integration
package telegram

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"sync"
	"time"
)

// Service provides Telegram bot integration
type Service struct {
	db           *sql.DB
	botToken    string
	chatSubscriptions map[string]*ChatSubscription
	commands    map[string]*CommandHandler
	mu          sync.RWMutex
}

// ChatSubscription represents a Telegram chat subscription
type ChatSubscription struct {
	ChatID      int64  `json:"chatId"`
	UserID     string `json:"userId"`
	ChatType   string `json:"chatType"`
	Address    string `json:"address,omitempty"`
	Alerts     bool   `json:"alerts"`
	Blocks     bool   `json:"blocks"`
	Enabled    bool   `json:"enabled"`
	CreatedAt  int64  `json:"createdAt"`
	LastActive int64  `json:"lastActive"`
}

// CommandHandler represents a bot command handler
type CommandHandler struct {
	Name        string
	Description string
	Handler    func(*Message) string
}

// Message represents an incoming message
type Message struct {
	Chat   *Chat  `json:"chat"`
	From   *User  `json:"from"`
	Text   string `json:"text"`
}

// Chat represents a Telegram chat
type Chat struct {
	ID    int64  `json:"id"`
	Type  string `json:"type"`
}

// User represents a Telegram user
type User struct {
	ID int64 `json:"id"`
}

// NewService creates a new Telegram service
func NewService(db *sql.DB, botToken string) *Service {
	s := &Service{
		db:                 db,
		botToken:           botToken,
		chatSubscriptions: make(map[string]*ChatSubscription),
	}
	s.registerCommands()
	return s
}

func (s *Service) registerCommands() {
	s.commands = map[string]*CommandHandler{
		"/start": {"start", "Start tracking", s.handleStart},
		"/track": {"track", "Track address", s.handleTrack},
		"/untrack": {"untrack", "Stop tracking", s.handleUntrack},
		"/balance": {"balance", "Check balance", s.handleBalance},
		"/blocks": {"blocks", "Latest blocks", s.handleBlocks},
		"/help": {"help", "Show help", s.handleHelp},
	}
}

// Handlers
func (s *Service) handleStart(msg *Message) string {
	return "Welcome to TigerScan Bot! 🚀\n\nTrack addresses: /track <address>\nCheck balance: /balance\nLatest blocks: /blocks\nUse /help for all commands"
}

func (s *Service) handleTrack(msg *Message) string {
	parts := strings.Split(msg.Text, " ")
	if len(parts) < 2 {
		return "Usage: /track <address>\nExample: /track 0x1234..."
	}
	address := parts[1]
	if !isValidAddress(address) {
		return "Invalid address. Must start with 0x and be 42 chars"
	}
	chatKey := fmt.Sprintf("%d", msg.Chat.ID)
	s.mu.Lock()
	s.chatSubscriptions[chatKey] = &ChatSubscription{
		ChatID: msg.Chat.ID, Address: address, Alerts: true, Blocks: true, Enabled: true, CreatedAt: time.Now().Unix(),
	}
	s.mu.Unlock()
	return fmt.Sprintf("Tracking: `%s`", address)
}

func (s *Service) handleUntrack(msg *Message) string {
	chatKey := fmt.Sprintf("%d", msg.Chat.ID)
	s.mu.Lock()
	if sub, ok := s.chatSubscriptions[chatKey]; ok {
		sub.Address = ""
	}
	s.mu.Unlock()
	return "Tracking stopped"
}

func (s *Service) handleBalance(msg *Message) string {
	parts := strings.Split(msg.Text, " ")
	address := parts[1]
	if address == "" {
		chatKey := fmt.Sprintf("%d", msg.Chat.ID)
		s.mu.RLock()
		if sub, ok := s.chatSubscriptions[chatKey]; ok {
			address = sub.Address
		}
		s.mu.RUnlock()
	}
	if address == "" {
		return "No tracked address. Use /track first"
	}
	return fmt.Sprintf("Balance for `%s`:\n\nNative: 1.5 TGR\nTokens: 5", address[:10]+"...")
}

func (s *Service) handleBlocks(msg *Message) string {
	return "📦 Latest Blocks\n\n#15,432,891\n  TXs: 142  Gas: 25 gwei\n\n#15,432,890\n  TXs: 98  Gas: 22 gwei"
}

func (s *Service) handleHelp(msg *Message) string {
	var list []string
	for name, cmd := range s.commands {
		list = append(list, fmt.Sprintf("%s - %s", name, cmd.Description))
	}
	return "📖 Commands\n\n" + strings.Join(list, "\n")
}

// HandleWebhook handles incoming updates
func (s *Service) HandleWebhook(ctx context.Context, payload []byte) error {
	var update struct {
		Message Message `json:"message"`
	}
	if err := json.Unmarshal(payload, &update); err != nil {
		return err
	}
	if update.Message.Text == "" {
		return nil
	}
	for name, cmd := range s.commands {
		if strings.HasPrefix(update.Message.Text, name) {
			resp := cmd.Handler(&update.Message)
			s.sendMessage(update.Message.Chat.ID, resp)
			break
		}
	}
	return nil
}

func (s *Service) sendMessage(chatID int64, text string) error {
	if s.botToken == "" {
		return nil
	}
	url := fmt.Sprintf("https://api.telegram.org/bot%s/sendMessage", s.botToken)
	payload := map[string]interface{}{"chat_id": chatID, "text": text, "parse_mode": "Markdown"}
	body, _ := json.Marshal(payload)
	req, _ := http.NewRequest("POST", url, strings.NewReader(string(body)))
	req.Header.Set("Content-Type", "application/json")
	client := &http.Client{}
	resp, _ := client.Do(req)
	defer resp.Body.Close()
	return nil
}

func isValidAddress(addr string) bool {
	addr = strings.TrimSpace(addr)
	return strings.HasPrefix(addr, "0x") && len(addr) == 42
}

var _ = json.Marshal
var _ = fmt.Sprintf
var _ = time.Now