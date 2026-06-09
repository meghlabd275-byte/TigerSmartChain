// Wallet Context - TigerSmartChain Web Wallet

import React, { createContext, useContext, useState, useCallback, ReactNode } from 'react';
import { ethers } from 'ethers';
import { WalletState, Transaction, Token } from '../types';

interface WalletContextType {
  account: string | null;
  balance: string;
  isLocked: boolean;
  isConnecting: boolean;
  error: string | null;
  tokens: Token[];
  connect: () => Promise<void>;
  disconnect: () => void;
  unlock: (password: string) => Promise<boolean>;
  lock: () => void;
  sendTransaction: (to: string, value: string, data?: string) => Promise<string>;
  signMessage: (message: string) => Promise<string>;
  addToken: (token: Token) => void;
  removeToken: (address: string) => void;
}

const WalletContext = createContext<WalletContextType | undefined>(undefined);

interface WalletProviderProps {
  children: ReactNode;
}

export const WalletProvider: React.FC<WalletProviderProps> = ({ children }) => {
  const [account, setAccount] = useState<string | null>(null);
  const [balance, setBalance] = useState<string>('0');
  const [isLocked, setIsLocked] = useState<boolean>(true);
  const [isConnecting, setIsConnecting] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [tokens, setTokens] = useState<Token[]>([]);
  const [privateKey, setPrivateKey] = useState<string | null>(null);
  const [provider, setProvider] = useState<ethers.JsonRpcProvider | null>(null);

  // Initialize provider
  const initProvider = useCallback(async () => {
    try {
      // Connect to TigerSmartChain network
      const rpcUrl = localStorage.getItem('tigersmartchain_rpc') || 'http://localhost:8545';
      const newProvider = new ethers.JsonRpcProvider(rpcUrl);
      setProvider(newProvider);
      return newProvider;
    } catch (err) {
      console.error('Failed to initialize provider:', err);
      setError('Failed to connect to network');
      return null;
    }
  }, []);

  // Connect wallet
  const connect = useCallback(async () => {
    setIsConnecting(true);
    setError(null);

    try {
      // Check for injected provider (MetaMask, etc.)
      if (typeof window.ethereum !== 'undefined') {
        const browserProvider = new ethers.BrowserProvider(window.ethereum);
        const accounts = await browserProvider.send('eth_requestAccounts', []);
        if (accounts.length > 0) {
          setAccount(accounts[0]);
          
          // Get balance
          const balance = await browserProvider.getBalance(accounts[0]);
          setBalance(balance.toString());
          
          setIsLocked(false);
          setIsConnecting(false);
          localStorage.setItem('tigersmartchain_account', accounts[0]);
          return;
        }
      }

      // Use stored private key if available
      const storedKey = localStorage.getItem('tigersmartchain_key');
      if (storedKey) {
        const wallet = new ethers.Wallet(storedKey);
        const prov = await initProvider();
        if (prov) {
          const connectedWallet = wallet.connect(prov);
          setAccount(connectedWallet.address);
          
          const balance = await prov.getBalance(connectedWallet.address);
          setBalance(balance.toString());
          
          setPrivateKey(storedKey);
          setIsLocked(false);
          localStorage.setItem('tigersmartchain_account', connectedWallet.address);
        }
      } else {
        // Generate new wallet
        const wallet = ethers.Wallet.createRandom();
        setPrivateKey(wallet.privateKey);
        setAccount(wallet.address);
        setIsLocked(true); // Require password to unlock
        localStorage.setItem('tigersmartchain_account', wallet.address);
      }
    } catch (err: any) {
      console.error('Connection error:', err);
      setError(err.message || 'Failed to connect wallet');
    } finally {
      setIsConnecting(false);
    }
  }, [initProvider]);

  // Disconnect wallet
  const disconnect = useCallback(() => {
    setAccount(null);
    setBalance('0');
    setIsLocked(true);
    setPrivateKey(null);
    setTokens([]);
    localStorage.removeItem('tigersmartchain_account');
  }, []);

  // Unlock wallet with password
  const unlock = useCallback(async (password: string): Promise<boolean> => {
    try {
      // Derive key from password
      const key = await ethers.Wallet.fromMnemonic(
        ethers.Mnemonic.fromEntropy(
          await ethers.id(password)
        )
      ).privateKey;

      setPrivateKey(key);
      setIsLocked(false);
      return true;
    } catch {
      return false;
    }
  }, []);

  // Lock wallet
  const lock = useCallback(() => {
    setIsLocked(true);
  }, []);

  // Send transaction
  const sendTransaction = useCallback(async (
    to: string,
    value: string,
    data?: string
  ): Promise<string> => {
    if (!provider || !account || !privateKey) {
      throw new Error('Wallet not connected');
    }

    try {
      const wallet = new ethers.Wallet(privateKey, provider);
      
      const tx: Transaction = {
        to,
        value: ethers.parseEther(value),
        data: data || '0x',
        gasLimit: 21000,
        gasPrice: await provider.getGasPrice(),
      };

      const response = await wallet.sendTransaction(tx);
      await response.wait();
      
      // Update balance
      const newBalance = await provider.getBalance(account);
      setBalance(newBalance.toString());
      
      return response.hash;
    } catch (err: any) {
      throw new Error(err.message || 'Transaction failed');
    }
  }, [provider, account, privateKey]);

  // Sign message
  const signMessage = useCallback(async (message: string): Promise<string> => {
    if (!privateKey) {
      throw new Error('Wallet not unlocked');
    }

    try {
      const wallet = new ethers.Wallet(privateKey);
      return await wallet.signMessage(message);
    } catch (err: any) {
      throw new Error(err.message || 'Sign failed');
    }
  }, [privateKey]);

  // Add token
  const addToken = useCallback((token: Token) => {
    setTokens(prev => {
      if (prev.some(t => t.address === token.address)) {
        return prev;
      }
      return [...prev, token];
    });
  }, []);

  // Remove token
  const removeToken = useCallback((address: string) => {
    setTokens(prev => prev.filter(t => t.address !== address));
  }, []);

  return (
    <WalletContext.Provider
      value={{
        account,
        balance,
        isLocked,
        isConnecting,
        error,
        tokens,
        connect,
        disconnect,
        unlock,
        lock,
        sendTransaction,
        signMessage,
        addToken,
        removeToken,
      }}
    >
      {children}
    </WalletContext.Provider>
  );
};

export const useWallet = (): WalletContextType => {
  const context = useContext(WalletContext);
  if (!context) {
    throw new Error('useWallet must be used within WalletProvider');
  }
  return context;
};

export default WalletContext;