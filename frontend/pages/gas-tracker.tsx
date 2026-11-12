/**
 * Gas Tracker
 *
 * Every figure on this page comes from indexed chain data: the blocks table for
 * the latest block's fill and base fee, and the gas_prices samples the tracker
 * service writes for current fees, averages, volatility and the history chart.
 *
 * The page previously generated its own data. It built 24 hours of price
 * history with Math.random, forecast two hours ahead with the same, and printed
 * a "Gas Prices by Chain" table with invented prices and volumes for Ethereum,
 * Base, Arbitrum, Optimism and Polygon. That table is gone: TigerScan measures
 * TigerSmartChain, and it has no basis for quoting another chain's fees.
 */

import React, { useState, useEffect, useCallback } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, AreaChart, Area } from 'recharts';
import api from '@/lib/api';

const NATIVE_SYMBOL = 'TGR';

interface GasPrice {
  timestamp: number;
  gasPrice: string;
  baseFee: string;
  priorityFee: string;
  gasUsed: string | null;
  gasLimit: string | null;
}

interface GasStats {
  current: { slow: string; standard: string; fast: string };
  average24h: string;
  average7d: string;
  volatility: string;
  trend: 'up' | 'down' | 'stable';
  samples: string | number | null;
  latestBlock: {
    number: number | null;
    gasUsed: string | null;
    gasLimit: string | null;
    baseFee: string;
    utilization: number;
    timestamp: number | null;
  };
}

const useGasOracle = () => {
  const [prices, setPrices] = useState<GasPrice[]>([]);
  const [stats, setStats] = useState<GasStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchGasData = useCallback(async () => {
    try {
      const [statsResponse, seriesResponse] = await Promise.all([
        api.getGasNetworkStats(),
        api.getGasPriceSeries({ limit: 288 }),
      ]);
      setStats(statsResponse as unknown as GasStats);
      setPrices((seriesResponse.items as unknown as GasPrice[]));
      setError(null);
    } catch (err: any) {
      setError(err?.response?.data?.error || 'Could not load gas data.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchGasData();
    const interval = setInterval(fetchGasData, 30000);
    return () => clearInterval(interval);
  }, [fetchGasData]);

  return { prices, stats, loading, error, refetch: fetchGasData };
};


// Gas speed selector component
interface GasSpeedSelectorProps {
  onSelect: (speed: 'slow' | 'standard' | 'fast') => void;
  prices: { slow: string; standard: string; fast: string };
}

const GasSpeedSelector: React.FC<GasSpeedSelectorProps> = ({ onSelect, prices }) => {
  const [selected, setSelected] = useState<'slow' | 'standard' | 'fast'>('standard');

  const handleSelect = (speed: 'slow' | 'standard' | 'fast') => {
    setSelected(speed);
    onSelect(speed);
  };

  const speedConfig = {
    slow: { label: 'Slow', desc: 'lower fee, less urgent', color: '#10b981' },
    standard: { label: 'Standard', desc: 'next few blocks', color: '#3b82f6' },
    fast: { label: 'Fast', desc: 'next block', color: '#ef4444' },
  };

  return (
    <div className="gas-speed-selector">
      {(['slow', 'standard', 'fast'] as const).map((speed) => (
        <button
          key={speed}
          onClick={() => handleSelect(speed)}
          className={`speed-btn ${selected === speed ? 'active' : ''}`}
          style={{
            borderColor: selected === speed ? speedConfig[speed].color : 'transparent',
            backgroundColor: selected === speed ? `${speedConfig[speed].color}20` : 'transparent',
          }}
        >
          <div className="speed-label" style={{ color: speedConfig[speed].color }}>
            {speedConfig[speed].label}
          </div>
          <div className="speed-price">{prices[speed]} Gwei</div>
          <div className="speed-desc">{speedConfig[speed].desc}</div>
        </button>
      ))}
      <style>{`
        .gas-speed-selector { display: flex; gap: 12px; margin: 20px 0; }
        .speed-btn {
          flex: 1; padding: 16px; border: 2px solid transparent; border-radius: 12px;
          background: #1e293b; cursor: pointer; transition: all 0.2s; text-align: center;
        }
        .speed-btn:hover { transform: translateY(-2px); }
        .speed-label { font-weight: 600; font-size: 14px; margin-bottom: 4px; }
        .speed-price { font-size: 20px; font-weight: 700; color: #e2e8f0; }
        .speed-desc { font-size: 12px; color: #94a3b8; margin-top: 4px; }
      `}</style>
    </div>
  );
};

// Gas history chart. Only measured samples are plotted; there is no forecast
// series, because the explorer has no model that would justify one.
interface GasChartProps {
  data: GasPrice[];
}

const GasChart: React.FC<GasChartProps> = ({ data }) => {
  if (data.length === 0) {
    return (
      <div className="gas-chart">
        <p style={{ color: '#94a3b8' }}>No gas price samples have been indexed yet.</p>
      </div>
    );
  }

  const chartData = data.map((p) => ({
    time: new Date(Number(p.timestamp) * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
    gas: Number(p.gasPrice),
    baseFee: Number(p.baseFee),
  }));

  return (
    <div className="gas-chart">
      <ResponsiveContainer width="100%" height={300}>
        <AreaChart data={chartData}>
          <defs>
            <linearGradient id="gasGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="#3b82f6" stopOpacity={0.3} />
              <stop offset="95%" stopColor="#3b82f6" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
          <XAxis dataKey="time" stroke="#94a3b8" fontSize={12} />
          <YAxis stroke="#94a3b8" fontSize={12} tickFormatter={(v) => `${v} Gwei`} />
          <Tooltip contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px' }} />
          <Area type="monotone" dataKey="gas" stroke="#3b82f6" fill="url(#gasGradient)" strokeWidth={2} name="Gas price" />
          <Line type="monotone" dataKey="baseFee" stroke="#8b5cf6" dot={false} name="Base fee" />
        </AreaChart>
      </ResponsiveContainer>
      <style>{`
        .gas-chart { background: #0f172a; border-radius: 12px; padding: 20px; margin: 20px 0; }
      `}</style>
    </div>
  );
};

// Stats cards component
interface StatsCardsProps {
  stats: GasStats;
}

const StatsCards: React.FC<StatsCardsProps> = ({ stats }) => {
  const utilization = stats.latestBlock?.utilization ?? 0;
  const statCards = [
    { label: 'Current', value: `${stats.current.standard} Gwei`, subtext: 'Standard' },
    { label: '24h Average', value: `${stats.average24h} Gwei`, subtext: 'Last 24 hours' },
    { label: '7d Average', value: `${stats.average7d} Gwei`, subtext: 'Last 7 days' },
    {
      label: 'Trend',
      value: stats.trend === 'up' ? 'Rising' : stats.trend === 'down' ? 'Falling' : 'Stable',
      subtext: '12h vs previous 12h',
      color: stats.trend === 'up' ? '#ef4444' : stats.trend === 'down' ? '#10b981' : '#94a3b8',
    },
    { label: 'Volatility', value: `${stats.volatility}`, subtext: 'Std deviation, Gwei' },
    {
      label: 'Latest Block',
      value: stats.latestBlock?.number != null ? `#${stats.latestBlock.number}` : '-',
      subtext: `${utilization}% full, base fee ${stats.latestBlock?.baseFee ?? '0'} Gwei`,
    },
  ];

  return (
    <div className="stats-grid">
      {statCards.map((card, i) => (
        <div key={i} className="stat-card">
          <div className="stat-label">{card.label}</div>
          <div className="stat-value" style={{ color: card.color }}>{card.value}</div>
          <div className="stat-subtext">{card.subtext}</div>
        </div>
      ))}
      <style>{`
        .stats-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 16px; margin: 20px 0; }
        .stat-card { background: #1e293b; border-radius: 12px; padding: 16px; text-align: center; }
        .stat-label { font-size: 12px; color: #94a3b8; text-transform: uppercase; margin-bottom: 8px; }
        .stat-value { font-size: 24px; font-weight: 700; color: #e2e8f0; }
        .stat-subtext { font-size: 11px; color: #64748b; margin-top: 4px; }
      `}</style>
    </div>
  );
};

// Transaction time estimator. The bands describe what each fee tier means for
// inclusion, not a measured confirmation time; the chain does not publish one.
interface TxTimeEstimatorProps {
  gasPrice: string;
}

const TxTimeEstimator: React.FC<TxTimeEstimatorProps> = ({ gasPrice }) => {
  const tier = (() => {
    const value = Number(gasPrice);
    if (!Number.isFinite(value) || value <= 0) return 'No fee data available.';
    if (value < 20) return 'Included when block space allows.';
    if (value < 40) return 'Typically included within a few blocks.';
    if (value < 100) return 'Likely included in the next block.';
    return 'Competitive for immediate inclusion.';
  })();

  return (
    <div className="tx-estimator">
      <div className="estimator-title">Fee tier at {gasPrice} Gwei</div>
      <div className="estimator-time">{tier}</div>
      <style>{`
        .tx-estimator {
          background: linear-gradient(135deg, #1e293b 0%, #0f172a 100%);
          border-radius: 12px; padding: 24px; text-align: center; margin: 20px 0;
        }
        .estimator-title { color: #94a3b8; font-size: 14px; margin-bottom: 8px; }
        .estimator-time { font-size: 20px; font-weight: 600; color: #3b82f6; }
      `}</style>
    </div>
  );
};


// Main Gas Tracker page component
const GasTracker: React.FC = () => {
  const { prices, stats, loading, error, refetch } = useGasOracle();
  const [selectedSpeed, setSelectedSpeed] = useState<'slow' | 'standard' | 'fast'>('standard');
  
  if (loading && !stats) {
    return (
      <div className="gas-tracker-page">
        <div className="loading-container">
          <div className="loading-spinner"></div>
          <p>Loading gas prices...</p>
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
            border-top-color: #3b82f6;
            border-radius: 50%;
            animation: spin 1s linear infinite;
          }
          @keyframes spin {
            to { transform: rotate(360deg); }
          }
        `}</style>
      </div>
    );
  }
  
  if (error) {
    return (
      <div className="gas-tracker-page">
        <div className="error-container">
          <p>Error: {error}</p>
          <button onClick={refetch}>Retry</button>
        </div>
      </div>
    );
  }
  
  return (
    <div className="gas-tracker-page">
      <div className="page-header">
        <h1>Gas Tracker</h1>
        <p>Fee levels on TigerSmartChain, from indexed block and gas-price data</p>
        <button className="refresh-btn" onClick={refetch}>↻ Refresh</button>
      </div>
      
      {stats && (
        <>
          <GasSpeedSelector
            onSelect={setSelectedSpeed}
            prices={stats.current}
          />
          
          <TxTimeEstimator gasPrice={stats.current[selectedSpeed]} />
          
          <StatsCards stats={stats} />
          
          <GasChart data={prices} />
          
        </>
      )}
      
      <style>{`
        .gas-tracker-page {
          padding: 24px;
          max-width: 1200px;
          margin: 0 auto;
        }
        .page-header {
          margin-bottom: 24px;
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
          padding: 8px 16px;
          background: #3b82f6;
          border: none;
          border-radius: 8px;
          color: white;
          cursor: pointer;
          font-weight: 500;
        }
        .refresh-btn:hover {
          background: #2563eb;
        }
        .error-container {
          text-align: center;
          padding: 40px;
          color: #ef4444;
        }
        .error-container button {
          margin-top: 16px;
          padding: 8px 16px;
          background: #3b82f6;
          border: none;
          border-radius: 8px;
          color: white;
          cursor: pointer;
        }
      `}</style>
    </div>
  );
};

export default GasTracker;