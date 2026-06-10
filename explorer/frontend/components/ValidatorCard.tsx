// ValidatorCard.tsx - Component for displaying validator information
import React, { useState, useEffect } from 'react';
import { useExplorer } from '../hooks/useExplorer';

interface ValidatorCardProps {
  validatorAddress: string;
}

interface ValidatorInfo {
  address: string;
  stake: string;
  delegators: number;
  commission: number;
  uptime: number;
  blocksProduced: number;
  blocksMissed: number;
  slashedCount: number;
  status: 'active' | 'jailed' | 'inactive';
  jailedUntil?: number;
}

export const ValidatorCard: React.FC<ValidatorCardProps> = ({ validatorAddress }) => {
  const { getValidatorInfo } = useExplorer();
  const [info, setInfo] = useState<ValidatorInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchInfo = async () => {
      try {
        setLoading(true);
        const data = await getValidatorInfo(validatorAddress);
        setInfo(data);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to fetch');
      } finally {
        setLoading(false);
      }
    };

    if (validatorAddress) {
      fetchInfo();
    }
  }, [validatorAddress, getValidatorInfo]);

  const formatStake = (stake: string): string => {
    const num = parseFloat(stake);
    if (isNaN(num)) return '0';
    return `${(num / 1e18).toFixed(2)} BNB`;
  };

  const formatUptime = (uptime: number): string => {
    return `${uptime.toFixed(2)}%`;
  };

  const shortenAddress = (addr: string): string => {
    if (!addr) return '';
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
  };

  const getStatusColor = (status: string): string => {
    switch (status) {
      case 'active': return 'green';
      case 'jailed': return 'red';
      case 'inactive': return 'gray';
      default: return 'gray';
    }
  };

  const getJailedTimeLeft = (jailedUntil?: number): string => {
    if (!jailedUntil) return '';
    const now = Math.floor(Date.now() / 1000);
    if (jailedUntil <= now) return 'Released';
    const hours = Math.floor((jailedUntil - now) / 3600);
    const minutes = Math.floor(((jailedUntil - now) % 3600) / 60);
    return `${hours}h ${minutes}m`;
  };

  if (loading) {
    return (
      <div className="validator-card loading">
        <div className="spinner"></div>
        <p>Loading validator info...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="validator-card error">
        <p className="error-message">Error: {error}</p>
      </div>
    );
  }

  if (!info) {
    return (
      <div className="validator-card empty">
        <p>No validator information available</p>
      </div>
    );
  }

  return (
    <div className="validator-card">
      <div className="card-header">
        <h3>Validator Details</h3>
        <span 
          className="status-badge" 
          style={{ backgroundColor: getStatusColor(info.status) }}
        >
          {info.status.toUpperCase()}
        </span>
      </div>
      
      <div className="card-body">
        <div className="address-row">
          <span className="label">Address:</span>
          <span className="value address">{shortenAddress(info.address)}</span>
        </div>
        
        <div className="stake-row">
          <span className="label">Stake:</span>
          <span className="value stake">{formatStake(info.stake)}</span>
        </div>
        
        <div className="stats-row">
          <div className="stat">
            <span className="label">Delegators:</span>
            <span className="value">{info.delegators}</span>
          </div>
          
          <div className="stat">
            <span className="label">Commission:</span>
            <span className="value">{info.commission}%</span>
          </div>
        </div>
        
        <div className="stats-row">
          <div className="stat">
            <span className="label">Uptime:</span>
            <span className="value">{formatUptime(info.uptime)}</span>
          </div>
          
          <div className="stat">
            <span className="label">Slash Count:</span>
            <span className="value">{info.slashedCount}</span>
          </div>
        </div>
        
        <div className="blocks-row">
          <div className="block-stat produced">
            <span className="label">Blocks Produced:</span>
            <span className="value">{info.blocksProduced}</span>
          </div>
          
          <div className="block-stat missed">
            <span className="label">Blocks Missed:</span>
            <span className="value">{info.blocksMissed}</span>
          </div>
        </div>
        
        {info.status === 'jailed' && info.jailedUntil && (
          <div className="jail-info">
            <span className="label">Jailed Until:</span>
            <span className="value">{getJailedTimeLeft(info.jailedUntil)}</span>
          </div>
        )}
      </div>
    </div>
  );
};

export default ValidatorCard;