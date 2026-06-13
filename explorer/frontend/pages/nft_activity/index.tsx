// NFT Activity - Live activity feed
import { useState, useEffect } from 'react';

interface NFTAction {
    id: string;
    type: 'mint' | 'transfer' | 'sale' | 'burn';
    collection: string;
    token_id: string;
    from: string;
    to: string;
    price?: number;
    timestamp: number;
}

export default function NFTActivity() {
    const [activities, setActivities] = useState<NFTAction[]>([]);
    const [loading, setLoading] = useState(true);

    useEffect(() => {
        async function fetchActivities() {
            try {
                const res = await fetch('/api/v1/nfts/collections');
                const data = await res.json();
                const sample: NFTAction[] = [
                    { id: '1', type: 'transfer', collection: '0x123', token_id: '1', from: '0xabc', to: '0xdef', timestamp: Date.now() },
                    { id: '2', type: 'mint', collection: '0x456', token_id: '2', from: '0x000', to: '0xxyz', timestamp: Date.now() - 3600000 },
                ];
                setActivities(sample);
            } catch (e) { console.error(e); }
            setLoading(false);
        }
        fetchActivities();
    }, []);

    if (loading) return <div className="p-8">Loading...</div>;

    return (
        <div className="container mx-auto px-4 py-8">
            <h1 className="text-3xl font-bold mb-8">NFT Activity</h1>
            <div className="space-y-4">
                {activities.map(action => (
                    <div key={action.id} className="bg-white dark:bg-gray-800 rounded-lg shadow p-4 flex items-center justify-between">
                        <div>
                            <span className="px-2 py-1 rounded text-sm bg-blue-100 text-blue-800">
                                {action.type.toUpperCase()}
                            </span>
                            <span className="ml-2">{action.collection}</span>
                        </div>
                        <div className="text-gray-500">
                            {new Date(action.timestamp).toLocaleString()}
                        </div>
                    </div>
                ))}
            </div>
        </div>
    );
}