// Network Context - TigerSmartChain Web Wallet

import React, { createContext, useContext, useState, useCallback, ReactNode } from 'react';
import { ethers } from 'ethers';

interface Network {
  id: number;
  name: string;
  rpcUrl: string;
  explorerUrl: string;
  chainId: number;
}

interface NetworkContextType {
  network: Network;
  networks: Network[];
  addNetwork: (network: Network) => void;
  switchNetwork: (networkId: number) => Promise<void>;
  getExplorerUrl: (txHash: string) => string;
  getAddressUrl: (address: string) => string;
  getBlockUrl: (blockNumber: number) => string;
}

const DEFAULT_NETWORKS: Network[] = [
  {
    id: 1,
    name: 'TigerSmartChain Mainnet',
    rpcUrl: 'https://mainnet.tigersmartchain.com',
    explorerUrl: 'https://scan.tigersmartchain.com',
    chainId: 6666,
  },
  {
    id: 2,
    name: 'TigerSmartChain Testnet',
    rpcUrl: 'https://testnet.tigersmartchain.com',
    explorerUrl: 'https://testnet.scan.tigersmartchain.com',
    chainId: 6667,
  },
  {
    id: 3,
    name: 'Localhost',
    rpcUrl: 'http://localhost:8545',
    explorerUrl: 'http://localhost:4000',
    chainId: 6666,
  },
];

const NetworkContext = createContext<NetworkContextType | undefined>(undefined);

interface NetworkProviderProps {
  children: ReactNode;
}

export const NetworkProvider: React.FC<NetworkProviderProps> = ({ children }) => {
  const [network, setNetwork] = useState<Network>(DEFAULT_NETWORKS[0]);
  const [networks, setNetworks] = useState<Network[]>(DEFAULT_NETWORKS);

  // Add custom network
  const addNetwork = useCallback((network: Network) => {
    setNetworks(prev => {
      if (prev.some(n => n.id === network.id)) {
        return prev;
      }
      return [...prev, network];
    });
  }, []);

  // Switch network
  const switchNetwork = useCallback(async (networkId: number) => {
    const newNetwork = networks.find(n => n.id === networkId);
    if (!newNetwork) {
      throw new Error('Network not found');
    }

    // Verify network connection
    try {
      const provider = new ethers.JsonRpcProvider(newNetwork.rpcUrl);
      await provider.getBlockNumber();
      setNetwork(newNetwork);
      localStorage.setItem('tigersmartchain_network', newNetwork.id.toString());
    } catch {
      throw new Error('Failed to connect to network');
    }
  }, [networks]);

  // Get explorer URL for transaction
  const getExplorerUrl = useCallback((txHash: string): string => {
    return `${network.explorerUrl}/tx/${txHash}`;
  }, [network]);

  // Get explorer URL for address
  const getAddressUrl = useCallback((address: string): string => {
    return `${network.explorerUrl}/address/${address}`;
  }, [network]);

  // Get explorer URL for block
  const getBlockUrl = useCallback((blockNumber: number): string => {
    return `${network.explorerUrl}/block/${blockNumber}`;
  }, [network]);

  return (
    <NetworkContext.Provider
      value={{
        network,
        networks,
        addNetwork,
        switchNetwork,
        getExplorerUrl,
        getAddressUrl,
        getBlockUrl,
      }}
    >
      {children}
    </NetworkContext.Provider>
  );
};

export const useNetwork = (): NetworkContextType => {
  const context = useContext(NetworkContext);
  if (!context) {
    throw new Error('useNetwork must be used within NetworkProvider');
  }
  return context;
};

export default NetworkContext;