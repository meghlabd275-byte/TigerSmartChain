// Pending Transactions Page
// Production-grade pending transactions (mempool) viewer

import React, { useState, useEffect, useCallback } from 'react';
import { useRouter } from 'next/router';
import Link from 'next/link';
import { 
  Container, Row, Col, Card, Table, Badge, Button, 
  Input, Spinner, Alert, Form 
} from 'react-bootstrap';

interface PendingTx {
  hash: string;
  from: string;
  to: string;
  value: string;
  gas_price: string;
  gas_limit: number;
  nonce: number;
  timestamp: number;
  size: number;
}

interface Props {
  transactions?: PendingTx[];
}

const PendingTransactionsPage: React.FC<Props> = () => {
  const router = useRouter();
  const [transactions, setTransactions] = useState<PendingTx[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filterFrom, setFilterFrom] = useState('');
  const [filterTo, setFilterTo] = useState('');
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [refreshInterval, setRefreshInterval] = useState(15); // seconds
  
  const fetchPendingTxs = useCallback(async () => {
    try {
      const params = new URLSearchParams();
      if (filterFrom) params.append('from', filterFrom);
      if (filterTo) params.append('to', filterTo);
      
      const response = await fetch(`/api/v1/mempool?${params}`);
      
      if (!response.ok) {
        throw new Error('Failed to fetch pending transactions');
      }
      
      const data = await response.json();
      setTransactions(data.transactions || []);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [filterFrom, filterTo]);
  
  useEffect(() => {
    fetchPendingTxs();
    
    if (autoRefresh) {
      const interval = setInterval(fetchPendingTxs, refreshInterval * 1000);
      return () => clearInterval(interval);
    }
  }, [fetchPendingTxs, autoRefresh, refreshInterval]);
  
  const formatValue = (value: string) => {
    try {
      const wei = BigInt(value);
      const eth = Number(wei) / 1e18;
      return eth.toFixed(6);
    } catch {
      return value;
    }
  };
  
  const formatGasPrice = (gasPrice: string) => {
    try {
      const gwei = Number(BigInt(gasPrice)) / 1e9;
      return gwei.toFixed(2);
    } catch {
      return gasPrice;
    }
  };
  
  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };
  
  const formatTimeAgo = (timestamp: number) => {
    const seconds = Math.floor((Date.now() / 1000) - timestamp);
    if (seconds < 60) return `${seconds}s ago`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    return `${hours}h ago`;
  };
  
  return (
    <Container fluid className="pending-tx-page">
      <Row className="mb-4">
        <Col>
          <h1>Pending Transactions</h1>
          <p className="text-muted">
            View transactions in the mempool waiting to be confirmed
          </p>
        </Col>
        <Col xs="auto">
          <div className="d-flex align-items-center gap-2">
            <Form.Check
              type="switch"
              id="auto-refresh"
              label="Auto-refresh"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
            />
            <Form.Select
              size="sm"
              value={refreshInterval}
              onChange={(e) => setRefreshInterval(parseInt(e.target.value))}
              style={{ width: 'auto' }}
            >
              <option value="5">5s</option>
              <option value="10">10s</option>
              <option value="15">15s</option>
              <option value="30">30s</option>
            </Form.Select>
          </div>
        </Col>
      </Row>
      
      {/* Filters */}
      <Card className="mb-4">
        <Card.Body>
          <Row className="g-3">
            <Col md={4}>
              <Input
                type="text"
                placeholder="Filter by sender address..."
                value={filterFrom}
                onChange={(e) => setFilterFrom(e.target.value)}
              />
            </Col>
            <Col md={4}>
              <Input
                type="text"
                placeholder="Filter by recipient address..."
                value={filterTo}
                onChange={(e) => setFilterTo(e.target.value)}
              />
            </Col>
            <Col md={4}>
              <Button variant="outline-primary" onClick={fetchPendingTxs}>
                Refresh Now
              </Button>
            </Col>
          </Row>
        </Card.Body>
      </Card>
      
      {/* Stats */}
      <Row className="mb-4">
        <Col md={3}>
          <Card bg="primary" text="white">
            <Card.Body>
              <div className="fs-4 fw-bold">{transactions.length}</div>
              <div>Pending Transactions</div>
            </Card.Body>
          </Card>
        </Col>
        <Col md={3}>
          <Card bg="info" text="white">
            <Card.Body>
              <div className="fs-4 fw-bold">
                {transactions.reduce((sum, tx) => sum + Number(tx.gas_limit), 0).toLocaleString()}
              </div>
              <div>Total Gas Limit</div>
            </Card.Body>
          </Card>
        </Col>
        <Col md={3}>
          <Card bg="warning" text="dark">
            <Card.Body>
              <div className="fs-4 fw-bold">
                {formatGasPrice(
                  transactions.reduce((sum, tx) => sum + BigInt(tx.gas_price) * BigInt(tx.gas_limit), 0n).toString()
                )}
              </div>
              <div>Total Gas Fees (Gwei)</div>
            </Card.Body>
          </Card>
        </Col>
        <Col md={3}>
          <Card bg="success" text="white">
            <Card.Body>
              <div className="fs-4 fw-bold">
                {transactions.reduce((sum, tx) => sum + tx.size, 0).toLocaleString()}
              </div>
              <div>Total Size (bytes)</div>
            </Card.Body>
          </Card>
        </Col>
      </Row>
      
      {/* Transactions Table */}
      <Card>
        <Card.Body className="p-0">
          {error && (
            <Alert variant="danger" className="m-3">
              {error}
            </Alert>
          )}
          
          {loading ? (
            <div className="text-center py-5">
              <Spinner animation="border" role="status">
                <span className="visually-hidden">Loading...</span>
              </Spinner>
            </div>
          ) : (
            <Table responsive hover className="mb-0">
              <thead>
                <tr>
                  <th>Hash</th>
                  <th>From</th>
                  <th>To</th>
                  <th>Value (TGR)</th>
                  <th>Gas Price</th>
                  <th>Gas Limit</th>
                  <th>Nonce</th>
                  <th>Size</th>
                  <th>Age</th>
                </tr>
              </thead>
              <tbody>
                {transactions.length === 0 ? (
                  <tr>
                    <td colSpan={9} className="text-center py-4">
                      No pending transactions
                    </td>
                  </tr>
                ) : (
                  transactions.map((tx) => (
                    <tr key={tx.hash}>
                      <td>
                        <Link href={`/tx/${tx.hash}`}>
                          <a className="font-monospace text-truncate d-inline-block" style={{ maxWidth: '120px' }}>
                            {tx.hash}
                          </a>
                        </Link>
                      </td>
                      <td>
                        <Link href={`/address/${tx.from}`}>
                          <a className="font-monospace text-truncate d-inline-block" style={{ maxWidth: '100px' }}>
                            {tx.from}
                          </a>
                        </Link>
                      </td>
                      <td>
                        {tx.to ? (
                          <Link href={`/address/${tx.to}`}>
                            <a className="font-monospace text-truncate d-inline-block" style={{ maxWidth: '100px' }}>
                              {tx.to}
                            </a>
                          </Link>
                        ) : (
                          <Badge bg="warning">Contract Creation</Badge>
                        )}
                      </td>
                      <td>{formatValue(tx.value)}</td>
                      <td>{formatGasPrice(tx.gas_price)} Gwei</td>
                      <td>{tx.gas_limit.toLocaleString()}</td>
                      <td>{tx.nonce}</td>
                      <td>{formatSize(tx.size)}</td>
                      <td>{formatTimeAgo(tx.timestamp)}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </Table>
          )}
        </Card.Body>
      </Card>
      
      <style jsx>{`
        .pending-tx-page {
          padding: 1rem;
        }
        .font-monospace {
          font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
        }
      `}</style>
    </Container>
  );
};

export default PendingTransactionsPage;