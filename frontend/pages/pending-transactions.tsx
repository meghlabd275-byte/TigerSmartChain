/**
 * Pending Transactions - Mempool viewer backed by the indexed pending pool.
 * Values come from GET /api/v1/txs/pending; nothing is generated client-side.
 */

import React, { useState, useEffect, useCallback } from 'react';
import { api } from '@/lib/api';

interface PendingTransaction {
  id: string;
  hash: string;
  from: string;
  to: string;
  value: string;
  gasPrice: number;
  gasLimit: number;
  nonce: number;
  input: string;
}

interface MempoolStats {
  totalPending: number;
  avgGasPrice: number;
  totalValue: number;
}

// The method is the first four bytes of the calldata.
const methodName = (input: string): string => {
  if (!input || input.length < 10) return 'transfer';
  const selector = input.slice(0, 10).toLowerCase();
  const known: Record<string, string> = {
    '0xa9059cbb': 'transfer',
    '0x095ea7b3': 'approve',
    '0x7ff36ab5': 'swapExactETHForTokens',
    '0x38ed1739': 'swapExactTokensForETH',
    '0x4e71d92d': 'claim',
    '0x2e1a7d4d': 'withdraw',
    '0xa1428175': 'mint',
  };
  return known[selector] ?? selector;
};

const fromWei = (wei: string): number => Number(wei) / 1e18;

const usePendingTransactions = () => {
  const [transactions, setTransactions] = useState<PendingTransaction[]>([]);
  const [stats, setStats] = useState<MempoolStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    try {
      const data = await api.getPendingTransactions();
      const txs = (data as any[]).map((tx) => ({
        id: tx.hash,
        hash: tx.hash,
        from: tx.from_address ?? tx.from ?? '',
        to: tx.to_address ?? tx.to ?? '',
        value: String(tx.value ?? '0'),
        gasPrice: Number(tx.gas_price ?? tx.gasPrice ?? 0),
        gasLimit: Number(tx.gas_limit ?? tx.gasLimit ?? 0),
        nonce: Number(tx.nonce ?? 0),
        input: tx.input ?? '',
      }));
      setTransactions(txs);

      const avgGasPrice = txs.length
        ? txs.reduce((acc, t) => acc + t.gasPrice, 0) / txs.length
        : 0;
      setStats({
        totalPending: txs.length,
        avgGasPrice: avgGasPrice / 1e9,
        totalValue: txs.reduce((acc, t) => acc + fromWei(t.value), 0),
      });
      setError(null);
    } catch (err) {
      setError('Failed to load the pending pool');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 5000);
    return () => clearInterval(interval);
  }, [fetchData]);

  return { transactions, stats, loading, error, refetch: fetchData };
};

const StatsCards: React.FC<{ stats: MempoolStats }> = ({ stats }) => (
  <div className="stats-grid">
    <div className="stat-card">
      <div className="stat-label">Pending Txs</div>
      <div className="stat-value">{stats.totalPending}</div>
    </div>
    <div className="stat-card">
      <div className="stat-label">Avg Gas Price</div>
      <div className="stat-value">{stats.avgGasPrice.toFixed(1)} Gwei</div>
    </div>
    <div className="stat-card">
      <div className="stat-label">Total Value</div>
      <div className="stat-value">{stats.totalValue.toFixed(4)} TGR</div>
    </div>
    <style>{`
      .stats-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; margin-bottom: 24px; }
      .stat-card { background: #1e293b; border-radius: 12px; padding: 20px; text-align: center; }
      .stat-label { font-size: 12px; color: #94a3b8; text-transform: uppercase; margin-bottom: 8px; }
      .stat-value { font-size: 24px; font-weight: 700; color: #e2e8f0; }
    `}</style>
  </div>
);

const TransactionRow: React.FC<{ tx: PendingTransaction }> = ({ tx }) => {
  const formatAddress = (addr: string) => (addr ? addr.slice(0, 10) + '...' + addr.slice(-8) : '—');
  return (
    <div className="tx-row">
      <span className="method">{methodName(tx.input)}</span>
      <span className="hash">{formatAddress(tx.hash)}</span>
      <span className="from">{formatAddress(tx.from)}</span>
      <span className="to">{formatAddress(tx.to)}</span>
      <span className="value">{fromWei(tx.value).toFixed(4)} TGR</span>
      <span className="gas">{(tx.gasPrice / 1e9).toFixed(1)} Gwei</span>
      <span className="nonce">{tx.nonce}</span>
      <style>{`
        .tx-row {
          display: grid;
          grid-template-columns: 150px 120px 100px 100px 110px 90px 60px;
          padding: 14px 16px;
          background: #1e293b;
          border-radius: 8px;
          margin-bottom: 8px;
          align-items: center;
          font-size: 13px;
        }
        .method { color: #8b5cf6; font-weight: 600; }
        .hash { color: #3b82f6; font-family: monospace; }
        .from, .to { color: #94a3b8; font-family: monospace; }
        .value { color: #10b981; font-weight: 600; }
        .gas { color: #f59e0b; }
        .nonce { color: #64748b; }
      `}</style>
    </div>
  );
};

const PendingTransactions: React.FC = () => {
  const { transactions, stats, loading, error, refetch } = usePendingTransactions();

  if (loading) return <div className="loading">Loading mempool...</div>;

  return (
    <div className="pending-page">
      <div className="header">
        <h1>📋 Pending Transactions</h1>
        <p>Transactions in the pending pool, refreshed every 5 seconds</p>
        <button onClick={refetch}>↻ Refresh</button>
      </div>
      {error && <div className="error">{error}</div>}
      {stats && <StatsCards stats={stats} />}
      {!error && transactions.length === 0 && (
        <div className="empty">The pending pool is empty.</div>
      )}
      {transactions.length > 0 && (
        <div className="table-header">
          <span>Method</span>
          <span>Hash</span>
          <span>From</span>
          <span>To</span>
          <span>Value</span>
          <span>Gas</span>
          <span>Nonce</span>
        </div>
      )}
      {transactions.map(tx => <TransactionRow key={tx.id} tx={tx} />)}
      <style>{`
        .pending-page { padding: 24px; max-width: 1400px; margin: 0 auto; }
        .header { display: flex; flex-direction: column; margin-bottom: 24px; }
        .header h1 { font-size: 32px; color: #e2e8f0; margin-bottom: 8px; }
        .header p { color: #94a3b8; }
        .header button { margin-top: 12px; align-self: flex-start; padding: 8px 16px; background: #3b82f6; border: none; border-radius: 8px; color: white; cursor: pointer; }
        .table-header { display: grid; grid-template-columns: 150px 120px 100px 100px 110px 90px 60px; padding: 12px 16px; color: #94a3b8; font-size: 11px; text-transform: uppercase; }
        .loading, .empty { padding: 40px; text-align: center; color: #94a3b8; }
        .error { padding: 16px; background: #7f1d1d; color: #fecaca; border-radius: 8px; margin-bottom: 16px; }
      `}</style>
    </div>
  );
};

export default PendingTransactions;
