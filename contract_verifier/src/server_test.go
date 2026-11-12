package contractverifier

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

const greeterSource = `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;
contract Greeter {
    string public greeting;
    constructor(string memory g) { greeting = g; }
    function set(string memory g) public { greeting = g; }
}`

// A real solc must produce bytecode for the source, not a canned constant.
func TestCompileContractProducesRealBytecode(t *testing.T) {
	solc := os.Getenv("SOLC_PATH")
	if solc == "" {
		t.Skip("SOLC_PATH is not set; install solc to run this test")
	}
	dir := t.TempDir()
	if err := os.WriteFile(dir+"/Greeter.sol", []byte(greeterSource), 0644); err != nil {
		t.Fatal(err)
	}

	s := &Server{cfg: &Config{SolidityPath: solc}}
	got, err := s.compileContract(dir, "Greeter", "0.8.26", false, 0)
	if err != nil {
		t.Fatalf("compileContract: %v", err)
	}

	// The previous code returned this exact string for every contract.
	const canned = "0x608060405234801561001057600080fd5b5061012a8061001e6000396000f3fe"
	if got.bytecode == canned {
		t.Fatal("compileContract returned the old hardcoded bytecode")
	}
	if !strings.HasPrefix(got.bytecode, "0x6080604052") {
		t.Fatalf("bytecode does not look like EVM output: %.40s", got.bytecode)
	}
	if len(got.bytecode) < 100 {
		t.Fatalf("bytecode is implausibly short: %d chars", len(got.bytecode))
	}

	// The ABI must describe this contract, not a placeholder function.
	var abi []map[string]any
	if err := json.Unmarshal([]byte(got.abi), &abi); err != nil {
		t.Fatalf("abi is not valid JSON: %v", err)
	}
	var names []string
	for _, e := range abi {
		if n, ok := e["name"].(string); ok {
			names = append(names, n)
		}
	}
	joined := strings.Join(names, ",")
	if !strings.Contains(joined, "greeting") || !strings.Contains(joined, "set") {
		t.Fatalf("ABI does not describe the compiled contract, got %v", names)
	}
	if strings.Contains(got.abi, `"name":"test"`) {
		t.Fatal("ABI is the old placeholder")
	}
}

// A compiler that cannot be run must fail loudly rather than return bytecode.
func TestCompileContractFailsWhenSolcIsMissing(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(dir+"/X.sol", []byte("pragma solidity ^0.8.20; contract X {}"), 0644); err != nil {
		t.Fatal(err)
	}
	// SOLC_PATH takes precedence over the configured path by design, so it is
	// cleared here to reach the configured one.
	t.Setenv("SOLC_PATH", "")
	s := &Server{cfg: &Config{SolidityPath: "/nonexistent/solc-does-not-exist"}}
	if _, err := s.compileContract(dir, "X", "", false, 0); err == nil {
		t.Fatal("a missing compiler must be an error, not silently canned bytecode")
	}
}

// Source that does not compile must fail.
func TestCompileContractFailsOnBadSource(t *testing.T) {
	solc := os.Getenv("SOLC_PATH")
	if solc == "" {
		t.Skip("SOLC_PATH is not set")
	}
	dir := t.TempDir()
	if err := os.WriteFile(dir+"/Bad.sol", []byte("contract Bad { this is not solidity }"), 0644); err != nil {
		t.Fatal(err)
	}
	s := &Server{cfg: &Config{SolidityPath: solc}}
	if _, err := s.compileContract(dir, "Bad", "0.8.26", false, 0); err == nil {
		t.Fatal("source that does not compile must be an error")
	}
}

// A source with several contracts must not silently verify the wrong one.
func TestCompileContractRejectsAmbiguousName(t *testing.T) {
	solc := os.Getenv("SOLC_PATH")
	if solc == "" {
		t.Skip("SOLC_PATH is not set")
	}
	dir := t.TempDir()
	src := "pragma solidity ^0.8.20;\n" +
		"contract A { function a() public pure returns (uint) { return 1; } }\n" +
		"contract B { function b() public pure returns (uint) { return 2; } }\n"
	if err := os.WriteFile(dir+"/Two.sol", []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	s := &Server{cfg: &Config{SolidityPath: solc}}
	if _, err := s.compileContract(dir, "Missing", "0.8.26", false, 0); err == nil {
		t.Fatal("an unknown contract name must be an error when the source has several")
	}
}

func TestSupportedCompilersProbesRealBinaries(t *testing.T) {
	solc := os.Getenv("SOLC_PATH")
	if solc == "" {
		t.Skip("SOLC_PATH is not set")
	}
	s := &Server{cfg: &Config{SolidityPath: solc}}
	got, err := s.GetSupportedCompilers(nil)
	if err != nil {
		t.Fatalf("GetSupportedCompilers: %v", err)
	}
	if len(got) == 0 {
		t.Fatal("expected at least one compiler")
	}
	// The reported version must be the one the binary actually is.
	for _, c := range got {
		if c.Version == "" {
			t.Fatalf("empty version reported: %+v", c)
		}
	}
	if got[0].Version != "0.8.26" {
		t.Fatalf("expected the installed 0.8.26, got %q", got[0].Version)
	}
}

func TestGetSupportedCompilersFailsWhenNoneAvailable(t *testing.T) {
	t.Setenv("SOLC_PATH", "")
	s := &Server{cfg: &Config{SolidityPath: "/nonexistent/solc"}}
	if _, err := s.GetSupportedCompilers(nil); err == nil {
		t.Fatal("with no compiler installed the list must be an error, not a fabricated list")
	}
}

// Constructor arguments must be decoded from the ABI, not passed through.
func TestDecodeConstructorArgsDecodesRealValues(t *testing.T) {
	abi := `[{"type":"constructor","inputs":[` +
		`{"name":"owner","type":"address"},` +
		`{"name":"amount","type":"uint256"},` +
		`{"name":"active","type":"bool"}]}]`
	bytecode := "0x6080"
	args := "0x" + "6080" +
		"0000000000000000000000001111111111111111111111111111111111111111" +
		"00000000000000000000000000000000000000000000000000000000000000ff" +
		"0000000000000000000000000000000000000000000000000000000000000001"

	s := &Server{cfg: &Config{}}
	got, err := s.DecodeConstructorArgs(abi, bytecode, args)
	if err != nil {
		t.Fatalf("DecodeConstructorArgs: %v", err)
	}
	if got == args {
		t.Fatal("arguments were passed through unchanged, so nothing was decoded")
	}
	var decoded map[string]string
	if err := json.Unmarshal([]byte(got), &decoded); err != nil {
		t.Fatalf("result is not JSON: %v (%s)", err, got)
	}
	if decoded["owner"] != "0x1111111111111111111111111111111111111111" {
		t.Fatalf("owner decoded to %q", decoded["owner"])
	}
	if decoded["amount"] != "255" {
		t.Fatalf("amount decoded to %q, want 255", decoded["amount"])
	}
	if decoded["active"] != "true" {
		t.Fatalf("active decoded to %q, want true", decoded["active"])
	}
}

func TestDecodeConstructorArgsWithNoArguments(t *testing.T) {
	s := &Server{cfg: &Config{}}
	got, err := s.DecodeConstructorArgs("[]", "0x6080", "")
	if err != nil {
		t.Fatal(err)
	}
	if got != "{}" {
		t.Fatalf("expected {}, got %q", got)
	}
}

func TestDecodeConstructorArgsRejectsOddLength(t *testing.T) {
	s := &Server{cfg: &Config{}}
	if _, err := s.DecodeConstructorArgs("[]", "0x60", "0x601"); err == nil {
		t.Fatal("a half byte must be an error")
	}
}

// A bool that is not 0 or 1 is not a bool.
func TestDecodeConstructorArgsRejectsMalformedBool(t *testing.T) {
	abi := `[{"type":"constructor","inputs":[{"name":"ok","type":"bool"}]}]`
	args := "0x" + "0000000000000000000000000000000000000000000000000000000000000007"
	s := &Server{cfg: &Config{}}
	got, err := s.DecodeConstructorArgs(abi, "", args)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(got, "undecodable") {
		t.Fatalf("a malformed bool must be reported as undecodable, got %s", got)
	}
}
