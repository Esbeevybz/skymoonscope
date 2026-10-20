# Simulation Cache Implementation Summary

## Overview
This document summarizes the implementation of a network-aware caching layer to avoid redundant simulations by hashing simulation requests and checking disk cache before invoking the simulation engine.

## Problem Statement
Previously, submitting the same contract + function + arguments would re-run the full simulation, wasting CPU and RPC quota. This occurred even when simulating the same request across different Stellar networks (testnet, mainnet, etc.).

## Solution
Implemented network-aware cache key generation that includes the network passphrase in the hash, ensuring that simulations on different networks are cached separately while avoiding redundant simulations on the same network.

## Changes Made

### 1. Cache Key Generation (`/core/src/cache/mod.rs`)

#### Added new method: `generate_key_with_network`
- **Location**: `SimulationCache::generate_key_with_network()`
- **Signature**: `pub fn generate_key_with_network(contract_id: &str, function_name: &str, args: &[String], network: &str) -> String`
- **Implementation**: Creates SHA256 hash from `contract_id + function_name + args_json + network`
- **Backward Compatibility**: Original `generate_key()` method retained for backward compatibility (does not include network)

### 2. Application State (`/core/src/main.rs`)

#### Updated AppState struct
- **Field Added**: `network_passphrase: String`
- **Purpose**: Store the Stellar network passphrase for cache key generation
- **Location**: `pub struct AppState` (line ~491-513)

#### Updated AppState instantiation (production)
- **Location**: Line ~2695-2715 in main.rs
- **Change**: Added `network_passphrase: config.network_passphrase.clone()` to AppState initialization

#### Updated AppState instantiation (tests)
- **Location**: Line ~3098-3113 in main.rs
- **Change**: Created complete AppState with all required fields for test setup
- **Network**: Set to `"Test SDF Network ; September 2015"` for testing

### 3. /analyze Endpoint Integration (`/core/src/main.rs`)

#### Updated cache key generation
- **Location**: Line ~1024-1031 in main.rs
- **Change**: Modified to use `SimulationCache::generate_key_with_network()` instead of `generate_key()`
- **Parameters**: Now includes `&state.network_passphrase` in the key generation

#### Cache Flow
1. Request arrives at `/analyze` endpoint with contract_id, function_name, args, and optional network-specific params
2. Network passphrase is retrieved from AppState
3. Network-aware cache key is generated using all parameters including network
4. Cache lookup is performed - if hit, return cached SimulationResult
5. On miss, simulation engine is invoked to run the full simulation
6. Result is stored in cache with the network-aware key
7. Response includes cache status header (`x-sky-moon-scope-cache: HIT` or `MISS`)

### 4. Test Suite (`/core/src/cache/mod.rs`)

#### Added 5 new cache key generation tests

1. **`test_cache_key_generation_is_deterministic`**
   - Verifies that the same inputs produce the same cache key
   - Ensures consistency for cache lookups

2. **`test_cache_key_includes_all_parameters`**
   - Verifies that different contract IDs produce different keys
   - Verifies that different function names produce different keys
   - Verifies that different arguments produce different keys
   - Ensures all parameters are included in the hash

3. **`test_cache_key_with_network_differentiates_networks`**
   - Verifies that same contract/function/args on testnet vs mainnet produce different keys
   - Ensures network isolation in cache

4. **`test_cache_key_with_network_is_deterministic`**
   - Verifies that the same inputs with the same network produce the same key
   - Ensures consistency across invocations

## Files Modified

1. **`/core/src/cache/mod.rs`**
   - Added `generate_key_with_network()` method
   - Added 5 new test functions for cache key generation

2. **`/core/src/main.rs`**
   - Updated `AppState` struct to include `network_passphrase`
   - Updated AppState instantiation in main (production)
   - Updated AppState instantiation in tests
   - Updated `/analyze` endpoint to use network-aware cache keys

## Technical Details

### Cache Key Generation Algorithm
```
input = contract_id + function_name + args_json + network
hash = SHA256(input)
cache_key = hex(hash)
```

### Network Isolation Example
- **Testnet Request**: Contract `CDLZ...`, function `transfer`, args `["arg1"]`
  - Cache Key: `sha256("CDLZ...transferarg1Test SDF Network ; September 2015")`
  
- **Mainnet Request**: Same contract, function, args
  - Cache Key: `sha256("CDLZ...transferarg1Public Global Stellar Network ; September 2015")`
  
- **Result**: Different cache keys ensure separate caching per network

## Benefits

1. **Eliminates Redundant Simulations**: Same contract/function/args on the same network uses cached result
2. **Network Isolation**: Different networks maintain separate caches
3. **RPC Quota Savings**: Reduced calls to Stellar RPC endpoints
4. **CPU Efficiency**: Expensive simulation operations are cached
5. **Backward Compatible**: Original `generate_key()` still available
6. **Testable**: Comprehensive test coverage for cache key generation

## Testing Strategy

### Unit Tests
- Test deterministic key generation
- Test that all parameters affect the key
- Test network isolation
- All tests located in `/core/src/cache/mod.rs`

### Integration Tests
The following should be tested when running the full test suite:
- Cache hit/miss scenarios in `/analyze` endpoint
- Response headers include correct cache status
- Network isolation works across multiple environments

### Manual Testing
```bash
# Test on testnet
curl -X POST http://localhost:8080/analyze \
  -H "Content-Type: application/json" \
  -d '{
    "contract_id": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
    "function_name": "transfer",
    "args": ["recipient", "100"]
  }'

# Response header should show:
# x-sky-moon-scope-cache: MISS (first request)
# x-sky-moon-scope-cache: HIT (second request with same params)

# With different network config, same request produces:
# x-sky-moon-scope-cache: MISS (different network = different cache key)
```

## Verification Checklist

- [x] `generate_key_with_network()` method added to SimulationCache
- [x] AppState struct includes `network_passphrase` field
- [x] AppState instantiation updated (production code)
- [x] AppState instantiation updated (test code)
- [x] /analyze endpoint uses `generate_key_with_network()`
- [x] Cache key generation tests added
- [x] Network isolation test added
- [x] Deterministic key generation test added
- [x] All parameters affect cache key test added
- [ ] Full test suite passes (blocked: Rust toolchain unavailable)

## Next Steps

1. Run full test suite to verify no regressions:
   ```bash
   cd core
   cargo test --lib cache::
   cargo test --lib main::
   ```

2. Run integration tests:
   ```bash
   cd core
   cargo test --test '*'
   ```

3. Manual testing on deployed environment:
   - Test on testnet with multiple requests
   - Verify cache hits are recorded
   - Test network switching produces separate cache entries

## Notes

- The implementation uses SHA256 for cache key generation, consistent with existing code
- Network passphrase is stored in AppState from configuration
- Cache key generation is deterministic and thread-safe
- The disk cache (sled) backend already supports the implementation
- Response headers (`x-sky-moon-scope-cache`) already integrated in existing code

## Related Issues

This implementation addresses the problem of redundant simulations causing unnecessary CPU and RPC quota usage.
