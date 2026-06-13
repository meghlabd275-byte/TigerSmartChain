/**
 * TigerScan - Transaction Details Page
 * 
 * Advanced implementation with:
 * - Full transaction details
 * - Internal transactions
 * - State changes
 * - Event logs
 * - Raw data
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
  Paper,
  Divider,
  List,
  ListItem,
  ListItemText,
  ListItemIcon,
  Skeleton,
  Accordion,
  AccordionSummary,
  AccordionDetails
} from '@mui/material';
import { 
  ArrowUpward, 
  ArrowDownward, 
  ExpandMore,
  CheckCircle,
  Error as ErrorIcon,
  Code,
  Receipt,
  AccountBalanceWallet
} from '@mui/icons-material';

const API_CONFIG = {
  baseUrl: process.env.NEXT_PUBLIC_API_URL || '/api',
  chainId: 6666
};

interface TransactionData {
  hash: string;
  block: number;
  blockHash: string;
  timestamp: number;
  from: string;
  fromName?: string;
  to: string;
  toName?: string;
  value: string;
  gasPrice: number;
  gasUsed: number;
  gasLimit: number;
  gasFee: string;
  nonce: number;
  transactionIndex: number;
  input: string;
  status: 'success' | 'failed' | 'pending';
  logs: Log[];
  internalTransactions: InternalTransaction[];
  stateChanges: StateChange[];
  tokenTransfers: TokenTransfer[];
}

interface Log {
  address: string;
  topics: string[];
  data: string;
  logIndex: number;
}

interface InternalTransaction {
  from: string;
  to: string;
  value: string;
  gas: number;
  input: string;
}

interface StateChange {
  key: string;
  before: string;
  after: string;
}

interface TokenTransfer {
  from: string;
  to: string;
  token: string;
  value: string;
  tokenId?: string;
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

export default function TransactionPage() {
  const router = useRouter();
  const { tx } = router.query;
  const [txData, setTxData] = useState<TransactionData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  
  const apiClient = new ApiClient();
  
  const fetchTxData = useCallback(async () => {
    if (!tx) return;
    
    setLoading(true);
    setError(null);
    
    try {
      const txHash = tx.toString();
      if (!/^0x[a-fA-F0-9]{64}$/.test(txHash)) {
        throw new Error('Invalid transaction hash');
      }
      
      const data = await apiClient.get<TransactionData>(`/transaction/${txHash}`);
      setTxData(data);
    } catch (err: any) {
      setError(err.message || 'Failed to fetch transaction data');
    } finally {
      setLoading(false);
    }
  }, [tx, apiClient]);
  
  useEffect(() => {
    fetchTxData();
  }, [fetchTxData]);
  
  const formatBalance = (balance: string | number): string => {
    const num = typeof balance === 'string' ? parseFloat(balance) : balance;
    if (num >= 1e9) return `${(num / 1e9).toFixed(6)}B`;
    if (num >= 1e6) return `${(num / 1e6).toFixed(6)}M`;
    if (num >= 1e3) return `${(num / 1e3).toFixed(6)}K`;
    return num.toFixed(6);
  };
  
  const formatTimestamp = (timestamp: number): string => {
    return new Date(timestamp * 1000).toLocaleString();
  };
  
  const formatHash = (hash: string): string => {
    if (!hash) return '-';
    return `${hash.substring(0, 14)}...${hash.substring(hash.length - 10)}`;
  };
  
  const decodeInput = (input: string): { method: string; params: string } | null => {
    if (!input || input.length < 10) return null;
    const methodId = input.substring(0, 10);
    // Common method signatures (simplified)
    const methods: Record<string, string> = {
      '0xa9059cbb': 'transfer(address,uint256)',
      '0x23b872dd': 'transferFrom(address,address,uint256)',
      '0x095ea7b3': 'approve(address,uint256)',
      '0x40c10f19': 'mint(address,uint256)',
      '0x2e1a7d4d': 'transfer(address,uint256)',
      '0x': 'undefined',
    };
    return {
      method: methods[methodId] || `unknown(${methodId})`,
      params: input.substring(10)
    };
  };
  
  if (loading) {
    return (
      <Box sx={{ p: 4 }}>
        <Skeleton variant="rectangular" height={200} sx={{ mb: 2 }} />
        <Skeleton variant="rectangular" height={400 }} />
      </Box>
    );
  }
  
  if (error) {
    return (
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Alert severity="error" sx={{ mb: 2 }}>{error}</Alert>
        <Button variant="contained" onClick={fetchTxData}>Retry</Button>
      </Container>
    );
  }
  
  if (!txData) {
    return (
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Alert severity="warning">Transaction not found</Alert>
      </Container>
    );
  }
  
  const decoded = decodeInput(txData.input);
  
  return (
    <>
      <Head>
        <title>Transaction {txData.hash.substring(0, 10)}... | TigerScan</title>
      </Head>
      
      <Container maxWidth="lg" sx={{ py: 4 }}>
        {/* Header */}
        <Paper sx={{ p: 3, mb: 3 }}>
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
            <Box>
              <Typography variant="h4" component="h1" gutterBottom>
                Transaction Details
              </Typography>
              <Typography variant="body2" sx={{ fontFamily: 'monospace', wordBreak: 'break-all' }}>
                {txData.hash}
              </Typography>
            </Box>
            <Box sx={{ ml: 'auto' }}>
              <Chip 
                icon={txData.status === 'success' ? <CheckCircle /> : <ErrorIcon />}
                label={txData.status.toUpperCase()}
                color={txData.status === 'success' ? 'success' : 'error'}
                size="large"
              />
            </Box>
          </Box>
        </Paper>
        
        {/* Overview */}
        <Grid container spacing={3} sx={{ mb: 3 }}>
          <Grid item xs={12} md={6}>
            <Card>
              <CardContent>
                <Typography variant="h6" gutterBottom>Overview</Typography>
                <List>
                  <ListItem>
                    <ListItemIcon><AccountBalanceWallet /></ListItemIcon>
                    <ListItemText primary="Block" secondary={`#${txData.block}`} />
                  </ListItem>
                  <ListItem>
                    <ListItemIcon><Receipt /></ListItemIcon>
                    <ListItemText primary="Timestamp" secondary={formatTimestamp(txData.timestamp)} />
                  </ListItem>
                  <ListItem>
                    <ListItemIcon><ArrowUpward /></ListItemIcon>
                    <ListItemText 
                      primary="From" 
                      secondary={txData.fromName || txData.from}
                      secondaryTypographyProps={{ sx: { fontFamily: 'monospace' }}
                    />
                  </ListItem>
                  <ListItem>
                    <ListItemIcon><ArrowDownward /></ListItemIcon>
                    <ListItemText 
                      primary="To" 
                      secondary={txData.toName || txData.to}
                      secondaryTypographyProps={{ sx: { fontFamily: 'monospace' }}
                    />
                  </ListItem>
                </List>
              </CardContent>
            </Card>
          </Grid>
          
          <Grid item xs={12} md={6}>
            <Card>
              <CardContent>
                <Typography variant="h6" gutterBottom>Value & Fees</Typography>
                <List>
                  <ListItem>
                    <ListItemText 
                      primary="Value" 
                      secondary={`${formatBalance(txData.value)} TGR`} 
                    />
                  </ListItem>
                  <ListItem>
                    <ListItemText 
                      primary="Gas Fee" 
                      secondary={`${formatBalance(txData.gasFee)} TGR`} 
                    />
                  </ListItem>
                  <ListItem>
                    <ListItemText 
                      primary="Gas Price" 
                      secondary={`${txData.gasPrice} Gwei`} 
                    />
                  </ListItem>
                  <ListItem>
                    <ListItemText 
                      primary="Gas Used / Limit" 
                      secondary={`${txData.gasUsed.toLocaleString()} / ${txData.gasLimit.toLocaleString()}`} 
                    />
                  </ListItem>
                </List>
              </CardContent>
            </Card>
          </Grid>
        </Grid>
        
        {/* Input Data */}
        {decoded && (
          <Card sx={{ mb: 3 }}>
            <CardContent>
              <Typography variant="h6" gutterBottom>
                <Code sx={{ mr: 1, verticalAlign: 'middle' }} />
                Input Data
              </Typography>
              <Divider sx={{ my: 2 }} />
              <Typography variant="subtitle2" color="primary" gutterBottom>
                Method: {decoded.method}
              </Typography>
              <Paper variant="outlined" sx={{ p: 2, mt: 2 }}>
                <Typography 
                  variant="body2" 
                  sx={{ 
                    fontFamily: 'monospace', 
                    wordBreak: 'break-all',
                    fontSize: '0.75rem'
                  }}
                >
                  {txData.input}
                </Typography>
              </Paper>
            </CardContent>
          </Card>
        )}
        
        {/* Internal Transactions */}
        {txData.internalTransactions && txData.internalTransactions.length > 0 && (
          <Accordion sx={{ mb: 2 }}>
            <AccordionSummary expandIcon={<ExpandMore />}>
              <Typography>
                Internal Transactions ({txData.internalTransactions.length})
              </Typography>
            </AccordionSummary>
            <AccordionDetails>
              <TableContainer>
                <Table size="small">
                  <TableHead>
                    <TableRow>
                      <TableCell>From</TableCell>
                      <TableCell>To</TableCell>
                      <TableCell>Value</TableCell>
                    </TableRow>
                  </TableHead>
                  <TableBody>
                    {txData.internalTransactions.map((tx, i) => (
                      <TableRow key={i}>
                        <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>
                          {formatHash(tx.from)}
                        </TableCell>
                        <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>
                          {formatHash(tx.to)}
                        </TableCell>
                        <TableCell>{formatBalance(tx.value)} TGR</TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </TableContainer>
            </AccordionDetails>
          </Accordion>
        )}
        
        {/* Event Logs */}
        {txData.logs && txData.logs.length > 0 && (
          <Accordion sx={{ mb: 2 }}>
            <AccordionSummary expandIcon={<ExpandMore />}>
              <Typography>
                Event Logs ({txData.logs.length})
              </Typography>
            </AccordionSummary>
            <AccordionDetails>
              {txData.logs.map((log, i) => (
                <Box key={i} sx={{ mb: 2, p: 2, bgcolor: 'grey.50', borderRadius: 1 }}>
                  <Typography variant="subtitle2">Log #{log.logIndex}</Typography>
                  <Typography variant="body2" sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>
                    Address: {formatHash(log.address)}
                  </Typography>
                  <Typography variant="body2" sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>
                    Topics: {log.topics.map(t => formatHash(t)).join(', ')}
                  </Typography>
                </Box>
              ))}
            </AccordionDetails>
          </Accordion>
        )}
        
        {/* Token Transfers */}
        {txData.tokenTransfers && txData.tokenTransfers.length > 0 && (
          <Accordion>
            <AccordionSummary expandIcon={<ExpandMore />}>
              <Typography>
                Token Transfers ({txData.tokenTransfers.length})
              </Typography>
            </AccordionSummary>
            <AccordionDetails>
              <TableContainer>
                <Table size="small">
                  <TableHead>
                    <TableRow>
                      <TableCell>Token</TableCell>
                      <TableCell>From</TableCell>
                      <TableCell>To</TableCell>
                      <TableCell>Value</TableCell>
                    </TableRow>
                  </TableHead>
                  <TableBody>
                    {txData.tokenTransfers.map((transfer, i) => (
                      <TableRow key={i}>
                        <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>
                          {formatHash(transfer.token)}
                        </TableCell>
                        <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>
                          {formatHash(transfer.from)}
                        </TableCell>
                        <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>
                          {formatHash(transfer.to)}
                        </TableCell>
                        <TableCell>{transfer.value}</TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </TableContainer>
            </AccordionDetails>
          </Accordion>
        )}
      </Container>
    </>
  );
}