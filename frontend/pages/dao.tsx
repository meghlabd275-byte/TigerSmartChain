/**
 * TigerScan - DAO Governance Page
 * 
 * Full DAO governance interface
 */

import { useState, useEffect } from 'react';
import Head from 'next/head';
import { Container, Typography, Card, CardContent, Grid, Button, Box, TextField, Paper, Divider, List, ListItem, ListItemText, Chip, Avatar, LinearProgress, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Tab, Tabs, Alert } from '@mui/material';
import { HowToVote, CheckCircle, Cancel, AccountBalance, TrendingUp, Description } from '@mui/icons-material';

interface Proposal {
  id: number;
  title: string;
  description: string;
  status: 'active' | 'passed' | 'failed' | 'executed';
  votesFor: number;
  votesAgainst: number;
  quorum: number;
  proposer: string;
  startBlock: number;
  endBlock: number;
  executed: boolean;
}

interface Delegate {
  address: string;
  votes: number;
  proposalsVoted: number;
}

export default function DaoPage() {
  const [tab, setTab] = useState(0);
  const [proposals, setProposals] = useState<Proposal[]>([]);
  const [delegates, setDelegates] = useState<Delegate[]>([]);
  const [loading, setLoading] = useState(true);
  const [voted, setVoted] = useState(false);
  
  useEffect(() => {
    setTimeout(() => {
      setProposals([
        { id: 1, title: 'Increase Validator Rewards', description: 'Proposal to increase validator rewards from 5% to 8%', status: 'active', votesFor: 1500000, votesAgainst: 500000, quorum: 2000000, proposer: '0x123...', startBlock: 1000, endBlock: 2000, executed: false },
        { id: 2, title: 'Add New Validator', description: 'Add new validator node from community', status: 'passed', votesFor: 2500000, votesAgainst: 200000, quorum: 2000000, proposer: '0x456...', startBlock: 500, endBlock: 1500, executed: true },
        { id: 3, title: 'Reduce Block Time', description: 'Reduce block time from 3s to 2s', status: 'failed', votesFor: 800000, votesAgainst: 1500000, quorum: 2000000, proposer: '0x789...', startBlock: 100, endBlock: 1100, executed: false },
      ]);
      setDelegates([
        { address: '0x1111111111111111111111111111111111111111', votes: 500000, proposalsVoted: 10 },
        { address: '0x2222222222222222222222222222222222222222', votes: 300000, proposalsVoted: 8 },
        { address: '0x3333333333333333333333333333333333333333', votes: 200000, proposalsVoted: 5 },
      ]);
      setLoading(false);
    }, 500);
  }, []);
  
  const getStatusColor = (status: string) => {
    switch (status) {
      case 'active': return 'primary';
      case 'passed': return 'success';
      case 'failed': return 'error';
      case 'executed': return 'success';
      default: return 'default';
    }
  };
  
  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'active': return <HowToVote />;
      case 'passed': return <CheckCircle />;
      case 'failed': return <Cancel />;
      default: return null;
    }
  };
  
  return (
    <>
      <Head><title>DAO Governance | TigerScan</title></Head>
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom><HowToVote sx={{ mr: 1 }} />DAO Governance</Typography>
        
        <Grid container spacing={2} sx={{ mb: 3 }}>
          <Grid item xs={12} md={4}>
            <Card><CardContent sx={{ textAlign: 'center' }}>
              <Typography variant="h4">{delegates.reduce((a, d) => a + d.votes, 0).toLocaleString()}</Typography>
              <Typography variant="body2" color="text.secondary">Total Votes</Typography>
            </CardContent></Card>
          </Grid>
          <Grid item xs={12} md={4}>
            <Card><CardContent sx={{ textAlign: 'center' }}>
              <Typography variant="h4">{proposals.filter(p => p.status === 'active').length}</Typography>
              <Typography variant="body2" color="text.secondary">Active Proposals</Typography>
            </CardContent></Card>
          </Grid>
          <Grid item xs={12} md={4}>
            <Card><CardContent sx={{ textAlign: 'center' }}>
              <Typography variant="h4">{delegates.length}</Typography>
              <Typography variant="body2" color="text.secondary">Delegates</Typography>
            </CardContent></Card>
          </Grid>
        </Grid>
        
        <Box sx={{ borderBottom: 1, borderColor: 'divider', mb: 3 }}>
          <Tabs value={tab} onChange={(_, v) => setTab(v)}>
            <Tab label="Proposals" />
            <Tab label="Cast Vote" />
            <Tab label="Delegates" />
            <Tab label="My Votes" />
          </Tabs>
        </Box>
        
        {tab === 0 && (
          <TableContainer>
            <Table>
              <TableHead><TableRow><TableCell>ID</TableCell><TableCell>Title</TableCell><TableCell>Status</TableCell><TableCell>Votes For</TableCell><TableCell>Votes Against</TableCell><TableCell>Quorum</TableCell></TableRow></TableHead>
              <TableBody>
                {proposals.map(p => (
                  <TableRow key={p.id}>
                    <TableCell>#{p.id}</TableCell>
                    <TableCell>
                      <Typography variant="body2" fontWeight="bold">{p.title}</Typography>
                      <Typography variant="caption" color="text.secondary">{p.description}</Typography>
                    </TableCell>
                    <TableCell>
                      <Chip icon={getStatusIcon(p.status)} label={p.status} color={getStatusColor(p.status) as any} size="small" />
                    </TableCell>
                    <TableCell>
                      <Typography color="success.main">{p.votesFor.toLocaleString()}</Typography>
                      <LinearProgress variant="determinate" value={(p.votesFor / (p.votesFor + p.votesAgainst)) * 100} sx={{ width: 100, height: 4, borderRadius: 2 }} />
                    </TableCell>
                    <TableCell>
                      <Typography color="error.main">{p.votesAgainst.toLocaleString()}</Typography>
                    </TableCell>
                    <TableCell>
                      <Typography>{p.quorum.toLocaleString()}</Typography>
                      <Typography variant="caption" color="text.secondary">{((p.votesFor + p.votesAgainst) / p.quorum * 100).toFixed(1)}%</Typography>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        )}
        
        {tab === 1 && (
          <Card>
            <CardContent>
              <Typography variant="h6" gutterBottom>Cast Your Vote</Typography>
              <Divider sx={{ my: 2 }} />
              <Alert severity="info" sx={{ mb: 2 }}>Connect your wallet to vote on active proposals</Alert>
              {proposals.filter(p => p.status === 'active').map(p => (
                <Box key={p.id} sx={{ mb: 3, p: 2, border: 1, borderColor: 'divider', borderRadius: 1 }}>
                  <Typography variant="subtitle1" fontWeight="bold">{p.title}</Typography>
                  <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>{p.description}</Typography>
                  <Box sx={{ display: 'flex', gap: 2 }}>
                    <Button variant="contained" color="success" startIcon={<CheckCircle />}>Vote For</Button>
                    <Button variant="contained" color="error" startIcon={<Cancel />}>Vote Against</Button>
                  </Box>
                </Box>
              ))}
            </CardContent>
          </Card>
        )}
        
        {tab === 2 && (
          <TableContainer>
            <Table>
              <TableHead><TableRow><TableCell>Delegate</TableCell><TableCell>Votes</TableCell><TableCell>Proposals Voted</TableCell></TableRow></TableHead>
              <TableBody>
                {delegates.map((d, i) => (
                  <TableRow key={i}>
                    <TableCell sx={{ fontFamily: 'monospace', fontSize: '0.75rem' }}>{d.address.substring(0, 10)}...</TableCell>
                    <TableCell>{d.votes.toLocaleString()}</TableCell>
                    <TableCell>{d.proposalsVoted}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        )}
        
        {tab === 3 && (
          <Card>
            <CardContent>
              <Typography variant="h6" gutterBottom>My Voting Power</Typography>
              <Divider sx={{ my: 2 }} />
              <TextField fullWidth label="Wallet Address" placeholder="0x..." sx={{ mb: 2 }} />
              <Button variant="contained">Check Votes</Button>
            </CardContent>
          </Card>
        )}
      </Container>
    </>
  );
}