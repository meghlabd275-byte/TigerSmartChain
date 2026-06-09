// Package gas_meter provides gas calculations for EVM execution.
package gas_meter

import "math/big"

// GasPool manages available gas for block execution.
type GasPool struct {
	gas uint64
}

func NewGasPool(gas uint64) *GasPool {
	return &GasPool{gas: gas}
}

func (gp *GasPool) Gas() uint64 {
	return gp.gas
}

func (gp *GasPool) SubGas(amount uint64) error {
	if gp.gas < amount {
		return ErrGasLimitReached
	}
	gp.gas -= amount
	return nil
}

func (gp *GasPool) AddGas(amount uint64) {
	gp.gas += amount
}

var ErrGasLimitReached = &GasError{"gas limit reached"}

type GasError struct {
	msg string
}

func (e *GasError) Error() string {
	return e.msg
}

// Gas costs
const (
	GasStep       uint64 = 1
	GasStop       uint64 = 0
	GasMemory     uint64 = 3
	GasBalance    uint64 = 20
	GasSLoad     uint64 = 20
	GasMload     uint64 = 3
	GasMstore    uint64 = 3
	GasCreate    uint64 = 32000
	GasCall      uint64 = 20
	GasCallValue uint64 = 9000
	GasLog       uint64 = 375
	GasLogTopic  uint64 = 375
	GasLogData   uint64 = 8
)

func CalcGasCost(op string, memory uint64, stackSize int) uint64 {
	switch op {
	case "STOP":
		return 0
	case "ADD", "SUB", "MUL", "DIV":
		return 3
	case "LT", "GT", "EQ", "ISZERO":
		return 3
	case "MLOAD", "MSTORE":
		return 3 + (memory+31)/32*GasMemory
	case "JUMP", "JUMPI":
		return 8
	case "POP":
		return 2
	case "PUSH1", "PUSH2", "PUSH3", "PUSH4", "PUSH5", "PUSH6", "PUSH7", "PUSH8", "PUSH9", "PUSH10", "PUSH11", "PUSH12", "PUSH13", "PUSH14", "PUSH15", "PUSH16", "PUSH17", "PUSH18", "PUSH19", "PUSH20", "PUSH21", "PUSH22", "PUSH23", "PUSH24", "PUSH25", "PUSH26", "PUSH27", "PUSH28", "PUSH29", "PUSH30", "PUSH31", "PUSH32":
		return 3
	case "DUP1", "DUP2", "DUP3", "DUP4", "DUP5", "DUP6", "DUP7", "DUP8", "DUP9", "DUP10", "DUP11", "DUP12", "DUP13", "DUP14", "DUP15", "DUP16":
		return 3
	case "SWAP1", "SWAP2", "SWAP3", "SWAP4", "SWAP5", "SWAP6", "SWAP7", "SWAP8", "SWAP9", "SWAP10", "SWAP11", "SWAP12", "SWAP13", "SWAP14", "SWAP15", "SWAP16":
		return 3
	case "RETURN":
		return 0
	case "REVERT":
		return 0
	case "INVALID":
		return 0
	case "ADDRESS", "ORIGIN", "CALLER", "CALLVALUE", "GASPRICE", "CHAINID", "BASEFEE":
		return 2
	case "BALANCE":
		return GasBalance
	case "SLOAD":
		return GasSLoad
	case "CREATE":
		return GasCreate
	case "CALL", "CALLCODE", "DELEGATECALL", "STATICCALL":
		return GasCall
	case "LOG0", "LOG1", "LOG2", "LOG3", "LOG4":
		return GasLog
	case "SELFDESTRUCT":
		return 0
	case "BLOCKHASH", "COINBASE", "TIMESTAMP", "NUMBER", "DIFFICULTY", "GASLIMIT":
		return 2
	default:
		return 3
	}
}

func CalcCallGas(gas uint64, memory uint64, value *big.Int) uint64 {
	calledGas := gas - gas/64
	if value != nil && value.Sign() > 0 {
		calledGas -= GasCallValue
	}
	return calledGas
}