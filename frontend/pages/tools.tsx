/**
 * TigerScan - Developer Tools Page
 * 
 * Tools: Gas Calculator, ABI Encoder, Signature Lookup, etc.
 */

import { useState } from 'react';
import Head from 'next/head';
import { Box, Container, Grid, Card, CardContent, Typography, TextField, Button, Tabs, Tab, Paper, Divider, Alert, Table, TableBody, TableCell, TableHead, TableRow } from '@mui/material';
import { Code, Calculate, Search, Description } from '@mui/icons-material';

const COMMON_SIGNATURES: Record<string, string> = {
  '0xa9059cbb': 'ERC20 transfer(address,uint256)',
  '0x23b872dd': 'ERC20 transferFrom(address,address,uint256)',
  '0x095ea7b3': 'ERC20 approve(address,uint256)',
  '0x40c10f19': 'ERC20 mint(address,uint256)',
  '0x2e1a7d4d': 'ERC20 burn(uint256)',
  '0x42842e0e': 'ERC721 safeTransferFrom(address,address,uint256)',
  '0xb88d4fde': 'ERC721 safeTransferFrom(address,address,uint256,bytes)',
  '0xf242432a': 'ERC1155 safeTransferFrom(address,address,uint256,uint256,bytes)',
  '0x2eb2c2d6': 'ERC1155 safeBatchTransferFrom(address,address,uint256[],uint256[],bytes)',
};

export default function ToolsPage() {
  const [tab, setTab] = useState(0);
  const [gasPrice, setGasPrice] = useState('20');
  const [gasLimit, setGasLimit] = useState('21000');
  const [calculatedGas, setCalculatedGas] = useState<{total: string; usd: string} | null>(null);
  const [signature, setSignature] = useState('');
  const [signatureResult, setSignatureResult] = useState<string | null>(null);
  const [abiInput, setAbiInput] = useState('');
  const [abiOutput, setAbiOutput] = useState('');
  
  const calculateGas = () => {
    const price = parseFloat(gasPrice) * 1e9;
    const limit = parseFloat(gasLimit);
    const total = (price * limit) / 1e18;
    const usd = total * 3000;
    setCalculatedGas({ total: total.toFixed(6), usd: usd.toFixed(2) });
  };
  
  const lookupSignature = () => {
    const result = COMMON_SIGNATURES[signature.toLowerCase()] || 'Unknown function signature';
    setSignatureResult(result);
  };
  
  return (
    <>
      <Head><title>Developer Tools | TigerScan</title></Head>
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom><Code sx={{ mr: 1, verticalAlign: 'middle' }} />Developer Tools</Typography>
        
        <Box sx={{ borderBottom: 1, borderColor: 'divider', mb: 3 }}>
          <Tabs value={tab} onChange={(_, v) => setTab(v)}>
            <Tab icon={<Calculate />} label="Gas Calculator" />
            <Tab icon={<Search />} label="Signature Lookup" />
            <Tab icon={<Description />} label="ABI Encoder" />
          </Tabs>
        </Box>
        
        {/* Gas Calculator */}
        {tab === 0 && (
          <Card>
            <CardContent>
              <Typography variant="h6" gutterBottom>Gas Calculator</Typography>
              <Divider sx={{ my: 2 }} />
              <Grid container spacing={3}>
                <Grid item xs={12} md={6}>
                  <TextField fullWidth label="Gas Price (Gwei)" value={gasPrice} onChange={(e) => setGasPrice(e.target.value)} helperText="Current network gas price" />
                </Grid>
                <Grid item xs={12} md={6}>
                  <TextField fullWidth label="Gas Limit" value={gasLimit} onChange={(e) => setGasLimit(e.target.value)} helperText="Estimated gas for transaction" />
                </Grid>
                <Grid item xs={12}>
                  <Button variant="contained" size="large" onClick={calculateGas}>Calculate</Button>
                </Grid>
                {calculatedGas && (
                  <Grid item xs={12}>
                    <Alert severity="success">
                      <Typography variant="h6">Total Gas Cost: {calculatedGas.total} ETH (${calculatedGas.usd} USD)</Typography>
                    </Alert>
                  </Grid>
                )}
              </Grid>
            </CardContent>
          </Card>
        )}
        
        {/* Signature Lookup */}
        {tab === 1 && (
          <Card>
            <CardContent>
              <Typography variant="h6" gutterBottom>Function Signature Lookup</Typography>
              <Divider sx={{ my: 2 }} />
              <TextField fullWidth label="Function Signature (4 bytes)" value={signature} onChange={(e) => setSignature(e.target.value)} placeholder="e.g., 0xa9059cbb" sx={{ mb: 2 }} />
              <Button variant="contained" onClick={lookupSignature}>Lookup</Button>
              {signatureResult && (
                <Alert severity="info" sx={{ mt: 2 }}>
                  <Typography variant="body1">{signatureResult}</Typography>
                </Alert>
              )}
              <Divider sx={{ my: 3 }} />
              <Typography variant="subtitle2" gutterBottom>Common Signatures</Typography>
              <Table size="small">
                <TableHead><TableRow><TableCell>Signature</TableCell><TableCell>Function</TableCell></TableRow></TableHead>
                <TableBody>
                  {Object.entries(COMMON_SIGNATURES).map(([sig, name]) => (
                    <TableRow key={sig} hover sx={{ cursor: 'pointer' }} onClick={() => { setSignature(sig); lookupSignature(); }}>
                      <TableCell sx={{ fontFamily: 'monospace' }}>{sig}</TableCell><TableCell>{name}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        )}
        
        {/* ABI Encoder */}
        {tab === 2 && (
          <Card>
            <CardContent>
              <Typography variant="h6" gutterBottom>ABI Encoder</Typography>
              <Divider sx={{ my: 2 }} />
              <TextField fullWidth multiline rows={6} label="JSON ABI" value={abiInput} onChange={(e) => setAbiInput(e.target.value)} placeholder='[{"name": "transfer", "type": "function", "inputs": [{"name": "to", "type": "address"}, {"name": "value", "type": "uint256"}]}]' sx={{ mb: 2 }} />
              <Button variant="contained" onClick={() => setAbiOutput('Encoded data would appear here...')}>Encode</Button>
              {abiOutput && (
                <Paper variant="outlined" sx={{ p: 2, mt: 2 }}>
                  <Typography variant="body2" sx={{ fontFamily: 'monospace', wordBreak: 'break-all' }}>{abiOutput}</Typography>
                </Paper>
              )}
            </CardContent>
          </Card>
        )}
      </Container>
    </>
  );
}