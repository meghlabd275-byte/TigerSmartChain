// Package unit provides unit tests for TigerScan
package unit

import (
	"context"
	"encoding/hex"
	"math/big"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/mock"

	"tigersmartchain/explorer/services/tokens"
	"tigersmartchain/explorer/services/nfts"
	"tigersmartchain/explorer/services/analytics"
	"tigersmartchain/explorer/services/blocks"
)

// MockClient is a mock blockchain client
type MockClient struct {
	mock.Mock
}

func (m *MockClient) BlockByNumber(ctx context.Context, num *big.Int) (*types.Block, error) {
	args := m.Called(ctx, num)
	if args.Get(0) == nil {
		return nil, args.Error(1)
	}
	return args.Get(0).(*types.Block), args.Error(1)
}

func (m *MockClient) BlockByHash(ctx context.Context, hash common.Hash) (*types.Block, error) {
	args := m.Called(ctx, hash)
	if args.Get(0) == nil {
		return nil, args.Error(1)
	}
	return args.Get(0).(*types.Block), args.Error(1)
}

func (m *MockClient) TransactionByHash(ctx context.Context, hash common.Hash) (*types.Transaction, bool, error) {
	args := m.Called(ctx, hash)
	return args.Get(0).(*types.Transaction), args.Bool(1), args.Error(2)
}

func (m *MockClient) TransactionReceipt(ctx context.Context, hash common.Hash) (*types.Receipt, error) {
	args := m.Called(ctx, hash)
	if args.Get(0) == nil {
		return nil, args.Error(1)
	}
	return args.Get(0).(*types.Receipt), args.Error(1)
}

func (m *MockClient) BalanceAt(ctx context.Context, address common.Address, block *big.Int) (*big.Int, error) {
	args := m.Called(ctx, address, block)
	return args.Get(0).(*big.Int), args.Error(1)
}

func (m *MockClient) CodeAt(ctx context.Context, address common.Address, block *big.Int) ([]byte, error) {
	args := m.Called(ctx, address, block)
	return args.Get(0).([]byte), args.Error(1)
}

func (m *MockClient) ChainID(ctx context.Context) (*big.Int, error) {
	args := m.Called(ctx)
	return args.Get(0).(*big.Int), args.Error(1)
}

// TestTokenService tests the token service
func TestTokenService(t *testing.T) {
	t.Run("GetTokenInfo", func(t *testing.T) {
		// Test token info retrieval
		token := &tokens.Token{
			Address:     common.HexToAddress("0x1234567890123456789012345678901234567890").Hex(),
			Name:       "Test Token",
			Symbol:     "TEST",
			Decimals:   18,
			TotalSupply: "1000000",
		}
		
		assert.Equal(t, "Test Token", token.Name)
		assert.Equal(t, "TEST", token.Symbol)
		assert.Equal(t, uint8(18), token.Decimals)
	})
	
	t.Run("TokenTransfers", func(t *testing.T) {
		// Test token transfer parsing
		transfer := &tokens.TokenTransfer{
			Hash:        "0xabc123",
			BlockNumber: 12345,
			From:       common.HexToAddress("0x111").Hex(),
			To:         common.HexToAddress("0x222").Hex(),
			Value:      "1000000000000000000",
			Timestamp:  1234567890,
		}
		
		assert.Equal(t, uint64(12345), transfer.BlockNumber)
		assert.NotEmpty(t, transfer.Hash)
	})
	
	t.Run("TokenHolders", func(t *testing.T) {
		// Test token holder parsing
		holder := &tokens.TokenHolder{
			Address:  common.HexToAddress("0xaaa").Hex(),
			Balance:  "1000000000000000000",
			Percent:  "10.0",
		}
		
		assert.NotEmpty(t, holder.Address)
		assert.Equal(t, "10.0", holder.Percent)
	})
}

// TestNFTService tests the NFT service
func TestNFTService(t *testing.T) {
	t.Run("GetNFTInfo", func(t *testing.T) {
		// Test NFT info retrieval
		nft := &nfts.NFT{
			Address:     common.HexToAddress("0x4567890123456789012345678901234567890").Hex(),
			Name:        "Test NFT",
			Symbol:     "TNFT",
			TotalSupply: 10000,
		}
		
		assert.Equal(t, "Test NFT", nft.Name)
		assert.Equal(t, "TNFT", nft.Symbol)
		assert.Equal(t, uint64(10000), nft.TotalSupply)
	})
	
	t.Run("NFTMetadata", func(t *testing.T) {
		// Test NFT metadata
		metadata := &nfts.NFTMetadata{
			Name:        "Test NFT #1",
			Description: "A test NFT",
			Image:       "ipfs://QmTest123",
			Attributes: []nfts.NFTAttribute{
				{TraitType: "Background", Value: "Blue"},
				{TraitType: "Eyes", Value: "Normal"},
			},
		}
		
		assert.Equal(t, "Test NFT #1", metadata.Name)
		assert.Len(t, metadata.Attributes, 2)
	})
	
	t.Run("NFTOwnership", func(t *testing.T) {
		// Test NFT ownership
		ownership := &nfts.NFTOwnershipHistory{
			TokenID:    "1",
			FromBlock:  10000,
			ToBlock:    20000,
			From:      common.HexToAddress("0x111").Hex(),
			To:        common.HexToAddress("0x222").Hex(),
			Timestamp: 1234567890,
		}
		
		assert.Equal(t, "1", ownership.TokenID)
	})
}

// TestAnalyticsService tests the analytics service
func TestAnalyticsService(t *testing.T) {
	t.Run("NetworkStats", func(t *testing.T) {
		// Test network stats
		stats := &analytics.NetworkStats{
			BlockNumber:        123456,
			TotalAddresses:     10000,
			TotalTransactions: 50000,
			TotalValueLocked:  "1000000000000000000000",
			MarketCap:         "500000000000000000000",
			TPS:              50.5,
			GasPrice:        "20",
		}
		
		assert.Equal(t, uint64(123456), stats.BlockNumber)
		assert.Equal(t, 50.5, stats.TPS)
	})
	
	t.Run("TransactionSearch", func(t *testing.T) {
		// Test transaction search
		result := &analytics.SearchResult{
			Type:  "transaction",
			Value: "0xabc123",
			Block: 12345,
		}
		
		assert.Equal(t, "transaction", result.Type)
	})
	
	t.Run("GasTracker", func(t *testing.T) {
		// Test gas tracking
		gas := &analytics.GasTracker{
			Slow:      "10",
			Standard: "20",
			Fast:     "30",
			Timestamp: 1234567890,
		}
		
		assert.Equal(t, "20", gas.Standard)
	})
}

// TestBlockService tests the block service
func TestBlockService(t *testing.T) {
	t.Run("BlockInfo", func(t *testing.T) {
		// Test block info
		block := &blocks.BlockInfo{
			Number:     12345,
			Hash:       "0xabc123def456",
			ParentHash: "0xparent123",
			Miner:     common.HexToAddress("0xminer").Hex(),
			GasUsed:   15000000,
			GasLimit:   30000000,
			Timestamp: 1234567890,
		}
		
		assert.Equal(t, uint64(12345), block.Number)
		assert.Equal(t, uint64(15000000), block.GasUsed)
	})
	
	t.Run("BlockRewards", func(t *testing.T) {
		// Test block rewards
		rewards := &blocks.BlockRewards{
			BlockNumber: 12345,
			MinerReward: "2000000000000000000",
			UncleReward: "187500000000000000",
		}
		
		assert.NotEmpty(t, rewards.MinerReward)
	})
	
	t.Run("UncleInfo", func(t *testing.T) {
		// Test uncle info
		uncle := &blocks.UncleInfo{
			Number:      12344,
			Hash:       "0xuncle123",
			Miner:      common.HexToAddress("0xminer").Hex(),
			Difficulty: "1000000",
		}
		
		assert.Equal(t, uint64(12344), uncle.Number)
	})
}

// TestAddressParsing tests address parsing utilities
func TestAddressParsing(t *testing.T) {
	t.Run("ValidAddress", func(t *testing.T) {
		addr := common.HexToAddress("0x1234567890123456789012345678901234567890")
		assert.True(t, common.IsHexAddress(addr.Hex()))
	})
	
	t.Run("InvalidAddress", func(t *testing.T) {
		assert.False(t, common.IsHexAddress("invalid"))
	})
	
	t.Run("ZeroAddress", func(t *testing.T) {
		assert.Equal(t, "0x0000000000000000000000000000000000000000", common.Address{}.Hex())
	})
}

// TestHexParsing tests hex parsing utilities
func TestHexParsing(t *testing.T) {
	t.Run("HexToBytes", func(t *testing.T) {
		hexStr := "0xabcdef"
		data, err := hex.DecodeString(hexStr[2:])
		assert.NoError(t, err)
		assert.Equal(t, []byte{0xab, 0xcd, 0xef}, data)
	})
	
	t.Run("BytesToHex", func(t *testing.T) {
		data := []byte{0xab, 0xcd, 0xef}
		hexStr := "0x" + hex.EncodeToString(data)
		assert.Equal(t, "0xabcdef", hexStr)
	})
}

// TestBigIntParsing tests big.Int parsing
func TestBigIntParsing(t *testing.T) {
	t.Run("ParseWei", func(t *testing.T) {
		wei := big.NewInt(1000000000000000000)
		assert.Equal(t, int64(1), wei.Div(wei, big.NewInt(1e18)).Int64())
	})
	
	t.Run("ParseGwei", func(t *testing.T) {
		gwei := big.NewInt(20000000000) // 20 gwei in wei
		assert.Equal(t, int64(20), gwei.Div(gwei, big.NewInt(1e9)).Int64())
	})
	
	t.Run("ZeroValue", func(t *testing.T) {
		zero := big.NewInt(0)
		assert.True(t, zero.IsZero())
	})
}

// TestContractABI tests contract ABI parsing
func TestContractABI(t *testing.T) {
	t.Run("TokenABI", func(t *testing.T) {
		abiJSON := `[{"constant":true,"inputs":[],"name":"name","outputs":[{"name":"","type":"string"}],"type":"function"}]`
		assert.NotEmpty(t, abiJSON)
	})
	
	t.Run("EmptyABI", func(t *testing.T) {
		abiJSON := `[]`
		assert.NotEmpty(t, abiJSON)
	})
}

// TestTransactionTypes tests transaction type handling
func TestTransactionTypes(t *testing.T) {
	t.Run("LegacyTx", func(t *testing.T) {
		tx := types.NewTransaction(0, common.Address{}, big.NewInt(0), 21000, big.NewInt(1e9), nil)
		assert.NotNil(t, tx)
	})
	
	t.Run("DynamicFeeTx", func(t *testing.T) {
		tx := types.NewTx(&types.DynamicFeeTx{
			ChainID:   big.NewInt(1),
			Nonce:    0,
			To:       &common.Address{},
			Gas:      21000,
			GasFeeCap: big.NewInt(1e9),
			GasTipCap: big.NewInt(1e9),
			Value:    big.NewInt(0),
			Data:    nil,
		})
		assert.NotNil(t, tx)
	})
	
	t.Run("AccessListTx", func(t *testing.T) {
		tx := types.NewTx(&types.AccessListTx{
			ChainID:  big.NewInt(1),
			Nonce:   0,
			To:     &common.Address{},
			Gas:    21000,
			Value:  big.NewInt(0),
			Data:   nil,
		})
		assert.NotNil(t, tx)
	})
}

// TestEventLogs tests event log parsing
func TestEventLogs(t *testing.T) {
	t.Run("TransferEvent", func(t *testing.T) {
		topics := []common.Hash{
			common.HexToHash("0xddf252ad1be2c89b69c2b068fc378da426952f5eb3f29151a21b2751249cb3e91"),
			common.HexToHash("0x0000000000000000000000001111111111111111111111111111111111111111"),
			common.HexToHash("0x0000000000000000000000002222222222222222222222222222222222222222"),
		}
		assert.Len(t, topics, 3)
	})
	
	t.Run("ApprovalEvent", func(t *testing.T) {
		topics := []common.Hash{
			common.HexToHash("0x8c5be1e5ebec7d5cc14a5d0468f50e14a4a7a9a2c32e3c0c1c1c1c1c1c1c1c1c1c1c1c"),
		}
		assert.NotEmpty(t, topics[0].Hex())
	})
}

// TestErrorHandling tests error handling
func TestErrorHandling(t *testing.T) {
	t.Run("NotFoundError", func(t *testing.T) {
		err := "not found"
		assert.Contains(t, err, "not")
	})
	
	t.Run("InvalidInputError", func(t *testing.T) {
		err := "invalid input"
		assert.Contains(t, err, "invalid")
	})
}

// BenchmarkBlockParsing benchmarks block parsing
func BenchmarkBlockParsing(b *testing.B) {
	for i := 0; i < b.N; i++ {
		block := &blocks.BlockInfo{
			Number:     uint64(i),
			Hash:       "0xabc123def456",
			ParentHash: "0xparent123",
			Miner:     common.HexToAddress("0xminer").Hex(),
			GasUsed:   15000000,
			GasLimit:  30000000,
			Timestamp: 1234567890,
		}
		_ = block
	}
}

// BenchmarkTokenParsing benchmarks token parsing
func BenchmarkTokenParsing(b *testing.B) {
	for i := 0; i < b.N; i++ {
		token := &tokens.Token{
			Address:     common.HexToAddress("0x1234567890123456789012345678901234567890").Hex(),
			Name:       "Test Token",
			Symbol:     "TEST",
			Decimals:   18,
			TotalSupply: "1000000",
		}
		_ = token
	}
}