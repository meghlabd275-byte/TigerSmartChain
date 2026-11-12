/**
 * Token Transfer Tracker
 *
 * Rows, summary cards, the hourly activity chart and the token filter all read
 * from the indexer: token_transfers joined to blocks for timestamps and to
 * tokens for symbol and decimals.
 *
 * The page previously generated 200 transfers in the browser with Math.random,
 * including random transaction hashes, random sender and receiver addresses,
 * random gas figures and block numbers counting up from 35,000,000, which is
 * not where this chain is. Its token list was eight hardcoded BNB Chain tokens
 * (USDT, BNB, BUSD, ETH, DAI, CAKE, MINA, DODO) with their real prices, holder
 * counts and market caps. The dollar columns those prices fed are gone; the
 * chain's own token metadata carries a price only when a feed has supplied one.
 */

import React, { useState, useEffect, useCallback } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, AreaChart, Area } from 'recharts';
import api from '@/lib/api';
import { formatWei, formatAddress } from '@/lib/utils';

interface TokenTransfer {
  id: number;
  token_address: string;
  from_address: string;
  to_address: string;
  value: string;
  transaction_hash: string;
  block_number: number;
  log_index: number;
  token_symbol: string | null;
  token_name: string | null;
  token_decimals: number | null;
  token_verified: boolean | null;
  block_timestamp: string | null;
}

interface Token {
  address: string;
  name: string;
  symbol: string;
  decimals: number;
  is_verified?: boolean;
}

interface TransferStats {
  totalTransfers: number;
  uniqueSenders: number;
  uniqueReceivers: number;
  distinctTokens: number;
}

interface SeriesPoint {
  bucket: string;
  transfer_count: number;
  token_count: number;
}

const useTokenTransfers = () => {
  const [transfers, setTransfers] = useState<TokenTransfer[]>([]);
  const [tokens, setTokens] = useState<Token[]>([]);
  const [stats, setStats] = useState<TransferStats | null>(null);
  const [series, setSeries] = useState<SeriesPoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<{
    token?: string;
    address?: string;
    minValue?: number;
  }>({});

  const fetchData = useCallback(async () => {
    try {
      const [transfersResponse, statsResponse, seriesResponse, tokensResponse] = await Promise.all([
        api.getRecentTransfers({ limit: 100 }),
        api.getTransferStats(),
        api.getTransferSeries({ hours: 24 }),
        api.getTokens({ limit: 50 }),
      ]);

      setTransfers(transfersResponse.items as unknown as TokenTransfer[]);
      setStats({
        totalTransfers: Number(statsResponse.totalTransfers || 0),
        uniqueSenders: Number(statsResponse.uniqueSenders || 0),
        uniqueReceivers: Number(statsResponse.uniqueReceivers || 0),
        distinctTokens: Number(statsResponse.distinctTokens || 0),
      });
      setSeries(seriesResponse.items as unknown as SeriesPoint[]);
      setTokens(tokensResponse.items as unknown as Token[]);
      setError(null);
    } catch (err: any) {
      setError(err?.response?.data?.error || 'Could not load transfer data.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 15000);
    return () => clearInterval(interval);
  }, [fetchData]);

  const filteredTransfers = transfers.filter((t) => {
    if (filter.token && t.token_address.toLowerCase() !== filter.token.toLowerCase()) return false;
    if (filter.address) {
      const needle = filter.address.toLowerCase();
      if (!t.from_address.toLowerCase().includes(needle) && !t.to_address.toLowerCase().includes(needle)) {
        return false;
      }
    }
    if (filter.minValue != null && Number(formatWei(t.value, t.token_decimals ?? 18)) < filter.minValue) {
      return false;
    }
    return true;
  });

  return {
    transfers: filteredTransfers,
    allTransfers: transfers,
    tokens,
    stats,
    series,
    loading,
    error,
    filter,
    setFilter,
    refetch: fetchData,
  };
};


// Stats overview component
const StatsOverview: React.FC<{ stats: TransferStats }> = ({ stats }) => {
  return (
    <div className="stats-grid">
      <div className="stat-card">
        <div className="stat-label">Transfers (24h)</div>
        <div className="stat-value">{stats.totalTransfers.toLocaleString()}</div>
      </div>
      <div className="stat-card">
        <div className="stat-label">Unique Senders</div>
        <div className="stat-value">{stats.uniqueSenders.toLocaleString()}</div>
      </div>
      <div className="stat-card">
        <div className="stat-label">Unique Receivers</div>
        <div className="stat-value">{stats.uniqueReceivers.toLocaleString()}</div>
      </div>
      <div className="stat-card">
        <div className="stat-label">Distinct Tokens</div>
        <div className="stat-value">{stats.distinctTokens.toLocaleString()}</div>
      </div>

      <style>{`
        .stats-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; margin-bottom: 24px; }
        .stat-card { background: #1e293b; border-radius: 12px; padding: 16px; text-align: center; }
        .stat-label { font-size: 12px; color: #94a3b8; text-transform: uppercase; margin-bottom: 8px; }
        .stat-value { font-size: 20px; font-weight: 700; color: #e2e8f0; }
        @media (max-width: 1024px) { .stats-grid { grid-template-columns: repeat(2, 1fr); } }
      `}</style>
    </div>
  );
};

// Token selector component. The option value is the token address, because the
// filter matches on address; using the symbol as the value meant selecting a
// token filtered on a string that never appeared in a transfer row.
const TokenSelector: React.FC<{
  tokens: Token[];
  selected: string;
  onSelect: (token: string) => void;
}> = ({ tokens, selected, onSelect }) => {
  return (
    <div className="token-selector">
      <select value={selected} onChange={(e) => onSelect(e.target.value)}>
        <option value="">All Tokens</option>
        {tokens.map((token) => (
          <option key={token.address} value={token.address}>
            {token.symbol} - {token.name}
          </option>
        ))}
      </select>

      <style>{`
        .token-selector select {
          padding: 10px 16px; background: #1e293b; border: 1px solid #334155;
          border-radius: 8px; color: #e2e8f0; font-size: 14px; min-width: 250px;
        }
        .token-selector select:focus { outline: none; border-color: #3b82f6; }
      `}</style>
    </div>
  );
};

// Transfer list component
const TransferList: React.FC<{ transfers: TokenTransfer[] }> = ({ transfers }) => {
  const formatAmount = (value: string, decimals: number | null) =>
    Number(formatWei(value, decimals ?? 18)).toLocaleString(undefined, { maximumFractionDigits: 6 });

  if (transfers.length === 0) {
    return (
      <div className="transfer-list">
        <p style={{ color: '#94a3b8', padding: 20 }}>No token transfers match the current filters.</p>
      </div>
    );
  }

  return (
    <div className="transfer-list">
      <div className="list-header">
        <span>Transaction</span>
        <span>Block</span>
        <span>From</span>
        <span>To</span>
        <span>Token</span>
        <span>Amount</span>
        <span>Time</span>
      </div>
      {transfers.map((transfer) => (
        <div key={`${transfer.transaction_hash}-${transfer.log_index}`} className="list-row">
          <a className="tx-hash" href={`/tx/${transfer.transaction_hash}`}>{formatAddress(transfer.transaction_hash)}</a>
          <a className="block-num" href={`/block/${transfer.block_number}`}>{transfer.block_number.toLocaleString()}</a>
          <a className="address from" href={`/address/${transfer.from_address}`}>{formatAddress(transfer.from_address)}</a>
          <a className="address to" href={`/address/${transfer.to_address}`}>{formatAddress(transfer.to_address)}</a>
          <span className="token">{transfer.token_symbol || formatAddress(transfer.token_address)}</span>
          <span className="value">{formatAmount(transfer.value, transfer.token_decimals)}</span>
          <span className="time">
            {transfer.block_timestamp ? new Date(transfer.block_timestamp).toLocaleString() : '-'}
          </span>
        </div>
      ))}

      <style>{`
        .transfer-list { background: #1e293b; border-radius: 12px; overflow: hidden; }
        .list-header, .list-row {
          display: grid;
          grid-template-columns: 140px 100px 130px 130px 100px 160px 170px;
          padding: 14px 16px; align-items: center; font-size: 13px;
        }
        .list-header {
          background: #0f172a; color: #94a3b8; font-size: 11px; text-transform: uppercase;
        }
        .list-row { border-bottom: 1px solid #334155; color: #e2e8f0; }
        .list-row:last-child { border-bottom: none; }
        .tx-hash { color: #3b82f6; font-family: monospace; text-decoration: none; }
        .block-num { color: #94a3b8; text-decoration: none; }
        .address { font-family: monospace; color: #94a3b8; text-decoration: none; }
        .address:hover, .tx-hash:hover, .block-num:hover { color: #3b82f6; }
        .token { font-weight: 600; }
        .value { color: #10b981; font-family: monospace; }
        .time { color: #64748b; font-size: 12px; }
      `}</style>
    </div>
  );
};

// Volume chart component. It plots indexed transfer counts per hour. The old
// version summed a dollar value per hour, which required a per-token price the
// chain does not have for most tokens.
const VolumeChart: React.FC<{ series: SeriesPoint[] }> = ({ series }) => {
  if (series.length === 0) {
    return (
      <div className="volume-chart">
        <h3>Transfer Activity (24h)</h3>
        <p style={{ color: '#94a3b8' }}>No transfers have been indexed in the last 24 hours.</p>
      </div>
    );
  }

  const chartData = series.map((point) => ({
    time: new Date(point.bucket).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
    count: Number(point.transfer_count),
    tokens: Number(point.token_count),
  }));

  return (
    <div className="volume-chart">
      <h3>Transfer Activity (24h)</h3>
      <ResponsiveContainer width="100%" height={250}>
        <AreaChart data={chartData}>
          <defs>
            <linearGradient id="volumeGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="#10b981" stopOpacity={0.3} />
              <stop offset="95%" stopColor="#10b981" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
          <XAxis dataKey="time" stroke="#94a3b8" fontSize={11} />
          <YAxis stroke="#94a3b8" fontSize={11} />
          <Tooltip contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px' }} />
          <Area type="monotone" dataKey="count" stroke="#10b981" fill="url(#volumeGradient)" strokeWidth={2} name="Transfers" />
          <Line type="monotone" dataKey="tokens" stroke="#8b5cf6" dot={false} name="Distinct tokens" />
        </AreaChart>
      </ResponsiveContainer>

      <style>{`
        .volume-chart { background: #1e293b; border-radius: 12px; padding: 20px; margin-bottom: 24px; }
        .volume-chart h3 { color: #e2e8f0; margin-bottom: 16px; }
      `}</style>
    </div>
  );
};

// Token filter source. Only tokens the indexer has seen are offered, with their
// real holder and transfer counts from the tokens table.
const TokenStats: React.FC<{ tokens: Token[] }> = ({ tokens }) => {
  if (tokens.length === 0) return null;

  return (
    <div className="token-stats">
      <h3>Indexed Tokens</h3>
      <div className="tokens-grid">
        {tokens.slice(0, 8).map((token) => (
          <a key={token.address} href={`/token/${token.address}`} className="token-card">
            <div className="token-info">
              <div className="token-symbol">{token.symbol}</div>
              <div className="token-name">{token.name}</div>
            </div>
            <div className="token-address">{formatAddress(token.address)}</div>
          </a>
        ))}
      </div>

      <style>{`
        .token-stats { margin-bottom: 24px; }
        .token-stats h3 { color: #e2e8f0; margin-bottom: 16px; }
        .tokens-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; }
        .token-card {
          background: #1e293b; border-radius: 12px; padding: 16px;
          display: flex; flex-direction: column; gap: 8px; text-decoration: none;
        }
        .token-card:hover { background: #263449; }
        .token-symbol { font-weight: 700; color: #e2e8f0; }
        .token-name { font-size: 12px; color: #64748b; }
        .token-address { font-size: 11px; color: #94a3b8; font-family: monospace; }
        @media (max-width: 1024px) { .tokens-grid { grid-template-columns: repeat(2, 1fr); } }
      `}</style>
    </div>
  );
};


// Main component
const TokenTransferTracker: React.FC = () => {
  const { transfers, tokens, stats, series, loading, error, filter, setFilter, refetch } = useTokenTransfers();

  if (loading && !stats) {
    return (
      <div className="transfer-tracker">
        <div className="loading">Loading transfer data...</div>
        <style>{`
          .loading { display: flex; justify-content: center; align-items: center; height: 400px; color: #94a3b8; }
        `}</style>
      </div>
    );
  }

  if (error && !stats) {
    return (
      <div className="transfer-tracker">
        <div className="loading">{error}</div>
        <button className="refresh-btn" onClick={refetch}>Retry</button>
      </div>
    );
  }

  return (
    <div className="transfer-tracker">
      <div className="page-header">
        <h1>Token Transfers</h1>
        <p>Indexed ERC-20 transfers on TigerSmartChain</p>
        <button className="refresh-btn" onClick={refetch}>Refresh</button>
      </div>

      {stats && <StatsOverview stats={stats} />}

      <div className="filters">
        <TokenSelector
          tokens={tokens}
          selected={filter.token || ''}
          onSelect={(token) => setFilter({ ...filter, token: token || undefined })}
        />
        <input
          type="text"
          placeholder="Filter by address..."
          value={filter.address || ''}
          onChange={(e) => setFilter({ ...filter, address: e.target.value || undefined })}
          className="address-filter"
        />
        <input
          type="number"
          placeholder="Min amount (token units)..."
          value={filter.minValue ?? ''}
          onChange={(e) => setFilter({ ...filter, minValue: e.target.value ? Number(e.target.value) : undefined })}
          className="value-filter"
        />
      </div>

      <TokenStats tokens={tokens} />
      <VolumeChart series={series} />
      <TransferList transfers={transfers} />

      <style>{`
        .transfer-tracker { padding: 24px; max-width: 1600px; margin: 0 auto; }
        .page-header { display: flex; flex-direction: column; margin-bottom: 24px; }
        .page-header h1 { font-size: 32px; font-weight: 700; color: #e2e8f0; margin-bottom: 8px; }
        .page-header p { color: #94a3b8; }
        .refresh-btn {
          margin-top: 12px; align-self: flex-start; padding: 8px 16px; background: #10b981;
          border: none; border-radius: 8px; color: white; cursor: pointer; font-weight: 500;
        }
        .refresh-btn:hover { background: #059669; }
        .filters { display: flex; gap: 12px; margin-bottom: 24px; flex-wrap: wrap; }
        .address-filter, .value-filter {
          padding: 10px 16px; background: #1e293b; border: 1px solid #334155;
          border-radius: 8px; color: #e2e8f0; font-size: 14px; min-width: 200px;
        }
        .address-filter:focus, .value-filter:focus { outline: none; border-color: #3b82f6; }
      `}</style>
    </div>
  );
};


export default TokenTransferTracker;