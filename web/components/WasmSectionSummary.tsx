"use client";

import React, { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { ChevronDown, ChevronRight, ArrowDownToLine, ArrowUpFromLine, Database, Layers, FileCode2 } from "lucide-react";
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import SorobanMetadataPanel from "./SorobanMetadataPanel";
import type {
  WasmValidationReport,
  WasmImportEntry,
  WasmExportEntry,
  WasmDataSegment,
  WasmSectionSummary,
} from "../lib/wasmValidation";

function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

// ── Tab identifiers ──────────────────────────────────────────────────────────

type TabId = "imports" | "exports" | "data" | "sections" | "soroban";

interface Tab {
  id: TabId;
  label: string;
  icon: React.ElementType;
  count: number;
}

// ── Individual row renderers ─────────────────────────────────────────────────

function KindBadge({ kind }: { kind: string }) {
  const colour: Record<string, string> = {
    function: "bg-indigo-100 text-indigo-700",
    table: "bg-amber-100 text-amber-700",
    memory: "bg-cyan-100 text-cyan-700",
    global: "bg-violet-100 text-violet-700",
    active: "bg-emerald-100 text-emerald-700",
    passive: "bg-slate-100 text-slate-600",
  };
  const normalised = kind.split("(")[0]; // strip "unknown(n)" suffix
  return (
    <span
      className={cn(
        "inline-block px-1.5 py-0.5 rounded text-[10px] font-medium leading-none",
        colour[normalised] ?? "bg-slate-100 text-slate-600"
      )}
    >
      {kind}
    </span>
  );
}

function ImportRow({ entry, index }: { entry: WasmImportEntry; index: number }) {
  return (
    <tr
      className={cn(
        "text-xs border-b border-slate-100 last:border-0",
        index % 2 === 0 ? "bg-white" : "bg-slate-50/60"
      )}
    >
      <td className="py-1.5 px-3 font-mono text-slate-500 truncate max-w-[120px]">
        {entry.module}
      </td>
      <td className="py-1.5 px-3 font-mono text-slate-800 truncate max-w-[180px]">
        {entry.name}
      </td>
      <td className="py-1.5 px-3 whitespace-nowrap">
        <KindBadge kind={entry.kind} />
      </td>
    </tr>
  );
}

function ExportRow({ entry, index }: { entry: WasmExportEntry; index: number }) {
  return (
    <tr
      className={cn(
        "text-xs border-b border-slate-100 last:border-0",
        index % 2 === 0 ? "bg-white" : "bg-slate-50/60"
      )}
    >
      <td className="py-1.5 px-3 font-mono text-slate-800 truncate max-w-[260px]">
        {entry.name}
      </td>
      <td className="py-1.5 px-3 whitespace-nowrap">
        <KindBadge kind={entry.kind} />
      </td>
      <td className="py-1.5 px-3 text-slate-400 text-right">{entry.index}</td>
    </tr>
  );
}

function DataRow({ segment, index }: { segment: WasmDataSegment; index: number }) {
  const offsetLabel =
    segment.offsetValue !== null
      ? `0x${segment.offsetValue.toString(16)}`
      : segment.kind === "passive"
      ? "—"
      : "expr";

  return (
    <tr
      className={cn(
        "text-xs border-b border-slate-100 last:border-0",
        index % 2 === 0 ? "bg-white" : "bg-slate-50/60"
      )}
    >
      <td className="py-1.5 px-3 text-slate-500">{segment.index}</td>
      <td className="py-1.5 px-3 whitespace-nowrap">
        <KindBadge kind={segment.kind} />
      </td>
      <td className="py-1.5 px-3 text-slate-400 font-mono">{offsetLabel}</td>
      <td className="py-1.5 px-3 text-slate-600 text-right">
        {segment.byteLength.toLocaleString()} B
      </td>
    </tr>
  );
}

function SectionRow({
  section,
  index,
}: {
  section: WasmSectionSummary;
  index: number;
}) {
  return (
    <tr
      className={cn(
        "text-xs border-b border-slate-100 last:border-0",
        index % 2 === 0 ? "bg-white" : "bg-slate-50/60"
      )}
    >
      <td className="py-1.5 px-3 text-slate-400">{section.id}</td>
      <td className="py-1.5 px-3 font-mono text-slate-800">{section.name}</td>
      <td className="py-1.5 px-3 text-slate-600 text-right">
        {section.size.toLocaleString()} B
      </td>
    </tr>
  );
}

// ── Tab content panels ───────────────────────────────────────────────────────

function ImportsPane({ imports }: { imports: WasmImportEntry[] }) {
  if (imports.length === 0) {
    return (
      <p className="py-6 text-center text-xs text-slate-400">
        No imports — this module is self-contained.
      </p>
    );
  }
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[360px]" aria-label="Module imports">
        <thead>
          <tr className="text-[10px] uppercase tracking-wide text-slate-400 bg-slate-50 border-b border-slate-200">
            <th className="py-1.5 px-3 text-left font-medium">Module</th>
            <th className="py-1.5 px-3 text-left font-medium">Name</th>
            <th className="py-1.5 px-3 text-left font-medium">Kind</th>
          </tr>
        </thead>
        <tbody>
          {imports.map((entry, i) => (
            <ImportRow key={`${entry.module}::${entry.name}::${i}`} entry={entry} index={i} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ExportsPane({ exports }: { exports: WasmExportEntry[] }) {
  if (exports.length === 0) {
    return (
      <p className="py-6 text-center text-xs text-slate-400">
        No exports — Soroban contracts should export entry-point functions.
      </p>
    );
  }
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[360px]" aria-label="Module exports">
        <thead>
          <tr className="text-[10px] uppercase tracking-wide text-slate-400 bg-slate-50 border-b border-slate-200">
            <th className="py-1.5 px-3 text-left font-medium">Name</th>
            <th className="py-1.5 px-3 text-left font-medium">Kind</th>
            <th className="py-1.5 px-3 text-right font-medium">Index</th>
          </tr>
        </thead>
        <tbody>
          {exports.map((entry, i) => (
            <ExportRow key={`${entry.name}::${i}`} entry={entry} index={i} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function DataPane({ dataSegments }: { dataSegments: WasmDataSegment[] }) {
  if (dataSegments.length === 0) {
    return (
      <p className="py-6 text-center text-xs text-slate-400">
        No data segments in this module.
      </p>
    );
  }
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[360px]" aria-label="Data segments">
        <thead>
          <tr className="text-[10px] uppercase tracking-wide text-slate-400 bg-slate-50 border-b border-slate-200">
            <th className="py-1.5 px-3 text-left font-medium">#</th>
            <th className="py-1.5 px-3 text-left font-medium">Kind</th>
            <th className="py-1.5 px-3 text-left font-medium">Offset</th>
            <th className="py-1.5 px-3 text-right font-medium">Size</th>
          </tr>
        </thead>
        <tbody>
          {dataSegments.map((seg, i) => (
            <DataRow key={seg.index} segment={seg} index={i} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function SectionsPane({ sections }: { sections: WasmSectionSummary[] }) {
  if (sections.length === 0) {
    return (
      <p className="py-6 text-center text-xs text-slate-400">
        No sections found.
      </p>
    );
  }
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[300px]" aria-label="Module sections">
        <thead>
          <tr className="text-[10px] uppercase tracking-wide text-slate-400 bg-slate-50 border-b border-slate-200">
            <th className="py-1.5 px-3 text-left font-medium">ID</th>
            <th className="py-1.5 px-3 text-left font-medium">Section</th>
            <th className="py-1.5 px-3 text-right font-medium">Size</th>
          </tr>
        </thead>
        <tbody>
          {sections.map((section, i) => (
            <SectionRow key={`${section.id}::${i}`} section={section} index={i} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ── Main component ───────────────────────────────────────────────────────────

export interface WasmSectionSummaryProps {
  report: WasmValidationReport;
  /** Default open state. Defaults to false (collapsed). */
  defaultOpen?: boolean;
  className?: string;
}

/**
 * Collapsible panel that summarises the decoded sections of a WASM module.
 *
 * Renders four tabs: Imports | Exports | Data | Sections.
 * Accepts a `WasmValidationReport` produced by `validateWasmModule()`.
 */
export default function WasmSectionSummaryPanel({
  report,
  defaultOpen = false,
  className,
}: WasmSectionSummaryProps) {
  const [isOpen, setIsOpen] = useState(defaultOpen);
  const [activeTab, setActiveTab] = useState<TabId>("exports");

  const hasSorobanMeta = Boolean(
    report.sorobanMeta &&
      ((report.sorobanMeta.specFunctions?.length ?? 0) > 0 ||
        (report.sorobanMeta.specTypes?.length ?? 0) > 0 ||
        (report.sorobanMeta.contractMeta?.length ?? 0) > 0)
  );

  const tabs: Tab[] = [
    {
      id: "exports",
      label: "Exports",
      icon: ArrowUpFromLine,
      count: report.exports.length,
    },
    ...(hasSorobanMeta
      ? [
          {
            id: "soroban" as TabId,
            label: "Soroban Spec",
            icon: FileCode2,
            count:
              (report.sorobanMeta?.specFunctions.length ?? 0) +
              (report.sorobanMeta?.specTypes.length ?? 0),
          },
        ]
      : []),
    {
      id: "imports",
      label: "Imports",
      icon: ArrowDownToLine,
      count: report.imports.length,
    },
    {
      id: "data",
      label: "Data",
      icon: Database,
      count: report.dataSegments?.length ?? 0,
    },
    {
      id: "sections",
      label: "Sections",
      icon: Layers,
      count: report.sections.length,
    },
  ];

  return (
    <div
      className={cn(
        "rounded-xl border border-slate-200 bg-white overflow-hidden shadow-sm",
        className
      )}
    >
      {/* Header / toggle */}
      <button
        type="button"
        onClick={() => setIsOpen((prev) => !prev)}
        aria-expanded={isOpen}
        className="w-full flex items-center justify-between px-4 py-3 text-left hover:bg-slate-50 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-indigo-500"
      >
        <div className="flex items-center gap-2">
          <Layers className="w-4 h-4 text-slate-400" aria-hidden="true" />
          <span className="text-sm font-semibold text-slate-700">
            Section Summary
          </span>
          <span className="text-xs text-slate-400">
            {report.sections.length} sections &bull;{" "}
            {(report.byteLength / 1024).toFixed(1)} KB
          </span>
        </div>
        {isOpen ? (
          <ChevronDown className="w-4 h-4 text-slate-400" aria-hidden="true" />
        ) : (
          <ChevronRight className="w-4 h-4 text-slate-400" aria-hidden="true" />
        )}
      </button>

      {/* Collapsible body */}
      <AnimatePresence initial={false}>
        {isOpen && (
          <motion.div
            key="panel-body"
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.18, ease: "easeInOut" }}
            className="overflow-hidden"
          >
            {/* Tab bar */}
            <div
              className="flex border-b border-slate-200 bg-slate-50/60 px-2"
              role="tablist"
              aria-label="WASM section details"
            >
              {tabs.map((tab) => {
                const Icon = tab.icon;
                const isActive = activeTab === tab.id;
                return (
                  <button
                    key={tab.id}
                    type="button"
                    role="tab"
                    id={`wasm-tab-${tab.id}`}
                    aria-selected={isActive}
                    aria-controls={`wasm-panel-${tab.id}`}
                    onClick={() => setActiveTab(tab.id)}
                    className={cn(
                      "flex items-center gap-1.5 px-3 py-2.5 text-xs font-medium border-b-2 transition-colors whitespace-nowrap",
                      isActive
                        ? "border-indigo-500 text-indigo-600"
                        : "border-transparent text-slate-500 hover:text-slate-700 hover:border-slate-300"
                    )}
                  >
                    <Icon className="w-3.5 h-3.5" aria-hidden="true" />
                    {tab.label}
                    {tab.count > 0 && (
                      <span
                        className={cn(
                          "ml-0.5 px-1.5 py-0.5 rounded-full text-[10px] leading-none",
                          isActive
                            ? "bg-indigo-100 text-indigo-600"
                            : "bg-slate-200 text-slate-500"
                        )}
                      >
                        {tab.count}
                      </span>
                    )}
                  </button>
                );
              })}
            </div>

            {/* Tab panels */}
            <div className="max-h-72 overflow-y-auto">
              <div
                role="tabpanel"
                id={`wasm-panel-exports`}
                aria-labelledby={`wasm-tab-exports`}
                hidden={activeTab !== "exports"}
              >
                {activeTab === "exports" && (
                  <ExportsPane exports={report.exports} />
                )}
              </div>
              <div
                role="tabpanel"
                id={`wasm-panel-imports`}
                aria-labelledby={`wasm-tab-imports`}
                hidden={activeTab !== "imports"}
              >
                {activeTab === "imports" && (
                  <ImportsPane imports={report.imports} />
                )}
              </div>
              <div
                role="tabpanel"
                id={`wasm-panel-data`}
                aria-labelledby={`wasm-tab-data`}
                hidden={activeTab !== "data"}
              >
                {activeTab === "data" && (
                  <DataPane dataSegments={report.dataSegments ?? []} />
                )}
              </div>
              <div
                role="tabpanel"
                id={`wasm-panel-sections`}
                aria-labelledby={`wasm-tab-sections`}
                hidden={activeTab !== "sections"}
              >
                {activeTab === "sections" && (
                  <SectionsPane sections={report.sections} />
                )}
              </div>
              {hasSorobanMeta && report.sorobanMeta && (
                <div
                  role="tabpanel"
                  id={`wasm-panel-soroban`}
                  aria-labelledby={`wasm-tab-soroban`}
                  hidden={activeTab !== "soroban"}
                  className="p-3"
                >
                  {activeTab === "soroban" && (
                    <SorobanMetadataPanel metadata={report.sorobanMeta} />
                  )}
                </div>
              )}
            </div>

            {/* Warnings footer */}
            {report.warnings.length > 0 && (
              <div className="border-t border-amber-200 bg-amber-50 px-4 py-2">
                {report.warnings.map((w, i) => (
                  <p key={i} className="text-xs text-amber-700">
                    ⚠ {w}
                  </p>
                ))}
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
