/**
 * TigerScan - API Playground Page
 * 
 * Interactive API testing with Swagger
 */

import { useState } from 'react';
import Head from 'next/head';
import { Container, Typography, Card, CardContent, Grid, TextField, Button, Box, Paper, Divider, List, ListItem, ListItemText, Accordion, AccordionSummary, AccordionDetails, Select, MenuItem, FormControl, InputLabel, Chip, Alert } from '@mui/material';
import { ExpandMore, Send, Code, PlayArrow } from '@mui/icons-material';

interface Endpoint {
  method: string;
  path: string;
  description: string;
  params: { name: string; type: string; required: boolean }[];
}

const ENDPOINTS: Endpoint[] = [
  { method: 'GET', path: '/api/v1/blocks', description: 'Get list of blocks', params: [{ name: 'page', type: 'number', required: false }, { name: 'limit', type: 'number', required: false }] },
  { method: 'GET', path: '/api/v1/blocks/{number}', description: 'Get block by number', params: [{ name: 'number', type: 'number', required: true }] },
  { method: 'GET', path: '/api/v1/transactions/{hash}', description: 'Get transaction by hash', params: [{ name: 'hash', type: 'string', required: true }] },
  { method: 'GET', path: '/api/v1/addresses/{address}', description: 'Get address details', params: [{ name: 'address', type: 'string', required: true }] },
  { method: 'GET', path: '/api/v1/tokens/{address}', description: 'Get token details', params: [{ name: 'address', type: 'string', required: true }] },
  { method: 'GET', path: '/api/v1/tokens/{address}/holders', description: 'Get token holders', params: [{ name: 'address', type: 'string', required: true }] },
  { method: 'GET', path: '/api/v1/tokens/{address}/transfers', description: 'Get token transfers', params: [{ name: 'address', type: 'string', required: true }] },
  { method: 'GET', path: '/api/v1/nfts/{address}/{tokenId}', description: 'Get NFT details', params: [{ name: 'address', type: 'string', required: true }, { name: 'tokenId', type: 'string', required: true }] },
  { method: 'GET', path: '/api/v1/validators', description: 'Get validators list', params: [] },
  { method: 'GET', path: '/api/v1/validators/{address}', description: 'Get validator details', params: [{ name: 'address', type: 'string', required: true }] },
  { method: 'GET', path: '/api/v1/search', description: 'Search', params: [{ name: 'q', type: 'string', required: true }] },
  { method: 'GET', path: '/api/v1/gas', description: 'Get current gas price', params: [] },
  { method: 'GET', path: '/api/v1/stats', description: 'Get network stats', params: [] },
];

const METHOD_COLORS: Record<string, string> = {
  GET: '#4caf50',
  POST: '#2196f3',
  PUT: '#ff9800',
  DELETE: '#f44336',
};

export default function ApiPlaygroundPage() {
  const [selectedEndpoint, setSelectedEndpoint] = useState<Endpoint | null>(null);
  const [params, setParams] = useState<Record<string, string>>({});
  const [response, setResponse] = useState<string>('');
  const [loading, setLoading] = useState(false);
  
  const testEndpoint = async () => {
    if (!selectedEndpoint) return;
    
    setLoading(true);
    let url = selectedEndpoint.path;
    
    // Replace path parameters
    for (const [key, value] of Object.entries(params)) {
      if (url.includes(`{${key}}`)) {
        url = url.replace(`{${key}}`, value);
      }
    }
    
    // Add query parameters
    const queryParams = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (!url.includes(`{${key}}`) && value) {
        queryParams.append(key, value);
      }
    }
    
    if (queryParams.toString()) {
      url += '?' + queryParams.toString();
    }
    
    // Simulate API call
    setTimeout(() => {
      setResponse(JSON.stringify({
        success: true,
        data: { message: 'Mock response for ' + url },
        timestamp: Date.now()
      }, null, 2));
      setLoading(false);
    }, 500);
  };
  
  return (
    <>
      <Head><title>API Playground | TigerScan</title></Head>
      <Container maxWidth="xl" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom><Code sx={{ mr: 1 }} />API Playground</Typography>
        
        <Grid container spacing={3}>
          <Grid item xs={12} md={4}>
            <Card sx={{ maxHeight: '80vh', overflow: 'auto' }}>
              <CardContent>
                <Typography variant="h6" gutterBottom>Endpoints</Typography>
                <Divider sx={{ my: 2 }} />
                {ENDPOINTS.map((ep, i) => (
                  <ListItem 
                    key={i} 
                    button 
                    selected={selectedEndpoint?.path === ep.path}
                    onClick={() => { setSelectedEndpoint(ep); setParams({}); }}
                  >
                    <Chip 
                      label={ep.method} 
                      size="small" 
                      sx={{ bgcolor: METHOD_COLORS[ep.method], color: 'white', mr: 1 }} 
                    />
                    <ListItemText 
                      primary={ep.path} 
                      secondary={ep.description}
                      primaryTypographyProps={{ fontSize: '0.85rem', fontFamily: 'monospace' }}
                    />
                  </ListItem>
                ))}
              </CardContent>
            </Card>
          </Grid>
          
          <Grid item xs={12} md={8}>
            {selectedEndpoint ? (
              <Card>
                <CardContent>
                  <Box sx={{ display: 'flex', alignItems: 'center', gap: 2, mb: 2 }}>
                    <Chip 
                      label={selectedEndpoint.method} 
                      sx={{ bgcolor: METHOD_COLORS[selectedEndpoint.method], color: 'white' }} 
                    />
                    <Typography variant="h6" sx={{ fontFamily: 'monospace' }}>{selectedEndpoint.path}</Typography>
                  </Box>
                  <Typography variant="body2" color="text.secondary" sx={{ mb: 3 }}>{selectedEndpoint.description}</Typography>
                  
                  {selectedEndpoint.params.length > 0 && (
                    <>
                      <Typography variant="subtitle2" gutterBottom>Parameters</Typography>
                      <Divider sx={{ my: 2 }} />
                      <Grid container spacing={2}>
                        {selectedEndpoint.params.map((param, i) => (
                          <Grid item xs={12} sm={6} key={i}>
                            <TextField 
                              fullWidth 
                              size="small"
                              label={param.name} 
                              type={param.type}
                              required={param.required}
                              helperText={param.required ? 'Required' : 'Optional'}
                              value={params[param.name] || ''}
                              onChange={(e) => setParams({...params, [param.name]: e.target.value})}
                            />
                          </Grid>
                        ))}
                      </Grid>
                    </>
                  )}
                  
                  <Box sx={{ mt: 3, display: 'flex', gap: 2 }}>
                    <Button 
                      variant="contained" 
                      startIcon={<Send />}
                      onClick={testEndpoint}
                      disabled={loading}
                    >
                      {loading ? 'Sending...' : 'Send Request'}
                    </Button>
                  </Box>
                  
                  {response && (
                    <>
                      <Typography variant="subtitle2" gutterBottom sx={{ mt: 3 }}>Response</Typography>
                      <Divider sx={{ my: 2 }} />
                      <Paper variant="outlined" sx={{ p: 2, bgcolor: 'grey.900', maxHeight: 400, overflow: 'auto' }}>
                        <Typography component="pre" sx={{ fontFamily: 'monospace', fontSize: '0.75rem', color: 'grey.300', whiteSpace: 'pre-wrap' }}>
                          {response}
                        </Typography>
                      </Paper>
                    </>
                  )}
                </CardContent>
              </Card>
            ) : (
              <Alert severity="info">Select an endpoint to test</Alert>
            )}
          </Grid>
        </Grid>
      </Container>
    </>
  );
}