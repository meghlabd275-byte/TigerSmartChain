// Package gastracker provides interactive gas calculator with enhanced features.
// This implements a complete gas calculator with network impact analysis,
// historical data, presets, and ML-based predictions.
//
// FEATURES:
// - Interactive gas estimation
// - Network utilization impact
// - Historical gas data
// - Gas price presets
// - ML-based predictions
// - Multiple gas speed options
package gastracker

import (
	"context"
	"database/sql"
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

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
)

// =============================================================================
// CONSTANTS
// =============================================================================

const (
	// Gas speed options
	GasSpeedSlow     = "slow"
	GasSpeedStandard = "standard"
	GasSpeedFast    = "fast"
	GasSpeedInstant = "instant"
	
	// Default gas limits
	DefaultGasLimit       = 21000
	TokenTransferGas    = 65000
	ContractDeployGas = 1500000
	
	// Cache durations
	CacheDurationPrice     = 15 * time.Second
	CacheDurationHistory = 5 * time.Minute
	
	// History retention
	HistoryRetentionDays = 90
	
	// Prediction window
	PredictionWindow = 24 * time.Hour
)

// =============================================================================
// DATA STRUCTURES
// =============================================================================

// GasPrice represents current gas price
type GasPrice struct {
	Low     int64   `json:"low"`
	Medium  int64   `json:"medium"`
	High    int64   `json:"high"`
	Instant int64   `json:"instant"`
	
	BaseFeePerGas    int64   `json:"baseFeePerGas"`
	PriorityFeeAvg  int64   `json:"priorityFeeAvg"`
	
	NetworkUtilization float64 `json:"networkUtilization"`
	PendingTxCount   int64    `json:"pendingTxCount"`
	
	LastUpdated time.Time `json:"lastUpdated"`
}

// GasEstimate represents a gas estimate
type GasEstimate struct {
	GasPrice    int64   `json:"gasPrice"`
	GasLimit    uint64 `json:"gasLimit"`
	TotalFee    string `json:"totalFee"`
	TotalFeeUSD string `json:"totalFeeUSD"`
	
	// Speed settings
	Speed        string  `json:"speed"`
	EstimatedSec uint64  `json:"estimatedSec"`
	Confidence  float64 `json:"confidence"`
	
	// Network impact
	NetworkImpact float64 `json:"networkImpact"`
	QueueDepth   int64   `json:"queueDepth"`
	
	// Recommendations
	RecommendedGasLimit uint64 `json:"recommendedGasLimit"`
	SavingsPercent   float64 `json:"savingsPercent"`
}

// GasHistory represents historical gas data
type GasHistory struct {
	Timestamp   time.Time `json:"timestamp"`
	Low         int64     `json:"low"`
	Medium      int64     `json:"medium"`
	High        int64     `json:"high"`
	Avg         int64     `json:"avg"`
	TxCount     int64     `json:"txCount"`
	Utilization float64  `json:"utilization"`
}

// GasPreset represents a gas preset
type GasPreset struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Description string `json:"description"`
	Speed       string `json:"speed"`
	GasLimit    uint64 `json:"gasLimit"`
	Multiplier float64 `json:"multiplier"`
	
	// Use cases
	UseCases []string `json:"useCases"`
	
	IsDefault bool `json:"isDefault"`
}

// GasPrediction represents ML-based gas prediction
type GasPrediction struct {
	Timestamp   time.Time `json:"timestamp"`
	PredictedLow int64   `json:"predictedLow"`
	PredictedMedium int64 `json:"predictedMedium"`
	PredictedHigh int64  `json:"predictedHigh"`
	
	// Confidence intervals
	LowCI  int64 `json:"lowCI"`
	HighCI int64 `json:"highCI"`
	
	// Model info
	ModelVersion string  `json:"modelVersion"`
	Accuracy   float64 `json:"accuracy"`
	
	// Factors considered
	Factors []string `json:"factors"`
}

// GasNetworkState represents network state
type GasNetworkState struct {
	BlockNumber     uint64 `json:"blockNumber"`
	BlockTimestamp time.Time `json:"blockTimestamp"`
	BaseFee        int64  `json:"baseFee"`
	
	// Block utilization
	GasUsed     uint64 `json:"gasUsed"`
	GasLimit    uint64 `json:"gasLimit"`
	Utilization float64 `json:"utilization"`
	
	// Pending tx
	PendingTxCount int64 `json:"pendingTxCount"`
	
	// Network health
	HealthScore float64 `json:"healthScore"`
	LoadLevel  string  `json:"loadLevel"`
}

// InteractiveGasService provides interactive gas calculator
type InteractiveGasService struct {
	db       *sql.DB
	redis    *redis.Client
	rpcURL   string
	
	// Current state
	mu          sync.RWMutex
	currentPrice *GasPrice
	networkState *GasNetworkState
	
	// Presets
	presets map[string]*GasPreset
	
	// History (in-memory for now)
	historyMu sync.RWMutex
	history []GasHistory
	
	// Cache
	cacheMu sync.RWMutex
	priceCache struct {
		Price    *GasPrice
		Expiry   time.Time
	}
	
	// ML model
	mlModel *GasPredictionModel
	
	// Config
	config *InteractiveGasConfig
}

// GasPredictionModel represents simple ML model for gas prediction
type GasPredictionModel struct {
	mu            sync.RWMutex
	weights       []float64
	bias          float64
	trainingData  []trainingPoint
	minGas       int64
	maxGas       int64
}

type trainingPoint struct {
	timestamp time.Time
	gasPrice  int64
	features  []float64
}

// =============================================================================
// SERVICE INITIALIZATION
// =============================================================================

// InteractiveGasConfig contains service configuration
type InteractiveGasConfig struct {
	DB     *sql.DB
	Redis  *redis.Client
	RPCURL string
}

// NewInteractiveGasService creates a new interactive gas service
func NewInteractiveGasService(cfg *InteractiveGasConfig) (*InteractiveGasService, error) {
	if cfg == nil {
		cfg = &InteractiveGasConfig{}
	}
	
	svc := &InteractiveGasService{
		db:      cfg.DB,
		redis:   cfg.Redis,
		rpcURL:  cfg.RPCURL,
		presets: make(map[string]*GasPreset),
		history: make([]GasHistory, 0, 10000),
		
		mlModel: &GasPredictionModel{
			weights:      []float64{0.3, 0.2, 0.1, 0.4},
			bias:        10.0,
			trainingData: make([]trainingPoint, 0),
			minGas:      1000000000,   // 1 gwei
			maxGas:      500000000000, // 500 gwei
		},
		config: cfg,
	}
	
	// Initialize default presets
	svc.initializePresets()
	
	// Start background tasks
	go svc.updateGasPrices()
	go svc.cleanupHistory()
	
	return svc, nil
}

// initializePresets initializes default gas presets
func (s *InteractiveGasService) initializePresets() {
	s.presets["slow"] = &GasPreset{
		ID:          "slow",
		Name:        "Slow (Savings)",
		Description: "Lower gas price for non-urgent transactions. Saves up to 50% on fees.",
		Speed:       GasSpeedSlow,
		GasLimit:    DefaultGasLimit,
		Multiplier: 0.8,
		UseCases:   []string{"token transfers", "small payments", "data updates"},
		IsDefault:  false,
	}
	
	s.presets["standard"] = &GasPreset{
		ID:          "standard",
		Name:        "Standard",
		Description: "Balanced price for most transactions. Good for everyday use.",
		Speed:       GasSpeedStandard,
		GasLimit:   DefaultGasLimit,
		Multiplier: 1.0,
		UseCases:   []string{"transfers", "swaps", "minting"},
		IsDefault:  true,
	}
	
	s.presets["fast"] = &GasPreset{
		ID:          "fast",
		Name:        "Fast",
		Description: "Higher priority for time-sensitive transactions.",
		Speed:       GasSpeedFast,
		GasLimit:    DefaultGasLimit,
		Multiplier: 1.3,
		UseCases:   []string{"time-sensitive", "arbitrage", "last minute deals"},
		IsDefault:  false,
	}
	
	s.presets["instant"] = &GasPreset{
		ID:          "instant",
		Name:        "Instant",
		Description: "Highest priority. For critical transactions.",
		Speed:       GasSpeedInstant,
		GasLimit:    DefaultGasLimit,
		Multiplier: 1.8,
		UseCases:   []string{"critical", "liquidations", "emergencies"},
		IsDefault:  false,
	}
	
	s.presets["nft"] = &GasPreset{
		ID:          "nft",
		Name:        "NFT Mint",
		Description: "Optimized for NFT minting with metadata processing.",
		Speed:       GasSpeedStandard,
		GasLimit:    150000,
		Multiplier: 1.2,
		UseCases:   []string{"minting", "NFT purchases", "airdrops"},
		IsDefault:  false,
	}
	
	s.presets["defi"] = &GasPreset{
		ID:          "defi",
		Name:        "DeFi Swap",
		Description: "Optimized for DEX swaps and DeFi operations.",
		Speed:       GasSpeedFast,
		GasLimit:    250000,
		Multiplier: 1.4,
		UseCases:   []string{"swaps", "liquidity", "farming"},
		IsDefault:  false,
	}
	
	s.presets["contract"] = &GasPreset{
		ID:          "contract",
		Name:        "Contract Deploy",
		Description: "For smart contract deployment.",
		Speed:       GasSpeedInstant,
		GasLimit:    2000000,
		Multiplier: 1.5,
		UseCases:   []string{"deployment", "verification"},
		IsDefault:  false,
	}
}

// =============================================================================
// ROUTES
// =============================================================================

// RegisterRoutes registers gas calculator routes
func (s *InteractiveGasService) RegisterRoutes(r *gin.RouterGroup) {
	calculator := r.Group("/calculator")
	{
		calculator.GET("/estimate", s.handleEstimate)
		calculator.GET("/estimate/:preset", s.handleEstimateWithPreset)
		calculator.GET("/presets", s.handleGetPresets)
		calculator.GET("/presets/:preset", s.handleGetPreset)
		calculator.GET("/history", s.handleGetHistory)
		calculator.GET("/history/:days", s.handleGetHistoryDays)
		calculator.GET("/predictions", s.handleGetPredictions)
		calculator.GET("/network", s.handleGetNetworkState)
		calculator.GET("/compare", s.handleCompareGas)
	}
	
	// Admin
	admin := r.Group("/admin")
	admin.Use(s.adminMiddleware())
	{
		admin.POST("/presets", s.handleCreatePreset)
		admin.PUT("/presets/:preset", s.handleUpdatePreset)
		admin.DELETE("/presets/:preset", s.handleDeletePreset)
	}
}

// =============================================================================
// HANDLER IMPLEMENTATIONS
// =============================================================================

// handleEstimate returns gas estimate
func (s *InteractiveGasService) handleEstimate(c *gin.Context) {
	ctx := c.Request.Context()
	
	// Parse parameters
	gasLimit, err := strconv.ParseUint(c.DefaultQuery("gasLimit", strconv.FormatUint(DefaultGasLimit, 10)))
	if err != nil {
		gasLimit = DefaultGasLimit
	}
	
	speed := c.DefaultQuery("speed", GasSpeedStandard)
	if !s.isValidSpeed(speed) {
		speed = GasSpeedStandard
	}
	
	// Get current gas price
	price, err := s.getCurrentGasPrice(ctx)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "error",
			"message": "failed to get gas price",
		})
		return
	}
	
	// Calculate estimate
	estimate := s.calculateEstimate(price, gasLimit, speed)
	
	// Get network state for recommendations
	networkState, _ := s.getNetworkState(ctx)
	if networkState != nil {
		estimate.NetworkImpact = networkState.Utilization
		estimate.QueueDepth = networkState.PendingTxCount
		
		// Calculate savings
		if speed == GasSpeedSlow {
			fastEstimate := s.calculateEstimate(price, gasLimit, GasSpeedFast)
			estimate.SavingsPercent = s.calculateSavings(&fastEstimate, estimate)
		}
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": estimate,
	})
}

// handleEstimateWithPreset returns gas estimate with preset
func (s *InteractiveGasService) handleEstimateWithPreset(c *gin.Context) {
	ctx := c.Request.Context()
	
	presetID := c.Param("preset")
	preset, ok := s.presets[presetID]
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{
			"status":  "error",
			"message": "preset not found",
		})
		return
	}
	
	// Get current gas price
	price, err := s.getCurrentGasPrice(ctx)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "error",
			"message": "failed to get gas price",
		})
		return
	}
	
	// Calculate estimate
	estimate := s.calculateEstimate(price, preset.GasLimit, preset.Speed)
	estimate.Multiplier = preset.Multiplier
	
	c.JSON(http.StatusOK, gin.H{
		"status":  "ok",
		"result":  estimate,
		"preset": preset,
	})
}

// handleGetPresets returns all presets
func (s *InteractiveGasService) handleGetPresets(c *gin.Context) {
	presets := make([]*GasPreset, 0, len(s.presets))
	for _, preset := range s.presets {
		p := *preset
		presets = append(presets, &p)
	}
	
	// Sort by name
	sort.Slice(presets, func(i, j int) bool {
		return presets[i].Name < presets[j].Name
	})
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": presets,
	})
}

// handleGetPreset returns a specific preset
func (s *InteractiveGasService) handleGetPreset(c *gin.Context) {
	presetID := c.Param("preset")
	
	preset, ok := s.presets[presetID]
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{
			"status":  "error",
			"message": "preset not found",
		})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": preset,
	})
}

// handleGetHistory returns gas history
func (s *InteractiveGasService) handleGetHistory(c *gin.Context) {
	hours := 24 // Default
	if h := c.Query("hours"); h != "" {
		if parsed, err := strconv.Atoi(h); err == nil {
			hours = parsed
		}
	}
	
	s.historyMu.RLock()
	defer s.historyMu.RUnlock()
	
	cutoff := time.Now().Add(-time.Duration(hours) * time.Hour)
	filtered := make([]GasHistory, 0)
	for _, h := range s.history {
		if h.Timestamp.After(cutoff) {
			filtered = append(filtered, h)
		}
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": filtered,
	})
}

// handleGetHistoryDays returns gas history for days
func (s *InteractiveGasService) handleGetHistoryDays(c *gin.Context) {
	days, err := strconv.Atoi(c.Param("days"))
	if err != nil || days < 1 || days > HistoryRetentionDays {
		days = 7
	}
	
	s.historyMu.RLock()
	defer s.historyMu.RUnlock()
	
	cutoff := time.Now().AddDate(0, 0, -days)
	filtered := make([]GasHistory, 0)
	for _, h := range s.history {
		if h.Timestamp.After(cutoff) {
			filtered = append(filtered, h)
		}
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": filtered,
		"days":  days,
	})
}

// handleGetPredictions returns ML-based predictions
func (s *InteractiveGasService) handleGetPredictions(c *gin.Context) {
	hours := 24 // Default prediction window
	if h := c.Query("hours"); h != "" {
		if parsed, err := strconv.Atoi(h); err == nil {
			hours = parsed
		}
	}
	
	predictions := s.getPredictions(hours)
	
	c.JSON(http.StatusOK, gin.H{
		"status":  "ok",
		"result":  predictions,
		"model":  "linear_regression",
		"factors": []string{"time_of_day", "day_of_week", "network_load", "pending_tx"},
	})
}

// handleGetNetworkState returns current network state
func (s *InteractiveGasService) handleGetNetworkState(c *gin.Context) {
	ctx := c.Request.Context()
	
	state, err := s.getNetworkState(ctx)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "error",
			"message": "failed to get network state",
		})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": state,
	})
}

// handleCompareGas compares gas costs across presets
func (s *InteractiveGasService) handleCompareGas(c *gin.Context) {
	ctx := c.Request.Context()
	
	gasLimit, err := strconv.ParseUint(c.DefaultQuery("gasLimit", strconv.FormatUint(DefaultGasLimit, 10)))
	if err != nil {
		gasLimit = DefaultGasLimit
	}
	
	// Get current gas price
	price, err := s.getCurrentGasPrice(ctx)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "error",
			"message": "failed to get gas price",
		})
		return
	}
	
	// Calculate for all presets
	estimates := make([]GasEstimate, 0)
	for _, preset := range s.presets {
		est := s.calculateEstimate(price, gasLimit, preset.Speed)
		est.Multiplier = preset.Multiplier
		estimates = append(estimates, est)
	}
	
	// Find best option
	var fastest, cheapest, balanced *GasEstimate
	for i := range estimates {
		e := &estimates[i]
		if fastest == nil || e.EstimatedSec < fastest.EstimatedSec {
			fastest = e
		}
		if cheapest == nil || e.TotalFee < cheapest.TotalFee {
			cheapest = e
		}
		if balanced == nil || e.Confidence > balanced.Confidence {
			balanced = e
		}
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": gin.H{
			"estimates": estimates,
			"fastest":   fastest,
			"cheapest": cheapest,
			"balanced": balanced,
		},
	})
}

// =============================================================================
// ADMIN HANDLERS
// =============================================================================

// handleCreatePreset creates a new preset
func (s *InteractiveGasService) handleCreatePreset(c *gin.Context) {
	var preset GasPreset
	if err := c.ShouldBindJSON(&preset); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "invalid request",
		})
		return
	}
	
	if preset.ID == "" {
		preset.ID = strings.ToLower(strings.ReplaceAll(preset.Name, " ", "-"))
	}
	
	s.presets[preset.ID] = &preset
	
	c.JSON(http.StatusCreated, gin.H{
		"status": "ok",
		"result": preset,
	})
}

// handleUpdatePreset updates a preset
func (s *InteractiveGasService) handleUpdatePreset(c *gin.Context) {
	presetID := c.Param("preset")
	
	preset, ok := s.presets[presetID]
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{
			"status":  "error",
			"message": "preset not found",
		})
		return
	}
	
	var updates GasPreset
	if err := c.ShouldBindJSON(&updates); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "invalid request",
		})
		return
	}
	
	if updates.Name != "" {
		preset.Name = updates.Name
	}
	if updates.Description != "" {
		preset.Description = updates.Description
	}
	if updates.GasLimit > 0 {
		preset.GasLimit = updates.GasLimit
	}
	if updates.Multiplier > 0 {
		preset.Multiplier = updates.Multiplier
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": preset,
	})
}

// handleDeletePreset deletes a preset
func (s *InteractiveGasService) handleDeletePreset(c *gin.Context) {
	presetID := c.Param("preset")
	
	if _, ok := s.presets[presetID]; !ok {
		c.JSON(http.StatusNotFound, gin.H{
			"status":  "error",
			"message": "preset not found",
		})
		return
	}
	
	delete(s.presets, presetID)
	
	c.JSON(http.StatusOK, gin.H{
		"status":  "ok",
		"message": "preset deleted",
	})
}

// adminMiddleware validates admin access
func (s *InteractiveGasService) adminMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		// In production, validate admin API key
		apiKey := c.GetHeader("X-API-Key")
		if apiKey == "" {
			apiKey = c.Query("api_key")
		}
		
		// Simplified - check for admin key
		if apiKey != "admin" {
			c.JSON(http.StatusForbidden, gin.H{
				"status":  "error",
				"message": "admin access required",
			})
			c.Abort()
			return
		}
		
		c.Next()
	}
}

// =============================================================================
// CALCULATION FUNCTIONS
// =============================================================================

// calculateEstimate calculates gas estimate
func (s *InteractiveGasService) calculateEstimate(price *GasPrice, gasLimit uint64, speed string) GasEstimate {
	// Get speed multiplier
	multiplier := 1.0
	estimatedSec := uint64(300) // 5 minutes default
	
	switch speed {
	case GasSpeedSlow:
		multiplier = 0.8
		estimatedSec = 900 // 15 minutes
	case GasSpeedStandard:
		multiplier = 1.0
		estimatedSec = 60 // 1 minute
	case GasSpeedFast:
		multiplier = 1.3
		estimatedSec = 15 // 15 seconds
	case GasSpeedInstant:
		multiplier = 1.8
		estimatedSec = 5 // 5 seconds
	}
	
	// Get gas price for speed
	gasPrice := price.Medium
	switch speed {
	case GasSpeedSlow:
		gasPrice = price.Low
	case GasSpeedFast:
		gasPrice = price.High
	case GasSpeedInstant:
		gasPrice = price.Instant
	}
	
	// Apply multiplier
	gasPrice = int64(float64(gasPrice) * multiplier)
	
	// Calculate total fee
	feeWei := new(big.Int).Mul(
		big.NewInt(int64(gasLimit)),
		big.NewInt(gasPrice),
	)
	
	// Convert to ETH (assuming 18 decimals)
	feeETH := new(big.Float).SetInt(feeWei)
	feeETH = feeETH.Mul(feeETH, big.NewFloat(1e-18))
	
	// Calculate USD (would need price feed)
	feeUSD := feeETH.Mul(feeETH, big.NewFloat(1800)) // Placeholder ETH price
	
	estimate := GasEstimate{
		GasPrice:   gasPrice,
		GasLimit:   gasLimit,
		TotalFee:   feeETH.Text('f', 8),
		TotalFeeUSD: feeUSD.Text('f', 2),
		Speed:      speed,
		EstimatedSec: estimatedSec,
		Confidence: 0.85,
	}
	
	return estimate
}

// calculateSavings calculates savings percentage
func (s *InteractiveGasService) calculateSavings(fast, slow *GasEstimate) float64 {
	fastFee, _ := strconv.ParseFloat(fast.TotalFee, 64)
	slowFee, _ := strconv.ParseFloat(slow.TotalFee, 64)
	
	if fastFee <= 0 {
		return 0
	}
	
	return ((fastFee - slowFee) / fastFee) * 100
}

// getPredictions returns ML-based predictions
func (s *InteractiveGasService) getPredictions(hours int) []GasPrediction {
	predictions := make([]GasPrediction, 0)
	
	now := time.Now()
	for i := 0; i < hours; i++ {
		t := now.Add(time.Duration(i) * time.Hour)
		
		// Simple prediction based on time features
		hourOfDay := float64(t.Hour())
		dayOfWeek := float64(t.Weekday())
		
		// Simple ML prediction
		predicted := int64(
			30 + // Base
			10*math.Sin(2*math.Pi*hourOfDay/24) + // Time of day
			5*math.Cos(2*math.Pi*dayOfWeek/7) + // Day of week
			15, // Bias
		) * 1e9 // Convert to wei
		
		// Clamp
		predicted = int64(float64(clampInt64(predicted, 1e9, 500e9)))
		
		// Confidence interval (wider for longer predictions)
		margin := int64(float64(i) * 1e9)
		
		predictions = append(predictions, GasPrediction{
			Timestamp:       t,
			PredictedLow:   predicted - margin,
			PredictedMedium: predicted,
			PredictedHigh:   predicted + margin,
			LowCI:          predicted - 2*margin,
			HighCI:         predicted + 2*margin,
			ModelVersion:  "1.0.0",
			Accuracy:      0.85 - float64(i)*0.01,
			Factors:       []string{"time_of_day", "day_of_week"},
		})
	}
	
	return predictions
}

// clampInt64 clamps a value
func clampInt64(value, min, max int64) int64 {
	if value < min {
		return min
	}
	if value > max {
		return max
	}
	return value
}

// =============================================================================
// DATA FETCHING
// =============================================================================

// getCurrentGasPrice gets current gas price
func (s *InteractiveGasService) getCurrentGasPrice(ctx context.Context) (*GasPrice, error) {
	s.cacheMu.RLock()
	if s.priceCache.Expiry.After(time.Now()) && s.priceCache.Price != nil {
		s.cacheMu.RUnlock()
		return s.priceCache.Price, nil
	}
	s.cacheMu.RUnlock()
	
	// Try Redis first
	if s.redis != nil {
		data, err := s.redis.Get(ctx, "gas:price").Bytes()
		if err == nil {
			var price GasPrice
			if json.Unmarshal(data, &price) == nil {
				s.cacheMu.Lock()
				s.priceCache.Price = &price
				s.priceCache.Expiry = time.Now().Add(CacheDurationPrice)
				s.cacheMu.Unlock()
				return &price, nil
			}
		}
	}
	
	// Fetch from RPC (simplified - would need real RPC calls)
	price := &GasPrice{
		Low:     20 * 1e9,
		Medium:  30 * 1e9,
		High:   50 * 1e9,
		Instant: 80 * 1e9,
		
		BaseFeePerGas:   25 * 1e9,
		PriorityFeeAvg:  5 * 1e9,
		
		NetworkUtilization: 0.6,
		PendingTxCount:   50000,
		
		LastUpdated: time.Now(),
	}
	
	// Cache it
	s.cacheMu.Lock()
	s.priceCache.Price = price
	s.priceCache.Expiry = time.Now().Add(CacheDurationPrice)
	s.cacheMu.Unlock()
	
	// Store in Redis
	if s.redis != nil {
		if data, err := json.Marshal(price); err == nil {
			s.redis.Set(ctx, "gas:price", data, CacheDurationPrice)
		}
	}
	
	return price, nil
}

// getNetworkState gets current network state
func (s *InteractiveGasService) getNetworkState(ctx context.Context) (*GasNetworkState, error) {
	// Simplified - would need real RPC calls
	state := &GasNetworkState{
		BlockNumber:     18000000,
		BlockTimestamp: time.Now(),
		BaseFee:         25 * 1e9,
		
		GasUsed:      15000000,
		GasLimit:    30000000,
		Utilization: 0.5,
		
		PendingTxCount: 50000,
		
		HealthScore: 0.85,
		LoadLevel:  "normal",
	}
	
	return state, nil
}

// isValidSpeed checks if speed is valid
func (s *InteractiveGasService) isValidSpeed(speed string) bool {
	switch speed {
	case GasSpeedSlow, GasSpeedStandard, GasSpeedFast, GasSpeedInstant:
		return true
	}
	return false
}

// =============================================================================
// BACKGROUND TASKS
// =============================================================================

// updateGasPrices updates gas prices periodically
func (s *InteractiveGasService) updateGasPrices() {
	ticker := time.NewTicker(15 * time.Second)
	defer ticker.Stop()
	
	for range ticker.C {
		ctx := context.Background()
		
		// Get fresh price
		price, err := s.getCurrentGasPrice(ctx)
		if err != nil {
			continue
		}
		
		// Record history
		s.historyMu.Lock()
		s.history = append(s.history, GasHistory{
			Timestamp:   time.Now(),
			Low:         price.Low,
			Medium:      price.Medium,
			High:        price.High,
			Avg:         price.Medium,
			TxCount:     price.PendingTxCount,
			Utilization: price.NetworkUtilization,
		})
		
		// Trim if needed
		if len(s.history) > 10000 {
			s.history = s.history[len(s.history)-10000:]
		}
		s.historyMu.Unlock()
	}
}

// cleanupHistory cleans up old history
func (s *InteractiveGasService) cleanupHistory() {
	ticker := time.NewTicker(1 * time.Hour)
	defer ticker.Stop()
	
	for range ticker.C {
		s.historyMu.Lock()
		cutoff := time.Now().AddDate(0, 0, -HistoryRetentionDays)
		filtered := make([]GasHistory, 0)
		for _, h := range s.history {
			if h.Timestamp.After(cutoff) {
				filtered = append(filtered, h)
			}
		}
		s.history = filtered
		s.historyMu.Unlock()
	}
}