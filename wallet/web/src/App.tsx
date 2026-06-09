// TigerSmartChain Web Wallet - Main Application
// A secure, industrial-grade web wallet for TigerSmartChain

import React, { useState, useEffect } from 'react';
import { useWallet } from './hooks/useWallet';
import { useNetwork } from './hooks/useNetwork';
import Header from './components/Header';
import WalletConnect from './components/WalletConnect';
import Dashboard from './components/Dashboard';
import SendTransaction from './components/SendTransaction';
import TokenBalance from './components/TokenBalance';
import TransactionHistory from './components/TransactionHistory';
import NetworkSelector from './components/NetworkSelector';
import './styles/App.css';

const App: React.FC = () => {
  const { account, connect, disconnect, isLocked } = useWallet();
  const { network, switchNetwork } = useNetwork();
  const [activeTab, setActiveTab] = useState<string>('dashboard');

  // Initialize wallet on mount
  useEffect(() => {
    // Check for existing session
    const storedAccount = localStorage.getItem('tigersmartchain_account');
    if (storedAccount) {
      // Auto-connect if session exists
      connect();
    }
  }, []);

  // Render wallet connection screen if not connected
  if (!account || isLocked) {
    return (
      <div className="app">
        <Header />
        <main className="app-main">
          <NetworkSelector />
          <WalletConnect onConnect={connect} />
        </main>
        <footer className="app-footer">
          <p>© 2024 TigerSmartChain. Industrial-Grade Security.</p>
        </footer>
      </div>
    );
  }

  // Render main dashboard
  return (
    <div className="app">
      <Header account={account} onDisconnect={disconnect} />
      <main className="app-main">
        <NetworkSelector />
        <div className="wallet-container">
          <div className="wallet-tabs">
            <button
              className={`tab ${activeTab === 'dashboard' ? 'active' : ''}`}
              onClick={() => setActiveTab('dashboard')}
            >
              Dashboard
            </button>
            <button
              className={`tab ${activeTab === 'send' ? 'active' : ''}`}
              onClick={() => setActiveTab('send')}
            >
              Send
            </button>
            <button
              className={`tab ${activeTab === 'tokens' ? 'active' : ''}`}
              onClick={() => setActiveTab('tokens')}
            >
              Tokens
            </button>
            <button
              className={`tab ${activeTab === 'history' ? 'active' : ''}`}
              onClick={() => setActiveTab('history')}
            >
              History
            </button>
          </div>
          <div className="wallet-content">
            {activeTab === 'dashboard' && <Dashboard />}
            {activeTab === 'send' && <SendTransaction />}
            {activeTab === 'tokens' && <TokenBalance />}
            {activeTab === 'history' && <TransactionHistory />}
          </div>
        </div>
      </main>
      <footer className="app-footer">
        <p>© 2024 TigerSmartChain. Industrial-Grade Security.</p>
      </footer>
    </div>
  );
};

export default App;