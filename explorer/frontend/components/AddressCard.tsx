// AddressCard.tsx - Component for displaying address information
import React, { useState, useEffect } from 'react';
import { useExplorer } from '../hooks/useExplorer';

interface AddressCardProps {
  address: string;
}

interface AddressInfo {
  balance: string;
  transactionCount: number;
  tokenCount: number;
  firstSeen: number;
  lastSeen: number;
}

export const AddressCard: React.FC<AddressCardProps> = ({ address }) => {
  const { getAddressInfo } = useExplorer();
  const [info, setInfo] = useState<AddressInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchInfo = async () => {
      try {
        setLoading(true);
        const data = await getAddressInfo(address);
        setInfo(data);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to fetch');
      } finally {
        setLoading(false);
      }
    };

    if (address) {
      fetchInfo();
    }
  }, [address, getAddressInfo]);

  const formatBalance = (balance: string): string => {
    const num = parseFloat(balance);
    if (isNaN(num)) return '0';
    if (num >= 1e9) return `${(num / 1e9).toFixed(2)}B`;
    if (num >= 1e6) return `${(num / 1e6).toFixed(2)}M`;
    if (num >= 1e3) return `${(num / 1e3).toFixed(2)}K`;
    return num.toFixed(4);
  };

  const formatDate = (timestamp: number): string => {
    return new Date(timestamp * 1000).toLocaleDateString();
  };

  const shortenAddress = (addr: string): string => {
    if (!addr) return '';
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
  };

  if (loading) {
    return (
      <div className="address-card loading">
        <div className="spinner"></div>
        <p>Loading address info...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="address-card error">
        <p className="error-message">Error: {error}</p>
      </div>
    );
  }

  if (!info) {
    return (
      <div className="address-card empty">
        <p>No information available</p>
      </div>
    );
  }

  return (
    <div className="address-card">
      <div className="card-header">
        <h3>Address Details</h3>
      </div>
      
      <div className="card-body">
        <div className="address-row">
          <span className="label">Address:</span>
          <span className="value address">{shortenAddress(address)}</span>
        </div>
        
        <div className="balance-row">
          <span className="label">Balance:</span>
          <span className="value balance">{formatBalance(info.balance)} TSC</span>
        </div>
        
        <div className="stats-row">
          <div className="stat">
            <span className="label">Transactions:</span>
            <span className="value">{info.transactionCount}</span>
          </div>
          
          <div className="stat">
            <span className="label">Tokens:</span>
            <span className="value">{info.tokenCount}</span>
          </div>
        </div>
        
        <div className="time-row">
          <div className="stat">
            <span className="label">First Seen:</span>
            <span className="value">{formatDate(info.firstSeen)}</span>
          </div>
          
          <div className="stat">
            <span className="label">Last Seen:</span>
            <span className="value">{formatDate(info.lastSeen)}</span>
          </div>
        </div>
      </div>
    </div>
  );
};

export default AddressCard;