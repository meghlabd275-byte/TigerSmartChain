package graphql

import (
	"testing"

	"github.com/graphql-go/graphql"
)

// TestSchemaIntrospection proves the programmatically built schema is a valid
// executable GraphQL schema.
func TestSchemaIntrospection(t *testing.T) {
	srv, err := NewServer(&Config{})
	if err != nil {
		t.Fatalf("NewServer: %v", err)
	}

	result := graphql.Do(graphql.Params{
		Schema:        srv.schema,
		RequestString: `{ __schema { queryType { name } mutationType { name } } }`,
	})
	if len(result.Errors) > 0 {
		t.Fatalf("introspection returned errors: %v", result.Errors)
	}

	data, ok := result.Data.(map[string]interface{})
	if !ok {
		t.Fatalf("unexpected introspection payload: %#v", result.Data)
	}
	schema, ok := data["__schema"].(map[string]interface{})
	if !ok {
		t.Fatalf("no __schema in payload: %#v", data)
	}
	queryType, ok := schema["queryType"].(map[string]interface{})
	if !ok {
		t.Fatalf("no queryType in payload: %#v", schema)
	}
	if got := queryType["name"]; got != "Query" {
		t.Fatalf("query type = %v, want Query", got)
	}
	mutationType, ok := schema["mutationType"].(map[string]interface{})
	if !ok {
		t.Fatalf("no mutationType in payload: %#v", schema)
	}
	if got := mutationType["name"]; got != "Mutation" {
		t.Fatalf("mutation type = %v, want Mutation", got)
	}
}

// TestNoDatabaseReturnsError proves DB-backed fields report an error instead of
// inventing data when no database is configured.
func TestNoDatabaseReturnsError(t *testing.T) {
	srv, err := NewServer(&Config{})
	if err != nil {
		t.Fatalf("NewServer: %v", err)
	}

	result := graphql.Do(graphql.Params{
		Schema:        srv.schema,
		RequestString: `{ latestBlock { number hash } }`,
	})
	if len(result.Errors) == 0 {
		t.Fatalf("expected an error with no database configured, got data %#v", result.Data)
	}
}

// TestInvalidInputRejected proves argument validation runs before any query.
func TestInvalidInputRejected(t *testing.T) {
	srv, err := NewServer(&Config{DB: nil})
	if err != nil {
		t.Fatalf("NewServer: %v", err)
	}

	cases := []string{
		`{ transaction(hash: "not-a-hash") { hash } }`,
		`{ account(address: "0xzz") { address } }`,
		`{ block(numberOrHash: "abc") { number } }`,
	}
	for _, q := range cases {
		result := graphql.Do(graphql.Params{Schema: srv.schema, RequestString: q})
		if len(result.Errors) == 0 {
			t.Errorf("query %q was accepted but should have been rejected", q)
		}
	}
}

// TestIntrospectionDetection covers the helper that gates __schema access.
func TestIntrospectionDetection(t *testing.T) {
	cases := map[string]bool{
		`{ latestBlock { number } }`:              false,
		`{ __schema { types { name } } }`:         true,
		`{ blocks { transactions { hash } } }`:    false,
		`{ stats { totalBlocks } }`:               false,
		`{ __type(name: "Block") { name } }`:      true,
		`{ account(address: "0x0") { address } }`: false,
	}
	for query, want := range cases {
		if got := containsIntrospection(query); got != want {
			t.Errorf("containsIntrospection(%q) = %v, want %v", query, got, want)
		}
	}
}

// TestClampLimit proves the limit guard bounds caller input.
func TestClampLimit(t *testing.T) {
	if got := clampLimit(0); got != defaultLimit {
		t.Errorf("clampLimit(0) = %d, want %d", got, defaultLimit)
	}
	if got := clampLimit(-5); got != defaultLimit {
		t.Errorf("clampLimit(-5) = %d, want %d", got, defaultLimit)
	}
	if got := clampLimit(1_000_000); got != maxLimit {
		t.Errorf("clampLimit(1e6) = %d, want %d", got, maxLimit)
	}
	if got := clampLimit(10); got != 10 {
		t.Errorf("clampLimit(10) = %d, want 10", got)
	}
}

// TestOrderForWhitelist proves only known sort keys reach the SQL ORDER BY.
func TestOrderForWhitelist(t *testing.T) {
	if got := orderFor("stake", "fallback DESC"); got != "total_stake DESC NULLS LAST" {
		t.Errorf("orderFor(stake) = %q", got)
	}
	if got := orderFor("; DROP TABLE blocks", "fallback DESC"); got != "fallback DESC" {
		t.Errorf("orderFor(injection) = %q, want the fallback", got)
	}
}
