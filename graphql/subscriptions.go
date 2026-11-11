package graphql

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/gorilla/websocket"
	"github.com/graphql-go/graphql"
	"github.com/graphql-go/graphql/language/ast"
	"github.com/graphql-go/graphql/language/parser"
	"github.com/graphql-go/graphql/language/source"
	"github.com/jackc/pgx/v5/pgxpool"
)

const (
	wsWriteTimeout = 10 * time.Second
	wsPongTimeout  = 60 * time.Second
	wsPingInterval = 25 * time.Second
	wsMaxMessage   = 1 << 20
)

// subscriptionHub polls the indexer for new blocks and transactions and fans
// them out to every active subscriber.
type subscriptionHub struct {
	mu            sync.Mutex
	blockSubs     map[chan *Block]struct{}
	txSubs        map[chan *Transaction]struct{}
	lastBlock     int64
	lastTxAt      time.Time
	stopOnce      sync.Once
	stopped       chan struct{}
	blockInterval time.Duration
	txInterval    time.Duration
}

func newSubscriptionHub() *subscriptionHub {
	return &subscriptionHub{
		blockSubs:     make(map[chan *Block]struct{}),
		txSubs:        make(map[chan *Transaction]struct{}),
		stopped:       make(chan struct{}),
		blockInterval: 2 * time.Second,
		txInterval:    2 * time.Second,
	}
}

func (h *subscriptionHub) stop() {
	h.stopOnce.Do(func() { close(h.stopped) })
}

// run polls the indexer until the hub is stopped.
func (h *subscriptionHub) run(ctx context.Context, pool *pgxpool.Pool) {
	blockTicker := time.NewTicker(h.blockInterval)
	txTicker := time.NewTicker(h.txInterval)
	defer blockTicker.Stop()
	defer txTicker.Stop()

	// Establish the starting point so subscribers only receive new rows.
	var head *int64
	if err := pool.QueryRow(ctx, "SELECT MAX(number) FROM blocks").Scan(&head); err == nil && head != nil {
		h.lastBlock = *head
	}
	h.lastTxAt = time.Now().UTC()

	for {
		select {
		case <-h.stopped:
			h.closeAll()
			return
		case <-ctx.Done():
			h.closeAll()
			return
		case <-blockTicker.C:
			h.pollBlocks(ctx, pool)
		case <-txTicker.C:
			h.pollTransactions(ctx, pool)
		}
	}
}

func (h *subscriptionHub) pollBlocks(ctx context.Context, pool *pgxpool.Pool) {
	h.mu.Lock()
	since := h.lastBlock
	hasSubs := len(h.blockSubs) > 0
	h.mu.Unlock()

	if !hasSubs {
		return
	}

	rows, err := pool.Query(ctx, `
		SELECT b.number, b.hash, b.parent_hash, b.timestamp, b.tx_count, b.uncle_count,
		       b.gas_used, b.gas_limit, b.miner, b.size, b.state_root,
		       COALESCE(array_agg(t.hash ORDER BY t.transaction_index) FILTER (WHERE t.hash IS NOT NULL), '{}')
		FROM blocks b
		LEFT JOIN transactions t ON t.block_number = b.number
		WHERE b.number > $1
		GROUP BY b.id, b.number, b.hash, b.parent_hash, b.timestamp, b.tx_count, b.uncle_count,
		         b.gas_used, b.gas_limit, b.miner, b.size, b.state_root
		ORDER BY b.number ASC
		LIMIT 50`, since)
	if err != nil {
		return
	}
	defer rows.Close()

	var highest int64 = since
	for rows.Next() {
		var (
			number     int64
			hash       string
			parentHash string
			timestamp  int64
			txCount    int32
			uncleCount int32
			gasUsed    int64
			gasLimit   int64
			miner      string
			size       int64
			stateRoot  *string
			txHashes   []string
		)
		if err := rows.Scan(&number, &hash, &parentHash, &timestamp, &txCount, &uncleCount,
			&gasUsed, &gasLimit, &miner, &size, &stateRoot, &txHashes); err != nil {
			return
		}

		block := &Block{
			Number:       fmt.Sprintf("%d", number),
			Hash:         hash,
			ParentHash:   parentHash,
			Timestamp:    time.Unix(timestamp, 0).UTC().Format(time.RFC3339),
			TxCount:      int(txCount),
			UncleCount:   int(uncleCount),
			GasUsed:      fmt.Sprintf("%d", gasUsed),
			GasLimit:     fmt.Sprintf("%d", gasLimit),
			Miner:        miner,
			Size:         fmt.Sprintf("%d", size),
			StateRoot:    stateRoot,
			Transactions: txHashes,
		}

		h.fanoutBlock(block)
		if number > highest {
			highest = number
		}
	}

	if highest > since {
		h.mu.Lock()
		if highest > h.lastBlock {
			h.lastBlock = highest
		}
		h.mu.Unlock()
	}
}

func (h *subscriptionHub) pollTransactions(ctx context.Context, pool *pgxpool.Pool) {
	h.mu.Lock()
	since := h.lastTxAt
	hasSubs := len(h.txSubs) > 0
	h.mu.Unlock()

	if !hasSubs {
		return
	}

	rows, err := pool.Query(ctx, `
		SELECT hash, nonce, block_number, block_hash, transaction_index, from_address, to_address,
		       value, gas_price, gas_limit, gas_used, input, status::text, block_timestamp, contract_address
		FROM transactions
		WHERE created_at > $1
		ORDER BY created_at ASC
		LIMIT 100`, since)
	if err != nil {
		return
	}
	defer rows.Close()

	for rows.Next() {
		tx, err := scanTransaction(rows)
		if err != nil {
			return
		}
		h.fanoutTransaction(tx)
	}

	// The watermark tracks wall clock time because created_at is assigned by
	// the database on insert.
	h.mu.Lock()
	h.lastTxAt = time.Now().UTC()
	h.mu.Unlock()
}

func (h *subscriptionHub) fanoutBlock(block *Block) {
	h.mu.Lock()
	subs := make([]chan *Block, 0, len(h.blockSubs))
	for ch := range h.blockSubs {
		subs = append(subs, ch)
	}
	h.mu.Unlock()

	for _, ch := range subs {
		select {
		case ch <- block:
		default:
			// A slow consumer must not block the poller.
		}
	}
}

func (h *subscriptionHub) fanoutTransaction(tx *Transaction) {
	h.mu.Lock()
	subs := make([]chan *Transaction, 0, len(h.txSubs))
	for ch := range h.txSubs {
		subs = append(subs, ch)
	}
	h.mu.Unlock()

	for _, ch := range subs {
		select {
		case ch <- tx:
		default:
		}
	}
}

func (h *subscriptionHub) closeAll() {
	h.mu.Lock()
	defer h.mu.Unlock()
	for ch := range h.blockSubs {
		close(ch)
		delete(h.blockSubs, ch)
	}
	for ch := range h.txSubs {
		close(ch)
		delete(h.txSubs, ch)
	}
}

// subscribeBlocks registers a block channel that is closed when ctx is done.
func (h *subscriptionHub) subscribeBlocks(ctx context.Context, _ int) <-chan *Block {
	ch := make(chan *Block, 32)
	h.mu.Lock()
	h.blockSubs[ch] = struct{}{}
	h.mu.Unlock()

	go func() {
		<-ctx.Done()
		h.mu.Lock()
		if _, ok := h.blockSubs[ch]; ok {
			delete(h.blockSubs, ch)
			close(ch)
		}
		h.mu.Unlock()
	}()

	return ch
}

// subscribeTransactions registers a transaction channel, optionally filtered by
// an address that must appear as sender or recipient.
func (h *subscriptionHub) subscribeTransactions(ctx context.Context, _ int, address string) <-chan *Transaction {
	raw := make(chan *Transaction, 64)
	h.mu.Lock()
	h.txSubs[raw] = struct{}{}
	h.mu.Unlock()

	filtered := make(chan *Transaction, 64)
	needle := strings.ToLower(strings.TrimSpace(address))

	go func() {
		defer close(filtered)
		for tx := range raw {
			if needle != "" {
				to := ""
				if tx.To != nil {
					to = *tx.To
				}
				if strings.ToLower(tx.From) != needle && strings.ToLower(to) != needle {
					continue
				}
			}
			select {
			case filtered <- tx:
			case <-ctx.Done():
				return
			}
		}
	}()

	go func() {
		<-ctx.Done()
		h.mu.Lock()
		if _, ok := h.txSubs[raw]; ok {
			delete(h.txSubs, raw)
			close(raw)
		}
		h.mu.Unlock()
	}()

	return filtered
}

// -----------------------------------------------------------------------------
// WebSocket transport (graphql-ws / graphql-transport-ws)
// -----------------------------------------------------------------------------

type wsMessage struct {
	ID      string          `json:"id,omitempty"`
	Type    string          `json:"type"`
	Payload json.RawMessage `json:"payload,omitempty"`
}

type wsHandler struct {
	srv *Server
}

func (wh *wsHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if r.URL.Path == "/ws" || isWebSocketUpgrade(r) {
		wh.srv.ServeWebSocket(w, r)
		return
	}
	wh.srv.serveHTTP(w, r)
}

func isWebSocketUpgrade(r *http.Request) bool {
	return strings.EqualFold(r.Header.Get("Upgrade"), "websocket")
}

// detectOperation reports whether the document holds a query, mutation or
// subscription operation.
func detectOperation(query string) (string, error) {
	doc, err := parser.Parse(parser.ParseParams{
		Source: source.NewSource(&source.Source{Body: []byte(query)}),
	})
	if err != nil {
		return "", fmt.Errorf("parse query: %w", err)
	}

	for _, def := range doc.Definitions {
		if op, ok := def.(*ast.OperationDefinition); ok {
			if op.Operation == "" {
				return "query", nil
			}
			return string(op.Operation), nil
		}
	}

	return "", errors.New("no operation found in document")
}

var upgrader = websocket.Upgrader{
	ReadBufferSize:  4096,
	WriteBufferSize: 4096,
	Subprotocols:    []string{"graphql-transport-ws", "graphql-ws"},
	CheckOrigin: func(r *http.Request) bool {
		// Origin validation happens in the CORS middleware.
		return true
	},
}

// ServeWebSocket upgrades the connection and runs the subscription protocol.
func (s *Server) ServeWebSocket(w http.ResponseWriter, r *http.Request) {
	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		return
	}
	defer conn.Close()

	conn.SetReadLimit(wsMaxMessage)
	conn.SetReadDeadline(time.Now().Add(wsPongTimeout))
	conn.SetPongHandler(func(string) error {
		return conn.SetReadDeadline(time.Now().Add(wsPongTimeout))
	})

	ctx, cancel := context.WithCancel(context.WithValue(r.Context(), serverKey{}, s))
	defer cancel()

	var writeMu sync.Mutex
	write := func(msg wsMessage) error {
		writeMu.Lock()
		defer writeMu.Unlock()
		if err := conn.SetWriteDeadline(time.Now().Add(wsWriteTimeout)); err != nil {
			return err
		}
		return conn.WriteJSON(msg)
	}

	// Ping loop keeps intermediaries from closing an idle connection.
	go func() {
		ticker := time.NewTicker(wsPingInterval)
		defer ticker.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				writeMu.Lock()
				err := conn.WriteControl(websocket.PingMessage, nil, time.Now().Add(wsWriteTimeout))
				writeMu.Unlock()
				if err != nil {
					cancel()
					return
				}
			}
		}
	}()

	active := newSubscriptionRegistry(ctx, write)

	for {
		var msg wsMessage
		if err := conn.ReadJSON(&msg); err != nil {
			return
		}
		conn.SetReadDeadline(time.Now().Add(wsPongTimeout))

		switch msg.Type {
		case "connection_init":
			if err := write(wsMessage{Type: "connection_ack"}); err != nil {
				return
			}
		case "ping":
			if err := write(wsMessage{Type: "pong"}); err != nil {
				return
			}
		case "subscribe", "start":
			var req graphQLRequest
			if err := json.Unmarshal(msg.Payload, &req); err != nil {
				write(wsMessage{ID: msg.ID, Type: "error", Payload: mustJSON(map[string]string{"message": "invalid payload"})})
				continue
			}
			if err := active.start(msg.ID, req); err != nil {
				write(wsMessage{ID: msg.ID, Type: "error", Payload: mustJSON(map[string]string{"message": err.Error()})})
			}
		case "complete", "stop":
			active.stop(msg.ID)
		case "connection_terminate":
			return
		}
	}
}

func mustJSON(value interface{}) json.RawMessage {
	raw, err := json.Marshal(value)
	if err != nil {
		return json.RawMessage(`{"message":"internal error"}`)
	}
	return raw
}

// subscriptionRegistry tracks the live subscriptions of one WebSocket client.
type subscriptionRegistry struct {
	ctx    context.Context
	write  func(wsMessage) error
	mu     sync.Mutex
	active map[string]context.CancelFunc
}

func newSubscriptionRegistry(ctx context.Context, write func(wsMessage) error) *subscriptionRegistry {
	return &subscriptionRegistry{
		ctx:    ctx,
		write:  write,
		active: make(map[string]context.CancelFunc),
	}
}

func (r *subscriptionRegistry) start(id string, req graphQLRequest) error {
	if id == "" {
		return errors.New("subscription id is required")
	}

	subCtx, cancel := context.WithCancel(r.ctx)
	r.mu.Lock()
	if existing, ok := r.active[id]; ok {
		existing()
	}
	r.active[id] = cancel
	r.mu.Unlock()

	operation, err := detectOperation(req.Query)
	if err != nil {
		cancel()
		return err
	}

	switch operation {
	case "query", "mutation":
		go r.runOnce(subCtx, id, cancel, req)
	case "subscription":
		go r.runSubscription(subCtx, id, cancel, req)
	default:
		cancel()
		return fmt.Errorf("unsupported operation %q", operation)
	}

	return nil
}

func (r *subscriptionRegistry) stop(id string) {
	r.mu.Lock()
	cancel, ok := r.active[id]
	if ok {
		delete(r.active, id)
	}
	r.mu.Unlock()
	if ok {
		cancel()
	}
}

func (r *subscriptionRegistry) runOnce(ctx context.Context, id string, cancel context.CancelFunc, req graphQLRequest) {
	defer cancel()

	result := r.execute(ctx, req)
	r.send(wsMessage{ID: id, Type: "next", Payload: mustJSON(result)})
	r.send(wsMessage{ID: id, Type: "complete"})
}

func (r *subscriptionRegistry) runSubscription(ctx context.Context, id string, cancel context.CancelFunc, req graphQLRequest) {
	defer cancel()

	srv, ok := ctx.Value(serverKey{}).(*Server)
	if !ok || srv == nil {
		r.send(wsMessage{ID: id, Type: "error", Payload: mustJSON(map[string]string{"message": "server unavailable"})})
		return
	}

	params := graphql.Params{
		Schema:         srv.schema,
		RequestString:  req.Query,
		VariableValues: req.Variables,
		OperationName:  req.OperationName,
		Context:        withDeps(ctx, &deps{pool: srv.pool, redis: srv.redis, cache: newResultCache(srv.redis, srv.cfg.CacheEnabled, srv.cfg.CacheTTL)}),
	}

	results := graphql.Subscribe(params)
	if results == nil {
		r.send(wsMessage{ID: id, Type: "error", Payload: mustJSON(map[string]string{"message": "subscription failed"})})
		return
	}

	for {
		select {
		case <-ctx.Done():
			r.send(wsMessage{ID: id, Type: "complete"})
			return
		case result, ok := <-results:
			if !ok {
				r.send(wsMessage{ID: id, Type: "complete"})
				return
			}
			if result == nil {
				continue
			}
			if len(result.Errors) > 0 {
				errs := make([]map[string]string, 0, len(result.Errors))
				for _, e := range result.Errors {
					errs = append(errs, map[string]string{"message": e.Message})
				}
				r.send(wsMessage{ID: id, Type: "error", Payload: mustJSON(errs)})
				return
			}
			r.send(wsMessage{ID: id, Type: "next", Payload: mustJSON(map[string]interface{}{"data": result.Data})})
		}
	}
}

func (r *subscriptionRegistry) send(msg wsMessage) {
	_ = r.write(msg)
}

// serverKey carries the Server instance on the WebSocket context.
type serverKey struct{}

// execute runs a single operation for a WebSocket client.
func (r *subscriptionRegistry) execute(ctx context.Context, req graphQLRequest) map[string]interface{} {
	srv, ok := ctx.Value(serverKey{}).(*Server)
	if !ok || srv == nil {
		return map[string]interface{}{"errors": []map[string]string{{"message": "server unavailable"}}}
	}

	result, err := srv.execute(ctx, req)
	if err != nil {
		return map[string]interface{}{"errors": []map[string]string{{"message": err.Error()}}}
	}

	payload := map[string]interface{}{"data": result.Data}
	if len(result.Errors) > 0 {
		errs := make([]map[string]string, 0, len(result.Errors))
		for _, e := range result.Errors {
			errs = append(errs, map[string]string{"message": e.Message})
		}
		payload["errors"] = errs
	}
	return payload
}
