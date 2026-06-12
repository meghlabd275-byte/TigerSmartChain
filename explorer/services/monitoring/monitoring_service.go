// Package monitoring provides monitoring and alerting services
package monitoring

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
	"time"
)

// MonitoringService provides monitoring and alerting
type MonitoringService struct {
	alertRules map[string]*AlertRule
	alerts    map[string]*Alert
	channels  map[string]*AlertChannel
	mu       sync.RWMutex
}

// AlertRule represents an alert rule
type AlertRule struct {
	ID          string    `json:"id"`
	Name        string    `json:"name"`
	Condition  string    `json:"condition"` // metric threshold
	Threshold  float64   `json:"threshold"`
	Duration   time.Duration `json:"duration"`
	Severity   string    `json:"severity"` // critical, high, medium, low
	Channels   []string  `json:"channels"`
	Enabled   bool      `json:"enabled"`
	CreatedAt  time.Time `json:"createdAt"`
}

// Alert represents an alert
type Alert struct {
	ID        string    `json:"id"`
	RuleID   string    `json:"ruleId"`
	Name     string    `json:"name"`
	Message  string    `json:"message"`
	Severity string    `json:"severity"`
	Status   string    `json:"status"` // firing, resolved
	StartTime time.Time `json:"startTime"`
	EndTime  *time.Time `json:"endTime,omitempty"`
	Labels   map[string]string `json:"labels"`
	Annotations map[string]string `json:"annotations"`
}

// AlertChannel represents notification channel
type AlertChannel struct {
	ID        string `json:"id"`
	Type     string `json:"type"` // slack, email, webhook, pagerduty
	Name     string `json:"name"`
	Config   map[string]string `json:"config"`
	Enabled  bool `json:"enabled"`
}

// MetricsCollector collects metrics
type MetricsCollector struct {
	metrics map[string]*Metric
	mu     sync.RWMutex
}

// Metric represents a metric
type Metric struct {
	Name      string            `json:"name"`
	Value    float64           `json:"value"`
	Labels   map[string]string `json:"labels"`
	Timestamp time.Time       `json:"timestamp"`
}

// NewMonitoringService creates monitoring service
func NewMonitoringService() *MonitoringService {
	return &MonitoringService{
		alertRules: make(map[string]*AlertRule),
		alerts:    make(map[string]*AlertAlert),
		channels:  make(map[string]*AlertChannel),
	}
}

// CreateAlertRule creates alert rule
func (s *MonitoringService) CreateAlertRule(rule *AlertRule) error {
	if rule == nil || rule.Name == "" {
		return fmt.Errorf("invalid rule")
	}
	
	rule.ID = generateAlertID()
	rule.CreatedAt = time.Now()
	rule.Enabled = true
	
	s.mu.Lock()
	s.alertRules[rule.ID] = rule
	s.mu.Unlock()
	
	return nil
}

// GetAlertRules gets all alert rules
func (s *MonitoringService) GetAlertRules() []*AlertRule {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	rules := make([]*AlertRule, 0, len(s.alertRules))
	for _, rule := range s.alertRules {
		rules = append(rules, rule)
	}
	
	return rules
}

// EvaluateRules evaluates alert rules
func (s *MonitoringService) EvaluateRules(collector *MetricsCollector) {
	s.mu.RLock()
	rules := make([]*AlertRule, 0, len(s.alertRules))
	for _, rule := range s.alertRules {
		if rule.Enabled {
			rules = append(rules, rule)
		}
	}
	s.mu.RUnlock()
	
	for _, rule := range rules {
		s.evaluateRule(rule, collector)
	}
}

func (s *MonitoringService) evaluateRule(rule *AlertRule, collector *MetricsCollector) {
	metricValue := collector.GetMetricValue(rule.Condition)
	
	if metricValue >= rule.Threshold {
		s.triggerAlert(rule, metricValue)
	} else {
		s.resolveAlert(rule.ID)
	}
}

func (s *MonitoringService) triggerAlert(rule *AlertRule, value float64) {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	alertID := fmt.Sprintf("%s_%d", rule.ID, time.Now().Unix())
	
	alert := &Alert{
		ID:        alertID,
		RuleID:   rule.ID,
		Name:     rule.Name,
		Message:  fmt.Sprintf("Alert %s triggered: %.2f >= %.2f", rule.Name, value, rule.Threshold),
		Severity: rule.Severity,
		Status:   "firing",
		StartTime: time.Now(),
		Labels:   map[string]string{"rule": rule.ID},
	}
	
	s.alerts[alertID] = alert
	
	// Send notifications
	s.sendNotifications(alert, rule.Channels)
}

func (s *MonitoringService) resolveAlert(ruleID string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	for id, alert := range s.alerts {
		if alert.RuleID == ruleID && alert.Status == "firing" {
			now := time.Now()
			alert.Status = "resolved"
			alert.EndTime = &now
		}
	}
}

// GetAlerts gets alerts
func (s *MonitoringService) GetAlerts(status string) []*Alert {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	var result []*Alert
	for _, alert := range s.alerts {
		if status == "" || alert.Status == status {
			result = append(result, alert)
		}
	}
	
	return result
}

// AddChannel adds notification channel
func (s *MonitoringService) AddChannel(channel *AlertChannel) error {
	if channel == nil || channel.Name == "" {
		return fmt.Errorf("invalid channel")
	}
	
	s.mu.Lock()
	s.channels[channel.ID] = channel
	s.mu.Unlock()
	
	return nil
}

func (s *MonitoringService) sendNotifications(alert *Alert, channelIDs []string) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	for _, id := range channelIDs {
		channel, ok := s.channels[id]
		if !ok || !channel.Enabled {
			continue
		}
		
		switch channel.Type {
		case "slack":
			s.sendSlackNotification(channel, alert)
		case "email":
			s.sendEmailNotification(channel, alert)
		case "webhook":
			s.sendWebhookNotification(channel, alert)
		case "pagerduty":
			s.sendPagerDutyNotification(channel, alert)
		}
	}
}

func (s *MonitoringService) sendSlackNotification(channel *AlertChannel, alert *Alert) {
	// Would send to Slack webhook
}

func (s *MonitoringService) sendEmailNotification(channel *AlertChannel, alert *Alert) {
	// Would send email
}

func (s *MonitoringService) sendWebhookNotification(channel *AlertChannel, alert *Alert) {
	// Would send webhook
}

func (s *MonitoringService) sendPagerDutyNotification(channel *AlertChannel, alert *Alert) {
	// Would send to PagerDuty
}

// MetricsCollector methods

func NewMetricsCollector() *MetricsCollector {
	return &MetricsCollector{
		metrics: make(map[string]*Metric),
	}
}

func (c *MetricsCollector) RecordMetric(name string, value float64, labels map[string]string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	
	key := makeMetricKey(name, labels)
	c.metrics[key] = &Metric{
		Name:      name,
		Value:    value,
		Labels:   labels,
		Timestamp: time.Now(),
	}
}

func (c *MetricsCollector) GetMetricValue(name string) float64 {
	c.mu.RLock()
	defer c.mu.RUnlock()
	
	for key, metric := range c.metrics {
		if strings.HasPrefix(key, name) {
			return metric.Value
		}
	}
	
	return 0
}

func (c *MetricsCollector) GetAllMetrics() []*Metric {
	c.mu.RLock()
	defer c.mu.RUnlock()
	
	metrics := make([]*Metric, 0, len(c.metrics))
	for _, metric := range c.metrics {
		metrics = append(metrics, metric)
	}
	
	return metrics
}

func makeMetricKey(name string, labels map[string]string) string {
	var parts []string
	parts = append(parts, name)
	
	for k, v := range labels {
		parts = append(parts, fmt.Sprintf("%s=%s", k, v))
	}
	
	return strings.Join(parts, ",")
}

func generateAlertID() string {
	return fmt.Sprintf("alert_%d", time.Now().UnixNano())
}

// RecordRequest records API request metric
func (c *MetricsCollector) RecordRequest(endpoint, method, status string, duration time.Duration) {
	labels := map[string]string{
		"endpoint": endpoint,
		"method":  method,
		"status":   status,
	}
	
	c.RecordMetric("api_request_duration_seconds", duration.Seconds(), labels)
}

// RecordBlock records block metric
func (c *MetricsCollector) RecordBlock(blockNumber uint64, txCount int) {
	labels := map[string]string{}
	c.RecordMetric("blocks_processed_total", float64(blockNumber), labels)
	c.RecordMetric("transactions_in_block", float64(txCount), labels)
}

// RecordGasPrice records gas price
func (c *MetricsCollector) RecordGasPrice(slow, standard, fast float64) {
	c.RecordMetric("gas_price_slow_gwei", slow, nil)
	c.RecordMetric("gas_price_standard_gwei", standard, nil)
	c.RecordMetric("gas_price_fast_gwei", fast, nil)
}

// RecordError records error metric
func (c *MetricsCollector) RecordError(errorType string) {
	labels := map[string]string{"type": errorType}
	c.RecordMetric("errors_total", 1, labels)
}

// PrometheusExport exports metrics in Prometheus format
func (c *MetricsCollector) PrometheusExport() string {
	var sb strings.Builder
	
	metrics := c.GetAllMetrics()
	
	for _, m := range metrics {
		labelStr := ""
		if len(m.Labels) > 0 {
			var labels []string
			for k, v := range m.Labels {
				labels = append(labels, fmt.Sprintf("%s=\"%s\"", k, v))
			}
			labelStr = "{" + strings.Join(labels, ",") + "}"
		}
		
		sb.WriteString(fmt.Sprintf("%s%s %.6f %d\n", 
			m.Name, labelStr, m.Value, m.Timestamp.Unix()))
	}
	
	return sb.String()
}

// JSONExport exports metrics in JSON format
func (c *MetricsCollector) JSONExport() (string, error) {
	metrics := c.GetAllMetrics()
	data, err := json.Marshal(metrics)
	return string(data), err
}

// InitMonitoringService initializes the service
func InitMonitoringService() (*MonitoringService, error) {
	return NewMonitoringService(), nil
}