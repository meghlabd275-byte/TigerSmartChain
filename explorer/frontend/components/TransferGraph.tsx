// Transfer Graph Component
// Production-grade D3.js visualization for token/NFT transfer flow analysis
// Supports: Token transfers, NFT transfers, address clustering, time-based animation

import React, { useEffect, useRef, useState, useCallback, useMemo } from 'react';
import * as d3 from 'd3';
import { useQuery } from 'react-query';

// =============================================================================
// TYPES
// =============================================================================

interface TransferNode {
  id: string;
  address: string;
  type: 'address' | 'contract' | 'token';
  label: string;
  value: number;
  valueUSD: number;
  txCount: number;
  inDegree: number;
  outDegree: number;
}

interface TransferLink {
  source: string;
  target: string;
  value: number;
  valueUSD: number;
  tokenAddress: string;
  transactionHash: string;
  timestamp: number;
  type: 'transfer' | 'swap' | 'mint' | 'burn';
}

interface TransferGraphData {
  nodes: TransferNode[];
  links: TransferLink[];
}

interface TransferGraphProps {
  tokenAddress?: string;
  address?: string;
  height?: number;
  timeRange?: '1h' | '24h' | '7d' | '30d';
  limit?: number;
  showAnimation?: boolean;
  clusterMode?: boolean;
}

// =============================================================================
// API FUNCTIONS
// =============================================================================

const API_BASE = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

interface TransferResponse {
  nodes: TransferNode[];
  links: TransferLink[];
}

async function fetchTransferGraph(
  tokenAddress?: string,
  address?: string,
  timeRange: string = '24h',
  limit: number = 100
): Promise<TransferResponse> {
  const params = new URLSearchParams();
  if (tokenAddress) params.set('token', tokenAddress);
  if (address) params.set('address', address);
  params.set('time_range', timeRange);
  params.set('limit', limit.toString());

  const response = await fetch(`${API_BASE}/api/v1/graphs/transfers?${params}`);
  if (!response.ok) {
    throw new Error('Failed to fetch transfer graph data');
  }
  return response.json();
}

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

// Format large numbers with K/M/B suffixes
function formatValue(value: number): string {
  if (value >= 1e9) return `${(value / 1e9).toFixed(2)}B`;
  if (value >= 1e6) return `${(value / 1e6).toFixed(2)}M`;
  if (value >= 1e3) return `${(value / 1e3).toFixed(2)}K`;
  return value.toFixed(2);
}

// Format USD value
function formatUSD(value: number): string {
  if (value >= 1e9) return `$${(value / 1e9).toFixed(2)}B`;
  if (value >= 1e6) return `$${(value / 1e6).toFixed(2)}M`;
  if (value >= 1e3) return `$${(value / 1e3).toFixed(2)}K`;
  return `$${value.toFixed(2)}`;
}

// Get node color based on type
function getNodeColor(type: TransferNode['type'], inDegree: number, outDegree: number): string {
  if (type === 'token') return '#6b7280';
  if (type === 'contract') return '#8b5cf6';
  // Color based on net flow
  const netFlow = outDegree - inDegree;
  if (netFlow > 10) return '#ef4444'; // Heavy sender - red
  if (netFlow > 5) return '#f97316'; // Sender - orange
  if (netFlow < -10) return '#10b981'; // Heavy receiver - green
  if (netFlow < -5) return '#22c55e'; // Receiver - light green
  return '#3b82f6'; // Neutral - blue
}

// Get link color based on type
function getLinkColor(type: TransferLink['type']): string {
  switch (type) {
    case 'mint': return '#10b981';
    case 'burn': return '#ef4444';
    case 'swap': return '#8b5cf6';
    default: return '#6b7280';
  }
}

// Get link width based on value
function getLinkWidth(value: number, maxValue: number): number {
  const minWidth = 1;
  const maxWidth = 8;
  return minWidth + (value / maxValue) * (maxWidth - minWidth);
}

// Cluster nodes using force simulation
function clusterNodes(
  nodes: TransferNode[],
  links: TransferLink[],
  clusterThreshold: number = 100000
): TransferNode[] {
  const nodeMap = new Map(nodes.map(n => [n.address, n]));
  const clusters: Map<string, TransferNode> = new Map();

  // Group nodes by similarity
  nodes.forEach(node => {
    const key = node.type;
    if (!clusters.has(key)) {
      clusters.set(key, { ...node, id: key, label: key.toUpperCase() });
    } else {
      const cluster = clusters.get(key)!;
      cluster.value += node.value;
      cluster.valueUSD += node.valueUSD;
      cluster.txCount += node.txCount;
    }
  });

  return Array.from(clusters.values());
}

// =============================================================================
// MAIN COMPONENT
// =============================================================================

export default function TransferGraph({
  tokenAddress,
  address,
  height = 600,
  timeRange = '24h',
  limit = 100,
  showAnimation = true,
  clusterMode = false,
}: TransferGraphProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [dimensions, setDimensions] = useState({ width: 800, height });
  const [selectedNode, setSelectedNode] = useState<TransferNode | null>(null);
  const [hoveredLink, setHoveredLink] = useState<TransferLink | null>(null);
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    content: string;
  } | null>(null);
  const [simulation, setSimulation] = useState<d3.Simulation<TransferNode, TransferLink> | null>(null);

  // Fetch data
  const { data, isLoading, error, refetch } = useQuery<TransferResponse>(
    ['transfer-graph', tokenAddress, address, timeRange, limit],
    () => fetchTransferGraph(tokenAddress, address, timeRange, limit),
    {
      refetchInterval: 30000, // Refresh every 30 seconds
      staleTime: 10000,
    }
  );

  // Handle resize
  useEffect(() => {
    const handleResize = () => {
      if (containerRef.current) {
        setDimensions({
          width: containerRef.current.clientWidth,
          height,
        });
      }
    };

    handleResize();
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, [height]);

  // Initialize D3 visualization
  useEffect(() => {
    if (!svgRef.current || !data || !data.nodes.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll('*').remove();

    const { width, height: h } = dimensions;
    const nodeData = clusterMode
      ? clusterNodes(data.nodes, data.links)
      : data.nodes;
    const linkData = data.links;

    // Calculate max values for scaling
    const maxLinkValue = Math.max(...linkData.map(l => l.valueUSD), 1);
    const maxNodeValue = Math.max(...nodeData.map(n => n.valueUSD), 1);

    // Create zoom behavior
    const zoom = d3.zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.1, 4])
      .on('zoom', (event) => {
        container.attr('transform', event.transform);
      });

    svg.call(zoom);

    // Create container
    const container = svg.append('g');

    // Define arrow markers for directed links
    svg.append('defs').selectAll('marker')
      .data(['transfer', 'mint', 'burn', 'swap'])
      .join('marker')
      .attr('id', d => `arrow-${d}`)
      .attr('viewBox', '0 -5 10 10')
      .attr('refX', 15)
      .attr('refY', 0)
      .attr('markerWidth', 6)
      .attr('markerHeight', 6)
      .attr('orient', 'auto')
      .append('path')
      .attr('d', 'M0,-5L10,0L0,5')
      .attr('fill', d => getLinkColor(d as TransferLink['type']));

    // Create force simulation
    const sim = d3.forceSimulation(nodeData)
      .force('link', d3.forceLink(linkData)
        .id((d: any) => d.address)
        .distance(100))
      .force('charge', d3.forceManyBody().strength(-300))
      .force('center', d3.forceCenter(width / 2, h / 2))
      .force('collision', d3.forceCollide().radius(30))
      .force('x', d3.forceX(width / 2).strength(0.05))
      .force('y', d3.forceY(h / 2).strength(0.05));

    setSimulation(sim);

    // Create links
    const link = container.append('g')
      .attr('class', 'links')
      .selectAll('line')
      .data(linkData)
      .join('line')
      .attr('stroke', d => getLinkColor(d.type))
      .attr('stroke-opacity', 0.6)
      .attr('stroke-width', d => getLinkWidth(d.valueUSD, maxLinkValue))
      .attr('marker-end', d => `url(#arrow-${d.type})`)
      .on('mouseenter', (event, d) => {
        setHoveredLink(d);
        setTooltip({
          x: event.pageX,
          y: event.pageY,
          content: `${formatUSD(d.valueUSD)} • ${d.transactionHash.slice(0, 10)}...`,
        });
      })
      .on('mouseleave', () => {
        setHoveredLink(null);
        setTooltip(null);
      });

    // Create nodes
    const node = container.append('g')
      .attr('class', 'nodes')
      .selectAll('g')
      .data(nodeData)
      .join('g')
      .call(d3.drag<SVGGElement, TransferNode>()
        .on('start', (event, d) => {
          if (!event.active) sim.alphaTarget(0.3).restart();
          d.fx = d.x;
          d.fy = d.y;
        })
        .on('drag', (event, d) => {
          d.fx = event.x;
          d.fy = event.y;
        })
        .on('end', (event, d) => {
          if (!event.active) sim.alphaTarget(0);
          d.fx = null;
          d.fy = null;
        }));

    // Add circles to nodes
    node.append('circle')
      .attr('r', d => 8 + Math.log2(d.txCount + 1) * 3)
      .attr('fill', d => getNodeColor(d.type, d.inDegree, d.outDegree))
      .attr('stroke', '#fff')
      .attr('stroke-width', 2)
      .attr('cursor', 'pointer')
      .on('click', (event, d) => {
        setSelectedNode(d);
      })
      .on('mouseenter', (event, d) => {
        setTooltip({
          x: event.pageX,
          y: event.pageY,
          content: `${d.label}\n${formatUSD(d.valueUSD)} • ${d.txCount} txs`,
        });
      })
      .on('mouseleave', () => {
        setTooltip(null);
      });

    // Add labels to large nodes
    node.filter(d => d.valueUSD > maxNodeValue * 0.1)
      .append('text')
      .text(d => d.label.slice(0, 6) + '...')
      .attr('x', 12)
      .attr('y', 4)
      .attr('font-size', '10px')
      .attr('fill', '#6b7280');

    // Update positions on tick
    sim.on('tick', () => {
      link
        .attr('x1', d => (d.source as any).x)
        .attr('y1', d => (d.source as any).y)
        .attr('x2', d => (d.target as any).x)
        .attr('y2', d => (d.target as any).y);

      node.attr('transform', d => `translate(${d.x},${d.y})`);
    });

    // Animation if enabled
    if (showAnimation) {
      // Pulse animation for new transfers
      node.append('circle')
        .attr('r', d => 8 + Math.log2(d.txCount + 1) * 3)
        .attr('fill', 'none')
        .attr('stroke', d => getNodeColor(d.type, d.inDegree, d.outDegree))
        .attr('stroke-width', 2)
        .attr('opacity', 0)
        .transition()
        .duration(1000)
        .delay((_, i) => i * 50)
        .attr('opacity', 0)
        .attr('r', d => 15 + Math.log2(d.txCount + 1) * 3);
    }

    // Cleanup
    return () => {
      sim.stop();
    };
  }, [data, dimensions, clusterMode, showAnimation]);

  // Loading state
  if (isLoading) {
    return (
      <div
        ref={containerRef}
        style={{
          width: '100%',
          height,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: '#f9fafb',
          borderRadius: '8px',
        }}
      >
        <div style={{ textAlign: 'center' }}>
          <div
            style={{
              width: '40px',
              height: '40px',
              border: '3px solid #e5e7eb',
              borderTopColor: '#3b82f6',
              borderRadius: '50%',
              animation: 'spin 1s linear infinite',
              margin: '0 auto 12px',
            }}
          />
          <p style={{ color: '#6b7280', fontSize: '14px' }}>
            Loading transfer graph...
          </p>
        </div>
        <style>{`
          @keyframes spin {
            to { transform: rotate(360deg); }
          }
        `}</style>
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div
        ref={containerRef}
        style={{
          width: '100%',
          height,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: '#fef2f2',
          borderRadius: '8px',
        }}
      >
        <div style={{ textAlign: 'center' }}>
          <p style={{ color: '#ef4444', fontSize: '14px', marginBottom: '8px' }}>
            Failed to load transfer graph
          </p>
          <button
            onClick={() => refetch()}
            style={{
              padding: '8px 16px',
              backgroundColor: '#3b82f6',
              color: '#fff',
              border: 'none',
              borderRadius: '6px',
              cursor: 'pointer',
            }}
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  // Empty state
  if (!data || !data.nodes.length) {
    return (
      <div
        ref={containerRef}
        style={{
          width: '100%',
          height,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: '#f9fafb',
          borderRadius: '8px',
        }}
      >
        <p style={{ color: '#6b7280', fontSize: '14px' }}>
          No transfer data available
        </p>
      </div>
    );
  }

  // Render
  return (
    <div ref={containerRef} style={{ position: 'relative', width: '100%', height }}>
      {/* Controls */}
      <div
        style={{
          position: 'absolute',
          top: 12,
          left: 12,
          display: 'flex',
          gap: '8px',
          zIndex: 10,
        }}
      >
        <select
          value={timeRange}
          onChange={(e) => {
            // Trigger refetch with new time range
            refetch();
          }}
          style={{
            padding: '6px 12px',
            fontSize: '12px',
            border: '1px solid #e5e7eb',
            borderRadius: '6px',
            backgroundColor: '#fff',
            cursor: 'pointer',
          }}
        >
          <option value="1h">Last 1 hour</option>
          <option value="24h">Last 24 hours</option>
          <option value="7d">Last 7 days</option>
          <option value="30d">Last 30 days</option>
        </select>

        <button
          onClick={() => refetch()}
          style={{
            padding: '6px 12px',
            fontSize: '12px',
            border: '1px solid #e5e7eb',
            borderRadius: '6px',
            backgroundColor: '#fff',
            cursor: 'pointer',
          }}
        >
          Refresh
        </button>
      </div>

      {/* Legend */}
      <div
        style={{
          position: 'absolute',
          bottom: 12,
          left: 12,
          display: 'flex',
          gap: '16px',
          fontSize: '11px',
          color: '#6b7280',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
          <span
            style={{
              width: 10,
              height: 10,
              borderRadius: '50%',
              backgroundColor: '#3b82f6',
            }}
          />
          <span>Neutral</span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
          <span
            style={{
              width: 10,
              height: 10,
              borderRadius: '50%',
              backgroundColor: '#ef4444',
            }}
          />
          <span>Sender</span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
          <span
            style={{
              width: 10,
              height: 10,
              borderRadius: '50%',
              backgroundColor: '#10b981',
            }}
          />
          <span>Receiver</span>
        </div>
      </div>

      {/* Node count */}
      <div
        style={{
          position: 'absolute',
          top: 12,
          right: 12,
          fontSize: '12px',
          color: '#6b7280',
        }}
      >
        {data.nodes.length} nodes • {data.links.length} transfers
      </div>

      {/* Tooltip */}
      {tooltip && (
        <div
          style={{
            position: 'fixed',
            left: tooltip.x + 10,
            top: tooltip.y + 10,
            padding: '8px 12px',
            backgroundColor: '#1f2937',
            color: '#fff',
            borderRadius: '6px',
            fontSize: '12px',
            whiteSpace: 'pre-line',
            pointerEvents: 'none',
            zIndex: 1000,
          }}
        >
          {tooltip.content}
        </div>
      )}

      {/* Graph */}
      <svg
        ref={svgRef}
        width={dimensions.width}
        height={dimensions.height}
        style={{
          backgroundColor: '#fafafa',
          borderRadius: '8px',
        }}
      />

      {/* Selected node details */}
      {selectedNode && (
        <div
          style={{
            position: 'absolute',
            right: 12,
            top: 60,
            width: 250,
            padding: 16,
            backgroundColor: '#fff',
            borderRadius: '8px',
            boxShadow: '0 4px 6px -1px rgba(0, 0, 0, 0.1)',
          }}
        >
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 12 }}>
            <h3 style={{ fontSize: '14px', fontWeight: 600, margin: 0 }}>
              Node Details
            </h3>
            <button
              onClick={() => setSelectedNode(null)}
              style={{
                background: 'none',
                border: 'none',
                cursor: 'pointer',
                fontSize: '18px',
                color: '#6b7280',
              }}
            >
              ×
            </button>
          </div>
          <div style={{ fontSize: '12px', color: '#374151' }}>
            <p style={{ marginBottom: 4 }}>
              <strong>Address:</strong> {selectedNode.address}
            </p>
            <p style={{ marginBottom: 4 }}>
              <strong>Type:</strong> {selectedNode.type}
            </p>
            <p style={{ marginBottom: 4 }}>
              <strong>Value:</strong> {formatUSD(selectedNode.valueUSD)}
            </p>
            <p style={{ marginBottom: 4 }}>
              <strong>Transactions:</strong> {selectedNode.txCount}
            </p>
            <p style={{ marginBottom: 4 }}>
              <strong>Sent:</strong> {selectedNode.outDegree} transfers
            </p>
            <p style={{ marginBottom: 4 }}>
              <strong>Received:</strong> {selectedNode.inDegree} transfers
            </p>
          </div>
          <a
            href={`/address/${selectedNode.address}`}
            style={{
              display: 'block',
              marginTop: 12,
              padding: '8px 16px',
              backgroundColor: '#3b82f6',
              color: '#fff',
              textAlign: 'center',
              borderRadius: '6px',
              textDecoration: 'none',
              fontSize: '12px',
            }}
          >
            View Address
          </a>
        </div>
      )}

      <style jsx>{`
        svg {
          overflow: visible;
        }
        line {
          transition: stroke-opacity 0.2s;
        }
        line:hover {
          stroke-opacity: 1;
        }
        circle {
          transition: transform 0.2s;
        }
        circle:hover {
          transform: scale(1.2);
        }
      `}</style>
    </div>
  );
}

// =============================================================================
// HELPER COMPONENTS
// =============================================================================

// Compact version for dashboards
export function MiniTransferGraph({
  tokenAddress,
  height = 300,
}: {
  tokenAddress: string;
  height?: number;
}) {
  return (
    <TransferGraph
      tokenAddress={tokenAddress}
      height={height}
      limit={50}
      showAnimation={false}
    />
  );
}

// Full page version
export function TransferGraphPage() {
  const [tokenAddress, setTokenAddress] = useState('');
  const [address, setAddress] = useState('');

  return (
    <div style={{ padding: '24px', maxWidth: 1400, margin: '0 auto' }}>
      <h1 style={{ fontSize: '24px', fontWeight: 700, marginBottom: 24 }}>
        Transfer Graph
      </h1>

      {/* Filters */}
      <div
        style={{
          display: 'flex',
          gap: '16px',
          marginBottom: 24,
        }}
      >
        <input
          type="text"
          placeholder="Token address (optional)"
          value={tokenAddress}
          onChange={(e) => setTokenAddress(e.target.value)}
          style={{
            flex: 1,
            padding: '10px 14px',
            border: '1px solid #e5e7eb',
            borderRadius: '8px',
            fontSize: '14px',
          }}
        />
        <input
          type="text"
          placeholder="Address (optional)"
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          style={{
            flex: 1,
            padding: '10px 14px',
            border: '1px solid #e5e7eb',
            borderRadius: '8px',
            fontSize: '14px',
          }}
        />
      </div>

      {/* Graph */}
      <TransferGraph
        tokenAddress={tokenAddress || undefined}
        address={address || undefined}
        height={700}
        showAnimation={true}
      />
    </div>
  );
}