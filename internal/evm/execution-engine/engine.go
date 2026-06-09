// Package evm provides the EVM execution engine for TigerSmartChain.
package evm

import (
	"fmt"
	"math/big"
	"strings"

	"github.com/tigersmartchain/tigersmartchain/internal/blockchain/transaction"
	"github.com/tigersmartchain/tigersmartchain/internal/evm/gas-meter"
	"github.com/tigersmartchain/tigersmartchain/internal/evm/interpreter"
	"github.com/tigersmartchain/tigersmartchain/internal/evm/precompiles"
	"github.com/tigersmartchain/tigersmartchain/internal/state"
	"github.com/tigersmartchain/tigersmartchain/pkg/types"
)

// VM represents the Ethereum Virtual Machine.
type VM struct {
	state      *state.StateDB
	chainConfig *types.ChainConfig
	gasPool    *gas_meter.GasPool
}

// New creates a new EVM instance.
func New(stateDB *state.StateDB, chainConfig *types.ChainConfig, gasLimit uint64) *VM {
	return &VM{
		state:      stateDB,
		chainConfig: chainConfig,
		gasPool:    gas_meter.NewGasPool(gasLimit),
	}
}

// Execute executes a contract and returns the result.
func (vm *VM) Execute(ctx *interpreter.VMContext, code []byte) ([]byte, error) {
	intrp := interpreter.NewInterpreter(vm.state, vm.chainConfig)
	result := intrp.Run(ctx, code)
	return result.Output, result.Err
}

// Result represents the execution result.
type Result struct {
	Output   []byte
	Err      error
	GasUsed uint64
	Reverted bool
}

// ExecuteTransaction executes a transaction.
func ExecuteTransaction(
	tx *transaction.Transaction,
	chainConfig *types.ChainConfig,
	stateDB *state.StateDB,
) (*Result, error) {
	if err := ValidateTransaction(tx, chainConfig); err != nil {
		return nil, err
	}

	sender := stateDB.GetAccount(tx.From)
	if sender == nil {
		return nil, fmt.Errorf("sender account not found")
	}

	gasCost := new(big.Int).Mul(tx.GasPrice, new(big.Int).SetUint64(tx.Gas))
	if sender.Balance.Cmp(gasCost) < 0 {
		return nil, fmt.Errorf("insufficient gas")
	}

	sender.Balance.Sub(sender.Balance, gasCost)

	ctx := &interpreter.VMContext{
		Caller:  tx.From,
		Origin: tx.From,
		Gas:    new(big.Int).SetUint64(tx.Gas),
		Value:  tx.Value,
		Input:  tx.Data,
		State:  stateDB,
	}

	vm := New(stateDB, chainConfig, tx.Gas)
	var result []byte
	var err error

	if tx.To == nil {
		result, err = vm.Execute(ctx, tx.Data)
	} else {
		code := stateDB.GetCode(*tx.To)
		result, err = vm.Execute(ctx, code)
	}

	if err != nil {
		return &Result{
			Output:   result,
			Err:     err,
			Reverted: strings.Contains(err.Error(), "reverted"),
		}, err
	}

	return &Result{Output: result}, nil
}

// ValidateTransaction validates a transaction.
func ValidateTransaction(tx *transaction.Transaction, chainConfig *types.ChainConfig) error {
	if tx.Gas < 21000 {
		return fmt.Errorf("gas too low: minimum 21000")
	}
	if tx.Value == nil {
		tx.Value = big.NewInt(0)
	}
	return nil
}

// Precompiles returns precompile addresses.
func Precompiles() []types.Address {
	return precompiles.ContractAddresses
}