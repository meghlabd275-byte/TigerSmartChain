/**
 * TigerScan - Analytics Charts Page
 * 
 * Advanced analytics dashboard with:
 * - Network statistics
 * - Transaction charts
 * - Gas analytics
 * - Market data
 */

import { useState, useEffect, useCallback } from 'react';
import Head from 'next/head';
import { 
  Box, 
  Container, 
  Grid, 
  Card, 
  CardContent, 
  Typography, 
  Select, 
  MenuItem,
  FormControl,
  InputLabel,
  Skeleton
} from '@mui/material';
import { 
  ShowChart,
  TrendingUp,
  AccountBalance,
  LocalGasStation,
  Speed,
  Timeline
} from '@mui/icons-material';
import { 
  LineChart, Line, AreaChart, Area, BarChart, Bar, 
  XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, Legend 
} from 'recharts';

const API_CONFIG = {
  baseUrl: process.env.NEXT_PUBLIC_API_URL || '/api'
};

interface ChartData {
  transactions: { date: string; count: number }[];
  gasPrice: { date: string; avg: number; max: number; min: number }[];
  blocks: { date: string; count: number; avgGas: number }[];
  tvl: { date: string; value: number }[];
  addresses: { date: string; new: number; total: number }[];
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

export default function ChartsPage() {
  const [data, setData] = useState<ChartData | null>(null);
  const [loading, setLoading] = useState(true);
  const [timeRange, setTimeRange] = useState('30d');
  
  const apiClient = new ApiClient();
  
  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      const result = await apiClient.get<ChartData>(`/analytics/charts?range=${timeRange}`);
      setData(result);
    } catch (err) {
      console.error('Failed to fetch analytics:', err);
      // Use mock data
      setData({
        transactions: Array.from({length: 30}, (_, i) => ({ date: new Date(Date.now() - (29-i)*86400000).toISOString().split('T')[0], count: Math.floor(Math.random() * 50000) + 10000 })),
        gasPrice: Array.from({length: 30}, (_, i) => ({ date: new Date(Date.now() - (29-i)*86400000).toISOString().split('T')[0], avg: Math.random() * 50 + 10, max: Math.random() * 100 + 30, min: Math.random() * 10 + 5 })),
        blocks: Array.from({length: 30}, (_, i) => ({ date: new Date(Date.now() - (29-i)*86400000).toISOString().split('T')[0], count: Math.floor(Math.random() * 5000) + 1000, avgGas: Math.floor(Math.random() * 50000000) + 20000000 })),
        tvl: Array.from({length: 30}, (_, i) => ({ date: new Date(Date.now() - (29-i)*86400000).toISOString().split('T')[0], value: Math.random() * 1000000000 + 500000000 })),
        addresses: Array.from({length: 30}, (_, i) => ({ date: new Date(Date.now() - (29-i)*86400000).toISOString().split('T')[0], new: Math.floor(Math.random() * 5000) + 1000, total: 100000 + i * 3000 }))
      });
    } finally {
      setLoading(false);
    }
  }, [timeRange, apiClient]);
  
  useEffect(() => { fetchData(); }, [fetchData]);
  
  if (loading || !data) {
    return (
      <Box sx={{ p: 4 }}>
        <Grid container spacing={3}>
          {[1,2,3,4,5,6].map(i => (
            <Grid item xs={12} md={6} key={i}><Skeleton variant="rectangular" height={300} /></Grid>
          ))}
        </Grid>
      </Box>
    );
  }
  
  return (
    <>
      <Head><title>Analytics | TigerScan</title></Head>
      <Container maxWidth="xl" sx={{ py: 4 }}>
        <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', mb: 4 }}>
          <Typography variant="h4"><ShowChart sx={{ mr: 1, verticalAlign: 'middle' }} />Analytics Dashboard</Typography>
          <FormControl sx={{ minWidth: 150 }}>
            <InputLabel>Time Range</InputLabel>
            <Select value={timeRange} label="Time Range" onChange={(e) => setTimeRange(e.target.value)}>
              <MenuItem value="7d">7 Days</MenuItem>
              <MenuItem value="30d">30 Days</MenuItem>
              <MenuItem value="90d">90 Days</MenuItem>
              <MenuItem value="1y">1 Year</MenuItem>
            </Select>
          </FormControl>
        </Box>
        
        <Grid container spacing={3}>
          {/* Transactions Chart */}
          <Grid item xs={12} lg={6}>
            <Card sx={{ height: '100%' }}>
              <CardContent>
                <Typography variant="h6" gutterBottom><Timeline sx={{ mr: 1, verticalAlign: 'middle' }} />Transactions</Typography>
                <ResponsiveContainer width="100%" height={280}>
                  <AreaChart data={data.transactions}>
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey="date" tick={{fontSize: 11}} />
                    <YAxis tick={{fontSize: 11}} />
                    <Tooltip />
                    <Area type="monotone" dataKey="count" stroke="#1976d2" fill="#1976d2" fillOpacity={0.3} name="Transactions" />
                  </AreaChart>
                </ResponsiveContainer>
              </CardContent>
            </Card>
          </Grid>
          
          {/* Gas Price Chart */}
          <Grid item xs={12} lg={6}>
            <Card sx={{ height: '100%' }}>
              <CardContent>
                <Typography variant="h6" gutterBottom><LocalGasStation sx={{ mr: 1, verticalAlign: 'middle' }} />Gas Price (Gwei)</Typography>
                <ResponsiveContainer width="100%" height={280}>
                  <LineChart data={data.gasPrice}>
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey="date" tick={{fontSize: 11}} />
                    <YAxis tick={{fontSize: 11}} />
                    <Tooltip />
                    <Legend />
                    <Line type="monotone" dataKey="avg" stroke="#1976d2" strokeWidth={2} name="Average" />
                    <Line type="monotone" dataKey="max" stroke="#f44336" strokeWidth={1} name="Max" />
                    <Line type="monotone" dataKey="min" stroke="#4caf50" strokeWidth={1} name="Min" />
                  </LineChart>
                </ResponsiveContainer>
              </CardContent>
            </Card>
          </Grid>
          
          {/* Blocks Chart */}
          <Grid item xs={12} lg={6}>
            <Card sx={{ height: '100%' }}>
              <CardContent>
                <Typography variant="h6" gutterBottom><Speed sx={{ mr: 1, verticalAlign: 'middle' }} />Blocks Produced</Typography>
                <ResponsiveContainer width="100%" height={280}>
                  <BarChart data={data.blocks}>
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey="date" tick={{fontSize: 11}} />
                    <YAxis tick={{fontSize: 11}} />
                    <Tooltip />
                    <Bar dataKey="count" fill="#1976d2" name="Blocks" />
                  </BarChart>
                </ResponsiveContainer>
              </CardContent>
            </Card>
          </Grid>
          
          {/* TVL Chart */}
          <Grid item xs={12} lg={6}>
            <Card sx={{ height: '100%' }}>
              <CardContent>
                <Typography variant="h6" gutterBottom><AccountBalance sx={{ mr: 1, verticalAlign: 'middle' }} />TVL (USD)</Typography>
                <ResponsiveContainer width="100%" height={280}>
                  <AreaChart data={data.tvl}>
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey="date" tick={{fontSize: 11}} />
                    <YAxis tick={{fontSize: 11}} tickFormatter={(v) => `$${(v/1e9).toFixed(1)}B`} />
                    <Tooltip formatter={(v: number) => `$${v.toLocaleString()}`} />
                    <Area type="monotone" dataKey="value" stroke="#9c27b0" fill="#9c27b0" fillOpacity={0.3} name="TVL" />
                  </AreaChart>
                </ResponsiveContainer>
              </CardContent>
            </Card>
          </Grid>
          
          {/* New Addresses */}
          <Grid item xs={12}>
            <Card>
              <CardContent>
                <Typography variant="h6" gutterBottom><TrendingUp sx={{ mr: 1, verticalAlign: 'middle' }} />Address Growth</Typography>
                <ResponsiveContainer width="100%" height={280}>
                  <LineChart data={data.addresses}>
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey="date" tick={{fontSize: 11}} />
                    <YAxis tick={{fontSize: 11}} />
                    <Tooltip />
                    <Legend />
                    <Line type="monotone" dataKey="new" stroke="#4caf50" strokeWidth={2} name="New Addresses" />
                    <Line type="monotone" dataKey="total" stroke="#1976d2" strokeWidth={2} name="Total Addresses" />
                  </LineChart>
                </ResponsiveContainer>
              </CardContent>
            </Card>
          </Grid>
        </Grid>
      </Container>
    </>
  );
}