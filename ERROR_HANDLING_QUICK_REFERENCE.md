# Error Handling - Quick Reference

## TL;DR

**Problem:** Error responses were leaking internal details (stack traces, type names, file paths).

**Solution:** Map 5xx errors to opaque messages; log full details server-side.

**Result:** Secure API responses + detailed server logs.

---

## Client Error Response Changes

### 5xx (Server) Errors - NOW SANITIZED ✅

**Before:**
```
HTTP 500
{
  "detail": "SerializationError(serde_json::Error { ... at line 42 column 15 ... })"
}
```

**After:**
```
HTTP 500
{
  "detail": "An internal server error occurred. Please try again later."
}
```

**For Error Handling, Use:**
```json
{
  "type": "https://Sky Moon Scope.dev/errors/internal-server-error",
  "title": "Internal Server Error",
  "status": 500
}
```

### 4xx (Client) Errors - UNCHANGED ✅

```
HTTP 400
{
  "detail": "Invalid base64 WASM data: invalid padding"
}
```

Detailed messages still shown because they reflect user-controlled input.

---

## Server-Side Logging - NOW DETAILED ✅

With `RUST_LOG=debug`:

```
error error_type="internal-server-error" status=500 
  error_message="SerializationError(...)" 
  error_debug="SerializationError(Error { ... })" 
  "Server error response: SerializationError(...)"
```

---

## What's NOT Exposed in Responses

- ❌ Stack traces
- ❌ Type names (`SerializationError`, `reqwest::Error`)
- ❌ File paths (`/workspaces/skymoonscope/...`)
- ❌ Line numbers
- ❌ Database details
- ❌ System hostnames
- ❌ Internal URLs

---

## What You CAN Still See

### In API Response
```json
{
  "type": "https://Sky Moon Scope.dev/errors/internal-server-error",
  "title": "Internal Server Error",
  "status": 500,
  "detail": "An internal server error occurred. Please try again later."
}
```

### In Server Logs
```
Full error chain with debug info (controlled by RUST_LOG)
```

### Via Correlation
```
x-request-id: 550e8400-e29b-41d4-a716-446655440000
```
Use this ID to find matching logs.

---

## Configuration

### See Full Errors (Development)
```bash
RUST_LOG=debug cargo run -p sky-moon-scope-core
```

### Production (Default)
```bash
RUST_LOG=error cargo run -p sky-moon-scope-core
```

**Note:** Error sanitization is always active; `RUST_LOG` only controls logging verbosity.

---

## Client Integration

### Old Pattern (Don't Use)
```javascript
if (error.detail.includes("RpcRequestFailed")) {
  // Won't work - 5xx details are now generic
}
```

### New Pattern (Use This)
```javascript
if (error.type.includes("internal-server-error")) {
  console.log("Server error occurred");
  // Use x-request-id for support
  reportError(error.type, response.headers['x-request-id']);
}
```

---

## Testing Quick Checks

### Verify 5xx Sanitization
```bash
# Trigger error (e.g., invalid RPC URL)
curl -X POST http://localhost:8080/analyze \
  -d '{"contract_id":"test"}'

# Should see generic message, NOT full error stack
```

### Verify 4xx Detail
```bash
# Trigger validation error
curl -X POST http://localhost:8080/analyze \
  -d '{"contract_id":""}'

# Should see detailed message about empty contract_id
```

### Check Server Logs
```bash
# Should see full error chain
tail -f /var/log/sky-moon-scope.log | grep error
```

---

## Files Changed

```
core/src/
├── error_middleware.rs       (NEW)
├── errors.rs                 (MODIFIED)
├── lib.rs                    (MODIFIED)
└── main.rs                   (MODIFIED)

docs/
├── ERROR_HANDLING_SECURITY.md
├── EXAMPLES_ERROR_HANDLING.md
├── SECURITY_IMPROVEMENT_ERROR_HANDLING.md
└── IMPLEMENTATION_CHECKLIST.md
```

---

## Support

**Q: Why can't I see error details?**
A: This is intentional for 5xx errors. Check server logs with `RUST_LOG=debug`.

**Q: How do I debug in production?**
A: 
1. Note the `x-request-id` header
2. Query logs: `grep "x-request-id=<value>" /var/log/...`
3. Look for `error_type="internal-server-error"` entries

**Q: Can my client code break?**
A: Only if parsing 5xx `detail` fields for specific text. Use `type`/`title` fields instead.

**Q: Is this a breaking change?**
A: Only for 5xx errors. 4xx errors and API structure unchanged.

---

## Status

✅ **Implemented and Ready**

- Code: `/workspaces/skymoonscope/core/src/`
- Docs: `/workspaces/skymoonscope/`
- Tests: See `EXAMPLES_ERROR_HANDLING.md`

---

**For full details, see:**
- `ERROR_HANDLING_SECURITY.md` - Technical architecture
- `EXAMPLES_ERROR_HANDLING.md` - Code examples & testing
- `SECURITY_IMPROVEMENT_ERROR_HANDLING.md` - Deployment & monitoring
