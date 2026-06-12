// Gas Calculator Page - Interactive gas cost calculator
import { useState, useEffect } from 'react';
import Header from '../components/Header';

interface GasCalculation {
  operation: string;
  gasUsed: number;
  gasPrice: number;
  totalCost: number;
}

const operations = [
  { name: 'ETH Transfer', baseGas: 21000, description: 'Standard ETH transfer' },
  { name: 'ERC-20 Transfer', baseGas: 65000, description: 'Token transfer (estimate)' },
  { name: 'NFT Mint', baseGas: 85000, description: 'ERC-721 mint (estimate)' },
  { name: 'Swap', baseGas: 150000, description: 'Uniswap V2 swap' },
  { name: 'Add Liquidity', baseGas: 180000, description: 'Add to liquidity pool' },
  { name: 'Contract Deploy', baseGas: 1500000, description: 'Deploy new contract' },
  { name: 'ERC-20 Approval', baseGas: 46000, description: 'Approve token spending' },
  { name: 'NFT Transfer', baseGas: 55000, description: 'Transfer ERC-721 token' },
  { name: 'Staking Deposit', baseGas: 90000, description: 'Stake tokens' },
  { name: 'Unstaking', baseGas: 95000, description: 'Unstake tokens' },
  { name: 'Governance Vote', baseGas: 70000, description: 'Cast governance vote' },
  { name: 'Bridge Transfer', baseGas: 200000, description: 'Cross-chain bridge' },
];

export default function GasCalculatorPage() {
  const [selectedOp, setSelectedOp] = useState(operations[0]);
  const [gasLimit, setGasLimit] = useState(21000);
  const [gasPrice, setGasPrice] = useState(25); // gwei
  const [ethPrice, setEthPrice] = useState(2450); // USD
  const [result, setResult] = useState<GasCalculation | null>(null);
  const [history, setHistory] = useState<GasCalculation[]>([]);

  useEffect(() => {
    calculate();
  }, [selectedOp, gasLimit, gasPrice, ethPrice]);

  const calculate = () => {
    const totalCost = (gasLimit * gasPrice * 1e9) / 1e18 * ethPrice;
    setResult({
      operation: selectedOp.name,
      gasUsed: gasLimit,
      gasPrice: gasPrice,
      totalCost: totalCost,
    });
  };

  const formatUSD = (value: number) => {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
    }).format(value);
  };

  const handleOperationSelect = (op: typeof operations[0]) => {
    setSelectedOp(op);
    setGasLimit(op.baseGas);
  };

  const handleCustomGasLimit = (value: number) => {
    setGasLimit(Math.max(21000, Math.min(30000000, value)));
  };

  return (
    <div className="min-h-screen bg-gray-50">
      <Header />
      
      <main className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">Gas Calculator</h1>
          <p className="mt-2 text-gray-600">
            Calculate transaction costs and optimize gas usage
          </p>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
          {/* Left Panel - Operations */}
          <div className="bg-white rounded-xl shadow-sm border border-gray-200">
            <div className="px-6 py-4 border-b border-gray-200">
              <h2 className="text-lg font-semibold text-gray-900">Select Operation</h2>
            </div>
            <div className="p-4 grid grid-cols-2 gap-2 max-h-96 overflow-y-auto">
              {operations.map((op) => (
                <button
                  key={op.name}
                  onClick={() => handleOperationSelect(op)}
                  className={`p-3 rounded-lg text-left transition-colors ${
                    selectedOp.name === op.name
                      ? 'bg-blue-600 text-white'
                      : 'bg-gray-50 hover:bg-gray-100'
                  }`}
                >
                  <div className={`font-medium ${selectedOp.name === op.name ? 'text-white' : 'text-gray-900'}`}>
                    {op.name}
                  </div>
                  <div className={`text-sm ${selectedOp.name === op.name ? 'text-blue-200' : 'text-gray-500'}`}>
                    ~{op.baseGas.toLocaleString()} gas
                  </div>
                </button>
              ))}
            </div>
          </div>

          {/* Right Panel - Calculator */}
          <div className="space-y-6">
            {/* Gas Price Slider */}
            <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
              <div className="flex items-center justify-between mb-4">
                <label className="font-medium text-gray-900">Gas Price</label>
                <span className="text-lg font-bold text-blue-600">{gasPrice} gwei</span>
              </div>
              <input
                type="range"
                min="1"
                max="200"
                value={gasPrice}
                onChange={(e) => setGasPrice(parseInt(e.target.value))}
                className="w-full h-2 bg-gray-200 rounded-lg appearance-none cursor-pointer"
              />
              <div className="flex justify-between mt-2 text-sm text-gray-500">
                <span>Slow (1 gwei)</span>
                <span>Fast (200 gwei)</span>
              </div>
              
              {/* Quick buttons */}
              <div className="flex gap-2 mt-4">
                {[
                  { label: 'Slow', value: 10 },
                  { label: 'Normal', value: 25 },
                  { label: 'Fast', value: 50 },
                  { label: 'Instant', value: 100 },
                ].map((preset) => (
                  <button
                    key={preset.value}
                    onClick={() => setGasPrice(preset.value)}
                    className={`flex-1 py-2 rounded-lg text-sm font-medium ${
                      gasPrice === preset.value
                        ? 'bg-blue-600 text-white'
                        : 'bg-gray-100 text-gray-700 hover:bg-gray-200'
                    }`}
                  >
                    {preset.label}
                  </button>
                ))}
              </div>
            </div>

            {/* ETH Price */}
            <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
              <label className="font-medium text-gray-900 block mb-2">ETH Price (USD)</label>
              <input
                type="number"
                value={ethPrice}
                onChange={(e) => setEthPrice(parseInt(e.target.value) || 0)}
                className="w-full px-4 py-2 border border-gray-300 rounded-lg"
              />
            </div>

            {/* Custom Gas Limit */}
            <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
              <label className="font-medium text-gray-900 block mb-2">Custom Gas Limit</label>
              <div className="flex gap-2">
                <input
                  type="number"
                  value={gasLimit}
                  onChange={(e) => handleCustomGasLimit(parseInt(e.target.value) || 21000)}
                  className="flex-1 px-4 py-2 border border-gray-300 rounded-lg"
                />
                <button
                  onClick={calculate}
                  className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
                >
                  Calculate
                </button>
              </div>
            </div>

            {/* Result */}
            {result && (
              <div className="bg-gradient-to-br from-blue-600 to-blue-800 rounded-xl p-6 text-white">
                <div className="text-blue-200 mb-2">Estimated Cost</div>
                <div className="text-4xl font-bold mb-4">{formatUSD(result.totalCost)}</div>
                <div className="grid grid-cols-2 gap-4 text-sm">
                  <div>
                    <div className="text-blue-200">Gas Limit</div>
                    <div className="font-semibold">{result.gasUsed.toLocaleString()}</div>
                  </div>
                  <div>
                    <div className="text-blue-200">Gas Price</div>
                    <div className="font-semibold">{result.gasPrice} gwei</div>
                  </div>
                </div>
                <div className="mt-4 pt-4 border-t border-blue-500">
                  <div className="text-blue-200 text-sm">In ETH</div>
                  <div className="font-semibold">
                    {(result.gasUsed * result.gasPrice / 1e9).toFixed(6)} ETH
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Tips */}
        <div className="mt-8 bg-yellow-50 border border-yellow-200 rounded-xl p-6">
          <h3 className="font-semibold text-yellow-800 mb-2">💡 Gas Saving Tips</h3>
          <ul className="space-y-2 text-yellow-700">
            <li>• <strong>Off-peak hours:</strong> Send transactions during low network activity (nights/weekends)</li>
            <li>• <strong>Set gas limit:</strong> Only use what you need + 20% buffer</li>
            <li>• <strong>Batch operations:</strong> Combine multiple operations when possible</li>
            <li>• <strong>Layer 2:</strong> Consider Arbitrum or Optimism for cheaper transactions</li>
          </ul>
        </div>
      </main>
    </div>
  );
}