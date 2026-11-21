# Panic Logging Verification Guide

## Files Modified

### Primary Changes
- **File**: `core/src/error_middleware.rs`
- **Changes**: 
  - Added panic detection via `tokio::spawn` task wrapping
  - Added `log_panic_string()` function to log panic with backtrace
  - Added `log_panic_unknown()` for unrecognizable panic types
  - Added `emit_panic_alarm_string()` to emit SysAlarm events
  - Added `emit_panic_alarm_unknown()` for unknown panic types
  - Updated `error_sanitization_middleware()` to catch and handle panics

### Integration Points
- Uses existing `sys_alarms` module (no changes needed)
- Uses existing tracing infrastructure (no changes needed)
- Registered in `main.rs` via middleware layer (no changes needed)

## Code Review Checklist

- [x] Panic detection via `tokio::spawn` task
- [x] Panic payload extraction (String and &str)
- [x] Backtrace capture via `std::backtrace::Backtrace`
- [x] ERROR level logging with structured fields
- [x] SysAlarm event emission
- [x] Webhook integration via existing `emit_sys_alarm`
- [x] Graceful degradation when webhook unavailable
- [x] HTTP 500 response returned (consistent with old behavior)
- [x] Opaque error messages to clients
- [x] Unit tests for logging and alarm functions
- [x] Documentation in module comments

## Testing Panic Logging

### Unit Tests

```bash
# Run all error_middleware tests
cargo test --lib error_middleware

# Run specific test
cargo test --lib error_middleware::tests::test_panic_logging_with_string_panic

# Run with backtrace
RUST_BACKTRACE=1 cargo test --lib error_middleware
```

**Expected Output**:
```
running 4 tests
test error_middleware::tests::test_error_chain_logging ... ok
test error_middleware::tests::test_panic_logging_with_string_panic ... ok
test error_middleware::tests::test_panic_alarm_emission ... ok
test error_middleware::tests::test_log_panic_unknown ... ok

test result: ok. 4 passed; 0 failed; 0 ignored
```

### Integration Testing

#### Test 1: Create Panic Endpoint

**File**: `core/src/handlers/test_handlers.rs`

```rust
use axum::response::IntoResponse;

#[get("/test-panic")]
pub async fn test_panic_endpoint() -> impl IntoResponse {
    panic!("Test panic for middleware verification");
}
```

**Register in router**:
```rust
// In main.rs router setup
.route("/test-panic", get(test_panic_endpoint))
```

#### Test 2: Trigger Panic

```bash
# Start server
cargo run --release

# In another terminal
curl -v http://localhost:3000/test-panic
```

**Expected Response**:
```
< HTTP/1.1 500 Internal Server Error
< content-type: text/plain; charset=utf-8
< content-length: 21

Internal server error
```

#### Test 3: Verify Panic Logged

**Check structured logs**:
```bash
# Grep for panic logs (adjust for your logging output format)
grep "Handler panic caught" /var/log/app.log

# Expected output:
# timestamp=... level=ERROR message="Handler panic caught: Test panic for middleware verification" panic_message="Test panic for middleware verification" backtrace="..." target=sky_moon_scope_core::error_middleware
```

#### Test 4: Verify SysAlarm Emitted

**Setup webhook receiver**:
```bash
# Start simple HTTP listener (requires netcat or similar)
nc -l -p 9999

# Or use Python
python3 -c "
from http.server import HTTPServer, BaseHTTPRequestHandler
import json

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        content_length = int(self.headers['Content-Length'])
        body = self.rfile.read(content_length)
        print('Received:', json.loads(body))
        self.send_response(200)
        self.end_headers()

HTTPServer(('localhost', 9999), Handler).serve_forever()
"
```

**Configure webhook URL**:
```bash
export Sky Moon Scope_ALARM_WEBHOOK_URL="http://localhost:9999/alarm"
```

**Restart server and trigger panic**:
```bash
# Server should POST to webhook
# Check webhook receiver output for:
{
  "event": "handler_panic",
  "resource": "request_handler",
  "pid": <process_id>,
  "node_id": <hostname>,
  "timestamp": "2024-11-21T..."
}
```

## Functional Requirements Verification

### Requirement 1: Catch Panics
✓ Panics in handlers are caught by middleware  
✓ Middleware detects panic via `is_panic()`  
✓ Panic prevents handler from completing  
✓ Middleware intercepts before response sent

**Verification**:
- Trigger panic endpoint → receive HTTP 500 (not crash)
- Server continues accepting requests after panic

### Requirement 2: Log Panic Payload
✓ Panic message extracted from payload  
✓ Message logged at ERROR level  
✓ Message included in structured fields  
✓ Both String and &str types supported

**Verification**:
```bash
# Check logs contain panic_message field
grep panic_message /var/log/app.log | head -1
# Should show: panic_message="..."
```

### Requirement 3: Log Backtrace
✓ Backtrace captured via `Backtrace::capture()`  
✓ Backtrace included in structured logs  
✓ Backtrace formatted for readability  
✓ Backtrace available for debugging

**Verification**:
```bash
export RUST_BACKTRACE=1
# Trigger panic
curl http://localhost:3000/test-panic
# Check logs for backtrace field
grep -A 10 "Handler panic caught" /var/log/app.log
```

### Requirement 4: Emit SysAlarm
✓ SysAlarm event created with handler_panic event type  
✓ Event includes required fields (pid, node_id, timestamp)  
✓ Event posted to webhook if configured  
✓ Gracefully degrades to log-only if webhook unavailable

**Verification**:
```bash
# With webhook configured
export Sky Moon Scope_ALARM_WEBHOOK_URL="http://localhost:9999"
curl http://localhost:3000/test-panic
# Should see POST to webhook

# Without webhook configured
unset Sky Moon Scope_ALARM_WEBHOOK_URL
curl http://localhost:3000/test-panic
# Should only see log entry, no webhook error
```

### Requirement 5: Return HTTP 500
✓ HTTP 500 status returned for panics  
✓ Response contains opaque message  
✓ No stack traces exposed to client  
✓ No internal implementation details leaked

**Verification**:
```bash
curl -v http://localhost:3000/test-panic
# Status line should show: HTTP/1.1 500 Internal Server Error
# Body should be: "Internal server error"
# No backtrace or stack trace in response
```

## Performance Impact Verification

### Test 1: Request Latency (No Panic)

```bash
# Benchmark non-panicking requests
ab -n 10000 -c 100 http://localhost:3000/health

# Compare with previous latency (should be negligible)
# Expected: <1% latency increase from task spawning
```

### Test 2: Memory Usage (No Leak)

```bash
# Monitor memory during sustained load
top -p $(pgrep -f "sky-moon-scope")

# Should remain stable (no memory leak)
# Each task adds minimal overhead (~1KB)
```

### Test 3: Backtrace Capture Cost

```bash
# Backtrace only captured on panic (rare event)
# Test with intentional panic flood
for i in {1..100}; do
  curl -s http://localhost:3000/test-panic &
done
wait

# Measure time and memory (should handle gracefully)
# System should recover quickly after panic storm ends
```

## Error Path Verification

### Test: Handler Errors (Not Panics)

**Expected**: Errors should still be handled normally

```bash
# Create endpoint that returns Err
#[get("/test-error")]
pub async fn test_error() -> Result<(), ApiError> {
    Err(ApiError::BadRequest("test error".to_string()))
}

# Test endpoint
curl -v http://localhost:3000/test-error
# Should return: HTTP 400 (not 500)
# Should include error message (not generic)
```

### Test: 4xx Errors Unchanged

**Expected**: Client errors should still show original message

```bash
# 404, 400, 401, etc. should all work as before
curl -v http://localhost:3000/nonexistent  # Should return 404
curl -v http://localhost:3000/bad-input    # Should return 400
```

## Logging Output Format

### Expected Log Entry Format

**Structured log output** (JSON-formatted):
```json
{
  "timestamp": "2024-11-21T10:15:30.123456Z",
  "level": "ERROR",
  "message": "Handler panic caught: attempt to divide by zero",
  "panic_message": "attempt to divide by zero",
  "backtrace": "#0  0x0000555555555555 in panicking ...",
  "target": "sky_moon_scope_core::error_middleware",
  "module_path": "sky_moon_scope_core::error_middleware"
}
```

### Verify Log Format

```bash
# Check for required fields
grep "panic_message" /var/log/app.log  # Should exist
grep "backtrace" /var/log/app.log      # Should exist  
grep "level.*ERROR" /var/log/app.log   # Should be ERROR

# Check JSON validity
cat /var/log/app.log | jq '.panic_message' | head -1
```

## Webhook Integration Verification

### Test Webhook Payload

```bash
# Create test webhook server
cat > webhook_test.py << 'EOF'
from http.server import HTTPServer, BaseHTTPRequestHandler
import json

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers['Content-Length'])
        body = json.loads(self.rfile.read(length))
        print("Alarm received:")
        print(f"  Event: {body.get('event')}")
        print(f"  Resource: {body.get('resource')}")
        print(f"  PID: {body.get('pid')}")
        print(f"  Node: {body.get('node_id')}")
        print(f"  Timestamp: {body.get('timestamp')}")
        self.send_response(200)
        self.end_headers()

HTTPServer(('0.0.0.0', 9999), Handler).serve_forever()
EOF

python3 webhook_test.py &
export Sky Moon Scope_ALARM_WEBHOOK_URL="http://localhost:9999/alarms"

# Trigger panic
curl http://localhost:3000/test-panic

# Should see output in webhook server showing alarm received
```

### Verify Payload Structure

```bash
# Expected fields in SysAlarmEvent
{
  "event": "handler_panic",           # ✓ Must be "handler_panic"
  "resource": "request_handler",      # ✓ Must be "request_handler"
  "value_percent": 0.0,               # ✓ Always 0.0 for panic
  "threshold_percent": 0.0,           # ✓ Always 0.0 for panic
  "process_memory_bytes": 123456,     # ✓ Must be present (can be 0)
  "total_memory_bytes": 8589934592,   # ✓ Must be present
  "pid": 12345,                       # ✓ Must be process PID
  "node_id": "hostname",              # ✓ Can be null if HOSTNAME unset
  "timestamp": "2024-11-21T10:15:30Z" # ✓ ISO 8601 format
}
```

## Rollback Verification

If reverting the change:

```bash
# All tests should still pass with old behavior
cargo test

# Old panic behavior: logs from panic hook (if any)
# New panic behavior: logs from middleware + SysAlarm

# Verify rollback steps
git revert <commit>
cargo build
cargo test
```

## Deployment Verification Checklist

- [ ] Code compiles without warnings
- [ ] All tests pass (unit + integration)
- [ ] No panics in normal operation
- [ ] Test panic endpoint works
- [ ] Panic logged to structured logs
- [ ] Backtrace appears in logs
- [ ] SysAlarm event created
- [ ] Webhook receives alarm (if configured)
- [ ] HTTP 500 returned to client
- [ ] Error message is opaque
- [ ] No memory leaks on sustained load
- [ ] No unintended performance regression
- [ ] 4xx errors still work correctly
- [ ] Server continues accepting requests after panic

## Logs Queries for Verification

### Find Panic Logs
```bash
# Using common log aggregation tools
# ELK Stack / Splunk / DataDog
filter level == ERROR AND panic_message != null
```

### Find Panic Alarms
```bash
# Webhook logs or alert tracking
filter event == "handler_panic"
```

### Correlate Panic and Alert
```bash
# Find panic log and subsequent alarm within 1 second
filter (level == ERROR AND panic_message != null) 
  OR (event == "handler_panic")
| stats values(*) by timestamp
```

### Count Panics Over Time
```bash
filter panic_message != null
| stats count as panic_count by 1h
```

## Success Criteria

✓ Panics are caught (HTTP 500 returned)  
✓ Panic message is logged to structured logs  
✓ Backtrace is captured and included in logs  
✓ SysAlarm event is emitted  
✓ Webhook receives alarm (if configured)  
✓ Logs are queryable and actionable  
✓ No production impact (no crashes, no data loss)  
✓ Performance is acceptable (<1% latency increase)  
✓ Integration with existing systems works  
✓ Documentation is complete and clear  
