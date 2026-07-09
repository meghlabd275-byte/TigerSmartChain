package main

import (
	"context"
	"log"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/tigersmartchain/go_services/internal/verifier"
)

func main() {
	log.Println("Starting Contract Verification Service...")

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	config := verifier.LoadConfig()
	v := verifier.NewContractVerifier(config)

	go func() {
		if err := v.StartWorker(ctx); err != nil {
			log.Printf("Verification worker error: %v", err)
		}
	}()

	go func() {
		if err := v.StartAPIServer(ctx); err != nil {
			log.Printf("API server error: %v", err)
		}
	}()

	log.Println("Contract Verification Service started")

	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit

	log.Println("Shutting down...")
	cancel()
	time.Sleep(2 * time.Second)
	log.Println("Service stopped")
}
