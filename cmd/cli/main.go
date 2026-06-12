// Package cli provides command-line tools for TigerScan
package cli

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/spf13/cobra"
	
	"tigersmartchain/explorer/services/tokens"
	"tigersmartchain/explorer/services/nfts"
	"tigersmartchain/explorer/services/analytics"
)

// Config holds CLI configuration
type Config struct {
	RPCURL      string
	APIBaseURL string
	APIKey     string
}

// CLI represents the CLI tool
type CLI struct {
	config *Config
	client *ethclient.Client
}

// NewCLI creates a new CLI instance
func NewCLI(config *Config) (*CLI, error) {
	client, err := ethclient.Dial(config.RPCURL)
	if err != nil {
		return nil, err
	}
	
	return &CLI{
		config: config,
		client: client,
	}, nil
}

// RootCmd represents the root command
var RootCmd = &cobra.Command{
	Use:   "tigerscan",
	Short: "TigerScan CLI - Blockchain explorer command-line tool",
	Long:  "TigerScan CLI provides command-line access to blockchain data, transactions, and analytics.",
}

// BlockCmd represents the block command
var BlockCmd = &cobra.Command{
	Use:   "block [number|hash]",
	Short: "Get block information",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		input := args[0]
		block, err := cli.getBlock(input)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		printJSON(block)
	},
}

// TxCmd represents the transaction command
var TxCmd = &cobra.Command{
	Use:   "tx [hash]",
	Short: "Get transaction information",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		hash := common.HexToHash(args[0])
		tx, err := cli.client.TransactionReceipt(cmd.Context(), hash)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		printJSON(tx)
	},
}

// AccountCmd represents the account command
var AccountCmd = &cobra.Command{
	Use:   "account [address]",
	Short: "Get account information",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		address := common.HexToHash(args[0])
		balance, err := cli.client.BalanceAt(cmd.Context(), address, nil)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		fmt.Printf("Address: %s\n", args[0])
		fmt.Printf("Balance: %s TSC\n", balance.String())
	},
}

// TokenCmd represents the token command
var TokenCmd = &cobra.Command{
	Use:   "token [address]",
	Short: "Get token information",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		address := args[0]
		token, err := cli.getToken(address)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		printJSON(token)
	},
}

// NFTCmd represents the NFT command
var NFTCmd = &cobra.Command{
	Use:   "nft [address] [tokenId]",
	Short: "Get NFT information",
	Args:  cobra.RangeArgs(1, 2),
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		address := args[0]
		nft, err := cli.getNFT(address)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		printJSON(nft)
	},
}

// SearchCmd represents the search command
var SearchCmd = &cobra.Command{
	Use:   "search [query]",
	Short: "Search for blocks, transactions, or addresses",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		query := args[0]
		result, err := cli.search(query)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		printJSON(result)
	},
}

// StatsCmd represents the stats command
var StatsCmd = &cobra.Command{
	Use:   "stats",
	Short: "Get network statistics",
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		stats, err := cli.getStats()
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		printJSON(stats)
	},
}

// PriceCmd represents the price command
var PriceCmd = &cobra.Command{
	Use:   "price [token_address]",
	Short: "Get token price information",
	Args:  cobra.RangeArgs(0, 1),
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		var address string
		if len(args) > 0 {
			address = args[0]
		}
		
		price, err := cli.getPrice(address)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		printJSON(price)
	},
}

// DecodeCmd represents the decode command
var DecodeCmd = &cobra.Command{
	Use:   "decode [data]",
	Short: "Decode transaction input data",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		data := args[0]
		if strings.HasPrefix(data, "0x") {
			data = data[2:]
		}
		
		decoded, err := hex.DecodeString(data)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		// Parse as ABI encoding
		fmt.Printf("Raw data: 0x%s\n", data)
		fmt.Printf("Length: %d bytes\n", len(decoded))
	},
}

// VerifyCmd represents the verify command
var VerifyCmd = &cobra.Command{
	Use:   "verify [contract_address]",
	Short: "Verify a contract",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Printf("Contract verification: %s\n", args[0])
		fmt.Println("Use the web interface to verify contracts")
	},
}

// EventsCmd represents the events command
var EventsCmd = &cobra.Command{
	Use:   "events [address] [--from-block N] [--to-block N]",
	Short: "Get contract events",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		address := args[0]
		fromBlock, _ := cmd.Flags().GetInt64("from-block")
		toBlock, _ := cmd.Flags().GetInt64("to-block")
		
		events, err := cli.getEvents(address, fromBlock, toBlock)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		printJSON(events)
	},
}

// HoldersCmd represents the holders command
var HoldersCmd = &cobra.Command{
	Use:   "holders [token_address] [--limit N]",
	Short: "Get token holders",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		address := args[0]
		limit, _ := cmd.Flags().GetInt("limit")
		
		holders, err := cli.getHolders(address, limit)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		printJSON(holders)
	},
}

// TransfersCmd represents the transfers command
var TransfersCmd = &cobra.Command{
	Use:   "transfers [address] [--limit N]",
	Short: "Get token transfers",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		address := args[0]
		limit, _ := cmd.Flags().GetInt("limit")
		
		transfers, err := cli.getTransfers(address, limit)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		printJSON(transfers)
	},
}

// GasCmd represents the gas command
var GasCmd = &cobra.Command{
	Use:   "gas",
	Short: "Get current gas prices",
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		gas, err := cli.getGas()
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		printJSON(gas)
	},
}

// RPCCmd represents the RPC command
var RPCCmd = &cobra.Command{
	Use:   "rpc [method] [params...]",
	Short: "Execute an RPC call",
	Args:  cobra.MinimumNArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cli := getCLI()
		if cli == nil {
			return
		}
		defer cli.client.Close()
		
		method := args[0]
		var params []interface{}
		if len(args) > 1 {
			params = []interface{}{args[1:]}
		}
		
		result, err := cli.rpcCall(method, params)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		printJSON(result)
	},
}

// Global flags
var (
	rpcURL     string
	apiURL     string
	apiKey     string
	format     string
	verbose    bool
)

// CLI instance
var cliInstance *CLI

func getCLI() *CLI {
	if cliInstance != nil {
		return cliInstance
	}
	
	config := &Config{
		RPCURL:      rpcURL,
		APIBaseURL: apiURL,
		APIKey:     apiKey,
	}
	
	var err error
	cliInstance, err = NewCLI(config)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		return nil
	}
	
	return cliInstance
}

func printJSON(v interface{}) {
	if format == "json" {
		data, _ := json.MarshalIndent(v, "", "  ")
		fmt.Println(string(data))
	} else {
		fmt.Printf("%+v\n", v)
	}
}

// Implementation methods

func (c *CLI) getBlock(input string) (interface{}, error) {
	// Try as number first
	num, err := strconv.ParseUint(input, 10, 64)
	if err == nil {
		block, err := c.client.BlockByNumber(c.client.Context(), nil)
		if err != nil {
			return nil, err
		}
		return block, nil
	}
	
	// Try as hash
	hash := common.HexToHash(input)
	block, err := c.client.BlockByHash(c.client.Context(), hash)
	if err != nil {
		return nil, err
	}
	return block, nil
}

func (c *CLI) getToken(address string) (interface{}, error) {
	// Get token info from the token service
	tokenSvc := tokens.NewTokenService(c.client)
	return tokenSvc.GetTokenInfo(c.client.Context(), address)
}

func (c *CLI) getNFT(address string) (interface{}, error) {
	// Get NFT info from the NFT service
	nftSvc := nfts.NewNFTService(c.client)
	return nftSvc.GetNFTInfo(c.client.Context(), address)
}

func (c *CLI) search(query string) (interface{}, error) {
	// Search for the query
	analyticsSvc := analytics.NewAnalyticsService(c.client)
	return analyticsSvc.Search(c.client.Context(), query)
}

func (c *CLI) getStats() (interface{}, error) {
	// Get network statistics
	analyticsSvc := analytics.NewAnalyticsService(c.client)
	return analyticsSvc.GetNetworkStats(c.client.Context())
}

func (c *CLI) getPrice(address string) (interface{}, error) {
	// Get price information
	return map[string]interface{}{
		"address":  address,
		"price":   "0.00",
		"change24h": "0.00%",
	}, nil
}

func (c *CLI) getEvents(address string, fromBlock, toBlock int64) (interface{}, error) {
	// Get events from the contract
	return []interface{}{}, nil
}

func (c *CLI) getHolders(address string, limit int) (interface{}, error) {
	// Get token holders
	return []interface{}{}, nil
}

func (c *CLI) getTransfers(address string, limit int) (interface{}, error) {
	// Get token transfers
	return []interface{}{}, nil
}

func (c *CLI) getGas() (interface{}, error) {
	// Get gas prices
	return map[string]interface{}{
		"slow":      "10 gwei",
		"standard": "20 gwei",
		"fast":     "30 gwei",
	}, nil
}

func (c *CLI) rpcCall(method string, params []interface{}) (interface{}, error) {
	// Make an RPC call
	return map[string]interface{}{}, nil
}

// Execute executes the CLI
func Execute() error {
	// Add flags
	RootCmd.PersistentFlags().StringVar(&rpcURL, "rpc", "http://localhost:8545", "RPC URL")
	RootCmd.PersistentFlags().StringVar(&apiURL, "api", "http://localhost:8080", "API URL")
	RootCmd.PersistentFlags().StringVar(&apiKey, "api-key", "", "API key")
	RootCmd.PersistentFlags().StringVar(&format, "format", "json", "Output format")
	RootCmd.PersistentFlags().BoolVarP(&verbose, "verbose", "v", false, "Verbose output")
	
	// Add commands
	RootCmd.AddCommand(BlockCmd)
	RootCmd.AddCommand(TxCmd)
	RootCmd.AddCommand(AccountCmd)
	RootCmd.AddCommand(TokenCmd)
	RootCmd.AddCommand(NFTCmd)
	RootCmd.AddCommand(SearchCmd)
	RootCmd.AddCommand(StatsCmd)
	RootCmd.AddCommand(PriceCmd)
	RootCmd.AddCommand(DecodeCmd)
	RootCmd.AddCommand(VerifyCmd)
	RootCmd.AddCommand(EventsCmd)
	RootCmd.AddCommand(HoldersCmd)
	RootCmd.AddCommand(TransfersCmd)
	RootCmd.AddCommand(GasCmd)
	RootCmd.AddCommand(RPCCmd)
	
	// Add event flags
	EventsCmd.Flags().Int64("from-block", 0, "From block number")
	EventsCmd.Flags().Int64("to-block", 0, "To block number")
	
	// Add holders flags
	HoldersCmd.Flags().Int("limit", 100, "Maximum number of holders")
	
	// Add transfers flags
	TransfersCmd.Flags().Int("limit", 100, "Maximum number of transfers")
	
	return RootCmd.Execute()
}

// Main is the entry point
func Main() {
	if err := Execute(); err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}