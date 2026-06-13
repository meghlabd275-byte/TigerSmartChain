// MEV Dashboard Component
// Production-grade real-time MEV transaction dashboard

import React, { useEffect, useState } from 'react';
import { useQuery } from 'react-query';

interface MEVTransaction {
  hash: string;
  type: 'sandwich' | 'arbitrage' | 'liquidate' | 'flash_loan';
  profit: number;
  blockNumber: number;
  timestamp: number;
  gasUsed: number;
  gasPrice: number;
}

interface MEVDashboardProps {
  height?: number;
  refreshInterval?: number;
}

const API_BASE = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

async function fetchMEVTransactions(limit: number = 50): Promise<MEVTransaction[]> {
  const response = await fetch(`${API_BASE}/api/v1/mev/transactions?limit=${limit}`);
  if (!response.ok) throw new Error('Failed to fetch');
  return response.json();
}

export default function MEVDashboard({ height = 600, refreshInterval = 10000 }: MEVDashboardProps) {
  const { data, isLoading, error, refetch } = useQuery<MEVTransaction[]>(
    'mev-transactions',
    () => fetchMEVTransactions(50),
    { refetchInterval: refreshInterval }
  );

  const [filter, setFilter] = useState<string>('all');

  const filteredData = data?.filter(tx => filter === 'all' || tx.type === filter) || [];

  if (isLoading) {
    return (
      <div style={{ height, display: 'flex', alignItems: 'center', justifyContent: 'center', backgroundColor: '#f9fafb', borderRadius: 8 }}>
        <p>Loading MEV data...</p>
      </div>
    );
  }

  if (error || !data) {
    return (
      <div style={{ height, display: 'flex', alignItems: 'center', justifyContent: 'center', backgroundColor: '#fef2f2', borderRadius: 8 }}>
        <p style={{ color: '#ef4444' }}>Failed to load MEV data</p>
      </div>
    );
  }

  return (
    <div style={{ height }}>
      {/* Header */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <div>
          <h2 style={{ fontSize: 20, fontWeight: 600, margin: 0 }}>MEV Dashboard</h2>
          <p style={{ fontSize: 12, color: '#6b7280', margin: '4px 0 0' }}>
            Real-time MEV transaction tracking
          </p>
        </div>
        <button onClick={() => refetch()} style={{ padding: '8px 16px', backgroundColor: '#3b82f6', color: '#fff', border: 'none', borderRadius: 6, cursor: 'pointer' }}>
          Refresh
        </button>
      </div>

      {/* Stats */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12, marginBottom: 24 }}>
        <StatCard label="Total MEV" value={data.length.toString()} />
        <StatCard label="Sandwich" value={data.filter(t => t.type === 'sandwich').length.toString()} />
        <StatCard label="Arbitrage" value={data.filter(t => t.type === 'arbitrage').length.toString()} />
        <StatCard label="Liquidations" value={data.filter(t => t.type === 'liquidate').length.toString()} />
      </div>

      {/* Filter */}
      <div style={{ display: 'flex', gap: 8, marginBottom: 16 }}>
        {['all', 'sandwich', 'arbitrage', 'liquidate', 'flash_loan'].map(type_ => (
          <button
            key={type_}
            onClick={() => setFilter(type_)}
            style={{
              padding: '6px 12px',
              backgroundColor: filter === type_ ? '#3b82f6' : '#fff',
              color: filter === type_ ? '#fff' : '#374151',
              border: '1px solid #e5e7eb',
              borderRadius: 6,
              fontSize: 12,
              cursor: 'pointer',
            }}
          >
            {type_ === 'all' ? 'All' : type_.charAt(0).toUpperCase() + type_.slice(1)}
          </button>
        ))}
      </div>

      {/* Table */}
      <div style={{ overflowY: 'auto', maxHeight: height - 250 }}>
        <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
          <thead>
            <tr style={{ backgroundColor: '#f9fafb' }}>
              <th style={{ padding: '10px 12px', textAlign: 'left', fontWeight: 500 }}>Type</th>
              <th style={{ padding: '10px 12px', textAlign: 'left', fontWeight: 500 }}>Hash</th>
              <th style={{ padding: '10px 12px', textAlign: 'right', fontWeight: 500 }}>Profit</th>
              <th style={{ padding: '10px 12px', textAlign: 'right', fontWeight: 500 }}>Block</th>
              <th style={{ padding: '10px 12px', textAlign: 'right', fontWeight: 500 }}>Gas</th>
            </tr>
          </thead>
          <tbody>
            {filteredData.map((tx) => (
              <tr key={tx.hash} style={{ borderBottom: '1px solid #f3f4f6' }}>
                <td style={{ padding: '10px 12px' }}>
                  <span style={{
                    padding: '4px 8px',
                    borderRadius: 4,
                    backgroundColor: tx.type === 'sandwich' ? '#fef3c7' : tx.type === 'arbitrage' ? '#dbeafe' : '#d1fae5',
                    color: tx.type === 'sandwich' ? '#92400e' : tx.type === 'arbitrage' ? '#1e40af' : '#065f46',
                    fontSize: 11,
                    fontWeight: 500,
                  }}>
                    {tx.type}
                  </span>
                </td>
                <td style={{ padding: '10px 12px', fontFamily: 'monospace', fontSize: 12 }}>
                  {tx.hash.slice(0, 10)}...
                </td>
                <td style={{ padding: '10px 12px', textAlign: 'right', fontWeight: 500, color: '#10b981' }}>
                  ${tx.profit.toFixed(2)}
                </td>
                <td style={{ padding: '10px 12px', textAlign: 'right' }}>{tx.blockNumber.toLocaleString()}</td>
                <td style={{ padding: '10px 12px', textAlign: 'right' }}>{tx.gasUsed.toLocaleString()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ padding: 16, backgroundColor: '#f9fafb', borderRadius: 8, textAlign: 'center' }}>
      <p style={{ fontSize: 12, color: '#6b7280', margin: '0 0 4px' }}>{label}</p>
      <p style={{ fontSize: 24, fontWeight: 600, margin: 0 }}>{value}</p>
    </div>
  );
}