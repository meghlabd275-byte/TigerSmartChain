/**
 * TigerScan - Token Approvals Page
 * 
 * Track and manage token approvals
 */

import { useState, useEffect, useCallback } from 'react';
import Head from 'next/head';
import { Box, Container, Grid, Card, CardContent, Typography, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Button, Alert, TextField, InputAdornment, Chip, Avatar, Skeleton } from '@mui/material';
import { Search, Warning, CheckCircle, FilterList } from '@mui/icons-material';

const API_CONFIG = { baseUrl: process.env.NEXT_PUBLIC_API_URL || '/api' };

interface Approval { spender: string; token: string; owner: string; allowance: string; timestamp: number; risk: 'safe' | 'warning' | 'danger'; }

class ApiClient {
  private baseUrl: string;
  constructor(baseUrl: string = API_CONFIG.baseUrl) { this.baseUrl = baseUrl; }
  async get<T>(endpoint: string): Promise<T> {
    const response = await fetch(`${this.baseUrl}${endpoint}`);
    if (!response.ok) throw new Error(`API Error: ${response.status}`);
    return response.json();
  }
}

export default function ApprovalsPage() {
  const [approvals, setApprovals] = useState<Approval[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [filter, setFilter] = useState<'all' | 'safe' | 'warning' | 'danger'>('all');
  
  const apiClient = new ApiClient();
  
  const fetchApprovals = useCallback(async () => {
    setLoading(true);
    try {
      const data = await apiClient.get<Approval[]>('/approvals?limit=100');
      setApprovals(data);
    } catch (err) {
      // Mock data
      setApprovals(Array.from({length: 20}, (_, i) => ({
        spender: `0x${Math.random().toString(16).slice(2, 42)}`,
        token: `0x${Math.random().toString(16).slice(2, 42)}`,
        owner: `0x${Math.random().toString(16).slice(2, 42)}`,
        allowance: (Math.random() * 1000000).toFixed(2),
        timestamp: Date.now() - Math.random() * 86400000 * 30,
        risk: ['safe', 'warning', 'danger'][Math.floor(Math.random() * 3)] as any
      })));
    } finally { setLoading(false); }
  }, [apiClient]);
  
  useEffect(() => { fetchApprovals(); }, [fetchApprovals]);
  
  const filteredApprovals = approvals.filter(a => {
    if (filter !== 'all' && a.risk !== filter) return false;
    if (search && !a.spender.toLowerCase().includes(search.toLowerCase()) && !a.owner.toLowerCase().includes(search.toLowerCase())) return false;
    return true;
  });
  
  const getRiskColor = (risk: string) => risk === 'safe' ? 'success' : risk === 'warning' ? 'warning' : 'error';
  
  if (loading) return <Box sx={{ p: 4 }}><Skeleton variant="rectangular" height={400} /></Box>;
  
  return (
    <>
      <Head><title>Token Approvals | TigerScan</title></Head>
      <Container maxWidth="xl" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom>Token Approvals</Typography>
        
        <Grid container spacing={2} sx={{ mb: 3 }}>
          <Grid item xs={12} md={6}><TextField fullWidth placeholder="Search address..." value={search} onChange={(e) => setSearch(e.target.value)} InputProps={{ startAdornment: <InputAdornment position="start"><Search /></InputAdornment> }} /></Grid>
          <Grid item xs={12} md={6}><Button variant="outlined" startIcon={<FilterList />} onClick={() => setFilter(filter === 'all' ? 'danger' : filter === 'danger' ? 'warning' : filter === 'warning' ? 'safe' : 'all')}>Filter: {filter.toUpperCase()}</Button></Grid>
        </Grid>
        
        <Card>
          <TableContainer>
            <Table>
              <TableHead><TableRow><TableCell>Owner</TableCell><TableCell>Token</TableCell><TableCell>Spender</TableCell><TableCell>Allowance</TableCell><TableCell>Risk</TableCell><TableCell>Action</TableCell></TableRow></TableHead>
              <TableBody>
                {filteredApprovals.map((approval, i) => (
                  <TableRow key={i} hover>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{approval.owner.substring(0, 10)}...</TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{approval.token.substring(0, 10)}...</TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{approval.spender.substring(0, 10)}...</TableCell>
                    <TableCell>{parseFloat(approval.allowance).toLocaleString()}</TableCell>
                    <TableCell><Chip icon={approval.risk === 'safe' ? <CheckCircle /> : <Warning />} label={approval.risk.toUpperCase()} color={getRiskColor(approval.risk)} size="small" /></TableCell>
                    <TableCell><Button size="small" variant="outlined">Revoke</Button></TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        </Card>
      </Container>
    </>
  );
}