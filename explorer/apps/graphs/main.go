// Package graphs provides graph/chart data API endpoints
package graphs

import (
	"context"
	"fmt"
	"net/http"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/go-redis/redis/v8"
	"github.com/jackc/pgx/v5"
	
	"tigersmartchain/explorer/services/analytics"
	"tigersmartchain/explorer/services/tokens"
	"tigersmartchain/explorer/services/priceoracle"
)

// Config holds the graph API configuration
type Config struct {
	DBURL       string
	RedisURL    string
	Port       string
	CacheTTL   time.Duration
}

// GraphService handles graph data requests
type GraphService struct {
	config      *Config
	db          *pgx.Conn
	redis       *redis.Client
	analyticsSvc *analytics.AnalyticsService
	tokenSvc   *tokens.TokenService
	priceSvc   *priceoracle.PriceOracle
}

// NewGraphService creates a new graph service
func NewGraphService(config *Config) (*GraphService, error) {
	ctx := context.Background()
	
	db, err := pgx.Connect(ctx, config.DBURL)
	if err != nil {
		return nil, err
	}
	
	rdb, err := redis.ParseURL(config.RedisURL)
	if err != nil {
		return nil, err
	}
	
	redisClient := redis.NewClient(rdb)
	
	return &GraphService{
		config:    config,
		db:       db,
		redis:    redisClient,
	}, nil
}

// SetServices sets the backend services
func (s *GraphService) SetServices(analyticsSvc *analytics.AnalyticsService, tokenSvc *tokens.TokenService, priceSvc *priceoracle.PriceOracle) {
	s.analyticsSvc = analyticsSvc
	s.tokenSvc = tokenSvc
	s.priceSvc = priceSvc
}

// ChartDataPoint represents a single data point on a chart
type ChartDataPoint struct {
	Timestamp time.Time `json:"timestamp"`
	Value     float64  `json:"value"`
	Label     string   `json:"label,omitempty"`
}

// ChartResponse represents a chart data response
type ChartResponse struct {
	Series []ChartSeries `json:"series"`
}

// ChartSeries represents a data series
type ChartSeries struct {
	Name   string        `json:"name"`
	Type   string        `json:"type"`
	Data  []ChartDataPoint `json:"data"`
}

// handleTPSChart handles TPS chart data
func (s *GraphService) handleTPSChart(c *gin.Context) {
	interval := c.DefaultQuery("interval", "24h")
	startTime, endTime := s.parseInterval(interval)
	
	// Try cache first
	cacheKey := fmt.Sprintf("chart:tps:%s", interval)
	if cached, err := s.redis.Get(c.Request.Context(), cacheKey).Result(); err == nil {
		c.JSON(http.StatusOK, gin.H{"data": cached})
		return
	}
	
	query := `
		SELECT date_trunc('minute', timestamp) as ts, 
		       COUNT(*) as tps
		FROM transactions
		WHERE timestamp >= $1 AND timestamp < $2
		GROUP BY date_trunc('minute', timestamp)
		ORDER BY ts
	`
	
	rows, err := s.db.Query(c.Request.Context(), query, startTime, endTime)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	series := ChartSeries{
		Name: "TPS",
		Type: "line",
		Data: []ChartDataPoint{},
	}
	
	for rows.Next() {
		var dp ChartDataPoint
		if err := rows.Scan(&dp.Timestamp, &dp.Value); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		series.Data = append(series.Data, dp)
	}
	
	// Cache the result
	s.redis.Set(c.Request.Context(), cacheKey, series, s.config.CacheTTL)
	
	c.JSON(http.StatusOK, ChartResponse{
		Series: []ChartSeries{series},
	})
}

// handleGasChart handles gas price chart data
func (s *GraphService) handleGasChart(c *gin.Context) {
	interval := c.DefaultQuery("interval", "24h")
	startTime, _ := s.parseInterval(interval)
	
	query := `
		SELECT date_trunc('hour', timestamp) as ts,
		       AVG(gas_price) as avg_gas,
		       MIN(gas_price) as min_gas,
		       MAX(gas_price) as max_gas
		FROM transactions
		WHERE timestamp >= $1
		GROUP BY date_trunc('hour', timestamp)
		ORDER BY ts
	`
	
	rows, err := s.db.Query(c.Request.Context(), query, startTime)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	avgSeries := ChartSeries{Name: "Average Gas", Type: "line", Data: []ChartDataPoint{}}
	minSeries := ChartSeries{Name: "Min Gas", Type: "line", Data: []ChartDataPoint{}}
	maxSeries := ChartSeries{Name: "Max Gas", Type: "line", Data: []ChartDataPoint{}}
	
	for rows.Next() {
		var avg, min, max float64
		var ts time.Time
		if err := rows.Scan(&ts, &avg, &min, &max); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		avgSeries.Data = append(avgSeries.Data, ChartDataPoint{Timestamp: ts, Value: avg})
		minSeries.Data = append(minSeries.Data, ChartDataPoint{Timestamp: ts, Value: min})
		maxSeries.Data = append(maxSeries.Data, ChartDataPoint{Timestamp: ts, Value: max})
	}
	
	c.JSON(http.StatusOK, ChartResponse{
		Series: []ChartSeries{avgSeries, minSeries, maxSeries},
	})
}

// handleTVLChart handles TVL chart data
func (s *GraphService) handleTVLChart(c *gin.Context) {
	interval := c.DefaultQuery("interval", "30d")
	startTime, _ := s.parseInterval(interval)
	
	query := `
		SELECT date_trunc('day', timestamp) as ts,
		       SUM(value) as tvl
		FROM token_balances
		WHERE timestamp >= $1
		GROUP BY date_trunc('day', timestamp)
		ORDER BY ts
	`
	
	rows, err := s.db.Query(c.Request.Context(), query, startTime)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	series := ChartSeries{
		Name: "TVL",
		Type: "area",
		Data: []ChartDataPoint{},
	}
	
	for rows.Next() {
		var dp ChartDataPoint
		if err := rows.Scan(&dp.Timestamp, &dp.Value); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		series.Data = append(series.Data, dp)
	}
	
	c.JSON(http.StatusOK, ChartResponse{
		Series: []ChartSeries{series},
	})
}

// handleMarketCapChart handles market cap chart data
func (s *GraphService) handleMarketCapChart(c *gin.Context) {
	interval := c.DefaultQuery("interval", "30d")
	startTime, _ := s.parseInterval(interval)
	
	query := `
		SELECT date_trunc('day', timestamp) as ts,
		       SUM(circulating_supply * price) as market_cap
		FROM token_prices
		WHERE timestamp >= $1
		GROUP BY date_trunc('day', timestamp)
		ORDER BY ts
	`
	
	rows, err := s.db.Query(c.Request.Context(), query, startTime)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	series := ChartSeries{
		Name: "Market Cap",
		Type: "area",
		Data: []ChartDataPoint{},
	}
	
	for rows.Next() {
		var dp ChartDataPoint
		if err := rows.Scan(&dp.Timestamp, &dp.Value); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		series.Data = append(series.Data, dp)
	}
	
	c.JSON(http.StatusOK, ChartResponse{
		Series: []ChartSeries{series},
	})
}

// handleTokenPriceChart handles token price chart data
func (s *GraphService) handleTokenPriceChart(c *gin.Context) {
	address := c.Param("address")
	interval := c.DefaultQuery("interval", "30d")
	startTime, _ := s.parseInterval(interval)
	
	query := `
		SELECT date_trunc('hour', timestamp) as ts,
		       price, volume_24h, market_cap
		FROM token_prices
		WHERE token_address = $1 AND timestamp >= $2
		ORDER BY ts
	`
	
	rows, err := s.db.Query(c.Request.Context(), query, address, startTime)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	priceSeries := ChartSeries{Name: "Price", Type: "line", Data: []ChartDataPoint{}}
	volumeSeries := ChartSeries{Name: "Volume", Type: "bar", Data: []ChartDataPoint{}}
	capSeries := ChartSeries{Name: "Market Cap", Type: "line", Data: []ChartDataPoint{}}
	
	for rows.Next() {
		var dp ChartDataPoint
		var volume, cap float64
		if err := rows.Scan(&dp.Timestamp, &dp.Value, &volume, &cap); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		priceSeries.Data = append(priceSeries.Data, dp)
		volumeSeries.Data = append(volumeSeries.Data, ChartDataPoint{Timestamp: dp.Timestamp, Value: volume})
		capSeries.Data = append(capSeries.Data, ChartDataPoint{Timestamp: dp.Timestamp, Value: cap})
	}
	
	c.JSON(http.StatusOK, ChartResponse{
		Series: []ChartSeries{priceSeries, volumeSeries, capSeries},
	})
}

// handleTransactionVolumeChart handles transaction volume chart
func (s *GraphService) handleTransactionVolumeChart(c *gin.Context) {
	interval := c.DefaultQuery("interval", "30d")
	startTime, _ := s.parseInterval(interval)
	
	query := `
		SELECT date_trunc('day', timestamp) as ts,
		       COUNT(*) as tx_count,
		       SUM(value) as volume
		FROM transactions
		WHERE timestamp >= $1
		GROUP BY date_trunc('day', timestamp)
		ORDER BY ts
	`
	
	rows, err := s.db.Query(c.Request.Context(), query, startTime)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	countSeries := ChartSeries{Name: "Transaction Count", Type: "bar", Data: []ChartDataPoint{}}
	volumeSeries := ChartSeries{Name: "Volume", Type: "area", Data: []ChartDataPoint{}}
	
	for rows.Next() {
		var ts time.Time
		var count int64
		var volume float64
		if err := rows.Scan(&ts, &count, &volume); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		countSeries.Data = append(countSeries.Data, ChartDataPoint{Timestamp: ts, Value: float64(count)})
		volumeSeries.Data = append(volumeSeries.Data, ChartDataPoint{Timestamp: ts, Value: volume})
	}
	
	c.JSON(http.StatusOK, ChartResponse{
		Series: []ChartSeries{countSeries, volumeSeries},
	})
}

// handleHolderDistributionChart handles holder distribution pie chart
func (s *GraphService) handleHolderDistributionChart(c *gin.Context) {
	address := c.Param("address")
	
	query := `
		SELECT 
			CASE 
				WHEN pct <= 1 THEN 'Whales (>1%)'
				WHEN pct <= 10 THEN 'Large (0.1-1%)'
				WHEN pct <= 50 THEN 'Medium (0.01-0.1%)'
				ELSE 'Small (<0.01%)'
			END as category,
			COUNT(*) as count
		FROM (
			SELECT address, balance * 100.0 / (SELECT SUM(balance) FROM token_holders WHERE token_address = $1) as pct
			FROM token_holders
			WHERE token_address = $1
		) sub
		GROUP BY category
		ORDER BY count DESC
	`
	
	rows, err := s.db.Query(c.Request.Context(), query, address)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	series := ChartSeries{Name: "Holders", Type: "pie", Data: []ChartDataPoint{}}
	
	for rows.Next() {
		var dp ChartDataPoint
		if err := rows.Scan(&dp.Label, &dp.Value); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		series.Data = append(series.Data, dp)
	}
	
	c.JSON(http.StatusOK, ChartResponse{
		Series: []ChartSeries{series},
	})
}

// handleGasHeatmap handles gas price heatmap data
func (s *GraphService) handleGasHeatmap(c *gin.Context) {
	days := c.DefaultQuery("days", "7")
	
	query := `
		SELECT 
			EXTRACT(DOW FROM timestamp) as day_of_week,
			EXTRACT(HOUR FROM timestamp) as hour,
			AVG(gas_price) as avg_gas
		FROM transactions
		WHERE timestamp >= NOW() - INTERVAL '1 day' * $1
		GROUP BY day_of_week, hour
		ORDER BY day_of_week, hour
	`
	
	daysInt, _ := strconv.Atoi(days)
	
	rows, err := s.db.Query(c.Request.Context(), query, daysInt)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type HeatmapPoint struct {
		X     int     `json:"x"`
		Y     int     `json:"y"`
		Value float64 `json:"value"`
	}
	
	var data []HeatmapPoint
	for rows.Next() {
		var hp HeatmapPoint
		if err := rows.Scan(&hp.X, &hp.Y, &hp.Value); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		data = append(data, hp)
	}
	
	c.JSON(http.StatusOK, gin.H{"heatmap": data})
}

// handleDailyStatsChart handles daily stats chart
func (s *GraphService) handleDailyStatsChart(c *gin.Context) {
	interval := c.DefaultQuery("interval", "30d")
	startTime, _ := s.parseInterval(interval)
	
	query := `
		SELECT date_trunc('day', timestamp) as ts,
		       COUNT(*) as tx_count,
		       COUNT(DISTINCT from_address) as active_addresses,
		       AVG(gas_price) as avg_gas,
		       SUM(gas_used * gas_price) as gas_fees
		FROM transactions
		WHERE timestamp >= $1
		GROUP BY date_trunc('day', timestamp)
		ORDER BY ts
	`
	
	rows, err := s.db.Query(c.Request.Context(), query, startTime)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	txSeries := ChartSeries{Name: "Transactions", Type: "bar", Data: []ChartDataPoint{}}
	addrSeries := ChartSeries{Name: "Active Addresses", Type: "line", Data: []ChartDataPoint{}}
	gasSeries := ChartSeries{Name: "Avg Gas", Type: "line", Data: []ChartDataPoint{}}
	feeSeries := ChartSeries{Name: "Gas Fees", Type: "area", Data: []ChartDataPoint{}}
	
	for rows.Next() {
		var dp ChartDataPoint
		var activeAddr int64
		var avgGas, fees float64
		if err := rows.Scan(&dp.Timestamp, &dp.Value, &activeAddr, &avgGas, &fees); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		txSeries.Data = append(txSeries.Data, dp)
		addrSeries.Data = append(addrSeries.Data, ChartDataPoint{Timestamp: dp.Timestamp, Value: float64(activeAddr)})
		gasSeries.Data = append(gasSeries.Data, ChartDataPoint{Timestamp: dp.Timestamp, Value: avgGas})
		feeSeries.Data = append(feeSeries.Data, ChartDataPoint{Timestamp: dp.Timestamp, Value: fees})
	}
	
	c.JSON(http.StatusOK, ChartResponse{
		Series: []ChartSeries{txSeries, addrSeries, gasSeries, feeSeries},
	})
}

// parseInterval parses the interval string to start and end times
func (s *GraphService) parseInterval(interval string) (time.Time, time.Time) {
	now := time.Now()
	var start time.Time
	
	switch interval {
	case "1h":
		start = now.Add(-1 * time.Hour)
	case "24h":
		start = now.Add(-24 * time.Hour)
	case "7d":
		start = now.Add(-7 * 24 * time.Hour)
	case "30d":
		start = now.Add(-30 * 24 * time.Hour)
	case "90d":
		start = now.Add(-90 * 24 * time.Hour)
	case "1y":
		start = now.Add(-365 * 24 * time.Hour)
	default:
		start = now.Add(-24 * time.Hour)
	}
	
	return start, now
}

// Router sets up the graph API router
func (s *GraphService) Router() *gin.Engine {
	r := gin.Default()
	
	r.GET("/charts/tps", s.handleTPSChart)
	r.GET("/charts/gas", s.handleGasChart)
	r.GET("/charts/tvl", s.handleTVLChart)
	r.GET("/charts/marketcap", s.handleMarketCapChart)
	r.GET("/charts/token/:address", s.handleTokenPriceChart)
	r.GET("/charts/volume", s.handleTransactionVolumeChart)
	r.GET("/charts/holders/:address", s.handleHolderDistributionChart)
	r.GET("/charts/heatmap", s.handleGasHeatmap)
	r.GET("/charts/daily", s.handleDailyStatsChart)
	
	return r
}

// Start starts the graph server
func (s *GraphService) Start() error {
	r := s.Router()
	return r.Run(s.config.Port)
}

// StartGraphServer starts the graph API server
func StartGraphServer(port string, dbURL string, redisURL string) error {
	config := &Config{
		DBURL:     dbURL,
		RedisURL:  redisURL,
		Port:     port,
		CacheTTL:  5 * time.Minute,
	}
	
	svc, err := NewGraphService(config)
	if err != nil {
		return err
	}
	
	return svc.Start()
}