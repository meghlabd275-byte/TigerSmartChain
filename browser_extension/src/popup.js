/**
 * TigerSmartChain Browser Extension - Popup Script
 */

// ============================================
// State
// ============================================

let currentTab = 'prices';
let prices = {};
let alerts = [];
let transactions = [];

// ============================================
// Initialization
// ============================================

document.addEventListener('DOMContentLoaded', async () => {
  // Load data
  await loadPrices();
  await loadAlerts();
  await loadTransactions();
  
  // Setup event listeners
  setupTabs();
  setupSearch();
  setupQuickActions();
  setupSettings();
});

// ============================================
// Data Loading
// ============================================

async function loadPrices() {
  try {
    const response = await chrome.runtime.sendMessage({ action: 'getPrices' });
    if (response && response.prices) {
      prices = response.prices;
      if (currentTab === 'prices') {
        renderPrices();
      }
    }
  } catch (e) {
    console.error('Failed to load prices:', e);
    // Use sample data
    prices = {
      ETH: { price: 3250.00, change24h: 2.5, volume24h: 15000000000 },
      BTC: { price: 67500.00, change24h: 1.2, volume24h: 28000000000 },
      BNB: { price: 580.00, change24h: -1.5, volume24h: 1200000000 },
      SOL: { price: 145.00, change24h: 5.8, volume24h: 2500000000 },
      MATIC: { price: 0.85, change24h: 3.2, volume24h: 850000000 },
      LINK: { price: 15.50, change24h: -0.8, volume24h: 650000000 },
      UNI: { price: 7.25, change24h: 4.5, volume24h: 320000000 },
      AAVE: { price: 95.00, change24h: 6.2, volume24h: 280000000 },
    };
    renderPrices();
  }
}

async function loadAlerts() {
  try {
    const response = await chrome.runtime.sendMessage({ action: 'getAlerts' });
    if (response && response.alerts) {
      alerts = response.alerts;
      if (currentTab === 'alerts') {
        renderAlerts();
      }
    }
  } catch (e) {
    console.error('Failed to load alerts:', e);
    // Use sample data
    alerts = [
      { id: '1', type: 'honeypot', severity: 'critical', title: 'Honeypot Detected', description: 'Contract traps users', timestamp: Date.now() - 300000 },
      { id: '2', type: 'phishing', severity: 'high', title: 'Phishing Site', description: 'Fake airdrop site', timestamp: Date.now() - 600000 },
    ];
    renderAlerts();
  }
}

async function loadTransactions() {
  try {
    const response = await chrome.runtime.sendMessage({ action: 'getTransactions' });
    if (response && response.transactions) {
      transactions = response.transactions;
      if (currentTab === 'transactions') {
        renderTransactions();
      }
    }
  } catch (e) {
    console.error('Failed to load transactions:', e);
    // Use sample data
    transactions = [
      { hash: '0x1234567890abcdef1234567890abcdef12345678', from: '0x742d35Cc6634C0532925a3b844Bc9e7595f12', to: '0x8ba1f109c0d31b80b3c2e2e2e2e2e2e2e2e2e2', value: '5.5', status: 'success' },
      { hash: '0xabcdef1234567890abcdef1234567890abcdef12', from: '0x8ba1f109c0d31b80b3c2e2e2e2e2e2e2e2e2', to: '0x742d35Cc6634C0532925a3b844Bc9e7595f12', value: '12.8', status: 'success' },
    ];
    renderTransactions();
  }
}

// ============================================
// Tab Switching
// ============================================

function setupTabs() {
  const tabs = document.querySelectorAll('.tab');
  
  tabs.forEach(tab => {
    tab.addEventListener('click', () => {
      tabs.forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
      currentTab = tab.dataset.tab;
      renderContent();
    });
  });
}

function renderContent() {
  switch (currentTab) {
    case 'prices':
      renderPrices();
      break;
    case 'alerts':
      renderAlerts();
      break;
    case 'transactions':
      renderTransactions();
      break;
    case 'stats':
      renderStats();
      break;
  }
}

// ============================================
// Render Functions
// ============================================

function renderPrices() {
  const content = document.getElementById('content');
  
  if (Object.keys(prices).length === 0) {
    content.innerHTML = '<div class="loading">No price data available</div>';
    return;
  }
  
  let html = '';
  
  for (const [token, data] of Object.entries(prices)) {
    const changeClass = data.change24h >= 0 ? 'positive' : 'negative';
    const changeSign = data.change24h >= 0 ? '+' : '';
    
    html += `
      <div class="price-item">
        <div class="price-token">${token}</div>
        <div class="price-value">
          <div class="price-usd">$${data.price.toLocaleString()}</div>
          <div class="price-change ${changeClass}">${changeSign}${data.change24h.toFixed(1)}%</div>
        </div>
      </div>
    `;
  }
  
  content.innerHTML = html;
}

function renderAlerts() {
  const content = document.getElementById('content');
  
  if (alerts.length === 0) {
    content.innerHTML = '<div class="loading">No alerts</div>';
    return;
  }
  
  let html = '';
  
  for (const alert of alerts) {
    const timeAgo = formatTimeAgo(alert.timestamp);
    
    html += `
      <div class="alert-item ${alert.severity}">
        <div class="alert-title">${alert.title}</div>
        <div class="alert-desc">${alert.description}</div>
        <div class="alert-time">${timeAgo}</div>
      </div>
    `;
  }
  
  content.innerHTML = html;
}

function renderTransactions() {
  const content = document.getElementById('content');
  
  if (transactions.length === 0) {
    content.innerHTML = '<div class="loading">No recent transactions</div>';
    return;
  }
  
  let html = '';
  
  for (const tx of transactions) {
    html += `
      <div class="tx-item">
        <div class="tx-hash">${tx.hash.slice(0, 18)}...</div>
        <div class="tx-info">
          <span>${tx.value} ETH</span>
          <span class="tx-status ${tx.status}">${tx.status}</span>
        </div>
      </div>
    `;
  }
  
  content.innerHTML = html;
}

function renderStats() {
  const content = document.getElementById('content');
  
  const totalValue = Object.values(prices).reduce((acc, p) => acc + (p.volume24h || 0), 0);
  const totalAlerts = alerts.filter(a => !a.read).length;
  const avgPrice = Object.values(prices).reduce((acc, p) => acc + p.price, 0) / Object.keys(prices).length;
  
  content.innerHTML = `
    <div class="stats-grid">
      <div class="stat-card">
        <div class="stat-value">$${(totalValue / 1e9).toFixed(1)}B</div>
        <div class="stat-label">24h Volume</div>
      </div>
      <div class="stat-card">
        <div class="stat-value">${totalAlerts}</div>
        <div class="stat-label">Unread Alerts</div>
      </div>
      <div class="stat-card">
        <div class="stat-value">$${avgPrice.toFixed(0)}</div>
        <div class="stat-label">Avg Token Price</div>
      </div>
      <div class="stat-card">
        <div class="stat-value">${Object.keys(prices).length}</div>
        <div class="stat-label">Tracked Tokens</div>
      </div>
    </div>
  `;
}

// ============================================
// Search
// ============================================

function setupSearch() {
  const searchInput = document.getElementById('searchInput');
  
  searchInput.addEventListener('keydown', async (e) => {
    if (e.key === 'Enter') {
      const query = searchInput.value.trim();
      if (query) {
        await performSearch(query);
      }
    }
  });
}

async function performSearch(query) {
  try {
    const response = await chrome.runtime.sendMessage({ 
      action: 'search', 
      query: query 
    });
    
    if (response && response.results && response.results.length > 0) {
      // Open first result
      const result = response.results[0];
      const url = getExplorerUrl(result.type, result.value);
      chrome.tabs.create({ url });
    } else {
      // Try general explorer search
      chrome.tabs.create({ url: `https://tigersmartchain.com/explorer/search?q=${encodeURIComponent(query)}` });
    }
  } catch (e) {
    console.error('Search failed:', e);
    chrome.tabs.create({ url: `https://tigersmartchain.com/explorer/search?q=${encodeURIComponent(query)}` });
  }
}

function getExplorerUrl(type, value) {
  switch (type) {
    case 'address':
      return `https://tigersmartchain.com/explorer/address/${value}`;
    case 'transaction':
      return `https://tigersmartchain.com/explorer/tx/${value}`;
    case 'block':
      return `https://tigersmartchain.com/explorer/block/${value}`;
    case 'token':
      return `https://tigersmartchain.com/explorer/token/${value}`;
    default:
      return `https://tigersmartchain.com/explorer/search?q=${value}`;
  }
}

// ============================================
// Quick Actions
// ============================================

function setupQuickActions() {
  const buttons = document.querySelectorAll('.action-btn');
  
  buttons.forEach(btn => {
    btn.addEventListener('click', async () => {
      const action = btn.dataset.action;
      
      switch (action) {
        case 'explorer':
          chrome.runtime.sendMessage({ action: 'openExplorer', path: '' });
          break;
        case 'dashboard':
          chrome.runtime.sendMessage({ action: 'openExplorer', path: 'dashboard' });
          break;
        case 'settings':
          chrome.runtime.sendMessage({ action: 'openExplorer', path: 'settings' });
          break;
      }
    });
  });
}

// ============================================
// Settings
// ============================================

function setupSettings() {
  const settingsBtn = document.getElementById('settingsBtn');
  
  settingsBtn.addEventListener('click', () => {
    chrome.runtime.sendMessage({ action: 'openExplorer', path: 'settings' });
  });
}

// ============================================
// Helper Functions
// ============================================

function formatTimeAgo(timestamp) {
  const diff = Date.now() - timestamp;
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(diff / 3600000);
  const days = Math.floor(diff / 86400000);
  
  if (minutes < 1) return 'Just now';
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  return `${days}d ago`;
}