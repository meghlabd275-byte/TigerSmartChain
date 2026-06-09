// useNetwork Hook - TigerSmartChain Web Wallet

import { useState, useCallback } from 'react';
import { ethers } from 'ethers';
import { Network } from '../types';

interface UseNetworkReturn {
  network: Network;
  networks: Network[];
  switchNetwork: (networkId: number) => Promise<void>;
  getExplorerUrl: (txHash: string) => string;
  getAddressUrl: (address: string) => string;
  getBlockUrl: (blockNumber: number) => string;
  provider: ethers.JsonRpcProvider | null;
}

const DEFAULT_NETWORKS: Network[] = [
  {
    id: 1,
    name: 'TigerSmartChain Mainnet',
    rpcUrl: 'https://mainnet.tigersmartchain.com',
    explorerUrl: 'https://scan.tigersmartchain.com',
    chainId: 6666,
    currency: { name: 'TigerCoin', symbol: 'TIGER', decimals: 18 },
  },
  {
    id: 2,
    name: 'TigerSmartChain Testnet',
    rpcUrl: 'https://testnet.tigersmartchain.com',
    explorerUrl: 'https://testnet.scan.tigersmartchain.com',
    chainId: 6667,
    currency: { name: 'TigerCoin', symbol: 'TIGER', decimals: 18 },
  },
  {
    id: 3,
    name: 'Localhost',
    rpcUrl: 'http://localhost:8545',
    explorerUrl: 'http://localhost:4000',
    chainId: 6666,
    currency: { name: 'TigerCoin', symbol: 'TIGER', decimals: 18 },
  },
];

export const useNetwork = (): UseNetworkReturn => {
  const [network, setNetwork] = useState<Network>(DEFAULT_NETWORKS[0]);
  const [networks, setNetworks] = useState<Network[]>(DEFAULT_NETWORKS);
  const [provider, setProvider] = useState<ethers.JsonRpcProvider | null>(null);

  // Switch network
  const switchNetwork = useCallback(async (networkId: number) => {
    const newNetwork = networks.find(n => n.id === networkId);
    if (!newNetwork) throw new Error('Network not found');

    try {
      const newProvider = new ethers.JsonRpcProvider(newNetwork.rpcUrl);
      await newProvider.getBlockNumber();
      setProvider(newProvider);
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

  return {
    network,
    networks,
    switchNetwork,
    getExplorerUrl,
    getAddressUrl,
    getBlockUrl,
    provider,
  };
};

export default useNetwork;