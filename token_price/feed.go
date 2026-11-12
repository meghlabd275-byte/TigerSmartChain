// Package main implements the TigerScan token price feed. It aggregates quotes
// from centralised exchange APIs and on-chain AMM pools, persists snapshots to
// PostgreSQL, caches them in Redis and streams updates over WebSocket.
package main

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"net/http"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/ethereum/go-ethereum"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/gorilla/websocket"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
	"github.com/tigersmartchain/wsauth"
)

// PriceConfig configures the price feed.
type PriceConfig struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
	Port           int
	// ChainRPC is the JSON-RPC endpoint of the chain whose AMM pools are priced.
	ChainRPC string
	// Factory and WETH locate the AMM used for pool lookups.
	Factory string
	WETH    string
	// TokenSymbols maps a token address to the exchange symbol used to quote it.
	TokenSymbols map[string]string
}

// ExchangePrice is a single venue's quote for one asset.
type ExchangePrice struct {
	Exchange         string    `json:"exchange"`
	Symbol           string    `json:"symbol"`
	Price            float64   `json:"price"`
	Volume24h        float64   `json:"volume_24h"`
	High24h          float64   `json:"high_24h"`
	Low24h           float64   `json:"low_24h"`
	ChangePercent24h float64   `json:"change_percent_24h"`
	Timestamp        time.Time `json:"timestamp"`
}

// AggregatedPrice is the volume-weighted consensus price across venues.
type AggregatedPrice struct {
	Symbol           string          `json:"symbol"`
	Address          string          `json:"address,omitempty"`
	Price            float64         `json:"price"`
	PriceWei         string          `json:"price_wei"`
	ChangePercent24h float64         `json:"change_percent_24h"`
	Volume24h        float64         `json:"volume_24h"`
	High24h          float64         `json:"high_24h"`
	Low24h           float64         `json:"low_24h"`
	Confidence       float64         `json:"confidence"`
	Sources          []string        `json:"sources"`
	Prices           []ExchangePrice `json:"prices"`
	WeightedPrice    float64         `json:"weighted_price"`
	Timestamp        time.Time       `json:"timestamp"`
}

// PricePoint is one observation in a price series.
type PricePoint struct {
	Price     float64   `json:"price"`
	Volume    float64   `json:"volume"`
	Timestamp time.Time `json:"timestamp"`
}

// ExchangeClient is a price venue.
type ExchangeClient interface {
	GetPrice(ctx context.Context, symbol string) (*ExchangePrice, error)
	GetHistoricalPrices(ctx context.Context, symbol string, start, end time.Time) ([]PricePoint, error)
}

// ---------------------------------------------------------------------------
// Binance
// ---------------------------------------------------------------------------

type BinanceClient struct {
	apiURL string
	client *http.Client
}

func NewBinanceClient() *BinanceClient {
	return &BinanceClient{
		apiURL: "https://api.binance.com",
		client: &http.Client{Timeout: 10 * time.Second},
	}
}

func (c *BinanceClient) GetPrice(ctx context.Context, symbol string) (*ExchangePrice, error) {
	url := fmt.Sprintf("%s/api/v3/ticker/24hr?symbol=%s", c.apiURL, strings.ToUpper(symbol))

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	resp, err := c.client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("binance returned status %d for %s", resp.StatusCode, symbol)
	}

	var ticker struct {
		LastPrice          string `json:"lastPrice"`
		PriceChangePercent string `json:"priceChangePercent"`
		QuoteVolume        string `json:"quoteVolume"`
		HighPrice          string `json:"highPrice"`
		LowPrice           string `json:"lowPrice"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&ticker); err != nil {
		return nil, err
	}

	price, err := strconv.ParseFloat(ticker.LastPrice, 64)
	if err != nil || price <= 0 {
		return nil, fmt.Errorf("binance returned unusable price %q for %s", ticker.LastPrice, symbol)
	}
	quoteVolume, _ := strconv.ParseFloat(ticker.QuoteVolume, 64)
	high, _ := strconv.ParseFloat(ticker.HighPrice, 64)
	low, _ := strconv.ParseFloat(ticker.LowPrice, 64)
	changePercent, _ := strconv.ParseFloat(ticker.PriceChangePercent, 64)

	return &ExchangePrice{
		Exchange:         "binance",
		Symbol:           strings.ToUpper(symbol),
		Price:            price,
		Volume24h:        quoteVolume,
		High24h:          high,
		Low24h:           low,
		ChangePercent24h: changePercent,
		Timestamp:        time.Now(),
	}, nil
}

func (c *BinanceClient) GetHistoricalPrices(ctx context.Context, symbol string, start, end time.Time) ([]PricePoint, error) {
	url := fmt.Sprintf("%s/api/v3/klines?symbol=%s&interval=1h&startTime=%d&endTime=%d",
		c.apiURL, strings.ToUpper(symbol), start.UnixMilli(), end.UnixMilli())

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	resp, err := c.client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("binance klines returned status %d for %s", resp.StatusCode, symbol)
	}

	var klines [][]json.RawMessage
	if err := json.NewDecoder(resp.Body).Decode(&klines); err != nil {
		return nil, err
	}

	prices := make([]PricePoint, 0, len(klines))
	for _, k := range klines {
		if len(k) < 6 {
			continue
		}
		var openAtMillis int64
		var openPrice, volume string
		if err := json.Unmarshal(k[0], &openAtMillis); err != nil {
			continue
		}
		if err := json.Unmarshal(k[1], &openPrice); err != nil {
			continue
		}
		if err := json.Unmarshal(k[5], &volume); err != nil {
			continue
		}
		price, err := strconv.ParseFloat(openPrice, 64)
		if err != nil {
			continue
		}
		vol, _ := strconv.ParseFloat(volume, 64)

		prices = append(prices, PricePoint{
			Price:     price,
			Volume:    vol,
			Timestamp: time.UnixMilli(openAtMillis),
		})
	}

	return prices, nil
}

// ---------------------------------------------------------------------------
// Coinbase
// ---------------------------------------------------------------------------

type CoinbaseClient struct {
	apiURL string
	client *http.Client
}

func NewCoinbaseClient() *CoinbaseClient {
	return &CoinbaseClient{
		apiURL: "https://api.exchange.coinbase.com",
		client: &http.Client{Timeout: 10 * time.Second},
	}
}

func (c *CoinbaseClient) GetPrice(ctx context.Context, symbol string) (*ExchangePrice, error) {
	pair := strings.ToUpper(symbol)
	url := fmt.Sprintf("%s/products/%s/stats", c.apiURL, pair)

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	resp, err := c.client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("coinbase returned status %d for %s", resp.StatusCode, pair)
	}

	var stats struct {
		Open   string `json:"open"`
		High   string `json:"high"`
		Low    string `json:"low"`
		Last   string `json:"last"`
		Volume string `json:"volume"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&stats); err != nil {
		return nil, err
	}

	price, err := strconv.ParseFloat(stats.Last, 64)
	if err != nil || price <= 0 {
		return nil, fmt.Errorf("coinbase returned unusable price %q for %s", stats.Last, pair)
	}
	high, _ := strconv.ParseFloat(stats.High, 64)
	low, _ := strconv.ParseFloat(stats.Low, 64)
	volume, _ := strconv.ParseFloat(stats.Volume, 64)

	// The 24h change is measured against the session open price.
	var changePercent float64
	if open, err := strconv.ParseFloat(stats.Open, 64); err == nil && open > 0 {
		changePercent = (price - open) / open * 100
	}

	return &ExchangePrice{
		Exchange:         "coinbase",
		Symbol:           pair,
		Price:            price,
		Volume24h:        volume * price,
		High24h:          high,
		Low24h:           low,
		ChangePercent24h: changePercent,
		Timestamp:        time.Now(),
	}, nil
}

// GetHistoricalPrices reads Coinbase's hourly candles.
func (c *CoinbaseClient) GetHistoricalPrices(ctx context.Context, symbol string, start, end time.Time) ([]PricePoint, error) {
	url := fmt.Sprintf("%s/products/%s/candles?granularity=3600&start=%s&end=%s",
		c.apiURL, strings.ToUpper(symbol), start.UTC().Format(time.RFC3339), end.UTC().Format(time.RFC3339))

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	resp, err := c.client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("coinbase candles returned status %d for %s", resp.StatusCode, symbol)
	}

	// Coinbase candle rows are [time, low, high, open, close, volume].
	var candles [][]json.Number
	if err := json.NewDecoder(resp.Body).Decode(&candles); err != nil {
		return nil, err
	}

	prices := make([]PricePoint, 0, len(candles))
	for _, candle := range candles {
		if len(candle) < 6 {
			continue
		}
		ts, err := candle[0].Int64()
		if err != nil {
			continue
		}
		open, err := candle[3].Float64()
		if err != nil {
			continue
		}
		volume, _ := candle[5].Float64()

		prices = append(prices, PricePoint{
			Price:     open,
			Volume:    volume,
			Timestamp: time.Unix(ts, 0),
		})
	}

	return prices, nil
}

// ---------------------------------------------------------------------------
// Kraken
// ---------------------------------------------------------------------------

type KrakenClient struct {
	apiURL string
	client *http.Client
}

func NewKrakenClient() *KrakenClient {
	return &KrakenClient{
		apiURL: "https://api.kraken.com",
		client: &http.Client{Timeout: 10 * time.Second},
	}
}

func (c *KrakenClient) GetPrice(ctx context.Context, symbol string) (*ExchangePrice, error) {
	url := fmt.Sprintf("%s/0/public/Ticker?pair=%s", c.apiURL, strings.ToUpper(symbol))

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	resp, err := c.client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("kraken returned status %d for %s", resp.StatusCode, symbol)
	}

	var payload struct {
		Error  []string `json:"error"`
		Result map[string]struct {
			Last   []string `json:"c"`
			Open   string   `json:"o"`
			High   []string `json:"h"`
			Low    []string `json:"l"`
			Volume []string `json:"v"`
		} `json:"result"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&payload); err != nil {
		return nil, err
	}
	if len(payload.Error) > 0 {
		return nil, fmt.Errorf("kraken error for %s: %s", symbol, strings.Join(payload.Error, ", "))
	}

	for pair, ticker := range payload.Result {
		if len(ticker.Last) == 0 {
			continue
		}
		price, err := strconv.ParseFloat(ticker.Last[0], 64)
		if err != nil || price <= 0 {
			continue
		}

		var high, low, volume float64
		if len(ticker.High) > 1 {
			high, _ = strconv.ParseFloat(ticker.High[1], 64)
		}
		if len(ticker.Low) > 1 {
			low, _ = strconv.ParseFloat(ticker.Low[1], 64)
		}
		if len(ticker.Volume) > 1 {
			volume, _ = strconv.ParseFloat(ticker.Volume[1], 64)
		}
		var changePercent float64
		if open, err := strconv.ParseFloat(ticker.Open, 64); err == nil && open > 0 {
			changePercent = (price - open) / open * 100
		}

		return &ExchangePrice{
			Exchange:         "kraken",
			Symbol:           pair,
			Price:            price,
			Volume24h:        volume * price,
			High24h:          high,
			Low24h:           low,
			ChangePercent24h: changePercent,
			Timestamp:        time.Now(),
		}, nil
	}

	return nil, fmt.Errorf("kraken returned no ticker data for %s", symbol)
}

// GetHistoricalPrices reads Kraken's hourly OHLC series.
func (c *KrakenClient) GetHistoricalPrices(ctx context.Context, symbol string, start, end time.Time) ([]PricePoint, error) {
	url := fmt.Sprintf("%s/0/public/OHLC?pair=%s&interval=60&since=%d",
		c.apiURL, strings.ToUpper(symbol), start.Unix())

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	resp, err := c.client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("kraken OHLC returned status %d for %s", resp.StatusCode, symbol)
	}

	var payload struct {
		Error  []string                   `json:"error"`
		Result map[string]json.RawMessage `json:"result"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&payload); err != nil {
		return nil, err
	}
	if len(payload.Error) > 0 {
		return nil, fmt.Errorf("kraken OHLC error for %s: %s", symbol, strings.Join(payload.Error, ", "))
	}

	var prices []PricePoint
	for key, raw := range payload.Result {
		if key == "last" {
			continue
		}
		var rows [][]json.Number
		if err := json.Unmarshal(raw, &rows); err != nil {
			continue
		}
		for _, row := range rows {
			if len(row) < 7 {
				continue
			}
			ts, err := row[0].Int64()
			if err != nil {
				continue
			}
			at := time.Unix(ts, 0)
			if at.Before(start) || at.After(end) {
				continue
			}
			open, err := row[1].Float64()
			if err != nil {
				continue
			}
			volume, _ := row[6].Float64()
			prices = append(prices, PricePoint{Price: open, Volume: volume, Timestamp: at})
		}
		break
	}

	return prices, nil
}

// ---------------------------------------------------------------------------
// Uniswap V2-style on-chain pools
// ---------------------------------------------------------------------------

// Function selectors: the leading four bytes of keccak256 of the signature.
const (
	selectorGetPair     = "e6a43905" // getPair(address,address)
	selectorGetReserves = "0902f1ac" // getReserves()
	selectorToken0      = "0dfe1681" // token0()
	selectorToken1      = "d21220a7" // token1()
	selectorDecimals    = "313ce567" // decimals()
)

// UniswapClient prices tokens from their deepest pool against the wrapped
// native token.
type UniswapClient struct {
	client  *ethclient.Client
	factory common.Address
	weth    common.Address

	mu        sync.RWMutex
	nativeUSD float64
}

func NewUniswapClient(rpcURL, factory, weth string) (*UniswapClient, error) {
	client, err := ethclient.Dial(rpcURL)
	if err != nil {
		return nil, fmt.Errorf("dial chain rpc: %w", err)
	}
	if !common.IsHexAddress(factory) {
		return nil, fmt.Errorf("factory %q is not a valid address", factory)
	}
	if !common.IsHexAddress(weth) {
		return nil, fmt.Errorf("weth %q is not a valid address", weth)
	}
	return &UniswapClient{
		client:  client,
		factory: common.HexToAddress(factory),
		weth:    common.HexToAddress(weth),
	}, nil
}

// SetNativePriceUSD records the reference price used to convert
// wrapped-native-denominated pool prices into USD.
func (c *UniswapClient) SetNativePriceUSD(price float64) {
	c.mu.Lock()
	c.nativeUSD = price
	c.mu.Unlock()
}

func (c *UniswapClient) nativePrice() float64 {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.nativeUSD
}

// GetPrice derives the USD price of the token in the symbol argument from its
// pool against the wrapped native token.
func (c *UniswapClient) GetPrice(ctx context.Context, tokenAddress string) (*ExchangePrice, error) {
	if !common.IsHexAddress(tokenAddress) {
		return nil, fmt.Errorf("%q is not a valid token address", tokenAddress)
	}
	token := common.HexToAddress(tokenAddress)

	nativeUSD := c.nativePrice()
	if nativeUSD <= 0 {
		return nil, fmt.Errorf("reference native price is not yet available")
	}
	if token == c.weth {
		return &ExchangePrice{
			Exchange:  "amm",
			Symbol:    tokenAddress,
			Price:     nativeUSD,
			Timestamp: time.Now(),
		}, nil
	}

	pair, err := c.pairFor(ctx, token)
	if err != nil {
		return nil, err
	}

	token0, err := c.readAddress(ctx, pair, selectorToken0)
	if err != nil {
		return nil, fmt.Errorf("read token0 of pool %s: %w", pair.Hex(), err)
	}
	token1, err := c.readAddress(ctx, pair, selectorToken1)
	if err != nil {
		return nil, fmt.Errorf("read token1 of pool %s: %w", pair.Hex(), err)
	}

	reserves, err := c.readReserves(ctx, pair)
	if err != nil {
		return nil, fmt.Errorf("read reserves of pool %s: %w", pair.Hex(), err)
	}

	tokenDecimals, err := c.readDecimals(ctx, token)
	if err != nil {
		return nil, fmt.Errorf("read decimals of %s: %w", token.Hex(), err)
	}
	wethDecimals, err := c.readDecimals(ctx, c.weth)
	if err != nil {
		return nil, fmt.Errorf("read decimals of the wrapped native token: %w", err)
	}

	var reserveToken, reserveNative *big.Int
	switch {
	case token0 == token:
		reserveToken, reserveNative = reserves[0], reserves[1]
	case token1 == token:
		reserveToken, reserveNative = reserves[1], reserves[0]
	default:
		return nil, fmt.Errorf("pool %s does not hold token %s", pair.Hex(), token.Hex())
	}

	if reserveToken.Sign() == 0 || reserveNative.Sign() == 0 {
		return nil, fmt.Errorf("pool %s has an empty reserve", pair.Hex())
	}

	// price = (reserveNative / 10^nativeDecimals) / (reserveToken / 10^tokenDecimals)
	priceNative := new(big.Float).Quo(
		new(big.Float).Quo(new(big.Float).SetInt(reserveNative), new(big.Float).SetInt(pow10(wethDecimals))),
		new(big.Float).Quo(new(big.Float).SetInt(reserveToken), new(big.Float).SetInt(pow10(tokenDecimals))),
	)
	priceNativeFloat, _ := priceNative.Float64()

	return &ExchangePrice{
		Exchange:  "amm",
		Symbol:    tokenAddress,
		Price:     priceNativeFloat * nativeUSD,
		Timestamp: time.Now(),
	}, nil
}

// GetHistoricalPrices reports that AMM pools have no queryable price series;
// historical prices come from the persisted snapshots instead.
func (c *UniswapClient) GetHistoricalPrices(ctx context.Context, symbol string, start, end time.Time) ([]PricePoint, error) {
	return nil, fmt.Errorf("amm pools do not expose a historical price series")
}

// pairFor calls factory.getPair(token, wrappedNative).
func (c *UniswapClient) pairFor(ctx context.Context, token common.Address) (common.Address, error) {
	selector, err := hex.DecodeString(selectorGetPair)
	if err != nil {
		return common.Address{}, err
	}

	calldata := make([]byte, 0, 68)
	calldata = append(calldata, selector...)
	calldata = append(calldata, common.LeftPadBytes(token.Bytes(), 32)...)
	calldata = append(calldata, common.LeftPadBytes(c.weth.Bytes(), 32)...)

	out, err := c.call(ctx, c.factory, calldata)
	if err != nil {
		return common.Address{}, fmt.Errorf("factory.getPair: %w", err)
	}
	pair := common.BytesToAddress(word(out, 0))
	if pair == (common.Address{}) {
		return common.Address{}, fmt.Errorf("no pool exists for token %s", token.Hex())
	}
	return pair, nil
}

func (c *UniswapClient) readAddress(ctx context.Context, target common.Address, selector string) (common.Address, error) {
	calldata, err := hex.DecodeString(selector)
	if err != nil {
		return common.Address{}, err
	}
	out, err := c.call(ctx, target, calldata)
	if err != nil {
		return common.Address{}, err
	}
	return common.BytesToAddress(word(out, 0)), nil
}

func (c *UniswapClient) readDecimals(ctx context.Context, token common.Address) (int, error) {
	calldata, err := hex.DecodeString(selectorDecimals)
	if err != nil {
		return 0, err
	}
	out, err := c.call(ctx, token, calldata)
	if err != nil {
		return 0, err
	}
	decimals := new(big.Int).SetBytes(word(out, 0))
	if !decimals.IsInt64() || decimals.Int64() < 0 || decimals.Int64() > 36 {
		return 0, fmt.Errorf("implausible decimals value %s", decimals.String())
	}
	return int(decimals.Int64()), nil
}

func (c *UniswapClient) readReserves(ctx context.Context, pair common.Address) ([3]*big.Int, error) {
	calldata, err := hex.DecodeString(selectorGetReserves)
	if err != nil {
		return [3]*big.Int{}, err
	}
	out, err := c.call(ctx, pair, calldata)
	if err != nil {
		return [3]*big.Int{}, err
	}
	return [3]*big.Int{
		new(big.Int).SetBytes(word(out, 0)),
		new(big.Int).SetBytes(word(out, 1)),
		new(big.Int).SetBytes(word(out, 2)),
	}, nil
}

func (c *UniswapClient) call(ctx context.Context, to common.Address, data []byte) ([]byte, error) {
	return c.client.CallContract(ctx, ethereum.CallMsg{To: &to, Data: data}, nil)
}

// ---------------------------------------------------------------------------
// Price feed
// ---------------------------------------------------------------------------

// PriceFeed aggregates venue prices, persists them and serves them over HTTP
// and WebSocket.
type PriceFeed struct {
	config  PriceConfig
	clients map[string]ExchangeClient
	pool    *pgxpool.Pool
	redis   *redis.Client
	wsHub   *PriceWsHub

	mu      sync.RWMutex
	prices  map[string]*AggregatedPrice
	history map[string][]PricePoint

	ctx    context.Context
	cancel context.CancelFunc
}

func NewPriceFeed(config PriceConfig) (*PriceFeed, error) {
	ctx, cancel := context.WithCancel(context.Background())

	pool, err := pgxpool.New(ctx, config.DBURL)
	if err != nil {
		cancel()
		return nil, fmt.Errorf("connect to postgres: %w", err)
	}
	if err := ensurePriceTables(ctx, pool); err != nil {
		cancel()
		return nil, err
	}

	rdb := redis.NewClient(&redis.Options{Addr: strings.TrimPrefix(config.RedisURL, "redis://")})
	if err := rdb.Ping(ctx).Err(); err != nil {
		cancel()
		return nil, fmt.Errorf("connect to redis: %w", err)
	}

	clients := map[string]ExchangeClient{
		"binance":  NewBinanceClient(),
		"coinbase": NewCoinbaseClient(),
		"kraken":   NewKrakenClient(),
	}

	if config.ChainRPC != "" && config.Factory != "" && config.WETH != "" {
		amm, err := NewUniswapClient(config.ChainRPC, config.Factory, config.WETH)
		if err != nil {
			cancel()
			return nil, err
		}
		clients["amm"] = amm
	}

	return &PriceFeed{
		config:  config,
		clients: clients,
		pool:    pool,
		redis:   rdb,
		wsHub:   NewPriceWsHub(),
		prices:  make(map[string]*AggregatedPrice),
		history: make(map[string][]PricePoint),
		ctx:     ctx,
		cancel:  cancel,
	}, nil
}

// Close releases the feed's connections.
func (f *PriceFeed) Close() {
	f.cancel()
	f.pool.Close()
	_ = f.redis.Close()
}

func (f *PriceFeed) Start() error {
	go f.wsHub.run()
	go f.updatePrices()
	go f.persistHistory()
	return nil
}

// trackedSymbols returns the exchange symbols the feed polls.
func (f *PriceFeed) trackedSymbols() []string {
	if len(f.config.TokenSymbols) > 0 {
		symbols := make([]string, 0, len(f.config.TokenSymbols))
		for _, symbol := range f.config.TokenSymbols {
			symbols = append(symbols, symbol)
		}
		sort.Strings(symbols)
		return symbols
	}
	return []string{"ETHUSDT", "BTCUSDT", "BNBUSDT", "MATICUSDT", "AVAXUSDT", "SOLUSDT"}
}

func (f *PriceFeed) updatePrices() {
	// Prime once at start-up so the feed is not empty for a whole interval.
	f.refresh()

	ticker := time.NewTicker(f.config.UpdateInterval)
	defer ticker.Stop()

	for {
		select {
		case <-f.ctx.Done():
			return
		case <-ticker.C:
			f.refresh()
		}
	}
}

func (f *PriceFeed) refresh() {
	for _, symbol := range f.trackedSymbols() {
		if err := f.aggregateSymbol(symbol); err != nil {
			fmt.Printf("price feed: %s: %v\n", symbol, err)
		}
	}

	// On-chain pools are quoted against the freshly aggregated native price.
	amm, hasAMM := f.clients["amm"].(*UniswapClient)
	if !hasAMM {
		return
	}
	if native, ok := f.GetPrice(f.config.TokenSymbols[f.config.WETH]); ok {
		amm.SetNativePriceUSD(native.Price)
	} else if eth, ok := f.GetPrice("ETHUSDT"); ok {
		amm.SetNativePriceUSD(eth.Price)
	}

	for address, symbol := range f.config.TokenSymbols {
		quote, err := amm.GetPrice(f.ctx, address)
		if err != nil {
			continue
		}
		f.store(&AggregatedPrice{
			Symbol:        symbol,
			Address:       address,
			Price:         quote.Price,
			PriceWei:      toWeiString(quote.Price),
			Confidence:    0.70,
			Sources:       []string{quote.Exchange},
			Prices:        []ExchangePrice{*quote},
			WeightedPrice: quote.Price,
			Timestamp:     time.Now(),
		})
	}
}

// aggregateSymbol fetches every venue's quote and computes the consensus price.
func (f *PriceFeed) aggregateSymbol(symbol string) error {
	quotes := make([]ExchangePrice, 0, len(f.clients))

	for name, client := range f.clients {
		quote, err := client.GetPrice(f.ctx, symbol)
		if err != nil || quote.Price <= 0 {
			continue
		}
		quote.Exchange = name
		quotes = append(quotes, *quote)
	}

	if len(quotes) == 0 {
		return fmt.Errorf("no venue returned a price")
	}

	// Drop quotes that deviate more than 20% from the median so a single stale
	// or manipulated feed cannot skew the average.
	quotes = filterOutliers(quotes)

	weightedSum, totalWeight := 0.0, 0.0
	high, low, changePercent := 0.0, 0.0, 0.0
	sources := make([]string, 0, len(quotes))

	for _, q := range quotes {
		weight := q.Volume24h
		if weight <= 0 {
			weight = 1
		}
		weightedSum += q.Price * weight
		totalWeight += weight
		changePercent += q.ChangePercent24h

		if high == 0 || q.Price > high {
			high = q.Price
		}
		if low == 0 || q.Price < low {
			low = q.Price
		}
		sources = append(sources, q.Exchange)
	}

	price := weightedSum / totalWeight

	aggregated := &AggregatedPrice{
		Symbol:           symbol,
		Price:            price,
		PriceWei:         toWeiString(price),
		ChangePercent24h: changePercent / float64(len(quotes)),
		Volume24h:        totalWeight,
		High24h:          high,
		Low24h:           low,
		Confidence:       calculateConfidence(len(quotes)),
		Sources:          sources,
		Prices:           quotes,
		WeightedPrice:    price,
		Timestamp:        time.Now(),
	}

	if address, ok := f.addressForSymbol(symbol); ok {
		aggregated.Address = address
	}

	f.store(aggregated)
	return nil
}

func (f *PriceFeed) addressForSymbol(symbol string) (string, bool) {
	for address, s := range f.config.TokenSymbols {
		if s == symbol {
			return address, true
		}
	}
	return "", false
}

// store publishes an aggregated price to memory, Redis, Postgres and WebSocket.
func (f *PriceFeed) store(price *AggregatedPrice) {
	f.mu.Lock()
	f.prices[price.Symbol] = price
	f.mu.Unlock()

	if data, err := json.Marshal(price); err == nil {
		f.redis.Set(f.ctx, fmt.Sprintf("price:%s", price.Symbol), data, 5*time.Minute)
	}

	f.wsHub.broadcastJSON(map[string]interface{}{"type": "price_update", "price": price})

	if price.Address == "" {
		return
	}
	if _, err := f.pool.Exec(f.ctx, `
		INSERT INTO token_prices (token_address, price_usd, timestamp, source)
		VALUES ($1, $2, $3, $4)
		ON CONFLICT (token_address, timestamp) DO UPDATE SET price_usd = EXCLUDED.price_usd`,
		strings.ToLower(price.Address), price.Price, time.Now().Unix(), strings.Join(price.Sources, ","),
	); err != nil {
		fmt.Printf("price feed: persist %s: %v\n", price.Symbol, err)
	}
}

// GetPrice returns the latest aggregated price for a symbol.
func (f *PriceFeed) GetPrice(symbol string) (*AggregatedPrice, bool) {
	f.mu.RLock()
	defer f.mu.RUnlock()
	price, ok := f.prices[symbol]
	return price, ok
}

// GetAllPrices returns every aggregated price.
func (f *PriceFeed) GetAllPrices() map[string]*AggregatedPrice {
	f.mu.RLock()
	defer f.mu.RUnlock()

	out := make(map[string]*AggregatedPrice, len(f.prices))
	for symbol, price := range f.prices {
		out[symbol] = price
	}
	return out
}

// GetHistory returns the in-memory observation series for a symbol, trimmed to
// the most recent limit points.
func (f *PriceFeed) GetHistory(symbol string, limit int) []PricePoint {
	f.mu.RLock()
	defer f.mu.RUnlock()

	history := f.history[symbol]
	if limit > 0 && len(history) > limit {
		history = history[len(history)-limit:]
	}
	out := make([]PricePoint, len(history))
	copy(out, history)
	return out
}

// GetHistoricalPrices returns venue history, falling back to the persisted
// series when no venue can serve the requested window.
func (f *PriceFeed) GetHistoricalPrices(symbol string, start, end time.Time) ([]PricePoint, error) {
	for _, client := range f.clients {
		prices, err := client.GetHistoricalPrices(f.ctx, symbol, start, end)
		if err == nil && len(prices) > 0 {
			sort.Slice(prices, func(i, j int) bool { return prices[i].Timestamp.Before(prices[j].Timestamp) })
			return prices, nil
		}
	}

	address, ok := f.addressForSymbol(symbol)
	if !ok {
		return nil, fmt.Errorf("no venue served history for %s and no address is mapped to it", symbol)
	}

	rows, err := f.pool.Query(f.ctx, `
		SELECT timestamp, price_usd::float8 FROM token_prices
		WHERE token_address = $1 AND timestamp BETWEEN $2 AND $3
		ORDER BY timestamp ASC`,
		strings.ToLower(address), start.Unix(), end.Unix())
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var points []PricePoint
	for rows.Next() {
		var ts int64
		var price float64
		if err := rows.Scan(&ts, &price); err != nil {
			continue
		}
		points = append(points, PricePoint{Price: price, Timestamp: time.Unix(ts, 0)})
	}
	return points, nil
}

// persistHistory snapshots the in-memory series once a minute, keeping 24h.
func (f *PriceFeed) persistHistory() {
	ticker := time.NewTicker(time.Minute)
	defer ticker.Stop()

	for {
		select {
		case <-f.ctx.Done():
			return
		case <-ticker.C:
			now := time.Now()
			cutoff := now.Add(-24 * time.Hour)

			f.mu.Lock()
			for symbol, price := range f.prices {
				history := append(f.history[symbol], PricePoint{
					Price:     price.Price,
					Volume:    price.Volume24h,
					Timestamp: now,
				})
				for len(history) > 0 && history[0].Timestamp.Before(cutoff) {
					history = history[1:]
				}
				f.history[symbol] = history
			}
			f.mu.Unlock()
		}
	}
}

// ---------------------------------------------------------------------------
// HTTP and WebSocket
// ---------------------------------------------------------------------------

func (f *PriceFeed) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/v1/price", f.handleGetPrice)
	mux.HandleFunc("/api/v1/prices", f.handleGetAllPrices)
	mux.HandleFunc("/api/v1/history", f.handleGetHistory)
	mux.HandleFunc("/api/v1/stats", f.handleGetStats)
	mux.HandleFunc("/ws", f.handleWS)
	return mux
}

func (f *PriceFeed) handleGetPrice(w http.ResponseWriter, r *http.Request) {
	symbol := r.URL.Query().Get("symbol")
	if symbol == "" {
		http.Error(w, "missing symbol", http.StatusBadRequest)
		return
	}

	price, ok := f.GetPrice(symbol)
	if !ok {
		http.NotFound(w, r)
		return
	}
	writeJSON(w, price)
}

func (f *PriceFeed) handleGetAllPrices(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, f.GetAllPrices())
}

func (f *PriceFeed) handleGetHistory(w http.ResponseWriter, r *http.Request) {
	symbol := r.URL.Query().Get("symbol")
	if symbol == "" {
		http.Error(w, "missing symbol", http.StatusBadRequest)
		return
	}

	limit := 100
	if l := r.URL.Query().Get("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil && parsed > 0 && parsed <= 1000 {
			limit = parsed
		}
	}

	writeJSON(w, map[string]interface{}{"symbol": symbol, "prices": f.GetHistory(symbol, limit)})
}

func (f *PriceFeed) handleGetStats(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, map[string]interface{}{
		"tokens_tracked": len(f.GetAllPrices()),
		"venues":         len(f.clients),
		"timestamp":      time.Now(),
	})
}

func (f *PriceFeed) handleWS(w http.ResponseWriter, r *http.Request) {
	conn, err := wsUpgrader.Upgrade(w, r, nil)
	if err != nil {
		return
	}
	f.wsHub.register <- conn

	// Drain inbound frames so the peer's pings are answered and a disconnect is
	// observed promptly.
	go func() {
		defer func() { f.wsHub.unregister <- conn }()
		for {
			if _, _, err := conn.ReadMessage(); err != nil {
				return
			}
		}
	}()
}

func writeJSON(w http.ResponseWriter, v interface{}) {
	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(v); err != nil {
		http.Error(w, "failed to encode response", http.StatusInternalServerError)
	}
}

var wsUpgrader = websocket.Upgrader{
	ReadBufferSize:  1024,
	WriteBufferSize: 1024,
	CheckOrigin:     wsauth.Check,
}

// PriceWsHub fans price updates out to connected WebSocket clients.
type PriceWsHub struct {
	clients    map[*websocket.Conn]bool
	broadcast  chan []byte
	register   chan *websocket.Conn
	unregister chan *websocket.Conn
	mu         sync.RWMutex
}

func NewPriceWsHub() *PriceWsHub {
	return &PriceWsHub{
		clients:    make(map[*websocket.Conn]bool),
		broadcast:  make(chan []byte, 256),
		register:   make(chan *websocket.Conn),
		unregister: make(chan *websocket.Conn),
	}
}

func (h *PriceWsHub) run() {
	for {
		select {
		case client := <-h.register:
			h.mu.Lock()
			h.clients[client] = true
			h.mu.Unlock()

		case client := <-h.unregister:
			h.mu.Lock()
			if _, ok := h.clients[client]; ok {
				delete(h.clients, client)
				_ = client.Close()
			}
			h.mu.Unlock()

		case message := <-h.broadcast:
			h.mu.Lock()
			for client := range h.clients {
				if err := client.WriteMessage(websocket.TextMessage, message); err != nil {
					_ = client.Close()
					delete(h.clients, client)
				}
			}
			h.mu.Unlock()
		}
	}
}

func (h *PriceWsHub) broadcastJSON(v interface{}) {
	data, err := json.Marshal(v)
	if err != nil {
		return
	}
	select {
	case h.broadcast <- data:
	default:
		// Drop the update rather than blocking the aggregation loop when a slow
		// client has filled the buffer.
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

func ensurePriceTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS token_prices (
			id BIGSERIAL PRIMARY KEY,
			token_address VARCHAR(42) NOT NULL,
			price_usd NUMERIC(78, 18) NOT NULL,
			timestamp BIGINT NOT NULL,
			source VARCHAR(64),
			created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
			UNIQUE(token_address, timestamp)
		)`,
		`CREATE INDEX IF NOT EXISTS idx_token_prices_address ON token_prices(token_address)`,
		`CREATE INDEX IF NOT EXISTS idx_token_prices_time ON token_prices(timestamp DESC)`,
	}
	for _, q := range queries {
		if _, err := pool.Exec(ctx, q); err != nil {
			return fmt.Errorf("prepare token_prices schema: %w", err)
		}
	}
	return nil
}

// filterOutliers removes quotes deviating more than 20% from the median.
func filterOutliers(quotes []ExchangePrice) []ExchangePrice {
	if len(quotes) < 3 {
		return quotes
	}

	sorted := make([]float64, len(quotes))
	for i, q := range quotes {
		sorted[i] = q.Price
	}
	sort.Float64s(sorted)
	median := sorted[len(sorted)/2]

	kept := make([]ExchangePrice, 0, len(quotes))
	for _, q := range quotes {
		if math.Abs(q.Price-median)/median <= 0.20 {
			kept = append(kept, q)
		}
	}
	if len(kept) == 0 {
		return quotes
	}
	return kept
}

// calculateConfidence reports how much independent agreement backs a price.
func calculateConfidence(sourceCount int) float64 {
	switch {
	case sourceCount >= 5:
		return 0.95
	case sourceCount >= 3:
		return 0.85
	case sourceCount >= 2:
		return 0.70
	default:
		return 0.50
	}
}

// toWeiString renders a price scaled by 1e18 with full precision.
func toWeiString(price float64) string {
	wei := new(big.Float).SetFloat64(price)
	wei.Mul(wei, big.NewFloat(1e18))
	scaled, _ := wei.Int(nil)
	return scaled.String()
}

// word extracts the i-th 32-byte word of an ABI-encoded return payload.
func word(data []byte, i int) []byte {
	start := i * 32
	if len(data) < start+32 {
		return make([]byte, 32)
	}
	return data[start : start+32]
}

// pow10 returns 10^n as a big.Int.
func pow10(n int) *big.Int {
	return new(big.Int).Exp(big.NewInt(10), big.NewInt(int64(n)), nil)
}
