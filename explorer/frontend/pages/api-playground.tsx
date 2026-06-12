// API Playground Page - Interactive API testing
import { useState } from 'react';
import Header from '../components/Header';

interface Endpoint {
  method: string;
  path: string;
  description: string;
  params?: string[];
  example?: string;
}

const endpoints: Endpoint[] = [
  { method: 'GET', path: '/blocks/latest', description: 'Get latest block', example: '' },
  { method: 'GET', path: '/blocks/:number', description: 'Get block by number', params: ['number'], example: '/blocks/15432891' },
  { method: 'GET', path: '/transactions/:hash', description: 'Get transaction', params: ['hash'], example: '/transactions/0x1234...' },
  { method: 'GET', path: '/accounts/:address', description: 'Get account details', params: ['address'], example: '/accounts/0x1234...' },
  { method: 'GET', path: '/tokens/:address', description: 'Get token details', params: ['address'], example: '/tokens/0x...' },
  { method: 'GET', path: '/tokens/:address/holders', description: 'Get token holders', params: ['address'] },
  { method: 'GET', path: '/nfts/:address', description: 'Get NFT collection', params: ['address'] },
  { method: 'GET', path: '/charts/tvl', description: 'Get TVL chart data', params: ['timeRange'] },
  { method: 'GET', path: '/gas/predictions', description: 'Get gas predictions', params: ['horizon'] },
  { method: 'GET', path: '/dex/pairs', description: 'Get DEX pairs' },
  { method: 'GET', path: '/search', description: 'Search', params: ['q'] },
];

export default function APIPlaygroundPage() {
  const [selectedEndpoint, setSelectedEndpoint] = useState<Endpoint>(endpoints[0]);
  const [params, setParams] = useState<Record<string, string>>({});
  const [response, setResponse] = useState<any>(null);
  const [loading, setLoading] = useState(false);
  const [apiKey, setApiKey] = useState('');

  const handleExecute = async () => {
    setLoading(true);
    try {
      let url = `${process.env.NEXT_PUBLIC_API_URL || 'https://api.tigerscan.io/v1'}${selectedEndpoint.path}`;
      
      // Replace params in path
      Object.entries(params).forEach(([key, value]) => {
        url = url.replace(`:${key}`, value);
      });

      const res = await fetch(url, {
        method: selectedEndpoint.method,
        headers: {
          'Content-Type': 'application/json',
          ...(apiKey ? { 'Authorization': `Bearer ${apiKey}` } : {}),
        },
      });

      const data = await res.json();
      setResponse({
        status: res.status,
        data: data,
      });
    } catch (error) {
      setResponse({
        status: 500,
        error: String(error),
      });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-gray-50">
      <Header />
      
      <main className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">API Playground</h1>
          <p className="mt-2 text-gray-600">
            Test and explore the TigerScan API interactively
          </p>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
          {/* Sidebar - Endpoints */}
          <div className="bg-white rounded-xl shadow-sm border border-gray-200">
            <div className="px-6 py-4 border-b border-gray-200">
              <h2 className="font-semibold text-gray-900">Endpoints</h2>
            </div>
            <div className="p-4 max-h-[600px] overflow-y-auto">
              <div className="space-y-1">
                {endpoints.map((ep) => (
                  <button
                    key={ep.path}
                    onClick={() => {
                      setSelectedEndpoint(ep);
                      setResponse(null);
                    }}
                    className={`w-full p-3 rounded-lg text-left transition-colors ${
                      selectedEndpoint.path === ep.path
                        ? 'bg-blue-600 text-white'
                        : 'hover:bg-gray-50'
                    }`}
                  >
                    <div className="flex items-center gap-2">
                      <span className={`text-xs font-bold px-2 py-0.5 rounded ${
                        ep.method === 'GET' ? 'bg-green-100 text-green-800' :
                        ep.method === 'POST' ? 'bg-blue-100 text-blue-800' :
                        'bg-yellow-100 text-yellow-800'
                      }`}>
                        {ep.method}
                      </span>
                      <span className={`text-sm ${selectedEndpoint.path === ep.path ? 'text-white' : 'text-gray-900'}`}>
                        {ep.path}
                      </span>
                    </div>
                    <div className={`text-sm mt-1 ${selectedEndpoint.path === ep.path ? 'text-blue-200' : 'text-gray-500'}`}>
                      {ep.description}
                    </div>
                  </button>
                ))}
              </div>
            </div>
          </div>

          {/* Main - Request & Response */}
          <div className="lg:col-span-2 space-y-6">
            {/* Request Panel */}
            <div className="bg-white rounded-xl shadow-sm border border-gray-200">
              <div className="px-6 py-4 border-b border-gray-200">
                <h2 className="font-semibold text-gray-900">Request</h2>
              </div>
              <div className="p-6 space-y-4">
                {/* API Key */}
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-2">API Key (optional)</label>
                  <input
                    type="password"
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder="Enter your API key..."
                    className="w-full px-4 py-2 border border-gray-300 rounded-lg"
                  />
                </div>

                {/* Parameters */}
                {selectedEndpoint.params && selectedEndpoint.params.length > 0 && (
                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-2">Parameters</label>
                    {selectedEndpoint.params.map((param) => (
                      <div key={param} className="mb-2">
                        <input
                          type="text"
                          value={params[param] || ''}
                          onChange={(e) => setParams({ ...params, [param]: e.target.value })}
                          placeholder={param}
                          className="w-full px-4 py-2 border border-gray-300 rounded-lg"
                        />
                      </div>
                    ))}
                  </div>
                )}

                {/* Execute Button */}
                <button
                  onClick={handleExecute}
                  disabled={loading}
                  className="w-full py-3 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700 disabled:opacity-50"
                >
                  {loading ? 'Loading...' : `Execute ${selectedEndpoint.method}`}
                </button>
              </div>
            </div>

            {/* Response Panel */}
            <div className="bg-white rounded-xl shadow-sm border border-gray-200">
              <div className="px-6 py-4 border-b border-gray-200 flex items-center justify-between">
                <h2 className="font-semibold text-gray-900">Response</h2>
                {response && (
                  <span className={`px-3 py-1 rounded-full text-sm font-medium ${
                    response.status < 300 ? 'bg-green-100 text-green-800' :
                    response.status < 400 ? 'bg-yellow-100 text-yellow-800' :
                    'bg-red-100 text-red-800'
                  }`}>
                    {response.status}
                  </span>
                )}
              </div>
              <div className="p-6">
                {response ? (
                  <pre className="bg-gray-900 text-green-400 p-4 rounded-lg overflow-x-auto text-sm max-h-96">
                    {JSON.stringify(response.data || response.error, null, 2)}
                  </pre>
                ) : (
                  <div className="text-center py-12 text-gray-500">
                    Click "Execute" to see the response
                  </div>
                )}
              </div>
            </div>

            {/* cURL Example */}
            <div className="bg-gray-900 rounded-xl p-6">
              <h3 className="text-gray-400 font-medium mb-2">cURL</h3>
              <code className="text-green-400 text-sm">
                curl -X {selectedEndpoint.method} "https://api.tigerscan.io/v1{selectedEndpoint.example || selectedEndpoint.path.replace('/:param', '/value')}" \
                -H "Authorization: Bearer YOUR_API_KEY"
              </code>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}