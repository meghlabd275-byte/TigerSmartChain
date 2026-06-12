// Documentation Page - API docs and guides
import { useState } from 'react';
import Header from '../components/Header';

interface DocSection {
  id: string;
  title: string;
  content: string;
}

const docSections: DocSection[] = [
  {
    id: 'getting-started',
    title: 'Getting Started',
    content: `Welcome to TigerScan API! This guide will help you get started with our API.

## Authentication
All API requests require an API key. You can create one in the API Dashboard.

## Base URL
All API requests should be made to:
\`\`\`
https://api.tigerscan.io/v1
\`\`\`

## Rate Limits
- Free tier: 5 requests/second
- Pro tier: 100 requests/second
- Enterprise: Custom limits`
  },
  {
    id: 'endpoints',
    title: 'API Endpoints',
    content: `## Blocks
\`GET /blocks\` - Get list of blocks
\`GET /blocks/:number\` - Get block by number
\`GET /blocks/latest\` - Get latest block

## Transactions
\`GET /transactions\` - Get list of transactions
\`GET /transactions/:hash\` - Get transaction by hash

## Accounts
\`GET /accounts/:address\` - Get account details
\`GET /accounts/:address/balance_history\` - Get balance history

## Tokens
\`GET /tokens\` - Get list of tokens
\`GET /tokens/:address\` - Get token details
\`GET /tokens/:address/holders\` - Get token holders`
  },
  {
    id: 'examples',
    title: 'Code Examples',
    content: `## cURL
\`\`\`bash
curl -X GET "https://api.tigerscan.io/v1/blocks/latest" \\
  -H "Authorization: Bearer YOUR_API_KEY"
\`\`\`

## JavaScript
\`\`\`javascript
const response = await fetch('https://api.tigerscan.io/v1/blocks/latest', {
  headers: {
    'Authorization': 'Bearer YOUR_API_KEY'
  }
});
const data = await response.json();
\`\`\`

## Python
\`\`\`python
import requests

response = requests.get(
  'https://api.tigerscan.io/v1/blocks/latest',
  headers={'Authorization': 'Bearer YOUR_API_KEY'}
)
data = response.json()
\`\`\``
  },
  {
    id: 'errors',
    title: 'Error Handling',
    content: `## Error Codes
| Code | Description |
|------|-------------|
| 400 | Bad Request - Invalid parameters |
| 401 | Unauthorized - Invalid API key |
| 403 | Forbidden - Rate limit exceeded |
| 404 | Not Found - Resource doesn't exist |
| 500 | Internal Server Error |

## Error Response Format
\`\`\`json
{
  "status": "error",
  "message": "Error description",
  "code": "ERROR_CODE"
}
\`\`\``
  },
  {
    id: 'webhooks',
    title: 'Webhooks',
    content: `## Setting Up Webhooks
Configure webhooks to receive real-time notifications for:
- New blocks
- Transaction confirmations
- Token transfers
- NFT events

## Webhook Payload
\`\`\`json
{
  "type": "new_block",
  "data": {
    "number": 12345678,
    "hash": "0x...",
    "timestamp": "2024-01-01T00:00:00Z"
  }
}
\`\`\``
  },
];

export default function DocsPage() {
  const [activeSection, setActiveSection] = useState('getting-started');

  return (
    <div className="min-h-screen bg-gray-50">
      <Header />
      
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">Documentation</h1>
          <p className="mt-2 text-gray-600">
            Complete API reference and guides for TigerScan
          </p>
        </div>

        <div className="flex gap-8">
          {/* Sidebar */}
          <div className="w-64 flex-shrink-0">
            <nav className="space-y-1">
              {docSections.map(section => (
                <button
                  key={section.id}
                  onClick={() => setActiveSection(section.id)}
                  className={`w-full text-left px-4 py-2 rounded-lg ${
                    activeSection === section.id
                      ? 'bg-blue-600 text-white'
                      : 'text-gray-700 hover:bg-gray-100'
                  }`}
                >
                  {section.title}
                </button>
              ))}
            </nav>
          </div>

          {/* Content */}
          <div className="flex-1">
            <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-8">
              {docSections.map(section => (
                <div key={section.id} className={activeSection === section.id ? 'block' : 'hidden'}>
                  <h2 className="text-2xl font-bold text-gray-900 mb-4">{section.title}</h2>
                  <pre className="bg-gray-900 text-green-400 p-4 rounded-lg overflow-x-auto whitespace-pre-wrap text-sm">
                    {section.content}
                  </pre>
                </div>
              ))}
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}