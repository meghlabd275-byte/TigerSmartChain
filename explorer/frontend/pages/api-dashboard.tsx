// API Dashboard - API usage stats and key management
import { useState, useEffect } from 'react';
import Header from '../components/Header';

interface APIKey {
  key: string;
  name: string;
  rateLimit: number;
  requestsToday: number;
  createdAt: string;
  expiresAt: string;
}

interface UsageStats {
  requestsToday: number;
  requestsLimit: number;
  rateLimit: number;
  endpoints: { name: string; count: number }[];
}

export default function APIDashboardPage() {
  const [keys, setKeys] = useState<APIKey[]>([]);
  const [usage, setUsage] = useState<UsageStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [newKeyName, setNewKeyName] = useState('');

  useEffect(() => {
    loadAPIKeys();
    loadUsage();
  }, []);

  const loadAPIKeys = async () => {
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL || ''}/pro/keys`, {
        headers: { 'Authorization': `Bearer ${localStorage.getItem('api_key')}` }
      });
      const data = await response.json();
      if (data.result) {
        setKeys(data.result);
      }
    } catch (error) {
      console.error('Failed to load API keys:', error);
    } finally {
      setLoading(false);
    }
  };

  const loadUsage = async () => {
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL || ''}/pro/usage`, {
        headers: { 'Authorization': `Bearer ${localStorage.getItem('api_key')}` }
      });
      const data = await response.json();
      if (data.result) {
        setUsage(data.result);
      }
    } catch (error) {
      console.error('Failed to load usage:', error);
    }
  };

  const handleCreateKey = async () => {
    if (!newKeyName) return;
    
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL || ''}/pro/keys`, {
        method: 'POST',
        headers: { 
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${localStorage.getItem('api_key')}`
        },
        body: JSON.stringify({ name: newKeyName }),
      });
      
      const data = await response.json();
      if (data.result) {
        setShowCreateModal(false);
        setNewKeyName('');
        loadAPIKeys();
        alert('API Key created: ' + data.result.key);
      }
    } catch (error) {
      console.error('Failed to create key:', error);
    }
  };

  const handleDeleteKey = async (key: string) => {
    if (!confirm('Are you sure you want to delete this API key?')) return;
    
    try {
      await fetch(`${process.env.NEXT_PUBLIC_API_URL || ''}/pro/keys/${key}`, {
        method: 'DELETE',
        headers: { 'Authorization': `Bearer ${localStorage.getItem('api_key')}` }
      });
      loadAPIKeys();
    } catch (error) {
      console.error('Failed to delete key:', error);
    }
  };

  const truncateKey = (key: string) => key.slice(0, 8) + '...' + key.slice(-4);

  return (
    <div className="min-h-screen bg-gray-50">
      <Header />
      
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="flex items-center justify-between mb-8">
          <div>
            <h1 className="text-3xl font-bold text-gray-900">API Dashboard</h1>
            <p className="mt-2 text-gray-600">Manage your API keys and monitor usage</p>
          </div>
          <button
            onClick={() => setShowCreateModal(true)}
            className="px-4 py-2 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700"
          >
            Create API Key
          </button>
        </div>

        {/* Usage Stats */}
        {usage && (
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-8">
            <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
              <div className="text-sm text-gray-500 mb-1">Requests Today</div>
              <div className="text-2xl font-bold text-gray-900">
                {usage.requestsToday.toLocaleString()}
              </div>
              <div className="mt-2 h-2 bg-gray-100 rounded-full overflow-hidden">
                <div 
                  className="h-full bg-blue-600 rounded-full"
                  style={{ width: `${(usage.requestsToday / usage.requestsLimit) * 100}%` }}
                />
              </div>
              <div className="text-xs text-gray-500 mt-1">
                of {usage.requestsLimit.toLocaleString()} daily limit
              </div>
            </div>
            
            <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
              <div className="text-sm text-gray-500 mb-1">Rate Limit</div>
              <div className="text-2xl font-bold text-gray-900">{usage.rateLimit}</div>
              <div className="text-xs text-gray-500 mt-1">requests/minute</div>
            </div>
            
            <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
              <div className="text-sm text-gray-500 mb-1">API Keys</div>
              <div className="text-2xl font-bold text-gray-900">{keys.length}</div>
              <div className="text-xs text-gray-500 mt-1">active keys</div>
            </div>
            
            <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
              <div className="text-sm text-gray-500 mb-1">Endpoints</div>
              <div className="text-2xl font-bold text-gray-900">{usage.endpoints.length}</div>
              <div className="text-xs text-gray-500 mt-1">in use</div>
            </div>
          </div>
        )}

        {/* Top Endpoints */}
        {usage && usage.endpoints.length > 0 && (
          <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6 mb-8">
            <h2 className="text-lg font-semibold text-gray-900 mb-4">Top Endpoints</h2>
            <div className="space-y-3">
              {usage.endpoints.map((endpoint, index) => (
                <div key={endpoint.name} className="flex items-center">
                  <span className="w-6 text-gray-400">{index + 1}</span>
                  <span className="flex-1 font-mono text-sm text-gray-700">{endpoint.name}</span>
                  <span className="text-gray-900">{endpoint.count.toLocaleString()}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* API Keys Table */}
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden">
          <div className="px-6 py-4 border-b border-gray-200">
            <h2 className="text-lg font-semibold text-gray-900">API Keys</h2>
          </div>
          
          {loading ? (
            <div className="p-12 flex justify-center">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600"></div>
            </div>
          ) : keys.length > 0 ? (
            <table className="w-full">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Name</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Key</th>
                  <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase">Rate Limit</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Created</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Expires</th>
                  <th className="px-6 py-3 text-center text-xs font-medium text-gray-500 uppercase">Action</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200">
                {keys.map(key => (
                  <tr key={key.key} className="hover:bg-gray-50">
                    <td className="px-6 py-4 font-medium text-gray-900">{key.name}</td>
                    <td className="px-6 py-4 font-mono text-sm">{truncateKey(key.key)}</td>
                    <td className="px-6 py-4 text-right">{key.rateLimit}/min</td>
                    <td className="px-6 py-4 text-gray-500">
                      {new Date(key.createdAt).toLocaleDateString()}
                    </td>
                    <td className="px-6 py-4 text-gray-500">
                      {key.expiresAt ? new Date(key.expiresAt).toLocaleDateString() : 'Never'}
                    </td>
                    <td className="px-6 py-4 text-center">
                      <button
                        onClick={() => handleDeleteKey(key.key)}
                        className="text-red-600 hover:text-red-800"
                      >
                        Delete
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <div className="p-12 text-center text-gray-500">
              No API keys found. Create one to get started.
            </div>
          )}
        </div>

        {/* Create Modal */}
        {showCreateModal && (
          <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
            <div className="bg-white rounded-xl shadow-xl p-6 w-full max-w-md">
              <h3 className="text-lg font-semibold text-gray-900 mb-4">Create API Key</h3>
              <input
                type="text"
                placeholder="Key name (e.g., My App)"
                value={newKeyName}
                onChange={(e) => setNewKeyName(e.target.value)}
                className="w-full px-4 py-2 border border-gray-300 rounded-lg mb-4"
              />
              <div className="flex gap-3">
                <button
                  onClick={handleCreateKey}
                  className="flex-1 px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
                >
                  Create
                </button>
                <button
                  onClick={() => setShowCreateModal(false)}
                  className="flex-1 px-4 py-2 border border-gray-300 rounded-lg hover:bg-gray-50"
                >
                  Cancel
                </button>
              </div>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}