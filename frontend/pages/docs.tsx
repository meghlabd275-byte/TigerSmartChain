/**
 * TigerScan - Documentation Page
 */

import { useState } from 'react';
import Head from 'next/head';
import { Container, Typography, Card, CardContent, Box, Tabs, Tab, List, ListItem, ListItemText, Link, Divider } from '@mui/material';
import { MenuBook } from '@mui/icons-material';

export default function DocsPage() {
  const [tab, setTab] = useState(0);
  
  const sections = [
    { title: 'Getting Started', content: 'Welcome to TigerScan! This documentation will help you understand and use the TigerScan block explorer.' },
    { title: 'API Reference', content: 'TigerScan provides a comprehensive API for developers. The base URL is /api/v1/' },
    { title: 'Contracts', content: 'TigerScan supports TEP-20, TEP-721, TEP-1155 token standards.' },
    { title: 'Glossary', content: 'Block: A collection of transactions. Transaction: A signed message from an account.' }
  ];
  
  return (
    <>
      <Head><title>Documentation | TigerScan</title></Head>
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom><MenuBook sx={{ mr: 1 }} />Documentation</Typography>
        
        <Box sx={{ borderBottom: 1, borderColor: 'divider', mb: 3 }}>
          <Tabs value={tab} onChange={(_, v) => setTab(v)}>
            {sections.map((s, i) => <Tab key={i} label={s.title} />)}
          </Tabs>
        </Box>
        
        <Card>
          <CardContent>
            <Typography variant="h5" gutterBottom>{sections[tab].title}</Typography>
            <Divider sx={{ my: 2 }} />
            <Typography variant="body1">{sections[tab].content}</Typography>
          </CardContent>
        </Card>
      </Container>
    </>
  );
}