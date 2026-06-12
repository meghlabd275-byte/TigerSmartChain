package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"strings"
	"sync"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/go-ethereum/ethclient"
)

// =============================================================================
// GRAPH VISUALIZATION SERVICE
// =============================================================================

// GraphService handles transaction flow visualization
type GraphService struct {
	client    *ethclient.Client
	redis    *RedisClient
	neo4j    *Neo4jClient
}

// RedisClient represents a Redis connection
type RedisClient struct {
	host string
	port int
}

// Neo4jClient represents a Neo4j graph database connection
type Neo4jClient struct {
	uri      string
	username string
	password string
}

// Node represents a graph node
type Node struct {
	ID         string                 `json:"id"`
	Type       string                 `json:"type"`
	Label     string                 `json:"label"`
	Properties map[string]interface{} `json:"properties"`
	Position   *Position             `json:"position,omitempty"`
}

// Position represents node position in visualization
type Position struct {
	X float64 `json:"x"`
	Y float64 `json:"y"`
}

// Edge represents a graph edge
type Edge struct {
	ID         string                 `json:"id"`
	Source     string                 `json:"source"`
	Target     string                 `json:"target"`
	Type       string                 `json:"type"`
	Label      string                 `json:"label"`
	Properties map[string]interface{} `json:"properties"`
}

// Graph represents the complete graph
type Graph struct {
	Nodes     []Node `json:"nodes"`
	Edges     []Edge `json:"edges"`
	Stats     *GraphStats `json:"stats"`
	Generated time.Time `json:"generated"`
}

// GraphStats represents graph statistics
type GraphStats struct {
	NodeCount    int     `json:"nodeCount"`
	EdgeCount   int     `json:"edgeCount"`
	Depth       int     `json:"depth"`
	MaxValue   float64 `json:"maxValue"`
	CentralNode string  `json:"centralNode"`
}

// TransactionFlow represents a transaction flow path
type TransactionFlow struct {
	From       string   `json:"from"`
	To         string   `json:"to"`
	Value      string   `json:"value"`
	Transactions []string `json:"transactions"`
	Timestamp  int64    `json:"timestamp"`
}

// NewGraphService creates a new graph visualization service
func NewGraphService(rpcURL string) (*GraphService, error) {
	client, err := ethclient.Dial(rpcURL)
	if err != nil {
		return nil, err
	}
	
	return &GraphService{
		client: client,
	}, nil
}

// GenerateTransactionGraph generates a graph visualization for a transaction
func (s *GraphService) GenerateTransactionGraph(txHash string, depth int) (*Graph, error) {
	ctx := context.Background()
	
	// Get transaction
	tx, _, err := s.client.TransactionByHash(ctx, common.HexToHash(txHash))
	if err != nil {
		return nil, err
	}
	
	graph := &Graph{
		Nodes: []Node{},
		Edges: []Edge{},
		Stats: &GraphStats{
			Depth: depth,
		},
		Generated: time.Now(),
	}
	
	// Add initial node (from address)
	fromAddr := tx.From().Hex()
	graph.Nodes = append(graph.Nodes, Node{
		ID:     fromAddr,
		Type:   "address",
		Label:  truncateAddress(fromAddr),
		Properties: map[string]interface{}{
			"address":   fromAddr,
			"isSender":  true,
			"txCount":   0,
		},
	})
	
	// Add recipient node
	toAddr := tx.To().Hex()
	graph.Nodes = append(graph.Nodes, Node{
		ID:     toAddr,
		Type:   "address",
		Label:  truncateAddress(toAddr),
		Properties: map[string]interface{}{
			"address":    toAddr,
			"isReceiver": true,
			"txCount":    0,
		},
	})
	
	// Add edge
	graph.Edges = append(graph.Edges, Edge{
		ID:        generateEdgeID(fromAddr, toAddr),
		Source:   fromAddr,
		Target:   toAddr,
		Type:     "transaction",
		Label:    "sent " + formatValue(tx.Value()),
		Properties: map[string]interface{}{
			"hash":      txHash,
			"value":     tx.Value().String(),
			"gasPrice":  tx.GasPrice().String(),
			"timestamp": time.Now().Unix(),
		},
	})
	
	// If depth > 0, trace related transactions
	if depth > 1 {
		relatedTxs, err := s.getRelatedTransactions(ctx, fromAddr, depth-1)
		if err == nil {
			for _, relatedTx := range relatedTxs {
				graph.Nodes = append(graph.Nodes, Node{
					ID:     relatedTx.To,
					Type:   "address",
					Label:  truncateAddress(relatedTx.To),
					Properties: map[string]interface{}{
						"address": relatedTx.To,
					},
				})
				
				graph.Edges = append(graph.Edges, Edge{
					ID:        generateEdgeID(fromAddr, relatedTx.To),
					Source:   fromAddr,
					Target:   relatedTx.To,
					Type:     "transaction",
					Label:    "sent " + relatedTx.Value,
				})
			}
		}
	}
	
	// Calculate stats
	graph.Stats.NodeCount = len(graph.Nodes)
	graph.Stats.EdgeCount = len(graph.Edges)
	graph.Stats.CentralNode = fromAddr
	graph.Stats.MaxValue = 0
	
	return graph, nil
}

// GenerateTokenFlowGraph generates a graph showing token flow
func (s *GraphService) GenerateTokenFlowGraph(tokenAddress string, topHolders int) (*Graph, error) {
	graph := &Graph{
		Nodes: []Node{},
		Edges: []Edge{},
		Stats: &GraphStats{},
	}
	
	// Add token node
	graph.Nodes = append(graph.Nodes, Node{
		ID:     tokenAddress,
		Type:   "token",
		Label:  "TOKEN",
		Properties: map[string]interface{}{
			"address": tokenAddress,
		},
	})
	
	// Add holder nodes (simulated - in production, get from database)
	for i := 0; i < topHolders; i++ {
		holder := fmt.Sprintf("0x%x", i+1)
		holder = strings.Repeat("0", 40-len(holder)) + holder[2:]
		
		graph.Nodes = append(graph.Nodes, Node{
			ID:     holder,
			Type:   "holder",
			Label:  truncateAddress(holder),
			Properties: map[string]interface{}{
				"address": holder,
				"balance": fmt.Sprintf("%d000000000000000000", 1000-i*10),
			},
		})
		
		// Add edge from token to holder
		graph.Edges = append(graph.Edges, Edge{
			ID:        generateEdgeID(tokenAddress, holder),
			Source:   tokenAddress,
			Target:   holder,
			Type:     "hold",
			Label:    "holds",
		})
	}
	
	graph.Stats.NodeCount = len(graph.Nodes)
	graph.Stats.EdgeCount = len(graph.Edges)
	
	return graph, nil
}

// GenerateNFTOwnershipGraph generates NFT ownership graph
func (s *GraphService) GenerateNFTOwnershipGraph(nftAddress string) (*Graph, error) {
	graph := &Graph{
		Nodes: []Node{},
		Edges: []Edge{},
		Stats: &GraphStats{},
	}
	
	// Add NFT collection node
	graph.Nodes = append(graph.Nodes, Node{
		ID:     nftAddress,
		Type:   "nft_collection",
		Label:  "NFT Collection",
		Properties: map[string]interface{}{
			"address": nftAddress,
		},
	})
	
	// Generate sample ownership (in production, query database)
	owners := []string{
		"0x742d35Cc6634C0532925a3b844Bc9e7595f12eB7",
		"0x8Ba1f109551bD432803012645Ac136ddd64DBA26",
		"0x9971f0f1290d5A64c48E5fE5eC4a2E7fF3d9A8b8",
	}
	
	for i, owner := range owners {
		graph.Nodes = append(graph.Nodes, Node{
			ID:     owner,
			Type:   "owner",
			Label:  fmt.Sprintf("Owner %d", i+1),
			Properties: map[string]interface{}{
				"address": owner,
				"nftCount": 5 - i,
			},
		})
		
		graph.Edges = append(graph.Edges, Edge{
			ID:        generateEdgeID(nftAddress, owner),
			Source:   nftAddress,
			Target:   owner,
			Type:     "owns",
			Label:    fmt.Sprintf("owns %d NFTs", 5-i),
		})
	}
	
	graph.Stats.NodeCount = len(graph.Nodes)
	graph.Stats.EdgeCount = len(graph.Edges)
	
	return graph, nil
}

// GetAddressRelationshipGraph generates relationship graph for an address
func (s *GraphService) GetAddressRelationshipGraph(address string, maxNodes int) (*Graph, error) {
	graph := &Graph{
		Nodes: []Node{},
		Edges: []Edge{},
		Stats: &GraphStats{
			CentralNode: address,
		},
		Generated: time.Now(),
	}
	
	// Add central node
	graph.Nodes = append(graph.Nodes, Node{
		ID:     address,
		Type:   "central_address",
		Label:  truncateAddress(address),
		Properties: map[string]interface{}{
			"address":    address,
			"isCentral":  true,
		},
	})
	
	// Generate related addresses (in production, query from transactions)
	relatedAddrs := s.generateRelatedAddresses(address, maxNodes)
	
	for _, relAddr := range relatedAddrs {
		relType := determineAddressType(relAddr)
		
		graph.Nodes = append(graph.Nodes, Node{
			ID:     relAddr,
			Type:   relType,
			Label:  truncateAddress(relAddr),
			Properties: map[string]interface{}{
				"address": relAddr,
				"type":   relType,
			},
		})
		
		// Determine relationship type
		relType := "transaction"
		if isContract(relAddr) {
			relType = "contract_call"
		}
		
		graph.Edges = append(graph.Edges, Edge{
			ID:        generateEdgeID(address, relAddr),
			Source:   address,
			Target:   relAddr,
			Type:     relType,
			Label:    relType,
		})
	}
	
	graph.Stats.NodeCount = len(graph.Nodes)
	graph.Stats.EdgeCount = len(graph.Edges)
	
	return graph, nil
}

// getRelatedTransactions gets related transactions (simplified)
func (s *GraphService) getRelatedTransactions(ctx context.Context, address string, depth int) ([]TransactionFlow, error) {
	// In production, query from database
	// This is a simplified version
	return []TransactionFlow{
		{
			From:  address,
			To:    "0x742d35Cc6634C0532925a3b844Bc9e7595f12eB7",
			Value: "1000000000000000000",
			Transactions: []string{},
			Timestamp: time.Now().Unix(),
		},
	}, nil
}

// generateRelatedAddresses generates related addresses
func (s *GraphService) generateRelatedAddresses(address string, maxNodes int) []string {
	related := []string{}
	
	// Generate deterministic but varied addresses
	for i := 0; i < maxNodes && i < 20; i++ {
		hash := sha256.Sum256([]byte(address + fmt.Sprintf("%d", i)))
		addr := "0x" + hex.EncodeToString(hash[:20])
		related = append(related, addr)
	}
	
	return related
}

// Graph to SVG export
func (s *GraphService) ExportToSVG(graph *Graph) (string, error) {
	var sb strings.Builder
	
	sb.WriteString(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 800">`)
	sb.WriteString(`<style>.node { fill: #F7931A; stroke: #333; stroke-width: 2; }</style>`)
	sb.WriteString(`<style>.edge { stroke: #666; stroke-width: 1.5; fill: none; }</style>`)
	sb.WriteString(`<style>.label { font-family: Arial; font-size: 12px; fill: #333; }</style>`)
	
	// Calculate positions
	centerX, centerY := 600.0, 400.0
	radius := 250.0
	
	// Draw edges first
	for i, edge := range graph.Edges {
		angle := float64(i) * 2 * 3.14159 / float64(len(graph.Edges))
		x := centerX + radius*0.3*float64(i%3-1)
		y := centerY + radius*0.3*float64(i%3-1)
		
		x2 := centerX + radius*float64(i+1)/float64(len(graph.Edges)+1) * float64(i%2*2-1)
		y2 := centerY + radius*float64(i+1)/float64(len(graph.Edges)+1) * float64(i%3-1)
		
		sb.WriteString(fmt.Sprintf(`<line class="edge" x1="%.1f" y1="%.1f" x2="%.1f" y2="%.1f"/>`,
			x, y, x2, y2))
	}
	
	// Draw nodes
	for i, node := range graph.Nodes {
		angle := float64(i) * 2 * 3.14159 / float64(len(graph.Nodes))
		if i == 0 {
			angle = -3.14159 / 2
		}
		
		x := centerX + radius*0.7*func() float64 {
			if i == 0 { return 0 }
			return float64(i%5-2) * 0.4
		}()
		y := centerY + radius*0.7*func() float64 {
			if i == 0 { return 0 }
			return float64(i%4-1.5) * 0.4
		}()
		
		nodeColor := "#F7931A"
		if node.Type == "token" || node.Type == "nft_collection" {
			nodeColor = "#627EEA"
		} else if node.Type == "central_address" {
			nodeColor = "#E5354B"
		}
		
		sb.WriteString(fmt.Sprintf(`<circle class="node" cx="%.1f" cy="%.1f" r="%.1f" fill="%s"/>`,
			x, y, 20, nodeColor))
		sb.WriteString(fmt.Sprintf(`<text class="label" x="%.1f" y="%.1f" text-anchor="middle">%s</text>`,
			x, y+35, node.Label))
	}
	
	sb.WriteString("</svg>")
	
	return sb.String(), nil
}

// Helper functions
func truncateAddress(addr string) string {
	if len(addr) > 10 {
		return addr[:6] + "..." + addr[len(addr)-4:]
	}
	return addr
}

func generateEdgeID(source, target string) string {
	hash := sha256.Sum256([]byte(source + target))
	return hex.EncodeToString(hash[:8])
}

func formatValue(value *big.Int) string {
	eth := new(big.Float).Quo(new(big.Float).SetInt(value), big.NewFloat(1e18))
	return fmt.Sprintf("%.4f ETH", eth)
}

func determineAddressType(addr string) string {
	// Simple heuristic
	if strings.HasPrefix(addr, "0x0000") {
		return "contract"
	}
	return "address"
}

func isContract(addr string) bool {
	// In production, check code at address
	return strings.HasPrefix(addr, "0x000")
}

// Export to D3.js format
func (s *GraphService) ExportToD3(graph *Graph) (string, error) {
	data := map[string]interface{}{
		"nodes": graph.Nodes,
		"links": graph.Edges,
	}
	
	jsonData, err := json.Marshal(data)
	if err != nil {
		return "", err
	}
	
	return string(jsonData), nil
}

// GetGraphMetrics calculates graph metrics
func (s *GraphService) GetGraphMetrics(graph *Graph) map[string]interface{} {
	// Calculate degree centrality
	nodeDegrees := make(map[string]int)
	for _, edge := range graph.Edges {
		nodeDegrees[edge.Source]++
		nodeDegrees[edge.Target]++
	}
	
	// Find most connected node
	centralNode := ""
	maxDegree := 0
	for node, degree := range nodeDegrees {
		if degree > maxDegree {
			maxDegree = degree
			centralNode = node
		}
	}
	
	return map[string]interface{}{
		"nodeCount":         len(graph.Nodes),
		"edgeCount":         len(graph.Edges),
		"averageDegree":     float64(len(graph.Edges) * 2) / float64(len(graph.Nodes)),
		"maxDegree":         maxDegree,
		"mostConnectedNode": centralNode,
		"density":           float64(len(graph.Edges)) / float64(len(graph.Nodes)*(len(graph.Nodes)-1)/2),
	}
}
