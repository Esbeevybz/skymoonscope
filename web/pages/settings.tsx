import Head from 'next/head';
import Link from 'next/link';
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ArrowLeft,
  CheckCircle2,
  Globe,
  Loader2,
  RefreshCw,
  RotateCcw,
  Save,
  Signal,
  Wifi,
  WifiOff,
  XCircle,
  Zap,
  TrendingUp,
  TrendingDown,
  Activity,
} from 'lucide-react';
import { ResponsiveContainer, AreaChart, Area, XAxis, YAxis, Tooltip } from 'recharts';

import { useNetwork } from '../context/NetworkContext';
import type { NetworkId, NetworkConfig } from '../context/NetworkContext';
import { NETWORKS, ALL_NETWORKS } from '../context/NetworkContext';
import { API_URL } from '../lib/api';
import { probeEndpoint } from '../lib/probeEndpoint';
import {
  DEFAULT_SETTINGS,
  clearSettings,
  loadSettings,
  saveSettings,
  validateEndpointUrl,
  validateSettings,
  loadNetworkProfile,
  saveNetworkProfile,
  clearNetworkProfile,
} from '../lib/userSettings';
import type { UserSettings, NetworkEndpointProfile } from '../lib/userSettings';

// ── Local types ────────────────────────────────────────────────────────────────

type TestState = { status: 'idle' | 'testing' | 'ok' | 'fail'; message?: string; latencyMs?: number };

interface PingResult {
  timestamp: number;
  latency: number;
  success: boolean;
}

interface PingSession {
  url: string;
  results: PingResult[];
}

type ConnectionQuality = 'excellent' | 'good' | 'fair' | 'poor' | 'offline';

// ── Constants ─────────────────────────────────────────────────────────────────

const IDLE: TestState = { status: 'idle' };

// ── Helper functions ──────────────────────────────────────────────────────────

function getConnectionQuality(latencyMs: number | null): ConnectionQuality {
  if (latencyMs === null) return 'offline';
  if (latencyMs < 50) return 'excellent';
  if (latencyMs < 100) return 'good';
  if (latencyMs < 200) return 'fair';
  return 'poor';
}

function getQualityColor(quality: ConnectionQuality): string {
  switch (quality) {
    case 'excellent': return 'text-emerald-400';
    case 'good':      return 'text-green-400';
    case 'fair':      return 'text-yellow-400';
    case 'poor':      return 'text-orange-400';
    case 'offline':   return 'text-red-400';
  }
}

function getQualityLabel(quality: ConnectionQuality): string {
  switch (quality) {
    case 'excellent': return 'Excellent';
    case 'good':      return 'Good';
    case 'fair':      return 'Fair';
    case 'poor':      return 'Poor';
    case 'offline':   return 'Offline';
  }
}

function getQualityBg(quality: ConnectionQuality): string {
  switch (quality) {
    case 'excellent': return 'bg-emerald-500/20 border-emerald-500/30';
    case 'good':      return 'bg-green-500/20 border-green-500/30';
    case 'fair':      return 'bg-yellow-500/20 border-yellow-500/30';
    case 'poor':      return 'bg-orange-500/20 border-orange-500/30';
    case 'offline':   return 'bg-red-500/20 border-red-500/30';
  }
}

function calculateStats(results: PingResult[]): {
  min: number; max: number; avg: number; p95: number; loss: number;
} {
  const successful = results.filter((r) => r.success);
  const latencies = successful.map((r) => r.latency).sort((a, b) => a - b);
  const total = results.length;
  const failed = total - successful.length;
  const loss = total > 0 ? (failed / total) * 100 : 0;

  if (latencies.length === 0) return { min: 0, max: 0, avg: 0, p95: 0, loss };

  const sum = latencies.reduce((a, b) => a + b, 0);
  const avg = sum / latencies.length;
  const p95Index = Math.floor(latencies.length * 0.95);
  const p95 = latencies[p95Index] || latencies[latencies.length - 1];

  return {
    min: latencies[0],
    max: latencies[latencies.length - 1],
    avg: Math.round(avg),
    p95,
    loss: parseFloat(loss.toFixed(1)),
  };
}

// ── Network tab badge helpers ─────────────────────────────────────────────────

/** Active tab ring colour keyed by network badge colour. */
const NETWORK_RING: Record<string, string> = {
  emerald: 'ring-emerald-500/60 text-emerald-400',
  cyan:    'ring-cyan-500/60 text-cyan-400',
  purple:  'ring-purple-500/60 text-purple-400',
  amber:   'ring-amber-500/60 text-amber-400',
};

const NETWORK_ACTIVE_BG: Record<string, string> = {
  emerald: 'bg-emerald-500/10 border-emerald-500/30',
  cyan:    'bg-cyan-500/10 border-cyan-500/30',
  purple:  'bg-purple-500/10 border-purple-500/30',
  amber:   'bg-amber-500/10 border-amber-500/30',
};

// ── Main page ─────────────────────────────────────────────────────────────────

export default function SettingsPage() {
  const { network: activeGlobalNetwork, networkId: activeGlobalNetworkId, setNetworkId } = useNetwork();

  // The tab the user is looking at in this page — defaults to the globally
  // active network but can be switched independently.
  const [selectedNetworkId, setSelectedNetworkId] = useState<NetworkId>(activeGlobalNetworkId);
  const selectedNetwork: NetworkConfig = NETWORKS[selectedNetworkId] ?? NETWORKS.testnet;

  // Per-network endpoint overrides for the selected tab.
  const [profile, setProfile] = useState<NetworkEndpointProfile>({ rpcUrl: '', indexerUrl: '' });

  // Global settings (requestTimeoutMs only — shared across all networks).
  const [globalSettings, setGlobalSettings] = useState<UserSettings>(DEFAULT_SETTINGS);

  const [profileErrors, setProfileErrors] = useState<Partial<Record<keyof NetworkEndpointProfile, string>>>({});
  const [saved, setSaved] = useState(false);

  // Test / ping state.
  const [rpcTest, setRpcTest] = useState<TestState>(IDLE);
  const [indexerTest, setIndexerTest] = useState<TestState>(IDLE);
  const [pingSession, setPingSession] = useState<PingSession | null>(null);
  const [isPinging, setIsPinging] = useState(false);
  const pingCount = 10;
  const pingInterval = 500;

  // Abort refs.
  const rpcAbortRef     = useRef<AbortController | null>(null);
  const indexerAbortRef = useRef<AbortController | null>(null);
  const pingAbortRef    = useRef<AbortController | null>(null);
  const mountedRef      = useRef(true);

  // ── Effects ─────────────────────────────────────────────────────────────────

  // Load global settings (timeout etc.) and the initial per-network profile.
  useEffect(() => {
    mountedRef.current = true;
    setGlobalSettings(loadSettings());
    setProfile(loadNetworkProfile(selectedNetworkId));
    return () => {
      mountedRef.current = false;
      rpcAbortRef.current?.abort();
      indexerAbortRef.current?.abort();
      pingAbortRef.current?.abort();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // When the user switches the tab, reload that network's profile and reset
  // any in-progress tests / ping sessions.
  useEffect(() => {
    rpcAbortRef.current?.abort();
    indexerAbortRef.current?.abort();
    pingAbortRef.current?.abort();

    setProfile(loadNetworkProfile(selectedNetworkId));
    setProfileErrors({});
    setSaved(false);
    setRpcTest(IDLE);
    setIndexerTest(IDLE);
    setIsPinging(false);
    setPingSession(null);
  }, [selectedNetworkId]);

  // ── Handlers ─────────────────────────────────────────────────────────────────

  /** Update a single profile field and clear its validation error. */
  const updateProfile = useCallback((key: keyof NetworkEndpointProfile, value: string) => {
    setProfile((prev) => ({ ...prev, [key]: value }));
    setSaved(false);
    setProfileErrors((prev) => ({ ...prev, [key]: undefined }));
    if (key === 'rpcUrl') { setRpcTest(IDLE); setPingSession(null); }
    if (key === 'indexerUrl') setIndexerTest(IDLE);
  }, []);

  /** Update the shared request timeout setting. */
  const updateTimeout = useCallback((value: number) => {
    setGlobalSettings((prev) => ({ ...prev, requestTimeoutMs: value }));
    setSaved(false);
  }, []);

  /** Save the current tab's profile + the shared timeout. */
  const handleSave = useCallback(() => {
    // Validate the endpoint URLs.
    const rpc = validateEndpointUrl(profile.rpcUrl);
    const indexer = validateEndpointUrl(profile.indexerUrl);
    const errors: Partial<Record<keyof NetworkEndpointProfile, string>> = {};
    if (!rpc.valid) errors.rpcUrl = rpc.error ?? undefined;
    if (!indexer.valid) errors.indexerUrl = indexer.error ?? undefined;

    if (Object.keys(errors).length > 0) {
      setProfileErrors(errors);
      setSaved(false);
      return;
    }

    // Persist the per-network profile.
    const savedProfile = saveNetworkProfile(selectedNetworkId, profile);
    setProfile(savedProfile);

    // Also persist the shared global settings (timeout).
    saveSettings(globalSettings);

    setProfileErrors({});
    setSaved(true);
  }, [selectedNetworkId, profile, globalSettings]);

  /**
   * Reset only the selected network's profile to defaults.
   * The shared timeout and other networks' profiles are untouched.
   */
  const handleReset = useCallback(() => {
    // Confirm destructive reset
    if (!window.confirm('Reset settings for the selected network? This action cannot be undone.')) {
      return;
    }
    rpcAbortRef.current?.abort();
    indexerAbortRef.current?.abort();
    pingAbortRef.current?.abort();

    clearNetworkProfile(selectedNetworkId);
    setProfile({ rpcUrl: '', indexerUrl: '' });
    setProfileErrors({});
    setSaved(false);
    setRpcTest(IDLE);
    setIndexerTest(IDLE);
    setIsPinging(false);
    setPingSession(null);
  }, [selectedNetworkId]);

  // ── Connection tests ──────────────────────────────────────────────────────

  const testRpc = useCallback(async () => {
    const target = validateEndpointUrl(profile.rpcUrl);
    if (!target.valid || target.normalized === '') {
      setRpcTest({ status: 'fail', message: target.error ?? 'Enter an RPC URL to test' });
      return;
    }

    rpcAbortRef.current?.abort();
    const controller = new AbortController();
    rpcAbortRef.current = controller;
    setRpcTest({ status: 'testing' });

    const result = await probeEndpoint(
      target.normalized,
      { jsonrpc: '2.0', id: 1, method: 'getHealth' },
      globalSettings.requestTimeoutMs,
      controller.signal,
    );

    if (mountedRef.current && !controller.signal.aborted) setRpcTest(result);
  }, [profile.rpcUrl, globalSettings.requestTimeoutMs]);

  const startPing = useCallback(async () => {
    const target = validateEndpointUrl(profile.rpcUrl);
    if (!target.valid || target.normalized === '') {
      setRpcTest({ status: 'fail', message: target.error ?? 'Enter an RPC URL to ping' });
      return;
    }

    pingAbortRef.current?.abort();
    const controller = new AbortController();
    pingAbortRef.current = controller;

    setIsPinging(true);
    const results: PingResult[] = [];

    for (let i = 0; i < pingCount; i++) {
      if (controller.signal.aborted) break;

      const pingController = new AbortController();
      const onAbort = () => pingController.abort();
      controller.signal.addEventListener('abort', onAbort, { once: true });

      const started = Date.now();
      try {
        const pingTimer = setTimeout(() => pingController.abort(), globalSettings.requestTimeoutMs);
        try {
          const response = await fetch(target.normalized, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'getHealth' }),
            signal: pingController.signal,
          });
          const latency = Date.now() - started;
          results.push({ timestamp: Date.now(), latency, success: response.ok || response.status === 404 });
        } finally {
          clearTimeout(pingTimer);
        }
      } catch {
        results.push({ timestamp: Date.now(), latency: globalSettings.requestTimeoutMs, success: false });
      } finally {
        controller.signal.removeEventListener('abort', onAbort);
      }

      if (!controller.signal.aborted && i < pingCount - 1) {
        await new Promise<void>((resolve) => {
          const sleepTimer = setTimeout(resolve, pingInterval);
          controller.signal.addEventListener('abort', () => { clearTimeout(sleepTimer); resolve(); }, { once: true });
        });
      }
    }

    if (mountedRef.current && !controller.signal.aborted) setPingSession({ url: target.normalized, results });
    if (mountedRef.current) setIsPinging(false);
  }, [profile.rpcUrl, globalSettings.requestTimeoutMs]);

  const testIndexer = useCallback(async () => {
    const target = validateEndpointUrl(profile.indexerUrl);
    if (!target.valid || target.normalized === '') {
      setIndexerTest({ status: 'fail', message: target.error ?? 'Enter an indexer URL to test' });
      return;
    }

    indexerAbortRef.current?.abort();
    const controller = new AbortController();
    indexerAbortRef.current = controller;
    setIndexerTest({ status: 'testing' });

    const result = await probeEndpoint(
      `${target.normalized}/health`,
      null,
      globalSettings.requestTimeoutMs,
      controller.signal,
    );

    if (mountedRef.current && !controller.signal.aborted) setIndexerTest(result);
  }, [profile.indexerUrl, globalSettings.requestTimeoutMs]);

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <>
      <Head>
        <title>Settings - Sky Moon Scope</title>
        <meta name="description" content="Configure custom Soroban RPC and indexer endpoints per network." />
      </Head>

      <main className="min-h-screen bg-slate-950 text-slate-100">
        <div className="mx-auto max-w-3xl px-4 py-8 sm:px-6 lg:px-8">
          <Link
            href="/"
            className="mb-6 inline-flex items-center gap-2 text-sm text-slate-400 transition-colors hover:text-cyan-400"
          >
            <ArrowLeft className="h-4 w-4" />
            Back to explorer
          </Link>

          <h1 className="text-2xl font-bold tracking-tight text-white">Settings</h1>
          <p className="mt-1 text-sm text-slate-400">
            Point Sky Moon Scope at your own self-hosted infrastructure. Each network stores its
            endpoints independently. Preferences are stored in this browser only and never leave
            your machine.
          </p>

          {/* ── Network tab switcher ─────────────────────────────────────── */}
          <div
            role="tablist"
            aria-label="Select network"
            className="mt-6 flex flex-wrap gap-2"
          >
            {ALL_NETWORKS.map((net) => {
              const isSelected = net.id === selectedNetworkId;
              const isGlobal   = net.id === activeGlobalNetworkId;
              const ring  = NETWORK_RING[net.badgeColor]  ?? 'ring-slate-500/60 text-slate-400';
              const bg    = NETWORK_ACTIVE_BG[net.badgeColor] ?? 'bg-slate-800/50 border-slate-700';
              return (
                <button
                  key={net.id}
                  role="tab"
                  aria-selected={isSelected}
                  aria-controls={`panel-${net.id}`}
                  id={`tab-${net.id}`}
                  type="button"
                  onClick={() => setSelectedNetworkId(net.id as NetworkId)}
                  className={`inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-sm font-medium transition-colors focus:outline-none focus:ring-2 ${
                    isSelected
                      ? `${bg} ring-2 ${ring}`
                      : 'border-slate-700 bg-slate-900 text-slate-400 hover:bg-slate-800 hover:text-slate-200'
                  }`}
                >
                  <span className={`h-2 w-2 rounded-full ${net.dotColor}`} aria-hidden="true" />
                  {net.shortName}
                  {isGlobal && (
                    <span
                      className="ml-1 rounded bg-slate-700 px-1 py-0.5 text-[10px] leading-none text-slate-300"
                      title="Currently active network"
                    >
                      active
                    </span>
                  )}
                </button>
              );
            })}
          </div>

          {/* ── Per-network endpoint fields ──────────────────────────────── */}
          <div
            id={`panel-${selectedNetworkId}`}
            role="tabpanel"
            aria-labelledby={`tab-${selectedNetworkId}`}
            className="mt-6 space-y-6"
          >
            <EndpointField
              id="rpcUrl"
              label={`Custom Soroban RPC endpoint — ${selectedNetwork.shortName}`}
              placeholder={selectedNetwork.rpcUrl}
              hint={`Leave blank to use the ${selectedNetwork.shortName} default (${selectedNetwork.rpcUrl}).`}
              value={profile.rpcUrl}
              error={profileErrors.rpcUrl}
              testState={rpcTest}
              onChange={(value) => updateProfile('rpcUrl', value)}
              onTest={testRpc}
            />

            <EndpointField
              id="indexerUrl"
              label={`Custom indexer / analyzer backend — ${selectedNetwork.shortName}`}
              placeholder={API_URL}
              hint={`Leave blank to use the built-in backend (${API_URL}).`}
              value={profile.indexerUrl}
              error={profileErrors.indexerUrl}
              testState={indexerTest}
              onChange={(value) => updateProfile('indexerUrl', value)}
              onTest={testIndexer}
            />
          </div>

          {/* ── Global settings ───────────────────────────────────────────── */}
          <div className="mt-6 rounded-2xl border border-slate-800 bg-slate-900/70 p-5">
            <label htmlFor="requestTimeoutMs" className="text-sm font-medium text-slate-200">
              Request timeout
            </label>
            <p className="mt-1 text-xs text-slate-500">
              How long to wait for a custom endpoint before giving up (1000–120000&nbsp;ms).
              Shared across all networks.
            </p>
            <input
              id="requestTimeoutMs"
              type="number"
              min={1000}
              max={120000}
              step={500}
              value={globalSettings.requestTimeoutMs}
              onChange={(e) => updateTimeout(Number(e.target.value))}
              className="mt-3 w-40 rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 font-mono text-sm text-slate-100 focus:outline-none focus:ring-2 focus:ring-cyan-500/50"
            />
          </div>

          {/* ── Diagnostic Ping Tool ──────────────────────────────────────── */}
          <div className="mt-6 rounded-2xl border border-slate-800 bg-slate-900/70 p-5">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Activity className="h-4 w-4 text-cyan-400" />
                <h3 className="text-sm font-medium text-slate-200">Diagnostic Ping Tool</h3>
              </div>
              <button
                type="button"
                onClick={startPing}
                disabled={isPinging || !profile.rpcUrl}
                className="inline-flex min-h-[36px] items-center gap-2 rounded-lg border border-slate-700 bg-slate-800 px-3 text-xs font-medium text-slate-200 transition-colors hover:bg-slate-700 disabled:opacity-60"
              >
                {isPinging ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <RefreshCw className="h-3.5 w-3.5" />
                )}
                {isPinging ? 'Pinging...' : 'Run Ping Test'}
              </button>
            </div>
            <p className="mt-1 text-xs text-slate-500">
              Send {pingCount} sequential requests to measure latency, jitter, and packet loss.
            </p>

            {pingSession && (
              <div className="mt-4 space-y-4">
                {(() => {
                  const stats = calculateStats(pingSession.results);
                  const latest = pingSession.results[pingSession.results.length - 1];
                  const quality = getConnectionQuality(latest?.success ? latest.latency : null);
                  return (
                    <div className={`rounded-xl border p-4 ${getQualityBg(quality)}`}>
                      <div className="flex items-center justify-between">
                        <div className="flex items-center gap-2">
                          {quality === 'offline' || quality === 'poor' ? (
                            <WifiOff className={`h-5 w-5 ${getQualityColor(quality)}`} />
                          ) : (
                            <Wifi className={`h-5 w-5 ${getQualityColor(quality)}`} />
                          )}
                          <span className={`text-lg font-bold ${getQualityColor(quality)}`}>
                            {getQualityLabel(quality)}
                          </span>
                        </div>
                        <span className="text-xs text-slate-400">
                          Latest: {latest?.success ? `${latest.latency}ms` : 'Failed'}
                        </span>
                      </div>
                      <div className="mt-3 grid grid-cols-4 gap-3 text-center">
                        {([['Min', stats.min, 'text-emerald-400'], ['Avg', stats.avg, 'text-cyan-400'], ['P95', stats.p95, 'text-yellow-400']] as const).map(([label, val, color]) => (
                          <div key={label} className="rounded-lg bg-slate-950/50 p-2">
                            <div className="text-xs text-slate-500">{label}</div>
                            <div className={`text-sm font-semibold ${color}`}>{val}ms</div>
                          </div>
                        ))}
                        <div className="rounded-lg bg-slate-950/50 p-2">
                          <div className="text-xs text-slate-500">Loss</div>
                          <div className={`text-sm font-semibold ${stats.loss > 0 ? 'text-red-400' : 'text-emerald-400'}`}>
                            {stats.loss}%
                          </div>
                        </div>
                      </div>
                    </div>
                  );
                })()}

                <div className="h-32">
                  <ResponsiveContainer width="100%" height="100%">
                    <AreaChart data={pingSession.results.map((r, i) => ({ ...r, index: i + 1 }))}>
                      <defs>
                        <linearGradient id="latencyGradient" x1="0" y1="0" x2="0" y2="1">
                          <stop offset="5%"  stopColor="#06b6d4" stopOpacity={0.3} />
                          <stop offset="95%" stopColor="#06b6d4" stopOpacity={0} />
                        </linearGradient>
                      </defs>
                      <XAxis dataKey="index" stroke="#64748b" fontSize={10} tickLine={false} />
                      <YAxis stroke="#64748b" fontSize={10} tickLine={false} tickFormatter={(v) => `${v}ms`} />
                      <Tooltip
                        contentStyle={{ backgroundColor: '#0f172a', borderColor: '#1e293b', borderRadius: '0.5rem', fontSize: '12px' }}
                        formatter={(value: number, name: string) => [`${value}ms`, name === 'latency' ? 'Latency' : name]}
                        labelFormatter={(label) => `Ping #${label}`}
                      />
                      <Area
                        type="monotone"
                        dataKey="latency"
                        stroke="#06b6d4"
                        strokeWidth={2}
                        fill="url(#latencyGradient)"
                        dot={(props) => {
                          const { cx, cy, payload } = props as any;
                          return (
                            <circle cx={cx} cy={cy} r={3}
                              fill={payload.success ? '#06b6d4' : '#ef4444'}
                              stroke="#0f172a" strokeWidth={1}
                            />
                          );
                        }}
                      />
                    </AreaChart>
                  </ResponsiveContainer>
                </div>
              </div>
            )}
          </div>

          {/* ── Action bar ───────────────────────────────────────────────── */}
          <div className="mt-8 flex flex-wrap items-center gap-3">
            <button
              type="button"
              onClick={handleSave}
              className="inline-flex min-h-[44px] items-center gap-2 rounded-lg bg-cyan-500 px-4 py-2.5 text-sm font-semibold text-slate-950 transition-colors hover:bg-cyan-400 focus:outline-none focus:ring-2 focus:ring-cyan-500/50"
            >
              <Save className="h-4 w-4" />
              Save {selectedNetwork.shortName} settings
            </button>
            <button
              type="button"
              onClick={handleReset}
              className="inline-flex min-h-[44px] items-center gap-2 rounded-lg border border-slate-700 bg-slate-900 px-4 py-2.5 text-sm font-medium text-slate-300 transition-colors hover:bg-slate-800 hover:text-white focus:outline-none focus:ring-2 focus:ring-cyan-500/50"
            >
              <RotateCcw className="h-4 w-4" />
              Reset {selectedNetwork.shortName} to defaults
            </button>
            {saved && (
              <span role="status" className="inline-flex items-center gap-1.5 text-sm text-emerald-400">
                <CheckCircle2 className="h-4 w-4" />
                {selectedNetwork.shortName} settings saved
              </span>
            )}
          </div>
        </div>
      </main>
    </>
  );
}

// ── EndpointField component ───────────────────────────────────────────────────

interface EndpointFieldProps {
  id: string;
  label: string;
  placeholder: string;
  hint: string;
  value: string;
  error?: string;
  testState: TestState;
  onChange: (value: string) => void;
  onTest: () => void;
}

function EndpointField({
  id,
  label,
  placeholder,
  hint,
  value,
  error,
  testState,
  onChange,
  onTest,
}: EndpointFieldProps) {
  return (
    <div className="rounded-2xl border border-slate-800 bg-slate-900/70 p-5">
      <label htmlFor={id} className="text-sm font-medium text-slate-200">
        {label}
      </label>
      <p className="mt-1 text-xs text-slate-500">{hint}</p>

      <div className="mt-3 flex flex-col gap-2 sm:flex-row">
        <input
          id={id}
          type="url"
          inputMode="url"
          spellCheck={false}
          value={value}
          placeholder={placeholder}
          aria-invalid={Boolean(error)}
          aria-describedby={error ? `${id}-error` : undefined}
          onChange={(e) => onChange(e.target.value)}
          className={`w-full rounded-lg border bg-slate-950 px-3 py-2 font-mono text-sm text-slate-100 focus:outline-none focus:ring-2 ${
            error
              ? 'border-red-500/60 focus:ring-red-500/40'
              : 'border-slate-700 focus:ring-cyan-500/50'
          }`}
        />
        <button
          type="button"
          onClick={onTest}
          disabled={testState.status === 'testing'}
          className="inline-flex min-h-[44px] shrink-0 items-center justify-center gap-2 rounded-lg border border-slate-700 bg-slate-800 px-4 text-sm font-medium text-slate-200 transition-colors hover:bg-slate-700 disabled:opacity-60"
        >
          {testState.status === 'testing' && <Loader2 className="h-4 w-4 animate-spin" />}
          Test
        </button>
      </div>

      {error && (
        <p id={`${id}-error`} role="alert" className="mt-2 text-xs text-red-400">
          {error}
        </p>
      )}

      {testState.status === 'ok' && (
        <p className="mt-2 inline-flex items-center gap-1.5 text-xs text-emerald-400">
          <CheckCircle2 className="h-3.5 w-3.5" />
          {testState.message}
          {testState.latencyMs !== undefined && ` in ${testState.latencyMs}ms`}
        </p>
      )}
      {testState.status === 'fail' && (
        <p className="mt-2 inline-flex items-center gap-1.5 text-xs text-red-400">
          <XCircle className="h-3.5 w-3.5" />
          {testState.message}
        </p>
      )}
    </div>
  );
}
