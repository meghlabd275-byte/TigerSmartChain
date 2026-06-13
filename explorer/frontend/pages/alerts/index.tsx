// TigerScan - Alerts Page

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface Alert { id: string; type: string; address: string; condition: string; threshold: string; active: boolean }
interface Watchlist { id: string; name: string; addresses: Array<{ address: string; label: string }> }

export default function Alerts() {
  const [alerts, setAlerts] = useState<Alert[]>([])
  const [watchlists, setWatchlists] = useState<Watchlist[]>([])
  const [loading, setLoading] = useState(true)
  const [showCreate, setShowCreate] = useState(false)
  const [newAlert, setNewAlert] = useState({ address: '', type: 'price_above', threshold: '' })

  useEffect(() => { fetchData() }, [])

  const fetchData = async () => {
    try {
      const [alertsRes, watchlistRes] = await Promise.all([
        fetch('https://api.tigerscan.io/v1/user/alerts'),
        fetch('https://api.tigerscan.io/v1/user/watchlists')
      ])
      const alertsData = await alertsRes.json()
      const watchlistData = await watchlistRes.json()
      setAlerts(alertsData.alerts || [])
      setWatchlists(watchlistData.watchlists || [])
    } catch (e) { console.error(e) }
    finally { setLoading(false) }
  }

  const createAlert = async () => {
    await fetch('https://api.tigerscan.io/v1/user/alert/create', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(newAlert)
    })
    setShowCreate(false)
    setNewAlert({ address: '', type: 'price_above', threshold: '' })
    fetchData()
  }

  const toggleAlert = async (id: string, active: boolean) => {
    await fetch('https://api.tigerscan.io/v1/user/alert/toggle', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id, active: !active })
    })
    fetchData()
  }

  const deleteAlert = async (id: string) => {
    await fetch('https://api.tigerscan.io/v1/user/alert/delete', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id })
    })
    fetchData()
  }

  const alertTypes = [
    { value: 'price_above', label: 'Price Above', icon: '📈' },
    { value: 'price_below', label: 'Price Below', icon: '📉' },
    { value: 'tx_confirmed', label: 'Transaction Confirmed', icon: '✓' },
    { value: 'whale', label: 'Whale Movement', icon: '🐋' },
  ]

  return (
    <div style={styles.container}>
      <Head><title>Alerts - TigerScan.io</title></Head>
      <header style={styles.header}>
        <Link href="/" style={styles.logo}>🐯 TigerScan</Link>
        <nav style={styles.nav}>
          <Link href="/dashboard">Dashboard</Link>
          <Link href="/alerts">Alerts</Link>
        </nav>
      </header>
      <main style={styles.main}>
        <div style={styles.header}>
          <h1 style={styles.title}>Alerts & Watchlists</h1>
          <button onClick={() => setShowCreate(true)} style={styles.button}>+ New Alert</button>
        </div>

        {showCreate && (
          <div style={styles.modal}>
            <div style={styles.modalContent}>
              <h3>Create Alert</h3>
              <div style={styles.field}>
                <label>Alert Type</label>
                <select value={newAlert.type} onChange={e => setNewAlert(a => ({ ...a, type: e.target.value }))} style={styles.select}>
                  {alertTypes.map(t => <option key={t.value} value={t.value}>{t.icon} {t.label}</option>)}
                </select>
              </div>
              <div style={styles.field}>
                <label>Token Address</label>
                <input value={newAlert.address} onChange={e => setNewAlert(a => ({ ...a, address: e.target.value }))} placeholder="0x..." style={styles.input} />
              </div>
              <div style={styles.field}>
                <label>Threshold</label>
                <input value={newAlert.threshold} onChange={e => setNewAlert(a => ({ ...a, threshold: e.target.value }))} placeholder="1000" style={styles.input} />
              </div>
              <div style={styles.modalButtons}>
                <button onClick={createAlert} style={styles.button}>Create</button>
                <button onClick={() => setShowCreate(false)} style={styles.cancelBtn}>Cancel</button>
              </div>
            </div>
          </div>
        )}

        <section style={styles.section}>
          <h2>Price Alerts</h2>
          {loading ? <p>Loading...</p> : alerts.length === 0 ? (
            <p style={styles.empty}>No alerts yet. Create one to get started!</p>
          ) : (
            <div style={styles.list}>
              {alerts.map(alert => (
                <div key={alert.id} style={styles.alertCard}>
                  <div style={styles.alertHeader}>
                    <span style={styles.alertType}>{alert.type}</span>
                    <label style={styles.switch}>
                      <input type="checkbox" checked={alert.active} onChange={() => toggleAlert(alert.id, alert.active)} />
                      <span style={styles.slider}></span>
                    </label>
                  </div>
                  <div style={styles.alertInfo}>
                    <code>{alert.address}</code>
                    <span>{alert.condition} {alert.threshold}</span>
                  </div>
                  <button onClick={() => deleteAlert(alert.id)} style={styles.deleteBtn}>Delete</button>
                </div>
              ))}
            </div>
          )}
        </section>

        <section style={styles.section}>
          <h2>Watchlists</h2>
          <div style={styles.list}>
            {watchlists.map(list => (
              <div key={list.id} style={styles.watchlistCard}>
                <h3>{list.name}</h3>
                <div style={styles.addresses}>
                  {list.addresses.map((addr, i) => (
                    <div key={i} style={styles.addressItem}>
                      <span style={styles.label}>{addr.label}</span>
                      <code>{addr.address.slice(0, 6)}...{addr.address.slice(-4)}</code>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </section>
      </main>
    </div>
  )
}

const styles: Record<string, React.CSSProperties> = {
  container: { fontFamily: 'system-ui, sans-serif', minHeight: '100vh', background: '#0a0a0f' },
  header: { display: 'flex', justifyContent: 'space-between', padding: '1rem 2rem', background: '#12121a', borderBottom: '1px solid #2a2a3a' },
  logo: { color: '#ff6b35', fontSize: '1.5rem', fontWeight: 'bold', textDecoration: 'none' },
  nav: { display: 'flex', gap: '1.5rem' },
  main: { maxWidth: '1000px', margin: '0 auto', padding: '2rem' },
  header: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1.5rem' },
  title: { color: '#fff', fontSize: '2rem', margin: 0 },
  button: { padding: '0.75rem 1.5rem', background: '#ff6b35', color: '#fff', border: 'none', borderRadius: '8px', cursor: 'pointer', fontWeight: 'bold' },
  cancelBtn: { padding: '0.75rem 1.5rem', background: '#333', color: '#fff', border: 'none', borderRadius: '8px', cursor: 'pointer' },
  modal: { position: 'fixed', top: 0, left: 0, right: 0, bottom: 0, background: 'rgba(0,0,0,0.8)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 100 },
  modalContent: { background: '#12121a', padding: '2rem', borderRadius: '12px', minWidth: '400px' },
  field: { marginBottom: '1rem' },
  input: { width: '100%', padding: '0.75rem', borderRadius: '8px', border: '1px solid #333', background: '#1a1a24', color: '#fff' },
  select: { width: '100%', padding: '0.75rem', borderRadius: '8px', border: '1px solid #333', background: '#1a1a24', color: '#fff' },
  modalButtons: { display: 'flex', gap: '1rem', marginTop: '1.5rem' },
  section: { background: '#12121a', padding: '1.5rem', borderRadius: '12px', marginBottom: '1.5rem' },
  empty: { color: '#666', fontStyle: 'italic' },
  list: { display: 'flex', flexDirection: 'column', gap: '1rem' },
  alertCard: { background: '#1a1a24', padding: '1rem', borderRadius: '8px' },
  alertHeader: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.5rem' },
  alertType: { color: '#ff6b35', fontWeight: 'bold' },
  switch: { position: 'relative', display: 'inline-block', width: '40px', height: '20px' },
  slider: { position: 'absolute', cursor: 'pointer', top: 0, left: 0, right: 0, bottom: 0, background: '#333', borderRadius: '20px', transition: '0.3s' },
  alertInfo: { display: 'flex', justifyContent: 'space-between', color: '#888' },
  deleteBtn: { marginTop: '0.5rem', padding: '0.25rem 0.5rem', background: '#3a1a1a', color: '#ff6b6b', border: 'none', borderRadius: '4px', cursor: 'pointer', fontSize: '0.8rem' },
  watchlistCard: { background: '#1a1a24', padding: '1rem', borderRadius: '8px' },
  addresses: { display: 'flex', flexDirection: 'column', gap: '0.5rem', marginTop: '0.5rem' },
  addressItem: { display: 'flex', justifyContent: 'space-between' },
  label: { color: '#888' },
}