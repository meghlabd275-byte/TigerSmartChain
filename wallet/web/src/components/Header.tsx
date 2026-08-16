// Header Component - TigerSmartChain Web Wallet

import React, { useState, useEffect, useCallback } from 'react';
import './Header.css';

type Theme = 'light' | 'dark';

function useTheme() {
  const [theme, setTheme] = useState<Theme>(() => {
    if (typeof window === 'undefined') return 'dark';
    const saved = localStorage.getItem('tsc-wallet-theme');
    return (saved as Theme) || 'dark';
  });

  useEffect(() => {
    const root = document.documentElement;
    root.classList.remove('light', 'dark');
    root.classList.add(theme);
    localStorage.setItem('tsc-wallet-theme', theme);
  }, [theme]);

  const toggleTheme = useCallback(() => {
    setTheme(prev => prev === 'light' ? 'dark' : 'light');
  }, []);

  return { theme, toggleTheme };
}

interface HeaderProps {
  account?: string | null;
  onDisconnect?: () => void;
}

const Header: React.FC<HeaderProps> = ({ account, onDisconnect }) => {
  const { theme, toggleTheme } = useTheme();

  const formatAddress = (addr: string): string => {
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
  };

  const copyAddress = () => {
    if (account) {
      navigator.clipboard.writeText(account);
    }
  };

  return (
    <header className="header">
      <div className="header-container">
        <div className="header-logo">
          <img 
            src="/logo.svg" 
            alt="TigerSmartChain" 
            className="logo-image"
          />
          <span className="logo-text">TigerSmartChain</span>
        </div>
        
        <div className="header-nav">
          <a href="/" className="nav-link">Explorer</a>
          <a href="/" className="nav-link">Staking</a>
          <a href="/" className="nav-link">Bridge</a>
          <a href="/" className="nav-link">Docs</a>
        </div>

        <button
          className="theme-toggle"
          onClick={toggleTheme}
          aria-label="Toggle theme"
          title={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
        >
          {theme === 'dark' ? '☀️' : '🌙'}
        </button>

        {account && (
          <div className="header-account">
            <button 
              className="account-button" 
              onClick={copyAddress}
              title="Click to copy address"
            >
              <span className="account-address">{formatAddress(account)}</span>
              <span className="account-status"></span>
            </button>
            <button 
              className="disconnect-button"
              onClick={onDisconnect}
            >
              Disconnect
            </button>
          </div>
        )}
      </div>
    </header>
  );
};

export default Header;