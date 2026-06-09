// TigerScan Block Card Component
// Displays a single block in the explorer

import React from 'react';
import { Link } from 'react-router-dom';
import './BlockCard.css';

interface BlockCardProps {
  block: {
    number: number;
    hash: string;
    timestamp: number;
    transactions: number;
    gasUsed: string;
    gasLimit: string;
    miner: string;
    parentHash: string;
  };
}

const BlockCard: React.FC<BlockCardProps> = ({ block }) => {
  const formatTimestamp = (ts: number): string => {
    const date = new Date(ts * 1000);
    return date.toLocaleString();
  };

  const formatAddress = (addr: string): string => {
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
  };

  const gasPercent = () => {
    const used = parseInt(block.gasUsed) || 0;
    const limit = parseInt(block.gasLimit) || 1;
    return ((used / limit) * 100).toFixed(1);
  };

  return (
    <div className="block-card">
      <div className="block-header">
        <Link to={`/block/${block.number}`} className="block-number">
          #{block.number.toLocaleString()}
        </Link>
        <span className="block-time">{formatTimestamp(block.timestamp)}</span>
      </div>

      <div className="block-hash">
        <span className="label">Hash:</span>
        <Link to={`/block/${block.hash}`} className="value">
          {formatAddress(block.hash)}
        </Link>
      </div>

      <div className="block-details">
        <div className="detail-item">
          <span className="label">Transactions</span>
          <span className="value">{block.transactions}</span>
        </div>

        <div className="detail-item">
          <span className="label">Gas Used</span>
          <span className="value">
            {parseInt(block.gasUsed).toLocaleString()} ({gasPercent()}%)
          </span>
        </div>

        <div className="detail-item">
          <span className="label">Miner</span>
          <Link to={`/address/${block.miner}`} className="value">
            {formatAddress(block.miner)}
          </Link>
        </div>
      </div>

      <div className="block-footer">
        <Link to={`/block/${block.number - 1}`} className="nav-link">
          ← Previous
        </Link>
        <Link to={`/block/${block.number + 1}`} className="nav-link">
          Next →
        </Link>
      </div>
    </div>
  );
};

export default BlockCard;