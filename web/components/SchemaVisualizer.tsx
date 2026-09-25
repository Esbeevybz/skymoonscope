'use client';

import React, { useMemo } from 'react';
import ReactFlow, {
  Background,
  BackgroundVariant,
  Controls,
  Handle,
  MiniMap,
  Position,
  type Edge,
  type Node,
  type NodeProps,
} from 'reactflow';
import 'reactflow/dist/style.css';

import { buildSchemaGraph } from '../lib/schemaGraph';
import type {
  SchemaContractNodeData,
  SchemaStorageNodeData,
} from '../lib/schemaGraph';
import type { ResourceReport } from '../lib/sorobantypes';

interface SchemaVisualizerProps {
  report: ResourceReport | null | undefined;
  /** Render ledger/TTL storage keys alongside the call tree. */
  includeStorage?: boolean;
  height?: number;
}

function ContractNode({ data }: NodeProps<SchemaContractNodeData>) {
  return (
    <div
      className={`min-w-[180px] rounded-xl border px-3 py-2 shadow-lg transition-colors ${
        data.isRoot
          ? 'border-cyan-400/70 bg-cyan-500/10'
          : 'border-slate-700 bg-slate-900'
      }`}
    >
      <Handle type="target" position={Position.Left} className="!bg-slate-500" />
      <p className="font-mono text-sm font-semibold text-cyan-300">{data.functionName}</p>
      <p className="mt-0.5 font-mono text-[11px] text-slate-400" title={data.contractId}>
        {data.shortContractId}
      </p>
      {data.isRoot && (
        <span className="mt-1 inline-block rounded bg-cyan-500/20 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-cyan-300">
          entry point
        </span>
      )}
      <Handle type="source" position={Position.Right} className="!bg-slate-500" />
    </div>
  );
}

function StorageNode({ data }: NodeProps<SchemaStorageNodeData>) {
  return (
    <div
      className={`min-w-[160px] max-w-[220px] rounded-lg border px-3 py-2 shadow-lg ${
        data.written
          ? 'border-amber-500/60 bg-amber-500/10'
          : 'border-slate-700 bg-slate-900/80'
      }`}
    >
      <Handle type="target" position={Position.Left} className="!bg-slate-500" />
      <p className="truncate font-mono text-xs text-slate-200" title={data.storageKey}>
        {data.storageKey}
      </p>
      <p className="mt-0.5 text-[10px] uppercase tracking-wide text-slate-400">
        {data.written ? 'written' : 'read'} &bull; {data.source}
        {data.remainingLedgers !== null && ` • TTL ${data.remainingLedgers}`}
      </p>
    </div>
  );
}

const nodeTypes = {
  contract: ContractNode,
  storage: StorageNode,
};

/**
 * Interactive node diagram of contract-to-contract calls and the ledger keys
 * each invocation touches.
 *
 * Rendered client-side only (React Flow needs real layout measurement), so the
 * caller should mount it behind a `next/dynamic` boundary with `ssr: false`.
 */
export function SchemaVisualizer({
  report,
  includeStorage = true,
  height = 420,
}: SchemaVisualizerProps) {
  const { nodes, edges, stats } = useMemo(
    () => buildSchemaGraph(report, { includeStorage }),
    [report, includeStorage],
  );

  const isEmpty = nodes.length === 0;
  const isNoReport = !report;

  if (isEmpty) {
    return (
      <div className="rounded-2xl border border-slate-800 bg-slate-900/60">
        <div
          style={{ height }}
          className="flex flex-col items-center justify-center overflow-hidden rounded-xl border border-slate-800 bg-slate-950"
          data-testid="schema-visualizer-empty-state"
        >
          {isNoReport ? (
            <div className="flex flex-col items-center gap-4 px-6 text-center">
              {/* Centered Illustration */}
              <div className="flex h-20 w-20 items-center justify-center rounded-full bg-slate-800/50">
                <svg
                  className="h-10 w-10 text-slate-500"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                  xmlns="http://www.w3.org/2000/svg"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={1.5}
                    d="M13 10V3L4 14h7v7l9-11h-7z"
                  />
                </svg>
              </div>

              {/* CTA Message */}
              <div>
                <h3 className="text-sm font-semibold text-slate-300">No Schema Yet</h3>
                <p className="mt-2 max-w-xs text-xs text-slate-500">
                  Upload a contract and run a simulation to view its cross-contract calls and
                  ledger key interactions.
                </p>
              </div>
            </div>
          ) : (
            <div className="flex flex-col items-center gap-3 px-6 text-center">
              {/* Alternative icon for analyses without cross-contract calls */}
              <div className="flex h-16 w-16 items-center justify-center rounded-full bg-slate-800/50">
                <svg
                  className="h-8 w-8 text-slate-500"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                  xmlns="http://www.w3.org/2000/svg"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={1.5}
                    d="M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-2.5a1 1 0 00-.8.4l-4.5 5.25a1 1 0 01-1.6-.2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
                  />
                </svg>
              </div>

              <div>
                <h3 className="text-sm font-semibold text-slate-300">No Call Graph</h3>
                <p className="mt-2 max-w-xs text-xs text-slate-500">
                  This contract doesn't make cross-contract calls. Run a simulation against a
                  contract with external calls to see its schema.
                </p>
              </div>
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <section className="rounded-2xl border border-slate-800 bg-slate-900/60 p-4">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div>
          <h4 className="text-sm font-semibold text-cyan-400">Subgraph Schema</h4>
          <p className="text-xs text-slate-500">
            {stats.contractNodes} contract call{stats.contractNodes === 1 ? '' : 's'} &bull;{' '}
            {stats.storageNodes} storage {stats.storageNodes === 1 ? 'key' : 'keys'} &bull; depth{' '}
            {stats.maxDepth}
            {stats.hiddenStorageNodes > 0 && ` (+${stats.hiddenStorageNodes} hidden)`}
          </p>
        </div>
        {stats.hasCycle && (
          <span className="rounded-full border border-amber-500/40 bg-amber-500/10 px-2 py-1 text-[11px] font-medium text-amber-300">
            Re-entrant call detected
          </span>
        )}
      </div>

      <div
        style={{ height }}
        className="overflow-hidden rounded-xl border border-slate-800 bg-slate-950"
        data-testid="schema-visualizer-canvas"
      >
        <ReactFlow
          nodes={nodes as unknown as Node[]}
          edges={edges as unknown as Edge[]}
          nodeTypes={nodeTypes}
          fitView
          minZoom={0.2}
          maxZoom={1.75}
          proOptions={{ hideAttribution: true }}
          nodesDraggable
          nodesConnectable={false}
        >
          <Background variant={BackgroundVariant.Dots} gap={16} size={1} color="#1e293b" />
          <Controls showInteractive={false} className="!bg-slate-900 !border-slate-700" />
          <MiniMap
            pannable
            zoomable
            maskColor="rgba(2, 6, 23, 0.75)"
            nodeColor={(node) =>
              node.type === 'storage' ? '#f59e0b' : '#22d3ee'
            }
            className="!bg-slate-900"
          />
        </ReactFlow>
      </div>
    </section>
  );
}

export default SchemaVisualizer;
