import React, { createContext, useContext, useState, useCallback, useMemo, useEffect } from 'react';
import { useRouter } from 'next/router';
import { useNetwork } from './NetworkContext';
import { useInvocationHistory } from '../components/InnovocationHistory';
import { loadLatestAnalysis, saveLatestAnalysis, clearLatestAnalysis } from '../lib/analysisStorage';
import { analyzeService, apiUrl, ApiError } from '../lib/api';
import { createUserFriendlyMessage, extractErrorDetails, formatError } from '../lib/errorHandling';
import {
  MOCK_CONTRACT_FUNCTIONS,
  generateMockResult,
  generateMockTransactions,
  type ContractFunction,
  type InvocationResult,
  type TransactionStatus
} from '../lib/sorobantypes';
import { DEFAULT_TRANSACTION_FILTER, type TransactionFilter } from '../lib/transactionFilters';
import type { GasGolfingSuggestion } from '../lib/gasGolfingSort';
import type { NavTab } from '../components/HeaderNav';
import { SEARCH_COMMAND_EVENT } from '../components/GlobalSearchModal';
import { extractFunctionNamesFromWasm } from '../lib/wasmFunctionExtractor';

const VALID_TABS: NavTab[] = ["explorer", "schema", "history", "transactions", "analytics"];

function arrayBufferToBase64(buffer: ArrayBuffer): string {
  let binary = '';
  const bytes = new Uint8Array(buffer);
  const len = bytes.byteLength;
  for (let i = 0; i < len; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return typeof window !== 'undefined' ? window.btoa(binary) : Buffer.from(binary, 'binary').toString('base64');
}

interface SimulationContextValue {
  tab: NavTab;
  setTab: (tab: NavTab) => void;
  contractId: string;
  setContractId: (id: string) => void;
  selectedFunction: ContractFunction;
  setSelectedFunction: (func: ContractFunction) => void;
  /**
   * The live set of contract functions shown in the sidebar and search overlay.
   *
   * After a WASM upload this is replaced with the real exports parsed from the
   * binary. Before any upload (or after a reset) it falls back to the mock
   * functions so the UI always has something useful to display.
   */
  contractFunctions: ContractFunction[];
  currentResult: InvocationResult | null;
  setCurrentResult: (result: InvocationResult | null) => void;
  loading: boolean;
  wasmFile: File | null;
  setWasmFile: (file: File | null) => void;
  wasmData: string | null;
  setWasmData: (data: string | null) => void;
  toastNotification: { message: string; type: 'error' | 'success' } | null;
  setToastNotification: (toast: { message: string; type: 'error' | 'success' } | null) => void;
  uploadResetKey: number;
  mockTransactions: any[];
  transactionFilter: TransactionFilter;
  handleTransactionStatusFilterChange: (status: TransactionStatus | 'all') => void;
  handleTransactionFunctionFilterChange: (functionName: string) => void;
  gasGolfingSuggestions: GasGolfingSuggestion[];
  setGasGolfingSuggestions: (suggestions: GasGolfingSuggestion[]) => void;
  gasGolfingLoading: boolean;
  gasGolfingError: string | null;
  setGasGolfingError: (error: string | null) => void;
  history: InvocationResult[];
  addToHistory: (result: InvocationResult) => void;
  handleSimulate: (inputs: Record<string, any>, customWasmData?: string) => Promise<void>;
  handleClearAnalysis: () => void;
  handleWasmReady: (file: File) => Promise<void>;
  analysisReport: any;
}

const SimulationContext = createContext<SimulationContextValue | null>(null);

export function SimulationProvider({ children }: { children: React.ReactNode }) {
  const router = useRouter();
  const { network } = useNetwork();
  
  const [tab, setTab] = useState<NavTab>('explorer');
  const [contractId, setContractId] = useState(
    'CAEZJVJ4N7P7GRUVD5NG5LYYH23AQHJUKQEUHW54LR5PGQX3V7FXD7Q'
  );
  const [selectedFunction, setSelectedFunction] = useState<ContractFunction>(
    MOCK_CONTRACT_FUNCTIONS[0]
  );
  // Holds the functions exported by the currently-loaded contract.
  // Starts as the mock set so the UI is never empty; replaced with real
  // exports when a WASM file is successfully parsed after upload; reset back
  // to the mock set when the user clears the upload.
  const [contractFunctions, setContractFunctions] = useState<ContractFunction[]>(
    MOCK_CONTRACT_FUNCTIONS
  );
  const [currentResult, setCurrentResult] = useState<InvocationResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [wasmFile, setWasmFile] = useState<File | null>(null);
  const [wasmData, setWasmData] = useState<string | null>(null);
  const [toastNotification, setToastNotification] = useState<{ message: string; type: 'error' | 'success' } | null>(null);
  const [uploadResetKey, setUploadResetKey] = useState(0);

  const mockTransactions = useMemo(() => generateMockTransactions(47), []);

  const [transactionStatusFilter, setTransactionStatusFilter] = useState<TransactionStatus | 'all'>(
    DEFAULT_TRANSACTION_FILTER.status,
  );
  const [transactionFunctionFilter, setTransactionFunctionFilter] = useState(
    DEFAULT_TRANSACTION_FILTER.functionName,
  );

  const transactionFilter: TransactionFilter = useMemo(
    () => ({ status: transactionStatusFilter, functionName: transactionFunctionFilter }),
    [transactionStatusFilter, transactionFunctionFilter],
  );

  const handleTransactionStatusFilterChange = useCallback((status: TransactionStatus | 'all') => {
    setTransactionStatusFilter(status);
  }, []);

  const handleTransactionFunctionFilterChange = useCallback((functionName: string) => {
    setTransactionFunctionFilter(functionName);
  }, []);

  useEffect(() => {
    if (network?.defaultContractId) {
      setContractId(network.defaultContractId);
    }
  }, [network]);

  const [gasGolfingSuggestions, setGasGolfingSuggestions] = useState<GasGolfingSuggestion[]>([]);
  const [gasGolfingLoading, setGasGolfingLoading] = useState(false);
  const [gasGolfingError, setGasGolfingError] = useState<string | null>(null);

  const { history, addToHistory } = useInvocationHistory();

  useEffect(() => {
    // loadLatestAnalysis is synchronous (plain localStorage). The result
    // already has wasmFileName embedded, so the sidebar can display the
    // contract name even though the File object is not available after refresh.
    const restored = loadLatestAnalysis();
    if (restored) {
      setCurrentResult(restored);
    }
  }, []);

  useEffect(() => {
    const requested = router.query.tab;
    const value = Array.isArray(requested) ? requested[0] : requested;
    if (value && VALID_TABS.includes(value as NavTab)) {
      setTab(value as NavTab);
    }
  }, [router.query.tab]);

  useEffect(() => {
    const handleCommand = (event: Event) => {
      const detail = (event as CustomEvent).detail as
        | { action?: string; payload?: { name?: string } }
        | undefined;
      if (detail?.action !== "select-function" || !detail.payload?.name) return;

      // Search the live contract functions list (real exports after upload, or
      // mocks before any upload) rather than the static mock array.
      const name = detail.payload.name;
      const match =
        contractFunctions.find((fn) => fn.name === name) ??
        MOCK_CONTRACT_FUNCTIONS.find((fn) => fn.name === name);
      if (!match) return;

      setSelectedFunction(match);
      setCurrentResult(null);
      setTab("explorer");
    };

    window.addEventListener(SEARCH_COMMAND_EVENT, handleCommand);
    return () => window.removeEventListener(SEARCH_COMMAND_EVENT, handleCommand);
  }, [contractFunctions]);

  const handleSimulate = async (inputs: Record<string, any>, customWasmData?: string) => {
    setLoading(true);
    let errorType: string | undefined;

    try {
      const activeWasmData = customWasmData ?? wasmData;
      const report = activeWasmData
        ? await analyzeService.analyzeWasm({
            wasm_bytes: activeWasmData,
            function_name: selectedFunction.name,
            args: Object.values(inputs).map((value) => String(value)),
          })
        : await analyzeService.analyze({
            contract_id: contractId,
            function_name: selectedFunction.name,
          });

      const result: InvocationResult = {
        id: Math.random().toString(36).slice(2),
        functionName: selectedFunction.name,
        // Persist the WASM filename so the sidebar can show it after a refresh,
        // when the in-memory File object is no longer available.
        wasmFileName: wasmFile?.name,
        inputs,
        result: generateMockResult(selectedFunction.name, inputs),
        analysisReport: report,
        resourceCost: report,
        stateSnapshot: report.state_snapshot,
        callGraphMermaid: report.call_graph_mermaid ?? undefined,
        timestamp: Date.now(),
        success: true,
      };

      setCurrentResult(result);
      addToHistory(result);
      saveLatestAnalysis(result);
      if (typeof window !== 'undefined' && (window as any).triggerConfetti) {
        (window as any).triggerConfetti();
      }
    } catch (error: any) {
      if (error instanceof ApiError) {
        errorType = error.body?.error;
      }

      const formatted = formatError(error);

      const errorResult: InvocationResult = {
        id: Math.random().toString(36).substring(7),
        functionName: selectedFunction.name,
        inputs,
        error: formatted.message || 'Analysis failed',
        errorType: errorType || 'ANALYSIS_ERROR',
        timestamp: Date.now(),
        success: false,
      };
      setCurrentResult(errorResult);
      addToHistory(errorResult);
      setToastNotification({ message: formatted.message || 'Analysis failed', type: 'error' });
    } finally {
      setLoading(false);
    }
  };

  const handleClearAnalysis = useCallback(() => {
    setCurrentResult(null);
    setWasmFile(null);
    setWasmData(null);
    clearLatestAnalysis();
    setUploadResetKey((k) => k + 1);
    // Reset to mock functions so the search overlay and sidebar are not empty
    // after the user clears the current contract upload.
    setContractFunctions(MOCK_CONTRACT_FUNCTIONS);
    setSelectedFunction(MOCK_CONTRACT_FUNCTIONS[0]);
  }, []);

  const handleWasmReady = async (file: File) => {
    setGasGolfingLoading(true);
    setGasGolfingError(null);
    setGasGolfingSuggestions([]);

    try {
      const bytes = await file.arrayBuffer();
      const base64Bytes = arrayBufferToBase64(bytes);

      // ── Extract exported functions from the WASM binary ──────────────────
      // Parse the binary client-side to discover all public function exports.
      // This runs synchronously on the already-read ArrayBuffer, so it adds no
      // extra network round-trip. The result updates the search index and the
      // function sidebar immediately, before the gas-golfing response arrives.
      try {
        const exportedNames = extractFunctionNamesFromWasm(bytes);
        if (exportedNames.length > 0) {
          // Convert bare names to the minimal ContractFunction shape the UI
          // needs. We have no ABI at this point, so inputs/outputs are left
          // empty; users can still invoke functions via the search overlay.
          const discovered: ContractFunction[] = exportedNames.map((name) => ({
            name,
            inputs: [],
          }));
          setContractFunctions(discovered);
          // Pre-select the first exported function so the form is not blank.
          setSelectedFunction(discovered[0]);
        }
      } catch {
        // WASM parsing is best-effort: if it fails, keep whatever functions
        // are currently loaded rather than clearing the list.
      }

      const res = await fetch(apiUrl('/analyze/gas-golfing'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          wasm_bytes: base64Bytes,
          contract_name: file.name.replace(/\.wasm$/i, ''),
        }),
      });

      if (!res.ok) {
        const err = await extractErrorDetails(res);
        throw new Error(createUserFriendlyMessage(err));
      }

      const data = await res.json();
      setGasGolfingSuggestions(
        (data?.report?.suggestions ?? []) as GasGolfingSuggestion[]
      );
    } catch (e) {
      setGasGolfingError(e instanceof Error ? e.message : 'Failed to analyze WASM');
    } finally {
      setGasGolfingLoading(false);
    }
  };

  const analysisReport = currentResult?.analysisReport ?? currentResult?.resourceCost;

  return (
    <SimulationContext.Provider
      value={{
        tab,
        setTab,
        contractId,
        setContractId,
        selectedFunction,
        setSelectedFunction,
        contractFunctions,
        currentResult,
        setCurrentResult,
        loading,
        wasmFile,
        setWasmFile,
        wasmData,
        setWasmData,
        toastNotification,
        setToastNotification,
        uploadResetKey,
        mockTransactions,
        transactionFilter,
        handleTransactionStatusFilterChange,
        handleTransactionFunctionFilterChange,
        gasGolfingSuggestions,
        setGasGolfingSuggestions,
        gasGolfingLoading,
        gasGolfingError,
        setGasGolfingError,
        history,
        addToHistory,
        handleSimulate,
        handleClearAnalysis,
        handleWasmReady,
        analysisReport,
      }}
    >
      {children}
    </SimulationContext.Provider>
  );
}

export function useSimulation() {
  const context = useContext(SimulationContext);
  if (!context) {
    throw new Error('useSimulation must be used within a SimulationProvider');
  }
  return context;
}
