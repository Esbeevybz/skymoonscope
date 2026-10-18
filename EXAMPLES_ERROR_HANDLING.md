# Error Handling Examples

This document provides concrete examples of how the new error handling system works across different scenarios.

## Before & After Comparison

### Scenario 1: Network Timeout During RPC Call (5xx Error)

**Before:**
```json
{
  "type": "https://Sky Moon Scope.dev/errors/internal-server-error",
  "title": "Internal Server Error",
  "status": 500,
  "detail": "RPC request failed: reqwest::error::Error { kind: Request, source: Some(hyper::error::Error { kind: Connect, source: Some(std::io::error::Error { kind: TimedOut, message: \"Connection timed out\", ... }) }) }",
  "instance": null
}
```

**After:**
```json
{
  "type": "https://Sky Moon Scope.dev/errors/internal-server-error",
  "title": "Internal Server Error",
  "status": 500,
  "detail": "An internal server error occurred. Please try again later.",
  "instance": null
}
```

**Server Log (visible with RUST_LOG=debug):**
```
error error_type="internal-server-error" status=500 error_message="RPC request failed: ..." error_debug="RpcRequestFailed(...)" "Server error response: RPC request failed: ..."
```

### Scenario 2: Invalid Base64 WASM Data (4xx Error)

**Before & After (Unchanged):**
```json
{
  "type": "https://Sky Moon Scope.dev/errors/bad-request",
  "title": "Bad Request",
  "status": 400,
  "detail": "Invalid base64 WASM data: invalid padding",
  "instance": null
}
```

Client still receives detailed validation error because this is user-controlled input.

### Scenario 3: Database Connection Pool Exhausted (5xx Error)

**Before:**
```json
{
  "type": "https://Sky Moon Scope.dev/errors/internal-server-error",
  "title": "Internal Server Error",
  "status": 500,
  "detail": "redis error: Connection { host: \"redis.internal.prod.k8s.local:6379\", db: 0 } - refused on timeout after 30s",
  "instance": null
}
```

**After:**
```json
{
  "type": "https://Sky Moon Scope.dev/errors/internal-server-error",
  "title": "Internal Server Error",
  "status": 500,
  "detail": "An internal server error occurred. Please try again later.",
  "instance": null
}
```

**Server Log:**
```
error error_type="internal-server-error" status=500 error_message="redis error: ..." "Server error response: redis error: Connection refused"
```

## Implementation Details

### Error Flow Diagram

```
Handler raises error
       ↓
  AppError enum
       ↓
  IntoResponse trait
       ├─→ Is 5xx?
       │    ├─→ YES: Log full details, return generic message
       │    └─→ NO: Return original error message
       ↓
  HTTP Response with ErrorResponse body
```

### Code Example: Handler with Proper Error Handling

```rust
// In your handler
#[post("/analyze")]
async fn analyze(
    State(state): State<AppState>,
    Json(payload): Json<AnalysisRequest>,
) -> Result<Json<AnalysisResult>, AppError> {
    // User-controlled validation - safe to expose
    if payload.contract_id.is_empty() {
        return Err(AppError::BadRequest(
            "contract_id cannot be empty".to_string()
        ));
    }

    // Internal operation - will be sanitized if it fails
    state.simulation_engine
        .simulate(&payload)
        .await
        .map(Json)
        .map_err(|e| {
            // This is converted to AppError via From trait
            // If it's a 5xx, the client won't see the details
            AppError::Internal(format!("Simulation failed: {}", e))
        })
}
```

### Error Logging Best Practices

When raising an error, ensure you log relevant context:

```rust
use tracing::{error, warn, info};

// Option 1: Using structured logging (recommended)
error!(
    contract_id = %payload.contract_id,
    simulation_params = ?payload.params,
    "Simulation failed for contract"
);
return Err(AppError::Internal("Simulation failed".to_string()));

// Option 2: Using error_middleware utility
use crate::error_middleware::log_error_chain;
match simulation_result {
    Err(e) => {
        log_error_chain(&e, "Simulating contract execution");
        return Err(AppError::Internal("Simulation failed".to_string()));
    }
    Ok(result) => Ok(Json(result)),
}
```

## Testing the Error Handling

### Manual Testing

1. **Trigger a 5xx error with invalid RPC configuration:**
   ```bash
   SOROBAN_RPC_URL="http://invalid.local:8000" \
   RUST_LOG=error \
   cargo run -p sky-moon-scope-core
   
   # Then call any endpoint that requires RPC
   curl -X POST http://localhost:8080/analyze \
     -H "Content-Type: application/json" \
     -d '{"contract_id":"...", "function":"..."}'
   ```

2. **Check server logs for full error details:**
   ```bash
   # With RUST_LOG=debug, you'll see:
   error error_type="internal-server-error" status=500 ...
   ```

3. **Verify client receives generic message:**
   ```bash
   # Response will contain:
   {"detail": "An internal server error occurred. Please try again later."}
   ```

### Automated Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_5xx_error_sanitization() {
        // Create an internal error
        let error = AppError::Internal(
            "sensitive database connection string".to_string()
        );

        // Convert to response
        let response = error.into_response();
        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Verify client receives generic message
        assert_eq!(
            json["detail"],
            "An internal server error occurred. Please try again later."
        );
    }

    #[tokio::test]
    async fn test_4xx_error_preserved() {
        let error = AppError::BadRequest("invalid contract id".to_string());
        let response = error.into_response();
        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Verify client receives original message for 4xx
        assert!(json["detail"].as_str().unwrap().contains("invalid contract id"));
    }
}
```

## Monitoring & Alerting

### Recommended Log Queries

**Find all 5xx errors:**
```
error_type="internal-server-error" status=500
```

**Find specific error types:**
```
error_type="internal-server-error" AND (
  error_message CONTAINS "timeout" OR
  error_message CONTAINS "connection refused"
)
```

**Track error frequency:**
```
status=500 | stats count by error_type, endpoint
```

### Metric Collection

Consider adding metrics for:

```rust
// Count by error type
metrics.record(
    "errors_total",
    1,
    &[KeyValue::new("error_type", error_type)],
);

// Track response time by status
let duration = start.elapsed();
metrics.record(
    "request_duration_ms",
    duration.as_millis() as u64,
    &[KeyValue::new("status", status.as_str())],
);
```

## Security Implications

### Information Not Exposed to Clients

- ❌ Stack traces
- ❌ File paths (`/workspaces/skymoonscope/core/src/...`)
- ❌ Module/type names (`reqwest::error::Error`)
- ❌ System configurations (hostnames, ports)
- ❌ Database details
- ❌ Internal service URLs

### Information Still Safe in Logs

- ✅ Full error chains with Debug info
- ✅ Request context (contract IDs, endpoints)
- ✅ System diagnostics (memory, CPU)
- ✅ Operational details (timeouts, retries)

The server logs remain accessible only to administrators with appropriate access controls.

## Troubleshooting

### Issue: Client Can't Determine What Went Wrong

**Solution:** Use the `type` and `title` fields in the error response:
- `type`: URI reference (e.g., `https://Sky Moon Scope.dev/errors/internal-server-error`)
- `title`: Human-readable summary (e.g., `Internal Server Error`)

These provide enough context for basic error handling without exposing implementation details.

### Issue: Can't Debug Errors in Production

**Solution:** 
1. Set `RUST_LOG=error` or `RUST_LOG=debug` as needed
2. Implement error tracking with unique error IDs (future enhancement)
3. Query logs using `x-request-id` correlation header
4. Use structured logging with additional context fields

### Issue: 4xx Errors Are Still Too Detailed

**Solution:** For validation errors you want to hide:
```rust
// Don't expose internal validation details:
Err(AppError::BadRequest(
    "Invalid request parameters".to_string()  // Generic
))

// Instead of:
Err(AppError::BadRequest(
    format!("Field '{}' must match regex: {}", field, regex)  // Specific
))
```
