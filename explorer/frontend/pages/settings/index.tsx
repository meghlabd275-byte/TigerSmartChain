// TigerScan - User Settings Page

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface User { id: string; email: string; username: string; preferences: Preferences }
interface Preferences { theme: string; currency: string; timezone: string; notifications: NotificationPrefs }
interface NotificationPrefs { email: boolean; push: boolean; telegram: boolean; price_alerts: boolean; tx_alerts: boolean }

export default function Settings() {
  const [user, setUser] = useState<User | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [message, setMessage] = useState('')
  const [prefs, setPrefs] = useState<Preferences>({ theme: 'dark', currency: 'USD', timezone: 'UTC', notifications: { email: true, push: false, telegram: false, price_alerts: true, tx_alerts: true } })

  useEffect(() => { fetchUser() }, [])

  const fetchUser = async () => {
    try {
      const res = await fetch('https://api.tigerscan.io/v1/user/me')
      const data = await res.json()
      setUser(data.user)
      if (data.user?.preferences) setPrefs(data.user.preferences)
    } catch (e) { console.error(e) }
    finally { setLoading(false) }
  }

  const saveSettings = async () => {
    setSaving(true)
    try {
      await fetch('https://api.tigerscan.io/v1/user/settings', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ preferences: prefs })
      })
      setMessage('Settings saved!')
    } catch (e) { setMessage('Error saving settings') }
    finally { setSaving(false) }
  }

  const updateNotification = (key: keyof NotificationPrefs) => {
    setPrefs(p => ({ ...p, notifications: { ...p.notifications, [key]: !p.notifications[key] } }))
  }

  return (
    <div style={styles.container}>
      <Head><title>Settings - TigerScan.io</title></Head>
      <header style={styles.header}>
        <Link href="/" style={styles.logo}>🐯 TigerScan</Link>
        <nav style={styles.nav}>
          <Link href="/dashboard">Dashboard</Link>
          <Link href="/settings">Settings</Link>
        </nav>
      </header>
      <main style={styles.main}>
        <h1 style={styles.title}>Settings</h1>

        {message && <div style={styles.message}>{message}</div>}

        <section style={styles.section}>
          <h2>Profile</h2>
          <div style={styles.field}>
            <label>Username</label>
            <input value={user?.username || ''} disabled style={styles.input} />
          </div>
          <div style={styles.field}>
            <label>Email</label>
            <input value={user?.email || ''} disabled style={styles.input} />
          </div>
        </section>

        <section style={styles.section}>
          <h2>Preferences</h2>
          <div style={styles.field}>
            <label>Theme</label>
            <select value={prefs.theme} onChange={e => setPrefs(p => ({ ...p, theme: e.target.value })} style={styles.select}>
              <option value="dark">Dark</option>
              <option value="light">Light</option>
              <option value="auto">Auto</option>
            </select>
          </div>
          <div style={styles.field}>
            <label>Currency</label>
            <select value={prefs.currency} onChange={e => setPrefs(p => ({ ...p, currency: e.target.value }))} style={styles.select}>
              <option value="USD">USD</option>
              <option value="EUR">EUR</option>
              <option value="GBP">GBP</option>
              <option value="CNY">CNY</option>
            </select>
          </div>
          <div style={styles.field}>
            <label>Timezone</label>
            <select value={prefs.timezone} onChange={e => setPrefs(p => ({ ...p, timezone: e.target.value }))} style={styles.select}>
              <option value="UTC">UTC</option>
              <option value="EST">EST</option>
              <option value="PST">PST</option>
              <option value="CET">CET</option>
            </select>
          </div>
        </section>

        <section style={styles.section}>
          <h2>Notifications</h2>
          <div style={styles.checkbox}>
            <input type="checkbox" checked={prefs.notifications.email} onChange={() => updateNotification('email')} />
            <span>Email Notifications</span>
          </div>
          <div style={styles.checkbox}>
            <input type="checkbox" checked={prefs.notifications.price_alerts} onChange={() => updateNotification('price_alerts')} />
            <span>Price Alerts</span>
          </div>
          <div style={styles.checkbox}>
            <input type="checkbox" checked={prefs.notifications.tx_alerts} onChange={() => updateNotification('tx_alerts')} />
            <span>Transaction Alerts</span>
          </div>
          <div style={styles.checkbox}>
            <input type="checkbox" checked={prefs.notifications.telegram} onChange={() => updateNotification('telegram')} />
            <span>Telegram Notifications</span>
          </div>
        </section>

        <section style={styles.section}>
          <h2>API Keys</h2>
          <p style={styles.info}>Manage your API keys for programmatic access.</p>
          <button style={styles.button}>Generate New API Key</button>
        </section>

        <section style={styles.section}>
          <h2>Connected Accounts</h2>
          <div style={styles.connection}>
            <span>Telegram</span>
            <button style={styles.connectBtn}>Connect</button>
          </div>
          <div style={styles.connection}>
            <span>Discord</span>
            <button style={styles.connectBtn}>Connect</button>
          </div>
        </section>

        <button onClick={saveSettings} disabled={saving} style={styles.saveBtn}>
          {saving ? 'Saving...' : 'Save Settings'}
        </button>
      </main>
    </div>
  )
}

const styles: Record<string, React.CSSProperties> = {
  container: { fontFamily: 'system-ui, sans-serif', minHeight: '100vh', background: '#0a0a0f' },
  header: { display: 'flex', justifyContent: 'space-between', padding: '1rem 2rem', background: '#12121a', borderBottom: '1px solid #2a2a3a' },
  logo: { color: '#ff6b35', fontSize: '1.5rem', fontWeight: 'bold', textDecoration: 'none' },
  nav: { display: 'flex', gap: '1.5rem' },
  main: { maxWidth: '800px', margin: '0 auto', padding: '2rem' },
  title: { color: '#fff', fontSize: '2rem', marginBottom: '2rem' },
  message: { padding: '1rem', background: '#1a3a1a', color: '#6bff6b', borderRadius: '8px', marginBottom: '1rem' },
  section: { background: '#12121a', padding: '1.5rem', borderRadius: '12px', marginBottom: '1.5rem' },
  field: { marginBottom: '1rem' },
  input: { width: '100%', padding: '0.75rem', borderRadius: '8px', border: '1px solid #333', background: '#1a1a24', color: '#888' },
  select: { width: '100%', padding: '0.75rem', borderRadius: '8px', border: '1px solid #333', background: '#1a1a24', color: '#fff' },
  checkbox: { display: 'flex', gap: '0.75rem', alignItems: 'center', marginBottom: '0.75rem' },
  info: { color: '#888', marginBottom: '1rem' },
  button: { padding: '0.75rem 1.5rem', background: '#2a2a3a', color: '#fff', border: 'none', borderRadius: '8px', cursor: 'pointer' },
  connection: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '1rem', background: '#1a1a24', borderRadius: '8px', marginBottom: '0.5rem' },
  connectBtn: { padding: '0.5rem 1rem', background: '#ff6b35', color: '#fff', border: 'none', borderRadius: '6px', cursor: 'pointer' },
  saveBtn: { width: '100%', padding: '1rem', background: 'linear-gradient(135deg, #ff6b35, #ff8f5a)', color: '#fff', border: 'none', borderRadius: '8px', fontSize: '1.1rem', fontWeight: 'bold', cursor: 'pointer' },
}