/**
 * TigerScan - Whale Tracking
 *
 * Large transfers, MEV bundles and flash loans, read from the indexer.
 * Reads GET /api/v1/whales, /whales/stats, /mev/bundles and /flash-loans.
 *
 * This page used to render ten hardcoded "whales" (Binance Hot, Wintermute,
 * Alameda Research) and a MEV feed built from Math.random(). None of it came
 * from the chain. The indexer does record all three kinds of activity, so the
 * page now shows what was actually indexed and nothing when there is nothing.
 */

import React, { useState, useEffect, useCallback } from 'react';
import Head from 'next/head';
import api from '@/lib/api';
import { formatCurrency, formatNumber, formatTimestamp } from '@/lib/utils';

interface WhaleTransfer {
  transactionHash: string;
  fromAddress: string;
  toAddress: string | null;
  value: string;
  valueUsd: string | null;
  tokenAddress: string | null;
  blockNumber: number;
  whaleType: string | null;
  createdAt: string;
}

interface MEVBundle {
  bundleHash: string;
  blockNumber: number;
  sender: string;
  mevType: string;
  gasUsed: number | null;
  profitUsd: string | null;
}

interface FlashLoan {
  transactionHash: string;
  blockNumber: number;
  borrower: string;
  provider: string | null;
  asset: string;
  amount: string;
  fee: string;
  profitUsd: string | null;
}

interface WhaleStats {
  whaleTransferCount: number;
  whaleVolume24h: string;
  mevProfit24h: string;
  mevBundleCount24h: number;
  flashProfit24h: string;
  flashLoanCount24h: number;
}

const shortAddress = (addr: string | null) =>
  addr ? `${addr.slice(0, 10)}…${addr.slice(-8)}` : '—';

const toNumber = (v: string | number | null | undefined): number => {
  if (v === null || v === undefined) return 0;
  const n = typeof v === 'number' ? v : Number(v);
  return Number.isFinite(n) ? n : 0;
};

export default function WhaleTrackingPage() {
  const [stats, setStats] = useState<WhaleStats | null>(null);
  const [transfers, setTransfers] = useState<WhaleTransfer[]>([]);
  const [bundles, setBundles] = useState<MEVBundle[]>([]);
  const [flashLoans, setFlashLoans] = useState<FlashLoan[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<'transfers' | 'mev' | 'flash'>('transfers');

  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      const [whaleStats, whaleList, mevList, flashList] = await Promise.all([
        api.getWhaleStats(),
        api.getWhales({ limit: 100 }),
        api.getMEVBundles({ limit: 100 }),
        api.getFlashLoans({ limit: 100 }),
      ]);
      setStats(whaleStats as unknown as WhaleStats);
      setTransfers((whaleList.items as unknown as WhaleTransfer[]) ?? []);
      setBundles((mevList.items as unknown as MEVBundle[]) ?? []);
      setFlashLoans((flashList.items as unknown as FlashLoan[]) ?? []);
      setError(null);
    } catch (err) {
      setStats(null);
      setTransfers([]);
      setBundles([]);
      setFlashLoans([]);
      setError(err instanceof Error ? err.message : 'Failed to load whale activity');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [fetchData]);

  const cards = [
    { label: 'Whale Transfers', value: stats ? formatNumber(stats.whaleTransferCount) : '—' },
    { label: 'Whale Volume (24h)', value: stats ? formatCurrency(toNumber(stats.whaleVolume24h)) : '—' },
    { label: 'MEV Bundles (24h)', value: stats ? formatNumber(stats.mevBundleCount24h) : '—' },
    { label: 'MEV Profit (24h)', value: stats ? formatCurrency(toNumber(stats.mevProfit24h)) : '—' },
    { label: 'Flash Loans (24h)', value: stats ? formatNumber(stats.flashLoanCount24h) : '—' },
  ];

  return (
    <>
      <Head><title>Whale Tracking | TigerScan</title></Head>
      <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
          <div className="flex items-center justify-between mb-6">
            <div>
              <h1 className="text-3xl font-bold text-gray-900 dark:text-white">Whale Tracking</h1>
              <p className="text-gray-500 dark:text-gray-400 mt-1">
                Large transfers, MEV bundles and flash loans recorded by the indexer
              </p>
            </div>
            <button onClick={fetchData} className="px-4 py-2 bg-primary-500 text-white rounded-lg hover:bg-primary-600">
              Refresh
            </button>
          </div>

          {error && (
            <div className="mb-6 rounded-lg border border-red-300 bg-red-50 dark:border-red-800 dark:bg-red-950 px-4 py-3 text-sm text-red-700 dark:text-red-300">
              {error}
            </div>
          )}

          <div className="grid grid-cols-2 md:grid-cols-5 gap-4 mb-8">
            {cards.map((card) => (
              <div key={card.label} className="bg-white dark:bg-dark-800 rounded-xl border p-4">
                <div className="text-xs uppercase text-gray-500">{card.label}</div>
                <div className="text-xl font-bold text-gray-900 dark:text-white mt-1">
                  {loading ? '…' : card.value}
                </div>
              </div>
            ))}
          </div>

          <div className="flex gap-2 mb-4">
            {(['transfers', 'mev', 'flash'] as const).map((t) => (
              <button
                key={t}
                onClick={() => setTab(t)}
                className={`px-4 py-2 rounded-lg text-sm font-medium ${
                  tab === t
                    ? 'bg-primary-500 text-white'
                    : 'bg-white dark:bg-dark-800 text-gray-600 dark:text-gray-300 border'
                }`}
              >
                {t === 'transfers' ? 'Large Transfers' : t === 'mev' ? 'MEV Bundles' : 'Flash Loans'}
              </button>
            ))}
          </div>

          <div className="bg-white dark:bg-dark-800 rounded-xl border overflow-hidden">
            <div className="overflow-x-auto">
              {tab === 'transfers' && (
                <table className="min-w-full text-sm">
                  <thead className="bg-gray-50 dark:bg-dark-700 text-gray-500">
                    <tr>
                      <th className="px-4 py-3 text-left">Transaction</th>
                      <th className="px-4 py-3 text-left">From</th>
                      <th className="px-4 py-3 text-left">To</th>
                      <th className="px-4 py-3 text-right">Value USD</th>
                      <th className="px-4 py-3 text-left">Type</th>
                      <th className="px-4 py-3 text-left">Block</th>
                      <th className="px-4 py-3 text-left">Time</th>
                    </tr>
                  </thead>
                  <tbody>
                    {transfers.length === 0 ? (
                      <tr><td colSpan={7} className="px-4 py-8 text-center text-gray-500">
                        No whale transfers have been indexed yet.
                      </td></tr>
                    ) : transfers.map((t) => (
                      <tr key={t.transactionHash} className="border-t dark:border-dark-700">
                        <td className="px-4 py-3 font-mono text-xs">{shortAddress(t.transactionHash)}</td>
                        <td className="px-4 py-3 font-mono text-xs">{shortAddress(t.fromAddress)}</td>
                        <td className="px-4 py-3 font-mono text-xs">{shortAddress(t.toAddress)}</td>
                        <td className="px-4 py-3 text-right">{t.valueUsd ? formatCurrency(toNumber(t.valueUsd)) : '—'}</td>
                        <td className="px-4 py-3">{t.whaleType ?? '—'}</td>
                        <td className="px-4 py-3">{formatNumber(t.blockNumber)}</td>
                        <td className="px-4 py-3 text-gray-500">{formatTimestamp(new Date(t.createdAt).getTime() / 1000)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}

              {tab === 'mev' && (
                <table className="min-w-full text-sm">
                  <thead className="bg-gray-50 dark:bg-dark-700 text-gray-500">
                    <tr>
                      <th className="px-4 py-3 text-left">Bundle</th>
                      <th className="px-4 py-3 text-left">Type</th>
                      <th className="px-4 py-3 text-left">Sender</th>
                      <th className="px-4 py-3 text-right">Profit USD</th>
                      <th className="px-4 py-3 text-left">Block</th>
                    </tr>
                  </thead>
                  <tbody>
                    {bundles.length === 0 ? (
                      <tr><td colSpan={5} className="px-4 py-8 text-center text-gray-500">
                        No MEV bundles have been indexed yet.
                      </td></tr>
                    ) : bundles.map((b) => (
                      <tr key={b.bundleHash} className="border-t dark:border-dark-700">
                        <td className="px-4 py-3 font-mono text-xs">{shortAddress(b.bundleHash)}</td>
                        <td className="px-4 py-3">{b.mevType}</td>
                        <td className="px-4 py-3 font-mono text-xs">{shortAddress(b.sender)}</td>
                        <td className="px-4 py-3 text-right">{b.profitUsd ? formatCurrency(toNumber(b.profitUsd)) : '—'}</td>
                        <td className="px-4 py-3">{formatNumber(b.blockNumber)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}

              {tab === 'flash' && (
                <table className="min-w-full text-sm">
                  <thead className="bg-gray-50 dark:bg-dark-700 text-gray-500">
                    <tr>
                      <th className="px-4 py-3 text-left">Transaction</th>
                      <th className="px-4 py-3 text-left">Borrower</th>
                      <th className="px-4 py-3 text-left">Provider</th>
                      <th className="px-4 py-3 text-right">Amount</th>
                      <th className="px-4 py-3 text-right">Fee</th>
                      <th className="px-4 py-3 text-right">Profit USD</th>
                      <th className="px-4 py-3 text-left">Block</th>
                    </tr>
                  </thead>
                  <tbody>
                    {flashLoans.length === 0 ? (
                      <tr><td colSpan={7} className="px-4 py-8 text-center text-gray-500">
                        No flash loans have been indexed yet.
                      </td></tr>
                    ) : flashLoans.map((f) => (
                      <tr key={`${f.transactionHash}-${f.asset}`} className="border-t dark:border-dark-700">
                        <td className="px-4 py-3 font-mono text-xs">{shortAddress(f.transactionHash)}</td>
                        <td className="px-4 py-3 font-mono text-xs">{shortAddress(f.borrower)}</td>
                        <td className="px-4 py-3 font-mono text-xs">{shortAddress(f.provider)}</td>
                        <td className="px-4 py-3 text-right">{f.amount}</td>
                        <td className="px-4 py-3 text-right">{f.fee}</td>
                        <td className="px-4 py-3 text-right">{f.profitUsd ? formatCurrency(toNumber(f.profitUsd)) : '—'}</td>
                        <td className="px-4 py-3">{formatNumber(f.blockNumber)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
