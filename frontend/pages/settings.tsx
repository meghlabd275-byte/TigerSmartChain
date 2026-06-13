/**
 * TigerScan - Settings Page
 */

import { useState } from 'react';
import Head from 'next/head';
import { Container, Typography, Card, CardContent, TextField, Button, Switch, FormControlLabel, Divider, Box, Alert } from '@mui/material';
import { Save, Notifications } from '@mui/icons-material';

export default function SettingsPage() {
  const [saved, setSaved] = useState(false);
  const [settings, setSettings] = useState({
    email: '', darkMode: true, notifications: true, compactTx: false, currency: 'USD'
  });
  
  const handleSave = () => {
    setSaved(true);
    setTimeout(() => setSaved(false), 3000);
  };
  
  return (
    <>
      <Head><title>Settings | TigerScan</title></Head>
      <Container maxWidth="md" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom>Settings</Typography>
        
        {saved && <Alert severity="success" sx={{ mb: 2 }}>Settings saved successfully!</Alert>}
        
        <Card sx={{ mb: 3 }}>
          <CardContent>
            <Typography variant="h6" gutterBottom>Account</Typography>
            <Divider sx={{ my: 2 }} />
            <TextField fullWidth label="Email" value={settings.email} onChange={(e) => setSettings({...settings, email: e.target.value})} sx={{ mb: 2 }} />
          </CardContent>
        </Card>
        
        <Card sx={{ mb: 3 }}>
          <CardContent>
            <Typography variant="h6" gutterBottom>Preferences</Typography>
            <Divider sx={{ my: 2 }} />
            <FormControlLabel control={<Switch checked={settings.darkMode} onChange={(e) => setSettings({...settings, darkMode: e.target.checked})} />} label="Dark Mode" />
            <FormControlLabel control={<Switch checked={settings.notifications} onChange={(e) => setSettings({...settings, notifications: e.target.checked})} />} label="Notifications" />
            <FormControlLabel control={<Switch checked={settings.compactTx} onChange={(e) => setSettings({...settings, compactTx: e.target.checked})} />} label="Compact Transactions" />
          </CardContent>
        </Card>
        
        <Button variant="contained" startIcon={<Save />} onClick={handleSave}>Save Settings</Button>
      </Container>
    </>
  );
}