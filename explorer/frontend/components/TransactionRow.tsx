// TigerScan Transaction Row Component
// Displays a single transaction in a table row

import React from 'react';
import { Link } from 'react-router-dom';
import './TransactionRow.css';

interface TransactionRowProps {
  tx: {
    hash: string;
    from: string;
    to: string;
    value: string;
    gasPrice: string;
    gasUsed: string;
    timestamp: number;
    status: 'success' | 'fail' | 'pending';
    blockNumber?: number;
  };
}

const TransactionRow: React.FC<TransactionRowProps> = ({ tx }) => {
  const formatAddress = (addr: string): string => {
    if (!addr) return '-';
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
  };

  const formatValue = (value: string): string => {
    const val = parseFloat(value) / 1e18;
    return val.toFixed(6);
  };

  const formatTimestamp = (ts: number): string => {
    const date = new Date(ts * 1000);
    return date.toLocaleString();
  };

  return (
    <tr className={`transaction-row ${tx.status}`}>
      <td className="tx-hash">
        <Link to={`/tx/${tx.hash}`}>
          {formatAddress(tx.hash)}
        </Link>
      </td>
      
      <td className="tx-from">
        <Link to={`/address/${tx.from}`}>
          {formatAddress(tx.from)}
        </Link>
      </td>
      
      <td className="tx-arrow">→</td>
      
      <td className="tx-to">
        <Link to={`/address/${tx.to}`}>
          {formatAddress(tx.to)}
        </Link>
      </td>
      
      <td className="tx-value">
        {formatValue(tx.value)} TGR
      </td>
      
      <td className="tx-gas">
        {tx.gasUsed || '-'}
      </td>
      
      <td className="tx-status">
        <span className={`status-badge ${tx.status}`}>
          {tx.status}
        </span>
      </td>
      
      <td className="tx-time">
        {tx.timestamp ? formatTimestamp(tx.timestamp) : '-'}
      </td>
    </tr>
  );
};

export default TransactionRow;