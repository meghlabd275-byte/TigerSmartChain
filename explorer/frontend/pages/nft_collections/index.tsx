// NFT Collections Page
// Production-grade NFT collections explorer with stats, filters

import React, { useState, useEffect, useCallback } from 'react';
import Link from 'next/link';
import { 
  Container, Row, Col, Card, Table, Badge, Button, 
  Input, Select, Pagination, Spinner, Alert, Form, Image 
} from 'react-bootstrap';

interface Collection {
  address: string;
  name: string;
  symbol: string;
  contract_type: string;
  total_supply: number;
  holder_count: number;
  volume_24h: number;
  volume_24h_usd: number;
  volume_7d: number;
  volume_7d_usd: number;
  floor_price: number;
  floor_price_usd: number;
  market_cap: number;
  image_url: string;
  category: string;
  verified: boolean;
}

interface Props {
  collections?: Collection[];
  total?: number;
  page?: number;
  limit?: number;
}

const NFTCollectionsPage: React.FC<Props> = () => {
  const [collections, setCollections] = useState<Collection[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(25);
  const [search, setSearch] = useState('');
  const [sortBy, setSortBy] = useState('volume_24h_usd');
  const [sortOrder, setSortOrder] = useState('desc');
  const [filterType, setFilterType] = useState('');
  
  const fetchCollections = useCallback(async () => {
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
      if (filterType) params.append('type', filterType);
      
      const response = await fetch(`/api/v1/nfts/collections?${params}`);
      
      if (!response.ok) {
        throw new Error('Failed to fetch collections');
      }
      
      const data = await response.json();
      setCollections(data.collections || []);
      setTotal(data.total || 0);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [page, limit, search, sortBy, sortOrder, filterType]);
  
  useEffect(() => {
    fetchCollections();
  }, [fetchCollections]);
  
  const handleSort = (column: string) => {
    if (sortBy === column) {
      setSortOrder(sortOrder === 'asc' ? 'desc' : 'asc');
    } else {
      setSortBy(column);
      setSortOrder('desc');
    }
  };
  
  const formatPrice = (price: number) => {
    if (!price || price === 0) return '-';
    return price.toLocaleString('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 2,
    });
  };
  
  const formatNumber = (num: number) => {
    if (!num) return '-';
    if (num >= 1000000) return (num / 1000000).toFixed(1) + 'M';
    if (num >= 1000) return (num / 1000).toFixed(1) + 'K';
    return num.toString();
  };
  
  const totalPages = Math.ceil(total / limit);
  
  return (
    <Container fluid className="nft-collections-page">
      <Row className="mb-4">
        <Col>
          <h1>NFT Collections</h1>
          <p className="text-muted">
            Explore NFT collections on TigerSmartChain
          </p>
        </Col>
      </Row>
      
      {/* Search and Filter */}
      <Card className="mb-4">
        <Card.Body>
          <Form onSubmit={(e) => { e.preventDefault(); setPage(1); fetchCollections(); }}>
            <Row className="g-3">
              <Col md={3}>
                <Input
                  type="text"
                  placeholder="Search collections..."
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                />
              </Col>
              <Col md={2}>
                <Select
                  value={filterType}
                  onChange={(e) => setFilterType(e.target.value)}
                >
                  <option value="">All Types</option>
                  <option value="ERC721">ERC721</option>
                  <option value="ERC1155">ERC1155</option>
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
      
      {/* Collections Grid */}
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
                  <th style={{ width: '80px' }}>Collection</th>
                  <th 
                    onClick={() => handleSort('name')}
                    className="cursor-pointer"
                  >
                    Name {sortBy === 'name' && (sortOrder === 'asc' ? ' ↑' : ' ↓')}
                  </th>
                  <th>Type</th>
                  <th 
                    onClick={() => handleSort('total_supply')}
                    className="cursor-pointer"
                  >
                    Items {sortBy === 'total_supply' && (sortOrder === 'asc' ? ' ↑' : ' ↓')}
                  </th>
                  <th 
                    onClick={() => handleSort('holder_count')}
                    className="cursor-pointer"
                  >
                    Owners {sortBy === 'holder_count' && (sortOrder === 'asc' ? ' ↑' : ' ↓')}
                  </th>
                  <th 
                    onClick={() => handleSort('floor_price_usd')}
                    className="cursor-pointer"
                  >
                    Floor {sortBy === 'floor_price_usd' && (sortOrder === 'asc' ? ' ↑' : ' ↓')}
                  </th>
                  <th 
                    onClick={() => handleSort('volume_24h_usd')}
                    className="cursor-pointer"
                  >
                    24h Vol {sortBy === 'volume_24h_usd' && (sortOrder === 'asc' ? ' ↑' : ' ↓')}
                  </th>
                  <th 
                    onClick={() => handleSort('volume_7d_usd')}
                    className="cursor-pointer"
                  >
                    7d Vol {sortBy === 'volume_7d_usd' && (sortOrder === 'asc' ? ' ↑' : ' ↓')}
                  </th>
                </tr>
              </thead>
              <tbody>
                {collections.length === 0 ? (
                  <tr>
                    <td colSpan={8} className="text-center py-4">
                      No collections found
                    </td>
                  </tr>
                ) : (
                  collections.map((collection) => (
                    <tr key={collection.address}>
                      <td>
                        {collection.image_url ? (
                          <Image 
                            src={collection.image_url} 
                            alt={collection.name}
                            width={48}
                            height={48}
                            rounded
                          />
                        ) : (
                          <div 
                            className="bg-secondary rounded d-flex align-items-center justify-content-center"
                            style={{ width: 48, height: 48 }}
                          >
                            <span className="text-white">?</span>
                          </div>
                        )}
                      </td>
                      <td>
                        <Link href={`/nft/${collection.address}`}>
                          <a className="fw-bold">
                            {collection.name}
                          </a>
                        </Link>
                        {collection.symbol && (
                          <br />
                        )}
                        {collection.symbol && (
                          <small className="text-muted">{collection.symbol}</small>
                        )}
                        {collection.verified && (
                          <Badge bg="success" className="ms-2">Verified</Badge>
                        )}
                      </td>
                      <td>
                        <Badge bg={collection.contract_type === 'ERC721' ? 'primary' : 'info'}>
                          {collection.contract_type}
                        </Badge>
                      </td>
                      <td>{formatNumber(collection.total_supply)}</td>
                      <td>{formatNumber(collection.holder_count)}</td>
                      <td>{formatPrice(collection.floor_price_usd)}</td>
                      <td>{formatPrice(collection.volume_24h_usd)}</td>
                      <td>{formatPrice(collection.volume_7d_usd)}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </Table>
          )}
        </Card.Body>
        
        {totalPages > 1 && (
          <Card.Footer>
            <Row className="align-items-center">
              <Col md={4}>
                <span className="text-muted">
                  Showing {((page - 1) * limit) + 1} to {Math.min(page * limit, total)} of {total} collections
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
        .nft-collections-page {
          padding: 1rem;
        }
        .cursor-pointer {
          cursor: pointer;
        }
      `}</style>
    </Container>
  );
};

export default NFTCollectionsPage;