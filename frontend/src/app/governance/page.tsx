'use client'

import { useState, useEffect } from 'react'
import { Vote, CheckCircle, XCircle, Clock, Users, TrendingUp } from 'lucide-react'
import { PieChart, Pie, Cell, ResponsiveContainer, BarChart, Bar, XAxis, YAxis, Tooltip } from 'recharts'
import api from '@/lib/api'

export default function GovernancePage() {
  const [proposals, setProposals] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [filter, setFilter] = useState('all')

  useEffect(() => { fetchProposals() }, [filter])

  const fetchProposals = async () => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.getGovernanceProposals({ status: filter === 'all' ? undefined : filter })
      setProposals(response.items || [])
    } catch (error) {
      setProposals([])
      setError('Failed to load data. Please try again later.')
    } finally { setLoading(false) }
  }

  const totalVoters = proposals.reduce((sum, p) => sum + (p.voteCount || 0), 0)
  const totalVotes = proposals.reduce(
    (sum, p) => sum + (Number(p.forVotes) || 0) + (Number(p.againstVotes) || 0),
    0
  )

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'active': return 'text-green-500 bg-green-100'
      case 'passed': return 'text-blue-500 bg-blue-100'
      case 'rejected': return 'text-red-500 bg-red-100'
      default: return 'text-gray-500 bg-gray-100'
    }
  }

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 py-8">
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-8">Governance</h1>

        {/* Stats */}
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Vote className="w-4 h-4" /><span className="text-sm">Total Proposals</span></div>
            <p className="text-2xl font-bold">{proposals.length}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><CheckCircle className="w-4 h-4" /><span className="text-sm">Active</span></div>
            <p className="text-2xl font-bold text-green-500">{proposals.filter(p => p.status === 'active').length}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Users className="w-4 h-4" /><span className="text-sm">Total Voters</span></div>
            <p className="text-2xl font-bold">{totalVoters.toLocaleString()}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><TrendingUp className="w-4 h-4" /><span className="text-sm">Total Votes</span></div>
            <p className="text-2xl font-bold">{totalVotes.toLocaleString()}</p>
          </div>
        </div>

        {/* Filter */}
        <div className="flex space-x-2 mb-6">
          {['all', 'active', 'passed', 'rejected'].map(f => (
            <button key={f} onClick={() => setFilter(f)} className={`px-4 py-2 rounded-lg capitalize ${filter === f ? 'bg-primary-500 text-white' : 'bg-white dark:bg-dark-800 border'}`}>
              {f}
            </button>
          ))}
        </div>

        {/* Proposals */}
        <div className="space-y-4">
          {loading ? <div className="animate-pulse space-y-4">{Array.from({length: 5}).map((_,i) => <div key={i} className="h-32 bg-gray-200 rounded-xl"></div>)}</div> : 
            error ? (
              <div className="bg-white dark:bg-dark-800 rounded-xl border p-6 text-center text-red-500">
                {error}
                <button onClick={fetchProposals} className="block mx-auto mt-3 px-4 py-2 rounded-lg bg-primary-500 text-white hover:bg-primary-600">Retry</button>
              </div>
            ) :
            proposals.length === 0 ? (
              <div className="bg-white dark:bg-dark-800 rounded-xl border p-6 text-center text-gray-500">No proposals found</div>
            ) :
            proposals.map((proposal) => {
              const forVotes = Number(proposal.forVotes) || 0
              const againstVotes = Number(proposal.againstVotes) || 0
              const total = forVotes + againstVotes
              const forPct = total > 0 ? (forVotes / total) * 100 : 0
              const againstPct = total > 0 ? (againstVotes / total) * 100 : 0
              return (
              <div key={proposal.id} className="bg-white dark:bg-dark-800 rounded-xl border p-6">
                <div className="flex items-start justify-between">
                  <div>
                    <div className="flex items-center space-x-3 mb-2">
                      <span className={`px-3 py-1 rounded-full text-xs font-medium ${getStatusColor(proposal.status)}`}>{proposal.status}</span>
                      <span className="text-gray-500 text-sm">{proposal.id}</span>
                    </div>
                    <h3 className="text-lg font-semibold">{proposal.title}</h3>
                    <p className="text-gray-500 text-sm mt-1">Starts Block: {proposal.startBlock} | Ends Block: {proposal.endBlock}</p>
                  </div>
                  <div className="text-right">
                    <p className="text-2xl font-bold">{proposal.voteCount?.toLocaleString() || 0}</p>
                    <p className="text-gray-500 text-sm">Total Votes</p>
                  </div>
                </div>
                <div className="mt-4 flex space-x-4">
                  <div className="flex-1">
                    <div className="flex justify-between text-sm mb-1"><span className="text-green-500">For</span><span>{forPct.toFixed(1)}%</span></div>
                    <div className="h-2 bg-gray-200 rounded-full"><div className="h-2 bg-green-500 rounded-full" style={{width: `${forPct}%`}}></div></div>
                  </div>
                  <div className="flex-1">
                    <div className="flex justify-between text-sm mb-1"><span className="text-red-500">Against</span><span>{againstPct.toFixed(1)}%</span></div>
                    <div className="h-2 bg-gray-200 rounded-full"><div className="h-2 bg-red-500 rounded-full" style={{width: `${againstPct}%`}}></div></div>
                  </div>
                </div>
              </div>
              )
            })
          }
        </div>
      </div>
    </div>
  )
}
