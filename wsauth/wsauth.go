// Package wsauth validates the Origin header on WebSocket upgrades.
//
// A WebSocket handshake is not covered by the same-origin policy: the browser
// attaches cookies and the Authorization header to a cross-site upgrade just
// as it does to a same-site one, and the page that opened the socket can read
// every frame. A handler that accepts any origin therefore lets any web page a
// logged-in user visits drive their authenticated session. gorilla/websocket
// disables its own origin check when CheckOrigin is set to a function that
// returns true, so returning true unconditionally is the vulnerable case, not
// the permissive default.
package wsauth

import (
	"net/http"
	"net/url"
	"os"
	"strings"
)

// OriginsEnv names the environment variable holding the comma-separated list
// of additional origins that may open a socket, for example when the explorer
// UI is served from a different host than the API.
const OriginsEnv = "TSC_WS_ALLOWED_ORIGINS"

// allowed returns the configured extra origins. The variable is read per
// call: a handshake is a rare event next to the traffic a socket carries, and
// reading it here means a change takes effect without a restart.
func allowed() []string {
	raw := os.Getenv(OriginsEnv)
	if raw == "" {
		return nil
	}
	var out []string
	for _, part := range strings.Split(raw, ",") {
		if part = strings.TrimSpace(part); part != "" {
			out = append(out, strings.TrimRight(part, "/"))
		}
	}
	return out
}

// Check is the CheckOrigin function for websocket.Upgrader. It accepts a
// request when the browser sent no Origin (a non-browser client such as an
// indexer or the CLI), when the Origin matches the Host the request arrived
// on, or when the Origin is listed in TSC_WS_ALLOWED_ORIGINS.
func Check(r *http.Request) bool {
	origin := r.Header.Get("Origin")
	if origin == "" {
		return true
	}

	u, err := url.Parse(origin)
	if err != nil || u.Host == "" {
		return false
	}

	if strings.EqualFold(u.Host, r.Host) {
		// Same host. The scheme is compared only when the request arrived over
		// TLS directly: behind a TLS-terminating proxy r.TLS is nil and the
		// server cannot tell which scheme the client used, so requiring https
		// there would reject legitimate same-origin traffic.
		return r.TLS == nil || strings.EqualFold(u.Scheme, "https")
	}

	origin = strings.TrimRight(origin, "/")
	for _, candidate := range allowed() {
		if strings.EqualFold(candidate, origin) {
			return true
		}
	}
	return false
}
