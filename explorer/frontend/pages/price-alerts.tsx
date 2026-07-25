/**
 * Price Alerts Page
 * Complete frontend for token price alerts management
 */

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface PriceAlert {
  id: number
  token_address: string
  token_symbol: string
  alert_type: string
  threshold: number
  enabled: boolean
  trigger_count: number
  created_at: string
}

interface NotificationChannel {
  id: number
  channel_type: string
  channel_value: string
  enabled: boolean
}

interface AlertHistory {
  id: number
  old_price: number
  new_price: number
  percentage: number
  triggered_at: string
}

export default function PriceAlertsPage() {
  const [alerts, setAlerts] = useState<PriceAlert[]>([])
  const [channels, setChannels] = useState<NotificationChannel[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'alerts' | 'channels' | 'history'>('alerts')
  
  // Form states
  const [newAlert, setNewAlert] = useState({
    token_address: '',
    token_symbol: '',
    alert_type: 'above',
    threshold: 0
  })
  const [newChannel, setNewChannel] = useState({
    channel_type: 'email',
    channel_value: ''
  })

  useEffect(() => {
    fetchAlerts()
    fetchChannels()
  }, [])

  async function fetchAlerts() {
    try {
      const res = await fetch('/api/v1/alerts')
      if (res.ok) {
        const data = await res.json()
        setAlerts(data.alerts || [])
      }
    } catch (err: any) {
      console.error('Failed to fetch alerts:', err)
    }
  }

  async function fetchChannels() {
    try {
      const res = await fetch('/api/v1/channels')
      if (res.ok) {
        const data = await res.json()
        setChannels(data.channels || [])
      }
    } catch (err: any) {
      console.error('Failed to fetch channels:', err)
    }
  }

  async function createAlert() {
    setLoading(true)
    try {
      const res = await fetch('/api/v1/alerts', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(newAlert)
      })
      if (res.ok) {
        await fetchAlerts()
        setNewAlert({ token_address: '', token_symbol: '', alert_type: 'above', threshold: 0 })
      }
    } catch (err: any) {
      setError(err.message)
    } finally {
      setLoading(false)
    }
  }

  async function deleteAlert(id: number) {
    try {
      const res = await fetch(`/api/v1/alerts/${id}`, { method: 'DELETE' })
      if (res.ok) {
        await fetchAlerts()
      }
    } catch (err: any) {
      setError(err.message)
    }
  }

  async function toggleAlert(id: number, enabled: boolean) {
    try {
      const res = await fetch(`/api/v1/alerts/${id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: !enabled })
      })
      if (res.ok) {
        await fetchAlerts()
      }
    } catch (err: any) {
      setError(err.message)
    }
  }

  async function createChannel() {
    setLoading(true)
    try {
      const res = await fetch('/api/v1/channels', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(newChannel)
      })
      if (res.ok) {
        await fetchChannels()
        setNewChannel({ channel_type: 'email', channel_value: '' })
      }
    } catch (err: any) {
      setError(err.message)
    } finally {
      setLoading(false)
    }
  }

  async function deleteChannel(id: number) {
    try {
      const res = await fetch(`/api/v1/channels/${id}`, { method: 'DELETE' })
      if (res.ok) {
        await fetchChannels()
      }
    } catch (err: any) {
      setError(err.message)
    }
  }

  function getAlertTypeLabel(type: string): string {
    const labels: Record<string, string> = {
      'above': 'Price Above',
      'below': 'Price Below',
      'change_up': 'Change Up',
      'change_down': 'Change Down',
      'volume': 'Volume Above'
    }
    return labels[type] || type
  }

  function getChannelTypeIcon(type: string): string {
    const icons: Record<string, string> = {
      'email': '📧',
      'telegram': '📱',
      'discord': '💬',
      'webhook': '🔗'
    }
    return icons[type] || '📢'
  }

  return (
    <>
      <Head>
        <title>Price Alerts | TigerScan</title>
      </Head>
      
      <div className="min-h-screen bg-gray-50">
        <header className="bg-white shadow">
          <div className="max-w-7xl mx-auto py-6 px-4 sm:px-6 lg:px-8">
            <div className="flex items-center justify-between">
              <h1 className="text-3xl font-bold text-gray-900">
                Price Alerts
              </h1>
              <Link href="/" className="text-blue-600 hover:text-blue-800">
                ← Back to Home
              </Link>
            </div>
          </div>
        </header>

        <main className="max-w-7xl mx-auto py-6 px-4 sm:px-6 lg:px-8">
          {error && (
            <div className="mb-4 p-4 bg-red-100 border border-red-400 text-red-700 rounded">
              {error}
              <button onClick={() => setError(null)} className="ml-4 text-sm underline">
                Dismiss
              </button>
            </div>
          )}

          {/* Tabs */}
          <div className="border-b border-gray-200 mb-6">
            <nav className="-mb-px flex space-x-8">
              {[
                { id: 'alerts', label: 'Alerts', count: alerts.length },
                { id: 'channels', label: 'Notification Channels', count: channels.length },
                { id: 'history', label: 'Alert History', count: 0 }
              ].map((tab) => (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id as any)}
                  className={`py-4 px-1 border-b-2 font-medium text-sm ${
                    activeTab === tab.id
                      ? 'border-blue-500 text-blue-600'
                      : 'border-transparent text-gray-500 hover:text-gray-700'
                  }`}
                >
                  {tab.label}
                  <span className="ml-2 px-2 py-0.5 text-xs rounded-full bg-gray-100">
                    {tab.count}
                  </span>
                </button>
              ))}
            </nav>
          </div>

          {/* Alerts Tab */}
          {activeTab === 'alerts' && (
            <div>
              {/* Create Alert Form */}
              <div className="bg-white p-6 rounded-lg shadow mb-6">
                <h3 className="text-lg font-medium mb-4">Create New Alert</h3>
                <div className="grid grid-cols-1 md:grid-cols-5 gap-4">
                  <input
                    type="text"
                    placeholder="Token Address"
                    value={newAlert.token_address}
                    onChange={(e) => setNewAlert({...newAlert, token_address: e.target.value})}
                    className="px-4 py-2 border border-gray-300 rounded-md"
                  />
                  <input
                    type="text"
                    placeholder="Symbol (e.g., USDT)"
                    value={newAlert.token_symbol}
                    onChange={(e) => setNewAlert({...newAlert, token_symbol: e.target.value})}
                    className="px-4 py-2 border border-gray-300 rounded-md"
                  />
                  <select
                    value={newAlert.alert_type}
                    onChange={(e) => setNewAlert({...newAlert, alert_type: e.target.value})}
                    className="px-4 py-2 border border-gray-300 rounded-md"
                  >
                    <option value="above">Price Above</option>
                    <option value="below">Price Below</option>
                    <option value="change_up">Change Up %</option>
                    <option value="change_down">Change Down %</option>
                    <option value="volume">Volume Above</option>
                  </select>
                  <input
                    type="number"
                    placeholder="Threshold"
                    value={newAlert.threshold || ''}
                    onChange={(e) => setNewAlert({...newAlert, threshold: parseFloat(e.target.value)})}
                    className="px-4 py-2 border border-gray-300 rounded-md"
                  />
                  <button
                    onClick={createAlert}
                    disabled={loading || !newAlert.token_address || !newAlert.token_symbol}
                    className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50"
                  >
                    Create Alert
                  </button>
                </div>
              </div>

              {/* Alerts List */}
              <div className="bg-white rounded-lg shadow overflow-hidden">
                {alerts.length > 0 ? (
                  <table className="min-w-full divide-y divide-gray-200">
                    <thead className="bg-gray-50">
                      <tr>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Token</th>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Alert Type</th>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Threshold</th>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Triggers</th>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Status</th>
                        <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase">Actions</th>
                      </tr>
                    </thead>
                    <tbody className="bg-white divide-y divide-gray-200">
                      {alerts.map((alert) => (
                        <tr key={alert.id}>
                          <td className="px-6 py-4 whitespace-nowrap">
                            <div className="font-medium text-gray-900">{alert.token_symbol}</div>
                            <div className="text-sm text-gray-500">{alert.token_address.slice(0, 10)}...</div>
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                            {getAlertTypeLabel(alert.alert_type)}
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                            {alert.alert_type.includes('change') ? `${alert.threshold}%` : `$${alert.threshold.toFixed(2)}`}
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                            {alert.trigger_count}
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap">
                            <button
                              onClick={() => toggleAlert(alert.id, alert.enabled)}
                              className={`px-2 py-1 text-xs font-medium rounded ${
                                alert.enabled ? 'bg-green-100 text-green-800' : 'bg-gray-100 text-gray-800'
                              }`}
                            >
                              {alert.enabled ? 'Active' : 'Disabled'}
                            </button>
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap text-right text-sm">
                            <button
                              onClick={() => deleteAlert(alert.id)}
                              className="text-red-600 hover:text-red-900"
                            >
                              Delete
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                ) : (
                  <div className="p-6 text-center text-gray-500">
                    No alerts configured. Create one above!
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Channels Tab */}
          {activeTab === 'channels' && (
            <div>
              {/* Create Channel Form */}
              <div className="bg-white p-6 rounded-lg shadow mb-6">
                <h3 className="text-lg font-medium mb-4">Add Notification Channel</h3>
                <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                  <select
                    value={newChannel.channel_type}
                    onChange={(e) => setNewChannel({...newChannel, channel_type: e.target.value})}
                    className="px-4 py-2 border border-gray-300 rounded-md"
                  >
                    <option value="email">Email</option>
                    <option value="telegram">Telegram</option>
                    <option value="discord">Discord</option>
                    <option value="webhook">Webhook</option>
                  </select>
                  <input
                    type="text"
                    placeholder={
                      newChannel.channel_type === 'email' ? 'email@example.com' :
                      newChannel.channel_type === 'telegram' ? 'Chat ID' :
                      newChannel.channel_type === 'discord' ? 'Webhook URL' :
                      'Webhook URL'
                    }
                    value={newChannel.channel_value}
                    onChange={(e) => setNewChannel({...newChannel, channel_value: e.target.value})}
                    className="px-4 py-2 border border-gray-300 rounded-md"
                  />
                  <button
                    onClick={createChannel}
                    disabled={loading || !newChannel.channel_value}
                    className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50"
                  >
                    Add Channel
                  </button>
                </div>
              </div>

              {/* Channels List */}
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                {channels.map((channel) => (
                  <div key={channel.id} className="bg-white p-4 rounded-lg shadow">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-2xl">{getChannelTypeIcon(channel.channel_type)}</span>
                      <span className={`px-2 py-1 text-xs font-medium rounded ${
                        channel.enabled ? 'bg-green-100 text-green-800' : 'bg-gray-100 text-gray-800'
                      }`}>
                        {channel.enabled ? 'Active' : 'Disabled'}
                      </span>
                    </div>
                    <div className="text-sm font-medium text-gray-900">
                      {channel.channel_type.charAt(0).toUpperCase() + channel.channel_type.slice(1)}
                    </div>
                    <div className="text-sm text-gray-500 truncate">
                      {channel.channel_value}
                    </div>
                    <button
                      onClick={() => deleteChannel(channel.id)}
                      className="mt-3 text-sm text-red-600 hover:text-red-900"
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* History Tab */}
          {activeTab === 'history' && (
            <div className="bg-white p-6 rounded-lg shadow">
              <p className="text-gray-500">Alert history will appear here when alerts are triggered.</p>
            </div>
          )}
        </main>
      </div>
    </>
  )
}
