package ws

import (
	"crypto/tls"
	"net/http"
	"net/http/httptest"
	"testing"
)

// TestCheckOrigin covers the handshake origin check. This is a security
// boundary: a WebSocket handshake is not subject to the same-origin policy, so
// if this returns true for a foreign origin, any page a user visits can open a
// socket to this server with the user's cookies attached.
func TestCheckOrigin(t *testing.T) {
	cases := []struct {
		name           string
		origin         string
		host           string
		allowedOrigins []string
		// tls marks the request as having arrived over TLS directly, which is
		// when the origin scheme can be checked.
		tls  bool
		want bool
	}{
		{
			name:   "same origin is allowed",
			origin: "https://scan.tigerchain.io",
			host:   "scan.tigerchain.io",
			tls:    true,
			want:   true,
		},
		{
			name:   "same origin with a port is allowed",
			origin: "http://localhost:3000",
			host:   "localhost:3000",
			want:   true,
		},
		{
			name:   "no origin is allowed, since it is not a browser",
			origin: "",
			host:   "scan.tigerchain.io",
			tls:    true,
			want:   true,
		},
		{
			name:   "a foreign origin is refused",
			origin: "https://evil.example",
			host:   "scan.tigerchain.io",
			tls:    true,
			want:   false,
		},
		{
			name:   "a subdomain is not the same origin",
			origin: "https://evil.scan.tigerchain.io",
			host:   "scan.tigerchain.io",
			tls:    true,
			want:   false,
		},
		{
			name:   "a different port is not the same origin",
			origin: "https://scan.tigerchain.io:8443",
			host:   "scan.tigerchain.io",
			tls:    true,
			want:   false,
		},
		{
			name:   "a downgraded scheme is refused when the request arrived over TLS",
			origin: "http://scan.tigerchain.io",
			host:   "scan.tigerchain.io",
			tls:    true,
			want:   false,
		},
		{
			name:   "an http origin on a plaintext server is same-origin",
			origin: "http://scan.tigerchain.io",
			host:   "scan.tigerchain.io",
			want:   true,
		},
		{
			name:           "an explicitly allowed origin is permitted",
			origin:         "https://wallet.example",
			host:           "scan.tigerchain.io",
			tls:            true,
			allowedOrigins: []string{"https://wallet.example"},
			want:           true,
		},
		{
			name:           "an origin that is not on the allow list is refused",
			origin:         "https://other.example",
			host:           "scan.tigerchain.io",
			tls:            true,
			allowedOrigins: []string{"https://wallet.example"},
			want:           false,
		},
		{
			name:   "an unparseable origin is refused",
			origin: "://not a url",
			host:   "scan.tigerchain.io",
			tls:    true,
			want:   false,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			s := &Server{cfg: &Config{AllowedOrigins: tc.allowedOrigins}}
			req := httptest.NewRequest(http.MethodGet, "/ws", nil)
			req.Host = tc.host
			if tc.tls {
				req.TLS = &tls.ConnectionState{}
			}
			if tc.origin != "" {
				req.Header.Set("Origin", tc.origin)
			}
			if got := s.checkOrigin(req); got != tc.want {
				t.Fatalf("checkOrigin(origin=%q, host=%q, allowed=%v, tls=%v) = %v, want %v",
					tc.origin, tc.host, tc.allowedOrigins, tc.tls, got, tc.want)
			}
		})
	}
}
