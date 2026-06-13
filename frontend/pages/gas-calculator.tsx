/**
 * TigerScan - Gas Calculator Page
 * 
 * Interactive gas calculator with real-time data
 */

import { useState, useEffect } from 'react';
import Head from 'next/head';
import { Container, Typography, Card, CardContent, Grid, TextField, Button, Box, Alert, Divider, Slider, Chip } from '@mui/material';
import { LocalGasStation, Calculate } from '@mui/icons-material';

export default function GasCalculatorPage() {
  const [gasPrice, setGasPrice] = useState(20);
  const [gasLimit, setGasLimit] = useState(21000);
  const [ethPrice, setEthPrice] = useState(3000);
  const [result, setResult] = useState<{total: number; usd: number} | null>(null);
  
  const presets = [
    { label: 'Slow', value: 5 },
    { label: 'Standard', value: 20 },
    { label: 'Fast', value: 50 },
    { label: 'Instant', value: 100 }
  ];
  
  const calculate = () => {
    const gwei = gasPrice * 1e9;
    const total = (gwei * gasLimit) / 1e18;
    setResult({ total, usd: total * ethPrice });
  };
  
  useEffect(() => { calculate(); }, [gasPrice, gasLimit, ethPrice]);
  
  return (
    <>
      <Head><title>Gas Calculator | TigerScan</title></Head>
      <Container maxWidth="md" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom><LocalGasStation sx={{ mr: 1 }} />Gas Calculator</Typography>
        
        <Card sx={{ mb: 3 }}>
          <CardContent>
            <Typography variant="h6" gutterBottom>Gas Price</Typography>
            <Box sx={{ mb: 2 }}>
              {presets.map(p => (
                <Chip key={p.label} label={`${p.label} (${p.value} Gwei)`} onClick={() => setGasPrice(p.value)} sx={{ mr: 1, mb: 1 }} color={gasPrice === p.value ? 'primary' : 'default'} />
              ))}
            </Box>
            <Slider value={gasPrice} onChange={(_, v) => setGasPrice(v as number)} min={1} max={500} valueLabelDisplay="auto" valueLabelFormat={(v) => `${v} Gwei`} />
          </CardContent>
        </Card>
        
        <Card sx={{ mb: 3 }}>
          <CardContent>
            <Typography variant="h6" gutterBottom>Gas Limit</Typography>
            <TextField fullWidth type="number" value={gasLimit} onChange={(e) => setGasLimit(parseInt(e.target.value) || 21000)} helperText="Standard ETH transfer: 21,000 | ERC-20 transfer: 65,000 | Swap: 150,000+" />
            <Box sx={{ mt: 2 }}>
              <Chip label="ETH Transfer: 21,000" onClick={() => setGasLimit(21000)} sx={{ mr: 1 }} />
              <Chip label="ERC-20: 65,000" onClick={() => setGasLimit(65000)} sx={{ mr: 1 }} />
              <Chip label="Swap: 200,000" onClick={() => setGasLimit(200000)} />
            </Box>
          </CardContent>
        </Card>
        
        <Card sx={{ mb: 3 }}>
          <CardContent>
            <Typography variant="h6" gutterBottom>ETH Price</Typography>
            <TextField fullWidth type="number" value={ethPrice} onChange={(e) => setEthPrice(parseInt(e.target.value) || 3000)} />
          </CardContent>
        </Card>
        
        {result && (
          <Alert severity="success" sx={{ mt: 3 }}>
            <Typography variant="h5">Total Gas Cost</Typography>
            <Typography variant="h4">{result.total.toFixed(6)} ETH</Typography>
            <Typography variant="h5">${result.usd.toFixed(2)} USD</Typography>
          </Alert>
        )}
      </Container>
    </>
  );
}