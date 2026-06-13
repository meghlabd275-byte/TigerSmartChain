// Verified Contracts List Page
// Production-grade verified contracts list with search, filter, pagination

import React, { useState, useEffect, useCallback } from 'react';
import { useRouter } from 'next/router';
import Link from 'next/link';
import { 
  Container, Row, Col, Card, Table, Badge, Button, 
  Input, Select, Pagination, Spinner, Alert, Form, Modal 
} from 'react-bootstrap';

interface Contract {
  address: string;
  contract_name: string;
  compiler_version: string;
  optimizer: boolean;
  optimizer_runs: number;
  evm_version: string;
  license: string;
  verified_at: string;
  tx_count: number;
}

interface Props {
  contracts?: Contract[];
  total?: number;
  page?: number;
  limit?: number;
}

const VerifiedContractsPage: React.FC<Props> = () => {
  const router = useRouter();
  const [contracts, setContracts] = useState<Contract[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(25);
  const [search, setSearch] = useState('');
  const [sortBy, setSortBy] = useState('verified_at');
  const [sortOrder, setSortOrder] = useState('desc');
  const [filterLicense, setFilterLicense] = useState('');
  const [filterOptimizer, setFilterOptimizer] = useState('');
  
  const fetchContracts = useCallback(async () => {
    setLoading(true);
    setError(null);
    
    try {
      const params = new URLSearchParams({
        page: page.toString(),
        limit: limit.toString(),
        sort_by: sortBy,
        sort_order: sortOrder,
      });
      
      if (search) params.append('q', search);
      if (filterLicense) params.append('license', filterLicense);
      if (filterOptimizer) params.append('optimizer', filterOptimizer);
      
      const response = await fetch(`/api/v1/verified?${params}`);
      
      if (!response.ok) {
        throw new Error('Failed to fetch contracts');
      }
      
      const data = await response.json();
      setContracts(data.contracts || []);
      setTotal(data.total || 0);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [page, limit, search, sortBy, sortOrder, filterLicense, filterOptimizer]);
  
  useEffect(() => {
    fetchContracts();
  }, [fetchContracts]);
  
  const handleSort = (column: string) => {
    if (sortBy === column) {
      setSortOrder(sortOrder === 'asc' ? 'desc' : 'asc');
    } else {
      setSortBy(column);
      setSortOrder('asc');
    }
  };
  
  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    setPage(1);
  };
  
  const totalPages = Math.ceil(total / limit);
  
  const formatDate = (dateStr: string) => {
    return new Date(dateStr).toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  };
  
  const licenses = ['MIT', 'GPL-3.0', 'LGPL-3.0', 'BSD-3-Clause', 'BSD-2-Clause', 'Apache-2.0', 'AGPL-3.0', 'UNLICENSED'];
  
  return (
    <Container fluid className="verified-contracts-page">
      <Row className="mb-4">
        <Col>
          <h1>Verified Contracts</h1>
          <p className="text-muted">
            Browse and search verified smart contracts on TigerSmartChain
          </p>
        </Col>
      </Row>
      
      {/* Search and Filter */}
      <Card className="mb-4">
        <Card.Body>
          <Form onSubmit={handleSearch}>
            <Row className="g-3">
              <Col md={4}>
                <Input
                  type="text"
                  placeholder="Search by name or address..."
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                />
              </Col>
              <Col md={2}>
                <Select
                  value={filterLicense}
                  onChange={(e) => setFilterLicense(e.target.value)}
                >
                  <option value="">All Licenses</option>
                  {licenses.map((l) => (
                    <option key={l} value={l}>{l}</option>
                  ))}
                </Select>
              </Col>
              <Col md={2}>
                <Select
                  value={filterOptimizer}
                  onChange={(e) => setFilterOptimizer(e.target.value)}
                >
                  <option value="">All</option>
                  <option value="true">Optimized</option>
                  <option value="false">Not Optimized</option>
                </Select>
              </Col>
              <Col md={2}>
                <Select
                  value={limit}
                  onChange={(e) => {
                    setLimit(parseInt(e.target.value));
                    setPage(1);
                  }}
                >
                  <option value="25">25 per page</option>
                  <option value="50">50 per page</option>
                  <option value="100">100 per page</option>
                </Select>
              </Col>
              <Col md={2}>
                <Button type="submit" variant="primary">
                  Search
                </Button>
              </Col>
            </Row>
          </Form>
        </Card.Body>
      </Card>
      
      {/* Results Table */}
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
                  <th 
                    onClick={() => handleSort('contract_name')}
                    className="cursor-pointer"
                  >
                    Contract {sortBy === 'contract_name' && (sortOrder === 'asc' ? ' ↑' : ' ↓')}
                  </th>
                  <th>Compiler</th>
                  <th>Optimizer</th>
                  <th>EVM</th>
                  <th 
                    onClick={() => handleSort('license')}
                    className="cursor-pointer"
                  >
                    License {sortBy === 'license' && (sortOrder === 'asc' ? ' ↑' : ' ↓')}
                  </th>
                  <th 
                    onClick={() => handleSort('verified_at')}
                    className="cursor-pointer"
                  >
                    Verified {sortBy === 'verified_at' && (sortOrder === 'asc' ? ' ↑' : ' ↓')}
                  </th>
                </tr>
              </thead>
              <tbody>
                {contracts.length === 0 ? (
                  <tr>
                    <td colSpan={6} className="text-center py-4">
                      No contracts found
                    </td>
                  </tr>
                ) : (
                  contracts.map((contract) => (
                    <tr key={contract.address}>
                      <td>
                        <Link href={`/contract/${contract.address}`}>
                          <a className="fw-bold">
                            {contract.contract_name}
                          </a>
                        </Link>
                        <br />
                        <small className="text-muted font-monospace">
                          {contract.address}
                        </small>
                      </td>
                      <td>
                        <code>{contract.compiler_version}</code>
                      </td>
                      <td>
                        {contract.optimizer ? (
                          <Badge bg="success">
                            Yes ({contract.optimizer_runs})
                          </Badge>
                        ) : (
                          <Badge bg="secondary">No</Badge>
                        )}
                      </td>
                      <td>{contract.evm_version}</td>
                      <td>
                        <Badge bg="light" text="dark">
                          {contract.license || 'N/A'}
                        </Badge>
                      </td>
                      <td>{formatDate(contract.verified_at)}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </Table>
          )}
        </Card.Body>
        
        {/* Pagination */}
        {totalPages > 1 && (
          <Card.Footer>
            <Row className="align-items-center">
              <Col md={4}>
                <span className="text-muted">
                  Showing {((page - 1) * limit) + 1} to {Math.min(page * limit, total)} of {total} contracts
                </span>
              </Col>
              <Col md={8}>
                <Pagination className="justify-content-end mb-0">
                  <Pagination.First 
                    onClick={() => setPage(1)}
                    disabled={page === 1}
                  />
                  <Pagination.Prev 
                    onClick={() => setPage(Math.max(1, page - 1))}
                    disabled={page === 1}
                  />
                  
                  {Array.from({ length: Math.min(5, totalPages) }, (_, i) => {
                    const pageNum = Math.max(1, Math.min(page - 2, totalPages - 4) + i);
                    return (
                      <Pagination.Item
                        key={pageNum}
                        active={pageNum === page}
                        onClick={() => setPage(pageNum)}
                      >
                        {pageNum}
                      </Pagination.Item>
                    );
                  })}
                  
                  <Pagination.Next 
                    onClick={() => setPage(Math.min(totalPages, page + 1))}
                    disabled={page === totalPages}
                  />
                  <Pagination.Last 
                    onClick={() => setPage(totalPages)}
                    disabled={page === totalPages}
                  />
                </Pagination>
              </Col>
            </Row>
          </Card.Footer>
        )}
      </Card>
      
      <style jsx>{`
        .verified-contracts-page {
          padding: 1rem;
        }
        .cursor-pointer {
          cursor: pointer;
        }
        .font-monospace {
          font-family: monospace;
        }
      `}</style>
    </Container>
  );
};

export default VerifiedContractsPage;