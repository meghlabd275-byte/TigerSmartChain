// Package graph provides transaction flow graph visualization services
package graph

import (
	"fmt"
	"strings"
	"time"
)

// GraphService provides transaction flow graph visualization
type GraphService struct {
	nodes    map[string]*Node
	edges    []*Edge
	maxDepth int
}

// Node represents a node in the transaction graph
type Node struct {
	ID        string    `json:"id"`
	Type      string    `json:"type"` // address, contract, token, dex
	Address   string    `json:"address"`
	Label    string    `json:"label"`
	TxCount  int       `json:"txCount"`
	Volume   string    `json:"volume"`
	IsContract bool   `json:"isContract"`
	Metadata map[string]string `json:"metadata,omitempty"`
}

// Edge represents an edge in the transaction graph
type Edge struct {
	ID        string `json:"id"`
	Source   string `json:"source"`
	Target   string `json:"target"`
	Type     string `json:"type"` // transfer, swap, call, internal
	Value    string `json:"value"`
	TxHash   string `json:"txHash"`
	Block    uint64 `json:"block"`
	Index    int    `json:"index"`
}

// TransactionGraph represents the complete transaction graph
type TransactionGraph struct {
	RootTxHash string    `json:"rootTxHash"`
	Nodes     []*Node   `json:"nodes"`
	Edges     []*Edge  `json:"edges"`
	Depth     int      `json:"depth"`
	Timestamp time.Time `json:"timestamp"`
}

// NewGraphService creates a new graph service
func NewGraphService() *GraphService {
	return &GraphService{
		nodes:    make(map[string]*Node),
		edges:    make([]*Edge, 0),
		maxDepth: 10,
	}
}

// BuildGraph builds a transaction flow graph from a transaction
func (g *GraphService) BuildGraph(txHash string, trace []*TxTrace) (*TransactionGraph, error) {
	graph := &TransactionGraph{
		RootTxHash: txHash,
		Nodes:     make([]*Node, 0),
		Edges:     make([]*Edge, 0),
		Timestamp: time.Now(),
	}
	
	if len(trace) == 0 {
		return graph, nil
	}
	
	// Build nodes from trace
	addressNodes := make(map[string]*Node)
	
	for i, t := range trace {
		// Add source node
		if _, ok := addressNodes[t.From]; !ok {
			addressNodes[t.From] = &Node{
				ID:      t.From,
				Address: t.From,
				Type:    g.determineNodeType(t.From),
			}
		}
		
		// Add target node
		if _, ok := addressNodes[t.To]; !ok {
			addressNodes[t.To] = &Node{
				ID:      t.To,
				Address: t.To,
				Type:    g.determineNodeType(t.To),
			}
		}
		
		// Add edge
		edge := &Edge{
			ID:      fmt.Sprintf("e%d", i),
			Source: t.From,
			Target: t.To,
			Type:   t.Type,
			Value:  t.Value,
			TxHash: t.TxHash,
			Block:  t.BlockNumber,
			Index:  i,
		}
		
		graph.Edges = append(graph.Edges, edge)
	}
	
	// Convert nodes map to slice
	for _, node := range addressNodes {
		graph.Nodes = append(graph.Nodes, node)
	}
	
	graph.Depth = g.calculateDepth(graph.Edges)
	
	return graph, nil
}

// TxTrace represents a transaction trace entry
type TxTrace struct {
	TxHash      string
	From        string
	To          string
	Value       string
	Type        string
	BlockNumber uint64
}

// determineNodeType determines the type of a node
func (g *GraphService) determineNodeType(address string) string {
	// In production, would check if address is a contract
	if strings.HasPrefix(address, "0x00000000") {
		return "system"
	}
	return "address"
}

// calculateDepth calculates the maximum depth of the graph
func (g *GraphService) calculateDepth(edges []*Edge) int {
	if len(edges) == 0 {
		return 0
	}
	
	depth := 0
	visited := make(map[string]bool)
	
	var traverse func(string, int)
	traverse = func(node string, d int) {
		if d > depth {
			depth = d
		}
		if visited[node] {
			return
		}
		visited[node] = true
		
		for _, e := range edges {
			if e.Source == node {
				traverse(e.Target, d+1)
			}
		}
	}
	
	// Start from first edge
	if len(edges) > 0 {
		traverse(edges[0].Source, 0)
	}
	
	return depth
}

// GetTransactionFlow gets transaction flow for an address
func (g *GraphService) GetTransactionFlow(address string, limit int) (*FlowSummary, error) {
	summary := &FlowSummary{
		Address: address,
		Inflow:  InFlow{TxCount: 0, Volume: "0"},
		Outflow: OutFlow{TxCount: 0, Volume: "0"},
		TopCounterparties: []string{},
	}
	
	return summary, nil
}

// FlowSummary represents a flow summary
type FlowSummary struct {
	Address          string   `json:"address"`
	Inflow          InFlow   `json:"inflow"`
	Outflow         OutFlow  `json:"outflow"`
	TopCounterparties []string `json:"topCounterparties"`
}

// InFlow represents inbound flow
type InFlow struct {
	TxCount int    `json:"txCount"`
	Volume  string `json:"volume"`
}

// OutFlow represents outbound flow
type OutFlow struct {
	TxCount int    `json:"txCount"`
	Volume  string `json:"volume"`
}

// GetGraphForAddress gets the graph for an address
func (g *GraphService) GetGraphForAddress(address string, depth int) (*TransactionGraph, error) {
	if depth > g.maxDepth {
		depth = g.maxDepth
	}
	
	// In production, would query database
	return &TransactionGraph{
		RootTxHash: address,
		Nodes:     []*Node{},
		Edges:     []*Edge{},
		Depth:     0,
	}, nil
}

// GenerateGraphImage generates a graph image (returns DOT format)
func (g *GraphService) GenerateGraphImage(graph *TransactionGraph) (string, error) {
	if graph == nil {
		return "", fmt.Errorf("nil graph")
	}
	
	var sb strings.Builder
	
	sb.WriteString("digraph txflow {\n")
	sb.WriteString("  rankdir=LR;\n")
	sb.WriteString("  node [shape=box];\n\n")
	
	// Write nodes
	for _, node := range graph.Nodes {
		label := node.Address
		if node.Label != "" {
			label = node.Label
		}
		
		shape := "box"
		if node.Type == "contract" {
			shape = "ellipse"
		}
		
		sb.WriteString(fmt.Sprintf("  %s [label=\"%s\" shape=%s];\n", 
			node.ID, label, shape))
	}
	
	sb.WriteString("\n")
	
	// Write edges
	for _, edge := range graph.Edges {
		sb.WriteString(fmt.Sprintf("  %s -> %s [label=\"%s\"];\n",
			edge.Source, edge.Target, edge.Type))
	}
	
	sb.WriteString("}\n")
	
	return sb.String(), nil
}

// GetClusterAnalysis performs cluster analysis
func (g *GraphService) GetClusterAnalysis(address string) (*ClusterResult, error) {
	return &ClusterResult{
		Address:     address,
		ClusterSize: 0,
		Members:     []string{},
		ClusterType: "unknown",
	}, nil
}

// ClusterResult represents cluster analysis result
type ClusterResult struct {
	Address     string   `json:"address"`
	ClusterSize int      `json:"clusterSize"`
	Members    []string `json:"members"`
	ClusterType string   `json:"clusterType"`
}

// InitGraphService initializes the service
func InitGraphService() (*GraphService, error) {
	return NewGraphService(), nil
}