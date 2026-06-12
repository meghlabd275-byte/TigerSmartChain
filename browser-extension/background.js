/**
 * TigerScan Browser Extension - Background Service Worker
 */

// Constants
const API_BASE = 'https://api.tigerscan.io/api/v2';
const NETWORK = 'tiger';

// State
let apiKey: string | null = null;

// Initialize
chrome.runtime.onInstalled.addListener(() => {
  console.log('TigerScan Extension installed');
  loadSettings();
});

chrome.runtime.onStartup.addListener(() => {
  loadSettings();
});

// Load settings
async function loadSettings() {
  const result = await chrome.storage.local.get(['apiKey']);
  apiKey = result.apiKey || null;
}

// API helper
async function apiRequest(endpoint: string, params: Record<string, any> = {}) {
  const url = new URL(`${API_BASE}${endpoint}`);
  Object.entries(params).forEach(([k, v]) => url.searchParams.set(k, String(v)));
  
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };
  
  if (apiKey) {
    headers['X-API-Key'] = apiKey;
  }
  
  const response = await fetch(url.toString(), { headers });
  const data = await response.json();
  
  if (data.status !== '1') {
    throw new Error(data.message || 'API error');
  }
  
  return data.result;
}

// Context menu - Search address
chrome.contextMenus?.create({
  id: 'searchAddress',
  title: 'Search on TigerScan',
  contexts: ['page', 'selection', 'link'],
});

chrome.contextMenus?.onClicked.addListener(async (info, tab) => {
  let query = info.selectionText || info.linkUrl || '';
  
  // Clean up query
  query = query.trim();
  if (query.startsWith('0x')) {
    query = query.substring(0, 66); // Limit to 66 chars for addresses/hashes
  }
  
  if (query) {
    const searchUrl = `https://tigerscan.io/search?q=${encodeURIComponent(query)}`;
    chrome.tabs.create({ url: searchUrl, active: true });
  }
});

// Tab updates - Show page action for TigerScan
chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (changeInfo.status === 'complete' && tab.url?.includes('tigerscan.io')) {
    chrome.tabs.sendMessage(tabId, { action: 'pageReady' });
  }
});

// Messages from content script
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  switch (message.type) {
    case 'getAddressInfo':
      handleGetAddressInfo(message.address)
        .then(sendResponse)
        .catch(err => sendResponse({ error: err.message }));
      return true;
      
    case 'getTransactionInfo':
      handleGetTransactionInfo(message.hash)
        .then(sendResponse)
        .catch(err => sendResponse({ error: err.message }));
      return true;
      
    case 'getTokenInfo':
      handleGetTokenInfo(message.address)
        .then(sendResponse)
        .catch(err => sendResponse({ error: err.message }));
      return true;
      
    case 'lookupAddress':
      handleLookupAddress(message.address)
        .then(sendResponse)
        .catch(err => sendResponse({ error: err.message }));
      return true;
      
    case 'getNetworkStats':
      handleGetNetworkStats()
        .then(sendResponse)
        .catch(err => sendResponse({ error: err.message }));
      return true;
      
    case 'getGasPrice':
      handleGetGasPrice()
        .then(sendResponse)
        .catch(err => sendResponse({ error: err.message }));
      return true;
      
    case 'getBalance':
      handleGetBalance(message.address)
        .then(sendResponse)
        .catch(err => sendResponse({ error: err.message }));
      return true;
      
    default:
      sendResponse({ error: 'Unknown message type' });
  }
});

// API Handlers
async function handleGetAddressInfo(address: string) {
  const account = await apiRequest(`/account/${address}`);
  const tokens = await apiRequest(`/account/${address}/tokens`, { limit: 10 });
  
  return {
    address: account.address,
    balance: account.balance,
    transactionCount: account.transactionCount,
    tokens: tokens.slice(0, 5),
  };
}

async function handleGetTransactionInfo(hash: string) {
  const tx = await apiRequest(`/tx/${hash}`);
  const receipt = await apiRequest(`/tx/${hash}/receipt`);
  
  return {
    ...tx,
    receipt,
  };
}

async function handleGetTokenInfo(address: string) {
  return apiRequest(`/token/${address}`);
}

async function handleLookupAddress(address: string) {
  // Try to determine if it's an address, transaction, or block
  if (address.startsWith('0x')) {
    if (address.length === 42) {
      // Ethereum address
      return handleGetAddressInfo(address);
    } else if (address.length === 66) {
      // Transaction or block hash - try tx first
      try {
        return await handleGetTransactionInfo(address);
      } catch {
        // Try block
        try {
          const blockNum = parseInt(address, 16);
          return await apiRequest(`/block/${blockNum}`);
        } catch {
          return { error: 'Not found' };
        }
      }
    }
  }
  
  // Try search
  return apiRequest('/search', { q: address });
}

async function handleGetNetworkStats() {
  return apiRequest('/stats');
}

async function handleGetGasPrice() {
  return apiRequest('/stats/gas');
}

async function handleGetBalance(address: string) {
  const account = await apiRequest(`/account/${address}`);
  return {
    address: account.address,
    balance: account.balance,
  };
}

// Badge updates for notifications
async function updateBadge(count: number) {
  if (count > 0) {
    await chrome.action.setBadgeText({ text: String(count) });
    await chrome.action.setBadgeBackgroundColor({ color: '#F7931A' });
  } else {
    await chrome.action.setBadgeText({ text: '' });
  }
}

// Export for testing
export {};