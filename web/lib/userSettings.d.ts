export declare const SETTINGS_STORAGE_KEY: string;
/**
 * LocalStorage key under which per-network endpoint profiles are stored.
 * Value is a JSON map of `{ [networkId]: NetworkEndpointProfile }`.
 */
export declare const NETWORK_PROFILES_KEY: string;
/** Ordered tuple of every supported NetworkId. */
export declare const SUPPORTED_NETWORK_IDS: ReadonlyArray<NetworkId>;

export declare const MIN_TIMEOUT_MS: number;
export declare const MAX_TIMEOUT_MS: number;

// ── Network IDs ───────────────────────────────────────────────────────────────

/**
 * Mirrors the `NetworkId` union in context/NetworkContext.tsx.
 * Kept in sync manually; tests will fail if they diverge.
 */
export type NetworkId = 'mainnet' | 'testnet' | 'futurenet' | 'localhost';

// ── Legacy flat settings (requestTimeoutMs + single-network overrides) ─────────

export interface UserSettings {
  /** Custom Soroban RPC endpoint; empty string means "use the network default". */
  rpcUrl: string;
  /** Custom indexer/analyzer backend base URL; empty string means "use NEXT_PUBLIC_API_URL". */
  indexerUrl: string;
  requestTimeoutMs: number;
}

export declare const DEFAULT_SETTINGS: UserSettings;

export interface EndpointValidation {
  valid: boolean;
  normalized: string;
  error: string | null;
}

export declare function validateEndpointUrl(
  value: string,
  options?: { allowEmpty?: boolean },
): EndpointValidation;

export declare function validateSettings(raw: Partial<UserSettings>): {
  valid: boolean;
  errors: Partial<Record<keyof UserSettings, string>>;
};

export declare function normalizeSettings(raw: unknown): UserSettings;

/** A minimal Storage-compatible interface so tests can inject a fake. */
export interface SettingsStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export declare function loadSettings(storage?: SettingsStorage): UserSettings;
export declare function saveSettings(
  settings: Partial<UserSettings>,
  storage?: SettingsStorage,
): UserSettings;
export declare function clearSettings(storage?: SettingsStorage): UserSettings;
export declare function resolveEndpoint(customUrl: string, fallbackUrl: string): string;

// ── Per-network endpoint profiles ─────────────────────────────────────────────

/**
 * A pair of custom endpoint overrides for one network.
 * An empty string for either field means "use the built-in default".
 */
export interface NetworkEndpointProfile {
  rpcUrl: string;
  indexerUrl: string;
}

/** Validation result for a single `NetworkEndpointProfile`. */
export interface ProfileValidation {
  valid: boolean;
  errors: Partial<Record<keyof NetworkEndpointProfile, string>>;
}

/** Returns an empty (all-default) endpoint profile. */
export declare function emptyProfile(): NetworkEndpointProfile;

/**
 * Coerce an arbitrary value into a clean, safe `NetworkEndpointProfile`.
 * Invalid URLs are silently dropped (field becomes empty string).
 */
export declare function normalizeProfile(raw: unknown): NetworkEndpointProfile;

/**
 * Validate both fields of a profile before saving.
 * Returns per-field error messages similar to `validateSettings`.
 */
export declare function validateProfile(
  raw: Partial<NetworkEndpointProfile>,
): ProfileValidation;

/**
 * Load the stored endpoint profile for `networkId`.
 *
 * If the network has no saved profile yet but the legacy flat settings key
 * (`SETTINGS_STORAGE_KEY`) holds non-blank URLs, those are automatically
 * migrated and saved under the new key — backward-compatible upgrade.
 */
export declare function loadNetworkProfile(
  networkId: NetworkId,
  storage?: SettingsStorage,
): NetworkEndpointProfile;

/**
 * Persist a profile for `networkId`.
 * Returns the normalised profile that was written.
 */
export declare function saveNetworkProfile(
  networkId: NetworkId,
  profile: Partial<NetworkEndpointProfile>,
  storage?: SettingsStorage,
): NetworkEndpointProfile;

/**
 * Remove the stored profile for `networkId`, reverting it to built-in defaults.
 * No-op if the network has no stored profile.
 */
export declare function clearNetworkProfile(
  networkId: NetworkId,
  storage?: SettingsStorage,
): void;

/**
 * Remove all stored per-network profiles (equivalent to a full settings wipe
 * for the new storage key).
 */
export declare function clearAllNetworkProfiles(storage?: SettingsStorage): void;

/**
 * Return a map of every network that has a saved profile.
 * Networks with no stored profile are absent from the result.
 */
export declare function loadAllNetworkProfiles(
  storage?: SettingsStorage,
): Partial<Record<NetworkId, NetworkEndpointProfile>>;

/**
 * Resolve the effective RPC URL for a network:
 *   1. Custom override from the per-network profile (if non-blank).
 *   2. `builtInRpcUrl` fallback (from `NETWORKS[networkId].rpcUrl`).
 */
export declare function resolveNetworkRpcUrl(
  networkId: NetworkId,
  builtInRpcUrl: string,
  storage?: SettingsStorage,
): string;

/**
 * Resolve the effective indexer URL for a network:
 *   1. Custom override from the per-network profile (if non-blank).
 *   2. `builtInIndexerUrl` fallback (typically `NEXT_PUBLIC_API_URL`).
 */
export declare function resolveNetworkIndexerUrl(
  networkId: NetworkId,
  builtInIndexerUrl: string,
  storage?: SettingsStorage,
): string;
