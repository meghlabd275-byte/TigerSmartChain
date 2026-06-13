// TigerScan - User Dashboard with Custom Widgets

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface Dashboard { id: string; name: string; widgets: Widget[] }
interface Widget { id: string; type: string; config: any }

export default function UserDashboard() {
  const [dashboards, setDashboards] = useState<Dashboard[]>([])
  const [loading, setLoading] = useState(true)
  const [showCreate, setShowCreate] = useState(false)
  const [newDashboardName, setNewDashboardName] = useState('')

  useEffect(() => { fetchDashboards() }, [])

  const fetchDashboards = async () => {
    try {
      const res = await fetch('https://api.tigerscan.io/v1/user/dashboards')
      const data = await res.json()
      setDashboards(data.dashboards || [])
    } catch (e) { console.error(e) }
    finally { setLoading(false) }
  }

  const createDashboard = async () => {
    await fetch('https://api.tigerscan.io/v1/user/dashboard/create', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: newDashboardName })
    })
    setShowCreate(false)
    setNewDashboardName('')
    fetchDashboards()
  }

  const widgetTypes = [
    { type: 'portfolio', name: 'Portfolio Value', icon: '💰' },
    { type: 'watchlist', name: 'Watchlist', icon: '👁️' },
    { type: 'price', name: 'Price Chart', icon: '📈' },
    { type: 'tx', name: 'Recent Transactions', icon: '📝' },
    { type: 'gas', name: 'Gas Tracker', icon: '⛽' },
    { type: 'nft', name: 'NFT Gallery', icon: '🖼️' },
  ]

  return (
    <div style={styles.container}>
      <Head><title>Dashboard - TigerScan.io</title></Head>
      <header style={styles.header}>
        <Link href="/" style={styles.logo}>🐯 TigerScan</Link>
        <nav style={styles.nav}>
          <Link href="/dashboard">Dashboard</Link>
          <Link href="/alerts">Alerts</Link>
          <Link href="/settings">Settings</Link>
        </nav>
      </header>
      <main style={styles.main}>
        <div style={styles.header}>
          <h1 style={styles.title}>My Dashboards</h1>
          <button onClick={() => setShowCreate(true)} style={styles.button}>+ New Dashboard</button>
        </div>

        {showCreate && (
          <div style={styles.modal}>
            <div style={styles.modalContent}>
              <h3>Create Dashboard</h3>
              <input value={newDashboardName} onChange={e => setNewDashboardName(e.target.value)} placeholder="Dashboard Name" style={styles.input} />
              <div style={styles.modalButtons}>
                <button onClick={createDashboard} style={styles.button}>Create</button>
                <button onClick={() => setShowCreate(false)} style={styles.cancelBtn}>Cancel</button>
              </div>
            </div>
          </div>
        )}

        {loading ? <p>Loading...</p> : (
          <div style={styles.grid}>
            {dashboards.map(d => (
              <div key={d.id} style={styles.card}>
                <h3>{d.name}</h3>
                <p>{d.widgets.length} widgets</p>
                <div style={styles.widgetPreview}>
                  {d.widgets.length === 0 ? <p style={styles.empty}>No widgets yet</p> : 
                    d.widgets.map(w => <span key={w.id} style={styles.widgetTag}>{w.type}</span>)}
                </div>
                <button style={styles.editBtn}>Edit Dashboard</button>
              </div>
            ))}
          </div>
        )}

        <h2 style={styles.subtitle}>Add Widget</h2>
        <div style={styles.widgetGrid}>
          {widgetTypes.map(w => (
            <div key={w.type} style={styles.widgetCard}>
              <span style={styles.widgetIcon}>{w.icon}</span>
              <span>{w.name}</span>
            </div>
          ))}
        </div>
      </main>
    </div>
  )
}

const styles: Record<string, React.CSSProperties> = {
  container: { fontFamily: 'system-ui, sans-serif', minHeight: '100vh', background: '#0a0a0f' },
  header: { display: 'flex', justifyContent: 'space-between', padding: '1rem 2rem', background: '#12121a', borderBottom: '1px solid #2a2a3a' },
  logo: { color: '#ff6b35', fontSize: '1.5rem', fontWeight: 'bold', textDecoration: 'none' },
  nav: { display: 'flex', gap: '1.5rem' },
  main: { maxWidth: '1200px', margin: '0 auto', padding: '2rem' },
  header: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1.5rem' },
  title: { color: '#fff', fontSize: '2rem', margin: 0 },
  button: { padding: '0.75rem 1.5rem', background: '#ff6b35', color: '#fff', border: 'none', borderRadius: '8px', cursor: 'pointer', fontWeight: 'bold' },
  cancelBtn: { padding: '0.75rem 1.5rem', background: '#333', color: '#fff', border: 'none', borderRadius: '8px', cursor: 'pointer' },
  modal: { position: 'fixed', top: 0, left: 0, right: 0, bottom: 0, background: 'rgba(0,0,0,0.8)', display: 'flex', alignItems: 'center', justifyContent: 'center' },
  modalContent: { background: '#12121a', padding: '2rem', borderRadius: '12px', minWidth: '400px' },
  input: { width: '100%', padding: '0.75rem', margin: '1rem 0', borderRadius: '8px', border: '1px solid #333', background: '#1a1a24', color: '#fff' },
  modalButtons: { display: 'flex', gap: '1rem' },
  grid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))', gap: '1.5rem', marginBottom: '3rem' },
  card: { background: '#12121a', padding: '1.5rem', borderRadius: '12px', border: '1px solid #2a2a3a' },
  empty: { color: '#666', fontStyle: 'italic' },
  widgetPreview: { display: 'flex', flexWrap: 'wrap', gap: '0.5rem', margin: '1rem 0' },
  widgetTag: { background: '#1a2a3a', padding: '0.25rem 0.5rem', borderRadius: '4px', fontSize: '0.8rem' },
  editBtn: { width: '100%', padding: '0.5rem', background: '#2a2a3a', color: '#fff', border: 'none', borderRadius: '6px', cursor: 'pointer' },
  subtitle: { color: '#fff', fontSize: '1.5rem', marginTop: '2rem' },
  widgetGrid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(150px, 1fr))', gap: '1rem' },
  widgetCard: { background: '#12121a', padding: '1.5rem', borderRadius: '12px', textAlign: 'center', cursor: 'pointer', border: '1px solid #333' },
  widgetIcon: { fontSize: '2rem', display: 'block', marginBottom: '0.5rem' },
}