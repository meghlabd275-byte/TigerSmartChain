/**
 * Top Holders - holder distribution for one token, read from the indexer.
 *
 * Sources: GET /api/v1/tokens/:address (token metadata),
 *          GET /api/v1/tokens/:address/holders (ranked holders),
 *          GET /api/v1/tokens/:address/analytics (concentration and buckets).
 */

import React, { useState, useEffect, useCallback } from 'react';
import { useRouter } from 'next/router';
import { PieChart, Pie, Cell, ResponsiveContainer, BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip } from 'recharts';
import { api } from '@/lib/api';

interface Holder {
  rank: number;
  address: string;
  balance: string;
  balanceUsd: number;
  percentage: number;
  isContract: boolean;
}

interface TokenStats {
  name: string;
  symbol: string;
  holdersCount: number;
  marketCap: number;
  top10: number;
  top50: number;
  top100: number;
}

interface HolderDistribution {
  range: string;
  count: number;
}

// Balances are stored as integers in the token's smallest unit.
const TOKEN_DECIMALS = 1e18;

const toNumber = (value: unknown): number => {
  if (value === null || value === undefined) return 0;
  const n = Number(value);
  return Number.isFinite(n) ? n : 0;
};

const useHolders = (tokenAddress?: string) => {
  const [holders, setHolders] = useState<Holder[]>([]);
  const [token, setToken] = useState<TokenStats | null>(null);
  const [distribution, setDistribution] = useState<HolderDistribution[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    if (!tokenAddress) {
      setLoading(false);
      return;
    }
    try {
      setLoading(true);

      const [tokenData, holderRows, analytics] = await Promise.all([
        api.getToken(tokenAddress),
        api.getTokenHolders(tokenAddress, { limit: 100 }),
        api.getTokenAnalytics(tokenAddress),
      ]);

      const rows = holderRows.items ?? [];
      const balances = rows.map(r => toNumber(r.balance) / TOKEN_DECIMALS);
      const totalBalance = balances.reduce((a, b) => a + b, 0);

      setHolders(
        rows.map((r, i) => ({
          rank: i + 1,
          address: r.address,
          balance: balances[i].toLocaleString(undefined, { maximumFractionDigits: 4 }),
          balanceUsd: toNumber(r.balanceUsd),
          percentage: totalBalance > 0 ? (balances[i] / totalBalance) * 100 : 0,
          isContract: Boolean(r.isContract),
        }))
      );

      setToken({
        name: tokenData?.name ?? '',
        symbol: tokenData?.symbol ?? '',
        holdersCount: toNumber((tokenData as any)?.holdersCount),
        marketCap: toNumber((tokenData as any)?.marketCap),
        top10: toNumber(analytics?.top10Percent),
        top50: toNumber(analytics?.top50Percent),
        top100: toNumber(analytics?.top100Percent),
      });

      setDistribution([
        { range: '> $10M', count: toNumber(analytics?.above10m) },
        { range: '$1M - $10M', count: toNumber(analytics?.from1mTo10m) },
        { range: '$100K - $1M', count: toNumber(analytics?.from100kTo1m) },
        { range: '$10K - $100K', count: toNumber(analytics?.from10kTo100k) },
        { range: '$1K - $10K', count: toNumber(analytics?.from1kTo10k) },
        { range: '< $1K', count: toNumber(analytics?.below1k) },
      ]);

      setError(null);
    } catch (err) {
      setError('Failed to load holder data for this token');
    } finally {
      setLoading(false);
    }
  }, [tokenAddress]);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 60000);
    return () => clearInterval(interval);
  }, [fetchData]);

  return { holders, token, distribution, loading, error, refetch: fetchData };
};

const StatsCards: React.FC<{ token: TokenStats }> = ({ token }) => (
  <div className="stats-grid">
    <div className="stat-card">
      <div className="stat-label">Total Holders</div>
      <div className="stat-value">{token.holdersCount.toLocaleString()}</div>
    </div>
    <div className="stat-card">
      <div className="stat-label">Market Cap</div>
      <div className="stat-value">${(token.marketCap / 1e9).toFixed(2)}B</div>
    </div>
    <div className="stat-card">
      <div className="stat-label">Top 10 Concentration</div>
      <div className="stat-value">{token.top10.toFixed(2)}%</div>
    </div>
    <div className="stat-card">
      <div className="stat-label">Top 100 Concentration</div>
      <div className="stat-value">{token.top100.toFixed(2)}%</div>
    </div>

    <style>{`
      .stats-grid {
        display: grid;
        grid-template-columns: repeat(4, 1fr);
        gap: 16px;
        margin-bottom: 24px;
      }
      .stat-card {
        background: #1e293b;
        border-radius: 12px;
        padding: 20px;
        text-align: center;
      }
      .stat-label { font-size: 12px; color: #94a3b8; text-transform: uppercase; margin-bottom: 8px; }
      .stat-value { font-size: 24px; font-weight: 700; color: #e2e8f0; }
      @media (max-width: 768px) { .stats-grid { grid-template-columns: repeat(2, 1fr); } }
    `}</style>
  </div>
);

const ConcentrationChart: React.FC<{ token: TokenStats }> = ({ token }) => {
  const data = [
    { name: 'Top 10', value: token.top10, color: '#ef4444' },
    { name: 'Top 11-50', value: Math.max(token.top50 - token.top10, 0), color: '#f59e0b' },
    { name: 'Top 51-100', value: Math.max(token.top100 - token.top50, 0), color: '#3b82f6' },
    { name: 'Others', value: Math.max(100 - token.top100, 0), color: '#10b981' },
  ];

  return (
    <div className="concentration-chart">
      <h3>Holder Concentration</h3>
      <div className="chart-container">
        <ResponsiveContainer width="100%" height={250}>
          <PieChart>
            <Pie data={data} dataKey="value" nameKey="name" cx="50%" cy="50%" innerRadius={60} outerRadius={100}>
              {data.map((entry, index) => <Cell key={index} fill={entry.color} />)}
            </Pie>
            <Tooltip contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px' }} />
          </PieChart>
        </ResponsiveContainer>
        <div className="legend">
          {data.map(item => (
            <div key={item.name} className="legend-item">
              <span className="color" style={{ backgroundColor: item.color }}></span>
              <span className="label">{item.name}</span>
              <span className="value">{item.value.toFixed(2)}%</span>
            </div>
          ))}
        </div>
      </div>

      <style>{`
        .concentration-chart {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .concentration-chart h3 { color: #e2e8f0; margin-bottom: 16px; }
        .chart-container { display: flex; align-items: center; }
        .legend { margin-left: 24px; }
        .legend-item { display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }
        .color { width: 12px; height: 12px; border-radius: 3px; }
        .label { color: #94a3b8; flex: 1; }
        .value { color: #e2e8f0; font-weight: 600; }
      `}</style>
    </div>
  );
};

const DistributionChart: React.FC<{ distribution: HolderDistribution[] }> = ({ distribution }) => (
  <div className="distribution-chart">
    <h3>Holder Distribution by Balance</h3>
    <ResponsiveContainer width="100%" height={300}>
      <BarChart data={distribution} layout="vertical">
        <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
        <XAxis type="number" stroke="#94a3b8" />
        <YAxis dataKey="range" type="category" stroke="#94a3b8" width={100} />
        <Tooltip contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px' }} />
        <Bar dataKey="count" fill="#3b82f6" name="Holders" />
      </BarChart>
    </ResponsiveContainer>

    <style>{`
      .distribution-chart {
        background: #1e293b;
        border-radius: 12px;
        padding: 20px;
        margin-bottom: 24px;
      }
      .distribution-chart h3 { color: #e2e8f0; margin-bottom: 16px; }
    `}</style>
  </div>
);

const HoldersTable: React.FC<{ holders: Holder[] }> = ({ holders }) => {
  const formatAddress = (addr: string) => addr.slice(0, 10) + '...' + addr.slice(-8);

  return (
    <div className="holders-table">
      <h3>Top Holders</h3>
      <div className="table">
        <div className="table-header">
          <span>Rank</span>
          <span>Address</span>
          <span>Balance</span>
          <span>Value</span>
          <span>Percentage</span>
        </div>
        {holders.map(holder => (
          <div key={holder.address} className="table-row">
            <span className="rank">#{holder.rank}</span>
            <span className="address">
              {formatAddress(holder.address)}
              {holder.isContract && <span className="contract-badge">Contract</span>}
            </span>
            <span className="balance">{holder.balance}</span>
            <span className="balance-usd">${holder.balanceUsd.toLocaleString(undefined, { maximumFractionDigits: 2 })}</span>
            <span className="percentage">{holder.percentage.toFixed(4)}%</span>
          </div>
        ))}
      </div>

      <style>{`
        .holders-table { background: #1e293b; border-radius: 12px; padding: 20px; }
        .holders-table h3 { color: #e2e8f0; margin-bottom: 16px; }
        .table { overflow-x: auto; }
        .table-header, .table-row {
          display: grid;
          grid-template-columns: 60px 200px 160px 140px 110px;
          padding: 14px 16px;
          align-items: center;
        }
        .table-header { background: #0f172a; color: #94a3b8; font-size: 11px; text-transform: uppercase; }
        .table-row { border-bottom: 1px solid #334155; color: #e2e8f0; font-size: 13px; }
        .rank { color: #64748b; font-weight: 600; }
        .address { font-family: monospace; color: #3b82f6; cursor: pointer; }
        .contract-badge { margin-left: 8px; padding: 2px 6px; background: #f59e0b; border-radius: 4px; font-size: 10px; color: white; }
        .balance { color: #e2e8f0; font-family: monospace; }
        .balance-usd { color: #10b981; font-weight: 600; }
        .percentage { color: #94a3b8; }
      `}</style>
    </div>
  );
};

const TopHolders: React.FC = () => {
  const router = useRouter();
  const tokenAddress = typeof router.query.address === 'string' ? router.query.address : undefined;
  const { holders, token, distribution, loading, error, refetch } = useHolders(tokenAddress);

  if (loading) {
    return <div className="loading">Loading...</div>;
  }

  if (!tokenAddress) {
    return (
      <div className="top-holders-page">
        <div className="page-header">
          <h1>🏆 Top Holders</h1>
          <p>Open this page with a token address to see its holder distribution.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="top-holders-page">
      <div className="page-header">
        <h1>🏆 Top Holders{token?.symbol ? ` - ${token.symbol}` : ''}</h1>
        <p>{token?.name || tokenAddress}</p>
        <button onClick={refetch}>↻ Refresh</button>
      </div>

      {error && <div className="error">{error}</div>}

      {token && <StatsCards token={token} />}

      <div className="grid">
        {token && <ConcentrationChart token={token} />}
        <DistributionChart distribution={distribution} />
      </div>

      {holders.length === 0 && !error ? (
        <div className="empty">No holders are indexed for this token yet.</div>
      ) : (
        <HoldersTable holders={holders} />
      )}

      <style>{`
        .top-holders-page { padding: 24px; max-width: 1400px; margin: 0 auto; }
        .page-header { margin-bottom: 24px; }
        .page-header h1 { font-size: 32px; color: #e2e8f0; margin-bottom: 8px; }
        .page-header p { color: #94a3b8; }
        .page-header button { margin-top: 12px; padding: 8px 16px; background: #3b82f6; border: none; border-radius: 8px; color: white; cursor: pointer; }
        .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }
        @media (max-width: 1024px) { .grid { grid-template-columns: 1fr; } }
        .loading, .empty { padding: 40px; text-align: center; color: #94a3b8; }
        .error { padding: 16px; background: #7f1d1d; color: #fecaca; border-radius: 8px; margin-bottom: 16px; }
      `}</style>
    </div>
  );
};

export default TopHolders;
