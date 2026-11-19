// userSettings.test.cjs — unit tests for custom RPC/indexer preference storage
// Closes Issue #622
// Runs with: node --test ./lib/userSettings.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  SETTINGS_STORAGE_KEY,
  NETWORK_PROFILES_KEY,
  SUPPORTED_NETWORK_IDS,
  DEFAULT_SETTINGS,
  validateEndpointUrl,
  validateSettings,
  normalizeSettings,
  loadSettings,
  saveSettings,
  clearSettings,
  resolveEndpoint,
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
} = require('./userSettings');

/** Minimal in-memory LocalStorage stand-in. */
function createStorage(initial = {}) {
  const map = new Map(Object.entries(initial));
  return {
    getItem: (key) => (map.has(key) ? map.get(key) : null),
    setItem: (key, value) => map.set(key, String(value)),
    removeItem: (key) => map.delete(key),
    get size() {
      return map.size;
    },
  };
}

// ── URL validation ──────────────────────────────────────────────────────────

test('validateEndpointUrl: accepts http and https URLs', () => {
  assert.equal(validateEndpointUrl('https://rpc.example.com').valid, true);
  assert.equal(validateEndpointUrl('http://localhost:8000/soroban/rpc').valid, true);
});

test('validateEndpointUrl: trims surrounding whitespace from valid URLs', () => {
  const result = validateEndpointUrl('  https://rpc.example.com/soroban/rpc  ');
  assert.deepEqual(result, {
    valid: true,
    normalized: 'https://rpc.example.com/soroban/rpc',
    error: null,
  });
});

test('validateEndpointUrl: strips trailing slashes when normalizing', () => {
  assert.equal(validateEndpointUrl('https://rpc.example.com///').normalized, 'https://rpc.example.com');
});

test('validateEndpointUrl: treats blank input as "use the default"', () => {
  assert.deepEqual(validateEndpointUrl(''), { valid: true, normalized: '', error: null });
  assert.deepEqual(validateEndpointUrl('   '), { valid: true, normalized: '', error: null });
  assert.equal(validateEndpointUrl(undefined).valid, true);
});

test('validateEndpointUrl: rejects blank input when allowEmpty is false', () => {
  const result = validateEndpointUrl('', { allowEmpty: false });
  assert.equal(result.valid, false);
  assert.match(result.error, /required/i);
});

test('validateEndpointUrl: rejects a URL with no protocol', () => {
  const result = validateEndpointUrl('rpc.example.com');
  assert.equal(result.valid, false);
  assert.match(result.error, /including the protocol/i);
});

test('validateEndpointUrl: rejects non-http protocols', () => {
  const result = validateEndpointUrl('ftp://rpc.example.com');
  assert.equal(result.valid, false);
  assert.match(result.error, /Unsupported protocol/);
});

test('validateEndpointUrl: rejects javascript and data URLs', () => {
  for (const value of ['javascript:alert(1)', 'data:text/html,<script>alert(1)</script>']) {
    const result = validateEndpointUrl(value);
    assert.equal(result.valid, false);
    assert.equal(result.normalized, value);
    assert.match(result.error, /Unsupported protocol/);
  }
});

test('validateEndpointUrl: rejects malformed and non-URL input', () => {
  for (const value of ['not-a-url', 'https://']) {
    const result = validateEndpointUrl(value);
    assert.equal(result.valid, false);
  }
});

// ── Normalization ───────────────────────────────────────────────────────────

test('normalizeSettings: fills in defaults for junk input', () => {
  assert.deepEqual(normalizeSettings(null), DEFAULT_SETTINGS);
  assert.deepEqual(normalizeSettings('nope'), DEFAULT_SETTINGS);
  assert.deepEqual(normalizeSettings({}), DEFAULT_SETTINGS);
});

test('normalizeSettings: drops invalid endpoints rather than storing them', () => {
  const normalized = normalizeSettings({ rpcUrl: 'not a url', indexerUrl: 'https://api.example.com' });
  assert.equal(normalized.rpcUrl, '');
  assert.equal(normalized.indexerUrl, 'https://api.example.com');
});

test('normalizeSettings: clamps the request timeout into range', () => {
  assert.equal(normalizeSettings({ requestTimeoutMs: 5 }).requestTimeoutMs, 1000);
  assert.equal(normalizeSettings({ requestTimeoutMs: 999999 }).requestTimeoutMs, 120000);
  assert.equal(normalizeSettings({ requestTimeoutMs: '4200' }).requestTimeoutMs, 4200);
  assert.equal(normalizeSettings({ requestTimeoutMs: NaN }).requestTimeoutMs, 15000);
});

// ── Form validation ─────────────────────────────────────────────────────────

test('validateSettings: reports per-field errors', () => {
  const result = validateSettings({ rpcUrl: 'ws://bad', indexerUrl: 'https://ok.example.com' });
  assert.equal(result.valid, false);
  assert.ok(result.errors.rpcUrl);
  assert.equal(result.errors.indexerUrl, undefined);
});

test('validateSettings: passes when both endpoints are blank', () => {
  assert.equal(validateSettings({ rpcUrl: '', indexerUrl: '' }).valid, true);
});

// ── Persistence ─────────────────────────────────────────────────────────────

test('loadSettings: returns defaults when nothing is stored', () => {
  assert.deepEqual(loadSettings(createStorage()), DEFAULT_SETTINGS);
});

test('loadSettings: returns defaults when storage is unavailable', () => {
  assert.deepEqual(loadSettings(null), DEFAULT_SETTINGS);
});

test('loadSettings: recovers from corrupt JSON instead of throwing', () => {
  const storage = createStorage({ [SETTINGS_STORAGE_KEY]: '{ not json' });
  assert.deepEqual(loadSettings(storage), DEFAULT_SETTINGS);
});

test('saveSettings then loadSettings round-trips normalized values', () => {
  const storage = createStorage();
  const written = saveSettings(
    { rpcUrl: 'https://my-rpc.example.com/', indexerUrl: '', requestTimeoutMs: 3000 },
    storage,
  );

  assert.equal(written.rpcUrl, 'https://my-rpc.example.com');
  assert.deepEqual(loadSettings(storage), written);
});

test('saveSettings: does not persist a rejected RPC URL', () => {
  const storage = createStorage();
  const written = saveSettings({ rpcUrl: 'javascript:alert(1)' }, storage);

  assert.equal(written.rpcUrl, '');
  assert.deepEqual(JSON.parse(storage.getItem(SETTINGS_STORAGE_KEY)), {
    ...DEFAULT_SETTINGS,
    rpcUrl: '',
  });
  assert.equal(loadSettings(storage).rpcUrl, '');
});

test('loadSettings: strips an unsafe RPC URL already present in LocalStorage', () => {
  const storage = createStorage({
    [SETTINGS_STORAGE_KEY]: JSON.stringify({
      rpcUrl: 'javascript:alert(1)',
      indexerUrl: 'https://api.example.com',
    }),
  });

  const loaded = loadSettings(storage);
  assert.equal(loaded.rpcUrl, '');
  assert.equal(loaded.indexerUrl, 'https://api.example.com');
});

test('saveSettings: writes under the documented storage key', () => {
  const storage = createStorage();
  saveSettings({ indexerUrl: 'https://api.example.com' }, storage);
  const raw = JSON.parse(storage.getItem(SETTINGS_STORAGE_KEY));
  assert.equal(raw.indexerUrl, 'https://api.example.com');
});

test('clearSettings: removes the stored value and returns defaults', () => {
  const storage = createStorage();
  saveSettings({ rpcUrl: 'https://my-rpc.example.com' }, storage);
  assert.equal(storage.size, 1);

  assert.deepEqual(clearSettings(storage), DEFAULT_SETTINGS);
  assert.equal(storage.size, 0);
  assert.deepEqual(loadSettings(storage), DEFAULT_SETTINGS);
});

// ── Endpoint resolution ─────────────────────────────────────────────────────

test('resolveEndpoint: prefers a valid custom URL over the fallback', () => {
  assert.equal(
    resolveEndpoint('https://my-rpc.example.com', 'https://soroban-testnet.stellar.org'),
    'https://my-rpc.example.com',
  );
});

test('resolveEndpoint: falls back when the custom URL is blank or invalid', () => {
  assert.equal(resolveEndpoint('', 'http://localhost:8080'), 'http://localhost:8080');
  assert.equal(resolveEndpoint('nonsense', 'http://localhost:8080'), 'http://localhost:8080');
});

test('resolveEndpoint: strips trailing slashes from the fallback too', () => {
  assert.equal(resolveEndpoint('', 'http://localhost:8080/'), 'http://localhost:8080');
});

// ── Per-network endpoint profiles ────────────────────────────────────────────

// ── Constants ─────────────────────────────────────────────────────────────────

test('NETWORK_PROFILES_KEY is a distinct key from SETTINGS_STORAGE_KEY', () => {
  assert.notEqual(NETWORK_PROFILES_KEY, SETTINGS_STORAGE_KEY);
  assert.equal(typeof NETWORK_PROFILES_KEY, 'string');
  assert.ok(NETWORK_PROFILES_KEY.length > 0);
});

test('SUPPORTED_NETWORK_IDS contains the four canonical networks', () => {
  assert.ok(Array.isArray(SUPPORTED_NETWORK_IDS));
  for (const id of ['mainnet', 'testnet', 'futurenet', 'localhost']) {
    assert.ok(SUPPORTED_NETWORK_IDS.includes(id), `${id} should be supported`);
  }
  assert.equal(SUPPORTED_NETWORK_IDS.length, 4);
});

// ── emptyProfile ──────────────────────────────────────────────────────────────

test('emptyProfile: returns an object with two blank strings', () => {
  const p = emptyProfile();
  assert.deepEqual(p, { rpcUrl: '', indexerUrl: '' });
});

test('emptyProfile: each call returns a fresh object (not shared reference)', () => {
  const a = emptyProfile();
  const b = emptyProfile();
  a.rpcUrl = 'mutated';
  assert.equal(b.rpcUrl, '');
});

// ── normalizeProfile ──────────────────────────────────────────────────────────

test('normalizeProfile: accepts valid URLs and strips trailing slashes', () => {
  const p = normalizeProfile({ rpcUrl: 'https://rpc.example.com/', indexerUrl: 'https://api.example.com' });
  assert.equal(p.rpcUrl, 'https://rpc.example.com');
  assert.equal(p.indexerUrl, 'https://api.example.com');
});

test('normalizeProfile: drops invalid URLs (falls back to empty string)', () => {
  const p = normalizeProfile({ rpcUrl: 'not-a-url', indexerUrl: 'javascript:alert(1)' });
  assert.equal(p.rpcUrl, '');
  assert.equal(p.indexerUrl, '');
});

test('normalizeProfile: treats blank fields as empty string', () => {
  const p = normalizeProfile({ rpcUrl: '', indexerUrl: '   ' });
  assert.equal(p.rpcUrl, '');
  assert.equal(p.indexerUrl, '');
});

test('normalizeProfile: returns empty profile for null or non-object input', () => {
  assert.deepEqual(normalizeProfile(null), emptyProfile());
  assert.deepEqual(normalizeProfile('string'), emptyProfile());
  assert.deepEqual(normalizeProfile(42), emptyProfile());
});

// ── validateProfile ───────────────────────────────────────────────────────────

test('validateProfile: passes when both fields are blank', () => {
  const result = validateProfile({ rpcUrl: '', indexerUrl: '' });
  assert.equal(result.valid, true);
  assert.deepEqual(result.errors, {});
});

test('validateProfile: passes when both fields are valid URLs', () => {
  const result = validateProfile({
    rpcUrl: 'https://rpc.example.com',
    indexerUrl: 'https://api.example.com',
  });
  assert.equal(result.valid, true);
});

test('validateProfile: fails with per-field errors for invalid URLs', () => {
  const result = validateProfile({ rpcUrl: 'not-a-url', indexerUrl: 'ws://bad-protocol' });
  assert.equal(result.valid, false);
  assert.ok(result.errors.rpcUrl, 'should have rpcUrl error');
  assert.ok(result.errors.indexerUrl, 'should have indexerUrl error');
});

test('validateProfile: one invalid field does not invalidate the other', () => {
  const result = validateProfile({ rpcUrl: 'not-a-url', indexerUrl: '' });
  assert.equal(result.valid, false);
  assert.ok(result.errors.rpcUrl);
  assert.equal(result.errors.indexerUrl, undefined);
});

// ── loadNetworkProfile ────────────────────────────────────────────────────────

test('loadNetworkProfile: returns empty profile when nothing is stored', () => {
  const storage = createStorage();
  assert.deepEqual(loadNetworkProfile('testnet', storage), emptyProfile());
});

test('loadNetworkProfile: returns stored profile for the correct network', () => {
  const storage = createStorage();
  saveNetworkProfile('testnet', { rpcUrl: 'https://custom.example.com', indexerUrl: '' }, storage);

  assert.deepEqual(loadNetworkProfile('testnet', storage), {
    rpcUrl: 'https://custom.example.com',
    indexerUrl: '',
  });
});

test('loadNetworkProfile: profiles for different networks are isolated', () => {
  const storage = createStorage();
  saveNetworkProfile('mainnet',  { rpcUrl: 'https://mainnet.example.com', indexerUrl: '' }, storage);
  saveNetworkProfile('testnet',  { rpcUrl: 'https://testnet.example.com', indexerUrl: '' }, storage);
  saveNetworkProfile('futurenet', { rpcUrl: 'https://futurenet.example.com', indexerUrl: '' }, storage);

  assert.equal(loadNetworkProfile('mainnet', storage).rpcUrl,  'https://mainnet.example.com');
  assert.equal(loadNetworkProfile('testnet', storage).rpcUrl,  'https://testnet.example.com');
  assert.equal(loadNetworkProfile('futurenet', storage).rpcUrl, 'https://futurenet.example.com');
  // localhost was never saved
  assert.deepEqual(loadNetworkProfile('localhost', storage), emptyProfile());
});

test('loadNetworkProfile: normalises a corrupt stored URL to empty string', () => {
  const storage = createStorage({
    [NETWORK_PROFILES_KEY]: JSON.stringify({ testnet: { rpcUrl: 'javascript:alert(1)', indexerUrl: '' } }),
  });
  const profile = loadNetworkProfile('testnet', storage);
  assert.equal(profile.rpcUrl, '');
});

test('loadNetworkProfile: returns empty profile for unknown networkId', () => {
  const storage = createStorage();
  assert.deepEqual(loadNetworkProfile('unknown', storage), emptyProfile());
  assert.deepEqual(loadNetworkProfile('', storage), emptyProfile());
  assert.deepEqual(loadNetworkProfile(null, storage), emptyProfile());
});

test('loadNetworkProfile: returns empty profile when storage is unavailable', () => {
  assert.deepEqual(loadNetworkProfile('testnet', null), emptyProfile());
});

test('loadNetworkProfile: handles corrupt NETWORK_PROFILES_KEY JSON gracefully', () => {
  const storage = createStorage({ [NETWORK_PROFILES_KEY]: '{ not json' });
  assert.deepEqual(loadNetworkProfile('testnet', storage), emptyProfile());
});

// ── Legacy migration ──────────────────────────────────────────────────────────

test('loadNetworkProfile: migrates non-blank legacy flat settings on first load', () => {
  // Pre-populate the legacy key with custom endpoints.
  const storage = createStorage({
    [SETTINGS_STORAGE_KEY]: JSON.stringify({
      rpcUrl: 'https://legacy-rpc.example.com',
      indexerUrl: 'https://legacy-api.example.com',
      requestTimeoutMs: 5000,
    }),
  });

  // No per-network profiles exist yet.
  const profile = loadNetworkProfile('testnet', storage);

  // Should have been migrated.
  assert.equal(profile.rpcUrl, 'https://legacy-rpc.example.com');
  assert.equal(profile.indexerUrl, 'https://legacy-api.example.com');

  // The new key should now exist in storage (so subsequent calls skip migration).
  const raw = JSON.parse(storage.getItem(NETWORK_PROFILES_KEY));
  assert.ok(raw.testnet, 'testnet profile should have been persisted');
});

test('loadNetworkProfile: second load after migration reads from new key, not legacy', () => {
  const storage = createStorage({
    [SETTINGS_STORAGE_KEY]: JSON.stringify({
      rpcUrl: 'https://legacy-rpc.example.com',
      indexerUrl: '',
      requestTimeoutMs: 5000,
    }),
  });

  // First load triggers migration.
  loadNetworkProfile('testnet', storage);

  // Overwrite the legacy key with something different.
  storage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify({ rpcUrl: 'https://DIFFERENT.example.com', indexerUrl: '' }));

  // Second load should still return the migrated (first) value.
  const profile = loadNetworkProfile('testnet', storage);
  assert.equal(profile.rpcUrl, 'https://legacy-rpc.example.com');
});

test('loadNetworkProfile: does NOT migrate when legacy settings have blank URLs', () => {
  // Only a requestTimeoutMs is set — no RPC / indexer override.
  const storage = createStorage({
    [SETTINGS_STORAGE_KEY]: JSON.stringify({ rpcUrl: '', indexerUrl: '', requestTimeoutMs: 30000 }),
  });

  const profile = loadNetworkProfile('testnet', storage);
  assert.deepEqual(profile, emptyProfile());
  // The new profiles key should NOT have been written.
  assert.equal(storage.getItem(NETWORK_PROFILES_KEY), null);
});

test('loadNetworkProfile: migration does not affect OTHER networks', () => {
  const storage = createStorage({
    [SETTINGS_STORAGE_KEY]: JSON.stringify({
      rpcUrl: 'https://legacy-rpc.example.com',
      indexerUrl: '',
      requestTimeoutMs: 5000,
    }),
  });

  // Migrate into testnet.
  loadNetworkProfile('testnet', storage);
  // mainnet should still be empty (migration is per-networkId, on-demand).
  assert.deepEqual(loadNetworkProfile('mainnet', storage), emptyProfile());
});

// ── saveNetworkProfile ────────────────────────────────────────────────────────

test('saveNetworkProfile: persists and returns the normalised profile', () => {
  const storage = createStorage();
  const saved = saveNetworkProfile('testnet', { rpcUrl: 'https://rpc.example.com/', indexerUrl: '' }, storage);

  // Trailing slash stripped.
  assert.equal(saved.rpcUrl, 'https://rpc.example.com');

  // Can be loaded back.
  assert.deepEqual(loadNetworkProfile('testnet', storage), saved);
});

test('saveNetworkProfile: writes under NETWORK_PROFILES_KEY, not SETTINGS_STORAGE_KEY', () => {
  const storage = createStorage();
  saveNetworkProfile('testnet', { rpcUrl: 'https://rpc.example.com', indexerUrl: '' }, storage);

  assert.ok(storage.getItem(NETWORK_PROFILES_KEY), 'should write new key');
  assert.equal(storage.getItem(SETTINGS_STORAGE_KEY), null, 'should not touch legacy key');
});

test('saveNetworkProfile: does not overwrite other networks', () => {
  const storage = createStorage();
  saveNetworkProfile('mainnet', { rpcUrl: 'https://mainnet.example.com', indexerUrl: '' }, storage);
  saveNetworkProfile('testnet', { rpcUrl: 'https://testnet.example.com', indexerUrl: '' }, storage);

  assert.equal(loadNetworkProfile('mainnet', storage).rpcUrl, 'https://mainnet.example.com');
  assert.equal(loadNetworkProfile('testnet', storage).rpcUrl, 'https://testnet.example.com');
});

test('saveNetworkProfile: ignores unknown networkId', () => {
  const storage = createStorage();
  saveNetworkProfile('unknown', { rpcUrl: 'https://rpc.example.com', indexerUrl: '' }, storage);
  assert.equal(storage.getItem(NETWORK_PROFILES_KEY), null);
});

test('saveNetworkProfile: no-op when storage is unavailable', () => {
  // Should not throw.
  assert.doesNotThrow(() =>
    saveNetworkProfile('testnet', { rpcUrl: 'https://rpc.example.com', indexerUrl: '' }, null),
  );
});

// ── clearNetworkProfile ───────────────────────────────────────────────────────

test('clearNetworkProfile: removes the stored profile for a network', () => {
  const storage = createStorage();
  saveNetworkProfile('testnet', { rpcUrl: 'https://rpc.example.com', indexerUrl: '' }, storage);
  clearNetworkProfile('testnet', storage);

  assert.deepEqual(loadNetworkProfile('testnet', storage), emptyProfile());
});

test('clearNetworkProfile: does not affect other networks', () => {
  const storage = createStorage();
  saveNetworkProfile('mainnet', { rpcUrl: 'https://mainnet.example.com', indexerUrl: '' }, storage);
  saveNetworkProfile('testnet', { rpcUrl: 'https://testnet.example.com', indexerUrl: '' }, storage);

  clearNetworkProfile('testnet', storage);

  assert.equal(loadNetworkProfile('mainnet', storage).rpcUrl, 'https://mainnet.example.com');
  assert.deepEqual(loadNetworkProfile('testnet', storage), emptyProfile());
});

test('clearNetworkProfile: removes the key entirely when the last profile is deleted', () => {
  const storage = createStorage();
  saveNetworkProfile('testnet', { rpcUrl: 'https://rpc.example.com', indexerUrl: '' }, storage);
  clearNetworkProfile('testnet', storage);

  assert.equal(storage.getItem(NETWORK_PROFILES_KEY), null);
});

test('clearNetworkProfile: is a no-op for a network with no stored profile', () => {
  const storage = createStorage();
  assert.doesNotThrow(() => clearNetworkProfile('testnet', storage));
});

// ── clearAllNetworkProfiles ───────────────────────────────────────────────────

test('clearAllNetworkProfiles: removes all stored profiles', () => {
  const storage = createStorage();
  for (const id of SUPPORTED_NETWORK_IDS) {
    saveNetworkProfile(id, { rpcUrl: `https://${id}.example.com`, indexerUrl: '' }, storage);
  }

  clearAllNetworkProfiles(storage);

  assert.equal(storage.getItem(NETWORK_PROFILES_KEY), null);
  for (const id of SUPPORTED_NETWORK_IDS) {
    assert.deepEqual(loadNetworkProfile(id, storage), emptyProfile());
  }
});

test('clearAllNetworkProfiles: does not affect the legacy settings key', () => {
  const storage = createStorage({
    [SETTINGS_STORAGE_KEY]: JSON.stringify({ rpcUrl: 'https://rpc.example.com', indexerUrl: '', requestTimeoutMs: 5000 }),
  });
  saveNetworkProfile('testnet', { rpcUrl: 'https://rpc.example.com', indexerUrl: '' }, storage);

  clearAllNetworkProfiles(storage);

  assert.ok(storage.getItem(SETTINGS_STORAGE_KEY), 'legacy key should be untouched');
});

// ── loadAllNetworkProfiles ────────────────────────────────────────────────────

test('loadAllNetworkProfiles: returns empty map when nothing is stored', () => {
  assert.deepEqual(loadAllNetworkProfiles(createStorage()), {});
});

test('loadAllNetworkProfiles: returns only networks that have a stored profile', () => {
  const storage = createStorage();
  saveNetworkProfile('mainnet',  { rpcUrl: 'https://mainnet.example.com', indexerUrl: '' }, storage);
  saveNetworkProfile('futurenet', { rpcUrl: 'https://futurenet.example.com', indexerUrl: '' }, storage);

  const all = loadAllNetworkProfiles(storage);

  assert.deepEqual(Object.keys(all).sort(), ['futurenet', 'mainnet']);
  assert.equal(all.mainnet.rpcUrl, 'https://mainnet.example.com');
  assert.equal(all.futurenet.rpcUrl, 'https://futurenet.example.com');
  assert.equal(all.testnet, undefined);
});

test('loadAllNetworkProfiles: ignores unknown keys written by future code', () => {
  // Simulate a future version adding an unsupported key.
  const storage = createStorage({
    [NETWORK_PROFILES_KEY]: JSON.stringify({
      testnet: { rpcUrl: 'https://testnet.example.com', indexerUrl: '' },
      unknownnetwork: { rpcUrl: 'https://unknown.example.com', indexerUrl: '' },
    }),
  });

  const all = loadAllNetworkProfiles(storage);
  assert.equal(Object.keys(all).length, 1);
  assert.ok(all.testnet);
  assert.equal(all.unknownnetwork, undefined);
});

// ── resolveNetworkRpcUrl ──────────────────────────────────────────────────────

test('resolveNetworkRpcUrl: uses custom RPC URL when set', () => {
  const storage = createStorage();
  saveNetworkProfile('testnet', { rpcUrl: 'https://my-rpc.example.com', indexerUrl: '' }, storage);

  const resolved = resolveNetworkRpcUrl('testnet', 'https://soroban-testnet.stellar.org', storage);
  assert.equal(resolved, 'https://my-rpc.example.com');
});

test('resolveNetworkRpcUrl: falls back to built-in URL when profile is blank', () => {
  const storage = createStorage();
  const resolved = resolveNetworkRpcUrl('testnet', 'https://soroban-testnet.stellar.org', storage);
  assert.equal(resolved, 'https://soroban-testnet.stellar.org');
});

test('resolveNetworkRpcUrl: does not leak one network URL into another', () => {
  const storage = createStorage();
  saveNetworkProfile('mainnet', { rpcUrl: 'https://mainnet-custom.example.com', indexerUrl: '' }, storage);

  // testnet has no stored profile — should return the testnet built-in, not mainnet.
  const resolved = resolveNetworkRpcUrl('testnet', 'https://soroban-testnet.stellar.org', storage);
  assert.equal(resolved, 'https://soroban-testnet.stellar.org');
});

test('resolveNetworkRpcUrl: strips trailing slashes from the built-in fallback', () => {
  const storage = createStorage();
  const resolved = resolveNetworkRpcUrl('testnet', 'https://soroban-testnet.stellar.org/', storage);
  assert.equal(resolved, 'https://soroban-testnet.stellar.org');
});

// ── resolveNetworkIndexerUrl ──────────────────────────────────────────────────

test('resolveNetworkIndexerUrl: uses custom indexer URL when set', () => {
  const storage = createStorage();
  saveNetworkProfile('testnet', { rpcUrl: '', indexerUrl: 'https://my-indexer.example.com' }, storage);

  const resolved = resolveNetworkIndexerUrl('testnet', 'https://api.default.example.com', storage);
  assert.equal(resolved, 'https://my-indexer.example.com');
});

test('resolveNetworkIndexerUrl: falls back to built-in when profile indexerUrl is blank', () => {
  const storage = createStorage();
  saveNetworkProfile('testnet', { rpcUrl: 'https://rpc.example.com', indexerUrl: '' }, storage);

  const resolved = resolveNetworkIndexerUrl('testnet', 'https://api.default.example.com', storage);
  assert.equal(resolved, 'https://api.default.example.com');
});

// ── Cross-network isolation end-to-end ───────────────────────────────────────

test('all four networks round-trip their own profiles without interference', () => {
  const storage = createStorage();

  const fixtures = {
    mainnet:   { rpcUrl: 'https://mainnet-rpc.example.com',   indexerUrl: 'https://mainnet-api.example.com' },
    testnet:   { rpcUrl: 'https://testnet-rpc.example.com',   indexerUrl: '' },
    futurenet: { rpcUrl: '',                                   indexerUrl: 'https://futurenet-api.example.com' },
    localhost: { rpcUrl: 'http://localhost:8000/soroban/rpc',  indexerUrl: 'http://localhost:9090' },
  };

  for (const [id, profile] of Object.entries(fixtures)) {
    saveNetworkProfile(id, profile, storage);
  }

  for (const [id, expected] of Object.entries(fixtures)) {
    const loaded = loadNetworkProfile(id, storage);
    assert.equal(loaded.rpcUrl, expected.rpcUrl, `${id} rpcUrl mismatch`);
    assert.equal(loaded.indexerUrl, expected.indexerUrl, `${id} indexerUrl mismatch`);
  }
});
