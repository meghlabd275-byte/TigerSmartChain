/**
 * TigerScan - Token Details Page
 * 
 * Advanced implementation with:
 * - Token overview
 * - Price and market cap
 * - Holders
 * - Transfers
 * - Contract info
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
  Tabs,
  Tab,
  LinearProgress
} from '@mui/material';
import { 
  TrendingUp, 
  TrendingDown,
  AccountBalanceWallet,
  People,
  SwapHoriz,
  Code,
  Token,
  ShowChart
} from '@mui/icons-material';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip as ChartTooltip, ResponsiveContainer } from 'recharts';

const API_CONFIG = {
  baseUrl: process.env.NEXT_PUBLIC_API_URL || '/api',
  chainId: 6666
};

interface TokenData {
  address: string;
  name: string;
  symbol: string;
  decimals: number;
  totalSupply: string;
  circulatingSupply: string;
  price: number;
  priceChange24h: number;
  priceChangePercent24h: number;
  marketCap: number;
  volume24h: number;
  holders: number;
  transfers24h: number;
  isVerified: boolean;
  website?: string;
  twitter?: string;
  description?: string;
  priceHistory: { timestamp: number; price: number }[];
  holdersList: Holder[];
  transfers: Transfer[];
}

interface Holder {
  address: string;
  balance: string;
  percentage: number;
}

interface Transfer {
  hash: string;
  block: number;
  timestamp: number;
  from: string;
  to: string;
  value: string;
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

export default function TokenPage() {
  const router = useRouter();
  const { token } = router.query;
  const [tokenData, setTokenData] = useState<TokenData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState(0);
  
  const apiClient = new ApiClient();
  
  const fetchTokenData = useCallback(async () => {
    if (!token) return;
    
    setLoading(true);
    setError(null);
    
    try {
      const address = token.toString();
      if (!/^0x[a-fA-F0-9]{40}$/.test(address)) {
        throw new Error('Invalid token address');
      }
      
      const data = await apiClient.get<TokenData>(`/token/${address}`);
      setTokenData(data);
    } catch (err: any) {
      setError(err.message || 'Failed to fetch token data');
    } finally {
      setLoading(false);
    }
  }, [token, apiClient]);
  
  useEffect(() => {
    fetchTokenData();
  }, [fetchTokenData]);
  
  const formatBalance = (balance: string | number): string => {
    const num = typeof balance === 'string' ? parseFloat(balance) : balance;
    if (num >= 1e9) return `${(num / 1e9).toFixed(2)}B`;
    if (num >= 1e6) return `${(num / 1e6).toFixed(2)}M`;
    if (num >= 1e3) return `${(num / 1e3).toFixed(2)}K`;
    return num.toFixed(2);
  };
  
  const formatUsd = (value: number): string => {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD'
    }).format(value);
  };
  
  const formatPercent = (value: number): string => {
    return `${value >= 0 ? '+' : ''}${value.toFixed(2)}%`;
  };
  
  const formatHash = (hash: string): string => {
    if (!hash) return '-';
    return `${hash.substring(0, 8)}...${hash.substring(hash.length - 6)}`;
  };
  
  if (loading) {
    return (
      <Box sx={{ p: 4 }}>
        <Skeleton variant="rectangular" height={200} sx={{ mb: 2 }} />
        <Skeleton variant="rectangular" height={400} />
      </Box>
    );
  }
  
  if (error) {
    return (
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Alert severity="error" sx={{ mb: 2 }}>{error}</Alert>
        <Button variant="contained" onClick={fetchTokenData}>Retry</Button>
      </Container>
    );
  }
  
  if (!tokenData) {
    return (
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Alert severity="warning">Token not found</Alert>
      </Container>
    );
  }
  
  return (
    <>
      <Head>
        <title>{tokenData.name} ({tokenData.symbol}) | TigerScan</title>
      </Head>
      
      <Container maxWidth="lg" sx={{ py: 4 }}>
        {/* Header */}
        <Paper sx={{ p: 3, mb: 3 }}>
          <Grid container spacing={3} alignItems="center">
            <Grid item>
              <Avatar sx={{ width: 64, height: 64, bgcolor: 'primary.main' }}>
                <Token sx={{ fontSize: 32 }} />
              </Avatar>
            </Grid>
            <Grid item xs>
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                <Typography variant="h4">{tokenData.name}</Typography>
                <Chip label={tokenData.symbol} size="small" />
                {tokenData.isVerified && <Chip label="Verified" color="success" size="small" />}
              </Box>
              <Typography variant="body2" sx={{ fontFamily: 'monospace' }}>
                {tokenData.address}
              </Typography>
            </Grid>
            <Grid item xs={12} md={4}>
              <Card variant="outlined">
                <CardContent>
                  <Typography variant="h4">{formatUsd(tokenData.price)}</Typography>
                  <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                    {tokenData.priceChangePercent24h >= 0 ? (
                      <TrendingUp color="success" />
                    ) : (
                      <TrendingDown color="error" />
                    )}
                    <Typography 
                      variant="body2"
                      color={tokenData.priceChangePercent24h >= 0 ? 'success.main' : 'error.main'}
                    >
                      {formatPercent(tokenData.priceChangePercent24h)} (24h)
                    </Typography>
                  </Box>
                </CardContent>
              </Card>
            </Grid>
          </Grid>
        </Paper>
        
        {/* Stats */}
        <Grid container spacing={2} sx={{ mb: 3 }}>
          <Grid item xs={6} md={3}>
            <Card>
              <CardContent sx={{ textAlign: 'center' }}>
                <Typography variant="h5">{formatUsd(tokenData.marketCap)}</Typography>
                <Typography variant="body2" color="text.secondary">Market Cap</Typography>
              </CardContent>
            </Card>
          </Grid>
          <Grid item xs={6} md={3}>
            <Card>
              <CardContent sx={{ textAlign: 'center' }}>
                <Typography variant="h5">{formatUsd(tokenData.volume24h)}</Typography>
                <Typography variant="body2" color="text.secondary">Volume (24h)</Typography>
              </CardContent>
            </Card>
          </Grid>
          <Grid item xs={6} md={3}>
            <Card>
              <CardContent sx={{ textAlign: 'center' }}>
                <Typography variant="h5">{tokenData.holders.toLocaleString()}</Typography>
                <Typography variant="body2" color="text.secondary">Holders</Typography>
              </CardContent>
            </Card>
          </Grid>
          <Grid item xs={6} md={3}>
            <Card>
              <CardContent sx={{ textAlign: 'center' }}>
                <Typography variant="h5">{tokenData.transfers24h.toLocaleString()}</Typography>
                <Typography variant="body2" color="text.secondary">Transfers (24h)</Typography>
              </CardContent>
            </Card>
          </Grid>
        </Grid>
        
        {/* Price Chart */}
        {tokenData.priceHistory && tokenData.priceHistory.length > 0 && (
          <Card sx={{ mb: 3 }}>
            <CardContent>
              <Typography variant="h6" gutterBottom>
                <ShowChart sx={{ mr: 1, verticalAlign: 'middle' }} />
                Price Chart (30 Days)
              </Typography>
              <ResponsiveContainer width="100%" height={250}>
                <LineChart data={tokenData.priceHistory}>
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis 
                    dataKey="timestamp" 
                    tickFormatter={(ts) => new Date(ts * 1000).toLocaleDateString()} 
                  />
                  <YAxis tickFormatter={(v) => `$${v.toFixed(2)}`} />
                  <ChartTooltip formatter={(v: number) => formatUsd(v)} labelFormatter={(ts) => new Date(ts * 1000).toLocaleDateString()} />
                  <Line 
                    type="monotone" 
                    dataKey="price" 
                    stroke="#1976d2" 
                    strokeWidth={2}
                    dot={false}
                  />
                </LineChart>
              </ResponsiveContainer>
            </CardContent>
          </Card>
        )}
        
        {/* Tabs */}
        <Box sx={{ borderBottom: 1, borderColor: 'divider', mb: 3 }}>
          <Tabs value={tab} onChange={(_, v) => setTab(v)}>
            <Tab label="Holders" />
            <Tab label="Transfers" />
            <Tab label="Info" />
          </Tabs>
        </Box>
        
        {/* Holders Tab */}
        {tab === 0 && (
          <TableContainer>
            <Table>
              <TableHead>
                <TableRow>
                  <TableCell>#</TableCell>
                  <TableCell>Address</TableCell>
                  <TableCell>Balance</TableCell>
                  <TableCell>%</TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {tokenData.holdersList.map((holder, i) => (
                  <TableRow key={i} hover>
                    <TableCell>{i + 1}</TableCell>
                    <TableCell sx={{ fontFamily: 'monospace' }}>{holder.address}</TableCell>
                    <TableCell>{formatBalance(holder.balance)} {tokenData.symbol}</TableCell>
                    <TableCell>
                      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                        <LinearProgress 
                          variant="determinate" 
                          value={holder.percentage} 
                          sx={{ width: 60, height: 6, borderRadius: 3 }} 
                        />
                        <Typography variant="body2">{holder.percentage.toFixed(2)}%</Typography>
                      </Box>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        )}
        
        {/* Transfers Tab */}
        {tab === 1 && (
          <TableContainer>
            <Table>
              <TableHead>
                <TableRow>
                  <TableCell>Transaction</TableCell>
                  <TableCell>Block</TableCell>
                  <TableCell>From</TableCell>
                  <TableCell>To</TableCell>
                  <TableCell>Value</TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {tokenData.transfers.map((tx, i) => (
                  <TableRow key={i} hover>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>
                      {formatHash(tx.hash)}
                    </TableCell>
                    <TableCell>{tx.block}</TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>
                      {formatHash(tx.from)}
                    </TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>
                      {formatHash(tx.to)}
                    </TableCell>
                    <TableCell>{tx.value} {tokenData.symbol}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        )}
        
        {/* Info Tab */}
        {tab === 2 && (
          <Grid container spacing={3}>
            <Grid item xs={12} md={6}>
              <Card>
                <CardContent>
                  <Typography variant="h6" gutterBottom>Contract Details</Typography>
                  <List>
                    <ListItem>
                      <ListItemText primary="Address" secondary={tokenData.address} secondaryTypographyProps={{ sx: { fontFamily: 'monospace' }}} />
                    </ListItem>
                    <ListItem>
                      <ListItemText primary="Decimals" secondary={tokenData.decimals.toString()} />
                    </ListItem>
                    <ListItem>
                      <ListItemText primary="Total Supply" secondary={tokenData.totalSupply} />
                    </ListItem>
                    <ListItem>
                      <ListItemText primary="Circulating Supply" secondary={tokenData.circulatingSupply} />
                    </ListItem>
                  </List>
                </CardContent>
              </Card>
            </Grid>
            <Grid item xs={12} md={6}>
              <Card>
                <CardContent>
                  <Typography variant="h6" gutterBottom>Links</Typography>
                  <List>
                    {tokenData.website && (
                      <ListItem>
                        <ListItemText primary="Website" secondary={tokenData.website} />
                      </ListItem>
                    )}
                    {tokenData.twitter && (
                      <ListItem>
                        <ListItemText primary="Twitter" secondary={tokenData.twitter} />
                      </ListItem>
                    )}
                  </List>
                </CardContent>
              </Card>
            </Grid>
          </Grid>
        )}
      </Container>
    </>
  );
}