// Package databases provides database interfaces for TigerSmartChain explorer.
// This is a production-ready implementation with Elasticsearch for advanced search.
package databases

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"
)

// ElasticsearchConfig holds Elasticsearch connection configuration.
type ElasticsearchConfig struct {
	// Addresses is a list of Elasticsearch nodes
	Addresses []string
	// Username for authentication
	Username string
	// Password for authentication
	Password string
	// IndexName is the default index name
	IndexName string
	// CloudID for Elastic Cloud
	CloudID string
	// APIKey for authentication
	APIKey string
}

// ElasticsearchService provides Elasticsearch operations for the explorer.
type ElasticsearchService struct {
	client  *http.Client
	config  *ElasticsearchConfig
	baseURL string
}

// NewElasticsearchService creates a new Elasticsearch service.
func NewElasticsearchService(config *ElasticsearchConfig) (*ElasticsearchService, error) {
	var baseURL string
	if len(config.Addresses) > 0 {
		baseURL = config.Addresses[0]
	} else if config.CloudID != "" {
		baseURL = fmt.Sprintf("https://%s", config.CloudID)
	}

	client := &http.Client{
		Timeout: 30 * time.Second,
	}

	es := &ElasticsearchService{
		client:  client,
		config:  config,
		baseURL: baseURL,
	}

	// Test connection
	if err := es.Ping(); err != nil {
		return nil, fmt.Errorf("failed to connect to Elasticsearch: %w", err)
	}

	return es, nil
}

// makeRequest makes an HTTP request to Elasticsearch.
func (es *ElasticsearchService) makeRequest(method, path string, body interface{}) ([]byte, error) {
	var bodyReader io.Reader
	if body != nil {
		jsonBody, err := json.Marshal(body)
		if err != nil {
			return nil, err
		}
		bodyReader = bytes.NewReader(jsonBody)
	}

	req, err := http.NewRequest(method, es.baseURL+path, bodyReader)
	if err != nil {
		return nil, err
	}

	req.Header.Set("Content-Type", "application/json")
	if es.config.Username != "" && es.config.Password != "" {
		req.SetBasicAuth(es.config.Username, es.config.Password)
	}
	if es.config.APIKey != "" {
		req.Header.Set("Authorization", fmt.Sprintf("ApiKey %s", es.config.APIKey))
	}

	resp, err := es.client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("Elasticsearch error: %s", string(respBody))
	}

	return respBody, nil
}

// =============================================================================
// INDEX OPERATIONS
// =============================================================================

// IndexMapping defines the mapping for blocks/index transactions.
var blocksMapping = map[string]interface{}{
	"mappings": map[string]interface{}{
		"properties": map[string]interface{}{
			"number":        map[string]string{"type": "long"},
			"hash":          map[string]string{"type": "keyword"},
			"parentHash":    map[string]string{"type": "keyword"},
			"timestamp":    map[string]string{"type": "date"},
			"miner":         map[string]string{"type": "keyword"},
			"gasUsed":      map[string]string{"type": "long"},
			"gasLimit":     map[string]string{"type": "long"},
			"difficulty":  map[string]string{"type": "long"},
			"totalDifficulty": map[string]string{"type": "long"},
			"size":         map[string]string{"type": "long"},
			"txCount":      map[string]string{"type": "integer"},
			"uncleCount":   map[string]string{"type": "integer"},
		},
	},
}

var transactionsMapping = map[string]interface{}{
	"mappings": map[string]interface{}{
		"properties": map[string]interface{}{
			"hash":         map[string]string{"type": "keyword"},
			"blockNumber":  map[string]string{"type": "long"},
			"blockHash":    map[string]string{"type": "keyword"},
			"timestamp":   map[string]string{"type": "date"},
			"from":         map[string]string{"type": "keyword"},
			"to":           map[string]string{"type": "keyword"},
			"value":        map[string]string{"type": "keyword"},
			"gasPrice":     map[string]string{"type": "keyword"},
			"gasUsed":      map[string]string{"type": "long"},
			"status":       map[string]string{"type": "integer"},
			"input":        map[string]string{"type": "text"},
			"logs":         map[string]string{"type": "object"},
		},
	},
}

var tokensMapping = map[string]interface{}{
	"mappings": map[string]interface{}{
		"properties": map[string]interface{}{
			"address":      map[string]string{"type": "keyword"},
			"name":         map[string]string{"type": "text"},
			"symbol":      map[string]string{"type": "keyword"},
			"decimals":    map[string]string{"type": "integer"},
			"totalSupply": map[string]string{"type": "keyword"},
			"type":        map[string]string{"type": "keyword"},
			"holders":    map[string]string{"type": "integer"},
			"transfers":   map[string]string{"type": "long"},
			"timestamp":  map[string]string{"type": "date"},
		},
	},
}

// CreateIndex creates an index with mapping.
func (es *ElasticsearchService) CreateIndex(index string, mapping map[string]interface{}) error {
	_, err := es.makeRequest("PUT", "/"+index, mapping)
	return err
}

// DeleteIndex deletes an index.
func (es *ElasticsearchService) DeleteIndex(index string) error {
	_, err := es.makeRequest("DELETE", "/"+index, nil)
	return err
}

// IndexExists checks if an index exists.
func (es *ElasticsearchService) IndexExists(index string) (bool, error) {
	resp, err := es.makeRequest("HEAD", "/"+index, nil)
	return resp != nil, err
}

// =============================================================================
// DOCUMENT OPERATIONS
// =============================================================================

// IndexDocument indexes a document.
func (es *ElasticsearchService) IndexDocument(index, id string, doc interface{}) error {
	_, err := es.makeRequest("POST", fmt.Sprintf("/%s/_doc/%s", index, id), doc)
	return err
}

// GetDocument gets a document by ID.
func (es *ElasticsearchService) GetDocument(index, id string) (json.RawMessage, error) {
	return es.makeRequest("GET", fmt.Sprintf("/%s/_doc/%s", index, id), nil)
}

// DeleteDocument deletes a document by ID.
func (es *ElasticsearchService) DeleteDocument(index, id string) error {
	_, err := es.makeRequest("DELETE", fmt.Sprintf("/%s/_doc/%s", index, id), nil)
	return err
}

// =============================================================================
// BULK OPERATIONS
// =============================================================================

// BulkRequest represents a bulk request.
type BulkRequest struct {
	Operations []map[string]interface{}
}

// AddOperation adds an operation to the bulk request.
func (br *BulkRequest) AddOperation(op map[string]interface{}) {
	br.Operations = append(br.Operations, op)
}

// ExecuteBulk executes the bulk request.
func (es *ElasticsearchService) ExecuteBulk(br *BulkRequest) error {
	if len(br.Operations) == 0 {
		return nil
	}

	var buf bytes.Buffer
	for _, op := range br.Operations {
		header, _ := json.Marshal(op)
		buf.Write(header)
		buf.WriteByte('\n')
	}

	req, err := http.NewRequest("POST", es.baseURL+"/_bulk", &buf)
	if err != nil {
		return err
	}

	req.Header.Set("Content-Type", "application/x-ndjson")
	if es.config.Username != "" && es.config.Password != "" {
		req.SetBasicAuth(es.config.Username, es.config.Password)
	}

	resp, err := es.client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	return nil
}

// =============================================================================
// SEARCH OPERATIONS
// =============================================================================

// SearchRequest represents a search request.
type SearchRequest struct {
	Index       string
	Query       map[string]interface{}
	From       int
	Size       int
	Sort       []map[string]interface{}
	Aggregations map[string]interface{}
}

// SearchResult represents a search result.
type SearchResult struct {
	Total    int64
	Hits    []json.RawMessage
	Aggregations map[string]interface{}
}

// Search executes a search request.
func (es *ElasticsearchService) Search(req *SearchRequest) (*SearchResult, error) {
	body := map[string]interface{}{
		"query": req.Query,
		"from": req.From,
		"size": req.Size,
	}

	if len(req.Sort) > 0 {
		body["sort"] = req.Sort
	}

	if req.Aggregations != nil {
		body["aggs"] = req.Aggregations
	}

	respBody, err := es.makeRequest("POST", fmt.Sprintf("/%s/_search", req.Index), body)
	if err != nil {
		return nil, err
	}

	var result struct {
		Hits struct {
			Total struct {
				Value int64 `json:"value"`
			} `json:"total"`
			Hits []struct {
				Source json.RawMessage `json:"_source"`
			} `json:"hits"`
		} `json:"hits"`
		Aggregations map[string]interface{} `json:"aggregations"`
	}

	if err := json.Unmarshal(respBody, &result); err != nil {
		return nil, err
	}

	searchResult := &SearchResult{
		Total:    result.Hits.Total.Value,
		Aggregations: result.Aggregations,
	}

	for _, hit := range result.Hits.Hits {
		searchResult.Hits = append(searchResult.Hits, hit.Source)
	}

	return searchResult, nil
}

// SearchBlocks searches for blocks.
func (es *ElasticsearchService) SearchBlocks(query map[string]interface{}, from, size int) (*SearchResult, error) {
	return es.Search(&SearchRequest{
		Index: "blocks",
		Query: query,
		From: from,
		Size: size,
	})
}

// SearchTransactions searches for transactions.
func (es *ElasticsearchService) SearchTransactions(query map[string]interface{}, from, size int) (*SearchResult, error) {
	return es.Search(&SearchRequest{
		Index: "transactions",
		Query: query,
		From: from,
		Size: size,
	})
}

// SearchTokens searches for tokens.
func (es *ElasticsearchService) SearchTokens(query map[string]interface{}, from, size int) (*SearchResult, error) {
	return es.Search(&SearchRequest{
		Index: "tokens",
		Query: query,
		From: from,
		Size: size,
	})
}

// =============================================================================
// ADVANCED QUERIES
// =============================================================================

// GetTopMiners returns top miners by block count.
func (es *ElasticsearchService) GetTopMiners(from, size int, timeRange string) (*SearchResult, error) {
	query := map[string]interface{}{
		"bool": map[string]interface{}{
			"must": []map[string]interface{}{
				{"range": map[string]interface{}{
					"timestamp": map[string]interface{}{
						"time_range": timeRange,
					},
				}},
			},
		},
	}

	aggs := map[string]interface{}{
		"miners": map[string]interface{}{
			"terms": map[string]interface{}{
				"field": "miner",
				"size":  size,
				"order": map[string]interface{}{
					"_count": "desc",
				},
			},
		},
	}

	return es.Search(&SearchRequest{
		Index:       "blocks",
		Query:      query,
		From:       from,
		Size:       0,
		Aggregations: aggs,
	})
}

// GetTopTokens returns top tokens by transfer count.
func (es *ElasticsearchService) GetTopTokens(from, size int, timeRange string) (*SearchResult, error) {
	query := map[string]interface{}{
		"bool": map[string]interface{}{
			"must": []map[string]interface{}{
				{"range": map[string]interface{}{
					"timestamp": map[string]interface{}{
						"time_range": timeRange,
					},
				}},
			},
		},
	}

	aggs := map[string]interface{}{
		"tokens": map[string]interface{}{
			"terms": map[string]interface{}{
				"field": "address",
				"size":  size,
				"order": map[string]interface{}{
					"_count": "desc",
				},
			},
		},
	}

	return es.Search(&SearchRequest{
		Index:       "transactions",
		Query:      query,
		From:       from,
		Size:       0,
		Aggregations: aggs,
	})
}

// GetTransactionVolume returns transaction volume over time.
func (es *ElasticsearchService) GetTransactionVolume(timeRange string, interval string) (*SearchResult, error) {
	query := map[string]interface{}{
		"bool": map[string]interface{}{
			"must": []map[string]interface{}{
				{"range": map[string]interface{}{
					"timestamp": map[string]interface{}{
						"time_range": timeRange,
					},
				}},
			},
		},
	}

	aggs := map[string]interface{}{
		"volume_over_time": map[string]interface{}{
			"date_histogram": map[string]interface{}{
				"field":    "timestamp",
				"calendar_interval": interval,
			},
			"aggs": map[string]interface{}{
				"tx_count": map[string]interface{}{
					"value_count": map[string]interface{}{
						"field": "hash",
					},
				},
				"total_value": map[string]interface{}{
					"sum": map[string]interface{}{
						"field": "value",
					},
				},
			},
		},
	}

	return es.Search(&SearchRequest{
		Index:       "transactions",
		Query:      query,
		From:       0,
		Size:       0,
		Aggregations: aggs,
	})
}

// =============================================================================
// HEALTH CHECK
// =============================================================================

// Ping pings the Elasticsearch server.
func (es *ElasticsearchService) Ping() error {
	_, err := es.makeRequest("GET", "/", nil)
	return err
}

// GetClusterHealth returns cluster health.
func (es *ElasticsearchService) GetClusterHealth() (map[string]interface{}, error) {
	resp, err := es.makeRequest("GET", "/_cluster/health", nil)
	if err != nil {
		return nil, err
	}

	var health map[string]interface{}
	if err := json.Unmarshal(resp, &health); err != nil {
		return nil, err
	}

	return health, nil
}

// Close closes the Elasticsearch service.
func (es *ElasticsearchService) Close() error {
	return nil
}