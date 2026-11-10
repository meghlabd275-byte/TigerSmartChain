// Package cli provides command-line tools for TigerScan
package cli

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"math/big"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/ethereum/go-ethereum"
	"github.com/ethereum/go-ethereum/accounts/abi"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/ethereum/go-ethereum/rpc"
	"github.com/spf13/cobra"
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
	rpc    *rpc.Client
	ctx    context.Context
}

// NewCLI creates a new CLI instance
func NewCLI(config *Config) (*CLI, error) {
	client, err := ethclient.Dial(config.RPCURL)
	if err != nil {
		return nil, err
	}

	rpcClient, err := rpc.DialContext(context.Background(), config.RPCURL)
	if err != nil {
		client.Close()
		return nil, err
	}

	return &CLI{
		config: config,
		client: client,
		rpc:    rpcClient,
		ctx:    context.Background(),
	}, nil
}

// close releases the RPC connections held by the CLI.
func (c *CLI) close() {
	if c.client != nil {
		c.client.Close()
	}
	if c.rpc != nil {
		c.rpc.Close()
	}
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
		defer cli.close()
		defer cli.rpc.Close()
		
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
		defer cli.close()
		defer cli.rpc.Close()
		
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
		defer cli.close()
		defer cli.rpc.Close()
		
		address := common.HexToAddress(args[0])
		balance, err := cli.client.BalanceAt(cmd.Context(), address, nil)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		
		ether := new(big.Float).Quo(new(big.Float).SetInt(balance), big.NewFloat(1e18))
		fmt.Printf("Address: %s\n", address.Hex())
		fmt.Printf("Balance: %s wei\n", balance.String())
		fmt.Printf("Balance: %s TSC\n", ether.Text('f', 18))
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
		defer cli.close()
		defer cli.rpc.Close()
		
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
		defer cli.close()
		defer cli.rpc.Close()
		
		address := args[0]
		tokenID := ""
		if len(args) > 1 {
			tokenID = args[1]
		}
		nft, err := cli.getNFT(address, tokenID)
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
		defer cli.close()
		defer cli.rpc.Close()
		
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
		defer cli.close()
		defer cli.rpc.Close()
		
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
		defer cli.close()
		defer cli.rpc.Close()
		
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
		defer cli.close()
		defer cli.rpc.Close()
		
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
		defer cli.close()
		defer cli.rpc.Close()
		
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
		defer cli.close()
		defer cli.rpc.Close()
		
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
		defer cli.close()
		defer cli.rpc.Close()
		
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
		defer cli.close()
		defer cli.rpc.Close()
		
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
		block, err := c.client.BlockByNumber(c.ctx, new(big.Int).SetUint64(num))
		if err != nil {
			return nil, err
		}
		return block, nil
	}
	
	// Try as hash
	hash := common.HexToHash(input)
	block, err := c.client.BlockByHash(c.ctx, hash)
	if err != nil {
		return nil, err
	}
	return block, nil
}

// apiGet performs a GET against the explorer REST API and decodes the JSON
// response. The base URL comes from the --api flag.
func (c *CLI) apiGet(path string, out interface{}) error {
	if strings.TrimSpace(c.config.APIBaseURL) == "" {
		return fmt.Errorf("API base URL is not configured (use --api)")
	}
	target, err := url.Parse(strings.TrimRight(c.config.APIBaseURL, "/") + path)
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(c.ctx, http.MethodGet, target.String(), nil)
	if err != nil {
		return err
	}
	if c.config.APIKey != "" {
		req.Header.Set("X-API-Key", c.config.APIKey)
	}
	resp, err := (&http.Client{Timeout: 30 * time.Second}).Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(io.LimitReader(resp.Body, 8<<20))
	if err != nil {
		return err
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("API %s returned %d: %s", path, resp.StatusCode, strings.TrimSpace(string(body)))
	}
	return json.Unmarshal(body, out)
}

// callContract performs an eth_call for a Solidity function signature and its
// already ABI-encoded arguments, returning the raw return data.
func (c *CLI) callContract(to common.Address, signature string, args ...[]byte) ([]byte, error) {
	data := make([]byte, 0, 4+32*len(args))
	data = append(data, crypto.Keccak256([]byte(signature))[:4]...)
	for _, arg := range args {
		data = append(data, common.LeftPadBytes(arg, 32)...)
	}
	return c.client.CallContract(c.ctx, ethereum.CallMsg{To: &to, Data: data}, nil)
}

func decodeUint(raw []byte) *big.Int {
	if len(raw) < 32 {
		return new(big.Int)
	}
	return new(big.Int).SetBytes(raw[:32])
}

func decodeString(raw []byte) (string, error) {
	stringTy, err := abi.NewType("string", "", nil)
	if err != nil {
		return "", err
	}
	values, err := abi.Arguments{{Type: stringTy}}.Unpack(raw)
	if err != nil {
		return "", err
	}
	s, _ := values[0].(string)
	return s, nil
}

func (c *CLI) getToken(address string) (interface{}, error) {
	addr := common.HexToAddress(address)
	out := map[string]interface{}{"address": addr.Hex()}

	read := func(signature string, args ...[]byte) ([]byte, error) {
		return c.callContract(addr, signature, args...)
	}

	raw, err := read("name()")
	if err != nil {
		return nil, fmt.Errorf("name(): %w", err)
	}
	if name, err := decodeString(raw); err == nil {
		out["name"] = name
	}
	if raw, err = read("symbol()"); err == nil {
		if symbol, err := decodeString(raw); err == nil {
			out["symbol"] = symbol
		}
	}
	if raw, err = read("decimals()"); err == nil {
		out["decimals"] = decodeUint(raw).String()
	}
	if raw, err = read("totalSupply()"); err == nil {
		out["totalSupply"] = decodeUint(raw).String()
	}
	return out, nil
}

func (c *CLI) getNFT(address, tokenID string) (interface{}, error) {
	addr := common.HexToAddress(address)
	out := map[string]interface{}{"address": addr.Hex()}

	if raw, err := c.callContract(addr, "name()"); err == nil {
		if name, err := decodeString(raw); err == nil {
			out["name"] = name
		}
	}
	if raw, err := c.callContract(addr, "symbol()"); err == nil {
		if symbol, err := decodeString(raw); err == nil {
			out["symbol"] = symbol
		}
	}
	if raw, err := c.callContract(addr, "totalSupply()"); err == nil {
		out["totalSupply"] = decodeUint(raw).String()
	}

	if tokenID == "" {
		return out, nil
	}

	id, ok := new(big.Int).SetString(strings.TrimSpace(tokenID), 10)
	if !ok {
		return nil, fmt.Errorf("invalid token id %q", tokenID)
	}
	out["tokenId"] = id.String()

	raw, err := c.callContract(addr, "ownerOf(uint256)", id.Bytes())
	if err != nil {
		return nil, fmt.Errorf("ownerOf(%s): %w", tokenID, err)
	}
	if len(raw) >= 32 {
		out["owner"] = common.BytesToAddress(raw[len(raw)-32:]).Hex()
	}
	if raw, err := c.callContract(addr, "tokenURI(uint256)", id.Bytes()); err == nil {
		if uri, err := decodeString(raw); err == nil {
			out["tokenURI"] = uri
		}
	}
	return out, nil
}

func (c *CLI) search(query string) (interface{}, error) {
	var out interface{}
	if err := c.apiGet("/api/v1/search?q="+url.QueryEscape(query), &out); err != nil {
		return nil, err
	}
	return out, nil
}

func (c *CLI) getStats() (interface{}, error) {
	var out interface{}
	if err := c.apiGet("/api/v1/stats/network", &out); err != nil {
		return nil, err
	}
	return out, nil
}

func (c *CLI) getPrice(address string) (interface{}, error) {
	if strings.TrimSpace(address) == "" {
		return nil, fmt.Errorf("token address required")
	}
	var out interface{}
	if err := c.apiGet("/api/v1/tokens/"+url.PathEscape(address)+"/price", &out); err != nil {
		return nil, err
	}
	return out, nil
}

// getEvents returns the logs emitted by a contract between two blocks.
func (c *CLI) getEvents(address string, fromBlock, toBlock int64) (interface{}, error) {
	addr := common.HexToAddress(address)
	query := ethereum.FilterQuery{Addresses: []common.Address{addr}}
	if fromBlock > 0 {
		query.FromBlock = big.NewInt(fromBlock)
	}
	if toBlock > 0 {
		query.ToBlock = big.NewInt(toBlock)
	}
	return c.client.FilterLogs(c.ctx, query)
}

func (c *CLI) getHolders(address string, limit int) (interface{}, error) {
	var out interface{}
	path := fmt.Sprintf("/api/v1/tokens/%s/holders?limit=%d", url.PathEscape(address), limit)
	if err := c.apiGet(path, &out); err != nil {
		return nil, err
	}
	return out, nil
}

func (c *CLI) getTransfers(address string, limit int) (interface{}, error) {
	var out interface{}
	path := fmt.Sprintf("/api/v1/tokens/%s/transfers?limit=%d", url.PathEscape(address), limit)
	if err := c.apiGet(path, &out); err != nil {
		return nil, err
	}
	return out, nil
}

// getGas derives slow/standard/fast fee levels from the pending block's base
// fee and the node's suggested priority fee.
func (c *CLI) getGas() (interface{}, error) {
	ctx := c.ctx

	header, err := c.client.HeaderByNumber(ctx, nil)
	if err != nil {
		return nil, err
	}

	if header.BaseFee == nil {
		price, err := c.client.SuggestGasPrice(ctx)
		if err != nil {
			return nil, err
		}
		return map[string]interface{}{
			"slow":     price.String(),
			"standard": price.String(),
			"fast":     price.String(),
			"unit":     "wei",
		}, nil
	}

	tip, err := c.client.SuggestGasTipCap(ctx)
	if err != nil {
		return nil, err
	}

	halfTip := new(big.Int).Div(tip, big.NewInt(2))
	slow := new(big.Int).Add(header.BaseFee, halfTip)
	standard := new(big.Int).Add(header.BaseFee, tip)
	fast := new(big.Int).Add(header.BaseFee, new(big.Int).Mul(tip, big.NewInt(2)))

	return map[string]interface{}{
		"baseFee":     header.BaseFee.String(),
		"priorityFee": tip.String(),
		"slow":        slow.String(),
		"standard":    standard.String(),
		"fast":        fast.String(),
		"unit":        "wei",
	}, nil
}

func (c *CLI) rpcCall(method string, params []interface{}) (interface{}, error) {
	var result interface{}
	if err := c.rpc.CallContext(c.ctx, &result, method, params...); err != nil {
		return nil, err
	}
	return result, nil
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