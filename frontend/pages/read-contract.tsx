/**
 * TigerScan - Read / Write Contract
 *
 * Reads the ABI the explorer has on file for a verified contract, encodes
 * calldata with ethers, and executes it against the node through the gateway's
 * read-only JSON-RPC passthrough.
 *
 * The previous version could not have worked. It never fetched an ABI — it
 * substituted a hardcoded ERC-20 sample for every address — it generated
 * function selectors with a 32-bit string hash instead of keccak, and it
 * invented the results, returning a random uint256 for balanceOf and a
 * hardcoded "Tiger"/"TGR"/18 for name, symbol and decimals regardless of the
 * contract. It also displayed a fixed sample Solidity file labelled "Verified"
 * for any address entered. All of that is gone: a function now returns what
 * the EVM returned, and a contract with no ABI on file says so.
 */

import React, { useState, useCallback, useEffect } from 'react';
import { Interface, FunctionFragment, Result, getAddress } from 'ethers';
import api from '@/lib/api';

interface AbiInput {
  name: string;
  type: string;
}

interface AbiItem {
  type: string;
  name?: string;
  inputs?: AbiInput[];
  outputs?: AbiInput[];
  stateMutability?: string;
}

const isReadFunction = (item: AbiItem): boolean =>
  item.type === 'function' &&
  (item.stateMutability === 'view' || item.stateMutability === 'pure' || item.stateMutability === undefined) &&
  (item.stateMutability === 'view' || item.stateMutability === 'pure');

const formatResult = (value: unknown): string => {
  if (typeof value === 'bigint') return value.toString();
  if (Array.isArray(value)) return `[${value.map(formatResult).join(', ')}]`;
  if (value && typeof value === 'object' && !(value instanceof String)) {
    return `(${Object.entries(value as Record<string, unknown>)
      .filter(([k]) => Number.isNaN(Number(k)))
      .map(([k, v]) => `${k}: ${formatResult(v)}`)
      .join(', ')})`;
  }
  return String(value);
};

export default function ReadContractPage() {
  const [address, setAddress] = useState('');
  const [iface, setIface] = useState<Interface | null>(null);
  const [functions, setFunctions] = useState<FunctionFragment[]>([]);
  const [selected, setSelected] = useState<FunctionFragment | null>(null);
  const [params, setParams] = useState<string[]>([]);
  const [results, setResults] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadContract = useCallback(async () => {
    const target = address.trim();
    if (!target) return;

    setError(null);
    setIface(null);
    setFunctions([]);
    setSelected(null);
    setResults({});

    let checksummed: string;
    try {
      checksummed = getAddress(target);
    } catch {
      setError('That is not a valid address.');
      return;
    }

    setLoading(true);
    try {
      const res: any = await api.getContractABI(checksummed);
      const abi: AbiItem[] = Array.isArray(res?.abi) ? res.abi : [];
      if (abi.length === 0) {
        setError('No verified ABI on file for this address. Verify the contract first.');
        return;
      }
      const contractIface = new Interface(abi as any);
      const fns = contractIface.fragments.filter(
        (f): f is FunctionFragment => f.type === 'function'
      );
      setIface(contractIface);
      setFunctions(fns);
      if (fns.length > 0) {
        setSelected(fns[0]);
        setParams(fns[0].inputs.map(() => ''));
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load the ABI');
    } finally {
      setLoading(false);
    }
  }, [address]);

  const selectFunction = (fn: FunctionFragment) => {
    setSelected(fn);
    setParams(fn.inputs.map(() => ''));
    setError(null);
  };

  const runFunction = useCallback(async () => {
    if (!iface || !selected) return;

    const target = address.trim();
    setLoading(true);
    setError(null);

    try {
      const parsed = params.map((raw, i) => {
        const type = selected.inputs[i].type;
        if (type === 'address') return getAddress(raw.trim());
        if (type.endsWith(']') || type === 'tuple') return JSON.parse(raw);
        if (/^u?int/.test(type)) return BigInt(raw.trim() || '0');
        return raw;
      });

      const calldata = iface.encodeFunctionData(selected, parsed);
      const raw: string = await api.rpcCall('eth_call', [
        { to: target, data: calldata },
        'latest',
      ]);

      if (selected.outputs && selected.outputs.length > 0) {
        const decoded: Result = iface.decodeFunctionResult(selected, raw);
        setResults((prev) => ({ ...prev, [selected.format('sighash')]: formatResult(decoded) }));
      } else {
        setResults((prev) => ({ ...prev, [selected.format('sighash')]: `call succeeded (${raw})` }));
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'The call failed');
    } finally {
      setLoading(false);
    }
  }, [address, iface, params, selected]);

  useEffect(() => {
    setResults({});
  }, [address]);

  const currentResult = selected ? results[selected.format('sighash')] : undefined;

  return (
    <div className="rwc">
      <div className="rwc-header">
        <h1>📝 Read Contract</h1>
        <p>Call a verified contract&apos;s view functions against the chain</p>
      </div>

      <div className="rwc-address">
        <input
          type="text"
          placeholder="Contract address (0x...)"
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && loadContract()}
        />
        <button onClick={loadContract} disabled={loading || !address.trim()}>
          {loading ? 'Loading…' : 'Load'}
        </button>
      </div>

      {error && <div className="rwc-error">{error}</div>}

      {iface && functions.length > 0 && (
        <div className="rwc-body">
          <div className="rwc-fn-list">
            {functions.map((fn) => (
              <button
                key={fn.format('sighash')}
                className={`rwc-fn ${selected?.format('sighash') === fn.format('sighash') ? 'active' : ''}`}
                onClick={() => selectFunction(fn)}
              >
                <span className={`tag ${isReadFunction({ type: 'function', stateMutability: fn.stateMutability }) ? 'read' : 'write'}`}>
                  {fn.stateMutability === 'view' || fn.stateMutability === 'pure' ? 'read' : 'write'}
                </span>
                {fn.name}
              </button>
            ))}
          </div>

          <div className="rwc-detail">
            {selected && (
              <>
                <h3>
                  {selected.name}
                  <span className="sig">
                    ({selected.inputs.map((i) => `${i.type} ${i.name}`).join(', ')})
                  </span>
                </h3>

                {selected.stateMutability !== 'view' && selected.stateMutability !== 'pure' && (
                  <div className="rwc-note">
                    This function changes state. It is encoded but not sent — the gateway
                    passthrough only forwards read-only methods.
                  </div>
                )}

                {selected.inputs.map((input, i) => (
                  <div className="rwc-param" key={`${input.name}-${i}`}>
                    <label>
                      {input.name || `arg${i}`} <em>{input.type}</em>
                    </label>
                    <input
                      type="text"
                      value={params[i] ?? ''}
                      onChange={(e) => {
                        const next = [...params];
                        next[i] = e.target.value;
                        setParams(next);
                      }}
                      placeholder={input.type === 'address' ? '0x...' : ''}
                    />
                  </div>
                ))}

                <button className="rwc-run" onClick={runFunction} disabled={loading}>
                  {loading ? '⏳ Calling…' : '🔍 Query'}
                </button>

                {currentResult !== undefined && (
                  <div className="rwc-result">
                    <span className="label">Result</span>
                    <span className="value">{currentResult}</span>
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      )}

      {!iface && !error && (
        <div className="rwc-placeholder">
          <div className="icon">📝</div>
          <h3>Contract Interaction</h3>
          <p>Enter a verified contract address to list and call its functions.</p>
        </div>
      )}

      <style>{`
        .rwc { padding: 24px; max-width: 1100px; margin: 0 auto; }
        .rwc-header { margin-bottom: 24px; }
        .rwc-header h1 { font-size: 32px; color: #e2e8f0; margin-bottom: 8px; }
        .rwc-header p { color: #94a3b8; }
        .rwc-address { display: flex; gap: 12px; margin-bottom: 24px; }
        .rwc-address input {
          flex: 1; padding: 14px 16px; background: #1e293b; border: 1px solid #334155;
          border-radius: 8px; color: #e2e8f0; font-family: monospace; font-size: 14px;
        }
        .rwc-address input:focus { outline: none; border-color: #3b82f6; }
        .rwc-address button {
          padding: 14px 24px; background: #3b82f6; border: none; border-radius: 8px;
          color: white; font-weight: 600; cursor: pointer;
        }
        .rwc-address button:disabled { opacity: 0.5; cursor: not-allowed; }
        .rwc-error {
          padding: 12px 16px; background: #ef444420; border: 1px solid #ef4444;
          border-radius: 8px; color: #fca5a5; margin-bottom: 24px;
        }
        .rwc-body { display: grid; grid-template-columns: 260px 1fr; gap: 24px; }
        @media (max-width: 900px) { .rwc-body { grid-template-columns: 1fr; } }
        .rwc-fn-list { display: flex; flex-direction: column; gap: 6px; }
        .rwc-fn {
          display: flex; align-items: center; gap: 8px; text-align: left;
          padding: 10px 12px; background: #1e293b; border: 1px solid #334155;
          border-radius: 8px; color: #cbd5e1; cursor: pointer; font-family: monospace;
        }
        .rwc-fn.active { border-color: #3b82f6; background: #1e3a5f; }
        .tag {
          font-size: 10px; text-transform: uppercase; padding: 2px 6px; border-radius: 4px;
          font-weight: 700;
        }
        .tag.read { background: #10b981; color: #052e16; }
        .tag.write { background: #f59e0b; color: #451a03; }
        .rwc-detail { background: #1e293b; border-radius: 12px; padding: 20px; }
        .rwc-detail h3 { color: #e2e8f0; margin-bottom: 16px; font-family: monospace; }
        .rwc-detail h3 .sig { color: #64748b; font-weight: 400; font-size: 13px; }
        .rwc-note {
          background: #451a03; color: #fed7aa; padding: 10px 12px; border-radius: 6px;
          font-size: 12px; margin-bottom: 16px;
        }
        .rwc-param { margin-bottom: 12px; }
        .rwc-param label { display: block; color: #94a3b8; font-size: 12px; margin-bottom: 4px; }
        .rwc-param label em { color: #64748b; font-style: normal; }
        .rwc-param input {
          width: 100%; padding: 10px 12px; background: #0f172a; border: 1px solid #334155;
          border-radius: 6px; color: #e2e8f0; font-family: monospace; font-size: 13px;
        }
        .rwc-param input:focus { outline: none; border-color: #3b82f6; }
        .rwc-run {
          width: 100%; padding: 12px; background: #10b981; border: none; border-radius: 8px;
          color: #052e16; font-weight: 700; cursor: pointer; margin-top: 8px;
        }
        .rwc-run:disabled { opacity: 0.5; cursor: not-allowed; }
        .rwc-result {
          margin-top: 16px; padding: 12px; background: #0f172a; border-radius: 8px;
          word-break: break-all;
        }
        .rwc-result .label { display: block; color: #64748b; font-size: 11px; text-transform: uppercase; }
        .rwc-result .value { color: #10b981; font-family: monospace; font-size: 13px; }
        .rwc-placeholder { text-align: center; padding: 80px 40px; background: #1e293b; border-radius: 12px; }
        .rwc-placeholder .icon { font-size: 64px; margin-bottom: 16px; }
        .rwc-placeholder h3 { color: #e2e8f0; margin-bottom: 8px; }
        .rwc-placeholder p { color: #64748b; }
      `}</style>
    </div>
  );
}
