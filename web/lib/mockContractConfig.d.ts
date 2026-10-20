import type { ContractFunction, TransactionRecord } from './sorobantypes';

export interface MockContractConfig {
  contractId: string;
  hasContractIdOverride: boolean;
  functions: ContractFunction[];
  results: Record<string, unknown>;
  transactions: TransactionRecord[] | null;
}

export function loadMockContractConfig(
  env: Record<string, string | undefined>,
  defaults: MockContractConfig,
): MockContractConfig;

export function resolveMockResult(
  functionName: string,
  overrides: Record<string, unknown>,
  fallback: () => unknown,
): unknown;