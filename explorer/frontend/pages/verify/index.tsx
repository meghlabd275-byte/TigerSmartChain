// TigerScan - Contract Verification Page
// Full Solidity/Vyper contract verification with proxy detection

import { useState } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface VerificationResult {
  id: string
  address: string
  verified: boolean
  compiler_version: string
  license_type: string
  proxy_type?: string
  implementation?: string
  match_type: string
  verified_at: string
}

interface SourceFile {
  name: string
  content: string
}

export default function VerifyContract() {
  const [address, setAddress] = useState('')
  const [compilerVersion, setCompilerVersion] = useState('v0.8.28')
  const [sourceFiles, setSourceFiles] = useState<SourceFile[]>([{ name: '', content: '' }])
  const [optimization, setOptimization] = useState(true)
  const [runs, setRuns] = useState(200)
  const [loading, setLoading] = useState(false)
  const [result, setResult] = useState<VerificationResult | null>(null)
  const [error, setError] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setLoading(true)
    setError('')
    setResult(null)

    try {
      const response = await fetch('https://api.tigerscan.io/v1/verify', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address,
          compiler_version: compilerVersion,
          source_files: sourceFiles.filter(f => f.name && f.content),
          optimization_enabled: optimization,
          optimization_runs: runs
        })
      })

      const data = await response.json()
      
      if (data.result) {
        setResult(data.result)
      } else {
        setError(data.error?.message || 'Verification failed')
      }
    } catch (err) {
      setError('Network error')
    } finally {
      setLoading(false)
    }
  }

  const addSourceFile = () => {
    setSourceFiles([...sourceFiles, { name: '', content: '' }])
  }

  const updateSourceFile = (index: number, field: keyof SourceFile, value: string) => {
    const updated = [...sourceFiles]
    updated[index][field] = value
    setSourceFiles(updated)
  }

  return (
    <div style={styles.container}>
      <Head>
        <title>Verify Contract - TigerScan.io</title>
      </Head>

      <header style={styles.header}>
        <Link href="/" style={styles.logo}>🐯 TigerScan</Link>
        <nav style={styles.nav}>
          <Link href="/blocks">Blocks</Link>
          <Link href="/transactions">Transactions</Link>
          <Link href="/tokens">Tokens</Link>
          <Link href="/verify">Verify</Link>
        </nav>
      </header>

      <main style={styles.main}>
        <h1 style={styles.title}>Verify Contract Source Code</h1>
        
        <form onSubmit={handleSubmit} style={styles.form}>
          <div style={styles.field}>
            <label style={styles.label}>Contract Address *</label>
            <input
              type="text"
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              placeholder="0x..."
              style={styles.input}
              required
            />
          </div>

          <div style={styles.field}>
            <label style={styles.label}>Compiler Version *</label>
            <select
              value={compilerVersion}
              onChange={(e) => setCompilerVersion(e.target.value)}
              style={styles.select}
            >
              <option value="v0.8.28">v0.8.28</option>
              <option value="v0.8.27">v0.8.27</option>
              <option value="v0.8.26">v0.8.26</option>
              <option value="v0.8.25">v0.8.25</option>
              <option value="v0.8.24">v0.8.24</option>
              <option value="v0.8.23">v0.8.23</option>
              <option value="v0.8.22">v0.8.22</option>
              <option value="v0.8.21">v0.8.21</option>
              <option value="v0.8.20">v0.8.20</option>
              <option value="v0.7.6">v0.7.6</option>
              <option value="v0.6.12">v0.6.12</option>
              <option value="v0.5.17">v0.5.17</option>
            </select>
          </div>

          <div style={styles.field}>
            <label style={styles.label}>Source Files *</label>
            {sourceFiles.map((file, index) => (
              <div key={index} style={styles.fileInput}>
                <input
                  type="text"
                  placeholder="Filename (e.g., Contract.sol)"
                  value={file.name}
                  onChange={(e) => updateSourceFile(index, 'name', e.target.value)}
                  style={styles.filename}
                />
                <textarea
                  placeholder="Paste source code here..."
                  value={file.content}
                  onChange={(e) => updateSourceFile(index, 'content', e.target.value)}
                  style={styles.code}
                  rows={10}
                />
              </div>
            ))}
            <button type="button" onClick={addSourceFile} style={styles.addButton}>
              + Add Another File
            </button>
          </div>

          <div style={styles.row}>
            <div style={styles.field}>
              <label style={styles.label}>
                <input
                  type="checkbox"
                  checked={optimization}
                  onChange={(e) => setOptimization(e.target.checked)}
                />
                {' '}Enable Optimization
              </label>
            </div>
            
            {optimization && (
              <div style={styles.field}>
                <label style={styles.label}>Optimization Runs</label>
                <input
                  type="number"
                  value={runs}
                  onChange={(e) => setRuns(parseInt(e.target.value))}
                  style={styles.input}
                  min={1}
                  max={10000}
                />
              </div>
            )}
          </div>

          <button type="submit" disabled={loading} style={styles.submit}>
            {loading ? 'Verifying...' : 'Verify & Publish'}
          </button>
        </form>

        {error && (
          <div style={styles.error}>{error}</div>
        )}

        {result && (
          <div style={result.verified ? styles.success : styles.failure}>
            <h3>{result.verified ? '✅ Contract Verified!' : '❌ Verification Failed'}</h3>
            
            <div style={styles.resultDetails}>
              <p><strong>Address:</strong> {result.address}</p>
              <p><strong>Compiler:</strong> {result.compiler_version}</p>
              <p><strong>License:</strong> {result.license_type}</p>
              <p><strong>Match Type:</strong> {result.match_type}</p>
              
              {result.proxy_type && (
                <p><strong>Proxy Type:</strong> {result.proxy_type}</p>
              )}
              
              {result.implementation && (
                <p><strong>Implementation:</strong> {result.implementation}</p>
              )}
              
              {result.verified_at && (
                <p><strong>Verified:</strong> {new Date(result.verified_at).toLocaleString()}</p>
              )}
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
  form: { background: '#12121a', padding: '2rem', borderRadius: '12px' },
  field: { marginBottom: '1.5rem' },
  label: { display: 'block', color: '#aaa', marginBottom: '0.5rem', fontSize: '0.9rem' },
  input: { width: '100%', padding: '0.75rem', borderRadius: '8px', border: '1px solid #333', background: '#1a1a24', color: '#fff', fontSize: '1rem' },
  select: { width: '100%', padding: '0.75rem', borderRadius: '8px', border: '1px solid #333', background: '#1a1a24', color: '#fff', fontSize: '1rem' },
  fileInput: { marginBottom: '1rem' },
  filename: { width: '100%', padding: '0.5rem', marginBottom: '0.5rem', borderRadius: '6px', border: '1px solid #333', background: '#1a1a24', color: '#fff' },
  code: { width: '100%', padding: '0.75rem', borderRadius: '8px', border: '1px solid #333', background: '#0d0d14', color: '#aaffaa', fontFamily: 'monospace', fontSize: '0.9rem' },
  addButton: { background: 'transparent', border: '1px dashed #444', color: '#888', padding: '0.5rem 1rem', borderRadius: '6px', cursor: 'pointer' },
  row: { display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' },
  submit: { width: '100%', padding: '1rem', borderRadius: '8px', border: 'none', background: 'linear-gradient(135deg, #ff6b35, #ff8f5a)', color: '#fff', fontSize: '1.1rem', fontWeight: 'bold', cursor: 'pointer', marginTop: '1rem' },
  error: { marginTop: '1rem', padding: '1rem', background: '#3a1a1a', color: '#ff6b6b', borderRadius: '8px' },
  success: { marginTop: '1rem', padding: '1.5rem', background: '#1a3a2a', color: '#6bff6b', borderRadius: '8px', border: '1px solid #2a5a2a' },
  failure: { marginTop: '1rem', padding: '1.5rem', background: '#3a1a1a', color: '#ff6b6b', borderRadius: '8px' },
  resultDetails: { marginTop: '1rem', fontSize: '0.95rem' }
}