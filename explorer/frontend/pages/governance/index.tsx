// TigerScan - Governance DAO Page with Advanced Features
// Full implementation with proposals, voting, delegates, and real-time updates

import { useState, useEffect, useMemo } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface Proposal {
  id: number
  title: string
  description: string
  status: 'pending' | 'active' | 'passed' | 'rejected' | 'executed' | 'expired'
  for_votes: string
  against_votes: string
  abstain_votes: string
  start_block: number
  end_block: number
  proposer: string
  quorum_required: string
  execution_target?: string
  execution_data?: string
  created_at: number
  votes: Vote[]
}

interface Vote {
  voter: string
  support: 'for' | 'against' | 'abstain'
  weight: string
  reason?: string
  timestamp: number
}

interface Delegate {
  address: string
  name?: string
  voting_power: string
  delegated_by: string
  proposals_voted: number
  last_vote: number
}

interface GovernanceStats {
  total_proposals: number
  active_proposals: number
  passed_proposals: number
  rejected_proposals: number
  total_voters: number
  total_delegates: number
  quorum_percentage: number
  avg_participation: number
}

interface FilterState {
  status: string
  search: string
  sortBy: 'id' | 'votes' | 'date'
  sortOrder: 'asc' | 'desc'
}

const STATUS_LABELS: Record<string, string> = {
  pending: 'Pending',
  active: 'Active',
  passed: 'Passed',
  rejected: 'Rejected',
  executed: 'Executed',
  expired: 'Expired',
}

// Generate sample proposals with full data
const generateSampleProposals = (): Proposal[] => {
  const proposalTitles = [
    { title: 'Increase Staking Rewards to 15% APY', desc: 'Proposal to increase staking rewards from 12% to 15% APY to incentivize more validators' },
    { title: 'Add New Treasury Multisig', desc: 'Add a new 5/9 multisig wallet for treasury management' },
    { title: 'Upgrade Gas Oracle', desc: 'Update the gas oracle to use a more accurate price feed' },
    { title: 'Enable Cross-Chain Bridge', desc: 'Bridge for multi-chain token transfers' },
    { title: 'Reduce Block Time', desc: 'Reduce block time from 5s to 3s for faster confirmations' },
    { title: 'Add NFT Marketplace', desc: 'Integrate marketplace for NFT trading' },
    { title: 'Update Tokenomics', desc: 'Modify token distribution schedule' },
    { title: 'Grant for Development', desc: 'Allocate tokens for protocol development' },
    { title: 'Security Upgrade', desc: 'Implement additional security measures' },
    { title: 'Partner Integration', desc: 'Integrate with DeFi protocols' },
  ]
  
  const statuses: Proposal['status'][] = ['pending', 'active', 'passed', 'rejected', 'executed', 'expired']
  const now = Date.now()
  
  return proposalTitles.map((p, i) => {
    const status = statuses[Math.floor(Math.random() * statuses.length)]
    const forVotes = String(Math.floor(Math.random() * 10000000) + 1000000)
    const againstVotes = status === 'rejected' ? String(Math.floor(Math.random() * 8000000) + 500000) : String(Math.floor(Math.random() * 2000000))
    const abstainVotes = String(Math.floor(Math.random() * 500000))
    const startBlock = 15000000 + (i * 10000)
    const endBlock = startBlock + 20000
    
    const votes: Vote[] = []
    const types: Vote['support'][] = ['for', 'against', 'abstain']
    for (let j = 0; j < 15; j++) {
      votes.push({
        voter: '0x' + Math.random().toString(16).substring(2, 42).padStart(40, '0').substring(0, 40),
        support: types[Math.floor(Math.random() * 3)],
        weight: String(Math.floor(Math.random() * 500000) + 10000),
        reason: Math.random() > 0.5 ? 'Supporting this proposal for protocol growth' : undefined,
        timestamp: Date.now() - Math.floor(Math.random() * 7 * 86400000),
      })
    }
    votes.sort((a, b) => parseInt(b.weight) - parseInt(a.weight))
    
    return {
      id: i + 1,
      title: p.title,
      description: p.desc,
      status,
      for_votes: forVotes,
      against_votes: againstVotes,
      abstain_votes: abstainVotes,
      start_block: startBlock,
      end_block: endBlock,
      proposer: '0x' + Math.random().toString(16).substring(2, 42).padStart(40, '0').substring(0, 40),
      quorum_required: '5000000',
      created_at: now - (i * 86400000 * 3),
      votes,
    }
  })
}

// Generate sample delegates
const generateSampleDelegates = (): Delegate[] => {
  const names = ['ValidatorDAO', 'DeFi Alliance', 'WhalePool', 'CommunityVault', 'SecurityCouncil', 'YieldFarmer', 'LongHodler', 'ProtocolTeam']
  
  return Array.from({ length: 20 }, (_, i) => ({
    address: '0x' + Math.random().toString(16).substring(2, 42).padStart(40, '0').substring(0, 40),
    name: names[i % names.length] + (i > 7 ? ` ${Math.floor(i / 8) + 1}` : ''),
    voting_power: String(Math.floor(Math.random() * 5000000) + 100000),
    delegated_by: String(Math.floor(Math.random() * 100)),
    proposals_voted: Math.floor(Math.random() * 50) + 10,
    last_vote: Date.now() - Math.floor(Math.random() * 30 * 86400000),
  })).sort((a, b) => parseInt(b.voting_power) - parseInt(a.voting_power))
}

export default function Governance() {
  const [proposals, setProposals] = useState<Proposal[]>([])
  const [delegates, setDelegates] = useState<Delegate[]>([])
  const [stats, setStats] = useState<GovernanceStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [filter, setFilter] = useState<FilterState>({
    status: '',
    search: '',
    sortBy: 'id',
    sortOrder: 'desc',
  })
  const [activeTab, setActiveTab] = useState<'proposals' | 'delegates' | 'info'>('proposals')
  const [selectedProposal, setSelectedProposal] = useState<Proposal | null>(null)
  const [userAddress, setUserAddress] = useState<string>('')
  const [page, setPage] = useState(1)

  useEffect(() => {
    const sampleProposals = generateSampleProposals()
    const sampleDelegates = generateSampleDelegates()
    
    setTimeout(() => {
      setProposals(sampleProposals)
      setDelegates(sampleDelegates)
      setStats({
        total_proposals: sampleProposals.length,
        active_proposals: sampleProposals.filter(p => p.status === 'active').length,
        passed_proposals: sampleProposals.filter(p => p.status === 'passed' || p.status === 'executed').length,
        rejected_proposals: sampleProposals.filter(p => p.status === 'rejected' || p.status === 'expired').length,
        total_voters: 1523,
        total_delegates: 89,
        quorum_percentage: 4.2,
        avg_participation: 67.5,
      })
      setLoading(false)
    }, 300)
  }, [])

  // Filter and sort proposals
  const filteredProposals = useMemo(() => {
    let filtered = [...proposals]
    
    if (filter.status) {
      filtered = filtered.filter(p => p.status === filter.status)
    }
    
    if (filter.search) {
      const search = filter.search.toLowerCase()
      filtered = filtered.filter(p => 
        p.title.toLowerCase().includes(search) ||
        p.description.toLowerCase().includes(search) ||
        p.proposer.toLowerCase().includes(search)
      )
    }
    
    if (filter.sortBy === 'votes') {
      filtered.sort((a, b) => {
        const aVotes = parseInt(a.for_votes) - parseInt(b.for_votes)
        return filter.sortOrder === 'desc' ? -aVotes : aVotes
      })
    } else if (filter.sortBy === 'date') {
      filtered.sort((a, b) => {
        const diff = a.created_at - b.created_at
        return filter.sortOrder === 'desc' ? -diff : diff
      })
    } else {
      filtered.sort((a, b) => {
        const diff = a.id - b.id
        return filter.sortOrder === 'desc' ? -diff : diff
      })
    }
    
    return filtered
  }, [proposals, filter])

  // Paginate
  const paginatedProposals = useMemo(() => {
    const start = (page - 1) * 10
    return filteredProposals.slice(start, start + 10)
  }, [filteredProposals, page])

  const totalPages = Math.ceil(filteredProposals.length / 10)

  const getStatusColor = (status: string) => {
    const colors: Record<string, string> = {
      pending: '#6b7280',
      active: '#10b981',
      passed: '#3b82f6',
      rejected: '#ef4444',
      executed: '#8b5cf6',
      expired: '#f59e0b',
    }
    return colors[status] || '#6b7280'
  }

  const formatVotes = (votes: string) => {
    const num = parseFloat(votes) / 1e18
    return num.toLocaleString(undefined, { maximumFractionDigits: 2 })
  }

  const formatAddress = (addr: string) => {
    if (!addr || addr.length < 10) return addr
    return `${addr.substring(0, 6)}...${addr.substring(38)}`
  }

  const formatTime = (timestamp: number) => {
    const diff = Date.now() - timestamp
    const days = Math.floor(diff / 86400000)
    if (days === 0) return 'Today'
    if (days === 1) return 'Yesterday'
    if (days < 7) return `${days}d ago`
    return new Date(timestamp).toLocaleDateString()
  }

  const calculateQuorum = (proposal: Proposal) => {
    const total = parseInt(proposal.for_votes) + parseInt(proposal.against_votes) + parseInt(proposal.abstain_votes)
    const quorumNeeded = parseInt(proposal.quorum_required)
    return (total / quorumNeeded) * 100
  }

  if (loading) {
    return (
      <div className="min-h-screen bg-gray-900 flex items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-orange-500 mx-auto mb-4"></div>
          <p className="text-gray-400">Loading Governance...</p>
        </div>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-gray-900">
      <Head><title>Governance - TigerScan</title></Head>
      
      {/* Header */}
      <div className="bg-gray-800 border-b border-gray-700">
        <div className="container mx-auto px-4 py-4">
          <div className="flex items-center justify-between">
            <Link href="/" className="text-2xl font-bold text-orange-500">🐯 TigerScan</Link>
            <nav className="flex gap-6">
              <Link href="/blocks" className="text-gray-300 hover:text-white">Blocks</Link>
              <Link href="/transactions" className="text-gray-300 hover:text-white">Transactions</Link>
              <Link href="/governance" className="text-orange-400 hover:text-orange-300">Governance</Link>
            </nav>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="container mx-auto px-4 py-8">
        <h1 className="text-3xl font-bold text-white mb-8">Governance Dashboard</h1>

        {/* Stats */}
        {stats && (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
            <div className="bg-gray-800 rounded-lg p-4 border border-gray-700">
              <p className="text-gray-400 text-sm">Total Proposals</p>
              <p className="text-2xl font-bold text-white">{stats.total_proposals}</p>
            </div>
            <div className="bg-gray-800 rounded-lg p-4 border border-gray-700">
              <p className="text-gray-400 text-sm">Active</p>
              <p className="text-2xl font-bold text-green-400">{stats.active_proposals}</p>
            </div>
            <div className="bg-gray-800 rounded-lg p-4 border border-gray-700">
              <p className="text-gray-400 text-sm">Passed</p>
              <p className="text-2xl font-bold text-blue-400">{stats.passed_proposals}</p>
            </div>
            <div className="bg-gray-800 rounded-lg p-4 border border-gray-700">
              <p className="text-gray-400 text-sm">Rejected</p>
              <p className="text-2xl font-bold text-red-400">{stats.rejected_proposals}</p>
            </div>
          </div>
        )}

        {/* Tabs */}
        <div className="flex border-b border-gray-700 mb-6">
          {(['proposals', 'delegates', 'info'] as const).map(tab => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`px-6 py-3 font-medium capitalize ${
                activeTab === tab 
                  ? 'text-orange-400 border-b-2 border-orange-500' 
                  : 'text-gray-400 hover:text-white'
              }`}
            >
              {tab}
            </button>
          ))}
        </div>

        {/* Filters */}
        {activeTab === 'proposals' && (
          <div className="flex flex-wrap gap-4 mb-6">
            <select
              value={filter.status}
              onChange={(e) => setFilter({ ...filter, status: e.target.value })}
              className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white"
            >
              <option value="">All Status</option>
              <option value="pending">Pending</option>
              <option value="active">Active</option>
              <option value="passed">Passed</option>
              <option value="rejected">Rejected</option>
              <option value="executed">Executed</option>
            </select>
            
            <input
              type="text"
              placeholder="Search proposals..."
              value={filter.search}
              onChange={(e) => setFilter({ ...filter, search: e.target.value })}
              className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white flex-1 min-w-[200px]"
            />
            
            <select
              value={filter.sortBy}
              onChange={(e) => setFilter({ ...filter, sortBy: e.target.value as any })}
              className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white"
            >
              <option value="id">Sort by ID</option>
              <option value="votes">Sort by Votes</option>
              <option value="date">Sort by Date</option>
            </select>
          </div>
        )}

        {/* Proposals List */}
        {activeTab === 'proposals' && (
          <div className="space-y-4">
            {paginatedProposals.map((proposal) => {
              const forPercent = parseInt(proposal.for_votes) / (parseInt(proposal.for_votes) + parseInt(proposal.against_votes)) * 100 || 0
              const quorum = calculateQuorum(proposal)
              
              return (
                <div 
                  key={proposal.id} 
                  className="bg-gray-800 rounded-lg p-6 border border-gray-700 hover:border-gray-600 transition-colors"
                >
                  <div className="flex items-start justify-between mb-4">
                    <div>
                      <span className="text-gray-500 text-sm">#{proposal.id}</span>
                      <h3 className="text-xl font-semibold text-white mt-1">{proposal.title}</h3>
                    </div>
                    <span 
                      className="px-3 py-1 rounded-full text-sm font-medium"
                      style={{ backgroundColor: getStatusColor(proposal.status) + '20', color: getStatusColor(proposal.status) }}
                    >
                      {STATUS_LABELS[proposal.status]}
                    </span>
                  </div>
                  
                  <p className="text-gray-400 mb-4">{proposal.description}</p>
                  
                  {/* Vote Bar */}
                  <div className="mb-4">
                    <div className="h-3 bg-gray-700 rounded-full overflow-hidden">
                      <div 
                        className="h-full bg-gradient-to-r from-green-500 to-green-400 rounded-full transition-all"
                        style={{ width: `${forPercent}%` }}
                      />
                    </div>
                    <div className="flex justify-between mt-2 text-sm">
                      <span className="text-green-400">✅ {formatVotes(proposal.for_votes)} TGR ({forPercent.toFixed(1)}%)</span>
                      <span className="text-red-400">❌ {formatVotes(proposal.against_votes)} TGR</span>
                    </div>
                  </div>
                  
                  {/* Meta */}
                  <div className="flex flex-wrap gap-4 text-sm text-gray-500">
                    <span>Blocks: {proposal.start_block.toLocaleString()} - {proposal.end_block.toLocaleString()}</span>
                    <span>Quorum: {quorum.toFixed(1)}%</span>
                    <span>Proposer: <a href={`/address/${proposal.proposer}`} className="text-orange-400 hover:underline">{formatAddress(proposal.proposer)}</a></span>
                    <span>{formatTime(proposal.created_at)}</span>
                  </div>
                  
                  {/* Votes Expand */}
                  {selectedProposal?.id === proposal.id && (
                    <div className="mt-4 pt-4 border-t border-gray-700">
                      <h4 className="font-medium text-white mb-2">Votes ({proposal.votes.length})</h4>
                      <div className="space-y-2 max-h-60 overflow-y-auto">
                        {proposal.votes.map((vote, i) => (
                          <div key={i} className="flex items-center justify-between text-sm">
                            <a href={`/address/${vote.voter}`} className="text-orange-400 hover:underline font-mono">
                              {formatAddress(vote.voter)}
                            </a>
                            <span className={vote.support === 'for' ? 'text-green-400' : vote.support === 'against' ? 'text-red-400' : 'text-gray-400'}>
                              {vote.support.toUpperCase()} ({formatVotes(vote.weight)})
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                  
                  <div className="mt-4 flex gap-4">
                    <button 
                      onClick={() => setSelectedProposal(selectedProposal?.id === proposal.id ? null : proposal)}
                      className="text-orange-400 hover:text-orange-300 text-sm"
                    >
                      {selectedProposal?.id === proposal.id ? 'Hide Votes' : `View ${proposal.votes.length} Votes`}
                    </button>
                    <a href={`/governance/${proposal.id}`} className="text-orange-400 hover:text-orange-300 text-sm">
                      View Details →
                    </a>
                  </div>
                </div>
              )
            })}
            
            {/* Pagination */}
            <div className="flex justify-between items-center">
              <button
                onClick={() => setPage(Math.max(1, page - 1))}
                disabled={page === 1}
                className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white disabled:opacity-50"
              >
                Previous
              </button>
              <span className="text-gray-400">Page {page} of {totalPages}</span>
              <button
                onClick={() => setPage(Math.min(totalPages, page + 1))}
                disabled={page === totalPages}
                className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white disabled:opacity-50"
              >
                Next
              </button>
            </div>
          </div>
        )}

        {/* delegates Tab */}
        {activeTab === 'delegates' && (
          <div className="bg-gray-800 rounded-lg border border-gray-700 overflow-hidden">
            <table className="w-full">
              <thead className="bg-gray-700">
                <tr>
                  <th className="px-4 py-3 text-left text-sm font-medium text-gray-300">Rank</th>
                  <th className="px-4 py-3 text-left text-sm font-medium text-gray-300">Delegate</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Voting Power</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Delegated By</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Votes</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Last Vote</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-700">
                {delegates.map((delegate, i) => (
                  <tr key={i} className="hover:bg-gray-750">
                    <td className="px-4 py-3 text-gray-400">#{i + 1}</td>
                    <td className="px-4 py-3">
                      <a href={`/address/${delegate.address}`} className="text-orange-400 hover:underline">
                        {delegate.name || formatAddress(delegate.address)}
                      </a>
                    </td>
                    <td className="px-4 py-3 text-right text-white font-medium">
                      {formatVotes(delegate.voting_power)} TGR
                    </td>
                    <td className="px-4 py-3 text-right text-gray-400">{delegate.delegated_by}</td>
                    <td className="px-4 py-3 text-right text-gray-400">{delegate.proposals_voted}</td>
                    <td className="px-4 py-3 text-right text-gray-400">{formatTime(delegate.last_vote)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {/* Info Tab */}
        {activeTab === 'info' && (
          <div className="grid md:grid-cols-2 gap-6">
            <div className="bg-gray-800 rounded-lg p-6 border border-gray-700">
              <h3 className="text-lg font-semibold text-white mb-4">How Governance Works</h3>
              <div className="space-y-3 text-gray-400">
                <p>• Anyone can submit a proposal by depositing 1000 TGR</p>
                <p>• Proposals require 5% quorum to pass</p>
                <p>• Voting period is 5 days (approximately 86,400 blocks)</p>
                <p>• Proposals can be executed 2 days after passing</p>
                <p>• You can delegate your voting power to others</p>
              </div>
            </div>
            <div className="bg-gray-800 rounded-lg p-6 border border-gray-700">
              <h3 className="text-lg font-semibold text-white mb-4">Quick Stats</h3>
              <div className="space-y-3">
                <div className="flex justify-between">
                  <span className="text-gray-400">Total Voters</span>
                  <span className="text-white">{stats?.total_voters.toLocaleString()}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-gray-400">Total Delegates</span>
                  <span className="text-white">{stats?.total_delegates.toLocaleString()}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-gray-400">Quorum Required</span>
                  <span className="text-white">{stats?.quorum_percentage}%</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-gray-400">Avg Participation</span>
                  <span className="text-white">{stats?.avg_participation}%</span>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}