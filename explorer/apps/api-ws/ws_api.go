// Package apiws provides WebSocket API for TigerScan Explorer.
package apiws

import (
	"encoding/json"
	"fmt"
	"net/http"
	"sync"
	"time"
)

// Server provides WebSocket API.
type Server struct {
	mu sync.RWMutex
	addr string
	clients map[string]*Client
	broadcast chan *Message
}

type Client struct {
	ID     string
	Send   chan []byte
	Filter Filter
}

type Filter struct {
	Blocks       bool
	Transactions bool
	Tokens       bool
	NFTs         bool
	Addresses    []string
}

type Message struct {
	Type    string          `json:"type"`
	Payload json.RawMessage `json:"payload"`
}

func NewServer(addr string) *Server {
	return &Server{
		addr: addr,
		clients: make(map[string]*Client),
		broadcast: make(chan *Message, 1000),
	}
}

func (s *Server) Start() error {
	go s.broadcaster()
	
	mux := http.NewServeMux()
	mux.HandleFunc("/ws", s.handleWebSocket)
	return http.ListenAndServe(s.addr, mux)
}

func (s *Server) handleWebSocket(w http.ResponseWriter, r *http.Request) {
	// Upgrade to WebSocket (simplified)
	client := &Client{
		ID: generateID(),
		Send: make(chan []byte, 256),
	}

	s.mu.Lock()
	s.clients[client.ID] = client
	s.mu.Unlock()

	defer func() {
		s.mu.Lock()
		delete(s.clients, client.ID)
		s.mu.Unlock()
		close(client.Send)
	}()

	// Send initial message
	client.Send <- []byte(`{"type":"connected","client_id":"`+client.ID+`"}`)

	// Handle incoming messages
	decoder := json.NewDecoder(r.Body)
	for {
		var msg Message
		if err := decoder.Decode(&msg); err != nil {
			break
		}
		s.handleMessage(client, &msg)
	}
}

func (s *Server) handleMessage(client *Client, msg *Message) {
	switch msg.Type {
	case "subscribe":
		var sub struct {
			Blocks       bool     `json:"blocks"`
			Transactions bool     `json:"transactions"`
			Tokens       bool     `json:"tokens"`
			NFTs         bool     `json:"nfts"`
			Addresses    []string `json:"addresses"`
		}
		json.Unmarshal(msg.Payload, &sub)
		client.Filter.Blocks = sub.Blocks
		client.Filter.Transactions = sub.Transactions
		client.Filter.Tokens = sub.Tokens
		client.Filter.NFTs = sub.NFTs
		client.Filter.Addresses = sub.Addresses
		
	case "unsubscribe":
		client.Filter = Filter{}
	}
}

func (s *Server) broadcaster() {
	for msg := range s.broadcast {
		s.mu.RLock()
		for _, client := range s.clients {
			if s.shouldSend(client, msg) {
				select {
				case client.Send <- s.formatMessage(msg):
				default:
				}
			}
		}
		s.mu.RUnlock()
	}
}

func (s *Server) shouldSend(client *Client, msg *Message) bool {
	switch msg.Type {
	case "new_block":
		return client.Filter.Blocks
	case "new_transaction":
		return client.Filter.Transactions
	case "token_transfer":
		return client.Filter.Tokens
	case "nft_transfer":
		return client.Filter.NFTs
	}
	return true
}

func (s *Server) formatMessage(msg *Message) []byte {
	data, _ := json.Marshal(msg)
	return data
}

func (s *Server) BroadcastBlock(block interface{}) {
	msg := Message{
		Type: "new_block",
		Payload: mustMarshal(block),
	}
	s.broadcast <- &msg
}

func (s *Server) BroadcastTransaction(tx interface{}) {
	msg := Message{
		Type: "new_transaction",
		Payload: mustMarshal(tx),
	}
	s.broadcast <- &msg
}

func (s *Server) BroadcastTokenTransfer(transfer interface{}) {
	msg := Message{
		Type: "token_transfer",
		Payload: mustMarshal(transfer),
	}
	s.broadcast <- &msg
}

func (s *Server) BroadcastNFTTransfer(transfer interface{}) {
	msg := Message{
		Type: "nft_transfer",
		Payload: mustMarshal(transfer),
	}
	s.broadcast <- &msg
}

func (s *Server) ClientCount() int {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return len(s.clients)
}

func generateID() string {
	return fmt.Sprintf("ws_%d_%d", time.Now().UnixNano(), time.Now().Nanosecond())
}

func mustMarshal(v interface{}) json.RawMessage {
	data, _ := json.Marshal(v)
	return data
}

var _ = fmt.Sprintf("")
