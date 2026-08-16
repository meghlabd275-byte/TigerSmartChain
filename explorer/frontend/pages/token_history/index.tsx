// Token History - Price history charts
import { useState, useEffect, useCallback } from 'react';

const API_BASE = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:12000';

interface PricePoint {
    timestamp: number;
    price: number;
    volume: number;
}

export default function TokenHistory() {
    const [prices, setPrices] = useState<PricePoint[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    const fetchPrices = useCallback(async () => {
        setLoading(true);
        setError(null);
        try {
            const res = await fetch(`${API_BASE}/api/v1/analytics/gas`);
            if (!res.ok) throw new Error(`Failed to load price history (${res.status})`);
            const data = await res.json();
            const points: PricePoint[] = Array.isArray(data) ? data : (data.prices ?? data.data ?? []);
            setPrices(points);
        } catch (e) {
            setError(e instanceof Error ? e.message : 'Failed to load price history');
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        fetchPrices();
    }, [fetchPrices]);

    const maxHeight = prices.length ? Math.max(...prices.map(p => p.price)) : 1;

    return (
        <div className="container mx-auto px-4 py-8">
            <h1 className="text-3xl font-bold mb-8">Token Price History</h1>
            <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
                <h2 className="text-xl font-semibold mb-4">TGR/USD Price</h2>

                {loading && (
                    <div className="flex items-center justify-center h-64">
                        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500"></div>
                    </div>
                )}

                {!loading && error && (
                    <div className="text-center py-16">
                        <p className="text-red-500 mb-4">{error}</p>
                        <button
                            onClick={fetchPrices}
                            className="px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600"
                        >
                            Retry
                        </button>
                    </div>
                )}

                {!loading && !error && prices.length === 0 && (
                    <div className="text-center py-16 text-gray-500">No data available</div>
                )}

                {!loading && !error && prices.length > 0 && (
                    <div className="h-64 flex items-end space-x-2">
                        {prices.map((p, i) => (
                            <div key={i} className="flex-1 bg-blue-500" style={{height: `${(p.price / maxHeight) * 100}%`}}></div>
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}