/**
 * Advanced Security Center - Complete security monitoring and threat detection
 * Real-time security alerts, honeypot detection, phishing monitoring
 */

import React, { useState, useEffect, useCallback } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, PieChart, Pie, Cell, BarChart, Bar } from 'recharts';

// Types for security data
interface SecurityAlert {
  id: string;
  type: 'honeypot' | 'phishing' | 'rug_pull' | 'exploit' | 'suspicious' | 'verified';
  severity: 'critical' | 'high' | 'medium' | 'low' | 'info';
  title: string;
  description: string;
  address: string;
  timestamp: number;
  status: 'active' | 'investigating' | 'resolved' | 'false_positive';
  details: Record<string, string | number>;
}

interface ThreatContract {
  id: string;
  address: string;
  name: string;
  type: 'honeypot' | 'phishing' | 'scam' | 'suspicious';
  detectedAt: number;
  victims: number;
  fundsLost: number;
  status: 'active' | 'flagged' | 'taken_down';
  source: string;
}

interface PhishingSite {
  id: string;
  url: string;
  target: string;
  type: 'impersonation' | 'fake_exchange' | 'fake_ico' | 'fake_airdrops';
  detectedAt: number;
  visits: number;
  reports: number;
  status: 'active' | 'investigating' | 'taken_down';
}

interface ExploitEvent {
  id: string;
  protocol: string;
  type: 'flash_loan' | 'reentrancy' | 'oracle_manipulation' | 'bridge_exploit' | 'rug_pull';
  amount: number;
  timestamp: number;
  blockNumber: number;
  txHash: string;
  status: 'confirmed' | 'suspected' | 'investigating';
}

interface SecurityScore {
  category: string;
  score: number;
  change: number;
  description: string;
}

interface NetworkSecurity {
  timestamp: number;
  alertsCount: number;
  threatsBlocked: number;
  suspiciousAddresses: number;
}

// Advanced security data hook
const useSecurityData = () => {
  const [alerts, setAlerts] = useState<SecurityAlert[]>([]);
  const [threatContracts, setThreatContracts] = useState<ThreatContract[]>([]);
  const [phishingSites, setPhishingSites] = useState<PhishingSite[]>([]);
  const [exploitEvents, setExploitEvents] = useState<ExploitEvent[]>([]);
  const [securityScores, setSecurityScores] = useState<SecurityScore[]>([]);
  const [networkSecurity, setNetworkSecurity] = useState<NetworkSecurity[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<string>('all');

  const fetchSecurityData = useCallback(async () => {
    try {
      setLoading(true);
      const now = Date.now();
      
      // Generate security alerts
      const alertData: SecurityAlert[] = [
        { id: '1', type: 'honeypot', severity: 'critical', title: 'Honeypot Detected', description: 'Contract traps users with fake transfer limits', address: '0x1234567890abcdef1234567890abcdef12345678', timestamp: now - 60000, status: 'active', details: { victims: 45, fundsLost: 125000 } },
        { id: '2', type: 'phishing', severity: 'high', title: 'Phishing Site Active', description: 'Fake airdrop site targeting users', address: '0x2345678901abcdef2345678901abcdef23456789', timestamp: now - 180000, status: 'investigating', details: { targets: 1200 } },
        { id: '3', type: 'rug_pull', severity: 'critical', title: 'Rug Pull Detected', description: 'Developer drained liquidity pool', address: '0x3456789012abcdef3456789012abcdef34567890', timestamp: now - 3600000, status: 'resolved', details: { fundsLost: 850000 } },
        { id: '4', type: 'exploit', severity: 'high', title: 'Reentrancy Vulnerability', description: 'Potential reentrancy in contract', address: '0x4567890123abcdef4567890123abcdef45678901', timestamp: now - 7200000, status: 'investigating', details: { tvl: 2500000 } },
        { id: '5', type: 'suspicious', severity: 'medium', title: 'Unusual Activity', description: 'Large token transfers detected', address: '0x5678901234abcdef5678901234abcdef56789012', timestamp: now - 10800000, status: 'resolved', details: { amount: 500000 } },
        { id: '6', type: 'verified', severity: 'info', title: 'Contract Verified', description: 'New verified contract deployed', address: '0x6789012345abcdef6789012345abcdef67890123', timestamp: now - 14400000, status: 'resolved', details: {} },
        { id: '7', type: 'honeypot', severity: 'critical', title: 'Fake NFT Mint', description: 'Contract never reveals winning chance', address: '0x7890123456abcdef7890123456abcdef78901234', timestamp: now - 18000000, status: 'active', details: { victims: 120, fundsLost: 450000 } },
        { id: '8', type: 'phishing', severity: 'high', title: 'Discord Phishing', description: 'Fake Discord admin posting malicious links', address: '0x8901234567abcdef8901234567abcdef89012345', timestamp: now - 21600000, status: 'investigating', details: { reports: 25 } },
      ];
      setAlerts(alertData);
      
      // Generate threat contracts
      const threatData: ThreatContract[] = [
        { id: '1', address: '0xHoneypot1', name: 'FakeToken', type: 'honeypot', detectedAt: now - 86400000, victims: 250, fundsLost: 450000, status: 'active', source: 'Honeypot Detector' },
        { id: '2', address: '0xPhish1', name: 'AirdropPro', type: 'phishing', detectedAt: now - 172800000, victims: 1200, fundsLost: 850000, status: 'taken_down', source: 'Community Report' },
        { id: '3', address: '0xScam1', name: 'MoonDAO', type: 'scam', detectedAt: now - 259200000, victims: 850, fundsLost: 2500000, status: 'flagged', source: 'Transaction Analyzer' },
        { id: '4', address: '0xSus1', name: 'SuspiciousPool', type: 'suspicious', detectedAt: now - 345600000, victims: 45, fundsLost: 125000, status: 'investigating', source: 'Behavior Analysis' },
        { id: '5', address: '0xHoneypot2', name: 'WinChance', type: 'honeypot', detectedAt: now - 432000000, victims: 180, fundsLost: 320000, status: 'active', source: 'Honeypot Detector' },
        { id: '6', address: '0xRug1', name: 'ExitToken', type: 'rug_pull', detectedAt: now - 518400000, victims: 450, fundsLost: 1200000, status: 'taken_down', source: 'Liquidity Scanner' },
      ];
      setThreatContracts(threatData);
      
      // Generate phishing sites
      const phishingData: PhishingSite[] = [
        { id: '1', url: 'etlhereum-prize[.]xyz', target: 'Ethereum Foundation', type: 'fake_airdrops', detectedAt: now - 3600000, visits: 15000, reports: 125, status: 'investigating' },
        { id: '2', url: 'uni-swap[.]net', target: 'Uniswap', type: 'impersonation', detectedAt: now - 7200000, visits: 8500, reports: 85, status: 'taken_down' },
        { id: '3', url: 'eth2-staking[.]info', target: 'Ethereum Foundation', type: 'fake_ico', detectedAt: now - 14400000, visits: 25000, reports: 320, status: 'active' },
        { id: '4', url: 'opensea-gift[.]com', target: 'OpenSea', type: 'fake_exchange', detectedAt: now - 21600000, visits: 12000, reports: 145, status: 'investigating' },
        { id: '5', url: 'metamask-verify[.]io', target: 'MetaMask', type: 'impersonation', detectedAt: now - 28800000, visits: 35000, reports: 520, status: 'active' },
      ];
      setPhishingSites(phishingData);
      
      // Generate exploit events
      const exploitData: ExploitEvent[] = [
        { id: '1', protocol: 'PolyNetwork', type: 'bridge_exploit', amount: 610000000, timestamp: now - 86400000, blockNumber: 18000000, txHash: '0xabc123', status: 'confirmed' },
        { id: '2', protocol: 'Wormhole', type: 'bridge_exploit', amount: 320000000, timestamp: now - 518400000, blockNumber: 17995000, txHash: '0xdef456', status: 'confirmed' },
        { id: '3', protocol: 'Ronin Network', type: 'bridge_exploit', amount: 625000000, timestamp: now - 1209600000, blockNumber: 17980000, txHash: '0xghi789', status: 'confirmed' },
        { id: '4', protocol: 'Euler Finance', type: 'flash_loan', amount: 197000000, timestamp: now - 2678400000, blockNumber: 17950000, txHash: '0xjkl012', status: 'confirmed' },
        { id: '5', protocol: 'Curve DAO', type: 'reentrancy', amount: 0, timestamp: now - 3196800000, blockNumber: 17920000, txHash: '0xmno345', status: 'suspected' },
      ];
      setExploitEvents(exploitData);
      
      // Generate security scores
      setSecurityScores([
        { category: 'Network Security', score: 92, change: 2.5, description: 'Strong network monitoring' },
        { category: 'Smart Contracts', score: 85, change: -1.2, description: 'Verified contracts' },
        { category: 'Wallet Security', score: 78, change: 5.8, description: 'Multi-sig recommended' },
        { category: 'DeFi Protocols', score: 88, change: 3.2, description: 'Low risk protocols' },
        { category: 'Cross-chain', score: 72, change: -2.5, description: 'Bridge vulnerabilities' },
      ]);
      
      // Generate network security trends
      const networkData: NetworkSecurity[] = [];
      for (let i = 30; i >= 0; i--) {
        const timestamp = now - i * 24 * 60 * 60 * 1000;
        networkData.push({
          timestamp,
          alertsCount: Math.floor(50 + Math.random() * 100),
          threatsBlocked: Math.floor(100 + Math.random() * 200),
          suspiciousAddresses: Math.floor(20 + Math.random() * 50),
        });
      }
      setNetworkSecurity(networkData);
      
      setError(null);
    } catch (err) {
      setError('Failed to fetch security data');
      console.error('Security data error:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchSecurityData();
    const interval = setInterval(fetchSecurityData, 30000);
    return () => clearInterval(interval);
  }, [fetchSecurityData]);

  return { 
    alerts: filter === 'all' ? alerts : alerts.filter(a => a.type === filter), 
    allAlerts: alerts,
    threatContracts, 
    phishingSites, 
    exploitEvents, 
    securityScores, 
    networkSecurity, 
    loading, 
    error, 
    filter,
    setFilter,
    refetch: fetchSecurityData 
  };
};

// Security score gauge component
interface ScoreGaugeProps {
  score: number;
  label: string;
  change: number;
}

const ScoreGauge: React.FC<ScoreGaugeProps> = ({ score, label, change }) => {
  const getColor = (s: number) => {
    if (s >= 90) return '#10b981';
    if (s >= 70) return '#f59e0b';
    return '#ef4444';
  };
  
  const circumference = 2 * Math.PI * 40;
  const dashOffset = circumference - (score / 100) * circumference;
  
  return (
    <div className="score-gauge">
      <svg width="100" height="100" viewBox="0 0 100 100">
        <circle cx="50" cy="50" r="40" fill="none" stroke="#334155" strokeWidth="8" />
        <circle 
          cx="50" cy="50" r="40" 
          fill="none" 
          stroke={getColor(score)} 
          strokeWidth="8"
          strokeDasharray={circumference}
          strokeDashoffset={dashOffset}
          strokeLinecap="round"
          transform="rotate(-90 50 50)"
        />
      </svg>
      <div className="gauge-value">
        <div className="gauge-score" style={{ color: getColor(score) }}>{score}</div>
        <div className="gauge-label">{label}</div>
        <div className={`gauge-change ${change >= 0 ? 'positive' : 'negative'}`}>
          {change >= 0 ? '↑' : '↓'} {Math.abs(change)}%
        </div>
      </div>
      <style>{`
        .score-gauge {
          position: relative;
          display: flex;
          flex-direction: column;
          align-items: center;
          padding: 16px;
        }
        .gauge-value {
          position: absolute;
          top: 50%;
          transform: translateY(-30%);
          text-align: center;
        }
        .gauge-score {
          font-size: 24px;
          font-weight: 700;
        }
        .gauge-label {
          font-size: 11px;
          color: #64748b;
        }
        .gauge-change {
          font-size: 11px;
          margin-top: 4px;
        }
        .gauge-change.positive { color: #10b981; }
        .gauge-change.negative { color: #ef4444; }
      `}</style>
    </div>
  );
};

// Overview cards component
interface OverviewCardsProps {
  alerts: SecurityAlert[];
  threatContracts: ThreatContract[];
  phishingSites: PhishingSite[];
  exploitEvents: ExploitEvent[];
}

const OverviewCards: React.FC<OverviewCardsProps> = ({ 
  alerts, 
  threatContracts, 
  phishingSites,
  exploitEvents 
}) => {
  const activeAlerts = alerts.filter(a => a.status === 'active').length;
  const activeThreats = threatContracts.filter(t => t.status === 'active').length;
  const activePhishing = phishingSites.filter(p => p.status === 'active').length;
  const totalExploits = exploitEvents.reduce((acc, e) => acc + e.amount, 0);
  
  return (
    <div className="overview-cards">
      <div className="overview-card critical">
        <div className="card-icon">🚨</div>
        <div className="card-content">
          <div className="card-label">Active Alerts</div>
          <div className="card-value">{activeAlerts}</div>
        </div>
      </div>
      <div className="overview-card">
        <div className="card-icon">⚠️</div>
        <div className="card-content">
          <div className="card-label">Threat Contracts</div>
          <div className="card-value">{activeThreats}</div>
        </div>
      </div>
      <div className="overview-card">
        <div className="card-icon">🎣</div>
        <div className="card-content">
          <div className="card-label">Phishing Sites</div>
          <div className="card-value">{activePhishing}</div>
        </div>
      </div>
      <div className="overview-card exploit">
        <div className="card-icon">💰</div>
        <div className="card-content">
          <div className="card-label">Exploits (All Time)</div>
          <div className="card-value">${(totalExploits / 1000000000).toFixed(1)}B</div>
        </div>
      </div>
      
      <style>{`
        .overview-cards {
          display: grid;
          grid-template-columns: repeat(4, 1fr);
          gap: 16px;
          margin-bottom: 24px;
        }
        .overview-card {
          display: flex;
          align-items: center;
          gap: 12px;
          background: #1e293b;
          border-radius: 12px;
          padding: 16px;
        }
        .overview-card.critical {
          background: linear-gradient(135deg, #ef4444 0%, #dc2626 100%);
        }
        .overview-card.exploit {
          background: linear-gradient(135deg, #f59e0b 0%, #d97706 100%);
        }
        .card-icon {
          font-size: 28px;
        }
        .card-label {
          font-size: 12px;
          color: rgba(255,255,255,0.7);
          text-transform: uppercase;
        }
        .card-value {
          font-size: 24px;
          font-weight: 700;
          color: white;
        }
        @media (max-width: 1024px) {
          .overview-cards { grid-template-columns: repeat(2, 1fr); }
        }
      `}</style>
    </div>
  );
};

// Alerts list component
interface AlertsListProps {
  alerts: SecurityAlert[];
  onDismiss: (id: string) => void;
}

const AlertsList: React.FC<AlertsListProps> = ({ alerts, onDismiss }) => {
  const severityColors: Record<string, string> = {
    critical: '#ef4444',
    high: '#f59e0b',
    medium: '#3b82f6',
    low: '#64748b',
    info: '#94a3b8',
  };
  
  const statusColors: Record<string, string> = {
    active: '#ef4444',
    investigating: '#f59e0b',
    resolved: '#10b981',
    false_positive: '#64748b',
  };
  
  return (
    <div className="alerts-list">
      <h3>Security Alerts</h3>
      {alerts.length === 0 ? (
        <div className="no-alerts">No alerts to display</div>
      ) : (
        <div className="alerts-table">
          <div className="table-header">
            <span>Severity</span>
            <span>Type</span>
            <span>Title</span>
            <span>Address</span>
            <span>Status</span>
            <span>Action</span>
          </div>
          {alerts.map((alert) => (
            <div key={alert.id} className="table-row">
              <span className="severity" style={{ color: severityColors[alert.severity] }}>
                {alert.severity.toUpperCase()}
              </span>
              <span className="type">{alert.type}</span>
              <span className="title">{alert.title}</span>
              <span className="address">{alert.address.slice(0, 10)}...</span>
              <span className="status" style={{ color: statusColors[alert.status] }}>
                {alert.status}
              </span>
              <button className="dismiss-btn" onClick={() => onDismiss(alert.id)}>Dismiss</button>
            </div>
          ))}
        </div>
      )}
      
      <style>{`
        .alerts-list {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .alerts-list h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
        }
        .no-alerts {
          color: #64748b;
          text-align: center;
          padding: 40px;
        }
        .alerts-table {
          overflow: hidden;
          border-radius: 8px;
        }
        .table-header, .table-row {
          display: grid;
          grid-template-columns: 80px 100px 1fr 100px 100px 80px;
          padding: 12px 16px;
          align-items: center;
        }
        .table-header {
          background: #0f172a;
          color: #94a3b8;
          font-size: 12px;
          text-transform: uppercase;
        }
        .table-row {
          border-bottom: 1px solid #334155;
        }
        .table-row:last-child {
          border-bottom: none;
        }
        .severity {
          font-weight: 600;
          font-size: 11px;
        }
        .type {
          font-size: 12px;
          color: #64748b;
        }
        .title {
          color: #e2e8f0;
        }
        .address {
          font-family: monospace;
          font-size: 12px;
          color: #3b82f6;
        }
        .status {
          font-size: 12px;
          text-transform: capitalize;
        }
        .dismiss-btn {
          padding: 4px 8px;
          background: #334155;
          border: none;
          border-radius: 4px;
          color: #94a3b8;
          font-size: 11px;
          cursor: pointer;
        }
        .dismiss-btn:hover {
          background: #475569;
          color: #e2e8f0;
        }
      `}</style>
    </div>
  );
};

// Threat contracts component
interface ThreatContractsProps {
  contracts: ThreatContract[];
}

const ThreatContracts: React.FC<ThreatContractsProps> = ({ contracts }) => {
  const typeColors: Record<string, string> = {
    honeypot: '#ef4444',
    phishing: '#f59e0b',
    scam: '#dc2626',
    suspicious: '#64748b',
  };
  
  const formatValue = (val: number) => {
    if (val >= 1000000) return `$${(val / 1000000).toFixed(1)}M`;
    if (val >= 1000) return `$${(val / 1000).toFixed(0)}K`;
    return `$${val}`;
  };
  
  return (
    <div className="threat-contracts">
      <h3>Known Threat Contracts</h3>
      <div className="contracts-list">
        {contracts.map((contract) => (
          <div key={contract.id} className="contract-item">
            <div className="contract-type" style={{ backgroundColor: typeColors[contract.type] }}>
              {contract.type}
            </div>
            <div className="contract-info">
              <div className="contract-name">{contract.name}</div>
              <div className="contract-address">{contract.address.slice(0, 14)}...</div>
            </div>
            <div className="contract-stats">
              <div className="contract-victims">{contract.victims} victims</div>
              <div className="contract-lost">{formatValue(contract.fundsLost)} lost</div>
            </div>
            <div className={`contract-status ${contract.status}`}>{contract.status}</div>
            <div className="contract-source">{contract.source}</div>
          </div>
        ))}
      </div>
      
      <style>{`
        .threat-contracts {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .threat-contracts h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
        }
        .contracts-list {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .contract-item {
          display: grid;
          grid-template-columns: 80px 1fr 120px 80px 100px;
          align-items: center;
          padding: 12px;
          background: #0f172a;
          border-radius: 8px;
        }
        .contract-type {
          padding: 4px 8px;
          border-radius: 4px;
          font-size: 10px;
          font-weight: 600;
          color: white;
          text-align: center;
          text-transform: uppercase;
        }
        .contract-name {
          font-weight: 600;
          color: #e2e8f0;
        }
        .contract-address {
          font-size: 11px;
          color: #64748b;
          font-family: monospace;
        }
        .contract-victims {
          font-size: 12px;
          color: #ef4444;
        }
        .contract-lost {
          font-size: 12px;
          color: #f59e0b;
        }
        .contract-status {
          font-size: 12px;
          text-transform: capitalize;
        }
        .contract-status.active { color: #ef4444; }
        .contract-status.flagged { color: #f59e0b; }
        .contract-status.taken_down { color: #10b981; }
        .contract-source {
          font-size: 11px;
          color: #64748b;
        }
      `}</style>
    </div>
  );
};

// Phishing sites component
interface PhishingSitesProps {
  sites: PhishingSite[];
}

const PhishingSites: React.FC<PhishingSitesProps> = ({ sites }) => {
  const typeIcons: Record<string, string> = {
    impersonation: '🎭',
    fake_exchange: '🏦',
    fake_ico: '💰',
    fake_airdrops: '🎁',
  };
  
  return (
    <div className="phishing-sites">
      <h3>Phishing Sites</h3>
      <div className="sites-list">
        {sites.map((site) => (
          <div key={site.id} className="site-item">
            <div className="site-icon">{typeIcons[site.type]}</div>
            <div className="site-info">
              <div className="site-url">{site.url.replace('[.]', '.')}</div>
              <div className="site-target">Targets: {site.target}</div>
            </div>
            <div className="site-stats">
              <div className="site-visits">{site.visits.toLocaleString()} visits</div>
              <div className="site-reports">{site.reports} reports</div>
            </div>
            <div className={`site-status ${site.status}`}>{site.status}</div>
          </div>
        ))}
      </div>
      
      <style>{`
        .phishing-sites {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .phishing-sites h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
        }
        .sites-list {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .site-item {
          display: grid;
          grid-template-columns: 40px 1fr 120px 80px;
          align-items: center;
          padding: 12px;
          background: #0f172a;
          border-radius: 8px;
        }
        .site-icon {
          font-size: 20px;
        }
        .site-url {
          font-family: monospace;
          color: #ef4444;
          font-size: 13px;
        }
        .site-target {
          font-size: 11px;
          color: #64748b;
        }
        .site-visits {
          color: #e2e8f0;
        }
        .site-reports {
          color: #f59e0b;
          font-size: 12px;
        }
        .site-status {
          font-size: 12px;
          text-transform: capitalize;
        }
        .site-status.active { color: #ef4444; }
        .site-status.investigating { color: #f59e0b; }
        .site-status.taken_down { color: #10b981; }
      `}</style>
    </div>
  );
};

// Exploit events component
interface ExploitEventsProps {
  events: ExploitEvent[];
}

const ExploitEvents: React.FC<ExploitEventsProps> = ({ events }) => {
  const typeColors: Record<string, string> = {
    flash_loan: '#3b82f6',
    reentrancy: '#ef4444',
    oracle_manipulation: '#f59e0b',
    bridge_exploit: '#dc2626',
    rug_pull: '#8b5cf6',
  };
  
  const statusColors: Record<string, string> = {
    confirmed: '#ef4444',
    suspected: '#f59e0b',
    investigating: '#3b82f6',
  };
  
  return (
    <div className="exploit-events">
      <h3>Major Exploit Events</h3>
      <div className="events-list">
        {events.map((event) => (
          <div key={event.id} className="event-item">
            <div className="event-type" style={{ backgroundColor: typeColors[event.type] }}>
              {event.type.replace('_', ' ')}
            </div>
            <div className="event-info">
              <div className="event-protocol">{event.protocol}</div>
              <div className="event-hash">{event.txHash.slice(0, 10)}...</div>
            </div>
            <div className="event-amount">${(event.amount / 1000000).toFixed(0)}M</div>
            <div className="event-date">{new Date(event.timestamp).toLocaleDateString()}</div>
            <div className="event-status" style={{ color: statusColors[event.status] }}>{event.status}</div>
          </div>
        ))}
      </div>
      
      <style>{`
        .exploit-events {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .exploit-events h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
        }
        .events-list {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .event-item {
          display: grid;
          grid-template-columns: 140px 1fr 100px 100px 80px;
          align-items: center;
          padding: 12px;
          background: #0f172a;
          border-radius: 8px;
        }
        .event-type {
          padding: 4px 8px;
          border-radius: 4px;
          font-size: 10px;
          font-weight: 600;
          color: white;
          text-align: center;
          text-transform: capitalize;
        }
        .event-protocol {
          font-weight: 600;
          color: #e2e8f0;
        }
        .event-hash {
          font-size: 11px;
          color: #64748b;
          font-family: monospace;
        }
        .event-amount {
          font-weight: 600;
          color: #ef4444;
        }
        .event-date {
          color: #64748b;
          font-size: 12px;
        }
        .event-status {
          font-size: 12px;
          text-transform: capitalize;
        }
      `}</style>
    </div>
  );
};

// Network security chart
interface NetworkSecurityChartProps {
  data: NetworkSecurity[];
}

const NetworkSecurityChart: React.FC<NetworkSecurityChartProps> = ({ data }) => {
  const chartData = data.slice(-14).map(d => ({
    date: new Date(d.timestamp).toLocaleDateString([], { month: 'short', day: 'numeric' }),
    alerts: d.alertsCount,
    blocked: d.threatsBlocked,
  }));
  
  return (
    <div className="security-chart">
      <h3>Security Trends (14 Days)</h3>
      <ResponsiveContainer width="100%" height={250}>
        <BarChart data={chartData}>
          <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
          <XAxis dataKey="date" stroke="#94a3b8" fontSize={11} />
          <YAxis stroke="#94a3b8" fontSize={11} />
          <Tooltip contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px' }} />
          <Bar dataKey="alerts" fill="#ef4444" name="Alerts" />
          <Bar dataKey="blocked" fill="#10b981" name="Blocked" />
        </BarChart>
      </ResponsiveContainer>
      
      <style>{`
        .security-chart {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .security-chart h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
        }
      `}</style>
    </div>
  );
};

// Filter component
interface FilterProps {
  filter: string;
  setFilter: (filter: string) => void;
}

const Filter: React.FC<FilterProps> = ({ filter, setFilter }) => {
  const filters = [
    { value: 'all', label: 'All' },
    { value: 'honeypot', label: 'Honeypots' },
    { value: 'phishing', label: 'Phishing' },
    { value: 'rug_pull', label: 'Rug Pulls' },
    { value: 'exploit', label: 'Exploits' },
    { value: 'suspicious', label: 'Suspicious' },
    { value: 'verified', label: 'Verified' },
  ];
  
  return (
    <div className="filter-bar">
      {filters.map((f) => (
        <button
          key={f.value}
          className={`filter-btn ${filter === f.value ? 'active' : ''}`}
          onClick={() => setFilter(f.value)}
        >
          {f.label}
        </button>
      ))}
      <style>{`
        .filter-bar {
          display: flex;
          gap: 8px;
          margin-bottom: 24px;
          flex-wrap: wrap;
        }
        .filter-btn {
          padding: 8px 16px;
          background: #1e293b;
          border: 1px solid #334155;
          border-radius: 8px;
          color: #94a3b8;
          cursor: pointer;
          transition: all 0.2s;
        }
        .filter-btn:hover {
          border-color: #ef4444;
          color: #e2e8f0;
        }
        .filter-btn.active {
          background: #ef4444;
          border-color: #ef4444;
          color: white;
        }
      `}</style>
    </div>
  );
};

// Main Security Center component
const SecurityCenter: React.FC = () => {
  const { 
    alerts, allAlerts, threatContracts, phishingSites, exploitEvents, securityScores, networkSecurity, loading, error, filter, setFilter, refetch 
  } = useSecurityData();
  
  const handleDismiss = (id: string) => {
    console.log('Dismiss alert:', id);
  };
  
  if (loading && alerts.length === 0) {
    return (
      <div className="security-center">
        <div className="loading-container">
          <div className="loading-spinner"></div>
          <p>Loading security data...</p>
        </div>
        <style>{`
          .loading-container {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            height: 400px;
            color: #94a3b8;
          }
          .loading-spinner {
            width: 40px;
            height: 40px;
            border: 3px solid #334155;
            border-top-color: #ef4444;
            border-radius: 50%;
            animation: spin 1s linear infinite;
          }
          @keyframes spin { to { transform: rotate(360deg); } }
        `}</style>
      </div>
    );
  }
  
  return (
    <div className="security-center">
      <div className="page-header">
        <h1>🛡️ Security Center</h1>
        <p>Real-time security monitoring, threat detection, and alerts</p>
        <button className="refresh-btn" onClick={refetch}>↻ Refresh</button>
      </div>
      
      <div className="scores-section">
        {securityScores.map((score) => (
          <ScoreGauge key={score.category} score={score.score} label={score.category} change={score.change} />
        ))}
      </div>
      
      <OverviewCards 
        alerts={allAlerts}
        threatContracts={threatContracts}
        phishingSites={phishingSites}
        exploitEvents={exploitEvents}
      />
      
      <Filter filter={filter} setFilter={setFilter} />
      
      <AlertsList alerts={alerts} onDismiss={handleDismiss} />
      
      <div className="dashboard-grid">
        <ThreatContracts contracts={threatContracts} />
        <PhishingSites sites={phishingSites} />
      </div>
      
      <ExploitEvents events={exploitEvents} />
      
      <NetworkSecurityChart data={networkSecurity} />
      
      <style>{`
        .security-center {
          padding: 24px;
          max-width: 1400px;
          margin: 0 auto;
        }
        .page-header {
          margin-bottom: 24px;
          display: flex;
          flex-direction: column;
        }
        .page-header h1 {
          font-size: 32px;
          font-weight: 700;
          color: #e2e8f0;
          margin-bottom: 8px;
        }
        .page-header p {
          color: #94a3b8;
        }
        .refresh-btn {
          margin-top: 12px;
          align-self: flex-start;
          padding: 8px 16px;
          background: #ef4444;
          border: none;
          border-radius: 8px;
          color: white;
          cursor: pointer;
          font-weight: 500;
        }
        .refresh-btn:hover { background: #dc2626; }
        .scores-section {
          display: flex;
          justify-content: space-around;
          background: #1e293b;
          border-radius: 12px;
          padding: 16px;
          margin-bottom: 24px;
        }
        .dashboard-grid {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: 24px;
        }
        @media (max-width: 1024px) {
          .dashboard-grid { grid-template-columns: 1fr; }
        }
      `}</style>
    </div>
  );
};

export default SecurityCenter;