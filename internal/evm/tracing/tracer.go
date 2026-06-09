// Package tracing provides EVM tracing capabilities for TigerSmartChain.
package tracing

import (
	"encoding/json"
	"fmt"
	"math/big"
)

// =============================================================================
// EVM TRACING
// =============================================================================

// Tracer defines the interface for EVM tracers.
type Tracer interface {
	// CaptureStart is called when execution starts
	CaptureStart(env *EVM, from, to string, create bool, input []byte, gas uint64, value *big.Int)

	// CaptureState is called after each instruction
	CaptureState(pc uint64, op string, gas, cost uint64, scope *ScopeContext, depth int, err error)

	// CaptureEnd is called when execution ends
	CaptureEnd(output []byte, gasUsed uint64, err error)

	// GetResult returns the tracer result
	GetResult() (json.RawMessage, error)
}

// =============================================================================
// STRUCT TRACER
// =============================================================================

// StructLog represents a single EVM operation.
type StructLog struct {
	Pc            uint64   `json:"pc"`
	Op            string   `json:"op"`
	Gas           uint64   `json:"gas"`
	GasCost       uint64   `json:"gasCost"`
	Memory        string   `json:"memory"`
	MemorySize    int      `json:"memorySize"`
	Stack         []string `json:"stack"`
	Storage       map[string]string `json:"storage"`
	Depth         int      `json:"depth"`
	Err           string   `json:"err,omitempty"`
}

// StructTracer is the standard JSON tracer.
type StructTracer struct {
	logs     []StructLog
	failed   bool
	output   []byte
	gasLimit uint64
	gasUsed  uint64
}

// NewStructTracer creates a new struct tracer.
func NewStructTracer() *StructTracer {
	return &StructTracer{
		logs: make([]StructLog, 0),
	}
}

// CaptureStart implements Tracer.
func (t *StructTracer) CaptureStart(env *EVM, from, to string, create bool, input []byte, gas uint64, value *big.Int) {
	t.gasLimit = gas
}

// CaptureState implements Tracer.
func (t *StructTracer) CaptureState(pc uint64, op string, gas, cost uint64, scope *ScopeContext, depth int, err error) {
	log := StructLog{
		Pc:         pc,
		Op:         op,
		Gas:        gas,
		GasCost:    cost,
		MemorySize: scope.MemorySize,
		Depth:      depth,
	}

	// Convert stack to strings
	if len(scope.Stack) > 0 {
		log.Stack = make([]string, len(scope.Stack))
		for i, v := range scope.Stack {
			log.Stack[i] = v
		}
	}

	// Convert memory to hex
	if scope.MemorySize > 0 {
		log.Memory = fmt.Sprintf("0x%x", scope.Memory)
	}

	t.logs = append(t.logs, log)
}

// CaptureEnd implements Tracer.
func (t *StructTracer) CaptureEnd(output []byte, gasUsed uint64, err error) {
	t.output = output
	t.gasUsed = gasUsed
	if err != nil {
		t.failed = true
	}
}

// GetResult implements Tracer.
func (t *StructTracer) GetResult() (json.RawMessage, error) {
	result := map[string]interface{}{
		"gas":         fmt.Sprintf("0x%x", t.gasUsed),
		"failed":      t.failed,
		"returnValue": fmt.Sprintf("0x%x", t.output),
		"structLogs":  t.logs,
	}

	return json.Marshal(result)
}

// =============================================================================
// CALL FRAME TRACER
// =============================================================================

// CallFrame represents a call frame.
type CallFrame struct {
	Type    string      `json:"type"`
	From   string      `json:"from"`
	To     string      `json:"to"`
	Value  string      `json:"value"`
	Input  string      `json:"input"`
	Output string      `json:"output"`
	Gas    uint64      `json:"gas"`
	Calls  []CallFrame `json:"calls,omitempty"`
	Error  string      `json:"error,omitempty"`
}

// CallTracer traces call frames.
type CallTracer struct {
	frames  []CallFrame
	current *CallFrame
}

// NewCallTracer creates a new call tracer.
func NewCallTracer() *CallTracer {
	return &CallTracer{
		frames: make([]CallFrame, 0),
	}
}

// CaptureStart implements Tracer.
func (t *CallTracer) CaptureStart(env *EVM, from, to string, create bool, input []byte, gas uint64, value *big.Int) {
	callType := "CALL"
	if create {
		callType = "CREATE"
	}

	t.current = &CallFrame{
		Type:   callType,
		From:   from,
		To:     to,
		Value:  value.String(),
		Input:  fmt.Sprintf("0x%x", input),
		Gas:    gas,
		Calls:  make([]CallFrame, 0),
	}
}

// CaptureEnd implements Tracer.
func (t *CallTracer) CaptureEnd(output []byte, gasUsed uint64, err error) {
	if t.current != nil {
		t.current.Output = fmt.Sprintf("0x%x", output)
		t.frames = append(t.frames, *t.current)
		t.current = nil
	}
}

// GetResult implements Tracer.
func (t *CallTracer) GetResult() (json.RawMessage, error) {
	return json.Marshal(t.frames)
}

// =============================================================================
// 4-BYTE TRACER
// =============================================================================

// FourByteTracer traces function selectors.
type FourByteTracer struct {
	selectors map[string]uint32
}

// NewFourByteTracer creates a new 4-byte tracer.
func NewFourByteTracer() *FourByteTracer {
	return &FourByteTracer{
		selectors: make(map[string]uint32),
	}
}

// CaptureState implements Tracer.
func (t *FourByteTracer) CaptureState(pc uint64, op string, gas, cost uint64, scope *ScopeContext, depth int, err error) {
	// Only track CALL operations
	callOps := map[string]bool{
		"CALL": true, "CALLCODE": true, "DELEGATECALL": true, "STATICCALL": true,
	}

	if !callOps[op] {
		return
	}

	// Get function selector from stack (4 bytes = 8 hex chars)
	if len(scope.Stack) >= 2 {
		selector := scope.Stack[len(scope.Stack)-2]
		if len(selector) >= 8 {
			t.selectors[selector[:8]]++
		}
	}
}

// GetResult implements Tracer.
func (t *FourByteTracer) GetResult() (json.RawMessage, error) {
	return json.Marshal(t.selectors)
}

// CaptureStart implements Tracer.
func (t *FourByteTracer) CaptureStart(env *EVM, from, to string, create bool, input []byte, gas uint64, value *big.Int) {}

// CaptureEnd implements Tracer.
func (t *FourByteTracer) CaptureEnd(output []byte, gasUsed uint64, err error) {}

// =============================================================================
// EVM CONTEXT
// =============================================================================

// EVM represents an Ethereum Virtual Machine instance.
type EVM struct {
	ChainID    *big.Int
	BlockNumber *big.Int
	Timestamp   *big.Int
	Coinbase    string
	Difficulty  *big.Int
	GasLimit    uint64
}

// ScopeContext represents the EVM scope context.
type ScopeContext struct {
	Memory     []byte
	MemorySize int
	Stack      []string
}

// =============================================================================
// PRECOMPILE TRACING
// =============================================================================

// PrecompileCall represents a precompile call.
type PrecompileCall struct {
	Address string
	Input   []byte
	GasUsed uint64
	Output  []byte
	Success bool
}

// PrecompileTracer traces precompile calls.
type PrecompileTracer struct {
	calls []PrecompileCall
}

// NewPrecompileTracer creates a new precompile tracer.
func NewPrecompileTracer() *PrecompileTracer {
	return &PrecompileTracer{
		calls: make([]PrecompileCall, 0),
	}
}

// TraceCall records a precompile call.
func (t *PrecompileTracer) TraceCall(call PrecompileCall) {
	t.calls = append(t.calls, call)
}

// GetCalls returns all traced calls.
func (t *PrecompileTracer) GetCalls() []PrecompileCall {
	return t.calls
}

// CaptureStart implements Tracer.
func (t *PrecompileTracer) CaptureStart(env *EVM, from, to string, create bool, input []byte, gas uint64, value *big.Int) {}

// CaptureState implements Tracer.
func (t *PrecompileTracer) CaptureState(pc uint64, op string, gas, cost uint64, scope *ScopeContext, depth int, err error) {}

// CaptureEnd implements Tracer.
func (t *PrecompileTracer) CaptureEnd(output []byte, gasUsed uint64, err error) {}

// GetResult implements Tracer.
func (t *PrecompileTracer) GetResult() (json.RawMessage, error) {
	return json.Marshal(t.calls)
}
