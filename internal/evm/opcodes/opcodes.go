// Package opcodes provides EVM opcodes for TigerSmartChain.
package opcodes

// Opcode represents an EVM opcode.
type Opcode byte

// EVM opcodes
const (
	STOP       Opcode = 0x00
	ADD        Opcode = 0x01
	MUL        Opcode = 0x02
	SUB        Opcode = 0x03
	DIV        Opcode = 0x04
	SDIV       Opcode = 0x05
	MOD        Opcode = 0x06
	SMOD       Opcode = 0x07
	ADDMOD     Opcode = 0x08
	MULMOD     Opcode = 0x09
	EXP        Opcode = 0x0a
	SIGNEXTEND Opcode = 0x0b

	LT     Opcode = 0x10
	GT     Opcode = 0x11
	SLT    Opcode = 0x12
	SGT    Opcode = 0x13
	EQ     Opcode = 0x14
	ISZERO Opcode = 0x15
	AND    Opcode = 0x16
	OR     Opcode = 0x17
	XOR    Opcode = 0x18
	NOT    Opcode = 0x19
	BYTE   Opcode = 0x1a
	SHL    Opcode = 0x1b
	SHR    Opcode = 0x1c
	SAR    Opcode = 0x1d

	KECCAK256 Opcode = 0x20

	ADDRESS      Opcode = 0x30
	BALANCE     Opcode = 0x31
	ORIGIN      Opcode = 0x32
	CALLER      Opcode = 0x33
	CALLVALUE   Opcode = 0x34
	CALLDATALOAD Opcode = 0x35
	CALLDATASIZE Opcode = 0x36
	CALLDATACOPY Opcode = 0x37
	RETURNDATASIZE Opcode = 0x3d
	RETURNDATACOPY Opcode = 0x3e
	EXTCODESIZE Opcode = 0x3b
	EXTCODECOPY Opcode = 0x3c
	EXTCODEHASH Opcode = 0x3f

	GASPRICE Opcode = 0x3a
	CHAINID Opcode = 0x46
	BASEFEE Opcode = 0x48

	POP      Opcode = 0x50
	MLOAD    Opcode = 0x51
	MSTORE   Opcode = 0x52
	MSTORE8 Opcode = 0x53
	JUMP    Opcode = 0x56
	JUMPI   Opcode = 0x57
	PC      Opcode = 0x58
	MSIZE   Opcode = 0x59
	GAS     Opcode = 0x5a
	JUMPDEST Opcode = 0x5b

	PUSH1  Opcode = 0x60
	PUSH2  Opcode = 0x61
	PUSH3  Opcode = 0x62
	PUSH4  Opcode = 0x63
	PUSH5  Opcode = 0x64
	PUSH6  Opcode = 0x65
	PUSH7  Opcode = 0x66
	PUSH8  Opcode = 0x67
	PUSH9  Opcode = 0x68
	PUSH10 Opcode = 0x69
	PUSH11 Opcode = 0x6a
	PUSH12 Opcode = 0x6b
	PUSH13 Opcode = 0x6c
	PUSH14 Opcode = 0x6d
	PUSH15 Opcode = 0x6e
	PUSH16 Opcode = 0x6f
	PUSH17 Opcode = 0x70
	PUSH18 Opcode = 0x71
	PUSH19 Opcode = 0x72
	PUSH20 Opcode = 0x73
	PUSH21 Opcode = 0x74
	PUSH22 Opcode = 0x75
	PUSH23 Opcode = 0x76
	PUSH24 Opcode = 0x77
	PUSH25 Opcode = 0x78
	PUSH26 Opcode = 0x79
	PUSH27 Opcode = 0x7a
	PUSH28 Opcode = 0x7b
	PUSH29 Opcode = 0x7c
	PUSH30 Opcode = 0x7d
	PUSH31 Opcode = 0x7e
	PUSH32 Opcode = 0x7f

	DUP1  Opcode = 0x80
	DUP2  Opcode = 0x81
	DUP3  Opcode = 0x82
	DUP4  Opcode = 0x83
	DUP5  Opcode = 0x84
	DUP6  Opcode = 0x85
	DUP7  Opcode = 0x86
	DUP8  Opcode = 0x87
	DUP9  Opcode = 0x88
	DUP10 Opcode = 0x89
	DUP11 Opcode = 0x8a
	DUP12 Opcode = 0x8b
	DUP13 Opcode = 0x8c
	DUP14 Opcode = 0x8d
	DUP15 Opcode = 0x8e
	DUP16 Opcode = 0x8f

	SWAP1  Opcode = 0x90
	SWAP2  Opcode = 0x91
	SWAP3  Opcode = 0x92
	SWAP4  Opcode = 0x93
	SWAP5  Opcode = 0x94
	SWAP6  Opcode = 0x95
	SWAP7  Opcode = 0x96
	SWAP8  Opcode = 0x97
	SWAP9  Opcode = 0x98
	SWAP10 Opcode = 0x99
	SWAP11 Opcode = 0x9a
	SWAP12 Opcode = 0x9b
	SWAP13 Opcode = 0x9c
	SWAP14 Opcode = 0x9d
	SWAP15 Opcode = 0x9e
	SWAP16 Opcode = 0x9f

	LOG0 Opcode = 0xa0
	LOG1 Opcode = 0xa1
	LOG2 Opcode = 0xa2
	LOG3 Opcode = 0xa3
	LOG4 Opcode = 0xa4

	CREATE       Opcode = 0xf0
	CALL         Opcode = 0xf1
	CALLCODE     Opcode = 0xf2
	RETURN       Opcode = 0xf3
	DELEGATECALL Opcode = 0xf4
	CREATE2      Opcode = 0xf5

	STATICCALL Opcode = 0xfa

	REVERT Opcode = 0xfd
	SELFDESTRUCT Opcode = 0xff

	INVALID Opcode = 0xfe

	BLOCKHASH Opcode = 0x40
	COINBASE Opcode = 0x41
	TIMESTAMP Opcode = 0x42
	NUMBER  Opcode = 0x43
	DIFFICULTY Opcode = 0x44
	GASLIMIT Opcode = 0x45
	MOVEFEE Opcode = 0x47

	SELFBALANCE Opcode = 0x47

	// EIP-1153: Transient Storage Opcodes
	TLOAD  Opcode = 0x5c
	TSTORE Opcode = 0x5d

	// EIP-5656: MCOPY
	MCOPY Opcode = 0x5e

	// EIP-7702: Set Code for Account Abstraction
	AUTH Opcode = 0xf6
	AUTHCALL Opcode = 0xf7
)

// Name returns the name of the opcode.
func (op Opcode) Name() string {
	names := map[Opcode]string{
		STOP:       "STOP",
		ADD:        "ADD",
		MUL:        "MUL",
		SUB:        "SUB",
		DIV:        "DIV",
		SDIV:       "SDIV",
		MOD:        "MOD",
		SMOD:       "SMOD",
		ADDMOD:     "ADDMOD",
		MULMOD:     "MULMOD",
		EXP:        "EXP",
		SIGNEXTEND:  "SIGNEXTEND",
		LT:        "LT",
		GT:        "GT",
		SLT:       "SLT",
		SGT:       "SGT",
		EQ:        "EQ",
		ISZERO:    "ISZERO",
		AND:       "AND",
		OR:        "OR",
		XOR:       "XOR",
		NOT:       "NOT",
		BYTE:      "BYTE",
		SHL:       "SHL",
		SHR:       "SHR",
		SAR:       "SAR",
		KECCAK256:  "KECCAK256",
		ADDRESS:   "ADDRESS",
		BALANCE:   "BALANCE",
		ORIGIN:   "ORIGIN",
		CALLER:   "CALLER",
		CALLVALUE: "CALLVALUE",
		CALLDATALOAD: "CALLDATALOAD",
		CALLDATASIZE: "CALLDATASIZE",
		CALLDATACOPY: "CALLDATACOPY",
		RETURNDATASIZE: "RETURNDATASIZE",
		RETURNDATACOPY: "RETURNDATACOPY",
		EXTCODESIZE: "EXTCODESIZE",
		EXTCODECOPY: "EXTCODECOPY",
		EXTCODEHASH: "EXTCODEHASH",
		GASPRICE: "GASPRICE",
		CHAINID: "CHAINID",
		BASEFEE: "BASEFEE",
		POP:     "POP",
		MLOAD:   "MLOAD",
		MSTORE:  "MSTORE",
		MSTORE8: "MSTORE8",
		JUMP:    "JUMP",
		JUMPI:   "JUMPI",
		PC:      "PC",
		MSIZE:   "MSIZE",
		GAS:     "GAS",
		JUMPDEST: "JUMPDEST",
		CREATE:       "CREATE",
		CALL:         "CALL",
		CALLCODE:     "CALLCODE",
		RETURN:       "RETURN",
		DELEGATECALL: "DELEGATECALL",
		CREATE2:     "CREATE2",
		STATICCALL:    "STATICCALL",
		REVERT:       "REVERT",
		SELFDESTRUCT:  "SELFDESTRUCT",
		INVALID:     "INVALID",
		BLOCKHASH:   "BLOCKHASH",
		COINBASE:    "COINBASE",
		TIMESTAMP:   "TIMESTAMP",
		NUMBER:      "NUMBER",
		DIFFICULTY:  "DIFFICULTY",
		GASLIMIT:    "GASLIMIT",
		SELFBALANCE: "SELFBALANCE",
	}

	name, ok := names[op]
	if !ok {
		// Handle PUSH, DUP, SWAP, LOG
		if op >= PUSH1 && op <= PUSH32 {
			return "PUSH"
		}
		if op >= DUP1 && op <= DUP16 {
			return "DUP"
		}
		if op >= SWAP1 && op <= SWAP16 {
			return "SWAP"
		}
		if op >= LOG0 && op <= LOG4 {
			return "LOG"
		}
		return "UNKNOWN"
	}
	return name
}

// IsPush returns true if this is a PUSH opcode.
func (op Opcode) IsPush() bool {
	return op >= PUSH1 && op <= PUSH32
}

// IsDup returns true if this is a DUP opcode.
func (op Opcode) IsDup() bool {
	return op >= DUP1 && op <= DUP16
}

// IsSwap returns true if this is a SWAP opcode.
func (op Opcode) IsSwap() bool {
	return op >= SWAP1 && op <= SWAP16
}

// IsLog returns true if this is a LOG opcode.
func (op Opcode) IsLog() bool {
	return op >= LOG0 && op <= LOG4
}

// StackHeightChange returns the stack height change for this opcode.
func (op Opcode) StackHeightChange() int {
	switch op {
	case STOP, INVALID, REVERT, RETURN:
		return 0
	case ADD, SUB, MUL, DIV, SDIV, MOD, SMOD, ADDMOD, MULMOD, EXP, SIGNEXTEND:
		return -1
	case LT, GT, SLT, SGT, EQ, ISZERO, AND, OR, XOR, NOT, BYTE, SHL, SHR, SAR:
		return -1
	case KECCAK256:
		return -1 + 2
	case ADDRESS, ORIGIN, CALLER, CALLVALUE, CALLDATASIZE, GASPRICE, CHAINID, BASEFEE:
		return +1
	case BALANCE, EXTCODESIZE, EXTCODEHASH, RETURNDATASIZE, SELFBALANCE:
		return -1 + 1
	case POP:
		return -1
	case MLOAD, MSTORE, MSTORE8:
		return -2 + 1
	case JUMP, JUMPI, PC:
		return 0
	case MSIZE, GAS:
		return +1
	case JUMPDEST:
		return 0
	case PUSH1, PUSH2, PUSH3, PUSH4, PUSH5, PUSH6, PUSH7, PUSH8, PUSH9, PUSH10, PUSH11, PUSH12, PUSH13, PUSH14, PUSH15, PUSH16, PUSH17, PUSH18, PUSH19, PUSH20, PUSH21, PUSH22, PUSH23, PUSH24, PUSH25, PUSH26, PUSH27, PUSH28, PUSH29, PUSH30, PUSH31, PUSH32:
		return +1
	case DUP1, DUP2, DUP3, DUP4, DUP5, DUP6, DUP7, DUP8, DUP9, DUP10, DUP11, DUP12, DUP13, DUP14, DUP15, DUP16:
		return +1
	case SWAP1, SWAP2, SWAP3, SWAP4, SWAP5, SWAP6, SWAP7, SWAP8, SWAP9, SWAP10, SWAP11, SWAP12, SWAP13, SWAP14, SWAP15, SWAP16:
		return 0
	case LOG0, LOG1, LOG2, LOG3, LOG4:
		return -2
	case CREATE:
		return -2 + 1
	case CALL, CALLCODE, DELEGATECALL, STATICCALL:
		return -3 + 1
	case CREATE2:
		return -3 + 1
	case SELFDESTRUCT:
		return -1
	case BLOCKHASH, COINBASE, TIMESTAMP, NUMBER, DIFFICULTY, GASLIMIT:
		return +1
	default:
		return 0
	}
}