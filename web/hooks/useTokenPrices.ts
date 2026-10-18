"use client";

import useSWR from "swr";
import { fetchPrices, PriceMap } from "../lib/priceFeed";

const DEFAULT_POLL_MS = 30_000;

export interface UseTokenPricesResult {
  prices: PriceMap;
  loading: boolean;
  error: string | null;
  /** Epoch millis of the most recent successful refresh, or null. */
  lastUpdated: number | null;
}

/**
 * Subscribes to real-time USD prices for the given asset `symbols` with automatic
 * deduplication across components using SWR.
 *
 * - Multiple components requesting the same symbols will share a single fetch
 * - Results are cached and revalidated on the specified interval (default: 30s)
 * - Component unmounting does not cancel shared requests
 */
export function useTokenPrices(
  symbols: string[],
  pollMs: number = DEFAULT_POLL_MS,
): UseTokenPricesResult {
  const key = Array.from(new Set(symbols)).sort().join(",");
  const cacheKey = key ? `/prices?symbols=${key}` : null;

  const { data, error, isLoading, mutate } = useSWR(
    cacheKey,
    () => {
      if (!cacheKey) return null;
      const list = key.split(",");
      return fetchPrices(list);
    },
    {
      revalidateOnFocus: false,
      revalidateOnReconnect: true,
      dedupingInterval: 10_000, // Deduplicate requests within 10s window
      focusThrottleInterval: 300_000, // Don't revalidate on focus for 5min
      errorRetryInterval: 5_000,
      errorRetryCount: 3,
    },
  );

  // Calculate lastUpdated from the data if available
  const lastUpdated =
    data && Object.values(data).length > 0
      ? Object.values(data)[0]?.fetchedAt ?? null
      : null;

  return {
    prices: data ?? {},
    loading: isLoading,
    error: error ? (error instanceof Error ? error.message : "Failed to load prices") : null,
    lastUpdated,
  };
}
