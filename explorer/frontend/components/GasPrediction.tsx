// Gas Prediction Component
// Production-grade ML-based gas prediction

import React, { useState, useEffect, useCallback } from 'react';

interface GasPrediction {
  current: number;
  fast: number;
  standard: number;
  slow: number;
  prediction: number[];
  timestamp: number;
}

const API_BASE = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:12000';

export default function GasPrediction({ height = 400 }: { height?: number }) {
  const [timeframe, setTimeframe] = useState('1h');
  const [prediction, setPrediction] = useState<GasPrediction | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchPrediction = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetch(`${API_BASE}/api/v1/gas`);
      if (!res.ok) throw new Error(`Failed to load gas prediction (${res.status})`);
      const data = await res.json();
      setPrediction(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load gas prediction');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchPrediction();
  }, [fetchPrediction]);

  const formatGwei = (wei: number) => (wei / 1e9).toFixed(2);

  return (
    <div style={{ height }}>
      {/* Header */}
      <div style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 20, fontWeight: 600, margin: '0 0 4px' }}>Gas Prediction</h2>
        <p style={{ fontSize: 12, color: '#6b7280', margin: 0 }}>
          ML-based gas price prediction
        </p>
      </div>

      {loading && (
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: 200 }}>
          <div style={{ width: 32, height: 32, border: '3px solid #e5e7eb', borderTopColor: '#3b82f6', borderRadius: '50%', animation: 'spin 1s linear infinite' }} />
        </div>
      )}

      {!loading && error && (
        <div style={{ textAlign: 'center', padding: 24 }}>
          <p style={{ fontSize: 14, color: '#ef4444', margin: '0 0 12px' }}>{error}</p>
          <button
            onClick={fetchPrediction}
            style={{ padding: '6px 16px', backgroundColor: '#3b82f6', color: '#fff', border: 'none', borderRadius: 6, fontSize: 12, cursor: 'pointer' }}
          >
            Retry
          </button>
        </div>
      )}

      {!loading && !error && !prediction && (
        <div style={{ textAlign: 'center', padding: 24, color: '#6b7280', fontSize: 14 }}>
          No data available
        </div>
      )}

      {!loading && !error && prediction && (
        <>
          {/* Current Gas */}
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12, marginBottom: 24 }}>
            <GasCard label="Fast" gwei={prediction.fast} color="#ef4444" />
            <GasCard label="Standard" gwei={prediction.standard} color="#3b82f6" />
            <GasCard label="Slow" gwei={prediction.slow} color="#22c55e" />
            <GasCard label="Current" gwei={prediction.current} color="#8b5cf6" />
          </div>

          {/* Prediction Chart */}
          <div style={{ marginBottom: 16 }}>
            <h3 style={{ fontSize: 14, fontWeight: 500, margin: '0 0 12px' }}>12-Hour Prediction</h3>
            <div style={{ height: 200, display: 'flex', alignItems: 'flex-end', gap: 4, padding: 16, backgroundColor: '#f9fafb', borderRadius: 8 }}>
              {prediction.prediction.map((value, index) => {
                const max = Math.max(...prediction.prediction);
                const min = Math.min(...prediction.prediction);
                const heightPercent = ((value - min) / (max - min)) * 100;
                const isCurrent = index === prediction.prediction.length - 1;
                
                return (
                  <div
                    key={index}
                    style={{
                      flex: 1,
                      height: `${Math.max(heightPercent, 10)}%`,
                      backgroundColor: isCurrent ? '#8b5cf6' : '#3b82f6',
                      borderRadius: 4,
                      transition: 'height 0.3s',
                    }}
                    title={`${formatGwei(value)} Gwei`}
                  />
                );
              })}
            </div>
            <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 8, fontSize: 10, color: '#6b7280' }}>
              <span>Now</span>
              <span>+3h</span>
              <span>+6h</span>
              <span>+9h</span>
              <span>+12h</span>
            </div>
          </div>

          {/* Timeframe selector */}
          <div style={{ display: 'flex', gap: 8 }}>
            {['15m', '1h', '4h', '24h'].map(tf => (
              <button
                key={tf}
                onClick={() => setTimeframe(tf)}
                style={{
                  padding: '6px 12px',
                  backgroundColor: timeframe === tf ? '#3b82f6' : '#fff',
                  color: timeframe === tf ? '#fff' : '#374151',
                  border: '1px solid #e5e7eb',
                  borderRadius: 6,
                  fontSize: 12,
                  cursor: 'pointer',
                }}
              >
                {tf}
              </button>
            ))}
          </div>
        </>
      )}

      <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
    </div>
  );
}

function GasCard({ label, gwei, color }: { label: string; gwei: number; color: string }) {
  return (
    <div style={{ padding: 16, backgroundColor: '#f9fafb', borderRadius: 8, textAlign: 'center' }}>
      <p style={{ fontSize: 12, color: '#6b7280', margin: '0 0 4px' }}>{label}</p>
      <p style={{ fontSize: 20, fontWeight: 600, margin: 0, color }}>{(gwei / 1e9).toFixed(2)}</p>
      <p style={{ fontSize: 10, color: '#9ca3af', margin: '4px 0 0' }}>Gwei</p>
    </div>
  );
}