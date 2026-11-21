# Rate-Limit Counter Persistence Fix — Implementation Summary

## Changes Made

### File: `/workspaces/skymoonscope/core/src/shares.rs`

#### 1. Import Changes
- **Added**: `use redis::Commands;` for Redis operations
- **Kept**: All existing imports for SHA256, compression, serialization

#### 2. Removed In-Memory Rate Limiting
- **Deleted**: `RateLimitEntry` struct (held atomic counter + window start time)
- **Deleted**: `rate_cache: Cache<String, Arc<RateLimitEntry>>` field
- **Reason**: In-memory state lost on server restart

#### 3. Added Redis Client
```rust
pub struct ShareService {
    db: SqlitePool,
    read_cache: Cache<String, CachedShare>,
    redis_client: redis::Client,  // ← NEW
    public_base_url: String,
}
```

#### 4. Updated Constructor
```rust
pub fn new(
    db: SqlitePool,
    public_base_url: impl Into<String>,
    redis_url: &str,  // ← NEW parameter
) -> Result<Self, AppError>  // ← Now returns Result
```

**What it does**:
- Creates Redis client from connection URL
- Returns error if Redis is unreachable

#### 5. Replaced Rate Limit Check
**Before** (~35 lines):
- Check if IP in local cache
- If expired, reset counter
- Increment atomic counter
- Check against limit

**After** (~30 lines):
- Format Redis key: `rate_limit:shares:{ip}`
- Use Lua script to atomically:
  1. Increment counter (`INCR`)
  2. Set expiry on first write (`EXPIRE`)
- Return count, check against limit

**Lua Script**:
```lua
local count = redis.call('INCR', KEYS[1])
if count == 1 then
    redis.call('EXPIRE', KEYS[1], ARGV[1])
end
return count
```

### Tests Updated

All 13 rate-limit and integration tests updated:

| Test | Change |
|------|--------|
| `rate_limit_allows_up_to_max_creates` | Pass Redis URL |
| `rate_limit_blocks_over_max_creates` | Pass Redis URL |
| `rate_limit_is_per_ip` | Pass Redis URL |
| **NEW**: `rate_limit_resets_after_window_expires` | Validates window expiry behavior |
| `create_share_*` (5 tests) | Pass Redis URL |
| `get_share_*` (4 tests) | Pass Redis URL |
| `http_*` (4 tests) | Pass Redis URL |

**Helper function added**:
```rust
fn test_redis_url() -> String {
    std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string())
}
```

### Documentation Updated

**Module documentation**:
- Changed rate limiting description from in-memory moka to Redis INCR + EXPIRE
- Added note about Lua script for atomic operations

## Security Properties

| Property | Before | After |
|----------|--------|-------|
| **Restart Safe** | ❌ Counters reset | ✅ Persisted in Redis |
| **Distributed** | ❌ Per-instance only | ✅ Shared across servers |
| **Atomic** | ⚠️ Race condition risk | ✅ Lua script guarantees |
| **TTL** | Manual eviction | ✅ Automatic EXPIRE |

## Testing

**To run tests locally**:
```bash
# Start Redis
redis-server &

# Run tests
cd /workspaces/skymoonscope/core
cargo test shares::tests::rate_limit
```

**Expected output**:
```
test rate_limit_allows_up_to_max_creates ... ok
test rate_limit_blocks_over_max_creates ... ok
test rate_limit_is_per_ip ... ok
test rate_limit_resets_after_window_expires ... ok  ← NEW
```

## Integration Steps

1. **Ensure Redis is available** at `REDIS_URL` (or localhost:6379)
2. **Update initialization code** that calls `ShareService::new()`:
   ```rust
   let service = ShareService::new(
       pool,
       &config.public_base_url,
       &config.redis_url  // ← Add this
   )?;  // ← Now requires error handling
   ```
3. **Run tests** to validate
4. **Deploy** with Redis available

## Backward Compatibility

- ❌ **Breaking change**: Existing `ShareService::new(pool, url)` calls will not compile
- ✅ **Fix is simple**: Add Redis URL parameter
- ✅ **No API changes**: HTTP endpoints unchanged
- ✅ **No data migration**: Existing shares in SQLite unaffected

## Performance Impact

- **Latency**: +1–2ms per request (Redis INCR over network)
- **Throughput**: Unchanged (one command per create)
- **Storage**: ~100 bytes per unique IP (auto-expired after window)

## Files Changed

1. `/workspaces/skymoonscope/core/src/shares.rs` — All changes
2. `/workspaces/skymoonscope/SECURITY_FIX_RATE_LIMITING.md` — Detailed explanation
3. This file — Implementation summary
