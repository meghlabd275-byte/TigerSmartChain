// Package notifications provides notifications for TigerScan Explorer.
package notifications

import (
	"encoding/json"
	"fmt"
	"net/http"
	"sync"
)

// Server provides notifications API.
type Server struct {
	mu sync.RWMutex
	db Database
	subscribers map[string]map[string]chan []byte
}

type Database interface {
	GetNotifications(userID string, limit int) ([]Notification, error)
	MarkAsRead(userID, notifID string) error
}

type Notification struct {
	ID        string `json:"id"`
	UserID   string `json:"user_id"`
	Type     string `json:"type"`
	Title    string `json:"title"`
	Message  string `json:"message"`
	Data     map[string]string `json:"data"`
	Read     bool   `json:"read"`
	CreatedAt int64  `json:"created_at"`
}

func NewServer() *Server {
	return &Server{
		subscribers: make(map[string]map[string]chan []byte),
	}
}

func (s *Server) SetDB(db Database) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.db = db
}

func (s *Server) HandleGetNotifications(w http.ResponseWriter, r *http.Request) {
	userID := r.URL.Query()..Get("user_id")
	if userID == "" {
		http.Error(w, "missing user_id", http.StatusBadRequest)
		return
	}

	s.mu.RLock()
	defer s.mu.RUnlock()

	notifs, _ := s.db.GetNotifications(userID, 50)
	json.NewEncoder(w).Encode(notifs)
}

func (s *Server) HandleMarkRead(w http.ResponseWriter, r *http.Request) {
	var req struct {
		UserID string `json:"user_id"`
		NotifID string `json:"notif_id"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	err := s.db.MarkAsRead(req.UserID, req.NotifID)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
}

func (s *Server) Subscribe(userID, channelID string) chan []byte {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.subscribers[userID] == nil {
		s.subscribers[userID] = make(map[string]chan []byte)
	}

	ch := make(chan []byte, 10)
	s.subscribers[userID][channelID] = ch
	return ch
}

func (s *Server) Unsubscribe(userID, channelID string) {
	s.mu.Lock()
	defer s.mu.Unlock()

	if ch, ok := s.subscribers[userID][channelID]; ok {
		close(ch)
		delete(s.subscribers[userID], channelID)
	}
}

func (s *Server) Notify(userID, channelID string, message []byte) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	if ch, ok := s.subscribers[userID][channelID]; ok {
		select {
		case ch <- message:
		default:
		}
	}
}

func (s *Server) HandleSubscribe(w http.ResponseWriter, r *http.Request) {
	var req struct {
		UserID string `json:"user_id"`
		ChannelID string `json:"channel_id"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	ch := s.Subscribe(req.UserID, req.ChannelID)
	defer s.Unsubscribe(req.UserID, req.ChannelID)

	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")

	select {
	case msg := <-ch:
		fmt.Fprintf(w, "data: %s\n\n", msg)
	case <-r.Context().Done():
	}
}
