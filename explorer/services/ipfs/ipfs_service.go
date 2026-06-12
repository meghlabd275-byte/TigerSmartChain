// Package ipfs provides IPFS integration for decentralized storage
package ipfs

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"mime/multipart"
	"strings"
	"sync"
	"time"
)

// IPFSNode represents an IPFS node connection
type IPFSNode struct {
	APIURL    string
	GatewayURL string
	APIToken  string
	mu        sync.RWMutex
}

// IPFSService provides IPFS storage services
type IPFSService struct {
	nodes      []*IPFSNode
	gatewayURL string
	pinQueue   chan *PinRequest
}

// PinRequest represents a pin request
type PinRequest struct {
	CID       string
	Name     string
	Metadata map[string]string
	Callback chan *PinResult
}

// PinResult represents pin operation result
type PinResult struct {
	CID     string `json:"cid"`
	Name   string `json:"name"`
	Pinned bool   `json:"pinned"`
	Error  error  `json:"error,omitempty"`
}

// IPFSFile represents a file in IPFS
type IPFSFile struct {
	CID       string    `json:"cid"`
	Name     string    `json:"name"`
	Size     int64     `json:"size"`
	Hash     string    `json:"hash"`
	Created  time.Time `json:"created"`
	Type     string    `json:"type"`
	MimeType string   `json:"mimeType"`
}

// NewIPFSService creates a new IPFS service
func NewIPFSService() *IPFSService {
	return &IPFSService{
		nodes:      initIPFSNodes(),
		pinQueue:  make(chan *PinRequest, 100),
	}
}

// initIPFSNodes initializes IPFS nodes
func initIPFSNodes() []*IPFSNode {
	return []*IPFSNode{
		{
			APIURL:     "https://ipfs-api.tigersmartchain.io",
			GatewayURL: "https://ipfs-gateway.tigersmartchain.io",
		},
		{
			APIURL:     "https://ipfs.io",
			GatewayURL: "https://ipfs.io",
		},
		{
			APIURL:     "https://gateway.pinata.cloud",
			GatewayURL: "https://gateway.pinata.cloud",
		},
	}
}

// AddFile adds a file to IPFS
func (s *IPFSService) AddFile(name string, data []byte) (*IPFSFile, error) {
	if len(data) == 0 {
		return nil, fmt.Errorf("empty data")
	}
	
	// Generate CID from data (simplified - would use proper IPFS)
	hash := generateCID(data)
	
	file := &IPFSFile{
		CID:      hash,
		Name:     name,
		Size:     int64(len(data)),
		Hash:     hash,
		Created: time.Now(),
		Type:     getFileType(name),
	}
	
	return file, nil
}

// AddJSON adds JSON data to IPFS
func (s *IPFSService) AddJSON(name string, data interface{}) (*IPFSFile, error) {
	jsonData, err := json.Marshal(data)
	if err != nil {
		return nil, err
	}
	
	return s.AddFile(name+".json", jsonData)
}

// AddContractSource adds contract source code to IPFS
func (s *IPFSService) AddContractSource(contract, source string, language string) (*IPFSFile, error) {
	metadata := map[string]string{
		"contract": contract,
		"language": language,
		"type":    "solidity",
		"uploaded": time.Now().Format(time.RFC3339),
	}
	
	sourceData := []byte(source)
	file, err := s.AddFile(contract+".sol", sourceData)
	if err != nil {
		return nil, err
	}
	
	// Add metadata
	metadataFile, err := s.AddJSON(contract+".metadata", metadata)
	if err == nil {
		return metadataFile, nil
	}
	
	return file, nil
}

// GetFile gets a file from IPFS
func (s *IPFSService) GetFile(cid string) ([]byte, error) {
	cid = normalizeCID(cid)
	
	// In production, would fetch from IPFS
	// For now, return mock data
	return []byte(fmt.Sprintf("mock data for %s", cid)), nil
}

// GetJSON gets JSON from IPFS
func (s *IPFSService) GetJSON(cid string, dest interface{}) error {
	data, err := s.GetFile(cid)
	if err != nil {
		return err
	}
	
	return json.Unmarshal(data, dest)
}

// GetGatewayURL gets public gateway URL for a CID
func (s *IPFSService) GetGatewayURL(cid string) string {
	cid = normalizeCID(cid)
	
	if len(s.nodes) > 0 {
		return fmt.Sprintf("%s/ipfs/%s", s.nodes[0].GatewayURL, cid)
	}
	
	return fmt.Sprintf("https://ipfs.io/ipfs/%s", cid)
}

// Pin pins a file to ensure persistence
func (s *IPFSService) Pin(cid string) error {
	cid = normalizeCID(cid)
	
	// In production, would send pin request to IPFS
	return nil
}

// Unpin unpins a file
func (s *IPFSService) Unpin(cid string) error {
	cid = normalizeCID(cid)
	
	// In production, would send unpin request
	return nil
}

// ListPinned lists pinned files
func (s *IPFSService) ListPinned() ([]*IPFSFile, error) {
	// In production, would query IPFS
	return []*IPFSFile{}, nil
}

// AddNode adds an IPFS node
func (s *IPFSService) AddNode(node *IPFSNode) error {
	if node == nil || node.APIURL == "" {
		return fmt.Errorf("invalid node")
	}
	
	s.nodes = append(s.nodes, node)
	return nil
}

// GenerateCID generates a CID for data
func generateCID(data []byte) string {
	// Using CIDv1 with dag-pb
	// In production, would use proper IPFS hashing
	if len(data) == 0 {
		return ""
	}
	
	// Simple hash for demonstration
	hash := fmt.Sprintf("Qm%x", data[:32])
	if len(hash) > 59 {
		hash = hash[:59]
	}
	
	return hash
}

// normalizeCID normalizes a CID
func normalizeCID(cid string) string {
	cid = strings.TrimPrefix(cid, "ipfs://")
	cid = strings.TrimPrefix(cid, "ipns://")
	
	// Remove /ipfs/ prefix if present
	if strings.HasPrefix(cid, "/ipfs/") {
		cid = strings.TrimPrefix(cid, "/ipfs/")
	}
	
	return cid
}

// getFileType determines file type from name
func getFileType(name string) string {
	ext := strings.ToLower(name)
	
	switch {
	case strings.HasSuffix(ext, ".sol"):
		return "solidity"
	case strings.HasSuffix(ext, ".json"):
		return "json"
	case strings.HasSuffix(ext, ".md"):
		return "markdown"
	case strings.HasSuffix(ext, ".js"):
		return "javascript"
	case strings.HasSuffix(ext, ".ts"):
		return "typescript"
	case strings.HasSuffix(ext, ".txt"):
		return "text"
	default:
		return "binary"
	}
}

// UploadForm uploads a file via multipart form
func (s *IPFSService) UploadForm(w io.Writer, fieldName, fileName string) (io.Writer, error) {
	// In production, would handle multipart upload
	return w, nil
}

// GetStats gets IPFS statistics
func (s *IPFSService) GetStats() (*IPFSStats, error) {
	return &IPFSStats{
		TotalFiles:  0,
		TotalSize:   0,
		PinnedCount: 0,
	}, nil
}

// IPFSStats represents IPFS statistics
type IPFSStats struct {
	TotalFiles  int   `json:"totalFiles"`
	TotalSize  int64 `json:"totalSize"`
	PinnedCount int   `json:"pinnedCount"`
}

// VerifyIntegrity verifies file integrity
func (s *IPFSService) VerifyIntegrity(cid string) (bool, error) {
	data, err := s.GetFile(cid)
	if err != nil {
		return false, err
	}
	
	expectedCID := generateCID(data)
	return cid == expectedCID, nil
}

// AddContractMetadata adds contract metadata to IPFS
func (s *IPFSService) AddContractMetadata(contract string, metadata *ContractMetadata) (*IPFSFile, error) {
	data, err := json.Marshal(metadata)
	if err != nil {
		return nil, err
	}
	
	return s.AddFile(contract+".metadata.json", data)
}

// ContractMetadata represents contract metadata
type ContractMetadata struct {
	ContractAddress string            `json:"contractAddress"`
	Name           string            `json:"name"`
	Version       string            `json:"version"`
	Language      string            `json:"language"`
	Compiler     string            `json:"compiler"`
	ABI          interface{}       `json:"abi"`
	SourceHash   string            `json:"sourceHash"`
	BuildInfo   *BuildInfo        `json:"buildInfo,omitempty"`
	Settings    *CompilerSettings `json:"settings,omitempty"`
}

// BuildInfo represents compilation build info
type BuildInfo struct {
	Optimizer bool `json:"optimizer"`
	Runs      int  `json:"runs"`
}

// CompilerSettings represents compiler settings
type CompilerSettings struct {
	EvmVersion string `json:"evmVersion"`
	Libraries map[string]string `json:"libraries,omitempty"`
}

// UploadWithEncryption uploads with client-side encryption
func (s *IPFSService) UploadWithEncryption(data []byte, password string) (*EncryptedUpload, error) {
	// In production, would encrypt before upload
	// For now, return mock
	return &EncryptedUpload{
		CID:       generateCID(data),
		IV:        hex.EncodeToString([]byte("16byteiv")),
		Salt:      hex.EncodeToString([]byte("32bytesalt")),
		UploadURL: s.GetGatewayURL(generateCID(data)),
	}, nil
}

// EncryptedUpload represents an encrypted upload
type EncryptedUpload struct {
	CID       string `json:"cid"`
	IV        string `json:"iv"`
	Salt      string `json:"salt"`
	UploadURL string `json:"uploadUrl"`
}

// DownloadWithDecryption downloads and decrypts
func (s *IPFSService) DownloadWithDecryption(cid, password string) ([]byte, error) {
	// In production, would decrypt after download
	return s.GetFile(cid)
}

// StartPinMonitor starts pinning monitoring
func (s *IPFSService) StartPinMonitor() {
	go func() {
		for {
			select {
			case req := <-s.pinQueue:
				result := &PinResult{
					CID:     req.CID,
					Name:   req.Name,
					Pinned: true,
				}
				
				if err := s.Pin(req.CID); err != nil {
					result.Pinned = false
					result.Error = err
				}
				
				req.Callback <- result
			}
		}
	}()
}

// QueuePin queues a pin request
func (s *IPFSService) QueuePin(cid, name string) chan *PinResult {
	callback := make(chan *PinResult, 1)
	s.pinQueue <- &PinRequest{
		CID:       cid,
		Name:      name,
		Callback:  callback,
	}
	return callback
}

// Context creates a context for IPFS operations
func (s *IPFSService) Context() context.Context {
	return context.Background()
}

// InitIPFSService initializes the service
func InitIPFSService() (*IPFSService, error) {
	return NewIPFSService(), nil
}