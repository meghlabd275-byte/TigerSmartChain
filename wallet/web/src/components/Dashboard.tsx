// Dashboard Component - TigerSmartChain Web Wallet

import React from 'react';
import { useWallet } from '../hooks/useWallet';
import { useNetwork } from '../hooks/useNetwork';
import './Dashboard.css';

const Dashboard: React.FC = () => {
  const { account, balance } = useWallet();
  const { getExplorerUrl } = useNetwork();

  const formatBalance = (bal: string): string => {
    try {
      const value = parseFloat(bal) / 1e18;
      return value.toFixed(6);
    } catch {
      return '0';
    }
  };

  return (
    <div className="dashboard">
      <div className="balance-card">
        <h3>Total Balance</h3>
        <div className="balance-amount">
          <span className="balance-value">{formatBalance(balance)}</span>
          <span className="balance-symbol">TIGER</span>
        </div>
        <div className="balance-usd">
          ≈ ${(parseFloat(formatBalance(balance)) * 0.0).toFixed(2)} USD
        </div>
      </div>

      <div className="quick-actions">
        <button className="action-button send">
          <span className="action-icon">↑</span>
          <span>Send</span>
        </button>
        <button className="action-button receive">
          <span className="action-icon">↓</span>
          <span>Receive</span>
        </button>
        <button className="action-button swap">
          <span className="action-icon">⇄</span>
          <span>Swap</span>
        </button>
        <button className="action-button stake">
          <span className="action-icon">◈</span>
          <span>Stake</span>
        </button>
      </div>

      <div className="account-info">
        <div className="info-row">
          <span className="info-label">Address</span>
          <a 
            href={account ? getExplorerUrl(account) : '#'} 
            className="info-value address"
          >
            {account?.slice(0, 10)}...{account?.slice(-8)}
          </a>
        </div>
        <div className="info-row">
          <span className="info-label">Network</span>
          <span className="info-value">TigerSmartChain</span>
        </div>
      </div>
    </div>
  );
};

export default Dashboard;