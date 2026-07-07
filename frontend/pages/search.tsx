/**
 * TigerScan - Search Results Page
 */

import { useState, useEffect } from 'react';
import { useRouter } from 'next/router';
import Head from 'next/head';
import { Box, Container, Grid, Card, CardContent, Typography, TextField, InputAdornment, List, ListItem, ListItemIcon, ListItemText, Chip, Button, Alert, Skeleton } from '@mui/material';
import { Search, AccountBalanceWallet, Receipt, Token, Category, HowToVote } from '@mui/icons-material';

export default function SearchPage() {
  const router = useRouter();
  const { q } = router.query;
  const [query, setQuery] = useState((q as string) || '');
  const [results, setResults] = useState<any>({ addresses: [], transactions: [], tokens: [], blocks: [], validators: [] });
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  
  const handleSearch = async () => {
    if (!query.trim()) return;
    setLoading(true);
    setSearched(true);
    
    try {
      await new Promise(r => setTimeout(r, 500));
      
      if (/^0x[a-fA-F0-9]{40}$/.test(query)) {
        setResults({ addresses: [{ address: query, balance: '1.5', type: 'address' }], transactions: [], tokens: [], blocks: [], validators: [] });
      } else if (/^0x[a-fA-F0-9]{64}$/.test(query)) {
        setResults({ addresses: [], transactions: [{ hash: query, value: '1.0', status: 'success' }], tokens: [], blocks: [], validators: [] });
      } else if (/^\d+$/.test(query)) {
        setResults({ addresses: [], transactions: [], tokens: [], blocks: [{ number: parseInt(query), hash: '0x...', txCount: 150 }], validators: [] });
      } else {
        setResults({ addresses: [], transactions: [], tokens: [{ name: query, symbol: query.toUpperCase(), address: '0x...' }], blocks: [], validators: [] });
      }
    } finally {
      setLoading(false);
    }
  };
  
  useEffect(() => { if (q) { setQuery(q as string); handleSearch(); }}, [q]);
  
  const getIcon = (type: string) => {
    switch (type) {
      case 'address': return <AccountBalanceWallet />;
      case 'transaction': return <Receipt />;
      case 'token': return <Token />;
      case 'block': return <Category />;
      case 'validator': return <HowToVote />;
      default: return <Search />;
    }
  };
  
  return (
    <>
      <Head><title>Search | TigerScan</title></Head>
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom>Search</Typography>
        
        <TextField fullWidth placeholder="Search by address, transaction hash, block number, or token..." value={query} onChange={(e) => setQuery(e.target.value)} onKeyPress={(e) => e.key === 'Enter' && handleSearch()} InputProps={{ startAdornment: <InputAdornment position="start"><Search /></InputAdornment>, sx: { fontSize: '1.2rem', py: 1 } }} sx={{ mb: 4 }} />
        
        {loading && <Grid container spacing={2}>{[1,2,3,4].map(i => <Grid item xs={12} key={i}><Skeleton variant="rectangular" height={60} /></Grid>)}</Grid>}
        
        {searched && !loading && (
          results.addresses.length === 0 && results.transactions.length === 0 && results.tokens.length === 0 && results.blocks.length === 0 && results.validators.length === 0
          ? <Alert severity="info">No results found for "{query}"</Alert>
          : <Grid container spacing={2}>
              {results.addresses.map((item: any, i: number) => (
                <Grid item xs={12} key={i}><Card sx={{ cursor: 'pointer' }} onClick={() => router.push(`/address/${item.address}`)}><CardContent><ListItemIcon><AccountBalanceWallet /></ListItemIcon><ListItemText primary="Address" secondary={item.address} /></CardContent></Card></Grid>
              ))}
              {results.transactions.map((item: any, i: number) => (
                <Grid item xs={12} key={i}><Card sx={{ cursor: 'pointer' }} onClick={() => router.push(`/transaction/${item.hash}`)}><CardContent><ListItemIcon><Receipt /></ListItemIcon><ListItemText primary="Transaction" secondary={item.hash} /></CardContent></Card></Grid>
              ))}
              {results.tokens.map((item: any, i: number) => (
                <Grid item xs={12} key={i}><Card sx={{ cursor: 'pointer' }} onClick={() => router.push(`/token/${item.address}`)}><CardContent><ListItemIcon><Token /></ListItemIcon><ListItemText primary={`${item.name} (${item.symbol})`} secondary={item.address} /></CardContent></Card></Grid>
              ))}
              {results.blocks.map((item: any, i: number) => (
                <Grid item xs={12} key={i}><Card sx={{ cursor: 'pointer' }} onClick={() => router.push(`/block/${item.number}`)}><CardContent><ListItemIcon><Category /></ListItemIcon><ListItemText primary={`Block #${item.number}`} secondary={`${item.txCount} transactions`} /></CardContent></Card></Grid>
              ))}
            </Grid>
        )}
      </Container>
    </>
  );
}