/**
 * TigerScan - Block Details Page
 * 
 * Advanced implementation with:
 * - Block details
 * - Transactions listing
 * - Uncle blocks
 * - Gas analysis
 * - Miner information
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
  CircularProgress,
  Alert,
  Avatar,
  Paper,
  Divider,
  List,
  ListItem,
  ListItemText,
  ListItemIcon,
  Skeleton
} from '@mui/material';
import { 
  AccountBalanceWallet, 
  ArrowUpward, 
  ArrowDownward, 
  LocalGasStation,
  Schedule,
  CheckCircle,
  Error as ErrorIcon,
  Warning,
  TransferWithinAStation
} from '@mui/icons-material';

const API_CONFIG = {
  baseUrl: process.env.NEXT_PUBLIC_API_URL || '/api',
  chainId: 6666
};

interface BlockData {
  number: number;
  hash: string;
  parentHash: string;
  sha3Uncles: string;
  miner: string;
  difficulty: string;
  totalDifficulty: string;
  gasUsed: number;
  gasLimit: number;
  gasFee: string;
  timestamp: number;
  transactions: Transaction[];
  uncles: string[];
  blockReward: string;
  uncleReward: string;
  nonce: string;
  extraData: string;
  logsBloom: string;
  receiptsRoot: string;
  transactionsRoot: string;
  stateRoot: string;
  minerName?: string;
}

interface Transaction {
  hash: string;
  block: number;
  timestamp: number;
  from: string;
  to: string;
  value: string;
  gasUsed: number;
  gasPrice: number;
  gasFee: string;
  status: 'success' | 'failed' | 'pending';
  method?: string;
}

class ApiClient {
  private baseUrl: string;
  
  constructor(baseUrl: string = API_CONFIG.baseUrl) {
    this.baseUrl = baseUrl;
  }
  
  async get<T>(endpoint: string): Promise<T> {
    const response = await fetch(`${this.baseUrl}${endpoint}`);
    if (!response.ok) {
      throw new Error(`API Error: ${response.status}`);
    }
    return response.json();
  }
}

export default function BlockPage() {
  const router = useRouter();
  const { block } = router.query;
  const [blockData, setBlockData] = useState<BlockData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  
  const apiClient = new ApiClient();
  
  const fetchBlockData = useCallback(async () => {
    if (!block) return;
    
    setLoading(true);
    setError(null);
    
    try {
      const blockNum = typeof block === 'string' ? parseInt(block) : parseInt(block?.toString() || '0');
      if (isNaN(blockNum)) {
        throw new Error('Invalid block number');
      }
      
      const data = await apiClient.get<BlockData>(`/block/${blockNum}`);
      setBlockData(data);
    } catch (err: any) {
      setError(err.message || 'Failed to fetch block data');
    } finally {
      setLoading(false);
    }
  }, [block, apiClient]);
  
  useEffect(() => {
    fetchBlockData();
  }, [fetchBlockData]);
  
  const formatBalance = (balance: string | number): string => {
    const num = typeof balance === 'string' ? parseFloat(balance) : balance;
    if (num >= 1e9) return `${(num / 1e9).toFixed(2)}B`;
    if (num >= 1e6) return `${(num / 1e6).toFixed(2)}M`;
    if (num >= 1e3) return `${(num / 1e3).toFixed(2)}K`;
    return num.toFixed(6);
  };
  
  const formatTimestamp = (timestamp: number): string => {
    return new Date(timestamp * 1000).toLocaleString();
  };
  
  const formatHash = (hash: string): string => {
    if (!hash) return '-';
    return `${hash.substring(0, 10)}...${hash.substring(hash.length - 8)}`;
  };
  
  const getGasPercentage = (used: number, limit: number): number => {
    return Math.round((used / limit) * 100);
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
        <Button variant="contained" onClick={fetchBlockData}>Retry</Button>
      </Container>
    );
  }
  
  if (!blockData) {
    return (
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Alert severity="warning">Block not found</Alert>
      </Container>
    );
  }
  
  return (
    <>
      <Head>
        <title>Block #{blockData.number} | TigerScan</title>
      </Head>
      
      <Container maxWidth="lg" sx={{ py: 4 }}>
        {/* Header */}
        <Paper sx={{ p: 3, mb: 3 }}>
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 2, mb: 2 }}>
            <Avatar sx={{ width: 64, height: 64, bgcolor: 'primary.main' }}>
              <AccountBalanceWallet sx={{ fontSize: 32 }} />
            </Avatar>
            <Box>
              <Typography variant="h4" component="h1">
                Block #{blockData.number}
              </Typography>
              <Typography variant="body2" color="text.secondary">
                {formatTimestamp(blockData.timestamp)}
              </Typography>
            </Box>
          </Box>
          
          <Grid container spacing={2}>
            <Grid item xs={6} md={3}>
              <Card variant="outlined">
                <CardContent sx={{ textAlign: 'center' }}>
                  <Typography variant="h5">{formatBalance(blockData.gasUsed)}</Typography>
                  <Typography variant="body2" color="text.secondary">
                    Gas Used ({getGasPercentage(blockData.gasUsed, blockData.gasLimit)}%)
                  </Typography>
                </CardContent>
              </Card>
            </Grid>
            <Grid item xs={6} md={3}>
              <Card variant="outlined">
                <CardContent sx={{ textAlign: 'center' }}>
                  <Typography variant="h5">{blockData.transactions.length}</Typography>
                  <Typography variant="body2" color="text.secondary">Transactions</Typography>
                </CardContent>
              </Card>
            </Grid>
            <Grid item xs={6} md={3}>
              <Card variant="outlined">
                <CardContent sx={{ textAlign: 'center' }}>
                  <Typography variant="h5">{blockData.uncles?.length || 0}</Typography>
                  <Typography variant="body2" color="text.secondary">Uncle Blocks</Typography>
                </CardContent>
              </Card>
            </Grid>
            <Grid item xs={6} md={3}>
              <Card variant="outlined">
                <CardContent sx={{ textAlign: 'center' }}>
                  <Typography variant="h5">{formatBalance(blockData.blockReward)}</Typography>
                  <Typography variant="body2" color="text.secondary">Block Reward</Typography>
                </CardContent>
              </Card>
            </Grid>
          </Grid>
        </Paper>
        
        {/* Block Details */}
        <Grid container spacing={3}>
          <Grid item xs={12} md={6}>
            <Card>
              <CardContent>
                <Typography variant="h6" gutterBottom>Block Hashes</Typography>
                <List>
                  <ListItem>
                    <ListItemText primary="Hash" secondary={blockData.hash} secondaryTypographyProps={{ sx: { fontFamily: 'monospace', fontSize: '0.75rem' } }} />
                  </ListItem>
                  <ListItem>
                    <ListItemText primary="Parent Hash" secondary={blockData.parentHash} secondaryTypographyProps={{ sx: { fontFamily: 'monospace', fontSize: '0.75rem' } }} />
                  </ListItem>
                  <ListItem>
                    <ListItemText primary="Nonce" secondary={blockData.nonce} secondaryTypographyProps={{ sx: { fontFamily: 'monospace' } }} />
                  </ListItem>
                  <ListItem>
                    <ListItemText primary="Transactions Root" secondary={blockData.transactionsRoot} secondaryTypographyProps={{ sx: { fontFamily: 'monospace', fontSize: '0.75rem' } }} />
                  </ListItem>
                  <ListItem>
                    <ListItemText primary="State Root" secondary={blockData.stateRoot} secondaryTypographyProps={{ sx: { fontFamily: 'monospace', fontSize: '0.75rem' } }} />
                  </ListItem>
                </List>
              </CardContent>
            </Card>
          </Grid>
          
          <Grid item xs={12} md={6}>
            <Card>
              <CardContent>
                <Typography variant="h6" gutterBottom>Miner & Rewards</Typography>
                <List>
                  <ListItem>
                    <ListItemIcon><AccountBalanceWallet /></ListItemIcon>
                    <ListItemText primary="Miner" secondary={blockData.minerName || blockData.miner} secondaryTypographyProps={{ sx: { fontFamily: 'monospace' } }} />
                  </ListItem>
                  <ListItem>
                    <ListItemIcon><LocalGasStation /></ListItemIcon>
                    <ListItemText primary="Gas Limit" secondary={blockData.gasLimit.toLocaleString()} />
                  </ListItem>
                  <ListItem>
                    <ListItemIcon><Schedule /></ListItemIcon>
                    <ListItemText primary="Difficulty" secondary={blockData.difficulty} />
                  </ListItem>
                  <ListItem>
                    <ListItemIcon><CheckCircle /></ListItemIcon>
                    <ListItemText primary="Block Reward" secondary={`${formatBalance(blockData.blockReward)} TGR`} />
                  </ListItem>
                  {blockData.uncleReward && (
                    <ListItem>
                      <ListItemIcon><Warning /></ListItemIcon>
                      <ListItemText primary="Uncle Reward" secondary={`${formatBalance(blockData.uncleReward)} TGR`} />
                    </ListItem>
                  )}
                </List>
              </CardContent>
            </Card>
          </Grid>
        </Grid>
        
        {/* Transactions */}
        <Card sx={{ mt: 3 }}>
          <CardContent>
            <Typography variant="h6" gutterBottom>
              <TransferWithinAStation sx={{ mr: 1, verticalAlign: 'middle' }} />
              Transactions ({blockData.transactions.length})
            </Typography>
            <TableContainer>
              <Table>
                <TableHead>
                  <TableRow>
                    <TableCell>Hash</TableCell>
                    <TableCell>From</TableCell>
                    <TableCell></TableCell>
                    <TableCell>To</TableCell>
                    <TableCell>Value</TableCell>
                    <TableCell>Gas</TableCell>
                    <TableCell>Status</TableCell>
                  </TableRow>
                </TableHead>
                <TableBody>
                  {blockData.transactions.map((tx) => (
                    <TableRow 
                      key={tx.hash} 
                      hover 
                      sx={{ cursor: 'pointer' }}
                      onClick={() => router.push(`/transaction/${tx.hash}`)}
                    >
                      <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>
                        {formatHash(tx.hash)}
                      </TableCell>
                      <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>
                        {formatHash(tx.from)}
                      </TableCell>
                      <TableCell align="center">
                        <ArrowUpward color="error" />
                      </TableCell>
                      <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>
                        {tx.to ? formatHash(tx.to) : '-'}
                      </TableCell>
                      <TableCell>{formatBalance(tx.value)} TGR</TableCell>
                      <TableCell>{tx.gasUsed.toLocaleString()}</TableCell>
                      <TableCell>
                        <Chip 
                          label={tx.status} 
                          color={tx.status === 'success' ? 'success' : 'error'}
                          size="small"
                        />
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </TableContainer>
          </CardContent>
        </Card>
      </Container>
    </>
  );
}