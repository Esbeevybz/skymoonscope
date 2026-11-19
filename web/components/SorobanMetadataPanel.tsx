"use client";

import React, { useState, useMemo } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  Code2,
  FileCode2,
  Terminal,
  Layers,
  Search,
  Copy,
  Check,
  ChevronDown,
  ChevronRight,
  Info,
  AlertTriangle,
  Cpu,
  Boxes,
} from "lucide-react";
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import type {
  SorobanMetaDecodeResult,
  SorobanSpecFunction,
  SorobanSpecType,
  SorobanMetaKV,
} from "../lib/sorobanMeta";

function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

// ── Badges & Micro-components ────────────────────────────────────────────────

function TypeBadge({ type }: { type: string }) {
  const getBadgeStyle = (t: string) => {
    if (t === "Address") return "bg-emerald-50 text-emerald-700 border-emerald-200";
    if (t.startsWith("Vec") || t.startsWith("Map") || t.startsWith("Tuple")) {
      return "bg-purple-50 text-purple-700 border-purple-200";
    }
    if (t.startsWith("Option") || t.startsWith("Result")) {
      return "bg-amber-50 text-amber-700 border-amber-200";
    }
    if (t === "Bool" || t === "Void") return "bg-slate-100 text-slate-700 border-slate-200";
    if (t.startsWith("U") || t.startsWith("I") || t === "Timepoint" || t === "Duration") {
      return "bg-blue-50 text-blue-700 border-blue-200";
    }
    if (t === "Bytes" || t.startsWith("BytesN") || t === "String" || t === "Symbol") {
      return "bg-cyan-50 text-cyan-700 border-cyan-200";
    }
    return "bg-indigo-50 text-indigo-700 border-indigo-200";
  };

  return (
    <span
      className={cn(
        "inline-flex items-center px-1.5 py-0.5 rounded text-[11px] font-mono border",
        getBadgeStyle(type)
      )}
    >
      {type}
    </span>
  );
}

function CopyableButton({ text, label = "Copy" }: { text: string; label?: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback if clipboard API is blocked
    }
  };

  return (
    <button
      type="button"
      onClick={handleCopy}
      title={label}
      aria-label={`${label} ${text}`}
      className="p-1 rounded text-slate-400 hover:text-slate-600 hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors"
    >
      {copied ? (
        <Check className="w-3.5 h-3.5 text-emerald-600" />
      ) : (
        <Copy className="w-3.5 h-3.5" />
      )}
    </button>
  );
}

// ── Sub-views ────────────────────────────────────────────────────────────────

interface FunctionCardProps {
  fn: SorobanSpecFunction;
}

function FunctionCard({ fn }: FunctionCardProps) {
  const [expanded, setExpanded] = useState(false);

  const signature = useMemo(() => {
    const args = fn.inputs.map((arg) => `${arg.name}: ${arg.type}`).join(", ");
    const rets = fn.outputs.length > 0 ? ` -> ${fn.outputs.join(", ")}` : "";
    return `fn ${fn.name}(${args})${rets}`;
  }, [fn]);

  return (
    <div className="border border-slate-200 dark:border-slate-800 rounded-lg p-3 bg-white dark:bg-slate-900/60 shadow-xs hover:border-indigo-300 dark:hover:border-indigo-700 transition-colors">
      <div
        className="flex items-start justify-between cursor-pointer gap-2"
        onClick={() => setExpanded(!expanded)}
      >
        <div className="flex items-center gap-2 flex-wrap min-w-0">
          <Terminal className="w-4 h-4 text-indigo-500 shrink-0" />
          <span className="font-mono font-medium text-xs sm:text-sm text-slate-900 dark:text-slate-100">
            {fn.name}
          </span>
          <div className="flex items-center gap-1.5 flex-wrap">
            {fn.inputs.map((inp) => (
              <span key={inp.name} className="text-xs text-slate-500 font-mono">
                {inp.name}: <TypeBadge type={inp.type} />
              </span>
            ))}
            {fn.outputs.length > 0 && (
              <>
                <span className="text-xs text-slate-400 font-mono">→</span>
                {fn.outputs.map((out, i) => (
                  <TypeBadge key={i} type={out} />
                ))}
              </>
            )}
          </div>
        </div>

        <div className="flex items-center gap-1 shrink-0">
          <CopyableButton text={signature} label="Copy function signature" />
          {expanded ? (
            <ChevronDown className="w-4 h-4 text-slate-400" />
          ) : (
            <ChevronRight className="w-4 h-4 text-slate-400" />
          )}
        </div>
      </div>

      {fn.doc && (
        <p className="mt-1 text-xs text-slate-600 dark:text-slate-400 line-clamp-2">
          {fn.doc}
        </p>
      )}

      <AnimatePresence>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            className="mt-3 pt-3 border-t border-slate-100 dark:border-slate-800 space-y-2 text-xs"
          >
            <div>
              <span className="text-[10px] font-semibold uppercase tracking-wider text-slate-400">
                Signature
              </span>
              <pre className="mt-0.5 p-2 bg-slate-50 dark:bg-slate-950 rounded font-mono text-[11px] text-slate-800 dark:text-slate-200 overflow-x-auto">
                {signature}
              </pre>
            </div>

            {fn.inputs.length > 0 && (
              <div>
                <span className="text-[10px] font-semibold uppercase tracking-wider text-slate-400">
                  Parameters ({fn.inputs.length})
                </span>
                <div className="mt-1 space-y-1">
                  {fn.inputs.map((inp) => (
                    <div
                      key={inp.name}
                      className="flex items-start justify-between p-1.5 rounded bg-slate-50/50 dark:bg-slate-800/40 text-xs"
                    >
                      <div>
                        <span className="font-mono font-medium text-slate-700 dark:text-slate-300">
                          {inp.name}
                        </span>
                        {inp.doc && (
                          <span className="ml-2 text-slate-500 text-[11px]">
                            — {inp.doc}
                          </span>
                        )}
                      </div>
                      <TypeBadge type={inp.type} />
                    </div>
                  ))}
                </div>
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function TypeCard({ type }: { type: SorobanSpecType }) {
  const getKindBadge = (kind: string) => {
    switch (kind) {
      case "struct":
        return "bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300";
      case "union":
        return "bg-purple-100 text-purple-700 dark:bg-purple-900/40 dark:text-purple-300";
      case "enum":
        return "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300";
      case "error_enum":
        return "bg-rose-100 text-rose-700 dark:bg-rose-900/40 dark:text-rose-300";
      default:
        return "bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300";
    }
  };

  return (
    <div className="border border-slate-200 dark:border-slate-800 rounded-lg p-3 bg-white dark:bg-slate-900/60 shadow-xs">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Boxes className="w-4 h-4 text-purple-500 shrink-0" />
          <span className="font-mono font-semibold text-xs sm:text-sm text-slate-900 dark:text-slate-100">
            {type.name}
          </span>
          <span
            className={cn(
              "px-1.5 py-0.5 rounded text-[10px] font-medium uppercase tracking-wide",
              getKindBadge(type.kind)
            )}
          >
            {type.kind}
          </span>
        </div>
        <CopyableButton text={type.name} label="Copy type name" />
      </div>

      {type.doc && (
        <p className="mt-1 text-xs text-slate-600 dark:text-slate-400">
          {type.doc}
        </p>
      )}
    </div>
  );
}

function BuildMetaRow({ entry, index }: { entry: SorobanMetaKV; index: number }) {
  const isSdkKey = entry.key === "rssdkver";
  const isRustKey = entry.key === "rsver";

  return (
    <tr
      className={cn(
        "text-xs border-b border-slate-100 dark:border-slate-800/80 last:border-0",
        index % 2 === 0 ? "bg-white dark:bg-slate-900/50" : "bg-slate-50/60 dark:bg-slate-800/30"
      )}
    >
      <td className="py-2 px-3 font-mono text-slate-600 dark:text-slate-400">
        <div className="flex items-center gap-1.5">
          {isSdkKey && <Cpu className="w-3.5 h-3.5 text-indigo-500 shrink-0" />}
          {isRustKey && <Code2 className="w-3.5 h-3.5 text-amber-500 shrink-0" />}
          <span>{entry.key}</span>
        </div>
      </td>
      <td className="py-2 px-3 font-mono font-medium text-slate-800 dark:text-slate-200">
        {entry.value}
      </td>
      <td className="py-2 px-3 text-right">
        <CopyableButton text={`${entry.key}=${entry.value}`} label="Copy key-value" />
      </td>
    </tr>
  );
}

// ── Main Panel ───────────────────────────────────────────────────────────────

export interface SorobanMetadataPanelProps {
  metadata: SorobanMetaDecodeResult;
  className?: string;
}

export default function SorobanMetadataPanel({
  metadata,
  className,
}: SorobanMetadataPanelProps) {
  const [activeTab, setActiveTab] = useState<"functions" | "types" | "build">("functions");
  const [searchQuery, setSearchQuery] = useState("");

  const filteredFunctions = useMemo(() => {
    if (!searchQuery.trim()) return metadata.specFunctions;
    const q = searchQuery.toLowerCase();
    return metadata.specFunctions.filter(
      (f) =>
        f.name.toLowerCase().includes(q) ||
        f.doc.toLowerCase().includes(q) ||
        f.inputs.some((i) => i.name.toLowerCase().includes(q) || i.type.toLowerCase().includes(q))
    );
  }, [metadata.specFunctions, searchQuery]);

  const filteredTypes = useMemo(() => {
    if (!searchQuery.trim()) return metadata.specTypes;
    const q = searchQuery.toLowerCase();
    return metadata.specTypes.filter(
      (t) =>
        t.name.toLowerCase().includes(q) ||
        t.kind.toLowerCase().includes(q) ||
        t.doc.toLowerCase().includes(q)
    );
  }, [metadata.specTypes, searchQuery]);

  const hasAnyData =
    metadata.contractMeta.length > 0 ||
    metadata.specFunctions.length > 0 ||
    metadata.specTypes.length > 0;

  if (!hasAnyData) {
    return (
      <div
        className={cn(
          "rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-6 text-center text-slate-400 text-xs",
          className
        )}
      >
        <Info className="w-5 h-5 mx-auto mb-2 text-slate-400" />
        No Soroban metadata custom sections found in this WASM binary.
      </div>
    );
  }

  return (
    <div
      className={cn(
        "rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 overflow-hidden shadow-xs",
        className
      )}
    >
      {/* Header bar with summary pill */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between p-3.5 bg-slate-50/80 dark:bg-slate-800/50 border-b border-slate-200 dark:border-slate-800 gap-2">
        <div className="flex items-center gap-2">
          <FileCode2 className="w-4 h-4 text-indigo-600 dark:text-indigo-400" />
          <h3 className="text-sm font-semibold text-slate-800 dark:text-slate-100">
            Soroban Contract Metadata
          </h3>
          <span className="text-xs text-slate-400">
            &bull; {metadata.specFunctions.length} ABI functions
          </span>
        </div>

        {/* Search filter */}
        <div className="relative w-full sm:w-56">
          <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-slate-400" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Filter functions & types..."
            className="w-full pl-8 pr-3 py-1.5 text-xs bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-md focus:outline-hidden focus:ring-1 focus:ring-indigo-500 text-slate-800 dark:text-slate-200"
          />
        </div>
      </div>

      {/* Tabs */}
      <div className="flex border-b border-slate-200 dark:border-slate-800 px-3 bg-slate-50/40 dark:bg-slate-800/30 gap-1 overflow-x-auto">
        <button
          type="button"
          onClick={() => setActiveTab("functions")}
          className={cn(
            "flex items-center gap-1.5 py-2 px-3 text-xs font-medium border-b-2 transition-colors whitespace-nowrap",
            activeTab === "functions"
              ? "border-indigo-600 text-indigo-600 dark:text-indigo-400"
              : "border-transparent text-slate-500 hover:text-slate-700 dark:hover:text-slate-300"
          )}
        >
          <Terminal className="w-3.5 h-3.5" />
          Functions ({metadata.specFunctions.length})
        </button>

        <button
          type="button"
          onClick={() => setActiveTab("types")}
          className={cn(
            "flex items-center gap-1.5 py-2 px-3 text-xs font-medium border-b-2 transition-colors whitespace-nowrap",
            activeTab === "types"
              ? "border-indigo-600 text-indigo-600 dark:text-indigo-400"
              : "border-transparent text-slate-500 hover:text-slate-700 dark:hover:text-slate-300"
          )}
        >
          <Boxes className="w-3.5 h-3.5" />
          Types ({metadata.specTypes.length})
        </button>

        <button
          type="button"
          onClick={() => setActiveTab("build")}
          className={cn(
            "flex items-center gap-1.5 py-2 px-3 text-xs font-medium border-b-2 transition-colors whitespace-nowrap",
            activeTab === "build"
              ? "border-indigo-600 text-indigo-600 dark:text-indigo-400"
              : "border-transparent text-slate-500 hover:text-slate-700 dark:hover:text-slate-300"
          )}
        >
          <Cpu className="w-3.5 h-3.5" />
          Build Meta ({metadata.contractMeta.length})
        </button>
      </div>

      {/* Body content */}
      <div className="p-3 max-h-96 overflow-y-auto">
        {activeTab === "functions" && (
          <div className="space-y-2">
            {filteredFunctions.length === 0 ? (
              <p className="py-6 text-center text-xs text-slate-400">
                {searchQuery ? "No functions match your filter." : "No functions found in ABI specification."}
              </p>
            ) : (
              filteredFunctions.map((fn) => <FunctionCard key={fn.name} fn={fn} />)
            )}
          </div>
        )}

        {activeTab === "types" && (
          <div className="space-y-2">
            {filteredTypes.length === 0 ? (
              <p className="py-6 text-center text-xs text-slate-400">
                {searchQuery ? "No types match your filter." : "No custom types defined in specification."}
              </p>
            ) : (
              filteredTypes.map((t) => <TypeCard key={t.name} type={t} />)
            )}
          </div>
        )}

        {activeTab === "build" && (
          <div>
            {metadata.contractMeta.length === 0 ? (
              <p className="py-6 text-center text-xs text-slate-400">
                No build metadata records found in contractmetav0.
              </p>
            ) : (
              <table className="w-full min-w-[320px]">
                <thead>
                  <tr className="text-[10px] uppercase tracking-wide text-slate-400 bg-slate-50 dark:bg-slate-800 border-b border-slate-200 dark:border-slate-700">
                    <th className="py-1.5 px-3 text-left font-medium">Key</th>
                    <th className="py-1.5 px-3 text-left font-medium">Value</th>
                    <th className="py-1.5 px-3 text-right font-medium">Action</th>
                  </tr>
                </thead>
                <tbody>
                  {metadata.contractMeta.map((entry, i) => (
                    <BuildMetaRow key={`${entry.key}::${i}`} entry={entry} index={i} />
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}
      </div>

      {/* Errors or Warnings footer */}
      {metadata.errors.length > 0 && (
        <div className="border-t border-rose-200 dark:border-rose-900 bg-rose-50 dark:bg-rose-950/40 px-4 py-2 flex items-center gap-2">
          <AlertTriangle className="w-4 h-4 text-rose-500 shrink-0" />
          <div className="text-xs text-rose-700 dark:text-rose-300">
            {metadata.errors.map((err, i) => (
              <p key={i}>{err}</p>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
