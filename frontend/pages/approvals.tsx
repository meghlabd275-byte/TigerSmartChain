/**
 * TigerScan - Token Approvals
 *
 * Lists token allowances from the indexer with the risk label the gateway
 * derives from the threat tables. Reads GET /api/v1/approvals and
 * GET /api/v1/approvals/stats.
 */

import { useState, useEffect, useCallback } from 'react';
import Head from 'next/head';
import { Box, Container, Grid, Card, CardContent, Typography, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Button, Alert, TextField, InputAdornment, Chip, Skeleton } from '@mui/material';
import { Search, Warning, CheckCircle, FilterList } from '@mui/icons-material';
import api from '@/lib/api';

type Risk = 'safe' | 'warning' | 'danger';

interface Approval {
  tokenAddress: string;
  owner: string;
  spender: string;
  value: string;
  transactionHash: string;
  blockNumber: number;
  risk: Risk;
}

interface ApprovalStats {
  total: number;
  safe: number;
  warning: number;
  danger: number;
}

// 2^256 - 1 as a decimal string. A value equal to this is the "unlimited"
// allowance convention. Compared as strings so the page keeps working on the
// project's ES2017 target, where BigInt is unavailable.
const MAX_UINT256_DEC = '115792089237316195423570985008687907853269984665640564039457584007913129639935';

const groupThousands = (digits: string): string =>
  digits.replace(/\B(?=(\d{3})+(?!\d))/g, ',');

const formatAllowance = (value: string): string => {
  if (!value) return '0';
  const digits = value.replace(/^0+/, '') || '0';
  if (digits === MAX_UINT256_DEC) return 'Unlimited';

  // Allowances carry 18 decimals; shift the point left to show the whole part.
  const whole = digits.length > 18 ? digits.slice(0, -18) : '0';
  return groupThousands(whole);
};

const shortAddress = (addr: string) => (addr ? `${addr.slice(0, 10)}...${addr.slice(-8)}` : '');

export default function ApprovalsPage() {
  const [approvals, setApprovals] = useState<Approval[]>([]);
  const [stats, setStats] = useState<ApprovalStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [filter, setFilter] = useState<'all' | Risk>('all');

  const fetchApprovals = useCallback(async () => {
    setLoading(true);
    try {
      const [list, summary] = await Promise.all([
        api.getApprovals({ limit: 100 }),
        api.getApprovalStats(),
      ]);
      setApprovals((list.items as unknown as Approval[]) ?? []);
      setStats(summary as ApprovalStats);
      setError(null);
    } catch (err) {
      // Do not substitute invented allowances. An approvals page that shows
      // spenders a user never approved is worse than one that shows nothing.
      setApprovals([]);
      setStats(null);
      setError(err instanceof Error ? err.message : 'Failed to load approvals');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { fetchApprovals(); }, [fetchApprovals]);

  const filteredApprovals = approvals.filter(a => {
    if (filter !== 'all' && a.risk !== filter) return false;
    if (search) {
      const q = search.toLowerCase();
      return a.spender.toLowerCase().includes(q) || a.owner.toLowerCase().includes(q) || a.tokenAddress.toLowerCase().includes(q);
    }
    return true;
  });

  const getRiskColor = (risk: string) => risk === 'safe' ? 'success' : risk === 'warning' ? 'warning' : 'error';

  if (loading) return <Box sx={{ p: 4 }}><Skeleton variant="rectangular" height={400} /></Box>;

  return (
    <>
      <Head><title>Token Approvals | TigerScan</title></Head>
      <Container maxWidth="xl" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom>Token Approvals</Typography>

        {error && <Alert severity="error" sx={{ mb: 3 }}>{error}</Alert>}

        {stats && (
          <Grid container spacing={2} sx={{ mb: 3 }}>
            {(['total', 'safe', 'warning', 'danger'] as const).map(key => (
              <Grid item xs={6} md={3} key={key}>
                <Card>
                  <CardContent>
                    <Typography variant="overline" color="text.secondary">{key}</Typography>
                    <Typography variant="h5">{(stats[key] ?? 0).toLocaleString()}</Typography>
                  </CardContent>
                </Card>
              </Grid>
            ))}
          </Grid>
        )}

        <Grid container spacing={2} sx={{ mb: 3 }}>
          <Grid item xs={12} md={6}><TextField fullWidth placeholder="Search address..." value={search} onChange={(e) => setSearch(e.target.value)} InputProps={{ startAdornment: <InputAdornment position="start"><Search /></InputAdornment> }} /></Grid>
          <Grid item xs={12} md={6}><Button variant="outlined" startIcon={<FilterList />} onClick={() => setFilter(filter === 'all' ? 'danger' : filter === 'danger' ? 'warning' : filter === 'warning' ? 'safe' : 'all')}>Filter: {filter.toUpperCase()}</Button></Grid>
        </Grid>

        <Card>
          <TableContainer>
            <Table>
              <TableHead><TableRow><TableCell>Owner</TableCell><TableCell>Token</TableCell><TableCell>Spender</TableCell><TableCell>Allowance</TableCell><TableCell>Risk</TableCell><TableCell>Transaction</TableCell></TableRow></TableHead>
              <TableBody>
                {filteredApprovals.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={6} align="center" sx={{ py: 4, color: 'text.secondary' }}>
                      No approvals match this view.
                    </TableCell>
                  </TableRow>
                ) : filteredApprovals.map((approval) => (
                  <TableRow key={`${approval.transactionHash}-${approval.spender}`} hover>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{shortAddress(approval.owner)}</TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{shortAddress(approval.tokenAddress)}</TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{shortAddress(approval.spender)}</TableCell>
                    <TableCell>{formatAllowance(approval.value)}</TableCell>
                    <TableCell><Chip icon={approval.risk === 'safe' ? <CheckCircle /> : <Warning />} label={approval.risk.toUpperCase()} color={getRiskColor(approval.risk)} size="small" /></TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{shortAddress(approval.transactionHash)}</TableCell>
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
