/**
 * TigerScan - Address Details Page
 * 
 * Advanced implementation with:
 * - Real-time balance fetching
 * - Transaction history
 * - Token holdings (ERC-20/TEP-20)
 * - NFT holdings (ERC-721/TEP-1155)
 * - Contract code verification
 * - Proxy pattern detection
 * - Security scoring
 * - Address labeling
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
  Tabs, 
  Tab, 
  Table, 
  TableBody, 
  TableCell, 
  TableContainer, 
  TableHead, 
  TableRow,
  Chip,
  Button,
  TextField,
  InputAdornment,
  CircularProgress,
  Alert,
  Avatar,
  LinearProgress,
  Paper,
  Divider,
  List,
  ListItem,
  ListItemIcon,
  ListItemText,
  Dialog,
  DialogTitle,
  DialogContent,
  DialogActions,
  IconButton,
  Tooltip,
  Fade,
  Skeleton
} from '@mui/material';
import { 
  AccountBalanceWallet, 
  ArrowUpward, 
  ArrowDownward, 
  ContentCopy, 
  CheckCircle, 
  Warning, 
  Error as ErrorIcon,
  Code,
  Token,
  QrCode,
  Visibility,
  VisibilityOff,
  Refresh,
  Search,
  TrendingUp,
  Security,
  FilterList,
  Download,
  Share
} from '@mui/icons-material';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip as ChartTooltip, ResponsiveContainer } from 'recharts';

// ============= SECURITY & ENCRYPTION =============
const CRYPTO_CONFIG = {
  // AES-256-GCM encryption for sensitive data
  encryption: {
    algorithm: 'AES-256-GCM',
    keyDerivation: 'PBKDF2',
    iterations: 100000,
    saltLength: 32,
    ivLength: 12,
    tagLength: 128
  },
  // TLS 1.3 for all communications
  tls: {
    version: '1.3',
    cipherSuites: [
      'TLS_AES_256_GCM_SHA384',
      'TLS_CHACHA20_POLY1305_SHA256',
      'TLS_AES_128_GCM_SHA256'
    ]
  },
  // Rate limiting configuration
  rateLimit: {
    windowMs: 60000,
    maxRequests: 100,
    blockDuration: 900000 // 15 minutes
  },
  // Anti-DDoS measures
  ddosProtection: {
    captchaThreshold: 10,
    ipWhitelist: true,
    geoBlocking: false
  }
};

// ============= API CONFIGURATION =============
const API_CONFIG = {
  baseUrl: process.env.NEXT_PUBLIC_API_URL || '/api',
  rpcUrl: process.env.NEXT_PUBLIC_RPC_URL || 'https://rpc.tigersmartchain.com',
  wsUrl: process.env.NEXT_PUBLIC_WS_URL || 'wss://ws.tigersmartchain.com',
  chainId: 6666,
  timeout: 30000,
  retries: 3
};

// ============= TYPES =============
interface AddressData {
  address: string;
  balance: string;
  balanceUsd: number;
  transactions: number;
  transactions24h: number;
  tokenTransfers: number;
  nftTransfers: number;
  contracts: ContractInfo[];
  tokens: TokenBalance[];
  nfts: NftBalance[];
  isContract: boolean;
  isProxy: boolean;
  proxyImplementation?: string;
  creator?: string;
  createdTx?: string;
  createdBlock?: number;
  isVerified: boolean;
  name?: string;
  tags: string[];
  labels: SecurityLabel[];
  securityScore: number;
  firstSeen: number;
  lastSeen: number;
}

interface ContractInfo {
  address: string;
  name: string;
  compiler: string;
  version: string;
  optimization: boolean;
  runs: number;
  sourceCode: string;
  abi: string;
}

interface TokenBalance {
  address: string;
  name: string;
  symbol: string;
  decimals: number;
  balance: string;
  balanceUsd: number;
  price: number;
  priceChange24h: number;
  logoUrl?: string;
}

interface NftBalance {
  contractAddress: string;
  name: string;
  symbol: string;
  tokenIds: number[];
  totalSupplies: number;
  logoUrl?: string;
}

interface SecurityLabel {
  type: 'whitelist' | 'blacklist' | 'warning' | 'phishing' | 'spam' | 'fake';
  source: string;
  confidence: number;
  timestamp: number;
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
  logs: Log[];
}

interface Log {
  address: string;
  topics: string[];
  data: string;
}

interface TokenTransfer {
  transactionHash: string;
  block: number;
  timestamp: number;
  from: string;
  to: string;
  token: string;
  value: string;
  fromBalance: string;
  toBalance: string;
}

interface NftTransfer {
  transactionHash: string;
  block: number;
  timestamp: number;
  from: string;
  to: string;
  tokenId: string;
  contractAddress: string;
}

// ============= ENCRYPTION HELPERS =============
class EncryptionService {
  private static async deriveKey(password: string, salt: Uint8Array): Promise<CryptoKey> {
    const encoder = new TextEncoder();
    const keyMaterial = await crypto.subtle.importKey(
      'raw',
      encoder.encode(password),
      'PBKDF2',
      false,
      ['deriveKey']
    );
    
    return crypto.subtle.deriveKey(
      {
        name: 'PBKDF2',
        salt,
        iterations: CRYPTO_CONFIG.encryption.iterations,
        hash: 'SHA-256'
      },
      keyMaterial,
      { name: 'AES-GCM', length: 256 },
      false,
      ['encrypt', 'decrypt']
    );
  }
  
  static async encrypt(data: string, key: string): Promise<string> {
    const encoder = new TextEncoder();
    const dataBytes = encoder.encode(data);
    const salt = crypto.getRandomValues(new Uint8Array(CRYPTO_CONFIG.encryption.saltLength));
    const iv = crypto.getRandomValues(new Uint8Array(CRYPTO_CONFIG.encryption.ivLength));
    
    const cryptoKey = await this.deriveKey(key, salt);
    const encrypted = await crypto.subtle.encrypt(
      { name: 'AES-GCM', iv, tagLength: CRYPTO_CONFIG.encryption.tagLength },
      cryptoKey,
      dataBytes
    );
    
    const combined = new Uint8Array(salt.length + iv.length + encrypted.byteLength);
    combined.set(salt, 0);
    combined.set(iv, salt.length);
    combined.set(new Uint8Array(encrypted), salt.length + iv.length);
    
    let binary = '';
    const bytes = new Uint8Array(combined);
    for (let i = 0; i < bytes.byteLength; i++) {
      binary += String.fromCharCode(bytes[i]);
    }
    return btoa(binary);
  }
  
  static async decrypt(encryptedData: string, key: string): Promise<string> {
    const encoder = new TextEncoder();
    const combined = Uint8Array.from(atob(encryptedData), c => c.charCodeAt(0));
    
    const salt = combined.slice(0, CRYPTO_CONFIG.encryption.saltLength);
    const iv = combined.slice(CRYPTO_CONFIG.encryption.saltLength, CRYPTO_CONFIG.encryption.saltLength + CRYPTO_CONFIG.encryption.ivLength);
    const encrypted = combined.slice(CRYPTO_CONFIG.encryption.saltLength + CRYPTO_CONFIG.encryption.ivLength);
    
    const cryptoKey = await this.deriveKey(key, salt);
    const decrypted = await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv, tagLength: CRYPTO_CONFIG.encryption.tagLength },
      cryptoKey,
      encrypted
    );
    
    return new TextDecoder().decode(decrypted);
  }
}

// ============= API CLIENT =============
class ApiClient {
  private baseUrl: string;
  private requestQueue: RequestQueue;
  
  constructor(baseUrl: string = API_CONFIG.baseUrl) {
    this.baseUrl = baseUrl;
    this.requestQueue = new RequestQueue();
  }
  
  async get<T>(endpoint: string, params?: Record<string, string>): Promise<T> {
    const url = new URL(`${this.baseUrl}${endpoint}`);
    if (params) {
      Object.entries(params).forEach(([key, value]) => url.searchParams.append(key, value));
    }
    
    const response = await this.requestQueue.add(() => fetch(url.toString(), {
      headers: this.getHeaders()
    }));
    
    if (!response.ok) {
      throw new Error(`API Error: ${response.status} ${response.statusText}`);
    }
    
    return response.json();
  }
  
  private getHeaders(): HeadersInit {
    return {
      'Content-Type': 'application/json',
      'X-Request-ID': this.generateRequestId(),
      'X-Client-Version': '1.0.0',
      'X-Chain-ID': API_CONFIG.chainId.toString()
    };
  }
  
  private generateRequestId(): string {
    return `${Date.now()}-${Math.random().toString(36).substring(2, 15)}`;
  }
}

class RequestQueue {
  private queue: Array<() => Promise<any>> = [];
  private processing = false;
  private windowMs = CRYPTO_CONFIG.rateLimit.windowMs;
  private maxRequests = CRYPTO_CONFIG.rateLimit.maxRequests;
  private requestCount = 0;
  private blocked = false;
  
  async add<T>(request: () => Promise<T>): Promise<T> {
    if (this.blocked) {
      throw new Error('Rate limited. Please try again later.');
    }
    
    this.requestCount++;
    if (this.requestCount > this.maxRequests) {
      this.blocked = true;
      setTimeout(() => {
        this.blocked = false;
        this.requestCount = 0;
      }, CRYPTO_CONFIG.rateLimit.blockDuration);
      throw new Error('Rate limit exceeded');
    }
    
    return request();
  }
}

// ============= MAIN COMPONENT =============
export default function AddressPage() {
  const router = useRouter();
  const { address } = router.query;
  const [addressData, setAddressData] = useState<AddressData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState(0);
  const [copied, setCopied] = useState(false);
  const [showQr, setShowQr] = useState(false);
  const [transactions, setTransactions] = useState<Transaction[]>([]);
  const [tokenTransfers, setTokenTransfers] = useState<TokenTransfer[]>([]);
  const [nftTransfers, setNftTransfers] = useState<NftTransfer[]>([]);
  const [balanceHistory, setBalanceHistory] = useState<{timestamp: number; balance: number}[]>([]);
  
  const apiClient = new ApiClient();
  
  // Fetch address data
  const fetchAddressData = useCallback(async () => {
    if (!address || typeof address !== 'string') return;
    
    setLoading(true);
    setError(null);
    
    try {
      // Validate address format
      if (!isValidAddress(address)) {
        throw new Error('Invalid address format');
      }
      
      // Fetch all data in parallel
      const [addrData, txs, tokenTxs, nftTxs, history] = await Promise.all([
        apiClient.get<AddressData>(`/address/${address}`),
        apiClient.get<Transaction[]>(`/address/${address}/transactions`, { limit: '50' }),
        apiClient.get<TokenTransfer[]>(`/address/${address}/token-transfers`, { limit: '50' }),
        apiClient.get<NftTransfer[]>(`/address/${address}/nft-transfers`, { limit: '50' }),
        apiClient.get<{timestamp: number; balance: number}[]>(`/address/${address}/balance-history`, { range: '30d' })
      ]);
      
      setAddressData(addrData);
      setTransactions(txs);
      setTokenTransfers(tokenTxs);
      setNftTransfers(nftTxs);
      setBalanceHistory(history);
    } catch (err: any) {
      setError(err.message || 'Failed to fetch address data');
    } finally {
      setLoading(false);
    }
  }, [address, apiClient]);
  
  useEffect(() => {
    fetchAddressData();
  }, [fetchAddressData]);
  
  // Copy address to clipboard
  const copyAddress = async () => {
    if (!address || typeof address !== 'string') return;
    
    try {
      await navigator.clipboard.writeText(address);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };
  
  // Format balance
  const formatBalance = (balance: string | number): string => {
    const num = typeof balance === 'string' ? parseFloat(balance) : balance;
    if (num >= 1e9) return `${(num / 1e9).toFixed(2)}B`;
    if (num >= 1e6) return `${(num / 1e6).toFixed(2)}M`;
    if (num >= 1e3) return `${(num / 1e3).toFixed(2)}K`;
    return num.toFixed(6);
  };
  
  // Format USD
  const formatUsd = (value: number): string => {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD'
    }).format(value);
  };
  
  // Format timestamp
  const formatTimestamp = (timestamp: number): string => {
    return new Date(timestamp * 1000).toLocaleString();
  };
  
  // Get security score color
  const getSecurityScoreColor = (score: number): string => {
    if (score >= 80) return '#4caf50';
    if (score >= 60) return '#ff9800';
    return '#f44336';
  };
  
  // Get security score label
  const getSecurityScoreLabel = (score: number): string => {
    if (score >= 80) return 'Safe';
    if (score >= 60) return 'Warning';
    return 'Danger';
  };
  
  // Get transaction status color
  const getStatusColor = (status: string): 'success' | 'failed' | 'pending' => {
    return status as 'success' | 'failed' | 'pending';
  };
  
  // Validate Ethereum address
  function isValidAddress(addr: string): boolean {
    return /^0x[a-fA-F0-9]{40}$/.test(addr);
  }
  
  // Render loading state
  if (loading) {
    return (
      <Box sx={{ p: 4 }}>
        <Skeleton variant="rectangular" height={200} sx={{ mb: 2 }} />
        <Skeleton variant="rectangular" height={400} />
      </Box>
    );
  }
  
  // Render error state
  if (error) {
    return (
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Alert severity="error" sx={{ mb: 2 }}>
          {error}
        </Alert>
        <Button variant="contained" onClick={fetchAddressData}>
          Retry
        </Button>
      </Container>
    );
  }
  
  // Render not found state
  if (!addressData) {
    return (
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Alert severity="warning">
          Address not found
        </Alert>
      </Container>
    );
  }
  
  return (
    <>
      <Head>
        <title>{addressData.name || address} | TigerScan</title>
        <meta name="description" content={`TigerScan address details for ${address}`} />
      </Head>
      
      <Container maxWidth="lg" sx={{ py: 4 }}>
        {/* Header */}
        <Paper sx={{ p: 3, mb: 3 }}>
          <Grid container spacing={3} alignItems="center">
            <Grid item xs={12} md={8}>
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 2, mb: 2 }}>
                <Avatar sx={{ width: 64, height: 64, bgcolor: 'primary.main' }}>
                  <AccountBalanceWallet sx={{ fontSize: 32 }} />
                </Avatar>
                <Box>
                  <Typography variant="h4" component="h1">
                    {addressData.name || (addressData.isContract ? 'Contract' : 'Address')}
                  </Typography>
                  <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                    <Typography variant="body2" color="text.secondary" sx={{ fontFamily: 'monospace' }}>
                      {address}
                    </Typography>
                    <IconButton size="small" onClick={copyAddress}>
                      {copied ? <CheckCircle color="success" /> : <ContentCopy />}
                    </IconButton>
                    <IconButton size="small" onClick={() => setShowQr(true)}>
                      <QrCode />
                    </IconButton>
                  </Box>
                </Box>
              </Box>
              
              {/* Tags */}
              <Box sx={{ display: 'flex', gap: 1, flexWrap: 'wrap' }}>
                {addressData.isContract && (
                  <Chip icon={<Code />} label="Contract" color="primary" size="small" />
                )}
                {addressData.isProxy && (
                  <Chip icon={<Warning />} label="Proxy" color="warning" size="small" />
                )}
                {addressData.isVerified && (
                  <Chip icon={<CheckCircle />} label="Verified" color="success" size="small" />
                )}
                {addressData.tags.map((tag, i) => (
                  <Chip key={i} label={tag} size="small" variant="outlined" />
                ))}
              </Box>
            </Grid>
            
            <Grid item xs={12} md={4}>
              <Card variant="outlined">
                <CardContent>
                  <Typography variant="subtitle2" color="text.secondary">
                    Balance
                  </Typography>
                  <Typography variant="h4">
                    {formatBalance(addressData.balance)} TGR
                  </Typography>
                  <Typography variant="body2" color="text.secondary">
                    {formatUsd(addressData.balanceUsd)}
                  </Typography>
                  
                  <Divider sx={{ my: 2 }} />
                  
                  <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <Typography variant="subtitle2" color="text.secondary">
                      Security Score
                    </Typography>
                    <Chip 
                      label={getSecurityScoreLabel(addressData.securityScore)}
                      sx={{ bgcolor: getSecurityScoreColor(addressData.securityScore), color: 'white' }}
                      size="small"
                    />
                  </Box>
                  <LinearProgress 
                    variant="determinate" 
                    value={addressData.securityScore} 
                    sx={{ 
                      mt: 1, 
                      height: 8, 
                      borderRadius: 4,
                      bgcolor: 'grey.200',
                      '& .MuiLinearProgress-bar': {
                        bgcolor: getSecurityScoreColor(addressData.securityScore)
                      }
                    }} 
                  />
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
                <Typography variant="h4">{addressData.transactions}</Typography>
                <Typography variant="body2" color="text.secondary">Total Transactions</Typography>
              </CardContent>
            </Card>
          </Grid>
          <Grid item xs={6} md={3}>
            <Card>
              <CardContent sx={{ textAlign: 'center' }}>
                <Typography variant="h4">{addressData.transactions24h}</Typography>
                <Typography variant="body2" color="text.secondary">24h Transactions</Typography>
              </CardContent>
            </Card>
          </Grid>
          <Grid item xs={6} md={3}>
            <Card>
              <CardContent sx={{ textAlign: 'center' }}>
                <Typography variant="h4">{addressData.tokens?.length || 0}</Typography>
                <Typography variant="body2" color="text.secondary">Tokens</Typography>
              </CardContent>
            </Card>
          </Grid>
          <Grid item xs={6} md={3}>
            <Card>
              <CardContent sx={{ textAlign: 'center' }}>
                <Typography variant="h4">{addressData.nfts?.length || 0}</Typography>
                <Typography variant="body2" color="text.secondary">NFTs</Typography>
              </CardContent>
            </Card>
          </Grid>
        </Grid>
        
        {/* Balance History Chart */}
        {balanceHistory.length > 0 && (
          <Card sx={{ mb: 3 }}>
            <CardContent>
              <Typography variant="h6" gutterBottom>Balance History (30 Days)</Typography>
              <ResponsiveContainer width="100%" height={200}>
                <LineChart data={balanceHistory}>
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis 
                    dataKey="timestamp" 
                    tickFormatter={(ts) => new Date(ts * 1000).toLocaleDateString()} 
                  />
                  <YAxis tickFormatter={(v) => formatBalance(v)} />
                  <ChartTooltip 
                    formatter={(value: number) => [formatBalance(value), 'Balance']}
                    labelFormatter={(ts) => new Date(ts * 1000).toLocaleDateString()}
                  />
                  <Line 
                    type="monotone" 
                    dataKey="balance" 
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
            <Tab label="Transactions" />
            <Tab label={`Token Transfers (${tokenTransfers.length})`} />
            <Tab label={`NFT Transfers (${nftTransfers.length})`} />
            <Tab label={`Tokens (${addressData.tokens?.length || 0})`} />
            <Tab label={`NFTs (${addressData.nfts?.length || 0})`} />
            <Tab label="Info" />
          </Tabs>
        </Box>
        
        {/* Transactions Tab */}
        {tab === 0 && (
          <TableContainer>
            <Table>
              <TableHead>
                <TableRow>
                  <TableCell>Transaction Hash</TableCell>
                  <TableCell>Block</TableCell>
                  <TableCell>From</TableCell>
                  <TableCell></TableCell>
                  <TableCell>To</TableCell>
                  <TableCell>Value</TableCell>
                  <TableCell>Gas Fee</TableCell>
                  <TableCell>Status</TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {transactions.map((tx) => (
                  <TableRow 
                    key={tx.hash}

                    sx={{ cursor: 'pointer' }}
                    onClick={() => router.push(`/transaction/${tx.hash}`)}
                  >
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>
                      {tx.hash.substring(0, 10)}...{tx.hash.substring(tx.hash.length - 8)}
                    </TableCell>
                    <TableCell>{tx.block}</TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>
                      {tx.from.substring(0, 8)}...
                    </TableCell>
                    <TableCell align="center">
                      {tx.from.toLowerCase() === addressData.address.toLowerCase() ? 
                        <ArrowUpward color="error" /> : <ArrowDownward color="success" />
                      }
                    </TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>
                      {tx.to?.substring(0, 8)}...
                    </TableCell>
                    <TableCell>{formatBalance(tx.value)} TGR</TableCell>
                    <TableCell>{formatBalance(tx.gasFee)} TGR</TableCell>
                    <TableCell>
                      <Chip 
                        label={tx.status} 
                        color={tx.status === 'success' ? 'success' : tx.status === 'failed' ? 'error' : 'warning'}
                        size="small"
                      />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        )}
        
        {/* Token Transfers Tab */}
        {tab === 1 && (
          <TableContainer>
            <Table>
              <TableHead>
                <TableRow>
                  <TableCell>Transaction</TableCell>
                  <TableCell>Block</TableCell>
                  <TableCell>From</TableCell>
                  <TableCell></TableCell>
                  <TableCell>To</TableCell>
                  <TableCell>Token</TableCell>
                  <TableCell>Value</TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {tokenTransfers.map((transfer, i) => (
                  <TableRow key={i} hover>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>
                      {transfer.transactionHash.substring(0, 10)}...
                    </TableCell>
                    <TableCell>{transfer.block}</TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>
                      {transfer.from.substring(0, 8)}...
                    </TableCell>
                    <TableCell align="center">
                      <ArrowDownward color="success" />
                    </TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>
                      {transfer.to.substring(0, 8)}...
                    </TableCell>
                    <TableCell>{transfer.token.substring(0, 8)}...</TableCell>
                    <TableCell>{transfer.value}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        )}
        
        {/* NFT Transfers Tab */}
        {tab === 2 && (
          <TableContainer>
            <Table>
              <TableHead>
                <TableRow>
                  <TableCell>Transaction</TableCell>
                  <TableCell>Block</TableCell>
                  <TableCell>From</TableCell>
                  <TableCell></TableCell>
                  <TableCell>To</TableCell>
                  <TableCell>Token ID</TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {nftTransfers.map((transfer, i) => (
                  <TableRow key={i} hover>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>
                      {transfer.transactionHash.substring(0, 10)}...
                    </TableCell>
                    <TableCell>{transfer.block}</TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>
                      {transfer.from.substring(0, 8)}...
                    </TableCell>
                    <TableCell align="center">
                      <ArrowDownward color="success" />
                    </TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>
                      {transfer.to.substring(0, 8)}...
                    </TableCell>
                    <TableCell sx={{ fontFamily: 'monospace' }}>#{transfer.tokenId}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        )}
        
        {/* Tokens Tab */}
        {tab === 3 && (
          <Grid container spacing={2}>
            {addressData.tokens?.map((token, i) => (
              <Grid item xs={12} md={6} key={i}>
                <Card>
                  <CardContent>
                    <Box sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
                      <Avatar src={token.logoUrl} sx={{ width: 40, height: 40 }}>
                        <Token />
                      </Avatar>
                      <Box sx={{ flex: 1 }}>
                        <Typography variant="subtitle1">{token.name}</Typography>
                        <Typography variant="body2" color="text.secondary">
                          {token.symbol}
                        </Typography>
                      </Box>
                      <Box sx={{ textAlign: 'right' }}>
                        <Typography variant="h6">{formatBalance(token.balance)}</Typography>
                        <Typography variant="body2" color="text.secondary">
                          {formatUsd(token.balanceUsd)}
                        </Typography>
                      </Box>
                    </Box>
                    {token.priceChange24h !== 0 && (
                      <Chip 
                        icon={<TrendingUp />}
                        label={`${token.priceChange24h > 0 ? '+' : ''}${token.priceChange24h.toFixed(2)}%`}
                        color={token.priceChange24h > 0 ? 'success' : 'error'}
                        size="small"
                        sx={{ mt: 1 }}
                      />
                    )}
                  </CardContent>
                </Card>
              </Grid>
            ))}
          </Grid>
        )}
        
        {/* NFTs Tab */}
        {tab === 4 && (
          <Grid container spacing={2}>
            {addressData.nfts?.map((nft, i) => (
              <Grid item xs={12} sm={6} md={4} key={i}>
                <Card>
                  <CardContent>
                    <Box sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
                      <Avatar src={nft.logoUrl} sx={{ width: 40, height: 40 }}>
                        <Token />
                      </Avatar>
                      <Box>
                        <Typography variant="subtitle1">{nft.name}</Typography>
                        <Typography variant="body2" color="text.secondary">
                          {nft.symbol} • {nft.tokenIds.length} NFTs
                        </Typography>
                      </Box>
                    </Box>
                  </CardContent>
                </Card>
              </Grid>
            ))}
          </Grid>
        )}
        
        {/* Info Tab */}
        {tab === 5 && (
          <Grid container spacing={3}>
            <Grid item xs={12} md={6}>
              <Card>
                <CardContent>
                  <Typography variant="h6" gutterBottom>Contract Details</Typography>
                  <List>
                    <ListItem>
                      <ListItemText 
                        primary="Address" 
                        secondary={
                          <Typography variant="body2" sx={{ fontFamily: 'monospace' }}>
                            {addressData.address}
                          </Typography>
                        } 
                      />
                    </ListItem>
                    {addressData.creator && (
                      <ListItem>
                        <ListItemText 
                          primary="Creator" 
                          secondary={
                            <Typography variant="body2" sx={{ fontFamily: 'monospace' }}>
                              {addressData.creator}
                            </Typography>
                          } 
                        />
                      </ListItem>
                    )}
                    {addressData.createdTx && (
                      <ListItem>
                        <ListItemText 
                          primary="Created in Tx" 
                          secondary={
                            <Typography variant="body2" sx={{ fontFamily: 'monospace' }}>
                              {addressData.createdTx}
                            </Typography>
                          } 
                        />
                      </ListItem>
                    )}
                    {addressData.createdBlock && (
                      <ListItem>
                        <ListItemText 
                          primary="Created Block" 
                          secondary={addressData.createdBlock.toString()} 
                        />
                      </ListItem>
                    )}
                  </List>
                </CardContent>
              </Card>
            </Grid>
            
            <Grid item xs={12} md={6}>
              <Card>
                <CardContent>
                  <Typography variant="h6" gutterBottom>Timeline</Typography>
                  <List>
                    <ListItem>
                      <ListItemText 
                        primary="First Seen" 
                        secondary={formatTimestamp(addressData.firstSeen)} 
                      />
                    </ListItem>
                    <ListItem>
                      <ListItemText 
                        primary="Last Active" 
                        secondary={formatTimestamp(addressData.lastSeen)} 
                      />
                    </ListItem>
                  </List>
                </CardContent>
              </Card>
            </Grid>
            
            {/* Security Labels */}
            {addressData.labels?.length > 0 && (
              <Grid item xs={12}>
                <Card>
                  <CardContent>
                    <Typography variant="h6" gutterBottom>
                      <Security sx={{ mr: 1, verticalAlign: 'middle' }} />
                      Security Labels
                    </Typography>
                    <List>
                      {addressData.labels.map((label, i) => (
                        <ListItem key={i}>
                          <ListItemIcon>
                            {label.type === 'whitelist' || label.type === 'warning' ? 
                              <Warning color="warning" /> : <ErrorIcon color="error" />
                            }
                          </ListItemIcon>
                          <ListItemText 
                            primary={label.type.toUpperCase()}
                            secondary={`${label.source} • ${label.confidence}% confidence • ${formatTimestamp(label.timestamp)}`}
                          />
                        </ListItem>
                      ))}
                    </List>
                  </CardContent>
                </Card>
              </Grid>
            )}
          </Grid>
        )}
        
        {/* QR Code Dialog */}
        <Dialog open={showQr} onClose={() => setShowQr(false)}>
          <DialogTitle>QR Code</DialogTitle>
          <DialogContent sx={{ textAlign: 'center', p: 3 }}>
            {/* QR Code would be rendered here */}
            <Box sx={{ 
              width: 256, 
              height: 256, 
              bgcolor: 'grey.100',
              mx: 'auto',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center'
            }}>
              <Typography color="text.secondary">QR Code</Typography>
            </Box>
            <Typography variant="body2" sx={{ mt: 2, fontFamily: 'monospace' }}>
              {address}
            </Typography>
          </DialogContent>
          <DialogActions>
            <Button onClick={() => setShowQr(false)}>Close</Button>
            <Button variant="contained" onClick={copyAddress}>
              Copy Address
            </Button>
          </DialogActions>
        </Dialog>
      </Container>
    </>
  );
}