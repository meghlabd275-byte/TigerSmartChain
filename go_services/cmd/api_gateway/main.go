package main

import (
	"context"
	"log"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/tigersmartchain/go_services/internal/gateway"
)

func main() {
	// Initialize Gin
	gin.SetMode(gin.ReleaseMode)
	router := gin.New()
	router.Use(gin.Recovery())
	router.Use(gin.Logger())

	// Load configuration
	config := gateway.LoadConfig()

	// Initialize middleware
	middleware := gateway.NewMiddleware(config)
	router.Use(middleware.CORS())
	router.Use(middleware.RateLimiter())
	router.Use(middleware.RequestLogger())

	// Initialize handlers
	handler := gateway.NewHandler(config)

	// Setup routes
	setupRoutes(router, handler)

	// Create HTTP server
	srv := &http.Server{
		Addr:           ":" + config.Port,
		Handler:        router,
		ReadTimeout:    30 * time.Second,
		WriteTimeout:   30 * time.Second,
		MaxHeaderBytes: 1 << 20,
	}

	// Start server in goroutine
	go func() {
		log.Printf("Starting API Gateway on port %s", config.Port)
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatalf("Failed to start server: %v", err)
		}
	}()

	// Wait for interrupt signal
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit
	log.Println("Shutting down server...")

	// Graceful shutdown
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := srv.Shutdown(ctx); err != nil {
		log.Fatal("Server forced to shutdown:", err)
	}

	log.Println("Server exited")
}

func setupRoutes(router *gin.Engine, h *gateway.Handler) {
	// Register all 180+ API endpoints
	h.RegisterAllRoutes(router)
}
