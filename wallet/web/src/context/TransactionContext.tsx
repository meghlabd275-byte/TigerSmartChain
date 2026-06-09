// Transaction Context - TigerSmartChain Web Wallet

import React, { createContext, useContext, useState, useCallback, ReactNode } from 'react';
import { ethers } from 'ethers';
import { Transaction, TransactionStatus } from '../types';

interface TransactionContextType {
  transactions: Transaction[];
  pendingCount: number;
  addTransaction: (tx: Transaction) => void;
  updateTransaction: (hash: string, status: TransactionStatus, receipt?: any) => void;
  getTransaction: (hash: string) => Transaction | undefined;
  clearHistory: () => void;
}

const TransactionContext = createContext<TransactionContextType | undefined>(undefined);

interface TransactionProviderProps {
  children: ReactNode;
}

export const TransactionProvider: React.FC<TransactionProviderProps> = ({ children }) => {
  const [transactions, setTransactions] = useState<Transaction[]>([]);

  // Get pending transaction count
  const pendingCount = transactions.filter(
    tx => tx.status === 'pending' || tx.status === 'submitted'
  ).length;

  // Add transaction
  const addTransaction = useCallback((tx: Transaction) => {
    setTransactions(prev => [...prev, tx]);
    
    // Store in localStorage
    const stored = JSON.parse(localStorage.getItem('tigersmartchain_txs') || '[]');
    stored.push(tx);
    localStorage.setItem('tigsermartchain_txs', JSON.stringify(stored));
  }, []);

  // Update transaction
  const updateTransaction = useCallback(
    (hash: string, status: TransactionStatus, receipt?: any) => {
      setTransactions(prev =>
        prev.map(tx =>
          tx.hash === hash
            ? { ...tx, status, receipt, timestamp: Date.now() }
            : tx
        )
      );
    },
    []
  );

  // Get transaction by hash
  const getTransaction = useCallback(
    (hash: string): Transaction | undefined => {
      return transactions.find(tx => tx.hash === hash);
    },
    [transactions]
  );

  // Clear history
  const clearHistory = useCallback(() => {
    setTransactions([]);
    localStorage.removeItem('tigersmartchain_txs');
  }, []);

  return (
    <TransactionContext.Provider
      value={{
        transactions,
        pendingCount,
        addTransaction,
        updateTransaction,
        getTransaction,
        clearHistory,
      }}
    >
      {children}
    </TransactionContext.Provider>
  );
};

export const useTransactions = (): TransactionContextType => {
  const context = useContext(TransactionContext);
  if (!context) {
    throw new Error('useTransactions must be used within TransactionProvider');
  }
  return context;
};

export default TransactionContext;