/**
 * Client-side storage utilities for persisting the latest WASM analysis result.
 *
 * This module handles saving and restoring the most recent analysis result
 * to/from browser local storage, allowing the UI to preserve state across
 * page refreshes.
 *
 * ## Why plain localStorage (not encryptedStorage)?
 *
 * Analysis results contain only performance metrics (CPU instructions, RAM
 * bytes, ledger footprint, etc.) — no private keys, wallet addresses, or
 * other secrets that need to be hidden from the browser's storage layer.
 * The previous encryptedStorage-based implementation used an ephemeral AES-GCM
 * key that was generated fresh on every page load, meaning any data saved in
 * one session could never be decrypted after a refresh.  That caused the
 * "decoded contract name not persisting after page refresh" bug (#416).
 * Plain localStorage is the correct choice for non-sensitive data that must
 * survive across page loads.
 */

import type { InvocationResult } from './sorobantypes';
import { debugWarn } from './debugUtils';
import {
  sanitizePlainText,
  sanitizeMermaidDefinition,
  stripControlChars,
} from './security';

const LATEST_ANALYSIS_KEY = 'sky-moon-scope-latest-analysis';

const MAX_FUNCTION_NAME_LENGTH = 64;
const MAX_WASM_FILE_NAME_LENGTH = 260; // max sane filesystem path length
const MAX_RESULT_FIELD_LENGTH = 1_000_000;

/**
 * Sanitize an untrusted object (e.g. restored from localStorage, which any
 * attacker can edit) into a render-safe InvocationResult. Immutable string
 * fields are length-capped and control-char-stripped; the mermaid definition —
 * the only field rendered through raw innerHTML downstream — is bounded so a
 * tampered graph cannot be used for a stored-XSS payload.
 */
export function sanitizeRestoredResult(parsed: unknown): InvocationResult | null {
  if (!parsed || typeof parsed !== 'object') return null;
  const result = parsed as Record<string, unknown>;

  if (typeof result.id !== 'string' || result.id.length === 0) return null;

  const functionName = sanitizePlainText(result.functionName, MAX_FUNCTION_NAME_LENGTH);
  if (!functionName) return null;

  return {
    ...result,
    id: stripControlChars(result.id).slice(0, 128),
    functionName,
    wasmFileName:
      typeof result.wasmFileName === 'string'
        ? sanitizePlainText(result.wasmFileName, MAX_WASM_FILE_NAME_LENGTH) ?? undefined
        : undefined,
    error:
      typeof result.error === 'string'
        ? sanitizePlainText(result.error, MAX_RESULT_FIELD_LENGTH)
        : result.error,
    errorType:
      typeof result.errorType === 'string'
        ? sanitizePlainText(result.errorType, 256)
        : result.errorType,
    callGraphMermaid:
      typeof result.callGraphMermaid === 'string'
        ? sanitizeMermaidDefinition(result.callGraphMermaid)
        : result.callGraphMermaid,
    inputs:
      result.inputs && typeof result.inputs === 'object'
        ? sanitizeRestoredInputs(result.inputs)
        : result.inputs,
  } as InvocationResult;
}

function sanitizeRestoredInputs(
  inputs: Record<string, unknown>,
): Record<string, unknown> {
  const sanitized: Record<string, unknown> = {};
  for (const [name, value] of Object.entries(inputs)) {
    const key = stripControlChars(name).slice(0, 128);
    if (typeof value === 'string') {
      sanitized[key] = sanitizePlainText(value, MAX_RESULT_FIELD_LENGTH);
    } else {
      sanitized[key] = value;
    }
  }
  return sanitized;
}

/** Returns `window.localStorage` when available, `null` during SSR. */
function getPlainStorage(): Storage | null {
  if (typeof window === 'undefined' || !window.localStorage) return null;
  return window.localStorage;
}

/**
 * Save the latest analysis result to local storage.
 * Only the most recent result is kept — new results overwrite old ones.
 *
 * This is a synchronous operation because plain localStorage is synchronous.
 * The return type is `void` (not `Promise<void>`) so callers do not need to
 * await it.
 *
 * @param result - The analysis result to persist
 */
export function saveLatestAnalysis(result: InvocationResult): void {
  const storage = getPlainStorage();
  if (!storage) return;

  try {
    const serialized = JSON.stringify(result);
    storage.setItem(LATEST_ANALYSIS_KEY, serialized);
  } catch (error) {
    // Silently fail if storage is full or unavailable
    debugWarn('Failed to save latest analysis to local storage:', error);
  }
}

/**
 * Restore the latest analysis result from local storage.
 * Returns null if no valid result is found or if parsing fails.
 *
 * This is a synchronous operation — it no longer returns a Promise.
 * Callers that previously awaited this function will continue to work because
 * `await null` resolves to `null` and `await value` resolves to `value`.
 *
 * @returns The restored analysis result, or null if unavailable
 */
export function loadLatestAnalysis(): InvocationResult | null {
  const storage = getPlainStorage();
  if (!storage) return null;

  try {
    const stored = storage.getItem(LATEST_ANALYSIS_KEY);
    if (!stored) return null;

    const parsed = JSON.parse(stored) as InvocationResult;

    // Validate and sanitize the stored data — localStorage is user-editable, so
    // restored content is treated as untrusted before it is re-rendered.
    const sanitized = sanitizeRestoredResult(parsed);
    if (!sanitized) {
      debugWarn('Invalid analysis result in storage, ignoring');
      return null;
    }

    return sanitized;
  } catch (error) {
    // Handle malformed JSON or other parsing errors gracefully
    debugWarn('Failed to load latest analysis from local storage:', error);
    return null;
  }
}

/**
 * Clear the stored latest analysis result.
 * Useful for cleanup or when explicitly resetting the UI state.
 */
export function clearLatestAnalysis(): void {
  const storage = getPlainStorage();
  if (!storage) return;

  try {
    storage.removeItem(LATEST_ANALYSIS_KEY);
  } catch (error) {
    debugWarn('Failed to clear latest analysis from local storage:', error);
  }
}
