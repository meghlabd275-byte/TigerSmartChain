/**
 * TigerScan - Transaction Simulator
 *
 * Runs a real EVM trace against the node through the gateway's read-only
 * JSON-RPC passthrough (POST /api/v1/rpc). Every number shown comes from the
 * node: eth_estimateGas, eth_call and debug_traceCall with the callTracer.
 *
 * This page previously fabricated its results. It invented a gas figure with
 * Math.random(), a 20-entry trace of random opcodes, a fake state change
 * ("100 ETH" -> a hardcoded remainder) and an unconditional Transfer log, then
 * reported "Transaction Would Succeed" with a success flag drawn from
 * Math.random(). A simulator that guesses is worse than no simulator: it tells
 * a user a transaction is safe when nothing was executed.
 */

import React, { useState, useCallback } from 'react';

interface CallFrame {
  type?: string;
  from?: string;
  to?: string;
  value?: string;
  gas?: string;
  gasUsed?: string;
  input?: string;
  output?: string;
  error?: string;
  calls?: CallFrame[];
}

interface SimulationResult {
  callSucceeded: boolean;
  returnValue: string | null;
  revertReason: string | null;
  gasEstimate: number | null;
  callTrace: CallFrame | null;
  gasUsed: number | null;
}

const hexToInt = (hex: string | undefined | null): number | null => {
  if (!hex) return null;
  const n = parseInt(hex, 16);
  return Number.isFinite(n) ? n : null;
};

// The gateway forwards only the methods on its allow-list and returns the raw
// JSON-RPC `result` field.
const rpc = async (method: string, params: unknown[]): Promise<any> => {
  const root = (process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080').replace(/\/+$/, '');
  const base = /\/api\/v1$/.test(root) ? root : `${root}/api/v1`;
  const res = await fetch(`${base}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ method, params }),
  });
  const body = await res.json().catch(() => null);
  if (!res.ok) {
    throw new Error(body?.error || body?.detail || `RPC request failed (${res.status})`);
  }
  return body;
};

const toHexQuantity = (decimal: string): string => {
  const trimmed = decimal.trim();
  if (!trimmed || trimmed === '0') return '0x0';
  if (!/^\d+$/.test(trimmed)) {
    throw new Error('Value must be a whole number of wei');
  }
  return `0x${BigInt(trimmed).toString(16)}`;
};

export default function SimulatorPage() {
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [valueWei, setValueWei] = useState('0');
  const [data, setData] = useState('');
  const [simulating, setSimulating] = useState(false);
  const [result, setResult] = useState<SimulationResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleSimulate = useCallback(async () => {
    setSimulating(true);
    setResult(null);
    setError(null);

    const call: Record<string, string> = { to };
    if (from) call.from = from;
    if (data) call.data = data;
    try {
      const valueHex = toHexQuantity(valueWei || '0');
      if (valueHex !== '0x0') call.value = valueHex;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Invalid value');
      setSimulating(false);
      return;
    }

    let callSucceeded = false;
    let returnValue: string | null = null;
    let revertReason: string | null = null;
    let gasEstimate: number | null = null;
    let callTrace: CallFrame | null = null;
    let gasUsed: number | null = null;

    // eth_call reports a revert as a JSON-RPC error whose data is the encoded
    // reason. That is the call's outcome, not a page failure, so it is recorded
    // rather than thrown.
    try {
      returnValue = await rpc('eth_call', [call, 'latest']);
      callSucceeded = true;
    } catch (err) {
      revertReason = err instanceof Error ? err.message : 'call failed';
    }

    try {
      const est = await rpc('eth_estimateGas', [call]);
      gasEstimate = hexToInt(est);
    } catch {
      // Estimation reverts for the same reason the call did; the revert
      // message from eth_call is the one worth showing, so this is left null.
      gasEstimate = null;
    }

    try {
      callTrace = await rpc('debug_traceCall', [call, 'latest', { tracer: 'callTracer' }]);
      gasUsed = hexToInt(callTrace?.gasUsed);
      if (callTrace?.error && !revertReason) {
        revertReason = callTrace.error;
        callSucceeded = false;
      }
    } catch (err) {
      // A node without the debug namespace is a real limitation, so it is
      // surfaced instead of silently dropping the trace section.
      setError(err instanceof Error ? err.message : 'Trace unavailable');
    }

    setResult({ callSucceeded, returnValue, revertReason, gasEstimate, callTrace, gasUsed });
    setSimulating(false);
  }, [from, to, valueWei, data]);

  const presets = [
    { name: 'Plain Transfer', data: '', value: '1000000000000000000' },
    { name: 'ERC-20 transfer', data: '0xa9059cbb0000000000000000000000000000000000000000000000000000000000000000', value: '0' },
    { name: 'ERC-20 approve', data: '0x095ea7b30000000000000000000000000000000000000000000000000000000000000000', value: '0' },
  ];

  return (
    <div className="simulation-page">
      <div className="page-header">
        <h1>⚡ Transaction Simulator</h1>
        <p>Trace a call against the node before you send it: return data, gas and the call tree</p>
      </div>

      {error && <div className="error-banner">{error}</div>}

      <div className="content">
        <div className="input-panel">
          <h3>Call Details</h3>

          <div className="presets">
            <span className="label">Quick Presets:</span>
            {presets.map((preset) => (
              <button
                key={preset.name}
                className="preset-btn"
                onClick={() => {
                  setData(preset.data);
                  setValueWei(preset.value);
                }}
              >
                {preset.name}
              </button>
            ))}
          </div>

          <Field label="From Address (optional)" value={from} onChange={setFrom} placeholder="0x..." />
          <Field label="To Address" value={to} onChange={setTo} placeholder="0x... (contract or wallet)" />
          <Field label="Value (wei)" value={valueWei} onChange={setValueWei} placeholder="0" />
          <Field label="Calldata (hex)" value={data} onChange={setData} placeholder="0x..." />

          <button className="simulate-btn" onClick={handleSimulate} disabled={simulating || !to}>
            {simulating ? '⏳ Tracing…' : '🔍 Simulate Call'}
          </button>
        </div>

        <div className="result-panel">
          {result ? (
            <SimulationResultView result={result} />
          ) : (
            <div className="placeholder">
              <div className="placeholder-icon">🔍</div>
              <h3>Nothing traced yet</h3>
              <p>Enter a call and run it. The result is the node&apos;s own execution, not an estimate.</p>
            </div>
          )}
        </div>
      </div>

      <style>{`
        .simulation-page { padding: 24px; max-width: 1400px; margin: 0 auto; }
        .page-header { margin-bottom: 24px; }
        .page-header h1 { font-size: 32px; color: #e2e8f0; margin-bottom: 8px; }
        .page-header p { color: #94a3b8; }
        .error-banner {
          background: #7f1d1d; color: #fecaca; padding: 12px 16px;
          border-radius: 8px; margin-bottom: 16px;
        }
        .content { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }
        @media (max-width: 1024px) { .content { grid-template-columns: 1fr; } }
        .input-panel { background: #1e293b; border-radius: 12px; padding: 24px; }
        .input-panel h3 { color: #e2e8f0; margin-bottom: 20px; }
        .presets { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 20px; align-items: center; }
        .presets .label { color: #64748b; font-size: 12px; margin-right: 8px; }
        .preset-btn {
          padding: 6px 12px; background: #334155; border: none; border-radius: 6px;
          color: #e2e8f0; font-size: 12px; cursor: pointer;
        }
        .preset-btn:hover { background: #475569; }
        .simulate-btn {
          width: 100%; padding: 14px; background: linear-gradient(135deg, #3b82f6, #8b5cf6);
          border: none; border-radius: 8px; color: white; font-size: 16px;
          font-weight: 600; cursor: pointer; margin-top: 20px;
        }
        .simulate-btn:disabled { opacity: 0.5; cursor: not-allowed; }
        .result-panel { min-height: 400px; }
        .placeholder { background: #1e293b; border-radius: 12px; padding: 60px 40px; text-align: center; }
        .placeholder-icon { font-size: 48px; margin-bottom: 16px; }
        .placeholder h3 { color: #e2e8f0; margin-bottom: 12px; }
        .placeholder p { color: #64748b; }
        .result-card { background: #1e293b; border-radius: 12px; padding: 20px; }
        .result-status { font-size: 16px; font-weight: 600; margin-bottom: 16px; }
        .result-status.ok { color: #10b981; }
        .result-status.bad { color: #ef4444; }
        .result-stats { display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; margin-bottom: 16px; }
        .stat { background: #0f172a; border-radius: 8px; padding: 12px; }
        .stat .label { display: block; color: #64748b; font-size: 11px; text-transform: uppercase; }
        .stat .value { color: #e2e8f0; font-family: monospace; font-size: 13px; word-break: break-all; }
        .section { margin-top: 16px; }
        .section h4 { color: #cbd5e1; font-size: 13px; margin-bottom: 8px; text-transform: uppercase; }
        .frame { border-left: 2px solid #334155; padding-left: 12px; margin: 8px 0; }
        .frame .line { color: #94a3b8; font-family: monospace; font-size: 12px; word-break: break-all; }
        .revert { color: #fca5a5; font-family: monospace; font-size: 12px; word-break: break-all; }
      `}</style>
    </div>
  );
}

const Field: React.FC<{
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}> = ({ label, value, onChange, placeholder }) => (
  <div className="input-group">
    <label>{label}</label>
    <input type="text" value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} />
    <style>{`
      .input-group { margin-bottom: 16px; }
      .input-group label {
        display: block; color: #94a3b8; font-size: 12px; margin-bottom: 8px; text-transform: uppercase;
      }
      .input-group input {
        width: 100%; padding: 12px 16px; background: #0f172a; border: 1px solid #334155;
        border-radius: 8px; color: #e2e8f0; font-size: 14px; font-family: monospace;
      }
      .input-group input:focus { outline: none; border-color: #3b82f6; }
    `}</style>
  </div>
);

const CallFrameView: React.FC<{ frame: CallFrame; depth: number }> = ({ frame, depth }) => (
  <div className="frame" style={{ marginLeft: depth * 12 }}>
    <div className="line">
      {frame.type || 'CALL'} {frame.to || frame.from || ''}
    </div>
    <div className="line">gas {frame.gas || '—'} · used {frame.gasUsed || '—'}</div>
    {frame.input && frame.input !== '0x' && <div className="line">in {frame.input}</div>}
    {frame.output && frame.output !== '0x' && <div className="line">out {frame.output}</div>}
    {frame.error && <div className="revert">error: {frame.error}</div>}
    {frame.calls?.map((child, i) => (
      <CallFrameView key={i} frame={child} depth={depth + 1} />
    ))}
  </div>
);

const SimulationResultView: React.FC<{ result: SimulationResult }> = ({ result }) => (
  <div className="result-card">
    <div className={`result-status ${result.callSucceeded ? 'ok' : 'bad'}`}>
      {result.callSucceeded ? '✓ eth_call succeeded' : '✗ eth_call reverted'}
    </div>

    <div className="result-stats">
      <div className="stat">
        <span className="label">Gas Used (trace)</span>
        <span className="value">{result.gasUsed !== null ? result.gasUsed.toLocaleString() : '—'}</span>
      </div>
      <div className="stat">
        <span className="label">Gas Estimate</span>
        <span className="value">
          {result.gasEstimate !== null ? result.gasEstimate.toLocaleString() : 'reverts'}
        </span>
      </div>
    </div>

    {result.returnValue && result.returnValue !== '0x' && (
      <div className="section">
        <h4>Return data</h4>
        <div className="frame"><div className="line">{result.returnValue}</div></div>
      </div>
    )}

    {result.revertReason && (
      <div className="section">
        <h4>Revert</h4>
        <div className="revert">{result.revertReason}</div>
      </div>
    )}

    {result.callTrace && (
      <div className="section">
        <h4>Call trace</h4>
        <CallFrameView frame={result.callTrace} depth={0} />
      </div>
    )}
  </div>
);
