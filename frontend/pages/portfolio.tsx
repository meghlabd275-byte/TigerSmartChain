/**
 * TigerScan - Portfolio Page
 */

import { useState, useEffect } from 'react';
import Head from 'next/head';
import { Container, Typography, Grid, Card, CardContent, TextField, Button, Box, Avatar, Chip, Alert } from '@mui/material';
import { AccountBalanceWallet, Add, TrendingUp } from '@mui/icons-material';

export default function PortfolioPage() {
  const [address, setAddress] = useState('');
  const [portfolios, setPortfolios] = useState<any[]>([]);
  const [loading, setLoading] = useState(false);
  
  const addPortfolio = async () => {
    if (!address) return;
    setLoading(true);
    // Mock portfolio data
    setPortfolios([...portfolios, { address, tokens: [{ name: 'TGR', balance: '1000', value: 1000 }, { name: 'USDT', balance: '500', value: 500 }], totalValue: 1500, chain: 'tigersmartchain' }]);
    setAddress('');
    setLoading(false);
  };
  
  return (
    <>
      <Head><title>Portfolio | TigerScan</title></Head>
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom><AccountBalanceWallet sx={{ mr: 1 }} />Portfolio Tracker</Typography>
        
        <Card sx={{ mb: 3, p: 2 }}>
          <Box sx={{ display: 'flex', gap: 2 }}>
            <TextField fullWidth placeholder="Enter wallet address..." value={address} onChange={(e) => setAddress(e.target.value)} />
            <Button variant="contained" startIcon={<Add />} onClick={addPortfolio}>Add</Button>
          </Box>
        </Card>
        
        {portfolios.length === 0 && <Alert severity="info">Add a wallet address to track your portfolio</Alert>}
        
        <Grid container spacing={3}>
          {portfolios.map((p, i) => (
            <Grid item xs={12} md={6} key={i}>
              <Card>
                <CardContent>
                  <Typography variant="h6">{p.address.substring(0, 8)}...{p.address.substring(36)}</Typography>
                  <Typography variant="h4" color="primary">${p.totalValue.toLocaleString()}</Typography>
                  {p.tokens.map((t: any, j: number) => (
                    <Box key={j} sx={{ display: 'flex', justifyContent: 'space-between', py: 1 }}>
                      <Typography>{t.name}</Typography>
                      <Typography>{t.balance} (${t.value})</Typography>
                    </Box>
                  ))}
                </CardContent>
              </Card>
            </Grid>
          ))}
        </Grid>
      </Container>
    </>
  );
}