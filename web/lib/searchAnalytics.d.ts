/** A single entry from the analytics frequency map. */
export interface AnalyticsEntry {
  /** The query string or command id. */
  value: string;
  /** Number of times it has been recorded. */
  count: number;
}

/** The shape of the object persisted to LocalStorage. */
export interface StoredAnalytics {
  /** Normalised query strings → hit counts. */
  queries: Record<string, number>;
  /** Command ids → selection counts. */
  commands: Record<string, number>;
}

/** LocalStorage key used by the analytics module. */
export declare const ANALYTICS_STORAGE_KEY: string;

/** Maximum number of distinct entries stored per dimension. */
export declare const MAX_ENTRIES: number;

/** Minimum normalised query length that is worth recording. */
export declare const MIN_QUERY_LENGTH: number;

/** Lowercase + collapse-whitespace normalisation applied to every query. */
export declare function normalizeQuery(query: string): string;

/**
 * Read the persisted analytics from storage.
 * Always returns a valid object; corrupt / missing data yields empty maps.
 */
export declare function loadAnalytics(storage?: Storage | null): StoredAnalytics;

/** Persist an analytics object. */
export declare function saveAnalytics(
  analytics: StoredAnalytics,
  storage?: Storage | null,
): void;

/**
 * Record a search interaction: the query the user typed and/or the command
 * they selected.  Pass an empty string for either dimension to skip it.
 * Queries shorter than `MIN_QUERY_LENGTH` are silently ignored.
 */
export declare function recordSearch(
  query: string,
  commandId: string,
  storage?: Storage | null,
): void;

/**
 * Return the `n` most-searched query strings, descending by count.
 * Ties are broken alphabetically.
 */
export declare function getTopQueries(
  n?: number,
  storage?: Storage | null,
): AnalyticsEntry[];

/**
 * Return the `n` most-selected command ids, descending by count.
 * Ties are broken alphabetically.
 */
export declare function getTopCommands(
  n?: number,
  storage?: Storage | null,
): AnalyticsEntry[];

/** Erase all stored analytics. */
export declare function clearAnalytics(storage?: Storage | null): void;
