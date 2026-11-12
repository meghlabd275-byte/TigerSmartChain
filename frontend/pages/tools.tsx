/**
 * TigerScan - Developer Tools
 *
 * Three tools, all backed by the gateway rather than by tables in this file:
 * a gas cost calculator priced off the live native rate, a keccak256 signature
 * lookup, and an ABI call encoder.
 */

import { useState, useEffect, useCallback } from 'react';
import Head from 'next/head';
import {
  Box, Container, Card, CardContent, Typography, TextField, Button, Tabs, Tab,
  Paper, Divider, Alert,
  InputAdornment, CircularProgress, Chip, IconButton, Tooltip,
} from '@mui/material';
import { Code, Calculate, Search, Description, ContentCopy, Check } from '@mui/icons-material';
import api from '@/lib/api';

interface SignatureResult {
  signature: string;
  kind: string;
  selector?: string;
  topic?: string;
}

export default function ToolsPage() {
  const [tab, setTab] = useState(0);

  // Gas calculator
  const [gasPrice, setGasPrice] = useState('');
  const [gasLimit, setGasLimit] = useState('21000');
  const [gasResult, setGasResult] = useState<{ costNative: string; costUsd: string; symbol: string } | null>(null);
  const [gasError, setGasError] = useState<string | null>(null);
  const [nativeSymbol, setNativeSymbol] = useState('TGR');
  const [gasPriceLoading, setGasPriceLoading] = useState(true);

  // Signature lookup
  const [signatureInput, setSignatureInput] = useState('');
  const [signatureKind, setSignatureKind] = useState<'function' | 'event'>('function');
  const [signatureResult, setSignatureResult] = useState<SignatureResult | null>(null);
  const [signatureError, setSignatureError] = useState<string | null>(null);
  const [signatureLoading, setSignatureLoading] = useState(false);

  // ABI encoder
  const [encodeSignature, setEncodeSignature] = useState('');
  const [encodeArgs, setEncodeArgs] = useState('');
  const [encodeResult, setEncodeResult] = useState<{ selector: string; calldata: string } | null>(null);
  const [encodeError, setEncodeError] = useState<string | null>(null);
  const [encodeLoading, setEncodeLoading] = useState(false);
  const [copied, setCopied] = useState(false);

  // Seed the gas price from the live oracle rather than a fixed 20 gwei.
  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      setGasPriceLoading(true);
      try {
        const oracle = await api.getGasOracle();
        if (!cancelled) {
          setGasPrice(oracle.standard || oracle.slow || '');
        }
      } catch {
        if (!cancelled) setGasError('Could not read the current gas price. Enter one manually.');
      } finally {
        if (!cancelled) setGasPriceLoading(false);
      }
    };
    load();

    api.getNativePrice()
      .then((p) => { if (!cancelled && p?.symbol) setNativeSymbol(p.symbol); })
      .catch(() => { /* keep the default label */ });

    return () => { cancelled = true; };
  }, []);

  // Cost = gasPrice(wei) * gasLimit, expressed in the native coin. The USD
  // figure comes from the live native price; when that is unavailable the cost
  // is shown in the native coin only rather than converted at a made-up rate.
  const calculateGas = useCallback(async () => {
    setGasError(null);
    setGasResult(null);

    const priceGwei = Number(gasPrice);
    const limit = Number(gasLimit);
    if (!Number.isFinite(priceGwei) || priceGwei < 0) {
      setGasError('Gas price must be a non-negative number.');
      return;
    }
    if (!Number.isFinite(limit) || limit <= 0) {
      setGasError('Gas limit must be a positive number.');
      return;
    }

    const costNative = (priceGwei * 1e-9) * limit;

    let costUsd = '';
    try {
      const native = await api.getNativePrice();
      const usd = Number(native.priceUsd);
      if (Number.isFinite(usd) && usd > 0) {
        costUsd = (costNative * usd).toFixed(2);
      }
    } catch {
      // No indexed price yet: report the native amount and say why there is no
      // dollar figure.
      setGasError('Native coin price is not indexed yet, so no USD estimate is shown.');
    }

    setGasResult({
      costNative: costNative.toFixed(9),
      costUsd,
      symbol: nativeSymbol,
    });
  }, [gasPrice, gasLimit, nativeSymbol]);

  const lookupSignature = useCallback(async () => {
    setSignatureLoading(true);
    setSignatureError(null);
    setSignatureResult(null);
    try {
      const result = await api.computeSignature(signatureInput, signatureKind);
      setSignatureResult(result);
    } catch (err: any) {
      setSignatureError(err?.response?.data?.error || 'Could not compute the hash for that signature.');
    } finally {
      setSignatureLoading(false);
    }
  }, [signatureInput, signatureKind]);

  const encodeCall = useCallback(async () => {
    setEncodeLoading(true);
    setEncodeError(null);
    setEncodeResult(null);
    try {
      // Arguments are entered as a JSON array so each one keeps its type: a
      // uint256 has to stay a string, and an address has to stay quoted.
      let args: unknown[];
      const trimmed = encodeArgs.trim();
      if (trimmed === '') {
        args = [];
      } else {
        try {
          const parsed = JSON.parse(trimmed);
          args = Array.isArray(parsed) ? parsed : [parsed];
        } catch {
          setEncodeError('Arguments must be a JSON array, e.g. ["0xabc...", "1000000000000000000"]');
          setEncodeLoading(false);
          return;
        }
      }
      const result = await api.encodeCall(encodeSignature, args);
      setEncodeResult({ selector: result.selector, calldata: result.calldata });
    } catch (err: any) {
      setEncodeError(err?.response?.data?.error || 'Could not encode that call.');
    } finally {
      setEncodeLoading(false);
    }
  }, [encodeSignature, encodeArgs]);

  const copy = async (text: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <>
      <Head><title>Developer Tools | TigerScan</title></Head>
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom>
          <Code sx={{ mr: 1, verticalAlign: 'middle' }} />
          Developer Tools
        </Typography>

        <Box sx={{ borderBottom: 1, borderColor: 'divider', mb: 3 }}>
          <Tabs value={tab} onChange={(_, v) => setTab(v)}>
            <Tab icon={<Calculate />} label="Gas Calculator" />
            <Tab icon={<Search />} label="Signature Lookup" />
            <Tab icon={<Description />} label="ABI Encoder" />
          </Tabs>
        </Box>

        {tab === 0 && (
          <Card>
            <CardContent>
              <Typography variant="h6" gutterBottom>Gas Calculator</Typography>
              <Divider sx={{ my: 2 }} />
              <Box sx={{ display: 'grid', gap: 3, gridTemplateColumns: { md: '1fr 1fr' } }}>
                <TextField
                  fullWidth
                  label="Gas Price (Gwei)"
                  value={gasPrice}
                  onChange={(e) => setGasPrice(e.target.value)}
                  disabled={gasPriceLoading}
                  helperText={gasPriceLoading ? 'Reading the current gas price...' : 'Prefilled from the live gas oracle'}
                />
                <TextField
                  fullWidth
                  label="Gas Limit"
                  value={gasLimit}
                  onChange={(e) => setGasLimit(e.target.value)}
                  helperText="21000 for a plain transfer"
                />
              </Box>
              <Button variant="contained" size="large" sx={{ mt: 3 }} onClick={calculateGas}>
                Calculate
              </Button>

              {gasError && <Alert severity="info" sx={{ mt: 3 }}>{gasError}</Alert>}

              {gasResult && (
                <Alert severity="success" sx={{ mt: 3 }}>
                  <Typography variant="h6">
                    {gasResult.costNative} {gasResult.symbol}
                    {gasResult.costUsd && ` ($${gasResult.costUsd} USD)`}
                  </Typography>
                </Alert>
              )}
            </CardContent>
          </Card>
        )}

        {tab === 1 && (
          <Card>
            <CardContent>
              <Typography variant="h6" gutterBottom>Signature Hash</Typography>
              <Typography variant="body2" color="text.secondary" gutterBottom>
                keccak256 of the canonical signature. Functions give a 4-byte selector;
                events give a 32-byte topic0.
              </Typography>
              <Divider sx={{ my: 2 }} />

              <Box sx={{ display: 'flex', gap: 2, mb: 2, alignItems: 'center' }}>
                <Button
                  variant={signatureKind === 'function' ? 'contained' : 'outlined'}
                  onClick={() => setSignatureKind('function')}
                >
                  Function
                </Button>
                <Button
                  variant={signatureKind === 'event' ? 'contained' : 'outlined'}
                  onClick={() => setSignatureKind('event')}
                >
                  Event
                </Button>
              </Box>

              <TextField
                fullWidth
                label={signatureKind === 'function' ? 'Function signature' : 'Event signature'}
                value={signatureInput}
                onChange={(e) => setSignatureInput(e.target.value)}
                placeholder={signatureKind === 'function'
                  ? 'transfer(address,uint256)'
                  : 'Transfer(address,address,uint256)'}
                InputProps={{
                  startAdornment: <InputAdornment position="start"><Search /></InputAdornment>,
                }}
                onKeyDown={(e) => { if (e.key === 'Enter') lookupSignature(); }}
                sx={{ mb: 2 }}
              />
              <Button variant="contained" onClick={lookupSignature} disabled={signatureLoading}>
                {signatureLoading ? <CircularProgress size={20} /> : 'Compute hash'}
              </Button>

              {signatureError && <Alert severity="error" sx={{ mt: 2 }}>{signatureError}</Alert>}

              {signatureResult && (
                <Paper variant="outlined" sx={{ p: 2, mt: 2 }}>
                  <Typography variant="caption" color="text.secondary">Canonical signature</Typography>
                  <Typography variant="body1" sx={{ fontFamily: 'monospace', mb: 1 }}>
                    {signatureResult.signature}
                  </Typography>
                  <Typography variant="caption" color="text.secondary">
                    {signatureResult.selector ? 'Selector (4 bytes)' : 'Topic0 (32 bytes)'}
                  </Typography>
                  <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                    <Typography variant="body2" sx={{ fontFamily: 'monospace', wordBreak: 'break-all' }}>
                      {signatureResult.selector || signatureResult.topic}
                    </Typography>
                    <Tooltip title={copied ? 'Copied' : 'Copy'}>
                      <IconButton size="small" onClick={() => copy(signatureResult.selector || signatureResult.topic || '')}>
                        {copied ? <Check fontSize="small" /> : <ContentCopy fontSize="small" />}
                      </IconButton>
                    </Tooltip>
                  </Box>
                </Paper>
              )}
            </CardContent>
          </Card>
        )}

        {tab === 2 && (
          <Card>
            <CardContent>
              <Typography variant="h6" gutterBottom>ABI Call Encoder</Typography>
              <Typography variant="body2" color="text.secondary" gutterBottom>
                The selector followed by the ABI-encoded arguments.
              </Typography>
              <Divider sx={{ my: 2 }} />

              <TextField
                fullWidth
                label="Function signature"
                value={encodeSignature}
                onChange={(e) => setEncodeSignature(e.target.value)}
                placeholder="transfer(address,uint256)"
                sx={{ mb: 2 }}
              />
              <TextField
                fullWidth
                multiline
                rows={3}
                label="Arguments (JSON array, in order)"
                value={encodeArgs}
                onChange={(e) => setEncodeArgs(e.target.value)}
                placeholder='["0x000000000000000000000000000000000000dEaD", "1000000000000000000"]'
                helperText="Large integers must be quoted strings so they keep full precision."
                sx={{ mb: 2 }}
              />
              <Button variant="contained" onClick={encodeCall} disabled={encodeLoading}>
                {encodeLoading ? <CircularProgress size={20} /> : 'Encode'}
              </Button>

              {encodeError && <Alert severity="error" sx={{ mt: 2 }}>{encodeError}</Alert>}

              {encodeResult && (
                <Paper variant="outlined" sx={{ p: 2, mt: 2 }}>
                  <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 1 }}>
                    <Chip label={`selector ${encodeResult.selector}`} size="small" />
                  </Box>
                  <Typography variant="caption" color="text.secondary">Calldata</Typography>
                  <Box sx={{ display: 'flex', alignItems: 'flex-start', gap: 1 }}>
                    <Typography variant="body2" sx={{ fontFamily: 'monospace', wordBreak: 'break-all', flex: 1 }}>
                      {encodeResult.calldata}
                    </Typography>
                    <Tooltip title={copied ? 'Copied' : 'Copy'}>
                      <IconButton size="small" onClick={() => copy(encodeResult.calldata)}>
                        {copied ? <Check fontSize="small" /> : <ContentCopy fontSize="small" />}
                      </IconButton>
                    </Tooltip>
                  </Box>
                </Paper>
              )}
            </CardContent>
          </Card>
        )}
      </Container>
    </>
  );
}
