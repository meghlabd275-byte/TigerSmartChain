// TigerScan - Security Analysis Page (Honeypot/Phishing Detection)

import { useState } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface SecurityReport {
  address: string
  is_malicious: boolean
  risk_level: string
  honeypot_score: number
  phishing_score: number
  scam_score: number
  findings: Array<{
    category: string
    severity: string
    description: string
    evidence: string
  }>
  recommendations: string[]
}

export default function Security() {
  const [address, setAddress] = useState('')
  const [loading, setLoading] = useState(false)
  const [result, setResult] = useState<SecurityReport | null>(null)
  const [error, setError] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setLoading(true)
    setError('')
    setResult(null)

    try {
      const response = await fetch('https://api.tigerscan.io/v1/security/analyze', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ address })
      })

      const data = await response.json()
      
      if (data.result) {
        setResult(data.result)
      } else {
        setError(data.error || 'Analysis failed')
      }
    } catch (err) {
      setError('Network error')
    } finally {
      setLoading(false)
    }
  }

  const getRiskColor = (level: string) => {
    switch (level) {
      case 'Critical': return '#ff3333'
      case 'High': return '#ff6b33'
      case 'Medium': return '#ffaa00'
      case 'Low': return '#88cc00'
      default: return '#00cc88'
    }
  }

  const getSeverityColor = (severity: string) => {
    switch (severity) {
      case 'Critical': return '#ff3333'
      case 'High': return '#ff6b33'
      case 'Medium': return '#ffaa00'
      default: return '#888'
    }
  }

  return (
    <div style={styles.container}>
      <Head><title>Security Analysis - TigerScan.io</title></Head>
      
      <header style={styles.header}>
        <Link href="/" style={styles.logo}>🐯 TigerScan</Link>
        <nav style={styles.nav}>
          <Link href="/blocks">Blocks</Link>
          <Link href="/security">Security</Link>
        </nav>
      </header>

      <main style={styles.main}>
        <h1 style={styles.title}>Contract Security Analysis</h1>
        
        <form onSubmit={handleSubmit} style={styles.form}>
          <div style={styles.field}>
            <label style={styles.label}>Contract Address</label>
            <input
              type="text"
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              placeholder="0x..."
              style={styles.input}
              required
            />
          </div>
          <button type="submit" disabled={loading} style={styles.submit}>
            {loading ? 'Analyzing...' : 'Analyze Contract'}
          </button>
        </form>

        {error && <div style={styles.error}>{error}</div>}

        {result && (
          <div style={styles.result}>
            {/* Risk Banner */}
            <div style={{...styles.riskBanner, background: result.is_malicious ? '#3a1a1a' : '#1a3a2a'}}>
              <div style={styles.riskIcon}>{result.is_malicious ? '⚠️' : '✅'}</div>
              <div>
                <h2 style={{...styles.riskTitle, color: getRiskColor(result.risk_level)}}>
                  {result.is_malicious ? 'MALICIOUS CONTRACT DETECTED' : 'CONTRACT APPEARS SAFE'}
                </h2>
                <p style={styles.riskLevel}>Risk Level: <strong style={{color: getRiskColor(result.risk_level)}}>{result.risk_level}</strong></p>
              </div>
            </div>

            {/* Scores */}
            <div style={styles.scoresGrid}>
              <div style={styles.scoreCard}>
                <div style={styles.scoreLabel}>Honeypot Score</div>
                <div style={{...styles.scoreValue, color: result.honeypot_score > 0.5 ? '#ff3333' : '#00cc88'}}>
                  {(result.honeypot_score * 100).toFixed(1)}%
                </div>
              </div>
              <div style={styles.scoreCard}>
                <div style={styles.scoreLabel}>Phishing Score</div>
                <div style={{...styles.scoreValue, color: result.phishing_score > 0.5 ? '#ff3333' : '#00cc88'}}>
                  {(result.phishing_score * 100).toFixed(1)}%
                </div>
              </div>
              <div style={styles.scoreCard}>
                <div style={styles.scoreLabel}>Scam Score</div>
                <div style={{...styles.scoreValue, color: result.scam_score > 0.5 ? '#ff3333' : '#00cc88'}}>
                  {(result.scam_score * 100).toFixed(1)}%
                </div>
              </div>
            </div>

            {/* Findings */}
            {result.findings.length > 0 && (
              <div style={styles.findings}>
                <h3>Security Findings</h3>
                {result.findings.map((finding, i) => (
                  <div key={i} style={styles.finding}>
                    <div style={styles.findingHeader}>
                      <span style={{...styles.severity, background: getSeverityColor(finding.severity)}}>{finding.severity}</span>
                      <span style={styles.category}>{finding.category}</span>
                    </div>
                    <p style={styles.description}>{finding.description}</p>
                    <code style={styles.evidence}>{finding.evidence}</code>
                  </div>
                ))}
              </div>
            )}

            {/* Recommendations */}
            <div style={styles.recommendations}>
              <h3>Recommendations</h3>
              <ul>
                {result.recommendations.map((rec, i) => (
                  <li key={i}>{rec}</li>
                ))}
              </ul>
            </div>
          </div>
        )}
      </main>
    </div>
  )
}

const styles: Record<string, React.CSSProperties> = {
  container: { fontFamily: 'system-ui, sans-serif', minHeight: '100vh', background: '#0a0a0f' },
  header: { display: 'flex', justifyContent: 'space-between', padding: '1rem 2rem', background: '#12121a', borderBottom: '1px solid #2a2a3a' },
  logo: { color: '#ff6b35', fontSize: '1.5rem', fontWeight: 'bold', textDecoration: 'none' },
  nav: { display: 'flex', gap: '1.5rem' },
  main: { maxWidth: '900px', margin: '0 auto', padding: '2rem' },
  title: { color: '#fff', fontSize: '2rem', marginBottom: '1.5rem' },
  form: { background: '#12121a', padding: '2rem', borderRadius: '12px', display: 'flex', gap: '1rem', alignItems: 'flex-end' },
  field: { flex: 1 },
  label: { display: 'block', color: '#aaa', marginBottom: '0.5rem' },
  input: { width: '100%', padding: '0.75rem', borderRadius: '8px', border: '1px solid #333', background: '#1a1a24', color: '#fff', fontSize: '1rem' },
  submit: { padding: '0.75rem 2rem', borderRadius: '8px', border: 'none', background: 'linear-gradient(135deg, #ff6b35, #ff8f5a)', color: '#fff', fontWeight: 'bold', cursor: 'pointer' },
  error: { marginTop: '1rem', padding: '1rem', background: '#3a1a1a', color: '#ff6b6b', borderRadius: '8px' },
  result: { marginTop: '2rem' },
  riskBanner: { padding: '1.5rem', borderRadius: '12px', display: 'flex', gap: '1rem', alignItems: 'center', border: '1px solid' },
  riskIcon: { fontSize: '3rem' },
  riskTitle: { margin: 0, fontSize: '1.5rem' },
  riskLevel: { color: '#aaa', margin: '0.5rem 0 0' },
  scoresGrid: { display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '1rem', marginTop: '1.5rem' },
  scoreCard: { background: '#12121a', padding: '1.5rem', borderRadius: '12px', textAlign: 'center' },
  scoreLabel: { color: '#888', fontSize: '0.9rem', marginBottom: '0.5rem' },
  scoreValue: { fontSize: '2rem', fontWeight: 'bold' },
  findings: { marginTop: '1.5rem', background: '#12121a', padding: '1.5rem', borderRadius: '12px' },
  finding: { padding: '1rem', background: '#0d0d14', borderRadius: '8px', marginBottom: '1rem' },
  findingHeader: { display: 'flex', gap: '0.5rem', marginBottom: '0.5rem' },
  severity: { padding: '2px 8px', borderRadius: '4px', fontSize: '0.75rem', fontWeight: 'bold', color: '#fff' },
  category: { color: '#888', fontSize: '0.85rem' },
  description: { color: '#ccc', margin: '0.5rem 0' },
  evidence: { display: 'block', background: '#1a1a24', padding: '0.5rem', borderRadius: '4px', fontSize: '0.8rem', color: '#888' },
  recommendations: { marginTop: '1.5rem', background: '#12121a', padding: '1.5rem', borderRadius: '12px' },
}