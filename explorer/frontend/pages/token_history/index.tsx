// Token History - Price history charts
import { useState, useEffect } from 'react';

interface PricePoint {
    timestamp: number;
    price: number;
    volume: number;
}

export default function TokenHistory() {
    const [prices, setPrices] = useState<PricePoint[]>([]);
    const [loading, setLoading] = useState(true);

    useEffect(() => {
        async function fetchPrices() {
            try {
                const res = await fetch('/api/v1/analytics/gas');
                const data = await res.json();
                // Generate sample data
                const samplePrices: PricePoint[] = Array.from({length: 30}, (_, i) => ({
                    timestamp: Date.now() - i * 86400000,
                    price: 1 + Math.random() * 0.5,
                    volume: 1000000 + Math.random() * 500000,
                }));
                setPrices(samplePrices.reverse());
            } catch (e) { console.error(e); }
            setLoading(false);
        }
        fetchPrices();
    }, []);

    if (loading) return <div className="p-8">Loading...</div>;

    return (
        <div className="container mx-auto px-4 py-8">
            <h1 className="text-3xl font-bold mb-8">Token Price History</h1>
            <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
                <h2 className="text-xl font-semibold mb-4">TGR/USD Price</h2>
                <div className="h-64 flex items-end space-x-2">
                    {prices.map((p, i) => (
                        <div key={i} className="flex-1 bg-blue-500" style={{height: `${(p.price / 2) * 100}%`}}></div>
                    ))}
                </div>
            </div>
        </div>
    );
}