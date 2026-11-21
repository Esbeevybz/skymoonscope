# Panic Logging Quick Reference

## What Changed

The middleware in `core/src/error_middleware.rs` now logs panic payloads and emits SysAlarm events when handlers panic.

## Key Files

- **Implementation**: `core/src/error_middleware.rs`
  - `error_sanitization_middleware()` - Main middleware function
  - `log_panic_string()` - Panic logging with backtrace
  - `emit_panic_alarm_string()` - SysAlarm emission
  
- **Documentation**: `PANIC_LOGGING_IMPLEMENTATION.md` - Full implementation details

## How It Works

1. Handler executes in a `tokio::spawn` task
2. If handler panics, task fails with `is_panic() == true`
3. Panic message extracted and logged at ERROR level
4. SysAlarm event emitted to webhook (if configured)
5. HTTP 500 returned to client with opaque message

## Output Example

### Structured Log (ERROR level)

```
panic_message="attempt to divide by zero"
backtrace="[stack trace output]"
message="Handler panic caught: attempt to divide by zero"
```

### Webhook Payload

```json
{
  "event": "handler_panic",
  "resource": "request_handler",
  "pid": 12345,
  "node_id": "service-01",
  "timestamp": "2024-11-21T10:15:30Z"
}
```

## Testing

```bash
# Run unit tests
cargo test --lib error_middleware

# Test with real panic (create test handler)
pub async fn test_panic() {
    panic!("Test panic message");
}

# Trigger panic
curl http://localhost:3000/test-panic

# Check logs for ERROR level with panic_message field
# Check webhook logs for handler_panic event
```

## Configuration

No new config needed. Reuses existing SysAlarm configuration:

```bash
# For webhook alerting (optional)
export Sky Moon Scope_ALARM_WEBHOOK_URL="https://alerts.example.com/hook"
```

## What Stays the Same

- HTTP 500 still returned to clients
- Opaque error messages still used (no stack traces exposed)
- All other middleware behavior unchanged
- Client error handling (4xx) unchanged

## What's New

✓ Panic payload logged to structured logs  
✓ Backtrace captured and included in logs  
✓ SysAlarm event emitted for each panic  
✓ Webhook integration for operational alerting  
✓ Full context available for root-cause analysis  

## Filtering Panic Logs

In your log aggregation tool:

```
# Find all panics
level=ERROR AND panic_message=*

# Find specific panic type
level=ERROR AND panic_message="attempted to divide by zero"

# Find panics from specific node
level=ERROR AND panic_message=* AND node_id="service-01"
```

## Filtering Alarm Events

```
# Find all panic alarms
event="handler_panic"

# Find panics from specific node
event="handler_panic" AND node_id="service-01"

# Count panic frequency per node
event="handler_panic" | stats count by node_id
```

## Monitoring Dashboard

Recommended metrics to track:

1. **Panic frequency**: Count of `event="handler_panic"` per time window
2. **Panic diversity**: Unique values of `panic_message` field
3. **Panic distribution**: Group `event="handler_panic"` by `node_id`
4. **Mean time to response**: Duration from panic to alarm received

## Troubleshooting

### No panic logs appearing

- Check log level is set to ERROR or DEBUG
- Verify handler is actually panicking (not returning errors)
- Check that middleware is registered in main.rs at startup

### Webhook alarms not received

- Verify `Sky Moon Scope_ALARM_WEBHOOK_URL` is set
- Check webhook is reachable (test with curl)
- Review webhook server logs for POST requests
- Check 5-second timeout is sufficient for your webhook

### Backtrace is empty

- Set `RUST_BACKTRACE=1` environment variable at startup
- Use release build with debug symbols: `cargo build --release`
- Verify `std::backtrace::Backtrace::capture()` support in your Rust version (1.65+)

## Integration with Existing Alarms

Panic alarms use the same webhook as existing SysAlarmMonitor (CPU/memory). 

Both types of alarms:
- Use identical webhook POST format
- Can be distinguished by `event` field
- Respect the same deduplication window (configurable)
- Include `pid` and `node_id` for correlation

Example aggregation:

```
# All system alarms (resource + handler)
event in ("threshold_breach", "handler_panic")

# Just resource alarms
event in ("threshold_breach", "threshold_recovered")

# Just panic alarms
event="handler_panic"
```
