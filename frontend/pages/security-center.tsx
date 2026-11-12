/**
 * Security Center
 *
 * Reports what the indexer holds: the malicious_contracts, phishing_urls and
 * scam_tokens tables, and this chain's own contract and token verification
 * coverage.
 *
 * Everything on this page used to be invented in the browser. Eight alerts with
 * made-up addresses (several not valid hex, e.g. "0xHoneypot1"), six threat
 * contracts with names like "FakeToken" and victim counts, five phishing
 * domains, five security gauges with arbitrary scores, and a "Major Exploit
 * Events" table naming PolyNetwork, Wormhole, Ronin, Euler and Curve with
 * multi-hundred-million dollar figures. Those are real incidents on other
 * chains. Showing them beside this chain's own threat counts implies they
 * happened here.
 */

import React, { useState, useEffect, useCallback } from 'react';
import { BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';
import api from '@/lib/api';
import { formatAddress } from '@/lib/utils';

interface ThreatRecord {
  id: number;
  address?: string;
  name?: string;
  symbol?: string;
  threat_type: string;
  confidence: number;
  description: string | null;
  source: string | null;
  first_seen: number | null;
  last_seen?: number | null;
  is_active: boolean;
}

interface PhishingRecord {
  id: number;
  url: string;
  domain: string;
  description: string | null;
  source: string | null;
  first_seen: number | null;
  last_seen: number | null;
  is_active: boolean;
}

interface ThreatSeriesPoint {
  day: string;
  malicious: number;
  phishing: number;
  scam: number;
  total: number;
}

interface SecurityOverview {
  threats: {
    maliciousActive: number;
    maliciousTotal: number;
    phishingActive: number;
    phishingTotal: number;
    scamActive: number;
    scamTotal: number;
    honeypots: number;
    averageConfidence: number;
  };
  coverage: {
    contractsTotal: number;
    contractsVerified: number;
    tokensTotal: number;
    tokensVerified: number;
    tokensFlaggedSpam: number;
  };
}

const useSecurityData = () => {
  const [overview, setOverview] = useState<SecurityOverview | null>(null);
  const [malicious, setMalicious] = useState<ThreatRecord[]>([]);
  const [scamTokens, setScamTokens] = useState<ThreatRecord[]>([]);
  const [phishing, setPhishing] = useState<PhishingRecord[]>([]);
  const [series, setSeries] = useState<ThreatSeriesPoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<string>('all');

  const fetchSecurityData = useCallback(async () => {
    try {
      const [overviewResponse, maliciousResponse, scamResponse, phishingResponse, seriesResponse] =
        await Promise.all([
          api.getSecurityOverview(),
          api.getMaliciousContracts({ limit: 50 }),
          api.getScamTokens({ limit: 50 }),
          api.getPhishingURLs({ limit: 50 }),
          api.getSecurityThreatSeries({ days: 14 }),
        ]);

      setOverview(overviewResponse as unknown as SecurityOverview);
      setMalicious(maliciousResponse.items as unknown as ThreatRecord[]);
      setScamTokens(scamResponse.items as unknown as ThreatRecord[]);
      setPhishing(phishingResponse.items as unknown as PhishingRecord[]);
      setSeries(seriesResponse.items as unknown as ThreatSeriesPoint[]);
      setError(null);
    } catch (err: any) {
      setError(err?.response?.data?.error || 'Could not load security data.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchSecurityData();
    const interval = setInterval(fetchSecurityData, 30000);
    return () => clearInterval(interval);
  }, [fetchSecurityData]);

  // The filter narrows the flagged-contract list. It used to filter a list of
  // invented alerts.
  const filteredContracts = filter === 'all'
    ? [...malicious, ...scamTokens]
    : [...malicious, ...scamTokens].filter((record) => record.threat_type === filter);

  return {
    overview,
    malicious,
    scamTokens,
    phishing,
    contracts: filteredContracts,
    allContracts: [...malicious, ...scamTokens],
    series,
    loading,
    error,
    filter,
    setFilter,
    refetch: fetchSecurityData,
  };
};


// Coverage gauge. It shows verified-code coverage, which is a real property of
// this chain, rather than an arbitrary "security score".
interface CoverageGaugeProps {
  label: string;
  verified: number;
  total: number;
}

const CoverageGauge: React.FC<CoverageGaugeProps> = ({ label, verified, total }) => {
  const percent = total > 0 ? Math.round((verified / total) * 100) : 0;
  const color = percent >= 80 ? '#10b981' : percent >= 50 ? '#f59e0b' : '#ef4444';
  const circumference = 2 * Math.PI * 40;
  const dashOffset = circumference - (percent / 100) * circumference;

  return (
    <div className="score-gauge">
      <svg width="100" height="100" viewBox="0 0 100 100">
        <circle cx="50" cy="50" r="40" fill="none" stroke="#334155" strokeWidth="8" />
        <circle
          cx="50" cy="50" r="40" fill="none" stroke={color} strokeWidth="8"
          strokeDasharray={circumference} strokeDashoffset={dashOffset}
          strokeLinecap="round" transform="rotate(-90 50 50)"
        />
      </svg>
      <div className="gauge-value">
        <div className="gauge-score" style={{ color }}>{percent}%</div>
        <div className="gauge-label">{label}</div>
        <div className="gauge-change">{verified.toLocaleString()} / {total.toLocaleString()}</div>
      </div>
      <style>{`
        .score-gauge { position: relative; display: flex; flex-direction: column; align-items: center; padding: 16px; }
        .gauge-value { position: absolute; top: 50%; transform: translateY(-30%); text-align: center; }
        .gauge-score { font-size: 24px; font-weight: 700; }
        .gauge-label { font-size: 11px; color: #64748b; }
        .gauge-change { font-size: 11px; margin-top: 4px; color: #94a3b8; }
      `}</style>
    </div>
  );
};

// Overview cards component
interface OverviewCardsProps {
  overview: SecurityOverview | null;
}

const OverviewCards: React.FC<OverviewCardsProps> = ({ overview }) => {
  const threats = overview?.threats;
  const coverage = overview?.coverage;

  const cards = [
    {
      label: 'Flagged Contracts',
      value: threats ? Number(threats.maliciousActive).toLocaleString() : '-',
      note: threats ? `${threats.maliciousTotal} recorded in total` : '',
    },
    {
      label: 'Honeypots',
      value: threats ? Number(threats.honeypots).toLocaleString() : '-',
      note: threats ? `average confidence ${Math.round(Number(threats.averageConfidence))}` : '',
    },
    {
      label: 'Phishing URLs',
      value: threats ? Number(threats.phishingActive).toLocaleString() : '-',
      note: threats ? `${threats.phishingTotal} recorded in total` : '',
    },
    {
      label: 'Scam Tokens',
      value: threats ? Number(threats.scamActive).toLocaleString() : '-',
      note: coverage ? `${coverage.tokensFlaggedSpam} tokens marked spam` : '',
    },
  ];

  return (
    <div className="overview-cards">
      {cards.map((card) => (
        <div key={card.label} className="overview-card">
          <div className="card-label">{card.label}</div>
          <div className="card-value">{card.value}</div>
          <div className="card-note">{card.note}</div>
        </div>
      ))}
      <style>{`
        .overview-cards { display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; margin-bottom: 24px; }
        .overview-card { background: #1e293b; border-radius: 12px; padding: 16px; }
        .card-label { font-size: 12px; color: #94a3b8; text-transform: uppercase; margin-bottom: 8px; }
        .card-value { font-size: 24px; font-weight: 700; color: #e2e8f0; }
        .card-note { font-size: 11px; color: #64748b; margin-top: 4px; }
        @media (max-width: 1024px) { .overview-cards { grid-template-columns: repeat(2, 1fr); } }
      `}</style>
    </div>
  );
};

// Flagged contracts table. Rows come from malicious_contracts and scam_tokens.
interface ThreatContractsProps {
  contracts: ThreatRecord[];
}

const ThreatContracts: React.FC<ThreatContractsProps> = ({ contracts }) => {
  if (contracts.length === 0) {
    return (
      <div className="threat-contracts">
        <h3>Flagged Contracts</h3>
        <p style={{ color: '#94a3b8' }}>No contracts are currently flagged.</p>
      </div>
    );
  }

  return (
    <div className="threat-contracts">
      <h3>Flagged Contracts</h3>
      <div className="contracts-list">
        {contracts.map((record) => (
          <a
            key={`${record.threat_type}-${record.id}`}
            className="contract-item"
            href={record.address ? `/address/${record.address}` : undefined}
          >
            <span className="contract-type">{record.threat_type.replace(/_/g, ' ')}</span>
            <span className="contract-name">{record.name || record.symbol || '-'}</span>
            <span className="contract-address">{record.address ? formatAddress(record.address) : '-'}</span>
            <span className="contract-confidence">confidence {record.confidence}</span>
            <span className="contract-source">{record.source || '-'}</span>
          </a>
        ))}
      </div>

      <style>{`
        .threat-contracts { background: #1e293b; border-radius: 12px; padding: 20px; margin-bottom: 24px; }
        .threat-contracts h3 { color: #e2e8f0; margin-bottom: 16px; }
        .contracts-list { display: flex; flex-direction: column; gap: 8px; }
        .contract-item {
          display: grid; grid-template-columns: 120px 1fr 140px 110px 120px;
          align-items: center; padding: 12px; background: #0f172a;
          border-radius: 8px; text-decoration: none;
        }
        .contract-item:hover { background: #16213a; }
        .contract-type {
          padding: 4px 8px; border-radius: 4px; font-size: 10px; font-weight: 600;
          color: white; text-align: center; text-transform: uppercase; background: #ef4444;
        }
        .contract-name { font-weight: 600; color: #e2e8f0; }
        .contract-address { font-size: 11px; color: #3b82f6; font-family: monospace; }
        .contract-confidence { font-size: 12px; color: #f59e0b; }
        .contract-source { font-size: 11px; color: #64748b; }
      `}</style>
    </div>
  );
};

// Phishing URLs component
interface PhishingSitesProps {
  sites: PhishingRecord[];
}

const PhishingSites: React.FC<PhishingSitesProps> = ({ sites }) => {
  if (sites.length === 0) {
    return (
      <div className="phishing-sites">
        <h3>Phishing URLs</h3>
        <p style={{ color: '#94a3b8' }}>No phishing URLs are currently listed.</p>
      </div>
    );
  }

  return (
    <div className="phishing-sites">
      <h3>Phishing URLs</h3>
      <div className="sites-list">
        {sites.map((site) => (
          <div key={site.id} className="site-item">
            <div className="site-info">
              <div className="site-url">{site.url}</div>
              <div className="site-target">{site.domain}</div>
            </div>
            <div className="site-source">{site.source || '-'}</div>
            <div className="site-status">{site.is_active ? 'active' : 'inactive'}</div>
          </div>
        ))}
      </div>

      <style>{`
        .phishing-sites { background: #1e293b; border-radius: 12px; padding: 20px; margin-bottom: 24px; }
        .phishing-sites h3 { color: #e2e8f0; margin-bottom: 16px; }
        .sites-list { display: flex; flex-direction: column; gap: 8px; }
        .site-item {
          display: grid; grid-template-columns: 1fr 140px 90px;
          align-items: center; padding: 12px; background: #0f172a; border-radius: 8px;
        }
        .site-url { font-family: monospace; color: #ef4444; font-size: 13px; word-break: break-all; }
        .site-target { font-size: 11px; color: #64748b; }
        .site-source { font-size: 11px; color: #94a3b8; }
        .site-status { font-size: 12px; color: #94a3b8; }
      `}</style>
    </div>
  );
};

// Threat activity chart, from first_seen timestamps.
interface ThreatSeriesChartProps {
  data: ThreatSeriesPoint[];
}

const ThreatSeriesChart: React.FC<ThreatSeriesChartProps> = ({ data }) => {
  if (data.length === 0) {
    return (
      <div className="security-chart">
        <h3>Threats Recorded (14 days)</h3>
        <p style={{ color: '#94a3b8' }}>No threats have been recorded in this window.</p>
      </div>
    );
  }

  const chartData = data.map((point) => ({
    date: new Date(point.day).toLocaleDateString([], { month: 'short', day: 'numeric' }),
    malicious: Number(point.malicious),
    phishing: Number(point.phishing),
    scam: Number(point.scam),
  }));

  return (
    <div className="security-chart">
      <h3>Threats Recorded (14 days)</h3>
      <ResponsiveContainer width="100%" height={250}>
        <BarChart data={chartData}>
          <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
          <XAxis dataKey="date" stroke="#94a3b8" fontSize={11} />
          <YAxis stroke="#94a3b8" fontSize={11} allowDecimals={false} />
          <Tooltip contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px' }} />
          <Bar dataKey="malicious" fill="#ef4444" name="Contracts" />
          <Bar dataKey="phishing" fill="#f59e0b" name="Phishing" />
          <Bar dataKey="scam" fill="#8b5cf6" name="Scam tokens" />
        </BarChart>
      </ResponsiveContainer>

      <style>{`
        .security-chart { background: #1e293b; border-radius: 12px; padding: 20px; margin-bottom: 24px; }
        .security-chart h3 { color: #e2e8f0; margin-bottom: 16px; }
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
    { value: 'scam', label: 'Scams' },
    { value: 'rugpull', label: 'Rug pulls' },
    { value: 'suspicious', label: 'Suspicious' },
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
        .filter-bar { display: flex; gap: 8px; margin-bottom: 24px; flex-wrap: wrap; }
        .filter-btn {
          padding: 8px 16px; background: #1e293b; border: 1px solid #334155;
          border-radius: 8px; color: #94a3b8; cursor: pointer; transition: all 0.2s;
        }
        .filter-btn:hover { border-color: #ef4444; color: #e2e8f0; }
        .filter-btn.active { background: #ef4444; border-color: #ef4444; color: white; }
      `}</style>
    </div>
  );
};


// Main Security Center component
const SecurityCenter: React.FC = () => {
  const {
    overview, contracts, phishing, series, loading, error, filter, setFilter, refetch,
  } = useSecurityData();

  if (loading && !overview) {
    return (
      <div className="security-center">
        <div className="loading-container">
          <div className="loading-spinner"></div>
          <p>Loading security data...</p>
        </div>
        <style>{`
          .loading-container { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 400px; color: #94a3b8; }
          .loading-spinner {
            width: 40px; height: 40px; border: 3px solid #334155; border-top-color: #ef4444;
            border-radius: 50%; animation: spin 1s linear infinite;
          }
          @keyframes spin { to { transform: rotate(360deg); } }
        `}</style>
      </div>
    );
  }

  if (error && !overview) {
    return (
      <div className="security-center">
        <div className="loading-container">{error}</div>
        <button className="refresh-btn" onClick={refetch}>Retry</button>
      </div>
    );
  }

  const coverage = overview?.coverage;

  return (
    <div className="security-center">
      <div className="page-header">
        <h1>Security Center</h1>
        <p>Flagged addresses, phishing URLs and verification coverage for TigerSmartChain</p>
        <button className="refresh-btn" onClick={refetch}>Refresh</button>
      </div>

      <div className="scores-section">
        <CoverageGauge
          label="Contracts verified"
          verified={Number(coverage?.contractsVerified ?? 0)}
          total={Number(coverage?.contractsTotal ?? 0)}
        />
        <CoverageGauge
          label="Tokens verified"
          verified={Number(coverage?.tokensVerified ?? 0)}
          total={Number(coverage?.tokensTotal ?? 0)}
        />
      </div>

      <OverviewCards overview={overview} />

      <Filter filter={filter} setFilter={setFilter} />

      <div className="dashboard-grid">
        <ThreatContracts contracts={contracts} />
        <PhishingSites sites={phishing} />
      </div>

      <ThreatSeriesChart data={series} />

      <style>{`
        .security-center { padding: 24px; max-width: 1400px; margin: 0 auto; }
        .page-header { margin-bottom: 24px; display: flex; flex-direction: column; }
        .page-header h1 { font-size: 32px; font-weight: 700; color: #e2e8f0; margin-bottom: 8px; }
        .page-header p { color: #94a3b8; }
        .refresh-btn {
          margin-top: 12px; align-self: flex-start; padding: 8px 16px; background: #ef4444;
          border: none; border-radius: 8px; color: white; cursor: pointer; font-weight: 500;
        }
        .refresh-btn:hover { background: #dc2626; }
        .scores-section {
          display: flex; justify-content: space-around; background: #1e293b;
          border-radius: 12px; padding: 16px; margin-bottom: 24px;
        }
        .dashboard-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }
        @media (max-width: 1024px) { .dashboard-grid { grid-template-columns: 1fr; } }
      `}</style>
    </div>
  );
};


export default SecurityCenter;