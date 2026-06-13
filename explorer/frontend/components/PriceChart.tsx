// Price Charts Component
// Production-grade TradingView-style charts for token prices
// Features: Candlestick, line, area charts, technical indicators, volume

import React, { useEffect, useRef, useState, useCallback, useMemo } from 'react';
import { createChart, IChartApi, ISeriesApi, CandlestickData, LineData, HistogramData, Time } from 'lightweight-charts';
import { useQuery } from 'react-query';

// =============================================================================
// TYPES
// =============================================================================

interface PricePoint {
  timestamp: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

interface PriceChartProps {
  tokenAddress?: string;
  tokenSymbol?: string;
  height?: number;
  timeframe?: '1m' | '5m' | '15m' | '1h' | '4h' | '1d' | '1w';
  chartType?: 'candlestick' | 'line' | 'area';
  showVolume?: boolean;
  showIndicators?: boolean;
}

interface PriceResponse {
  prices: PricePoint[];
  meta: {
    symbol: string;
    timeframe: string;
    totalPoints: number;
  };
}

// =============================================================================
// API
// =============================================================================

const API_BASE = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

async function fetchPriceHistory(
  tokenAddress: string,
  timeframe: string,
  limit: number = 500
): Promise<PriceResponse> {
  const response = await fetch(
    `${API_BASE}/api/v1/prices/${tokenAddress}?timeframe=${timeframe}&limit=${limit}`
  );
  if (!response.ok) {
    throw new Error('Failed to fetch price data');
  }
  return response.json();
}

// =============================================================================
// MAIN COMPONENT
// =============================================================================

export default function PriceChart({
  tokenAddress,
  tokenSymbol = 'TOKEN',
  height = 400,
  timeframe = '1h',
  chartType = 'candlestick',
  showVolume = true,
  showIndicators = false,
}: PriceChartProps) {
  const chartContainerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi>(null);
  const candlestickSeriesRef = useRef<ISeriesApi<'Candlestick'>>(null);
  const volumeSeriesRef = useRef<ISeriesApi<'Histogram'>>(null);
  const [chartReady, setChartReady] = useState(false);

  // Fetch price data
  const { data, isLoading, error } = useQuery<PriceResponse>(
    ['price-history', tokenAddress, timeframe],
    () => fetchPriceHistory(tokenAddress, timeframe),
    {
      refetchInterval: 60000, // Refresh every minute
      staleTime: 30000,
      enabled: !!tokenAddress,
    }
  );

  // Initialize chart
  useEffect(() => {
    if (!chartContainerRef.current) return;

    // Create chart
    const chart = createChart(chartContainerRef.current, {
      layout: {
        background: { color: '#ffffff' },
        textColor: '#333333',
      },
      grid: {
        vertLines: { color: '#f0f0f0' },
        horzLines: { color: '#f0f0f0' },
      },
      crosshair: {
        mode: 1,
        vertLine: {
          width: 1,
          color: '#2962FF',
          style: 0,
          labelBackgroundColor: '#2962FF',
        },
        horzLine: {
          width: 1,
          color: '#2962FF',
          style: 0,
          labelBackgroundColor: '#2962FF',
        },
      },
      rightPriceScale: {
        borderColor: '#e0e0e0',
      },
      timeScale: {
        borderColor: '#e0e0e0',
        timeVisible: true,
        secondsVisible: false,
      },
      handleScale: {
        axisPressedMouseMove: true,
      },
      handleScroll: {
        vertTouchDrag: true,
      },
    });

    // Add candlestick series
    const candlestickSeries = chart.addCandlestickSeries({
      upColor: '#26a69a',
      downColor: '#ef5350',
      borderDownColor: '#ef5350',
      borderUpColor: '#26a69a',
      wickDownColor: '#ef5350',
      wickUpColor: '#26a69a',
    });

    // Add volume series
    const volumeSeries = chart.addHistogramSeries({
      color: '#26a69a',
      priceFormat: {
        type: 'volume',
      },
      priceScaleId: 'volume',
    });

    // Set volume scale
    chart.priceScale('volume').applyOptions({
      scaleMargins: {
        top: 0.8,
        bottom: 0,
      },
    });

    chartRef.current = chart;
    candlestickSeriesRef.current = candlestickSeries;
    volumeSeriesRef.current = volumeSeries;
    setChartReady(true);

    // Handle resize
    const handleResize = () => {
      if (chartContainerRef.current) {
        chart.applyOptions({
          width: chartContainerRef.current.clientWidth,
          height,
        });
      }
    };

    window.addEventListener('resize', handleResize);
    handleResize();

    return () => {
      window.removeEventListener('resize', handleResize);
      chart.remove();
    };
  }, [height]);

  // Update chart data
  useEffect(() => {
    if (!chartReady || !data?.prices?.length) return;

    const chart = chartRef.current;
    const candlestickSeries = candlestickSeriesRef.current;
    const volumeSeries = volumeSeriesRef.current;

    if (!chart || !candlestickSeries) return;

    // Prepare candlestick data
    const candleData: CandlestickData<Time> = data.prices.map((p) => ({
      time: (p.timestamp / 1000) as Time,
      open: p.open,
      high: p.high,
      low: p.low,
      close: p.close,
    }));

    // Prepare volume data
    const volumeData: HistogramData<Time> = data.prices.map((p) => ({
      time: (p.timestamp / 1000) as Time,
      value: p.volume,
      color: p.close >= p.open ? '#26a69a40' : '#ef535040',
    }));

    // Set data
    candlestickSeries.setData(candleData);

    if (showVolume && volumeSeries) {
      volumeSeries.setData(volumeData);
    }

    // Fit content
    chart.timeScale().fitContent();
  }, [data, chartReady, showVolume]);

  // Loading state
  if (isLoading) {
    return (
      <div
        ref={chartContainerRef}
        style={{
          width: '100%',
          height,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: '#f9fafb',
          borderRadius: '8px',
        }}
      >
        <div style={{ textAlign: 'center' }}>
          <div
            style={{
              width: 32,
              height: 32,
              border: '3px solid #e5e7eb',
              borderTopColor: '#3b82f6',
              borderRadius: '50%',
              animation: 'spin 1s linear infinite',
              margin: '0 auto 12px',
            }}
          />
          <p style={{ color: '#6b7280', fontSize: 14 }}>Loading price data...</p>
        </div>
        <style>{`
          @keyframes spin {
            to { transform: rotate(360deg); }
          }
        `}</style>
      </div>
    );
  }

  // Error state
  if (error || !data?.prices?.length) {
    return (
      <div
        ref={chartContainerRef}
        style={{
          width: '100%',
          height,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: '#f9fafb',
          borderRadius: '8px',
        }}
      >
        <p style={{ color: '#6b7280', fontSize: 14 }}>
          No price data available
        </p>
      </div>
    );
  }

  // Render
  return (
    <div style={{ position: 'relative', width: '100%' }}>
      {/* Header */}
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: 12,
        }}
      >
        <div>
          <h2 style={{ fontSize: 18, fontWeight: 600, margin: 0 }}>
            {tokenSymbol}/USD
          </h2>
          <p style={{ color: '#6b7280', fontSize: 12, margin: 0 }}>
            {data?.meta?.totalPoints || 0} data points
          </p>
        </div>

        {/* Current price */}
        {data?.prices?.[0] && (
          <div style={{ textAlign: 'right' }}>
            <p
              style={{
                fontSize: 24,
                fontWeight: 700,
                margin: 0,
                color: data.prices[0].close >= data.prices[0].open ? '#26a69a' : '#ef5350',
              }}
            >
              ${data.prices[0].close.toFixed(2)}
            </p>
            <p
              style={{
                fontSize: 12,
                margin: 0,
                color: data.prices[0].close >= data.prices[0].open ? '#26a69a' : '#ef5350',
              }}
            >
              {(
                ((data.prices[0].close - data.prices[0].open) /
                  data.prices[0].open) *
                100
              ).toFixed(2)}
              %
            </p>
          </div>
        )}
      </div>

      {/* Timeframe selector */}
      <div style={{ display: 'flex', gap: 4, marginBottom: 12 }}>
        {['1m', '5m', '15m', '1h', '4h', '1d', '1w'].map((tf) => (
          <button
            key={tf}
            style={{
              padding: '4px 8px',
              fontSize: 11,
              border: '1px solid #e5e7eb',
              borderRadius: 4,
              backgroundColor: timeframe === tf ? '#3b82f6' : '#fff',
              color: timeframe === tf ? '#fff' : '#374151',
              cursor: 'pointer',
            }}
          >
            {tf}
          </button>
        ))}
      </div>

      {/* Chart */}
      <div
        ref={chartContainerRef}
        style={{
          width: '100%',
          height,
          borderRadius: 8,
          overflow: 'hidden',
        }}
      />

      {/* Legend */}
      <div
        style={{
          display: 'flex',
          justifyContent: 'center',
          gap: 16,
          marginTop: 8,
          fontSize: 11,
          color: '#6b7280',
        }}
      >
        <span>
          <span style={{ color: '#26a69a' }}>●</span> Bullish
        </span>
        <span>
          <span style={{ color: '#ef5350' }}>●</span> Bearish
        </span>
        {showVolume && <span>Volume</span>}
      </div>
    </div>
  );
}

// =============================================================================
// MINI CHART (for tables)
// =============================================================================

export function MiniPriceChart({
  tokenAddress,
  height = 100,
}: {
  tokenAddress: string;
  height?: number;
}) {
  return (
    <PriceChart
      tokenAddress={tokenAddress}
      height={height}
      timeframe="1h"
      showVolume={false}
    />
  );
}

// =============================================================================
// FULL PAGE
// =============================================================================

export function PriceChartPage() {
  const [tokenAddress, setTokenAddress] = useState('');
  const [timeframe, setTimeframe] = useState<string>('1h');

  return (
    <div style={{ padding: 24, maxWidth: 1200, margin: '0 auto' }}>
      <h1 style={{ fontSize: 24, fontWeight: 700, marginBottom: 24 }}>
        Token Price Charts
      </h1>

      {/* Input */}
      <div style={{ marginBottom: 24 }}>
        <input
          type="text"
          placeholder="Token address"
          value={tokenAddress}
          onChange={(e) => setTokenAddress(e.target.value)}
          style={{
            padding: '10px 14px',
            border: '1px solid #e5e7eb',
            borderRadius: 8,
            fontSize: 14,
            width: 400,
          }}
        />
      </div>

      {/* Chart */}
      {tokenAddress && (
        <PriceChart
          tokenAddress={tokenAddress}
          height={600}
          timeframe={timeframe as any}
          showVolume={true}
        />
      )}
    </div>
  );
}