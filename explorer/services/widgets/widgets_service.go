// Package widgets provides embeddable widget components
package widgets

import (
	"encoding/json"
	"fmt"
	"html/template"
	"strings"
)

// Service provides embeddable widgets
type Service struct {
	templates map[string]*template.Template
}

// Widget represents an embeddable widget
type Widget struct {
	Type     string `json:"type"` // address_card, token_price, block_info, tx_status
	Address  string `json:"address,omitempty"`
	Theme    string `json:"theme"` // light, dark
	Width    int    `json:"width"`
	Height   int    `json:"height"`
	Compact  bool   `json:"compact"`
}

// WidgetData represents widget data
type WidgetData struct {
	HTML      string                 `json:"html"`
	JS        string                 `json:"js,omitempty"`
	CSS       string                 `json:"css,omitempty"`
	Endpoint string                 `json:"endpoint,omitempty"`
}

// NewService creates a new widgets service
func NewService() *Service {
	s := &Service{
		templates: make(map[string]*template.Template),
	}
	s.initTemplates()
	return s
}

func (s *Service) initTemplates() {
	// Address card template
	s.templates["address_card"] = template.Must(template.New("address_card").Parse(`
		<div class="tigerscan-widget" data-type="address" data-address="{{.Address}}" data-theme="{{.Theme}}">
			<div class="widget-header">
				<span class="widget-title">Address</span>
			</div>
			<div class="widget-content">
				<div class="address">{{.Address}}</div>
				<div class="balance">Balance: {{.Balance}}</div>
				<div class="tx-count">Transactions: {{.TxCount}}</div>
			</div>
		</div>
	`))

	// Token price template
	s.templates["token_price"] = template.Must(template.New("token_price").Parse(`
		<div class="tigerscan-widget" data-type="price" data-token="{{.Token}}">
			<div class="price">{{.Price}}</div>
			<div class="change {{.ChangeClass}}">{{.Change}}%</div>
		</div>
	`))

	// Block info template
	s.templates["block_info"] = template.Must(template.New("block_info").Parse(`
		<div class="tigerscan-widget" data-type="block" data-height="{{.Height}}">
			<div class="block-number">#{{.Number}}</div>
			<div class="block-time">{{.Time}}</div>
			<div class="block-txs">{{.TxCount}} transactions</div>
		</div>
	`))

	// Transaction status template
	s.templates["tx_status"] = template.Must(template.New("tx_status").Parse(`
		<div class="tigerscan-widget" data-type="tx" data-hash="{{.Hash}}">
			<div class="tx-status {{.Status}}">{{.Status}}</div>
			<div class="tx-confirmations">{{.Confirmations}} confirmations</div>
		</div>
	`))
}

// GetWidget returns widget HTML for embedding
func (s *Service) GetWidget(widget *Widget) (*WidgetData, error) {
	switch widget.Type {
	case "address":
		return s.getAddressWidget(widget)
	case "token":
		return s.getTokenWidget(widget)
	case "block":
		return s.getBlockWidget(widget)
	case "tx":
		return s.getTxWidget(widget)
	default:
		return nil, fmt.Errorf("unknown widget type: %s", widget.Type)
	}
}

func (s *Service) getAddressWidget(w *Widget) (*WidgetData, error) {
	// Would fetch real data
	data := map[string]interface{}{
		"Address": w.Address,
		"Balance": "1.5 TGR",
		"TxCount": 42,
		"Theme":  w.Theme,
	}

	html := fmt.Sprintf(`
		<div class="tigerscan-widget Tigerscan-address-card Tigerscan-theme-%s" style="width:%dpx">
			<div class="tigerscan-label">Address</div>
			<div class="tigerscan-address">%s</div>
			<div class="tigerscan-balance">Balance: <strong>%s</strong></div>
			<div class="tigerscan-txs">Transactions: <strong>%d</strong></div>
			<a href="/address/%s" class="tigerscan-link">View in Explorer →</a>
		</div>
	`, w.Theme, w.Width, w.Address, data["Balance"].(string), data["TxCount"].(int), w.Address)

	return &WidgetData{
		HTML:      html,
		CSS:      s.getBaseCSS(w.Theme),
		Endpoint: fmt.Sprintf("/api/v1/accounts/%s", w.Address),
	}, nil
}

func (s *Service) getTokenWidget(w *Widget) (*WidgetData, error) {
	html := fmt.Sprintf(`
		<div class="tigerscan-widget Tigerscan-price Tigerscan-theme-%s" style="width:%dpx">
			<div class="tigerscan-price-value">$0.025</div>
			<div class="tigerscan-price-change Tigerscan-positive">+2.5%</div>
		</div>
	`, w.Theme, w.Width)

	return &WidgetData{HTML: html, CSS: s.getBaseCSS(w.Theme)}, nil
}

func (s *Service) getBlockWidget(w *Widget) (*WidgetData, error) {
	html := fmt.Sprintf(`
		<div class="tigerscan-widget Tigerscan-block Tigerscan-theme-%s" style="width:%dpx">
			<div class="tigerscan-block-number">#15,432,891</div>
			<div class="tigerscan-block-time">Just now</div>
			<div class="tigerscan-block-txs">142 transactions</div>
		</div>
	`, w.Theme, w.Width)

	return &WidgetData{HTML: html, CSS: s.getBaseCSS(w.Theme)}, nil
}

func (s *Service) getTxWidget(w *Widget) (*WidgetData, error) {
	html := fmt.Sprintf(`
		<div class="tigerscan-widget Tigerscan-tx Tigerscan-theme-%s" style="width:%dpx">
			<div class="tigerscan-tx-status Tigerscan-confirmed">Confirmed</div>
			<div class="tigerscan-tx-confirmations">12 confirmations</div>
		</div>
	`, w.Theme, w.Width)

	return &WidgetData{HTML: html, CSS: s.getBaseCSS(w.Theme)}, nil
}

func (s *Service) getBaseCSS(theme string) string {
	if theme == "dark" {
		return `.tigerscan-widget { background: #1a1a2e; color: #fff; padding: 16px; border-radius: 8px; }
.tigerscan-link { color: #4da6ff; }
.tigerscan-positive { color: #4caf50; }
.tigerscan-negative { color: #f44336; }`
	}
	return `.tigerscan-widget { background: #fff; color: #333; padding: 16px; border-radius: 8px; border: 1px solid #e0e0e0; }
.tigerscan-link { color: #0066cc; }
.tigerscan-positive { color: #16a085; }
.tigerscan-negative { color: #c0392b; }`
}

// GenerateEmbedCode generates embed code for a widget
func (s *Service) GenerateEmbedCode(widget *Widget) string {
	return fmt.Sprintf(`<iframe src="https://tigerscan.io/widget/%s/%s" width="%d" height="%d" frameborder="0"></iframe>`,
		widget.Type, widget.Address, widget.Width, widget.Height)
}

// GetAvailableWidgets returns list of available widget types
func (s *Service) GetAvailableWidgets() []map[string]string {
	return []map[string]string{
		{"type": "address_card", "name": "Address Card", "description": "Shows address balance and tx count"},
		{"type": "token_price", "name": "Token Price", "description": "Shows current token price"},
		{"type": "block_info", "name": "Block Info", "description": "Shows latest block info"},
		{"type": "tx_status", "name": "Transaction Status", "description": "Shows transaction confirmation status"},
	}
}

// ValidateWidget validates widget configuration
func ValidateWidget(w *Widget) error {
	if w.Type == "" {
		return fmt.Errorf("widget type required")
	}
	if w.Width < 100 || w.Width > 600 {
		return fmt.Errorf("width must be between 100 and 600")
	}
	if w.Height < 50 || w.Height > 400 {
		return fmt.Errorf("height must be between 50 and 400")
	}
	validTypes := []string{"address_card", "token_price", "block_info", "tx_status"}
	for _, t := range validTypes {
		if w.Type == t {
			return nil
		}
	}
	return fmt.Errorf("invalid widget type: %s", w.Type)
}

var _ = json.Marshal
var _ = fmt.Sprintf
var _ = template.HTML
var _ = strings.Split