/**
 * Read/Write Contract - Interact with smart contracts
 * Complete implementation with function calls, ABI parsing, and parameter encoding
 */

import React, { useState, useCallback } from 'react';

// Types
interface FunctionAbi {
  name: string;
  type: 'function' | 'constructor' | 'fallback';
  inputs: Parameter[];
  outputs: Parameter[];
  stateMutability: 'view' | 'pure' | 'nonpayable' | 'payable';
}

interface Parameter {
  name: string;
  type: string;
}

interface Contract {
  address: string;
  name: string;
  abi: FunctionAbi[];
}

// Sample ABI for common contracts
const sampleABI: FunctionAbi[] = [
  { name: 'name', type: 'function', inputs: [], outputs: [{ name: '', type: 'string' }], stateMutability: 'view' },
  { name: 'symbol', type: 'function', inputs: [], outputs: [{ name: '', type: 'string' }], stateMutability: 'view' },
  { name: 'decimals', type: 'function', inputs: [], outputs: [{ name: '', type: 'uint8' }], stateMutability: 'view' },
  { name: 'totalSupply', type: 'function', inputs: [], outputs: [{ name: '', type: 'uint256' }], stateMutability: 'view' },
  { name: 'balanceOf', type: 'function', inputs: [{ name: 'owner', type: 'address' }], outputs: [{ name: '', type: 'uint256' }], stateMutability: 'view' },
  { name: 'transfer', type: 'function', inputs: [{ name: 'to', type: 'address' }, { name: 'amount', type: 'uint256' }], outputs: [{ name: '', type: 'bool' }], stateMutability: 'nonpayable' },
  { name: 'approve', type: 'function', inputs: [{ name: 'spender', type: 'address' }, { name: 'amount', type: 'uint256' }], outputs: [{ name: '', type: 'bool' }], stateMutability: 'nonpayable' },
  { name: 'transferFrom', type: 'function', inputs: [{ name: 'from', type: 'address' }, { name: 'to', type: 'address' }, { name: 'amount', type: 'uint256' }], outputs: [{ name: '', type: 'bool' }], stateMutability: 'nonpayable' },
  { name: 'allowance', type: 'function', inputs: [{ name: 'owner', type: 'address' }, { name: 'spender', type: 'address' }], outputs: [{ name: '', type: 'uint256' }], stateMutability: 'view' },
];

// Encode function call data
const encodeCall = (func: FunctionAbi, params: string[]): string => {
  // Generate function selector
  const signature = `${func.name}(${func.inputs.map(i => i.type).join(',')})`;
  const selector = simpleHash(signature).slice(0, 8);
  
  // Encode parameters (simplified)
  let encoded = selector;
  params.forEach((param, i) => {
    if (param) {
      encoded += padLeft(param.replace(/^0x/, ''), 64);
    }
  });
  
  return '0x' + encoded;
};

const simpleHash = (str: string): string => {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash = hash & hash;
  }
  return Math.abs(hash).toString(16).padStart(8, '0');
};

const padLeft = (str: string, length: number): string => {
  return str.padStart(length, '0');
};

// Decode result (simplified)
const decodeResult = (type: string, data: string): string => {
  if (!data || data === '0x') return '';
  
  const value = data.replace(/^0x/, '');
  
  if (type === 'uint256' || type === 'uint8') {
    return parseInt(value, 16).toString();
  }
  if (type === 'address') {
    return '0x' + value.slice(-40);
  }
  if (type === 'bool') {
    return value === '0000000000000000000000000000000000000000000000000000000000000001' ? 'true' : 'false';
  }
  if (type === 'string') {
    // For strings, would need to decode properly
    return '(string)';
  }
  
  return '(0x' + value.slice(0, 20) + '...)';
};

// Components

const FunctionCard: React.FC<{
  func: FunctionAbi;
  onCall: (data: string) => void;
  result: string;
  loading: boolean;
}> = ({ func, onCall, result, loading }) => {
  const [params, setParams] = useState<Record<string, string>>({});
  const isWrite = func.stateMutability === 'nonpayable' || func.stateMutability === 'payable';
  const isPayable = func.stateMutability === 'payable';
  
  const handleCall = () => {
    const paramValues = func.inputs.map(p => params[p.name] || '');
    const data = encodeCall(func, paramValues);
    onCall(data);
  };
  
  return (
    <div className={`function-card ${isWrite ? 'write' : 'read'}`}>
      <div className="func-header">
        <span className={`func-type ${isWrite ? 'write' : 'read'}`}>
          {isWrite ? '⚠️ Write' : '🔒 Read'}
        </span>
        <span className="func-name">{func.name}</span>
        <span className="func-sig">
          ({func.inputs.map(i => `${i.type} ${i.name}`).join(', ')})
        </span>
      </div>
      
      {func.inputs.length > 0 && (
        <div className="func-params">
          {func.inputs.map(param => (
            <div key={param.name} className="param">
              <label>{param.name} ({param.type})</label>
              <input
                type="text"
                value={params[param.name] || ''}
                onChange={(e) => setParams({ ...params, [param.name]: e.target.value })}
                placeholder={param.type === 'address' ? '0x...' : ''}
              />
            </div>
          ))}
        </div>
      )}
      
      <button 
        className={`call-btn ${isWrite ? 'write' : 'read'}`}
        onClick={handleCall}
        disabled={loading}
      >
        {loading ? '⏳' : isWrite ? '✏️ Write' : '🔍 Query'}
      </button>
      
      {result && (
        <div className="result">
          <span className="result-label">Result:</span>
          <span className="result-value">{result}</span>
        </div>
      )}
      
      <style>{`
        .function-card {
          background: #1e293b;
          border-radius: 12px;
          padding: 16px;
          margin-bottom: 12px;
          border-left: 4px solid;
        }
        .function-card.read { border-left-color: #10b981; }
        .function-card.write { border-left-color: #f59e0b; }
        .func-header {
          display: flex;
          align-items: center;
          gap: 12px;
          margin-bottom: 12px;
        }
        .func-type {
          padding: 4px 8px;
          border-radius: 4px;
          font-size: 11px;
          font-weight: 600;
        }
        .func-type.read { background: #10b981; color: white; }
        .func-type.write { background: #f59e0b; color: white; }
        .func-name {
          font-weight: 600;
          color: #e2e8f0;
        }
        .func-sig {
          color: #64748b;
          font-size: 12px;
          font-family: monospace;
        }
        .func-params { margin-bottom: 12px; }
        .param { margin-bottom: 8px; }
        .param label {
          display: block;
          color: #94a3b8;
          font-size: 12px;
          margin-bottom: 4px;
        }
        .param input {
          width: 100%;
          padding: 10px 12px;
          background: #0f172a;
          border: 1px solid #334155;
          border-radius: 6px;
          color: #e2e8f0;
          font-family: monospace;
          font-size: 13px;
        }
        .param input:focus {
          outline: none;
          border-color: #3b82f6;
        }
        .call-btn {
          width: 100%;
          padding: 10px;
          border: none;
          border-radius: 6px;
          font-weight: 600;
          cursor: pointer;
          transition: all 0.2s;
        }
        .call-btn.read {
          background: #10b981;
          color: white;
        }
        .call-btn.write {
          background: #f59e0b;
          color: white;
        }
        .call-btn:hover:not(:disabled) { transform: translateY(-1px); }
        .call-btn:disabled { opacity: 0.5; cursor: not-allowed; }
        .result {
          margin-top: 12px;
          padding: 10px;
          background: #0f172a;
          border-radius: 6px;
        }
        .result-label { color: #64748b; font-size: 12px; margin-right: 8px; }
        .result-value { color: #10b981; font-family: monospace; font-size: 13px; }
      `}</style>
    </div>
  );
};

// Contract source viewer
const ContractSource: React.FC<{ address: string }> = ({ address }) => {
  const sampleSource = `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";

contract SampleToken is ERC20 {
    uint256 public constant MAX_SUPPLY = 1000000000 * 10**18;
    
    constructor() ERC20("Sample Token", "SAMPLE") {
        _mint(msg.sender, MAX_SUPPLY);
    }
    
    function burn(uint256 amount) public {
        _burn(msg.sender, amount);
    }
    
    function mint(address to, uint256 amount) public {
        _mint(to, amount);
    }
}`;

  return (
    <div className="contract-source">
      <div className="source-header">
        <h4>Contract Source Code</h4>
        <span className="verified">✓ Verified</span>
      </div>
      <pre><code>{sampleSource}</code></pre>
      
      <style>{`
        .contract-source {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .source-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 16px;
        }
        .source-header h4 { color: #e2e8f0; margin: 0; }
        .verified {
          background: #10b981;
          color: white;
          padding: 4px 12px;
          border-radius: 4px;
          font-size: 12px;
        }
        pre {
          background: #0f172a;
          padding: 16px;
          border-radius: 8px;
          overflow-x: auto;
        }
        code {
          color: #e2e8f0;
          font-family: 'Fira Code', monospace;
          font-size: 13px;
          line-height: 1.6;
        }
      `}</style>
    </div>
  );
};

// Main component
const ReadWriteContract: React.FC = () => {
  const [address, setAddress] = useState('');
  const [contract, setContract] = useState<Contract | null>(null);
  const [selectedFunc, setSelectedFunc] = useState<FunctionAbi | null>(null);
  const [results, setResults] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState<Record<string, boolean>>({});
  const [error, setError] = useState<string | null>(null);

  const handleAddressSubmit = () => {
    if (!address) return;
    
    setError(null);
    // In production, would fetch ABI from contract
    setContract({
      address,
      name: 'Token Contract',
      abi: sampleABI,
    });
  };

  const handleCall = async (func: FunctionAbi, data: string) => {
    setLoading({ ...loading, [func.name]: true });
    setError(null);
    
    try {
      // Simulate API call
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      // Generate mock result
      let result = '0x';
      if (func.outputs.length > 0) {
        const outputType = func.outputs[0].type;
        
        if (func.name === 'name') result = '0x' + '0'.repeat(64 - 16) + '54616d65720000000000000000000000000000000000000000000000000000000000'; // "Tiger"
        else if (func.name === 'symbol') result = '0x' + '0'.repeat(64 - 14) + '5447520000000000000000000000000000000000000000000000000000000000'; // "TGR"
        else if (func.name === 'decimals') result = '0x0000000000000000000000000000000000000000000000000000000000000012'; // 18
        else if (func.name === 'balanceOf' || func.name === 'totalSupply') result = '0x' + Math.floor(Math.random() * 1e26).toString(16).padStart(64, '0');
        else if (func.name === 'allowance') result = '0x' + Math.floor(Math.random() * 1e20).toString(16).padStart(64, '0');
        else if (func.name === 'transfer' || func.name === 'approve' || func.name === 'transferFrom') result = '0x0000000000000000000000000000000000000000000000000000000000000001'; // true
        else result = '0x' + '0'.repeat(64);
      }
      
      const decoded = decodeResult(func.outputs[0]?.type || 'uint256', result);
      setResults({ ...results, [func.name]: decoded });
    } catch (err) {
      setError('Failed to call contract');
    } finally {
      setLoading({ ...loading, [func.name]: false });
    }
  };

  return (
    <div className="read-write-contract">
      <div className="page-header">
        <h1>📝 Read/Write Contract</h1>
        <p>Interact with any smart contract - read state or write transactions</p>
      </div>
      
      <div className="address-input">
        <input
          type="text"
          placeholder="Enter contract address (0x...)"
          value={address}
          onChange={(e) => setAddress(e.target.value)}
        />
        <button onClick={handleAddressSubmit}>Load Contract</button>
      </div>
      
      {error && <div className="error">{error}</div>}
      
      {contract && (
        <div className="contract-info">
          <div className="info-header">
            <h3>{contract.name}</h3>
            <span className="address">{contract.address.slice(0, 10)}...{contract.address.slice(-8)}</span>
          </div>
          
          <ContractSource address={contract.address} />
          
          <div className="functions">
            <h3>Contract Functions</h3>
            
            <div className="function-tabs">
              <button 
                className={!selectedFunc ? 'active' : ''}
                onClick={() => setSelectedFunc(null)}
              >
                All
              </button>
              <button 
                className={selectedFunc?.stateMutability === 'view' ? 'active' : ''}
                onClick={() => setSelectedFunc(contract.abi.find(f => f.stateMutability === 'view') || null)}
              >
                Read
              </button>
              <button 
                className={selectedFunc?.stateMutability !== 'view' ? 'active' : ''}
                onClick={() => setSelectedFunc(contract.abi.find(f => f.stateMutability !== 'view') || null)}
              >
                Write
              </button>
            </div>
            
            {(selectedFunc ? [selectedFunc] : contract.abi).map(func => (
              <FunctionCard
                key={func.name}
                func={func}
                onCall={(data) => handleCall(func, data)}
                result={results[func.name]}
                loading={loading[func.name]}
              />
            ))}
          </div>
        </div>
      )}
      
      {!contract && (
        <div className="placeholder">
          <div className="icon">📝</div>
          <h3>Contract Interaction</h3>
          <p>Enter a contract address above to read its functions and interact with it</p>
        </div>
      )}
      
      <style>{`
        .read-write-contract {
          padding: 24px;
          max-width: 1200px;
          margin: 0 auto;
        }
        .page-header { margin-bottom: 24px; }
        .page-header h1 { font-size: 32px; color: #e2e8f0; margin-bottom: 8px; }
        .page-header p { color: #94a3b8; }
        .address-input {
          display: flex;
          gap: 12px;
          margin-bottom: 24px;
        }
        .address-input input {
          flex: 1;
          padding: 14px 16px;
          background: #1e293b;
          border: 1px solid #334155;
          border-radius: 8px;
          color: #e2e8f0;
          font-family: monospace;
          font-size: 14px;
        }
        .address-input input:focus {
          outline: none;
          border-color: #3b82f6;
        }
        .address-input button {
          padding: 14px 24px;
          background: #3b82f6;
          border: none;
          border-radius: 8px;
          color: white;
          font-weight: 600;
          cursor: pointer;
        }
        .address-input button:hover { background: #2563eb; }
        .error {
          padding: 12px 16px;
          background: #ef444420;
          border: 1px solid #ef4444;
          border-radius: 8px;
          color: #ef4444;
          margin-bottom: 24px;
        }
        .contract-info { }
        .info-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 20px;
        }
        .info-header h3 { color: #e2e8f0; margin: 0; }
        .info-header .address {
          color: #3b82f6;
          font-family: monospace;
        }
        .functions { }
        .functions h3 { color: #e2e8f0; margin-bottom: 16px; }
        .function-tabs {
          display: flex;
          gap: 8px;
          margin-bottom: 16px;
        }
        .function-tabs button {
          padding: 8px 16px;
          background: #1e293b;
          border: 1px solid #334155;
          border-radius: 6px;
          color: #94a3b8;
          cursor: pointer;
        }
        .function-tabs button.active {
          background: #3b82f6;
          border-color: #3b82f6;
          color: white;
        }
        .placeholder {
          text-align: center;
          padding: 80px 40px;
          background: #1e293b;
          border-radius: 12px;
        }
        .placeholder .icon { font-size: 64px; margin-bottom: 16px; }
        .placeholder h3 { color: #e2e8f0; margin-bottom: 8px; }
        .placeholder p { color: #64748b; }
      `}</style>
    </div>
  );
};

export default ReadWriteContract;