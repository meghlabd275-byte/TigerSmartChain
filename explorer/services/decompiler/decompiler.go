// Package decompiler provides EVM bytecode disassembly and basic decompilation services
// This service converts raw bytecode into human-readable EVM opcodes and provides basic structural analysis
package decompiler

import (
	"encoding/hex"
	"fmt"
	"strings"
)

// EVM Opcodes map with mnemonics
var OpCodes = map[byte]string{
	0x00: "STOP", 0x01: "ADD", 0x02: "MUL", 0x03: "SUB", 0x04: "DIV",
	0x05: "SDIV", 0x06: "MOD", 0x07: "SMOD", 0x08: "ADDMOD", 0x09: "MULMOD",
	0x0a: "EXP", 0x0b: "SIGNEXTEND", 0x10: "LT", 0x11: "GT", 0x12: "SLT",
	0x13: "SGT", 0x14: "EQ", 0x15: "ISZERO", 0x16: "AND", 0x17: "OR",
	0x18: "XOR", 0x19: "NOT", 0x1a: "BYTE", 0x1b: "SHL", 0x1c: "SHR",
	0x1d: "SAR", 0x20: "SHA3", 0x30: "ADDRESS", 0x31: "BALANCE", 0x32: "ORIGIN",
	0x33: "CALLER", 0x34: "CALLVALUE", 0x35: "CALLDATASIZE", 0x36: "CALLDATALOAD",
	0x37: "CALLDATACOPY", 0x38: "CODESIZE", 0x39: "CODECOPY", 0x3a: "GASPRICE",
	0x3b: "EXTCODESIZE", 0x3c: "EXTCODECOPY", 0x3d: "RETURNDATASIZE",
	0x3e: "RETURNDATACOPY", 0x3f: "EXTCODEHASH", 0x40: "BLOCKHASH",
	0x41: "COINBASE", 0x42: "TIMESTAMP", 0x43: "NUMBER", 0x44: "DIFFICULTY",
	0x45: "GASLIMIT", 0x46: "CHAINID", 0x47: "BASEFEE", 0x48: "BLOBHASH",
	0x49: "BLOBBASEFEE", 0x50: "POP", 0x51: "MLOAD", 0x52: "MSTORE",
	0x53: "MSTORE8", 0x54: "SLOAD", 0x55: "SSTORE", 0x56: "JUMP",
	0x57: "JUMPI", 0x58: "PC", 0x59: "MSIZE", 0x5a: "GAS", 0x5b: "JUMPDEST",
	0x60: "PUSH1", 0x61: "PUSH2", 0x62: "PUSH3", 0x63: "PUSH4", 0x64: "PUSH5",
	0x65: "PUSH6", 0x66: "PUSH7", 0x67: "PUSH8", 0x68: "PUSH9", 0x69: "PUSH10",
	0x6a: "PUSH11", 0x6b: "PUSH12", 0x6c: "PUSH13", 0x6d: "PUSH14", 0x6e: "PUSH15",
	0x6f: "PUSH16", 0x70: "PUSH17", 0x71: "PUSH18", 0x72: "PUSH19", 0x73: "PUSH20",
	0x74: "PUSH21", 0x75: "PUSH22", 0x76: "PUSH23", 0x77: "PUSH24", 0x78: "PUSH25",
	0x79: "PUSH26", 0x7a: "PUSH27", 0x7b: "PUSH28", 0x7c: "PUSH29", 0x7d: "PUSH30",
	0x7e: "PUSH31", 0x7f: "PUSH32", 0x80: "DUP1", 0x81: "DUP2", 0x82: "DUP3",
	0x83: "DUP4", 0x84: "DUP5", 0x85: "DUP6", 0x86: "DUP7", 0x87: "DUP8",
	0x88: "DUP9", 0x89: "DUP10", 0x8a: "DUP11", 0x8b: "DUP12", 0x8c: "DUP13",
	0x8d: "DUP14", 0x8e: "DUP15", 0x8f: "DUP16", 0x90: "SWAP1", 0x91: "SWAP2",
	0x92: "SWAP3", 0x93: "SWAP4", 0x94: "SWAP5", 0x95: "SWAP6", 0x96: "SWAP7",
	0x97: "SWAP8", 0x98: "SWAP9", 0x99: "SWAP10", 0x9a: "SWAP11", 0x9b: "SWAP12",
	0x9c: "SWAP13", 0x9d: "SWAP14", 0x9e: "SWAP15", 0x9f: "SWAP16", 0xa0: "LOG0",
	0xa1: "LOG1", 0xa2: "LOG2", 0xa3: "LOG3", 0xa4: "LOG4", 0xf0: "CREATE",
	0xf1: "CALL", 0xf2: "CALLCODE", 0xf3: "RETURN", 0xf4: "DELEGATECALL",
	0xf5: "CREATE2", 0xfa: "STATICCALL", 0xfd: "REVERT", 0xfe: "INVALID",
	0xff: "SELFDESTRUCT",
}

// isPushOp checks if opcode is a PUSH instruction
func isPushOp(code byte) (bool, int) {
	if code >= 0x60 && code <= 0x7f {
		return true, int(code-0x60+1)
	}
	return false, 0
}

// DisassembledInstruction represents a disassembled EVM instruction
type DisassembledInstruction struct {
	PC       uint64 `json:"pc"`
	OpCode   byte  `json:"opcode"`
	Mnemonic string `json:"mnemonic"`
	PushData string `json:"pushData,omitempty"`
}

// DisassemblyResult contains the full disassembly
type DisassemblyResult struct {
	Bytecode  string                   `json:"bytecode"`
	Length   int                      `json:"length"`
	Ops      []DisassembledInstruction `json:"ops"`
	Functions []FunctionSignature     `json:"functions,omitempty"`
}

// FunctionSignature represents an identified function
type FunctionSignature struct {
	EntryPC   uint64 `json:"entryPc"`
	Selector string `json:"selector"`
	Inputs   int    `json:"inputs"`
	Name     string `json:"name,omitempty"`
}

// Decompile converts raw bytecode to EVM opcodes
func Decompile(bytecodeHex string) (*DisassemblyResult, error) {
	bytecodeHex = strings.TrimPrefix(bytecodeHex, "0x")
	
	bytecode, err := hex.DecodeString(bytecodeHex)
	if err != nil {
		return nil, fmt.Errorf("invalid hex bytecode: %w", err)
	}
	
	if len(bytecode) == 0 {
		return &DisassemblyResult{Bytecode: bytecodeHex, Length: 0, Ops: []DisassembledInstruction{}}, nil
	}
	
	result := &DisassemblyResult{
		Bytecode: bytecodeHex,
		Length:  len(bytecode),
		Ops:    make([]DisassembledInstruction, 0, len(bytecode)),
	}
	
	var pushData []byte
	pc := uint64(0)
	
	for pc < uint64(len(bytecode)) {
		code := bytecode[pc]
		
		isPush, bytesToPush := isPushOp(code)
		if isPush {
			pushEnd := pc + 1 + uint64(bytesToPush)
			if pushEnd > uint64(len(bytecode)) {
				pushData = bytecode[pc+1:]
			} else {
				pushData = bytecode[pc+1:pushEnd]
			}
			
			result.Ops = append(result.Ops, DisassembledInstruction{
				PC:       pc,
				OpCode:   code,
				Mnemonic: fmt.Sprintf("PUSH%d", bytesToPush),
				PushData: hex.EncodeToString(pushData),
			})
			pc = pushEnd
			continue
		}
		
		mnemonic, ok := OpCodes[code]
		if !ok {
			mnemonic = "UNKNOWN"
		}
		
		result.Ops = append(result.Ops, DisassembledInstruction{
			PC:      pc,
			OpCode:  code,
			Mnemonic: mnemonic,
		})
		
		pc++
	}
	
	result.Functions = identifyFunctions(bytecode)
	return result, nil
}

// identifyFunctions finds common function entry points
func identifyFunctions(bytecode []byte) []FunctionSignature {
	var functions []FunctionSignature
	
	knownSelectors := map[string]string{
		"a0e8a2a5": "transfer(address,uint256)",
		"a9059cbb": "transfer(address,uint256)",
		"23b872dd": "transferFrom(address,address,uint256)",
		"095ea7b3": "approve(address,uint256)",
		"dd62ed3e": "allowance(address,address)",
		"70a08231": "balanceOf(address)",
		"313ce567": "decimals()",
		"18160ddd": "totalSupply()",
	}
	
	for selector, signature := range knownSelectors {
		selBytes, _ := hex.DecodeString(selector)
		for i := 0; i < len(bytecode)-4; i++ {
			if i+4 < len(bytecode) && bytesEqual(bytecode[i:i+4], selBytes) {
				functions = append(functions, FunctionSignature{
					EntryPC:   uint64(i),
					Selector: "0x" + selector,
					Inputs:   countInputs(signature),
					Name:     signature,
				})
			}
		}
	}
	
	return functions
}

func bytesEqual(a, b []byte) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func countInputs(sig string) int {
	count := 0
	for _, c := range sig {
		if c == ',' {
			count++
		}
	}
	return count + 1
}

// StorageVariable represents an identified storage variable
type StorageVariable struct {
	Slot uint64 `json:"slot"`
	Name string `json:"name"`
	Type string `json:"type,omitempty"`
}

// GetStorageLayout determines storage variable locations
func GetStorageLayout(bytecodeHex string) ([]StorageVariable, error) {
	bytecodeHex = strings.TrimPrefix(bytecodeHex, "0x")
	bytecode, err := hex.DecodeString(bytecodeHex)
	if err != nil {
		return nil, err
	}
	
	var vars []StorageVariable
	pc := uint64(0)
	slotNum := 0
	for pc < uint64(len(bytecode)) {
		if bytecode[pc] == 0x54 {
			vars = append(vars, StorageVariable{
				Slot: uint64(slotNum),
				Name: fmt.Sprintf("storage_%d", slotNum),
			})
			slotNum++
		}
		pc++
	}
	
	return vars, nil
}

// ContractAnalysis contains analysis results
type ContractAnalysis struct {
	BytecodeLength int                `json:"bytecodeLength"`
	FunctionCount int                 `json:"functionCount"`
	OpCount       int                 `json:"opCount"`
	OpcodeCounts  map[string]int      `json:"opcodeCounts"`
	CreatesContracts bool            `json:"createsContracts"`
	ExternalCalls bool               `json:"externalCalls"`
	SelfDestructs bool               `json:"selfDestructs"`
	UsesDelegateCall bool            `json:"usesDelegateCall"`
}

// AnalyzeContract provides basic contract analysis
func AnalyzeContract(bytecodeHex string) (*ContractAnalysis, error) {
	result, err := Decompile(bytecodeHex)
	if err != nil {
		return nil, err
	}
	
	analysis := &ContractAnalysis{
		BytecodeLength: result.Length,
		FunctionCount:  len(result.Functions),
		OpCount:        len(result.Ops),
		OpcodeCounts:   make(map[string]int),
	}
	
	for _, op := range result.Ops {
		analysis.OpcodeCounts[op.Mnemonic]++
	}
	
	for _, op := range result.Ops {
		if op.Mnemonic == "CREATE" || op.Mnemonic == "CREATE2" {
			analysis.CreatesContracts = true
		}
		if op.Mnemonic == "CALL" || op.Mnemonic == "STATICCALL" {
			analysis.ExternalCalls = true
		}
		if op.Mnemonic == "SELFDESTRUCT" {
			analysis.SelfDestructs = true
		}
		if op.Mnemonic == "DELEGATECALL" {
			analysis.UsesDelegateCall = true
		}
	}
	
	return analysis, nil
}