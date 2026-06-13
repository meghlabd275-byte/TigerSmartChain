/**
 * TigerScan - Validator Details Page
 * 
 * Advanced implementation with:
 * - Validator info
 * - Performance metrics
 * - Rewards
 * - Delegations
 */

import { useState, useEffect, useCallback } from 'react';
import { useRouter } from 'next/router';
import Head from 'next/head';
import { 
  Box, 
  Container, 
  Grid, 
  Card, 
  CardContent, 
  Typography, 
  Table, 
  TableBody, 
  TableCell, 
  TableContainer, 
  TableHead, 
  TableRow,
  Chip,
  Button,
  Alert,
  Avatar,
  Paper,
  Divider,
  List,
  ListItem,
  ListItemText,
  ListItemIcon,
  Skeleton,
  LinearProgress
} from '@mui/material';
import { 
  HowToVote,
  TrendingUp,
  AccountBalanceWallet,
  Star,
  EmojiEvents,
  Group
} from '@mui/icons-material';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip as ChartTooltip, ResponsiveContainer } from 'recharts';

const API_CONFIG = {
  baseUrl: process.env.NEXT_PUBLIC_API_URL || '/api',
  chainId: 6666
};

interface ValidatorData {
  address: string;
  name?: string;
  status: 'active' | 'inactive' | 'jailed';
  uptime: number;
  totalStaked: string;
  selfStaked: string;
  delegators: number;
  commission: number;
  rewards: string;
  slashCount: number;
  lastProposal: number;
  performance30d: number;
  rank: number;
  delegations: Delegation[];
  rewardsHistory: { timestamp: number; rewards: number }[];
}

interface Delegation {
  delegator: string;
  amount: string;
  rewards: string;
}

class ApiClient {
  private baseUrl: string;
  constructor(baseUrl: string = API_CONFIG.baseUrl) {
    this.baseUrl = baseUrl;
  }
  async get<T>(endpoint: string): Promise<T> {
    const response = await fetch(`${this.baseUrl}${endpoint}`);
    if (!response.ok) throw new Error(`API Error: ${response.status}`);
    return response.json();
  }
}

export default function ValidatorPage() {
  const router = useRouter();
  const { validator } = router.query;
  const [data, setData] = useState<ValidatorData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  
  const apiClient = new ApiClient();
  
  const fetchData = useCallback(async () => {
    if (!validator) return;
    setLoading(true);
    setError(null);
    try {
      const address = validator.toString();
      if (!/^0x[a-fA-F0-9]{40}$/.test(address)) {
        throw new Error('Invalid validator address');
      }
      const result = await apiClient.get<ValidatorData>(`/validator/${address}`);
      setData(result);
    } catch (err: any) {
      setError(err.message || 'Failed to fetch validator data');
    } finally {
      setLoading(false);
    }
  }, [validator, apiClient]);
  
  useEffect(() => { fetchData(); }, [fetchData]);
  
  const formatBalance = (balance: string | number): string => {
    const num = typeof balance === 'string' ? parseFloat(balance) : balance;
    if (num >= 1e9) return `${(num / 1e9).toFixed(2)}B`;
    if (num >= 1e6) return `${(num / 1e6).toFixed(2)}M`;
    if (num >= 1e3) return `${(num / 1e3).toFixed(2)}K`;
    return num.toFixed(2);
  };
  
  const getStatusColor = (status: string): 'success' | 'error' | 'warning' => {
    if (status === 'active') return 'success';
    if (status === 'inactive') return 'warning';
    return 'error';
  };
  
  if (loading) return <Box sx={{ p: 4 }}><Skeleton variant="rectangular" height={200} /><Skeleton variant="rectangular" height={400} /></Box>;
  if (error) return <Container maxWidth="lg" sx={{ py: 4 }}><Alert severity="error">{error}</Alert><Button onClick={fetchData}>Retry</Button></Container>;
  if (!data) return <Container maxWidth="lg" sx={{ py: 4 }}><Alert severity="warning">Validator not found</Alert></Container>;
  
  return (
    <>
      <Head><title>{data.name || data.address} | TigerScan</title></Head>
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Paper sx={{ p: 3, mb: 3 }}>
          <Grid container spacing={3} alignItems="center">
            <Grid item><Avatar sx={{ width: 64, height: 64, bgcolor: 'primary.main' }}><HowToVote sx={{ fontSize: 32 }} /></Avatar></Grid>
            <Grid item xs>
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                <Typography variant="h4">{data.name || 'Validator'}</Typography>
                <Chip label={`Rank #${data.rank}`} color="primary" size="small" />
                <Chip label={data.status} color={getStatusColor(data.status)} size="small" />
              </Box>
              <Typography variant="body2" sx={{ fontFamily: 'monospace' }}>{data.address}</Typography>
            </Grid>
          </Grid>
        </Paper>
        
        <Grid container spacing={2} sx={{ mb: 3 }}>
          <Grid item xs={6} md={3}><Card><CardContent sx={{ textAlign: 'center' }}><Typography variant="h5">{formatBalance(data.totalStaked)}</Typography><Typography variant="body2" color="text.secondary">Total Staked</Typography></CardContent></Card></Grid>
          <Grid item xs={6} md={3}><Card><CardContent sx={{ textAlign: 'center' }}><Typography variant="h5">{data.delegators}</Typography><Typography variant="body2" color="text.secondary">Delegators</Typography></CardContent></Card></Grid>
          <Grid item xs={6} md={3}><Card><CardContent sx={{ textAlign: 'center' }}><Typography variant="h5">{data.uptime}%</Typography><Typography variant="body2" color="text.secondary">Uptime</Typography></CardContent></Card></Grid>
          <Grid item xs={6} md={3}><Card><CardContent sx={{ textAlign: 'center' }}><Typography variant="h5">{data.performance30d}%</Typography><Typography variant="body2" color="text.secondary">Performance (30d)</Typography></CardContent></Card></Grid>
        </Grid>
        
        <Grid container spacing={3}>
          <Grid item xs={12} md={6}>
            <Card>
              <CardContent>
                <Typography variant="h6" gutterBottom>Details</Typography>
                <List>
                  <ListItem><ListItemText primary="Self Staked" secondary={data.selfStaked} /></ListItem>
                  <ListItem><ListItemText primary="Commission" secondary={`${data.commission}%`} /></ListItem>
                  <ListItem><ListItemText primary="Total Rewards" secondary={data.rewards} /></ListItem>
                  <ListItem><ListItemText primary="Slash Count" secondary={data.slashCount.toString()} /></ListItem>
                  <ListItem><ListItemText primary="Last Proposal" secondary={`Block #${data.lastProposal}`} /></ListItem>
                </List>
              </CardContent>
            </Card>
          </Grid>
          <Grid item xs={12} md={6}>
            <Card>
              <CardContent>
                <Typography variant="h6" gutterBottom><Group sx={{ mr: 1 }} />Delegations</Typography>
                <TableContainer>
                  <Table size="small">
                    <TableHead><TableRow><TableCell>Delegator</TableCell><TableCell>Amount</TableCell></TableRow></TableHead>
                    <TableBody>
                      {data.delegations.slice(0, 10).map((d, i) => (
                        <TableRow key={i}><TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{d.delegator.substring(0, 8)}...</TableCell><TableCell>{formatBalance(d.amount)}</TableCell></TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </TableContainer>
              </CardContent>
            </Card>
          </Grid>
        </Grid>
      </Container>
    </>
  );
}