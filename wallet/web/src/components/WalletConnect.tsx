// WalletConnect Component - TigerSmartChain Web Wallet

import React, { useState } from 'react';
import './WalletConnect.css';

interface WalletConnectProps {
  onConnect: () => Promise<void>;
}

const WalletConnect: React.FC<WalletConnectProps> = ({ onConnect }) => {
  const [isConnecting, setIsConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);

  const handleConnect = async () => {
    setIsConnecting(true);
    setError(null);
    try {
      await onConnect();
    } catch (err: any) {
      setError(err.message || 'Failed to connect');
    } finally {
      setIsConnecting(false);
    }
  };

  return (
    <div className="wallet-connect">
      <div className="connect-card">
        <h2 className="connect-title">Connect Your Wallet</h2>
        <p className="connect-description">
          Connect to TigerSmartChain and access your assets securely.
        </p>

        {error && (
          <div className="connect-error">
            {error}
          </div>
        )}

        <div className="connect-options">
          <button
            className="connect-option metamask"
            onClick={handleConnect}
            disabled={isConnecting}
          >
            <img src="/metamask.svg" alt="MetaMask" />
            <span>MetaMask</span>
          </button>

          <button
            className="connect-option walletconnect"
            onClick={handleConnect}
            disabled={isConnecting}
          >
            <img src="/walletconnect.svg" alt="WalletConnect" />
            <span>WalletConnect</span>
          </button>

          <button
            className="connect-option ledger"
            onClick={handleConnect}
            disabled={isConnecting}
          >
            <img src="/ledger.svg" alt="Ledger" />
            <span>Ledger</span>
          </button>

          <button
            className="connect-option trezor"
            onClick={handleConnect}
            disabled={isConnecting}
          >
            <img src="/trezor.svg" alt="Trezor" />
            <span>Trezor</span>
          </button>
        </div>

        <div className="connect-divider">
          <span>or</span>
        </div>

        <button
          className="create-wallet-button"
          onClick={() => setShowCreate(!showCreate)}
        >
          Create New Wallet
        </button>

        {showCreate && (
          <div className="create-wallet-form">
            <p className="warning-text">
              ⚠️ Save your private key securely. It cannot be recovered if lost.
            </p>
            <button
              className="generate-button"
              onClick={handleConnect}
            >
              Generate Wallet
            </button>
          </div>
        )}

        {isConnecting && (
          <div className="connecting">
            <div className="spinner"></div>
            <span>Connecting...</span>
          </div>
        )}
      </div>
    </div>
  );
};

export default WalletConnect;