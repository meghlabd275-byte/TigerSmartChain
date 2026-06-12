// Package flattener provides Solidity contract flattening for TigerScan.
// Flattens multi-file Solidity contracts into single files for verification.
package flattener

import (
"fmt"
"regexp"
"strings"
"sync"

"github.com/tigersmartchain/tigersmartchain/explorer/services/verifier"
)

// =============================================================================
// CONTRACT FLATTENER
// =============================================================================

// Service provides contract flattening functionality
type Service struct {
mu sync.RWMutex

// Source code cache
cache map[string]*FlattenResult

// Parser
parser *SolidityParser

// Configuration
maxFiles      int
maxLines     int
removeLicense bool
}

// FlattenResult holds the result of flattening
type FlattenResult struct {
Source     string   `json:"source"`
Imports    []string `json:"imports"`
Licenses   []string `json:"licenses"`
Contracts  []string `json:"contracts"`
Pragmas    []string `json:"pragmas"`
FileName   string   `json:"fileName"`
LineCount int      `json:"lineCount"`
}

// =============================================================================
// SOLIDITY PARSER
// =============================================================================

// SolidityParser parses Solidity source files
type SolidityParser struct {
importRegex *regexp.Regexp
pragmaRegex *regexp.Regexp
licenseRegex *regexp.Regexp
contractRegex *regexp.Regexp
}

// NewParser creates a new Solidity parser
func NewParser() *SolidityParser {
return &SolidityParser{
importRegex:  regexp.MustCompile(`(?m)^import\s+(?:(?:"([^"]+)"|'([^']+)')\s*;|([\w.]+)\s*;)`),
pragmaRegex:  regexp.MustCompile(`(?m)^pragma\s+solidity\s+([^;]+);`),
licenseRegex: regexp.MustCompile(`(?m)^// SPDX-License-Identifier:\s*(\S+)`),
contractRegex: regexp.MustCompile(`(?m)^(contract|interface|library)\s+(\w+)`),
}
}

// =============================================================================
// NEW SERVICE
// =============================================================================

// NewService creates a new flattener service
func NewService() *Service {
return &Service{
parser: NewParser(),
cache:  make(map[string]*FlattenResult),
maxFiles: 100,
maxLines: 100000,
}
}

// =============================================================================
// FLATTEN
// =============================================================================

// Flatten flattens a Solidity contract
func (s *Service) Flatten(source string) (*FlattenResult, error) {
// Check cache
s.mu.RLock()
if cached, ok := s.cache[source]; ok {
s.mu.RUnlock()
return cached, nil
}
s.mu.RUnlock()

// Validate
if len(source) > s.maxLines*200 {
return nil, fmt.Errorf("source code too large")
}

// Parse imports
imports := s.parser.extractImports(source)

// Check import count
if len(imports) > s.maxFiles {
return nil, fmt.Errorf("too many imports: %d", len(imports))
}

// Extract pragmas
pragmas := s.parser.extractPragmas(source)

// Extract licenses
licenses := s.parser.extractLicenses(source)

// Extract contracts
contracts := s.parser.extractContracts(source)

// Build flattened source
flattened := s.buildFlattenedSource(source, imports, pragmas, licenses)

// Check line count
lines := strings.Count(flattened, "\n")
if lines > s.maxLines {
return nil, fmt.Errorf("flattened source too large: %d lines", lines)
}

result := &FlattenResult{
Source:     flattened,
Imports:    imports,
Licenses:   licenses,
Contracts:  contracts,
Pragmas:    pragmas,
LineCount:  lines,
}

// Cache result
s.mu.Lock()
s.cache[source] = result
s.mu.Unlock()

return result, nil
}

// =============================================================================
// FLATTEN WITH SOURCES
// =============================================================================

// FlattenWithSources flattens using provided source files
func (s *Service) FlattenWithSources(mainSource string, sources map[string]string) (*FlattenResult, error) {
// Build dependency graph
deps := s.buildDependencyGraph(mainSource, sources)

// Order by dependency
ordered := s.orderByDependency(deps)

// Build flattened source
flattened := s.buildOrderedSource(mainSource, ordered, sources)

return &FlattenResult{
Source:     flattened,
LineCount:  strings.Count(flattened, "\n"),
Contracts:  s.parser.extractContracts(mainSource),
Pragmas:    s.parser.extractPragmas(mainSource),
Licenses:   s.parser.extractLicenses(mainSource),
}, nil
}

// =============================================================================
// DEPENDENCY ANALYSIS
// =============================================================================

func (s *Service) buildDependencyGraph(mainSource string, sources map[string]string) map[string][]string {
deps := make(map[string][]string)

// Add main source
deps["main"] = s.parser.extractImports(mainSource)

// Add all sources
for path, source := range sources {
deps[path] = s.parser.extractImports(source)
}

return deps
}

func (s *Service) orderByDependency(deps map[string][]string) []string {
ordered := make([]string, 0, len(deps))
visited := make(map[string]bool)

var visit func(path string)
visit = func(path string) {
if visited[path] {
return
}
visited[path] = true

// Visit dependencies first
for _, dep := range deps[path] {
visit(dep)
}

ordered = append(ordered, path)
}

// Start with main
visit("main")

return ordered
}

// =============================================================================
// SOURCE BUILDING
// =============================================================================

func (s *Service) buildFlattenedSource(source string, imports, pragmas, licenses []string) string {
var sb strings.Builder

// SPDX License Identifier
if len(licenses) > 0 {
sb.WriteString("// SPDX-License-Identifier: ")
sb.WriteString(licenses[0])
sb.WriteString("\n\n")
}

// Pragma statements
for _, p := range pragmas {
sb.WriteString("pragma solidity ")
sb.WriteString(p)
sb.WriteString(";\n")
}
sb.WriteString("\n")

// Note about flattened source
sb.WriteString("// File flattened by TigerScan\n")
sb.WriteString("// Original imports: ")
sb.WriteString(strings.Join(imports, ", "))
sb.WriteString("\n\n")

// Add imports as comments
sb.WriteString("// Original source files:\n")
for _, imp := range imports {
sb.WriteString("// - ")
sb.WriteString(imp)
sb.WriteString("\n")
}
sb.WriteString("\n")

// Add the main source
sb.WriteString(source)

return sb.String()
}

func (s *Service) buildOrderedSource(mainSource string, ordered []string, sources map[string]string) string {
var sb strings.Builder

// Header
sb.WriteString("// SPDX-License-Identifier: MIT\n")
sb.WriteString("// Flattened by TigerScan\n\n")

// Add all sources in order (skip main, add it last)
for _, path := range ordered {
if path == "main" {
continue
}
if source, ok := sources[path]; ok {
sb.WriteString("\n// ===== ")
sb.WriteString(path)
sb.WriteString(" =====\n\n")
sb.WriteString(source)
sb.WriteString("\n")
}
}

// Add main source last
sb.WriteString("\n// ===== main =====\n\n")
sb.WriteString(mainSource)

return sb.String()
}

// =============================================================================
// PARSER METHODS
// =============================================================================

func (p *SolidityParser) extractImports(source string) []string {
matches := p.importRegex.FindAllStringSubmatch(source, -1)
imports := make([]string, 0, len(matches))

for _, match := range matches {
if len(match) > 1 {
// Group 1: double quotes, Group 2: single quotes, Group 3: named import
imp := match[1]
if imp == "" {
imp = match[2]
}
if imp == "" {
imp = match[3]
}
if imp != "" {
imports = append(imports, imp)
}
}
}

return imports
}

func (p *SolidityParser) extractPragmas(source string) []string {
matches := p.pragmaRegex.FindAllStringSubmatch(source, -1)
pragmas := make([]string, 0, len(matches))

for _, match := range matches {
if len(match) > 1 {
pragmas = append(pragmas, match[1])
}
}

return pragmas
}

func (p *SolidityParser) extractLicenses(source string) []string {
matches := p.licenseRegex.FindAllStringSubmatch(source, -1)
licenses := make([]string, 0, len(matches))

for _, match := range matches {
if len(match) > 1 {
licenses = append(licenses, match[1])
}
}

return licenses
}

func (p *SolidityParser) extractContracts(source string) []string {
matches := p.contractRegex.FindAllStringSubmatch(source, -1)
contracts := make([]string, 0, len(matches))

for _, match := range matches {
if len(match) > 2 {
contracts = append(contracts, match[2])
}
}

return contracts
}

// =============================================================================
// VALIDATION
// =============================================================================

// ValidateSource checks if Solidity source is valid
func (s *Service) ValidateSource(source string) (*ValidationResult, error) {
result := &ValidationResult{
IsValid: true,
Errors:  make([]string, 0),
Warnings: make([]string, 0),
}

// Check for SPDX license
licenses := s.parser.extractLicenses(source)
if len(licenses) == 0 {
result.Warnings = append(result.Warnings, "No SPDX license identifier found")
}

// Check for version pragma
pragmas := s.parser.extractPragmas(source)
if len(pragmas) == 0 {
result.Warnings = append(result.Warnings, "No version pragma found")
}

// Check for imports
imports := s.parser.extractImports(source)
if len(imports) == 0 {
result.Warnings = append(result.Warnings, "No imports found - already flattened?")
}

// Check for contracts
contracts := s.parser.extractContracts(source)
if len(contracts) == 0 {
result.IsValid = false
result.Errors = append(result.Errors, "No contract/interface/library found")
}

// Check for common syntax issues
if strings.Contains(source, "abstract abstract") {
result.Errors = append(result.Errors, "Duplicate 'abstract' keyword")
result.IsValid = false
}

if strings.Contains(source, "contract contract") {
result.Errors = append(result.Errors, "Duplicate 'contract' keyword")
result.IsValid = false
}

return result, nil
}

// ValidationResult holds validation result
type ValidationResult struct {
IsValid  bool     `json:"isValid"`
Errors   []string `json:"errors"`
Warnings []string `json:"warnings"`
}

// =============================================================================
// COMPARE
// =============================================================================

// CompareBytecode compares deployed bytecode with compiled
type CompareResult struct {
Match       bool     `json:"match"`
Deployed   string   `json:"deployedBytecode"`
Compiled   string   `json:"compiledBytecode"`
MatchedLen int      `json:"matchedLength"`
DiffPos    int      `json:"diffPosition,omitempty"`
}

// CompareBytecode compares deployed bytecode with compiled
func (s *Service) CompareBytecode(deployed, compiled string) *CompareResult {
// Normalize (remove metadata hash)
deployed = normalizeBytecode(deployed)
compiled = normalizeBytecode(compiled)

// Simple comparison
if deployed == compiled {
return &CompareResult{
Match:      true,
Deployed:   deployed,
Compiled:   compiled,
MatchedLen: len(deployed),
}
}

// Find first difference
diffPos := -1
minLen := len(deployed)
if len(compiled) < minLen {
minLen = len(compiled)
}

for i := 0; i < minLen; i++ {
if deployed[i] != compiled[i] {
diffPos = i
break
}
}

if diffPos == -1 {
diffPos = minLen
}

return &CompareResult{
Match:       false,
Deployed:    deployed,
Compiled:    compiled,
MatchedLen: diffPos,
DiffPos:    diffPos,
}
}

func normalizeBytecode(code string) string {
// Remove 0x prefix
code = strings.TrimPrefix(code, "0x")

// Remove metadata hash (usually at the end)
// Solidity metadata hash starts with a264
if idx := strings.Index(code, "a2646970667358221220"); idx > 0 {
code = code[:idx]
}

return code
}

// =============================================================================
// INTERFACE FOR VERIFIER
// =============================================================================

// Implement verifier.FlattenerInterface
var _ verifier.FlattenerInterface = (*Service)(nil)

// Flatten implements the FlattenerInterface
func (s *Service) FlattenContract(source string) (string, error) {
result, err := s.Flatten(source)
if err != nil {
return "", err
}
return result.Source, nil
}

// Validate implements the FlattenerInterface
func (s *Service) Validate(source string) (bool, error) {
result, err := s.ValidateSource(source)
if err != nil {
return false, err
}
return result.IsValid, nil
}

// =============================================================================
// CLI / API
// =============================================================================

/*
Usage:

1. As library:
   flattener := flattener.NewService()
   
   // Flatten single file
   result, err := flattener.Flatten(sourceCode)
   fmt.Println(result.Source)
   
   // Flatten with dependencies
   sources := map[string]string{
       "contracts/Token.sol": "...", 
       "contracts/SafeMath.sol": "...",
   }
   result, err := flattener.FlattenWithSources(mainSource, sources)
   
   // Validate
   validation, _ := flattener.ValidateSource(sourceCode)
   if !validation.IsValid {
       fmt.Println(validation.Errors...)
   }

2. Via API:
   POST /api/v1/contracts/flatten
   {
       "source": "pragma...",
       "sources": {...}
   }
*/
