// GasTracker.tsx - Component for displaying real-time gas prices
import React, { useState, useEffect } from 'react';
import { useExplorer } from '../hooks/useExplorer';

interface GasPrice {
  low: number;
  medium: number;
  high: number;
  baseFee: number;
}

export const GasTracker: React.FC = () => {
  const { getGasPrice } = useExplorer();
  const [gasPrice, setGasPrice] = useState<GasPrice | null>(null);
  const [loading, setLoading] = useState(true);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [error, setError] = useState<string | null>(null);

  const fetchGasPrice = async () => {
    try {
      const price = await getGasPrice();
      setGasPrice(price);
      setLastUpdated(new Date());
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to fetch gas');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchGasPrice();
    
    // Refresh every 30 seconds
    const interval = setInterval(fetchGasPrice, 30000);
    return () => clearInterval(interval);
  }, [getGasPrice]);

  const formatGwei = (wei: number): string => {
    return (wei / 1e9).toFixed(2);
  };

  const getSloweLabel = (price: number): string => {
    if (price < 10) return 'Very Low';
    if (price < 20) return 'Low';
    if (price < 30) return 'Medium';
    if (price < 50) return 'High';
    return 'Very High';
  };

  const getColor = (tier: 'low' | 'medium' | 'high'): string => {
    if (!gasPrice) return '#6b7280';
    
    switch (tier) {
      case 'low': return '#22c55e';
      case 'medium': return '#eab308';
      case 'high': return '#ef4444';
      default: return '#6b7280';
    }
  };

  if (loading && !gasPrice) {
    return (
      <div className="gas-tracker loading">
        <div className="spinner"></div>
        <p>Loading gas prices...</p>
      </div>
    );
  }

  if (error && !gasPrice) {
    return (
      <div className="gas-tracker error">
        <p className="error-message">Error: {error}</p>
      </div>
    );
  }

  return (
    <div className="gas-tracker">
      <div className="card-header">
        <h3>Gas Tracker</h3>
        {lastUpdated && (
          <span className="last-updated">
            Updated: {lastUpdated.toLocaleTimeString()}
          </span>
        )}
      </div>
      
      <div className="card-body">
        <div className="gas-tiers">
          <div className="gas-tier low">
            <span className="tier-label">Slow</span>
            <span className="tier-price" style={{ color: getColor('low') }}>
              {formatGwei(gasPrice?.low || 0)} Gwei
            </span>
            <span className="tier-indicator">
              {getSloweLabel(gasPrice?.low || 0)}
            </span>
          </div>
          
          <div className="gas-tier medium">
            <span className="tier-label">Standard</span>
            <span className="tier-price" style={{ color: getColor('medium') }}>
              {formatGwei(gasPrice?.medium || 0)} Gwei
            </span>
            <span className="tier-indicator">
              {getSloweLabel(gasPrice?.medium || 0)}
            </span>
          </div>
          
          <div className="gas-tier high">
            <span className="tier-label">Fast</span>
            <span className="tier-price" style={{ color: getColor('high') }}>
              {formatGwei(gasPrice?.high || 0)} Gwei
            </span>
            <span className="tier-indicator">
              {getSloweLabel(gasPrice?.high || 0)}
            </span>
          </div>
        </div>
        
        {gasPrice?.baseFee && (
          <div className="base-fee">
            <span className="label">Base Fee:</span>
            <span className="value">{formatGwei(gasPrice.baseFee)} Gwei</span>
          </div>
        )}
        
        <button className="refresh-button" onClick={fetchGasPrice}>
          Refresh
        </button>
      </div>
    </div>
  );
};

export default GasTracker;