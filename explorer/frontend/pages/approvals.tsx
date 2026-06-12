// Token Approvals Page - Track token allowances
import { useState, useEffect } from 'react';
import Header from '../components/Header';

interface Approval {
  id: number;
  tokenAddress: string;
  owner: string;
  spender: string;
  value: string;
  isIncrease: boolean;
  timestamp: string;
  transactionHash: string;
}

interface Allowance {
  tokenAddress: string;
  owner: string;
  spender: string;
  value: string;
  lastUpdate: string;
}

export default function ApprovalsPage() {
  const [approvals, setApprovals] = useState<Approval[]>([]);
  const [allowances, setAllowances] = useState<Allowance[]>([]);
  const [loading, setLoading] = useState(true);
  const [activeTab, setActiveTab] = useState<'approvals' | 'allowances'>('approvals');
  const [filterSpender, setFilterSpender] = useState('');
  const [filterToken, setFilterToken] = useState('');

  useEffect(() => {
    if (activeTab === 'approvals') {
      loadApprovals();
    } else {
      loadAllowances();
    }
  }, [activeTab, filterSpender, filterToken]);

  const loadApprovals = async () => {
    setLoading(true);
    try {
      let url = `${process.env.NEXT_PUBLIC_API_URL || ''}/tokens/approvals?limit=50`;
      if (filterSpender) url += `&spender=${filterSpender}`;
      
      const response = await fetch(url);
      const data = await response.json();
      
      if (data.result) {
        setApprovals(data.result);
      }
    } catch (error) {
      console.error('Failed to load approvals:', error);
    } finally {
      setLoading(false);
    }
  };

  const loadAllowances = async () => {
    setLoading(true);
    try {
      let url = `${process.env.NEXT_PUBLIC_API_URL || ''}/tokens/approvals?limit=50`;
      if (filterToken) url += `&tokenAddress=${filterToken}`;
      
      const response = await fetch(url);
      const data = await response.json();
      
      if (data.result) {
        // Convert approvals to allowances
        const uniqueAllowances = new Map<string, Allowance>();
        data.result.forEach((a: Approval) => {
          const key = `${a.tokenAddress}-${a.owner}-${a.spender}`;
          if (!uniqueAllowances.has(key) || new Date(a.timestamp) > new Date(uniqueAllowances.get(key)!.lastUpdate)) {
            uniqueAllowances.set(key, {
              tokenAddress: a.tokenAddress,
              owner: a.owner,
              spender: a.spender,
              value: a.value,
              lastUpdate: a.timestamp,
            });
          }
        });
        setAllowances(Array.from(uniqueAllowances.values()));
      }
    } catch (error) {
      console.error('Failed to load allowances:', error);
    } finally {
      setLoading(false);
    }
  };

  const handleRevoke = async (tokenAddress: string, owner: string, spender: string) => {
    if (!confirm('Are you sure you want to revoke this approval?')) return;
    
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL || ''}/tokens/revoke_approval`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ tokenAddress, owner, spender }),
      });
      
      const data = await response.json();
      if (data.status === 'ok') {
        alert('Revocation transaction submitted');
        loadAllowances();
      }
    } catch (error) {
      console.error('Failed to revoke:', error);
      alert('Failed to submit revocation');
    }
  };

  const formatValue = (value: string) => {
    try {
      const num = parseFloat(value);
      if (value.length > 18) {
        return new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 2 }).format(num / 1e18);
      }
      return new Intl.NumberFormat('en-US', { maximumFractionDigits: 6 }).format(num);
    } catch {
      return value;
    }
  };

  const truncateAddress = (addr: string) => {
    return addr.slice(0, 6) + '...' + addr.slice(-4);
  };

  return (
    <div className="min-h-screen bg-gray-50">
      <Header />
      
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">Token Approvals</h1>
          <p className="mt-2 text-gray-600">Track and manage token allowances</p>
        </div>

        {/* Filters */}
        <div className="flex gap-4 mb-6">
          <input
            type="text"
            placeholder="Filter by spender address"
            value={filterSpender}
            onChange={(e) => setFilterSpender(e.target.value)}
            className="flex-1 px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
          />
          <input
            type="text"
            placeholder="Filter by token address"
            value={filterToken}
            onChange={(e) => setFilterToken(e.target.value)}
            className="flex-1 px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
          />
        </div>

        {/* Tabs */}
        <div className="border-b border-gray-200 mb-6">
          <nav className="flex gap-4">
            <button
              onClick={() => setActiveTab('approvals')}
              className={`py-3 px-1 border-b-2 font-medium text-sm ${
                activeTab === 'approvals'
                  ? 'border-blue-600 text-blue-600'
                  : 'border-transparent text-gray-500 hover:text-gray-700'
              }`}
            >
              Approval History
            </button>
            <button
              onClick={() => setActiveTab('allowances')}
              className={`py-3 px-1 border-b-2 font-medium text-sm ${
                activeTab === 'allowances'
                  ? 'border-blue-600 text-blue-600'
                  : 'border-transparent text-gray-500 hover:text-gray-700'
              }`}
            >
              Active Allowances
            </button>
          </nav>
        </div>

        {/* Content */}
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden">
          {loading ? (
            <div className="p-12 flex justify-center">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600"></div>
            </div>
          ) : activeTab === 'approvals' ? (
            <table className="w-full">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Token</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500">Owner</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500">Spender</th>
                  <th className="px-6 py-3 text-right text-xs font-medium text-gray-500">Value</th>
                  <th className="px-6 py-3 text-center text-xs font-medium text-gray-500">Type</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500">Date</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200">
                {approvals.map((approval) => (
                  <tr key={approval.id} className="hover:bg-gray-50">
                    <td className="px-6 py-4">
                      <a href={`/token/${approval.tokenAddress}`} className="text-blue-600 hover:underline">
                        {truncateAddress(approval.tokenAddress)}
                      </a>
                    </td>
                    <td className="px-6 py-4 text-gray-900">{truncateAddress(approval.owner)}</td>
                    <td className="px-6 py-4">
                      <a href={`/address/${approval.spender}`} className="text-blue-600 hover:underline">
                        {truncateAddress(approval.spender)}
                      </a>
                    </td>
                    <td className="px-6 py-4 text-right font-mono">{formatValue(approval.value)}</td>
                    <td className="px-6 py-4 text-center">
                      <span className={`px-2 py-1 rounded-full text-xs font-medium ${
                        approval.isIncrease 
                          ? 'bg-green-100 text-green-800' 
                          : 'bg-red-100 text-red-800'
                      }`}>
                        {approval.isIncrease ? 'Approve' : 'Revoke'}
                      </span>
                    </td>
                    <td className="px-6 py-4 text-gray-500 text-sm">
                      {new Date(approval.timestamp).toLocaleDateString()}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <table className="w-full">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Token</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500">Owner</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500">Spender</th>
                  <th className="px-6 py-3 text-right text-xs font-medium text-gray-500">Allowance</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500">Last Update</th>
                  <th className="px-6 py-3 text-center text-xs font-medium text-gray-500">Action</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200">
                {allowances.map((allowance, index) => (
                  <tr key={index} className="hover:bg-gray-50">
                    <td className="px-6 py-4">
                      <a href={`/token/${allowance.tokenAddress}`} className="text-blue-600 hover:underline">
                        {truncateAddress(allowance.tokenAddress)}
                      </a>
                    </td>
                    <td className="px-6 py-4 text-gray-900">{truncateAddress(allowance.owner)}</td>
                    <td className="px-6 py-4">
                      <a href={`/address/${allowance.spender}`} className="text-blue-600 hover:underline">
                        {truncateAddress(allowance.spender)}
                      </a>
                    </td>
                    <td className="px-6 py-4 text-right font-mono text-red-600">
                      {formatValue(allowance.value)}
                    </td>
                    <td className="px-6 py-4 text-gray-500 text-sm">
                      {new Date(allowance.lastUpdate).toLocaleDateString()}
                    </td>
                    <td className="px-6 py-4 text-center">
                      <button
                        onClick={() => handleRevoke(allowance.tokenAddress, allowance.owner, allowance.spender)}
                        className="px-3 py-1 bg-red-600 text-white rounded hover:bg-red-700 text-sm"
                      >
                        Revoke
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}

          {!loading && ((activeTab === 'approvals' && approvals.length === 0) || 
            (activeTab === 'allowances' && allowances.length === 0)) && (
            <div className="p-12 text-center text-gray-500">
              No {activeTab === 'approvals' ? 'approvals' : 'allowances'} found
            </div>
          )}
        </div>

        {/* Warning Banner */}
        <div className="mt-6 bg-yellow-50 border border-yellow-200 rounded-lg p-4">
          <div className="flex">
            <div className="flex-shrink-0">
              <svg className="h-5 w-5 text-yellow-400" viewBox="0 0 20 20" fill="currentColor">
                <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clipRule="evenodd" />
              </svg>
            </div>
            <div className="ml-3">
              <h3 className="text-sm font-medium text-yellow-800">Security Warning</h3>
              <div className="mt-2 text-sm text-yellow-700">
                <p>Only approve tokens to contracts you trust. Review the spender address before approving.</p>
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}