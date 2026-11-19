import type { SearchCommand } from './searchCommands';

export interface HistoryEntry {
  id: string;
  title: string;
  subtitle?: string;
  group: string;
  href?: string;
  action?: string;
  payload?: Record<string, unknown>;
  /** Unix timestamp (ms) of the most recent visit. */
  visitedAt: number;
}

/** LocalStorage key used to persist the search history. */
export declare const STORAGE_KEY: string;

/** Maximum number of entries kept in history. */
export declare const MAX_HISTORY: number;

/** Build a `HistoryEntry` from a `SearchCommand`. */
export declare function commandToEntry(command: SearchCommand): HistoryEntry;

/**
 * Push `command` to the top of the history list.
 * Deduplicates by id and trims to `MAX_HISTORY`.
 */
export declare function pushHistory(history: HistoryEntry[], command: SearchCommand): HistoryEntry[];

/** Remove a single entry by id. */
export declare function removeHistoryEntry(history: HistoryEntry[], id: string): HistoryEntry[];

/** Return an empty history list. */
export declare function clearHistory(): HistoryEntry[];

/**
 * Convert history entries to `SearchCommand`-shaped objects so they work with
 * `filterCommands`.
 */
export declare function historyToCommands(history: HistoryEntry[]): SearchCommand[];

/** Load history from localStorage. Returns [] on any error. */
export declare function loadHistory(): HistoryEntry[];

/** Persist history to localStorage. Silently ignores errors. */
export declare function saveHistory(history: HistoryEntry[]): void;
