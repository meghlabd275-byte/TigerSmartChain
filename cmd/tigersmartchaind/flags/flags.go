// Package flags provides command-line flags for tigersmartchaind.
package flags

// Chain configuration
var ChainID uint64
var DataDir string
var ConfigFile string

// Logging
var Verbosity bool

// HTTP server
var HTTPHost string
var HTTPPort uint
var HTTPModules string
var HTTPVirtualHosts string
var HTTPCorsOrigins string

// WebSocket server
var WSHost string
var WSPort uint
var WSModules string
var WSOrigins string

// P2P networking
var Bootnodes string
var Key string
var NetworkID uint64

// Database
var SyncMode string
var Cache uint64
var PersistBlockCount uint64
var LevelDbMemory uint64

// Mining
var Mine bool
var MineThreads uint64
var MineWorkPercent int

// RPC authentication
var AuthJWT string
var AuthAPIVecret string

// API
var APIEnable bool

// Metrics
var MetricsEnabled bool
var MetricsAddr string
var MetricsPort uint

// Other
var NoDiscover bool
var DiscoveryPort uint
var ListenAddr string
var MaxPeers uint64
var GasFloor uint64
var GasCeil uint64

// Defaults
const (
	DefaultChainID    = 9001
	DefaultHTTPHost   = "127.0.0.1"
	DefaultHTTPPort  = 8545
	DefaultWSHost    = "127.0.0.1"
	DefaultWSPort    = 8546
	DefaultCache     = 1024
	DefaultMaxPeers  = 100
)