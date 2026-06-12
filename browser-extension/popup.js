/**
 * TigerScan Browser Extension - Popup Script
 */

document.addEventListener('DOMContentLoaded', async () => {
  const API_BASE = 'https://api.tigerscan.io/api/v2';
  
  // Elements
  const blockHeightEl = document.getElementById('blockHeight');
  const tpsEl = document.getElementById('tps');
  const addressesEl = document.getElementById('addresses');
  const transactionsEl = document.getElementById('transactions');
  const gasSlowEl = document.getElementById('gasSlow');
  const gasStandardEl = document.getElementById('gasStandard');
  const gasFastEl = document.getElementById('gasFast');
  const searchInput = document.getElementById('searchInput') as HTMLInputElement;
  const searchBtn = document.getElementById('searchBtn');
  
  // Format number
  function formatNumber(num: number): string {
    if (num >= 1000000) {
      return (num / 1000000).toFixed(1) + 'M';
    }
    if (num >= 1000) {
      return (num / 1000).toFixed(1) + 'K';
    }
    return num.toString();
  }
  
  // Load network stats
  async function loadStats() {
    try {
      // Get network stats
      const statsRes = await fetch(`${API_BASE}/stats`);
      const statsData = await statsRes.json();
      
      if (statsData.status === '1' && statsData.result) {
        const stats = statsData.result;
        
        if (blockHeightEl) {
          blockHeightEl.textContent = formatNumber(parseInt(stats.blockNumber));
        }
        if (tpsEl) {
          tpsEl.textContent = parseFloat(stats.tps).toFixed(1);
        }
        if (addressesEl) {
          addressesEl.textContent = formatNumber(parseInt(stats.totalAddresses));
        }
        if (transactionsEl) {
          transactionsEl.textContent = formatNumber(parseInt(stats.totalTransactions));
        }
      }
      
      // Get gas price
      const gasRes = await fetch(`${API_BASE}/stats/gas`);
      const gasData = await gasRes.json();
      
      if (gasData.status === '1' && gasData.result) {
        const gas = gasData.result;
        
        if (gasSlowEl) {
          gasSlowEl.textContent = gas.slow + ' Gwei';
        }
        if (gasStandardEl) {
          gasStandardEl.textContent = gas.standard + ' Gwei';
        }
        if (gasFastEl) {
          gasFastEl.textContent = gas.fast + ' Gwei';
        }
      }
    } catch (error) {
      console.error('Error loading stats:', error);
      
      // Show fallback values
      if (blockHeightEl) blockHeightEl.textContent = 'Error';
      if (tpsEl) tpsEl.textContent = 'Error';
      if (gasSlowEl) gasSlowEl.textContent = 'Error';
    }
  }
  
  // Handle search
  function handleSearch() {
    const query = searchInput?.value?.trim();
    if (!query) {
      return;
    }
    
    let url = '';
    
    // Determine search type
    if (query.startsWith('0x')) {
      if (query.length === 42) {
        // Address
        url = `https://tigerscan.io/address/${query}`;
      } else if (query.length === 66) {
        // Transaction hash
        url = `https://tigerscan.io/tx/${query}`;
      } else {
        url = `https://tigerscan.io/search?q=${encodeURIComponent(query)}`;
      }
    } else if (!isNaN(Number(query))) {
      // Block number
      url = `https://tigerscan.io/block/${query}`;
    } else {
      url = `https://tigerscan.io/search?q=${encodeURIComponent(query)}`;
    }
    
    // Open in new tab
    chrome.tabs.create({ url, active: true });
  }
  
  // Event listeners
  if (searchBtn) {
    searchBtn.addEventListener('click', handleSearch);
  }
  
  if (searchInput) {
    searchInput.addEventListener('keypress', (e) => {
      if (e.key === 'Enter') {
        handleSearch();
      }
    });
  }
  
  // Initial load
  loadStats();
  
  // Refresh every 30 seconds
  setInterval(loadStats, 30000);
});