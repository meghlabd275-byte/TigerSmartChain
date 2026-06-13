// Gas History Page
// Historical gas price data with charts

import React, { useState, useEffect, useCallback } from 'react';
import { Container, Row, Col, Card, Table, Badge, Button, Spinner, Alert } from 'react-bootstrap';

interface GasHistory {
  timestamp: number;
  avg_gas_price: string;
  min_gas_price: string;
  max_gas_price: string;
  avg_gas_price_usd: string;
  transactions_count: number;
  blocks_count: number;
}

interface Props {
  history?: GasHistory[];
}

const GasHistoryPage: React.FC<Props> = () => {
  const [history, setHistory] = useState<GasHistory[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [period, setPeriod] = useState('24h');
  const [granularity, setGranularity] = useState('1h');
  
  const fetchHistory = useCallback(async () => {
    setLoading(true);
    setError(null);
    
    try {
      const response = await fetch(`/api/v1/gas/history?period=${period}&granularity=${granularity}`);
      
      if (!response.ok) {
        throw new Error('Failed to fetch gas history');
      }
      
      const data = await response.json();
      setHistory(data.history || []);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [period, granularity]);
  
  useEffect(() => {
    fetchHistory();
  }, [fetchHistory]);
  
  const formatGwei = (gwei: string) => {
    try {
      return (Number(BigInt(gwei)) / 1e9).toFixed(2);
    } catch {
      return gwei;
    }
  };
  
  const formatUSD = (usd: string) => {
    try {
      return Number(usd).toLocaleString('en-US', { style: 'currency', currency: 'USD' });
    } catch {
      return usd;
    }
  };
  
  const formatDate = (timestamp: number) => {
    const date = new Date(timestamp * 1000);
    if (granularity === '1h' || granularity === '4h') {
      return date.toLocaleString('en-US', { 
        month: 'short', day: 'numeric', hour: 'numeric' 
      });
    }
    return date.toLocaleString('en-US', { 
      month: 'short', day: 'numeric' 
    });
  };
  
  return (
    <Container fluid className="gas-history-page">
      <Row className="mb-4">
        <Col>
          <h1>Gas History</h1>
          <p className="text-muted">
            Historical gas prices and network activity
          </p>
        </Col>
      </Row>
      
      {/* Filters */}
      <Card className="mb-4">
        <Card.Body>
          <Row className="g-3">
            <Col md={3}>
              <select 
                className="form-select"
                value={period}
                onChange={(e) => setPeriod(e.target.value)}
              >
                <option value="24h">Last 24 hours</option>
                <option value="7d">Last 7 days</option>
                <option value="30d">Last 30 days</option>
                <option value="90d">Last 90 days</option>
              </select>
            </Col>
            <Col md={3}>
              <select 
                className="form-select"
                value={granularity}
                onChange={(e) => setGranularity(e.target.value)}
              >
                <option value="15m">15 minutes</option>
                <option value="1h">1 hour</option>
                <option value="4h">4 hours</option>
                <option value="1d">1 day</option>
              </select>
            </Col>
            <Col md={3}>
              <Button variant="primary" onClick={fetchHistory}>
                Refresh
              </Button>
            </Col>
          </Row>
        </Card.Body>
      </Card>
      
      {/* Summary Stats */}
      <Row className="mb-4">
        <Col md={3}>
          <Card>
            <Card.Body>
              <div className="text-muted">Average Gas</div>
              <div className="fs-4 fw-bold">
                {history.length > 0 ? formatGwei(history[history.length - 1].avg_gas_price) : '-'} Gwei
              </div>
            </Card.Body>
          </Card>
        </Col>
        <Col md={3}>
          <Card>
            <Card.Body>
              <div className="text-muted">Low Gas</div>
              <div className="fs-4 fw-bold text-success">
                {history.length > 0 ? formatGwei(history.reduce((min, h) => 
                  BigInt(h.min_gas_price) < BigInt(min.min_gas_price) ? h : min, history[0]).min_gas_price) : '-'} Gwei
              </div>
            </Card.Body>
          </Card>
        </Col>
        <Col md={3}>
          <Card>
            <Card.Body>
              <div className="text-muted">High Gas</div>
              <div className="fs-4 fw-bold text-danger">
                {history.length > 0 ? formatGwei(history.reduce((max, h) => 
                  BigInt(h.max_gas_price) > BigInt(max.max_gas_price) ? h : max, history[0]).max_gas_price) : '-'} Gwei
              </div>
            </Card.Body>
          </Card>
        </Col>
        <Col md={3}>
          <Card>
            <Card.Body>
              <div className="text-muted">Total Transactions</div>
              <div className="fs-4 fw-bold">
                {history.length > 0 ? history.reduce((sum, h) => sum + h.transactions_count, 0).toLocaleString() : '-'}
              </div>
            </Card.Body>
          </Card>
        </Col>
      </Row>
      
      {/* History Table */}
      <Card>
        <Card.Body className="p-0">
          {error && <Alert variant="danger" className="m-3">{error}</Alert>}
          
          {loading ? (
            <div className="text-center py-5">
              <Spinner animation="border" role="status" />
            </div>
          ) : (
            <Table responsive hover className="mb-0">
              <thead>
                <tr>
                  <th>Time</th>
                  <th>Avg Gas</th>
                  <th>Min Gas</th>
                  <th>Max Gas</th>
                  <th>USD Price</th>
                  <th>Transactions</th>
                  <th>Blocks</th>
                </tr>
              </thead>
              <tbody>
                {history.length === 0 ? (
                  <tr><td colSpan={7} className="text-center py-4">No data</td></tr>
                ) : (
                  history.map((h, i) => (
                    <tr key={i}>
                      <td>{formatDate(h.timestamp)}</td>
                      <td>{formatGwei(h.avg_gas_price)} Gwei</td>
                      <td>{formatGwei(h.min_gas_price)} Gwei</td>
                      <td>{formatGwei(h.max_gas_price)} Gwei</td>
                      <td>{formatUSD(h.avg_gas_price_usd)}</td>
                      <td>{h.transactions_count.toLocaleString()}</td>
                      <td>{h.blocks_count.toLocaleString()}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </Table>
          )}
        </Card.Body>
      </Card>
      
      <style jsx>{`
        .gas-history-page { padding: 1rem; }
      `}</style>
    </Container>
  );
};

export default GasHistoryPage;