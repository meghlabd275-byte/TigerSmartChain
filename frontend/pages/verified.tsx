/**
 * TigerScan - Verified Contracts Page
 */

import { useState, useEffect } from 'react';
import Head from 'next/head';
import { Box, Container, Typography, Grid, Card, CardContent, TextField, InputAdornment, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Chip, Avatar, Skeleton } from '@mui/material';
import { Search, Code, CheckCircle } from '@mui/icons-material';

interface Contract { address: string; name: string; compiler: string; version: string; verified: number; }

export default function VerifiedPage() {
  const [contracts, setContracts] = useState<Contract[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  
  useEffect(() => {
    setTimeout(() => {
      setContracts(Array.from({length: 20}, (_, i) => ({
        address: `0x${Math.random().toString(16).slice(2, 42)}`,
        name: ['Token', 'NFT', 'Staking', 'Governance', 'Bridge'][i % 5] + 'Contract' + i,
        compiler: 'solc',
        version: '0.8.19',
        verified: Date.now() - i * 86400000
      })));
      setLoading(false);
    }, 500);
  }, []);
  
  const filtered = contracts.filter(c => c.name.toLowerCase().includes(search.toLowerCase()) || c.address.includes(search));
  
  return (
    <>
      <Head><title>Verified Contracts | TigerScan</title></Head>
      <Container maxWidth="xl" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom><CheckCircle sx={{ mr: 1 }} />Verified Contracts</Typography>
        
        <TextField fullWidth placeholder="Search verified contracts..." value={search} onChange={(e) => setSearch(e.target.value)} InputProps={{ startAdornment: <InputAdornment position="start"><Search /></InputAdornment> }} sx={{ mb: 3 }} />
        
        {loading ? <Skeleton variant="rectangular" height={400} /> : (
          <TableContainer>
            <Table>
              <TableHead><TableRow><TableCell>Contract</TableCell><TableCell>Address</TableCell><TableCell>Compiler</TableCell><TableCell>Version</TableCell><TableCell>Verified</TableCell></TableRow></TableHead>
              <TableBody>
                {filtered.map((c, i) => (
                  <TableRow key={i} hover>
                    <TableCell><Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}><Avatar sx={{ width: 24, height: 24, bgcolor: 'primary.main' }}><Code /></Avatar>{c.name}</Box></TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{c.address.substring(0, 10)}...</TableCell>
                    <TableCell>{c.compiler}</TableCell>
                    <TableCell><Chip label={c.version} size="small" /></TableCell>
                    <TableCell>{new Date(c.verified).toLocaleDateString()}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        )}
      </Container>
    </>
  );
}