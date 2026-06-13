/**
 * TigerSmartChain Browser Extension - Background Service Worker
 * Handles: API communication, alerts, notifications, data sync
 */

// ============================================
// Configuration
// ============================================

const CONFIG = {
  API_BASE: 'https://api.tigersmartchain.com',
  WS_ENDPOINT: 'wss://api.tigersmartchain.com/ws',
  PRICE_UPDATE_INTERVAL: 60000, // 1 minute
  ALERT_CHECK_INTERVAL: 300000, // 5 minutes
  MAX_STORAGE_ITEMS: 1000,
};

// ============================================
// State
// ============================================

let wsConnection = null;
let priceCache = new Map();
let alertCache = new Map();
let txCache = new Map();
let settings = {
  alertsEnabled: true,
  priceAlerts: true,
  whaleAlerts: true,
  securityAlerts: true,
  autoRefresh: true,
};

// ============================================
// Initialization
// ============================================

async function init() {
  console.log('[TigerSmartChain] Initializing extension...');
  
  // Load settings from storage
  await loadSettings();
  
  // Start connections
  connectWebSocket();
  startAlarms();
  
  // Setup message listeners
  chrome.runtime.onMessage.addListener(handleMessage);
  
  console.log('[TigerSmartChain] Extension initialized');
}

async function loadSettings() {
  const result = await chrome.storage.local.get('settings');
  if (result.settings) {
    settings = { ...settings, ...result.settings };
  }
}

// ============================================
// WebSocket Connection
// ============================================

function connectWebSocket() {
  if (wsConnection) {
    wsConnection.close();
  }
  
  try {
    wsConnection = new WebSocket(CONFIG.WS_ENDPOINT);
    
    wsConnection.onopen = () => {
      console.log('[TigerSmartChain] WebSocket connected');
      // Subscribe to updates
      sendWSMessage({ type: 'subscribe', channels: ['prices', 'alerts', 'transactions'] });
    };
    
    wsConnection.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        handleWSMessage(data);
      } catch (e) {
        console.error('[TigerSmartChain] Failed to parse message:', e);
      }
    };
    
    wsConnection.onclose = () => {
      console.log('[TigerSmartChain] WebSocket disconnected, reconnecting...');
      setTimeout(connectWebSocket, 5000);
    };
    
    wsConnection.onerror = (error) => {
      console.error('[TigerSmartChain] WebSocket error:', error);
    };
  } catch (e) {
    console.error('[TigerSmartChain] Failed to connect WebSocket:', e);
  }
}

function sendWSMessage(message) {
  if (wsConnection && wsConnection.readyState === WebSocket.OPEN) {
    wsConnection.send(JSON.stringify(message));
  }
}

function handleWSMessage(data) {
  switch (data.type) {
    case 'price_update':
      handlePriceUpdate(data);
      break;
    case 'alert':
      handleAlert(data);
      break;
    case 'transaction':
      handleTransaction(data);
      break;
    case 'block':
      handleBlock(data);
      break;
  }
}

// ============================================
// Price Updates
// ============================================

async function handlePriceUpdate(data) {
  const { token, price, change24h, volume24h } = data;
  
  priceCache.set(token, {
    price,
    change24h,
    volume24h,
    timestamp: Date.now(),
  });
  
  // Check for price alerts
  if (settings.priceAlerts) {
    await checkPriceAlerts(token, price);
  }
  
  // Update badge
  updateBadge();
}

async function checkPriceAlerts(token, price) {
  const result = await chrome.storage.local.get('priceAlerts');
  const alerts = result.priceAlerts || [];
  
  for (const alert of alerts) {
    if (alert.token !== token) continue;
    
    const shouldTrigger = 
      (alert.condition === 'above' && price > alert.threshold) ||
      (alert.condition === 'below' && price < alert.threshold);
    
    if (shouldTrigger) {
      // Show notification
      chrome.notifications.create({
        type: 'basic',
        iconUrl: 'assets/icon128.png',
        title: `Price Alert: ${token}`,
        message: `${token} is now $${price.toFixed(2)} (${alert.condition} $${alert.threshold})`,
      });
    }
  }
}

// ============================================
// Alerts
// ============================================

async function handleAlert(data) {
  if (!settings.alertsEnabled || !settings.securityAlerts) {
    return;
  }
  
  const alert = {
    id: data.id || crypto.randomUUID(),
    type: data.type,
    severity: data.severity,
    title: data.title,
    description: data.description,
    address: data.address,
    timestamp: Date.now(),
    read: false,
  };
  
  alertCache.set(alert.id, alert);
  
  // Show notification for critical/high severity
  if (data.severity === 'critical' || data.severity === 'high') {
    chrome.notifications.create({
      type: 'basic',
      iconUrl: 'assets/icon128.png',
      title: `Security Alert: ${data.title}`,
      message: data.description,
    });
  }
  
  // Update badge
  updateBadge();
}

// ============================================
// Transactions
// ============================================

async function handleTransaction(data) {
  const tx = {
    hash: data.hash,
    from: data.from,
    to: data.to,
    value: data.value,
    status: data.status,
    timestamp: Date.now(),
  };
  
  txCache.set(tx.hash, tx);
  
  // Check for whale alerts
  if (settings.whaleAlerts) {
    await checkWhaleAlerts(tx);
  }
}

async function checkWhaleAlerts(tx) {
  const result = await chrome.storage.local.get('whaleThreshold');
  const threshold = result.whaleThreshold || 1000000; // Default 1M
  
  if (parseFloat(tx.value) > threshold) {
    chrome.notifications.create({
      type: 'basic',
      iconUrl: 'assets/icon128.png',
      title: 'Whale Transaction Detected',
      message: `Large transaction: ${tx.value} ETH to ${tx.to}`,
    });
  }
}

// ============================================
// Blocks
// ============================================

function handleBlock(data) {
  // Could trigger notifications for new blocks
}

// ============================================
// Badge
// ============================================

function updateBadge() {
  let count = alertCache.size;
  
  if (count > 0) {
    chrome.action.setBadgeText({ text: count.toString() });
    chrome.action.setBadgeBackgroundColor({ color: '#ef4444' });
  } else {
    chrome.action.setBadgeText({ text: '' });
  }
}

// ============================================
// Alarms
// ============================================

function startAlarms() {
  // Price check every minute
  chrome.alarms.create('price-check', {
    periodInMinutes: 1,
  });
  
  // Alert check every 5 minutes
  chrome.alarms.create('alert-check', {
    periodInMinutes: 5,
  });
}

chrome.alarms.onAlarm.addListener(async (alarm) => {
  if (alarm.name === 'price-check') {
    await fetchPrices();
  } else if (alarm.name === 'alert-check') {
    await fetchAlerts();
  }
});

// ============================================
// API Calls
// ============================================

async function fetchPrices() {
  try {
    const response = await fetch(`${CONFIG.API_BASE}/prices`);
    const prices = await response.json();
    
    for (const [token, data] of Object.entries(prices)) {
      priceCache.set(token, { ...data, timestamp: Date.now() });
    }
  } catch (e) {
    console.error('[TigerSmartChain] Failed to fetch prices:', e);
  }
}

async function fetchAlerts() {
  try {
    const response = await fetch(`${CONFIG.API_BASE}/alerts?unread=true`);
    const alerts = await response.json();
    
    for (const alert of alerts) {
      if (!alertCache.has(alert.id)) {
        alertCache.set(alert.id, alert);
      }
    }
    
    updateBadge();
  } catch (e) {
    console.error('[TigerSmartChain] Failed to fetch alerts:', e);
  }
}

// ============================================
// Message Handler
// ============================================

async function handleMessage(message, sender, sendResponse) {
  switch (message.action) {
    case 'getPrices':
      sendResponse({ prices: Object.fromEntries(priceCache) });
      break;
    
    case 'getAlerts':
      const alerts = Array.from(alertCache.values());
      sendResponse({ alerts });
      break;
    
    case 'getTransactions':
      const txs = Array.from(txCache.values());
      sendResponse({ transactions: txs });
      break;
    
    case 'search':
      try {
        const results = await search(message.query);
        sendResponse({ results });
      } catch (e) {
        sendResponse({ error: e.message });
      }
      break;
    
    case 'getSettings':
      sendResponse({ settings });
      break;
    
    case 'saveSettings':
      settings = { ...settings, ...message.settings };
      await chrome.storage.local.set({ settings });
      sendResponse({ success: true });
      break;
    
    case 'openExplorer':
      chrome.tabs.create({ url: `${CONFIG.API_BASE}/explorer/${message.path}` });
      sendResponse({ success: true });
      break;
    
    case 'markAlertRead':
      const alert = alertCache.get(message.alertId);
      if (alert) {
        alert.read = true;
        alertCache.set(message.alertId, alert);
        updateBadge();
      }
      sendResponse({ success: true });
      break;
    
    default:
      sendResponse({ error: 'Unknown action' });
  }
  
  return true;
}

// ============================================
// Search
// ============================================

async function search(query) {
  const results = [];
  
  // Check if it's an address
  if (/^0x[a-fA-F0-9]{40}$/.test(query)) {
    results.push({ type: 'address', value: query });
  }
  
  // Check if it's a transaction hash
  if (/^0x[a-fA-F0-9]{64}$/.test(query)) {
    results.push({ type: 'transaction', value: query });
  }
  
  // Check if it's a block number
  if (/^\d+$/.test(query)) {
    results.push({ type: 'block', value: query });
  }
  
  // API search
  try {
    const response = await fetch(`${CONFIG.API_BASE}/search?q=${encodeURIComponent(query)}`);
    const apiResults = await response.json();
    results.push(...apiResults);
  } catch (e) {
    console.error('[TigerSmartChain] Search failed:', e);
  }
  
  return results;
}

// ============================================
// Context Menus
// ============================================

chrome.contextMenus?.create({
  id: 'search-address',
  title: 'Search Address on TigerSmartChain',
  contexts: ['selection'],
});

chrome.contextMenus?.create({
  id: 'view-transaction',
  title: 'View Transaction',
  contexts: ['link'],
  targetUrlPatterns: ['*://etherscan.io/tx/*', '*://*.scan/*/tx/*'],
});

chrome.contextMenus?.onClicked.addListener((info, tab) => {
  if (info.menuItemId === 'search-address' && info.selectionText) {
    chrome.tabs.create({ url: `${CONFIG.API_BASE}/explorer/address/${info.selectionText}` });
  }
});

// ============================================
// Install/Update
// ============================================

chrome.runtime.onInstalled.addListener((details) => {
  if (details.reason === 'install') {
    console.log('[TigerSmartChain] Extension installed');
    // Open welcome page
    chrome.tabs.create({ url: 'src/welcome.html' });
  } else if (details.reason === 'update') {
    console.log('[TigerSmartChain] Extension updated');
  }
});

// Start
init();