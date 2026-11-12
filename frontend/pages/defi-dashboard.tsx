/**
 * DeFi Dashboard
 *
 * Reads the DeFi tables the indexer maintains: defi_tvl and defi_tvl_history
 * for protocol TVL, lending_rates for money-market rates, and dex_pairs for
 * liquidity, volume and fees.
 *
 * Every figure on this page used to be a literal in the source. The protocol
 * list was Aave, Compound, Uniswap, Curve, MakerDAO, Yearn, Convex, SushiSwap,
 * Lido and Rocket Pool carrying their real mainnet TVL, volume, fee and user
 * counts; the pools table had eight pairs with invented APYs; the lending cards
 * had six invented supply and borrow rates; the 30-day TVL chart walked from an
 * invented 85 billion with Math.random; and "TVL by Chain" listed Ethereum,
 * Arbitrum, Optimism, Polygon, Base and Avalanche. None of those protocols run
 * here and TigerScan cannot measure another chain's TVL, so the page was
 * reporting other networks' numbers under this chain's name.
 *
 * Two sections are gone rather than replaced. "TVL by Category" needs a
 * category column that defi_tvl does not have, and "TVL by Chain" is about
 * chains this explorer does not index.
 */

import React, { useState, useEffect, useCallback } from 'react';
import { AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';
import api from '@/lib/api';
import { formatAddress } from '@/lib/utils';

interface DeFiSummary {
  protocolTvl: string | number;
  protocolCount: number;
  protocolVolume24h: string | number;
  dexLiquidity: string | number;
  dexVolume24h: string | number;
  dexFees24h: string | number;
  pairCount: number;
  lendingMarketCount: number;
}

interface Protocol {
  protocol: string;
  tvl: string | number;
  tvl_change_24h: string | number | null;
  volume_24h: string | number | null;
  updated_at: string | null;
}

interface TVLPoint {
  date: string;
  tvl: string | number;
  protocol_count: number;
}

interface LendingMarket {
  protocol: string;
  token: string;
  supply_rate: string | number;
  borrow_rate: string | number;
  utilization: string | number;
  total_supply: string | number | null;
  total_borrow: string | number | null;
}

interface Pool {
  pair_address: string;
  token0_symbol: string | null;
  token1_symbol: string | null;
  token0_address: string;
  token1_address: string;
  liquidity_usd: string | number | null;
  volume_24h: string | number | null;
  fee_24h: string | number | null;
  fee_yield_annual_pct: string | number | null;
  pair_type: string | null;
}

const useDeFiData = () => {
  const [summary, setSummary] = useState<DeFiSummary | null>(null);
  const [protocols, setProtocols] = useState<Protocol[]>([]);
  const [tvlHistory, setTvlHistory] = useState<TVLPoint[]>([]);
  const [markets, setMarkets] = useState<LendingMarket[]>([]);
  const [pools, setPools] = useState<Pool[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchDeFiData = useCallback(async () => {
    try {
      const [summaryResponse, protocolsResponse, historyResponse, lendingResponse, poolsResponse] =
        await Promise.all([
          api.getDeFiSummary(),
          api.getDeFiProtocols({ limit: 25 }),
          api.getDeFiTVLHistory({ days: 30 }),
          api.getDeFiLending({ limit: 50 }),
          api.getDeFiPools({ limit: 25 }),
        ]);

      setSummary(summaryResponse as unknown as DeFiSummary);
      setProtocols(protocolsResponse.items as unknown as Protocol[]);
      setTvlHistory(historyResponse.items as unknown as TVLPoint[]);
      setMarkets(lendingResponse.items as unknown as LendingMarket[]);
      setPools(poolsResponse.items as unknown as Pool[]);
      setError(null);
    } catch (err: any) {
      setError(err?.response?.data?.error || 'Could not load DeFi data.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchDeFiData();
    const interval = setInterval(fetchDeFiData, 60000);
    return () => clearInterval(interval);
  }, [fetchDeFiData]);

  return { summary, protocols, tvlHistory, markets, pools, loading, error, refetch: fetchDeFiData };
};

// Amounts arrive as numeric strings, which can be larger than a JS number can
// hold exactly. Only divide for display; never round-trip through Number first.
const scaled = (value: string | number | null | undefined, divisor: number, digits: number) => {
  if (value == null) return null;
  const raw = typeof value === 'string' ? Number(value) : value;
  if (!Number.isFinite(raw)) return null;
  return (raw / divisor).toFixed(digits);
};

const SummaryCards: React.FC<{ summary: DeFiSummary | null }> = ({ summary }) => {
  const cards = [
    {
      label: 'Protocol TVL',
      value: summary ? scaled(summary.protocolTvl, 1, 0) ?? '0' : '-',
      note: summary ? `${summary.protocolCount} protocols tracked` : '',
      main: true,
    },
    {
      label: 'DEX Liquidity',
      value: summary ? scaled(summary.dexLiquidity, 1, 0) ?? '0' : '-',
      note: summary ? `${summary.pairCount} pairs indexed` : '',
    },
    {
      label: 'DEX Volume (24h)',
      value: summary ? scaled(summary.dexVolume24h, 1, 0) ?? '0' : '-',
      note: summary ? `fees ${scaled(summary.dexFees24h, 1, 0) ?? '0'}` : '',
    },
    {
      label: 'Lending Markets',
      value: summary ? Number(summary.lendingMarketCount).toLocaleString() : '-',
      note: summary ? `protocol volume ${scaled(summary.protocolVolume24h, 1, 0) ?? '0'}` : '',
    },
  ];

  return (
    <div className="overview-cards">
      {cards.map((card) => (
        <div key={card.label} className={`overview-card${card.main ? ' main' : ''}`}>
          <div className="card-label">{card.label}</div>
          <div className="card-value">{card.value}</div>
          <div className="card-note">{card.note}</div>
        </div>
      ))}

      <style>{`
        .overview-cards { display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; margin-bottom: 24px; }
        .overview-card { background: #1e293b; border-radius: 12px; padding: 20px; }
        .overview-card.main { background: linear-gradient(135deg, #8b5cf6 0%, #6366f1 100%); }
        .card-label { font-size: 12px; color: rgba(255,255,255,0.7); text-transform: uppercase; margin-bottom: 8px; }
        .card-value { font-size: 26px; font-weight: 700; color: #fff; }
        .card-note { font-size: 12px; color: rgba(255,255,255,0.6); margin-top: 6px; }
        @media (max-width: 1024px) { .overview-cards { grid-template-columns: repeat(2, 1fr); } }
      `}</style>
    </div>
  );
};

const TVLChart: React.FC<{ history: TVLPoint[] }> = ({ history }) => {
  if (history.length === 0) {
    return (
      <div className="tvl-chart">
        <h3>Protocol TVL</h3>
        <p style={{ color: '#94a3b8' }}>No TVL history has been recorded yet.</p>
      </div>
    );
  }

  const chartData = history.map((point) => ({
    date: new Date(point.date).toLocaleDateString([], { month: 'short', day: 'numeric' }),
    tvl: Number(point.tvl),
    protocols: Number(point.protocol_count),
  }));

  return (
    <div className="tvl-chart">
      <h3>Protocol TVL (30 days)</h3>
      <ResponsiveContainer width="100%" height={250}>
        <AreaChart data={chartData}>
          <defs>
            <linearGradient id="tvlGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="#8b5cf6" stopOpacity={0.3} />
              <stop offset="95%" stopColor="#8b5cf6" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
          <XAxis dataKey="date" stroke="#94a3b8" fontSize={11} />
          <YAxis stroke="#94a3b8" fontSize={11} />
          <Tooltip contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px' }} />
          <Area type="monotone" dataKey="tvl" stroke="#8b5cf6" fill="url(#tvlGradient)" strokeWidth={2} name="TVL" />
        </AreaChart>
      </ResponsiveContainer>

      <style>{`
        .tvl-chart { background: #1e293b; border-radius: 12px; padding: 20px; margin-bottom: 24px; }
        .tvl-chart h3 { color: #e2e8f0; margin-bottom: 16px; font-size: 16px; }
      `}</style>
    </div>
  );
};

const TopProtocols: React.FC<{ protocols: Protocol[] }> = ({ protocols }) => {
  if (protocols.length === 0) {
    return (
      <div className="top-protocols">
        <h3>Protocols by TVL</h3>
        <p style={{ color: '#94a3b8' }}>No protocols are being tracked.</p>
      </div>
    );
  }

  return (
    <div className="top-protocols">
      <h3>Protocols by TVL</h3>
      <div className="protocols-list">
        {protocols.map((protocol, i) => {
          const change = protocol.tvl_change_24h == null ? null : Number(protocol.tvl_change_24h);
          return (
            <div key={protocol.protocol} className="protocol-item">
              <div className="protocol-rank">{i + 1}</div>
              <div className="protocol-info">
                <div className="protocol-name">{protocol.protocol}</div>
                <div className="protocol-volume">
                  volume {scaled(protocol.volume_24h, 1, 0) ?? '0'}
                </div>
              </div>
              <div className="protocol-tvl">{scaled(protocol.tvl, 1, 0) ?? '0'}</div>
              <div className={`protocol-change ${change != null && change >= 0 ? 'positive' : 'negative'}`}>
                {change == null ? '-' : `${change >= 0 ? '+' : ''}${change.toFixed(2)}%`}
              </div>
            </div>
          );
        })}
      </div>

      <style>{`
        .top-protocols { background: #1e293b; border-radius: 12px; padding: 20px; margin-bottom: 24px; }
        .top-protocols h3 { color: #e2e8f0; margin-bottom: 16px; font-size: 16px; }
        .protocols-list { display: flex; flex-direction: column; gap: 8px; }
        .protocol-item {
          display: grid; grid-template-columns: 32px 1fr 140px 90px;
          align-items: center; padding: 12px; background: #0f172a; border-radius: 8px;
        }
        .protocol-rank { color: #64748b; font-weight: 600; }
        .protocol-name { font-weight: 600; color: #e2e8f0; }
        .protocol-volume { font-size: 12px; color: #64748b; }
        .protocol-tvl { font-weight: 600; color: #e2e8f0; text-align: right; }
        .protocol-change { font-size: 13px; text-align: right; }
        .protocol-change.positive { color: #10b981; }
        .protocol-change.negative { color: #ef4444; }
      `}</style>
    </div>
  );
};

const LendingMarkets: React.FC<{ markets: LendingMarket[] }> = ({ markets }) => {
  if (markets.length === 0) {
    return (
      <div className="lending-markets">
        <h3>Lending Markets</h3>
        <p style={{ color: '#94a3b8' }}>No lending markets are being tracked.</p>
      </div>
    );
  }

  return (
    <div className="lending-markets">
      <h3>Lending Markets</h3>
      <div className="markets-grid">
        {markets.map((market) => {
          const utilization = Number(market.utilization || 0);
          return (
            <div key={`${market.protocol}-${market.token}`} className="market-card">
              <div className="market-header">
                <span className="market-protocol">{market.protocol}</span>
                <a className="market-token" href={`/token/${market.token}`}>{formatAddress(market.token)}</a>
              </div>
              <div className="market-rates">
                <div className="rate-item">
                  <span className="rate-label">Supply</span>
                  <span className="rate-value supply">{Number(market.supply_rate || 0).toFixed(4)}%</span>
                </div>
                <div className="rate-item">
                  <span className="rate-label">Borrow</span>
                  <span className="rate-value borrow">{Number(market.borrow_rate || 0).toFixed(4)}%</span>
                </div>
              </div>
              <div className="market-utilization">
                <span>Utilization</span>
                <div className="utilization-bar">
                  <div className="utilization-fill" style={{ width: `${Math.min(utilization, 100)}%` }}></div>
                </div>
                <span>{utilization.toFixed(2)}%</span>
              </div>
              <div className="market-liquidity">
                Supplied {scaled(market.total_supply, 1, 0) ?? '0'} / borrowed {scaled(market.total_borrow, 1, 0) ?? '0'}
              </div>
            </div>
          );
        })}
      </div>

      <style>{`
        .lending-markets { background: #1e293b; border-radius: 12px; padding: 20px; margin-bottom: 24px; }
        .lending-markets h3 { color: #e2e8f0; margin-bottom: 16px; font-size: 16px; }
        .markets-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 16px; }
        .market-card { background: #0f172a; border-radius: 12px; padding: 16px; }
        .market-header { display: flex; justify-content: space-between; margin-bottom: 12px; }
        .market-protocol { font-weight: 600; color: #e2e8f0; }
        .market-token { color: #3b82f6; font-family: monospace; font-size: 12px; text-decoration: none; }
        .market-rates { display: flex; gap: 24px; margin-bottom: 12px; }
        .rate-item { display: flex; flex-direction: column; }
        .rate-label { font-size: 12px; color: #64748b; }
        .rate-value.supply { color: #10b981; }
        .rate-value.borrow { color: #ef4444; }
        .market-utilization {
          display: flex; align-items: center; gap: 8px; font-size: 12px;
          color: #64748b; margin-bottom: 12px;
        }
        .utilization-bar { flex: 1; height: 6px; background: #334155; border-radius: 3px; overflow: hidden; }
        .utilization-fill { height: 100%; background: #3b82f6; border-radius: 3px; }
        .market-liquidity { font-size: 12px; color: #94a3b8; }
      `}</style>
    </div>
  );
};

const TopPools: React.FC<{ pools: Pool[] }> = ({ pools }) => {
  if (pools.length === 0) {
    return (
      <div className="top-pools">
        <h3>DEX Pools</h3>
        <p style={{ color: '#94a3b8' }}>No pairs have been indexed yet.</p>
      </div>
    );
  }

  return (
    <div className="top-pools">
      <h3>DEX Pools by Liquidity</h3>
      <div className="pools-table">
        <div className="table-header">
          <span>Pair</span>
          <span>Liquidity</span>
          <span>Volume (24h)</span>
          <span>Fee yield</span>
        </div>
        {pools.map((pool) => {
          const yieldPct = pool.fee_yield_annual_pct == null ? null : Number(pool.fee_yield_annual_pct);
          return (
            <a key={pool.pair_address} className="table-row" href={`/address/${pool.pair_address}`}>
              <div className="pool-pair">
                <span className="pool-tokens">
                  {pool.token0_symbol || formatAddress(pool.token0_address)}
                  {' / '}
                  {pool.token1_symbol || formatAddress(pool.token1_address)}
                </span>
                <span className="pool-address">{formatAddress(pool.pair_address)}</span>
              </div>
              <span className="pool-tvl">{scaled(pool.liquidity_usd, 1, 0) ?? '-'}</span>
              <span className="pool-volume">{scaled(pool.volume_24h, 1, 0) ?? '-'}</span>
              <span className="pool-yield">
                {yieldPct == null ? '-' : `${yieldPct.toFixed(2)}%`}
              </span>
            </a>
          );
        })}
      </div>

      <style>{`
        .top-pools { background: #1e293b; border-radius: 12px; padding: 20px; margin-bottom: 24px; }
        .top-pools h3 { color: #e2e8f0; margin-bottom: 16px; font-size: 16px; }
        .pools-table { overflow: hidden; border-radius: 8px; }
        .table-header, .table-row {
          display: grid; grid-template-columns: 2fr 1fr 1fr 1fr;
          padding: 12px 16px; align-items: center;
        }
        .table-header { background: #0f172a; color: #94a3b8; font-size: 12px; text-transform: uppercase; }
        .table-row { border-bottom: 1px solid #334155; text-decoration: none; }
        .table-row:last-child { border-bottom: none; }
        .table-row:hover { background: #16213a; }
        .pool-pair { display: flex; flex-direction: column; }
        .pool-tokens { font-weight: 600; color: #e2e8f0; }
        .pool-address { font-size: 12px; color: #64748b; font-family: monospace; }
        .pool-tvl, .pool-volume { color: #94a3b8; }
        .pool-yield { color: #10b981; font-weight: 600; }
      `}</style>
    </div>
  );
};

const DeFiDashboard: React.FC = () => {
  const { summary, protocols, tvlHistory, markets, pools, loading, error, refetch } = useDeFiData();

  if (loading && !summary) {
    return (
      <div className="defi-dashboard">
        <div className="loading-container">
          <div className="loading-spinner"></div>
          <p>Loading DeFi data...</p>
        </div>
        <style>{`
          .loading-container { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 400px; color: #94a3b8; }
          .loading-spinner {
            width: 40px; height: 40px; border: 3px solid #334155; border-top-color: #8b5cf6;
            border-radius: 50%; animation: spin 1s linear infinite;
          }
          @keyframes spin { to { transform: rotate(360deg); } }
        `}</style>
      </div>
    );
  }

  if (error && !summary) {
    return (
      <div className="defi-dashboard">
        <div className="error-container">
          <p>{error}</p>
          <button onClick={refetch}>Retry</button>
        </div>
      </div>
    );
  }

  return (
    <div className="defi-dashboard">
      <div className="page-header">
        <h1>DeFi Dashboard</h1>
        <p>Protocol TVL, DEX liquidity and lending rates indexed on TigerSmartChain</p>
        <button className="refresh-btn" onClick={refetch}>Refresh</button>
      </div>

      <SummaryCards summary={summary} />
      <TVLChart history={tvlHistory} />
      <TopProtocols protocols={protocols} />
      <TopPools pools={pools} />
      <LendingMarkets markets={markets} />

      <style>{`
        .defi-dashboard { padding: 24px; max-width: 1400px; margin: 0 auto; }
        .page-header { margin-bottom: 24px; display: flex; flex-direction: column; }
        .page-header h1 { font-size: 32px; font-weight: 700; color: #e2e8f0; margin-bottom: 8px; }
        .page-header p { color: #94a3b8; }
        .refresh-btn {
          margin-top: 12px; align-self: flex-start; padding: 8px 16px; background: #8b5cf6;
          border: none; border-radius: 8px; color: white; cursor: pointer; font-weight: 500;
        }
        .refresh-btn:hover { background: #7c3aed; }
        .error-container { text-align: center; padding: 40px; color: #ef4444; }
        .error-container button {
          margin-top: 16px; padding: 8px 16px; background: #8b5cf6; border: none;
          border-radius: 8px; color: white; cursor: pointer;
        }
      `}</style>
    </div>
  );
};

export default DeFiDashboard;
