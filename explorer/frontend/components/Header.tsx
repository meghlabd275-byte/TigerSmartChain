// TigerScan Header Component
// The main header for TigerSmartChain Explorer

import React, { useState } from 'react';
import { Link } from 'react-router-dom';
import './Header.css';

interface HeaderProps {
  searchQuery?: string;
  onSearch?: (query: string) => void;
}

const Header: React.FC<HeaderProps> = ({ searchQuery = '', onSearch }) => {
  const [query, setQuery] = useState(searchQuery);
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (onSearch) {
      onSearch(query);
    }
  };

  return (
    <header className="header">
      <div className="header-container">
        <Link to="/" className="header-logo">
          <img 
            src="/images/logo.svg" 
            alt="TigerScan" 
            className="logo-image"
          />
          <span className="logo-text">TigerScan</span>
        </Link>

        <form className="header-search" onSubmit={handleSearch}>
          <input
            type="text"
            className="search-input"
            placeholder="Search by address / tx hash / block / token"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <button type="submit" className="search-button">
            <svg viewBox="0 0 24 24" width="20" height="20">
              <path fill="currentColor" d="M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 2.5 9.5 2.5S3 5.91 3 9.5 6.41 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z"/>
            </svg>
          </button>
        </form>

        <nav className="header-nav">
          <Link to="/" className="nav-link">Home</Link>
          <Link to="/blocks" className="nav-link">Blocks</Link>
          <Link to="/transactions" className="nav-link">Transactions</Link>
          <Link to="/tokens" className="nav-link">Tokens</Link>
          <Link to="/nfts" className="nav-link">NFTs</Link>
          <Link to="/validators" className="nav-link">Validators</Link>
          <Link to="/analytics" className="nav-link">Analytics</Link>
        </nav>

        <div className="header-actions">
          <button 
            className="network-switch"
            onClick={() => {}}
          >
            <span className="network-dot"></span>
            Mainnet
          </button>
          
          <button 
            className="mobile-menu-toggle"
            onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
          >
            <span></span>
            <span></span>
            <span></span>
          </button>
        </div>
      </div>
    </header>
  );
};

export default Header;