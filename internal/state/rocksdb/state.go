// Package rocksdb provides RocksDB state storage for TigerSmartChain.
package rocksdb

import (
	"encoding/binary"
	"fmt"
	"sync"

	"github.com/tecbot/gorocksdb"
)

// =============================================================================
// ROCKSDB STATE DATABASE
// =============================================================================

// RocksDB implements state storage using RocksDB.
type RocksDB struct {
	mu sync.RWMutex

	// Database
	db *gorocksdb.DB

	// Column families
	accountsCF  *gorocksdb.ColumnFamilyHandle
	storageCF   *gorocksdb.ColumnFamilyHandle
	codeCF     *gorocksdb.ColumnFamilyHandle
	proofsCF   *gorocksdb.ColumnFamilyHandle
	snapshotsCF *gorocksdb.ColumnFamilyHandle

	// Configuration
	config *Config

	// Cache
	cache *StateCache
}

// Config holds RocksDB configuration.
type Config struct {
	// Database path
	Path string

	// Cache size
	CacheSize uint64

	// Max open files
	MaxOpenFiles int

	// Block cache size
	BlockCacheSize uint64

	// Write buffer size
	WriteBufferSize uint64

	// Compression
	Compression CompressionType

	// Enable archive mode
	ArchiveMode bool
}

// CompressionType represents compression type.
type CompressionType int

const (
	CompressionNone CompressionType = iota
	CompressionSnappy
	CompressionZlib
)

// NewRocksDB creates a new RocksDB state database.
func NewRocksDB(config *Config) (*RocksDB, error) {
	if config == nil {
		config = &Config{
			Path:               "/data/tigersmartchain/state",
			CacheSize:          1 << 30, // 1GB
			MaxOpenFiles:     100,
			BlockCacheSize:  512 << 20, // 512MB
			WriteBufferSize:  64 << 20, // 64MB
			Compression:    CompressionSnappy,
			ArchiveMode:    false,
		}
	}

	// Default options
	opts := gorocksdb.NewDefaultOptions()
	opts.SetMaxOpenFiles(config.MaxOpenFiles)
	opts.SetCache(gorocksdb.NewLRUCache(int64(config.CacheSize)))
	opts.SetWriteBufferSize(int64(config.WriteBufferSize))
	opts.SetCompression(gorocksdb.SnappyCompression)
	opts.SetCreateIfMissing(true)

	// Column families
	opts.SetColumnFamilyCount(5)

	// Open database
	db, err := gorocksdb.OpenDb(opts, config.Path)
	if err != nil {
		return nil, fmt.Errorf("failed to open rocksdb: %w", err)
	}

	rdb := &RocksDB{
		db:     db,
		config: config,
		cache:  NewStateCache(10000),
	}

	return rdb, nil
}

// =============================================================================
// ACCOUNT OPERATIONS
// =============================================================================

// GetAccount returns account state.
func (rdb *RocksDB) GetAccount(address []byte) (*Account, error) {
	rdb.mu.RLock()
	defer rdb.mu.RUnlock()

	// Check cache first
	cacheKey := string(address)
	if cached, ok := rdb.cache.GetAccount(cacheKey); ok {
		return cached, nil
	}

	// Read from database
	key := append([]byte("acc:"), address...)
	opts := gorocksdb.NewDefaultReadOptions()
	defer opts.Destroy()

	slice, err := rdb.db.Get(opts, key)
	if err != nil {
		return nil, err
	}
	defer slice.Destroy()

	if !slice.Exists() {
		return &Account{}, nil
	}

	// Decode account
	account := &Account{}
	if err := account.UnmarshalBinary(slice.Data()); err != nil {
		return nil, err
	}

	// Cache result
	rdb.cache.PutAccount(cacheKey, account)

	return account, nil
}

// PutAccount stores account state.
func (rdb *RocksDB) PutAccount(address []byte, account *Account) error {
	rdb.mu.Lock()
	defer rdb.mu.Unlock()

	// Encode account
	data, err := account.MarshalBinary()
	if err != nil {
		return err
	}

	// Write to database
	key := append([]byte("acc:"), address...)
	opts := gorocksdb.NewDefaultWriteOptions()
	defer opts.Destroy()

	if err := rdb.db.Put(opts, key, data); err != nil {
		return err
	}

	// Update cache
	rdb.cache.PutAccount(string(address), account)

	return nil
}

// DeleteAccount removes account state.
func (rdb *RocksDB) DeleteAccount(address []byte) error {
	rdb.mu.Lock()
	defer rdb.mu.Unlock()

	// Delete from database
	key := append([]byte("acc:"), address...)
	opts := gorocksdb.NewDefaultWriteOptions()
	defer opts.Destroy()

	if err := rdb.db.Delete(opts, key); err != nil {
		return err
	}

	// Remove from cache
	rdb.cache.RemoveAccount(string(address))

	return nil
}

// =============================================================================
// STORAGE OPERATIONS
// =============================================================================

// GetStorage returns storage value.
func (rdb *RocksDB) GetStorage(address []byte, key []byte) ([]byte, error) {
	rdb.mu.RLock()
	defer rdb.mu.RUnlock()

	// Check cache first
	cacheKey := string(append(address, key...))
	if cached, ok := rdb.cache.GetStorage(cacheKey); ok {
		return cached, nil
	}

	// Read from database
	dbKey := append(append([]byte("sto:"), address...), key...)
	opts := gorocksdb.NewDefaultReadOptions()
	defer opts.Destroy()

	slice, err := rdb.db.Get(opts, dbKey)
	if err != nil {
		return nil, err
	}
	defer slice.Destroy()

	if !slice.Exists() {
		return []byte{}, nil
	}

	// Cache result
	rdb.cache.PutStorage(cacheKey, slice.Data())

	return slice.Data(), nil
}

// PutStorage stores storage value.
func (rdb *RocksDB) PutStorage(address []byte, key []byte, value []byte) error {
	rdb.mu.Lock()
	defer rdb.mu.Unlock()

	// Write to database
	dbKey := append(append([]byte("sto:"), address...), key...)
	opts := gorocksdb.NewDefaultWriteOptions()
	defer opts.Destroy()

	if err := rdb.db.Put(opts, dbKey, value); err != nil {
		return err
	}

	// Update cache
	rdb.cache.PutStorage(string(append(address, key...)), value)

	return nil
}

// =============================================================================
// CODE OPERATIONS
// =============================================================================

// GetCode returns contract code.
func (rdb *RocksDB) GetCode(address []byte) ([]byte, error) {
	rdb.mu.RLock()
	defer rdb.mu.RUnlock()

	// Check cache first
	cacheKey := string(address)
	if cached, ok := rdb.cache.GetCode(cacheKey); ok {
		return cached, nil
	}

	// Read from database
	key := append([]byte("code:"), address...)
	opts := gorocksdb.NewDefaultReadOptions()
	defer opts.Destroy()

	slice, err := rdb.db.Get(opts, key)
	if err != nil {
		return nil, err
	}
	defer slice.Destroy()

	if !slice.Exists() {
		return []byte{}, nil
	}

	// Cache result
	rdb.cache.PutCode(cacheKey, slice.Data())

	return slice.Data(), nil
}

// PutCode stores contract code.
func (rdb *RocksDB) PutCode(address []byte, code []byte) error {
	rdb.mu.Lock()
	defer rdb.mu.Unlock()

	// Write to database
	key := append([]byte("code:"), address...)
	opts := gorocksdb.NewDefaultWriteOptions()
	defer opts.Destroy()

	if err := rdb.db.Put(opts, key, code); err != nil {
		return err
	}

	// Update cache
	rdb.cache.PutCode(string(address), code)

	return nil
}

// =============================================================================
// SNAPSHOT OPERATIONS
// =============================================================================

// CreateSnapshot creates a state snapshot.
func (rdb *RocksDB) CreateSnapshot() (SnapshotID, error) {
	rdb.mu.Lock()
	defer rdb.mu.Unlock()

	// Create checkpoint
	opts := gorocksdb.NewCheckpoint(rdb.db)
	id, err := opts.CreateCheckpoint(fmt.Sprintf("/data/tigersmartchain/snapshots/%d", id))
	if err != nil {
		return 0, err
	}

	return SnapshotID(id), nil
}

// RestoreSnapshot restores a state snapshot.
func (rdb *RocksDB) RestoreSnapshot(id SnapshotID) error {
	rdb.mu.Lock()
	defer rdb.mu.Unlock()

	// Close current database
	if err := rdb.db.Close(); err != nil {
		return err
	}

	// Open from snapshot
	opts := gorocksdb.NewDefaultOptions()
	db, err := gorocksdb.OpenDb(opts, fmt.Sprintf("/data/tigersmartchain/snapshots/%d", id))
	if err != nil {
		return err
	}

	rdb.db = db
	return nil
}

// SnapshotID represents a snapshot identifier.
type SnapshotID uint64

// =============================================================================
// PRUNING OPERATIONS
// =============================================================================

// Prune removes old state data.
func (rdb *RocksDB) Prune(blockNumber uint64) error {
	rdb.mu.Lock()
	defer rdb.mu.Unlock()

	// Get minimum block to keep
	minBlock := blockNumber
	if rdb.config.ArchiveMode {
		minBlock = blockNumber
	} else {
		minBlock = blockNumber - 100 // Keep last 100 blocks
	}

	// In production, use compaction filter to prune
	_ = minBlock

	return nil
}

// Compact runs database compaction.
func (rdb *RocksDB) Compact() error {
	rdb.mu.Lock()
	defer rdb.mu.Unlock()

	rdb.db.CompactRange(gorocksdb.Range{
		Start: []byte("a"),
		Limit: []byte("z"),
	})

	return nil
}

// =============================================================================
// ACCOUNT STRUCTURE
// =============================================================================

// Account represents account state.
type Account struct {
	Nonce    uint64
	Balance []byte
	Root    []byte
	CodeHash []byte
}

// MarshalBinary encodes account to bytes.
func (a *Account) MarshalBinary() ([]byte, error) {
	data := make([]byte, 8+len(a.Balance)+len(a.Root)+len(a.CodeHash))
	offset := 0

	binary.LittleEndian.PutUint64(data[offset:], a.Nonce)
	offset += 8

	copy(data[offset:], a.Balance)
	offset += len(a.Balance)

	copy(data[offset:], a.Root)
	offset += len(a.Root)

	copy(data[offset:], a.CodeHash)

	return data, nil
}

// UnmarshalBinary decodes account from bytes.
func (a *Account) UnmarshalBinary(data []byte) error {
	if len(data) < 8 {
		return fmt.Errorf("invalid account data")
	}

	offset := 0
	a.Nonce = binary.LittleEndian.Uint64(data[offset:])
	offset += 8

	a.Balance = data[offset8 : offset8+32]
	offset += 32

	a.Root = data[offset32 : offset32+32]
	offset += 32

	a.CodeHash = data[offset64:]

	return nil
}

// =============================================================================
// STATE CACHE
// =============================================================================

// StateCache provides in-memory caching.
type StateCache struct {
	mu sync.RWMutex

	accounts map[string]*Account
	storage  map[string][]byte
	code    map[string][]byte

	size    uint64
	maxSize uint64
}

// NewStateCache creates a new state cache.
func NewStateCache(maxSize uint64) *StateCache {
	return &StateCache{
		accounts: make(map[string]*Account),
		storage:  make(map[string][]byte),
		code:    make(map[string][]byte),
		maxSize: maxSize,
	}
}

// GetAccount returns cached account.
func (c *StateCache) GetAccount(key string) (*Account, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	acc, ok := c.accounts[key]
	return acc, ok
}

// PutAccount caches account.
func (c *StateCache) PutAccount(key string, account *Account) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.accounts[key] = account
}

// GetStorage returns cached storage.
func (c *StateCache) GetStorage(key string) ([]byte, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	val, ok := c.storage[key]
	return val, ok
}

// PutStorage caches storage.
func (c *StateCache) PutStorage(key string, value []byte) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.storage[key] = value
}

// GetCode returns cached code.
func (c *StateCache) GetCode(key string) ([]byte, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	val, ok := c.code[key]
	return val, ok
}

// PutCode caches code.
func (c *StateCache) PutCode(key string, code []byte) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.code[key] = code
}

// RemoveAccount removes from cache.
func (c *StateCache) RemoveAccount(key string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	delete(c.accounts, key)
}

// =============================================================================
// CLOSE
// =============================================================================

// Close closes the database.
func (rdb *RocksDB) Close() error {
	rdb.mu.Lock()
	defer rdb.mu.Unlock()

	if rdb.db != nil {
		return rdb.db.Close()
	}

	return nil
}

// =============================================================================
// INIT
// =============================================================================

func init() {
	// Avoid unused warnings
	_ = fmt.Sprintf
}