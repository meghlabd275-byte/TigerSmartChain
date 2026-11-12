package wsauth

import (
	"crypto/tls"
	"net/http"
	"net/http/httptest"
	"testing"
)

func request(host, origin string, overTLS bool) *http.Request {
	r := httptest.NewRequest(http.MethodGet, "http://"+host+"/ws", nil)
	r.Host = host
	if origin != "" {
		r.Header.Set("Origin", origin)
	}
	if overTLS {
		r.TLS = &tls.ConnectionState{}
	}
	return r
}

func TestCheck(t *testing.T) {
	t.Run("a request with no origin is allowed", func(t *testing.T) {
		// Non-browser clients send no Origin; rejecting them would break
		// indexers and the CLI.
		if !Check(request("api.example.com", "", false)) {
			t.Fatal("expected a request without an Origin to be allowed")
		}
	})

	t.Run("the same origin is allowed", func(t *testing.T) {
		if !Check(request("api.example.com", "http://api.example.com", false)) {
			t.Fatal("expected the same origin to be allowed")
		}
	})

	t.Run("the same origin over tls is allowed", func(t *testing.T) {
		if !Check(request("api.example.com", "https://api.example.com", true)) {
			t.Fatal("expected the same https origin to be allowed")
		}
	})

	t.Run("a foreign origin is refused", func(t *testing.T) {
		// This is the attack: evil.example opens the socket and reads the
		// frames with the victim's cookies attached.
		if Check(request("api.example.com", "https://evil.example", false)) {
			t.Fatal("expected a foreign origin to be refused")
		}
	})

	t.Run("a lookalike subdomain is refused", func(t *testing.T) {
		if Check(request("api.example.com", "https://api.example.com.evil.example", false)) {
			t.Fatal("expected a lookalike origin to be refused")
		}
	})

	t.Run("a different port is refused", func(t *testing.T) {
		if Check(request("api.example.com", "http://api.example.com:9999", false)) {
			t.Fatal("expected a different port to be refused")
		}
	})

	t.Run("a downgraded scheme over tls is refused", func(t *testing.T) {
		if Check(request("api.example.com", "http://api.example.com", true)) {
			t.Fatal("expected http over a TLS request to be refused")
		}
	})

	t.Run("a malformed origin is refused", func(t *testing.T) {
		if Check(request("api.example.com", "not a url", false)) {
			t.Fatal("expected a malformed origin to be refused")
		}
	})

	t.Run("a scheme-only origin is refused", func(t *testing.T) {
		if Check(request("api.example.com", "https://", false)) {
			t.Fatal("expected an origin with no host to be refused")
		}
	})

	t.Run("a configured extra origin is allowed", func(t *testing.T) {
		t.Setenv(OriginsEnv, "https://explorer.example, https://other.example/")
		if !Check(request("api.example.com", "https://explorer.example", false)) {
			t.Fatal("expected a configured origin to be allowed")
		}
		if !Check(request("api.example.com", "https://other.example", false)) {
			t.Fatal("expected a configured origin with a trailing slash to be allowed")
		}
	})

	t.Run("an unconfigured origin stays refused", func(t *testing.T) {
		t.Setenv(OriginsEnv, "https://explorer.example")
		if Check(request("api.example.com", "https://evil.example", false)) {
			t.Fatal("expected an origin outside the allow-list to be refused")
		}
	})

	t.Run("an empty allow-list does not allow everything", func(t *testing.T) {
		t.Setenv(OriginsEnv, "")
		if Check(request("api.example.com", "https://evil.example", false)) {
			t.Fatal("expected an empty allow-list to refuse a foreign origin")
		}
	})

	t.Run("a trailing slash on the origin does not matter", func(t *testing.T) {
		if !Check(request("api.example.com", "https://api.example.com/", false)) {
			t.Fatal("expected a trailing slash to be ignored")
		}
	})
}
