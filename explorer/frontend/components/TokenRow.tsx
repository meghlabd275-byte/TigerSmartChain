// TigerScan Token Row Component
// Displays a single token in a table row

import React from 'react';
import { Link } from 'react-router-dom';
import './TokenRow.css';

interface TokenRowProps {
  token: {
    address: string;
    name: string;
    symbol: string;
    decimals: number;
    totalSupply: string;
    holders: number;
    transfers: number;
    price?: string;
    volume24h?: string;
  };
}

const TokenRow: React.FC<TokenRowProps> = ({ token }) => {
  const formatSupply = (supply: string, decimals: number): string => {
    const val = parseFloat(supply) / Math.pow(10, decimals);
    return val.toLocaleString(undefined, { maximumFractionDigits: 2 });
  };

  const formatPrice = (price: string | undefined): string => {
    if (!price) return '-';
    const val = parseFloat(price);
    if (val < 0.01) return `$${val.toFixed(6)}`;
    return `$${val.toFixed(2)}`;
  };

  return (
    <tr className="token-row">
      <td className="token-rank">#</td>
      
      <td className="token-info">
        <Link to={`/token/${token.address}`} className="token-name">
          {token.name}
        </Link>
        <span className="token-symbol">{token.symbol}</span>
      </td>
      
      <td className="token-price">
        {formatPrice(token.price)}
      </td>
      
      <td className="token-change">
        <span className="change-positive">+0.00%</span>
      </td>
      
      <td className="token-volume">
        ${token.volume24h || '-'}
      </td>
      
      <td className="token-holders">
        {token.holders.toLocaleString()}
      </td>
      
      <td className="token-transfers">
        {token.transfers.toLocaleString()}
      </td>
      
      <td className="token-supply">
        {formatSupply(token.totalSupply, token.decimals)}
      </td>
    </tr>
  );
};

export default TokenRow;