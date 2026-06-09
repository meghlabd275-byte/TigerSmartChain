// Package interpreter provides the EVM interpreter for TigerSmartChain.
package interpreter

import (
	"math/big"

	"github.com/tigersmartchain/tigersmartchain/internal/state"
	"github.com/tigersmartchain/tigersmartchain/pkg/types"
)

// VMContext is the execution context.
type VMContext struct {
	Caller     types.Address
	Origin    types.Address
	Gas        *big.Int
	Value      *big.Int
	Input      []byte
	ChainID    uint64
	BlockNum   uint64
	Timestamp  uint64
	BaseFee    *big.Int
	Coinbase   types.Address
	State     *state.StateDB
}

// Result is the execution result.
type Result struct {
	Output   []byte
	GasUsed uint64
	Err     error
	Reverted bool
}

// Interpreter executes EVM bytecode.
type Interpreter struct {
	state      *state.StateDB
	chainConfig *types.ChainConfig
	static    bool
}

// NewInterpreter creates a new interpreter.
func NewInterpreter(stateDB *state.StateDB, chainConfig *types.ChainConfig) *Interpreter {
	return &Interpreter{
		state:      stateDB,
		chainConfig: chainConfig,
	}
}

// SetStatic sets static mode.
func (in *Interpreter) SetStatic(static bool) {
	in.static = static
}

// Run executes the code.
func (in *Interpreter) Run(ctx *VMContext, code []byte) *Result {
	return &Result{
		Output:   []byte{},
		GasUsed:  0,
		Reverted: false,
	}
}

// Stack represents the EVM stack.
type Stack struct {
	data []*big.Int
}

func newStack() *Stack {
	return &Stack{data: make([]*big.Int, 0, 1024)}
}

func (s *Stack) Push(val *big.Int) {
	s.data = append(s.data, new(big.Int).Set(val))
}

func (s *Stack) Pop() *big.Int {
	if len(s.data) == 0 {
		return big.NewInt(0)
	}
	val := s.data[len(s.data)-1]
	s.data = s.data[:len(s.data)-1]
	return val
}

func (s *Stack) Dup(depth int) {
	if len(s.data) < depth {
		return
	}
	s.data = append(s.data, new(big.Int).Set(s.data[len(s.data)-depth]))
}

func (s *Stack) Swap(depth int) {
	if len(s.data) <= depth {
		return
	}
	i := len(s.data) - 1 - depth
	j := len(s.data) - 1
	s.data[i], s.data[j] = s.data[j], s.data[i]
}

func (s *Stack) Len() int {
	return len(s.data)
}