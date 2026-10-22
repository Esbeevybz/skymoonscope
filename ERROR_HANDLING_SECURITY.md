# Error Handling Security Implementation

## Problem Statement

Previously, the Sky Moon Scope API could expose sensitive internal information in JSON API responses through `{:?}` Debug formatting. This included:
- Full stack traces
- Internal type names and module paths
- File paths and line numbers
- System-level error details

This information leakage could aid attackers in reconnaissance and vulnerability discovery.

## Resolution

The implementation maps all internal (5xx) errors to opaque user-facing messages while preserving full Debug details for server-side logging only. This follows the RFC 7807 "Problem Details for HTTP APIs" standard.

### Architecture

#### 1. Error Response Sanitization (`core/src/errors.rs`)

The `AppError::IntoResponse` trait implementation now:

- **For 5xx (Server) Errors:**
  - Logs full Debug details server-side using structured tracing
  - Returns a generic opaque message to clients: *"An internal server error occurred. Please try again later."*
  - Preserves HTTP status codes, problem type URIs, and titles for client-side error categorization

- **For 4xx (Client) Errors:**
  - Returns the original error message as-is (user-controlled input validation)
  - These typically reflect invalid parameters, bad auth, etc., which are safe to expose

Example 5xx response to client:
```json
{
  "type": "https://Sky Moon Scope.dev/errors/internal-server-error",
  "title": "Internal Server Error",
  "status": 500,
  "detail": "An internal server error occurred. Please try again later.",
  "instance": null
}
```

#### 2. Error Middleware (`core/src/error_middleware.rs`)

Provides middleware utilities for error handling:

- **`error_sanitization_middleware`:** Intercepts responses and ensures no 5xx responses leak details
- **`log_error_chain`:** Utility to extract and log the full error chain with context for debugging

Usage in Axum:
```rust
.layer(middleware::from_fn(error_middleware::error_sanitization_middleware))
```

#### 3. Server-Side Logging

When a 5xx error occurs, the error handler logs full context:

```
error(
    error_type = "internal-server-error",
    status = 500,
    error_message = "...",
    error_debug = "...",
    "Server error response: ..."
)
```

This is controlled by the `RUST_LOG` environment variable and never exposed to clients.

### Key Features

1. **RFC 7807 Compliance:** Error responses follow the Problem Details standard for consistency with API consumers
2. **Structured Logging:** Errors are logged with full context (error type, status, debug info) for investigation
3. **Client/Server Separation:** Client receives minimal info; server logs contain full details
4. **Backward Compatible:** 4xx error messages remain unchanged; only 5xx errors are sanitized
5. **Request Correlation:** Works with `x-request-id` header for tracing errors across logs

### Deployment & Configuration

No deployment changes required. The middleware is automatically applied to all routes. Error visibility is controlled via `RUST_LOG`:

```bash
# Production: show only errors
RUST_LOG=error cargo run -p sky-moon-scope-core

# Development: show all details
RUST_LOG=debug cargo run -p sky-moon-scope-core
```

### Testing

To verify the implementation:

1. **Trigger a 5xx error** (e.g., network failure during simulation):
   - Client receives: `{"detail": "An internal server error occurred..."}`
   - Server logs include: Full error chain with debug info

2. **Trigger a 4xx error** (e.g., invalid base64 WASM):
   - Client receives: `{"detail": "Invalid base64 WASM data: ..."}` (detailed message)
   - Server logs include: Same error info

3. **Check logs** (when `RUST_LOG=debug`):
   ```
   error error_type=internal-server-error status=500 error_message="..." Server error response: ...
   ```

### Migration Notes

This change is **non-breaking** for:
- Error response format (still RFC 7807 compliant)
- HTTP status codes (unchanged)
- Authentication/authorization flows

Clients that rely on specific error message text from 5xx responses will need to update to handle the generic message. 4xx errors remain detailed and stable.

### Future Improvements

1. **Error Request IDs:** Attach unique error IDs to responses for support escalation
2. **Metric Aggregation:** Track error frequencies by type and handler
3. **Alert Integration:** Auto-escalate critical error patterns
4. **Error Classification:** Automatically categorize errors (transient, permanent, user, system)

## Files Modified

- `core/src/errors.rs`: Enhanced `IntoResponse` to sanitize 5xx details
- `core/src/error_middleware.rs` (NEW): Error handling middleware module
- `core/src/lib.rs`: Added `error_middleware` module export
- `core/src/main.rs`: Integrated error middleware into router stack

## References

- [RFC 7807: Problem Details for HTTP APIs](https://tools.ietf.org/html/rfc7807)
- [OWASP: Information Disclosure](https://owasp.org/www-project-web-security-testing-guide/latest/4-Web_Application_Security_Testing/01-Information_Gathering/02-Fingerprint_Web_Framework.html)
