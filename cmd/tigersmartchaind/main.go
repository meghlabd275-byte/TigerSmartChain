// Package main implements the TigerSmartChain node executable.
package main

import (
	"fmt"
	"os"

	"github.com/tigersmartchain/tigersmartchain/cmd/tigersmartchaind/cmd"
)

func main() {
	// Run the CLI application
	if err := cmd.Execute(); err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}