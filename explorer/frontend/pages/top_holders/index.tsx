// Top Holders Page
// Token holder rankings with distribution

import React, { useState, useEffect, useCallback } from 'react';
import Link from 'next/link';
import { Container, Row, Col, Card, Table, Badge, Button, Input, Select, Pagination, Spinner, Alert } from 'react-bootstrap';

interface Holder {
  address: string;
  balance: string;
  balance_usd: number;
  percentage: number;
  rank: number;
  token_name: string;
  token_symbol: string;
}

interface Props {
  holders?: Holder[];
}

const TopHoldersPage: React.FC = () => {
  const [holders, setHolders] = useState<Holder[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(50);
  const [token, setToken] = useState('');
  
  const fetchHolders = useCallback(async () => {
    setLoading(true);
    setError(null);
    
    try {
      const params = new URLSearchParams({
        page: page.toString(),
        limit: limit.toString(),
      });
      if (token) params.append('token', token);
      
      const response = await fetch(`/api/v1/top-holders?${params}`);
      
      if (!response.ok) {
        throw new Error('Failed to fetch holders');
      }
      
      const data = await response.json();
      setHolders(data.holders || []);
      setTotal(data.total || 0);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [page, limit, token]);
  
  useEffect(() => {
    fetchHolders();
  }, [fetchHolders]);
  
  const formatBalance = (balance: string) => {
    try {
      const num = Number(balance);
      if (num >= 1e9) return (num / 1e9).toFixed(2) + 'B';
      if (num >= 1e6) return (num / 1e6).toFixed(2) + 'M';
      if (num >= 1e3) return (num / 1e3).toFixed(2) + 'K';
      return num.toFixed(2);
    } catch {
      return balance;
    }
  };
  
  const formatUSD = (usd: number) => {
    return usd.toLocaleString('en-US', { style: 'currency', currency: 'USD' });
  };
  
  const totalPages = Math.ceil(total / limit);
  
  return (
    <Container fluid className="top-holders-page">
      <Row className="mb-4">
        <Col>
          <h1>Top Holders</h1>
          <p className="text-muted">
            Token holder rankings and distribution
          </p>
        </Col>
      </Row>
      
      <Card className="mb-4">
        <Card.Body>
          <Row className="g-3">
            <Col md={4}>
              <Input
                type="text"
                placeholder="Filter by token address..."
                value={token}
                onChange={(e) => setToken(e.target.value)}
              />
            </Col>
            <Col md={2}>
              <Select value={limit} onChange={(e) => { setLimit(parseInt(e.target.value)); setPage(1); }}>
                <option value="50">50</option>
                <option value="100">100</option>
                <option value="200">200</option>
              </Select>
            </Col>
            <Col md={2}>
              <Button variant="primary" onClick={fetchHolders}>Search</Button>
            </Col>
          </Row>
        </Card.Body>
      </Card>
      
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
                  <th>Rank</th>
                  <th>Address</th>
                  <th>Balance</th>
                  <th>USD Value</th>
                  <th>Percentage</th>
                  <th>Token</th>
                </tr>
              </thead>
              <tbody>
                {holders.length === 0 ? (
                  <tr><td colSpan={6} className="text-center py-4">No holders found</td></tr>
                ) : (
                  holders.map((h) => (
                    <tr key={h.address}>
                      <td>
                        <Badge bg={h.rank <= 3 ? 'warning' : 'secondary'}>
                          #{h.rank}
                        </Badge>
                      </td>
                      <td>
                        <Link href={`/address/${h.address}`}>
                          <a className="font-monospace">{h.address}</a>
                        </Link>
                      </td>
                      <td>{formatBalance(h.balance)}</td>
                      <td>{formatUSD(h.balance_usd)}</td>
                      <td>{h.percentage.toFixed(2)}%</td>
                      <td>
                        {h.token_symbol}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </Table>
          )}
        </Card.Body>
        
        {totalPages > 1 && (
          <Card.Footer>
            <Pagination className="justify-content-end mb-0">
              <Pagination.First onClick={() => setPage(1)} disabled={page === 1} />
              <Pagination.Prev onClick={() => setPage(Math.max(1, page - 1))} disabled={page === 1} />
              <Pagination.Next onClick={() => setPage(Math.min(totalPages, page + 1))} disabled={page === totalPages} />
              <Pagination.Last onClick={() => setPage(totalPages)} disabled={page === totalPages} />
            </Pagination>
          </Card.Footer>
        )}
      </Card>
      
      <style jsx>{`
        .top-holders-page { padding: 1rem; }
        .font-monospace { font-family: monospace; }
      `}</style>
    </Container>
  );
};

export default TopHoldersPage;