// DEX Pairs Page
// Production-grade DEX pairs explorer with liquidity and volume

import React, { useState, useEffect, useCallback } from 'react';
import Link from 'next/link';
import { Container, Row, Col, Card, Table, Badge, Button, Input, Select, Pagination, Spinner, Alert } from 'react-bootstrap';

interface DEXPair {
  id: string;
  dex_name: string;
  token0: { symbol: string; address: string };
  token1: { symbol: string; address: string };
  reserve0: string;
  reserve1: string;
  volume_24h: number;
  volume_7d: number;
  fees_24h: number;
  liquidity: number;
  price: number;
}

const DEXPairsPage: React.FC = () => {
  const [pairs, setPairs] = useState<DEXPair[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(25);
  const [search, setSearch] = useState('');
  const [sortBy, setSortBy] = useState('liquidity');
  const [filterDex, setFilterDex] = useState('');
  
  const fetchPairs = useCallback(async () => {
    setLoading(true);
    try {
      const params = new URLSearchParams({
        page: page.toString(),
        limit: limit.toString(),
        sort: sortBy,
      });
      if (search) params.append('q', search);
      if (filterDex) params.append('dex', filterDex);
      
      const res = await fetch(`/api/v1/dex/pairs?${params}`);
      if (!res.ok) throw new Error('Failed to fetch');
      const data = await res.json();
      setPairs(data.pairs || []);
      setTotal(data.total || 0);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error');
    } finally {
      setLoading(false);
    }
  }, [page, limit, search, sortBy, filterDex]);
  
  useEffect(() => { fetchPairs(); }, [fetchPairs]);
  
  const formatUSD = (v: number) => v.toLocaleString('en-US', { style: 'currency', currency: 'USD' });
  const formatNum = (v: number) => v >= 1e6 ? (v/1e6).toFixed(2)+'M' : v >= 1e3 ? (v/1e3).toFixed(2)+'K' : v.toFixed(2);
  
  return (
    <Container fluid className="dex-pairs-page">
      <Row className="mb-4"><Col><h1>DEX Pairs</h1><p className="text-muted">Liquidity pools and trading pairs</p></Col></Row>
      
      <Card className="mb-4">
        <Card.Body>
          <Row className="g-3">
            <Col md={4}><Input placeholder="Search pairs..." value={search} onChange={e=>setSearch(e.target.value)} /></Col>
            <Col md={2}><Select value={filterDex} onChange={e=>setFilterDex(e.target.value)}><option value="">All DEXs</option><option value="pancakeswap">PancakeSwap</option><option value="biswap">BiSwap</option></Select></Col>
            <Col md={2}><Select value={limit.toString()} onChange={e=>setLimit(parseInt(e.target.value))}><option value="25">25</option><option value="50">50</option><option value="100">100</option></Select></Col>
            <Col md={2}><Button onClick={fetchPairs}>Search</Button></Col>
          </Row>
        </Card.Body>
      </Card>
      
      <Card>
        <Card.Body className="p-0">
          {error && <Alert variant="danger" className="m-3">{error}</Alert>}
          {loading ? <div className="text-center py-5"><Spinner/></div> : (
            <Table responsive hover>
              <thead><tr><th>Pair</th><th>DEX</th><th>Liquidity</th><th>Volume 24h</th><th>Volume 7d</th><th>Fees 24h</th></tr></thead>
              <tbody>
                {pairs.length === 0 ? <tr><td colSpan={6} className="text-center">No pairs</td></tr> : pairs.map(p => (
                  <tr key={p.id}>
                    <td><Link href={`/pair/${p.id}`}><a>{p.token0.symbol}/{p.token1.symbol}</a></Link></td>
                    <td><Badge bg="primary">{p.dex_name}</Badge></td>
                    <td>{formatUSD(p.liquidity)}</td>
                    <td>{formatUSD(p.volume_24h)}</td>
                    <td>{formatUSD(p.volume_7d)}</td>
                    <td>{formatUSD(p.fees_24h)}</td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}
        </Card.Body>
      </Card>
      
      <style jsx>{`.dex-pairs-page { padding: 1rem; }`}</style>
    </Container>
  );
};

export default DEXPairsPage;