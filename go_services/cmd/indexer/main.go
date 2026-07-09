package main

import (
	"context"
	"log"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/tigersmartchain/go_services/internal/indexer"
)

func main() {
	log.Println("Starting TigerSmartChain Indexer Service...")

	// Load configuration
	config := indexer.LoadConfig()

	// Create context with cancellation
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Create indexer
	idx := indexer.NewIndexer(config)

	// Start block indexer
	go func() {
		if err := idx.StartBlockIndexer(ctx); err != nil {
			log.Printf("Block indexer error: %v", err)
		}
	}()

	// Start transaction indexer
	go func() {
		if err := idx.StartTransactionIndexer(ctx); err != nil {
			log.Printf("Transaction indexer error: %v", err)
		}
	}()

	// Start token indexer
	go func() {
		if err := idx.StartTokenIndexer(ctx); err != nil {
			log.Printf("Token indexer error: %v", err)
		}
	}()

	// Start NFT indexer
	go func() {
		if err := idx.StartNFTIndexer(ctx); err != nil {
			log.Printf("NFT indexer error: %v", err)
		}
	}()

	// Start internal transaction indexer
	go func() {
		if err := idx.StartInternalTxIndexer(ctx); err != nil {
			log.Printf("Internal tx indexer error: %v", err)
		}
	}()

	log.Println("All indexers started successfully")

	// Wait for interrupt signal
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit

	log.Println("Shutting down indexers...")

	// Graceful shutdown
	cancel()
	time.Sleep(5 * time.Second)

	log.Println("Indexers stopped")
}
