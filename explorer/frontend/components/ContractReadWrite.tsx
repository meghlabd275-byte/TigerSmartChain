// Contract Read/Write Interface
// Production-grade interactive contract reader/writer UI
// Features: ABI parsing, function calls, read/write methods, gas estimation

import React, { useState, useEffect, useCallback } from 'react';

// =============================================================================
// TYPES
// =============================================================================

interface ABIFunction {
  name: string;
  type: 'function' | 'constructor' | 'fallback' | 'receive';
  inputs: ABIParameter[];
  outputs: ABIParameter[];
  stateMutability: 'pure' | 'view' | 'nonpayable' | 'payable';
}

interface ABIParameter {
  name: string;
  type: string;
  components?: ABIParameter[];
}

interface ContractMethod {
  function: ABIFunction;
  inputs: { [key: string]: string };
  result?: string;
  error?: string;
  loading?: boolean;
  gasEstimate?: string;
}

interface ContractReadWriteProps {
  contractAddress: string;
  abi?: ABIFunction[];
  title?: string;
  defaultMethod?: string;
}

// =============================================================================
// API
// =============================================================================

const API_BASE = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

async function fetchABI(address: string): Promise<ABIFunction[]> {
  const response = await fetch(`${API_BASE}/api/v1/contracts/${address}/abi`);
  if (!response.ok) {
    throw new Error('Failed to fetch ABI');
  }
  const data = await response.json();
  return data.abi || [];
}

async function callReadMethod(
  address: string,
  method: string,
  params: string[]
): Promise<string> {
  const response = await fetch(`${API_BASE}/api/v1/contracts/${address}/read`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ method, params }),
  });
  
  if (!response.ok) {
    const err = await response.json();
    throw new Error(err.error || 'Call failed');
  }
  
  const data = await response.json();
  return data.result;
}

async function estimateGas(
  address: string,
  method: string,
  params: string[],
  value?: string
): Promise<string> {
  const response = await fetch(`${API_BASE}/api/v1/contracts/${address}/estimate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ method, params, value }),
  });
  
  if (!response.ok) {
    const err = await response.json();
    throw new Error(err.error || 'Estimation failed');
  }
  
  const data = await response.json();
  return data.gas;
}

async function writeMethod(
  address: string,
  method: string,
  params: string[],
  value?: string
): Promise<string> {
  const response = await fetch(`${API_BASE}/api/v1/contracts/${address}/write`, {
    method: 'POST',
    headers: { 
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${localStorage.getItem('auth_token')}`,
    },
    body: JSON.stringify({ method, params, value }),
  });
  
  if (!response.ok) {
    const err = await response.json();
    throw new Error(err.error || 'Write failed');
  }
  
  const data = await response.json();
  return data.txHash;
}

// =============================================================================
// MAIN COMPONENT
// =============================================================================

export default function ContractReadWrite({
  contractAddress,
  abi = [],
  title = 'Contract Interaction',
  defaultMethod,
}: ContractReadWriteProps) {
  const [methods, setMethods] = useState<ABIFunction[]>([]);
  const [selectedMethod, setSelectedMethod] = useState<ABIFunction | null>(null);
  const [inputs, setInputs] = useState<{ [key: string]: string }>({});
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [gasEstimate, setGasEstimate] = useState<string | null>(null);
  const [writeValue, setWriteValue] = useState('');

  // Load ABI
  useEffect(() => {
    if (abi.length > 0) {
      setMethods(abi.filter((f) => f.type === 'function'));
    } else if (contractAddress) {
      fetchABI(contractAddress)
        .then((fetchedABI) => {
          setMethods(fetchedABI.filter((f) => f.type === 'function'));
        })
        .catch(console.error);
    }
  }, [contractAddress, abi]);

  // Set default method
  useEffect(() => {
    if (defaultMethod && methods.length > 0) {
      const method = methods.find((m) => m.name === defaultMethod);
      if (method) {
        setSelectedMethod(method);
      }
    }
  }, [defaultMethod, methods]);

  // Handle method selection
  const handleMethodSelect = (methodName: string) => {
    const method = methods.find((m) => m.name === methodName);
    if (method) {
      setSelectedMethod(method);
      setInputs({});
      setResult(null);
      setError(null);
      setGasEstimate(null);
    }
  };

  // Handle input change
  const handleInputChange = (paramName: string, value: string) => {
    setInputs((prev) => ({ ...prev, [paramName]: value }));
  };

  // Estimate gas
  const handleEstimateGas = useCallback(async () => {
    if (!selectedMethod || !contractAddress) return;

    const params = selectedMethod.inputs.map((input) => inputs[input.name] || '');

    try {
      const gas = await estimateGas(contractAddress, selectedMethod.name, params, writeValue || undefined);
      setGasEstimate(gas);
    } catch (err: any) {
      setError(err.message);
    }
  }, [selectedMethod, contractAddress, inputs, writeValue]);

  // Call read method
  const handleRead = useCallback(async () => {
    if (!selectedMethod || !contractAddress) return;

    setLoading(true);
    setError(null);
    setResult(null);

    try {
      const params = selectedMethod.inputs.map((input) => inputs[input.name] || '');
      const result = await callReadMethod(contractAddress, selectedMethod.name, params);
      setResult(result);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }, [selectedMethod, contractAddress, inputs]);

  // Write method
  const handleWrite = useCallback(async () => {
    if (!selectedMethod || !contractAddress) return;

    setLoading(true);
    setError(null);
    setResult(null);

    try {
      const params = selectedMethod.inputs.map((input) => inputs[input.name] || '');
      const txHash = await writeMethod(
        contractAddress,
        selectedMethod.name,
        params,
        writeValue || undefined
      );
      setResult(`Transaction submitted: ${txHash}`);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }, [selectedMethod, contractAddress, inputs, writeValue]);

  // Determine if method is read or write
  const isWriteMethod = selectedMethod?.stateMutability === 'nonpayable' || selectedMethod?.stateMutability === 'payable';
  const isPayable = selectedMethod?.stateMutability === 'payable';

  return (
    <div style={{ padding: 16, backgroundColor: '#fff', borderRadius: 8 }}>
      <h3 style={{ fontSize: 18, fontWeight: 600, marginBottom: 16 }}>{title}</h3>

      {/* Contract Address */}
      <div style={{ marginBottom: 16 }}>
        <label style={{ display: 'block', fontSize: 12, color: '#6b7280', marginBottom: 4 }}>
          Contract Address
        </label>
        <input
          type="text"
          value={contractAddress}
          readOnly
          style={{
            width: '100%',
            padding: '8px 12px',
            border: '1px solid #e5e7eb',
            borderRadius: 6,
            fontSize: 13,
            fontFamily: 'monospace',
          }}
        />
      </div>

      {/* Method Selector */}
      <div style={{ marginBottom: 16 }}>
        <label style={{ display: 'block', fontSize: 12, color: '#6b7280', marginBottom: 4 }}>
          Select Method
        </label>
        <select
          value={selectedMethod?.name || ''}
          onChange={(e) => handleMethodSelect(e.target.value)}
          style={{
            width: '100%',
            padding: '8px 12px',
            border: '1px solid #e5e7eb',
            borderRadius: 6,
            fontSize: 13,
          }}
        >
          <option value="">-- Select Method --</option>
          {methods.map((method) => (
            <option key={method.name} value={method.name}>
              {method.name} ({method.stateMutability})
            </option>
          ))}
        </select>
      </div>

      {/* Method Info */}
      {selectedMethod && (
        <div style={{ marginBottom: 16, padding: 12, backgroundColor: '#f9fafb', borderRadius: 6 }}>
          <p style={{ fontSize: 12, color: '#6b7280', margin: 0 }}>
            <strong>Type:</strong> {selectedMethod.type} |{' '}
            <strong>State:</strong> {selectedMethod.stateMutability}
          </p>
          {selectedMethod.inputs.length > 0 && (
            <p style={{ fontSize: 12, color: '#6b7280', margin: '8px 0 0' }}>
              <strong>Inputs:</strong> {selectedMethod.inputs.map((i) => `${i.type} ${i.name}`).join(', ')}
            </p>
          )}
          {selectedMethod.outputs.length > 0 && (
            <p style={{ fontSize: 12, color: '#6b7280', margin: '8px 0 0' }}>
              <strong>Outputs:</strong> {selectedMethod.outputs.map((o) => `${o.type} ${o.name}`).join(', ')}
            </p>
          )}
        </div>
      )}

      {/* Inputs */}
      {selectedMethod && selectedMethod.inputs.length > 0 && (
        <div style={{ marginBottom: 16 }}>
          {selectedMethod.inputs.map((input, index) => (
            <div key={index} style={{ marginBottom: 12 }}>
              <label style={{ display: 'block', fontSize: 12, color: '#6b7280', marginBottom: 4 }}>
                {input.name} ({input.type})
              </label>
              <input
                type="text"
                value={inputs[input.name] || ''}
                onChange={(e) => handleInputChange(input.name, e.target.value)}
                placeholder={`Enter ${input.type}`}
                style={{
                  width: '100%',
                  padding: '8px 12px',
                  border: '1px solid #e5e7eb',
                  borderRadius: 6,
                  fontSize: 13,
                  fontFamily: 'monospace',
                }}
              />
            </div>
          ))}
        </div>
      )}

      {/* Payable Value */}
      {isPayable && (
        <div style={{ marginBottom: 16 }}>
          <label style={{ display: 'block', fontSize: 12, color: '#6b7280', marginBottom: 4 }}>
            Value (ETH)
          </label>
          <input
            type="text"
            value={writeValue}
            onChange={(e) => setWriteValue(e.target.value)}
            placeholder="0.0"
            style={{
              width: '100%',
              padding: '8px 12px',
              border: '1px solid #e5e7eb',
              borderRadius: 6,
              fontSize: 13,
            }}
          />
        </div>
      )}

      {/* Actions */}
      {selectedMethod && (
        <div style={{ display: 'flex', gap: 12, marginBottom: 16 }}>
          {!isWriteMethod ? (
            <button
              onClick={handleRead}
              disabled={loading}
              style={{
                flex: 1,
                padding: '10px 16px',
                backgroundColor: loading ? '#9ca3af' : '#3b82f6',
                color: '#fff',
                border: 'none',
                borderRadius: 6,
                fontSize: 14,
                fontWeight: 500,
                cursor: loading ? 'not-allowed' : 'pointer',
              }}
            >
              {loading ? 'Querying...' : 'Query'}
            </button>
          ) : (
            <>
              <button
                onClick={handleEstimateGas}
                disabled={loading}
                style={{
                  flex: 1,
                  padding: '10px 16px',
                  backgroundColor: '#f59e0b',
                  color: '#fff',
                  border: 'none',
                  borderRadius: 6,
                  fontSize: 14,
                  fontWeight: 500,
                  cursor: loading ? 'not-allowed' : 'pointer',
                }}
              >
                Estimate Gas
              </button>
              <button
                onClick={handleWrite}
                disabled={loading}
                style={{
                  flex: 1,
                  padding: '10px 16px',
                  backgroundColor: loading ? '#9ca3af' : '#10b981',
                  color: '#fff',
                  border: 'none',
                  borderRadius: 6,
                  fontSize: 14,
                  fontWeight: 500,
                  cursor: loading ? 'not-allowed' : 'pointer',
                }}
              >
                {loading ? 'Processing...' : 'Write'}
              </button>
            </>
          )}
        </div>
      )}

      {/* Gas Estimate */}
      {gasEstimate && (
        <div
          style={{
            marginBottom: 16,
            padding: 12,
            backgroundColor: '#fef3c7',
            borderRadius: 6,
          }}
        >
          <p style={{ fontSize: 12, margin: 0, color: '#92400e' }}>
            <strong>Estimated Gas:</strong> {gasEstimate}
          </p>
        </div>
      )}

      {/* Result */}
      {result && (
        <div
          style={{
            marginBottom: 16,
            padding: 12,
            backgroundColor: '#ecfdf5',
            borderRadius: 6,
          }}
        >
          <p style={{ fontSize: 12, fontWeight: 500, margin: '0 0 4px', color: '#065f46' }}>
            Result:
          </p>
          <pre
            style={{
              fontSize: 12,
              fontFamily: 'monospace',
              margin: 0,
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-all',
              color: '#065f46',
            }}
          >
            {result}
          </pre>
        </div>
      )}

      {/* Error */}
      {error && (
        <div
          style={{
            marginBottom: 16,
            padding: 12,
            backgroundColor: '#fef2f2',
            borderRadius: 6,
          }}
        >
          <p style={{ fontSize: 12, fontWeight: 500, margin: '0 0 4px', color: '#dc2626' }}>
            Error:
          </p>
          <pre
            style={{
              fontSize: 12,
              fontFamily: 'monospace',
              margin: 0,
              whiteSpace: 'pre-wrap',
              color: '#dc2626',
            }}
          >
            {error}
          </pre>
        </div>
      )}
    </div>
  );
}

// =============================================================================
// COMPACT VERSION
// =============================================================================

export function ContractReadOnly({
  contractAddress,
  method,
  abi = [],
}: {
  contractAddress: string;
  method: string;
  abi?: ABIFunction[];
}) {
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleQuery = async () => {
    setLoading(true);
    setError(null);
    setResult(null);

    try {
      const data = await callReadMethod(contractAddress, method, []);
      setResult(data);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div>
      <button
        onClick={handleQuery}
        disabled={loading}
        style={{
          padding: '6px 12px',
          backgroundColor: loading ? '#9ca3af' : '#3b82f6',
          color: '#fff',
          border: 'none',
          borderRadius: 4,
          fontSize: 12,
          cursor: loading ? 'not-allowed' : 'pointer',
        }}
      >
        {loading ? 'Loading...' : method}
      </button>

      {result && (
        <pre
          style={{
            marginTop: 8,
            padding: 8,
            backgroundColor: '#f9fafb',
            borderRadius: 4,
            fontSize: 11,
            fontFamily: 'monospace',
            overflow: 'auto',
          }}
        >
          {result}
        </pre>
      )}

      {error && (
        <p style={{ marginTop: 8, fontSize: 11, color: '#ef4444' }}>{error}</p>
      )}
    </div>
  );
}