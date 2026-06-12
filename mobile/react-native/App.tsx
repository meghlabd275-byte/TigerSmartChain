/**
 * TigerScan Mobile - Main Application
 * 
 * React Native mobile application for TigerScan blockchain explorer
 */

import React, { useState, useEffect } from 'react';
import {
  StyleSheet,
  View,
  Text,
  TextInput,
  TouchableOpacity,
  FlatList,
  SafeAreaView,
  StatusBar,
  ActivityIndicator,
  ScrollView,
  Image,
  Dimensions,
} from 'react-native';
import { NavigationContainer } from '@react-navigation/native';
import { createBottomTabNavigator } from '@react-navigation/bottom-tabs';
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import axios from 'axios';
import { ethers } from 'ethers';

// Screen dimensions
const { width: SCREEN_WIDTH } = Dimensions.get('window');

// Types
interface Block {
  number: number;
  hash: string;
  gasUsed: number;
  gasLimit: number;
  timestamp: number;
  miner: string;
}

interface Transaction {
  hash: string;
  from: string;
  to: string;
  value: string;
  gasPrice: string;
  status: number;
}

interface Token {
  address: string;
  name: string;
  symbol: string;
  price: string;
  change24h: number;
}

interface NFT {
  address: string;
  name: string;
  symbol: string;
  floorPrice: string;
}

// API Configuration
const API_BASE = 'https://api.tigerscan.io/api/v2';

// Custom Hooks
const useTigerScan = () => {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const getBlock = async (number: number) => {
    setLoading(true);
    try {
      const response = await axios.get(`${API_BASE}/block/${number}`);
      return response.data.result;
    } catch (e: any) {
      setError(e.message);
      return null;
    } finally {
      setLoading(false);
    }
  };

  const getLatestBlock = async () => {
    setLoading(true);
    try {
      const response = await axios.get(`${API_BASE}/block/latest`);
      return response.data.result;
    } catch (e: any) {
      setError(e.message);
      return null;
    } finally {
      setLoading(false);
    }
  };

  const getTransaction = async (hash: string) => {
    setLoading(true);
    try {
      const response = await axios.get(`${API_BASE}/tx/${hash}`);
      return response.data.result;
    } catch (e: any) {
      setError(e.message);
      return null;
    } finally {
      setLoading(false);
    }
  };

  const getTokens = async (page: number = 1) => {
    setLoading(true);
    try {
      const response = await axios.get(`${API_BASE}/tokens`, {
        params: { page, offset: 20 },
      });
      return response.data.result;
    } catch (e: any) {
      setError(e.message);
      return [];
    } finally {
      setLoading(false);
    }
  };

  const getNFTs = async (page: number = 1) => {
    setLoading(true);
    try {
      const response = await axios.get(`${API_BASE}/nfts`, {
        params: { page, filter: 'all' },
      });
      return response.data.result;
    } catch (e: any) {
      setError(e.message);
      return [];
    } finally {
      setLoading(false);
    }
  };

  const getNetworkStats = async () => {
    setLoading(true);
    try {
      const response = await axios.get(`${API_BASE}/stats`);
      return response.data.result;
    } catch (e: any) {
      setError(e.message);
      return null;
    } finally {
      setLoading(false);
    }
  };

  const getGasPrice = async () => {
    try {
      const response = await axios.get(`${API_BASE}/stats/gas`);
      return response.data.result;
    } catch (e: any) {
      return { slow: '10', standard: '20', fast: '30' };
    }
  };

  const search = async (query: string) => {
    setLoading(true);
    try {
      const response = await axios.get(`${API_BASE}/search`, {
        params: { q: query },
      });
      return response.data.result;
    } catch (e: any) {
      setError(e.message);
      return null;
    } finally {
      setLoading(false);
    }
  };

  const getAccount = async (address: string) => {
    setLoading(true);
    try {
      const response = await axios.get(`${API_BASE}/account/${address}`);
      return response.data.result;
    } catch (e: any) {
      setError(e.message);
      return null;
    } finally {
      setLoading(false);
    }
  };

  return {
    loading,
    error,
    getBlock,
    getLatestBlock,
    getTransaction,
    getTokens,
    getNFTs,
    getNetworkStats,
    getGasPrice,
    search,
    getAccount,
  };
};

// Components

const Header = ({ title, onBack }: { title: string; onBack?: () => void }) => (
  <View style={styles.header}>
    {onBack && (
      <TouchableOpacity onPress={onBack} style={styles.backButton}>
        <Text style={styles.backText}>←</Text>
      </TouchableOpacity>
    )}
    <Text style={styles.headerTitle}>{title}</Text>
  </View>
);

const SearchBar = ({ onSearch }: { onSearch: (q: string) => void }) => {
  const [query, setQuery] = useState('');
  
  return (
    <View style={styles.searchContainer}>
      <TextInput
        style={styles.searchInput}
        placeholder="Search blocks, transactions, addresses..."
        placeholderTextColor="#666"
        value={query}
        onChangeText={setQuery}
        onSubmitEditing={() => onSearch(query)}
      />
    </View>
  );
};

const BlockCard = ({ block }: { block: Block }) => (
  <TouchableOpacity style={styles.card}>
    <View style={styles.cardHeader}>
      <Text style={styles.cardTitle}>#{block.number}</Text>
      <Text style={styles.cardTime}>
        {new Date(block.timestamp * 1000).toLocaleTimeString()}
      </Text>
    </View>
    <Text style={styles.cardHash} numberOfLines={1}>
      {block.hash}
    </Text>
    <View style={styles.cardStats}>
      <Text style={styles.cardStat}>
        Gas: {((block.gasUsed / block.gasLimit) * 100).toFixed(1)}%
      </Text>
      <Text style={styles.cardMiner} numberOfLines={1}>
        Miner: {block.miner}
      </Text>
    </View>
  </TouchableOpacity>
);

const TransactionCard = ({ tx }: { tx: Transaction }) => (
  <TouchableOpacity style={styles.card}>
    <View style={styles.cardHeader}>
      <Text style={styles.txHash} numberOfLines={1}>
        {tx.hash}
      </Text>
      <Text style={[
        styles.txStatus,
        { color: tx.status === 1 ? '#4CAF50' : '#F44336' }
      ]}>
        {tx.status === 1 ? '✓' : '✗'}
      </Text>
    </View>
    <View style={styles.cardStats}>
      <Text style={styles.cardStat}>
        From: {tx.from.slice(0, 10)}...
      </Text>
      <Text style={styles.cardStat}>
        To: {tx.to.slice(0, 10)}...
      </Text>
    </View>
    <Text style={styles.txValue}>
      {ethers.formatEther(tx.value)} TSC
    </Text>
  </TouchableOpacity>
);

const TokenCard = ({ token }: { token: Token }) => (
  <TouchableOpacity style={styles.card}>
    <View style={styles.cardHeader}>
      <View style={styles.tokenIcon}>
        <Text style={styles.tokenIconText}>
          {token.symbol.slice(0, 2).toUpperCase()}
        </Text>
      </View>
      <View style={styles.tokenInfo}>
        <Text style={styles.tokenName}>{token.name}</Text>
        <Text style={styles.tokenSymbol}>{token.symbol}</Text>
      </View>
    </View>
    <Text style={styles.tokenPrice}>
      ${(parseFloat(token.price) / 1e8).toFixed(2)}
    </Text>
    <Text style={[
      styles.tokenChange,
      { color: token.change24h >= 0 ? '#4CAF50' : '#F44336' }
    ]}>
      {token.change24h >= 0 ? '+' : ''}{token.change24h.toFixed(2)}%
    </Text>
  </TouchableOpacity>
);

const NFTCard = ({ nft }: { nft: NFT }) => (
  <TouchableOpacity style={styles.card}>
    <View style={styles.nftImage}>
      <Text style={styles.nftImageText}>🖼️</Text>
    </View>
    <Text style={styles.nftName} numberOfLines={1}>
      {nft.name}
    </Text>
    <Text style={styles.nftSymbol}>{nft.symbol}</Text>
    <Text style={styles.nftFloor}>
      Floor: {ethers.formatEther(nft.floorPrice)} TSC
    </Text>
  </TouchableOpacity>
);

const StatCard = ({ label, value }: { label: string; value: string }) => (
  <View style={styles.statCard}>
    <Text style={styles.statLabel}>{label}</Text>
    <Text style={styles.statValue}>{value}</Text>
  </View>
);

// Screens

const HomeScreen = ({ navigation }: any) => {
  const [latestBlock, setLatestBlock] = useState<Block | null>(null);
  const [stats, setStats] = useState<any>(null);
  const [gas, setGas] = useState<any>(null);
  const api = useTigerScan();

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    const block = await api.getLatestBlock();
    if (block) {
      setLatestBlock({
        number: parseInt(block.blockNumber),
        hash: block.hash,
        gasUsed: parseInt(block.gasUsed),
        gasLimit: parseInt(block.gasLimit),
        timestamp: parseInt(block.timestamp),
        miner: block.miner,
      });
    }

    const networkStats = await api.getNetworkStats();
    if (networkStats) {
      setStats(networkStats);
    }

    const gasPrice = await api.getGasPrice();
    if (gasPrice) {
      setGas(gasPrice);
    }
  };

  return (
    <SafeAreaView style={styles.container}>
      <StatusBar barStyle="dark-content" />
      <Header title="TigerScan" />
      
      <ScrollView style={styles.content}>
        {/* Network Stats */}
        {stats && (
          <View style={styles.statsGrid}>
            <StatCard label="Block" value={`#${stats.blockNumber}`} />
            <StatCard label="TPS" value={stats.tps} />
            <StatCard label="Addresses" value={stats.totalAddresses} />
            <StatCard label="Transactions" value={stats.totalTransactions} />
          </View>
        )}

        {/* Gas Prices */}
        {gas && (
          <View style={styles.section}>
            <Text style={styles.sectionTitle}>Gas Price</Text>
            <View style={styles.gasGrid}>
              <View style={styles.gasCard}>
                <Text style={styles.gasLabel}>Slow</Text>
                <Text style={styles.gasValue}>{gas.slow} Gwei</Text>
              </View>
              <View style={styles.gasCard}>
                <Text style={styles.gasLabel}>Standard</Text>
                <Text style={styles.gasValue}>{gas.standard} Gwei</Text>
              </View>
              <View style={styles.gasCard}>
                <Text style={styles.gasLabel}>Fast</Text>
                <Text style={styles.gasValue}>{gas.fast} Gwei</Text>
              </View>
            </View>
          </View>
        )}

        {/* Latest Block */}
        {latestBlock && (
          <View style={styles.section}>
            <Text style={styles.sectionTitle}>Latest Block</Text>
            <BlockCard block={latestBlock} />
          </View>
        )}
      </ScrollView>
    </SafeAreaView>
  );
};

const BlocksScreen = ({ navigation }: any) => {
  const [blocks, setBlocks] = useState<Block[]>([]);
  const api = useTigerScan();

  useEffect(() => {
    loadBlocks();
  }, []);

  const loadBlocks = async () => {
    const latest = await api.getLatestBlock();
    if (latest) {
      const blockNum = parseInt(latest.blockNumber);
      const blocksList = [];
      for (let i = 0; i < 10 && blockNum - i > 0; i++) {
        const block = await api.getBlock(blockNum - i);
        if (block) {
          blocksList.push({
            number: parseInt(block.blockNumber),
            hash: block.hash,
            gasUsed: parseInt(block.gasUsed),
            gasLimit: parseInt(block.gasLimit),
            timestamp: parseInt(block.timestamp),
            miner: block.miner,
          });
        }
      }
      setBlocks(blocksList);
    }
  };

  return (
    <SafeAreaView style={styles.container}>
      <Header title="Blocks" />
      <FlatList
        data={blocks}
        keyExtractor={(item) => item.number.toString()}
        renderItem={({ item }) => <BlockCard block={item} />}
        contentContainerStyle={styles.list}
      />
    </SafeAreaView>
  );
};

const TokensScreen = ({ navigation }: any) => {
  const [tokens, setTokens] = useState<Token[]>([]);
  const api = useTigerScan();

  useEffect(() => {
    loadTokens();
  }, []);

  const loadTokens = async () => {
    const tokensList = await api.getTokens();
    if (tokensList) {
      setTokens(tokensList.map((t: any) => ({
        address: t.address,
        name: t.name,
        symbol: t.symbol,
        price: t.price,
        change24h: parseFloat(t.change24h || 0),
      })));
    }
  };

  return (
    <SafeAreaView style={styles.container}>
      <Header title="Tokens" />
      <FlatList
        data={tokens}
        keyExtractor={(item) => item.address}
        renderItem={({ item }) => <TokenCard token={item} />}
        contentContainerStyle={styles.list}
      />
    </SafeAreaView>
  );
};

const NFTsScreen = ({ navigation }: any) => {
  const [nfts, setNFTs] = useState<NFT[]>([]);
  const api = useTigerScan();

  useEffect(() => {
    loadNFTs();
  }, []);

  const loadNFTs = async () => {
    const nftsList = await api.getNFTs();
    if (nftsList) {
      setNFTs(nftsList.map((n: any) => ({
        address: n.address,
        name: n.name,
        symbol: n.symbol,
        floorPrice: n.floorPrice,
      })));
    }
  };

  return (
    <SafeAreaView style={styles.container}>
      <Header title="NFTs" />
      <FlatList
        data={nfts}
        keyExtractor={(item) => item.address}
        renderItem={({ item }) => <NFTCard nft={item} />}
        contentContainerStyle={styles.list}
      />
    </SafeAreaView>
  );
};

const SearchScreen = ({ navigation }: any) => {
  const [results, setResults] = useState<any>(null);
  const [query, setQuery] = useState('');
  const api = useTigerScan();

  const handleSearch = async (q: string) => {
    const results = await api.search(q);
    setResults(results);
  };

  return (
    <SafeAreaView style={styles.container}>
      <Header title="Search" />
      <SearchBar onSearch={handleSearch} />
      
      {results && (
        <ScrollView style={styles.content}>
          {results.type === 'block' && (
            <BlockCard block={results.data} />
          )}
          {results.type === 'transaction' && (
            <TransactionCard tx={results.data} />
          )}
          {results.type === 'token' && (
            <TokenCard token={results.data} />
          )}
        </ScrollView>
      )}
    </SafeAreaView>
  );
};

const SettingsScreen = ({ navigation }: any) => (
  <SafeAreaView style={styles.container}>
    <Header title="Settings" />
    <View style={styles.content}>
      <Text style={styles.settingsText}>Settings coming soon...</Text>
    </View>
  </SafeAreaView>
);

// Navigation
const Tab = createBottomTabNavigator();
const Stack = createNativeStackNavigator();

const TabNavigator = () => (
  <Tab.Navigator
    screenOptions={{
      headerShown: false,
      tabBarStyle: styles.tabBar,
      tabBarActiveTintColor: '#F7931A',
      tabBarInactiveTintColor: '#666',
    }}
  >
    <Tab.Screen 
      name="Home" 
      component={HomeScreen}
      options={{ tabBarLabel: 'Home', tabBarIcon: () => <Text>🏠</Text> }}
    />
    <Tab.Screen 
      name="Blocks" 
      component={BlocksScreen}
      options={{ tabBarLabel: 'Blocks', tabBarIcon: () => <Text>🧱</Text> }}
    />
    <Tab.Screen 
      name="Tokens" 
      component={TokensScreen}
      options={{ tabBarLabel: 'Tokens', tabBarIcon: () => <Text>💰</Text> }}
    />
    <Tab.Screen 
      name="NFTs" 
      component={NFTsScreen}
      options={{ tabBarLabel: 'NFTs', tabBarIcon: () => <Text>🖼️</Text> }}
    />
    <Tab.Screen 
      name="Search" 
      component={SearchScreen}
      options={{ tabBarLabel: 'Search', tabBarIcon: () => <Text>🔍</Text> }}
    />
  </Tab.Navigator>
);

const App = () => (
  <NavigationContainer>
    <TabNavigator />
  </NavigationContainer>
);

// Styles
const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#fff',
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: 16,
    backgroundColor: '#F7931A',
  },
  backButton: {
    marginRight: 12,
  },
  backText: {
    fontSize: 24,
    color: '#fff',
  },
  headerTitle: {
    fontSize: 20,
    fontWeight: 'bold',
    color: '#fff',
  },
  content: {
    flex: 1,
    padding: 16,
  },
  searchContainer: {
    padding: 16,
    backgroundColor: '#f5f5f5',
  },
  searchInput: {
    backgroundColor: '#fff',
    padding: 12,
    borderRadius: 8,
    fontSize: 16,
  },
  list: {
    padding: 16,
  },
  card: {
    backgroundColor: '#fff',
    padding: 16,
    marginBottom: 12,
    borderRadius: 12,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.1,
    shadowRadius: 4,
    elevation: 3,
  },
  cardHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 8,
  },
  cardTitle: {
    fontSize: 18,
    fontWeight: 'bold',
  },
  cardTime: {
    fontSize: 14,
    color: '#666',
  },
  cardHash: {
    fontSize: 12,
    color: '#666',
    marginBottom: 8,
  },
  cardStats: {
    flexDirection: 'row',
    justifyContent: 'space-between',
  },
  cardStat: {
    fontSize: 14,
    color: '#666',
  },
  cardMiner: {
    fontSize: 14,
    color: '#666',
  },
  statsGrid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    justifyContent: 'space-between',
    marginBottom: 16,
  },
  statCard: {
    width: (SCREEN_WIDTH - 48) / 2,
    backgroundColor: '#f5f5f5',
    padding: 16,
    borderRadius: 8,
    marginBottom: 12,
    alignItems: 'center',
  },
  statLabel: {
    fontSize: 14,
    color: '#666',
  },
  statValue: {
    fontSize: 18,
    fontWeight: 'bold',
    color: '#F7931A',
  },
  section: {
    marginBottom: 16,
  },
  sectionTitle: {
    fontSize: 18,
    fontWeight: 'bold',
    marginBottom: 12,
  },
  gasGrid: {
    flexDirection: 'row',
    justifyContent: 'space-between',
  },
  gasCard: {
    flex: 1,
    backgroundColor: '#f5f5f5',
    padding: 12,
    borderRadius: 8,
    marginHorizontal: 4,
    alignItems: 'center',
  },
  gasLabel: {
    fontSize: 12,
    color: '#666',
  },
  gasValue: {
    fontSize: 16,
    fontWeight: 'bold',
    color: '#F7931A',
  },
  txHash: {
    fontSize: 14,
    color: '#666',
    flex: 1,
  },
  txStatus: {
    fontSize: 16,
    fontWeight: 'bold',
  },
  txValue: {
    fontSize: 16,
    fontWeight: 'bold',
    marginTop: 8,
  },
  tokenIcon: {
    width: 40,
    height: 40,
    borderRadius: 20,
    backgroundColor: '#F7931A',
    justifyContent: 'center',
    alignItems: 'center',
  },
  tokenIconText: {
    color: '#fff',
    fontWeight: 'bold',
  },
  tokenInfo: {
    marginLeft: 12,
    flex: 1,
  },
  tokenName: {
    fontSize: 16,
    fontWeight: 'bold',
  },
  tokenSymbol: {
    fontSize: 14,
    color: '#666',
  },
  tokenPrice: {
    fontSize: 18,
    fontWeight: 'bold',
    marginTop: 8,
  },
  tokenChange: {
    fontSize: 14,
    marginTop: 4,
  },
  nftImage: {
    width: '100%',
    height: 150,
    backgroundColor: '#f5f5f5',
    justifyContent: 'center',
    alignItems: 'center',
    borderRadius: 8,
    marginBottom: 12,
  },
  nftImageText: {
    fontSize: 48,
  },
  nftName: {
    fontSize: 16,
    fontWeight: 'bold',
  },
  nftSymbol: {
    fontSize: 14,
    color: '#666',
  },
  nftFloor: {
    fontSize: 14,
    marginTop: 4,
  },
  tabBar: {
    backgroundColor: '#fff',
    borderTopWidth: 1,
    borderTopColor: '#eee',
  },
  settingsText: {
    fontSize: 16,
    color: '#666',
    textAlign: 'center',
    marginTop: 50,
  },
});

export default App;