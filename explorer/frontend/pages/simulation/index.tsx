// Transaction Simulation Page
// Production-grade transaction simulator with safety checks

import React, { useState } from 'react';
import { Container, Row, Col, Card, Form, Button, Alert, Spinner } from 'react-bootstrap';

interface SimulationResult {
  success: boolean;
  gas_used: number;
  gas_price: string;
  total_cost: string;
  state_changes: { address: string; key: string; old_value: string; new_value: string }[];
  logs: { address: string; topics: string[]; data: string }[];
  reverted: boolean;
  revert_reason: string | null;
}

const SimulationPage: React.FC = () => {
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [value, setValue] = useState('');
  const [data, setData] = useState('');
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<SimulationResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const simulate = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError(null);
    setResult(null);

    try {
      const res = await fetch('/api/v1/simulate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ from, to, value, data })
      });
      
      if (!res.ok) throw new Error('Simulation failed');
      const data = await res.json();
      setResult(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <Container fluid className="simulation-page">
      <Row className="mb-4">
        <Col><h1>Transaction Simulator</h1><p className="text-muted">Preview transaction outcomes before sending</p></Col>
      </Row>

      <Row>
        <Col md={6}>
          <Card>
            <Card.Body>
              <Form onSubmit={simulate}>
                <Form.Group className="mb-3">
                  <Form.Label>From Address</Form.Label>
                  <Form.Control type="text" placeholder="0x..." value={from} onChange={e=>setFrom(e.target.value)} required />
                </Form.Group>
                <Form.Group className="mb-3">
                  <Form.Label>To Address</Form.Label>
                  <Form.Control type="text" placeholder="0x..." value={to} onChange={e=>setTo(e.target.value)} required />
                </Form.Group>
                <Form.Group className="mb-3">
                  <Form.Label>Value (TGR)</Form.Label>
                  <Form.Control type="number" placeholder="0" value={value} onChange={e=>setValue(e.target.value)} />
                </Form.Group>
                <Form.Group className="mb-3">
                  <Form.Label>Data (Hex)</Form.Label>
                  <Form.Control as="textarea" rows={4} placeholder="0x..." value={data} onChange={e=>setData(e.target.value)} />
                </Form.Group>
                <Button type="submit" variant="primary" disabled={loading}>
                  {loading ? <><Spinner size="sm" /> Simulating...</> : 'Simulate Transaction'}
                </Button>
              </Form>
            </Card.Body>
          </Card>
        </Col>

        <Col md={6}>
          {error && <Alert variant="danger">{error}</Alert>}
          
          {result && (
            <Card className={result.reverted ? 'border-danger' : 'border-success'}>
              <Card.Header>
                {result.reverted ? <span className="text-danger">Transaction Would Fail</span> : <span className="text-success">Transaction Would Succeed</span>}
              </Card.Body>
              <Card.Text>
                <strong>Gas Used:</strong> {result.gas_used.toLocaleString()}<br />
                <strong>Gas Price:</strong> {result.gas_price} Wei<br />
                <strong>Total Cost:</strong> {result.total_cost} Wei<br />
                {result.revert_reason && (
                  <>
                    <strong className="text-danger">Revert Reason:</strong> {result.revert_reason}
                  </>
                )}
              </Card.Text>
              {result.state_changes.length > 0 && (
                <>
                  <h6>State Changes:</h6>
                  <ul>
                    {result.state_changes.map((change, i) => (
                      <li key={i}>
                        <small>{change.address}: {change.key} {change.old_value} → {change.new_value}</small>
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </Card>
          )}
        </Col>
      </Row>

      <style jsx>{`.simulation-page { padding: 1rem; }`}</style>
    </Container>
  );
};

export default SimulationPage;