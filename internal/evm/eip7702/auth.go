// Package eip7702 provides EIP-7702 Account Abstraction implementation.
// EIP-7702 enables smart contract wallets through AUTHORIZE opcode.
package eip7702

import (
	"crypto/sha256"
	"fmt"
	"math/big"
	"sync"
	"time"

	"github.com/tigersmartchain/tigersmartchain/internal/state"
)

// =============================================================================
// EIP-7702: Account Abstraction
// =============================================================================

// AuthorizationLevel represents the authorization level.
type AuthorizationLevel byte

const (
	// AuthLevelNone means no authorization
	AuthLevelNone AuthorizationLevel = 0x00
	// AuthLevelCall means authorized for calls
	AuthLevelCall AuthorizationLevel = 0x01
	// AuthLevelDelegateCall means authorized for delegate calls
	AuthLevelDelegateCall AuthorizationLevel = 0x02
	// AuthLevelSetCode means authorized to set code
	AuthLevelSetCode AuthorizationLevel = 0x04
)

// Authorization represents an EIP-7702 authorization.
type Authorization struct {
	// Authorizer is the address that authorized
	Authorizer string
	// Authorizee is the authorized address
	Authorizee string
	// Contract is the target contract address
	Contract string
	// Nonce is the authorization nonce
	Nonce *big.Int
	// ChainID is the chain ID
	ChainID *big.Int
	// ExpiresAt is the expiration timestamp
	ExpiresAt uint64
	// Level is the authorization level
	Level AuthorizationLevel
	// Signature is the authorization signature
	Signature []byte
}

// AuthManager manages EIP-7702 authorizations.
type AuthManager struct {
	mu sync.RWMutex

	// Authorizations by (authorizer, authorizee, nonce)
	authorizations map[string]*Authorization

	// Authorized code by address
	authorizedCodes map[string][]byte

	// State database
	stateDB state.Database

	// Chain ID
	chainID *big.Int
}

// NewAuthManager creates a new authorization manager.
func NewAuthManager(chainID *big.Int) *AuthManager {
	return &AuthManager{
		authorizations:  make(map[string]*Authorization),
		authorizedCodes: make(map[string][]byte),
		chainID:        chainID,
	}
}

// GetAuthorizationKey generates the authorization key.
func GetAuthorizationKey(authorizer, authorizee string, nonce *big.Int) string {
	data := fmt.Sprintf("%s:%s:%s", authorizer, authorizee, nonce.String())
	h := sha256.Sum256([]byte(data))
	return fmt.Sprintf("%x", h)
}

// Authorize creates an authorization for an address.
func (am *AuthManager) Authorize(auth *Authorization) error {
	if auth == nil {
		return fmt.Errorf("nil authorization")
	}

	if auth.Authorizee == "" {
		return fmt.Errorf("empty authorizee")
	}

	if auth.ChainID == nil {
		auth.ChainID = am.chainID
	}

	am.mu.Lock()
	defer am.mu.Unlock()

	key := GetAuthorizationKey(auth.Authorizer, auth.Authorizee, auth.Nonce)
	am.authorizations[key] = auth

	return nil
}

// GetAuthorization returns an authorization.
func (am *AuthManager) GetAuthorization(authorizer, authorizee string, nonce *big.Int) (*Authorization, error) {
	am.mu.RLock()
	defer am.mu.RUnlock()

	key := GetAuthorizationKey(authorizer, authorizee, nonce)
	auth, ok := am.authorizations[key]
	if !ok {
		return nil, fmt.Errorf("authorization not found")
	}

	// Check expiration
	if auth.ExpiresAt > 0 {
		if uint64(time.Now().Unix()) > auth.ExpiresAt {
			return nil, fmt.Errorf("authorization expired")
		}
	}

	// Check chain ID
	if auth.ChainID != nil && auth.ChainID.Cmp(am.chainID) != 0 {
		return nil, fmt.Errorf("wrong chain ID")
	}

	return auth, nil
}

// RevokeAuthorization revokes an authorization.
func (am *AuthManager) RevokeAuthorization(authorizer, authorizee string, nonce *big.Int) error {
	am.mu.Lock()
	defer am.mu.Unlock()

	key := GetAuthorizationKey(authorizer, authorizee, nonce)
	if _, ok := am.authorizations[key]; !ok {
		return fmt.Errorf("authorization not found")
	}

	delete(am.authorizations, key)
	return nil
}

// SetCode sets the code for an address (EIP-7702 SETCODE).
func (am *AuthManager) SetCode(addr string, code []byte) error {
	am.mu.Lock()
	defer am.mu.Unlock()

	am.authorizedCodes[addr] = code
	return nil
}

// GetCode returns the code for an address.
func (am *AuthManager) GetCode(addr string) ([]byte, bool) {
	am.mu.RLock()
	defer am.mu.RUnlock()

	code, ok := am.authorizedCodes[addr]
	return code, ok
}

// HasAuthorization checks if an address has authorization.
func (am *AuthManager) HasAuthorization(addr string) bool {
	am.mu.RLock()
	defer am.mu.RUnlock()

	_, ok := am.authorizations[GetAuthorizationKey("", addr, big.NewInt(0))]
	return ok
}

// GetAuthorizations returns all authorizations for an address.
func (am *AuthManager) GetAuthorizations(authorizee string) []*Authorization {
	am.mu.RLock()
	defer am.mu.RUnlock()

	result := make([]*Authorization, 0)
	for _, auth := range am.authorizations {
		if auth.Authorizee == authorizee {
			result = append(result, auth)
		}
	}

	return result
}

// ClearExpired clears expired authorizations.
func (am *AuthManager) ClearExpired() {
	am.mu.Lock()
	defer am.mu.Unlock()

	now := uint64(time.Now().Unix())
	for key, auth := range am.authorizations {
		if auth.ExpiresAt > 0 && now > auth.ExpiresAt {
			delete(am.authorizations, key)
		}
	}
}

// SetStateDB sets the state database.
func (am *AuthManager) SetStateDB(db state.Database) {
	am.mu.Lock()
	defer am.mu.Unlock()
	am.stateDB = db
}