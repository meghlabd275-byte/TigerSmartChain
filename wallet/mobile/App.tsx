// TigerSmartChain Mobile Wallet
// React Native mobile wallet application

import React, { useState, useEffect } from 'react';
import {
  View,
  Text,
  TextInput,
  TouchableOpacity,
  StyleSheet,
  SafeAreaView,
  ScrollView,
  Alert,
  ActivityIndicator,
} from 'react-native';
import { ethers } from 'ethers';

// Network configuration
const NETWORKS = {
  mainnet: {
    name: 'TigerSmartChain Mainnet',
    chainId: 6666,
    rpcUrl: 'https://mainnet.tigersmartchain.com',
    explorerUrl: 'https://scan.tigersmartchain.com',
  },
  testnet: {
    name: 'TigerSmartChain Testnet',
    chainId: 6667,
    rpcUrl: 'https://testnet.tigersmartchain.com',
    explorerUrl: 'https://testnet.scan.tigersmartchain.com',
  },
};

// App Component
export default function App() {
  const [screen, setScreen] = useState<'connect' | 'home' | 'send' | 'receive'>('connect');
  const [wallet, setWallet] = useState<ethers.Wallet | null>(null);
  const [account, setAccount] = useState<string>('');
  const [balance, setBalance] = useState<string>('0');
  const [network, setNetwork] = useState<string>('mainnet');
  const [loading, setLoading] = useState<boolean>(false);
  const [toAddress, setToAddress] = useState<string>('');
  const [amount, setAmount] = useState<string>('');

  // Initialize wallet
  useEffect(() => {
    initWallet();
  }, []);

  const initWallet = async () => {
    try {
      // Try to load saved wallet
      // In production, use secure storage
      setLoading(true);
      
      // Create or load wallet
      const wallet = ethers.Wallet.createRandom();
      setWallet(wallet);
      setAccount(wallet.address);
      
      // Get balance
      await updateBalance(wallet);
    } catch (error) {
      Alert.alert('Error', 'Failed to initialize wallet');
    } finally {
      setLoading(false);
    }
  };

  const updateBalance = async (w: ethers.Wallet) => {
    try {
      const provider = new ethers.JsonRpcProvider(NETWORKS[network as keyof typeof NETWORKS].rpcUrl);
      const bal = await provider.getBalance(w.address);
      setBalance(ethers.formatEther(bal));
    } catch (error) {
      console.log('Failed to update balance');
    }
  };

  const handleSend = async () => {
    if (!wallet || !toAddress || !amount) {
      Alert.alert('Error', 'Please fill all fields');
      return;
    }

    try {
      setLoading(true);
      const provider = new ethers.JsonRpcProvider(NETWORKS[network as keyof typeof NETWORKS].rpcUrl);
      const connectedWallet = wallet.connect(provider);

      const tx = await connectedWallet.sendTransaction({
        to: toAddress,
        value: ethers.parseEther(amount),
      });

      Alert.alert('Success', `Transaction sent: ${tx.hash}`);
      setToAddress('');
      setAmount('');
      
      // Update balance
      await updateBalance(wallet);
    } catch (error: any) {
      Alert.alert('Error', error.message || 'Transaction failed');
    } finally {
      setLoading(false);
    }
  };

  const handleCopyAddress = () => {
    // Use clipboard in production
    Alert.alert('Address', account);
  };

  // Connect Screen
  if (screen === 'connect') {
    return (
      <SafeAreaView style={styles.container}>
        <View style={styles.content}>
          <Text style={styles.logo}>🐯</Text>
          <Text style={styles.title}>TigerWallet</Text>
          <Text style={styles.subtitle}>TigerSmartChain Mobile Wallet</Text>
          
          <View style={styles.networkSelector}>
            <TouchableOpacity
              style={[styles.networkButton, network === 'mainnet' && styles.networkActive]}
              onPress={() => setNetwork('mainnet')}
            >
              <Text style={styles.networkText}>Mainnet</Text>
            </TouchableOpacity>
            <TouchableOpacity
              style={[styles.networkButton, network === 'testnet' && styles.networkActive]}
              onPress={() => setNetwork('testnet')}
            >
              <Text style={styles.networkText}>Testnet</Text>
            </TouchableOpacity>
          </View>

          {loading ? (
            <ActivityIndicator size="large" color="#FF6B00" />
          ) : (
            <TouchableOpacity style={styles.button} onPress={() => setScreen('home')}>
              <Text style={styles.buttonText}>Enter Wallet</Text>
            </TouchableOpacity>
          )}
        </View>
      </SafeAreaView>
    );
  }

  // Home Screen
  if (screen === 'home') {
    return (
      <SafeAreaView style={styles.container}>
        <View style={styles.header}>
          <Text style={styles.networkLabel}>{NETWORKS[network as keyof typeof NETWORKS].name}</Text>
        </View>

        <ScrollView style={styles.content}>
          <View style={styles.balanceCard}>
            <Text style={styles.balanceLabel}>Balance</Text>
            <Text style={styles.balanceValue}>{balance}</Text>
            <Text style={styles.balanceSymbol}>TGR</Text>
          </View>

          <TouchableOpacity style={styles.addressCard} onPress={handleCopyAddress}>
            <Text style={styles.addressLabel}>Your Address</Text>
            <Text style={styles.addressValue}>
              {account.slice(0, 10)}...{account.slice(-8)}
            </Text>
          </TouchableOpacity>

          <View style={styles.actions}>
            <TouchableOpacity
              style={styles.actionButton}
              onPress={() => setScreen('send')}
            >
              <Text style={styles.actionIcon}>↑</Text>
              <Text style={styles.actionText}>Send</Text>
            </TouchableOpacity>

            <TouchableOpacity
              style={styles.actionButton}
              onPress={() => setScreen('receive')}
            >
              <Text style={styles.actionIcon}>↓</Text>
              <Text style={styles.actionText}>Receive</Text>
            </TouchableOpacity>
          </View>
        </ScrollView>

        <View style={styles.tabBar}>
          <TouchableOpacity style={styles.tab} onPress={() => setScreen('home')}>
            <Text style={styles.tabText}>Home</Text>
          </TouchableOpacity>
          <TouchableOpacity style={styles.tab} onPress={() => setScreen('send')}>
            <Text style={styles.tabText}>Send</Text>
          </TouchableOpacity>
          <TouchableOpacity style={styles.tab} onPress={() => setScreen('receive')}>
            <Text style={styles.tabText}>Receive</Text>
          </TouchableOpacity>
        </View>
      </SafeAreaView>
    );
  }

  // Send Screen
  if (screen === 'send') {
    return (
      <SafeAreaView style={styles.container}>
        <View style={styles.header}>
          <TouchableOpacity onPress={() => setScreen('home')}>
            <Text style={styles.backButton}>← Back</Text>
          </TouchableOpacity>
          <Text style={styles.headerTitle}>Send TGR</Text>
        </View>

        <View style={styles.content}>
          <View style={styles.inputGroup}>
            <Text style={styles.inputLabel}>Recipient Address</Text>
            <TextInput
              style={styles.input}
              value={toAddress}
              onChangeText={setToAddress}
              placeholder="0x..."
              placeholderTextColor="#666"
            />
          </View>

          <View style={styles.inputGroup}>
            <Text style={styles.inputLabel}>Amount (TGR)</Text>
            <TextInput
              style={styles.input}
              value={amount}
              onChangeText={setAmount}
              placeholder="0.0"
              placeholderTextColor="#666"
              keyboardType="decimal-pad"
            />
          </View>

          {loading ? (
            <ActivityIndicator size="large" color="#FF6B00" />
          ) : (
            <TouchableOpacity style={styles.button} onPress={handleSend}>
              <Text style={styles.buttonText}>Send</Text>
            </TouchableOpacity>
          )}
        </View>
      </SafeAreaView>
    );
  }

  // Receive Screen
  if (screen === 'receive') {
    return (
      <SafeAreaView style={styles.container}>
        <View style={styles.header}>
          <TouchableOpacity onPress={() => setScreen('home')}>
            <Text style={styles.backButton}>← Back</Text>
          </TouchableOpacity>
          <Text style={styles.headerTitle}>Receive TGR</Text>
        </View>

        <View style={styles.content}>
          <View style={styles.qrPlaceholder}>
            <Text style={styles.qrText}>QR Code</Text>
            <Text style={styles.qrAddress}>{account}</Text>
          </View>

          <TouchableOpacity style={styles.button} onPress={handleCopyAddress}>
            <Text style={styles.buttonText}>Copy Address</Text>
          </TouchableOpacity>
        </View>
      </SafeAreaView>
    );
  }

  return null;
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#1a1a2e',
  },
  header: {
    padding: 20,
    borderBottomWidth: 1,
    borderBottomColor: '#2a2a4e',
  },
  headerTitle: {
    color: '#fff',
    fontSize: 18,
    fontWeight: 'bold',
  },
  content: {
    flex: 1,
    padding: 20,
  },
  logo: {
    fontSize: 60,
    textAlign: 'center',
  },
  title: {
    color: '#fff',
    fontSize: 28,
    fontWeight: 'bold',
    textAlign: 'center',
    marginTop: 10,
  },
  subtitle: {
    color: '#888',
    fontSize: 14,
    textAlign: 'center',
  },
  networkSelector: {
    flexDirection: 'row',
    justifyContent: 'center',
    marginTop: 30,
  },
  networkButton: {
    padding: 10,
    margin: 5,
    borderRadius: 20,
    borderWidth: 1,
    borderColor: '#333',
  },
  networkActive: {
    backgroundColor: '#FF6B00',
    borderColor: '#FF6B00',
  },
  networkText: {
    color: '#fff',
  },
  button: {
    backgroundColor: '#FF6B00',
    padding: 15,
    borderRadius: 10,
    marginTop: 30,
  },
  buttonText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: 'bold',
    textAlign: 'center',
  },
  networkLabel: {
    color: '#FF6B00',
    fontSize: 12,
  },
  balanceCard: {
    backgroundColor: '#2a2a4e',
    padding: 20,
    borderRadius: 15,
    marginBottom: 20,
  },
  balanceLabel: {
    color: '#888',
    fontSize: 14,
  },
  balanceValue: {
    color: '#fff',
    fontSize: 36,
    fontWeight: 'bold',
  },
  balanceSymbol: {
    color: '#888',
    fontSize: 16,
  },
  addressCard: {
    backgroundColor: '#2a2a4e',
    padding: 15,
    borderRadius: 10,
    marginBottom: 20,
  },
  addressLabel: {
    color: '#888',
    fontSize: 12,
  },
  addressValue: {
    color: '#fff',
    fontSize: 14,
    marginTop: 5,
  },
  actions: {
    flexDirection: 'row',
    justifyContent: 'space-between',
  },
  actionButton: {
    backgroundColor: '#2a2a4e',
    padding: 20,
    borderRadius: 10,
    width: '48%',
    alignItems: 'center',
  },
  actionIcon: {
    fontSize: 24,
    color: '#FF6B00',
  },
  actionText: {
    color: '#fff',
    marginTop: 10,
  },
  tabBar: {
    flexDirection: 'row',
    borderTopWidth: 1,
    borderTopColor: '#2a2a4e',
    padding: 10,
  },
  tab: {
    flex: 1,
    alignItems: 'center',
  },
  tabText: {
    color: '#888',
  },
  backButton: {
    color: '#FF6B00',
    fontSize: 16,
  },
  inputGroup: {
    marginBottom: 20,
  },
  inputLabel: {
    color: '#888',
    marginBottom: 5,
  },
  input: {
    backgroundColor: '#2a2a4e',
    padding: 15,
    borderRadius: 10,
    color: '#fff',
  },
  qrPlaceholder: {
    backgroundColor: '#2a2a4e',
    padding: 40,
    borderRadius: 15,
    alignItems: 'center',
    marginBottom: 20,
  },
  qrText: {
    color: '#888',
    marginBottom: 10,
  },
  qrAddress: {
    color: '#fff',
    fontSize: 12,
    textAlign: 'center',
  },
});