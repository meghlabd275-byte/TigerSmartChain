/**
 * TigerScan - Verified Contracts Page
 */

import { useState, useEffect, useCallback } from 'react';
import Head from 'next/head';
import Link from 'next/link';
import { Box, Container, Typography, TextField, InputAdornment, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Chip, Avatar, Skeleton, Alert, Pagination } from '@mui/material';
import { Search, Code, CheckCircle } from '@mui/icons-material';
import api from '@/lib/api';
import type { Contract } from '@/types';

const PAGE_SIZE = 25;

export default function VerifiedPage() {
  const [contracts, setContracts] = useState<Contract[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState('');

  const fetchContracts = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await api.getVerifiedContracts({ page, limit: PAGE_SIZE });
      setContracts(res.items);
      setTotal(res.total);
    } catch (err) {
      setContracts([]);
      setTotal(0);
      setError(err instanceof Error ? err.message : 'Failed to load verified contracts');
    } finally {
      setLoading(false);
    }
  }, [page]);

  useEffect(() => {
    fetchContracts();
  }, [fetchContracts]);

  // The gateway has no server-side filter on this endpoint, so the search is
  // applied to the page in view rather than silently pretending to span all
  // results.
  const filtered = contracts.filter((c) => {
    const q = search.trim().toLowerCase();
    if (!q) return true;
    return (
      (c.contractName || '').toLowerCase().includes(q) ||
      (c.address || '').toLowerCase().includes(q)
    );
  });

  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));

  return (
    <>
      <Head><title>Verified Contracts | TigerScan</title></Head>
      <Container maxWidth="xl" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom>
          <CheckCircle sx={{ mr: 1, verticalAlign: 'middle' }} />
          Verified Contracts
        </Typography>

        <TextField
          fullWidth
          placeholder="Filter by name or address on this page..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          InputProps={{ startAdornment: <InputAdornment position="start"><Search /></InputAdornment> }}
          sx={{ mb: 3 }}
        />

        {error && <Alert severity="error" sx={{ mb: 3 }}>{error}</Alert>}

        {loading ? (
          <Skeleton variant="rectangular" height={400} />
        ) : filtered.length === 0 ? (
          <Box sx={{ py: 8, textAlign: 'center', color: 'text.secondary' }}>
            <Typography>
              {search ? 'No verified contract on this page matches that filter.' : 'No verified contracts yet.'}
            </Typography>
          </Box>
        ) : (
          <>
            <TableContainer>
              <Table>
                <TableHead>
                  <TableRow>
                    <TableCell>Contract</TableCell>
                    <TableCell>Address</TableCell>
                    <TableCell>Compiler</TableCell>
                    <TableCell>Version</TableCell>
                    <TableCell>License</TableCell>
                    <TableCell>Verified</TableCell>
                  </TableRow>
                </TableHead>
                <TableBody>
                  {filtered.map((c) => (
                    <TableRow key={c.address} hover>
                      <TableCell>
                        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                          <Avatar sx={{ width: 24, height: 24, bgcolor: 'primary.main' }}><Code /></Avatar>
                          <Link href={`/address/${c.address}`} style={{ color: 'inherit' }}>
                            {c.contractName || 'Unnamed contract'}
                          </Link>
                        </Box>
                      </TableCell>
                      <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>
                        <Link href={`/address/${c.address}`} style={{ color: 'inherit' }}>
                          {c.address}
                        </Link>
                      </TableCell>
                      <TableCell>{c.compiler || '—'}</TableCell>
                      <TableCell>{c.compilerVersion ? <Chip label={c.compilerVersion} size="small" /> : '—'}</TableCell>
                      <TableCell>{c.licenseType || '—'}</TableCell>
                      <TableCell>
                        {c.verifiedAt ? new Date(c.verifiedAt).toLocaleDateString() : '—'}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </TableContainer>

            {pageCount > 1 && (
              <Box sx={{ display: 'flex', justifyContent: 'center', mt: 3 }}>
                <Pagination count={pageCount} page={page} onChange={(_, p) => setPage(p)} color="primary" />
              </Box>
            )}
          </>
        )}
      </Container>
    </>
  );
}
