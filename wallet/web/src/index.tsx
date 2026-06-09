// TigerSmartChain Web Wallet
// A secure, industrial-grade web wallet for TigerSmartChain

import React from 'react';
import ReactDOM from 'react-dom/client';
import { WalletProvider } from './context/WalletContext';
import { NetworkProvider } from './context/NetworkContext';
import { TransactionProvider } from './context/TransactionContext';
import App from './App';
import './styles/index.css';

const root = ReactDOM.createRoot(
  document.getElementById('root') as HTMLElement
);

root.render(
  <React.StrictMode>
    <NetworkProvider>
      <WalletProvider>
        <TransactionProvider>
          <App />
        </TransactionProvider>
      </WalletProvider>
    </NetworkProvider>
  </React.StrictMode>
);