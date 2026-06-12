// Package main provides CLI commands for TigerSmartChain Explorer
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/tigersmartchain/tigersmartchain/explorer/services/sdk"
)

// CLI represents the command line interface
type CLI struct {
	config *Config
	sdk    *sdk.SDKClient
}

// Config represents CLI configuration
type Config struct {
	APIKey   string
	BaseURL  string
	ChainID  uint64
	Output   string
	Verbose  bool
}

// Commands
var (
	commands = map[string]func(*CLI, []string) error{
		"block":        cmdBlock,
		"tx":           cmdTransaction,
		"account":      cmdAccount,
		"token":        cmdToken,
		"search":       cmdSearch,
		"logs":         cmdLogs,
		"balance":      cmdBalance,
		"transfers":    cmdTransfers,
		"contract":     cmdContract,
		"read":         cmdReadContract,
		"write":        cmdWriteContract,
		"gas":          cmdGasPrice,
		"blocknumber":  cmdBlockNumber,
		"nfts":         cmdNFTs,
		"internal":      cmdInternalTx,
		"receipt":      cmdReceipt,
	}
)

	cfg = &Config{}
)

func main() {
	// Parse global flags
	flag.StringVar(&cfg.APIKey, "api-key", "", "API key for authentication")
	flag.StringVar(&cfg.BaseURL, "url", "https://api.tigersmartchain.io", "API base URL")
	flag.StringVar(&cfg.ChainID, "chain", "1", "Chain ID (1=ETH, 56=BSC, 137=Polygon)")
	flag.StringVar(&cfg.Output, "output", "json", "Output format (json, text)")
	flag.BoolVar(&cfg.Verbose, "v", false, "Verbose output")
	flag.Parse()

	// Get command
	args := flag.Args()
	if len(args) == 0 {
		printUsage()
		os.Exit(1)
	}

	command := args[0]
	args = args[1:]

	// Initialize SDK client
	sdkClient := sdk.InitSDK(cfg.APIKey, cfg.BaseID, 1) // Would use cfg.ChainID

	cli := &CLI{
		config: cfg,
		sdk:    sdkClient,
	}

	// Execute command
	cmd, ok := commands[command]
	if !ok {
		fmt.Printf("Unknown command: %s\n", command)
		printUsage()
		os.Exit(1)
	}

	if err := cmd(cli, args); err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}

func printUsage() {
	fmt.Println(`TigerSmartChain Explorer CLI

Usage: tigersmartchain [global options] <command> [command options]

Global Options:
  --api-key string    API key for authentication
  --url string        API base URL (default: https://api.tigersmartchain.io)
  --chain string     Chain ID (1=ETH, 56=BSC, 137=Polygon)
  --output string    Output format (json, text)
  -v                 Verbose output

Commands:
  block <number|hash>        Get block information
  tx <hash>                 Get transaction details
  account <address>          Get account information
  token <address>           Get token information
  search <query>             Search blocks, transactions, addresses
  logs [options]            Get logs
  balance <address>          Get account balance
  transfers <address>       Get token transfers
  contract <address>        Get contract information
  read <address> <method>   Read from contract
  write <address> <method>  Write to contract
  gas                       Get current gas prices
  blocknumber                Get current block number
  nfts <address>            Get NFTs for address
  internal <tx>              Get internal transactions
  receipt <tx>              Get transaction receipt

Examples:
  tigersmartchain block 18000000
  tigersmartchain tx 0x1234...
  tigersmartchain account 0xabcd...
  tigersmartchain token 0xdef... --output text
  tigersmartchain search "uniswap"
  tigersmartchain balance 0x1234...
  tigersmartchain gas
  tigersmartchain blocknumber`)
}

// Command implementations

func cmdBlock(cli *CLI, args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("block number or hash required")
	}

	blockNumOrHash := args[0]

	block, err := cli.sdk.GetBlock(blockNumOrHash)
	if err != nil {
		return fmt.Errorf("failed to get block: %w", err)
	}

	return printOutput(cli.config.Output, block)
}

func cmdTransaction(cli *CLI, args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("transaction hash required")
	}

	txHash := args[0]

	tx, err := cli.sdk.GetTransaction(txHash)
	if err != nil {
		return fmt.Errorf("failed to get transaction: %w", err)
	}

	return printOutput(cli.config.Output, tx)
}

func cmdAccount(cli *CLI, args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("account address required")
	}

	address := args[0]

	account, err := cli.sdk.GetAccount(address)
	if err != nil {
		return fmt.Errorf("failed to get account: %w", err)
	}

	return printOutput(cli.config.Output, account)
}

func cmdToken(cli *CLI, args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("token address required")
	}

	address := args[0]

	token, err := cli.sdk.GetToken(address)
	if err != nil {
		return fmt.Errorf("failed to get token: %w", err)
	}

	return printOutput(cli.config.Output, token)
}

func cmdSearch(cli *CLI, args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("search query required")
	}

	query := args[0]

	result, err := cli.sdk.Search(query)
	if err != nil {
		return fmt.Errorf("search failed: %w", err)
	}

	return printOutput(cli.config.Output, result)
}

func cmdLogs(cli *CLI, args []string) error {
	opts := &sdk.LogQueryOpts{}

	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--address":
			if i+1 < len(args) {
				opts.Address = args[i+1]
				i++
			}
		case "--from-block":
			if i+1 < len(args) {
				fmt.Sscanf(args[i+1], "%d", &opts.FromBlock)
				i++
			}
		case "--to-block":
			if i+1 < len(args) {
				fmt.Sscanf(args[i+1], "%d", &opts.ToBlock)
				i++
			}
		}
	}

	logs, err := cli.sdk.GetLogs(opts)
	if err != nil {
		return fmt.Errorf("failed to get logs: %w", err)
	}

	return printOutput(cli.config.Output, logs)
}

func cmdBalance(cli *CLI, args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("address required")
	}

	address := args[0]

	account, err := cli.sdk.GetAccount(address)
	if err != nil {
		return fmt.Errorf("failed to get balance: %w", err)
	}

	fmt.Printf("Balance: %s\n", account.Balance)
	return nil
}

func cmdTransfers(cli *CLI, args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("address required")
	}

	address := args[0]

	transfers, err := cli.sdk.GetTokenTransfers(address, nil)
	if err != nil {
		return fmt.Errorf("failed to get transfers: %w", err)
	}

	return printOutput(cli.config.Output, transfers)
}

func cmdContract(cli *CLI, args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("contract address required")
	}

	address := args[0]

	token, err := cli.sdk.GetToken(address)
	if err != nil {
		return fmt.Errorf("failed to get contract: %w", err)
	}

	return printOutput(cli.config.Output, token)
}

func cmdReadContract(cli *CLI, args []string) error {
	if len(args) < 2 {
		return fmt.Errorf("contract address and method required")
	}

	address := args[0]
	method := args[1]

	result, err := cli.sdk.ReadContract(address, method, nil)
	if err != nil {
		return fmt.Errorf("failed to read contract: %w", err)
	}

	return printOutput(cli.config.Output, result)
}

func cmdWriteContract(cli *CLI, args []string) error {
	if len(args) < 2 {
		return fmt.Errorf("contract address and method required")
	}

	address := args[0]
	method := args[1]

	result, err := cli.sdk.WriteContract(address, method, nil, nil)
	if err != nil {
		return fmt.Errorf("failed to write contract: %w", err)
	}

	fmt.Printf("Transaction hash: %s\n", result)
	return nil
}

func cmdGasPrice(cli *CLI, args []string) error {
	gasPrice, err := cli.sdk.GetGasPrice()
	if err != nil {
		return fmt.Errorf("failed to get gas price: %w", err)
	}

	return printOutput(cli.config.Output, gasPrice)
}

func cmdBlockNumber(cli *CLI, args []string) error {
	blockNum, err := cli.sdk.GetBlockNumber()
	if err != nil {
		return fmt.Errorf("failed to get block number: %w", err)
	}

	fmt.Printf("Current block number: %d\n", blockNum)
	return nil
}

func cmdNFTs(cli *CLI, args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("address required")
	}

	address := args[0]

	nfts, err := cli.sdk.GetNFTs(address, nil)
	if err != nil {
		return fmt.Errorf("failed to get NFTs: %w", err)
	}

	return printOutput(cli.config.Output, nfts)
}

func cmdInternalTx(cli *CLI, args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("transaction hash required")
	}

	txHash := args[0]

	txs, err := cli.sdk.GetInternalTransactions(txHash)
	if err != nil {
		return fmt.Errorf("failed to get internal transactions: %w", err)
	}

	return printOutput(cli.config.Output, txs)
}

func cmdReceipt(cli *CLI, args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("transaction hash required")
	}

	txHash := args[0]

	receipt, err := cli.sdk.GetTxReceipt(txHash)
	if err != nil {
		return fmt.Errorf("failed to get receipt: %w", err)
	}

	return printOutput(cli.config.Output, receipt)
}

func printOutput(format string, v interface{}) error {
	switch format {
	case "json":
		data, err := json.MarshalIndent(v, "", "  ")
		if err != nil {
			return err
		}
		fmt.Println(string(data))
	case "text":
		printText(v)
	default:
		return fmt.Errorf("unknown output format: %s", format)
	}
	return nil
}

func printText(v interface{}) {
	switch t := v.(type) {
	case *sdk.Block:
		fmt.Printf("Block #%d\n", t.Number)
		fmt.Printf("  Hash: %s\n", t.Hash)
		fmt.Printf("  Timestamp: %d\n", t.Timestamp)
		fmt.Printf("  Transactions: %d\n", len(t.Transactions))
	case *sdk.Transaction:
		fmt.Printf("Transaction: %s\n", t.Hash)
		fmt.Printf("  From: %s\n", t.From)
		fmt.Printf("  To: %s\n", t.To)
		fmt.Printf("  Value: %s\n", t.Value)
		fmt.Printf("  Block: %d\n", t.BlockNumber)
	case *sdk.Account:
		fmt.Printf("Address: %s\n", t.Address)
		fmt.Printf("  Balance: %s\n", t.Balance)
		fmt.Printf("  Nonce: %d\n", t.Nonce)
	case *sdk.Token:
		fmt.Printf("Token: %s (%s)\n", t.Name, t.Symbol)
		fmt.Printf("  Decimals: %d\n", t.Decimals)
		fmt.Printf("  Total Supply: %s\n", t.TotalSupply)
	default:
		fmt.Printf("%+v\n", v)
	}
}

// Chain ID mapping
var chainIDs = map[string]uint64{
	"eth":    1,
	"ethereum": 1,
	"bsc":     56,
	"binance": 56,
	"polygon": 137,
	"matic":  137,
	"avax":    43114,
	"arbitrum": 42161,
	"optimism": 10,
	"base":    8453,
}

func parseChainID(s string) uint64 {
	if id, ok := chainIDs[strings.ToLower(s)]; ok {
		return id
	}
	return 1
}

// Context with timeout
func withTimeout(timeout time.Duration) (context.Context, context.CancelFunc) {
	return context.WithTimeout(context.Background(), timeout)
}