// Tools Page - Bytecode decoder, ABI encoder, contract comparator
import { useState } from 'react';
import Header from '../components/Header';

export default function ToolsPage() {
  const [activeTool, setActiveTool] = useState('decoder');
  const [input, setInput] = useState('');
  const [output, setOutput] = useState('');
  const [loading, setLoading] = useState(false);

  const tools = [
    { id: 'decoder', label: 'Bytecode Decoder', description: 'Decode EVM bytecode to opcodes' },
    { id: 'encoder', label: 'ABI Encoder', description: 'Encode function calls' },
    { id: 'comparator', label: 'Contract Comparator', description: 'Compare two contracts' },
    { id: 'verifier', label: 'Contract Verifier', description: 'Verify contract source code' },
  ];

  const handleDecode = async () => {
    if (!input) return;
    setLoading(true);
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL || ''}/tools/decode_input`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ data: input }),
      });
      const data = await response.json();
      setOutput(data.result || data.message || 'Failed to decode');
    } catch (error) {
      setOutput('Error decoding bytecode');
    } finally {
      setLoading(false);
    }
  };

  const handleVerifyMessage = async () => {
    if (!input) return;
    setLoading(true);
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL || ''}/tools/verify_message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ message: input }),
      });
      const data = await response.json();
      setOutput(JSON.stringify(data, null, 2));
    } catch (error) {
      setOutput('Error verifying message');
    } finally {
      setLoading(false);
    }
  };

  const handleVerifySignature = async () => {
    if (!input) return;
    setLoading(true);
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL || ''}/tools/verify_signature`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ signature: input }),
      });
      const data = await response.json();
      setOutput(JSON.stringify(data, null, 2));
    } catch (error) {
      setOutput('Error verifying signature');
    } finally {
      setLoading(false);
    }
  };

  const handleExecute = () => {
    switch (activeTool) {
      case 'decoder':
        handleDecode();
        break;
      case 'encoder':
        handleDecode();
        break;
      case 'comparator':
        setOutput('Enter two contract addresses to compare');
        break;
      case 'verifier':
        handleVerifyMessage();
        break;
      default:
        handleDecode();
    }
  };

  return (
    <div className="min-h-screen bg-gray-50">
      <Header />
      
      <main className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">Developer Tools</h1>
          <p className="mt-2 text-gray-600">Utilities for smart contract development</p>
        </div>

        {/* Tool Selection */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
          {tools.map(tool => (
            <button
              key={tool.id}
              onClick={() => {
                setActiveTool(tool.id);
                setInput('');
                setOutput('');
              }}
              className={`p-4 rounded-lg border-2 text-left transition-all ${
                activeTool === tool.id
                  ? 'border-blue-600 bg-blue-50'
                  : 'border-gray-200 hover:border-gray-300'
              }`}
            >
              <div className="font-medium text-gray-900">{tool.label}</div>
              <div className="text-sm text-gray-500">{tool.description}</div>
            </button>
          ))}
        </div>

        {/* Input Area */}
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6 mb-6">
          <label className="block text-sm font-medium text-gray-700 mb-2">
            {activeTool === 'decoder' ? 'Bytecode' : 
             activeTool === 'encoder' ? 'Function Signature' :
             activeTool === 'comparator' ? 'Contract Address' : 'Message'}
          </label>
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={
              activeTool === 'decoder' ? '0x60806040523481...' :
              activeTool === 'encoder' ? 'transfer(address,uint256)' :
              activeTool === 'comparator' ? '0x...' : 'Hello World'
            }
            rows={6}
            className="w-full px-4 py-2 border border-gray-300 rounded-lg font-mono text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
          />
          
          <div className="mt-4 flex gap-3">
            <button
              onClick={handleExecute}
              disabled={loading || !input}
              className="px-6 py-2 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {loading ? 'Processing...' : 'Execute'}
            </button>
            <button
              onClick={() => {
                setInput('');
                setOutput('');
              }}
              className="px-6 py-2 border border-gray-300 text-gray-700 rounded-lg font-medium hover:bg-gray-50"
            >
              Clear
            </button>
          </div>
        </div>

        {/* Output Area */}
        <div className="bg-gray-900 rounded-xl shadow-sm p-6">
          <div className="flex items-center justify-between mb-2">
            <label className="text-sm font-medium text-gray-400">Output</label>
            {output && (
              <button
                onClick={() => navigator.clipboard.writeText(output)}
                className="text-sm text-gray-400 hover:text-white"
              >
                Copy
              </button>
            )}
          </div>
          <pre className="font-mono text-sm text-green-400 overflow-x-auto whitespace-pre-wrap">
            {output || 'Output will appear here...'}
          </pre>
        </div>

        {/* Quick Reference */}
        <div className="mt-8 bg-white rounded-xl shadow-sm border border-gray-200 p-6">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">Common Opcodes</h2>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
            {[
              { code: '0x00', name: 'STOP', desc: 'Halts execution' },
              { code: '0x01', name: 'ADD', desc: 'Addition' },
              { code: '0x02', name: 'MUL', desc: 'Multiplication' },
              { code: '0x10', name: 'SUB', desc: 'Subtraction' },
              { code: '0x11', name: 'DIV', desc: 'Division' },
              { code: '0x14', name: 'EQ', desc: 'Equality' },
              { code: '0x15', name: 'ISZERO', desc: 'Is Zero' },
              { code: '0x30', name: 'ADDRESS', desc: 'Get caller address' },
              { code: '0x31', name: 'BALANCE', desc: 'Get balance' },
              { code: '0x33', name: 'CALLER', desc: 'Get msg.sender' },
              { code: '0x34', name: 'CALLVALUE', desc: 'Get msg.value' },
              { code: '0x35', name: 'CALLDATASIZE', desc: 'Get calldata size' },
              { code: '0x36', name: 'CALLDATALOAD', desc: 'Load calldata' },
              { code: '0x3B', name: 'EXTCODESIZE', desc: 'Get code size' },
              { code: '0x3F', name: 'EXTCODEHASH', desc: 'Get code hash' },
              { code: '0x54', name: 'SLOAD', desc: 'Load from storage' },
              { code: '0x55', name: 'SSTORE', desc: 'Store to storage' },
              { code: '0xA0', name: 'LOG0', desc: 'Log event' },
              { code: '0xF1', name: 'CALL', desc: 'Call contract' },
              { code: '0xF2', name: 'STATICCALL', desc: 'Static call' },
              { code: '0xF3', name: 'DELEGATECALL', desc: 'Delegate call' },
              { code: '0xFA', name: 'CREATE', desc: 'Create contract' },
              { code: '0xFF', name: 'SUICIDE', desc: 'Self destruct' },
            ].map(op => (
              <div key={op.code} className="flex gap-2">
                <code className="text-blue-600 font-mono">{op.code}</code>
                <span className="text-gray-600">{op.name}</span>
              </div>
            ))}
          </div>
        </div>
      </main>
    </div>
  );
}