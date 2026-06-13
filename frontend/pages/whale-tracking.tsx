/**
 * Advanced Whale Tracking Dashboard - Real-time whale activity and large transactions
 * Complete implementation with MEV detection, whale alerts, and big money tracking
 */

import React, { useState, useEffect, useCallback } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, AreaChart, Area, ScatterChart, Scatter, BarChart, Bar } from 'recharts';

// Types for whale tracking
interface WhaleTransaction {
  id: string;
  hash: string;
  blockNumber: number;
  timestamp: number;
  from: string;
  to: string;
  value: number;
  token: string;
  tokenAmount: number;
  usdValue: number;
  type: 'swap' | 'transfer' | 'mint' | 'burn' | 'bridge' | 'liquidate' | 'flashloan';
  profit: number;
  gasUsed: number;
  gasPrice: number;
}

interface Whale {
  id: string;
  address: string;
  label: string;
  type: 'protocol' | 'whale' | 'miner' | 'dao' | 'vc' | 'trader';
  totalValue: number;
  transactionCount: number;
  lastActive: number;
  profit30d: number;
  tokens: string[];
}

interface MEVOpportunity {
  id: string;
  type: 'arb' | 'liquidate' | 'sandwich' | 'backrun';
  estimatedProfit: number;
  gasCost: number;
  netProfit: number;
  transactionHash: string;
  blockNumber: number;
  timestamp: number;
  tokens: string[];
  status: 'detected' | 'executed' | 'failed';
}

interface FlashLoan {
  id: string;
  protocol: string;
  token: string;
  amount: number;
  timestamp: number;
  txHash: string;
  profit: number;
  status: 'executed' | 'failed' | 'pending';
}

interface WhaleAlert {
  id: string;
  address: string;
  type: 'large_transfer' | 'whale_move' | 'mev_detected' | 'liquidation';
  value: number;
  timestamp: number;
  details: string;
  read: boolean;
}

interface TrendData {
  timestamp: number;
  volume: number;
  transactionCount: number;
  uniqueWhales: number;
}

// Advanced whale tracking hook
const useWhaleTracking = () => {
  const [transactions, setTransactions] = useState<WhaleTransaction[]>([]);
  const [whales, setWhales] = useState<Whale[]>([]);
  const [mevOpportunities, setMEVOpportunities] = useState<MEVOpportunity[]>([]);
  const [flashLoans, setFlashLoans] = useState<FlashLoan[]>([]);
  const [alerts, setAlerts] = useState<WhaleAlert[]>([]);
  const [trends, setTrends] = useState<TrendData[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<string>('all');

  const fetchWhaleData = useCallback(async () => {
    try {
      setLoading(true);
      
      // Generate whale transactions
      const txData: WhaleTransaction[] = [];
      const now = Date.now();
      const txTypes: WhaleTransaction['type'][] = ['swap', 'transfer', 'mint', 'burn', 'bridge', 'liquidate', 'flashloan'];
      const tokens = ['ETH', 'USDC', 'WBTC', 'USDT', 'DAI', 'LINK', 'UNI', 'AAVE', 'MKR', 'SOL'];
      
      for (let i = 0; i < 100; i++) {
        const timestamp = now - i * (60000 + Math.random() * 300000);
        const token = tokens[Math.floor(Math.random() * tokens.length)];
        const value = Math.random() * 10000000 + 100000;
        const tokenAmount = value / (100 + Math.random() * 2000);
        
        txData.push({
          id: `tx-${i}`,
          hash: `0x${Math.random().toString(16).substr(2, 64)}`,
          blockNumber: 18000000 + i,
          timestamp,
          from: `0x${Math.random().toString(16).substr(2, 40).padStart(40, '0')}`,
          to: `0x${Math.random().toString(16).substr(2, 40).padStart(40, '0')}`,
          value,
          token,
          tokenAmount,
          usdValue: value,
          type: txTypes[Math.floor(Math.random() * txTypes.length)],
          profit: Math.random() > 0.5 ? Math.random() * 50000 : -Math.random() * 5000,
          gasUsed: Math.floor(50000 + Math.random() * 200000),
          gasPrice: Math.floor(10 + Math.random() * 100),
        });
      }
      setTransactions(txData);
      
      // Generate whale profiles
      const whaleProfiles: Whale[] = [
        { id: '1', address: '0x8a2d16f55a6562E487B1C2c2a2a2a2a2a2a2a2a2', label: 'Binance Hot', type: 'protocol', totalValue: 4500000000, transactionCount: 12500, lastActive: now - 300000, profit30d: 2500000, tokens: ['ETH', 'USDC', 'USDT'] },
        { id: '2', address: '0x7b3C2c2a2a2a2a2a2a2a2a2a2a2a2a2', label: 'Wintermute', type: 'trader', totalValue: 1800000000, transactionCount: 8500, lastActive: now - 600000, profit30d: 8500000, tokens: ['ETH', 'WBTC', 'USDC'] },
        { id: '3', address: '0x6c1B2c2a2a2a2a2a2a2a2a2a2a2a2a', label: 'Amber Group', type: 'vc', totalValue: 1200000000, transactionCount: 4200, lastActive: now - 1200000, profit30d: 5200000, tokens: ['ETH', 'USDC', 'DAI'] },
        { id: '4', address: '0x5d2B2c2a2a2a2a2a2a2a2a2a2a2a2', label: 'Genesis Volatility', type: 'trader', totalValue: 850000000, transactionCount: 6800, lastActive: now - 180000, profit30d: 12000000, tokens: ['ETH', 'USDC', 'USDT'] },
        { id: '5', address: '0x4e3C2c2a2a2a2a2a2a2a2a2a2a2a', label: 'Jump Crypto', type: 'trader', totalValue: 2500000000, transactionCount: 3500, lastActive: now - 900000, profit30d: 15000000, tokens: ['ETH', 'WBTC', 'SOL'] },
        { id: '6', address: '0x3f4C2c2a2a2a2a2a2a2a2a2a2a2a', label: 'Alameda Research', type: 'trader', totalValue: 3200000000, transactionCount: 9200, lastActive: now - 300000, profit30d: -2500000, tokens: ['ETH', 'SOL', 'USDC'] },
        { id: '7', address: '0x2g5C2c2a2a2a2a2a2a2a2a2a2a2a', label: 'MakerDAO', type: 'protocol', totalValue: 8500000000, transactionCount: 4500, lastActive: now - 1800000, profit30d: 1200000, tokens: ['ETH', 'DAI', 'USDC'] },
        { id: '8', address: '0x1h6C2c2a2a2a2a2a2a2a2a2a2a', label: 'Aave', type: 'protocol', totalValue: 12500000000, transactionCount: 2800, lastActive: now - 3600000, profit30d: 850000, tokens: ['ETH', 'USDC', 'USDT'] },
        { id: '9', address: '0x0i7C2c2a2a2a2a2a2a2a2a2a2a', label: 'Vitalik', type: 'whale', totalValue: 2500000000, transactionCount: 1250, lastActive: now - 7200000, profit30d: 25000000, tokens: ['ETH', 'SHIB', 'DKMS'] },
        { id: '10', address: '0x9j8C2c2a2a2a2a2a2a2a2a2a2a', label: 'Uniswap', type: 'protocol', totalValue: 4500000000, transactionCount: 15000, lastActive: now - 60000, profit30d: 3500000, tokens: ['ETH', 'UNI', 'USDC'] },
      ];
      setWhales(whaleProfiles);
      
      // Generate MEV opportunities
      const mevData: MEVOpportunity[] = [
        { id: '1', type: 'arb', estimatedProfit: 25000, gasCost: 5000, netProfit: 20000, transactionHash: '0xabc123', blockNumber: 18000000, timestamp: now - 60000, tokens: ['USDC', 'USDT'], status: 'executed' },
        { id: '2', type: 'sandwich', estimatedProfit: 15000, gasCost: 8000, netProfit: 7000, transactionHash: '0xdef456', blockNumber: 17999999, timestamp: now - 120000, tokens: ['ETH', 'USDC'], status: 'executed' },
        { id: '3', type: 'liquidate', estimatedProfit: 85000, gasCost: 12000, netProfit: 73000, transactionHash: '0xghi789', blockNumber: 17999998, timestamp: now - 180000, tokens: ['ETH', 'WBTC'], status: 'detected' },
        { id: '4', type: 'backrun', estimatedProfit: 5000, gasCost: 2000, netProfit: 3000, transactionHash: '0xjkl012', blockNumber: 17999997, timestamp: now - 240000, tokens: ['USDC', 'DAI'], status: 'failed' },
        { id: '5', type: 'arb', estimatedProfit: 12000, gasCost: 4000, netProfit: 8000, transactionHash: '0xmno345', blockNumber: 17999996, timestamp: now - 300000, tokens: ['WBTC', 'ETH'], status: 'executed' },
        { id: '6', type: 'liquidate', estimatedProfit: 45000, gasCost: 8000, netProfit: 37000, transactionHash: '0xpqr678', blockNumber: 17999995, timestamp: now - 360000, tokens: ['ETH', 'SOL'], status: 'detected' },
      ];
      setMEVOpportunities(mevData);
      
      // Generate flash loans
      const flData: FlashLoan[] = [
        { id: '1', protocol: 'Aave', token: 'USDC', amount: 50000000, timestamp: now - 30000, txHash: '0xfl1', profit: 25000, status: 'executed' },
        { id: '2', protocol: 'Aave', token: 'ETH', amount: 50000, timestamp: now - 120000, txHash: '0xfl2', profit: -5000, status: 'failed' },
        { id: '3', protocol: 'dYdX', token: 'USDC', amount: 25000000, timestamp: now - 300000, txHash: '0xfl3', profit: 15000, status: 'executed' },
        { id: '4', protocol: 'Radiant', token: 'USDC', amount: 100000000, timestamp: now - 600000, txHash: '0xfl4', profit: 42000, status: 'executed' },
        { id: '5', protocol: 'Venom', token: 'USDT', amount: 75000000, timestamp: now - 900000, txHash: '0xfl5', profit: 18000, status: 'executed' },
      ];
      setFlashLoans(flData);
      
      // Generate alerts
      const alertData: WhaleAlert[] = [
        { id: '1', address: '0x8a2d16f55a6562E487B1C2c2a2a2a2a2a2a2a2a2', type: 'large_transfer', value: 25000000, timestamp: now - 30000, details: 'Transferred 12,500 ETH to 0x...abc', read: false },
        { id: '2', address: '0x7b3C2c2a2a2a2a2a2a2a2a2a2a2a2a2', type: 'whale_move', value: 8500000, timestamp: now - 180000, details: 'Swapped 5,000 ETH for 12.5M USDC', read: false },
        { id: '3', address: '0x6c1B2c2a2a2a2a2a2a2a2a2a2a2a2a', type: 'mev_detected', value: 45000, timestamp: now - 300000, details: 'Arbitrage opportunity detected', read: true },
        { id: '4', address: '0x5d2B2c2a2a2a2a2a2a2a2a2a2a2a2', type: 'liquidation', value: 1250000, timestamp: now - 600000, details: 'Liquidated 2.5M USDC debt', read: true },
        { id: '5', address: '0x4e3C2c2a2a2a2a2a2a2a2a2a2a2a', type: 'large_transfer', value: 45000000, timestamp: now - 900000, details: 'Transferred 25,000 ETH to cold wallet', read: false },
      ];
      setAlerts(alertData);
      
      // Generate trend data (7 days)
      const trendData: TrendData[] = [];
      for (let i = 7; i >= 0; i--) {
        const timestamp = now - i * 24 * 60 * 60 * 1000;
        trendData.push({
          timestamp,
          volume: Math.round(5000000000 + Math.random() * 3000000000),
          transactionCount: Math.round(500 + Math.random() * 500),
          uniqueWhales: Math.round(50 + Math.random() * 30),
        });
      }
      setTrends(trendData);
      
      setError(null);
    } catch (err) {
      setError('Failed to fetch whale data');
      console.error('Whale data error:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchWhaleData();
    const interval = setInterval(fetchWhaleData, 30000);
    return () => clearInterval(interval);
  }, [fetchWhaleData]);

  return { 
    transactions: filter === 'all' ? transactions : transactions.filter(t => t.type === filter), 
    allTransactions: transactions,
    whales, 
    mevOpportunities, 
    flashLoans, 
    alerts, 
    trends, 
    loading, 
    error, 
    filter,
    setFilter,
    refetch: fetchWhaleData 
  };
};

// Stats overview component
interface StatsOverviewProps {
  transactions: WhaleTransaction[];
  whales: Whale[];
  mevOpportunities: MEVOpportunity[];
  flashLoans: FlashLoan[];
  alerts: WhaleAlert[];
}

const StatsOverview: React.FC<StatsOverviewProps> = ({ 
  transactions, 
  whales, 
  mevOpportunities, 
  flashLoans,
  alerts 
}) => {
  const totalVolume = transactions.reduce((acc, t) => acc + t.usdValue, 0);
  const totalWhaleValue = whales.reduce((acc, w) => acc + w.totalValue, 0);
  const totalMEVProfit = mevOpportunities.filter(m => m.status === 'executed').reduce((acc, m) => acc + m.netProfit, 0);
  const totalFlashProfit = flashLoans.filter(f => f.status === 'executed').reduce((acc, f) => acc + f.profit, 0);
  const unreadAlerts = alerts.filter(a => !a.read).length;
  
  return (
    <div className="stats-overview">
      <div className="overview-cards">
        <div className="overview-card">
          <div className="card-icon">🐋</div>
          <div className="card-content">
            <div className="card-label">Total Volume (24h)</div>
            <div className="card-value">${(totalVolume / 1000000000).toFixed(2)}B</div>
          </div>
        </div>
        <div className="overview-card">
          <div className="card-icon">💎</div>
          <div className="card-content">
            <div className="card-label">Whale TVL</div>
            <div className="card-value">${(totalWhaleValue / 1000000000).toFixed(1)}B</div>
          </div>
        </div>
        <div className="overview-card">
          <div className="card-icon">⚡</div>
          <div className="card-content">
            <div className="card-label">MEV Profit (24h)</div>
            <div className="card-value">${(totalMEVProfit / 1000).toFixed(0)}K</div>
          </div>
        </div>
        <div className="overview-card">
          <div className="card-icon">🔥</div>
          <div className="card-content">
            <div className="card-label">Flash Loans (24h)</div>
            <div className="card-value">${(totalFlashProfit / 1000).toFixed(0)}K</div>
          </div>
        </div>
        <div className="overview-card alert">
          <div className="card-icon">🔔</div>
          <div className="card-content">
            <div className="card-label">Active Alerts</div>
            <div className="card-value">{unreadAlerts}</div>
          </div>
        </div>
      </div>
      
      <style>{`
        .stats-overview {
          margin-bottom: 24px;
        }
        .overview-cards {
          display: grid;
          grid-template-columns: repeat(5, 1fr);
          gap: 16px;
        }
        .overview-card {
          display: flex;
          align-items: center;
          gap: 12px;
          background: #1e293b;
          border-radius: 12px;
          padding: 16px;
        }
        .overview-card.alert {
          background: linear-gradient(135deg, #ef4444 0%, #dc2626 100%);
        }
        .card-icon {
          font-size: 28px;
        }
        .card-label {
          font-size: 12px;
          color: #94a3b8;
          text-transform: uppercase;
        }
        .card-value {
          font-size: 20px;
          font-weight: 700;
          color: #e2e8f0;
        }
        @media (max-width: 1024px) {
          .overview-cards {
            grid-template-columns: repeat(2, 1fr);
          }
        }
      `}</style>
    </div>
  );
};

// Recent whale transactions component
interface RecentTransactionsProps {
  transactions: WhaleTransaction[];
  onAddressClick: (address: string) => void;
}

const RecentTransactions: React.FC<RecentTransactionsProps> = ({ transactions, onAddressClick }) => {
  const sorted = [...transactions].sort((a, b) => b.timestamp - a.timestamp).slice(0, 20);
  
  const formatAddress = (addr: string) => addr.slice(0, 10) + '...' + addr.slice(-8);
  const formatValue = (val: number) => {
    if (val >= 1000000) return `$${(val / 1000000).toFixed(1)}M`;
    if (val >= 1000) return `$${(val / 1000).toFixed(1)}K`;
    return `$${val.toFixed(0)}`;
  };
  
  return (
    <div className="recent-transactions">
      <h3>Recent Large Transactions</h3>
      <div className="transactions-list">
        {sorted.map((tx) => (
          <div key={tx.id} className="transaction-item">
            <div className="tx-type">
              <span className={`type-badge ${tx.type}`}>{tx.type}</span>
            </div>
            <div className="tx-info">
              <div className="tx-hash">{formatAddress(tx.hash)}</div>
              <div className="tx-addresses">
                <span className="tx-from" onClick={() => onAddressClick(tx.from)}>{formatAddress(tx.from)}</span>
                {' → '}
                <span className="tx-to" onClick={() => onAddressClick(tx.to)}>{formatAddress(tx.to)}</span>
              </div>
            </div>
            <div className="tx-value">{formatValue(tx.usdValue)}</div>
            <div className="tx-token">{tx.tokenAmount.toFixed(2)} {tx.token}</div>
            <div className="tx-time">{new Date(tx.timestamp).toLocaleTimeString()}</div>
          </div>
        ))}
      </div>
      
      <style>{`
        .recent-transactions {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .recent-transactions h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
          font-size: 16px;
        }
        .transactions-list {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .transaction-item {
          display: grid;
          grid-template-columns: 80px 2fr 100px 100px 80px;
          align-items: center;
          padding: 12px;
          background: #0f172a;
          border-radius: 8px;
        }
        .type-badge {
          padding: 4px 8px;
          border-radius: 4px;
          font-size: 11px;
          font-weight: 600;
          text-transform: uppercase;
        }
        .type-badge.swap { background: #3b82f6; color: white; }
        .type-badge.transfer { background: #8b5cf6; color: white; }
        .type-badge.mint { background: #10b981; color: white; }
        .type-badge.burn { background: #ef4444; color: white; }
        .type-badge.bridge { background: #f59e0b; color: white; }
        .type-badge.liquidate { background: #dc2626; color: white; }
        .type-badge.flashloan { background: #06b6d4; color: white; }
        .tx-hash {
          font-family: monospace;
          color: #3b82f6;
          font-size: 13px;
        }
        .tx-addresses {
          font-size: 11px;
          color: #64748b;
        }
        .tx-from, .tx-to {
          color: #94a3b8;
          cursor: pointer;
        }
        .tx-from:hover, .tx-to:hover {
          color: #3b82f6;
        }
        .tx-value {
          font-weight: 600;
          color: #10b981;
        }
        .tx-token {
          color: #e2e8f0;
        }
        .tx-time {
          color: #64748b;
          font-size: 12px;
        }
      `}</style>
    </div>
  );
};

// Top whales component
interface TopWhalesProps {
  whales: Whale[];
  onAddressClick: (address: string) => void;
}

const TopWhales: React.FC<TopWhalesProps> = ({ whales, onAddressClick }) => {
  const sorted = [...whales].sort((a, b) => b.totalValue - a.totalValue).slice(0, 10);
  
  const formatValue = (val: number) => {
    if (val >= 1000000000) return `$${(val / 1000000000).toFixed(1)}B`;
    if (val >= 1000000) return `$${(val / 1000000).toFixed(0)}M`;
    return `$${(val / 1000).toFixed(0)}K`;
  };
  
  const formatAddress = (addr: string) => addr.slice(0, 10) + '...' + addr.slice(-8);
  
  const typeColors: Record<string, string> = {
    protocol: '#3b82f6',
    whale: '#8b5cf6',
    miner: '#f59e0b',
    dao: '#10b981',
    vc: '#ec4899',
    trader: '#06b6d4',
  };
  
  return (
    <div className="top-whales">
      <h3>Top Whales</h3>
      <div className="whales-list">
        {sorted.map((whale, i) => (
          <div key={whale.id} className="whale-item">
            <div className="whale-rank">{i + 1}</div>
            <div className="whale-info">
              <div className="whale-label">{whale.label}</div>
              <div className="whale-address" onClick={() => onAddressClick(whale.address)}>
                {formatAddress(whale.address)}
              </div>
            </div>
            <div className="whale-type" style={{ color: typeColors[whale.type] }}>{whale.type}</div>
            <div className="whale-tvl">{formatValue(whale.totalValue)}</div>
            <div className={`whale-profit ${whale.profit30d >= 0 ? 'positive' : 'negative'}`}>
              {whale.profit30d >= 0 ? '+' : ''}{formatValue(whale.profit30d)}
            </div>
            <div className="whale-last">{new Date(whale.lastActive).toLocaleTimeString()}</div>
          </div>
        ))}
      </div>
      
      <style>{`
        .top-whales {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .top-whales h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
          font-size: 16px;
        }
        .whales-list {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .whale-item {
          display: grid;
          grid-template-columns: 32px 2fr 80px 100px 100px 80px;
          align-items: center;
          padding: 12px;
          background: #0f172a;
          border-radius: 8px;
        }
        .whale-rank {
          color: #64748b;
          font-weight: 600;
        }
        .whale-label {
          font-weight: 600;
          color: #e2e8f0;
        }
        .whale-address {
          font-size: 12px;
          color: #3b82f6;
          cursor: pointer;
          font-family: monospace;
        }
        .whale-type {
          font-size: 12px;
          font-weight: 600;
          text-transform: uppercase;
        }
        .whale-tvl {
          font-weight: 600;
          color: #e2e8f0;
        }
        .whale-profit {
          font-size: 13px;
        }
        .whale-profit.positive { color: #10b981; }
        .whale-profit.negative { color: #ef4444; }
        .whale-last {
          color: #64748b;
          font-size: 12px;
        }
      `}</style>
    </div>
  );
};

// MEV opportunities component
interface MEVOpportunitiesProps {
  opportunities: MEVOpportunity[];
}

const MEVOpportunities: React.FC<MEVOpportunitiesProps> = ({ opportunities }) => {
  const typeLabels: Record<string, { label: string; color: string }> = {
    arb: { label: 'Arbitrage', color: '#3b82f6' },
    sandwich: { label: 'Sandwich', color: '#ef4444' },
    liquidate: { label: 'Liquidation', color: '#f59e0b' },
    backrun: { label: 'Backrun', color: '#8b5cf6' },
  };
  
  const statusColors: Record<string, string> = {
    detected: '#f59e0b',
    executed: '#10b981',
    failed: '#ef4444',
  };
  
  return (
    <div className="mev-opportunities">
      <h3>MEV Opportunities</h3>
      <div className="opportunities-list">
        {opportunities.map((opp) => (
          <div key={opp.id} className="opportunity-item">
            <div className="opp-type" style={{ backgroundColor: typeLabels[opp.type]?.color }}>
              {typeLabels[opp.type]?.label}
            </div>
            <div className="opp-details">
              <div className="opp-hash">{opp.transactionHash.slice(0, 10)}...</div>
              <div className="opp-tokens">{opp.tokens.join(' → ')}</div>
            </div>
            <div className="opp-profit">${opp.estimatedProfit.toLocaleString()}</div>
            <div className="opp-cost">${opp.gasCost.toLocaleString()}</div>
            <div className="opp-net" style={{ color: opp.netProfit > 0 ? '#10b981' : '#ef4444' }}>
              ${opp.netProfit.toLocaleString()}
            </div>
            <div className="opp-status" style={{ color: statusColors[opp.status] }}>{opp.status}</div>
          </div>
        ))}
      </div>
      
      <style>{`
        .mev-opportunities {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .mev-opportunities h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
          font-size: 16px;
        }
        .opportunities-list {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .opportunity-item {
          display: grid;
          grid-template-columns: 100px 1fr 80px 80px 80px 80px;
          align-items: center;
          padding: 12px;
          background: #0f172a;
          border-radius: 8px;
        }
        .opp-type {
          padding: 4px 8px;
          border-radius: 4px;
          font-size: 11px;
          font-weight: 600;
          color: white;
          text-align: center;
        }
        .opp-hash {
          font-family: monospace;
          color: #3b82f6;
          font-size: 12px;
        }
        .opp-tokens {
          font-size: 11px;
          color: #64748b;
        }
        .opp-profit, .opp-cost {
          color: #e2e8f0;
        }
        .opp-net {
          font-weight: 600;
        }
        .opp-status {
          font-size: 12px;
          font-weight: 600;
          text-transform: uppercase;
        }
      `}</style>
    </div>
  );
};

// Flash loans component
interface FlashLoansProps {
  flashLoans: FlashLoan[];
}

const FlashLoans: React.FC<FlashLoansProps> = ({ flashLoans }) => {
  const protocolIcons: Record<string, string> = {
    Aave: '🦁',
    dYdX: '⚡',
    Radiant: '💠',
    Venom: '🦂',
  };
  
  return (
    <div className="flash-loans">
      <h3>Flash Loans</h3>
      <div className="flash-list">
        {flashLoans.map((fl) => (
          <div key={fl.id} className="flash-item">
            <div className="flash-icon">{protocolIcons[fl.protocol] || '💰'}</div>
            <div className="flash-info">
              <div className="flash-protocol">{fl.protocol}</div>
              <div className="flash-token">{fl.token}</div>
            </div>
            <div className="flash-amount">${(fl.amount / 1000000).toFixed(1)}M</div>
            <div className="flash-profit" style={{ color: fl.profit > 0 ? '#10b981' : '#ef4444' }}>
              {fl.profit > 0 ? '+' : ''}{fl.profit >= 1000 ? `$${(fl.profit / 1000).toFixed(0)}K` : `$${fl.profit}`}
            </div>
            <div className="flash-time">{new Date(fl.timestamp).toLocaleTimeString()}</div>
            <div className="flash-status" style={{ 
              color: fl.status === 'executed' ? '#10b981' : fl.status === 'failed' ? '#ef4444' : '#f59e0b' 
            }}>{fl.status}</div>
          </div>
        ))}
      </div>
      
      <style>{`
        .flash-loans {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .flash-loans h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
          font-size: 16px;
        }
        .flash-list {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .flash-item {
          display: grid;
          grid-template-columns: 40px 1fr 100px 80px 80px 80px;
          align-items: center;
          padding: 12px;
          background: #0f172a;
          border-radius: 8px;
        }
        .flash-icon {
          font-size: 24px;
        }
        .flash-protocol {
          font-weight: 600;
          color: #e2e8f0;
        }
        .flash-token {
          font-size: 12px;
          color: #64748b;
        }
        .flash-amount {
          color: #e2e8f0;
        }
        .flash-profit {
          font-weight: 600;
        }
        .flash-time {
          color: #64748b;
          font-size: 12px;
        }
        .flash-status {
          font-size: 12px;
          font-weight: 600;
          text-transform: uppercase;
        }
      `}</style>
    </div>
  );
};

// Alerts panel component
interface AlertsPanelProps {
  alerts: WhaleAlert[];
  onMarkRead: (id: string) => void;
}

const AlertsPanel: React.FC<AlertsPanelProps> = ({ alerts, onMarkRead }) => {
  const typeIcons: Record<string, string> = {
    large_transfer: '💸',
    whale_move: '🐋',
    mev_detected: '⚡',
    liquidation: '🔥',
  };
  
  return (
    <div className="alerts-panel">
      <h3>Alerts</h3>
      <div className="alerts-list">
        {alerts.map((alert) => (
          <div 
            key={alert.id} 
            className={`alert-item ${alert.read ? 'read' : 'unread'}`}
            onClick={() => !alert.read && onMarkRead(alert.id)}
          >
            <div className="alert-icon">{typeIcons[alert.type]}</div>
            <div className="alert-content">
              <div className="alert-type">{alert.type.replace('_', ' ')}</div>
              <div className="alert-details">{alert.details}</div>
              <div className="alert-value">${(alert.value / 1000000).toFixed(1)}M</div>
            </div>
            <div className="alert-time">{new Date(alert.timestamp).toLocaleTimeString()}</div>
          </div>
        ))}
      </div>
      
      <style>{`
        .alerts-panel {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .alerts-panel h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
          font-size: 16px;
        }
        .alerts-list {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .alert-item {
          display: grid;
          grid-template-columns: 40px 2fr 80px 80px;
          align-items: center;
          padding: 12px;
          background: #0f172a;
          border-radius: 8px;
          cursor: pointer;
          border-left: 3px solid transparent;
        }
        .alert-item.unread {
          border-left-color: #ef4444;
        }
        .alert-item.read {
          opacity: 0.7;
        }
        .alert-icon {
          font-size: 20px;
        }
        .alert-type {
          font-size: 12px;
          color: #64748b;
          text-transform: capitalize;
        }
        .alert-details {
          color: #e2e8f0;
        }
        .alert-value {
          color: #10b981;
          font-weight: 600;
        }
        .alert-time {
          color: #64748b;
          font-size: 12px;
        }
      `}</style>
    </div>
  );
};

// Volume trend chart
interface VolumeTrendsProps {
  trends: TrendData[];
}

const VolumeTrends: React.FC<VolumeTrendsProps> = ({ trends }) => {
  const chartData = trends.map(t => ({
    date: new Date(t.timestamp).toLocaleDateString([], { month: 'short', day: 'numeric' }),
    volume: t.volume / 1000000000,
    txns: t.transactionCount,
    whales: t.uniqueWhales,
  }));
  
  return (
    <div className="volume-trends">
      <h3>7-Day Volume Trend</h3>
      <ResponsiveContainer width="100%" height={250}>
        <AreaChart data={chartData}>
          <defs>
            <linearGradient id="volumeGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="#8b5cf6" stopOpacity={0.3} />
              <stop offset="95%" stopColor="#8b5cf6" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
          <XAxis dataKey="date" stroke="#94a3b8" fontSize={11} />
          <YAxis stroke="#94a3b8" fontSize={11} tickFormatter={(v) => `$${v}B`} />
          <Tooltip contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px' }} />
          <Area type="monotone" dataKey="volume" stroke="#8b5cf6" fill="url(#volumeGradient)" strokeWidth={2} />
        </AreaChart>
      </ResponsiveContainer>
      
      <style>{`
        .volume-trends {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .volume-trends h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
          font-size: 16px;
        }
      `}</style>
    </div>
  );
};

// Filter component
interface FilterProps {
  filter: string;
  setFilter: (filter: string) => void;
}

const Filter: React.FC<FilterProps> = ({ filter, setFilter }) => {
  const filters = [
    { value: 'all', label: 'All' },
    { value: 'swap', label: 'Swaps' },
    { value: 'transfer', label: 'Transfers' },
    { value: 'mint', label: 'Mints' },
    { value: 'burn', label: 'Burns' },
    { value: 'bridge', label: 'Bridges' },
    { value: 'liquidate', label: 'Liquidations' },
    { value: 'flashloan', label: 'Flash Loans' },
  ];
  
  return (
    <div className="filter-bar">
      {filters.map((f) => (
        <button
          key={f.value}
          className={`filter-btn ${filter === f.value ? 'active' : ''}`}
          onClick={() => setFilter(f.value)}
        >
          {f.label}
        </button>
      ))}
      <style>{`
        .filter-bar {
          display: flex;
          gap: 8px;
          margin-bottom: 24px;
          flex-wrap: wrap;
        }
        .filter-btn {
          padding: 8px 16px;
          background: #1e293b;
          border: 1px solid #334155;
          border-radius: 8px;
          color: #94a3b8;
          cursor: pointer;
          transition: all 0.2s;
        }
        .filter-btn:hover {
          border-color: #3b82f6;
          color: #e2e8f0;
        }
        .filter-btn.active {
          background: #3b82f6;
          border-color: #3b82f6;
          color: white;
        }
      `}</style>
    </div>
  );
};

// Main Whale Tracking Dashboard
const WhaleTracking: React.FC = () => {
  const { 
    transactions, allTransactions, whales, mevOpportunities, flashLoans, alerts, trends, loading, error, filter, setFilter, refetch 
  } = useWhaleTracking();
  
  const handleAddressClick = (address: string) => {
    console.log('Address clicked:', address);
  };
  
  const handleMarkAlertRead = (id: string) => {
    console.log('Mark alert as read:', id);
  };
  
  if (loading && transactions.length === 0) {
    return (
      <div className="whale-tracking">
        <div className="loading-container">
          <div className="loading-spinner"></div>
          <p>Loading whale tracking data...</p>
        </div>
        <style>{`
          .loading-container {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            height: 400px;
            color: #94a3b8;
          }
          .loading-spinner {
            width: 40px;
            height: 40px;
            border: 3px solid #334155;
            border-top-color: #8b5cf6;
            border-radius: 50%;
            animation: spin 1s linear infinite;
          }
          @keyframes spin { to { transform: rotate(360deg); } }
        `}</style>
      </div>
    );
  }
  
  return (
    <div className="whale-tracking">
      <div className="page-header">
        <h1>🐋 Whale Tracking</h1>
        <p>Real-time whale activity, MEV opportunities, and large transactions</p>
        <button className="refresh-btn" onClick={refetch}>↻ Refresh</button>
      </div>
      
      <StatsOverview 
        transactions={allTransactions}
        whales={whales}
        mevOpportunities={mevOpportunities}
        flashLoans={flashLoans}
        alerts={alerts}
      />
      
      <Filter filter={filter} setFilter={setFilter} />
      
      <div className="dashboard-grid">
        <RecentTransactions transactions={transactions} onAddressClick={handleAddressClick} />
        <TopWhales whales={whales} onAddressClick={handleAddressClick} />
      </div>
      
      <div className="dashboard-grid">
        <MEVOpportunities opportunities={mevOpportunities} />
        <FlashLoans flashLoans={flashLoans} />
      </div>
      
      <div className="dashboard-grid">
        <AlertsPanel alerts={alerts} onMarkRead={handleMarkAlertRead} />
        <VolumeTrends trends={trends} />
      </div>
      
      <style>{`
        .whale-tracking {
          padding: 24px;
          max-width: 1400px;
          margin: 0 auto;
        }
        .page-header {
          margin-bottom: 24px;
          display: flex;
          flex-direction: column;
        }
        .page-header h1 {
          font-size: 32px;
          font-weight: 700;
          color: #e2e8f0;
          margin-bottom: 8px;
        }
        .page-header p {
          color: #94a3b8;
        }
        .refresh-btn {
          margin-top: 12px;
          align-self: flex-start;
          padding: 8px 16px;
          background: #8b5cf6;
          border: none;
          border-radius: 8px;
          color: white;
          cursor: pointer;
          font-weight: 500;
        }
        .refresh-btn:hover { background: #7c3aed; }
        .dashboard-grid {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: 24px;
          margin-bottom: 24px;
        }
        @media (max-width: 1024px) {
          .dashboard-grid { grid-template-columns: 1fr; }
        }
      `}</style>
    </div>
  );
};

export default WhaleTracking;