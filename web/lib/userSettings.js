/**
 * User preference storage for custom Soroban RPC and indexer endpoints.
 *
 * Power users running a self-hosted RPC node or indexer can point Sky Moon Scope at
 * their own infrastructure. Preferences live in LocalStorage only — nothing is
 * sent to the backend — and every read is defensive because the value can be
 * edited by hand or left over from an older schema.
 *
 * Per-network endpoint profiles
 * ─────────────────────────────
 * Each NetworkId ("mainnet" | "testnet" | "futurenet" | "localhost") can store
 * its own rpcUrl and indexerUrl independently under NETWORK_PROFILES_KEY.
 * The legacy flat SETTINGS_STORAGE_KEY is still honoured for the requestTimeoutMs
 * preference and is migrated to the active network's profile on first load.
 */

const SETTINGS_STORAGE_KEY = 'sky-moon-scope-user-settings';

/**
 * Key under which a map of { [networkId]: { rpcUrl, indexerUrl } } lives.
 * Stored separately from the legacy key so old code keeps working unchanged.
 */
const NETWORK_PROFILES_KEY = 'sky-moon-scope-network-profiles';

/**
 * All supported NetworkIds.  Matches the union type in NetworkContext.tsx.
 * Kept here as a plain array so this module stays framework-free and testable.
 */
const SUPPORTED_NETWORK_IDS = ['mainnet', 'testnet', 'futurenet', 'localhost'];

/** Empty string means "fall back to the built-in endpoint for the network". */
const DEFAULT_SETTINGS = {
  rpcUrl: '',
  indexerUrl: '',
  requestTimeoutMs: 15000,
};

const MIN_TIMEOUT_MS = 1000;
const MAX_TIMEOUT_MS = 120000;

// Debug warning gate for development only
const debugWarn = (...args) => {
  if (typeof window !== 'undefined' && process.env.NODE_ENV === 'development') {
    console.warn(...args);
  }
};

/**
 * Validate a user supplied endpoint.
 *
 * @param {string} value
 * @param {{ allowEmpty?: boolean }} [options]
 * @returns {{ valid: boolean, normalized: string, error: string|null }}
 */
function validateEndpointUrl(value, options = {}) {
  const allowEmpty = options.allowEmpty !== false;
  const raw = typeof value === 'string' ? value.trim() : '';

  if (raw === '') {
    return allowEmpty
      ? { valid: true, normalized: '', error: null }
      : { valid: false, normalized: '', error: 'Endpoint URL is required' };
  }

  let parsed;
  try {
    parsed = new URL(raw);
  } catch {
    return {
      valid: false,
      normalized: raw,
      error: 'Enter a full URL including the protocol, e.g. https://rpc.example.com',
    };
  }

  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    return {
      valid: false,
      normalized: raw,
      error: `Unsupported protocol "${parsed.protocol}" — use http:// or https://`,
    };
  }

  if (!parsed.hostname) {
    return { valid: false, normalized: raw, error: 'URL is missing a hostname' };
  }

  // Strip trailing slashes so path joining downstream never doubles up.
  const normalized = parsed.toString().replace(/\/+$/, '');
  return { valid: true, normalized, error: null };
}

function clampTimeout(value) {
  const num = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(num)) return DEFAULT_SETTINGS.requestTimeoutMs;
  return Math.min(MAX_TIMEOUT_MS, Math.max(MIN_TIMEOUT_MS, Math.round(num)));
}

/** Coerce an arbitrary object into a complete, valid settings object. */
function normalizeSettings(raw) {
  const input = raw && typeof raw === 'object' ? raw : {};
  const rpc = validateEndpointUrl(input.rpcUrl);
  const indexer = validateEndpointUrl(input.indexerUrl);

  return {
    rpcUrl: rpc.valid ? rpc.normalized : '',
    indexerUrl: indexer.valid ? indexer.normalized : '',
    requestTimeoutMs: clampTimeout(input.requestTimeoutMs),
  };
}

/** Validate a whole form payload before saving. */
function validateSettings(raw) {
  const input = raw && typeof raw === 'object' ? raw : {};
  const errors = {};

  const rpc = validateEndpointUrl(input.rpcUrl);
  if (!rpc.valid) errors.rpcUrl = rpc.error;

  const indexer = validateEndpointUrl(input.indexerUrl);
  if (!indexer.valid) errors.indexerUrl = indexer.error;

  return { valid: Object.keys(errors).length === 0, errors };
}

function resolveStorage(storage) {
  if (storage) return storage;
  if (typeof window !== 'undefined' && window.localStorage) return window.localStorage;
  return null;
}

/**
 * Read saved settings. Always returns a complete object, falling back to
 * defaults when storage is unavailable or the stored value is corrupt.
 */
function loadSettings(storage) {
  const store = resolveStorage(storage);
  if (!store) return { ...DEFAULT_SETTINGS };

  try {
    const raw = store.getItem(SETTINGS_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    return normalizeSettings(JSON.parse(raw));
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

/** Persist settings, returning the normalized object that was written. */
function saveSettings(settings, storage) {
  const normalized = normalizeSettings(settings);
  const store = resolveStorage(storage);
  if (!store) return normalized;

  try {
    store.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(normalized));
  } catch (error) {
    debugWarn('Failed to persist Sky Moon Scope settings:', error);
  }

  return normalized;
}

function clearSettings(storage) {
  const store = resolveStorage(storage);
  if (store) {
    try {
      store.removeItem(SETTINGS_STORAGE_KEY);
    } catch (error) {
      debugWarn('Failed to clear Sky Moon Scope settings:', error);
    }
  }
  return { ...DEFAULT_SETTINGS };
}

/** Custom endpoint wins over the built-in/network default when set. */
function resolveEndpoint(customUrl, fallbackUrl) {
  const custom = validateEndpointUrl(customUrl);
  if (custom.valid && custom.normalized !== '') return custom.normalized;
  return typeof fallbackUrl === 'string' ? fallbackUrl.replace(/\/+$/, '') : '';
}

// ── Per-network endpoint profiles ─────────────────────────────────────────────

/**
 * Empty endpoint profile — both fields blank mean "use the network default".
 *
 * @returns {{ rpcUrl: string, indexerUrl: string }}
 */
function emptyProfile() {
  return { rpcUrl: '', indexerUrl: '' };
}

/**
 * Normalise a raw profile object read from storage into a clean, safe shape.
 * Invalid or missing URLs are silently dropped (fall back to default).
 *
 * @param {unknown} raw
 * @returns {{ rpcUrl: string, indexerUrl: string }}
 */
function normalizeProfile(raw) {
  const input = raw && typeof raw === 'object' ? raw : {};
  const rpc = validateEndpointUrl(/** @type {string} */ (input.rpcUrl));
  const indexer = validateEndpointUrl(/** @type {string} */ (input.indexerUrl));
  return {
    rpcUrl: rpc.valid ? rpc.normalized : '',
    indexerUrl: indexer.valid ? indexer.normalized : '',
  };
}

/**
 * Validate a profile before saving it.
 *
 * @param {{ rpcUrl?: string, indexerUrl?: string }} raw
 * @returns {{ valid: boolean, errors: { rpcUrl?: string, indexerUrl?: string } }}
 */
function validateProfile(raw) {
  const input = raw && typeof raw === 'object' ? raw : {};
  const errors = {};
  const rpc = validateEndpointUrl(/** @type {string} */ (input.rpcUrl));
  if (!rpc.valid) errors.rpcUrl = rpc.error;
  const indexer = validateEndpointUrl(/** @type {string} */ (input.indexerUrl));
  if (!indexer.valid) errors.indexerUrl = indexer.error;
  return { valid: Object.keys(errors).length === 0, errors };
}

/**
 * Read the raw profiles map from storage.
 * Always returns a plain object (possibly empty) — never throws.
 *
 * @param {object|null} [storage]
 * @returns {Record<string, { rpcUrl: string, indexerUrl: string }>}
 */
function readRawProfiles(storage) {
  const store = resolveStorage(storage);
  if (!store) return {};
  try {
    const raw = store.getItem(NETWORK_PROFILES_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

/**
 * Load the endpoint profile for a specific network.
 *
 * On the very first call for a `networkId`, if the legacy flat settings key
 * has a non-blank rpcUrl or indexerUrl, those values are silently migrated
 * into the profile for that network and saved — so existing users never lose
 * their custom endpoints when the app updates.
 *
 * @param {string} networkId
 * @param {object|null} [storage]
 * @returns {{ rpcUrl: string, indexerUrl: string }}
 */
function loadNetworkProfile(networkId, storage) {
  if (typeof networkId !== 'string' || !SUPPORTED_NETWORK_IDS.includes(networkId)) {
    return emptyProfile();
  }

  const store = resolveStorage(storage);
  const profiles = readRawProfiles(store);

  if (networkId in profiles) {
    return normalizeProfile(profiles[networkId]);
  }

  // ── Backward-compat migration ─────────────────────────────────────────────
  // If this network's profile has never been saved, check whether the legacy
  // flat key has custom URLs (written by the old single-profile code).  Only
  // migrate when there IS something non-blank — we don't want to litter every
  // network's profile with empty strings and mark them as "explicitly set".
  //
  // Migration runs AT MOST ONCE: a `__migrated` sentinel is written into the
  // profiles map so that subsequent calls for OTHER networks don't also inherit
  // the legacy URL.  The sentinel is only visible to this function and is never
  // returned to callers.
  if (store) {
    try {
      const legacyRaw = store.getItem(SETTINGS_STORAGE_KEY);
      if (legacyRaw) {
        const legacy = normalizeSettings(JSON.parse(legacyRaw));
        // Skip migration if the sentinel is already present (ran before).
        if (!profiles.__migrated && (legacy.rpcUrl !== '' || legacy.indexerUrl !== '')) {
          const migrated = { rpcUrl: legacy.rpcUrl, indexerUrl: legacy.indexerUrl };
          profiles[networkId] = migrated;
          // Mark migration as done so other networks don't inherit this URL.
          profiles.__migrated = true;
          try {
            store.setItem(NETWORK_PROFILES_KEY, JSON.stringify(profiles));
          } catch (e) {
            debugWarn('Failed to migrate legacy settings to network profile:', e);
          }
          return migrated;
        }
      }
    } catch {
      // Corrupt legacy data — fall through to the empty default.
    }
  }

  return emptyProfile();
}

/**
 * Persist a profile for a specific network.
 * Returns the normalised profile that was written.
 *
 * @param {string} networkId
 * @param {{ rpcUrl?: string, indexerUrl?: string }} profile
 * @param {object|null} [storage]
 * @returns {{ rpcUrl: string, indexerUrl: string }}
 */
function saveNetworkProfile(networkId, profile, storage) {
  const normalized = normalizeProfile(profile);
  if (typeof networkId !== 'string' || !SUPPORTED_NETWORK_IDS.includes(networkId)) {
    return normalized;
  }

  const store = resolveStorage(storage);
  if (!store) return normalized;

  const profiles = readRawProfiles(store);
  profiles[networkId] = normalized;

  try {
    store.setItem(NETWORK_PROFILES_KEY, JSON.stringify(profiles));
  } catch (e) {
    debugWarn('Failed to persist network endpoint profile:', e);
  }

  return normalized;
}

/**
 * Remove the stored profile for a specific network, returning it to defaults.
 *
 * @param {string} networkId
 * @param {object|null} [storage]
 */
function clearNetworkProfile(networkId, storage) {
  if (typeof networkId !== 'string' || !SUPPORTED_NETWORK_IDS.includes(networkId)) return;

  const store = resolveStorage(storage);
  if (!store) return;

  const profiles = readRawProfiles(store);
  if (!(networkId in profiles)) return;

  delete profiles[networkId];

  try {
    if (Object.keys(profiles).length === 0) {
      store.removeItem(NETWORK_PROFILES_KEY);
    } else {
      store.setItem(NETWORK_PROFILES_KEY, JSON.stringify(profiles));
    }
  } catch (e) {
    debugWarn('Failed to clear network endpoint profile:', e);
  }
}

/**
 * Remove all stored per-network profiles.
 *
 * @param {object|null} [storage]
 */
function clearAllNetworkProfiles(storage) {
  const store = resolveStorage(storage);
  if (!store) return;
  try {
    store.removeItem(NETWORK_PROFILES_KEY);
  } catch (e) {
    debugWarn('Failed to clear all network endpoint profiles:', e);
  }
}

/**
 * Load all stored profiles as a map keyed by networkId.
 * Networks with no saved profile are not included in the result.
 *
 * @param {object|null} [storage]
 * @returns {Record<string, { rpcUrl: string, indexerUrl: string }>}
 */
function loadAllNetworkProfiles(storage) {
  const raw = readRawProfiles(resolveStorage(storage));
  const result = {};
  for (const id of SUPPORTED_NETWORK_IDS) {
    if (id in raw) {
      result[id] = normalizeProfile(raw[id]);
    }
  }
  return result;
}

/**
 * Resolve the effective RPC URL for a network:
 *   1. The custom override stored in the per-network profile (if set).
 *   2. The fallback built-in RPC URL from `NETWORKS[networkId].rpcUrl`.
 *
 * @param {string} networkId
 * @param {string} builtInRpcUrl   Default from NETWORKS config.
 * @param {object|null} [storage]
 * @returns {string}
 */
function resolveNetworkRpcUrl(networkId, builtInRpcUrl, storage) {
  const profile = loadNetworkProfile(networkId, storage);
  return resolveEndpoint(profile.rpcUrl, builtInRpcUrl);
}

/**
 * Resolve the effective indexer URL for a network:
 *   1. The custom override stored in the per-network profile (if set).
 *   2. The `builtInIndexerUrl` fallback (typically NEXT_PUBLIC_API_URL).
 *
 * @param {string} networkId
 * @param {string} builtInIndexerUrl
 * @param {object|null} [storage]
 * @returns {string}
 */
function resolveNetworkIndexerUrl(networkId, builtInIndexerUrl, storage) {
  const profile = loadNetworkProfile(networkId, storage);
  return resolveEndpoint(profile.indexerUrl, builtInIndexerUrl);
}

module.exports = {
  SETTINGS_STORAGE_KEY,
  NETWORK_PROFILES_KEY,
  SUPPORTED_NETWORK_IDS,
  DEFAULT_SETTINGS,
  MIN_TIMEOUT_MS,
  MAX_TIMEOUT_MS,
  validateEndpointUrl,
  validateSettings,
  normalizeSettings,
  loadSettings,
  saveSettings,
  clearSettings,
  resolveEndpoint,
  // Per-network profile API
  emptyProfile,
  normalizeProfile,
  validateProfile,
  loadNetworkProfile,
  saveNetworkProfile,
  clearNetworkProfile,
  clearAllNetworkProfiles,
  loadAllNetworkProfiles,
  resolveNetworkRpcUrl,
  resolveNetworkIndexerUrl,
};
