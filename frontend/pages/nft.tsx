/**
 * TigerScan - NFT Details Page
 * 
 * NFT details with metadata, transfers, and owners
 */

import { useState, useEffect, useCallback } from 'react';
import { useRouter } from 'next/router';
import Head from 'next/head';
import { Box, Container, Grid, Card, CardContent, Typography, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Chip, Button, Alert, Avatar, Paper, Divider, List, ListItem, ListItemText, Skeleton, Tabs, Tab } from '@mui/material';
import { Image, SwapHoriz, AccountBalanceWallet } from '@mui/icons-material';

const API_CONFIG = { baseUrl: process.env.NEXT_PUBLIC_API_URL || '/api' };

interface NftData {
  contractAddress: string;
  tokenId: string;
  name: string;
  description: string;
  imageUrl: string;
  attributes: { trait_type: string; value: string }[];
  owner: string;
  transfers: { from: string; to: string; hash: string; timestamp: number }[];
  metadata: Record<string, string>;
}

class ApiClient {
  private baseUrl: string;
  constructor(baseUrl: string = API_CONFIG.baseUrl) { this.baseUrl = baseUrl; }
  async get<T>(endpoint: string): Promise<T> {
    const response = await fetch(`${this.baseUrl}${endpoint}`);
    if (!response.ok) throw new Error(`API Error: ${response.status}`);
    return response.json();
  }
}

export default function NftPage() {
  const router = useRouter();
  const { contract, tokenId } = router.query;
  const [nftData, setNftData] = useState<NftData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState(0);
  
  const apiClient = new ApiClient();
  
  const fetchNftData = useCallback(async () => {
    if (!contract || !tokenId) return;
    setLoading(true);
    setError(null);
    try {
      const data = await apiClient.get<NftData>(`/nft/${contract}/${tokenId}`);
      setNftData(data);
    } catch (err: any) {
      setError(err.message || 'Failed to fetch NFT data');
    } finally {
      setLoading(false);
    }
  }, [contract, tokenId, apiClient]);
  
  useEffect(() => { fetchNftData(); }, [fetchNftData]);
  
  const formatHash = (hash: string): string => hash ? `${hash.substring(0, 10)}...${hash.substring(hash.length - 8)}` : '-';
  
  if (loading) return <Box sx={{ p: 4 }}><Skeleton variant="rectangular" height={400} /></Box>;
  if (error) return <Container maxWidth="lg" sx={{ py: 4 }}><Alert severity="error">{error}</Alert><Button onClick={fetchNftData}>Retry</Button></Container>;
  if (!nftData) return <Container maxWidth="lg" sx={{ py: 4 }}><Alert severity="warning">NFT not found</Alert></Container>;
  
  return (
    <>
      <Head><title>{nftData.name} | TigerScan</title></Head>
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Grid container spacing={3}>
          <Grid item xs={12} md={5}>
            <Card>
              <CardContent>
                <Paper sx={{ bgcolor: 'grey.100', p: 4, textAlign: 'center', minHeight: 300, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                  {nftData.imageUrl ? <img src={nftData.imageUrl} alt={nftData.name} style={{ maxWidth: '100%', maxHeight: 300 }} /> : <Image sx={{ fontSize: 100, color: 'grey.400' }} />}
                </Paper>
              </CardContent>
            </Card>
          </Grid>
          <Grid item xs={12} md={7}>
            <Typography variant="h4" gutterBottom>{nftData.name}</Typography>
            <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>Token ID: #{nftData.tokenId}</Typography>
            {nftData.description && <Typography variant="body1" sx={{ mb: 3 }}>{nftData.description}</Typography>}
            
            <Card variant="outlined" sx={{ mb: 2 }}>
              <CardContent>
                <Typography variant="subtitle2" gutterBottom>Owner</Typography>
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                  <AccountBalanceWallet />
                  <Typography sx={{ fontFamily: 'monospace' }}>{nftData.owner}</Typography>
                </Box>
              </CardContent>
            </Card>
            
            {nftData.attributes && nftData.attributes.length > 0 && (
              <Card variant="outlined">
                <CardContent>
                  <Typography variant="subtitle2" gutterBottom>Attributes</Typography>
                  <Grid container spacing={1}>
                    {nftData.attributes.map((attr, i) => (
                      <Grid item xs={6} sm={4} key={i}>
                        <Paper variant="outlined" sx={{ p: 1.5, textAlign: 'center' }}>
                          <Typography variant="caption" color="text.secondary">{attr.trait_type}</Typography>
                          <Typography variant="body2" fontWeight="bold">{attr.value}</Typography>
                        </Paper>
                      </Grid>
                    ))}
                  </Grid>
                </CardContent>
              </Card>
            )}
          </Grid>
        </Grid>
        
        <Box sx={{ borderBottom: 1, borderColor: 'divider', mt: 4, mb: 2 }}>
          <Tabs value={tab} onChange={(_, v) => setTab(v)}>
            <Tab label={`Transfers (${nftData.transfers?.length || 0})`} />
            <Tab label="Details" />
          </Tabs>
        </Box>
        
        {tab === 0 && nftData.transfers && nftData.transfers.length > 0 && (
          <TableContainer>
            <Table>
              <TableHead><TableRow><TableCell>Transaction</TableCell><TableCell>From</TableCell><TableCell>To</TableCell><TableCell>Date</TableCell></TableRow></TableHead>
              <TableBody>
                {nftData.transfers.map((t, i) => (
                  <TableRow key={i}>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{formatHash(t.hash)}</TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{formatHash(t.from)}</TableCell>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{formatHash(t.to)}</TableCell>
                    <TableCell>{new Date(t.timestamp * 1000).toLocaleString()}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        )}
        
        {tab === 1 && (
          <Grid container spacing={2}>
            <Grid item xs={12} md={6}>
              <Card><CardContent><Typography variant="subtitle2">Contract</Typography><Typography sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>{nftData.contractAddress}</Typography></CardContent></Card>
            </Grid>
            <Grid item xs={12} md={6}>
              <Card><CardContent><Typography variant="subtitle2">Token ID</Typography><Typography>{nftData.tokenId}</Typography></CardContent></Card>
            </Grid>
          </Grid>
        )}
      </Container>
    </>
  );
}