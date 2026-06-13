// Uncle Blocks Page
// Ommer/uncle blocks explorer

import React, { useState, useEffect, useCallback } from 'react';
import Link from 'next/link';
import { Container, Row, Col, Card, Table, Badge, Button, Input, Pagination, Spinner, Alert } from 'react-bootstrap';

interface Uncle {
  number: number;
  hash: string;
  parent_hash: string;
  miner: string;
  difficulty: string;
  timestamp: number;
  gas_used: number;
  gas_limit: number;
  reward: string;
}

const UncleBlocksPage: React.FC = () => {
  const [uncles, setUncles] = useState<Uncle[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(25);
  
  const fetchUncles = useCallback(async () => {
    setLoading(true);
    setError(null);
    
    try {
      const response = await fetch(`/api/v1/uncles?page=${page}&limit=${limit}`);
      
      if (!response.ok) {
        throw new Error('Failed to fetch uncles');
      }
      
      const data = await response.json();
      setUncles(data.uncles || []);
      setTotal(data.total || 0);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [page, limit]);
  
  useEffect(() => {
    fetchUncles();
  }, [fetchUncles]);
  
  const formatDate = (timestamp: number) => {
    return new Date(timestamp * 1000).toLocaleString('en-US', {
      month: 'short', day: 'numeric', hour: 'numeric', minute: 'numeric'
    });
  };
  
  const formatGas = (used: number, limit: number) => {
    return ((used / limit) * 100).toFixed(1) + '%';
  };
  
  const totalPages = Math.ceil(total / limit);
  
  return (
    <Container fluid className="uncle-blocks-page">
      <Row className="mb-4">
        <Col>
          <h1>Uncle Blocks</h1>
          <p className="text-muted">
            Ommer blocks (uncles) - blocks not included in the canonical chain
          </p>
        </Col>
      </Row>
      
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
                  <th>Block</th>
                  <th>Hash</th>
                  <th>Parent Hash</th>
                  <th>Miner</th>
                  <th>Gas Used</th>
                  <th>Timestamp</th>
                  <th>Reward</th>
                </tr>
              </thead>
              <tbody>
                {uncles.length === 0 ? (
                  <tr><td colSpan={7} className="text-center py-4">No uncle blocks</td></tr>
                ) : (
                  uncles.map((uncle) => (
                    <tr key={uncle.hash}>
                      <td><Badge bg="warning">#{uncle.number}</Badge></td>
                      <td><code className="text-truncate" style={{maxWidth: '120px'}}>{uncle.hash}</code></td>
                      <td><code className="text-truncate" style={{maxWidth: '120px'}}>{uncle.parent_hash}</code></td>
                      <td>
                        <Link href={`/address/${uncle.miner}`}>
                          <a>{uncle.miner}</a>
                        </Link>
                      </td>
                      <td>{formatGas(uncle.gas_used, uncle.gas_limit)}</td>
                      <td>{formatDate(uncle.timestamp)}</td>
                      <td>{uncle.reward} TGR</td>
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
              <Pagination.Item active>{page}</Pagination.Item>
              <Pagination.Next onClick={() => setPage(Math.min(totalPages, page + 1))} disabled={page === totalPages} />
              <Pagination.Last onClick={() => setPage(totalPages)} disabled={page === totalPages} />
            </Pagination>
          </Card.Footer>
        )}
      </Card>
      
      <style jsx>{`
        .uncle-blocks-page { padding: 1rem; }
      `}</style>
    </Container>
  );
};

export default UncleBlocksPage;