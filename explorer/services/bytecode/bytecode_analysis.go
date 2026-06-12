// Package bytecode provides bytecode analysis for contracts.
package bytecode

import (
	"encoding/hex"
	"fmt"
	"strings"
)

// AnalysisResult represents bytecode analysis result
type AnalysisResult struct {
	ContractAddress  string   `json:"contractAddress"`
	Bytecode        string   `json:"bytecode"`
	RuntimeBytecode  string   `json:"runtimeBytecode"`
	OpCodes         []string `json:"opCodes"`
	Functions      []Function `json:"functions"`
	Libraries     []string `json:"libraries"`
	IsProxy       bool     `json:"isProxy"`
	Implementation string   `json:"implementation,omitempty"`
	ProxyType     string   `json:"proxyType,omitempty"`
	Salt         string   `json:"salt,omitempty"`
	Deployer     string   `json:"deployer,omitempty"`
	CreatedViaCreate2 bool   `json:"createdViaCreate2"`
	Match        bool    `json:"match"`
}

// Function represents a detected function
type Function struct {
	Signature string `json:"signature"`
	Selector  string `json:"selector"`
	Type      string `json:"type"` // "pure", "view", "nonpayable", "payable"
}

// Service provides bytecode analysis
type Service struct {
	// EVM opcodes map
	opcodes map[string]string
}

// NewService creates a new bytecode analysis service
func NewService() *Service {
	s := &Service{
		opcodes: map[string]string{
			"00": "STOP", "01": "ADD", "02": "MUL", "03": "SUB", "04": "DIV",
			"05": "SDIV", "06": "MOD", "07": "SMOD", "08": "ADDMOD",
			"09": "MULMOD", "0a": "EXP", "0b": "SIGNEXTEND", "10": "LT",
			"11": "GT", "12": "SLT", "13": "SGT", "14": "EQ", "15": "ISZERO",
			"16": "AND", "17": "OR", "18": "XOR", "19": "NOT", "1a": "BYTE",
			"1b": "SHL", "1c": "SHR", "1d": "SAR", "20": "SHA3",
			"30": "ADDRESS", "31": "BALANCE", "32": "ORIGIN", "33": "CALLER",
			"34": "CALLVALUE", "35": "CALLDATASIZE", "36": "CALLDATALOAD",
			"37": "CALLDATACOPY", "38": "CODESIZE", "39": "CODECOPY",
			"3a": "GASPRICE", "3b": "EXTCODESIZE", "3c": "EXTCODECOPY",
			"3d": "RETURNDATASIZE", "3e": "RETURNDATACOPY", "3f": "EXTCODEHASH",
			"40": "BLOCKHASH", "41": "COINBASE", "42": "TIMESTAMP", "43": "NUMBER",
			"44": "DIFFICULTY", "45": "GASLIMIT", "46": "CHAINID", "47": "BASEFEE",
			"50": "POP", "51": "MLOAD", "52": "MSTORE", "53": "MSTORE8",
			"54": "SLOAD", "55": "SSTORE", "56": "JUMP", "57": "JUMPI",
			"58": "PC", "59": "MSIZE", "5a": "GAS", "5b": "JUMPDEST",
			"60": "PUSH1", "61": "PUSH2", "62": "PUSH3", "63": "PUSH4",
			"64": "PUSH5", "65": "PUSH6", "66": "PUSH7", "67": "PUSH8",
			"68": "PUSH9", "69": "PUSH10", "6a": "PUSH11", "6b": "PUSH12",
			"6c": "PUSH13", "6d": "PUSH14", "6e": "PUSH15", "6f": "PUSH16",
			"70": "PUSH17", "71": "PUSH18", "72": "PUSH19", "73": "PUSH20",
			"74": "PUSH21", "75": "PUSH22", "76": "PUSH23", "77": "PUSH24",
			"78": "PUSH25", "79": "PUSH26", "7a": "PUSH27", "7b": "PUSH28",
			"7c": "PUSH29", "7d": "PUSH30", "7e": "PUSH31", "7f": "PUSH32",
			"80": "DUP1", "81": "DUP2", "82": "DUP3", "83": "DUP4",
			"84": "DUP5", "85": "DUP6", "86": "DUP7", "87": "DUP8",
			"88": "DUP9", "89": "DUP10", "8a": "DUP11", "8b": "DUP12",
			"8c": "DUP13", "8d": "DUP14", "8e": "DUP15", "8f": "DUP16",
			"90": "SWAP1", "91": "SWAP2", "92": "SWAP3", "93": "SWAP4",
			"94": "SWAP5", "95": "SWAP6", "96": "SWAP7", "97": "SWAP8",
			"98": "SWAP9", "99": "SWAP10", "9a": "SWAP11", "9b": "SWAP12",
			"9c": "SWAP13", "9d": "SWAP14", "9e": "SWAP15", "9f": "SWAP16",
			"a0": "LOG0", "a1": "LOG1", "a2": "LOG2", "a3": "LOG3",
			"a4": "LOG4", "b0": "CREATE", "b1": "CALL", "b2": "CALLCODE",
			"b3": "RETURN", "b4": "DELEGATECALL", "b5": "CREATE2",
			"b6": "STATICCALL", "b7": "REVERT", "b8": "INVALID",
			"b9": "SELFDESTRUCT", "f0": "CREATE", "f1": "CALL",
			"f2": "CALLCODE", "f3": "RETURN", "f4": "DELEGATECALL",
			"f5": "CREATE2", "fa": "STATICCALL", "fd": "REVERT",
			"fe": "INVALID", "ff": "SELFDESTRUCT",
		},
	}
	return s
}

// Analyze performs bytecode analysis
func (s *Service) Analyze(address, bytecode, runtimeBytecode string) (*AnalysisResult, error) {
	result := &AnalysisResult{
		ContractAddress: address,
		Bytecode:       bytecode,
		RuntimeBytecode: runtimeBytecode,
	}

	// Parse opcodes
	if len(bytecode) > 2 {
		result.OpCodes = s.parseOpcodes(strings.TrimPrefix(bytecode, "0x"))
	}

	// Detect functions
	result.Functions = s.detectFunctions(result.OpCodes)

	// Detect libraries
	result.Libraries = s.detectLibraries(bytecode)

	// Check for proxy patterns
	result.IsProxy, result.ProxyType, result.Implementation = s.detectProxy(bytecode, runtimeBytecode)

	// Check for Create2
	result.CreatedViaCreate2 = s.detectCreate2(bytecode)

	// Extract deployer
	result.Deployer = s.extractDeployer(bytecode)

	// Extract salt
	result.Salt = s.extractSalt(bytecode)

	return result, nil
}

// parseOpcodes parses bytecode into opcodes
func (s *Service) parseOpcodes(hexData string) []string {
	data, err := hex.DecodeString(hexData)
	if err != nil {
		return nil
	}

	var opcodes []string
	i := 0

	for i < len(data) {
		op := fmt.Sprintf("%02x", data[i])

		if name, ok := s.opcodes[op]; ok {
			opcodes = append(opcodes, name)

			// Handle push operations
			if op >= "60" && op <= "7f" {
				pushSize := int(op[1]-'0') + 1
				i++
				for j := 0; j < pushSize && i < len(data); j++ {
					i++
				}
			} else {
				i++
			}
		} else {
			i++
		}
	}

	return opcodes
}

// detectFunctions detects functions from bytecode
func (s *Service) detectFunctions(opcodes []string) []Function {
	var functions []Function
	seen := make(map[string]bool)

	for i, op := range opcodes {
		if op == "PUSH4" && i+4 < len(opcodes) {
			selector := ""
			for j := 1; j <= 4; j++ {
				if i+j < len(opcodes) {
					selector += opcodes[i+j]
				}
			}

			if !seen[selector] && len(selector) == 8 {
				seen[selector] = true

				fnType := "nonpayable"
				if i > 0 {
					prev := opcodes[i-1]
					if prev == "CALLVALUE" {
						fnType = "payable"
					}
				}

				functions = append(functions, Function{
					Selector: "0x" + selector,
					Type:     fnType,
				})
			}
		}
	}

	return functions
}

// detectLibraries detects library usage
func (s *Service) detectLibraries(bytecode string) []string {
	data, _ := hex.DecodeString(strings.TrimPrefix(bytecode, "0x"))

	var libraries []string
	libraryAddresses := []string{
		"0000000000000000000000000000000000000000", // Placeholder
	}

	for _, addr := range libraryAddresses {
		if strings.Contains(bytecode, addr) {
			libraries = append(libraries, "0x"+addr)
		}
	}

	// Also detect by Keccak256 placeholder pattern
	if len(data) > 0 {
		// Common library patterns
		libraryHashes := []string{
			"1e974d59", // keccak256("Math")
			"a1d8f8a",  // keccak256("SafeMath")
		}
		for _, hash := range libraryHashes {
			if strings.Contains(bytecode, hash) {
				libraries = append(libraries, hash)
			}
		}
	}

	return libraries
}

// detectProxy detects proxy patterns (EIP-1967)
func (s *Service) detectProxy(bytecode, runtimeBytecode string) (bool, string, string) {
	data, _ := hex.DecodeString(strings.TrimPrefix(runtimeBytecode, "0x"))
	if len(data) < 40 {
		return false, "", ""
	}

	// Check for EIP-1967 storage slots
	// Implementation slot: 0x360894a13ba1a3210667c82849283898f12a5f3a54 (slot 0)
	// Admin slot: 0xb53127684a518b3f83a863c4192a8c2c5ab1d1e4 (slot 1)
	// Beacon slot: 0xa3f3ab5f5c1a1e1a3f3ab5f5c1a1e1a3f3ab5f5c1a1e1 (slot 2)

	implSlot := "360894a13ba1a3210667c82849283898f12a5f3a54"
	adminSlot := "b53127684a518b3f83a863c4192a8c2c5ab1d1e4"
	beaconSlot := "a3f3ab5f5c1a1e1a3f3ab5f5c1a1e1a3f3ab5f5c1a1e1"

	if strings.Contains(bytecode, implSlot) {
		return true, "immutable", ""
	}

	if strings.Contains(bytecode, adminSlot) {
		return true, "proxy", ""
	}

	if strings.Contains(bytecode, beaconSlot) {
		return true, "beacon", ""
	}

	// Check for upgradeable patterns
	// Universal Upgradeable Proxy Standard (UUPS) - EIP-1822
	uupsSlot := "7050c9e3"
	if strings.Contains(bytecode, uupsSlot) {
		return true, "uups", ""
	}

	return false, "", ""
}

// detectCreate2 detects Create2 deployment
func (s *Service) detectCreate2(bytecode string) bool {
	return strings.Contains(bytecode, "5a") // CREATE2 opcode
}

// extractDeployer extracts deployer address
func (s *Service) extractDeployer(bytecode string) string {
	data, _ := hex.DecodeString(strings.TrimPrefix(bytecode, "0x"))

	// Look for CREATE/CREATE2 followed by address
	for i := 0; i < len(data)-21; i++ {
		if (data[i] == 0xf0 || data[i] == 0xf5) && data[i+1] == 0x3d && data[i+2] == 0x33 {
			// Extract address from stack
			addr := make([]byte, 20)
			copy(addr, data[i+3:i+23])
			return "0x" + hex.EncodeToString(addr)
		}
	}

	return ""
}

// extractSalt extracts Create2 salt
func (s *Service) extractSalt(bytecode string) string {
	data, _ := hex.DecodeString(strings.TrimPrefix(bytecode, "0x"))

	// Look for PUSH2 (0x61) before CREATE2 (0xf5)
	for i := 0; i < len(data)-3; i++ {
		if data[i] == 0x61 && data[i+3] == 0xf5 {
			salt := make([]byte, 2)
			copy(salt, data[i+1:i+3])
			return "0x" + hex.EncodeToString(salt)
		}
	}

	return ""
}

// CompareBytecodes compares two contracts bytecode
func (s *Service) CompareBytecodes(bytecode1, bytecode2 string) (bool, float64) {
	if len(bytecode1) != len(bytecode2) {
		return false, 0
	}

	matches := 0
	total := 0

	for i := 0; i < len(bytecode1); i++ {
		if bytecode1[i] == 'x' || bytecode2[i] == 'x' {
			continue
		}

		total++
		if bytecode1[i] == bytecode2[i] {
			matches++
		}
	}

	if total == 0 {
		return false, 0
	}

	similarity := float64(matches) / float64(total) * 100
	return matches == total, similarity
}

var _ = fmt.Sprintf // Use fmt
var _ = strings.TrimSpace // Use strings