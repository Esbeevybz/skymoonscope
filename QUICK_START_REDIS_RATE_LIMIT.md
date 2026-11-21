# Quick Start: Redis-Based Rate Limiting

## TL;DR

The rate-limit counter is now persistent in Redis instead of in-memory. This fixes a security vulnerability where attackers could bypass rate limits by timing server restarts.

## What Changed

| Aspect | Before | After |
|--------|--------|-------|
| Storage | Moka in-memory cache | Redis |
| Persistence | Lost on restart ❌ | Survives restart ✅ |
| Distribution | Per-instance only | Shared across servers ✅ |
| Constructor | `new(pool, url)` | `new(pool, url, redis_url)?` |

## For Developers

### Update Code

**Before**:
```rust
let service = ShareService::new(pool, "https://app.example.com");
```

**After**:
```rust
let service = ShareService::new(
    pool,
    "https://app.example.com",
    &config.redis_url,
)?;  // Now returns Result
```

### Run Tests

```bash
# Start Redis (if not running)
redis-server &

# Run rate-limit tests
cargo test shares::tests::rate_limit -- --nocapture

# Run all shares tests
cargo test shares::tests
```

## For DevOps

### Prerequisites

Redis must be running:
```bash
# Local (dev)
redis-server

# Docker
docker run -d -p 6379:6379 redis:latest

# Production
# Ensure Redis is accessible at REDIS_URL
```

### Environment Variables

```bash
# Default (localhost)
export REDIS_URL="redis://127.0.0.1:6379"

# Custom host
export REDIS_URL="redis://redis-prod.example.com:6379"

# With auth
export REDIS_URL="redis://user:password@redis.example.com:6379"
```

### Verification

```bash
# Check Redis connectivity
redis-cli ping
# Output: PONG

# Check rate-limit keys in Redis
redis-cli KEYS "rate_limit:shares:*"
# Output: (empty list or keys like: rate_limit:shares:192.168.1.1)

# Check TTL on a rate-limit key
redis-cli TTL "rate_limit:shares:192.168.1.1"
# Output: 45 (seconds remaining)
```

## How It Works

```
User makes request
    ↓
extract_client_ip("192.168.1.1")
    ↓
check_rate_limit("192.168.1.1")
    ↓
Redis Lua Script:
  1. INCR rate_limit:shares:192.168.1.1  → returns 1, 2, 3, ...
  2. If count == 1: EXPIRE rate_limit:shares:192.168.1.1 60
  3. Return count
    ↓
Check count <= 10?
    ├─ YES → Allow request
    └─ NO → Return 429 Too Many Requests
```

## Troubleshooting

### Problem: "Redis connection error"

**Symptom**: All create_share requests fail with 500 Internal Server Error

**Check**:
1. Is Redis running? `redis-cli ping`
2. Is Redis URL correct? Check `REDIS_URL` env var
3. Is the server reachable? Try `telnet redis-host 6379`

**Fix**:
```bash
# Start Redis
redis-server &

# Or use Docker
docker run -d -p 6379:6379 redis:latest
```

### Problem: "Rate limit never resets"

**Symptom**: After 10 requests, always get 429, even after waiting

**Cause**: Possible Redis key TTL not set (shouldn't happen with new code)

**Check**:
```bash
redis-cli KEYS "rate_limit:shares:*"
redis-cli TTL "rate_limit:shares:192.168.1.1"  # Should be > 0
```

**Fix**: This shouldn't happen. If it does, restart Redis:
```bash
redis-cli FLUSHDB  # Clear all keys (dev only)
# Then restart your app
```

### Problem: Tests fail with "redis://127.0.0.1:6379 connection refused"

**Cause**: Redis not running during tests

**Fix**:
```bash
# Start Redis in background
redis-server &

# Or run Redis in another terminal
# Then run tests
cargo test shares::tests
```

## Monitoring

### Redis Memory Usage

```bash
redis-cli INFO memory
# Look for: used_memory_human

# Should be minimal (< 1MB for rate limits alone)
```

### Rate Limit Keys

```bash
# Count active rate-limit entries
redis-cli --scan --pattern "rate_limit:shares:*" | wc -l

# Delete all rate-limit entries (careful!)
redis-cli --scan --pattern "rate_limit:shares:*" | \
  xargs redis-cli DEL
```

### Monitoring with Redis CLI

```bash
# Watch rate-limit activity
redis-cli MONITOR | grep rate_limit

# Real-time stats
redis-cli --stat
```

## Security Notes

### Network
- If Redis is remote, use TLS or a private network
- Never expose Redis on the public internet

### Authentication
- If Redis requires auth, add credentials to URL:
  ```
  redis://username:password@host:6379
  ```

### Persistence
- For production, Redis should be backed up (RDB or AOF)
- Rate-limit data can be lost without affecting functionality (will re-count on next window)

## Performance

- **Per-request overhead**: 1-2ms (local) to 10-50ms (remote)
- **Negligible** for POST /api/shares (already 100-500ms due to compression)
- **No scaling issues** up to 1000 unique IPs

## Rollback (if needed)

If you need to disable Redis-based rate limiting:

1. Use a fallback: Create an in-memory rate limiter
2. Or disable rate limiting temporarily (not recommended)
3. Contact the security team

```rust
// Emergency fallback (not secure, for testing only)
let redis_url = "redis://localhost:6379";  // Fake/disabled endpoint
let service = ShareService::new(pool, url, redis_url)?;
// This will fail and prevent share creation (safe failure)
```

## References

- [Redis INCR](https://redis.io/commands/incr/)
- [Redis EXPIRE](https://redis.io/commands/expire/)
- [Redis Lua Scripting](https://redis.io/docs/latest/develop/interact/programmability/eval-intro/)
- [Redis CLI](https://redis.io/docs/manual/cli/)
