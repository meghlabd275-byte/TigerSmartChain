/**
 * TigerScan Browser Extension - Content Script
 */

(function() {
  'use strict';

  // TigerScan API configuration
  const API_BASE = 'https://api.tigerscan.io/api/v2';

  // Helper functions
  function isEthAddress(str: string): boolean {
    return /^0x[a-fA-F0-9]{40}$/.test(str);
  }

  function isTxHash(str: string): boolean {
    return /^0x[a-fA-F0-9]{64}$/.test(str);
  }

  function formatAddress(addr: string): string {
    return addr.substring(0, 6) + '...' + addr.substring(38);
  }

  function formatBalance(wei: string): string {
    const eth = parseFloat(wei) / 1e18;
    return eth.toFixed(4);
  }

  // Create tooltip element
  function createTooltip(x: number, y: number, content: string): HTMLElement {
    const tooltip = document.createElement('div');
    tooltip.className = 'tigerscan-tooltip';
    tooltip.style.cssText = `
      position: fixed;
      left: ${x + 15}px;
      top: ${y + 15}px;
      background: #fff;
      border: 1px solid #ddd;
      border-radius: 8px;
      padding: 12px;
      box-shadow: 0 4px 12px rgba(0,0,0,0.15);
      z-index: 999999;
      max-width: 300px;
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      font-size: 14px;
      color: #333;
    `;
    tooltip.innerHTML = content;
    return tooltip;
  }

  // Fetch address info from API
  async function fetchAddressInfo(address: string) {
    try {
      const response = await fetch(`${API_BASE}/account/${address}`, {
        headers: {
          'Content-Type': 'application/json',
        },
      });
      const data = await response.json();
      return data.result;
    } catch (error) {
      console.error('TigerScan: Error fetching address info:', error);
      return null;
    }
  }

  // Handle mouse events on addresses
  async function handleAddressHover(event: MouseEvent) {
    const target = event.target as HTMLElement;
    const text = target.textContent?.trim() || '';
    
    if (!isEthAddress(text)) {
      return;
    }
    
    // Remove existing tooltip
    const existing = document.querySelector('.tigerscan-tooltip');
    if (existing) {
      existing.remove();
    }
    
    // Show loading tooltip
    const loadingTooltip = createTooltip(
      event.clientX,
      event.clientY,
      '<div style="text-align:center;"><span>Loading...</span></div>'
    );
    document.body.appendChild(loadingTooltip);
    
    // Fetch info
    const info = await fetchAddressInfo(text);
    loadingTooltip.remove();
    
    if (!info) {
      return;
    }
    
    // Create info tooltip
    const content = `
      <div style="font-weight:bold; margin-bottom:8px; color:#F7931A;">
        TigerScan Address Info
      </div>
      <div style="margin-bottom:4px;">
        <span style="color:#666;">Address:</span>
        <span style="font-family:monospace;">${formatAddress(text)}</span>
      </div>
      <div style="margin-bottom:4px;">
        <span style="color:#666;">Balance:</span>
        <span>${formatBalance(info.balance)} TSC</span>
      </div>
      <div style="margin-bottom:8px;">
        <span style="color:#666;">Transactions:</span>
        <span>${parseInt(info.transactionCount).toLocaleString()}</span>
      </div>
      <a href="https://tigerscan.io/address/${text}" 
         target="_blank" 
         style="color:#F7931A;text-decoration:none;font-size:12px;">
        View on TigerScan →
      </a>
    `;
    
    const tooltip = createTooltip(event.clientX, event.clientY, content);
    document.body.appendChild(tooltip);
    
    // Position tooltip if off-screen
    const rect = tooltip.getBoundingClientRect();
    if (rect.right > window.innerWidth) {
      tooltip.style.left = `${event.clientX - rect.width - 15}px`;
    }
    if (rect.bottom > window.innerHeight) {
      tooltip.style.top = `${event.clientY - rect.height - 15}px`;
    }
    
    // Remove on mouseout
    target.addEventListener('mouseout', () => {
      tooltip.remove();
    }, { once: true });
  }

  // Add click handler for transactions
  function handleAddressClick(event: MouseEvent) {
    const target = event.target as HTMLElement;
    const text = target.textContent?.trim() || '';
    
    if (!isEthAddress(text)) {
      return;
    }
    
    // Open in new tab
    window.open(`https://tigerscan.io/address/${text}`, '_blank');
  }

  // Initialize
  function init() {
    console.log('TigerScan Content Script loaded');
    
    // Use event delegation for better performance
    document.addEventListener('mouseover', handleAddressHover, true);
    document.addEventListener('click', handleAddressClick, true);
  }

  // Wait for DOM
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }

  // Listen for messages from background
  chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message.action === 'pageReady') {
      console.log('TigerScan: Page ready');
    }
  });
})();