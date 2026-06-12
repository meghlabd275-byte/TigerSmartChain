// Package docs provides auto-generated documentation services for smart contracts
package docs

import (
	"encoding/json"
	"fmt"
	"strings"
	"time"
)

// DocsService provides documentation generation
type DocsService struct {
	templates map[string]*Template
}

// Template represents a documentation template
type Template struct {
	Name    string `json:"name"`
	Content string `json:"content"`
}

// ContractDocs represents contract documentation
type ContractDocs struct {
	Contract   string         `json:"contract"`
	Name      string         `json:"name"`
	Title     string         `json:"title"`
	Description string       `json:"description"`
	Functions []FunctionDoc `json:"functions"`
	Events    []EventDoc    `json:"events"`
	Variables []VariableDoc `json:"variables"`
	Errors    []ErrorDoc   `json:"errors"`
}

// FunctionDoc represents function documentation
type FunctionDoc struct {
	Name        string    `json:"name"`
	Signature   string    `json:"signature"`
	Title       string    `json:"title"`
	Description string   `json:"description"`
	Params      []ParamDoc `json:"params"`
	Returns     []ReturnDoc `json:"returns"`
	Visibility string   `json:"visibility"`
	StateMutability string `json:"stateMutability"`
	Modifiers  []string `json:"modifiers"`
	Events     []string `json:"events"`
}

// ParamDoc represents parameter documentation
type ParamDoc struct {
	Name string `json:"name"`
	Type string `json:"type"`
	Description string `json:"description"`
}

// ReturnDoc represents return value documentation
type ReturnDoc struct {
	Name string `json:"name"`
	Type string `json:"type"`
	Description string `json:"description"`
}

// EventDoc represents event documentation
type EventDoc struct {
	Name        string      `json:"name"`
	Signature   string      `json:"signature"`
	Description string     `json:"description"`
	Params     []ParamDoc `json:"params"`
}

// VariableDoc represents variable documentation
type VariableDoc struct {
	Name string `json:"name"`
	Type string `json:"type"`
	Description string `json:"description"`
}

// ErrorDoc represents error documentation
type ErrorDoc struct {
	Name string `json:"name"`
	Signature string `json:"signature"`
	Description string `json:"description"`
}

// NewDocsService creates a new docs service
func NewDocsService() *DocsService {
	return &DocsService{
		templates: initTemplates(),
	}
}

func initTemplates() map[string]*Template {
	return map[string]*Template{
		"readme": {
			Name: "README",
			Content: `# {{title}}

{{description}}

## Contract Address

\`{{address}}\`

## Table of Contents

{{toc}}

## Functions

{{functions}}

## Events

{{events}}

## Permissions

{{permissions}}
`,
		},
	}
}

// GenerateDocs generates documentation from ABI
func (s *DocsService) GenerateDocs(contractName, title, description string, abi []byte) (*ContractDocs, error) {
	var abiItems []json.RawMessage
	if err := json.Unmarshal(abi, &abiItems); err != nil {
		return nil, err
	}
	
	docs := &ContractDocs{
		Contract: contractName,
		Title: title,
		Description: description,
		Functions: []FunctionDoc{},
		Events: []EventDoc{},
		Variables: []VariableDoc{},
		Errors: []ErrorDoc{},
	}
	
	for _, item := range abiItems {
		var abiEntry map[string]interface{}
		if err := json.Unmarshal(item, &abiEntry); err != nil {
			continue
		}
		
		_type, ok := abiEntry["type"].(string)
		if !ok {
			continue
		}
		
		switch _type {
		case "function":
			docs.Functions = append(docs.Functions, s.parseFunction(abiEntry))
		case "event":
			docs.Events = append(docs.Events, s.parseEvent(abiEntry))
		case "error":
			docs.Errors = append(docs.Errors, s.parseError(abiEntry))
		}
	}
	
	return docs, nil
}

func (s *DocsService) parseFunction(abi map[string]interface{}) FunctionDoc {
	name, _ := abi["name"].(string)
	inputs, _ := abi["inputs"].([]interface{})
	outputs, _ := abi["outputs"].([]interface{})
	stateMutability, _ := abi["stateMutability"].(string)
	visibility, _ := abi["visibility"].(string)
	
	doc := FunctionDoc{
		Name: name,
		Signature: s.generateSignature(name, inputs),
		Visibility: visibility,
		StateMutability: stateMutability,
		Params: s.parseParams(inputs),
		Returns: s.parseReturns(outputs),
	}
	
	return doc
}

func (s *DocsService) parseEvent(abi map[string]interface{}) EventDoc {
	name, _ := abi["name"].(string)
	inputs, _ := abi["inputs"].([]interface{})
	
	return EventDoc{
		Name: name,
		Signature: s.generateSignature(name, inputs),
		Params: s.parseParams(inputs),
	}
}

func (s *DocsService) parseError(abi map[string]interface{}) ErrorDoc {
	name, _ := abi["name"].(string)
	inputs, _ := abi["inputs"].([]interface{})
	
	return ErrorDoc{
		Name: name,
		Signature: s.generateSignature(name, inputs),
	}
}

func (s *DocsService) parseParams(inputs []interface{}) []ParamDoc {
	var params []ParamDoc
	if inputs == nil {
		return params
	}
	
	for _, input := range inputs {
		inputMap, ok := input.(map[string]interface{})
		if !ok {
			continue
		}
		
		name, _ := inputMap["name"].(string)
		typeStr, _ := inputMap["type"].(string)
		
		params = append(params, ParamDoc{
			Name: name,
			Type: typeStr,
		})
	}
	
	return params
}

func (s *DocsService) parseReturns(outputs []interface{}) []ReturnDoc {
	var returns []ReturnDoc
	if outputs == nil {
		return returns
	}
	
	for i, output := range outputs {
		outputMap, ok := output.(map[string]interface{})
		if !ok {
			continue
		}
		
		typeStr, _ := outputMap["type"].(string)
		name, _ := outputMap["name"].(string)
		
		if name == "" {
			name = fmt.Sprintf("r%d", i)
		}
		
		returns = append(returns, ReturnDoc{
			Name: name,
			Type: typeStr,
		})
	}
	
	return returns
}

func (s *DocsService) generateSignature(name string, params []interface{}) string {
	var paramTypes []string
	
	if params == nil {
		return name + "()"
	}
	
	for _, p := range params {
		pMap, ok := p.(map[string]interface{})
		if !ok {
			continue
		}
		
		typeStr, _ := pMap["type"].(string)
		paramTypes = append(paramTypes, typeStr)
	}
	
	return name + "(" + strings.Join(paramTypes, ",") + ")"
}

// GenerateMarkdown generates markdown documentation
func (s *DocsService) GenerateMarkdown(docs *ContractDocs, templateName string) (string, error) {
	template, ok := s.templates[templateName]
	if !ok {
		template = s.templates["readme"]
	}
	
	content := template.Content
	content = strings.ReplaceAll(content, "{{title}}", docs.Title)
	content = strings.ReplaceAll(content, "{{description}}", docs.Description)
	content = strings.ReplaceAll(content, "{{address}}", docs.Contract)
	content = strings.ReplaceAll(content, "{{functions}}", s.formatFunctions(docs.Functions))
	content = strings.ReplaceAll(content, "{{events}}", s.formatEvents(docs.Events))
	
	return content, nil
}

func (s *DocsService) formatFunctions(funcs []FunctionDoc) string {
	var lines []string
	
	for _, fn := range funcs {
		lines = append(lines, fmt.Sprintf("### %s\n\n%s\n\n\`%s\`", fn.Name, fn.Description, fn.Signature))
	}
	
	return strings.Join(lines, "\n\n")
}

func (s *DocsService) formatEvents(events []EventDoc) string {
	var lines []string
	
	for _, ev := range events {
		lines = append(lines, fmt.Sprintf("### %s\n\n\`%s\`", ev.Name, ev.Signature))
	}
	
	return strings.Join(lines, "\n\n")
}

// GenerateHTML generates HTML documentation
func (s *DocsService) GenerateHTML(docs *ContractDocs) (string, error) {
	var sb strings.Builder
	
	sb.WriteString(fmt.Sprintf(`<!DOCTYPE html>
<html>
<head>
<title>%s</title>
<style>
body { font-family: Arial, sans-serif; margin: 40px; }
.function { background: #f5f5f5; padding: 15px; margin: 10px 0; border-left: 4px solid #007bff; }
.event { background: #fff3cd; padding: 15px; margin: 10px 0; border-left: 4px solid #ffc107; }
code { background: #e9ecef; padding: 2px 6px; }
</style>
</head>
<body>
<h1>%s</h1>
<p>%s</p>

<h2>Functions</h2>
`, docs.Title, docs.Title, docs.Description))
	
	for _, fn := range docs.Functions {
		sb.WriteString(fmt.Sprintf(`<div class="function">
<h3>%s</h3>
<p>%s</p>
<code>%s</code>
</div>`, fn.Name, fn.Description, fn.Signature))
	}
	
	sb.WriteString("</body></html>")
	
	return sb.String(), nil
}

// GenerateOpenAPI generates OpenAPI spec
func (s *DocsService) GenerateOpenAPI(docs *ContractDocs) (map[string]interface{}, error) {
	spec := map[string]interface{}{
		"openapi": "3.0.0",
		"info": map[string]interface{}{
			"title": docs.Title,
			"description": docs.Description,
			"version": "1.0.0",
		},
		"paths": make(map[string]interface{}),
	}
	
	for _, fn := range docs.Functions {
		path := "/" + fn.Name
		spec["paths"].(map[string]interface{})[path] = map[string]interface{}{
			"post": map[string]interface{}{
				"summary": fn.Name,
				"parameters": s.paramsToOpenAPI(fn.Params),
			},
		}
	}
	
	return spec, nil
}

func (s *DocsService) paramsToOpenAPI(params []ParamDoc) []map[string]interface{} {
	var result []map[string]interface{}
	
	for _, p := range params {
		result = append(result, map[string]interface{}{
			"name": p.Name,
			"in": "query",
			"schema": map[string]string{
				"type": p.Type,
			},
		})
	}
	
	return result
}

// InitDocsService initializes the service
func InitDocsService() (*DocsService, error) {
	return NewDocsService(), nil
}