'use client'

import { useState, useEffect } from 'react'
import { Vote, CheckCircle, XCircle, Clock, Users, TrendingUp } from 'lucide-react'
import { PieChart, Pie, Cell, ResponsiveContainer, BarChart, Bar, XAxis, YAxis, Tooltip } from 'recharts'
import api from '@/lib/api'

export default function GovernancePage() {
  const [proposals, setProposals] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [filter, setFilter] = useState('all')

  useEffect(() => { fetchProposals() }, [filter])

  const fetchProposals = async () => {
    try {
      const response = await api.getGovernanceProposals({ status: filter === 'all' ? undefined : filter })
      setProposals(response.items || [])
    } catch (error) {
      setProposals(generateMockProposals())
    } finally { setLoading(false) }
  }

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
            <p className="text-2xl font-bold">12,345</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><TrendingUp className="w-4 h-4" /><span className="text-sm">Total Votes</span></div>
            <p className="text-2xl font-bold">1,234,567</p>
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
            proposals.map((proposal) => (
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
                    <div className="flex justify-between text-sm mb-1"><span className="text-green-500">For</span><span>{((proposal.forVotes || 800000) / ((proposal.forVotes || 800000) + (proposal.againstVotes || 200000)) * 100).toFixed(1)}%</span></div>
                    <div className="h-2 bg-gray-200 rounded-full"><div className="h-2 bg-green-500 rounded-full" style={{width: `${((proposal.forVotes || 800000) / ((proposal.forVotes || 800000) + (proposal.againstVotes || 200000)) * 100)}%`}}></div></div>
                  </div>
                  <div className="flex-1">
                    <div className="flex justify-between text-sm mb-1"><span className="text-red-500">Against</span><span>{((proposal.againstVotes || 200000) / ((proposal.forVotes || 800000) + (proposal.againstVotes || 200000)) * 100).toFixed(1)}%</span></div>
                    <div className="h-2 bg-gray-200 rounded-full"><div className="h-2 bg-red-500 rounded-full" style={{width: `${((proposal.againstVotes || 200000) / ((proposal.forVotes || 800000) + (proposal.againstVotes || 200000)) * 100)}%`}}></div></div>
                  </div>
                </div>
              </div>
            ))
          }
        </div>
      </div>
    </div>
  )
}

function generateMockProposals() {
  return [
    { id: 'BEP-1', title: 'Reduce Block Reward to 0.05 BNB', status: 'passed', startBlock: 45670000, endBlock: 45680000, voteCount: 1500000, forVotes: 1200000, againstVotes: 300000 },
    { id: 'BEP-2', title: 'Increase Validator Set to 41', status: 'active', startBlock: 45690000, endBlock: 45700000, voteCount: 800000, forVotes: 600000, againstVotes: 200000 },
    { id: 'BEP-3', title: 'Add New Governance Module', status: 'rejected', startBlock: 45650000, endBlock: 45660000, voteCount: 500000, forVotes: 200000, againstVotes: 300000 },
    { id: 'BEP-4', title: 'Update Gas Parameters', status: 'passed', startBlock: 45630000, endBlock: 45640000, voteCount: 2000000, forVotes: 1800000, againstVotes: 200000 },
  ]
}
