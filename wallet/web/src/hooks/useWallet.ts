// useWallet Hook - TigerSmartChain Web Wallet

import { useState, useEffect, useCallback } from 'react';
import { ethers } from 'ethers';
import { WalletState, Token, Transaction } from '../types';

interface UseWalletReturn extends WalletState {
  connect: () => Promise<void>;
  disconnect: () => void;
  unlock: (password: string) => Promise<boolean>;
  lock: () => void;
  sendTransaction: (to: string, value: string, data?: string) => Promise<string>;
  signMessage: (message: string) => Promise<string>;
  addToken: (token: Token) => void;
  removeToken: (address: string) => void;
  refreshBalance: () => Promise<void>;
}

export const useWallet = (): UseWalletReturn => {
  const [account, setAccount] = useState<string | null>(null);
  const [balance, setBalance] = useState<string>('0');
  const [isLocked, setIsLocked] = useState<boolean>(true);
  const [isConnecting, setIsConnecting] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [tokens, setTokens] = useState<Token[]>([]);
  const [provider, setProvider] = useState<ethers.JsonRpcProvider | null>(null);
  const [wallet, setWallet] = useState<ethers.Wallet | null>(null);

  // Initialize on mount
  useEffect(() => {
    const init = async () => {
      try {
        const rpcUrl = localStorage.getItem('tigersmartchain_rpc') || 'http://localhost:8545';
        const newProvider = new ethers.JsonRpcProvider(rpcUrl);
        setProvider(newProvider);

        // Check for stored account
        const storedAccount = localStorage.getItem('tigersmartchain_account');
        if (storedAccount) {
          setAccount(storedAccount);
          
          // Get balance
          const balance = await newProvider.getBalance(storedAccount);
          setBalance(balance.toString());
        }
      } catch (err) {
        console.error('Init error:', err);
      }
    };

    init();
  }, []);

  // Connect wallet
  const connect = useCallback(async () => {
    setIsConnecting(true);
    setError(null);

    try {
      // Check for injected provider
      if (typeof window.ethereum !== 'undefined') {
        const browserProvider = new ethers.BrowserProvider(window.ethereum);
        const accounts = await browserProvider.send('eth_requestAccounts', []);
        if (accounts.length > 0) {
          setAccount(accounts[0]);
          const balance = await browserProvider.getBalance(accounts[0]);
          setBalance(balance.toString());
          setIsLocked(false);
          localStorage.setItem('tigersmartchain_account', accounts[0]);
          setIsConnecting(false);
          return;
        }
      }

      // Use stored private key
      const storedKey = localStorage.getItem('tigersmartchain_key');
      if (storedKey && provider) {
        const newWallet = new ethers.Wallet(storedKey, provider);
        setWallet(newWallet);
        setAccount(newWallet.address);
        const balance = await provider.getBalance(newWallet.address);
        setBalance(balance.toString());
        setIsLocked(false);
        localStorage.setItem('tigersmartchain_account', newWallet.address);
      } else {
        // Create new wallet
        const newWallet = ethers.Wallet.createRandom();
        setWallet(newWallet);
        setAccount(newWallet.address);
        setIsLocked(true);
        localStorage.setItem('tigersmartchain_account', newWallet.address);
        localStorage.setItem('tigersmartchain_key', newWallet.privateKey);
      }
    } catch (err: any) {
      setError(err.message || 'Failed to connect');
    } finally {
      setIsConnecting(false);
    }
  }, [provider]);

  // Disconnect
  const disconnect = useCallback(() => {
    setAccount(null);
    setBalance('0');
    setIsLocked(true);
    setWallet(null);
    setTokens([]);
    localStorage.removeItem('tigersmartchain_account');
  }, []);

  // Unlock with password
  const unlock = useCallback(async (password: string): Promise<boolean> => {
    if (!wallet) return false;
    setIsLocked(false);
    return true;
  }, [wallet]);

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
    if (!wallet) throw new Error('Wallet not connected');
    if (isLocked) throw new Error('Wallet is locked');

    const tx = {
      to,
      value: ethers.parseEther(value),
      data: data || '0x',
    };

    const response = await wallet.sendTransaction(tx);
    await response.wait();

    // Update balance
    if (provider && account) {
      const newBalance = await provider.getBalance(account);
      setBalance(newBalance.toString());
    }

    return response.hash;
  }, [wallet, provider, account, isLocked]);

  // Sign message
  const signMessage = useCallback(async (message: string): Promise<string> => {
    if (!wallet) throw new Error('Wallet not connected');
    if (isLocked) throw new Error('Wallet is locked');

    return await wallet.signMessage(message);
  }, [wallet, isLocked]);

  // Add token
  const addToken = useCallback((token: Token) => {
    setTokens(prev => {
      if (prev.some(t => t.address === token.address)) return prev;
      return [...prev, token];
    });
  }, []);

  // Remove token
  const removeToken = useCallback((address: string) => {
    setTokens(prev => prev.filter(t => t.address !== address));
  }, []);

  // Refresh balance
  const refreshBalance = useCallback(async () => {
    if (!provider || !account) return;
    const newBalance = await provider.getBalance(account);
    setBalance(newBalance.toString());
  }, [provider, account]);

  return {
    account,
    balance,
    isLocked,
    isConnecting,
    error,
    connect,
    disconnect,
    unlock,
    lock,
    sendTransaction,
    signMessage,
    addToken,
    removeToken,
    refreshBalance,
  };
};

export default useWallet;