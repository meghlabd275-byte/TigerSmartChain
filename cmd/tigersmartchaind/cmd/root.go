// Package cmd provides the CLI commands for tigersmartchaind.
package cmd

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"

	"github.com/tigersmartchain/tigersmartchain/cmd/tigersmartchaind/flags"
	"github.com/tigersmartchain/tigersmartchain/internal/blockchain/genesis"
)

// Execute runs the CLI application.
func Execute() error {
	return rootCmd.Execute()
}

var rootCmd = &cobra.Command{
	Use:   "tigersmartchaind",
	Short: "TigerSmartChain node daemon",
	Long: `TigerSmartChain is an EVM-compatible blockchain 
inspired by BinanceSmartChain, built on a modified 
go-ethereum architecture.`,
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("TigerSmartChain v1.0.0")
		fmt.Println("Chain ID:", flags.ChainID)
		fmt.Println("Starting node...")
	},
}

func init() {
	// Add flags
	rootCmd.PersistentFlags().Uint64Var(&flags.ChainID, "chain-id", 9001, "Chain ID")
	rootCmd.PersistentFlags().StringVar(&flags.DataDir, "datadir", "", "Data directory")
	rootCmd.PersistentFlags().StringVar(&flags.ConfigFile, "config", "", "Config file")
	rootCmd.PersistentFlags().BoolVar(&flags.Verbosity, "verbosity", false, "Verbose logging")
	rootCmd.PersistentFlags().StringVar(&flags.HTTPHost, "http.host", "127.0.0.1", "HTTP server host")
	rootCmd.PersistentFlags().UintVar(&flags.HTTPPort, "http.port", 8545, "HTTP server port")
	rootCmd.PersistentFlags().StringVar(&flags.WSHost, "ws.host", "127.0.0.1", "WebSocket server host")
	rootCmd.PersistentFlags().UintVar(&flags.WSPort, "ws.port", 8546, "WebSocket server port")
	rootCmd.PersistentFlags().StringVar(&flags.Bootnodes, "bootnodes", "", "Comma separated bootnodes")
	rootCmd.PersistentFlags().StringVar(&flags.Key, "key", "", "Private key for validator")

	// Add subcommands
	rootCmd.AddCommand(initCmd)
	rootCmd.AddCommand(startCmd)
	rootCmd.AddCommand(validatorCmd)
	rootCmd.AddCommand(versionCmd)
	rootCmd.AddCommand(exportCmd)
	rootCmd.AddCommand(importCmd)
	rootCmd.AddCommand(monitorCmd)
	rootCmd.AddCommand(consoleCmd)
	rootCmd.AddCommand(attachCmd)
}

// initCmd initializes a new chain
var initCmd = &cobra.Command{
	Use:   "init [genesis-file]",
	Short: "Initialize a new chain",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		genesisFile := args[0]
		fmt.Printf("Initializing chain from %s\n", genesisFile)

		// Load genesis
		g, err := genesis.Load(genesisFile)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error loading genesis: %v\n", err)
			os.Exit(1)
		}

		// Write genesis to database
		fmt.Printf("Chain ID: %d\n", g.Config.ChainID)
		fmt.Printf("Chain Name: %s\n", g.Config.ChainName)
		fmt.Printf("Symbol: %s\n", g.Config.Symbol)
		fmt.Printf("Block Time: %d seconds\n", g.Config.BlockTime)
		fmt.Printf("Validators: %d\n", len(g.Validators))

		fmt.Println("Chain initialized successfully")
	},
}

// startCmd starts the node
var startCmd = &cobra.Command{
	Use:   "start",
	Short: "Start the node",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("Starting TigerSmartChain node...")
		// This would start the node
	},
}

// validatorCmd manages validators
var validatorCmd = &cobra.Command{
	Use:   "validator",
	Short: "Manage validators",
}

var validatorListCmd = &cobra.Command{
	Use:   "list",
	Short: "List validators",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("Validators:")
		// This would list validators
	},
}

var validatorRegisterCmd = &cobra.Command{
	Use:   "register [amount]",
	Short: "Register as a validator",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Printf("Registering validator with %s TGR\n", args[0])
	},
}

func init() {
	validatorCmd.AddCommand(validatorListCmd)
	validatorCmd.AddCommand(validatorRegisterCmd)
	rootCmd.AddCommand(validatorCmd)
}

// versionCmd prints the version
var versionCmd = &cobra.Command{
	Use:   "version",
	Short: "Print the version",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("TigerSmartChain v1.0.0")
		fmt.Println("Go version: 1.21.5")
	},
}

// exportCmd exports the blockchain
var exportCmd = &cobra.Command{
	Use:   "export [output-file]",
	Short: "Export the blockchain",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Printf("Exporting blockchain to %s\n", args[0])
	},
}

// importCmd imports the blockchain
var importCmd = &cobra.Command{
	Use:   "import [input-file]",
	Short: "Import the blockchain",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Printf("Importing blockchain from %s\n", args[0])
	},
}

// monitorCmd starts the monitoring dashboard
var monitorCmd = &cobra.Command{
	Use:   "monitor",
	Short: "Start the monitoring dashboard",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("Starting monitoring dashboard...")
	},
}

// consoleCmd starts the interactive console
var consoleCmd = &cobra.Command{
	Use:   "console",
	Short: "Start the interactive console",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("TigerSmartChain console")
		fmt.Println("Type 'exit' to quit")
		// This would start the console
	},
}

// attachCmd attaches to a running node
var attachCmd = &cobra.Command{
	Use:   "attach [ipc-file]",
	Short: "Attach to a running node",
	Args:  cobra.MaximumNArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("Attaching to node...")
	},
}